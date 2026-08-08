use scorium_core::ast::*;

#[derive(Debug, Clone)]
pub struct FormatOptions {
    pub indent_width: usize,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self { indent_width: 4 }
    }
}

pub fn format(doc: &Document) -> String {
    format_with(doc, &FormatOptions::default())
}

pub fn format_with(doc: &Document, opts: &FormatOptions) -> String {
    let mut printer = Printer { opts, out: String::new() };
    printer.print_items(&doc.items, 0);
    printer.print_comments(&doc.trailing, 0);
    while printer.out.ends_with("\n\n") {
        printer.out.pop();
    }
    if !printer.out.is_empty() && !printer.out.ends_with('\n') {
        printer.out.push('\n');
    }
    printer.out
}

struct Printer<'a> {
    opts: &'a FormatOptions,
    out: String,
}

impl Printer<'_> {
    fn indent(&mut self, depth: usize) {
        self.out.push_str(&" ".repeat(depth * self.opts.indent_width));
    }

    fn print_items(&mut self, items: &[Item], depth: usize) {
        for item in items {
            if item.trivia.blank_line_before {
                self.out.push('\n');
            }
            self.print_comments(&item.trivia.leading, depth);
            self.indent(depth);
            self.print_item_kind(&item.kind, depth);
            if let Some(trailing) = &item.trivia.trailing {
                self.out.push(' ');
                self.out.push_str(&render_comment(trailing));
            }
            self.out.push('\n');
        }
    }

    fn print_comments(&mut self, comments: &[Comment], depth: usize) {
        for c in comments {
            self.indent(depth);
            self.out.push_str(&render_comment(c));
            self.out.push('\n');
        }
    }

    fn print_item_kind(&mut self, kind: &ItemKind, depth: usize) {
        match kind {
            ItemKind::Leaf(leaf) => {
                self.out.push_str(&leaf.key);
                self.out.push_str(" = ");
                self.print_expr(&leaf.value);
            }
            ItemKind::Node(node) => {
                self.out.push_str(&node.name);
                if let Some(header) = &node.header {
                    self.out.push(' ');
                    self.print_header(header);
                }
                self.out.push_str(" {\n");
                self.print_items(&node.body, depth + 1);
                self.indent(depth);
                self.out.push('}');
            }
            ItemKind::VarDef(v) => {
                self.out.push('@');
                self.out.push_str(&v.name);
                self.out.push_str(" = ");
                self.print_expr(&v.value);
            }
            ItemKind::Include(inc) => {
                self.out.push_str("include ");
                self.print_strlit(&inc.path);
            }
            ItemKind::If(stmt) => {
                self.out.push_str("if ");
                self.print_expr(&stmt.cond);
                self.out.push_str(" then\n");
                self.print_items(&stmt.then_body, depth + 1);
                for (cond, body) in &stmt.elifs {
                    self.indent(depth);
                    self.out.push_str("elseif ");
                    self.print_expr(cond);
                    self.out.push_str(" then\n");
                    self.print_items(body, depth + 1);
                }
                if let Some(body) = &stmt.else_body {
                    self.indent(depth);
                    self.out.push_str("else\n");
                    self.print_items(body, depth + 1);
                }
                self.indent(depth);
                self.out.push_str("end");
            }
            ItemKind::For(stmt) => {
                self.out.push_str("for ");
                self.out.push_str(&stmt.var);
                self.out.push_str(" = ");
                self.print_expr(&stmt.start);
                self.out.push_str(", ");
                self.print_expr(&stmt.stop);
                if let Some(step) = &stmt.step {
                    self.out.push_str(", ");
                    self.print_expr(step);
                }
                self.out.push_str(" do\n");
                self.print_items(&stmt.body, depth + 1);
                self.indent(depth);
                self.out.push_str("end");
            }
            ItemKind::While(stmt) => {
                self.out.push_str("while ");
                self.print_expr(&stmt.cond);
                self.out.push_str(" do\n");
                self.print_items(&stmt.body, depth + 1);
                self.indent(depth);
                self.out.push_str("end");
            }
            ItemKind::Local(l) => {
                self.out.push_str("local ");
                self.out.push_str(&l.name);
                self.out.push_str(" = ");
                self.print_expr(&l.value);
            }
            ItemKind::Return(r) => {
                self.out.push_str("return");
                if let Some(v) = &r.value {
                    self.out.push(' ');
                    self.print_expr(v);
                }
            }
            ItemKind::FnDef(f) => {
                self.out.push_str("fn ");
                self.out.push_str(&f.name);
                self.out.push('(');
                self.out.push_str(&f.params.join(", "));
                self.out.push_str(") {\n");
                self.print_items(&f.body, depth + 1);
                self.indent(depth);
                self.out.push('}');
            }
            ItemKind::Script(s) => {
                self.out.push_str("script {");
                self.out.push_str(&s.raw);
                self.out.push('}');
            }
            ItemKind::Call(expr) => self.print_expr(expr),
        }
    }

    fn print_header(&mut self, header: &HeaderValue) {
        match header {
            HeaderValue::Bare(s) => self.out.push_str(s),
            HeaderValue::Quoted(s) => {
                self.out.push('"');
                self.out.push_str(&escape_str(s));
                self.out.push('"');
            }
        }
    }

    fn print_strlit(&mut self, lit: &StrLit) {
        match lit {
            StrLit::Quoted(s) => {
                self.out.push('"');
                self.out.push_str(&escape_str(s));
                self.out.push('"');
            }
            StrLit::Bare(parts) => {
                for part in parts {
                    match part {
                        StrPart::Lit(s) => self.out.push_str(s),
                        StrPart::Interp(name, _) => {
                            self.out.push('$');
                            self.out.push_str(name);
                        }
                    }
                }
            }
        }
    }

    fn print_expr(&mut self, expr: &Expr) {
        self.print_expr_prec(expr, 0);
    }

    /// Prints an expression with only the parentheses needed to preserve
    /// the AST's precedence and associativity. The parser deliberately
    /// drops grouping nodes, so ignoring precedence here can turn
    /// `(a + b) * c` into the meaningfully different `a + b * c`.
    fn print_expr_prec(&mut self, expr: &Expr, min_prec: u8) {
        let prec = expr_precedence(expr);
        let parenthesize = prec < min_prec;
        if parenthesize {
            self.out.push('(');
        }
        match &expr.kind {
            ExprKind::Int(n) => self.out.push_str(&n.to_string()),
            ExprKind::Float(n) => self.out.push_str(&format_float(*n)),
            ExprKind::Bool(b) => self.out.push_str(if *b { "true" } else { "false" }),
            ExprKind::Nil => self.out.push_str("nil"),
            ExprKind::Color(hex) => {
                self.out.push('#');
                self.out.push_str(hex);
            }
            ExprKind::Duration(n, unit) => {
                self.out.push_str(&format_duration_amount(*n));
                self.out.push_str(unit);
            }
            ExprKind::Str(lit) => self.print_strlit(lit),
            ExprKind::List(items) => {
                self.out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.print_expr_prec(item, 0);
                }
                self.out.push(']');
            }
            ExprKind::Ident(name) => self.out.push_str(name),
            ExprKind::Unary(op, operand) => {
                match op {
                    UnOp::Neg => self.out.push('-'),
                    UnOp::Not => self.out.push_str("not "),
                }
                self.print_expr_prec(operand, PREC_UNARY);
            }
            ExprKind::Binary(op, l, r) => {
                let op_prec = binop_precedence(*op);
                self.print_expr_prec(l, op_prec);
                self.out.push(' ');
                self.out.push_str(binop_symbol(*op));
                self.out.push(' ');
                // All binary operators are left-associative. Requiring a
                // strictly higher precedence on the right preserves an
                // explicitly grouped right child such as `a - (b - c)`.
                self.print_expr_prec(r, op_prec + 1);
            }
            ExprKind::Call(callee, args) => {
                self.print_expr_prec(callee, PREC_POSTFIX);
                self.out.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.print_expr_prec(arg, 0);
                }
                self.out.push(')');
            }
            ExprKind::Member(base, field, _) => {
                self.print_expr_prec(base, PREC_POSTFIX);
                self.out.push('.');
                self.out.push_str(field);
            }
        }
        if parenthesize {
            self.out.push(')');
        }
    }
}

const PREC_OR: u8 = 1;
const PREC_AND: u8 = 3;
const PREC_COMPARE: u8 = 5;
const PREC_ADD: u8 = 7;
const PREC_MUL: u8 = 9;
const PREC_UNARY: u8 = 11;
const PREC_POSTFIX: u8 = 13;
const PREC_PRIMARY: u8 = 15;

fn expr_precedence(expr: &Expr) -> u8 {
    match &expr.kind {
        ExprKind::Binary(op, _, _) => binop_precedence(*op),
        ExprKind::Unary(_, _) => PREC_UNARY,
        ExprKind::Call(_, _) | ExprKind::Member(_, _, _) => PREC_POSTFIX,
        _ => PREC_PRIMARY,
    }
}

fn binop_precedence(op: BinOp) -> u8 {
    match op {
        BinOp::Or => PREC_OR,
        BinOp::And => PREC_AND,
        BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => PREC_COMPARE,
        BinOp::Add | BinOp::Sub => PREC_ADD,
        BinOp::Mul | BinOp::Div | BinOp::Mod => PREC_MUL,
    }
}

fn render_comment(c: &Comment) -> String {
    if c.block {
        format!("--[[{}]]", c.text)
    } else {
        format!("# {}", c.text.trim())
    }
}

fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

/// Floats always print with a decimal point (`1.0`, not `1`) so
/// re-lexing produces a `Float` again rather than an `Int` -- printing
/// `1` for `1.0` would be a silent type change, not just a style choice.
fn format_float(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 {
        format!("{n:.1}")
    } else {
        n.to_string()
    }
}

/// Durations don't have this problem: `600` and `600.0` both re-lex as
/// the same `DurationLit`, so the shorter form is fine.
fn format_duration_amount(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

fn binop_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::NotEq => "~=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::LtEq => "<=",
        BinOp::GtEq => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
    }
}
