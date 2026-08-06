use scorium_core::ast::{ExprKind, ItemKind, StrLit, StrPart};
use scorium_core::Source;

fn parse_ok(src: &str) -> scorium_core::Document {
    let source = Source::new("<test>", src);
    match scorium_core::parse(&source) {
        Ok(doc) => doc,
        Err(e) => panic!("expected {src:?} to parse, got: {e}"),
    }
}

fn parse_err(src: &str) -> scorium_core::SyntaxError {
    let source = Source::new("<test>", src);
    scorium_core::parse(&source).expect_err("expected a parse error")
}

#[test]
fn basic_node_and_leaves() {
    let doc = parse_ok(
        r#"
        server {
            port = 8080
            timeout = 5s
            enabled = true
        }
        "#,
    );
    assert_eq!(doc.items.len(), 1);
    let ItemKind::Node(node) = &doc.items[0].kind else { panic!("expected node") };
    assert_eq!(node.name, "server");
    assert_eq!(node.body.len(), 3);
}

#[test]
fn nested_nodes() {
    let doc = parse_ok(
        r#"
        server {
            tls {
                enabled = true
                certificate = cert.pem
            }
        }
        "#,
    );
    let ItemKind::Node(server) = &doc.items[0].kind else { panic!() };
    let ItemKind::Node(tls) = &server.body[0].kind else { panic!() };
    assert_eq!(tls.name, "tls");
    let ItemKind::Leaf(cert) = &tls.body[1].kind else { panic!() };
    assert_eq!(cert.key, "certificate");
    match &cert.value.kind {
        ExprKind::Str(StrLit::Bare(parts)) => {
            assert_eq!(parts, &vec![StrPart::Lit("cert.pem".to_string())]);
        }
        other => panic!("expected bare string, got {other:?}"),
    }
}

#[test]
fn typed_leaves() {
    let doc = parse_ok(
        r#"
        leaf_int = 8080
        leaf_bool = true
        leaf_bare = zsh
        leaf_quoted = "$HOME/example"
        leaf_color = #8EDDFF
        leaf_duration = 600ms
        leaf_list = [one, two, "three four"]
        "#,
    );
    assert_eq!(doc.items.len(), 7);
    let get = |i: usize| -> &scorium_core::ast::LeafDecl {
        let ItemKind::Leaf(l) = &doc.items[i].kind else { panic!() };
        l
    };
    assert!(matches!(get(0).value.kind, ExprKind::Int(8080)));
    assert!(matches!(get(1).value.kind, ExprKind::Bool(true)));
    assert!(matches!(&get(3).value.kind, ExprKind::Str(StrLit::Quoted(s)) if s == "$HOME/example"));
    assert!(matches!(&get(4).value.kind, ExprKind::Color(hex) if hex == "8EDDFF"));
    assert!(matches!(get(5).value.kind, ExprKind::Duration(600.0, _)));
    let ExprKind::List(items) = &get(6).value.kind else { panic!() };
    assert_eq!(items.len(), 3);
}

#[test]
fn quoted_strings_are_never_interpolated() {
    let doc = parse_ok(r#"path = "$HOME/example""#);
    let ItemKind::Leaf(leaf) = &doc.items[0].kind else { panic!() };
    assert!(matches!(&leaf.value.kind, ExprKind::Str(StrLit::Quoted(s)) if s == "$HOME/example"));
}

#[test]
fn bare_string_interpolation_parts() {
    let doc = parse_ok("binding = $mod+Return");
    let ItemKind::Leaf(leaf) = &doc.items[0].kind else { panic!() };
    let ExprKind::Str(StrLit::Bare(parts)) = &leaf.value.kind else { panic!("expected bare string") };
    assert_eq!(parts.len(), 2);
    assert!(matches!(&parts[0], StrPart::Interp(name, _) if name == "mod"));
    assert!(matches!(&parts[1], StrPart::Lit(s) if s == "+Return"));
}

#[test]
fn variable_definition() {
    let doc = parse_ok("@mod = SUPER");
    let ItemKind::VarDef(v) = &doc.items[0].kind else { panic!() };
    assert_eq!(v.name, "mod");
}

#[test]
fn expression_arithmetic() {
    let doc = parse_ok("size = base * 2");
    let ItemKind::Leaf(leaf) = &doc.items[0].kind else { panic!() };
    let ExprKind::Binary(op, lhs, rhs) = &leaf.value.kind else { panic!("expected binary expr") };
    assert_eq!(*op, scorium_core::ast::BinOp::Mul);
    assert!(matches!(&lhs.kind, ExprKind::Ident(n) if n == "base"));
    assert!(matches!(rhs.kind, ExprKind::Int(2)));
}

#[test]
fn squeezed_multiply_is_an_error() {
    let err = parse_err("size = base*2");
    assert!(matches!(err, scorium_core::SyntaxError::SqueezedOperator { .. }), "got: {err:?}");
}

#[test]
fn unspaced_plus_is_a_bare_string() {
    // SUPER+Return must NOT error: `+`/`-` stay embeddable unspaced.
    let doc = parse_ok("key = SUPER+Return");
    let ItemKind::Leaf(leaf) = &doc.items[0].kind else { panic!() };
    assert!(matches!(&leaf.value.kind, ExprKind::Str(_)));
}

#[test]
fn dollar_in_expression_errors() {
    let err = parse_err("gaps = $base * 2");
    assert!(matches!(err, scorium_core::SyntaxError::DollarInExpression { .. }), "got: {err:?}");
}

#[test]
fn at_in_expression_errors() {
    let err = parse_err("gaps = @base * 2");
    assert!(matches!(err, scorium_core::SyntaxError::AtInExpression { .. }), "got: {err:?}");
}

#[test]
fn condition_bare_word_is_ident_not_string() {
    let doc = parse_ok("if debug then\n    x = 1\nend");
    let ItemKind::If(stmt) = &doc.items[0].kind else { panic!() };
    assert!(matches!(&stmt.cond.kind, ExprKind::Ident(n) if n == "debug"));
}

#[test]
fn if_elseif_else() {
    let doc = parse_ok(
        r#"
        if environment == production then
            server {
                workers = 8
            }
        elseif environment == staging then
            server {
                workers = 4
            }
        else
            server {
                workers = 2
            }
        end
        "#,
    );
    let ItemKind::If(stmt) = &doc.items[0].kind else { panic!() };
    assert_eq!(stmt.elifs.len(), 1);
    assert!(stmt.else_body.is_some());
}

#[test]
fn for_loop() {
    let doc = parse_ok(
        r#"
        for i = 1, 3 do
            server {
                port = base_port + i
                name = node-$i
            }
        end
        "#,
    );
    let ItemKind::For(stmt) = &doc.items[0].kind else { panic!() };
    assert_eq!(stmt.var, "i");
    assert!(matches!(stmt.start.kind, ExprKind::Int(1)));
    assert!(matches!(stmt.stop.kind, ExprKind::Int(3)));
}

#[test]
fn function_definition() {
    let doc = parse_ok(
        r#"
        fn service(name, port) {
            service {
                id = $name
                port = port
            }
        }
        "#,
    );
    let ItemKind::FnDef(f) = &doc.items[0].kind else { panic!() };
    assert_eq!(f.name, "service");
    assert_eq!(f.params, vec!["name".to_string(), "port".to_string()]);
}

#[test]
fn call_statement() {
    let doc = parse_ok("service(web, 8080)");
    assert!(matches!(&doc.items[0].kind, ItemKind::Call(_)));
}

#[test]
fn include_statement() {
    let doc = parse_ok(r#"include "theme.scor""#);
    let ItemKind::Include(inc) = &doc.items[0].kind else { panic!() };
    assert!(matches!(&inc.path, StrLit::Quoted(s) if s == "theme.scor"));
}

#[test]
fn script_block_raw_capture() {
    let doc = parse_ok(
        r#"
        script {
            local x = 1 + 1
        }
        "#,
    );
    let ItemKind::Script(s) = &doc.items[0].kind else { panic!() };
    assert!(s.raw.contains("local x = 1 + 1"));
}

#[test]
fn script_block_with_braces_and_lua_strings() {
    let doc = parse_ok(
        r#"
        script {
            local t = { a = 1, b = 'x{y}z' }
        }
        "#,
    );
    let ItemKind::Script(s) = &doc.items[0].kind else { panic!() };
    assert!(s.raw.contains("x{y}z"));
}

#[test]
fn comments_are_preserved_as_trivia() {
    let doc = parse_ok(
        "# leading comment\nport = 8080 # trailing comment\n-- lua style\n--[[ block ]]\ntimeout = 5s",
    );
    assert_eq!(doc.items.len(), 2);
    assert_eq!(doc.items[0].trivia.leading.len(), 1);
    assert!(doc.items[0].trivia.trailing.is_some());
    assert_eq!(doc.items[1].trivia.leading.len(), 2);
}

#[test]
fn hash_comment_vs_color_literal() {
    let doc = parse_ok("primary = #8EDDFF\n# just a comment\ntimeout = 5s");
    assert_eq!(doc.items.len(), 2);
    let ItemKind::Leaf(leaf) = &doc.items[0].kind else { panic!() };
    assert!(matches!(&leaf.value.kind, ExprKind::Color(_)));
}

#[test]
fn malformed_syntax_reports_span() {
    let err = parse_err("server {\n    port = \n}");
    assert!(matches!(err, scorium_core::SyntaxError::UnexpectedToken { .. }));
}

#[test]
fn node_header() {
    let doc = parse_ok(
        r#"
        output eDP-1 {
            enabled = true
        }
        "#,
    );
    let ItemKind::Node(node) = &doc.items[0].kind else { panic!() };
    assert_eq!(node.name, "output");
    assert!(node.header.is_some());
}
