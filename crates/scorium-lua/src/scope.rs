//! Variable resolution. Three layers, checked innermost-first:
//!
//! 1. lexically-scoped bindings (`for`/`fn`/`local`), pushed and popped
//!    around the block that owns them;
//! 2. `@name = value` definitions, visible everywhere after their
//!    declaration (one shared map, matching "definitions are visible
//!    after their declaration" and includes sharing one environment);
//! 3. host-registered values.
//!
//! An identifier that resolves nowhere isn't an error here -- the caller
//! falls back to treating it as a literal string, which is what lets
//! `select(kitty, alacritty, foot)` skip quoting.

use std::collections::HashMap;

use scorium_core::Value;

use crate::registry::Registry;

#[derive(Default)]
pub struct Scope {
    lexical: Vec<HashMap<String, Value>>,
    vardefs: HashMap<String, Value>,
    /// A stack of `lexical.len()` snapshots, one per currently-open node
    /// body -- see [`Self::is_reassignable_local`].
    node_boundaries: Vec<usize>,
}

impl Scope {
    pub fn new() -> Self {
        Self { lexical: vec![HashMap::new()], vardefs: HashMap::new(), node_boundaries: Vec::new() }
    }

    /// Call before pushing a node body's own scope frame. Locals declared
    /// at or below this depth (a function's parameters, an ancestor
    /// node's locals) become ineligible for `key = value` reassignment
    /// inside this node body -- only locals declared *within* it are.
    pub fn enter_node_body(&mut self) {
        self.node_boundaries.push(self.lexical.len());
    }

    pub fn exit_node_body(&mut self) {
        self.node_boundaries.pop();
    }

    pub fn push(&mut self) {
        self.lexical.push(HashMap::new());
    }

    pub fn pop(&mut self) {
        self.lexical.pop();
        if self.lexical.is_empty() {
            self.lexical.push(HashMap::new());
        }
    }

    pub fn set_local(&mut self, name: String, value: Value) {
        self.lexical.last_mut().expect("at least one scope always present").insert(name, value);
    }

    /// Is `name` a lexically-scoped local declared *within the
    /// innermost currently-open node body* (not an `@`-vardef, not a
    /// sibling leaf, not a host value, and not a local from further out
    /// -- e.g. a function's parameters aren't reassignable from inside a
    /// node the function's body opens)? `key = value` reassigns such a
    /// local rather than emitting a leaf exactly when this is true --
    /// this is what lets `n = n + 1` advance a `while` loop counter
    /// while `service { port = port }` still emits a `port` leaf even
    /// though `port` is also a function parameter. See
    /// [`Self::reassign_local`] and `docs/GRAMMAR.md`.
    pub fn is_reassignable_local(&self, name: &str) -> bool {
        let boundary = self.node_boundaries.last().copied().unwrap_or(0);
        self.lexical[boundary..].iter().any(|frame| frame.contains_key(name))
    }

    /// Updates an existing local in whichever frame currently holds it.
    /// Panics if `name` isn't reassignable -- callers must check
    /// [`Self::is_reassignable_local`] first.
    pub fn reassign_local(&mut self, name: &str, value: Value) {
        let boundary = self.node_boundaries.last().copied().unwrap_or(0);
        for frame in self.lexical[boundary..].iter_mut().rev() {
            if frame.contains_key(name) {
                frame.insert(name.to_string(), value);
                return;
            }
        }
        unreachable!("reassign_local called without checking is_reassignable_local first");
    }

    pub fn set_vardef(&mut self, name: String, value: Value) {
        self.vardefs.insert(name, value);
    }

    pub fn lookup(&self, name: &str, registry: &Registry) -> Option<Value> {
        for frame in self.lexical.iter().rev() {
            if let Some(v) = frame.get(name) {
                return Some(v.clone());
            }
        }
        if let Some(v) = self.vardefs.get(name) {
            return Some(v.clone());
        }
        registry.get_value(name).cloned()
    }

    /// Every name currently visible, innermost-wins -- used to give a
    /// `script { }` block read access to Scorium variables as Lua
    /// globals. One-way: writes inside the script don't flow back.
    pub fn all_visible(&self, registry: &Registry) -> HashMap<String, Value> {
        let mut out = registry.values.clone();
        out.extend(self.vardefs.clone());
        for frame in &self.lexical {
            out.extend(frame.clone());
        }
        out
    }
}
