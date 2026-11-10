//! symbol and option introspection/scoping: DownValues, Context, Begin/End, SetOptions/OptionValue, HoldPattern, Inactive.
//!
//! Gap-fill module; see the individual builtin docs for semantics.
//!
//! All functions return an unevaluated `Value::Call` when their input is
//! inapplicable (e.g. a non-symbol argument), matching Wolfram Language
//! symbolic behaviour.

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, FunctionDefinition, Value};

/// Build an unevaluated call, unwrapping any held `Pattern` args so the result
/// looks like normal symbolic output.
fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args
            .iter()
            .map(|a| match a {
                Value::Pattern(e) => crate::pattern::unwrap_expr_to_value(e),
                other => other.clone(),
            })
            .collect(),
    }
}

/// Extract a symbol name from an argument that may arrive as a bare symbol
/// (`HoldAll`), a held `Value::Pattern`, or a string.
fn symbol_arg(arg: &Value) -> Option<String> {
    match arg {
        Value::Symbol(s) | Value::Str(s) => Some(s.clone()),
        Value::Pattern(Expr::Symbol(s)) => Some(s.clone()),
        Value::Pattern(Expr::Call { head, .. }) => match head.as_ref() {
            Expr::Symbol(s) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Build a `HoldPattern[lhs] :> rhs` rule value.
fn held_rule(lhs: Expr, rhs: Value, delayed: bool) -> Value {
    let lhs_wrapped = Expr::Call {
        head: Box::new(Expr::Symbol("HoldPattern".to_string())),
        args: vec![lhs],
    };
    Value::Rule {
        lhs: Box::new(Value::Pattern(lhs_wrapped)),
        rhs: Box::new(rhs),
        delayed,
    }
}

/// Rule with the rhs held as a pattern (for definition bodies).
fn held_rule_expr(lhs: Expr, rhs: Expr, delayed: bool) -> Value {
    held_rule(lhs, Value::Pattern(rhs), delayed)
}

/// Reconstruct the `lhs` expression of a definition (with an optional guard).
fn definition_lhs(name: &str, def: &FunctionDefinition) -> Expr {
    let call = Expr::Call {
        head: Box::new(Expr::Symbol(name.to_string())),
        args: def.params.clone(),
    };
    if let Some(guard) = &def.guard {
        Expr::PatternGuard {
            pattern: Box::new(call),
            condition: Box::new(guard.clone()),
        }
    } else {
        call
    }
}

/// `DownValues[f]` — definitions of the form `f[...] := ...`.
pub fn builtin_down_values(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(name) = args.first().and_then(symbol_arg) else {
        return Ok(unevaluated("DownValues", args));
    };
    let rules: Vec<Value> = match env.get(&name) {
        Some(Value::Function(fd)) => fd
            .definitions
            .iter()
            .map(|def| held_rule_expr(definition_lhs(&fd.name, def), def.body.clone(), true))
            .collect(),
        _ => Vec::new(),
    };
    Ok(Value::List(rules))
}

/// `OwnValues[s]` — value definitions of the form `s = value`.
pub fn builtin_own_values(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(name) = args.first().and_then(symbol_arg) else {
        return Ok(unevaluated("OwnValues", args));
    };
    match env.get(&name) {
        Some(
            Value::Function(_)
            | Value::Builtin(_, _)
            | Value::BytecodeFunction(_)
            | Value::PureFunction { .. },
        ) => Ok(Value::List(Vec::new())),
        Some(v) => Ok(Value::List(vec![held_rule(Expr::Symbol(name), v, true)])),
        None => Ok(Value::List(Vec::new())),
    }
}

/// `UpValues[f]` — upvalue rules registered with `f /: ...`.
pub fn builtin_up_values(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(name) = args.first().and_then(symbol_arg) else {
        return Ok(unevaluated("UpValues", args));
    };
    Ok(Value::List(env.get_upvalues(&name)))
}

/// `SubValues[f]` — curried definitions of the form `f[a][b] := ...`.
pub fn builtin_sub_values(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(name) = args.first().and_then(symbol_arg) else {
        return Ok(unevaluated("SubValues", args));
    };
    let rules: Vec<Value> = env
        .get_subvalues(&name)
        .into_iter()
        .map(|sv| {
            let lhs = Expr::Call {
                head: Box::new(Expr::Call {
                    head: Box::new(Expr::Symbol(name.clone())),
                    args: sv.outer,
                }),
                args: sv.inner,
            };
            let lhs = if let Some(guard) = sv.guard {
                Expr::PatternGuard {
                    pattern: Box::new(lhs),
                    condition: Box::new(guard),
                }
            } else {
                lhs
            };
            held_rule_expr(lhs, sv.rhs, sv.delayed)
        })
        .collect();
    Ok(Value::List(rules))
}

/// Render a rule value as `lhs :> rhs` / `lhs -> rhs`.
fn rule_to_string(rule: &Value) -> Option<String> {
    if let Value::Rule { lhs, rhs, delayed } = rule {
        let op = if *delayed { ":> " } else { "-> " };
        Some(format!("{} {}{}", lhs, op, rhs))
    } else {
        None
    }
}

/// `Definition[sym]` — a readable text rendering of all definitions of `sym`.
pub fn builtin_definition(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(name) = args.first().and_then(symbol_arg) else {
        return Ok(unevaluated("Definition", args));
    };

    let mut lines: Vec<String> = Vec::new();
    if let Ok(Value::List(rules)) = builtin_own_values(args, env) {
        for r in &rules {
            if let Some(s) = rule_to_string(r) {
                lines.push(s);
            }
        }
    }
    if let Ok(Value::List(rules)) = builtin_down_values(args, env) {
        for r in &rules {
            if let Some(s) = rule_to_string(r) {
                lines.push(s);
            }
        }
    }
    if let Ok(Value::List(rules)) = builtin_sub_values(args, env) {
        for r in &rules {
            if let Some(s) = rule_to_string(r) {
                lines.push(s);
            }
        }
    }
    for r in env.get_upvalues(&name) {
        if let Some(s) = rule_to_string(&r) {
            lines.push(format!("{} /: {}", name, s));
        }
    }

    if lines.is_empty() {
        return Ok(Value::Str(format!("{} is undefined.", name)));
    }
    Ok(Value::Str(format!("{}\n", lines.join("\n"))))
}

// ── Context / scoping ────────────────────────────────────────────────────────

/// `Context[sym]` — the context of a symbol (or the current context).
pub fn builtin_context(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(name) = args.first().and_then(symbol_arg) else {
        return Ok(unevaluated("Context", args));
    };
    let current = env.current_context();
    let ctx = match name.rfind('`') {
        Some(idx) => name[..=idx].to_string(),
        None => current,
    };
    Ok(Value::Str(ctx))
}

/// Coerce an evaluated argument to a context string.
fn context_string(arg: &Value) -> Option<String> {
    match arg {
        Value::Str(s) | Value::Symbol(s) => Some(s.clone()),
        Value::Pattern(Expr::Symbol(s)) | Value::Pattern(Expr::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// `Begin["ctx`"]` — push a context, returning it.
pub fn builtin_begin(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Begin requires exactly 1 argument".to_string(),
        ));
    }
    let ctx = context_string(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "String".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    env.push_context(ctx.clone());
    env.set_propagate("$Context".to_string(), Value::Str(ctx.clone()));
    Ok(Value::Str(ctx))
}

/// `End[]` — pop the innermost context, returning it.
pub fn builtin_end(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return Err(EvalError::Error("End takes no arguments".to_string()));
    }
    let ctx = env.pop_context().unwrap_or_else(|| "Global`".to_string());
    // `$Context` should reflect the context that is now current, not the one
    // just popped.
    env.set_propagate("$Context".to_string(), Value::Str(env.current_context()));
    Ok(Value::Str(ctx))
}

/// `BeginPackage["ctx`"]` — push a context (package scope), returning it.
pub fn builtin_begin_package(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "BeginPackage requires exactly 1 argument".to_string(),
        ));
    }
    let ctx = context_string(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "String".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    env.push_context(ctx.clone());
    env.set_propagate("$Context".to_string(), Value::Str(ctx.clone()));
    Ok(Value::Str(ctx))
}

/// `EndPackage[]` — pop the innermost (package) context, returning it.
pub fn builtin_end_package(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return Err(EvalError::Error(
            "EndPackage takes no arguments".to_string(),
        ));
    }
    let ctx = env.pop_context().unwrap_or_else(|| "Global`".to_string());
    env.set_propagate("$Context".to_string(), Value::Str(env.current_context()));
    Ok(Value::Str(ctx))
}

// ── Options ──────────────────────────────────────────────────────────────────

/// Extract `(key, value)` from a rule, evaluating a held rhs.
fn option_key_value(v: &Value, env: &Env) -> Result<Option<(String, Value)>, EvalError> {
    if let Value::Rule { lhs, rhs, .. } = v {
        let key = match lhs.as_ref() {
            Value::Symbol(s) | Value::Str(s) => s.clone(),
            Value::Pattern(Expr::Symbol(s)) | Value::Pattern(Expr::Str(s)) => s.clone(),
            _ => return Ok(None),
        };
        let value = match rhs.as_ref() {
            Value::Pattern(e) => crate::eval::eval(e, env)?,
            other => other.clone(),
        };
        return Ok(Some((key, value)));
    }
    Ok(None)
}

/// Convert a value into a list of `(name, value)` option pairs.
pub(crate) fn options_from_value(v: &Value, env: &Env) -> Result<Vec<(String, Value)>, EvalError> {
    let mut out = Vec::new();
    let items: Vec<&Value> = match v {
        Value::List(items) => items.iter().collect(),
        Value::Rule { .. } => vec![v],
        Value::Assoc(map) => {
            for (k, val) in map {
                out.push((k.clone(), val.clone()));
            }
            return Ok(out);
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "List of rules".to_string(),
                got: v.type_name().to_string(),
            });
        }
    };
    for item in items {
        if let Some((k, val)) = option_key_value(item, env)? {
            out.push((k, val));
        }
    }
    Ok(out)
}

fn options_list(opts: &[(String, Value)]) -> Value {
    Value::List(
        opts.iter()
            .map(|(k, v)| Value::Rule {
                lhs: Box::new(Value::Symbol(k.clone())),
                rhs: Box::new(v.clone()),
                delayed: false,
            })
            .collect(),
    )
}

/// `Options[f]` — the default options registered for `f`.
pub fn builtin_options(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(name) = args.first().and_then(symbol_arg) else {
        return Ok(unevaluated("Options", args));
    };
    Ok(options_list(&env.get_options(&name)))
}

/// `SetOptions[f, rules...]` — register default options for `f`.
pub fn builtin_set_options(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "SetOptions requires at least 2 arguments".to_string(),
        ));
    }
    let name = symbol_arg(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Symbol".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    let mut opts = env.get_options(&name);
    for arg in &args[1..] {
        for (k, v) in options_from_value(arg, env)? {
            if let Some(slot) = opts.iter_mut().find(|(ek, _)| *ek == k) {
                slot.1 = v;
            } else {
                opts.push((k, v));
            }
        }
    }
    env.replace_options(&name, opts.clone());
    Ok(options_list(&opts))
}

/// `OptionValue[name]` / `OptionValue[f, name]` — look up a collected option.
pub fn builtin_option_value(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = match args.first() {
        Some(a) => option_name(a),
        None => None,
    };
    let Some(name) = name else {
        return Ok(unevaluated("OptionValue", args));
    };

    if args.len() >= 2 {
        // OptionValue[f, name] — use the function's defaults.
        if let Some(f) = symbol_arg(&args[0])
            && let Some((_, v)) = env.get_options(&f).into_iter().find(|(k, _)| *k == name)
        {
            return Ok(v);
        }
    } else if let Some(Value::Assoc(map)) = env.get("$OptionsValues")
        && let Some(v) = map.get(&name)
    {
        return Ok(v.clone());
    }

    Ok(unevaluated("OptionValue", args))
}

fn option_name(a: &Value) -> Option<String> {
    match a {
        Value::Symbol(s) | Value::Str(s) => Some(s.clone()),
        Value::Pattern(Expr::Symbol(s)) | Value::Pattern(Expr::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// `OptionsPattern[]` used as a value (normally handled inside parameters).
pub fn builtin_options_pattern(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Pattern(Expr::Call {
        head: Box::new(Expr::Symbol("OptionsPattern".to_string())),
        args: args.iter().map(crate::eval::table::value_to_expr).collect(),
    }))
}

// ── Inactive / Activate ──────────────────────────────────────────────────────

/// `Inactive[head]` value (the call form is desugared in the evaluator).
pub fn builtin_inactive(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Inactive requires exactly 1 argument".to_string(),
        ));
    }
    let inner = match &args[0] {
        Value::Symbol(s) => Value::Symbol(s.clone()),
        other => other.clone(),
    };
    Ok(Value::Call {
        head: "Inactive".to_string(),
        args: vec![inner],
    })
}

/// `Activate[expr]` — remove all `Inactive` wrappers and evaluate.
pub fn builtin_activate(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Activate requires exactly 1 argument".to_string(),
        ));
    }
    activate_value(args[0].clone(), env)
}

fn activate_value(v: Value, env: &Env) -> Result<Value, EvalError> {
    match v {
        Value::Call { head, args } => {
            let new_args: Result<Vec<Value>, _> =
                args.into_iter().map(|a| activate_value(a, env)).collect();
            let new_args = new_args?;
            if head == "Inactive" && new_args.len() == 1 {
                return Ok(new_args.into_iter().next().unwrap());
            }
            if let Some(inner) = head
                .strip_prefix("Inactive[")
                .and_then(|s| s.strip_suffix(']'))
            {
                return crate::eval::apply_function(
                    &Value::Symbol(inner.to_string()),
                    &new_args,
                    env,
                );
            }
            Ok(Value::Call {
                head,
                args: new_args,
            })
        }
        Value::List(items) => {
            let mapped: Result<Vec<Value>, _> =
                items.into_iter().map(|a| activate_value(a, env)).collect();
            Ok(Value::List(mapped?))
        }
        other => Ok(other),
    }
}

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin_env(env, "DownValues", builtin_down_values);
    super::register_builtin_env(env, "OwnValues", builtin_own_values);
    super::register_builtin_env(env, "UpValues", builtin_up_values);
    super::register_builtin_env(env, "SubValues", builtin_sub_values);
    super::register_builtin_env(env, "Definition", builtin_definition);
    super::register_builtin_env(env, "Context", builtin_context);
    super::register_builtin_env(env, "Begin", builtin_begin);
    super::register_builtin_env(env, "End", builtin_end);
    super::register_builtin_env(env, "BeginPackage", builtin_begin_package);
    super::register_builtin_env(env, "EndPackage", builtin_end_package);
    super::register_builtin_env(env, "Options", builtin_options);
    super::register_builtin_env(env, "SetOptions", builtin_set_options);
    super::register_builtin_env(env, "OptionValue", builtin_option_value);
    super::register_builtin(env, "OptionsPattern", builtin_options_pattern);
    super::register_builtin(env, "Inactive", builtin_inactive);
    super::register_builtin_env(env, "Activate", builtin_activate);

    // Hold* attributes so arguments are not prematurely evaluated.
    for name in [
        "DownValues",
        "OwnValues",
        "UpValues",
        "SubValues",
        "Definition",
        "Options",
        "Context",
        "OptionValue",
        "HoldPattern",
        "Verbatim",
        "Inactive",
    ] {
        env.set_attributes(name, vec!["HoldAll".to_string(), "Protected".to_string()]);
    }
    env.set_attributes(
        "SetOptions",
        vec!["HoldFirst".to_string(), "Protected".to_string()],
    );
    env.set_attributes("Begin", vec!["Protected".to_string()]);
    env.set_attributes("End", vec!["Protected".to_string()]);
    env.set_attributes("BeginPackage", vec!["Protected".to_string()]);
    env.set_attributes("EndPackage", vec!["Protected".to_string()]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use crate::lexer;
    use crate::parser;

    fn eval_str(input: &str) -> Value {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn({
                let input = input.to_string();
                move || {
                    let env = Env::new();
                    builtins::register_builtins(&env);
                    let tokens = lexer::tokenize(&input).unwrap();
                    let ast = parser::parse(tokens).unwrap();
                    crate::eval::eval_program(&ast, &env).unwrap()
                }
            })
            .unwrap()
            .join()
            .unwrap()
    }

    fn contains(v: &Value, needle: &str) -> bool {
        format!("{}", v).contains(needle)
    }

    #[test]
    fn test_own_values() {
        let v = eval_str("x = 5; OwnValues[x]");
        assert!(contains(&v, "HoldPattern"), "got {}", v);
        assert!(contains(&v, "5"), "got {}", v);
    }

    #[test]
    fn test_down_values() {
        let v = eval_str("f[a_] := a^2; DownValues[f]");
        assert!(contains(&v, "HoldPattern"), "got {}", v);
        assert!(contains(&v, "f"), "got {}", v);
    }

    #[test]
    fn test_down_values_no_defs() {
        let v = eval_str("DownValues[f]");
        assert_eq!(v, Value::List(vec![]));
    }

    #[test]
    fn test_up_values() {
        let v = eval_str("f /: g[f[x_]] := x; UpValues[f]");
        assert!(contains(&v, "HoldPattern"), "got {}", v);
        // The upvalue fires for a plain call.
        assert_eq!(
            eval_str("f /: g[f[x_]] := x; g[f[42]]"),
            Value::Integer(rug::Integer::from(42))
        );
    }

    #[test]
    fn test_up_value_guard() {
        // A passing guard fires the upvalue.
        assert_eq!(
            eval_str("f /: Plus[f[x_], y_] := x + y /; x > 0; f[5] + 1"),
            Value::Integer(rug::Integer::from(6))
        );
        // A failing guard leaves the call intact.
        assert!(matches!(
            eval_str("f /: Plus[f[x_], y_] := x + y /; x > 0; f[-1] + 1"),
            Value::Call { .. }
        ));
    }

    #[test]
    fn test_up_values_on_builtin_head() {
        // Upvalue on Plus must not shadow the builtin.
        assert_eq!(
            eval_str("f /: Plus[f[x_], y_] := h[x, y]; f[3] + 1"),
            Value::Call {
                head: "h".to_string(),
                args: vec![
                    Value::Integer(rug::Integer::from(3)),
                    Value::Integer(rug::Integer::from(1))
                ]
            }
        );
        assert_eq!(eval_str("1 + 2"), Value::Integer(rug::Integer::from(3)));
    }

    #[test]
    fn test_sub_values() {
        let v = eval_str("f[a_][b_] := a + b; f[3][4]");
        assert_eq!(v, Value::Integer(rug::Integer::from(7)));
        let sv = eval_str("f[a_][b_] := a + b; SubValues[f]");
        assert!(contains(&sv, "HoldPattern"), "got {}", sv);
    }

    #[test]
    fn test_context() {
        assert_eq!(eval_str("Context[x]"), Value::Str("Global`".to_string()));
        // Begin/End push and pop.
        assert_eq!(
            eval_str("Begin[\"Foo`\"]; Context[x]"),
            Value::Str("Foo`".to_string())
        );
        assert_eq!(
            eval_str("Begin[\"Foo`\"]; End[]"),
            Value::Str("Foo`".to_string())
        );
        // Nested contexts restore correctly.
        assert_eq!(
            eval_str("Begin[\"A`\"]; Begin[\"B`\"]; End[]; Context[x]"),
            Value::Str("A`".to_string())
        );
    }

    #[test]
    fn test_begin_package() {
        assert_eq!(
            eval_str("BeginPackage[\"Pkg`\"]; Context[y]"),
            Value::Str("Pkg`".to_string())
        );
        assert_eq!(
            eval_str("BeginPackage[\"Pkg`\"]; EndPackage[]; Context[y]"),
            Value::Str("Global`".to_string())
        );
    }

    #[test]
    fn test_options() {
        assert_eq!(
            eval_str("SetOptions[f, a -> 1]; Options[f]"),
            Value::List(vec![Value::Rule {
                lhs: Box::new(Value::Symbol("a".to_string())),
                rhs: Box::new(Value::Integer(rug::Integer::from(1))),
                delayed: false,
            }])
        );
    }

    #[test]
    fn test_options_assignment() {
        let v = eval_str("Options[g] = {b -> 2}; b /. Options[g]");
        assert_eq!(v, Value::Integer(rug::Integer::from(2)));
    }

    #[test]
    fn test_options_pattern_and_option_value() {
        eval_str("");
        assert_eq!(
            eval_str("Options[f] = {a -> 1}; f[OptionsPattern[]] := OptionValue[a]; f[]"),
            Value::Integer(rug::Integer::from(1))
        );
        assert_eq!(
            eval_str("f[OptionsPattern[]] := OptionValue[a]; f[a -> 9]"),
            Value::Integer(rug::Integer::from(9))
        );
        // Call-time option overrides default.
        assert_eq!(
            eval_str("Options[f] = {a -> 1}; f[OptionsPattern[]] := OptionValue[a]; f[a -> 5]"),
            Value::Integer(rug::Integer::from(5))
        );
    }

    #[test]
    fn test_options_pattern_with_positional() {
        assert_eq!(
            eval_str("f[x_, OptionsPattern[]] := x + OptionValue[b]; f[2, b -> 3]"),
            Value::Integer(rug::Integer::from(5))
        );
    }

    #[test]
    fn test_hold_pattern() {
        // HoldPattern matches its content without letting it evaluate.
        assert_eq!(eval_str("MatchQ[3, HoldPattern[3]]"), Value::Bool(true));
        assert_eq!(eval_str("MatchQ[3, HoldPattern[4]]"), Value::Bool(false));
        assert_eq!(
            eval_str("MatchQ[a + b, HoldPattern[a + b]]"),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_verbatim() {
        // Verbatim matches its content literally.
        assert_eq!(eval_str("MatchQ[0, Verbatim[0]]"), Value::Bool(true));
        assert_eq!(eval_str("MatchQ[1, Verbatim[0]]"), Value::Bool(false));
        assert_eq!(
            eval_str("MatchQ[a + b, Verbatim[a + b]]"),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_inactive_activate() {
        assert_eq!(
            eval_str("Inactive[Plus][1, 2]"),
            Value::Call {
                head: "Inactive[Plus]".to_string(),
                args: vec![
                    Value::Integer(rug::Integer::from(1)),
                    Value::Integer(rug::Integer::from(2))
                ]
            }
        );
        assert_eq!(
            eval_str("Activate[Inactive[Plus][1, 2]]"),
            Value::Integer(rug::Integer::from(3))
        );
    }

    #[test]
    fn test_definition() {
        let v = eval_str("f[x_] := x^2; Definition[f]");
        match v {
            Value::Str(s) => {
                assert!(s.contains("f[x_]"), "got {}", s);
                assert!(s.contains(":>"), "got {}", s);
            }
            other => panic!("expected string, got {}", other),
        }
    }

    #[test]
    fn test_undefined_returns_unevaluated() {
        assert_eq!(
            eval_str("DownValues[5]"),
            Value::Call {
                head: "DownValues".to_string(),
                args: vec![Value::Integer(rug::Integer::from(5))]
            }
        );
    }
}
