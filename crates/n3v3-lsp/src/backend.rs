//! LSP backend implementation.
//! LSP 后端实现。
//!
//! Implements the Language Server Protocol for n3v3.
//! 实现 n3v3 的语言服务器协议。

use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use n3v3_lexer::Lexer;
use n3v3_syntax::{ImplDef, Type, TypeKind};

use crate::Document;
use crate::capabilities::server_capabilities;
use crate::semantic_tokens::generate_semantic_tokens_from_ast;
use crate::stdlib_completion::completion_items as stdlib_completion_items;
use crate::symbol_index::SymbolKind as IndexSymbolKind;
use n3v3_common::Span;
use n3v3_diagnostic::{DiagnosticKind, Severity as NeveSeverity};

/// The LSP backend.
/// LSP 后端。
pub struct Backend {
    /// The LSP client. / LSP 客户端。
    client: Client,
    /// Open documents. / 打开的文档。
    documents: DashMap<String, Document>,
    /// Monotonic counter for debouncing did_change diagnostics.
    /// 单调计数器，用于 debounce did_change 诊断。
    change_generation: AtomicU64,
    /// Pending content changes keyed by URI, debounced with change_generation.
    /// 待处理的文档内容变更。
    pending_changes: DashMap<String, String>,
}

impl Backend {
    /// Create a new backend.
    /// 创建新的后端。
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: DashMap::new(),
            change_generation: AtomicU64::new(0),
            pending_changes: DashMap::new(),
        }
    }

    /// Publish diagnostics for a document.
    /// 发布文档的诊断信息。
    async fn publish_diagnostics(&self, uri: &Url, doc: &Document) {
        let diagnostics: Vec<Diagnostic> = doc
            .diagnostics
            .iter()
            .map(|diag| {
                let severity = match diag.severity {
                    NeveSeverity::Error => DiagnosticSeverity::ERROR,
                    NeveSeverity::Warning => DiagnosticSeverity::WARNING,
                    NeveSeverity::Note => DiagnosticSeverity::INFORMATION,
                };

                let primary_span = if diag.span.is_empty() {
                    diag.labels
                        .first()
                        .map(|label| label.span)
                        .unwrap_or(diag.span)
                } else {
                    diag.span
                };

                let mut message = diag.message.clone();
                for note in &diag.notes {
                    message.push_str("\n\nnote: ");
                    message.push_str(note);
                }
                if let Some(help) = &diag.help {
                    message.push_str("\n\nhelp: ");
                    message.push_str(help);
                }

                let related_information = if diag.labels.is_empty() {
                    None
                } else {
                    Some(
                        diag.labels
                            .iter()
                            .map(|label| DiagnosticRelatedInformation {
                                location: Location {
                                    uri: uri.clone(),
                                    range: range_for_span(doc, label.span),
                                },
                                message: label.message.clone(),
                            })
                            .collect(),
                    )
                };

                let source = match diag.kind {
                    DiagnosticKind::Lexer => "n3v3.lexer",
                    DiagnosticKind::Parser => "n3v3.parser",
                    DiagnosticKind::Type => "n3v3.type",
                    DiagnosticKind::Eval => "n3v3.eval",
                    DiagnosticKind::Module => "n3v3.module",
                };

                let code_description = diag.code.and_then(|code| {
                    Url::parse(&code.doc_url())
                        .ok()
                        .map(|href| CodeDescription { href })
                });

                Diagnostic {
                    range: range_for_span(doc, primary_span),
                    severity: Some(severity),
                    code: diag
                        .code
                        .map(|code| NumberOrString::String(code.as_str().to_string())),
                    code_description,
                    source: Some(source.to_string()),
                    message,
                    related_information,
                    tags: None,
                    data: None,
                }
            })
            .collect();

        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }
}

fn range_for_span(doc: &Document, span: Span) -> Range {
    let content_len = doc.content.len();
    let mut start: usize = span.start.into();
    let mut end: usize = span.end.into();

    if start > content_len {
        start = content_len;
    }
    if end > content_len {
        end = content_len;
    }
    if end < start {
        end = start;
    }

    if start == end
        && let Some(next) = next_char_offset(&doc.content, start)
    {
        end = next;
    }

    let (start_line, start_col) = doc.position_at(start);
    let (end_line, end_col) = doc.position_at(end);

    Range {
        start: Position::new(start_line, start_col),
        end: Position::new(end_line, end_col),
    }
}

fn next_char_offset(content: &str, offset: usize) -> Option<usize> {
    if offset >= content.len() {
        return None;
    }

    content[offset..]
        .chars()
        .next()
        .map(|ch| offset + ch.len_utf8())
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "n3v3-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: server_capabilities(),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "n3v3 language server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let content = params.text_document.text;

        let doc = Document::new(uri.clone(), content);
        self.publish_diagnostics(&params.text_document.uri, &doc)
            .await;
        self.documents.insert(uri, doc);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();

        if let Some(change) = params.content_changes.into_iter().next() {
            // Debounce: bump the generation counter before sleeping so newer
            // changes that arrive during the sleep will see a higher counter.
            self.change_generation.fetch_add(1, Ordering::Release);
            let generation = self.change_generation.load(Ordering::Acquire);

            // Store the incoming text so the last writer wins after the quiet period.
            self.pending_changes.insert(uri.clone(), change.text);

            // Wait for a quiet period (300 ms) before re-analysing.
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;

            // If a newer change arrived while we were sleeping, skip this one.
            if self.change_generation.load(Ordering::Acquire) != generation {
                return;
            }

            // Apply the winning content and publish diagnostics.
            if let Some((_, content)) = self.pending_changes.remove(&uri) {
                if let Some(mut doc) = self.documents.get_mut(&uri) {
                    doc.update(content);
                }
                // Re-fetch the doc immutably for diagnostics.
                if let Some(doc) = self.documents.get(&uri) {
                    self.publish_diagnostics(&params.text_document.uri, &doc)
                        .await;
                }
            }
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.to_string();

        if let Some(text) = params.text
            && let Some(mut doc) = self.documents.get_mut(&uri)
        {
            doc.update(text);
            self.publish_diagnostics(&params.text_document.uri, &doc)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.documents.remove(&uri);

        // Clear diagnostics / 清除诊断信息
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn did_change_configuration(&self, _params: DidChangeConfigurationParams) {
        // Stub: configuration changes (e.g. editor settings) are not yet
        // applied dynamically. Restart the server to pick up new settings.
    }

    async fn did_change_watched_files(&self, _params: DidChangeWatchedFilesParams) {
        // Stub: file watcher events are not yet processed.
        // A full implementation would invalidate cached module analyses
        // for changed files and re-publish diagnostics.
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let pos = params.text_document_position_params.position;

        if let Some(doc) = self.documents.get(&uri) {
            let offset = doc.offset_at(pos.line, pos.character);

            // Try to get symbol information first / 首先尝试获取符号信息
            if let Some(ref index) = doc.symbol_index
                && let Some(symbol) = index.find_definition_site_at(offset)
            {
                // Format symbol kind nicely / 格式化符号类型
                let kind_str = match symbol.kind {
                    IndexSymbolKind::Function => "function",
                    IndexSymbolKind::Variable => "variable",
                    IndexSymbolKind::Parameter => "parameter",
                    IndexSymbolKind::TypeAlias => "type alias",
                    IndexSymbolKind::Struct => "struct",
                    IndexSymbolKind::Enum => "enum",
                    IndexSymbolKind::Variant => "variant",
                    IndexSymbolKind::Trait => "trait",
                    IndexSymbolKind::Field => "field",
                    IndexSymbolKind::Method => "method",
                };

                // Get the full definition text using full_span
                // 使用 full_span 获取完整的定义文本
                let full_start: usize = symbol.full_span.start.into();
                let full_end: usize = symbol.full_span.end.into();
                let definition_text = if full_end <= doc.content.len() {
                    // Limit to first line for display / 限制显示第一行
                    let full_text = &doc.content[full_start..full_end];
                    let first_line = full_text.lines().next().unwrap_or(full_text);
                    if first_line.len() > 80 {
                        format!("{}...", &first_line[..77])
                    } else {
                        first_line.to_string()
                    }
                } else {
                    symbol.name.clone()
                };

                let mut hover_text =
                    if let Some(type_info) = doc.definition_hovers.get(&symbol.def_span) {
                        format!(
                            "**{}** `{}`\n\nType: `{}`\n\n```n3v3\n{}\n```",
                            kind_str, symbol.name, type_info, definition_text
                        )
                    } else {
                        format!(
                            "**{}** `{}`\n\n```n3v3\n{}\n```",
                            kind_str, symbol.name, definition_text
                        )
                    };

                // Append builtin docs if available / 如果有内置文档则追加
                let full_name = if let Some(prefix) = doc
                    .symbol_index
                    .as_ref()
                    .and_then(|idx| idx.find_name_at(offset))
                {
                    prefix
                } else {
                    symbol.name.clone()
                };
                if let Some(docs) = builtin_hover_docs(&full_name) {
                    hover_text.push_str("\n\n---\n");
                    hover_text.push_str(docs);
                }

                let start: usize = symbol.def_span.start.into();
                let end: usize = symbol.def_span.end.into();
                let (start_line, start_col) = doc.position_at(start);
                let (end_line, end_col) = doc.position_at(end);

                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: hover_text,
                    }),
                    range: Some(Range {
                        start: Position::new(start_line, start_col),
                        end: Position::new(end_line, end_col),
                    }),
                }));
            }

            if let Some((span, hover_text)) = doc.semantic_hover_at(offset) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("`{hover_text}`"),
                    }),
                    range: Some(range_for_span(&doc, span)),
                }));
            }

            if let Some(ref index) = doc.symbol_index
                && let Some(symbol) = index.find_definition_at(offset)
                && let Some(type_info) = doc.definition_hovers.get(&symbol.def_span)
            {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("`{type_info}`"),
                    }),
                    range: Some(range_for_span(&doc, symbol.def_span)),
                }));
            }

            // Fallback to token-based hover / 回退到基于 token 的悬停
            let lexer = Lexer::new(&doc.content);
            let (tokens, _) = lexer.tokenize();

            for token in tokens {
                let start: usize = token.span.start.into();
                let end: usize = token.span.end.into();
                if start <= offset && offset < end {
                    let token_text = &doc.content[start..end];
                    let hover_text = format!("Token: `{}`\nKind: `{:?}`", token_text, token.kind);

                    let (start_line, start_col) = doc.position_at(start);
                    let (end_line, end_col) = doc.position_at(end);

                    return Ok(Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: hover_text,
                        }),
                        range: Some(Range {
                            start: Position::new(start_line, start_col),
                            end: Position::new(end_line, end_col),
                        }),
                    }));
                }
            }
        }

        Ok(None)
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let pos = params.text_document_position_params.position;

        if let Some(doc) = self.documents.get(&uri) {
            let offset = doc.offset_at(pos.line, pos.character);

            // Find the call expression at the cursor position.
            // 查找光标位置处的调用表达式。
            if let Some(signatures) = find_call_signatures(&doc, offset) {
                // Determine active parameter based on comma count before cursor.
                // 根据光标前的逗号数量确定当前参数索引。
                let active_parameter = count_commas_before(&doc.content, offset);

                return Ok(Some(SignatureHelp {
                    signatures,
                    active_signature: Some(0),
                    active_parameter: Some(active_parameter),
                }));
            }
        }

        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let pos = params.text_document_position.position;

        let mut items = Vec::new();

        // Extract the current word prefix for relevance scoring.
        // 提取当前单词前缀以进行相关度评分。
        let prefix = self
            .documents
            .get(&uri)
            .map(|doc| word_prefix_at_position(&doc.content, &doc, pos))
            .unwrap_or_default();

        // Get context for smarter completion / 获取上下文以实现更智能的补全
        let trigger_char = params
            .context
            .as_ref()
            .and_then(|c| c.trigger_character.as_deref());

        // Check if we're after a dot (member access) / 检查是否在点后面（成员访问）
        let is_dot_completion = trigger_char == Some(".");

        if is_dot_completion {
            // Type-aware method completion / 类型感知的方法补全
            let receiver_type = if let Some(doc) = self.documents.get(&uri) {
                let dot_offset = doc.offset_at(pos.line, pos.character);
                find_receiver_type_name(&doc, dot_offset)
            } else {
                None
            };
            let mut method_items = self.get_method_completions(receiver_type.as_deref());
            for item in &mut method_items {
                item.sort_text = Some(score_sort_text(&item.label, &prefix));
            }
            items.extend(method_items);
        } else {
            // Most relevant first: document symbols from current file
            // 最相关的优先：当前文件的文档符号
            if let Some(doc) = self.documents.get(&uri) {
                let mut doc_items = self.get_document_completions(&doc, pos);
                for item in &mut doc_items {
                    item.sort_text = Some(score_sort_text(&item.label, &prefix));
                }
                items.extend(doc_items);

                // Import path completions / 导入路径补全
                if let Some(mut import_paths) = get_import_completions(&doc, pos) {
                    for item in &mut import_paths {
                        item.sort_text = Some(score_sort_text(&item.label, &prefix));
                    }
                    items.extend(import_paths);
                }
            }

            // Standard library functions / 标准库函数
            let mut stdlib_items = self.get_stdlib_completions();
            for item in &mut stdlib_items {
                item.sort_text = Some(score_sort_text(&item.label, &prefix));
            }
            items.extend(stdlib_items);

            // Types / 类型
            let mut type_items = self.get_type_completions();
            for item in &mut type_items {
                item.sort_text = Some(score_sort_text(&item.label, &prefix));
            }
            items.extend(type_items);

            // Keywords / 关键字
            let mut keyword_items = self.get_keyword_completions();
            for item in &mut keyword_items {
                item.sort_text = Some(score_sort_text(&item.label, &prefix));
            }
            items.extend(keyword_items);
        }

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn completion_resolve(&self, mut item: CompletionItem) -> Result<CompletionItem> {
        // Enrich completion items with documentation on resolve.
        // 在解析时丰富补全项的文档信息。
        if item.documentation.is_none() {
            let docs = completion_documentation(&item.label, item.kind);
            if let Some(doc) = docs {
                item.documentation = Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: doc,
                }));
            }
        }
        Ok(item)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri.to_string();

        if let Some(doc) = self.documents.get(&uri)
            && let Ok(formatted) = n3v3_fmt::format(&doc.content)
            && formatted != doc.content
        {
            let lines: Vec<&str> = doc.content.lines().collect();
            let end_line = lines.len().saturating_sub(1) as u32;
            let end_col = lines.last().map(|l| l.len() as u32).unwrap_or(0);

            return Ok(Some(vec![TextEdit {
                range: Range {
                    start: Position::new(0, 0),
                    end: Position::new(end_line, end_col),
                },
                new_text: formatted,
            }]));
        }

        Ok(None)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.to_string();

        if let Some(doc) = self.documents.get(&uri) {
            // Use AST-based semantic tokens for accurate type/definition classification.
            // Falls back to lexer-based tokens for anything not covered by the AST.
            // 使用基于 AST 的语义 token 进行准确的类型/定义分类。
            // 对于 AST 未覆盖的部分，回退到基于词法分析器的 token。
            let semantic_tokens = generate_semantic_tokens_from_ast(&doc.content);

            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: semantic_tokens,
            })));
        }

        Ok(None)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.to_string();

        if let Some(doc) = self.documents.get(&uri)
            && let Some(ref ast) = doc.ast
        {
            let mut symbols = Vec::new();
            let span_range = |span: n3v3_common::Span| {
                let start: usize = span.start.into();
                let end: usize = span.end.into();
                let (start_line, start_col) = doc.position_at(start);
                let (end_line, end_col) = doc.position_at(end);
                Range {
                    start: Position::new(start_line, start_col),
                    end: Position::new(end_line, end_col),
                }
            };

            for item in &ast.items {
                use n3v3_syntax::ItemKind;

                let symbol = match &item.kind {
                    ItemKind::Let(def) => {
                        let name = format!("{:?}", def.pattern.kind);
                        #[allow(deprecated)]
                        DocumentSymbol {
                            name,
                            detail: None,
                            kind: SymbolKind::VARIABLE,
                            tags: None,
                            deprecated: None,
                            range: span_range(item.span),
                            selection_range: span_range(def.pattern.span),
                            children: None,
                        }
                    }
                    ItemKind::Fn(def) =>
                    {
                        #[allow(deprecated)]
                        DocumentSymbol {
                            name: def.name.name.clone(),
                            detail: None,
                            kind: SymbolKind::FUNCTION,
                            tags: None,
                            deprecated: None,
                            range: span_range(item.span),
                            selection_range: span_range(def.name.span),
                            children: None,
                        }
                    }
                    ItemKind::TypeAlias(def) =>
                    {
                        #[allow(deprecated)]
                        DocumentSymbol {
                            name: def.name.name.clone(),
                            detail: None,
                            kind: SymbolKind::TYPE_PARAMETER,
                            tags: None,
                            deprecated: None,
                            range: span_range(item.span),
                            selection_range: span_range(def.name.span),
                            children: None,
                        }
                    }
                    ItemKind::Struct(def) =>
                    {
                        #[allow(deprecated)]
                        DocumentSymbol {
                            name: def.name.name.clone(),
                            detail: None,
                            kind: SymbolKind::STRUCT,
                            tags: None,
                            deprecated: None,
                            range: span_range(item.span),
                            selection_range: span_range(def.name.span),
                            children: None,
                        }
                    }
                    ItemKind::Enum(def) =>
                    {
                        #[allow(deprecated)]
                        DocumentSymbol {
                            name: def.name.name.clone(),
                            detail: None,
                            kind: SymbolKind::ENUM,
                            tags: None,
                            deprecated: None,
                            range: span_range(item.span),
                            selection_range: span_range(def.name.span),
                            children: None,
                        }
                    }
                    ItemKind::Trait(def) => {
                        let mut children = Vec::new();

                        for assoc in &def.assoc_types {
                            #[allow(deprecated)]
                            children.push(DocumentSymbol {
                                name: assoc.name.name.clone(),
                                detail: None,
                                kind: SymbolKind::TYPE_PARAMETER,
                                tags: None,
                                deprecated: None,
                                range: span_range(assoc.span),
                                selection_range: span_range(assoc.name.span),
                                children: None,
                            });
                        }

                        for trait_item in &def.items {
                            #[allow(deprecated)]
                            children.push(DocumentSymbol {
                                name: trait_item.name.name.clone(),
                                detail: None,
                                kind: SymbolKind::METHOD,
                                tags: None,
                                deprecated: None,
                                range: span_range(trait_item.span),
                                selection_range: span_range(trait_item.name.span),
                                children: None,
                            });
                        }

                        #[allow(deprecated)]
                        DocumentSymbol {
                            name: def.name.name.clone(),
                            detail: None,
                            kind: SymbolKind::INTERFACE,
                            tags: None,
                            deprecated: None,
                            range: span_range(item.span),
                            selection_range: span_range(def.name.span),
                            children: Some(children),
                        }
                    }
                    ItemKind::Impl(def) => {
                        let mut children = Vec::new();

                        for assoc in &def.assoc_type_impls {
                            #[allow(deprecated)]
                            children.push(DocumentSymbol {
                                name: assoc.name.name.clone(),
                                detail: None,
                                kind: SymbolKind::TYPE_PARAMETER,
                                tags: None,
                                deprecated: None,
                                range: span_range(assoc.span),
                                selection_range: span_range(assoc.name.span),
                                children: None,
                            });
                        }

                        for impl_item in &def.items {
                            #[allow(deprecated)]
                            children.push(DocumentSymbol {
                                name: impl_item.name.name.clone(),
                                detail: None,
                                kind: SymbolKind::METHOD,
                                tags: None,
                                deprecated: None,
                                range: span_range(impl_item.span),
                                selection_range: span_range(impl_item.name.span),
                                children: None,
                            });
                        }

                        let impl_name = format_impl_name(def);
                        #[allow(deprecated)]
                        DocumentSymbol {
                            name: impl_name,
                            detail: None,
                            kind: SymbolKind::CLASS,
                            tags: None,
                            deprecated: None,
                            range: span_range(item.span),
                            selection_range: span_range(item.span),
                            children: Some(children),
                        }
                    }
                    ItemKind::Import(_) => continue,
                    ItemKind::ExprStmt(_) => continue,
                    _ => continue,
                };

                symbols.push(symbol);
            }

            return Ok(Some(DocumentSymbolResponse::Nested(symbols)));
        }

        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let pos = params.text_document_position_params.position;

        if let Some(doc) = self.documents.get(&uri)
            && let Some(ref index) = doc.symbol_index
        {
            let offset = doc.offset_at(pos.line, pos.character);

            if let Some(symbol) = index.find_definition_at(offset) {
                let start: usize = symbol.def_span.start.into();
                let end: usize = symbol.def_span.end.into();
                let (start_line, start_col) = doc.position_at(start);
                let (end_line, end_col) = doc.position_at(end);

                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: params
                        .text_document_position_params
                        .text_document
                        .uri
                        .clone(),
                    range: Range {
                        start: Position::new(start_line, start_col),
                        end: Position::new(end_line, end_col),
                    },
                })));
            }
        }

        Ok(None)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let pos = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        if let Some(doc) = self.documents.get(&uri)
            && let Some(ref index) = doc.symbol_index
        {
            let offset = doc.offset_at(pos.line, pos.character);
            let refs = index.find_references_at(offset, include_declaration);

            if !refs.is_empty() {
                let locations: Vec<Location> = refs
                    .iter()
                    .map(|r| {
                        let start: usize = r.span.start.into();
                        let end: usize = r.span.end.into();
                        let (start_line, start_col) = doc.position_at(start);
                        let (end_line, end_col) = doc.position_at(end);

                        Location {
                            uri: params.text_document_position.text_document.uri.clone(),
                            range: Range {
                                start: Position::new(start_line, start_col),
                                end: Position::new(end_line, end_col),
                            },
                        }
                    })
                    .collect();

                return Ok(Some(locations));
            }
        }

        Ok(None)
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let pos = params.text_document_position_params.position;

        if let Some(doc) = self.documents.get(&uri)
            && let Some(ref index) = doc.symbol_index
        {
            let offset = doc.offset_at(pos.line, pos.character);
            let refs = index.find_references_at(offset, true);

            if !refs.is_empty() {
                let highlights: Vec<DocumentHighlight> = refs
                    .iter()
                    .map(|r| {
                        let start: usize = r.span.start.into();
                        let end: usize = r.span.end.into();
                        let (start_line, start_col) = doc.position_at(start);
                        let (end_line, end_col) = doc.position_at(end);

                        let kind = if r.is_write {
                            DocumentHighlightKind::WRITE
                        } else {
                            DocumentHighlightKind::READ
                        };

                        DocumentHighlight {
                            range: Range {
                                start: Position::new(start_line, start_col),
                                end: Position::new(end_line, end_col),
                            },
                            kind: Some(kind),
                        }
                    })
                    .collect();

                return Ok(Some(highlights));
            }
        }

        Ok(None)
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let pos = params.text_document_position.position;
        let new_name = params.new_name;

        if let Some(doc) = self.documents.get(&uri)
            && let Some(ref index) = doc.symbol_index
        {
            let offset = doc.offset_at(pos.line, pos.character);

            let refs = index.find_references_at(offset, true);

            if !refs.is_empty() {
                let edits: Vec<TextEdit> = refs
                    .iter()
                    .map(|r| {
                        let start: usize = r.span.start.into();
                        let end: usize = r.span.end.into();
                        let (start_line, start_col) = doc.position_at(start);
                        let (end_line, end_col) = doc.position_at(end);

                        TextEdit {
                            range: Range {
                                start: Position::new(start_line, start_col),
                                end: Position::new(end_line, end_col),
                            },
                            new_text: new_name.clone(),
                        }
                    })
                    .collect();

                let mut changes = std::collections::HashMap::new();
                changes.insert(
                    params.text_document_position.text_document.uri.clone(),
                    edits,
                );

                return Ok(Some(WorkspaceEdit {
                    changes: Some(changes),
                    document_changes: None,
                    change_annotations: None,
                }));
            }
        }

        Ok(None)
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri.to_string();
        let pos = params.position;

        if let Some(doc) = self.documents.get(&uri)
            && let Some(ref index) = doc.symbol_index
        {
            let offset = doc.offset_at(pos.line, pos.character);

            // Find the symbol at this position / 在此位置查找符号
            if let Some(reference) = index.find_reference_at(offset) {
                let start: usize = reference.span.start.into();
                let end: usize = reference.span.end.into();
                let (start_line, start_col) = doc.position_at(start);
                let (end_line, end_col) = doc.position_at(end);

                return Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
                    range: Range {
                        start: Position::new(start_line, start_col),
                        end: Position::new(end_line, end_col),
                    },
                    placeholder: reference.name.clone(),
                }));
            }
        }

        Ok(None)
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = params.query.to_lowercase();
        let mut symbols = Vec::new();

        // Search across all open documents / 在所有打开的文档中搜索
        for entry in self.documents.iter() {
            let doc = entry.value();
            let uri =
                Url::parse(&doc.uri).unwrap_or_else(|_| Url::parse("file:///unknown").unwrap());

            if let Some(ref index) = doc.symbol_index {
                for (name, defs) in &index.definitions {
                    // Filter by query / 按查询过滤
                    if query.is_empty() || name.to_lowercase().contains(&query) {
                        for def in defs {
                            let start: usize = def.def_span.start.into();
                            let end: usize = def.def_span.end.into();
                            let (start_line, start_col) = doc.position_at(start);
                            let (end_line, end_col) = doc.position_at(end);

                            #[allow(deprecated)]
                            symbols.push(SymbolInformation {
                                name: name.clone(),
                                kind: convert_symbol_kind(def.kind),
                                tags: None,
                                deprecated: None,
                                location: Location {
                                    uri: uri.clone(),
                                    range: Range {
                                        start: Position::new(start_line, start_col),
                                        end: Position::new(end_line, end_col),
                                    },
                                },
                                container_name: None,
                            });
                        }
                    }
                }
            }
        }

        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(symbols))
        }
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri.to_string();
        if let Some(doc) = self.documents.get(&uri) {
            let hints = build_inlay_hints(&doc);
            return Ok(Some(hints));
        }
        Ok(None)
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri.to_string();
        if let Some(doc) = self.documents.get(&uri) {
            let ranges = build_folding_ranges(&doc);
            return Ok(Some(ranges));
        }
        Ok(None)
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri.to_string();
        if let Some(doc) = self.documents.get(&uri)
            && let Some(ref index) = doc.symbol_index
        {
            let mut lenses = Vec::new();
            for (name, syms) in &index.definitions {
                for sym in syms {
                    // Only show reference counts on functions and types
                    if !matches!(
                        sym.kind,
                        IndexSymbolKind::Function
                            | IndexSymbolKind::Struct
                            | IndexSymbolKind::Trait
                    ) {
                        continue;
                    }
                    let refs = index.get_references(name);
                    let usage_count = refs.iter().filter(|r| !r.is_write).count();

                    let start: usize = sym.def_span.start.into();
                    let end: usize = sym.def_span.end.into();
                    let (sl, sc) = doc.position_at(start);
                    let (el, ec) = doc.position_at(end);

                    let title = if usage_count == 1 {
                        "1 reference".to_string()
                    } else {
                        format!("{usage_count} references")
                    };

                    lenses.push(CodeLens {
                        range: Range {
                            start: Position::new(sl, sc),
                            end: Position::new(el, ec),
                        },
                        command: Some(Command {
                            title,
                            command: "n3v3.peekReferences".to_string(),
                            arguments: Some(vec![serde_json::json!({
                                "uri": uri,
                                "position": { "line": sl, "character": sc },
                            })]),
                        }),
                        data: None,
                    });
                }
            }
            return Ok(Some(lenses));
        }
        Ok(None)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let actions: Vec<CodeActionOrCommand> = params
            .context
            .diagnostics
            .iter()
            .filter_map(|d| {
                let source = d.source.as_deref().unwrap_or("n3v3");
                // Only offer actions for n3v3 diagnostics (parse / type / eval).
                if source.starts_with("n3v3") {
                    // Trim the message to a reasonable title length.
                    let short = if d.message.len() > 80 {
                        format!("{}…", &d.message[..77])
                    } else {
                        d.message.clone()
                    };
                    Some(CodeActionOrCommand::CodeAction(CodeAction {
                        title: format!("n3v3: {short}"),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![d.clone()]),
                        edit: None,
                        command: None,
                        is_preferred: None,
                        disabled: None,
                        data: None,
                    }))
                } else {
                    None
                }
            })
            .collect();
        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }
}

// =============================================================================
// Completion scoring helpers / 补全评分辅助
// =============================================================================

/// Extract the word prefix at the cursor position.
/// 提取光标位置处的单词前缀。
fn word_prefix_at_position(content: &str, doc: &Document, pos: Position) -> String {
    let offset = doc.offset_at(pos.line, pos.character);
    let bytes = content.as_bytes();
    let start = (0..offset)
        .rev()
        .take_while(|&i| {
            let b = bytes[i];
            b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
        })
        .last()
        .map(|i| i + 1)
        .unwrap_or(offset);
    content[start..offset].to_string()
}

/// Compute a sort text for completion relevance scoring.
/// Score: exact=1000, prefix=900+len, contains=500, default=0.
/// 计算补全相关度评分的排序文本。
fn score_sort_text(label: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        return format!("0500:{label}");
    }
    let label_lower = label.to_lowercase();
    let prefix_lower = prefix.to_lowercase();
    if label_lower == prefix_lower {
        format!("1000:{label}")
    } else if label_lower.starts_with(&prefix_lower) {
        let score = 900 + prefix.len().min(99) as u32;
        format!("{score:04}:{label}")
    } else if label_lower.contains(&prefix_lower) {
        format!("0500:{label}")
    } else {
        format!("0000:{label}")
    }
}

impl Backend {
    /// Get keyword completions.
    /// 获取关键字补全。
    fn get_keyword_completions(&self) -> Vec<CompletionItem> {
        let keywords = vec![
            ("let", "Let binding", "let ${1:name} = ${2:value};"),
            (
                "fn",
                "Function definition",
                "fn ${1:name}(${2:params}) = ${3:body};",
            ),
            (
                "if",
                "If expression",
                "if ${1:condition} -> ${2:then_branch} else ${3:else_branch}",
            ),
            (
                "match",
                "Match expression",
                "match ${1:expr} {\n\t${2:pattern} -> ${3:body},\n}",
            ),
            ("type", "Type alias", "type ${1:Name} = ${2:Type};"),
            (
                "struct",
                "Struct definition",
                "struct ${1:Name} {\n\t${2:field}: ${3:Type},\n};",
            ),
            (
                "enum",
                "Enum definition",
                "enum ${1:Name} {\n\t${2:Variant},\n};",
            ),
            (
                "trait",
                "Trait definition",
                "trait ${1:Name} {\n\t${2:items}\n};",
            ),
            (
                "impl",
                "Implementation",
                "impl ${1:Trait} for ${2:Type} {\n\t${3:items}\n};",
            ),
            ("use", "Use statement", "use ${1:module};"),
            ("true", "Boolean true", "true"),
            ("false", "Boolean false", "false"),
        ];

        keywords
            .into_iter()
            .map(|(label, detail, snippet)| CompletionItem {
                label: label.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some(detail.to_string()),
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            })
            .collect()
    }

    /// Get standard library completions.
    /// 获取标准库补全。
    fn get_stdlib_completions(&self) -> Vec<CompletionItem> {
        stdlib_completion_items()
    }

    /// Get type completions.
    /// 获取类型补全。
    fn get_type_completions(&self) -> Vec<CompletionItem> {
        let types = vec![
            ("Int", "Arbitrary precision integer"),
            ("Float", "64-bit floating point"),
            ("Bool", "Boolean"),
            ("Char", "Unicode character"),
            ("String", "UTF-8 string"),
            ("Path", "File system path"),
            ("Unit", "Unit type ()"),
            ("List", "List<T>"),
            ("Option", "Option<T> - Some or None"),
            ("Result", "Result<T, E> - Ok or Err"),
            ("Stream", "Stream<T> - Lazily evaluated stream of values"),
            ("Command", "Command value"),
            ("Pipeline", "Pipeline value"),
            ("ProcessResult", "Process exit result"),
        ];

        types
            .into_iter()
            .map(|(label, detail)| CompletionItem {
                label: label.to_string(),
                kind: Some(CompletionItemKind::TYPE_PARAMETER),
                detail: Some(detail.to_string()),
                insert_text: Some(label.to_string()),
                ..Default::default()
            })
            .collect()
    }

    /// Get method completions for dot-triggered completion, optionally filtered by receiver type.
    /// 获取点触发补全的方法补全，可按接收者类型过滤。
    fn get_method_completions(&self, receiver_type: Option<&str>) -> Vec<CompletionItem> {
        // Each entry: (label, detail, snippet, return_type, applicable_to)
        // applicable_to is a space-separated string of type names this method works on.
        // applicable_to 是该方法适用的类型名称（空格分隔）。
        let methods: Vec<(&str, &str, &str, &str, &str)> = vec![
            // List methods / 列表方法
            (
                "map",
                "Map function over elements",
                "map(${1:fn(x) x})",
                "List<U>",
                "List Option Result",
            ),
            (
                "filter",
                "Filter elements",
                "filter(${1:fn(x) true})",
                "List<T>",
                "List Option",
            ),
            (
                "fold",
                "Fold with accumulator",
                "fold(${1:init}, ${2:fn(acc, x) acc})",
                "U",
                "List",
            ),
            (
                "foldRight",
                "Right-fold",
                "foldRight(${1:init}, ${2:fn(x, acc) acc})",
                "U",
                "List",
            ),
            ("len", "Get length", "len()", "Int", "List String"),
            (
                "isEmpty",
                "Check if empty",
                "isEmpty()",
                "Bool",
                "List String",
            ),
            ("head", "Get first element", "head()", "Option<T>", "List"),
            ("first", "Get first element", "first()", "Option<T>", "List"),
            ("tail", "Get all but first", "tail()", "List<T>", "List"),
            ("last", "Get last element", "last()", "Option<T>", "List"),
            ("init", "Get all but last", "init()", "List<T>", "List"),
            (
                "get",
                "Get element at index",
                "get(${1:index})",
                "Option<T>",
                "List",
            ),
            (
                "elem",
                "Get element at index",
                "elem(${1:index})",
                "Option<T>",
                "List",
            ),
            (
                "reverse",
                "Reverse elements",
                "reverse()",
                "List<T>",
                "List",
            ),
            ("sort", "Sort elements", "sort()", "List<T>", "List"),
            ("sum", "Sum of elements", "sum()", "Number", "List"),
            (
                "product",
                "Product of elements",
                "product()",
                "Number",
                "List",
            ),
            ("max", "Maximum element", "max()", "Option<T>", "List"),
            ("min", "Minimum element", "min()", "Option<T>", "List"),
            (
                "all",
                "All match predicate",
                "all(${1:fn(x) true})",
                "Bool",
                "List",
            ),
            (
                "any",
                "Any matches predicate",
                "any(${1:fn(x) false})",
                "Bool",
                "List",
            ),
            (
                "zip",
                "Zip with another",
                "zip(${1:other})",
                "List<(T,U)>",
                "List",
            ),
            (
                "unzip",
                "Unzip into two",
                "unzip()",
                "(List<T>,List<U>)",
                "List",
            ),
            ("take", "Take first n", "take(${1:n})", "List<T>", "List"),
            ("drop", "Drop first n", "drop(${1:n})", "List<T>", "List"),
            (
                "join",
                "Join with separator",
                "join(${1:sep})",
                "String",
                "List",
            ),
            ("cons", "Prepend element", "cons(${1:x})", "List<T>", "List"),
            (
                "replicate",
                "Create by replicating",
                "replicate(${1:n}, ${2:v})",
                "List<T>",
                "List",
            ),
            (
                "contains",
                "Check if contains",
                "contains(${1:x})",
                "Bool",
                "List String",
            ),
            (
                "indexOf",
                "Find index",
                "indexOf(${1:x})",
                "Option<Int>",
                "List",
            ),
            // String methods / 字符串方法
            ("len", "Get length", "len()", "Int", "String"),
            ("isEmpty", "Check if empty", "isEmpty()", "Bool", "String"),
            (
                "split",
                "Split by separator",
                "split(${1:sep})",
                "List<String>",
                "String",
            ),
            ("trim", "Trim whitespace", "trim()", "String", "String"),
            ("upper", "To uppercase", "upper()", "String", "String"),
            ("lower", "To lowercase", "lower()", "String", "String"),
            (
                "contains",
                "Check if contains",
                "contains(${1:sub})",
                "Bool",
                "String",
            ),
            (
                "startsWith",
                "Check prefix",
                "startsWith(${1:p})",
                "Bool",
                "String",
            ),
            (
                "endsWith",
                "Check suffix",
                "endsWith(${1:s})",
                "Bool",
                "String",
            ),
            (
                "replace",
                "Replace substring",
                "replace(${1:from}, ${2:to})",
                "String",
                "String",
            ),
            (
                "substring",
                "Get substring",
                "substring(${1:s}, ${2:e})",
                "String",
                "String",
            ),
            (
                "lines",
                "Split into lines",
                "lines()",
                "List<String>",
                "String",
            ),
            ("chars", "Get characters", "chars()", "List<Char>", "String"),
            (
                "repeat",
                "Repeat n times",
                "repeat(${1:n})",
                "String",
                "String",
            ),
            ("toInt", "Parse as Int", "toInt()", "Option<Int>", "String"),
            (
                "toFloat",
                "Parse as Float",
                "toFloat()",
                "Option<Float>",
                "String",
            ),
            // Option/Result methods
            ("unwrap", "Unwrap value", "unwrap()", "T", "Option Result"),
            (
                "unwrapOr",
                "Unwrap or default",
                "unwrapOr(${1:d})",
                "T",
                "Option Result",
            ),
            ("isSome", "Check if Some", "isSome()", "Bool", "Option"),
            ("isNone", "Check if None", "isNone()", "Bool", "Option"),
            ("isOk", "Check if Ok", "isOk()", "Bool", "Result"),
            ("isErr", "Check if Err", "isErr()", "Bool", "Result"),
            (
                "map",
                "Map inner value",
                "map(${1:fn(x) x})",
                "Option<U>",
                "Option Result",
            ),
            (
                "andThen",
                "Chain operations",
                "andThen(${1:fn(x) S(x)})",
                "Option<U>",
                "Option Result",
            ),
            (
                "orElse",
                "Fallback value",
                "orElse(${1:fn() S(x)})",
                "Option<T>",
                "Option Result",
            ),
            (
                "filter",
                "Filter optional",
                "filter(${1:pred})",
                "Option<T>",
                "Option",
            ),
            // Record methods / 记录方法
            (
                "keys",
                "Get record keys",
                "keys()",
                "List<String>",
                "Record",
            ),
            (
                "values",
                "Get record values",
                "values()",
                "List<T>",
                "Record",
            ),
            (
                "hasField",
                "Check if has field",
                "hasField(${1:name})",
                "Bool",
                "Record",
            ),
        ];

        methods
            .into_iter()
            .filter(|(_, _, _, _, applies)| {
                // If no receiver type known, show all methods.
                // 如果不知道接收者类型，则显示所有方法。
                receiver_type.is_none_or(|rt| applies.split_whitespace().any(|t| t == rt))
            })
            .map(
                |(label, detail, snippet, ret_type, _applies)| CompletionItem {
                    label: label.to_string(),
                    kind: Some(CompletionItemKind::METHOD),
                    detail: Some(format!("{} -> {}", detail, ret_type)),
                    insert_text: Some(snippet.to_string()),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    ..Default::default()
                },
            )
            .collect()
    }

    /// Get completions from document symbols.
    /// 从文档符号获取补全。
    fn get_document_completions(&self, doc: &Document, _pos: Position) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        if let Some(ref index) = doc.symbol_index {
            for (name, defs) in &index.definitions {
                if let Some(def) = defs.first() {
                    let kind = match def.kind {
                        IndexSymbolKind::Function => CompletionItemKind::FUNCTION,
                        IndexSymbolKind::Variable => CompletionItemKind::VARIABLE,
                        IndexSymbolKind::Parameter => CompletionItemKind::VARIABLE,
                        IndexSymbolKind::TypeAlias => CompletionItemKind::TYPE_PARAMETER,
                        IndexSymbolKind::Struct => CompletionItemKind::STRUCT,
                        IndexSymbolKind::Enum => CompletionItemKind::ENUM,
                        IndexSymbolKind::Variant => CompletionItemKind::ENUM_MEMBER,
                        IndexSymbolKind::Trait => CompletionItemKind::INTERFACE,
                        IndexSymbolKind::Field => CompletionItemKind::FIELD,
                        IndexSymbolKind::Method => CompletionItemKind::METHOD,
                    };

                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(kind),
                        detail: Some(format!("{:?}", def.kind)),
                        ..Default::default()
                    });
                }
            }
        }

        items
    }
}

/// Helper function to convert symbol kind.
/// 转换符号类型的辅助函数。
fn convert_symbol_kind(kind: IndexSymbolKind) -> SymbolKind {
    match kind {
        IndexSymbolKind::Function => SymbolKind::FUNCTION,
        IndexSymbolKind::Variable => SymbolKind::VARIABLE,
        IndexSymbolKind::Parameter => SymbolKind::VARIABLE,
        IndexSymbolKind::TypeAlias => SymbolKind::TYPE_PARAMETER,
        IndexSymbolKind::Struct => SymbolKind::STRUCT,
        IndexSymbolKind::Enum => SymbolKind::ENUM,
        IndexSymbolKind::Variant => SymbolKind::ENUM_MEMBER,
        IndexSymbolKind::Trait => SymbolKind::INTERFACE,
        IndexSymbolKind::Field => SymbolKind::FIELD,
        IndexSymbolKind::Method => SymbolKind::METHOD,
    }
}

/// Format an impl header for symbol views.
/// 为符号视图格式化 impl 标题。
fn format_impl_name(def: &ImplDef) -> String {
    let target = format_type_name(&def.target);
    match &def.trait_ {
        Some(trait_ty) => format!("impl {} for {}", format_type_name(trait_ty), target),
        None => format!("impl {}", target),
    }
}

/// Render a type into a compact name for display.
/// 将类型渲染为紧凑的显示名称。
fn format_type_name(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Named { path, args } => {
            let name = path
                .iter()
                .map(|part| part.name.as_str())
                .collect::<Vec<_>>()
                .join(".");
            if args.is_empty() {
                name
            } else {
                let args_str = args
                    .iter()
                    .map(format_type_name)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}<{}>", name, args_str)
            }
        }
        TypeKind::Function { params, result } => {
            let params_str = params
                .iter()
                .map(format_type_name)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({}) -> {}", params_str, format_type_name(result))
        }
        TypeKind::Tuple(elems) => {
            let elems_str = elems
                .iter()
                .map(format_type_name)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({})", elems_str)
        }
        TypeKind::Record(fields) => {
            let fields_str = fields
                .iter()
                .map(|field| format!("{}: {}", field.name.name, format_type_name(&field.ty)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("#{{ {} }}", fields_str)
        }
        TypeKind::Unit => "()".to_string(),
        TypeKind::Infer => "_".to_string(),
        _ => "_".to_string(),
    }
}

// =============================================================================
// Signature help helpers
// =============================================================================

/// Find call signatures at the given offset.
fn find_call_signatures(doc: &Document, offset: usize) -> Option<Vec<SignatureInformation>> {
    let content = &doc.content;
    let (fn_name, _open_paren) = find_callee_at_offset(content, offset)?;

    if let Some(params) = lookup_fn_params(doc, &fn_name) {
        let label = if params.is_empty() {
            format!("{fn_name}()")
        } else {
            format!("{fn_name}({params})")
        };
        let parameters: Vec<ParameterInformation> = params
            .split(',')
            .map(|p| ParameterInformation {
                label: ParameterLabel::Simple(p.trim().to_string()),
                documentation: None,
            })
            .collect();
        return Some(vec![SignatureInformation {
            label,
            documentation: None,
            parameters: Some(parameters),
            active_parameter: None,
        }]);
    }

    if let Some(label) = builtin_signature(&fn_name) {
        return Some(vec![SignatureInformation {
            label,
            documentation: None,
            parameters: None,
            active_parameter: None,
        }]);
    }

    None
}

fn find_callee_at_offset(source: &str, offset: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    let offset = offset.min(bytes.len());

    let mut depth = 0u32;
    let mut paren_pos = None;
    for i in (0..offset).rev() {
        match bytes[i] {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' => {
                if depth == 0 {
                    if bytes[i] == b'(' {
                        paren_pos = Some(i);
                        break;
                    }
                } else {
                    depth -= 1;
                }
            }
            _ => {}
        }
    }
    let open_paren = paren_pos?;

    let before = &source[..open_paren];
    let name_end = before.trim_end().len();
    let before_trimmed = &before[..name_end];
    let fn_name = before_trimmed
        .rsplit(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .next()?
        .trim()
        .to_string();

    if fn_name.is_empty() {
        return None;
    }
    Some((fn_name, open_paren))
}

fn lookup_fn_params(doc: &Document, name: &str) -> Option<String> {
    let index = doc.symbol_index.as_ref()?;
    let symbols = index.definitions.get(name)?;
    let symbol = symbols
        .iter()
        .find(|s| s.kind == IndexSymbolKind::Function)?;
    let def_start: usize = symbol.full_span.start.into();
    let def_end: usize = symbol.full_span.end.into();
    let def_text = &doc.content[def_start..def_end.min(doc.content.len())];
    let open = def_text.find('(')?;
    let close = def_text[open..].find(')')?;
    let params = def_text[open + 1..open + close].trim();
    if params.is_empty() || params == ")" {
        Some(String::new())
    } else {
        Some(params.to_string())
    }
}

fn builtin_signature(name: &str) -> Option<String> {
    // 60+ builtin function signatures for signature help.
    match name {
        // IO - file operations (16)
        "io.readFile" => Some("io.readFile(path: String) -> String".to_string()),
        "io.readFilePath" => Some("io.readFilePath(path: Path) -> String".to_string()),
        "io.readFileBytesPath" => Some("io.readFileBytesPath(path: Path) -> Bytes".to_string()),
        "io.writeFile" => Some("io.writeFile(path: String, content: String) -> ()".to_string()),
        "io.writeFilePath" => {
            Some("io.writeFilePath(path: Path, content: String) -> ()".to_string())
        }
        "io.writeFileBytesPath" => {
            Some("io.writeFileBytesPath(path: Path, bytes: Bytes) -> ()".to_string())
        }
        "io.appendFile" => Some("io.appendFile(path: String, content: String) -> ()".to_string()),
        "io.appendFilePath" => {
            Some("io.appendFilePath(path: Path, content: String) -> ()".to_string())
        }
        "io.readDir" => Some("io.readDir(path: String) -> List<String>".to_string()),
        "io.createDirAll" => Some("io.createDirAll(path: String) -> ()".to_string()),
        "io.removeDirAll" => Some("io.removeDirAll(path: String) -> ()".to_string()),
        "io.pathExists" => Some("io.pathExists(path: String) -> Bool".to_string()),
        "io.isDir" => Some("io.isDir(path: String) -> Bool".to_string()),
        "io.isFile" => Some("io.isFile(path: String) -> Bool".to_string()),
        "io.hashFile" => Some("io.hashFile(path: String) -> String".to_string()),
        "io.hashString" => Some("io.hashString(s: String) -> String".to_string()),
        // IO - env / dir (4)
        "io.getEnv" => Some("io.getEnv(name: String) -> Option<String>".to_string()),
        "io.currentDir" => Some("io.currentDir() -> String".to_string()),
        "io.homeDir" => Some("io.homeDir() -> Option<String>".to_string()),
        "io.currentSystem" => Some("io.currentSystem() -> String".to_string()),
        // IO - command / process (8)
        "io.command" => {
            Some("io.command(program: String, args: List<String>) -> Command".to_string())
        }
        "io.commandWith" => Some("io.commandWith(opts: Record) -> Command".to_string()),
        "io.commandWithRedirects" => Some(
            "io.commandWithRedirects(cmd: Command, redirs: List<Redirect>) -> Command".to_string(),
        ),
        "io.execCommand" => Some("io.execCommand(cmd: Command) -> ProcessResult".to_string()),
        "io.pipeline" => Some("io.pipeline(commands: List<Command>) -> Pipeline".to_string()),
        "io.pipelineWithRedirects" => Some(
            "io.pipelineWithRedirects(p: Pipeline, redirs: List<Redirect>) -> Pipeline".to_string(),
        ),
        "io.execPipeline" => Some("io.execPipeline(p: Pipeline) -> ProcessResult".to_string()),
        "io.processSuccess" => Some("io.processSuccess(r: ProcessResult) -> Bool".to_string()),
        "io.processStdout" => Some("io.processStdout(r: ProcessResult) -> String".to_string()),
        "io.processStderr" => Some("io.processStderr(r: ProcessResult) -> String".to_string()),
        "io.processCode" => Some("io.processCode(r: ProcessResult) -> Int".to_string()),
        // IO - Stream (14)
        "io.streamList" => Some("io.streamList(xs: List<T>) -> Stream<T>".to_string()),
        "io.streamLines" => Some("io.streamLines(path: String) -> Stream<String>".to_string()),
        "io.streamCommand" => Some("io.streamCommand(cmd: Command) -> Stream<String>".to_string()),
        "io.streamBytes" => Some("io.streamBytes(path: String) -> Stream<Bytes>".to_string()),
        "io.streamMap" => {
            Some("io.streamMap(s: Stream<A>, f: fn(A) -> B) -> Stream<B>".to_string())
        }
        "io.streamFilter" => {
            Some("io.streamFilter(s: Stream<T>, pred: fn(T) -> Bool) -> Stream<T>".to_string())
        }
        "io.streamTake" => Some("io.streamTake(s: Stream<T>, n: Int) -> Stream<T>".to_string()),
        "io.streamDrop" => Some("io.streamDrop(s: Stream<T>, n: Int) -> Stream<T>".to_string()),
        "io.streamCollect" => Some("io.streamCollect(s: Stream<T>) -> List<T>".to_string()),
        "io.streamPipe" => {
            Some("io.streamPipe(s: Stream<T>, cmd: Command) -> ProcessResult".to_string())
        }
        "io.streamForEach" => {
            Some("io.streamForEach(s: Stream<T>, cb: fn(T) -> ()) -> ()".to_string())
        }
        "io.streamFold" => {
            Some("io.streamFold(s: Stream<T>, init: A, f: fn(A, T) -> A) -> A".to_string())
        }
        "io.streamWithTimeout" => {
            Some("io.streamWithTimeout(s: Stream<T>, ms: Int) -> Stream<Option<T>>".to_string())
        }
        // IO - Task (7)
        "io.taskCommand" => Some("io.taskCommand(cmd: Command) -> Task<ProcessResult>".to_string()),
        "io.taskPipeline" => {
            Some("io.taskPipeline(p: Pipeline) -> Task<ProcessResult>".to_string())
        }
        "io.awaitTask" => Some("io.awaitTask(task: Task<T>) -> T".to_string()),
        "io.awaitTasks" => Some("io.awaitTasks(tasks: List<Task<T>>) -> List<T>".to_string()),
        "io.awaitAny" => Some("io.awaitAny(tasks: List<Task<T>>) -> T".to_string()),
        "io.awaitTaskWithTimeout" => {
            Some("io.awaitTaskWithTimeout(task: Task<T>, ms: Int) -> Option<T>".to_string())
        }
        "io.cancel" => Some("io.cancel(task: Task<T>) -> ()".to_string()),
        "io.liveNext" => Some("io.liveNext(live: Live<T>) -> T".to_string()),
        "io.liveCurrent" => Some("io.liveCurrent(live: Live<T>) -> Option<T>".to_string()),
        "io.liveCancel" => Some("io.liveCancel(live: Live<T>) -> ()".to_string()),
        // IO - TTY / Job / Misc (8)
        "io.isTTY" => Some("io.isTTY(fd: Int) -> Bool".to_string()),
        "io.terminalSize" => Some("io.terminalSize() -> Option<(Int, Int)>".to_string()),
        "io.setRawMode" => Some("io.setRawMode(fd: Int, enable: Bool) -> ()".to_string()),
        "io.resetTerminal" => Some("io.resetTerminal(fd: Int) -> ()".to_string()),
        "io.readKey" => Some("io.readKey(fd: Int) -> Int".to_string()),
        "io.jobs" => Some("io.jobs() -> List<{id: Int, state: String}>".to_string()),
        "io.waitAnyJob" => Some("io.waitAnyJob() -> {id: Int, result: ProcessResult}".to_string()),
        "io.kill" => Some("io.kill(pid: Int, signal: Int) -> ()".to_string()),
        "io.args" => Some("io.args() -> List<String>".to_string()),
        // List functions (12)
        "list.map" => Some("list.map(xs: List<A>, f: fn(A) -> B) -> List<B>".to_string()),
        "list.filter" => {
            Some("list.filter(xs: List<T>, pred: fn(T) -> Bool) -> List<T>".to_string())
        }
        "list.fold" => Some("list.fold(xs: List<T>, init: A, f: fn(A, T) -> A) -> A".to_string()),
        "list.foldRight" => {
            Some("list.foldRight(xs: List<T>, init: A, f: fn(T, A) -> A) -> A".to_string())
        }
        "list.zip" => Some("list.zip(xs: List<A>, ys: List<B>) -> List<(A, B)>".to_string()),
        "list.take" => Some("list.take(xs: List<T>, n: Int) -> List<T>".to_string()),
        "list.drop" => Some("list.drop(xs: List<T>, n: Int) -> List<T>".to_string()),
        "list.head" => Some("list.head(xs: List<T>) -> Option<T>".to_string()),
        "list.tail" => Some("list.tail(xs: List<T>) -> List<T>".to_string()),
        "list.reverse" => Some("list.reverse(xs: List<T>) -> List<T>".to_string()),
        "list.sort" => Some("list.sort(xs: List<T>) -> List<T>".to_string()),
        "list.sum" => Some("list.sum(xs: List<Number>) -> Number".to_string()),
        // String functions (8)
        "string.split" => Some("string.split(s: String, sep: String) -> List<String>".to_string()),
        "string.join" => Some("string.join(xs: List<String>, sep: String) -> String".to_string()),
        "string.trim" => Some("string.trim(s: String) -> String".to_string()),
        "string.upper" => Some("string.upper(s: String) -> String".to_string()),
        "string.lower" => Some("string.lower(s: String) -> String".to_string()),
        "string.contains" => Some("string.contains(s: String, sub: String) -> Bool".to_string()),
        "string.replace" => {
            Some("string.replace(s: String, from: String, to: String) -> String".to_string())
        }
        "string.len" => Some("string.len(s: String) -> Int".to_string()),
        _ => None,
    }
}
fn count_commas_before(source: &str, offset: usize) -> u32 {
    let bytes = source.as_bytes();
    let offset = offset.min(bytes.len());
    let mut depth = 0u32;
    let mut paren_start = 0usize;
    for i in (0..offset).rev() {
        match bytes[i] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    paren_start = i + 1;
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    let slice = &source[paren_start..offset];
    slice.bytes().filter(|&b| b == b',').count() as u32
}

// =============================================================================
// Completion documentation / 补全文档
// =============================================================================

/// Get documentation for a completion item.
fn completion_documentation(label: &str, _kind: Option<CompletionItemKind>) -> Option<String> {
    match label {
        // IO - file
        "io.readFile" => {
            Some("Reads entire file contents as a String.\n\n**Effect:** I/O".to_string())
        }
        "io.readFilePath" => Some("Reads file from a typed Path.\n\n**Effect:** I/O".to_string()),
        "io.readFileBytesPath" => {
            Some("Reads raw bytes from a typed Path.\n\n**Effect:** I/O".to_string())
        }
        "io.writeFile" => {
            Some("Writes content to a file. Creates if missing.\n\n**Effect:** I/O".to_string())
        }
        "io.writeFilePath" => {
            Some("Writes content to a typed Path.\n\n**Effect:** I/O".to_string())
        }
        "io.appendFile" => Some("Appends content to a file.\n\n**Effect:** I/O".to_string()),
        "io.readDir" => Some("Lists directory contents.\n\n**Effect:** I/O".to_string()),
        "io.createDirAll" => {
            Some("Creates directories recursively.\n\n**Effect:** I/O".to_string())
        }
        "io.removeDirAll" => {
            Some("Removes directories recursively.\n\n**Effect:** I/O".to_string())
        }
        "io.pathExists" => {
            Some("Returns `true` if path exists on filesystem.\n\n**Effect:** I/O".to_string())
        }
        "io.isDir" => Some("Returns `true` if path is a directory.\n\n**Effect:** I/O".to_string()),
        "io.isFile" => Some("Returns `true` if path is a file.\n\n**Effect:** I/O".to_string()),
        "io.hashFile" => {
            Some("Returns SHA-256 hash of file contents.\n\n**Effect:** I/O".to_string())
        }
        "io.hashString" => {
            Some("Returns SHA-256 hash of a string.\n\n**Effect:** Pure".to_string())
        }
        // IO - env
        "io.getEnv" => {
            Some("Gets environment variable value, or `None`.\n\n**Effect:** I/O".to_string())
        }
        "io.currentDir" => {
            Some("Returns current working directory.\n\n**Effect:** I/O".to_string())
        }
        "io.homeDir" => Some("Returns home directory, or `None`.\n\n**Effect:** I/O".to_string()),
        // IO - command
        "io.command" => Some(
            "Constructs a `Command` from program + args. Pure constructor.\n\n**Effect:** Pure"
                .to_string(),
        ),
        "io.commandWith" => {
            Some("Constructs a `Command` with options record.\n\n**Effect:** Pure".to_string())
        }
        "io.execCommand" => {
            Some("Executes a command, returns `ProcessResult`.\n\n**Effect:** Process".to_string())
        }
        "io.pipeline" => Some(
            "Constructs a `Pipeline` from commands. Pure constructor.\n\n**Effect:** Pure"
                .to_string(),
        ),
        "io.execPipeline" => {
            Some("Executes a pipeline, returns `ProcessResult`.\n\n**Effect:** Process".to_string())
        }
        "io.processSuccess" => {
            Some("Returns `true` if process exited with code 0.\n\n**Effect:** Pure".to_string())
        }
        "io.processStdout" => {
            Some("Returns stdout of a `ProcessResult`.\n\n**Effect:** Pure".to_string())
        }
        "io.processStderr" => {
            Some("Returns stderr of a `ProcessResult`.\n\n**Effect:** Pure".to_string())
        }
        "io.processCode" => {
            Some("Returns exit code of a `ProcessResult`.\n\n**Effect:** Pure".to_string())
        }
        // IO - Stream
        "io.streamList" => {
            Some("Creates a `Stream<T>` from a `List<T>`.\n\n**Effect:** Stream".to_string())
        }
        "io.streamLines" => Some(
            "Streams lines from a file as `Stream<String>`.\n\n**Effect:** Stream + File I/O"
                .to_string(),
        ),
        "io.streamCommand" => {
            Some("Streams stdout lines from a command.\n\n**Effect:** Stream + Process".to_string())
        }
        "io.streamBytes" => Some(
            "Streams byte chunks from a file as `Stream<Bytes>`.\n\n**Effect:** Stream + File I/O"
                .to_string(),
        ),
        "io.streamMap" => Some(
            "Transforms each element of a stream via closure.\n\n**Effect:** Stream".to_string(),
        ),
        "io.streamFilter" => {
            Some("Filters stream elements by predicate.\n\n**Effect:** Stream".to_string())
        }
        "io.streamTake" => {
            Some("Takes first N elements from a stream.\n\n**Effect:** Stream".to_string())
        }
        "io.streamDrop" => {
            Some("Drops first N elements from a stream.\n\n**Effect:** Stream".to_string())
        }
        "io.streamCollect" => {
            Some("Collects all stream elements into a `List<T>`.\n\n**Effect:** Stream".to_string())
        }
        "io.streamPipe" => {
            Some("Pipes stream contents to a command.\n\n**Effect:** Stream + Process".to_string())
        }
        "io.streamForEach" => {
            Some("Runs callback for each stream element.\n\n**Effect:** Stream".to_string())
        }
        "io.streamFold" => {
            Some("Folds a stream with accumulator function.\n\n**Effect:** Stream".to_string())
        }
        "io.streamWithTimeout" => Some(
            "Adds timeout to stream (wraps in `Option`).\n\n**Effect:** Stream + Time".to_string(),
        ),
        // IO - Task
        "io.taskCommand" => Some(
            "Creates a `Task` from a command for async execution.\n\n**Effect:** Task".to_string(),
        ),
        "io.awaitTask" => {
            Some("Awaits a single task result (blocking).\n\n**Effect:** Task".to_string())
        }
        "io.awaitTasks" => {
            Some("Awaits multiple tasks, returns list of results.\n\n**Effect:** Task".to_string())
        }
        "io.awaitAny" => Some("Awaits first task to complete.\n\n**Effect:** Task".to_string()),
        "io.awaitTaskWithTimeout" => {
            Some("Awaits task with timeout, returns `Option<T>`.\n\n**Effect:** Task".to_string())
        }
        "io.cancel" => Some("Cancels a running task.\n\n**Effect:** Task".to_string()),
        // IO - TTY / Job
        "io.isTTY" => Some("Returns `true` if fd is a terminal.\n\n**Effect:** Device".to_string()),
        "io.terminalSize" => Some(
            "Returns terminal dimensions as `Option<(rows, cols)>`.\n\n**Effect:** Device"
                .to_string(),
        ),
        "io.setRawMode" => {
            Some("Sets terminal to raw mode (no line buffering).\n\n**Effect:** I/O".to_string())
        }
        "io.resetTerminal" => {
            Some("Resets terminal to normal mode.\n\n**Effect:** I/O".to_string())
        }
        "io.readKey" => {
            Some("Reads a single byte from fd (requires raw mode).\n\n**Effect:** I/O".to_string())
        }
        "io.liveNext" => Some("Gets the next reactive value.\n\n**Effect:** Host".to_string()),
        "io.liveCurrent" => Some(
            "Reads the current reactive value without waiting.\n\n**Effect:** Host".to_string(),
        ),
        "io.liveCancel" => Some("Stops a reactive value.\n\n**Effect:** Host".to_string()),
        "io.jobs" => Some("Lists running background jobs.\n\n**Effect:** Task".to_string()),
        "io.waitAnyJob" => {
            Some("Waits for any background job to complete.\n\n**Effect:** Time".to_string())
        }
        "io.kill" => Some("Kills a process by PID with signal.\n\n**Effect:** Process".to_string()),
        "io.args" => {
            Some("Returns command-line arguments passed to script.\n\n**Effect:** Pure".to_string())
        }
        // List
        "list.map" => {
            Some("Applies function to each element. O(n).\n\n**Effect:** Pure".to_string())
        }
        "list.filter" => {
            Some("Returns elements satisfying predicate. O(n).\n\n**Effect:** Pure".to_string())
        }
        "list.fold" => Some(
            "Reduces list from left with binary function. O(n).\n\n**Effect:** Pure".to_string(),
        ),
        "list.foldRight" => Some(
            "Reduces list from right with binary function. O(n).\n\n**Effect:** Pure".to_string(),
        ),
        "list.zip" => {
            Some("Combines two lists into pairs. O(min(n,m)).\n\n**Effect:** Pure".to_string())
        }
        "list.take" => Some("Returns first N elements. O(n).\n\n**Effect:** Pure".to_string()),
        "list.drop" => Some("Drops first N elements. O(n).\n\n**Effect:** Pure".to_string()),
        "list.head" => {
            Some("Returns first element, or `None`. O(1).\n\n**Effect:** Pure".to_string())
        }
        "list.tail" => Some("Returns all but first element. O(1).\n\n**Effect:** Pure".to_string()),
        "list.reverse" => Some("Reverses element order. O(n).\n\n**Effect:** Pure".to_string()),
        "list.sort" => Some("Sorts elements (stable). O(n log n).\n\n**Effect:** Pure".to_string()),
        "list.sum" => Some("Sums numeric elements. O(n).\n\n**Effect:** Pure".to_string()),
        // String
        "string.split" => {
            Some("Splits string by separator into list. O(n).\n\n**Effect:** Pure".to_string())
        }
        "string.join" => {
            Some("Joins list of strings with separator. O(n).\n\n**Effect:** Pure".to_string())
        }
        "string.trim" => {
            Some("Removes leading/trailing whitespace. O(n).\n\n**Effect:** Pure".to_string())
        }
        "string.upper" => Some("Converts to uppercase. O(n).\n\n**Effect:** Pure".to_string()),
        "string.lower" => Some("Converts to lowercase. O(n).\n\n**Effect:** Pure".to_string()),
        "string.contains" => {
            Some("Returns `true` if contains substring. O(n).\n\n**Effect:** Pure".to_string())
        }
        "string.replace" => {
            Some("Replaces all occurrences of substring. O(n).\n\n**Effect:** Pure".to_string())
        }
        "string.len" => Some("Returns string length. O(1).\n\n**Effect:** Pure".to_string()),
        "string.chars" => Some("Returns list of characters. O(n).\n\n**Effect:** Pure".to_string()),
        "string.lines" => Some("Splits into lines. O(n).\n\n**Effect:** Pure".to_string()),
        _ => None,
    }
}

// =============================================================================
// Type-aware completion helpers
// =============================================================================

/// Find the type name of the receiver expression before the dot at `offset`.
///
/// Walks the parsed AST to find the expression whose span ends at or near
/// the dot position, -> looks up its type in the semantics table.
fn find_receiver_type_name(doc: &Document, dot_offset: usize) -> Option<String> {
    let ast = doc.ast.as_ref()?;
    let semantics = doc.semantics.as_ref()?;

    // Walk the AST to find the expression that ends right before the dot.
    // 遍历 AST 找到在点之前结束的表达式。
    let receiver_span = find_receiver_expr_span(ast, dot_offset)?;

    // Look up the expression's type.
    // 查找表达式的类型。
    let ty = semantics.expr_type(receiver_span)?;

    // Convert Ty to a user-friendly type name.
    // 将 Ty 转换为用户友好的类型名称。
    type_to_name(ty, semantics)
}

/// Walk the AST to find the expression whose span ends just before the dot offset.
fn find_receiver_expr_span(ast: &n3v3_syntax::SourceFile, dot_offset: usize) -> Option<Span> {
    for item in &ast.items {
        if let Some(span) = find_expr_ending_at(item, dot_offset) {
            return Some(span);
        }
    }
    None
}

fn find_expr_ending_at(item: &n3v3_syntax::Item, dot_offset: usize) -> Option<Span> {
    use n3v3_syntax::ItemKind;

    match &item.kind {
        ItemKind::Let(def) => expr_ending_at(&def.value, dot_offset),
        ItemKind::Fn(def) => expr_ending_at(&def.body, dot_offset),
        ItemKind::ExprStmt(expr) => expr_ending_at(expr, dot_offset),
        _ => None,
    }
}

fn expr_ending_at(expr: &n3v3_syntax::Expr, dot_offset: usize) -> Option<Span> {
    use n3v3_syntax::ExprKind;

    let expr_end: usize = expr.span.end.into();
    let expr_start: usize = expr.span.start.into();

    // If the dot position is within this expression or just after it (before the dot),
    // this could be the receiver.
    // 如果点位置在这个表达式内或紧接其后（在点之前），这可能是接收者。
    if expr_start < dot_offset && dot_offset <= expr_end + 1 {
        // Check children first (since we want the innermost expression).
        // 先检查子表达式（因为我们想要最内层的表达式）。
        match &expr.kind {
            ExprKind::Call { func, args } => {
                if let Some(s) = expr_ending_at(func, dot_offset) {
                    return Some(s);
                }
                for arg in args {
                    if let Some(s) = expr_ending_at(arg, dot_offset) {
                        return Some(s);
                    }
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                if let Some(s) = expr_ending_at(receiver, dot_offset) {
                    return Some(s);
                }
                for arg in args {
                    if let Some(s) = expr_ending_at(arg, dot_offset) {
                        return Some(s);
                    }
                }
            }
            ExprKind::Field { base, .. } => {
                if let Some(s) = expr_ending_at(base, dot_offset) {
                    return Some(s);
                }
            }
            ExprKind::Binary { left, right, .. } => {
                if let Some(s) = expr_ending_at(left, dot_offset) {
                    return Some(s);
                }
                if let Some(s) = expr_ending_at(right, dot_offset) {
                    return Some(s);
                }
            }
            ExprKind::Block { stmts, expr: tail } => {
                for stmt in stmts {
                    if let n3v3_syntax::StmtKind::Let { value, .. } = &stmt.kind
                        && let Some(s) = expr_ending_at(value, dot_offset)
                    {
                        return Some(s);
                    }
                }
                if let Some(tail_expr) = tail
                    && let Some(s) = expr_ending_at(tail_expr, dot_offset)
                {
                    return Some(s);
                }
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if let Some(s) = expr_ending_at(condition, dot_offset) {
                    return Some(s);
                }
                if let Some(s) = expr_ending_at(then_branch, dot_offset) {
                    return Some(s);
                }
                if let Some(s) = expr_ending_at(else_branch, dot_offset) {
                    return Some(s);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                if let Some(s) = expr_ending_at(scrutinee, dot_offset) {
                    return Some(s);
                }
                for arm in arms {
                    if let Some(s) = expr_ending_at(&arm.body, dot_offset) {
                        return Some(s);
                    }
                }
            }
            ExprKind::Let { value, body, .. } => {
                if let Some(s) = expr_ending_at(value, dot_offset) {
                    return Some(s);
                }
                if let Some(s) = expr_ending_at(body, dot_offset) {
                    return Some(s);
                }
            }
            ExprKind::Var(_)
            | ExprKind::List(_)
            | ExprKind::Record(_)
            | ExprKind::Lambda { .. }
            | ExprKind::String(_)
            | ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Char(_)
            | ExprKind::PathLit(_) => {
                // Leaf expressions: return their span as the receiver.
                // 叶子表达式：返回它们的 span 作为接收者。
            }
            _ => {}
        }

        // Return this expression's span if the dot is after it.
        // 如果点在它之后，返回该表达式的 span。
        if dot_offset == expr_end || dot_offset == expr_end + 1 {
            return Some(expr.span);
        }
    }

    None
}

/// Convert a Ty to a human-readable type name for method filtering.
fn type_to_name(ty: &n3v3_hir::Ty, semantics: &n3v3_frontend::ModuleSemantics) -> Option<String> {
    use n3v3_hir::TyKind;

    match &ty.kind {
        TyKind::String => Some("String".to_string()),
        TyKind::Int => Some("Int".to_string()),
        TyKind::Float => Some("Float".to_string()),
        TyKind::Bool => Some("Bool".to_string()),
        TyKind::Char => Some("Char".to_string()),
        TyKind::Unit => Some("Unit".to_string()),
        TyKind::Named(def_id, _args) => n3v3_typeck::builtin_type_name(*def_id)
            .map(str::to_string)
            .or_else(|| def_id_to_name(*def_id, semantics)),
        TyKind::Record(_) | TyKind::DynamicRecord(_) | TyKind::SafeRecordBase(_) => {
            Some("Record".to_string())
        }
        TyKind::Fn(..) => Some("Fn".to_string()),
        TyKind::Tuple(_) => Some("Tuple".to_string()),
        _ => None,
    }
}

/// Resolve a DefId to a human-readable name using the module semantics.
fn def_id_to_name(
    def_id: n3v3_hir::DefId,
    semantics: &n3v3_frontend::ModuleSemantics,
) -> Option<String> {
    semantics.global_names.get(&def_id).cloned()
}

// =============================================================================
// Inlay hints / 内联提示
// =============================================================================

/// Build inlay hints showing inferred types for let bindings and functions.
fn build_inlay_hints(doc: &Document) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    let ast = match doc.ast.as_ref() {
        Some(ast) => ast,
        None => return hints,
    };
    let semantics = match doc.semantics.as_ref() {
        Some(s) => s,
        None => return hints,
    };

    use n3v3_syntax::ItemKind;

    for item in &ast.items {
        match &item.kind {
            ItemKind::Let(def) => {
                // Show type hint for let bindings: `let x: <Type> = ...`
                // 为 let 绑定显示类型提示：`let x: <Type> = ...`
                if let Some(ty) = semantics.expr_type(def.value.span) {
                    let type_str = format_type_hint(ty, Some(&semantics.global_names));
                    if !type_str.is_empty() && type_str != "()" {
                        let end: usize = def.value.span.start.into();
                        let (line, col) = doc.position_at(end);
                        hints.push(InlayHint {
                            position: Position::new(line, col),
                            label: InlayHintLabel::String(format!(": {}", type_str)),
                            kind: Some(InlayHintKind::TYPE),
                            text_edits: None,
                            tooltip: None,
                            padding_left: Some(true),
                            padding_right: None,
                            data: None,
                        });
                    }
                }
            }
            ItemKind::Fn(def) => {
                // Show return type hint: `fn name(...): <ReturnType>`
                // 显示返回类型提示：`fn name(...): <ReturnType>`
                if let Some(ty) = semantics.expr_type(def.body.span) {
                    let type_str = format_type_hint(ty, Some(&semantics.global_names));
                    if !type_str.is_empty() && type_str != "()" {
                        // Place hint at end of the parameter list
                        // 将提示放在参数列表的末尾
                        let hint_pos = if let Some(last_param) = def.params.last() {
                            let end: usize = last_param.span.end.into();
                            let (line, col) = doc.position_at(end + 1);
                            Position::new(line, col)
                        } else {
                            let end: usize = def.name.span.end.into();
                            let (line, col) = doc.position_at(end + 2); // after "fn name"
                            Position::new(line, col)
                        };
                        hints.push(InlayHint {
                            position: hint_pos,
                            label: InlayHintLabel::String(format!("-> {}", type_str)),
                            kind: Some(InlayHintKind::TYPE),
                            text_edits: None,
                            tooltip: None,
                            padding_left: Some(true),
                            padding_right: None,
                            data: None,
                        });
                    }
                }
            }
            ItemKind::ExprStmt(expr) => {
                collect_expr_inlay_hints(expr, doc, semantics, &mut hints);
            }
            _ => {}
        }
    }

    hints
}

fn collect_expr_inlay_hints(
    expr: &n3v3_syntax::Expr,
    doc: &Document,
    semantics: &n3v3_frontend::ModuleSemantics,
    hints: &mut Vec<InlayHint>,
) {
    use n3v3_syntax::{ExprKind, StmtKind};

    match &expr.kind {
        ExprKind::Let { value, body, .. } => {
            if let Some(ty) = semantics.expr_type(value.span) {
                let type_str = format_type_hint(ty, Some(&semantics.global_names));
                if !type_str.is_empty() && type_str != "()" {
                    let end: usize = value.span.start.into();
                    let (line, col) = doc.position_at(end);
                    hints.push(InlayHint {
                        position: Position::new(line, col),
                        label: InlayHintLabel::String(format!(": {}", type_str)),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: None,
                        padding_left: Some(true),
                        padding_right: None,
                        data: None,
                    });
                }
            }
            collect_expr_inlay_hints(value, doc, semantics, hints);
            collect_expr_inlay_hints(body, doc, semantics, hints);
        }
        ExprKind::Block { stmts, expr: tail } => {
            for stmt in stmts {
                if let StmtKind::Let { value, .. } = &stmt.kind {
                    collect_expr_inlay_hints(value, doc, semantics, hints);
                }
            }
            if let Some(tail) = tail {
                collect_expr_inlay_hints(tail, doc, semantics, hints);
            }
        }
        _ => {}
    }
}

/// Format a type for inlay hint display.
fn format_type_hint(
    ty: &n3v3_hir::Ty,
    names: Option<&std::collections::HashMap<n3v3_hir::DefId, String>>,
) -> String {
    use n3v3_hir::TyKind;
    match &ty.kind {
        TyKind::String => "String".to_string(),
        TyKind::Int => "Int".to_string(),
        TyKind::Float => "Float".to_string(),
        TyKind::Bool => "Bool".to_string(),
        TyKind::Char => "Char".to_string(),
        TyKind::Unit => "()".to_string(),
        TyKind::Named(_, args) => {
            if args.is_empty() {
                def_id_to_name_hint(ty, names)
            } else {
                let args_str: Vec<String> =
                    args.iter().map(|t| format_type_hint(t, names)).collect();
                format!(
                    "{}<{}>",
                    def_id_to_name_hint(ty, names),
                    args_str.join(", ")
                )
            }
        }
        TyKind::Fn(params, ret) => {
            let params_str: Vec<String> =
                params.iter().map(|t| format_type_hint(t, names)).collect();
            format!(
                "fn({}) -> {}",
                params_str.join(", "),
                format_type_hint(ret, names)
            )
        }
        TyKind::Tuple(types) => {
            let types_str: Vec<String> = types.iter().map(|t| format_type_hint(t, names)).collect();
            format!("({})", types_str.join(", "))
        }
        TyKind::Record(fields) | TyKind::DynamicRecord(fields) => {
            let fields_str: Vec<String> = fields
                .iter()
                .map(|(name, ty)| format!("{}: {}", name, format_type_hint(ty, names)))
                .collect();
            format!("#{{ {} }}", fields_str.join(", "))
        }
        _ => String::new(),
    }
}

fn def_id_to_name_hint(
    ty: &n3v3_hir::Ty,
    names: Option<&std::collections::HashMap<n3v3_hir::DefId, String>>,
) -> String {
    use n3v3_hir::TyKind;
    // Resolve the DefId from a Named type to its canonical name.
    if let TyKind::Named(def_id, _) = &ty.kind
        && let Some(names) = names
        && let Some(name) = names.get(def_id)
    {
        return name.clone();
    }
    // Fall back to "T" when the names map is unavailable or the DefId is unknown.
    "T".to_string()
}

// =============================================================================
// Import path completion / 导入路径补全
// =============================================================================

/// Get import path completions if the cursor is in an import statement.
fn get_import_completions(doc: &Document, pos: Position) -> Option<Vec<CompletionItem>> {
    // Check if we're in an import statement context.
    // 检查是否在导入语句上下文中。
    let line_offset = doc.offset_at(pos.line, 0);
    let line_text = &doc.content[line_offset..doc.content.len().min(line_offset + 200)];
    let trimmed = line_text.trim();

    if !trimmed.starts_with("use ") {
        return None;
    }

    // Extract the current import path being typed.
    // 提取当前正在输入的导入路径。
    let after_use = &trimmed[7..]; // after "use "
    let prefix = after_use.trim_end_matches(';');

    // Find .n3v3 files in the workspace relative to the current file.
    // 查找工作区中相对于当前文件的 .n3v3 文件。
    let current_dir = std::path::Path::new(&doc.uri)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let mut items = Vec::new();
    // Collect entries so we can iterate twice.
    // 收集条目以便可以迭代两次。
    let all_entries: Vec<_> = match std::fs::read_dir(&current_dir) {
        Ok(entries) => entries.flatten().collect(),
        Err(_) => return None,
    };

    for entry in &all_entries {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // Match .n3v3 files, excluding current file.
        // 匹配 .n3v3 文件，排除当前文件。
        if name.ends_with(".n3v3") && !doc.uri.ends_with(name) {
            let module_name = name.trim_end_matches(".n3v3");
            if prefix.is_empty() || module_name.starts_with(prefix) {
                items.push(CompletionItem {
                    label: module_name.to_string(),
                    kind: Some(CompletionItemKind::MODULE),
                    detail: Some(format!("Import module ./{}", name)),
                    insert_text: Some(module_name.to_string()),
                    ..Default::default()
                });
            }
        }
    }
    // Also suggest subdirectories as module prefixes.
    // 同时建议子目录作为模块前缀。
    for entry in &all_entries {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') && (prefix.is_empty() || name.starts_with(prefix)) {
                items.push(CompletionItem {
                    label: format!("{}/", name),
                    kind: Some(CompletionItemKind::MODULE),
                    detail: Some(format!("Module directory ./{}", name)),
                    insert_text: Some(format!("{}/", name)),
                    ..Default::default()
                });
            }
        }
    }

    if items.is_empty() { None } else { Some(items) }
}

// =============================================================================
// Enhanced hover documentation / 增强悬停文档
// =============================================================================

/// Get documentation text for a builtin symbol name.
pub(crate) fn builtin_hover_docs(name: &str) -> Option<&'static str> {
    match name {
        "io.readFile" => Some(
            "Reads the entire contents of a file as a String.\n\n**Signature:** `io.readFile(path: String) -> String`\n**Effect:** Yes (I/O)",
        ),
        "io.readFilePath" => Some(
            "Reads file contents from a typed Path.\n\n**Signature:** `io.readFilePath(path: Path) -> String`\n**Effect:** Yes (I/O)",
        ),
        "io.writeFile" => Some(
            "Writes content to a file, creating it if necessary.\n\n**Signature:** `io.writeFile(path: String, content: String) -> ()`\n**Effect:** Yes (I/O)",
        ),
        "io.writeFilePath" => Some(
            "Writes content to a typed Path.\n\n**Signature:** `io.writeFilePath(path: Path, content: String) -> ()`\n**Effect:** Yes (I/O)",
        ),
        "io.pathExists" => Some(
            "Returns `true` if the path exists on the filesystem.\n\n**Signature:** `io.pathExists(path: String) -> Bool`\n**Effect:** Yes (I/O)",
        ),
        "io.getEnv" => Some(
            "Returns the value of an environment variable, or `None`.\n\n**Signature:** `io.getEnv(name: String) -> Option<String>`\n**Effect:** Yes (I/O)",
        ),
        "io.command" => Some(
            "Constructs a `Command` value from a program and arguments.\n\n**Signature:** `io.command(program: String, args: List<String>) -> Command`\n**Effect:** No (pure constructor)",
        ),
        "io.execCommand" => Some(
            "Executes a command and returns `ProcessResult`.\n\n**Signature:** `io.execCommand(cmd: Command) -> ProcessResult`\n**Effect:** Yes (process)",
        ),
        "io.pipeline" => Some(
            "Constructs a `Pipeline` from a list of commands (pipe chain).\n\n**Signature:** `io.pipeline(cmds: List<Command>) -> Pipeline`\n**Effect:** No (pure constructor)",
        ),
        "io.execPipeline" => Some(
            "Executes a pipeline and returns `ProcessResult`.\n\n**Signature:** `io.execPipeline(p: Pipeline) -> ProcessResult`\n**Effect:** Yes (process)",
        ),
        "io.streamList" => Some(
            "Creates a Stream from a List for lazy processing.\n\n**Signature:** `io.streamList(xs: List<T>) -> Stream<T>`\n**Effect:** Yes (stream)",
        ),
        "io.streamMap" => Some(
            "Transforms each element of a Stream using a closure.\n\n**Signature:** `io.streamMap(s: Stream<A>, f: fn(A) -> B) -> Stream<B>`\n**Effect:** Yes (stream)",
        ),
        "io.streamFilter" => Some(
            "Filters elements from a Stream using a predicate.\n\n**Signature:** `io.streamFilter(s: Stream<T>, pred: fn(T) -> Bool) -> Stream<T>`\n**Effect:** Yes (stream)",
        ),
        "io.streamCollect" => Some(
            "Collects all elements from a Stream into a List.\n\n**Signature:** `io.streamCollect(s: Stream<T>) -> List<T>`\n**Effect:** Yes (stream)",
        ),
        "io.streamFold" => Some(
            "Folds a Stream with an accumulator function.\n\n**Signature:** `io.streamFold(s: Stream<T>, init: A, f: fn(A, T) -> A) -> A`\n**Effect:** Yes (stream)",
        ),
        "io.cancel" => Some(
            "Cancels a running task.\n\n**Signature:** `io.cancel(task: Task<T>) -> ()`\n**Effect:** Yes (task)",
        ),
        "io.args" => Some(
            "Returns the list of command-line arguments.\n\n**Signature:** `io.args() -> List<String>`\n**Effect:** No (pure)",
        ),
        "list.map" => Some(
            "Applies a function to each element, returning a new list.\n\n**Signature:** `list.map(xs: List<A>, f: fn(A) -> B) -> List<B>`\n**Complexity:** O(n)",
        ),
        "list.filter" => Some(
            "Returns elements that satisfy the predicate.\n\n**Signature:** `list.filter(xs: List<T>, pred: fn(T) -> Bool) -> List<T>`\n**Complexity:** O(n)",
        ),
        "list.fold" => Some(
            "Reduces a list from the left with a binary function.\n\n**Signature:** `list.fold(xs: List<T>, init: A, f: fn(A, T) -> A) -> A`\n**Complexity:** O(n)",
        ),
        "string.split" => Some(
            "Splits a string by separator into a list of substrings.\n\n**Signature:** `string.split(s: String, sep: String) -> List<String>`\n**Complexity:** O(n)",
        ),
        "string.join" => Some(
            "Joins a list of strings with a separator.\n\n**Signature:** `string.join(xs: List<String>, sep: String) -> String`\n**Complexity:** O(n)",
        ),
        _ => None,
    }
}

// =============================================================================
// Folding ranges / 折叠区域
// =============================================================================

/// Build folding ranges from the AST.
fn build_folding_ranges(doc: &Document) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    let ast = match doc.ast.as_ref() {
        Some(ast) => ast,
        None => return ranges,
    };

    use n3v3_syntax::ItemKind;

    for item in &ast.items {
        match &item.kind {
            ItemKind::Fn(def) => {
                let (sl, _) = doc.position_at(def.body.span.start.into());
                let (el, _) = doc.position_at(def.body.span.end.into());
                if el > sl {
                    ranges.push(FoldingRange {
                        start_line: sl,
                        start_character: None,
                        end_line: el,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Region),
                        collapsed_text: None,
                    });
                }
            }
            ItemKind::Struct(_) | ItemKind::Enum(_) | ItemKind::Trait(_) | ItemKind::Impl(_) => {
                let (sl, _) = doc.position_at(item.span.start.into());
                let (el, _) = doc.position_at(item.span.end.into());
                if el > sl {
                    ranges.push(FoldingRange {
                        start_line: sl,
                        start_character: None,
                        end_line: el,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Region),
                        collapsed_text: None,
                    });
                }
            }
            ItemKind::ExprStmt(expr) => {
                collect_folds(expr.span, doc, &mut ranges);
            }
            ItemKind::Let(def) => {
                collect_folds(def.value.span, doc, &mut ranges);
            }
            _ => {}
        }
    }
    ranges.sort_by_key(|r| r.start_line);
    ranges
}

fn collect_folds(span: Span, doc: &Document, ranges: &mut Vec<FoldingRange>) {
    // Add range if spans multiple lines.
    let (sl, _) = doc.position_at(span.start.into());
    let (el, _) = doc.position_at(span.end.into());
    if el > sl {
        ranges.push(FoldingRange {
            start_line: sl,
            start_character: None,
            end_line: el,
            end_character: None,
            kind: Some(FoldingRangeKind::Region),
            collapsed_text: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_scoring_exact_match() {
        let sort = score_sort_text("println", "print");
        assert!(
            sort.starts_with("09"),
            "prefix match should score ~900: {sort}"
        );

        let sort = score_sort_text("print", "print");
        assert!(
            sort.starts_with("1000"),
            "exact match should score 1000: {sort}"
        );
    }

    #[test]
    fn test_completion_scoring_prefix() {
        let sort = score_sort_text("io.readFile", "io.read");
        assert!(sort.starts_with("09"), "prefix match: {sort}");

        let sort = score_sort_text("len", "l");
        assert!(sort.starts_with("0901"), "short prefix: {sort}");
    }

    #[test]
    fn test_completion_scoring_contains() {
        let sort = score_sort_text("io.readFilePath", "Path");
        assert!(sort.starts_with("0500"), "contains match: {sort}");
    }

    #[test]
    fn test_completion_scoring_no_match() {
        let sort = score_sort_text("len", "xyz");
        assert!(sort.starts_with("0000"), "no match: {sort}");
    }

    #[test]
    fn test_completion_scoring_empty_prefix() {
        let sort = score_sort_text("anything", "");
        assert!(sort.starts_with("0500"), "empty prefix: {sort}");
    }

    #[test]
    fn test_completion_scoring_case_insensitive() {
        let sort = score_sort_text("PrintLn", "print");
        assert!(sort.starts_with("09"), "case-insensitive prefix: {sort}");
    }

    #[test]
    fn test_code_lens_reference_counts() {
        let source = "fn greet() = 42; let x = greet(); let y = greet();";
        let doc = Document::new("file:///test.n3v3".to_string(), source.to_string());
        if let Some(ref index) = doc.symbol_index {
            let refs = index.get_references("greet");
            let usage_count = refs.iter().filter(|r| !r.is_write).count();
            assert_eq!(usage_count, 2, "greet() should have 2 call sites");

            // Verify the definition exists
            let defs = index.get_definitions("greet");
            assert!(defs.is_some(), "greet should have a definition");
            let defs = defs.unwrap();
            assert!(
                defs.iter()
                    .any(|s| matches!(s.kind, IndexSymbolKind::Function)),
                "greet should be a function"
            );
        }
    }
}
