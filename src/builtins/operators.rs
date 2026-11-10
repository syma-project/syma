use crate::ast::Expr;
use crate::env::Env;
use crate::eval::apply_function;
use crate::value::{EvalError, Value};
use rug::Integer;
use std::rc::Rc;

// ── Identity ──

/// Identity[expr] returns expr unchanged.
pub fn builtin_identity(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        0 => Ok(Value::Null),
        1 => Ok(args[0].clone()),
        _ => Ok(Value::Sequence(args.to_vec())),
    }
}

// ── Helper: create env with stored function values ──

/// Store function values under internal names.
///
/// Note: the names intentionally contain no `_`, because `eval` treats any
/// symbol containing `_` as a blank pattern rather than a normal binding.
fn func_env_with_stored(parent: &Env, values: Rc<Vec<Value>>) -> (Env, Vec<String>) {
    let child = parent.child();
    let mut names = Vec::new();
    for (i, v) in values.iter().enumerate() {
        let name = format!("op{}", i);
        child.set(name.clone(), v.clone());
        names.push(name);
    }
    (child, names)
}

/// Build `names[0][names[1][...[names.last()[#]]]]` when `right_to_left`, and the
/// reverse nesting (`names.last()[...[names[0][#]]]`) otherwise. `#` is the slot.
fn compose_body(names: &[String], right_to_left: bool) -> Expr {
    let mut body = Expr::Slot(None);
    let ordered: Box<dyn Iterator<Item = &String>> = if right_to_left {
        Box::new(names.iter().rev())
    } else {
        Box::new(names.iter())
    };
    for name in ordered {
        body = Expr::Call {
            head: Box::new(Expr::Symbol(name.clone())),
            args: vec![body],
        };
    }
    body
}

// ── Helper: get list slice from Value ──

fn get_list(val: &Value) -> Result<&[Value], EvalError> {
    match val {
        Value::List(items) => Ok(items.as_slice()),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: val.type_name().to_string(),
        }),
    }
}

// ── Composition / RightComposition ──

/// Composition[f, g, h, ...][x] = f(g(h(...(x))))
/// Compose functions right-to-left: rightmost applied first.
pub fn builtin_composition(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Symbol("Identity".to_string()));
    }
    if args.len() == 1 {
        return Ok(args[0].clone());
    }
    let funcs: Rc<Vec<Value>> = Rc::new(args.to_vec());
    let (child_env, names) = func_env_with_stored(env, funcs);
    // Function names are bound in order [f, g, h]; nesting right-to-left gives
    // f[g[h[#]]].
    let body = compose_body(&names, true);
    Ok(Value::PureFunction {
        body,
        slot_count: 1,
        params: vec![],
        env: Some(child_env),
    })
}

/// RightComposition[f, g, h, ...] = Composition[..., h, g, f]
/// Leftmost function is applied first: (...h(g(f(x)))).
pub fn builtin_right_composition(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Symbol("Identity".to_string()));
    }
    if args.len() == 1 {
        return Ok(args[0].clone());
    }
    let funcs: Rc<Vec<Value>> = Rc::new(args.to_vec());
    let (child_env, names) = func_env_with_stored(env, funcs);
    // Applying left-to-right gives h[g[f[#]]].
    let body = compose_body(&names, false);
    Ok(Value::PureFunction {
        body,
        slot_count: 1,
        params: vec![],
        env: Some(child_env),
    })
}

// ── Through ──

/// Through[{f, g, h}[expr]] applies each function to the same expression,
/// giving `{f[expr], g[expr], h[expr]}`. A single function head passes through
/// directly: `Through[f[expr]]` = `f[expr]`.
pub fn builtin_through(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Through requires exactly 1 argument".to_string(),
        ));
    }

    match &args[0] {
        Value::Call { head, args: inner } => {
            let funcs = parse_through_head(head);
            if funcs.len() == 1 {
                apply_function(&funcs[0], inner, env)
            } else {
                let mut result = Vec::with_capacity(funcs.len());
                for f in &funcs {
                    result.push(apply_function(f, inner, env)?);
                }
                Ok(Value::List(result))
            }
        }
        Value::List(funcs) => {
            let mut result = Vec::with_capacity(funcs.len());
            for f in funcs {
                result.push(apply_function(f, &[], env)?);
            }
            Ok(Value::List(result))
        }
        // Anything else (e.g. an already-evaluated atom) passes through.
        other => Ok(other.clone()),
    }
}

/// Recover function values from a `Call` head string.
///
/// A head is either a single name (e.g. `"Abs"`) or the rendered form of a list
/// of functions (e.g. `"{Builtin[Abs], Builtin[Sign]}"`), since `Value::Call`
/// stores its head as a string.
fn parse_through_head(head: &str) -> Vec<Value> {
    let trimmed = head.trim();
    if let Some(inner) = trimmed.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        inner
            .split(',')
            .map(|part| through_func(part.trim()))
            .collect()
    } else {
        vec![through_func(trimmed)]
    }
}

fn through_func(part: &str) -> Value {
    for prefix in ["Builtin[", "Function["] {
        if let Some(name) = part.strip_prefix(prefix).and_then(|s| s.strip_suffix(']'))
            && !name.is_empty()
            && !name.contains('[')
        {
            return Value::Symbol(name.to_string());
        }
    }
    Value::Symbol(part.to_string())
}

// ── OperatorApply ──

/// OperatorApply[f][{a, b, c}] = f[a, b, c]
pub fn builtin_operator_apply(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "OperatorApply requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            apply_function(items.first().unwrap_or(&Value::Null), &items[1..], env)
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── Curry ──

/// Curry[f, n] converts a function of n arguments into nested unary functions.
/// Curry[f, 2][a][b] = f[a, b]
pub fn builtin_curry(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Curry requires exactly 2 arguments: Curry[f, n]".to_string(),
        ));
    }
    let func = &args[0];
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if n < 0 {
        return Err(EvalError::Error(
            "Curry: arity must be non-negative".to_string(),
        ));
    }

    if n == 0 {
        return Ok(func.clone());
    }

    let n = n as usize;
    let child = env.child();
    child.set("curryFn".to_string(), func.clone());

    // The first argument is bound as a named parameter; each further argument
    // is bound by an inner `Function`, so the call chain accumulates its args
    // and finally applies the stored function to all of them at once.
    let mut bound = vec!["curryArg0".to_string()];
    let body = curry_body("curryFn", &mut bound, n);
    Ok(Value::PureFunction {
        body,
        slot_count: 0,
        params: vec!["curryArg0".to_string()],
        env: Some(child),
    })
}

/// Build the body of a curried chain. `bound` holds the parameter names already
/// bound; while fewer than `n` names are bound, wrap the rest in a nested
/// `Function[{next}, ...]`. Once `n` names exist, apply `fname` to all of them.
fn curry_body(fname: &str, bound: &mut Vec<String>, n: usize) -> Expr {
    if bound.len() == n {
        return Expr::Call {
            head: Box::new(Expr::Symbol(fname.to_string())),
            args: bound.iter().map(|p| Expr::Symbol(p.clone())).collect(),
        };
    }
    let next = format!("curryArg{}", bound.len());
    bound.push(next.clone());
    let inner = curry_body(fname, bound, n);
    bound.pop();
    Expr::Function {
        params: vec![next],
        body: Box::new(inner),
    }
}

// ── UnCurry ──

/// UnCurry[f] converts a curried function back to one taking a list of arguments.
/// UnCurry[F][{a, b, c}] = F[a][b][c]
pub fn builtin_uncurry(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "UnCurry requires exactly 1 argument".to_string(),
        ));
    }
    let func = &args[0];
    let child = env.child();
    child.set("uncurryFn".to_string(), func.clone());
    // Fold the list through `#1[#2]&`, i.e. apply the curried function to each
    // argument in turn: F[a][b][c].
    let apply_step = Expr::Function {
        params: vec!["uncurryG".to_string(), "uncurryX".to_string()],
        body: Box::new(Expr::Call {
            head: Box::new(Expr::Symbol("uncurryG".to_string())),
            args: vec![Expr::Symbol("uncurryX".to_string())],
        }),
    };
    let body = Expr::Call {
        head: Box::new(Expr::Symbol("Fold".to_string())),
        args: vec![
            apply_step,
            Expr::Symbol("uncurryFn".to_string()),
            Expr::Slot(None),
        ],
    };
    Ok(Value::PureFunction {
        body,
        slot_count: 1,
        params: vec![],
        env: Some(child),
    })
}

// ── Set operations ──

/// SubsetQ[list1, list2] checks if every element of list1 appears in list2.
pub fn builtin_subset_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "SubsetQ requires exactly 2 arguments".to_string(),
        ));
    }
    let l1 = get_list(&args[0])?;
    let l2 = get_list(&args[1])?;
    let result = l1
        .iter()
        .all(|item| l2.iter().any(|item2| item.struct_eq(item2)));
    Ok(Value::Bool(result))
}

/// SymmetricDifference[list1, list2] returns elements in either list but not both.
pub fn builtin_symmetric_difference(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "SymmetricDifference requires exactly 2 arguments".to_string(),
        ));
    }
    let a = get_list(&args[0])?;
    let b = get_list(&args[1])?;
    let result: Vec<Value> = a
        .iter()
        .filter(|item| !b.iter().any(|item2| item.struct_eq(item2)))
        .cloned()
        .chain(
            b.iter()
                .filter(|item| !a.iter().any(|item2| item.struct_eq(item2)))
                .cloned(),
        )
        .collect();
    Ok(Value::List(result))
}

// ── SelectFirst / SelectLast ──

/// SelectFirst[list, crit] returns the first element matching the predicate.
pub fn builtin_select_first(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "SelectFirst requires exactly 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let crit = &args[1];
    for item in items {
        let keep = apply_function(crit, &[item.clone()], env)?;
        if keep.to_bool() {
            return Ok(item.clone());
        }
    }
    Ok(Value::Call {
        head: "Missing".to_string(),
        args: vec![],
    })
}

/// SelectLast[list, crit] returns the last element matching the predicate.
pub fn builtin_select_last(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "SelectLast requires exactly 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let crit = &args[1];
    let mut last_match: Option<Value> = None;
    for item in items {
        let keep = apply_function(crit, &[item.clone()], env)?;
        if keep.to_bool() {
            last_match = Some(item.clone());
        }
    }
    match last_match {
        Some(val) => Ok(val),
        None => Ok(Value::Call {
            head: "Missing".to_string(),
            args: vec![],
        }),
    }
}

// ── PositionFirst / PositionLast ──

/// PositionFirst[list, elem] returns the 1-indexed first position of elem.
pub fn builtin_position_first(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "PositionFirst requires exactly 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let elem = &args[1];
    for (i, item) in items.iter().enumerate() {
        if item.struct_eq(elem) {
            return Ok(Value::Integer(Integer::from((i + 1) as i64)));
        }
    }
    Ok(Value::Call {
        head: "Missing".to_string(),
        args: vec![],
    })
}

/// PositionLast[list, elem] returns the 1-indexed last position of elem.
pub fn builtin_position_last(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "PositionLast requires exactly 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let elem = &args[1];
    let mut last_pos: Option<usize> = None;
    for (i, item) in items.iter().enumerate() {
        if item.struct_eq(elem) {
            last_pos = Some(i);
        }
    }
    match last_pos {
        Some(pos) => Ok(Value::Integer(Integer::from((pos + 1) as i64))),
        None => Ok(Value::Call {
            head: "Missing".to_string(),
            args: vec![],
        }),
    }
}

// ── Replace ──

/// Replace[expr, rule] replaces the first match (top-level only).
pub fn builtin_replace(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "Replace requires 2 or 3 arguments".to_string(),
        ));
    }
    let expr = &args[0];
    let rules = &args[1];

    use crate::eval::rules::apply_rules_value;

    let result = apply_rules_value(expr, rules, env)?;
    Ok(result)
}

// ── MapAll ──

/// MapAll[f, expr] applies f to every subexpression, mapping children first.
///
/// For a call `h[a, b]` this yields `f[h[f[a], f[b]]]`. The head of a call is
/// not itself mapped (only the arguments are). A top-level list is mapped
/// element-wise without an additional wrapping application, so
/// `MapAll[Head, {1, 2}]` gives `{Integer, Integer}`.
pub fn builtin_map_all(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MapAll requires exactly 2 arguments".to_string(),
        ));
    }
    let f = &args[0];
    let expr = &args[1];
    let wrap_top = !matches!(expr, Value::List(_));
    map_all_inner(f, expr, env, wrap_top)
}

/// Recursively map children first, then (when `wrap`) apply `f` to the rebuilt
/// node. Atoms always have `f` applied.
fn map_all_inner(f: &Value, val: &Value, env: &Env, wrap: bool) -> Result<Value, EvalError> {
    let rebuilt = match val {
        Value::List(items) => {
            let mapped: Result<Vec<Value>, EvalError> = items
                .iter()
                .map(|item| map_all_inner(f, item, env, true))
                .collect();
            Value::List(mapped?)
        }
        Value::Call { head, args } => {
            let mapped: Result<Vec<Value>, EvalError> = args
                .iter()
                .map(|arg| map_all_inner(f, arg, env, true))
                .collect();
            Value::Call {
                head: head.clone(),
                args: mapped?,
            }
        }
        atom => return apply_function(f, &[atom.clone()], env),
    };
    if wrap {
        apply_function(f, &[rebuilt], env)
    } else {
        Ok(rebuilt)
    }
}

// ── Undulate (flatten at all levels) ──

/// Undulate[list] flattens nested lists at all levels.
pub fn builtin_undulate(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Undulate requires exactly 1 argument".to_string(),
        ));
    }
    fn flatten_all(v: &Value) -> Vec<Value> {
        match v {
            Value::List(items) => {
                let mut result = Vec::new();
                for item in items {
                    result.extend(flatten_all(item));
                }
                result
            }
            _ => vec![v.clone()],
        }
    }
    Ok(Value::List(flatten_all(&args[0])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Env;

    fn val_int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn val_list(vec: Vec<Value>) -> Value {
        Value::List(vec)
    }
    fn val_bool(b: bool) -> Value {
        Value::Bool(b)
    }
    fn val_str(s: &str) -> Value {
        Value::Str(s.to_string())
    }

    #[test]
    fn test_subset_q_true() {
        assert_eq!(
            builtin_subset_q(&[
                val_list(vec![val_int(1), val_int(2)]),
                val_list(vec![val_int(1), val_int(2), val_int(3)])
            ])
            .unwrap(),
            val_bool(true)
        );
    }

    #[test]
    fn test_subset_q_false() {
        assert_eq!(
            builtin_subset_q(&[
                val_list(vec![val_int(1), val_int(4)]),
                val_list(vec![val_int(1), val_int(2), val_int(3)])
            ])
            .unwrap(),
            val_bool(false)
        );
    }

    #[test]
    fn test_subset_q_empty() {
        assert_eq!(
            builtin_subset_q(&[val_list(vec![]), val_list(vec![val_int(1)])]).unwrap(),
            val_bool(true)
        );
    }

    #[test]
    fn test_subset_q_single_element() {
        assert_eq!(
            builtin_subset_q(&[
                val_list(vec![val_int(3)]),
                val_list(vec![val_int(1), val_int(2), val_int(3)])
            ])
            .unwrap(),
            val_bool(true)
        );
    }

    #[test]
    fn test_symmetric_difference() {
        let result = builtin_symmetric_difference(&[
            val_list(vec![val_int(1), val_int(2), val_int(3)]),
            val_list(vec![val_int(2), val_int(3), val_int(4)]),
        ])
        .unwrap();
        assert_eq!(result, val_list(vec![val_int(1), val_int(4)]));
    }

    #[test]
    fn test_symmetric_difference_disjoint() {
        let result = builtin_symmetric_difference(&[
            val_list(vec![val_int(1), val_int(2)]),
            val_list(vec![val_int(3), val_int(4)]),
        ])
        .unwrap();
        assert_eq!(
            result,
            val_list(vec![val_int(1), val_int(2), val_int(3), val_int(4)])
        );
    }

    #[test]
    fn test_symmetric_difference_identical() {
        let result = builtin_symmetric_difference(&[
            val_list(vec![val_int(1), val_int(2)]),
            val_list(vec![val_int(1), val_int(2)]),
        ])
        .unwrap();
        assert_eq!(result, val_list(vec![]));
    }

    #[test]
    fn test_symmetric_difference_empty() {
        let result =
            builtin_symmetric_difference(&[val_list(vec![]), val_list(vec![val_int(1)])]).unwrap();
        assert_eq!(result, val_list(vec![val_int(1)]));
    }

    #[test]
    fn test_position_first() {
        assert_eq!(
            builtin_position_first(
                &[
                    val_list(vec![val_int(1), val_int(2), val_int(3), val_int(2)]),
                    val_int(2)
                ],
                &Env::new()
            )
            .unwrap(),
            val_int(2)
        );
    }

    #[test]
    fn test_position_first_not_found() {
        let result = builtin_position_first(
            &[val_list(vec![val_int(1), val_int(2)]), val_int(9)],
            &Env::new(),
        )
        .unwrap();
        assert_eq!(
            result,
            Value::Call {
                head: "Missing".to_string(),
                args: vec![]
            }
        );
    }

    #[test]
    fn test_position_last() {
        assert_eq!(
            builtin_position_last(
                &[
                    val_list(vec![val_int(1), val_int(2), val_int(3), val_int(2)]),
                    val_int(2)
                ],
                &Env::new()
            )
            .unwrap(),
            val_int(4)
        );
    }

    #[test]
    fn test_position_last_not_found() {
        let result = builtin_position_last(
            &[val_list(vec![val_int(1), val_int(2)]), val_int(9)],
            &Env::new(),
        )
        .unwrap();
        assert_eq!(
            result,
            Value::Call {
                head: "Missing".to_string(),
                args: vec![]
            }
        );
    }

    #[test]
    fn test_undule_basic() {
        let result =
            builtin_undulate(&[val_list(vec![val_int(1), val_int(2), val_int(3)])]).unwrap();
        assert_eq!(result, val_list(vec![val_int(1), val_int(2), val_int(3)]));
    }

    #[test]
    fn test_undule_nested() {
        let result = builtin_undulate(&[val_list(vec![
            val_int(1),
            val_list(vec![val_int(2), val_list(vec![val_int(3)])]),
            val_int(4),
        ])])
        .unwrap();
        assert_eq!(
            result,
            val_list(vec![val_int(1), val_int(2), val_int(3), val_int(4)])
        );
    }

    #[test]
    fn test_undule_deeply_nested() {
        let result = builtin_undulate(&[val_list(vec![val_list(vec![val_list(vec![val_list(
            vec![val_int(1)],
        )])])])])
        .unwrap();
        assert_eq!(result, val_list(vec![val_int(1)]));
    }

    #[test]
    fn test_undule_empty() {
        let result = builtin_undulate(&[val_list(vec![])]).unwrap();
        assert_eq!(result, val_list(vec![]));
    }

    #[test]
    fn test_undule_with_strings() {
        let result =
            builtin_undulate(&[val_list(vec![val_str("a"), val_list(vec![val_str("b")])])])
                .unwrap();
        assert_eq!(result, val_list(vec![val_str("a"), val_str("b")]));
    }

    fn test_env() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    #[test]
    fn test_select_first() {
        let env = test_env();
        let even_crit = Value::PureFunction {
            body: Expr::Call {
                head: Box::new(Expr::Symbol("EvenQ".to_string())),
                args: vec![Expr::Slot(None)],
            },
            slot_count: 1,
            params: vec![],
            env: None,
        };
        let result = builtin_select_first(
            &[
                val_list(vec![val_int(1), val_int(3), val_int(6), val_int(8)]),
                even_crit,
            ],
            &env,
        )
        .unwrap();
        assert_eq!(result, val_int(6));
    }

    #[test]
    fn test_select_first_none() {
        let env = test_env();
        let gt_crit = Value::PureFunction {
            body: Expr::Call {
                head: Box::new(Expr::Symbol("Greater".to_string())),
                args: vec![Expr::Slot(None), Expr::Integer(Integer::from(10))],
            },
            slot_count: 1,
            params: vec![],
            env: None,
        };
        let result = builtin_select_first(
            &[val_list(vec![val_int(1), val_int(2), val_int(5)]), gt_crit],
            &env,
        )
        .unwrap();
        assert_eq!(
            result,
            Value::Call {
                head: "Missing".to_string(),
                args: vec![]
            }
        );
    }

    #[test]
    fn test_select_last() {
        let env = test_env();
        let even_crit = Value::PureFunction {
            body: Expr::Call {
                head: Box::new(Expr::Symbol("EvenQ".to_string())),
                args: vec![Expr::Slot(None)],
            },
            slot_count: 1,
            params: vec![],
            env: None,
        };
        let result = builtin_select_last(
            &[
                val_list(vec![val_int(2), val_int(3), val_int(6), val_int(5)]),
                even_crit,
            ],
            &env,
        )
        .unwrap();
        assert_eq!(result, val_int(6));
    }

    #[test]
    fn test_select_last_none() {
        let env = test_env();
        let big_crit = Value::PureFunction {
            body: Expr::Call {
                head: Box::new(Expr::Symbol("Greater".to_string())),
                args: vec![Expr::Slot(None), Expr::Integer(Integer::from(100))],
            },
            slot_count: 1,
            params: vec![],
            env: None,
        };
        let result =
            builtin_select_last(&[val_list(vec![val_int(1), val_int(2)]), big_crit], &env).unwrap();
        assert_eq!(
            result,
            Value::Call {
                head: "Missing".to_string(),
                args: vec![]
            }
        );
    }

    #[test]
    fn test_curry_zero() {
        let env = test_env();
        let result = builtin_curry(&[Value::Symbol("Plus".to_string()), val_int(0)], &env).unwrap();
        assert_eq!(result, Value::Symbol("Plus".to_string()));
    }

    #[test]
    fn test_curry_returns_pure_function() {
        let env = test_env();
        let result = builtin_curry(&[Value::Symbol("Plus".to_string()), val_int(2)], &env).unwrap();
        assert!(matches!(result, Value::PureFunction { .. }));
    }

    #[test]
    fn test_composition_empty() {
        let env = test_env();
        let result = builtin_composition(&[], &env).unwrap();
        assert_eq!(result, Value::Symbol("Identity".to_string()));
    }

    #[test]
    fn test_composition_single() {
        let env = test_env();
        let result = builtin_composition(&[Value::Symbol("Sin".to_string())], &env).unwrap();
        assert_eq!(result, Value::Symbol("Sin".to_string()));
    }

    #[test]
    fn test_composition_returns_pure_function() {
        let env = test_env();
        let result = builtin_composition(
            &[
                Value::Symbol("Sin".to_string()),
                Value::Symbol("Cos".to_string()),
            ],
            &env,
        )
        .unwrap();
        assert!(matches!(result, Value::PureFunction { .. }));
    }

    #[test]
    fn test_right_composition_empty() {
        let env = test_env();
        let result = builtin_right_composition(&[], &env).unwrap();
        assert_eq!(result, Value::Symbol("Identity".to_string()));
    }

    #[test]
    fn test_right_composition_single() {
        let env = test_env();
        let result = builtin_right_composition(&[Value::Symbol("Sin".to_string())], &env).unwrap();
        assert_eq!(result, Value::Symbol("Sin".to_string()));
    }

    #[test]
    fn test_right_composition_returns_pure_function() {
        let env = test_env();
        let result = builtin_right_composition(
            &[
                Value::Symbol("Sin".to_string()),
                Value::Symbol("Cos".to_string()),
            ],
            &env,
        )
        .unwrap();
        assert!(matches!(result, Value::PureFunction { .. }));
    }

    #[test]
    fn test_through_basic() {
        let env = test_env();
        let expr = Value::Call {
            head: "Abs".to_string(),
            args: vec![val_int(-5)],
        };
        let result = builtin_through(&[expr], &env).unwrap();
        assert_eq!(result, val_int(5));
    }

    #[test]
    fn test_through_multi() {
        let env = test_env();
        let expr = Value::Call {
            head: "Abs".to_string(),
            args: vec![val_int(-3)],
        };
        let result = builtin_through(&[expr], &env).unwrap();
        assert_eq!(result, val_int(3));
    }

    #[test]
    fn test_through_wrong_args() {
        let env = test_env();
        assert!(builtin_through(&[], &env).is_err());
    }

    fn val_func(name: &str) -> Value {
        Value::Symbol(name.to_string())
    }

    #[test]
    fn test_operator_apply_basic() {
        let env = test_env();
        let expr = val_list(vec![val_func("Plus"), val_int(1), val_int(2), val_int(3)]);
        let result = builtin_operator_apply(&[expr], &env).unwrap();
        assert_eq!(result, val_int(6));
    }

    #[test]
    fn test_operator_apply_single_arg() {
        let env = test_env();
        let expr = val_list(vec![val_func("Abs"), val_int(-5)]);
        let result = builtin_operator_apply(&[expr], &env).unwrap();
        assert_eq!(result, val_int(5));
    }

    #[test]
    fn test_uncurry_returns_pure_function() {
        let env = test_env();
        let result = builtin_uncurry(&[Value::Symbol("f".to_string())], &env).unwrap();
        assert!(matches!(result, Value::PureFunction { .. }));
    }

    #[test]
    fn test_replace_basic() {
        let env = test_env();
        let rule = Value::Rule {
            lhs: Box::new(Value::Pattern(Expr::Symbol("x".to_string()))),
            rhs: Box::new(Value::Pattern(Expr::Symbol("y".to_string()))),
            delayed: false,
        };
        let result = builtin_replace(&[Value::Symbol("x".to_string()), rule], &env).unwrap();
        assert_eq!(result, Value::Symbol("y".to_string()));
    }

    #[test]
    fn test_replace_no_match() {
        let env = test_env();
        let rule = Value::Rule {
            lhs: Box::new(Value::Pattern(Expr::Symbol("z".to_string()))),
            rhs: Box::new(Value::Pattern(Expr::Symbol("y".to_string()))),
            delayed: false,
        };
        let result = builtin_replace(&[Value::Symbol("x".to_string()), rule], &env).unwrap();
        assert_eq!(result, Value::Symbol("x".to_string()));
    }

    #[test]
    fn test_map_all_basic() {
        let env = test_env();
        let head_func = Value::Symbol("Head".to_string());
        let result =
            builtin_map_all(&[head_func, val_list(vec![val_int(1), val_int(2)])], &env).unwrap();
        assert!(matches!(result, Value::List(_)));
    }

    #[test]
    fn test_map_all_identity() {
        let env = test_env();
        let result = builtin_map_all(
            &[val_func("Identity"), val_list(vec![val_int(1), val_int(2)])],
            &env,
        )
        .unwrap();
        assert_eq!(result, val_list(vec![val_int(1), val_int(2)]));
    }

    #[test]
    fn test_composition_three_functions() {
        let env = test_env();
        let result = builtin_composition(
            &[
                Value::Symbol("f".to_string()),
                Value::Symbol("g".to_string()),
                Value::Symbol("h".to_string()),
            ],
            &env,
        )
        .unwrap();
        assert!(matches!(result, Value::PureFunction { .. }));
    }

    #[test]
    fn test_curry_three_args() {
        let env = test_env();
        let result = builtin_curry(&[Value::Symbol("Plus".to_string()), val_int(3)], &env).unwrap();
        assert!(matches!(result, Value::PureFunction { .. }));
    }

    #[test]
    fn test_identity_builtin() {
        assert_eq!(builtin_identity(&[val_int(7)]).unwrap(), val_int(7));
        assert_eq!(builtin_identity(&[]).unwrap(), Value::Null);
    }

    #[test]
    fn test_composition_nests_right_to_left() {
        let env = test_env();
        // Composition[f, g][x] = f[g[x]]
        let f = builtin_composition(&[val_func("f"), val_func("g")], &env).unwrap();
        let result = apply_function(&f, &[Value::Symbol("x".to_string())], &env).unwrap();
        assert_eq!(
            result,
            Value::Call {
                head: "f".to_string(),
                args: vec![Value::Call {
                    head: "g".to_string(),
                    args: vec![Value::Symbol("x".to_string())],
                }],
            }
        );
    }

    #[test]
    fn test_right_composition_nests_left_to_right() {
        let env = test_env();
        // RightComposition[f, g][x] = g[f[x]]
        let f = builtin_right_composition(&[val_func("f"), val_func("g")], &env).unwrap();
        let result = apply_function(&f, &[Value::Symbol("x".to_string())], &env).unwrap();
        assert_eq!(
            result,
            Value::Call {
                head: "g".to_string(),
                args: vec![Value::Call {
                    head: "f".to_string(),
                    args: vec![Value::Symbol("x".to_string())],
                }],
            }
        );
    }

    #[test]
    fn test_composition_applies_end_to_end() {
        let env = test_env();
        // Composition[Abs, Sign][-3] = Abs[Sign[-3]] = Abs[-1] = 1
        let f = builtin_composition(&[val_func("Abs"), val_func("Sign")], &env).unwrap();
        let result = apply_function(&f, &[val_int(-3)], &env).unwrap();
        assert_eq!(result, val_int(1));
    }

    #[test]
    fn test_curry_accumulates_args() {
        let env = test_env();
        let curried = builtin_curry(&[val_func("Plus"), val_int(2)], &env).unwrap();
        let step1 = apply_function(&curried, &[val_int(3)], &env).unwrap();
        let step2 = apply_function(&step1, &[val_int(4)], &env).unwrap();
        assert_eq!(step2, val_int(7));
    }

    #[test]
    fn test_curry_accumulates_three_args() {
        let env = test_env();
        let curried = builtin_curry(&[val_func("Times"), val_int(3)], &env).unwrap();
        let s1 = apply_function(&curried, &[val_int(2)], &env).unwrap();
        let s2 = apply_function(&s1, &[val_int(3)], &env).unwrap();
        let s3 = apply_function(&s2, &[val_int(4)], &env).unwrap();
        assert_eq!(s3, val_int(24));
    }

    #[test]
    fn test_uncurry_applies_curried_function() {
        let env = test_env();
        let curried = builtin_curry(&[val_func("Plus"), val_int(3)], &env).unwrap();
        let uncurried = builtin_uncurry(&[curried], &env).unwrap();
        let result = apply_function(
            &uncurried,
            &[val_list(vec![val_int(1), val_int(2), val_int(3)])],
            &env,
        )
        .unwrap();
        assert_eq!(result, val_int(6));
    }

    #[test]
    fn test_through_list_head() {
        let env = test_env();
        let expr = Value::Call {
            head: "{Abs, Sign}".to_string(),
            args: vec![val_int(-3)],
        };
        let result = builtin_through(&[expr], &env).unwrap();
        assert_eq!(result, val_list(vec![val_int(3), val_int(-1)]));
    }

    #[test]
    fn test_through_rendered_builtin_head() {
        let env = test_env();
        // This is the head string a `{Abs, Sign}[...]` call actually carries.
        let expr = Value::Call {
            head: "{Builtin[Abs], Builtin[Sign]}".to_string(),
            args: vec![val_int(-4)],
        };
        let result = builtin_through(&[expr], &env).unwrap();
        assert_eq!(result, val_list(vec![val_int(4), val_int(-1)]));
    }

    #[test]
    fn test_map_all_wraps_call_children_first() {
        let env = test_env();
        let expr = Value::Call {
            head: "h".to_string(),
            args: vec![
                Value::Symbol("a".to_string()),
                Value::Symbol("b".to_string()),
            ],
        };
        let result = builtin_map_all(&[val_func("f"), expr], &env).unwrap();
        // f[h[f[a], f[b]]]
        let fcall = |s: &str| Value::Call {
            head: "f".to_string(),
            args: vec![Value::Symbol(s.to_string())],
        };
        assert_eq!(
            result,
            Value::Call {
                head: "f".to_string(),
                args: vec![Value::Call {
                    head: "h".to_string(),
                    args: vec![fcall("a"), fcall("b")],
                }],
            }
        );
    }

    #[test]
    fn test_map_all_does_not_wrap_head() {
        let env = test_env();
        let expr = Value::Call {
            head: "g".to_string(),
            args: vec![Value::Symbol("a".to_string())],
        };
        let result = builtin_map_all(&[val_func("f"), expr], &env).unwrap();
        // f[g[f[a]]] — head `g` is not itself mapped.
        assert_eq!(
            result,
            Value::Call {
                head: "f".to_string(),
                args: vec![Value::Call {
                    head: "g".to_string(),
                    args: vec![Value::Call {
                        head: "f".to_string(),
                        args: vec![Value::Symbol("a".to_string())],
                    }],
                }],
            }
        );
    }

    #[test]
    fn test_map_all_atom() {
        let env = test_env();
        let result =
            builtin_map_all(&[val_func("f"), Value::Symbol("a".to_string())], &env).unwrap();
        assert_eq!(
            result,
            Value::Call {
                head: "f".to_string(),
                args: vec![Value::Symbol("a".to_string())],
            }
        );
    }
}
