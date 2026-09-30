//! Integration tests for n3v3-typeck crate.
//!
//! This file contains extensive edge case tests for type checking.

use n3v3_diagnostic::{Diagnostic, ErrorCode, Severity};
use n3v3_hir::lower;
use n3v3_parser::parse;
use n3v3_typeck::{TypeChecker, format_type};

fn check_source(source: &str) -> Vec<Diagnostic> {
    let (ast, parse_diags) = parse(source);
    if !parse_diags.is_empty() {
        // Return parse errors as diagnostics for tests that check parse failures
        return parse_diags;
    }

    let hir = lower(&ast);
    let mut checker = TypeChecker::new();
    checker.check(&hir);
    checker.diagnostics()
}

fn check_no_errors(source: &str) {
    let diags = check_source(source);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
}

fn check_has_errors(source: &str) {
    let diags = check_source(source);
    assert!(!diags.is_empty(), "expected type errors but got none");
}

fn assert_has_diagnostic(source: &str, severity: Severity, message_fragment: &str) {
    let diags = check_source(source);
    assert!(
        diags
            .iter()
            .any(|diag| { diag.severity == severity && diag.message.contains(message_fragment) }),
        "expected diagnostic containing '{message_fragment}', got {:?}",
        diags
    );
}

fn assert_warning_previous_label_contains(
    source: &str,
    message_fragment: &str,
    previous_fragment: &str,
) {
    let diags = check_source(source);
    let diag = diags
        .iter()
        .find(|diag| diag.severity == Severity::Warning && diag.message.contains(message_fragment))
        .unwrap_or_else(|| {
            panic!(
                "expected warning containing '{message_fragment}', got {:?}",
                diags
            )
        });
    assert!(
        diag.labels.len() >= 2,
        "expected unreachable warning to carry previous-pattern label, got {:?}",
        diag
    );
    let previous_snippet = source.get(diag.labels[1].span.range()).unwrap_or("");
    assert!(
        previous_snippet.contains(previous_fragment),
        "expected previous label to contain '{previous_fragment}', got snippet {:?} in {:?}",
        previous_snippet,
        diag
    );
}

fn assert_warning_label_message_contains(
    source: &str,
    message_fragment: &str,
    label_index: usize,
    expected_fragment: &str,
) {
    let diags = check_source(source);
    let diag = diags
        .iter()
        .find(|diag| diag.severity == Severity::Warning && diag.message.contains(message_fragment))
        .unwrap_or_else(|| {
            panic!(
                "expected warning containing '{message_fragment}', got {:?}",
                diags
            )
        });
    let label = diag.labels.get(label_index).unwrap_or_else(|| {
        panic!(
            "expected warning label at index {label_index}, got {:?}",
            diag
        )
    });
    assert!(
        label.message.contains(expected_fragment),
        "expected label {label_index} to contain '{expected_fragment}', got {:?} in {:?}",
        label.message,
        diag
    );
}

fn assert_diagnostic_note_contains(
    source: &str,
    severity: Severity,
    message_fragment: &str,
    note_fragment: &str,
) {
    let diags = check_source(source);
    let diag = diags
        .iter()
        .find(|diag| diag.severity == severity && diag.message.contains(message_fragment))
        .unwrap_or_else(|| {
            panic!(
                "expected diagnostic containing '{message_fragment}', got {:?}",
                diags
            )
        });
    assert!(
        diag.notes.iter().any(|note| note.contains(note_fragment)),
        "expected diagnostic note containing '{note_fragment}', got {:?}",
        diag
    );
}

fn assert_diagnostic_label_contains(
    source: &str,
    severity: Severity,
    message_fragment: &str,
    label_fragment: &str,
) {
    let diags = check_source(source);
    let diag = diags
        .iter()
        .find(|diag| diag.severity == severity && diag.message.contains(message_fragment))
        .unwrap_or_else(|| {
            panic!(
                "expected diagnostic containing '{message_fragment}', got {:?}",
                diags
            )
        });
    assert!(
        diag.labels
            .iter()
            .any(|label| label.message.contains(label_fragment)),
        "expected diagnostic label containing '{label_fragment}', got {:?}",
        diag
    );
}

// ============================================================================
// 基本类型字面量
// ============================================================================

#[test]
fn test_typeck_int_literal() {
    check_no_errors("let x = 42;");
}

#[test]
fn test_typeck_float_literal() {
    check_no_errors("let x = 3.14;");
}

#[test]
fn test_typeck_bool_literal_true() {
    check_no_errors("let x = true;");
}

#[test]
fn test_typeck_bool_literal_false() {
    check_no_errors("let x = false;");
}

#[test]
fn test_typeck_string_literal() {
    check_no_errors("let x = \"hello\";");
}

#[test]
fn test_typeck_string_empty() {
    check_no_errors("let x = \"\";");
}

#[test]
fn test_typeck_char_literal() {
    check_no_errors("let x = 'a';");
}

// ============================================================================
// 算术运算
// ============================================================================

#[test]
fn test_typeck_int_addition() {
    check_no_errors("let x = 1 + 2;");
}

#[test]
fn test_typeck_int_subtraction() {
    check_no_errors("let x = 5 - 3;");
}

#[test]
fn test_typeck_int_multiplication() {
    check_no_errors("let x = 4 * 5;");
}

#[test]
fn test_typeck_int_division() {
    check_no_errors("let x = 10 / 2;");
}

#[test]
fn test_typeck_int_modulo() {
    check_no_errors("let x = 10 % 3;");
}

#[test]
fn test_typeck_complex_arithmetic() {
    check_no_errors("let x = 1 + 2 * 3 - 4 / 2;");
}

#[test]
fn test_typeck_nested_parentheses() {
    check_no_errors("let x = ((1 + 2) * (3 + 4));");
}

#[test]
fn test_typeck_float_arithmetic() {
    check_no_errors("let x = 1.0 + 2.0;");
}

#[test]
fn test_typeck_float_operations() {
    check_no_errors("let x = 1.5 * 2.5 - 0.5;");
}

// ============================================================================
// 比较运算
// ============================================================================

#[test]
fn test_typeck_less_than() {
    check_no_errors("let x = 1 < 2;");
}

#[test]
fn test_typeck_greater_than() {
    check_no_errors("let x = 2 > 1;");
}

#[test]
fn test_typeck_less_than_or_equal() {
    check_no_errors("let x = 1 <= 2;");
}

#[test]
fn test_typeck_greater_than_or_equal() {
    check_no_errors("let x = 2 >= 1;");
}

#[test]
fn test_typeck_equality() {
    check_no_errors("let x = 1 == 1;");
}

#[test]
fn test_typeck_inequality() {
    check_no_errors("let x = 1 != 2;");
}

#[test]
fn test_typeck_string_equality() {
    check_no_errors("let x = \"a\" == \"b\";");
}

#[test]
fn test_typeck_bool_equality() {
    check_no_errors("let x = true == false;");
}

// ============================================================================
// 枚举构造器
// ============================================================================

#[test]
fn test_typeck_enum_constructor() {
    check_no_errors("enum Option { Some(Int), None }; let x = Some(1);");
}

#[test]
fn test_typeck_enum_match() {
    check_no_errors(
        "enum Option { Some(Int), None }; let x = Some(1); let y = match x { Some(v) -> v, None -> 0 };",
    );
}

#[test]
fn test_typeck_std_list_builtin_signatures_allow_valid_uses() {
    check_no_errors(
        r#"
            use std.list;
            let count: Int = list.len(list.range(1, 4));
            let keep: Bool = list.isEmpty([]);
        "#,
    );
}

#[test]
fn test_typeck_std_list_builtin_signatures_reject_wrong_annotation() {
    assert_has_diagnostic(
        r#"
            fn expectBool(x: Bool) -> Bool = x;
            use std.list (len);
            let wrong = expectBool(len([1, 2, 3]));
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_string_builtin_signatures_allow_valid_uses() {
    check_no_errors(
        r#"
            use std.string = string;
            let size: Int = string.len("abc");
            let ok: Bool = string.contains("abc", "a");
        "#,
    );
}

#[test]
fn test_typeck_std_string_builtin_signatures_reject_wrong_annotation() {
    assert_has_diagnostic(
        r#"
            fn expectInt(x: Int) -> Int = x;
            use std.string = string;
            let wrong = expectInt(string.trim("abc"));
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_option_builtins_allow_try_and_coalesce() {
    check_no_errors(
        r#"
            use std.option = option;
            let a = option.some(41)? + 1;
            let b = option.none ?? 5;
            let c = option.unwrap_or(option.some(3), 0);
        "#,
    );
}

#[test]
fn test_typeck_std_option_builtin_signatures_reject_wrong_use() {
    assert_has_diagnostic(
        r#"
            fn expectInt(x: Int) -> Int = x;
            use std.option = option;
            let wrong = expectInt(option.is_some(option.some(1)));
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_result_builtins_allow_try_and_unwrap() {
    check_no_errors(
        r#"
            use std.result = result;
            let a = result.ok(41)? + 1;
            let b = result.unwrap(result.ok(7));
            let c = result.unwrap_err(result.err("boom"));
        "#,
    );
}

#[test]
fn test_typeck_std_result_builtin_signatures_reject_wrong_use() {
    assert_has_diagnostic(
        r#"
            fn expectInt(x: Int) -> Int = x;
            use std.result = result;
            let wrong = expectInt(result.is_ok(result.ok(1)));
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_math_constants_have_float_type() {
    check_no_errors(
        r#"
            use std.math = math;
            let pi: Float = math.pi;
            let e: Float = math.e;
            let inf: Float = math.inf;
            let nan: Float = math.nan;
        "#,
    );
}

#[test]
fn test_typeck_std_math_conversion_bridges_have_explicit_result_types() {
    check_no_errors(
        r#"
            use std.math = math;
            let i: Int = math.toInt(true);
            let f: Float = math.toFloat("1.5");
        "#,
    );
}

#[test]
fn test_typeck_std_math_float_predicates_have_explicit_result_types() {
    check_no_errors(
        r#"
            use std.math = math;
            let a: Bool = math.isNan(math.nan);
            let b: Bool = math.isInf(math.inf);
        "#,
    );
}

#[test]
fn test_typeck_std_math_rounding_bridges_have_explicit_result_types() {
    check_no_errors(
        r#"
            use std.math = math;
            let a: Int = math.floor(1.9);
            let b: Int = math.ceil(1.1);
            let c: Int = math.round(1.6);
        "#,
    );
}

#[test]
fn test_typeck_std_math_unary_float_transforms_have_explicit_result_types() {
    check_no_errors(
        r#"
            use std.math = math;
            let a: Float = math.sqrt(9.0);
            let b: Float = math.log(1.0);
            let c: Float = math.log10(1000.0);
            let d: Float = math.exp(0.0);
        "#,
    );
}

#[test]
fn test_typeck_std_math_trigonometric_bridges_have_explicit_result_types() {
    check_no_errors(
        r#"
            use std.math = math;
            let a: Float = math.sin(0.0);
            let b: Float = math.cos(0.0);
            let c: Float = math.tan(0.0);
        "#,
    );
}

#[test]
fn test_typeck_std_math_constants_reject_wrong_annotation() {
    assert_has_diagnostic(
        r#"
            fn expectInt(x: Int) -> Int = x;
            use std.math = math;
            let wrong = expectInt(math.pi) + expectInt(math.inf) + expectInt(math.nan);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_math_conversion_bridges_reject_wrong_annotation() {
    assert_has_diagnostic(
        r#"
            fn expectFloat(x: Float) -> Float = x;
            use std.math = math;
            let wrong = expectFloat(math.toInt(true));
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_math_float_predicates_reject_int_argument() {
    assert_has_diagnostic(
        r#"
            use std.math = math;
            let wrong = math.isNan(1);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_math_rounding_bridges_reject_int_argument() {
    assert_has_diagnostic(
        r#"
            use std.math = math;
            let wrong = math.floor(1);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_math_unary_float_transforms_reject_int_argument() {
    assert_has_diagnostic(
        r#"
            use std.math = math;
            let wrong = math.sqrt(1);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_math_trigonometric_bridges_reject_int_argument() {
    assert_has_diagnostic(
        r#"
            use std.math = math;
            let wrong = math.sin(1);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_math_function_remains_inference_hole_outside_explicit_surface() {
    let source = r#"
        use std.math = math;
        let value = math.abs(1);
    "#;
    let (ast, parse_diags) = parse(source);
    assert!(
        parse_diags.is_empty(),
        "unexpected parse errors: {:?}",
        parse_diags
    );

    let hir = lower(&ast);
    let def_id = hir.items.last().expect("let binding should exist").id;

    let mut checker = TypeChecker::new();
    checker.check(&hir);
    assert!(
        checker.diagnostics_ref().is_empty(),
        "unexpected type errors: {:?}",
        checker.diagnostics_ref()
    );

    let ty = checker
        .global_type(def_id)
        .expect("global type should exist");
    let rendered = format_type(&ty);
    // With v4.0 function-level generalization, the inference hole is wrapped in Forall.
    // This is correct: `value` has a polymorphic type that can be instantiated later.
    assert!(
        rendered.contains('?'),
        "expected math.abs result to contain an inference hole, got {rendered}",
    );
}

#[test]
fn test_typeck_std_path_builtins_allow_valid_uses() {
    check_no_errors(
        r#"
            use std.path = path;
            let structured = path.fromString("/tmp/file.txt");
            let rendered: String = toString(structured);
            let joined: String = path.join("a", "b");
            let parent = path.parent("/tmp/file.txt") ?? "/";
            let abs: Bool = path.is_absolute("/tmp/file.txt");
        "#,
    );
}

#[test]
fn test_typeck_std_path_builtins_reject_wrong_use() {
    assert_has_diagnostic(
        r#"
            fn expectInt(x: Int) -> Int = x;
            use std.path = path;
            let wrong = expectInt(path.is_absolute("/tmp"));
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_path_from_string_does_not_silently_fit_legacy_string_api() {
    assert_has_diagnostic(
        r#"
            use std.path = path;
            let wrong = path.is_absolute(path.fromString("/tmp"));
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_typed_path_adapters_allow_valid_uses() {
    check_no_errors(
        r#"
            use std.path = path;
            let nested: Path = path.joinPath(path.fromString("/tmp"), "n3v3.txt");
            let parent: Path = path.parentPath(nested) ?? path.fromString("/");
            let name: String = path.filenamePath(nested) ?? "missing";
            let ext: String = path.extensionPath(nested) ?? "missing";
            let abs: Bool = path.isAbsolutePath(parent);
            let rendered: String = toString(parent);
        "#,
    );
}

#[test]
fn test_typeck_std_typed_path_adapters_reject_string_receiver() {
    assert_has_diagnostic(
        r#"
            use std.path = path;
            let wrong = path.extensionPath("n3v3.txt");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_builtins_allow_sort_and_extrema() {
    check_no_errors(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let sorted_names: List<String> = list.sort(["b", "a"]);
            let sorted_paths: List<Path> = list.sort(io.readDirEntryPaths(path.fromString("/tmp")));
            let max_value: Option<Int> = list.max([1, 3, 2]);
            let min_value: Option<Int> = list.min([1, 3, 2]);
        "#,
    );
}

#[test]
fn test_typeck_std_list_structural_helpers_allow_precise_uses() {
    check_no_errors(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries: List<Path> = io.readDirEntryPaths(path.fromString("/tmp"));
            let first: Option<Path> = list.head(entries);
            let last: Option<Path> = list.last(entries);
            let init: List<Path> = list.init(entries);
            let reversed: List<Path> = list.reverse(entries);
        "#,
    );
}

#[test]
fn test_typeck_std_list_get_allows_precise_use() {
    check_no_errors(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries: List<Path> = io.readDirEntryPaths(path.fromString("/tmp"));
            let picked: Option<Path> = list.get(0, entries);
        "#,
    );
}

#[test]
fn test_typeck_std_list_cons_allows_precise_use() {
    check_no_errors(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries: List<Path> = io.readDirEntryPaths(path.fromString("/tmp"));
            let with_root: List<Path> = list.cons(path.fromString("/"), entries);
        "#,
    );
}

#[test]
fn test_typeck_std_list_take_allows_precise_use() {
    check_no_errors(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries: List<Path> = io.readDirEntryPaths(path.fromString("/tmp"));
            let prefix: List<Path> = list.take(2, entries);
        "#,
    );
}

#[test]
fn test_typeck_std_list_drop_allows_precise_use() {
    check_no_errors(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries: List<Path> = io.readDirEntryPaths(path.fromString("/tmp"));
            let suffix: List<Path> = list.drop(1, entries);
        "#,
    );
}

#[test]
fn test_typeck_std_list_contains_allows_precise_use() {
    check_no_errors(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries: List<Path> = io.readDirEntryPaths(path.fromString("/tmp"));
            let has_root: Bool = list.contains(path.fromString("/"), entries);
        "#,
    );
}

#[test]
fn test_typeck_std_list_index_of_allows_precise_use() {
    check_no_errors(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let entries: List<Path> = io.readDirEntryPaths(path.fromString("/tmp"));
            let root_index: Option<Int> = list.indexOf(path.fromString("/"), entries);
        "#,
    );
}

#[test]
fn test_typeck_std_list_sum_allows_precise_use() {
    check_no_errors(
        r#"
            use std.list = list;
            let total: Int = list.sum([1, 2, 3]);
        "#,
    );
}

#[test]
fn test_typeck_std_list_product_allows_precise_use() {
    check_no_errors(
        r#"
            use std.list = list;
            let total: Int = list.product([2, 3, 4]);
        "#,
    );
}

#[test]
fn test_typeck_std_list_replicate_allows_precise_use() {
    check_no_errors(
        r#"
            use std.list = list;
            use std.path = path;
            let entries: List<Path> = list.replicate(2, path.fromString("/tmp"));
        "#,
    );
}

#[test]
fn test_typeck_std_list_zip_allows_precise_use() {
    check_no_errors(
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
}

#[test]
fn test_typeck_std_list_unzip_allows_precise_use() {
    check_no_errors(
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
}

#[test]
fn test_typeck_std_list_fold_right_allows_precise_use() {
    check_no_errors(
        r#"
            use std.list = list;
            fn step(x, acc) = x + acc;
            let total: Int = list.foldRight(0, step, [1, 2, 3]);
        "#,
    );
}

#[test]
fn test_typeck_std_list_extrema_reject_non_int_items() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            let wrong = list.max(["b", "a"]);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_cons_rejects_non_list_tail() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            let wrong = list.cons(1, 2);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_take_rejects_non_int_count() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            let wrong = list.take("2", [1, 2, 3]);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_drop_rejects_non_int_count() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            let wrong = list.drop("1", [1, 2, 3]);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_contains_rejects_mismatched_element_type() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            let wrong = list.contains(1, ["1", "2"]);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_index_of_rejects_mismatched_element_type() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            let wrong = list.indexOf(1, ["1", "2"]);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_sum_rejects_non_int_items() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            let wrong = list.sum(["1", "2"]);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_product_rejects_non_int_items() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            let wrong = list.product(["2", "3"]);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_replicate_rejects_non_int_count() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            use std.path = path;
            let wrong = list.replicate("2", path.fromString("/tmp"));
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_zip_rejects_non_list_argument() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            use std.io = io;
            use std.path = path;
            let wrong = list.zip(io.readDirEntryPaths(path.fromString("/tmp")), 1);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_unzip_rejects_non_pair_items() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            let wrong = list.unzip([1, 2]);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_fold_right_rejects_non_list_argument() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            fn step(x, acc) = x + acc;
            let wrong = list.foldRight(0, step, 1);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_get_rejects_non_int_index() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            let wrong = list.get("0", [1, 2]);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_list_structural_helpers_reject_non_list_argument() {
    assert_has_diagnostic(
        r#"
            use std.list = list;
            let wrong = list.head(1);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_builtins_allow_valid_uses() {
    check_no_errors(
        r#"
            use std.io = io;
            use std.path = path;
            let content: String = io.readFile("/tmp/file.txt");
            let entries_legacy: List<String> = io.readDir("/tmp");
            let structured: String = io.readFilePath(path.fromString("/tmp/file.txt"));
            let entries: List<String> = io.readDirPath(path.fromString("/tmp"));
            let entry_paths: List<Path> = io.readDirEntryPaths(path.fromString("/tmp"));
            let write_text_legacy: Unit = io.writeFile("/tmp/file.out", "hello");
            let append_text_legacy: Unit = io.appendFile("/tmp/file.out", "hello");
            let write_text: Unit = io.writeFilePath(path.fromString("/tmp/file.out"), "hello");
            let append_text: Unit = io.appendFilePath(path.fromString("/tmp/file.out"), "hello");
            let binary: Bytes = io.readFileBytesPath(path.fromString("/tmp/file.bin"));
            let write_binary: Unit =
                io.writeFileBytesPath(path.fromString("/tmp/file.out"), binary);
            let append_binary: Unit =
                io.appendFileBytesPath(path.fromString("/tmp/file.out"), binary);
            let cwd_path: Path = io.currentDirPath();
            let home_path: Option<Path> = io.homeDirPath();
            let home_text: Option<String> = io.homeDir();
            let system: String = io.currentSystem();
            let created_dir_legacy: Unit = io.createDirAll("/tmp/n3v3-dir");
            let removed_dir_legacy: Unit = io.removeDirAll("/tmp/n3v3-dir");
            let exists_legacy: Bool = io.pathExists("/tmp/file.txt");
            let dir_legacy: Bool = io.isDir("/tmp");
            let file_legacy: Bool = io.isFile("/tmp/file.txt");
            let created_dir: Unit = io.createDirAllPath(path.fromString("/tmp/n3v3-dir"));
            let removed_dir: Unit = io.removeDirAllPath(path.fromString("/tmp/n3v3-dir"));
            let cmd: Command = io.command("printf", ["n3v3"]);
            let cmd2: Command = io.commandWith(#{ program = "printf", args = ["n3v3"], cwd = "/tmp" });
            let cmd3: Command = io.commandWithRedirects(cmd, [io.redirectStdoutPath(path.fromString("/tmp/n3v3.out"))]);
            let redirect: Redirect = io.redirectStdoutPath(path.fromString("/tmp/n3v3.out"));
            let redirect_err: Redirect = io.redirectStderrPath(path.fromString("/tmp/n3v3.err"));
            let redirect_in: Redirect = io.redirectStdinPath(path.fromString("/tmp/n3v3.in"));
            let pipe: Pipeline = io.pipeline([cmd, cmd2]);
            let pipe2: Pipeline = io.pipelineWithRedirects(pipe, [redirect]);
            let staged_pipe: Pipeline = io.pipeline([io.commandWithRedirects(cmd, [redirect_err]), cmd2]);
            let pipe_proc: ProcessResult = io.execPipeline(pipe);
            let pipe_proc2: ProcessResult = io.execPipeline(pipe2);
            let staged_pipe_proc: ProcessResult = io.execPipeline(staged_pipe);
            let redirected_pipe_proc: ProcessResult =
                io.execPipeline(io.pipelineWithRedirects(pipe, [redirect]));
            let redirected_many_pipe_proc: ProcessResult =
                io.execPipeline(io.pipelineWithRedirects(pipe, [redirect_in, redirect]));
            let task = io.taskCommand(cmd);
            let pipe_task = io.taskPipeline(pipe2);
            let redirected_task = io.taskCommand(cmd3);
            let task_text: String = toString(task);
            let proc_task: ProcessResult = io.awaitTask(task);
            let proc_pipe_task: ProcessResult = io.awaitTask(pipe_task);
            let proc_tasks: List<ProcessResult> = io.awaitTasks([task, pipe_task]);
            let redirected_proc_task: ProcessResult = io.awaitTask(redirected_task);
            let redirected_proc: ProcessResult =
                io.execCommand(io.commandWithRedirects(cmd2, [redirect]));
            let redirected_err_proc: ProcessResult =
                io.execCommand(io.commandWithRedirects(cmd2, [redirect_err]));
            let redirected_in_proc: ProcessResult =
                io.execCommand(io.commandWithRedirects(cmd2, [redirect_in]));
            let redirected_many_proc: ProcessResult =
                io.execCommand(io.commandWithRedirects(cmd2, [redirect, redirect_err]));
            let proc: ProcessResult = io.execCommand(io.command("rustc", ["--version"]));
            let proc0: ProcessResult = io.execCommand(io.command("rustc", ["--version"]));
            let proc5: ProcessResult = io.execCommand(io.command("sh", ["-c", "rustc --version"]));
            let proc6: ProcessResult =
                io.execCommand(io.commandWith(#{ program = "rustc", args = ["--version"] }));
            let proc_ok: Bool = io.processSuccess(proc);
            let proc_out0: String =
                io.processStdout(io.execCommand(io.command("printf", ["n3v3"])));
            let proc_out: String = io.processStdout(io.execCommand(io.command("rustc", ["--version"])));
            let proc_code: Int = io.processCode(io.execCommand(io.command("rustc", ["--version"])));
            let proc_code0: Int = io.processCode(io.execCommand(io.command("sh", ["-c", "printf n3v3"])));
            let proc_code1: Int =
                io.processCode(io.execCommand(io.commandWith(#{ program = "printf", args = ["n3v3"] })));
            let proc_err: String = io.processStderr(io.execCommand(io.command("rustc", ["--version"])));
            let exists: Bool = io.pathExistsPath(path.fromString("/tmp/file.txt"));
            let dir: Bool = io.isDirPath(path.fromString("/tmp"));
            let file: Bool = io.isFilePath(path.fromString("/tmp/file.txt"));
            let cwd_text: String = toString(cwd_path);
            let cmd_text: String = toString(cmd);
            let proc_text: String = toString(proc);
            let digest: String = io.hashString("abc");
            let digest_file: String = io.hashFile("/tmp/file.txt");
            let digest_path: String = io.hashFilePath(path.fromString("/tmp/file.txt"));
            let cwd: String = io.currentDir();
            let env = io.getEnv("HOME") ?? "";
        "#,
    );
}

#[test]
fn test_typeck_std_io_builtins_reject_wrong_use() {
    assert_has_diagnostic(
        r#"
            fn expectInt(x: Int) -> Int = x;
            use std.io = io;
            let wrong = expectInt(io.pathExists("."));
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_read_file_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.readFilePath("/tmp/file.txt");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_read_file_bytes_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.readFileBytesPath("/tmp/file.bin");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_read_dir_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.readDirPath("/tmp");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_read_dir_entry_paths_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.readDirEntryPaths("/tmp");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_hash_string_rejects_non_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.hashString(1);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_write_file_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.writeFilePath("/tmp/file.txt", "hello");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_append_file_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.appendFilePath("/tmp/file.txt", "hello");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_write_file_bytes_path_rejects_non_bytes_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            use std.path = path;
            let wrong = io.writeFileBytesPath(path.fromString("/tmp/file.bin"), "not-bytes");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_append_file_bytes_path_rejects_non_bytes_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            use std.path = path;
            let wrong = io.appendFileBytesPath(path.fromString("/tmp/file.bin"), "not-bytes");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_path_exists_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.pathExistsPath("/tmp/file.txt");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_is_dir_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.isDirPath("/tmp");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_is_file_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.isFilePath("/tmp/file.txt");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_current_dir_path_does_not_silently_fit_string_annotation() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            use std.path = path;
            let wrong = path.is_absolute(io.currentDirPath());
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_home_dir_path_does_not_silently_fit_plain_path_annotation() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            use std.path = path;
            let wrong = path.is_absolute(io.homeDirPath());
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_create_dir_all_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.createDirAllPath("/tmp/n3v3-dir");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_remove_dir_all_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.removeDirAllPath("/tmp/n3v3-dir");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_hash_file_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.hashFilePath("/tmp/file.txt");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_command_with_rejects_non_string_program() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.commandWith(#{ program = 1, args = ["n3v3"] });
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_pipeline_rejects_non_command_list_items() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.pipeline(["printf"]);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_pipeline_with_redirects_rejects_command_argument_pair() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            use std.path = path;
            let wrong = io.pipelineWithRedirects(
                io.command("printf", ["n3v3"]),
                [io.redirectStdoutPath(path.fromString("/tmp/n3v3.out"))]
            );
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_pipeline_with_redirects_rejects_non_redirect_list_items() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.pipelineWithRedirects(
                io.pipeline([io.command("printf", ["n3v3"])]),
                [io.command("cat", [])]
            );
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_exec_pipeline_rejects_command_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.execPipeline(io.command("printf", ["n3v3"]));
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_exec_pipeline_with_embedded_redirects_rejects_non_redirect_list_items() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.execPipeline(
                io.pipelineWithRedirects(
                    io.pipeline([io.command("printf", ["n3v3"])]),
                    [io.command("cat", [])]
                )
            );
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_redirect_stdout_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.redirectStdoutPath("/tmp/n3v3.out");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_redirect_stderr_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.redirectStderrPath("/tmp/n3v3.err");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_redirect_stdin_path_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.redirectStdinPath("/tmp/n3v3.in");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_task_command_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.taskCommand("printf");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_task_pipeline_rejects_command_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.taskPipeline(io.command("printf", ["n3v3"]));
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_await_task_rejects_command_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.awaitTask(io.command("printf", ["n3v3"]));
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_await_tasks_rejects_non_task_list_items() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.awaitTasks([io.command("printf", ["n3v3"])]);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_exec_command_with_embedded_redirects_rejects_non_redirect_list_items() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.execCommand(
                io.commandWithRedirects(
                    io.command("printf", ["n3v3"]),
                    [io.command("cat", [])]
                )
            );
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_command_with_redirects_rejects_non_redirect_list_items() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.commandWithRedirects(
                io.command("printf", ["n3v3"]),
                [io.command("cat", [])]
            );
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_exec_command_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.execCommand("rustc");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_exec_no_longer_exposes_legacy_record_fields() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.execCommand(io.command("printf", ["n3v3"])).stdout;
        "#,
        Severity::Error,
        "field access on non-record type",
    );
}

#[test]
fn test_typeck_std_io_exec_with_no_longer_exposes_legacy_record_fields() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.execCommand(io.commandWith(#{ program = "printf", args = ["n3v3"] })).code;
        "#,
        Severity::Error,
        "field access on non-record type",
    );
}

#[test]
fn test_typeck_std_io_process_success_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.processSuccess("not-a-result");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_process_stdout_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.processStdout("not-a-result");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_process_code_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.processCode("not-a-result");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_io_process_stderr_rejects_string_argument() {
    assert_has_diagnostic(
        r#"
            use std.io = io;
            let wrong = io.processStderr("not-a-result");
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_fetch_builtins_allow_valid_uses() {
    check_no_errors(
        r#"
            use std.fetch = fetch;
            let path: String = fetch.path("Cargo.toml").path;
            let remoteHash: String = fetch.url("https://example.com/archive.tar.gz").hash;
            let verified: String = fetch.pathWithHash("Cargo.toml", "0000000000000000000000000000000000000000000000000000000000000000").hash;
            let hash: String = fetch.urlWithHash("https://example.com/archive.tar.gz", "0000000000000000000000000000000000000000000000000000000000000000").hash;
            let cached: Bool = fetch.git("https://example.com/repo.git", "main").cached;
            let verifiedGit: String = fetch.gitWithHash("https://example.com/repo.git", "main", "0000000000000000000000000000000000000000000000000000000000000000").hash;
        "#,
    );
}

#[test]
fn test_typeck_std_fetch_builtins_reject_wrong_use() {
    assert_has_diagnostic(
        r#"
            fn expectInt(x: Int) -> Int = x;
            use std.fetch = fetch;
            let wrong = expectInt(fetch.path("Cargo.toml").cached);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_map_and_set_builtins_allow_valid_uses() {
    check_no_errors(
        r#"
            use std.Map;
            use std.Set;
            use std.list = list;
            let map = Map.insert("a", 1, Map.empty);
            let value: Int = Map.getWithDefault("a", 0, map);
            let present: Bool = Map.contains("a", map);
            let total: Int = list.sum(Map.values(map));
            let set = Set.insert(1, Set.empty);
            let count: Int = Set.size(set);
            let hasOne: Bool = Set.contains(1, set);
        "#,
    );
}

#[test]
fn test_typeck_std_map_and_set_builtins_reject_wrong_use() {
    assert_has_diagnostic(
        r#"
            fn expectInt(x: Int) -> Int = x;
            use std.Map;
            use std.Set;
            let map = Map.insert("a", 1, Map.empty);
            let set = Set.insert(1, Set.empty);
            let wrong = expectInt(Map.contains("a", map)) + expectInt(Set.isEmpty(set));
        "#,
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_std_map_values_rejects_non_map_argument() {
    assert_has_diagnostic(
        r#"
            use std.Map;
            let wrong = Map.values(1);
        "#,
        Severity::Error,
        "type mismatch",
    );
}

// ============================================================================
// 逻辑运算
// ============================================================================

#[test]
fn test_typeck_logical_and() {
    check_no_errors("let x = true && false;");
}

#[test]
fn test_typeck_logical_or() {
    check_no_errors("let x = true || false;");
}

#[test]
fn test_typeck_logical_not() {
    check_no_errors("let x = !true;");
}

#[test]
fn test_typeck_complex_logical() {
    check_no_errors("let x = true && false || !true;");
}

#[test]
fn test_typeck_logical_with_comparison() {
    check_no_errors("let x = 1 < 2 && 3 > 2;");
}

#[test]
fn test_typeck_logical_and_wrong_type() {
    check_has_errors("let x = 1 && 2;");
}

#[test]
fn test_typeck_logical_or_wrong_type() {
    check_has_errors("let x = 1 || 2;");
}

#[test]
fn test_typeck_not_wrong_type() {
    check_has_errors("let x = !42;");
}

// ============================================================================
// 条件表达式
// ============================================================================

#[test]
fn test_typeck_if_then_else_int() {
    check_no_errors("let x = if true -> 1 else 0;");
}

#[test]
fn test_typeck_if_then_else_string() {
    check_no_errors("let x = if true -> \"yes\" else \"no\";");
}

#[test]
fn test_typeck_if_then_else_bool() {
    check_no_errors("let x = if true -> true else false;");
}

#[test]
fn test_typeck_if_with_comparison() {
    check_no_errors("let x = if 1 < 2 -> 10 else 20;");
}

#[test]
fn test_typeck_if_nested() {
    check_no_errors("let x = if true -> if false -> 1 else 2 else 3;");
}

#[test]
fn test_typeck_if_deeply_nested() {
    check_no_errors("let x = if true -> if true -> if false -> 1 else 2 else 3 else 4;");
}

#[test]
fn test_typeck_if_branch_type_mismatch() {
    check_has_errors("let x = if true -> 1 else false;");
}

#[test]
fn test_typeck_if_branch_type_mismatch_string_int() {
    check_has_errors("let x = if true -> \"hello\" else 42;");
}

#[test]
fn test_typeck_if_condition_not_bool() {
    check_has_errors("let x = if 42 -> 1 else 0;");
}

#[test]
fn test_typeck_if_condition_not_bool_string() {
    check_has_errors("let x = if \"true\" -> 1 else 0;");
}

// ============================================================================
// 元组
// ============================================================================

#[test]
fn test_typeck_tuple_pair() {
    check_no_errors("let x = (1, 2);");
}

#[test]
fn test_typeck_tuple_triple() {
    check_no_errors("let x = (1, 2, 3);");
}

#[test]
fn test_typeck_tuple_mixed_types() {
    check_no_errors("let x = (1, true, \"hello\");");
}

#[test]
fn test_typeck_tuple_nested() {
    check_no_errors("let x = ((1, 2), (3, 4));");
}

#[test]
fn test_typeck_tuple_deeply_nested() {
    check_no_errors("let x = (((1, 2), 3), 4);");
}

#[test]
fn test_typeck_tuple_with_expressions() {
    check_no_errors("let x = (1 + 2, 3 * 4);");
}

// ============================================================================
// 列表
// ============================================================================

#[test]
fn test_typeck_list_empty() {
    check_no_errors("let x = [];");
}

#[test]
fn test_typeck_list_single() {
    check_no_errors("let x = [1];");
}

#[test]
fn test_typeck_builtin_list_annotation_rejects_non_list_value() {
    assert_has_diagnostic(
        "let x: List<Int> = \"not a list\";",
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_list_multiple() {
    check_no_errors("let x = [1, 2, 3];");
}

#[test]
fn test_typeck_list_strings() {
    check_no_errors("let x = [\"a\", \"b\", \"c\"];");
}

#[test]
fn test_typeck_list_bools() {
    check_no_errors("let x = [true, false, true];");
}

#[test]
fn test_typeck_list_nested() {
    check_no_errors("let x = [[1, 2], [3, 4]];");
}

#[test]
fn test_typeck_list_with_expressions() {
    check_no_errors("let x = [1 + 2, 3 * 4, 5 - 1];");
}

#[test]
fn test_typeck_list_heterogeneous() {
    // Lists must be homogeneous
    check_has_errors("let x = [1, true];");
}

#[test]
fn test_typeck_list_mixed_types() {
    check_has_errors("let x = [1, \"hello\"];");
}

// ============================================================================
// 记录
// ============================================================================

#[test]
fn test_typeck_record_single_field() {
    check_no_errors("let x = #{ a = 1 };");
}

#[test]
fn test_typeck_record_multiple_fields() {
    check_no_errors("let x = #{ a = 1, b = 2, c = 3 };");
}

#[test]
fn test_typeck_record_mixed_types() {
    check_no_errors("let x = #{ name = \"alice\", age = 30, active = true };");
}

#[test]
fn test_typeck_record_nested() {
    check_no_errors("let x = #{ inner = #{ a = 1 } };");
}

#[test]
fn test_typeck_record_with_expressions() {
    check_no_errors("let x = #{ sum = 1 + 2, product = 3 * 4 };");
}

#[test]
fn test_typeck_record_field_access_after_record_binding() {
    check_no_errors(
        r#"
            let config = #{ port = 40, host = "localhost" };
            let x = config.port;
        "#,
    );
}

#[test]
fn test_typeck_record_field_access_reports_missing_known_field() {
    check_has_errors(
        r#"
            let config = #{ port = 40, host = "localhost" };
            let x = config.missing;
        "#,
    );
}

#[test]
fn test_typeck_dynamic_record_field_chain_on_unknown_param() {
    check_no_errors(
        r#"
            let outputs = fn(inputs) inputs.dep.packages.default;
        "#,
    );
}

#[test]
fn test_typeck_dynamic_record_constraints_accumulate_on_same_base() {
    check_no_errors(
        r#"
            let project = fn(inputs) #{
                pkg = inputs.dep.packages.default,
                src = inputs.dep.sources.default
            };
            let x = project(#{
                dep = #{
                    packages = #{ default = 1 },
                    sources = #{ default = 2 }
                }
            });
        "#,
    );
}

#[test]
fn test_typeck_dynamic_record_constraints_reject_missing_nested_field() {
    check_has_errors(
        r#"
            let outputs = fn(inputs) inputs.dep.packages.default;
            let x = outputs(#{ dep = #{} });
        "#,
    );
}

#[test]
fn test_typeck_lazy_force() {
    check_no_errors("let thunk = ~42; let x = force(thunk);");
}

#[test]
fn test_typeck_lazy_predicates() {
    check_no_errors("let thunk = ~42; let x = isLazy(thunk); let y = isEvaluated(thunk);");
}

#[test]
fn test_typeck_or_pattern_with_shared_binding() {
    check_no_errors("let x = match (1, 2) { (0, v) | (1, v) -> v, _ -> 0 };");
}

#[test]
fn test_typeck_binding_pattern() {
    check_no_errors("let x = match 42 { n @ 42 -> n, _ -> 0 };");
}

#[test]
fn test_typeck_list_rest_pattern() {
    check_no_errors(
        "
        let x = match [1, 2, 3, 4] {
            [first, ..middle, last] -> match middle {
                [a, b] -> first + a + b + last,
                _ -> 0,
            },
            _ -> 0,
        };
        ",
    );
}

#[test]
fn test_typeck_try_on_option_like_enum() {
    check_no_errors(
        "
        enum Option { Some(Int), None };
        let x = Some(41)? + 1;
        ",
    );
}

#[test]
fn test_typeck_try_on_result_like_enum() {
    check_no_errors(
        "
        enum Result { Ok(Int), Err(String) };
        let x = Ok(41)? + 1;
        ",
    );
}

#[test]
fn test_typeck_try_rejects_known_non_optional_value() {
    check_has_errors("let x = 41?;");
}

#[test]
fn test_typeck_coalesce_on_option_like_enum() {
    check_no_errors(
        "
        enum Option { Some(Int), None };
        let x = Some(41) ?? 0;
        ",
    );
}

#[test]
fn test_typeck_coalesce_rejects_known_non_optional_value() {
    check_has_errors("let x = 41 ?? 0;");
}

#[test]
fn test_typeck_safe_field_coalesce_defaults_to_string() {
    check_no_errors(
        "
        let r = #{ name = \"test\" };
        let x = r?.missing ?? \"default\";
        ",
    );
}

#[test]
fn test_typeck_safe_field_on_unknown_param_with_coalesce() {
    check_no_errors(
        "
        let readName = fn(config) config?.name ?? \"default\";
        ",
    );
}

#[test]
fn test_typeck_safe_field_on_unknown_param_rejects_non_record_callsite() {
    check_has_errors(
        "
        let readName = fn(config) config?.name ?? \"default\";
        let value = readName(42);
        ",
    );
}

#[test]
fn test_typeck_safe_field_on_unknown_param_allows_missing_record_field() {
    check_no_errors(
        "
        let readName = fn(config) config?.name ?? \"default\";
        let value = readName(#{});
        ",
    );
}

#[test]
fn test_typeck_safe_field_on_unknown_param_accepts_option_record_callsite() {
    check_no_errors(
        "
        use std.option = option;
        let readName = fn(config) config?.name ?? \"default\";
        let value = readName(option.some(#{ name = \"n3v3\" }));
        ",
    );
}

#[test]
fn test_typeck_safe_field_on_unknown_param_checks_present_field_type() {
    check_has_errors(
        "
        let readName = fn(config) config?.name ?? \"default\";
        let value = readName(#{ name = 42 });
        ",
    );
}

#[test]
fn test_typeck_method_call_falls_back_to_function_call_semantics() {
    // Method fallback now emits a warning; check for no errors (warnings OK)
    let diags = check_source(
        "
        fn twice(x: Int) -> Int = x + x;
        let y = 21.twice();
        ",
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == n3v3_diagnostic::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
}

#[test]
fn test_typeck_trait_method_dispatch_takes_precedence_over_callable_target_fallback() {
    check_no_errors(
        "
        fn twice(x: Int) -> String = \"fallback\";
        trait Twice { fn twice(self) -> Int; };
        impl Twice for Int {
            fn twice(self) -> Int = self + self;
        };
        let value: Int = 21.twice();
        ",
    );
}

#[test]
fn test_typeck_method_call_without_dispatch_or_callable_reports_dedicated_missing_method() {
    let diags = check_source(
        "
        let value = 21.missing();
        ",
    );
    let diag = diags
        .iter()
        .find(|diag| diag.message.contains("no method `missing` found for `Int`"))
        .unwrap_or_else(|| {
            panic!(
                "expected dedicated missing-method diagnostic, got {:?}",
                diags
            )
        });
    assert_eq!(diag.severity, Severity::Error);
    assert_eq!(diag.code, Some(ErrorCode::UnknownMethod));
    assert!(
        diag.notes
            .iter()
            .any(|note| note.contains("no callable fallback `missing(receiver, ...)`")),
        "expected fallback note, got {:?}",
        diag
    );
}

#[test]
fn test_typeck_method_call_resolves_trait_impl_signature() {
    check_no_errors(
        "
        trait Show { fn show(self) -> String; };
        impl Show for Int {
            fn show(self) -> String = toString(self);
        };
        let x = 1.show();
        ",
    );
}

// ============================================================================
// 函数定义
// ============================================================================

#[test]
fn test_typeck_function_simple() {
    check_no_errors("fn add_one(x) = x + 1;");
}

#[test]
fn test_typeck_function_two_params() {
    check_no_errors("fn add(a, b) = a + b;");
}

#[test]
fn test_typeck_function_three_params() {
    check_no_errors("fn sum3(a, b, c) = a + b + c;");
}

#[test]
fn test_typeck_function_returns_bool() {
    check_no_errors("fn is_positive(x) = x > 0;");
}

#[test]
fn test_typeck_function_with_if() {
    check_no_errors("fn abs(x) = if x < 0 -> -x else x;");
}

#[test]
fn test_typeck_function_identity() {
    check_no_errors("fn identity(x) = x;");
}

#[test]
fn test_typeck_function_constant() {
    check_no_errors("fn always_42() = 42;");
}

#[test]
fn test_typeck_multiple_functions() {
    check_no_errors(
        "
        fn double(x) = x * 2;
        fn triple(x) = x * 3;
        fn quadruple(x) = double(double(x));
    ",
    );
}

// ============================================================================
// 函数调用
// ============================================================================

#[test]
fn test_typeck_function_call() {
    check_no_errors(
        "
        fn double(x) = x * 2;
        let y = double(21);
    ",
    );
}

#[test]
fn test_typeck_function_call_nested() {
    check_no_errors(
        "
        fn double(x) = x * 2;
        fn add_one(x) = x + 1;
        let y = add_one(double(5));
    ",
    );
}

#[test]
fn test_typeck_function_call_chain() {
    check_no_errors(
        "
        fn f(x) = x + 1;
        fn g(x) = x * 2;
        fn h(x) = x - 1;
        let y = h(g(f(10)));
    ",
    );
}

// ============================================================================
// 递归函数
// ============================================================================

#[test]
fn test_typeck_recursive_factorial() {
    check_no_errors(
        "
        fn fact(n) = if n <= 1 -> 1 else n * fact(n - 1);
    ",
    );
}

#[test]
fn test_typeck_recursive_fibonacci() {
    check_no_errors(
        "
        fn fib(n) = if n <= 1 -> n else fib(n - 1) + fib(n - 2);
    ",
    );
}

#[test]
fn test_typeck_recursive_sum() {
    check_no_errors(
        "
        fn sum_to(n) = if n <= 0 -> 0 else n + sum_to(n - 1);
    ",
    );
}

#[test]
fn test_typeck_mutually_recursive() {
    check_no_errors(
        "
        fn is_even(n) = if n == 0 -> true else is_odd(n - 1);
        fn is_odd(n) = if n == 0 -> false else is_even(n - 1);
    ",
    );
}

// ============================================================================
// 管道运算符
// ============================================================================

#[test]
fn test_typeck_pipe_simple() {
    check_no_errors(
        "
        fn double(x) = x * 2;
        let x = 5 |> double;
    ",
    );
}

#[test]
fn test_typeck_pipe_chain() {
    check_no_errors(
        "
        fn double(x) = x * 2;
        fn add_one(x) = x + 1;
        let x = 5 |> double |> add_one;
    ",
    );
}

#[test]
fn test_typeck_pipe_long_chain() {
    check_no_errors(
        "
        fn f(x) = x + 1;
        fn g(x) = x * 2;
        fn h(x) = x - 1;
        let x = 10 |> f |> g |> h |> f |> g;
    ",
    );
}

// ============================================================================
// 模式匹配
// ============================================================================

#[test]
fn test_typeck_match_literal() {
    check_no_errors(
        "
        let x = match 1 {
            0 -> 100,
            1 -> 200,
            _ -> 300
        };
    ",
    );
}

#[test]
fn test_typeck_match_wildcard() {
    check_no_errors(
        "
        let x = match 5 {
            _ -> 42
        };
    ",
    );
}

#[test]
fn test_typeck_match_binding() {
    check_no_errors(
        "
        let x = match 42 {
            n -> n + 1
        };
    ",
    );
}

#[test]
fn test_typeck_match_bool() {
    check_no_errors(
        "
        let x = match true {
            true -> 1,
            false -> 0
        };
    ",
    );
}

#[test]
fn test_typeck_match_tuple() {
    check_no_errors(
        "
        let x = match (1, 2) {
            (a, b) -> a + b
        };
    ",
    );
}

#[test]
fn test_typeck_match_nested_tuple() {
    check_no_errors(
        "
        let x = match ((1, 2), 3) {
            ((a, b), c) -> a + b + c
        };
    ",
    );
}

#[test]
fn test_typeck_match_arm_type_mismatch() {
    check_has_errors(
        "
        let x = match 1 {
            0 -> 100,
            1 -> true,
            _ -> 300
        };
    ",
    );
}

#[test]
fn test_typeck_match_returns_consistent_type() {
    check_no_errors(
        "
        let x = match 1 {
            0 -> false,
            _ -> true
        };
    ",
    );
}

#[test]
fn test_typeck_match_bool_non_exhaustive() {
    assert_has_diagnostic(
        "
        let x = match true {
            true -> 1
        };
        ",
        Severity::Error,
        "non-exhaustive pattern match",
    );
}

#[test]
fn test_typeck_match_enum_non_exhaustive() {
    assert_has_diagnostic(
        "
        enum Option { Some(Int), None };
        let x = match Some(1) {
            Some(value) -> value
        };
        ",
        Severity::Error,
        "non-exhaustive pattern match",
    );
}

#[test]
fn test_typeck_match_user_enum_missing_patterns_follow_declaration_order() {
    let source = "
        enum Status { Pending(Int), Running, Done(String), Failed };
        let x = match Running() {
            Running -> 1
        };
        ";
    assert_has_diagnostic(source, Severity::Error, "non-exhaustive pattern match");
    assert_diagnostic_note_contains(
        source,
        Severity::Error,
        "non-exhaustive pattern match",
        "missing patterns: Pending(_), Done(_), Failed",
    );
}

#[test]
fn test_typeck_match_builtin_option_non_exhaustive() {
    assert_has_diagnostic(
        "
        use std.option = option;
        let x = match option.some(1) {
            Some(value) -> value
        };
        ",
        Severity::Error,
        "non-exhaustive pattern match",
    );
}

#[test]
fn test_typeck_match_builtin_result_non_exhaustive() {
    assert_has_diagnostic(
        "
        use std.result = result;
        let x = match result.ok(1) {
            Ok(value) -> value
        };
        ",
        Severity::Error,
        "non-exhaustive pattern match",
    );
}

#[test]
fn test_typeck_match_builtin_option_rejects_wrong_constructor() {
    assert_has_diagnostic(
        "
        use std.option = option;
        let x = match option.some(1) {
            Ok(value) -> value,
            None -> 0
        };
        ",
        Severity::Error,
        "constructor does not match expected builtin type",
    );
}

#[test]
fn test_typeck_match_unreachable_after_wildcard() {
    let source = "
        let x = match true {
            _ -> 1,
            false -> 0
        };
        ";
    assert_has_diagnostic(source, Severity::Warning, "unreachable pattern");
    assert_warning_label_message_contains(
        source,
        "unreachable pattern",
        1,
        "matches all remaining values",
    );
}

#[test]
fn test_typeck_match_builtin_option_unreachable_after_complete_coverage() {
    assert_has_diagnostic(
        "
        use std.option = option;
        let x = match option.some(1) {
            Some(_) | None -> 1,
            Some(value) -> value
        };
        ",
        Severity::Warning,
        "unreachable pattern",
    );
}

#[test]
fn test_typeck_match_builtin_option_subset_shadowing_is_unreachable() {
    let source = "
        use std.option = option;
        let x = match option.some(1) {
            Some(_) -> 1,
            Some(value) -> value,
            None -> 0
        };
        ";
    assert_has_diagnostic(source, Severity::Warning, "unreachable pattern");
    assert_warning_previous_label_contains(source, "unreachable pattern", "Some(_) -> 1");
    assert_warning_label_message_contains(
        source,
        "unreachable pattern",
        1,
        "already covers this case",
    );
}

#[test]
fn test_typeck_match_builtin_result_subset_shadowing_is_unreachable() {
    assert_has_diagnostic(
        "
        use std.result = result;
        let x = match result.ok(1) {
            Ok(_) -> 1,
            Ok(value) -> value,
            Err(_) -> 0
        };
        ",
        Severity::Warning,
        "unreachable pattern",
    );
}

#[test]
fn test_typeck_match_user_enum_subset_shadowing_is_unreachable() {
    assert_has_diagnostic(
        "
        enum Value { Int(Int), Missing };
        let x = match Int(1) {
            Int(_) -> 1,
            Int(value) -> value,
            Missing -> 0
        };
        ",
        Severity::Warning,
        "unreachable pattern",
    );
}

#[test]
fn test_typeck_match_single_variant_irrefutable_constructor_makes_later_arm_unreachable() {
    let source = "
        enum Only { Mk(Int) };
        let x = match Mk(1) {
            Mk(value) -> value,
            Mk(1) -> 1
        };
        ";
    assert_has_diagnostic(source, Severity::Warning, "unreachable pattern");
    assert_warning_previous_label_contains(source, "unreachable pattern", "Mk(value) -> value");
}

#[test]
fn test_typeck_enum_variant_matching_enum_name_reports_conflict() {
    assert_has_diagnostic(
        "
        enum Only { Only(Int) };
        let x = Only(1);
        ",
        Severity::Error,
        "enum variant `Only` conflicts with the enum name `Only`",
    );
}

#[test]
fn test_typeck_duplicate_enum_variant_reports_duplicate() {
    assert_has_diagnostic(
        "
        enum Choice { Yes(Int), Yes(Int) };
        let x = Yes(1);
        ",
        Severity::Error,
        "duplicate enum variant `Yes`",
    );
}

#[test]
fn test_typeck_match_bool_or_pattern_complete_then_later_arm_is_unreachable() {
    assert_has_diagnostic(
        "
        let x = match true {
            true | false -> 1,
            true -> 2
        };
        ",
        Severity::Warning,
        "unreachable pattern",
    );
}

#[test]
fn test_typeck_match_guarded_arm_does_not_make_match_exhaustive() {
    assert_has_diagnostic(
        "
        let x = match true {
            value if value -> 1
        };
        ",
        Severity::Error,
        "non-exhaustive pattern match",
    );
}

#[test]
fn test_typeck_match_guarded_arm_does_not_make_later_arm_unreachable() {
    check_no_errors(
        "
        let x = match true {
            value if value -> 1,
            true -> 2,
            false -> 3
        };
        ",
    );
}

// ============================================================================
// 一元运算符
// ============================================================================

#[test]
fn test_typeck_unary_neg_int() {
    check_no_errors("let x = -42;");
}

#[test]
fn test_typeck_unary_neg_float() {
    check_no_errors("let x = -3.14;");
}

#[test]
fn test_typeck_unary_not_bool() {
    check_no_errors("let x = !true;");
}

#[test]
fn test_typeck_double_neg() {
    check_no_errors("let x = - -42;");
}

#[test]
fn test_typeck_double_not() {
    check_no_errors("let x = !!true;");
}

#[test]
fn test_typeck_unary_neg_expression() {
    check_no_errors("let x = -(1 + 2);");
}

// ============================================================================
// 字符串连接
// ============================================================================

#[test]
fn test_typeck_string_concat() {
    check_no_errors("let x = \"hello\" ++ \" world\";");
}

#[test]
fn test_typeck_string_concat_chain() {
    check_no_errors("let x = \"a\" ++ \"b\" ++ \"c\";");
}

// ============================================================================
// 列表连接
// ============================================================================

#[test]
fn test_typeck_list_concat() {
    check_no_errors("let x = [1, 2] ++ [3, 4];");
}

#[test]
fn test_typeck_list_concat_empty() {
    check_no_errors("let x = [] ++ [1, 2];");
}

#[test]
fn test_typeck_list_concat_chain() {
    check_no_errors("let x = [1] ++ [2] ++ [3];");
}

// ============================================================================
// 记录合并
// ============================================================================

#[test]
fn test_typeck_record_merge() {
    check_no_errors("let x = #{ a = 1 } // #{ b = 2 };");
}

#[test]
fn test_typeck_record_merge_override() {
    check_no_errors("let x = #{ a = 1 } // #{ a = 2 };");
}

#[test]
fn test_typeck_record_merge_chain() {
    check_no_errors("let x = #{ a = 1 } // #{ b = 2 } // #{ c = 3 };");
}

// ============================================================================
// 多重 let 绑定
// ============================================================================

#[test]
fn test_typeck_multiple_lets() {
    check_no_errors("let a = 1; let b = 2; let c = a + b;");
}

#[test]
fn test_typeck_let_shadowing() {
    // 顶层 let 之间不能互相引用，这里测试单独的 let
    check_no_errors("let x = 1 + 1;");
}

#[test]
fn test_typeck_let_uses_previous() {
    // 在函数内部可以使用前面定义的变量
    check_no_errors(
        "
        fn test() = {
            let a = 10;
            let b = a * 2;
            a + b
        };
    ",
    );
}

// ============================================================================
// 复杂表达式
// ============================================================================

#[test]
fn test_typeck_complex_expression_1() {
    check_no_errors("let x = if 1 + 2 > 2 -> (3, 4) else (5, 6);");
}

#[test]
fn test_typeck_complex_expression_2() {
    check_no_errors(
        "
        fn f(x) = x * 2;
        let x = if true -> f(5) else f(10);
    ",
    );
}

#[test]
fn test_typeck_complex_expression_3() {
    check_no_errors(
        "
        let x = match (1, 2) {
            (0, _) -> 0,
            (_, 0) -> 0,
            (a, b) -> a * b
        };
    ",
    );
}

// ============================================================================
// Lambda 表达式
// ============================================================================

#[test]
fn test_typeck_lambda_simple() {
    check_no_errors("let f = fn(x) x + 1;");
}

#[test]
fn test_typeck_lambda_multiple_params() {
    check_no_errors("let f = fn(x, y) x + y;");
}

#[test]
fn test_typeck_lambda_preserves_explicit_param_types() {
    let source = "let f = fn(x: Int) x;";
    let (ast, parse_diags) = parse(source);
    assert!(
        parse_diags.is_empty(),
        "unexpected parse errors: {:?}",
        parse_diags
    );

    let hir = lower(&ast);
    let def_id = hir.items[0].id;

    let mut checker = TypeChecker::new();
    checker.check(&hir);
    assert!(
        checker.diagnostics_ref().is_empty(),
        "unexpected type errors: {:?}",
        checker.diagnostics_ref()
    );

    let ty = checker
        .global_type(def_id)
        .expect("global type should exist");
    assert_eq!(format_type(&ty), "(Int) -> Int");
}

#[test]
fn test_typeck_closure_in_function() {
    // 在函数内定义闭包
    check_no_errors(
        "
        fn make_adder(n) = fn(x) x + n;
    ",
    );
}

#[test]
fn test_typeck_block_let_tuple_pattern() {
    check_no_errors("fn sum_pair() = { let (x, y) = (1, 2); x + y };");
}

// ============================================================================
// 类型推导边缘情况
// ============================================================================

#[test]
fn test_typeck_polymorphic_identity() {
    // 单次调用多态函数是可以的
    check_no_errors(
        "
        fn id(x) = x;
        let a = id(42);
    ",
    );
}

#[test]
fn test_typeck_global_type_preserves_explicit_generic_params() {
    let source = "fn id<T>(x: T) -> T = x;";
    let (ast, parse_diags) = parse(source);
    assert!(
        parse_diags.is_empty(),
        "unexpected parse errors: {:?}",
        parse_diags
    );

    let hir = lower(&ast);
    let def_id = hir.items[0].id;

    let mut checker = TypeChecker::new();
    checker.check(&hir);
    assert!(
        checker.diagnostics_ref().is_empty(),
        "unexpected type errors: {:?}",
        checker.diagnostics_ref()
    );

    let ty = checker
        .global_type(def_id)
        .expect("global type should exist");
    assert_eq!(format_type(&ty), "forall T. (T) -> T");
}

#[test]
fn test_typeck_generic_trait_bounds_are_order_independent() {
    assert_has_diagnostic(
        "
        fn require<T: Show>(value: T) -> T = value;
        trait Show {};
        require(1);
    ",
        Severity::Error,
        "does not implement trait `Show`",
    );
}

#[test]
fn test_typeck_generic_trait_impl_matches_concrete_type() {
    check_no_errors(
        "
        trait Show { fn show(self) -> String; };
        impl Show for Int {
            fn show(self) -> String = \"int\";
        };
        impl<T: Show> Show for List<T> {
            fn show(self) -> String = \"list\";
        };
        let rendered: String = [1].show();
    ",
    );
}

#[test]
fn test_typeck_polymorphic_const() {
    // 单次调用多态函数是可以的
    check_no_errors(
        "
        fn const_val(x, y) = x;
        let a = const_val(1, true);
    ",
    );
}

#[test]
fn test_typeck_higher_order_function() {
    check_no_errors(
        "
        fn apply(f, x) = f(x);
        fn double(x) = x * 2;
        let y = apply(double, 21);
    ",
    );
}

// ============================================================================
// 压力测试
// ============================================================================

#[test]
fn test_typeck_many_lets() {
    check_no_errors(
        "
        let a = 1;
        let b = 2;
        let c = 3;
        let d = 4;
        let e = 5;
        let f = a + b + c + d + e;
    ",
    );
}

#[test]
fn test_typeck_many_functions() {
    check_no_errors(
        "
        fn f1(x) = x + 1;
        fn f2(x) = x + 2;
        fn f3(x) = x + 3;
        fn f4(x) = x + 4;
        fn f5(x) = x + 5;
        let y = f1(f2(f3(f4(f5(0)))));
    ",
    );
}

#[test]
fn test_typeck_deeply_nested_if() {
    check_no_errors(
        "
        let x = if true ->
            if true ->
                if true ->
                    if true ->
                        1
                    else
                        2
                else
                    3
            else
                4
        else
            5;
    ",
    );
}

#[test]
fn test_typeck_complex_match() {
    check_no_errors(
        "
        let x = match (1, (2, 3)) {
            (0, _) -> 0,
            (_, (0, _)) -> 1,
            (_, (_, 0)) -> 2,
            (a, (b, c)) -> a + b + c
        };
    ",
    );
}

// ============================================================================
// Trait 关联类型
// ============================================================================

#[test]
fn test_typeck_assoc_type_bounds_satisfied() {
    check_no_errors(
        "
        trait Show { };
        trait Iterator { type Item: Show; };
        struct Foo {};
        impl Show for Int { };
        impl Iterator for Foo { type Item = Int; };
    ",
    );
}

#[test]
fn test_typeck_assoc_type_bounds_missing_impl() {
    check_has_errors(
        "
        trait Show { };
        trait Iterator { type Item: Show; };
        struct Foo {};
        impl Iterator for Foo { type Item = Int; };
    ",
    );
}
#[test]
fn test_typeck_assoc_type_bounds_are_order_independent() {
    assert_has_diagnostic(
        "
        trait Iterator { type Item: Show; };
        trait Show {};
        struct Foo {};
        impl Iterator for Foo { type Item = Int; };
    ",
        Severity::Error,
        "associated type 'Item' in impl of trait 'Iterator' must satisfy bound 'Show'",
    );
}

#[test]
fn test_typeck_assoc_type_bounds_accept_canonical_self_assoc_binding() {
    check_no_errors(
        "
        trait Show { };
        trait Iterator { type Item: Show; type Alias; };
        struct Foo {};
        impl Show for Int { };
        impl Iterator for Foo {
            type Alias = Int;
            type Item = Self.Alias;
        };
    ",
    );
}

#[test]
fn test_typeck_assoc_type_bounds_report_canonical_type_from_self_assoc_binding() {
    let source = "
        trait Show { };
        trait Iterator { type Item: Show; type Alias; };
        struct Foo {};
        impl Iterator for Foo {
            type Alias = Int;
            type Item = Self.Alias;
        };
    ";
    assert_diagnostic_label_contains(
        source,
        Severity::Error,
        "associated type 'Item' in impl of trait 'Iterator' must satisfy bound 'Show'",
        "associated type resolves to `Int` here",
    );
    assert_diagnostic_note_contains(
        source,
        Severity::Error,
        "associated type 'Item' in impl of trait 'Iterator' must satisfy bound 'Show'",
        "`Int` does not implement `Show`",
    );
}

#[test]
fn test_typeck_assoc_type_bounds_report_cyclic_assoc_definition() {
    assert_has_diagnostic(
        "
        trait Show { };
        trait Iterator { type Item: Show; };
        struct Foo {};
        impl Iterator for Foo {
            type Item = Self.Item;
        };
        ",
        Severity::Error,
        "cyclic associated type definition `Self.Item`",
    );
}

#[test]
fn test_typeck_impl_method_body_return_type_mismatch() {
    assert_has_diagnostic(
        "
        struct Counter {};
        impl Counter {
            fn value(self) -> Int = true;
        };
        ",
        Severity::Error,
        "impl method `value` return type",
    );
}

#[test]
fn test_typeck_trait_impl_method_body_return_type_mismatch() {
    assert_has_diagnostic(
        "
        trait Show {
            fn show(self) -> Int;
        };
        struct Counter {};
        impl Show for Counter {
            fn show(self) -> Int = true;
        };
        ",
        Severity::Error,
        "impl method `show` return type",
    );
}

#[test]
fn test_typeck_to_string_builtin() {
    check_no_errors("let x = toString(42);");
}

#[test]
fn test_typeck_trait_impl_method_signature_return_mismatch() {
    assert_has_diagnostic(
        "
        trait Show {
            fn show(self) -> Int;
        };
        struct Counter {};
        impl Show for Counter {
            fn show(self) -> String = \"counter\";
        };
        ",
        Severity::Error,
        "does not match trait `Show` signature",
    );
}

#[test]
fn test_typeck_trait_impl_method_signature_param_mismatch() {
    assert_has_diagnostic(
        "
        trait Add {
            fn add(self, x: Int) -> Int;
        };
        struct Counter {};
        impl Add for Counter {
            fn add(self) -> Int = 0;
        };
        ",
        Severity::Error,
        "does not match trait `Add` signature",
    );
}

#[test]
fn test_typeck_trait_self_type_signature_support() {
    check_no_errors(
        "
        trait Eq {
            fn eq(self, other: Self) -> Bool;
        };
        struct Counter {};
        impl Eq for Counter {
            fn eq(self, other: Self) -> Bool = true;
        };
        ",
    );
}

#[test]
fn test_typeck_trait_assoc_type_use_site_support() {
    check_no_errors(
        "
        trait Iterator {
            type Item;
            fn first(self) -> Self.Item;
        };
        struct Counter {};
        impl Iterator for Counter {
            type Item = Int;
            fn first(self) -> Self.Item = 1;
        };
        ",
    );
}

#[test]
fn test_typeck_trait_assoc_type_use_site_body_mismatch() {
    assert_has_diagnostic(
        "
        trait Iterator {
            type Item;
            fn first(self) -> Self.Item;
        };
        struct Counter {};
        impl Iterator for Counter {
            type Item = Int;
            fn first(self) -> Self.Item = true;
        };
        ",
        Severity::Error,
        "impl method `first` return type",
    );
}

#[test]
fn test_typeck_trait_method_call_uses_canonical_assoc_return_type() {
    check_no_errors(
        "
        trait Iterator {
            type Item;
            fn first(self) -> Self.Item;
        };
        impl Iterator for Int {
            type Item = String;
            fn first(self) -> Self.Item = toString(self);
        };
        let value: String = 1.first();
        ",
    );
}

#[test]
fn test_typeck_trait_method_call_checks_canonical_assoc_return_type() {
    assert_has_diagnostic(
        "
        trait Iterator {
            type Item;
            fn first(self) -> Self.Item;
        };
        impl Iterator for Int {
            type Item = String;
            fn first(self) -> Self.Item = toString(self);
        };
        fn expectInt(x: Int) -> Int = x;
        let value = expectInt(1.first());
        ",
        Severity::Error,
        "type mismatch",
    );
}

#[test]
fn test_typeck_trait_method_call_uses_canonical_default_assoc_alias_return_type() {
    check_no_errors(
        "
        trait Iterator {
            type Alias;
            type Item = Self.Alias;
            fn first(self) -> Self.Item;
        };
        impl Iterator for Int {
            type Alias = String;
            fn first(self) -> Self.Item = toString(self);
        };
        let value: String = 1.first();
        ",
    );
}

#[test]
fn test_typeck_trait_signature_mismatch_reports_assoc_projection_label() {
    assert_diagnostic_label_contains(
        "
        trait Iterator {
            type Item;
            fn first(self, fallback: Self.Item) -> Self.Item;
        };
        impl Iterator for Int {
            type Item = String;
            fn first(self, fallback: Int) -> Int = fallback;
        };
        ",
        Severity::Error,
        "does not match trait `Iterator` signature",
        "`Self.Item` resolves to `String` here",
    );
}

#[test]
fn test_typeck_impl_method_body_mismatch_reports_assoc_projection_label() {
    assert_diagnostic_label_contains(
        "
        trait Iterator {
            type Item;
            fn first(self) -> Self.Item;
        };
        impl Iterator for Int {
            type Item = String;
            fn first(self) -> Self.Item = true;
        };
        ",
        Severity::Error,
        "impl method `first` return type",
        "`Self.Item` resolves to `String` here",
    );
}

// ============================================================================
// 错误检测测试
// ============================================================================

#[test]
fn test_typeck_detects_type_error_in_if() {
    check_has_errors("let x = if true -> 1 else \"string\";");
}

#[test]
fn test_typeck_detects_type_error_in_list() {
    check_has_errors("let x = [1, 2, true];");
}

#[test]
fn test_typeck_detects_non_bool_condition() {
    check_has_errors("let x = if 42 -> 1 else 2;");
}

#[test]
fn test_typeck_detects_logical_on_non_bool() {
    check_has_errors("let x = 1 && true;");
}

#[test]
fn test_typeck_detects_not_on_non_bool() {
    check_has_errors("let x = !\"hello\";");
}

#[test]
fn test_typeck_detects_match_arm_mismatch() {
    check_has_errors(
        "
        let x = match 1 {
            0 -> 0,
            _ -> \"not zero\"
        };
    ",
    );
}
#[test]
fn test_typeck_live_cancel_rejects_non_live_argument() {
    check_has_errors(
        r#"
        use std.io = io;
        let x = io.liveCancel(1);
        "#,
    );
}

#[test]
fn test_typeck_jobs_rejects_arguments() {
    check_has_errors(
        r#"
        use std.io = io;
        let x = io.jobs(1);
        "#,
    );
}

#[test]
fn test_typeck_wait_any_job_rejects_arguments() {
    check_has_errors(
        r#"
        use std.io = io;
        let x = io.waitAnyJob(1);
        "#,
    );
}
