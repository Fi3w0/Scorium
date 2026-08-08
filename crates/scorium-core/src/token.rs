//! Lexical tokens. The lexer runs eagerly over the whole source into a
//! `Vec<Token>` (config files are small; a streaming lexer buys nothing
//! here), so the parser gets cheap arbitrary lookahead by index.

use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Int(i64),
    Float(f64),
    /// The text between the quotes, with `\"`, `\\`, `\n`, `\t`, `\r`
    /// escapes already resolved.
    QuotedString(String),
    /// A bare (unquoted) run of non-delimiter characters. Could turn out
    /// to be a keyword-like identifier, a literal fallback string, or (if
    /// it contains `$`) an interpolated string -- the parser decides.
    BareWord(String),
    /// Hex digits only, no leading `#`, exactly 6 or 8 characters.
    ColorLit(String),
    /// The numeric part and the unit suffix (`ms`, `s`, `m`) of a duration
    /// literal, kept apart so the parser doesn't need to re-split it.
    DurationLit(f64, String),
    /// Raw bytes between a `script {` opener and its matching `}`. The
    /// lexer recognizes Lua strings/comments only to balance braces; the
    /// parser passes the original source slice through unchanged.
    ScriptBody,

    // Keywords
    If,
    Elseif,
    Else,
    For,
    While,
    Do,
    End,
    Return,
    Local,
    Fn,
    Script,
    Include,
    And,
    Or,
    Not,
    True,
    False,
    Nil,

    // Punctuation
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Comma,
    Eq,
    Dot,
    At,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    DotDot,
    EqEq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,

    /// Trivia: preserved so the formatter can round-trip comments. Never
    /// meaningful to the parser's grammar, but present in the stream.
    Comment(String, CommentStyle),

    Eof,
}

/// Which marker a comment used, so the formatter can preserve it instead
/// of silently normalizing a `#` comment to `--` or vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentStyle {
    Hash,
    DashDash,
    Block,
}

impl TokenKind {
    pub fn is_comment(&self) -> bool {
        matches!(self, TokenKind::Comment(..))
    }

    /// A short human name for diagnostics ("expected X, found `end`").
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Int(n) => n.to_string(),
            TokenKind::Float(n) => n.to_string(),
            TokenKind::QuotedString(s) => format!("\"{s}\""),
            TokenKind::BareWord(s) => s.clone(),
            TokenKind::ColorLit(h) => format!("#{h}"),
            TokenKind::DurationLit(n, u) => format!("{n}{u}"),
            TokenKind::ScriptBody => "script body".into(),
            TokenKind::If => "if".into(),
            TokenKind::Elseif => "elseif".into(),
            TokenKind::Else => "else".into(),
            TokenKind::For => "for".into(),
            TokenKind::While => "while".into(),
            TokenKind::Do => "do".into(),
            TokenKind::End => "end".into(),
            TokenKind::Return => "return".into(),
            TokenKind::Local => "local".into(),
            TokenKind::Fn => "fn".into(),
            TokenKind::Script => "script".into(),
            TokenKind::Include => "include".into(),
            TokenKind::And => "and".into(),
            TokenKind::Or => "or".into(),
            TokenKind::Not => "not".into(),
            TokenKind::True => "true".into(),
            TokenKind::False => "false".into(),
            TokenKind::Nil => "nil".into(),
            TokenKind::LBrace => "{".into(),
            TokenKind::RBrace => "}".into(),
            TokenKind::LBracket => "[".into(),
            TokenKind::RBracket => "]".into(),
            TokenKind::LParen => "(".into(),
            TokenKind::RParen => ")".into(),
            TokenKind::Comma => ",".into(),
            TokenKind::Eq => "=".into(),
            TokenKind::Dot => ".".into(),
            TokenKind::At => "@".into(),
            TokenKind::Plus => "+".into(),
            TokenKind::Minus => "-".into(),
            TokenKind::Star => "*".into(),
            TokenKind::Slash => "/".into(),
            TokenKind::Percent => "%".into(),
            TokenKind::DotDot => "..".into(),
            TokenKind::EqEq => "==".into(),
            TokenKind::NotEq => "~=".into(),
            TokenKind::Lt => "<".into(),
            TokenKind::Gt => ">".into(),
            TokenKind::LtEq => "<=".into(),
            TokenKind::GtEq => ">=".into(),
            TokenKind::Comment(..) => "comment".into(),
            TokenKind::Eof => "end of file".into(),
        }
    }

    /// Does this token continue an expression that started at the
    /// previous primary? Used to decide "single bare token" (a plain
    /// value) vs. "the start of a real expression" (Pratt parsing, where
    /// `$name`/`@name` are errors).
    pub fn continues_expression(&self) -> bool {
        matches!(
            self,
            TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Percent
                | TokenKind::DotDot
                | TokenKind::EqEq
                | TokenKind::NotEq
                | TokenKind::Lt
                | TokenKind::Gt
                | TokenKind::LtEq
                | TokenKind::GtEq
                | TokenKind::And
                | TokenKind::Or
                | TokenKind::Dot
                | TokenKind::LParen
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

pub fn keyword(word: &str) -> Option<TokenKind> {
    Some(match word {
        "if" => TokenKind::If,
        "elseif" => TokenKind::Elseif,
        "else" => TokenKind::Else,
        "for" => TokenKind::For,
        "while" => TokenKind::While,
        "do" => TokenKind::Do,
        "end" => TokenKind::End,
        "return" => TokenKind::Return,
        "local" => TokenKind::Local,
        "fn" => TokenKind::Fn,
        "script" => TokenKind::Script,
        "include" => TokenKind::Include,
        "and" => TokenKind::And,
        "or" => TokenKind::Or,
        "not" => TokenKind::Not,
        "true" => TokenKind::True,
        "false" => TokenKind::False,
        "nil" => TokenKind::Nil,
        _ => return None,
    })
}
