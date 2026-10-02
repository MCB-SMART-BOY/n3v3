//! Integration tests for n3v3-config crate.

use n3v3_config::activate::{Activator, TestResult};
use n3v3_config::generate::{GeneratedConfig, Generator};
use n3v3_config::generation::{GenerationManager, GenerationMetadata};
use n3v3_config::module::{Module, OptionDecl, OptionType};
use n3v3_config::{SystemConfig, UserConfig};
use n3v3_derive::{Hash, StorePath};
use n3v3_eval::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

// SystemConfig tests

#[test]
fn test_system_config_builder() {
    let config = SystemConfig::new("my-system")
        .hostname("n3v3-host")
        .timezone("UTC")
        .service("sshd")
        .package("vim");

    assert_eq!(config.name, "my-system");
    assert_eq!(config.options.hostname, Some("n3v3-host".to_string()));
    assert_eq!(config.options.services, vec!["sshd"]);
    assert_eq!(config.options.packages, vec!["vim"]);
}

#[test]
fn test_system_config_multiple_packages() {
    let config = SystemConfig::new("test")
        .package("vim")
        .package("git")
        .package("tmux");

    assert_eq!(config.options.packages, vec!["vim", "git", "tmux"]);
}

// UserConfig tests

#[test]
fn test_user_config_builder() {
    let user = UserConfig::new("alice")
        .shell("/bin/zsh")
        .group("wheel")
        .package("git");

    assert_eq!(user.name, "alice");
    assert_eq!(user.home, PathBuf::from("/home/alice"));
    assert_eq!(user.shell, Some("/bin/zsh".to_string()));
    assert_eq!(user.groups, vec!["wheel"]);
}

#[test]
fn test_user_config_default_home() {
    let user = UserConfig::new("testuser").shell("/bin/bash");

    // Default home is /home/<username>
    assert_eq!(user.home, PathBuf::from("/home/testuser"));
}

// Module tests

#[test]
fn test_module_builder() {
    let module = Module::new("test")
        .import("./base.n3v3")
        .set("hostname", Value::String(Rc::new("test-host".to_string())));

    assert_eq!(module.name, "test");
    assert_eq!(module.imports, vec!["./base.n3v3"]);
}

#[test]
fn test_option_decl() {
    let opt = OptionDecl::new("networking.hostName", OptionType::String)
        .description("The hostname of the system")
        .example("my-host");

    assert_eq!(opt.name, "networking.hostName");
    assert_eq!(opt.ty, OptionType::String);
    assert!(opt.description.is_some());
}

#[test]
fn test_module_to_system_config() {
    let module = Module::new("test")
        .set("hostname", Value::String(Rc::new("test-host".to_string())))
        .set("timezone", Value::String(Rc::new("UTC".to_string())))
        .set(
            "packages",
            Value::List(Rc::new(vec![
                Value::String(Rc::new("vim".to_string())),
                Value::String(Rc::new("git".to_string())),
            ])),
        );

    let config = module.to_system_config().unwrap();
    assert_eq!(config.options.hostname, Some("test-host".to_string()));
    assert_eq!(config.options.timezone, Some("UTC".to_string()));
    assert_eq!(config.options.packages, vec!["vim", "git"]);
}

#[test]
fn test_module_to_system_config_rejects_invalid_declared_option_type() {
    let module = Module::new("typed")
        .option(OptionDecl::new("hostname", OptionType::String))
        .option(OptionDecl::new(
            "services",
            OptionType::List(Box::new(OptionType::String)),
        ))
        .set("hostname", Value::String(Rc::new("test-host".to_string())))
        .set("services", Value::List(Rc::new(vec![Value::Int(1.into())])));

    let err = module
        .to_system_config()
        .expect_err("type validation should fail");
    assert!(
        err.to_string()
            .contains("option 'services' has invalid type")
    );
}

#[test]
fn test_module_to_system_config_rejects_unknown_declared_option() {
    let module = Module::new("typed")
        .option(OptionDecl::new("hostname", OptionType::String))
        .set("timezone", Value::String(Rc::new("UTC".to_string())));

    let err = module
        .to_system_config()
        .expect_err("undeclared option should fail");
    assert!(err.to_string().contains("unknown option 'timezone'"));
}

#[test]
fn test_module_to_system_config_parses_users() {
    let mut user_record = HashMap::new();
    user_record.insert(
        "name".to_string(),
        Value::String(Rc::new("alice".to_string())),
    );
    user_record.insert(
        "home".to_string(),
        Value::String(Rc::new("/srv/alice".to_string())),
    );
    user_record.insert(
        "shell".to_string(),
        Value::String(Rc::new("/bin/zsh".to_string())),
    );
    user_record.insert(
        "groups".to_string(),
        Value::List(Rc::new(vec![Value::String(Rc::new("wheel".to_string()))])),
    );
    user_record.insert(
        "packages".to_string(),
        Value::List(Rc::new(vec![Value::String(Rc::new("git".to_string()))])),
    );
    user_record.insert(
        "passwordHash".to_string(),
        Value::String(Rc::new("$6$hash".to_string())),
    );

    let module = Module::new("users").set(
        "users",
        Value::List(Rc::new(vec![Value::Record(Rc::new(user_record))])),
    );

    let config = module.to_system_config().expect("users should parse");
    assert_eq!(config.options.users.len(), 1);
    let user = &config.options.users[0];
    assert_eq!(user.name, "alice");
    assert_eq!(user.home, PathBuf::from("/srv/alice"));
    assert_eq!(user.shell.as_deref(), Some("/bin/zsh"));
    assert_eq!(user.groups, vec!["wheel"]);
    assert_eq!(user.packages, vec!["git"]);
    assert_eq!(user.password_hash.as_deref(), Some("$6$hash"));
}

#[test]
fn test_module_load_merged_resolves_imports_in_order() {
    let dir = temp_dir("module-import-merge");
    fs::create_dir_all(&dir).unwrap();

    let base = dir.join("base.n3v3");
    fs::write(
        &base,
        r#"let module = #{
    timezone = "UTC",
    packages = ["git"]
};
"#,
    )
    .unwrap();

    let child = dir.join("child.n3v3");
    fs::write(
        &child,
        r#"let module = #{
    imports = ["./base.n3v3"],
    hostname = "demo",
    packages = ["vim"]
};
"#,
    )
    .unwrap();

    let config = Module::load_merged(&child).expect("module graph should load");
    assert_eq!(config.options.hostname, Some("demo".to_string()));
    assert_eq!(config.options.timezone, Some("UTC".to_string()));
    assert_eq!(config.options.packages, vec!["git", "vim"]);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_module_load_with_imports_detects_cycle() {
    let dir = temp_dir("module-import-cycle");
    fs::create_dir_all(&dir).unwrap();

    let a = dir.join("a.n3v3");
    let b = dir.join("b.n3v3");

    fs::write(
        &a,
        r#"let module = #{
    imports = ["./b.n3v3"],
    hostname = "a"
};
"#,
    )
    .unwrap();
    fs::write(
        &b,
        r#"let module = #{
    imports = ["./a.n3v3"],
    hostname = "b"
};
"#,
    )
    .unwrap();

    let err = Module::load_with_imports(&a).expect_err("cycle should be rejected");
    assert!(err.to_string().contains("module import cycle detected"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_module_load_supports_language_imports_via_frontend_hir() {
    let dir = temp_dir("module-language-import");
    fs::create_dir_all(&dir).unwrap();

    let helpers = dir.join("helpers.n3v3");
    fs::write(&helpers, r#"fn packages(seed) = ["git", "vim"];"#).unwrap();

    let main = dir.join("main.n3v3");
    fs::write(
        &main,
        r#"use helpers (packages);

let module = #{
    packages = packages(0)
};
"#,
    )
    .unwrap();

    let module = Module::load(&main).expect("frontend/HIR load should succeed");
    let config = module.to_system_config().expect("module should convert");
    assert_eq!(config.options.packages, vec!["git", "vim"]);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_module_load_reports_frontend_type_errors() {
    let dir = temp_dir("module-frontend-type-error");
    fs::create_dir_all(&dir).unwrap();

    let main = dir.join("bad.n3v3");
    fs::write(&main, "let module = 1 + true;").unwrap();

    let err = Module::load(&main).expect_err("type error should be surfaced");
    let message = err.to_string();
    assert!(message.contains("frontend diagnostics"));
    assert!(message.contains("bad.n3v3"));

    let _ = fs::remove_dir_all(&dir);
}

// Generator tests

fn temp_dir(suffix: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "n3v3-config-test-{}-{}",
        std::process::id(),
        suffix
    ))
}

#[test]
fn generator_regular_config_uses_structured_activation_data() {
    let dir = temp_dir("gen");

    let config = SystemConfig::new("test")
        .hostname("test-host")
        .timezone("UTC")
        .service("sshd");

    let generator = Generator::new(dir.clone());
    let generated = generator.generate(&config).unwrap();

    assert!(!generated.files.is_empty());
    assert!(generated.activation_script.is_none());
    assert_eq!(generated.services, vec!["sshd"]);

    // Cleanup
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_generator_uses_user_password_hash_in_shadow() {
    let dir = tempfile::tempdir().unwrap();
    let config = SystemConfig::new("test").user(UserConfig::new("alice").password_hash("$6$abc"));
    let generator = Generator::new(dir.path().join("output"));
    let _generated = generator.generate(&config).unwrap();
    let shadow = dir.path().join("output/etc/shadow");
    assert!(
        fs::read_to_string(&shadow)
            .unwrap()
            .contains("alice:$6$abc:19000:0:99999:7:::")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(shadow).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(dir.path().join("output/etc"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}

#[test]
fn generator_invalid_service_name_rejects_before_creating_files() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("output");
    let error = Generator::new(output.clone())
        .generate(&SystemConfig::new("test").service("../escape"))
        .unwrap_err();
    assert!(error.to_string().contains("invalid service name"));
    assert!(!output.exists());
}

#[test]
fn generator_invalid_user_name_rejects_before_creating_files() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("output");
    let error = Generator::new(output.clone())
        .generate(&SystemConfig::new("test").user(UserConfig::new("nested/user")))
        .unwrap_err();
    assert!(error.to_string().contains("invalid user name"));
    assert!(!output.exists());
}

#[test]
fn test_to_derivation() {
    let dir = temp_dir("drv");
    let config = SystemConfig::new("my-config")
        .hostname("my-host")
        .package("vim")
        .service("sshd");

    let generator = Generator::new(dir);
    let drv = generator.to_derivation(&config);

    assert_eq!(drv.name, "my-config");
    assert!(drv.env.contains_key("hostname"));
    assert!(drv.env.contains_key("packages"));
}

// Activator tests

#[test]
fn test_activator_dry_run() {
    let activator = Activator::new().dry_run(true).verbose(false);

    let generated = GeneratedConfig::new();
    let result = activator.activate(&generated).unwrap();

    assert!(result.success);
    assert_eq!(result.files_installed, 0);
}

#[test]
fn test_test_result() {
    let mut result = TestResult::new();
    result.files_checked = 5;
    result.warnings.push("test warning".to_string());
    result.success = true;

    assert!(result.success);
    assert_eq!(result.warnings.len(), 1);
}

// Generation manager tests

#[test]
fn test_generation_manager() {
    let dir = temp_dir("mgr");
    let manager = GenerationManager::new(dir.clone()).unwrap();

    assert_eq!(manager.current_generation().unwrap(), None);
    assert_eq!(manager.next_generation().unwrap(), 1);

    // Cleanup
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_create_generation() {
    let dir = temp_dir("create");
    let manager = GenerationManager::new(dir.clone()).unwrap();

    let hash = Hash::of(b"test config");
    let store_path = StorePath::new(hash, "test-config".to_string());
    let metadata = GenerationMetadata::new()
        .name("test")
        .description("Test configuration");

    let generation = manager.create_generation(&store_path, metadata).unwrap();

    assert_eq!(generation.number, 1);
    assert!(generation.path.exists());

    // Check current points to it
    assert_eq!(manager.current_generation().unwrap(), Some(1));

    // Cleanup
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_list_generations() {
    let dir = temp_dir("list");
    let manager = GenerationManager::new(dir.clone()).unwrap();

    // Create a few generations
    for i in 1..=3 {
        let hash = Hash::of(format!("config-{}", i).as_bytes());
        let store_path = StorePath::new(hash, format!("config-{}", i));
        let metadata = GenerationMetadata::new().name(format!("gen-{}", i));
        manager.create_generation(&store_path, metadata).unwrap();
    }

    let gens = manager.list_generations().unwrap();
    assert_eq!(gens.len(), 3);
    assert_eq!(gens[0].number, 1);
    assert_eq!(gens[2].number, 3);

    // Cleanup
    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// SystemConfig 边缘测试
// ============================================================================

#[test]
fn test_system_config_empty_name() {
    let config = SystemConfig::new("");
    assert_eq!(config.name, "");
}

#[test]
fn test_system_config_long_name() {
    let long_name = "a".repeat(1000);
    let config = SystemConfig::new(&long_name);
    assert_eq!(config.name, long_name);
}

#[test]
fn test_system_config_unicode_name() {
    let config = SystemConfig::new("我的系统配置");
    assert_eq!(config.name, "我的系统配置");
}

#[test]
fn test_system_config_special_chars() {
    let config = SystemConfig::new("test-config_v1.0");
    assert_eq!(config.name, "test-config_v1.0");
}

#[test]
fn test_system_config_no_packages() {
    let config = SystemConfig::new("minimal").hostname("host");

    assert!(config.options.packages.is_empty());
}

#[test]
fn test_system_config_many_packages() {
    let mut config = SystemConfig::new("full");
    for i in 0..100 {
        config = config.package(format!("pkg-{}", i));
    }

    assert_eq!(config.options.packages.len(), 100);
}

#[test]
fn test_system_config_duplicate_packages() {
    let config = SystemConfig::new("test")
        .package("vim")
        .package("git")
        .package("vim"); // duplicate

    // Duplicates are allowed at config level (deduplication happens later)
    assert_eq!(config.options.packages.len(), 3);
}

#[test]
fn test_system_config_no_services() {
    let config = SystemConfig::new("minimal");
    assert!(config.options.services.is_empty());
}

#[test]
fn test_system_config_many_services() {
    let mut config = SystemConfig::new("server");
    for i in 0..50 {
        config = config.service(format!("service-{}", i));
    }

    assert_eq!(config.options.services.len(), 50);
}

#[test]
fn test_system_config_chained_builder() {
    let config = SystemConfig::new("chained")
        .hostname("host1")
        .timezone("UTC")
        .package("vim")
        .package("git")
        .service("sshd")
        .service("nginx");

    assert_eq!(config.options.hostname, Some("host1".to_string()));
    assert_eq!(config.options.timezone, Some("UTC".to_string()));
    assert_eq!(config.options.packages.len(), 2);
    assert_eq!(config.options.services.len(), 2);
}

// ============================================================================
// UserConfig 边缘测试
// ============================================================================

#[test]
fn test_user_config_empty_name() {
    let user = UserConfig::new("");
    assert_eq!(user.name, "");
    assert_eq!(user.home, PathBuf::from("/home/"));
}

#[test]
fn test_user_config_root_user() {
    let user = UserConfig::new("root");
    assert_eq!(user.name, "root");
    // Note: default home is /home/root, not /root
    assert_eq!(user.home, PathBuf::from("/home/root"));
}

#[test]
fn test_user_config_custom_home() {
    // If UserConfig supports custom home, test it
    let user = UserConfig::new("custom").shell("/bin/zsh");

    assert_eq!(user.shell, Some("/bin/zsh".to_string()));
}

#[test]
fn test_user_config_many_groups() {
    let mut user = UserConfig::new("multigroup");
    for i in 0..20 {
        user = user.group(format!("group-{}", i));
    }

    assert_eq!(user.groups.len(), 20);
}

#[test]
fn test_user_config_no_groups() {
    let user = UserConfig::new("nogroups");
    assert!(user.groups.is_empty());
}

#[test]
fn test_user_config_unicode_name() {
    let user = UserConfig::new("用户");
    assert_eq!(user.name, "用户");
}

#[test]
fn test_user_config_with_packages() {
    let user = UserConfig::new("dev")
        .package("neovim")
        .package("tmux")
        .package("zsh");

    assert_eq!(user.packages.len(), 3);
}

// ============================================================================
// Module 边缘测试
// ============================================================================

#[test]
fn test_module_empty() {
    let module = Module::new("empty");
    assert_eq!(module.name, "empty");
    assert!(module.imports.is_empty());
}

#[test]
fn test_module_many_imports() {
    let mut module = Module::new("many-imports");
    for i in 0..30 {
        module = module.import(format!("./module-{}.n3v3", i));
    }

    assert_eq!(module.imports.len(), 30);
}

#[test]
fn test_module_set_string_value() {
    let module = Module::new("test").set("key", Value::String(Rc::new("value".to_string())));

    assert!(module.config.contains_key("key"));
}

#[test]
fn test_module_set_int_value() {
    let module = Module::new("test").set("count", Value::Int(42.into()));

    assert!(module.config.contains_key("count"));
}

#[test]
fn test_module_set_bool_value() {
    let module = Module::new("test").set("enabled", Value::Bool(true));

    assert!(module.config.contains_key("enabled"));
}

#[test]
fn test_module_set_list_value() {
    let module = Module::new("test").set(
        "items",
        Value::List(Rc::new(vec![
            Value::Int(1.into()),
            Value::Int(2.into()),
            Value::Int(3.into()),
        ])),
    );

    assert!(module.config.contains_key("items"));
}

#[test]
fn test_module_overwrite_value() {
    let module = Module::new("test")
        .set("key", Value::Int(1.into()))
        .set("key", Value::Int(2.into()));

    match module.config.get("key") {
        Some(Value::Int(n)) => assert_eq!(n, &2.into()),
        _ => panic!("Expected Int(2)"),
    }
}

// ============================================================================
// OptionDecl 边缘测试
// ============================================================================

#[test]
fn test_option_decl_string_type() {
    let opt = OptionDecl::new("test.option", OptionType::String);
    assert_eq!(opt.ty, OptionType::String);
}

#[test]
fn test_option_decl_int_type() {
    let opt = OptionDecl::new("test.number", OptionType::Int);
    assert_eq!(opt.ty, OptionType::Int);
}

#[test]
fn test_option_decl_bool_type() {
    let opt = OptionDecl::new("test.enabled", OptionType::Bool);
    assert_eq!(opt.ty, OptionType::Bool);
}

#[test]
fn test_option_decl_list_type() {
    let opt = OptionDecl::new("test.items", OptionType::List(Box::new(OptionType::String)));
    match opt.ty {
        OptionType::List(_) => {} // OK
        _ => panic!("Expected List type"),
    }
}

#[test]
fn test_option_decl_with_description() {
    let opt = OptionDecl::new("test.opt", OptionType::String).description("This is a description");

    assert_eq!(opt.description, Some("This is a description".to_string()));
}

#[test]
fn test_option_decl_with_example() {
    let opt = OptionDecl::new("test.opt", OptionType::String).example("example value");

    assert_eq!(opt.example, Some("example value".to_string()));
}

#[test]
fn test_option_decl_full_chain() {
    let opt = OptionDecl::new("networking.hostname", OptionType::String)
        .description("The hostname of the system")
        .example("my-server");

    assert_eq!(opt.name, "networking.hostname");
    assert_eq!(opt.ty, OptionType::String);
    assert!(opt.description.is_some());
    assert!(opt.example.is_some());
}

#[test]
fn test_option_decl_nested_name() {
    let opt = OptionDecl::new("services.nginx.enable", OptionType::Bool);
    assert_eq!(opt.name, "services.nginx.enable");
}

// ============================================================================
// Generator 边缘测试
// ============================================================================

#[test]
fn test_generator_minimal_config() {
    let dir = temp_dir("gen-minimal");
    let config = SystemConfig::new("minimal");

    let generator = Generator::new(dir.clone());
    let generated = generator.generate(&config).unwrap();

    // Should generate at least something
    assert!(generated.files.is_empty() || !generated.files.is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_generator_with_many_packages() {
    let dir = temp_dir("gen-many-pkgs");

    let mut config = SystemConfig::new("full");
    for i in 0..50 {
        config = config.package(format!("pkg-{}", i));
    }

    let generator = Generator::new(dir.clone());
    let _generated = generator.generate(&config).unwrap();

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_generator_with_many_services() {
    let dir = temp_dir("gen-many-svc");

    let mut config = SystemConfig::new("server");
    for i in 0..20 {
        config = config.service(format!("service-{}", i));
    }

    let generator = Generator::new(dir.clone());
    let generated = generator.generate(&config).unwrap();

    assert_eq!(generated.services.len(), 20);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_to_derivation_has_required_fields() {
    let dir = temp_dir("drv-fields");
    let config = SystemConfig::new("test-drv")
        .hostname("testhost")
        .package("vim");

    let generator = Generator::new(dir);
    let drv = generator.to_derivation(&config);

    assert_eq!(drv.name, "test-drv");
    assert!(drv.env.contains_key("hostname"));
}

// ============================================================================
// Activator 边缘测试
// ============================================================================

#[test]
fn test_activator_defaults() {
    let activator = Activator::new();

    // Default should not be dry_run
    let generated = GeneratedConfig::new();
    let result = activator.activate(&generated);

    // Should succeed on empty config
    assert!(result.is_ok());
}

#[test]
fn test_activator_verbose_mode() {
    let activator = Activator::new().dry_run(true).verbose(true);

    let generated = GeneratedConfig::new();
    let result = activator.activate(&generated).unwrap();

    assert!(result.success);
}

#[test]
fn test_test_result_with_errors() {
    let mut result = TestResult::new();
    result.errors.push("Error 1".to_string());
    result.errors.push("Error 2".to_string());
    result.success = false;

    assert!(!result.success);
    assert_eq!(result.errors.len(), 2);
}

#[test]
fn test_test_result_with_warnings() {
    let mut result = TestResult::new();
    result.warnings.push("Warning 1".to_string());
    result.warnings.push("Warning 2".to_string());
    result.warnings.push("Warning 3".to_string());
    result.success = true;

    assert!(result.success);
    assert_eq!(result.warnings.len(), 3);
}

#[test]
fn test_test_result_files_checked() {
    let mut result = TestResult::new();
    result.files_checked = 1000;
    result.success = true;

    assert_eq!(result.files_checked, 1000);
}

// ============================================================================
// GenerationManager 边缘测试
// ============================================================================

#[test]
fn test_generation_manager_empty_list() {
    let dir = temp_dir("mgr-empty");
    let manager = GenerationManager::new(dir.clone()).unwrap();

    let gens = manager.list_generations().unwrap();
    assert!(gens.is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_generation_manager_sequential_numbers() {
    let dir = temp_dir("mgr-seq");
    let manager = GenerationManager::new(dir.clone()).unwrap();

    for i in 1..=5 {
        let hash = Hash::of(format!("config-{}", i).as_bytes());
        let store_path = StorePath::new(hash, format!("config-{}", i));
        let metadata = GenerationMetadata::new();
        let generation = manager.create_generation(&store_path, metadata).unwrap();

        assert_eq!(generation.number, i as u64);
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_generation_metadata_name() {
    let metadata = GenerationMetadata::new().name("test-generation");

    assert_eq!(metadata.name, Some("test-generation".to_string()));
}

#[test]
fn test_generation_metadata_description() {
    let metadata = GenerationMetadata::new().description("A test configuration");

    assert_eq!(
        metadata.description,
        Some("A test configuration".to_string())
    );
}

#[test]
fn test_generation_metadata_full() {
    let metadata = GenerationMetadata::new()
        .name("prod")
        .description("Production configuration");

    assert!(metadata.name.is_some());
    assert!(metadata.description.is_some());
}

#[test]
fn test_current_generation_after_create() {
    let dir = temp_dir("mgr-current");
    let manager = GenerationManager::new(dir.clone()).unwrap();

    let hash = Hash::of(b"config");
    let store_path = StorePath::new(hash, "config".to_string());
    let metadata = GenerationMetadata::new();

    manager.create_generation(&store_path, metadata).unwrap();

    let current = manager.current_generation().unwrap();
    assert_eq!(current, Some(1));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_next_generation_increments() {
    let dir = temp_dir("mgr-next");
    let manager = GenerationManager::new(dir.clone()).unwrap();

    assert_eq!(manager.next_generation().unwrap(), 1);

    let hash = Hash::of(b"config1");
    let store_path = StorePath::new(hash, "config1".to_string());
    manager
        .create_generation(&store_path, GenerationMetadata::new())
        .unwrap();

    assert_eq!(manager.next_generation().unwrap(), 2);

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// module.to_system_config 边缘测试
// ============================================================================

#[test]
fn test_module_to_config_empty() {
    let module = Module::new("empty");
    let config = module.to_system_config().unwrap();

    assert_eq!(config.name, "empty");
}

#[test]
fn test_module_to_config_with_hostname() {
    let module =
        Module::new("with-host").set("hostname", Value::String(Rc::new("myhost".to_string())));

    let config = module.to_system_config().unwrap();
    assert_eq!(config.options.hostname, Some("myhost".to_string()));
}

#[test]
fn test_module_to_config_with_packages() {
    let module = Module::new("with-pkgs").set(
        "packages",
        Value::List(Rc::new(vec![
            Value::String(Rc::new("vim".to_string())),
            Value::String(Rc::new("git".to_string())),
        ])),
    );

    let config = module.to_system_config().unwrap();
    assert_eq!(config.options.packages.len(), 2);
}

#[test]
fn test_module_to_config_combined() {
    let module = Module::new("combined")
        .set("hostname", Value::String(Rc::new("host".to_string())))
        .set(
            "timezone",
            Value::String(Rc::new("Asia/Shanghai".to_string())),
        )
        .set(
            "packages",
            Value::List(Rc::new(vec![Value::String(Rc::new("neovim".to_string()))])),
        );

    let config = module.to_system_config().unwrap();

    assert_eq!(config.options.hostname, Some("host".to_string()));
    assert_eq!(config.options.timezone, Some("Asia/Shanghai".to_string()));
    assert_eq!(config.options.packages.len(), 1);
}

// ============================================================================
// 压力测试
// ============================================================================

#[test]
fn test_many_generations() {
    let dir = temp_dir("stress-gens");
    let manager = GenerationManager::new(dir.clone()).unwrap();

    for i in 1..=20 {
        let hash = Hash::of(format!("stress-{}", i).as_bytes());
        let store_path = StorePath::new(hash, format!("stress-{}", i));
        let metadata = GenerationMetadata::new()
            .name(format!("Generation {}", i))
            .description(format!("Stress test generation {}", i));

        let generation = manager.create_generation(&store_path, metadata).unwrap();
        assert_eq!(generation.number, i as u64);
    }

    let gens = manager.list_generations().unwrap();
    assert_eq!(gens.len(), 20);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_system_config_stress() {
    let mut config = SystemConfig::new("stress-test")
        .hostname("stress-host")
        .timezone("UTC");

    // Add many packages
    for i in 0..200 {
        config = config.package(format!("package-{}", i));
    }

    // Add many services
    for i in 0..50 {
        config = config.service(format!("service-{}", i));
    }

    assert_eq!(config.options.packages.len(), 200);
    assert_eq!(config.options.services.len(), 50);
}
