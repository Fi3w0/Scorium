use scorium_core::entry::{Entry, LeafEntry, NodeEntry};
use scorium_core::{Span, Value};
use scorium_schema::{DuplicateKeyPolicy, NodeSchema, Schema, SchemaErrorKind, ValueType};

fn span(n: u32) -> Span {
    Span::new(n, n + 1)
}

fn leaf(key: &str, value: Value, at: u32) -> Entry {
    Entry::Leaf(LeafEntry { key: key.to_string(), key_span: span(at), value, span: span(at) })
}

fn node(name: &str, children: Vec<Entry>, at: u32) -> Entry {
    Entry::Node(NodeEntry {
        name: name.to_string(),
        name_span: span(at),
        header: None,
        header_span: None,
        children,
        span: span(at),
    })
}

fn server_schema() -> Schema {
    Schema::builder()
        .node(
            "server",
            NodeSchema::builder()
                .key("host", ValueType::String)
                .required_key("port", ValueType::Integer)
                .key("timeout", ValueType::Duration)
                .key("enabled", ValueType::Boolean)
                .build(),
        )
        .build()
}

#[test]
fn valid_document_passes() {
    let schema = server_schema();
    let entries =
        vec![node("server", vec![leaf("host", Value::Str("localhost".into()), 1), leaf("port", Value::Int(8080), 2)], 0)];
    let result = schema.validate(&entries);
    assert!(result.is_valid(), "{:?}", result.errors);
}

#[test]
fn unknown_node_is_reported_with_suggestion() {
    let schema = server_schema();
    let entries = vec![node("servr", vec![], 0)];
    let result = schema.validate(&entries);
    assert_eq!(result.errors.len(), 1);
    match &result.errors[0] {
        SchemaErrorKind::UnknownNode { name, suggestion, .. } => {
            assert_eq!(name, "servr");
            assert_eq!(suggestion.as_deref(), Some("server"));
        }
        other => panic!("expected UnknownNode, got {other:?}"),
    }
}

#[test]
fn unknown_key_is_reported_with_suggestion() {
    let schema = server_schema();
    let entries = vec![node("server", vec![leaf("timeuot", Value::Duration(dur()), 1), leaf("port", Value::Int(1), 2)], 0)];
    let result = schema.validate(&entries);
    let unknown = result
        .errors
        .iter()
        .find(|e| matches!(e, SchemaErrorKind::UnknownKey { .. }))
        .unwrap_or_else(|| panic!("expected an UnknownKey error, got {:?}", result.errors));
    match unknown {
        SchemaErrorKind::UnknownKey { name, suggestion, .. } => {
            assert_eq!(name, "timeuot");
            assert_eq!(suggestion.as_deref(), Some("timeout"));
        }
        _ => unreachable!(),
    }
}

fn dur() -> scorium_core::DurationValue {
    scorium_core::DurationValue::new(5.0, scorium_core::DurationUnit::Seconds)
}

#[test]
fn wrong_type_is_reported() {
    let schema = server_schema();
    // `port` is declared Integer; a bare string value is wrong.
    let entries = vec![node("server", vec![leaf("port", Value::Str("many".into()), 1)], 0)];
    let result = schema.validate(&entries);
    assert!(result.errors.iter().any(|e| matches!(e, SchemaErrorKind::WrongType { key, .. } if key == "port")));
}

#[test]
fn duration_without_unit_is_a_type_error_not_a_lex_error() {
    // `timeout = 5` (a bare integer, not `5s`) must fail schema
    // validation against ValueType::Duration -- the lexer never guesses
    // a unit, so this is purely a type mismatch at validation time.
    let schema = server_schema();
    let entries = vec![node("server", vec![leaf("timeout", Value::Int(5), 1), leaf("port", Value::Int(1), 2)], 0)];
    let result = schema.validate(&entries);
    assert!(result.errors.iter().any(|e| matches!(e, SchemaErrorKind::WrongType { key, .. } if key == "timeout")));
}

#[test]
fn missing_required_key_is_reported() {
    let schema = server_schema();
    let entries = vec![node("server", vec![leaf("host", Value::Str("localhost".into()), 1)], 0)];
    let result = schema.validate(&entries);
    assert!(result.errors.iter().any(|e| matches!(e, SchemaErrorKind::MissingRequiredKey { key, .. } if key == "port")));
}

#[test]
fn duplicate_key_is_reported_by_default() {
    let schema = server_schema();
    let entries = vec![node("server", vec![leaf("port", Value::Int(1), 1), leaf("port", Value::Int(2), 2)], 0)];
    let result = schema.validate(&entries);
    assert!(result.errors.iter().any(|e| matches!(e, SchemaErrorKind::DuplicateKey { key, .. } if key == "port")));
}

#[test]
fn duplicate_key_allowed_under_last_wins_policy() {
    let schema = Schema::builder()
        .node(
            "server",
            NodeSchema::builder().key("port", ValueType::Integer).duplicate_key_policy(DuplicateKeyPolicy::LastWins).build(),
        )
        .build();
    let entries = vec![node("server", vec![leaf("port", Value::Int(1), 1), leaf("port", Value::Int(2), 2)], 0)];
    let result = schema.validate(&entries);
    assert!(result.is_valid(), "{:?}", result.errors);
}

#[test]
fn nested_node_schema() {
    let schema = Schema::builder()
        .node(
            "server",
            NodeSchema::builder().node("tls", NodeSchema::builder().key("enabled", ValueType::Boolean).build()).build(),
        )
        .build();
    let entries = vec![node("server", vec![node("tls", vec![leaf("enabled", Value::Bool(true), 1)], 1)], 0)];
    let result = schema.validate(&entries);
    assert!(result.is_valid(), "{:?}", result.errors);
}

#[test]
fn custom_type_validation() {
    #[derive(Debug)]
    struct Percentage;
    impl scorium_schema::CustomType for Percentage {
        fn name(&self) -> &str {
            "percentage"
        }
        fn validate(&self, value: &Value) -> Result<Value, String> {
            match value.as_number() {
                Some(n) if (0.0..=100.0).contains(&n) => Ok(value.clone()),
                Some(n) => Err(format!("{n} is out of range 0..=100")),
                None => Err(format!("expected a percentage, found {}", value.type_name())),
            }
        }
    }

    let schema = Schema::builder()
        .node("volume", NodeSchema::builder().key("level", ValueType::Custom(std::rc::Rc::new(Percentage))).build())
        .build();

    let ok = vec![node("volume", vec![leaf("level", Value::Int(50), 1)], 0)];
    assert!(schema.validate(&ok).is_valid());

    let bad = vec![node("volume", vec![leaf("level", Value::Int(150), 1)], 0)];
    let result = schema.validate(&bad);
    assert!(result.errors.iter().any(|e| matches!(e, SchemaErrorKind::WrongType { key, .. } if key == "level")));
}

#[test]
fn unknown_keys_allowed_when_configured() {
    let schema = Schema::builder().node("server", NodeSchema::builder().allow_unknown_keys(true).build()).build();
    let entries = vec![node("server", vec![leaf("whatever", Value::Str("x".into()), 1)], 0)];
    assert!(schema.validate(&entries).is_valid());
}
