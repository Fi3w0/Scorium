//! The evaluated configuration tree: what a host application receives
//! after `scorium-lua` evaluates a document. Distinct from `ast::Item`
//! (syntax) the way `Value` is distinct from `ast::Expr` -- entries hold
//! resolved values, not expressions still waiting to be evaluated.

use crate::span::Span;
use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Leaf(LeafEntry),
    Node(NodeEntry),
    Include(IncludeEntry),
    HostCall(HostCallEntry),
}

impl Entry {
    pub fn span(&self) -> Span {
        match self {
            Entry::Leaf(e) => e.span,
            Entry::Node(e) => e.span,
            Entry::Include(e) => e.span,
            Entry::HostCall(e) => e.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeafEntry {
    pub key: String,
    pub key_span: Span,
    pub value: Value,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeEntry {
    pub name: String,
    pub name_span: Span,
    pub header: Option<String>,
    pub header_span: Option<Span>,
    pub children: Vec<Entry>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncludeEntry {
    pub path: String,
    pub resolved_path: Option<std::path::PathBuf>,
    pub span: Span,
}

/// A call into a host-registered operation that isn't itself a value
/// (e.g. an imperative action). Most host functions instead return a
/// `Value` used directly as part of an expression; this variant is for
/// registrations that want a standalone entry in the tree, matching the
/// "one registry, multiple surfaces" model.
#[derive(Debug, Clone, PartialEq)]
pub struct HostCallEntry {
    pub name: String,
    pub args: Vec<Value>,
    pub result: Option<Value>,
    pub span: Span,
}
