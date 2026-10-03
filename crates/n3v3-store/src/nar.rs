//! NAR (Nix ARchive) format implementation.
//! NAR (Nix ARchive) 格式实现。
//!
//! NAR is a deterministic archive format used by Nix for storing build outputs.
//! It captures files, directories, and symlinks in a reproducible way.
//! NAR 是 Nix 使用的确定性归档格式，用于存储构建输出。
//! 它以可重现的方式捕获文件、目录和符号链接。
//!
//! ## Format Specification 格式规范
//!
//! NAR uses a simple string-based format:
//! NAR 使用简单的基于字符串的格式：
//!
//! - All strings are length-prefixed (8 bytes, little-endian)
//! - 所有字符串都有长度前缀（8 字节，小端序）
//! - Strings are padded to 8-byte alignment
//! - 字符串填充到 8 字节对齐
//! - The format is recursive for directories
//! - 目录采用递归格式

use n3v3_derive::Hash;
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

/// NAR magic string. / NAR 魔术字符串。
const NAR_MAGIC: &str = "nix-archive-1";

/// Mode bits NAR restores for executable and plain files.
/// NAR 为可执行文件与普通文件恢复的权限位。
const EXECUTABLE_MODE: u32 = 0o755;
const REGULAR_MODE: u32 = 0o644;

/// Whether the entry carries NAR's `executable` flag.
/// 条目是否带 NAR 的 `executable` 标记。
#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

/// Non-Unix platforms have no POSIX mode bits, so the flag stays unset.
/// 非 Unix 平台没有 POSIX 权限位，因此不设置该标记。
#[cfg(not(unix))]
fn is_executable(_metadata: &fs::Metadata) -> bool {
    false
}

/// Restore POSIX mode bits; a no-op where the platform has none.
/// 恢复 POSIX 权限位；平台没有权限位时为空操作。
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

/// Errors during NAR operations.
/// NAR 操作期间的错误。
#[derive(Debug, Error)]
pub enum NarError {
    /// I/O error. / I/O 错误。
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Invalid NAR format. / 无效的 NAR 格式。
    #[error("invalid NAR format: {0}")]
    InvalidFormat(String),

    /// Unexpected end of archive. / 归档意外结束。
    #[error("unexpected end of archive")]
    UnexpectedEof,

    /// Path traversal attempt. / 路径遍历尝试。
    #[error("path traversal attempt detected")]
    PathTraversal,
}

/// Create missing ancestors without following an existing symlink.
fn ensure_safe_parent(dest: &Path) -> Result<(), NarError> {
    let Some(parent) = dest.parent() else {
        return Ok(());
    };
    let mut current = PathBuf::new();
    for component in parent.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::CurDir => {
                current.push(component.as_os_str());
                continue;
            }
            Component::ParentDir => return Err(NarError::PathTraversal),
            Component::Normal(part) => current.push(part),
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(NarError::PathTraversal);
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(NarError::InvalidFormat(format!(
                    "archive parent is not a directory: {}",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(&current)?,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Reject occupied destinations, including dangling symlinks.
fn ensure_unoccupied(dest: &Path) -> Result<(), NarError> {
    match fs::symlink_metadata(dest) {
        Ok(_) => Err(NarError::InvalidFormat(format!(
            "archive destination already exists: {}",
            dest.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// NAR writer for creating archives.
/// 用于创建归档的 NAR 写入器。
pub struct NarWriter<W: Write> {
    writer: W,
    bytes_written: u64,
}

impl<W: Write> NarWriter<W> {
    /// Create a new NAR writer.
    /// 创建新的 NAR 写入器。
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            bytes_written: 0,
        }
    }

    /// Write a path (file, directory, or symlink) to the archive.
    /// 将路径（文件、目录或符号链接）写入归档。
    pub fn write_path(&mut self, path: &Path) -> Result<(), NarError> {
        self.write_str(NAR_MAGIC)?;
        self.write_entry(path)
    }

    /// Write an entry (recursive).
    /// 写入条目（递归）。
    fn write_entry(&mut self, path: &Path) -> Result<(), NarError> {
        self.write_str("(")?;

        let metadata = fs::symlink_metadata(path)?;

        if metadata.is_symlink() {
            self.write_str("type")?;
            self.write_str("symlink")?;
            self.write_str("target")?;
            let target = fs::read_link(path)?;
            self.write_str(&target.to_string_lossy())?;
        } else if metadata.is_file() {
            self.write_str("type")?;
            self.write_str("regular")?;

            // Check if executable
            // 检查是否可执行
            if is_executable(&metadata) {
                self.write_str("executable")?;
                self.write_str("")?;
            }

            self.write_str("contents")?;
            let contents = fs::read(path)?;
            self.write_bytes(&contents)?;
        } else if metadata.is_dir() {
            self.write_str("type")?;
            self.write_str("directory")?;

            // Read and sort directory entries for determinism
            // 读取并排序目录条目以确保确定性
            let mut entries: Vec<_> = fs::read_dir(path)?.filter_map(|e| e.ok()).collect();
            entries.sort_by_key(|e| e.file_name());

            for entry in entries {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                // Skip special entries
                // 跳过特殊条目
                if name_str == "." || name_str == ".." {
                    continue;
                }

                self.write_str("entry")?;
                self.write_str("(")?;
                self.write_str("name")?;
                self.write_str(&name_str)?;
                self.write_str("node")?;
                self.write_entry(&entry.path())?;
                self.write_str(")")?;
            }
        }

        self.write_str(")")?;
        Ok(())
    }

    /// Write a length-prefixed string with padding.
    /// 写入带填充的长度前缀字符串。
    fn write_str(&mut self, s: &str) -> Result<(), NarError> {
        self.write_bytes(s.as_bytes())
    }

    /// Write length-prefixed bytes with padding.
    /// 写入带填充的长度前缀字节。
    fn write_bytes(&mut self, data: &[u8]) -> Result<(), NarError> {
        let len = data.len() as u64;
        self.writer.write_all(&len.to_le_bytes())?;
        self.writer.write_all(data)?;
        self.bytes_written += 8 + len;

        // Pad to 8-byte alignment
        // 填充到 8 字节对齐
        let padding = (8 - (len % 8)) % 8;
        if padding > 0 {
            self.writer.write_all(&vec![0u8; padding as usize])?;
            self.bytes_written += padding;
        }

        Ok(())
    }

    /// Get the number of bytes written.
    /// 获取已写入的字节数。
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    /// Finish writing and return the inner writer.
    /// 完成写入并返回内部写入器。
    pub fn finish(self) -> W {
        self.writer
    }
}

/// NAR reader for extracting archives.
/// 用于提取归档的 NAR 读取器。
pub struct NarReader<R: Read> {
    reader: R,
    bytes_read: u64,
    /// Lookahead buffer for peeked strings.
    /// 用于预读字符串的缓冲区。
    lookahead: Option<String>,
}

impl<R: Read> NarReader<R> {
    /// Create a new NAR reader.
    /// 创建新的 NAR 读取器。
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            bytes_read: 0,
            lookahead: None,
        }
    }

    /// Extract the archive to a destination path.
    /// 将归档提取到目标路径。
    pub fn extract(&mut self, dest: &Path) -> Result<(), NarError> {
        let magic = self.read_str()?;
        if magic != NAR_MAGIC {
            return Err(NarError::InvalidFormat(format!(
                "expected magic '{}', got '{}'",
                NAR_MAGIC, magic
            )));
        }

        self.extract_entry(dest)?;
        let mut trailing = [0u8; 1];
        if self.reader.read(&mut trailing)? != 0 {
            return Err(NarError::InvalidFormat(
                "trailing data after root entry".into(),
            ));
        }
        Ok(())
    }

    /// Extract a single entry (recursive).
    /// 提取单个条目（递归）。
    fn extract_entry(&mut self, dest: &Path) -> Result<(), NarError> {
        self.expect_str("(")?;
        self.expect_str("type")?;

        let entry_type = self.read_str()?;
        match entry_type.as_str() {
            "regular" => self.extract_regular(dest)?,
            "directory" => self.extract_directory(dest)?,
            "symlink" => self.extract_symlink(dest)?,
            _ => {
                return Err(NarError::InvalidFormat(format!(
                    "unknown entry type: {}",
                    entry_type
                )));
            }
        }

        Ok(())
    }

    /// Extract a regular file.
    /// 提取普通文件。
    fn extract_regular(&mut self, dest: &Path) -> Result<(), NarError> {
        let mut is_executable = false;
        let mut has_contents = false;

        loop {
            match self.read_str()?.as_str() {
                "executable" if !is_executable && !has_contents => {
                    self.expect_str("")?;
                    is_executable = true;
                }
                "contents" if !has_contents => {
                    let contents = self.read_bytes()?;
                    ensure_safe_parent(dest)?;
                    let mut file = OpenOptions::new().write(true).create_new(true).open(dest)?;
                    file.write_all(&contents)?;
                    has_contents = true;
                }
                ")" if has_contents => {
                    set_mode(
                        dest,
                        if is_executable {
                            EXECUTABLE_MODE
                        } else {
                            REGULAR_MODE
                        },
                    )?;
                    return Ok(());
                }
                tag => {
                    return Err(NarError::InvalidFormat(format!(
                        "unexpected or duplicate tag in regular file: {tag}"
                    )));
                }
            }
        }
    }

    /// Extract a directory.
    /// 提取目录。
    fn extract_directory(&mut self, dest: &Path) -> Result<(), NarError> {
        ensure_safe_parent(dest)?;
        fs::create_dir(dest)?;
        let mut names = HashSet::new();

        loop {
            let tag = self.read_str()?;
            match tag.as_str() {
                "entry" => {
                    self.expect_str("(")?;
                    self.expect_str("name")?;

                    let name = self.read_str()?;
                    if name.is_empty()
                        || name.contains('/')
                        || name.contains('\\')
                        || name.contains('\0')
                        || name == ".."
                        || name == "."
                    {
                        return Err(NarError::PathTraversal);
                    }
                    if !names.insert(name.clone()) {
                        return Err(NarError::InvalidFormat(format!(
                            "duplicate directory entry: {name}"
                        )));
                    }
                    self.expect_str("node")?;
                    let entry_path = dest.join(&name);
                    self.extract_entry(&entry_path)?;
                    self.expect_str(")")?;
                }
                ")" => {
                    return Ok(());
                }
                _ => {
                    return Err(NarError::InvalidFormat(format!(
                        "unexpected tag in directory: {}",
                        tag
                    )));
                }
            }
        }
    }

    /// Extract a symlink.
    /// 提取符号链接。
    fn extract_symlink(&mut self, dest: &Path) -> Result<(), NarError> {
        self.expect_str("target")?;
        let target = self.read_str()?;
        self.expect_str(")")?;
        ensure_safe_parent(dest)?;
        ensure_unoccupied(dest)?;

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, dest)?;

        #[cfg(windows)]
        {
            // On Windows, try to determine if target is a directory
            // 在 Windows 上，尝试确定目标是否为目录
            let target_path = dest.parent().unwrap_or(Path::new(".")).join(&target);
            if target_path.is_dir() {
                std::os::windows::fs::symlink_dir(&target, dest)?;
            } else {
                std::os::windows::fs::symlink_file(&target, dest)?;
            }
        }

        Ok(())
    }

    /// Read a length-prefixed string.
    /// 读取长度前缀字符串。
    fn read_str(&mut self) -> Result<String, NarError> {
        // Check lookahead first
        // 首先检查预读缓冲区
        if let Some(s) = self.lookahead.take() {
            return Ok(s);
        }
        let bytes = self.read_bytes()?;
        String::from_utf8(bytes).map_err(|e| NarError::InvalidFormat(e.to_string()))
    }

    /// Maximum size for a single NAR data element (10 MB).
    /// Prevents OOM on maliciously crafted archives with inflated length fields.
    const MAX_NAR_ELEMENT_SIZE: u64 = 10 * 1024 * 1024;

    /// Read length-prefixed bytes with padding.
    /// 读取带填充的长度前缀字节。
    fn read_bytes(&mut self) -> Result<Vec<u8>, NarError> {
        // Read length (8 bytes, little-endian)
        // 读取长度（8 字节，小端序）
        let mut len_buf = [0u8; 8];
        self.reader.read_exact(&mut len_buf)?;
        let len = u64::from_le_bytes(len_buf);
        self.bytes_read += 8;

        // Guard against maliciously large length fields that would OOM
        if len > Self::MAX_NAR_ELEMENT_SIZE {
            return Err(NarError::InvalidFormat(format!(
                "element size {len} exceeds maximum {}",
                Self::MAX_NAR_ELEMENT_SIZE
            )));
        }

        // Read data
        // 读取数据
        let data_len = usize::try_from(len).map_err(|_| {
            NarError::InvalidFormat(format!("data length {len} exceeds addressable memory"))
        })?;
        let mut data = vec![0u8; data_len];
        self.reader.read_exact(&mut data)?;
        self.bytes_read += len;

        // Read padding
        // 读取填充
        let padding = (8 - (len % 8)) % 8;
        if padding > 0 {
            let mut pad_buf = vec![0u8; padding as usize];
            self.reader.read_exact(&mut pad_buf)?;
            self.bytes_read += padding;
            if pad_buf.iter().any(|byte| *byte != 0) {
                return Err(NarError::InvalidFormat(
                    "non-zero NAR string padding".to_string(),
                ));
            }
        }

        Ok(data)
    }

    /// Expect a specific string.
    /// 期望一个特定的字符串。
    fn expect_str(&mut self, expected: &str) -> Result<(), NarError> {
        let actual = self.read_str()?;
        if actual != expected {
            Err(NarError::InvalidFormat(format!(
                "expected '{}', got '{}'",
                expected, actual
            )))
        } else {
            Ok(())
        }
    }

    /// Get the number of bytes read.
    /// 获取已读取的字节数。
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}

/// Compute the NAR hash of a path.
/// 计算路径的 NAR 哈希。
///
/// This creates a NAR archive in memory and returns its hash.
/// 这会在内存中创建 NAR 归档并返回其哈希。
pub fn hash_path(path: &Path) -> Result<Hash, NarError> {
    let mut buffer = Vec::new();
    let mut writer = NarWriter::new(&mut buffer);
    writer.write_path(path)?;
    Ok(Hash::of(&buffer))
}

/// Create a NAR archive of a path and return the bytes.
/// 创建路径的 NAR 归档并返回字节。
pub fn create_nar(path: &Path) -> Result<Vec<u8>, NarError> {
    let mut buffer = Vec::new();
    let mut writer = NarWriter::new(&mut buffer);
    writer.write_path(path)?;
    Ok(buffer)
}

/// Extract a NAR archive from bytes to a destination.
/// 从字节中提取 NAR 归档到目标位置。
pub fn extract_nar(data: &[u8], dest: &Path) -> Result<(), NarError> {
    let mut reader = NarReader::new(data);
    reader.extract(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn append_element(archive: &mut Vec<u8>, element: &[u8]) {
        archive.extend_from_slice(&(element.len() as u64).to_le_bytes());
        archive.extend_from_slice(element);
        archive.resize(archive.len() + (8 - element.len() % 8) % 8, 0);
    }

    fn append_regular_node(archive: &mut Vec<u8>, contents: &[u8]) {
        for value in [b"(".as_slice(), b"type", b"regular", b"contents"] {
            append_element(archive, value);
        }
        append_element(archive, contents);
        append_element(archive, b")");
    }

    fn directory_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive = Vec::new();
        for value in [NAR_MAGIC.as_bytes(), b"(", b"type", b"directory"] {
            append_element(&mut archive, value);
        }
        for (name, contents) in entries {
            for value in [b"entry".as_slice(), b"(", b"name"] {
                append_element(&mut archive, value);
            }
            append_element(&mut archive, name.as_bytes());
            append_element(&mut archive, b"node");
            append_regular_node(&mut archive, contents);
            append_element(&mut archive, b")");
        }
        append_element(&mut archive, b")");
        archive
    }
    use tempfile::TempDir;

    #[test]
    fn test_nar_regular_file() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let file_path = root.join("test.txt");
        fs::write(&file_path, b"Hello, NAR!").unwrap();

        // Create NAR
        let nar_data = create_nar(&file_path).unwrap();
        assert!(!nar_data.is_empty());

        // Extract NAR
        let extract_dir = TempDir::new().unwrap();
        let extract_root = extract_dir.path().canonicalize().unwrap();
        let extract_path = extract_root.join("extracted.txt");
        extract_nar(&nar_data, &extract_path).unwrap();

        // Verify contents
        let contents = fs::read_to_string(&extract_path).unwrap();
        assert_eq!(contents, "Hello, NAR!");
    }

    #[cfg(unix)]
    #[test]
    fn test_nar_executable_file() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let file_path = root.join("script.sh");
        fs::write(&file_path, b"#!/bin/sh\necho hello").unwrap();

        // Make executable
        set_mode(&file_path, EXECUTABLE_MODE).unwrap();

        // Create and extract NAR
        let nar_data = create_nar(&file_path).unwrap();

        let extract_dir = TempDir::new().unwrap();
        let extract_root = extract_dir.path().canonicalize().unwrap();
        let extract_path = extract_root.join("script.sh");
        extract_nar(&nar_data, &extract_path).unwrap();

        // Verify executable bit
        let metadata = fs::metadata(&extract_path).unwrap();
        assert!(is_executable(&metadata));
    }

    #[test]
    fn test_nar_directory() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let dir_path = root.join("mydir");
        fs::create_dir(&dir_path).unwrap();
        fs::write(dir_path.join("a.txt"), b"File A").unwrap();
        fs::write(dir_path.join("b.txt"), b"File B").unwrap();

        let subdir = dir_path.join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("c.txt"), b"File C").unwrap();

        // Create and extract NAR
        let nar_data = create_nar(&dir_path).unwrap();

        let extract_dir = TempDir::new().unwrap();
        let extract_root = extract_dir.path().canonicalize().unwrap();
        let extract_path = extract_root.join("extracted");
        extract_nar(&nar_data, &extract_path).unwrap();

        // Verify structure
        assert!(extract_path.is_dir());
        assert_eq!(
            fs::read_to_string(extract_path.join("a.txt")).unwrap(),
            "File A"
        );
        assert_eq!(
            fs::read_to_string(extract_path.join("b.txt")).unwrap(),
            "File B"
        );
        assert_eq!(
            fs::read_to_string(extract_path.join("subdir/c.txt")).unwrap(),
            "File C"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_nar_symlink() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let file_path = root.join("target.txt");
        fs::write(&file_path, b"Target content").unwrap();

        let link_path = root.join("link.txt");
        std::os::unix::fs::symlink("target.txt", &link_path).unwrap();

        // Create NAR of the symlink
        let nar_data = create_nar(&link_path).unwrap();

        let extract_dir = TempDir::new().unwrap();
        let extract_root = extract_dir.path().canonicalize().unwrap();
        let extract_path = extract_root.join("extracted_link");
        extract_nar(&nar_data, &extract_path).unwrap();

        // Verify it's a symlink pointing to the right target
        assert!(extract_path.is_symlink());
        assert_eq!(
            fs::read_link(&extract_path).unwrap().to_string_lossy(),
            "target.txt"
        );
    }

    #[test]
    fn test_nar_hash_determinism() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, b"Deterministic content").unwrap();

        // Hash should be the same for identical content
        let hash1 = hash_path(&file_path).unwrap();
        let hash2 = hash_path(&file_path).unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_nar_directory_sorting() {
        // Create directory with files in different order
        let temp1 = TempDir::new().unwrap();
        let dir1 = temp1.path().join("dir");
        fs::create_dir(&dir1).unwrap();
        fs::write(dir1.join("z.txt"), b"z").unwrap();
        fs::write(dir1.join("a.txt"), b"a").unwrap();
        fs::write(dir1.join("m.txt"), b"m").unwrap();

        let temp2 = TempDir::new().unwrap();
        let dir2 = temp2.path().join("dir");
        fs::create_dir(&dir2).unwrap();
        fs::write(dir2.join("a.txt"), b"a").unwrap();
        fs::write(dir2.join("m.txt"), b"m").unwrap();
        fs::write(dir2.join("z.txt"), b"z").unwrap();

        // Hashes should be identical (order doesn't matter, sorting does)
        let hash1 = hash_path(&dir1).unwrap();
        let hash2 = hash_path(&dir2).unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_nar_path_traversal_prevention() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let src = root.join("src");
        // Create a directory tree with various entry types
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("sub").join("safe.txt"), b"safe").unwrap();
        std::fs::write(src.join("root.txt"), b"root").unwrap();

        // Normal roundtrip should succeed
        let nar_data = create_nar(&src).unwrap();
        let out = root.join("out");
        extract_nar(&nar_data, &out).unwrap();
        assert!(out.join("sub").join("safe.txt").exists());
        assert!(out.join("root.txt").exists());
        assert_eq!(
            std::fs::read_to_string(out.join("root.txt")).unwrap(),
            "root"
        );

        // Corrupted/malformed NAR input should error
        let result = extract_nar(b"not a valid NAR archive", &root.join("bad"));
        assert!(result.is_err());

        // The path traversal guard in extract_directory checks for:
        //   name.contains('/') || name.contains('\\') || name == ".." || name == "."
        // These guards prevent malicious archives from writing outside dest.
    }

    #[test]
    fn extract_duplicate_directory_name_returns_error() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let archive = directory_archive(&[("same", b"first"), ("same", b"second")]);

        let error = extract_nar(&archive, &root.join("out")).unwrap_err();

        assert!(matches!(error, NarError::InvalidFormat(message) if message.contains("duplicate")));
    }

    #[test]
    fn extract_unsafe_directory_names_returns_path_traversal() {
        for (index, name) in [
            "/absolute",
            "../escape",
            "nested/file",
            "nested\\file",
            ".",
            "..",
            "",
        ]
        .into_iter()
        .enumerate()
        {
            let temp = TempDir::new().unwrap();
            let root = temp.path().canonicalize().unwrap();
            let archive = directory_archive(&[(name, b"payload")]);
            let error = extract_nar(&archive, &root.join(format!("out-{index}"))).unwrap_err();
            assert!(matches!(error, NarError::PathTraversal), "name: {name:?}");
        }
    }

    #[test]
    fn extract_existing_destination_does_not_clobber_content() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let source = root.join("source");
        let destination = root.join("destination");
        fs::write(&source, b"replacement").unwrap();
        fs::write(&destination, b"original").unwrap();
        let archive = create_nar(&source).unwrap();

        assert!(extract_nar(&archive, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"original");
    }

    #[test]
    fn extract_archive_with_trailing_data_returns_error() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let source = root.join("source");
        fs::write(&source, b"payload").unwrap();
        let mut archive = create_nar(&source).unwrap();
        archive.push(0);

        let error = extract_nar(&archive, &root.join("destination")).unwrap_err();
        assert!(matches!(error, NarError::InvalidFormat(message) if message.contains("trailing")));
    }

    #[cfg(unix)]
    #[test]
    fn extract_parent_symlink_does_not_escape_destination() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let source = root.join("source");
        let outside = root.join("outside");
        let parent_link = root.join("parent-link");
        fs::write(&source, b"payload").unwrap();
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, &parent_link).unwrap();
        let archive = create_nar(&source).unwrap();

        let error = extract_nar(&archive, &parent_link.join("escaped")).unwrap_err();

        assert!(matches!(error, NarError::PathTraversal));
        assert!(!outside.join("escaped").exists());
    }

    #[cfg(unix)]
    #[test]
    fn extract_directory_preserves_internal_symlink() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("target"), b"payload").unwrap();
        std::os::unix::fs::symlink("target", source.join("link")).unwrap();
        let archive = create_nar(&source).unwrap();
        let destination = root.join("destination");

        extract_nar(&archive, &destination).unwrap();

        assert_eq!(
            fs::read_link(destination.join("link")).unwrap(),
            Path::new("target")
        );
        assert_eq!(fs::read(destination.join("link")).unwrap(), b"payload");
    }
}
