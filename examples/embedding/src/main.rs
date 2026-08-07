//! A complete embedding of Scorium: parse a source string, evaluate it
//! against a host runtime that registers a value and a function, validate
//! the result against a schema, and inspect the evaluated tree.
//!
//! Run it with:
//!
//! ```text
//! cargo run -p scorium-embedding-example
//! ```
//!
//! This mirrors what a real embedding application does -- only the host
//! functions and schema are toy ones. See `docs/EMBEDDING.md` for the full
//! API surface.

use scorium_core::diagnostic;
use scorium_core::{parse, Source, Value};
use scorium_lua::{Runtime, RuntimeOptions};
use scorium_schema::{NodeSchema, Schema, ValueType};
use std::path::Path;

/// The configuration source. In a real application this comes from a file
/// the user chose; here it is inlined so the example is self-contained.
const CONFIG: &str = "\
@base_port = 8000

# `environment` is a host-registered value and `double` is a host-registered
# function (see main) -- both reach expressions through the same registry.
server {
    host = localhost
    port = double(base_port)
    timeout = 5s
    enabled = environment == production
}
";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = Source::new("<embedding-example>", CONFIG);

    // 1. Parse: source text -> AST.
    let doc = match parse(&source) {
        Ok(doc) => doc,
        Err(err) => {
            // `miette` renders the error with a source excerpt and caret.
            eprintln!("{:?}", diagnostic::with_source(err, &source));
            std::process::exit(1);
        }
    };

    // 2. Build the sandboxed runtime and register host capabilities.
    let mut runtime = Runtime::with_options(RuntimeOptions::default())?;
    runtime.register_value("environment", Value::Str("production".into()));
    runtime.register_function("double", |args| match args.first() {
        Some(Value::Int(n)) => Ok(Value::Int(n * 2)),
        Some(Value::Float(f)) => Ok(Value::Float(f * 2.0)),
        _ => Err("double() expects one number".into()),
    });

    // 3. Evaluate: AST -> typed entry tree. The base directory is where
    //    relative `include "..."` paths resolve; `<inline>` has none.
    let output = runtime.evaluate(&doc, &source, Path::new("."))?;
    for warning in &output.warnings {
        eprintln!("warning: {warning}");
    }

    // 4. Validate against a schema the host application defines.
    let schema = Schema::builder()
        .node(
            "server",
            NodeSchema::builder()
                .required_key("host", ValueType::String)
                .required_key("port", ValueType::Integer)
                .key("timeout", ValueType::Duration)
                .key("enabled", ValueType::Boolean)
                .build(),
        )
        .build();

    let result = schema.validate(&output.entries);
    if result.is_valid() {
        println!("configuration is valid");
    } else {
        for report in result.reports(&source) {
            eprintln!("{report:?}");
        }
    }

    // 5. Inspect the evaluated tree. This is where a host would apply the
    //    configuration; here we just print it.
    println!("evaluated entries:");
    for entry in &output.entries {
        println!("  {entry:?}");
    }

    Ok(())
}
