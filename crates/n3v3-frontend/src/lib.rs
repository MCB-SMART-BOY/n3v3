//! Frontend analysis pipeline for n3v3.
//! n3v3 的前端分析管线。
//!
//! This crate provides a small, stable API to parse, lower, and type check
//! source text in a single pass for tools like the LSP and CLI.
//! 本 crate 提供稳定的 API，用于在一次流程中完成解析、降级与类型检查，
//! 便于 LSP 和 CLI 等工具复用。

mod action_plans;
mod driver;
mod session;

use action_plans::collect_action_plans;

pub use driver::{
    FrontendDriver, FrontendError, ModuleAnalysis, ModuleParseResult, ProgramAnalysis,
    ProgramDiagnosticModule, ProgramDiagnosticStats, ProgramEvaluableModule, ProgramLoweredModule,
    ProgramParsedModule, analyze_module_path, parse_module_file,
};
pub use n3v3_diagnostic::Diagnostic;
pub use n3v3_hir::Module;
use n3v3_hir::{DefId, ItemKind as HirItemKind, LocalId, Ty, TyKind};
pub use n3v3_syntax::SourceFile;
pub use session::{
    FrontendSession, SessionBuildInputs, SessionBuildResult, SessionCheckError,
    SessionCheckedModule, SessionCheckedSource, SessionDefinedBinding, SessionDisplayError,
    SessionError, SessionEvaluableModule, SessionLoadedDiagnostics, SessionLoadedModule,
    SessionModuleContext, SessionPreparedModule, SessionPreparedReplInput,
    SessionPreparedReplSource, SessionReplFileInput, SessionResolvedImports,
    SessionSourceCheckError, SessionVisibleState,
};

use n3v3_common::{ActionPlan, Span};
use n3v3_hir::{ModuleId, lower};
use n3v3_parser::parse;
use n3v3_typeck::{TypeChecker, format_builtin_named_type};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub(crate) fn read_diagnostic_source(path: &Path) -> String {
    match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!(
                "failed to read source for diagnostic attribution at {}: {error}",
                path.display()
            );
            String::new()
        }
    }
}

/// Canonical per-module semantic side tables.
/// 规范化后的单模块语义 side tables。
#[derive(Debug, Clone, Default)]
pub struct ModuleSemantics {
    /// Resolved method call targets keyed by expression span.
    /// 按表达式 span 存储的方法调用解析结果。
    pub method_resolutions: HashMap<Span, DefId>,
    /// Resolved associated-type projections keyed by explicit type-use span.
    /// 按显式类型使用位置 span 存储的关联类型投影解析结果。
    pub assoc_projection_resolutions: HashMap<Span, Ty>,
    /// Final inferred types for global definitions.
    /// 全局定义的最终推断类型。
    pub global_types: HashMap<DefId, Ty>,
    /// Source spans for global definitions.
    /// 全局定义的源码位置。
    pub global_spans: HashMap<DefId, Span>,
    /// Final inferred types for local definitions.
    /// 局部定义的最终推断类型。
    pub local_types: HashMap<LocalId, Ty>,
    /// Final inferred types for expressions.
    /// 表达式的最终推断类型。
    pub expr_types: HashMap<Span, Ty>,
    /// Human-readable names for global definitions (for type display).
    /// 全局定义的可读名称（用于类型显示）。
    pub global_names: HashMap<DefId, String>,
    /// Definitions inferred as effectful in this module or imported from dependencies.
    /// 本模块或依赖中推断为有副作用的定义。
    pub effectful_definitions: HashSet<DefId>,
    /// Typed host action plans keyed by their HIR expression span.
    /// 按 HIR 表达式 span 索引的类型化宿主动作计划。
    pub action_plans: HashMap<Span, ActionPlan>,
}

impl ModuleSemantics {
    /// Look up a typed host action plan by expression span.
    /// 按表达式 span 查询类型化宿主动作计划。
    pub fn action_plan(&self, span: Span) -> Option<&ActionPlan> {
        self.action_plans.get(&span)
    }

    /// Look up the type of a global definition.
    /// 查询全局定义的类型。
    pub fn global_type(&self, def_id: DefId) -> Option<&Ty> {
        self.global_types.get(&def_id)
    }

    /// Look up the source span of a global definition.
    /// 查询全局定义的源码位置。
    pub fn global_span(&self, def_id: DefId) -> Option<Span> {
        self.global_spans.get(&def_id).copied()
    }

    /// Look up the type of a local definition.
    /// 查询局部定义的类型。
    pub fn local_type(&self, local_id: LocalId) -> Option<&Ty> {
        self.local_types.get(&local_id)
    }

    /// Look up the type of an expression by span.
    /// 按 span 查询表达式类型。
    pub fn expr_type(&self, span: Span) -> Option<&Ty> {
        self.expr_types.get(&span)
    }

    /// Look up the resolved method target for an expression span.
    /// 查询表达式 span 对应的方法目标。
    pub fn method_resolution(&self, span: Span) -> Option<DefId> {
        self.method_resolutions.get(&span).copied()
    }

    /// Look up the resolved associated-type projection for a type-use span.
    /// 查询类型使用位置 span 对应的关联类型投影结果。
    pub fn assoc_projection_resolution(&self, span: Span) -> Option<&Ty> {
        self.assoc_projection_resolutions.get(&span)
    }
}

/// Result of analyzing a single source string.
/// 单个源文本的分析结果。
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// Parsed AST. / 解析后的 AST。
    pub ast: SourceFile,
    /// Lowered HIR. / 降级后的 HIR。
    pub hir: Module,
    /// Resolved method call targets for HIR evaluation.
    /// 用于 HIR 求值的方法调用解析结果。
    pub method_resolutions: HashMap<Span, DefId>,
    /// Canonical semantic side tables for this module.
    /// 当前模块的规范语义 side tables。
    pub semantics: ModuleSemantics,
    /// Diagnostics from parse + type check. / 解析与类型检查诊断。
    pub diagnostics: Vec<Diagnostic>,
}

/// Diagnostic counts grouped by blocking/non-blocking role.
/// 按阻断/非阻断角色分组的诊断计数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiagnosticStats {
    /// Number of syntax (lexer or parser) diagnostics with error severity.
    /// 严重程度为错误的语法（词法或语法分析）诊断数量。
    pub parse_errors: usize,
    /// Number of non-syntax diagnostics with error severity.
    /// 严重程度为错误且非语法类的诊断数量。
    pub non_parse_errors: usize,
    /// Number of warning diagnostics.
    /// 警告诊断数量。
    pub warnings: usize,
}

impl DiagnosticStats {
    /// Total number of blocking diagnostics.
    /// 阻断诊断总数。
    pub fn error_count(self) -> usize {
        self.parse_errors + self.non_parse_errors
    }

    /// Whether any loaded dependency diagnostics are blocking.
    /// 已加载依赖诊断是否包含阻断错误。
    pub fn has_errors(self) -> bool {
        self.error_count() > 0
    }
}

pub(crate) fn diagnostics_have_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == n3v3_diagnostic::Severity::Error)
}

pub(crate) fn collect_diagnostic_stats<'a, I>(diagnostics: I) -> DiagnosticStats
where
    I: IntoIterator<Item = &'a Diagnostic>,
{
    let mut stats = DiagnosticStats::default();

    for diagnostic in diagnostics {
        match (diagnostic.severity, diagnostic.kind) {
            (n3v3_diagnostic::Severity::Error, kind) if kind.is_syntax() => {
                stats.parse_errors += 1;
            }
            (n3v3_diagnostic::Severity::Error, _) => {
                stats.non_parse_errors += 1;
            }
            (n3v3_diagnostic::Severity::Warning, _) => {
                stats.warnings += 1;
            }
            (n3v3_diagnostic::Severity::Note, _) => {}
        }
    }

    stats
}

/// Diagnostic counts for dependency modules loaded during snippet analysis.
/// snippet 分析期间已加载依赖模块的诊断计数。
pub type LoadedSnippetDiagnosticStats = DiagnosticStats;

/// Result of analyzing one in-memory snippet against a rooted frontend session.
/// 基于带根目录的 frontend 会话分析单个内存 snippet 的结果。
#[derive(Debug, Clone)]
pub struct SnippetAnalysis {
    /// Lowered HIR for the current in-memory snippet.
    /// 当前内存 snippet 的降级 HIR。
    pub hir: Module,
    /// Canonical semantic side tables for the current snippet.
    /// 当前 snippet 的规范语义 side tables。
    pub semantics: ModuleSemantics,
    /// Diagnostics attributed to the current snippet.
    /// 归属到当前 snippet 的诊断。
    pub diagnostics: Vec<Diagnostic>,
    /// Loaded dependency modules and their diagnostics/semantics.
    /// 已加载依赖模块及其诊断/语义结果。
    pub loaded_modules: Vec<LoadedSnippetModule>,
    /// Loaded dependency modules ready for HIR evaluation.
    /// 已可用于 HIR 求值的依赖模块。
    pub evaluable_loaded_modules: Vec<SessionEvaluableModule>,
}

/// One dependency module loaded while analyzing a snippet.
/// 分析 snippet 过程中加载的单个依赖模块。
#[derive(Debug, Clone)]
pub struct LoadedSnippetModule {
    /// Loaded module id.
    /// 已加载模块 ID。
    pub module_id: ModuleId,
    /// Backing source path on disk.
    /// 模块对应的磁盘路径。
    pub file_path: PathBuf,
    /// Source text read for diagnostic attribution.
    /// 用于诊断归属展示的源码文本。
    pub source: String,
    /// Lowered HIR for this dependency when available.
    /// 当前依赖模块可用时的降级 HIR。
    pub hir: Option<Module>,
    /// Final diagnostics for this dependency.
    /// 当前依赖模块的最终诊断。
    pub diagnostics: Vec<Diagnostic>,
    /// Canonical semantic side tables for this dependency when available.
    /// 当前依赖模块可用时的规范语义 side tables。
    pub semantics: ModuleSemantics,
}

impl SnippetAnalysis {
    /// Summarize current snippet diagnostics by blocking/non-blocking role.
    /// 按阻断/非阻断角色汇总当前 snippet 诊断。
    pub fn diagnostic_stats(&self) -> DiagnosticStats {
        collect_diagnostic_stats(self.diagnostics.iter())
    }

    /// Whether current snippet diagnostics contain any blocking errors.
    /// 当前 snippet 诊断是否包含任何阻断错误。
    pub fn has_blocking_diagnostics(&self) -> bool {
        diagnostics_have_errors(&self.diagnostics)
    }

    /// Summarize loaded dependency diagnostics by blocking/non-blocking role.
    /// 按阻断/非阻断角色汇总已加载依赖诊断。
    pub fn loaded_diagnostic_stats(&self) -> LoadedSnippetDiagnosticStats {
        collect_diagnostic_stats(
            self.loaded_modules
                .iter()
                .flat_map(|entry| entry.diagnostics.iter()),
        )
    }

    /// Whether loaded dependency diagnostics contain any blocking errors.
    /// 已加载依赖诊断是否包含任何阻断错误。
    pub fn loaded_has_blocking_diagnostics(&self) -> bool {
        self.loaded_diagnostic_stats().has_errors()
    }
}

/// Analyze a source string and return AST, HIR, and diagnostics.
/// 分析源文本并返回 AST、HIR 以及诊断。
pub fn analyze_source(source: &str) -> AnalysisResult {
    // Parse first so we can reuse the AST for symbol indexing.
    // 先解析以便复用 AST 做符号索引。
    let (ast, mut diagnostics) = parse(source);
    let hir = lower(&ast);
    let mut checker = TypeChecker::new();
    checker.check(&hir);
    let mut semantics = collect_module_semantics(&checker, &hir);
    if diagnostics_have_errors(&diagnostics) {
        semantics.action_plans.clear();
    }
    let method_resolutions = checker.method_resolutions().clone();
    diagnostics.extend(rewrite_diagnostics_with_module_names(
        checker.diagnostics(),
        &hir,
    ));

    AnalysisResult {
        ast,
        hir,
        method_resolutions,
        semantics,
        diagnostics,
    }
}

/// Analyze an already-parsed AST.
/// 分析已解析的 AST。
pub fn analyze_ast(ast: &SourceFile) -> AnalysisResult {
    let hir = lower(ast);
    let mut checker = TypeChecker::new();
    checker.check(&hir);
    let semantics = collect_module_semantics(&checker, &hir);
    let method_resolutions = checker.method_resolutions().clone();
    let diagnostics = rewrite_diagnostics_with_module_names(checker.diagnostics(), &hir);

    AnalysisResult {
        ast: ast.clone(),
        hir,
        method_resolutions,
        semantics,
        diagnostics,
    }
}

/// Analyze one already-parsed snippet against a rooted frontend session.
/// 基于带根目录的 frontend 会话分析单个已解析 snippet。
pub fn analyze_snippet_ast(
    ast: &SourceFile,
    root_dir: impl AsRef<Path>,
) -> Result<SnippetAnalysis, SessionError> {
    let mut session = FrontendSession::new(root_dir);
    let build = session.build_module_from_ast(
        ast,
        "__eval__".to_string(),
        vec!["__eval__".to_string()],
        &SessionBuildInputs::default(),
    )?;
    let mut analysis = session.analyze_module(&build.module);
    let loaded_pending: std::collections::HashSet<_> = build.newly_loaded.iter().copied().collect();
    let mut loaded_modules = session
        .loaded_modules_in_order()
        .into_iter()
        .filter(|entry| loaded_pending.contains(&entry.module_id))
        .map(|entry| {
            let file_path = entry.file_path;
            LoadedSnippetModule {
                module_id: entry.module_id,
                source: read_diagnostic_source(&file_path),
                file_path,
                hir: entry.module,
                diagnostics: entry.analysis.diagnostics,
                semantics: entry.analysis.semantics,
            }
        })
        .collect::<Vec<_>>();
    let has_blocking_diagnostics = diagnostics_have_errors(&analysis.diagnostics)
        || loaded_modules
            .iter()
            .any(|entry| diagnostics_have_errors(&entry.diagnostics));
    if has_blocking_diagnostics {
        analysis.semantics.action_plans.clear();
        for entry in &mut loaded_modules {
            entry.semantics.action_plans.clear();
        }
    }
    let evaluable_loaded_modules = session
        .evaluable_loaded_modules_in_order()
        .into_iter()
        .filter(|entry| loaded_pending.contains(&entry.module_id))
        .collect();

    Ok(SnippetAnalysis {
        hir: build.module,
        semantics: analysis.semantics,
        diagnostics: analysis.diagnostics,
        loaded_modules,
        evaluable_loaded_modules,
    })
}

pub(crate) fn collect_module_semantics(checker: &TypeChecker, module: &Module) -> ModuleSemantics {
    let global_types = checker
        .global_types_ref()
        .keys()
        .filter_map(|def_id| checker.global_type(*def_id).map(|ty| (*def_id, ty)))
        .collect();
    let assoc_projection_resolutions = checker
        .assoc_projection_resolutions()
        .keys()
        .filter_map(|span| {
            checker
                .assoc_projection_resolution(*span)
                .map(|ty| (*span, ty))
        })
        .collect();
    let local_types = checker
        .local_definitions_ref()
        .keys()
        .filter_map(|local_id| checker.local_type(*local_id).map(|ty| (*local_id, ty)))
        .collect();
    let expr_types = checker
        .expr_types_ref()
        .keys()
        .filter_map(|span| checker.expr_type(*span).map(|ty| (*span, ty)))
        .collect();

    let action_plans = if diagnostics_have_errors(checker.diagnostics_ref()) {
        HashMap::new()
    } else {
        collect_action_plans(module)
    };

    ModuleSemantics {
        method_resolutions: checker.method_resolutions().clone(),
        assoc_projection_resolutions,
        global_types,
        global_spans: checker.global_spans_ref().clone(),
        local_types,
        expr_types,
        global_names: checker.global_names_ref().clone(),
        effectful_definitions: checker.effectful_definitions().clone(),
        action_plans,
    }
}

/// Format a type using names available in the given module.
/// 使用给定模块中可见的名称格式化类型。
pub fn format_type_in_module(ty: &Ty, module: &Module) -> String {
    format_type_in_modules(ty, [module])
}

/// Format an explicit type-use span using canonical semantic projections when available.
/// 优先使用规范语义中的关联类型投影结果来格式化显式类型使用位置。
pub fn format_type_use_in_module(
    semantics: &ModuleSemantics,
    module: &Module,
    type_use_span: Span,
    fallback: &Ty,
) -> String {
    semantics
        .assoc_projection_resolution(type_use_span)
        .map(|ty| format_type_in_module(ty, module))
        .unwrap_or_else(|| format_type_in_module(fallback, module))
}

/// Format a type using an explicit map of visible type names.
/// 使用显式提供的可见类型名称映射格式化类型。
pub fn format_type_with_names_map(ty: &Ty, names: &HashMap<DefId, String>) -> String {
    format_type_with_names(ty, names)
}

/// Format a type using names collected from multiple modules.
/// 使用多个模块中收集到的名称格式化类型。
pub fn format_type_in_modules<'a>(
    ty: &Ty,
    modules: impl IntoIterator<Item = &'a Module>,
) -> String {
    let names = collect_item_names_from_modules(modules);
    format_type_with_names(ty, &names)
}

/// Rewrite diagnostics so local named types are rendered readably.
/// 重写诊断文本，使当前模块内的命名类型以可读名称显示。
pub fn rewrite_diagnostics_with_module_names(
    diagnostics: Vec<Diagnostic>,
    module: &Module,
) -> Vec<Diagnostic> {
    rewrite_diagnostics_with_module_set(diagnostics, [module])
}

/// Rewrite diagnostics so named types from multiple modules render readably.
/// 重写诊断文本，使多个模块中的命名类型都以可读名称显示。
pub fn rewrite_diagnostics_with_module_set<'a>(
    diagnostics: Vec<Diagnostic>,
    modules: impl IntoIterator<Item = &'a Module>,
) -> Vec<Diagnostic> {
    let names = collect_item_names_from_modules(modules);
    rewrite_diagnostics_with_names(diagnostics, &names)
}

/// Rewrite diagnostics using a precomputed type-name map.
/// 使用预先收集的类型名映射重写诊断文本。
pub fn rewrite_diagnostics_with_names(
    diagnostics: Vec<Diagnostic>,
    names: &HashMap<DefId, String>,
) -> Vec<Diagnostic> {
    if names.is_empty() {
        return diagnostics;
    }

    let mut replacements: Vec<_> = names
        .iter()
        .map(|(def_id, name)| (format!("Type#{}", def_id.0), name.clone()))
        .collect();
    replacements.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));

    diagnostics
        .into_iter()
        .map(|mut diagnostic| {
            diagnostic.message = replace_type_placeholders(&diagnostic.message, &replacements);
            for label in &mut diagnostic.labels {
                label.message = replace_type_placeholders(&label.message, &replacements);
            }
            for note in &mut diagnostic.notes {
                *note = replace_type_placeholders(note, &replacements);
            }
            if let Some(help) = &mut diagnostic.help {
                *help = replace_type_placeholders(help, &replacements);
            }
            diagnostic
        })
        .collect()
}

/// Collect visible item names from multiple modules keyed by definition ID.
/// 从多个模块收集可见项名称，并按定义 ID 建立映射。
pub fn collect_item_names_from_modules<'a>(
    modules: impl IntoIterator<Item = &'a Module>,
) -> HashMap<DefId, String> {
    let mut names = HashMap::new();
    for module in modules {
        for item in &module.items {
            match &item.kind {
                HirItemKind::Fn(def) => {
                    names.insert(item.id, def.name.clone());
                }
                HirItemKind::Expr(_) => {}
                HirItemKind::Struct(def) => {
                    names.insert(item.id, def.name.clone());
                }
                HirItemKind::Enum(def) => {
                    names.insert(item.id, def.name.clone());
                    for variant in &def.variants {
                        names.insert(variant.id, variant.name.clone());
                    }
                }
                HirItemKind::TypeAlias(def) => {
                    names.insert(item.id, def.name.clone());
                }
                HirItemKind::Trait(def) => {
                    names.insert(item.id, def.name.clone());
                }
                HirItemKind::Impl(_) => {}
            }
        }
    }
    names
}

fn replace_type_placeholders(input: &str, replacements: &[(String, String)]) -> String {
    let mut output = input.to_string();
    for (needle, replacement) in replacements {
        output = output.replace(needle, replacement);
    }
    output
}

fn format_type_with_names(ty: &Ty, names: &HashMap<DefId, String>) -> String {
    match &ty.kind {
        TyKind::Int => "Int".to_string(),
        TyKind::Float => "Float".to_string(),
        TyKind::Bool => "Bool".to_string(),
        TyKind::Char => "Char".to_string(),
        TyKind::String => "String".to_string(),
        TyKind::Unit => "()".to_string(),
        TyKind::Var(id) => format!("?{id}"),
        TyKind::Param(_, name) => name.clone(),
        TyKind::SelfType => "Self".to_string(),
        TyKind::SelfAssoc(name) => format!("Self.{name}"),
        TyKind::Tuple(items) => {
            let parts: Vec<_> = items
                .iter()
                .map(|item| format_type_with_names(item, names))
                .collect();
            format!("({})", parts.join(", "))
        }
        TyKind::Record(fields) => {
            let parts: Vec<_> = fields
                .iter()
                .map(|(name, ty)| format!("{name}: {}", format_type_with_names(ty, names)))
                .collect();
            format!("{{ {} }}", parts.join(", "))
        }
        TyKind::DynamicRecord(fields) => {
            let parts: Vec<_> = fields
                .iter()
                .map(|(name, ty)| format!("{name}: {}", format_type_with_names(ty, names)))
                .collect();
            if parts.is_empty() {
                "{ .. }".to_string()
            } else {
                format!("{{ {}, .. }}", parts.join(", "))
            }
        }
        TyKind::SafeRecordBase(fields) => {
            let parts: Vec<_> = fields
                .iter()
                .map(|(name, ty)| format!("{name}: {}", format_type_with_names(ty, names)))
                .collect();
            if parts.is_empty() {
                "RecordOrOption[{ .. }]".to_string()
            } else {
                format!("RecordOrOption[{{ {}, .. }}]", parts.join(", "))
            }
        }
        TyKind::Fn(params, ret) => {
            let params: Vec<_> = params
                .iter()
                .map(|param| format_type_with_names(param, names))
                .collect();
            format!(
                "({}) -> {}",
                params.join(", "),
                format_type_with_names(ret, names)
            )
        }
        TyKind::Forall(params, inner) => {
            format!(
                "forall {}. {}",
                params.join(", "),
                format_type_with_names(inner, names)
            )
        }
        TyKind::Named(def_id, args) => {
            if let Some(formatted) =
                format_builtin_named_type(*def_id, args, &|arg| format_type_with_names(arg, names))
            {
                formatted
            } else if let Some(name) = names.get(def_id) {
                if args.is_empty() {
                    name.clone()
                } else {
                    let args: Vec<_> = args
                        .iter()
                        .map(|arg| format_type_with_names(arg, names))
                        .collect();
                    format!("{name}[{}]", args.join(", "))
                }
            } else {
                n3v3_typeck::format_type(ty)
            }
        }
        TyKind::Unknown => "_".to_string(),
    }
}
