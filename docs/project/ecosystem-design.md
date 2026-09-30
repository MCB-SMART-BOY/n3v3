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

This page describes the v5.0.0 ecosystem surface; implementation status is marked as `Implemented`, `Experimental`, or `Planned` and linked to the relevant code or command.
本文描述 v5.0.0 的生态系统表面；实现状态使用 `Implemented`、`Experimental` 或 `Planned` 标记，并指向对应代码或命令。

## 1. Architecture

n3v3's ecosystem is built on a Nix-inspired content-addressed model:

```
flake.n3v3 ──→ Flake (inputs + outputs)
    │
    ▼
flake.lock ──→ FlakeLock (pinned hashes)
    │
    ▼
n3v3-store ──→ Content-addressed /n3v3/store
    │
    ├── n3v3-fetch (URL, Git, local)
    ├── n3v3-builder (sandboxed builds)
    └── n3v3-config (system configuration)
```

### 1.1 Design Principles

**Content addressing**: Every artifact in the store is identified by a cryptographic hash of its contents. This guarantees:
- **Reproducibility**: Same inputs always produce the same output path.
- **Deduplication**: Identical artifacts are stored only once.
- **Integrity**: Tampered artifacts are detectable via hash mismatch.

**Sandboxed builds**: Every build runs in an isolated environment with:
- No network access unless explicitly declared.
- Restricted filesystem access (only declared inputs).
- Deterministic build outputs.

**Generational profiles**: User environments use atomic generation-based switching:
- Each `n3v3 package install` creates a new generation.
- `n3v3 package rollback` switches to the previous generation atomically.
- Generations are garbage-collection roots, protecting active packages.

### 1.2 Crate Architecture

| Crate | Purpose | Status |
|-------|---------|--------|
| `n3v3-fetch` | URL, Git, and local file fetching with hash verification | Implemented |
| `n3v3-store` | Content-addressed store with NAR archives, signatures, GC | Implemented |
| `n3v3-builder` | Sandboxed builds (native, Docker, simple backends) | Implemented |
| `n3v3-config` | System configuration with generation-based rollback | Implemented |
The published CLI package is `n3v3`, and it installs the `n3v3` binary; the subsystem crates above are workspace crates under `crates/`.
已发布的 CLI 软件包名为 `n3v3`，安装后提供 `n3v3` 二进制文件；上表子系统 crate 均为 `crates/` 下的 workspace crate。

## 2. Flake System / Implemented

### 2.1 `flake.n3v3`: Project Manifest

The flake manifest declares inputs (dependencies) and outputs (packages, modules, configurations):

```n3v3
{
    description = "Example project manifest",
    inputs = {
        nixpkgs = { url = "github:NixOS/nixpkgs/nixos-unstable" },
        n3v3_std = { url = "github:MCB-SMART-BOY/n3v3/v5.0.0" },
    },
    outputs = fn(inputs) {
        packages = {
            hello = {
                name = "hello",
                src = "./src",
                builder = inputs.n3v3_std.buildN3v3Package
            }
        }
    },
}
```

Records are always `{ ... }` (the `#{ ... }` spelling is a legacy alias kept for
compatibility), a lambda is written `fn(inputs) { ... }`, and manifests are plain
values — the same shape as `examples/flake.n3v3`.
记录统一写作 `{ ... }`（`#{ ... }` 是保留的旧写法），lambda 写作 `fn(inputs) { ... }`，
清单本身是普通值，与 `examples/flake.n3v3` 同形。

### 2.2 `flake.lock`: Dependency Lockfile

JSON lockfile pinning all dependency hashes for reproducible builds:

```json
{
  "nodes": {
    "nixpkgs": {
      "locked": {
        "narHash": "sha256-abc123...",
        "type": "github",
        "owner": "NixOS",
        "repo": "nixpkgs",
        "rev": "abc123..."
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

### 3.1 Content-Addressed Storage

The store resides at `/n3v3/store` (configurable via `N3V3_STORE` environment variable). Each artifact is stored at a path derived from its content hash:

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

Packages are serialized as NAR (n3v3 ARchive) archives, a deterministic archive format that preserves:
- File permissions and types (regular, directory, symlink)
- Modification times (normalized for reproducibility)
- File contents

NAR archives support Ed25519 signature verification for integrity.

### 3.3 Garbage Collection

The store supports generation-based garbage collection:
- **Generation roots**: Active profiles and their generations protect packages.
- **GC sweep**: `n3v3 store gc` removes unreferenced store paths.
- **Dry-run mode**: Preview what would be removed before executing.

### 3.4 Binary Cache / Substituter

Store artifacts can be served via binary caches:
- **Cache URLs**: Remote HTTP(S) endpoints serving NAR archives.
- **narinfo files**: Metadata files with hashes and signatures.
- **Substitution**: `n3v3 build` can download pre-built artifacts instead of building locally.
- **Upload**: Successful builds can be uploaded to writable caches.

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

When installing a package, the installer:
1. Searches for exact matches in the store.
2. Falls back to prefix matching (e.g., `hello` matches `hello-1.0`).
3. Reports ambiguous matches when multiple versions exist.

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
| `native` | Build directly on host | Development, trusted packages |
| `docker` | Build in Docker container | CI, untrusted packages |
| `simple` | Minimal sandbox with seccomp | Lightweight isolation |
| `auto` | Auto-select best backend | Default |

### 5.2 Sandbox Features

- **Filesystem isolation**: Only declared inputs are visible.
- **Network isolation**: No network access unless declared in flake.
- **seccomp filtering**: System call filtering on Linux.
- **User namespace**: Unprivileged user mapping.

## 6. Future Directions

### 6.1 Package Index / Registry / Implemented

n3v3 supports a central package registry for discovering and publishing packages. The registry CLI is now available:

```bash
n3v3 registry-update   # Update local registry index
n3v3 registry-serve    # Start local registry server
n3v3 registry-publish  # Publish package to registry
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
