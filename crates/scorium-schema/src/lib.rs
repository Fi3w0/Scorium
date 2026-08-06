//! `scorium-schema`: lets a host application declare which nodes and
//! keys a Scorium document may contain, then validate an evaluated
//! [`scorium_core::entry::Entry`] tree against that declaration.
//!
//! ```
//! use scorium_schema::{Schema, NodeSchema, ValueType};
//!
//! let schema = Schema::builder()
//!     .node(
//!         "server",
//!         NodeSchema::builder()
//!             .key("host", ValueType::String)
//!             .required_key("port", ValueType::Integer)
//!             .key("timeout", ValueType::Duration)
//!             .build(),
//!     )
//!     .build();
//! ```
//!
//! There is deliberately no schema *file* format -- the task this crate
//! solves (unknown-key/type/duplicate checking with typo suggestions) is
//! well served by a plain Rust builder API, and the brief for this
//! project is explicit that inventing an unstable schema language isn't
//! worth doing without a concrete need for one.

mod builder;
mod error;
mod types;
mod validate;

pub use builder::{DuplicateKeyPolicy, KeySchema, NodeSchema, NodeSchemaBuilder, Schema, SchemaBuilder};
pub use error::{SchemaErrorKind, ValidationResult};
pub use types::{CustomType, ValueType};
pub use validate::validate;

impl Schema {
    /// Validates an evaluated entry tree against this schema, collecting
    /// every problem found (not just the first).
    pub fn validate(&self, entries: &[scorium_core::entry::Entry]) -> ValidationResult {
        validate::validate(self, entries)
    }
}
