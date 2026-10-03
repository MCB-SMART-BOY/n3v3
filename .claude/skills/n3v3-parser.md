# n3v3-parser: Lexer & Parser

## Architecture

```
Source Code (.n3v3)
       │
       ▼
┌────────────────────────────────────────────────────┐
│  n3v3-lexer (logos derive macro)                    │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────┐ │
│  │ char stream  │→ │ token stream │→ │ [Spanned  │ │
│  │ (peek/adv)   │  │ (Token enum) │  │ <Token>]  │ │
│  └──────────────┘  └──────────────┘  └───────────┘ │
│  Handles: int/float/hex/oct/bin, strings, comments, │
│  chars, identifiers, operators, delimiters           │
└────────────────────────────────────────────────────┘
       │ [Spanned<Token>]
       ▼
┌────────────────────────────────────────────────────┐
│  n3v3-parser (recursive descent, LL(1))             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────┐  │
│  │ token stream │→ │ Pratt        │→ │ AST      │  │
│  │ (pos cursor) │  │ precedence   │  │ SourceFile│  │
│  └──────────────┘  └──────────────┘  └──────────┘  │
│  Handles: source_file→items→exprs→patterns→types   │
└────────────────────────────────────────────────────┘
       │ SourceFile { items, tail_expr, span }
       ▼
┌────────────────────────────────────────────────────┐
│  n3v3-syntax (AST node definitions)                 │
│  Expr, Item, ExprKind, Pattern, Type, SourceFile, Span  │
└────────────────────────────────────────────────────┘
```

## Syntax Transformations (v3.0 → v4.0)

| Old Form | v3.0 Form | v4.0 Form | Rationale |
|----------|-----------|-----------|-----------|
| `struct Foo {}` / `enum Bar {}` | `type Foo = {}` / `type Bar = \| ...` | same | Unified type declaration |
| `import std.list` | `use std.list` | same | Shorter, Rust-aligned |
| `import std.io as io` | same | `use std.io = io` | `=` matches binding syntax |
| `fn(x) x + 1` | `\|x\| x + 1` | same | Rust-style lambda |
| `#{ x = 1 }` | `{ x = 1 }` | same | Delimiter-driven container theory |
| `// comment` | line-start `& comment` / 行首 `& comment` | `-- comment` (line / 行), `-- -- ... -- --` (block / 块); line-start `&` remains supported / 行首 `&` 仍可用 | `&` comments require line start (optional indentation) / `&` 注释必须在行首（可缩进） |
| `a // b` (merge) | `a & b` | same | Inside expressions, `&` merges records / 表达式内 `&` 合并记录 |
| `if cond then a else b` | same | `if cond -> a else b` | Arrow unifies with match |
| `lazy expr` | same | `~expr` | Lightweight prefix |
| `fn foo() effect = ...` | same | `fn foo() = ...` | Auto-inferred |
| `pub fn` | same | `fn` | All public by default |

**Backward compatibility:** The parser accepts 10 legacy spellings:
`struct`, `enum`, `import`, `pub`, `as`, `then`, `lazy`, `effect`, `super`,
and `crate`. The lexer emits dedicated tokens for `struct`, `enum`, `super`,
and `crate`; the other six remain identifiers and are recognized contextually
by the parser. Canonical v4.0 source uses 12 canonical keywords.

## Lexer Design (n3v3-lexer)

### Token dispatch flow

```
peek_char()
    │
    ├── digit/number → number() → Int | Float
    ├── '"'           → string() → Str
    ├── '\''          → char()   → Char
    ├── '`'           → interpolated() → InterpolatedStr
    ├── letter/_      → ident()  → Ident | Keyword
    ├── '/'           → path or div
    ├── '&'           → line-start legacy comment or record merge (&& = and)
    ├── '-'           → -- line comment / -- -- ... -- -- block comment
    ├── '|'           → |, |>, ||
    ├── '{'/'}'/'('...→ Delimiter
    └── ...
```

**Slash disambiguation.** The lexer tracks the last emitted token kind
(`Lexer::last_token_kind`). `/` begins an absolute path literal only when the
previous token cannot end an operand (`can_start_operand()`); after an operand
(Int, Float, String, Char, Bool/True/False, PathLit, Ident, `?`, `)`, `]`, `}`,
interpolated-string end, `self`) it is the division operator, so `6/2` lexes as
`Int(6) Slash Int(2)` and `/etc/hosts` remains a path literal. A path starts
only when the next character is alphanumeric or `_ - .`.

**Identifier characters.** `identifier()` requires an ASCII letter or `_` in
first position (anything else is `E0001`), then consumes `char::is_alphanumeric`,
so continuation characters may be Unicode (`café` is one identifier). The
tree-sitter grammar in `tree-sitter-n3v3/grammar.js` is stricter
(`[a-zA-Z_][a-zA-Z0-9_]*`) and does not yet track v4.0 syntax
(`if -> else`, `~`, bare `|` enums, postfix `?`).

### Resolved gaps
- Unicode `\u{...}` escapes: ✅ Done (v3.18+)
- Shebang handling: ✅ Done (M22) — handled by parser

## Parser Design (n3v3-parser)

### Expression precedence (Pratt parser)

```
1.  .  ?.  ()  []           ← primary/postfix
2.  ?                        ← postfix error propagation
3.  !  -                     ← prefix unary
4.  ^                        ← power
5.  *  /  %                  ← multiplicative
6.  +  -                     ← additive
7.  ++                       ← concatenation
8.  <  <=  >  >=  ==  !=     ← comparison
9.  &&                       ← logical and
10. ||                       ← logical or
11. ??                       ← null coalescing
12. |>                       ← pipe
13. &                        ← record merge
```

### Recursive descent structure

```
parse_module()
  └── parse_item() × N
        ├── parse_let()      → LetBinding
        ├── parse_fn()       → FnDef
        ├── parse_type()     → TypeDef (struct/enum unified)
        ├── parse_trait()    → TraitDef
        ├── parse_impl()     → ImplBlock
        ├── parse_use()      → UseStatement
        └── parse_expr()     → Expr (top-level expression tail)

parse_expr(0)  ← Pratt entry at minimum binding power
  └── parse_atom()
        ├── literal (int/float/string/char/bool/unit)
        ├── path literal (./ ../ /)
        ├── identifier or call
        ├── |params| body   ← lambda (v3.0)
        ├── { fields }      ← record (v3.0)
        ├── [ items ]       ← list
        ├── ( expr )        ← grouping or tuple
        ├── if -> else
        ├── match { arms }
        └── ~expr
```

## AST Types (n3v3-syntax)

```rust
pub struct SourceFile {
    pub items: Vec<Item>,
    pub tail_expr: Option<Expr>,
    pub comments: Vec<Comment>,
    pub span: Span,
}
pub enum ItemKind {
    Let(LetDef),
    Fn(FnDef),
    TypeAlias(TypeAlias),
    Struct(StructDef),
    Enum(EnumDef),
    Trait(TraitDef),
    Impl(ImplDef),
    Import(ImportDef),
    ExprStmt(Expr),
}
pub enum ExprKind {
    Int(Int),
    String(String),
    Var(Ident),
    Record(Vec<RecordField>),
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    Lambda { params: Vec<LambdaParam>, return_type: Option<Type>, body: Box<Expr> },
    Call { func: Box<Expr>, args: Vec<Expr> },
    Binary { op: BinOp, left: Box<Expr>, right: Box<Expr> },
    If { condition: Box<Expr>, then_branch: Box<Expr>, else_branch: Box<Expr> },
    Match { scrutinee: Box<Expr>, arms: Vec<MatchArm> },
    Block { stmts: Vec<Stmt>, expr: Option<Box<Expr>> },
    Lazy(Box<Expr>),
    Path(Vec<Ident>),
}
```

## Public AST API boundary

All public AST enums that describe syntax (`ItemKind`, `VariantKind`,
`PathPrefix`, `ImportItems`, `Visibility`, `ExprKind`, `StmtKind`, `BinOp`,
`UnaryOp`, `StringPart`, `PatternKind`, `LiteralPattern`, and `TypeKind`) are
`#[non_exhaustive]`. External consumers must include a wildcard arm when
matching them; this is the v5 API policy for future syntax evolution.

The AST-to-HIR boundary must preserve unsupported future forms as diagnostics
or reject them explicitly. It must not silently turn a node into a wildcard,
empty import, or unrelated fallback.

## Integration Points

| From | To | Data |
|------|----|------|
| n3v3-lexer | n3v3-parser | `Vec<Spanned<Token>>` |
| n3v3-parser | n3v3-hir | `SourceFile` |
| n3v3-parser | n3v3-fmt | `SourceFile` (round-trip) |
| n3v3-parser | n3v3-lsp | `SourceFile` (for analysis) |

## Key Files

| File | What |
|------|------|
| `crates/n3v3-lexer/src/lexer.rs` | Lexer — `peek_char()`, `advance()`, `number()`, `identifier()`, `skip_block_comment()`, `can_start_operand()` |
| `crates/n3v3-lexer/src/token.rs` | Token enum — Int, Float, Str, Ident, Keywords, Delimiters, Operators |
| `crates/n3v3-common/src/span.rs` | `Span` / `BytePos` — start/end positions for diagnostics |
| `crates/n3v3-parser/src/lib.rs` | Parser entry + `Parser::new_with_comments()`, token/trivia wiring |
| `crates/n3v3-parser/src/parser.rs` | Recursive descent + Pratt parser + `parse_module()` |
| `crates/n3v3-parser/src/recovery.rs` | Error recovery — sync points and diagnostic de-duplication |
| `crates/n3v3-syntax/src/expr.rs` | Core `Expr` / `ExprKind` / `Stmt` / `MatchArm` types |
| `crates/n3v3-syntax/src/ast.rs` | `Item` / `ItemKind` / `SourceFile` / `FnDef` / `TypeAlias` |
| `crates/n3v3-syntax/src/types.rs` | `TypeKind` / `Type` / `GenericParam` / trait AST |
| `crates/n3v3-syntax/src/pattern.rs` | `PatternKind` / `Pattern` types |
| `crates/n3v3-common/src/trivia.rs` | `Comment` / `CommentKind` — lexer trivia consumed by n3v3-fmt |
| `tests/parser.rs` | 239 parser tests (golden and integration) |

- **Golden tests**: Parse source, compare formatted AST output to `.txt` baseline.
- **Integration tests**: `tests/parser.rs` — tests cover canonical and legacy
  syntax forms, including wildcard patterns.
- **Verified parser coverage**: shebang handling, canonical record/enum forms,
  implicit record arguments, legacy effect markers, delimiter-sensitive
  `match` parsing, and wildcard-vs-variable pattern classification.
- **Comment trivia**: `tokenize_with_trivia()` passes comments into
  `SourceFile.comments`; the formatter emits them. This does not guarantee
  exact preservation of their original placement.
