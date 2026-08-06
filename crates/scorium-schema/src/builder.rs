//! The schema definition API: `Schema::builder()...build()`, and the
//! same shape one level down for `NodeSchema`.

use std::collections::HashMap;

use crate::types::ValueType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateKeyPolicy {
    /// A repeated key is a validation error.
    Error,
    /// A repeated key is allowed; the last occurrence is authoritative.
    LastWins,
    /// A repeated key is allowed; the first occurrence is authoritative.
    FirstWins,
}

#[derive(Clone)]
pub struct KeySchema {
    pub value_type: ValueType,
    pub required: bool,
}

#[derive(Clone)]
pub struct NodeSchema {
    pub(crate) keys: HashMap<String, KeySchema>,
    pub(crate) children: HashMap<String, NodeSchema>,
    pub(crate) allow_unknown_keys: bool,
    pub(crate) duplicate_key_policy: DuplicateKeyPolicy,
}

impl NodeSchema {
    pub fn builder() -> NodeSchemaBuilder {
        NodeSchemaBuilder::new()
    }
}

pub struct NodeSchemaBuilder {
    inner: NodeSchema,
}

impl Default for NodeSchemaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeSchemaBuilder {
    pub fn new() -> Self {
        Self {
            inner: NodeSchema {
                keys: HashMap::new(),
                children: HashMap::new(),
                allow_unknown_keys: false,
                duplicate_key_policy: DuplicateKeyPolicy::Error,
            },
        }
    }

    /// Declares an optional key.
    pub fn key(mut self, name: impl Into<String>, value_type: ValueType) -> Self {
        self.inner.keys.insert(name.into(), KeySchema { value_type, required: false });
        self
    }

    /// Declares a required key: missing it is a validation error.
    pub fn required_key(mut self, name: impl Into<String>, value_type: ValueType) -> Self {
        self.inner.keys.insert(name.into(), KeySchema { value_type, required: true });
        self
    }

    /// Declares a valid nested node name within this node's body.
    pub fn node(mut self, name: impl Into<String>, schema: NodeSchema) -> Self {
        self.inner.children.insert(name.into(), schema);
        self
    }

    pub fn allow_unknown_keys(mut self, allow: bool) -> Self {
        self.inner.allow_unknown_keys = allow;
        self
    }

    pub fn duplicate_key_policy(mut self, policy: DuplicateKeyPolicy) -> Self {
        self.inner.duplicate_key_policy = policy;
        self
    }

    pub fn build(self) -> NodeSchema {
        self.inner
    }
}

#[derive(Clone, Default)]
pub struct Schema {
    pub(crate) nodes: HashMap<String, NodeSchema>,
    pub(crate) root_keys: HashMap<String, KeySchema>,
    pub(crate) allow_unknown_nodes: bool,
}

impl Schema {
    pub fn builder() -> SchemaBuilder {
        SchemaBuilder::default()
    }
}

#[derive(Default)]
pub struct SchemaBuilder {
    inner: Schema,
}

impl SchemaBuilder {
    /// Declares a valid top-level node name.
    pub fn node(mut self, name: impl Into<String>, schema: NodeSchema) -> Self {
        self.inner.nodes.insert(name.into(), schema);
        self
    }

    /// Declares a valid top-level leaf key (outside any node).
    pub fn key(mut self, name: impl Into<String>, value_type: ValueType) -> Self {
        self.inner.root_keys.insert(name.into(), KeySchema { value_type, required: false });
        self
    }

    pub fn required_key(mut self, name: impl Into<String>, value_type: ValueType) -> Self {
        self.inner.root_keys.insert(name.into(), KeySchema { value_type, required: true });
        self
    }

    pub fn allow_unknown_nodes(mut self, allow: bool) -> Self {
        self.inner.allow_unknown_nodes = allow;
        self
    }

    pub fn build(self) -> Schema {
        self.inner
    }
}
