//! `scorium`: the command-line front end for the Scorium configuration
//! language.
//!
//! Subcommands:
//!
//! ```text
//! scorium check <file>        parse + evaluate; report diagnostics
//! scorium parse <file>        print the parsed syntax tree
//! scorium fmt <file>          format a file in place
//! scorium fmt --check <file>  exit non-zero if a file isn't formatted
//! scorium eval <file>         print the evaluated configuration tree
//! ```
//!
//! `check` and `eval` run against a *generic* runtime: no host functions
//! or schema are attached, because those are application-specific and a
//! real embedding application supplies its own. Control flow, variables,
//! arithmetic, includes, and `script { }` blocks all work without a host;
//! only host-registered operations are unavailable. See `docs/EMBEDDING.md`
//! for how a host wires those in.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use scorium_core::entry::Entry;
use scorium_core::{diagnostic, parse, Source, Value};
use scorium_format::format;
use scorium_lua::Runtime;

#[derive(Parser)]
#[command(name = "scorium", version, about = "Check, format, parse, and evaluate .scor configuration files")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse and evaluate a file, reporting any diagnostics found.
    Check {
        /// Path to a `.scor` file.
        path: PathBuf,
    },
    /// Parse a file and print its syntax tree for debugging.
    Parse {
        /// Path to a `.scor` file.
        path: PathBuf,
    },
    /// Format a file. Without `--check`, the file is rewritten in place.
    Fmt {
        /// Path to a `.scor` file.
        path: PathBuf,
        /// Exit non-zero without writing if the file isn't already formatted.
        #[arg(long)]
        check: bool,
    },
    /// Parse, evaluate, and print the resulting configuration tree.
    Eval {
        /// Path to a `.scor` file.
        path: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { path } => run_check(&path),
        Command::Parse { path } => run_parse(&path),
        Command::Fmt { path, check } => run_fmt(&path, check),
        Command::Eval { path } => run_eval(&path),
    }
}

/// Reads a `.scor` file into a [`Source`], or prints an error and returns a
/// failure code. Keeping this in one place means every subcommand reports
/// a missing/unreadable file the same way.
fn load(path: &Path) -> Result<Source, ExitCode> {
    match Source::from_path(path) {
        Ok(source) => Ok(source),
        Err(err) => {
            eprintln!("error: cannot read {}: {err}", path.display());
            Err(ExitCode::FAILURE)
        }
    }
}

/// The directory a file's relative `include "..."` paths resolve against.
/// Falls back to the current directory for the (unusual) case of a file
/// with no parent.
fn base_dir(path: &Path) -> &Path {
    path.parent().unwrap_or(Path::new("."))
}

/// Reports a parse error through `miette` with the source excerpt attached.
fn print_parse_error(err: scorium_core::SyntaxError, source: &Source) {
    eprintln!("{:?}", diagnostic::with_source(err, source));
}

fn run_check(path: &Path) -> ExitCode {
    let source = match load(path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let doc = match parse(&source) {
        Ok(doc) => doc,
        Err(err) => {
            print_parse_error(err, &source);
            return ExitCode::FAILURE;
        }
    };
    let runtime = match Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("error: failed to start evaluation runtime: {err}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.evaluate(&doc, &source, base_dir(path)) {
        Ok(output) => {
            for warning in &output.warnings {
                eprintln!("warning: {warning}");
            }
            println!(
                "{}: ok ({} entries, generic runtime -- no schema or host functions attached)",
                path.display(),
                output.entries.len()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{:?}", err.report());
            ExitCode::FAILURE
        }
    }
}

fn run_parse(path: &Path) -> ExitCode {
    let source = match load(path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    match parse(&source) {
        Ok(doc) => {
            println!("{:#?}", doc);
            ExitCode::SUCCESS
        }
        Err(err) => {
            print_parse_error(err, &source);
            ExitCode::FAILURE
        }
    }
}

fn run_fmt(path: &Path, check: bool) -> ExitCode {
    let source = match load(path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let doc = match parse(&source) {
        Ok(doc) => doc,
        Err(err) => {
            print_parse_error(err, &source);
            return ExitCode::FAILURE;
        }
    };
    let formatted = format(&doc);
    if check {
        if formatted == source.text() {
            ExitCode::SUCCESS
        } else {
            eprintln!("{}: not formatted (run `scorium fmt {}` to fix)", path.display(), path.display());
            ExitCode::FAILURE
        }
    } else {
        match std::fs::write(path, formatted.as_bytes()) {
            Ok(()) => {
                println!("{}: formatted", path.display());
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("error: cannot write {}: {err}", path.display());
                ExitCode::FAILURE
            }
        }
    }
}

fn run_eval(path: &Path) -> ExitCode {
    let source = match load(path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let doc = match parse(&source) {
        Ok(doc) => doc,
        Err(err) => {
            print_parse_error(err, &source);
            return ExitCode::FAILURE;
        }
    };
    let runtime = match Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("error: failed to start evaluation runtime: {err}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.evaluate(&doc, &source, base_dir(path)) {
        Ok(output) => {
            for warning in &output.warnings {
                eprintln!("warning: {warning}");
            }
            print_entries(&output.entries, 0);
            // Make it explicit that this is the generic tree: host-registered
            // functions and schema validation are an embedding application's
            // responsibility, not something the standalone CLI can provide.
            eprintln!(
                "(evaluated against the generic runtime: {} entries; host \
                 functions and schema validation require an embedding application)",
                output.entries.len()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{:?}", err.report());
            ExitCode::FAILURE
        }
    }
}

/// Prints an evaluated entry tree as an indented, human-readable outline.
/// Distinct from the `Debug` impl (which exposes every field): this is the
/// shape an embedding application's user would recognise as "the config".
fn print_entries(entries: &[Entry], depth: usize) {
    let indent = "  ".repeat(depth);
    for entry in entries {
        match entry {
            Entry::Leaf(leaf) => {
                println!("{}{} = {}", indent, leaf.key, format_value(&leaf.value));
            }
            Entry::Node(node) => {
                if let Some(header) = &node.header {
                    println!("{}{} {} {{", indent, node.name, header);
                } else {
                    println!("{}{} {{", indent, node.name);
                }
                print_entries(&node.children, depth + 1);
                println!("{indent}}}");
            }
            Entry::Include(inc) => {
                if let Some(resolved) = &inc.resolved_path {
                    println!("{}include {:?} -> {}", indent, inc.path, resolved.display());
                } else {
                    println!("{}include {:?}", indent, inc.path);
                }
            }
            Entry::HostCall(call) => {
                println!("{}{}()", indent, call.name);
            }
        }
    }
}

/// Renders a value for the `eval` outline. Strings are quoted so they're
/// distinguishable from booleans/numbers/identifiers, matching how a reader
/// would expect config output to look.
fn format_value(value: &Value) -> String {
    match value {
        Value::Str(s) => format!("{s:?}"),
        _ => value.to_string(),
    }
}
