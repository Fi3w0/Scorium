//! Expected value types, including the extension point for host-defined
//! types (byte sizes, frequencies, addresses, percentages, ...).

use std::fmt;
use std::rc::Rc;

use scorium_core::Value;

/// A host-defined type: validates (and describes) an already-typed
/// [`Value`] beyond what Scorium's built-in literal types express.
///
/// Scope note: this validates a *value that already parsed* as one of
/// Scorium's core typed literals (usually a string or number) -- it does
/// not add new lexer syntax. Host-pluggable literal *syntax* (a bespoke
/// token shape parsed directly by the lexer) is deferred; see
/// `docs/ROADMAP.md`.
pub trait CustomType: fmt::Debug {
    /// The type's name, used in diagnostics (`expected percentage, found ...`).
    fn name(&self) -> &str;

    /// Checks `value` and, optionally, produces a normalized form (e.g. a
    /// `ByteSize` custom type might parse `"10MB"` and hand back a
    /// canonical `Value::Int` of bytes). Returning the same value
    /// unchanged is fine when normalization isn't needed.
    fn validate(&self, value: &Value) -> Result<Value, String>;
}

#[derive(Clone)]
pub enum ValueType {
    String,
    Integer,
    Float,
    Boolean,
    Color,
    Duration,
    List(Box<ValueType>),
    /// Accepts any typed value.
    Any,
    Custom(Rc<dyn CustomType>),
}

impl fmt::Debug for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl ValueType {
    pub fn name(&self) -> String {
        match self {
            ValueType::String => "string".into(),
            ValueType::Integer => "integer".into(),
            ValueType::Float => "float".into(),
            ValueType::Boolean => "boolean".into(),
            ValueType::Color => "color".into(),
            ValueType::Duration => "duration".into(),
            ValueType::List(inner) => format!("list of {}", inner.name()),
            ValueType::Any => "any".into(),
            ValueType::Custom(c) => c.name().to_string(),
        }
    }

    /// Checks `value` against this type, returning the (possibly
    /// normalized, for a [`CustomType`]) value on success.
    pub fn check(&self, value: &Value) -> Result<Value, String> {
        match (self, value) {
            (ValueType::String, Value::Str(_)) => Ok(value.clone()),
            (ValueType::Integer, Value::Int(_)) => Ok(value.clone()),
            (ValueType::Float, Value::Float(_) | Value::Int(_)) => Ok(value.clone()),
            (ValueType::Boolean, Value::Bool(_)) => Ok(value.clone()),
            (ValueType::Color, Value::Color(_)) => Ok(value.clone()),
            (ValueType::Duration, Value::Duration(_)) => Ok(value.clone()),
            (ValueType::List(inner), Value::List(items)) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(inner.check(item)?);
                }
                Ok(Value::List(out))
            }
            (ValueType::Any, _) => Ok(value.clone()),
            (ValueType::Custom(c), _) => c.validate(value),
            _ => Err(format!("expected {}, found {}", self.name(), value.type_name())),
        }
    }
}
