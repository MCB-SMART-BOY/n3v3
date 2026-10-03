# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

n3v3 is a pure functional language for system configuration, built in Rust. 17 crates in a Cargo workspace. The CLI binary is `n3v3` (at `n3v3-cli/`). Published on crates.io as `n3v3`. Install: `cargo install n3v3`. Usage: `n3v3`.

## Build & Test

```bash
cargo build -p n3v3                     # build CLI
cargo check --workspace                 # fast validation
cargo test --workspace                  # all tests
cargo test --test end_to_end -- --nocapture  # 558 E2E tests
cargo test --test parser                # 239 parser tests
scripts/validate.sh                     # complete project quality gate
scripts/validate.sh --quick             # pre-commit quick gates (not a full gate)
.claude/skills/run-n3v3/driver.sh       # nine CLI smoke scenarios, not all commands
```

## Architecture

The canonical pipeline is `Lexer → Parser → HIR → Typeck → Eval`. All tooling (CLI, LSP, REPL) goes through `n3v3-frontend`.

```
crates/n3v3-lexer/     logos tokenizer
crates/n3v3-parser/    recursive descent LL(1)
crates/n3v3-syntax/    AST definitions
crates/n3v3-hir/       name resolution + lowering
crates/n3v3-typeck/    Hindley-Milner + traits
crates/n3v3-frontend/  pipeline facade (shared by CLI/LSP/REPL)
crates/n3v3-eval/      tree-walking interpreter (HIR canonical; AST from n3v3-syntax is lowered through HIR; AST compat path removed in v5.0)
crates/n3v3-std/       standard library (I/O, Stream<T>, collections)
crates/n3v3-lsp/       Language Server (26 methods)
crates/n3v3-fmt/       formatter
```

## Syntax (v4 grammar, current workspace product v5.0.3)

`let`/`fn`/`;` optional at top level. `use` not `import`. `|x|` not `fn(x)`. `{ }` records (no `#` prefix). `&` merges records in expressions; a lone `&` at line start (after optional whitespace) also starts a legacy line comment. Canonical line comments use `--`; block comments use `-- -- ... -- --`. `type Foo = | A | B` for enums. 12 canonical keywords. The parser retains 10 legacy spellings for source compatibility; only `struct`, `enum`, `super`, and `crate` are emitted as dedicated lexer tokens.
在表达式内，`&` 用于合并记录；单个 `&` 位于行首（允许前导空白，且下一字符不是 `&`）时也兼容行注释。规范行注释使用 `--`，块注释使用 `-- -- ... -- --`。

v4.0 simplifications: `if cond -> a else b` (was `then`), `use p = alias` (was `as`), `~expr` (was `lazy`), `effect` auto-inferred, `pub` removed (all public).

## Key Rules

- **Code is truth.** Skills in `.claude/skills/` must match actual code. Run `.claude/hooks/verify-skills.sh` after API changes.
- **Canonical pipeline only.** Do not introduce new AST fallback paths. AST compatibility path (`n3v3_eval::compat`) was removed in v5.0; AST definitions come from `n3v3-syntax` and are lowered through HIR.
- **Every effectful builtin** needs: typeck entry + frontend wire-up + HIR eval + REPL `:type` + LSP hover + E2E parity. Update `is_effectful_builtin()` in `n3v3-std`.
- **Match must be exhaustive.** Use `if-else` for non-exhaustive conditions.
- **Conventional Commits:** `feat(scope):`, `fix(scope):`, `docs:`, `refactor:`, `release:`.

## Agent Infrastructure

`.claude/` contains the full development toolchain:
- `.claude/rules.md` — hard rules: code-is-truth, `.claude/` synchronization, verifiability, the driver, the single quality-gate entry point, and documentation standards
- `.claude/skills/` — per-crate and cross-cutting architecture references
- `.claude/skills/run-n3v3/` — smoke-test driver
- `.claude/workflows/README.md` — workflow DSL contracts and result schemas (`full-test.js`, `pre-release.js`), loaded by the harness in CJS style
- `.claude/templates/` — commit, PR, and issue templates
- `.claude/hooks/` — pre-commit (`scripts/validate.sh --quick`), verify-skills, check-regressions, fmt-all (checks that `examples/**/*.n3v3` are canonically formatted; it does not rewrite files)
- `scripts/validate.sh` — the single quality-gate entry point
- `scripts/check-docs.sh` — documentation standards validator
- `scripts/counts.sh` — canonical project facts and counts
- `scripts/install-gate-tools.sh` — pinned external gate tools (gitleaks, trivy, cargo-audit, cargo-deny); the single version source for CI and release gates
- `docs/contributor/contributing.md` § Documentation Standards — human-readable documentation standard
- `.claude/forward-plan.md` — current status and roadmap
- `.claude/audit-report.md` — design audit findings (100% resolved, 62/62 ✅)

Use this map before changing a repository surface:

| Change | Read first |
| --- | --- |
| Quality gates and project facts | `scripts/validate.sh`, `scripts/counts.sh` |
| Documentation structure and claims | `scripts/check-docs.sh`, `docs/contributor/contributing.md` § Documentation Standards |
| Workflow automation | `.claude/workflows/README.md`, `.claude/hooks/` |

Do not hand-write counts or versions; use `scripts/counts.sh` as the source of
truth. `scripts/check-docs.sh` mechanically checks documentation claims.

## Current Status

The current workspace version is v5.0.3. v5.0.2 was published on crates.io as `n3v3` (`cargo install n3v3`) and has a GitHub Release with five platform archives; consult [GitHub Releases](https://github.com/MCB-SMART-BOY/n3v3/releases) and [crates.io](https://crates.io/crates/n3v3) for the latest published versions. 558 E2E tests, 26 LSP methods (19 requests + 7 notifications; `did_change_configuration` and `did_change_watched_files` are stubs and require a server restart), 13 Stream<T> APIs, 55 error codes, 12 canonical keywords. Public AST enums are `#[non_exhaustive]`; AST/HIR unsupported-node handling is explicit. Audit grade B+ → A- (62/62 fixed). All milestones complete. The v5.0.1 GitHub Release was blocked by the old ARM64 cross image; v5.0.2 builds natively on ARM64. Check [AUR `n3v3-bin`](https://aur.archlinux.org/packages/n3v3-bin) for availability before installing.
