//! Action metadata and the first typed host-operation boundary.
//! 动作元数据与首个类型化宿主操作边界。

use crate::Span;
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
#[cfg(unix)]
use std::ffi::CString;
use std::fmt;
#[cfg(unix)]
use std::fs::{File, OpenOptions};
use std::io;
#[cfg(unix)]
use std::io::Read;
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(unix)]
use std::path::Component;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::Arc;
/// A compact summary of the effects an intrinsic may perform.
/// 内置函数可能执行的副作用摘要。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EffectSummary(u16);

impl EffectSummary {
    /// No host effect.
    /// 无宿主副作用。
    pub const PURE: Self = Self(0);
    /// Some host interaction occurs, without a narrower classification.
    /// 发生宿主交互，但没有更细的分类。
    pub const HOST: Self = Self(1 << 0);
    /// File contents are read.
    /// 读取文件内容。
    pub const FILE_READ: Self = Self(Self::HOST.0 | (1 << 1));
    /// File contents are written.
    /// 写入文件内容。
    pub const FILE_WRITE: Self = Self(Self::HOST.0 | (1 << 2));
    /// A process is started or controlled.
    /// 启动或控制进程。
    pub const PROCESS: Self = Self(Self::HOST.0 | (1 << 3));
    /// A background task is created or controlled.
    /// 创建或控制后台任务。
    pub const TASK: Self = Self(Self::HOST.0 | (1 << 4));
    /// Data is written to an output device.
    /// 向输出设备写入数据。
    pub const OUTPUT: Self = Self(Self::HOST.0 | (1 << 5));
    /// A network request is performed.
    /// 执行网络请求。
    pub const NETWORK: Self = Self(Self::HOST.0 | (1 << 6));
    /// Time or timer state is observed.
    /// 读取时间或计时器状态。
    pub const TIME: Self = Self(Self::HOST.0 | (1 << 7));
    /// A signal or terminal is controlled.
    /// 控制信号或终端。
    pub const DEVICE: Self = Self(Self::HOST.0 | (1 << 8));

    /// Return true when this summary contains no effects.
    /// 判断摘要是否不包含副作用。
    pub const fn is_pure(self) -> bool {
        self.0 == Self::PURE.0
    }

    /// Return true when any host interaction is present.
    /// 判断摘要是否包含宿主交互。
    pub const fn is_host_effect(self) -> bool {
        self.contains(Self::HOST)
    }

    /// Return true when all bits in `other` are present.
    /// 判断摘要是否包含 `other` 的全部标记。
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Combine two summaries.
    /// 合并两个副作用摘要。
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// The host operation represented by an action plan.
/// 动作计划表示的宿主操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HostOpKind {
    /// Read a UTF-8 file into a language string.
    /// 将 UTF-8 文件读为语言字符串。
    ReadFile,
}

impl fmt::Display for HostOpKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadFile => formatter.write_str("readFile"),
        }
    }
}

/// A pure, owned description of one process execution.
/// 一个纯的、拥有所有数据的进程执行描述。
///
/// `ProcessPlan` contains no process handles and performs no host operation.
/// It is an internal migration boundary for the existing `Command` and
/// `Pipeline` runtime values; it is not a language-level `Value`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessPlan {
    stages: Vec<ProcessStage>,
    boundary_redirects: Vec<ProcessRedirect>,
}

impl ProcessPlan {
    /// Create a process plan from owned stages and boundary redirects.
    /// 从拥有所有权的阶段和边界重定向创建进程计划。
    pub fn new(stages: Vec<ProcessStage>, boundary_redirects: Vec<ProcessRedirect>) -> Self {
        Self {
            stages,
            boundary_redirects,
        }
    }

    /// Return process stages in execution order.
    /// 按执行顺序返回进程阶段。
    pub fn stages(&self) -> &[ProcessStage] {
        &self.stages
    }

    /// Return redirects attached to the process boundary.
    /// 返回附加到进程边界的重定向。
    pub fn boundary_redirects(&self) -> &[ProcessRedirect] {
        &self.boundary_redirects
    }
}

/// An owned process stage in a `ProcessPlan`.
/// `ProcessPlan` 中拥有所有权的进程阶段。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessStage {
    program: String,
    args: Vec<String>,
    cwd: Option<String>,
    stdin: Option<String>,
    env: BTreeMap<String, String>,
    redirects: Vec<ProcessRedirect>,
}

impl ProcessStage {
    /// Create a process stage without host-side execution.
    /// 创建不执行宿主操作的进程阶段。
    pub fn new(
        program: String,
        args: Vec<String>,
        cwd: Option<String>,
        stdin: Option<String>,
        env: BTreeMap<String, String>,
        redirects: Vec<ProcessRedirect>,
    ) -> Self {
        Self {
            program,
            args,
            cwd,
            stdin,
            env,
            redirects,
        }
    }

    /// Return the executable name or path.
    /// 返回可执行文件名称或路径。
    pub fn program(&self) -> &str {
        &self.program
    }

    /// Return arguments in their original order.
    /// 按原始顺序返回参数。
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Return the optional working directory.
    /// 返回可选工作目录。
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    /// Return the optional configured stdin text.
    /// 返回可选的已配置 stdin 文本。
    pub fn stdin(&self) -> Option<&str> {
        self.stdin.as_deref()
    }

    /// Return the environment entries in stable key order.
    /// 按稳定键顺序返回环境变量。
    pub fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    /// Return redirects attached to this stage.
    /// 返回附加到此阶段的重定向。
    pub fn redirects(&self) -> &[ProcessRedirect] {
        &self.redirects
    }
}

/// The stream affected by a process redirect.
/// 进程重定向影响的流。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProcessStream {
    /// Standard input.
    /// 标准输入。
    Stdin,
    /// Standard output.
    /// 标准输出。
    Stdout,
    /// Standard error.
    /// 标准错误。
    Stderr,
}

/// An owned process redirect.
/// 拥有所有权的进程重定向。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessRedirect {
    stream: ProcessStream,
    path: PathBuf,
}

impl ProcessRedirect {
    /// Create a process redirect without opening its path.
    /// 创建不打开路径的进程重定向。
    pub fn new(stream: ProcessStream, path: PathBuf) -> Self {
        Self { stream, path }
    }

    /// Return the redirected stream.
    /// 返回被重定向的流。
    pub const fn stream(&self) -> ProcessStream {
        self.stream
    }

    /// Return the raw redirect path.
    /// 返回原始重定向路径。
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Metadata shared by effect inference and action planning.
/// 由副作用推断与动作规划共享的元数据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntrinsicMetadata<'a> {
    /// The queried builtin name.
    /// 查询到的内置函数名称。
    pub name: &'a str,
    /// Effect summary for this builtin.
    /// 此内置函数的副作用摘要。
    pub effects: EffectSummary,
    /// Typed host operation when one is available in the current migration phase.
    /// 当前迁移阶段已经有类型化宿主操作时返回它。
    pub host_op: Option<HostOpKind>,
}

/// Look up effect metadata for a builtin name.
/// 查询内置函数的副作用元数据。
///
/// The fallback rules intentionally preserve the existing effect boundary:
/// unknown `io.*` builtins remain effectful, while registered pure constructors
/// and inspectors return `EffectSummary::PURE`.
pub fn intrinsic_metadata(name: &str) -> Option<IntrinsicMetadata<'_>> {
    let effects = if matches!(
        name,
        "print"
            | "println"
            | "read"
            | "write"
            | "cmd"
            | "env"
            | "exec"
            | "run"
            | "sh"
            | "ls"
            | "exists"
            | "pwd"
            | "home"
    ) {
        match name {
            "print" | "println" => EffectSummary::OUTPUT,
            "read" => EffectSummary::FILE_READ,
            "write" => EffectSummary::FILE_WRITE,
            "exec" | "run" | "sh" | "cmd" => EffectSummary::PROCESS,
            _ => EffectSummary::HOST,
        }
    } else {
        let (namespace, operation) = name.split_once('.')?;
        match namespace {
            "fetch" => EffectSummary::NETWORK,
            "io" if is_pure_io_operation(operation) => EffectSummary::PURE,
            "io" => classify_io_operation(operation),
            _ => return None,
        }
    };

    Some(IntrinsicMetadata {
        name,
        effects,
        host_op: (name == "io.readFile").then_some(HostOpKind::ReadFile),
    })
}

/// Check whether a builtin performs a host effect.
/// 判断内置函数是否执行宿主副作用。
pub fn is_effectful_builtin(name: &str) -> bool {
    intrinsic_metadata(name).is_some_and(|metadata| metadata.effects.is_host_effect())
}

fn is_pure_io_operation(operation: &str) -> bool {
    matches!(
        operation,
        "processSuccess"
            | "processStdout"
            | "processCode"
            | "processStderr"
            | "command"
            | "commandWith"
            | "commandWithRedirects"
            | "pipeline"
            | "pipelineWithRedirects"
            | "redirectStdoutPath"
            | "redirectStderrPath"
            | "redirectStdinPath"
            | "taskCommand"
            | "taskPipeline"
            | "eventMap"
            | "eventFilter"
            | "reactive"
            | "watchFile"
            | "every"
            | "hashString"
            | "currentSystem"
            | "streamList"
            | "streamMap"
            | "streamFilter"
            | "streamTake"
            | "streamDrop"
    )
}

fn classify_io_operation(operation: &str) -> EffectSummary {
    if matches!(
        operation,
        "isTTY" | "terminalSize" | "readKey" | "input" | "readPassword"
    ) {
        return EffectSummary::DEVICE;
    }
    if matches!(operation, "streamLines" | "streamBytes") {
        return EffectSummary::FILE_READ;
    }
    if operation == "streamCommand" {
        return EffectSummary::PROCESS;
    }
    if operation == "streamWithTimeout" {
        return EffectSummary::TIME;
    }
    if operation == "readFile" || operation.starts_with("read") {
        return EffectSummary::FILE_READ;
    }
    if operation.starts_with("write") || operation.starts_with("append") {
        return EffectSummary::FILE_WRITE;
    }
    if operation.starts_with("exec") || operation == "run" || operation == "shell" {
        return EffectSummary::PROCESS;
    }
    if operation.starts_with("spawn")
        || operation.starts_with("await")
        || operation == "cancel"
        || operation == "poll"
        || operation == "jobs"
    {
        return EffectSummary::TASK;
    }
    if operation.contains("Stream") || operation.starts_with("stream") {
        return EffectSummary::HOST;
    }
    if operation.contains("Signal") || operation == "setRawMode" || operation == "resetTerminal" {
        return EffectSummary::DEVICE;
    }
    if operation == "retry" || operation == "ensure" || operation == "waitAnyJob" {
        return EffectSummary::TIME;
    }
    EffectSummary::HOST
}

/// A typed host operation in an action plan.
/// 动作计划中的类型化宿主操作。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostOp {
    /// Read a UTF-8 file from a string path.
    /// 从字符串路径读取 UTF-8 文件。
    ReadFile { path: String },
}

impl HostOp {
    /// Return the operation kind.
    /// 返回操作种类。
    pub const fn kind(&self) -> HostOpKind {
        match self {
            Self::ReadFile { .. } => HostOpKind::ReadFile,
        }
    }

    /// Return the operation's effect summary.
    /// 返回操作的副作用摘要。
    pub const fn effects(&self) -> EffectSummary {
        match self {
            Self::ReadFile { .. } => EffectSummary::FILE_READ,
        }
    }

    /// Execute this operation through an explicit host boundary.
    /// 通过显式宿主边界执行此操作。
    pub fn execute(&self, host: &mut dyn Host) -> Result<HostValue, HostError> {
        match self {
            Self::ReadFile { path } => host.read_file(path).map(HostValue::String),
        }
    }
}

/// A plan for one currently supported action.
/// 当前支持的单个动作计划。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionPlan {
    span: Span,
    operation: HostOp,
}

impl ActionPlan {
    /// Create a plan from a typed host operation.
    /// 从类型化宿主操作创建动作计划。
    pub fn new(span: Span, operation: HostOp) -> Self {
        Self { span, operation }
    }

    /// Return the source span that produced this plan.
    /// 返回生成此计划的源码 span。
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Return the planned operation.
    /// 返回计划中的操作。
    pub fn operation(&self) -> &HostOp {
        &self.operation
    }

    /// Return the operation kind.
    /// 返回计划中的操作种类。
    pub const fn host_op_kind(&self) -> HostOpKind {
        self.operation.kind()
    }

    /// Return the effects represented by this plan.
    /// 返回此计划表示的副作用。
    pub const fn effects(&self) -> EffectSummary {
        self.operation.effects()
    }

    /// Execute the plan through an explicit host.
    /// 通过显式宿主执行此计划。
    pub fn execute(&self, host: &mut dyn Host) -> Result<HostValue, HostError> {
        self.operation.execute(host)
    }
}

/// Values returned by typed host operations.
/// 类型化宿主操作返回的值。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostValue {
    /// UTF-8 text returned by `ReadFile`.
    /// `ReadFile` 返回的 UTF-8 文本。
    String(String),
}

/// Errors raised at the host boundary.
/// 宿主边界产生的错误。
#[derive(Debug)]
pub enum HostError {
    /// A file could not be read.
    /// 文件无法读取。
    ReadFile { path: String, source: io::Error },
    /// The configured host root could not be used.
    /// 配置的宿主根目录不可用。
    InvalidRoot { path: String, source: io::Error },
    /// A requested path escaped the configured host root.
    /// 请求路径越过了配置的宿主根目录。
    PathDenied { path: String, root: String },
    /// This host implementation is unavailable on the target platform.
    /// 当前目标平台不可用此宿主实现。
    UnsupportedPlatform,
}

impl HostError {
    fn read_file(path: &str, source: io::Error) -> Self {
        Self::ReadFile {
            path: path.to_string(),
            source,
        }
    }

    fn invalid_root(path: &Path, source: io::Error) -> Self {
        Self::InvalidRoot {
            path: path.to_string_lossy().into_owned(),
            source,
        }
    }

    fn path_denied(path: &str, root: &Path) -> Self {
        Self::PathDenied {
            path: path.to_string(),
            root: root.to_string_lossy().into_owned(),
        }
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadFile { path, source } => {
                write!(formatter, "failed to read file {path:?}: {source}")
            }
            Self::InvalidRoot { path, source } => {
                write!(formatter, "invalid host root {path:?}: {source}")
            }
            Self::PathDenied { path, root } => {
                write!(
                    formatter,
                    "refusing to read path {path:?} outside host root {root:?}"
                )
            }
            Self::UnsupportedPlatform => {
                formatter.write_str("OS host is unavailable on this platform")
            }
        }
    }
}

impl Error for HostError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ReadFile { source, .. } | Self::InvalidRoot { source, .. } => Some(source),
            Self::PathDenied { .. } | Self::UnsupportedPlatform => None,
        }
    }
}

/// Explicit host capability used by action plans.
/// 动作计划使用的显式宿主能力。
pub trait Host {
    /// Read a UTF-8 file.
    /// 读取 UTF-8 文件。
    fn read_file(&mut self, path: &str) -> Result<String, HostError>;
}

/// Host implementation backed by a configured operating-system root.
/// 基于显式配置操作系统根目录的宿主实现。
///
/// Unix reads are anchored to an opened root directory and reject symlinks
/// component-by-component. Unsupported platforms fail closed.
#[derive(Clone, Debug)]
pub struct OsHost {
    root: PathBuf,
    #[cfg(unix)]
    root_dir: Arc<File>,
}

impl OsHost {
    /// Create a host that can read only existing files below `root`.
    /// 创建只能读取 `root` 下已有文件的宿主。
    ///
    /// Relative paths are required. Parent traversal, absolute paths, and
    /// symlinks are rejected. Non-Unix targets return `UnsupportedPlatform`
    /// until an equivalent race-resistant directory-handle API is available.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, HostError> {
        let requested_root = root.as_ref();
        #[cfg(not(unix))]
        {
            let _ = requested_root;
            return Err(HostError::UnsupportedPlatform);
        }
        #[cfg(unix)]
        {
            let canonical_root = std::fs::canonicalize(requested_root)
                .map_err(|source| HostError::invalid_root(requested_root, source))?;
            if !canonical_root.is_dir() {
                return Err(HostError::invalid_root(
                    requested_root,
                    io::Error::new(io::ErrorKind::InvalidInput, "host root is not a directory"),
                ));
            }
            let root_dir = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&canonical_root)
                .map_err(|source| HostError::invalid_root(requested_root, source))?;
            Ok(Self {
                root: canonical_root,
                root_dir: Arc::new(root_dir),
            })
        }
    }

    #[cfg(unix)]
    fn relative_components<'a>(
        &self,
        path: &'a str,
    ) -> Result<Vec<&'a std::ffi::OsStr>, HostError> {
        let requested = Path::new(path);
        if requested.is_absolute() {
            return Err(HostError::path_denied(path, &self.root));
        }
        let mut components = Vec::new();
        for component in requested.components() {
            if matches!(component, Component::ParentDir) {
                return Err(HostError::path_denied(path, &self.root));
            }
            if let Component::Normal(component) = component {
                components.push(component);
            }
        }
        if components.is_empty() {
            return Err(HostError::read_file(
                path,
                io::Error::new(io::ErrorKind::InvalidInput, "path must name a file"),
            ));
        }
        Ok(components)
    }

    #[cfg(unix)]
    fn open_relative_file(&self, path: &str) -> Result<File, HostError> {
        let components = self.relative_components(path)?;
        let mut directory = self
            .root_dir
            .try_clone()
            .map_err(|source| HostError::read_file(path, source))?;
        for (index, component) in components.iter().enumerate() {
            let next = open_relative_component(&directory, component)
                .map_err(|source| HostError::read_file(path, source))?;
            if index + 1 == components.len() {
                return Ok(next);
            }
            directory = next;
        }
        Err(HostError::read_file(
            path,
            io::Error::new(io::ErrorKind::InvalidInput, "path must name a file"),
        ))
    }
}

#[cfg(unix)]
fn open_relative_component(directory: &File, component: &std::ffi::OsStr) -> io::Result<File> {
    let name = CString::new(component.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
    let fd = unsafe {
        // SAFETY: `name` is NUL-terminated and remains alive for this call.
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` is a newly owned descriptor returned by `openat`.
    Ok(unsafe { File::from_raw_fd(fd) })
}

impl Host for OsHost {
    fn read_file(&mut self, path: &str) -> Result<String, HostError> {
        #[cfg(not(unix))]
        {
            let _ = path;
            return Err(HostError::UnsupportedPlatform);
        }
        #[cfg(unix)]
        {
            let mut file = self.open_relative_file(path)?;
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|source| HostError::read_file(path, source))?;
            Ok(content)
        }
    }
}

/// Deterministic fake host for action-plan tests and dry runs.
/// 用于动作计划测试和 dry run 的确定性 fake host。
#[derive(Debug, Default)]
pub struct FakeHost {
    files: HashMap<String, String>,
    read_calls: Vec<String>,
}

impl FakeHost {
    /// Create an empty fake host.
    /// 创建空 fake host。
    pub fn new() -> Self {
        Self::default()
    }

    /// Register file content returned by future reads.
    /// 注册后续读取时返回的文件内容。
    pub fn insert_file(&mut self, path: impl Into<String>, content: impl Into<String>) {
        self.files.insert(path.into(), content.into());
    }

    /// Return read paths in execution order.
    /// 按执行顺序返回读取过的路径。
    pub fn read_calls(&self) -> &[String] {
        &self.read_calls
    }
}

impl Host for FakeHost {
    fn read_file(&mut self, path: &str) -> Result<String, HostError> {
        self.read_calls.push(path.to_string());
        self.files.get(path).cloned().ok_or_else(|| {
            HostError::read_file(
                path,
                io::Error::new(io::ErrorKind::NotFound, "fake file is not registered"),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intrinsic_metadata_classifies_read_file() {
        let metadata = intrinsic_metadata("io.readFile").expect("readFile metadata");
        assert_eq!(metadata.effects, EffectSummary::FILE_READ);
        assert_eq!(metadata.host_op, Some(HostOpKind::ReadFile));
        assert!(is_effectful_builtin("io.readFile"));
        assert_eq!(
            intrinsic_metadata("io.command")
                .expect("pure command metadata")
                .effects,
            EffectSummary::PURE
        );
        assert!(!is_effectful_builtin("io.command"));
        for name in [
            "io.isTTY",
            "io.terminalSize",
            "io.readKey",
            "io.input",
            "io.readPassword",
        ] {
            assert!(is_effectful_builtin(name), "{name} must remain effectful");
        }
        for name in [
            "io.streamLines",
            "io.streamCommand",
            "io.streamBytes",
            "io.streamWithTimeout",
            "io.liveCurrent",
            "io.liveCancel",
        ] {
            assert!(is_effectful_builtin(name), "{name} must remain effectful");
        }
    }
    #[test]
    fn fake_host_executes_read_file_plan_and_records_call() {
        let span = Span::from_usize(3, 14);
        let plan = ActionPlan::new(
            span,
            HostOp::ReadFile {
                path: "config.n3v3".to_string(),
            },
        );
        let mut host = FakeHost::new();
        host.insert_file("config.n3v3", "host-content");

        let value = plan.execute(&mut host).expect("fake read should succeed");

        assert_eq!(value, HostValue::String("host-content".to_string()));
        assert_eq!(host.read_calls(), ["config.n3v3"]);
        assert_eq!(plan.span(), span);
    }

    #[test]
    fn fake_host_reports_missing_file_with_source_error() {
        let plan = ActionPlan::new(
            Span::DUMMY,
            HostOp::ReadFile {
                path: "missing.n3v3".to_string(),
            },
        );
        let mut host = FakeHost::new();

        let error = plan.execute(&mut host).expect_err("missing fake file");

        assert!(error.to_string().contains("missing.n3v3"));
        assert!(error.source().is_some());
    }
    #[test]
    fn os_host_rejects_unscoped_paths() {
        let root = std::env::current_dir().expect("test current directory");
        let mut host = OsHost::new(&root).expect("test host root");
        let absolute_path = root.join("Cargo.toml").to_string_lossy().into_owned();

        assert!(matches!(
            host.read_file(&absolute_path),
            Err(HostError::PathDenied { .. })
        ));
        assert!(matches!(
            host.read_file("../Cargo.toml"),
            Err(HostError::PathDenied { .. })
        ));
    }
}
