//! Integration tests for compatibility frontend session APIs.
//! 兼容前端会话 API 的集成测试。

mod support;

use n3v3_diagnostic::{DiagnosticKind, Severity};
use n3v3_frontend::{
    FrontendSession, SessionBuildInputs, SessionCheckError, SessionDisplayError,
    SessionModuleContext, SessionSourceCheckError, SessionVisibleState,
};
use n3v3_parser::parse;
use support::module_fixtures::create_test_module;
use tempfile::TempDir;

fn parse_ok(source: &str) -> n3v3_syntax::SourceFile {
    let (ast, diagnostics) = parse(source);
    assert!(
        diagnostics.is_empty(),
        "unexpected parse diagnostics: {diagnostics:?}"
    );
    ast
}

#[test]
fn test_frontend_session_builds_in_memory_module_against_loaded_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["math"], "fn add(x, y) = x + y;");

    let ast = parse_ok(
        r#"
            use math (add);
            fn compute() = add(1, 2);
        "#,
    );

    let mut session = FrontendSession::new(temp_dir.path());
    let build = session
        .build_module_from_ast(
            &ast,
            "repl".to_string(),
            Vec::new(),
            &SessionBuildInputs::default(),
        )
        .expect("session build should succeed");

    assert_eq!(
        build.newly_loaded.len(),
        1,
        "expected one loaded dependency"
    );
    assert!(
        !build.global_defs.is_empty(),
        "expected current module globals"
    );

    let analysis = session.analyze_module(&build.module);
    assert!(
        analysis.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analysis.diagnostics
    );
}

#[test]
fn test_frontend_session_does_not_leak_dependency_definition_names() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["dependency"], "fn compute() = 1;");

    let ast = parse_ok(
        r#"
            use dependency;
            fn compute() = 2;
        "#,
    );
    let mut session = FrontendSession::new(temp_dir.path());
    let build = session
        .build_module_from_ast(
            &ast,
            "repl".to_string(),
            Vec::new(),
            &SessionBuildInputs::default(),
        )
        .expect("session build should succeed");

    let analysis = session.analyze_module(&build.module);
    assert!(
        analysis.diagnostics.iter().all(|diagnostic| {
            diagnostic.kind != DiagnosticKind::Parser
                || !diagnostic.message.contains("duplicate definition")
        }),
        "dependency definitions leaked into current-module diagnostics: {:?}",
        analysis.diagnostics
    );
}

#[test]
fn test_frontend_session_resolves_trait_impls_across_modules() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(
        temp_dir.path(),
        &["traits"],
        "trait Show { fn show(self) -> String; };",
    );
    create_test_module(
        temp_dir.path(),
        &["impls"],
        r#"
            use traits (Show);
            impl Show for String {
                fn show(self) -> String = self;
            };
        "#,
    );

    let ast = parse_ok(
        r#"
            use traits (Show);
            use impls;
            fn run() -> String = "ok".show();
        "#,
    );
    let mut session = FrontendSession::new(temp_dir.path());
    let build = session
        .build_module_from_ast(
            &ast,
            "repl".to_string(),
            Vec::new(),
            &SessionBuildInputs::default(),
        )
        .expect("session build should succeed");

    let analysis = session.analyze_module(&build.module);
    assert!(
        analysis
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error),
        "unexpected session trait diagnostics: {:?}",
        analysis.diagnostics
    );
}

#[test]
fn test_frontend_session_resolves_imported_bindings_and_module_aliases() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["math"], "fn add(x, y) = x + y;");

    let ast = parse_ok(
        r#"
            use math (add);
            use math = ops;
            fn compute() = ops.add(add(1, 2), 3);
        "#,
    );

    let mut session = FrontendSession::new(temp_dir.path());
    session
        .build_module_from_ast(
            &ast,
            "repl".to_string(),
            Vec::new(),
            &SessionBuildInputs::default(),
        )
        .expect("session build should succeed");

    let resolved = session
        .resolve_ast_imports(&ast, &[])
        .expect("use resolution should succeed");

    assert!(
        resolved.bindings.iter().any(|(name, _)| name == "add"),
        "expected imported item binding, got {:?}",
        resolved.bindings
    );
    assert!(
        resolved.module_aliases.iter().any(|alias| alias == "ops"),
        "expected imported module alias, got {:?}",
        resolved.module_aliases
    );
}

#[test]
fn test_frontend_session_resolves_builtin_std_import_bookkeeping() {
    let temp_dir = TempDir::new().unwrap();
    let ast = parse_ok(
        r#"
            use std.list (len);
            use std.list = list_ops;
            use std (*);
            fn compute() = list_ops.len([1, 2]) + len([3, 4]);
        "#,
    );

    let session = FrontendSession::new(temp_dir.path());
    let resolved = session
        .resolve_ast_imports(&ast, &[])
        .expect("builtin std import resolution should succeed");

    assert!(
        resolved
            .builtin_item_imports
            .iter()
            .any(|(name, builtin)| name == "len" && builtin == "list.len"),
        "expected builtin item import, got {:?}",
        resolved.builtin_item_imports
    );
    assert!(
        resolved
            .builtin_module_imports
            .iter()
            .any(|(alias, module)| alias == "list_ops" && module == "list"),
        "expected builtin module alias, got {:?}",
        resolved.builtin_module_imports
    );
    assert!(
        resolved
            .builtin_module_imports
            .iter()
            .any(|(alias, module)| alias == "math" && module == "math"),
        "expected root std glob module import, got {:?}",
        resolved.builtin_module_imports
    );
}

#[test]
fn test_frontend_session_prepares_module_and_resolves_imports_in_one_step() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["math"], "fn add(x, y) = x + y;");

    let ast = parse_ok(
        r#"
            use math (add);
            use math = ops;
            use std.list = list_ops;
            use std.list (len);
            fn compute() = ops.add(len([1, 2]), list_ops.len([3, 4]));
        "#,
    );

    let mut session = FrontendSession::new(temp_dir.path());
    let prepared = session
        .prepare_module_from_ast(
            &ast,
            "repl".to_string(),
            Vec::new(),
            &SessionBuildInputs::default(),
        )
        .expect("session prepare should succeed");

    assert_eq!(
        prepared.newly_loaded.len(),
        1,
        "expected one loaded dependency"
    );
    assert!(
        !prepared.global_defs.is_empty(),
        "expected current module globals"
    );
    assert!(
        prepared
            .resolved_imports
            .bindings
            .iter()
            .any(|(name, _)| name == "add"),
        "expected imported binding, got {:?}",
        prepared.resolved_imports.bindings
    );
    assert!(
        prepared
            .resolved_imports
            .module_aliases
            .iter()
            .any(|alias| alias == "ops"),
        "expected imported module alias, got {:?}",
        prepared.resolved_imports.module_aliases
    );
    assert!(
        prepared
            .resolved_imports
            .builtin_item_imports
            .iter()
            .any(|(name, builtin)| name == "len" && builtin == "list.len"),
        "expected builtin item import, got {:?}",
        prepared.resolved_imports.builtin_item_imports
    );
    assert!(
        prepared
            .resolved_imports
            .builtin_module_imports
            .iter()
            .any(|(alias, module)| alias == "list_ops" && module == "list"),
        "expected builtin module alias, got {:?}",
        prepared.resolved_imports.builtin_module_imports
    );
}

#[test]
fn test_frontend_session_prepares_checked_module_in_one_step() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["math"], "fn add(x, y) = x + y;");

    let ast = parse_ok(
        r#"
            use math (add);
            use std.list = list_ops;
            fn compute() = list_ops.len([add(1, 2)]);
        "#,
    );

    let mut session = FrontendSession::new(temp_dir.path());
    let checked = session
        .prepare_checked_module_with_context(
            &ast,
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect("checked session prepare should succeed");

    assert_eq!(
        checked.prepared.newly_loaded.len(),
        1,
        "expected one loaded dependency"
    );
    assert!(
        checked.analysis.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        checked.analysis.diagnostics
    );
    assert!(
        checked
            .prepared
            .resolved_imports
            .bindings
            .iter()
            .any(|(name, _)| name == "add"),
        "expected imported binding, got {:?}",
        checked.prepared.resolved_imports.bindings
    );
}

#[test]
fn test_frontend_session_parses_and_checks_source_in_one_step() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["math"], "fn add(x, y) = x + y;");

    let mut session = FrontendSession::new(temp_dir.path());
    let checked = session
        .parse_checked_source_with_context(
            r#"
                use math (add);
                fn compute() = add(1, 2);
            "#,
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect("checked source parse should succeed");

    assert!(
        !checked.ast.items.is_empty(),
        "expected parsed AST items for checked source"
    );
    assert!(
        checked.checked.analysis.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        checked.checked.analysis.diagnostics
    );
}

#[test]
fn test_frontend_session_parse_checked_source_reports_parse_diagnostics() {
    let temp_dir = TempDir::new().unwrap();
    let mut session = FrontendSession::new(temp_dir.path());

    let error = session
        .parse_checked_source_with_context(
            "let x = ;",
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect_err("parse-broken source should fail before checked-module preparation");

    let SessionSourceCheckError::ParseDiagnostics(diagnostics) = error else {
        panic!("expected parse diagnostics, got {error:?}");
    };
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Parser && diag.severity == Severity::Error),
        "expected parser diagnostics, got {:?}",
        diagnostics
    );
}

#[test]
fn test_frontend_session_source_check_error_projects_parse_diagnostics_for_display() {
    let temp_dir = TempDir::new().unwrap();
    let mut session = FrontendSession::new(temp_dir.path());

    let error = session
        .parse_checked_source_with_context(
            "let x = ;",
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect_err("parse-broken source should fail before checked-module preparation")
        .into_display_error("let x = ;", "let x = ;");

    let SessionDisplayError::Diagnostics {
        source_name,
        source,
        diagnostics,
    } = error
    else {
        panic!("expected source-attributed diagnostics, got {error:?}");
    };
    assert_eq!(source_name, "let x = ;");
    assert_eq!(source, "let x = ;");
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Parser && diag.severity == Severity::Error),
        "expected parser diagnostics, got {:?}",
        diagnostics
    );
}

#[test]
fn test_frontend_session_parses_checked_source_with_explicit_display_name() {
    let temp_dir = TempDir::new().unwrap();
    let mut session = FrontendSession::new(temp_dir.path());

    let error = session
        .parse_checked_source_with_context_for_display_as(
            "<snippet>",
            "let x = ;",
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect_err("parse-broken source should surface the explicit display name");

    let SessionDisplayError::Diagnostics {
        source_name,
        source,
        diagnostics,
    } = error
    else {
        panic!("expected source-attributed diagnostics, got {error:?}");
    };
    assert_eq!(source_name, "<snippet>");
    assert_eq!(source, "let x = ;");
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Parser && diag.severity == Severity::Error),
        "expected parser diagnostics, got {:?}",
        diagnostics
    );
}

#[test]
fn test_frontend_session_parses_checked_source_with_context_for_display() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["math"], "fn add(x, y) = x + y;");

    let mut session = FrontendSession::new(temp_dir.path());
    let checked = session
        .parse_checked_source_with_context_for_display(
            r#"
                use math (add);
                fn compute() = add(1, 2);
            "#,
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect("checked source parse-for-display should succeed");

    assert!(
        !checked.ast.items.is_empty(),
        "expected parsed AST items for checked source"
    );
    assert!(
        checked.checked.analysis.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        checked.checked.analysis.diagnostics
    );
}

#[test]
fn test_frontend_session_parses_checks_and_formats_binding_type_in_one_step() {
    let temp_dir = TempDir::new().unwrap();
    let mut session = FrontendSession::new(temp_dir.path());

    let formatted = session
        .parse_format_binding_type_with_context(
            "struct User {}; fn id(x: User) -> User = x;",
            "id",
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect("binding-type formatting should succeed");

    assert_eq!(formatted, Some("(User) -> User".to_string()));
}

#[test]
fn test_frontend_session_prepares_repl_expression_source_and_marks_ephemeral() {
    let prepared = FrontendSession::prepare_repl_source("1 + 2");

    assert_eq!(prepared.source, "let __expr__ = 1 + 2;");
    assert!(
        !prepared.persist_defs,
        "wrapped expression inputs should stay ephemeral"
    );
}

#[test]
fn test_frontend_session_prepares_repl_input_with_source_name_and_context() {
    let temp_dir = TempDir::new().unwrap();
    let session = FrontendSession::new(temp_dir.path());
    let prepared = session.prepare_repl_input("1 + 2");

    assert_eq!(prepared.source_name, "<repl>");
    assert_eq!(prepared.source, "let __expr__ = 1 + 2;");
    assert!(
        !prepared.persist_defs,
        "wrapped expression inputs should stay ephemeral"
    );
    assert_eq!(prepared.context.root_dir, None);
    assert_eq!(prepared.context.module_path, Vec::<String>::new());
    assert_eq!(prepared.context.module_name, "repl");
}

#[test]
fn test_frontend_session_prepares_repl_item_source_and_marks_persistent() {
    let prepared = FrontendSession::prepare_repl_source("let answer = 42");

    assert_eq!(prepared.source, "let answer = 42;");
    assert!(
        prepared.persist_defs,
        "item inputs should keep persistent REPL semantics"
    );
}

#[test]
fn test_frontend_session_parses_and_formats_repl_type_query_in_one_step() {
    let temp_dir = TempDir::new().unwrap();
    let mut session = FrontendSession::new(temp_dir.path());

    let formatted = session
        .parse_format_repl_type("1 + 2", &SessionVisibleState::default())
        .expect("REPL type query formatting should succeed");

    assert_eq!(formatted, Some("Int".to_string()));
}

#[test]
fn test_frontend_session_returns_canonical_repl_context() {
    let temp_dir = TempDir::new().unwrap();
    let session = FrontendSession::new(temp_dir.path());
    let context = session.repl_context();

    assert_eq!(context.root_dir, None);
    assert_eq!(context.module_path, Vec::<String>::new());
    assert_eq!(context.module_name, "repl");
}

#[test]
fn test_frontend_session_repl_type_query_projects_display_diagnostics() {
    let temp_dir = TempDir::new().unwrap();
    let mut session = FrontendSession::new(temp_dir.path());

    let error = session
        .parse_format_repl_type_for_display("41?", &SessionVisibleState::default())
        .expect_err("invalid REPL type query should surface display diagnostics");

    let SessionDisplayError::Diagnostics {
        source_name,
        source,
        diagnostics,
    } = error
    else {
        panic!("expected source-attributed diagnostics, got {error:?}");
    };
    assert_eq!(source_name, "<repl:type>");
    assert_eq!(source, "let __type__ = 41?;");
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Type && diag.severity == Severity::Error),
        "expected type diagnostics, got {:?}",
        diagnostics
    );
}

#[test]
fn test_frontend_session_prepared_module_skips_hidden_expr_bindings() {
    let temp_dir = TempDir::new().unwrap();
    let ast = parse_ok("let __expr__ = 1 + 2;");

    let mut session = FrontendSession::new(temp_dir.path());
    let prepared = session
        .prepare_module_with_context(
            &ast,
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect("prepared module should succeed");

    assert!(
        prepared.defined_bindings.is_empty(),
        "hidden expression bindings should not be exposed, got {:?}",
        prepared.defined_bindings
    );
}

#[test]
fn test_frontend_session_formats_checked_binding_type_for_direct_global_reference() {
    let temp_dir = TempDir::new().unwrap();
    let ast = parse_ok(
        r#"
            let x = 41;
            let __type__ = x;
        "#,
    );

    let mut session = FrontendSession::new(temp_dir.path());
    let checked = session
        .prepare_checked_module_with_context(
            &ast,
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect("checked session prepare should succeed");

    assert_eq!(
        session.format_checked_binding_type(&checked, "__type__"),
        Some("Int".to_string())
    );
}

#[test]
fn test_frontend_session_formats_named_checked_binding_types_with_current_module_names() {
    let temp_dir = TempDir::new().unwrap();
    let ast = parse_ok("struct User {}; fn id(x: User) -> User = x;");

    let mut session = FrontendSession::new(temp_dir.path());
    let checked = session
        .prepare_checked_module_with_context(
            &ast,
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect("checked session prepare should succeed");

    assert_eq!(
        session.format_checked_binding_type(&checked, "id"),
        Some("(User) -> User".to_string())
    );
}

#[test]
fn test_frontend_session_prepare_checked_module_reports_loaded_module_errors() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["broken"], "fn bad() = 1 + true;");

    let ast = parse_ok(
        r#"
            use broken (bad);
            fn run() = 1;
        "#,
    );

    let mut session = FrontendSession::new(temp_dir.path());
    let error = session
        .prepare_checked_module_with_context(
            &ast,
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect_err("broken dependency should block checked prepare");

    let SessionCheckError::LoadedModules(entries) = error else {
        panic!("expected loaded-module diagnostics, got {error:?}");
    };
    assert_eq!(entries.len(), 1, "expected one broken dependency");
    assert!(entries[0].file_path.ends_with("broken.n3v3"));
    assert!(
        entries[0]
            .diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Type && diag.severity == Severity::Error),
        "expected type error diagnostics, got {:?}",
        entries[0].diagnostics
    );
}

#[test]
fn test_frontend_session_check_error_projects_loaded_module_diagnostics_for_display() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["broken"], "fn bad() = 1 + true;");

    let ast = parse_ok(
        r#"
            use broken (bad);
            fn run() = 1;
        "#,
    );

    let mut session = FrontendSession::new(temp_dir.path());
    let error = session
        .prepare_checked_module_with_context(
            &ast,
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect_err("broken dependency should block checked prepare")
        .into_display_error("fn run() = 1;", "fn run() = 1;");

    let SessionDisplayError::LoadedModules(entries) = error else {
        panic!("expected loaded-module display error, got {error:?}");
    };
    assert_eq!(entries.len(), 1, "expected one broken dependency");
    assert!(entries[0].file_path.ends_with("broken.n3v3"));
    assert!(
        entries[0]
            .diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Type && diag.severity == Severity::Error),
        "expected type error diagnostics, got {:?}",
        entries[0].diagnostics
    );
}

#[test]
fn test_frontend_session_prepare_checked_module_reports_current_module_errors() {
    let temp_dir = TempDir::new().unwrap();
    let ast = parse_ok("fn bad() = 1 + true;");

    let mut session = FrontendSession::new(temp_dir.path());
    let error = session
        .prepare_checked_module_with_context(
            &ast,
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect_err("broken current module should block checked prepare");

    let SessionCheckError::ModuleDiagnostics(diagnostics) = error else {
        panic!("expected current-module diagnostics, got {error:?}");
    };
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Type && diag.severity == Severity::Error),
        "expected type error diagnostics, got {:?}",
        diagnostics
    );
}

#[test]
fn test_frontend_session_check_error_projects_current_module_diagnostics_for_display() {
    let temp_dir = TempDir::new().unwrap();
    let ast = parse_ok("fn bad() = 1 + true;");

    let mut session = FrontendSession::new(temp_dir.path());
    let error = session
        .prepare_checked_module_with_context(
            &ast,
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect_err("broken current module should block checked prepare")
        .into_display_error("fn bad() = 1 + true;", "fn bad() = 1 + true;");

    let SessionDisplayError::Diagnostics {
        source_name,
        source,
        diagnostics,
    } = error
    else {
        panic!("expected source-attributed diagnostics, got {error:?}");
    };
    assert_eq!(source_name, "fn bad() = 1 + true;");
    assert_eq!(source, "fn bad() = 1 + true;");
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Type && diag.severity == Severity::Error),
        "expected type error diagnostics, got {:?}",
        diagnostics
    );
}

#[test]
fn test_frontend_session_commit_prepared_module_carries_visible_state_between_inputs() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["math"], "fn add(x, y) = x + y;");

    let mut session = FrontendSession::new(temp_dir.path());
    let mut visible_state = SessionVisibleState::default();

    let first = parse_ok(
        r#"
            use math (add);
            use std.list = list_ops;
            fn compute() = add(1, 2);
        "#,
    );
    let first_prepared = session
        .prepare_module_with_visible_state(&first, "repl".to_string(), Vec::new(), &visible_state)
        .expect("first prepare should succeed");
    session.commit_prepared_module(&mut visible_state, first_prepared);
    assert_eq!(
        session.persisted_modules().len(),
        1,
        "expected committed module to be persisted in the session"
    );

    let second = parse_ok(
        r#"
            fn again() = list_ops.len([add(1, 2)]);
        "#,
    );
    let second_prepared = session
        .prepare_module_with_visible_state(&second, "repl".to_string(), Vec::new(), &visible_state)
        .expect("second prepare should succeed");
    let analysis = session.analyze_module(&second_prepared.module);

    assert!(
        analysis.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analysis.diagnostics
    );
    assert!(
        second_prepared.global_defs.contains_key("again"),
        "expected current module globals"
    );
}

#[test]
fn test_frontend_session_returns_loaded_modules_in_dependency_order() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["util"], "fn inc(x) = x + 1;");
    create_test_module(
        temp_dir.path(),
        &["math"],
        "use util (inc); fn add_one(x) = inc(x);",
    );

    let ast = parse_ok(
        r#"
            use math (add_one);
            fn compute() = add_one(1);
        "#,
    );

    let mut session = FrontendSession::new(temp_dir.path());
    let build = session
        .build_module_from_ast(
            &ast,
            "repl".to_string(),
            Vec::new(),
            &SessionBuildInputs::default(),
        )
        .expect("session build should succeed");

    assert_eq!(
        build.newly_loaded.len(),
        2,
        "expected transitive dependencies to load"
    );

    let loaded = session.loaded_modules_in_order();
    assert_eq!(loaded.len(), 2, "expected two loaded dependency modules");
    assert!(loaded[0].file_path.ends_with("util.n3v3"));
    assert!(loaded[1].file_path.ends_with("math.n3v3"));
    assert!(
        loaded.iter().all(|entry| entry.module.is_some()),
        "expected lowered HIR for loaded modules, got {:?}",
        loaded
    );
    assert!(
        loaded
            .iter()
            .all(|entry| entry.analysis.diagnostics.is_empty()),
        "unexpected diagnostics in loaded modules: {:?}",
        loaded
    );
}

#[test]
fn test_frontend_session_returns_only_evaluable_loaded_modules_in_dependency_order() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["util"], "fn inc(x) = x + 1;");
    create_test_module(temp_dir.path(), &["broken"], "fn bad() = 1 + true;");

    let ast = parse_ok(
        r#"
            use util (inc);
            use broken (bad);
            fn compute() = inc(1);
        "#,
    );

    let mut session = FrontendSession::new(temp_dir.path());
    session
        .build_module_from_ast(
            &ast,
            "repl".to_string(),
            Vec::new(),
            &SessionBuildInputs::default(),
        )
        .expect("session build should succeed");

    let evaluable = session.evaluable_loaded_modules_in_order();
    assert_eq!(
        evaluable.len(),
        1,
        "expected only the clean dependency to be evaluable"
    );
    assert!(
        !evaluable[0].module.items.is_empty(),
        "expected lowered HIR items for the clean dependency"
    );
    assert!(
        evaluable[0].method_resolutions.is_empty(),
        "expected no method resolutions for the simple helper module, got {:?}",
        evaluable[0].method_resolutions
    );
}

#[test]
fn test_frontend_session_hides_clean_dependency_action_plans_when_another_dependency_fails() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(
        temp_dir.path(),
        &["clean"],
        r#"
            use std.io = io;
            let content = io.readFile("config.n3v3");
        "#,
    );
    create_test_module(temp_dir.path(), &["broken"], "fn bad() = 1 + true;");

    let ast = parse_ok(
        r#"
            use clean (content);
            use broken (bad);
            fn compute() = 1;
        "#,
    );

    let mut session = FrontendSession::new(temp_dir.path());
    let build = session
        .build_module_from_ast(
            &ast,
            "repl".to_string(),
            Vec::new(),
            &SessionBuildInputs::default(),
        )
        .expect("session build should succeed");
    let current_analysis = session.analyze_module(&build.module);
    assert!(
        current_analysis.semantics.action_plans.is_empty(),
        "current action plans must be hidden when a dependency fails: {:?}",
        current_analysis
    );

    let loaded = session.loaded_modules_in_order();
    assert!(
        loaded.iter().any(|entry| {
            entry
                .analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error)
        }),
        "expected one dependency to have a blocking diagnostic: {:?}",
        loaded
    );
    assert!(
        loaded
            .iter()
            .all(|entry| entry.analysis.semantics.action_plans.is_empty()),
        "clean dependency action plans must be hidden when another dependency fails: {:?}",
        loaded
    );
}

#[test]
fn test_frontend_session_rechecks_prior_dependency_errors_before_new_input() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["broken"], "fn bad() = 1 + true;");

    let mut session = FrontendSession::new(temp_dir.path());
    let first = parse_ok("use broken (bad); fn first() = 1;");
    let first_error = session
        .prepare_checked_module_with_context(
            &first,
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect_err("the first input should reject the broken dependency");
    assert!(matches!(first_error, SessionCheckError::LoadedModules(_)));

    let second = parse_ok(
        r#"
            use std.io = io;
            let content = io.readFile("config.n3v3");
        "#,
    );
    let second_error = session
        .prepare_checked_module_with_context(
            &second,
            &SessionModuleContext::repl(),
            &SessionVisibleState::default(),
        )
        .expect_err("a prior broken dependency must block later checked input");
    let SessionCheckError::LoadedModules(entries) = second_error else {
        panic!("expected prior dependency diagnostics, got {second_error:?}");
    };
    assert_eq!(entries.len(), 1);
    assert!(entries[0].file_path.ends_with("broken.n3v3"));
}

#[test]
fn test_frontend_session_resolves_file_module_context_under_current_root() {
    let temp_dir = TempDir::new().unwrap();
    std::fs::create_dir_all(temp_dir.path().join("app")).unwrap();
    std::fs::write(
        temp_dir.path().join("app").join("mod.n3v3"),
        "let answer = 42;",
    )
    .unwrap();

    let mut session = FrontendSession::new(temp_dir.path());
    let context = session
        .repl_context_for_file(temp_dir.path().join("app").join("mod.n3v3"), false)
        .expect("module context should resolve");

    assert_eq!(context.root_dir.as_deref(), Some(session.root_dir()));
    assert_eq!(context.module_path, vec!["app".to_string()]);
    assert_eq!(context.module_name, "app");
}

#[test]
fn test_frontend_session_loads_repl_file_input_under_current_root() {
    let temp_dir = TempDir::new().unwrap();
    std::fs::create_dir_all(temp_dir.path().join("app")).unwrap();
    std::fs::write(
        temp_dir.path().join("app").join("mod.n3v3"),
        "let answer = 42;",
    )
    .unwrap();

    let mut session = FrontendSession::new(temp_dir.path());
    let input = session
        .load_repl_file_input(temp_dir.path().join("app").join("mod.n3v3"), false)
        .expect("REPL file input should load");

    assert_eq!(
        input.source_name,
        temp_dir
            .path()
            .join("app")
            .join("mod.n3v3")
            .display()
            .to_string(),
        "expected file-backed REPL source name to match the user-facing path"
    );
    assert_eq!(
        input.file_path,
        temp_dir.path().join("app").join("mod.n3v3"),
        "expected user-facing file path to be preserved"
    );
    assert_eq!(input.source, "let answer = 42;");
    assert_eq!(input.context.root_dir.as_deref(), Some(session.root_dir()));
    assert_eq!(input.context.module_path, vec!["app".to_string()]);
    assert_eq!(input.context.module_name, "app");
}

#[test]
fn test_frontend_session_load_repl_file_input_reports_read_error() {
    let temp_dir = TempDir::new().unwrap();
    let mut session = FrontendSession::new(temp_dir.path());
    let missing = temp_dir.path().join("missing.n3v3");

    let error = session
        .load_repl_file_input(&missing, false)
        .expect_err("missing REPL file should surface a display error");

    let SessionDisplayError::Message(message) = error else {
        panic!("expected read failure message, got {error:?}");
    };
    assert!(
        message.contains("Cannot read file"),
        "expected read failure prefix, got {message}"
    );
    assert!(
        message.contains("missing.n3v3"),
        "expected missing file path, got {message}"
    );
}

#[test]
fn test_frontend_session_can_rebase_root_when_pristine() {
    let first = TempDir::new().unwrap();
    let second = TempDir::new().unwrap();
    std::fs::create_dir_all(second.path().join("app")).unwrap();
    std::fs::write(
        second.path().join("app").join("mod.n3v3"),
        "let answer = 42;",
    )
    .unwrap();

    let mut session = FrontendSession::new(first.path());
    let context = session
        .repl_context_for_file(second.path().join("app").join("mod.n3v3"), true)
        .expect("context should rebase to new root");

    assert_eq!(context.root_dir.as_deref(), Some(session.root_dir()));
    assert_eq!(
        session.root_dir(),
        second.path().join("app").canonicalize().unwrap().as_path()
    );
    assert_eq!(context.module_path, Vec::<String>::new());
}

#[test]
fn test_frontend_session_rejects_root_switch_when_not_pristine() {
    let first = TempDir::new().unwrap();
    let second = TempDir::new().unwrap();
    std::fs::write(second.path().join("main.n3v3"), "let answer = 42;").unwrap();

    let mut session = FrontendSession::new(first.path());
    session.record_module(n3v3_frontend::analyze_source("let x = 1;").hir);

    let error = session
        .repl_context_for_file(second.path().join("main.n3v3"), true)
        .expect_err("non-empty sessions should not silently mix project roots");

    assert!(
        error.to_string().contains(":clear"),
        "expected guidance to clear the session, got {error}"
    );
}

#[test]
fn test_frontend_session_attributes_diagnostics_for_newly_loaded_modules() {
    let temp_dir = TempDir::new().unwrap();
    create_test_module(temp_dir.path(), &["broken"], "fn bad() = 1 + true;");

    let ast = parse_ok(
        r#"
            use broken (bad);
            fn run() = 1;
        "#,
    );

    let mut session = FrontendSession::new(temp_dir.path());
    let build = session
        .build_module_from_ast(
            &ast,
            "repl".to_string(),
            Vec::new(),
            &SessionBuildInputs::default(),
        )
        .expect("session build should succeed");

    let diagnostics = session.loaded_module_diagnostics(&build.newly_loaded);
    assert_eq!(diagnostics.len(), 1, "expected one broken dependency");
    assert!(diagnostics[0].file_path.ends_with("broken.n3v3"));
    assert!(
        diagnostics[0].source.contains("fn bad() = 1 + true;"),
        "expected projected source text, got {:?}",
        diagnostics[0].source
    );
    assert!(
        diagnostics[0]
            .diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Type && diag.severity == Severity::Error),
        "expected type error diagnostics, got {:?}",
        diagnostics[0].diagnostics
    );
}

#[test]
fn test_frontend_session_checked_module_analysis_returns_errors() {
    let temp_dir = TempDir::new().unwrap();
    let ast = parse_ok("fn bad() = 1 + true;");

    let mut session = FrontendSession::new(temp_dir.path());
    let build = session
        .build_module_from_ast(
            &ast,
            "repl".to_string(),
            Vec::new(),
            &SessionBuildInputs::default(),
        )
        .expect("session build should succeed");

    let diagnostics = session
        .analyze_module_checked(&build.module)
        .expect_err("broken current module should report semantic errors");

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.kind == DiagnosticKind::Type && diag.severity == Severity::Error),
        "expected type error diagnostics, got {:?}",
        diagnostics
    );
}
