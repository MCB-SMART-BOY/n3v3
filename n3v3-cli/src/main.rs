//! n3v3 CLI - The n3v3 language command line interface.
//! n3v3 CLI - n3v3 语言的命令行界面。

mod commands;
mod output;
mod platform;
mod registry_client;

use clap::{Parser, Subcommand};

/// Main CLI structure.
/// 主 CLI 结构体。
#[derive(Parser)]
#[command(name = "n3v3")]
#[command(
    author,
    version,
    about = "n3v3 - A standalone language for system configuration and structured shell automation",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output. / 启用详细输出。
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Suppress the final error message line; exit codes are unchanged.
    /// 抑制最后一行错误信息；退出码不变。
    #[arg(short, long, global = true)]
    quiet: bool,
}

/// Available CLI commands.
/// 可用的 CLI 命令。
#[derive(Subcommand)]
enum Commands {
    /// Evaluate an expression. / 求值表达式。
    Eval {
        /// The expression to evaluate. / 要求值的表达式。
        expr: String,
    },

    /// Run tests in a directory. / 运行目录中的测试。
    Test {
        /// Directory to find tests in. / 查找测试的目录。
        #[arg(default_value = ".")]
        dir: String,
    },

    /// Run an n3v3 file. / 运行 n3v3 文件。
    Run {
        /// The file to run. / 要运行的文件。
        file: String,

        /// Arguments to pass to the script (accessible via io.args()).
        /// 传递给脚本的参数（通过 io.args() 访问）。
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Type check a file. Effect checking is on by default.
    /// 类型检查文件。默认启用副作用检查。
    Check {
        /// The file to check. / 要检查的文件。
        file: String,

        /// Allow effectful operations (disable purity enforcement).
        /// 允许副作用操作（禁用纯度检查）。
        #[arg(long)]
        allow_effects: bool,
    },

    /// Format a file or directory. / 格式化文件或目录。
    Fmt {
        #[command(subcommand)]
        action: FmtAction,
    },

    /// Setup editor integration (Helix, VS Code, etc).
    /// 设置编辑器集成（Helix、VS Code 等）。
    Setup {
        /// Editor to setup for.
        /// 要设置的编辑器。
        #[arg(default_value = "helix")]
        editor: String,
    },

    /// Start the Language Server Protocol server.
    /// 启动语言服务器协议服务。
    Lsp {
        /// Run a health check instead of starting the server.
        /// 运行健康检查而不是启动服务器。
        #[arg(long)]
        check: bool,

        /// Print LSP version and capabilities.
        /// 打印 LSP 版本和能力。
        #[arg(long)]
        version: bool,
    },

    /// Start an interactive REPL. / 启动交互式 REPL。
    Repl,

    /// View documentation (like man pages). / 查看文档（类似 man 手册）。
    Doc {
        /// Topic to view (index, quickstart, tutorial, spec, api, diagnostics, philosophy, install, architecture, onboarding, contributing, feature-matrix, lsp, stability, ecosystem-design, registry, changelog).
        /// 要查看的主题（index, quickstart, tutorial, spec, api, diagnostics, philosophy, install, architecture, onboarding, contributing, feature-matrix, lsp, stability, ecosystem-design, registry, changelog）。
        topic: Option<String>,

        /// List all available topics. / 列出所有可用主题。
        #[arg(long, short)]
        list: bool,
    },

    /// Initialize a new n3v3 project in the given directory.
    /// 在给定目录中初始化新的 n3v3 项目。
    Init {
        /// Directory to create the project in. / 要创建项目的目录。
        dir: String,
    },

    /// Print extended documentation for an error code. / 打印错误代码的扩展文档。
    Explain {
        /// The error code to explain (e.g., E0200, E0300).
        code: String,
    },

    /// Print version information. / 打印版本信息。
    Version,

    /// Build a package (Unix only). / 构建软件包（仅限 Unix）。
    #[cfg(unix)]
    Build {
        /// Package name or path. / 软件包名称或路径。
        package: Option<String>,

        /// Build backend (native, docker, simple). / 构建后端（native, docker, simple）。
        #[arg(long, default_value = "auto")]
        backend: String,

        /// Binary cache URL (repeatable). / 二进制缓存 URL（可重复）。
        #[arg(long = "cache-url", value_name = "URL")]
        cache_urls: Vec<String>,

        /// Local binary cache directory (repeatable). / 本地二进制缓存目录（可重复）。
        #[arg(long = "cache-dir", value_name = "DIR")]
        cache_dirs: Vec<String>,

        /// Disable substituter downloads. / 禁用 substituter 下载。
        #[arg(long = "no-substitute")]
        no_substitute: bool,

        /// Upload successful outputs to writable caches. / 将成功产物上传到可写缓存。
        #[arg(long = "cache-upload")]
        cache_upload: bool,

        /// Public key for narinfo signature verification (`ed25519:<base64>`, repeatable).
        /// narinfo 签名验证公钥（`ed25519:<base64>`，可重复）。
        #[arg(long = "cache-public-key", value_name = "KEY")]
        cache_public_keys: Vec<String>,

        /// Private key for narinfo signing on upload (`ed25519:<base64>`, repeatable).
        /// 上传时用于 narinfo 签名的私钥（`ed25519:<base64>`，可重复）。
        #[arg(long = "cache-private-key", value_name = "KEY")]
        cache_private_keys: Vec<String>,
    },

    /// Package management commands (Unix only). / 软件包管理命令（仅限 Unix）。
    #[cfg(unix)]
    Package {
        #[command(subcommand)]
        action: PackageAction,
    },

    /// Search for packages (Unix only). / 搜索软件包（仅限 Unix）。
    #[cfg(unix)]
    Search {
        /// Search query. / 搜索查询。
        query: String,
    },

    /// Show package or platform information. / 显示软件包或平台信息。
    Info {
        /// Package name (Unix only). / 软件包名称（仅限 Unix）。
        package: Option<String>,

        /// Show platform capabilities. / 显示平台功能。
        #[arg(long, short = 'p')]
        platform: bool,
    },

    /// Update dependencies (Unix only). / 更新依赖（仅限 Unix）。
    #[cfg(unix)]
    Update,

    /// Update the package index from a remote registry (Unix only). / 从远程注册表更新软件包索引（仅限 Unix）。
    #[cfg(unix)]
    RegistryUpdate {
        /// Registry URL (defaults to $N3V3_REGISTRY or https://registry.n3v3.dev/packages.json).
        registry_url: Option<String>,
    },

    /// Start a local package registry server (Unix only).
    /// 启动本地软件包注册服务器（仅限 Unix）。
    #[cfg(unix)]
    RegistryServe {
        /// Directory containing package data.
        /// 包含软件包数据的目录。
        #[arg(default_value = "./registry-data")]
        dir: String,
        /// Address to listen on. Only loopback addresses are accepted.
        /// 要监听的地址。仅接受回环地址。
        #[arg(long, default_value = "127.0.0.1")]
        host: std::net::IpAddr,
        /// Port to listen on.
        /// 要监听的端口。
        #[arg(long, default_value = "8080")]
        port: u16,
    },

    /// Publish a package to a registry (Unix only).
    /// 将软件包发布到注册表（仅限 Unix）。
    #[cfg(unix)]
    RegistryPublish {
        /// Directory containing flake.n3v3 or package.n3v3.
        /// 包含 flake.n3v3 或 package.n3v3 的目录。
        #[arg(default_value = ".")]
        dir: String,
        /// Registry URL (defaults to $N3V3_REGISTRY).
        /// 注册表 URL（默认为 $N3V3_REGISTRY）。
        #[arg(long)]
        registry_url: Option<String>,
    },

    /// System configuration commands (Unix only). / 系统配置命令（仅限 Unix）。
    #[cfg(unix)]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Store management commands (Unix only). / 存储管理命令（仅限 Unix）。
    #[cfg(unix)]
    Store {
        #[command(subcommand)]
        action: StoreAction,
    },
}

/// Format subcommands.
/// 格式化子命令。
#[derive(Subcommand)]
enum FmtAction {
    /// Format a file. / 格式化文件。
    File {
        /// The file to format. / 要格式化的文件。
        file: String,
        /// Write changes to file. / 将更改写入文件。
        #[arg(short, long)]
        write: bool,
    },
    /// Check if a file is formatted. / 检查文件是否已格式化。
    Check {
        /// The file to check. / 要检查的文件。
        file: String,
    },
    /// Format all files in a directory. / 格式化目录中的所有文件。
    Dir {
        /// The directory to format. / 要格式化的目录。
        dir: String,
        /// Write changes to files. / 将更改写入文件。
        #[arg(short, long)]
        write: bool,
    },
}

/// Package management subcommands (Unix only).
/// 软件包管理子命令（仅限 Unix）。
#[cfg(unix)]
#[derive(Subcommand)]
enum PackageAction {
    /// Install a package. / 安装软件包。
    Install {
        /// Package to install. / 要安装的软件包。
        package: String,
    },
    /// Remove a package. / 移除软件包。
    Remove {
        /// Package to remove. / 要移除的软件包。
        package: String,
    },
    /// List installed packages. / 列出已安装的软件包。
    List,
    /// Rollback to previous generation. / 回滚到上一代。
    Rollback,
}

/// Configuration management subcommands (Unix only).
/// 配置管理子命令（仅限 Unix）。
#[cfg(unix)]
#[derive(Subcommand)]
enum ConfigAction {
    /// Build system configuration. / 构建系统配置。
    Build,
    /// Switch to new configuration. / 切换到新配置。
    Switch,
    /// Interactively switch to a specific generation. / 交互式切换到特定代。
    SwitchTo,
    /// Rollback to previous configuration. / 回滚到上一个配置。
    Rollback,
    /// List configuration generations. / 列出配置代。
    List,
    /// Verify generation activation snapshot integrity. / 校验 generation 激活快照完整性。
    Verify {
        /// Verify all generations instead of only current. / 校验全部 generation，而不是仅当前。
        #[arg(long, short = 'a', conflicts_with = "generation")]
        all: bool,
        /// Generation number to verify. / 要校验的 generation 编号。
        #[arg(conflicts_with = "all")]
        generation: Option<u64>,
    },
}

/// Store management subcommands (Unix only).
/// 存储管理子命令（仅限 Unix）。
#[cfg(unix)]
#[derive(Subcommand)]
enum StoreAction {
    /// Run garbage collection. / 运行垃圾回收。
    Gc,
    /// Show store information. / 显示存储信息。
    Info,
}

/// Main entry point.
/// 主入口点。
fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        // Cross-platform commands (language features)
        // 跨平台命令（语言功能）
        Commands::Eval { expr } => commands::eval::run(&expr, cli.verbose),
        Commands::Test { dir } => commands::test::run(&dir, cli.verbose),
        Commands::Run { file, args } => {
            if !args.is_empty() {
                n3v3_std::set_script_args(args);
            }
            commands::run::run(&file, cli.verbose)
        }
        Commands::Check {
            file,
            allow_effects,
        } => commands::check::run(&file, cli.verbose, allow_effects),
        Commands::Fmt { action } => match action {
            FmtAction::File { file, write } => commands::fmt::run(&file, write),
            FmtAction::Check { file } => commands::fmt::check(&file),
            FmtAction::Dir { dir, write } => commands::fmt::format_dir(&dir, write),
        },
        Commands::Setup { editor } => match editor.as_str() {
            "helix" => commands::setup_helix::run(),
            "vscode" | "code" | "vs" => commands::setup_vscode::run(),
            _ => Err(format!(
                "unknown editor: {editor}. Supported: helix, vscode"
            )),
        },
        Commands::Lsp { check, version } => {
            if check {
                commands::lsp::check()
            } else if version {
                commands::lsp::version()
            } else {
                commands::lsp::run()
            }
        }
        Commands::Repl => commands::repl::run(),
        Commands::Explain { code } => commands::explain::run(&code),
        Commands::Doc { topic, list } => {
            if list || topic.is_none() {
                commands::doc::list()
            } else {
                commands::doc::view(topic.as_deref().unwrap())
            }
        }
        Commands::Init { dir } => commands::init::run(&dir),
        Commands::Version => {
            // Match the output of `--version` (clap).
            println!("n3v3 {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Commands::Info { package, platform } => {
            if platform || package.is_none() {
                commands::info::platform_info()
            } else {
                #[cfg(unix)]
                {
                    commands::info::run(package.as_deref().unwrap())
                }
                #[cfg(not(unix))]
                {
                    let _ = package;
                    eprintln!("Package info is only available on Unix systems");
                    Ok(())
                }
            }
        }

        // Unix-only commands (package management)
        // 仅限 Unix 的命令（软件包管理）
        #[cfg(unix)]
        Commands::Build {
            package,
            backend,
            cache_urls,
            cache_dirs,
            no_substitute,
            cache_upload,
            cache_public_keys,
            cache_private_keys,
        } => commands::build::run(commands::build::BuildRunArgs {
            package: package.as_deref(),
            backend_arg: &backend,
            cli_cache_urls: &cache_urls,
            cli_cache_dirs: &cache_dirs,
            cli_substitute: !no_substitute,
            cli_cache_upload: cache_upload,
            cli_cache_public_keys: &cache_public_keys,
            cli_cache_private_keys: &cache_private_keys,
        }),
        #[cfg(unix)]
        Commands::Package { action } => match action {
            PackageAction::Install { package } => commands::install::run(&package),
            PackageAction::Remove { package } => commands::remove::run(&package),
            PackageAction::List => commands::install::list(),
            PackageAction::Rollback => commands::remove::rollback(),
        },
        #[cfg(unix)]
        Commands::Search { query } => commands::search::run(&query),
        #[cfg(unix)]
        Commands::RegistryUpdate { registry_url } => {
            commands::registry::update(registry_url.as_deref())
        }
        #[cfg(unix)]
        Commands::RegistryServe { dir, host, port } => {
            commands::registry_serve::run(&dir, host, port)
        }
        #[cfg(unix)]
        Commands::RegistryPublish { dir, registry_url } => {
            commands::registry_publish::run(&dir, registry_url.as_deref())
        }
        #[cfg(unix)]
        Commands::Update => commands::update::run(),
        #[cfg(unix)]
        Commands::Config { action } => match action {
            ConfigAction::Build => commands::config::build(),
            ConfigAction::Switch => commands::config::switch(),
            ConfigAction::SwitchTo => commands::config::switch_interactive(),
            ConfigAction::Rollback => commands::config::rollback(),
            ConfigAction::List => commands::config::list_generations(),
            ConfigAction::Verify { all, generation } => commands::config::verify(generation, all),
        },
        #[cfg(unix)]
        Commands::Store { action } => match action {
            StoreAction::Gc => commands::store::gc(),
            StoreAction::Info => commands::store::info(),
        },
    };

    if let Err(e) = result {
        if !cli.quiet {
            eprintln!("error: {}", e);
        }
        // Distinguish error categories via exit code for scripting use.
        // 通过退出码区分错误类型，便于脚本使用。
        let code = if e.starts_with("parse error") {
            2
        } else if e.starts_with("type error") {
            3
        } else if e.starts_with("eval error") || e.starts_with("runtime error") {
            4
        } else {
            1 // I/O, config, unknown errors
        };
        std::process::exit(code);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::{Cli, Commands};
    use clap::Parser;

    #[test]
    fn registry_serve_default_host_is_loopback() {
        let cli = Cli::try_parse_from(["n3v3", "registry-serve"]).unwrap();
        match cli.command {
            Commands::RegistryServe { host, port, .. } => {
                assert!(host.is_loopback());
                assert_eq!(port, 8080);
            }
            _ => panic!("expected registry-serve command"),
        }
    }

    #[test]
    fn registry_serve_host_option_parses_ip_address() {
        let cli = Cli::try_parse_from(["n3v3", "registry-serve", "--host", "::1", "--port", "0"])
            .unwrap();
        match cli.command {
            Commands::RegistryServe { host, port, .. } => {
                assert!(host.is_loopback());
                assert_eq!(port, 0);
            }
            _ => panic!("expected registry-serve command"),
        }
    }
}
