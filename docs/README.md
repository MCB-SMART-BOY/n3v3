<div align="center">

<img src="../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>n3v3 Documentation Hub</h1>

<p>
  <strong><a href="../README.md">Home</a></strong> ·
  <strong><a href="./">Docs</a></strong> ·
  <strong><a href="user/quickstart.md">Quickstart</a></strong> ·
  <strong><a href="reference/spec.md">Spec</a></strong> ·
  <strong><a href="reference/api.md">API</a></strong>
</p>

</div>

**Current version: v5.0.2**. 12 canonical keywords, 55 diagnostic codes, 558 E2E tests, 26 LSP methods. Install via `cargo install n3v3`; five platform archives are available on [GitHub Releases](https://github.com/MCB-SMART-BOY/n3v3/releases/tag/v5.0.2).

Welcome to the n3v3 documentation hub. For **developer reference** (per-crate APIs,
architecture, integration points), see `.claude/skills/` — 13 skill entry points
covering crate and cross-cutting tooling, kept in sync with the actual code.

---

## Start Here

- New user: [user/install.md](user/install.md), [user/quickstart.md](user/quickstart.md), [user/tutorial.md](user/tutorial.md), [runnable examples / 可运行示例](../examples/README.md)
- Language reference: [reference/spec.md](reference/spec.md), [reference/api.md](reference/api.md), [reference/diagnostics.md](reference/diagnostics.md)
- Contributor: [contributor/contributing.md](contributor/contributing.md), [contributor/onboarding.md](contributor/onboarding.md), [contributor/architecture.md](contributor/architecture.md)
- Project and ecosystem: [project/philosophy.md](project/philosophy.md), [project/feature-matrix.md](project/feature-matrix.md), [project/ecosystem-design.md](project/ecosystem-design.md), [project/registry.md](project/registry.md), [project/changelog.md](project/changelog.md)
- Tooling and policy: [reference/lsp.md](reference/lsp.md), [reference/stability.md](reference/stability.md)
- **Developer skills**: [`.claude/skills/`](../.claude/skills/) — 13 skill entry points for architecture, APIs, and key files

---

## By Audience

### I want to use n3v3

- [user/install.md](user/install.md): installation, platform notes, binary cache setup
- [user/quickstart.md](user/quickstart.md): quickest path to first expression and file
- [user/tutorial.md](user/tutorial.md): learn the language surface systematically
- [examples/README.md](../examples/README.md): index of runnable teaching examples / 可运行教学示例索引

### I want exact language truth

- [reference/spec.md](reference/spec.md): syntax and semantic rules
- [reference/api.md](reference/api.md): standard library reference
- [reference/diagnostics.md](reference/diagnostics.md): diagnostic code index

### I want to understand project reality

- [project/philosophy.md](project/philosophy.md): design principles and trade-offs
- [project/feature-matrix.md](project/feature-matrix.md): real support matrix
- [project/ecosystem-design.md](project/ecosystem-design.md): flake, store, builder, registry
- [project/registry.md](project/registry.md): package registry behavior and policy
- [project/changelog.md](project/changelog.md): release history, including v5.0.2

### I want tooling and policy

- [reference/lsp.md](reference/lsp.md): Language Server Protocol methods and capabilities
- [reference/stability.md](reference/stability.md): stability tiers and compatibility policy

### I want to contribute

- [contributor/contributing.md](contributor/contributing.md): setup, workflow, style
- [contributor/onboarding.md](contributor/onboarding.md): codebase reading order
- [contributor/architecture.md](contributor/architecture.md): crate responsibilities and pipeline

---

## CLI Docs

The built-in catalog is exposed through these 17 `n3v3 doc` topics:

```bash
n3v3 doc index
n3v3 doc quickstart
n3v3 doc tutorial
n3v3 doc spec
n3v3 doc api
n3v3 doc diagnostics
n3v3 doc philosophy
n3v3 doc install
n3v3 doc architecture
n3v3 doc onboarding
n3v3 doc contributing
n3v3 doc feature-matrix
n3v3 doc lsp
n3v3 doc stability
n3v3 doc ecosystem-design
n3v3 doc registry
n3v3 doc changelog
```

For command behavior, use `n3v3 --help`.

---

<div align="center">

**[Main README](../README.md)** · **[License: MPL-2.0](../LICENSE)**

</div>
