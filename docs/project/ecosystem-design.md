<div align="center">

<img src="../../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>n3v3 Ecosystem Design</h1>

<p><em>生态系统设计</em></p>

<p>
  <strong><a href="../../README.md">Home</a></strong> ·
  <strong><a href="../README.md">Docs</a></strong>
</p>

</div>

---

# n3v3 Ecosystem Design

This page describes the v5.0.3 workspace ecosystem surface; implementation status is marked as `Implemented`, `Experimental`, or `Planned` and linked to the relevant code or command. v5.0.2 was published; consult [GitHub Releases](https://github.com/MCB-SMART-BOY/n3v3/releases) for the latest published release.
本文描述 v5.0.3 工作区生态系统表面；实现状态使用 `Implemented`、`Experimental` 或 `Planned` 标记，并指向对应代码或命令。v5.0.2 已发布；最近发布版本请查看 [GitHub Releases](https://github.com/MCB-SMART-BOY/n3v3/releases)。

## 1. Architecture

n3v3's ecosystem is built on a Nix-inspired content-addressed model:

```
flake.n3v3 ──→ Flake (inputs + outputs)
    │
    ▼
flake.lock ──→ FlakeLock (pinned hashes)
    │
    ▼
n3v3-store ──→ Store paths and binary caches
    │
    ├── n3v3-fetch (URL, Git, local)
    ├── n3v3-builder (native, Docker, simple backends)
    └── n3v3-config (system configuration)
```

### 1.1 Design Principles

**Content addressing / 内容寻址**: Directly added files and directories use content hashes; derivation output paths derive from the derivation hash, while NAR content hashes are recorded and checked separately during cache substitution. A stable path for identical inputs is not a guarantee of identical build bytes; reproducibility depends on the builder, dependencies, environment, and chosen backend (see `crates/n3v3-store/src/store.rs`, `crates/n3v3-derive/src/output.rs`, `crates/n3v3-store/src/cache.rs`).
直接加入 store 的文件和目录按内容哈希命名；推导产物路径由推导哈希派生，NAR 内容哈希在缓存替换时另行记录和校验。相同输入得到相同路径不等于产物字节必然相同；可重现性取决于 builder、依赖、环境和所选后端。

**Build isolation / 构建隔离**: `n3v3 build` selects a backend (`auto`, `native`, `docker`, `simple`). On Linux with available namespaces, `native` attempts filesystem and namespace isolation and disables network by default; `docker` defaults to `--network none` but needs Docker and its configured image. `simple` runs a host process with a reduced environment, not a filesystem or network sandbox. Linux `native` execution can fall back to this simple mode if namespace availability changes. Isolation alone does not make outputs deterministic; seccomp policy is not enforced by a BPF filter (`crates/n3v3-builder/src/sandbox.rs`, `crates/n3v3-builder/src/docker.rs`).
`n3v3 build` 可选择 `auto`、`native`、`docker` 或 `simple`。Linux 有可用 namespace 时，`native` 尝试文件系统及命名空间隔离，默认禁用网络；`docker` 默认 `--network none`，但依赖 Docker 与配置的镜像。`simple` 仅以收窄的环境变量在宿主机执行，不隔离文件系统或网络；Linux `native` 在 namespace 可用性变化后也可能回退到该模式。隔离本身不保证产物确定，seccomp 策略目前没有 BPF 过滤器执行。

**Generational profiles / 代际 profile**: A successful local `n3v3 package install` or removal creates a generation. `n3v3 package rollback` switches the active profile to a previous generation; active generations are store GC roots.
本地软件包成功安装或移除后创建新一代；`n3v3 package rollback` 切回上一代；活跃代是 store 垃圾回收根。查询远程版本而安装失败时不会创建新代。

### 1.2 Crate Architecture

| Crate | Purpose | Status |
|-------|---------|--------|
| `n3v3-fetch` | URL, Git, and local file fetching with hash verification | Implemented |
| `n3v3-store` | Content-addressed store with NAR archives, signatures, GC | Implemented |
| `n3v3-builder` | Native/Docker isolation when available; simple host execution fallback | Implemented |
| `n3v3-config` | System configuration with generation-based rollback | Implemented |

The published CLI package is `n3v3`, and it installs the `n3v3` binary; the subsystem crates above are workspace crates under `crates/`.
已发布的 CLI 软件包名为 `n3v3`，安装后提供 `n3v3` 二进制文件；上表子系统 crate 均为 `crates/` 下的 workspace crate。

## 2. Flake System / Implemented

### 2.1 `flake.n3v3`: Project Manifest

The manifest is a plain `flake` binding whose `outputs` field is a lambda. This syntax follows `examples/flake.n3v3` (illustrative subset, not a promised buildable package):
清单使用普通的 `flake` 绑定，`outputs` 字段为 lambda。以下语法参考 `examples/flake.n3v3`（仅展示结构，不承诺是可构建的软件包）：

```n3v3
flake = {
    description = "Example system configuration for n3v3",
    inputs = { nevepkgs = { url = "github:neve-lang/nevepkgs" } },
    outputs = |inputs| ({
        system = { packages = [inputs.nevepkgs.git] }
    })
}
```

Records use `{ ... }`; lambdas use `|inputs| expression` (not `fn(inputs) { ... }`). See `examples/flake.n3v3` for the complete example.
记录使用 `{ ... }`；lambda 使用 `|inputs| expression`（不是 `fn(inputs) { ... }`）。完整示例见 `examples/flake.n3v3`。

### 2.2 `flake.lock`: Dependency Lockfile

The JSON lockfile records resolved input URLs, hashes and (for Git) revisions; it does not guarantee deterministic builder outputs (`crates/n3v3-config/src/flake.rs`):
JSON 锁文件记录解析后的输入 URL、哈希以及 Git 修订版本；不保证 builder 产物确定。

```json
{
  "version": 1,
  "nodes": {
    "root": { "inputs": { "nevepkgs": "nevepkgs" } },
    "nevepkgs": {
      "locked": {
        "url": "github:neve-lang/nevepkgs",
        "narHash": "<resolved-content-hash>",
        "lastModified": 0,
        "rev": "<resolved-git-revision>"
      }
    }
  }
}
```

### 2.3 Input Types

| Type | Syntax | Example |
|------|--------|---------|
| GitHub | `owner/repo` | `github:MCB-SMART-BOY/n3v3` |
| Git | `git+https://...` | `git+https://git.example.com/repo` |
| URL | `https://...` | `https://example.com/pkg.tar.gz` |
| Local path | `./path` | `./lib/mylib` |

## 3. Store / Implemented

### 3.1 Store Paths

The store resides at `/n3v3/store` (configurable via `N3V3_STORE`). Content added directly is hashed; derivation output paths are derived from derivation hashes rather than from the bytes produced by a build:

```
/n3v3/store/
  ├── abc123...-hello-1.0/
  │   ├── bin/
  │   │   └── hello
  │   └── share/
  ├── def456...-coreutils-9.0/
  └── ...
```

### 3.2 NAR Archives

Packages can be serialized as NAR archives preserving file types, permissions and contents; this does not itself establish deterministic build outputs. Binary-cache downloads verify applicable hashes; Ed25519 narinfo signatures are verified only when a cache public key is configured.
软件包可序列化为保留文件类型、权限及内容的 NAR 归档；这本身不保证构建产物确定。二进制缓存下载会校验适用的哈希；仅在配置缓存公钥时校验 Ed25519 narinfo 签名。

### 3.3 Garbage Collection

The store supports generation-based garbage collection:
- **Generation roots**: Active profiles and their generations protect packages.
- **GC sweep**: `n3v3 store gc` removes unreferenced store paths.
- **Dry-run mode**: Preview what would be removed before executing.

### 3.4 Binary Cache / Substituter

Store artifacts can be served via configured binary caches:
已配置的二进制缓存可以提供 store 产物；本地目录或 HTTP(S) 均可用。
- **Cache URLs / 缓存地址**: HTTP(S) endpoints serving NAR archives and narinfo metadata; local directories also work.
- **Substitution / 替换**: `n3v3 build` can download cached artifacts when configured and enabled, instead of building locally; missing/unavailable substitutes fall back to a local build.
- **Signatures / 签名**: Cache signatures are optional; configure `--cache-public-key` for signature verification and `--cache-private-key` for upload signing. Without a public key, signatures are not required by default.
- **Upload / 上传**: Successful builds can be uploaded when a writable cache and upload are configured.

## 4. Package Management / Implemented

### 4.1 CLI Commands

```bash
# Install a package to the user profile
n3v3 package install hello

# Remove a package from the user profile
n3v3 package remove hello

# List installed packages
n3v3 package list

# Search the store and package index
n3v3 search <query>

# Rollback to previous generation
n3v3 package rollback

# Build a package
n3v3 build <package> --backend native

# Update dependencies
n3v3 update
```

### 4.2 Profile Generations

Each installation or removal creates a new profile generation:

```
~/.n3v3/profile/
  ├── generation-1/
  │   ├── manifest
  │   └── bin/
  ├── generation-2/
  │   ├── manifest
  │   └── bin/
  └── current -> generation-2  (symlink)
```

Rollback atomically switches the `current` symlink to the previous generation.

### 4.3 Package Resolution

`n3v3 package install` resolves an existing local store path, then exact or prefix matches. With `N3V3_REGISTRY` set, a missing local package triggers a registry metadata query that reports available versions and stops with instructions to add the package to the local store; it does not download or install the remote package (`n3v3-cli/src/commands/install.rs`).
`n3v3 package install` 从本地 store 查找路径、精确名称或前缀。若设置 `N3V3_REGISTRY`，本地缺失时会查询注册表元数据、报告可用版本并提示先将包加入本地 store；不会下载或安装远程包。
### 4.4 System Configuration

The `n3v3 config` subsystem manages system-wide configuration with the same generation-based model:
- `n3v3 config build`: Build system configuration from flake.
- `n3v3 config switch`: Atomically switch to new configuration.
- `n3v3 config rollback`: Revert to previous configuration.
- `n3v3 config list`: List configuration generations.
- `n3v3 config verify`: Verify generation activation snapshot integrity.

### 4.5 Package Index

n3v3 supports a simple JSON package index for discovery:

```json
[
  {"name": "nevepkgs.git", "description": "Git version control"},
  {"name": "nevepkgs.curl", "description": "URL transfer tool"},
  {"name": "nevepkgs.n3v3", "description": "The n3v3 language"}
]
```

Location: `$HOME/.n3v3/package-index.json` or `$N3V3_PACKAGE_INDEX`

The index is searched by `n3v3 search <query>` which matches against both package name and description.

## 5. Build System

### 5.1 Build Backends

| Backend | Description | Use Case |
|---------|-------------|----------|
| `native` | Linux namespace isolation when available; may fall back to simple execution if availability changes | Linux with working namespaces |
| `docker` | Build inside Docker (default network mode `none`) | Host with Docker and configured image |
| `simple` | Host execution with reduced environment; no filesystem/network isolation | Trusted builds only |
| `auto` | Choose available native, Docker, or simple backend | Default |

### 5.2 Isolation Boundaries

On a Linux host with working namespaces, `native` attempts mount/user/PID/network namespace isolation and a restricted filesystem view; the default `SandboxConfig` disables network. Docker uses its own container settings (default network mode `none`). `simple`, including fallback when native namespaces are unavailable at execution time, does not restrict host filesystem or network access. The seccomp policy in `sandbox.rs` is currently logged, not applied as a syscall filter. Neither isolation nor a derivation hash guarantees reproducible output bytes.
Linux 的 namespace 可用时，`native` 尝试 mount/user/PID/network namespace 及受限文件系统视图，默认 `SandboxConfig` 关闭网络。Docker 使用容器自身设置（默认网络模式 `none`）。`simple`（也包括执行时 native namespace 不可用后的回退）不限制宿主机文件系统和网络；`sandbox.rs` 的 seccomp 策略当前仅记录日志，没有作为系统调用过滤器应用。隔离与推导哈希均不能保证产物字节可重现。

## 6. Future Directions

### 6.1 Package Index / Registry / Implemented (local discovery)

The v1 HTTP API and local registry CLI are implemented; the built-in server binds only loopback, requires `N3V3_REGISTRY_TOKEN` at startup, and authenticates publish requests. The registry client searches and resolves metadata, but `package install` does not fetch remote packages (`n3v3-cli/src/commands/registry_serve.rs`, `n3v3-cli/src/registry_client.rs`, `n3v3-cli/src/commands/install.rs`).
v1 HTTP API 和本地注册表 CLI 已实现；内置服务仅绑定 loopback，启动时要求 `N3V3_REGISTRY_TOKEN`，发布请求必须认证。客户端搜索并解析元数据，但 `package install` 不会获取远程软件包。

```bash
n3v3 registry-update  # Update local registry index
# Set N3V3_REGISTRY_TOKEN privately in the environment before running
# n3v3 registry-serve or n3v3 registry-publish against the loopback server.
```

The package index is a simple JSON file at `$HOME/.n3v3/package-index.json` or `$N3V3_PACKAGE_INDEX`:

```json
[
  {"name": "nevepkgs.git", "description": "Git version control"},
  {"name": "nevepkgs.curl", "description": "URL transfer tool"},
  {"name": "nevepkgs.n3v3", "description": "The n3v3 language"}
]
```

The index is searched by `n3v3 search <query>` which matches against both package name and description.

### 6.2 Module System Integration / Planned

Tighter integration between the flake system and n3v3's module system, allowing `use` to resolve flake inputs.

### 6.3 Remote Registry & Publishing / Planned

Future expansion of the registry system to support remote publishing workflows, decentralized package discovery, and multi-registry federation.

### 6.4 Cross-Platform Support / Planned

Currently package management is Unix-only. Planned work may extend it to Windows via a different store model.

## 7. References

- [Stability Tiers](../reference/stability.md) — Stdlib stability guarantees
- [Forward Plan](../../.claude/forward-plan.md) — Overall project phases
- [Feature Matrix](feature-matrix.md) — Capability assessment
- [API Reference](../reference/api.md) — Stdlib API documentation
- [Specification](../reference/spec.md) — Language specification
