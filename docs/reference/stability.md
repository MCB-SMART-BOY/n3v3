<div align="center">

<img src="../../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>n3v3 Stability &amp; Platform Support</h1>

<p><em>稳定性与平台支持</em></p>

<p>
  <strong><a href="../../README.md">Home</a></strong> ·
  <strong><a href="../README.md">Docs</a></strong>
</p>

</div>


## API Stability Tiers

This document defines the stability guarantees for the n3v3 standard library (stdlib), platform support, and compiler-facing AST APIs. Current release is v5.0.2 (v4.0 syntax is canonical; legacy keywords accepted for backward compatibility). Breaking changes to stable APIs will only occur with a major version bump.

## Tier 1: Stable ✅

**Guarantee**: These APIs are guaranteed not to break within v5.x. They have been extensively exercised in real-world usage and their semantics are well-understood.

### Core I/O

| API | Signature | Description |
|-----|-----------|-------------|
| `print` | `(value: a) -> Unit` | Print value to stdout without newline |
| `println` | `(value: a) -> Unit` | Print value to stdout with newline |
| `io.read` | `(path: String) -> String` | Read file contents from a path |
| `io.write` | `(path: String, content: String) -> Unit` | Write string content to a file |
| `io.readFile` | `(path: String) -> String` | Read entire file contents |
| `io.writeFile` | `(path: String, content: String) -> Unit` | Write string to file |
| `io.execCommand` | `(cmd: Command) -> ProcessResult` | Execute a command synchronously |
| `io.execPipeline` | `(pipeline: Pipeline) -> ProcessResult` | Execute a pipeline synchronously |
| `io.command` | `(name: String, args: List String) -> Command` | Construct a Command value |
| `io.pipeline` | `(cmds: List Command) -> Pipeline` | Construct a Pipeline value |
| `io.args` | `() -> (List String, Record)` | Get script arguments and parsed flags |
| `io.getEnv` | `(name: String) -> Option String` | Get environment variable |
| `io.glob` | `(pattern: String) -> List Path` | Glob pattern matching |

Type-checker contract: `io.read(String) -> String`. The runtime additionally accepts a `Path` value, but that wider branch is an implementation convenience rather than a separate typed signature.
类型检查器契约：`io.read(String) -> String`。运行时额外接受 `Path` 值，但这个更宽的分支属于实现便利，不是独立的类型签名。


### Type Conversion

| API | Signature | Description |
|-----|-----------|-------------|
| `toString` | `(value: a) -> String` | Convert any value to string |
| `toInt` | `(value: a) -> Int` | Convert to Int (from Int, Float, String, Bool) |
| `toFloat` | `(value: a) -> Float` | Convert to Float (from Int, Float, String) |

### List Operations

`len` is the only global list operation. Everything else lives in the `list`
module and must be qualified (or imported with `use std.list`).
`len` 是唯一的全局列表操作；其余函数都在 `list` 模块中，必须限定调用或先 `use std.list`。

| API | Signature | Description |
|-----|-----------|-------------|
| `len` | `(list: List a) -> Int` | Length of list or string |
| `list.head` | `(list: List a) -> Option a` | First element |
| `list.tail` | `(list: List a) -> List a` | All but first element |
| `list.last` | `(list: List a) -> Option a` | Last element |
| `list.map` | `(f: a -> b, list: List a) -> List b` | Transform each element |
| `list.filter` | `(f: a -> Bool, list: List a) -> List a` | Keep elements matching predicate |
| `list.fold` | `(init: b, f: b -> a -> b, list: List a) -> b` | Left fold — **not usable through the CLI/HIR pipeline**: typed argument order differs from evaluator order; runtime reports `fold expects a list` |
| `list.foldRight` | `(init: b, f: a -> b -> b, list: List a) -> b` | Right fold — **not usable through the CLI/HIR pipeline**: no HIR evaluator dispatch |

### Type Introspection

| API | Signature | Description |
|-----|-----------|-------------|
| `typeOf` | `(value: a) -> String` | Get runtime type name |

### Path Manipulation

| API | Signature | Description |
|-----|-----------|-------------|
| `path.fromString` | `(s: String) -> Path` | Parse string to Path |
| `path.joinPath` | `(base: Path, child: String) -> Path` | Join a path with a child segment |

### TTY / Terminal

| API | Signature | Description |
|-----|-----------|-------------|
| `io.isTTY` | `(fd: Int) -> Bool` | Check if fd is a terminal |
| `io.terminalSize` | `() -> Option<{rows: Int, cols: Int}>` | Get terminal dimensions |
| `io.setRawMode` | `(fd: Int, enable: Bool) -> Unit` | Set terminal raw mode |
| `io.resetTerminal` | `(fd: Int) -> Unit` | Reset terminal to normal mode |
| `io.readKey` | `(fd: Int) -> Int` | Read a single byte from fd |

### String Operations

| API | Signature | Description |
|-----|-----------|-------------|
| `string interpolation (+)` | `(a: String, b: String) -> String` | Concatenate strings |
| `string.contains` | `(s: String, substr: String) -> Bool` | Check substring membership |
| `string.split` | `(s: String, delim: String) -> List String` | Split string by delimiter |

### List Module

| API | Signature | Description |
|-----|-----------|-------------|
| `list.map` | `(f: a -> b, list: List a) -> List b` | Map over list |
| `list.filter` | `(f: a -> Bool, list: List a) -> List a` | Filter list |
| `list.fold` | `(init: b, f: b -> a -> b, list: List a) -> b` | Left fold — **not usable through the CLI/HIR pipeline** (typed argument order differs from evaluator order; `fold expects a list`) |

`list.fold` is type-checked as `(init, function, list)`, but HIR evaluation reads `(init, list, function)`. The `n3v3-std` closure-evaluation stub is not the error reached through the CLI pipeline. `list.foldRight` has a type-checker entry but no HIR dispatch. `list.map` and `list.filter` have separate working HIR dispatch and are not affected by the fold argument mismatch.
`list.fold` 的类型检查参数顺序是 `(init, function, list)`，但 HIR 求值读取 `(init, list, function)`。通过 CLI 流水线运行时不会进入 `n3v3-std` 的闭包求值桩函数。`list.foldRight` 在类型检查器中有条目，但没有 HIR 派发。`list.map` 和 `list.filter` 有独立的 HIR 派发，不受 fold 参数顺序不一致的影响。

### Basic Types

All these types are stable:

- `Int` — arbitrary-precision signed integer
- `Float` — 64-bit IEEE 754 float
- `Bool` — Boolean (`true` / `false`)
- `String` — UTF-8 string
- `Char` — Unicode scalar value
- `List a` — Homogeneous list
- `Record` — Named field records
- `Unit` — Unit type (`()`)

### Process Result Accessors

| API | Signature | Description |
|-----|-----------|-------------|
| `io.processSuccess` | `(p: ProcessResult) -> Bool` | Whether process exited with code 0 |
| `io.processStdout` | `(p: ProcessResult) -> String` | Process stdout |
| `io.processStderr` | `(p: ProcessResult) -> String` | Process stderr |
| `io.processCode` | `(p: ProcessResult) -> Int` | Process exit code |

---

## Tier 2: Stable but Evolving ⚠️

**Guarantee**: These APIs are stable in their current form but may gain new optional parameters in minor releases. Existing call sites will not break.

### Stream<T> API (13 APIs)

| API | Signature | Description |
|-----|-----------|-------------|
| `io.streamList` | `(list: List a) -> Stream a` | Create stream from list |
| `io.streamLines` | `(path: String) -> Stream String` | Stream lines from file |
| `io.streamCommand` | `(cmd: Command) -> Stream String` | Stream stdout of command |
| `io.streamBytes` | `(path: String) -> Stream Bytes` | Stream raw bytes |
| `io.streamCollect` | `(s: Stream a) -> List a` | Collect stream into list |
| `io.streamMap` | `(s: Stream a, f: a -> b) -> Stream b` | Map over stream |
| `io.streamFilter` | `(s: Stream a, f: a -> Bool) -> Stream a` | Filter stream |
| `io.streamTake` | `(s: Stream a, n: Int) -> Stream a` | Take first n elements |
| `io.streamDrop` | `(s: Stream a, n: Int) -> Stream a` | Drop first n elements |
| `io.streamPipe` | `(s: Stream<String>, cmd: Command) -> ProcessResult` | Collect the stream and pipe it to a command |
| `io.streamForEach` | `(s: Stream a, f: a -> Unit) -> Unit` | Apply closure to each element |
| `io.streamFold` | `(s: Stream a, init: b, f: b -> a -> b) -> b` | Left fold over stream |
| `io.streamWithTimeout` | `(s: Stream a, ms: Int) -> Stream (Option a)` | Stream with timeout per element |

### Task Management (7 APIs)

| API | Signature | Description |
|-----|-----------|-------------|
| `io.spawn` | `(task: Task[ProcessResult]) -> Int` | Spawn a task and return its integer spawn ID |
| `io.poll` | `(spawnId: Int) -> Option[ProcessResult]` | Poll a spawn ID for a completed process result |
| `io.cancel` | `(spawnId: Int) -> Unit` | Cancel and remove a spawned task |
| `io.awaitTask` | `(task: Task[ProcessResult]) -> ProcessResult` | Block until a task completes |
| `io.awaitTasks` | `(tasks: List[Task[ProcessResult]]) -> List[ProcessResult]` | Await multiple tasks |
| `io.awaitAny` | `(tasks: List[Task[ProcessResult]]) -> ProcessResult` | Await the first completed task |
| `io.awaitTaskWithTimeout` | `(task: Task[ProcessResult], ms: Int) -> Option[ProcessResult]` | Await with a timeout |


### Registry CLI (Implemented)

On Unix, `n3v3 registry-update`, `n3v3 registry-serve`, and
`n3v3 registry-publish` are implemented CLI commands.
在 Unix 上，`n3v3 registry-update`、`n3v3 registry-serve` 和
`n3v3 registry-publish` 是已实现的 CLI 命令。


### File I/O

| API | Signature | Description |
|-----|-----------|-------------|
| `io.readFileLines` | `(path: String, f: String -> Unit) -> Unit` | Read file lines and invoke the callback |
| `io.atomicWrite` | `(path: String, content: String) -> Unit` | Atomic file write |
| `io.tempDir` | `(callback: Path -> A) -> A` | Create a temporary directory for a callback |
| `io.createDirAll` | `(path: String) -> Unit` | Create directory tree |
| `io.removeDirAll` | `(path: String) -> Unit` | Remove directory tree |

### Path Operations

| API | Signature | Description |
|-----|-----------|-------------|
| `path.parentPath` | `(p: Path) -> Option Path` | Parent directory |
| `path.filenamePath` | `(p: Path) -> Option String` | Filename component |
| `path.extensionPath` | `(p: Path) -> Option String` | File extension |

### Signal Handling

| API | Signature | Description |
|-----|-----------|-------------|
| `io.onSignal` | `(signal: String, handler: () -> Unit) -> Unit` | Register signal handler (Unix-only; other platforms report an error) |

### Bytes Type

| API | Signature | Description |
|-----|-----------|-------------|
| `bytes.fromString` | `(s: String) -> Bytes` | Convert string to bytes |
| `bytes.fromList` | `(list: List Int) -> Bytes` | Convert byte list to Bytes |

---

## Tier 3: Experimental 🔬

**Guarantee**: These APIs may change in minor releases. They represent emerging capabilities that need real-world validation before stabilization. Tier 3 APIs will be promoted to Tier 2 after **2 minor releases** of real-world usage without API changes.

### Job Control (2 APIs)

| API | Signature | Description |
|-----|-----------|-------------|
| `io.jobs` | `() -> List {id: Int, state: String}` | List background jobs |
| `io.waitAnyJob` | `() -> {id: Int, result: ProcessResult}` | Wait for any background job |

### Effect Control Flow

| API | Signature | Description |
|-----|-----------|-------------|
| `io.retry` | `(check: () -> Bool, maxAttempts: Int, backoffMs: Int) -> Bool` | Retry until the check passes |
| `io.ensure` | `(check: () -> Bool, timeoutMs: Int, intervalMs: Int) -> Bool` | Wait until the check passes or the timeout elapses |
| `io.every` | `(ms: Int) -> Event Int` | Periodic event source |
| `io.watchFile` | `(path: String) -> Event String` | File-change event source |

### Reactive System

| API | Signature | Description |
|-----|-----------|-------------|
| `io.reactive` | `(event: Event a) -> Live a` | Turn an event source into a live value |
| `io.liveNext` | `(live: Live a) -> a` | Get the next value |
| `io.liveCurrent` | `(live: Live a) -> Option a` | Read the current value without waiting |
| `io.liveCancel` | `(live: Live a) -> Unit` | Stop the live value |

### Fetch

| API | Signature | Description |
|-----|-----------|-------------|
| `fetch.url` | `(url: String) -> {path: String, hash: String, cached: Bool}` | Fetch URL into the store |
| `fetch.urlWithHash` | `(url: String, hash: String) -> {path: String, hash: String, cached: Bool}` | Fetch with integrity check |
| `fetch.git` | `(url: String, rev: String) -> {path: String, hash: String, cached: Bool}` | Fetch git repository |

---

## Promotion Process

Tier 3 → Tier 2 promotion requires:

1. **2 minor releases** of real-world usage without API changes.
2. **No open design issues** against the API surface.
3. **Documentation coverage** in the API reference.
4. **Test coverage** in the end-to-end test suite.

Tier 2 → Tier 1 promotion requires:

1. **4 minor releases** of Tier 2 stability.
2. **Widespread adoption** across the n3v3 ecosystem.
3. **Formal specification** of behavior (where applicable).

## Breaking Change Policy

- **Tier 1**: Breaking changes only with a major version bump (v5.0).
- **Tier 2**: Breaking changes require a deprecation cycle of at least 1 minor release.
- **Tier 3**: Breaking changes may happen in any minor release without prior deprecation.

When a Tier 2 API needs a breaking change, the old API must:
1. Emit a **deprecation warning** at compile time.
2. Be documented as deprecated in the API reference.
3. Remain functional for at least **1 minor release** before removal.

## Platform Support Tiers

| Tier | Platform | Language Core | REPL | LSP | Sandbox | System Config | Package Mgmt |
|------|----------|:---:|:---:|:---:|:-------:|:------------:|:------------:|
| **Tier 1** | Linux (x86_64, aarch64) | ✅ | ✅ | ✅ | ✅ Native | ✅ | ✅ |
| **Tier 2** | macOS (x86_64, aarch64) | ✅ | ✅ | ✅ | ✅ Docker | ❌ | ❌ |
| **Tier 3** | Windows (x86_64) | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ |

### Tier Definitions

- **Tier 1 (Linux)**: Full language and system support. The v5.0.1 GitHub Release was blocked by an ARM64 cross-link failure; v5.0.2 builds natively on ARM64 and publishes both Linux archives. This is the primary development target.
- **Tier 2 (macOS)**: Near-full language support. Sandbox uses Docker backend. System config and package management unavailable (requires Linux namespaces); verified cache fetch cannot publish store entries without Linux `renameat2(RENAME_NOREPLACE)`. CI runs workspace tests.
- **Tier 3 (Windows)**: Language core only (lexer, parser, HIR, typeck, eval, fmt, LSP). Signal handling, TTY, sandbox, system config, package management, and registry are unavailable. `OsHost` is unsupported, and verified cache fetch cannot publish store entries. CI runs workspace tests, including fail-closed platform-boundary cases.

### What Each Tier Gets

| Capability | Tier 1 | Tier 2 | Tier 3 |
|-----------|:------:|:------:|:------:|
| Language compilation and execution | ✅ | ✅ | ✅ |
| REPL | ✅ | ✅ | ✅ |
| LSP (language server) | ✅ | ✅ | ✅ |
| Formatter | ✅ | ✅ | ✅ |
| Stream<T> (13 APIs) | ✅ | ✅ | ✅ |
| Task (spawn/poll/cancel) | ✅ | ✅ | ✅ |
| Signal handling (INT/TERM/HUP) | ✅ | ❌ | ❌ |
| TTY (raw mode, terminal size) | ✅ | ❌ | ❌ |
| Native sandbox (Linux namespaces) | ✅ | ❌ | ❌ |
| Docker sandbox backend | ✅ | ✅ | ❌ |
| System config (generations) | ✅ | ❌ | ❌ |
| Package management | ✅ | ❌ | ❌ |
| Registry client | ✅ | ❌ | ❌ |
| Full CI test suite | ✅ | ✅ (workspace tests; unsupported operations assert rejection) | ✅ (workspace tests; unsupported operations assert rejection) |
| Release binaries | ✅ (v5.0.2 x86_64, aarch64) | ✅ (v5.0.2 x86_64, aarch64) | ✅ (v5.0.2 x86_64) |

### Promotion Path

- **Tier 3 → Tier 2**: Requires Docker sandbox backend validation + full CI test matrix.
- **Tier 2 → Tier 1**: Requires native sandbox (Linux namespaces) or equivalent + full ecosystem feature parity.

## Versioning / 版本策略

n3v3 follows a **SemVer-hybrid** model, adapted for rapid language evolution:

| Version | Meaning | Breaking changes |
|---------|---------|------------------|
| **Major** (v4 → v5) | Semantic completeness milestone. | Allowed, with documented migration paths. |
| **Minor** (v5.0 → v5.1) | Feature release. New APIs, syntax improvements, tooling expansion. | Allowed with deprecation warnings (1 minor release grace period). |
| **Patch** (v5.0.0 → v5.0.1) | Bug fix release. No new features. | Not allowed. |

### Deprecation Policy / 废弃策略

All external-facing changes follow this lifecycle:

Historical v4.x example: the following timeline records the AST evaluator migration completed before v5.0.0; it is not the v5.x deprecation schedule.
v4.x 历史示例：下面的时间线记录 v5.0.0 之前完成的 AST evaluator 迁移，不是 v5.x 的废弃时间表。

```
v4.X: deprecation warning → v4.X+1: continued warning → v4.X+2: removal
```

Example (AST evaluator removal):
- **v3.18.0**: `#[deprecated]` on `AstEnv`/`AstEvaluator` (warning emitted)
- **v4.0.4**: Types removed (breaking change in major)

### v4.0 Exit Criteria (Historical)

v4.0 marked the transition from "language prototype" to "stable language platform":

1. AST compat path (`n3v3_eval::compat`) fully removed — **Implemented** in v4.0.4
2. All 6 known implementation gaps closed — **Implemented** in v4.0.4
3. 12 canonical keywords, v4.0 syntax canonical — **Implemented** in v4.0.4
4. Release policy formalized and stable — **Implemented** in v4.0.4
5. 62/62 design audit findings resolved — **Implemented** in v4.0.4
6. Semantic convergence: all features survive lowering without loss — **Implemented** in v4.0.4

### v5.0 AST API cutover

The public AST is a forward-compatible boundary. External consumers must
wildcard-match all public syntax enums, including `ItemKind`, `ExprKind`,
`PatternKind`, `TypeKind`, and their operator/import/literal sub-enums.

AST-to-HIR lowering rejects unsupported import forms and preserves unsupported
expression, statement, and pattern forms as explicit error nodes. Type checking
reports those nodes before evaluation; no unsupported form is silently treated
as a wildcard or empty construct.

### Why Not Strict SemVer?

- n3v3 completed the v4.0 milestone and continues rapid evolution
- Syntax v4.0 solidified the language surface after the v3.0 overhaul
- Minor releases are the primary feature-delivery vehicle
- Major releases represent "quality milestones" rather than "anything that breaks"
