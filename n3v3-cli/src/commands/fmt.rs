//! The `n3v3 fmt` command.
//! `n3v3 fmt` 命令。

use crate::{commands::diagnostics, output};
use std::fs;
use std::path::Path;

fn split_shebang(source: &str) -> (&str, &str) {
    if !source.starts_with("#!") {
        return ("", source);
    }
    match source.find('\n') {
        Some(newline) => source.split_at(newline + 1),
        None => (source, ""),
    }
}

fn format_contents(raw: &str) -> Result<String, n3v3_fmt::FormatError> {
    let (shebang, source) = split_shebang(raw);
    n3v3_fmt::format(source).map(|formatted| format!("{shebang}{formatted}"))
}

/// Format an n3v3 source file.
/// 格式化 n3v3 源文件。
pub fn run(file: &str, write: bool) -> Result<(), String> {
    let path = Path::new(file);

    if !path.exists() {
        return Err(format!("File not found: {}", file));
    }

    let raw = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let (_, source) = split_shebang(&raw);

    let formatted = match format_contents(&raw) {
        Ok(formatted) => formatted,
        Err(err) => {
            if let Some(diags) = err.diagnostics() {
                let source_name = path.display().to_string();
                diagnostics::emit_source_diagnostics(&source_name, source, diags);
                return Err("format parse error".to_string());
            }
            return Err(format!("Format error: {}", err));
        }
    };

    if write {
        if formatted != raw {
            fs::write(path, &formatted).map_err(|e| format!("Failed to write file: {}", e))?;
            output::success(&format!("Formatted: {file}"));
        } else {
            output::info(&format!("Already formatted: {file}"));
        }
    } else {
        // Print the formatted code
        // 打印格式化后的代码
        print!("{}", formatted);
    }

    Ok(())
}

/// Check if a file is formatted.
/// 检查文件是否已格式化。
pub fn check(file: &str) -> Result<(), String> {
    let path = Path::new(file);

    if !path.exists() {
        return Err(format!("File not found: {}", file));
    }

    let raw = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let (_, source) = split_shebang(&raw);

    let is_formatted = match format_contents(&raw) {
        Ok(formatted) => formatted == raw,
        Err(err) => {
            if let Some(diags) = err.diagnostics() {
                let source_name = path.display().to_string();
                diagnostics::emit_source_diagnostics(&source_name, source, diags);
                return Err("format parse error".to_string());
            }
            return Err(format!("Format error: {}", err));
        }
    };

    if is_formatted {
        output::success(&format!("OK: {file}"));
        Ok(())
    } else {
        Err(format!("Would reformat: {file}"))
    }
}

/// Format all n3v3 files in a directory.
/// 格式化目录中的所有 n3v3 文件。
pub fn format_dir(dir: &str, write: bool) -> Result<(), String> {
    let path = Path::new(dir);
    for ancestor in path.ancestors().filter(|part| !part.as_os_str().is_empty()) {
        let metadata = fs::symlink_metadata(ancestor)
            .map_err(|e| format!("Failed to inspect directory {}: {e}", ancestor.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Refusing symlinked directory: {}",
                ancestor.display()
            ));
        }
        if ancestor == path && !metadata.is_dir() {
            return Err(format!("Not a directory: {}", dir));
        }
    }

    let mut errors = Vec::new();
    format_dir_recursive(path, write, &mut errors)?;

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{} files would be reformatted", errors.len()))
    }
}

/// Recursively format all n3v3 files in a directory.
/// 递归格式化目录中的所有 n3v3 文件。
fn format_dir_recursive(dir: &Path, write: bool, errors: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory {}: {e}", dir.display()))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry in {}: {e}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| format!("Failed to inspect {}: {e}", path.display()))?;

        // Do not descend into directory links or format linked files, including cycles.
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            format_dir_recursive(&path, write, errors)?;
        } else if file_type.is_file()
            && path.extension().is_some_and(|ext| ext == "n3v3")
            && let Err(e) = run(&path.to_string_lossy(), write)
        {
            errors.push(e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check, format_contents, format_dir, run};

    #[test]
    fn format_contents_shebang_preserves_exact_prefix() {
        let shebang = "#!/usr/bin/env -S n3v3 run\r\n";
        let raw = format!("{shebang}let value=1\n");
        let formatted = format_contents(&raw).expect("source should format");
        assert!(formatted.starts_with(shebang));
        assert_eq!(
            &formatted[shebang.len()..],
            n3v3_fmt::format("let value=1\n").unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_write_preserves_shebang_and_executable_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("script.n3v3");
        let shebang = "#!/usr/bin/env n3v3 run\n";
        std::fs::write(&path, format!("{shebang}let value=1\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o751)).unwrap();

        run(path.to_str().unwrap(), true).expect("write format should succeed");

        let formatted = std::fs::read_to_string(&path).unwrap();
        assert!(formatted.starts_with(shebang));
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
            0o751
        );
        check(path.to_str().unwrap()).expect("written source should be canonical");
    }

    #[cfg(unix)]
    #[test]
    fn format_dir_write_skips_file_symlinks_inside_and_outside_root() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let inside_path = root.path().join("inside.txt");
        let outside_path = outside.path().join("outside.n3v3");
        let ordinary_path = root.path().join("ordinary.n3v3");
        let raw = "let value=1\n";
        std::fs::write(&inside_path, raw).unwrap();
        std::fs::write(&outside_path, raw).unwrap();
        std::fs::write(&ordinary_path, raw).unwrap();
        symlink(&inside_path, root.path().join("linked-inside.n3v3")).unwrap();
        symlink(&outside_path, root.path().join("linked-outside.n3v3")).unwrap();

        format_dir(root.path().to_str().unwrap(), true).unwrap();
        assert_ne!(std::fs::read_to_string(ordinary_path).unwrap(), raw);

        assert_eq!(std::fs::read_to_string(&inside_path).unwrap(), raw);
        assert_eq!(std::fs::read_to_string(&outside_path).unwrap(), raw);
        assert_eq!(
            std::fs::read_to_string(root.path().join("linked-inside.n3v3")).unwrap(),
            raw
        );
    }

    #[cfg(unix)]
    #[test]
    fn format_dir_write_skips_symlinked_directories_and_cycles() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let inside = root.path().join("nested");
        std::fs::create_dir(&inside).unwrap();
        std::fs::create_dir(outside.path().join("child")).unwrap();
        let outside_path = outside.path().join("outside.n3v3");
        std::fs::write(&outside_path, "let value=1\n").unwrap();
        symlink(outside.path(), root.path().join("external-dir")).unwrap();
        symlink(root.path(), inside.join("cycle")).unwrap();

        format_dir(root.path().to_str().unwrap(), true).unwrap();

        assert_eq!(
            std::fs::read_to_string(outside_path).unwrap(),
            "let value=1\n"
        );
        let error =
            format_dir(root.path().join("external-dir").to_str().unwrap(), true).unwrap_err();
        assert!(error.contains("Refusing symlinked directory"));
        let error = format_dir(
            root.path().join("external-dir/child").to_str().unwrap(),
            true,
        )
        .unwrap_err();
        assert!(error.contains("Refusing symlinked directory"));
    }
}
