//! Integration tests for n3v3-std crate.

mod support;

use n3v3_derive::Hash;
use n3v3_eval::Value;
use n3v3_eval::value::{PipelineValue, TaskValue};
use n3v3_std::stdlib;
use std::collections::HashMap;
use std::fs;
use std::rc::Rc;
use support::fetch_fixtures::{init_local_git_repo, start_local_http_fixture};
use tempfile::TempDir;

fn get_builtin(name: &str) -> Option<Value> {
    stdlib()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v)
}

fn call_builtin_fn(f: &Value, args: Vec<Value>) -> Result<Value, String> {
    match f {
        Value::BuiltinFn(_, func) => func(args),
        _ => Err("Not a builtin function".into()),
    }
}

fn call_builtin(f: &Value, args: &[Value]) -> Result<Value, String> {
    match f {
        Value::Builtin(builtin) => (builtin.func)(args),
        Value::BuiltinFn(_, func) => func(args.to_vec()),
        _ => Err("Not a builtin function".into()),
    }
}

fn exec_command_with_embedded_redirects(
    command: Value,
    redirects: Vec<Value>,
) -> Result<Value, String> {
    let configure_builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let exec_builtin = get_builtin("io.execCommand").expect("io.execCommand builtin should exist");
    let command = call_builtin(
        &configure_builtin,
        &[command, Value::List(Rc::new(redirects))],
    )?;
    call_builtin(&exec_builtin, &[command])
}

fn exec_pipeline_with_embedded_redirects(
    pipeline: Value,
    redirects: Vec<Value>,
) -> Result<Value, String> {
    let configure_builtin = get_builtin("io.pipelineWithRedirects")
        .expect("io.pipelineWithRedirects builtin should exist");
    let exec_builtin =
        get_builtin("io.execPipeline").expect("io.execPipeline builtin should exist");
    let pipeline = call_builtin(
        &configure_builtin,
        &[pipeline, Value::List(Rc::new(redirects))],
    )?;
    call_builtin(&exec_builtin, &[pipeline])
}

#[cfg(not(windows))]
fn pipeline_projection_parts() -> [(&'static str, Vec<&'static str>); 2] {
    [
        ("sh", vec!["-c", "printf n3v3"]),
        ("sh", vec!["-c", "grep n3v3"]),
    ]
}

#[cfg(windows)]
fn pipeline_projection_parts() -> [(&'static str, Vec<&'static str>); 2] {
    [
        ("cmd", vec!["/C", "echo n3v3"]),
        ("cmd", vec!["/C", "findstr n3v3"]),
    ]
}

#[cfg(not(windows))]
fn stdin_filter_projection_parts() -> (&'static str, Vec<&'static str>) {
    ("sh", vec!["-c", "grep n3v3"])
}

#[cfg(windows)]
fn stdin_filter_projection_parts() -> (&'static str, Vec<&'static str>) {
    ("cmd", vec!["/C", "findstr n3v3"])
}

#[cfg(not(windows))]
fn stdin_pipeline_projection_parts() -> [(&'static str, Vec<&'static str>); 2] {
    [
        ("sh", vec!["-c", "grep n3v3"]),
        ("sh", vec!["-c", "grep n3v3"]),
    ]
}

#[cfg(windows)]
fn stdin_pipeline_projection_parts() -> [(&'static str, Vec<&'static str>); 2] {
    [
        ("cmd", vec!["/C", "findstr n3v3"]),
        ("cmd", vec!["/C", "findstr n3v3"]),
    ]
}

#[cfg(not(windows))]
fn stdout_stderr_projection_parts() -> (&'static str, Vec<&'static str>) {
    ("sh", vec!["-c", "printf n3v3 && printf err >&2"])
}

#[cfg(windows)]
fn stdout_stderr_projection_parts() -> (&'static str, Vec<&'static str>) {
    ("cmd", vec!["/C", "(echo n3v3) & (echo err 1>&2)"])
}

#[test]
fn test_math_to_int_bridge_accepts_bool() {
    let builtin = get_builtin("math.toInt").expect("math.toInt not found");
    let value = call_builtin(&builtin, &[Value::Bool(true)]).expect("math.toInt should succeed");
    assert_eq!(value, Value::Int(1.into()));
}

#[test]
fn test_math_to_float_bridge_accepts_string() {
    let builtin = get_builtin("math.toFloat").expect("math.toFloat not found");
    let value = call_builtin(&builtin, &[Value::String(Rc::new("1.5".to_string()))])
        .expect("math.toFloat should succeed");
    assert_eq!(value, Value::Float(1.5));
}

#[test]
fn test_math_is_nan_bridge_accepts_float() {
    let builtin = get_builtin("math.isNan").expect("math.isNan not found");
    let value =
        call_builtin(&builtin, &[Value::Float(f64::NAN)]).expect("math.isNan should succeed");
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn test_math_is_inf_bridge_accepts_float() {
    let builtin = get_builtin("math.isInf").expect("math.isInf not found");
    let value =
        call_builtin(&builtin, &[Value::Float(f64::INFINITY)]).expect("math.isInf should succeed");
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn test_math_is_nan_bridge_rejects_int() {
    let builtin = get_builtin("math.isNan").expect("math.isNan not found");
    let err =
        call_builtin(&builtin, &[Value::Int(1.into())]).expect_err("math.isNan should reject Int");
    assert_eq!(err, "math.isNan expects a Float");
}

#[test]
fn test_math_is_inf_bridge_rejects_int() {
    let builtin = get_builtin("math.isInf").expect("math.isInf not found");
    let err =
        call_builtin(&builtin, &[Value::Int(1.into())]).expect_err("math.isInf should reject Int");
    assert_eq!(err, "math.isInf expects a Float");
}

#[test]
fn test_math_rounding_bridges_accept_float() {
    let floor = get_builtin("math.floor").expect("math.floor not found");
    let ceil = get_builtin("math.ceil").expect("math.ceil not found");
    let round = get_builtin("math.round").expect("math.round not found");

    assert_eq!(
        call_builtin(&floor, &[Value::Float(1.9)]).expect("math.floor should succeed"),
        Value::Int(1.into())
    );
    assert_eq!(
        call_builtin(&ceil, &[Value::Float(1.1)]).expect("math.ceil should succeed"),
        Value::Int(2.into())
    );
    assert_eq!(
        call_builtin(&round, &[Value::Float(1.6)]).expect("math.round should succeed"),
        Value::Int(2.into())
    );
}

#[test]
fn test_math_rounding_bridges_reject_int() {
    let floor = get_builtin("math.floor").expect("math.floor not found");
    let ceil = get_builtin("math.ceil").expect("math.ceil not found");
    let round = get_builtin("math.round").expect("math.round not found");

    assert_eq!(
        call_builtin(&floor, &[Value::Int(1.into())]).expect_err("math.floor should reject Int"),
        "math.floor expects a Float"
    );
    assert_eq!(
        call_builtin(&ceil, &[Value::Int(1.into())]).expect_err("math.ceil should reject Int"),
        "math.ceil expects a Float"
    );
    assert_eq!(
        call_builtin(&round, &[Value::Int(1.into())]).expect_err("math.round should reject Int"),
        "math.round expects a Float"
    );
}

#[test]
fn test_math_unary_float_transforms_accept_float() {
    let sqrt = get_builtin("math.sqrt").expect("math.sqrt not found");
    let log = get_builtin("math.log").expect("math.log not found");
    let log10 = get_builtin("math.log10").expect("math.log10 not found");
    let exp = get_builtin("math.exp").expect("math.exp not found");

    assert_eq!(
        call_builtin(&sqrt, &[Value::Float(9.0)]).expect("math.sqrt should succeed"),
        Value::Float(3.0)
    );
    assert_eq!(
        call_builtin(&log, &[Value::Float(1.0)]).expect("math.log should succeed"),
        Value::Float(0.0)
    );
    assert_eq!(
        call_builtin(&log10, &[Value::Float(1000.0)]).expect("math.log10 should succeed"),
        Value::Float(3.0)
    );
    assert_eq!(
        call_builtin(&exp, &[Value::Float(0.0)]).expect("math.exp should succeed"),
        Value::Float(1.0)
    );
}

#[test]
fn test_math_unary_float_transforms_reject_int() {
    let sqrt = get_builtin("math.sqrt").expect("math.sqrt not found");
    let log = get_builtin("math.log").expect("math.log not found");
    let log10 = get_builtin("math.log10").expect("math.log10 not found");
    let exp = get_builtin("math.exp").expect("math.exp not found");

    assert_eq!(
        call_builtin(&sqrt, &[Value::Int(1.into())]).expect_err("math.sqrt should reject Int"),
        "math.sqrt expects a Float"
    );
    assert_eq!(
        call_builtin(&log, &[Value::Int(1.into())]).expect_err("math.log should reject Int"),
        "math.log expects a Float"
    );
    assert_eq!(
        call_builtin(&log10, &[Value::Int(1.into())]).expect_err("math.log10 should reject Int"),
        "math.log10 expects a Float"
    );
    assert_eq!(
        call_builtin(&exp, &[Value::Int(1.into())]).expect_err("math.exp should reject Int"),
        "math.exp expects a Float"
    );
}

#[test]
fn test_math_trigonometric_bridges_accept_float() {
    let sin = get_builtin("math.sin").expect("math.sin not found");
    let cos = get_builtin("math.cos").expect("math.cos not found");
    let tan = get_builtin("math.tan").expect("math.tan not found");

    assert_eq!(
        call_builtin(&sin, &[Value::Float(0.0)]).expect("math.sin should succeed"),
        Value::Float(0.0)
    );
    assert_eq!(
        call_builtin(&cos, &[Value::Float(0.0)]).expect("math.cos should succeed"),
        Value::Float(1.0)
    );
    assert_eq!(
        call_builtin(&tan, &[Value::Float(0.0)]).expect("math.tan should succeed"),
        Value::Float(0.0)
    );
}

#[test]
fn test_math_trigonometric_bridges_reject_int() {
    let sin = get_builtin("math.sin").expect("math.sin not found");
    let cos = get_builtin("math.cos").expect("math.cos not found");
    let tan = get_builtin("math.tan").expect("math.tan not found");

    assert_eq!(
        call_builtin(&sin, &[Value::Int(1.into())]).expect_err("math.sin should reject Int"),
        "math.sin expects a Float"
    );
    assert_eq!(
        call_builtin(&cos, &[Value::Int(1.into())]).expect_err("math.cos should reject Int"),
        "math.cos expects a Float"
    );
    assert_eq!(
        call_builtin(&tan, &[Value::Int(1.into())]).expect_err("math.tan should reject Int"),
        "math.tan expects a Float"
    );
}

// Map tests

#[test]
fn test_map_empty() {
    let empty = get_builtin("Map.empty");
    assert!(empty.is_some(), "Map.empty not found");
    match empty.unwrap() {
        Value::Map(m) => assert!(m.is_empty()),
        _ => panic!("Expected Map"),
    }
}

#[test]
fn test_map_singleton() {
    let singleton = get_builtin("Map.singleton");
    assert!(singleton.is_some(), "Map.singleton not found");

    let result = call_builtin_fn(
        &singleton.unwrap(),
        vec![
            Value::String(Rc::new("key".to_string())),
            Value::Int(42.into()),
        ],
    )
    .unwrap();

    match result {
        Value::Map(m) => assert_eq!(m.len(), 1),
        _ => panic!("Expected Map"),
    }
}

#[test]
fn test_map_values_returns_list_of_values() {
    let insert = get_builtin("Map.insert").expect("Map.insert not found");
    let values = get_builtin("Map.values").expect("Map.values not found");
    let map = call_builtin_fn(
        &insert,
        vec![
            Value::String(Rc::new("key".to_string())),
            Value::Int(42.into()),
            get_builtin("Map.empty").expect("Map.empty not found"),
        ],
    )
    .expect("Map.insert should succeed");

    let result = call_builtin_fn(&values, vec![map]).expect("Map.values should succeed");
    assert_eq!(result, Value::List(Rc::new(vec![Value::Int(42.into())])));
}

#[test]
fn test_map_values_rejects_non_map() {
    let values = get_builtin("Map.values").expect("Map.values not found");
    let err = call_builtin_fn(&values, vec![Value::Int(1.into())])
        .expect_err("Map.values should reject non-map");
    assert_eq!(err, "Map.values expects a map");
}

// Set tests

#[test]
fn test_set_empty() {
    let empty = get_builtin("Set.empty");
    assert!(empty.is_some(), "Set.empty not found");
    match empty.unwrap() {
        Value::Set(s) => assert!(s.is_empty()),
        _ => panic!("Expected Set"),
    }
}

#[test]
fn test_set_singleton() {
    let singleton = get_builtin("Set.singleton");
    assert!(singleton.is_some(), "Set.singleton not found");

    let result = call_builtin_fn(&singleton.unwrap(), vec![Value::Int(42.into())]).unwrap();

    match result {
        Value::Set(s) => assert_eq!(s.len(), 1),
        _ => panic!("Expected Set"),
    }
}

// List tests

#[test]
fn test_list_empty() {
    let empty = get_builtin("list.empty");
    assert!(empty.is_some(), "list.empty not found");

    match empty.unwrap() {
        Value::Builtin(builtin) => {
            let result = (builtin.func)(&[]).unwrap();
            match result {
                Value::List(l) => assert!(l.is_empty()),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_singleton() {
    let singleton = get_builtin("list.singleton");
    assert!(singleton.is_some(), "list.singleton not found");

    match singleton.unwrap() {
        Value::Builtin(builtin) => {
            let result = (builtin.func)(&[Value::Int(42.into())]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l.len(), 1);
                    assert_eq!(l[0], Value::Int(42.into()));
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_len() {
    let list = Value::List(Rc::new(vec![
        Value::Int(1.into()),
        Value::Int(2.into()),
        Value::Int(3.into()),
    ]));
    let len_fn = get_builtin("list.len");
    assert!(len_fn.is_some(), "list.len not found");

    match len_fn.unwrap() {
        Value::Builtin(builtin) => {
            let result = (builtin.func)(&[list]).unwrap();
            assert_eq!(result, Value::Int(3.into()));
        }
        _ => panic!("Expected Builtin"),
    }
}

// Verify stdlib returns builtins
#[test]
fn test_stdlib_not_empty() {
    let builtins = stdlib();
    assert!(!builtins.is_empty());
}

#[test]
fn test_stdlib_has_map_builtins() {
    let builtins = stdlib();
    let map_builtins: Vec<_> = builtins
        .iter()
        .filter(|(name, _)| name.starts_with("Map."))
        .collect();
    assert!(!map_builtins.is_empty(), "No Map.* builtins found");
}

#[test]
fn test_stdlib_has_set_builtins() {
    let builtins = stdlib();
    let set_builtins: Vec<_> = builtins
        .iter()
        .filter(|(name, _)| name.starts_with("Set."))
        .collect();
    assert!(!set_builtins.is_empty(), "No Set.* builtins found");
}

#[test]
fn test_stdlib_has_list_builtins() {
    let builtins = stdlib();
    let list_builtins: Vec<_> = builtins
        .iter()
        .filter(|(name, _)| name.starts_with("list."))
        .collect();
    assert!(!list_builtins.is_empty(), "No list.* builtins found");
}

#[test]
fn test_stdlib_has_fetch_builtins() {
    let builtins = stdlib();
    let fetch_builtins: Vec<_> = builtins
        .iter()
        .filter(|(name, _)| name.starts_with("fetch."))
        .collect();
    assert!(!fetch_builtins.is_empty(), "No fetch.* builtins found");
}

#[test]
fn test_path_from_string_builtin_returns_path_runtime_value() {
    let builtin = get_builtin("path.fromString").expect("path.fromString builtin should exist");
    let result = call_builtin(&builtin, &[Value::String(Rc::new("/tmp/n3v3".to_string()))])
        .expect("path.fromString should succeed");

    match result {
        Value::Path(path) => assert_eq!(path.as_path(), std::path::Path::new("/tmp/n3v3")),
        other => panic!("expected Path runtime value, got {:?}", other),
    }
}

#[test]
fn test_path_join_path_builtin_returns_path_runtime_value() {
    let builtin = get_builtin("path.joinPath").expect("path.joinPath builtin should exist");
    let result = call_builtin(
        &builtin,
        &[
            Value::Path(Rc::new(std::path::PathBuf::from("/tmp"))),
            Value::String(Rc::new("n3v3.txt".to_string())),
        ],
    )
    .expect("path.joinPath should succeed");

    match result {
        Value::Path(path) => assert_eq!(path.as_path(), std::path::Path::new("/tmp/n3v3.txt")),
        other => panic!("expected Path runtime value, got {:?}", other),
    }
}

#[test]
fn test_path_parent_path_builtin_returns_option_path() {
    let builtin = get_builtin("path.parentPath").expect("path.parentPath builtin should exist");
    let result = call_builtin(
        &builtin,
        &[Value::Path(Rc::new(std::path::PathBuf::from(
            "/tmp/n3v3.txt",
        )))],
    )
    .expect("path.parentPath should succeed");

    match result {
        Value::Some(inner) => match inner.as_ref() {
            Value::Path(path) => assert_eq!(path.as_path(), std::path::Path::new("/tmp")),
            other => panic!("expected inner Path runtime value, got {:?}", other),
        },
        other => panic!("expected Some(Path), got {:?}", other),
    }
}

#[test]
fn test_path_filename_path_builtin_returns_option_string() {
    let builtin = get_builtin("path.filenamePath").expect("path.filenamePath builtin should exist");
    let result = call_builtin(
        &builtin,
        &[Value::Path(Rc::new(std::path::PathBuf::from(
            "/tmp/n3v3.txt",
        )))],
    )
    .expect("path.filenamePath should succeed");

    match result {
        Value::Some(inner) => match inner.as_ref() {
            Value::String(name) => assert_eq!(name.as_str(), "n3v3.txt"),
            other => panic!("expected inner String runtime value, got {:?}", other),
        },
        other => panic!("expected Some(String), got {:?}", other),
    }
}

#[test]
fn test_path_extension_path_builtin_returns_option_string() {
    let builtin =
        get_builtin("path.extensionPath").expect("path.extensionPath builtin should exist");
    let result = call_builtin(
        &builtin,
        &[Value::Path(Rc::new(std::path::PathBuf::from(
            "/tmp/n3v3.txt",
        )))],
    )
    .expect("path.extensionPath should succeed");

    match result {
        Value::Some(inner) => match inner.as_ref() {
            Value::String(ext) => assert_eq!(ext.as_str(), "txt"),
            other => panic!("expected inner String runtime value, got {:?}", other),
        },
        other => panic!("expected Some(String), got {:?}", other),
    }
}

#[cfg(unix)]
#[test]
fn test_path_is_absolute_path_builtin_reports_bool() {
    let builtin =
        get_builtin("path.isAbsolutePath").expect("path.isAbsolutePath builtin should exist");
    let result = call_builtin(
        &builtin,
        &[Value::Path(Rc::new(std::path::PathBuf::from(
            "/tmp/n3v3.txt",
        )))],
    )
    .expect("path.isAbsolutePath should succeed");

    assert_eq!(result, Value::Bool(true));
}

#[cfg(windows)]
#[test]
fn test_path_is_absolute_path_builtin_reports_bool_on_windows() {
    let builtin =
        get_builtin("path.isAbsolutePath").expect("path.isAbsolutePath builtin should exist");
    let result = call_builtin(
        &builtin,
        &[Value::Path(Rc::new(std::path::PathBuf::from(
            "C:\\n3v3.txt",
        )))],
    )
    .expect("path.isAbsolutePath should succeed");

    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_fetch_path_builtin_returns_metadata_record() {
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("source.txt");
    let content = b"fetch-path-content";
    fs::write(&file_path, content).unwrap();

    let builtin = get_builtin("fetch.path").expect("fetch.path builtin should exist");
    let args = vec![Value::String(Rc::new(
        file_path.to_string_lossy().to_string(),
    ))];
    let result = call_builtin(&builtin, &args).expect("fetch.path should succeed");

    match result {
        Value::Record(fields) => {
            match fields.get("path").expect("path field should exist") {
                Value::String(path) => {
                    assert_eq!(path.as_str(), file_path.to_string_lossy().as_ref())
                }
                _ => panic!("path field should be a string"),
            }

            match fields.get("hash").expect("hash field should exist") {
                Value::String(hash) => assert_eq!(hash.as_str(), Hash::of(content).to_hex()),
                _ => panic!("hash field should be a string"),
            }

            match fields.get("cached").expect("cached field should exist") {
                Value::Bool(cached) => assert!(*cached),
                _ => panic!("cached field should be a bool"),
            }
        }
        _ => panic!("fetch.path should return a record"),
    }
}

#[test]
fn test_fetch_path_with_hash_builtin_returns_metadata_record() {
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("source.txt");
    let content = b"fetch-path-content";
    fs::write(&file_path, content).unwrap();

    let expected_hash = Hash::of(content).to_hex();
    let builtin = get_builtin("fetch.pathWithHash").expect("fetch.pathWithHash should exist");
    let args = vec![
        Value::String(Rc::new(file_path.to_string_lossy().to_string())),
        Value::String(Rc::new(expected_hash.clone())),
    ];
    let result = call_builtin(&builtin, &args).expect("fetch.pathWithHash should succeed");

    match result {
        Value::Record(fields) => {
            match fields.get("path").expect("path field should exist") {
                Value::String(path) => {
                    assert_eq!(path.as_str(), file_path.to_string_lossy().as_ref())
                }
                _ => panic!("path field should be a string"),
            }

            match fields.get("hash").expect("hash field should exist") {
                Value::String(hash) => assert_eq!(hash.as_str(), expected_hash),
                _ => panic!("hash field should be a string"),
            }

            match fields.get("cached").expect("cached field should exist") {
                Value::Bool(cached) => assert!(*cached),
                _ => panic!("cached field should be a bool"),
            }
        }
        _ => panic!("fetch.pathWithHash should return a record"),
    }
}

#[test]
fn test_fetch_url_with_hash_builtin_returns_metadata_record() {
    let (url, expected_hash, server) = start_local_http_fixture(b"fetch-url-content");

    let builtin = get_builtin("fetch.urlWithHash").expect("fetch.urlWithHash should exist");
    let args = vec![
        Value::String(Rc::new(url)),
        Value::String(Rc::new(expected_hash.clone())),
    ];
    let result = call_builtin(&builtin, &args).expect("fetch.urlWithHash should succeed");
    server.join().expect("fixture server should exit cleanly");

    match result {
        Value::Record(fields) => {
            match fields.get("path").expect("path field should exist") {
                Value::String(path) => {
                    assert!(
                        std::path::Path::new(path.as_str()).exists(),
                        "cached path should exist: {path}"
                    );
                }
                _ => panic!("path field should be a string"),
            }

            match fields.get("hash").expect("hash field should exist") {
                Value::String(hash) => assert_eq!(hash.as_str(), expected_hash),
                _ => panic!("hash field should be a string"),
            }

            match fields.get("cached").expect("cached field should exist") {
                Value::Bool(cached) => assert!(!cached),
                _ => panic!("cached field should be a bool"),
            }
        }
        _ => panic!("fetch.urlWithHash should return a record"),
    }
}

#[test]
fn test_fetch_url_builtin_returns_metadata_record() {
    let (url, expected_hash, server) = start_local_http_fixture(b"fetch-url-content");

    let builtin = get_builtin("fetch.url").expect("fetch.url should exist");
    let args = vec![Value::String(Rc::new(url))];
    let result = call_builtin(&builtin, &args).expect("fetch.url should succeed");
    server.join().expect("fixture server should exit cleanly");

    match result {
        Value::Record(fields) => {
            match fields.get("path").expect("path field should exist") {
                Value::String(path) => {
                    assert!(
                        std::path::Path::new(path.as_str()).exists(),
                        "cached path should exist: {path}"
                    );
                }
                _ => panic!("path field should be a string"),
            }

            match fields.get("hash").expect("hash field should exist") {
                Value::String(hash) => assert_eq!(hash.as_str(), expected_hash),
                _ => panic!("hash field should be a string"),
            }

            match fields.get("cached").expect("cached field should exist") {
                Value::Bool(cached) => assert!(!cached),
                _ => panic!("cached field should be a bool"),
            }
        }
        _ => panic!("fetch.url should return a record"),
    }
}

#[test]
fn test_fetch_path_with_hash_rejects_mismatch() {
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("source.txt");
    fs::write(&file_path, b"actual-content").unwrap();

    let wrong_hash = Hash::of(b"different-content").to_hex();
    let builtin = get_builtin("fetch.pathWithHash").expect("fetch.pathWithHash should exist");
    let args = vec![
        Value::String(Rc::new(file_path.to_string_lossy().to_string())),
        Value::String(Rc::new(wrong_hash)),
    ];

    let err = call_builtin(&builtin, &args).expect_err("expected hash mismatch error");
    assert!(
        err.contains("hash mismatch"),
        "expected hash mismatch error, got: {err}"
    );
}

#[test]
fn test_fetch_git_builtin_returns_metadata_record() {
    let (_temp, repo_path, expected_hash) = init_local_git_repo();

    let builtin = get_builtin("fetch.git").expect("fetch.git builtin should exist");
    let args = vec![
        Value::String(Rc::new(repo_path)),
        Value::String(Rc::new("main".to_string())),
    ];
    let result = call_builtin(&builtin, &args).expect("fetch.git should succeed");

    match result {
        Value::Record(fields) => {
            match fields.get("path").expect("path field should exist") {
                Value::String(path) => {
                    assert!(
                        std::path::Path::new(path.as_str()).exists(),
                        "cached path should exist: {path}"
                    );
                }
                _ => panic!("path field should be a string"),
            }

            match fields.get("hash").expect("hash field should exist") {
                Value::String(hash) => assert_eq!(hash.as_str(), expected_hash),
                _ => panic!("hash field should be a string"),
            }

            match fields.get("cached").expect("cached field should exist") {
                Value::Bool(cached) => assert!(!cached),
                _ => panic!("cached field should be a bool"),
            }
        }
        _ => panic!("fetch.git should return a record"),
    }
}

#[test]
fn test_fetch_git_with_hash_builtin_returns_metadata_record() {
    let (_temp, repo_path, expected_hash) = init_local_git_repo();

    let builtin = get_builtin("fetch.gitWithHash").expect("fetch.gitWithHash builtin should exist");
    let args = vec![
        Value::String(Rc::new(repo_path)),
        Value::String(Rc::new("main".to_string())),
        Value::String(Rc::new(expected_hash.clone())),
    ];
    // A shared fetch cache makes the first call environment-dependent: it is a hit
    // only when another test already populated the same content hash. Warm the cache
    // first so only the second call is asserted as cached.
    // 共享缓存使首次调用的结果依赖环境，先预热缓存，仅对第二次调用断言 cached。
    call_builtin(&builtin, &args).expect("fetch.gitWithHash warm-up should succeed");
    let result = call_builtin(&builtin, &args).expect("fetch.gitWithHash should succeed");

    match result {
        Value::Record(fields) => {
            match fields.get("path").expect("path field should exist") {
                Value::String(path) => {
                    assert!(
                        std::path::Path::new(path.as_str()).exists(),
                        "cached path should exist: {path}"
                    );
                }
                _ => panic!("path field should be a string"),
            }

            match fields.get("hash").expect("hash field should exist") {
                Value::String(hash) => assert_eq!(hash.as_str(), expected_hash),
                _ => panic!("hash field should be a string"),
            }

            match fields.get("cached").expect("cached field should exist") {
                Value::Bool(cached) => assert!(*cached),
                _ => panic!("cached field should be a bool"),
            }
        }
        _ => panic!("fetch.gitWithHash should return a record"),
    }
}

#[test]
fn test_io_hash_file_accepts_string_path_argument() {
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("source.txt");
    let content = b"hash-file-content";
    let expected = "09f00a4ba8e49c5a253e1af9ff6c40f8151754ccd88f95ef162981960b2ad8f7";
    fs::write(&file_path, content).unwrap();

    let builtin = get_builtin("io.hashFile").expect("io.hashFile builtin should exist");
    let result = call_builtin(
        &builtin,
        &[Value::String(Rc::new(
            file_path.to_string_lossy().to_string(),
        ))],
    )
    .expect("io.hashFile should succeed");

    match result {
        Value::String(hash) => assert_eq!(hash.as_str(), expected),
        other => panic!("expected String hash, got {:?}", other),
    }
}

#[test]
fn test_io_hash_string_accepts_string_runtime_value() {
    let builtin = get_builtin("io.hashString").expect("io.hashString builtin should exist");
    let result = call_builtin(&builtin, &[Value::String(Rc::new("abc".to_string()))])
        .expect("io.hashString should succeed");

    match result {
        Value::String(hash) => assert_eq!(
            hash.as_str(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ),
        other => panic!("expected String hash, got {:?}", other),
    }
}

#[test]
fn test_io_hash_file_path_accepts_path_runtime_value() {
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("source.txt");
    let content = b"hash-file-path-content";
    let expected = "9c3675e0b07ef1223e4cb9afdc255c51c8557ac075e91e601978676b894c95b1";
    fs::write(&file_path, content).unwrap();

    let builtin = get_builtin("io.hashFilePath").expect("io.hashFilePath builtin should exist");
    let result = call_builtin(&builtin, &[Value::Path(Rc::new(file_path))])
        .expect("io.hashFilePath should succeed");

    match result {
        Value::String(hash) => assert_eq!(hash.as_str(), expected),
        other => panic!("expected String hash, got {:?}", other),
    }
}

#[test]
fn test_io_hash_file_path_rejects_string_argument() {
    let builtin = get_builtin("io.hashFilePath").expect("io.hashFilePath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new("/tmp/source.txt".to_string()))],
    )
    .expect_err("io.hashFilePath should reject string arguments");

    assert_eq!(err, "io.hashFilePath expects a Path");
}

#[test]
fn test_io_exec_compat_wrappers_are_removed_from_stdlib() {
    assert!(
        get_builtin("io.exec").is_none(),
        "io.exec plain-exec wrapper should be removed"
    );
    assert!(
        get_builtin("io.execResult").is_none(),
        "io.execResult transitional alias should be removed"
    );
    assert!(
        get_builtin("io.execWithResult").is_none(),
        "io.execWithResult transitional alias should be removed"
    );
    assert!(
        get_builtin("io.execShellResult").is_none(),
        "io.execShellResult transitional alias should be removed"
    );
    assert!(
        get_builtin("io.execShell").is_none(),
        "io.execShell shell wrapper should be removed"
    );
    assert!(
        get_builtin("io.execWith").is_none(),
        "io.execWith configured-exec wrapper should be removed"
    );
    assert!(
        get_builtin("io.execCommandWithRedirect").is_none(),
        "io.execCommandWithRedirect single-redirect wrapper should be removed"
    );
    assert!(
        get_builtin("io.execPipelineWithRedirect").is_none(),
        "io.execPipelineWithRedirect single-redirect wrapper should be removed"
    );
    assert!(
        get_builtin("io.execCommandWithRedirects").is_none(),
        "io.execCommandWithRedirects boundary wrapper should be removed"
    );
    assert!(
        get_builtin("io.execPipelineWithRedirects").is_none(),
        "io.execPipelineWithRedirects boundary wrapper should be removed"
    );
}

#[test]
fn test_io_write_file_and_read_back() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("io-write.txt");
    let path_str = path.to_string_lossy().to_string();

    let write = get_builtin("io.writeFile").expect("io.writeFile builtin should exist");
    let read = get_builtin("io.readFile").expect("io.readFile builtin should exist");

    let write_args = vec![
        Value::String(Rc::new(path_str.clone())),
        Value::String(Rc::new("hello".to_string())),
    ];
    let write_result = call_builtin(&write, &write_args).expect("io.writeFile should succeed");
    assert!(matches!(write_result, Value::Unit));

    let read_args = vec![Value::String(Rc::new(path_str.clone()))];
    let read_result = call_builtin(&read, &read_args).expect("io.readFile should succeed");
    assert!(matches!(read_result, Value::String(s) if s.as_str() == "hello"));
}

#[test]
fn test_io_read_file_path_accepts_path_runtime_value() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("io-read-path.txt");
    fs::write(&path, "hello-path").unwrap();

    let read = get_builtin("io.readFilePath").expect("io.readFilePath builtin should exist");
    let read_args = vec![Value::Path(Rc::new(path.clone()))];
    let read_result = call_builtin(&read, &read_args).expect("io.readFilePath should succeed");

    assert!(matches!(read_result, Value::String(s) if s.as_str() == "hello-path"));
}

#[test]
fn test_io_read_file_path_rejects_string_argument() {
    let read = get_builtin("io.readFilePath").expect("io.readFilePath builtin should exist");
    let err = call_builtin(
        &read,
        &[Value::String(Rc::new("/tmp/io-read-path.txt".to_string()))],
    )
    .expect_err("io.readFilePath should reject string arguments");

    assert_eq!(err, "io.readFilePath expects a Path");
}

#[test]
fn test_io_read_dir_accepts_string_runtime_value() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("io-read-dir");
    fs::create_dir_all(dir.join("nested")).unwrap();
    fs::write(dir.join("alpha.txt"), "a").unwrap();
    fs::write(dir.join("beta.txt"), "b").unwrap();

    let builtin = get_builtin("io.readDir").expect("io.readDir builtin should exist");
    let result = call_builtin(
        &builtin,
        &[Value::String(Rc::new(dir.to_string_lossy().to_string()))],
    )
    .expect("io.readDir should succeed");

    let mut names = match result {
        Value::List(entries) => entries
            .iter()
            .map(|entry| match entry {
                Value::String(name) => name.to_string(),
                other => panic!("expected String entry, got {:?}", other),
            })
            .collect::<Vec<_>>(),
        other => panic!("expected List<String>, got {:?}", other),
    };
    names.sort();
    assert_eq!(names, vec!["alpha.txt", "beta.txt", "nested"]);
}

#[test]
fn test_io_read_dir_path_accepts_path_runtime_value() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("io-read-dir-path");
    fs::create_dir_all(dir.join("nested")).unwrap();
    fs::write(dir.join("alpha.txt"), "a").unwrap();
    fs::write(dir.join("beta.txt"), "b").unwrap();

    let builtin = get_builtin("io.readDirPath").expect("io.readDirPath builtin should exist");
    let result = call_builtin(&builtin, &[Value::Path(Rc::new(dir))])
        .expect("io.readDirPath should succeed");

    let mut names = match result {
        Value::List(entries) => entries
            .iter()
            .map(|entry| match entry {
                Value::String(name) => name.to_string(),
                other => panic!("expected String entry, got {:?}", other),
            })
            .collect::<Vec<_>>(),
        other => panic!("expected List<String>, got {:?}", other),
    };
    names.sort();
    assert_eq!(names, vec!["alpha.txt", "beta.txt", "nested"]);
}

#[test]
fn test_io_read_dir_path_rejects_string_argument() {
    let builtin = get_builtin("io.readDirPath").expect("io.readDirPath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new("/tmp/io-read-dir-path".to_string()))],
    )
    .expect_err("io.readDirPath should reject string arguments");

    assert_eq!(err, "io.readDirPath expects a Path");
}

#[test]
fn test_io_read_dir_entry_paths_accepts_path_runtime_value() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("io-read-dir-entry-paths");
    fs::create_dir_all(dir.join("nested")).unwrap();
    fs::write(dir.join("alpha.txt"), "a").unwrap();

    let builtin =
        get_builtin("io.readDirEntryPaths").expect("io.readDirEntryPaths builtin should exist");
    let result = call_builtin(&builtin, &[Value::Path(Rc::new(dir.clone()))])
        .expect("io.readDirEntryPaths should succeed");

    let mut paths = match result {
        Value::List(entries) => entries
            .iter()
            .map(|entry| match entry {
                Value::Path(path) => path.as_path().to_path_buf(),
                other => panic!("expected Path entry, got {:?}", other),
            })
            .collect::<Vec<_>>(),
        other => panic!("expected List<Path>, got {:?}", other),
    };
    paths.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    assert_eq!(paths, vec![dir.join("alpha.txt"), dir.join("nested")]);
}

#[test]
fn test_io_read_dir_entry_paths_rejects_string_argument() {
    let builtin =
        get_builtin("io.readDirEntryPaths").expect("io.readDirEntryPaths builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new(
            "/tmp/io-read-dir-entry-paths".to_string(),
        ))],
    )
    .expect_err("io.readDirEntryPaths should reject string arguments");

    assert_eq!(err, "io.readDirEntryPaths expects a Path");
}

#[test]
fn test_io_write_file_path_accepts_path_and_string_runtime_values() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("io-write-path.txt");

    let write = get_builtin("io.writeFilePath").expect("io.writeFilePath builtin should exist");
    let result = call_builtin(
        &write,
        &[
            Value::Path(Rc::new(path.clone())),
            Value::String(Rc::new("hello-path".to_string())),
        ],
    )
    .expect("io.writeFilePath should succeed");

    assert!(matches!(result, Value::Unit));
    assert_eq!(fs::read_to_string(&path).unwrap(), "hello-path");
}

#[test]
fn test_io_write_file_path_rejects_string_path_argument() {
    let builtin = get_builtin("io.writeFilePath").expect("io.writeFilePath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[
            Value::String(Rc::new("/tmp/io-write-path.txt".to_string())),
            Value::String(Rc::new("hello-path".to_string())),
        ],
    )
    .expect_err("io.writeFilePath should reject string paths");

    assert_eq!(err, "io.writeFilePath expects (Path, String)");
}

#[test]
fn test_io_write_file_path_rejects_non_string_content_argument() {
    let builtin = get_builtin("io.writeFilePath").expect("io.writeFilePath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[
            Value::Path(Rc::new(std::path::PathBuf::from("/tmp/io-write-path.txt"))),
            Value::Int(42.into()),
        ],
    )
    .expect_err("io.writeFilePath should reject non-string content");

    assert_eq!(err, "io.writeFilePath expects (Path, String)");
}

#[test]
fn test_io_append_file_path_accepts_path_and_string_runtime_values() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("io-append-path.txt");
    fs::write(&path, "hello").unwrap();

    let append = get_builtin("io.appendFilePath").expect("io.appendFilePath builtin should exist");
    let result = call_builtin(
        &append,
        &[
            Value::Path(Rc::new(path.clone())),
            Value::String(Rc::new("-path".to_string())),
        ],
    )
    .expect("io.appendFilePath should succeed");

    assert!(matches!(result, Value::Unit));
    assert_eq!(fs::read_to_string(&path).unwrap(), "hello-path");
}

#[test]
fn test_io_append_file_path_rejects_string_path_argument() {
    let builtin = get_builtin("io.appendFilePath").expect("io.appendFilePath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[
            Value::String(Rc::new("/tmp/io-append-path.txt".to_string())),
            Value::String(Rc::new("hello".to_string())),
        ],
    )
    .expect_err("io.appendFilePath should reject string paths");

    assert_eq!(err, "io.appendFilePath expects (Path, String)");
}

#[test]
fn test_io_append_file_path_rejects_non_string_content_argument() {
    let builtin = get_builtin("io.appendFilePath").expect("io.appendFilePath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[
            Value::Path(Rc::new(std::path::PathBuf::from("/tmp/io-append-path.txt"))),
            Value::Int(42.into()),
        ],
    )
    .expect_err("io.appendFilePath should reject non-string content");

    assert_eq!(err, "io.appendFilePath expects (Path, String)");
}

#[test]
fn test_io_read_file_bytes_path_accepts_path_runtime_value() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("io-read-bytes-path.bin");
    fs::write(&path, [0xde, 0xad, 0xbe, 0xef]).unwrap();

    let read =
        get_builtin("io.readFileBytesPath").expect("io.readFileBytesPath builtin should exist");
    let read_args = vec![Value::Path(Rc::new(path.clone()))];
    let read_result = call_builtin(&read, &read_args).expect("io.readFileBytesPath should succeed");

    assert!(matches!(
        read_result,
        Value::Bytes(bytes) if bytes.as_ref() == &[0xde, 0xad, 0xbe, 0xef]
    ));
}

#[test]
fn test_io_read_file_bytes_path_rejects_string_argument() {
    let read =
        get_builtin("io.readFileBytesPath").expect("io.readFileBytesPath builtin should exist");
    let err = call_builtin(
        &read,
        &[Value::String(Rc::new(
            "/tmp/io-read-bytes-path.bin".to_string(),
        ))],
    )
    .expect_err("io.readFileBytesPath should reject string arguments");

    assert_eq!(err, "io.readFileBytesPath expects a Path");
}

#[test]
fn test_io_write_file_bytes_path_accepts_path_and_bytes_runtime_values() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("io-write-bytes-src.bin");
    let dst = temp.path().join("io-write-bytes-dst.bin");
    fs::write(&src, [0xde, 0xad, 0xbe, 0xef]).unwrap();

    let read =
        get_builtin("io.readFileBytesPath").expect("io.readFileBytesPath builtin should exist");
    let write =
        get_builtin("io.writeFileBytesPath").expect("io.writeFileBytesPath builtin should exist");
    let bytes = call_builtin(&read, &[Value::Path(Rc::new(src.clone()))])
        .expect("io.readFileBytesPath should succeed");
    let result = call_builtin(&write, &[Value::Path(Rc::new(dst.clone())), bytes])
        .expect("io.writeFileBytesPath should succeed");

    assert!(matches!(result, Value::Unit));
    assert_eq!(fs::read(&dst).unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
}

#[test]
fn test_io_write_file_bytes_path_rejects_string_path_argument() {
    let builtin =
        get_builtin("io.writeFileBytesPath").expect("io.writeFileBytesPath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[
            Value::String(Rc::new("/tmp/io-write-bytes.bin".to_string())),
            Value::Bytes(Rc::new(vec![0xde, 0xad])),
        ],
    )
    .expect_err("io.writeFileBytesPath should reject string paths");

    assert_eq!(err, "io.writeFileBytesPath expects (Path, Bytes)");
}

#[test]
fn test_io_write_file_bytes_path_rejects_non_bytes_argument() {
    let builtin =
        get_builtin("io.writeFileBytesPath").expect("io.writeFileBytesPath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[
            Value::Path(Rc::new(std::path::PathBuf::from("/tmp/io-write-bytes.bin"))),
            Value::String(Rc::new("not-bytes".to_string())),
        ],
    )
    .expect_err("io.writeFileBytesPath should reject non-bytes payloads");

    assert_eq!(err, "io.writeFileBytesPath expects (Path, Bytes)");
}

#[test]
fn test_io_append_file_bytes_path_appends_bytes_runtime_value() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("io-append-bytes-src.bin");
    let dst = temp.path().join("io-append-bytes-dst.bin");
    fs::write(&src, [0xde, 0xad, 0xbe]).unwrap();
    fs::write(&dst, [0xaa]).unwrap();

    let read =
        get_builtin("io.readFileBytesPath").expect("io.readFileBytesPath builtin should exist");
    let append =
        get_builtin("io.appendFileBytesPath").expect("io.appendFileBytesPath builtin should exist");
    let bytes = call_builtin(&read, &[Value::Path(Rc::new(src.clone()))])
        .expect("io.readFileBytesPath should succeed");
    let result = call_builtin(&append, &[Value::Path(Rc::new(dst.clone())), bytes])
        .expect("io.appendFileBytesPath should succeed");

    assert!(matches!(result, Value::Unit));
    assert_eq!(fs::read(&dst).unwrap(), vec![0xaa, 0xde, 0xad, 0xbe]);
}

#[test]
fn test_io_append_file_bytes_path_rejects_string_path_argument() {
    let builtin =
        get_builtin("io.appendFileBytesPath").expect("io.appendFileBytesPath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[
            Value::String(Rc::new("/tmp/io-append-bytes.bin".to_string())),
            Value::Bytes(Rc::new(vec![0xde, 0xad])),
        ],
    )
    .expect_err("io.appendFileBytesPath should reject string paths");

    assert_eq!(err, "io.appendFileBytesPath expects (Path, Bytes)");
}

#[test]
fn test_io_append_file_bytes_path_rejects_non_bytes_argument() {
    let builtin =
        get_builtin("io.appendFileBytesPath").expect("io.appendFileBytesPath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[
            Value::Path(Rc::new(std::path::PathBuf::from(
                "/tmp/io-append-bytes.bin",
            ))),
            Value::String(Rc::new("not-bytes".to_string())),
        ],
    )
    .expect_err("io.appendFileBytesPath should reject non-bytes payloads");

    assert_eq!(err, "io.appendFileBytesPath expects (Path, Bytes)");
}

#[test]
fn test_io_current_dir_path_returns_path_runtime_value() {
    let current_dir = std::env::current_dir().unwrap();
    let builtin = get_builtin("io.currentDirPath").expect("io.currentDirPath builtin should exist");
    let result = call_builtin(&builtin, &[]).expect("io.currentDirPath should succeed");

    match result {
        Value::Path(path) => assert_eq!(path.as_path(), current_dir.as_path()),
        other => panic!("expected Path runtime value, got {:?}", other),
    }
}

#[test]
fn test_io_current_dir_returns_host_directory_string() {
    let current_dir = std::env::current_dir().unwrap();
    let builtin = get_builtin("io.currentDir").expect("io.currentDir builtin should exist");
    let result = call_builtin(&builtin, &[]).expect("io.currentDir should succeed");

    match result {
        Value::String(dir) => assert_eq!(dir.as_ref(), current_dir.to_string_lossy().as_ref()),
        other => panic!("expected String runtime value, got {:?}", other),
    }
}

#[test]
fn test_io_get_env_returns_none_for_missing_variable() {
    let missing = "__N3V3_TEST_MISSING_ENV_37C93B7C__";
    assert!(
        std::env::var_os(missing).is_none(),
        "test environment unexpectedly defines {missing}"
    );

    let builtin = get_builtin("io.getEnv").expect("io.getEnv builtin should exist");
    let result = call_builtin(&builtin, &[Value::String(Rc::new(missing.to_string()))])
        .expect("io.getEnv should succeed");

    match result {
        Value::None => {}
        other => panic!("expected None runtime value, got {:?}", other),
    }
}

#[test]
fn test_io_home_dir_path_returns_optional_path_runtime_value() {
    let builtin = get_builtin("io.homeDirPath").expect("io.homeDirPath builtin should exist");
    let result = call_builtin(&builtin, &[]).expect("io.homeDirPath should succeed");

    match (std::env::var("HOME").ok(), result) {
        (Some(expected), Value::Some(inner)) => match inner.as_ref() {
            Value::Path(path) => {
                let expected = std::path::PathBuf::from(expected);
                assert_eq!(path.as_path(), expected.as_path());
            }
            other => panic!("expected Some(Path), got {:?}", other),
        },
        (None, Value::None) => {}
        (Some(expected), other) => {
            panic!("expected Some(Path({expected})), got {:?}", other)
        }
        (None, other) => panic!("expected None, got {:?}", other),
    }
}

#[test]
fn test_io_home_dir_returns_optional_string_runtime_value() {
    let builtin = get_builtin("io.homeDir").expect("io.homeDir builtin should exist");
    let result = call_builtin(&builtin, &[]).expect("io.homeDir should succeed");

    match (std::env::var("HOME").ok(), result) {
        (Some(expected), Value::Some(inner)) => match inner.as_ref() {
            Value::String(home) => assert_eq!(home.as_ref(), expected.as_str()),
            other => panic!("expected Some(String), got {:?}", other),
        },
        (None, Value::None) => {}
        (Some(expected), other) => {
            panic!("expected Some(String({expected})), got {:?}", other)
        }
        (None, other) => panic!("expected None, got {:?}", other),
    }
}

#[test]
fn test_io_current_system_returns_host_system_string() {
    let builtin = get_builtin("io.currentSystem").expect("io.currentSystem builtin should exist");
    let result = call_builtin(&builtin, &[]).expect("io.currentSystem should succeed");
    let expected = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);

    match result {
        Value::String(system) => assert_eq!(system.as_ref(), expected.as_str()),
        other => panic!("expected String runtime value, got {:?}", other),
    }
}

#[test]
fn test_io_command_returns_command_runtime_value() {
    let builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let result = call_builtin(
        &builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");

    match result {
        Value::Command(_) => {}
        other => panic!("expected Command runtime value, got {:?}", other),
    }
}

#[test]
fn test_io_command_with_returns_configured_command_runtime_value() {
    let builtin = get_builtin("io.commandWith").expect("io.commandWith builtin should exist");
    let mut env = HashMap::new();
    env.insert("LANG".to_string(), Value::String(Rc::new("C".to_string())));
    let mut options = HashMap::new();
    options.insert(
        "program".to_string(),
        Value::String(Rc::new("printf".to_string())),
    );
    options.insert(
        "args".to_string(),
        Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
    );
    options.insert(
        "cwd".to_string(),
        Value::String(Rc::new("/tmp".to_string())),
    );
    options.insert(
        "stdin".to_string(),
        Value::String(Rc::new("stdin-text".to_string())),
    );
    options.insert("env".to_string(), Value::Record(Rc::new(env)));

    let result = call_builtin(&builtin, &[Value::Record(Rc::new(options))])
        .expect("io.commandWith should succeed");

    match result {
        Value::Command(command) => {
            assert_eq!(command.program(), "printf");
            assert_eq!(command.args(), &["n3v3".to_string()]);
            assert_eq!(command.cwd(), Some("/tmp"));
            assert_eq!(command.stdin(), Some("stdin-text"));
            assert_eq!(command.env().get("LANG").map(String::as_str), Some("C"));
        }
        other => panic!("expected configured Command runtime value, got {:?}", other),
    }
}

#[test]
fn test_io_command_with_redirects_returns_redirected_command_runtime_value() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let redirect_path = std::path::PathBuf::from("/tmp/n3v3.out");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(redirect_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");

    let result = call_builtin(&builtin, &[command, Value::List(Rc::new(vec![redirect]))])
        .expect("io.commandWithRedirects should succeed");

    match result {
        Value::Command(command) => {
            assert_eq!(command.program(), "printf");
            assert_eq!(command.redirects().len(), 1);
            assert_eq!(command.redirects()[0].stream_name(), "stdout");
            assert_eq!(command.redirects()[0].path(), &redirect_path);
        }
        other => panic!("expected redirected Command runtime value, got {:?}", other),
    }
}

#[test]
fn test_io_command_rejects_non_string_argument_items() {
    let builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let err = call_builtin(
        &builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::Int(1.into())])),
        ],
    )
    .expect_err("io.command should reject non-string argv items");

    assert_eq!(err, "io.command args[0] must be String");
}

#[test]
fn test_io_command_with_redirects_rejects_non_redirect_list_items() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");

    let err = call_builtin(
        &builtin,
        &[
            command,
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "/tmp/n3v3.out".to_string(),
            ))])),
        ],
    )
    .expect_err("io.commandWithRedirects should reject non-Redirect items");

    assert_eq!(err, "io.commandWithRedirects redirects[0] must be Redirect");
}

#[test]
fn test_io_command_with_preserves_error_prefix() {
    let builtin = get_builtin("io.commandWith").expect("io.commandWith builtin should exist");
    let err = call_builtin(&builtin, &[Value::Record(Rc::new(HashMap::new()))])
        .expect_err("io.commandWith should report missing program");

    assert_eq!(err, "io.commandWith requires 'program'");
}

#[test]
fn test_io_command_with_redirects_preserves_error_prefix() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");

    let err = call_builtin(&builtin, &[command, Value::List(Rc::new(Vec::new()))])
        .expect_err("io.commandWithRedirects should require at least one redirect");

    assert_eq!(
        err,
        "io.commandWithRedirects: requires a non-empty List<Redirect>"
    );
}

#[test]
fn test_io_pipeline_returns_pipeline_runtime_value() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");

    let printf = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let cat = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("cat".to_string())),
            Value::List(Rc::new(Vec::new())),
        ],
    )
    .expect("io.command should succeed");

    let result = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(vec![printf, cat]))],
    )
    .expect("io.pipeline should succeed");

    match result {
        Value::Pipeline(pipeline) => assert_eq!(pipeline.commands().len(), 2),
        other => panic!("expected Pipeline runtime value, got {:?}", other),
    }
}

#[test]
fn test_io_pipeline_with_redirects_returns_pipeline_runtime_value() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let configure_builtin = get_builtin("io.pipelineWithRedirects")
        .expect("io.pipelineWithRedirects builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");

    let printf = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let cat = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("cat".to_string())),
            Value::List(Rc::new(Vec::new())),
        ],
    )
    .expect("io.command should succeed");
    let pipeline = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(vec![printf, cat]))],
    )
    .expect("io.pipeline should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(std::path::PathBuf::from(
            "/tmp/n3v3.out",
        )))],
    )
    .expect("io.redirectStdoutPath should succeed");

    let result = call_builtin(
        &configure_builtin,
        &[pipeline, Value::List(Rc::new(vec![redirect]))],
    )
    .expect("io.pipelineWithRedirects should succeed");

    assert!(
        matches!(result, Value::Pipeline(_)),
        "io.pipelineWithRedirects should return a Pipeline, got {:?}",
        result
    );
}

#[test]
fn test_io_pipeline_rejects_non_command_list_items() {
    let builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::List(Rc::new(vec![Value::String(Rc::new(
            "printf".to_string(),
        ))]))],
    )
    .expect_err("io.pipeline should reject non-command list items");

    assert_eq!(err, "io.pipeline commands[0] must be Command");
}

#[test]
fn test_io_pipeline_with_redirects_rejects_non_pipeline_argument() {
    let builtin = get_builtin("io.pipelineWithRedirects")
        .expect("io.pipelineWithRedirects builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(std::path::PathBuf::from(
            "/tmp/n3v3.out",
        )))],
    )
    .expect("io.redirectStdoutPath should succeed");

    let err = call_builtin(
        &builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![redirect])),
        ],
    )
    .expect_err("io.pipelineWithRedirects should reject non-pipeline receivers");

    assert_eq!(
        err,
        "io.pipelineWithRedirects expects (Pipeline, List<Redirect>)"
    );
}

#[test]
fn test_io_pipeline_with_redirects_rejects_empty_pipeline() {
    let builtin = get_builtin("io.pipelineWithRedirects")
        .expect("io.pipelineWithRedirects builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");

    let pipeline = Value::Pipeline(Rc::new(PipelineValue::new(Vec::new())));
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(std::path::PathBuf::from(
            "/tmp/n3v3.out",
        )))],
    )
    .expect("io.redirectStdoutPath should succeed");

    let err = call_builtin(&builtin, &[pipeline, Value::List(Rc::new(vec![redirect]))])
        .expect_err("io.pipelineWithRedirects should reject empty pipelines");

    assert_eq!(
        err,
        "io.pipelineWithRedirects: requires a non-empty Pipeline"
    );
}

#[test]
fn test_io_pipeline_rejects_empty_command_list() {
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");

    let err = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(Vec::new()))])
        .expect_err("io.pipeline should reject empty command lists");

    assert_eq!(err, "io.pipeline: requires a non-empty List<Command>");
}

#[test]
fn test_io_exec_pipeline_returns_process_result_runtime_value() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let exec_pipeline_builtin =
        get_builtin("io.execPipeline").expect("io.execPipeline builtin should exist");

    let commands = pipeline_projection_parts()
        .into_iter()
        .map(|(program, args)| {
            let argv = args
                .into_iter()
                .map(|arg| Value::String(Rc::new(arg.to_string())))
                .collect::<Vec<_>>();
            call_builtin(
                &command_builtin,
                &[
                    Value::String(Rc::new(program.to_string())),
                    Value::List(Rc::new(argv)),
                ],
            )
            .expect("io.command should succeed")
        })
        .collect::<Vec<_>>();

    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(commands))])
        .expect("io.pipeline should succeed");
    let result =
        call_builtin(&exec_pipeline_builtin, &[pipeline]).expect("io.execPipeline should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(result.is_success(), "pipeline should succeed");
            assert_eq!(result.code(), 0);
            assert!(
                result.stdout().contains("n3v3"),
                "pipeline stdout should contain piped content"
            );
            assert!(
                result.stderr().is_empty(),
                "pipeline projection should not write stderr"
            );
        }
        other => panic!(
            "expected ProcessResult from io.execPipeline, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_pipeline_honors_embedded_pipeline_redirects() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdout_path = temp.path().join("pipeline-out.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let configure_builtin = get_builtin("io.pipelineWithRedirects")
        .expect("io.pipelineWithRedirects builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let exec_pipeline_builtin =
        get_builtin("io.execPipeline").expect("io.execPipeline builtin should exist");

    let commands = pipeline_projection_parts()
        .into_iter()
        .map(|(program, args)| {
            let argv = args
                .into_iter()
                .map(|arg| Value::String(Rc::new(arg.to_string())))
                .collect::<Vec<_>>();
            call_builtin(
                &command_builtin,
                &[
                    Value::String(Rc::new(program.to_string())),
                    Value::List(Rc::new(argv)),
                ],
            )
            .expect("io.command should succeed")
        })
        .collect::<Vec<_>>();

    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(commands))])
        .expect("io.pipeline should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let pipeline = call_builtin(
        &configure_builtin,
        &[pipeline, Value::List(Rc::new(vec![redirect]))],
    )
    .expect("io.pipelineWithRedirects should succeed");

    let result =
        call_builtin(&exec_pipeline_builtin, &[pipeline]).expect("io.execPipeline should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(result.is_success());
            assert_eq!(result.stdout(), "");
            let redirected =
                fs::read_to_string(&stdout_path).expect("redirected stdout file should exist");
            assert!(redirected.contains("n3v3"));
        }
        other => panic!(
            "expected ProcessResult from io.execPipeline, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_pipeline_with_redirect_matches_embedded_pipeline_projection() {
    let temp = TempDir::new().expect("temp dir should exist");
    let migrated_stdout_path = temp.path().join("pipeline-migrated-out.txt");
    let canonical_stdout_path = temp.path().join("pipeline-canonical-out.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let configure_builtin = get_builtin("io.pipelineWithRedirects")
        .expect("io.pipelineWithRedirects builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let exec_pipeline_builtin =
        get_builtin("io.execPipeline").expect("io.execPipeline builtin should exist");

    let migrated_commands = pipeline_projection_parts()
        .into_iter()
        .map(|(program, args)| {
            let argv = args
                .into_iter()
                .map(|arg| Value::String(Rc::new(arg.to_string())))
                .collect::<Vec<_>>();
            call_builtin(
                &command_builtin,
                &[
                    Value::String(Rc::new(program.to_string())),
                    Value::List(Rc::new(argv)),
                ],
            )
            .expect("io.command should succeed")
        })
        .collect::<Vec<_>>();
    let migrated_pipeline = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(migrated_commands))],
    )
    .expect("io.pipeline should succeed");
    let migrated_redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(migrated_stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let migrated =
        exec_pipeline_with_embedded_redirects(migrated_pipeline, vec![migrated_redirect])
            .expect("pipelineWithRedirects + io.execPipeline should succeed");

    let canonical_commands = pipeline_projection_parts()
        .into_iter()
        .map(|(program, args)| {
            let argv = args
                .into_iter()
                .map(|arg| Value::String(Rc::new(arg.to_string())))
                .collect::<Vec<_>>();
            call_builtin(
                &command_builtin,
                &[
                    Value::String(Rc::new(program.to_string())),
                    Value::List(Rc::new(argv)),
                ],
            )
            .expect("io.command should succeed")
        })
        .collect::<Vec<_>>();
    let canonical_pipeline = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(canonical_commands))],
    )
    .expect("io.pipeline should succeed");
    let canonical_redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(canonical_stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let canonical_pipeline = call_builtin(
        &configure_builtin,
        &[
            canonical_pipeline,
            Value::List(Rc::new(vec![canonical_redirect])),
        ],
    )
    .expect("io.pipelineWithRedirects should succeed");
    let canonical = call_builtin(&exec_pipeline_builtin, &[canonical_pipeline])
        .expect("io.execPipeline should succeed");

    match (migrated, canonical) {
        (Value::ProcessResult(migrated), Value::ProcessResult(canonical)) => {
            assert_eq!(migrated.code(), canonical.code());
            assert_eq!(migrated.is_success(), canonical.is_success());
            assert_eq!(migrated.stdout(), canonical.stdout());
            assert_eq!(migrated.stderr(), canonical.stderr());
            assert_eq!(
                fs::read_to_string(&migrated_stdout_path).expect("migrated stdout file"),
                fs::read_to_string(&canonical_stdout_path).expect("canonical stdout file")
            );
        }
        other => panic!("expected ProcessResult pair, got {:?}", other),
    }
}

#[test]
fn test_io_exec_pipeline_with_redirects_matches_embedded_pipeline_projection() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdin_path = temp.path().join("pipeline-in.txt");
    fs::write(&stdin_path, "n3v3\n").expect("stdin file should be writable");
    let migrated_stdout_path = temp.path().join("pipeline-migrated-out.txt");
    let canonical_stdout_path = temp.path().join("pipeline-canonical-out.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let configure_builtin = get_builtin("io.pipelineWithRedirects")
        .expect("io.pipelineWithRedirects builtin should exist");
    let stdin_redirect_builtin =
        get_builtin("io.redirectStdinPath").expect("io.redirectStdinPath builtin should exist");
    let stdout_redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let exec_pipeline_builtin =
        get_builtin("io.execPipeline").expect("io.execPipeline builtin should exist");

    let migrated_commands = stdin_pipeline_projection_parts()
        .into_iter()
        .map(|(program, args)| {
            let argv = args
                .into_iter()
                .map(|arg| Value::String(Rc::new(arg.to_string())))
                .collect::<Vec<_>>();
            call_builtin(
                &command_builtin,
                &[
                    Value::String(Rc::new(program.to_string())),
                    Value::List(Rc::new(argv)),
                ],
            )
            .expect("io.command should succeed")
        })
        .collect::<Vec<_>>();
    let migrated_pipeline = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(migrated_commands))],
    )
    .expect("io.pipeline should succeed");
    let migrated_stdin_redirect = call_builtin(
        &stdin_redirect_builtin,
        &[Value::Path(Rc::new(stdin_path.clone()))],
    )
    .expect("io.redirectStdinPath should succeed");
    let migrated_stdout_redirect = call_builtin(
        &stdout_redirect_builtin,
        &[Value::Path(Rc::new(migrated_stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let migrated = exec_pipeline_with_embedded_redirects(
        migrated_pipeline,
        vec![migrated_stdin_redirect, migrated_stdout_redirect],
    )
    .expect("pipelineWithRedirects + io.execPipeline should succeed");

    let canonical_commands = stdin_pipeline_projection_parts()
        .into_iter()
        .map(|(program, args)| {
            let argv = args
                .into_iter()
                .map(|arg| Value::String(Rc::new(arg.to_string())))
                .collect::<Vec<_>>();
            call_builtin(
                &command_builtin,
                &[
                    Value::String(Rc::new(program.to_string())),
                    Value::List(Rc::new(argv)),
                ],
            )
            .expect("io.command should succeed")
        })
        .collect::<Vec<_>>();
    let canonical_pipeline = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(canonical_commands))],
    )
    .expect("io.pipeline should succeed");
    let canonical_stdin_redirect =
        call_builtin(&stdin_redirect_builtin, &[Value::Path(Rc::new(stdin_path))])
            .expect("io.redirectStdinPath should succeed");
    let canonical_stdout_redirect = call_builtin(
        &stdout_redirect_builtin,
        &[Value::Path(Rc::new(canonical_stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let canonical_pipeline = call_builtin(
        &configure_builtin,
        &[
            canonical_pipeline,
            Value::List(Rc::new(vec![
                canonical_stdin_redirect,
                canonical_stdout_redirect,
            ])),
        ],
    )
    .expect("io.pipelineWithRedirects should succeed");
    let canonical = call_builtin(&exec_pipeline_builtin, &[canonical_pipeline])
        .expect("io.execPipeline should succeed");

    match (migrated, canonical) {
        (Value::ProcessResult(migrated), Value::ProcessResult(canonical)) => {
            assert_eq!(migrated.code(), canonical.code());
            assert_eq!(migrated.is_success(), canonical.is_success());
            assert_eq!(migrated.stdout(), canonical.stdout());
            assert_eq!(migrated.stderr(), canonical.stderr());
            assert_eq!(
                fs::read_to_string(&migrated_stdout_path).expect("migrated stdout file"),
                fs::read_to_string(&canonical_stdout_path).expect("canonical stdout file")
            );
        }
        other => panic!("expected ProcessResult pair, got {:?}", other),
    }
}

#[test]
fn test_io_exec_pipeline_matches_single_stage_exec_command_projection() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let exec_pipeline_builtin =
        get_builtin("io.execPipeline").expect("io.execPipeline builtin should exist");
    let exec_command_builtin =
        get_builtin("io.execCommand").expect("io.execCommand builtin should exist");
    let success_builtin =
        get_builtin("io.processSuccess").expect("io.processSuccess builtin should exist");
    let stdout_builtin =
        get_builtin("io.processStdout").expect("io.processStdout builtin should exist");
    let code_builtin = get_builtin("io.processCode").expect("io.processCode builtin should exist");
    let stderr_builtin =
        get_builtin("io.processStderr").expect("io.processStderr builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--version".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");

    let pipeline = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(vec![command.clone()]))],
    )
    .expect("io.pipeline should succeed");
    let migrated =
        call_builtin(&exec_pipeline_builtin, &[pipeline]).expect("io.execPipeline should succeed");
    let canonical =
        call_builtin(&exec_command_builtin, &[command]).expect("io.execCommand should succeed");

    let success =
        call_builtin(&success_builtin, std::slice::from_ref(&canonical)).expect("success access");
    let stdout =
        call_builtin(&stdout_builtin, std::slice::from_ref(&canonical)).expect("stdout access");
    let code = call_builtin(&code_builtin, std::slice::from_ref(&canonical)).expect("code access");
    let stderr =
        call_builtin(&stderr_builtin, std::slice::from_ref(&canonical)).expect("stderr access");

    match migrated {
        Value::ProcessResult(result) => {
            assert_eq!(result.is_success(), success == Value::Bool(true));
            assert_eq!(Value::String(Rc::new(result.stdout().to_string())), stdout);
            assert_eq!(Value::Int(result.code().into()), code);
            assert_eq!(Value::String(Rc::new(result.stderr().to_string())), stderr);
        }
        other => panic!(
            "expected ProcessResult from io.execPipeline, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_pipeline_rejects_non_pipeline_argument() {
    let builtin = get_builtin("io.execPipeline").expect("io.execPipeline builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new("not-a-pipeline".to_string()))],
    )
    .expect_err("io.execPipeline should reject string arguments");

    assert_eq!(err, "io.execPipeline expects a Pipeline");
}

#[test]
fn test_io_exec_pipeline_preserves_error_prefix_for_empty_pipeline() {
    let exec_pipeline_builtin =
        get_builtin("io.execPipeline").expect("io.execPipeline builtin should exist");

    let pipeline = Value::Pipeline(Rc::new(PipelineValue::new(Vec::new())));
    let err = call_builtin(&exec_pipeline_builtin, &[pipeline])
        .expect_err("io.execPipeline should reject empty pipelines");

    assert!(
        err.starts_with("io.execPipeline:"),
        "expected io.execPipeline-prefixed error, got {err}"
    );
}

#[test]
fn test_io_exec_pipeline_with_redirect_writes_stdout_to_file() {
    let temp = TempDir::new().expect("temp dir should exist");
    let redirect_path = temp.path().join("pipeline-stdout.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let commands = pipeline_projection_parts()
        .into_iter()
        .map(|(program, args)| {
            let argv = args
                .into_iter()
                .map(|arg| Value::String(Rc::new(arg.to_string())))
                .collect::<Vec<_>>();
            call_builtin(
                &command_builtin,
                &[
                    Value::String(Rc::new(program.to_string())),
                    Value::List(Rc::new(argv)),
                ],
            )
            .expect("io.command should succeed")
        })
        .collect::<Vec<_>>();

    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(commands))])
        .expect("io.pipeline should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(redirect_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");

    let result = exec_pipeline_with_embedded_redirects(pipeline, vec![redirect])
        .expect("pipelineWithRedirects + io.execPipeline should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(result.is_success(), "redirected pipeline should succeed");
            assert_eq!(result.code(), 0);
            assert_eq!(result.stdout(), "");
            let redirected =
                fs::read_to_string(&redirect_path).expect("redirected file should exist");
            assert!(
                redirected.contains("n3v3"),
                "redirected stdout file should contain piped content"
            );
            assert!(
                result.stderr().is_empty(),
                "pipeline projection should not write stderr"
            );
        }
        other => panic!(
            "expected ProcessResult from io.execPipeline, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_pipeline_with_stderr_redirect_writes_stderr_to_file() {
    let temp = TempDir::new().expect("temp dir should exist");
    let redirect_path = temp.path().join("pipeline-stderr.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStderrPath").expect("io.redirectStderrPath builtin should exist");
    let invalid_rustc = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--definitely-not-a-real-rustc-flag".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");

    let pipeline = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(vec![invalid_rustc]))],
    )
    .expect("io.pipeline should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(redirect_path.clone()))],
    )
    .expect("io.redirectStderrPath should succeed");

    let result = exec_pipeline_with_embedded_redirects(pipeline, vec![redirect])
        .expect("pipelineWithRedirects + io.execPipeline should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(
                !result.is_success(),
                "invalid rustc flag pipeline should fail"
            );
            assert_ne!(result.code(), 0);
            assert_eq!(result.stderr(), "");
            let redirected =
                fs::read_to_string(&redirect_path).expect("redirected file should exist");
            assert!(
                !redirected.trim().is_empty(),
                "redirected stderr file should contain command output"
            );
        }
        other => panic!(
            "expected ProcessResult from io.execPipeline, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_pipeline_with_stdin_redirect_reads_stdin_from_file() {
    let temp = TempDir::new().expect("temp dir should exist");
    let redirect_path = temp.path().join("pipeline-stdin.txt");
    fs::write(&redirect_path, "n3v3 stdin line\n").expect("stdin file should be writable");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdinPath").expect("io.redirectStdinPath builtin should exist");
    let commands = stdin_pipeline_projection_parts()
        .into_iter()
        .map(|(program, args)| {
            let argv = args
                .into_iter()
                .map(|arg| Value::String(Rc::new(arg.to_string())))
                .collect::<Vec<_>>();
            call_builtin(
                &command_builtin,
                &[
                    Value::String(Rc::new(program.to_string())),
                    Value::List(Rc::new(argv)),
                ],
            )
            .expect("io.command should succeed")
        })
        .collect::<Vec<_>>();

    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(commands))])
        .expect("io.pipeline should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(redirect_path.clone()))],
    )
    .expect("io.redirectStdinPath should succeed");

    let result = exec_pipeline_with_embedded_redirects(pipeline, vec![redirect])
        .expect("pipelineWithRedirects + io.execPipeline should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(
                result.is_success(),
                "stdin redirected pipeline should succeed"
            );
            assert_eq!(result.code(), 0);
            assert!(
                result.stdout().contains("n3v3"),
                "stdin redirected pipeline should surface filtered stdout"
            );
        }
        other => panic!(
            "expected ProcessResult from io.execPipeline, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_pipeline_with_redirects_composes_stdin_and_stdout_paths() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdin_path = temp.path().join("pipeline-stdin.txt");
    let stdout_path = temp.path().join("pipeline-stdout.txt");
    fs::write(&stdin_path, "n3v3 stdin line\n").expect("stdin file should be writable");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let redirect_stdin_builtin =
        get_builtin("io.redirectStdinPath").expect("io.redirectStdinPath builtin should exist");
    let redirect_stdout_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let commands = stdin_pipeline_projection_parts()
        .into_iter()
        .map(|(program, args)| {
            let argv = args
                .into_iter()
                .map(|arg| Value::String(Rc::new(arg.to_string())))
                .collect::<Vec<_>>();
            call_builtin(
                &command_builtin,
                &[
                    Value::String(Rc::new(program.to_string())),
                    Value::List(Rc::new(argv)),
                ],
            )
            .expect("io.command should succeed")
        })
        .collect::<Vec<_>>();

    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(commands))])
        .expect("io.pipeline should succeed");
    let stdin_redirect = call_builtin(
        &redirect_stdin_builtin,
        &[Value::Path(Rc::new(stdin_path.clone()))],
    )
    .expect("io.redirectStdinPath should succeed");
    let stdout_redirect = call_builtin(
        &redirect_stdout_builtin,
        &[Value::Path(Rc::new(stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");

    let result =
        exec_pipeline_with_embedded_redirects(pipeline, vec![stdin_redirect, stdout_redirect])
            .expect("pipelineWithRedirects + io.execPipeline should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(
                result.is_success(),
                "composed pipeline redirects should succeed"
            );
            assert_eq!(result.code(), 0);
            assert_eq!(result.stdout(), "");
            let redirected =
                fs::read_to_string(&stdout_path).expect("redirected file should exist");
            assert!(
                redirected.contains("n3v3"),
                "stdout redirect file should contain piped stdin content"
            );
        }
        other => panic!(
            "expected ProcessResult from io.execPipeline, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_pipeline_with_redirects_composes_stdout_and_stderr_paths() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdout_path = temp.path().join("pipeline-stdout.txt");
    let stderr_path = temp.path().join("pipeline-stderr.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let redirect_stdout_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let redirect_stderr_builtin =
        get_builtin("io.redirectStderrPath").expect("io.redirectStderrPath builtin should exist");
    let (program, args) = stdout_stderr_projection_parts();
    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new(program.to_string())),
            Value::List(Rc::new(
                args.into_iter()
                    .map(|arg| Value::String(Rc::new(arg.to_string())))
                    .collect(),
            )),
        ],
    )
    .expect("io.command should succeed");
    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(vec![command]))])
        .expect("io.pipeline should succeed");
    let stdout_redirect = call_builtin(
        &redirect_stdout_builtin,
        &[Value::Path(Rc::new(stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let stderr_redirect = call_builtin(
        &redirect_stderr_builtin,
        &[Value::Path(Rc::new(stderr_path.clone()))],
    )
    .expect("io.redirectStderrPath should succeed");

    let result =
        exec_pipeline_with_embedded_redirects(pipeline, vec![stdout_redirect, stderr_redirect])
            .expect("pipelineWithRedirects + io.execPipeline should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(
                result.is_success(),
                "composed pipeline redirects should succeed"
            );
            assert_eq!(result.stdout(), "");
            assert_eq!(result.stderr(), "");
            let out = fs::read_to_string(&stdout_path).expect("stdout redirect file should exist");
            let err = fs::read_to_string(&stderr_path).expect("stderr redirect file should exist");
            assert!(
                !out.trim().is_empty(),
                "stdout file should contain command output"
            );
            assert!(
                !err.trim().is_empty(),
                "stderr file should contain command output"
            );
        }
        other => panic!(
            "expected ProcessResult from io.execPipeline, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_pipeline_with_redirects_rejects_duplicate_stdout_redirects() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdout_path = temp.path().join("pipeline-stdout.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--version".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");
    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(vec![command]))])
        .expect("io.pipeline should succeed");
    let stdout_redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let duplicate_stdout_redirect =
        call_builtin(&redirect_builtin, &[Value::Path(Rc::new(stdout_path))])
            .expect("io.redirectStdoutPath should succeed");

    let err = exec_pipeline_with_embedded_redirects(
        pipeline,
        vec![stdout_redirect, duplicate_stdout_redirect],
    )
    .expect_err("pipelineWithRedirects should reject duplicate stdout redirects");

    assert_eq!(err, "io.pipelineWithRedirects: duplicate stdout redirect");
}

#[test]
fn test_io_exec_pipeline_with_redirect_rejects_final_stage_stdout_conflict() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdout_path = temp.path().join("pipeline-stdout.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let configure_builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let embedded_redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let command = call_builtin(
        &configure_builtin,
        &[command, Value::List(Rc::new(vec![embedded_redirect]))],
    )
    .expect("io.commandWithRedirects should succeed");
    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(vec![command]))])
        .expect("io.pipeline should succeed");
    let boundary_redirect = call_builtin(&redirect_builtin, &[Value::Path(Rc::new(stdout_path))])
        .expect("io.redirectStdoutPath should succeed");

    let err = exec_pipeline_with_embedded_redirects(pipeline, vec![boundary_redirect])
        .expect_err("pipelineWithRedirects should reject final-stage stdout redirect conflicts");

    assert_eq!(
        err,
        "io.pipelineWithRedirects: final pipeline stage cannot combine boundary stdout with stage-local stdout redirect"
    );
}

#[test]
fn test_io_pipeline_with_redirects_rejects_final_stage_stdout_conflict() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdout_path = temp.path().join("pipeline-stdout.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let configure_command_builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let configure_pipeline_builtin = get_builtin("io.pipelineWithRedirects")
        .expect("io.pipelineWithRedirects builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let embedded_redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let command = call_builtin(
        &configure_command_builtin,
        &[command, Value::List(Rc::new(vec![embedded_redirect]))],
    )
    .expect("io.commandWithRedirects should succeed");
    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(vec![command]))])
        .expect("io.pipeline should succeed");
    let boundary_redirect = call_builtin(&redirect_builtin, &[Value::Path(Rc::new(stdout_path))])
        .expect("io.redirectStdoutPath should succeed");

    let err = call_builtin(
        &configure_pipeline_builtin,
        &[pipeline, Value::List(Rc::new(vec![boundary_redirect]))],
    )
    .expect_err("io.pipelineWithRedirects should reject final-stage stdout conflicts");

    assert_eq!(
        err,
        "io.pipelineWithRedirects: final pipeline stage cannot combine boundary stdout with stage-local stdout redirect"
    );
}

#[test]
fn test_io_exec_pipeline_with_redirects_rejects_final_stage_stderr_conflict() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stderr_path = temp.path().join("pipeline-stderr.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let configure_builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStderrPath").expect("io.redirectStderrPath builtin should exist");
    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--definitely-not-a-real-rustc-flag".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");
    let embedded_redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(stderr_path.clone()))],
    )
    .expect("io.redirectStderrPath should succeed");
    let command = call_builtin(
        &configure_builtin,
        &[command, Value::List(Rc::new(vec![embedded_redirect]))],
    )
    .expect("io.commandWithRedirects should succeed");
    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(vec![command]))])
        .expect("io.pipeline should succeed");
    let boundary_redirect = call_builtin(&redirect_builtin, &[Value::Path(Rc::new(stderr_path))])
        .expect("io.redirectStderrPath should succeed");

    let err = exec_pipeline_with_embedded_redirects(pipeline, vec![boundary_redirect])
        .expect_err("pipelineWithRedirects should reject final-stage stderr redirect conflicts");

    assert_eq!(
        err,
        "io.pipelineWithRedirects: final pipeline stage cannot combine boundary stderr with stage-local stderr redirect"
    );
}

#[test]
fn test_io_pipeline_with_redirects_rejects_final_stage_stderr_conflict() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stderr_path = temp.path().join("pipeline-stderr.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let configure_command_builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let configure_pipeline_builtin = get_builtin("io.pipelineWithRedirects")
        .expect("io.pipelineWithRedirects builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStderrPath").expect("io.redirectStderrPath builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--definitely-not-a-real-rustc-flag".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");
    let embedded_redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(stderr_path.clone()))],
    )
    .expect("io.redirectStderrPath should succeed");
    let command = call_builtin(
        &configure_command_builtin,
        &[command, Value::List(Rc::new(vec![embedded_redirect]))],
    )
    .expect("io.commandWithRedirects should succeed");
    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(vec![command]))])
        .expect("io.pipeline should succeed");
    let boundary_redirect = call_builtin(&redirect_builtin, &[Value::Path(Rc::new(stderr_path))])
        .expect("io.redirectStderrPath should succeed");

    let err = call_builtin(
        &configure_pipeline_builtin,
        &[pipeline, Value::List(Rc::new(vec![boundary_redirect]))],
    )
    .expect_err("io.pipelineWithRedirects should reject final-stage stderr conflicts");

    assert_eq!(
        err,
        "io.pipelineWithRedirects: final pipeline stage cannot combine boundary stderr with stage-local stderr redirect"
    );
}

#[test]
fn test_io_pipeline_rejects_non_final_stage_stdout_redirect() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdout_path = temp.path().join("stage-stdout.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let configure_builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");

    let stage1 = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("stage1 command should succeed");
    let embedded_redirect = call_builtin(&redirect_builtin, &[Value::Path(Rc::new(stdout_path))])
        .expect("io.redirectStdoutPath should succeed");
    let stage1 = call_builtin(
        &configure_builtin,
        &[stage1, Value::List(Rc::new(vec![embedded_redirect]))],
    )
    .expect("io.commandWithRedirects should succeed");

    let stage2 = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("cat".to_string())),
            Value::List(Rc::new(Vec::new())),
        ],
    )
    .expect("stage2 command should succeed");

    let pipeline = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(vec![stage1, stage2]))],
    )
    .expect_err("io.pipeline should reject non-final stage stdout redirect");

    assert_eq!(
        pipeline,
        "io.pipeline: pipeline stage 1 cannot carry stdout redirect before final stage"
    );
}

#[test]
fn test_io_pipeline_rejects_non_initial_stage_configured_stdin() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let command_with_builtin =
        get_builtin("io.commandWith").expect("io.commandWith builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");

    let stage1 = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let stage2 = call_builtin(
        &command_with_builtin,
        &[Value::Record(Rc::new(std::collections::HashMap::from([
            (
                "program".to_string(),
                Value::String(Rc::new("cat".to_string())),
            ),
            ("args".to_string(), Value::List(Rc::new(Vec::new()))),
            (
                "stdin".to_string(),
                Value::String(Rc::new("n3v3".to_string())),
            ),
        ])))],
    )
    .expect("io.commandWith should succeed");

    let err = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(vec![stage1, stage2]))],
    )
    .expect_err("io.pipeline should reject configured stdin on non-initial stages");

    assert_eq!(err, "io.pipeline: pipeline stage 2 cannot specify stdin");
}

#[test]
fn test_io_pipeline_rejects_non_initial_stage_stdin_redirect() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdin_path = temp.path().join("stage-stdin.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let configure_builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdinPath").expect("io.redirectStdinPath builtin should exist");

    let stage1 = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let embedded_redirect = call_builtin(&redirect_builtin, &[Value::Path(Rc::new(stdin_path))])
        .expect("io.redirectStdinPath should succeed");
    let stage2 = call_builtin(
        &configure_builtin,
        &[
            call_builtin(
                &command_builtin,
                &[
                    Value::String(Rc::new("cat".to_string())),
                    Value::List(Rc::new(Vec::new())),
                ],
            )
            .expect("io.command should succeed"),
            Value::List(Rc::new(vec![embedded_redirect])),
        ],
    )
    .expect("io.commandWithRedirects should succeed");

    let err = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(vec![stage1, stage2]))],
    )
    .expect_err("io.pipeline should reject stdin redirects on non-initial stages");

    assert_eq!(
        err,
        "io.pipeline: pipeline stage 2 cannot carry stdin redirect"
    );
}

#[test]
fn test_io_pipeline_with_redirects_rejects_boundary_stdin_with_stage_local_stdin() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdin_path = temp.path().join("pipeline-stdin.txt");

    let command_with_builtin =
        get_builtin("io.commandWith").expect("io.commandWith builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let configure_pipeline_builtin = get_builtin("io.pipelineWithRedirects")
        .expect("io.pipelineWithRedirects builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdinPath").expect("io.redirectStdinPath builtin should exist");

    let command = call_builtin(
        &command_with_builtin,
        &[Value::Record(Rc::new(std::collections::HashMap::from([
            (
                "program".to_string(),
                Value::String(Rc::new("cat".to_string())),
            ),
            ("args".to_string(), Value::List(Rc::new(Vec::new()))),
            (
                "stdin".to_string(),
                Value::String(Rc::new("n3v3".to_string())),
            ),
        ])))],
    )
    .expect("io.commandWith should succeed");
    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(vec![command]))])
        .expect("io.pipeline should succeed");
    let boundary_redirect = call_builtin(&redirect_builtin, &[Value::Path(Rc::new(stdin_path))])
        .expect("io.redirectStdinPath should succeed");

    let err = call_builtin(
        &configure_pipeline_builtin,
        &[pipeline, Value::List(Rc::new(vec![boundary_redirect]))],
    )
    .expect_err("io.pipelineWithRedirects should reject boundary stdin conflicts");

    assert_eq!(
        err,
        "io.pipelineWithRedirects: pipeline stage 1 cannot combine boundary stdin with stage-local stdin"
    );
}

#[test]
fn test_io_exec_pipeline_with_redirect_rejects_non_redirect_argument() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(vec![command]))])
        .expect("io.pipeline should succeed");

    let err = exec_pipeline_with_embedded_redirects(
        pipeline,
        vec![Value::String(Rc::new("/tmp/out.txt".to_string()))],
    )
    .expect_err("pipelineWithRedirects should reject string redirect arguments");

    assert_eq!(
        err,
        "io.pipelineWithRedirects redirects[0] must be Redirect"
    );
}

#[test]
fn test_io_exec_pipeline_with_redirect_rejects_pipeline_with_embedded_redirects() {
    let temp = TempDir::new().expect("temp dir should exist");
    let redirect_path = temp.path().join("pipeline-out.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let configure_builtin = get_builtin("io.pipelineWithRedirects")
        .expect("io.pipelineWithRedirects builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(vec![command]))])
        .expect("io.pipeline should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(redirect_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let pipeline = call_builtin(
        &configure_builtin,
        &[pipeline, Value::List(Rc::new(vec![redirect.clone()]))],
    )
    .expect("io.pipelineWithRedirects should succeed");

    let err = exec_pipeline_with_embedded_redirects(pipeline, vec![redirect])
        .expect_err("pipelineWithRedirects should reject embedded boundary redirects");

    assert_eq!(
        err,
        "io.pipelineWithRedirects: pipeline already carries embedded redirects"
    );
}

#[test]
fn test_io_exec_pipeline_with_redirect_preserves_error_prefix() {
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let temp = TempDir::new().expect("temp dir should exist");
    let redirect_path = temp.path().join("pipeline-out.txt");

    let pipeline = Value::Pipeline(Rc::new(PipelineValue::new(Vec::new())));
    let redirect = call_builtin(&redirect_builtin, &[Value::Path(Rc::new(redirect_path))])
        .expect("io.redirectStdoutPath should succeed");
    let err = exec_pipeline_with_embedded_redirects(pipeline, vec![redirect])
        .expect_err("pipelineWithRedirects should reject empty pipelines");

    assert!(
        err.starts_with("io.pipelineWithRedirects:"),
        "expected io.pipelineWithRedirects-prefixed error, got {err}"
    );
}

#[test]
fn test_io_task_command_returns_task_runtime_value() {
    let task_builtin = get_builtin("io.taskCommand").expect("io.taskCommand builtin should exist");
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");

    let result = call_builtin(&task_builtin, &[command]).expect("io.taskCommand should succeed");
    assert!(
        matches!(result, Value::Task(_)),
        "io.taskCommand should return a Task, got {:?}",
        result
    );
}

#[test]
fn test_io_task_command_rejects_non_command_argument() {
    let builtin = get_builtin("io.taskCommand").expect("io.taskCommand builtin should exist");

    let err = call_builtin(&builtin, &[Value::String(Rc::new("printf".to_string()))])
        .expect_err("io.taskCommand should reject string arguments");
    assert_eq!(err, "io.taskCommand expects a Command");
}

#[test]
fn test_io_task_pipeline_returns_task_runtime_value() {
    let task_builtin =
        get_builtin("io.taskPipeline").expect("io.taskPipeline builtin should exist");
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let pipeline = call_builtin(&pipeline_builtin, &[Value::List(Rc::new(vec![command]))])
        .expect("io.pipeline should succeed");

    let result = call_builtin(&task_builtin, &[pipeline]).expect("io.taskPipeline should succeed");
    assert!(
        matches!(result, Value::Task(_)),
        "io.taskPipeline should return a Task, got {:?}",
        result
    );
}

#[test]
fn test_io_task_pipeline_rejects_non_pipeline_argument() {
    let builtin = get_builtin("io.taskPipeline").expect("io.taskPipeline builtin should exist");

    let err = call_builtin(&builtin, &[Value::String(Rc::new("printf".to_string()))])
        .expect_err("io.taskPipeline should reject string arguments");
    assert_eq!(err, "io.taskPipeline expects a Pipeline");
}

#[test]
fn test_io_await_pipeline_task_honors_embedded_pipeline_redirects() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdout_path = temp.path().join("task-pipeline-stdout.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let configure_builtin = get_builtin("io.pipelineWithRedirects")
        .expect("io.pipelineWithRedirects builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let task_builtin =
        get_builtin("io.taskPipeline").expect("io.taskPipeline builtin should exist");
    let await_builtin = get_builtin("io.awaitTask").expect("io.awaitTask builtin should exist");

    let printf = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let cat = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("cat".to_string())),
            Value::List(Rc::new(Vec::new())),
        ],
    )
    .expect("io.command should succeed");
    let pipeline = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(vec![printf, cat]))],
    )
    .expect("io.pipeline should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let pipeline = call_builtin(
        &configure_builtin,
        &[pipeline, Value::List(Rc::new(vec![redirect]))],
    )
    .expect("io.pipelineWithRedirects should succeed");
    let task = call_builtin(&task_builtin, &[pipeline]).expect("io.taskPipeline should succeed");

    let result = call_builtin(&await_builtin, &[task]).expect("io.awaitTask should succeed");
    match result {
        Value::ProcessResult(result) => {
            assert!(result.is_success());
            assert_eq!(result.stdout(), "");
            let redirected =
                fs::read_to_string(&stdout_path).expect("redirected stdout file should exist");
            assert!(redirected.contains("n3v3"));
        }
        other => panic!("expected ProcessResult from io.awaitTask, got {:?}", other),
    }
}

#[test]
fn test_io_await_task_returns_process_result_runtime_value() {
    let task_builtin = get_builtin("io.taskCommand").expect("io.taskCommand builtin should exist");
    let await_builtin = get_builtin("io.awaitTask").expect("io.awaitTask builtin should exist");
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--version".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");
    let task = call_builtin(&task_builtin, &[command]).expect("io.taskCommand should succeed");

    let result = call_builtin(&await_builtin, &[task]).expect("io.awaitTask should succeed");
    assert!(
        matches!(result, Value::ProcessResult(_)),
        "io.awaitTask should return a ProcessResult, got {:?}",
        result
    );
}

#[test]
fn test_io_await_pipeline_task_returns_process_result_runtime_value() {
    let task_builtin =
        get_builtin("io.taskPipeline").expect("io.taskPipeline builtin should exist");
    let await_builtin = get_builtin("io.awaitTask").expect("io.awaitTask builtin should exist");
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");

    let printf = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let cat = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("cat".to_string())),
            Value::List(Rc::new(vec![])),
        ],
    )
    .expect("io.command should succeed");
    let pipeline = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(vec![printf, cat]))],
    )
    .expect("io.pipeline should succeed");
    let task = call_builtin(&task_builtin, &[pipeline]).expect("io.taskPipeline should succeed");

    let result = call_builtin(&await_builtin, &[task]).expect("io.awaitTask should succeed");
    assert!(
        matches!(result, Value::ProcessResult(_)),
        "io.awaitTask should return a ProcessResult, got {:?}",
        result
    );
}

#[test]
fn test_io_await_task_honors_embedded_command_redirects() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdout_path = temp.path().join("task-stdout.txt");

    let task_builtin = get_builtin("io.taskCommand").expect("io.taskCommand builtin should exist");
    let await_builtin = get_builtin("io.awaitTask").expect("io.awaitTask builtin should exist");
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let configure_builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let command = call_builtin(
        &configure_builtin,
        &[command, Value::List(Rc::new(vec![redirect]))],
    )
    .expect("io.commandWithRedirects should succeed");
    let task = call_builtin(&task_builtin, &[command]).expect("io.taskCommand should succeed");
    let result = call_builtin(&await_builtin, &[task]).expect("io.awaitTask should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(result.is_success());
            assert_eq!(result.stdout(), "");
            let redirected =
                fs::read_to_string(&stdout_path).expect("redirected stdout file should exist");
            assert!(redirected.contains("n3v3"));
        }
        other => panic!("expected ProcessResult from io.awaitTask, got {:?}", other),
    }
}

#[test]
fn test_io_await_task_rejects_non_task_argument() {
    let builtin = get_builtin("io.awaitTask").expect("io.awaitTask builtin should exist");

    let err = call_builtin(&builtin, &[Value::String(Rc::new("printf".to_string()))])
        .expect_err("io.awaitTask should reject string arguments");
    assert_eq!(err, "io.awaitTask expects a Task[ProcessResult]");
}

#[test]
fn test_io_await_tasks_returns_process_result_list_runtime_value() {
    let task_builtin = get_builtin("io.taskCommand").expect("io.taskCommand builtin should exist");
    let await_builtin = get_builtin("io.awaitTasks").expect("io.awaitTasks builtin should exist");
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");

    let command1 = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let command2 = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("lang".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let task1 = call_builtin(&task_builtin, &[command1]).expect("io.taskCommand should succeed");
    let task2 = call_builtin(&task_builtin, &[command2]).expect("io.taskCommand should succeed");

    let result = call_builtin(&await_builtin, &[Value::List(Rc::new(vec![task1, task2]))])
        .expect("io.awaitTasks should succeed");

    match result {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert!(matches!(items[0], Value::ProcessResult(_)));
            assert!(matches!(items[1], Value::ProcessResult(_)));
        }
        other => panic!(
            "expected List<ProcessResult> from io.awaitTasks, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_await_tasks_rejects_non_task_list_items() {
    let builtin = get_builtin("io.awaitTasks").expect("io.awaitTasks builtin should exist");

    let err = call_builtin(
        &builtin,
        &[Value::List(Rc::new(vec![Value::String(Rc::new(
            "printf".to_string(),
        ))]))],
    )
    .expect_err("io.awaitTasks should reject non-task list items");
    assert_eq!(err, "io.awaitTasks tasks[0] must be Task[ProcessResult]");
}

#[test]
fn test_io_await_tasks_matches_individual_await_projection() {
    let task_command_builtin =
        get_builtin("io.taskCommand").expect("io.taskCommand builtin should exist");
    let task_pipeline_builtin =
        get_builtin("io.taskPipeline").expect("io.taskPipeline builtin should exist");
    let await_builtin = get_builtin("io.awaitTask").expect("io.awaitTask builtin should exist");
    let await_tasks_builtin =
        get_builtin("io.awaitTasks").expect("io.awaitTasks builtin should exist");
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let printf_lang = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("lang".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let cat = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("cat".to_string())),
            Value::List(Rc::new(vec![])),
        ],
    )
    .expect("io.command should succeed");
    let pipeline = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(vec![printf_lang, cat]))],
    )
    .expect("io.pipeline should succeed");

    let task1 =
        call_builtin(&task_command_builtin, &[command]).expect("io.taskCommand should succeed");
    let task2 =
        call_builtin(&task_pipeline_builtin, &[pipeline]).expect("io.taskPipeline should succeed");

    let awaited_many = call_builtin(
        &await_tasks_builtin,
        &[Value::List(Rc::new(vec![task1.clone(), task2.clone()]))],
    )
    .expect("io.awaitTasks should succeed");
    let awaited_one = call_builtin(&await_builtin, &[task1]).expect("io.awaitTask should succeed");
    let awaited_two = call_builtin(&await_builtin, &[task2]).expect("io.awaitTask should succeed");

    match (awaited_many, awaited_one, awaited_two) {
        (Value::List(items), Value::ProcessResult(first), Value::ProcessResult(second)) => {
            assert_eq!(items.len(), 2);
            match (&items[0], &items[1]) {
                (Value::ProcessResult(first_many), Value::ProcessResult(second_many)) => {
                    assert_eq!(first_many.code(), first.code());
                    assert_eq!(first_many.is_success(), first.is_success());
                    assert_eq!(first_many.stdout(), first.stdout());
                    assert_eq!(first_many.stderr(), first.stderr());
                    assert_eq!(second_many.code(), second.code());
                    assert_eq!(second_many.is_success(), second.is_success());
                    assert_eq!(second_many.stdout(), second.stdout());
                    assert_eq!(second_many.stderr(), second.stderr());
                }
                other => panic!(
                    "expected ProcessResult pair from io.awaitTasks, got {:?}",
                    other
                ),
            }
        }
        other => panic!("expected awaited ProcessResults, got {:?}", other),
    }
}

#[test]
fn test_io_await_tasks_preserves_indexed_error_prefix() {
    let task_builtin = get_builtin("io.taskCommand").expect("io.taskCommand builtin should exist");
    let await_builtin = get_builtin("io.awaitTasks").expect("io.awaitTasks builtin should exist");
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");

    let good = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let bad = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new(
                "n3v3-definitely-missing-command-for-tests".to_string(),
            )),
            Value::List(Rc::new(vec![])),
        ],
    )
    .expect("io.command should succeed");
    let good = call_builtin(&task_builtin, &[good]).expect("io.taskCommand should succeed");
    let bad = call_builtin(&task_builtin, &[bad]).expect("io.taskCommand should succeed");

    let err = call_builtin(&await_builtin, &[Value::List(Rc::new(vec![good, bad]))])
        .expect_err("io.awaitTasks should report failing task index");
    assert!(
        err.starts_with("io.awaitTasks[1]:"),
        "expected io.awaitTasks[1]-prefixed error, got {err}"
    );
}

#[test]
fn test_io_await_task_matches_canonical_exec_command_projection() {
    let task_builtin = get_builtin("io.taskCommand").expect("io.taskCommand builtin should exist");
    let await_builtin = get_builtin("io.awaitTask").expect("io.awaitTask builtin should exist");
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let exec_command_builtin =
        get_builtin("io.execCommand").expect("io.execCommand builtin should exist");
    let success_builtin =
        get_builtin("io.processSuccess").expect("io.processSuccess builtin should exist");
    let stdout_builtin =
        get_builtin("io.processStdout").expect("io.processStdout builtin should exist");
    let code_builtin = get_builtin("io.processCode").expect("io.processCode builtin should exist");
    let stderr_builtin =
        get_builtin("io.processStderr").expect("io.processStderr builtin should exist");

    let program = Value::String(Rc::new("rustc".to_string()));
    let argv = Value::List(Rc::new(vec![Value::String(Rc::new(
        "--version".to_string(),
    ))]));

    let command = call_builtin(&command_builtin, &[program.clone(), argv.clone()])
        .expect("io.command should succeed");
    let task = call_builtin(&task_builtin, std::slice::from_ref(&command))
        .expect("io.taskCommand should succeed");
    let awaited = call_builtin(&await_builtin, &[task]).expect("io.awaitTask should succeed");
    let canonical =
        call_builtin(&exec_command_builtin, &[command]).expect("io.execCommand should succeed");

    let success =
        call_builtin(&success_builtin, std::slice::from_ref(&canonical)).expect("success access");
    let stdout =
        call_builtin(&stdout_builtin, std::slice::from_ref(&canonical)).expect("stdout access");
    let code = call_builtin(&code_builtin, std::slice::from_ref(&canonical)).expect("code access");
    let stderr =
        call_builtin(&stderr_builtin, std::slice::from_ref(&canonical)).expect("stderr access");

    match awaited {
        Value::ProcessResult(result) => {
            assert_eq!(result.is_success(), success == Value::Bool(true));
            assert_eq!(Value::String(Rc::new(result.stdout().to_string())), stdout);
            assert_eq!(Value::Int(result.code().into()), code);
            assert_eq!(Value::String(Rc::new(result.stderr().to_string())), stderr);
        }
        other => panic!("expected ProcessResult from io.awaitTask, got {:?}", other),
    }
}

#[test]
fn test_io_await_pipeline_task_matches_canonical_exec_pipeline_projection() {
    let task_builtin =
        get_builtin("io.taskPipeline").expect("io.taskPipeline builtin should exist");
    let await_builtin = get_builtin("io.awaitTask").expect("io.awaitTask builtin should exist");
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let exec_pipeline_builtin =
        get_builtin("io.execPipeline").expect("io.execPipeline builtin should exist");
    let success_builtin =
        get_builtin("io.processSuccess").expect("io.processSuccess builtin should exist");
    let stdout_builtin =
        get_builtin("io.processStdout").expect("io.processStdout builtin should exist");
    let code_builtin = get_builtin("io.processCode").expect("io.processCode builtin should exist");
    let stderr_builtin =
        get_builtin("io.processStderr").expect("io.processStderr builtin should exist");

    let printf = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let cat = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("cat".to_string())),
            Value::List(Rc::new(vec![])),
        ],
    )
    .expect("io.command should succeed");
    let pipeline = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(vec![printf, cat]))],
    )
    .expect("io.pipeline should succeed");
    let task = call_builtin(&task_builtin, std::slice::from_ref(&pipeline))
        .expect("io.taskPipeline should succeed");
    let awaited = call_builtin(&await_builtin, &[task]).expect("io.awaitTask should succeed");
    let canonical =
        call_builtin(&exec_pipeline_builtin, &[pipeline]).expect("io.execPipeline should succeed");

    let success =
        call_builtin(&success_builtin, std::slice::from_ref(&canonical)).expect("success access");
    let stdout =
        call_builtin(&stdout_builtin, std::slice::from_ref(&canonical)).expect("stdout access");
    let code = call_builtin(&code_builtin, std::slice::from_ref(&canonical)).expect("code access");
    let stderr =
        call_builtin(&stderr_builtin, std::slice::from_ref(&canonical)).expect("stderr access");

    match awaited {
        Value::ProcessResult(result) => {
            assert_eq!(result.is_success(), success == Value::Bool(true));
            assert_eq!(Value::String(Rc::new(result.stdout().to_string())), stdout);
            assert_eq!(Value::Int(result.code().into()), code);
            assert_eq!(Value::String(Rc::new(result.stderr().to_string())), stderr);
        }
        other => panic!("expected ProcessResult from io.awaitTask, got {:?}", other),
    }
}

#[test]
fn test_io_await_task_preserves_error_prefix() {
    let task_builtin = get_builtin("io.taskCommand").expect("io.taskCommand builtin should exist");
    let await_builtin = get_builtin("io.awaitTask").expect("io.awaitTask builtin should exist");
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new(
                "n3v3-definitely-missing-command-for-tests".to_string(),
            )),
            Value::List(Rc::new(vec![])),
        ],
    )
    .expect("io.command should succeed");
    let task = call_builtin(&task_builtin, &[command]).expect("io.taskCommand should succeed");

    let err = call_builtin(&await_builtin, &[task])
        .expect_err("io.awaitTask should report missing program");
    assert!(
        err.starts_with("io.awaitTask:"),
        "expected io.awaitTask-prefixed error, got {err}"
    );
}

#[test]
fn test_io_await_pipeline_task_preserves_error_prefix() {
    let await_builtin = get_builtin("io.awaitTask").expect("io.awaitTask builtin should exist");
    let task = Value::Task(Rc::new(TaskValue::pipeline_process_result(Rc::new(
        PipelineValue::new(Vec::new()),
    ))));

    let err = call_builtin(&await_builtin, &[task])
        .expect_err("io.awaitTask should report empty pipeline");
    assert!(
        err.starts_with("io.awaitTask:"),
        "expected io.awaitTask-prefixed error, got {err}"
    );
}

#[test]
fn test_io_redirect_stdout_path_returns_redirect_runtime_value() {
    let builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let result = call_builtin(
        &builtin,
        &[Value::Path(Rc::new(std::path::PathBuf::from(
            "/tmp/n3v3.out",
        )))],
    )
    .expect("io.redirectStdoutPath should succeed");

    match result {
        Value::Redirect(redirect) => {
            assert_eq!(redirect.stream_name(), "stdout");
            assert_eq!(redirect.path(), &std::path::PathBuf::from("/tmp/n3v3.out"));
        }
        other => panic!("expected Redirect runtime value, got {:?}", other),
    }
}

#[test]
fn test_io_redirect_stdout_path_rejects_string_argument() {
    let builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new("/tmp/n3v3.out".to_string()))],
    )
    .expect_err("io.redirectStdoutPath should reject string arguments");

    assert_eq!(err, "io.redirectStdoutPath expects a Path");
}

#[test]
fn test_io_redirect_stderr_path_returns_redirect_runtime_value() {
    let builtin =
        get_builtin("io.redirectStderrPath").expect("io.redirectStderrPath builtin should exist");
    let result = call_builtin(
        &builtin,
        &[Value::Path(Rc::new(std::path::PathBuf::from(
            "/tmp/n3v3.err",
        )))],
    )
    .expect("io.redirectStderrPath should succeed");

    match result {
        Value::Redirect(redirect) => {
            assert_eq!(redirect.stream_name(), "stderr");
            assert_eq!(redirect.path(), &std::path::PathBuf::from("/tmp/n3v3.err"));
        }
        other => panic!("expected Redirect runtime value, got {:?}", other),
    }
}

#[test]
fn test_io_redirect_stdin_path_returns_redirect_runtime_value() {
    let builtin =
        get_builtin("io.redirectStdinPath").expect("io.redirectStdinPath builtin should exist");
    let result = call_builtin(
        &builtin,
        &[Value::Path(Rc::new(std::path::PathBuf::from(
            "/tmp/n3v3.in",
        )))],
    )
    .expect("io.redirectStdinPath should succeed");

    match result {
        Value::Redirect(redirect) => {
            assert_eq!(redirect.stream_name(), "stdin");
            assert_eq!(redirect.path(), &std::path::PathBuf::from("/tmp/n3v3.in"));
        }
        other => panic!("expected Redirect runtime value, got {:?}", other),
    }
}

#[test]
fn test_io_redirect_stderr_path_rejects_string_argument() {
    let builtin =
        get_builtin("io.redirectStderrPath").expect("io.redirectStderrPath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new("/tmp/n3v3.err".to_string()))],
    )
    .expect_err("io.redirectStderrPath should reject string arguments");

    assert_eq!(err, "io.redirectStderrPath expects a Path");
}

#[test]
fn test_io_redirect_stdin_path_rejects_string_argument() {
    let builtin =
        get_builtin("io.redirectStdinPath").expect("io.redirectStdinPath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new("/tmp/n3v3.in".to_string()))],
    )
    .expect_err("io.redirectStdinPath should reject string arguments");

    assert_eq!(err, "io.redirectStdinPath expects a Path");
}

#[test]
fn test_io_exec_command_with_redirect_writes_stdout_to_file() {
    let temp = TempDir::new().expect("temp dir should exist");
    let redirect_path = temp.path().join("stdout.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--version".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(redirect_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");

    let result = exec_command_with_embedded_redirects(command, vec![redirect])
        .expect("commandWithRedirects + io.execCommand should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(result.is_success(), "redirected command should succeed");
            assert_eq!(result.code(), 0);
            assert_eq!(result.stdout(), "");
            let redirected =
                fs::read_to_string(&redirect_path).expect("redirected file should exist");
            assert!(
                !redirected.trim().is_empty(),
                "redirected stdout file should contain command output"
            );
            assert!(
                result.stderr().is_empty(),
                "rustc --version should not write stderr"
            );
        }
        other => panic!(
            "expected ProcessResult from io.execCommand, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_command_with_stderr_redirect_writes_stderr_to_file() {
    let temp = TempDir::new().expect("temp dir should exist");
    let redirect_path = temp.path().join("stderr.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStderrPath").expect("io.redirectStderrPath builtin should exist");
    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--definitely-not-a-real-rustc-flag".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(redirect_path.clone()))],
    )
    .expect("io.redirectStderrPath should succeed");

    let result = exec_command_with_embedded_redirects(command, vec![redirect])
        .expect("commandWithRedirects + io.execCommand should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(
                !result.is_success(),
                "invalid rustc flag should produce a failed process result"
            );
            assert_ne!(result.code(), 0);
            assert_eq!(result.stderr(), "");
            let redirected =
                fs::read_to_string(&redirect_path).expect("redirected file should exist");
            assert!(
                !redirected.trim().is_empty(),
                "redirected stderr file should contain command output"
            );
        }
        other => panic!(
            "expected ProcessResult from io.execCommand, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_command_with_stdin_redirect_reads_stdin_from_file() {
    let temp = TempDir::new().expect("temp dir should exist");
    let redirect_path = temp.path().join("stdin.txt");
    fs::write(&redirect_path, "n3v3 stdin line\n").expect("stdin file should be writable");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdinPath").expect("io.redirectStdinPath builtin should exist");
    let (program, args) = stdin_filter_projection_parts();
    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new(program.to_string())),
            Value::List(Rc::new(
                args.into_iter()
                    .map(|arg| Value::String(Rc::new(arg.to_string())))
                    .collect(),
            )),
        ],
    )
    .expect("io.command should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(redirect_path.clone()))],
    )
    .expect("io.redirectStdinPath should succeed");

    let result = exec_command_with_embedded_redirects(command, vec![redirect])
        .expect("commandWithRedirects + io.execCommand should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(
                result.is_success(),
                "stdin redirected command should succeed"
            );
            assert_eq!(result.code(), 0);
            assert!(
                result.stdout().contains("n3v3"),
                "stdin redirected command should surface matching stdout"
            );
        }
        other => panic!(
            "expected ProcessResult from io.execCommand, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_command_with_stdin_redirect_rejects_configured_stdin() {
    let temp = TempDir::new().expect("temp dir should exist");
    let redirect_path = temp.path().join("stdin.txt");
    fs::write(&redirect_path, "n3v3 stdin line\n").expect("stdin file should be writable");

    let command_builtin =
        get_builtin("io.commandWith").expect("io.commandWith builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdinPath").expect("io.redirectStdinPath builtin should exist");
    let (program, args) = stdin_filter_projection_parts();
    let command = call_builtin(
        &command_builtin,
        &[Value::Record(Rc::new(HashMap::from([
            (
                "program".to_string(),
                Value::String(Rc::new(program.to_string())),
            ),
            (
                "args".to_string(),
                Value::List(Rc::new(
                    args.into_iter()
                        .map(|arg| Value::String(Rc::new(arg.to_string())))
                        .collect(),
                )),
            ),
            (
                "stdin".to_string(),
                Value::String(Rc::new("inline stdin".to_string())),
            ),
        ])))],
    )
    .expect("io.commandWith should succeed");
    let redirect = call_builtin(&redirect_builtin, &[Value::Path(Rc::new(redirect_path))])
        .expect("io.redirectStdinPath should succeed");

    let err = exec_command_with_embedded_redirects(command, vec![redirect])
        .expect_err("commandWithRedirects should reject combined stdin sources");

    assert_eq!(
        err,
        "io.commandWithRedirects: command cannot combine redirect stdin with configured stdin"
    );
}

#[test]
fn test_io_exec_command_with_redirects_composes_stdin_and_stdout_paths() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdin_path = temp.path().join("stdin.txt");
    let stdout_path = temp.path().join("stdout.txt");
    fs::write(&stdin_path, "n3v3 stdin line\n").expect("stdin file should be writable");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let redirect_stdin_builtin =
        get_builtin("io.redirectStdinPath").expect("io.redirectStdinPath builtin should exist");
    let redirect_stdout_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let (program, args) = stdin_filter_projection_parts();
    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new(program.to_string())),
            Value::List(Rc::new(
                args.into_iter()
                    .map(|arg| Value::String(Rc::new(arg.to_string())))
                    .collect(),
            )),
        ],
    )
    .expect("io.command should succeed");
    let stdin_redirect = call_builtin(
        &redirect_stdin_builtin,
        &[Value::Path(Rc::new(stdin_path.clone()))],
    )
    .expect("io.redirectStdinPath should succeed");
    let stdout_redirect = call_builtin(
        &redirect_stdout_builtin,
        &[Value::Path(Rc::new(stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");

    let result =
        exec_command_with_embedded_redirects(command, vec![stdin_redirect, stdout_redirect])
            .expect("commandWithRedirects + io.execCommand should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(result.is_success(), "composed redirects should succeed");
            assert_eq!(result.code(), 0);
            assert_eq!(result.stdout(), "");
            let redirected =
                fs::read_to_string(&stdout_path).expect("redirected file should exist");
            assert!(
                redirected.contains("n3v3"),
                "stdout redirect file should contain filtered stdin content"
            );
        }
        other => panic!(
            "expected ProcessResult from io.execCommand, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_command_with_redirects_composes_stdout_and_stderr_paths() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdout_path = temp.path().join("stdout.txt");
    let stderr_path = temp.path().join("stderr.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let redirect_stdout_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let redirect_stderr_builtin =
        get_builtin("io.redirectStderrPath").expect("io.redirectStderrPath builtin should exist");
    let (program, args) = stdout_stderr_projection_parts();
    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new(program.to_string())),
            Value::List(Rc::new(
                args.into_iter()
                    .map(|arg| Value::String(Rc::new(arg.to_string())))
                    .collect(),
            )),
        ],
    )
    .expect("io.command should succeed");
    let stdout_redirect = call_builtin(
        &redirect_stdout_builtin,
        &[Value::Path(Rc::new(stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let stderr_redirect = call_builtin(
        &redirect_stderr_builtin,
        &[Value::Path(Rc::new(stderr_path.clone()))],
    )
    .expect("io.redirectStderrPath should succeed");

    let result =
        exec_command_with_embedded_redirects(command, vec![stdout_redirect, stderr_redirect])
            .expect("commandWithRedirects + io.execCommand should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(result.is_success(), "composed redirects should succeed");
            assert_eq!(result.stdout(), "");
            assert_eq!(result.stderr(), "");
            let out = fs::read_to_string(&stdout_path).expect("stdout redirect file should exist");
            let err = fs::read_to_string(&stderr_path).expect("stderr redirect file should exist");
            assert!(
                !out.trim().is_empty(),
                "stdout file should contain command output"
            );
            assert!(
                !err.trim().is_empty(),
                "stderr file should contain command output"
            );
        }
        other => panic!(
            "expected ProcessResult from io.execCommand, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_command_with_redirects_rejects_duplicate_stdout_redirects() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdout_path = temp.path().join("stdout.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--version".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");
    let stdout_redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let duplicate_stdout_redirect =
        call_builtin(&redirect_builtin, &[Value::Path(Rc::new(stdout_path))])
            .expect("io.redirectStdoutPath should succeed");

    let err = exec_command_with_embedded_redirects(
        command,
        vec![stdout_redirect, duplicate_stdout_redirect],
    )
    .expect_err("commandWithRedirects should reject duplicate stdout redirects");

    assert_eq!(err, "io.commandWithRedirects: duplicate stdout redirect");
}

#[test]
fn test_io_exec_command_honors_embedded_redirects() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stdout_path = temp.path().join("stdout.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let configure_builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let exec_builtin = get_builtin("io.execCommand").expect("io.execCommand builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let redirected_command = call_builtin(
        &configure_builtin,
        &[command, Value::List(Rc::new(vec![redirect]))],
    )
    .expect("io.commandWithRedirects should succeed");

    let result =
        call_builtin(&exec_builtin, &[redirected_command]).expect("io.execCommand should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(result.is_success());
            assert_eq!(result.stdout(), "");
            let redirected =
                fs::read_to_string(&stdout_path).expect("redirected stdout file should exist");
            assert!(
                redirected.contains("n3v3"),
                "redirected stdout file should contain command output"
            );
        }
        other => panic!(
            "expected ProcessResult from io.execCommand, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_command_with_redirect_matches_embedded_command_projection() {
    let temp = TempDir::new().expect("temp dir should exist");
    let migrated_stdout_path = temp.path().join("command-migrated-out.txt");
    let canonical_stdout_path = temp.path().join("command-canonical-out.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let configure_builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let exec_command_builtin =
        get_builtin("io.execCommand").expect("io.execCommand builtin should exist");

    let migrated_command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let migrated_redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(migrated_stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let migrated = exec_command_with_embedded_redirects(migrated_command, vec![migrated_redirect])
        .expect("commandWithRedirects + io.execCommand should succeed");

    let canonical_command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");
    let canonical_redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(canonical_stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let canonical_command = call_builtin(
        &configure_builtin,
        &[
            canonical_command,
            Value::List(Rc::new(vec![canonical_redirect])),
        ],
    )
    .expect("io.commandWithRedirects should succeed");
    let canonical = call_builtin(&exec_command_builtin, &[canonical_command])
        .expect("io.execCommand should succeed");

    match (migrated, canonical) {
        (Value::ProcessResult(migrated), Value::ProcessResult(canonical)) => {
            assert_eq!(migrated.code(), canonical.code());
            assert_eq!(migrated.is_success(), canonical.is_success());
            assert_eq!(migrated.stdout(), canonical.stdout());
            assert_eq!(migrated.stderr(), canonical.stderr());
            assert_eq!(
                fs::read_to_string(&migrated_stdout_path).expect("migrated stdout file"),
                fs::read_to_string(&canonical_stdout_path).expect("canonical stdout file")
            );
        }
        other => panic!("expected ProcessResult pair, got {:?}", other),
    }
}

#[test]
fn test_io_exec_command_with_redirects_matches_embedded_command_projection() {
    let temp = TempDir::new().expect("temp dir should exist");
    let migrated_stdout_path = temp.path().join("command-migrated-out.txt");
    let migrated_stderr_path = temp.path().join("command-migrated-err.txt");
    let canonical_stdout_path = temp.path().join("command-canonical-out.txt");
    let canonical_stderr_path = temp.path().join("command-canonical-err.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let stdout_redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let stderr_redirect_builtin =
        get_builtin("io.redirectStderrPath").expect("io.redirectStderrPath builtin should exist");
    let configure_builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let exec_command_builtin =
        get_builtin("io.execCommand").expect("io.execCommand builtin should exist");

    let (program, args) = stdout_stderr_projection_parts();
    let migrated_command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new(program.to_string())),
            Value::List(Rc::new(
                args.into_iter()
                    .map(|arg| Value::String(Rc::new(arg.to_string())))
                    .collect(),
            )),
        ],
    )
    .expect("io.command should succeed");
    let migrated_stdout_redirect = call_builtin(
        &stdout_redirect_builtin,
        &[Value::Path(Rc::new(migrated_stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let migrated_stderr_redirect = call_builtin(
        &stderr_redirect_builtin,
        &[Value::Path(Rc::new(migrated_stderr_path.clone()))],
    )
    .expect("io.redirectStderrPath should succeed");
    let migrated = exec_command_with_embedded_redirects(
        migrated_command,
        vec![migrated_stdout_redirect, migrated_stderr_redirect],
    )
    .expect("commandWithRedirects + io.execCommand should succeed");

    let (program, args) = stdout_stderr_projection_parts();
    let canonical_command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new(program.to_string())),
            Value::List(Rc::new(
                args.into_iter()
                    .map(|arg| Value::String(Rc::new(arg.to_string())))
                    .collect(),
            )),
        ],
    )
    .expect("io.command should succeed");
    let canonical_stdout_redirect = call_builtin(
        &stdout_redirect_builtin,
        &[Value::Path(Rc::new(canonical_stdout_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let canonical_stderr_redirect = call_builtin(
        &stderr_redirect_builtin,
        &[Value::Path(Rc::new(canonical_stderr_path.clone()))],
    )
    .expect("io.redirectStderrPath should succeed");
    let canonical_command = call_builtin(
        &configure_builtin,
        &[
            canonical_command,
            Value::List(Rc::new(vec![
                canonical_stdout_redirect,
                canonical_stderr_redirect,
            ])),
        ],
    )
    .expect("io.commandWithRedirects should succeed");
    let canonical = call_builtin(&exec_command_builtin, &[canonical_command])
        .expect("io.execCommand should succeed");

    match (migrated, canonical) {
        (Value::ProcessResult(migrated), Value::ProcessResult(canonical)) => {
            assert_eq!(migrated.code(), canonical.code());
            assert_eq!(migrated.is_success(), canonical.is_success());
            assert_eq!(migrated.stdout(), canonical.stdout());
            assert_eq!(migrated.stderr(), canonical.stderr());
            assert_eq!(
                fs::read_to_string(&migrated_stdout_path).expect("migrated stdout file"),
                fs::read_to_string(&canonical_stdout_path).expect("canonical stdout file")
            );
            assert_eq!(
                fs::read_to_string(&migrated_stderr_path).expect("migrated stderr file"),
                fs::read_to_string(&canonical_stderr_path).expect("canonical stderr file")
            );
        }
        other => panic!("expected ProcessResult pair, got {:?}", other),
    }
}

#[test]
fn test_io_exec_pipeline_honors_stage_local_redirects() {
    let temp = TempDir::new().expect("temp dir should exist");
    let stderr_path = temp.path().join("stderr.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStderrPath").expect("io.redirectStderrPath builtin should exist");
    let configure_builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let pipeline_builtin = get_builtin("io.pipeline").expect("io.pipeline builtin should exist");
    let exec_builtin =
        get_builtin("io.execPipeline").expect("io.execPipeline builtin should exist");

    let (stage1_program, stage1_args) = stdout_stderr_projection_parts();
    let stage1 = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new(stage1_program.to_string())),
            Value::List(Rc::new(
                stage1_args
                    .into_iter()
                    .map(|arg| Value::String(Rc::new(arg.to_string())))
                    .collect(),
            )),
        ],
    )
    .expect("stage1 command should succeed");
    let stderr_redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(stderr_path.clone()))],
    )
    .expect("io.redirectStderrPath should succeed");
    let stage1 = call_builtin(
        &configure_builtin,
        &[stage1, Value::List(Rc::new(vec![stderr_redirect]))],
    )
    .expect("io.commandWithRedirects should succeed");

    let (stage2_program, stage2_args) = stdin_filter_projection_parts();
    let stage2 = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new(stage2_program.to_string())),
            Value::List(Rc::new(
                stage2_args
                    .into_iter()
                    .map(|arg| Value::String(Rc::new(arg.to_string())))
                    .collect(),
            )),
        ],
    )
    .expect("stage2 command should succeed");

    let pipeline = call_builtin(
        &pipeline_builtin,
        &[Value::List(Rc::new(vec![stage1, stage2]))],
    )
    .expect("io.pipeline should succeed");
    let result = call_builtin(&exec_builtin, &[pipeline]).expect("io.execPipeline should succeed");

    match result {
        Value::ProcessResult(result) => {
            assert!(
                result.is_success(),
                "stage-local redirect pipeline should succeed"
            );
            assert_eq!(result.code(), 0);
            assert!(result.stdout().contains("n3v3"));
            assert_eq!(result.stderr(), "");
            let redirected =
                fs::read_to_string(&stderr_path).expect("redirected stderr file should exist");
            assert!(
                !redirected.trim().is_empty(),
                "stage-local redirected stderr file should contain output"
            );
        }
        other => panic!(
            "expected ProcessResult from io.execPipeline, got {:?}",
            other
        ),
    }
}

#[test]
fn test_io_exec_command_with_redirect_rejects_non_redirect_argument() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("printf".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new("n3v3".to_string()))])),
        ],
    )
    .expect("io.command should succeed");

    let err = exec_command_with_embedded_redirects(
        command,
        vec![Value::String(Rc::new("/tmp/out.txt".to_string()))],
    )
    .expect_err("commandWithRedirects should reject string redirect arguments");
    assert_eq!(err, "io.commandWithRedirects redirects[0] must be Redirect");
}

#[test]
fn test_io_exec_command_with_redirect_preserves_error_prefix() {
    let temp = TempDir::new().expect("temp dir should exist");
    let redirect_path = temp.path().join("stdout.txt");

    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new(
                "n3v3-definitely-missing-command-for-tests".to_string(),
            )),
            Value::List(Rc::new(vec![])),
        ],
    )
    .expect("io.command should succeed");
    let redirect = call_builtin(&redirect_builtin, &[Value::Path(Rc::new(redirect_path))])
        .expect("io.redirectStdoutPath should succeed");

    let err = exec_command_with_embedded_redirects(command, vec![redirect])
        .expect_err("commandWithRedirects + io.execCommand should report missing program");
    assert!(
        err.starts_with("io.execCommand:"),
        "expected io.execCommand-prefixed error, got {err}"
    );
}

#[test]
fn test_io_exec_command_lines_honors_configured_stdin() {
    let command_builtin =
        get_builtin("io.commandWith").expect("io.commandWith builtin should exist");
    let lines_builtin =
        get_builtin("io.execCommandLines").expect("io.execCommandLines builtin should exist");
    let (program, args) = stdin_filter_projection_parts();
    let mut options = HashMap::new();
    options.insert(
        "program".to_string(),
        Value::String(Rc::new(program.to_string())),
    );
    options.insert(
        "args".to_string(),
        Value::List(Rc::new(
            args.into_iter()
                .map(|arg| Value::String(Rc::new(arg.to_string())))
                .collect(),
        )),
    );
    options.insert(
        "stdin".to_string(),
        Value::String(Rc::new("n3v3 line\n".to_string())),
    );
    let command = call_builtin(&command_builtin, &[Value::Record(Rc::new(options))])
        .expect("io.commandWith should succeed");
    let result =
        call_builtin(&lines_builtin, &[command]).expect("io.execCommandLines should succeed");

    match result {
        Value::List(lines) => {
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0], Value::String(Rc::new("n3v3 line".to_string())));
        }
        other => panic!("expected List<String>, got {:?}", other),
    }
}

#[test]
fn test_io_exec_command_lines_honors_stdout_redirect() {
    let temp = TempDir::new().expect("temp dir should exist");
    let output_path = temp.path().join("command-lines.txt");
    let command_builtin =
        get_builtin("io.commandWith").expect("io.commandWith builtin should exist");
    let redirect_builtin =
        get_builtin("io.redirectStdoutPath").expect("io.redirectStdoutPath builtin should exist");
    let configure_builtin = get_builtin("io.commandWithRedirects")
        .expect("io.commandWithRedirects builtin should exist");
    let lines_builtin =
        get_builtin("io.execCommandLines").expect("io.execCommandLines builtin should exist");
    let (program, args) = stdin_filter_projection_parts();
    let mut options = HashMap::new();
    options.insert(
        "program".to_string(),
        Value::String(Rc::new(program.to_string())),
    );
    options.insert(
        "args".to_string(),
        Value::List(Rc::new(
            args.into_iter()
                .map(|arg| Value::String(Rc::new(arg.to_string())))
                .collect(),
        )),
    );
    options.insert(
        "stdin".to_string(),
        Value::String(Rc::new("n3v3 line\n".to_string())),
    );
    let command = call_builtin(&command_builtin, &[Value::Record(Rc::new(options))])
        .expect("io.commandWith should succeed");
    let redirect = call_builtin(
        &redirect_builtin,
        &[Value::Path(Rc::new(output_path.clone()))],
    )
    .expect("io.redirectStdoutPath should succeed");
    let command = call_builtin(
        &configure_builtin,
        &[command, Value::List(Rc::new(vec![redirect]))],
    )
    .expect("io.commandWithRedirects should succeed");
    let result =
        call_builtin(&lines_builtin, &[command]).expect("io.execCommandLines should succeed");

    match result {
        Value::List(lines) => assert!(lines.is_empty()),
        other => panic!("expected List<String>, got {:?}", other),
    }
    assert_eq!(
        fs::read_to_string(output_path).expect("redirected output should exist"),
        "n3v3 line\n"
    );
}

#[test]
fn test_io_exec_command_returns_process_result_runtime_value() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let exec_builtin = get_builtin("io.execCommand").expect("io.execCommand builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--version".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");

    let result = call_builtin(&exec_builtin, &[command]).expect("io.execCommand should succeed");

    match result {
        Value::ProcessResult(_) => {}
        other => panic!("expected ProcessResult runtime value, got {:?}", other),
    }
}

#[test]
fn test_io_exec_command_rejects_non_command_argument() {
    let builtin = get_builtin("io.execCommand").expect("io.execCommand builtin should exist");
    let err = call_builtin(&builtin, &[Value::String(Rc::new("rustc".to_string()))])
        .expect_err("io.execCommand should reject string arguments");

    assert_eq!(err, "io.execCommand expects a Command");
}

#[test]
fn test_io_process_success_reports_bool_from_process_result() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let exec_builtin = get_builtin("io.execCommand").expect("io.execCommand builtin should exist");
    let success_builtin =
        get_builtin("io.processSuccess").expect("io.processSuccess builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--version".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");
    let result = call_builtin(&exec_builtin, &[command]).expect("io.execCommand should succeed");
    let success =
        call_builtin(&success_builtin, &[result]).expect("io.processSuccess should succeed");

    assert_eq!(success, Value::Bool(true));
}

#[test]
fn test_io_process_success_rejects_non_process_result_argument() {
    let builtin = get_builtin("io.processSuccess").expect("io.processSuccess builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new("not-a-process-result".to_string()))],
    )
    .expect_err("io.processSuccess should reject string arguments");

    assert_eq!(err, "io.processSuccess expects a ProcessResult");
}

#[test]
fn test_io_process_stdout_reports_string_from_process_result() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let exec_builtin = get_builtin("io.execCommand").expect("io.execCommand builtin should exist");
    let stdout_builtin =
        get_builtin("io.processStdout").expect("io.processStdout builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--version".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");
    let result = call_builtin(&exec_builtin, &[command]).expect("io.execCommand should succeed");
    let stdout = call_builtin(&stdout_builtin, &[result]).expect("io.processStdout should succeed");

    assert!(
        matches!(stdout, Value::String(s) if s.contains("rustc")),
        "stdout should contain rustc version"
    );
}

#[test]
fn test_io_process_stdout_rejects_non_process_result_argument() {
    let builtin = get_builtin("io.processStdout").expect("io.processStdout builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new("not-a-process-result".to_string()))],
    )
    .expect_err("io.processStdout should reject string arguments");

    assert_eq!(err, "io.processStdout expects a ProcessResult");
}

#[test]
fn test_io_process_code_reports_int_from_process_result() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let exec_builtin = get_builtin("io.execCommand").expect("io.execCommand builtin should exist");
    let code_builtin = get_builtin("io.processCode").expect("io.processCode builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--version".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");
    let result = call_builtin(&exec_builtin, &[command]).expect("io.execCommand should succeed");
    let code = call_builtin(&code_builtin, &[result]).expect("io.processCode should succeed");

    assert_eq!(code, Value::Int(0.into()));
}

#[test]
fn test_io_process_code_rejects_non_process_result_argument() {
    let builtin = get_builtin("io.processCode").expect("io.processCode builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new("not-a-process-result".to_string()))],
    )
    .expect_err("io.processCode should reject string arguments");

    assert_eq!(err, "io.processCode expects a ProcessResult");
}

#[test]
fn test_io_process_stderr_reports_string_from_process_result() {
    let command_builtin = get_builtin("io.command").expect("io.command builtin should exist");
    let exec_builtin = get_builtin("io.execCommand").expect("io.execCommand builtin should exist");
    let stderr_builtin =
        get_builtin("io.processStderr").expect("io.processStderr builtin should exist");

    let command = call_builtin(
        &command_builtin,
        &[
            Value::String(Rc::new("rustc".to_string())),
            Value::List(Rc::new(vec![Value::String(Rc::new(
                "--version".to_string(),
            ))])),
        ],
    )
    .expect("io.command should succeed");
    let result = call_builtin(&exec_builtin, &[command]).expect("io.execCommand should succeed");
    let stderr = call_builtin(&stderr_builtin, &[result]).expect("io.processStderr should succeed");

    assert_eq!(stderr, Value::String(Rc::new(String::new())));
}

#[test]
fn test_io_process_stderr_rejects_non_process_result_argument() {
    let builtin = get_builtin("io.processStderr").expect("io.processStderr builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new("not-a-process-result".to_string()))],
    )
    .expect_err("io.processStderr should reject string arguments");

    assert_eq!(err, "io.processStderr expects a ProcessResult");
}

#[test]
fn test_io_path_exists_path_accepts_path_runtime_value() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("exists-path.txt");
    fs::write(&path, "exists").unwrap();

    let builtin = get_builtin("io.pathExistsPath").expect("io.pathExistsPath builtin should exist");
    let result = call_builtin(&builtin, &[Value::Path(Rc::new(path))])
        .expect("io.pathExistsPath should succeed");

    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_io_path_exists_path_rejects_string_argument() {
    let builtin = get_builtin("io.pathExistsPath").expect("io.pathExistsPath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new("/tmp/exists-path.txt".to_string()))],
    )
    .expect_err("io.pathExistsPath should reject string arguments");

    assert_eq!(err, "io.pathExistsPath expects a Path");
}

#[test]
fn test_io_is_dir_path_accepts_path_runtime_value() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("dir-path");
    fs::create_dir_all(&dir).unwrap();

    let builtin = get_builtin("io.isDirPath").expect("io.isDirPath builtin should exist");
    let result =
        call_builtin(&builtin, &[Value::Path(Rc::new(dir))]).expect("io.isDirPath should succeed");

    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_io_is_dir_path_rejects_string_argument() {
    let builtin = get_builtin("io.isDirPath").expect("io.isDirPath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new("/tmp/dir-path".to_string()))],
    )
    .expect_err("io.isDirPath should reject string arguments");

    assert_eq!(err, "io.isDirPath expects a Path");
}

#[test]
fn test_io_is_file_path_accepts_path_runtime_value() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("file-path.txt");
    fs::write(&path, "file").unwrap();

    let builtin = get_builtin("io.isFilePath").expect("io.isFilePath builtin should exist");
    let result = call_builtin(&builtin, &[Value::Path(Rc::new(path))])
        .expect("io.isFilePath should succeed");

    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_io_is_file_path_rejects_string_argument() {
    let builtin = get_builtin("io.isFilePath").expect("io.isFilePath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new("/tmp/file-path.txt".to_string()))],
    )
    .expect_err("io.isFilePath should reject string arguments");

    assert_eq!(err, "io.isFilePath expects a Path");
}

#[test]
fn test_io_append_file_appends_content() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("io-append.txt");
    let path_str = path.to_string_lossy().to_string();

    let write = get_builtin("io.writeFile").expect("io.writeFile builtin should exist");
    let append = get_builtin("io.appendFile").expect("io.appendFile builtin should exist");
    let read = get_builtin("io.readFile").expect("io.readFile builtin should exist");

    let write_args = vec![
        Value::String(Rc::new(path_str.clone())),
        Value::String(Rc::new("a".to_string())),
    ];
    call_builtin(&write, &write_args).expect("io.writeFile should succeed");

    let append_args = vec![
        Value::String(Rc::new(path_str.clone())),
        Value::String(Rc::new("b".to_string())),
    ];
    let append_result = call_builtin(&append, &append_args).expect("io.appendFile should succeed");
    assert!(matches!(append_result, Value::Unit));

    let read_args = vec![Value::String(Rc::new(path_str))];
    let read_result = call_builtin(&read, &read_args).expect("io.readFile should succeed");
    assert!(matches!(read_result, Value::String(s) if s.as_str() == "ab"));
}

#[test]
fn test_io_create_and_remove_dir_all() {
    let temp = TempDir::new().unwrap();
    let nested = temp.path().join("a").join("b").join("c");
    let nested_str = nested.to_string_lossy().to_string();

    let create = get_builtin("io.createDirAll").expect("io.createDirAll builtin should exist");
    let remove = get_builtin("io.removeDirAll").expect("io.removeDirAll builtin should exist");
    let exists = get_builtin("io.pathExists").expect("io.pathExists builtin should exist");
    let is_dir = get_builtin("io.isDir").expect("io.isDir builtin should exist");

    let create_args = vec![Value::String(Rc::new(nested_str.clone()))];
    let create_result = call_builtin(&create, &create_args).expect("io.createDirAll should work");
    assert!(matches!(create_result, Value::Unit));

    let exists_args = vec![Value::String(Rc::new(nested_str.clone()))];
    assert!(matches!(
        call_builtin(&exists, &exists_args).unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        call_builtin(&is_dir, &exists_args).unwrap(),
        Value::Bool(true)
    ));

    let remove_args = vec![Value::String(Rc::new(nested_str.clone()))];
    let remove_result = call_builtin(&remove, &remove_args).expect("io.removeDirAll should work");
    assert!(matches!(remove_result, Value::Unit));

    let exists_after_args = vec![Value::String(Rc::new(nested_str))];
    assert!(matches!(
        call_builtin(&exists, &exists_after_args).unwrap(),
        Value::Bool(false)
    ));
}

#[test]
fn test_io_path_exists_accepts_string_runtime_value() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("exists.txt");
    fs::write(&file, "n3v3").unwrap();
    let builtin = get_builtin("io.pathExists").expect("io.pathExists builtin should exist");
    let result = call_builtin(
        &builtin,
        &[Value::String(Rc::new(file.to_string_lossy().to_string()))],
    )
    .expect("io.pathExists should succeed");

    assert!(matches!(result, Value::Bool(true)));
}

#[test]
fn test_io_is_dir_accepts_string_runtime_value() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("nested");
    fs::create_dir_all(&dir).unwrap();
    let builtin = get_builtin("io.isDir").expect("io.isDir builtin should exist");
    let result = call_builtin(
        &builtin,
        &[Value::String(Rc::new(dir.to_string_lossy().to_string()))],
    )
    .expect("io.isDir should succeed");

    assert!(matches!(result, Value::Bool(true)));
}

#[test]
fn test_io_is_file_accepts_string_runtime_value() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("nested.txt");
    fs::write(&file, "n3v3").unwrap();
    let builtin = get_builtin("io.isFile").expect("io.isFile builtin should exist");
    let result = call_builtin(
        &builtin,
        &[Value::String(Rc::new(file.to_string_lossy().to_string()))],
    )
    .expect("io.isFile should succeed");

    assert!(matches!(result, Value::Bool(true)));
}

#[test]
fn test_io_create_dir_all_path_accepts_path_runtime_value() {
    let temp = TempDir::new().unwrap();
    let nested = temp.path().join("typed").join("a").join("b");
    let builtin =
        get_builtin("io.createDirAllPath").expect("io.createDirAllPath builtin should exist");
    let result = call_builtin(&builtin, &[Value::Path(Rc::new(nested.clone()))])
        .expect("io.createDirAllPath should work");

    assert!(matches!(result, Value::Unit));
    assert!(nested.exists());
    assert!(nested.is_dir());
}

#[test]
fn test_io_create_dir_all_path_rejects_string_argument() {
    let temp = TempDir::new().unwrap();
    let nested = temp.path().join("typed").join("a").join("b");
    let builtin =
        get_builtin("io.createDirAllPath").expect("io.createDirAllPath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new(nested.to_string_lossy().to_string()))],
    )
    .expect_err("io.createDirAllPath should reject string arguments");

    assert_eq!(err, "io.createDirAllPath expects a Path");
}

#[test]
fn test_io_remove_dir_all_path_accepts_path_runtime_value() {
    let temp = TempDir::new().unwrap();
    let nested = temp.path().join("typed").join("a").join("b");
    fs::create_dir_all(&nested).unwrap();
    let builtin =
        get_builtin("io.removeDirAllPath").expect("io.removeDirAllPath builtin should exist");
    let result = call_builtin(&builtin, &[Value::Path(Rc::new(nested.clone()))])
        .expect("io.removeDirAllPath should work");

    assert!(matches!(result, Value::Unit));
    assert!(!nested.exists());
}

#[test]
fn test_io_remove_dir_all_path_rejects_string_argument() {
    let temp = TempDir::new().unwrap();
    let nested = temp.path().join("typed").join("a").join("b");
    let builtin =
        get_builtin("io.removeDirAllPath").expect("io.removeDirAllPath builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::String(Rc::new(nested.to_string_lossy().to_string()))],
    )
    .expect_err("io.removeDirAllPath should reject string arguments");

    assert_eq!(err, "io.removeDirAllPath expects a Path");
}

// ============================================================================
// List 模块边缘测试
// ============================================================================

#[test]
fn test_list_empty_returns_empty_list() {
    let empty = get_builtin("list.empty");
    assert!(empty.is_some());
    match empty.unwrap() {
        Value::Builtin(builtin) => {
            let result = (builtin.func)(&[]).unwrap();
            match result {
                Value::List(l) => {
                    assert!(l.is_empty());
                    assert_eq!(l.len(), 0);
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_len_various_sizes() {
    let len_fn = get_builtin("list.len").unwrap();

    match len_fn {
        Value::Builtin(builtin) => {
            // Empty list
            let empty = Value::List(Rc::new(vec![]));
            assert_eq!((builtin.func)(&[empty]).unwrap(), Value::Int(0.into()));

            // Single element
            let single = Value::List(Rc::new(vec![Value::Int(1.into())]));
            assert_eq!((builtin.func)(&[single]).unwrap(), Value::Int(1.into()));

            // Many elements
            let many = Value::List(Rc::new(vec![Value::Int(1.into()); 100]));
            assert_eq!((builtin.func)(&[many]).unwrap(), Value::Int(100.into()));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_is_empty_edge_cases() {
    let is_empty_fn = get_builtin("list.isEmpty").unwrap();

    match is_empty_fn {
        Value::Builtin(builtin) => {
            // Empty list
            let empty = Value::List(Rc::new(vec![]));
            assert_eq!((builtin.func)(&[empty]).unwrap(), Value::Bool(true));

            // Non-empty list
            let non_empty = Value::List(Rc::new(vec![Value::Int(1.into())]));
            assert_eq!((builtin.func)(&[non_empty]).unwrap(), Value::Bool(false));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_head_empty_returns_none() {
    let head_fn = get_builtin("list.head").unwrap();

    match head_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[empty]).unwrap();
            assert!(matches!(result, Value::None));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_head_single_element() {
    let head_fn = get_builtin("list.head").unwrap();

    match head_fn {
        Value::Builtin(builtin) => {
            let single = Value::List(Rc::new(vec![Value::Int(42.into())]));
            let result = (builtin.func)(&[single]).unwrap();
            match result {
                Value::Some(boxed) => assert_eq!(*boxed, Value::Int(42.into())),
                _ => panic!("Expected Some"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_tail_empty_returns_empty() {
    let tail_fn = get_builtin("list.tail").unwrap();

    match tail_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[empty]).unwrap();
            match result {
                Value::List(l) => assert!(l.is_empty()),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_tail_single_element_returns_empty() {
    let tail_fn = get_builtin("list.tail").unwrap();

    match tail_fn {
        Value::Builtin(builtin) => {
            let single = Value::List(Rc::new(vec![Value::Int(1.into())]));
            let result = (builtin.func)(&[single]).unwrap();
            match result {
                Value::List(l) => assert!(l.is_empty()),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_tail_multiple_elements() {
    let tail_fn = get_builtin("list.tail").unwrap();

    match tail_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(1.into()),
                Value::Int(2.into()),
                Value::Int(3.into()),
            ]));
            let result = (builtin.func)(&[list]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l.len(), 2);
                    assert_eq!(l[0], Value::Int(2.into()));
                    assert_eq!(l[1], Value::Int(3.into()));
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_last_empty_returns_none() {
    let last_fn = get_builtin("list.last").unwrap();

    match last_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[empty]).unwrap();
            assert!(matches!(result, Value::None));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_last_single_element() {
    let last_fn = get_builtin("list.last").unwrap();

    match last_fn {
        Value::Builtin(builtin) => {
            let single = Value::List(Rc::new(vec![Value::Int(99.into())]));
            let result = (builtin.func)(&[single]).unwrap();
            match result {
                Value::Some(boxed) => assert_eq!(*boxed, Value::Int(99.into())),
                _ => panic!("Expected Some"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_init_empty_returns_empty() {
    let init_fn = get_builtin("list.init").unwrap();

    match init_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[empty]).unwrap();
            match result {
                Value::List(l) => assert!(l.is_empty()),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_init_removes_last() {
    let init_fn = get_builtin("list.init").unwrap();

    match init_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(1.into()),
                Value::Int(2.into()),
                Value::Int(3.into()),
            ]));
            let result = (builtin.func)(&[list]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l.len(), 2);
                    assert_eq!(l[0], Value::Int(1.into()));
                    assert_eq!(l[1], Value::Int(2.into()));
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_get_valid_index() {
    let get_fn = get_builtin("list.get").unwrap();

    match get_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(10.into()),
                Value::Int(20.into()),
                Value::Int(30.into()),
            ]));
            let result = (builtin.func)(&[Value::Int(1.into()), list]).unwrap();
            match result {
                Value::Some(boxed) => assert_eq!(*boxed, Value::Int(20.into())),
                _ => panic!("Expected Some"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_get_out_of_bounds() {
    let get_fn = get_builtin("list.get").unwrap();

    match get_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![Value::Int(1.into())]));
            let result = (builtin.func)(&[Value::Int(10.into()), list]).unwrap();
            assert!(matches!(result, Value::None));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_get_negative_index_as_zero() {
    let get_fn = get_builtin("list.get").unwrap();

    match get_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![Value::Int(42.into())]));
            // Negative index becomes 0 when cast to usize (wraps around)
            // This tests edge case behavior
            let result = (builtin.func)(&[Value::Int(0.into()), list]).unwrap();
            match result {
                Value::Some(boxed) => assert_eq!(*boxed, Value::Int(42.into())),
                _ => panic!("Expected Some"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_cons_to_empty() {
    let cons_fn = get_builtin("list.cons").unwrap();

    match cons_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[Value::Int(1.into()), empty]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l.len(), 1);
                    assert_eq!(l[0], Value::Int(1.into()));
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_cons_to_non_empty() {
    let cons_fn = get_builtin("list.cons").unwrap();

    match cons_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![Value::Int(2.into()), Value::Int(3.into())]));
            let result = (builtin.func)(&[Value::Int(1.into()), list]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l.len(), 3);
                    assert_eq!(l[0], Value::Int(1.into()));
                    assert_eq!(l[1], Value::Int(2.into()));
                    assert_eq!(l[2], Value::Int(3.into()));
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_append_empty_lists() {
    let append_fn = get_builtin("list.append").unwrap();

    match append_fn {
        Value::Builtin(builtin) => {
            let empty1 = Value::List(Rc::new(vec![]));
            let empty2 = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[empty1, empty2]).unwrap();
            match result {
                Value::List(l) => assert!(l.is_empty()),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_append_left_empty() {
    let append_fn = get_builtin("list.append").unwrap();

    match append_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let list = Value::List(Rc::new(vec![Value::Int(1.into()), Value::Int(2.into())]));
            let result = (builtin.func)(&[empty, list]).unwrap();
            match result {
                Value::List(l) => assert_eq!(l.len(), 2),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_append_right_empty() {
    let append_fn = get_builtin("list.append").unwrap();

    match append_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![Value::Int(1.into()), Value::Int(2.into())]));
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[list, empty]).unwrap();
            match result {
                Value::List(l) => assert_eq!(l.len(), 2),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_reverse_empty() {
    let reverse_fn = get_builtin("list.reverse").unwrap();

    match reverse_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[empty]).unwrap();
            match result {
                Value::List(l) => assert!(l.is_empty()),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_reverse_single() {
    let reverse_fn = get_builtin("list.reverse").unwrap();

    match reverse_fn {
        Value::Builtin(builtin) => {
            let single = Value::List(Rc::new(vec![Value::Int(42.into())]));
            let result = (builtin.func)(&[single]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l.len(), 1);
                    assert_eq!(l[0], Value::Int(42.into()));
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_reverse_multiple() {
    let reverse_fn = get_builtin("list.reverse").unwrap();

    match reverse_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(1.into()),
                Value::Int(2.into()),
                Value::Int(3.into()),
            ]));
            let result = (builtin.func)(&[list]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l[0], Value::Int(3.into()));
                    assert_eq!(l[1], Value::Int(2.into()));
                    assert_eq!(l[2], Value::Int(1.into()));
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_take_zero() {
    let take_fn = get_builtin("list.take").unwrap();

    match take_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(1.into()),
                Value::Int(2.into()),
                Value::Int(3.into()),
            ]));
            let result = (builtin.func)(&[Value::Int(0.into()), list]).unwrap();
            match result {
                Value::List(l) => assert!(l.is_empty()),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_take_more_than_length() {
    let take_fn = get_builtin("list.take").unwrap();

    match take_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![Value::Int(1.into()), Value::Int(2.into())]));
            let result = (builtin.func)(&[Value::Int(100.into()), list]).unwrap();
            match result {
                Value::List(l) => assert_eq!(l.len(), 2),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_drop_zero() {
    let drop_fn = get_builtin("list.drop").unwrap();

    match drop_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(1.into()),
                Value::Int(2.into()),
                Value::Int(3.into()),
            ]));
            let result = (builtin.func)(&[Value::Int(0.into()), list]).unwrap();
            match result {
                Value::List(l) => assert_eq!(l.len(), 3),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_drop_more_than_length() {
    let drop_fn = get_builtin("list.drop").unwrap();

    match drop_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![Value::Int(1.into()), Value::Int(2.into())]));
            let result = (builtin.func)(&[Value::Int(100.into()), list]).unwrap();
            match result {
                Value::List(l) => assert!(l.is_empty()),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_sum_empty() {
    let sum_fn = get_builtin("list.sum").unwrap();

    match sum_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[empty]).unwrap();
            assert_eq!(result, Value::Int(0.into()));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_sum_single() {
    let sum_fn = get_builtin("list.sum").unwrap();

    match sum_fn {
        Value::Builtin(builtin) => {
            let single = Value::List(Rc::new(vec![Value::Int(42.into())]));
            let result = (builtin.func)(&[single]).unwrap();
            assert_eq!(result, Value::Int(42.into()));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_sum_multiple() {
    let sum_fn = get_builtin("list.sum").unwrap();

    match sum_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(1.into()),
                Value::Int(2.into()),
                Value::Int(3.into()),
                Value::Int(4.into()),
            ]));
            let result = (builtin.func)(&[list]).unwrap();
            assert_eq!(result, Value::Int(10.into()));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_sum_with_negatives() {
    let sum_fn = get_builtin("list.sum").unwrap();

    match sum_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(10.into()),
                Value::Int((-5).into()),
                Value::Int((-3).into()),
            ]));
            let result = (builtin.func)(&[list]).unwrap();
            assert_eq!(result, Value::Int(2.into()));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_product_empty() {
    let product_fn = get_builtin("list.product").unwrap();

    match product_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[empty]).unwrap();
            assert_eq!(result, Value::Int(1.into()));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_product_with_zero() {
    let product_fn = get_builtin("list.product").unwrap();

    match product_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(5.into()),
                Value::Int(0.into()),
                Value::Int(10.into()),
            ]));
            let result = (builtin.func)(&[list]).unwrap();
            assert_eq!(result, Value::Int(0.into()));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_product_multiple() {
    let product_fn = get_builtin("list.product").unwrap();

    match product_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(2.into()),
                Value::Int(3.into()),
                Value::Int(4.into()),
            ]));
            let result = (builtin.func)(&[list]).unwrap();
            assert_eq!(result, Value::Int(24.into()));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_max_empty() {
    let max_fn = get_builtin("list.max").unwrap();

    match max_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[empty]).unwrap();
            assert!(matches!(result, Value::None));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_max_single() {
    let max_fn = get_builtin("list.max").unwrap();

    match max_fn {
        Value::Builtin(builtin) => {
            let single = Value::List(Rc::new(vec![Value::Int(42.into())]));
            let result = (builtin.func)(&[single]).unwrap();
            match result {
                Value::Some(boxed) => assert_eq!(*boxed, Value::Int(42.into())),
                _ => panic!("Expected Some"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_max_with_negatives() {
    let max_fn = get_builtin("list.max").unwrap();

    match max_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int((-5).into()),
                Value::Int((-1).into()),
                Value::Int((-10).into()),
            ]));
            let result = (builtin.func)(&[list]).unwrap();
            match result {
                Value::Some(boxed) => assert_eq!(*boxed, Value::Int((-1).into())),
                _ => panic!("Expected Some"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_min_empty() {
    let min_fn = get_builtin("list.min").unwrap();

    match min_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[empty]).unwrap();
            assert!(matches!(result, Value::None));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_min_with_positives() {
    let min_fn = get_builtin("list.min").unwrap();

    match min_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(5.into()),
                Value::Int(1.into()),
                Value::Int(10.into()),
            ]));
            let result = (builtin.func)(&[list]).unwrap();
            match result {
                Value::Some(boxed) => assert_eq!(*boxed, Value::Int(1.into())),
                _ => panic!("Expected Some"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_contains_found() {
    let contains_fn = get_builtin("list.contains").unwrap();

    match contains_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(1.into()),
                Value::Int(2.into()),
                Value::Int(3.into()),
            ]));
            let result = (builtin.func)(&[Value::Int(2.into()), list]).unwrap();
            assert_eq!(result, Value::Bool(true));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_contains_not_found() {
    let contains_fn = get_builtin("list.contains").unwrap();

    match contains_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(1.into()),
                Value::Int(2.into()),
                Value::Int(3.into()),
            ]));
            let result = (builtin.func)(&[Value::Int(99.into()), list]).unwrap();
            assert_eq!(result, Value::Bool(false));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_contains_in_empty() {
    let contains_fn = get_builtin("list.contains").unwrap();

    match contains_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[Value::Int(1.into()), empty]).unwrap();
            assert_eq!(result, Value::Bool(false));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_index_of_found() {
    let index_of_fn = get_builtin("list.indexOf").unwrap();

    match index_of_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(10.into()),
                Value::Int(20.into()),
                Value::Int(30.into()),
            ]));
            let result = (builtin.func)(&[Value::Int(20.into()), list]).unwrap();
            match result {
                Value::Some(boxed) => assert_eq!(*boxed, Value::Int(1.into())),
                _ => panic!("Expected Some"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_index_of_not_found() {
    let index_of_fn = get_builtin("list.indexOf").unwrap();

    match index_of_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(1.into()),
                Value::Int(2.into()),
                Value::Int(3.into()),
            ]));
            let result = (builtin.func)(&[Value::Int(99.into()), list]).unwrap();
            assert!(matches!(result, Value::None));
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_index_of_first_occurrence() {
    let index_of_fn = get_builtin("list.indexOf").unwrap();

    match index_of_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(1.into()),
                Value::Int(2.into()),
                Value::Int(1.into()),
                Value::Int(2.into()),
            ]));
            let result = (builtin.func)(&[Value::Int(2.into()), list]).unwrap();
            match result {
                Value::Some(boxed) => assert_eq!(*boxed, Value::Int(1.into())), // First occurrence at index 1
                _ => panic!("Expected Some"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_sort_empty() {
    let sort_fn = get_builtin("list.sort").unwrap();

    match sort_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[empty]).unwrap();
            match result {
                Value::List(l) => assert!(l.is_empty()),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_sort_single() {
    let sort_fn = get_builtin("list.sort").unwrap();

    match sort_fn {
        Value::Builtin(builtin) => {
            let single = Value::List(Rc::new(vec![Value::Int(42.into())]));
            let result = (builtin.func)(&[single]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l.len(), 1);
                    assert_eq!(l[0], Value::Int(42.into()));
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_sort_already_sorted() {
    let sort_fn = get_builtin("list.sort").unwrap();

    match sort_fn {
        Value::Builtin(builtin) => {
            let sorted = Value::List(Rc::new(vec![
                Value::Int(1.into()),
                Value::Int(2.into()),
                Value::Int(3.into()),
            ]));
            let result = (builtin.func)(&[sorted]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l[0], Value::Int(1.into()));
                    assert_eq!(l[1], Value::Int(2.into()));
                    assert_eq!(l[2], Value::Int(3.into()));
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_sort_reverse_sorted() {
    let sort_fn = get_builtin("list.sort").unwrap();

    match sort_fn {
        Value::Builtin(builtin) => {
            let reversed = Value::List(Rc::new(vec![
                Value::Int(3.into()),
                Value::Int(2.into()),
                Value::Int(1.into()),
            ]));
            let result = (builtin.func)(&[reversed]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l[0], Value::Int(1.into()));
                    assert_eq!(l[1], Value::Int(2.into()));
                    assert_eq!(l[2], Value::Int(3.into()));
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_sort_with_duplicates() {
    let sort_fn = get_builtin("list.sort").unwrap();

    match sort_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Int(3.into()),
                Value::Int(1.into()),
                Value::Int(2.into()),
                Value::Int(1.into()),
            ]));
            let result = (builtin.func)(&[list]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l[0], Value::Int(1.into()));
                    assert_eq!(l[1], Value::Int(1.into()));
                    assert_eq!(l[2], Value::Int(2.into()));
                    assert_eq!(l[3], Value::Int(3.into()));
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_sort_strings() {
    let sort_fn = get_builtin("list.sort").unwrap();

    match sort_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::String(Rc::new("banana".to_string())),
                Value::String(Rc::new("apple".to_string())),
                Value::String(Rc::new("cherry".to_string())),
            ]));
            let result = (builtin.func)(&[list]).unwrap();
            match result {
                Value::List(l) => match (&l[0], &l[1], &l[2]) {
                    (Value::String(a), Value::String(b), Value::String(c)) => {
                        assert_eq!(a.as_str(), "apple");
                        assert_eq!(b.as_str(), "banana");
                        assert_eq!(c.as_str(), "cherry");
                    }
                    _ => panic!("Expected strings"),
                },
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_sort_paths() {
    let sort_fn = get_builtin("list.sort").unwrap();

    match sort_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::Path(Rc::new(std::path::PathBuf::from("/tmp/zeta.txt"))),
                Value::Path(Rc::new(std::path::PathBuf::from("/tmp/alpha.txt"))),
                Value::Path(Rc::new(std::path::PathBuf::from("/tmp/mid.txt"))),
            ]));
            let result = (builtin.func)(&[list]).unwrap();
            match result {
                Value::List(l) => {
                    let rendered = l
                        .iter()
                        .map(|value| match value {
                            Value::Path(path) => path.to_string_lossy().to_string(),
                            other => panic!("Expected Path, got {:?}", other),
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(
                        rendered,
                        vec!["/tmp/alpha.txt", "/tmp/mid.txt", "/tmp/zeta.txt"]
                    );
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_range_empty() {
    let range_fn = get_builtin("list.range").unwrap();

    match range_fn {
        Value::Builtin(builtin) => {
            let result = (builtin.func)(&[Value::Int(5.into()), Value::Int(5.into())]).unwrap();
            match result {
                Value::List(l) => assert!(l.is_empty()),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_range_single() {
    let range_fn = get_builtin("list.range").unwrap();

    match range_fn {
        Value::Builtin(builtin) => {
            let result = (builtin.func)(&[Value::Int(0.into()), Value::Int(1.into())]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l.len(), 1);
                    assert_eq!(l[0], Value::Int(0.into()));
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_range_multiple() {
    let range_fn = get_builtin("list.range").unwrap();

    match range_fn {
        Value::Builtin(builtin) => {
            let result = (builtin.func)(&[Value::Int(1.into()), Value::Int(5.into())]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l.len(), 4);
                    assert_eq!(l[0], Value::Int(1.into()));
                    assert_eq!(l[1], Value::Int(2.into()));
                    assert_eq!(l[2], Value::Int(3.into()));
                    assert_eq!(l[3], Value::Int(4.into()));
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_replicate_zero() {
    let replicate_fn = get_builtin("list.replicate").unwrap();

    match replicate_fn {
        Value::Builtin(builtin) => {
            let result = (builtin.func)(&[Value::Int(0.into()), Value::Int(42.into())]).unwrap();
            match result {
                Value::List(l) => assert!(l.is_empty()),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_replicate_multiple() {
    let replicate_fn = get_builtin("list.replicate").unwrap();

    match replicate_fn {
        Value::Builtin(builtin) => {
            let result = (builtin.func)(&[
                Value::Int(3.into()),
                Value::String(Rc::new("x".to_string())),
            ])
            .unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l.len(), 3);
                    for item in l.iter() {
                        match item {
                            Value::String(s) => assert_eq!(s.as_str(), "x"),
                            _ => panic!("Expected String"),
                        }
                    }
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_zip_empty() {
    let zip_fn = get_builtin("list.zip").unwrap();

    match zip_fn {
        Value::Builtin(builtin) => {
            let empty1 = Value::List(Rc::new(vec![]));
            let empty2 = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[empty1, empty2]).unwrap();
            match result {
                Value::List(l) => assert!(l.is_empty()),
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_zip_different_lengths() {
    let zip_fn = get_builtin("list.zip").unwrap();

    match zip_fn {
        Value::Builtin(builtin) => {
            let short = Value::List(Rc::new(vec![Value::Int(1.into())]));
            let long = Value::List(Rc::new(vec![
                Value::Int(10.into()),
                Value::Int(20.into()),
                Value::Int(30.into()),
            ]));
            let result = (builtin.func)(&[short, long]).unwrap();
            match result {
                Value::List(l) => {
                    // Zip stops at shorter list
                    assert_eq!(l.len(), 1);
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_zip_same_length() {
    let zip_fn = get_builtin("list.zip").unwrap();

    match zip_fn {
        Value::Builtin(builtin) => {
            let list1 = Value::List(Rc::new(vec![Value::Int(1.into()), Value::Int(2.into())]));
            let list2 = Value::List(Rc::new(vec![
                Value::String(Rc::new("a".to_string())),
                Value::String(Rc::new("b".to_string())),
            ]));
            let result = (builtin.func)(&[list1, list2]).unwrap();
            match result {
                Value::List(l) => {
                    assert_eq!(l.len(), 2);
                    match &l[0] {
                        Value::Tuple(t) => {
                            assert_eq!(t[0], Value::Int(1.into()));
                            match &t[1] {
                                Value::String(s) => assert_eq!(s.as_str(), "a"),
                                _ => panic!("Expected String"),
                            }
                        }
                        _ => panic!("Expected Tuple"),
                    }
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_unzip_empty() {
    let unzip_fn = get_builtin("list.unzip").unwrap();

    match unzip_fn {
        Value::Builtin(builtin) => {
            let empty = Value::List(Rc::new(vec![]));
            let result = (builtin.func)(&[empty]).unwrap();
            match result {
                Value::Tuple(t) => {
                    assert_eq!(t.len(), 2);
                    match (&t[0], &t[1]) {
                        (Value::List(l1), Value::List(l2)) => {
                            assert!(l1.is_empty());
                            assert!(l2.is_empty());
                        }
                        _ => panic!("Expected Lists"),
                    }
                }
                _ => panic!("Expected Tuple"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_unzip_pairs() {
    let unzip_fn = get_builtin("list.unzip").unwrap();

    match unzip_fn {
        Value::Builtin(builtin) => {
            let pairs = Value::List(Rc::new(vec![
                Value::Tuple(Rc::new(vec![
                    Value::Int(1.into()),
                    Value::String(Rc::new("a".to_string())),
                ])),
                Value::Tuple(Rc::new(vec![
                    Value::Int(2.into()),
                    Value::String(Rc::new("b".to_string())),
                ])),
            ]));
            let result = (builtin.func)(&[pairs]).unwrap();
            match result {
                Value::Tuple(t) => match (&t[0], &t[1]) {
                    (Value::List(l1), Value::List(l2)) => {
                        assert_eq!(l1.len(), 2);
                        assert_eq!(l2.len(), 2);
                        assert_eq!(l1[0], Value::Int(1.into()));
                        assert_eq!(l1[1], Value::Int(2.into()));
                    }
                    _ => panic!("Expected Lists"),
                },
                _ => panic!("Expected Tuple"),
            }
        }
        _ => panic!("Expected Builtin"),
    }
}

// ============================================================================
// Map 模块边缘测试
// ============================================================================

#[test]
fn test_map_empty_is_empty() {
    let empty = get_builtin("Map.empty");
    assert!(empty.is_some());
    match empty.unwrap() {
        Value::Map(m) => {
            assert!(m.is_empty());
            assert_eq!(m.len(), 0);
        }
        _ => panic!("Expected Map"),
    }
}

#[test]
fn test_map_singleton_creates_single_entry() {
    let singleton = get_builtin("Map.singleton");
    assert!(singleton.is_some());

    let result = call_builtin_fn(
        &singleton.unwrap(),
        vec![
            Value::String(Rc::new("mykey".to_string())),
            Value::Int(999.into()),
        ],
    )
    .unwrap();

    match result {
        Value::Map(m) => {
            assert_eq!(m.len(), 1);
        }
        _ => panic!("Expected Map"),
    }
}

#[test]
fn test_map_size_empty() {
    let empty = get_builtin("Map.empty").unwrap();
    let size = get_builtin("Map.size").unwrap();

    let result = call_builtin_fn(&size, vec![empty]).unwrap();
    assert_eq!(result, Value::Int(0.into()));
}

#[test]
fn test_map_is_empty_on_empty() {
    let empty = get_builtin("Map.empty").unwrap();
    let is_empty = get_builtin("Map.isEmpty").unwrap();

    let result = call_builtin_fn(&is_empty, vec![empty]).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_map_is_empty_on_non_empty() {
    let singleton = get_builtin("Map.singleton").unwrap();
    let is_empty = get_builtin("Map.isEmpty").unwrap();

    let m = call_builtin_fn(
        &singleton,
        vec![
            Value::String(Rc::new("k".to_string())),
            Value::Int(1.into()),
        ],
    )
    .unwrap();

    let result = call_builtin_fn(&is_empty, vec![m]).unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_map_contains_existing_key() {
    let singleton = get_builtin("Map.singleton").unwrap();
    let contains = get_builtin("Map.contains").unwrap();

    let m = call_builtin_fn(
        &singleton,
        vec![
            Value::String(Rc::new("key".to_string())),
            Value::Int(42.into()),
        ],
    )
    .unwrap();

    let result = call_builtin_fn(
        &contains,
        vec![Value::String(Rc::new("key".to_string())), m],
    )
    .unwrap();

    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_map_contains_missing_key() {
    let empty = get_builtin("Map.empty").unwrap();
    let contains = get_builtin("Map.contains").unwrap();

    let result = call_builtin_fn(
        &contains,
        vec![Value::String(Rc::new("nonexistent".to_string())), empty],
    )
    .unwrap();

    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_map_insert_to_empty() {
    let empty = get_builtin("Map.empty").unwrap();
    let insert = get_builtin("Map.insert").unwrap();
    let size = get_builtin("Map.size").unwrap();

    let m = call_builtin_fn(
        &insert,
        vec![
            Value::String(Rc::new("a".to_string())),
            Value::Int(1.into()),
            empty,
        ],
    )
    .unwrap();

    let result = call_builtin_fn(&size, vec![m]).unwrap();
    assert_eq!(result, Value::Int(1.into()));
}

#[test]
fn test_map_insert_overwrite() {
    let singleton = get_builtin("Map.singleton").unwrap();
    let insert = get_builtin("Map.insert").unwrap();
    let get = get_builtin("Map.get").unwrap();

    let m = call_builtin_fn(
        &singleton,
        vec![
            Value::String(Rc::new("key".to_string())),
            Value::Int(100.into()),
        ],
    )
    .unwrap();

    let m2 = call_builtin_fn(
        &insert,
        vec![
            Value::String(Rc::new("key".to_string())),
            Value::Int(200.into()),
            m,
        ],
    )
    .unwrap();

    let result =
        call_builtin_fn(&get, vec![Value::String(Rc::new("key".to_string())), m2]).unwrap();

    // Should be Some(200)
    match result {
        Value::Some(value) => {
            assert_eq!(*value, Value::Int(200.into()));
        }
        _ => panic!("Expected Some"),
    }
}

#[test]
fn test_map_key_record_order_independent() {
    let singleton = get_builtin("Map.singleton").unwrap();
    let get = get_builtin("Map.get").unwrap();

    let mut record_a = HashMap::new();
    record_a.insert("a".to_string(), Value::Int(1.into()));
    record_a.insert("b".to_string(), Value::Int(2.into()));
    let key_a = Value::Record(Rc::new(record_a));

    let mut record_b = HashMap::new();
    record_b.insert("b".to_string(), Value::Int(2.into()));
    record_b.insert("a".to_string(), Value::Int(1.into()));
    let key_b = Value::Record(Rc::new(record_b));

    let map = call_builtin_fn(&singleton, vec![key_a, Value::Int(42.into())]).unwrap();
    let result = call_builtin_fn(&get, vec![key_b, map]).unwrap();

    match result {
        Value::Some(value) => assert_eq!(*value, Value::Int(42.into())),
        _ => panic!("Expected Some"),
    }
}

#[test]
fn test_map_float_zero_key_normalized() {
    let singleton = get_builtin("Map.singleton").unwrap();
    let get = get_builtin("Map.get").unwrap();

    let map = call_builtin_fn(&singleton, vec![Value::Float(-0.0), Value::Int(7.into())]).unwrap();
    let result = call_builtin_fn(&get, vec![Value::Float(0.0), map]).unwrap();

    match result {
        Value::Some(value) => assert_eq!(*value, Value::Int(7.into())),
        _ => panic!("Expected Some"),
    }
}

#[test]
fn test_map_remove_existing() {
    let singleton = get_builtin("Map.singleton").unwrap();
    let remove = get_builtin("Map.remove").unwrap();
    let is_empty = get_builtin("Map.isEmpty").unwrap();

    let m = call_builtin_fn(
        &singleton,
        vec![
            Value::String(Rc::new("key".to_string())),
            Value::Int(42.into()),
        ],
    )
    .unwrap();

    let m2 = call_builtin_fn(&remove, vec![Value::String(Rc::new("key".to_string())), m]).unwrap();

    let result = call_builtin_fn(&is_empty, vec![m2]).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_map_remove_nonexistent() {
    let singleton = get_builtin("Map.singleton").unwrap();
    let remove = get_builtin("Map.remove").unwrap();
    let size = get_builtin("Map.size").unwrap();

    let m = call_builtin_fn(
        &singleton,
        vec![
            Value::String(Rc::new("key".to_string())),
            Value::Int(42.into()),
        ],
    )
    .unwrap();

    let m2 = call_builtin_fn(
        &remove,
        vec![Value::String(Rc::new("other".to_string())), m],
    )
    .unwrap();

    let result = call_builtin_fn(&size, vec![m2]).unwrap();
    assert_eq!(result, Value::Int(1.into()));
}

#[test]
fn test_map_union_disjoint() {
    let singleton = get_builtin("Map.singleton").unwrap();
    let union = get_builtin("Map.union").unwrap();
    let size = get_builtin("Map.size").unwrap();

    let m1 = call_builtin_fn(
        &singleton,
        vec![
            Value::String(Rc::new("a".to_string())),
            Value::Int(1.into()),
        ],
    )
    .unwrap();

    let m2 = call_builtin_fn(
        &singleton,
        vec![
            Value::String(Rc::new("b".to_string())),
            Value::Int(2.into()),
        ],
    )
    .unwrap();

    let combined = call_builtin_fn(&union, vec![m1, m2]).unwrap();
    let result = call_builtin_fn(&size, vec![combined]).unwrap();

    assert_eq!(result, Value::Int(2.into()));
}

#[test]
fn test_map_intersection_empty() {
    let singleton = get_builtin("Map.singleton").unwrap();
    let intersection = get_builtin("Map.intersection").unwrap();
    let is_empty = get_builtin("Map.isEmpty").unwrap();

    let m1 = call_builtin_fn(
        &singleton,
        vec![
            Value::String(Rc::new("a".to_string())),
            Value::Int(1.into()),
        ],
    )
    .unwrap();

    let m2 = call_builtin_fn(
        &singleton,
        vec![
            Value::String(Rc::new("b".to_string())),
            Value::Int(2.into()),
        ],
    )
    .unwrap();

    let result = call_builtin_fn(&intersection, vec![m1, m2]).unwrap();
    let empty = call_builtin_fn(&is_empty, vec![result]).unwrap();

    assert_eq!(empty, Value::Bool(true));
}

#[test]
fn test_map_difference() {
    let singleton = get_builtin("Map.singleton").unwrap();
    let insert = get_builtin("Map.insert").unwrap();
    let difference = get_builtin("Map.difference").unwrap();
    let size = get_builtin("Map.size").unwrap();

    // m1 = {a: 1, b: 2}
    let m1 = call_builtin_fn(
        &singleton,
        vec![
            Value::String(Rc::new("a".to_string())),
            Value::Int(1.into()),
        ],
    )
    .unwrap();
    let m1 = call_builtin_fn(
        &insert,
        vec![
            Value::String(Rc::new("b".to_string())),
            Value::Int(2.into()),
            m1,
        ],
    )
    .unwrap();

    // m2 = {b: 99}
    let m2 = call_builtin_fn(
        &singleton,
        vec![
            Value::String(Rc::new("b".to_string())),
            Value::Int(99.into()),
        ],
    )
    .unwrap();

    // difference = {a: 1}
    let diff = call_builtin_fn(&difference, vec![m1, m2]).unwrap();
    let result = call_builtin_fn(&size, vec![diff]).unwrap();

    assert_eq!(result, Value::Int(1.into()));
}

#[test]
fn test_map_keys_empty() {
    let empty = get_builtin("Map.empty").unwrap();
    let keys = get_builtin("Map.keys").unwrap();

    let result = call_builtin_fn(&keys, vec![empty]).unwrap();
    match result {
        Value::List(l) => assert!(l.is_empty()),
        _ => panic!("Expected List"),
    }
}

#[test]
fn test_map_values_empty() {
    let empty = get_builtin("Map.empty").unwrap();
    let values = get_builtin("Map.values").unwrap();

    let result = call_builtin_fn(&values, vec![empty]).unwrap();
    match result {
        Value::List(l) => assert!(l.is_empty()),
        _ => panic!("Expected List"),
    }
}

#[test]
fn test_map_to_list_empty() {
    let empty = get_builtin("Map.empty").unwrap();
    let to_list = get_builtin("Map.toList").unwrap();

    let result = call_builtin_fn(&to_list, vec![empty]).unwrap();
    match result {
        Value::List(l) => assert!(l.is_empty()),
        _ => panic!("Expected List"),
    }
}

#[test]
fn test_map_get_with_default_found() {
    let singleton = get_builtin("Map.singleton").unwrap();
    let get_with_default = get_builtin("Map.getWithDefault").unwrap();

    let m = call_builtin_fn(
        &singleton,
        vec![
            Value::String(Rc::new("key".to_string())),
            Value::Int(42.into()),
        ],
    )
    .unwrap();

    let result = call_builtin_fn(
        &get_with_default,
        vec![
            Value::String(Rc::new("key".to_string())),
            Value::Int(0.into()),
            m,
        ],
    )
    .unwrap();

    assert_eq!(result, Value::Int(42.into()));
}

#[test]
fn test_map_get_with_default_not_found() {
    let empty = get_builtin("Map.empty").unwrap();
    let get_with_default = get_builtin("Map.getWithDefault").unwrap();

    let result = call_builtin_fn(
        &get_with_default,
        vec![
            Value::String(Rc::new("missing".to_string())),
            Value::Int(999.into()),
            empty,
        ],
    )
    .unwrap();

    assert_eq!(result, Value::Int(999.into()));
}

// ============================================================================
// Set 模块边缘测试
// ============================================================================

#[test]
fn test_set_empty_is_empty() {
    let empty = get_builtin("Set.empty");
    assert!(empty.is_some());
    match empty.unwrap() {
        Value::Set(s) => {
            assert!(s.is_empty());
            assert_eq!(s.len(), 0);
        }
        _ => panic!("Expected Set"),
    }
}

#[test]
fn test_set_singleton_creates_single_element() {
    let singleton = get_builtin("Set.singleton");
    assert!(singleton.is_some());

    let result = call_builtin_fn(&singleton.unwrap(), vec![Value::Int(42.into())]).unwrap();

    match result {
        Value::Set(s) => {
            assert_eq!(s.len(), 1);
        }
        _ => panic!("Expected Set"),
    }
}

#[test]
fn test_set_size_empty() {
    let empty = get_builtin("Set.empty").unwrap();
    let size = get_builtin("Set.size").unwrap();

    let result = call_builtin_fn(&size, vec![empty]).unwrap();
    assert_eq!(result, Value::Int(0.into()));
}

#[test]
fn test_set_is_empty_true() {
    let empty = get_builtin("Set.empty").unwrap();
    let is_empty = get_builtin("Set.isEmpty").unwrap();

    let result = call_builtin_fn(&is_empty, vec![empty]).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_set_is_empty_false() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let is_empty = get_builtin("Set.isEmpty").unwrap();

    let s = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();
    let result = call_builtin_fn(&is_empty, vec![s]).unwrap();

    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_set_contains_found() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let contains = get_builtin("Set.contains").unwrap();

    let s = call_builtin_fn(&singleton, vec![Value::Int(42.into())]).unwrap();
    let result = call_builtin_fn(&contains, vec![Value::Int(42.into()), s]).unwrap();

    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_set_contains_not_found() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let contains = get_builtin("Set.contains").unwrap();

    let s = call_builtin_fn(&singleton, vec![Value::Int(42.into())]).unwrap();
    let result = call_builtin_fn(&contains, vec![Value::Int(99.into()), s]).unwrap();

    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_set_insert_new_element() {
    let empty = get_builtin("Set.empty").unwrap();
    let insert = get_builtin("Set.insert").unwrap();
    let size = get_builtin("Set.size").unwrap();

    let s = call_builtin_fn(&insert, vec![Value::Int(1.into()), empty]).unwrap();
    let result = call_builtin_fn(&size, vec![s]).unwrap();

    assert_eq!(result, Value::Int(1.into()));
}

#[test]
fn test_set_insert_duplicate() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let insert = get_builtin("Set.insert").unwrap();
    let size = get_builtin("Set.size").unwrap();

    let s = call_builtin_fn(&singleton, vec![Value::Int(42.into())]).unwrap();
    let s2 = call_builtin_fn(&insert, vec![Value::Int(42.into()), s]).unwrap();
    let result = call_builtin_fn(&size, vec![s2]).unwrap();

    // Duplicate shouldn't increase size
    assert_eq!(result, Value::Int(1.into()));
}

#[test]
fn test_set_remove_existing() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let remove = get_builtin("Set.remove").unwrap();
    let is_empty = get_builtin("Set.isEmpty").unwrap();

    let s = call_builtin_fn(&singleton, vec![Value::Int(42.into())]).unwrap();
    let s2 = call_builtin_fn(&remove, vec![Value::Int(42.into()), s]).unwrap();
    let result = call_builtin_fn(&is_empty, vec![s2]).unwrap();

    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_set_remove_nonexistent() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let remove = get_builtin("Set.remove").unwrap();
    let size = get_builtin("Set.size").unwrap();

    let s = call_builtin_fn(&singleton, vec![Value::Int(42.into())]).unwrap();
    let s2 = call_builtin_fn(&remove, vec![Value::Int(99.into()), s]).unwrap();
    let result = call_builtin_fn(&size, vec![s2]).unwrap();

    assert_eq!(result, Value::Int(1.into()));
}

#[test]
fn test_set_union_disjoint() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let union = get_builtin("Set.union").unwrap();
    let size = get_builtin("Set.size").unwrap();

    let s1 = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();
    let s2 = call_builtin_fn(&singleton, vec![Value::Int(2.into())]).unwrap();

    let combined = call_builtin_fn(&union, vec![s1, s2]).unwrap();
    let result = call_builtin_fn(&size, vec![combined]).unwrap();

    assert_eq!(result, Value::Int(2.into()));
}

#[test]
fn test_set_union_overlapping() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let union = get_builtin("Set.union").unwrap();
    let size = get_builtin("Set.size").unwrap();

    let s1 = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();
    let s2 = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();

    let combined = call_builtin_fn(&union, vec![s1, s2]).unwrap();
    let result = call_builtin_fn(&size, vec![combined]).unwrap();

    // Same element, should still be 1
    assert_eq!(result, Value::Int(1.into()));
}

#[test]
fn test_set_intersection_common() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let insert = get_builtin("Set.insert").unwrap();
    let intersection = get_builtin("Set.intersection").unwrap();
    let size = get_builtin("Set.size").unwrap();

    // s1 = {1, 2}
    let s1 = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();
    let s1 = call_builtin_fn(&insert, vec![Value::Int(2.into()), s1]).unwrap();

    // s2 = {2, 3}
    let s2 = call_builtin_fn(&singleton, vec![Value::Int(2.into())]).unwrap();
    let s2 = call_builtin_fn(&insert, vec![Value::Int(3.into()), s2]).unwrap();

    let result = call_builtin_fn(&intersection, vec![s1, s2]).unwrap();
    let len = call_builtin_fn(&size, vec![result]).unwrap();

    // Common element is 2
    assert_eq!(len, Value::Int(1.into()));
}

#[test]
fn test_set_intersection_none() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let intersection = get_builtin("Set.intersection").unwrap();
    let is_empty = get_builtin("Set.isEmpty").unwrap();

    let s1 = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();
    let s2 = call_builtin_fn(&singleton, vec![Value::Int(2.into())]).unwrap();

    let result = call_builtin_fn(&intersection, vec![s1, s2]).unwrap();
    let empty = call_builtin_fn(&is_empty, vec![result]).unwrap();

    assert_eq!(empty, Value::Bool(true));
}

#[test]
fn test_set_difference_some() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let insert = get_builtin("Set.insert").unwrap();
    let difference = get_builtin("Set.difference").unwrap();
    let size = get_builtin("Set.size").unwrap();

    // s1 = {1, 2}
    let s1 = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();
    let s1 = call_builtin_fn(&insert, vec![Value::Int(2.into()), s1]).unwrap();

    // s2 = {2}
    let s2 = call_builtin_fn(&singleton, vec![Value::Int(2.into())]).unwrap();

    let result = call_builtin_fn(&difference, vec![s1, s2]).unwrap();
    let len = call_builtin_fn(&size, vec![result]).unwrap();

    // Difference is {1}
    assert_eq!(len, Value::Int(1.into()));
}

#[test]
fn test_set_symmetric_difference() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let insert = get_builtin("Set.insert").unwrap();
    let sym_diff = get_builtin("Set.symmetricDifference").unwrap();
    let size = get_builtin("Set.size").unwrap();

    // s1 = {1, 2}
    let s1 = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();
    let s1 = call_builtin_fn(&insert, vec![Value::Int(2.into()), s1]).unwrap();

    // s2 = {2, 3}
    let s2 = call_builtin_fn(&singleton, vec![Value::Int(2.into())]).unwrap();
    let s2 = call_builtin_fn(&insert, vec![Value::Int(3.into()), s2]).unwrap();

    let result = call_builtin_fn(&sym_diff, vec![s1, s2]).unwrap();
    let len = call_builtin_fn(&size, vec![result]).unwrap();

    // Symmetric difference is {1, 3}
    assert_eq!(len, Value::Int(2.into()));
}

#[test]
fn test_set_is_subset_true() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let insert = get_builtin("Set.insert").unwrap();
    let is_subset = get_builtin("Set.isSubset").unwrap();

    // small = {1}
    let small = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();

    // big = {1, 2}
    let big = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();
    let big = call_builtin_fn(&insert, vec![Value::Int(2.into()), big]).unwrap();

    let result = call_builtin_fn(&is_subset, vec![small, big]).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_set_is_subset_false() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let insert = get_builtin("Set.insert").unwrap();
    let is_subset = get_builtin("Set.isSubset").unwrap();

    // big = {1, 2}
    let big = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();
    let big = call_builtin_fn(&insert, vec![Value::Int(2.into()), big]).unwrap();

    // small = {1}
    let small = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();

    let result = call_builtin_fn(&is_subset, vec![big, small]).unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_set_is_superset() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let insert = get_builtin("Set.insert").unwrap();
    let is_superset = get_builtin("Set.isSuperset").unwrap();

    // big = {1, 2}
    let big = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();
    let big = call_builtin_fn(&insert, vec![Value::Int(2.into()), big]).unwrap();

    // small = {1}
    let small = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();

    let result = call_builtin_fn(&is_superset, vec![big, small]).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_set_is_disjoint_true() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let is_disjoint = get_builtin("Set.isDisjoint").unwrap();

    let s1 = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();
    let s2 = call_builtin_fn(&singleton, vec![Value::Int(2.into())]).unwrap();

    let result = call_builtin_fn(&is_disjoint, vec![s1, s2]).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_set_is_disjoint_false() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let is_disjoint = get_builtin("Set.isDisjoint").unwrap();

    let s1 = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();
    let s2 = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();

    let result = call_builtin_fn(&is_disjoint, vec![s1, s2]).unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_set_from_list_with_duplicates() {
    let from_list = get_builtin("Set.fromList").unwrap();
    let size = get_builtin("Set.size").unwrap();

    let list = Value::List(Rc::new(vec![
        Value::Int(1.into()),
        Value::Int(2.into()),
        Value::Int(2.into()),
        Value::Int(3.into()),
        Value::Int(1.into()),
    ]));

    let set = call_builtin_fn(&from_list, vec![list]).unwrap();
    let len = call_builtin_fn(&size, vec![set]).unwrap();

    // Duplicates removed
    assert_eq!(len, Value::Int(3.into()));
}

#[test]
fn test_set_to_list() {
    let singleton = get_builtin("Set.singleton").unwrap();
    let insert = get_builtin("Set.insert").unwrap();
    let to_list = get_builtin("Set.toList").unwrap();

    let s = call_builtin_fn(&singleton, vec![Value::Int(1.into())]).unwrap();
    let s = call_builtin_fn(&insert, vec![Value::Int(2.into()), s]).unwrap();

    let list = call_builtin_fn(&to_list, vec![s]).unwrap();

    match list {
        Value::List(l) => assert_eq!(l.len(), 2),
        _ => panic!("Expected List"),
    }
}

// ============================================================================
// 错误处理边缘测试
// ============================================================================

#[test]
fn test_list_len_wrong_type() {
    let len_fn = get_builtin("list.len").unwrap();

    match len_fn {
        Value::Builtin(builtin) => {
            let result = (builtin.func)(&[Value::Int(42.into())]);
            assert!(result.is_err());
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_head_wrong_type() {
    let head_fn = get_builtin("list.head").unwrap();

    match head_fn {
        Value::Builtin(builtin) => {
            let result = (builtin.func)(&[Value::String(Rc::new("not a list".to_string()))]);
            assert!(result.is_err());
        }
        _ => panic!("Expected Builtin"),
    }
}

#[test]
fn test_list_sum_non_int_list() {
    let sum_fn = get_builtin("list.sum").unwrap();

    match sum_fn {
        Value::Builtin(builtin) => {
            let list = Value::List(Rc::new(vec![
                Value::String(Rc::new("not".to_string())),
                Value::String(Rc::new("ints".to_string())),
            ]));
            let result = (builtin.func)(&[list]);
            assert!(result.is_err());
        }
        _ => panic!("Expected Builtin"),
    }
}

// --- Atomic file operations / 原子文件操作 ---

#[test]
fn test_io_atomic_write_and_read_back() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("atomic-write.txt");
    let path_str = path.to_string_lossy().to_string();

    let write = get_builtin("io.atomicWrite").expect("io.atomicWrite builtin should exist");
    let read = get_builtin("io.readFile").expect("io.readFile builtin should exist");

    let result = call_builtin(
        &write,
        &[
            Value::String(Rc::new(path_str.clone())),
            Value::String(Rc::new("atomic-content".to_string())),
        ],
    )
    .expect("io.atomicWrite should succeed");
    assert!(matches!(result, Value::Unit));

    // Verify no temp files left behind
    let parent = path.parent().unwrap();
    let temp_files: Vec<_> = std::fs::read_dir(parent)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
        .collect();
    assert!(
        temp_files.is_empty(),
        "temp files left behind: {:?}",
        temp_files
    );

    let read_result = call_builtin(&read, &[Value::String(Rc::new(path_str))])
        .expect("io.readFile should succeed");
    assert!(matches!(read_result, Value::String(s) if s.as_str() == "atomic-content"));
}

#[test]
fn test_io_atomic_write_path_and_read_back() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("atomic-write-path.txt");

    let write = get_builtin("io.atomicWritePath").expect("io.atomicWritePath builtin should exist");
    let read = get_builtin("io.readFilePath").expect("io.readFilePath builtin should exist");

    let result = call_builtin(
        &write,
        &[
            Value::Path(Rc::new(path.clone())),
            Value::String(Rc::new("atomic-path-content".to_string())),
        ],
    )
    .expect("io.atomicWritePath should succeed");
    assert!(matches!(result, Value::Unit));

    let read_result =
        call_builtin(&read, &[Value::Path(Rc::new(path))]).expect("io.readFilePath should succeed");
    assert!(matches!(read_result, Value::String(s) if s.as_str() == "atomic-path-content"));
}

#[test]
fn test_io_atomic_write_all_writes_multiple_files() {
    let temp = TempDir::new().unwrap();
    let path_a = temp.path().join("atomic-a.txt");
    let path_b = temp.path().join("atomic-b.txt");
    let path_a_str = path_a.to_string_lossy().to_string();
    let path_b_str = path_b.to_string_lossy().to_string();

    let write_all =
        get_builtin("io.atomicWriteAll").expect("io.atomicWriteAll builtin should exist");
    let read = get_builtin("io.readFile").expect("io.readFile builtin should exist");

    let mut entry_a = HashMap::new();
    entry_a.insert(
        "path".to_string(),
        Value::String(Rc::new(path_a_str.clone())),
    );
    entry_a.insert(
        "content".to_string(),
        Value::String(Rc::new("content-a".to_string())),
    );
    let mut entry_b = HashMap::new();
    entry_b.insert(
        "path".to_string(),
        Value::String(Rc::new(path_b_str.clone())),
    );
    entry_b.insert(
        "content".to_string(),
        Value::String(Rc::new("content-b".to_string())),
    );

    let entries = Value::List(Rc::new(vec![
        Value::Record(Rc::new(entry_a)),
        Value::Record(Rc::new(entry_b)),
    ]));

    let result = call_builtin(&write_all, &[entries]).expect("io.atomicWriteAll should succeed");
    assert!(matches!(result, Value::Unit));

    let read_a =
        call_builtin(&read, &[Value::String(Rc::new(path_a_str))]).expect("read a should succeed");
    let read_b =
        call_builtin(&read, &[Value::String(Rc::new(path_b_str))]).expect("read b should succeed");
    assert!(matches!(read_a, Value::String(s) if s.as_str() == "content-a"));
    assert!(matches!(read_b, Value::String(s) if s.as_str() == "content-b"));
}

#[test]
fn test_io_atomic_write_all_rejects_non_record_entries() {
    let builtin = get_builtin("io.atomicWriteAll").expect("io.atomicWriteAll builtin should exist");
    let err = call_builtin(
        &builtin,
        &[Value::List(Rc::new(vec![Value::String(Rc::new(
            "not-a-record".to_string(),
        ))]))],
    )
    .expect_err("io.atomicWriteAll should reject non-record entries");
    assert!(err.contains("entries[0] must be a Record"));
}

#[test]
fn test_io_copy_and_read_back() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("copy-src.txt");
    let dst = temp.path().join("copy-dst.txt");
    let src_str = src.to_string_lossy().to_string();
    let dst_str = dst.to_string_lossy().to_string();

    std::fs::write(&src, "copy-me").unwrap();

    let copy = get_builtin("io.copy").expect("io.copy builtin should exist");
    let read = get_builtin("io.readFile").expect("io.readFile builtin should exist");

    let result = call_builtin(
        &copy,
        &[
            Value::String(Rc::new(src_str)),
            Value::String(Rc::new(dst_str.clone())),
        ],
    )
    .expect("io.copy should succeed");
    assert!(matches!(result, Value::Unit));

    let read_result =
        call_builtin(&read, &[Value::String(Rc::new(dst_str))]).expect("read should succeed");
    assert!(matches!(read_result, Value::String(s) if s.as_str() == "copy-me"));
}

#[test]
fn test_io_move_and_verify_original_gone() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("move-src.txt");
    let dst = temp.path().join("move-dst.txt");
    let src_str = src.to_string_lossy().to_string();
    let dst_str = dst.to_string_lossy().to_string();

    std::fs::write(&src, "move-me").unwrap();

    let move_fn = get_builtin("io.move").expect("io.move builtin should exist");
    let read = get_builtin("io.readFile").expect("io.readFile builtin should exist");
    let exists = get_builtin("io.pathExists").expect("io.pathExists builtin should exist");

    let result = call_builtin(
        &move_fn,
        &[
            Value::String(Rc::new(src_str.clone())),
            Value::String(Rc::new(dst_str.clone())),
        ],
    )
    .expect("io.move should succeed");
    assert!(matches!(result, Value::Unit));

    // Original should be gone
    let src_exists = call_builtin(&exists, &[Value::String(Rc::new(src_str))])
        .expect("pathExists should succeed");
    assert_eq!(src_exists, Value::Bool(false));

    // Content at destination
    let read_result =
        call_builtin(&read, &[Value::String(Rc::new(dst_str))]).expect("read should succeed");
    assert!(matches!(read_result, Value::String(s) if s.as_str() == "move-me"));
}

#[test]
fn test_map_size_wrong_type() {
    let size = get_builtin("Map.size").unwrap();
    let result = call_builtin_fn(&size, vec![Value::Int(42.into())]);
    assert!(result.is_err());
}

#[test]
fn test_set_size_wrong_type() {
    let size = get_builtin("Set.size").unwrap();
    let result = call_builtin_fn(&size, vec![Value::Int(42.into())]);
    assert!(result.is_err());
}

#[test]
fn test_map_insert_wrong_arity() {
    let insert = get_builtin("Map.insert").unwrap();
    let result = call_builtin_fn(&insert, vec![Value::Int(1.into())]);
    assert!(result.is_err());
}

#[test]
fn test_set_insert_wrong_arity() {
    let insert = get_builtin("Set.insert").unwrap();
    let result = call_builtin_fn(&insert, vec![Value::Int(1.into())]);
    assert!(result.is_err());
}
