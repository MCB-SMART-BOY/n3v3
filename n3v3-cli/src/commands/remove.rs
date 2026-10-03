//! The `n3v3 remove` command.
//! `n3v3 remove` 命令。
//!
//! Removes packages from the user environment.
//! 从用户环境中移除软件包。

use crate::commands::install::{
    clean_failed_generation, create_profile_generation, get_next_generation, get_profile_dir,
    get_store_dir, load_current_packages, replace_current_link_atomically, validate_generation,
};
use crate::output;
use n3v3_store::Store;
use std::fs;
use std::path::Path;

/// Remove a package from the user environment.
/// 从用户环境中移除软件包。
pub fn run(package: &str) -> Result<(), String> {
    remove_at(package, &get_store_dir(), &get_profile_dir()?)
}

fn remove_at(package: &str, store_dir: &Path, profile_dir: &Path) -> Result<(), String> {
    if !store_dir.is_absolute() {
        return Err(format!(
            "Store root '{}' must be absolute",
            store_dir.display()
        ));
    }
    let store = Store::open_at(store_dir.to_path_buf())
        .map_err(|error| format!("Failed to open store '{}': {error}", store_dir.display()))?;
    let _lock = store
        .lock_profiles()
        .map_err(|error| format!("Failed to lock store '{}': {error}", store_dir.display()))?;
    store.register_profile(profile_dir).map_err(|error| {
        format!(
            "Failed to register profile '{}': {error}",
            profile_dir.display()
        )
    })?;
    let current_link = profile_dir.join("current");
    let packages = load_current_packages(profile_dir, store_dir)?;
    if packages.is_empty() {
        return Err("No packages installed".to_string());
    }

    let mut matches = packages
        .iter()
        .filter(|path| matches_package(path, package));
    let removed_path = matches
        .next()
        .ok_or_else(|| format!("Package '{package}' is not installed"))?;
    if matches.next().is_some() {
        return Err(format!(
            "Package '{package}' is ambiguous in current profile. Please specify a more exact name."
        ));
    }
    let retained = packages
        .iter()
        .filter(|path| *path != removed_path)
        .cloned()
        .collect::<Vec<_>>();

    let generation = get_next_generation(profile_dir)?;
    let gen_dir = create_profile_generation(profile_dir, store_dir, generation, &retained)?;
    if let Err(error) = replace_current_link_atomically(&current_link, &gen_dir) {
        return Err(clean_failed_generation(&gen_dir, error));
    }

    output::success(&format!("Removed '{package}' (generation {generation})"));
    println!("  Removed: {}", removed_path.display());
    Ok(())
}

fn matches_package(path: &Path, package: &str) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let logical_name = logical_store_name(name);
    logical_name == package
        || name == package
        || logical_name
            .strip_prefix(package)
            .is_some_and(|rest| rest.starts_with('-'))
}

/// Rollback to a previous generation.
/// 回滚到上一代。
pub fn rollback() -> Result<(), String> {
    let generation = rollback_at(&get_profile_dir()?, &get_store_dir())?;
    output::success(&format!("Rolled back to generation {generation}"));
    Ok(())
}

fn rollback_at(profile_dir: &Path, store_dir: &Path) -> Result<u32, String> {
    let store = Store::open_at(store_dir.to_path_buf())
        .map_err(|error| format!("Failed to open store '{}': {error}", store_dir.display()))?;
    let _lock = store
        .lock_profiles()
        .map_err(|error| format!("Failed to lock store '{}': {error}", store_dir.display()))?;
    store.register_profile(profile_dir).map_err(|error| {
        format!(
            "Failed to register profile '{}': {error}",
            profile_dir.display()
        )
    })?;
    let current_link = profile_dir.join("current");
    load_current_packages(profile_dir, store_dir)?;
    let current_target = fs::read_link(&current_link)
        .map_err(|e| format!("No current generation to rollback: {e}"))?;
    let current_name = current_target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Invalid current generation".to_string())?;
    let current_num: u32 = current_name
        .strip_prefix("generation-")
        .and_then(|number| number.parse().ok())
        .ok_or_else(|| "Invalid generation number".to_string())?;

    for previous in (1..current_num).rev() {
        let generation_dir = profile_dir.join(format!("generation-{previous}"));
        if fs::symlink_metadata(&generation_dir).is_err() {
            continue;
        }
        if validate_generation(profile_dir, store_dir, &generation_dir).is_err() {
            continue;
        }
        replace_current_link_atomically(&current_link, &generation_dir)?;
        return Ok(previous);
    }
    Err("No valid previous generation to rollback to".to_string())
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
        assert_eq!(logical_store_name("hello-1.0"), "hello-1.0");
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
            "n3v3-remove-link-test-{}-{}",
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
    fn remove_and_rollback_with_generation_gap_restores_commands() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store = root.join("store");
        let profile = root.join("home/.n3v3/profile");
        for name in ["pkg-a", "pkg-b"] {
            let bin = store.join(name).join("bin");
            fs::create_dir_all(&bin).unwrap();
            fs::write(bin.join(name), name).unwrap();
        }
        fs::create_dir_all(&profile).unwrap();
        let first = create_profile_generation(&profile, &store, 1, &[store.join("pkg-a")]).unwrap();
        let second = create_profile_generation(
            &profile,
            &store,
            2,
            &[store.join("pkg-a"), store.join("pkg-b")],
        )
        .unwrap();
        replace_current_link_atomically(&profile.join("current"), &second).unwrap();
        remove_at("pkg-b", &store, &profile).unwrap();
        let third = fs::read_link(profile.join("current")).unwrap();
        assert_eq!(third.file_name().unwrap(), "generation-3");
        assert!(third.join("bin/pkg-a").exists());
        assert!(!third.join("bin/pkg-b").exists());
        fs::remove_dir_all(second).unwrap();
        assert_eq!(rollback_at(&profile, &store).unwrap(), 1);
        let restored = fs::read_link(profile.join("current")).unwrap();
        assert_eq!(restored, first);
        assert!(restored.join("bin/pkg-a").exists());
        assert!(!restored.join("bin/pkg-b").exists());
        assert_eq!(
            load_current_packages(&profile, &store).unwrap(),
            vec![store.join("pkg-a")]
        );
    }
}
