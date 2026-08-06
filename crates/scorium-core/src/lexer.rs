//! The Scorium lexer: source text -> a flat `Vec<Token>` (comments
//! included as trivia). Runs eagerly since config files are small, which
//! gives the parser free arbitrary lookahead by index.
//!
//! The one lexer-level design decision worth calling out: operators need
//! spaces around them (`base * 2`, not `base*2`), but `+`/`-`/`:`/`.` stay
//! embeddable unspaced (`SUPER+Return`, `spawn:kitty`, `workspace-$i`),
//! because those are common in bare-string "combo" tokens and the spec's
//! own examples rely on them. `*`, `/`, `%`, and the comparison operators
//! are the ones that can silently produce a confusing bare word, so those
//! are the ones that error when squeezed between two word characters.
//! See `docs/GRAMMAR.md` for the full writeup of this ambiguity.

use crate::diagnostic::SyntaxError;
use crate::span::Span;
use crate::token::{keyword, CommentStyle, Token, TokenKind};

pub fn tokenize(source: &str) -> Result<Vec<Token>, SyntaxError> {
    Lexer::new(source).run()
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

fn is_hard_delim(c: char) -> bool {
    matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | ',' | '"' | '\'' | '@' | '=' | '#')
}

/// A char that could start a new bare-word token (used to decide whether
/// an operator is "squeezed" against what follows it).
fn continues_word(c: char) -> bool {
    !c.is_whitespace() && !is_hard_delim(c)
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, bytes: src.as_bytes(), pos: 0 }
    }

    fn run(mut self) -> Result<Vec<Token>, SyntaxError> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token()?;
            let is_eof = tok.kind == TokenKind::Eof;
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn char_at(&self, byte_pos: usize) -> Option<char> {
        self.src[byte_pos..].chars().next()
    }

    fn current(&self) -> Option<char> {
        self.char_at(self.pos)
    }

    fn peek(&self, n: usize) -> Option<char> {
        let mut p = self.pos;
        for _ in 0..n {
            p += self.char_at(p)?.len_utf8();
        }
        self.char_at(p)
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.current()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Result<Token, SyntaxError> {
        self.skip_whitespace();
        let start = self.pos as u32;
        let Some(c) = self.current() else {
            return Ok(Token::new(TokenKind::Eof, Span::at(start)));
        };

        let simple = |kind: TokenKind, len: usize| (kind, len);
        let single_char: Option<(TokenKind, usize)> = match c {
            '(' => Some(simple(TokenKind::LParen, 1)),
            ')' => Some(simple(TokenKind::RParen, 1)),
            '[' => Some(simple(TokenKind::LBracket, 1)),
            ']' => Some(simple(TokenKind::RBracket, 1)),
            '{' => Some(simple(TokenKind::LBrace, 1)),
            '}' => Some(simple(TokenKind::RBrace, 1)),
            ',' => Some(simple(TokenKind::Comma, 1)),
            '@' => Some(simple(TokenKind::At, 1)),
            _ => None,
        };
        if let Some((kind, len)) = single_char {
            self.pos += len;
            return Ok(Token::new(kind, Span::new(start, self.pos as u32)));
        }

        if c == '"' || c == '\'' {
            return self.scan_quoted_string(start, c);
        }
        if c == '#' {
            return self.scan_hash(start);
        }
        if c == '=' {
            return Ok(if self.peek(1) == Some('=') {
                self.pos += 2;
                Token::new(TokenKind::EqEq, Span::new(start, self.pos as u32))
            } else {
                self.pos += 1;
                Token::new(TokenKind::Eq, Span::new(start, self.pos as u32))
            });
        }
        if c == '-' && self.peek(1) == Some('-') {
            return self.scan_dash_comment(start);
        }

        self.scan_word_or_operator(start)
    }

    /// Matches an operator starting at `pos` (absolute byte offset),
    /// returning its length, token kind, and whether squeezing it between
    /// two word characters should be an error (`*`, `/`, `%`, comparisons)
    /// versus silently absorbed into the surrounding word (`+`, `-`).
    fn match_operator(&self, pos: usize) -> Option<(usize, TokenKind, bool)> {
        let c = self.char_at(pos)?;
        let c2 = self.char_at(pos + c.len_utf8());
        match (c, c2) {
            ('<', Some('=')) => Some((2, TokenKind::LtEq, true)),
            ('>', Some('=')) => Some((2, TokenKind::GtEq, true)),
            ('~', Some('=')) => Some((2, TokenKind::NotEq, true)),
            ('+', _) => Some((1, TokenKind::Plus, false)),
            ('-', _) => Some((1, TokenKind::Minus, false)),
            ('*', _) => Some((1, TokenKind::Star, true)),
            ('/', _) => Some((1, TokenKind::Slash, true)),
            ('%', _) => Some((1, TokenKind::Percent, true)),
            ('<', _) => Some((1, TokenKind::Lt, true)),
            ('>', _) => Some((1, TokenKind::Gt, true)),
            _ => None,
        }
    }

    fn scan_word_or_operator(&mut self, start: u32) -> Result<Token, SyntaxError> {
        let start_pos = self.pos;
        loop {
            if self.eof() {
                break;
            }
            let c = self.current().unwrap();
            if c.is_whitespace() || is_hard_delim(c) {
                break;
            }
            if c == '-' && self.peek(1) == Some('-') {
                break; // start of a `--` comment ends the current word
            }
            if let Some((op_len, op_kind, error_on_squeeze)) = self.match_operator(self.pos) {
                let squeezed_left = self.pos > start_pos;
                let after = self.char_at(self.pos + op_len);
                let squeezed_right = after.is_some_and(continues_word)
                    && !(after == Some('-') && self.char_at(self.pos + op_len + 1) == Some('-'));
                if squeezed_left && squeezed_right {
                    if error_on_squeeze {
                        let op_span = Span::new(self.pos as u32, (self.pos + op_len) as u32);
                        let word_end = self.pos + op_len + after.map_or(0, |c| c.len_utf8());
                        let original = self.src[start_pos..word_end].to_string();
                        let suggestion = format!(
                            "{} {} {}",
                            &self.src[start_pos..self.pos],
                            op_kind.describe(),
                            &self.src[self.pos + op_len..word_end]
                        );
                        return Err(SyntaxError::SqueezedOperator { original, suggestion, span: op_span });
                    }
                    // '+' / '-': absorb into the bare word and keep going.
                    self.pos += op_len;
                    continue;
                }
                // Not squeezed: this operator ends the current word (or,
                // if nothing was scanned yet, it *is* the token).
                break;
            }
            self.advance();
        }

        if self.pos == start_pos {
            let (len, kind, _) = self
                .match_operator(self.pos)
                .expect("scan_word_or_operator called on a non-word, non-operator character");
            self.pos += len;
            return Ok(Token::new(kind, Span::new(start, self.pos as u32)));
        }

        let text = &self.src[start_pos..self.pos];
        Ok(Token::new(classify_word(text), Span::new(start, self.pos as u32)))
    }

    /// `quote` is `"` or `'` -- Scorium proper only ever writes `"`, but a
    /// `script { }` body may contain Lua's `'...'` strings, and accepting
    /// both keeps this lexer's brace-counting (used to find the raw text
    /// of a script block) accurate even though script bodies aren't
    /// otherwise interpreted by this tokenizer.
    fn scan_quoted_string(&mut self, start: u32, quote: char) -> Result<Token, SyntaxError> {
        self.advance(); // opening quote
        let mut out = String::new();
        loop {
            match self.current() {
                None => {
                    return Err(SyntaxError::UnterminatedString { span: Span::new(start, self.pos as u32) })
                }
                Some(c) if c == quote => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    match self.advance() {
                        Some('"') => out.push('"'),
                        Some('\'') => out.push('\''),
                        Some('\\') => out.push('\\'),
                        Some('n') => out.push('\n'),
                        Some('t') => out.push('\t'),
                        Some('r') => out.push('\r'),
                        Some(other) => {
                            out.push('\\');
                            out.push(other);
                        }
                        None => {
                            return Err(SyntaxError::UnterminatedString {
                                span: Span::new(start, self.pos as u32),
                            })
                        }
                    }
                }
                Some(c) => {
                    out.push(c);
                    self.advance();
                }
            }
        }
        Ok(Token::new(TokenKind::QuotedString(out), Span::new(start, self.pos as u32)))
    }

    /// `#` starts either a color literal (`#8EDDFF`) or a comment (`#
    /// hello`), decided by whether a maximal run of hex digits right
    /// after it is exactly 6 or 8 characters long.
    fn scan_hash(&mut self, start: u32) -> Result<Token, SyntaxError> {
        let hash_end = self.pos + 1;
        let mut run_end = hash_end;
        while self.char_at(run_end).is_some_and(|c| c.is_ascii_hexdigit()) {
            run_end += 1;
        }
        let hex_len = run_end - hash_end;
        if hex_len == 6 || hex_len == 8 {
            self.pos = run_end;
            let hex = self.src[hash_end..run_end].to_string();
            return Ok(Token::new(TokenKind::ColorLit(hex), Span::new(start, self.pos as u32)));
        }
        // Not a color: a `#` comment runs to end of line.
        self.pos = hash_end;
        while let Some(c) = self.current() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
        let text = self.src[hash_end..self.pos].to_string();
        Ok(Token::new(
            TokenKind::Comment(text, CommentStyle::Hash),
            Span::new(start, self.pos as u32),
        ))
    }

    /// `--` starts either a line comment or, if followed by `[[`, a Lua
    /// style block comment terminated by `]]`.
    fn scan_dash_comment(&mut self, start: u32) -> Result<Token, SyntaxError> {
        self.pos += 2; // "--"
        if self.current() == Some('[') && self.peek(1) == Some('[') {
            self.pos += 2;
            let body_start = self.pos;
            loop {
                match self.current() {
                    None => {
                        return Err(SyntaxError::UnterminatedComment {
                            span: Span::new(start, self.pos as u32),
                        })
                    }
                    Some(']') if self.peek(1) == Some(']') => {
                        let text = self.src[body_start..self.pos].to_string();
                        self.pos += 2;
                        return Ok(Token::new(
                            TokenKind::Comment(text, CommentStyle::Block),
                            Span::new(start, self.pos as u32),
                        ));
                    }
                    _ => {
                        self.advance();
                    }
                }
            }
        }
        let body_start = self.pos;
        while let Some(c) = self.current() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
        let text = self.src[body_start..self.pos].to_string();
        Ok(Token::new(
            TokenKind::Comment(text, CommentStyle::DashDash),
            Span::new(start, self.pos as u32),
        ))
    }
}

fn classify_word(text: &str) -> TokenKind {
    if let Some(kw) = keyword(text) {
        return kw;
    }
    if let Ok(i) = text.parse::<i64>() {
        return TokenKind::Int(i);
    }
    if is_plain_float(text) {
        if let Ok(f) = text.parse::<f64>() {
            return TokenKind::Float(f);
        }
    }
    if let Some((num, unit)) = split_duration(text) {
        if let Ok(amount) = num.parse::<f64>() {
            return TokenKind::DurationLit(amount, unit.to_string());
        }
    }
    TokenKind::BareWord(text.to_string())
}

fn is_plain_float(text: &str) -> bool {
    let Some((int_part, frac_part)) = text.split_once('.') else {
        return false;
    };
    !int_part.is_empty()
        && !frac_part.is_empty()
        && int_part.chars().all(|c| c.is_ascii_digit())
        && frac_part.chars().all(|c| c.is_ascii_digit())
}

/// Splits a token like `600ms` / `1.5s` / `2m` into its numeric prefix and
/// unit suffix. A bare number is never treated as a duration -- the unit
/// is part of the type, not inferred.
fn split_duration(text: &str) -> Option<(&str, &str)> {
    for unit in ["ms", "s", "m"] {
        if let Some(num) = text.strip_suffix(unit) {
            if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit() || c == '.') {
                return Some((num, unit));
            }
        }
    }
    None
}
