//! Conversion between Scorium's [`Value`] and `mlua::Value`, used to give
//! `script { }` blocks read access to Scorium variables and to bridge
//! host functions implemented in Lua.

use mlua::Lua;
use scorium_core::Value;

pub fn to_lua(lua: &Lua, value: &Value) -> mlua::Result<mlua::Value> {
    Ok(match value {
        Value::Int(i) => mlua::Value::Integer(*i),
        Value::Float(f) => mlua::Value::Number(*f),
        Value::Bool(b) => mlua::Value::Boolean(*b),
        Value::Nil => mlua::Value::Nil,
        Value::Str(s) => mlua::Value::String(lua.create_string(s)?),
        // Colors and durations don't have a native Lua representation;
        // they cross into Lua as their canonical text form. Scorium-side
        // evaluation (arithmetic, `.darken()`, etc.) never round-trips
        // through Lua, so this only matters for `script { }` blocks that
        // want to read one for display/logging.
        Value::Color(c) => mlua::Value::String(lua.create_string(c.to_string())?),
        Value::Duration(d) => mlua::Value::String(lua.create_string(d.to_string())?),
        Value::List(items) => {
            let t = lua.create_table()?;
            for (i, item) in items.iter().enumerate() {
                t.set(i + 1, to_lua(lua, item)?)?;
            }
            mlua::Value::Table(t)
        }
    })
}

pub fn from_lua(value: &mlua::Value) -> Value {
    match value {
        mlua::Value::Nil => Value::Nil,
        mlua::Value::Boolean(b) => Value::Bool(*b),
        mlua::Value::Integer(i) => Value::Int(*i),
        mlua::Value::Number(f) => Value::Float(*f),
        mlua::Value::String(s) => Value::Str(s.to_string_lossy()),
        mlua::Value::Table(t) => {
            let len = t.raw_len();
            let mut items = Vec::with_capacity(len);
            for i in 1..=len {
                match t.raw_get::<mlua::Value>(i) {
                    Ok(v) => items.push(from_lua(&v)),
                    Err(_) => break,
                }
            }
            Value::List(items)
        }
        _ => Value::Nil,
    }
}
