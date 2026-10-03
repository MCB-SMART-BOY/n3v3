<div align="center">

<img src="../../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>Design Philosophy</h1>

<p><em>设计哲学</em></p>

<p>
  <strong><a href="../../README.md">Home</a></strong> ·
  <strong><a href="../README.md">Docs</a></strong>
</p>

</div>

---

## The Vision / 愿景

"I don't hate Nix. I want to BE Nix — but the Nix that could have been, if we started fresh today."

说实话，我不讨厌 Nix。我想成为 Nix——但是是那个如果今天从头开始的话，本应该成为的 Nix。

n3v3 draws on Nix's ideals (purity, reproducibility, declarative configuration) without claiming that every current build is isolated or reproducible. It does not aim for nixpkgs compatibility.
n3v3 借鉴 Nix 的理念（纯函数、可重现、声明式配置），但不宣称当前每次构建都隔离或可重现；不以兼容 nixpkgs 为目标。
Current workspace version: v5.0.3. For published versions, consult [GitHub Releases](https://github.com/MCB-SMART-BOY/n3v3/releases).

当前工作区版本：v5.0.3。已发布版本请查看 [GitHub Releases](https://github.com/MCB-SMART-BOY/n3v3/releases)。

> "Inherit and surpass" — that's the goal. / 继承，然后超越。

## Core Principles / 设计原则

### 1. Zero Ambiguity / 零歧义

Every syntax has one meaning. No guessing.

每种语法只有一个意思，不用猜。

```n3v3
{ x = 1 }        -- ALWAYS a record (container delimited by { })
```

```n3v3
{ x = 1; x }     -- ALWAYS a block
```

```n3v3
|x| x + 1        -- ALWAYS a lambda
```

These are three alternative writings, each a complete item on its own: written
on consecutive lines without separators they would parse as one application
(`{ x = 1 }` applied to `{ x = 1; x }`), so keep them in separate items or
separate them with `;`.
三者是并列的写法，各自都是独立项：若不加分隔直接连写，它们会被解析为一次函数应用
（`{ x = 1 }` 作用于 `{ x = 1; x }`），因此需要分项或使用 `;` 分隔。

### 2. Syntax Unity / 语法统一

Similar concepts, similar syntax.

相似的东西，相似的写法。

| Concept / 概念 | Syntax / 语法 |
|---------------|---------------|
| Named function / 命名函数 | `add(x, y) = x + y` |
| Lambda / Lambda | `|x, y| x + y` |
| Function type / 函数类型 | `Int -> Int` |
| Match arm / 匹配分支 | `pattern -> result` |

Arrow (`->`) always means "produces".

箭头（`->`）永远表示「产出」。

### 3. No Magic Indentation / 不靠缩进

Explicit delimiters. No Python-style whitespace sensitivity.

有明确的分隔符，不像 Python 那样对空格敏感。

### 4. Pure Functional / 纯函数

Pure by default; effects are checked at the boundary where they are introduced. No mutable state.

默认保持纯函数；在引入 effect 的边界进行检查。没有可变状态。

### 5. Simplicity / 简洁

A minimal set of keywords (current v5.0.3 workspace: 12 canonical keywords; the v4.0 syntax cleanup reduced the v1.x set from 21).

精简的关键字集合（当前 v5.0.3 工作区：12 个规范关键字；v4.0 语法清理将 v1.x 的 21 个缩减为当前集合）。

```
let  fn   type  trait  impl  use
self if   else  match
true false
```

### 6. Unix Philosophy / Unix 哲学

Do one thing well. Compose. Text is universal.

做好一件事。组合。文本是通用接口。

## Nix vs n3v3 / Nix 对比

| Pain Point / 槽点 | Nix | n3v3 |
|------------------|-----|------|
| Record or function? / 记录还是函数？ | `{ x = 1; }` vs `{ x }: x` | `{ x = 1 }` vs `|x| x` |
| Type safety / 类型安全 | None / 没有 | Hindley-Milner / HM 类型推导 |
| Recursion / 递归 | `rec { }` | Automatic / 自动处理 |
| Inherit / 继承字段 | `inherit x y z;` | `{ x, y, z }` |
| Error timing / 报错时机 | Runtime / 运行时 | Compile-time / 编译时 |

## Current Status / 开发进度

| Area / 领域 | Status | Notes / 说明 |
|------------|--------|-------------|
| Language core / 语言核心 | Implemented | Workspace v5.0.3 retains the canonical v4 syntax published in v5.0.2; parser/lowering/typeck/eval form one pipeline. Evidence: `crates/n3v3-parser`, `crates/n3v3-hir`, `crates/n3v3-typeck`, `crates/n3v3-eval` |
| Runtime / 运行时 | Implemented | HIR is the canonical runtime path and the AST compatibility evaluator is removed. Evidence: `crates/n3v3-eval` |
| Toolchain / 工具链 | Experimental | 26 LSP methods are implemented; `did_change_configuration` and `did_change_watched_files` remain stubs. Evidence: `crates/n3v3-lsp/src/backend.rs` |
| Package system / 包管理 | Experimental | Fetch/store/builder and local registry are implemented; registry-backed installation only lists versions when the local store misses. Binary-cache signing is optional when keys are configured. Evidence: `n3v3-cli/src/commands/install.rs`, `n3v3-cli/src/registry_client.rs`, `n3v3-cli/src/commands/build.rs` |
| OS integration / 系统集成 | Experimental | Linux native namespaces depend on availability; Docker needs Docker and an image, while `simple` is not a filesystem/network sandbox. Evidence: `n3v3-cli/src/commands/build.rs`, `crates/n3v3-builder/src/sandbox.rs` |

### Planned work / 当前计划

- **Planned**: Registry public launch (`registry.n3v3.dev`), after the current Experimental validation work in [the registry design](registry.md).
- **Planned**: Cross-platform sandbox improvements.
- **Planned**: Standard library expansion.
- **计划**：注册表公开发布（`registry.n3v3.dev`），先完成[注册表设计](registry.md)中当前的 Experimental 验证工作。
- **计划**：跨平台沙箱改进。
- **计划**：标准库扩展。

### Planned future directions / 计划中的未来方向

- **Planned**: HKT, macros, n3v3 OS, decentralized package federation.
- **计划**：高阶类型、宏、n3v3 OS、去中心化包联邦。

## The Name / 名字由来

*n3v3* = "snow" in Italian/Portuguese. Same spirit as Nix (Latin "snow"), new journey.

n3v3 是意大利语和葡萄牙语的「雪」，跟 Nix（拉丁语的「雪」）同源。同样的精神，新的旅程。

---

> "Perfection is achieved when there is nothing left to take away." / 「完美不是无可增加，而是无可删减。」
