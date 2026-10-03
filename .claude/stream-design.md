# n3v3 Stream<T> Design Document

<div align="center">

**版本**: v1.1
**日期**: 2026-06-17
**Status / 状态**: Historical design (v1.1); Implemented: 13 Stream<T> APIs in the current standard library and type checker (`scripts/counts.sh stream_apis`). `io.streamWrite` remains a design proposal, not a registered API.
历史设计（v1.1）；当前标准库及类型检查器已实现 13 个 Stream<T> API（`scripts/counts.sh stream_apis`）。`io.streamWrite` 仍是设计提案，并未注册为 API。
**关联**: [Effect Boundary](./effect-boundary-design.md) · [Forward Plan](./forward-plan.md) · [Feature Matrix](../docs/project/feature-matrix.md)
**作者**: Chief Architect

</div>

---

The motivation and implementation sketches below record the original proposal, not a complete description of today's runtime. For current signatures see [`docs/reference/api.md`](../docs/reference/api.md); for runtime effect classification see §3 and §6 below.
下文的动机与实现草图记录的是原始方案，不代表当前 runtime 的完整说明。当前签名见 [`docs/reference/api.md`](../docs/reference/api.md)；当前构造器的效果分类见下文 §3 和 §6。

## 1. 动机 / Motivation

### 1.1 问题陈述

Before `Stream<T>` was introduced, n3v3 streaming I/O supported only **callbacks**:
引入 `Stream<T>` 之前，n3v3 的流式 I/O 仅支持**回调模式**：

```n3v3
-- Historical callback APIs (not the only streaming APIs today)
io.execCommandStreaming(cmd, |line| { io.print(line) });
io.execPipelineStreaming(pipe, |line| { io.print(line) });
io.readFileLines(path, |line| { io.print(line) });
```

这种模式有根本局限：

| 问题 | 表现 | Bash 等价 |
|------|------|-----------|
| 无法组合变换 | 不能 filter/map 后再 pipe | `cmd1 \| grep foo \| cmd2` |
| 无法惰性消费 | 必须一次性配好回调 | `cmd \| head -5` |
| 无法中断流 | 回调里 return 不停止生产者 | `Ctrl+C` on pipe |
| 无法存储流 | 不能把流赋值给变量再传递 | `output=$(cmd)` |

**Original gap (now addressed):** there was no first-class `Stream<T>`.
**原始缺口（现已解决）**：当时没有一等 `Stream<T>` 类型。

### 1.2 目标

1. **一等 `Stream<T>` 类型**，可构造、变换、消费、传递
2. **声明式管道组合**：`cmd1 |> streamFilter(grep) |> cmd2`
3. **惰性求值**：元素按需产生，支持背压
4. **可取消**：消费者可以停止读，生产者收到信号停止
5. **与现有类型无缝集成**：Command → Stream, Stream → Command, Stream → List

---

## 2. 类型定义 / Type Definition

### 2.1 n3v3 语言层

```n3v3
-- Stream<T> 是一等类型
-- T 是流中元素的类型

-- 构造流 (值语义，惰性)
io.streamLines(path: Path): Stream<String>
io.streamCommand(cmd: Command): Stream<String>     -- stdout 行流
io.streamList(list: List<T>): Stream<T>             -- 将列表转为流
io.streamBytes(path: Path): Stream<Bytes>           -- 字节块流

-- 变换流 (惰性，返回新流)
io.streamMap(s: Stream<A>, f: A -> B): Stream<B>
io.streamFilter(s: Stream<T>, f: T -> Bool): Stream<T>
io.streamTake(s: Stream<T>, n: Int): Stream<T>      -- 取前 n 个
io.streamDrop(s: Stream<T>, n: Int): Stream<T>      -- 跳过前 n 个

-- 消费流 (触发求值)
io.streamCollect(s: Stream<T>): List<T>             -- 收集为列表
io.streamPipe(s: Stream<String>, cmd: Command): ProcessResult  -- 流入命令 stdin
io.streamWrite(s: Stream<String>, path: Path): Unit -- proposed, not registered / 设计提案，当前未注册
io.streamForEach(s: Stream<T>, f: T -> Unit): Unit  -- 逐元素消费
io.streamFold(s: Stream<T>, init: A, f: A -> T -> A): A  -- 严格折叠

-- 超时控制
io.streamWithTimeout(s: Stream<T>, ms: Int): Stream<Option<T>>  -- 元素级超时
```

### 2.2 Rust 实现层

```rust
// crates/n3v3-eval/src/value.rs

/// Opaque stream runtime object.
/// The generic parameter is erased at runtime; type checking ensures safety.
#[derive(Clone)]
pub struct StreamValue {
    /// Shared mutable iterator state behind Rc<RefCell>.
    /// Uses a bounded channel for backpressure.
    inner: Rc<RefCell<StreamState>>,
}

enum StreamState {
    /// Channel-based: producer thread pushes, consumer polls.
    Channel {
        rx: std::sync::mpsc::Receiver<Result<Value, String>>,
        /// Signal to producer that consumer stopped reading.
        cancelled: Rc<std::sync::atomic::AtomicBool>,
    },
    /// Iterator-based: wraps an eager iterator (for list sources).
    Iterator {
        iter: Rc<RefCell<Box<dyn Iterator<Item = Value>>>>,
    },
    /// Exhausted or consumed.
    Done,
}
```

### 2.3 Value 类型层次（更新后）

```
Value (运行时值)
  ├── ... (现有基本类型)
  ├── Value.command(CommandValue)         ← 已有
  ├── Value.pipeline(PipelineValue)       ← 已有
  ├── Value.processResult(ProcessResult)  ← 已有
  ├── Value.task(TaskValue)               ← 已有
  ├── Value.stream(StreamValue)           ← 新增
  └── Value.unit / Value.list / ...
```

---

## 3. API 详解 / API Specification

### 3.1 构造流（按当前 runtime 边界）

这些 API 返回惰性流对象；但当前 runtime 的部分构造器会在构造阶段执行
宿主操作，因此不能统一视为纯构造。类型检查器与用户文档的规范参数是
`String`；runtime 额外接受 `Path` 作为兼容扩展。

| Builtin | 签名 | 返回 | 当前效果 |
|---------|------|------|------|
| `io.streamLines` | `String -> Stream<String>` | `Stream<String>` | 构造时打开文件并启动生产者 |
| `io.streamCommand` | `Command -> Stream<String>` | `Stream<String>` | 构造时启动进程并启动生产者 |
| `io.streamList` | `List<T> -> Stream<T>` | `Stream<T>` | 纯数据转换 |
| `io.streamBytes` | `String -> Stream<Bytes>` | `Stream<Bytes>` | 构造时打开文件并启动生产者 |

> **设计目标与当前实现的差异**：理想的 cold Stream 应在消费时才打开文件或
> 启动进程；当前实现尚未达到该契约。恢复 cold 语义前，必须同步更新
> `intrinsic_metadata`、typeck、HIR evaluator、Lean 规则和行为测试。

### 3.2 变换流（无副作用，惰性）

这些 API 返回新的 `StreamValue`，包装原始流加上变换闭包：

| Builtin | 签名 | 返回 | 说明 |
|---------|------|------|------|
| `io.streamMap` | `Stream<A> * (A -> B) -> Stream<B>` | `Stream<B>` | 逐元素映射 |
| `io.streamFilter` | `Stream<T> * (T -> Bool) -> Stream<T>` | `Stream<T>` | 惰性过滤 |
| `io.streamTake` | `Stream<T> * Int -> Stream<T>` | `Stream<T>` | 截断 |
| `io.streamDrop` | `Stream<T> * Int -> Stream<T>` | `Stream<T>` | 跳过 |

> **设计决策**：变换是纯函数（`fn(A) -> B` 无 `effect`），所以不涉及 `IOState`。类比：Rust 的 `Iterator::map`。

### 3.3 消费流（有副作用，触发 I/O）

这些 API 触发流的实际求值：

| Builtin | 签名 | 有效果？ | 说明 |
|---------|------|---------|------|
| `io.streamCollect` | `Stream<T> -> List<T>` | ✅ | 收集为列表（可能触发 I/O） |
| `io.streamPipe` | `Stream<String> * Command -> ProcessResult` | ✅ | 流入命令 stdin |
| `io.streamWrite` | `Stream<String> * Path -> Unit` | Planned / 计划中 | 原始设计提案；当前未注册，不能调用 |
| `io.streamForEach` | `Stream<T> * (T -> Unit) -> Unit` | ✅ | 逐元素消费（有副作用） |
| `io.streamFold` | `Stream<T> * A * (A -> T -> A) -> A` | ✅ | 严格折叠（会触发 I/O） |

### 3.4 超时控制

`io.streamWithTimeout` 当前在构造阶段读取计时器并启动生产者，因此归入
`Time` 效果；它仍返回 `Stream<Option<T>>`，消费阶段继续可能触发源流效果。

```n3v3
io.streamWithTimeout(s: Stream<String>, ms: Int): Stream<Option<String>>
```

---

## 4. 与现有类型的集成 / Integration

### 4.1 Command → Stream → Command 管道

```n3v3
-- Bash:  cmd1 | grep "error" | wc -l
-- n3v3:
use std.io = io;
use std.string = str;

let cmd1 = io.command("journalctl", ["-n", "100"]);
let cmd2 = io.command("wc", ["-l"]);

let stream = io.streamCommand(cmd1);
let filtered = io.streamFilter(stream, |line| { str.contains(line, "error") });
let result = io.streamPipe(filtered, cmd2);
```

### 4.2 `|>` 管道语法集成

```n3v3
-- Pipeline composition sketch (current v4 lambda syntax):
let result = io.command("journalctl", ["-n", "100"])
    |> io.streamCommand
    |> |s| { io.streamFilter(s, |line| { str.contains(line, "error") }) }
    |> |s| { io.streamPipe(s, cmd2) };
```

> `|>` 当前已支持 `Command |> Command → Pipeline` 和 `x |> f → f(x)`。
> 引入 `Stream<T>` 后，`Stream<T> |> fn → fn(stream)` 自然工作，
> 无需额外语法变化。

### 4.3 Stream 与 Task 的关系

| 维度 | Stream<T> | Task<T> |
|------|-----------|---------|
| 求值方向 | **pull** (消费者拉取) | **push** (生产者在后台计算) |
| 生命周期 | 一次性消费 | 可 poll 多次 |
| 取消 | 消费者 drop stream | `io.cancel(spawnId)` |
| 并发 | 流元素串行产生 | 后台线程 |
| 典型用途 | 管道、文件处理 | 后台任务、超时等待 |

两者互补且正交。`Stream<T>` 不替换 `Task<T>`。

---

## 5. 内部实现 / Internal Implementation

### 5.1 Channel-based 流（命令/文件源）

```
┌──────────────────┐     bounded channel     ┌──────────────────┐
│  Producer Thread  │ ──────────────────────> │  Consumer (eval)  │
│  (spawned lazily) │   Result<Value,String>  │  (poll in eval)   │
│                   │ <────────────────────── │                   │
│  AtomicBool       │     cancel signal       │  AtomicBool       │
└──────────────────┘                         └──────────────────┘
```

- **Channel 容量**：默认 16 个元素（提供背压）
- **Cancel 信号**：消费者 drop 时设置 `AtomicBool`，生产者检查后停止
- **错误传播**：生产者发送 `Err(String)`，消费者 `next()` 返回 `Err`

### 5.2 Iterator-based 流（列表源）

```rust
StreamState::Iterator {
    iter: Rc<RefCell<Box<dyn Iterator<Item = Value>>>>,
}
```

简单包装现有 `Vec<Value>` 的 `into_iter()`。无 channel 开销。

### 5.3 变换链

```
StreamValue(原始源)
  └── StreamState::Channel { rx, cancelled }
       │
       ▼ streamMap(f)
StreamValue(变换包装)
  └── StreamState::Wrapped {
          source: StreamValue,     // 上游
          transform: Transform,    // 变换函数
      }
```

每个变换 API 返回一个新的 `StreamValue`，持有上游的引用和变换闭包。消费时递归 poll 上游。

### 5.4 大小限制（安全）

沿用现有常量：

```rust
// crates/n3v3-std/src/io/mod.rs (现有)
const MAX_STDIN_BYTES: usize = 10 * 1024 * 1024;     // 10 MB
const MAX_OUTPUT_BYTES: usize = 50 * 1024 * 1024;    // 50 MB
const MAX_STREAM_LINES: usize = 100_000;              // 100k lines
```

`StreamValue` 消费时逐个检查 limits，超出则返回 `Err` 并 cancel 生产者。

---

## 6. 效果边界 / Effect Boundary

### 6.1 分类

| `io.streamLines` | **有宿主效果**（构造） | 当前打开文件并启动生产者线程 |
| `io.streamCommand` | **有宿主效果**（构造） | 当前启动进程并启动生产者线程 |
| `io.streamList` | **无副作用**（构造） | 纯数据转换 |
| `io.streamBytes` | **有宿主效果**（构造） | 当前打开文件并启动生产者线程 |
| `io.streamMap` | **无副作用**（变换） | 纯函数包装 |
| `io.streamFilter` | **无副作用**（变换） | 纯函数包装 |
| `io.streamTake` | **无副作用**（变换） | 纯计数 |
| `io.streamDrop` | **无副作用**（变换） | 纯计数 |
| `io.streamCollect` | **有副作用**（消费） | 触发文件/进程 I/O |
| `io.streamPipe` | **有副作用**（消费） | 启动进程 |
| `io.streamWrite` (planned / 计划中) | 未注册，不属于当前效果分类 | 原始设计中的文件写入操作 |
| `io.streamForEach` | **有副作用**（消费） | 执行有副作用回调 |
| `io.streamFold` | **有副作用**（消费） | 触发 I/O |
| `io.streamWithTimeout` | **有宿主效果**（构造） | 当前读取计时器并启动生产者 |

### 6.2 `intrinsic_metadata` 当前事实

效果分类的 canonical source 是 `crates/n3v3-common/src/action.rs` 的
`intrinsic_metadata` registry；`.claude/` 文档和 Lean 规则不能再维护独立的
`is_effectful_builtin` 白名单。当前 registry 将 `streamLines`、`streamBytes`
归为 `FILE_READ`，`streamCommand` 归为 `PROCESS`，`streamWithTimeout` 归为
`TIME`，并保留 `streamList` 与变换器为纯操作。


---

## 7. 形式化语义 / Formal Semantics (Lean 4)

### 7.1 当前形式化边界

本节保留 Stream 的设计草图；当前 canonical 规则以
`formal/n3v3/Spec/Effects.lean` 的 EffectEval v4.3 和
`.claude/effect-boundary-design.md` 为准。构造器的宿主效果必须按
`intrinsic_metadata` 的 `FILE_READ`、`PROCESS`、`TIME` 分类传播。

### 7.2 设计草图中的 EffectEval 规则
```
EffectEval v4.2 新增规则 (8 条):

-- 消费规则
streamCollect  : eval(stream) ⇓ list → EffectEval env σ e (list) σ'
streamPipe     : eval(stream) ⇓ cmd → EffectEval env σ e (processResult) σ'
streamWrite    : eval(stream) ⇓ path → EffectEval env σ e unit σ'
streamForEach  : 逐元素 f(elem) → EffectEval env σ e unit σ'
streamFold     : 严格折叠 → EffectEval env σ e acc σ'

-- 流元素产生规则 (内部)
streamNext     : channel recv → Some(elem) | None (EOF) | Err

-- 取消规则
streamCancel   : cancelled → EffectEval ... → unit
streamTimeout  : recv_timeout ms → Some(elem) | None
```

### 7.2 精化桥 (Refinement)

`StreamValue` 的 Rust 状态机 (Channel/Iterator/Done) 对应 Lean 中的 `StreamState` 归纳类型：

```lean
inductive StreamState (α : Type) where
  | pending : Stream (List (Result α String)) → StreamState α
  | done    : StreamState α
```

---

## 8. 实现阶段 / Implementation Phases

### Phase A: 核心类型 + 基础 API（1-2 天）

1. `StreamValue` 类型定义 + `Value::Stream` variant
2. `io.streamList` (最简——迭代器包装)
3. `io.streamCollect` (消费)
4. 3-5 个 E2E 测试

### Phase B: Channel-based 流（1-2 天）

5. `io.streamLines` (文件源)
6. `io.streamCommand` (命令源)
7. Channel 背压 + cancel 信号
8. 5+ E2E 测试（含 cancel）

### Phase C: 变换 + 管道（1 天）

9. `io.streamMap`, `io.streamFilter`, `io.streamTake`, `io.streamDrop`
10. `io.streamPipe` (流入命令)
11. 5+ E2E 测试

### Phase D: 形式化 + 文档（1 天）

12. EffectEval 规则更新 (8 条)
13. `is_effectful_builtin()` 更新
14. 更新 effect-boundary.md, feature-matrix.md

---

## 9. 非目标 / Non-Goals

- ❌ 异步 I/O (tokio/async)——Phase 4 不需要引入异步运行时
- ❌ 无限流/生成器——`Stream<T>` 总是有界的
- ❌ 流合并 (merge/zip)——属于 Phase 5 的并发模型
- ❌ 流序列化——与当前设计空间无关

---

## 10. 风险 / Risks

| 风险 | 概率 | 缓解 |
|------|------|------|
| Channel 死锁 | 中 | Bounded channel + 超时 + cancel 信号 |
| 线程泄漏 | 低 | Consumer drop 时 join producer thread |
| 性能（线程开销） | 低 | Iterator-based 路径无需线程；Channel 路径的线程开销远小于进程启动 |
| 与 Task 语义混淆 | 中 | 文档明确区分 pull vs push；类型不兼容 |
| Lean 形式化复杂度 | 中 | 先用 axiom 建模 channel，后续精化 |

---

## 11. 决策记录 / Decision Log

| ID | 决策 | 理由 |
|----|------|------|
| D-S01 | 构造时不启动 I/O（惰性） | 保持纯构造/副作用分离原则 |
| D-S02 | Channel 容量 16 | 平衡内存与吞吐；与 Rust `std::sync::mpsc` 默认一致 |
| D-S03 | 变换是纯函数（无 effect 签名） | `map`/`filter` 的回调不应有副作用 |
| D-S04 | `Stream<T>` 不替换 `Task<T>` | Pull vs push 正交 |
| D-S05 | `io.streamPipe` 返回 `ProcessResult`（阻塞） | 与 `io.execPipeline` 一致；流式 pipe 留到 Phase 5 |
| D-S06 | 元素级超时用 `Stream<Option<T>>` | 保持 `Stream<T>` 纯净；超时是可选的包装器 |
