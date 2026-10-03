//! Binary cache for pre-built derivations.
//! 预构建推导的二进制缓存。
//!
//! The binary cache allows sharing pre-built store paths between machines,
//! avoiding the need to rebuild packages from source.
//! 二进制缓存允许在机器之间共享预构建的存储路径，
//! 避免从源码重新构建包。

use crate::nar::{self, NarError};
use crate::{Database, Store, StoreError};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use n3v3_derive::{Derivation, Hash, StorePath};
use reqwest::Url;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

const MAX_COMPRESSED_NAR_SIZE: u64 = 256 * 1024 * 1024;
const MAX_NARINFO_SIZE: u64 = 1024 * 1024;
const MAX_NAR_SIZE: u64 = 512 * 1024 * 1024;
const REMOTE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_CACHE_REDIRECTS: usize = 10;

fn ensure_size_within_limit(size: u64, limit: u64, field: &str) -> Result<(), CacheError> {
    if size > limit {
        return Err(CacheError::InvalidManifest(format!(
            "{field} {size} exceeds cache limit {limit}"
        )));
    }
    Ok(())
}

fn read_bounded_nar<R: Read>(reader: R, limit: u64, field: &str) -> Result<Vec<u8>, CacheError> {
    let mut output = Vec::new();
    reader.take(limit + 1).read_to_end(&mut output)?;
    ensure_size_within_limit(output.len() as u64, limit, field)?;
    Ok(output)
}

struct BoundedNarWriter {
    output: Vec<u8>,
    limit: u64,
    is_over_limit: bool,
}

impl Write for BoundedNarWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if (bytes.len() as u64) > self.limit - self.output.len() as u64 {
            self.is_over_limit = true;
            return Err(std::io::Error::other(
                "decompressed NAR exceeds cache limit",
            ));
        }
        self.output.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Create a placeholder derivation for cached paths.
/// 为缓存路径创建占位推导。
fn placeholder_derivation(name: &str) -> Derivation {
    Derivation {
        name: name.to_string(),
        version: "0.0.0".to_string(),
        system: "unknown".to_string(),
        builder: "/bin/sh".to_string(),
        args: Vec::new(),
        env: BTreeMap::new(),
        input_drvs: BTreeMap::new(),
        input_srcs: Vec::new(),
        outputs: BTreeMap::new(),
    }
}

/// Errors that can occur during cache operations.
/// 缓存操作期间可能发生的错误。
#[derive(Debug, Error)]
pub enum CacheError {
    /// Store error. / 存储错误。
    #[error("store error: {0}")]
    Store(#[from] StoreError),

    /// Fetch error. / 获取错误。
    #[error("fetch error: {0}")]
    Fetch(String),

    /// I/O error. / I/O 错误。
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error. / 序列化错误。
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Compression error. / 压缩错误。
    #[error("compression error: {0}")]
    Compression(String),

    /// NAR error. / NAR 错误。
    #[error("NAR error: {0}")]
    Nar(#[from] NarError),

    /// Cache not found. / 未找到缓存。
    #[error("cache not found: {0}")]
    NotFound(String),

    /// Invalid cache manifest. / 无效的缓存清单。
    #[error("invalid cache manifest: {0}")]
    InvalidManifest(String),

    /// Hash mismatch during substitute verification. / 替换验证期间哈希不匹配。
    #[error("hash mismatch for {kind}: expected {expected}, got {actual}")]
    HashMismatch {
        kind: &'static str,
        expected: String,
        actual: String,
    },

    /// Signature verification failure. / 签名验证失败。
    #[error("signature verification failed: {0}")]
    Signature(String),

    /// Atomically publishing a verified substitute failed. / 原子发布替换结果失败。
    #[error("failed to publish verified substitute at {path:?}: {source}")]
    Publish {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Failed to discard an invalid cached download. / 清理无效缓存下载失败。
    #[error(
        "failed to discard invalid cached download at {path:?}: {source}; original error: {original}"
    )]
    Cleanup {
        path: PathBuf,
        source: std::io::Error,
        #[source]
        original: Box<CacheError>,
    },
}

struct StagedPath {
    _directory: tempfile::TempDir,
    path: PathBuf,
    nar_hash: Hash,
}

/// A cached store path with metadata.
/// 带有元数据的缓存存储路径。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPath {
    /// The store path. / 存储路径。
    pub path: StorePath,

    /// The derivation that produced this path. / 产生此路径的推导。
    pub derivation: Derivation,

    /// References to other store paths. / 对其他存储路径的引用。
    pub references: Vec<StorePath>,

    /// Size in bytes (uncompressed). / 大小（字节，未压缩）。
    pub size: u64,

    /// Declared compressed file size, absent in legacy JSON manifests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,

    /// Compression format. / 压缩格式。
    pub compression: CompressionFormat,

    /// Download URL (for remote caches). / 下载 URL（用于远程缓存）。
    pub url: Option<String>,

    /// Expected hash of compressed NAR payload (optional). / 压缩 NAR 载荷的预期哈希（可选）。
    pub file_hash: Option<String>,

    /// Expected hash of decompressed NAR bytes (optional). / 解压后 NAR 字节的预期哈希（可选）。
    pub nar_hash: Option<String>,
}

/// Compression formats supported by the cache.
/// 缓存支持的压缩格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionFormat {
    /// No compression. / 无压缩。
    None,
    /// gzip compression. / gzip 压缩。
    Gzip,
    /// xz compression (LZMA). / xz 压缩 (LZMA)。
    Xz,
    /// zstd compression. / zstd 压缩。
    Zstd,
}

impl CompressionFormat {
    /// Get file extension for this compression format.
    /// 获取此压缩格式的文件扩展名。
    pub fn extension(&self) -> &'static str {
        match self {
            CompressionFormat::None => ".nar",
            CompressionFormat::Gzip => ".nar.gz",
            CompressionFormat::Xz => ".nar.xz",
            CompressionFormat::Zstd => ".nar.zst",
        }
    }

    /// Parse compression name from narinfo.
    /// 从 narinfo 解析压缩格式名称。
    fn from_narinfo(value: &str) -> Option<Self> {
        match value {
            "none" => Some(CompressionFormat::None),
            "gzip" => Some(CompressionFormat::Gzip),
            "xz" => Some(CompressionFormat::Xz),
            "zstd" => Some(CompressionFormat::Zstd),
            _ => None,
        }
    }

    /// Render compression name for narinfo.
    /// 渲染用于 narinfo 的压缩格式名称。
    fn as_narinfo(&self) -> &'static str {
        match self {
            CompressionFormat::None => "none",
            CompressionFormat::Gzip => "gzip",
            CompressionFormat::Xz => "xz",
            CompressionFormat::Zstd => "zstd",
        }
    }
}

/// Parsed narinfo metadata for substituter protocol.
/// substituter 协议的 narinfo 元数据。
#[derive(Debug, Clone)]
struct NarInfo {
    store_path: StorePath,
    url: String,
    compression: CompressionFormat,
    file_size: u64,
    nar_size: Option<u64>,
    references: Vec<StorePath>,
    nar_hash: Option<String>,
    file_hash: Option<String>,
    signature: Option<String>,
}

impl NarInfo {
    fn parse(content: &str) -> Result<Self, CacheError> {
        let mut store_path = None;
        let mut url = None;
        let mut compression = CompressionFormat::Xz;
        let mut file_size = None;
        let mut nar_size = None;
        let mut references = Vec::new();
        let mut nar_hash = None;
        let mut file_hash = None;
        let mut signature = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some((key, value)) = line.split_once(':') else {
                return Err(CacheError::InvalidManifest(format!(
                    "invalid narinfo line: {}",
                    line
                )));
            };
            let key = key.trim();
            let value = value.trim();

            match key {
                "StorePath" => {
                    store_path = parse_store_path_token(value, "StorePath")?;
                }
                "URL" => {
                    if value.is_empty() {
                        return Err(CacheError::InvalidManifest(
                            "narinfo URL cannot be empty".to_string(),
                        ));
                    }
                    url = Some(value.to_string());
                }
                "Compression" => {
                    compression = CompressionFormat::from_narinfo(value).ok_or_else(|| {
                        CacheError::InvalidManifest(format!(
                            "unsupported narinfo compression '{}'",
                            value
                        ))
                    })?;
                }
                "FileSize" => {
                    file_size = Some(value.parse::<u64>().map_err(|_| {
                        CacheError::InvalidManifest(format!("invalid FileSize '{}'", value))
                    })?);
                }
                "NarSize" => {
                    nar_size = Some(value.parse::<u64>().map_err(|_| {
                        CacheError::InvalidManifest(format!("invalid NarSize '{}'", value))
                    })?);
                }
                "References" => {
                    references = value
                        .split_whitespace()
                        .map(|token| {
                            parse_store_path_token(token, "References").and_then(|entry| {
                                entry.ok_or_else(|| {
                                    CacheError::InvalidManifest(format!(
                                        "invalid reference store path '{}'",
                                        token
                                    ))
                                })
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                }
                "NarHash" if !value.is_empty() => {
                    nar_hash = Some(value.to_string());
                }
                "FileHash" if !value.is_empty() => {
                    file_hash = Some(value.to_string());
                }
                "Sig" if !value.is_empty() => {
                    signature = Some(value.to_string());
                }
                _ => {}
            }
        }

        Ok(Self {
            store_path: store_path.ok_or_else(|| {
                CacheError::InvalidManifest("narinfo missing StorePath".to_string())
            })?,
            url: url
                .ok_or_else(|| CacheError::InvalidManifest("narinfo missing URL".to_string()))?,
            compression,
            file_size: file_size.ok_or_else(|| {
                CacheError::InvalidManifest("narinfo missing FileSize".to_string())
            })?,
            nar_size,
            references,
            nar_hash,
            file_hash,
            signature,
        })
    }

    fn to_unsigned_text(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("StorePath: {}", self.store_path.display_name()));
        lines.push(format!("URL: {}", self.url));
        lines.push(format!("Compression: {}", self.compression.as_narinfo()));
        lines.push(format!("FileSize: {}", self.file_size));
        if let Some(file_hash) = &self.file_hash {
            lines.push(format!("FileHash: {}", file_hash));
        }
        if let Some(nar_hash) = &self.nar_hash {
            lines.push(format!("NarHash: {}", nar_hash));
        }
        if let Some(nar_size) = self.nar_size {
            lines.push(format!("NarSize: {}", nar_size));
        }
        if !self.references.is_empty() {
            let refs = self
                .references
                .iter()
                .map(StorePath::display_name)
                .collect::<Vec<_>>()
                .join(" ");
            lines.push(format!("References: {}", refs));
        }
        lines.push(String::new());
        lines.join("\n")
    }

    fn to_text(&self) -> String {
        let mut lines = self
            .to_unsigned_text()
            .lines()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if lines.last().is_some_and(|line| line.is_empty()) {
            lines.pop();
        }
        if let Some(signature) = &self.signature {
            lines.push(format!("Sig: {}", signature));
        }
        lines.push(String::new());
        lines.join("\n")
    }
}

#[derive(Debug, Clone, Copy)]
enum NarInfoSource<'a> {
    Local(&'a Path),
    Remote(&'a str),
}

fn parse_store_path_token(token: &str, field: &str) -> Result<Option<StorePath>, CacheError> {
    if token.is_empty() {
        return Ok(None);
    }

    if let Some(store_path) = StorePath::parse(Path::new(token)) {
        return Ok(Some(store_path));
    }

    if let Some(store_path) = StorePath::parse_name(token) {
        return Ok(Some(store_path));
    }

    Err(CacheError::InvalidManifest(format!(
        "invalid {} store path '{}'",
        field, token
    )))
}

fn is_absolute_cache_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://")
}

fn parse_remote_http_url(url: &str) -> Result<Url, CacheError> {
    let parsed = Url::parse(url).map_err(|err| {
        CacheError::InvalidManifest(format!("invalid remote cache URL '{url}': {err}"))
    })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(CacheError::InvalidManifest(format!(
            "remote cache URL '{url}' must be HTTP(S) without credentials"
        )));
    }
    Ok(parsed)
}

fn remote_cache_client(url: &Url) -> Result<Client, CacheError> {
    let origin = url.origin();
    Client::builder()
        .timeout(REMOTE_DOWNLOAD_TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.url().origin() != origin {
                return attempt.error("remote cache redirect changed origin");
            }
            if attempt.previous().len() >= MAX_CACHE_REDIRECTS {
                return attempt.error("remote cache redirect limit exceeded");
            }
            attempt.follow()
        }))
        .build()
        .map_err(|err| CacheError::Fetch(format!("failed to create client for {url}: {err}")))
}

fn resolve_narinfo_url(url: &str, source: NarInfoSource<'_>) -> Result<String, CacheError> {
    match source {
        NarInfoSource::Local(base) => {
            if is_absolute_cache_url(url) {
                Ok(url.to_string())
            } else {
                Ok(base.join(url).to_string_lossy().to_string())
            }
        }
        NarInfoSource::Remote(base) => {
            let base_url = parse_remote_http_url(base)?;
            if url.starts_with('/') {
                return Err(CacheError::InvalidManifest(format!(
                    "narinfo URL '{url}' escapes configured remote cache origin '{base}'"
                )));
            }
            let base_with_slash =
                parse_remote_http_url(&format!("{}/", base.trim_end_matches('/')))?;
            let resolved = base_with_slash.join(url).map_err(|err| {
                CacheError::InvalidManifest(format!(
                    "invalid narinfo URL '{url}' from '{base}': {err}"
                ))
            })?;
            if !matches!(resolved.scheme(), "http" | "https")
                || resolved.origin() != base_url.origin()
                || !resolved.username().is_empty()
                || resolved.password().is_some()
            {
                return Err(CacheError::InvalidManifest(format!(
                    "narinfo URL '{url}' escapes configured remote cache origin '{base}'"
                )));
            }
            Ok(resolved.to_string())
        }
    }
}

fn parse_cache_hash(input: Option<&str>) -> Result<Option<Hash>, CacheError> {
    let Some(raw) = input else {
        return Ok(None);
    };

    let normalized = raw
        .trim()
        .strip_prefix("blake3:")
        .or_else(|| raw.trim().strip_prefix("blake3-"))
        .unwrap_or(raw.trim());

    if normalized.is_empty() {
        return Ok(None);
    }

    Hash::from_hex(normalized).map(Some).map_err(|_| {
        CacheError::InvalidManifest(format!("invalid blake3 hash format '{}'", raw.trim()))
    })
}

fn format_hash(hash: &Hash) -> String {
    format!("blake3:{}", hash.to_hex())
}

const REMOTE_RETRY_ATTEMPTS: usize = 3;
const REMOTE_RETRY_BASE_DELAY_MS: u64 = 150;

fn remote_retry_delay(attempt: usize) -> Duration {
    Duration::from_millis(REMOTE_RETRY_BASE_DELAY_MS * (attempt as u64 + 1))
}

fn should_retry_http_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error() || status.as_u16() == 429
}

fn should_retry_reqwest_error(error: &reqwest::Error) -> bool {
    if let Some(status) = error.status() {
        return should_retry_http_status(status);
    }
    error.is_timeout() || error.is_connect()
}

fn decode_prefixed_base64<const N: usize>(
    input: &str,
    prefix: &str,
    field_name: &str,
) -> Result<[u8; N], CacheError> {
    let trimmed = input.trim();
    let encoded = trimmed.strip_prefix(prefix).ok_or_else(|| {
        CacheError::Signature(format!(
            "{} must start with '{}' (got '{}')",
            field_name, prefix, trimmed
        ))
    })?;

    let decoded = BASE64_STANDARD
        .decode(encoded)
        .map_err(|e| CacheError::Signature(format!("{} is not valid base64: {}", field_name, e)))?;
    decoded.try_into().map_err(|_| {
        CacheError::Signature(format!(
            "{} decoded length mismatch: expected {} bytes",
            field_name, N
        ))
    })
}

fn parse_ed25519_public_key(value: &str) -> Result<VerifyingKey, CacheError> {
    let key_bytes = decode_prefixed_base64::<32>(value, "ed25519:", "cache public key")?;
    VerifyingKey::from_bytes(&key_bytes).map_err(|e| {
        CacheError::Signature(format!("cache public key is invalid ed25519 bytes: {}", e))
    })
}

fn parse_ed25519_private_key(value: &str) -> Result<SigningKey, CacheError> {
    let key_bytes = decode_prefixed_base64::<32>(value, "ed25519:", "cache private key")?;
    Ok(SigningKey::from_bytes(&key_bytes))
}

fn parse_ed25519_signature(value: &str) -> Result<Signature, CacheError> {
    let sig_bytes = decode_prefixed_base64::<64>(value, "ed25519:", "narinfo Sig")?;
    Ok(Signature::from_bytes(&sig_bytes))
}

fn verify_narinfo_signature(narinfo: &NarInfo, public_key: &str) -> Result<(), CacheError> {
    let signature = narinfo.signature.as_deref().ok_or_else(|| {
        CacheError::Signature("narinfo is missing Sig but cache requires signatures".to_string())
    })?;
    let verifying_key = parse_ed25519_public_key(public_key)?;
    let signature = parse_ed25519_signature(signature)?;

    let payload = narinfo.to_unsigned_text();
    verifying_key
        .verify(payload.as_bytes(), &signature)
        .map_err(|_| {
            CacheError::Signature(format!(
                "narinfo Sig validation failed for {}",
                narinfo.store_path.display_name()
            ))
        })
}

/// Configuration for a binary cache.
/// 二进制缓存的配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Cache name. / 缓存名称。
    pub name: String,

    /// Base URL for remote cache. / 远程缓存的基础 URL。
    pub url: Option<String>,

    /// Local directory for cache storage. / 缓存存储的本地目录。
    pub local_dir: Option<PathBuf>,

    /// Public key for signature verification. / 用于签名验证的公钥。
    pub public_key: Option<String>,

    /// Private key for narinfo signing on upload. / 上传 narinfo 时使用的私钥。
    pub private_key: Option<String>,

    /// Priority (higher = preferred). / 优先级（越高越优先）。
    pub priority: i32,

    /// Whether to use this cache for uploads. / 是否使用此缓存进行上传。
    pub upload: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            url: None,
            local_dir: None,
            public_key: None,
            private_key: None,
            priority: 50,
            upload: false,
        }
    }
}

/// Binary cache manager.
/// 二进制缓存管理器。
pub struct BinaryCache {
    /// The local store. / 本地存储。
    store: Store,

    /// Configured caches (sorted by priority). / 配置的缓存（按优先级排序）。
    caches: Vec<CacheConfig>,

    /// Local cache directory for downloads. / 下载的本地缓存目录。
    cache_dir: PathBuf,
}

impl BinaryCache {
    /// Create a new binary cache manager.
    /// 创建新的二进制缓存管理器。
    pub fn new(store: Store) -> Result<Self, CacheError> {
        let cache_dir = store.root().join("cache");
        fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            store,
            caches: Vec::new(),
            cache_dir,
        })
    }

    /// Add a cache configuration.
    /// 添加缓存配置。
    pub fn add_cache(&mut self, config: CacheConfig) {
        self.caches.push(config);
        // Sort by priority (descending)
        // 按优先级排序（降序）
        self.caches
            .sort_by_key(|entry| std::cmp::Reverse(entry.priority));
    }

    fn fetch_text_with_retry(&self, url: &str) -> Result<String, CacheError> {
        let parsed = parse_remote_http_url(url)?;
        let client = remote_cache_client(&parsed)?;
        for attempt in 0..REMOTE_RETRY_ATTEMPTS {
            let response = match client.get(parsed.clone()).send() {
                Ok(response) => response,
                Err(err)
                    if should_retry_reqwest_error(&err) && attempt + 1 < REMOTE_RETRY_ATTEMPTS =>
                {
                    std::thread::sleep(remote_retry_delay(attempt));
                    continue;
                }
                Err(err) => {
                    return Err(CacheError::Fetch(format!(
                        "failed to fetch remote narinfo {url}: {err}"
                    )));
                }
            };
            let status = response.status();
            if status == reqwest::StatusCode::NOT_FOUND {
                return Err(CacheError::NotFound(url.to_string()));
            }
            if should_retry_http_status(status) && attempt + 1 < REMOTE_RETRY_ATTEMPTS {
                std::thread::sleep(remote_retry_delay(attempt));
                continue;
            }
            if !status.is_success() {
                return Err(CacheError::Fetch(format!(
                    "failed to fetch remote narinfo {url}: HTTP {status}"
                )));
            }
            let size_context = format!("narinfo size at {url}");
            if let Some(length) = response.content_length() {
                ensure_size_within_limit(length, MAX_NARINFO_SIZE, &size_context)?;
            }
            let mut content = Vec::new();
            response
                .take(MAX_NARINFO_SIZE + 1)
                .read_to_end(&mut content)
                .map_err(|err| {
                    CacheError::Fetch(format!("failed to read remote narinfo {url}: {err}"))
                })?;
            ensure_size_within_limit(content.len() as u64, MAX_NARINFO_SIZE, &size_context)?;
            return String::from_utf8(content).map_err(|err| {
                CacheError::InvalidManifest(format!("invalid UTF-8 in narinfo at {url}: {err}"))
            });
        }
        Err(CacheError::Fetch(format!(
            "failed to fetch remote narinfo {url} after {REMOTE_RETRY_ATTEMPTS} attempts"
        )))
    }

    fn fetch_file_with_retry(&self, url: &str, dest: &Path) -> Result<(), CacheError> {
        let parsed = parse_remote_http_url(url)?;
        let client = remote_cache_client(&parsed)?;
        for attempt in 0..REMOTE_RETRY_ATTEMPTS {
            let response = match client.get(parsed.clone()).send() {
                Ok(response) => response,
                Err(err)
                    if should_retry_reqwest_error(&err) && attempt + 1 < REMOTE_RETRY_ATTEMPTS =>
                {
                    std::thread::sleep(remote_retry_delay(attempt));
                    continue;
                }
                Err(err) => {
                    return Err(CacheError::Fetch(format!(
                        "failed to download {url}: {err}"
                    )));
                }
            };
            let status = response.status();
            if status == reqwest::StatusCode::NOT_FOUND {
                return Err(CacheError::NotFound(url.to_string()));
            }
            if should_retry_http_status(status) && attempt + 1 < REMOTE_RETRY_ATTEMPTS {
                std::thread::sleep(remote_retry_delay(attempt));
                continue;
            }
            if !status.is_success() {
                return Err(CacheError::Fetch(format!(
                    "failed to download {url}: HTTP {status}"
                )));
            }
            if let Some(length) = response.content_length() {
                ensure_size_within_limit(length, MAX_COMPRESSED_NAR_SIZE, "compressed FileSize")?;
            }
            let mut staged = tempfile::NamedTempFile::new_in(&self.cache_dir)?;
            let copied =
                std::io::copy(&mut response.take(MAX_COMPRESSED_NAR_SIZE + 1), &mut staged)?;
            ensure_size_within_limit(copied, MAX_COMPRESSED_NAR_SIZE, "compressed file size")?;
            staged
                .persist(dest)
                .map_err(|err| CacheError::Io(err.error))?;
            return Ok(());
        }
        Err(CacheError::Fetch(format!(
            "failed to download {url} after {REMOTE_RETRY_ATTEMPTS} attempts"
        )))
    }

    /// Query if a path is available in any cache.
    /// 查询路径是否在任何缓存中可用。
    pub fn query(&self, path: &StorePath) -> Result<Option<CachedPath>, CacheError> {
        let mut deferred_fetch_error = None;

        // Check each cache in priority order
        // 按优先级顺序检查每个缓存
        for cache in &self.caches {
            match self.query_cache(cache, path) {
                Ok(Some(cached)) => return Ok(Some(cached)),
                Ok(None) => {}
                Err(CacheError::Fetch(message)) => {
                    deferred_fetch_error = Some(CacheError::Fetch(message));
                }
                Err(err) => return Err(err),
            }
        }

        if let Some(err) = deferred_fetch_error {
            return Err(err);
        }

        Ok(None)
    }

    /// Query a specific cache for a path.
    /// 在特定缓存中查询路径。
    fn query_cache(
        &self,
        cache: &CacheConfig,
        path: &StorePath,
    ) -> Result<Option<CachedPath>, CacheError> {
        // Try local cache first
        // 首先尝试本地缓存
        if let Some(local_dir) = &cache.local_dir {
            let narinfo_path = local_dir.join(format!("{}.narinfo", path.hash()));
            if narinfo_path.exists() {
                let narinfo = fs::read_to_string(&narinfo_path)?;
                let cached = self.parse_narinfo(
                    &narinfo,
                    path,
                    NarInfoSource::Local(local_dir),
                    cache.public_key.as_deref(),
                )?;
                return Ok(Some(cached));
            }

            if let Some(cached) = Self::query_json_manifest(cache, path, local_dir)? {
                return Ok(Some(cached));
            }
        }

        // Try remote cache
        // 尝试远程缓存
        if let Some(url) = &cache.url {
            let manifest_url = format!("{}/{}.narinfo", url, path.hash());
            match self.fetch_text_with_retry(&manifest_url) {
                Ok(content) => {
                    let cached = self.parse_narinfo(
                        &content,
                        path,
                        NarInfoSource::Remote(url),
                        cache.public_key.as_deref(),
                    )?;
                    return Ok(Some(cached));
                }
                Err(CacheError::NotFound(_)) => {}
                Err(err) => return Err(err),
            }
        }

        Ok(None)
    }

    fn query_json_manifest(
        cache: &CacheConfig,
        path: &StorePath,
        local_dir: &Path,
    ) -> Result<Option<CachedPath>, CacheError> {
        let manifest_path = local_dir.join(format!("{}.json", path.hash()));
        if !manifest_path.exists() {
            return Ok(None);
        }
        if cache.public_key.is_some() {
            return Err(CacheError::Signature(format!(
                "signed cache {} has only unsigned JSON metadata for {}",
                cache.name,
                path.display_name()
            )));
        }
        let manifest = fs::read_to_string(&manifest_path)?;
        let mut cached: CachedPath = serde_json::from_str(&manifest)?;
        if cached.path != *path {
            return Err(CacheError::InvalidManifest(format!(
                "JSON path mismatch: expected '{}', got '{}'",
                path.display_name(),
                cached.path.display_name()
            )));
        }
        Self::validate_cached_sizes(&cached)?;
        if cached.url.is_none() {
            let nar_path =
                local_dir.join(format!("{}{}", path.hash(), cached.compression.extension()));
            cached.url = Some(nar_path.to_string_lossy().to_string());
        }
        Ok(Some(cached))
    }

    /// Download and install a cached path.
    /// 下载并安装缓存的路径。
    pub fn fetch(&mut self, cached: &CachedPath) -> Result<(), CacheError> {
        let mut visiting = HashSet::new();
        let mut db = Database::open(self.store.root().to_path_buf())?;
        self.fetch_with_references(cached, &mut visiting, &mut db)
    }

    fn fetch_with_references(
        &mut self,
        cached: &CachedPath,
        visiting: &mut HashSet<StorePath>,
        db: &mut Database,
    ) -> Result<(), CacheError> {
        if !visiting.insert(cached.path.clone()) {
            // Cyclic references in metadata should not cause infinite recursion.
            // 元数据中的循环引用不应导致无限递归。
            return Ok(());
        }

        let result = (|| -> Result<(), CacheError> {
            self.validate_store_path(&cached.path)?;
            Self::validate_cached_sizes(cached)?;
            for reference in &cached.references {
                self.validate_store_path(reference)?;
                if self.store.path_exists(reference) {
                    self.backfill_existing_path_metadata(reference, visiting, db)?;
                    continue;
                }

                let reference_cached = self.query(reference)?.ok_or_else(|| {
                    CacheError::NotFound(format!(
                        "missing referenced path {} required by {}",
                        reference.display_name(),
                        cached.path.display_name()
                    ))
                })?;
                self.fetch_with_references(&reference_cached, visiting, db)?;
            }

            if self.store.path_exists(&cached.path) {
                return self.register_fetched_path_info(db, cached, None);
            }

            let nar_file = self.download_nar(cached)?;
            let staged = match self
                .verify_downloaded_file_hash(cached, &nar_file)
                .and_then(|()| self.stage_and_verify_nar(cached, &nar_file))
            {
                Ok(staged) => staged,
                Err(original) => {
                    if let Err(source) = fs::remove_file(&nar_file) {
                        return Err(CacheError::Cleanup {
                            path: nar_file,
                            source,
                            original: Box::new(original),
                        });
                    }
                    return Err(original);
                }
            };
            self.publish_staged_path(&staged.path, &cached.path)?;
            self.register_fetched_path_info(db, cached, Some(staged.nar_hash))
        })();

        visiting.remove(&cached.path);
        result
    }

    fn backfill_existing_path_metadata(
        &mut self,
        path: &StorePath,
        visiting: &mut HashSet<StorePath>,
        db: &mut Database,
    ) -> Result<(), CacheError> {
        if db.query(path)?.is_some() {
            return Ok(());
        }

        // An unavailable cache is recoverable only when local scanning supplies
        // the complete reference set; scanning errors propagate without registering.
        match self.query(path) {
            Ok(Some(cached)) => self.fetch_with_references(&cached, visiting, db),
            Ok(None) | Err(_) => {
                let info = self.store.scan_path_metadata(path)?;
                db.register(info)?;
                Ok(())
            }
        }
    }

    fn register_fetched_path_info(
        &self,
        db: &mut Database,
        cached: &CachedPath,
        extracted_nar_hash: Option<Hash>,
    ) -> Result<(), CacheError> {
        let nar_hash = if let Some(hash) = extracted_nar_hash {
            hash
        } else {
            parse_cache_hash(cached.nar_hash.as_deref())?.unwrap_or(*cached.path.hash())
        };
        let previous_references = db
            .query(&cached.path)?
            .map(|info| info.references)
            .unwrap_or_default();

        let mut info = self.store.scan_path_metadata(&cached.path)?;
        info.nar_hash = nar_hash;
        info.nar_size = cached.size;
        for reference in &cached.references {
            if reference != &cached.path {
                info.add_reference(reference.clone());
            }
        }
        for reference in previous_references {
            info.add_reference(reference);
        }
        db.register(info)?;
        Ok(())
    }

    fn validate_cached_sizes(cached: &CachedPath) -> Result<(), CacheError> {
        ensure_size_within_limit(cached.size, MAX_NAR_SIZE, "NarSize")?;
        if let Some(file_size) = cached.file_size {
            ensure_size_within_limit(file_size, MAX_COMPRESSED_NAR_SIZE, "FileSize")?;
        }
        Ok(())
    }

    /// Verify the compressed file size and optional hash before decompression.
    fn verify_downloaded_file_hash(
        &self,
        cached: &CachedPath,
        nar_file: &Path,
    ) -> Result<(), CacheError> {
        let actual_size = fs::metadata(nar_file)?.len();
        ensure_size_within_limit(actual_size, MAX_COMPRESSED_NAR_SIZE, "compressed file size")?;
        if let Some(expected_size) = cached.file_size
            && actual_size != expected_size
        {
            return Err(CacheError::InvalidManifest(format!(
                "compressed FileSize mismatch for {}: expected {}, got {}",
                cached.path.display_name(),
                expected_size,
                actual_size
            )));
        }
        let Some(expected) = parse_cache_hash(cached.file_hash.as_deref())? else {
            return Ok(());
        };
        let content = read_bounded_nar(
            fs::File::open(nar_file)?,
            MAX_COMPRESSED_NAR_SIZE,
            "compressed file size",
        )?;
        let actual = Hash::of(&content);
        if actual != expected {
            return Err(CacheError::HashMismatch {
                kind: "nar-compressed",
                expected: format_hash(&expected),
                actual: format_hash(&actual),
            });
        }
        Ok(())
    }

    /// Verify decompressed NAR hash if metadata provides it.
    /// 若元数据提供 NAR 哈希则校验解压结果。
    fn verify_extracted_nar_hash(
        &self,
        cached: &CachedPath,
        actual_hash: &Hash,
    ) -> Result<(), CacheError> {
        let Some(expected) = parse_cache_hash(cached.nar_hash.as_deref())? else {
            return Ok(());
        };

        if *actual_hash != expected {
            return Err(CacheError::HashMismatch {
                kind: "nar",
                expected: format_hash(&expected),
                actual: format_hash(actual_hash),
            });
        }

        Ok(())
    }

    /// Download a NAR file from cache.
    /// 从缓存下载 NAR 文件。
    fn download_nar(&self, cached: &CachedPath) -> Result<PathBuf, CacheError> {
        let url = cached
            .url
            .as_ref()
            .ok_or_else(|| CacheError::NotFound("No download URL".to_string()))?;

        let filename = format!("{}{}", cached.path.hash(), cached.compression.extension());
        let dest = self.cache_dir.join(&filename);

        if !dest.exists() {
            if let Some(local_path) = url.strip_prefix("file://") {
                Self::copy_local_nar(Path::new(local_path), &dest)?;
            } else {
                let local_path = Path::new(url);
                if local_path.is_absolute() && local_path.exists() {
                    Self::copy_local_nar(local_path, &dest)?;
                } else {
                    self.fetch_file_with_retry(url, &dest)?;
                }
            }
        }

        Ok(dest)
    }

    /// Copy a local NAR file into the binary cache download directory.
    /// 将本地 NAR 文件复制到二进制缓存下载目录。
    fn copy_local_nar(source: &Path, dest: &Path) -> Result<(), CacheError> {
        if !source.exists() {
            return Err(CacheError::NotFound(source.to_string_lossy().to_string()));
        }
        ensure_size_within_limit(
            fs::metadata(source)?.len(),
            MAX_COMPRESSED_NAR_SIZE,
            "compressed file size",
        )?;
        let mut staged = tempfile::NamedTempFile::new_in(dest.parent().unwrap_or(Path::new(".")))?;
        let copied = std::io::copy(
            &mut fs::File::open(source)?.take(MAX_COMPRESSED_NAR_SIZE + 1),
            &mut staged,
        )?;
        ensure_size_within_limit(copied, MAX_COMPRESSED_NAR_SIZE, "compressed file size")?;
        staged
            .persist(dest)
            .map_err(|err| CacheError::Io(err.error))?;
        Ok(())
    }

    /// Extract and verify a NAR in a private directory under the store root.
    fn stage_and_verify_nar(
        &self,
        cached: &CachedPath,
        nar_file: &Path,
    ) -> Result<StagedPath, CacheError> {
        let compressed_data = read_bounded_nar(
            fs::File::open(nar_file)?,
            MAX_COMPRESSED_NAR_SIZE,
            "compressed file size",
        )?;
        let nar_data = self.decompress_nar(&compressed_data, cached.compression, cached.size)?;
        let nar_hash = Hash::of(&nar_data);
        self.verify_extracted_nar_hash(cached, &nar_hash)?;
        if nar_data.len() as u64 != cached.size {
            return Err(CacheError::InvalidManifest(format!(
                "decompressed NAR size mismatch for {}: expected {}, got {}",
                cached.path.display_name(),
                cached.size,
                nar_data.len()
            )));
        }

        let directory = tempfile::Builder::new()
            .prefix(".n3v3-substitute-")
            .tempdir_in(self.store.root())?;
        let path = directory.path().join("result");
        nar::extract_nar(&nar_data, &path)?;
        self.verify_store_path_hash_compatibility(&cached.path, &path)?;
        Ok(StagedPath {
            _directory: directory,
            path,
            nar_hash,
        })
    }

    fn publish_staged_path(
        &self,
        staged_path: &Path,
        store_path: &StorePath,
    ) -> Result<(), CacheError> {
        let destination = self.store.to_path(store_path);
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        {
            use nix::fcntl::{RenameFlags, renameat2};
            match renameat2(
                None,
                staged_path,
                None,
                &destination,
                RenameFlags::RENAME_NOREPLACE,
            ) {
                Ok(()) => Ok(()),
                Err(nix::errno::Errno::EEXIST) => Err(CacheError::InvalidManifest(format!(
                    "store destination already exists: {}",
                    destination.display()
                ))),
                Err(errno) => Err(CacheError::Publish {
                    path: destination,
                    source: std::io::Error::from_raw_os_error(errno as i32),
                }),
            }
        }
        #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
        {
            let _ = staged_path;
            Err(CacheError::InvalidManifest(format!(
                "atomic no-replace publication is unavailable for {} on this platform",
                destination.display()
            )))
        }
    }

    fn validate_store_path(&self, path: &StorePath) -> Result<(), CacheError> {
        let name = path.name();
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains('\0') {
            return Err(CacheError::InvalidManifest(format!(
                "unsafe store path name: {name:?}"
            )));
        }
        Ok(())
    }

    /// Decompress NAR data with a hard output bound, including for streaming xz.
    fn decompress_nar(
        &self,
        data: &[u8],
        compression: CompressionFormat,
        declared_size: u64,
    ) -> Result<Vec<u8>, CacheError> {
        let limit = declared_size.min(MAX_NAR_SIZE);
        match compression {
            CompressionFormat::None => {
                ensure_size_within_limit(data.len() as u64, limit, "decompressed NAR size")?;
                Ok(data.to_vec())
            }
            CompressionFormat::Gzip => read_bounded_nar(
                flate2::read::GzDecoder::new(data),
                limit,
                "decompressed NAR size",
            ),
            CompressionFormat::Xz => {
                let mut writer = BoundedNarWriter {
                    output: Vec::new(),
                    limit,
                    is_over_limit: false,
                };
                let result = lzma_rs::xz_decompress(&mut std::io::Cursor::new(data), &mut writer);
                if writer.is_over_limit {
                    return Err(CacheError::InvalidManifest(format!(
                        "decompressed NAR size exceeds cache limit {limit}"
                    )));
                }
                result.map_err(|e| {
                    CacheError::Compression(format!("xz decompression failed: {e}"))
                })?;
                Ok(writer.output)
            }
            CompressionFormat::Zstd => {
                let decoder = zstd::Decoder::new(data)
                    .map_err(|e| CacheError::Compression(format!("zstd decoder failed: {e}")))?;
                read_bounded_nar(decoder, limit, "decompressed NAR size")
            }
        }
    }

    /// Compute the hash of a store path using NAR format.
    /// 使用 NAR 格式计算存储路径的哈希。
    fn compute_path_hash(&self, path: &Path) -> Result<Hash, CacheError> {
        // Hash the path using NAR format for deterministic results
        // 使用 NAR 格式哈希路径以获得确定性结果
        let hash = nar::hash_path(path)?;
        Ok(hash)
    }

    /// Compute the hash of a store path using store-native hashing rules.
    /// 使用 store 原生规则计算存储路径哈希。
    fn compute_store_native_hash(&self, path: &Path) -> Result<Hash, CacheError> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(path)?;
            return Ok(Hash::of(target.as_os_str().as_encoded_bytes()));
        }
        if metadata.is_file() {
            let content = fs::read(path)?;
            return Ok(Hash::of(&content));
        }
        if metadata.is_dir() {
            let mut hasher = n3v3_derive::Hasher::new();
            Self::hash_dir_native(path, &mut hasher)?;
            return Ok(hasher.finalize());
        }
        Ok(Hash::of(b""))
    }

    fn hash_dir_native(path: &Path, hasher: &mut n3v3_derive::Hasher) -> Result<(), CacheError> {
        let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, std::io::Error>>()?;
        entries.sort_by_key(|entry| entry.file_name());

        hasher.update_byte(b'D');
        hasher.update_u64(entries.len() as u64);
        for entry in entries {
            let name = entry.file_name();
            hasher.update_byte(b'E');
            hasher.update_len_prefixed(name.as_os_str().as_encoded_bytes());

            let child = entry.path();
            let metadata = fs::symlink_metadata(&child)?;
            if metadata.file_type().is_symlink() {
                hasher.update_byte(b'L');
                let target = fs::read_link(&child)?;
                hasher.update_len_prefixed(target.as_os_str().as_encoded_bytes());
            } else if metadata.is_file() {
                hasher.update_byte(b'F');
                let content = fs::read(&child)?;
                hasher.update_len_prefixed(&content);
            } else if metadata.is_dir() {
                Self::hash_dir_native(&child, hasher)?;
            } else {
                hasher.update_byte(b'U');
            }
        }
        Ok(())
    }

    /// Accept both historical hash styles for compatibility:
    /// - NAR hash of extracted path.
    /// - Native store hash used by add_file/add_content/add_dir.
    ///
    /// 兼容两类历史哈希风格：解包路径的 NAR 哈希，以及
    /// add_file/add_content/add_dir 使用的 store 原生哈希。
    fn matches_expected_store_hash(
        &self,
        expected_path: &StorePath,
        extracted_path: &Path,
    ) -> Result<bool, CacheError> {
        let expected = *expected_path.hash();
        if self.compute_path_hash(extracted_path)? == expected {
            return Ok(true);
        }
        Ok(self.compute_store_native_hash(extracted_path)? == expected)
    }

    fn verify_store_path_hash_compatibility(
        &self,
        expected_path: &StorePath,
        extracted_path: &Path,
    ) -> Result<(), CacheError> {
        if self.matches_expected_store_hash(expected_path, extracted_path)? {
            return Ok(());
        }

        let actual = self.compute_path_hash(extracted_path)?;
        Err(StoreError::HashMismatch {
            expected: *expected_path.hash(),
            actual,
        }
        .into())
    }

    /// Parse a .narinfo file.
    /// 解析 .narinfo 文件。
    fn parse_narinfo(
        &self,
        content: &str,
        expected_path: &StorePath,
        source: NarInfoSource<'_>,
        required_public_key: Option<&str>,
    ) -> Result<CachedPath, CacheError> {
        let narinfo = NarInfo::parse(content)?;
        if narinfo.store_path != *expected_path {
            return Err(CacheError::InvalidManifest(format!(
                "narinfo StorePath mismatch: expected '{}', got '{}'",
                expected_path.display_name(),
                narinfo.store_path.display_name()
            )));
        }
        if let Some(public_key) = required_public_key {
            verify_narinfo_signature(&narinfo, public_key)?;
        }

        let file_hash = narinfo.file_hash.clone();
        let nar_hash = narinfo.nar_hash.clone();
        // Validate hash format eagerly to fail fast on malformed narinfo metadata.
        // 预先校验哈希格式，尽早拒绝损坏的 narinfo 元数据。
        parse_cache_hash(file_hash.as_deref())?;
        parse_cache_hash(nar_hash.as_deref())?;
        ensure_size_within_limit(narinfo.file_size, MAX_COMPRESSED_NAR_SIZE, "FileSize")?;
        ensure_size_within_limit(
            narinfo.nar_size.unwrap_or(narinfo.file_size),
            MAX_NAR_SIZE,
            "NarSize",
        )?;

        let resolved_url = resolve_narinfo_url(&narinfo.url, source)?;

        Ok(CachedPath {
            path: narinfo.store_path.clone(),
            derivation: placeholder_derivation(&narinfo.store_path.display_name()),
            references: narinfo.references.clone(),
            size: narinfo.nar_size.unwrap_or(narinfo.file_size),
            file_size: Some(narinfo.file_size),
            compression: narinfo.compression,
            url: Some(resolved_url),
            file_hash,
            nar_hash,
        })
    }

    fn maybe_sign_narinfo(
        &self,
        cache: &CacheConfig,
        narinfo: &mut NarInfo,
    ) -> Result<(), CacheError> {
        let private_key = match cache.private_key.as_deref() {
            Some(private_key) => private_key,
            None => {
                if cache.public_key.is_some() {
                    return Err(CacheError::Signature(
                        "cache has public_key but no private_key for narinfo signing".to_string(),
                    ));
                }
                return Ok(());
            }
        };

        let signing_key = parse_ed25519_private_key(private_key)?;
        let signature = signing_key.sign(narinfo.to_unsigned_text().as_bytes());
        narinfo.signature = Some(format!(
            "ed25519:{}",
            BASE64_STANDARD.encode(signature.to_bytes())
        ));
        Ok(())
    }

    /// Upload a store path to all writable caches.
    /// 将存储路径上传到所有可写缓存。
    pub fn push(&self, path: &StorePath) -> Result<(), CacheError> {
        let store_path = self.store.to_path(path);
        if !store_path.exists() {
            return Err(CacheError::NotFound(path.to_string()));
        }

        for cache in &self.caches {
            if cache.upload {
                self.push_to_cache(cache, path)?;
            }
        }

        Ok(())
    }

    /// Upload a path to a specific cache.
    /// 将路径上传到特定缓存。
    fn push_to_cache(&self, cache: &CacheConfig, path: &StorePath) -> Result<(), CacheError> {
        // Create NAR archive
        // 创建 NAR 归档
        let (nar_file, nar_size, nar_hash) = self.create_nar(path)?;
        let file_hash = Hash::of(&fs::read(&nar_file)?);
        let file_size = fs::metadata(&nar_file)?.len();
        let references = self.discover_references(path)?;

        // Write manifest
        // 写入清单
        let cached = CachedPath {
            path: path.clone(),
            derivation: placeholder_derivation(&path.to_string()),
            references: references.clone(),
            size: nar_size,
            file_size: Some(file_size),
            compression: CompressionFormat::Xz,
            url: None,
            file_hash: Some(format_hash(&file_hash)),
            nar_hash: Some(format_hash(&nar_hash)),
        };

        // Upload to local cache
        // 上传到本地缓存
        if let Some(local_dir) = &cache.local_dir {
            fs::create_dir_all(local_dir)?;

            let nar_filename = format!("{}{}", path.hash(), CompressionFormat::Xz.extension());
            let nar_dest = local_dir.join(&nar_filename);
            fs::copy(&nar_file, &nar_dest)?;

            let mut narinfo = NarInfo {
                store_path: path.clone(),
                url: nar_filename,
                compression: CompressionFormat::Xz,
                file_size,
                nar_size: Some(nar_size),
                references: references.clone(),
                nar_hash: Some(format_hash(&nar_hash)),
                file_hash: Some(format_hash(&file_hash)),
                signature: None,
            };
            self.maybe_sign_narinfo(cache, &mut narinfo)?;
            fs::write(
                local_dir.join(format!("{}.narinfo", path.hash())),
                narinfo.to_text(),
            )?;

            let manifest_path = local_dir.join(format!("{}.json", path.hash()));
            let mut cached = cached.clone();
            cached.url = Some(nar_dest.to_string_lossy().to_string());
            let manifest = serde_json::to_string_pretty(&cached)?;
            fs::write(manifest_path, manifest)?;
        }

        if let Some(remote_url) = &cache.url {
            let mut narinfo = NarInfo {
                store_path: path.clone(),
                url: format!("{}{}", path.hash(), CompressionFormat::Xz.extension()),
                compression: CompressionFormat::Xz,
                file_size,
                nar_size: Some(nar_size),
                references: references.clone(),
                nar_hash: Some(format_hash(&nar_hash)),
                file_hash: Some(format_hash(&file_hash)),
                signature: None,
            };
            self.maybe_sign_narinfo(cache, &mut narinfo)?;
            self.push_to_remote_cache(remote_url, path, &nar_file, &narinfo.to_text())?;
        }

        // Remote upload uses a minimal HTTP PUT protocol:
        //   <base>/<hash>.nar.xz and <base>/<hash>.narinfo
        // 远程上传使用最小 HTTP PUT 协议：
        //   <base>/<hash>.nar.xz 与 <base>/<hash>.narinfo

        Ok(())
    }

    fn discover_references(&self, path: &StorePath) -> Result<Vec<StorePath>, CacheError> {
        if let Some(references) = self.try_references_from_database(path) {
            return Ok(references);
        }

        let store_path = self.store.to_path(path);
        let mut references = HashSet::new();
        self.discover_references_in_path(&store_path, path, &mut references)?;
        let mut references = references.into_iter().collect::<Vec<_>>();
        references.sort();
        Ok(references)
    }

    fn try_references_from_database(&self, path: &StorePath) -> Option<Vec<StorePath>> {
        let mut db = Database::open(self.store.root().to_path_buf()).ok()?;
        let info = db.query(path).ok()??;
        if info.references.is_empty() {
            return None;
        }

        let mut references = info
            .references
            .into_iter()
            .filter(|reference| reference != path && self.store.path_exists(reference))
            .collect::<Vec<_>>();
        references.sort();
        Some(references)
    }

    fn discover_references_in_path(
        &self,
        fs_path: &Path,
        current: &StorePath,
        references: &mut HashSet<StorePath>,
    ) -> Result<(), CacheError> {
        let metadata = fs::symlink_metadata(fs_path)?;

        if metadata.file_type().is_symlink() {
            let target = fs::read_link(fs_path)?;
            self.extract_references_from_bytes(
                target.to_string_lossy().as_bytes(),
                current,
                references,
            );
            return Ok(());
        }

        if metadata.is_dir() {
            for entry in fs::read_dir(fs_path)? {
                let entry = entry?;
                self.discover_references_in_path(&entry.path(), current, references)?;
            }
            return Ok(());
        }

        if metadata.is_file() {
            let content = fs::read(fs_path)?;
            self.extract_references_from_bytes(&content, current, references);
        }

        Ok(())
    }

    fn extract_references_from_bytes(
        &self,
        content: &[u8],
        current: &StorePath,
        references: &mut HashSet<StorePath>,
    ) {
        let text = String::from_utf8_lossy(content);
        for token in text
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '+')))
        {
            if token.len() <= 65 {
                continue;
            }

            let Some(candidate) = StorePath::parse_name(token) else {
                continue;
            };
            if candidate == *current {
                continue;
            }
            if self.store.path_exists(&candidate) {
                references.insert(candidate);
            }
        }
    }

    /// Upload NAR payload and narinfo metadata to a remote cache via HTTP PUT.
    /// 通过 HTTP PUT 上传 NAR 载荷与 narinfo 元数据到远程缓存。
    fn push_to_remote_cache(
        &self,
        base_url: &str,
        path: &StorePath,
        nar_file: &Path,
        narinfo_text: &str,
    ) -> Result<(), CacheError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| CacheError::Fetch(format!("failed to build HTTP client: {}", e)))?;

        let nar_name = format!("{}{}", path.hash(), CompressionFormat::Xz.extension());
        let nar_url = format!("{}/{}", base_url.trim_end_matches('/'), nar_name);
        let nar_bytes = fs::read(nar_file)?;
        Self::put_with_retry(
            &client,
            &nar_url,
            "application/x-nix-nar",
            &nar_bytes,
            "NAR",
        )?;

        let narinfo_url = format!("{}/{}.narinfo", base_url.trim_end_matches('/'), path.hash());
        Self::put_with_retry(
            &client,
            &narinfo_url,
            "text/x-nix-narinfo",
            narinfo_text.as_bytes(),
            "narinfo",
        )?;

        Ok(())
    }

    fn put_with_retry(
        client: &Client,
        url: &str,
        content_type: &str,
        body: &[u8],
        label: &str,
    ) -> Result<(), CacheError> {
        for attempt in 0..REMOTE_RETRY_ATTEMPTS {
            match client
                .put(url)
                .header("content-type", content_type)
                .body(body.to_vec())
                .send()
            {
                Ok(resp) if resp.status().is_success() => return Ok(()),
                Ok(resp) => {
                    let status = resp.status();
                    if should_retry_http_status(status) && attempt + 1 < REMOTE_RETRY_ATTEMPTS {
                        std::thread::sleep(remote_retry_delay(attempt));
                        continue;
                    }
                    return Err(CacheError::Fetch(format!(
                        "remote cache rejected {} upload: {} {}",
                        label, url, status
                    )));
                }
                Err(err) => {
                    if should_retry_reqwest_error(&err) && attempt + 1 < REMOTE_RETRY_ATTEMPTS {
                        std::thread::sleep(remote_retry_delay(attempt));
                        continue;
                    }
                    return Err(CacheError::Fetch(format!(
                        "failed to upload {}: {}",
                        label, err
                    )));
                }
            }
        }

        Err(CacheError::Fetch(format!(
            "failed to upload {} after {} attempts",
            label, REMOTE_RETRY_ATTEMPTS
        )))
    }

    /// Create a NAR archive of a store path.
    /// 创建存储路径的 NAR 归档。
    fn create_nar(&self, path: &StorePath) -> Result<(PathBuf, u64, Hash), CacheError> {
        let store_path = self.store.to_path(path);
        let nar_file = self.cache_dir.join(format!("{}.nar.xz", path.hash()));

        // Create NAR archive using our implementation
        // 使用我们的实现创建 NAR 归档
        let nar_data = nar::create_nar(&store_path)?;
        let nar_size = nar_data.len() as u64;
        let nar_hash = Hash::of(&nar_data);

        // Compress with xz
        // 使用 xz 压缩
        let compressed = self.compress_nar(&nar_data, CompressionFormat::Xz)?;

        // Write to file
        // 写入文件
        fs::write(&nar_file, compressed)?;

        Ok((nar_file, nar_size, nar_hash))
    }

    /// Compress NAR data with the specified format.
    /// 使用指定格式压缩 NAR 数据。
    fn compress_nar(&self, data: &[u8], format: CompressionFormat) -> Result<Vec<u8>, CacheError> {
        match format {
            CompressionFormat::None => Ok(data.to_vec()),
            CompressionFormat::Gzip => {
                let mut encoder =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(data).map_err(|e| {
                    CacheError::Compression(format!("gzip compression failed: {}", e))
                })?;
                encoder
                    .finish()
                    .map_err(|e| CacheError::Compression(format!("gzip finish failed: {}", e)))
            }
            CompressionFormat::Xz => {
                let mut compressed = Vec::new();
                lzma_rs::xz_compress(&mut std::io::Cursor::new(data), &mut compressed).map_err(
                    |e| CacheError::Compression(format!("xz compression failed: {}", e)),
                )?;
                Ok(compressed)
            }
            CompressionFormat::Zstd => zstd::encode_all(std::io::Cursor::new(data), 3)
                .map_err(|e| CacheError::Compression(format!("zstd compression failed: {}", e))),
        }
    }

    /// Get cache statistics.
    /// 获取缓存统计信息。
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            total_caches: self.caches.len(),
            cache_dir_size: Self::dir_size(&self.cache_dir).unwrap_or(0),
        }
    }

    /// Calculate directory size.
    /// 计算目录大小。
    fn dir_size(path: &Path) -> Result<u64, std::io::Error> {
        let mut total = 0;
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                if metadata.is_file() {
                    total += metadata.len();
                } else if metadata.is_dir() {
                    total += Self::dir_size(&entry.path())?;
                }
            }
        }
        Ok(total)
    }
}

/// Cache statistics.
/// 缓存统计信息。
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of configured caches. / 配置的缓存数量。
    pub total_caches: usize,

    /// Size of local cache directory in bytes. / 本地缓存目录的大小（字节）。
    pub cache_dir_size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PathInfo;
    use ed25519_dalek::{Signer, SigningKey};
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    fn deterministic_signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn cache_public_key(signing_key: &SigningKey) -> String {
        format!(
            "ed25519:{}",
            BASE64_STANDARD.encode(signing_key.verifying_key().to_bytes())
        )
    }

    fn cache_private_key(signing_key: &SigningKey) -> String {
        format!("ed25519:{}", BASE64_STANDARD.encode(signing_key.to_bytes()))
    }

    fn unsigned_narinfo(path: &StorePath) -> NarInfo {
        NarInfo {
            store_path: path.clone(),
            url: format!("{}.nar.xz", path.hash()),
            compression: CompressionFormat::Xz,
            file_size: 42,
            nar_size: Some(100),
            references: Vec::new(),
            nar_hash: None,
            file_hash: None,
            signature: None,
        }
    }

    fn signed_narinfo_text(path: &StorePath, signing_key: &SigningKey) -> String {
        let mut narinfo = unsigned_narinfo(path);
        let payload = narinfo.to_unsigned_text();
        let signature = signing_key.sign(payload.as_bytes());
        narinfo.signature = Some(format!(
            "ed25519:{}",
            BASE64_STANDARD.encode(signature.to_bytes())
        ));
        narinfo.to_text()
    }

    fn stage_store_file(
        store: &Store,
        temp_root: &std::path::Path,
        source_name: &str,
        content: &[u8],
        store_name: &str,
    ) -> StorePath {
        let source = temp_root.join(source_name);
        fs::write(&source, content).unwrap();
        let nar_hash = nar::hash_path(&source).unwrap();
        let store_path = StorePath::new(nar_hash, store_name.to_string());
        let upload_path = store.to_path(&store_path);
        if let Some(parent) = upload_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::copy(&source, &upload_path).unwrap();
        store_path
    }

    struct TestHttpCacheServer {
        base_url: String,
        storage: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        fail_puts: Arc<Mutex<HashMap<String, usize>>>,
        fail_gets: Arc<Mutex<HashMap<String, usize>>>,
        request_counts: Arc<Mutex<HashMap<String, usize>>>,
        redirects: Arc<Mutex<HashMap<String, String>>>,
        no_length: Arc<Mutex<HashSet<String>>>,
        stop: Arc<AtomicBool>,
        handle: Option<JoinHandle<()>>,
    }

    impl TestHttpCacheServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let addr = listener.local_addr().unwrap();

            let storage = Arc::new(Mutex::new(HashMap::new()));
            let fail_puts = Arc::new(Mutex::new(HashMap::new()));
            let fail_gets = Arc::new(Mutex::new(HashMap::new()));
            let request_counts = Arc::new(Mutex::new(HashMap::new()));
            let redirects = Arc::new(Mutex::new(HashMap::new()));
            let no_length = Arc::new(Mutex::new(HashSet::new()));
            let stop = Arc::new(AtomicBool::new(false));

            let storage_worker = Arc::clone(&storage);
            let fail_puts_worker = Arc::clone(&fail_puts);
            let fail_gets_worker = Arc::clone(&fail_gets);
            let request_counts_worker = Arc::clone(&request_counts);
            let redirects_worker = Arc::clone(&redirects);
            let no_length_worker = Arc::clone(&no_length);
            let stop_worker = Arc::clone(&stop);
            let handle = thread::spawn(move || {
                while !stop_worker.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let _ = handle_http_connection(
                                &mut stream,
                                &storage_worker,
                                &fail_puts_worker,
                                &fail_gets_worker,
                                &request_counts_worker,
                                &redirects_worker,
                                &no_length_worker,
                            );
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            });

            Self {
                base_url: format!("http://{}", addr),
                storage,
                fail_puts,
                fail_gets,
                request_counts,
                redirects,
                no_length,
                stop,
                handle: Some(handle),
            }
        }

        fn read_path(&self, path: &str) -> Option<Vec<u8>> {
            self.storage
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(path)
                .cloned()
        }

        fn fail_next_put(&self, path: &str, count: usize) {
            self.fail_puts
                .lock()
                .unwrap()
                .insert(path.to_string(), count);
        }

        fn fail_next_get(&self, path: &str, count: usize) {
            self.fail_gets
                .lock()
                .unwrap()
                .insert(path.to_string(), count);
        }

        fn request_count(&self, method: &str, path: &str) -> usize {
            let key = format!("{} {}", method, path);
            self.request_counts
                .lock()
                .unwrap()
                .get(&key)
                .copied()
                .unwrap_or(0)
        }

        fn write_path(&self, path: &str, content: &[u8]) {
            self.storage
                .lock()
                .unwrap()
                .insert(path.to_string(), content.to_vec());
        }

        fn redirect_path(&self, path: &str, location: &str) {
            self.redirects
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(path.to_string(), location.to_string());
        }

        fn serve_without_length(&self, path: &str) {
            self.no_length
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(path.to_string());
        }
    }

    impl Drop for TestHttpCacheServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn find_header_end(buf: &[u8]) -> Option<usize> {
        buf.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn parse_content_length(headers: &str) -> usize {
        headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.trim().eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }

    fn write_http_response(
        stream: &mut std::net::TcpStream,
        status: &str,
        content_type: &str,
        body: &[u8],
    ) -> std::io::Result<()> {
        let headers = format!(
            "HTTP/1.1 {}\r\nContent-Length: {}\r\nContent-Type: {}\r\nConnection: close\r\n\r\n",
            status,
            body.len(),
            content_type
        );
        stream.write_all(headers.as_bytes())?;
        stream.write_all(body)?;
        stream.flush()?;
        Ok(())
    }

    fn handle_http_connection(
        stream: &mut std::net::TcpStream,
        storage: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
        fail_puts: &Arc<Mutex<HashMap<String, usize>>>,
        fail_gets: &Arc<Mutex<HashMap<String, usize>>>,
        request_counts: &Arc<Mutex<HashMap<String, usize>>>,
        redirects: &Arc<Mutex<HashMap<String, String>>>,
        no_length: &Arc<Mutex<HashSet<String>>>,
    ) -> std::io::Result<()> {
        // The listener is non-blocking and accepted streams inherit that mode,
        // so a request body that arrives in several segments would make `read`
        // fail with `WouldBlock` and close the connection mid-request.
        // listener 是非阻塞的，accept 到的连接会继承该模式，因此分多段到达的请求体会让
        // `read` 返回 `WouldBlock` 并在请求中途关闭连接。
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;

        let mut request = Vec::new();
        let mut buf = [0u8; 1024];
        let mut header_end = None;

        while header_end.is_none() {
            let n = stream.read(&mut buf)?;
            if n == 0 {
                break;
            }
            request.extend_from_slice(&buf[..n]);
            header_end = find_header_end(&request);
        }

        let Some(header_end_idx) = header_end else {
            return write_http_response(stream, "400 Bad Request", "text/plain", b"bad request");
        };
        let body_start = header_end_idx + 4;
        let header_text = String::from_utf8_lossy(&request[..header_end_idx]);
        let mut header_lines = header_text.lines();
        let request_line = header_lines.next().unwrap_or_default();
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default();
        let path = parts.next().unwrap_or("/");
        let content_length = parse_content_length(&header_text);
        let request_key = format!("{} {}", method, path);
        *request_counts
            .lock()
            .unwrap()
            .entry(request_key)
            .or_insert(0) += 1;

        let mut body = request[body_start..].to_vec();
        while body.len() < content_length {
            let n = stream.read(&mut buf)?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&buf[..n]);
        }
        if body.len() > content_length {
            body.truncate(content_length);
        }

        match method {
            "PUT" => {
                if let Some(remaining) = fail_puts
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get_mut(path)
                    && *remaining > 0
                {
                    *remaining -= 1;
                    return write_http_response(
                        stream,
                        "503 Service Unavailable",
                        "text/plain",
                        b"retry later",
                    );
                }
                storage
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(path.to_string(), body);
                write_http_response(stream, "200 OK", "text/plain", b"ok")
            }
            "GET" => {
                if let Some(remaining) = fail_gets
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get_mut(path)
                    && *remaining > 0
                {
                    *remaining -= 1;
                    return write_http_response(
                        stream,
                        "503 Service Unavailable",
                        "text/plain",
                        b"retry later",
                    );
                }
                if let Some(location) = redirects
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .get(path)
                {
                    let headers = format!(
                        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                    return stream.write_all(headers.as_bytes());
                }
                let payload = storage
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(path)
                    .cloned();
                match payload {
                    Some(payload) => {
                        let content_type = if path.ends_with(".narinfo") {
                            "text/x-nix-narinfo"
                        } else {
                            "application/octet-stream"
                        };
                        if no_length
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                            .contains(path)
                        {
                            stream.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")?;
                            stream.write_all(&payload)
                        } else {
                            write_http_response(stream, "200 OK", content_type, &payload)
                        }
                    }
                    None => write_http_response(stream, "404 Not Found", "text/plain", b"missing"),
                }
            }
            _ => write_http_response(
                stream,
                "405 Method Not Allowed",
                "text/plain",
                b"method not allowed",
            ),
        }
    }

    #[test]
    fn test_compression_format_extension() {
        assert_eq!(CompressionFormat::None.extension(), ".nar");
        assert_eq!(CompressionFormat::Gzip.extension(), ".nar.gz");
        assert_eq!(CompressionFormat::Xz.extension(), ".nar.xz");
        assert_eq!(CompressionFormat::Zstd.extension(), ".nar.zst");
    }

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert_eq!(config.name, "default");
        assert_eq!(config.priority, 50);
        assert!(!config.upload);
    }

    #[test]
    fn test_cache_priority_sorting() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = Store::open_at(temp.path().to_path_buf()).unwrap();
        let mut cache = BinaryCache::new(store).unwrap();

        cache.add_cache(CacheConfig {
            name: "low".to_string(),
            priority: 10,
            ..Default::default()
        });

        cache.add_cache(CacheConfig {
            name: "high".to_string(),
            priority: 100,
            ..Default::default()
        });

        cache.add_cache(CacheConfig {
            name: "medium".to_string(),
            priority: 50,
            ..Default::default()
        });

        // Should be sorted by descending priority
        // 应按优先级降序排序
        assert_eq!(cache.caches[0].name, "high");
        assert_eq!(cache.caches[1].name, "medium");
        assert_eq!(cache.caches[2].name, "low");
    }

    #[test]
    fn test_parse_narinfo_remote_resolves_relative_url() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let cache = BinaryCache::new(store).unwrap();

        let path = StorePath::new(Hash::of(b"narinfo"), "pkg-1.0".to_string());
        let content = format!(
            "StorePath: {}\nURL: nar/{}.nar.xz\nCompression: xz\nFileSize: 42\nNarSize: 100\n",
            path.display_name(),
            path.hash()
        );

        let parsed = cache
            .parse_narinfo(
                &content,
                &path,
                NarInfoSource::Remote("https://cache.example"),
                None,
            )
            .unwrap();
        let expected = format!("https://cache.example/nar/{}.nar.xz", path.hash());

        assert_eq!(parsed.url.as_deref(), Some(expected.as_str()));
        assert_eq!(parsed.size, 100);
        assert_eq!(parsed.file_size, Some(42));
        assert_eq!(parsed.compression, CompressionFormat::Xz);
    }

    #[test]
    fn test_parse_cache_hash_accepts_prefixes_and_raw() {
        let hash = Hash::of(b"payload");
        let raw = hash.to_hex();

        let parsed_raw = parse_cache_hash(Some(&raw)).unwrap().unwrap();
        assert_eq!(parsed_raw, hash);

        let parsed_colon = parse_cache_hash(Some(&format!("blake3:{}", raw)))
            .unwrap()
            .unwrap();
        assert_eq!(parsed_colon, hash);

        let parsed_dash = parse_cache_hash(Some(&format!("blake3-{}", raw)))
            .unwrap()
            .unwrap();
        assert_eq!(parsed_dash, hash);
    }

    #[test]
    fn test_parse_cache_hash_rejects_invalid_format() {
        let err = parse_cache_hash(Some("blake3:not-a-hex")).unwrap_err();
        assert!(matches!(err, CacheError::InvalidManifest(_)));
    }

    #[test]
    fn test_parse_narinfo_rejects_store_path_mismatch() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let cache = BinaryCache::new(store).unwrap();

        let expected = StorePath::new(Hash::of(b"expected"), "pkg-1.0".to_string());
        let other = StorePath::new(Hash::of(b"other"), "pkg-1.0".to_string());
        let content = format!(
            "StorePath: {}\nURL: {}.nar.xz\nCompression: xz\nFileSize: 42\n",
            other.display_name(),
            other.hash()
        );

        let err = cache
            .parse_narinfo(
                &content,
                &expected,
                NarInfoSource::Remote("https://cache.example"),
                None,
            )
            .unwrap_err();

        assert!(matches!(err, CacheError::InvalidManifest(_)));
    }

    #[test]
    fn query_signed_cache_with_only_json_rejects_unsigned_downgrade() {
        let temp = tempfile::TempDir::new().unwrap();
        let local = temp.path().join("cache");
        fs::create_dir(&local).unwrap();
        let path = StorePath::new(Hash::of(b"signed-json"), "package".to_string());
        fs::write(local.join(format!("{}.json", path.hash())), b"{}").unwrap();
        let mut cache =
            BinaryCache::new(Store::open_at(temp.path().join("store")).unwrap()).unwrap();
        cache.add_cache(CacheConfig {
            local_dir: Some(local),
            public_key: Some(cache_public_key(&deterministic_signing_key(17))),
            ..Default::default()
        });
        assert!(matches!(cache.query(&path), Err(CacheError::Signature(_))));
    }

    #[test]
    fn query_json_with_different_requested_path_rejects_manifest() {
        let temp = tempfile::TempDir::new().unwrap();
        let local = temp.path().join("cache");
        fs::create_dir(&local).unwrap();
        let requested = StorePath::new(Hash::of(b"requested"), "package".to_string());
        let other = StorePath::new(Hash::of(b"other"), "package".to_string());
        let cached = CachedPath {
            path: other,
            derivation: placeholder_derivation("package"),
            references: Vec::new(),
            size: 0,
            file_size: None,
            compression: CompressionFormat::None,
            url: None,
            file_hash: None,
            nar_hash: None,
        };
        fs::write(
            local.join(format!("{}.json", requested.hash())),
            serde_json::to_vec(&cached).unwrap(),
        )
        .unwrap();
        let mut cache =
            BinaryCache::new(Store::open_at(temp.path().join("store")).unwrap()).unwrap();
        cache.add_cache(CacheConfig {
            local_dir: Some(local),
            ..Default::default()
        });
        assert!(
            matches!(cache.query(&requested), Err(CacheError::InvalidManifest(message)) if message.contains("JSON path mismatch"))
        );
    }

    #[test]
    fn query_legacy_json_without_file_size_keeps_local_cache_compatible() {
        let temp = tempfile::TempDir::new().unwrap();
        let local = temp.path().join("cache");
        fs::create_dir(&local).unwrap();
        let path = StorePath::new(Hash::of(b"legacy"), "package".to_string());
        let metadata = CachedPath {
            path: path.clone(),
            derivation: placeholder_derivation("package"),
            references: Vec::new(),
            size: 12,
            file_size: None,
            compression: CompressionFormat::None,
            url: None,
            file_hash: None,
            nar_hash: None,
        };
        let mut json = serde_json::to_value(&metadata).unwrap();
        json.as_object_mut().unwrap().remove("file_size");
        fs::write(
            local.join(format!("{}.json", path.hash())),
            serde_json::to_vec(&json).unwrap(),
        )
        .unwrap();
        let mut cache =
            BinaryCache::new(Store::open_at(temp.path().join("store")).unwrap()).unwrap();
        cache.add_cache(CacheConfig {
            local_dir: Some(local),
            ..Default::default()
        });
        let queried = cache.query(&path).unwrap().unwrap();
        assert_eq!(queried.path, path);
        assert_eq!(queried.file_size, None);
    }

    #[test]
    fn parse_narinfo_oversized_declared_sizes_reject_before_download() {
        let temp = tempfile::TempDir::new().unwrap();
        let cache = BinaryCache::new(Store::open_at(temp.path().join("store")).unwrap()).unwrap();
        let path = StorePath::new(Hash::of(b"oversize"), "package".to_string());
        for (file_size, nar_size, rejected_field) in [
            (MAX_COMPRESSED_NAR_SIZE + 1, 1, "FileSize"),
            (1, MAX_NAR_SIZE + 1, "NarSize"),
        ] {
            let content = format!(
                "StorePath: {}\nURL: archive.nar.xz\nFileSize: {file_size}\nNarSize: {nar_size}\n",
                path.display_name()
            );
            assert!(matches!(
                cache.parse_narinfo(&content, &path, NarInfoSource::Remote("https://cache.example"), None),
                Err(CacheError::InvalidManifest(message)) if message.contains(rejected_field)
            ));
        }
    }

    #[test]
    fn test_parse_narinfo_accepts_valid_signature_when_key_configured() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let cache = BinaryCache::new(store).unwrap();

        let signing_key = deterministic_signing_key(7);
        let path = StorePath::new(Hash::of(b"signed"), "pkg-1.0".to_string());
        let content = signed_narinfo_text(&path, &signing_key);
        let public_key = cache_public_key(&signing_key);

        let parsed = cache
            .parse_narinfo(
                &content,
                &path,
                NarInfoSource::Remote("https://cache.example"),
                Some(public_key.as_str()),
            )
            .unwrap();

        assert_eq!(parsed.path, path);
    }

    #[test]
    fn test_parse_narinfo_rejects_invalid_signature_when_key_configured() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let cache = BinaryCache::new(store).unwrap();

        let signing_key = deterministic_signing_key(7);
        let wrong_key = deterministic_signing_key(9);
        let path = StorePath::new(Hash::of(b"signed-invalid"), "pkg-1.0".to_string());
        let content = signed_narinfo_text(&path, &signing_key);
        let public_key = cache_public_key(&wrong_key);

        let err = cache
            .parse_narinfo(
                &content,
                &path,
                NarInfoSource::Remote("https://cache.example"),
                Some(public_key.as_str()),
            )
            .unwrap_err();

        assert!(matches!(err, CacheError::Signature(_)));
    }

    #[test]
    fn test_parse_narinfo_rejects_missing_signature_when_key_configured() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let cache = BinaryCache::new(store).unwrap();

        let signing_key = deterministic_signing_key(7);
        let public_key = cache_public_key(&signing_key);
        let path = StorePath::new(Hash::of(b"signed-missing"), "pkg-1.0".to_string());
        let content = unsigned_narinfo(&path).to_unsigned_text();

        let err = cache
            .parse_narinfo(
                &content,
                &path,
                NarInfoSource::Remote("https://cache.example"),
                Some(public_key.as_str()),
            )
            .unwrap_err();

        assert!(matches!(err, CacheError::Signature(_)));
    }

    #[test]
    fn test_push_local_cache_signs_narinfo_and_query_verifies_signature() {
        let temp = tempfile::TempDir::new().unwrap();
        let local_cache = temp.path().join("signed-cache");
        let store_root = temp.path().join("store");
        let store = Store::open_at(store_root.clone()).unwrap();
        let store_path = store.add_content(b"payload-signed", "pkg-1.0").unwrap();

        let signing_key = deterministic_signing_key(11);
        let private_key = cache_private_key(&signing_key);
        let public_key = cache_public_key(&signing_key);

        let mut upload_cache = BinaryCache::new(store).unwrap();
        upload_cache.add_cache(CacheConfig {
            name: "signed-local-upload".to_string(),
            local_dir: Some(local_cache.clone()),
            private_key: Some(private_key),
            upload: true,
            ..Default::default()
        });
        upload_cache.push(&store_path).unwrap();

        let narinfo_path = local_cache.join(format!("{}.narinfo", store_path.hash()));
        let narinfo_content = fs::read_to_string(&narinfo_path).unwrap();
        assert!(narinfo_content.contains("Sig: ed25519:"));

        let verify_store = Store::open_at(store_root).unwrap();
        let mut verify_cache = BinaryCache::new(verify_store).unwrap();
        verify_cache.add_cache(CacheConfig {
            name: "signed-local-verify".to_string(),
            local_dir: Some(local_cache),
            public_key: Some(public_key),
            ..Default::default()
        });

        let queried = verify_cache
            .query(&store_path)
            .unwrap()
            .expect("signed cache query should succeed");
        assert_eq!(queried.path, store_path);
    }

    #[test]
    fn test_push_rejects_public_key_without_private_key() {
        let temp = tempfile::TempDir::new().unwrap();
        let local_cache = temp.path().join("signed-cache");
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let store_path = store.add_content(b"payload-signed", "pkg-1.0").unwrap();

        let signing_key = deterministic_signing_key(11);
        let mut cache = BinaryCache::new(store).unwrap();
        cache.add_cache(CacheConfig {
            name: "signed-local-upload".to_string(),
            local_dir: Some(local_cache),
            public_key: Some(cache_public_key(&signing_key)),
            upload: true,
            ..Default::default()
        });

        let err = cache.push(&store_path).unwrap_err();
        assert!(matches!(err, CacheError::Signature(_)));
    }

    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[test]
    fn test_remote_cache_signed_roundtrip_query_and_fetch() {
        let temp = tempfile::TempDir::new().unwrap();
        let server = TestHttpCacheServer::start();
        let signing_key = deterministic_signing_key(21);
        let private_key = cache_private_key(&signing_key);
        let public_key = cache_public_key(&signing_key);

        let upload_store = Store::open_at(temp.path().join("upload-store")).unwrap();
        let source = temp.path().join("remote-source.txt");
        fs::write(&source, b"remote-cache-payload").unwrap();
        let nar_hash = nar::hash_path(&source).unwrap();
        let store_path = StorePath::new(nar_hash, "pkg-1.0".to_string());
        let upload_path = upload_store.to_path(&store_path);
        if let Some(parent) = upload_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::copy(&source, &upload_path).unwrap();

        let mut upload_cache = BinaryCache::new(upload_store).unwrap();
        upload_cache.add_cache(CacheConfig {
            name: "remote-upload".to_string(),
            url: Some(server.base_url.clone()),
            private_key: Some(private_key),
            upload: true,
            ..Default::default()
        });
        upload_cache.push(&store_path).unwrap();

        let narinfo_path = format!("/{}.narinfo", store_path.hash());
        let uploaded_narinfo = String::from_utf8(server.read_path(&narinfo_path).unwrap()).unwrap();
        assert!(uploaded_narinfo.contains("Sig: ed25519:"));

        let fetch_root = temp.path().join("fetch-store");
        let mut fetch_cache =
            BinaryCache::new(Store::open_at(fetch_root.clone()).unwrap()).unwrap();
        fetch_cache.add_cache(CacheConfig {
            name: "remote-read".to_string(),
            url: Some(server.base_url.clone()),
            public_key: Some(public_key),
            ..Default::default()
        });

        let cached = fetch_cache
            .query(&store_path)
            .unwrap()
            .expect("remote cache query should return narinfo");
        fetch_cache.fetch(&cached).unwrap();

        let fetched_store = Store::open_at(fetch_root).unwrap();
        assert!(fetched_store.path_exists(&store_path));
        let fetched_path = fetched_store.to_path(&store_path);
        let content = fs::read(fetched_path).unwrap();
        assert_eq!(content, b"remote-cache-payload");
    }

    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[test]
    fn test_remote_cache_roundtrip_fetch_for_add_content_path() {
        let temp = tempfile::TempDir::new().unwrap();
        let server = TestHttpCacheServer::start();

        let upload_store = Store::open_at(temp.path().join("upload-store")).unwrap();
        let store_path = upload_store
            .add_content(b"remote-add-content-roundtrip", "pkg-1.0")
            .unwrap();

        let mut upload_cache = BinaryCache::new(upload_store).unwrap();
        upload_cache.add_cache(CacheConfig {
            name: "remote-upload".to_string(),
            url: Some(server.base_url.clone()),
            upload: true,
            ..Default::default()
        });
        upload_cache.push(&store_path).unwrap();

        let fetch_root = temp.path().join("fetch-store");
        let mut fetch_cache =
            BinaryCache::new(Store::open_at(fetch_root.clone()).unwrap()).unwrap();
        fetch_cache.add_cache(CacheConfig {
            name: "remote-read".to_string(),
            url: Some(server.base_url.clone()),
            ..Default::default()
        });

        let cached = fetch_cache
            .query(&store_path)
            .unwrap()
            .expect("remote cache query should return narinfo");
        fetch_cache.fetch(&cached).unwrap();

        let fetched_store = Store::open_at(fetch_root).unwrap();
        assert!(fetched_store.path_exists(&store_path));
        let fetched_path = fetched_store.to_path(&store_path);
        assert_eq!(
            fs::read(fetched_path).unwrap(),
            b"remote-add-content-roundtrip"
        );
    }

    #[test]
    fn test_http_cache_server_reads_split_request_body() {
        let server = TestHttpCacheServer::start();
        let addr = server
            .base_url
            .strip_prefix("http://")
            .expect("base url keeps the http scheme");

        let mut client = std::net::TcpStream::connect(addr).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        client
            .write_all(
                b"PUT /split HTTP/1.1\r\nHost: localhost\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabc",
            )
            .unwrap();
        // Windows returns a non-blocking socket when accepting from a
        // non-blocking listener, so a body split across reads used to close the
        // connection mid-request; Unix returns a blocking one, so the failure
        // only surfaces on Windows.
        // Windows 上从非阻塞 listener accept 会得到非阻塞连接，分段到达的请求体曾导致连接
        // 在请求中途被关闭；Unix 返回阻塞连接，因此该故障只在 Windows 暴露。
        thread::sleep(std::time::Duration::from_millis(100));
        client.write_all(b"def").unwrap();

        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        assert!(response.contains("200 OK"), "got: {response:?}");
        assert_eq!(
            server.read_path("/split"),
            Some(b"abcdef".to_vec()),
            "the full request body must be stored"
        );
    }

    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[test]
    fn test_remote_cache_roundtrip_fetch_for_add_dir_path() {
        let temp = tempfile::TempDir::new().unwrap();
        let server = TestHttpCacheServer::start();

        let source_dir = temp.path().join("remote-source-dir");
        fs::create_dir_all(source_dir.join("nested")).unwrap();
        fs::write(source_dir.join("root.txt"), b"remote-root").unwrap();
        fs::write(source_dir.join("nested").join("child.txt"), b"remote-child").unwrap();

        let upload_store = Store::open_at(temp.path().join("upload-store")).unwrap();
        let store_path = upload_store.add_dir(&source_dir, "pkg-dir-1.0").unwrap();

        let mut upload_cache = BinaryCache::new(upload_store).unwrap();
        upload_cache.add_cache(CacheConfig {
            name: "remote-upload".to_string(),
            url: Some(server.base_url.clone()),
            upload: true,
            ..Default::default()
        });
        upload_cache.push(&store_path).unwrap();

        let fetch_root = temp.path().join("fetch-store");
        let mut fetch_cache =
            BinaryCache::new(Store::open_at(fetch_root.clone()).unwrap()).unwrap();
        fetch_cache.add_cache(CacheConfig {
            name: "remote-read".to_string(),
            url: Some(server.base_url.clone()),
            ..Default::default()
        });

        let cached = fetch_cache
            .query(&store_path)
            .unwrap()
            .expect("remote cache query should return narinfo");
        fetch_cache.fetch(&cached).unwrap();

        let fetched_store = Store::open_at(fetch_root).unwrap();
        let fetched_path = fetched_store.to_path(&store_path);
        assert_eq!(
            fs::read(fetched_path.join("root.txt")).unwrap(),
            b"remote-root"
        );
        assert_eq!(
            fs::read(fetched_path.join("nested").join("child.txt")).unwrap(),
            b"remote-child"
        );
    }

    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[test]
    fn test_fetch_remote_cache_recursively_fetches_references() {
        let temp = tempfile::TempDir::new().unwrap();
        let server = TestHttpCacheServer::start();

        let upload_store = Store::open_at(temp.path().join("upload-store")).unwrap();
        let dependency = stage_store_file(
            &upload_store,
            temp.path(),
            "dep-source.txt",
            b"dependency-payload",
            "dep-1.0",
        );
        let root_payload = format!("root-uses-{}", dependency.display_name());
        let root = stage_store_file(
            &upload_store,
            temp.path(),
            "root-source.txt",
            root_payload.as_bytes(),
            "root-1.0",
        );

        let mut upload_cache = BinaryCache::new(upload_store).unwrap();
        upload_cache.add_cache(CacheConfig {
            name: "remote-upload".to_string(),
            url: Some(server.base_url.clone()),
            upload: true,
            ..Default::default()
        });
        upload_cache.push(&dependency).unwrap();
        upload_cache.push(&root).unwrap();

        let root_narinfo_path = format!("/{}.narinfo", root.hash());
        let root_narinfo =
            String::from_utf8(server.read_path(&root_narinfo_path).unwrap()).unwrap();
        let root_narinfo = format!(
            "{}References: {}\n",
            root_narinfo,
            dependency.display_name()
        );
        server.write_path(&root_narinfo_path, root_narinfo.as_bytes());

        let fetch_root = temp.path().join("fetch-store");
        let mut fetch_cache =
            BinaryCache::new(Store::open_at(fetch_root.clone()).unwrap()).unwrap();
        fetch_cache.add_cache(CacheConfig {
            name: "remote-read".to_string(),
            url: Some(server.base_url.clone()),
            ..Default::default()
        });

        let cached = fetch_cache
            .query(&root)
            .unwrap()
            .expect("root path should be available");
        fetch_cache.fetch(&cached).unwrap();

        let fetched_store = Store::open_at(fetch_root).unwrap();
        assert!(fetched_store.path_exists(&root));
        assert!(fetched_store.path_exists(&dependency));

        let mut db = Database::open(fetched_store.root().to_path_buf()).unwrap();
        let root_info = db
            .query(&root)
            .unwrap()
            .expect("root metadata should be registered");
        let dep_info = db
            .query(&dependency)
            .unwrap()
            .expect("dependency metadata should be registered");
        assert!(root_info.references.contains(&dependency));
        assert!(dep_info.references.is_empty());
    }

    #[test]
    fn test_fetch_existing_path_registers_metadata_without_download() {
        let temp = tempfile::TempDir::new().unwrap();
        let store_root = temp.path().join("store");
        let store = Store::open_at(store_root.clone()).unwrap();
        let dependency = store
            .add_content(b"dependency-existing", "dep-1.0")
            .unwrap();
        let embedded = store
            .add_content(b"embedded-existing", "embedded-1.0")
            .unwrap();
        let root = store
            .add_content(
                store.to_path(&embedded).to_string_lossy().as_bytes(),
                "root-1.0",
            )
            .unwrap();

        let cached = CachedPath {
            path: root.clone(),
            derivation: placeholder_derivation("root-1.0"),
            references: vec![dependency.clone()],
            size: 777,
            file_size: None,
            compression: CompressionFormat::Xz,
            url: None,
            file_hash: None,
            nar_hash: None,
        };

        let mut cache = BinaryCache::new(Store::open_at(store_root.clone()).unwrap()).unwrap();
        cache.fetch(&cached).unwrap();
        let mut reduced = cached.clone();
        reduced.references.clear();
        cache.fetch(&reduced).unwrap();

        let mut db = Database::open(store_root).unwrap();
        let info = db
            .query(&root)
            .unwrap()
            .expect("existing path metadata should be registered");
        assert_eq!(info.nar_hash, *root.hash());
        assert_eq!(info.nar_size, 777);
        assert!(info.references.contains(&dependency));
        assert!(info.references.contains(&embedded));

        let dep_info = db
            .query(&dependency)
            .unwrap()
            .expect("existing dependency metadata should be backfilled");
        assert_eq!(dep_info.nar_hash, *dependency.hash());
        assert!(dep_info.references.is_empty());
        assert!(dep_info.nar_size > 0);
    }

    #[test]
    fn test_fetch_existing_path_backfills_reference_metadata_when_cache_query_fails() {
        let temp = tempfile::TempDir::new().unwrap();
        let store_root = temp.path().join("store");
        let store = Store::open_at(store_root.clone()).unwrap();
        let dependency = store
            .add_content(b"dependency-existing", "dep-1.0")
            .unwrap();
        let root = store.add_content(b"root-existing", "root-1.0").unwrap();

        let down_server = TestHttpCacheServer::start();
        let down_url = down_server.base_url.clone();
        drop(down_server);

        let cached = CachedPath {
            path: root.clone(),
            derivation: placeholder_derivation("root-1.0"),
            references: vec![dependency.clone()],
            size: 777,
            file_size: None,
            compression: CompressionFormat::Xz,
            url: None,
            file_hash: None,
            nar_hash: None,
        };

        let mut cache = BinaryCache::new(Store::open_at(store_root.clone()).unwrap()).unwrap();
        cache.add_cache(CacheConfig {
            name: "down-cache".to_string(),
            url: Some(down_url),
            ..Default::default()
        });
        cache.fetch(&cached).unwrap();

        let mut db = Database::open(store_root).unwrap();
        let dep_info = db
            .query(&dependency)
            .unwrap()
            .expect("dependency metadata should still be backfilled");
        assert_eq!(dep_info.nar_hash, *dependency.hash());
        assert!(dep_info.references.is_empty());
    }

    #[test]
    fn fetch_existing_reference_without_metadata_retains_transitive_dependency() {
        let temp = tempfile::tempdir().unwrap();
        let store_root = temp.path().join("store");
        let store = Store::open_at(store_root.clone()).unwrap();
        let third = store.add_content(b"third", "third").unwrap();
        let second = store
            .add_content(store.to_path(&third).to_string_lossy().as_bytes(), "second")
            .unwrap();
        let first = store.add_content(b"first", "first").unwrap();
        let garbage = store.add_content(b"garbage", "garbage").unwrap();
        let database = Database::open(store_root.clone()).unwrap();
        fs::remove_file(database.info_path(&second)).unwrap();

        let cached = CachedPath {
            path: first.clone(),
            derivation: placeholder_derivation("first"),
            references: vec![second.clone()],
            size: 5,
            file_size: None,
            compression: CompressionFormat::Xz,
            url: None,
            file_hash: None,
            nar_hash: None,
        };
        let mut cache = BinaryCache::new(store).unwrap();
        cache.fetch(&cached).unwrap();
        let mut store = Store::open_at(store_root.clone()).unwrap();
        let mut database = Database::open(store_root).unwrap();
        assert!(
            database
                .query(&second)
                .unwrap()
                .unwrap()
                .references
                .contains(&third)
        );
        {
            let gc = crate::GarbageCollector::new(&mut store);
            gc.add_root("first", &first).unwrap();
        }
        let result = crate::GarbageCollector::new(&mut store).collect().unwrap();
        assert_eq!(result.deleted, 1);
        assert!(store.path_exists(&second));
        assert!(store.path_exists(&third));
        assert!(!store.path_exists(&garbage));
    }

    #[test]
    fn test_fetch_remote_cache_fails_when_reference_is_missing() {
        let temp = tempfile::TempDir::new().unwrap();
        let server = TestHttpCacheServer::start();

        let upload_store = Store::open_at(temp.path().join("upload-store")).unwrap();
        let root = stage_store_file(
            &upload_store,
            temp.path(),
            "root-missing-ref-source.txt",
            b"root-with-missing-ref",
            "root-1.0",
        );
        let missing_reference = StorePath::new(Hash::of(b"missing-reference"), "dep-1.0".into());

        let mut upload_cache = BinaryCache::new(upload_store).unwrap();
        upload_cache.add_cache(CacheConfig {
            name: "remote-upload".to_string(),
            url: Some(server.base_url.clone()),
            upload: true,
            ..Default::default()
        });
        upload_cache.push(&root).unwrap();

        let root_narinfo_path = format!("/{}.narinfo", root.hash());
        let root_narinfo =
            String::from_utf8(server.read_path(&root_narinfo_path).unwrap()).unwrap();
        let root_narinfo = format!(
            "{}References: {}\n",
            root_narinfo,
            missing_reference.display_name()
        );
        server.write_path(&root_narinfo_path, root_narinfo.as_bytes());

        let fetch_root = temp.path().join("fetch-store");
        let mut fetch_cache =
            BinaryCache::new(Store::open_at(fetch_root.clone()).unwrap()).unwrap();
        fetch_cache.add_cache(CacheConfig {
            name: "remote-read".to_string(),
            url: Some(server.base_url.clone()),
            ..Default::default()
        });

        let cached = fetch_cache
            .query(&root)
            .unwrap()
            .expect("root path should be available");
        let err = fetch_cache.fetch(&cached).unwrap_err();
        assert!(matches!(err, CacheError::NotFound(_)));
        let fetched_store = Store::open_at(fetch_root).unwrap();
        assert!(!fetched_store.path_exists(&root));
    }

    #[test]
    fn test_push_remote_cache_retries_transient_put_failure() {
        let temp = tempfile::TempDir::new().unwrap();
        let server = TestHttpCacheServer::start();

        let upload_store = Store::open_at(temp.path().join("upload-store")).unwrap();
        let source = temp.path().join("remote-retry-source.txt");
        fs::write(&source, b"remote-cache-retry").unwrap();
        let nar_hash = nar::hash_path(&source).unwrap();
        let store_path = StorePath::new(nar_hash, "pkg-1.0".to_string());
        let upload_path = upload_store.to_path(&store_path);
        if let Some(parent) = upload_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::copy(&source, &upload_path).unwrap();

        let narinfo_path = format!("/{}.narinfo", store_path.hash());
        server.fail_next_put(&narinfo_path, 1);

        let mut upload_cache = BinaryCache::new(upload_store).unwrap();
        upload_cache.add_cache(CacheConfig {
            name: "remote-upload-retry".to_string(),
            url: Some(server.base_url.clone()),
            upload: true,
            ..Default::default()
        });

        upload_cache.push(&store_path).unwrap();
        assert!(server.read_path(&narinfo_path).is_some());
    }

    #[test]
    fn test_query_remote_cache_retries_transient_get_failure() {
        let temp = tempfile::TempDir::new().unwrap();
        let server = TestHttpCacheServer::start();
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let mut cache = BinaryCache::new(store).unwrap();

        let store_path = StorePath::new(Hash::of(b"remote-get-retry"), "pkg-1.0".to_string());
        let narinfo_path = format!("/{}.narinfo", store_path.hash());
        let narinfo = unsigned_narinfo(&store_path).to_text();
        server.write_path(&narinfo_path, narinfo.as_bytes());
        server.fail_next_get(&narinfo_path, 1);

        cache.add_cache(CacheConfig {
            name: "remote-query-retry".to_string(),
            url: Some(server.base_url.clone()),
            ..Default::default()
        });

        let cached = cache
            .query(&store_path)
            .unwrap()
            .expect("query should recover after transient GET failure");
        assert_eq!(cached.path, store_path);
        assert_eq!(server.request_count("GET", &narinfo_path), 2);
    }

    #[test]
    fn test_query_remote_cache_does_not_retry_not_found() {
        let temp = tempfile::TempDir::new().unwrap();
        let server = TestHttpCacheServer::start();
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let mut cache = BinaryCache::new(store).unwrap();

        let store_path = StorePath::new(Hash::of(b"remote-not-found"), "pkg-1.0".to_string());
        let narinfo_path = format!("/{}.narinfo", store_path.hash());

        cache.add_cache(CacheConfig {
            name: "remote-query-404".to_string(),
            url: Some(server.base_url.clone()),
            ..Default::default()
        });

        assert!(cache.query(&store_path).unwrap().is_none());
        assert_eq!(server.request_count("GET", &narinfo_path), 1);
    }

    #[test]
    fn remote_narinfo_disallowed_urls_rejected_before_payload_request() {
        let temp = tempfile::TempDir::new().unwrap();
        let server = TestHttpCacheServer::start();
        let victim = TestHttpCacheServer::start();
        let local_nar = temp.path().join("private.nar");
        fs::write(&local_nar, b"private").unwrap();
        let path = StorePath::new(Hash::of(b"untrusted-nar-url"), "pkg".to_string());
        let narinfo_path = format!("/{}.narinfo", path.hash());
        let mut cache =
            BinaryCache::new(Store::open_at(temp.path().join("store")).unwrap()).unwrap();
        cache.add_cache(CacheConfig {
            url: Some(server.base_url.clone()),
            ..Default::default()
        });
        for bad_url in [
            format!("file://{}", local_nar.display()),
            local_nar.display().to_string(),
            format!("{}/payload.nar", victim.base_url),
            format!(
                "//{}/payload.nar",
                victim.base_url.trim_start_matches("http://")
            ),
        ] {
            let mut narinfo = unsigned_narinfo(&path);
            narinfo.url = bad_url.clone();
            server.write_path(&narinfo_path, narinfo.to_text().as_bytes());
            let err = cache.query(&path).unwrap_err();
            assert!(
                matches!(&err, CacheError::InvalidManifest(message)
                    if message.contains(&bad_url) && message.contains(&server.base_url)),
                "{err}"
            );
        }
        assert_eq!(victim.request_count("GET", "/payload.nar"), 0);
        assert!(
            !cache
                .cache_dir
                .join(format!("{}.nar.xz", path.hash()))
                .exists()
        );
    }

    #[test]
    fn remote_narinfo_oversized_response_rejected_with_and_without_length() {
        let temp = tempfile::TempDir::new().unwrap();
        let server = TestHttpCacheServer::start();
        let path = StorePath::new(Hash::of(b"oversized-narinfo"), "pkg".to_string());
        let narinfo_path = format!("/{}.narinfo", path.hash());
        let mut cache =
            BinaryCache::new(Store::open_at(temp.path().join("store")).unwrap()).unwrap();
        cache.add_cache(CacheConfig {
            url: Some(server.base_url.clone()),
            ..Default::default()
        });
        let oversized = vec![b'#'; MAX_NARINFO_SIZE as usize + 1];
        server.write_path(&narinfo_path, &oversized);
        let err = cache.query(&path).unwrap_err();
        assert!(matches!(err, CacheError::InvalidManifest(message)
            if message.contains("narinfo size") && message.contains(&narinfo_path)));

        server.serve_without_length(&narinfo_path);
        let err = cache.query(&path).unwrap_err();
        assert!(matches!(err, CacheError::InvalidManifest(message)
            if message.contains("narinfo size") && message.contains(&narinfo_path)));

        let mut boundary = unsigned_narinfo(&path).to_text().into_bytes();
        boundary.resize(MAX_NARINFO_SIZE as usize, b' ');
        server.write_path(&narinfo_path, &boundary);
        assert!(cache.query(&path).unwrap().is_some());
    }

    #[test]
    fn remote_narinfo_cross_origin_redirect_rejected_before_following() {
        let temp = tempfile::TempDir::new().unwrap();
        let server = TestHttpCacheServer::start();
        let victim = TestHttpCacheServer::start();
        let path = StorePath::new(Hash::of(b"redirect-narinfo"), "pkg".to_string());
        let narinfo_path = format!("/{}.narinfo", path.hash());
        server.redirect_path(&narinfo_path, &format!("{}{narinfo_path}", victim.base_url));
        let mut cache =
            BinaryCache::new(Store::open_at(temp.path().join("store")).unwrap()).unwrap();
        cache.add_cache(CacheConfig {
            url: Some(server.base_url.clone()),
            ..Default::default()
        });
        let err = cache.query(&path).unwrap_err();
        assert!(matches!(err, CacheError::Fetch(message)
            if message.contains(&narinfo_path) && message.contains("redirect")));
        assert_eq!(server.request_count("GET", &narinfo_path), 1);
        assert_eq!(victim.request_count("GET", &narinfo_path), 0);
    }

    #[test]
    fn remote_nar_cross_origin_redirect_rejected_before_following() {
        let temp = tempfile::TempDir::new().unwrap();
        let server = TestHttpCacheServer::start();
        let victim = TestHttpCacheServer::start();
        let path = StorePath::new(Hash::of(b"redirect-nar"), "pkg".to_string());
        let narinfo_path = format!("/{}.narinfo", path.hash());
        let nar_path = format!("/{}.nar.xz", path.hash());
        server.write_path(&narinfo_path, unsigned_narinfo(&path).to_text().as_bytes());
        server.redirect_path(&nar_path, &format!("{}{nar_path}", victim.base_url));
        let mut cache =
            BinaryCache::new(Store::open_at(temp.path().join("store")).unwrap()).unwrap();
        cache.add_cache(CacheConfig {
            url: Some(server.base_url.clone()),
            ..Default::default()
        });
        let cached = cache.query(&path).unwrap().unwrap();
        let err = cache.fetch(&cached).unwrap_err();
        assert!(matches!(err, CacheError::Fetch(message)
            if message.contains(&nar_path) && message.contains("redirect")));
        assert_eq!(server.request_count("GET", &nar_path), 1);
        assert_eq!(victim.request_count("GET", &nar_path), 0);
    }

    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[test]
    fn remote_nar_same_origin_redirect_preserves_verified_download() {
        let temp = tempfile::TempDir::new().unwrap();
        let server = TestHttpCacheServer::start();
        let source = Store::open_at(temp.path().join("source")).unwrap();
        let path = source.add_content(b"same-origin-redirect", "pkg").unwrap();
        let mut upload_cache = BinaryCache::new(source).unwrap();
        upload_cache.add_cache(CacheConfig {
            url: Some(server.base_url.clone()),
            upload: true,
            ..Default::default()
        });
        upload_cache.push(&path).unwrap();

        let nar_path = format!("/{}.nar.xz", path.hash());
        let redirected_path = "/redirected.nar.xz";
        let nar_bytes = server.read_path(&nar_path).unwrap();
        server.write_path(redirected_path, &nar_bytes);
        server.redirect_path(&nar_path, redirected_path);
        let fetch_store = Store::open_at(temp.path().join("fetch")).unwrap();
        let mut fetch_cache = BinaryCache::new(fetch_store).unwrap();
        fetch_cache.add_cache(CacheConfig {
            url: Some(server.base_url.clone()),
            ..Default::default()
        });
        let cached = fetch_cache.query(&path).unwrap().unwrap();
        fetch_cache.fetch(&cached).unwrap();
        assert_eq!(
            fs::read(fetch_cache.store.to_path(&path)).unwrap(),
            b"same-origin-redirect"
        );
        assert_eq!(server.request_count("GET", &nar_path), 1);
        assert_eq!(server.request_count("GET", redirected_path), 1);
    }

    #[test]
    fn test_query_uses_lower_priority_cache_when_higher_remote_unavailable() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let mut cache = BinaryCache::new(store).unwrap();
        let local_cache = temp.path().join("local-cache");
        fs::create_dir_all(&local_cache).unwrap();

        let down_server = TestHttpCacheServer::start();
        let down_url = down_server.base_url.clone();
        drop(down_server);

        let store_path = StorePath::new(Hash::of(b"fallback-query"), "pkg-1.0".to_string());
        let narinfo_path = local_cache.join(format!("{}.narinfo", store_path.hash()));
        fs::write(&narinfo_path, unsigned_narinfo(&store_path).to_text()).unwrap();

        cache.add_cache(CacheConfig {
            name: "high-remote-down".to_string(),
            url: Some(down_url),
            priority: 100,
            ..Default::default()
        });
        cache.add_cache(CacheConfig {
            name: "low-local".to_string(),
            local_dir: Some(local_cache),
            priority: 10,
            ..Default::default()
        });

        let cached = cache
            .query(&store_path)
            .unwrap()
            .expect("lower-priority cache should still satisfy query");
        assert_eq!(cached.path, store_path);
    }

    #[test]
    fn test_query_returns_fetch_error_when_only_remote_cache_is_unavailable() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let mut cache = BinaryCache::new(store).unwrap();

        let down_server = TestHttpCacheServer::start();
        let down_url = down_server.base_url.clone();
        drop(down_server);

        cache.add_cache(CacheConfig {
            name: "remote-down".to_string(),
            url: Some(down_url),
            ..Default::default()
        });

        let store_path = StorePath::new(Hash::of(b"remote-down"), "pkg-1.0".to_string());
        let err = cache.query(&store_path).unwrap_err();
        assert!(matches!(err, CacheError::Fetch(_)));
    }

    #[test]
    fn test_push_local_cache_records_references_in_narinfo() {
        let temp = tempfile::TempDir::new().unwrap();
        let local_cache = temp.path().join("local-cache");
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let dependency = stage_store_file(
            &store,
            temp.path(),
            "dep-reference-source.txt",
            b"dependency-payload",
            "dep-1.0",
        );
        let root_payload = format!("depends-on:{}\n", dependency.display_name());
        let root = stage_store_file(
            &store,
            temp.path(),
            "root-reference-source.txt",
            root_payload.as_bytes(),
            "root-1.0",
        );

        let mut cache = BinaryCache::new(store).unwrap();
        cache.add_cache(CacheConfig {
            name: "local".to_string(),
            local_dir: Some(local_cache.clone()),
            upload: true,
            ..Default::default()
        });

        cache.push(&root).unwrap();

        let narinfo_path = local_cache.join(format!("{}.narinfo", root.hash()));
        let narinfo = fs::read_to_string(&narinfo_path).unwrap();
        assert!(narinfo.contains(&format!("References: {}", dependency.display_name())));

        let queried = cache.query(&root).unwrap().expect("query should succeed");
        assert_eq!(queried.references, vec![dependency]);
    }

    #[test]
    fn test_push_local_cache_prefers_database_references_when_present() {
        let temp = tempfile::TempDir::new().unwrap();
        let local_cache = temp.path().join("local-cache");
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let dependency = stage_store_file(
            &store,
            temp.path(),
            "dep-db-reference-source.txt",
            b"dependency-payload",
            "dep-1.0",
        );
        let root = stage_store_file(
            &store,
            temp.path(),
            "root-db-reference-source.txt",
            b"root-without-inline-reference",
            "root-1.0",
        );

        let mut db = Database::open(store.root().to_path_buf()).unwrap();
        let mut info = PathInfo::new(root.clone(), *root.hash(), 0);
        info.add_reference(dependency.clone());
        db.register(info).unwrap();

        let mut cache = BinaryCache::new(store).unwrap();
        cache.add_cache(CacheConfig {
            name: "local".to_string(),
            local_dir: Some(local_cache.clone()),
            upload: true,
            ..Default::default()
        });

        cache.push(&root).unwrap();

        let narinfo_path = local_cache.join(format!("{}.narinfo", root.hash()));
        let narinfo = fs::read_to_string(&narinfo_path).unwrap();
        assert!(narinfo.contains(&format!("References: {}", dependency.display_name())));

        let queried = cache.query(&root).unwrap().expect("query should succeed");
        assert_eq!(queried.references, vec![dependency]);
    }

    #[test]
    fn test_push_local_cache_ignores_nonexistent_reference_tokens() {
        let temp = tempfile::TempDir::new().unwrap();
        let local_cache = temp.path().join("local-cache");
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let fake_reference = format!("{}-ghost-1.0", "a".repeat(64));
        let root_payload = format!("not-a-real-ref:{}\n", fake_reference);
        let root = stage_store_file(
            &store,
            temp.path(),
            "root-fake-reference-source.txt",
            root_payload.as_bytes(),
            "root-1.0",
        );

        let mut cache = BinaryCache::new(store).unwrap();
        cache.add_cache(CacheConfig {
            name: "local".to_string(),
            local_dir: Some(local_cache.clone()),
            upload: true,
            ..Default::default()
        });

        cache.push(&root).unwrap();

        let narinfo_path = local_cache.join(format!("{}.narinfo", root.hash()));
        let narinfo = fs::read_to_string(&narinfo_path).unwrap();
        assert!(!narinfo.contains("References:"));

        let queried = cache.query(&root).unwrap().expect("query should succeed");
        assert!(queried.references.is_empty());
    }

    #[test]
    fn test_push_local_cache_writes_narinfo_and_query_uses_it() {
        let temp = tempfile::TempDir::new().unwrap();
        let local_cache = temp.path().join("local-cache");
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let store_path = store.add_content(b"payload", "pkg-1.0").unwrap();

        let mut cache = BinaryCache::new(store).unwrap();
        cache.add_cache(CacheConfig {
            name: "local".to_string(),
            local_dir: Some(local_cache.clone()),
            upload: true,
            ..Default::default()
        });

        cache.push(&store_path).unwrap();

        let narinfo_path = local_cache.join(format!("{}.narinfo", store_path.hash()));
        assert!(narinfo_path.exists());

        let narinfo = std::fs::read_to_string(&narinfo_path).unwrap();
        assert!(narinfo.contains(&format!("StorePath: {}", store_path.display_name())));
        assert!(narinfo.contains("Compression: xz"));

        let queried = cache
            .query(&store_path)
            .unwrap()
            .expect("query should succeed");
        assert_eq!(queried.path, store_path);
        assert!(queried.url.is_some());
    }

    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[test]
    fn test_local_cache_roundtrip_fetch_for_add_content_path() {
        let temp = tempfile::TempDir::new().unwrap();
        let local_cache = temp.path().join("local-cache");

        let upload_root = temp.path().join("upload-store");
        let upload_store = Store::open_at(upload_root).unwrap();
        let store_path = upload_store
            .add_content(b"roundtrip-payload", "pkg-1.0")
            .unwrap();

        let mut upload_cache = BinaryCache::new(upload_store).unwrap();
        upload_cache.add_cache(CacheConfig {
            name: "local-upload".to_string(),
            local_dir: Some(local_cache.clone()),
            upload: true,
            ..Default::default()
        });
        upload_cache.push(&store_path).unwrap();

        let fetch_root = temp.path().join("fetch-store");
        let mut fetch_cache =
            BinaryCache::new(Store::open_at(fetch_root.clone()).unwrap()).unwrap();
        fetch_cache.add_cache(CacheConfig {
            name: "local-read".to_string(),
            local_dir: Some(local_cache),
            ..Default::default()
        });

        let cached = fetch_cache
            .query(&store_path)
            .unwrap()
            .expect("query should return cached metadata");
        fetch_cache.fetch(&cached).unwrap();

        let fetched_store = Store::open_at(fetch_root).unwrap();
        assert!(fetched_store.path_exists(&store_path));
        let fetched_path = fetched_store.to_path(&store_path);
        assert_eq!(fs::read(fetched_path).unwrap(), b"roundtrip-payload");
    }

    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[test]
    fn test_local_cache_roundtrip_fetch_for_add_dir_path() {
        let temp = tempfile::TempDir::new().unwrap();
        let local_cache = temp.path().join("local-cache");

        let source_dir = temp.path().join("source-dir");
        fs::create_dir_all(source_dir.join("nested")).unwrap();
        fs::write(source_dir.join("root.txt"), b"root-file").unwrap();
        fs::write(source_dir.join("nested").join("child.txt"), b"child-file").unwrap();

        let upload_root = temp.path().join("upload-store");
        let upload_store = Store::open_at(upload_root).unwrap();
        let store_path = upload_store.add_dir(&source_dir, "pkg-dir-1.0").unwrap();

        let mut upload_cache = BinaryCache::new(upload_store).unwrap();
        upload_cache.add_cache(CacheConfig {
            name: "local-upload".to_string(),
            local_dir: Some(local_cache.clone()),
            upload: true,
            ..Default::default()
        });
        upload_cache.push(&store_path).unwrap();

        let fetch_root = temp.path().join("fetch-store");
        let mut fetch_cache =
            BinaryCache::new(Store::open_at(fetch_root.clone()).unwrap()).unwrap();
        fetch_cache.add_cache(CacheConfig {
            name: "local-read".to_string(),
            local_dir: Some(local_cache),
            ..Default::default()
        });

        let cached = fetch_cache
            .query(&store_path)
            .unwrap()
            .expect("query should return cached metadata");
        fetch_cache.fetch(&cached).unwrap();

        let fetched_store = Store::open_at(fetch_root).unwrap();
        let fetched_path = fetched_store.to_path(&store_path);
        assert_eq!(
            fs::read(fetched_path.join("root.txt")).unwrap(),
            b"root-file"
        );
        assert_eq!(
            fs::read(fetched_path.join("nested").join("child.txt")).unwrap(),
            b"child-file"
        );
    }

    #[test]
    fn test_fetch_rejects_file_hash_mismatch() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let mut cache = BinaryCache::new(store).unwrap();

        let source = temp.path().join("source.txt");
        fs::write(&source, b"hello-cache").unwrap();

        let nar_data = nar::create_nar(&source).unwrap();
        let nar_hash = Hash::of(&nar_data);
        let mut compressed = Vec::new();
        lzma_rs::xz_compress(&mut std::io::Cursor::new(&nar_data), &mut compressed).unwrap();
        let nar_file = temp.path().join("payload.nar.xz");
        fs::write(&nar_file, &compressed).unwrap();

        let cached = CachedPath {
            path: StorePath::new(nar_hash, "pkg-1.0".to_string()),
            derivation: placeholder_derivation("pkg-1.0"),
            references: Vec::new(),
            size: nar_data.len() as u64,
            file_size: None,
            compression: CompressionFormat::Xz,
            url: Some(nar_file.to_string_lossy().to_string()),
            file_hash: Some(format_hash(&Hash::of(b"wrong-file-hash"))),
            nar_hash: Some(format_hash(&nar_hash)),
        };

        let err = cache.fetch(&cached).unwrap_err();
        assert!(matches!(
            err,
            CacheError::HashMismatch {
                kind: "nar-compressed",
                ..
            }
        ));
    }

    #[test]
    fn test_fetch_rejects_nar_hash_mismatch() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let mut cache = BinaryCache::new(store).unwrap();

        let source = temp.path().join("source.txt");
        fs::write(&source, b"hello-nar").unwrap();

        let nar_data = nar::create_nar(&source).unwrap();
        let nar_hash = Hash::of(&nar_data);
        let mut compressed = Vec::new();
        lzma_rs::xz_compress(&mut std::io::Cursor::new(&nar_data), &mut compressed).unwrap();
        let nar_file = temp.path().join("payload.nar.xz");
        fs::write(&nar_file, &compressed).unwrap();

        let cached = CachedPath {
            path: StorePath::new(nar_hash, "pkg-1.0".to_string()),
            derivation: placeholder_derivation("pkg-1.0"),
            references: Vec::new(),
            size: nar_data.len() as u64,
            file_size: None,
            compression: CompressionFormat::Xz,
            url: Some(nar_file.to_string_lossy().to_string()),
            file_hash: Some(format_hash(&Hash::of(&compressed))),
            nar_hash: Some(format_hash(&Hash::of(b"wrong-nar-hash"))),
        };

        let err = cache.fetch(&cached).unwrap_err();
        assert!(matches!(err, CacheError::HashMismatch { kind: "nar", .. }));
    }

    #[test]
    fn test_fetch_hash_mismatch_does_not_register_metadata() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let store_root = root.join("store");
        let store = Store::open_at(store_root.clone()).unwrap();
        let mut cache = BinaryCache::new(store).unwrap();

        let source = root.join("source-hash-mismatch.txt");
        fs::write(&source, b"hello-hash-mismatch").unwrap();

        let nar_data = nar::create_nar(&source).unwrap();
        let nar_hash = Hash::of(&nar_data);
        let mut compressed = Vec::new();
        lzma_rs::xz_compress(&mut std::io::Cursor::new(&nar_data), &mut compressed).unwrap();
        let nar_file = root.join("payload-hash-mismatch.nar.xz");
        fs::write(&nar_file, &compressed).unwrap();

        let wrong_store_path = StorePath::new(Hash::of(b"wrong-store-path"), "pkg-1.0".to_string());
        let cached = CachedPath {
            path: wrong_store_path.clone(),
            derivation: placeholder_derivation("pkg-1.0"),
            references: Vec::new(),
            size: nar_data.len() as u64,
            file_size: None,
            compression: CompressionFormat::Xz,
            url: Some(nar_file.to_string_lossy().to_string()),
            file_hash: Some(format_hash(&Hash::of(&compressed))),
            nar_hash: Some(format_hash(&nar_hash)),
        };

        let err = cache.fetch(&cached).unwrap_err();
        assert!(matches!(
            err,
            CacheError::Store(StoreError::HashMismatch { .. })
        ));
        assert!(!cache.store.path_exists(&wrong_store_path));
        assert!(fs::read_dir(&store_root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".n3v3-substitute-")
        }));
        let mut db = Database::open(store_root).unwrap();
        assert!(db.query(&wrong_store_path).unwrap().is_none());
    }

    #[test]
    fn fetch_malformed_nar_cleans_staging_and_leaves_final_absent() {
        let temp = tempfile::TempDir::new().unwrap();
        let store_root = temp.path().join("store");
        let store = Store::open_at(store_root.clone()).unwrap();
        let mut cache = BinaryCache::new(store).unwrap();
        let nar_data = b"malformed NAR data";
        let nar_hash = Hash::of(nar_data);
        let mut compressed = Vec::new();
        lzma_rs::xz_compress(&mut std::io::Cursor::new(nar_data), &mut compressed).unwrap();
        let nar_file = temp.path().join("malformed.nar.xz");
        fs::write(&nar_file, &compressed).unwrap();
        let cached = CachedPath {
            path: StorePath::new(nar_hash, "malformed-1.0".to_string()),
            derivation: placeholder_derivation("malformed-1.0"),
            references: Vec::new(),
            size: nar_data.len() as u64,
            file_size: None,
            compression: CompressionFormat::Xz,
            url: Some(nar_file.to_string_lossy().to_string()),
            file_hash: Some(format_hash(&Hash::of(&compressed))),
            nar_hash: Some(format_hash(&nar_hash)),
        };

        assert!(matches!(cache.fetch(&cached), Err(CacheError::Nar(_))));
        assert!(!cache.store.path_exists(&cached.path));
        assert!(fs::read_dir(store_root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".n3v3-substitute-")
        }));
    }

    #[test]
    fn fetch_unsafe_store_name_rejects_before_writing_outside_store() {
        let temp = tempfile::TempDir::new().unwrap();
        let store_root = temp.path().join("store");
        let store = Store::open_at(store_root).unwrap();
        let mut cache = BinaryCache::new(store).unwrap();
        let unsafe_path = StorePath::new(Hash::of(b"unsafe"), "pkg/../../escaped".to_string());
        let cached = CachedPath {
            path: unsafe_path,
            derivation: placeholder_derivation("unsafe"),
            references: Vec::new(),
            size: 0,
            file_size: None,
            compression: CompressionFormat::None,
            url: None,
            file_hash: None,
            nar_hash: None,
        };

        assert!(matches!(
            cache.fetch(&cached),
            Err(CacheError::InvalidManifest(message)) if message.contains("unsafe store path")
        ));
        assert!(!temp.path().join("escaped").exists());
    }
    #[test]
    fn fetch_compressed_size_mismatch_discards_bad_copy_before_retry() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let source = root.join("source");
        fs::write(&source, b"retry payload").unwrap();
        let nar_data = nar::create_nar(&source).unwrap();
        let mut compressed = Vec::new();
        lzma_rs::xz_compress(&mut std::io::Cursor::new(&nar_data), &mut compressed).unwrap();
        let archive = root.join("archive.nar.xz");
        fs::write(&archive, b"bad").unwrap();
        let path = StorePath::new(nar::hash_path(&source).unwrap(), "package".to_string());
        let store = Store::open_at(root.join("store")).unwrap();
        let mut cache = BinaryCache::new(store).unwrap();
        let cached = CachedPath {
            path: path.clone(),
            derivation: placeholder_derivation("package"),
            references: Vec::new(),
            size: nar_data.len() as u64,
            file_size: Some(compressed.len() as u64),
            compression: CompressionFormat::Xz,
            url: Some(archive.to_string_lossy().to_string()),
            file_hash: Some(format_hash(&Hash::of(&compressed))),
            nar_hash: Some(format_hash(&Hash::of(&nar_data))),
        };
        assert!(matches!(
            cache.fetch(&cached),
            Err(CacheError::InvalidManifest(message)) if message.contains("FileSize mismatch")
        ));
        assert!(
            !cache
                .cache_dir
                .join(format!("{}.nar.xz", path.hash()))
                .exists()
        );
        fs::write(&archive, compressed).unwrap();
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        {
            cache.fetch(&cached).unwrap();
            assert_eq!(
                fs::read(cache.store.to_path(&path)).unwrap(),
                b"retry payload"
            );
        }
        #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
        {
            assert!(matches!(
                cache.fetch(&cached),
                Err(CacheError::InvalidManifest(message))
                    if message.contains("atomic no-replace publication is unavailable")
            ));
            assert!(!cache.store.path_exists(&path));
        }
    }

    #[test]
    fn fetch_oversized_compressed_input_rejects_before_copying() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("oversized.nar");
        fs::File::create(&archive)
            .unwrap()
            .set_len(MAX_COMPRESSED_NAR_SIZE + 1)
            .unwrap();
        let path = StorePath::new(Hash::of(b"oversized-compressed"), "package".to_string());
        let store = Store::open_at(temp.path().join("store")).unwrap();
        let mut cache = BinaryCache::new(store).unwrap();
        let cached = CachedPath {
            path: path.clone(),
            derivation: placeholder_derivation("package"),
            references: Vec::new(),
            size: 1,
            file_size: None,
            compression: CompressionFormat::None,
            url: Some(archive.to_string_lossy().to_string()),
            file_hash: None,
            nar_hash: None,
        };
        assert!(matches!(
            cache.fetch(&cached),
            Err(CacheError::InvalidManifest(message)) if message.contains("compressed file size")
        ));
        assert!(
            !cache
                .cache_dir
                .join(format!("{}.nar", path.hash()))
                .exists()
        );
    }

    #[test]
    fn decompress_over_limit_fails_before_unbounded_output_growth() {
        let temp = tempfile::TempDir::new().unwrap();
        let cache = BinaryCache::new(Store::open_at(temp.path().join("store")).unwrap()).unwrap();
        let oversized = vec![b'A'; 256 * 1024];
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(&oversized).unwrap();
        let mut xz = Vec::new();
        lzma_rs::xz_compress(&mut std::io::Cursor::new(&oversized), &mut xz).unwrap();
        let zstd = zstd::encode_all(std::io::Cursor::new(&oversized), 0).unwrap();
        for (format, archive) in [
            (CompressionFormat::Gzip, gzip.finish().unwrap()),
            (CompressionFormat::Xz, xz),
            (CompressionFormat::Zstd, zstd),
        ] {
            assert!(
                matches!(
                    cache.decompress_nar(&archive, format, 128),
                    Err(CacheError::InvalidManifest(message)) if message.contains("decompressed NAR size")
                ),
                "decode must fail on the declared output bound for {format:?}"
            );
        }
    }

    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    #[test]
    fn fetch_when_atomic_publication_unavailable_rejects_without_store_entry() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let source = root.join("source");
        fs::write(&source, b"verified payload").unwrap();
        let nar_data = nar::create_nar(&source).unwrap();
        let archive = root.join("payload.nar");
        fs::write(&archive, &nar_data).unwrap();
        let store_root = root.join("store");
        let mut cache = BinaryCache::new(Store::open_at(store_root.clone()).unwrap()).unwrap();
        let path = StorePath::new(nar::hash_path(&source).unwrap(), "package".to_string());
        let cached = CachedPath {
            path: path.clone(),
            derivation: placeholder_derivation("package"),
            references: Vec::new(),
            size: nar_data.len() as u64,
            file_size: Some(nar_data.len() as u64),
            compression: CompressionFormat::None,
            url: Some(archive.to_string_lossy().into_owned()),
            file_hash: Some(format_hash(&Hash::of(&nar_data))),
            nar_hash: Some(format_hash(&Hash::of(&nar_data))),
        };

        assert!(matches!(
            cache.fetch(&cached),
            Err(CacheError::InvalidManifest(message))
                if message.contains("atomic no-replace publication is unavailable")
        ));
        assert!(!cache.store.path_exists(&path));
        assert!(
            Database::open(store_root)
                .unwrap()
                .query(&path)
                .unwrap()
                .is_none()
        );
    }

    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[test]
    fn publish_when_destination_appears_does_not_replace_existing_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let cache = BinaryCache::new(Store::open_at(temp.path().join("store")).unwrap()).unwrap();
        let path = StorePath::new(Hash::of(b"publish"), "package".to_string());
        let staged = temp.path().join("staged");
        fs::create_dir(&staged).unwrap();
        fs::write(staged.join("result"), b"verified").unwrap();
        let destination = cache.store.to_path(&path);
        fs::create_dir(&destination).unwrap();
        assert!(matches!(
            cache.publish_staged_path(&staged, &path),
            Err(CacheError::InvalidManifest(message)) if message.contains("already exists")
        ));
        assert!(staged.join("result").exists());
        assert!(destination.is_dir());
        assert!(!destination.join("result").exists());
    }
}
