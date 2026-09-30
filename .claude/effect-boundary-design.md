# n3v3 Effect Boundary Design Document

<div align="center">

**版本**: v1.2
**日期**: 2026-06-17
**状态**: 实现完成 (v4.0.1)
**关联**: [前向计划](./forward-plan.md) · [开发计划](../../.claude/forward-plan.md) · [功能矩阵](../docs/project/feature-matrix.md)

</div>

---

## 1. 概述 / Overview

### 1.1 目的

本文档是 **G4 决策门**（Effect Boundary / 副作用边界）的设计文档。它定义：

1. n3v3 语言中 **纯函数**（pure）与 **副作用函数**（effectful）的边界在哪里
2. 所有副作用操作的**完整清单**及其语义
3. Rust 实现与 Lean 形式规范的**逐条对应关系**
4. 安全边界（H-1, H-2, M-1, M-4）在设计层面的体现
5. 效果系统的**运行时对象模型**（Command, Pipeline, Task, Redirect 等）

### 1.2 为什么需要这份文档

| 问题 | 没有这份文档时 | 有这份文档后 |
|------|--------------|------------|
| 新功能的语义定义 | 参考实现，容易分叉 | 先查规范，再写代码 |
| Rust 与 Lean 的对齐 | 手动比对，容易漂移 | 有对照表，CI 验证 |
| 安全审计 | 每次重新审查全部代码 | 只审查边界变更 |
| 效果边界判定 | 靠 `is_effectful_builtin()` 隐式定义 | 由 intrinsic metadata 显式声明，布尔接口兼容旧调用方 |

### 1.3 核心原则

1. **所有有副作用的 builtin 在 `intrinsic_metadata()` 中注册（用户代码中的 effect 自动推导）**：`is_effectful_builtin()` 只从共享注册表派生兼容布尔结果
2. **所有副作用必须在 Lean 规范中有对应规则**：EffectEval 规则覆盖 100% 的副作用路径
3. **安全约束是规则的显式前提**：大小限制、路径安全检查、环境变量过滤不是"实现细节"，而是语义规则的必要组成部分
4. **纯函数化原则**：进程执行的结果是 `ProcessResult`（不可变值），不是隐式状态变更

---

## 2. 效果边界定义 / Effect Boundary Definition

### 2.1 什么是"纯" / Pure

一个 n3v3 表达式是**纯的**当且仅当：

1. 它的求值不依赖任何外部状态（文件系统、网络、环境变量、时间、随机数）
2. 它的求值不产生任何外部可观察的副作用（输出、文件写入、进程启动）
3. 对于相同的输入，总是产生相同的输出（引用透明）

纯表达式走 `BigStep` 语义（`Spec/Eval.lean`），不需要 IOState。

### 2.2 什么是"有副作用的" / Effectful

一个 n3v3 表达式是**有效果的**当且仅当它满足以下任一条件：

1. **进程执行**：启动外部进程（execCommand, execPipeline）
2. **文件 I/O**：读写文件系统（readFile, writeFile, readFileLines）
3. **输出**：向 stdout/stderr 写入（print, println，以及所有进程执行的输出捕获）
4. **网络**：通过 fetch 发起网络请求
5. **时间**：依赖系统时间或计时器（every, timeout 系列）
6. **信号**：注册或响应 OS 信号（onSignal）

有效果的表达式走 `EffectEval` 语义（`Spec/Effects.lean`），需要 `IOState` 来追踪累积输出。

### 2.3 边界判定函数与动作元数据

Rust 侧的规范注册表是 `n3v3_common::intrinsic_metadata(name)`，位于
`crates/n3v3-common/src/action.rs`。它同时提供：

- `EffectSummary`：从 `HOST` 细化到文件读写、进程、任务、输出、网络、时间和设备；
- `HostOpKind`：当前迁移阶段已经拥有类型化宿主操作的 builtin 种类；
- `is_effectful_builtin(name)`：为既有 typeck/stdlib 调用方保留的布尔兼容接口，
  其结果直接来自上述注册表。

```rust
let metadata = n3v3_common::intrinsic_metadata(name);
let is_effectful = metadata.is_some_and(|item| item.effects.is_host_effect());
```

当前第一阶段只把字面量 `io.readFile("path")` 收集为
`n3v3_common::ActionPlan`（其中操作为 `HostOp::ReadFile`）。frontend 将计划放入
`ModuleSemantics.action_plans`，用于检查和工具观察；有阻断诊断时不发布计划，
收集动作计划不会执行宿主操作。真正执行必须显式提供 `Host`，测试使用
`FakeHost`，操作系统实现使用 `OsHost::new(root)`；OS host 只允许 root 下的
相对路径，并拒绝绝对路径、`..` 和越过 root 的 symlink。动态路径暂不伪造计划。

**注册表是效果边界的 Rust 侧权威来源。** 新增 effectful builtin 时，必须更新
`intrinsic_metadata`，同步 typeck 类型签名、evaluator 分发、标准库实现和
`Spec/Effects.lean`；若已有类型化 `HostOp`，同时补齐 fake/real host 与行为测试。

---

## 3. 效果操作清单 / Effect Operation Inventory

### 3.1 完整清单（按类别分组）

#### A. 进程执行（有副作用：启动外部进程、捕获输出）

| Builtin | 模式 | EffectEval 规则 | Rust 入口 |
|---------|------|----------------|-----------|
| `io.execCommand` | 阻塞 | `execCommand` | `builtin_exec_command` |
| `io.execPipeline` | 阻塞 | `execPipeline` | `builtin_exec_pipeline` |
| `io.execCommandStreaming` | 流式 | `execCommandStreaming` | `builtin_exec_streaming` |
| `io.execPipelineStreaming` | 流式 | `execPipelineStreaming` | `builtin_exec_pipeline_streaming` |
| `io.execCommandStreamingWithTimeout` | 流式+超时 | `execCommandStreamingTimeout` / `TimeoutExpired` | `builtin_exec_streaming_with_timeout` |
| `io.execPipelineStreamingWithTimeout` | 流式+超时 | `execPipelineStreamingTimeout` / `TimeoutExpired` | `builtin_exec_pipeline_streaming_with_timeout` |

#### B. 文件 I/O（有副作用：读写文件系统）

| Builtin | 模式 | EffectEval 规则 | Rust 入口 |
|---------|------|----------------|-----------|
| `io.readFile` | 阻塞 | `readFile` | `builtin_read_file` |
| `io.writeFile` | 阻塞 | `writeFile` | `builtin_write_file` |
| `io.readFileLines` | 流式 | `readFileLines` | `builtin_read_file_lines` |

#### C. 任务/并发（有副作用：创建异步任务）

| Builtin | 模式 | EffectEval 规则 | Rust 入口 |
|---------|------|----------------|-----------|
| `io.spawn` | 延迟 | `spawn` | `builtin_spawn` |
| `io.awaitTask` | 阻塞 | `awaitTask` | `builtin_await_task` |
| `io.awaitTasks` | 阻塞 | `awaitTasks` | `builtin_await_tasks` |
| `io.awaitTaskWithTimeout` | 阻塞+超时 | `awaitTaskTimeout` | `builtin_await_task_timeout` |
| `io.cancel` | 直接 | `cancel` | `builtin_cancel` |
| `io.awaitAny` | 阻塞 | `awaitAny` | `builtin_await_any` |

#### D. 输出（有副作用：写入 stdout/stderr）

| Builtin | 模式 | EffectEval 规则 | Rust 入口 |
|---------|------|----------------|-----------|
| `print` | 直接 | `pure` (通过 BigStep + IOState) | `builtin_print` |
| `println` | 直接 | `pure` (通过 BigStep + IOState) | `builtin_println` |

#### E. 纯构造函数（无副作用，创建运行时对象）

| Builtin | 说明 |
|---------|------|
| `io.command` | 创建不带配置的 Command |
| `io.commandWith` | 创建带配置的 Command（cwd, env, stdin, redirects） |
| `io.commandWithRedirects` | 创建带重定向列表的 Command |
| `io.pipeline` | 创建 Pipeline |
| `io.pipelineWithRedirects` | 创建带重定向的 Pipeline |
| `io.redirectStdoutPath` | 创建 stdout → Path 重定向 |
| `io.redirectStderrPath` | 创建 stderr → Path 重定向 |
| `io.redirectStdinPath` | 创建 stdin ← Path 重定向 |
| `io.taskCommand` | 将 Command 包装为 Task |
| `io.taskPipeline` | 将 Pipeline 包装为 Task |

#### F. 纯检查器（无副作用，访问 ProcessResult 字段）

| Builtin | 说明 |
|---------|------|
| `io.processSuccess` | 检查退出码是否为 0 |
| `io.processStdout` | 获取 stdout 内容 |
| `io.processCode` | 获取退出码 |
| `io.processStderr` | 获取 stderr 内容 |

#### G. Stream 操作（构造效果尚未收敛）

| Builtin | 模式 | 效果 | EffectEval 规则 | 说明 |
|---------|------|------|----------------|------|
| `io.streamList` | 构造 | ❌ 无 | — | 列表转惰性流 |
| `io.streamLines` | 构造 | ✅ 文件读 | — | 当前实现创建线程并打开文件；尚未 cold |
| `io.streamCommand` | 构造 | ✅ 进程 | — | 当前实现创建线程并 spawn 进程；尚未 cold |
| `io.streamBytes` | 构造 | ✅ 文件读 | — | 当前实现创建线程并打开文件；尚未 cold |
| `io.streamMap` | 变换 | ❌ 无 | streamMap | 逐元素映射 ✅ |
| `io.streamFilter` | 变换 | ❌ 无 | streamFilter | 惰性过滤 ✅ |
| `io.streamTake` | 变换 | ❌ 无 | streamTake | 截断 ✅ |
| `io.streamDrop` | 变换 | ❌ 无 | streamDrop | 跳过 ✅ |
| `io.streamCollect` | 消费 | ✅ | streamCollect | 收集为列表 |
| `io.streamPipe` | 消费 | ✅ | streamPipe | 流入命令 stdin |
| `io.streamWrite` | 消费 | ✅ | streamWrite | 写入文件 |
| `io.streamForEach` | 消费 | ✅ | streamForEach | 逐元素消费 |
| `io.streamFold` | 消费 | ✅ | streamFold | 严格折叠 |
| `io.streamWithTimeout` | 包装 | ✅ 时间 | — | 当前构造读取 `Instant`；尚未 cold |

`crates/n3v3-common/src/action.rs` 按当前 runtime 行为将上述四个构造器标为
effectful，避免纯函数绕过效果闸门。若要恢复 cold Stream 契约，必须把线程创建、
文件打开、进程 spawn 和计时器读取移到显式 terminal/action host operation，
并同步 registry、typeck、Lean 和 E2E。

#### H. TTY 控制（有副作用：配置终端设备）

| Builtin | 模式 | 效果 | EffectEval 规则 | 说明 |
|---------|------|------|----------------|------|
| `io.setRawMode` | 直接 | ✅ | `setRawMode` | 设置终端 raw mode |
| `io.resetTerminal` | 直接 | ✅ | `resetTerminal` | 恢复终端默认模式 |
| `io.readKey` | 直接 | ✅ | `readKey` | 从 fd 读取单字节 |

#### I. Job 控制（有副作用：查询/等待后台作业）

| Builtin | 模式 | 效果 | EffectEval 规则 | 说明 |
|---------|------|------|----------------|------|
| `io.jobs` | 直接 | ✅ | `jobs` | 列出活跃 spawn ID |
| `io.waitAnyJob` | 阻塞 | ✅ | `waitAnyJob` | 等待任意作业完成 |

### 3.2 分类说明

效果分类由 `n3v3_common::intrinsic_metadata` 维护，不能只按 builtin 名称或
旧的分类表推断：

- `streamList`、`streamMap`、`streamFilter`、`streamTake`、`streamDrop` 只构造
  或变换内存描述，当前保持纯；
- `streamLines`、`streamCommand`、`streamBytes`、`streamWithTimeout` 按当前
  runtime 行为标为 effectful，因为构造阶段已经创建线程、打开文件、spawn
  进程或读取计时器；
- `isTTY`、`terminalSize`、`readKey`、`input`、`readPassword` 读取宿主设备或
  输入，归入 `DEVICE`；`liveCurrent` 与 `liveCancel` 也不再伪装成纯操作；
- 其他 Process、文件、Task、网络、输出和显式终端操作按 registry 中的
  `EffectSummary` 分类；纯构造器和 ProcessResult 检查器仍可保持纯。

当前 runtime 与 cold Stream/Action 边界尚未完全收敛；任何恢复纯构造或扩大
执行范围的改动都必须同时更新 registry、typeck、evaluator、Lean、文档和行为测试。

---

## 4. 运行时对象模型 / Runtime Object Model

### 4.1 类型层次

```
Expr (源语言表达式)
  └── Expr.builtin(name, args)  ← 效果 builtin 的入口

Value (运行时值)
  ├── Value.processResult(code, stdout, stderr)  ← 进程执行结果
  ├── Value.string(s)                             ← 文件内容
  ├── Value.closure(x, body, env)                 ← Task / spawn 结果
  ├── Value.list(elems)                           ← readFileLines 结果
  ├── Value.someVal(v) / Value.noneVal            ← 超时结果
  ├── Value.stream(StreamValue)                  ← Phase 4 (planned)
  └── Value.unit                                  ← writeFile 等操作的结果

IOState (效果状态，在 EffectEval 中传递)
  ├── stdin  : String   ← 累积的标准输入
  ├── stdout : String   ← 累积的标准输出
  └── stderr : String   ← 累积的标准错误
```

### 4.2 关键设计决策

**D-009: Effects are explicit and runtime-mediated**
- 效果不是隐式的（不像 Haskell 的 IO monad）
- 效果通过类型系统自动推断（`effect` 关键字在 v4.0 中已可选）
- 纯函数不能调用有效果的 builtin（`n3v3 check --pure` 强制检查）

**D-012: Process control is modeled, not inferred**
- `Command` 是一等运行时对象，不是裸字符串
- Shell 行为已收回到显式 `Command` 构造
- 不再有 string-only 的 shell wrapper

**D-013: Initial effect boundary is Task<T> plus effect summaries**
- `Task[ProcessResult]` 是延迟执行的单元
- 效果摘要由类型系统推导（不在本文档范围内）

---

## 5. 安全边界 / Security Boundaries

### 5.1 缓冲区大小限制（H-1, H-2）

**H-1: stdin 限制（10 MB）**

```
MAX_STDIN_BYTES = 10 * 1024 * 1024
```

- 在所有 6 个进程执行路径中强制检查
- `EffectEval.execCommand` 规则将 `hstdin_len : stdin_str.length ≤ MAX_STDIN_BYTES` 作为**显式前提**
- Rust: `stdin.len() > MAX_STDIN_BYTES → Err`

**H-2: stdout/stderr 限制（50 MB）**

```
MAX_OUTPUT_BYTES = 50 * 1024 * 1024
```

- 在所有 6 个进程执行路径中强制检查
- `EffectEval.execCommand` 规则将 `hout_len` 和 `herr_len` 作为**显式前提**
- Rust: `output.stdout.len() > MAX_OUTPUT_BYTES → Err`

**Lean 定理**：`Verify/Limits.lean` 证明了对任意 stdin/stdout/stderr 大小，检查通过当且仅当大小在限制内。

### 5.2 路径遍历防护（M-1）

**任何包含 `..` 组件的重定向路径被替换为安全哨兵**：

```
/dev/null/n3v3-blocked-traversal
```

- Rust: `resolve_redirect_path()` 检查 `Path.components().any(|c| c == ParentDir)`
- Lean: `resolve_redirect_path()` 检查 `has_parent_dir redirect`
- **定理**：`Verify/Path.lean` 证明了 `path_safety_with_safe_cwd` — 安全的 cwd + 安全的 redirect → 安全的输出路径

### 5.3 环境变量过滤（M-4）

**子进程环境中永远移除以下危险变量**：

```
LD_PRELOAD, LD_LIBRARY_PATH, DYLD_INSERT_LIBRARIES, DYLD_LIBRARY_PATH
```

- Rust: `configured_process_command()` 调用 `cmd.env_remove()` 逐个移除
- Lean: `strip_dangerous()` 过滤列表
- **定理**：`Verify/Environ.lean` 证明了 `env_safety_theorem` — 过滤后的环境不包含任何危险键

---

## 6. EffectEval 规则清单 / EffectEval Rule Inventory

### 6.1 规则总览（34 条，EffectEval v4.3）

| # | 规则名称 | 类别 | 核心前提 |
|---|---------|------|---------|
| 1 | `pure` | 纯桥接 | `BigStep env e v` |
| 2 | `execCommand` | 阻塞执行 | `hstdin_len + hout_len + herr_len` |
| 3 | `execPipeline` | 阻塞管道 | `hsize_all_stages + hfinal_out + hfinal_err` |
| 4 | `spawn` | 延迟创建 | `body` 被包装为 closure |
| 5 | `awaitTask` | 阻塞等待 | 完整子求值 |
| 6 | `awaitTaskTimeout` | 超时等待 | 完成子求值或超时 → Option |
| 7 | `execCommandStreaming` | 流式执行 | `hstdin_len + hline_count + hout_len + herr_len` |
| 8 | `execPipelineStreaming` | 流式管道 | `hsize_all_stages + hline_count + hfinal_*` |
| 9 | `execCommandStreamingTimeout` | 流式+超时（成功） | 所有前提 + timeout 未过期 |
| 10 | `execCommandStreamingTimeoutExpired` | 流式+超时（超时） | timeout 已过期 → None |
| 11 | `execPipelineStreamingTimeout` | 管道+超时（成功） | 所有前提 + timeout 未过期 |
| 12 | `execPipelineStreamingTimeoutExpired` | 管道+超时（超时） | timeout 已过期 → None |
| 13 | `readFileLines` | 流式读文件 | `hline_count` |
| 14 | `readFile` | 阻塞读文件 | `content = fileContent path` |
| 15 | `writeFile` | 阻塞写文件 | （无额外前提） |
| 16 | `retry_success` | 重试成功 | action 在 maxAttempts 内成功 |
| 17 | `retry_failure` | 重试失败 | action 在 maxAttempts 内均失败 |
| 18 | `ensure_success` | 确保成功 | action 在 timeoutMs 内成功 |
| 19 | `ensure_timeout` | 确保超时 | action 在 timeoutMs 内未成功 |
| 20 | `cancel` | 任务取消 | 终止运行中的 TaskHandle |
| 21 | `awaitAny` | 等待最先完成 | 多个 TaskHandle 中首个完成者 |
| 22-26 | `streamConstruct` | Stream 构造 (5) | streamList/streamLines/streamCommand/streamBytes/streamCollect |
| 27-31 | `streamTransform` | Stream 变换+管道 (5) | streamMap/streamFilter/streamTake/streamDrop/streamPipe |
| 32-34 | `streamConsume` | Stream 消费 (3) | streamForEach/streamFold/streamWithTimeout |

### 6.2 规则与 Rust 的映射

每个 EffectEval 规则对应 Rust 中的一个 `builtin_*` 函数：

| EffectEval 规则 | Rust 函数（`crates/n3v3-eval/src/eval.rs`） | 阻塞路径数 |
|----------------|------------------------------------------|-----------|
| `execCommand` | `builtin_exec_command` | 5 个阻塞路径 |
| `execPipeline` | `builtin_exec_pipeline` | 5 个阻塞路径 |
| `execCommandStreaming` | `builtin_exec_streaming` | 流式 |
| `execPipelineStreaming` | `builtin_exec_pipeline_streaming` | 流式 |
| `execCommandStreamingTimeout` | `builtin_exec_streaming_with_timeout` | 流式+超时 |
| `execPipelineStreamingTimeout` | `builtin_exec_pipeline_streaming_with_timeout` | 流式+超时 |
| `readFileLines` | `builtin_read_file_lines` | 流式 |
| `readFile` | `builtin_read_file` | 阻塞 |
| `writeFile` | `builtin_write_file` | 阻塞 |
| `spawn` | `builtin_spawn` | 延迟 |
| `awaitTask` | `builtin_await_task` | 阻塞 |

### 6.3 核心不变量

从 EffectEval 规则中提取的、所有副作用路径都必须维护的不变量：

1. **I-1: stdin 大小限制** — `hstdin_len : size ≤ MAX_STDIN_BYTES`
2. **I-2: stdout 大小限制** — `hout_len : size ≤ MAX_OUTPUT_BYTES`
3. **I-3: stderr 大小限制** — `herr_len : size ≤ MAX_OUTPUT_BYTES`
4. **I-4: 流式行数限制** — `hline_count : lines ≤ MAX_STREAM_LINES`
5. **I-5: 路径安全** — 重定向路径不包含 `..`
6. **I-6: 环境安全** — 子进程环境中无 LD_PRELOAD 等危险变量
7. **I-7: IOState 单调性** — stdout/stderr 只追加，不删除

---

## 7. 流式与延迟执行模型 / Streaming & Deferred Model

### 7.1 流式执行

流式执行与阻塞执行的区别：

| 维度 | 阻塞 (Blocking) | 流式 (Streaming) |
|------|----------------|-----------------|
| 输出传递 | 一次性返回完整字符串 | 逐行回调 |
| 内存占用 | O(output_size) | O(line_size) |
| 超时支持 | 无（外部用 awaitTaskWithTimeout） | 内置（StreamingWithTimeout 变体） |
| 行数限制 | 无 | `MAX_STREAM_LINES = 100,000` |
| Lean 规则 | `execCommand` | `execCommandStreaming` |

### 7.2 超时模型

超时有两个层次：

1. **流式超时**（内置）：`execCommandStreamingWithTimeout(program, args, stdin, timeout_ms)`
   - 成功 → `Some(ProcessResult)`
   - 超时 → `None`，进程被 `libc::kill` 终止

2. **任务超时**（外部）：`awaitTaskWithTimeout(task, timeout_ms)`
   - 适用于任何 Task（Command 或 Pipeline）
   - 当前是**阻塞式**超时（没有取消机制）

---

## 8. 当前缺口与后续工作 / Gaps & Future Work

### 8.1 语义缺口

| 缺口 | 影响 | 优先级 |
|------|------|--------|
| EffectEval 的 `awaitTaskTimeout` 规则是框架占位 | Task 超时的形式语义未定义 | 中 |
| `BigStep.matchOn` 缺少 fallthrough 规则 | 多臂模式匹配中非第一臂无法在 Lean 中直接证明 | 高 |
| `big_step_deterministic` 引理未证明 | env_preservation 不能直接使用给定求值结果 | 中 |

### 8.2 功能缺口

| 缺口 | 影响 | 优先级 |
|------|------|--------|
| 流式处理没有 cancel/poll 模型 | 无法取消长时间运行的流 | 低 |
| 管道的流式句柄 | 只能阻塞式消费管道输出 | 中 |
| `cmd1 |> cmd2` 管道语法 | 目前只能用 `io.pipeline([...])` 构造 | 低 |
| 后台调度 / 非阻塞 task runtime | Task 只能阻塞式等待 | 低 |

### 8.3 形式化缺口

| 缺口 | 影响 | 优先级 |
|------|------|--------|
| `progress_match` 公理 | 通用模式匹配的类型安全未形式化证明 | 已记录 |
| `progress_app_general` / `progress_pipe_general` 公理 | 非 lam 应用的类型安全未形式化证明 | 已记录 |
| Refinement bridge 缺少 Rust 转录正确性证明 | 需要外部工具（coq-of-rust / Aeneas） | 长期 |

---

## 9. 对照表 / Cross-Reference

### 9.1 关键文件索引

| 层次 | 文件 | 职责 |
|------|------|------|
| 效果判定（Rust） | `crates/n3v3-common/src/lib.rs` | `is_effectful_builtin()` — 单一权威来源 |
| 进程执行（Rust） | `crates/n3v3-std/src/io/mod.rs` | 所有 `builtin_exec_*` + 安全检查 |
| 文件 I/O（Rust） | `crates/n3v3-std/src/io/fs.rs` | `builtin_read_file`, `builtin_write_file` |
| 求值器接入（Rust） | `crates/n3v3-eval/src/eval.rs` | builtin 分发 |
| 纯语义（Lean） | `formal/n3v3/Spec/Eval.lean` | `BigStep` — 大步操作语义 |
| 效果语义（Lean） | `formal/n3v3/Spec/Effects.lean` | `EffectEval` — 15 条规则 |
| 安全证明（Lean） | `formal/n3v3/Verify/` | Path, Environ, Limits |
| 精化桥（Lean） | `formal/n3v3/Refinement/` | Rust ↔ Lean 对应 |
| 类型安全（Lean） | `formal/n3v3/Proofs/Safety.lean` | `progress_preservation` + `type_safety` |

### 9.2 安全审计闭合状态

| 审计编号 | 问题 | Rust 修复 | Lean 定理 | 精化桥 |
|---------|------|----------|----------|--------|
| H-1 | stdin 无大小限制 | `MAX_STDIN_BYTES` 检查 | `Verify/Limits.lean` | `Refinement/Limits.lean` |
| H-2 | stdout/stderr 无大小限制 | `MAX_OUTPUT_BYTES` 检查 | `Verify/Limits.lean` | `Refinement/Limits.lean` |
| M-1 | 路径遍历（`..`） | `resolve_redirect_path` | `Verify/Path.lean` | `Refinement/Path.lean` |
| M-4 | LD_PRELOAD 注入 | `configured_process_command` | `Verify/Environ.lean` | `Refinement/Environ.lean` |

**全部 4 项安全审计已完成 Rust 修复 + Lean 机器验证 + 精化桥。**

---

## 10. 维护规则 / Maintenance Rules

### 10.1 新增效果 builtin 的检查清单

当新增一个效果 builtin 时，必须：

1. [ ] 在 `n3v3_common::is_effectful_builtin()` 中注册
2. [ ] 在 `Spec/Effects.lean` 中添加对应的 `EffectEval` 规则
3. [ ] 在 Rust 实现中添加安全检查（大小限制、路径安全、环境过滤）
4. [ ] 在 `Verify/` 中添加安全定理（如果涉及新的安全属性）
5. [ ] 在 `tests/end_to_end.rs` 中添加端到端测试
6. [ ] 更新本文档的效果操作清单（第 3 节）
7. [ ] 更新 `feature-matrix.md` 的功能状态

### 10.2 纯/效果的边界不应被破坏

以下模式是**反模式**，不应出现：

- ❌ 纯函数通过隐藏的全局状态产生副作用
- ❌ 效果 builtin 被标记为纯（绕过 `--pure` 检查）
- ❌ Lean 规范缺少对应 Rust 实现的规则
- ❌ 安全约束被作为"实现细节"而非语义前提

---

## 附录 A：EffectEval 规则完整签名

见 `formal/n3v3/Spec/Effects.lean`。每条规则的完整签名包含：

- 求值环境 `env` 和 I/O 状态 `σ`（输入/输出）
- 参数的纯求值前提（`BigStep` 子推导）
- 安全约束前提（大小限制）
- 输出值和新的 I/O 状态

## 附录 B：更新日志

| 日期 | 版本 | 变更 |
|------|------|------|
| 2026-05-09 | v1.0 | 初始版本。G4 决策门文档。覆盖 15 条 EffectEval 规则、4 项安全审计、完整的运行时对象模型。 |

---

## 11. 变更记录 / Changelog

### 11.1 v1.2 (2026-06-17)
- v4.0.1 发布：所有效果操作已实现，crates.io 已发布
- 所有安全审计闭合（H-1, H-2, M-1, M-4）

### 11.0 v1.1 (2026-05-12)
- 新增 Stream<T> 效果分类 (G 组，14 个 API)
- 更新统计数字
- 更新类型层次图
- 关联: `stream-design.md` v1.0
