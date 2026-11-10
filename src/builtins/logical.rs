use crate::env::Env;
use crate::value::{EvalError, Value};

/// Evaluate a potentially-held value (Value::Pattern) in the environment.
/// If the value is already evaluated (not a Pattern), returns it as-is.
fn eval_held(val: &Value, env: &Env) -> Result<Value, EvalError> {
    match val {
        Value::Pattern(expr) => crate::eval::eval(expr, env),
        _ => Ok(val.clone()),
    }
}

/// Logical negation of an already-evaluated value.
///
/// `True`/`False` flip; symbolic comparisons are rewritten to their negations
/// (`Not[a == b]` → `a != b`) as Wolfram Language does; anything else stays as
/// an unevaluated `Not[...]`.
pub(crate) fn not_value(val: Value) -> Value {
    match val {
        Value::Bool(b) => Value::Bool(!b),
        Value::Call { head, args } => match head.as_str() {
            "Equal" if args.len() == 2 => Value::Call {
                head: "Unequal".into(),
                args,
            },
            "Unequal" if args.len() == 2 => Value::Call {
                head: "Equal".into(),
                args,
            },
            "Less" if args.len() == 2 => Value::Call {
                head: "GreaterEqual".into(),
                args,
            },
            "Greater" if args.len() == 2 => Value::Call {
                head: "LessEqual".into(),
                args,
            },
            "LessEqual" if args.len() == 2 => Value::Call {
                head: "Greater".into(),
                args,
            },
            "GreaterEqual" if args.len() == 2 => Value::Call {
                head: "Less".into(),
                args,
            },
            "Not" if args.len() == 1 => args.into_iter().next().unwrap(),
            _ => Value::Call {
                head: "Not".into(),
                args: vec![Value::Call { head, args }],
            },
        },
        other => Value::Call {
            head: "Not".into(),
            args: vec![other],
        },
    }
}

/// Binary conjunction of already-evaluated operands, matching [`builtin_and`].
///
/// Used by the bytecode VM and JIT so their inlined `And` agrees with the
/// tree-walking evaluator.
pub(crate) fn and_values(a: Value, b: Value) -> Value {
    match (&a, &b) {
        (Value::Bool(false), _) | (_, Value::Bool(false)) => Value::Bool(false),
        (Value::Bool(true), _) => b,
        (_, Value::Bool(true)) => a,
        _ => Value::Call {
            head: "And".into(),
            args: vec![a, b],
        },
    }
}

/// Binary disjunction of already-evaluated operands, matching [`builtin_or`].
pub(crate) fn or_values(a: Value, b: Value) -> Value {
    match (&a, &b) {
        (Value::Bool(true), _) | (_, Value::Bool(true)) => Value::Bool(true),
        (Value::Bool(false), _) => b,
        (_, Value::Bool(false)) => a,
        _ => Value::Call {
            head: "Or".into(),
            args: vec![a, b],
        },
    }
}

/// And[args] — short-circuit logical conjunction.
///
/// And[] = True. And[x] = x. Arguments are evaluated left to right; the first
/// `False` short-circuits to `False`. `True` arguments are dropped and the
/// remaining (symbolic) arguments are conjoined.
pub fn builtin_and(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Bool(true));
    }
    let mut rest: Vec<Value> = Vec::new();
    for arg in args {
        match eval_held(arg, env)? {
            Value::Bool(false) => return Ok(Value::Bool(false)),
            Value::Bool(true) => {}
            val => rest.push(val),
        }
    }
    match rest.len() {
        0 => Ok(Value::Bool(true)),
        1 => Ok(rest.pop().unwrap()),
        _ => Ok(Value::Call {
            head: "And".into(),
            args: rest,
        }),
    }
}

/// Or[args] — short-circuit logical disjunction.
///
/// Or[] = False. Or[x] = x. Arguments are evaluated left to right; the first
/// `True` short-circuits to `True`. `False` arguments are dropped and the
/// remaining (symbolic) arguments are disjoined.
pub fn builtin_or(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Bool(false));
    }
    let mut rest: Vec<Value> = Vec::new();
    for arg in args {
        match eval_held(arg, env)? {
            Value::Bool(true) => return Ok(Value::Bool(true)),
            Value::Bool(false) => {}
            val => rest.push(val),
        }
    }
    match rest.len() {
        0 => Ok(Value::Bool(false)),
        1 => Ok(rest.pop().unwrap()),
        _ => Ok(Value::Call {
            head: "Or".into(),
            args: rest,
        }),
    }
}

/// Not[expr] — logical negation (see [`not_value`]).
pub fn builtin_not(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Not requires exactly 1 argument".to_string(),
        ));
    }
    Ok(not_value(args[0].clone()))
}

/// Xor[args] — logical exclusive OR (parity).
///
/// Xor[] = False. Xor[x] = x. Matching `True` arguments cancel in pairs; the
/// result is `True`/`False` when no symbolic arguments remain.
pub fn builtin_xor(args: &[Value]) -> Result<Value, EvalError> {
    let mut parity = false;
    let mut rest: Vec<Value> = Vec::new();
    for arg in args {
        match arg {
            Value::Bool(true) => parity = !parity,
            Value::Bool(false) => {}
            val => {
                // Equal arguments cancel each other (x ⊻ x = False).
                if let Some(pos) = rest.iter().position(|v| v.struct_eq(val)) {
                    rest.remove(pos);
                } else {
                    rest.push(val.clone());
                }
            }
        }
    }
    Ok(match rest.len() {
        0 => Value::Bool(parity),
        1 => {
            let only = rest.pop().unwrap();
            if parity { not_value(only) } else { only }
        }
        _ => {
            let call = Value::Call {
                head: "Xor".into(),
                args: rest,
            };
            if parity { not_value(call) } else { call }
        }
    })
}

/// Nand[args] — logical NAND: Not[And[...]].
///
/// Nand[] = False. Nand[x] = Not[x].
pub fn builtin_nand(args: &[Value]) -> Result<Value, EvalError> {
    let mut rest: Vec<Value> = Vec::new();
    for arg in args {
        match arg {
            Value::Bool(false) => return Ok(Value::Bool(true)),
            Value::Bool(true) => {}
            val => rest.push(val.clone()),
        }
    }
    Ok(match rest.len() {
        0 => Value::Bool(false),
        1 => not_value(rest.pop().unwrap()),
        _ => not_value(Value::Call {
            head: "And".into(),
            args: rest,
        }),
    })
}

/// Nor[args] — logical NOR: Not[Or[...]].
///
/// Nor[] = True. Nor[x] = Not[x].
pub fn builtin_nor(args: &[Value]) -> Result<Value, EvalError> {
    let mut rest: Vec<Value> = Vec::new();
    for arg in args {
        match arg {
            Value::Bool(true) => return Ok(Value::Bool(false)),
            Value::Bool(false) => {}
            val => rest.push(val.clone()),
        }
    }
    Ok(match rest.len() {
        0 => Value::Bool(true),
        1 => not_value(rest.pop().unwrap()),
        _ => not_value(Value::Call {
            head: "Or".into(),
            args: rest,
        }),
    })
}

/// Implies[p, q] — logical implication: Not[p] || q.
///
/// `Implies[False, q]` and `Implies[p, True]` are `True`; `Implies[True, q]`
/// is `q`; `Implies[p, False]` is `Not[p]`; otherwise stays symbolic.
pub fn builtin_implies(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Implies requires exactly 2 arguments".to_string(),
        ));
    }
    let p = eval_held(&args[0], env)?;
    let q = eval_held(&args[1], env)?;
    Ok(match (&p, &q) {
        (Value::Bool(false), _) => Value::Bool(true),
        (Value::Bool(true), _) => q,
        (_, Value::Bool(true)) => Value::Bool(true),
        (_, Value::Bool(false)) => not_value(p),
        _ => Value::Call {
            head: "Implies".into(),
            args: vec![p, q],
        },
    })
}

/// Equivalent[args] — all arguments have the same truth value.
///
/// Equivalent[] = True. Equivalent[x] = True. Symbolic arguments are kept and
/// combined, cancelling out definite `True`/`False` arguments.
pub fn builtin_equivalent(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() <= 1 {
        return Ok(Value::Bool(true));
    }
    let mut has_true = false;
    let mut has_false = false;
    let mut rest: Vec<Value> = Vec::new();
    for arg in args {
        match arg {
            Value::Bool(true) => has_true = true,
            Value::Bool(false) => has_false = true,
            // Collapse structurally identical symbolic arguments (idempotent).
            val if !rest.iter().any(|v| v.struct_eq(val)) => rest.push(val.clone()),
            _ => {}
        }
    }
    // A definite True and a definite False can never all be equal.
    if has_true && has_false {
        return Ok(Value::Bool(false));
    }
    // With a definite value present the result is `And` of the symbolic
    // arguments (negated when the definite value is False).
    if has_true {
        return Ok(conjoin(rest));
    }
    if has_false {
        let negated = rest.into_iter().map(not_value).collect();
        return Ok(conjoin(negated));
    }
    // No definite values: all symbolic arguments must be equal.
    match rest.len() {
        0 | 1 => Ok(Value::Bool(true)),
        _ => Ok(Value::Call {
            head: "Equivalent".into(),
            args: rest,
        }),
    }
}

/// Conjoin already-evaluated operands into `True`, the single operand, or an
/// `And[...]` call.
fn conjoin(mut operands: Vec<Value>) -> Value {
    match operands.len() {
        0 => Value::Bool(true),
        1 => operands.pop().unwrap(),
        _ => Value::Call {
            head: "And".into(),
            args: operands,
        },
    }
}

/// Majority[args] — majority rule.
///
/// Requires an odd number of arguments.
/// Returns True if more than half of the arguments are truthy.
pub fn builtin_majority(args: &[Value]) -> Result<Value, EvalError> {
    #[allow(clippy::manual_is_multiple_of)]
    if args.is_empty() || args.len() % 2 == 0 {
        return Err(EvalError::Error(
            "Majority requires an odd number of arguments".to_string(),
        ));
    }
    let count = args.iter().filter(|v| v.to_bool()).count();
    Ok(Value::Bool(count > args.len() / 2))
}

/// Boole[expr] — convert boolean to integer.
///
/// Returns 1 if expr is True, 0 otherwise.
pub fn builtin_boole(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Boole requires exactly 1 argument".to_string(),
        ));
    }
    if matches!(&args[0], Value::Bool(true)) {
        Ok(Value::Integer(rug::Integer::from(1)))
    } else {
        Ok(Value::Integer(rug::Integer::from(0)))
    }
}

/// BooleanQ[expr] — test if expr is a boolean (True or False).
pub fn builtin_boolean_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "BooleanQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(matches!(&args[0], Value::Bool(_))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Env;

    fn test_env() -> Env {
        Env::new()
    }

    fn b(b: bool) -> Value {
        Value::Bool(b)
    }

    fn sym(s: &str) -> Value {
        Value::Symbol(s.into())
    }

    fn call(head: &str, args: Vec<Value>) -> Value {
        Value::Call {
            head: head.into(),
            args,
        }
    }

    fn eval_str(src: &str) -> Value {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let tokens = crate::lexer::tokenize(src).unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        let mut last = Value::Null;
        for expr in &ast {
            last = crate::eval::eval(expr, &env).unwrap();
        }
        last
    }

    // ── And ──

    #[test]
    fn test_and_empty() {
        let env = test_env();
        assert_eq!(builtin_and(&[], &env).unwrap(), b(true));
    }

    #[test]
    fn test_and_single_arg() {
        let env = test_env();
        assert_eq!(builtin_and(&[b(true)], &env).unwrap(), b(true));
        assert_eq!(builtin_and(&[b(false)], &env).unwrap(), b(false));
    }

    #[test]
    fn test_and_two_args() {
        let env = test_env();
        assert_eq!(builtin_and(&[b(true), b(true)], &env).unwrap(), b(true));
        assert_eq!(builtin_and(&[b(true), b(false)], &env).unwrap(), b(false));
        assert_eq!(builtin_and(&[b(false), b(true)], &env).unwrap(), b(false));
        assert_eq!(builtin_and(&[b(false), b(false)], &env).unwrap(), b(false));
    }

    #[test]
    fn test_and_returns_last_value() {
        let env = test_env();
        let forty_two = Value::Integer(rug::Integer::from(42));
        assert_eq!(
            builtin_and(&[b(true), forty_two.clone()], &env).unwrap(),
            forty_two
        );
    }

    #[test]
    fn test_and_symbolic_is_preserved() {
        let env = test_env();
        // A symbolic argument is neither True nor False and must survive.
        assert_eq!(
            builtin_and(&[sym("a"), sym("b")], &env).unwrap(),
            call("And", vec![sym("a"), sym("b")])
        );
        // A later False still short-circuits the conjunction.
        assert_eq!(builtin_and(&[sym("a"), b(false)], &env).unwrap(), b(false));
        // True arguments are dropped.
        assert_eq!(builtin_and(&[b(true), sym("a")], &env).unwrap(), sym("a"));
    }

    #[test]
    fn test_and_short_circuits() {
        let out = eval_str("And[False, 1/0]");
        assert_eq!(out, b(false));
    }

    // ── Or ──

    #[test]
    fn test_or_empty() {
        let env = test_env();
        assert_eq!(builtin_or(&[], &env).unwrap(), b(false));
    }

    #[test]
    fn test_or_single_arg() {
        let env = test_env();
        assert_eq!(builtin_or(&[b(true)], &env).unwrap(), b(true));
        assert_eq!(builtin_or(&[b(false)], &env).unwrap(), b(false));
    }

    #[test]
    fn test_or_two_args() {
        let env = test_env();
        assert_eq!(builtin_or(&[b(true), b(true)], &env).unwrap(), b(true));
        assert_eq!(builtin_or(&[b(true), b(false)], &env).unwrap(), b(true));
        assert_eq!(builtin_or(&[b(false), b(true)], &env).unwrap(), b(true));
        assert_eq!(builtin_or(&[b(false), b(false)], &env).unwrap(), b(false));
    }

    #[test]
    fn test_or_returns_first_truthy() {
        let env = test_env();
        let forty_two = Value::Integer(rug::Integer::from(42));
        assert_eq!(
            builtin_or(&[b(false), forty_two.clone()], &env).unwrap(),
            forty_two
        );
    }

    #[test]
    fn test_or_symbolic_is_preserved() {
        let env = test_env();
        assert_eq!(
            builtin_or(&[sym("a"), sym("b")], &env).unwrap(),
            call("Or", vec![sym("a"), sym("b")])
        );
        assert_eq!(builtin_or(&[sym("a"), b(true)], &env).unwrap(), b(true));
        assert_eq!(builtin_or(&[b(false), sym("a")], &env).unwrap(), sym("a"));
    }

    // ── Not ──

    #[test]
    fn test_not() {
        assert_eq!(builtin_not(&[b(true)]).unwrap(), b(false));
        assert_eq!(builtin_not(&[b(false)]).unwrap(), b(true));
    }

    #[test]
    fn test_not_symbolic() {
        assert_eq!(
            builtin_not(&[sym("x")]).unwrap(),
            call("Not", vec![sym("x")])
        );
        // Comparisons are negated structurally.
        assert_eq!(
            builtin_not(&[call("Equal", vec![sym("a"), sym("b")])]).unwrap(),
            call("Unequal", vec![sym("a"), sym("b")])
        );
        assert_eq!(
            builtin_not(&[call("Less", vec![sym("a"), sym("b")])]).unwrap(),
            call("GreaterEqual", vec![sym("a"), sym("b")])
        );
        assert_eq!(
            builtin_not(&[call("Not", vec![sym("x")])]).unwrap(),
            sym("x")
        );
    }

    #[test]
    fn test_not_wrong_arity() {
        assert!(builtin_not(&[]).is_err());
        assert!(builtin_not(&[b(true), b(false)]).is_err());
    }

    // ── Xor ──

    #[test]
    fn test_xor_empty() {
        assert_eq!(builtin_xor(&[]).unwrap(), b(false));
    }

    #[test]
    fn test_xor_single() {
        assert_eq!(builtin_xor(&[b(true)]).unwrap(), b(true));
        assert_eq!(builtin_xor(&[b(false)]).unwrap(), b(false));
    }

    #[test]
    fn test_xor_two_args() {
        assert_eq!(builtin_xor(&[b(true), b(true)]).unwrap(), b(false));
        assert_eq!(builtin_xor(&[b(true), b(false)]).unwrap(), b(true));
        assert_eq!(builtin_xor(&[b(false), b(true)]).unwrap(), b(true));
        assert_eq!(builtin_xor(&[b(false), b(false)]).unwrap(), b(false));
    }

    #[test]
    fn test_xor_three_args() {
        // True, True, True → 3 trues (odd) → True
        assert_eq!(builtin_xor(&[b(true), b(true), b(true)]).unwrap(), b(true));
        // True, True, False → 2 trues (even) → False
        assert_eq!(
            builtin_xor(&[b(true), b(true), b(false)]).unwrap(),
            b(false)
        );
    }

    #[test]
    fn test_xor_symbolic_cancels() {
        assert_eq!(builtin_xor(&[sym("a"), sym("a")]).unwrap(), b(false));
        assert_eq!(
            builtin_xor(&[sym("a"), sym("a"), sym("a")]).unwrap(),
            sym("a")
        );
        assert_eq!(
            builtin_xor(&[sym("a"), sym("b")]).unwrap(),
            call("Xor", vec![sym("a"), sym("b")])
        );
        // Xor[a, True] = Not[a].
        assert_eq!(
            builtin_xor(&[sym("a"), b(true)]).unwrap(),
            call("Not", vec![sym("a")])
        );
    }

    // ── Nand ──

    #[test]
    fn test_nand_empty() {
        assert_eq!(builtin_nand(&[]).unwrap(), b(false));
    }

    #[test]
    fn test_nand_two_args() {
        assert_eq!(builtin_nand(&[b(true), b(true)]).unwrap(), b(false));
        assert_eq!(builtin_nand(&[b(true), b(false)]).unwrap(), b(true));
        assert_eq!(builtin_nand(&[b(false), b(true)]).unwrap(), b(true));
        assert_eq!(builtin_nand(&[b(false), b(false)]).unwrap(), b(true));
    }

    #[test]
    fn test_nand_symbolic() {
        assert_eq!(
            builtin_nand(&[sym("a"), sym("b")]).unwrap(),
            call("Not", vec![call("And", vec![sym("a"), sym("b")])])
        );
        assert_eq!(
            builtin_nand(&[sym("a")]).unwrap(),
            call("Not", vec![sym("a")])
        );
        assert_eq!(builtin_nand(&[sym("a"), b(false)]).unwrap(), b(true));
    }

    // ── Nor ──

    #[test]
    fn test_nor_empty() {
        assert_eq!(builtin_nor(&[]).unwrap(), b(true));
    }

    #[test]
    fn test_nor_two_args() {
        assert_eq!(builtin_nor(&[b(true), b(true)]).unwrap(), b(false));
        assert_eq!(builtin_nor(&[b(true), b(false)]).unwrap(), b(false));
        assert_eq!(builtin_nor(&[b(false), b(true)]).unwrap(), b(false));
        assert_eq!(builtin_nor(&[b(false), b(false)]).unwrap(), b(true));
    }

    #[test]
    fn test_nor_symbolic() {
        assert_eq!(builtin_nor(&[sym("a"), b(true)]).unwrap(), b(false));
        assert_eq!(
            builtin_nor(&[sym("a")]).unwrap(),
            call("Not", vec![sym("a")])
        );
    }

    // ── Implies ──

    #[test]
    fn test_implies_truth_table() {
        let env = test_env();
        assert_eq!(builtin_implies(&[b(true), b(true)], &env).unwrap(), b(true));
        assert_eq!(
            builtin_implies(&[b(true), b(false)], &env).unwrap(),
            b(false)
        );
        assert_eq!(
            builtin_implies(&[b(false), b(true)], &env).unwrap(),
            b(true)
        );
        assert_eq!(
            builtin_implies(&[b(false), b(false)], &env).unwrap(),
            b(true)
        );
    }

    #[test]
    fn test_implies_symbolic() {
        let env = test_env();
        assert_eq!(
            builtin_implies(&[b(true), sym("a")], &env).unwrap(),
            sym("a")
        );
        assert_eq!(
            builtin_implies(&[b(false), sym("a")], &env).unwrap(),
            b(true)
        );
        assert_eq!(
            builtin_implies(&[sym("a"), b(true)], &env).unwrap(),
            b(true)
        );
        assert_eq!(
            builtin_implies(&[sym("a"), b(false)], &env).unwrap(),
            call("Not", vec![sym("a")])
        );
        assert_eq!(
            builtin_implies(&[sym("a"), sym("b")], &env).unwrap(),
            call("Implies", vec![sym("a"), sym("b")])
        );
    }

    #[test]
    fn test_implies_wrong_arity() {
        let env = test_env();
        assert!(builtin_implies(&[], &env).is_err());
        assert!(builtin_implies(&[b(true)], &env).is_err());
        assert!(builtin_implies(&[b(true), b(true), b(true)], &env).is_err());
    }

    // ── Equivalent ──

    #[test]
    fn test_equivalent_empty() {
        assert_eq!(builtin_equivalent(&[]).unwrap(), b(true));
    }

    #[test]
    fn test_equivalent_single() {
        assert_eq!(builtin_equivalent(&[b(true)]).unwrap(), b(true));
        assert_eq!(builtin_equivalent(&[b(false)]).unwrap(), b(true));
    }

    #[test]
    fn test_equivalent_two_args() {
        assert_eq!(builtin_equivalent(&[b(true), b(true)]).unwrap(), b(true));
        assert_eq!(builtin_equivalent(&[b(true), b(false)]).unwrap(), b(false));
        assert_eq!(builtin_equivalent(&[b(false), b(true)]).unwrap(), b(false));
        assert_eq!(builtin_equivalent(&[b(false), b(false)]).unwrap(), b(true));
    }

    #[test]
    fn test_equivalent_three_args() {
        assert_eq!(
            builtin_equivalent(&[b(true), b(true), b(true)]).unwrap(),
            b(true)
        );
        assert_eq!(
            builtin_equivalent(&[b(true), b(false), b(true)]).unwrap(),
            b(false)
        );
    }

    #[test]
    fn test_equivalent_symbolic() {
        assert_eq!(builtin_equivalent(&[sym("a"), sym("a")]).unwrap(), b(true));
        assert_eq!(builtin_equivalent(&[b(true), sym("a")]).unwrap(), sym("a"));
        assert_eq!(
            builtin_equivalent(&[b(false), sym("a")]).unwrap(),
            call("Not", vec![sym("a")])
        );
        assert_eq!(
            builtin_equivalent(&[sym("a"), sym("b")]).unwrap(),
            call("Equivalent", vec![sym("a"), sym("b")])
        );
        // Duplicated symbols collapse; a definite False negates the result.
        assert_eq!(
            builtin_equivalent(&[b(false), sym("a"), sym("a")]).unwrap(),
            call("Not", vec![sym("a")])
        );
        assert_eq!(
            builtin_equivalent(&[b(false), b(false), sym("a")]).unwrap(),
            call("Not", vec![sym("a")])
        );
        assert_eq!(
            builtin_equivalent(&[b(true), b(false), sym("a")]).unwrap(),
            b(false)
        );
    }

    // ── Majority ──

    #[test]
    fn test_majority_basic() {
        // 3 args: need 2+ to be True
        assert_eq!(
            builtin_majority(&[b(true), b(true), b(false)]).unwrap(),
            b(true)
        );
        assert_eq!(
            builtin_majority(&[b(true), b(false), b(false)]).unwrap(),
            b(false)
        );
    }

    #[test]
    fn test_majority_odd_required() {
        assert!(builtin_majority(&[]).is_err());
        assert!(builtin_majority(&[b(true), b(false)]).is_err());
        assert!(builtin_majority(&[b(true), b(false), b(true), b(false)]).is_err());
    }

    // ── Boole ──

    #[test]
    fn test_boole_true() {
        let result = builtin_boole(&[b(true)]).unwrap();
        assert_eq!(result, Value::Integer(rug::Integer::from(1)));
    }

    #[test]
    fn test_boole_false() {
        let result = builtin_boole(&[b(false)]).unwrap();
        assert_eq!(result, Value::Integer(rug::Integer::from(0)));
    }

    #[test]
    fn test_boole_non_bool() {
        let result = builtin_boole(&[Value::Integer(rug::Integer::from(42))]).unwrap();
        assert_eq!(result, Value::Integer(rug::Integer::from(0)));
        let result = builtin_boole(&[Value::Str("hello".to_string())]).unwrap();
        assert_eq!(result, Value::Integer(rug::Integer::from(0)));
        let result = builtin_boole(&[Value::Null]).unwrap();
        assert_eq!(result, Value::Integer(rug::Integer::from(0)));
    }

    #[test]
    fn test_boole_wrong_arity() {
        assert!(builtin_boole(&[]).is_err());
        assert!(builtin_boole(&[b(true), b(false)]).is_err());
    }

    // ── BooleanQ ──

    #[test]
    fn test_boolean_q_true() {
        assert_eq!(builtin_boolean_q(&[b(true)]).unwrap(), b(true));
        assert_eq!(builtin_boolean_q(&[b(false)]).unwrap(), b(true));
    }

    #[test]
    fn test_boolean_q_false() {
        assert_eq!(
            builtin_boolean_q(&[Value::Integer(rug::Integer::from(1))]).unwrap(),
            b(false)
        );
        assert_eq!(
            builtin_boolean_q(&[Value::Str("hello".to_string())]).unwrap(),
            b(false)
        );
        assert_eq!(builtin_boolean_q(&[Value::Null]).unwrap(), b(false));
        assert_eq!(
            builtin_boolean_q(&[Value::Symbol("x".to_string())]).unwrap(),
            b(false)
        );
    }

    #[test]
    fn test_boolean_q_wrong_arity() {
        assert!(builtin_boolean_q(&[]).is_err());
        assert!(builtin_boolean_q(&[b(true), b(false)]).is_err());
    }
}
