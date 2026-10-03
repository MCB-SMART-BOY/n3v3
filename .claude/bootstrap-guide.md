<div align="center">

<img src="../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>Bootstrap Package Examples</h1>

<p><em>Bootstrap 示例包</em></p>

<p>
  <strong><a href="../README.md">Home</a></strong> ·
  <strong><a href="../docs/README.md">Docs</a></strong>
</p>

</div>

---

This is a planned bootstrap package-chain sketch, not a runnable n3v3 package example. The code block below uses proposed derivation syntax and placeholder sources/hashes; it must not be copied into `n3v3 build`. No package files from the former `examples/bootstrap/` directory ship today. `examples/ci-bootstrap.n3v3` is an unrelated, executable CI script, not a bootstrap package.
本文档是计划中的 bootstrap 包链草图，不是可运行的 n3v3 包示例。下方代码块包含提议中的 derivation 语法及占位源地址/哈希，不应复制给 `n3v3 build` 执行。旧 `examples/bootstrap/` 目录下的包文件现已不随仓库提供。`examples/ci-bootstrap.n3v3` 是独立的可执行 CI 脚本，不是 bootstrap 包。

## 什么是 Bootstrap 基础包? / What Are Bootstrap Packages?

这些示例代表从零启动工具链时最早期的一批基础构件，用来表达 n3v3 将来如何描述自举过程。

These examples represent the earliest building blocks of a future bootstrap chain and show how n3v3 may describe that process.

## Bootstrap 顺序 / Bootstrap Order

```
1. musl libc        → C 标准库 / C standard library
2. binutils         → 二进制工具 (ld, as, ar) / Binary utilities
3. gcc              → C/C++ 编译器 / C/C++ compiler
4. make             → 构建工具 / Build tool
5. bash             → Shell 解释器 / Shell interpreter
6. coreutils        → 核心工具 (ls, cp, etc.) / Core utilities
```

## 包定义结构 / Package Definition Structure
The following is proposed package metadata and build-phase pseudocode, not a supported `.n3v3` derivation definition.
以下是拟议包元数据和构建阶段的伪代码，不是当前支持的 `.n3v3` derivation 定义。

```text
{
    name = "package-name",
    version = "1.0.0",

    meta = {
        description = "Package description",
        homepage = "https://...",
        license = "MIT",
        platforms = ["x86_64-linux"],
    },

    src = fetchurl {
        url = "https://...",
        hash = "sha256-...",
    },

    buildInputs = [ /* dependencies */ ],

    buildPhase = ''
        make -j$NIX_BUILD_CORES
    '',

    installPhase = ''
        make install PREFIX=$out
    '',
}
```

## Planned package list / 计划中的包列表

### 📋 计划中 / Planned (示例文件已移除 / example files removed)

- **musl** (1.2.4) - Lightweight C standard library
- **binutils** (2.41) - GNU binary utilities (ld, as, ar, objdump, etc.)
- **gcc** (13.2.0) - GNU Compiler Collection (C, C++)

### 📋 计划中 / Planned

- **make** - GNU Make build tool
- **bash** - Bourne Again Shell
- **coreutils** - GNU core utilities
- **findutils** - GNU find, xargs, locate
- **diffutils** - GNU diff, cmp, diff3
- **patch** - GNU patch utility
- **sed** - Stream editor
- **grep** - Pattern matching
- **gawk** - GNU awk
- **gzip** - Compression utility
- **bzip2** - Compression utility
- **xz** - Compression utility
- **tar** - Archive tool

## 设计原则 / Design Principles

### 1. 最小化依赖 / Minimal Dependencies

Bootstrap 包应该尽可能少地依赖其他包，理想情况下只依赖更早阶段的基础构件。

### 2. 可复现构建 / Reproducible Builds

所有包必须:
- 使用固定版本
- 包含 SHA-256 校验和
- 避免网络访问(构建时)
- 使用确定性构建标志

### 3. 文档化 / Documentation

每个包应包含:
- 清晰的描述
- 构建步骤说明
- 依赖关系
- 许可证信息

### 4. 优化空间 / Space Optimization

- 移除不必要的文档和本地化文件
- Strip 二进制文件
- 分离开发文件到 `dev` 输出

## Intended usage after packages are implemented / 包实现后的预期用法

The CLI supports `n3v3 info` and (on Unix) `n3v3 build`, but the package definitions described above do not exist in this tree. The following build command is only an intended future workflow, not an executable example for this repository:
CLI 提供 `n3v3 info` 及（Unix 上的）`n3v3 build`，但本页描述的包定义在仓库中尚不存在。以下构建命令仅表示未来预期流程，不是本仓库可执行的示例：

```text
n3v3 build <package.n3v3>   # future package file / 未来的包文件
```

### 查看包信息 / Show Package Info

```bash
n3v3 info                # 包与平台信息 / package and platform information
n3v3 doc registry        # 包注册表文档 / package registry documentation
```

## 哈希值获取 / Getting Hashes

由于包定义中使用的哈希值是占位符,实际使用时需要获取真实哈希:

```bash
# 方法 1: 使用 nix-prefetch-url (如果可用)
nix-prefetch-url https://musl.libc.org/releases/musl-1.2.4.tar.gz

# 方法 2: 手动下载并计算
wget https://musl.libc.org/releases/musl-1.2.4.tar.gz
sha256sum musl-1.2.4.tar.gz
```

## 与 Nix 的区别 / Differences from Nix

虽然 n3v3 参考了 Nix 的设计,但有关键区别:

1. **语法**: n3v3 使用现代化的零歧义语法
2. **类型系统**: 强类型,Hindley-Milner 推导
3. **兼容性**: 不兼容 nixpkgs,从零构建生态

## Future contributions / 未来的贡献方向

Once runnable bootstrap packages exist, contributors should add a checked `.n3v3` file under `examples/`, specify real source hashes and dependencies, verify the build, and only then propose a pull request.
待 bootstrap 包具备可运行定义后，贡献者应在 `examples/` 中添加已验证的 `.n3v3` 文件，填写真实源哈希及依赖，验证构建后再提交 Pull Request。

## 参考资料 / References

- [Linux From Scratch](http://www.linuxfromscratch.org/)
- [Nix Pills](https://nixos.org/guides/nix-pills/)
- [GNU Build System](https://www.gnu.org/software/automake/manual/html_node/Autotools-Introduction.html)

---

*Describe bootstrap packages in n3v3.*
