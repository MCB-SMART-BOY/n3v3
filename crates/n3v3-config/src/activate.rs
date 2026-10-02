//! Configuration activation.
//! 配置激活。
//!
//! Handles switching between system configurations.
//! 处理系统配置之间的切换。

use crate::ConfigError;
use crate::generate::GeneratedConfig;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_TEMP_FILE_ATTEMPTS: usize = 100;

/// Configuration activator.
/// 配置激活器。
pub struct Activator {
    /// The system root (usually /). / 系统根目录（通常是 /）。
    root: PathBuf,
    /// Whether to perform a dry run. / 是否执行试运行。
    dry_run: bool,
    /// Whether to show verbose output. / 是否显示详细输出。
    verbose: bool,
}

impl Activator {
    /// Create a new activator.
    /// 创建新的激活器。
    pub fn new() -> Self {
        Self {
            root: PathBuf::from("/"),
            dry_run: false,
            verbose: false,
        }
    }

    /// Set the system root.
    /// 设置系统根目录。
    pub fn root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = root.into();
        self
    }

    /// Enable dry run mode.
    /// 启用试运行模式。
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Enable verbose output.
    /// 启用详细输出。
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Activate a configuration.
    /// 激活配置。
    pub fn activate(&self, generated: &GeneratedConfig) -> Result<ActivationResult, ConfigError> {
        reject_activation_script(generated)?;
        let root = checked_root(&self.root)?;
        let mut result = ActivationResult::new();
        let mut tx = ActivationTransaction::new(root.clone());

        let apply_result = (|| -> Result<(), ConfigError> {
            for file in &generated.files {
                let target = resolve_target_under_root(&root, &file.target)?;
                check_parent(&root, &target, false, &mut Vec::new())?;
                if self.verbose {
                    println!(
                        "Installing {} -> {}",
                        file.source.display(),
                        target.display()
                    );
                }

                if !self.dry_run {
                    tx.install_file(&file.source, &target, file.mode)?;
                }

                result.files_installed += 1;
            }

            // Enable services
            // 启用服务
            for service in &generated.services {
                if self.verbose {
                    println!("Enabling service: {}", service);
                }

                if !self.dry_run {
                    self.enable_service(service, &mut tx)?;
                }
                result.services_enabled += 1;
            }

            Ok(())
        })();

        if let Err(err) = apply_result {
            if !self.dry_run {
                if let Err(rollback_err) = tx.rollback() {
                    return Err(ConfigError::Activation(format!(
                        "activation failed: {}; rollback failed: {}",
                        err, rollback_err
                    )));
                }
                if self.verbose {
                    println!("Activation failed; changes rolled back.");
                }
            }
            return Err(err);
        }

        result.success = true;
        Ok(result)
    }

    /// Switch to a new configuration.
    /// 切换到新配置。
    pub fn switch(
        &self,
        from: Option<&GeneratedConfig>,
        to: &GeneratedConfig,
    ) -> Result<ActivationResult, ConfigError> {
        match self.activate(to) {
            Ok(result) => Ok(result),
            Err(err) => {
                let Some(prev) = from else {
                    return Err(err);
                };
                if self.dry_run {
                    return Err(err);
                }
                if self.verbose {
                    println!("Switch failed, rolling back to previous configuration...");
                }
                match self.activate(prev) {
                    Ok(_) => Err(ConfigError::Activation(format!(
                        "switch failed and rolled back to previous configuration: {}",
                        err
                    ))),
                    Err(rollback_err) => Err(ConfigError::Activation(format!(
                        "switch failed: {}; rollback to previous configuration failed: {}",
                        err, rollback_err
                    ))),
                }
            }
        }
    }

    /// Test a configuration without activating.
    /// 测试配置但不激活。
    pub fn test(&self, generated: &GeneratedConfig) -> Result<TestResult, ConfigError> {
        let root = checked_root(&self.root)?;
        let mut result = TestResult::new();

        for file in &generated.files {
            let target = resolve_target_under_root(&root, &file.target)?;
            check_parent(&root, &target, false, &mut Vec::new())?;

            // Check if target directory exists or can be created
            // 检查目标目录是否存在或可以创建
            if let Some(parent) = target.parent()
                && !parent.exists()
            {
                result
                    .warnings
                    .push(format!("Directory will be created: {}", parent.display()));
            }

            // Check if target exists and would be overwritten
            // 检查目标是否存在并将被覆盖
            if target.exists() {
                result
                    .warnings
                    .push(format!("File will be overwritten: {}", target.display()));
            }

            result.files_checked += 1;
        }

        // Arbitrary shell is not confined by N3V3_ROOT; testing must not claim it is safe.
        if let Some(script) = &generated.activation_script {
            result.errors.push(format!(
                "activation script {} is unsupported: N3V3_ROOT cannot confine shell execution",
                script.display()
            ));
        }
        result.success = result.errors.is_empty();
        Ok(result)
    }
}

impl Activator {
    /// Enable a systemd service by creating a wants symlink.
    /// 通过创建 wants 符号链接启用 systemd 服务。
    fn enable_service(
        &self,
        service: &str,
        tx: &mut ActivationTransaction,
    ) -> Result<(), ConfigError> {
        if !is_valid_service_name(service) {
            return Err(ConfigError::Activation(format!(
                "invalid service name: {}",
                service
            )));
        }

        let unit_rel = PathBuf::from(format!("/etc/systemd/system/{}.service", service));
        let wants_rel = PathBuf::from(format!(
            "/etc/systemd/system/multi-user.target.wants/{}.service",
            service
        ));
        let unit_path = resolve_target_under_root(&tx.root, &unit_rel)?;
        let wants_path = resolve_target_under_root(&tx.root, &wants_rel)?;
        check_parent(&tx.root, &unit_path, false, &mut Vec::new())?;
        if !fs::metadata(&unit_path).is_ok_and(|metadata| metadata.is_file()) {
            return Err(ConfigError::Activation(format!(
                "service unit file does not exist for '{}': {}",
                service,
                unit_path.display()
            )));
        }

        // Use a relative target inside multi-user.target.wants for portability
        // across chroots and alternate roots.
        // 在 multi-user.target.wants 中使用相对目标，便于 chroot 与自定义 root。
        let link_target = PathBuf::from(format!("../{}.service", service));
        tx.install_symlink(&wants_path, &link_target)
    }
}

/// Backup entry for a touched path during activation.
/// 激活过程中被修改路径的备份条目。
#[derive(Debug)]
enum PathBackup {
    File {
        path: PathBuf,
        bytes: Vec<u8>,
        mode: Option<u32>,
        owner: Option<(u32, u32)>,
    },
    Symlink {
        path: PathBuf,
        target: PathBuf,
    },
}

/// File-system transaction used by activation for rollback.
/// 激活使用的文件系统事务（用于回滚）。
#[derive(Debug)]
struct ActivationTransaction {
    root: PathBuf,
    created_paths: Vec<PathBuf>,
    created_set: HashSet<PathBuf>,
    created_dirs: Vec<PathBuf>,
    backups: Vec<PathBackup>,
    backup_set: HashSet<PathBuf>,
}

impl ActivationTransaction {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            created_paths: Vec::new(),
            created_set: HashSet::new(),
            created_dirs: Vec::new(),
            backups: Vec::new(),
            backup_set: HashSet::new(),
        }
    }

    fn install_file(&mut self, source: &Path, target: &Path, mode: u32) -> Result<(), ConfigError> {
        let mut source_file = File::open(source)?;
        self.prepare_target(target)?;
        let owner = existing_file_ownership(target)?;
        let (mut temp_file, temp_path) = create_temp_file(target)?;
        let install_result = (|| -> Result<(), ConfigError> {
            io::copy(&mut source_file, &mut temp_file)?;
            apply_file_metadata(&temp_file, target, owner, Some(mode))?;
            temp_file.sync_all()?;
            replace_path(&temp_path, target)
        })();
        if install_result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        install_result
    }

    fn install_symlink(&mut self, path: &Path, target: &Path) -> Result<(), ConfigError> {
        self.prepare_target(path)?;
        let temp_path = unique_temp_path(path)?;
        create_symlink(target, &temp_path)?;
        if let Err(err) = fs::rename(&temp_path, path) {
            let _ = fs::remove_file(&temp_path);
            return Err(ConfigError::Io(err));
        }
        Ok(())
    }

    fn prepare_target(&mut self, path: &Path) -> Result<(), ConfigError> {
        let mut new_dirs = Vec::new();
        check_parent(&self.root, path, true, &mut new_dirs)?;
        self.created_dirs.extend(new_dirs);
        self.capture_before_write(path)
    }

    fn capture_before_write(&mut self, path: &Path) -> Result<(), ConfigError> {
        if self.created_set.contains(path) || self.backup_set.contains(path) {
            return Ok(());
        }
        match fs::symlink_metadata(path) {
            Ok(metadata) => self.capture_existing(path, &metadata)?,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                self.created_set.insert(path.to_path_buf());
                self.created_paths.push(path.to_path_buf());
            }
            Err(err) => return Err(ConfigError::Io(err)),
        }
        Ok(())
    }

    fn capture_existing(
        &mut self,
        path: &Path,
        metadata: &fs::Metadata,
    ) -> Result<(), ConfigError> {
        let backup = if metadata.file_type().is_symlink() {
            PathBackup::Symlink {
                path: path.to_path_buf(),
                target: fs::read_link(path)?,
            }
        } else if metadata.is_file() {
            PathBackup::File {
                path: path.to_path_buf(),
                bytes: fs::read(path)?,
                mode: file_mode(metadata),
                owner: file_ownership(metadata),
            }
        } else {
            return Err(ConfigError::Activation(format!(
                "unsupported target path type: {}",
                path.display()
            )));
        };
        self.backups.push(backup);
        self.backup_set.insert(path.to_path_buf());
        Ok(())
    }

    fn rollback(&mut self) -> Result<(), ConfigError> {
        let mut first_error = None;
        for path in self.created_paths.iter().rev() {
            record_error(&mut first_error, remove_created_path(path));
        }
        for backup in self.backups.iter().rev() {
            record_error(&mut first_error, restore_backup(backup));
        }
        for path in self.created_dirs.iter().rev() {
            record_error(&mut first_error, remove_created_dir(path));
        }
        first_error.map_or(Ok(()), Err)
    }
}

fn reject_activation_script(generated: &GeneratedConfig) -> Result<(), ConfigError> {
    if let Some(script) = &generated.activation_script {
        return Err(ConfigError::Activation(format!(
            "activation script {} is unsupported: N3V3_ROOT cannot confine arbitrary shell execution",
            script.display()
        )));
    }
    Ok(())
}

fn checked_root(root: &Path) -> Result<PathBuf, ConfigError> {
    let canonical = fs::canonicalize(root).map_err(|err| {
        ConfigError::Activation(format!(
            "failed to resolve activation root {}: {}",
            root.display(),
            err
        ))
    })?;
    if !canonical.is_dir() {
        return Err(ConfigError::Activation(format!(
            "activation root is not a directory: {}",
            root.display()
        )));
    }
    let mut ancestor = PathBuf::new();
    for component in canonical.components() {
        ancestor.push(component);
        check_trusted_directory(&ancestor, ancestor != canonical)?;
    }
    Ok(canonical)
}

#[cfg(unix)]
fn check_trusted_directory(path: &Path, is_ancestor: bool) -> Result<(), ConfigError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path)?;
    let uid = unsafe { libc::geteuid() };
    let is_sticky_parent = is_ancestor
        && metadata.uid() == 0
        && metadata.mode() & 0o1000 != 0
        && metadata.mode() & 0o020 != 0;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || (metadata.uid() != uid && metadata.uid() != 0)
        || (metadata.mode() & 0o022 != 0 && !is_sticky_parent)
    {
        return Err(ConfigError::Activation(format!(
            "untrusted writable activation directory: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_trusted_directory(_path: &Path, _is_ancestor: bool) -> Result<(), ConfigError> {
    Ok(())
}

fn check_parent(
    root: &Path,
    target: &Path,
    should_create: bool,
    created_dirs: &mut Vec<PathBuf>,
) -> Result<(), ConfigError> {
    let parent = target.parent().ok_or_else(|| {
        ConfigError::Activation(format!("target has no parent: {}", target.display()))
    })?;
    let relative = parent.strip_prefix(root).map_err(|_| {
        ConfigError::Activation(format!(
            "target escapes activation root: {}",
            target.display()
        ))
    })?;
    let mut current = root.to_path_buf();
    check_trusted_directory(root, false)?;
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ConfigError::Activation(format!(
                    "target parent is a symlink: {}",
                    current.display()
                )));
            }
            Ok(metadata) if metadata.is_dir() => check_trusted_directory(&current, false)?,
            Ok(_) => {
                return Err(ConfigError::Activation(format!(
                    "target parent is not a directory: {}",
                    current.display()
                )));
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound && should_create => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    fs::DirBuilder::new().mode(0o755).create(&current)?;
                }
                #[cfg(not(unix))]
                fs::create_dir(&current)?;
                created_dirs.push(current.clone());
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(ConfigError::Io(err)),
        }
    }
    Ok(())
}

fn create_temp_file(target: &Path) -> Result<(File, PathBuf), ConfigError> {
    for _ in 0..MAX_TEMP_FILE_ATTEMPTS {
        let path = unique_temp_path(target)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(ConfigError::Io(err)),
        }
    }
    Err(ConfigError::Activation(format!(
        "failed to create temporary file beside {}",
        target.display()
    )))
}

fn unique_temp_path(target: &Path) -> Result<PathBuf, ConfigError> {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let parent = target.parent().ok_or_else(|| {
        ConfigError::Activation(format!("target has no parent: {}", target.display()))
    })?;
    let name = target.file_name().ok_or_else(|| {
        ConfigError::Activation(format!("target has no file name: {}", target.display()))
    })?;
    let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    Ok(parent.join(format!(
        ".{}.n3v3-{}-{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        nonce
    )))
}

fn replace_path(temp_path: &Path, target: &Path) -> Result<(), ConfigError> {
    if let Err(err) = fs::rename(temp_path, target) {
        let _ = fs::remove_file(temp_path);
        return Err(ConfigError::Io(err));
    }
    Ok(())
}

fn restore_backup(backup: &PathBackup) -> Result<(), ConfigError> {
    match backup {
        PathBackup::File {
            path,
            bytes,
            mode,
            owner,
        } => restore_file(path, bytes, *mode, *owner),
        PathBackup::Symlink { path, target } => restore_symlink(path, target),
    }
}

fn restore_file(
    path: &Path,
    bytes: &[u8],
    mode: Option<u32>,
    owner: Option<(u32, u32)>,
) -> Result<(), ConfigError> {
    let (mut temp_file, temp_path) = create_temp_file(path)?;
    let restore_result = (|| -> Result<(), ConfigError> {
        temp_file.write_all(bytes)?;
        apply_file_metadata(&temp_file, path, owner, mode)?;
        temp_file.sync_all()?;
        replace_path(&temp_path, path)
    })();
    if restore_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    restore_result
}

fn restore_symlink(path: &Path, target: &Path) -> Result<(), ConfigError> {
    let temp_path = unique_temp_path(path)?;
    create_symlink(target, &temp_path)?;
    replace_path(&temp_path, path)
}

fn remove_created_path(path: &Path) -> Result<(), ConfigError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Err(ConfigError::Activation(format!(
            "refusing to remove unexpected directory during rollback: {}",
            path.display()
        ))),
        Ok(_) => fs::remove_file(path).map_err(ConfigError::Io),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(ConfigError::Io(err)),
    }
}

fn remove_created_dir(path: &Path) -> Result<(), ConfigError> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(ConfigError::Io(err)),
    }
}

fn record_error(first_error: &mut Option<ConfigError>, result: Result<(), ConfigError>) {
    if first_error.is_none()
        && let Err(err) = result
    {
        *first_error = Some(err);
    }
}

fn apply_file_metadata(
    file: &File,
    target: &Path,
    owner: Option<(u32, u32)>,
    mode: Option<u32>,
) -> Result<(), ConfigError> {
    if let Some((uid, gid)) = owner {
        set_file_ownership(file, uid, gid).map_err(|err| {
            ConfigError::Activation(format!(
                "failed to preserve ownership of {} (uid {uid}, gid {gid}): {err}",
                target.display()
            ))
        })?;
    }
    if let Some(mode) = mode {
        set_file_mode(file, mode).map_err(|err| {
            ConfigError::Activation(format!(
                "failed to set mode of {} to {mode:o}: {err}",
                target.display()
            ))
        })?;
    }
    Ok(())
}

#[cfg(unix)]
fn existing_file_ownership(target: &Path) -> Result<Option<(u32, u32)>, ConfigError> {
    match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.is_file() => Ok(file_ownership(&metadata)),
        Ok(_) => Ok(None),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(ConfigError::Activation(format!(
            "failed to inspect ownership of {}: {err}",
            target.display()
        ))),
    }
}

#[cfg(not(unix))]
fn existing_file_ownership(_target: &Path) -> Result<Option<(u32, u32)>, ConfigError> {
    Ok(None)
}

#[cfg(unix)]
fn file_ownership(metadata: &fs::Metadata) -> Option<(u32, u32)> {
    use std::os::unix::fs::MetadataExt;
    Some((metadata.uid(), metadata.gid()))
}

#[cfg(not(unix))]
fn file_ownership(_metadata: &fs::Metadata) -> Option<(u32, u32)> {
    None
}

#[cfg(unix)]
fn set_file_ownership(file: &File, uid: u32, gid: u32) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: file remains open while fchown uses its borrowed descriptor.
    if unsafe { libc::fchown(file.as_raw_fd(), uid as libc::uid_t, gid as libc::gid_t) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_file_ownership(_file: &File, _uid: u32, _gid: u32) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(metadata.permissions().mode())
}

#[cfg(not(unix))]
fn file_mode(_metadata: &fs::Metadata) -> Option<u32> {
    None
}

#[cfg(unix)]
fn set_file_mode(file: &File, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_file_mode(_file: &File, _mode: u32) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn create_symlink(target: &Path, path: &Path) -> Result<(), ConfigError> {
    std::os::unix::fs::symlink(target, path).map_err(ConfigError::Io)
}

#[cfg(not(unix))]
fn create_symlink(target: &Path, path: &Path) -> Result<(), ConfigError> {
    fs::write(path, target.to_string_lossy().as_bytes()).map_err(ConfigError::Io)
}

impl Default for Activator {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of activation.
/// 激活结果。
#[derive(Debug, Clone)]
pub struct ActivationResult {
    /// Whether activation succeeded. / 激活是否成功。
    pub success: bool,
    /// Number of files installed. / 已安装的文件数。
    pub files_installed: usize,
    /// Number of services enabled. / 已启用的服务数。
    pub services_enabled: usize,
    /// Output from activation script. / 激活脚本的输出。
    pub script_output: Option<String>,
}

impl ActivationResult {
    /// Create a new activation result.
    /// 创建新的激活结果。
    pub fn new() -> Self {
        Self {
            success: false,
            files_installed: 0,
            services_enabled: 0,
            script_output: None,
        }
    }
}

impl Default for ActivationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of configuration test.
/// 配置测试结果。
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Whether test passed. / 测试是否通过。
    pub success: bool,
    /// Number of files checked. / 已检查的文件数。
    pub files_checked: usize,
    /// Warnings encountered. / 遇到的警告。
    pub warnings: Vec<String>,
    /// Errors encountered. / 遇到的错误。
    pub errors: Vec<String>,
}

impl TestResult {
    /// Create a new test result.
    /// 创建新的测试结果。
    pub fn new() -> Self {
        Self {
            success: false,
            files_checked: 0,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }
}

impl Default for TestResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve a target path under the configured root and reject path traversal.
/// 在配置的根目录下解析目标路径并拒绝路径穿越。
fn resolve_target_under_root(root: &Path, target: &Path) -> Result<PathBuf, ConfigError> {
    let mut rel = PathBuf::new();
    for comp in target.components() {
        match comp {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(seg) => rel.push(seg),
            Component::ParentDir => {
                return Err(ConfigError::Activation(format!(
                    "invalid target path (parent traversal): {}",
                    target.display()
                )));
            }
            Component::Prefix(_) => {
                return Err(ConfigError::Activation(format!(
                    "invalid target path prefix: {}",
                    target.display()
                )));
            }
        }
    }
    Ok(root.join(rel))
}

/// Validate service unit name.
/// 验证服务单元名称。
fn is_valid_service_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '@'))
}

/// Rollback to a previous configuration.
/// 回滚到之前的配置。
pub fn rollback(generation: u64, generations_dir: &Path) -> Result<PathBuf, ConfigError> {
    let gen_path = generations_dir.join(format!("generation-{}", generation));

    if !gen_path.exists() {
        return Err(ConfigError::NotFound(format!(
            "generation {} not found",
            generation
        )));
    }

    Ok(gen_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::GeneratedFile;
    use tempfile::tempdir;

    fn write_exec_script(path: &Path, content: &str) -> Result<(), Box<dyn std::error::Error>> {
        fs::write(path, content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        }
        Ok(())
    }

    #[cfg(unix)]
    fn set_distinct_group_if_permitted(path: &Path) -> io::Result<(u32, u32)> {
        let original = fs::metadata(path)?;
        let (uid, gid) = file_ownership(&original).expect("Unix file has ownership");
        // Restricted root containers may not have CAP_CHOWN; the current-group case still runs.
        if unsafe { libc::geteuid() } == 0
            && let Some(other_gid) = gid.checked_add(1)
        {
            let file = File::open(path)?;
            match set_file_ownership(&file, uid, other_gid) {
                Ok(()) => return Ok((uid, other_gid)),
                Err(err) if err.raw_os_error() == Some(libc::EPERM) => {}
                Err(err) => return Err(err),
            }
        }
        Ok((uid, gid))
    }

    #[cfg(unix)]
    #[test]
    fn activate_replacing_existing_file_preserves_owner_group_and_mode()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;
        let root = tempdir()?;
        let target = root.path().join("etc/existing.conf");
        fs::create_dir_all(target.parent().expect("target has parent"))?;
        fs::write(&target, "old-content\n")?;
        let expected_owner = set_distinct_group_if_permitted(&target)?;
        let source = root.path().join("source");
        fs::write(&source, "new-content\n")?;
        let mut generated = GeneratedConfig::new();
        generated.files.push(GeneratedFile {
            source,
            target: PathBuf::from("/etc/existing.conf"),
            mode: 0o640,
        });

        Activator::new().root(root.path()).activate(&generated)?;
        let metadata = fs::metadata(&target)?;
        assert_eq!(fs::read_to_string(&target)?, "new-content\n");
        assert_eq!(metadata.permissions().mode() & 0o777, 0o640);
        assert_eq!(file_ownership(&metadata), Some(expected_owner));
        Ok(())
    }

    #[test]
    fn test_activate_enables_service_symlink() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let src = root.path().join("demo.service.src");
        fs::write(&src, "[Service]\nExecStart=/bin/true\n")?;

        let mut generated = GeneratedConfig::new();
        generated.files.push(GeneratedFile {
            source: src,
            target: PathBuf::from("/etc/systemd/system/demo.service"),
            mode: 0o644,
        });
        generated.services.push("demo".to_string());

        let activator = Activator::new().root(root.path());
        let result = activator.activate(&generated)?;
        assert!(result.success);
        assert_eq!(result.services_enabled, 1);

        let wants = root
            .path()
            .join("etc/systemd/system/multi-user.target.wants/demo.service");
        assert!(fs::symlink_metadata(&wants).is_ok());
        #[cfg(unix)]
        {
            let link_target = fs::read_link(&wants)?;
            assert_eq!(link_target, PathBuf::from("../demo.service"));
        }

        Ok(())
    }

    #[test]
    fn activate_supplied_script_fails_before_mutation() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let target = root.path().join("etc/existing.conf");
        fs::create_dir_all(target.parent().expect("target has parent"))?;
        fs::write(&target, "old-content\n")?;
        let source = root.path().join("existing.new");
        fs::write(&source, "new-content\n")?;
        let marker = root.path().join("script-ran");
        let script = root.path().join("activate.sh");
        write_exec_script(
            &script,
            &format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        )?;

        let mut generated = GeneratedConfig::new();
        generated.files.push(GeneratedFile {
            source,
            target: PathBuf::from("/etc/existing.conf"),
            mode: 0o644,
        });
        generated.activation_script = Some(script);

        let err = Activator::new()
            .root(root.path())
            .activate(&generated)
            .expect_err("supplied script must be rejected");
        assert!(err.to_string().contains("cannot confine"));
        assert_eq!(fs::read_to_string(target)?, "old-content\n");
        assert!(!marker.exists(), "rejected activation script must not run");
        Ok(())
    }

    #[test]
    fn activate_dry_run_rejects_supplied_script() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let mut generated = GeneratedConfig::new();
        generated.activation_script = Some(root.path().join("activate.sh"));

        let err = Activator::new()
            .root(root.path())
            .dry_run(true)
            .activate(&generated)
            .expect_err("dry run must not promise arbitrary script execution");
        assert!(err.to_string().contains("cannot confine"));
        Ok(())
    }

    #[test]
    fn activate_later_failure_restores_file_mode_owner_and_symlink()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let etc = root.path().join("etc");
        fs::create_dir_all(&etc)?;
        let file_target = etc.join("existing.conf");
        fs::write(&file_target, "old-content\n")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file_target, fs::Permissions::from_mode(0o600))?;
        }
        #[cfg(unix)]
        let original_owner = set_distinct_group_if_permitted(&file_target)?;
        let link_target = etc.join("linked.conf");
        #[cfg(unix)]
        std::os::unix::fs::symlink("original.conf", &link_target)?;
        #[cfg(not(unix))]
        fs::write(&link_target, "old-link-placeholder")?;

        let file_source = root.path().join("existing.new");
        fs::write(&file_source, "new-content\n")?;
        let link_source = root.path().join("linked.new");
        fs::write(&link_source, "replacement\n")?;
        let mut generated = GeneratedConfig::new();
        generated.files.push(GeneratedFile {
            source: file_source,
            target: PathBuf::from("/etc/existing.conf"),
            mode: 0o644,
        });
        generated.files.push(GeneratedFile {
            source: link_source,
            target: PathBuf::from("/etc/linked.conf"),
            mode: 0o644,
        });
        generated.files.push(GeneratedFile {
            source: root.path().join("missing"),
            target: PathBuf::from("/etc/later.conf"),
            mode: 0o644,
        });

        Activator::new()
            .root(root.path())
            .activate(&generated)
            .expect_err("missing later source must fail activation");
        assert_eq!(fs::read_to_string(&file_target)?, "old-content\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&file_target)?.permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                file_ownership(&fs::metadata(&file_target)?),
                Some(original_owner)
            );
            assert_eq!(fs::read_link(&link_target)?, PathBuf::from("original.conf"));
        }
        #[cfg(not(unix))]
        assert_eq!(fs::read_to_string(&link_target)?, "old-link-placeholder");
        assert!(!etc.join("later.conf").exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn activate_symlinked_parent_leaves_outside_sentinel_unchanged()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let outside = tempdir()?;
        let sentinel = outside.path().join("sentinel");
        fs::write(&sentinel, "outside-content\n")?;
        std::os::unix::fs::symlink(outside.path(), root.path().join("etc"))?;
        let source = root.path().join("replacement");
        fs::write(&source, "replacement\n")?;
        let mut generated = GeneratedConfig::new();
        generated.files.push(GeneratedFile {
            source,
            target: PathBuf::from("/etc/sentinel"),
            mode: 0o644,
        });

        let err = Activator::new()
            .root(root.path())
            .activate(&generated)
            .expect_err("symlinked parent must be rejected");
        assert!(err.to_string().contains("parent is a symlink"));
        assert_eq!(fs::read_to_string(sentinel)?, "outside-content\n");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn activate_later_service_failure_restores_existing_symlink()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let systemd = root.path().join("etc/systemd/system");
        let wants = systemd.join("multi-user.target.wants");
        fs::create_dir_all(&wants)?;
        let enabled = wants.join("demo.service");
        std::os::unix::fs::symlink("../old-demo.service", &enabled)?;
        fs::write(systemd.join("demo.service"), "[Service]\n")?;
        let mut generated = GeneratedConfig::new();
        generated.services = vec!["demo".to_string(), "../invalid".to_string()];

        Activator::new()
            .root(root.path())
            .activate(&generated)
            .expect_err("later invalid service must roll back earlier service");
        assert_eq!(
            fs::read_link(enabled)?,
            PathBuf::from("../old-demo.service")
        );
        Ok(())
    }

    #[test]
    fn test_activate_rejects_invalid_service_name() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let mut generated = GeneratedConfig::new();
        generated.services.push("../bad".to_string());

        let activator = Activator::new().root(root.path());
        let err = activator
            .activate(&generated)
            .expect_err("activation should fail");
        assert!(err.to_string().contains("invalid service name"));
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn activate_untrusted_writable_root_rejects_without_writing()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;
        let root = tempdir()?;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o777))?;
        let source = root.path().join("source");
        fs::write(&source, "private")?;
        let mut generated = GeneratedConfig::new();
        generated.files.push(GeneratedFile {
            source,
            target: PathBuf::from("/etc/secret"),
            mode: 0o600,
        });
        let error = Activator::new()
            .root(root.path())
            .activate(&generated)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("untrusted writable activation directory")
        );
        assert!(!root.path().join("etc").exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn activate_untrusted_writable_descendant_rejects_without_writing()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;
        let root = tempdir()?;
        let etc = root.path().join("etc");
        fs::create_dir(&etc)?;
        fs::set_permissions(&etc, fs::Permissions::from_mode(0o777))?;
        let source = root.path().join("source");
        fs::write(&source, "private")?;
        let mut generated = GeneratedConfig::new();
        generated.files.push(GeneratedFile {
            source,
            target: PathBuf::from("/etc/secret"),
            mode: 0o600,
        });
        let error = Activator::new()
            .root(root.path())
            .activate(&generated)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("untrusted writable activation directory")
        );
        assert!(!etc.join("secret").exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn activation_temp_file_is_private_before_writing() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;
        let root = tempdir()?;
        let target = root.path().join("secret");
        let (file, temp_path) = create_temp_file(&target)?;
        assert_eq!(file.metadata()?.permissions().mode() & 0o777, 0o600);
        drop(file);
        fs::remove_file(temp_path)?;
        Ok(())
    }
}
