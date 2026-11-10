//! Error, exception and message builtins.
//!
//! This module owns the user-facing side of the message system: `Message` /
//! `MessageName`, the `On`/`Off` toggles, `Quiet`, `Assert` (with
//! `$AssertFunction`), `HoldForm`, and the `$MessageList` bookkeeping. The
//! underlying formatting/suppression state lives in [`crate::messages`].

use crate::ast::Expr;
use crate::env::Env;
use crate::eval::{apply_function, eval};
use crate::messages;
use crate::value::{BuiltinFn, EvalError, Format, Value};

/// Throw[val] / Throw[val, tag] — raise a value as a thrown exception
/// (caught by Catch). With a tag only a matching `Catch[..., tag]` handler
/// intercepts it.
pub fn builtin_throw(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => Err(EvalError::Thrown(Box::new(args[0].clone()))),
        2 => Err(EvalError::ThrownTagged {
            value: Box::new(args[0].clone()),
            tag: Box::new(args[1].clone()),
        }),
        _ => Err(EvalError::Error(
            "Throw requires 1 or 2 arguments".to_string(),
        )),
    }
}

/// Error["message"] — raise a general error.
pub fn builtin_error(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Error requires exactly 1 argument".to_string(),
        ));
    }
    let msg = match &args[0] {
        Value::Str(s) => s.clone(),
        other => format!("{}", other),
    };
    Err(EvalError::Error(msg))
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn truthy(v: &Value) -> bool {
    matches!(v, Value::Bool(true)) || matches!(v, Value::Symbol(s) if s == "True")
}

/// Extract the held expression from a `HoldAll` argument.
fn held_expr(v: &Value) -> Expr {
    match v {
        Value::Pattern(e) => e.clone(),
        other => crate::eval::table::value_to_expr(other),
    }
}

/// Unwrap a held argument into a plain (still unevaluated) value.
fn held_value(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => crate::pattern::unwrap_expr_to_value(e),
        other => other.clone(),
    }
}

/// Build an unevaluated `head[args...]` call from raw held arguments.
fn unevaluated(name: &str, args: &[Value]) -> Value {
    Value::Call {
        head: name.to_string(),
        args: args.iter().map(|a| Value::Pattern(held_expr(a))).collect(),
    }
}

/// Extract a symbol name from a (possibly held) symbol argument.
fn symbol_name(v: &Value) -> String {
    match v {
        Value::Symbol(s) => s.clone(),
        Value::Str(s) => s.clone(),
        Value::Builtin(name, _) => name.clone(),
        Value::Pattern(e) => symbol_name(&crate::pattern::unwrap_expr_to_value(e)),
        Value::Formatted { value, .. } => symbol_name(value),
        other => other.to_string(),
    }
}

/// A string value's contents, or the rendered form of anything else.
fn str_or_render(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        other => messages::render_arg(other),
    }
}

/// Build the full `Sym::tag` name from a `MessageName[sym, "tag"]` value, a
/// bare symbol, or a string. Bare strings are treated as full names.
fn message_tag(v: &Value) -> String {
    match v {
        Value::Call { head, args } if head == "MessageName" && args.len() == 2 => {
            messages::full_name(&symbol_name(&args[0]), &str_or_render(&args[1]))
        }
        Value::Str(s) => s.clone(),
        Value::Symbol(s) => s.clone(),
        Value::Pattern(e) => message_tag(&crate::pattern::unwrap_expr_to_value(e)),
        Value::Formatted { value, .. } => message_tag(value),
        other => other.to_string(),
    }
}

/// Keep the `$MessageList` symbol in sync with the thread's emitted messages.
fn sync_message_list(env: &Env) {
    let list: Vec<Value> = messages::message_list()
        .into_iter()
        .map(|full| {
            let (sym, name) = messages::split_tag(&full);
            Value::Call {
                head: "MessageName".to_string(),
                args: vec![Value::Symbol(sym), Value::Str(name)],
            }
        })
        .collect();
    env.set_propagate("$MessageList".to_string(), Value::List(list));
}

// ── Message / MessageName ─────────────────────────────────────────────────────

/// `Message[sym::tag, args...]` — emit a formatted message.
///
/// The first argument is held so a `MessageName[sym, "tag"]` (from the
/// `sym::tag` syntax) can be resolved. If the name was assigned a template
/// (`sym::tag = "..."`), that template is used; otherwise the registered
/// template is looked up. Remaining arguments fill the `` `1` ``…`` `n` ``
/// placeholders.
pub fn builtin_message(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Message requires at least 1 argument".to_string(),
        ));
    }
    let held_is_name = matches!(
        held_value(&args[0]),
        Value::Call { head, .. } if head == "MessageName"
    );
    let mut tag = message_tag(&args[0]);
    let mut template = messages::get_template(&tag);

    // A held `MessageName[...]` may have been given a template via assignment;
    // evaluate it to (a) resolve dynamic tags and (b) pick up that template.
    if held_is_name {
        match eval(&held_expr(&args[0]), env) {
            Ok(Value::Call { head, args: mn }) if head == "MessageName" && mn.len() == 2 => {
                tag = messages::full_name(&symbol_name(&mn[0]), &str_or_render(&mn[1]));
                if template.is_none() {
                    template = messages::get_template(&tag);
                }
            }
            Ok(Value::Str(t)) if template.is_none() => {
                template = Some(t);
            }
            _ => {}
        }
    }

    let fmt_args: Vec<String> = args[1..].iter().map(messages::render_arg).collect();
    messages::emit_with_template(&tag, template.as_deref(), &fmt_args);
    sync_message_list(env);
    Ok(Value::Null)
}

/// `MessageName[sym, "tag"]` — the symbolic message-name object.
pub fn builtin_message_name(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MessageName requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Call {
        head: "MessageName".to_string(),
        args: vec![
            Value::Symbol(symbol_name(&args[0])),
            Value::Str(symbol_name(&args[1])),
        ],
    })
}

// ── On / Off ──────────────────────────────────────────────────────────────────

/// `Off[spec...]` — silence the given messages.
pub fn builtin_off(args: &[Value]) -> Result<Value, EvalError> {
    for a in args {
        if let Some(spec) = messages::value_to_spec(a) {
            messages::set_off(spec);
        }
    }
    Ok(Value::Null)
}

/// `On[spec...]` — re-enable the given messages.
pub fn builtin_on(args: &[Value]) -> Result<Value, EvalError> {
    for a in args {
        if let Some(spec) = messages::value_to_spec(a) {
            messages::set_on(spec);
        }
    }
    Ok(Value::Null)
}

// ── Quiet ─────────────────────────────────────────────────────────────────────

/// Collect the message specs from a (held) `Quiet` second argument. `None`
/// means "everything".
fn quiet_specs(second: Option<&Value>) -> Option<Vec<String>> {
    let v = match second? {
        Value::Pattern(e) => crate::pattern::unwrap_expr_to_value(e),
        other => other.clone(),
    };
    match v {
        Value::Symbol(s) if s == "All" => None,
        Value::List(items) => {
            let specs: Vec<String> = items.iter().filter_map(messages::value_to_spec).collect();
            if specs.is_empty() { None } else { Some(specs) }
        }
        other => messages::value_to_spec(&other).map(|s| vec![s]),
    }
}

/// `Quiet[expr]` / `Quiet[expr, specs]` — evaluate `expr` with the given
/// messages suppressed.
pub fn builtin_quiet(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(unevaluated("Quiet", args));
    }
    let specs = quiet_specs(args.get(1));
    messages::push_quiet(specs);
    let result = eval(&held_expr(&args[0]), env);
    messages::pop_quiet();
    result
}

// ── Assert ─────────────────────────────────────────────────────────────────────

/// Resolve the message name used when an `Assert` fails. A bare tag `t` maps to
/// `t::asrt`; a full `Sym::tag` is used verbatim; the default is `Assert::asrt`.
fn assert_tag(second: Option<&Value>) -> String {
    match second {
        Some(v) => {
            let spec = message_tag(v);
            if spec.contains("::") {
                spec
            } else {
                messages::full_name(&spec, "asrt")
            }
        }
        None => "Assert::asrt".to_string(),
    }
}

/// `Assert[test]` / `Assert[test, tag]` — emit a message (or run
/// `$AssertFunction`) when `test` is False.
pub fn builtin_assert(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(unevaluated("Assert", args));
    }
    let condition = eval(&held_expr(&args[0]), env)?;
    if truthy(&condition) {
        return Ok(Value::Null);
    }
    if matches!(&condition, Value::Bool(false))
        || matches!(&condition, Value::Symbol(s) if s == "False")
    {
        // `$AssertFunction`, when set to something other than Identity, handles
        // the failure instead of the built-in message.
        if let Some(f) = env.get("$AssertFunction")
            && !matches!(&f, Value::Symbol(s) if s == "Identity")
        {
            let _ = apply_function(&f, &[Value::Pattern(held_expr(&args[0]))], env)?;
            return Ok(Value::Null);
        }
        let tag = assert_tag(args.get(1));
        messages::emit(&tag, &[condition.to_string()]);
        sync_message_list(env);
        return Ok(Value::Null);
    }
    Ok(unevaluated("Assert", args))
}

// ── HoldForm ──────────────────────────────────────────────────────────────────

/// `HoldForm[expr]` — display `expr` without evaluating it.
pub fn builtin_hold_form(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("HoldForm", args));
    }
    Ok(Value::Formatted {
        format: Format::InputForm,
        value: Box::new(held_value(&args[0])),
    })
}

// ── Registration ──────────────────────────────────────────────────────────────

/// Register the message/error builtins. Called from
/// [`crate::builtins::register_builtins`] after the core tables so the
/// message-aware versions win.
pub fn register(env: &Env) {
    for (name, func) in [
        (
            "Message",
            builtin_message as fn(&[Value], &Env) -> Result<Value, EvalError>,
        ),
        ("Assert", builtin_assert),
        ("Quiet", builtin_quiet),
    ] {
        env.set(
            name.to_string(),
            Value::Builtin(name.to_string(), BuiltinFn::Env(func)),
        );
    }
    for name in ["Assert", "Quiet"] {
        env.set_attributes(
            name,
            vec![
                "HoldAll".to_string(),
                "Locked".to_string(),
                "ReadProtected".to_string(),
            ],
        );
    }
    // `Message` holds only its first argument; the rest are evaluated.
    env.set_attributes(
        "Message",
        vec![
            "HoldFirst".to_string(),
            "Locked".to_string(),
            "ReadProtected".to_string(),
        ],
    );

    for (name, func) in [
        (
            "MessageName",
            builtin_message_name as fn(&[Value]) -> Result<Value, EvalError>,
        ),
        ("Off", builtin_off),
        ("On", builtin_on),
        ("HoldForm", builtin_hold_form),
    ] {
        env.set(
            name.to_string(),
            Value::Builtin(name.to_string(), BuiltinFn::Pure(func)),
        );
    }
    env.set_attributes(
        "MessageName",
        vec![
            "HoldFirst".to_string(),
            "Locked".to_string(),
            "ReadProtected".to_string(),
        ],
    );
    env.set_attributes(
        "HoldForm",
        vec![
            "HoldAll".to_string(),
            "Locked".to_string(),
            "ReadProtected".to_string(),
        ],
    );

    env.set("$MessageList".to_string(), Value::List(vec![]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Env;
    use crate::messages as msg;
    use rug::Integer;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn test_env() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        msg::reset_state();
        env
    }

    fn eval_str(src: &str, env: &Env) -> Result<Value, EvalError> {
        let tokens = crate::lexer::tokenize(src).unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        let mut last = Value::Null;
        for expr in ast {
            last = eval(&expr, env)?;
        }
        Ok(last)
    }

    fn message_name(sym: &str, tag: &str) -> Value {
        Value::Call {
            head: "MessageName".to_string(),
            args: vec![Value::Symbol(sym.to_string()), Value::Str(tag.to_string())],
        }
    }

    // ── Throw / Error (existing behavior) ───────────────────────────────

    #[test]
    fn test_throw() {
        let result = builtin_throw(&[int(99)]);
        assert!(
            matches!(result, Err(EvalError::Thrown(ref v)) if matches!(v.as_ref(), Value::Integer(_)))
        );
    }

    #[test]
    fn test_throw_tagged() {
        let result = builtin_throw(&[int(1), Value::Str("tag".to_string())]);
        assert!(matches!(
            result,
            Err(EvalError::ThrownTagged { ref tag, .. })
                if matches!(tag.as_ref(), Value::Str(s) if s == "tag")
        ));
    }

    #[test]
    fn test_throw_wrong_arity() {
        assert!(builtin_throw(&[]).is_err());
        assert!(builtin_throw(&[int(1), int(2), int(3)]).is_err());
    }

    #[test]
    fn test_error_from_string() {
        let result = builtin_error(&[Value::Str("boom".to_string())]);
        assert!(matches!(result, Err(EvalError::Error(ref m)) if m == "boom"));
    }

    #[test]
    fn test_error_formats_non_string() {
        let result = builtin_error(&[int(7)]);
        assert!(matches!(result, Err(EvalError::Error(ref m)) if m == "7"));
    }

    // ── MessageName ─────────────────────────────────────────────────────

    #[test]
    fn test_message_name_builds_call() {
        let v = builtin_message_name(&[
            Value::Symbol("Foo".to_string()),
            Value::Str("bar".to_string()),
        ])
        .unwrap();
        assert!(matches!(
            v,
            Value::Call { ref head, ref args }
                if head == "MessageName"
                    && args.len() == 2
                    && matches!(&args[0], Value::Symbol(s) if s == "Foo")
                    && matches!(&args[1], Value::Str(s) if s == "bar")
        ));
    }

    #[test]
    fn test_message_name_arity() {
        assert!(builtin_message_name(&[Value::Symbol("Foo".to_string())]).is_err());
    }

    // ── Message formatting ──────────────────────────────────────────────

    #[test]
    fn test_message_uses_template_and_records_list() {
        msg::reset_state();
        let env = test_env();
        msg::set_template("Foo::bar", "value `1` is bad");
        msg::clear_message_list();
        let (result, msgs) = msg::with_buffer(|| {
            builtin_message(&[message_name("Foo", "bar"), int(3)], &env).unwrap()
        });
        assert!(matches!(result, Value::Null));
        assert_eq!(msgs, vec!["Foo::bar: value 3 is bad".to_string()]);
        // $MessageList is updated.
        match env.get("$MessageList") {
            Some(Value::List(items)) => assert_eq!(items.len(), 1),
            other => panic!("expected $MessageList list, got {other:?}"),
        }
    }

    #[test]
    fn test_message_unknown_tag_joins_args() {
        msg::reset_state();
        let env = test_env();
        let (_, msgs) = msg::with_buffer(|| {
            builtin_message(&[message_name("Nope", "tag"), int(1), int(2)], &env).unwrap()
        });
        assert_eq!(msgs, vec!["Nope::tag: 1, 2".to_string()]);
    }

    #[test]
    fn test_message_requires_argument() {
        let env = test_env();
        assert!(builtin_message(&[], &env).is_err());
    }

    // ── Off / On ────────────────────────────────────────────────────────

    #[test]
    fn test_off_silences_and_on_restores() {
        msg::reset_state();
        let env = test_env();
        let name = message_name("Tog", "m");
        let (_, msgs) = msg::with_buffer(|| {
            builtin_off(&[name.clone()]).unwrap();
            builtin_message(&[name.clone()], &env).unwrap();
            builtin_on(&[name.clone()]).unwrap();
            builtin_message(&[name], &env).unwrap();
        });
        assert_eq!(msgs.len(), 1, "only the re-enabled message should print");
        assert_eq!(msgs[0], "Tog::m: ");
    }

    #[test]
    fn test_off_by_symbol_group() {
        msg::reset_state();
        let env = test_env();
        let (_, msgs) = msg::with_buffer(|| {
            builtin_off(&[Value::Symbol("Grp".to_string())]).unwrap();
            builtin_message(&[message_name("Grp", "a")], &env).unwrap();
            builtin_message(&[message_name("Grp", "b")], &env).unwrap();
        });
        assert!(msgs.is_empty());
    }

    // ── Quiet ───────────────────────────────────────────────────────────

    #[test]
    fn test_quiet_suppresses_messages_in_expr() {
        msg::reset_state();
        let env = test_env();
        msg::set_template("Q::a", "a!");
        let expr = Expr::Call {
            head: Box::new(Expr::Symbol("Message".to_string())),
            args: vec![Expr::Call {
                head: Box::new(Expr::Symbol("MessageName".to_string())),
                args: vec![Expr::Symbol("Q".to_string()), Expr::Str("a".to_string())],
            }],
        };
        let held = Value::Pattern(expr);
        let (_, msgs) = msg::with_buffer(|| {
            builtin_quiet(&[held], &env).unwrap();
        });
        assert!(msgs.is_empty(), "Quiet should suppress");
    }

    #[test]
    fn test_quiet_named_only_listed() {
        msg::reset_state();
        assert!(msg::is_message_on("QQ::a"));
        msg::push_quiet(Some(vec!["QQ::a".to_string()]));
        assert!(!msg::is_message_on("QQ::a"));
        assert!(msg::is_message_on("QQ::b"));
        msg::pop_quiet();
    }

    // ── Assert / $AssertFunction ────────────────────────────────────────

    #[test]
    fn test_assert_true_is_silent() {
        msg::reset_state();
        let env = test_env();
        let (result, msgs) =
            msg::with_buffer(|| builtin_assert(&[Value::Pattern(Expr::Bool(true))], &env).unwrap());
        assert!(matches!(result, Value::Null));
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_assert_false_emits_message() {
        msg::reset_state();
        let env = test_env();
        let (_, msgs) = msg::with_buffer(|| {
            builtin_assert(&[Value::Pattern(Expr::Bool(false))], &env).unwrap()
        });
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].starts_with("Assert::asrt: "), "got: {}", msgs[0]);
        assert!(msgs[0].contains("False"));
    }

    #[test]
    fn test_assert_custom_tag() {
        msg::reset_state();
        let env = test_env();
        let (_, msgs) = msg::with_buffer(|| {
            builtin_assert(
                &[
                    Value::Pattern(Expr::Bool(false)),
                    Value::Str("My".to_string()),
                ],
                &env,
            )
            .unwrap()
        });
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].starts_with("My::asrt: "), "got: {}", msgs[0]);
    }

    #[test]
    fn test_assert_function_identity_emits() {
        msg::reset_state();
        let env = test_env();
        // The default `$AssertFunction` is `Identity`, which still emits.
        env.set(
            "$AssertFunction".to_string(),
            Value::Symbol("Identity".to_string()),
        );
        let (_, msgs) = msg::with_buffer(|| {
            builtin_assert(&[Value::Pattern(Expr::Bool(false))], &env).unwrap()
        });
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn test_assert_function_applied_on_failure() {
        msg::reset_state();
        let env = test_env();
        // Track invocation via a side-effecting definition.
        env.set(
            "$AssertFunction".to_string(),
            Value::Builtin("$af".to_string(), BuiltinFn::Pure(|_| Ok(Value::Null))),
        );
        let (result, msgs) = msg::with_buffer(|| {
            builtin_assert(&[Value::Pattern(Expr::Bool(false))], &env).unwrap()
        });
        assert!(matches!(result, Value::Null));
        assert!(msgs.is_empty());
    }

    // ── HoldForm ────────────────────────────────────────────────────────

    #[test]
    fn test_hold_form_defers_display() {
        let v = builtin_hold_form(&[Value::Pattern(Expr::Call {
            head: Box::new(Expr::Symbol("Plus".to_string())),
            args: vec![
                Expr::Integer(Integer::from(1)),
                Expr::Integer(Integer::from(1)),
            ],
        })])
        .unwrap();
        assert_eq!(v.to_string(), "1 + 1");
    }

    // ── End-to-end parsing: `Sym::tag` ──────────────────────────────────

    #[test]
    fn test_end_to_end_message_name_parsing() {
        msg::reset_state();
        let env = test_env();
        msg::set_template("Tag::msg", "tag message `1`");
        msg::clear_message_list();
        let (_, msgs) = msg::with_buffer(|| {
            eval_str("Message[Tag::msg, 7]", &env).unwrap();
        });
        assert_eq!(msgs, vec!["Tag::msg: tag message 7".to_string()]);
        // $MessageList records the full name.
        match env.get("$MessageList") {
            Some(Value::List(items)) => {
                assert_eq!(items.len(), 1);
                match &items[0] {
                    Value::Call { head, args } if head == "MessageName" => {
                        assert!(matches!(&args[0], Value::Symbol(s) if s == "Tag"));
                        assert!(matches!(&args[1], Value::Str(s) if s == "msg"));
                    }
                    other => panic!("expected MessageName element, got {other:?}"),
                }
            }
            other => panic!("expected $MessageList, got {other:?}"),
        }
    }

    #[test]
    fn test_end_to_end_off_then_message() {
        msg::reset_state();
        let env = test_env();
        let (_, msgs) = msg::with_buffer(|| {
            eval_str("Off[Gen::x]; Message[Gen::x]", &env).unwrap();
        });
        assert!(msgs.is_empty(), "off message must be silent");
    }

    #[test]
    fn test_end_to_end_quiet_named_specs() {
        msg::reset_state();
        let env = test_env();
        let (_, msgs) = msg::with_buffer(|| {
            eval_str("Quiet[{Message[QA::a], Message[QB::b]}, {QA::a}]", &env).unwrap();
        });
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].starts_with("QB::b: "), "got: {}", msgs[0]);
    }

    #[test]
    fn test_message_template_with_stringform_arg() {
        msg::reset_state();
        let env = test_env();
        msg::set_template("SF::m", "outer(`1`)");
        let (_, msgs) = msg::with_buffer(|| {
            eval_str(r#"Message[SF::m, StringForm["x=`1`", 9]]"#, &env).unwrap();
        });
        assert_eq!(msgs, vec!["SF::m: outer(x=9)".to_string()]);
    }

    // ── Check (message -> caught) ───────────────────────────────────────

    #[test]
    fn test_check_catches_generated_message() {
        msg::reset_state();
        let env = test_env();
        let (result, _) =
            msg::with_buffer(|| eval_str("Check[Message[Power::infy], caught]", &env).unwrap());
        assert!(matches!(result, Value::Symbol(s) if s == "caught"));
    }

    #[test]
    fn test_check_passes_through_without_message() {
        msg::reset_state();
        let env = test_env();
        let (result, msgs) = msg::with_buffer(|| eval_str("Check[1 + 1, caught]", &env).unwrap());
        assert!(matches!(result, Value::Integer(n) if n == 2));
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_check_tag_filters_messages() {
        msg::reset_state();
        let env = test_env();
        // Only `Power::infy` should trigger the failure branch.
        let (result, _) = msg::with_buffer(|| {
            eval_str("Check[Message[Other::x], caught, Other::x]", &env).unwrap()
        });
        assert!(matches!(result, Value::Symbol(s) if s == "caught"));
        let (result2, _) = msg::with_buffer(|| {
            eval_str("Check[Message[Other::x], caught, Nope::y]", &env).unwrap()
        });
        assert!(matches!(result2, Value::Null));
    }
}
