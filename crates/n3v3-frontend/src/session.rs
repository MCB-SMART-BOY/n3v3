//! Compatibility frontend session for incremental and REPL-style consumers.
//! 面向增量场景与 REPL 的兼容前端会话。

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use n3v3_common::Span;
use n3v3_hir::{
    DefId, ExprKind, Import as HirImport, ImportKind as HirImportKind, ImportPathPrefix,
    ItemKind as HirItemKind, ModuleId, ModuleInfo, ModuleLoadError, ModuleLoader, ModulePath,
    Resolver, Ty, resolve_std_builtin_import, supports_canonical_std_import,
};
use n3v3_parser::parse;
use n3v3_syntax::{
    ImportDef, ImportItems, ItemKind, PathPrefix, PatternKind, SourceFile, Visibility,
};
use n3v3_typeck::TypeChecker;

use crate::{
    Diagnostic, Module, ModuleAnalysis, ModuleSemantics, collect_item_names_from_modules,
    collect_module_semantics, diagnostics_have_errors, format_type_with_names_map,
    read_diagnostic_source, rewrite_diagnostics_with_names,
};

const REPL_EXPR_BINDING_NAME: &str = "__expr__";
const REPL_TYPE_QUERY_BINDING_NAME: &str = "__type__";
const REPL_SOURCE_NAME: &str = "<repl>";
const REPL_TYPE_QUERY_SOURCE_NAME: &str = "<repl:type>";

/// Diagnostics attributed to one loaded module in a frontend session.
/// Frontend 会话中归属到单个已加载模块的诊断。
#[derive(Debug, Clone)]
pub struct SessionLoadedDiagnostics {
    /// Loaded module id.
    /// 已加载模块 ID。
    pub module_id: ModuleId,
    /// Backing source path on disk.
    /// 模块对应的磁盘路径。
    pub file_path: PathBuf,
    /// Source text read for diagnostic attribution.
    /// 用于诊断归属展示的源码文本。
    pub source: String,
    /// Final diagnostics for the module.
    /// 模块的最终诊断。
    pub diagnostics: Vec<Diagnostic>,
}

/// Dependency-first loaded module entry produced by one frontend session.
/// frontend 会话产出的依赖优先已加载模块条目。
#[derive(Debug, Clone)]
pub struct SessionLoadedModule {
    /// Loaded module id.
    /// 已加载模块 ID。
    pub module_id: ModuleId,
    /// Backing source path on disk.
    /// 模块对应的磁盘路径。
    pub file_path: PathBuf,
    /// Lowered HIR for this dependency when available.
    /// 当前依赖模块可用时的降级 HIR。
    pub module: Option<Module>,
    /// Final diagnostics and semantics for this dependency.
    /// 当前依赖模块的最终诊断与语义结果。
    pub analysis: ModuleAnalysis,
}

/// Dependency-first loaded module entry ready for HIR evaluation.
/// 可直接用于 HIR 求值的依赖优先已加载模块条目。
#[derive(Debug, Clone)]
pub struct SessionEvaluableModule {
    /// Loaded module id.
    /// 已加载模块 ID。
    pub module_id: ModuleId,
    /// Lowered HIR for this dependency.
    /// 当前依赖模块的降级 HIR。
    pub module: Module,
    /// Resolved method call targets needed before evaluation.
    /// 求值前需要的方法调用解析结果。
    pub method_resolutions: HashMap<Span, DefId>,
}

/// Inputs needed to build one in-memory module against the current session.
/// 基于当前会话构建单个内存模块时所需的输入。
#[derive(Debug, Clone, Default)]
pub struct SessionBuildInputs {
    /// Next available global definition id.
    /// 下一个可用的全局定义 ID。
    pub next_def_id: u32,
    /// Existing globals already visible to the caller.
    /// 调用方当前可见的既有全局定义。
    pub existing_globals: Vec<(String, DefId)>,
    /// Imported definitions already visible to the caller.
    /// 调用方当前可见的导入定义。
    pub imported_defs: Vec<(String, DefId)>,
    /// Imported namespace aliases already visible to the caller.
    /// 调用方当前可见的模块命名空间别名。
    pub imported_module_aliases: Vec<String>,
    /// Imported builtin items already visible to the caller.
    /// 调用方当前可见的 builtin 项导入。
    pub builtin_item_imports: Vec<(String, String)>,
    /// Imported builtin modules already visible to the caller.
    /// 调用方当前可见的 builtin 模块导入。
    pub builtin_module_imports: Vec<(String, String)>,
}

/// Frontend-owned module context for one in-memory input.
/// frontend 持有的单个内存输入模块上下文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionModuleContext {
    /// Root directory the module context was resolved against.
    /// 模块上下文解析时使用的根目录。
    pub root_dir: Option<PathBuf>,
    /// Resolved module path segments for relative imports.
    /// 用于相对导入的模块路径段。
    pub module_path: Vec<String>,
    /// Friendly module name for lowering/debugging.
    /// 用于 lowering/调试的模块名。
    pub module_name: String,
}

/// Frontend-owned snapshot of caller-visible semantic state.
/// frontend 持有的调用方可见语义状态快照。
#[derive(Debug, Clone, Default)]
pub struct SessionVisibleState {
    next_def_id: u32,
    existing_globals: HashMap<String, DefId>,
    imported_defs: HashMap<String, DefId>,
    imported_module_aliases: HashSet<String>,
    builtin_item_imports: HashMap<String, String>,
    builtin_module_imports: HashMap<String, String>,
}

/// Result of building one in-memory module through a frontend session.
/// 通过 frontend 会话构建单个内存模块的结果。
#[derive(Debug, Clone)]
pub struct SessionBuildResult {
    /// Lowered current module.
    /// 当前输入降级后的模块。
    pub module: Module,
    /// Newly loaded dependency modules caused by this input.
    /// 当前输入触发的新加载依赖模块。
    pub newly_loaded: Vec<ModuleId>,
    /// Next global definition id after lowering this input.
    /// 当前输入降级完成后的下一个全局定义 ID。
    pub next_def_id: u32,
    /// Global definitions introduced by the current module resolver.
    /// 当前模块解析器引入的全局定义。
    pub global_defs: HashMap<String, DefId>,
}

/// Result of preparing one in-memory module plus import bookkeeping.
/// 准备单个内存模块及其导入 bookkeeping 的结果。
#[derive(Debug, Clone)]
pub struct SessionPreparedModule {
    /// Lowered current module.
    /// 当前输入降级后的模块。
    pub module: Module,
    /// Newly loaded dependency modules caused by this input.
    /// 当前输入触发的新加载依赖模块。
    pub newly_loaded: Vec<ModuleId>,
    /// Next global definition id after lowering this input.
    /// 当前输入降级完成后的下一个全局定义 ID。
    pub next_def_id: u32,
    /// Global definitions introduced by the current module resolver.
    /// 当前模块解析器引入的全局定义。
    pub global_defs: HashMap<String, DefId>,
    /// User-visible bindings introduced by the current input.
    /// 当前输入引入的用户可见绑定。
    pub defined_bindings: Vec<SessionDefinedBinding>,
    /// Resolved import bookkeeping visible to the caller after this input.
    /// 当前输入完成后对调用方可见的导入 bookkeeping。
    pub resolved_imports: SessionResolvedImports,
}

/// Result of preparing and semantically checking one in-memory module.
/// 准备并通过语义检查的单个内存模块结果。
#[derive(Debug, Clone)]
pub struct SessionCheckedModule {
    /// Prepared current module plus import bookkeeping.
    /// 准备完成的当前模块及其导入 bookkeeping。
    pub prepared: SessionPreparedModule,
    /// Canonical semantic analysis for the current module.
    /// 当前模块的规范语义分析结果。
    pub analysis: ModuleAnalysis,
}

/// Result of parsing and semantically checking one in-memory source input.
/// 解析并通过语义检查的单个内存源码输入结果。
#[derive(Debug, Clone)]
pub struct SessionCheckedSource {
    /// Parsed source AST.
    /// 解析后的源码 AST。
    pub ast: SourceFile,
    /// Checked module plus import bookkeeping.
    /// 检查后的模块及其导入 bookkeeping。
    pub checked: SessionCheckedModule,
}

/// Resolved import bindings for one in-memory module.
/// 单个内存模块的导入绑定解析结果。
#[derive(Debug, Clone, Default)]
pub struct SessionResolvedImports {
    /// Name bindings imported into the current scope.
    /// 导入到当前作用域的名称绑定。
    pub bindings: Vec<(String, DefId)>,
    /// Namespace aliases introduced by module imports.
    /// 模块命名空间导入引入的别名。
    pub module_aliases: Vec<String>,
    /// Builtin item imports keyed by in-scope name.
    /// 作用域内名称到 builtin 全名的映射。
    pub builtin_item_imports: Vec<(String, String)>,
    /// Builtin module imports keyed by namespace alias.
    /// 命名空间别名到 builtin 模块前缀的映射。
    pub builtin_module_imports: Vec<(String, String)>,
}

/// One user-visible binding introduced by an in-memory input.
/// 单个内存输入引入的用户可见绑定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDefinedBinding {
    /// Binding name visible to the caller.
    /// 对调用方可见的绑定名称。
    pub name: String,
    /// Whether the binding was declared public.
    /// 绑定是否声明为公开。
    pub is_public: bool,
}

/// Normalized REPL source ready for frontend parsing/checking.
/// 已规范化、可直接交给 frontend 解析/检查的 REPL 源码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPreparedReplSource {
    /// Final source text after REPL-specific wrapping.
    /// 经过 REPL 特定包装后的最终源码。
    pub source: String,
    /// Whether definitions from this input should persist in session state.
    /// 当前输入中的定义是否应持久化到会话状态。
    pub persist_defs: bool,
}

/// One in-memory REPL input prepared by the frontend.
/// frontend 准备完成的单个内存 REPL 输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPreparedReplInput {
    /// User-facing source name used for CLI attribution.
    /// 用于 CLI 归因展示的用户可见源码名称。
    pub source_name: String,
    /// Final source text after REPL-specific wrapping.
    /// 经过 REPL 特定包装后的最终源码。
    pub source: String,
    /// Whether definitions from this input should persist in session state.
    /// 当前输入中的定义是否应持久化到会话状态。
    pub persist_defs: bool,
    /// Resolved module context for the prepared input.
    /// 该输入对应的已解析模块上下文。
    pub context: SessionModuleContext,
}

/// One file-backed REPL input prepared by the frontend.
/// frontend 准备完成的单个基于文件的 REPL 输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReplFileInput {
    /// User-facing source name used for CLI attribution.
    /// 用于 CLI 归因展示的用户可见源码名称。
    pub source_name: String,
    /// User-facing file path used for CLI attribution.
    /// 用于 CLI 归因展示的用户可见文件路径。
    pub file_path: PathBuf,
    /// Source text read from the file.
    /// 从文件中读取的源码文本。
    pub source: String,
    /// Resolved module context for the file-backed input.
    /// 该文件输入对应的已解析模块上下文。
    pub context: SessionModuleContext,
}

impl SessionVisibleState {
    /// Returns whether the visible semantic snapshot is empty.
    /// 返回可见语义快照是否为空。
    pub fn is_pristine(&self) -> bool {
        self.next_def_id == 0
            && self.existing_globals.is_empty()
            && self.imported_defs.is_empty()
            && self.imported_module_aliases.is_empty()
            && self.builtin_item_imports.is_empty()
            && self.builtin_module_imports.is_empty()
    }

    /// Borrow the next available global definition id tracked by this snapshot.
    /// 借用当前快照跟踪的下一个全局定义 ID。
    pub fn next_def_id(&self) -> u32 {
        self.next_def_id
    }

    /// Build compatibility frontend inputs against one session-level def-id counter.
    /// 基于当前快照和会话级 def-id 计数器构建兼容 frontend 输入。
    pub fn build_inputs(&self, session_next_def_id: u32) -> SessionBuildInputs {
        SessionBuildInputs {
            next_def_id: self.next_def_id.max(session_next_def_id),
            existing_globals: self
                .existing_globals
                .iter()
                .map(|(name, def_id)| (name.clone(), *def_id))
                .collect(),
            imported_defs: self
                .imported_defs
                .iter()
                .map(|(name, def_id)| (name.clone(), *def_id))
                .collect(),
            imported_module_aliases: self.imported_module_aliases.iter().cloned().collect(),
            builtin_item_imports: self
                .builtin_item_imports
                .iter()
                .map(|(name, builtin)| (name.clone(), builtin.clone()))
                .collect(),
            builtin_module_imports: self
                .builtin_module_imports
                .iter()
                .map(|(alias, prefix)| (alias.clone(), prefix.clone()))
                .collect(),
        }
    }

    /// Apply resolved import bookkeeping to this visible semantic snapshot.
    /// 将解析后的导入 bookkeeping 合并到当前可见语义快照。
    pub fn apply_resolved_imports(&mut self, resolved: &SessionResolvedImports) {
        for (name, def_id) in &resolved.bindings {
            self.imported_defs.insert(name.clone(), *def_id);
        }
        for alias in &resolved.module_aliases {
            self.imported_module_aliases.insert(alias.clone());
        }
        for (name, builtin) in &resolved.builtin_item_imports {
            self.builtin_item_imports
                .insert(name.clone(), builtin.clone());
        }
        for (alias, prefix) in &resolved.builtin_module_imports {
            self.builtin_module_imports
                .insert(alias.clone(), prefix.clone());
        }
    }

    /// Apply one prepared module's visible semantic effects to this snapshot.
    /// 将单个 prepared module 的可见语义效果合并到当前快照。
    pub fn apply_prepared_module(&mut self, prepared: &SessionPreparedModule) {
        self.next_def_id = self.next_def_id.max(prepared.next_def_id);
        for (name, def_id) in &prepared.global_defs {
            self.existing_globals.insert(name.clone(), *def_id);
        }
        self.apply_resolved_imports(&prepared.resolved_imports);
    }
}

/// Errors produced by compatibility session operations.
/// 兼容会话操作返回的错误。
#[derive(Debug, Clone)]
pub enum SessionError {
    /// Import path could not be resolved relative to the current module.
    /// 相对于当前模块无法解析导入路径。
    CannotResolveImportPath(String),
    /// Loading a dependency module failed.
    /// 加载依赖模块失败。
    ModuleLoad(ModuleLoadError),
    /// Import resolution failed after modules were loaded.
    /// 模块加载后导入解析失败。
    ImportResolution(String),
    /// Module context resolution or validation failed.
    /// 模块上下文解析或校验失败。
    Context(String),
}

/// Errors produced while preparing and checking one in-memory module.
/// 准备并检查单个内存模块时返回的错误。
#[derive(Debug, Clone)]
pub enum SessionCheckError {
    /// Session/module-loading level failure.
    /// 会话或模块加载层面的失败。
    Session(SessionError),
    /// Newly loaded dependency modules produced blocking diagnostics.
    /// 新加载依赖模块产生了阻塞性诊断。
    LoadedModules(Vec<SessionLoadedDiagnostics>),
    /// Current in-memory module produced blocking diagnostics.
    /// 当前内存模块产生了阻塞性诊断。
    ModuleDiagnostics(Vec<Diagnostic>),
}

/// Errors produced while parsing and checking one in-memory source input.
/// 解析并检查单个内存源码输入时返回的错误。
#[derive(Debug, Clone)]
pub enum SessionSourceCheckError {
    /// The source failed during parsing.
    /// 源码在解析阶段失败。
    ParseDiagnostics(Vec<Diagnostic>),
    /// The source parsed, but checked-module preparation failed.
    /// 源码解析成功，但 checked-module 准备失败。
    Check(SessionCheckError),
}

/// Frontend-owned error projection ready for CLI-style display.
/// frontend 持有的、可直接用于 CLI 展示的错误投影。
#[derive(Debug, Clone)]
pub enum SessionDisplayError {
    /// Diagnostics attributed to the current in-memory source.
    /// 归属到当前内存源码的诊断。
    Diagnostics {
        /// User-facing source name used for diagnostic emission.
        /// 用于诊断发射的用户可见源码名称。
        source_name: String,
        /// Source text used for diagnostic emission.
        /// 用于诊断发射的源码文本。
        source: String,
        /// Diagnostics attributed to the source.
        /// 归属到该源码的诊断。
        diagnostics: Vec<Diagnostic>,
    },
    /// Diagnostics attributed to newly loaded dependency modules.
    /// 归属到新加载依赖模块的诊断。
    LoadedModules(Vec<SessionLoadedDiagnostics>),
    /// Plain error message without source diagnostics.
    /// 不带源码诊断的普通错误消息。
    Message(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::CannotResolveImportPath(path) => {
                write!(
                    f,
                    "cannot resolve import path '{path}' from current REPL context"
                )
            }
            SessionError::ModuleLoad(err) => write!(f, "module load error: {err}"),
            SessionError::ImportResolution(err) => write!(f, "use resolution error: {err}"),
            SessionError::Context(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<SessionError> for SessionCheckError {
    fn from(value: SessionError) -> Self {
        SessionCheckError::Session(value)
    }
}

impl From<SessionCheckError> for SessionSourceCheckError {
    fn from(value: SessionCheckError) -> Self {
        SessionSourceCheckError::Check(value)
    }
}

impl SessionCheckError {
    /// Project one checked-module error into a CLI-displayable frontend error.
    /// 将单个 checked-module 错误投影为可直接展示的 frontend 错误。
    pub fn into_display_error(
        self,
        current_source_name: impl Into<String>,
        current_source: impl Into<String>,
    ) -> SessionDisplayError {
        let current_source_name = current_source_name.into();
        let current_source = current_source.into();
        match self {
            SessionCheckError::Session(err) => SessionDisplayError::Message(err.to_string()),
            SessionCheckError::LoadedModules(entries) => {
                SessionDisplayError::LoadedModules(entries)
            }
            SessionCheckError::ModuleDiagnostics(diagnostics) => SessionDisplayError::Diagnostics {
                source_name: current_source_name,
                source: current_source,
                diagnostics,
            },
        }
    }
}

impl SessionSourceCheckError {
    /// Project one source-check error into a CLI-displayable frontend error.
    /// 将单个源码检查错误投影为可直接展示的 frontend 错误。
    pub fn into_display_error(
        self,
        current_source_name: impl Into<String>,
        current_source: impl Into<String>,
    ) -> SessionDisplayError {
        let current_source_name = current_source_name.into();
        let current_source = current_source.into();
        match self {
            SessionSourceCheckError::ParseDiagnostics(diagnostics) => {
                SessionDisplayError::Diagnostics {
                    source_name: current_source_name,
                    source: current_source,
                    diagnostics,
                }
            }
            SessionSourceCheckError::Check(err) => {
                err.into_display_error(current_source_name, current_source)
            }
        }
    }
}

impl SessionModuleContext {
    /// Build a REPL-local in-memory module context.
    /// 构建 REPL 本地内存模块上下文。
    pub fn repl() -> Self {
        Self {
            root_dir: None,
            module_path: Vec::new(),
            module_name: "repl".to_string(),
        }
    }
}

/// Compatibility frontend session used by REPL/incremental consumers.
/// REPL/增量消费者使用的兼容前端会话。
#[derive(Debug, Clone)]
pub struct FrontendSession {
    loader: ModuleLoader,
    persisted_modules: Vec<Module>,
}

impl FrontendSession {
    /// Normalize one raw REPL input into frontend-owned source text.
    /// 将一条原始 REPL 输入规范化为 frontend 持有的源码文本。
    pub fn prepare_repl_source(input: &str) -> SessionPreparedReplSource {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return SessionPreparedReplSource {
                source: String::new(),
                persist_defs: true,
            };
        }

        let is_item = trimmed.starts_with("let ")
            || trimmed.starts_with("fn ")
            || trimmed.starts_with("type ")
            || trimmed.starts_with("struct ")
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("trait ")
            || trimmed.starts_with("impl ")
            || trimmed.starts_with("use ")
            || trimmed.starts_with("pub ");

        if is_item {
            let source = if trimmed.ends_with(';') {
                trimmed.to_string()
            } else {
                format!("{trimmed};")
            };
            SessionPreparedReplSource {
                source,
                persist_defs: true,
            }
        } else {
            // Strip trailing semicolons to avoid ;; in wrapped source
            let expr = trimmed.strip_suffix(';').unwrap_or(trimmed);
            SessionPreparedReplSource {
                source: format!("let {REPL_EXPR_BINDING_NAME} = {expr};"),
                persist_defs: false,
            }
        }
    }

    /// Normalize one REPL `:type` query expression into frontend-owned source text.
    /// 将一条 REPL `:type` 查询表达式规范化为 frontend 持有的源码文本。
    pub fn prepare_repl_type_query_source(expr: &str) -> String {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            String::new()
        } else {
            format!("let {REPL_TYPE_QUERY_BINDING_NAME} = {trimmed};")
        }
    }

    /// Prepare one in-memory REPL input with canonical source naming and context.
    /// 使用规范源码名称和上下文准备单个内存 REPL 输入。
    pub fn prepare_repl_input(&self, input: &str) -> SessionPreparedReplInput {
        let prepared = Self::prepare_repl_source(input);
        SessionPreparedReplInput {
            source_name: REPL_SOURCE_NAME.to_string(),
            source: prepared.source,
            persist_defs: prepared.persist_defs,
            context: self.repl_context(),
        }
    }

    /// Create a new session rooted at the given directory.
    /// 创建一个以给定目录为根的前端会话。
    pub fn new(root_dir: impl AsRef<Path>) -> Self {
        // Canonicalize to ensure consistent path comparisons (e.g. macOS /var vs /private/var)
        let root = root_dir
            .as_ref()
            .canonicalize()
            .unwrap_or_else(|_| root_dir.as_ref().to_path_buf());
        Self {
            loader: ModuleLoader::new(root),
            persisted_modules: Vec::new(),
        }
    }

    /// Convenience alias for creating a new session rooted at the given directory.
    /// 以给定目录为根创建新会话的便捷别名。
    pub fn with_root_dir(root_dir: impl AsRef<Path>) -> Self {
        Self::new(root_dir)
    }

    /// Configure an explicit std source root.
    /// 配置显式的 std 源码根目录。
    pub fn with_std_path(mut self, std_path: impl AsRef<Path>) -> Self {
        self.loader = self.loader.with_std_path(std_path);
        self
    }

    /// Borrow the current root directory.
    /// 借用当前根目录。
    pub fn root_dir(&self) -> &Path {
        self.loader.root_dir()
    }

    /// Reset the session while keeping the same root directory.
    /// 在保持根目录不变的情况下重置会话。
    pub fn clear(&mut self) {
        let root_dir = self.root_dir().to_path_buf();
        *self = Self::new(root_dir);
    }

    /// Reset the session to a new root directory.
    /// 将会话重置到新的根目录。
    pub fn rebase_root(&mut self, root_dir: impl AsRef<Path>) {
        *self = Self::new(root_dir);
    }

    /// Returns whether the session is still semantically empty.
    /// 返回当前会话是否仍然是语义空状态。
    pub fn is_pristine(&self) -> bool {
        self.loader.load_order().is_empty() && self.persisted_modules.is_empty()
    }

    /// Borrow the underlying compatibility module loader.
    /// 借用底层兼容 `ModuleLoader`。
    pub fn module_loader(&self) -> &ModuleLoader {
        &self.loader
    }

    /// Borrow the underlying compatibility module loader mutably.
    /// 可变借用底层兼容 `ModuleLoader`。
    pub fn module_loader_mut(&mut self) -> &mut ModuleLoader {
        &mut self.loader
    }

    /// Borrow loaded module info by id.
    /// 按 id 借用已加载模块信息。
    pub fn module_info(&self, module_id: ModuleId) -> Option<&ModuleInfo> {
        self.loader.get_module(module_id)
    }

    /// Borrow a lowered module by id.
    /// 按 id 借用降级后的模块。
    pub fn hir_module(&self, module_id: ModuleId) -> Option<&Module> {
        self.loader.hir_module(module_id)
    }

    /// Borrow the loaded dependency-first order.
    /// 借用依赖优先的已加载顺序。
    pub fn load_order(&self) -> &[ModuleId] {
        self.loader.load_order()
    }

    /// Get the next global definition id counter tracked by the loader.
    /// 获取 loader 维护的全局定义 ID 计数器。
    pub fn next_def_id(&self) -> u32 {
        self.loader.next_def_id()
    }

    /// Update the next global definition id counter tracked by the loader.
    /// 更新 loader 维护的全局定义 ID 计数器。
    pub fn set_def_id_counter(&mut self, next: u32) {
        self.loader.set_def_id_counter(next);
    }

    /// Record a persisted in-memory module.
    /// 记录一个持久化的内存模块。
    pub fn record_module(&mut self, module: Module) {
        self.persisted_modules.push(module);
    }

    /// Commit one prepared in-memory module into visible state plus persisted session state.
    /// 将单个 prepared 内存模块提交到可见状态与持久化会话状态。
    pub fn commit_prepared_module(
        &mut self,
        visible_state: &mut SessionVisibleState,
        prepared: SessionPreparedModule,
    ) {
        visible_state.apply_prepared_module(&prepared);
        self.record_module(prepared.module);
    }

    /// Borrow all persisted in-memory modules.
    /// 借用所有持久化内存模块。
    pub fn persisted_modules(&self) -> &[Module] {
        &self.persisted_modules
    }

    /// Analyze loaded modules from the compatibility loader.
    /// 分析兼容 loader 中的已加载模块。
    pub fn analyze_loaded_modules(&self) -> HashMap<ModuleId, ModuleAnalysis> {
        let mut modules = HashMap::new();
        for entry in self.loaded_modules_in_order() {
            modules.insert(entry.module_id, entry.analysis);
        }
        modules
    }

    /// Return dependency-first loaded module entries with HIR and semantic results.
    /// 返回带 HIR 与语义结果的依赖优先已加载模块条目。
    pub fn loaded_modules_in_order(&self) -> Vec<SessionLoadedModule> {
        let (global_types, global_spans, global_fn_bounds, global_traits) =
            self.collect_loaded_global_env();
        let type_names = self.collect_type_names(None);
        let mut global_effectful_definitions = HashSet::new();
        let mut entries = Vec::new();

        for module_id in self.loader.load_order() {
            let Some(info) = self.loader.get_module(*module_id) else {
                continue;
            };

            let parse_diagnostics = self.loader.parsed_diagnostics(*module_id).unwrap_or(&[]);
            if !parse_diagnostics.is_empty() {
                entries.push(SessionLoadedModule {
                    module_id: *module_id,
                    file_path: info.file_path.clone(),
                    module: None,
                    analysis: ModuleAnalysis {
                        diagnostics: parse_diagnostics.to_vec(),
                        semantics: ModuleSemantics::default(),
                    },
                });
                continue;
            }

            let Some(module) = self.loader.hir_module(*module_id) else {
                continue;
            };

            let mut checker = TypeChecker::with_global_env_and_traits(
                global_types.clone(),
                global_spans.clone(),
                global_fn_bounds.clone(),
                global_traits.clone(),
            )
            .with_effectful_definitions(global_effectful_definitions.iter().copied());
            checker.check(module);
            global_effectful_definitions.extend(checker.effectful_definitions().iter().copied());
            let semantics = collect_module_semantics(&checker, module);
            let diagnostics =
                rewrite_diagnostics_with_names(checker.diagnostics_ref().to_vec(), &type_names);

            entries.push(SessionLoadedModule {
                module_id: *module_id,
                file_path: info.file_path.clone(),
                module: Some(module.clone()),
                analysis: ModuleAnalysis {
                    diagnostics,
                    semantics,
                },
            });
        }

        let has_blocking_diagnostics = entries
            .iter()
            .any(|entry| diagnostics_have_errors(&entry.analysis.diagnostics));
        if has_blocking_diagnostics {
            for entry in &mut entries {
                entry.analysis.semantics.action_plans.clear();
            }
        }
        entries
    }

    /// Return dependency-first loaded modules that are ready for HIR evaluation.
    /// 返回已可用于 HIR 求值的依赖优先已加载模块。
    pub fn evaluable_loaded_modules_in_order(&self) -> Vec<SessionEvaluableModule> {
        self.loaded_modules_in_order()
            .into_iter()
            .filter_map(|entry| {
                let module = entry.module?;
                if diagnostics_have_errors(&entry.analysis.diagnostics) {
                    return None;
                }

                Some(SessionEvaluableModule {
                    module_id: entry.module_id,
                    module,
                    method_resolutions: entry.analysis.semantics.method_resolutions,
                })
            })
            .collect()
    }

    /// Analyze a current in-memory module against loaded + persisted session state.
    /// 基于已加载模块与持久化模块状态分析当前内存模块。
    pub fn analyze_module(&self, current_module: &Module) -> ModuleAnalysis {
        let mut modules: Vec<&Module> = self
            .loader
            .load_order()
            .iter()
            .filter_map(|module_id| self.loader.hir_module(*module_id))
            .collect();
        modules.extend(self.persisted_modules.iter());
        modules.push(current_module);

        let (global_types, global_spans, global_fn_bounds, global_traits) =
            Self::collect_global_env(modules.iter().copied());
        let mut checker = TypeChecker::with_global_env_and_traits(
            global_types,
            global_spans,
            global_fn_bounds,
            global_traits,
        )
        .with_repl_mode(true);

        let mut has_blocking_dependency_diagnostics = self.loader.load_order().iter().any(|id| {
            self.loader
                .parsed_diagnostics(*id)
                .is_some_and(diagnostics_have_errors)
        });
        for module in modules.iter().take(modules.len() - 1) {
            checker.check(module);
            has_blocking_dependency_diagnostics |=
                diagnostics_have_errors(checker.diagnostics_ref());
            checker.clear_diagnostics();
            checker.clear_method_resolutions();
            checker.clear_assoc_projection_resolutions();
        }

        checker.check(current_module);
        let type_names = self.collect_type_names(Some(current_module));
        let diagnostics =
            rewrite_diagnostics_with_names(checker.diagnostics_ref().to_vec(), &type_names);
        let mut semantics = collect_module_semantics(&checker, current_module);
        if has_blocking_dependency_diagnostics {
            semantics.action_plans.clear();
        }

        ModuleAnalysis {
            diagnostics,
            semantics,
        }
    }

    /// Analyze a current in-memory module and return diagnostics on semantic failure.
    /// 分析当前内存模块，并在存在语义错误时直接返回诊断。
    pub fn analyze_module_checked(
        &self,
        current_module: &Module,
    ) -> Result<ModuleAnalysis, Vec<Diagnostic>> {
        let analysis = self.analyze_module(current_module);
        if diagnostics_have_errors(&analysis.diagnostics) {
            return Err(analysis.diagnostics);
        }

        Ok(analysis)
    }

    /// Collect readable type names visible to the current session.
    /// 收集当前会话可见的可读类型名。
    pub fn type_names(&self) -> HashMap<DefId, String> {
        self.collect_type_names(None)
    }

    /// Collect readable type names visible to the current session plus one extra module.
    /// 收集当前会话加一个额外模块可见的可读类型名。
    pub fn type_names_with_current(&self, current_module: &Module) -> HashMap<DefId, String> {
        self.collect_type_names(Some(current_module))
    }

    /// Format one type using names visible to the current session plus one extra module.
    /// 使用当前会话和一个额外模块可见的名称格式化类型。
    pub fn format_type_with_current_names(&self, ty: &Ty, current_module: &Module) -> String {
        let names = self.type_names_with_current(current_module);
        format_type_with_names_map(ty, &names)
    }

    /// Resolve the semantic type target for one named binding in a checked module.
    /// 解析 checked 模块中某个命名绑定对应的语义类型目标。
    pub fn checked_binding_type_target(
        &self,
        checked: &SessionCheckedModule,
        binding_name: &str,
    ) -> Option<DefId> {
        find_checked_binding_type_target(&checked.prepared.module, binding_name)
    }

    /// Resolve the semantic type of one named binding in a checked module.
    /// 解析 checked 模块中某个命名绑定对应的语义类型。
    pub fn checked_binding_type<'a>(
        &self,
        checked: &'a SessionCheckedModule,
        binding_name: &str,
    ) -> Option<&'a Ty> {
        let def_id = self.checked_binding_type_target(checked, binding_name)?;
        checked.analysis.semantics.global_type(def_id)
    }

    /// Format the semantic type of one named binding in a checked module.
    /// 格式化 checked 模块中某个命名绑定对应的语义类型。
    pub fn format_checked_binding_type(
        &self,
        checked: &SessionCheckedModule,
        binding_name: &str,
    ) -> Option<String> {
        let ty = self.checked_binding_type(checked, binding_name)?;
        Some(self.format_type_with_current_names(ty, &checked.prepared.module))
    }

    /// Parse source text, check it, and format one named binding type in one frontend-owned step.
    /// 在一个 frontend 持有的步骤中完成源码解析、检查与命名绑定类型格式化。
    pub fn parse_format_binding_type_with_context(
        &mut self,
        source: &str,
        binding_name: &str,
        context: &SessionModuleContext,
        visible_state: &SessionVisibleState,
    ) -> Result<Option<String>, SessionSourceCheckError> {
        let checked = self.parse_checked_source_with_context(source, context, visible_state)?;
        Ok(self.format_checked_binding_type(&checked.checked, binding_name))
    }

    /// Parse one REPL type query and format the queried expression type in one frontend-owned step.
    /// 在一个 frontend 持有的步骤中解析 REPL 类型查询并格式化被查询表达式的类型。
    pub fn parse_format_repl_type(
        &mut self,
        expr: &str,
        visible_state: &SessionVisibleState,
    ) -> Result<Option<String>, SessionSourceCheckError> {
        let source = Self::prepare_repl_type_query_source(expr);
        self.parse_format_binding_type_with_context(
            &source,
            REPL_TYPE_QUERY_BINDING_NAME,
            &self.repl_context(),
            visible_state,
        )
    }

    /// Collect diagnostics for selected loaded modules using canonical frontend analysis.
    /// 使用规范 frontend 分析收集指定已加载模块的诊断。
    pub fn loaded_module_diagnostics(
        &self,
        module_ids: &[ModuleId],
    ) -> Vec<SessionLoadedDiagnostics> {
        let selected: std::collections::HashSet<_> = module_ids.iter().copied().collect();
        let mut entries = Vec::new();

        for entry in self.loaded_modules_in_order() {
            if !selected.contains(&entry.module_id) {
                continue;
            }
            if diagnostics_have_errors(&entry.analysis.diagnostics) {
                entries.push(SessionLoadedDiagnostics {
                    module_id: entry.module_id,
                    file_path: entry.file_path.clone(),
                    source: read_diagnostic_source(&entry.file_path),
                    diagnostics: entry.analysis.diagnostics,
                });
            }
        }

        entries
    }

    /// Build a current in-memory module against the session state.
    /// 基于当前会话状态构建当前内存模块。
    pub fn build_module_from_ast(
        &mut self,
        ast: &SourceFile,
        module_name: String,
        module_path: Vec<String>,
        inputs: &SessionBuildInputs,
    ) -> Result<SessionBuildResult, SessionError> {
        self.set_def_id_counter(inputs.next_def_id.max(self.next_def_id()));
        let known_modules: HashSet<_> = self.loader.load_order().iter().copied().collect();

        for item in &ast.items {
            let ItemKind::Import(import) = &item.kind else {
                continue;
            };
            if supports_canonical_std_import(import) {
                continue;
            }

            let Some(import_path) = ModulePath::from_import_def(import) else {
                return Err(SessionError::CannotResolveImportPath(format_import_path(
                    import,
                )));
            };
            let Some(absolute_path) = self
                .loader
                .resolve_module_path(&import_path, Some(&module_path))
            else {
                return Err(SessionError::CannotResolveImportPath(format_import_path(
                    import,
                )));
            };

            self.loader
                .load_module(&absolute_path)
                .map_err(SessionError::ModuleLoad)?;
        }

        let mut resolver = Resolver::new();
        resolver.set_def_id_counter(inputs.next_def_id.max(self.next_def_id()));
        resolver.register_existing_globals(inputs.existing_globals.clone());
        resolver.register_imports(inputs.imported_defs.clone());
        for alias in &inputs.imported_module_aliases {
            resolver.register_module_import_alias(alias.clone());
        }
        for (name, builtin_name) in &inputs.builtin_item_imports {
            resolver.register_builtin_item_import(name.clone(), builtin_name.clone());
        }
        for (alias, module_prefix) in &inputs.builtin_module_imports {
            resolver.register_builtin_module_import(alias.clone(), module_prefix.clone());
        }
        resolver.set_module_loader(self.loader.clone());
        let module = resolver.resolve_with_path(ast, module_name, module_path);
        let next_def_id = resolver.next_def_id().max(self.next_def_id());
        self.set_def_id_counter(next_def_id);

        Ok(SessionBuildResult {
            module,
            newly_loaded: self
                .loader
                .load_order()
                .iter()
                .copied()
                .filter(|module_id| !known_modules.contains(module_id))
                .collect(),
            next_def_id,
            global_defs: resolver.global_defs().clone(),
        })
    }

    /// Build one in-memory module and resolve its imports in one frontend-owned step.
    /// 在一个 frontend 持有的步骤中完成单个内存模块构建与导入解析。
    pub fn prepare_module_from_ast(
        &mut self,
        ast: &SourceFile,
        module_name: String,
        module_path: Vec<String>,
        inputs: &SessionBuildInputs,
    ) -> Result<SessionPreparedModule, SessionError> {
        let build = self.build_module_from_ast(ast, module_name, module_path.clone(), inputs)?;
        let resolved_imports = self.resolve_ast_imports(ast, &module_path)?;

        Ok(SessionPreparedModule {
            module: build.module,
            newly_loaded: build.newly_loaded,
            next_def_id: build.next_def_id,
            global_defs: build.global_defs,
            defined_bindings: defined_bindings_from_ast(ast),
            resolved_imports,
        })
    }

    /// Build one in-memory module from a frontend-owned module context.
    /// 基于 frontend 持有的模块上下文构建单个内存模块。
    pub fn prepare_module_with_context(
        &mut self,
        ast: &SourceFile,
        context: &SessionModuleContext,
        visible_state: &SessionVisibleState,
    ) -> Result<SessionPreparedModule, SessionError> {
        self.validate_module_context(context)?;
        self.prepare_module_with_visible_state(
            ast,
            context.module_name.clone(),
            context.module_path.clone(),
            visible_state,
        )
    }

    /// Build one in-memory module from a caller-visible semantic snapshot.
    /// 基于调用方可见语义快照构建单个内存模块。
    pub fn prepare_module_with_visible_state(
        &mut self,
        ast: &SourceFile,
        module_name: String,
        module_path: Vec<String>,
        visible_state: &SessionVisibleState,
    ) -> Result<SessionPreparedModule, SessionError> {
        let inputs = visible_state.build_inputs(self.next_def_id());
        self.prepare_module_from_ast(ast, module_name, module_path, &inputs)
    }

    /// Prepare and semantically check one in-memory module from a frontend-owned context.
    /// 基于 frontend 持有的上下文准备并语义检查单个内存模块。
    pub fn prepare_checked_module_with_context(
        &mut self,
        ast: &SourceFile,
        context: &SessionModuleContext,
        visible_state: &SessionVisibleState,
    ) -> Result<SessionCheckedModule, SessionCheckError> {
        let prepared = self.prepare_module_with_context(ast, context, visible_state)?;
        self.check_prepared_module(prepared)
    }

    /// Prepare and semantically check one in-memory module from visible caller state.
    /// 基于调用方可见状态准备并语义检查单个内存模块。
    pub fn prepare_checked_module_with_visible_state(
        &mut self,
        ast: &SourceFile,
        module_name: String,
        module_path: Vec<String>,
        visible_state: &SessionVisibleState,
    ) -> Result<SessionCheckedModule, SessionCheckError> {
        let prepared =
            self.prepare_module_with_visible_state(ast, module_name, module_path, visible_state)?;
        self.check_prepared_module(prepared)
    }

    /// Parse source text and prepare one checked in-memory module from a frontend-owned context.
    /// 基于 frontend 持有的上下文解析源码并准备单个 checked 内存模块。
    pub fn parse_checked_source_with_context(
        &mut self,
        source: &str,
        context: &SessionModuleContext,
        visible_state: &SessionVisibleState,
    ) -> Result<SessionCheckedSource, SessionSourceCheckError> {
        let (ast, diagnostics) = parse(source);
        if !diagnostics.is_empty() {
            return Err(SessionSourceCheckError::ParseDiagnostics(diagnostics));
        }

        let checked = self.prepare_checked_module_with_context(&ast, context, visible_state)?;
        Ok(SessionCheckedSource { ast, checked })
    }

    /// Parse source text, check it, and project failures into one display-ready frontend error.
    /// 解析源码、完成检查，并将失败投影为可直接展示的 frontend 错误。
    pub fn parse_checked_source_with_context_for_display(
        &mut self,
        source: &str,
        context: &SessionModuleContext,
        visible_state: &SessionVisibleState,
    ) -> Result<SessionCheckedSource, SessionDisplayError> {
        self.parse_checked_source_with_context_for_display_as(
            source,
            source,
            context,
            visible_state,
        )
    }

    /// Parse source text, check it, and project failures into one display-ready frontend error
    /// with an explicit user-facing source name.
    /// 解析源码、完成检查，并使用显式的用户可见源码名称将失败投影为可直接展示的 frontend 错误。
    pub fn parse_checked_source_with_context_for_display_as(
        &mut self,
        source_name: impl Into<String>,
        source: &str,
        context: &SessionModuleContext,
        visible_state: &SessionVisibleState,
    ) -> Result<SessionCheckedSource, SessionDisplayError> {
        self.parse_checked_source_with_context(source, context, visible_state)
            .map_err(|err| err.into_display_error(source_name, source))
    }

    /// Parse one REPL type query and project failures into one display-ready frontend error.
    /// 解析一条 REPL 类型查询，并将失败投影为可直接展示的 frontend 错误。
    pub fn parse_format_repl_type_for_display(
        &mut self,
        expr: &str,
        visible_state: &SessionVisibleState,
    ) -> Result<Option<String>, SessionDisplayError> {
        let source = Self::prepare_repl_type_query_source(expr);
        self.parse_format_repl_type(expr, visible_state)
            .map_err(|err| err.into_display_error(REPL_TYPE_QUERY_SOURCE_NAME, source))
    }

    /// Build the canonical in-memory REPL module context.
    /// 构建规范的内存 REPL 模块上下文。
    pub fn repl_context(&self) -> SessionModuleContext {
        SessionModuleContext::repl()
    }

    /// Resolve one file-backed REPL input context, optionally rebasing the session root.
    /// 解析单个基于文件的 REPL 输入上下文，并在允许时重设会话根目录。
    pub fn repl_context_for_file(
        &mut self,
        path: impl AsRef<Path>,
        allow_root_switch: bool,
    ) -> Result<SessionModuleContext, SessionError> {
        self.module_context_for_file(path, allow_root_switch)
    }

    /// Load one file-backed REPL input and resolve its canonical frontend context.
    /// 加载单个基于文件的 REPL 输入，并解析其规范 frontend 上下文。
    pub fn load_repl_file_input(
        &mut self,
        path: impl AsRef<Path>,
        allow_root_switch: bool,
    ) -> Result<SessionReplFileInput, SessionDisplayError> {
        let file_path = path.as_ref().to_path_buf();
        let source_name = file_path.display().to_string();
        let source = std::fs::read_to_string(&file_path).map_err(|err| {
            SessionDisplayError::Message(format!(
                "Cannot read file '{}': {}",
                file_path.display(),
                err
            ))
        })?;
        let context = self
            .repl_context_for_file(&file_path, allow_root_switch)
            .map_err(|err| SessionDisplayError::Message(err.to_string()))?;

        Ok(SessionReplFileInput {
            source_name,
            file_path,
            source,
            context,
        })
    }

    /// Resolve a file-backed module context, optionally rebasing the session root.
    /// 解析基于文件的模块上下文，并在允许时重设会话根目录。
    pub fn module_context_for_file(
        &mut self,
        path: impl AsRef<Path>,
        allow_root_switch: bool,
    ) -> Result<SessionModuleContext, SessionError> {
        let canonical = path.as_ref().canonicalize().map_err(|e| {
            SessionError::Context(format!(
                "cannot resolve path '{}': {}",
                path.as_ref().display(),
                e
            ))
        })?;

        let (context_root, module_path) = match canonical.strip_prefix(self.root_dir()) {
            Ok(relative) => (
                self.root_dir().to_path_buf(),
                module_path_from_relative(relative),
            ),
            Err(_) => {
                let (root_dir, module_path) = infer_module_root_and_path(&canonical)?;
                if !(allow_root_switch && self.is_pristine()) {
                    return Err(SessionError::Context(format!(
                        "loaded file '{}' is outside the current REPL session root '{}'; run :clear before switching to another project root",
                        canonical.display(),
                        self.root_dir().display()
                    )));
                }
                self.rebase_root(&root_dir);
                (self.root_dir().to_path_buf(), module_path)
            }
        };

        Ok(SessionModuleContext {
            root_dir: Some(context_root),
            module_name: module_name_from_path(&module_path),
            module_path,
        })
    }

    /// Validate that one module context still belongs to the current session root.
    /// 校验模块上下文仍然属于当前会话根目录。
    pub fn validate_module_context(
        &self,
        context: &SessionModuleContext,
    ) -> Result<(), SessionError> {
        if let Some(root_dir) = &context.root_dir
            && root_dir != self.root_dir()
        {
            return Err(SessionError::Context(format!(
                "REPL 模块图当前固定在会话根目录 '{}'；请在该项目根目录启动 REPL，或加载同一根目录下的文件",
                self.root_dir().display()
            )));
        }
        Ok(())
    }

    /// Resolve imports for a current in-memory module against the session state.
    /// 基于当前会话状态解析当前内存模块的导入。
    pub fn resolve_ast_imports(
        &self,
        ast: &SourceFile,
        current_module_path: &[String],
    ) -> Result<SessionResolvedImports, SessionError> {
        let mut resolved = SessionResolvedImports::default();

        for item in &ast.items {
            let ItemKind::Import(import) = &item.kind else {
                continue;
            };
            if let Some(builtin) = resolve_std_builtin_import(import) {
                resolved.builtin_item_imports.extend(builtin.item_imports);
                resolved
                    .builtin_module_imports
                    .extend(builtin.module_imports);
                continue;
            }

            let bindings = self
                .loader
                .resolve_import(
                    &hir_import_from_ast(item.span, import)?,
                    current_module_path,
                )
                .map_err(|err| SessionError::ImportResolution(err.to_string()))?;
            resolved.bindings.extend(bindings);

            if matches!(import.items, ImportItems::Module)
                && let Some(alias) = import
                    .alias
                    .as_ref()
                    .map(|alias| alias.name.clone())
                    .or_else(|| import.path.last().map(|segment| segment.name.clone()))
            {
                resolved.module_aliases.push(alias);
            }
        }

        Ok(resolved)
    }

    #[allow(clippy::type_complexity)]
    fn collect_global_env<'a>(
        modules: impl IntoIterator<Item = &'a Module>,
    ) -> (
        HashMap<DefId, Ty>,
        HashMap<DefId, n3v3_common::Span>,
        HashMap<DefId, Vec<(u32, n3v3_typeck::TraitBound)>>,
        n3v3_typeck::TraitResolver,
    ) {
        let modules: Vec<&Module> = modules.into_iter().collect();
        let global_traits = TypeChecker::collect_global_trait_resolver(modules.iter().copied());
        let mut global_types = HashMap::new();
        let mut global_spans = HashMap::new();
        let mut global_fn_bounds = HashMap::new();

        for module in &modules {
            let (types, spans, bounds) =
                TypeChecker::collect_signatures_with_trait_resolver(module, &global_traits);
            global_types.extend(types);
            global_spans.extend(spans);
            global_fn_bounds.extend(bounds);
        }

        (global_types, global_spans, global_fn_bounds, global_traits)
    }

    #[allow(clippy::type_complexity)]
    fn collect_loaded_global_env(
        &self,
    ) -> (
        HashMap<DefId, Ty>,
        HashMap<DefId, n3v3_common::Span>,
        HashMap<DefId, Vec<(u32, n3v3_typeck::TraitBound)>>,
        n3v3_typeck::TraitResolver,
    ) {
        let modules = self
            .loader
            .load_order()
            .iter()
            .filter_map(|module_id| self.loader.hir_module(*module_id));
        Self::collect_global_env(modules)
    }

    fn collect_type_names(&self, current_module: Option<&Module>) -> HashMap<DefId, String> {
        let mut modules: Vec<&Module> = self
            .loader
            .load_order()
            .iter()
            .filter_map(|module_id| self.loader.hir_module(*module_id))
            .collect();
        modules.extend(self.persisted_modules.iter());
        if let Some(module) = current_module {
            modules.push(module);
        }
        collect_item_names_from_modules(modules)
    }

    fn check_prepared_module(
        &self,
        prepared: SessionPreparedModule,
    ) -> Result<SessionCheckedModule, SessionCheckError> {
        let loaded_module_ids = self.loader.load_order().to_vec();
        let loaded_module_diagnostics = self.loaded_module_diagnostics(&loaded_module_ids);
        if !loaded_module_diagnostics.is_empty() {
            return Err(SessionCheckError::LoadedModules(loaded_module_diagnostics));
        }

        let analysis = self
            .analyze_module_checked(&prepared.module)
            .map_err(SessionCheckError::ModuleDiagnostics)?;

        Ok(SessionCheckedModule { prepared, analysis })
    }
}

impl Default for FrontendSession {
    fn default() -> Self {
        let root_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new(root_dir)
    }
}

fn hir_import_from_ast(span: Span, import: &ImportDef) -> Result<HirImport, SessionError> {
    let prefix = match import.prefix {
        PathPrefix::Absolute => ImportPathPrefix::Absolute,
        PathPrefix::Self_ => ImportPathPrefix::Self_,
        PathPrefix::Super => ImportPathPrefix::Super,
        PathPrefix::Crate => ImportPathPrefix::Crate,
        _ => {
            return Err(SessionError::ImportResolution(format!(
                "unsupported import prefix at {}..{}",
                span.start.0, span.end.0
            )));
        }
    };

    let kind = match &import.items {
        ImportItems::Module => HirImportKind::Module,
        ImportItems::Items(items) => {
            HirImportKind::Items(items.iter().map(|item| item.name.clone()).collect())
        }
        ImportItems::All => HirImportKind::All,
        _ => {
            return Err(SessionError::ImportResolution(format!(
                "unsupported import items at {}..{}",
                span.start.0, span.end.0
            )));
        }
    };

    Ok(HirImport {
        prefix,
        path: import
            .path
            .iter()
            .map(|segment| segment.name.clone())
            .collect(),
        kind,
        alias: import.alias.as_ref().map(|alias| alias.name.clone()),
        is_pub: import.visibility == Visibility::Public,
        span,
    })
}

fn find_checked_binding_type_target(module: &Module, binding_name: &str) -> Option<DefId> {
    module.items.iter().find_map(|item| match &item.kind {
        HirItemKind::Fn(def) if def.name == binding_name => match &def.body.kind {
            ExprKind::Global(def_id) => Some(*def_id),
            _ => Some(item.id),
        },
        HirItemKind::Struct(def) if def.name == binding_name => Some(item.id),
        HirItemKind::Enum(def) if def.name == binding_name => Some(item.id),
        HirItemKind::TypeAlias(def) if def.name == binding_name => Some(item.id),
        HirItemKind::Trait(def) if def.name == binding_name => Some(item.id),
        _ => None,
    })
}

fn defined_bindings_from_ast(ast: &SourceFile) -> Vec<SessionDefinedBinding> {
    ast.items
        .iter()
        .flat_map(|item| {
            let is_public = item_visibility(item) != Visibility::Private;
            item_defined_names(item)
                .into_iter()
                .map(move |name| SessionDefinedBinding { name, is_public })
        })
        .collect()
}

fn item_visibility(item: &n3v3_syntax::Item) -> Visibility {
    match &item.kind {
        ItemKind::Let(def) => def.visibility,
        ItemKind::Fn(def) => def.visibility,
        ItemKind::TypeAlias(def) => def.visibility,
        ItemKind::Struct(def) => def.visibility,
        ItemKind::Enum(def) => def.visibility,
        ItemKind::Trait(def) => def.visibility,
        ItemKind::Import(def) => def.visibility,
        ItemKind::Impl(_) => Visibility::Private,
        ItemKind::ExprStmt(_) => Visibility::Private,
        _ => Visibility::Private,
    }
}

fn item_defined_names(item: &n3v3_syntax::Item) -> Vec<String> {
    match &item.kind {
        ItemKind::Let(let_def) => match &let_def.pattern.kind {
            PatternKind::Var(ident)
                if ident.name != REPL_EXPR_BINDING_NAME
                    && ident.name != REPL_TYPE_QUERY_BINDING_NAME =>
            {
                vec![ident.name.clone()]
            }
            _ => Vec::new(),
        },
        ItemKind::Fn(fn_def) if fn_def.name.name != REPL_TYPE_QUERY_BINDING_NAME => {
            vec![fn_def.name.name.clone()]
        }
        ItemKind::TypeAlias(def) => vec![def.name.name.clone()],
        ItemKind::Struct(def) => vec![def.name.name.clone()],
        ItemKind::Enum(def) => vec![def.name.name.clone()],
        ItemKind::Trait(def) => vec![def.name.name.clone()],
        ItemKind::Import(import) => match &import.items {
            ImportItems::Module => import
                .alias
                .as_ref()
                .map(|alias| vec![alias.name.clone()])
                .or_else(|| import.path.last().map(|name| vec![name.name.clone()]))
                .unwrap_or_default(),
            ImportItems::Items(items) => items.iter().map(|item| item.name.clone()).collect(),
            ImportItems::All => Vec::new(),
            _ => Vec::new(),
        },
        ItemKind::Impl(_) => Vec::new(),
        ItemKind::Fn(_) => Vec::new(),
        ItemKind::ExprStmt(_) => Vec::new(),
        _ => Vec::new(),
    }
}

fn infer_module_root_and_path(canonical: &Path) -> Result<(PathBuf, Vec<String>), SessionError> {
    let mut root_dir = canonical
        .parent()
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let mut rel_path = canonical.to_path_buf();
    let mut saw_src = false;
    for ancestor in canonical.ancestors() {
        if ancestor.file_name().is_some_and(|name| name == "src") {
            if let Some(parent) = ancestor.parent() {
                root_dir = parent.to_path_buf();
                rel_path = canonical
                    .strip_prefix(ancestor)
                    .unwrap_or(canonical)
                    .to_path_buf();
                saw_src = true;
            }
            break;
        }
    }

    if !saw_src {
        rel_path = canonical
            .strip_prefix(&root_dir)
            .unwrap_or(canonical)
            .to_path_buf();
    }

    Ok((root_dir, module_path_from_relative(&rel_path)))
}

fn module_path_from_relative(path: &Path) -> Vec<String> {
    let mut module_path: Vec<String> = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect();
    if let Some(last) = module_path.last_mut()
        && last.ends_with(".n3v3")
    {
        *last = last.trim_end_matches(".n3v3").to_string();
    }
    if module_path.last().map(|segment| segment.as_str()) == Some("mod") {
        module_path.pop();
    }
    if module_path.len() == 1 && module_path[0] == "lib" {
        module_path.clear();
    }
    module_path
}

fn module_name_from_path(module_path: &[String]) -> String {
    module_path
        .last()
        .cloned()
        .unwrap_or_else(|| "main".to_string())
}

fn format_import_path(import: &ImportDef) -> String {
    let prefix = match import.prefix {
        PathPrefix::Absolute => "",
        PathPrefix::Self_ => "self.",
        PathPrefix::Super => "super.",
        PathPrefix::Crate => "crate.",
        _ => "",
    };
    format!(
        "{}{}",
        prefix,
        import
            .path
            .iter()
            .map(|segment| segment.name.as_str())
            .collect::<Vec<_>>()
            .join(".")
    )
}
