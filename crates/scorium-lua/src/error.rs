//! Evaluation-time diagnostics: everything that can go wrong once syntax
//! is already valid -- undefined variables, wrong types, sandbox limits,
//! include failures.

use miette::Diagnostic;
use scorium_core::{Source, Span};
use thiserror::Error;

#[derive(Debug, Clone, Error, Diagnostic)]
pub enum EvalErrorKind {
    #[error("`${name}` is not defined")]
    #[diagnostic(code(scorium::eval::undefined_interpolation), help("define it first with `@{name} = value`"))]
    UndefinedInterpolation {
        name: String,
        #[label("not defined")]
        span: Span,
    },

    #[error("unknown function `{name}`")]
    #[diagnostic(code(scorium::eval::unknown_function))]
    UnknownFunction {
        name: String,
        #[label("not registered by the host, and not a Scorium `fn`")]
        span: Span,
    },

    #[error("{message}")]
    #[diagnostic(code(scorium::eval::type_error))]
    TypeError {
        message: String,
        #[label("here")]
        span: Span,
    },

    #[error("division by zero")]
    #[diagnostic(code(scorium::eval::division_by_zero))]
    DivisionByZero {
        #[label("this division")]
        span: Span,
    },

    #[error("integer arithmetic overflow")]
    #[diagnostic(code(scorium::eval::arithmetic_overflow), help("use smaller integer operands or a float"))]
    ArithmeticOverflow {
        #[label("this operation exceeds the integer range")]
        span: Span,
    },

    #[error("`include` is disabled by the host application")]
    #[diagnostic(code(scorium::eval::includes_disabled))]
    IncludesDisabled {
        #[label("include not allowed here")]
        span: Span,
    },

    #[error("include path `{path}` is not allowed by the host's path policy")]
    #[diagnostic(code(scorium::eval::include_path_denied), help("paths may not traverse above the including file's directory"))]
    IncludePathDenied {
        path: String,
        #[label("denied by policy")]
        span: Span,
    },

    #[error("include cycle detected: {chain}")]
    #[diagnostic(code(scorium::eval::include_cycle))]
    IncludeCycle {
        chain: String,
        #[label("this include closes the cycle")]
        span: Span,
    },

    #[error("failed to read include `{path}`: {message}")]
    #[diagnostic(code(scorium::eval::include_io))]
    IncludeIo {
        path: String,
        message: String,
        #[label("could not read this file")]
        span: Span,
    },

    #[error("include `{path}` failed to parse: {message}")]
    #[diagnostic(code(scorium::eval::include_parse))]
    IncludeParse {
        path: String,
        message: String,
        #[label("included here")]
        span: Span,
    },

    #[error("script execution failed: {message}")]
    #[diagnostic(code(scorium::eval::script_error))]
    ScriptError {
        message: String,
        #[label("in this script block")]
        span: Span,
    },

    #[error("loop budget exceeded ({limit} iterations); this program may not terminate")]
    #[diagnostic(
        code(scorium::eval::loop_budget_exceeded),
        help("Scorium caps total loop iterations per evaluation as a sandbox limit")
    )]
    LoopBudgetExceeded {
        limit: u64,
        #[label("still looping here")]
        span: Span,
    },

    #[error("function call depth exceeded ({limit}); this function may recurse forever")]
    #[diagnostic(code(scorium::eval::call_depth_exceeded), help("Scorium caps nested `fn` calls as a sandbox limit"))]
    CallDepthExceeded {
        limit: u32,
        #[label("called too deeply here")]
        span: Span,
    },
}

/// An [`EvalErrorKind`] paired with the source it happened in -- needed
/// because, with includes, an error can originate in a file other than
/// the one the caller started with.
#[derive(Debug, Clone, Error)]
#[error("{kind}")]
pub struct EvalError {
    pub kind: EvalErrorKind,
    /// The source the error happened in. Named `src`, not `source`,
    /// because `thiserror` treats a field literally named `source` as
    /// the `std::error::Error::source()` chain -- this is source *text*
    /// for diagnostic rendering, not a wrapped error.
    pub src: Source,
}

impl EvalError {
    pub fn new(kind: EvalErrorKind, src: Source) -> Self {
        Self { kind, src }
    }

    /// A ready-to-print `miette::Report` with the originating source
    /// attached, regardless of which included file the error came from.
    pub fn report(&self) -> miette::Report {
        miette::Report::new(self.kind.clone()).with_source_code(self.src.named_source())
    }
}
