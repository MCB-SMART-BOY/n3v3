//! Configuration generations.
//! 配置代。
//!
//! Manages configuration history for rollback support.
//! 管理配置历史以支持回滚。

use crate::ConfigError;
use n3v3_derive::StorePath;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Generations directory name.
/// 代目录名称。
const GENERATIONS_DIR: &str = "generations";

/// Generation manager.
/// 代管理器。
pub struct GenerationManager {
    /// Base directory for generations. / 代的基础目录。
    base_dir: PathBuf,
}

impl GenerationManager {
    /// Create a new generation manager.
    /// 创建新的代管理器。
    pub fn new(base_dir: PathBuf) -> Result<Self, ConfigError> {
        fs::create_dir_all(&base_dir)?;
        let base_dir = fs::canonicalize(&base_dir)?;
        check_generation_ancestors(&base_dir)?;
        let gen_dir = base_dir.join(GENERATIONS_DIR);
        fs::create_dir_all(&gen_dir)?;
        check_generations_directory(&gen_dir)?;
        Ok(Self { base_dir })
    }

    /// Get the generations directory.
    /// 获取代目录。
    fn generations_dir(&self) -> PathBuf {
        self.base_dir.join(GENERATIONS_DIR)
    }

    /// Get the current generation link path.
    /// 获取当前代链接路径。
    fn current_link(&self) -> PathBuf {
        self.generations_dir().join("current")
    }
    /// Read the generation whose activation completed successfully.
    pub fn active_generation(&self) -> Result<Option<u64>, ConfigError> {
        self.generation_at_link(&self.generations_dir().join("active"))
    }

    /// Record successful activation independently of the latest build.
    pub fn mark_active_generation(&self, number: u64) -> Result<(), ConfigError> {
        let generation = self.load_generation(number)?;
        replace_current_link_atomically(&self.generations_dir().join("active"), &generation.path)
            .map_err(PublicationError::into_config_error)
    }
    /// Read the most recently completed build, independent of activation.
    pub fn latest_built_generation(&self) -> Result<Option<u64>, ConfigError> {
        self.generation_at_link(&self.generations_dir().join("latest-built"))
    }

    /// Record a completed build so a failed activation remains retryable.
    pub fn mark_latest_built_generation(&self, number: u64) -> Result<(), ConfigError> {
        let generation = self.load_generation(number)?;
        replace_current_link_atomically(
            &self.generations_dir().join("latest-built"),
            &generation.path,
        )
        .map_err(PublicationError::into_config_error)
    }

    /// Get the path for a specific generation.
    /// 获取特定代的路径。
    fn generation_path(&self, num: u64) -> PathBuf {
        self.generations_dir().join(format!("generation-{}", num))
    }

    /// Read the target of a generation link.
    /// 读取代链接的目标。
    ///
    /// Unix stores the link as a symlink; other platforms (Windows without
    /// symlink privileges) store it as a text file holding the target path.
    /// Unix 上链接是符号链接；其他平台（无符号链接权限的 Windows）写成保存目标
    /// 路径的文本文件。
    fn read_link_target(link: &PathBuf) -> Result<Option<PathBuf>, ConfigError> {
        if link.is_symlink() {
            return Ok(Some(fs::read_link(link)?));
        }
        if link.exists() {
            return Ok(Some(PathBuf::from(fs::read_to_string(link)?.trim())));
        }
        Ok(None)
    }

    /// Get the current generation number.
    /// 获取当前代号。
    pub fn current_generation(&self) -> Result<Option<u64>, ConfigError> {
        self.generation_at_link(&self.current_link())
    }

    fn generation_at_link(&self, link: &PathBuf) -> Result<Option<u64>, ConfigError> {
        let Some(target) = Self::read_link_target(link)? else {
            return Ok(None);
        };
        let name = target
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ConfigError::Invalid("invalid generation link".to_string()))?;
        let gen_str = name
            .strip_prefix("generation-")
            .ok_or_else(|| ConfigError::Invalid("invalid generation link format".to_string()))?;
        let number = gen_str
            .parse::<u64>()
            .map_err(|_| ConfigError::Invalid("invalid generation number".to_string()))?;
        if target != self.generation_path(number) {
            return Err(ConfigError::Invalid(format!(
                "generation pointer {} does not belong to this manager",
                link.display()
            )));
        }
        Ok(Some(number))
    }

    /// Get the next generation number after every retained generation.
    /// 获取所有保留代之后的下一个代号。
    pub fn next_generation(&self) -> Result<u64, ConfigError> {
        let mut max_generation = self.current_generation()?;
        for entry in fs::read_dir(self.generations_dir())? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(number) = name
                .to_str()
                .and_then(|value| value.strip_prefix("generation-"))
                .and_then(|value| value.parse::<u64>().ok())
            else {
                continue;
            };
            max_generation =
                Some(max_generation.map_or(number, |current: u64| current.max(number)));
        }

        max_generation
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| ConfigError::Invalid("generation number overflow".to_string()))
    }

    /// Create and publish a new generation.
    /// 创建并发布一个新代。
    pub fn create_generation(
        &self,
        store_path: &StorePath,
        metadata: GenerationMetadata,
    ) -> Result<Generation, ConfigError> {
        let generation = self.create_generation_unpublished(store_path, metadata)?;
        self.publish_generation(&generation)
            .map_err(|error| error.into_config_error())?;
        Ok(generation)
    }

    /// Create a generation without changing the current pointer.
    /// 创建一个 generation，但不修改 current 指针。
    pub fn create_generation_unpublished(
        &self,
        store_path: &StorePath,
        metadata: GenerationMetadata,
    ) -> Result<Generation, ConfigError> {
        let gen_num = self.next_generation()?;
        let gen_path = self.generation_path(gen_num);
        create_private_generation_dir(&gen_path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                ConfigError::Invalid(format!("generation {} already exists", gen_num))
            } else {
                ConfigError::Io(error)
            }
        })?;

        if let Err(error) = write_generation_contents(&gen_path, store_path, &metadata) {
            let _ = fs::remove_dir_all(&gen_path);
            return Err(error);
        }

        Ok(Generation {
            number: gen_num,
            path: gen_path,
            store_path: store_path.clone(),
            metadata,
        })
    }

    /// Publish a fully prepared generation as current.
    /// 将已完整准备的 generation 发布为 current。
    pub fn publish_generation(&self, generation: &Generation) -> Result<(), PublicationError> {
        self.publish_generation_with_sync(generation, |parent| {
            #[cfg(unix)]
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })
    }

    fn publish_generation_with_sync(
        &self,
        generation: &Generation,
        sync_parent: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> Result<(), PublicationError> {
        let expected_path = self.generation_path(generation.number);
        if generation.path != expected_path {
            return Err(PublicationError::NotCommitted(ConfigError::Invalid(
                format!(
                    "generation {} does not belong to this manager",
                    generation.number
                ),
            )));
        }
        self.load_generation(generation.number)
            .map_err(PublicationError::NotCommitted)?;
        let previous =
            Self::read_link_target(&self.current_link()).map_err(PublicationError::NotCommitted)?;
        match replace_current_link_with_sync(&self.current_link(), &expected_path, sync_parent) {
            Ok(()) => Ok(()),
            Err(PublicationError::Committed { source, .. }) => {
                let restoration = match previous {
                    Some(target) => replace_current_link_atomically(&self.current_link(), &target),
                    None => self.clear_current().map_err(PublicationError::NotCommitted),
                };
                Err(PublicationError::Committed {
                    source,
                    restoration: restoration
                        .err()
                        .map(PublicationError::into_config_error)
                        .map(Box::new),
                })
            }
            Err(error) => Err(error),
        }
    }

    /// List all generations.
    /// 列出所有代。
    pub fn list_generations(&self) -> Result<Vec<Generation>, ConfigError> {
        let mut generations = Vec::new();
        let dir = self.generations_dir();

        if !dir.exists() {
            return Ok(generations);
        }

        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if let Some(gen_str) = name_str.strip_prefix("generation-")
                && let Ok(gen_num) = gen_str.parse::<u64>()
                && let Ok(generation) = self.load_generation(gen_num)
            {
                generations.push(generation);
            }
        }

        generations.sort_by_key(|g| g.number);
        Ok(generations)
    }

    /// Load a specific generation.
    /// 加载特定的代。
    pub fn load_generation(&self, number: u64) -> Result<Generation, ConfigError> {
        let gen_path = self.generation_path(number);

        if !gen_path.exists() {
            return Err(ConfigError::NotFound(format!("generation {}", number)));
        }

        // Load metadata
        // 加载元数据
        let meta_path = gen_path.join("metadata.json");
        let metadata = if meta_path.exists() {
            let content = fs::read_to_string(&meta_path)?;
            serde_json::from_str(&content)
                .map_err(|e| ConfigError::Invalid(format!("JSON error: {}", e)))?
        } else {
            GenerationMetadata::default()
        };

        // Load store path
        // 加载存储路径
        let store_link = gen_path.join("system");
        let store_path_str = Self::read_link_target(&store_link)?
            .ok_or_else(|| ConfigError::Invalid("missing system link".to_string()))?
            .to_string_lossy()
            .into_owned();

        let store_path = StorePath::parse_name(&store_path_str)
            .ok_or_else(|| ConfigError::Invalid("invalid store path".to_string()))?;

        Ok(Generation {
            number,
            path: gen_path,
            store_path,
            metadata,
        })
    }

    /// Switch to a specific generation.
    /// 切换到特定的代。
    pub fn switch_to(&self, number: u64) -> Result<Generation, ConfigError> {
        let generation = self.load_generation(number)?;
        self.publish_generation(&generation)
            .map_err(PublicationError::into_config_error)?;
        Ok(generation)
    }

    /// Remove the current pointer while retaining every generation.
    /// 删除 current 指针，同时保留所有 generation。
    pub fn clear_current(&self) -> Result<(), ConfigError> {
        let current = self.current_link();
        if current.is_dir() && !current.is_symlink() {
            return Err(ConfigError::Invalid(format!(
                "current generation path is a directory: {}",
                current.display()
            )));
        }
        if current.exists() || current.is_symlink() {
            fs::remove_file(current)?;
            #[cfg(unix)]
            fs::File::open(self.generations_dir())?.sync_all()?;
        }
        Ok(())
    }

    /// Delete old generations, keeping the last N.
    /// 删除旧的代，保留最后 N 个。
    pub fn collect_garbage(&self, keep: usize) -> Result<usize, ConfigError> {
        let mut generations = self.list_generations()?;
        if generations.len() <= keep {
            return Ok(0);
        }
        let current = self.current_generation()?;
        let active = self.active_generation()?;
        let latest_built = self.latest_built_generation()?;
        generations.sort_by_key(|generation| std::cmp::Reverse(generation.number));
        let mut deleted = 0;
        for generation in generations.into_iter().skip(keep) {
            if [current, active, latest_built].contains(&Some(generation.number)) {
                continue;
            }
            if generation.path.exists() {
                fs::remove_dir_all(&generation.path)?;
                deleted += 1;
            }
        }
        Ok(deleted)
    }
}

/// A publication error indicates whether the new current pointer was renamed.
#[derive(Debug)]
pub enum PublicationError {
    /// Publication did not rename `current`.
    NotCommitted(ConfigError),
    /// Publication renamed `current`; restoration may also have failed.
    Committed {
        source: Box<ConfigError>,
        restoration: Option<Box<ConfigError>>,
    },
}

impl PublicationError {
    fn into_config_error(self) -> ConfigError {
        match self {
            Self::NotCommitted(error) => error,
            Self::Committed {
                source,
                restoration,
            } => ConfigError::Invalid(match restoration {
                Some(error) => format!(
                    "current pointer changed but sync failed: {source}; failed to restore old pointer: {error}"
                ),
                None => format!(
                    "current pointer changed but sync failed: {source}; old pointer restored"
                ),
            }),
        }
    }
}

impl std::fmt::Display for PublicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotCommitted(error) => write!(f, "{error}"),
            Self::Committed {
                source,
                restoration,
            } => match restoration {
                Some(error) => write!(
                    f,
                    "current pointer changed but sync failed: {source}; failed to restore old pointer: {error}"
                ),
                None => write!(
                    f,
                    "current pointer changed but sync failed: {source}; old pointer restored"
                ),
            },
        }
    }
}

fn create_private_generation_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn private_file(path: &Path) -> std::io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn check_generation_ancestors(base: &Path) -> Result<(), ConfigError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mut path = PathBuf::new();
        for component in base.components() {
            path.push(component);
            let metadata = fs::symlink_metadata(&path)?;
            let is_sticky_parent =
                path != base && metadata.uid() == 0 && metadata.mode() & 0o1000 != 0;
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || (metadata.uid() != 0 && metadata.uid() != unsafe { libc::geteuid() })
                || (metadata.mode() & 0o022 != 0 && !is_sticky_parent)
            {
                return Err(ConfigError::Invalid(format!(
                    "untrusted generation state directory: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn check_generations_directory(path: &Path) -> Result<(), ConfigError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o022 != 0
        {
            return Err(ConfigError::Invalid(format!(
                "untrusted generations directory: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

/// Write and durably record the contents of an unpublished generation.
/// 写入并持久记录未发布 generation 的内容。
fn write_generation_contents(
    gen_path: &Path,
    store_path: &StorePath,
    metadata: &GenerationMetadata,
) -> Result<(), ConfigError> {
    let meta_json = serde_json::to_string_pretty(metadata)
        .map_err(|error| ConfigError::Invalid(format!("JSON error: {}", error)))?;
    let meta_path = gen_path.join("metadata.json");
    let mut meta_file = private_file(&meta_path)?;
    use std::io::Write;
    meta_file.write_all(meta_json.as_bytes())?;
    meta_file.sync_all()?;

    let store_link = gen_path.join("system");
    #[cfg(unix)]
    std::os::unix::fs::symlink(store_path.display_name(), &store_link)?;
    #[cfg(not(unix))]
    {
        let mut file = private_file(&store_link)?;
        file.write_all(store_path.display_name().as_bytes())?;
        file.sync_all()?;
    }

    #[cfg(unix)]
    fs::File::open(gen_path)?.sync_all()?;
    Ok(())
}

/// Atomically replace the current generation link/file.
/// 原子替换当前代链接/文件。
fn replace_current_link_atomically(current: &Path, target: &Path) -> Result<(), PublicationError> {
    replace_current_link_with_sync(current, target, |parent| {
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })
}

fn replace_current_link_with_sync(
    current: &Path,
    target: &Path,
    sync_parent: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<(), PublicationError> {
    let parent = current.parent().ok_or_else(|| {
        PublicationError::NotCommitted(ConfigError::Invalid(
            "invalid current link path".to_string(),
        ))
    })?;
    if current.is_dir() && !current.is_symlink() {
        return Err(PublicationError::NotCommitted(ConfigError::Invalid(
            format!(
                "current generation path is a directory: {}",
                current.display()
            ),
        )));
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temp = parent.join(format!(".current.tmp-{}-{}", std::process::id(), nonce));
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, &temp)
        .map_err(|error| PublicationError::NotCommitted(ConfigError::Io(error)))?;
    #[cfg(not(unix))]
    {
        use std::io::Write;
        let mut file = private_file(&temp)
            .map_err(|error| PublicationError::NotCommitted(ConfigError::Io(error)))?;
        file.write_all(target.to_string_lossy().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|error| PublicationError::NotCommitted(ConfigError::Io(error)))?;
    }
    if let Err(error) = fs::rename(&temp, current) {
        let _ = fs::remove_file(&temp);
        return Err(PublicationError::NotCommitted(ConfigError::Io(error)));
    }
    sync_parent(parent).map_err(|error| PublicationError::Committed {
        source: Box::new(ConfigError::Io(error)),
        restoration: None,
    })
}

/// A configuration generation.
/// 配置代。
#[derive(Debug, Clone)]
pub struct Generation {
    /// Generation number. / 代号。
    pub number: u64,
    /// Path to the generation directory. / 代目录的路径。
    pub path: PathBuf,
    /// Store path of the configuration. / 配置的存储路径。
    pub store_path: StorePath,
    /// Generation metadata. / 代元数据。
    pub metadata: GenerationMetadata,
}

/// Generation metadata.
/// 代元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationMetadata {
    /// Creation timestamp. / 创建时间戳。
    pub created_at: u64,
    /// Configuration name. / 配置名称。
    pub name: Option<String>,
    /// Description. / 描述。
    pub description: Option<String>,
    /// Git commit (if applicable). / Git 提交（如果适用）。
    pub git_commit: Option<String>,
}

impl Default for GenerationMetadata {
    fn default() -> Self {
        Self {
            created_at: current_timestamp(),
            name: None,
            description: None,
            git_commit: None,
        }
    }
}

impl GenerationMetadata {
    /// Create new metadata.
    /// 创建新的元数据。
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the name.
    /// 设置名称。
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the description.
    /// 设置描述。
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

/// Get current Unix timestamp.
/// 获取当前 Unix 时间戳。
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_generation_reads_plain_file_link() {
        let dir = tempfile::tempdir().unwrap();
        let manager = GenerationManager::new(dir.path().to_path_buf()).unwrap();

        // Windows writes the current link as a plain file (no symlink privilege),
        // so simulate that form on every platform.
        // Windows 上当前代链接写成普通文件（无符号链接权限），因此在所有平台模拟该形式。
        let current = dir.path().join(GENERATIONS_DIR).join("current");
        fs::write(
            &current,
            manager.generation_path(7).to_string_lossy().as_bytes(),
        )
        .unwrap();

        assert_eq!(manager.current_generation().unwrap(), Some(7));
        assert_eq!(manager.next_generation().unwrap(), 8);
    }

    #[test]
    fn next_generation_after_rollback_uses_highest_retained_number() {
        let dir = tempfile::tempdir().unwrap();
        let manager = GenerationManager::new(dir.path().to_path_buf()).unwrap();
        let first = StorePath::new(n3v3_derive::Hash::of(b"one"), "one".to_string());
        let second = StorePath::new(n3v3_derive::Hash::of(b"two"), "two".to_string());
        manager
            .create_generation(&first, GenerationMetadata::new())
            .unwrap();
        manager
            .create_generation(&second, GenerationMetadata::new())
            .unwrap();
        manager.switch_to(1).unwrap();

        assert_eq!(manager.current_generation().unwrap(), Some(1));
        assert_eq!(manager.next_generation().unwrap(), 3);
        assert!(manager.generation_path(2).exists());
    }

    #[test]
    fn next_generation_maximum_retained_number_reports_overflow() {
        let dir = tempfile::tempdir().unwrap();
        let manager = GenerationManager::new(dir.path().to_path_buf()).unwrap();
        fs::create_dir(manager.generation_path(u64::MAX)).unwrap();

        let error = manager.next_generation().unwrap_err();
        assert!(error.to_string().contains("overflow"));
    }

    #[test]
    fn create_generation_unpublished_preserves_current_pointer() {
        let dir = tempfile::tempdir().unwrap();
        let manager = GenerationManager::new(dir.path().to_path_buf()).unwrap();
        let first = StorePath::new(n3v3_derive::Hash::of(b"one"), "one".to_string());
        let second = StorePath::new(n3v3_derive::Hash::of(b"two"), "two".to_string());
        manager
            .create_generation(&first, GenerationMetadata::new())
            .unwrap();
        let unpublished = manager
            .create_generation_unpublished(&second, GenerationMetadata::new())
            .unwrap();

        assert_eq!(unpublished.number, 2);
        assert_eq!(manager.current_generation().unwrap(), Some(1));
    }
    #[test]
    fn publication_post_rename_sync_failure_restores_previous_pointer() {
        let dir = tempfile::tempdir().unwrap();
        let manager = GenerationManager::new(dir.path().to_path_buf()).unwrap();
        let store = StorePath::new(n3v3_derive::Hash::of(b"config"), "config".to_string());
        manager
            .create_generation(&store, GenerationMetadata::new())
            .unwrap();
        let next = manager
            .create_generation_unpublished(&store, GenerationMetadata::new())
            .unwrap();
        let error = manager
            .publish_generation_with_sync(&next, |_| {
                Err(std::io::Error::other("injected parent sync failure"))
            })
            .unwrap_err();
        assert!(matches!(
            error,
            PublicationError::Committed {
                restoration: None,
                ..
            }
        ));
        assert_eq!(manager.current_generation().unwrap(), Some(1));
        assert!(manager.generation_path(next.number).exists());
        assert!(manager.load_generation(1).is_ok());
    }

    #[test]
    fn publication_post_rename_sync_failure_without_previous_pointer_clears_current() {
        let dir = tempfile::tempdir().unwrap();
        let manager = GenerationManager::new(dir.path().to_path_buf()).unwrap();
        let store = StorePath::new(n3v3_derive::Hash::of(b"config"), "config".to_string());
        let generation = manager
            .create_generation_unpublished(&store, GenerationMetadata::new())
            .unwrap();
        let error = manager
            .publish_generation_with_sync(&generation, |_| {
                Err(std::io::Error::other("injected parent sync failure"))
            })
            .unwrap_err();
        assert!(matches!(
            error,
            PublicationError::Committed {
                restoration: None,
                ..
            }
        ));
        assert_eq!(manager.current_generation().unwrap(), None);
        assert!(manager.load_generation(generation.number).is_ok());
    }

    #[test]
    fn generation_private_metadata_is_created_without_public_read_access() {
        let dir = tempfile::tempdir().unwrap();
        let manager = GenerationManager::new(dir.path().to_path_buf()).unwrap();
        let store = StorePath::new(n3v3_derive::Hash::of(b"config"), "config".to_string());
        let generation = manager
            .create_generation_unpublished(&store, GenerationMetadata::new())
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(generation.path.join("metadata.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(generation.path).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }

    #[test]
    fn generation_garbage_collection_keeps_active_and_latest_built() {
        let dir = tempfile::tempdir().unwrap();
        let manager = GenerationManager::new(dir.path().to_path_buf()).unwrap();
        let store = StorePath::new(n3v3_derive::Hash::of(b"config"), "config".to_string());
        for _ in 0..3 {
            manager
                .create_generation(&store, GenerationMetadata::new())
                .unwrap();
        }
        manager.mark_active_generation(1).unwrap();
        manager.mark_latest_built_generation(2).unwrap();
        assert_eq!(manager.collect_garbage(0).unwrap(), 0);
        for number in 1..=3 {
            assert!(manager.load_generation(number).is_ok());
        }
    }
}
