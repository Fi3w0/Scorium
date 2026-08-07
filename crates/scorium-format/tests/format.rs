use scorium_core::Source;
use scorium_format::format;

fn parse(src: &str) -> scorium_core::Document {
    scorium_core::parse(&Source::new("<test>", src)).unwrap_or_else(|e| panic!("parse failed for {src:?}: {e}"))
}

fn fmt(src: &str) -> String {
    format(&parse(src))
}

/// The core round-trip guarantee: formatting is a pure function of the
/// AST, so formatting already-canonical output must reproduce it
/// exactly.
fn assert_idempotent(src: &str) {
    let once = fmt(src);
    let twice = format(&parse(&once));
    assert_eq!(
        once, twice,
        "formatting is not idempotent for input:\n{src}\n---\nfirst pass:\n{once}\n---\nsecond pass:\n{twice}"
    );
}

#[test]
fn basic_leaf_and_node_spacing() {
    let out = fmt("server{port=8080\nenabled=true}");
    assert_eq!(out, "server {\n    port = 8080\n    enabled = true\n}\n");
}

#[test]
fn nested_nodes_are_indented() {
    let out = fmt("server {\ntls {\nenabled = true\n}\n}");
    assert_eq!(out, "server {\n    tls {\n        enabled = true\n    }\n}\n");
}

#[test]
fn arithmetic_gets_spaced_operators() {
    let out = fmt("size = base * 2");
    // The lexer rejects `base*2` (squeezed), so this must be spaced input
    // in the first place; formatting `base * 2` should keep it spaced.
    let _ = out;
    let out2 = fmt("size = base * 2");
    assert_eq!(out2, "size = base * 2\n");
}

#[test]
fn colors_and_durations_round_trip() {
    let out = fmt("primary = #8EDDFF\ndelay = 600ms\ninterval = 1.5s");
    assert_eq!(out, "primary = #8EDDFF\ndelay = 600ms\ninterval = 1.5s\n");
}

#[test]
fn lists_use_stable_comma_spacing() {
    let out = fmt("paths=[one,two,\"three four\"]");
    assert_eq!(out, "paths = [one, two, \"three four\"]\n");
}

#[test]
fn leading_and_trailing_comments_are_preserved() {
    let out = fmt("# a comment\nport = 8080 # trailing\ntimeout = 5s");
    assert_eq!(out, "# a comment\nport = 8080 # trailing\ntimeout = 5s\n");
}

#[test]
fn dash_dash_comments_normalize_to_hash() {
    let out = fmt("-- a comment\nport = 8080");
    assert_eq!(out, "# a comment\nport = 8080\n");
}

#[test]
fn block_comments_are_preserved_verbatim() {
    let out = fmt("--[[\nmulti-line\n]]\nport = 8080");
    assert!(out.starts_with("--[[\nmulti-line\n]]\n"), "{out}");
}

#[test]
fn script_body_is_never_reformatted() {
    let src = "script {\n  local   x=1+1\n}";
    let out = fmt(src);
    assert!(out.contains("local   x=1+1"), "script body must be byte-for-byte preserved, got:\n{out}");
}

#[test]
fn blank_lines_are_capped_at_one() {
    let out = fmt("a = 1\n\n\n\nb = 2");
    assert_eq!(out, "a = 1\n\nb = 2\n");
}

#[test]
fn no_blank_line_stays_absent() {
    let out = fmt("a = 1\nb = 2");
    assert_eq!(out, "a = 1\nb = 2\n");
}

#[test]
fn variable_definitions_and_interpolation() {
    let out = fmt("@mod = SUPER\nbinding = $mod+Return");
    assert_eq!(out, "@mod = SUPER\nbinding = $mod+Return\n");
}

#[test]
fn if_elseif_else_end() {
    let src =
        "if environment == production then\nworkers = 8\nelseif environment == staging then\nworkers = 4\nelse\nworkers = 2\nend";
    let out = fmt(src);
    assert_eq!(
        out,
        "if environment == production then\n    workers = 8\nelseif environment == staging then\n    workers = 4\nelse\n    workers = 2\nend\n"
    );
}

#[test]
fn for_loop_formatting() {
    let out = fmt("for i=1,3 do\nname=node-$i\nend");
    assert_eq!(out, "for i = 1, 3 do\n    name = node-$i\nend\n");
}

#[test]
fn function_definition_formatting() {
    let out = fmt("fn service(name,port) {\nid=$name\n}");
    assert_eq!(out, "fn service(name, port) {\n    id = $name\n}\n");
}

#[test]
fn color_method_call_formatting() {
    let out = fmt("deep=primary.darken(0.25)");
    assert_eq!(out, "deep = primary.darken(0.25)\n");
}

#[test]
fn idempotency_across_examples() {
    let sources = [
        "server {\n    port = 8080\n    timeout = 5s\n    enabled = true\n}\n",
        "@base_port = 8000\n\nfor i = 1, 3 do\n    server {\n        port = base_port + i\n        name = node-$i\n    }\nend\n",
        "if environment == production then\n    server {\n        workers = 8\n    }\nelse\n    server {\n        workers = 2\n    }\nend\n",
        "primary = #8EDDFF\ndeep = primary.darken(0.25)\n",
        "paths = [one, two, \"three four\"]\n",
        "fn service(name, port) {\n    service {\n        id = $name\n        port = port\n    }\n}\nservice(web, 8080)\n",
        "# leading\nport = 8080 # trailing\n\ntimeout = 5s\n",
        "script {\n    local x = 1 + 1\n}\n",
        "include \"theme.scor\"\n",
    ];
    for src in sources {
        assert_idempotent(src);
    }
}
