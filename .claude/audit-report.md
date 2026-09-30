# n3v3 设计审查报告 (Design Audit)
> **历史快照 / Historical snapshot**：本报告记录的是 2026-06-17 设计审计的快照；报告中的版本、测试数和问题状态均属于当次审计，不代表当前仓库状态。当前版本与计数以 [`scripts/counts.sh`](../scripts/counts.sh) 为准；后续状态以文件与验证证据为准。
>
> **Historical snapshot**: This report records the 2026-06-17 design audit. Its versions, counts, and finding states describe that audit only and are not a claim about the current repository. Use [`scripts/counts.sh`](../scripts/counts.sh) for current facts; later status notes are grounded in the cited files and checks.


**日期**: 2026-06-17 | **版本**: v4.0.1 | **审查范围**: 全项目 (17 crates, 541 E2E)

---

## 概述

从成熟语言设计师角度，对 n3v3 项目进行全方位审查。共发现 **62 个问题**，按严重程度分为 Critical / High / Medium / Low。

### 问题分布

| 类别 | Critical | High | Medium | Low | 合计 |
|------|----------|------|--------|-----|------|
| 架构与依赖 | 3 | 2 | 4 | 3 | 12 |
| 安全与错误处理 | 1 | 3 | 2 | 0 | 6 |
| 类型系统正确性 | 1 | 4 | 7 | 7 | 19 |
| 测试与 CI | 0 | 6 | 4 | 4 | 14 |
| API 与文档 | 7 | 0 | 2 | 2 | 11 |
| CLI/REPL/LSP | 3 | 4 | 4 | 5 | 16 |
> 本报告的原始条目保留审计当时的现象描述；带有 `Implemented`、`Partially implemented` 或 `Planned / open` 的标记和证据，才表示当前核对结果。


---

## Critical (必须立即修复)

### C1. reqwest TLS 特性被覆盖 — Implemented


**文件**: `crates/n3v3-fetch/Cargo.toml:12`, `n3v3-cli/Cargo.toml:44`

workspace 定义 `reqwest = { features = ["blocking", "rustls-tls"] }`，但两个 crate 用 `features = ["blocking"]` 覆盖了 workspace 的 features，导致 `rustls-tls` 丢失。这意味着所有 HTTPS 请求(registry、fetch)都会因为没有 TLS 后端而失败。

```toml
# 当前(错误):
reqwest = { workspace = true, features = ["blocking"] }
# 应为:
reqwest = { workspace = true }
```

**影响**: n3v3-fetch 的 URL 获取、n3v3-cli 的 registry 操作全部无法使用 HTTPS。
**当前状态（Implemented）**：两个 crate 现使用 workspace `reqwest` 配置，保留 workspace 的 `rustls-tls` 特性。证据：`crates/n3v3-fetch/Cargo.toml:12`、`n3v3-cli/Cargo.toml:43`。


### C2. AST 节点完全未密封 — Implemented


**文件**: `crates/n3v3-syntax/src/ast.rs`, `expr.rs`, `types.rs`

所有 AST 类型 (`ExprKind` 34 variants, `ItemKind` 11 variants, `TypeKind`, `PatternKind`, `BinOp`, `UnaryOp`, `VariantKind`, `StmtKind`) 都缺少 `#[non_exhaustive]`，且所有字段都是 `pub`。任何外部 crate 可以绕过 Parser 直接构造 AST 节点。

```rust
// 当前: 添加 variant = breaking change
pub enum ExprKind { ... }
// 应为:
#[non_exhaustive]
pub enum ExprKind { ... }
```

**影响**: 语义版本控制形同虚设。每次添加语法特性都需要 major version bump。
**当前状态（Implemented）**：公共 AST 枚举已加 `#[non_exhaustive]`，并在 v5.0.0 API cutover 中完成 workspace 匹配迁移。证据：`crates/n3v3-syntax/src/ast.rs`、`expr.rs`、`pattern.rs`、`types.rs` 的枚举定义。


### C3. 大量 `pub` 类型应改为 `pub(crate)`

以下类型标记为 `pub` 但从未被 crate 外部使用，泄露了实现细节：

| 类型 | 位置 | 建议 |
|------|------|------|
| `InferContext` | `typeck/src/infer.rs:12` | `pub(crate)` |
| `Substitution` | `typeck/src/unify.rs:14` | `pub(crate)` |
| `ThunkState` | `eval/src/value.rs:40` | `pub(crate)` |
| `PatternClass` | `eval/src/pattern.rs:125` | `pub(crate)` |
| `Discriminant` | `eval/src/pattern.rs:179` | `pub(crate)` |
| `LiteralValue` | `eval/src/pattern.rs:228` | `pub(crate)` |
| `MatchHints` | `eval/src/pattern.rs:255` | `pub(crate)` |
| `ModuleCache` | `hir/src/incremental.rs:15` | `pub(crate)` |
| `format_value` | `eval/src/builtin.rs:1847` | `pub(crate)` |
| `LinesChannelRx/BytesChannelRx` | `eval/src/value.rs:908-911` | `pub(crate)` |

### C4. README 语法与解析器不一致 — Implemented


原始发现（历史快照）：README 中多处使用已废弃的语法，与当时的 parser 行为不匹配：


| README 内容 | 实际 v4.0 语法 |
|-------------|---------------|
| `lazy { loadAllFromDisk() }` | `~(loadAllFromDisk())` |
| `save(...) effect = ...` | `save(...) = ...` (effect 自动推导) |
| 多处的 `then` 关键字 | `->` |
| `\|\| io.read(...)` 双竖线闭包 | `\|x\|` 单竖线 (不支持零参 `\|\|`) |

### C5. 所有示例文件使用过时语法 — Implemented


- 原始发现（历史快照）：10 个文件中 8 个使用 `import` (legacy) 而不是 `use` (canonical)
- 原始发现（历史快照）：全部使用 `then` 而不是 `->`
- 原始发现（历史快照）：部分使用 `as` 别名语法而非 `=`
- 原始发现（历史快照）：部分使用 `{ }` 记录语法而非当前语法

`learning/` 目录下的文件语法不统一是当时的审计观察；当前示例状态以 `examples/` 和 `scripts/check-docs.sh` 的检查为准。


**当前状态（Implemented）**：README 已使用 v4.0 语法（如 `~(...)`、`->`、自动推导 effect）；示例目录已迁移到 canonical `use`/`=` 语法。证据：`README.md:75-102`、`examples/`，以及 `scripts/check-docs.sh` 的 examples/snippets 检查。


### C6. Formatter 静默丢弃所有注释 — Implemented


**文件**: `crates/n3v3-fmt/src/format.rs`

原始发现（历史快照）：当时 AST 不包含注释信息(CST 的常见做法)，formatter 没有保留注释的策略；`test_comment_preserved` 当时是假阳性。
**当前状态（Implemented）**：parser 将 comment trivia 保留在 `SourceFile.comments`，formatter 按源码位置输出注释。证据：`crates/n3v3-fmt/src/format.rs:35-127`、`tests/parser.rs::test_parse_retains_comment_trivia`、`tests/fmt.rs::test_format_preserves_comment_text`。


### C7. `CacheStats` 命名冲突 — Implemented


**文件**: `hir/src/incremental.rs:98` vs `store/src/cache.rs:1505`

两个完全不同的 `CacheStats` struct 命名为同名，都是 `pub`。同时依赖 `n3v3-hir` 和 `n3v3-store` 的代码会产生歧义。
**当前状态（Implemented）**：HIR 统计类型已重命名为 `HirCacheStats`，避免与 store 的 `CacheStats` 冲突。证据：`crates/n3v3-hir/src/incremental.rs:97-105`。


---

## High (应尽快修复)

### H1. Phase D 残留: n3v3-eval 依赖未使用的 n3v3-parser — Implemented


**文件**: `crates/n3v3-eval/Cargo.toml:13`

`ast_eval.rs` 已删除但 `n3v3-parser.workspace = true` 仍在。edition 2024 对 lib target 的 `unused_crate_dependencies` 是 **deny-by-default**。
**当前状态（Implemented）**：`n3v3-eval` 已移除该未使用依赖；当前 workspace 依赖声明以 `crates/n3v3-eval/Cargo.toml` 为准。


### H2. n3v3-config 有 3 个未使用的依赖 — Implemented


**文件**: `crates/n3v3-config/Cargo.toml:10,13,15`

`n3v3-common`、`n3v3-lexer`、`n3v3-parser` 在 `n3v3-config/src/` 中没有被引用。同样是 edition 2024 的潜在编译错误。
**当前状态（Implemented）**：`n3v3-config` 已清理这三项未使用依赖；当前声明以 `crates/n3v3-config/Cargo.toml` 为准。


### H3. 泛型 trait bound 从未在调用点强制执行 — Implemented


**文件**: `typeck/src/check/mod.rs:2718`, `typeck/src/traits.rs:655-718`

`ConstraintSolver` 结构体存在但**从未被实例化**。当多态函数 `fn foo[T: Show](x: T) = ...` 被调用时，实际类型参数不检查是否满足 `Show` trait。trait bound 只检查关联类型(impl 层面)，函数调用点的泛型约束完全缺失。
**当前状态（Implemented）**：调用点现在检查泛型 trait bound；证据：`tests/end_to_end.rs::test_end_to_end_trait_bound_enforced` 与 `tests/typeck.rs::test_typeck_generic_trait_bounds_are_order_independent`。


### H4. `types_match` 忽略类型参数 — Implemented


**文件**: `typeck/src/traits.rs:361-383`

`List[Int]` 和 `List[String]` 在 trait impl 查找时被视为相同类型:
```rust
(TyKind::Named(id1, _), TyKind::Named(id2, _)) => id1 == id2,
```
如果存在 `impl Show for List[Int]` 和 `impl Show for List[String]`，会选错 impl。
**当前状态（Implemented）**：trait impl 匹配已覆盖类型参数，并有具体类型匹配回归测试。证据：`tests/typeck.rs::test_typeck_generic_trait_impl_matches_concrete_type`。


### H5. Enum 类型构造函数始终有空类型参数

**文件**: `typeck/src/check/mod.rs:1997,2023,2045,3439`

所有 enum 类型使用 `TyKind::Named(def_id, Vec::new())`(空泛型参数)。泛型 enum (如 `enum Result[A, E] { Ok(A), Err(E) }`) 的类型参数在类型检查期间被丢弃，导致 `Ok(42)` 和 `Ok("hello")` 可能被统一为相同的 enum 类型。

### H6. 42 个 Mutex 中毒风险点 — Implemented


**文件**: `eval/src/eval.rs`(5), `std/src/io/mod.rs`(30), `store/src/cache.rs`(7)

所有 `.lock().unwrap()` 调用在 mutex 中毒时会 panic 而不是恢复。spawn registry (`io/mod.rs` 中的 30 个) 风险最高 — 进程执行期间的线程 panic 会导致后续所有 spawn 操作失败。

**修复方案**: `lock().unwrap_or_else(|e| e.into_inner())` 或使用 `parking_lot::Mutex`(不中毒)。
**当前状态（Implemented）**：受影响的锁获取路径改为在 mutex poisoned 时恢复内部值，而不是直接 panic；证据：`crates/n3v3-std/src/io/mod.rs`、`crates/n3v3-eval/src/eval.rs`、`crates/n3v3-store/src/cache.rs` 中的 `unwrap_or_else(|e| e.into_inner())`。


### H7. `fork()` 在多线程环境中的安全性

**文件**: `builder/src/sandbox.rs:665`

Rust 程序中 `fork()` 可能死锁 — 如果 fork 时另一个线程持有锁，子进程继承锁的状态。sandbox 在 fork 后立即 exec，这减轻了风险但不能完全消除。需要审计哪些锁在 fork 时刻可能被持有。

### H8. 无递归深度限制 — Implemented


**文件**: `eval/src/eval.rs`

TCO 处理了尾递归，但非尾递归函数(如 `fn sum(n) = if n == 0 -> 0 else n + sum(n - 1)`)没有深度计数器，中等输入就会栈溢出。
**当前状态（Implemented）**：求值器同时维护非尾调用深度和原生栈预算，预算耗尽返回可诊断错误。证据：`crates/n3v3-eval/src/eval.rs:220-261` 及 `test_non_tail_recursion_reports_stack_budget_instead_of_aborting`。


### H9. occurs check 在动态/安全记录字段中被绕过 — Implemented


**文件**: `typeck/src/check/mod.rs:1459-1560`

`constrain_dynamic_record_field`、`constrain_safe_record_base`、`extend_safe_record_base` 直接调用 `self.subst.extend(var, new_ty)`，绕过了 `unify()` 中的 occurs check。可能产生无限类型而没有任何诊断。
**当前状态（Implemented）**：动态/安全记录约束统一经由带 occurs check 的 `extend_subst`。证据：`crates/n3v3-typeck/src/check/mod.rs:1676-1778`、`crates/n3v3-typeck/src/unify.rs:391-408`。


### H10. LSP hover 中类型名显示为 "T"

**文件**: `lsp/src/backend.rs` `def_id_to_name_hint`

用户自定义类型的 hover/inlay hint 全部显示 `"T"` 而非实际类型名。`def_id_to_name_hint` 返回硬编码的 `"T".to_string()`。

### H11. Formatter 不在超长行换行

**文件**: `fmt/src/format.rs`

`would_exceed_width` 检查只对 Record 和 List 进行。函数调用、二元表达式、match 表达式超过 `max_width=100` 时不会换行，直接产生超长输出。

### H12. `path.to_str().unwrap()` 在非 UTF-8 路径上 panic — Implemented


**文件**: `cli/src/commands/fmt.rs:120`

Linux 路径是任意字节序列。非 UTF-8 路径会直接 panic CLI。
**当前状态（Implemented）**：formatter CLI 使用 `to_string_lossy()` 处理路径。证据：`n3v3-cli/src/commands/fmt.rs:119-123`。


### H13. Ctrl+C 在 REPL 求值期间无保护

**文件**: `cli/src/commands/repl.rs:262-287`

`evaluate_repl_input` 同步执行期间按 Ctrl+C 会直接杀死进程(无历史保存)。应注册 SIGINT handler 或在求值循环中检查信号。

---

## Medium (应该修复)

### M1. n3v3-derive 依赖未使用的 n3v3-common
`crates/n3v3-derive/Cargo.toml:10` — edition 2024 潜在编译警告/错误。

### M2. n3v3-store 依赖未使用的 n3v3-common
`crates/n3v3-store/Cargo.toml:10` — 同上。

### M3. n3v3-fetch 重复声明依赖而非使用 workspace ref
`tar`、`flate2`、`xz2`、`tempfile`、`blake3`(未使用)在 `n3v3-fetch/Cargo.toml` 中直接写版本号。

### M4. n3v3-std 和 n3v3-cli 有未跟踪的孤立依赖
`glob`、`rpassword`、`termimad` 不在 workspace `[dependencies]` 中，版本漂移风险。

### M5. n3v3-builder nix 特性重复声明
直接写了完整的 `nix = { version = "0.29", features = [...] }` 而非 `nix = { workspace = true }`。

### M6. `instantiate` 中的 Forall 参数绑定依赖脆弱的名称匹配
`unify.rs:380-401` — 通过 `"t0"` 前缀和 `parse::<u32>()` 解析参数名。

### M7. SafeRecordBase/DynamicRecord 在穷举性分析中未处理
`pattern_analysis.rs:627-630` — match 穷举性检查不覆盖安全记录访问和动态记录。

### M8. Thunk 在二进制/比较操作中未 force
`eval.rs:913-1065` — 如果 Thunk 直接到达算术操作会得到奇怪的报错而非自动求值。

### M9. 信号检查只在 TCO 循环中
`eval.rs:1126` — `map`/`fold`/`filter` 等长时间运行的内置函数不检查信号，求值器不可中断。

### M10. 同一作用域内无重复名称检测
`hir/src/resolve.rs:755-761` — `define_local` 静默覆盖已存在的绑定。

### M11. 名称解析未找到时的错误路径不一致
未解析的简单名称 → `Builtin(name)`，未解析的路径 → `Global(DefId(u32::MAX))` — 用户得到不同质量的错误信息。

### M12. top-level let 总是标记为 effectful
`hir/src/resolve.rs:1061-1083` — `effectful: true` 硬编码，意味着模块级 let 绑定总是允许副作用。

### M13. eval 和 typeck 各自独立收集 VariantCtor 信息
`eval.rs:374-384` vs `check/mod.rs:2028-2053` — 没有共享的事实来源，可能产生不一致。

### M14. `:cd` 改变全局 CWD 但不更新语义状态
`repl.rs:235` — REPL 中 `:cd` 后模块解析可能用错误路径。

### M15. LSP `did_change` 无防抖
每次按键触发完整的重新分析和诊断发布，大文件性能差。

### M16. CLI 错误消息丢弃诊断细节 — Implemented

`eval.rs:20` — `return Err("parse error")` 不报告错误数量/位置/内容。
原始发现（历史快照）：`eval.rs:20` 曾只返回 `parse error`，不报告错误数量、位置或内容。当前 CLI 会先输出诊断，再返回带计数的错误上下文；证据：`n3v3-cli/src/commands/eval.rs:16-21`、`86-103`。

### M17. 所有错误使用 exit code 1
无区分：parse error / type error / runtime error / IO error 全返回 1。

### M18. `debug_assert!` 用于缩进平衡检查
`fmt/src/format.rs:51` — release build 中不平衡缩进静默产生损坏输出。

### M19. NAR 路径穿越测试是空桩
`store/src/nar.rs` — 测试函数构建了恶意 header 但没有实际提取验证。

### M20. typeck 核心模块零单元测试
`infer.rs`、`unify.rs`、`traits.rs`、`pattern_analysis.rs` 仅通过集成测试覆盖，修改风险高。

### M21. eval 单元测试极少
`eval.rs` 仅有 1 个 `#[test]` 函数，核心求值逻辑完全靠集成测试。

### M22. parser shebang 测试已移入实现路径 — Implemented
parser 与 frontend 现在都会剥离 shebang；证据：`crates/n3v3-parser/src/lib.rs:38-43`、`crates/n3v3-frontend/src/driver.rs:145-149`、`tests/parser.rs::test_parse_shebang`。


---

## Low (技术债务)

### L1. tree-sitter-n3v3 不在 workspace 中 — Implemented
`tree-sitter-n3v3/bindings/rust/` 已加入 workspace `members`，会参与构建、测试和 lint。证据：根 `Cargo.toml:7-13`。


### L2. tree-sitter-n3v3 build.rs 引用不存在的 parser.c — Implemented
生成的 `tree-sitter-n3v3/src/parser.c` 已纳入版本库，Rust `build.rs` 会检查并编译该文件。证据：`tree-sitter-n3v3/bindings/rust/build.rs:2-20`、版本库中的 `tree-sitter-n3v3/src/parser.c`。


### L3. `panic = "abort"` 可能破坏依赖的 catch_unwind
`Cargo.toml:28` — release profile 设 `panic = "abort"`，某些依赖(如 tokio)内部用 `catch_unwind`。

### L4. `u64 -> usize` 截断风险(32-bit)
`store/src/nar.rs:384` — 32 位平台 >4GB NAR 文件会截断。

### L5. Rc 指针强制转换为 usize 用于哈希
`eval/src/value.rs:1396-1484`, `eval/src/env.rs:119` — 依赖地址稳定性，脆弱但当前安全。

### L6. CI: 无覆盖率测量、无 benchmark、无 fuzz testing — Planned (open)

没有 `cargo-tarpaulin`/`cargo-llvm-cov`，没有 criterion benchmark，没有 `libfuzzer-sys`。

### L7. CI: Windows 跳过大部分集成测试
builder、config、fetch、frontend、lsp、store、fmt 集成测试在 Windows 上不运行。

### L8. CI: `cargo audit` 用 `continue-on-error: true`
依赖漏洞不阻塞 CI 通过。

### L9. CI: 文档测试已覆盖；MSRV 检查仍未声明 — Partially implemented
`cargo test --doc --workspace` 已加入 `.github/workflows/ci.yml:51-53`，并由 `scripts/validate.sh` 的 test gate 调用；但 workspace `Cargo.toml` 仍未声明 `rust-version`，MSRV 检查保持 open。


### L10. `FnDef.effect` 字段仍保留在 AST 中但关键字已移除
`n3v3-syntax/src/ast.rs:67` — effect 关键字已成为 legacy，但 AST 还带着这个字段。

### L11. n3v3-config ServiceUnit 用 String 而非 enum
`config/src/generate.rs:39` — `service_type: String` 允许任意字符串，应为 `ServiceType` enum。

### L12. n3v3-std I/O 有 14 对重复函数
`readFile`/`readFilePath`、`writeFile`/`writeFilePath` 等 — 两套 API 做同样的事。

### L13. n3v3-cli 有孤立的 `n3v3-lexer` 依赖 — Implemented

`n3v3-cli/Cargo.toml:15` — CLI 不直接使用 n3v3-lexer，但声明了依赖(edition 2024 bin target 是 warning)。
**当前状态（Implemented）**：CLI 已移除该孤立依赖；当前依赖声明以 `n3v3-cli/Cargo.toml` 为准。


### L14. REPL 历史保存错误静默丢弃
`repl.rs:310, 106` — `let _ = save_repl_history(...)` 丢弃所有错误。

### L15. LSP 缺失 did_change_configuration/did_change_watched_files — Implemented (stub)
`Backend` 已实现这两个通知方法（接受事件但不动态应用配置或文件变更）；证据：`crates/n3v3-lsp/src/backend.rs:265-274`。当前限制是修改配置或 watched files 后仍需重启服务器。


### L16. LSP code_action 只生成无操作的标签
`backend.rs:1070-1097` — 快速修复空有其表，没有任何实际编辑。

### L17. `n3v3 test` 和 `n3v3 run` 缺少 doc comment
`test.rs` (0 docs), `run.rs` (0 docs) — `n3v3 test --help` 和 `n3v3 run --help` 无帮助文本。

### L18. `--version` flag 和 `version` 子命令输出不一致

`main.rs:125,377-381` — `n3v3 --version` 和 `n3v3 version` 输出不同格式。
## 本次工作流与文档标准化（2026-09-25）

本次台账同步遵循仓库现有的单一事实源，不在本报告复制标准全文：

- `scripts/validate.sh`：唯一质量闸门入口，统一 secrets、format、lint、build、deps、test、CLI smoke、docs 和 skills gates。
- `scripts/counts.sh`：当前版本与计数的规范事实源。
- `scripts/check-docs.sh`：文档链接、版本、计数、registry、示例和 `n3v3-check` 片段的机械校验器。
- [`.claude/workflows/README.md`](workflows/README.md)：工作流原语、证据边界和失败传播契约。
- 文档标准见 [`docs/contributor/contributing.md`](../docs/contributor/contributing.md) 的 Documentation Standards，开发规则见 [`rules.md`](rules.md)；本报告不重复这两处全文。

旧计数已按规范事实更正：547/550 → 554 E2E tests，21 → 26 LSP methods，14 → 13 Stream<T> APIs。当前还应以 `scripts/counts.sh` 输出为准（包括 55 diagnostic codes、12 canonical keywords、21 Lean modules）。

---

## 影响 E2E 的开放问题

当前仍需补充或继续核对的 E2E 行为：

| 问题 | 当前状态 | 下一步 |
|------|---------|--------|
| H5 (enum 泛型参数完整性) | Planned / open | 添加泛型 enum 类型安全回归 |

已完成的 H3、H4、H8、H9 和 C6 已从当前 gap 表移除；其历史覆盖记录保留在本报告的审计快照中。



---

## 修复状态路线图

### Implemented

- C1：workspace `reqwest` TLS 配置已统一。
- C2：公共 AST 枚举已使用 `#[non_exhaustive]`。
- C4/C5：README 与示例已迁移到 v4.0 canonical syntax。
- C6：formatter 已保留 comment trivia。
- C7：缓存统计类型已重命名为 `HirCacheStats`。
- H1/H2、M1/M2/M3/M5、L13：未使用或重复依赖已清理。
- H3/H4/H6/H8/H9/H12、M16：类型约束、锁恢复、栈预算、occurs check、路径和诊断细节已有代码或测试证据。
- L1/L2：tree-sitter binding 已加入 workspace，生成的 `src/parser.c` 已提交并被 build script 检查。
- L9 的 `cargo test --doc` 子项已进入 CI；L15 的两个 LSP 通知已实现为明确的 stub。
- `io.tempDir`：Implemented。typeck `(Path -> A) -> A` 声明保持不变；evaluator-owned dispatch 现在由 `n3v3-eval/src/eval.rs:1712-1717` 调用 `builtin_temp_dir`，回调返回原值并在成功/失败后尝试清理目录（`eval.rs:2658-2678`）；std 侧保留 stub（`n3v3-std/src/io/fs.rs:1550-1555`）。证据：`tests/end_to_end.rs::test_end_to_end_io_temp_dir_returns_value_and_cleans_up`、`test_end_to_end_io_temp_dir_is_effectful`。

### Planned / open

- C3：仍需继续收敛不必要的公开类型。
- H5：enum 泛型参数完整性仍需继续核对。
- L3、L6-L8、L10-L12、L14、L16-L18：对应构建、CI、API 和交互体验债务仍开放。
- MSRV / `rust-version`：workspace 尚未声明最低 Rust 版本。
- `json_to_value`：外部 JSON 递归深度仍无显式上限。
- `kill_process`：PID 复用窗口仍未消除。
- `n3v3-typeck` trait 回退：多个候选 impl 时首个 HashMap 条目的选择仍不确定。
- `tree-sitter-n3v3/grammar.js`：仍保留 v3.x 形态，尚未与 v4.0 语法完全收敛。
- `io.readFileLines` 及其 Path 变体：typeck 保留两个 callback → `Unit` 声明（`crates/n3v3-typeck/src/check/builtin_type.rs:1040-1062`）。String 与 Path 变体均有真实 E2E 覆盖（`tests/end_to_end.rs::test_end_to_end_io_read_file_lines_calls_callback`、`test_end_to_end_io_read_file_lines_path_reads_file` 及 missing-file error parity）；std 注册表保留 evaluator-owned Path stub，canonical HIR evaluator 负责实际 Path 读取。判定：两种变体均已实现；不要把 std stub 误判为不可用。
- `io.streamLines` / `io.streamBytes`：typeck/docs 契约仍将输入声明为 `String`（`crates/n3v3-typeck/src/check/builtin_type.rs:1459-1472`）；runtime-only 路径在 `crates/n3v3-std/src/io/fs.rs:1577-1586,1706-1714` 额外接受 `Path` 值。判定：以 typeck 声明为准，这是保持 Planned / open 的接受态扩展，不引入未声明的类型联合。
- `io.jobs` / `io.waitAnyJob`：runtime 已存在（`crates/n3v3-std/src/io/mod.rs:814-874`），文档和 E2E 也存在（`docs/reference/stability.md:215-216`；`tests/end_to_end.rs::test_jobs_returns_list`、`test_wait_any_job_returns_result`、`test_task_multiple_spawn_jobs_list`、`test_task_wait_any_job_with_spawn`）。但 `crates/n3v3-typeck/src/check/builtin_type.rs` 没有对应 typed declaration；同文件相邻的 `io.spawn` / `io.awaitAny` 声明（`builtin_type.rs:1389-1424`）说明这不是有意未声明。判定：漏声明而非有意未声明，Experimental until typed declaration is added。


---

## 分数卡（2026-06-17 历史基线）

| 维度 | 评分 | 说明 |
|------|------|------|
| 架构设计 | **B+** | 管道架构清晰，依赖图是无环 DAG。扣分: 依赖管理不干净 |
| 类型安全 | **B** | H-M + Trait 基础扎实。扣分: trait bound 未强制，enum generic 不完整 |
| 运行时安全 | **B-** | 无 unsafe 滥用，TCO 实现正确。扣分: 无递归限制，mutex 中毒风险 |
| 错误处理 | **C+** | 生产代码 `?` 传播一致。扣分: 429 个 unwrap, CLI 错误消息质量 |
| 测试覆盖 | **B** | 541 E2E 覆盖广。扣分: 核心模块缺单元测试，无 benchmark/fuzz |
| API 设计 | **C+** | 管道 facade 模式干净。扣分: AST 未密封，大量 pub 泄露 |
| 文档质量 | **C** | 结构完整。扣分: README 语法漂移，示例全部过时 |
| 开发体验 | **B-** | LSP 20 methods, REPL 实用。扣分: 错误消息不精确，comment 被丢弃 |
| **综合** | **B-** | 语言核心健壮，工程化不足。建议重点投入类型系统正确性和 API 稳定性 |

---

## AST/HIR/IR Review (2026-09-25)

按项目 v4.0/v5.0 语法设定重新核对 lexer → parser → AST → HIR → typeck → eval 全链路，重点检查 AST 解析覆盖与 IR 完整性。发现并修复：

| # | 位置 | 问题 | 修复 | 验证 |
|---|------|------|------|------|
| 1 | `n3v3-lexer/src/lexer.rs` | `/` 一律按绝对路径字面量扫描，`6/2` 被切成 `PathLit` | 记录 `last_token_kind`，仅在上一 token 不能结束操作数时进入路径扫描 | `tests/lexer.rs::test_compact_division_is_not_an_absolute_path` |
| 2 | `n3v3-hir/src/resolve.rs` | 枚举变体与已有全局同名时静默覆盖；与枚举自身同名被误报为重复变体 | 按 `(enum DefId, span)` 记录变体，冲突时给出精确诊断 | `tests/typeck.rs`（self-name / duplicate 两例） |
| 3 | `n3v3-eval/src/eval.rs` | 零参数项求值语义与 typeck 不一致（显式 `fn f() = …` 被当作可调用对象） | 统一为值绑定语义；移除临时引入的 `FnDef.is_binding` 字段 | `cargo test --workspace`、typeck 一致性 |
| 4 | `n3v3-eval/src/eval.rs` | `io.defer` 只在程序结束时统一执行，失败路径不执行 | 延迟动作归属调用帧，帧退出（成功或失败）即执行，并恢复外层作用域 | `tests/end_to_end.rs` 三个 defer 测试 |
| 5 | `n3v3-eval/src/value.rs` + `eval.rs` | thunk 失败后被缓存为 `Evaluated(Err(..))`，再次 `force` 得到 `Ok(Err(..))` 且状态与错误不一致 | 新增 `ThunkState::Failed(EvalError)`，失败结果原样缓存 | `eval::tests::test_force_caches_thunk_failure` |
| 6 | `n3v3-eval/src/eval.rs` | `ExprKind::MethodCall` 不在 `eval_with_tco` 分支中，方法调用尾递归退化为普通递归 | 方法调用解析拆分为 `resolve_method_call`，尾位置返回 `TcoResult::TailCall` | `tests/eval.rs::test_eval_method_call_tail_recursion_is_optimized`（11_000 层） |
| 7 | `n3v3-eval/src/eval.rs` | `MAX_RECURSION_DEPTH = 10_000` 实际不可达：单帧数 KB，约 1_000 层即原生栈溢出并 `abort`（debug/release 均复现） | `apply` 测量当前线程可用栈（`pthread_getattr_np`，512 KiB 保留，探测失败回退 512 KiB），预算耗尽即返回可诊断错误 | CLI 复现转为 `E0303`；`eval::tests::test_non_tail_recursion_reports_stack_budget_instead_of_aborting` |
| 8 | `n3v3-lsp/src/semantic_tokens.rs` | 列号按字符计数、长度按字节计算，astral 字符使同行后续 token 偏移 | `offset_to_line_col` / `utf16_length` 统一按 UTF-16 码元计算 | `tests/lsp.rs::test_semantic_tokens_positions_use_utf16_code_units` |
| 9 | `tests/end_to_end.rs` | `test_defer_execution_order` 为恒真断言（`assert!(ok || err)`） | 删除并替换为真实行为测试 | 同上 |

未修复（记录为后续边界）：

| 位置 | 现象 | 影响 |
|------|------|------|
| `n3v3-lexer` | 非 ASCII 标识符被拒绝（`E0001 unexpected character`） | 语言词汇表限定为 ASCII 标识符；如需 Unicode 标识符需单独立项 |
| `n3v3-eval` | 表达式深度（非 `apply` 路径）不受栈预算约束 | 极深嵌套表达式仍可能触及原生栈；当前由解析器先行失败覆盖 |
| `n3v3-typeck` / `n3v3-eval` | 零参数 `fn f() = …` 是值绑定，`f()` 不是合法调用 | 与 Rust 直觉不同，属既有语义；已在 changelog 与技能文档中明确 |

**安全复审（2026-09-25，security-reviewer，只读）**：本批次新增的 3 处 `unsafe`（`n3v3-common::kill_process` 的 `libc::kill`、`n3v3-eval` 的栈探测与信号处理、tree-sitter 的 `unsafe extern "C"`）逐条核对无越界读写、无 UB，平台回退为 fail-safe（预算偏保守，不会放过溢出）。无 critical/high。cargo audit / cargo deny / trivy / gitleaks 均通过（`validate.sh` Gate 1/5）。两条 low 属既有代码，**不由本批次引入**，留待后续：

| 位置 | 现象 | 建议 |
|------|------|------|
| `crates/n3v3-eval/src/builtin.rs` `json_to_value`（`builtin.rs:1637-1730`，`fromJSON` 入口） | 递归解析外部 JSON 无深度上限；不经过 `Evaluator::apply`，因此新增的栈预算守卫覆盖不到，深层嵌套 `[` 可在解析期耗尽原生栈 | 增加显式深度上限（例如 128）并返回可诊断错误；`value_to_json` 同样处理；加测试固定上限 |
| `crates/n3v3-common/src/lib.rs` `kill_process`（`lib.rs:30-40`） | `pid as i32` 隐式截断改变 `kill` 语义（Linux `pid_max ≤ 2^22`，实际不可达）；超时 kill 与 `child.wait_with_output()` 回收之间存在 PID 复用窗口 | `libc::pid_t::try_from` + 失败跳过；或由持有 `Child` 的线程终止 / Linux 上用 `pidfd_open` |

其他 info 级建议（不阻塞）：`install_signal_handler` 忽略 `libc::sigaction` 返回值；`DepthGuard` 的裸指针可改用 `&Cell<u32>` 持有以去掉 `unsafe`；注册 `io.onSignal` 会取代 TERM/HUP 的默认终止语义，需在文档中说明。

**独立审查（2026-09-25，agent://reviewer，只读）**：`CHANGES REQUESTED`，两条 Major 均属本次新增的按帧 `io.defer` 语义，已修复并补齐失败前/通过后的回归证据：

| # | 位置 | 问题 | 修复 | 证据 |
|---|------|------|------|------|
| 1 | `eval.rs` `eval_module` / `force_thunk` | 模块帧与 thunk 帧注册的 defer 永不执行（相对旧的“程序结束统一执行”是静默回归） | 模块第二遍求值包在 take/settle/restore 中；thunk 子求值器在丢弃前 settle | `test_defer_runs_at_module_scope`、`test_defer_runs_inside_thunk_frame`（禁用修复即失败） |
| 2 | `eval.rs` TCO 循环 | 尾位置的 defer 在被调用者执行前、以 FIFO 顺序运行，与同构非尾调用相反 | 帧 defer 挂起到 `pending_frame_defers`，链结束时由内层向外层 flush | `test_defer_order_matches_tail_and_non_tail_calls`、`test_defer_runs_when_tail_called_frame_fails`（禁用修复即失败） |
| 3 | `lexer.rs` `can_start_operand` | 漏掉后缀 `?`，`total?/2` 被词法成 `PathLit("/2")` | 操作数结束集合加入 `Question` | `tests/lexer.rs::test_slash_after_try_operator_is_division` |
| 4 | `eval.rs` `run_defers` | 单个 defer 失败会跳过同帧其余 defer，函数注释却称“全部执行” | 全部尝试，报告首个错误 | 与 #2 的测试共用路径 |
| 5 | `tests/end_to_end.rs` | `test_end_to_end_bytes_len` 恒真断言掩盖了源码缺少 `use std.io = io`（该用例从未真正通过类型检查）；retry 用例同样恒真 | bytes 用例改为 TempDir 真实读取并断言 `bytes.len(data) > 0`；retry 用例断言类型检查干净且 HIR 求值成功 | 两用例现均为真实断言并通过 |
| 6 | 文档/配置 | spec 的“标识符为 ASCII”与实现（ASCII 起始 + Unicode 续字符）矛盾；技能表引用不存在的 `n3v3-lexer/src/span.rs`、`n3v3-syntax/src/item.rs`、`n3v3-parser/src/{expr,pattern}.rs`；`.editorconfig` 对生成的 `parser.c` 用 4 空格；`semantic_tokens.rs` 存留死变量与失实注释 | 按实现改写 spec 与技能；表内路径改为实际文件并纳入 `verify-skills.sh` 校验；`parser.c` 单独声明 2 空格；删除死变量并修正注释 | `verify-skills.sh` 17 → 25 项检查全通过 |

**新增后续边界（非本批次引入，未修复）**：

| 位置 | 现象 | 影响 / 建议 |
|------|------|-----------|
| `n3v3-typeck/src/traits.rs` `resolve_method` trait 回退分支 | 在 `trait_impls`（HashMap）上取首个匹配的 impl，迭代顺序随进程随机种子变化 | 多个候选 impl 且接收者仍是类型变量时，方法选择不确定且被写进运行时分派表；需确定性排序 + 多匹配歧义诊断。该循环早于本批次存在（`git diff --cached` 中为上下文行），本批次的替换匹配扩大了可匹配面 |
| `crates/n3v3-eval/src/diagnostics.rs` | 递归上限与栈预算以 `TypeError` → E0303 “runtime type error” + “加个类型标注”帮助呈现，且 >80 字符的消息被截断 | 建议为递归/栈耗尽增加独立 `EvalError` 变体与 `ErrorCode`（会新增第 56 个错误码，需同步 codes 表与文档）；本批次先把消息缩短到不被截断 |
| `tree-sitter-n3v3/grammar.js` | 仍是 v3.x 形态（`if … then … else`、`lazy`、强制 `{ … }` 枚举、无后缀 `?`），而 `src/parser.c` 已随本批次重新生成 | 编辑器语法与编译器行为分歧；需按 v4.0 规范补规则并重新 generate |

## AST/HIR/Action IR Review (2026-09-30)

在前一轮 AST/HIR 缺陷修复的基础上，重新核对当前 v4/v5 canonical syntax
与 `Parser → AST → HIR → Typeck → Eval` 边界，并单独检查 Action/Process/Stream
是否已经形成独立 IR。

结论：

- AST 与 HIR 仍是当前唯一 canonical 表示；解析器支持的表达式、模式、类型、
  顶层项在 `n3v3-syntax` 与 `n3v3-hir` 中有对应变体，unsupported 节点保留为
  error 节点并由 frontend/typeck 报告，未发现新的静默 wildcard 降级。
- 当前没有独立的 Action/Process/Job/Stream IR。`Command`、`Pipeline`、`Task`、
  `Stream`、`Event` 和 `Live` 仍由 HIR builtin 调用与 evaluator runtime value
  表示；因此不能把 HIR 误称为完整 Action IR。
- 为收敛边界，新增了不改变默认求值语义的最小 shadow path：共享 intrinsic
  metadata/`EffectSummary`、`HostOpKind`，以及 canonical HIR 上的
  `ModuleSemantics.action_plans`。当前仅将字面量
  `io.readFile("path")` 规划为 `HostOp::ReadFile`；动态路径不生成伪计划。
  `check`、LSP、formatter 和分析路径只收集计划，不执行宿主操作。
- `intrinsic_metadata` 的复核发现并修正了 stream 构造器、TTY 状态查询以及
  `liveCurrent`/`liveCancel` 的纯效果漂移；当前 registry 不再让这些 runtime
  宿主操作绕过 effect gate。Stream runtime 仍在构造阶段创建线程/打开文件/
  spawn/读取计时器，因此 cold Stream 是明确的后续边界，而不是已完成事实。
- `OsHost` 改为 root-scoped host：拒绝绝对路径、`..` 和越过 root 的 symlink；
  `HostError` 使用转义路径显示并保留底层错误 source。
- action plan 只在无阻断诊断的分析中发布；对于多模块/REPL snippet，
  frontend 会在任一模块出现阻断诊断时从所有暴露的 `ModuleSemantics` 中清空
  plans。它仍是 inspection-only side table，不是完整 host-op allowlist，也不覆盖
  动态路径和运行时控制流。

本轮实现证据：`cargo test -p n3v3-common --lib action::tests`、`cargo test
--test action_plan`、`cargo test --test frontend -- --nocapture`、`cargo test
--test frontend_driver -- --nocapture`、`cargo test --test frontend_session --
--nocapture`。完整 workspace 与项目质量门需以本次交付前的实际运行结果为准。

## Phase 3 ProcessPlan Adapter Slice

本轮将 Process/Job 迁移推进到一个不改变语言公开契约的内部切片：

- `n3v3-common::ProcessPlan`、`ProcessStage`、`ProcessRedirect` 和
  `ProcessStream` 只保存拥有所有权的进程描述，不持有 `Child`、线程或其他
  宿主资源，也不执行宿主操作。
- `n3v3-std` 从现有 `CommandValue`/`PipelineValue` 做唯一 lowering，保留
  stage 顺序、参数、cwd、stdin、环境和嵌入/边界 redirect；阻塞的
  `io.execCommand`、`io.execPipeline`、普通 `Task` await 以及
  `io.execCommandLines` 通过计划阶段执行。
- 旧 `ProcessResult` 非零退出语义、redirect 拓扑校验、危险环境变量过滤、
  stdin/output 限制和入口错误前缀未被改写。计划构造不读 stdin redirect、
  不创建输出文件、不启动进程。
- 本切片没有把计划加入 `Value`，没有切换 `Task` 为 `Job<T>` 或删除整数
  spawn ID，也没有扩展 frontend `action_plans`、Action 语法、cold Stream、
  evaluator-owned streaming、timeout、await-any、spawn 或 Event/Live。

该切片是可回滚的执行层迁移，不代表独立 Action/Process/Job/Stream/Reactive
IR 已完成。下一步仍需单独定义 typed Job 生命周期、可取消执行、timeout/
spawn adapter、cold Stream 和 reactive 边界，再进行公开 API cutover。

---



## 最新验证状态 (v5.0.0)

**Current status (2026-09-30):** The current product facts are v5.0.0, 558 E2E tests, 26 LSP methods, 55 diagnostic codes, 12 canonical keywords, 13 Stream<T> APIs, and 21 Lean modules; `scripts/counts.sh` is the source for these values. The v5.0.0 AST boundary cutover is implemented, comment trivia is retained by the parser/formatter, L1/L2 and the doc-test part of L9 are implemented, L15 is an explicit stub, and `io.tempDir` is implemented through evaluator-owned dispatch. Open follow-up boundaries are the undeclared MSRV, CI coverage/benchmark/fuzz gaps, JSON recursion depth, PID reuse during `kill_process`, nondeterministic trait fallback selection, the v3.x-shaped tree-sitter grammar, the runtime-only Path acceptance for `io.streamLines` / `io.streamBytes`, missing typeck declarations for `io.jobs` / `io.waitAnyJob` (Experimental until typed declaration is added), and independent Process/Job/Stream IR beyond the current `io.readFile` action-plan shadow path.

The release fixes documented above remain historical evidence. New changes MUST be validated against the current source and targeted behavior, not this ledger alone.

**Historical follow-up note (2026-09-23):** `Cargo.lock` was refreshed and `git2` was upgraded to `0.21.0` for current RustSec advisories. The canonical pipeline then had 2,690 passing workspace tests; the custom enum coalesce evaluator regression was fixed and covered. The note is retained as review history, not as the current count source.

---

## 评审修复记录 (2026-09-25 · 第二轮)

范围：AST/IR 解析正确性、文档与技能事实一致性、质量闸门真实性、格式化器往返。
证明方式：每条修复都有可复跑的命令或负对照，不以静态检查冒充行为证明。

### 已修复并验证

| 问题 | 证据 | 修复 |
|---|---|---|
| `scripts/validate.sh` deps 闸门恒 PASS：`if ! output+=$($command 2>&1; printf '\n')` 只反映最后一条 `printf` 的状态 | 负对照：把 `trivy` 换成 `exit 1` 的假二进制，修复前仍 `[PASS]`，修复后 `[FAIL] deps` | 改为先 `chunk=$($command 2>&1)` 再判定状态；`shellcheck` 干净 |
| `scripts/check-docs.sh` 空输入假 PASS（examples 0 匹配、n3v3-check 抽取为空） | 突变测试：把 `find examples -name '*.n3v3'` 改为不匹配 → `[FAIL] examples: no *.n3v3 file matched`；把 awk 的 `inside = 1` 改为 `0` → `[FAIL] snippets: 11 fence(s) found but none extracted` | 两个守卫；未突变时 `0 failed`、`11 n3v3-check block(s) pass` |
| `.claude/hooks/fmt-all.sh` 用 `n3v3 fmt file <path>`（只写 stdout）判定格式 → 除崩溃外恒 PASS | 植入未格式化文件后修复版 `exit=1` 并打印 `Would reformat` | 改用 `n3v3 fmt check`；文案与注释同步 |
| 格式化器打印 `[x, ..]` 时丢分隔符 → 输出 `[x..]` 无法解析 | `n3v3 fmt file` 后 `n3v3 check` 报 E0101；修复后 `[h, ..]` 保留且可解析 | `crates/n3v3-fmt/src/format.rs` 的分隔符改为只看 `init`；测试 `test_format_list_rest_pattern_keeps_separator` |
| 格式化器把零参 lambda 打印为 `||`，而 `||` 是逻辑或 token → 输出无法解析 | `n3v3 check` 对 `\|\| { 1 }` 报 E0101，对 `fn() { 1 }` 通过 | 空参数改为 `fn()`；测试 `test_format_zero_param_lambda_is_parseable` |
| 格式化器把记录体 lambda 打印成 `\|x\| { ... }`，而 lambda 体位置的 `{` 是块 → 多字段即 E0101（`examples/flake.n3v3` 受影响） | `outputs = \|inputs\| {` + 多字段 → E0101；加括号后 `({ ... })` 可解析且幂等 | 记录体加括号；测试 `test_format_lambda_record_body_is_parenthesized` |
| 词法错误被计为类型错误：`n3v3 check` 对 `let x = "a\q"` 报 “1 type error(s)” 且退出 3（契约：2=parse、3=type） | 同上命令，修复前 `EXIT=3`，修复后 `EXIT=2` + `parse error: 1 diagnostic(s)` | 新增 `DiagnosticKind::is_syntax()`；`check.rs`、`eval.rs`、frontend `collect_diagnostic_stats` 统一使用 |
| `n3v3 run` 语法错误退出 1（消息以计数开头，不匹配 `main.rs` 的前缀映射） | 修复前 `EXIT=1`，修复后 `EXIT=2` | `run.rs` 消息改为 `parse error: N diagnostic(s)` |
| `io.readFileLinesPath` 的 typeck 注释声称 evaluator 会拒绝其 Path 形式（与代码相反） | dispatch `crates/n3v3-eval/src/eval.rs:1724-1729` → `builtin_read_file_lines_path`（要求 `Value::Path`）；`path.fromString` + `path.joinPath` 实测输出 `path-variant-ran`、exit 0 | 注释改为事实描述；补两个 E2E（成功 + 不存在文件的错误路径） |
| `test_end_to_end_io_temp_dir_is_effectful` 断言 `!has_effect_error`，而该诊断只在 lambda 作用域产生 → 无法失败 | 副作用闸门实际位于 CLI（`check.rs`），前端测试无法观察 | 删除该测试；在 `n3v3-cli/tests/cli_exit_codes.rs` 增补 CLI 级副作用断言 |
| `release.yml` 绕过唯一闸门入口，且 tag 推送不触发 `ci.yml`（`on.push.branches` 只作用于分支）→ 发布产物可能来自未跑测试的提交 | 工作流文件对比；`validate.sh --release` 已内建发布模式文档检查 | 发布闸门改为调用 `scripts/validate.sh --release --strict`；工具安装抽为 `scripts/install-gate-tools.sh`（CI 与发布共用，版本单源） |
| `--quiet` 帮助文案 “Suppress output” 与实现不符（只抑制最后一行 `error:`） | `n3v3 check --quiet` 仍打印诊断与 `[ERROR]` 行，仅省略 `error:` 行；退出码不变 | 文案改为 “Suppress the final error message line; exit codes are unchanged.” |
| 文档/技能事实过时 | `check-docs.sh` 的计数校验 + 实测 | `CLAUDE.md` 计数 554→556、钩子清单去掉已删除的 `save-test-baseline`、补 `install-gate-tools.sh`；`n3v3-fmt` 技能补往返规则、删除“丢弃注释(C6)”的过时说法、记录 shebang 限制 |
| `--release` 模式的 CLI 闸门可能验证陈旧二进制：`gate_build` 只跑 `cargo check`（不产出二进制），`driver.sh` 在二进制已存在时不再构建，`check-docs.sh` 固定使用 `target/debug/n3v3` | 修复前 `target/release/n3v3` 停留在 09-25 17:45：对 `let x = "a\q"` 输出 `type error(s) found` 且退出 3；`cargo build --release -p n3v3` 后同一命令退出 2 且前缀为 `parse error:` | `--release` 的 build 闸门构建 release CLI；cli-smoke/docs 分别通过参数与 `N3V3_BIN` 使用该二进制；`driver.sh` 每次先 `cargo build`（新鲜时为空操作）；`docs/contributor/contributing.md` 记录二进制选择 |

独立复核（`agent://reviewer`）在本轮改动中发现的问题及修复：

| 问题 | 证据 | 修复 |
|---|---|---|
| `n3v3 run` 只打印语法错误的计数、丢掉诊断正文：明细来自 `parser_diagnostic_modules_in_order()`（仅 `Parser` 类过滤），而报错条件用 `stats.parse_errors`（含 Lexer） | 修复前 `n3v3 run` 对 `let x = "a\q"` 只有 `error: parse error: 1 diagnostic(s)`（无 E0004 正文），`n3v3 check` 同输入会打印正文 | 方法改名并改为 `is_syntax()` 过滤（`syntax_diagnostic_modules_in_order`），`run.rs` 调用同步；CLI 测试新增正文断言（`invalid escape sequence`） |
| 格式化器漏掉 `RecordUpdate` 体的括号：`body_needs_parens` 只匹配 `ExprKind::Record` | `f = \|x\| ({ x \| a = 1 })` 被打印成 `f = \|x\| { x \| a = 1 }`，重新解析报 E0200（字段列表被当作 or 模式） | 匹配 `Record(_) \| RecordUpdate { .. }`；新增 `test_format_lambda_record_update_body_is_parenthesized` |
| 顶层 `let` 一律省略，而裸形式只支持变量/通配符模式 → 打印结果无法解析（复核只报了尾逗号，实际更宽） | `(x, y) = t`、`[a, b] = xs`、`(x,) = t`、`(_, y) = t` 全部 `E0101/E0104`；`let (x, y) = t`、`let [a, b] = xs` 通过 | `format_let` 仅在模式为 `Var`/`Wildcard` 时省略 `let`；新增 `test_format_top_level_non_variable_pattern_keeps_let`、`test_format_top_level_variable_pattern_omits_let` |
| 一元组模式丢尾逗号：`PatternKind::Tuple` 不补尾逗号，`(x,)` 打印成 `(x)` → 绑定整个元组而非解构 | `let (x,) = t` 打印为 `(x) = t`（含义改变） | 镜像表达式打印器的 `patterns.len() == 1` 尾逗号；新增 `test_format_single_element_tuple_pattern_keeps_comma` |
| 测试闸门聚合管道在 `set -euo pipefail` 下可能直接终止脚本（无 `test result:` 行时 grep 返回 1） | `summary=$(... \| grep ... \| awk ...)` 无兜底，awk 的 END 分支不可达 | 加 `\|\| true` 并显式判定：无结果行时 `[FAIL] test` |
| snippets 零输入守卫单边：`fences == 0` 时仍 PASS `0 n3v3-check block(s)` | 守卫只在 `fences > 0 && total == 0` 时触发 | 增加 `fences -eq 0` → `[FAIL] snippets` |
| `--quick` 模式仍可能用缺失/陈旧的 debug 二进制跑 docs 闸门（quick 不含 cli-smoke）；且 `run_gate` 成功时丢弃输出，check-docs 内部的 `[SKIP]` 不可见 | quick 的 gate 集为 format/lint/build/skills/docs，`build` 只跑 `cargo check` | 非 `--release` 模式的 build 闸门改为 `cargo check` + `cargo build -p n3v3`；`N3V3_BIN`/`CARGO_TARGET_DIR` 在两个脚本中一致生效；release 模式的 docs 闸门改为 `--release --strict`（负对照：`N3V3_BIN=/nonexistent/n3v3` → `2 failed`），因此内部跳过会失败而不是 PASS |

新增 CLI 契约测试 `n3v3-cli/tests/cli_exit_codes.rs`（5 个）：词法错误→2、语法错误→2、类型错误→3、`n3v3 run` 词法错误→2、副作用内置的闸门开关行为。

### 未修复（需要决策或属独立任务）

| 问题 | 事实 | 影响 | 建议 |
|---|---|---|---|
| 格式化器丢弃 shebang | `n3v3-parser/src/lib.rs:39-41` 在解析前剥离 `#!`，AST 不携带该信息 | `n3v3 fmt file --write` 会移除可执行脚本的 shebang；`examples/` 中 6 个文件带 shebang | 需在解析结果/AST 上记录 shebang（公共 AST 变更，需评审）；本轮只记录限制 |
| lambda 体的裸记录与块歧义 | lambda 参数后的 `{` 一律按块解析（`parse_lambda_body`）；`\|x\| { a = 1, b = 2 }` 报 E0101，`\|x\| ({ ... })` 与 `\|x\| #{ ... }` 可解析 | 打印器已按括号形式输出；但语言层面“记录体 lambda 不能写裸 `{...}`”未被规范明确 | 若要支持裸记录体，需语法/解析器/规范/tree-sitter/Lean 同步变更；属语言决策 |
