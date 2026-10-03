<div align="center">

<img src="../../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>Complete Tutorial</h1>

<p><em>完整教程</em></p>

<p>
  <strong><a href="../../README.md">Home</a></strong> ·
  <strong><a href="../README.md">Docs</a></strong>
</p>

</div>

---

> Learn the language from basics to modules. / 从基础到模块的完整教程。

## 1. Basics / 基础


### Values and Bindings

All bindings are immutable:

```n3v3
x = 42
name = "Alice"
valid = true
```

### Functions

```n3v3
-- Named function
add(x: Int, y: Int) -> Int = x + y

-- Lambda
multiply = |x, y| x * y

-- With string interpolation
greet(name) = `Hello, {name}!`
```

### Records

```n3v3
user = {
    name = "Bob",
    age = 30,
}

-- Access
n = user.name

-- Update (creates new record)
older = user & { age = 31 }

-- Shorthand
name = "Alice"
u = { name, age = 25 }  -- same as { name = name, age = 25 }
```

### Lists

```n3v3
nums = [1, 2, 3, 4, 5]

-- Concatenate
combined = [1, 2] ++ [3, 4]

-- Comprehension
doubled = [x * 2 | x <- nums]
filtered = [x | x <- nums, x > 2]
```

### Blocks

```n3v3
result = {
    a = 10
    b = 20
    a + b   -- last expression is returned
}
```

---


### 值和绑定

所有绑定都是不可变的：

```n3v3
x = 42
name = "Alice"
valid = true
```

### 函数

```n3v3
-- 命名函数
add(x: Int, y: Int) -> Int = x + y

-- Lambda
multiply = |x, y| x * y

-- 带字符串插值
greet(name) = `你好，{name}！`
```

### 记录

```n3v3
user = {
    name = "小明",
    age = 30,
}

-- 访问字段
n = user.name

-- 更新（创建新记录）
older = user & { age = 31 }

-- 简写
name = "小红"
u = { name, age = 25 }  -- 等价于 { name = name, age = 25 }
```

### 列表

```n3v3
nums = [1, 2, 3, 4, 5]

-- 拼接
combined = [1, 2] ++ [3, 4]

-- 推导
doubled = [x * 2 | x <- nums]
filtered = [x | x <- nums, x > 2]
```

### 代码块

```n3v3
result = {
    a = 10
    b = 20
    a + b   -- 最后一个表达式作为返回值
}
```

---


## 2. Type System / 类型系统


### Basic Types

```n3v3
-- Primitive types include: Int, Float, Bool, Char, String, Unit
```

### Compound Types

```n3v3
-- Tuple
type Point = (Int, Int)

-- List
type Numbers = List<Int>

-- Record type
type User = { name: String, age: Int }
```

### Generics

```n3v3
first<T>(xs: List<T>) -> Option<T> = match xs {
    [] -> None,
    [h, ..] -> Some(h),
}

identity<T>(x: T) -> T = x
```

### Type Inference

n3v3 uses Hindley-Milner:

```n3v3
double = |x| x * 2     -- inferred: Int -> Int
id = |x| x             -- inferred: forall a. a -> a
```

---


### 基本类型

```n3v3
-- 基本类型包括：Int、Float、Bool、Char、String、Unit
```

### 复合类型

```n3v3
-- 元组
type Point = (Int, Int)

-- 列表
type Numbers = List<Int>

-- 记录类型
type User = { name: String, age: Int }
```

### 泛型

```n3v3
first<T>(xs: List<T>) -> Option<T> = match xs {
    [] -> None,
    [h, ..] -> Some(h),
}

identity<T>(x: T) -> T = x
```

### 类型推导

n3v3 用的是 Hindley-Milner 算法：

```n3v3
double = |x| x * 2     -- 推导出：Int -> Int
id = |x| x             -- 推导出：forall a. a -> a
```

---


## 3. Pattern Matching / 模式匹配


### Basics

```n3v3
describe(x) = match x {
    0 -> "zero",
    1 -> "one",
    n -> `other: {n}`,
}
```

### Lists

```n3v3
sum(xs) = match xs {
    [] -> 0,
    [h, ..t] -> h + sum(t),
}
```

### Records

```n3v3
getName(user) = match user {
    { name, .. } -> name,
    _ -> "unknown",
}

isAdult(user) = match user {
    { age, .. } if age >= 18 -> true,
    _ -> false,
}
```

### Option and Result

```n3v3
divide(a, b) = {
    if b == 0 -> Err("div by zero")
    else Ok(a / b)
}

match divide(10, 2) {
    Ok(n) -> `Got: {n}`,
    Err(e) -> `Error: {e}`,
}
```

---


### 基础

```n3v3
describe(x) = match x {
    0 -> "零",
    1 -> "一",
    n -> `其他：{n}`,
}
```

### 列表匹配

```n3v3
sum(xs) = match xs {
    [] -> 0,
    [h, ..t] -> h + sum(t),
}
```

### 记录匹配

```n3v3
getName(user) = match user {
    { name, .. } -> name,
    _ -> "未知",
}

isAdult(user) = match user {
    { age, .. } if age >= 18 -> true,
    _ -> false,
}
```

### Option 和 Result

```n3v3
divide(a, b) = {
    if b == 0 -> Err("除以零了")
    else Ok(a / b)
}

match divide(10, 2) {
    Ok(n) -> `结果：{n}`,
    Err(e) -> `出错：{e}`,
}
```

---


## 4. Traits / Trait


### Define

```n3v3
trait Show {
    fn show(self) -> String;
}

trait Eq {
    fn eq(self, other: Self) -> Bool;
}

type Point = Int;

impl Show for Point {
    fn show(self) -> String = `Point({self})`;
}

impl Eq for Point {
    fn eq(self, other: Self) -> Bool = self == other;
}
```

### Bounds

A bound such as `T: Show` requires `T` to implement `Show`; a function also needs a real body returning its declared result.

---


### 定义

```n3v3
trait Show {
    fn show(self) -> String;
}

trait Eq {
    fn eq(self, other: Self) -> Bool;
}

type Point = Int;

impl Show for Point {
    fn show(self) -> String = `Point({self})`;
}

impl Eq for Point {
    fn eq(self, other: Self) -> Bool = self == other;
}
```

### 约束

像 `T: Show` 这样的约束要求 `T` 实现 `Show`；函数还必须有返回声明结果的真实函数体。

---


## 5. Modules / 模块

`use std.list = list` imports the standard list module with the alias `list`. The bundled [module lesson](../../examples/learning/04_modules.n3v3) uses only this standard module; no local import paths, system files, or network are needed.
`use std.list = list` 用别名 `list` 导入标准列表模块。[模块课程](../../examples/learning/04_modules.n3v3) 只使用该标准模块，不需要本地导入路径、系统文件或网络。

```n3v3-check
use std.list = list

numbers = [1, 2, 3, 4]
doubled = list.map(|x| x * 2, numbers)
even = list.filter(|x| x % 2 == 0, numbers)
result = even
```

Run `n3v3 run examples/learning/04_modules.n3v3` from the repository root. The final value is `[2, 4]`.
在仓库根目录运行 `n3v3 run examples/learning/04_modules.n3v3`，最终结果是 `[2, 4]`。

---


## 6. Best Practices / 写代码的建议


1. **Annotate public APIs** to make their contracts clear.
2. **Use immutable data** and pipes for transformation chains.
3. **Match exhaustively** — handle the empty list as well as nonempty lists.

```n3v3-check
use std.list = list

data = [1, 2, 3, 4]
valid = |x| x % 2 == 0
transform = |x| x * 2

filter_valid(xs) = list.filter(valid, xs)
map_transform(xs) = list.map(transform, xs)

sum(xs) = match xs {
    [] -> 0,
    [head, ..rest] -> head + sum(rest),
}

result = data
    |> filter_valid
    |> map_transform
    |> sum
```

`list.map` and `list.filter` run as shown. For an executable sum, use exhaustive list matching rather than `list.fold`: its type-checker currently accepts `(init, function, list)`, but the evaluator expects `(init, list, function)` and rejects that call. This pipeline produces `12`.
`list.map` 和 `list.filter` 可按示例运行。求和时请使用穷尽的列表匹配，而不是 `list.fold`：其类型检查器目前接受 `(init, function, list)`，求值器却要求 `(init, list, function)`，因此前一种调用会在运行时报错。此管道计算结果为 `12`。

1. **公开 API 加上类型注解**，明确其契约。
2. **用不可变数据和管道**构建数据变换链。
3. **匹配要穷尽**——既处理空列表，也处理非空列表。

---


## 7. Mini-project: Space Supplies / 趣味小项目：太空补给计分

The [space-supplies example](../../examples/learning/08_space_supplies.n3v3) uses no network or filesystem APIs and has no platform-specific paths. Run it from the repository root; the CLI may separately warn if it cannot lock the repository's flake input.
[太空补给示例](../../examples/learning/08_space_supplies.n3v3)本身不调用网络或文件系统 API，也不依赖平台路径。从仓库根目录运行；如果 CLI 无法锁定仓库的 flake 输入，可能另行打印警告。

1. Make a list of records with `name` and `units`. A zero-unit entry stays in the manifest to show what was not loaded.
   建立含 `name`、`units` 字段的记录列表。零单位的项目留在清单中，表示该物资尚未装载。
2. `score(item)` matches the supply name: oxygen earns three points per unit, water two, and anything else one. `sum_scores(items)` matches both the empty list and a head with its remaining tail.
   `score(item)` 匹配物资名称：氧气每单位得三分，水得两分，其他得一分。`sum_scores(items)` 同时匹配空列表及含头项和剩余项的列表。
3. The comprehension `[item | item <- supplies, item.units > 0]` keeps loaded supplies. `loaded |> sum_scores` passes that list into the scorer; for four oxygen and two water units the total is `4 * 3 + 2 * 2 = 16`.
   列表推导式 `[item | item <- supplies, item.units > 0]` 仅保留已装载物资；`loaded |> sum_scores` 将其传入计分函数。四单位氧气和两单位水的总分是 `4 * 3 + 2 * 2 = 16`。
4. A `report` record stores the mission name, score and readiness status; the final `result = report.score` selects a predictable scalar output instead of depending on record field display order.
   `report` 记录保存任务名称、分数和准备状态；最后的 `result = report.score` 取出确定的标量结果，不依赖记录字段的显示顺序。

```bash
n3v3 run examples/learning/08_space_supplies.n3v3
```

The result line is `[OK] 16`. Try changing `units` or adding a supply name: the fallback match branch will score the new item.
结果行是 `[OK] 16`。可以修改 `units` 或添加新物资名称：匹配中的兜底分支会为新物资计分。

---

## Next / 接下来


- [Spec](../reference/spec.md) — full language reference
- [API](../reference/api.md) — standard library
- [Philosophy](../project/philosophy.md) — why these design choices

---


- [语言规范](../reference/spec.md) — 完整语法参考
- [标准库](../reference/api.md) — API 文档
- [设计哲学](../project/philosophy.md) — 为什么这样设计

---

<div align="center">

```
═══════════════════════════════════════════════════════════════════════════════
                           Happy hacking! 写代码愉快！
═══════════════════════════════════════════════════════════════════════════════
```

</div>
