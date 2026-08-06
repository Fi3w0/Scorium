//! `scorium-core`: source handling, spans, the lexer, the parser, the
//! AST, typed literal values, and syntax diagnostics for the Scorium
//! configuration language. Application-independent -- nothing here knows
//! what a valid node or key is for any particular host.

pub mod ast;
pub mod diagnostic;
pub mod entry;
pub mod lexer;
pub mod parser;
pub mod source;
pub mod span;
pub mod token;
pub mod value;

pub use ast::Document;
pub use diagnostic::SyntaxError;
pub use source::Source;
pub use span::{Span, Spanned};
pub use value::{ColorValue, DurationUnit, DurationValue, Value};

/// Lexes and parses `source`'s text into a [`Document`].
pub fn parse(source: &Source) -> Result<Document, SyntaxError> {
    let tokens = lexer::tokenize(source.text())?;
    parser::parse(source.text(), tokens)
}
