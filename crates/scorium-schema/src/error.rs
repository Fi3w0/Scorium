//! Validation diagnostics, including Levenshtein-based typo suggestions
//! for misspelled node/key names.

use miette::Diagnostic;
use scorium_core::{Source, Span};
use thiserror::Error;

#[derive(Debug, Clone, Error, Diagnostic)]
pub enum SchemaErrorKind {
    #[error("unknown node `{name}`")]
    #[diagnostic(code(scorium::schema::unknown_node))]
    UnknownNode {
        name: String,
        suggestion: Option<String>,
        #[label("not declared in the schema")]
        span: Span,
    },

    #[error("unknown key `{name}`")]
    #[diagnostic(code(scorium::schema::unknown_key))]
    UnknownKey {
        name: String,
        node: String,
        suggestion: Option<String>,
        #[label("not declared for `{node}`")]
        span: Span,
    },

    #[error("wrong type for `{key}`: {message}")]
    #[diagnostic(code(scorium::schema::wrong_type))]
    WrongType {
        key: String,
        message: String,
        #[label("here")]
        span: Span,
    },

    #[error("missing required key `{key}` in `{node}`")]
    #[diagnostic(code(scorium::schema::missing_required_key))]
    MissingRequiredKey {
        key: String,
        node: String,
        #[label("this node is missing `{key}`")]
        span: Span,
    },

    #[error("duplicate key `{key}`")]
    #[diagnostic(code(scorium::schema::duplicate_key))]
    DuplicateKey {
        key: String,
        #[label("also set here")]
        span: Span,
        #[label("first set here")]
        first_span: Span,
    },

    #[error("invalid header for node `{node}`: {message}")]
    #[diagnostic(code(scorium::schema::invalid_header))]
    InvalidHeader {
        node: String,
        message: String,
        #[label("here")]
        span: Span,
    },
}

impl SchemaErrorKind {
    /// A rendering-ready `miette::Report` for this one error, given the
    /// source it was found in.
    pub fn report(&self, source: &Source) -> miette::Report {
        miette::Report::new(self.clone()).with_source_code(source.named_source())
    }
}

/// Every problem found by [`crate::validate`], in source order.
#[derive(Debug, Default)]
pub struct ValidationResult {
    pub errors: Vec<SchemaErrorKind>,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn reports<'a>(&'a self, source: &'a Source) -> impl Iterator<Item = miette::Report> + 'a {
        self.errors.iter().map(move |e| e.report(source))
    }
}

/// The Levenshtein (edit) distance between two strings, used to suggest
/// a likely-intended key/node name for a typo. Small enough, and used
/// rarely enough (only on a validation failure), that pulling in a crate
/// for this would be the over-engineered choice.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev_diag = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            let up_left = prev_diag;
            prev_diag = row[j];
            row[j] = (row[j] + 1).min(row[j - 1] + 1).min(up_left + cost);
        }
    }
    row[b.len()]
}

/// The closest candidate to `name` worth suggesting, or `None` if
/// nothing is close enough to be more helpful than confusing.
pub fn suggest<'a>(name: &str, candidates: impl Iterator<Item = &'a String>) -> Option<String> {
    candidates
        .map(|c| (c, levenshtein(name, c)))
        .filter(|(c, dist)| *dist > 0 && *dist <= 2 && *dist < name.len().max(c.len()))
        .min_by_key(|(_, dist)| *dist)
        .map(|(c, _)| c.clone())
}
