//! Walks an evaluated [`Entry`] tree against a [`Schema`], collecting
//! every problem found rather than stopping at the first one.

use std::collections::{HashMap, HashSet};

use scorium_core::entry::{Entry, NodeEntry};
use scorium_core::Span;

use crate::builder::{DuplicateKeyPolicy, NodeSchema, Schema};
use crate::error::{suggest, SchemaErrorKind, ValidationResult};

pub fn validate(schema: &Schema, entries: &[Entry]) -> ValidationResult {
    let mut errors = Vec::new();
    let mut seen_root: HashMap<String, Span> = HashMap::new();
    let mut required_seen_root: HashSet<String> = HashSet::new();

    for entry in entries {
        match entry {
            Entry::Node(n) => match schema.nodes.get(&n.name) {
                Some(node_schema) => validate_node(n, node_schema, &mut errors),
                None if !schema.allow_unknown_nodes => errors.push(SchemaErrorKind::UnknownNode {
                    name: n.name.clone(),
                    suggestion: suggest(&n.name, schema.nodes.keys()),
                    span: n.name_span,
                }),
                None => {}
            },
            Entry::Leaf(l) => {
                if let Some(&first) = seen_root.get(&l.key) {
                    errors.push(SchemaErrorKind::DuplicateKey { key: l.key.clone(), span: l.key_span, first_span: first });
                } else {
                    seen_root.insert(l.key.clone(), l.key_span);
                }
                required_seen_root.insert(l.key.clone());
                match schema.root_keys.get(&l.key) {
                    Some(key_schema) => {
                        if let Err(message) = key_schema.value_type.check(&l.value) {
                            errors.push(SchemaErrorKind::WrongType { key: l.key.clone(), message, span: l.key_span });
                        }
                    }
                    None => errors.push(SchemaErrorKind::UnknownKey {
                        name: l.key.clone(),
                        node: "<document>".to_string(),
                        suggestion: suggest(&l.key, schema.root_keys.keys()),
                        span: l.key_span,
                    }),
                }
            }
            Entry::Include(_) | Entry::HostCall(_) => {}
        }
    }

    let doc_span = entries.first().map(Entry::span).unwrap_or(Span::at(0));
    for (key, key_schema) in &schema.root_keys {
        if key_schema.required && !required_seen_root.contains(key) {
            errors.push(SchemaErrorKind::MissingRequiredKey { key: key.clone(), node: "<document>".to_string(), span: doc_span });
        }
    }

    ValidationResult { errors }
}

fn validate_node(node: &NodeEntry, schema: &NodeSchema, errors: &mut Vec<SchemaErrorKind>) {
    let mut seen: HashMap<String, Span> = HashMap::new();
    let mut required_seen: HashSet<String> = HashSet::new();

    for child in &node.children {
        match child {
            Entry::Leaf(l) => {
                if let Some(&first) = seen.get(&l.key) {
                    if schema.duplicate_key_policy == DuplicateKeyPolicy::Error {
                        errors.push(SchemaErrorKind::DuplicateKey { key: l.key.clone(), span: l.key_span, first_span: first });
                    }
                } else {
                    seen.insert(l.key.clone(), l.key_span);
                }
                required_seen.insert(l.key.clone());
                match schema.keys.get(&l.key) {
                    Some(key_schema) => {
                        if let Err(message) = key_schema.value_type.check(&l.value) {
                            errors.push(SchemaErrorKind::WrongType { key: l.key.clone(), message, span: l.key_span });
                        }
                    }
                    None if !schema.allow_unknown_keys => errors.push(SchemaErrorKind::UnknownKey {
                        name: l.key.clone(),
                        node: node.name.clone(),
                        suggestion: suggest(&l.key, schema.keys.keys()),
                        span: l.key_span,
                    }),
                    None => {}
                }
            }
            Entry::Node(child_node) => match schema.children.get(&child_node.name) {
                Some(child_schema) => validate_node(child_node, child_schema, errors),
                None => errors.push(SchemaErrorKind::UnknownNode {
                    name: child_node.name.clone(),
                    suggestion: suggest(&child_node.name, schema.children.keys()),
                    span: child_node.name_span,
                }),
            },
            Entry::Include(_) | Entry::HostCall(_) => {}
        }
    }

    for (key, key_schema) in &schema.keys {
        if key_schema.required && !required_seen.contains(key) {
            errors.push(SchemaErrorKind::MissingRequiredKey { key: key.clone(), node: node.name.clone(), span: node.span });
        }
    }
}
