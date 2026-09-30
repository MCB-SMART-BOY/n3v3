# n3v3-lean: Formal Verification (Lean 4)

## Architecture

```
formal/
├── lakefile.lean              ← Lake build config
├── lean-toolchain             ← Lean version pin
└── n3v3/
    ├── Spec/                  ← Formal specification
    │   ├── Syntax.lean        ← Ty, Expr, Value, Pattern, BinOp
    │   ├── Typing.lean        ← Γ ⊢ e : τ  (type system)
    │   ├── Eval.lean          ← env ⊢ e ⇓ v  (big-step semantics)
    │   ├── Values.lean        ← EnvMatches(P), canonical forms
    │   ├── BigStep.lean       ← Big-step v2 (27 rules, incl. matchOn_fallthrough)
    │   └── Effects.lean       ← EffectEval v4.3 (34 rules)
    ├── Proofs/                ← Machine-checked proofs
    │   ├── Safety.lean        ← type_safety: [] ⊢ e : τ → ∃ v, [] ⊢ e ⇓ v
    │   └── SafetyLemmas.lean  ← 5 verified pattern lemmas
    ├── Verify/                ← Security property proofs
    │   ├── Path.lean          ← Path traversal safety (M-1)
    │   ├── Environ.lean       ← Environment injection (M-4)
    │   └── Limits.lean        ← Buffer size limits (H-1, H-2)
    └── Refinement/            ← Rust ↔ Lean bridge (scaffolded)

    ← 21 Lean modules (`find formal -name '*.lean'`) →
```

## Proof Dependency Graph

```
        Syntax.lean
        /    |    \
       /     |     \
  Typing.lean  Eval.lean  Values.lean
       \      |      /
        \     |     /
      Safety.lean ←── SafetyLemmas.lean
           │
           ├── Effects.lean ←── BigStep.lean
           │
           └── Verify/
                ├── Path.lean
                ├── Environ.lean
                └── Limits.lean
```

## Theorem Coverage

| Theorem | Status | File |
|---------|--------|------|
| `type_safety` | 13/17 proved | `Proofs/Safety.lean` |
| `binop_correctness` | 12/12 proved | `Proofs/Safety.lean` |
| `env_preservation` | Extracted lemma | `Proofs/Safety.lean` |
| `wildcard_match` | ✅ | `Proofs/SafetyLemmas.lean` |
| `lit_int_match` | ✅ | `Proofs/SafetyLemmas.lean` |
| `lit_bool_match` | ✅ | `Proofs/SafetyLemmas.lean` |
| `bool_full` | ✅ | `Proofs/SafetyLemmas.lean` |
| `unit_match` | ✅ | `Proofs/SafetyLemmas.lean` |
| `path_safety` | ✅ | `Verify/Path.lean` |
| `env_safety` | ✅ | `Verify/Environ.lean` |
| `buffer_limits` | ✅ | `Verify/Limits.lean` |
| `bytes_formalized` | ✅ | `Spec/Syntax.lean` + `Spec/Values.lean` |

## Remaining Axioms (Blocked on Lean 4.29+)

```
Axiom 1: non-lam app  — big-step closure body recursion
Axiom 2: non-lam pipe — big-step pipe body recursion
Axiom 3: general matchOn — mutual inductive limitation
```

## Key Formalisms

### Type System
```
Γ ⊢ e : τ   — "expression e has type τ in context Γ"

lit_int : — ⊢ lit_int(n) : Int
var     : (x,τ) ∈ Γ ⊢ var(x) : τ
lam     : (x,τ₁)::Γ ⊢ body : τ₂ → Γ ⊢ lam(x,body) : Fn τ₁ τ₂
app     : Γ ⊢ f : Fn τ₁ τ₂ → Γ ⊢ arg : τ₁ → Γ ⊢ app(f,arg) : τ₂
let     : Γ ⊢ val : τ → (x,τ)::Γ ⊢ body : τ' → Γ ⊢ letIn(x,val,body) : τ'
```

### Big-Step Semantics
```
env ⊢ e ⇓ v   — "e evaluates to v in env"

lit_int(n) ⇓ int(n)
var(x)     ⇓ v  where (x,v) ∈ env
lam(x,body) ⇓ closure(x, body, env)
app(f,arg)  ⇓ v  where f⇓closure(x,body,env'), arg⇓varg, (x,varg)::env'⊢body⇓v
```

### Effects
```
env ⊢ e ⇓[σ] v, σ'   — "e evaluates to v, transforming I/O state σ to σ'"

execCommand: env ⊢ cmd ⇓[σ] run_process(cmd), σ ∪ {stdout, stderr}
```

## Rust ↔ Lean Refinement Bridge

```
Rust (implementation)                 Lean (specification)
─────────────────────                 ────────────────────
resolve_redirect_path(path, cwd)  ←→  Verify/Path.lean
configured_process_command(env)   ←→  Verify/Environ.lean
stdin_size_check / output_check   ←→  Verify/Limits.lean
eval_expr(expr, env)              ←→  Spec/Eval.lean  (partial)
```

## Development Workflow

```bash
cd formal

# Build
lake build

# Check a theorem
lake env lean n3v3/Proofs/Safety.lean

# Run Lean language server
lake exe lean4 --server

# Add a theorem:
# 1. Define lemma in appropriate file
# 2. Prove using tactics
# 3. Add to module dependency chain
# 4. lake build to verify all
```

## Key Files

| File | Purpose |
|------|---------|
| `formal/lakefile.lean` | Lake build configuration |
| `formal/n3v3/Spec/Syntax.lean` | Abstract syntax — Ty, Expr, Value, Pattern, BinOp |
| `formal/n3v3/Spec/Typing.lean` | Typing judgment: Γ ⊢ e : τ |
| `formal/n3v3/Spec/Eval.lean` | Big-step evaluation v2 (`inductive BigStep`, env ⊢ e ⇓ v) + matchOn_fallthrough |
| `formal/n3v3/Proofs/Values.lean` | Values + ValueTyping + EnvMatches(P) |
| `formal/n3v3/Spec/Effects.lean` | EffectEval v4.3 (34 rules) |
| `formal/n3v3/Proofs/Safety.lean` | type_safety theorem (13/17) + BinOp (12/12) |
| `formal/n3v3/Proofs/SafetyLemmas.lean` | Pattern matching lemmas (5 verified) |
| `formal/n3v3/Verify/Path.lean` | Path traversal proof (M-1) |
| `formal/n3v3/Verify/Environ.lean` | Environment safety (M-4) |
| `formal/n3v3/Verify/Limits.lean` | Buffer limits (H-1, H-2) |
