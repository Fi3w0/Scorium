//! Byte-offset source spans, shared by every stage from lexing through
//! diagnostics. Line/column info is derived on demand by `miette` from the
//! offsets plus the original source text, so nothing here tracks lines.

/// A half-open byte range `[start, end)` into a [`crate::Source`]'s text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        debug_assert!(start <= end);
        Self { start, end }
    }

    /// A zero-width span at `pos`, used for "expected X here" diagnostics.
    pub fn at(pos: u32) -> Self {
        Self { start: pos, end: pos }
    }

    /// The smallest span covering both `self` and `other`.
    pub fn join(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub fn as_range(self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }
}

impl From<Span> for miette::SourceSpan {
    fn from(s: Span) -> Self {
        (s.start as usize, s.len() as usize).into()
    }
}

/// A node paired with the span of source text it was parsed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}
