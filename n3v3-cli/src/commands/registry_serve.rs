//! Registry server for n3v3 packages (v1 API).
//! n3v3 软件包注册服务器 (v1 API)。

use crate::output;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

const MAX_REQUEST_BODY_SIZE: usize = 10 * 1024 * 1024;
const MAX_REQUEST_LINE_SIZE: usize = 8 * 1024;
const MAX_HEADER_LINE_SIZE: usize = 8 * 1024;
const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_HEADER_COUNT: usize = 100;
const MAX_CONCURRENT_CONNECTIONS: usize = 64;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PACKAGE_NAME_LENGTH: usize = 128;
const MAX_VERSION_LENGTH: usize = 128;
const MAX_DESCRIPTION_LENGTH: usize = 16 * 1024;
const MAX_DEPENDENCIES: usize = 1_024;
const MAX_METADATA_VALUE_LENGTH: usize = 512;

fn is_valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_PACKAGE_NAME_LENGTH
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn is_valid_cache_hash(hash: &str) -> bool {
    !hash.is_empty()
        && hash.len() <= MAX_PACKAGE_NAME_LENGTH
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn validate_version_metadata(metadata: &VersionMetadata) -> Result<(), &'static str> {
    let version = metadata.version.as_bytes();
    if version.is_empty()
        || version.len() > MAX_VERSION_LENGTH
        || !version
            .iter()
            .copied()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+' | b'_'))
    {
        return Err("invalid version");
    }
    if metadata.description.len() > MAX_DESCRIPTION_LENGTH {
        return Err("description is too large");
    }
    if metadata.dependencies.len() > MAX_DEPENDENCIES {
        return Err("too many dependencies");
    }
    if metadata.dependencies.iter().any(|(name, requirement)| {
        !is_valid_package_name(name)
            || requirement.is_empty()
            || requirement.len() > MAX_METADATA_VALUE_LENGTH
            || requirement.chars().any(char::is_control)
    }) {
        return Err("invalid dependency metadata");
    }
    let bounded_fields = [
        metadata.nar_hash.as_deref(),
        metadata.file_hash.as_deref(),
        metadata.license.as_deref(),
        metadata.published_at.as_deref(),
    ];
    if bounded_fields
        .into_iter()
        .flatten()
        .any(|value| value.len() > MAX_METADATA_VALUE_LENGTH || value.chars().any(char::is_control))
    {
        return Err("invalid metadata field");
    }
    Ok(())
}

// =============================================================================
// Registry v1 data model / 注册表 v1 数据模型
// =============================================================================

/// Index entry in the package index.
/// 包索引中的索引条目。
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct IndexEntry {
    name: String,
    versions: Vec<String>,
    description: String,
}

/// Per-version metadata for a package.
/// 包的每版本元数据。
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct VersionMetadata {
    version: String,
    #[serde(default)]
    nar_hash: Option<String>,
    #[serde(default)]
    file_hash: Option<String>,
    #[serde(default)]
    dependencies: HashMap<String, String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
}

/// Full package metadata (all versions).
/// 包的完整元数据（所有版本）。
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct PackageMetadata {
    name: String,
    versions: Vec<VersionMetadata>,
}

// =============================================================================
// In-memory registry state / 内存中的注册表状态
// =============================================================================

struct RegistryState {
    data_dir: PathBuf,
    /// Cached index for fast responses. / 缓存的索引以快速响应。
    index: Vec<IndexEntry>,
    /// Cached per-package metadata. / 缓存的每包元数据。
    packages: HashMap<String, PackageMetadata>,
}

impl RegistryState {
    fn load(data_dir: &Path) -> Self {
        let mut state = Self {
            data_dir: data_dir.to_path_buf(),
            index: Vec::new(),
            packages: HashMap::new(),
        };
        state.reload_index();
        state
    }

    /// Reload the index from disk. / 从磁盘重新加载索引。
    fn reload_index(&mut self) {
        let index_path = self.data_dir.join("v1").join("index.json");
        if let Ok(content) = fs::read_to_string(&index_path)
            && let Ok(entries) = serde_json::from_str::<Vec<IndexEntry>>(&content)
        {
            self.index = entries;
        }
    }

    /// Load a package's metadata from disk. / 从磁盘加载包的元数据。
    fn load_package(&mut self, name: &str) -> Option<&PackageMetadata> {
        if self.packages.contains_key(name) {
            return self.packages.get(name);
        }
        let pkg_path = self
            .data_dir
            .join("v1")
            .join("packages")
            .join(format!("{name}.json"));
        if let Ok(content) = fs::read_to_string(&pkg_path)
            && let Ok(meta) = serde_json::from_str::<PackageMetadata>(&content)
        {
            self.packages.insert(name.to_string(), meta);
            return self.packages.get(name);
        }
        None
    }

    /// Save a package's metadata to disk and update the index.
    /// 将包元数据保存到磁盘并更新索引。
    fn save_package(&mut self, meta: PackageMetadata) -> Result<(), String> {
        let name = meta.name.clone();
        let pkg_dir = self.data_dir.join("v1").join("packages");
        fs::create_dir_all(&pkg_dir).map_err(|e| format!("failed to create packages dir: {e}"))?;

        let pkg_path = pkg_dir.join(format!("{name}.json"));
        let content = serde_json::to_string_pretty(&meta).map_err(|e| format!("serialize: {e}"))?;
        fs::write(&pkg_path, &content).map_err(|e| format!("write: {e}"))?;

        // Update index
        let versions: Vec<String> = meta.versions.iter().map(|v| v.version.clone()).collect();
        let description = meta
            .versions
            .first()
            .map(|v| v.description.clone())
            .unwrap_or_default();

        // Update or insert in index
        if let Some(entry) = self.index.iter_mut().find(|e| e.name == name) {
            entry.versions = versions;
            entry.description = description;
        } else {
            self.index.push(IndexEntry {
                name: name.clone(),
                versions,
                description,
            });
        }

        // Save index
        self.save_index()?;

        // Update cache
        self.packages.insert(name, meta);

        Ok(())
    }

    fn save_index(&self) -> Result<(), String> {
        let index_dir = self.data_dir.join("v1");
        fs::create_dir_all(&index_dir).map_err(|e| format!("failed to create v1 dir: {e}"))?;
        let index_path = index_dir.join("index.json");
        let content =
            serde_json::to_string_pretty(&self.index).map_err(|e| format!("serialize: {e}"))?;
        fs::write(&index_path, &content).map_err(|e| format!("write: {e}"))?;
        Ok(())
    }

    /// Search packages by name substring (case-insensitive).
    /// 按名称子串搜索包（不区分大小写）。
    fn search(&self, query: &str) -> Vec<&IndexEntry> {
        let q = query.to_lowercase();
        self.index
            .iter()
            .filter(|e| e.name.to_lowercase().contains(&q))
            .collect()
    }
}

// =============================================================================
// HTTP response helpers / HTTP 响应辅助
// =============================================================================

fn json_response(body: &str, status: &str) -> String {
    let len = body.len();
    format!("{status}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\n\r\n{body}")
}

fn ok_json(body: &str) -> String {
    json_response(body, "HTTP/1.1 200 OK")
}

fn not_found_json(body: &str) -> String {
    json_response(body, "HTTP/1.1 404 Not Found")
}

fn bad_request(body: &str) -> String {
    json_response(body, "HTTP/1.1 400 Bad Request")
}

fn unauthorized() -> String {
    json_response(
        r#"{"error":"publish requires bearer authentication"}"#,
        "HTTP/1.1 401 Unauthorized",
    )
}

fn method_not_allowed() -> String {
    let body = r#"{"error":"method not allowed"}"#;
    json_response(body, "HTTP/1.1 405 Method Not Allowed")
}

fn text_response(body: &str, status: &str) -> String {
    let len = body.len();
    format!("{status}\r\nContent-Type: text/plain\r\nContent-Length: {len}\r\n\r\n{body}")
}

/// Binary file response (returns headers string, body written separately).
fn binary_headers(len: usize, content_type: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\nCache-Control: public, max-age=31536000, immutable\r\n\r\n"
    )
}

// =============================================================================
// Route handlers / 路由处理器
// =============================================================================

fn handle_index(_state: &RwLock<RegistryState>) -> String {
    let body = serde_json::json!({
        "registry": "n3v3",
        "version": "1.0",
        "apiVersions": ["v1"],
        "endpoints": {
            "index": "/v1/index.json",
            "search": "/v1/search?q={query}",
            "package": "/v1/packages/{name}",
            "version": "/v1/packages/{name}/{version}",
            "publish": "POST /v1/packages/{name}",
            "narinfo": "/v1/{hash}.narinfo",
            "nar": "/v1/nar/{hash}.nar"
        }
    });
    ok_json(&body.to_string())
}

fn handle_health() -> String {
    text_response("OK", "HTTP/1.1 200 OK")
}

fn handle_v1_index(state: &RwLock<RegistryState>) -> String {
    let state = state.read().unwrap();
    let body = serde_json::to_string(&state.index).unwrap_or_else(|_| "[]".to_string());
    ok_json(&body)
}

fn handle_v1_search(query: &str, state: &RwLock<RegistryState>) -> String {
    let state = state.read().unwrap();
    if query.is_empty() {
        let body = serde_json::json!({"results": [], "total": 0});
        return ok_json(&body.to_string());
    }
    let results: Vec<&IndexEntry> = state.search(query);
    let total = results.len();
    let body = serde_json::json!({
        "results": results,
        "total": total,
    });
    ok_json(&body.to_string())
}

fn handle_v1_package(name: &str, state: &RwLock<RegistryState>) -> String {
    if !is_valid_package_name(name) {
        return bad_request(&serde_json::json!({"error": "invalid package name"}).to_string());
    }
    let mut state = state.write().unwrap();
    match state.load_package(name) {
        Some(meta) => {
            let body = serde_json::to_string(meta).unwrap_or_default();
            ok_json(&body)
        }
        None => {
            let body = serde_json::json!({"error": format!("package '{name}' not found")});
            not_found_json(&body.to_string())
        }
    }
}

fn handle_v1_package_version(name: &str, version: &str, state: &RwLock<RegistryState>) -> String {
    if !is_valid_package_name(name) {
        return bad_request(&serde_json::json!({"error": "invalid package name"}).to_string());
    }
    let mut state = state.write().unwrap();
    match state.load_package(name) {
        Some(meta) => match meta.versions.iter().find(|v| v.version == version) {
            Some(ver_meta) => {
                let body = serde_json::to_string(ver_meta).unwrap_or_default();
                ok_json(&body)
            }
            None => {
                let body = serde_json::json!({
                    "error": format!("version '{version}' not found for package '{name}'")
                });
                not_found_json(&body.to_string())
            }
        },
        None => {
            let body = serde_json::json!({"error": format!("package '{name}' not found")});
            not_found_json(&body.to_string())
        }
    }
}

fn handle_v1_publish(name: &str, body: &str, state: &RwLock<RegistryState>) -> String {
    if !is_valid_package_name(name) {
        return bad_request(&serde_json::json!({"error": "invalid package name"}).to_string());
    }
    if body.len() > MAX_REQUEST_BODY_SIZE {
        return bad_request(&serde_json::json!({"error": "request body is too large"}).to_string());
    }

    let version: VersionMetadata = match serde_json::from_str(body) {
        Ok(version) => version,
        Err(error) => {
            return bad_request(
                &serde_json::json!({"error": format!("invalid request body: {error}")}).to_string(),
            );
        }
    };
    if let Err(error) = validate_version_metadata(&version) {
        return bad_request(&serde_json::json!({"error": error}).to_string());
    }
    save_published_version(name, version, state)
}

fn save_published_version(
    name: &str,
    mut version: VersionMetadata,
    state: &RwLock<RegistryState>,
) -> String {
    let mut state = state.write().unwrap();
    let mut package = state
        .load_package(name)
        .cloned()
        .unwrap_or_else(|| PackageMetadata {
            name: name.to_string(),
            versions: Vec::new(),
        });
    if package
        .versions
        .iter()
        .any(|existing| existing.version == version.version)
    {
        let error = format!("version {} already exists", version.version);
        return bad_request(&serde_json::json!({"error": error}).to_string());
    }
    if version.published_at.is_none() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        version.published_at = Some(timestamp.to_string());
    }
    package.versions.insert(0, version);
    package.name = name.to_string();

    match state.save_package(package) {
        Ok(()) => {
            let body = serde_json::json!({"status": "published", "name": name});
            ok_json(&body.to_string())
        }
        Err(error) => {
            let body = serde_json::json!({"error": error});
            json_response(&body.to_string(), "HTTP/1.1 500 Internal Server Error")
        }
    }
}

// =============================================================================
// Legacy backward-compat handlers / 向后兼容的旧版处理器
// =============================================================================

fn handle_legacy_packages_json(data_dir: &Path) -> String {
    let index_path = data_dir.join("packages.json");
    match fs::read_to_string(&index_path) {
        Ok(content) => ok_json(&content),
        Err(_) => ok_json("[]"),
    }
}

fn handle_legacy_package(data_dir: &Path, name: &str) -> String {
    if !is_valid_package_name(name) {
        return bad_request(&serde_json::json!({"error": "invalid package name"}).to_string());
    }
    let pkg_path = data_dir.join("packages").join(format!("{name}.json"));
    match fs::read_to_string(&pkg_path) {
        Ok(content) => ok_json(&content),
        Err(_) => {
            let body = serde_json::json!({"error": format!("package '{name}' not found")});
            not_found_json(&body.to_string())
        }
    }
}

// =============================================================================
// Request router / 请求路由
// =============================================================================

fn parse_query_string(path: &str) -> (&str, HashMap<String, String>) {
    let (base, qs) = path.split_once('?').unwrap_or((path, ""));
    let params: HashMap<String, String> = qs
        .split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect();
    (base, params)
}

fn has_publish_access(authorization: Option<&str>, token: &str) -> bool {
    authorization.and_then(|value| value.strip_prefix("Bearer ")) == Some(token)
}

fn route_request(
    method: &str,
    path: &str,
    body: &str,
    authorization: Option<&str>,
    token: &str,
    data_dir: &Path,
    state: &RwLock<RegistryState>,
) -> (String, Option<Vec<u8>>) {
    let (base_path, query) = parse_query_string(path);

    // Binary cache / NAR serving
    if let Some(hash) = base_path.strip_prefix("/v1/nar/") {
        if method == "GET" && hash.ends_with(".nar") {
            let hash = hash.trim_end_matches(".nar");
            return handle_nar_download(hash, data_dir);
        }
        return (not_found_json(r#"{"error":"invalid nar path"}"#), None);
    }
    if let Some(hash) = base_path.strip_prefix("/v1/")
        && hash.ends_with(".narinfo")
        && method == "GET"
    {
        let hash = hash.trim_end_matches(".narinfo");
        return (handle_narinfo(hash, data_dir), None);
    }

    // v1 API / v1 API
    if base_path == "/v1/index.json" && method == "GET" {
        return (handle_v1_index(state), None);
    }
    if base_path == "/v1/search" && method == "GET" {
        let q = query.get("q").cloned().unwrap_or_default();
        return (handle_v1_search(&q, state), None);
    }
    if let Some(rest) = base_path.strip_prefix("/v1/packages/") {
        let segments: Vec<&str> = rest.split('/').collect();
        match segments.len() {
            1 => {
                let name = segments[0];
                if method == "GET" {
                    return (handle_v1_package(name, state), None);
                }
                if method == "POST" {
                    if !has_publish_access(authorization, token) {
                        return (unauthorized(), None);
                    }
                    return (handle_v1_publish(name, body, state), None);
                }
                return (method_not_allowed(), None);
            }
            2 => {
                if method == "GET" {
                    return (
                        handle_v1_package_version(segments[0], segments[1], state),
                        None,
                    );
                }
                return (method_not_allowed(), None);
            }
            _ => {}
        }
    }

    // Legacy endpoints
    if path == "/" && method == "GET" {
        return (handle_index(state), None);
    }
    if path == "/health" && method == "GET" {
        return (handle_health(), None);
    }
    if path == "/packages.json" && method == "GET" {
        return (handle_legacy_packages_json(data_dir), None);
    }
    if let Some(name) = path.strip_prefix("/packages/") {
        if method == "GET" {
            return (handle_legacy_package(data_dir, name), None);
        }
        if method == "POST" {
            if !has_publish_access(authorization, token) {
                return (unauthorized(), None);
            }
            return (handle_v1_publish(name, body, state), None);
        }
        return (method_not_allowed(), None);
    }

    (not_found_json(r#"{"error":"not found"}"#), None)
}

// =============================================================================
// NAR / Binary cache handlers / NAR / 二进制缓存处理器
// =============================================================================

/// Serve a NAR archive file.
/// 提供 NAR 归档文件。
fn handle_nar_download(hash: &str, data_dir: &Path) -> (String, Option<Vec<u8>>) {
    if !is_valid_cache_hash(hash) {
        return (not_found_json(r#"{"error":"invalid nar hash"}"#), None);
    }
    let nar_path = data_dir.join("nar").join(format!("{hash}.nar"));
    match fs::read(&nar_path) {
        Ok(data) => {
            let headers = binary_headers(data.len(), "application/x-nix-archive");
            (headers, Some(data))
        }
        Err(_) => (not_found_json(r#"{"error":"nar not found"}"#), None),
    }
}

/// Serve a narinfo metadata file.
/// 提供 narinfo 元数据文件。
fn handle_narinfo(hash: &str, data_dir: &Path) -> String {
    if !is_valid_cache_hash(hash) {
        return not_found_json(r#"{"error":"invalid narinfo hash"}"#);
    }
    let narinfo_path = data_dir.join(format!("{hash}.narinfo"));
    match fs::read_to_string(&narinfo_path) {
        Ok(content) => {
            let len = content.len();
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {len}\r\nCache-Control: public, max-age=31536000, immutable\r\n\r\n{content}"
            )
        }
        Err(_) => {
            let body = serde_json::json!({"error": "narinfo not found"});
            not_found_json(&body.to_string())
        }
    }
}

fn read_bounded_line(
    reader: &mut impl BufRead,
    max_bytes: usize,
    label: &str,
) -> Result<String, String> {
    let mut line = Vec::new();
    reader
        .take((max_bytes + 1) as u64)
        .read_until(b'\n', &mut line)
        .map_err(|error| format!("failed to read {label}: {error}"))?;
    if line.len() > max_bytes {
        return Err(format!("{label} exceeds {max_bytes} bytes"));
    }
    if !line.ends_with(b"\n") {
        return Err(format!("incomplete {label}"));
    }
    String::from_utf8(line).map_err(|error| format!("{label} is not UTF-8: {error}"))
}

fn read_request(
    reader: &mut impl BufRead,
) -> Result<(String, String, String, Option<String>), String> {
    let request_line = read_bounded_line(reader, MAX_REQUEST_LINE_SIZE, "request line")?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "request method is missing".to_string())?
        .to_string();
    let path = parts
        .next()
        .ok_or_else(|| "request path is missing".to_string())?
        .to_string();

    let mut content_length = 0usize;
    let mut authorization = None;
    let mut header_bytes = 0usize;
    let mut header_count = 0usize;
    loop {
        let header_line = read_bounded_line(reader, MAX_HEADER_LINE_SIZE, "request header")?;
        header_bytes += header_line.len();
        if header_bytes > MAX_HEADER_BYTES {
            return Err(format!("request headers exceed {MAX_HEADER_BYTES} bytes"));
        }
        let header = header_line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }
        header_count += 1;
        if header_count > MAX_HEADER_COUNT {
            return Err(format!("request headers exceed {MAX_HEADER_COUNT} entries"));
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value
                .trim()
                .parse()
                .map_err(|error| format!("invalid Content-Length: {error}"))?;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("authorization")
        {
            match authorization {
                Some(_) => return Err("duplicate Authorization header".to_string()),
                None => {
                    authorization = Some(value.trim_start_matches([' ', '\t']).to_string());
                }
            }
        }
    }
    if content_length > MAX_REQUEST_BODY_SIZE {
        return Err(format!(
            "request body exceeds {MAX_REQUEST_BODY_SIZE} bytes"
        ));
    }
    let mut body = vec![0u8; content_length];
    reader
        .read_exact(&mut body)
        .map_err(|error| format!("failed to read request body: {error}"))?;
    let body =
        String::from_utf8(body).map_err(|error| format!("request body is not UTF-8: {error}"))?;
    Ok((method, path, body, authorization))
}

fn set_connection_timeouts(stream: &TcpStream) -> Result<(), String> {
    stream
        .set_read_timeout(Some(CONNECTION_TIMEOUT))
        .map_err(|error| format!("failed to set registry connection read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(CONNECTION_TIMEOUT))
        .map_err(|error| format!("failed to set registry connection write timeout: {error}"))
}

fn handle_connection(
    mut stream: TcpStream,
    data_dir: &Path,
    state: &RwLock<RegistryState>,
    token: &str,
) -> Result<(), String> {
    set_connection_timeouts(&stream)?;
    let (method, path, body, authorization) = {
        let mut reader = BufReader::new(&mut stream);
        read_request(&mut reader)?
    };
    let (headers, binary_body) = route_request(
        &method,
        &path,
        &body,
        authorization.as_deref(),
        token,
        data_dir,
        state,
    );
    stream
        .write_all(headers.as_bytes())
        .map_err(|error| format!("failed to write response headers: {error}"))?;
    if let Some(binary) = binary_body {
        stream
            .write_all(&binary)
            .map_err(|error| format!("failed to write response body: {error}"))?;
    }
    Ok(())
}

fn bind_loopback(host: IpAddr, port: u16) -> Result<(SocketAddr, TcpListener), String> {
    if !host.is_loopback() {
        return Err(format!(
            "refusing public registry bind {host}: use a loopback address"
        ));
    }
    let address = SocketAddr::new(host, port);
    let listener = TcpListener::bind(address)
        .map_err(|error| format!("failed to bind to {address}: {error}"))?;
    Ok((address, listener))
}

struct ConnectionPermit {
    active: Arc<AtomicUsize>,
}

impl ConnectionPermit {
    fn try_acquire(active: &Arc<AtomicUsize>) -> Option<Self> {
        let mut count = active.load(Ordering::Acquire);
        loop {
            if count >= MAX_CONCURRENT_CONNECTIONS {
                return None;
            }
            match active.compare_exchange_weak(
                count,
                count + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(Self {
                        active: Arc::clone(active),
                    });
                }
                Err(observed) => count = observed,
            }
        }
    }
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

fn require_registry_token(token: Option<String>) -> Result<String, String> {
    let token = token.ok_or_else(|| {
        "N3V3_REGISTRY_TOKEN must be set to serve or publish packages".to_string()
    })?;
    if token.is_empty()
        || token.len() > MAX_HEADER_LINE_SIZE - "Authorization: Bearer \r\n".len()
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b',')
    {
        return Err(
            "N3V3_REGISTRY_TOKEN must be a non-empty header-sized visible ASCII value (no commas)"
                .to_string(),
        );
    }
    Ok(token)
}

pub(super) fn read_registry_token() -> Result<String, String> {
    require_registry_token(std::env::var("N3V3_REGISTRY_TOKEN").ok())
}

pub fn run(dir: &str, host: IpAddr, port: u16) -> Result<(), String> {
    let token = Arc::new(read_registry_token()?);
    let (address, listener) = bind_loopback(host, port)?;
    let data_dir = PathBuf::from(dir);
    fs::create_dir_all(&data_dir)
        .map_err(|error| format!("failed to create registry directory: {error}"))?;
    fs::create_dir_all(data_dir.join("v1").join("packages"))
        .map_err(|error| format!("failed to create v1 directories: {error}"))?;
    let state = Arc::new(RwLock::new(RegistryState::load(&data_dir)));
    let active = Arc::new(AtomicUsize::new(0));

    output::info(&format!("Registry server listening on http://{address}"));
    output::info(&format!("Serving packages from {}", data_dir.display()));
    output::info("API v1 endpoints available under /v1/");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let Some(permit) = ConnectionPermit::try_acquire(&active) else {
                    output::warning("registry connection limit reached; dropping connection");
                    continue;
                };
                let data_dir = data_dir.clone();
                let state = state.clone();
                let token = Arc::clone(&token);
                if let Err(error) = std::thread::Builder::new().spawn(move || {
                    let _permit = permit;
                    if let Err(error) = handle_connection(stream, &data_dir, &state, &token) {
                        output::warning(&format!("request error: {error}"));
                    }
                }) {
                    output::warning(&format!("failed to spawn registry connection: {error}"));
                }
            }
            Err(error) => output::warning(&format!("connection error: {error}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_state() -> (TempDir, RwLock<RegistryState>) {
        let dir = tempfile::tempdir().expect("temp dir");
        let state = RwLock::new(RegistryState::load(dir.path()));
        (dir, state)
    }

    fn route_http_request(
        request: &str,
        data_dir: &Path,
        state: &RwLock<RegistryState>,
        token: &str,
    ) -> Result<String, String> {
        let (method, path, body, authorization) =
            read_request(&mut BufReader::new(request.as_bytes()))?;
        Ok(route_request(
            &method,
            &path,
            &body,
            authorization.as_deref(),
            token,
            data_dir,
            state,
        )
        .0)
    }

    #[test]
    fn read_request_valid_route_and_body_remain_available() {
        let request = b"POST /v1/packages/demo HTTP/1.1\r\nContent-Length: 2\r\n\r\nok";
        let parsed = read_request(&mut BufReader::new(request.as_slice())).unwrap();
        assert_eq!(
            parsed,
            ("POST".into(), "/v1/packages/demo".into(), "ok".into(), None)
        );
    }

    #[test]
    fn read_request_at_header_count_limit_is_accepted() {
        let request = format!(
            "GET /health HTTP/1.1\r\n{}\r\n",
            "X-Key: value\r\n".repeat(MAX_HEADER_COUNT)
        );
        let parsed = read_request(&mut BufReader::new(request.as_bytes())).unwrap();
        assert_eq!(parsed.1, "/health");
    }

    #[test]
    fn read_request_overlong_request_line_is_rejected() {
        let request = format!(
            "GET /{} HTTP/1.1\r\n\r\n",
            "x".repeat(MAX_REQUEST_LINE_SIZE)
        );
        let error = read_request(&mut BufReader::new(request.as_bytes())).unwrap_err();
        assert!(error.contains("request line exceeds"));
    }

    #[test]
    fn read_request_overlong_header_is_rejected() {
        let request = format!(
            "GET /health HTTP/1.1\r\nX-Long: {}\r\n\r\n",
            "x".repeat(MAX_HEADER_LINE_SIZE)
        );
        let error = read_request(&mut BufReader::new(request.as_bytes())).unwrap_err();
        assert!(error.contains("request header exceeds"));
    }

    #[test]
    fn read_request_excess_total_header_bytes_are_rejected() {
        let header = format!("X-Large: {}\r\n", "x".repeat(MAX_HEADER_LINE_SIZE / 2));
        let request = format!(
            "GET /health HTTP/1.1\r\n{}\r\n",
            header.repeat(MAX_HEADER_BYTES / header.len() + 1)
        );
        let error = read_request(&mut BufReader::new(request.as_bytes())).unwrap_err();
        assert!(error.contains("request headers exceed") && error.contains("bytes"));
    }

    #[test]
    fn read_request_excess_header_count_is_rejected() {
        let request = format!(
            "GET /health HTTP/1.1\r\n{}\r\n",
            "X-Key: value\r\n".repeat(MAX_HEADER_COUNT + 1)
        );
        let error = read_request(&mut BufReader::new(request.as_bytes())).unwrap_err();
        assert!(error.contains("entries"));
    }

    #[test]
    fn read_request_oversized_body_is_rejected_before_reading() {
        let request = format!(
            "POST /v1/packages/demo HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_REQUEST_BODY_SIZE + 1
        );
        let error = read_request(&mut BufReader::new(request.as_bytes())).unwrap_err();
        assert!(error.contains("request body exceeds"));
    }

    #[test]
    fn read_request_duplicate_authorization_is_rejected() {
        let request = b"POST /v1/packages/demo HTTP/1.1\r\nAuthorization: Bearer one\r\nauthorization: Bearer two\r\n\r\n";
        let error = read_request(&mut BufReader::new(request.as_slice())).unwrap_err();
        assert_eq!(error, "duplicate Authorization header");
    }

    #[test]
    fn read_request_oversized_authorization_is_rejected() {
        let request = format!(
            "POST /v1/packages/demo HTTP/1.1\r\nAuthorization: Bearer {}\r\n\r\n",
            "x".repeat(MAX_HEADER_LINE_SIZE)
        );
        let error = read_request(&mut BufReader::new(request.as_bytes())).unwrap_err();
        assert!(error.contains("request header exceeds"));
    }

    #[test]
    fn set_connection_timeouts_stalled_peer_has_read_and_write_bounds() {
        let (_address, listener) =
            bind_loopback(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 0).unwrap();
        let _client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        set_connection_timeouts(&server).unwrap();
        assert_eq!(server.read_timeout().unwrap(), Some(CONNECTION_TIMEOUT));
        assert_eq!(server.write_timeout().unwrap(), Some(CONNECTION_TIMEOUT));
    }

    #[test]
    fn connection_permit_limit_and_drop_release_slot() {
        let active = Arc::new(AtomicUsize::new(0));
        let mut permits: Vec<_> = (0..MAX_CONCURRENT_CONNECTIONS)
            .map(|_| ConnectionPermit::try_acquire(&active).unwrap())
            .collect();
        assert!(ConnectionPermit::try_acquire(&active).is_none());
        assert_eq!(active.load(Ordering::Acquire), MAX_CONCURRENT_CONNECTIONS);
        drop(permits.pop());
        assert!(ConnectionPermit::try_acquire(&active).is_some());
        drop(permits);
        assert_eq!(active.load(Ordering::Acquire), 0);
    }

    #[test]
    fn bind_loopback_local_address_binds_locally() {
        let (address, listener) =
            bind_loopback(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 0).unwrap();
        assert!(address.ip().is_loopback());
        assert!(listener.local_addr().unwrap().ip().is_loopback());
    }

    #[test]
    fn bind_loopback_non_loopback_address_fails_closed() {
        for host in [
            IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
            IpAddr::V4(std::net::Ipv4Addr::new(192, 0, 2, 1)),
        ] {
            let error = bind_loopback(host, 0).unwrap_err();
            assert!(error.contains("refusing public registry bind"));
        }
    }

    #[test]
    fn require_registry_token_missing_or_invalid_fails_closed() {
        assert!(require_registry_token(None).is_err());
        for invalid in ["", " contains-space", "bad\r\nheader", "comma,value"] {
            assert!(require_registry_token(Some(invalid.to_string())).is_err());
        }
        assert!(require_registry_token(Some("x".repeat(MAX_HEADER_LINE_SIZE))).is_err());
    }

    #[test]
    fn publish_missing_or_wrong_authorization_does_not_mutate_state() {
        let (dir, state) = test_state();
        let token = std::process::id().to_string();
        let body = r#"{"version":"1.0.0"}"#;
        for (path, auth) in [
            ("/v1/packages/demo", String::new()),
            (
                "/v1/packages/demo",
                format!("Authorization: Bearer {token}-wrong\r\n"),
            ),
            (
                "/v1/packages/demo",
                format!("Authorization: Bearer {token} \r\n"),
            ),
            (
                "/v1/packages/demo",
                format!("Authorization: bearer {token}\r\n"),
            ),
            ("/packages/demo", String::new()),
        ] {
            let request = format!(
                "POST {path} HTTP/1.1\r\nOrigin: http://untrusted.invalid\r\n{auth}Content-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let response = route_http_request(&request, dir.path(), &state, &token).unwrap();
            assert!(response.starts_with("HTTP/1.1 401 Unauthorized"));
            assert!(!response.contains("Access-Control-Allow-Origin"));
            let index = route_http_request(
                "GET /v1/index.json HTTP/1.1\r\n\r\n",
                dir.path(),
                &state,
                &token,
            )
            .unwrap();
            assert!(index.ends_with("\r\n\r\n[]"));
            assert!(!dir.path().join("v1/index.json").exists());
        }
    }

    #[test]
    fn publish_correct_authorization_updates_state_and_get_remains_public() {
        let (dir, state) = test_state();
        let token = std::process::id().to_string();
        let body = r#"{"version":"1.0.0"}"#;
        let request = format!(
            "POST /v1/packages/demo HTTP/1.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let response = route_http_request(&request, dir.path(), &state, &token).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(!response.contains("Access-Control-Allow-Origin"));
        assert!(dir.path().join("v1/index.json").exists());
        let response = route_http_request(
            "GET /v1/packages/demo HTTP/1.1\r\n\r\n",
            dir.path(),
            &state,
            &token,
        )
        .unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("1.0.0"));
    }

    #[test]
    fn publish_valid_local_metadata_updates_registry() {
        let (_dir, state) = test_state();
        let metadata = serde_json::json!({
            "name": "hello",
            "version": "1.0.0",
            "dependencies": {},
            "description": "A local package",
            "manifest": "name = \"hello\""
        });
        let response = handle_v1_publish("hello", &metadata.to_string(), &state);
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(state.read().unwrap().index.len(), 1);
    }

    #[test]
    fn publish_invalid_metadata_is_rejected_without_state_change() {
        let (_dir, state) = test_state();
        let invalid = serde_json::json!({
            "version": "../1.0",
            "dependencies": {},
            "description": ""
        });
        let response = handle_v1_publish("hello", &invalid.to_string(), &state);
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(state.read().unwrap().index.is_empty());
    }

    #[test]
    fn package_read_rejects_invalid_name_without_loading_other_metadata() {
        let (dir, state) = test_state();
        let packages = dir.path().join("v1").join("packages");
        std::fs::create_dir_all(&packages).unwrap();
        std::fs::write(
            dir.path().join("v1").join("index.json"),
            "[{\"name\":\"private\",\"versions\":[],\"description\":\"secret\"}]",
        )
        .unwrap();
        let response = handle_v1_package("..", &state);
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(!response.contains("secret"));
    }

    #[test]
    fn cache_hash_invalid_characters_do_not_alias_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("nar")).unwrap();
        std::fs::write(dir.path().join("nar").join("abc.nar"), "archive").unwrap();
        let (response, body) = handle_nar_download("abc?", dir.path());
        assert!(response.starts_with("HTTP/1.1 404 Not Found"));
        assert!(body.is_none());
    }

    #[test]
    fn test_registry_publish_and_search() {
        let (_dir, state) = test_state();

        let meta = PackageMetadata {
            name: "hello".to_string(),
            versions: vec![VersionMetadata {
                version: "1.0.0".to_string(),
                nar_hash: Some("sha256-abc".to_string()),
                file_hash: None,
                dependencies: HashMap::new(),
                description: "A test package".to_string(),
                license: Some("MIT".to_string()),
                published_at: Some("1234567890".to_string()),
            }],
        };

        state.write().unwrap().save_package(meta).expect("save");

        let guard = state.read().unwrap();
        let results = guard.search("hello");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "hello");
        assert!(results[0].versions.contains(&"1.0.0".to_string()));
        drop(guard);

        let mut guard = state.write().unwrap();
        let pkg = guard.load_package("hello");
        assert!(pkg.is_some());
        let pkg = pkg.unwrap();
        assert_eq!(pkg.versions[0].version, "1.0.0");
    }

    #[test]
    fn test_registry_search_case_insensitive() {
        let (_dir, state) = test_state();

        let meta = PackageMetadata {
            name: "MyPackage".to_string(),
            versions: vec![VersionMetadata {
                version: "1.0.0".to_string(),
                nar_hash: None,
                file_hash: None,
                dependencies: HashMap::new(),
                description: String::new(),
                license: None,
                published_at: None,
            }],
        };

        state.write().unwrap().save_package(meta).expect("save");
        let guard = state.read().unwrap();
        assert_eq!(guard.search("mypackage").len(), 1);
        assert_eq!(guard.search("nonexistent").len(), 0);
    }

    #[test]
    fn test_registry_multi_version() {
        let (_dir, state) = test_state();
        let meta = PackageMetadata {
            name: "lib".to_string(),
            versions: vec![
                VersionMetadata {
                    version: "2.0.0".to_string(),
                    nar_hash: Some("sha256-xyz".to_string()),
                    file_hash: None,
                    dependencies: HashMap::new(),
                    description: "v2".to_string(),
                    license: None,
                    published_at: None,
                },
                VersionMetadata {
                    version: "1.0.0".to_string(),
                    nar_hash: Some("sha256-abc".to_string()),
                    file_hash: None,
                    dependencies: HashMap::new(),
                    description: "v1".to_string(),
                    license: None,
                    published_at: None,
                },
            ],
        };
        state.write().unwrap().save_package(meta).expect("save");
        let mut guard = state.write().unwrap();
        let pkg = guard.load_package("lib").cloned().unwrap();
        assert_eq!(pkg.versions.len(), 2);
    }
}
