//! `scorium-lua`: the sandboxed evaluation runtime for Scorium.
//!
//! Evaluates a parsed [`scorium_core::Document`] into a tree of
//! [`scorium_core::entry::Entry`] values. Control flow is interpreted
//! directly (see `eval.rs`); a `script { }` block runs as real Lua
//! against a restricted [`mlua::Lua`] state (`math`/`string`/`table`
//! only -- no `io`, `os`, `package`, `debug`, process, or network
//! access). One [`Registry`] backs both the function-call surface and
//! (through [`Runtime::register_lua_function`]) Lua-implemented host
//! functions, so a host never has to implement an operation twice.

mod error;
mod eval;
mod registry;
mod runtime;
mod scope;
mod value_bridge;

pub use error::{EvalError, EvalErrorKind};
pub use registry::{HostFunction, Registry};
pub use runtime::{EvalOutput, IncludePolicy, Runtime, RuntimeOptions};
pub use value_bridge::{from_lua, to_lua};
