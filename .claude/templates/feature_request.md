## Feature Description
<!-- What should n3v3 be able to do? -->

## Use Case
<!-- Why do you need this? What problem does it solve? -->

## Proposed Syntax
```n3v3
<!-- How would you use this feature? -->
```

## Affected Pipeline Stages
<!--
Trace the proposal through the canonical pipeline:
Lexer → Parser → AST → HIR → Typeck → Eval.
Select every affected stage and explain the impact in the sections below.
-->
- [ ] Lexer
- [ ] Parser
- [ ] AST (`n3v3-syntax`)
- [ ] HIR lowering
- [ ] Type checker
- [ ] HIR evaluator
- [ ] Standard library

## Effectful Builtin Parity (if applicable)
<!-- Every new effectful builtin needs all of these integration points. -->
- [ ] Not an effectful builtin
- [ ] `n3v3_common::intrinsic_metadata()` entry (source of effect classification / 效果分类的权威来源)
- [ ] Typeck entry
- [ ] Frontend wire-up
- [ ] HIR eval
- [ ] REPL `:type`
- [ ] LSP hover
- [ ] E2E parity

## Tooling Impact
- [ ] Formatter impact evaluated (syntax, formatting, or round-trip behavior)
- [ ] LSP impact evaluated (diagnostics, hover, completion, or semantic tokens)

## Acceptance Criteria
<!-- Describe observable behavior, boundaries, diagnostics, and error handling. -->
- [ ] The proposed syntax and user-visible behavior are specified above.
- [ ] Canonical pipeline impact is addressed, or explicitly marked not applicable with rationale.
- [ ] Effectful builtin parity is complete, or explicitly marked not applicable with rationale.
- [ ] Formatter and LSP impact is addressed, or explicitly marked not applicable with rationale.
