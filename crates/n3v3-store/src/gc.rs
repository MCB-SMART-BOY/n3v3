//! Garbage collection for the store.
//! 存储的垃圾回收。
//!
//! Garbage collection removes paths that are no longer reachable from
//! any GC root.
//! 垃圾回收移除从任何 GC 根不再可达的路径。

use crate::{Database, Store, StoreError};
use n3v3_derive::StorePath;
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// GC roots directory.
/// GC 根目录。
const GC_ROOTS_DIR: &str = "gcroots";

/// Garbage collector for the store.
/// 存储的垃圾回收器。
pub struct GarbageCollector<'a> {
    store: &'a mut Store,
    profile_dir: Option<PathBuf>,
}

impl<'a> GarbageCollector<'a> {
    /// Create a collector that discovers all store-registered profiles.
    pub fn new(store: &'a mut Store) -> Self {
        Self {
            store,
            profile_dir: None,
        }
    }

    /// Create a collector for an explicit profile directory.
    pub fn with_profile_dir(store: &'a mut Store, profile_dir: PathBuf) -> Self {
        Self {
            store,
            profile_dir: Some(profile_dir),
        }
    }

    /// Get the GC roots directory.
    /// 获取 GC 根目录。
    fn roots_dir(&self) -> PathBuf {
        self.store.root().join(GC_ROOTS_DIR)
    }

    /// Add a GC root.
    /// 添加 GC 根。
    pub fn add_root(&self, name: &str, path: &StorePath) -> Result<(), StoreError> {
        Self::validate_root_name(name)?;
        let roots_dir = self.roots_dir();
        fs::create_dir_all(&roots_dir)?;

        let link_path = roots_dir.join(name);
        let target = self.store.to_path(path);

        // Remove existing link if present
        // 如果存在则移除现有链接
        if link_path.exists() || link_path.is_symlink() {
            fs::remove_file(&link_path)?;
        }

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link_path)?;

        #[cfg(not(unix))]
        fs::write(&link_path, target.to_string_lossy().as_bytes())?;

        Ok(())
    }

    /// Remove a GC root.
    /// 移除 GC 根。
    pub fn remove_root(&self, name: &str) -> Result<(), StoreError> {
        Self::validate_root_name(name)?;
        let link_path = self.roots_dir().join(name);
        if link_path.exists() || link_path.is_symlink() {
            fs::remove_file(&link_path)?;
        }
        Ok(())
    }

    fn validate_root_name(name: &str) -> Result<(), StoreError> {
        let mut components = Path::new(name).components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return Err(StoreError::InvalidPath(format!(
                "Invalid GC root name '{name}'"
            )));
        }
        Ok(())
    }

    fn validate_store_context(&self) -> Result<(), StoreError> {
        if let Some(configured) = std::env::var_os("N3V3_STORE") {
            let configured = PathBuf::from(configured);
            if !configured.is_absolute()
                || fs::canonicalize(&configured)? != fs::canonicalize(self.store.root())?
            {
                return Err(StoreError::InvalidPath(format!(
                    "N3V3_STORE '{}' does not match GC store '{}'",
                    configured.display(),
                    self.store.root().display()
                )));
            }
        }
        Ok(())
    }

    /// List all GC roots. Invalid roots abort collection rather than making referenced paths collectible.
    pub fn list_roots(&self) -> Result<Vec<(String, StorePath)>, StoreError> {
        let roots_dir = self.roots_dir();
        match fs::symlink_metadata(&roots_dir) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => {
                return Err(StoreError::InvalidPath(format!(
                    "GC roots directory '{}' is not a directory",
                    roots_dir.display()
                )));
            }
        }

        let mut roots = Vec::new();
        for entry in fs::read_dir(&roots_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let target = if path.is_symlink() {
                fs::read_link(&path)?
            } else {
                PathBuf::from(fs::read_to_string(&path)?)
            };
            let store_path = self.parse_store_root(&target)?;
            roots.push((name, store_path));
        }
        Ok(roots)
    }

    fn parse_store_root(&self, path: &Path) -> Result<StorePath, StoreError> {
        let store_root = fs::canonicalize(self.store.root())?;
        let parent = path.parent().ok_or_else(|| {
            StoreError::InvalidPath(format!("GC root '{}' has no parent", path.display()))
        })?;
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(StoreError::InvalidPath(format!(
                "GC root '{}' is not an absolute direct store entry",
                path.display()
            )));
        }
        let canonical_parent = fs::canonicalize(parent).map_err(|error| {
            StoreError::InvalidPath(format!(
                "Failed to resolve GC root '{}' parent '{}': {error}",
                path.display(),
                parent.display()
            ))
        })?;
        if canonical_parent != store_root {
            return Err(StoreError::InvalidPath(format!(
                "GC root '{}' is outside store '{}'",
                path.display(),
                store_root.display()
            )));
        }
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            StoreError::InvalidPath(format!(
                "Failed to inspect GC root '{}': {error}",
                path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(StoreError::InvalidPath(format!(
                "GC root '{}' is a store alias",
                path.display()
            )));
        }
        let store_path = StorePath::parse(path).ok_or_else(|| {
            StoreError::InvalidPath(format!("Invalid GC root '{}'", path.display()))
        })?;
        if !self.store.path_exists(&store_path) {
            return Err(StoreError::PathNotFound(path.display().to_string()));
        }
        Ok(store_path)
    }

    fn profile_roots(&self) -> Result<Vec<StorePath>, StoreError> {
        let mut profiles = self.store.profile_dirs()?;
        if let Some(profile) = &self.profile_dir
            && !profiles.contains(profile)
        {
            profiles.push(profile.clone());
        }
        let mut roots = Vec::new();
        for profile in profiles {
            roots.extend(self.read_profile_roots(&profile)?);
        }
        Ok(roots)
    }

    fn read_profile_roots(&self, profile_dir: &Path) -> Result<Vec<StorePath>, StoreError> {
        match fs::symlink_metadata(profile_dir) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => {
                return Err(StoreError::InvalidPath(format!(
                    "Profile root '{}' is not a directory",
                    profile_dir.display()
                )));
            }
        }
        crate::store::validate_profile_path(profile_dir)?;
        self.validate_current_profile(profile_dir)?;

        let mut roots = Vec::new();
        for entry in fs::read_dir(profile_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(number) = name.strip_prefix("generation-") else {
                continue;
            };
            if number.parse::<u32>().is_err() {
                continue;
            }
            if !entry.file_type()?.is_dir() {
                return Err(StoreError::InvalidPath(format!(
                    "Profile generation '{}' is not a directory",
                    entry.path().display()
                )));
            }
            let metadata =
                fs::symlink_metadata(entry.path().join("manifest")).map_err(|error| {
                    StoreError::InvalidPath(format!(
                        "Failed to inspect generation manifest '{}': {error}",
                        entry.path().display()
                    ))
                })?;
            if !metadata.file_type().is_file() {
                return Err(StoreError::InvalidPath(format!(
                    "Generation manifest '{}' is not a regular file",
                    entry.path().display()
                )));
            }
            let manifest = entry.path().join("manifest");
            let content = fs::read_to_string(&manifest)?;
            for package in content.lines().filter(|line| !line.is_empty()) {
                let path = Path::new(package);
                if StorePath::parse(path).is_some() {
                    roots.push(self.parse_store_root(path)?);
                } else {
                    self.validate_untracked_package(path)?;
                    let (_, references) = self.store.scan_path_references(path, None)?;
                    roots.extend(references);
                }
            }
        }
        Ok(roots)
    }

    fn validate_untracked_package(&self, path: &Path) -> Result<(), StoreError> {
        let parent = path.parent().ok_or_else(|| {
            StoreError::InvalidPath(format!("Invalid profile package '{}'", path.display()))
        })?;
        let store_root = fs::canonicalize(self.store.root())?;
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            StoreError::InvalidPath(format!(
                "Failed to inspect profile package '{}': {error}",
                path.display()
            ))
        })?;
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir))
            || fs::canonicalize(parent)? != store_root
            || !metadata.file_type().is_dir()
            || fs::canonicalize(path)?.parent() != Some(store_root.as_path())
        {
            return Err(StoreError::InvalidPath(format!(
                "Invalid profile package '{}' outside store '{}'",
                path.display(),
                self.store.root().display()
            )));
        }
        Ok(())
    }

    fn validate_current_profile(&self, profile_dir: &Path) -> Result<(), StoreError> {
        let current = profile_dir.join("current");
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target = fs::read_link(&current)?;
                let target = if target.is_absolute() {
                    target
                } else {
                    profile_dir.join(target)
                };
                let name = target
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("");
                if target.parent() != Some(profile_dir)
                    || !name
                        .strip_prefix("generation-")
                        .is_some_and(|number| number.parse::<u32>().is_ok())
                    || !fs::symlink_metadata(&target)
                        .is_ok_and(|metadata| metadata.file_type().is_dir())
                {
                    return Err(StoreError::InvalidPath(format!(
                        "Invalid profile current target '{}'",
                        target.display()
                    )));
                }
            }
            Ok(_) => {
                return Err(StoreError::InvalidPath(format!(
                    "Profile current pointer '{}' is not a symlink",
                    current.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    /// Find all paths reachable from the GC roots.
    /// 查找从 GC 根可达的所有路径。
    pub fn find_live_paths(&mut self) -> Result<HashSet<StorePath>, StoreError> {
        let _lock = self.store.lock_profiles()?;
        self.find_live_paths_locked()
    }

    fn find_live_paths_locked(&mut self) -> Result<HashSet<StorePath>, StoreError> {
        self.validate_store_context()?;
        let roots = self.list_roots()?;
        let profile_roots = self.profile_roots()?;
        let mut live = HashSet::new();
        let mut database = Database::open(self.store.root().to_path_buf())?;

        for (_, root_path) in roots {
            self.add_reachable(&root_path, &mut live, &mut database)?;
        }
        for root_path in profile_roots {
            self.add_reachable(&root_path, &mut live, &mut database)?;
        }
        Ok(live)
    }

    /// Add a path and all its references to the live set.
    /// 将路径及其所有引用添加到存活集合。
    fn add_reachable(
        &mut self,
        path: &StorePath,
        live: &mut HashSet<StorePath>,
        database: &mut Database,
    ) -> Result<(), StoreError> {
        if live.contains(path) {
            return Ok(());
        }
        if !self.store.path_exists(path) {
            return Err(StoreError::PathNotFound(path.display_name()));
        }
        live.insert(path.clone());

        if let Some(info) = database.query(path)? {
            if info.path != *path {
                return Err(StoreError::InvalidPath(format!(
                    "Metadata for '{}' belongs to '{}'",
                    path.display_name(),
                    info.path.display_name()
                )));
            }
            for reference in info.references {
                self.add_reachable(&reference, live, database)?;
            }
        } else if !path.name().ends_with(".drv") {
            return Err(StoreError::InvalidPath(format!(
                "Cannot collect reachable store path '{}' without PathInfo metadata",
                path.display_name()
            )));
        }
        if path.name().ends_with(".drv") {
            let drv = self.store.read_derivation(path)?;
            for input_drv in drv.input_drvs.keys() {
                self.add_reachable(input_drv, live, database)?;
            }
            for input_src in &drv.input_srcs {
                self.add_reachable(input_src, live, database)?;
            }
        }
        Ok(())
    }

    /// Collect garbage and return the number of paths deleted.
    /// 收集垃圾并返回删除的路径数量。
    pub fn collect(&mut self) -> Result<GcResult, StoreError> {
        let _lock = self.store.lock_profiles()?;
        let live = self.find_live_paths_locked()?;
        let all_paths = self.store.list_paths()?;

        let mut deleted = 0;
        let mut freed_bytes = 0u64;

        for path in all_paths {
            if !live.contains(&path) {
                let fs_path = self.store.to_path(&path);
                if let Ok(size) = dir_size(&fs_path) {
                    freed_bytes += size;
                }
                self.store.delete(&path)?;
                deleted += 1;
            }
        }

        Ok(GcResult {
            deleted,
            freed_bytes,
        })
    }

    /// Dry-run garbage collection and return what would be deleted.
    /// 干运行垃圾回收并返回将被删除的内容。
    pub fn dry_run(&mut self) -> Result<Vec<StorePath>, StoreError> {
        let _lock = self.store.lock_profiles()?;
        let live = self.find_live_paths_locked()?;
        let all_paths = self.store.list_paths()?;

        Ok(all_paths
            .into_iter()
            .filter(|p| !live.contains(p))
            .collect())
    }
}

/// Result of garbage collection.
/// 垃圾回收的结果。
#[derive(Debug, Clone)]
pub struct GcResult {
    /// Number of paths deleted. / 删除的路径数量。
    pub deleted: usize,
    /// Total bytes freed. / 释放的总字节数。
    pub freed_bytes: u64,
}

impl GcResult {
    /// Format freed bytes as a human-readable string.
    /// 将释放的字节格式化为人类可读的字符串。
    pub fn freed_human(&self) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if self.freed_bytes >= GB {
            format!("{:.2} GiB", self.freed_bytes as f64 / GB as f64)
        } else if self.freed_bytes >= MB {
            format!("{:.2} MiB", self.freed_bytes as f64 / MB as f64)
        } else if self.freed_bytes >= KB {
            format!("{:.2} KiB", self.freed_bytes as f64 / KB as f64)
        } else {
            format!("{} B", self.freed_bytes)
        }
    }
}

/// Calculate bytes stored in regular files and symlinks without following links.
/// 计算普通文件与符号链接占用的字节数，且不跟随链接。
fn dir_size(path: &Path) -> Result<u64, StoreError> {
    let mut size = 0u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(current) = pending.pop() {
        let metadata = fs::symlink_metadata(&current)?;
        if metadata.is_file() || metadata.file_type().is_symlink() {
            size += metadata.len();
        } else if metadata.is_dir() {
            for entry in fs::read_dir(&current)? {
                pending.push(entry?.path());
            }
        }
    }
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PathInfo;
    use n3v3_derive::Hash;

    #[test]
    fn dry_run_retained_generations_and_reference_closure_survive() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("store")).unwrap();
        let profile = temp.path().join("home/.n3v3/profile");
        let first = store.add_content(b"first", "first").unwrap();
        let second = store.add_content(b"second", "second").unwrap();
        let referenced = store.add_content(b"reference", "reference").unwrap();
        let garbage = store.add_content(b"garbage", "garbage").unwrap();
        let mut info = PathInfo::new(first.clone(), Hash::of(b"first"), 5);
        info.add_reference(referenced.clone());
        let mut database = Database::open(store.root().to_path_buf()).unwrap();
        database.register(info).unwrap();
        database
            .register(PathInfo::new(second.clone(), Hash::of(b"second"), 6))
            .unwrap();
        database
            .register(PathInfo::new(referenced.clone(), Hash::of(b"reference"), 9))
            .unwrap();
        for (number, path) in [(1, &first), (3, &second)] {
            let generation = profile.join(format!("generation-{number}"));
            fs::create_dir_all(&generation).unwrap();
            fs::write(
                generation.join("manifest"),
                format!("{}\n", store.to_path(path).display()),
            )
            .unwrap();
        }
        let plain_package = store.root().join("plain-pkg");
        fs::create_dir_all(&plain_package).unwrap();
        fs::write(
            profile.join("generation-1/manifest"),
            format!(
                "{}\n{}\n",
                store.to_path(&first).display(),
                plain_package.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(profile.join("generation-3"), profile.join("current")).unwrap();

        let mut gc = GarbageCollector::with_profile_dir(&mut store, profile);
        let candidates = gc.dry_run().unwrap();
        assert_eq!(candidates, vec![garbage]);
        assert!(store.path_exists(&first));
        assert!(store.path_exists(&referenced));
        assert!(plain_package.is_dir());
    }

    #[test]
    fn collect_same_content_different_named_roots_retains_both() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("store")).unwrap();
        let first = store.add_content(b"shared", "a").unwrap();
        let second = store.add_content(b"shared", "b").unwrap();
        let garbage = store.add_content(b"garbage", "garbage").unwrap();
        {
            let gc = GarbageCollector::new(&mut store);
            gc.add_root("first", &first).unwrap();
            gc.add_root("second", &second).unwrap();
        }

        let result = GarbageCollector::new(&mut store).collect().unwrap();
        assert_eq!(result.deleted, 1);
        assert!(store.path_exists(&first));
        assert!(store.path_exists(&second));
        assert!(!store.path_exists(&garbage));
    }

    #[cfg(unix)]
    #[test]
    fn collect_plain_package_symlink_and_content_retain_managed_closure() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("store")).unwrap();
        let dependency = store.add_content(b"dependency", "dep@1").unwrap();
        let transitive = store.add_content(b"transitive", "transitive").unwrap();
        let content = store.add_content(b"content", "content@2").unwrap();
        let garbage = store.add_content(b"garbage", "garbage").unwrap();
        let mut db = Database::open(store.root().to_path_buf()).unwrap();
        let mut info = db.query(&dependency).unwrap().unwrap();
        info.add_reference(transitive.clone());
        db.register(info).unwrap();

        let plain = store.root().join("plain-pkg");
        fs::create_dir(&plain).unwrap();
        symlink(store.to_path(&dependency), plain.join("linked")).unwrap();
        fs::write(
            plain.join("contents"),
            store.to_path(&content).to_string_lossy().as_bytes(),
        )
        .unwrap();
        let profile = temp.path().join("home/.n3v3/profile");
        fs::create_dir_all(profile.join("generation-1")).unwrap();
        fs::write(
            profile.join("generation-1/manifest"),
            format!("{}\n", plain.display()),
        )
        .unwrap();

        let result = GarbageCollector::with_profile_dir(&mut store, profile)
            .collect()
            .unwrap();
        assert_eq!(result.deleted, 1);
        assert!(store.path_exists(&dependency));
        assert!(store.path_exists(&transitive));
        assert!(store.path_exists(&content));
        assert!(!store.path_exists(&garbage));
    }

    #[cfg(unix)]
    #[test]
    fn collect_unscannable_plain_package_aborts_before_deletion() {
        use std::os::unix::net::UnixListener;

        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("store")).unwrap();
        let garbage = store.add_content(b"garbage", "garbage").unwrap();
        let plain = store.root().join("plain-pkg");
        fs::create_dir(&plain).unwrap();
        let _socket = UnixListener::bind(plain.join("unscannable")).unwrap();
        let profile = temp.path().join("home/.n3v3/profile");
        fs::create_dir_all(profile.join("generation-1")).unwrap();
        fs::write(
            profile.join("generation-1/manifest"),
            format!("{}\n", plain.display()),
        )
        .unwrap();

        let error = GarbageCollector::with_profile_dir(&mut store, profile)
            .collect()
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Cannot scan unsupported store entry")
        );
        assert!(store.path_exists(&garbage));
    }

    #[cfg(unix)]
    #[test]
    fn collect_plain_package_external_symlink_aborts_before_deletion() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("store")).unwrap();
        let dependency = store.add_content(b"dependency", "dependency").unwrap();
        let external = temp.path().join("external");
        fs::create_dir(&external).unwrap();
        fs::write(
            external.join("reference"),
            store.to_path(&dependency).to_string_lossy().as_bytes(),
        )
        .unwrap();
        let plain = store.root().join("plain-pkg");
        fs::create_dir(&plain).unwrap();
        symlink(&external, plain.join("external")).unwrap();
        let profile = temp.path().join("home/.n3v3/profile");
        fs::create_dir_all(profile.join("generation-1")).unwrap();
        fs::write(
            profile.join("generation-1/manifest"),
            format!("{}\n", plain.display()),
        )
        .unwrap();

        let error = GarbageCollector::with_profile_dir(&mut store, profile)
            .collect()
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Cannot determine store references")
        );
        assert!(store.path_exists(&dependency));
    }

    #[test]
    fn dry_run_corrupt_manifest_or_root_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("store")).unwrap();
        let profile = temp.path().join("home/.n3v3/profile");
        let generation = profile.join("generation-1");
        fs::create_dir_all(&generation).unwrap();
        let package = store.add_content(b"package", "package").unwrap();
        fs::write(generation.join("manifest"), "/outside-store/package\n").unwrap();
        {
            let mut gc = GarbageCollector::with_profile_dir(&mut store, profile.clone());
            assert!(gc.dry_run().is_err());
        }
        fs::write(
            generation.join("manifest"),
            format!("{}\n", store.to_path(&package).display()),
        )
        .unwrap();
        fs::create_dir_all(store.root().join("gcroots")).unwrap();
        fs::write(
            store.root().join("gcroots/corrupt"),
            "/outside-store/package",
        )
        .unwrap();
        let mut gc = GarbageCollector::with_profile_dir(&mut store, profile);
        assert!(gc.dry_run().is_err());
        assert!(store.path_exists(&package));
    }

    #[test]
    fn collect_two_registered_homes_preserves_every_generation() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("store")).unwrap();
        let first = store.add_content(b"first", "first").unwrap();
        let second = store.add_content(b"second", "second").unwrap();
        let third = store.add_content(b"third", "third").unwrap();
        let garbage = store.add_content(b"garbage", "garbage").unwrap();
        let mut database = Database::open(store.root().to_path_buf()).unwrap();
        for (path, contents) in [
            (&first, b"first".as_slice()),
            (&second, b"second".as_slice()),
            (&third, b"third".as_slice()),
        ] {
            database
                .register(PathInfo::new(
                    path.clone(),
                    Hash::of(contents),
                    contents.len() as u64,
                ))
                .unwrap();
        }
        for (home, generations) in [
            ("home-a", vec![(1, &first), (2, &second)]),
            ("home-b", vec![(1, &third)]),
        ] {
            let profile = temp.path().join(home).join(".n3v3/profile");
            for (number, package) in generations {
                let generation = profile.join(format!("generation-{number}"));
                fs::create_dir_all(&generation).unwrap();
                fs::write(
                    generation.join("manifest"),
                    format!("{}\n", store.to_path(package).display()),
                )
                .unwrap();
            }
            let _lock = store.lock_profiles().unwrap();
            store.register_profile(&profile).unwrap();
        }
        let mut gc = GarbageCollector::new(&mut store);
        let result = gc.collect().unwrap();
        assert_eq!(result.deleted, 1);
        assert!(store.path_exists(&first));
        assert!(store.path_exists(&second));
        assert!(store.path_exists(&third));
        assert!(!store.path_exists(&garbage));
    }

    #[cfg(unix)]
    #[test]
    fn collect_unreferenced_directory_with_cyclic_and_outward_symlinks_counts_local_bytes() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("store")).unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"outside contents are not freed").unwrap();

        let garbage = StorePath::new(Hash::of(b"unused-directory"), "unused-directory".into());
        let garbage_dir = store.to_path(&garbage);
        fs::create_dir(&garbage_dir).unwrap();
        fs::write(garbage_dir.join("local"), b"local").unwrap();
        let cycle = garbage_dir.join("cycle");
        let outward = garbage_dir.join("outward");
        symlink(".", &cycle).unwrap();
        symlink(&outside, &outward).unwrap();
        let expected = b"local".len() as u64
            + fs::symlink_metadata(&cycle).unwrap().len()
            + fs::symlink_metadata(&outward).unwrap().len();

        let result = GarbageCollector::new(&mut store).collect().unwrap();
        assert_eq!(result.deleted, 1);
        assert_eq!(result.freed_bytes, expected);
        assert!(!garbage_dir.exists());
        assert_eq!(
            fs::read(outside.join("sentinel")).unwrap(),
            b"outside contents are not freed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn collect_profile_root_without_path_info_aborts_before_deleting_dependencies() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("store")).unwrap();
        let dependency = store.add_content(b"dependency", "dependency").unwrap();
        let garbage = store.add_content(b"garbage", "garbage").unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        symlink(store.to_path(&dependency), source.join("dependency")).unwrap();
        let package = store.add_dir(&source, "package").unwrap();
        let mut database = Database::open(store.root().to_path_buf()).unwrap();
        let package_info = database.query(&package).unwrap().unwrap();
        assert!(package_info.references.contains(&dependency));
        fs::remove_file(database.info_path(&package)).unwrap();
        let profile = temp.path().join("home/.n3v3/profile");
        fs::create_dir_all(profile.join("generation-1")).unwrap();
        fs::write(
            profile.join("generation-1/manifest"),
            format!("{}\n", store.to_path(&package).display()),
        )
        .unwrap();

        let error = GarbageCollector::with_profile_dir(&mut store, profile)
            .collect()
            .unwrap_err();
        assert!(
            error.to_string().contains("without PathInfo metadata"),
            "{error}"
        );
        assert!(store.path_exists(&package));
        assert!(store.path_exists(&dependency));
        assert!(store.path_exists(&garbage));
    }

    #[test]
    fn collect_malformed_registered_profile_aborts_without_deleting() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("store")).unwrap();
        let garbage = store.add_content(b"garbage", "garbage").unwrap();
        let outside = temp.path().join("private");
        fs::write(&outside, "do not read or delete").unwrap();
        let registry = store.root().join(".profiles");
        fs::create_dir(&registry).unwrap();
        let content = outside.to_str().unwrap();
        fs::write(
            registry.join(Hash::of(content.as_bytes()).to_string()),
            content,
        )
        .unwrap();

        let error = GarbageCollector::new(&mut store).collect().unwrap_err();
        assert!(
            error.to_string().contains("Invalid profile root"),
            "{error}"
        );
        assert!(store.path_exists(&garbage));
        assert_eq!(
            fs::read_to_string(outside).unwrap(),
            "do not read or delete"
        );
    }

    #[cfg(unix)]
    #[test]
    fn dry_run_alias_manifest_aborts_instead_of_losing_real_package() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("store")).unwrap();
        let real = store.add_content(b"real", "real").unwrap();
        let alias = store.root().join(format!("{}-alias", Hash::of(b"alias")));
        std::os::unix::fs::symlink(store.to_path(&real), &alias).unwrap();
        let profile = temp.path().join("home/.n3v3/profile");
        let generation = profile.join("generation-1");
        fs::create_dir_all(&generation).unwrap();
        fs::write(
            generation.join("manifest"),
            format!("{}\n", alias.display()),
        )
        .unwrap();
        let mut gc = GarbageCollector::with_profile_dir(&mut store, profile);
        assert!(gc.collect().is_err());
        assert!(store.path_exists(&real));
    }
}
