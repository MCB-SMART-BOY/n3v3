//! The `n3v3 config` commands.
//! `n3v3 config` 命令。

use crate::output;
use crate::platform::{PlatformCapabilities, warn_system_config_unavailable};
use n3v3_config::{
    activate::Activator,
    generate::{GeneratedConfig, GeneratedFile, Generator},
    generation::{Generation, GenerationManager, GenerationMetadata, PublicationError},
    module::Module,
};
use n3v3_derive::Hash;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Snapshot manifest file name under a generation directory.
/// generation 目录中的快照清单文件名。
const GENERATED_SNAPSHOT_FILE: &str = "generated-config.json";
/// Snapshot artifact directory under a generation directory.
/// generation 目录中的快照产物目录。
const GENERATED_ARTIFACTS_DIR: &str = "generated-artifacts";

/// Serializable snapshot of a generated configuration.
/// 可序列化的已生成配置快照。
#[derive(Debug, Serialize, Deserialize)]
struct GeneratedConfigSnapshot {
    files: Vec<GeneratedFileSnapshot>,
    services: Vec<String>,
    activation_script: Option<String>,
    #[serde(default)]
    activation_script_hash: Option<String>,
}

/// Serializable snapshot entry for a generated file.
/// 生成文件的可序列化快照条目。
#[derive(Debug, Serialize, Deserialize)]
struct GeneratedFileSnapshot {
    source: String,
    target: String,
    mode: u32,
    #[serde(default)]
    hash: Option<String>,
}

/// Get the default configuration file path.
/// 获取默认配置文件路径。
fn default_config_path() -> PathBuf {
    // Look for configuration in standard locations
    // 在标准位置查找配置
    let candidates = [
        PathBuf::from("./configuration.n3v3"),
        PathBuf::from("/etc/n3v3/configuration.n3v3"),
    ];

    for path in &candidates {
        if path.exists() {
            return path.clone();
        }
    }

    // Also check user config
    // 也检查用户配置
    if let Some(path) = dirs_config_path()
        && path.exists()
    {
        return path;
    }

    PathBuf::from("./configuration.n3v3")
}

/// Get the user's config directory path.
/// 获取用户的配置目录路径。
fn dirs_config_path() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".config/n3v3/configuration.n3v3"))
}

/// Get the generations directory.
/// 获取代目录。
fn generations_dir() -> PathBuf {
    std::env::var("N3V3_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/var/lib/n3v3"))
}

/// Hold uniquely created staging until the generation snapshot is complete.
struct BuildStaging {
    path: PathBuf,
    should_clean: bool,
}

impl Drop for BuildStaging {
    fn drop(&mut self) {
        if self.should_clean {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn build_staging() -> Result<BuildStaging, String> {
    if let Some(explicit) = std::env::var_os("N3V3_BUILD_DIR") {
        let path = PathBuf::from(explicit);
        let path = check_private_staging(&path)?;
        return Ok(BuildStaging {
            path,
            should_clean: false,
        });
    }
    create_default_staging(&std::env::temp_dir())
}

fn create_default_staging(temp_root: &Path) -> Result<BuildStaging, String> {
    let parent = check_staging_parent(temp_root)?;
    use std::ffi::{CString, OsString};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let template = parent.join("n3v3-build-XXXXXX");
    let mut bytes = CString::new(template.as_os_str().as_bytes())
        .map_err(|error| {
            format!(
                "Invalid build staging path '{}': {}",
                template.display(),
                error
            )
        })?
        .into_bytes_with_nul();
    if unsafe { libc::mkdtemp(bytes.as_mut_ptr().cast()) }.is_null() {
        return Err(format!(
            "Failed to create private staging directory: {}",
            std::io::Error::last_os_error()
        ));
    }
    let path = PathBuf::from(OsString::from_vec(bytes[..bytes.len() - 1].to_vec()));
    Ok(BuildStaging {
        path,
        should_clean: true,
    })
}

fn check_staging_parent(path: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::MetadataExt;
    let canonical = fs::canonicalize(path).map_err(|error| {
        format!(
            "Failed to resolve staging directory '{}': {}",
            path.display(),
            error
        )
    })?;
    let mut current = PathBuf::new();
    for component in canonical.components() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(|error| {
            format!(
                "Failed to inspect staging parent '{}': {}",
                current.display(),
                error
            )
        })?;
        let is_sticky_root_parent = metadata.uid() == 0 && metadata.mode() & 0o1000 != 0;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || (metadata.uid() != unsafe { libc::geteuid() } && metadata.uid() != 0)
            || (metadata.mode() & 0o022 != 0 && !is_sticky_root_parent)
        {
            return Err(format!(
                "Unsafe staging parent '{}': must be owned and not group/world writable",
                current.display()
            ));
        }
    }
    Ok(canonical)
}

fn check_private_staging(path: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                format!(
                    "Failed to resolve N3V3_BUILD_DIR '{}': {}",
                    path.display(),
                    error
                )
            })?
            .join(path)
    };
    let mut current = PathBuf::new();
    for component in absolute.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(format!(
                "Unsafe N3V3_BUILD_DIR '{}': parent traversal",
                path.display()
            ));
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "Unsafe N3V3_BUILD_DIR '{}': contains symlinks",
                    path.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(format!(
                    "Failed to inspect N3V3_BUILD_DIR '{}': {}",
                    current.display(),
                    error
                ));
            }
        }
    }
    if !absolute.exists() {
        let parent = absolute
            .parent()
            .ok_or_else(|| format!("N3V3_BUILD_DIR '{}' has no parent", path.display()))?;
        let trusted_parent = check_staging_parent(parent)?;
        let name = absolute
            .file_name()
            .ok_or_else(|| format!("N3V3_BUILD_DIR '{}' has no directory name", path.display()))?;
        let new_path = trusted_parent.join(name);
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&new_path)
            .map_err(|error| {
                format!(
                    "Failed to create N3V3_BUILD_DIR '{}': {}",
                    new_path.display(),
                    error
                )
            })?;
        return Ok(new_path);
    }
    let metadata = fs::symlink_metadata(&absolute)
        .map_err(|error| format!("Unsafe N3V3_BUILD_DIR '{}': {}", path.display(), error))?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(format!(
            "Unsafe N3V3_BUILD_DIR '{}': must be an owned private directory without symlinks",
            path.display()
        ));
    }
    check_staging_parent(&absolute)
}

/// Get activation root for `n3v3 config switch`.
/// 获取 `n3v3 config switch` 的激活根目录。
fn activation_root() -> PathBuf {
    std::env::var("N3V3_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

/// Whether to run activation in dry-run mode.
/// 是否以 dry-run 模式运行激活。
fn activation_dry_run() -> bool {
    std::env::var("N3V3_CONFIG_DRY_RUN")
        .ok()
        .map(|v| parse_env_bool(v.as_str()))
        .unwrap_or(false)
}

/// Parse common truthy/falsy environment boolean values.
/// 解析常见环境变量布尔值。
fn parse_env_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Build system configuration.
/// 构建系统配置。
pub fn build() -> Result<(), String> {
    // Check platform support
    // 检查平台支持
    let caps = PlatformCapabilities::detect();
    if !caps.system_config {
        warn_system_config_unavailable();
        return Err("System configuration is only supported on Linux.".to_string());
    }

    let config_path = default_config_path();

    output::info(&format!(
        "Building system configuration from {}...",
        config_path.display()
    ));

    // Load the configuration module
    // 加载配置模块
    let mut system_config = if config_path.exists() {
        Module::load_merged(&config_path)
            .map_err(|e| format!("Failed to load configuration graph: {}", e))?
    } else {
        output::warning("No configuration file found, using default configuration.");
        Module::new("default")
            .to_system_config()
            .map_err(|e| format!("Failed to build default configuration: {}", e))?
    };

    let gen_manager = GenerationManager::new(generations_dir())
        .map_err(|e| format!("Failed to initialize generation manager: {}", e))?;
    let expected_generation = gen_manager
        .next_generation()
        .map_err(|e| format!("Failed to determine next generation number: {}", e))?;
    system_config.generation = expected_generation;

    // Generate configuration files
    // 生成配置文件
    let staging = build_staging()?;
    let generator = Generator::new(staging.path.clone());
    let generated = generator
        .generate(&system_config)
        .map_err(|e| format!("Failed to generate configuration: {}", e))?;

    output::info(&format!(
        "Generated {} configuration files.",
        generated.files.len()
    ));

    let drv = generator.to_derivation(&system_config);
    let metadata = GenerationMetadata::new()
        .name(&system_config.name)
        .description("Built by n3v3 config build");
    let generation = prepare_and_publish_generation(
        &gen_manager,
        expected_generation,
        &drv.drv_path(),
        metadata,
        &generated,
    )?;

    system_config.generation = generation.number;
    output::success(&format!("Created generation {}.", generation.number));
    output::success("Configuration built successfully.");
    println!();
    output::info("To activate this configuration, run:");
    println!("  n3v3 config switch");

    Ok(())
}

/// Finish a generation snapshot before publishing its current pointer.
/// 在发布 current 指针前完成 generation 快照。
fn prepare_and_publish_generation(
    manager: &GenerationManager,
    expected_number: u64,
    store_path: &n3v3_derive::StorePath,
    metadata: GenerationMetadata,
    generated: &GeneratedConfig,
) -> Result<Generation, String> {
    let generation = manager
        .create_generation_unpublished(store_path, metadata)
        .map_err(|error| format!("Failed to create unpublished generation: {}", error))?;
    if generation.number != expected_number {
        return Err(clean_incomplete_generation(
            manager,
            &generation,
            format!(
                "Generation allocation changed from {} to {}; retry the build",
                expected_number, generation.number
            ),
        ));
    }

    if let Err(error) = save_generated_snapshot(generated, &generation.path)
        .and_then(|_| verify_generation_snapshot(&generation))
    {
        return Err(clean_incomplete_generation(manager, &generation, error));
    }
    if let Err(error) = manager.publish_generation(&generation) {
        let cause = format!(
            "Failed to publish generation {}: {}",
            generation.number, error
        );
        return match error {
            PublicationError::NotCommitted(_) => {
                Err(clean_incomplete_generation(manager, &generation, cause))
            }
            PublicationError::Committed {
                restoration: Some(_),
                ..
            } => Err(format!(
                "{}; retained generation {} because pointer restoration failed",
                cause, generation.number
            )),
            PublicationError::Committed {
                restoration: None, ..
            } => match manager.current_generation() {
                Ok(Some(current)) if current == generation.number => Err(format!(
                    "{}; retained referenced generation {}",
                    cause, generation.number
                )),
                Ok(_) => Err(clean_incomplete_generation(manager, &generation, cause)),
                Err(check_error) => Err(format!(
                    "{}; retained generation because current pointer could not be checked: {}",
                    cause, check_error
                )),
            },
        };
    }
    manager
        .mark_latest_built_generation(generation.number)
        .map_err(|error| {
            format!(
                "Generation {} was published but failed to record latest build: {}",
                generation.number, error
            )
        })?;
    Ok(generation)
}

/// Remove only the unpublished directory allocated by this build attempt.
/// 仅删除本次构建分配的未发布目录。
fn clean_incomplete_generation(
    manager: &GenerationManager,
    generation: &Generation,
    cause: String,
) -> String {
    match manager.current_generation() {
        Ok(Some(number)) if number == generation.number => {
            return format!(
                "{}; retained generation {} because current points to it",
                cause, generation.number
            );
        }
        Err(error) => {
            return format!(
                "{}; retained generation {} because current could not be checked: {}",
                cause, generation.number, error
            );
        }
        _ => {}
    }
    match fs::remove_dir_all(&generation.path) {
        Ok(()) => cause,
        Err(error) => format!(
            "{}; failed to clean incomplete generation {} at '{}': {}",
            cause,
            generation.number,
            generation.path.display(),
            error
        ),
    }
}

/// Convert an absolute path into a path string relative to `base`.
/// 将绝对路径转换为相对于 `base` 的路径字符串。
fn rel_path_string(base: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(base)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|_| {
            format!(
                "Failed to store generated artifact outside generation dir: {}",
                path.display()
            )
        })
}

/// Normalize an artifact filename for snapshot storage.
/// 规范化快照存储用的产物文件名。
fn normalize_artifact_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Hash file contents to a hex string.
/// 将文件内容哈希为十六进制字符串。
fn hash_file_hex(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|e| format!("Failed to read '{}': {}", path.display(), e))?;
    Ok(Hash::of(&bytes).to_hex())
}

/// Resolve a snapshot artifact path and reject path traversal.
/// 解析快照产物路径并拒绝路径穿越。
fn resolve_snapshot_artifact_path(generation_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    use std::ffi::OsStr;
    use std::path::Component;

    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(format!(
            "Invalid snapshot artifact path '{}': absolute paths are not allowed",
            rel
        ));
    }

    let mut components = rel_path.components();
    let first = components.next().ok_or_else(|| {
        format!(
            "Invalid snapshot artifact path '{}': empty path is not allowed",
            rel
        )
    })?;

    match first {
        Component::Normal(seg) if seg == OsStr::new(GENERATED_ARTIFACTS_DIR) => {}
        _ => {
            return Err(format!(
                "Invalid snapshot artifact path '{}': must be under '{}'",
                rel, GENERATED_ARTIFACTS_DIR
            ));
        }
    }

    for comp in components {
        match comp {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "Invalid snapshot artifact path '{}': path traversal is not allowed",
                    rel
                ));
            }
        }
    }

    Ok(generation_dir.join(rel_path))
}

/// Save generated files into generation-local snapshot artifacts.
/// 将生成文件保存为 generation 本地快照产物。
fn save_generated_snapshot(
    generated: &GeneratedConfig,
    generation_dir: &Path,
) -> Result<(), String> {
    if generated.activation_script.is_some() {
        return Err(
            "Activation scripts are not supported in durable configuration snapshots".to_string(),
        );
    }

    let artifacts_dir = generation_dir.join(GENERATED_ARTIFACTS_DIR);
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&artifacts_dir)
        .map_err(|error| {
            format!(
                "Failed to create generation artifact dir '{}': {}",
                artifacts_dir.display(),
                error
            )
        })?;
    let mut snapshot = GeneratedConfigSnapshot {
        files: Vec::with_capacity(generated.files.len()),
        services: generated.services.clone(),
        activation_script: None,
        activation_script_hash: None,
    };

    for (index, file) in generated.files.iter().enumerate() {
        snapshot.files.push(copy_snapshot_file(
            file,
            index,
            generation_dir,
            &artifacts_dir,
        )?);
    }
    sync_directory(&artifacts_dir)?;
    write_snapshot_manifest(generation_dir, &snapshot)?;
    sync_directory(generation_dir)?;
    Ok(())
}

/// Copy one generated file without overwriting an existing artifact.
/// 复制一个生成文件，且不覆盖已有产物。
fn copy_snapshot_file(
    file: &GeneratedFile,
    index: usize,
    generation_dir: &Path,
    artifacts_dir: &Path,
) -> Result<GeneratedFileSnapshot, String> {
    let name = file
        .source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let artifact_name = format!("{:04}-{}", index, normalize_artifact_name(name));
    let artifact_path = artifacts_dir.join(artifact_name);
    let mut source = File::open(&file.source)
        .map_err(|error| format!("Failed to read '{}': {}", file.source.display(), error))?;
    use std::os::unix::fs::OpenOptionsExt;
    let mut artifact = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&artifact_path)
        .map_err(|error| format!("Failed to create '{}': {}", artifact_path.display(), error))?;
    std::io::copy(&mut source, &mut artifact)
        .map_err(|error| format!("Failed to copy '{}': {}", file.source.display(), error))?;
    artifact
        .sync_all()
        .map_err(|error| format!("Failed to sync '{}': {}", artifact_path.display(), error))?;

    Ok(GeneratedFileSnapshot {
        source: rel_path_string(generation_dir, &artifact_path)?,
        target: file.target.to_string_lossy().to_string(),
        mode: file.mode,
        hash: Some(hash_file_hex(&artifact_path)?),
    })
}

/// Durably install the snapshot manifest after all artifacts are durable.
/// 在所有产物持久化后原子写入快照清单。
fn write_snapshot_manifest(
    generation_dir: &Path,
    snapshot: &GeneratedConfigSnapshot,
) -> Result<(), String> {
    let snapshot_path = generation_dir.join(GENERATED_SNAPSHOT_FILE);
    if snapshot_path.exists() {
        return Err(format!(
            "Generation snapshot already exists: {}",
            snapshot_path.display()
        ));
    }
    let temp_path = generation_dir.join(".generated-config.json.tmp");
    let content = serde_json::to_vec_pretty(snapshot)
        .map_err(|error| format!("Failed to serialize generation snapshot: {}", error))?;
    use std::os::unix::fs::OpenOptionsExt;
    let mut temp = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp_path)
        .map_err(|error| {
            format!(
                "Failed to create snapshot manifest '{}': {}",
                temp_path.display(),
                error
            )
        })?;
    temp.write_all(&content)
        .and_then(|_| temp.sync_all())
        .map_err(|error| format!("Failed to persist snapshot manifest: {}", error))?;
    fs::rename(&temp_path, &snapshot_path)
        .map_err(|error| format!("Failed to publish snapshot manifest: {}", error))
}

/// Sync a directory on platforms that support directory fsync.
/// 在支持目录 fsync 的平台持久化目录项。
fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Failed to sync directory '{}': {}", path.display(), error))?;
    Ok(())
}

/// Load generated snapshot artifacts from a generation directory.
/// 从 generation 目录加载生成快照产物。
fn load_generated_snapshot(generation_dir: &Path) -> Result<Option<GeneratedConfig>, String> {
    let snapshot_path = generation_dir.join(GENERATED_SNAPSHOT_FILE);
    if !snapshot_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&snapshot_path)
        .map_err(|e| format!("Failed to read generation snapshot: {}", e))?;
    let snapshot: GeneratedConfigSnapshot = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid generation snapshot JSON: {}", e))?;
    if snapshot.activation_script.is_some() || snapshot.activation_script_hash.is_some() {
        return Err(
            "Legacy activation-script snapshots are unsafe and cannot be activated; rebuild the configuration"
                .to_string(),
        );
    }

    let mut generated = GeneratedConfig::new();
    generated.services = snapshot.services;

    for file in snapshot.files {
        let source = resolve_snapshot_artifact_path(generation_dir, &file.source)?;
        if !source.exists() {
            return Err(format!(
                "Missing generated artifact '{}' referenced by snapshot",
                source.display()
            ));
        }
        if let Some(expected_hash) = &file.hash {
            let actual_hash = hash_file_hex(&source)?;
            if &actual_hash != expected_hash {
                return Err(format!(
                    "Hash mismatch for generated artifact '{}': expected {}, got {}",
                    source.display(),
                    expected_hash,
                    actual_hash
                ));
            }
        }
        generated.files.push(GeneratedFile {
            source,
            target: PathBuf::from(file.target),
            mode: file.mode,
        });
    }

    Ok(Some(generated))
}

/// Activate a specific generation.
/// 激活指定 generation。
fn activate_generation(generation: &Generation, is_dry_run: bool) -> Result<(), String> {
    output::info(&format!(
        "Activating generation {} ({})...",
        generation.number,
        generation.store_path.display_name()
    ));

    let generated = load_generated_snapshot(&generation.path)?.ok_or_else(|| {
        format!(
            "Generation {} is missing '{}' snapshot. Rebuild the configuration to restore immutable activation artifacts.",
            generation.number, GENERATED_SNAPSHOT_FILE
        )
    })?;

    let root = activation_root();
    if is_dry_run {
        output::warning("Dry-run activation enabled via N3V3_CONFIG_DRY_RUN.");
    }

    let activator = Activator::new()
        .root(&root)
        .dry_run(is_dry_run)
        .verbose(true);
    let result = activator
        .activate(&generated)
        .map_err(|e| format!("Failed to activate generation {}: {}", generation.number, e))?;

    if is_dry_run {
        output::info(&format!("Previewed generation {}.", generation.number));
        output::info(&format!(
            "Would install {} file(s), enable {} service(s).",
            result.files_installed, result.services_enabled
        ));
        return Ok(());
    }
    output::success(&format!("Activated generation {}.", generation.number));
    output::info(&format!(
        "Installed {} file(s), enabled {} service(s).",
        result.files_installed, result.services_enabled
    ));
    if let Some(script_output) = result.script_output
        && !script_output.trim().is_empty()
    {
        output::info("Activation script output:");
        println!("{}", script_output.trim_end());
    }

    Ok(())
}

/// Verify that a generation has a valid activation snapshot.
/// 校验 generation 的激活快照是否有效。
fn verify_generation_snapshot(generation: &Generation) -> Result<(), String> {
    match load_generated_snapshot(&generation.path)? {
        Some(_) => Ok(()),
        None => Err(format!(
            "generation {} is missing '{}'",
            generation.number, GENERATED_SNAPSHOT_FILE
        )),
    }
}

/// Switch generation pointer and activate; restore previous pointer on activation failure.
/// 切换 generation 指针并激活；若激活失败则恢复之前的指针。
fn switch_to_generation_with_activation<F>(
    gen_manager: &GenerationManager,
    gen_num: u64,
    mut activate: F,
) -> Result<Generation, String>
where
    F: FnMut(&Generation) -> Result<(), String>,
{
    let previous_current = gen_manager
        .current_generation()
        .map_err(|error| format!("Failed to get current generation: {}", error))?;
    let previous_active = gen_manager
        .active_generation()
        .map_err(|error| format!("Failed to get active generation: {}", error))?;
    let generation = gen_manager
        .switch_to(gen_num)
        .map_err(|error| format!("Failed to switch to generation {}: {}", gen_num, error))?;
    if let Err(error) = activate(&generation) {
        let previous = previous_active.or(previous_current);
        let restore = match previous {
            Some(number) if number != gen_num => gen_manager.switch_to(number).map(|_| ()),
            None => gen_manager.clear_current(),
            _ => Ok(()),
        };
        return match restore {
            Ok(()) => Err(format!(
                "Activation failed for generation {}: {}. Restored previous current generation pointer {:?}.",
                gen_num, error, previous
            )),
            Err(restore_error) => Err(format!(
                "Activation failed for generation {}: {}. Failed to restore current generation pointer: {}",
                gen_num, error, restore_error
            )),
        };
    }
    gen_manager
        .mark_active_generation(gen_num)
        .map_err(|error| {
            format!(
                "Generation {} activated, but failed to record active pointer: {}",
                gen_num, error
            )
        })?;
    Ok(generation)
}

fn run_selected_generation<F>(
    gen_manager: &GenerationManager,
    gen_num: u64,
    is_dry_run: bool,
    mut activate: F,
) -> Result<Generation, String>
where
    F: FnMut(&Generation) -> Result<(), String>,
{
    if !is_dry_run {
        return switch_to_generation_with_activation(gen_manager, gen_num, activate);
    }
    let generation = gen_manager
        .load_generation(gen_num)
        .map_err(|error| format!("Failed to preview generation {gen_num}: {error}"))?;
    activate(&generation)?;
    Ok(generation)
}

/// Switch to a new or specific configuration.
/// 切换到新配置或特定配置。
pub fn switch() -> Result<(), String> {
    // Check platform support
    // 检查平台支持
    let caps = PlatformCapabilities::detect();
    if !caps.system_config {
        warn_system_config_unavailable();
        return Err("System configuration is only supported on Linux.".to_string());
    }

    let gen_manager = GenerationManager::new(generations_dir())
        .map_err(|e| format!("Failed to initialize generation manager: {}", e))?;

    let current = gen_manager
        .latest_built_generation()
        .map_err(|error| format!("Failed to get latest built generation: {}", error))?
        .or(gen_manager
            .current_generation()
            .map_err(|error| format!("Failed to get current generation: {}", error))?);

    match current {
        Some(gen_num) => {
            let is_dry_run = activation_dry_run();
            run_selected_generation(&gen_manager, gen_num, is_dry_run, |generation| {
                activate_generation(generation, is_dry_run)
            })?;
            Ok(())
        }
        None => {
            Err("No configuration has been built yet. Run 'n3v3 config build' first.".to_string())
        }
    }
}

/// Verify generation activation snapshot integrity.
/// 校验 generation 激活快照完整性。
pub fn verify(generation: Option<u64>, all: bool) -> Result<(), String> {
    if all && generation.is_some() {
        return Err("Cannot specify both --all and a generation number.".to_string());
    }

    let gen_manager = GenerationManager::new(generations_dir())
        .map_err(|e| format!("Failed to initialize generation manager: {}", e))?;

    let targets: Vec<Generation> = if all {
        let gens = gen_manager
            .list_generations()
            .map_err(|e| format!("Failed to list generations: {}", e))?;
        if gens.is_empty() {
            return Err("No configuration generations found.".to_string());
        }
        gens
    } else if let Some(gen_num) = generation {
        vec![
            gen_manager
                .load_generation(gen_num)
                .map_err(|e| format!("Failed to load generation {}: {}", gen_num, e))?,
        ]
    } else {
        let current = gen_manager
            .current_generation()
            .map_err(|e| format!("Failed to get current generation: {}", e))?
            .ok_or_else(|| "No configuration has been built yet.".to_string())?;
        vec![
            gen_manager
                .load_generation(current)
                .map_err(|e| format!("Failed to load generation {}: {}", current, e))?,
        ]
    };

    output::header("Generation Snapshot Verification");

    let mut failed = 0usize;
    for generation in targets {
        match verify_generation_snapshot(&generation) {
            Ok(()) => output::success(&format!(
                "Generation {}: snapshot integrity OK",
                generation.number
            )),
            Err(err) => {
                failed += 1;
                output::error(&format!(
                    "Generation {}: snapshot verification failed: {}",
                    generation.number, err
                ));
            }
        }
    }

    if failed == 0 {
        Ok(())
    } else {
        Err(format!("{} generation(s) failed verification", failed))
    }
}

fn previous_retained_generation(
    gen_manager: &GenerationManager,
    current: u64,
) -> Result<Generation, String> {
    let previous = gen_manager
        .list_generations()
        .map_err(|error| format!("Failed to list generations for rollback: {error}"))?
        .into_iter()
        .filter(|generation| generation.number < current)
        .max_by_key(|generation| generation.number)
        .ok_or_else(|| format!("No retained generation before {current} to roll back to."))?;
    verify_generation_snapshot(&previous)?;
    Ok(previous)
}

/// Rollback to a previous configuration.
/// 回滚到上一个配置。
pub fn rollback() -> Result<(), String> {
    // Check platform support
    // 检查平台支持
    let caps = PlatformCapabilities::detect();
    if !caps.system_config {
        warn_system_config_unavailable();
        return Err("System configuration is only supported on Linux.".to_string());
    }

    let gen_manager = GenerationManager::new(generations_dir())
        .map_err(|e| format!("Failed to initialize generation manager: {}", e))?;

    let current = gen_manager
        .active_generation()
        .map_err(|error| format!("Failed to get active generation: {}", error))?
        .or(gen_manager
            .current_generation()
            .map_err(|error| format!("Failed to get current generation: {}", error))?);

    let current = current.ok_or_else(|| "No configuration has been built yet.".to_string())?;
    let previous = previous_retained_generation(&gen_manager, current)?;
    let is_dry_run = activation_dry_run();
    output::info(&format!(
        "{} generation {} from {}...",
        if is_dry_run {
            "Previewing rollback to"
        } else {
            "Rolling back to"
        },
        previous.number,
        current
    ));
    let generation =
        run_selected_generation(&gen_manager, previous.number, is_dry_run, |generation| {
            activate_generation(generation, is_dry_run)
        })?;
    if !is_dry_run {
        output::success(&format!("Rolled back to generation {}.", generation.number));
    }
    Ok(())
}

/// List all configuration generations.
/// 列出所有配置代。
pub fn list_generations() -> Result<(), String> {
    let gen_manager = GenerationManager::new(generations_dir())
        .map_err(|e| format!("Failed to initialize generation manager: {}", e))?;

    let current = gen_manager
        .current_generation()
        .map_err(|error| format!("Failed to get current generation: {}", error))?;
    let active = gen_manager
        .active_generation()
        .map_err(|error| format!("Failed to get active generation: {}", error))?;

    let generations = gen_manager
        .list_generations()
        .map_err(|e| format!("Failed to list generations: {}", e))?;

    if generations.is_empty() {
        output::info("No configuration generations found.");
        output::info("Run 'n3v3 config build' to create one.");
        return Ok(());
    }

    output::header("System Configuration Generations");

    let mut table = output::Table::new(vec!["#", "Name", "Description", "Status"]);

    for generation in generations.iter().rev() {
        let status = match (
            Some(generation.number) == current,
            Some(generation.number) == active,
        ) {
            (true, true) => "current, active",
            (true, false) => "current",
            (false, true) => "active",
            (false, false) => "",
        };
        let name = generation.metadata.name.as_deref().unwrap_or("unnamed");
        let desc = generation.metadata.description.as_deref().unwrap_or("");

        table.add_row(vec![&generation.number.to_string(), name, desc, status]);
    }

    table.print();

    Ok(())
}

/// Interactively switch to a specific generation.
/// 交互式切换到特定代。
pub fn switch_interactive() -> Result<(), String> {
    // Check platform support
    // 检查平台支持
    let caps = PlatformCapabilities::detect();
    if !caps.system_config {
        warn_system_config_unavailable();
        return Err("System configuration is only supported on Linux.".to_string());
    }

    let gen_manager = GenerationManager::new(generations_dir())
        .map_err(|e| format!("Failed to initialize generation manager: {}", e))?;

    let generations = gen_manager
        .list_generations()
        .map_err(|e| format!("Failed to list generations: {}", e))?;

    if generations.is_empty() {
        return Err("No generations available. Run 'n3v3 config build' first.".to_string());
    }

    // Show available generations
    // 显示可用代
    list_generations()?;

    println!();

    // Prompt for generation number
    // 提示输入代编号
    if let Some(input) = output::prompt("Enter generation number to switch to") {
        let gen_num: u64 = input
            .parse()
            .map_err(|_| format!("Invalid generation number: {}", input))?;

        let is_dry_run = activation_dry_run();
        let generation =
            run_selected_generation(&gen_manager, gen_num, is_dry_run, |generation| {
                activate_generation(generation, is_dry_run)
            })?;
        if !is_dry_run {
            output::success(&format!("Switched to generation {}.", generation.number));
        }
    } else {
        output::info("Switch cancelled.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use n3v3_derive::{Hash as DeriveHash, StorePath};
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_root = std::env::temp_dir().canonicalize().unwrap();
        let dir = temp_root.join(format!(
            "n3v3-config-command-test-{}-{}-{}",
            prefix,
            std::process::id(),
            nonce
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::canonicalize(dir).unwrap()
    }

    fn dummy_generation(path: PathBuf, number: u64) -> Generation {
        Generation {
            number,
            path,
            store_path: StorePath::new(DeriveHash::of(b"dummy"), format!("dummy-{}", number)),
            metadata: GenerationMetadata::new(),
        }
    }

    #[test]
    fn test_parse_env_bool() {
        assert!(parse_env_bool("1"));
        assert!(parse_env_bool("TRUE"));
        assert!(parse_env_bool(" yes "));
        assert!(!parse_env_bool("0"));
        assert!(!parse_env_bool("false"));
        assert!(!parse_env_bool("off"));
        assert!(!parse_env_bool("random"));
    }

    #[test]
    fn test_verify_rejects_all_with_generation() {
        let err = verify(Some(1), true).expect_err("verify should reject conflicting arguments");
        assert!(err.contains("Cannot specify both --all and a generation number"));
    }

    #[test]
    fn test_generated_snapshot_roundtrip() {
        let dir = temp_dir("snapshot");
        let gen_dir = dir.join("generation-1");
        fs::create_dir_all(&gen_dir).unwrap();

        let source = dir.join("source.conf");
        fs::write(&source, "content\n").unwrap();
        let mut generated = GeneratedConfig::new();
        generated.files.push(GeneratedFile {
            source,
            target: PathBuf::from("/etc/test.conf"),
            mode: 0o644,
        });
        generated.services.push("demo".to_string());

        save_generated_snapshot(&generated, &gen_dir).unwrap();
        let loaded = load_generated_snapshot(&gen_dir).unwrap().unwrap();

        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files[0].target, PathBuf::from("/etc/test.conf"));
        assert_eq!(loaded.files[0].mode, 0o644);
        assert!(loaded.files[0].source.exists());
        assert_eq!(loaded.services, vec!["demo".to_string()]);
        assert!(loaded.activation_script.is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_generated_snapshot_detects_hash_mismatch() {
        let dir = temp_dir("snapshot-hash-mismatch");
        let gen_dir = dir.join("generation-1");
        fs::create_dir_all(&gen_dir).unwrap();

        let source = dir.join("source.conf");
        fs::write(&source, "content\n").unwrap();
        let mut generated = GeneratedConfig::new();
        generated.files.push(GeneratedFile {
            source,
            target: PathBuf::from("/etc/test.conf"),
            mode: 0o644,
        });

        save_generated_snapshot(&generated, &gen_dir).unwrap();

        let snapshot_path = gen_dir.join(GENERATED_SNAPSHOT_FILE);
        let snapshot: GeneratedConfigSnapshot =
            serde_json::from_str(&fs::read_to_string(&snapshot_path).unwrap()).unwrap();
        let artifact_path = gen_dir.join(&snapshot.files[0].source);
        fs::write(&artifact_path, "tampered\n").unwrap();

        let err = load_generated_snapshot(&gen_dir).unwrap_err();
        assert!(err.contains("Hash mismatch"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_legacy_snapshot_without_hashes() {
        let dir = temp_dir("snapshot-legacy");
        let gen_dir = dir.join("generation-1");
        let artifacts = gen_dir.join(GENERATED_ARTIFACTS_DIR);
        fs::create_dir_all(&artifacts).unwrap();

        let artifact_rel = format!("{}/{}", GENERATED_ARTIFACTS_DIR, "legacy.conf");
        let artifact_path = gen_dir.join(&artifact_rel);
        fs::write(&artifact_path, "legacy\n").unwrap();

        let legacy_snapshot = json!({
            "files": [{
                "source": artifact_rel,
                "target": "/etc/legacy.conf",
                "mode": 0o644
            }],
            "services": ["legacy-service"],
            "activation_script": null
        });
        fs::write(
            gen_dir.join(GENERATED_SNAPSHOT_FILE),
            serde_json::to_string_pretty(&legacy_snapshot).unwrap(),
        )
        .unwrap();

        let loaded = load_generated_snapshot(&gen_dir).unwrap().unwrap();
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files[0].target, PathBuf::from("/etc/legacy.conf"));
        assert_eq!(loaded.services, vec!["legacy-service".to_string()]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_snapshot_rejects_parent_traversal_source_path() {
        let dir = temp_dir("snapshot-traversal-source");
        let gen_dir = dir.join("generation-1");
        fs::create_dir_all(&gen_dir).unwrap();

        let invalid_snapshot = json!({
            "files": [{
                "source": "../outside.conf",
                "target": "/etc/legacy.conf",
                "mode": 0o644
            }],
            "services": [],
            "activation_script": null
        });
        fs::write(
            gen_dir.join(GENERATED_SNAPSHOT_FILE),
            serde_json::to_string_pretty(&invalid_snapshot).unwrap(),
        )
        .unwrap();

        let err = load_generated_snapshot(&gen_dir).unwrap_err();
        assert!(err.contains("Invalid snapshot artifact path"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_snapshot_legacy_activation_script_is_rejected() {
        let dir = temp_dir("snapshot-legacy-script");
        let gen_dir = dir.join("generation-1");
        fs::create_dir_all(&gen_dir).unwrap();

        let invalid_snapshot = json!({
            "files": [],
            "services": [],
            "activation_script": "/tmp/evil.sh"
        });
        fs::write(
            gen_dir.join(GENERATED_SNAPSHOT_FILE),
            serde_json::to_string_pretty(&invalid_snapshot).unwrap(),
        )
        .unwrap();

        let err = load_generated_snapshot(&gen_dir).unwrap_err();
        assert!(err.contains("Legacy activation-script snapshots"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_snapshot_with_activation_script_rejects_before_writing() {
        let dir = temp_dir("snapshot-script-input");
        let gen_dir = dir.join("generation-1");
        fs::create_dir(&gen_dir).unwrap();
        let mut generated = GeneratedConfig::new();
        generated.activation_script = Some(dir.join("untrusted-script"));

        let error = save_generated_snapshot(&generated, &gen_dir).unwrap_err();
        assert!(error.contains("Activation scripts are not supported"));
        assert!(!gen_dir.join(GENERATED_SNAPSHOT_FILE).exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_verify_generation_snapshot_missing_manifest() {
        let dir = temp_dir("verify-missing");
        let gen_dir = dir.join("generation-1");
        fs::create_dir_all(&gen_dir).unwrap();

        let generation = dummy_generation(gen_dir.clone(), 1);
        let err = verify_generation_snapshot(&generation).unwrap_err();
        assert!(err.contains("missing"));
        assert!(err.contains(GENERATED_SNAPSHOT_FILE));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_activate_generation_requires_snapshot_manifest() {
        let dir = temp_dir("activate-missing-snapshot");
        let gen_dir = dir.join("generation-1");
        fs::create_dir_all(&gen_dir).unwrap();

        let generation = dummy_generation(gen_dir.clone(), 1);
        let err = activate_generation(&generation, false).expect_err("activation should fail");
        assert!(err.contains("missing 'generated-config.json'"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_verify_generation_snapshot_ok() {
        let dir = temp_dir("verify-ok");
        let gen_dir = dir.join("generation-1");
        fs::create_dir_all(&gen_dir).unwrap();

        let source = dir.join("source.conf");
        fs::write(&source, "content\n").unwrap();
        let mut generated = GeneratedConfig::new();
        generated.files.push(GeneratedFile {
            source,
            target: PathBuf::from("/etc/test.conf"),
            mode: 0o644,
        });
        save_generated_snapshot(&generated, &gen_dir).unwrap();

        let generation = dummy_generation(gen_dir.clone(), 1);
        verify_generation_snapshot(&generation).unwrap();

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_switch_to_generation_with_activation_restores_previous_pointer_on_failure() {
        let state_dir = temp_dir("switch-restore");
        let manager = GenerationManager::new(state_dir.clone()).expect("generation manager");

        let gen1_store = StorePath::new(DeriveHash::of(b"gen1"), "gen1".to_string());
        let gen2_store = StorePath::new(DeriveHash::of(b"gen2"), "gen2".to_string());

        manager
            .create_generation(&gen1_store, GenerationMetadata::new())
            .expect("create generation 1");
        manager
            .create_generation(&gen2_store, GenerationMetadata::new())
            .expect("create generation 2");

        assert_eq!(manager.current_generation().unwrap(), Some(2));

        let err = switch_to_generation_with_activation(&manager, 1, |_| {
            Err("simulated activation failure".to_string())
        })
        .expect_err("switch should fail");

        assert!(err.contains("Restored previous current generation pointer Some(2)"));
        assert_eq!(manager.current_generation().unwrap(), Some(2));

        let _ = fs::remove_dir_all(&state_dir);
    }

    #[test]
    fn preview_generation_keeps_current_and_active_pointers() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let manager = GenerationManager::new(root_path).unwrap();
        let store = StorePath::new(DeriveHash::of(b"config"), "config".to_string());
        let generated = GeneratedConfig::new();
        prepare_and_publish_generation(&manager, 1, &store, GenerationMetadata::new(), &generated)
            .unwrap();
        switch_to_generation_with_activation(&manager, 1, |_| Ok(())).unwrap();
        prepare_and_publish_generation(&manager, 2, &store, GenerationMetadata::new(), &generated)
            .unwrap();
        let mut called = false;
        run_selected_generation(&manager, 2, true, |_| {
            called = true;
            Ok(())
        })
        .unwrap();
        assert!(called);
        assert_eq!(manager.current_generation().unwrap(), Some(2));
        assert_eq!(manager.active_generation().unwrap(), Some(1));
        run_selected_generation(&manager, 1, true, |_| Ok(())).unwrap();
        assert_eq!(manager.current_generation().unwrap(), Some(2));
        assert_eq!(manager.active_generation().unwrap(), Some(1));
    }

    #[test]
    fn prepare_generation_failed_snapshot_preserves_prior_snapshot_and_pointer() {
        let state_dir = temp_dir("failed-build");
        let manager = GenerationManager::new(state_dir.clone()).unwrap();
        let store = StorePath::new(DeriveHash::of(b"gen1"), "gen1".to_string());
        let source = state_dir.join("source.conf");
        fs::write(&source, "original\n").unwrap();
        let mut generated = GeneratedConfig::new();
        generated.files.push(GeneratedFile {
            source: source.clone(),
            target: PathBuf::from("/etc/test.conf"),
            mode: 0o644,
        });
        let first = prepare_and_publish_generation(
            &manager,
            1,
            &store,
            GenerationMetadata::new(),
            &generated,
        )
        .unwrap();
        let first_snapshot = fs::read(first.path.join(GENERATED_SNAPSHOT_FILE)).unwrap();
        generated.files[0].source = state_dir.join("missing.conf");

        let failure = prepare_and_publish_generation(
            &manager,
            2,
            &store,
            GenerationMetadata::new(),
            &generated,
        )
        .unwrap_err();
        assert!(failure.contains("Failed to read"));
        assert_eq!(manager.current_generation().unwrap(), Some(1));
        assert_eq!(
            fs::read(first.path.join(GENERATED_SNAPSHOT_FILE)).unwrap(),
            first_snapshot
        );
        assert!(!state_dir.join("generations/generation-2").exists());

        generated.files[0].source = source;
        let second = prepare_and_publish_generation(
            &manager,
            2,
            &store,
            GenerationMetadata::new(),
            &generated,
        )
        .unwrap();
        assert_eq!(second.number, 2);
        assert_eq!(manager.current_generation().unwrap(), Some(2));
        let _ = fs::remove_dir_all(state_dir);
    }

    #[test]
    fn prepare_generation_after_rollback_does_not_overwrite_retained_snapshot() {
        let state_dir = temp_dir("build-after-rollback");
        let manager = GenerationManager::new(state_dir.clone()).unwrap();
        let store = StorePath::new(DeriveHash::of(b"config"), "config".to_string());
        let generated = GeneratedConfig::new();
        let first = prepare_and_publish_generation(
            &manager,
            1,
            &store,
            GenerationMetadata::new(),
            &generated,
        )
        .unwrap();
        let second = prepare_and_publish_generation(
            &manager,
            2,
            &store,
            GenerationMetadata::new(),
            &generated,
        )
        .unwrap();
        let second_snapshot = fs::read(second.path.join(GENERATED_SNAPSHOT_FILE)).unwrap();
        manager.switch_to(first.number).unwrap();

        let third = prepare_and_publish_generation(
            &manager,
            3,
            &store,
            GenerationMetadata::new(),
            &generated,
        )
        .unwrap();
        assert_eq!(third.number, 3);
        assert_eq!(manager.current_generation().unwrap(), Some(3));
        assert_eq!(
            fs::read(second.path.join(GENERATED_SNAPSHOT_FILE)).unwrap(),
            second_snapshot
        );
        let _ = fs::remove_dir_all(state_dir);
    }

    #[test]
    fn switch_activation_failure_without_previous_pointer_restores_none() {
        let state_dir = temp_dir("switch-no-previous");
        let manager = GenerationManager::new(state_dir.clone()).unwrap();
        let store = StorePath::new(DeriveHash::of(b"config"), "config".to_string());
        manager
            .create_generation_unpublished(&store, GenerationMetadata::new())
            .unwrap();
        let error = switch_to_generation_with_activation(&manager, 1, |_| {
            Err("simulated activation failure".to_string())
        })
        .unwrap_err();
        assert!(error.contains("Restored previous current generation pointer None"));
        assert_eq!(manager.current_generation().unwrap(), None);
        let _ = fs::remove_dir_all(state_dir);
    }
    #[test]
    fn build_staging_shared_or_symlink_directory_is_rejected() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let shared = root_path.join("shared");
        fs::create_dir(&shared).unwrap();
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(
            check_private_staging(&shared)
                .unwrap_err()
                .contains("Unsafe")
        );
        let alias = root_path.join("alias");
        symlink(&shared, &alias).unwrap();
        assert!(
            check_private_staging(&alias)
                .unwrap_err()
                .contains("symlinks")
        );
    }

    #[test]
    fn build_staging_custom_missing_directory_is_created_privately() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let custom = root_path.join("custom-build");
        let canonical = check_private_staging(&custom).unwrap();
        assert_eq!(canonical, custom);
        assert_eq!(
            fs::metadata(custom).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn build_staging_default_is_private_unique_and_removed_on_drop() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let first = create_default_staging(&root_path).unwrap();
        let second = create_default_staging(&root_path).unwrap();
        assert_ne!(first.path, second.path);
        assert_eq!(
            fs::metadata(&first.path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let path = first.path.clone();
        drop(first);
        assert!(!path.exists());
        assert!(second.path.exists());
    }

    #[test]
    fn snapshot_artifacts_are_private_at_creation() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let generation = root_path.join("generation-1");
        fs::create_dir(&generation).unwrap();
        let source = root_path.join("shadow");
        fs::write(&source, "alice:hash\n").unwrap();
        let mut generated = GeneratedConfig::new();
        generated.files.push(GeneratedFile {
            source,
            target: PathBuf::from("/etc/shadow"),
            mode: 0o640,
        });
        save_generated_snapshot(&generated, &generation).unwrap();
        let artifacts = generation.join(GENERATED_ARTIFACTS_DIR);
        let artifact = fs::read_dir(&artifacts)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        for path in [artifact, generation.join(GENERATED_SNAPSHOT_FILE)] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(
            fs::metadata(artifacts).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn rollback_with_missing_intermediate_generation_selects_retained_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let manager = GenerationManager::new(root_path.clone()).unwrap();
        let store = StorePath::new(DeriveHash::of(b"config"), "config".to_string());
        let generated = GeneratedConfig::new();
        for number in 1..=3 {
            prepare_and_publish_generation(
                &manager,
                number,
                &store,
                GenerationMetadata::new(),
                &generated,
            )
            .unwrap();
        }
        std::fs::remove_dir_all(root_path.join("generations/generation-2")).unwrap();

        assert_eq!(previous_retained_generation(&manager, 3).unwrap().number, 1);
        assert!(previous_retained_generation(&manager, 1).is_err());
    }

    #[test]
    fn build_failed_activation_restores_active_and_preserves_latest_build() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let manager = GenerationManager::new(root_path).unwrap();
        let store = StorePath::new(DeriveHash::of(b"config"), "config".to_string());
        let generated = GeneratedConfig::new();
        prepare_and_publish_generation(&manager, 1, &store, GenerationMetadata::new(), &generated)
            .unwrap();
        switch_to_generation_with_activation(&manager, 1, |_| Ok(())).unwrap();
        prepare_and_publish_generation(&manager, 2, &store, GenerationMetadata::new(), &generated)
            .unwrap();
        let latest = manager.latest_built_generation().unwrap().unwrap();
        assert_eq!(latest, 2);
        switch_to_generation_with_activation(&manager, latest, |_| Err("activation failed".into()))
            .unwrap_err();
        assert_eq!(manager.current_generation().unwrap(), Some(1));
        assert_eq!(manager.active_generation().unwrap(), Some(1));
        assert_eq!(manager.latest_built_generation().unwrap(), Some(2));
        assert!(manager.load_generation(2).is_ok());
    }

    #[test]
    fn incomplete_cleanup_does_not_remove_current_generation() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let manager = GenerationManager::new(root_path).unwrap();
        let store = StorePath::new(DeriveHash::of(b"config"), "config".to_string());
        let generated = GeneratedConfig::new();
        let generation = prepare_and_publish_generation(
            &manager,
            1,
            &store,
            GenerationMetadata::new(),
            &generated,
        )
        .unwrap();
        assert!(
            clean_incomplete_generation(&manager, &generation, "failure".into())
                .contains("retained")
        );
        assert!(generation.path.exists());
        assert_eq!(manager.current_generation().unwrap(), Some(1));
    }
}
