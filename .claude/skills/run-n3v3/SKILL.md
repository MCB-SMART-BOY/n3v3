---
name: run-n3v3
description: Build, run, smoke-test, and drive the n3v3 language toolchain (repl, eval, check, fmt, lsp). Use this when asked to run n3v3, test n3v3 CLI behavior, or verify a change works.
---

# Run n3v3

n3v3 is a pure functional language for system configuration, built as a
Rust Cargo workspace. The CLI binary (`n3v3`) is at `./target/debug/n3v3`
(debug) or `./target/release/n3v3` (release). Paths in this document are
relative to the repo root.

## Prerequisites

```bash
sudo apt-get install -y build-essential pkg-config libssl-dev
```

Rust toolchain is assumed present (`rustc`, `cargo`). If missing:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
```

## Build

cargo build -p n3v3          # debug — fast compile, for dev iteration
cargo build -p n3v3 --release  # release — for performance benchmarks

## Run (agent path) — smoke driver

The driver exercises nine representative CLI smoke scenarios (not every CLI
command) and exits 0 if all checks pass:
驱动脚本只覆盖九个代表性的 CLI 冒烟场景（并非全部命令），全部通过时退出码为 0：

```bash
.claude/skills/run-n3v3/driver.sh          # debug binary
.claude/skills/run-n3v3/driver.sh release  # release binary
```

It covers: `eval`, `run`, `check`, `repl` (piped input), `fmt file`,
`fmt check`, `lsp --check`, legacy v3.0 syntax compatibility, and error diagnostics.
覆盖 `eval`、`run`、`check`、管道输入的 `repl`、`fmt file`、`fmt check`、
`lsp --check`、旧 v3.0 语法兼容性和错误诊断。

### Individual commands

```bash
BIN=./target/debug/n3v3

# Evaluate an expression
echo '1 + 2 * 3' | $BIN repl          # → 7 (interactive)
$BIN eval "1 + 2 * 3"                 # → 7 (non-interactive)

# Run a .n3v3 file
$BIN run examples/data/lists.n3v3

# Type-check a file (effectful calls rejected by default)
$BIN check path/to/file.n3v3
$BIN check --allow-effects path/to/file.n3v3

# Format a file
$BIN fmt file path/to/file.n3v3        # prints to stdout
$BIN fmt check path/to/file.n3v3       # exit 0 if already formatted

# LSP health check
$BIN lsp --check                       # 7-point diagnostic

# Start LSP server (stdio transport)
$BIN lsp

# REPL with piped input (non-interactive)
printf '1 + 2\n:quit\n' | $BIN repl
```

### REPL (interactive, under tmux)

For tests that need a real interactive REPL session (screenshots, tab
completion), run under tmux:

```bash
tmux new-session -d -s n3v3-repl './target/debug/n3v3 repl'
tmux send-keys -t n3v3-repl '1 + 2' Enter
sleep 0.3
tmux capture-pane -t n3v3-repl -p > /tmp/repl-screenshot.txt
tmux send-keys -t n3v3-repl ':quit' Enter
tmux kill-session -t n3v3-repl
```

## Run (human path)

cargo run -p n3v3 -- repl     # interactive REPL
cargo run -p n3v3 -- run file.n3v3

## Test

```bash
cargo test --workspace                     # unit + integration
cargo test --test end_to_end -- --nocapture  # 558 E2E tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Gotchas

- **`n3v3 fmt file` outputs to stdout unless `--write` is supplied.** In write
  mode it replaces the file atomically and preserves a leading shebang plus
  Unix permission bits. `n3v3 fmt check` returns non-zero for unformatted files;
  this means "would reformat."
- **`n3v3 check` rejects effectful calls by default.** Use
  `--allow-effects` to bypass purity checking. Only `Error` diagnostics make
  the command fail; warnings do not, and a clean check prints
  `[OK] OK - No errors found`.
- **REPL stdin piping works for simple input** but multi-line programs
  with indentation may need the tmux approach above.
- **LSP health check requires filesystem state** — git repo, Helix
  grammar/queries. It may report fewer checks outside a full checkout.
- **Version output must match the workspace `Cargo.toml` version.** Run
  `n3v3 --version` and use `./scripts/counts.sh` to verify the current
  version (`v5.0.2`, published on crates.io and GitHub Releases).

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `cargo build` fails with "linker not found" | `sudo apt-get install build-essential` |
| `cargo build` fails with "ssl" errors | `sudo apt-get install libssl-dev` |
| `n3v3 lsp --check` reports missing grammar | Run `n3v3 setup helix` first |
| REPL hangs with piped input | Use `printf` with explicit `:quit\n` terminator |
| `n3v3 fmt check` fails after printing formatted output | Use `n3v3 fmt file <path> --write`; stdout-only formatting does not modify the file |

## Driver

[driver.sh](driver.sh) — nine representative smoke scenarios, not every CLI
path. Run it from the repo root. It creates a temp directory, writes .n3v3
test files, exercises the listed commands, and cleans up.
从仓库根目录运行；脚本创建临时目录、写入 .n3v3 测试文件、执行上述场景后清理。
