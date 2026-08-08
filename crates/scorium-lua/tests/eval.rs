use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use scorium_core::entry::Entry;
use scorium_core::{Source, Value};
use scorium_lua::{IncludePolicy, Runtime, RuntimeOptions};

fn eval(src: &str) -> Vec<Entry> {
    eval_with(src, |_| {})
}

fn eval_with(src: &str, configure: impl FnOnce(&mut Runtime)) -> Vec<Entry> {
    let source = Source::new("<test>", src);
    let doc = scorium_core::parse(&source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let mut runtime = Runtime::new().expect("sandbox init");
    configure(&mut runtime);
    let out = runtime.evaluate(&doc, &source, Path::new(".")).unwrap_or_else(|e| panic!("eval failed: {}", e.kind));
    out.entries
}

fn eval_err(src: &str) -> scorium_lua::EvalErrorKind {
    let source = Source::new("<test>", src);
    let doc = scorium_core::parse(&source).expect("parse should succeed");
    let runtime = Runtime::new().expect("sandbox init");
    runtime.evaluate(&doc, &source, Path::new(".")).expect_err("expected eval error").kind
}

fn leaf<'a>(entries: &'a [Entry], key: &str) -> &'a Value {
    entries
        .iter()
        .find_map(|e| match e {
            Entry::Leaf(l) if l.key == key => Some(&l.value),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no leaf named `{key}`"))
}

fn node<'a>(entries: &'a [Entry], name: &str) -> &'a scorium_core::entry::NodeEntry {
    entries
        .iter()
        .find_map(|e| match e {
            Entry::Node(n) if n.name == name => Some(n),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no node named `{name}`"))
}

#[test]
fn basic_node_and_leaves() {
    let entries = eval("server {\n    port = 8080\n    enabled = true\n}");
    let server = node(&entries, "server");
    assert_eq!(leaf(&server.children, "port"), &Value::Int(8080));
    assert_eq!(leaf(&server.children, "enabled"), &Value::Bool(true));
}

#[test]
fn variable_and_interpolation() {
    let entries = eval("@mod = SUPER\nbinding = $mod+Return");
    assert_eq!(leaf(&entries, "binding"), &Value::Str("SUPER+Return".to_string()));
}

#[test]
fn unicode_variable_interpolation_preserves_following_text() {
    let entries = eval("@café = olé\nbinding = $café-x");
    assert_eq!(leaf(&entries, "binding"), &Value::Str("olé-x".into()));
}

#[test]
fn undefined_interpolation_is_an_error() {
    let kind = eval_err("binding = $nope+Return");
    assert!(matches!(kind, scorium_lua::EvalErrorKind::UndefinedInterpolation { .. }), "{kind:?}");
}

#[test]
fn arithmetic_expression() {
    let entries = eval("@base_port = 8000\nport = base_port + 1");
    assert_eq!(leaf(&entries, "port"), &Value::Int(8001));
}

#[test]
fn integer_overflow_is_a_diagnostic_instead_of_a_panic_or_wrap() {
    let kind = eval_err("value = 9223372036854775807 + 1");
    assert!(matches!(kind, scorium_lua::EvalErrorKind::ArithmeticOverflow { .. }), "{kind:?}");

    let kind = eval_err("@min = -9223372036854775807 - 1\nvalue = -min");
    assert!(matches!(kind, scorium_lua::EvalErrorKind::ArithmeticOverflow { .. }), "{kind:?}");
}

#[test]
fn minimum_integer_modulo_negative_one_is_zero() {
    let entries = eval("@min = -9223372036854775807 - 1\nvalue = min % -1");
    assert_eq!(leaf(&entries, "value"), &Value::Int(0));
}

#[test]
fn mixed_integer_float_comparisons_do_not_round_large_integers() {
    let entries = eval("equal = 9007199254740993 == 9007199254740992.0\ngreater = 9007199254740993 > 9007199254740992.0");
    assert_eq!(leaf(&entries, "equal"), &Value::Bool(false));
    assert_eq!(leaf(&entries, "greater"), &Value::Bool(true));
}

#[test]
fn condition_true_branch() {
    let entries = eval_with("if environment == production then\n    workers = 8\nelse\n    workers = 2\nend", |rt| {
        rt.register_value("environment", Value::Str("production".into()));
    });
    assert_eq!(leaf(&entries, "workers"), &Value::Int(8));
}

#[test]
fn condition_false_branch_uses_else() {
    let entries = eval_with("if environment == production then\n    workers = 8\nelse\n    workers = 2\nend", |rt| {
        rt.register_value("environment", Value::Str("staging".into()));
    });
    assert_eq!(leaf(&entries, "workers"), &Value::Int(2));
}

#[test]
fn for_loop_generates_siblings() {
    let entries = eval("for i = 1, 3 do\n    server {\n        name = node-$i\n        index = i\n    }\nend");
    let servers: Vec<_> = entries
        .iter()
        .filter_map(|e| match e {
            Entry::Node(n) if n.name == "server" => Some(n),
            _ => None,
        })
        .collect();
    assert_eq!(servers.len(), 3);
    assert_eq!(leaf(&servers[0].children, "name"), &Value::Str("node-1".into()));
    assert_eq!(leaf(&servers[0].children, "index"), &Value::Int(1));
    assert_eq!(leaf(&servers[2].children, "index"), &Value::Int(3));
}

#[test]
fn while_loop_terminates() {
    let entries = eval("local n = 0\nwhile n < 3 do\n    item {\n        value = n\n    }\n    n = n + 1\nend");
    let items: Vec<_> = entries
        .iter()
        .filter_map(|e| match e {
            Entry::Node(n) if n.name == "item" => Some(n),
            _ => None,
        })
        .collect();
    assert_eq!(items.len(), 3);
}

#[test]
fn function_definition_and_call() {
    let entries = eval(
        r#"
        fn service(name, port) {
            service {
                id = $name
                port = port
            }
        }
        service(web, 8080)
        "#,
    );
    let svc = node(&entries, "service");
    assert_eq!(leaf(&svc.children, "id"), &Value::Str("web".into()));
    assert_eq!(leaf(&svc.children, "port"), &Value::Int(8080));
}

#[test]
fn leaf_key_matching_a_fn_parameter_still_emits_a_leaf() {
    // `port` is both the fn's parameter name and a leaf key -- the leaf
    // must win here, not a silent no-op self-reassignment.
    let entries = eval(
        r#"
        fn service(port) {
            service {
                port = port
            }
        }
        service(8080)
        "#,
    );
    let svc = node(&entries, "service");
    assert_eq!(leaf(&svc.children, "port"), &Value::Int(8080));
}

#[test]
fn accumulator_local_inside_a_node_body_is_reassignable() {
    let entries = eval(
        r#"
        service {
            local total = 0
            for i = 1, 3 do
                total = total + i
            end
            sum = total
        }
        "#,
    );
    let svc = node(&entries, "service");
    assert_eq!(leaf(&svc.children, "sum"), &Value::Int(6));
}

#[test]
fn color_darken_method() {
    let entries = eval("primary = #8EDDFF\ndeep = primary.darken(1.0)");
    assert_eq!(leaf(&entries, "deep"), &Value::Color(scorium_core::ColorValue::rgb(0, 0, 0)));
}

#[test]
fn color_methods_reject_missing_wrong_or_extra_arguments() {
    for src in [
        "primary = #8EDDFF\ndeep = primary.darken()",
        "primary = #8EDDFF\ndeep = primary.darken(oops)",
        "primary = #8EDDFF\ndeep = primary.darken(0.2, 0.3)",
    ] {
        let kind = eval_err(src);
        assert!(matches!(kind, scorium_lua::EvalErrorKind::TypeError { .. }), "{kind:?}");
    }
}

#[test]
fn host_function_call_via_registry() {
    let entries = eval_with("terminal = select(kitty, alacritty, foot)", |rt| {
        rt.register_function("select", |args| {
            args.first().cloned().ok_or_else(|| "select() needs at least one argument".to_string())
        });
    });
    assert_eq!(leaf(&entries, "terminal"), &Value::Str("kitty".into()));
}

#[test]
fn unknown_function_is_an_error() {
    let kind = eval_err("x = nonexistent(1, 2)");
    assert!(matches!(kind, scorium_lua::EvalErrorKind::UnknownFunction { .. }), "{kind:?}");
}

#[test]
fn script_block_cannot_touch_filesystem() {
    let kind = eval_err("script {\n    local f = io.open(\"/etc/passwd\")\n}");
    let scorium_lua::EvalErrorKind::ScriptError { message, .. } = kind else { panic!("expected ScriptError, got {kind:?}") };
    assert!(message.contains("io"), "expected the sandbox to reject `io`, got: {message}");
}

#[test]
fn script_block_cannot_spawn_processes() {
    let kind = eval_err("script {\n    os.execute(\"echo pwned\")\n}");
    let scorium_lua::EvalErrorKind::ScriptError { message, .. } = kind else { panic!("expected ScriptError, got {kind:?}") };
    assert!(message.contains("os"), "expected the sandbox to reject `os`, got: {message}");
}

#[test]
fn script_block_can_use_math_and_string() {
    // Should not error: math/string/table are the only stdlibs exposed.
    let _entries = eval("script {\n    local x = math.floor(3.7)\n    local s = string.upper(\"hi\")\n}");
}

#[test]
fn native_host_function_is_callable_from_script_blocks() {
    let calls = Rc::new(Cell::new(0));
    let observed = Rc::clone(&calls);
    let _entries = eval_with("script {\n    notify(41)\n}", |rt| {
        rt.register_function("notify", move |args| {
            assert_eq!(args, &[Value::Int(41)]);
            observed.set(observed.get() + 1);
            Ok(Value::Nil)
        });
    });
    assert_eq!(calls.get(), 1);
}

#[test]
fn lua_host_function_is_callable_from_expressions_and_scripts() {
    let mut runtime = Runtime::new().unwrap();
    let double = runtime.lua().create_function(|_, value: i64| Ok(value * 2)).unwrap();
    runtime.register_lua_function("double", double);
    let source = Source::new("<test>", "value = double(21)\nscript {\n    assert(double(3) == 6)\n}");
    let doc = scorium_core::parse(&source).unwrap();
    let out = runtime.evaluate(&doc, &source, Path::new(".")).unwrap();
    assert_eq!(leaf(&out.entries, "value"), &Value::Int(42));
}

#[test]
fn script_globals_and_library_mutations_do_not_leak_between_blocks() {
    let _entries = eval(
        "script {\n    leaked = 1\n    math.abs = nil\n}\nscript {\n    assert(leaked == nil)\n    assert(math.abs(-1) == 1)\n}",
    );
}

#[test]
fn script_cannot_reach_shared_vm_state_through_base_functions() {
    let _entries =
        eval("script {\n    assert(collectgarbage == nil)\n    assert(getmetatable == nil)\n    assert(load == nil)\n}");
}

#[test]
fn includes_merge_into_includer() {
    let dir = std::env::temp_dir().join(format!("scorium-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("theme.scor"), "@accent = teal\n").unwrap();
    let main_src = "include \"theme.scor\"\ncolor_name = $accent";
    let source = Source::new(dir.join("main.scor").display().to_string(), main_src);
    let doc = scorium_core::parse(&source).unwrap();
    let runtime = Runtime::new().unwrap();
    let out = runtime.evaluate(&doc, &source, &dir).unwrap_or_else(|e| panic!("{}", e.kind));
    assert_eq!(leaf(&out.entries, "color_name"), &Value::Str("teal".into()));
    assert!(out.entries.iter().any(|e| matches!(e, Entry::Include(_))));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn include_cycle_is_detected() {
    let dir = std::env::temp_dir().join(format!("scorium-test-cycle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.scor"), "include \"b.scor\"\n").unwrap();
    std::fs::write(dir.join("b.scor"), "include \"a.scor\"\n").unwrap();
    let source = Source::new(dir.join("a.scor").display().to_string(), std::fs::read_to_string(dir.join("a.scor")).unwrap());
    let doc = scorium_core::parse(&source).unwrap();
    let runtime = Runtime::new().unwrap();
    let err = runtime.evaluate(&doc, &source, &dir).expect_err("expected a cycle error");
    assert!(matches!(err.kind, scorium_lua::EvalErrorKind::IncludeCycle { .. }), "{:?}", err.kind);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn includes_can_be_disabled_by_host() {
    let source = Source::new("<test>", "include \"whatever.scor\"");
    let doc = scorium_core::parse(&source).unwrap();
    let runtime = Runtime::with_options(RuntimeOptions {
        include_policy: IncludePolicy { enabled: false, allow_parent_traversal: false },
        ..RuntimeOptions::default()
    })
    .unwrap();
    let err = runtime.evaluate(&doc, &source, Path::new(".")).expect_err("expected includes-disabled error");
    assert!(matches!(err.kind, scorium_lua::EvalErrorKind::IncludesDisabled { .. }));
}

#[test]
fn default_include_policy_rejects_absolute_paths() {
    let dir = std::env::temp_dir().join(format!("scorium-test-absolute-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("outside.scor");
    std::fs::write(&target, "value = exposed\n").unwrap();
    let src = format!("include {:?}", target.display().to_string());
    let source = Source::new("<test>", src);
    let doc = scorium_core::parse(&source).unwrap();
    let runtime = Runtime::new().unwrap();
    let err = runtime.evaluate(&doc, &source, &dir).expect_err("absolute include must be denied");
    assert!(matches!(err.kind, scorium_lua::EvalErrorKind::IncludePathDenied { .. }), "{:?}", err.kind);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn dots_inside_a_filename_are_not_parent_traversal() {
    let dir = std::env::temp_dir().join(format!("scorium-test-dot-name-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("theme..scor"), "value = allowed\n").unwrap();
    let source = Source::new("<test>", "include \"theme..scor\"");
    let doc = scorium_core::parse(&source).unwrap();
    let runtime = Runtime::new().unwrap();
    let out = runtime.evaluate(&doc, &source, &dir).unwrap();
    assert_eq!(leaf(&out.entries, "value"), &Value::Str("allowed".into()));
    std::fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn default_include_policy_rejects_symlink_escapes() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!("scorium-test-symlink-{}", std::process::id()));
    let base = root.join("base");
    let outside = root.join("outside.scor");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(&outside, "value = exposed\n").unwrap();
    symlink(&outside, base.join("linked.scor")).unwrap();
    let source = Source::new("<test>", "include \"linked.scor\"");
    let doc = scorium_core::parse(&source).unwrap();
    let runtime = Runtime::new().unwrap();
    let err = runtime.evaluate(&doc, &source, &base).expect_err("symlink escape must be denied");
    assert!(matches!(err.kind, scorium_lua::EvalErrorKind::IncludePathDenied { .. }), "{:?}", err.kind);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn loop_budget_prevents_runaway_while() {
    let runtime = Runtime::with_options(RuntimeOptions { max_loop_iterations: 100, ..RuntimeOptions::default() }).unwrap();
    let source = Source::new("<test>", "local n = 0\nwhile true do\n    n = n + 1\nend");
    let doc = scorium_core::parse(&source).unwrap();
    let err = runtime.evaluate(&doc, &source, Path::new(".")).expect_err("expected loop budget error");
    assert!(matches!(err.kind, scorium_lua::EvalErrorKind::LoopBudgetExceeded { .. }));
}

#[test]
fn call_depth_limit_prevents_recursive_function_stack_overflow() {
    let runtime = Runtime::with_options(RuntimeOptions { max_function_call_depth: 16, ..RuntimeOptions::default() }).unwrap();
    let source = Source::new("<test>", "fn recurse() {\n    return recurse()\n}\nvalue = recurse()");
    let doc = scorium_core::parse(&source).unwrap();
    let err = runtime.evaluate(&doc, &source, Path::new(".")).expect_err("expected recursive call limit");
    assert!(matches!(err.kind, scorium_lua::EvalErrorKind::CallDepthExceeded { .. }), "{:?}", err.kind);
}

#[test]
fn lua_memory_limit_prevents_unbounded_allocation() {
    let runtime =
        Runtime::with_options(RuntimeOptions { max_lua_memory_bytes: 512 * 1024, ..RuntimeOptions::default() }).unwrap();
    let source = Source::new("<test>", "script {\n    local huge = string.rep(\"x\", 2 * 1024 * 1024)\n}");
    let doc = scorium_core::parse(&source).unwrap();
    let err = runtime.evaluate(&doc, &source, Path::new(".")).expect_err("expected Lua allocation to be bounded");
    assert!(matches!(err.kind, scorium_lua::EvalErrorKind::ScriptError { .. }), "{:?}", err.kind);
}
