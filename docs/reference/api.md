<div align="center">

<img src="../../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>Standard Library API</h1>

<p><em>标准库接口文档</em></p>

<p>
  <strong><a href="../../README.md">Home</a></strong> ·
  <strong><a href="../README.md">Docs</a></strong>
</p>

</div>

---

> 标准库里有哪些函数，参数是什么，返回什么。


## Using the stdlib / 标准库用法


标准库用命名空间组织。直接 use 用 `list.map`，取个别名也行。

```n3v3
use std.list;                      -- list.map, list.filter 都能用
use std.list (map, filter, fold);  -- 只导入这三个
use std.string = Str;              -- Str.len("hi")
use std.Map;
use std.Set;
```


## Core Builtins (global) / 核心内置函数（全局）


```n3v3
id<A>(x: A) -> A
const<A, B>(x: A, y: B) -> A
print(x: A) -> Unit
len(x: A) -> Int
typeOf(x: A) -> String
toString(x: A) -> String
toInt(x: A) -> Int
toFloat(x: A) -> Float
assert(cond: Bool) -> Unit
assertEq<A>(a: A, b: A) -> Unit
trace(label: A, value: B) -> B
force(x: A) -> A
isEvaluated(x: A) -> Bool
```




## List Module (std.list) / 列表模块（std.list）


```n3v3
[] -> List<A>                      -- 空列表字面量（`list.empty` 目前只有 `<builtin:list.empty>` 值，不可当列表用）
list.singleton<A>(x: A) -> List<A>
list.len<A>(xs: List<A>) -> Int
list.isEmpty<A>(xs: List<A>) -> Bool
list.head<A>(xs: List<A>) -> Option<A>
list.last<A>(xs: List<A>) -> Option<A>
list.tail<A>(xs: List<A>) -> List<A>
list.init<A>(xs: List<A>) -> List<A>
list.get<A>(index: Int, xs: List<A>) -> Option<A>
list.cons<A>(x: A, xs: List<A>) -> List<A>
list.take<A>(n: Int, xs: List<A>) -> List<A>
list.drop<A>(n: Int, xs: List<A>) -> List<A>
list.contains<A>(x: A, xs: List<A>) -> Bool
list.indexOf<A>(x: A, xs: List<A>) -> Option<Int>
list.append<A>(xs: List<A>, ys: List<A>) -> List<A>
list.reverse<A>(xs: List<A>) -> List<A>
list.map<A, B>(f: A -> B, xs: List<A>) -> List<B>
list.filter<A>(pred: A -> Bool, xs: List<A>) -> List<A>
list.fold<A, B>(init: B, f: B -> A -> B, xs: List<A>) -> B   -- 运行时尚未支持：报 "list.fold requires runtime closure evaluation"
list.foldRight<A, B>(init: B, f: A -> B -> B, xs: List<A>) -> B   -- 运行时尚未支持（同上）
list.sum(xs: List<Int>) -> Int
list.product(xs: List<Int>) -> Int
list.sort<A>(xs: List<A>) -> List<A>
list.max(xs: List<Int>) -> Option<Int>
list.min(xs: List<Int>) -> Option<Int>
list.range(start: Int, end: Int) -> List<Int>
list.replicate<A>(n: Int, value: A) -> List<A>
list.zip<A, B>(xs: List<A>, ys: List<B>) -> List[(A, B)]
list.unzip<A, B>(pairs: List[(A, B)]) -> (List<A>, List<B>)
```




## String Module (std.string) / 字符串模块（std.string）


```n3v3
string.len(s: String) -> Int
string.chars(s: String) -> List[Char]
string.split(s: String, sep: String) -> List<String>
string.join(xs: List<String>, sep: String) -> String
string.trim(s: String) -> String
string.upper(s: String) -> String
string.lower(s: String) -> String
string.contains(s: String, needle: String) -> Bool
string.startsWith(s: String, prefix: String) -> Bool
string.endsWith(s: String, suffix: String) -> Bool
string.replace(s: String, from: String, to: String) -> String
string.substring(s: String, start: Int, end: Int) -> String
string.isEmpty(s: String) -> Bool
string.repeat(s: String, n: Int) -> String
string.lines(s: String) -> List<String>
```




## Option Module (std.option) / Option 模块（std.option）


```n3v3
type Option<T> = | Some(T) | None

option.some<A>(x: A) -> Option<A>
option.none -> Option<A>
option.is_some<A>(opt: Option<A>) -> Bool
option.is_none<A>(opt: Option<A>) -> Bool
option.unwrap<A>(opt: Option<A>) -> A
option.unwrap_or<A>(opt: Option<A>, default: A) -> A
```




## Result Module (std.result) / Result 模块（std.result）


```n3v3
type Result<T, E> = | Ok(T) | Err(E)

result.ok<T, E>(x: T) -> Result<T, E>
result.err<T, E>(e: E) -> Result<T, E>
result.is_ok<T, E>(res: Result<T, E>) -> Bool
result.is_err<T, E>(res: Result<T, E>) -> Bool
result.unwrap<T, E>(res: Result<T, E>) -> T
result.unwrap_err<T, E>(res: Result<T, E>) -> E
```




## Math Module (std.math) / 数学模块（std.math）


The current explicit `std.math` surface is intentionally narrow. Today it
contains only the canonical conversion bridges, float predicates, rounding
helpers, unary float transforms, trigonometric helpers, and constant bindings
below.

```n3v3
math.toInt(x: A) -> Int
math.toFloat(x: A) -> Float
math.isNan(x: Float) -> Bool
math.isInf(x: Float) -> Bool
math.floor(x: Float) -> Int
math.ceil(x: Float) -> Int
math.round(x: Float) -> Int
math.sqrt(x: Float) -> Float
math.log(x: Float) -> Float
math.log10(x: Float) -> Float
math.exp(x: Float) -> Float
math.sin(x: Float) -> Float
math.cos(x: Float) -> Float
math.tan(x: Float) -> Float
math.pi -> Float
math.e -> Float
math.inf -> Float
math.nan -> Float
math.abs(x: A) -> A
math.clamp(x: A, lo: A, hi: A) -> A
math.max(a: A, b: A) -> A
math.min(a: A, b: A) -> A
math.pow(base: Int, exp: Int) -> Int
```


当前显式公开的 `std.math` surface 刻意保持很窄。现在只有下面这些
canonical 转换桥、浮点谓词、取整 helper、一元浮点变换、三角 helper 和常量绑定属于 typed public API。



## I/O Module (std.io) / I/O 模块（std.io）


I/O helpers are impure and raise runtime errors on failure.
I/O 函数是非纯的，失败会抛出运行时错误。

```n3v3
io.readFile(path: String) -> String
io.readFilePath(path: Path) -> String
io.readFileBytesPath(path: Path) -> Bytes
io.readDirPath(path: Path) -> List<String>
io.readDirEntryPaths(path: Path) -> List<Path>
io.writeFilePath(path: Path, content: String) -> Unit
io.appendFilePath(path: Path, content: String) -> Unit
io.writeFileBytesPath(path: Path, bytes: Bytes) -> Unit
io.appendFileBytesPath(path: Path, bytes: Bytes) -> Unit
io.readDir(path: String) -> List<String>
io.writeFile(path: String, content: String) -> Unit
io.appendFile(path: String, content: String) -> Unit
io.createDirAll(path: String) -> Unit
io.createDirAllPath(path: Path) -> Unit
io.removeDirAll(path: String) -> Unit
io.removeDirAllPath(path: Path) -> Unit
io.pathExists(path: String) -> Bool
io.pathExistsPath(path: Path) -> Bool
io.isDir(path: String) -> Bool
io.isDirPath(path: Path) -> Bool
io.isFile(path: String) -> Bool
io.isFilePath(path: Path) -> Bool
io.getEnv(name: String) -> Option<String>
io.currentDir() -> String
io.currentDirPath() -> Path
io.homeDirPath() -> Option<Path>
io.command(program: String, args: List<String>) -> Command
io.commandWith(opts: {
  program: String,
  args?: List<String>,
  cwd?: String,
  env?: { ...String },
  stdin?: String
}) -> Command
io.commandWithRedirects(command: Command, redirects: List<Redirect>) -> Command
io.pipeline(commands: List<Command>) -> Pipeline
io.pipelineWithRedirects(pipeline: Pipeline, redirects: List<Redirect>) -> Pipeline
io.redirectStdoutPath(path: Path) -> Redirect
io.redirectStderrPath(path: Path) -> Redirect
io.redirectStdinPath(path: Path) -> Redirect
io.taskCommand(command: Command) -> Task[ProcessResult]
io.taskPipeline(pipeline: Pipeline) -> Task[ProcessResult]
io.awaitTask(task: Task[ProcessResult]) -> ProcessResult
io.awaitTasks(tasks: List<Task[ProcessResult]>) -> List<ProcessResult>
io.execCommand(command: Command) -> ProcessResult
io.execPipeline(pipeline: Pipeline) -> ProcessResult
io.processSuccess(result: ProcessResult) -> Bool
io.processStdout(result: ProcessResult) -> String
io.processCode(result: ProcessResult) -> Int
io.processStderr(result: ProcessResult) -> String
io.homeDir() -> Option<String>
io.hashFile(path: String) -> String
io.hashFilePath(path: Path) -> String
io.hashString(content: String) -> String
io.currentSystem() -> String
io.spawn(task: Task[ProcessResult]) -> Int
io.poll(spawnId: Int) -> Option<ProcessResult>
io.cancel(spawnId: Int) -> Unit
io.awaitAny(tasks: List<Task[ProcessResult]>) -> ProcessResult
io.awaitTaskWithTimeout(task: Task[ProcessResult], ms: Int) -> Option<ProcessResult>
io.setRawMode(fd: Int, enable: Bool) -> Unit
io.resetTerminal(fd: Int) -> Unit

-- Stream<T> APIs (13 APIs)
io.streamList(list: List<T>) -> Stream<T>
io.streamLines(path: String) -> Stream<String>
io.streamCommand(cmd: Command) -> Stream<String>
io.streamBytes(path: String) -> Stream<Bytes>
io.streamMap(s: Stream<A>, f: A -> B) -> Stream<B>
io.streamFilter(s: Stream<T>, f: T -> Bool) -> Stream<T>
io.streamTake(s: Stream<T>, n: Int) -> Stream<T>
io.streamDrop(s: Stream<T>, n: Int) -> Stream<T>
io.streamCollect(s: Stream<T>) -> List<T>
io.streamPipe(s: Stream<String>, cmd: Command) -> ProcessResult
io.streamForEach(s: Stream<T>, f: T -> Unit) -> Unit
io.streamFold(s: Stream<T>, init: A, f: A -> T -> A) -> A
io.streamWithTimeout(s: Stream<T>, ms: Int) -> Stream<Option<T>>

-- Short I/O aliases (v4.0+): read, write, exec, cmd, run, sh, env, pwd, home, ok, stdout, stderr, code, ls, exists
-- 别名同时提供限定形式 / the aliases also exist in qualified form
io.read(path: String) -> String
io.write(path: String, content: String) -> Unit
io.run(cmd: Command) -> ProcessResult
io.shell(command: String) -> ProcessResult
io.env() -> Record

io.args() -> (List<String>, Record)
io.walk(path: Path) -> List<Path>
io.chmod(path: Path, mode: Int) -> Unit
io.chown(path: Path, uid: Int, gid: Int) -> Unit
io.copy(from: String, to: String) -> Unit
io.copyPath(from: Path, to: Path) -> Unit
io.move(from: String, to: String) -> Unit
io.movePath(from: Path, to: Path) -> Unit
io.readlink(path: Path) -> Path
io.symlink(target: Path, link: Path) -> Unit
io.atomicWrite(path: String, content: String) -> Unit
io.atomicWritePath(path: Path, content: String) -> Unit
io.atomicWriteAll(files: List<{ path: String, content: String }>) -> Unit
io.setEnv(name: String, value: String) -> Unit
io.unsetEnv(name: String) -> Unit
io.sleep(ms: Int) -> Unit
io.which(program: String) -> Option<String>
io.isTTY(fd: Int) -> Bool
io.terminalSize() -> Option<{ rows: Int, cols: Int }>
io.input(prompt: String) -> String
io.readKey(fd: Int) -> Int
io.readPassword(prompt: String) -> String
io.lines(path: String) -> List<String>
io.readFileLines(path: String, f: String -> Unit) -> Unit
io.readFileLinesPath(path: Path, f: String -> Unit) -> Unit
io.defer(cleanup: () -> Unit) -> Unit
io.onSignal(signal: String, handler: () -> Unit) -> Unit
io.jobs() -> List<{ id: Int, state: String }>
io.waitAnyJob() -> { id: Int, result: ProcessResult }
io.spawnWithTimeout(task: Task[ProcessResult], ms: Int) -> Int
io.execCommandLines(cmd: Command) -> List<String>
io.execCommandStreaming(cmd: Command, f: String -> Unit) -> ProcessResult
io.execCommandStreamingWithTimeout(cmd: Command, f: String -> Unit, ms: Int) -> Option<ProcessResult>
io.execPipelineStreaming(pipeline: Pipeline, f: String -> Unit) -> ProcessResult
io.execPipelineStreamingWithTimeout(pipeline: Pipeline, f: String -> Unit, ms: Int) -> Option<ProcessResult>

-- Glob 与事件/反应式
io.glob(pattern: String) -> List<Path>
io.every(ms: Int) -> Event<Int>
io.watchFile(path: String) -> Event<String>
io.eventNext(event: Event<A>) -> A
io.liveNext(live: Live<A>) -> A
```

## Bytes Module (std.bytes)

```n3v3
bytes.len(b: Bytes) -> Int
bytes.isEmpty(b: Bytes) -> Bool
bytes.fromString(s: String) -> Bytes
bytes.fromList(xs: List<Int>) -> Bytes
bytes.toList(b: Bytes) -> List<Int>
bytes.toString(b: Bytes) -> String
bytes.concat(a: Bytes, b: Bytes) -> Bytes
```




## Path Module (std.path) / 路径模块（std.path）


```n3v3
path.fromString(path: String) -> Path
path.joinPath(base: Path, child: String) -> Path
path.parentPath(path: Path) -> Option<Path>
path.filenamePath(path: Path) -> Option<String>
path.extensionPath(path: Path) -> Option<String>
path.isAbsolutePath(path: Path) -> Bool
path.join(a: String, b: String) -> String
path.parent(path: String) -> Option<String>
path.filename(path: String) -> Option<String>
path.extension(path: String) -> Option<String>
path.is_absolute(path: String) -> Bool
```





## Event / 事件

| Function | Signature | Effect |
|----------|-----------|--------|
| `io.every(ms)` | `Int -> Event<Int>` | effect |
| `io.watchFile(path)` | `String -> Event<String>` | effect |
| `io.eventNext(event)` | `Event<a> -> a` | effect |
| `io.eventMap(event, fn)` | `Event<a> -> (a -> b) -> Event<b>` | eval-owned |
| `io.eventFilter(event, fn)` | `Event<a> -> (a -> Bool) -> Event<a>` | eval-owned |

## Reactive / 反应式

| Function | Signature | Effect |
|----------|-----------|--------|
| `io.reactive(event)` | `Event<a> -> Live<a>` | effect |
| `io.liveNext(live)` | `Live<a> -> a` | effect |
| `io.liveCurrent(live)` | `Live<a> -> Option<a>` | effect |
| `io.liveCancel(live)` | `Live<a> -> ()` | effect |

## Temporal / 时序

| Function | Signature | Effect |
|----------|-----------|--------|
| `io.retry(fn, maxAttempts, backoffMs)` | `(() -> a) -> Int -> Int -> a` | effect |
| `io.ensure(check, timeoutMs, intervalMs)` | `(() -> Bool) -> Int -> Int -> Bool` | effect |

## Map / Set Namespaces (Map.*, Set.*) / Map / Set 命名空间（Map.*、Set.*）


```n3v3
Map.empty -> Map<K, V>
Map.singleton(key: K, value: V) -> Map<K, V>
Map.fromList(items: List<(K, V)>) -> Map<K, V>
Map.get(key: K, map: Map<K, V>) -> Option<V>
Map.getWithDefault(key: K, default: V, map: Map<K, V>) -> V
Map.contains(key: K, map: Map<K, V>) -> Bool
Map.size(map: Map<K, V>) -> Int
Map.isEmpty(map: Map<K, V>) -> Bool
Map.values(map: Map<K, V>) -> List<V>
Map.insert(key: K, value: V, map: Map<K, V>) -> Map<K, V>
Map.remove(key: K, map: Map<K, V>) -> Map<K, V>
Map.union(left: Map<K, V>, right: Map<K, V>) -> Map<K, V>
Map.intersection(left: Map<K, V>, right: Map<K, V>) -> Map<K, V>
Map.difference(left: Map<K, V>, right: Map<K, V>) -> Map<K, V>
Map.keys(map: Map<K, V>) -> List<String>
Map.toList(map: Map<K, V>) -> List<(String, V)>

-- 以下闭包成员已在 std 注册，但运行时返回
-- "requires closure evaluation support"，当前不可用：
-- Map.filter / Map.filterWithKey / Map.fold / Map.foldWithKey /
-- Map.map / Map.mapWithKey / Map.update
-- Set.filter / Set.fold / Set.map / Set.partition

Set.empty -> Set<A>
Set.singleton(value: A) -> Set<A>
Set.fromList(items: List<A>) -> Set<A>
Set.contains(value: A, set: Set<A>) -> Bool
Set.size(set: Set<A>) -> Int
Set.isEmpty(set: Set<A>) -> Bool
Set.insert(value: A, set: Set<A>) -> Set<A>
Set.remove(value: A, set: Set<A>) -> Set<A>
Set.union(left: Set<A>, right: Set<A>) -> Set<A>
Set.intersection(left: Set<A>, right: Set<A>) -> Set<A>
Set.difference(left: Set<A>, right: Set<A>) -> Set<A>
Set.symmetricDifference(left: Set<A>, right: Set<A>) -> Set<A>
Set.isSubset(left: Set<A>, right: Set<A>) -> Bool
Set.isSuperset(left: Set<A>, right: Set<A>) -> Bool
Set.isDisjoint(left: Set<A>, right: Set<A>) -> Bool
Set.toList(set: Set<A>) -> List<A>
```




## Package System (in progress) / 包管理（开发中）


```n3v3
derivation {
    name: String,
    system: String,
    builder: String,
    args: List<String>,      -- optional
    version: String,         -- optional (defaults to 0.0.0)
    ...                      -- other string fields become env vars
} -> Derivation
```

```n3v3
fetch.path(path: String) -> { path: String, hash: String, cached: Bool }
fetch.pathWithHash(path: String, hash: String) -> { path: String, hash: String, cached: Bool }
fetch.url(url: String) -> { path: String, hash: String, cached: Bool }
fetch.urlWithHash(url: String, hash: String) -> { path: String, hash: String, cached: Bool }
fetch.git(url: String, rev: String) -> { path: String, hash: String, cached: Bool }
fetch.gitWithHash(url: String, rev: String, hash: String) -> { path: String, hash: String, cached: Bool }
```

Note: Fetch helpers are impure and can access local filesystem/network. Prefer
`*WithHash` variants for reproducible builds.
注意：fetch 函数是带副作用的，可能访问本地文件系统或网络。为了可复现构建，优先使用带 `WithHash` 的版本。




## Example / 示例


```n3v3
use std.list (filter, map);
use std.string;

let users = [
    { name = "Alice", age = 30 },
    { name = "Bob", age = 25 },
];

let names = map(|u| u.name, filter(|u| u.age >= 18, users));

let joined = string.join(names, ", ");

-- => "Alice, Bob"
```

---

```n3v3
use std.list (filter, map);
use std.string;

let users = [
    { name = "小明", age = 30 },
    { name = "小红", age = 25 },
];

let names = map(|u| u.name, filter(|u| u.age >= 18, users));

let joined = string.join(names, "、");

-- => "小明、小红"
```

---

<div align="center">

```
═══════════════════════════════════════════════════════════════════════════════
                    Build something. Break something. Learn.
═══════════════════════════════════════════════════════════════════════════════
```

</div>
