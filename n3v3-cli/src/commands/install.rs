//! The `n3v3 install` command.
//! `n3v3 install` 命令。
//!
//! Installs packages into the user environment.
//! 将软件包安装到用户环境。

use crate::output;
use n3v3_store::Store;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Install a package to the user environment.
/// 将软件包安装到用户环境。
pub fn run(package: &str) -> Result<(), String> {
    install_at(package, &get_store_dir(), &get_profile_dir()?)
}

fn install_at(package: &str, store_dir: &PathBuf, profile_dir: &PathBuf) -> Result<(), String> {
    if !store_dir.is_absolute() {
        return Err(format!(
            "Store root '{}' must be absolute",
            store_dir.display()
        ));
    }
    let store = Store::open_at(store_dir.clone())
        .map_err(|error| format!("Failed to open store '{}': {error}", store_dir.display()))?;
    let _lock = store
        .lock_profiles()
        .map_err(|error| format!("Failed to lock store '{}': {error}", store_dir.display()))?;
    let package_path = find_package(store_dir, package)?;
    validate_store_package(store_dir, &package_path)?;

    fs::create_dir_all(profile_dir).map_err(|e| {
        format!(
            "Failed to create profile directory '{}': {e}",
            profile_dir.display()
        )
    })?;
    store.register_profile(profile_dir).map_err(|error| {
        format!(
            "Failed to register profile '{}': {error}",
            profile_dir.display()
        )
    })?;
    let current_link = profile_dir.join("current");
    let mut packages = load_current_packages(profile_dir, store_dir)?;

    if packages.contains(&package_path) {
        output::info(&format!("Package '{package}' is already installed"));
        return Ok(());
    }

    packages.push(package_path.clone());
    let generation = get_next_generation(profile_dir)?;
    let gen_dir = create_profile_generation(profile_dir, store_dir, generation, &packages)?;
    if let Err(error) = replace_current_link_atomically(&current_link, &gen_dir) {
        return Err(clean_failed_generation(&gen_dir, error));
    }

    output::success(&format!("Installed '{package}' to generation {generation}"));
    println!("  {package} -> {}", package_path.display());
    Ok(())
}

pub(super) fn load_current_packages(
    profile_dir: &Path,
    store_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    let current_link = profile_dir.join("current");
    let metadata = match fs::symlink_metadata(&current_link) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Failed to inspect current link: {error}")),
    };
    if !metadata.file_type().is_symlink() {
        return Err(format!(
            "Invalid current profile pointer '{}': expected a symlink",
            current_link.display()
        ));
    }

    let target =
        fs::read_link(&current_link).map_err(|e| format!("Failed to read current link: {e}"))?;
    let generation_dir = resolve_link_target(profile_dir, &target);
    validate_generation(profile_dir, store_dir, &generation_dir)?;
    read_manifest_packages(&generation_dir, store_dir)
}

pub(super) fn create_profile_generation(
    profile_dir: &Path,
    store_dir: &Path,
    generation: u32,
    packages: &[PathBuf],
) -> Result<PathBuf, String> {
    let generation_dir = profile_dir.join(format!("generation-{generation}"));
    fs::create_dir(&generation_dir)
        .map_err(|e| format!("Failed to reserve generation {generation}: {e}"))?;
    if let Err(error) = populate_generation(&generation_dir, store_dir, packages) {
        return Err(clean_failed_generation(&generation_dir, error));
    }
    Ok(generation_dir)
}

fn populate_generation(
    generation_dir: &Path,
    store_dir: &Path,
    packages: &[PathBuf],
) -> Result<(), String> {
    for package in packages {
        validate_store_package(store_dir, package)?;
    }
    let manifest = packages
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(generation_dir.join("manifest"), format!("{manifest}\n"))
        .map_err(|e| format!("Failed to write generation manifest: {e}"))?;
    create_bin_links(generation_dir, packages)
}

fn create_bin_links(generation_dir: &Path, packages: &[PathBuf]) -> Result<(), String> {
    let links = collect_bin_links(packages)?;
    let bin_dir = generation_dir.join("bin");
    fs::create_dir(&bin_dir).map_err(|e| format!("Failed to create bin directory: {e}"))?;
    for (name, target) in links {
        let destination = bin_dir.join(name);
        symlink(&target, &destination).map_err(|e| {
            format!(
                "Failed to create binary link '{}': {e}",
                destination.display()
            )
        })?;
    }
    Ok(())
}

fn collect_bin_links(
    packages: &[PathBuf],
) -> Result<BTreeMap<std::ffi::OsString, PathBuf>, String> {
    let mut links = BTreeMap::new();
    for package in packages {
        let package_bin = package.join("bin");
        match fs::symlink_metadata(&package_bin) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(format!(
                    "Package bin '{}' is not a directory",
                    package_bin.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to inspect package bin '{}': {error}",
                    package_bin.display()
                ));
            }
        }
        for entry in fs::read_dir(&package_bin).map_err(|e| {
            format!(
                "Failed to read package bin '{}': {e}",
                package_bin.display()
            )
        })? {
            let entry = entry.map_err(|e| format!("Failed to read package bin entry: {e}"))?;
            let name = entry.file_name();
            if links.insert(name.clone(), entry.path()).is_some() {
                return Err(format!(
                    "Binary '{}' is provided by multiple packages",
                    name.to_string_lossy()
                ));
            }
        }
    }
    Ok(links)
}

pub(super) fn read_manifest_packages(
    generation_dir: &Path,
    store_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    let manifest_path = generation_dir.join("manifest");
    let metadata = fs::symlink_metadata(&manifest_path).map_err(|error| {
        format!(
            "Failed to inspect manifest '{}': {error}",
            manifest_path.display()
        )
    })?;
    if !metadata.file_type().is_file() {
        return Err(format!(
            "Manifest '{}' is not a regular file",
            manifest_path.display()
        ));
    }
    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read manifest '{}': {e}", manifest_path.display()))?;
    let mut packages = Vec::new();
    for line in manifest.lines() {
        if line.is_empty() {
            continue;
        }
        let package = PathBuf::from(line);
        validate_store_package(store_dir, &package)?;
        if packages.contains(&package) {
            return Err(format!(
                "Duplicate package entry in '{}'",
                manifest_path.display()
            ));
        }
        packages.push(package);
    }
    Ok(packages)
}

pub(super) fn validate_generation(
    profile_dir: &Path,
    store_dir: &Path,
    generation_dir: &Path,
) -> Result<(), String> {
    validate_generation_location(profile_dir, generation_dir)?;
    let packages = read_manifest_packages(generation_dir, store_dir)?;
    let expected_links = collect_bin_links(&packages)?;
    validate_generation_bin_links(generation_dir, &expected_links)
}

fn validate_generation_bin_links(
    generation_dir: &Path,
    expected_links: &BTreeMap<std::ffi::OsString, PathBuf>,
) -> Result<(), String> {
    let bin_dir = generation_dir.join("bin");
    let metadata = fs::symlink_metadata(&bin_dir).map_err(|e| {
        format!(
            "Failed to inspect generation bin '{}': {e}",
            bin_dir.display()
        )
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!(
            "Generation bin '{}' is not a directory",
            bin_dir.display()
        ));
    }
    let mut remaining = expected_links.clone();
    for entry in fs::read_dir(&bin_dir)
        .map_err(|e| format!("Failed to read generation bin '{}': {e}", bin_dir.display()))?
    {
        let entry = entry.map_err(|e| format!("Failed to read generation bin entry: {e}"))?;
        let entry_path = entry.path();
        let expected = remaining
            .remove(&entry.file_name())
            .ok_or_else(|| format!("Unexpected generation binary '{}'", entry_path.display()))?;
        let target = fs::read_link(&entry_path).map_err(|e| {
            format!(
                "Invalid generation binary link '{}': {e}",
                entry_path.display()
            )
        })?;
        if target != expected {
            return Err(format!(
                "Generation binary link '{}' targets '{}' instead of '{}'",
                entry_path.display(),
                target.display(),
                expected.display()
            ));
        }
    }
    if !remaining.is_empty() {
        return Err("Generation is missing one or more binary links".to_string());
    }
    Ok(())
}

fn validate_generation_location(profile_dir: &Path, generation_dir: &Path) -> Result<(), String> {
    let name = generation_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Invalid current generation path".to_string())?;
    let is_generation = name
        .strip_prefix("generation-")
        .is_some_and(|number| number.parse::<u32>().is_ok());
    if generation_dir.parent() != Some(profile_dir) || !is_generation {
        return Err(format!(
            "Invalid profile generation target '{}'",
            generation_dir.display()
        ));
    }
    let metadata = fs::symlink_metadata(generation_dir).map_err(|e| {
        format!(
            "Failed to inspect generation '{}': {e}",
            generation_dir.display()
        )
    })?;
    if !metadata.file_type().is_dir() {
        return Err(format!(
            "Profile generation '{}' is not a directory",
            generation_dir.display()
        ));
    }
    Ok(())
}

fn validate_store_package(store_dir: &Path, package: &Path) -> Result<(), String> {
    if package
        .components()
        .any(|part| matches!(part, Component::ParentDir))
        || package.parent() != Some(store_dir)
    {
        return Err(format!(
            "Manifest path '{}' is outside the store",
            package.display()
        ));
    }
    let metadata = fs::symlink_metadata(package).map_err(|e| {
        format!(
            "Failed to inspect store package '{}': {e}",
            package.display()
        )
    })?;
    if !metadata.file_type().is_dir() {
        return Err(format!(
            "Manifest package '{}' must be a real store directory",
            package.display()
        ));
    }
    let canonical_store = fs::canonicalize(store_dir)
        .map_err(|e| format!("Failed to resolve store '{}': {e}", store_dir.display()))?;
    let canonical_package = fs::canonicalize(package)
        .map_err(|e| format!("Invalid manifest package '{}': {e}", package.display()))?;
    if canonical_package.parent() != Some(canonical_store.as_path()) || !canonical_package.is_dir()
    {
        return Err(format!(
            "Manifest package '{}' is not a store directory",
            package.display()
        ));
    }
    Ok(())
}

fn resolve_link_target(parent: &Path, target: &Path) -> PathBuf {
    if target.is_absolute() {
        target.to_path_buf()
    } else {
        parent.join(target)
    }
}

pub(super) fn clean_failed_generation(generation_dir: &Path, error: String) -> String {
    match fs::remove_dir_all(generation_dir) {
        Ok(()) => error,
        Err(cleanup_error) => format!(
            "{error}; failed to remove unpublished generation '{}': {cleanup_error}",
            generation_dir.display()
        ),
    }
}

fn unique_nonce() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{timestamp}", std::process::id())
}

/// Get the store directory.
/// 获取存储目录。
pub(super) fn get_store_dir() -> PathBuf {
    std::env::var_os("N3V3_STORE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/n3v3/store"))
}

/// Resolve the profile under HOME; never write to an arbitrary fallback directory.
pub(super) fn get_profile_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .ok_or_else(|| "HOME must be set to locate the profile".to_string())?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        return Err(format!("HOME '{}' must be absolute", home.display()));
    }
    Ok(home.join(".n3v3").join("profile"))
}

/// Find a package in the store, with registry fallback.
/// 在存储中查找软件包，带注册表回退。
fn find_package(store_dir: &PathBuf, package: &str) -> Result<PathBuf, String> {
    // Direct path
    let direct = store_dir.join(package);
    if direct.exists() {
        return Ok(direct);
    }

    // Search for matching packages
    if store_dir.exists() {
        let mut exact_matches = Vec::new();
        let mut fuzzy_matches = Vec::new();

        for entry in fs::read_dir(store_dir).map_err(|e| format!("Failed to read store: {e}"))? {
            let entry = entry.map_err(|e| format!("Failed to read entry: {e}"))?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let logical_name = logical_store_name(&name_str);

            if logical_name == package || name_str == package {
                exact_matches.push(entry.path());
                continue;
            }

            if logical_name
                .strip_prefix(package)
                .is_some_and(|rest| rest.starts_with('-'))
            {
                fuzzy_matches.push(entry.path());
            }
        }

        if exact_matches.len() == 1 {
            return Ok(exact_matches.remove(0));
        }
        if exact_matches.len() > 1 {
            let candidates = exact_matches
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "Package '{}' is ambiguous. Please use a full store path name. Matches: {}",
                package, candidates
            ));
        }

        if fuzzy_matches.len() == 1 {
            return Ok(fuzzy_matches.remove(0));
        }
        if fuzzy_matches.len() > 1 {
            let candidates = fuzzy_matches
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "Package '{}' matches multiple versions. Please specify a more exact name: {}",
                package, candidates
            ));
        }
    }

    // Query remote registry for available versions
    if let Some(client) = crate::registry_client::RegistryClient::from_env() {
        match client.get_package(package) {
            Ok(pkg) => {
                let versions: Vec<String> =
                    pkg.versions.iter().map(|v| v.version.clone()).collect();
                if versions.is_empty() {
                    return Err(format!(
                        "Package '{package}' not found in store or registry"
                    ));
                }
                return Err(format!(
                    "Package '{package}' not found in local store.\n\
                     Available versions from registry: {}\n\
                     To install: fetch and add to local store first.",
                    versions.join(", ")
                ));
            }
            Err(e) => {
                return Err(format!(
                    "Package '{package}' not found in store (registry query failed: {e})"
                ));
            }
        }
    }

    Err(format!(
        "Package '{package}' not found in store. Set N3V3_REGISTRY to query a remote registry."
    ))
}

/// Extract logical store name from a store entry (strip leading hash prefix when present).
/// 从 store 条目提取逻辑名称（存在时去掉前导哈希前缀）。
fn logical_store_name(entry: &str) -> &str {
    if let Some((prefix, rest)) = entry.split_once('-')
        && (prefix.len() == 64 || prefix.len() == 32)
        && prefix.bytes().all(|b| b.is_ascii_hexdigit())
    {
        rest
    } else {
        entry
    }
}

/// Atomically replace the current generation symlink.
/// 原子替换当前代符号链接。
pub(super) fn replace_current_link_atomically(
    link_path: &PathBuf,
    target: &PathBuf,
) -> Result<(), String> {
    if link_path.is_dir() && !link_path.is_symlink() {
        return Err(format!(
            "Failed to update current link: path is a directory: {}",
            link_path.display()
        ));
    }

    let parent = link_path.parent().ok_or_else(|| {
        format!(
            "Failed to update current link: no parent for {}",
            link_path.display()
        )
    })?;
    fs::create_dir_all(parent).map_err(|e| {
        format!(
            "Failed to create profile directory '{}': {}",
            parent.display(),
            e
        )
    })?;

    let temp_link = parent.join(format!(".current.tmp-{}", unique_nonce()));

    symlink(target, &temp_link).map_err(|e| {
        format!(
            "Failed to create temporary current link '{}': {}",
            temp_link.display(),
            e
        )
    })?;

    fs::rename(&temp_link, link_path).map_err(|e| {
        let _ = fs::remove_file(&temp_link);
        format!(
            "Failed to atomically replace current link '{}': {}",
            link_path.display(),
            e
        )
    })
}

/// Get the next generation number.
/// 获取下一个代编号。
pub(super) fn get_next_generation(profile_dir: &Path) -> Result<u32, String> {
    let mut max_gen = 0;

    if profile_dir.exists() {
        for entry in
            fs::read_dir(profile_dir).map_err(|e| format!("Failed to read profile: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if let Some(num_str) = name_str.strip_prefix("generation-")
                && let Ok(num) = num_str.parse::<u32>()
            {
                max_gen = max_gen.max(num);
            }
        }
    }

    max_gen
        .checked_add(1)
        .ok_or_else(|| "Profile generation number exhausted".to_string())
}

/// List installed packages.
/// 列出已安装的软件包。
pub fn list() -> Result<(), String> {
    let profile_dir = get_profile_dir()?;
    let packages = load_current_packages(&profile_dir, &get_store_dir())?;
    if packages.is_empty() {
        output::info("No packages installed");
        return Ok(());
    }
    output::header("Installed Packages");
    let mut table = output::Table::new(vec!["#", "Package"]);
    for (index, package) in packages.iter().enumerate() {
        let name = package
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| package.display().to_string());
        table.add_row(vec![&(index + 1).to_string(), &name]);
    }
    table.print();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_logical_store_name_with_hash_prefix() {
        let entry = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef-hello-1.0";
        assert_eq!(logical_store_name(entry), "hello-1.0");
    }

    #[test]
    fn test_logical_store_name_without_hash_prefix() {
        let entry = "hello-1.0";
        assert_eq!(logical_store_name(entry), "hello-1.0");
    }

    #[cfg(unix)]
    #[test]
    fn test_replace_current_link_atomically() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_root = std::env::temp_dir().canonicalize().unwrap();
        let root = temp_root.join(format!(
            "n3v3-install-link-test-{}-{}",
            std::process::id(),
            nonce
        ));
        let target1 = root.join("generation-1");
        let target2 = root.join("generation-2");
        let current = root.join("current");

        fs::create_dir_all(&target1).unwrap();
        fs::create_dir_all(&target2).unwrap();

        replace_current_link_atomically(&current, &target1).unwrap();
        assert_eq!(fs::read_link(&current).unwrap(), target1);

        replace_current_link_atomically(&current, &target2).unwrap();
        assert_eq!(fs::read_link(&current).unwrap(), target2);

        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn install_two_packages_keeps_both_commands_and_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store = root.join("store");
        let profile = root.join("home/.n3v3/profile");
        for name in ["pkg-a", "pkg-b"] {
            let bin = store.join(name).join("bin");
            fs::create_dir_all(&bin).unwrap();
            fs::write(bin.join(name), format!("{name}\n")).unwrap();
        }
        install_at("pkg-a", &store, &profile).unwrap();
        install_at("pkg-b", &store, &profile).unwrap();
        let generation = fs::read_link(profile.join("current")).unwrap();
        let packages = load_current_packages(&profile, &store).unwrap();
        assert_eq!(packages, vec![store.join("pkg-a"), store.join("pkg-b")]);
        for name in ["pkg-a", "pkg-b"] {
            let link = generation.join("bin").join(name);
            assert_eq!(
                fs::read_link(&link).unwrap(),
                store.join(name).join("bin").join(name)
            );
            assert_eq!(fs::read_to_string(link).unwrap(), format!("{name}\n"));
        }
    }

    #[test]
    fn install_binary_collision_preserves_current_and_no_partial_generation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store = root.join("store");
        let profile = root.join("home/.n3v3/profile");
        for name in ["pkg-a", "pkg-b"] {
            let bin = store.join(name).join("bin");
            fs::create_dir_all(&bin).unwrap();
            fs::write(bin.join("shared"), name).unwrap();
        }
        install_at("pkg-a", &store, &profile).unwrap();
        let current = fs::read_link(profile.join("current")).unwrap();
        assert!(
            install_at("pkg-b", &store, &profile)
                .unwrap_err()
                .contains("multiple packages")
        );
        assert_eq!(fs::read_link(profile.join("current")).unwrap(), current);
        assert!(!profile.join("generation-2").exists());
        assert_eq!(
            fs::read_to_string(current.join("bin/shared")).unwrap(),
            "pkg-a"
        );
    }

    #[test]
    fn install_corrupt_current_manifest_fails_before_generation_creation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store = root.join("store");
        let profile = root.join("home/.n3v3/profile");
        fs::create_dir_all(store.join("pkg-a")).unwrap();
        install_at("pkg-a", &store, &profile).unwrap();
        let current = fs::read_link(profile.join("current")).unwrap();
        fs::write(current.join("manifest"), "/outside-store/pkg\n").unwrap();
        assert!(install_at("pkg-a", &store, &profile).is_err());
        assert_eq!(fs::read_link(profile.join("current")).unwrap(), current);
        assert!(!profile.join("generation-2").exists());
    }

    #[test]
    fn create_profile_generation_existing_number_preserves_contents() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store = root.join("store");
        let profile = root.join("home/.n3v3/profile");
        let existing = profile.join("generation-1");
        fs::create_dir_all(&existing).unwrap();
        fs::write(existing.join("user-data"), "keep").unwrap();

        assert!(create_profile_generation(&profile, &store, 1, &[]).is_err());
        assert_eq!(
            fs::read_to_string(existing.join("user-data")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn validate_generation_unexpected_binary_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store = root.join("store");
        let profile = root.join("home/.n3v3/profile");
        fs::create_dir_all(&profile).unwrap();
        let generation = create_profile_generation(&profile, &store, 1, &[]).unwrap();
        symlink("/outside/store", generation.join("bin/unexpected")).unwrap();

        assert!(validate_generation(&profile, &store, &generation).is_err());
    }

    #[test]
    fn install_store_alias_rejected_before_generation_publication() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store_dir = root.join("store");
        let profile = root.join("home/.n3v3/profile");
        let store = Store::open_at(store_dir.clone()).unwrap();
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("data"), "package").unwrap();
        let package = store.add_dir(&source, "real").unwrap();
        let real_path = store.to_path(&package);
        symlink(&real_path, store_dir.join("alias")).unwrap();

        let error = install_at("alias", &store_dir, &profile).unwrap_err();
        assert!(error.contains("real store directory"), "{error}");
        assert!(!profile.join("current").exists());
        let mut gc_store = Store::open_at(store_dir).unwrap();
        let mut gc = n3v3_store::GarbageCollector::new(&mut gc_store);
        assert!(gc.dry_run().unwrap().contains(&package));
        assert!(real_path.exists());
    }

    #[test]
    fn install_waits_for_gc_lock_before_publishing_generation() {
        use std::sync::mpsc;
        use std::time::Duration;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store_dir = root.join("store");
        let profile = root.join("home/.n3v3/profile");
        fs::create_dir_all(store_dir.join("package")).unwrap();
        let store = Store::open_at(store_dir.clone()).unwrap();
        let lock = store.lock_profiles().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let worker_store = store_dir.clone();
        let worker_profile = profile.clone();
        let worker = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            finished_tx
                .send(install_at("package", &worker_store, &worker_profile))
                .unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            finished_rx
                .recv_timeout(Duration::from_millis(100))
                .is_err()
        );
        assert!(!profile.join("current").exists());
        drop(lock);
        finished_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        worker.join().unwrap();
        assert_eq!(
            load_current_packages(&profile, &store_dir).unwrap(),
            vec![store_dir.join("package")]
        );
    }

    #[test]
    fn install_two_homes_gc_retains_both_profiles_and_old_generations() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store_dir = root.join("store");
        let mut store = Store::open_at(store_dir.clone()).unwrap();
        let mut packages = Vec::new();
        for name in ["first", "second", "third"] {
            let source = root.join(name);
            fs::create_dir(&source).unwrap();
            fs::write(source.join("data"), name).unwrap();
            packages.push(store.add_dir(&source, name).unwrap());
        }
        let garbage = store.add_content(b"unused", "unused").unwrap();
        let profile_a = root.join("home-a/.n3v3/profile");
        let profile_b = root.join("home-b/.n3v3/profile");
        install_at(&packages[0].display_name(), &store_dir, &profile_a).unwrap();
        install_at(&packages[1].display_name(), &store_dir, &profile_a).unwrap();
        install_at(&packages[2].display_name(), &store_dir, &profile_b).unwrap();

        let result = n3v3_store::GarbageCollector::new(&mut store)
            .collect()
            .unwrap();
        assert_eq!(result.deleted, 1);
        assert!(!store.path_exists(&garbage));
        for package in &packages {
            assert!(
                store.path_exists(package),
                "GC removed {}",
                package.display_name()
            );
        }
        assert_eq!(
            fs::read_to_string(profile_a.join("generation-1/manifest")).unwrap(),
            format!("{}\n", store.to_path(&packages[0]).display())
        );
    }
}
