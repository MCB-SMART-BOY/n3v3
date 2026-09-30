//! Compatibility multi-module frontend driver.
//! 兼容式多模块前端驱动。

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use n3v3_common::Span;
use n3v3_hir::{DefId, ModuleId, ModuleInfo, ModuleLoadError, ModuleLoader};
use n3v3_parser::parse;
use n3v3_typeck::TypeChecker;

use crate::{
    Diagnostic, DiagnosticStats, Module, ModuleSemantics, SourceFile, collect_diagnostic_stats,
    collect_item_names_from_modules, collect_module_semantics, diagnostics_have_errors,
    read_diagnostic_source, rewrite_diagnostics_with_names,
};

/// Per-module semantic analysis produced by the compatibility driver.
/// 兼容驱动产出的单模块语义分析结果。
#[derive(Debug, Clone)]
pub struct ModuleAnalysis {
    /// Diagnostics for this module after parse/type analysis.
    /// 当前模块在解析/类型检查后的诊断。
    pub diagnostics: Vec<Diagnostic>,
    /// Canonical semantic side tables for this module.
    /// 当前模块的规范语义 side tables。
    pub semantics: ModuleSemantics,
}

/// Multi-module program analysis produced by the compatibility driver.
/// 兼容驱动产出的多模块程序分析结果。
#[derive(Debug, Clone)]
pub struct ProgramAnalysis {
    root_id: ModuleId,
    loader: ModuleLoader,
    modules: HashMap<ModuleId, ModuleAnalysis>,
    type_names: HashMap<DefId, String>,
}

/// Dependency-first program module diagnostics entry.
/// 依赖优先的程序模块诊断条目。
#[derive(Debug, Clone)]
pub struct ProgramDiagnosticModule {
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

/// Program-wide diagnostic counts grouped by blocking/non-blocking role.
/// 按阻断/非阻断角色分组的程序级诊断计数。
pub type ProgramDiagnosticStats = DiagnosticStats;

/// Dependency-first program module entry with a successfully parsed AST.
/// 带有成功解析 AST 的依赖优先程序模块条目。
#[derive(Debug, Clone)]
pub struct ProgramParsedModule {
    /// Loaded module id.
    /// 已加载模块 ID。
    pub module_id: ModuleId,
    /// Backing source path on disk.
    /// 模块对应的磁盘路径。
    pub file_path: PathBuf,
    /// Resolved module path segments.
    /// 解析后的模块路径段。
    pub module_path: Vec<String>,
    /// Parsed AST for this module.
    /// 当前模块的已解析 AST。
    pub ast: SourceFile,
}

/// Dependency-first program module entry with parse-clean AST and lowered HIR.
/// 带有解析成功 AST 与已降级 HIR 的依赖优先程序模块条目。
#[derive(Debug, Clone)]
pub struct ProgramLoweredModule {
    /// Loaded module id.
    /// 已加载模块 ID。
    pub module_id: ModuleId,
    /// Backing source path on disk.
    /// 模块对应的磁盘路径。
    pub file_path: PathBuf,
    /// Parsed AST for this module.
    /// 当前模块的已解析 AST。
    pub ast: SourceFile,
    /// Lowered HIR for this module.
    /// 当前模块的降级 HIR。
    pub module: Module,
    /// Final diagnostics for this module after semantic analysis.
    /// 当前模块在语义分析后的最终诊断。
    pub diagnostics: Vec<Diagnostic>,
}

/// Result of parsing one module file for compatibility consumers.
/// 面向兼容消费者的单模块文件解析结果。
#[derive(Debug, Clone)]
pub struct ModuleParseResult {
    /// Backing source path on disk.
    /// 模块对应的磁盘路径。
    pub file_path: PathBuf,
    /// Resolved module path segments.
    /// 解析后的模块路径段。
    pub module_path: Vec<String>,
    /// Source text read for parsing and diagnostic attribution.
    /// 用于解析与诊断归属展示的源码文本。
    pub source: String,
    /// Final parser diagnostics for this file.
    /// 当前文件的最终解析诊断。
    pub diagnostics: Vec<Diagnostic>,
    /// Parse-clean AST payload when parsing succeeded.
    /// 解析成功时的 AST 负载。
    pub ast: Option<SourceFile>,
}

/// Dependency-first program module entry ready for HIR evaluation.
/// 可直接用于 HIR 求值的依赖优先程序模块条目。
#[derive(Debug, Clone)]
pub struct ProgramEvaluableModule {
    /// Loaded module id.
    /// 已加载模块 ID。
    pub module_id: ModuleId,
    /// Lowered HIR for this program module.
    /// 当前程序模块的降级 HIR。
    pub module: Module,
    /// Resolved method call targets needed before evaluation.
    /// 求值前需要的方法调用解析结果。
    pub method_resolutions: HashMap<Span, DefId>,
}

/// Parse one module file into compatibility-facing diagnostics and AST payload.
/// 将单个模块文件解析为面向兼容层的诊断与 AST 负载。
pub fn parse_module_file(
    file_path: impl AsRef<Path>,
    module_path: &[String],
) -> std::io::Result<ModuleParseResult> {
    let file_path = file_path.as_ref().to_path_buf();
    let raw_source = std::fs::read_to_string(&file_path)?;
    // Strip shebang line if present (#!/usr/bin/env n3v3 ...)
    let source = if raw_source.starts_with("#!") {
        raw_source.lines().skip(1).collect::<Vec<_>>().join("\n")
    } else {
        raw_source
    };
    let (ast, diagnostics) = parse(&source);

    Ok(ModuleParseResult {
        file_path,
        module_path: module_path.to_vec(),
        source,
        ast: diagnostics.is_empty().then_some(ast),
        diagnostics,
    })
}

impl ProgramAnalysis {
    /// Entry/root module ID for this analysis.
    /// 当前分析的入口/根模块 ID。
    pub fn root_module_id(&self) -> ModuleId {
        self.root_id
    }

    /// Module load order (dependencies first).
    /// 模块加载顺序（依赖优先）。
    pub fn load_order(&self) -> &[ModuleId] {
        self.loader.load_order()
    }

    /// Borrow module info by ID.
    /// 按 ID 借用模块信息。
    pub fn module_info(&self, id: ModuleId) -> Option<&ModuleInfo> {
        self.loader.get_module(id)
    }

    /// Borrow lowered HIR by module ID.
    /// 按模块 ID 借用降级后的 HIR。
    pub fn hir_module(&self, id: ModuleId) -> Option<&Module> {
        self.loader.hir_module(id)
    }

    /// Borrow parsed AST by module ID.
    /// 按模块 ID 借用解析后的 AST。
    pub fn parsed_source(&self, id: ModuleId) -> Option<&SourceFile> {
        self.loader.parsed_source(id)
    }

    /// Borrow cached parse diagnostics by module ID.
    /// 按模块 ID 借用缓存的解析诊断。
    pub fn parsed_diagnostics(&self, id: ModuleId) -> Option<&[Diagnostic]> {
        self.loader.parsed_diagnostics(id)
    }

    /// Borrow the final diagnostics for a module.
    /// 借用模块的最终诊断。
    pub fn diagnostics(&self, id: ModuleId) -> Option<&[Diagnostic]> {
        self.modules
            .get(&id)
            .map(|module| module.diagnostics.as_slice())
    }

    /// Borrow method resolutions for a module.
    /// 借用模块的方法解析结果。
    pub fn method_resolutions(&self, id: ModuleId) -> Option<&HashMap<Span, DefId>> {
        self.modules
            .get(&id)
            .map(|module| &module.semantics.method_resolutions)
    }

    /// Borrow canonical side tables for a module.
    /// 借用模块的规范语义 side tables。
    pub fn semantics(&self, id: ModuleId) -> Option<&ModuleSemantics> {
        self.modules.get(&id).map(|module| &module.semantics)
    }

    /// Borrow the shared visible type-name map.
    /// 借用共享的可见类型名映射。
    pub fn type_names(&self) -> &HashMap<DefId, String> {
        &self.type_names
    }

    /// Summarize program diagnostics by blocking/non-blocking role.
    /// 按阻断/非阻断角色汇总程序诊断。
    pub fn diagnostic_stats(&self) -> ProgramDiagnosticStats {
        collect_diagnostic_stats(
            self.load_order()
                .iter()
                .flat_map(|module_id| self.diagnostics(*module_id).unwrap_or(&[]).iter()),
        )
    }

    /// Whether the program contains any blocking diagnostics.
    /// 当前程序是否包含任何阻断诊断。
    pub fn has_blocking_diagnostics(&self) -> bool {
        self.diagnostic_stats().has_errors()
    }

    /// Return dependency-first blocking diagnostic messages with file attribution.
    /// 返回带文件归属、按依赖优先顺序排列的阻断诊断文本。
    pub fn blocking_diagnostic_messages(&self) -> Vec<String> {
        self.load_order()
            .iter()
            .filter_map(|module_id| {
                let info = self.module_info(*module_id)?;
                Some(
                    self.diagnostics(*module_id)
                        .unwrap_or(&[])
                        .iter()
                        .filter(|diagnostic| {
                            diagnostic.severity == n3v3_diagnostic::Severity::Error
                        })
                        .map(|diagnostic| {
                            format!("{}: {}", info.file_path.display(), diagnostic.message)
                        }),
                )
            })
            .flatten()
            .collect()
    }

    fn collect_diagnostic_modules_in_order<P>(
        &self,
        include_empty: bool,
        mut predicate: P,
    ) -> Vec<ProgramDiagnosticModule>
    where
        P: FnMut(&Diagnostic) -> bool,
    {
        self.load_order()
            .iter()
            .filter_map(|module_id| {
                let info = self.module_info(*module_id)?;
                let diagnostics = self
                    .diagnostics(*module_id)?
                    .iter()
                    .filter(|diagnostic| predicate(diagnostic))
                    .cloned()
                    .collect::<Vec<_>>();
                if diagnostics.is_empty() && !include_empty {
                    return None;
                }

                Some(ProgramDiagnosticModule {
                    module_id: *module_id,
                    file_path: info.file_path.clone(),
                    source: read_diagnostic_source(&info.file_path),
                    diagnostics,
                })
            })
            .collect()
    }

    /// Return dependency-first program diagnostics entries.
    /// 返回依赖优先的程序诊断条目。
    pub fn diagnostic_modules_in_order(&self) -> Vec<ProgramDiagnosticModule> {
        self.collect_diagnostic_modules_in_order(true, |_| true)
    }

    /// Return dependency-first program syntax diagnostic entries.
    /// 返回依赖优先的程序语法诊断条目。
    ///
    /// Syntax means lexer or parser diagnostics, matching `DiagnosticKind::is_syntax()`.
    /// Callers use this to print the reason a file cannot be parsed; a lexer-only
    /// failure has no `Parser` diagnostic but must still show its detail.
    /// 语法指词法或语法诊断，与 `DiagnosticKind::is_syntax()` 一致。调用方用它打印
    /// 文件无法解析的原因；只有词法错误的文件没有 `Parser` 诊断，但同样必须显示明细。
    pub fn syntax_diagnostic_modules_in_order(&self) -> Vec<ProgramDiagnosticModule> {
        self.collect_diagnostic_modules_in_order(false, |diagnostic| diagnostic.kind.is_syntax())
    }

    /// Return dependency-first parsed program modules with parse-clean ASTs.
    /// 返回带有解析成功 AST 的依赖优先程序模块。
    pub fn parsed_modules_in_order(&self) -> Vec<ProgramParsedModule> {
        self.load_order()
            .iter()
            .filter_map(|module_id| {
                let info = self.module_info(*module_id)?;
                if !self
                    .parsed_diagnostics(*module_id)
                    .unwrap_or(&[])
                    .is_empty()
                {
                    return None;
                }
                let ast = self.parsed_source(*module_id)?.clone();
                Some(ProgramParsedModule {
                    module_id: *module_id,
                    file_path: info.file_path.clone(),
                    module_path: info.path.clone(),
                    ast,
                })
            })
            .collect()
    }

    /// Return dependency-first program modules with parse-clean AST and lowered HIR.
    /// 返回带有解析成功 AST 与已降级 HIR 的依赖优先程序模块。
    pub fn lowered_modules_in_order(&self) -> Vec<ProgramLoweredModule> {
        self.load_order()
            .iter()
            .filter_map(|module_id| {
                let info = self.module_info(*module_id)?;
                if !self
                    .parsed_diagnostics(*module_id)
                    .unwrap_or(&[])
                    .is_empty()
                {
                    return None;
                }

                Some(ProgramLoweredModule {
                    module_id: *module_id,
                    file_path: info.file_path.clone(),
                    ast: self.parsed_source(*module_id)?.clone(),
                    module: self.hir_module(*module_id)?.clone(),
                    diagnostics: self.diagnostics(*module_id)?.to_vec(),
                })
            })
            .collect()
    }

    /// Return dependency-first program modules that are ready for HIR evaluation.
    /// 返回已可用于 HIR 求值的依赖优先程序模块。
    pub fn evaluable_modules_in_order(&self) -> Vec<ProgramEvaluableModule> {
        self.load_order()
            .iter()
            .filter_map(|module_id| {
                let module = self.hir_module(*module_id)?.clone();
                let analysis = self.modules.get(module_id)?;
                if diagnostics_have_errors(&analysis.diagnostics) {
                    return None;
                }

                Some(ProgramEvaluableModule {
                    module_id: *module_id,
                    module,
                    method_resolutions: analysis.semantics.method_resolutions.clone(),
                })
            })
            .collect()
    }
}

/// Errors produced by the compatibility frontend driver.
/// 兼容前端驱动返回的错误。
#[derive(Debug, Clone)]
pub enum FrontendError {
    /// Module loading failed before semantic analysis could complete.
    /// 在语义分析开始前模块加载失败。
    ModuleLoad(ModuleLoadError),
}

impl fmt::Display for FrontendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrontendError::ModuleLoad(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for FrontendError {}

/// Compatibility driver for multi-module frontend analysis.
/// 多模块前端分析的兼容驱动。
#[derive(Debug, Clone)]
pub struct FrontendDriver {
    root_dir: PathBuf,
    std_path: Option<PathBuf>,
    flake_input_roots: HashMap<String, PathBuf>,
}

impl FrontendDriver {
    /// Create a driver rooted at a source directory.
    /// 创建一个以源码目录为根的驱动。
    pub fn new(root_dir: impl AsRef<Path>) -> Self {
        Self {
            root_dir: root_dir.as_ref().to_path_buf(),
            std_path: None,
            flake_input_roots: HashMap::new(),
        }
    }

    /// Configure an explicit std source root for module resolution.
    /// 为模块解析配置显式的 std 源码根路径。
    pub fn with_std_path(mut self, std_path: impl AsRef<Path>) -> Self {
        self.std_path = Some(std_path.as_ref().to_path_buf());
        self
    }

    /// Configure flake input roots for dependency module resolution.
    /// 为依赖模块解析配置 flake 输入根目录。
    pub fn with_flake_inputs(mut self, inputs: HashMap<String, PathBuf>) -> Self {
        self.flake_input_roots = inputs;
        self
    }

    /// Analyze a root module path and all transitively loaded modules.
    /// 分析一个根模块路径及其所有传递加载的模块。
    pub fn analyze_module_path(
        &self,
        module_path: &[String],
    ) -> Result<ProgramAnalysis, FrontendError> {
        let mut loader = ModuleLoader::new(&self.root_dir);
        if let Some(std_path) = &self.std_path {
            loader = loader.with_std_path(std_path);
        }
        if !self.flake_input_roots.is_empty() {
            loader = loader.with_flake_inputs(self.flake_input_roots.clone());
        }

        let root_id = loader
            .load_module(module_path)
            .map_err(FrontendError::ModuleLoad)?;

        let modules: Vec<&Module> = loader
            .load_order()
            .iter()
            .filter_map(|module_id| loader.hir_module(*module_id))
            .collect();
        let global_traits = TypeChecker::collect_global_trait_resolver(modules.iter().copied());
        let mut global_types = HashMap::new();
        let mut global_spans = HashMap::new();
        let mut global_fn_bounds = HashMap::new();
        let mut global_effectful_definitions = HashSet::new();
        for module in &modules {
            let (types, spans, bounds) =
                TypeChecker::collect_signatures_with_trait_resolver(module, &global_traits);
            global_types.extend(types);
            global_spans.extend(spans);
            global_fn_bounds.extend(bounds);
        }

        let type_names = collect_item_names_from_modules(
            loader
                .load_order()
                .iter()
                .filter_map(|module_id| loader.hir_module(*module_id)),
        );

        let mut modules = HashMap::new();
        for module_id in loader.load_order() {
            let parse_diagnostics = loader.parsed_diagnostics(*module_id).unwrap_or(&[]);
            if !parse_diagnostics.is_empty() {
                modules.insert(
                    *module_id,
                    ModuleAnalysis {
                        diagnostics: parse_diagnostics.to_vec(),
                        semantics: ModuleSemantics::default(),
                    },
                );
                continue;
            }

            let Some(module) = loader.hir_module(*module_id) else {
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
            let semantics = collect_module_semantics(&checker, module);
            global_effectful_definitions.extend(checker.effectful_definitions().iter().copied());
            let diagnostics =
                rewrite_diagnostics_with_names(checker.diagnostics_ref().to_vec(), &type_names);

            modules.insert(
                *module_id,
                ModuleAnalysis {
                    diagnostics,
                    semantics,
                },
            );
        }

        let has_blocking_diagnostics = modules
            .values()
            .any(|module| diagnostics_have_errors(&module.diagnostics));
        if has_blocking_diagnostics {
            for module in modules.values_mut() {
                module.semantics.action_plans.clear();
            }
        }

        Ok(ProgramAnalysis {
            root_id,
            loader,
            modules,
            type_names,
        })
    }
}

/// Analyze a module path rooted at the given source directory.
/// 分析指定源码目录下的模块路径。
pub fn analyze_module_path(
    root_dir: impl AsRef<Path>,
    module_path: &[String],
) -> Result<ProgramAnalysis, FrontendError> {
    FrontendDriver::new(root_dir).analyze_module_path(module_path)
}
