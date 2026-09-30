//! The `n3v3 repl` command.
//! `n3v3 repl` 命令。

use crate::{commands::diagnostics, output};
use n3v3_common::Span;
use n3v3_eval::{EvaluableModuleRef, Evaluator, Value, builtins};
use n3v3_frontend::{
    FrontendSession, SessionDefinedBinding, SessionDisplayError, SessionModuleContext,
    SessionPreparedModule, SessionVisibleState,
};
use n3v3_hir::{DefId, Module, ModuleId};
use n3v3_std::stdlib;
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{MatchingBracketValidator, Validator};
use rustyline::{Context, Helper};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

#[cfg(test)]
const REPL_FN_TYPE_ID: DefId = DefId(u32::MAX - 100);
#[cfg(test)]
const REPL_LAZY_TYPE_ID: DefId = DefId(u32::MAX - 101);

/// Run the REPL.
/// 运行 REPL。
pub fn run() -> Result<(), String> {
    output::info(&format!("n3v3 REPL v{}", env!("CARGO_PKG_VERSION")));
    println!("Type :help for help, :quit to exit");
    println!();

    let shared_names: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let mut rl: rustyline::Editor<ReplHelper, rustyline::history::FileHistory> =
        rustyline::Editor::new().map_err(|e| e.to_string())?;
    rl.set_helper(Some(ReplHelper {
        completer: ReplCompleter::new(Rc::clone(&shared_names)),
        highlighter: MatchingBracketHighlighter::new(),
        hinter: HistoryHinter {},
        validator: MatchingBracketValidator::new(),
        _names: Rc::clone(&shared_names),
    }));
    load_repl_history(&mut rl);
    let root_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut runtime_state = ReplHirState::new();
    let mut semantic_state = ReplSemanticState::with_root_dir(root_dir);

    // Buffer for multi-line input
    // 多行输入缓冲区
    let mut input_buffer = String::new();
    let mut in_multiline = false;

    loop {
        let prompt = if in_multiline { "....> " } else { "n3v3> " };
        let readline = rl.readline(prompt);
        match readline {
            Ok(line) => {
                let line_trimmed = line.trim_end();

                // Handle backslash continuation (explicit multi-line)
                if let Some(stripped) = line_trimmed.strip_suffix('\\') {
                    input_buffer.push_str(stripped);
                    input_buffer.push('\n');
                    in_multiline = true;
                    continue;
                }

                // Accumulate input
                if in_multiline {
                    input_buffer.push('\n');
                    input_buffer.push_str(&line);
                } else {
                    input_buffer = line.to_string();
                }

                let input = input_buffer.trim();

                // Empty input: reset state
                if input.is_empty() {
                    input_buffer.clear();
                    in_multiline = false;
                    continue;
                }

                // Check if input is complete:
                // - REPL commands execute immediately
                // - Ends with ; → complete statement
                // - Single line with balanced braces and no ; → bare expression, execute
                let has_unclosed = has_unclosed_braces(input);
                let is_single_line = !input.contains('\n');
                let is_complete = input.starts_with(':')
                    || input.ends_with(';')
                    || (is_single_line && !has_unclosed && !input.ends_with(';'));

                if !is_complete {
                    // Not complete yet — continue accumulating
                    in_multiline = true;
                    continue;
                }

                in_multiline = false;

                if let Err(e) = rl.add_history_entry(input) {
                    eprintln!("warning: failed to add history entry: {e}");
                }

                // Handle REPL commands
                // 处理 REPL 命令
                if input.starts_with(':') {
                    let parts: Vec<&str> = input.split_whitespace().collect();
                    let cmd = parts.first().unwrap_or(&"");

                    match *cmd {
                        ":quit" | ":q" => break,
                        ":version" | ":v" => {
                            println!("n3v3 REPL v{}", env!("CARGO_PKG_VERSION"));
                            input_buffer.clear();
                            continue;
                        }
                        ":help" | ":h" => {
                            println!("REPL Commands:");
                            // REPL 命令：
                            println!("  :version, :v      Show version");
                            println!("  :help, :h         Show this help");
                            println!("  :quit, :q         Exit the REPL");
                            println!("  :env              Show all current bindings");
                            println!("  :type <expr>      Show the type of an expression");
                            println!("  :save <file>      Save current bindings to a file");
                            println!("  :load <file>      Load and evaluate an n3v3 file");
                            println!("  :cd <dir>         Change working directory");
                            println!("  :clear            Clear all bindings (keeps builtins)");
                            println!();
                            println!("Tips:");
                            println!("  - Use 'let x = ...' to define variables");
                            println!("  - Use 'fn name(...) = ...' to define functions");
                            println!("  - All definitions persist across inputs");
                            println!("  - Type ; to finish, or continue on next line for blocks");
                            input_buffer.clear();
                            continue;
                        }
                        ":env" => {
                            let builtins_count = builtins().len();
                            let user_bindings = runtime_state.user_bindings();
                            let user_binding_count = user_bindings.len();
                            println!("Project root: {}", semantic_state.root_dir().display());
                            println!();

                            if user_bindings.is_empty() {
                                println!("(no user-defined bindings)");
                            } else {
                                println!("User-defined bindings:");
                                for (name, is_pub) in &user_bindings {
                                    let vis = if *is_pub { "pub" } else { "   " };
                                    println!("  {} {}", vis, name);
                                }
                            }
                            println!();
                            println!(
                                "({} builtins, {} user-defined)",
                                builtins_count, user_binding_count
                            );
                            input_buffer.clear();
                            continue;
                        }
                        ":type" => {
                            if parts.len() < 2 {
                                println!("Usage: :type <expression>");
                                input_buffer.clear();
                                continue;
                            }
                            let expr_str = parts[1..].join(" ");
                            match infer_repl_type(&expr_str, &runtime_state, &semantic_state) {
                                Ok(ty) => println!("{ty}"),
                                Err(error) => diagnostics::emit_session_display_error(error),
                            }
                            input_buffer.clear();
                            continue;
                        }
                        ":load" => {
                            if parts.len() < 2 {
                                println!("Usage: :load <file.n3v3>");
                                input_buffer.clear();
                                continue;
                            }
                            let file_path = parts[1..].join(" ");
                            match semantic_state
                                .load_repl_file_input(&file_path, runtime_state.is_pristine())
                            {
                                Ok(input) => {
                                    match evaluate_repl_input_with_source_name(
                                        &input.source_name,
                                        &input.source,
                                        true,
                                        &input.context,
                                        &mut runtime_state,
                                        &mut semantic_state,
                                    ) {
                                        Ok(_) => println!("Loaded: {}", input.source_name),
                                        Err(error) => {
                                            diagnostics::emit_session_display_error(error)
                                        }
                                    }
                                }
                                Err(error) => diagnostics::emit_session_display_error(error),
                            }
                            input_buffer.clear();
                            continue;
                        }
                        ":save" => {
                            if parts.len() < 2 {
                                println!("Usage: :save <file.n3v3>");
                                input_buffer.clear();
                                continue;
                            }
                            let file_path = parts[1..].join(" ");
                            match save_repl_bindings(&file_path, &runtime_state) {
                                Ok(()) => println!(
                                    "Saved {} bindings to {}",
                                    runtime_state.user_bindings().len(),
                                    file_path
                                ),
                                Err(e) => println!("Error: {e}"),
                            }
                            input_buffer.clear();
                            continue;
                        }
                        ":cd" => {
                            if parts.len() < 2 {
                                println!("Usage: :cd <directory>");
                                input_buffer.clear();
                                continue;
                            }
                            let dir_path = parts[1..].join(" ");
                            match std::env::set_current_dir(&dir_path) {
                                Ok(()) => {
                                    println!("Changed to {}", dir_path);
                                    // Rebase semantic state so module resolution uses new root.
                                    semantic_state.rebase_root(&dir_path);
                                }
                                Err(e) => println!("Error: {e}"),
                            }
                            input_buffer.clear();
                            continue;
                        }
                        ":clear" => {
                            runtime_state.clear();
                            semantic_state.clear();
                            shared_names.borrow_mut().clear();
                            println!("Environment cleared");
                            input_buffer.clear();
                            continue;
                        }
                        _ => {
                            println!("Unknown command: {}", input);
                            println!("Type :help for available commands");
                            input_buffer.clear();
                            continue;
                        }
                    }
                }

                // Prepare input for parsing - wrap bare expressions as let bindings
                // 准备用于解析的输入 - 将裸表达式包装为 let 绑定
                let prepared_input = semantic_state.prepare_repl_input(input);

                match evaluate_repl_input_with_source_name(
                    &prepared_input.source_name,
                    &prepared_input.source,
                    prepared_input.persist_defs,
                    &prepared_input.context,
                    &mut runtime_state,
                    &mut semantic_state,
                ) {
                    Ok(value) => {
                        // Update tab completion names after successful evaluation
                        let mut names: Vec<String> = runtime_state
                            .user_bindings()
                            .iter()
                            .map(|(name, _)| name.to_string())
                            .collect();
                        names.sort();
                        names.dedup();
                        *shared_names.borrow_mut() = names;

                        if !matches!(value, Value::Unit) {
                            println!("{}", format_repl_value(&value));
                        }
                    }
                    Err(error) => diagnostics::emit_session_display_error(error),
                }

                // Clear buffer after processing
                // 处理后清除缓冲区
                input_buffer.clear();
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                in_multiline = false;
                input_buffer.clear();
                continue;
            }
            Err(ReadlineError::Eof) => {
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }

    // Save history before exit
    if let Err(e) = save_repl_history(&mut rl) {
        eprintln!("warning: failed to save REPL history: {e}");
    }
    println!("Goodbye!");
    Ok(())
}

type ReplInputContext = SessionModuleContext;
type ReplSemanticState = FrontendSession;

#[derive(Clone)]
struct ReplHirState {
    evaluator: Evaluator,
    visible_state: SessionVisibleState,
    user_bindings: HashMap<String, bool>,
    evaluated_modules: HashSet<ModuleId>,
}

impl Default for ReplHirState {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplHirState {
    fn new() -> Self {
        Self::with_root_dir(PathBuf::from("."))
    }

    fn with_root_dir(_root_dir: PathBuf) -> Self {
        Self {
            evaluator: Evaluator::new().with_extra_builtins(std_builtin_values()),
            visible_state: SessionVisibleState::default(),
            user_bindings: HashMap::new(),
            evaluated_modules: HashSet::new(),
        }
    }

    fn clear(&mut self) {
        *self = Self::new();
    }

    fn is_pristine(&self) -> bool {
        self.visible_state.is_pristine()
            && self.user_bindings.is_empty()
            && self.evaluated_modules.is_empty()
    }

    fn user_bindings(&self) -> Vec<(&str, bool)> {
        let mut bindings: Vec<_> = self
            .user_bindings
            .iter()
            .map(|(name, is_pub)| (name.as_str(), *is_pub))
            .collect();
        bindings.sort_by_key(|(name, _)| *name);
        bindings
    }

    fn eval_persistent(
        &mut self,
        prepared: SessionPreparedModule,
        semantic_state: &mut ReplSemanticState,
        method_resolutions: HashMap<Span, DefId>,
    ) -> Result<Value, String> {
        self.eval_pending_loaded_modules(semantic_state)?;
        let value = self
            .evaluator
            .eval_evaluable_module(EvaluableModuleRef::new(
                &prepared.module,
                &method_resolutions,
            ))
            .map_err(|e| format!("evaluation error: {e:?}"))?;
        self.record_user_bindings(&prepared.defined_bindings);
        semantic_state.commit_prepared_module(&mut self.visible_state, prepared);
        Ok(value)
    }

    fn eval_ephemeral(
        &mut self,
        module: &Module,
        semantic_state: &ReplSemanticState,
        method_resolutions: HashMap<Span, DefId>,
    ) -> Result<Value, String> {
        self.eval_pending_loaded_modules(semantic_state)?;
        let mut evaluator = self.evaluator.clone();
        evaluator
            .eval_evaluable_module(EvaluableModuleRef::new(module, &method_resolutions))
            .map_err(|e| format!("evaluation error: {e:?}"))
    }

    fn record_user_bindings(&mut self, bindings: &[SessionDefinedBinding]) {
        for binding in bindings {
            self.user_bindings
                .insert(binding.name.clone(), binding.is_public);
        }
    }

    fn eval_pending_loaded_modules(
        &mut self,
        semantic_state: &ReplSemanticState,
    ) -> Result<(), String> {
        for entry in semantic_state.evaluable_loaded_modules_in_order() {
            if self.evaluated_modules.contains(&entry.module_id) {
                continue;
            }

            self.evaluator
                .eval_evaluable_module(EvaluableModuleRef::new(
                    &entry.module,
                    &entry.method_resolutions,
                ))
                .map_err(|e| format!("evaluation error: {e:?}"))?;
            self.evaluated_modules.insert(entry.module_id);
        }

        Ok(())
    }
}

#[cfg(test)]
fn evaluate_repl_input(
    current_source: &str,
    persist_defs: bool,
    context: &ReplInputContext,
    runtime_state: &mut ReplHirState,
    semantic_state: &mut ReplSemanticState,
) -> Result<Value, SessionDisplayError> {
    evaluate_repl_input_with_source_name(
        current_source,
        current_source,
        persist_defs,
        context,
        runtime_state,
        semantic_state,
    )
}

fn evaluate_repl_input_with_source_name(
    current_source_name: &str,
    current_source: &str,
    persist_defs: bool,
    context: &ReplInputContext,
    runtime_state: &mut ReplHirState,
    semantic_state: &mut ReplSemanticState,
) -> Result<Value, SessionDisplayError> {
    let checked_input = semantic_state.parse_checked_source_with_context_for_display_as(
        current_source_name,
        current_source,
        context,
        &runtime_state.visible_state,
    )?;
    let method_resolutions = checked_input
        .checked
        .analysis
        .semantics
        .method_resolutions
        .clone();
    let prepared = checked_input.checked.prepared;

    let value = if persist_defs {
        runtime_state.eval_persistent(prepared, semantic_state, method_resolutions)
    } else {
        runtime_state.eval_ephemeral(&prepared.module, semantic_state, method_resolutions)
    }
    .map_err(SessionDisplayError::Message)?;

    Ok(value)
}

fn std_builtin_values() -> impl Iterator<Item = (String, Value)> {
    stdlib()
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
}

fn infer_repl_type(
    expr: &str,
    runtime_state: &ReplHirState,
    state: &ReplSemanticState,
) -> Result<String, SessionDisplayError> {
    let runtime = runtime_state.clone();
    let mut semantic = state.clone();
    semantic
        .parse_format_repl_type_for_display(expr, &runtime.visible_state)?
        .ok_or_else(|| {
            SessionDisplayError::Message("internal error: failed to infer queried type".to_string())
        })
}

/// Save current REPL session to a .n3v3 file.
/// Writes all non-command input history entries as a replayable script.
fn save_repl_bindings(_path: &str, _runtime_state: &ReplHirState) -> Result<(), String> {
    // :save saves the current bindings list as a reference file.
    // Full value serialization requires evaluator introspection (future work).
    use std::io::Write;
    let mut file =
        std::fs::File::create(_path).map_err(|e| format!("cannot create {}: {e}", _path))?;

    writeln!(file, "// Saved from n3v3 REPL").unwrap();
    writeln!(file, "// To restore: review and adjust definitions below").unwrap();
    writeln!(file).unwrap();

    let bindings = _runtime_state.user_bindings();
    if bindings.is_empty() {
        writeln!(file, "// (no bindings defined in this session)").unwrap();
    } else {
        for (name, is_pub) in &bindings {
            let vis = if *is_pub { "pub " } else { "" };
            writeln!(file, "{vis}let {name} = ...; // (re-evaluate to restore)").unwrap();
        }
    }
    Ok(())
}

// ===== REPL rustyline integration =====

/// Custom REPL completer providing command, variable, and file-path completion.
struct ReplCompleter {
    file_completer: FilenameCompleter,
    /// Shared list of known names (variables, functions, builtins) for tab completion.
    names: Rc<RefCell<Vec<String>>>,
}

impl ReplCompleter {
    fn new(names: Rc<RefCell<Vec<String>>>) -> Self {
        Self {
            file_completer: FilenameCompleter::new(),
            names,
        }
    }
}

/// Built-in names always available for completion.
const BUILTIN_COMPLETIONS: &[&str] = &[
    "print",
    "println",
    "typeOf",
    "force",
    "isLazy",
    "isEvaluated",
    "toString",
    "toInt",
    "toFloat",
    "toChar",
    "io",
    "list",
    "map",
    "path",
    "Map",
    "Set",
    "option",
    "result",
    "string",
    "math",
    "fetch",
    "Some",
    "None",
    "Ok",
    "Err",
    "Int",
    "String",
    "Float",
    "Bool",
    "Char",
    "Unit",
    "Command",
    "Pipeline",
    "ProcessResult",
    "Task",
    "Event",
    "Live",
    "let",
    "fn",
    "match",
    "if",
    "else",
    "use",
    "type",
    "enum",
    "struct",
    "trait",
    "impl",
    "effect",
    "type",
];

const REPL_COMMANDS: &[&str] = &[
    ":help", ":h", ":quit", ":q", ":version", ":v", ":env", ":type", ":clear", ":load", ":save",
    ":cd",
];

const FILE_ARG_COMMANDS: &[&str] = &[":load", ":save", ":cd"];

impl Completer for ReplCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let line_up_to_pos = &line[..pos];

        // Complete REPL commands starting with ':'
        if line_up_to_pos.starts_with(':') {
            let completions: Vec<Pair> = REPL_COMMANDS
                .iter()
                .filter(|c| c.starts_with(line_up_to_pos))
                .map(|c| Pair {
                    display: c.to_string(),
                    replacement: c.to_string(),
                })
                .collect();
            if !completions.is_empty() {
                return Ok((0, completions));
            }
        }

        // Complete file paths for :load, :save, :cd
        for cmd in FILE_ARG_COMMANDS {
            let prefix = format!("{} ", cmd);
            if line_up_to_pos.starts_with(&prefix) {
                return self.file_completer.complete(line, pos, ctx);
            }
        }

        // Complete identifiers (variable/function names)
        if !line_up_to_pos.starts_with(':') {
            // Extract the last word being typed
            let word_start = line_up_to_pos
                .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
                .map(|i| i + 1)
                .unwrap_or(0);
            let partial = &line_up_to_pos[word_start..];

            if !partial.is_empty() {
                let names = self.names.borrow();
                let mut completions: Vec<Pair> = names
                    .iter()
                    .filter(|n| n.starts_with(partial) && *n != partial)
                    .map(|n| Pair {
                        display: n.clone(),
                        replacement: n.clone(),
                    })
                    .collect();
                // Also match builtins
                for name in BUILTIN_COMPLETIONS {
                    if name.starts_with(partial) && *name != partial {
                        completions.push(Pair {
                            display: name.to_string(),
                            replacement: name.to_string(),
                        });
                    }
                }
                completions.sort_by(|a, b| a.display.cmp(&b.display));
                completions.dedup_by(|a, b| a.display == b.display);
                if !completions.is_empty() {
                    return Ok((word_start, completions));
                }
            }
        }

        Ok((pos, Vec::new()))
    }
}

/// REPL helper combining completer, highlighter, hinter, and validator.
struct ReplHelper {
    completer: ReplCompleter,
    highlighter: MatchingBracketHighlighter,
    hinter: HistoryHinter,
    validator: MatchingBracketValidator,
    /// Shared list of known names for tab completion.
    _names: Rc<RefCell<Vec<String>>>,
}

impl Completer for ReplHelper {
    type Candidate = Pair;
    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        self.completer.complete(line, pos, ctx)
    }
}

impl Highlighter for ReplHelper {
    fn highlight<'l>(&self, line: &'l str, pos: usize) -> std::borrow::Cow<'l, str> {
        self.highlighter.highlight(line, pos)
    }
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        default: bool,
    ) -> std::borrow::Cow<'b, str> {
        self.highlighter.highlight_prompt(prompt, default)
    }
    fn highlight_hint<'h>(&self, hint: &'h str) -> std::borrow::Cow<'h, str> {
        self.highlighter.highlight_hint(hint)
    }
    fn highlight_candidate<'c>(
        &self,
        candidate: &'c str,
        completion: rustyline::CompletionType,
    ) -> std::borrow::Cow<'c, str> {
        self.highlighter.highlight_candidate(candidate, completion)
    }
}

impl Hinter for ReplHelper {
    type Hint = String;
    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        self.hinter.hint(line, pos, ctx)
    }
}

impl Validator for ReplHelper {
    fn validate(
        &self,
        ctx: &mut rustyline::validate::ValidationContext,
    ) -> rustyline::Result<rustyline::validate::ValidationResult> {
        self.validator.validate(ctx)
    }
    fn validate_while_typing(&self) -> bool {
        self.validator.validate_while_typing()
    }
}

impl Helper for ReplHelper {}

/// Get the n3v3 config directory (~/.config/n3v3), creating it if needed.
fn n3v3_config_dir() -> Option<PathBuf> {
    let dir = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("n3v3")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config").join("n3v3")
    } else if let Ok(profile) = std::env::var("USERPROFILE") {
        PathBuf::from(profile).join(".config").join("n3v3")
    } else {
        return None;
    };
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

/// Check if a string has unclosed braces/brackets/parens.
fn has_unclosed_braces(input: &str) -> bool {
    let mut depth_paren = 0i32;
    let mut depth_brace = 0i32;
    let mut depth_bracket = 0i32;
    let mut in_string = false;
    let mut in_char = false;
    let mut escape = false;

    for ch in input.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' => escape = true,
            '"' if !in_char => in_string = !in_string,
            '\'' if !in_string => in_char = !in_char,
            '(' if !in_string && !in_char => depth_paren += 1,
            ')' if !in_string && !in_char => depth_paren -= 1,
            '{' if !in_string && !in_char => depth_brace += 1,
            '}' if !in_string && !in_char => depth_brace -= 1,
            '[' if !in_string && !in_char => depth_bracket += 1,
            ']' if !in_string && !in_char => depth_bracket -= 1,
            _ => {}
        }
    }

    depth_paren > 0 || depth_brace > 0 || depth_bracket > 0
}

fn repl_history_path() -> Option<PathBuf> {
    n3v3_config_dir().map(|d| d.join("history"))
}

/// Load REPL command history from disk.
fn load_repl_history<H: Helper>(rl: &mut rustyline::Editor<H, rustyline::history::FileHistory>) {
    if let Some(path) = repl_history_path()
        && let Err(e) = rl.load_history(&path)
    {
        eprintln!("warning: failed to load REPL history: {e}");
    }
}

/// Save REPL command history to disk.
fn save_repl_history<H: Helper>(
    rl: &mut rustyline::Editor<H, rustyline::history::FileHistory>,
) -> Result<(), String> {
    if let Some(path) = repl_history_path() {
        rl.save_history(&path)
            .map_err(|e| format!("failed to save history: {e}"))
    } else {
        Ok(())
    }
}

/// Format a Value for user-friendly REPL display.
/// Uses Display-like formatting instead of Debug ({:?}).
fn format_repl_value(value: &Value) -> String {
    match value {
        Value::Unit => "()".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => format!("\"{}\"", s),
        Value::Char(c) => c.to_string(),
        Value::None => "None".to_string(),
        Value::Some(v) => format!("Some({})", format_repl_value(v)),
        Value::Ok(v) => format!("Ok({})", format_repl_value(v)),
        Value::Err(e) => format!("Err({})", format_repl_value(e)),
        Value::VariantWithId { name, payload, .. } => {
            format!("{}({})", name, format_repl_value(payload))
        }
        Value::Variant(name, payload) => format!("{}({})", name, format_repl_value(payload)),
        Value::List(items) => {
            let items_str: Vec<String> = items.iter().map(format_repl_value).collect();
            format!("[{}]", items_str.join(", "))
        }
        Value::Tuple(elems) => {
            let elems_str: Vec<String> = elems.iter().map(format_repl_value).collect();
            format!("({})", elems_str.join(", "))
        }
        Value::Record(fields) => {
            let fields_str: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{} = {}", k, format_repl_value(v)))
                .collect();
            format!("{{ {} }}", fields_str.join(", "))
        }
        Value::Map(m) => {
            let entries: Vec<String> = m
                .iter()
                .map(|(k, v)| format!("{}: {}", k, format_repl_value(v)))
                .collect();
            format!("Map({{ {} }})", entries.join(", "))
        }
        Value::Set(s) => {
            let items: Vec<String> = s.iter().cloned().collect();
            format!("Set({{ {} }})", items.join(", "))
        }
        Value::Path(p) => format!("{}", p.display()),
        Value::Command(c) => format!("Command({} {})", c.program(), c.args().join(" ")),
        Value::Pipeline(p) => {
            let stages: Vec<String> = p
                .commands()
                .iter()
                .map(|c| c.program().to_string())
                .collect();
            format!("Pipeline({})", stages.join(" | "))
        }
        Value::ProcessResult(r) => {
            if r.is_success() {
                format!("[exit {}]\n{}", r.code(), r.stdout().trim_end())
            } else {
                format!("[exit {}]\nstderr: {}", r.code(), r.stderr().trim_end())
            }
        }
        Value::Task(_) => "Task(...)".to_string(),
        Value::Redirect(_) => "Redirect(...)".to_string(),
        Value::Bytes(b) => format!("<{} bytes>", b.len()),
        Value::Event(_) => "Event(..)".to_string(),
        Value::Live(_) => "Live(..)".to_string(),
        Value::Stream(_) => "<stream>".to_string(),
        Value::Thunk(_) => "<thunk>".to_string(),
        Value::Builtin(_) => "<builtin>".to_string(),
        Value::BuiltinFn(n, _) => format!("<builtin {}>", n),
        Value::Closure { .. } => "<function>".to_string(),
        Value::VariantCtor { name, .. } => format!("<constructor {}>", name),
        Value::VariantCtorWithId { name, .. } => format!("<constructor {}>", name),
    }
}
#[cfg(test)]
mod tests {
    use super::{
        REPL_FN_TYPE_ID, REPL_LAZY_TYPE_ID, ReplHirState, ReplInputContext, ReplSemanticState,
        evaluate_repl_input, infer_repl_type,
    };
    use n3v3_common::Span;
    use n3v3_diagnostic::{ErrorCode, Severity};
    use n3v3_eval::{
        Value,
        value::{
            CommandValue, PipelineValue, ProcessResultValue, RedirectValue, TaskOutputKind,
            TaskValue,
        },
    };
    use n3v3_frontend::{FrontendSession, SessionDisplayError};
    use n3v3_hir::{DefId, ItemKind as HirItemKind, Ty, TyKind, lower};
    use n3v3_parser::parse;

    fn normalize_inference_vars(input: &str) -> String {
        let mut output = String::new();
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            output.push(ch);
            if ch == '?' {
                while matches!(chars.peek(), Some(next) if next.is_ascii_digit()) {
                    chars.next();
                }
            }
        }

        output
    }

    fn assert_repl_type_diagnostic(
        expr: &str,
        runtime: &ReplHirState,
        state: &ReplSemanticState,
        expected_code: ErrorCode,
        message_fragment: &str,
    ) {
        let err = infer_repl_type(expr, runtime, state)
            .expect_err("expected REPL type inference to fail with diagnostics");
        let SessionDisplayError::Diagnostics { diagnostics, .. } = err else {
            panic!("expected diagnostic-bearing REPL type error, got {:?}", err);
        };
        let diag = diagnostics
            .iter()
            .find(|diag| {
                diag.code == Some(expected_code) && diag.message.contains(message_fragment)
            })
            .unwrap_or_else(|| {
                panic!(
                    "expected REPL diagnostic code {:?} containing {:?}, got {:?}",
                    expected_code, message_fragment, diagnostics
                )
            });
        assert_eq!(diag.severity, Severity::Error);
    }

    use n3v3_typeck::{
        BYTES_TYPE_ID, COMMAND_TYPE_ID, LIST_TYPE_ID, MAP_TYPE_ID, OPTION_TYPE_ID, PATH_TYPE_ID,
        PIPELINE_TYPE_ID, PROCESS_RESULT_TYPE_ID, REDIRECT_TYPE_ID, RESULT_TYPE_ID, SET_TYPE_ID,
        TASK_TYPE_ID, builtin_bytes, builtin_command, builtin_event, builtin_list, builtin_live,
        builtin_map, builtin_option, builtin_path, builtin_pipeline, builtin_process_result,
        builtin_redirect, builtin_result, builtin_set, builtin_stream, builtin_task,
        format_builtin_named_type,
    };
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn unknown_ty() -> Ty {
        Ty {
            kind: TyKind::Unknown,
            span: Span::DUMMY,
        }
    }

    fn named_repl_ty(def_id: DefId, args: Vec<Ty>) -> Ty {
        Ty {
            kind: TyKind::Named(def_id, args),
            span: Span::DUMMY,
        }
    }

    fn fn_repl_ty(arity: usize) -> Ty {
        Ty {
            kind: TyKind::Fn(vec![unknown_ty(); arity], Box::new(unknown_ty())),
            span: Span::DUMMY,
        }
    }

    fn type_from_value(value: &Value) -> Ty {
        match value {
            Value::Int(_) => Ty {
                kind: TyKind::Int,
                span: Span::DUMMY,
            },
            Value::Float(_) => Ty {
                kind: TyKind::Float,
                span: Span::DUMMY,
            },
            Value::Bool(_) => Ty {
                kind: TyKind::Bool,
                span: Span::DUMMY,
            },
            Value::Char(_) => Ty {
                kind: TyKind::Char,
                span: Span::DUMMY,
            },
            Value::String(_) => Ty {
                kind: TyKind::String,
                span: Span::DUMMY,
            },
            Value::Unit => Ty {
                kind: TyKind::Unit,
                span: Span::DUMMY,
            },
            Value::Path(_) => builtin_path(Span::DUMMY),
            Value::Bytes(_) => builtin_bytes(Span::DUMMY),
            Value::Command(_) => builtin_command(Span::DUMMY),
            Value::Pipeline(_) => builtin_pipeline(Span::DUMMY),
            Value::Redirect(_) => builtin_redirect(Span::DUMMY),
            Value::Task(task) => builtin_task(task_output_type(task), Span::DUMMY),
            Value::ProcessResult(_) => builtin_process_result(Span::DUMMY),
            Value::List(items) => builtin_list(common_runtime_type(items.iter()), Span::DUMMY),
            Value::Tuple(items) => Ty {
                kind: TyKind::Tuple(items.iter().map(type_from_value).collect()),
                span: Span::DUMMY,
            },
            Value::Record(fields) => Ty {
                kind: TyKind::Record(
                    fields
                        .iter()
                        .map(|(name, value)| (name.clone(), type_from_value(value)))
                        .collect(),
                ),
                span: Span::DUMMY,
            },
            Value::Some(value) => builtin_option(type_from_value(value), Span::DUMMY),
            Value::None => builtin_option(unknown_ty(), Span::DUMMY),
            Value::Ok(value) => builtin_result(type_from_value(value), unknown_ty(), Span::DUMMY),
            Value::Err(value) => builtin_result(unknown_ty(), type_from_value(value), Span::DUMMY),
            Value::Map(_) => builtin_map(unknown_ty(), unknown_ty(), Span::DUMMY),
            Value::Set(_) => builtin_set(unknown_ty(), Span::DUMMY),
            Value::Thunk(_) => named_repl_ty(REPL_LAZY_TYPE_ID, vec![unknown_ty()]),
            Value::Builtin(builtin) => fn_repl_ty(builtin.arity),
            Value::BuiltinFn(_, _) => named_repl_ty(REPL_FN_TYPE_ID, Vec::new()),
            Value::Closure { params, .. } => fn_repl_ty(params.len()),
            Value::VariantCtor { arity, .. } | Value::VariantCtorWithId { arity, .. } => {
                fn_repl_ty(*arity)
            }
            Value::VariantWithId { name, payload, .. } => match name.as_str() {
                "Some" => builtin_option(type_from_value(payload), Span::DUMMY),
                "None" => builtin_option(unknown_ty(), Span::DUMMY),
                "Ok" => builtin_result(type_from_value(payload), unknown_ty(), Span::DUMMY),
                "Err" => builtin_result(unknown_ty(), type_from_value(payload), Span::DUMMY),
                _ => unknown_ty(),
            },
            Value::Variant(name, payload) => match name.as_str() {
                "Some" => builtin_option(type_from_value(payload), Span::DUMMY),
                "None" => builtin_option(unknown_ty(), Span::DUMMY),
                "Ok" => builtin_result(type_from_value(payload), unknown_ty(), Span::DUMMY),
                "Err" => builtin_result(unknown_ty(), type_from_value(payload), Span::DUMMY),
                _ => unknown_ty(),
            },
            Value::Event(_) => builtin_event(unknown_ty(), Span::DUMMY),
            Value::Live(_) => builtin_live(unknown_ty(), Span::DUMMY),
            Value::Stream(_) => builtin_stream(unknown_ty(), Span::DUMMY),
        }
    }

    fn task_output_type(task: &TaskValue) -> Ty {
        match task.output() {
            TaskOutputKind::ProcessResult => builtin_process_result(Span::DUMMY),
        }
    }

    fn common_runtime_type<'a>(values: impl Iterator<Item = &'a Value>) -> Ty {
        let mut iter = values;
        let Some(first) = iter.next() else {
            return unknown_ty();
        };
        let first_ty = type_from_value(first);
        let first_fmt = format_repl_type(&first_ty);
        if iter.all(|value| format_repl_type(&type_from_value(value)) == first_fmt) {
            first_ty
        } else {
            unknown_ty()
        }
    }

    fn format_repl_type(ty: &Ty) -> String {
        match &ty.kind {
            TyKind::Int => "Int".to_string(),
            TyKind::Float => "Float".to_string(),
            TyKind::Bool => "Bool".to_string(),
            TyKind::Char => "Char".to_string(),
            TyKind::String => "String".to_string(),
            TyKind::Unit => "()".to_string(),
            TyKind::Var(id) => format!("?{}", id),
            TyKind::Param(_, name) => name.clone(),
            TyKind::SelfType => "Self".to_string(),
            TyKind::SelfAssoc(name) => format!("Self.{name}"),
            TyKind::Tuple(items) => {
                let parts: Vec<_> = items.iter().map(format_repl_type).collect();
                format!("({})", parts.join(", "))
            }
            TyKind::Record(fields) => {
                let parts: Vec<_> = fields
                    .iter()
                    .map(|(name, ty)| format!("{name}: {}", format_repl_type(ty)))
                    .collect();
                format!("{{ {} }}", parts.join(", "))
            }
            TyKind::DynamicRecord(fields) => {
                let parts: Vec<_> = fields
                    .iter()
                    .map(|(name, ty)| format!("{name}: {}", format_repl_type(ty)))
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
                    .map(|(name, ty)| format!("{name}: {}", format_repl_type(ty)))
                    .collect();
                if parts.is_empty() {
                    "RecordOrOption[{ .. }]".to_string()
                } else {
                    format!("RecordOrOption[{{ {}, .. }}]", parts.join(", "))
                }
            }
            TyKind::Fn(params, ret) => {
                let params: Vec<_> = params.iter().map(format_repl_type).collect();
                format!("({}) -> {}", params.join(", "), format_repl_type(ret))
            }
            TyKind::Forall(params, inner) => {
                format!("forall {}. {}", params.join(", "), format_repl_type(inner))
            }
            TyKind::Named(def_id, args)
                if [
                    LIST_TYPE_ID,
                    OPTION_TYPE_ID,
                    RESULT_TYPE_ID,
                    MAP_TYPE_ID,
                    SET_TYPE_ID,
                ]
                .contains(def_id) =>
            {
                format_builtin_named_type(*def_id, args, &format_repl_type)
                    .unwrap_or_else(|| n3v3_typeck::format_type(ty))
            }
            TyKind::Named(def_id, _) if *def_id == REPL_FN_TYPE_ID => "Fn".to_string(),
            TyKind::Named(def_id, args) if *def_id == REPL_LAZY_TYPE_ID => {
                format!("Lazy[{}]", format_repl_type(&args[0]))
            }
            TyKind::Named(_, _) => n3v3_typeck::format_type(ty),
            TyKind::Unknown => "_".to_string(),
        }
    }

    #[test]
    fn repl_type_infers_basic_expression() {
        let runtime = ReplHirState::new();
        let state = ReplSemanticState::default();
        let ty = infer_repl_type("1 + 2", &runtime, &state).expect("type inference should succeed");
        assert_eq!(ty, "Int");
    }

    #[test]
    fn repl_type_uses_persistent_bindings() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("let x = 41;", true, &context, &mut runtime, &mut state)
            .expect("definition should evaluate");

        let ty = infer_repl_type("x + 1", &runtime, &state).expect("type inference should succeed");
        assert_eq!(ty, "Int");
    }

    #[test]
    fn repl_type_preserves_checked_function_signature() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "fn id<T>(x: T) -> T = x;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("definition should evaluate");

        let ty = infer_repl_type("id", &runtime, &state).expect("type inference should succeed");
        assert_eq!(ty, "forall T. (T) -> T");
    }

    #[test]
    fn repl_type_uses_canonical_assoc_return_for_trait_method_calls() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            r#"
            trait Iterator {
                type Item;
                fn first(self) -> Self.Item;
            };
            impl Iterator for Int {
                type Item = String;
                fn first(self) -> Self.Item = toString(self);
            };
            "#,
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("trait definition should evaluate");

        let ty =
            infer_repl_type("1.first()", &runtime, &state).expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_formats_local_named_types_readably() {
        let source = "struct User {}; fn id(x: User) -> User = x;";
        let (ast, diagnostics) = parse(source);
        assert!(
            diagnostics.is_empty(),
            "unexpected parse diagnostics: {diagnostics:?}"
        );

        let hir = lower(&ast);
        let ty = match &hir.items[1].kind {
            HirItemKind::Fn(fn_def) => Ty {
                kind: TyKind::Fn(
                    fn_def.params.iter().map(|param| param.ty.clone()).collect(),
                    Box::new(fn_def.return_ty.clone()),
                ),
                span: Span::DUMMY,
            },
            other => panic!("expected function item, got {other:?}"),
        };

        let mut state = ReplSemanticState::default();
        state.record_module(hir.clone());
        let formatted = state.format_type_with_current_names(&ty, &hir);
        assert_eq!(formatted, "(User) -> User");
    }

    #[test]
    fn repl_type_formats_dynamic_record_shape_readably() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "let outputs = fn(inputs) inputs.dep.packages.default;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("definition should evaluate");

        let ty =
            infer_repl_type("outputs", &runtime, &state).expect("type inference should succeed");
        let normalized = normalize_inference_vars(&ty);
        // With function-level generalization (v3.19), the type is wrapped in Forall.
        assert!(
            normalized.contains("({ dep: { packages: { default: ?, .. }, .. }, .. }) -> ?"),
            "expected dynamic record function type, got {normalized}"
        );
    }

    #[test]
    fn repl_type_uses_optional_flow_result_for_try_expr() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.option = option;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type("option.some(41)? + 1", &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Int");
    }

    #[test]
    fn repl_type_uses_path_runtime_bridge_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(r#"path.fromString("/tmp/file.txt")"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Path");
    }

    #[test]
    fn repl_type_uses_typed_path_adapter_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"path.extensionPath(path.joinPath(path.fromString("/tmp"), "n3v3.txt"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Option[String]");
    }

    #[test]
    fn repl_type_uses_std_list_sort_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");
        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"list.sort(io.readDirEntryPaths(path.fromString("/tmp")))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "List[Path]");
    }

    #[test]
    fn repl_type_uses_std_list_max_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(r#"list.max([1, 3, 2])"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Option[Int]");
    }

    #[test]
    fn repl_type_uses_std_list_head_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");
        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"list.head(io.readDirEntryPaths(path.fromString("/tmp")))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Option[Path]");
    }

    #[test]
    fn repl_type_uses_std_list_reverse_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");
        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"list.reverse(io.readDirEntryPaths(path.fromString("/tmp")))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "List[Path]");
    }

    #[test]
    fn repl_type_uses_std_list_get_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");
        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"list.get(0, io.readDirEntryPaths(path.fromString("/tmp")))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Option[Path]");
    }

    #[test]
    fn repl_type_uses_std_list_cons_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");
        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"list.cons(path.fromString("/"), io.readDirEntryPaths(path.fromString("/tmp")))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "List[Path]");
    }

    #[test]
    fn repl_type_uses_std_list_take_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");
        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"list.take(2, io.readDirEntryPaths(path.fromString("/tmp")))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "List[Path]");
    }

    #[test]
    fn repl_type_uses_std_list_drop_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");
        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"list.drop(1, io.readDirEntryPaths(path.fromString("/tmp")))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "List[Path]");
    }

    #[test]
    fn repl_type_uses_std_list_contains_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");
        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"list.contains(path.fromString("/"), io.readDirEntryPaths(path.fromString("/tmp")))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Bool");
    }

    #[test]
    fn repl_type_uses_std_list_index_of_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");
        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"list.indexOf(path.fromString("/"), io.readDirEntryPaths(path.fromString("/tmp")))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Option[Int]");
    }

    #[test]
    fn repl_type_uses_std_list_sum_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(r#"list.sum([1, 2, 3])"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Int");
    }

    #[test]
    fn repl_type_uses_std_list_product_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(r#"list.product([2, 3, 4])"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Int");
    }

    #[test]
    fn repl_type_uses_std_list_replicate_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"list.replicate(2, path.fromString("/tmp"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "List[Path]");
    }

    #[test]
    fn repl_type_uses_std_list_zip_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");
        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"list.zip(io.readDirEntryPaths(path.fromString("/tmp")), [1, 2])"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "List[(Path, Int)]");
    }

    #[test]
    fn repl_type_uses_std_math_constant_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.math = math;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty =
            infer_repl_type("math.inf", &runtime, &state).expect("type inference should succeed");
        assert_eq!(ty, "Float");
    }

    #[test]
    fn repl_type_uses_std_math_conversion_results() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.math = math;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let int_ty = infer_repl_type("math.toInt(true)", &runtime, &state)
            .expect("type inference should succeed");
        let float_ty = infer_repl_type(r#"math.toFloat("1.5")"#, &runtime, &state)
            .expect("type inference should succeed");

        assert_eq!(int_ty, "Int");
        assert_eq!(float_ty, "Float");
    }

    #[test]
    fn repl_type_uses_std_math_float_predicate_results() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.math = math;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let nan_ty = infer_repl_type("math.isNan(math.nan)", &runtime, &state)
            .expect("type inference should succeed");
        let inf_ty = infer_repl_type("math.isInf(math.inf)", &runtime, &state)
            .expect("type inference should succeed");

        assert_eq!(nan_ty, "Bool");
        assert_eq!(inf_ty, "Bool");
    }

    #[test]
    fn repl_type_uses_std_math_rounding_results() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.math = math;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let floor_ty = infer_repl_type("math.floor(1.9)", &runtime, &state)
            .expect("type inference should succeed");
        let ceil_ty = infer_repl_type("math.ceil(1.1)", &runtime, &state)
            .expect("type inference should succeed");
        let round_ty = infer_repl_type("math.round(1.6)", &runtime, &state)
            .expect("type inference should succeed");

        assert_eq!(floor_ty, "Int");
        assert_eq!(ceil_ty, "Int");
        assert_eq!(round_ty, "Int");
    }

    #[test]
    fn repl_type_uses_std_math_unary_float_transform_results() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.math = math;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let sqrt_ty = infer_repl_type("math.sqrt(9.0)", &runtime, &state)
            .expect("type inference should succeed");
        let log_ty = infer_repl_type("math.log(1.0)", &runtime, &state)
            .expect("type inference should succeed");
        let log10_ty = infer_repl_type("math.log10(1000.0)", &runtime, &state)
            .expect("type inference should succeed");
        let exp_ty = infer_repl_type("math.exp(0.0)", &runtime, &state)
            .expect("type inference should succeed");

        assert_eq!(sqrt_ty, "Float");
        assert_eq!(log_ty, "Float");
        assert_eq!(log10_ty, "Float");
        assert_eq!(exp_ty, "Float");
    }

    #[test]
    fn repl_type_uses_std_math_trigonometric_results() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.math = math;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let sin_ty = infer_repl_type("math.sin(0.0)", &runtime, &state)
            .expect("type inference should succeed");
        let cos_ty = infer_repl_type("math.cos(0.0)", &runtime, &state)
            .expect("type inference should succeed");
        let tan_ty = infer_repl_type("math.tan(0.0)", &runtime, &state)
            .expect("type inference should succeed");

        assert_eq!(sin_ty, "Float");
        assert_eq!(cos_ty, "Float");
        assert_eq!(tan_ty, "Float");
    }

    #[test]
    fn repl_type_keeps_std_math_function_as_inference_hole() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.math = math;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type("math.abs(1)", &runtime, &state)
            .expect("type inference should succeed");
        assert!(
            ty.starts_with('?'),
            "expected math.abs to remain an inference hole, got {ty}",
        );
    }

    #[test]
    fn repl_type_uses_std_fetch_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.fetch = fetch;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(r#"fetch.path("Cargo.toml").cached"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Bool");
    }

    #[test]
    fn repl_type_uses_std_fetch_path_with_hash_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.fetch = fetch;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"fetch.pathWithHash("Cargo.toml", "0000000000000000000000000000000000000000000000000000000000000000").hash"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_std_fetch_url_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.fetch = fetch;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"fetch.url("https://example.com/archive.tar.gz").hash"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_std_fetch_url_with_hash_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.fetch = fetch;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"fetch.urlWithHash("https://example.com/archive.tar.gz", "0000000000000000000000000000000000000000000000000000000000000000").hash"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_std_fetch_git_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.fetch = fetch;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(r#"fetch.git("/tmp/repo", "main").hash"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_std_fetch_git_with_hash_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.fetch = fetch;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"fetch.gitWithHash("/tmp/repo", "main", "0000000000000000000000000000000000000000000000000000000000000000").hash"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_std_map_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.Map;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"Map.values(Map.insert("a", 1, Map.empty))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "List[Int]");
    }

    #[test]
    fn repl_type_uses_std_set_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.Set;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            "Set.isDisjoint(Set.fromList([1]), Set.fromList([2]))",
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Bool");
    }

    #[test]
    fn repl_type_uses_std_list_unzip_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"list.unzip([
                (path.fromString("/tmp"), 1),
                (path.fromString("/var"), 2),
            ])"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "(List[Path], List[Int])");
    }

    #[test]
    fn repl_type_uses_std_list_fold_right_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.list = list;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");
        evaluate_repl_input(
            "fn step(x, acc) = x + acc;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("function should evaluate");

        let ty = infer_repl_type(r#"list.foldRight(0, step, [1, 2, 3])"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Int");
    }

    #[test]
    fn repl_type_uses_io_read_file_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.readFilePath(path.fromString("/tmp/file.txt"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_io_read_file_bytes_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.readFileBytesPath(path.fromString("/tmp/file.bin"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Bytes");
    }

    #[test]
    fn repl_type_uses_io_read_dir_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.readDirPath(path.fromString("/tmp"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "List[String]");
    }

    #[test]
    fn repl_type_uses_io_read_dir_entry_paths_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.readDirEntryPaths(path.fromString("/tmp"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "List[Path]");
    }

    #[test]
    fn repl_type_uses_io_write_file_bytes_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.writeFileBytesPath(path.fromString("/tmp/file.out"), io.readFileBytesPath(path.fromString("/tmp/file.bin")))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "()");
    }

    #[test]
    fn repl_type_uses_io_write_file_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.writeFilePath(path.fromString("/tmp/file.out"), "hello")"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "()");
    }

    #[test]
    fn repl_type_uses_io_write_file_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.writeFile("/tmp/file.out", "hello")"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "()");
    }

    #[test]
    fn repl_type_uses_io_append_file_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.appendFile("/tmp/file.out", "hello")"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "()");
    }

    #[test]
    fn repl_type_uses_io_append_file_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.appendFilePath(path.fromString("/tmp/file.out"), "hello")"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "()");
    }

    #[test]
    fn repl_type_uses_io_append_file_bytes_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.appendFileBytesPath(path.fromString("/tmp/file.out"), io.readFileBytesPath(path.fromString("/tmp/file.bin")))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "()");
    }

    #[test]
    fn repl_type_uses_io_current_dir_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.currentDirPath()"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Path");
    }

    #[test]
    fn repl_type_uses_io_current_dir_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.currentDir()"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_io_get_env_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.getEnv("HOME")"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Option[String]");
    }

    #[test]
    fn repl_type_uses_io_home_dir_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.homeDirPath()"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Option[Path]");
    }

    #[test]
    fn repl_type_uses_io_home_dir_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.homeDir()"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Option[String]");
    }

    #[test]
    fn repl_type_uses_io_current_system_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.currentSystem()"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_io_read_file_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.readFile("/tmp/file.txt")"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_io_read_dir_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.readDir("/tmp")"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "List[String]");
    }

    #[test]
    fn repl_type_uses_io_create_dir_all_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.createDirAll("/tmp/n3v3-dir")"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "()");
    }

    #[test]
    fn repl_type_uses_io_create_dir_all_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.io = io; use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.createDirAllPath(path.fromString("/tmp/n3v3-dir"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "()");
    }

    #[test]
    fn repl_type_uses_io_remove_dir_all_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.io = io; use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.removeDirAllPath(path.fromString("/tmp/n3v3-dir"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "()");
    }

    #[test]
    fn repl_type_uses_io_remove_dir_all_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.removeDirAll("/tmp/n3v3-dir")"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "()");
    }

    #[test]
    fn repl_type_uses_io_path_exists_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.pathExists("/tmp/file.txt")"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Bool");
    }

    #[test]
    fn repl_type_uses_io_is_dir_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.isDir("/tmp")"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Bool");
    }

    #[test]
    fn repl_type_uses_io_is_file_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.isFile("/tmp/file.txt")"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Bool");
    }

    #[test]
    fn repl_type_uses_io_hash_file_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.io = io; use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.hashFilePath(path.fromString("/tmp/file.txt"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_io_hash_file_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.hashFile("/tmp/file.txt")"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_io_hash_string_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.hashString("abc")"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_io_command_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.command("printf", ["n3v3"])"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Command");
    }

    #[test]
    fn repl_type_uses_io_command_with_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.commandWith(#{ program = "printf", args = ["n3v3"], cwd = "/tmp" })"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Command");
    }

    #[test]
    fn repl_type_uses_io_exec_command_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.execCommand(io.command("rustc", ["--version"]))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "ProcessResult");
    }

    #[test]
    fn repl_type_uses_io_pipeline_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.pipeline([io.command("printf", ["n3v3"]), io.command("cat", [])])"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Pipeline");
    }

    #[test]
    fn repl_type_uses_io_pipeline_with_redirects_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.io = io;\nuse std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.pipelineWithRedirects(io.pipeline([io.command("printf", ["n3v3"]), io.command("cat", [])]), [io.redirectStdoutPath(path.fromString("/tmp/n3v3.out"))])"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Pipeline");
    }

    #[test]
    fn repl_type_uses_io_exec_pipeline_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.execPipeline(io.pipeline([io.command("printf", ["n3v3"]), io.command("cat", [])]))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "ProcessResult");
    }

    #[test]
    fn repl_type_uses_io_exec_pipeline_with_redirect_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.execPipeline(
                io.pipelineWithRedirects(
                    io.pipeline([io.command("printf", ["n3v3"]), io.command("cat", [])]),
                    [io.redirectStdoutPath(path.fromString("/tmp/n3v3.out"))]
                )
            )"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "ProcessResult");
    }

    #[test]
    fn repl_type_uses_io_exec_pipeline_with_redirects_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.execPipeline(
                io.pipelineWithRedirects(
                    io.pipeline([io.command("printf", ["n3v3"]), io.command("cat", [])]),
                    [
                        io.redirectStdoutPath(path.fromString("/tmp/n3v3.out")),
                        io.redirectStderrPath(path.fromString("/tmp/n3v3.err"))
                    ]
                )
            )"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "ProcessResult");
    }

    #[test]
    fn repl_type_uses_io_command_with_redirects_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.commandWithRedirects(
                io.command("printf", ["n3v3"]),
                [io.redirectStdoutPath(path.fromString("/tmp/n3v3.out"))]
            )"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Command");
    }

    #[test]
    fn repl_type_uses_io_redirect_stdout_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.redirectStdoutPath(path.fromString("/tmp/n3v3.out"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Redirect");
    }

    #[test]
    fn repl_type_uses_io_redirect_stderr_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.redirectStderrPath(path.fromString("/tmp/n3v3.err"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Redirect");
    }

    #[test]
    fn repl_type_uses_io_redirect_stdin_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.redirectStdinPath(path.fromString("/tmp/n3v3.in"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Redirect");
    }

    #[test]
    fn repl_type_uses_io_exec_command_with_redirect_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.execCommand(
                io.commandWithRedirects(
                    io.command("printf", ["n3v3"]),
                    [io.redirectStdoutPath(path.fromString("/tmp/n3v3.out"))]
                )
            )"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "ProcessResult");
    }

    #[test]
    fn repl_type_uses_io_exec_command_with_redirects_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.execCommand(
                io.commandWithRedirects(
                    io.command("printf", ["n3v3"]),
                    [
                        io.redirectStdoutPath(path.fromString("/tmp/n3v3.out")),
                        io.redirectStderrPath(path.fromString("/tmp/n3v3.err"))
                    ]
                )
            )"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "ProcessResult");
    }

    #[test]
    fn repl_type_uses_io_task_command_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.taskCommand(io.command("printf", ["n3v3"]))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Task[ProcessResult]");
    }

    #[test]
    fn repl_type_uses_io_task_pipeline_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.taskPipeline(io.pipeline([io.command("printf", ["n3v3"]), io.command("cat", [])]))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Task[ProcessResult]");
    }

    #[test]
    fn repl_type_uses_io_await_task_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.awaitTask(io.taskCommand(io.command("rustc", ["--version"])))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "ProcessResult");
    }

    #[test]
    fn repl_type_uses_io_await_pipeline_task_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.awaitTask(io.taskPipeline(io.pipeline([io.command("printf", ["n3v3"]), io.command("cat", [])])))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "ProcessResult");
    }

    #[test]
    fn repl_type_uses_io_await_tasks_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.awaitTasks([io.taskCommand(io.command("printf", ["n3v3"])), io.taskPipeline(io.pipeline([io.command("printf", ["lang"]), io.command("cat", [])]))])"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "List[ProcessResult]");
    }

    #[test]
    fn repl_type_uses_explicit_shell_exec_command_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.execCommand(io.command("sh", ["-c", "rustc --version"]))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "ProcessResult");
    }

    #[test]
    fn repl_type_uses_io_exec_with_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.execCommand(io.commandWith(#{ program = "rustc", args = ["--version"] }))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "ProcessResult");
    }

    #[test]
    fn repl_type_uses_io_process_success_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.processSuccess(io.execCommand(io.command("rustc", ["--version"])))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Bool");
    }

    #[test]
    fn repl_type_uses_io_process_stdout_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.processStdout(io.execCommand(io.command("rustc", ["--version"])))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_io_process_code_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.processCode(io.execCommand(io.command("rustc", ["--version"])))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Int");
    }

    #[test]
    fn repl_type_uses_io_process_stderr_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.processStderr(io.execCommand(io.command("rustc", ["--version"])))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_io_path_exists_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.pathExistsPath(path.fromString("/tmp/file.txt"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Bool");
    }

    #[test]
    fn repl_type_uses_io_is_dir_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(r#"io.isDirPath(path.fromString("/tmp"))"#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Bool");
    }

    #[test]
    fn repl_type_uses_io_is_file_path_result() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");
        evaluate_repl_input(
            "use std.path = path;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            r#"io.isFilePath(path.fromString("/tmp/file.txt"))"#,
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "Bool");
    }

    #[test]
    fn repl_type_uses_optional_flow_result_for_coalesce_expr() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.option = option;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type("option.none ?? 5", &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Int");
    }

    #[test]
    fn repl_type_uses_optional_flow_result_for_safe_field_coalesce_expr() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.option = option;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type(
            "option.some(#{ name = \"test\" })?.name ?? \"default\"",
            &runtime,
            &state,
        )
        .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_uses_optional_flow_result_for_builtin_result_try_expr() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.result = result;",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("use should evaluate");

        let ty = infer_repl_type("result.ok(41)? + 1", &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Int");
    }

    #[test]
    fn repl_type_uses_optional_flow_result_for_enum_some_try_expr() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "enum Option { Some(Int), None };",
            true,
            &context,
            &mut runtime,
            &mut state,
        )
        .expect("enum definition should evaluate");

        let ty = infer_repl_type("Some(41)? + 1", &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "Int");
    }

    #[test]
    fn repl_type_uses_optional_flow_result_for_record_safe_field_expr() {
        let runtime = ReplHirState::new();
        let state = ReplSemanticState::default();

        let ty = infer_repl_type(r#"#{ name = "test" }?.name ?? "default""#, &runtime, &state)
            .expect("type inference should succeed");
        assert_eq!(ty, "String");
    }

    #[test]
    fn repl_type_reports_invalid_try_optional_flow_diagnostic() {
        let runtime = ReplHirState::new();
        let state = ReplSemanticState::default();
        assert_repl_type_diagnostic(
            "41?",
            &runtime,
            &state,
            ErrorCode::TypeMismatch,
            "`?` expects Option-like or Result-like value",
        );
    }

    #[test]
    fn repl_type_reports_invalid_coalesce_optional_flow_diagnostic() {
        let runtime = ReplHirState::new();
        let state = ReplSemanticState::default();
        assert_repl_type_diagnostic(
            "41 ?? 0",
            &runtime,
            &state,
            ErrorCode::TypeMismatch,
            "`??` expects Option-like value",
        );
    }

    #[test]
    fn repl_type_reports_invalid_safe_field_boundary_diagnostic() {
        let runtime = ReplHirState::new();
        let state = ReplSemanticState::default();
        assert_repl_type_diagnostic(
            r#"42?.name ?? "default""#,
            &runtime,
            &state,
            ErrorCode::TypeMismatch,
            "safe field access requires a record or Option[Record]",
        );
    }

    #[test]
    fn repl_type_reports_invalid_io_read_file_path_diagnostic() {
        let mut runtime = ReplHirState::new();
        let mut state = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("use std.io = io;", true, &context, &mut runtime, &mut state)
            .expect("use should evaluate");

        assert_repl_type_diagnostic(
            r#"io.readFilePath("/tmp/file.txt")"#,
            &runtime,
            &state,
            ErrorCode::TypeMismatch,
            "type mismatch",
        );
    }

    #[test]
    fn repl_runtime_value_type_formatting_is_readable() {
        let ty = type_from_value(&Value::List(std::rc::Rc::new(vec![Value::Int(1.into())])));
        assert_eq!(format_repl_type(&ty), "List[Int]");
    }

    #[test]
    fn repl_runtime_value_type_formatting_uses_path_builtin_name() {
        let ty = type_from_value(&Value::Path(std::rc::Rc::new(PathBuf::from("/tmp/n3v3"))));
        assert!(matches!(ty.kind, TyKind::Named(def_id, _) if def_id == PATH_TYPE_ID));
        assert_eq!(format_repl_type(&ty), "Path");
    }

    #[test]
    fn repl_runtime_value_type_formatting_uses_bytes_builtin_name() {
        let ty = type_from_value(&Value::Bytes(std::rc::Rc::new(vec![
            0xde, 0xad, 0xbe, 0xef,
        ])));
        assert!(matches!(ty.kind, TyKind::Named(def_id, _) if def_id == BYTES_TYPE_ID));
        assert_eq!(format_repl_type(&ty), "Bytes");
    }

    #[test]
    fn repl_runtime_value_type_formatting_uses_command_builtin_name() {
        let ty = type_from_value(&Value::Command(std::rc::Rc::new(CommandValue::new(
            "printf",
            vec!["n3v3".to_string()],
        ))));
        assert!(matches!(ty.kind, TyKind::Named(def_id, _) if def_id == COMMAND_TYPE_ID));
        assert_eq!(format_repl_type(&ty), "Command");
    }

    #[test]
    fn repl_runtime_value_type_formatting_uses_pipeline_builtin_name() {
        let ty = type_from_value(&Value::Pipeline(std::rc::Rc::new(PipelineValue::new(
            vec![
                std::rc::Rc::new(CommandValue::new("printf", vec!["n3v3".to_string()])),
                std::rc::Rc::new(CommandValue::new("cat", Vec::new())),
            ],
        ))));
        assert!(matches!(ty.kind, TyKind::Named(def_id, _) if def_id == PIPELINE_TYPE_ID));
        assert_eq!(format_repl_type(&ty), "Pipeline");
    }

    #[test]
    fn repl_runtime_value_type_formatting_uses_redirect_builtin_name() {
        let ty = type_from_value(&Value::Redirect(std::rc::Rc::new(
            RedirectValue::stdout_path("/tmp/n3v3.out"),
        )));
        assert!(matches!(ty.kind, TyKind::Named(def_id, _) if def_id == REDIRECT_TYPE_ID));
        assert_eq!(format_repl_type(&ty), "Redirect");
    }

    #[test]
    fn repl_runtime_value_type_formatting_uses_process_result_builtin_name() {
        let ty = type_from_value(&Value::ProcessResult(std::rc::Rc::new(
            ProcessResultValue::new(0, true, "stdout", "stderr"),
        )));
        assert!(matches!(ty.kind, TyKind::Named(def_id, _) if def_id == PROCESS_RESULT_TYPE_ID));
        assert_eq!(format_repl_type(&ty), "ProcessResult");
    }

    #[test]
    fn repl_runtime_value_type_formatting_uses_task_builtin_name() {
        let ty = type_from_value(&Value::Task(std::rc::Rc::new(
            n3v3_eval::value::TaskValue::command_process_result(std::rc::Rc::new(
                CommandValue::new("printf", vec!["n3v3".to_string()]),
            )),
        )));
        assert!(matches!(ty.kind, TyKind::Named(def_id, _) if def_id == TASK_TYPE_ID));
        assert_eq!(format_repl_type(&ty), "Task[ProcessResult]");
    }

    #[test]
    fn repl_hir_runtime_persists_bindings_across_inputs() {
        let mut runtime = ReplHirState::new();
        let mut semantic = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        let value = evaluate_repl_input("let x = 41;", true, &context, &mut runtime, &mut semantic)
            .expect("definition should evaluate");
        assert_eq!(value, Value::Int(41.into()));

        let expr = FrontendSession::prepare_repl_source("x + 1");
        let value = evaluate_repl_input(
            &expr.source,
            expr.persist_defs,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect("expression should evaluate");
        assert_eq!(value, Value::Int(42.into()));
    }

    #[test]
    fn repl_hir_runtime_preserves_method_calls_across_inputs() {
        let mut runtime = ReplHirState::new();
        let mut semantic = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            r#"
            trait Twice { fn twice(self) -> Int; };
            impl Twice for Int {
                fn twice(self) -> Int = self + self;
            };
            "#,
            true,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect("trait definition should evaluate");

        let expr = FrontendSession::prepare_repl_source("21.twice()");
        let value = evaluate_repl_input(
            &expr.source,
            expr.persist_defs,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect("cross-input method call should evaluate");
        assert_eq!(value, Value::Int(42.into()));
    }

    #[test]
    fn repl_hir_runtime_preserves_std_imports_across_inputs() {
        let mut runtime = ReplHirState::new();
        let mut semantic = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use std.string = string;",
            true,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect("use should evaluate");

        let expr = FrontendSession::prepare_repl_source(r#"string.len("abcd")"#);
        let value = evaluate_repl_input(
            &expr.source,
            expr.persist_defs,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect("stdlib call should evaluate");
        assert_eq!(value, Value::Int(4.into()));
    }

    #[test]
    fn repl_hir_runtime_preserves_redefinition_across_inputs() {
        let mut runtime = ReplHirState::new();
        let mut semantic = ReplSemanticState::default();
        let context = ReplInputContext::repl();

        evaluate_repl_input("let x = 1;", true, &context, &mut runtime, &mut semantic)
            .expect("first definition should evaluate");
        let value = evaluate_repl_input(
            "let x = x + 1;",
            true,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect("redefinition should evaluate");
        assert_eq!(value, Value::Int(2.into()));

        let expr = FrontendSession::prepare_repl_source("x");
        let value = evaluate_repl_input(
            &expr.source,
            expr.persist_defs,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect("latest binding should evaluate");
        assert_eq!(value, Value::Int(2.into()));

        let ty = infer_repl_type("x", &runtime, &semantic).expect("type query should succeed");
        assert_eq!(ty, "Int");
    }

    #[test]
    fn repl_hir_runtime_preserves_project_module_item_imports() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("math.n3v3"), "fn add(x, y) = x + y;").unwrap();

        let mut runtime = ReplHirState::with_root_dir(temp_dir.path().to_path_buf());
        let mut semantic = ReplSemanticState::with_root_dir(temp_dir.path());
        let context = ReplInputContext::repl();

        evaluate_repl_input(
            "use math (add);",
            true,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect("module import should evaluate");

        let expr = FrontendSession::prepare_repl_source("add(1, 2)");
        let value = evaluate_repl_input(
            &expr.source,
            expr.persist_defs,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect("imported function should evaluate");
        assert_eq!(value, Value::Int(3.into()));
    }

    #[test]
    fn repl_hir_runtime_preserves_project_module_namespace_imports() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("math.n3v3"), "fn add(x, y) = x + y;").unwrap();

        let mut runtime = ReplHirState::with_root_dir(temp_dir.path().to_path_buf());
        let mut semantic = ReplSemanticState::with_root_dir(temp_dir.path());
        let context = ReplInputContext::repl();

        // Namespace import: brings module into scope, items accessible via selective import
        evaluate_repl_input("use math;", true, &context, &mut runtime, &mut semantic)
            .expect("module namespace import should evaluate");

        // Selective import from namespaced module — dotted access math.add is
        // TODO: namespace-qualified dotted access (math.add) not yet resolved for
        // project-local modules; selective import is the working path.
        evaluate_repl_input(
            "use math (add);",
            true,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect("selective import should evaluate");

        let expr = FrontendSession::prepare_repl_source("add(20, 22)");
        let value = evaluate_repl_input(
            &expr.source,
            expr.persist_defs,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect("imported function should evaluate");
        assert_eq!(value, Value::Int(42.into()));
    }

    #[test]
    fn repl_hir_runtime_loads_file_with_relative_module_imports() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join("app")).unwrap();
        fs::write(
            temp_dir.path().join("app").join("helper.n3v3"),
            "fn inc(x) = x + 1;",
        )
        .unwrap();
        fs::write(
            temp_dir.path().join("app").join("mod.n3v3"),
            "use self.helper (inc); let answer = inc(41);",
        )
        .unwrap();

        let mut runtime = ReplHirState::with_root_dir(temp_dir.path().to_path_buf());
        let mut semantic = ReplSemanticState::with_root_dir(temp_dir.path());
        let context = semantic
            .repl_context_for_file(
                temp_dir.path().join("app").join("mod.n3v3"),
                runtime.is_pristine(),
            )
            .expect("file context should resolve");

        let source = fs::read_to_string(temp_dir.path().join("app").join("mod.n3v3")).unwrap();
        let value = evaluate_repl_input(&source, true, &context, &mut runtime, &mut semantic)
            .expect("loaded file should evaluate");
        assert_eq!(value, Value::Int(42.into()));

        let expr = FrontendSession::prepare_repl_source("answer");
        let context = semantic.repl_context();
        let value = evaluate_repl_input(
            &expr.source,
            expr.persist_defs,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect("loaded binding should persist");
        assert_eq!(value, Value::Int(42.into()));
    }

    #[test]
    fn repl_reports_type_errors_from_newly_imported_modules() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("broken.n3v3"), "fn bad() = 1 + true;").unwrap();

        let mut runtime = ReplHirState::with_root_dir(temp_dir.path().to_path_buf());
        let mut semantic = ReplSemanticState::with_root_dir(temp_dir.path());
        let context = ReplInputContext::repl();

        let error = evaluate_repl_input(
            "use broken (bad);",
            true,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect_err("importing a broken module should surface diagnostics");

        match error {
            SessionDisplayError::LoadedModules(entries) => {
                assert_eq!(entries.len(), 1);
                assert!(entries[0].file_path.ends_with("broken.n3v3"));
                assert!(
                    entries[0]
                        .diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.severity == Severity::Error)
                );
            }
            other => panic!("expected module diagnostics, got {other:?}"),
        }
    }

    #[test]
    fn repl_checked_input_reports_type_errors_from_newly_imported_modules() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("broken.n3v3"), "fn bad() = 1 + true;").unwrap();

        let runtime = ReplHirState::with_root_dir(temp_dir.path().to_path_buf());
        let mut semantic = ReplSemanticState::with_root_dir(temp_dir.path());
        let context = ReplInputContext::repl();

        let error = semantic
            .parse_checked_source_with_context_for_display(
                "use broken (bad);",
                &context,
                &runtime.visible_state,
            )
            .expect_err("broken imports should surface loaded-module diagnostics through the shared checked-input layer");

        match error {
            SessionDisplayError::LoadedModules(entries) => {
                assert_eq!(entries.len(), 1);
                assert!(entries[0].file_path.ends_with("broken.n3v3"));
                assert!(
                    entries[0]
                        .diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.severity == Severity::Error)
                );
            }
            other => panic!("expected module diagnostics, got {other:?}"),
        }
    }

    #[test]
    fn repl_rechecks_prior_dependency_errors_before_new_input() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("broken.n3v3"), "fn bad() = 1 + true;").unwrap();

        let mut runtime = ReplHirState::with_root_dir(temp_dir.path().to_path_buf());
        let mut semantic = ReplSemanticState::with_root_dir(temp_dir.path());
        let context = ReplInputContext::repl();

        let first_error = evaluate_repl_input(
            "use broken (bad);",
            true,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect_err("the first input should report the broken dependency");
        assert!(matches!(first_error, SessionDisplayError::LoadedModules(_)));

        let second_error = evaluate_repl_input(
            r#"use std.io = io; let content = io.readFile("config.n3v3");"#,
            true,
            &context,
            &mut runtime,
            &mut semantic,
        )
        .expect_err("a prior broken dependency must block later input");
        let SessionDisplayError::LoadedModules(entries) = second_error else {
            panic!("expected prior dependency diagnostics, got {second_error:?}");
        };
        assert_eq!(entries.len(), 1);
        assert!(entries[0].file_path.ends_with("broken.n3v3"));
    }

    #[test]
    fn repl_can_switch_project_root_after_clear_for_load() {
        let first = TempDir::new().unwrap();
        let second = TempDir::new().unwrap();
        fs::create_dir_all(second.path().join("app")).unwrap();
        fs::write(
            second.path().join("app").join("helper.n3v3"),
            "fn inc(x) = x + 1;",
        )
        .unwrap();
        fs::write(
            second.path().join("app").join("mod.n3v3"),
            "use self.helper (inc); let answer = inc(41);",
        )
        .unwrap();

        let mut runtime = ReplHirState::with_root_dir(first.path().to_path_buf());
        runtime.clear();
        let mut semantic = ReplSemanticState::with_root_dir(first.path());

        let context = semantic
            .repl_context_for_file(
                second.path().join("app").join("mod.n3v3"),
                runtime.is_pristine(),
            )
            .expect("context should rebase to the new root");
        assert_eq!(context.root_dir.as_deref(), Some(semantic.root_dir()));

        let source = fs::read_to_string(second.path().join("app").join("mod.n3v3")).unwrap();
        let value = evaluate_repl_input(&source, true, &context, &mut runtime, &mut semantic)
            .expect("loaded binding should evaluate after root switch");
        assert_eq!(value, Value::Int(42.into()));
    }

    #[test]
    fn repl_rejects_project_root_switch_when_session_is_not_clear() {
        let first = TempDir::new().unwrap();
        let second = TempDir::new().unwrap();
        fs::write(second.path().join("main.n3v3"), "let answer = 42;").unwrap();

        let mut runtime = ReplHirState::with_root_dir(first.path().to_path_buf());
        let mut semantic = ReplSemanticState::with_root_dir(first.path());
        let context = ReplInputContext::repl();
        evaluate_repl_input("let x = 1;", true, &context, &mut runtime, &mut semantic)
            .expect("definition should evaluate");

        let error = semantic
            .repl_context_for_file(second.path().join("main.n3v3"), runtime.is_pristine())
            .expect_err("non-empty sessions should not silently mix project roots");
        assert!(
            error.to_string().contains(":clear"),
            "expected guidance to clear the session, got {error}"
        );
    }
}
