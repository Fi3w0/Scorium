//! Recursive-descent parser: tokens -> [`ast::Document`].
//!
//! The one non-obvious design point is how a leaf's value decides between
//! "a single bare token" (a literal, possibly with `$` interpolation) and
//! "a real expression" (Pratt-parsed, where `$name`/`@name` are errors):
//! it looks at whether the token *after* the first one continues an
//! expression (an operator, or `(` for a call). One token with no
//! continuation is always a value; anything else goes through full
//! expression parsing. See `docs/GRAMMAR.md`.

use crate::ast::*;
use crate::diagnostic::SyntaxError;
use crate::span::Span;
use crate::token::{CommentStyle, Token, TokenKind};

pub fn parse(src: &str, tokens: Vec<Token>) -> Result<Document, SyntaxError> {
    Parser::new(src, tokens).parse_document()
}

struct Parser<'a> {
    src: &'a str,
    tokens: Vec<Token>,
    pos: usize,
}

/// The keywords that begin a statement, as opposed to a node/leaf name.
fn statement_keyword(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::If
            | TokenKind::For
            | TokenKind::While
            | TokenKind::Local
            | TokenKind::Return
            | TokenKind::Fn
            | TokenKind::Script
            | TokenKind::Include
    )
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, tokens: Vec<Token>) -> Self {
        Self { src, tokens, pos: 0 }
    }

    // -- low-level token access ------------------------------------------

    fn last_index(&self) -> usize {
        self.tokens.len() - 1
    }

    /// The index of the first non-comment token at or after `from`.
    fn sig(&self, from: usize) -> usize {
        let mut i = from.min(self.last_index());
        while i < self.last_index() && self.tokens[i].kind.is_comment() {
            i += 1;
        }
        i
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.sig(self.pos)]
    }

    fn peek2(&self) -> &Token {
        let i = self.sig(self.pos);
        &self.tokens[self.sig(i + 1)]
    }

    fn current_span(&self) -> Span {
        self.peek().span
    }

    fn bump(&mut self) -> Token {
        let i = self.sig(self.pos);
        let tok = self.tokens[i].clone();
        if !matches!(tok.kind, TokenKind::Eof) {
            self.pos = i + 1;
        } else {
            self.pos = i;
        }
        tok
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind)
    }

    fn expect(&mut self, kind: TokenKind, expected: &str) -> Result<Token, SyntaxError> {
        if self.check(&kind) {
            Ok(self.bump())
        } else if matches!(self.peek().kind, TokenKind::Eof) {
            Err(SyntaxError::UnexpectedEof { context: expected.to_string(), span: self.current_span() })
        } else {
            Err(SyntaxError::UnexpectedToken {
                expected: expected.to_string(),
                found: self.peek().kind.describe(),
                span: self.current_span(),
            })
        }
    }

    /// Consumes a name (node/leaf/variable/function identifier),
    /// producing a friendly error if a reserved word appears instead.
    fn expect_name(&mut self, context: &str) -> Result<(String, Span), SyntaxError> {
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::BareWord(w) => {
                self.bump();
                Ok((w.clone(), tok.span))
            }
            other if statement_keyword(other) || matches!(other, TokenKind::Else | TokenKind::Elseif | TokenKind::End) => {
                Err(SyntaxError::ReservedWord { keyword: other.describe(), span: tok.span })
            }
            TokenKind::Eof => Err(SyntaxError::UnexpectedEof { context: context.to_string(), span: tok.span }),
            other => Err(SyntaxError::UnexpectedToken { expected: context.to_string(), found: other.describe(), span: tok.span }),
        }
    }

    fn same_line(&self, end: u32, start: u32) -> bool {
        let (end, start) = (end as usize, start as usize);
        if end > start || end > self.src.len() || start > self.src.len() {
            return true;
        }
        !self.src[end..start].contains('\n')
    }

    fn to_comment(tok: &Token) -> Comment {
        let TokenKind::Comment(text, style) = &tok.kind else { unreachable!("to_comment called on a non-comment token") };
        Comment { text: text.clone(), block: matches!(style, CommentStyle::Block), span: tok.span }
    }

    /// If the very next raw token is a comment on the same line as
    /// `prev_end`, attaches it as `items.last_mut()`'s trailing comment.
    fn attach_trailing(&mut self, items: &mut [Item]) {
        let Some(last) = items.last_mut() else { return };
        if last.trivia.trailing.is_some() {
            return;
        }
        if self.pos > self.last_index() {
            return;
        }
        let tok = &self.tokens[self.pos];
        if tok.kind.is_comment() && self.same_line(last.span.end, tok.span.start) {
            last.trivia.trailing = Some(Self::to_comment(tok));
            self.pos += 1;
        }
    }

    /// Consumes every consecutive raw comment token from the current
    /// position, to be attached as the next item's leading comments.
    fn collect_leading(&mut self) -> Vec<Comment> {
        let mut out = Vec::new();
        while self.pos <= self.last_index() && self.tokens[self.pos].kind.is_comment() {
            out.push(Self::to_comment(&self.tokens[self.pos]));
            self.pos += 1;
        }
        out
    }

    // -- top level --------------------------------------------------------

    fn parse_document(&mut self) -> Result<Document, SyntaxError> {
        let mut items = Vec::new();
        loop {
            self.attach_trailing(&mut items);
            let leading = self.collect_leading();
            if matches!(self.peek().kind, TokenKind::Eof) {
                return Ok(Document { items, trailing: leading });
            }
            items.push(self.parse_item(leading)?);
        }
    }

    /// Parses items until one of `stop` is the current token, returning
    /// the items and any comments trailing the last one (dangling
    /// comments right before the closing keyword/brace).
    fn parse_block(&mut self, stop: &[TokenKind]) -> Result<(Vec<Item>, Vec<Comment>), SyntaxError> {
        let mut items = Vec::new();
        loop {
            self.attach_trailing(&mut items);
            let leading = self.collect_leading();
            if stop.iter().any(|k| self.check(k)) || matches!(self.peek().kind, TokenKind::Eof) {
                return Ok((items, leading));
            }
            items.push(self.parse_item(leading)?);
        }
    }

    fn parse_item(&mut self, leading: Vec<Comment>) -> Result<Item, SyntaxError> {
        let start = self.current_span();
        let kind = match &self.peek().kind {
            TokenKind::At => self.parse_vardef()?,
            TokenKind::If => self.parse_if()?,
            TokenKind::For => self.parse_for()?,
            TokenKind::While => self.parse_while()?,
            TokenKind::Local => self.parse_local()?,
            TokenKind::Return => self.parse_return()?,
            TokenKind::Fn => self.parse_fndef()?,
            TokenKind::Script => self.parse_script()?,
            TokenKind::Include => self.parse_include()?,
            TokenKind::BareWord(_) => self.parse_leaf_node_or_call()?,
            TokenKind::Eof => {
                return Err(SyntaxError::UnexpectedEof { context: "a node, leaf, or statement".into(), span: start })
            }
            other => {
                return Err(SyntaxError::UnexpectedToken {
                    expected: "a node, leaf, or statement".into(),
                    found: other.describe(),
                    span: start,
                })
            }
        };
        let end = self.tokens[self.pos.saturating_sub(1).min(self.last_index())].span;
        let span = start.join(end);
        Ok(Item { kind, span, trivia: Trivia { leading, trailing: None } })
    }

    // -- statements ---------------------------------------------------------

    fn parse_vardef(&mut self) -> Result<ItemKind, SyntaxError> {
        self.bump(); // @
        let (name, name_span) = self.expect_name("a variable name after `@`")?;
        self.expect(TokenKind::Eq, "`=` after the variable name")?;
        let value = self.parse_value()?;
        Ok(ItemKind::VarDef(VarDef { name, name_span, value }))
    }

    fn parse_if(&mut self) -> Result<ItemKind, SyntaxError> {
        self.bump(); // if
        let cond = self.parse_condition()?;
        self.expect_then()?;
        let (then_body, _) = self.parse_block(&[TokenKind::Elseif, TokenKind::Else, TokenKind::End])?;
        let mut elifs = Vec::new();
        while self.check(&TokenKind::Elseif) {
            self.bump();
            let c = self.parse_condition()?;
            self.expect_then()?;
            let (body, _) = self.parse_block(&[TokenKind::Elseif, TokenKind::Else, TokenKind::End])?;
            elifs.push((c, body));
        }
        let else_body = if self.check(&TokenKind::Else) {
            self.bump();
            Some(self.parse_block(&[TokenKind::End])?.0)
        } else {
            None
        };
        self.expect(TokenKind::End, "`end`")?;
        Ok(ItemKind::If(IfStmt { cond, then_body, elifs, else_body }))
    }

    /// `then` isn't its own token kind (it behaves like a keyword only
    /// here); accepted as a bare word so it doesn't need a dedicated
    /// `TokenKind` variant used nowhere else in the grammar.
    fn expect_then(&mut self) -> Result<(), SyntaxError> {
        match &self.peek().kind {
            TokenKind::BareWord(w) if w == "then" => {
                self.bump();
                Ok(())
            }
            TokenKind::Eof => Err(SyntaxError::UnexpectedEof { context: "`then`".into(), span: self.current_span() }),
            other => Err(SyntaxError::UnexpectedToken {
                expected: "`then`".into(),
                found: other.describe(),
                span: self.current_span(),
            }),
        }
    }

    fn parse_for(&mut self) -> Result<ItemKind, SyntaxError> {
        self.bump(); // for
        let (var, var_span) = self.expect_name("a loop variable name")?;
        self.expect(TokenKind::Eq, "`=` after the loop variable")?;
        let start = self.parse_condition()?;
        self.expect(TokenKind::Comma, "`,` between the loop bounds")?;
        let stop = self.parse_condition()?;
        let step = if self.check(&TokenKind::Comma) {
            self.bump();
            Some(self.parse_condition()?)
        } else {
            None
        };
        self.expect(TokenKind::Do, "`do`")?;
        let (body, _) = self.parse_block(&[TokenKind::End])?;
        self.expect(TokenKind::End, "`end`")?;
        Ok(ItemKind::For(ForStmt { var, var_span, start, stop, step, body }))
    }

    fn parse_while(&mut self) -> Result<ItemKind, SyntaxError> {
        self.bump(); // while
        let cond = self.parse_condition()?;
        self.expect(TokenKind::Do, "`do`")?;
        let (body, _) = self.parse_block(&[TokenKind::End])?;
        self.expect(TokenKind::End, "`end`")?;
        Ok(ItemKind::While(WhileStmt { cond, body }))
    }

    fn parse_local(&mut self) -> Result<ItemKind, SyntaxError> {
        self.bump(); // local
        let (name, name_span) = self.expect_name("a local variable name")?;
        self.expect(TokenKind::Eq, "`=` after the local variable name")?;
        let value = self.parse_value()?;
        Ok(ItemKind::Local(LocalStmt { name, name_span, value }))
    }

    fn parse_return(&mut self) -> Result<ItemKind, SyntaxError> {
        self.bump(); // return
        let ends_statement = matches!(self.peek().kind, TokenKind::End | TokenKind::Else | TokenKind::Elseif | TokenKind::Eof);
        let value = if ends_statement { None } else { Some(self.parse_value()?) };
        Ok(ItemKind::Return(ReturnStmt { value }))
    }

    fn parse_fndef(&mut self) -> Result<ItemKind, SyntaxError> {
        self.bump(); // fn
        let (name, name_span) = self.expect_name("a function name")?;
        self.expect(TokenKind::LParen, "`(` after the function name")?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                let (p, _) = self.expect_name("a parameter name")?;
                params.push(p);
                if self.check(&TokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen, "`)` after the parameter list")?;
        self.expect(TokenKind::LBrace, "`{` to start the function body")?;
        let (body, _) = self.parse_block(&[TokenKind::RBrace])?;
        self.expect(TokenKind::RBrace, "`}` to close the function body")?;
        Ok(ItemKind::FnDef(FnDef { name, name_span, params, body }))
    }

    /// `script { ... }`: the body is captured as a raw source slice
    /// between the braces, never tokenized as Scorium syntax. Brace
    /// depth is tracked through the *already-lexed* token stream (which
    /// still balances real `{`/`}` even inside Lua code, since comments
    /// and quoted strings -- including Lua's `'...'` -- are consumed as
    /// single tokens by the lexer) so this doesn't need a second lexer.
    fn parse_script(&mut self) -> Result<ItemKind, SyntaxError> {
        let script_start = self.current_span();
        self.bump(); // script
        self.expect(TokenKind::LBrace, "`{` to start the script body")?;
        let body_start = self.tokens[self.pos].span.start;
        let mut depth = 1usize;
        let inner_end;
        loop {
            let tok = self.tokens.get(self.pos).cloned();
            match tok {
                None | Some(Token { kind: TokenKind::Eof, .. }) => {
                    return Err(SyntaxError::UnexpectedEof {
                        context: "`}` to close the `script` body".into(),
                        span: script_start,
                    })
                }
                Some(t) => {
                    match t.kind {
                        TokenKind::LBrace => depth += 1,
                        TokenKind::RBrace => {
                            depth -= 1;
                            if depth == 0 {
                                inner_end = t.span.start;
                                self.pos += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    self.pos += 1;
                }
            }
        }
        let raw = self.src[body_start as usize..inner_end as usize].to_string();
        Ok(ItemKind::Script(ScriptBlock { raw, inner_span: Span::new(body_start, inner_end) }))
    }

    fn parse_include(&mut self) -> Result<ItemKind, SyntaxError> {
        self.bump(); // include
        let path_span = self.current_span();
        let path = self.parse_str_value("an include path")?;
        Ok(ItemKind::Include(IncludeStmt { path, path_span }))
    }

    /// A bare identifier starts either a node (`name { }` / `name header
    /// { }`), a leaf (`name = value`), or a call statement (`name(args)`,
    /// e.g. invoking a `fn`).
    fn parse_leaf_node_or_call(&mut self) -> Result<ItemKind, SyntaxError> {
        let (name, name_span) = self.expect_name("a node or leaf name")?;
        match &self.peek().kind {
            TokenKind::LBrace => {
                self.bump();
                let (body, _) = self.parse_block(&[TokenKind::RBrace])?;
                let close = self.expect(TokenKind::RBrace, "`}` to close the node body")?;
                Ok(ItemKind::Node(NodeDecl {
                    name,
                    name_span,
                    header: None,
                    header_span: None,
                    body,
                    body_span: name_span.join(close.span),
                }))
            }
            TokenKind::Eq => {
                self.bump();
                let value = self.parse_value()?;
                Ok(ItemKind::Leaf(LeafDecl { key: name, key_span: name_span, value }))
            }
            TokenKind::LParen => {
                let callee = Expr::new(ExprKind::Ident(name), name_span);
                let call = self.parse_call_from(callee)?;
                Ok(ItemKind::Call(call))
            }
            TokenKind::BareWord(_) | TokenKind::QuotedString(_) if matches!(self.peek2().kind, TokenKind::LBrace) => {
                let header_tok = self.bump();
                let header = match header_tok.kind {
                    TokenKind::BareWord(w) => HeaderValue::Bare(w),
                    TokenKind::QuotedString(s) => HeaderValue::Quoted(s),
                    _ => unreachable!(),
                };
                self.bump(); // {
                let (body, _) = self.parse_block(&[TokenKind::RBrace])?;
                let close = self.expect(TokenKind::RBrace, "`}` to close the node body")?;
                Ok(ItemKind::Node(NodeDecl {
                    name,
                    name_span,
                    header: Some(header),
                    header_span: Some(header_tok.span),
                    body,
                    body_span: name_span.join(close.span),
                }))
            }
            TokenKind::Eof => Err(SyntaxError::UnexpectedEof {
                context: "`{` (a node), `=` (a leaf), or `(` (a call) after this name".into(),
                span: self.current_span(),
            }),
            other => Err(SyntaxError::UnexpectedToken {
                expected: "`{` (a node), `=` (a leaf), or `(` (a call)".into(),
                found: other.describe(),
                span: self.current_span(),
            }),
        }
    }

    // -- values & expressions ------------------------------------------------

    /// Parses a value position: a leaf's value, a list element, or a call
    /// argument. Single bare token with no continuation -> a plain value
    /// (string/number/etc, `$name` allowed); anything else -> a full
    /// expression (`$name` and `@name` are errors there). See
    /// [`Self::parse_condition`] for why conditions/loop bounds differ.
    fn parse_value(&mut self) -> Result<Expr, SyntaxError> {
        if !self.peek2().kind.continues_expression() {
            if let Some(expr) = self.try_parse_single_token_value()? {
                return Ok(expr);
            }
        }
        self.parse_expr_bp(0)
    }

    /// `if`/`while` conditions and `for` loop bounds are computational
    /// positions, not declarative data: a bare word there means "look up
    /// this variable" (`if debug then`), not "the literal string
    /// `debug`". So, unlike [`Self::parse_value`], this always goes
    /// through full expression parsing rather than the single-token
    /// bare-string fast path.
    fn parse_condition(&mut self) -> Result<Expr, SyntaxError> {
        self.parse_expr_bp(0)
    }

    fn try_parse_single_token_value(&mut self) -> Result<Option<Expr>, SyntaxError> {
        let tok = self.peek().clone();
        let expr = match &tok.kind {
            TokenKind::Int(n) => Expr::new(ExprKind::Int(*n), tok.span),
            TokenKind::Float(n) => Expr::new(ExprKind::Float(*n), tok.span),
            TokenKind::True => Expr::new(ExprKind::Bool(true), tok.span),
            TokenKind::False => Expr::new(ExprKind::Bool(false), tok.span),
            TokenKind::Nil => Expr::new(ExprKind::Nil, tok.span),
            TokenKind::ColorLit(hex) => Expr::new(ExprKind::Color(hex.clone()), tok.span),
            TokenKind::DurationLit(n, u) => Expr::new(ExprKind::Duration(*n, u.clone()), tok.span),
            TokenKind::QuotedString(s) => Expr::new(ExprKind::Str(StrLit::Quoted(s.clone())), tok.span),
            TokenKind::BareWord(w) => classify_bare_word(w, tok.span, false)?,
            TokenKind::LBracket => return Ok(Some(self.parse_list()?)),
            _ => return Ok(None),
        };
        self.bump();
        Ok(Some(expr))
    }

    fn parse_list(&mut self) -> Result<Expr, SyntaxError> {
        let open = self.expect(TokenKind::LBracket, "`[`")?;
        let mut items = Vec::new();
        if !self.check(&TokenKind::RBracket) {
            loop {
                items.push(self.parse_value()?);
                if self.check(&TokenKind::Comma) {
                    self.bump();
                    if self.check(&TokenKind::RBracket) {
                        break; // trailing comma
                    }
                } else {
                    break;
                }
            }
        }
        let close = self.expect(TokenKind::RBracket, "`]` to close the list")?;
        Ok(Expr::new(ExprKind::List(items), open.span.join(close.span)))
    }

    fn parse_str_value(&mut self, context: &str) -> Result<StrLit, SyntaxError> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::QuotedString(s) => {
                self.bump();
                Ok(StrLit::Quoted(s))
            }
            TokenKind::BareWord(w) => {
                self.bump();
                Ok(parse_interpolated(&w, tok.span))
            }
            TokenKind::Eof => Err(SyntaxError::UnexpectedEof { context: context.to_string(), span: tok.span }),
            other => Err(SyntaxError::UnexpectedToken { expected: context.to_string(), found: other.describe(), span: tok.span }),
        }
    }

    fn binop(kind: &TokenKind) -> Option<(BinOp, u8, u8)> {
        Some(match kind {
            TokenKind::Or => (BinOp::Or, 1, 2),
            TokenKind::And => (BinOp::And, 3, 4),
            TokenKind::EqEq => (BinOp::Eq, 5, 6),
            TokenKind::NotEq => (BinOp::NotEq, 5, 6),
            TokenKind::Lt => (BinOp::Lt, 5, 6),
            TokenKind::Gt => (BinOp::Gt, 5, 6),
            TokenKind::LtEq => (BinOp::LtEq, 5, 6),
            TokenKind::GtEq => (BinOp::GtEq, 5, 6),
            TokenKind::Plus => (BinOp::Add, 7, 8),
            TokenKind::Minus => (BinOp::Sub, 7, 8),
            TokenKind::Star => (BinOp::Mul, 9, 10),
            TokenKind::Slash => (BinOp::Div, 9, 10),
            TokenKind::Percent => (BinOp::Mod, 9, 10),
            _ => return None,
        })
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Result<Expr, SyntaxError> {
        let mut lhs = self.parse_unary()?;
        while let Some((op, l_bp, r_bp)) = Self::binop(&self.peek().kind) {
            if l_bp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.parse_expr_bp(r_bp)?;
            let span = lhs.span.join(rhs.span);
            lhs = Expr::new(ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), span);
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, SyntaxError> {
        match &self.peek().kind {
            TokenKind::Minus => {
                let t = self.bump();
                let operand = self.parse_unary()?;
                let span = t.span.join(operand.span);
                Ok(Expr::new(ExprKind::Unary(UnOp::Neg, Box::new(operand)), span))
            }
            TokenKind::Not => {
                let t = self.bump();
                let operand = self.parse_unary()?;
                let span = t.span.join(operand.span);
                Ok(Expr::new(ExprKind::Unary(UnOp::Not, Box::new(operand)), span))
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, SyntaxError> {
        let mut expr = self.parse_primary()?;
        while self.check(&TokenKind::LParen) {
            expr = self.parse_call_from(expr)?;
        }
        Ok(expr)
    }

    fn parse_call_from(&mut self, callee: Expr) -> Result<Expr, SyntaxError> {
        let open = self.expect(TokenKind::LParen, "`(`")?;
        let mut args = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                args.push(self.parse_value()?);
                if self.check(&TokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let close = self.expect(TokenKind::RParen, "`)` to close the argument list")?;
        let _ = open;
        let span = callee.span.join(close.span);
        Ok(Expr::new(ExprKind::Call(Box::new(callee), args), span))
    }

    fn parse_primary(&mut self) -> Result<Expr, SyntaxError> {
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::Int(n) => {
                self.bump();
                Ok(Expr::new(ExprKind::Int(*n), tok.span))
            }
            TokenKind::Float(n) => {
                self.bump();
                Ok(Expr::new(ExprKind::Float(*n), tok.span))
            }
            TokenKind::True => {
                self.bump();
                Ok(Expr::new(ExprKind::Bool(true), tok.span))
            }
            TokenKind::False => {
                self.bump();
                Ok(Expr::new(ExprKind::Bool(false), tok.span))
            }
            TokenKind::Nil => {
                self.bump();
                Ok(Expr::new(ExprKind::Nil, tok.span))
            }
            TokenKind::ColorLit(hex) => {
                self.bump();
                Ok(Expr::new(ExprKind::Color(hex.clone()), tok.span))
            }
            TokenKind::DurationLit(n, u) => {
                let (n, u) = (*n, u.clone());
                self.bump();
                Ok(Expr::new(ExprKind::Duration(n, u), tok.span))
            }
            TokenKind::QuotedString(s) => {
                let s = s.clone();
                self.bump();
                Ok(Expr::new(ExprKind::Str(StrLit::Quoted(s)), tok.span))
            }
            TokenKind::LBracket => self.parse_list(),
            TokenKind::LParen => {
                self.bump();
                let inner = self.parse_expr_bp(0)?;
                self.expect(TokenKind::RParen, "`)` to close the parenthesized expression")?;
                Ok(inner)
            }
            TokenKind::At => {
                self.bump();
                let (name, _) = self.expect_name("a variable name")?;
                Err(SyntaxError::AtInExpression { name, span: tok.span })
            }
            TokenKind::BareWord(w) => {
                let expr = classify_bare_word(w, tok.span, true)?;
                self.bump();
                Ok(expr)
            }
            TokenKind::Eof => Err(SyntaxError::UnexpectedEof { context: "an expression".into(), span: tok.span }),
            other => {
                Err(SyntaxError::UnexpectedToken { expected: "an expression".into(), found: other.describe(), span: tok.span })
            }
        }
    }
}

/// Classifies a bare-word token into an expression: `$`-containing words
/// interpolate into a string; a word with a dot splits into a member
/// access (`primary.darken` -> `Member(Ident("primary"), "darken")` --
/// whether `primary` is a real binding or this should fall back to the
/// literal string `"primary.darken"` is decided at evaluation time, not
/// here, since the parser can't know what's in scope); anything else is
/// a plain identifier, resolved at evaluation time as a known
/// variable/parameter/loop-var or, failing that, a literal string.
///
/// `in_expression` controls what a *pure* `$name` token (nothing else in
/// the word) does: inside a real expression that's the documented error
/// (`$name` cannot be used in an expression); in a single-token value
/// position it's valid interpolation, same as `$name` mixed with other
/// text (`$mod+Return`) always is.
fn classify_bare_word(word: &str, span: Span, in_expression: bool) -> Result<Expr, SyntaxError> {
    if let Some(name) = word.strip_prefix('$') {
        if in_expression && !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(SyntaxError::DollarInExpression { name: name.to_string(), span });
        }
    }
    if word.contains('$') {
        return Ok(Expr::new(ExprKind::Str(parse_interpolated(word, span)), span));
    }
    if let Some((base, field)) = word.split_once('.') {
        let base_expr = Expr::new(ExprKind::Ident(base.to_string()), span);
        return Ok(Expr::new(ExprKind::Member(Box::new(base_expr), field.to_string(), span), span));
    }
    Ok(Expr::new(ExprKind::Ident(word.to_string()), span))
}

/// Splits a bare word's text into literal and `$name` interpolation
/// parts. Each `$` starts an identifier run (alphanumeric/underscore);
/// anything else is literal text, copied through unchanged.
fn parse_interpolated(text: &str, base_span: Span) -> StrLit {
    let mut parts = Vec::new();
    let mut lit = String::new();
    let bytes_base = base_span.start;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '$' {
            let rest = &text[i + 1..];
            let name_len = rest.find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(rest.len());
            if name_len == 0 {
                lit.push('$');
                continue;
            }
            if !lit.is_empty() {
                parts.push(StrPart::Lit(std::mem::take(&mut lit)));
            }
            let name = &rest[..name_len];
            let start = bytes_base + i as u32 + 1;
            let span = Span::new(start, start + name_len as u32);
            parts.push(StrPart::Interp(name.to_string(), span));
            for _ in 0..name_len {
                chars.next();
            }
        } else {
            lit.push(c);
        }
    }
    if !lit.is_empty() || parts.is_empty() {
        parts.push(StrPart::Lit(lit));
    }
    StrLit::Bare(parts)
}
