//! Generated Rust ↔ Lean differential evaluator.
//!
//! Run with:
//! `cargo run --release -p n3v3 --example gen_diff_test -- -n 100 -d 4 -s 42`.
use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use tempfile::Builder;

// ============================================================
// Simple xorshift64 RNG — zero dependencies
// ============================================================
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }
    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
    fn rand_int(&mut self, min: i64, max: i64) -> i64 {
        (self.next() % (max - min + 1) as u64) as i64 + min
    }
    fn rand_bool(&mut self) -> bool {
        self.next().is_multiple_of(2)
    }
    fn rand_choice(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}

// ============================================================
// Expression generator
// ============================================================
struct GenState {
    rng: Rng,
}

impl GenState {
    fn new(seed: u64) -> Self {
        Self {
            rng: Rng::new(seed),
        }
    }

    fn gen_int(&mut self, depth: usize) -> (String, String) {
        if depth == 0 {
            let n = self.rng.rand_int(-100, 100);
            let ln = if n < 0 {
                format!("({} : Int)", n)
            } else {
                n.to_string()
            };
            return (n.to_string(), format!("(Expr.lit_int {})", ln));
        }
        match self.rng.rand_choice(4) {
            0 => {
                let n = self.rng.rand_int(-100, 100);
                let ln = if n < 0 {
                    format!("({} : Int)", n)
                } else {
                    n.to_string()
                };
                (n.to_string(), format!("(Expr.lit_int {})", ln))
            }
            1 => {
                let (l, ll) = self.gen_int(depth - 1);
                let (r, rl) = self.gen_int(depth - 1);
                (
                    format!("({l} + {r})"),
                    format!("(Expr.binop BinOp.Add {ll} {rl})"),
                )
            }
            2 => {
                let (l, ll) = self.gen_int(depth - 1);
                let (r, rl) = self.gen_int(depth - 1);
                (
                    format!("({l} - {r})"),
                    format!("(Expr.binop BinOp.Sub {ll} {rl})"),
                )
            }
            _ => {
                let (l, ll) = self.gen_int(depth - 1);
                let (r, rl) = self.gen_int(depth - 1);
                (
                    format!("({l} * {r})"),
                    format!("(Expr.binop BinOp.Mul {ll} {rl})"),
                )
            }
        }
    }

    fn gen_bool(&mut self, depth: usize) -> (String, String) {
        if depth == 0 {
            let b = if self.rng.rand_bool() {
                "true"
            } else {
                "false"
            };
            return (b.to_string(), format!("(Expr.lit_bool {b})"));
        }
        match self.rng.rand_choice(4) {
            0 => {
                let b = if self.rng.rand_bool() {
                    "true"
                } else {
                    "false"
                };
                (b.to_string(), format!("(Expr.lit_bool {b})"))
            }
            1 => {
                let (l, ll) = self.gen_int(depth - 1);
                let (r, rl) = self.gen_int(depth - 1);
                (
                    format!("({l} == {r})"),
                    format!("(Expr.binop BinOp.Eq {ll} {rl})"),
                )
            }
            2 => {
                let (l, ll) = self.gen_bool(depth - 1);
                let (r, rl) = self.gen_bool(depth - 1);
                (
                    format!("({l} && {r})"),
                    format!("(Expr.binop BinOp.And {ll} {rl})"),
                )
            }
            _ => {
                let (l, ll) = self.gen_bool(depth - 1);
                let (r, rl) = self.gen_bool(depth - 1);
                (
                    format!("({l} || {r})"),
                    format!("(Expr.binop BinOp.Or {ll} {rl})"),
                )
            }
        }
    }

    fn gen_expr(&mut self, depth: usize) -> (String, String) {
        if self.rng.rand_bool() {
            self.gen_int(depth)
        } else {
            self.gen_bool(depth)
        }
    }
}

fn fixed_cases() -> Vec<(String, String)> {
    [
        ("1 + 2", "(Expr.binop BinOp.Add (Expr.lit_int 1) (Expr.lit_int 2))"),
        (
            "(3 + 4) * 2",
            "(Expr.binop BinOp.Mul (Expr.binop BinOp.Add (Expr.lit_int 3) (Expr.lit_int 4)) (Expr.lit_int 2))",
        ),
        ("10 - 3", "(Expr.binop BinOp.Sub (Expr.lit_int 10) (Expr.lit_int 3))"),
        (
            "1 == 1",
            "(Expr.binop BinOp.Eq (Expr.lit_int 1) (Expr.lit_int 1))",
        ),
        (
            "1 == 2",
            "(Expr.binop BinOp.Eq (Expr.lit_int 1) (Expr.lit_int 2))",
        ),
        (
            "true && false",
            "(Expr.binop BinOp.And (Expr.lit_bool true) (Expr.lit_bool false))",
        ),
        (
            "false || true",
            "(Expr.binop BinOp.Or (Expr.lit_bool false) (Expr.lit_bool true))",
        ),
    ]
    .into_iter()
    .map(|(source, lean)| (source.to_string(), lean.to_string()))
    .collect()
}

// ============================================================
// Test runner
// ============================================================

fn project_dir() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `<root>/n3v3-cli`; its parent is the workspace root.
    // This example used to live in `scripts/`, so `file!()`-relative arithmetic
    // stopped at `n3v3-cli` and made `formal_dir()` point at a missing directory.
    // `CARGO_MANIFEST_DIR` 是 `<root>/n3v3-cli`，其父目录即工作区根。该示例原先位于
    // `scripts/`，基于 `file!()` 的相对路径只退到 `n3v3-cli`，导致 `formal_dir()`
    // 指向不存在的目录（`lake` 因此 ENOENT）。
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn n3v3_bin() -> PathBuf {
    let release = project_dir().join("target/release/n3v3");
    if release.exists() {
        release
    } else {
        PathBuf::from("cargo")
    }
}

fn formal_dir() -> PathBuf {
    project_dir().join("formal")
}

fn format_process_failure(prefix: &str, result: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    format!(
        "{prefix}(status={}): stdout={:?}; stderr={:?}",
        result.status,
        stdout.trim(),
        stderr.trim()
    )
}

fn run_rust(n3v3_source: &str, temp_dir: &Path) -> String {
    // Effect tests call `io.*` builtins; the module binding is part of the
    // program, so prepend it here instead of repeating it in every generated
    // source string. Without it the run fails with an unresolved `io` and the
    // suite only sees empty stdout.
    // 效果测试调用 `io.*` 内建，模块绑定属于程序本身：在此统一补上，避免每段生成源码
    // 各自重复。缺少它时求值报未解析的 `io`，套件只能看到空 stdout。
    let source = format!("use std.io = io;\n{n3v3_source}");
    let mut tmp = Builder::new()
        .suffix(".n3v3")
        .tempfile_in(temp_dir)
        .expect("create temporary n3v3 input");
    tmp.write_all(source.as_bytes()).expect("write n3v3 input");

    let bin = n3v3_bin();
    let output = if bin.file_name().is_some_and(|n| n == "n3v3") {
        Command::new(&bin).arg("run").arg(tmp.path()).output()
    } else {
        Command::new("cargo")
            .args(["run", "-q", "-p", "n3v3", "--", "run"])
            .arg(tmp.path())
            .output()
    };

    match output {
        Ok(result) if result.status.success() => String::from_utf8_lossy(&result.stdout)
            .trim()
            .trim_start_matches("[OK] ")
            .trim_start_matches("\u{2713} ")
            .to_string(),
        Ok(result) => format_process_failure("RUST_ERR", &result),
        Err(error) => format!("RUST_ERR: {error}"),
    }
}

fn run_lean(lean_exprs: &[String], _test_names: &[String], temp_dir: &Path) -> Vec<String> {
    let mut lean_code = String::from(
        r#"import n3v3.Tests.Eval
set_option maxRecDepth 100000
open n3v3 (Expr BinOp)

def main : IO Unit := do
"#,
    );

    for expr in lean_exprs {
        lean_code.push_str(&format!("  IO.println (fmt (evalClosed {}))\n", expr));
    }
    lean_code.push_str("  pure ()\n");

    let mut tmp = Builder::new()
        .suffix(".lean")
        .tempfile_in(temp_dir)
        .expect("create temporary Lean input");
    tmp.write_all(lean_code.as_bytes())
        .expect("write Lean input");

    let output = Command::new("lake")
        .args(["env", "lean", "--run"])
        .arg(tmp.path())
        .current_dir(formal_dir())
        .output();

    match output {
        Ok(result) if result.status.success() => String::from_utf8_lossy(&result.stdout)
            .lines()
            .map(|line| line.trim())
            .filter(|line| {
                !line.is_empty() && !line.starts_with("warning:") && !line.contains("(interpreter)")
            })
            .map(|line| line.to_string())
            .collect(),
        Ok(result) => vec![format_process_failure("LEAN_ERR", &result)],
        Err(error) => vec![format!("LEAN_ERR: {error}")],
    }
}

// ============================================================
// Effects property tests
// ============================================================

struct EffectTest {
    name: String,
    src: String,
    expected: String,
}

fn generate_effects_tests(n: usize, seed: u64) -> Vec<EffectTest> {
    let mut rng = Rng::new(seed);
    let mut tests = Vec::new();
    for i in 0..n {
        match rng.rand_choice(5) {
            0 => {
                let msg = format!("hello_{i}");
                tests.push(EffectTest { name: "execCommand".into(),
                    src: format!("let result = io.execCommand(io.command(\"echo\", [\"{msg}\"])); io.processStdout(result)"),
                    expected: msg });
            }
            1 => tests.push(EffectTest { name: "pipeline".into(),
                src: "let p = io.pipeline([io.command(\"echo\", [\"n3v3\"]), io.command(\"cat\", [])]); let r = io.execPipeline(p); toString(io.processSuccess(r))".into(),
                expected: "true".into() }),
            2 => tests.push(EffectTest { name: "stdin-small".into(),
                src: "let cmd = io.commandWith({ program = \"cat\", stdin = \"hello\" }); let r = io.execCommand(cmd); io.processStdout(r)".into(),
                expected: "hello".into() }),
            3 => tests.push(EffectTest { name: "exit-code".into(),
                src: "let r = io.execCommand(io.command(\"true\", [])); toString(io.processSuccess(r))".into(),
                expected: "true".into() }),
            _ => tests.push(EffectTest { name: "env-check".into(),
                src: "let r = io.execCommand(io.command(\"env\", [])); let out = io.processStdout(r); if out == \"\" -> \"empty\" else \"has-env\"".into(),
                expected: "has-env".into() }),
        }
    }
    tests
}

fn run_effects_suite(n: usize, seed: u64, temp_dir: &Path) -> (usize, usize) {
    println!("\nGenerating {n} effects tests (seed={seed})");
    let tests = generate_effects_tests(n, seed);
    let mut passed = 0;
    let mut failed = 0;
    for (i, t) in tests.iter().enumerate() {
        let output = run_rust(&t.src, temp_dir);
        if output.contains(&t.expected) || output == t.expected {
            passed += 1;
            if i % 10 == 0 && i > 0 {
                println!("  {i}/{n}...");
            }
        } else {
            failed += 1;
            let s = if output.len() > 50 {
                &output[..50]
            } else {
                &output
            };
            println!(
                "  ❌ {name}: expected '{exp}', got '{s}'",
                name = t.name,
                exp = t.expected
            );
        }
    }
    println!("\n  Effects: {passed}/{} passed", passed + failed);
    (passed, failed)
}

// ============================================================
// Main
// ============================================================

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let mut n_tests = 50usize;
    let mut depth = 3usize;
    let mut seed: Option<u64> = None;
    let mut effects = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-n" => {
                i += 1;
                if i < args.len() {
                    n_tests = args[i].parse().unwrap_or(50);
                }
            }
            "-d" => {
                i += 1;
                if i < args.len() {
                    depth = args[i].parse().unwrap_or(3);
                }
            }
            "-s" => {
                i += 1;
                if i < args.len() {
                    seed = Some(args[i].parse().unwrap_or(42));
                }
            }
            "--effects" => {
                effects = true;
            }
            _ => {}
        }
        i += 1;
    }

    let seed = seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() % 100000)
            .unwrap_or(42)
    });

    let temp_dir = tempfile::tempdir().expect("create private temporary directory");
    if effects {
        let (_, f) = run_effects_suite(n_tests, seed, temp_dir.path());
        return if f > 0 {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }

    let mut state = GenState::new(seed);
    println!("Generating {n_tests} tests (depth={depth}, seed={seed})");

    let mut n3v3_sources = Vec::with_capacity(n_tests);
    let mut lean_exprs = Vec::with_capacity(n_tests);
    for (n3v3_expr, lean_expr) in fixed_cases().into_iter().take(n_tests) {
        n3v3_sources.push(n3v3_expr);
        lean_exprs.push(lean_expr);
    }
    while n3v3_sources.len() < n_tests {
        let generated = n3v3_sources.len();
        if generated % 25 == 0 {
            println!("  Generated {generated}/{n_tests}...");
        }
        let (n3v3_expr, lean_expr) = state.gen_expr(depth);
        n3v3_sources.push(n3v3_expr);
        lean_exprs.push(lean_expr);
    }
    let test_names = (0..n_tests)
        .map(|index| format!("test_{index}"))
        .collect::<Vec<_>>();
    println!("  Generated {n_tests}/{n_tests} expressions");

    println!("Running Rust evaluator...");
    let mut rust_results = Vec::new();
    for (i, src) in n3v3_sources.iter().enumerate() {
        if i % 25 == 0 && i > 0 {
            println!("  Rust: {i}/{n_tests}...");
        }
        rust_results.push(run_rust(src, temp_dir.path()));
    }
    println!("  Rust: {n_tests}/{n_tests} done");

    println!("Running Lean evaluator...");
    let lean_results = run_lean(&lean_exprs, &test_names, temp_dir.path());
    println!("  Lean: {} results", lean_results.len());

    let mut passed = 0;
    #[allow(clippy::needless_range_loop)]
    let mut mismatches = Vec::new();
    #[allow(clippy::needless_range_loop)]
    for i in 0..n_tests {
        let rust = rust_results.get(i).map(|s| s.as_str()).unwrap_or("N/A");
        let lean = lean_results.get(i).map(|s| s.as_str()).unwrap_or("N/A");
        if rust == lean {
            passed += 1;
        } else {
            mismatches.push((i, &n3v3_sources[i], rust.to_string(), lean.to_string()));
        }
    }

    if !mismatches.is_empty() {
        println!(
            "\n{:<8} {:<40} {:<10} {:<10}",
            "Test", "n3v3 Source", "Rust", "Lean"
        );
        println!("{}", "=".repeat(90));
        for (i, src, rust, lean) in &mismatches {
            let s = if src.len() > 38 { &src[..38] } else { src };
            println!("❌ test_{:<3}  {:<38}  {:<10} {:<10}", i, s, rust, lean);
        }
    }

    println!("\n{}", "=".repeat(50));
    println!("  Results: {passed}/{n_tests} passed (seed={seed})");
    let msg = if passed == n_tests {
        "🎉 ALL MATCH".to_string()
    } else {
        format!("{} mismatches", n_tests - passed)
    };
    println!("  {msg}");
    println!("{}", "=".repeat(50));
    if passed != n_tests {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
