//! control flow and errors: Catch, Throw, Check, CheckAbort, TimeConstrained, MemoryConstrained, Enclose, Goto/Label.
//!
//! Gap-fill module; see the individual builtin docs for semantics.
//!
//! Non-local control flow is modelled with dedicated [`EvalError`] variants
//! (`Thrown`, `ThrownTagged`, `Abort`, `Goto`) that unwind the evaluator until
//! a matching handler intercepts them. Only `Catch` and `Goto` need
//! unevaluated arguments and are therefore dispatched as special forms from
//! `eval::eval_call_inner`; the remaining builtins here rely on `HoldAll`.

use std::time::Instant;

use crate::ast::Expr;
use crate::env::Env;
use crate::eval::apply_function;
use crate::eval::eval;
use crate::value::{BuiltinFn, EvalError, Value};
use rug::Integer;

/// Register the builtins implemented in this module.
fn truthy(v: &Value) -> bool {
    matches!(v, Value::Bool(true)) || matches!(v, Value::Symbol(s) if s == "True")
}

/// Assert[test, tag?] — returns `Null`, emitting a message when `test` fails.
pub fn builtin_assert(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(unevaluated_values("Assert", args));
    }
    if truthy(&args[0]) {
        return Ok(Value::Null);
    }
    if matches!(&args[0], Value::Bool(false))
        || matches!(&args[0], Value::Symbol(s) if s == "False")
    {
        crate::messages::emit("Assert::asrt", &[format!("{}", args[0])]);
        return Ok(Value::Null);
    }
    Ok(unevaluated_values("Assert", args))
}

fn outcome_value(name: &str, actual: Value, expected: Value, success: bool) -> Value {
    let mut map = std::collections::HashMap::new();
    map.insert(
        "Outcome".to_string(),
        Value::Str(if success { "Success" } else { "Failure" }.to_string()),
    );
    map.insert("TestID".to_string(), Value::Str(name.to_string()));
    map.insert("ActualOutput".to_string(), actual);
    map.insert("ExpectedOutput".to_string(), expected);
    Value::Call {
        head: "TestResultObject".to_string(),
        args: vec![Value::Assoc(map)],
    }
}

/// VerificationTest[test] / VerificationTest[actual, expected] — a test result.
pub fn builtin_verification_test(args: &[Value]) -> Result<Value, EvalError> {
    let (actual, expected, success) = match args.len() {
        1 => {
            let ok = truthy(&args[0]);
            (args[0].clone(), Value::Bool(true), ok)
        }
        2 => {
            let ok = args[0].struct_eq(&args[1]);
            (args[0].clone(), args[1].clone(), ok)
        }
        _ => return Ok(unevaluated_values("VerificationTest", args)),
    };
    Ok(outcome_value("VerificationTest", actual, expected, success))
}

/// TestReport[tests] — aggregate counts for a list of test results.
pub fn builtin_test_report(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated_values("TestReport", args));
    }
    let items: Vec<&Value> = match &args[0] {
        Value::List(items) => items.iter().collect(),
        other => vec![other],
    };
    let mut success = 0i64;
    let mut failure = 0i64;
    for it in &items {
        if let Value::Call { head, args: a } = it
            && head == "TestResultObject"
            && let Some(Value::Assoc(map)) = a.first()
            && let Some(Value::Str(outcome)) = map.get("Outcome")
        {
            if outcome == "Success" {
                success += 1;
            } else {
                failure += 1;
            }
        }
    }
    let mut map = std::collections::HashMap::new();
    map.insert(
        "TestsCount".to_string(),
        Value::Integer(rug::Integer::from(items.len() as i64)),
    );
    map.insert(
        "SuccessCount".to_string(),
        Value::Integer(rug::Integer::from(success)),
    );
    map.insert(
        "FailureCount".to_string(),
        Value::Integer(rug::Integer::from(failure)),
    );
    Ok(Value::Call {
        head: "TestReportObject".to_string(),
        args: vec![Value::Assoc(map)],
    })
}

pub fn register(env: &Env) {
    register_pure(env, "Assert", builtin_assert);
    register_pure(env, "VerificationTest", builtin_verification_test);
    register_pure(env, "TestReport", builtin_test_report);
    register_pure(env, "Throw", builtin_throw);
    register_pure(env, "Abort", builtin_abort);
    register_pure(env, "Label", builtin_label);

    // HoldAll builtins: their first argument must be evaluated manually.
    for (name, func) in [
        (
            "Check",
            builtin_check as fn(&[Value], &Env) -> Result<Value, EvalError>,
        ),
        ("CheckAbort", builtin_check_abort),
        ("TimeConstrained", builtin_time_constrained),
        ("MemoryConstrained", builtin_memory_constrained),
        ("Enclose", builtin_enclose),
        ("Piecewise", builtin_piecewise),
        ("Unless", builtin_unless),
        ("WithCleanup", builtin_with_cleanup),
        ("IgnoringInactive", builtin_ignoring_inactive),
    ] {
        env.set(
            name.to_string(),
            Value::Builtin(name.to_string(), BuiltinFn::Env(func)),
        );
        env.set_attributes(
            name,
            vec![
                "HoldAll".to_string(),
                "Locked".to_string(),
                "ReadProtected".to_string(),
            ],
        );
    }
}

fn register_pure(env: &Env, name: &str, func: fn(&[Value]) -> Result<Value, EvalError>) {
    env.set(
        name.to_string(),
        Value::Builtin(name.to_string(), BuiltinFn::Pure(func)),
    );
}

/// `$Aborted` — the canonical result of an aborted evaluation.
fn aborted() -> Value {
    Value::Symbol("$Aborted".to_string())
}

// ── Throw / Abort / Label ────────────────────────────────────────────────────

/// `Throw[value]` / `Throw[value, tag]` — raise a value, unwinding to `Catch`.
pub fn builtin_throw(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => Err(EvalError::Thrown(Box::new(args[0].clone()))),
        2 => Err(EvalError::ThrownTagged {
            value: Box::new(args[0].clone()),
            tag: Box::new(args[1].clone()),
        }),
        _ => Err(EvalError::NoMatch {
            head: "Throw".to_string(),
            args: Box::new(args.to_vec()),
        }),
    }
}

/// `Abort[]` — abort the current evaluation (caught by `CheckAbort`/`Catch`).
pub fn builtin_abort(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return Err(EvalError::NoMatch {
            head: "Abort".to_string(),
            args: Box::new(args.to_vec()),
        });
    }
    Err(EvalError::Abort)
}

/// `Label[tag]` — a jump target marker; evaluates to its tag.
pub fn builtin_label(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::NoMatch {
            head: "Label".to_string(),
            args: Box::new(args.to_vec()),
        });
    }
    Ok(args[0].clone())
}

// ── Catch ────────────────────────────────────────────────────────────────────

/// `Catch[expr]`, `Catch[expr, tag]`, `Catch[expr, tag, f]`.
///
/// Dispatched as a special form (the first argument is held).
pub(crate) fn eval_catch(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 3 {
        return Ok(unevaluated("Catch", args));
    }
    // Evaluate the optional tag and handler.
    let catch_tag = match args.get(1) {
        Some(e) => Some(eval(e, env)?),
        None => None,
    };
    let handler = match args.get(2) {
        Some(e) => Some(eval(e, env)?),
        None => None,
    };

    match eval(&args[0], env) {
        Ok(v) => Ok(v),
        Err(EvalError::Thrown(value)) => dispatch_caught(*value, None, catch_tag, handler, env),
        Err(EvalError::ThrownTagged { value, tag }) => {
            dispatch_caught(*value, Some(*tag), catch_tag, handler, env)
        }
        Err(EvalError::Abort) => Ok(aborted()),
        Err(e) => Err(e),
    }
}

/// Decide whether a throw matches the current `Catch`; if so run the handler.
fn dispatch_caught(
    value: Value,
    throw_tag: Option<Value>,
    catch_tag: Option<Value>,
    handler: Option<Value>,
    env: &Env,
) -> Result<Value, EvalError> {
    let matched = match &catch_tag {
        // A tagless Catch catches every throw.
        None => true,
        // A pattern tag (`_`, `_t`, ...) matches any throw.
        Some(Value::Pattern(_)) => true,
        // A tagged Catch only matches a tagged throw with the same tag.
        Some(ct) => matches!(&throw_tag, Some(tt) if ct.struct_eq(tt)),
    };
    if !matched {
        // Re-raise unchanged so an outer handler can catch it.
        return match (value, throw_tag) {
            (v, None) => Err(EvalError::Thrown(Box::new(v))),
            (v, Some(t)) => Err(EvalError::ThrownTagged {
                value: Box::new(v),
                tag: Box::new(t),
            }),
        };
    }
    match handler {
        Some(f) => apply_function(&f, &[value], env),
        None => Ok(value),
    }
}

// ── Check / CheckAbort ───────────────────────────────────────────────────────

/// `Check[expr, failexpr]` / `Check[expr, failexpr, tag]`.
///
/// Evaluate `expr`; if it generates an error (but not a non-local transfer such
/// as `Throw`/`Return`/`Break`/`Goto`/`Abort`), evaluate `failexpr` instead.
pub fn builtin_check(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Ok(unevaluated_values("Check", args));
    }
    let expr = held_expr(&args[0]);
    match eval(&expr, env) {
        Ok(v) => Ok(v),
        Err(e) if is_message_error(&e) => eval(&held_expr(&args[1]), env),
        Err(e) => Err(e),
    }
}

/// `CheckAbort[expr, failexpr]` — like `Check` but only catches `Abort[]`.
pub fn builtin_check_abort(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated_values("CheckAbort", args));
    }
    let expr = held_expr(&args[0]);
    match eval(&expr, env) {
        Ok(v) => Ok(v),
        Err(EvalError::Abort) => eval(&held_expr(&args[1]), env),
        Err(e) => Err(e),
    }
}

/// Errors that represent a generated message (as opposed to a control transfer).
fn is_message_error(e: &EvalError) -> bool {
    !matches!(
        e,
        EvalError::Thrown(_)
            | EvalError::ThrownTagged { .. }
            | EvalError::Return(_)
            | EvalError::Break
            | EvalError::Continue
            | EvalError::Goto(_)
            | EvalError::Abort
    )
}

// ── TimeConstrained / MemoryConstrained ──────────────────────────────────────

/// `TimeConstrained[expr, t]` / `TimeConstrained[expr, t, failexpr]`.
///
/// Evaluates `expr` and, if it took longer than `t` seconds, returns
/// `failexpr` (or `$Aborted`). Note: this is a post-hoc check — evaluation is
/// not interrupted mid-way.
pub fn builtin_time_constrained(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Ok(unevaluated_values("TimeConstrained", args));
    }
    let seconds =
        crate::builtins::to_f64(&eval(&held_expr(&args[1]), env)?).unwrap_or(f64::INFINITY);

    let start = Instant::now();
    let result = eval(&held_expr(&args[0]), env);
    let elapsed = start.elapsed().as_secs_f64();

    let fail = || match args.get(2) {
        Some(f) => eval(&held_expr(f), env),
        None => Ok(aborted()),
    };

    match result {
        Ok(v) if seconds > 0.0 && elapsed <= seconds => Ok(v),
        Ok(_) => fail(),
        Err(e) if seconds <= 0.0 || elapsed > seconds => fail().map_err(|_| e),
        Err(e) => Err(e),
    }
}

/// `MemoryConstrained[expr, m]` / `MemoryConstrained[expr, m, failexpr]`.
///
/// Evaluates `expr` and returns `failexpr` (or `$Aborted`) when process memory
/// exceeds `m` bytes.
pub fn builtin_memory_constrained(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Ok(unevaluated_values("MemoryConstrained", args));
    }
    let limit = eval(&held_expr(&args[1]), env)
        .ok()
        .and_then(|v| crate::builtins::to_f64(&v))
        .unwrap_or(f64::INFINITY);

    let result = eval(&held_expr(&args[0]), env);

    if limit <= 0.0
        || process_memory_bytes()
            .map(|m| m as f64 > limit)
            .unwrap_or(false)
    {
        return match args.get(2) {
            Some(f) => eval(&held_expr(f), env),
            None => Ok(aborted()),
        };
    }
    result
}

/// Best-effort resident-set-size of the current process, in bytes.
fn process_memory_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let resident_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(resident_pages * 4096)
}

// ── Enclose ──────────────────────────────────────────────────────────────────

/// `Enclose[expr]` / `Enclose[expr, tag]` — a basic `Catch` variant that
/// returns the thrown value for a matching throw.
pub fn builtin_enclose(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated_values("Enclose", args));
    }
    let tag = match args.get(1) {
        Some(e) => Some(eval(&held_expr(e), env)?),
        None => None,
    };
    match eval(&held_expr(&args[0]), env) {
        Ok(v) => Ok(v),
        Err(EvalError::Thrown(value)) => {
            if tag.is_none() {
                Ok(*value)
            } else {
                Err(EvalError::Thrown(value))
            }
        }
        Err(EvalError::ThrownTagged { value, tag: t }) => match &tag {
            None => Ok(*value),
            Some(ct) if ct.struct_eq(&t) => Ok(*value),
            _ => Err(EvalError::ThrownTagged { value, tag: t }),
        },
        Err(e) => Err(e),
    }
}

// ── Piecewise / Unless / WithCleanup / IgnoringInactive ──────────────────────

/// `Piecewise[{{v1, c1}, {v2, c2}, …}, default]`.
///
/// Evaluate the conditions in order and return the value of the first branch
/// whose condition is `True`; if every condition is `False`, return `default`
/// (or `0` when no default is given). A call with a malformed piece list, or a
/// condition that does not reduce to a boolean, is returned unevaluated.
pub fn builtin_piecewise(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated_values("Piecewise", args));
    }
    let pieces = held_expr(&args[0]);
    let items = match &pieces {
        Expr::List(items) => items,
        _ => return Ok(unevaluated_values("Piecewise", args)),
    };
    for item in items {
        let pair = match item {
            Expr::List(pair) if pair.len() == 2 => pair,
            _ => return Ok(unevaluated_values("Piecewise", args)),
        };
        match eval(&pair[1], env)? {
            Value::Bool(true) => return eval(&pair[0], env),
            Value::Bool(false) => {}
            _ => return Ok(unevaluated_values("Piecewise", args)),
        }
    }
    match args.get(1) {
        Some(default) => eval(&held_expr(default), env),
        None => Ok(Value::Integer(Integer::from(0))),
    }
}

/// `Unless[test, body]` — evaluate `body` unless `test` evaluates to `True`.
///
/// With the `HoldAll` attribute the test and body are held; the test is
/// evaluated first and `body` only runs when the test is not `True`.
pub fn builtin_unless(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated_values("Unless", args));
    }
    if eval(&held_expr(&args[0]), env)?.to_bool() {
        Ok(Value::Null)
    } else {
        eval(&held_expr(&args[1]), env)
    }
}

/// `WithCleanup[expr, cleanup]` — evaluate `expr`, then always evaluate
/// `cleanup`, returning the result of `expr`.
///
/// An error raised by `expr` (including non-local transfers such as `Throw`)
/// propagates after the cleanup has run, so cleanup actions always execute.
pub fn builtin_with_cleanup(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated_values("WithCleanup", args));
    }
    let result = eval(&held_expr(&args[0]), env);
    let cleanup = eval(&held_expr(&args[1]), env);
    match result {
        Ok(value) => {
            cleanup?;
            Ok(value)
        }
        Err(error) => {
            // The primary error wins; a cleanup error is still surfaced when
            // the body itself succeeded (handled above).
            let _ = cleanup;
            Err(error)
        }
    }
}

/// `IgnoringInactive[expr]` — evaluate `expr`, treating `Inactive[…]` wrappers
/// as if they were active, then evaluate the resulting expression.
///
/// `Inactive[head]` becomes `head` and `Inactive[head][args]` becomes
/// `head[args]`; stripping is applied recursively throughout the result.
pub fn builtin_ignoring_inactive(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated_values("IgnoringInactive", args));
    }
    let value = eval(&held_expr(&args[0]), env)?;
    let (stripped, changed) = strip_inactive(value);
    if !changed {
        return Ok(stripped);
    }
    let expr = crate::eval::table::value_to_expr(&stripped);
    eval(&expr, env)
}

/// Recursively remove `Inactive[…]` wrappers. Returns the transformed value and
/// whether anything changed.
fn strip_inactive(value: Value) -> (Value, bool) {
    match value {
        Value::Call { head, args } => {
            let mut changed = false;
            let new_args = args
                .into_iter()
                .map(|a| {
                    let (na, c) = strip_inactive(a);
                    changed |= c;
                    na
                })
                .collect::<Vec<_>>();
            if head == "Inactive" {
                if new_args.len() == 1 {
                    return (new_args.into_iter().next().unwrap(), true);
                }
                return (
                    Value::Call {
                        head,
                        args: new_args,
                    },
                    changed,
                );
            }
            if let Some(inner) = head
                .strip_prefix("Inactive[")
                .and_then(|s| s.strip_suffix(']'))
            {
                return (
                    Value::Call {
                        head: inner.to_string(),
                        args: new_args,
                    },
                    true,
                );
            }
            (
                Value::Call {
                    head,
                    args: new_args,
                },
                changed,
            )
        }
        Value::List(items) => {
            let mut changed = false;
            let new_items = items
                .into_iter()
                .map(|i| {
                    let (ni, c) = strip_inactive(i);
                    changed |= c;
                    ni
                })
                .collect();
            (Value::List(new_items), changed)
        }
        other => (other, false),
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Extract the held expression from a `HoldAll` argument.
fn held_expr(v: &Value) -> Expr {
    match v {
        Value::Pattern(e) => e.clone(),
        other => crate::eval::table::value_to_expr(other),
    }
}

/// Build an unevaluated `head[args...]` call from raw held arguments.
fn unevaluated(name: &str, args: &[Expr]) -> Value {
    Value::Call {
        head: name.to_string(),
        args: args.iter().map(expr_to_held_value).collect(),
    }
}

fn unevaluated_values(name: &str, args: &[Value]) -> Value {
    Value::Call {
        head: name.to_string(),
        args: args.iter().map(|a| Value::Pattern(held_expr(a))).collect(),
    }
}

/// Convert an expression to a value without evaluating it (for unevaluated calls).
fn expr_to_held_value(e: &Expr) -> Value {
    match e {
        Expr::Integer(n) => Value::Integer(n.clone()),
        Expr::Real(r) => Value::Real(r.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Str(s) => Value::Str(s.clone()),
        Expr::Symbol(s) => Value::Symbol(s.clone()),
        Expr::Null => Value::Null,
        Expr::List(items) => Value::List(items.iter().map(expr_to_held_value).collect()),
        Expr::Call { head, args } => Value::Call {
            head: match head.as_ref() {
                Expr::Symbol(s) => s.clone(),
                other => other.to_string(),
            },
            args: args.iter().map(expr_to_held_value).collect(),
        },
        other => Value::Pattern(other.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use crate::lexer;
    use crate::parser;
    use rug::Integer;

    fn eval_str(input: &str) -> Value {
        let env = Env::new();
        builtins::register_builtins(&env);
        let tokens = lexer::tokenize(input).unwrap();
        let ast = parser::parse(tokens).unwrap_or_else(|e| {
            panic!("Parse error for input {:?}: {:?}", input, e);
        });
        crate::eval::eval_program(&ast, &env)
            .unwrap_or_else(|e| panic!("Eval error for input {:?}: {:?}", input, e))
    }

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    #[test]
    fn test_catch_throw_untagged() {
        assert_eq!(eval_str("Catch[Throw[42]]"), int(42));
        assert_eq!(eval_str("Catch[1 + Throw[7] + 100]"), int(7));
    }

    #[test]
    fn test_catch_tagged() {
        assert_eq!(eval_str("Catch[Throw[42, foo], foo]"), int(42));
        assert_eq!(eval_str("Catch[Catch[Throw[42, foo], bar], foo]"), int(42));
    }

    #[test]
    fn test_catch_handler() {
        assert_eq!(eval_str("Catch[Throw[4], _t, #^2 &]"), int(16));
    }

    #[test]
    fn test_throw_bubbles_to_top_level() {
        assert_eq!(eval_str("Throw[99]"), int(99));
    }

    #[test]
    fn test_catch_without_throw() {
        assert_eq!(eval_str("Catch[3 + 4]"), int(7));
    }

    #[test]
    fn test_check_success() {
        assert_eq!(eval_str("Check[1 + 1, 0]"), int(2));
    }

    #[test]
    fn test_check_catches_error() {
        // Error[] raises EvalError::Error which Check must intercept.
        assert_eq!(eval_str("Check[Error[\"boom\"], 0]"), int(0));
    }

    #[test]
    fn test_check_does_not_catch_throw() {
        assert_eq!(eval_str("Catch[Check[Throw[5], 0]]"), int(5));
    }

    #[test]
    fn test_check_abort() {
        assert_eq!(eval_str("CheckAbort[Abort[], 0]"), int(0));
        assert_eq!(eval_str("CheckAbort[1 + 1, 0]"), int(2));
    }

    #[test]
    fn test_abort_top_level() {
        assert_eq!(eval_str("Abort[]"), Value::Symbol("$Aborted".to_string()));
    }

    #[test]
    fn test_time_constrained_normal() {
        assert_eq!(eval_str("TimeConstrained[1 + 1, 100]"), int(2));
    }

    #[test]
    fn test_time_constrained_zero() {
        assert_eq!(
            eval_str("TimeConstrained[1 + 1, 0]"),
            Value::Symbol("$Aborted".to_string())
        );
        assert_eq!(eval_str("TimeConstrained[1 + 1, 0, 99]"), int(99));
    }

    #[test]
    fn test_memory_constrained_normal() {
        assert_eq!(eval_str("MemoryConstrained[1 + 1, 1000000000]"), int(2));
    }

    #[test]
    fn test_memory_constrained_zero() {
        assert_eq!(
            eval_str("MemoryConstrained[1 + 1, 0]"),
            Value::Symbol("$Aborted".to_string())
        );
        assert_eq!(eval_str("MemoryConstrained[1 + 1, 0, 7]"), int(7));
    }

    #[test]
    fn test_enclose() {
        assert_eq!(eval_str("Enclose[Throw[3]]"), int(3));
        assert_eq!(eval_str("Enclose[Throw[3, t], t]"), int(3));
        assert_eq!(eval_str("Enclose[1 + 1]"), int(2));
    }

    #[test]
    fn test_goto_label_basic() {
        // Jump forward over the assignment to y.
        assert_eq!(eval_str("x = 1; Goto[skip]; x = 2; Label[skip]; x"), int(1));
    }

    #[test]
    fn test_goto_label_backward_loop() {
        // Label before the Goto gives a backward jump.
        assert_eq!(
            eval_str("i = 0; Label[again]; i = i + 1; If[i < 4, Goto[again]]; i"),
            int(4)
        );
    }

    #[test]
    fn test_goto_unmatched_returns_unevaluated() {
        let v = eval_str("Goto[nowhere]");
        match v {
            Value::Call { head, .. } => assert_eq!(head, "Goto"),
            other => panic!("expected unevaluated Goto, got {:?}", other),
        }
    }

    #[test]
    fn test_throw_wrong_arity_unevaluated() {
        let v = builtin_throw(&[int(1), int(2), int(3)]).unwrap_err();
        assert!(matches!(v, EvalError::NoMatch { .. }));
    }

    fn assert_unevaluated(v: Value, head: &str) {
        match v {
            Value::Call { head: h, .. } => assert_eq!(h, head),
            other => panic!("expected unevaluated {head}, got {other:?}"),
        }
    }

    // ── Piecewise ────────────────────────────────────────────────────────────

    #[test]
    fn test_piecewise_first_true_branch() {
        assert_eq!(
            eval_str("Piecewise[{{1, False}, {2, True}, {3, True}}]"),
            int(2)
        );
        assert_eq!(eval_str("Piecewise[{{5, True}}]"), int(5));
    }

    #[test]
    fn test_piecewise_default() {
        assert_eq!(eval_str("Piecewise[{{1, False}, {2, False}}, 99]"), int(99));
        // No true branch and no default yields 0.
        assert_eq!(eval_str("Piecewise[{{1, False}}]"), int(0));
    }

    #[test]
    fn test_piecewise_only_selected_branch_evaluated() {
        assert_eq!(
            eval_str("Piecewise[{{10, True}, {Error[\"boom\"], True}}]"),
            int(10)
        );
    }

    #[test]
    fn test_piecewise_symbolic_condition_unevaluated() {
        let v = eval_str("Piecewise[{{a, b > 0}}]");
        assert_unevaluated(v, "Piecewise");
        // A default does not let a symbolic condition reduce.
        let v = eval_str("Piecewise[{{a, b > 0}}, 0]");
        assert_unevaluated(v, "Piecewise");
    }

    #[test]
    fn test_piecewise_malformed_unevaluated() {
        let v = eval_str("Piecewise[1]");
        assert_unevaluated(v, "Piecewise");
        let v = eval_str("Piecewise[{{1, True, False}}]");
        assert_unevaluated(v, "Piecewise");
    }

    // ── Unless ───────────────────────────────────────────────────────────────

    #[test]
    fn test_unless_false_runs_body() {
        assert_eq!(eval_str("Unless[False, 7]"), int(7));
        assert_eq!(eval_str("Unless[1 > 2, 7]"), int(7));
    }

    #[test]
    fn test_unless_true_returns_null() {
        assert_eq!(eval_str("Unless[True, 7]"), Value::Null);
        assert_eq!(eval_str("Unless[True, Error[\"boom\"]]"), Value::Null);
    }

    #[test]
    fn test_unless_body_side_effect() {
        assert_eq!(eval_str("Unless[False, x = 2]; x"), int(2));
    }

    #[test]
    fn test_unless_wrong_arity_unevaluated() {
        let v = eval_str("Unless[True]");
        assert_unevaluated(v, "Unless");
    }

    // ── WithCleanup ──────────────────────────────────────────────────────────

    #[test]
    fn test_with_cleanup_runs_and_returns_body() {
        assert_eq!(
            eval_str("{WithCleanup[y = 5, y = 9], y}"),
            Value::List(vec![int(5), int(9)])
        );
    }

    #[test]
    fn test_with_cleanup_runs_on_error() {
        assert_eq!(eval_str("Catch[WithCleanup[Throw[1], z = 3]]; z"), int(3));
    }

    #[test]
    fn test_with_cleanup_wrong_arity_unevaluated() {
        let v = eval_str("WithCleanup[1]");
        assert_unevaluated(v, "WithCleanup");
    }

    // ── IgnoringInactive ─────────────────────────────────────────────────────

    #[test]
    fn test_ignoring_inactive_activates_head() {
        assert_eq!(eval_str("IgnoringInactive[Inactive[Plus][1, 2]]"), int(3));
        assert_eq!(eval_str("IgnoringInactive[Inactive[Sin][0]]"), int(0));
    }

    #[test]
    fn test_ignoring_inactive_plain_expr() {
        assert_eq!(eval_str("IgnoringInactive[1 + 1]"), int(2));
    }

    #[test]
    fn test_ignoring_inactive_nested() {
        assert_eq!(
            eval_str("IgnoringInactive[{Inactive[Plus][1, 1], Inactive[Times][2, 3]}]"),
            Value::List(vec![int(2), int(6)])
        );
    }

    #[test]
    fn test_inactive_alone_stays_inactive() {
        let v = eval_str("Inactive[Plus][1, 2]");
        match v {
            Value::Call { head, .. } => assert_eq!(head, "Inactive[Plus]"),
            other => panic!("expected inactive call, got {other:?}"),
        }
    }

    #[test]
    fn test_ignoring_inactive_wrong_arity_unevaluated() {
        let v = eval_str("IgnoringInactive[1, 2]");
        assert_unevaluated(v, "IgnoringInactive");
    }

    // ── Check / TimeConstrained interactions ─────────────────────────────────

    #[test]
    fn test_check_does_not_catch_time_constrained_abort() {
        // TimeConstrained turns the abort into the plain value $Aborted, which
        // Check must pass through rather than treating as an error.
        assert_eq!(
            eval_str("Check[TimeConstrained[Abort[], 0], 42]"),
            Value::Symbol("$Aborted".to_string())
        );
        assert_eq!(
            eval_str("CheckAbort[TimeConstrained[Abort[], 0], 7]"),
            Value::Symbol("$Aborted".to_string())
        );
    }

    #[test]
    fn test_time_constrained_wraps_check() {
        // The inner Check handles the error and TimeConstrained returns it.
        assert_eq!(
            eval_str("TimeConstrained[Check[Error[\"x\"], 5], 100]"),
            int(5)
        );
    }

    #[test]
    fn test_check_time_constrained_error_uses_fail_expr() {
        // With a zero budget the TimeConstrained fail expression wins over the
        // error raised inside, so Check sees the successful fail value.
        assert_eq!(
            eval_str("Check[TimeConstrained[Error[\"x\"], 0], 42]"),
            Value::Symbol("$Aborted".to_string())
        );
        assert_eq!(
            eval_str("Check[TimeConstrained[Error[\"x\"], 0, 5], 42]"),
            int(5)
        );
    }

    #[test]
    fn test_assert_true_is_null() {
        assert_eq!(eval_str("Assert[1 == 1]"), Value::Null);
    }

    #[test]
    fn test_verification_test_outcome() {
        let out = eval_str("VerificationTest[1 == 1]").to_string();
        assert!(out.contains("\"Outcome\" -> \"Success\""), "got {out}");
        let out = eval_str("VerificationTest[1, 2]").to_string();
        assert!(out.contains("\"Outcome\" -> \"Failure\""), "got {out}");
    }

    #[test]
    fn test_test_report_counts() {
        let out =
            eval_str("TestReport[{VerificationTest[True], VerificationTest[False]}]").to_string();
        assert!(out.contains("\"TestsCount\" -> 2"), "got {out}");
        assert!(out.contains("\"FailureCount\" -> 1"), "got {out}");
    }
}
