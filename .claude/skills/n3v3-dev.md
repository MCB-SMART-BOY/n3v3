# n3v3-dev: Project Architecture & Development

## Crate Dependency Graph

```
n3v3-cli ──────┬── n3v3-frontend ───┬── n3v3-parser ─── n3v3-lexer
               │                    │                   n3v3-syntax
               │                    ├── n3v3-hir
               │                    ├── n3v3-typeck
               │                    └── n3v3-eval ───── n3v3-std
               │
               ├── n3v3-lsp ──────── n3v3-frontend
               ├── n3v3-fmt ──────── n3v3-parser + n3v3-lexer
               ├── n3v3-config ───── n3v3-eval
               ├── n3v3-builder ──── n3v3-store + n3v3-fetch + n3v3-derive
               └── n3v3-diagnostic ─ (all crates)
```

**Dependency law**: `n3v3-frontend` is the **single entry point** for the language pipeline. No crate except `n3v3-cli`, `n3v3-lsp`, and `n3v3-config` should directly construct the parser + HIR + typeck chain.

## Data Flow Through the Pipeline

```
User Input (.n3v3 file or REPL line)
       │
       ▼
┌──────────────────────────────────────────────┐
│  n3v3-frontend (Driver + Session)             │
│  ┌─────────────────────────────────────────┐ │
│  │ 1. Lex  → [Token]                       │ │
│  │ 2. Parse → AST Module                   │ │
│  │ 3. Lower → Resolved HIR                 │ │
│  │ 4. Check → Typed HIR                    │ │
│  │ 5. Eval  → Value                        │ │
│  └─────────────────────────────────────────┘ │
│  Side tables: diagnostics, type map, def map  │
└──────────────────────────────────────────────┘
       │
       ├──→ CLI output (run/eval/check)
       ├──→ LSP responses (hover/completion/diag)
       └──→ Config evaluation (flake.nix-like)
```

For multi-module analysis, the frontend first finalizes one shared
`TraitResolver` from all loaded HIR modules, then gives each module a cloned
resolver and a fresh `TypeChecker`. Module-local definition and diagnostic
state is never reused across dependency and current-module checks.

`ModuleSemantics.action_plans` is an inspection-only side table. The frontend
clears plans for every analyzed module when any module has a blocking error;
evaluation must use the separately filtered, diagnostic-clean HIR path.

`n3v3-common::ProcessPlan` is the current internal process-description
boundary. `n3v3-std` lowers existing `CommandValue`/`PipelineValue` values into
owned stages before blocking command, pipeline, `execCommandLines`, and
ordinary Task-await execution; it does not change `Value`, `Task`, integer spawn
IDs, or evaluator streaming behavior. `ProcessPlan` is not a frontend action
executed by `check`, LSP, formatter, or pure analysis.

The `n3v3 check` purity gate walks canonical HIR with the frontend's method
resolution table. Resolved trait methods are checked by their method identity;
unresolved targets retain builtin fallback checking, and index operands,
guards, comprehension conditions, lambdas, and interpolations are traversed.

System-facing durability boundaries are separate from the language pipeline.
Configuration generations publish immutable, hashed activation snapshots before
moving `current`; failed activation restores the prior pointer, rollback selects
the nearest retained earlier generation, and `N3V3_CONFIG_DRY_RUN` previews
without changing `current` or `active`. Activation is root-confined,
transactional, preserves the ownership of replaced files, and rejects generated
shell scripts. Package profiles publish validated immutable generations. Direct
store additions reject empty or multi-component names, register `PathInfo`, and
record discovered store references; GC retains all profile generations,
recursively follows those references, and aborts before deletion if any reachable
ordinary store path lacks metadata. NAR/cache inputs
are untrusted until bounded extraction and declared size/hash verification
complete. The loopback-only built-in registry requires `N3V3_REGISTRY_TOKEN` for
publish requests; reads remain anonymous and public service still needs a gateway.

## Build Commands

```bash
cargo build -p n3v3         # CLI binary (CI target)
cargo check --workspace     # Fast validation (no codegen)
cargo test --workspace      # Unit + integration
cargo test --test end_to_end -- --nocapture  # E2E
cargo fmt --all             # Format (enforced in CI)
cargo clippy --workspace --all-targets -- -D warnings  # Lint
scripts/validate.sh          # Complete project quality gate / 项目完整质量门
scripts/validate.sh --quick  # Pre-commit only; skips full tests and CLI smoke / 仅 pre-commit，跳过完整测试与 CLI 冒烟
```

## Current CLI and diagnostic facts

- `n3v3 check` treats only `Severity::Error` diagnostics as errors. Warnings do
  not cause a failure; a clean check prints `[OK] OK - No errors found`.
- Typeck emits `duplicate definition of \`name\` shadows previous` as a
  `DiagnosticKind::Type` warning, not a parser diagnostic.
- `n3v3 doc --list` exposes 17 topics: `index`, `quickstart`, `tutorial`, `spec`,
  `api`, `diagnostics`, `philosophy`, `install`, `architecture`, `onboarding`,
  `contributing`, `feature-matrix`, `lsp`, `stability`, `ecosystem-design`,
  `registry`, and `changelog`.

## Commit Conventions

```
feat(repl): add history persistence    # New feature
fix(ci): correct script paths          # Bug fix
docs: update changelog                 # Documentation
refactor(typeck): simplify unification # Refactor
style(fmt): trailing commas            # Style only
release: bump version to 3.20.0        # Release
```

## Feature Addition Checklist

Every new effectful builtin:
1. `n3v3-common`: Register effect metadata in `intrinsic_metadata()`
2. `n3v3-std`: Register the runtime binding and delegate classification to the shared registry
3. `n3v3-frontend`: Wire into pipeline
4. `n3v3-eval`: Implement HIR evaluation
5. REPL: `:type` support
6. LSP: hover + completion
7. `tests/end_to_end.rs`: E2E parity test
8. `docs/reference/api.md`: Document
9. `docs/project/feature-matrix.md`: Update status

## Architecture Decisions

| Decision | When | Why |
|----------|------|-----|
| HIR as canonical pipeline | v1.2 | Single semantic truth, no AST/HIR divergence |
| `n3v3-frontend` as facade | v1.2 | Share analysis across CLI + LSP + REPL |
| AST compat path removed | v4.0 (2026-06) | HIR evaluator is the only path; ~3500 lines deleted |
| SemVer-hybrid release | v3.19 | Rapid evolution with clear deprecation lifecycle |
| `match` must be exhaustive | v3.18+ | Compiler-grade safety; if-else for non-exhaustive |

## Project Status

| Phase | Status | Key Deliverable |
|-------|--------|----------------|
| Syntax v4.0 | ✅ | 12 canonical keywords, `if->`, `use=`, `~expr`, effect auto |
| Phase 5 (Ecosystem) | ✅ | Flake/lock/store, Registry v1, crates.io |
| Phase 4 (Shell) | ✅ | 13 Stream<T> APIs, Task, TTY |
| Phase 3 (Runtime) | ✅ | Path/Bytes/Command/ProcessResult |
| Phase 2 (Type) | ✅ | Exhaustive match, trait dispatch |
| Phase 1 (Convergence) | ✅ | Canonical HIR pipeline |
