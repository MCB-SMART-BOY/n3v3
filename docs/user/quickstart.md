<div align="center">

<img src="../../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>5-Minute Quick Start</h1>

<p><em>5 分钟快速上手</em></p>

<p>
  <strong><a href="../../README.md">Home</a></strong> ·
  <strong><a href="../README.md">Docs</a></strong>
</p>

</div>

---

> *Life's too short for long tutorials. Let's get you hacking in 5 minutes.*  
> 人生苦短，教程太长。5 分钟让你上手。

## Step 1: Install (30 sec) / 第一步：安装（30 秒）

```bash
# Pre-built binary
curl -fsSL https://github.com/MCB-SMART-BOY/n3v3/releases/latest/download/n3v3-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv n3v3 /usr/local/bin/

# Or Arch Linux (AUR package: n3v3-bin)
yay -S n3v3-bin

# From source / 从源码安装
git clone https://github.com/MCB-SMART-BOY/n3v3.git && cd n3v3
cargo install --path n3v3-cli --locked

# Or build without installing into PATH / 或仅构建二进制而不安装到 PATH
# cargo build --release
# ./target/release/n3v3 repl
```

## Step 2: Play with REPL (1 min) / 第二步：玩玩 REPL（1 分钟）

```bash
$ n3v3 repl
n3v3> 1 + 2 * 3
7
n3v3> double = |x| x * 2
n3v3> double(21)
42
n3v3> { name = "hacker", power = 9001 }
{power = 9001, name = "hacker"}   # 字段打印顺序不保证
n3v3> { a = 10; b = 20; a + b }
30
n3v3> :quit
```

**REPL Commands / 常用命令:** `:help` `:env` `:clear` `:load file.n3v3` `:quit`

## Step 3: Write a File (1 min) / 第三步：写个文件（1 分钟）

Create `hello.n3v3`:
创建 `hello.n3v3`：

```n3v3-check
greet(name: String) -> String = `Hello, {name}!`

factorial(n: Int) -> Int = {
    if n <= 1 -> 1 else n * factorial(n - 1)
}

let result = {
    greeting = greet("World"),
    magic = factorial(5),
}
```

Run it:
运行：

```bash
$ n3v3 run hello.n3v3
[OK] #{greeting = "Hello, World!", magic = 120}   # 字段打印顺序不保证

$ n3v3 check hello.n3v3
[OK] OK - No errors found
# Effect checking is enabled by default; use --allow-effects for effectful examples.
```

## Step 4: Types (1 min) / 第四步：类型系统（1 分钟）

```n3v3
-- Inferred
x = 42                -- x: Int
f = |n| n * 2       -- f: Int -> Int

-- Explicit
add(a: Int, b: Int) -> Int = a + b

-- Generics
identity<T>(x: T) -> T = x
```

## Step 5: Pattern Matching (1 min) / 第五步：模式匹配（1 分钟）

```n3v3
describe(opt) = match opt {
    Some(x) -> `Got: {x}`,
    None    -> "Nothing",
}

sum(xs) = match xs {
    []       -> 0,
    [h, ..t] -> h + sum(t),
}
```

## Cheat Sheet / 语法速查

| What / 项目 | n3v3 |
|------------|------|
| Record | `{ x = 1 }` |
| Lambda | `\|x\| x + 1` |
| Function | `add(a, b) = a + b` |
| Block | `{ x = 1; x }` |
| List | `[1, 2, 3]` |
| Pipe | `x \|> f \|> g` |
| Interpolation | `` `Hello {name}` `` |
| Match | `match x { p -> e }` |
| Comment | `-- text --` or `& text` (v4.0) |

## Next / 接下来

- [Tutorial](tutorial.md) — deeper dive / 深入学习
- [Spec](../reference/spec.md) — language reference / 语言规范
- [API](../reference/api.md) — standard library / 标准库参考

---

> Now go build something cool! / 去构建点酷的东西吧！
