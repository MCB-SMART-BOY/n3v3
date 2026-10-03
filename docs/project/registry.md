<div align="center">

<img src="../../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>n3v3 Package Registry</h1>

<p><em>软件包注册表</em></p>

<p>
  <strong><a href="../../README.md">Home</a></strong> ·
  <strong><a href="../README.md">Docs</a></strong>
</p>

</div>

---

# n3v3 Package Registry

> *Local package discovery and optional binary caching for n3v3.*
> n3v3 本地软件包发现服务及可选二进制缓存。
This registry document describes the v5.0.2 implementation. The published CLI package is `n3v3`, and its binary is `n3v3`.
本文描述 v5.0.2 的注册表实现。已发布的 CLI 软件包名为 `n3v3`，其二进制文件名为 `n3v3`。

---

## Overview / 概述

The built-in registry implements a local v1 HTTP API with JSON metadata and read routes for NAR archives. `registry.n3v3.dev` is a **Planned** public deployment, not an available hosted service. The registry client supports index/search and version metadata resolution, not NAR download or remote installation.
内置注册表提供本地 v1 HTTP API、JSON 元数据及 NAR 归档读取路由。`registry.n3v3.dev` 的公开部署仍是 **Planned**，不是现有托管服务。注册表客户端支持索引、搜索及版本元数据查询，不会下载 NAR 或远程安装。

The local server/client are **Experimental** for deployment: `n3v3 registry-serve` binds only loopback and requires a token at startup; publishing requests require that token. Public hosting, TLS/auth gateway, production signing keys and operational policy remain **Planned**.
本地服务/客户端部署层面仍属 **Experimental**：`n3v3 registry-serve` 仅绑定 loopback，启动需令牌，发布请求必须携带令牌。公开托管、TLS/认证网关、生产签名密钥与运维策略仍为 **Planned**。

## Current State / 当前状态

### Server (`n3v3 registry-serve`)
- **Location**: `n3v3-cli/src/commands/registry_serve.rs`
- **Protocol**: HTTP v1 API with JSON responses
- **v1 routes**: `/v1/index.json`, `/v1/search`, `/v1/packages/<name>`, `/v1/packages/<name>/<version>`, `/v1/<hash>.narinfo`, `/v1/nar/<hash>.nar`; publishing uses `POST /v1/packages/<name>`
- **Data model**: Index entries with version lists, per-version metadata with nar_hash and file_hash
- **Exposure boundary**: built-in server only binds loopback addresses. Startup
  requires a non-empty `N3V3_REGISTRY_TOKEN` (visible ASCII, no commas, within one
  HTTP header line). `POST /v1/packages/<name>` requires exactly
  `Authorization: Bearer <token>`; reads remain unauthenticated. The legacy
  `POST /packages/<name>` route has the same requirement. No wildcard CORS
  headers are sent. This is a local development/test tool, not a public service.

### Client (registry client library)
- **Location**: `n3v3-cli/src/registry_client.rs`
- **Capabilities**: Package index fetching, search, package and version metadata resolution; no NAR download method
- **CLI integration**: `n3v3 search` queries v1 search; `n3v3 package install` queries metadata only if a package is absent locally, reports available versions and asks the user to fetch/add to the local store before installing. `n3v3 registry-update` separately fetches the index into a local file (`n3v3-cli/src/commands/registry.rs`).

### Binary Cache
- **Location**: `n3v3-cli/src/commands/build.rs`, `crates/n3v3-store/src/cache.rs`
- **Features**: Content-addressed NAR cache, optional narinfo signing/verification when keys are configured, multi-cache priority; no default signature requirement
- **CLI flags**: `--cache-url`, `--cache-dir`, `--cache-public-key`, `--cache-private-key`, `--no-substitute`, `--cache-upload`


## v1 API / v1 API

The canonical server routes are under `/v1/`; legacy `/packages.json` and `/packages/<name>` routes remain compatibility endpoints.
规范服务器路由位于 `/v1/` 下；旧版 `/packages.json` 与 `/packages/<name>` 路由仍作为兼容端点保留。

### GET `/v1/index.json`
Returns the full package index:
返回完整软件包索引：
```json
[
  {
    "name": "hello",
    "versions": ["1.0.0", "1.2.0"],
    "description": "A friendly greeting program"
  }
]
```

### GET `/v1/packages/<name>`
Returns metadata for all versions of one package:
返回一个软件包的全部版本元数据：
```json
{
  "name": "hello",
  "versions": [
    {
      "version": "1.0.0",
      "nar_hash": "sha256:abc123...",
      "file_hash": "sha256:def456...",
      "dependencies": {},
      "description": "Initial release"
    }
  ]
}
```

### GET `/v1/packages/<name>/<version>`
Returns metadata for one package version.
返回一个软件包版本的元数据。

### GET `/v1/search?q=<query>`
Search is case-insensitive across package names.
搜索按软件包名称进行大小写不敏感匹配。

### GET `/v1/<hash>.narinfo`
Returns cache metadata; a signature is present only when the cache is configured to sign narinfo.
返回缓存元数据；仅当缓存配置了 narinfo 签名时才带签名。

### GET `/v1/nar/<hash>.nar`

The server exposes a NAR archive by content hash; this read route is distinct from the registry client's metadata-only functionality.
服务端按内容哈希提供 NAR 归档读取路由；这与注册表客户端只查询元数据的功能不同。

### POST `/v1/packages/<name>`
Publishes version metadata for a package. Set the same `N3V3_REGISTRY_TOKEN` in
the local server and `n3v3 registry-publish` process; the publisher sends the
token in the `Authorization: Bearer <token>` header. The token is not printed.
发布软件包版本元数据。服务端和发布客户端必须设置相同的本地令牌。

## Public Launch Plan / 公开启动计划

### Experimental: Internal Validation (Current)

| Step | Status | Description |
|------|--------|-------------|
| Server implementation | Implemented | v1 API routes in `registry_serve.rs` |
| Client implementation | Implemented | Index fetch, search and version metadata resolution; remote package installation is not implemented |
| Binary cache | Implemented | Optional NAR signing/verification when keys are configured, plus multi-cache support |
| Local testing | Experimental | Loopback-only `n3v3 registry-serve` and `n3v3 registry-publish`; server startup requires a token and write routes require bearer authentication |
| Authentication and TLS termination | Planned | Local token does not replace a TLS/auth gateway or dedicated server for public access |
| Domain & hosting | Planned | `registry.n3v3.dev` setup |
| Signing key generation | Planned | Production ed25519 keys |
| Rate limiting | Planned | Per-IP throttling |
| Terms of service | Planned | Package submission policy |

### Planned: Public Beta

1. Implement or deploy authenticated publishing with TLS termination
2. Add bounded concurrency and rate limiting appropriate for public traffic
3. Deploy the dedicated service at `registry.n3v3.dev`
4. Publish the initial reviewed package set
5. Open package submission with a review process and monitor for 4-6 weeks

### Planned: General Availability

1. Remove beta label
2. Document package authoring guide
3. Establish community package maintenance process
4. Set up mirror infrastructure

## Security Model / 安全模型

- **NAR integrity**: Cache substitution checks applicable NAR/file hashes against cache metadata; a derivation output's store path is not proof that build bytes are reproducible
- **narinfo signing**: Ed25519 verification is applied when a cache public key is configured, not required by default
- **Substitution**: Users explicitly configure cache sources and trusted public keys; without a public key, cache metadata signatures are not enforced
- **Upload signing**: A configured private key signs narinfo during cache upload; uploads are optional
- **Server exposure**: cache signatures do not authenticate package-publish HTTP
  requests. The built-in server refuses non-loopback addresses and requires
  `N3V3_REGISTRY_TOKEN` for writes, but local users/processes that can read the
  token can publish. There is no HTTPS on the built-in server; public hosting
  still requires a TLS/auth gateway or dedicated server, policy, and operational
  controls. Do not expose the loopback server with a public port-forward.

## Configuration / 配置

For local publishing, set a private token in both process environments (avoid
committing it or placing it in shell history), start `n3v3 registry-serve`,
then run `n3v3 registry-publish <package-dir> --registry-url http://127.0.0.1:<port>`.
Use the server's actual port. The URL must be the registry base URL, without
`/v1`; `registry-publish` posts to `/v1/packages/<name>`. Reads do not need the
token. `package install` only queries the registry for version metadata when
a package is missing locally; it does not download or install a remote version.
Do not send this token to an untrusted URL; use a gateway with TLS and stronger
access controls for remote deployments.

```bash
# Point discovery at a registry you operate; set N3V3_REGISTRY to its base URL.
# The public domain is not yet deployed.
n3v3 search hello
n3v3 package install hello  # local store only; missing local package lists remote versions
n3v3 registry-update

# With cache URL and keys supplied via environment variables, a build may
# substitute from that cache and optionally upload signed metadata.
n3v3 build --cache-url "$CACHE_URL" \
           --cache-public-key "$CACHE_PUBLIC_KEY" \
           --cache-upload \
           --cache-private-key "$CACHE_PRIVATE_KEY"
```

## Related Files / 相关文件

| File | Purpose |
|------|---------|
| `n3v3-cli/src/registry_client.rs` | HTTP client for v1 API |
| `n3v3-cli/src/commands/registry_serve.rs` | Registry server (local dev) |
| `n3v3-cli/src/commands/registry_publish.rs` | Package publisher |
| `n3v3-cli/src/commands/registry.rs` | `n3v3 registry-update` command |
| `n3v3-cli/src/commands/search.rs` | `n3v3 search` command |
| `n3v3-cli/src/commands/install.rs` | `n3v3 package install` command |
| `n3v3-cli/src/commands/build.rs` | Binary cache integration |
| `crates/n3v3-store/src/cache.rs` | Content-addressed cache |
| `crates/n3v3-store/src/nar.rs` | NAR archive format |
