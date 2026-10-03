# n3v3 Forward Plan — 2026-10-03


## Current State / 当前状态

The v5.0.0 canonical HIR pipeline remains the implementation baseline in the v5.0.3 workspace. v5.0.3 is published on [crates.io](https://crates.io/crates/n3v3) with five [GitHub Release archives](https://github.com/MCB-SMART-BOY/n3v3/releases/tag/v5.0.3), and [AUR `n3v3-bin`](https://aur.archlinux.org/packages/n3v3-bin) is available. The v5.0.1 GitHub Release was blocked by the ARM64 Linux cross-link failure; v5.0.2 first shipped all five archives. Current facts are owned by `scripts/counts.sh`:

| Fact | Current value |
|------|---------------|
| Product version | v5.0.3 (workspace) |
| E2E tests | 558 E2E tests |
| LSP surface | 26 LSP methods |
| Diagnostics | 55 diagnostic codes |
| Canonical syntax | 12 canonical keywords |
| Stream surface | 13 Stream<T> APIs |
| Formalization | 21 Lean modules |

The AST/HIR/tooling boundary cutover is implemented. Open follow-up work is:

- Phase 1/2 Action IR shadow path is implemented: `n3v3_common::intrinsic_metadata`
  owns effect summaries and `io.readFile` metadata; `n3v3-frontend` publishes
  non-executing literal-read `ActionPlan` entries; `FakeHost`/`OsHost` provide the
  explicit host seam. This does not change default CLI evaluation or add action
  syntax. The remaining migration phases (Process/Job, action syntax, Stream and
  reactive separation) remain open.
- The current `OsHost` is race-resistant on Unix by anchoring reads to a pinned
  root directory and opening each component with `O_NOFOLLOW`; non-Unix targets
  fail closed with `UnsupportedPlatform` until an equivalent directory-handle
  implementation is available.
- MSRV / `rust-version` remains undeclared.
- CI now includes deterministic Rust ↔ Lean differential evaluation; code-coverage, benchmark, and fuzz gates remain absent.
- `json_to_value` still has no explicit recursion-depth limit.
- `kill_process` still has a PID-reuse window.
- Trait fallback selection remains nondeterministic for multiple candidates.
- The tree-sitter grammar remains v3.x-shaped.
- `io.tempDir` is Implemented through evaluator-owned dispatch; it returns the callback value and cleans up afterward (`crates/n3v3-eval/src/eval.rs:1712-1717,2658-2678`; `tests/end_to_end.rs::test_end_to_end_io_temp_dir_returns_value_and_cleans_up`).
- `io.streamLines` and `io.streamBytes` retain a runtime-only Path acceptance extension; the typeck/docs contract remains `String` (`crates/n3v3-typeck/src/check/builtin_type.rs:1459-1472`).
- CLI gate fidelity: `--release` now builds the release CLI in its `build` gate and drives `cli-smoke`/`docs` with that binary (`N3V3_BIN` for the docs checker), and `.claude/skills/run-n3v3/driver.sh` rebuilds before every run. Before this, a stale `target/release/n3v3` still produced `[PASS]` for those gates - the stale binary reported `type error(s) found`/exit 3 for a lexer error that current code classifies as `parse error`/exit 2. `cargo build --release -p n3v3` in release mode costs ~80 s cold, ~0 s warm.
- `.claude/skills/run-n3v3/driver.sh:86` has a pre-existing `shellcheck` SC2086 (unquoted `$BIN` in `timeout 3 $BIN lsp --check`). No gate runs `shellcheck`, so it is informational; `scripts/validate.sh` and `scripts/check-docs.sh` are clean.

The remaining typeck/runtime mismatch is the runtime-only `Path` acceptance
extension for `io.streamLines` and `io.streamBytes`; typeck declarations remain
canonical. Use `scripts/validate.sh` as the project-wide quality-gate entry
point; use `scripts/counts.sh` for facts and `scripts/check-docs.sh` for
documentation checks.

**Latest review (2026-09-25)**: a full lexer → parser → AST → HIR → typeck → eval
re-check against the v4.0/v5.0 syntax settled nine defects — slash-vs-division
lexing, enum-variant name conflicts, zero-parameter binding semantics, per-frame
`io.defer`, thunk failure caching, method-call TCO, the native stack budget,
UTF-16 semantic token positions, and one tautological defer test. See
[`audit-report.md`](audit-report.md) → "AST/HIR/IR Review (2026-09-25)" for
evidence and the remaining boundaries (ASCII-only identifiers, expression-depth
stack budget, zero-parameter call semantics).

**Second review round (2026-09-25, same day)**: a critical pass over the review
itself, the quality gates, and the formatter found and fixed eleven issues —
a dependency gate that always passed, two zero-input false PASS paths in the
docs checker, a formatting hook that could not fail, three formatter defects
whose output did not re-parse (`[x, ..]` separator, `||` zero-parameter lambda,
record-typed lambda bodies), lexer errors reported as type errors (exit 3
instead of 2), `n3v3 run` exiting 1 on syntax errors, a false typeck comment
about `io.readFileLinesPath`, an effect test that could not fail, a release gate
that bypassed the single entry point while tag pushes skip `ci.yml`, and a
`--quiet` help text that did not match its behaviour. See
[`audit-report.md`](audit-report.md) → "评审修复记录 (2026-09-25 · 第二轮)" for
per-item evidence; the open decisions are the shebang loss in `n3v3 fmt` and the
bare-record lambda body grammar question.

**Latest Action IR review (2026-09-30)**: AST/HIR remain the canonical
representation; no standalone Action/Process/Job/Stream IR exists yet. The
current `ActionPlan` shadow path covers only literal `io.readFile` and remains
inspection-only; frontend analysis redacts plans from every exposed module when
any module has a blocking diagnostic. Process/Job/Stream IR migration remains
open. See [`audit-report.md`](audit-report.md) → "AST/HIR/Action IR Review
(2026-09-30)".

**Phase 3 ProcessPlan adapter slice**: `n3v3-common` now owns an owned,
non-executing `ProcessPlan`/`ProcessStage`/`ProcessRedirect` description.
`n3v3-std` lowers existing `CommandValue`/`PipelineValue` values into that
description before blocking command, pipeline, `execCommandLines`, and
ordinary Task-await execution; existing `ProcessResult`, redirect, environment
filtering, error prefix, and size-limit contracts remain in force. This is an
internal adapter, not a `Value`, `Job<T>`, Action syntax, frontend plan
expansion, or cold Stream implementation. Spawn/timeout/await-any and
evaluator-owned streaming paths remain legacy boundaries for later slices.

**2026-10-03 product hardening closeout**: the platform paths now share explicit
fail-closed boundaries. Configuration builds persist immutable, hashed
activation artifacts before publishing a generation; switch/rollback activate
those artifacts under a checked root and restore the previous pointer on
failure. NAR extraction rejects unsafe entries and trailing payloads, binary
cache substitution verifies declared size/hash before extraction, direct store
additions register metadata and references, and GC treats validated profile
generations plus store references as roots. `n3v3 init` uses create-new writes
with rollback, `n3v3 fmt --write` preserves shebangs and permission bits, all 25
examples are canonical, and the loopback-only built-in registry requires
`N3V3_REGISTRY_TOKEN` for publishing. CI runs the generated fixed-seed Rust ↔
Lean differential suite rather than the removed self-comparison script.

The acceptance evidence does not justify expanding Action/Job/Stream IR in this
hardening slice: the canonical HIR path is complete, while the remaining
migration changes product semantics rather than closing a demonstrated safety
gap. Public registry deployment is likewise deferred; the built-in service
remains a loopback development server, and public operation requires the
separate TLS/auth/policy gateway described in `docs/project/registry.md`.


## Release Exit Criteria / 发布退出标准


| # | Criterion | Status |
|---|-----------|--------|
| 1 | AST compat path fully removed | Implemented (2026-06-16) — `ast_eval.rs` deleted |
| 2 | All 6 implementation gaps closed | Implemented — shebang is handled by parser |
| 3 | Lean axioms documented | Implemented — 3 axioms documented (Lean 4.29+ remains an external prerequisite) |
| 4 | Release policy stable for 2+ minor versions | Implemented — v3.18 → v3.19 → v4.0 → v4.0.4 → v5.0.0 |
| 5 | External contribution policy published | Implemented — `docs/contributor/contributing.md` |
| 6 | Semantic convergence verified | Implemented — the 12 historical E2E gaps were resolved |


## Completed Milestones / 已完成里程碑


### Audit Completion — Implemented (historical milestone)


**Goal**: Close remaining audit items. Grade: B+ → A-.

| ID | Task | Effort | Status |
|----|------|--------|--------|
| A1 | Close unicode char escape `\u{...}` in lexer | Small | Implemented |
| A2 | Fix C1: `store/src/cache.rs` unwrap() → Result | Large | Not applicable — all 225 were in test code |
| A3 | Centralize `libc` dependency — workspace deps, 5 crates | Small | Implemented |
| A4 | Fix tree-sitter-n3v3 edition 2021→2024 | Small | Implemented |
| A5 | Add missing Map/Set/math to api.md (D12) | Small | Implemented — already present |
| A6 | Fix spec Nix comparison (D14) | Trivial | Implemented |
| A7 | Fix stability.md version example (D16) | Trivial | Implemented |


**Milestone deliverables**: 7/7 items resolved. Historical audit total: 62/62. Current open items are tracked in `audit-report.md`.


### Gap Closure — Implemented (2026-06-16)


**Goal**: Close the 12 E2E gap tests. These represent real missing features.

The 12 gaps, ordered by impact:

| # | Gap | Crate to fix | Status |
|---|-----|-------------|--------|
| B1 | v3.0 enum pipe syntax (`\| Red \| Green \| Blue`) | n3v3-parser | Implemented (2026-06-16) |
| B2 | TupleIndex expression | n3v3-eval (HIR) | Implemented (already worked, test added) |
| B3 | Block-with-let lowering | n3v3-hir | Implemented (already worked, test added) |
| B4 | Nested blocks lowering | n3v3-hir | Implemented (already worked, test added) |
| B5 | Generic identity inference | n3v3-typeck | Implemented (2026-06-16) — generalize + instantiate fix |
| B6 | Option match pattern lowering | n3v3-hir | Implemented (already worked, test added) |
| B7 | Record match pattern lowering | n3v3-hir | Implemented with current record syntax; v3.0 syntax remains historical |
| B8 | `?.` safe access lowering | n3v3-hir | Implemented (already worked, test added) |
| B9 | Impl method dispatch | n3v3-hir + n3v3-typeck | Implemented (2026-06-16) — impl Int method dispatch |
| B10 | Stdlib pipeline module resolution | n3v3-frontend | Implemented (2026-06-16) |
| B11 | List comprehension HIR/AST parity | n3v3-eval | Implemented (already worked, test added) |
| B12 | Match Option HIR/AST parity | n3v3-eval | Implemented (already worked, test added) |


**Milestone deliverables**: all historical gap tests were un-ignored and the milestone was completed. The current E2E count is owned by `scripts/counts.sh`.


### Ecosystem Readiness — Implemented


**Goal**: Decision gates Q6 + Q7.

| ID | Task | Status |
|----|------|--------|
| C1 | Q6: Registry internal validation period | Implemented — `docs/project/registry.md` |
| C2 | Q7: External contribution policy | Implemented |
| C3 | Q6: Registry public launch plan | Implemented — phased plan in `registry.md` |
| C4 | CONTRIBUTING.md update | Implemented (comprehensive guide) |
| C5 | CLA/DCO decision | Planned — decision remains open; MPL-2.0 is inbound-only |




### v5.0 API Cutover — Implemented (v5.0.0 workspace release)


**Goal**: Seal the public AST boundary and preserve future syntax additions across the canonical pipeline.

| ID | Task | Status |
|----|------|--------|
| D1 | Add `#[non_exhaustive]` to public AST enums | Implemented |
| D2 | Migrate all workspace AST matches to wildcard handling | Implemented |
| D3 | Add AST/HIR unsupported-node diagnostics | Implemented |
| D4 | Version workspace and internal requirements to v5.0.0 | Implemented |
| D5 | Add external-consumer compile-fail regression | Implemented |
| D6 | Update stability and migration documentation | Implemented |

## Next Steps / 下一步


```
Implemented: canonical parser/HIR/typeck/eval pipeline and AST/HIR/tooling boundary handling.
Implemented: v5.0.0 API cutover and documented contributor/workflow contracts.
Planned: close the open follow-up boundaries listed in Current State and `audit-report.md`.
Verification entry point: `scripts/validate.sh` (use `--ci` or `--release` for the corresponding gate set).
```


## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| MSRV remains undeclared | Medium | Medium | Add and validate `rust-version` in a dedicated change |
| CI has no coverage/benchmark/fuzz gates | Medium | Medium | Plan dedicated CI coverage and performance work |
| tree-sitter grammar diverges from v4.0 | High | Medium | Update `grammar.js`, regenerate, and validate through `scripts/validate.sh` |
| JSON depth and PID reuse remain open | Medium | High | Add explicit depth bounds and process-identity-safe termination |
| `io.streamLines` / `io.streamBytes` runtime-only Path extension | Medium | Medium | Keep typeck/docs `String` declarations in `builtin_type.rs:1459-1472` canonical; reconcile runtime acceptance at `fs.rs:1577-1586,1706-1714` |
| Built-in registry lacks TLS, policy, audit, and multi-tenant isolation | High if port-forwarded | High | Keep it loopback-only with bearer-authenticated writes; require a TLS/auth gateway or dedicated registry before public exposure |


## Decision Gates / 决策门


```
✅ Q4 (Windows)  ✅ Q5 (Release)  ✅ Q6 (Registry)  ✅ Q7 (Contributions)  ✅ Q8 (AST)  ✅ Q9 (Parser)

**All decision gates cleared.**
```

Q6 + Q7 were the last gates before v4.0; the v5.0.0 AST API cutover is complete. These gates are historical release evidence, not a second current quality contract.


## 2026-06-16: Comprehensive Design Audit (historical snapshot, 62 findings)


6-agent sweep across architecture, safety, type system, tests, API, and CLI/LSP.

**Score: B-** — Core sound, engineering maturity needs improvement.

**Fixed this session (13 findings):**

| ID | Finding | Status |
|----|---------|--------|
| C1 | reqwest TLS features overwritten → HTTPS broken | ✅ Fixed |
| H1 | n3v3-eval unused dep on n3v3-parser | ✅ Removed |
| H2 | n3v3-config 3 unused deps | ✅ Removed |
| M1 | n3v3-derive unused dep on n3v3-common | ✅ Removed |
| M2 | n3v3-store unused dep on n3v3-common | ✅ Removed |
| M3 | n3v3-fetch duplicate dep declarations | ✅ → workspace refs |
| M4 | Orphan deps (glob, rpassword, termimad) | ✅ Noted for later |
| M5 | n3v3-builder nix feature duplication | ✅ → workspace ref |
| L13 | n3v3-cli unused dep on n3v3-lexer | ✅ Removed |
| C4 | README syntax drift (lazy, then, effect, ||) | ✅ v4.0 syntax |
| C5 | Examples legacy syntax (import, then, as) | ✅ 25 files fixed |
| H12 | fmt.rs UTF-8 path panic | ✅ to_string_lossy() |
| M16 | CLI error messages generic | ✅ Diagnostic counts |

**Historical priority list (superseded):** current open items are maintained in `audit-report.md`, not this historical table.


| ID | Finding | Current status |
|----|---------|----------------|
| C2 | AST nodes unsealed (no `#[non_exhaustive]`) | Implemented |
| C3 | 10+ pub types → pub(crate) | Planned |
| C7 | CacheStats → HirCacheStats | Implemented |
| C6 | Formatter drops all comments | Implemented |
| H3 | Trait bounds never enforced at call sites | Implemented |
| H4 | types_match ignores type args | Implemented |
| H5 | Enum generics always empty args | Planned |
| H6 | Mutex poisoning | Implemented |
| H8 | Recursion stack protection | Implemented |
| H9 | Occurs check in dynamic records | Implemented |


See `.claude/audit-report.md` for full details (62 findings, fix roadmap).

---

## Appendix: Semantic Convergence Plan (merged from docs/project/semantic-convergence-plan.md)

### Decisions (D-001 through D-007)

**D-001: Single semantic authority** — The canonical execution pipeline is `Parser -> Resolved HIR -> Typed HIR -> HIR Evaluation`. AST evaluation has been fully removed (v4.0). Differential/oracle testing and temporary bootstrap paths are now HIR-native.

**D-002: No implicit fallback** — `n3v3 eval` and `n3v3 run` must not silently fall back to AST. Any AST path must be explicitly requested and visible in output and tests. `n3v3 check`, REPL, and LSP must not use AST fallback at all.

**D-003: One shared frontend driver** — CLI, REPL, and LSP must converge on one shared frontend/driver result. Consumers should not hand-roll `ModuleLoader + TypeChecker + diagnostics rewrite`.

**D-004: Typed HIR stays side-table based** — Do not rewrite all HIR nodes to carry embedded types. Typed HIR is defined via normalized side tables and semantic artifacts.

**D-005: Runtime convergence precedes new features** — Prioritize semantic consistency over feature expansion. Builtin/runtime behavior must converge under HIR before effectful capability growth.

**D-006: Module infrastructure splits before crate splits** — Decompose module infrastructure inside existing crates first. Do not extract new crates until boundaries are proven under CLI/REPL/LSP usage. `ModuleLoader` may temporarily survive as a compatibility facade only.

**D-007: Stable content identity replaces mtime as semantic authority** — Stable content hash becomes the semantic identity. `mtime`, dirty flags, and ad-hoc hash state are only performance hints. Invalidation must be phase-aware: parse cache on source change; module graph on import-set/target change; lowering/name-resolution on own-source or dependency export-surface change; typed side tables on own-HIR or dependency type/export-surface change.

### Friction Map

1. **Semantic authority is converged** — AST compat evaluation was fully removed in v4.0 and all evaluation now uses canonical HIR. Remaining friction is consumer-specific orchestration in `run`, `eval`, `build`, and config-related flows; some consumers still reconstruct parse/lower/typecheck/eval steps instead of using one shared driver.

2. **Module infrastructure is over-coupled** — `ModuleLoader` mixes file discovery, source caching, module graph construction, import resolution, lowering orchestration, and diagnostics accumulation. `Resolver` still knows about module loading concerns and std import shortcuts. Cache identity still leans on `mtime`, dirty flags, and non-stable hash behavior.

3. **Tooling semantics still drift** — CLI `check`, REPL `:type`, and LSP hover/diagnostics still recompute semantic data in different ways. Type names, method resolutions, and diagnostics rewriting are not produced from one canonical artifact.

4. **Type semantics are not yet compiler-grade** — Exhaustiveness and unreachable-pattern diagnostics exist as embedded checker logic rather than a dedicated analysis pass. Trait dispatch and associated-type use-site resolution are not yet normalized through one solving pipeline. `Try`/`Option`/`Result`/`coalesce`/safe field access still require semantic tightening.

5. **Effect boundaries are still implicit** — Stdlib process and filesystem calls still directly touch the host. System-facing APIs are still mostly string/record-based. `build`, `config`, `fetch`, and `store` do not yet depend on an explicit effect runtime boundary.

### Target Architecture

**Layer boundaries** — The kernel owns parsing, HIR lowering, name resolution, type checking, typed side tables, and pure HIR evaluation (no host effects). The frontend driver (`n3v3-frontend`) is the sole public orchestration entrypoint: source loading, module graph assembly, diagnostics aggregation, and typed artifact publication. The effect runtime is the only layer allowed to execute host-side effects: process spawning, filesystem, environment, cancellation/timeout/signal mediation. System platform consumers (`build`, `config`, `package`, `fetch`, `store`) consume canonical frontend artifacts and explicit effect runtime APIs.

**Canonical artifacts** — `ParsedProgram`, `ResolvedProgram`, `TypedProgram`, `ProgramDiagnostics`, `ModuleSemantics`, `AttributedDiagnosticSet`. Typed HIR aggregates resolved HIR, normalized global/local/expression types, method resolutions, associated-type projections, readable display-name map, and merged diagnostics.

*This content was merged from `docs/project/semantic-convergence-plan.md`. The original file has been removed.*

---

## Appendix B: Strategic Framework (merged from docs/project/language-roadmap.md)

### Glossary / 术语对照

| Term | 中文 | Meaning |
|------|------|---------|
| canonical pipeline | 规范执行管线 | `Parser → HIR → Typeck → Eval` — the single semantic authority |
| semantic convergence | 语义收敛 | All pipeline stages agree on one language meaning |
| effect boundary | 副作用边界 | Pure expressions vs I/O/process/system modification |
| structured runtime | 结构化运行时 | `Path`, `Command`, `ProcessResult` — not strings |
| work package (WP) | 工作包 | Smallest trackable unit for issues/PRs |
| decision gate | 决策门 | Must-decide question before entering a phase |

### Decision Gate Rationale / 决策门详解

**G1: Canonical Pipeline** ✅ — HIR evaluation is the sole canonical runtime. AST compat path removed in v4.0.

**G2: Method Semantics** ✅ — Dispatch order: inherent impl → trait method → callable fallback. If none, `UnknownMethod` diagnostic.

**G3: Failure Propagation** ✅ — `?`/`??`/`?.` unified through `resolve_optional_flow_payload`. Closure matrix covers Option/Result, user enums, safe field access.

**G4: Effect Boundary** — Core language stays pure; effects go through a dedicated execution layer. See `.claude/effect-boundary-design.md`.

**G5: Bash Replacement Scope** — Structured replacement of common shell workloads. NOT full POSIX compat. NOT interactive shell.

### Work Package Catalog / 工作包目录

| ID | Work Package | Status |
|----|-------------|--------|
| WP-0A | Feature support matrix | ✅ |
| WP-0B | Real end-to-end harness | ✅ |
| WP-0C | Documentation status correction | ✅ |
| WP-1A | Pattern lowering fidelity | ✅ |
| WP-1B | Try/Option/Result unification | ✅ |
| WP-1C | Method/trait dispatch unification | ✅ |
| WP-1D | HIR evaluator parity | ✅ |
| WP-1E | Remove placeholder hacks | ✅ |
| WP-2A | Exhaustiveness checking | ✅ |
| WP-2B | Unreachable pattern analysis | ✅ |
| WP-2C | Associated type resolution | ✅ |
| WP-2D | REPL/LSP semantic fidelity | ✅ |
| WP-3A | Path runtime type | ⚠️ |
| WP-3B | Bytes runtime type | ✅ |
| WP-3C | Command/ProcessResult types | ✅ |
| WP-3D | Pipeline/stream handles | ✅ |
| WP-4A | Effect boundary design | ✅ |
| WP-4B | Pure vs task execution split | ✅ |
| WP-4C | Stdlib effect classification | ✅ |
| WP-5A | Redirection runtime | ✅ |
| WP-5B | Scoped env/cwd | ✅ |
| WP-5C | Timeout/retry/cancel/bg | ✅ |
| WP-5D | Signal/TTY/shebang | ✅ |
| WP-5E | Port validation corpus to n3v3 | ⚠️ |
| WP-6A | Lockfile/resolver | ✅ |
| WP-6B | Registry/package metadata | ⚠️ |
| WP-6C | Stdlib stability tiers | ✅ |
| WP-6D | Release/compat policy | ✅ |

### Explicit Deferrals / 明确延后项

| Item | Why deferred |
|------|-------------|
| Macros | Amplify semantic instability if added too early |
| HKT | Not justified before core type semantics complete |
| FFI | Needs stable runtime + effect boundary first |
| Interactive shell | Larger scope than non-interactive scripting |
| Full POSIX compat | High cost, low strategic value |

### Progress Metrics / 进度度量

| Metric | What it measures |
|--------|-----------------|
| Feature matrix coverage | % of syntax/semantic items classified |
| Real E2E count | Programs through real pipeline |
| Lossy lowering count | Constructs degrading during HIR lowering |
| Stringly API count | System APIs as raw strings |
| Script port count | Shell scripts replaced by n3v3 |

### What Must Not Happen / 不可触碰的红线

- Do not add syntax while lowering is lossy.
- Do not claim "complete" based on smoke tests alone.
- Do not treat shell replacement as "just add exec builtins".
- Do not let pure config evaluation become effectful scripting.
- Do not maintain dual AST/HIR semantics.

### Acceptance Standard / 验收标准

**Standalone language**: one canonical pipeline, tests cover real pipeline, docs describe real boundary, stdlib structured and stable, tooling reflects actual semantics.

**Bash replacement**: commands/pipelines/redirects/env/failures are first-class, common automation needs no Bash escape, effect model explicit enough for system config.

*This content was merged from `docs/project/language-roadmap.md`. The original file has been removed.*
