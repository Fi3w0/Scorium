//! A named, in-memory Scorium source file.

use std::path::Path;
use std::sync::Arc;

/// One `.scor` source: a name (a file path, or a synthetic name like
/// `<inline>`) and its text. Cheap to clone; the text is reference-counted
/// so diagnostics can hold their own copy for rendering.
#[derive(Debug, Clone)]
pub struct Source {
    name: Arc<str>,
    text: Arc<str>,
}

impl Source {
    pub fn new(name: impl Into<Arc<str>>, text: impl Into<Arc<str>>) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
        }
    }

    pub fn from_path(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        Ok(Self::new(path.display().to_string(), text))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// A [`miette::NamedSource`] snapshot for attaching to a diagnostic.
    pub fn named_source(&self) -> miette::NamedSource<String> {
        miette::NamedSource::new(&*self.name, self.text.to_string())
    }
}
