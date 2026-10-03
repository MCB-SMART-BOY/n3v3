# n3v3 Language Development Skills

A comprehensive skill suite for developing the n3v3 functional programming language (v5.0.2).

## Available Skills

| Skill | Purpose | Crate(s) |
|-------|---------|----------|
| [n3v3-dev](n3v3-dev.md) | General workflow, build, test, CI | workspace |
| [n3v3-parser](n3v3-parser.md) | Lexer + recursive descent parser | n3v3-lexer, n3v3-parser, n3v3-syntax |
| [n3v3-typeck](n3v3-typeck.md) | Hindley-Milner type inference + Traits | n3v3-typeck |
| [n3v3-hir](n3v3-hir.md) | HIR lowering + name resolution | n3v3-hir |
| [n3v3-eval](n3v3-eval.md) | HIR tree-walking interpreter | n3v3-eval |
| [n3v3-std](n3v3-std.md) | Standard library (I/O, Stream<T>, List) | n3v3-std |
| [n3v3-lsp](n3v3-lsp.md) | Language Server Protocol | n3v3-lsp |
| [n3v3-test](n3v3-test.md) | Testing strategy + 558 E2E tests | tests/ |
| [n3v3-diagnostic](n3v3-diagnostic.md) | Compiler error system (55 diagnostic codes) | n3v3-diagnostic |
| [n3v3-fmt](n3v3-fmt.md) | Code formatter | n3v3-fmt |
| [n3v3-effect](n3v3-effect.md) | Effect system (EffectEval v4.3) | n3v3-typeck, n3v3-eval |
| [n3v3-lean](n3v3-lean.md) | Formal verification (Lean 4) | formal/ |
| [run-n3v3](run-n3v3/SKILL.md) | Build, run, smoke-test, and drive the CLI (`run-n3v3/driver.sh`) | workspace |

## Language Pipeline

```
Source Code (.n3v3)
       │
       ▼
┌──────────────┐  n3v3-lexer (logos)
│   Lexer      │  Tokens with Span
└──────┬───────┘
       │
       ▼
┌──────────────┐  n3v3-parser (recursive descent LL(1))
│   Parser     │  AST: SourceFile { items: [...] }
└──────┬───────┘
       │
       ▼
┌──────────────┐  n3v3-hir (name resolution + lowering)
│  HIR Lower   │  Resolved HIR with scoped definitions
└──────┬───────┘
       │
       ▼
┌──────────────┐  n3v3-typeck (Hindley-Milner + Traits)
│  Type Check  │  Typed HIR with inferred types
└──────┬───────┘
       │
       ▼
┌──────────────┐  n3v3-eval (tree-walking, lazy)
│  Evaluator   │  Value: Int | String | List | ...
└──────────────┘
```

## Syntax Quick Reference

| Feature | v3.0 | v4.0 |
|---------|------|------|
| Type definition | `type Foo = { }` / `type Bar = \| ...` | same |
| Import | `use std.list` | same |
| Import alias | `use std.io as io` | `use std.io = io` |
| Lambda | `\|x\| x + 1` | same |
| Record | `{ x = 1 }` | same |
| Comment | `& comment` (also `--` still works) | same |
| Top-level | `let`/`fn`/`;` optional | same |
| Record merge | `a & b` | same |
| If expression | `if cond then a else b` | `if cond -> a else b` |
| Lazy evaluation | `lazy expr` | `~expr` |
| Effect annotation | `fn foo() effect = ...` | `fn foo() = ...` (auto-inferred) |
| Visibility | `pub fn` | `fn` (all public by default) |
| Keywords | 17 | 12 canonical keywords |

## Crate Map

| Crate | Responsibility |
|-------|----------------|
| `n3v3-lexer` | Tokenization (logos) |
| `n3v3-parser` | Recursive descent parser (LL(1)) |
| `n3v3-syntax` | AST node definitions + source file |
| `n3v3-hir` | High-level IR + name resolution |
| `n3v3-typeck` | Hindley-Milner type inference + Traits |
| `n3v3-frontend` | Pipeline facade (driver + session) |
| `n3v3-eval` | HIR tree-walking interpreter |
| `n3v3-std` | Standard library (I/O, Stream<T>, collections) |
| `n3v3-lsp` | Language Server Protocol (26 LSP methods: 19 requests + 7 notifications) |
| `n3v3-fmt` | Code formatter (37/37 idempotency) |
| `n3v3-cli` | Command-line interface |
| `n3v3-diagnostic` | Error reporting (ariadne) |
| `n3v3-common` | Shared utilities + kill_process |
| `n3v3-store` | Content-addressed store (BLAKE3) |
| `n3v3-fetch` | Source fetching (URL, Git, local) |
| `n3v3-builder` | Sandbox build system |
| `n3v3-config` | System configuration (generation-based) |
| `n3v3-derive` | Derivation model + hashing |

## Current Status (v5.0.2)

| Metric | Value |
|--------|-------|
| Phase 6 (Syntax v4.0) | ✅ Complete |
| Phase 5 (Ecosystem) | ✅ Complete |
| Phase 4 (Shell) | ✅ Complete |
| E2E Tests | 558 E2E tests (all pass) |
| Formatter idempotency | 37/37 |
| Stream<T> APIs | 13 Stream<T> APIs |
| LSP Methods | 26 LSP methods (19 requests + 7 notifications) |
| EffectEval Rules | 34 (v4.3) |
| Lean Modules | 21 Lean modules |
| Error Codes | 55 diagnostic codes |
| Keywords | 12 canonical keywords |
| Release CI | v5.0.2 passed; five GitHub Release archives uploaded |
| crates.io | v5.0.2 published (`cargo install n3v3`) |

## Decisions (2026-06-03)

- G2: Method dispatch is type-based; retain callable fallback
- Match: `match` must be exhaustive; non-exhaustive → `if-else`
- Lean: Continue full formal verification
- AST: compat path removed in v4.0 — HIR evaluator is canonical
- Release: SemVer-hybrid (see `docs/reference/stability.md`)
