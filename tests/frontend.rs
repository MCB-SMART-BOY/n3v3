//! Integration tests for the frontend analysis pipeline.
//! 前端分析管线的集成测试。

use n3v3_diagnostic::{DiagnosticKind, ErrorCode};
use n3v3_frontend::{DiagnosticStats, analyze_snippet_ast, analyze_source};
use n3v3_hir::ItemKind;
use n3v3_parser::parse;
use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_frontend_reports_parse_errors() {
    let result = analyze_source("let x =");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Parser),
        "expected parser diagnostics"
    );
}

#[test]
fn test_frontend_reports_type_errors() {
    let result = analyze_source("let x = 1 + true;");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Type),
        "expected type diagnostics"
    );
}

#[test]
fn test_frontend_rejects_duplicate_enum_variant_names() {
    let result = analyze_source("enum First { Same }; enum Second { Same };");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.message.contains("duplicate enum variant `Same`")),
        "expected duplicate variant diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn test_frontend_formats_named_types_readably_in_diagnostics() {
    let result = analyze_source(
        r#"
            struct User {};
            fn broken(x: User) -> Int = x.name;
        "#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Type),
        "expected type diagnostics, got {:?}",
        result.diagnostics
    );

    for diagnostic in &result.diagnostics {
        assert!(
            !diagnostic.message.contains("Type#"),
            "unexpected raw type placeholder in message: {:?}",
            diagnostic
        );
        for label in &diagnostic.labels {
            assert!(
                !label.message.contains("Type#"),
                "unexpected raw type placeholder in label: {:?}",
                diagnostic
            );
        }
        for note in &diagnostic.notes {
            assert!(
                !note.contains("Type#"),
                "unexpected raw type placeholder in note: {:?}",
                diagnostic
            );
        }
        if let Some(help) = &diagnostic.help {
            assert!(
                !help.contains("Type#"),
                "unexpected raw type placeholder in help: {:?}",
                diagnostic
            );
        }
    }
}

#[test]
fn test_frontend_accepts_record_field_access_after_record_binding() {
    let result = analyze_source(
        r#"
            let config = #{ port = 40, host = "localhost" };
            let x = config.port;
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_lazy_force_pipeline() {
    let result = analyze_source(
        r#"
            let thunk = ~42;
            let x = force(thunk);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_or_and_binding_patterns() {
    let result = analyze_source(
        r#"
            let a = match (1, 2) { (0, v) | (1, v) -> v, _ -> 0 };
            let b = match 42 { n @ 42 -> n, _ -> 0 };
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_list_rest_patterns() {
    let result = analyze_source(
        r#"
            let x = match [1, 2, 3, 4] {
                [first, ..middle, last] -> match middle {
                    [a, b] -> first + a + b + last,
                    _ -> 0,
                },
                _ -> 0,
            };
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_reports_impl_method_type_errors() {
    let result = analyze_source(
        r#"
            struct Counter {};
            impl Counter {
                fn value(self) -> Int = true;
            };
        "#,
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Type),
        "expected impl method type diagnostics, got {:?}",
        result.diagnostics
    );
}

#[test]
fn test_frontend_reports_trait_impl_signature_mismatch() {
    let result = analyze_source(
        r#"
            trait Show { fn show(self) -> Int; };
            struct Counter {};
            impl Show for Counter {
                fn show(self) -> String = "counter";
            };
        "#,
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Type),
        "expected trait impl signature diagnostics, got {:?}",
        result.diagnostics
    );
}

#[test]
fn test_frontend_accepts_self_and_assoc_type_use_sites() {
    let result = analyze_source(
        r#"
            trait Iterator {
                type Item;
                fn first(self) -> Self.Item;
            };
            struct Counter {};
            impl Iterator for Counter {
                type Item = Int;
                fn first(self) -> Self.Item = 1;
            };
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_assoc_bound_through_canonical_self_assoc_binding() {
    let result = analyze_source(
        r#"
            trait Show { };
            trait Iterator { type Item: Show; type Alias; };
            struct Foo {};
            impl Show for Int { };
            impl Iterator for Foo {
                type Alias = Int;
                type Item = Self.Alias;
            };
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_assoc_bound_failure_uses_canonical_type_rendering() {
    let result = analyze_source(
        r#"
            trait Show { };
            trait Iterator { type Item: Show; type Alias; };
            struct Foo {};
            impl Iterator for Foo {
                type Alias = Int;
                type Item = Self.Alias;
            };
        "#,
    );
    let diag = result
        .diagnostics
        .iter()
        .find(|diag| {
            diag.message.contains(
                "associated type 'Item' in impl of trait 'Iterator' must satisfy bound 'Show'",
            )
        })
        .expect("expected assoc-type bound diagnostic");

    assert!(
        diag.labels.iter().any(|label| label
            .message
            .contains("associated type resolves to `Int` here")),
        "expected canonical assoc-bound label, got {:?}",
        diag
    );
    assert!(
        diag.notes
            .iter()
            .any(|note| note.contains("`Int` does not implement `Show`")),
        "expected canonical assoc-bound note, got {:?}",
        diag
    );
}

#[test]
fn test_frontend_exposes_assoc_projection_resolutions_for_explicit_self_item_use_sites() {
    let result = analyze_source(
        r#"
            trait Iterator {
                type Item;
                fn first(self, fallback: Self.Item) -> Self.Item;
            };
            impl Iterator for Int {
                type Item = String;
                fn first(self, fallback: Self.Item) -> Self.Item = fallback;
            };
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );

    let impl_def = result
        .hir
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Impl(impl_def) => Some(impl_def),
            _ => None,
        })
        .expect("impl definition should exist");
    let method = impl_def.items.first().expect("impl method should exist");
    let fallback_span = method.params[1].ty.span;
    let return_span = method.return_ty.span;

    let fallback_ty = result
        .semantics
        .assoc_projection_resolution(fallback_span)
        .expect("fallback Self.Item projection should be recorded");
    let return_ty = result
        .semantics
        .assoc_projection_resolution(return_span)
        .expect("return Self.Item projection should be recorded");

    assert_eq!(
        n3v3_frontend::format_type_in_module(fallback_ty, &result.hir),
        "String"
    );
    assert_eq!(
        n3v3_frontend::format_type_in_module(return_ty, &result.hir),
        "String"
    );
}

#[test]
fn test_frontend_keeps_trait_self_assoc_spans_source_level_when_impl_is_present() {
    let result = analyze_source(
        r#"
            trait Iterator {
                type Item;
                fn first(self, fallback: Self.Item) -> Self.Item;
            };
            impl Iterator for Int {
                type Item = String;
                fn first(self, fallback: Self.Item) -> Self.Item = fallback;
            };
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );

    let trait_def = result
        .hir
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Trait(trait_def) => Some(trait_def),
            _ => None,
        })
        .expect("trait definition should exist");
    let impl_def = result
        .hir
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Impl(impl_def) => Some(impl_def),
            _ => None,
        })
        .expect("impl definition should exist");

    let trait_method = trait_def.items.first().expect("trait method should exist");
    let impl_method = impl_def.items.first().expect("impl method should exist");

    assert!(
        result
            .semantics
            .assoc_projection_resolution(trait_method.params[1].span)
            .is_none(),
        "trait param Self.Item span should stay source-level"
    );
    assert!(
        result
            .semantics
            .assoc_projection_resolution(trait_method.return_ty.span)
            .is_none(),
        "trait return Self.Item span should stay source-level"
    );

    let impl_param_ty = result
        .semantics
        .assoc_projection_resolution(impl_method.params[1].ty.span)
        .expect("impl param Self.Item projection should be recorded");
    let impl_return_ty = result
        .semantics
        .assoc_projection_resolution(impl_method.return_ty.span)
        .expect("impl return Self.Item projection should be recorded");

    assert_eq!(
        n3v3_frontend::format_type_in_module(impl_param_ty, &result.hir),
        "String"
    );
    assert_eq!(
        n3v3_frontend::format_type_in_module(impl_return_ty, &result.hir),
        "String"
    );
}

#[test]
fn test_frontend_trait_signature_mismatch_uses_projection_labels() {
    let result = analyze_source(
        r#"
            trait Iterator {
                type Item;
                fn first(self, fallback: Self.Item) -> Self.Item;
            };
            impl Iterator for Int {
                type Item = String;
                fn first(self, fallback: Int) -> Int = fallback;
            };
        "#,
    );
    let diag = result
        .diagnostics
        .iter()
        .find(|diag| {
            diag.message
                .contains("does not match trait `Iterator` signature")
        })
        .expect("expected impl signature mismatch diagnostic");

    assert!(
        diag.labels.iter().any(|label| label
            .message
            .contains("`Self.Item` resolves to `String` here")),
        "expected canonical assoc-projection label, got {:?}",
        diag
    );
}

#[test]
fn test_frontend_impl_method_body_mismatch_uses_projection_labels() {
    let result = analyze_source(
        r#"
            trait Iterator {
                type Item;
                fn first(self) -> Self.Item;
            };
            impl Iterator for Int {
                type Item = String;
                fn first(self) -> Self.Item = true;
            };
        "#,
    );
    let diag = result
        .diagnostics
        .iter()
        .find(|diag| diag.message.contains("impl method `first` return type"))
        .expect("expected impl body mismatch diagnostic");

    assert!(
        diag.labels.iter().any(|label| label
            .message
            .contains("`Self.Item` resolves to `String` here")),
        "expected canonical assoc-projection label, got {:?}",
        diag
    );
}

#[test]
fn test_frontend_accepts_try_on_option_and_result_like_enums() {
    let result = analyze_source(
        r#"
            enum Option { Some(Int), None };
            enum Result { Ok(Int), Err(String) };
            let a = Some(41)? + 1;
            let b = Ok(1)? + 1;
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_coalesce_on_safe_field_and_option_enum() {
    let result = analyze_source(
        r#"
            enum Option { Some(Int), None };
            let a = Some(41) ?? 0;
            let r = #{ name = "test" };
            let b = r?.missing ?? "default";
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_reports_try_invalid_optional_flow_message_and_code() {
    let result = analyze_source("let value = 41?;");
    let diag = result
        .diagnostics
        .iter()
        .find(|diag| {
            diag.message
                .contains("`?` expects Option-like or Result-like value")
        })
        .unwrap_or_else(|| {
            panic!(
                "expected canonical invalid-try diagnostic, got {:?}",
                result.diagnostics
            )
        });
    assert_eq!(diag.kind, DiagnosticKind::Type);
    assert_eq!(diag.code, Some(ErrorCode::TypeMismatch));
}

#[test]
fn test_frontend_reports_coalesce_invalid_optional_flow_message_and_code() {
    let result = analyze_source("let value = 41 ?? 0;");
    let diag = result
        .diagnostics
        .iter()
        .find(|diag| diag.message.contains("`??` expects Option-like value"))
        .unwrap_or_else(|| {
            panic!(
                "expected canonical invalid-coalesce diagnostic, got {:?}",
                result.diagnostics
            )
        });
    assert_eq!(diag.kind, DiagnosticKind::Type);
    assert_eq!(diag.code, Some(ErrorCode::TypeMismatch));
}

#[test]
fn test_frontend_reports_safe_field_boundary_message_and_code() {
    let result = analyze_source(r#"let value = 42?.name ?? "default";"#);
    let diag = result
        .diagnostics
        .iter()
        .find(|diag| {
            diag.message
                .contains("safe field access requires a record or Option[Record]")
        })
        .unwrap_or_else(|| {
            panic!(
                "expected canonical invalid safe-field diagnostic, got {:?}",
                result.diagnostics
            )
        });
    assert_eq!(diag.kind, DiagnosticKind::Type);
    assert_eq!(diag.code, Some(ErrorCode::TypeMismatch));
}

#[test]
fn test_frontend_reports_invalid_io_read_file_path_message_and_code() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let value = io.readFilePath("/tmp/file.txt");
        "#,
    );
    let diag = result
        .diagnostics
        .iter()
        .find(|diag| diag.message.contains("type mismatch"))
        .unwrap_or_else(|| {
            panic!(
                "expected canonical invalid io.readFilePath diagnostic, got {:?}",
                result.diagnostics
            )
        });
    assert_eq!(diag.kind, DiagnosticKind::Type);
    assert_eq!(diag.code, Some(ErrorCode::TypeMismatch));
}

#[test]
fn test_frontend_snippet_accepts_local_imports_against_root_dir() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("math.n3v3"), "fn add(x, y) = x + y;").unwrap();

    let source = "use math (add); let result = add(1, 2);";
    let (ast, diagnostics) = parse(source);
    assert!(
        diagnostics.is_empty(),
        "unexpected parse diagnostics: {:?}",
        diagnostics
    );

    let analysis =
        analyze_snippet_ast(&ast, temp_dir.path()).expect("snippet analysis should succeed");
    assert!(
        analysis.diagnostics.is_empty(),
        "unexpected snippet diagnostics: {:?}",
        analysis.diagnostics
    );
    assert!(
        analysis
            .loaded_modules
            .iter()
            .any(|entry| entry.file_path.ends_with("math.n3v3") && entry.diagnostics.is_empty()),
        "expected successfully loaded dependency module, got {:?}",
        analysis.loaded_modules
    );
}

#[test]
fn test_frontend_snippet_hides_dependency_action_plans_when_root_has_errors() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("math.n3v3"),
        r#"
            use std.io = io;
            let content = io.readFile("config.n3v3");
        "#,
    )
    .unwrap();

    let source = "use math (content); let broken: Int = true;";
    let (ast, diagnostics) = parse(source);
    assert!(
        diagnostics.is_empty(),
        "unexpected parse diagnostics: {:?}",
        diagnostics
    );

    let analysis =
        analyze_snippet_ast(&ast, temp_dir.path()).expect("snippet analysis should succeed");
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == n3v3_diagnostic::Severity::Error),
        "expected root type diagnostics, got {:?}",
        analysis.diagnostics
    );
    assert!(analysis.semantics.action_plans.is_empty());
    assert!(
        analysis
            .loaded_modules
            .iter()
            .all(|entry| entry.semantics.action_plans.is_empty()),
        "dependency action plans must be hidden when the snippet has errors: {:?}",
        analysis.loaded_modules
    );
}

#[test]
fn test_frontend_snippet_reports_loaded_module_diagnostics() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("math.n3v3"), "fn add(x, y) = ;").unwrap();

    let source = "use math (add); let result = add(1, 2);";
    let (ast, diagnostics) = parse(source);
    assert!(
        diagnostics.is_empty(),
        "unexpected parse diagnostics: {:?}",
        diagnostics
    );

    let analysis =
        analyze_snippet_ast(&ast, temp_dir.path()).expect("snippet analysis should succeed");
    assert!(
        analysis
            .loaded_modules
            .iter()
            .flat_map(|entry| entry.diagnostics.iter())
            .any(|diag| diag.kind == DiagnosticKind::Parser),
        "expected loaded parser diagnostics, got {:?}",
        analysis.loaded_modules
    );
}

#[test]
fn test_frontend_snippet_preserves_dependency_first_loaded_module_order() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("util.n3v3"), "fn inc(x) = x + 1;").unwrap();
    fs::write(
        temp_dir.path().join("math.n3v3"),
        "use util (inc); fn add_one(x) = inc(x);",
    )
    .unwrap();

    let source = "use math (add_one); let result = add_one(1);";
    let (ast, diagnostics) = parse(source);
    assert!(
        diagnostics.is_empty(),
        "unexpected parse diagnostics: {:?}",
        diagnostics
    );

    let analysis =
        analyze_snippet_ast(&ast, temp_dir.path()).expect("snippet analysis should succeed");
    let loaded_paths: Vec<_> = analysis
        .loaded_modules
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

    assert_eq!(loaded_paths, vec!["util.n3v3", "math.n3v3"]);
}

#[test]
fn test_frontend_snippet_loaded_diagnostic_stats_distinguish_errors_and_warnings() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("bad_parse.n3v3"), "fn broken(x) =").unwrap();
    fs::write(
        temp_dir.path().join("bad_type.n3v3"),
        "fn bad() = 1 + true;",
    )
    .unwrap();
    fs::write(
        temp_dir.path().join("warn_only.n3v3"),
        r#"
            use std.option = option;
            fn warned() = match option.some(1) {
                Some(_) -> 1,
                Some(inner) -> inner,
                None -> 0
            };
        "#,
    )
    .unwrap();

    let source = "use bad_parse; use bad_type; use warn_only; let result = 1;";
    let (ast, diagnostics) = parse(source);
    assert!(
        diagnostics.is_empty(),
        "unexpected parse diagnostics: {:?}",
        diagnostics
    );

    let analysis =
        analyze_snippet_ast(&ast, temp_dir.path()).expect("snippet analysis should succeed");
    let stats: DiagnosticStats = analysis.loaded_diagnostic_stats();

    assert!(
        stats.parse_errors > 0,
        "expected loaded parse errors, got {:?}",
        stats
    );
    assert!(
        stats.non_parse_errors > 0,
        "expected loaded type errors, got {:?}",
        stats
    );
    assert_eq!(stats.warnings, 1);
    assert!(stats.has_errors());
    assert!(analysis.loaded_has_blocking_diagnostics());
    assert!(
        analysis
            .loaded_modules
            .iter()
            .all(|entry| !entry.source.is_empty()),
        "expected loaded module source text for diagnostics, got {:?}",
        analysis.loaded_modules
    );
}

#[test]
fn test_frontend_snippet_current_diagnostic_stats_report_blocking_type_errors() {
    let temp_dir = TempDir::new().unwrap();
    let source = "let value = 1 + true;";
    let (ast, diagnostics) = parse(source);
    assert!(
        diagnostics.is_empty(),
        "unexpected parse diagnostics: {:?}",
        diagnostics
    );

    let analysis =
        analyze_snippet_ast(&ast, temp_dir.path()).expect("snippet analysis should succeed");
    let stats: DiagnosticStats = analysis.diagnostic_stats();

    assert_eq!(stats.parse_errors, 0);
    assert!(
        stats.non_parse_errors > 0,
        "expected current snippet type errors, got {:?}",
        stats
    );
    assert_eq!(stats.warnings, 0);
    assert!(stats.has_errors());
    assert!(analysis.has_blocking_diagnostics());
}

#[test]
fn test_frontend_snippet_returns_only_evaluable_loaded_modules_in_dependency_order() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("util.n3v3"), "fn inc(x) = x + 1;").unwrap();
    fs::write(
        temp_dir.path().join("math.n3v3"),
        "use util (inc); fn add_one(x) = inc(x);",
    )
    .unwrap();
    fs::write(temp_dir.path().join("broken.n3v3"), "fn bad() = 1 + true;").unwrap();

    let source = "use math (add_one); use broken (bad); let result = add_one(1);";
    let (ast, diagnostics) = parse(source);
    assert!(
        diagnostics.is_empty(),
        "unexpected parse diagnostics: {:?}",
        diagnostics
    );

    let analysis =
        analyze_snippet_ast(&ast, temp_dir.path()).expect("snippet analysis should succeed");
    assert!(
        analysis.loaded_modules.iter().any(|entry| {
            entry.file_path.ends_with("broken.n3v3")
                && entry
                    .diagnostics
                    .iter()
                    .any(|diag| diag.kind == DiagnosticKind::Type)
        }),
        "expected broken loaded dependency diagnostics, got {:?}",
        analysis.loaded_modules
    );

    let names_by_module_id: HashMap<_, _> = analysis
        .loaded_modules
        .iter()
        .map(|entry| {
            (
                entry.module_id,
                entry
                    .file_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
            )
        })
        .collect();
    let evaluable_paths: Vec<_> = analysis
        .evaluable_loaded_modules
        .iter()
        .map(|entry| names_by_module_id.get(&entry.module_id).unwrap().clone())
        .collect();

    assert_eq!(evaluable_paths, vec!["util.n3v3", "math.n3v3"]);
    assert!(
        analysis
            .evaluable_loaded_modules
            .iter()
            .all(|entry| !entry.module.items.is_empty()),
        "expected evaluable modules to include lowered HIR, got {:?}",
        analysis.evaluable_loaded_modules
    );
}

#[test]
fn test_frontend_accepts_trait_method_call_analysis() {
    let result = analyze_source(
        r#"
            trait Show { fn show(self) -> String; };
            impl Show for Int {
                fn show(self) -> String = toString(self);
            };
            let x = 1.show();
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_method_dispatch_precedence_records_method_resolution() {
    let result = analyze_source(
        r#"
            fn twice(x: Int) -> String = "fallback";
            trait Twice { fn twice(self) -> Int; };
            impl Twice for Int {
                fn twice(self) -> Int = self + self;
            };
            let value: Int = 21.twice();
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
    assert_eq!(
        result.semantics.method_resolutions.len(),
        1,
        "expected trait dispatch to record one canonical method resolution"
    );
}

#[test]
fn test_frontend_callable_target_fallback_does_not_record_method_resolution() {
    let result = analyze_source(
        r#"
            fn twice(x: Int) -> Int = x + x;
            let value = 21.twice();
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
    assert!(
        result.semantics.method_resolutions.is_empty(),
        "callable-target fallback should not record a method resolution"
    );
}

#[test]
fn test_frontend_reports_dedicated_missing_method_diagnostic_when_no_fallback_exists() {
    let result = analyze_source("let value = 21.missing();");
    let diag = result
        .diagnostics
        .iter()
        .find(|diag| diag.message.contains("no method `missing` found for `Int`"))
        .unwrap_or_else(|| {
            panic!(
                "expected dedicated missing-method diagnostic, got {:?}",
                result.diagnostics
            )
        });
    assert_eq!(diag.kind, DiagnosticKind::Type);
    assert_eq!(diag.code, Some(ErrorCode::UnknownMethod));
}

#[test]
fn test_frontend_accepts_std_item_and_module_imports() {
    let result = analyze_source(
        r#"
            use std.list (len);
            use std.string = string;
            let a = len([1, 2, 3]);
            let b = string.len("abc");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_glob_imports() {
    let result = analyze_source(
        r#"
            use std.list (*);
            let x = len([1, 2, 3]);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_option_and_result_builtins() {
    let result = analyze_source(
        r#"
            use std.option = option;
            use std.result = result;
            let a = option.some(41)? + 1;
            let b = option.none ?? 5;
            let c = result.ok(1)? + 1;
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_math_constants() {
    let result = analyze_source(
        r#"
            use std.math = math;
            let top = math.inf;
            let quiet = math.nan;
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_math_conversion_bridges() {
    let result = analyze_source(
        r#"
            use std.math = math;
            let count = math.toInt(true);
            let ratio = math.toFloat("1.5");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_math_float_predicates() {
    let result = analyze_source(
        r#"
            use std.math = math;
            let a = math.isNan(math.nan);
            let b = math.isInf(math.inf);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_math_rounding_bridges() {
    let result = analyze_source(
        r#"
            use std.math = math;
            let a = math.floor(1.9);
            let b = math.ceil(1.1);
            let c = math.round(1.6);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_math_unary_float_transforms() {
    let result = analyze_source(
        r#"
            use std.math = math;
            let a = math.sqrt(9.0);
            let b = math.log(1.0);
            let c = math.log10(1000.0);
            let d = math.exp(0.0);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_math_trigonometric_bridges() {
    let result = analyze_source(
        r#"
            use std.math = math;
            let a = math.sin(0.0);
            let b = math.cos(0.0);
            let c = math.tan(0.0);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_math_function_pending_explicit_surface() {
    let result = analyze_source(
        r#"
            use std.math = math;
            let value = math.abs(1);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_path_builtins() {
    let result = analyze_source(
        r#"
            use std.path = path;
            let p = path.fromString("/tmp/file.txt");
            let d = toString(p);
            let a = path.join("a", "b");
            let b = path.parent("/tmp/file.txt") ?? "/";
            let c = path.is_absolute("/tmp/file.txt");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_typed_path_adapters() {
    let result = analyze_source(
        r#"
            use std.path = path;
            let nested = path.joinPath(path.fromString("/tmp"), "n3v3.txt");
            let parent = path.parentPath(nested) ?? path.fromString("/");
            let name = path.filenamePath(nested) ?? "missing";
            let ext = path.extensionPath(nested) ?? "missing";
            let abs = path.isAbsolutePath(parent);
            let shown = toString(parent);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_fetch_path_bridge() {
    let result = analyze_source(
        r#"
            use std.fetch = fetch;
            let value = fetch.path("Cargo.toml").hash;
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_fetch_path_with_hash_bridge() {
    let result = analyze_source(
        r#"
            use std.fetch = fetch;
            let value = fetch.pathWithHash(
                "Cargo.toml",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ).hash;
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_fetch_url_bridge() {
    let result = analyze_source(
        r#"
            use std.fetch = fetch;
            let value = fetch.url("https://example.com/archive.tar.gz").hash;
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_fetch_url_with_hash_bridge() {
    let result = analyze_source(
        r#"
            use std.fetch = fetch;
            let value = fetch.urlWithHash(
                "https://example.com/archive.tar.gz",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ).hash;
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_fetch_git_bridge() {
    let result = analyze_source(
        r#"
            use std.fetch = fetch;
            let value = fetch.git("/tmp/repo", "main").hash;
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_fetch_git_with_hash_bridge() {
    let result = analyze_source(
        r#"
            use std.fetch = fetch;
            let value = fetch.gitWithHash("/tmp/repo", "main", "0000000000000000000000000000000000000000000000000000000000000000").hash;
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_current_system_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let system = io.currentSystem();
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_current_dir_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let cwd = io.currentDir();
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_get_env_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let value = io.getEnv("HOME");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_hash_file_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let digest = io.hashFile("/tmp/file.txt");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_hash_string_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let digest = io.hashString("abc");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_read_file_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let content = io.readFile("/tmp/file.txt");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_read_dir_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.list = list;
            let entries = list.sort(io.readDir("/tmp"));
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_hash_file_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let digest = io.hashFilePath(path.fromString("/tmp/file.txt"));
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_exec_migrated_process_result_surface() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let result = io.execCommand(io.command("rustc", ["--version"]));
            let shown = toString(result);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_explicit_shell_command_process_result_surface() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let result = io.execCommand(io.command("sh", ["-c", "rustc --version"]));
            let shown = toString(result);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_exec_with_migrated_process_result_surface() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let result = io.execCommand(io.commandWith(#{ program = "rustc", args = ["--version"] }));
            let shown = toString(result);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_read_file_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let content = io.readFilePath(path.fromString("/tmp/file.txt"));
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_read_file_bytes_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let bytes = io.readFileBytesPath(path.fromString("/tmp/file.bin"));
            let shown = toString(bytes);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_read_dir_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            use std.list = list;
            let entries = io.readDirPath(path.fromString("/tmp"));
            let sorted = list.sort(entries);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_sort_and_extrema_builtins() {
    let result = analyze_source(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let sorted = list.sort(io.readDirEntryPaths(path.fromString("/tmp")));
            let hi = list.max([1, 3, 2]);
            let lo = list.min([1, 3, 2]);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_structural_helpers() {
    let result = analyze_source(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries = io.readDirEntryPaths(path.fromString("/tmp"));
            let first = list.head(entries);
            let last = list.last(entries);
            let init = list.init(entries);
            let reversed = list.reverse(entries);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_get_builtin() {
    let result = analyze_source(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries = io.readDirEntryPaths(path.fromString("/tmp"));
            let picked = list.get(0, entries);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_cons_builtin() {
    let result = analyze_source(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries = io.readDirEntryPaths(path.fromString("/tmp"));
            let rooted = list.cons(path.fromString("/"), entries);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_take_builtin() {
    let result = analyze_source(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries = io.readDirEntryPaths(path.fromString("/tmp"));
            let prefix = list.take(2, entries);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_drop_builtin() {
    let result = analyze_source(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries = io.readDirEntryPaths(path.fromString("/tmp"));
            let suffix = list.drop(1, entries);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_contains_builtin() {
    let result = analyze_source(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries = io.readDirEntryPaths(path.fromString("/tmp"));
            let has_root = list.contains(path.fromString("/"), entries);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_index_of_builtin() {
    let result = analyze_source(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries = io.readDirEntryPaths(path.fromString("/tmp"));
            let root_index = list.indexOf(path.fromString("/"), entries);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_sum_builtin() {
    let result = analyze_source(
        r#"
            use std.list = list;
            let total = list.sum([1, 2, 3]);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_product_builtin() {
    let result = analyze_source(
        r#"
            use std.list = list;
            let total = list.product([2, 3, 4]);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_replicate_builtin() {
    let result = analyze_source(
        r#"
            use std.list = list;
            use std.path = path;
            let entries = list.replicate(2, path.fromString("/tmp"));
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_zip_builtin() {
    let result = analyze_source(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let pairs = list.zip(
                io.readDirEntryPaths(path.fromString("/tmp")),
                [1, 2],
            );
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_unzip_builtin() {
    let result = analyze_source(
        r#"
            use std.list = list;
            use std.path = path;
            let pairs = [
                (path.fromString("/tmp"), 1),
                (path.fromString("/var"), 2),
            ];
            let result = list.unzip(pairs);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_list_fold_right_builtin() {
    let result = analyze_source(
        r#"
            use std.list = list;
            fn step(x, acc) = x + acc;
            let total = list.foldRight(0, step, [1, 2, 3]);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_read_dir_entry_paths_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let entries = io.readDirEntryPaths(path.fromString("/tmp"));
            let shown = toString(entries);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_write_file_bytes_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let bytes = io.readFileBytesPath(path.fromString("/tmp/file.bin"));
            let done = io.writeFileBytesPath(path.fromString("/tmp/file.out"), bytes);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_write_file_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let done = io.writeFilePath(path.fromString("/tmp/file.out"), "hello");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_write_file_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let done = io.writeFile("/tmp/file.out", "hello");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_append_file_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let done = io.appendFile("/tmp/file.out", "hello");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_append_file_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let done = io.appendFilePath(path.fromString("/tmp/file.out"), "hello");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_append_file_bytes_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let bytes = io.readFileBytesPath(path.fromString("/tmp/file.bin"));
            let done = io.appendFileBytesPath(path.fromString("/tmp/file.out"), bytes);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_current_dir_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let cwd = io.currentDirPath();
            let shown = toString(cwd);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_home_dir_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let home = io.homeDirPath();
            let shown = toString(home);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_home_dir_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let home = io.homeDir();
            let shown = home ?? "missing";
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_create_dir_all_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let done = io.createDirAll("/tmp/n3v3-dir");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_create_dir_all_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let done = io.createDirAllPath(path.fromString("/tmp/n3v3-dir"));
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_remove_dir_all_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let done = io.removeDirAllPath(path.fromString("/tmp/n3v3-dir"));
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_remove_dir_all_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let done = io.removeDirAll("/tmp/n3v3-dir");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_path_exists_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let exists = io.pathExists("/tmp/file.txt");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_is_dir_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let dir = io.isDir("/tmp");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_is_file_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let file = io.isFile("/tmp/file.txt");
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_command_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let cmd = io.command("printf", ["n3v3"]);
            let shown = toString(cmd);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_command_with_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let cmd = io.commandWith(#{ program = "printf", args = ["n3v3"], cwd = "/tmp" });
            let shown = toString(cmd);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_exec_command_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let result = io.execCommand(io.command("rustc", ["--version"]));
            let shown = toString(result);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_pipeline_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let pipe = io.pipeline([io.command("printf", ["n3v3"]), io.command("cat", [])]);
            let shown = toString(pipe);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_pipeline_with_redirects_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let pipe = io.pipelineWithRedirects(
                io.pipeline([io.command("printf", ["n3v3"]), io.command("cat", [])]),
                [io.redirectStdoutPath(path.fromString("/tmp/n3v3.out"))]
            );
            let shown = toString(pipe);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_exec_pipeline_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let result = io.execPipeline(
                io.pipeline([io.command("printf", ["n3v3"]), io.command("cat", [])])
            );
            let shown = io.processStdout(result);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_exec_pipeline_with_redirect_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let result = io.execPipeline(
                io.pipelineWithRedirects(
                    io.pipeline([io.command("printf", ["n3v3"]), io.command("cat", [])]),
                    [io.redirectStdoutPath(path.fromString("/tmp/n3v3.out"))]
                )
            );
            let shown = io.processCode(result);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_exec_pipeline_with_redirects_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let result = io.execPipeline(
                io.pipelineWithRedirects(
                    io.pipeline([io.command("printf", ["n3v3"]), io.command("cat", [])]),
                    [
                        io.redirectStdoutPath(path.fromString("/tmp/n3v3.out")),
                        io.redirectStderrPath(path.fromString("/tmp/n3v3.err"))
                    ]
                )
            );
            let shown = io.processCode(result);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_command_with_redirects_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let cmd = io.commandWithRedirects(
                io.command("printf", ["n3v3"]),
                [io.redirectStdoutPath(path.fromString("/tmp/n3v3.out"))]
            );
            let shown = toString(cmd);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_redirect_stdout_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let redirect = io.redirectStdoutPath(path.fromString("/tmp/n3v3.out"));
            let shown = toString(redirect);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_redirect_stderr_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let redirect = io.redirectStderrPath(path.fromString("/tmp/n3v3.err"));
            let shown = toString(redirect);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_redirect_stdin_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let redirect = io.redirectStdinPath(path.fromString("/tmp/n3v3.in"));
            let shown = toString(redirect);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_exec_command_with_redirect_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let result = io.execCommand(
                io.commandWithRedirects(
                    io.command("printf", ["n3v3"]),
                    [io.redirectStdoutPath(path.fromString("/tmp/n3v3.out"))]
                )
            );
            let shown = io.processCode(result);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_exec_command_with_redirects_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let result = io.execCommand(
                io.commandWithRedirects(
                    io.command("printf", ["n3v3"]),
                    [
                        io.redirectStdoutPath(path.fromString("/tmp/n3v3.out")),
                        io.redirectStderrPath(path.fromString("/tmp/n3v3.err"))
                    ]
                )
            );
            let shown = io.processCode(result);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_task_command_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let task = io.taskCommand(io.command("printf", ["n3v3"]));
            let shown = toString(task);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_task_pipeline_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let task = io.taskPipeline(io.pipeline([
                io.command("printf", ["n3v3"]),
                io.command("cat", [])
            ]));
            let shown = toString(task);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_await_task_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let task = io.taskCommand(io.command("rustc", ["--version"]));
            let result = io.awaitTask(task);
            let shown = io.processCode(result);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_await_tasks_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let results = io.awaitTasks([
                io.taskCommand(io.command("printf", ["n3v3"])),
                io.taskPipeline(io.pipeline([io.command("printf", ["lang"]), io.command("cat", [])]))
            ]);
            let shown = toString(results);
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_process_success_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let success = io.processSuccess(io.execCommand(io.command("rustc", ["--version"])));
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_process_stdout_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let stdout = io.processStdout(io.execCommand(io.command("rustc", ["--version"])));
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_process_code_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let code = io.processCode(io.execCommand(io.command("rustc", ["--version"])));
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_process_stderr_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            let stderr = io.processStderr(io.execCommand(io.command("rustc", ["--version"])));
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_path_exists_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let exists = io.pathExistsPath(path.fromString("/tmp/file.txt"));
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_is_dir_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let dir = io.isDirPath(path.fromString("/tmp"));
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_io_is_file_path_bridge() {
    let result = analyze_source(
        r#"
            use std.io = io;
            use std.path = path;
            let file = io.isFilePath(path.fromString("/tmp/file.txt"));
        "#,
    );
    // Filter out known warnings (not errors):
    // - callable fallback deprecation
    // - unreachable pattern (binding patterns on literals are conservatively reported)
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

#[test]
fn test_frontend_accepts_std_map_and_set_builtins() {
    let result = analyze_source(
        r#"
            use std.Map;
            use std.Set;
            use std.list = list;
            let map = Map.insert("a", 1, Map.empty);
            let values = Map.values(map);
            let set = Set.insert(1, Set.empty);
            let value = Map.getWithDefault("a", 0, map) + Set.size(set) + list.sum(values);
        "#,
    );
    let non_warning_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == n3v3_diagnostic::Severity::Error
                && !d.message.contains("callable fallback")
        })
        .collect();
    assert!(
        non_warning_diags.is_empty(),
        "unexpected diagnostics: {:?}",
        non_warning_diags
    );
}

// --- Type checker unit tests (M20) ---

#[test]
fn test_typeck_rejects_duplicate_top_level_let() {
    let result = analyze_source(
        r#"
        let x = 1;
        let x = 2;
        "#,
    );
    let has_duplicate = result
        .diagnostics
        .iter()
        .any(|d| d.message.contains("duplicate") || d.message.contains("already defined"));
    assert!(
        has_duplicate,
        "expected duplicate definition error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn test_typeck_rejects_top_level_refutable_pattern_without_binding() {
    let result = analyze_source("let 1 = 2;");
    let has_pattern_error = result.diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains("top-level pattern") && diagnostic.message.contains("bind")
    });
    assert!(
        has_pattern_error,
        "expected top-level pattern diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn test_typeck_auto_infers_effect_for_io_call() {
    let result = analyze_source(
        r#"
        use std.io = io;
        let handler = io.onSignal("INT", fn() { io.print("interrupted!"); () });
        "#,
    );
    let has_lambda_error = result
        .diagnostics
        .iter()
        .any(|d| d.message.contains("in lambda"));
    assert!(
        !has_lambda_error,
        "lambda inside effectful let should be allowed, got {:?}",
        result.diagnostics
    );
}

fn assert_typeck_effect_diagnostics(source: &str, expected_effect_site: Option<&str>) {
    let result = analyze_source(source);
    let (effect_errors, other_errors): (Vec<_>, Vec<_>) = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == n3v3_diagnostic::Severity::Error)
        .partition(|diagnostic| {
            diagnostic.message.contains("effectful call")
                && diagnostic.message.contains("in lambda")
        });
    assert!(
        other_errors.is_empty(),
        "unexpected parse/type errors: {other_errors:?}"
    );
    if let Some(expected_site) = expected_effect_site {
        assert!(
            !effect_errors.is_empty(),
            "expected effect diagnostics for `{expected_site}`, got {:?}",
            result.diagnostics
        );
        assert!(
            effect_errors
                .iter()
                .all(|diagnostic| { source[diagnostic.span.range()].contains(expected_site) }),
            "unexpected effect diagnostic sites: {effect_errors:?}"
        );
    } else {
        assert!(
            effect_errors.is_empty(),
            "unexpected effect diagnostics: {effect_errors:?}"
        );
    }
}

#[test]
fn test_typeck_forward_effect_call_in_nested_lambda_reports_effect() {
    assert_typeck_effect_diagnostics(
        r#"
        use std.io = io;
        |file: String| |suffix: String| load_file(file + suffix);
        fn load_file(file: String) -> String = io.readFile(file);
        "#,
        Some("load_file"),
    );
}

#[test]
fn test_typeck_multihop_effect_call_in_nested_lambda_is_order_independent() {
    let declarations = [
        "fn first(file: String) -> String = second(file);",
        "fn second(file: String) -> String = third(file);",
        "fn third(file: String) -> String = io.readFile(file);",
    ];
    let source_orders = [
        declarations.join("\n"),
        declarations
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n"),
    ];
    for declarations in source_orders {
        let source = format!(
            r#"
            use std.io = io;
            |file: String| |suffix: String| first(file + suffix);
            {declarations}
            "#
        );
        assert_typeck_effect_diagnostics(&source, Some("first"));
    }
}

#[test]
fn test_typeck_recursive_effect_chain_reports_effect_before_definitions() {
    assert_typeck_effect_diagnostics(
        r#"
        use std.io = io;
        |file: String| entry(file);
        fn entry(file: String) -> String = next(file);
        fn next(file: String) -> String =
            if file == "" -> io.readFile(file) else entry("");
        "#,
        Some("entry"),
    );
}

#[test]
fn test_typeck_recursive_pure_chain_does_not_infer_effect() {
    assert_typeck_effect_diagnostics(
        r#"
        |value: Int| entry(value);
        fn entry(value: Int) -> Int = next(value);
        fn next(value: Int) -> Int = if value == 0 -> 0 else entry(value - 1);
        "#,
        None,
    );
}

#[test]
fn test_typeck_explicit_effect_marker_propagates_to_forward_callers() {
    assert_typeck_effect_diagnostics(
        r#"
        |value: Int| first(value);
        fn first(value: Int) -> Int = marked(value);
        effect fn marked(value: Int) -> Int = value;
        "#,
        Some("first"),
    );
}

#[test]
fn test_typeck_forward_effect_chain_allows_enclosed_nested_lambdas() {
    assert_typeck_effect_diagnostics(
        r#"
        use std.io = io;
        fn outer(file: String) -> String = {
            let nested = |prefix: String| |suffix: String| first(prefix + suffix);
            nested(file)("")
        };
        fn first(file: String) -> String = second(file);
        fn second(file: String) -> String = io.readFile(file);
        "#,
        None,
    );
}

#[test]
fn test_typeck_forward_effect_chain_allows_enclosed_impl_lambdas() {
    assert_typeck_effect_diagnostics(
        r#"
        use std.io = io;
        trait Read { fn read(self) -> String; };
        impl Read for String {
            fn read(self) -> String = {
                let nested = |prefix: String| |suffix: String| first(prefix + suffix);
                nested(self)("")
            };
        };
        fn first(file: String) -> String = second(file);
        fn second(file: String) -> String = io.readFile(file);
        "#,
        None,
    );
}
#[test]
fn test_typeck_method_effect_propagates_to_forward_lambda() {
    assert_typeck_effect_diagnostics(
        r#"
        use std.io = io;
        trait Loader { fn load(self) -> String; };
        impl Loader for String {
            fn load(self) -> String = io.readFile(self);
        };
        fn wrapper(path: String) -> String = path.load();
        |path: String| wrapper(path);
        "#,
        Some("wrapper"),
    );
}

#[test]
fn test_typeck_trait_effect_marker_propagates_to_impl_method() {
    assert_typeck_effect_diagnostics(
        r#"
        trait Loader { fn load(self) -> String effect; };
        impl Loader for String {
            fn load(self) -> String = self;
        };
        fn wrapper(path: String) -> String = path.load();
        |path: String| wrapper(path);
        "#,
        Some("wrapper"),
    );
}

#[test]
fn test_typeck_trait_effect_marker_is_order_independent() {
    let source = r#"
        impl Loader for String {
            fn load(self) -> String = self;
        };
        trait Loader { fn load(self) -> String effect; };
        fn wrapper(path: String) -> String = path.load();
        |path: String| wrapper(path);
        "#;
    let result = analyze_source(source);
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("using callable fallback")),
        "trait impl order should not require callable fallback: {:?}",
        result.diagnostics
    );
    assert_typeck_effect_diagnostics(source, Some("wrapper"));
}

#[test]
fn test_typeck_trait_impl_registration_is_order_independent() {
    let result = analyze_source(
        r#"
        impl Loader for String {};
        trait Loader { fn load(self) -> String; };
        "#,
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("missing method `load`")),
        "impl-before-trait should still validate trait completeness: {:?}",
        result.diagnostics
    );
}
#[test]
fn test_typeck_pure_method_named_like_builtin_is_not_effectful() {
    assert_typeck_effect_diagnostics(
        r#"
        trait Reader { fn read(self) -> String; };
        impl Reader for String {
            fn read(self) -> String = self;
        };
        |value: String| value.read();
        "#,
        None,
    );
}

#[test]
fn test_typeck_function_value_references_are_not_effectful_calls() {
    assert_typeck_effect_diagnostics(
        r#"
        use std.io = io;
        fn load(path: String) -> String = io.readFile(path);
        |path: String| (io.readFile, load);
        "#,
        None,
    );
}

#[test]
fn test_typeck_match_guard_in_lambda_reports_effect() {
    assert_typeck_effect_diagnostics(
        r#"
        use std.io = io;
        |file: String| match true {
            true if io.pathExists(file) -> true,
            _ -> false
        };
        "#,
        Some("io.pathExists"),
    );
}

#[test]
fn test_typeck_list_comp_condition_in_lambda_reports_effect() {
    assert_typeck_effect_diagnostics(
        r#"
        use std.io = io;
        |files: List<String>| [file | file <- files, io.pathExists(file)];
        "#,
        Some("io.pathExists"),
    );
}

#[test]
fn test_typeck_match_guard_effect_propagates_to_forward_callers() {
    assert_typeck_effect_diagnostics(
        r#"
        use std.io = io;
        |file: String| first(file);
        fn first(file: String) -> Bool = file_exists(file);
        fn file_exists(file: String) -> Bool = match true {
            true if io.pathExists(file) -> true,
            _ -> false
        };
        "#,
        Some("first"),
    );
}

#[test]
fn test_typeck_list_comp_condition_effect_propagates_to_forward_callers() {
    assert_typeck_effect_diagnostics(
        r#"
        use std.io = io;
        |files: List<String>| first(files);
        fn first(files: List<String>) -> List<String> = existing(files);
        fn existing(files: List<String>) -> List<String> =
            [file | file <- files, io.pathExists(file)];
        "#,
        Some("first"),
    );
}
