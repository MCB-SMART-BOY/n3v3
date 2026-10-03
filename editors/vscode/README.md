# n3v3 Language Support for VS Code

Syntax highlighting, auto-completion, diagnostics, formatting, and code navigation for the [n3v3](https://github.com/MCB-SMART-BOY/n3v3) programming language.

## Features

- **Syntax highlighting** — Full TextMate grammar for `.n3v3` files
- **Diagnostics** — Real-time parse and type errors as you type
- **Auto-completion** — Keywords, stdlib functions, types, and type-aware method completion (54 methods across 5 receiver types)
- **Hover** — Type information and documentation on hover
- **Go to Definition** — Jump to symbol definitions
- **Find References** — Find all references to a symbol
- **Rename** — Rename symbols across files
- **Signature Help** — Function parameter hints (80+ builtin signatures)
- **Code Formatting** — Format documents with `n3v3 fmt`
- **Code Lens** — Reference counts above function/struct/trait definitions
- **Document Symbols** — Breadcrumb and outline support
- **Workspace Symbols** — Search symbols across the workspace
- **Semantic Tokens** — AST-based syntax highlighting (10 token types, 3 modifiers)
- **Inlay Hints** — Inline type annotations
- **Folding Ranges** — Code folding for blocks, structs, enums, traits, impls, match
- **Code Actions** — Quick-fix suggestions for parse and type errors

## Requirements / 前置条件

- [n3v3 CLI](https://github.com/MCB-SMART-BOY/n3v3) installed and available on `$PATH`.
- Install a built extension package that includes `out/extension.js` for LSP features. `n3v3 setup vscode` copies the language metadata and syntax grammar from a source checkout; it does not copy or compile the extension's JavaScript.

- 已安装 [n3v3 CLI](https://github.com/MCB-SMART-BOY/n3v3)，并可通过 `$PATH` 找到。
- 如需 LSP 功能，请安装包含 `out/extension.js` 的已构建扩展包。`n3v3 setup vscode` 只会从源码检出目录复制语言元数据和语法规则，不会复制或编译扩展的 JavaScript。

## Quick Start / 快速开始

1. Install n3v3: follow the [installation guide](https://github.com/MCB-SMART-BOY/n3v3#installation).
2. Install a built n3v3 extension package in VS Code.
3. Open a `.n3v3` file to activate the extension.

1. 按[安装指南](https://github.com/MCB-SMART-BOY/n3v3#installation)安装 n3v3。
2. 在 VS Code 中安装已构建的 n3v3 扩展包。
3. 打开 `.n3v3` 文件以激活扩展。

## Configuration

This extension contributes the following settings (configurable in VS Code settings):

| Setting | Default | Description |
|---------|---------|-------------|
| `editor.tabSize` | 4 | Tab size for n3v3 files |
| `editor.insertSpaces` | true | Use spaces instead of tabs |
| `editor.codeLens` | true | Show reference counts |

## License

MPL-2.0 — same as the n3v3 language project.
