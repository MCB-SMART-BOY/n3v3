<div align="center">

<img src="../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>Examples / 示例</h1>

<p><em>Representative runnable samples for n3v3 — also serves as teaching material.</em></p>

<p>
  <strong><a href="../README.md">Home</a></strong> ·
  <strong><a href="../docs/">Docs</a></strong>
</p>

</div>

---

## Basics / 基础

| File | Content |
|------|---------|
| [`basics/arithmetic.n3v3`](basics/arithmetic.n3v3) | Integer arithmetic, operator precedence, negative numbers |
| [`basics/booleans.n3v3`](basics/booleans.n3v3) | Boolean logic, equality, short-circuit evaluation |
| [`basics/variables.n3v3`](basics/variables.n3v3) | Let bindings, shadowing, nested scopes |

## Functions / 函数

| File | Content |
|------|---------|
| [`functions/lambda.n3v3`](functions/lambda.n3v3) | Lambda expressions, higher-order functions, closures |
| [`functions/pipe.n3v3`](functions/pipe.n3v3) | Pipe operator `\|>`, function chaining |

## Control Flow / 控制流

| File | Content |
|------|---------|
| [`control-flow/match.n3v3`](control-flow/match.n3v3) | Pattern matching with `match`, wildcard and binding patterns |

## Data / 数据结构

| File | Content |
|------|---------|
| [`data/records.n3v3`](data/records.n3v3) | Record creation, field access, nested records |
| [`data/lists.n3v3`](data/lists.n3v3) | List literals, map/filter/length, concatenation |

## I/O / 输入输出

| File | Content |
|------|---------|
| [`io/files.n3v3`](io/files.n3v3) | File read/write/append, directory operations |
| [`io/process.n3v3`](io/process.n3v3) | Process execution, pipelines, stdin, exit codes |

`io/files.n3v3` assumes a Unix-like system with a writable `/tmp` directory. It overwrites `/tmp/n3v3-example.txt` and writes under `/tmp/n3v3-demo/`; run it only if those paths are safe to use.
`io/files.n3v3` 需要类 Unix 系统和可写的 `/tmp` 目录。它会覆盖 `/tmp/n3v3-example.txt`，并在 `/tmp/n3v3-demo/` 下写文件；确认这些路径可以安全使用后再运行。

## Learning Path / 学习路径

Run the lessons from the repository root with `n3v3 run examples/learning/<file>.n3v3`.
在仓库根目录使用 `n3v3 run examples/learning/<file>.n3v3` 运行课程示例。

| File / 文件 | Content / 内容 |
|-------------|----------------|
| [`learning/01_basics.n3v3`](learning/01_basics.n3v3) | Values, types, records and comprehensions / 值、类型、记录和列表推导 |
| [`learning/02_pattern_matching.n3v3`](learning/02_pattern_matching.n3v3) | Exhaustive patterns / 穷尽模式匹配 |
| [`learning/03_error_handling.n3v3`](learning/03_error_handling.n3v3) | Result variants / Result 变体 |
| [`learning/04_modules.n3v3`](learning/04_modules.n3v3) | Standard `std.list` import and alias; no platform files / 标准 `std.list` 导入与别名；无平台文件依赖 |
| [`learning/05_functional.n3v3`](learning/05_functional.n3v3) | Higher-order list functions and recursive sum / 高阶列表函数与递归求和 |
| [`learning/06_script.n3v3`](learning/06_script.n3v3) | CLI args and filesystem glob (effectful) / CLI 参数与文件匹配（有副作用） |
| [`learning/07_advanced.n3v3`](learning/07_advanced.n3v3) | Task records and summary / 任务记录与汇总 |
| [`learning/08_space_supplies.n3v3`](learning/08_space_supplies.n3v3) | Pure, self-contained space-supplies scoring; [tutorial](../docs/user/tutorial.md#7-mini-project-space-supplies--趣味小项目太空补给计分) / 纯函数、自包含的太空补给计分；见教程 |

## Running / 运行

```bash
n3v3 run examples/basics/arithmetic.n3v3
n3v3 run examples/functions/lambda.n3v3
n3v3 run examples/control-flow/match.n3v3
n3v3 run examples/data/records.n3v3
n3v3 run examples/io/files.n3v3
```

## Bootstrap

```bash
n3v3 run examples/ci-bootstrap.n3v3
```

`examples/ci-bootstrap.n3v3` is the only bootstrap example in the tree: it
drives the project's own CI steps (format, clippy, build, test) from n3v3.
The earlier toolchain package files (`examples/bootstrap/*.n3v3`) were removed;
the bootstrap chain below is design intent, not shipped code.
`examples/ci-bootstrap.n3v3` 是仓库中唯一的 bootstrap 示例：用 n3v3 驱动项目自身的
CI 步骤（format、clippy、build、test）。早期的工具链包文件（`examples/bootstrap/*.n3v3`）
已移除；下文的自举顺序属于设计意图，不是已交付的代码。

## One-shot discovery demos / 单次发现演示

`file-watcher.n3v3` performs one glob and reports the current file count; it
does not keep watching. `test-runner.n3v3` discovers and lists test files; it
does not execute them. Use `n3v3 test <dir>` for the shipped test runner.

`file-watcher.n3v3` 只执行一次 glob 并报告当前文件数量，不会持续监控；
`test-runner.n3v3` 只发现并列出测试文件，不执行测试。项目提供的测试运行器入口是
`n3v3 test <dir>`。
