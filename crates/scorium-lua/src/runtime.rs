//! The public entry point: a sandboxed [`Runtime`] that evaluates a
//! parsed [`Document`] into a tree of [`Entry`] values.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use mlua::{Lua, LuaOptions, StdLib};
use scorium_core::entry::Entry;
use scorium_core::{Document, Source, Value};

use crate::error::EvalError;
use crate::eval::Evaluator;
use crate::registry::Registry;

/// Whether, and how strictly, `include "..."` is allowed.
#[derive(Debug, Clone)]
pub struct IncludePolicy {
    pub enabled: bool,
    /// If `false` (the default), include paths with a `..` component are
    /// rejected rather than allowed to walk above the including file's
    /// directory. Absolute paths and symlink escapes are rejected too.
    pub allow_parent_traversal: bool,
}

impl Default for IncludePolicy {
    fn default() -> Self {
        Self { enabled: true, allow_parent_traversal: false }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    pub include_policy: IncludePolicy,
    /// Total `for`/`while` loop iterations allowed across one
    /// evaluation, as a sandbox limit against runaway config scripts.
    pub max_loop_iterations: u64,
    /// Maximum nesting depth for calls to Scorium `fn` definitions.
    pub max_function_call_depth: u32,
    /// Lua VM instructions allowed per `script { }` block.
    pub max_script_instructions: u64,
    /// Maximum bytes the restricted Lua state may allocate. This caps
    /// strings, tables, compiled chunks, and other Lua-owned memory across
    /// an evaluation runtime. Set to `0` to disable the memory limit.
    pub max_lua_memory_bytes: usize,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            include_policy: IncludePolicy::default(),
            max_loop_iterations: 1_000_000,
            max_function_call_depth: 256,
            max_script_instructions: 50_000_000,
            max_lua_memory_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
pub struct EvalOutput {
    pub entries: Vec<Entry>,
    pub warnings: Vec<String>,
}

/// A sandboxed evaluation environment: a restricted Lua state (no `io`,
/// `os`, `package`, `debug`, process, or network access -- only `math`,
/// `string`, and `table`) plus the host's function/value registrations.
pub struct Runtime {
    lua: Lua,
    registry: Registry,
    options: RuntimeOptions,
    script_budget: Rc<Cell<u64>>,
}

impl Runtime {
    pub fn new() -> Result<Self, mlua::Error> {
        Self::with_options(RuntimeOptions::default())
    }

    pub fn with_options(options: RuntimeOptions) -> Result<Self, mlua::Error> {
        let lua = Lua::new_with(StdLib::MATH | StdLib::STRING | StdLib::TABLE, LuaOptions::default())?;
        lua.set_memory_limit(options.max_lua_memory_bytes)?;
        let budget = Rc::new(Cell::new(options.max_script_instructions));
        {
            let budget = budget.clone();
            lua.set_hook(mlua::HookTriggers::new().every_nth_instruction(1000), move |_lua, _debug| {
                let remaining = budget.get();
                if remaining <= 1000 {
                    return Err(mlua::Error::RuntimeError(
                        "script exceeded its instruction budget (sandbox execution limit)".into(),
                    ));
                }
                budget.set(remaining - 1000);
                Ok(mlua::VmState::Continue)
            });
        }
        Ok(Self { lua, registry: Registry::default(), options, script_budget: budget })
    }

    /// Registers a host function reachable from expressions (`f(a, b)`)
    /// and from `script { }` blocks alike -- the "one registry, multiple
    /// surfaces" mechanism. See `docs/EMBEDDING.md`.
    pub fn register_function(
        &mut self,
        name: impl Into<String>,
        f: impl Fn(&[Value]) -> Result<Value, String> + 'static,
    ) -> &mut Self {
        self.registry.register_function(name, f);
        self
    }

    /// Registers a host-provided Lua function through the same registry.
    pub fn register_lua_function(&mut self, name: impl Into<String>, f: mlua::Function) -> &mut Self {
        self.registry.register_lua_function(name, f);
        self
    }

    /// Registers a host-provided value, reachable as a plain identifier
    /// in expressions (e.g. `environment` resolving to a host-supplied
    /// string) and inside `script { }` blocks.
    pub fn register_value(&mut self, name: impl Into<String>, value: Value) -> &mut Self {
        self.registry.register_value(name, value);
        self
    }

    pub fn options(&self) -> &RuntimeOptions {
        &self.options
    }

    /// Accesses the restricted Lua state for advanced host integration,
    /// primarily to create a function for [`Self::register_lua_function`].
    /// Values created by a different [`Lua`] state cannot be registered.
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub(crate) fn registry(&self) -> &Registry {
        &self.registry
    }

    pub(crate) fn reset_script_budget(&self) {
        self.script_budget.set(self.options.max_script_instructions);
    }

    /// Evaluates an already-parsed document. `base_dir` is where relative
    /// `include "..."` paths resolve from -- typically the directory
    /// containing `source`, or the current directory for an in-memory
    /// source with no file of its own.
    pub fn evaluate(&self, doc: &Document, source: &Source, base_dir: &Path) -> Result<EvalOutput, EvalError> {
        let evaluator = Evaluator::new(self, source.clone(), base_dir.to_path_buf());
        let (entries, warnings) = evaluator.run(doc)?;
        Ok(EvalOutput { entries, warnings })
    }
}
