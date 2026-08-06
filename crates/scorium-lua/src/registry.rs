//! The one registration mechanism behind both of Scorium's surfaces: a
//! host function registered here is reachable from expressions
//! (`select(kitty, foot)`) and from `script { }` blocks alike, since both
//! paths call through the same [`Registry`]. Scorium's core node/leaf
//! grammar is the declarative surface and never goes through this at all
//! -- it's just data the host walks afterward. A host that wants
//! node-shaped sugar over a registered function (the way a `bind { }`
//! block might desugar to a `bind()` call) builds that itself on top of
//! the evaluated tree; see `docs/EMBEDDING.md`.

use std::collections::HashMap;
use std::rc::Rc;

use scorium_core::Value;

pub type NativeFn = dyn Fn(&[Value]) -> Result<Value, String>;

#[derive(Clone)]
pub enum HostFunction {
    Native(Rc<NativeFn>),
    Lua(mlua::Function),
}

#[derive(Default, Clone)]
pub struct Registry {
    pub(crate) functions: HashMap<String, HostFunction>,
    pub(crate) values: HashMap<String, Value>,
}

impl Registry {
    pub fn register_function(&mut self, name: impl Into<String>, f: impl Fn(&[Value]) -> Result<Value, String> + 'static) {
        self.functions.insert(name.into(), HostFunction::Native(Rc::new(f)));
    }

    pub fn register_lua_function(&mut self, name: impl Into<String>, f: mlua::Function) {
        self.functions.insert(name.into(), HostFunction::Lua(f));
    }

    pub fn register_value(&mut self, name: impl Into<String>, value: Value) {
        self.values.insert(name.into(), value);
    }

    pub fn get_function(&self, name: &str) -> Option<&HostFunction> {
        self.functions.get(name)
    }

    pub fn get_value(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }
}
