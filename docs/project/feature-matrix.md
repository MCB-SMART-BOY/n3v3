<div align="center">

<img src="../../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>n3v3 Feature Matrix</h1>

<p><em>真实功能支持矩阵（当前工作区 v5.0.3）</em></p>

> **Current workspace: v5.0.3; [latest published release](https://github.com/MCB-SMART-BOY/n3v3/releases); canonical HIR pipeline since v5.0.0.**
> 12 canonical keywords, `let`/`fn`/`;` optional, `use` not `import`,
> `|x|` not `fn(x)`, `{ }` records, `&` record merge (also a line comment
> at the start of a line, after optional whitespace), `--` line comments,
> `if -> else` not `then/else`, `~` not `lazy`, and `pub` removed.
> The parser retains 10 legacy spellings for source compatibility; only
> `struct`, `enum`, `super`, and `crate` are dedicated lexer tokens.
> 当前工作区为 v5.0.3；[最近发布版本](https://github.com/MCB-SMART-BOY/n3v3/releases)以发布页为准；`&` 在表达式中合并记录，仅位于行首（可有前导空白）
> 且后面不是 `&` 时也可作行注释；`--` 同样可作行注释。

<p>
  <strong><a href="../../README.md">Home</a></strong> ·
  <strong><a href="../README.md">Docs</a></strong> ·
  <strong><a href="../../.claude/forward-plan.md">Forward Plan</a></strong>
</p>

</div>

---

这份文档的目的只有一个：

**把“n3v3 现在到底做到哪一步”写清楚。**

它不是宣传页，也不是愿景列表。
它记录的是当前仓库里的真实支持情况。

## 怎么看 / How To Read

### 状态标记

| 标记 | 含义 |
|------|------|
| `✅` | 这一层基本支持，当前没有明显语义断裂 |
| `⚠️` | 部分支持，或者有明显语义分叉、精度不足、工具链不一致 |
| `❌` | 当前基本不支持，或只能靠占位逻辑通过 |
| `N/A` | 这一层不适用 |

### 这份矩阵目前是什么

这是当前 `v5.0.3` 工作区状态的高风险特性概览，先覆盖**最容易误判为“已经完成”**的特性。

它当前重点回答：

- parser 接受了，不代表语言真的支持了
- AST 只是前端分析输入，不是运行时路径
- 文档写了，不代表工具链已经对齐

后续版本会继续扩展到更完整的 feature inventory。

## 总体判断 / High-Level Truth

当前项目的总体情况可以用以下事实概括：

1. **语法表面比语义闭环走得更快。**
2. **主 CLI、LSP、REPL 和测试都通过 canonical HIR runtime；AST 仅保留为分析输入。**
3. **Implemented：系统脚本能力已覆盖管道、重定向、进程执行、流式输出、信号、Task、glob 与 Stream<T>；证据见下方系统脚本能力矩阵与 `tests/end_to_end.rs`。**
4. **端到端测试覆盖真实 frontend/runtime smoke tests。**
5. **Implemented：13 Stream<T> APIs 已接入标准库与 evaluator；证据为 `scripts/counts.sh` 与 `crates/n3v3-std` / `crates/n3v3-eval`。**
6. **Experimental：26 LSP methods（19 requests + 7 notifications）已实现；CodeLens 与补全评分排序已接入，`did_change_configuration` 和 `did_change_watched_files` 仍是 stub。证据见 `crates/n3v3-lsp/src/backend.rs`。**
7. **Implemented (local discovery only)：Registry v1 API 和 RegistryClient 支持搜索/版本元数据查询；`package install` 只安装本地 store 中的包，远程回退仅列出可用版本，尚不会下载或安装远程包。证据见 `n3v3-cli/src/commands/registry_serve.rs`、`n3v3-cli/src/registry_client.rs` 和 `n3v3-cli/src/commands/install.rs`。**
8. **当前 v5.0.3 工作区使用规范 v4 语法**：12 canonical keywords，`let`/`fn`/`;` 可省略，`|x|` lambda，`{}` record；`&` 是 record merge，也可在行首作注释；`--` 作行注释；`pub` 已移除。

## 语言高风险特性矩阵 / High-Risk Language Features

| Feature / 特性 | Parser | HIR Lowering | Type Check | AST Analysis | HIR Runtime | Tooling | 当前判断 |
|----------------|--------|--------------|------------|--------------|-------------|---------|----------|
| 基础字面量、算术、记录、列表、元组 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ v5.0 syntax 收敛；管道/记录/列表/元组在 Parser→HIR→Typeck→HIR Runtime 全链路闭环 |
| 模块导入与模块图 | ✅ | ✅ | ✅ | ✅ | ⚠️ | ⚠️ | 模块系统已有实装，主 CLI 路径统一走 HIR；边缘场景仍可能产生导入诊断。 |
| 列表推导 | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ | 语言层基本可用，工具链覆盖不足 |
| 安全字段访问 `?.` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 已收敛：`resolve_optional_flow_payload` 统一处理 builtin Option / record / option-record；类型拒绝非 record 非 option 调用点，与 runtime 一致；REPL `:type` / LSP hover / diagnostics 均已闭环 |
| 路径字面量 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | `./config` 推断为 `Path` 类型；typed-path adapter 和 bridge 全覆盖 |
| 惰性表达式 `~` (lazy) | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ | `ExprKind::Lazy` 经 HIR 降级后由 canonical evaluator 执行；工具链覆盖仍需继续补齐 |
| 空值合并 `??` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 已收敛：`resolve_optional_flow_payload` 统一处理 builtin Option / user enum `Some/None`；类型拒绝非 Option-like 的 `??` 调用点，与 runtime 一致；REPL `:type` / LSP hover / diagnostics 均已闭环 |
| 错误传播 `?` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 已收敛：`resolve_optional_flow_payload` 统一处理 builtin Option/Result / user enum `Some/None`/`Ok/Err`；类型拒绝非 optional 的 `?` 调用点，与 runtime 一致；REPL `:type` / LSP hover / diagnostics 均已闭环 |
| Trait 定义与 impl 完整性 | ✅ | ✅ | ⚠️ | N/A | N/A | ⚠️ | impl 签名规范化与方法派发优先顺序已定；impl-assoc 类型解析已统一到 canonical 路径；方法调用 UnknownMethod 诊断已到位；关联类型与缺省 alias 链的解析在全管线一致 |
| 关联类型（声明与完整性） | ✅ | ✅ | ⚠️ | N/A | N/A | ⚠️ | 声明层稳定；`Self.Item` / `Self.Alias` 链解析与缺省已通过 canonical impl-assoc helper 统一；投影解析已暴露到 `ModuleSemantics`；impl-signature 不匹配已带投影标签 |
| 方法调用语法 `x.foo(y)` | ✅ | ✅ | ⚠️ | ✅ | ✅ | ⚠️ | canonical 派发顺序：inherent → trait method → callable fallback；`UnknownMethod` 诊断码已稳定；派发优先级已有端到端测试锁定；未来是否移除 callable fallback 待后续决策 |
| Or pattern `a | b` | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ | HIR lowering/typecheck/runtime 已收敛，工具链覆盖仍需继续补齐 |
| Binding pattern `x @ pat` | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ | `name @ pattern` 已在 AST/HIR 路径闭环，工具链覆盖仍需继续补齐 |
| List rest pattern `[x, ..xs]` | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ | `init/rest/tail` 语义已在 AST/HIR 路径闭环，工具链覆盖仍需继续补齐 |
| 记录模式匹配 | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ | 已接入穷尽性检查：覆盖所有声明字段即为 exhaustive；缺失字段报告 non-exhaustive；工具链覆盖仍需补齐 |
| Match 穷尽性检查 | N/A | N/A | ✅ | N/A | N/A | ⚠️ | 支持 Bool/Unit/Int/Float/Char/String、用户枚举、builtin Option/Result、Record（字段覆盖）、List（空/非空）、Tuple（逐位）；NonExhaustiveMatch 错误码；工具链覆盖仍需补齐 |
| Unreachable pattern 警告 | N/A | N/A | ⚠️ | N/A | N/A | ❌ | 现已支持“前置分支已完成总覆盖”后的不可达告警，包括不可反驳分支、布尔全覆盖、用户枚举全覆盖与 builtin `Option/Result` 全覆盖；更细粒度的子集判定仍需继续扩展 |
| REPL `:type` | N/A | N/A | N/A | N/A | N/A | ⚠️ | 现在会复用增量 REPL 会话中的已加载模块、历史 HIR 模块与当前输入，一起做 typecheck 后查询表达式与全局定义类型；但跨项目根目录切换、跨模块命名类型显示和更完整的工具链镜像仍需继续补齐 |
| 一等 Stream<T> | N/A | N/A | N/A | N/A | N/A | N/A | N/A（系统标准库特性；详见下方系统脚本能力矩阵） |
| 真实端到端执行测试 | N/A | N/A | N/A | N/A | N/A | Experimental | `tests/end_to_end.rs` 定义 558 E2E tests；真实 frontend/HIR runtime smoke baseline |

## 工具链一致性矩阵 / Tooling Fidelity Matrix

| Area / 领域 | 现状 | 主要问题 |
|-------------|------|----------|
| `n3v3 check` | Experimental | 默认启用 effect checking；`--allow-effects` 放宽副作用调用限制。类型检查能跑，当前模块和已加载依赖模块中的命名类型能在 diagnostics 中较可读地显示；完整语义镜像和更多编译器级保证仍需补齐 |
| `n3v3 eval` | ⚠️ 可用 | 表达式与模块导入统一走 frontend/HIR；未收敛的是少数导入或运行时边缘诊断，不存在 AST runtime fallback |
| `n3v3 run` | ⚠️ 可用 | 普通模块图和常见 `std` item/module/glob 导入统一走 HIR，跨模块命名类型在 diagnostics 中的显示也已更可读；剩余问题是少数边缘导入/运行时语义 |
| REPL | ⚠️ 可用 | 交互与 `:type` 都能工作，类型查询和求值主路径都已开始围绕增量 HIR runtime 收敛；普通持久绑定、跨输入重定义、跨输入 trait/impl 方法派发、常见 `std.<module>` 导入、项目内模块 item/module 导入、`:load` 文件场景下的相对模块导入、新导入模块的 type diagnostics 展示，以及清空会话后的安全跨项目根目录切换都已可工作。当前仍明确缺少更完整的 module graph/tooling 镜像 |
| Formatter | ⚠️ 基本可用 | 日常可用，但“稳定且幂等”还应继续验证 |
| LSP | Experimental (26 LSP methods; 19 requests + 7 notifications) | 前端管线已接入，hover 支持定义点类型和语义类型。`goto definition` / `references` / `rename` 对局部遮蔽场景已按实际绑定解析。补全评分排序 (exact/prefix/contains)。CodeLens 引用计数。26 methods: initialize, initialized, shutdown, didOpen, didChange, didSave, didClose, didChangeConfiguration (stub), didChangeWatchedFiles (stub), hover, completion, completionItem/resolve, signatureHelp, definition, references, documentHighlight, rename, prepareRename, formatting, documentSymbol, workspace/symbol, semanticTokens/full, inlayHint, foldingRange, codeLens, codeAction |
| End-to-end tests | Experimental | 可信 smoke baseline：tests/end_to_end.rs 定义 558 E2E tests；覆盖 Task spawn/poll/cancel/awaitAny, signals, glob, env/cwd, redirects, streaming, bytes, shebang, 13 Stream<T> APIs, TTY, job control, defer/retry/ensure, try/catch/option, fmt roundtrip, init scaffold, test discovery, io.readKey；覆盖深度仍需继续扩展 |

## 系统脚本能力矩阵 / System Scripting Matrix

### Implemented capabilities / 已实现能力

| 能力 | 说明 |
|------|------|
| 文件操作 | `io.readFile/read` / `io.writeFile/write` / `io.appendFile` / `io.chmod` / `io.symlink` + typed-path 变体 |
| 原子文件操作 | `io.atomicWrite` / `io.atomicWriteAll` (两阶段提交) / `io.copy` / `io.move` |
| 目录操作 | `io.createDirAll` / `io.removeDirAll` / `io.walk` / `io.tempDir` + typed-path 变体 |
| 环境变量 | `io.getEnv` / `io.env()` (返回 Record) |
| 进程执行 | `io.execCommand` / `io.execPipeline` + 超时 + 杀进程 |
| 流式 I/O | `io.execCommandStreaming` / `io.execPipelineStreaming` / `io.readFileLines` |
| 路径字面量 | `./config` 推断为 `Path` 类型，`io.readFilePath(./x)` 直接可用 |
| 字符串拼接 | `"a" + "b"` |
| 管道语法 | `a |> f` 等价于 `f(a)` |
| 输出 | `print` / `println` 全局可用 |
| 效果系统 | v4.0: auto-inferred, no keyword needed; function/impl/lambda全覆盖 |
| REPL | 历史持久化、Tab 补全、`:save` / `:cd`、智能输入完成 |
| 事件系统 | `Event<T>`、`io.every(ms)`、`io.watchFile(p)`、`io.eventNext(e)` |
| 反应式 | `Live<T>`、`io.reactive(e)`、`io.liveNext(l)` |
| 时序约束 | `io.retry(fn, n, ms)`、`io.ensure(check, timeout, interval)` |
| 流组合子 | `io.glob` + `list.filter` + `list.map` 已验证 |

### Experimental gaps / 仍处于 Experimental 的能力

| Capability / 能力 | 当前状态 | 为什么还不能叫“替代 Bash” |
|-------------------|----------|----------------------------|
| 一等 `Path` 类型 | ✅ | `./path` 字面量推断为 `Path` 类型；`io.readFilePath(./test)` 直接可用，无需 `path.fromString()`；typed adapter（`path.joinPath` / `path.parentPath` / `path.filenamePath` / `path.extensionPath`）和 typed bridge（`io.readFilePath` / `io.writeFilePath` / `io.*Path`）已覆盖核心场景 |
| 一等 `Bytes` 类型 | ✅ | 内部已落 stable runtime identity；`std.io.readFileBytesPath : Path -> Bytes` / `std.io.writeFileBytesPath : Path -> Bytes -> Unit` / `std.io.appendFileBytesPath : Path -> Bytes -> Unit` 已提供公开 file-boundary bridges；Lean 形式化已完成（Ty.Bytes + Value.bytes + EffectEval 规则 + 精化桥） |
| 一等 `Command` 类型 | ⚠️ | 内部已落 runtime identity，且 `std.io.command` / `std.io.commandWith` / `std.io.commandWithRedirects` 与 `std.io.execCommand` 已提供首批公开构造/执行桥；`Command` 现在已能承载 `cwd` / `env` / `stdin` 与 typed `Redirect` 列表，而 shell 行为也已收回到显式 `Command` 构造，而不是额外的 string-only wrapper |
| 一等 `ProcessResult` 类型 | ⚠️ | `std.io.execCommand` / `std.io.execPipeline` 已能公开返回 `ProcessResult`，且 `std.io.processSuccess` / `std.io.processStdout` / `std.io.processCode` / `std.io.processStderr` 已提供首批 pure inspector bridge；当前剩余问题不再是结果对象缺失，而是更丰富的 effect model 仍未完成 |
| 一等管道 | Implemented | `cmd1 |> cmd2 |> cmd3` 管道语法已实现（HIR evaluator）；`io.pipeline([...])` / `io.pipelineWithRedirects` 构造时拒绝无效 pipeline；`io.execPipeline` / `io.execPipelineStreaming` / `io.execPipelineStreamingWithTimeout` 提供阻塞+流式+超时执行；`io.streamPipe` 实现流到命令的标准输入管道；Stream<T> 已实现，但流式句柄和更广的进程编排模型仍需后续 |
| 一等重定向 | ⚠️ | 内部已落 runtime identity，且 `std.io.redirectStdoutPath` / `std.io.redirectStderrPath` / `std.io.redirectStdinPath` 已提供最小公开构造桥；边界级 `stdout -> Path` / `stderr -> Path` / `stdin <- Path` 组合现在通过 `io.commandWithRedirects` / `io.pipelineWithRedirects` 收进一等对象，再走 `io.execCommand` / `io.execPipeline` 的 canonical 执行主线；同时对 boundary/stage-local 重复 redirect、non-final `stdout` 截流、以及 non-first stage 的 `stdin` 配置冲突继续做显式拒绝；但还没有更广的流模型或 stage-local redirect 语法 |
| 一等 `Task<T>` | ✅ | `std.io.taskCommand` / `std.io.taskPipeline` 构造 Task；`std.io.awaitTask` / `std.io.awaitTasks` / `std.io.awaitAny` / `std.io.awaitTaskWithTimeout` 消费 Task；`std.io.cancel` 取消 Task；`std.io.spawn` / `std.io.spawnWithTimeout` / `std.io.poll` 非阻塞管理 |
| 流式处理 | ✅ | `io.execCommandStreaming` / `io.execPipelineStreaming` / `io.readFileLines` / `io.readFileLinesPath` (逐行回调)；超时变体 `io.execCommandStreamingWithTimeout` / `io.execPipelineStreamingWithTimeout` 支持总时限 + 进程终止 |
| timeout / cancel | ✅ | `io.awaitTaskWithTimeout` 支持 Command/Pipeline 超时（阻塞式）；`io.execCommandStreamingWithTimeout` / `io.execPipelineStreamingWithTimeout` 支持流式超时；统一 kill 机制（M-2）已落地 `n3v3-common::kill_process`；`io.cancel` / `io.awaitAny` 已实现 |
| signal / TTY | ✅ | `io.isTTY(fd)` / `io.terminalSize()` / `io.setRawMode(fd, enable)` / `io.resetTerminal(fd)` / `io.readKey(fd)` 已实现；`io.onSignal` 支持 INT/TERM/HUP/USR1/USR2 注册回调；求值器在安全点轮询原子标志并分派 |
| shebang / argv / 脚本入口 | ✅ | shebang + argv：`io.args()` 返回 `(List<String>, Record)` 结构化元组；`-flag` 解析为 Record 字段，负数自动识别为位置参数；支持紧凑格式 `-j8` 和 `--` 分隔 |
| glob / 文件查询组合子 | ✅ | `io.glob(pattern)` 返回 `List<Path>`，可与 `list.filter`/`list.map` 组合 |
| 一等 Stream<T> | Implemented | 13 Stream<T> APIs：构造 (streamList/streamLines/streamCommand/streamBytes)、变换 (streamMap/streamFilter/streamTake/streamDrop)、消费 (streamCollect/streamPipe/streamForEach/streamFold)、包装 (streamWithTimeout)；Channel-based + Iterator-based 双路径实现；证据见 `scripts/counts.sh`、`crates/n3v3-std` 与 `crates/n3v3-eval` |

## 当前最该关注的缺口 / Most Important Gaps

### 1. 不是“语法少”，而是“语义不一致”

最危险的不是 parser 不支持，而是：

- parser 支持了
- HIR lowering、typeck 或 canonical HIR runtime 仍未闭环

这种状态最容易制造“看起来已经有了”的错觉。

### 2. 方法调用、模式系统是当前的核心风险区

这两块都属于：

- 表面看起来已经有语法
- 实际上还没有完全稳定的统一语义

所以它们必须优先于新语法。`?` / `??` / `?.` 的 optional-flow 语义已在 PR-017 中完成收敛，不再属于风险区。

### 3. Bash 替代已覆盖常用主路径，但仍有边界

当前已覆盖常用系统脚本路径；剩余难点主要是更广的 effect model、流式句柄和更丰富的进程编排，而不是基础命令执行入口。

- 管道 — **Implemented**（`|>` 语法 + Pipeline 构造 + 执行 + 流式）
- 重定向 — **Implemented**（stdin/stdout/stderr + boundary 冲突检测）
- 进程上下文 — **Implemented**（cwd + env 隔离）
- 失败组合 — **Implemented**（`?` 传播 + ProcessResult 检查器）
- 长任务控制 — **Implemented**（Task spawn/poll/cancel/awaitAny）
- 脚本入口模型 — **Implemented**（shebang + `io.args()` + argv 解析）
- 流变换 — **Implemented**（13 Stream<T> APIs）

> 开发状态见 [`.claude/forward-plan.md`](../../.claude/forward-plan.md)。
