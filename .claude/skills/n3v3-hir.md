# n3v3-hir: High-Level Intermediate Representation

## Architecture

```
SourceFile (AST) from n3v3-parser
       │
       ▼
┌─────────────────────────────────────────────────┐
│  Phase 1: Name Resolution (resolve.rs)           │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐ │
│  │ Resolver   │→ │ globals    │→ │ resolved   │ │
│  │ scopes     │  │ + scopes   │  │ identifiers│ │
│  └────────────┘  └────────────┘  └────────────┘ │
│  Walks AST items; resolves use statements;       │
│  builds module graph                              │
└─────────────────────────────────────────────────┘
       │ Resolved names + module graph
       ▼
┌─────────────────────────────────────────────────┐
│  Phase 2: Lowering (module_lowering.rs)          │
│  ┌────────────┐  ┌──────────┐  ┌──────────────┐ │
│  │ AST→HIR    │→ │ Pattern  │→ │ Effect       │ │
│  │ transformer│  │ lowering │  │ inference    │ │
│  └────────────┘  └──────────┘  └──────────────┘ │
│  Transforms AST nodes to HIR nodes;              │
│  resolves pattern bindings; marks effects         │
└─────────────────────────────────────────────────┘
       │ Resolved HIR Module
       ▼
┌─────────────────────────────────────────────────┐
│  Phase 3: Module Graph (module_paths.rs)         │
│  ┌────────────┐  ┌──────────┐  ┌──────────────┐ │
│  │ Path       │→ │ ModuleId │→ │ Loaded       │ │
│  │ resolution │  │ mapping  │  │ Module cache │ │
│  └────────────┘  └──────────┘  └──────────────┘ │
│  Handles: self, super, crate, relative, absolute  │
│  Flake-aware: resolves flake inputs as modules    │
└─────────────────────────────────────────────────┘
```

## AST → HIR Transformation

| AST Node | HIR Node | Key Change |
|----------|----------|------------|
| `ast::ExprKind::Var` | `ExprKind::Var` / `ExprKind::Global` | Name → `LocalId` or `DefId` |
| `ast::ExprKind::Call` | `ExprKind::Call` | Function name resolved |
| `ast::ExprKind::Lambda` | `ExprKind::Lambda` | Parameter bindings resolved |
| `x.foo(y)` | `ExprKind::MethodCall { .. }` | Method receiver and fallback target retained |
| `ast::ExprKind::Match` | `ExprKind::Match` | Pattern bindings resolved |
| AST imports | `Module.imports` | Import path resolved in the module registry |
| `expr?` | `ExprKind::Try` | Optional-flow wrapper |
| `~expr` | `ExprKind::Lazy` | Thunk wrapper |

## Name Resolution Strategy

```
Scope stack (lexically nested):
┌─────────────────────────────────┐
│  Module scope (top-level items) │ ← global definitions
│  ┌───────────────────────────┐  │
│  │  Block scope { ... }      │  │ ← let bindings in blocks
│  │  ┌─────────────────────┐  │  │
│  │  │  Lambda scope |x|.. │  │  │ ← parameter bindings
│  │  └─────────────────────┘  │  │
│  └───────────────────────────┘  │
└─────────────────────────────────┘
```

Resolution order for `use`:
1. `self` → current module
2. `super` → parent module
3. `crate` → workspace root
4. `std.*` → standard library (built-in overrides)
5. Relative path → resolved from current file's directory
6. Flake input → resolved from `flake.n3v3`

## Enum variant registration

Enum type names and variant constructors share one global namespace, so the
resolver refuses ambiguous registrations instead of overwriting:

| Declaration | Diagnostic |
|-------------|-----------|
| `enum Choice { Yes(Int), Yes(Int) }` | `duplicate enum variant \`Yes\`` |
| `enum Only { Only(Int) }` | `enum variant \`Only\` conflicts with the enum name \`Only\`` |
| Variant name already bound by another global | `duplicate enum variant \`…\`` |

Variant spans are keyed by `(enum DefId, span)` in `variant_ids`; variants that
collide are reported and skipped rather than silently shadowing an existing
global.

## Key Types

```rust
pub struct Module {
    pub id: ModuleId,
    pub name: String,
    pub items: Vec<Item>,
    pub imports: Vec<Import>,
    pub exports: Option<Vec<String>>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct DefId(pub u32);
pub struct LocalId(pub u32);

pub struct Item {
    pub id: DefId,
    pub kind: ItemKind,
    pub span: Span,
}

pub struct Expr {
    pub kind: ExprKind,
    pub ty: Ty,
    pub span: Span,
}

pub enum ExprKind {
    Var(LocalId),
    Global(DefId),
    Builtin(String),
    Call(Box<Expr>, Vec<Expr>),
    MethodCall { receiver: Box<Expr>, method: String, target: Box<Expr>, args: Vec<Expr> },
    Lazy(Box<Expr>),
    Error(String),
    // Record, list, tuple, match, block, and other expression forms.
}
```

## Integration Points

| From | To | Data |
| n3v3-parser | n3v3-hir | `SourceFile` (AST) |
| n3v3-hir | n3v3-typeck | `Module` (resolved, ready for type inference) |
| n3v3-hir | n3v3-frontend | `DefId`/`ModuleId` and module graph side tables |

## Key Files

| File | What |
|------|------|
| `crates/n3v3-hir/src/hir.rs` | HIR node definitions |
| `crates/n3v3-hir/src/resolve.rs` | Name resolution (scope stack, definition registration) |
| `crates/n3v3-hir/src/module_lowering.rs` | AST → HIR transformation |
| `crates/n3v3-hir/src/module_paths.rs` | Module path resolution (self/super/crate/flake) |

## Testing

- **Lowering fidelity**: Every pattern form must survive lowering without loss.
- **Lossy lowering is a bug**: If a construct degrades to wildcard, it must be fixed.
- **Module resolution**: Test self/super/crate/relative/absolute/`use =` paths.
- **Runtime boundary**: HIR evaluator tests cover lazy parameters, forced
  conditions/guards/comprehension filters, and environment restoration after
  errors and tail calls.

## Current Audit Boundaries

- Canonical lowering preserves index expressions, destructuring bindings,
  constructor identity, built-in generic type IDs, and `std.bytes` import
  registration.
- Effect inference is declaration-order independent, resolves method calls
  before its final fixed-point pass, and traverses nested lambdas, guards, and
  comprehension conditions.
- Tooling consumers use canonical spans for destructured/or-pattern bindings
  and constructor references; synthetic item mappings remain an explicit
  boundary for future convergence.
- `n3v3-frontend` publishes a non-executing `ModuleSemantics.action_plans`
  side table collected from canonical HIR. The current typed operation is
  literal `io.readFile` → `HostOp::ReadFile`; dynamic host paths do not get a
  fabricated plan.

When any module in a frontend analysis has a blocking diagnostic, the frontend
redacts `action_plans` from all exposed module semantics. Evaluation consumers
must use diagnostic-clean HIR entries rather than inspection side tables.

The current execution-layer ProcessPlan adapter is separate from this HIR side
table. It lowers existing runtime `Command`/`Pipeline` values only at explicit
blocking execution boundaries; it does not add a new HIR node, expose a
`Job<T>` type, or make Stream/Event/Live lifecycles part of HIR.

## Unsupported-node policy

Unsupported future AST forms are not lowered to a valid-looking HIR node.
Expression and statement fallbacks become `ExprKind::Error` /
`StmtKind::Expr` error nodes, unsupported patterns become
`PatternKind::Error`, and unsupported import prefixes/items are rejected at
the frontend boundary. Type checking reports preserved HIR errors before
evaluation.

- Resolver stores boundary diagnostics in `Module.diagnostics`; `TypeChecker`
  forwards them before semantic passes, so unsupported declarations, enum
  variants, and type forms cannot disappear silently.

- Top-level patterns with no bindings are accepted only for `_`; known
  refutable forms produce an AST-to-HIR boundary diagnostic.
