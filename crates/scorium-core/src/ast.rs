//! The Scorium syntax tree. Produced by the parser, consumed by
//! `scorium-lua` (evaluation) and `scorium-format` (pretty-printing).
//!
//! Comment preservation covers comments that lead an item (on their own
//! line above it) or trail one (on the same line, after it) at
//! statement/item granularity -- a comment between `=` and its value, or
//! inside a list or call's parentheses, is not tracked and is dropped by
//! the formatter. That limitation is documented in `docs/LANGUAGE.md`.

use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub text: String,
    pub block: bool,
    pub span: Span,
}

/// Comment trivia attached to an [`Item`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Trivia {
    pub leading: Vec<Comment>,
    pub trailing: Option<Comment>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub items: Vec<Item>,
    /// Comments left over after the last item (end-of-file trivia).
    pub trailing: Vec<Comment>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    pub span: Span,
    pub trivia: Trivia,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Leaf(LeafDecl),
    Node(NodeDecl),
    VarDef(VarDef),
    Include(IncludeStmt),
    If(IfStmt),
    For(ForStmt),
    While(WhileStmt),
    Local(LocalStmt),
    Return(ReturnStmt),
    FnDef(FnDef),
    Script(ScriptBlock),
    Call(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeafDecl {
    pub key: String,
    pub key_span: Span,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeaderValue {
    Bare(String),
    Quoted(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeDecl {
    pub name: String,
    pub name_span: Span,
    pub header: Option<HeaderValue>,
    pub header_span: Option<Span>,
    pub body: Vec<Item>,
    pub body_span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarDef {
    pub name: String,
    pub name_span: Span,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncludeStmt {
    pub path: StrLit,
    pub path_span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfStmt {
    pub cond: Expr,
    pub then_body: Vec<Item>,
    pub elifs: Vec<(Expr, Vec<Item>)>,
    pub else_body: Option<Vec<Item>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForStmt {
    pub var: String,
    pub var_span: Span,
    pub start: Expr,
    pub stop: Expr,
    pub step: Option<Expr>,
    pub body: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhileStmt {
    pub cond: Expr,
    pub body: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalStmt {
    pub name: String,
    pub name_span: Span,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDef {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<String>,
    pub body: Vec<Item>,
}

/// `script { ... }`: raw text handed to the sandboxed Lua runtime
/// verbatim, never transpiled or reformatted.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptBlock {
    pub raw: String,
    pub inner_span: Span,
}

/// A string literal value: either fully quoted (verbatim, no
/// interpolation) or bare (a sequence of literal text and `$name`
/// interpolation parts).
#[derive(Debug, Clone, PartialEq)]
pub enum StrLit {
    Quoted(String),
    Bare(Vec<StrPart>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    Lit(String),
    Interp(String, Span),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    Nil,
    Str(StrLit),
    /// `#RRGGBB` / `#RRGGBBAA`, hex digits without the `#`.
    Color(String),
    Duration(f64, String),
    List(Vec<Expr>),
    /// A bare identifier used where an expression is expected. Resolved
    /// at evaluation time: a known variable/parameter/loop-var becomes a
    /// reference, anything else falls back to a literal string -- this is
    /// what lets `select(kitty, alacritty, foot)` skip quoting.
    Ident(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    /// `base.field`, split from a single bare-word token at parse time
    /// (see `docs/GRAMMAR.md`); resolved at evaluation time the same way
    /// `Ident` is, since `base` might turn out to be a literal string.
    Member(Box<Expr>, String, Span),
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}
