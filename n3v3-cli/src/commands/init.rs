//! Initialize a new n3v3 project.
//! 初始化新的 n3v3 项目。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

const PROJECT_FILES: [&str; 3] = ["flake.n3v3", "main.n3v3", ".gitignore"];

fn reject_existing_path(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(format!("project output already exists: {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to inspect {}: {error}", path.display())),
    }
}

fn write_new_file(path: &Path, content: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    if let Err(error) = file.write_all(content.as_bytes()) {
        let cleanup = fs::remove_file(path)
            .err()
            .map(|cleanup| format!("; failed to remove partial file: {cleanup}"))
            .unwrap_or_default();
        return Err(format!(
            "failed to write {}: {error}{cleanup}",
            path.display()
        ));
    }
    Ok(())
}

fn write_project_files(dir: &Path, files: &[(&str, &str)]) -> Result<(), String> {
    let mut created = Vec::with_capacity(files.len());
    for (name, content) in files {
        let path = dir.join(name);
        if let Err(error) = write_new_file(&path, content) {
            let cleanup_errors = created
                .iter()
                .filter_map(|created_path| fs::remove_file(created_path).err())
                .map(|cleanup| cleanup.to_string())
                .collect::<Vec<_>>();
            if cleanup_errors.is_empty() {
                return Err(error);
            }
            return Err(format!(
                "{error}; failed to roll back project files: {}",
                cleanup_errors.join("; ")
            ));
        }
        created.push(path);
    }
    Ok(())
}

fn project_name(dir: &Path) -> String {
    dir.file_name()
        .unwrap_or("my-project".as_ref())
        .to_string_lossy()
        .into_owned()
}

fn render_flake(name: &str) -> String {
    format!(
        r#"{{
    description = "An n3v3 project",
    name = "{name}",
    version = "0.1.0",

    inputs = {{}},

    outputs = fn(inputs) {{
        let pkgs = {{}};
        let checks = {{
            default = fn() {{ true }},
        }};
        {{ packages = pkgs, checks = checks }}
    }},
}}"#
    )
}

fn render_main(name: &str) -> String {
    format!(
        r#"#!/usr/bin/env n3v3 run
-- {name} — main entry point
use std.io = io;

let (args, _) = io.args();
let name = match args {{
    [n, ..] -> n,
    [] -> "World"
}};
io.println("Hello, " ++ name ++ "!");
"#
    )
}

pub fn run(dir: &str) -> Result<(), String> {
    let dir = Path::new(dir);
    if fs::symlink_metadata(dir).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!("project directory is a symlink: {}", dir.display()));
    }
    for file in PROJECT_FILES {
        reject_existing_path(&dir.join(file))?;
    }

    let did_create_dir = !dir.exists();
    fs::create_dir_all(dir).map_err(|error| format!("mkdir {}: {error}", dir.display()))?;
    let name = project_name(dir);
    let flake = render_flake(&name);
    let main = render_main(&name);
    let result = write_project_files(
        dir,
        &[
            ("flake.n3v3", &flake),
            ("main.n3v3", &main),
            (".gitignore", "result\n.direnv\n"),
        ],
    );
    if let Err(error) = result {
        if did_create_dir && let Err(cleanup) = fs::remove_dir(dir) {
            return Err(format!(
                "{error}; failed to remove partial project directory: {cleanup}"
            ));
        }
        return Err(error);
    }

    println!("✅ Created n3v3 project in {}", dir.display());
    println!("   cd {} && n3v3 run main.n3v3", dir.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn run_writes_canonical_main_source() {
        let dir = tempfile::tempdir().expect("temporary project directory");
        run(dir.path().to_str().expect("temporary path should be UTF-8"))
            .expect("init should create the project");

        let main = std::fs::read_to_string(dir.path().join("main.n3v3"))
            .expect("generated main.n3v3 should be readable");
        assert!(main.contains("use std.io = io;"));
        assert!(main.contains("let (args, _) = io.args();"));
        assert!(!main.contains("fn main() ="));
        assert!(!main.contains("import std.io"));
        assert!(!main.contains("effect ="));

        let analysis = n3v3_frontend::analyze_source(&main);

        let has_errors = analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == n3v3_diagnostic::Severity::Error);
        assert!(
            !has_errors,
            "generated source should type-check: {:?}",
            analysis.diagnostics
        );
    }

    #[test]
    fn run_existing_output_rejects_without_partial_project() {
        for existing in ["main.n3v3", ".gitignore", "flake.n3v3"] {
            let dir = tempfile::tempdir().expect("temporary directory");
            let path = dir.path().join(existing);
            std::fs::write(&path, "user content").expect("seed collision");
            let result = run(dir.path().to_str().expect("UTF-8 temporary path"));
            assert!(result.is_err(), "existing {existing} should reject init");
            assert_eq!(std::fs::read_to_string(path).unwrap(), "user content");
            for other in super::PROJECT_FILES {
                if other != existing {
                    assert!(
                        !dir.path().join(other).exists(),
                        "{other} created after collision"
                    );
                }
            }
        }
    }

    #[test]
    fn write_project_files_late_collision_rolls_back_created_files() {
        let dir = tempfile::tempdir().expect("temporary directory");
        std::fs::write(dir.path().join("main.n3v3"), "raced content").unwrap();
        let result = super::write_project_files(
            dir.path(),
            &[("flake.n3v3", "flake"), ("main.n3v3", "main")],
        );
        assert!(result.is_err());
        assert!(!dir.path().join("flake.n3v3").exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("main.n3v3")).unwrap(),
            "raced content"
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_symlink_output_rejects_without_partial_project() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("temporary directory");
        let destination = dir.path().join("user-data");
        std::fs::write(&destination, "user content").unwrap();
        symlink(&destination, dir.path().join(".gitignore")).unwrap();
        let result = run(dir.path().to_str().expect("UTF-8 temporary path"));
        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(destination).unwrap(),
            "user content"
        );
        assert!(!dir.path().join("flake.n3v3").exists());
        assert!(!dir.path().join("main.n3v3").exists());

        let alias = dir.path().join("alias");
        symlink(dir.path(), &alias).unwrap();
        assert!(run(alias.to_str().unwrap()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn run_dangling_symlink_output_rejects_without_creating_files() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        symlink(dir.path().join("missing"), dir.path().join("main.n3v3")).unwrap();
        assert!(run(dir.path().to_str().unwrap()).is_err());
        assert!(!dir.path().join("flake.n3v3").exists());
        assert!(!dir.path().join(".gitignore").exists());
    }

    #[cfg(unix)]
    #[test]
    fn run_writes_loadable_flake_source() {
        let dir = tempfile::tempdir().expect("temporary project directory");
        run(dir.path().to_str().expect("temporary path should be UTF-8"))
            .expect("init should create the project");

        let flake = n3v3_config::flake::Flake::load(dir.path())
            .expect("generated flake.n3v3 should evaluate through frontend/HIR");
        assert_eq!(flake.description.as_deref(), Some("An n3v3 project"));
        assert!(
            flake.outputs.is_some(),
            "generated flake should define outputs"
        );
    }
}
