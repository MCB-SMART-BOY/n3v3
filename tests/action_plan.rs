//! Integration tests for typed action plans and the explicit host boundary.
//! 类型化动作计划与显式宿主边界的集成测试。

use n3v3_common::{FakeHost, HostOp, HostOpKind, HostValue};
use n3v3_frontend::analyze_source;

#[test]
fn test_frontend_collects_read_file_plan_without_host_execution() {
    let analysis = analyze_source(
        r#"
            use std.io = io;
            let content = io.readFile("config.n3v3");
        "#,
    );
    assert!(
        !analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == n3v3_diagnostic::Severity::Error),
        "unexpected diagnostics: {:?}",
        analysis.diagnostics
    );

    assert_eq!(analysis.semantics.action_plans.len(), 1);
    let plan = analysis
        .semantics
        .action_plans
        .values()
        .next()
        .expect("readFile action plan");
    assert_eq!(plan.host_op_kind(), HostOpKind::ReadFile);
    assert_eq!(plan.effects(), n3v3_common::EffectSummary::FILE_READ);
    assert!(matches!(
        plan.operation(),
        HostOp::ReadFile { path } if path == "config.n3v3"
    ));

    let mut host = FakeHost::new();
    host.insert_file("config.n3v3", "planned-content");
    assert!(host.read_calls().is_empty());

    let value = plan.execute(&mut host).expect("fake host read");

    assert_eq!(value, HostValue::String("planned-content".to_string()));
    assert_eq!(host.read_calls(), ["config.n3v3"]);
}

#[test]
fn test_frontend_skips_dynamic_read_file_plan() {
    let analysis = analyze_source(
        r#"
            use std.io = io;
            let path = "config.n3v3";
            let content = io.readFile(path);
        "#,
    );

    assert!(analysis.semantics.action_plans.is_empty());
}

#[test]
fn test_frontend_hides_action_plan_when_type_check_fails() {
    let analysis = analyze_source(
        r#"
            use std.io = io;
            let content: Int = io.readFile("config.n3v3");
        "#,
    );

    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == n3v3_diagnostic::Severity::Error)
    );
    assert!(analysis.semantics.action_plans.is_empty());
}
#[test]
fn test_read_file_action_plan_propagates_missing_file_error() {
    let analysis = analyze_source(
        r#"
            use std.io = io;
            let content = io.readFile("missing.n3v3");
        "#,
    );
    let plan = analysis
        .semantics
        .action_plans
        .values()
        .next()
        .expect("readFile action plan");
    let mut host = FakeHost::new();

    let error = plan
        .execute(&mut host)
        .expect_err("missing file should fail");

    assert!(error.to_string().contains("missing.n3v3"));
    assert!(std::error::Error::source(&error).is_some());
}
