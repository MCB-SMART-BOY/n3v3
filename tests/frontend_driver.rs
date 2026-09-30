//! Integration tests for the multi-module frontend driver.
//! 多模块前端驱动的集成测试。

mod support;

use n3v3_diagnostic::{DiagnosticKind, Severity};
use n3v3_frontend::{DiagnosticStats, FrontendDriver, parse_module_file};
use support::module_fixtures::create_test_module;
use tempfile::TempDir;

#[test]
fn test_frontend_driver_analyzes_multi_module_program() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    create_test_module(
        root,
        &["math"],
        r#"
            fn add(x, y) = x + y;
        "#,
    );
    create_test_module(
        root,
        &["main"],
        r#"
            use math (add);
            fn compute() = add(1, 2);
        "#,
    );

    let analysis = FrontendDriver::new(root)
        .analyze_module_path(&["main".into()])
        .unwrap();

    assert_eq!(analysis.load_order().len(), 2);
    assert!(
        !analysis.type_names().is_empty(),
        "expected shared type name map to be populated"
    );

    for module_id in analysis.load_order() {
        assert!(analysis.module_info(*module_id).is_some());
        assert!(analysis.hir_module(*module_id).is_some());
        assert!(analysis.parsed_source(*module_id).is_some());
        assert!(
            analysis.diagnostics(*module_id).unwrap().is_empty(),
            "unexpected diagnostics for module {:?}: {:?}",
            module_id,
            analysis.diagnostics(*module_id).unwrap()
        );
    }
}

#[test]
fn test_frontend_driver_hides_action_plans_when_any_module_has_errors() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    create_test_module(
        root,
        &["math"],
        r#"
            use std.io = io;
            let content = io.readFile("config.n3v3");
        "#,
    );
    create_test_module(
        root,
        &["main"],
        r#"
            use math (content);
            let broken: Int = true;
        "#,
    );

    let analysis = FrontendDriver::new(root)
        .analyze_module_path(&["main".into()])
        .unwrap();

    assert!(
        analysis
            .diagnostic_modules_in_order()
            .iter()
            .flat_map(|module| module.diagnostics.iter())
            .any(|diagnostic| diagnostic.severity == Severity::Error),
        "expected a blocking diagnostic"
    );
    assert!(
        analysis.load_order().iter().all(|module_id| analysis
            .semantics(*module_id)
            .unwrap()
            .action_plans
            .is_empty()),
        "action plans must be hidden when any module has errors"
    );
}

#[test]
fn test_frontend_driver_resolves_trait_impls_across_modules() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    create_test_module(
        root,
        &["traits"],
        r#"
            trait Show { fn show(self) -> String; };
        "#,
    );
    create_test_module(
        root,
        &["impls"],
        r#"
            use traits (Show);
            impl Show for String {
                fn show(self) -> String = self;
            };
        "#,
    );
    create_test_module(
        root,
        &["main"],
        r#"
            use traits (Show);
            use impls;
            fn run() -> String = "ok".show();
        "#,
    );

    let analysis = FrontendDriver::new(root)
        .analyze_module_path(&["main".into()])
        .unwrap();

    for module_id in analysis.load_order() {
        let diagnostics = analysis.diagnostics(*module_id).unwrap();
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity != Severity::Error),
            "unexpected cross-module trait diagnostics for {:?}: {:?}",
            module_id,
            diagnostics
        );
    }
}

#[test]
fn test_frontend_driver_preserves_parse_diagnostics_per_module() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    create_test_module(
        root,
        &["bad"],
        r#"
            fn broken(x) =
        "#,
    );
    create_test_module(
        root,
        &["main"],
        r#"
            use bad (broken);
            fn run() = 1;
        "#,
    );

    let analysis = FrontendDriver::new(root)
        .analyze_module_path(&["main".into()])
        .unwrap();

    let bad_module = analysis
        .load_order()
        .iter()
        .copied()
        .find(|module_id| {
            analysis
                .module_info(*module_id)
                .map(|info| info.path.as_slice() == ["bad"])
                .unwrap_or(false)
        })
        .expect("bad module should be loaded");

    let diagnostics = analysis.diagnostics(bad_module).unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Parser),
        "expected parser diagnostics, got {:?}",
        diagnostics
    );
    assert!(
        analysis.method_resolutions(bad_module).unwrap().is_empty(),
        "parse-broken module should not have method resolutions"
    );
}

#[test]
fn test_frontend_driver_returns_dependency_first_diagnostic_modules() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    create_test_module(
        root,
        &["bad_parse"],
        r#"
            fn broken(x) =
        "#,
    );
    create_test_module(root, &["bad_type"], "fn bad() = 1 + true;");
    create_test_module(
        root,
        &["main"],
        r#"
            use bad_parse;
            use bad_type;
            fn run() = 1;
        "#,
    );

    let analysis = FrontendDriver::new(root)
        .analyze_module_path(&["main".into()])
        .unwrap();

    let diagnostic_modules = analysis.diagnostic_modules_in_order();
    let paths: Vec<_> = diagnostic_modules
        .iter()
        .map(|entry| {
            entry
                .file_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    assert_eq!(paths, vec!["bad_parse.n3v3", "bad_type.n3v3", "main.n3v3"]);

    let parse_entry = diagnostic_modules
        .iter()
        .find(|entry| entry.file_path.ends_with("bad_parse.n3v3"))
        .expect("expected parse-broken module diagnostics");
    assert!(
        parse_entry
            .diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Parser),
        "expected parser diagnostics, got {:?}",
        parse_entry.diagnostics
    );
    assert!(
        parse_entry.source.contains("fn broken"),
        "expected diagnostic source text, got {:?}",
        parse_entry.source
    );

    let type_entry = diagnostic_modules
        .iter()
        .find(|entry| entry.file_path.ends_with("bad_type.n3v3"))
        .expect("expected type-broken module diagnostics");
    assert!(
        type_entry
            .diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Type),
        "expected type diagnostics, got {:?}",
        type_entry.diagnostics
    );
}

#[test]
fn test_frontend_driver_returns_only_syntax_diagnostic_modules_in_dependency_order() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    create_test_module(
        root,
        &["bad_parse_a"],
        r#"
            fn broken_a(x) =
        "#,
    );
    create_test_module(root, &["bad_type"], "fn bad() = 1 + true;");
    create_test_module(
        root,
        &["bad_parse_b"],
        r#"
            fn broken_b(x) =
        "#,
    );
    create_test_module(
        root,
        &["main"],
        r#"
            use bad_parse_a;
            use bad_type;
            use bad_parse_b;
            fn run() = 1;
        "#,
    );

    let analysis = FrontendDriver::new(root)
        .analyze_module_path(&["main".into()])
        .unwrap();

    let syntax_paths: Vec<_> = analysis
        .syntax_diagnostic_modules_in_order()
        .into_iter()
        .map(|entry| {
            assert!(
                entry.diagnostics.iter().all(|diag| diag.kind.is_syntax()),
                "expected syntax-only diagnostics, got {:?}",
                entry.diagnostics
            );
            entry
                .file_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();

    assert_eq!(syntax_paths, vec!["bad_parse_a.n3v3", "bad_parse_b.n3v3"]);
}

#[test]
fn test_frontend_driver_returns_only_parse_clean_modules_in_dependency_order() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    create_test_module(root, &["util"], "fn inc(x) = x + 1;");
    create_test_module(
        root,
        &["bad_parse"],
        r#"
            fn broken(x) =
        "#,
    );
    create_test_module(
        root,
        &["main"],
        r#"
            use util (inc);
            use bad_parse;
            fn run() = inc(1);
        "#,
    );

    let analysis = FrontendDriver::new(root)
        .analyze_module_path(&["main".into()])
        .unwrap();

    let parsed_paths: Vec<_> = analysis
        .parsed_modules_in_order()
        .into_iter()
        .map(|entry| {
            entry
                .file_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();

    assert_eq!(parsed_paths, vec!["util.n3v3", "main.n3v3"]);
}

#[test]
fn test_frontend_driver_returns_only_lowered_modules_in_dependency_order() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    create_test_module(root, &["util"], "fn inc(x) = x + 1;");
    create_test_module(root, &["bad_type"], "fn bad() = 1 + true;");
    create_test_module(
        root,
        &["warn_only"],
        r#"
            use std.option = option;
            fn warned() = match option.some(1) {
                Some(_) -> 1,
                Some(inner) -> inner,
                None -> 0
            };
        "#,
    );
    create_test_module(
        root,
        &["bad_parse"],
        r#"
            fn broken(x) =
        "#,
    );
    create_test_module(
        root,
        &["main"],
        r#"
            use util (inc);
            use bad_type;
            use warn_only;
            use bad_parse;
            fn run() = inc(1);
        "#,
    );

    let analysis = FrontendDriver::new(root)
        .analyze_module_path(&["main".into()])
        .unwrap();

    let lowered_modules = analysis.lowered_modules_in_order();
    let lowered_paths: Vec<_> = lowered_modules
        .iter()
        .map(|entry| {
            entry
                .file_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();

    assert_eq!(
        lowered_paths,
        vec!["util.n3v3", "bad_type.n3v3", "warn_only.n3v3", "main.n3v3"]
    );
    assert!(
        lowered_modules
            .iter()
            .all(|entry| !entry.ast.items.is_empty()),
        "expected parse-clean AST payloads, got {:?}",
        lowered_modules
    );
    assert!(
        lowered_modules
            .iter()
            .all(|entry| !entry.module.items.is_empty()),
        "expected lowered HIR payloads, got {:?}",
        lowered_modules
    );

    let bad_type = lowered_modules
        .iter()
        .find(|entry| entry.file_path.ends_with("bad_type.n3v3"))
        .expect("expected type-broken lowered module");
    assert!(
        bad_type
            .diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Type),
        "expected final type diagnostics, got {:?}",
        bad_type.diagnostics
    );

    let warn_only = lowered_modules
        .iter()
        .find(|entry| entry.file_path.ends_with("warn_only.n3v3"))
        .expect("expected warning-only lowered module");
    assert!(
        warn_only
            .diagnostics
            .iter()
            .any(|diag| diag.severity == Severity::Warning),
        "expected warning diagnostics, got {:?}",
        warn_only.diagnostics
    );
}

#[test]
fn test_frontend_driver_diagnostic_stats_distinguish_errors_and_warnings() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    create_test_module(
        root,
        &["bad_parse"],
        r#"
            fn broken(x) =
        "#,
    );
    create_test_module(root, &["bad_type"], "fn bad() = 1 + true;");
    create_test_module(
        root,
        &["warn_only"],
        r#"
            use std.option = option;
            fn warned() = match option.some(1) {
                Some(_) -> 1,
                Some(inner) -> inner,
                None -> 0
            };
        "#,
    );
    create_test_module(
        root,
        &["main"],
        r#"
            use bad_parse;
            use bad_type;
            use warn_only;
            fn run() = 1;
        "#,
    );

    let analysis = FrontendDriver::new(root)
        .analyze_module_path(&["main".into()])
        .unwrap();
    let stats: DiagnosticStats = analysis.diagnostic_stats();

    assert!(
        stats.parse_errors > 0,
        "expected parse errors, got {:?}",
        stats
    );
    assert!(
        stats.non_parse_errors > 0,
        "expected blocking non-parse errors"
    );
    assert_eq!(stats.warnings, 1);
    assert!(stats.has_errors());
    assert!(analysis.has_blocking_diagnostics());

    let warning_entry = analysis
        .diagnostic_modules_in_order()
        .into_iter()
        .find(|entry| entry.file_path.ends_with("warn_only.n3v3"))
        .expect("expected warning-only module diagnostics");
    assert!(
        warning_entry
            .diagnostics
            .iter()
            .any(|diag| diag.severity == Severity::Warning),
        "expected warning diagnostics, got {:?}",
        warning_entry.diagnostics
    );
}

#[test]
fn test_frontend_driver_blocking_diagnostic_messages_preserve_order_and_exclude_warnings() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    create_test_module(
        root,
        &["bad_parse"],
        r#"
            fn broken(x) =
        "#,
    );
    create_test_module(root, &["bad_type"], "fn bad() = 1 + true;");
    create_test_module(
        root,
        &["warn_only"],
        r#"
            use std.option = option;
            fn warned() = match option.some(1) {
                Some(_) -> 1,
                Some(inner) -> inner,
                None -> 0
            };
        "#,
    );
    create_test_module(
        root,
        &["main"],
        r#"
            use bad_parse;
            use bad_type;
            use warn_only;
            fn run() = 1;
        "#,
    );

    let analysis = FrontendDriver::new(root)
        .analyze_module_path(&["main".into()])
        .unwrap();
    let messages = analysis.blocking_diagnostic_messages();

    assert!(
        messages.len() >= 2,
        "expected parse + type messages, got {:?}",
        messages
    );
    assert!(
        messages[0].contains("bad_parse.n3v3"),
        "expected parse-broken module first, got {:?}",
        messages
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("bad_type.n3v3")),
        "expected type-broken module message, got {:?}",
        messages
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("warn_only.n3v3")),
        "warning-only module should not produce blocking messages: {:?}",
        messages
    );
}

#[test]
fn test_frontend_parse_module_file_returns_parse_clean_ast_payload() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    create_test_module(root, &["std", "mod"], "let result = 42;");
    let parsed = parse_module_file(root.join("std").join("mod.n3v3"), &["std".into()]).unwrap();

    assert!(
        parsed.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        parsed
    );
    assert_eq!(parsed.module_path, vec!["std"]);
    assert!(parsed.ast.is_some(), "expected parse-clean AST payload");
    assert!(parsed.source.contains("let result = 42;"));
}

#[test]
fn test_frontend_driver_returns_only_evaluable_modules_in_dependency_order() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    create_test_module(root, &["util"], "fn inc(x) = x + 1;");
    create_test_module(root, &["broken"], "fn bad() = 1 + true;");
    create_test_module(
        root,
        &["main"],
        r#"
            use util (inc);
            use broken (bad);
            fn compute() = inc(1);
        "#,
    );

    let analysis = FrontendDriver::new(root)
        .analyze_module_path(&["main".into()])
        .unwrap();

    let names_by_module_id: std::collections::HashMap<_, _> = analysis
        .load_order()
        .iter()
        .filter_map(|module_id| {
            analysis.module_info(*module_id).map(|info| {
                (
                    *module_id,
                    info.file_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                )
            })
        })
        .collect();
    let evaluable_paths: Vec<_> = analysis
        .evaluable_modules_in_order()
        .into_iter()
        .map(|entry| names_by_module_id.get(&entry.module_id).unwrap().clone())
        .collect();

    assert_eq!(evaluable_paths, vec!["util.n3v3", "main.n3v3"]);
}
