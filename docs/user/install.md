<div align="center">

<img src="../../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>Installation Guide</h1>

<p><em>安装指南</em></p>

<p>
  <strong><a href="../../README.md">Home</a></strong> ·
  <strong><a href="../README.md">Docs</a></strong>
</p>

</div>

---

> *Get n3v3 running. Should take about 2 minutes.*  
> 装个 n3v3，两分钟搞定。

## System Requirements / 系统要求

- **OS**: Linux, macOS, Windows / 操作系统：Linux、macOS、Windows
- **RAM**: 512 MB / 内存：512 MB
- **Disk**: 100 MB / 硬盘：100 MB
- **Docker** (optional): Required for sandbox builds on macOS/Windows / Docker（可选）：macOS/Windows 上想用沙箱构建时需要

## Platform Feature Matrix / 各平台功能支持

| Feature / 功能 | Linux | macOS | Windows |
|---------------|-------|-------|---------|
| Language Core / 语言核心 | ✅ | ✅ | ✅ |
| REPL / 交互式 REPL | ✅ | ✅ | ✅ |
| Formatter/LSP / 格式化与 LSP | ✅ | ✅ | ✅ |
| Native Sandbox Build / 原生沙箱构建 | ✅ | ❌ | ❌ |
| Docker Build / Docker 构建 | ✅ | ✅ | ✅ |
| System Configuration / 系统配置 | ✅ | ❌ | ❌ |

## Quick Install / 快速安装

### Via Cargo (all platforms) / 通过 Cargo 安装（全平台）

```bash
cargo install n3v3
```
This crates.io package is named `n3v3`; it installs the `n3v3` binary.  
该 crates.io 软件包名为 `n3v3`，安装后提供 `n3v3` 二进制文件。

Requires Rust 1.85+. This is the recommended method for developers.

### Linux / macOS Script / Linux / macOS 脚本安装

```bash
curl -fsSL https://raw.githubusercontent.com/MCB-SMART-BOY/n3v3/master/scripts/install.sh | sh
```

### Windows Script / Windows 脚本安装

```powershell
irm https://raw.githubusercontent.com/MCB-SMART-BOY/n3v3/master/scripts/install.ps1 | iex
```

### Manual Release Install / 手动安装发行版

```bash
# Linux x86_64
curl -LO https://github.com/MCB-SMART-BOY/n3v3/releases/latest/download/n3v3-x86_64-unknown-linux-gnu.tar.gz
tar xzf n3v3-x86_64-unknown-linux-gnu.tar.gz
sudo install -m 755 n3v3 /usr/local/bin/n3v3

# Linux aarch64
curl -LO https://github.com/MCB-SMART-BOY/n3v3/releases/latest/download/n3v3-aarch64-unknown-linux-gnu.tar.gz

# macOS Intel
curl -LO https://github.com/MCB-SMART-BOY/n3v3/releases/latest/download/n3v3-x86_64-apple-darwin.tar.gz

# macOS Apple Silicon
curl -LO https://github.com/MCB-SMART-BOY/n3v3/releases/latest/download/n3v3-aarch64-apple-darwin.tar.gz
```

Windows release asset / Windows 发行文件：

```powershell
Invoke-WebRequest https://github.com/MCB-SMART-BOY/n3v3/releases/latest/download/n3v3-x86_64-pc-windows-msvc.zip -OutFile n3v3.zip
Expand-Archive n3v3.zip -DestinationPath $env:LOCALAPPDATA\n3v3\bin -Force
```

### Arch Linux / Arch Linux 用户

```bash
# AUR package: n3v3-bin
yay -S n3v3-bin
```

### Build from Source / 从源码编译

```bash
# Requires Rust 1.85+ / 需要 Rust 1.85 以上
git clone https://github.com/MCB-SMART-BOY/n3v3.git
cd n3v3

# Install into Cargo bin dir / 安装到 Cargo bin 目录
cargo install --path n3v3-cli --locked

# Or build a release binary / 或仅构建 release 二进制
cargo build --release
sudo install -m 755 target/release/n3v3 /usr/local/bin/n3v3
```

## Verify Installation / 验证安装

```bash
n3v3 --version          # prints: n3v3 5.0.1 / 输出：n3v3 5.0.1
n3v3 info --platform    # Check platform capabilities / 查看平台支持
n3v3 repl               # Start interactive REPL / 启动 REPL
n3v3 eval "1 + 2"       # Evaluate expression / 计算表达式
```

## Binary Cache & Signatures / 二进制缓存与签名

```bash
# One key for all caches / 单个密钥应用到全部缓存
n3v3 build ./pkg.n3v3 \
  --cache-url https://cache.example.org \
  --cache-dir /var/lib/n3v3/cache \
  --cache-public-key 'ed25519:<base64-public-key>' \
  --cache-private-key 'ed25519:<base64-private-key>' \
  --cache-upload
```

Environment variables / 环境变量：

- `N3V3_BINARY_CACHE_URLS` (comma-separated) / 远程缓存 URL（逗号分隔）
- `N3V3_BINARY_CACHE_LOCAL_DIRS` (comma-separated) / 本地缓存目录（逗号分隔）
- `N3V3_BINARY_CACHE_PUBLIC_KEYS` (comma-separated, fallback `N3V3_BINARY_CACHE_PUBLIC_KEY`)
- `N3V3_BINARY_CACHE_PRIVATE_KEYS` (comma-separated, fallback `N3V3_BINARY_CACHE_PRIVATE_KEY`)
- `N3V3_BINARY_CACHE_UPLOAD` (`true/false`)
- `N3V3_SUBSTITUTE` (`true/false`)

Key mapping rule / 密钥映射规则：

- `0` keys: disabled / 禁用
- `1` key: apply to all cache sources / 应用于所有缓存源
- `N` keys: must match total cache sources / 必须等于缓存源总数

Behavior notes / 行为说明：

- Push writes `narinfo` with `FileHash`, `NarHash`, and `References`.
- Reference discovery is best-effort by scanning store payloads for existing store path tokens.
- Fetching a cached path resolves `References` recursively before installing the target path.
- If a referenced path is missing in caches, fetch fails to avoid partial closure installs.
- Remote retry only applies to transient failures (`429`, `5xx`, timeout/connect errors).

## Troubleshooting / 常见问题

### "Command not found: n3v3" / 提示找不到命令

Make sure n3v3 is in your PATH:
确保 n3v3 在 PATH 中：

```bash
export PATH="$HOME/.local/bin:$PATH"  # Add to ~/.bashrc or ~/.zshrc
```

### Windows SmartScreen Warning / SmartScreen 弹窗

Click "More info" → "Run anyway"
点「更多信息」→「仍要运行」

### Linux Linking Errors / Linux 链接错误

```bash
sudo apt install build-essential  # Debian/Ubuntu
sudo dnf install gcc              # Fedora
sudo pacman -S base-devel         # Arch
```

### macOS Security Warning / macOS 安全警告

```bash
xattr -d com.apple.quarantine n3v3  # Remove quarantine flag
```

## What's Next / 接下来

- [Quick Start](quickstart.md) — 5 minutes to first code / 5 分钟写出第一段代码
- [Tutorial](tutorial.md) — Complete walkthrough / 从头到尾带你过一遍
- [Spec](../reference/spec.md) — Language reference / 语言规范

---

> Installed? Good. Now go hack. / 装好就开干吧。
