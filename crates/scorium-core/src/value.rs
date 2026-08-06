//! Typed runtime values: the result of evaluating a Scorium literal or
//! expression. Distinct from `ast::Expr`, which is the *syntax* -- `Value`
//! is what you get after evaluation, and it's what `scorium-schema`
//! validates and what host applications inspect.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationUnit {
    Millis,
    Seconds,
    Minutes,
}

impl DurationUnit {
    pub fn suffix(self) -> &'static str {
        match self {
            DurationUnit::Millis => "ms",
            DurationUnit::Seconds => "s",
            DurationUnit::Minutes => "m",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ms" => Some(DurationUnit::Millis),
            "s" => Some(DurationUnit::Seconds),
            "m" => Some(DurationUnit::Minutes),
            _ => None,
        }
    }
}

/// A duration literal such as `600ms`, `1.5s`, `2m`. The unit is part of
/// the type -- a bare number is never guessed at as a duration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DurationValue {
    pub amount: f64,
    pub unit: DurationUnit,
}

impl DurationValue {
    pub fn new(amount: f64, unit: DurationUnit) -> Self {
        Self { amount, unit }
    }

    pub fn as_millis(self) -> f64 {
        match self.unit {
            DurationUnit::Millis => self.amount,
            DurationUnit::Seconds => self.amount * 1_000.0,
            DurationUnit::Minutes => self.amount * 60_000.0,
        }
    }
}

impl fmt::Display for DurationValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.amount.fract() == 0.0 {
            write!(f, "{}{}", self.amount as i64, self.unit.suffix())
        } else {
            write!(f, "{}{}", self.amount, self.unit.suffix())
        }
    }
}

/// A color literal such as `#8EDDFF` or `#101820CC`. Stored as real RGBA
/// channels (not a string) so expressions like `primary.darken(0.25)` have
/// something to operate on; the original textual form is preserved
/// separately by the formatter, which reads the source span rather than
/// re-deriving text from the channels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorValue {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl ColorValue {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parses a `RRGGBB` or `RRGGBBAA` hex string (no leading `#`).
    pub fn parse_hex(hex: &str) -> Option<Self> {
        let bytes = |s: &str| u8::from_str_radix(s, 16).ok();
        match hex.len() {
            6 => Some(Self::rgb(bytes(&hex[0..2])?, bytes(&hex[2..4])?, bytes(&hex[4..6])?)),
            8 => Some(Self::rgba(bytes(&hex[0..2])?, bytes(&hex[2..4])?, bytes(&hex[4..6])?, bytes(&hex[6..8])?)),
            _ => None,
        }
    }

    /// Darkens each channel by `amount` (0.0..=1.0), clamped.
    pub fn darken(self, amount: f64) -> Self {
        let scale = 1.0 - amount.clamp(0.0, 1.0);
        Self {
            r: (self.r as f64 * scale).round() as u8,
            g: (self.g as f64 * scale).round() as u8,
            b: (self.b as f64 * scale).round() as u8,
            a: self.a,
        }
    }

    /// Lightens each channel toward white by `amount` (0.0..=1.0), clamped.
    pub fn lighten(self, amount: f64) -> Self {
        let t = amount.clamp(0.0, 1.0);
        let mix = |c: u8| (c as f64 + (255.0 - c as f64) * t).round() as u8;
        Self { r: mix(self.r), g: mix(self.g), b: mix(self.b), a: self.a }
    }

    /// Returns a copy with the alpha channel replaced (`amount` in 0.0..=1.0).
    pub fn alpha(self, amount: f64) -> Self {
        Self { a: (amount.clamp(0.0, 1.0) * 255.0).round() as u8, ..self }
    }

    pub fn to_hex(self, include_alpha: bool) -> String {
        if include_alpha {
            format!("{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        } else {
            format!("{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        }
    }
}

impl fmt::Display for ColorValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.to_hex(self.a != 255))
    }
}

/// An evaluated Scorium value. This is the type schema validation checks
/// and the type host applications receive back from evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Nil,
    Str(String),
    Color(ColorValue),
    Duration(DurationValue),
    List(Vec<Value>),
}

impl Value {
    /// Lua-style truthiness: everything is truthy except `nil` and `false`.
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "integer",
            Value::Float(_) => "float",
            Value::Bool(_) => "boolean",
            Value::Nil => "nil",
            Value::Str(_) => "string",
            Value::Color(_) => "color",
            Value::Duration(_) => "duration",
            Value::List(_) => "list",
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Nil => write!(f, "nil"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Color(c) => write!(f, "{c}"),
            Value::Duration(d) => write!(f, "{d}"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
        }
    }
}
