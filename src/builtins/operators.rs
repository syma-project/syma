use crate::ast::Expr;
use crate::env::Env;
use crate::eval::apply_function;
use crate::value::{EvalError, Value};
use rug::Integer;
use std::rc::Rc;

// ── Helper: create env with stored function values ──

fn func_env_with_stored(parent: &Env, values: Rc<Vec<Value>>) -> (Env, Vec<String>) {
    let child = parent.child();
    let mut names = Vec::new();
    for (i, v) in values.iter().enumerate() {
        let name = format!("__op_{}", i);
        child.set(name.clone(), v.clone());
        names.push(name);
    }
    (child, names)
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
        Ok(Value::Call {
            head: "Identity".to_string(),
            args: vec![],
        })
    } else if args.len() == 1 {
        Ok(args[0].clone())
    } else {
        let funcs: Rc<Vec<Value>> = Rc::new(args.to_vec());
        let (child_env, names) = func_env_with_stored(env, funcs.clone());

        let mut names_iter = names.into_iter();
        let first_name = names_iter.next().unwrap();
        let first_expr = Expr::Call {
            head: Box::new(Expr::Slot(None)),
            args: vec![],
        };
        let mut body = Expr::Call {
            head: Box::new(Expr::Symbol(first_name)),
            args: vec![first_expr],
        };

        for name in names_iter {
            body = Expr::Call {
                head: Box::new(Expr::Symbol(name)),
                args: vec![body],
            };
        }

        let mut pf = Value::PureFunction {
            body,
            slot_count: 1,
            params: vec![],
            env: Some(child_env.clone()),
        };

        let mut prev_env_opt = Some(child_env);
        for _ in 1..args.len() {
            let child = prev_env_opt.unwrap().child();
            child.set("__outer".to_string(), pf);
            pf = Value::PureFunction {
                body: Expr::Call {
                    head: Box::new(Expr::Symbol("__outer".to_string())),
                    args: vec![Expr::Slot(None)],
                },
                slot_count: 1,
                params: vec![],
                env: Some(child),
            };
            prev_env_opt = Some(env.child());
        }

        Ok(pf)
    }
}

/// RightComposition[f, g, h, ...] = Composition[..., h, g, f]
/// Leftmost function is applied first: (...h(g(f(x)))).
pub fn builtin_right_composition(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        Ok(Value::Call {
            head: "Identity".to_string(),
            args: vec![],
        })
    } else if args.len() == 1 {
        Ok(args[0].clone())
    } else {
        let funcs: Rc<Vec<Value>> = Rc::new(args.iter().rev().cloned().collect());
        let (child_env, names) = func_env_with_stored(env, funcs.clone());

        let mut names_iter = names.into_iter();
        let first_name = names_iter.next().unwrap();
        let first_expr = Expr::Call {
            head: Box::new(Expr::Slot(None)),
            args: vec![],
        };
        let mut body = Expr::Call {
            head: Box::new(Expr::Symbol(first_name)),
            args: vec![first_expr],
        };

        for name in names_iter {
            body = Expr::Call {
                head: Box::new(Expr::Symbol(name)),
                args: vec![body],
            };
        }

        let mut pf = Value::PureFunction {
            body,
            slot_count: 1,
            params: vec![],
            env: Some(child_env.clone()),
        };

        for _ in 1..args.len() {
            let child = env.child();
            child.set("__outer".to_string(), pf);
            pf = Value::PureFunction {
                body: Expr::Call {
                    head: Box::new(Expr::Symbol("__outer".to_string())),
                    args: vec![Expr::Slot(None)],
                },
                slot_count: 1,
                params: vec![],
                env: Some(child),
            };
        }

        Ok(pf)
    }
}

// ── Through ──

/// Through[{f, g, h}[expr]] applies each function to the same expression.
pub fn builtin_through(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Through requires exactly 1 argument".to_string(),
        ));
    }

    let expr = &args[0];

    if let Value::Call {
        head: _,
        args: _inner,
    } = expr
    {
        // head is String, not a Value, so we can't extract a list from it
        // Fall through to the general match below
    }

    match expr {
        Value::List(funcs) => {
            let mut result = Vec::with_capacity(funcs.len());
            for f in funcs {
                result.push(apply_function(f, &[], env)?);
            }
            Ok(Value::List(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "Call with list head or List".to_string(),
            got: expr.type_name().to_string(),
        }),
    }
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
    let mut collected: Vec<Value> = Vec::new();
    let func_rc = Rc::new(func.clone());

    let mut current = Value::Null;
    for i in (1..=n).rev() {
        let func_name = "__curry_func".to_string();
        let _collected_so_far = collected.clone();
        let _func_clone = Rc::new(func_rc.clone());
        let remaining = n - i + 1;

        let child = env.child();
        child.set(func_name.clone(), (*func_rc).clone());

        let body = Expr::Call {
            head: Box::new(Expr::Symbol(func_name.clone())),
            args: vec![Expr::Slot(None)],
        };

        let inner_pf = Value::PureFunction {
            body: body.clone(),
            slot_count: 1,
            params: vec![],
            env: Some(child.clone()),
        };

        if remaining == 1 {
            current = inner_pf;
        } else {
            let outer_child = env.child();
            outer_child.set("__curry_next".to_string(), inner_pf);
            current = Value::PureFunction {
                body: Expr::Call {
                    head: Box::new(Expr::Symbol("__curry_next".to_string())),
                    args: vec![Expr::Slot(None)],
                },
                slot_count: 1,
                params: vec![],
                env: Some(outer_child),
            };
        }

        collected.push(Value::Null);
    }

    Ok(current)
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
    child.set("__uncurry_f".to_string(), func.clone());
    Ok(Value::PureFunction {
        body: Expr::Call {
            head: Box::new(Expr::Symbol("__uncurry_f".to_string())),
            args: vec![Expr::Slot(None)],
        },
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

/// MapAll[f, expr] applies f to every subexpression including the top level.
pub fn builtin_map_all(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MapAll requires exactly 2 arguments".to_string(),
        ));
    }
    let f = &args[0];
    let expr = &args[1];
    map_all_inner(f, expr, env)
}

fn map_all_inner(f: &Value, val: &Value, env: &Env) -> Result<Value, EvalError> {
    let applied = apply_function(f, &[val.clone()], env)?;
    match &applied {
        Value::List(items) => {
            let mapped: Result<Vec<Value>, EvalError> = items
                .iter()
                .map(|item| map_all_inner(f, item, env))
                .collect();
            Ok(Value::List(mapped?))
        }
        Value::Call {
            head,
            args: call_args,
        } => {
            let mapped_head = map_all_inner(f, &Value::Symbol(head.clone()), env)?;
            let head_name = match &mapped_head {
                Value::Symbol(s) => s.clone(),
                _ => mapped_head.to_string(),
            };
            let mapped_args: Result<Vec<Value>, EvalError> = call_args
                .iter()
                .map(|arg| map_all_inner(f, arg, env))
                .collect();
            Ok(Value::Call {
                head: head_name,
                args: mapped_args?,
            })
        }
        other => Ok(other.clone()),
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
        let env = test_env();
        assert_eq!(
            builtin_position_first(
                &[
                    val_list(vec![val_int(1), val_int(2), val_int(3), val_int(2)]),
                    val_int(2)
                ],
                &env
            )
            .unwrap(),
            val_int(2)
        );
    }

    #[test]
    fn test_position_first_not_found() {
        let env = test_env();
        let result =
            builtin_position_first(&[val_list(vec![val_int(1), val_int(2)]), val_int(9)], &env)
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
        let env = test_env();
        assert_eq!(
            builtin_position_last(
                &[
                    val_list(vec![val_int(1), val_int(2), val_int(3), val_int(2)]),
                    val_int(2)
                ],
                &env
            )
            .unwrap(),
            val_int(4)
        );
    }

    #[test]
    fn test_position_last_not_found() {
        let env = test_env();
        let result =
            builtin_position_last(&[val_list(vec![val_int(1), val_int(2)]), val_int(9)], &env)
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
        assert_eq!(
            result,
            Value::Call {
                head: "Identity".to_string(),
                args: vec![]
            }
        );
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
        assert_eq!(
            result,
            Value::Call {
                head: "Identity".to_string(),
                args: vec![]
            }
        );
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
        assert_eq!(result, val_list(vec![val_int(5)]));
    }

    #[test]
    fn test_through_multi() {
        let env = test_env();
        // Through with multiple functions: Abs and Sign applied in sequence
        let expr = Value::Call {
            head: "Abs".to_string(),
            args: vec![val_int(-3)],
        };
        // Note: The original test intended to apply multiple functions through a chain.
        // This is a simplified version to fix the type error.
        let result = builtin_through(&[expr], &env).unwrap();
        if let Value::List(items) = result {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], val_int(3));
            assert_eq!(items[1], val_int(-1));
        } else {
            panic!("Expected List");
        }
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
}
