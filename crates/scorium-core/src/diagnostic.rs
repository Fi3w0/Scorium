//! Syntax-level diagnostics: everything the lexer and parser can report.
//! Each variant renders through `miette` with a source excerpt, a caret,
//! and (where useful) a message that names the fix, not just the failure.

use miette::Diagnostic;
use thiserror::Error;

use crate::span::Span;

#[derive(Debug, Error, Diagnostic)]
pub enum SyntaxError {
    #[error("unexpected character `{ch}`")]
    #[diagnostic(code(scorium::lex::unexpected_char))]
    UnexpectedChar {
        ch: char,
        #[label("not valid here")]
        span: Span,
    },

    #[error("unterminated string literal")]
    #[diagnostic(code(scorium::lex::unterminated_string), help("add a closing `\"`"))]
    UnterminatedString {
        #[label("string starts here")]
        span: Span,
    },

    #[error("unterminated block comment")]
    #[diagnostic(code(scorium::lex::unterminated_comment), help("add a closing `]]`"))]
    UnterminatedComment {
        #[label("comment starts here")]
        span: Span,
    },

    #[error("operators in expressions require spaces around them")]
    #[diagnostic(code(scorium::lex::squeezed_operator), help("write `{suggestion}`, not `{original}`"))]
    SqueezedOperator {
        original: String,
        suggestion: String,
        #[label("this operator needs spaces on both sides")]
        span: Span,
    },

    #[error("expected {expected}, found {found}")]
    #[diagnostic(code(scorium::parse::unexpected_token))]
    UnexpectedToken {
        expected: String,
        found: String,
        #[label("here")]
        span: Span,
    },

    #[error("`@{name}` only defines a variable on its own line (`@{name} = value`); in an expression use `{name}`")]
    #[diagnostic(code(scorium::parse::at_in_expression))]
    AtInExpression {
        name: String,
        #[label("not allowed in an expression")]
        span: Span,
    },

    #[error("`${name}` cannot be used in an expression")]
    #[diagnostic(code(scorium::parse::dollar_in_expression), help("use `{name}` for an expression value"))]
    DollarInExpression {
        name: String,
        #[label("not allowed in an expression")]
        span: Span,
    },

    #[error("`{keyword}` is a reserved word, not a node name")]
    #[diagnostic(code(scorium::parse::reserved_word))]
    ReservedWord {
        keyword: String,
        #[label("reserved")]
        span: Span,
    },

    #[error("unexpected end of file: {context}")]
    #[diagnostic(code(scorium::parse::unexpected_eof))]
    UnexpectedEof {
        context: String,
        #[label("expected more input after this")]
        span: Span,
    },

    #[error("include cycle detected: {chain}")]
    #[diagnostic(code(scorium::include::cycle))]
    IncludeCycle {
        chain: String,
        #[label("this include closes the cycle")]
        span: Span,
    },
}

/// Attaches source text to a [`SyntaxError`] for rendering: prefer this
/// over building a wrapper type, since `miette::Report` already knows how
/// to carry source code alongside a diagnostic.
pub fn with_source(error: SyntaxError, src: &crate::source::Source) -> miette::Report {
    miette::Report::new(error).with_source_code(src.named_source())
}
