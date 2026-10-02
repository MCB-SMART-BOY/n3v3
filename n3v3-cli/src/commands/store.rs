//! The `n3v3 store` commands.
//! `n3v3 store` 命令。

use crate::commands::install::get_profile_dir;
use crate::output;
use n3v3_store::{Store, gc::GarbageCollector};
use std::fs;
use std::path::Path;

/// Run garbage collection.
/// 运行垃圾回收。
pub fn gc() -> Result<(), String> {
    let status = output::Status::new("Analyzing store for garbage collection");

    let store_result = Store::open();
    let mut store = match store_result {
        Ok(s) => s,
        Err(e) => {
            status.fail(Some("Failed to open store"));
            return Err(format!("Failed to open store: {}", e));
        }
    };

    register_profile_if_present(&store, &get_profile_dir()?)?;

    let mut gc = GarbageCollector::new(&mut store);

    // First do a dry run
    // 首先进行模拟运行
    let to_delete = gc
        .dry_run()
        .map_err(|e| format!("Failed to analyze store: {}", e))?;

    status.success(Some("Store analysis complete"));

    if to_delete.is_empty() {
        output::success("No garbage to collect.");
        return Ok(());
    }

    output::header("Garbage Collection");
    output::kv("Paths to delete", &to_delete.len().to_string());
    println!();

    for path in &to_delete {
        output::list_item(&path.display_name());
    }

    println!();

    // Confirm before deletion
    // 删除前确认
    if !output::confirm("Proceed with deletion?") {
        output::info("Garbage collection cancelled");
        return Ok(());
    }

    let delete_status = output::Status::new("Deleting garbage paths");

    let collect_result = gc.collect();
    match collect_result {
        Ok(result) => {
            delete_status.success(None);
            output::success(&format!(
                "Deleted {} paths, freed {}.",
                result.deleted,
                result.freed_human()
            ));
            Ok(())
        }
        Err(e) => {
            delete_status.fail(Some("Deletion failed"));
            Err(format!("Failed to collect garbage: {}", e))
        }
    }
}

fn register_profile_if_present(store: &Store, profile: &Path) -> Result<(), String> {
    match fs::symlink_metadata(profile) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Failed to inspect profile '{}': {error}",
                profile.display()
            ));
        }
        Ok(_) => {}
    }
    let _lock = store
        .lock_profiles()
        .map_err(|error| format!("Failed to lock store '{}': {error}", store.root().display()))?;
    store.register_profile(profile).map_err(|error| {
        format!(
            "Failed to register profile '{}': {error}",
            profile.display()
        )
    })
}

/// Show store information.
/// 显示存储信息。
pub fn info() -> Result<(), String> {
    let store = Store::open().map_err(|e| format!("Failed to open store: {}", e))?;

    let paths = store
        .list_paths()
        .map_err(|e| format!("Failed to list paths: {}", e))?;

    let size = store
        .size()
        .map_err(|e| format!("Failed to get store size: {}", e))?;

    output::header("n3v3 Store Information");
    output::kv("Location", &store.root().display().to_string());
    output::kv("Paths", &paths.len().to_string());
    output::kv("Size", &output::format_size(size));
    println!();

    if !paths.is_empty() {
        output::section("Recent paths");
        let mut table = output::Table::new(vec!["#", "Path"]);
        for (i, path) in paths.iter().take(10).enumerate() {
            table.add_row(vec![&(i + 1).to_string(), &path.display_name()]);
        }
        table.print();

        if paths.len() > 10 {
            output::info(&format!("... and {} more", paths.len() - 10));
        }
    }

    Ok(())
}
