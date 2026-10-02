//! Metadata database for the store.
//! 存储的元数据数据库。
//!
//! Stores information about derivations, their outputs, and references.
//! 存储有关推导、其输出和引用的信息。

use crate::StoreError;
use n3v3_derive::{Hash, StorePath};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

/// Metadata about a store path.
/// 存储路径的元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathInfo {
    /// The store path. / 存储路径。
    pub path: StorePath,
    /// Hash of the path contents. / 路径内容的哈希。
    pub nar_hash: Hash,
    /// Size of the path in bytes. / 路径的大小（字节）。
    pub nar_size: u64,
    /// Paths that this path references. / 此路径引用的路径。
    pub references: HashSet<StorePath>,
    /// The derivation that produced this path (if any). / 产生此路径的推导（如有）。
    pub deriver: Option<StorePath>,
    /// Registration time (Unix timestamp). / 注册时间（Unix 时间戳）。
    pub registration_time: u64,
    /// Whether this is a valid path. / 是否为有效路径。
    pub valid: bool,
}

impl PathInfo {
    /// Create a new PathInfo.
    /// 创建新的 PathInfo。
    pub fn new(path: StorePath, nar_hash: Hash, nar_size: u64) -> Self {
        Self {
            path,
            nar_hash,
            nar_size,
            references: HashSet::new(),
            deriver: None,
            registration_time: current_time(),
            valid: true,
        }
    }

    /// Add a reference.
    /// 添加引用。
    pub fn add_reference(&mut self, path: StorePath) {
        self.references.insert(path);
    }

    /// Set the deriver.
    /// 设置推导器。
    pub fn set_deriver(&mut self, drv: StorePath) {
        self.deriver = Some(drv);
    }
}

/// The metadata database.
/// 元数据数据库。
pub struct Database {
    /// Root directory for the database. / 数据库的根目录。
    root: PathBuf,
    /// Cached path info. / 缓存的路径信息。
    cache: HashMap<StorePath, PathInfo>,
}

impl Database {
    /// Get the root directory of the database.
    /// 获取数据库的根目录。
    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    /// Open the database at the given root.
    /// 在给定的根目录打开数据库。
    pub fn open(root: PathBuf) -> Result<Self, StoreError> {
        let db_dir = root.join("db");
        fs::create_dir_all(&db_dir)?;

        Ok(Self {
            root: db_dir,
            cache: HashMap::new(),
        })
    }

    /// Encode the complete name into bounded, filesystem-safe components.
    pub(crate) fn info_path(&self, store_path: &StorePath) -> PathBuf {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        const COMPONENT_BYTES: usize = 64;
        let mut path = self.root.join(store_path.hash().to_string());
        for chunk in store_path.name().as_bytes().chunks(COMPONENT_BYTES) {
            let mut component = String::with_capacity(chunk.len() * 2);
            for byte in chunk {
                component.push(char::from(HEX[(byte >> 4) as usize]));
                component.push(char::from(HEX[(byte & 0xf) as usize]));
            }
            path.push(component);
        }
        path.join("info.json")
    }

    fn legacy_info_path(&self, store_path: &StorePath) -> PathBuf {
        self.root.join(format!("{}.json", store_path.hash()))
    }

    fn read_info(path: &std::path::Path, requested: &StorePath) -> Result<PathInfo, StoreError> {
        let info: PathInfo = serde_json::from_str(&fs::read_to_string(path)?)?;
        if info.path != *requested {
            return Err(StoreError::InvalidPath(format!(
                "PathInfo '{}' for '{}' belongs to '{}'",
                path.display(),
                requested.display_name(),
                info.path.display_name()
            )));
        }
        Ok(info)
    }

    fn legacy_info(&self, store_path: &StorePath) -> Result<Option<PathInfo>, StoreError> {
        let path = self.legacy_info_path(store_path);
        match fs::read_to_string(&path) {
            Ok(json) => {
                let info: PathInfo = serde_json::from_str(&json)?;
                if info.path != *store_path {
                    return Err(StoreError::InvalidPath(format!(
                        "Legacy PathInfo '{}' for '{}' belongs to '{}'",
                        path.display(),
                        store_path.display_name(),
                        info.path.display_name()
                    )));
                }
                Ok(Some(info))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Register a path in the database.
    /// 在数据库中注册路径。
    pub fn register(&mut self, info: PathInfo) -> Result<(), StoreError> {
        let path = self.info_path(&info.path);
        let json = serde_json::to_string_pretty(&info)?;
        fs::create_dir_all(path.parent().ok_or_else(|| {
            StoreError::InvalidPath(format!("Invalid PathInfo location '{}'", path.display()))
        })?)?;
        fs::write(&path, json)?;
        self.cache.insert(info.path.clone(), info);
        Ok(())
    }

    /// Query path info.
    /// 查询路径信息。
    pub fn query(&mut self, store_path: &StorePath) -> Result<Option<PathInfo>, StoreError> {
        if let Some(info) = self.cache.get(store_path) {
            return Ok(Some(info.clone()));
        }

        let path = self.info_path(store_path);
        let info = match fs::symlink_metadata(&path) {
            Ok(_) => Some(Self::read_info(&path, store_path)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.legacy_info(store_path)?
            }
            Err(error) => return Err(error.into()),
        };
        if let Some(info) = &info {
            self.cache.insert(store_path.clone(), info.clone());
        }
        Ok(info)
    }

    /// Check if a path is valid (registered and exists).
    /// 检查路径是否有效（已注册且存在）。
    pub fn is_valid(&mut self, store_path: &StorePath) -> Result<bool, StoreError> {
        Ok(self.query(store_path)?.map(|i| i.valid).unwrap_or(false))
    }

    /// Get all references of a path.
    /// 获取路径的所有引用。
    pub fn get_references(
        &mut self,
        store_path: &StorePath,
    ) -> Result<HashSet<StorePath>, StoreError> {
        Ok(self
            .query(store_path)?
            .map(|i| i.references)
            .unwrap_or_default())
    }

    /// Get paths that reference the given path (referrers).
    /// 获取引用给定路径的路径（引用者）。
    pub fn get_referrers(
        &mut self,
        store_path: &StorePath,
    ) -> Result<HashSet<StorePath>, StoreError> {
        let mut referrers = HashSet::new();
        for path in self.list_all()? {
            if self.get_references(&path)?.contains(store_path) {
                referrers.insert(path);
            }
        }
        Ok(referrers)
    }

    /// Delete path info from the database.
    /// 从数据库中删除路径信息。
    pub fn delete(&mut self, store_path: &StorePath) -> Result<(), StoreError> {
        let path = self.info_path(store_path);
        let has_current = path.exists();
        if has_current {
            Self::read_info(&path, store_path)?;
        }
        let has_legacy = match self.legacy_info(store_path) {
            Ok(info) => info.is_some(),
            // A different name can own the old hash-only record. Never delete it.
            Err(StoreError::InvalidPath(_)) if has_current => false,
            Err(error) => return Err(error),
        };
        if has_legacy {
            fs::remove_file(self.legacy_info_path(store_path))?;
        }
        if has_current {
            fs::remove_file(&path)?;
        }
        self.cache.remove(store_path);
        Ok(())
    }

    /// Invalidate a path (mark as not valid).
    /// 使路径无效（标记为无效）。
    pub fn invalidate(&mut self, store_path: &StorePath) -> Result<(), StoreError> {
        if let Some(mut info) = self.query(store_path)? {
            info.valid = false;
            self.register(info)?;
        }
        Ok(())
    }

    /// List all registered paths.
    /// 列出所有已注册的路径。
    pub fn list_all(&self) -> Result<Vec<StorePath>, StoreError> {
        let mut paths = HashSet::new();
        let mut pending = vec![self.root.clone()];
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                if kind.is_dir() {
                    pending.push(entry.path());
                } else if kind.is_file() && entry.path().extension().is_some_and(|e| e == "json") {
                    let info: PathInfo = serde_json::from_str(&fs::read_to_string(entry.path())?)?;
                    paths.insert(info.path);
                }
            }
        }
        let mut paths = paths.into_iter().collect::<Vec<_>>();
        paths.sort();
        Ok(paths)
    }
}

/// Get current Unix timestamp.
/// 获取当前 Unix 时间戳。
fn current_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_info_same_hash_different_names_coexist() {
        let temp = tempfile::tempdir().unwrap();
        let hash = Hash::of(b"same contents");
        let first = StorePath::new(hash, "a".into());
        let second = StorePath::new(hash, "b".into());
        let mut db = Database::open(temp.path().to_path_buf()).unwrap();
        db.register(PathInfo::new(first.clone(), hash, 10)).unwrap();
        db.register(PathInfo::new(second.clone(), hash, 20))
            .unwrap();

        let mut reopened = Database::open(temp.path().to_path_buf()).unwrap();
        assert_eq!(reopened.query(&first).unwrap().unwrap().nar_size, 10);
        assert_eq!(reopened.query(&second).unwrap().unwrap().nar_size, 20);
        assert_eq!(
            reopened.list_all().unwrap(),
            vec![first.clone(), second.clone()]
        );
        reopened.delete(&first).unwrap();
        assert!(reopened.query(&first).unwrap().is_none());
        assert_eq!(reopened.query(&second).unwrap().unwrap().nar_size, 20);
    }

    #[test]
    fn path_info_matching_legacy_record_reads_without_duplicate_enumeration() {
        let temp = tempfile::tempdir().unwrap();
        let hash = Hash::of(b"legacy");
        let path = StorePath::new(hash, "legacy".into());
        let info = PathInfo::new(path.clone(), hash, 42);
        let mut db = Database::open(temp.path().to_path_buf()).unwrap();
        let legacy = db.legacy_info_path(&path);
        fs::write(&legacy, serde_json::to_vec(&info).unwrap()).unwrap();
        assert_eq!(db.query(&path).unwrap().unwrap().nar_size, 42);
        db.register(info).unwrap();
        assert_eq!(db.list_all().unwrap(), vec![path.clone()]);
        db.delete(&path).unwrap();
        assert!(!legacy.exists());
        assert!(db.query(&path).unwrap().is_none());
    }

    #[test]
    fn path_info_mismatched_legacy_record_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let hash = Hash::of(b"shared");
        let first = StorePath::new(hash, "a".into());
        let second = StorePath::new(hash, "b".into());
        let mut db = Database::open(temp.path().to_path_buf()).unwrap();
        let legacy = db.legacy_info_path(&first);
        fs::write(
            &legacy,
            serde_json::to_vec(&PathInfo::new(first.clone(), hash, 1)).unwrap(),
        )
        .unwrap();
        let error = db.query(&second).unwrap_err();
        assert!(error.to_string().contains("Legacy PathInfo"), "{error}");
        assert!(error.to_string().contains(&first.display_name()), "{error}");
        assert!(db.delete(&second).is_err());
        assert!(legacy.exists());
        assert_eq!(db.query(&first).unwrap().unwrap().nar_size, 1);
        db.register(PathInfo::new(second.clone(), hash, 2)).unwrap();
        assert_eq!(db.list_all().unwrap(), vec![first.clone(), second.clone()]);
        assert_eq!(db.query(&second).unwrap().unwrap().nar_size, 2);
        db.delete(&second).unwrap();
        assert!(legacy.exists());
        assert_eq!(db.query(&first).unwrap().unwrap().nar_size, 1);
    }
}
