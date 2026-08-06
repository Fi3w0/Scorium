//! The statement interpreter and expression evaluator.
//!
//! Control flow (`if`/`for`/`while`/`fn`/`local`/`return`) is interpreted
//! directly in Rust, walking the AST -- not transpiled to Lua source and
//! not run through the Lua VM. Only expression *evaluation* mixes in
//! Lua-like semantics (Lua truthiness, `and`/`or` returning an operand
//! rather than a bool), and only `script { }` bodies run as real Lua,
//! through the sandboxed state on [`crate::Runtime`]. This keeps spans
//! and error messages precise (no text round-trip through a second
//! grammar) at the cost of re-implementing Lua's statement semantics in
//! Rust; see `docs/EMBEDDING.md` for the full rationale.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use scorium_core::ast::*;
use scorium_core::entry::{Entry, HostCallEntry, IncludeEntry, LeafEntry, NodeEntry};
use scorium_core::{ColorValue, DurationUnit, DurationValue, Source, Span, Value};

use crate::error::{EvalError, EvalErrorKind};
use crate::registry::HostFunction;
use crate::runtime::Runtime;
use crate::scope::Scope;
use crate::value_bridge;

pub(crate) enum Flow {
    Normal,
    Return(Option<Value>),
}

pub(crate) struct Evaluator<'rt> {
    runtime: &'rt Runtime,
    scope: Scope,
    functions: HashMap<String, Rc<FnDef>>,
    sinks: Vec<Vec<Entry>>,
    source: Source,
    base_dir: PathBuf,
    ancestors: Vec<PathBuf>,
    loop_iterations: u64,
    warnings: Vec<String>,
}

impl<'rt> Evaluator<'rt> {
    pub(crate) fn new(runtime: &'rt Runtime, source: Source, base_dir: PathBuf) -> Self {
        Self {
            runtime,
            scope: Scope::new(),
            functions: HashMap::new(),
            sinks: vec![Vec::new()],
            source,
            base_dir,
            ancestors: Vec::new(),
            loop_iterations: 0,
            warnings: Vec::new(),
        }
    }

    pub(crate) fn run(mut self, doc: &Document) -> Result<(Vec<Entry>, Vec<String>), EvalError> {
        self.exec_items(&doc.items)?;
        Ok((self.sinks.pop().unwrap_or_default(), self.warnings))
    }

    fn err(&self, kind: EvalErrorKind) -> EvalError {
        EvalError::new(kind, self.source.clone())
    }

    fn emit(&mut self, entry: Entry) {
        self.sinks.last_mut().expect("evaluate() always keeps a root sink").push(entry);
    }

    // -- statements -----------------------------------------------------

    fn exec_items(&mut self, items: &[Item]) -> Result<Flow, EvalError> {
        for item in items {
            match self.exec_item(item)? {
                Flow::Normal => {}
                flow => return Ok(flow),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_item(&mut self, item: &Item) -> Result<Flow, EvalError> {
        match &item.kind {
            ItemKind::Leaf(leaf) => {
                let value = self.eval_expr(&leaf.value)?;
                // `key = value` is overloaded: declaratively it's a leaf,
                // but it's also the only spelling Scorium has for
                // updating a `local` (there's no separate reassignment
                // statement). If `key` is already a lexical local, this
                // updates it in place instead of emitting an entry --
                // without this, a `while` loop counter could never
                // advance. See docs/GRAMMAR.md.
                if self.scope.is_reassignable_local(&leaf.key) {
                    self.scope.reassign_local(&leaf.key, value);
                } else {
                    self.emit(Entry::Leaf(LeafEntry { key: leaf.key.clone(), key_span: leaf.key_span, value, span: item.span }));
                }
                Ok(Flow::Normal)
            }
            ItemKind::Node(node) => self.exec_node(node, item.span),
            ItemKind::VarDef(v) => {
                let val = self.eval_expr(&v.value)?;
                self.scope.set_vardef(v.name.clone(), val);
                Ok(Flow::Normal)
            }
            ItemKind::Local(l) => {
                let val = self.eval_expr(&l.value)?;
                self.scope.set_local(l.name.clone(), val);
                Ok(Flow::Normal)
            }
            ItemKind::If(stmt) => self.exec_if(stmt),
            ItemKind::For(stmt) => self.exec_for(stmt, item.span),
            ItemKind::While(stmt) => self.exec_while(stmt, item.span),
            ItemKind::FnDef(f) => {
                self.functions.insert(f.name.clone(), Rc::new(f.clone()));
                Ok(Flow::Normal)
            }
            ItemKind::Return(r) => {
                let v = match &r.value {
                    Some(e) => Some(self.eval_expr(e)?),
                    None => None,
                };
                Ok(Flow::Return(v))
            }
            ItemKind::Script(s) => {
                self.exec_script(s, item.span)?;
                Ok(Flow::Normal)
            }
            ItemKind::Include(inc) => {
                self.exec_include(inc, item.span)?;
                Ok(Flow::Normal)
            }
            ItemKind::Call(expr) => self.exec_call_stmt(expr, item.span),
        }
    }

    fn exec_node(&mut self, node: &NodeDecl, span: Span) -> Result<Flow, EvalError> {
        let header = node.header.as_ref().map(|h| match h {
            HeaderValue::Bare(s) | HeaderValue::Quoted(s) => s.clone(),
        });
        self.sinks.push(Vec::new());
        self.scope.enter_node_body();
        self.scope.push();
        let result = self.exec_items(&node.body);
        self.scope.pop();
        self.scope.exit_node_body();
        let children = self.sinks.pop().unwrap_or_default();
        result?;
        self.emit(Entry::Node(NodeEntry {
            name: node.name.clone(),
            name_span: node.name_span,
            header,
            header_span: node.header_span,
            children,
            span,
        }));
        Ok(Flow::Normal)
    }

    fn exec_if(&mut self, stmt: &IfStmt) -> Result<Flow, EvalError> {
        if self.eval_expr(&stmt.cond)?.is_truthy() {
            return self.exec_scoped(&stmt.then_body);
        }
        for (cond, body) in &stmt.elifs {
            if self.eval_expr(cond)?.is_truthy() {
                return self.exec_scoped(body);
            }
        }
        match &stmt.else_body {
            Some(body) => self.exec_scoped(body),
            None => Ok(Flow::Normal),
        }
    }

    fn exec_scoped(&mut self, body: &[Item]) -> Result<Flow, EvalError> {
        self.scope.push();
        let result = self.exec_items(body);
        self.scope.pop();
        result
    }

    fn exec_for(&mut self, stmt: &ForStmt, span: Span) -> Result<Flow, EvalError> {
        let start = self.eval_number(&stmt.start)?;
        let stop = self.eval_number(&stmt.stop)?;
        let step = match &stmt.step {
            Some(e) => self.eval_number(e)?,
            None => 1.0,
        };
        if step == 0.0 {
            return Err(self.err(EvalErrorKind::TypeError { message: "a `for` step cannot be 0".into(), span }));
        }
        let mut i = start;
        loop {
            if step > 0.0 && i > stop {
                break;
            }
            if step < 0.0 && i < stop {
                break;
            }
            self.tick_budget(span)?;
            self.scope.push();
            let loop_val = if i.fract() == 0.0 { Value::Int(i as i64) } else { Value::Float(i) };
            self.scope.set_local(stmt.var.clone(), loop_val);
            let flow = self.exec_items(&stmt.body);
            self.scope.pop();
            match flow? {
                Flow::Normal => {}
                flow @ Flow::Return(_) => return Ok(flow),
            }
            i += step;
        }
        Ok(Flow::Normal)
    }

    fn exec_while(&mut self, stmt: &WhileStmt, span: Span) -> Result<Flow, EvalError> {
        while self.eval_expr(&stmt.cond)?.is_truthy() {
            self.tick_budget(span)?;
            self.scope.push();
            let flow = self.exec_items(&stmt.body);
            self.scope.pop();
            match flow? {
                Flow::Normal => {}
                flow @ Flow::Return(_) => return Ok(flow),
            }
        }
        Ok(Flow::Normal)
    }

    fn tick_budget(&mut self, span: Span) -> Result<(), EvalError> {
        self.loop_iterations += 1;
        let limit = self.runtime.options().max_loop_iterations;
        if self.loop_iterations > limit {
            return Err(self.err(EvalErrorKind::LoopBudgetExceeded { limit, span }));
        }
        Ok(())
    }

    fn exec_script(&mut self, block: &ScriptBlock, span: Span) -> Result<(), EvalError> {
        let lua = self.runtime.lua();
        let globals = lua.globals();
        for (name, value) in self.scope.all_visible(self.runtime.registry()) {
            let lv = value_bridge::to_lua(lua, &value)
                .map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
            globals.set(name, lv).map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
        }
        self.runtime.reset_script_budget();
        lua.load(&block.raw)
            .set_name("script")
            .exec()
            .map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))
    }

    fn exec_include(&mut self, inc: &IncludeStmt, span: Span) -> Result<(), EvalError> {
        let options = self.runtime.options();
        if !options.include_policy.enabled {
            return Err(self.err(EvalErrorKind::IncludesDisabled { span }));
        }
        let path_str = self.eval_strlit(&inc.path, inc.path_span)?;
        if !options.include_policy.allow_parent_traversal && path_str.contains("..") {
            return Err(self.err(EvalErrorKind::IncludePathDenied { path: path_str, span }));
        }
        let resolved = self.base_dir.join(&path_str);
        let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
        if self.ancestors.contains(&canonical) {
            let mut chain: Vec<String> = self.ancestors.iter().map(|p| p.display().to_string()).collect();
            chain.push(canonical.display().to_string());
            return Err(self.err(EvalErrorKind::IncludeCycle { chain: chain.join(" -> "), span }));
        }

        let text = std::fs::read_to_string(&resolved)
            .map_err(|e| self.err(EvalErrorKind::IncludeIo { path: path_str.clone(), message: e.to_string(), span }))?;
        let included_source = Source::new(resolved.display().to_string(), text);
        let doc = scorium_core::parse(&included_source)
            .map_err(|e| self.err(EvalErrorKind::IncludeParse { path: path_str.clone(), message: e.to_string(), span }))?;

        self.emit(Entry::Include(IncludeEntry { path: path_str, resolved_path: Some(resolved.clone()), span }));

        let new_dir = resolved.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| self.base_dir.clone());
        let prev_source = std::mem::replace(&mut self.source, included_source);
        let prev_dir = std::mem::replace(&mut self.base_dir, new_dir);
        self.ancestors.push(canonical);

        let flow = self.exec_items(&doc.items);

        self.ancestors.pop();
        self.source = prev_source;
        self.base_dir = prev_dir;

        flow?;
        Ok(())
    }

    fn exec_call_stmt(&mut self, expr: &Expr, span: Span) -> Result<Flow, EvalError> {
        if let ExprKind::Call(callee, args) = &expr.kind {
            if let ExprKind::Ident(name) = &callee.kind {
                if !self.functions.contains_key(name) {
                    let arg_vals = args.iter().map(|a| self.eval_expr(a)).collect::<Result<Vec<_>, _>>()?;
                    let result = self.call_host(name, &arg_vals, span)?;
                    self.emit(Entry::HostCall(HostCallEntry { name: name.clone(), args: arg_vals, result: Some(result), span }));
                    return Ok(Flow::Normal);
                }
            }
        }
        self.eval_expr(expr)?;
        Ok(Flow::Normal)
    }

    // -- expressions ------------------------------------------------------

    fn eval_expr(&mut self, expr: &Expr) -> Result<Value, EvalError> {
        match &expr.kind {
            ExprKind::Int(n) => Ok(Value::Int(*n)),
            ExprKind::Float(n) => Ok(Value::Float(*n)),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Nil => Ok(Value::Nil),
            ExprKind::Color(hex) => Ok(Value::Color(ColorValue::parse_hex(hex).expect("lexer only emits valid 6/8-digit hex"))),
            ExprKind::Duration(n, unit) => {
                Ok(Value::Duration(DurationValue::new(*n, DurationUnit::parse(unit).expect("lexer only emits ms/s/m units"))))
            }
            ExprKind::Str(lit) => self.eval_strlit(lit, expr.span).map(Value::Str),
            ExprKind::List(items) => {
                let vals = items.iter().map(|e| self.eval_expr(e)).collect::<Result<Vec<_>, _>>()?;
                Ok(Value::List(vals))
            }
            ExprKind::Ident(name) => Ok(self.resolve_ident(name).unwrap_or_else(|| Value::Str(name.clone()))),
            ExprKind::Unary(op, operand) => {
                let v = self.eval_expr(operand)?;
                self.eval_unary(*op, v, expr.span)
            }
            ExprKind::Binary(op, l, r) => self.eval_binary(*op, l, r, expr.span),
            ExprKind::Call(callee, args) => self.eval_call(callee, args, expr.span),
            ExprKind::Member(base, field, field_span) => self.eval_member(base, field, *field_span),
        }
    }

    /// Resolves a bare identifier to a *real* binding: a lexical
    /// variable (local/loop-var/param), an `@`-defined variable, a
    /// sibling leaf already emitted earlier in the same node body (this
    /// is what makes `deep = primary.darken(0.25)` see an earlier
    /// `primary = #8EDDFF` leaf in the same block), or a host-registered
    /// value. Returns `None` if `name` isn't bound to anything -- the
    /// caller decides what "not bound" means (a literal string for a
    /// plain `Ident`, but a different fallback for `Member`).
    fn resolve_ident(&self, name: &str) -> Option<Value> {
        self.scope.lookup(name, self.runtime.registry()).or_else(|| self.lookup_sibling_leaf(name))
    }

    /// Sibling-leaf access is scoped to the *innermost* currently-open
    /// node body only, matching how a leaf like `deep = primary.darken(...)`
    /// reads an earlier `primary` leaf in the same block -- not an
    /// ancestor block's leaves.
    fn lookup_sibling_leaf(&self, name: &str) -> Option<Value> {
        self.sinks.last()?.iter().rev().find_map(|e| match e {
            Entry::Leaf(l) if l.key == name => Some(l.value.clone()),
            _ => None,
        })
    }

    fn eval_number(&mut self, e: &Expr) -> Result<f64, EvalError> {
        let v = self.eval_expr(e)?;
        v.as_number().ok_or_else(|| {
            self.err(EvalErrorKind::TypeError { message: format!("expected a number, found {}", v.type_name()), span: e.span })
        })
    }

    fn eval_strlit(&mut self, lit: &StrLit, _span: Span) -> Result<String, EvalError> {
        match lit {
            StrLit::Quoted(s) => Ok(s.clone()),
            StrLit::Bare(parts) => {
                let mut out = String::new();
                for part in parts {
                    match part {
                        StrPart::Lit(s) => out.push_str(s),
                        StrPart::Interp(name, span) => {
                            let value = self.scope.lookup(name, self.runtime.registry()).ok_or_else(|| {
                                self.err(EvalErrorKind::UndefinedInterpolation { name: name.clone(), span: *span })
                            })?;
                            out.push_str(&value.to_string());
                        }
                    }
                }
                Ok(out)
            }
        }
    }

    fn eval_unary(&mut self, op: UnOp, v: Value, span: Span) -> Result<Value, EvalError> {
        match op {
            UnOp::Not => Ok(Value::Bool(!v.is_truthy())),
            UnOp::Neg => match v {
                Value::Int(i) => Ok(Value::Int(-i)),
                Value::Float(f) => Ok(Value::Float(-f)),
                other => {
                    Err(self.err(EvalErrorKind::TypeError { message: format!("cannot negate a {}", other.type_name()), span }))
                }
            },
        }
    }

    fn eval_binary(&mut self, op: BinOp, l: &Expr, r: &Expr, span: Span) -> Result<Value, EvalError> {
        match op {
            BinOp::And => {
                let lv = self.eval_expr(l)?;
                if lv.is_truthy() {
                    self.eval_expr(r)
                } else {
                    Ok(lv)
                }
            }
            BinOp::Or => {
                let lv = self.eval_expr(l)?;
                if lv.is_truthy() {
                    Ok(lv)
                } else {
                    self.eval_expr(r)
                }
            }
            BinOp::Eq | BinOp::NotEq => {
                let lv = self.eval_expr(l)?;
                let rv = self.eval_expr(r)?;
                let eq = values_equal(&lv, &rv);
                Ok(Value::Bool(if op == BinOp::Eq { eq } else { !eq }))
            }
            BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
                let lv = self.eval_expr(l)?;
                let rv = self.eval_expr(r)?;
                self.eval_compare(op, &lv, &rv, span)
            }
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                let lv = self.eval_expr(l)?;
                let rv = self.eval_expr(r)?;
                self.eval_arith(op, &lv, &rv, span)
            }
        }
    }

    fn eval_compare(&self, op: BinOp, l: &Value, r: &Value, span: Span) -> Result<Value, EvalError> {
        let result = if let (Some(a), Some(b)) = (l.as_number(), r.as_number()) {
            compare(op, a, b)
        } else if let (Value::Str(a), Value::Str(b)) = (l, r) {
            compare(op, a, b)
        } else {
            return Err(self.err(EvalErrorKind::TypeError {
                message: format!("cannot compare {} and {}", l.type_name(), r.type_name()),
                span,
            }));
        };
        Ok(Value::Bool(result))
    }

    fn eval_arith(&self, op: BinOp, l: &Value, r: &Value, span: Span) -> Result<Value, EvalError> {
        if let (Value::Int(a), Value::Int(b)) = (l, r) {
            return match op {
                BinOp::Add => Ok(Value::Int(a.wrapping_add(*b))),
                BinOp::Sub => Ok(Value::Int(a.wrapping_sub(*b))),
                BinOp::Mul => Ok(Value::Int(a.wrapping_mul(*b))),
                BinOp::Mod => {
                    if *b == 0 {
                        Err(self.err(EvalErrorKind::DivisionByZero { span }))
                    } else {
                        Ok(Value::Int(a.rem_euclid(*b)))
                    }
                }
                BinOp::Div => {
                    if *b == 0 {
                        Err(self.err(EvalErrorKind::DivisionByZero { span }))
                    } else {
                        Ok(Value::Float(*a as f64 / *b as f64))
                    }
                }
                _ => unreachable!("non-arithmetic op reached eval_arith"),
            };
        }
        if let (Some(a), Some(b)) = (l.as_number(), r.as_number()) {
            return match op {
                BinOp::Add => Ok(Value::Float(a + b)),
                BinOp::Sub => Ok(Value::Float(a - b)),
                BinOp::Mul => Ok(Value::Float(a * b)),
                BinOp::Div => {
                    if b == 0.0 {
                        Err(self.err(EvalErrorKind::DivisionByZero { span }))
                    } else {
                        Ok(Value::Float(a / b))
                    }
                }
                BinOp::Mod => {
                    if b == 0.0 {
                        Err(self.err(EvalErrorKind::DivisionByZero { span }))
                    } else {
                        Ok(Value::Float(a.rem_euclid(b)))
                    }
                }
                _ => unreachable!("non-arithmetic op reached eval_arith"),
            };
        }
        Err(self.err(EvalErrorKind::TypeError {
            message: format!("cannot apply arithmetic to {} and {}", l.type_name(), r.type_name()),
            span,
        }))
    }

    fn eval_call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Result<Value, EvalError> {
        if let ExprKind::Member(base, field, field_span) = &callee.kind {
            let base_val = self.eval_expr(base)?;
            let arg_vals = args.iter().map(|a| self.eval_expr(a)).collect::<Result<Vec<_>, _>>()?;
            return self.call_method(base_val, field, &arg_vals, *field_span);
        }
        let ExprKind::Ident(name) = &callee.kind else {
            return Err(self.err(EvalErrorKind::TypeError { message: "this expression is not callable".into(), span }));
        };
        let arg_vals = args.iter().map(|a| self.eval_expr(a)).collect::<Result<Vec<_>, _>>()?;
        if let Some(fn_def) = self.functions.get(name).cloned() {
            return self.call_user_fn(&fn_def, &arg_vals);
        }
        self.call_host(name, &arg_vals, span)
    }

    fn call_method(&self, base: Value, field: &str, args: &[Value], span: Span) -> Result<Value, EvalError> {
        match base {
            Value::Color(c) => {
                let amount = args.first().and_then(|v| v.as_number()).unwrap_or(0.0);
                match field {
                    "darken" => Ok(Value::Color(c.darken(amount))),
                    "lighten" => Ok(Value::Color(c.lighten(amount))),
                    "alpha" => Ok(Value::Color(c.alpha(amount))),
                    other => Err(self.err(EvalErrorKind::TypeError { message: format!("color has no method `{other}`"), span })),
                }
            }
            other => {
                Err(self
                    .err(EvalErrorKind::TypeError { message: format!("{} has no method `{field}`", other.type_name()), span }))
            }
        }
    }

    fn call_user_fn(&mut self, f: &FnDef, args: &[Value]) -> Result<Value, EvalError> {
        self.scope.push();
        for (i, p) in f.params.iter().enumerate() {
            self.scope.set_local(p.clone(), args.get(i).cloned().unwrap_or(Value::Nil));
        }
        let flow = self.exec_items(&f.body);
        self.scope.pop();
        Ok(match flow? {
            Flow::Return(v) => v.unwrap_or(Value::Nil),
            Flow::Normal => Value::Nil,
        })
    }

    fn call_host(&self, name: &str, args: &[Value], span: Span) -> Result<Value, EvalError> {
        match self.runtime.registry().get_function(name).cloned() {
            Some(HostFunction::Native(f)) => f(args).map_err(|message| self.err(EvalErrorKind::TypeError { message, span })),
            Some(HostFunction::Lua(f)) => {
                let lua_args: Vec<mlua::Value> = args
                    .iter()
                    .map(|v| value_bridge::to_lua(self.runtime.lua(), v))
                    .collect::<mlua::Result<_>>()
                    .map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
                let result: mlua::MultiValue =
                    f.call(lua_args).map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
                let first = result.into_iter().next().unwrap_or(mlua::Value::Nil);
                Ok(value_bridge::from_lua(&first))
            }
            None => Err(self.err(EvalErrorKind::UnknownFunction { name: name.to_string(), span })),
        }
    }

    /// A bare (uncalled) `base.field` is ambiguous: it's the same shape
    /// whether someone meant member access (`theme.primary`, if `theme`
    /// turns out to be bound to something) or just wrote a dotted bare
    /// string (`certificate = cert.pem`). Real member *access* without a
    /// call isn't otherwise supported (colors only expose methods), so
    /// the rule is: if `base` isn't a real binding, this was always just
    /// a literal string; if it is, member access legitimately isn't
    /// supported on it and that's an error.
    fn eval_member(&mut self, base: &Expr, field: &str, span: Span) -> Result<Value, EvalError> {
        if let ExprKind::Ident(name) = &base.kind {
            if self.resolve_ident(name).is_none() {
                return Ok(Value::Str(format!("{name}.{field}")));
            }
        }
        let base_val = self.eval_expr(base)?;
        Err(self.err(EvalErrorKind::TypeError {
            message: format!(
                "{} has no field `{field}` (only method calls like `.{field}(...)` are supported)",
                base_val.type_name()
            ),
            span,
        }))
    }
}

fn values_equal(l: &Value, r: &Value) -> bool {
    match (l, r) {
        (Value::Int(a), Value::Float(b)) | (Value::Float(b), Value::Int(a)) => (*a as f64) == *b,
        _ => l == r,
    }
}

fn compare<T: PartialOrd>(op: BinOp, a: T, b: T) -> bool {
    match op {
        BinOp::Lt => a < b,
        BinOp::Gt => a > b,
        BinOp::LtEq => a <= b,
        BinOp::GtEq => a >= b,
        _ => unreachable!("non-comparison op reached compare()"),
    }
}
