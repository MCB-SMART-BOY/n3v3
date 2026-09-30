<div align="center">

<img src="assets/logo.svg" width="140" alt="n3v3 logo">

<h1>n3v3</h1>

<p>一门带类型系统的系统自动化语言 —— 配置、构建、监控、脚本，都在里面。</p>

<p>
  <a href="https://github.com/MCB-SMART-BOY/n3v3/actions/workflows/ci.yml">
    <img src="https://github.com/MCB-SMART-BOY/n3v3/actions/workflows/ci.yml/badge.svg" alt="CI">
  </a>
  <a href="https://github.com/MCB-SMART-BOY/n3v3/releases">
    <img src="https://img.shields.io/github/v/release/MCB-SMART-BOY/n3v3?color=blue">
  </a>
  <a href="LICENSE">
    <img src="https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg">
  </a>
</p>

</div>

---

## 一眼看懂

这是 Bash：

```bash
#!/bin/bash
set -euo pipefail
CONFIG="$1"
if [ ! -f "$CONFIG" ]; then echo "没有配置文件" >&2; exit 1; fi
PORT=$(grep port "$CONFIG" | cut -d= -f2 | tr -d ' ')
if [ -z "$PORT" ]; then PORT=8080; fi
for i in 1 2 3; do
    curl -sf "http://localhost:$PORT/health" && break
    sleep 2
done
```

这是 n3v3 v5.0.0（语法 v4.0）：

```n3v3-check
#!/usr/bin/env n3v3 run
use std.io = io

type Option<T> = | Some(T) | None
config = { port = Some(8080) }
port = config.port ?? 8080

ready = io.pathExistsPath(./health-check)
io.retry(fn() { ready }, 3, 2000);
```

类型系统在编译期告诉你 `config.port` 对不对、`??` 的默认值类型匹不匹配。不需要等到半夜告警响了才发现变量名拼错了。

---

## 类型系统

不只是"有类型"，而是编译期就能拦住一大类错误。

```n3v3-check
use std.io = io
use std.option = option

& 代数数据类型。编译期检查穷尽性——漏分支直接报错
type Health = | Alive | Dead(Int, String)

summarize(h: Health) -> String = match h {
    Alive -> "ok",
    Dead(code, stderr) -> `退出码 {code}：{stderr}`,
}

& 错误传播。? 碰到 None 或 Err 就短路返回
loadAndParse(path: Path) -> String = option.some(io.readFilePath(path))?

& 安全字段访问。?. 碰到 None 就返回 None
server = Some({ port = Some(8080) })
port = (server?.port ?? Some(8080)) ?? 8080

& 惰性求值。只在第一次 force 的时候计算
expensive = ~42
& ……中间可能根本不用 ……
value = force(expensive)

& trait 和泛型
trait HealthCheck { fn check(self) -> Health };
impl HealthCheck for Int {
    fn check(self) -> Health = if self == 1 -> Alive() else Dead(1, "不健康");
};
```

`print` 和 `println` 全局可用，不需要 import。字符串插值 `` `你好 {name}` ``。路径字面量 `./foo` 直接就是 `Path` 类型，不是字符串。支持记录更新 `{ old | field = newValue }`，以及 `config & override` 记录合并。

---

## 系统能力

**一等管道**。命令之间用 `|>` 串联：

```n3v3-check
use std.io = io
pipeline = io.command("ls", ["-la"]) |> io.command("grep", ["n3v3"])
result = io.execPipeline(pipeline)
output = io.processStdout(result)
```

**流式处理**。逐行处理命令输出，带超时自动杀进程：

```n3v3-check
use std.io = io
use std.string = str

runStreaming() = io.execCommandStreamingWithTimeout(
    io.command("journalctl", ["-f"]),
    |line| {
        if str.contains(line, "error") -> {
            io.appendFilePath(./errors.log, line)
        } else {
            ()
        }
    },
    5000,
)

& Stream<T> 变换管道
lines = io.streamLines("./log.txt")
mapped = io.streamMap(lines, |line| str.upper(line))
filtered = io.streamFilter(mapped, |line| str.contains(line, "ERROR"))
limited = io.streamTake(filtered, 10)
results = io.streamCollect(limited);

& Stream<T> 管道到命令
io.streamPipe(io.streamList(["line1", "line2"]), io.command("grep", ["line"]));
```

**原子写**。先写临时文件再 rename，不会写出写到一半断电损坏的文件：

```n3v3-check
use std.io = io
io.atomicWritePath(./critical.json, "{}");
```

**信号处理**。注册操作系统信号回调：

```n3v3-check
use std.io = io
registerSignals() = {
    io.onSignal("TERM", fn() { () });
    io.onSignal("INT", fn() { () });
}
```

**重试和条件等待**：

```n3v3-check
use std.io = io
ready = io.pathExistsPath(./health-check);
io.retry(fn() { ready }, 5, 1000);
io.ensure(fn() { ready }, 30000, 500);
```

**二进制数据**。`Bytes` 是一等类型：

```n3v3-check
use std.io = io
data = io.readFileBytesPath(./binary.bin);
io.writeFileBytesPath(./copy.bin, data);
```

**文件操作**。增删改查、遍历、权限、符号链接 —— 全有 typed-path 变体：

```n3v3-check
use std.io = io
io.writeFilePath(./a.txt, "内容");
content = io.readFilePath(./a.txt);
io.appendFilePath(./a.txt, "追加");
io.copyPath(./a.txt, ./b.txt);
io.movePath(./b.txt, ./c.txt);
io.createDirAllPath(./dir/sub);
paths = io.walk(./dir);
io.chmod(./script, 0o755);
io.symlink(./target, ./link);
tempPath = io.tempDir(|dir| dir);
```

---

## 效果系统

```n3v3-check
use std.io = io
& 纯函数。不能调 IO，编译器强制检查
add(x: Int, y: Int) -> Int = x + y

& effect 由编译器自动推断，无需手动标注
save(path: Path, data: String) = io.writeFilePath(path, data)

& n3v3 check 默认检查 effect；n3v3 check --allow-effects 允许 effectful 函数调用
```

---

## 工具链

一个二进制文件，自带全套工具：

```bash
n3v3 run foo.n3v3     # 执行脚本（支持 shebang）
n3v3 eval "1 + 2"     # 求值表达式
n3v3 test ./tests     # 运行目录中的测试（发现 *_test.n3v3 与 test/*.n3v3）
n3v3 check foo.n3v3   # 类型检查（默认检查 effect）
n3v3 fmt file         # 代码格式化
n3v3 repl             # 交互式 REPL（历史持久化、Tab 补全、:type、:save、:cd）
n3v3 doc spec         # 内置文档（17 个主题）
n3v3 explain E0200    # 错误码扩展说明
n3v3 init ./proj      # 初始化项目
n3v3 setup helix      # 生成编辑器集成
n3v3 lsp              # Language Server
n3v3 info             # 包与平台信息
n3v3 version          # 版本信息

# 以下命令标注 "Unix only"，仅 Unix 可用
n3v3 search <query>   # 搜索包索引
n3v3 build <pkg>      # 构建包
n3v3 update           # 更新依赖
n3v3 package install <pkg>   # 安装包（另有 package remove / list / rollback）
n3v3 config build     # 构建系统配置（另有 config switch / switch-to / rollback / list / verify）
n3v3 store gc         # 垃圾回收（另有 store info）
n3v3 registry-update  # 更新 registry 索引
n3v3 registry-serve   # 启动本地 registry 服务
n3v3 registry-publish # 发布包到 registry
```

---

## 形式化验证

不是说"我们觉得没问题"。`formal/` 目录里有 21 个 Lean 4 模块，核心语义做了机器检查的证明：34 条 EffectEval 规则（v4.3）覆盖了全部 I/O 路径（含 Stream<T> Phase C 5 条规则），全部二元运算符有类型安全证明（含除零规则），管道安全、环境注入防护、缓冲区大小限制共 5 项安全审计全部机器验证。`cd formal && lake build` 一把过。

[语言规范](docs/reference/spec.md) · [功能矩阵](docs/project/feature-matrix.md) · [路线图](.claude/forward-plan.md) · [更新日志](docs/project/changelog.md) · [贡献指南](docs/contributor/contributing.md)

---

**Implemented: Shell 能力替代** ✅ — Stream<T> 13 APIs、E2E 558 测试、Formatter 幂等性 37/37、Clippy 0 warnings。

**Implemented: 生态补完** ✅ — flake/lock 系统、content-addressed store、registry CLI（`registry-update` / `registry-serve` / `registry-publish`）、稳定性分级（Tier 1/2/3）。

示例脚本：`examples/test-runner.n3v3`（测试运行器）、`examples/ci-bootstrap.n3v3`（CI 启动脚本）、`examples/file-watcher.n3v3`（文件监控）、`examples/system-config.n3v3`（系统配置）。

---

MPL-2.0

## 装一个

```bash
curl -fsSL https://raw.githubusercontent.com/MCB-SMART-BOY/n3v3/master/scripts/install.sh | sh
```

Arch 用 AUR 的 `n3v3-bin`：`paru -S n3v3-bin`。源码编译：`cargo install --path n3v3-cli --locked`。
