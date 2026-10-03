<div align="center">

<img src="../../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>n3v3 Language Specification — v4 Grammar Syntax</h1>

<p><em>n3v3 language specification — v4 grammar syntax, product v5.0.2 (pending release) · 语言规范：v4 语法形态，产品 v5.0.2（待发布）— 含形式化语义 (Lean-verified)</em></p>

<p>
  <strong><a href="../../README.md">Home</a></strong> ·
  <strong><a href="../README.md">Docs</a></strong>
</p>

</div>

---

> *The formal spec. For when you need the precise truth.*  
> 语言规范：当你需要严谨的定义时使用。

## Version Context / 版本语境

This specification documents the v4 grammar syntax for product v5.0.2 (pending release). The product version and grammar version are distinct: v5.0.2 retains the v4 surface syntax shipped in v5.0.1; the AST-to-HIR pipeline became canonical in v5.0.0.
本规范记录产品 v5.0.2（待发布）使用的 v4 语法形态。产品版本与语法版本不同：v5.0.2 保留 v5.0.1 已发布的 v4 表层语法；AST 到 HIR 的流程自 v5.0.0 起作为唯一规范路径。

### v5.0 AST/HIR cutover / v5.0 AST/HIR 切换

`n3v3-frontend` parses source into an AST, lowers it to HIR, and type-checks the HIR; `n3v3-eval::Evaluator` evaluates HIR modules. The former `n3v3_eval::compat` AST evaluator path was removed in v5.0.0; `n3v3-eval` now exposes the HIR evaluator only (`crates/n3v3-frontend/src/lib.rs:274-315`, `crates/n3v3-eval/src/lib.rs:1-14`).
`n3v3-frontend` 将源代码解析为 AST、降级为 HIR 并检查 HIR；`n3v3-eval::Evaluator` 对 HIR 模块求值。旧的 `n3v3_eval::compat` AST 求值路径已在 v5.0.0 删除；`n3v3-eval` 现在只公开 HIR 求值器（`crates/n3v3-frontend/src/lib.rs:274-315`、`crates/n3v3-eval/src/lib.rs:1-14`）。

Migration: consumers that called `AstEnv`, `AstEvaluator`, or `n3v3_eval::compat` must switch to `n3v3-frontend::analyze_source`/`analyze_ast`, then pass the resulting HIR module and method-resolution table to `n3v3-eval::Evaluator::eval_evaluable_module` (or `eval_module_with_method_resolutions`). Public syntax enums remain inspection boundaries; wildcard-match them to remain forward-compatible.
迁移：曾调用 `AstEnv`、`AstEvaluator` 或 `n3v3_eval::compat` 的消费者必须改用 `n3v3-frontend::analyze_source`/`analyze_ast`，再将得到的 HIR 模块与 method-resolution table 传给 `n3v3-eval::Evaluator::eval_evaluable_module`（或 `eval_module_with_method_resolutions`）。公共 syntax enum 仍是检查边界；请使用 wildcard match 以保持前向兼容。

## 1. Design Principles / 设计原则


- **Zero Ambiguity**: Every construct parses exactly one way
- **Syntactic Consistency**: Similar things look similar
- **Indentation Independent**: Explicit delimiters, no significant whitespace
- **Pure by default**: Referential transparency is preferred; host side effects are explicit and traceable through effect inference and checking.


- **默认纯函数式**：优先保持引用透明；主机副作用通过 effect 推导与检查显式可追踪。


## 2. Symbol Reference / 符号速查


| Symbol | Purpose | Example |
|--------|---------|---------|
| `( )` | Grouping, tuples, function args | `(1, 2)`, `f(x)` |
| `[ ]` | Lists | `[1, 2, 3]` |
| `{ }` | Records, blocks | `{ x = 1 }`, `{ let x = 1; x }` |
| `< >` | Generic parameters | `List<Int>` |
| `->` | Function types, match arms | `Int -> Int` |
| `,` | Item separator | `[1, 2, 3]` |
| `;` | Statement terminator (optional at top level) | `let x = 1` |
| `:` | Type annotation | `x: Int` |
| `=` | Value binding | `x = 1` |
| `&` | Record merge, comments | `a & b`, `& comment` |
| `|>` | Pipe | `x |> f` |


| 符号 | 干啥的 | 例子 |
|------|--------|------|
| `( )` | 分组、元组、函数参数 | `(1, 2)`, `f(x)` |
| `[ ]` | 列表 | `[1, 2, 3]` |
| `{ }` | 记录、代码块 | `{ x = 1 }`, `{ let x = 1; x }` |
| `< >` | 泛型参数 | `List<Int>` |
| `->` | 函数类型、匹配分支 | `Int -> Int` |
| `,` | 分隔并列项 | `[1, 2, 3]` |
| `;` | 语句结尾（顶层可选） | `let x = 1` |
| `:` | 类型声明 | `x: Int` |
| `=` | 绑定值 | `x = 1` |
| `&` | 记录合并、注释 | `a & b`, `& comment` |
| `|>` | 管道 | `x |> f` |


## 3. Lexical Elements / 词法元素


### Comments / 注释

```n3v3
-- single line comment --

--
   multi-line comment
   -- can be nested --
--

& line comment (v4.0)
```

### Literals

```n3v3
-- Integers
42  -17  0xFF  0o77  0b1010  1_000_000

-- Floats
3.14  -2.5  1.0e-5

-- Booleans
true  false

-- Characters
'a'  '\n'  '\u{1F600}'

-- Strings
"hello\nworld"

-- Interpolated strings
`hello {name}`

-- Path literals (first-class `Path` values)
./relative  ../parent  /absolute
```

**Identifiers** start with an ASCII letter or `_`; continuation characters may
be any Unicode alphanumeric (so `café` is one identifier). A non-ASCII character
in first position is a lexical error (`E0001`).

**`/` depends on context.** After a token that can end an operand (`Int`,
`Float`, `String`, `Char`, `Bool`, path literal, interpolated-string end,
identifier, `self`, `?`, `)`, `]`, `}`) `/` is division; otherwise it starts an
absolute path literal. `6/2` is division, `/etc/hosts` is a path literal. A path
literal requires the next character to be alphanumeric or `_`, `-`, `.`.


### 注释

```n3v3
-- 单行注释 --

--
   多行注释
   -- 可以嵌套 --
--
```

### 字面量

```n3v3
-- 整数
42  -17  0xFF  0o77  0b1010  1_000_000

-- 浮点数
3.14  -2.5  1.0e-5

-- 布尔值
true  false

-- 字符
'a'  '\n'  '\u{1F600}'

-- 字符串
"hello\nworld"

-- 插值字符串
`你好 {name}`

-- 路径字面量（目前就是字符串）
./relative  ../parent  /absolute
```

**标识符**首字符为 ASCII 字母或 `_`；后续字符可以是任意 Unicode 字母数字（因此 `café` 是一个标识符）。首字符出现非 ASCII 字符是词法错误（`E0001`）。

**`/` 的含义取决于上下文。** 当上一个 token 可以结束操作数（`Int`、`Float`、`String`、`Char`、`Bool`、路径字面量、插值字符串结束、标识符、`self`、`?`、`)`、`]`、`}`）时，`/` 是除法；否则是绝对路径字面量的开始。因此 `6/2` 是除法，`/etc/hosts` 是路径字面量。路径字面量要求后一个字符是字母数字或 `_`、`-`、`.`。


## 4. Types / 类型


### Primitive Types

```n3v3
Int     -- arbitrary precision integer
Float   -- 64-bit floating point
Bool    -- boolean
Char    -- Unicode character
String  -- UTF-8 string
Unit    -- empty type ()
```

Path literals (`./config`, `/etc/hosts`) are first-class `Path` values.
`io.readFilePath(./x)` and typed-path adapters are available.

### Compound Types

```n3v3
List<Int>                       -- list
Option<Int>                     -- optional value
Result<Int, String>             -- result with error
(Int, String)                   -- tuple
(Int, Int) -> Int               -- function (single tuple param)
Int -> Int -> Int               -- curried function
{ name: String, port: Int }    -- record type

-- Runtime object types
Command                         -- process command
Pipeline                        -- command pipeline
ProcessResult                   -- process execution result
Task<T>                         -- deferred task
Bytes                           -- binary data
```


### 原始类型

```n3v3
Int     -- 任意精度整数
Float   -- 64 位浮点
Bool    -- 布尔
Char    -- Unicode 字符
String  -- UTF-8 字符串
Unit    -- 空类型 ()
```

路径字面量（`./config`、`/etc/hosts`）是一等 `Path` 值。
`io.readFilePath(./x)` 和 typed-path adapter 均已可用。

### 复合类型

```n3v3
List<Int>                       -- 列表
Option<Int>                     -- 可选值
Result<Int, String>             -- 带错误的结果
(Int, String)                   -- 元组
(Int, Int) -> Int               -- 函数（单参数元组）
Int -> Int -> Int               -- 柯里化函数
{ name: String, port: Int }    -- 记录类型

-- 运行时对象类型
Command                         -- 进程命令
Pipeline                        -- 命令管道
ProcessResult                   -- 进程执行结果
Task<T>                         -- 延迟任务
Bytes                           -- 二进制数据
```


## 5. Definitions / 定义


Top-level `let`/`fn`/`;` are **optional** in v4.0. `struct`/`enum` → `type`.

```n3v3
-- Type alias
type Port = Int

-- Type (struct/enum unified)
type Point = { x: Float, y: Float }
type Option<T> = | Some(T) | None

-- Trait
trait Show {
    show(self) -> String
}

-- Implementation
impl Show for Point {
    show(self) = `({self.x}, {self.y})`
}

-- Function (let/fn optional)
add(x: Int, y: Int) -> Int = x + y

-- Or with explicit keywords
let add = |x: Int, y: Int| x + y;

-- Effectful function (effect auto-inferred)
readConfig(path: String) -> String = io.readFile(path)
```

**Zero-parameter items are value bindings.** `fn f() = 1` (like `f() = 1` and
`f = 1`) binds `f` to the value `1` of type `Int`; it is not a callable of type
`Fn([], Int)`. `f + 1` evaluates to `2`, while `f()` is a type error (`E0200`,
`Int` vs `Fn`). Functions with at least one parameter keep their function type.


顶层 `let`/`fn`/`;` 在 v4.0 中是**可选的**。`struct`/`enum` → `type`。

```n3v3
-- 类型别名
type Port = Int

-- 类型（struct/enum 统一为 type）
type Point = { x: Float, y: Float }
type Option<T> = | Some(T) | None

-- Trait
trait Show {
    show(self) -> String
}

-- 实现
impl Show for Point {
    show(self) = `({self.x}, {self.y})`
}

-- 函数（let/fn 可选）
add(x: Int, y: Int) -> Int = x + y

-- 或者用显式关键字
let add = |x: Int, y: Int| x + y;

-- 副作用函数（effect 自动推导）
readConfig(path: String) -> String = io.readFile(path)
```

**零参数项是值绑定。** `fn f() = 1`（与 `f() = 1`、`f = 1` 相同）把 `f` 绑定到类型为 `Int` 的值 `1`，而不是类型为 `Fn([], Int)` 的可调用对象。`f + 1` 求值为 `2`，`f()` 是类型错误（`E0200`，`Int` 与 `Fn` 不匹配）。至少带一个参数的函数保持函数类型。


## 6. Expressions / 表达式


### Bindings

```n3v3
let x = 42;
let (a, b) = (1, 2);
let { x, y } = point;
```

### Records (v4.0)

```n3v3
{ x = 0, y = 0 }
{ name }              -- shorthand
{ point | x = 10 }    -- update
config & override     -- merge
```

### Lists

```n3v3
[1, 2, 3]
[1, 2] ++ [3, 4]
[x * 2 | x <- xs, x > 0]    -- comprehension
```

### Closures / Lambdas (v4.0)

```n3v3
|x| x + 1
|x, y| x + y
|x: Int| -> Int { x + 1 }
```

### Conditionals

```n3v3
if x > 0 -> "positive" else "non-positive"
```

`if` is an expression. The `else` branch is required, and both branches must have the same type. Omitting `else` is a syntax error (`E0100`, expected `Else`), not a type error. `{}` is an empty `Record`, not `Unit`, so an empty branch must use `()`:

```n3v3
if cond -> { io.println("warn") } else ();
```

`if` 是表达式。`else` 分支必填，且两分支类型必须一致。省略 `else` 是语法错误（`E0100`，expected `Else`），不是类型错误。`{}` 是空 `Record` 而不是 `Unit`，因此空分支必须写成 `()`：

```n3v3
if cond -> { io.println("warn") } else ();
```

### Pattern Matching (Must Be Exhaustive)

`match` requires exhaustive coverage. For non-exhaustive conditions, use `if-else`; `if` itself always requires both branches and matching branch types.

```n3v3
-- match: exhaustive (compiler enforces)
match x {
    0 -> "zero",
    1 -> "one",
    _ -> "other",
}

-- if-else: non-exhaustive (no compiler guarantee)
if x > 0 -> "positive"
else if x < 0 -> "negative"
else "zero"
```

### Error Handling

```n3v3
let data = fetch(url)?;     -- propagate error
let x = maybe ?? default;   -- default value
user?.profile?.name         -- safe access
```

### Method Calls

```n3v3
x.foo(y)
```

Current canonical dispatch order:

1. Try inherent/trait method resolution on the receiver `x`.
2. If a matching method exists, use that method.
3. Otherwise fall back to the lowered callable-target form `foo(x, y)`.
4. If no method exists and no callable fallback target `foo` exists either, type checking reports a dedicated missing-method diagnostic at the call site.
5. If a callable fallback target exists, its ordinary call diagnostics still apply.


### 绑定

```n3v3
let x = 42;
let (a, b) = (1, 2);
let { x, y } = point;
```

### 记录

```n3v3
{ x = 0, y = 0 }
{ name }              -- 简写
{ point | x = 10 }    -- 更新
config & override     -- 合并
```

### 列表

```n3v3
[1, 2, 3]
[1, 2] ++ [3, 4]
[x * 2 | x <- xs, x > 0]    -- 推导式
```

### 闭包 / Lambdas (v4.0)

```n3v3
|x| x + 1
|x, y| x + y
|x: Int| -> Int { x + 1 }
```

### 条件

```n3v3
if x > 0 -> "正数" else "非正数"
```

`if` 是表达式。`else` 分支必填，且两分支类型必须一致。省略 `else` 是语法错误（`E0100`，expected `Else`），不是类型错误。`{}` 是空 `Record` 而不是 `Unit`，因此空分支必须写成 `()`：

```n3v3
if cond -> { io.println("warn") } else ();
```

### 模式匹配（必须穷尽）

`match` 要求穷尽覆盖。非穷尽场景使用 `if-else`；`if` 本身始终要求两条分支且分支类型一致。

```n3v3
-- match：穷尽（编译器强制）
match x {
    0 -> "零",
    1 -> "一",
    _ -> "其他",
}

-- if-else：非穷尽（不强制覆盖所有情况）
if x > 0 -> "正数"
else if x < 0 -> "负数"
else "零"
```

### 错误处理

```n3v3
let data = fetch(url)?;     -- 传播错误
let x = maybe ?? default;   -- 默认值
user?.profile?.name         -- 安全访问
```

### 方法调用

```n3v3
x.foo(y)
```

当前规范的派发顺序是：

1. 先在接收者 `x` 上尝试固有方法 / trait 方法解析。
2. 如果找到匹配方法，就走该方法。
3. 如果没有找到，再回退到降级后的可调用目标形式 `foo(x, y)`。
4. 如果两条分支都不成立，则类型检查失败。


## 7. Operators / 操作符


| Operator | Meaning |
|----------|---------|
| `+ - * / %` | Arithmetic |
| `^` | Power |
| `== !=` | Equality |
| `< <= > >=` | Comparison |
| `&& \|\|` | Logical and/or |
| `!` | Logical not |
| `++` | Concatenation |
| `&` | Record merge |
| `??` | Default value |
| `?.` | Safe access |
| `\|>` | Pipe |
| `?` | Error propagation |

### Precedence (high to low)

1. `.` `?.` `()` `[]`
2. `?` (postfix)
3. `!` `-` (prefix)
4. `^`
5. `* / %`
6. `+ -`
7. `++`
8. `< <= > >= == !=`
9. `&&`
10. `||`
11. `??`
12. `|>`
13. `&`


| 操作符 | 意思 |
|--------|------|
| `+ - * / %` | 算术运算 |
| `^` | 幂运算 |
| `== !=` | 相等判断 |
| `< <= > >=` | 比较大小 |
| `&& \|\|` | 逻辑与/或 |
| `!` | 逻辑非 |
| `++` | 拼接 |
| `&` | 记录合并 |
| `??` | 默认值 |
| `?.` | 安全访问 |
| `\|>` | 管道 |
| `?` | 错误传播 |

### 优先级（从高到低）

1. `.` `?.` `()` `[]`
2. `?` (后缀)
3. `!` `-` (前缀)
4. `^`
5. `* / %`
6. `+ -`
7. `++`
8. `< <= > >= == !=`
9. `&&`
10. `||`
11. `??`
12. `|>`
13. `&`


## 8. Modules / 模块


```n3v3
add(x: Int, y: Int) -> Int = x + y

use std.list
use std.list (map, filter)
use std.list = L
use self.utils
use super.common
use crate.utils
```


```n3v3
add(x: Int, y: Int) -> Int = x + y

use std.list
use std.list (map, filter)
use std.list = L
use self.utils
use super.common
use crate.utils
```


## 9. Lazy Evaluation / 惰性求值


```n3v3
let expensive = ~compute();
let result = force(expensive);
```

## 10. Effect System / 副作用系统

n3v3 distinguishes pure functions from effectful ones at compile time.
n3v3 在编译期区分纯函数和副作用函数。

### Effect Annotation / 副作用注解

Effectfulness is auto-inferred by the type checker. The `effect` keyword is accepted for backward compatibility but is not required.
副作用性由类型检查器自动推断。`effect` 关键字仅为向后兼容而接受，并非必需。

```n3v3
-- Pure function: no effects allowed / 纯函数：不允许副作用
fn pureAdd(x: Int, y: Int) -> Int = x + y;

-- Effectful function: may call io.*, fetch.* / 副作用函数：可调用 io.*, fetch.*
fn readHostname() -> String = io.readFile("/etc/hostname");
```

### Effect Checking / 副作用检查

The type checker infers effectfulness from effectful builtins and transitively from called functions; an explicit `effect` keyword remains accepted for compatibility but is not required. Effectful functions therefore remain expressible—the system makes their host effects visible instead of claiming that all code is side-effect free.
类型检查器根据有副作用的内置函数，并沿调用关系传递地推导函数的 effect；显式 `effect` 关键字仍为兼容性保留但不是必需的。因此有副作用的函数仍可表达；系统让主机副作用可见，而不是声称所有代码都没有副作用。

`n3v3 check` enables a CLI effect gate by default: it rejects collected effectful calls and exits with `effect check failed`. `n3v3 check --allow-effects` disables only that gate and still performs parsing and type checking. Warnings do not make a successful check fail; the success output is `[OK] OK - No errors found` (or includes the warning count).
`n3v3 check` 默认启用 CLI effect gate：拒绝收集到的副作用调用并以 `effect check failed` 退出。`n3v3 check --allow-effects` 只关闭该 gate，仍执行解析与类型检查。警告不会让成功检查失败；成功输出为 `[OK] OK - No errors found`（或附带警告数量）。

- Inspector functions (`processSuccess`, `processStdout`, `processCode`, `processStderr`) are pure.
- 检查器函数（`processSuccess`、`processStdout`、`processCode`、`processStderr`）是纯函数。

### Effectful Builtins / 副作用内置函数

The effect registry classifies host-facing `std.io` and `std.fetch` operations as effectful; process inspectors and pure constructors remain pure.
effect registry 将面向主机的 `std.io` 与 `std.fetch` 操作分类为有副作用；Process inspector 与纯构造器仍是纯函数。

| Module | Functions | Effect |
|--------|-----------|--------|
| `std.io` | `readFile`, `writeFile`, `execCommand`, `execPipeline`, `getEnv`, ... | I/O, Process |
| `std.io` | `processSuccess`, `processStdout`, `processCode`, `processStderr` | Pure (inspectors) |
| `std.fetch` | `url`, `git`, `path` | Network, Filesystem |
| `std.list` | All functions | Pure |
| `std.path` | All functions | Pure |

## 11. Scripting / 脚本

n3v3 supports shebang-based scripting with command-line argument access.
n3v3 支持基于 shebang 的脚本编写和命令行参数访问。

```n3v3
#!/usr/bin/env n3v3
use std.io = io;

-- Access script arguments / 访问脚本参数
let args = io.args();

-- Run a command with timeout / 带超时运行命令
let cmd = io.command("echo", ["hello"]);
let task = io.taskCommand(cmd);
let result = io.awaitTaskWithTimeout(task, 5000);
```

### Shebang Support / Shebang 支持

- `.n3v3` files starting with `#!/usr/bin/env n3v3` can be executed directly
- 以 `#!/usr/bin/env n3v3` 开头的 `.n3v3` 文件可直接执行
- The shebang line is automatically stripped before parsing
- Shebang 行在解析前自动去除
- Remaining CLI arguments are available via `io.args() -> (List<String>, Record)`
- 剩余 CLI 参数可通过 `io.args() -> (List<String>, Record)` 获取（第一个元素是位置参数列表，第二个是解析后的 flag 记录）

---

## 12. Implementation Status / 实现状态

The v4 grammar compatibility items covered by this specification are `Implemented` in v5.0.0. Lexer, parser, lowering, type checking, and HIR evaluation use the canonical pipeline; shebang stripping is an `Implemented` CLI concern rather than a parser feature.
本规范涉及的 v4 语法兼容项在 v5.0.0 中均为 `Implemented`。词法、解析、降级、类型检查与 HIR 求值使用规范流水线；shebang 去除属于 CLI 的 `Implemented` 行为，而不是 parser 特性。

Migration status is recorded in the version-context section above; completed items are not kept in a gap table.
迁移状态记录在上面的版本语境章节；已完成事项不再保留在差距表中。

---

## Appendix A: Keywords / 附录 A: 关键字


```
let fn type trait impl
use self
if else match
true false
```

**12 keywords** (v4.0 canonical; the parser retains 10 legacy spellings)

> Legacy spellings (`struct`, `enum`, `import`, `pub`, `as`, `then`, `lazy`,
> `effect`, `super`, `crate`) remain accepted for source compatibility. The
> lexer emits dedicated tokens for `struct`, `enum`, `super`, and `crate`;
> the other six are contextual parser identifiers.


```
let fn type trait impl
use self
if else match
true false
```

**12 个规范关键字**（v4.0；解析器保留 10 个旧拼写用于源码兼容）


## Appendix B: Nix Comparison / 附录 B: 跟 Nix 对照


| Nix | n3v3 v4.0 |
|-----|-----------|
| `{ a = 1; }` | `{ a = 1 }` |
| `[ 1 2 3 ]` | `[1, 2, 3]` |
| `x: x + 1` | `\|x\| x + 1` |
| `a // b` | `a & b` |
| `"${x}"` | `` `{x}` `` |
| `inherit x;` | `{ x }` |
| `rec { }` | Automatic recursion |
| `with pkgs; [foo]` | `use pkgs = p; [p.foo]` |
| `if cond then a else b` | `if cond -> a else b` |

---<div align="center">

```
═══════════════════════════════════════════════════════════════════════════════
                    The spec is the contract. Read it well.
═══════════════════════════════════════════════════════════════════════════════
```

</div>


---

# Part II: Formal Semantics / 第二部分：形式化语义

> *Machine-checked in Lean 4. See `formal/` for proofs.*  
> *Lean 4 机器检查验证。证明见 `formal/`。*

## F.1 Abstract Syntax / 抽象语法

The formal semantics is defined over an abstract syntax tree (AST) that strips
away surface syntax concerns (precedence, whitespace, comments). The AST types
are defined in `formal/n3v3/Spec/Syntax.lean`.

形式化语义基于抽象语法树（AST），剥离了表面语法的细节。AST 类型定义在 `formal/n3v3/Spec/Syntax.lean`。

### Types (Ty)

```
Ty ::= Int | Float | Bool | Char | String | Unit
     | List Ty | Tuple (List Ty) | Record (List (String × Ty))
     | Fn (param: Ty) (ret: Ty) (effect: Effect)
     | Option Ty | Command | Pipeline | ProcessResult | Task Ty
```

`Fn` carries an `Effect` annotation:
- `Effect.Pure` — no host effects
- `Effect.Effectful` — may perform I/O, process execution, network

### Expressions (Expr)

```
Expr ::= lit_int(n) | lit_float(f) | lit_bool(b) | lit_char(c)
       | lit_string(s) | lit_unit
       | var(x) | app(f, arg) | lam(x, body)
       | letIn(x, val, body) | binop(op, l, r)
       | matchOn(scrutinee, arms) | builtin(name, args)
```

### Values (Value)

```
Value ::= int(n) | float(f) | bool(b) | char(c) | string(s) | unit
        | list(List Value) | tuple(List Value) | record(List (String × Value))
        | processResult(code, stdout, stderr)
        | closure(x, body, env)
```

### BinOp

```
BinOp ::= Add | Sub | Mul | Div | Mod
        | Eq | Neq | Lt | Le | Gt | Ge
        | And | Or | Pipe
```

### Patterns (Pattern)

```
Pattern ::= wildcard | var(x) | lit_int(n) | lit_bool(b) | lit_string(s)
          | tuple(List Pattern) | list(List Pattern, rest: Bool)
          | record(List (String × Pattern))
```

## F.2 Type System / 类型系统

Defined in `formal/n3v3/Spec/Typing.lean`. The typing judgment is:

```
Γ ⊢ e : τ   — "expression e has type τ in context Γ"
```

Where `Γ : Ctx = List (String × Ty)`.

### Key typing rules

| Rule | Premises | Conclusion |
|------|----------|------------|
| lit_int | — | Γ ⊢ lit_int(n) : Int |
| lit_bool | — | Γ ⊢ lit_bool(b) : Bool |
| var | (x, τ) ∈ Γ | Γ ⊢ var(x) : τ |
| lam | (x,τ₁)::Γ ⊢ body : τ₂ | Γ ⊢ lam(x,body) : Fn τ₁ τ₂ eff |
| app | Γ ⊢ f : Fn τ₁ τ₂ eff, Γ ⊢ arg : τ₁ | Γ ⊢ app(f,arg) : τ₂ |
| let | Γ ⊢ val : τ, (x,τ)::Γ ⊢ body : τ' | Γ ⊢ letIn(x,val,body) : τ' |
| match | Γ ⊢ scrutinee : τs, AllArmsMatch(Γ, arms, τs, τ) | Γ ⊢ matchOn(scrutinee, arms) : τ |

The `AllArmsMatch` judgment ensures all match arms return the same type and
patterns are well-typed in their respective extended contexts.

### Pattern typing: PatHasType

```
PatHasType(Γ, p, τ, Γ') — "pattern p matches type τ, extending context Γ to Γ'"
```

| Pattern | Extended context Γ' |
|---------|-------------------|
| wildcard | Γ (unchanged) |
| var(x) | (x, τ) :: Γ |
| lit_int(n) | Γ (unchanged) |
| lit_bool(b) | Γ (unchanged) |

## F.3 Evaluation Semantics / 求值语义

Defined in `formal/n3v3/Spec/Eval.lean`. The evaluation judgment is:

```
env ⊢ e ⇓ v   — "expression e evaluates to value v in environment env"
```

Where `env : Env = List (String × Value)`.

This is a **big-step operational semantics** (natural semantics). The rules
define a direct relation between expressions and their values.

### Key evaluation rules

| Expression | Rule | Result |
|-----------|------|--------|
| lit_int(n) | — | int(n) |
| lit_bool(b) | — | bool(b) |
| var(x) | (x,v) ∈ env | v |
| lam(x,body) | — | closure(x,body,env) |
| app(f,arg) | f⇓closure(x,body,env'), arg⇓varg, (x,varg)::env'⊢body⇓vres | vres |
| letIn(x,val,body) | val⇓vval, (x,vval)::env⊢body⇓vbody | vbody |
| binop(Add,l,r) | l⇓int(n), r⇓int(m) | int(n+m) |
| matchOn(scrutinee,arms) | scrutinee⇓v, first arm (p,e) matches v with binds, binds++env⊢e⇓vres | vres |

### Pattern matching: Matches

```
Matches(p, v, binds) — "pattern p matches value v, producing bindings binds"
```

| Pattern | Binding |
|---------|---------|
| wildcard | [] |
| var(x) | [(x, v)] |
| lit_int(n) | [] (if v = int(n)) |
| lit_bool(b) | [] (if v = bool(b)) |

## F.4 Effect Semantics / 副作用语义

Defined in `formal/n3v3/Spec/Effects.lean` (v4.3, 34 rules). The effectful evaluation judgment:

```
env ⊢ e ⇓[σ] v, σ'   — "e evaluates to v, transforming I/O state σ to σ'"
```

Where `IOState = { stdin, stdout, stderr }` accumulates process I/O.

### Effect rules (v4.3, 34 rules)

| Category | Rule | Description |
|----------|------|-------------|
| Pure | `pure` | Lift BigStep, σ unchanged |
| Blocking | `execCommand` | Execute a process |
| Blocking | `execPipeline` | Execute a pipeline |
| Deferred | `spawn` | Create a deferred task |
| Deferred | `awaitTask` | Block on task completion |
| Deferred | `awaitTaskTimeout` | Timeout before completion |
| Deferred | `awaitTasks` | Await multiple tasks |
| Deferred | `cancel` | Cancel a spawned task |
| Deferred | `awaitAny` | Await first completing task |
| Streaming | `execCommandStreaming` | Streaming command execution |
| Streaming | `execPipelineStreaming` | Streaming pipeline execution |
| Streaming | `execCommandStreamingTimeout` | Streaming with timeout (success) |
| Streaming | `execCommandStreamingTimeoutExpired` | Streaming with timeout (expired) |
| Streaming | `execPipelineStreamingTimeout` | Pipeline streaming with timeout (success) |
| Streaming | `execPipelineStreamingTimeoutExpired` | Pipeline streaming with timeout (expired) |
| Streaming | `readFileLines` | Read file line by line |
| File | `readFile` | Read file content |
| File | `writeFile` | Write file content |
| File | `readFileBytes` | Read file as Bytes |
| File | `writeFileBytes` | Write Bytes to file |
| Stream | `streamCollect` | Collect stream into list |
| Stream | `streamPipe` | Pipe stream into command stdin |
| Stream | `streamForEach` | Consume stream element-wise |
| Stream | `streamFold` | Strict fold over stream |
| Stream | `streamWithTimeout` | Element-level timeout wrapper |
| Retry | `retrySuccess` | Retry on success |
| Retry | `retryFailure` | Retry exhausted |
| Ensure | `ensureSuccess` | Condition wait success |
| Ensure | `ensureTimeout` | Condition wait timeout |

### Size limit enforcement (v4)

```
MAX_STDIN_BYTES   = 10 * 1024 * 1024  (10 MB)
MAX_OUTPUT_BYTES  = 50 * 1024 * 1024  (50 MB)
MAX_STREAM_LINES  = 100_000           (100k lines)
```

These are **mandatory premises** in every applicable rule — any valid
`EffectEval` derivation tree is a proof that limits were respected.

## F.5 Type Safety / 类型安全

Theorem in `formal/n3v3/Proofs/Safety.lean`:

```
type_safety : [] ⊢ e : τ → ∃ v, [] ⊢ e ⇓ v
```

**"Well-typed closed programs do not get stuck."**

Status: 13 of 17 HasType constructors have machine-checked proofs
(lit_*, var, lam, letIn, app lam case, pipe lam case,
all 12 BinOp operators). Additional verified lemmas in
`SafetyLemmas.lean` cover wildcard, lit_int, lit_bool,
bool_full (both arms via matchOn_fallthrough), and unit
pattern matching. Three axioms remain for non-lam app/pipe
(big-step closure body recursion) and general matchOn
(Lean 4.29 mutual inductive limitation).

## F.6 Verified Security Properties / 已验证安全性质

The following properties are stated and machine-checked in `formal/n3v3/Verify/`:

### Path Safety (Verify/Path.lean)

```
Theorem path_safety_with_safe_cwd:
  For any redirect path, if the path contains "..", the sentinel is returned.
  If cwd and redirect are both safe (no ".."), the resolved path is safe.
```

Corresponds to Rust: `resolve_redirect_path` in `crates/n3v3-std/src/io/mod.rs`.
Security audit finding: M-1 (path traversal).

### Environment Safety (Verify/Environ.lean)

```
Theorem env_safety_theorem:
  For any command environment, after stripping dangerous keys
  (LD_PRELOAD, LD_LIBRARY_PATH, DYLD_INSERT_LIBRARIES, DYLD_LIBRARY_PATH),
  the child process environment is safe.
```

Corresponds to Rust: `configured_process_command` in `crates/n3v3-std/src/io/mod.rs`.
Security audit finding: M-4 (environment injection).

### Buffer Size Limits (Verify/Limits.lean)

```
Theorem full_check_correct:
  The stdin check succeeds iff size ≤ MAX_STDIN_BYTES.
  The output check succeeds iff both stdout and stderr ≤ MAX_OUTPUT_BYTES.

Theorem premises_equivalent_to_checks:
  The EffectEval premises are exactly equivalent to the check functions passing.
```

Corresponds to Rust: all five blocking execution paths in `crates/n3v3-std/src/io/mod.rs`.
Security audit findings: H-1 (stdin size), H-2 (output size).

---


</div>
