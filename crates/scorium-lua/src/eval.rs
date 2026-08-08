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

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
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
    function_call_depth: u32,
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
            function_call_depth: 0,
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
        let env = lua.create_table().map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;

        // Each block receives a fresh environment. Copying the restricted
        // base globals prevents one config (or an earlier block) from
        // leaking globals into the next; copying library tables prevents
        // `math.abs = ...`-style mutation from poisoning later blocks.
        for pair in globals.clone().pairs::<mlua::Value, mlua::Value>() {
            let (key, value) = pair.map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
            if let mlua::Value::String(name) = &key {
                // These base functions operate on state outside `_ENV` and
                // would let one supposedly isolated block mutate the shared
                // Lua VM (for example through the string metatable), disable
                // garbage collection, or compile a chunk in the real global
                // environment.
                if ["_G", "collectgarbage", "getmetatable", "load"].iter().any(|blocked| name.as_bytes() == blocked.as_bytes()) {
                    continue;
                }
            }
            let value = match value {
                mlua::Value::Table(table) => {
                    let copy =
                        lua.create_table().map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
                    for entry in table.pairs::<mlua::Value, mlua::Value>() {
                        let (k, v) = entry.map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
                        copy.raw_set(k, v).map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
                    }
                    mlua::Value::Table(copy)
                }
                other => other,
            };
            env.raw_set(key, value).map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
        }
        env.set("_G", env.clone()).map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;

        for (name, value) in self.scope.all_visible(self.runtime.registry()) {
            let lv = value_bridge::to_lua(lua, &value)
                .map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
            env.set(name, lv).map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
        }
        for (name, function) in &self.runtime.registry().functions {
            let lua_function = match function {
                HostFunction::Native(function) => {
                    let function = Rc::clone(function);
                    lua.create_function(move |lua, args: mlua::MultiValue| {
                        let args: Vec<Value> = args.iter().map(value_bridge::from_lua).collect();
                        let result = function(&args).map_err(mlua::Error::RuntimeError)?;
                        value_bridge::to_lua(lua, &result)
                    })
                }
                HostFunction::Lua(function) => Ok(function.clone()),
            }
            .map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
            env.set(name.as_str(), lua_function)
                .map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
        }
        self.runtime.reset_script_budget();
        lua.load(&block.raw)
            .set_name("script")
            .set_environment(env)
            .exec()
            .map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))
    }

    fn exec_include(&mut self, inc: &IncludeStmt, span: Span) -> Result<(), EvalError> {
        let options = self.runtime.options();
        if !options.include_policy.enabled {
            return Err(self.err(EvalErrorKind::IncludesDisabled { span }));
        }
        let path_str = self.eval_strlit(&inc.path, inc.path_span)?;
        let include_path = Path::new(&path_str);
        if !options.include_policy.allow_parent_traversal
            && (include_path.is_absolute() || include_path.components().any(|c| c == Component::ParentDir))
        {
            return Err(self.err(EvalErrorKind::IncludePathDenied { path: path_str, span }));
        }
        let resolved = self.base_dir.join(include_path);
        // A relative path without `..` can still escape through a symlink.
        // When the restrictive policy is active, compare canonical paths
        // before opening the target. Missing targets continue to the normal
        // IncludeIo diagnostic below.
        if !options.include_policy.allow_parent_traversal {
            if let (Ok(base), Ok(target)) = (self.base_dir.canonicalize(), resolved.canonicalize()) {
                if !target.starts_with(&base) {
                    return Err(self.err(EvalErrorKind::IncludePathDenied { path: path_str, span }));
                }
            }
        }
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
                Value::Int(i) => {
                    i.checked_neg().map(Value::Int).ok_or_else(|| self.err(EvalErrorKind::ArithmeticOverflow { span }))
                }
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
        let ordering = match (l, r) {
            (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            (Value::Int(a), Value::Float(b)) => cmp_int_float(*a, *b),
            (Value::Float(a), Value::Int(b)) => cmp_int_float(*b, *a).map(Ordering::reverse),
            (Value::Str(a), Value::Str(b)) => Some(a.cmp(b)),
            _ => {
                return Err(self.err(EvalErrorKind::TypeError {
                    message: format!("cannot compare {} and {}", l.type_name(), r.type_name()),
                    span,
                }))
            }
        };
        // As in Lua, every ordered comparison involving NaN is false.
        Ok(Value::Bool(ordering.is_some_and(|ordering| compare_ordering(op, ordering))))
    }

    fn eval_arith(&self, op: BinOp, l: &Value, r: &Value, span: Span) -> Result<Value, EvalError> {
        if let (Value::Int(a), Value::Int(b)) = (l, r) {
            return match op {
                BinOp::Add => {
                    a.checked_add(*b).map(Value::Int).ok_or_else(|| self.err(EvalErrorKind::ArithmeticOverflow { span }))
                }
                BinOp::Sub => {
                    a.checked_sub(*b).map(Value::Int).ok_or_else(|| self.err(EvalErrorKind::ArithmeticOverflow { span }))
                }
                BinOp::Mul => {
                    a.checked_mul(*b).map(Value::Int).ok_or_else(|| self.err(EvalErrorKind::ArithmeticOverflow { span }))
                }
                BinOp::Mod => {
                    if *b == 0 {
                        Err(self.err(EvalErrorKind::DivisionByZero { span }))
                    } else if *b == -1 {
                        // The mathematical result is zero, but Rust's `%`
                        // operation overflows for i64::MIN / -1.
                        Ok(Value::Int(0))
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
            return self.call_user_fn(&fn_def, &arg_vals, span);
        }
        self.call_host(name, &arg_vals, span)
    }

    fn call_method(&self, base: Value, field: &str, args: &[Value], span: Span) -> Result<Value, EvalError> {
        match base {
            Value::Color(c) => {
                if !matches!(field, "darken" | "lighten" | "alpha") {
                    return Err(self.err(EvalErrorKind::TypeError { message: format!("color has no method `{field}`"), span }));
                }
                if args.len() != 1 {
                    return Err(self.err(EvalErrorKind::TypeError {
                        message: format!("color.{field}() expects exactly one numeric argument"),
                        span,
                    }));
                }
                let amount = args[0].as_number().ok_or_else(|| {
                    self.err(EvalErrorKind::TypeError {
                        message: format!("color.{field}() expects a number, found {}", args[0].type_name()),
                        span,
                    })
                })?;
                match field {
                    "darken" => Ok(Value::Color(c.darken(amount))),
                    "lighten" => Ok(Value::Color(c.lighten(amount))),
                    "alpha" => Ok(Value::Color(c.alpha(amount))),
                    _ => unreachable!("known color method checked above"),
                }
            }
            other => {
                Err(self
                    .err(EvalErrorKind::TypeError { message: format!("{} has no method `{field}`", other.type_name()), span }))
            }
        }
    }

    fn call_user_fn(&mut self, f: &FnDef, args: &[Value], span: Span) -> Result<Value, EvalError> {
        let limit = self.runtime.options().max_function_call_depth;
        if self.function_call_depth >= limit {
            return Err(self.err(EvalErrorKind::CallDepthExceeded { limit, span }));
        }
        self.function_call_depth += 1;
        self.scope.push();
        for (i, p) in f.params.iter().enumerate() {
            self.scope.set_local(p.clone(), args.get(i).cloned().unwrap_or(Value::Nil));
        }
        let flow = self.exec_items(&f.body);
        self.scope.pop();
        self.function_call_depth -= 1;
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
                let result: mlua::MultiValue = f
                    .call(mlua::MultiValue::from_vec(lua_args))
                    .map_err(|e| self.err(EvalErrorKind::ScriptError { message: e.to_string(), span }))?;
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
        (Value::Int(a), Value::Float(b)) | (Value::Float(b), Value::Int(a)) => cmp_int_float(*a, *b) == Some(Ordering::Equal),
        _ => l == r,
    }
}

/// Exact ordering between an i64 and f64 without first rounding the integer
/// to f64. That rounding would incorrectly make 9_007_199_254_740_993 equal
/// to 9_007_199_254_740_992.0.
fn cmp_int_float(integer: i64, float: f64) -> Option<Ordering> {
    if float.is_nan() {
        return None;
    }
    const I64_MIN_AS_F64: f64 = -9_223_372_036_854_775_808.0;
    const I64_MAX_PLUS_ONE_AS_F64: f64 = 9_223_372_036_854_775_808.0;
    if float < I64_MIN_AS_F64 {
        return Some(Ordering::Greater);
    }
    if float >= I64_MAX_PLUS_ONE_AS_F64 {
        return Some(Ordering::Less);
    }

    let truncated = float.trunc() as i64;
    match integer.cmp(&truncated) {
        Ordering::Equal if float.fract() > 0.0 => Some(Ordering::Less),
        Ordering::Equal if float.fract() < 0.0 => Some(Ordering::Greater),
        ordering => Some(ordering),
    }
}

fn compare_ordering(op: BinOp, ordering: Ordering) -> bool {
    match op {
        BinOp::Lt => ordering == Ordering::Less,
        BinOp::Gt => ordering == Ordering::Greater,
        BinOp::LtEq => ordering != Ordering::Greater,
        BinOp::GtEq => ordering != Ordering::Less,
        _ => unreachable!("non-comparison op reached compare()"),
    }
}
