use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::Integer;

pub fn register(env: &Env) {
    // Domain symbols
    for sym in &[
        "Reals",
        "Integers",
        "Rationals",
        "Complexes",
        "Booleans",
        "Primes",
    ] {
        env.set(sym.to_string(), Value::Symbol(sym.to_string()));
    }
    env.set(
        "Element".to_string(),
        Value::Builtin(
            "Element".to_string(),
            crate::value::BuiltinFn::Pure(builtin_element),
        ),
    );
    env.set(
        "Refine".to_string(),
        Value::Builtin(
            "Refine".to_string(),
            crate::value::BuiltinFn::Env(builtin_refine),
        ),
    );
    // $Assumptions default value
    env.set("$Assumptions".to_string(), Value::List(vec![]));
}

// ── Element ──

fn builtin_element(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Element requires exactly 2 arguments".to_string(),
        ));
    }
    let x = &args[0];
    let dom = &args[1];

    let dom_name = match dom {
        Value::Symbol(s) => s.as_str(),
        _ => {
            return Ok(Value::Call {
                head: "Element".to_string(),
                args: args.to_vec(),
            });
        }
    };

    fn is_concrete(v: &Value) -> bool {
        matches!(
            v,
            Value::Integer(_) | Value::Rational(_) | Value::Real(_) | Value::Complex { .. }
        )
    }

    let result = match dom_name {
        "Integers" if is_concrete(x) => Value::Symbol(if matches!(x, Value::Integer(_)) {
            "True".to_string()
        } else {
            "False".to_string()
        }),
        "Rationals" if is_concrete(x) => {
            Value::Symbol(if matches!(x, Value::Integer(_) | Value::Rational(_)) {
                "True".to_string()
            } else {
                "False".to_string()
            })
        }
        "Reals" if is_concrete(x) => Value::Symbol(
            if matches!(x, Value::Integer(_) | Value::Rational(_) | Value::Real(_)) {
                "True".to_string()
            } else {
                "False".to_string()
            },
        ),
        "Complexes" if is_concrete(x) => Value::Symbol("True".to_string()),
        "Booleans" => match x {
            Value::Symbol(s) if s == "True" || s == "False" => Value::Symbol("True".to_string()),
            Value::Bool(_) => Value::Symbol("True".to_string()),
            _ => {
                if is_concrete(x) {
                    Value::Symbol("False".to_string())
                } else {
                    return Ok(Value::Call {
                        head: "Element".to_string(),
                        args: args.to_vec(),
                    });
                }
            }
        },
        "Primes" => {
            if let Value::Integer(n) = x {
                Value::Symbol(if n.is_positive() && prime_q(n) {
                    "True".to_string()
                } else {
                    "False".to_string()
                })
            } else if is_concrete(x) {
                Value::Symbol("False".to_string())
            } else {
                return Ok(Value::Call {
                    head: "Element".to_string(),
                    args: args.to_vec(),
                });
            }
        }
        _ => {
            return Ok(Value::Call {
                head: "Element".to_string(),
                args: args.to_vec(),
            });
        }
    };
    Ok(result)
}

fn prime_q(n: &Integer) -> bool {
    // Use u64 Miller-Rabin for small, trial division for large
    if let Some(n64) = n.to_u64() {
        crate::builtins::number_theory::is_prime_u64(n64)
    } else {
        trial_prime_check(n)
    }
}

fn trial_prime_check(n: &Integer) -> bool {
    if n < &Integer::from(2) {
        return false;
    }
    let mut d = Integer::from(2);
    let limit = Integer::from(n.clone().sqrt_ref());
    while d <= limit {
        if (n.clone() % &d).is_zero() {
            return false;
        }
        d += 1;
    }
    true
}

// ── Refine ──

fn builtin_refine(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    match args.len() {
        1 => {
            let assum = env.get("$Assumptions").unwrap_or(Value::List(vec![]));
            refine_expr(&args[0], &assum)
        }
        2 => refine_expr(&args[0], &args[1]),
        _ => Err(EvalError::Error(
            "Refine requires 1 or 2 arguments".to_string(),
        )),
    }
}

fn refine_expr(val: &Value, assum: &Value) -> Result<Value, EvalError> {
    match val {
        // Sqrt[x_^2] with x>0 → x
        // Sqrt[x_^2] with Element[x, Reals] → Abs[x]
        Value::Call { head, args } if head == "Sqrt" && args.len() == 1 => {
            if let Value::Call {
                head: ph,
                args: pargs,
            } = &args[0]
                && ph == "Power"
                && pargs.len() == 2
                && let Value::Integer(e) = &pargs[1]
                && *e == 2
            {
                let var = &pargs[0];
                if implies_positive(var, assum) {
                    return Ok(var.clone());
                }
                if implies_non_negative(var, assum) {
                    return Ok(var.clone());
                }
                if implies_real(var, assum) {
                    return Ok(Value::Call {
                        head: "Abs".to_string(),
                        args: vec![var.clone()],
                    });
                }
            }
            // Recurse into argument
            let refined = refine_expr(&args[0], assum)?;
            if refined != args[0] {
                Ok(Value::Call {
                    head: "Sqrt".to_string(),
                    args: vec![refined],
                })
            } else {
                Ok(val.clone())
            }
        }

        // Abs[x] with x>0 → x
        // Abs[x] with x<0 → -x
        Value::Call { head, args } if head == "Abs" && args.len() == 1 => {
            let var = &args[0];
            if implies_positive(var, assum) || implies_non_negative(var, assum) {
                return Ok(var.clone());
            }
            if implies_negative(var, assum) {
                return Ok(Value::Call {
                    head: "Times".to_string(),
                    args: vec![Value::Integer(Integer::from(-1)), var.clone()],
                });
            }
            let refined = refine_expr(&args[0], assum)?;
            if refined != args[0] {
                Ok(Value::Call {
                    head: "Abs".to_string(),
                    args: vec![refined],
                })
            } else {
                Ok(val.clone())
            }
        }

        // Conjugate[x] with Element[x, Reals] → x
        Value::Call { head, args } if head == "Conjugate" && args.len() == 1 => {
            let var = &args[0];
            if implies_real(var, assum) {
                return Ok(var.clone());
            }
            let refined = refine_expr(&args[0], assum)?;
            if refined != args[0] {
                Ok(Value::Call {
                    head: "Conjugate".to_string(),
                    args: vec![refined],
                })
            } else {
                Ok(val.clone())
            }
        }

        // Re[x] with Element[x, Reals] → x
        Value::Call { head, args } if head == "Re" && args.len() == 1 => {
            let var = &args[0];
            if implies_real(var, assum) {
                return Ok(var.clone());
            }
            let refined = refine_expr(&args[0], assum)?;
            if refined != args[0] {
                Ok(Value::Call {
                    head: "Re".to_string(),
                    args: vec![refined],
                })
            } else {
                Ok(val.clone())
            }
        }

        // Im[x] with Element[x, Reals] → 0
        Value::Call { head, args } if head == "Im" && args.len() == 1 => {
            let var = &args[0];
            if implies_real(var, assum) {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let refined = refine_expr(&args[0], assum)?;
            if refined != args[0] {
                Ok(Value::Call {
                    head: "Im".to_string(),
                    args: vec![refined],
                })
            } else {
                Ok(val.clone())
            }
        }

        // Sign[x] with x>0 → 1, x<0 → -1
        Value::Call { head, args } if head == "Sign" && args.len() == 1 => {
            let var = &args[0];
            if implies_positive(var, assum) {
                return Ok(Value::Integer(Integer::from(1)));
            }
            if implies_negative(var, assum) {
                return Ok(Value::Integer(Integer::from(-1)));
            }
            let refined = refine_expr(&args[0], assum)?;
            if refined != args[0] {
                Ok(Value::Call {
                    head: "Sign".to_string(),
                    args: vec![refined],
                })
            } else {
                Ok(val.clone())
            }
        }

        // Lists: refine each element
        Value::List(items) => {
            let refined: Result<Vec<Value>, EvalError> =
                items.iter().map(|v| refine_expr(v, assum)).collect();
            Ok(Value::List(refined?))
        }

        // Other calls: refine all arguments
        Value::Call { head, args } => {
            let mut changed = false;
            let refined_args: Result<Vec<Value>, EvalError> = args
                .iter()
                .map(|a| {
                    let r = refine_expr(a, assum)?;
                    if &r != a {
                        changed = true;
                    }
                    Ok(r)
                })
                .collect();
            let refined_args = refined_args?;
            if changed {
                Ok(Value::Call {
                    head: head.clone(),
                    args: refined_args,
                })
            } else {
                Ok(val.clone())
            }
        }

        _ => Ok(val.clone()),
    }
}

// ── Assumption extraction helpers ──

fn extract_conditions(assum: &Value) -> Vec<&Value> {
    match assum {
        Value::Call { head, args } if head == "And" => {
            args.iter().flat_map(|a| extract_conditions(a)).collect()
        }
        Value::List(items) => items.iter().flat_map(extract_conditions).collect(),
        Value::Symbol(s) if s == "True" => vec![],
        _ => vec![assum],
    }
}

fn is_var(v: &Value, target: &Value) -> bool {
    match (v, target) {
        (Value::Symbol(a), Value::Symbol(b)) => a == b,
        _ => v == target,
    }
}

fn condition_implies_positive(var: &Value, cond: &Value) -> bool {
    match cond {
        // x > 0  or  0 < x
        Value::Call { head, args } if head == "Greater" && args.len() == 2 => {
            is_var(&args[0], var) && matches!(&args[1], Value::Integer(n) if n.is_zero())
                || is_var(&args[1], var) && matches!(&args[0], Value::Integer(n) if n.is_zero())
        }
        _ => false,
    }
}

fn condition_implies_non_negative(var: &Value, cond: &Value) -> bool {
    match cond {
        // x >= 0  or 0 <= x
        Value::Call { head, args } if head == "GreaterEqual" && args.len() == 2 => {
            is_var(&args[0], var) && matches!(&args[1], Value::Integer(n) if n.is_zero())
                || is_var(&args[1], var) && matches!(&args[0], Value::Integer(n) if n.is_zero())
        }
        _ => condition_implies_positive(var, cond),
    }
}

fn condition_implies_negative(var: &Value, cond: &Value) -> bool {
    match cond {
        // x < 0  or  0 > x
        Value::Call { head, args } if head == "Less" && args.len() == 2 => {
            is_var(&args[0], var) && matches!(&args[1], Value::Integer(n) if n.is_zero())
                || is_var(&args[1], var) && matches!(&args[0], Value::Integer(n) if n.is_zero())
        }
        _ => false,
    }
}

fn condition_implies_real(var: &Value, cond: &Value) -> bool {
    match cond {
        Value::Call { head, args } if head == "Element" && args.len() == 2 => {
            is_var(&args[0], var)
                && matches!(
                    &args[1],
                    Value::Symbol(s) if s == "Reals" || s == "Integers" || s == "Rationals"
                )
        }
        _ => false,
    }
}

fn implies_positive(var: &Value, assum: &Value) -> bool {
    extract_conditions(assum)
        .iter()
        .any(|c| condition_implies_positive(var, c))
}

fn implies_non_negative(var: &Value, assum: &Value) -> bool {
    extract_conditions(assum)
        .iter()
        .any(|c| condition_implies_non_negative(var, c))
}

fn implies_negative(var: &Value, assum: &Value) -> bool {
    extract_conditions(assum)
        .iter()
        .any(|c| condition_implies_negative(var, c))
}

fn implies_real(var: &Value, assum: &Value) -> bool {
    extract_conditions(assum)
        .iter()
        .any(|c| condition_implies_real(var, c))
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;
    use rug::Integer;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn sym(s: &str) -> Value {
        Value::Symbol(s.to_string())
    }
    fn real(f: f64) -> Value {
        Value::Real(rug::Float::with_val(crate::value::DEFAULT_PRECISION, f))
    }

    #[test]
    fn test_element_integers() {
        assert_eq!(
            builtin_element(&[int(3), sym("Integers")]).unwrap(),
            sym("True")
        );
        assert_eq!(
            builtin_element(&[real(3.0), sym("Integers")]).unwrap(),
            sym("False")
        );
        assert_eq!(
            builtin_element(&[sym("x"), sym("Integers")]).unwrap(),
            Value::Call {
                head: "Element".to_string(),
                args: vec![sym("x"), sym("Integers")]
            }
        );
    }

    #[test]
    fn test_element_reals() {
        assert_eq!(
            builtin_element(&[int(3), sym("Reals")]).unwrap(),
            sym("True")
        );
        assert_eq!(
            builtin_element(&[real(2.5), sym("Reals")]).unwrap(),
            sym("True")
        );
        assert_eq!(
            builtin_element(&[Value::Complex { re: 1.0, im: 1.0 }, sym("Reals")]).unwrap(),
            sym("False")
        );
    }

    #[test]
    fn test_element_complexes() {
        assert_eq!(
            builtin_element(&[int(3), sym("Complexes")]).unwrap(),
            sym("True")
        );
        assert_eq!(
            builtin_element(&[Value::Complex { re: 1.0, im: 1.0 }, sym("Complexes")]).unwrap(),
            sym("True")
        );
    }

    #[test]
    fn test_element_booleans() {
        assert_eq!(
            builtin_element(&[sym("True"), sym("Booleans")]).unwrap(),
            sym("True")
        );
        assert_eq!(
            builtin_element(&[sym("False"), sym("Booleans")]).unwrap(),
            sym("True")
        );
        assert_eq!(
            builtin_element(&[int(0), sym("Booleans")]).unwrap(),
            sym("False")
        );
    }

    #[test]
    fn test_refine_sqrt_x2_positive() {
        // Refine[Sqrt[x^2], x > 0] → x
        let expr = Value::Call {
            head: "Sqrt".to_string(),
            args: vec![Value::Call {
                head: "Power".to_string(),
                args: vec![sym("x"), int(2)],
            }],
        };
        let assum = Value::Call {
            head: "Greater".to_string(),
            args: vec![sym("x"), int(0)],
        };
        let result = builtin_refine(&[expr, assum], &Env::new()).unwrap();
        assert_eq!(result, sym("x"));
    }

    #[test]
    fn test_refine_sqrt_x2_real() {
        // Refine[Sqrt[x^2], Element[x, Reals]] → Abs[x]
        let expr = Value::Call {
            head: "Sqrt".to_string(),
            args: vec![Value::Call {
                head: "Power".to_string(),
                args: vec![sym("x"), int(2)],
            }],
        };
        let assum = Value::Call {
            head: "Element".to_string(),
            args: vec![sym("x"), sym("Reals")],
        };
        let result = builtin_refine(&[expr, assum], &Env::new()).unwrap();
        assert_eq!(
            result,
            Value::Call {
                head: "Abs".to_string(),
                args: vec![sym("x")],
            }
        );
    }

    #[test]
    fn test_refine_abs_positive() {
        // Refine[Abs[x], x > 0] → x
        let expr = Value::Call {
            head: "Abs".to_string(),
            args: vec![sym("x")],
        };
        let assum = Value::Call {
            head: "Greater".to_string(),
            args: vec![sym("x"), int(0)],
        };
        assert_eq!(
            builtin_refine(&[expr, assum], &Env::new()).unwrap(),
            sym("x")
        );
    }

    #[test]
    fn test_refine_abs_negative() {
        // Refine[Abs[x], x < 0] → -x
        let expr = Value::Call {
            head: "Abs".to_string(),
            args: vec![sym("x")],
        };
        let assum = Value::Call {
            head: "Less".to_string(),
            args: vec![sym("x"), int(0)],
        };
        let result = builtin_refine(&[expr, assum], &Env::new()).unwrap();
        assert_eq!(
            result,
            Value::Call {
                head: "Times".to_string(),
                args: vec![int(-1), sym("x")],
            }
        );
    }

    #[test]
    fn test_refine_conjugate_real() {
        // Refine[Conjugate[x], Element[x, Reals]] → x
        let expr = Value::Call {
            head: "Conjugate".to_string(),
            args: vec![sym("x")],
        };
        let assum = Value::Call {
            head: "Element".to_string(),
            args: vec![sym("x"), sym("Reals")],
        };
        assert_eq!(
            builtin_refine(&[expr, assum], &Env::new()).unwrap(),
            sym("x")
        );
    }

    #[test]
    fn test_refine_im_real() {
        // Refine[Im[x], Element[x, Reals]] → 0
        let expr = Value::Call {
            head: "Im".to_string(),
            args: vec![sym("x")],
        };
        let assum = Value::Call {
            head: "Element".to_string(),
            args: vec![sym("x"), sym("Reals")],
        };
        assert_eq!(builtin_refine(&[expr, assum], &Env::new()).unwrap(), int(0));
    }

    #[test]
    fn test_refine_re_real() {
        // Refine[Re[x], Element[x, Reals]] → x
        let expr = Value::Call {
            head: "Re".to_string(),
            args: vec![sym("x")],
        };
        let assum = Value::Call {
            head: "Element".to_string(),
            args: vec![sym("x"), sym("Reals")],
        };
        assert_eq!(
            builtin_refine(&[expr, assum], &Env::new()).unwrap(),
            sym("x")
        );
    }

    #[test]
    fn test_refine_and_assumptions() {
        // Refine[Abs[x], x > 0 && x < 10] → x  (because x > 0 is implied)
        let expr = Value::Call {
            head: "Abs".to_string(),
            args: vec![sym("x")],
        };
        let assum = Value::Call {
            head: "And".to_string(),
            args: vec![
                Value::Call {
                    head: "Greater".to_string(),
                    args: vec![sym("x"), int(0)],
                },
                Value::Call {
                    head: "Less".to_string(),
                    args: vec![sym("x"), int(10)],
                },
            ],
        };
        assert_eq!(
            builtin_refine(&[expr, assum], &Env::new()).unwrap(),
            sym("x")
        );
    }

    #[test]
    fn test_refine_list() {
        // Refine[{Sqrt[x^2], Sqrt[y^2]}, x > 0] → {x, Sqrt[y^2]}
        let expr = Value::List(vec![
            Value::Call {
                head: "Sqrt".to_string(),
                args: vec![Value::Call {
                    head: "Power".to_string(),
                    args: vec![sym("x"), int(2)],
                }],
            },
            Value::Call {
                head: "Sqrt".to_string(),
                args: vec![Value::Call {
                    head: "Power".to_string(),
                    args: vec![sym("y"), int(2)],
                }],
            },
        ]);
        let assum = Value::Call {
            head: "Greater".to_string(),
            args: vec![sym("x"), int(0)],
        };
        let result = builtin_refine(&[expr, assum], &Env::new()).unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                sym("x"),
                Value::Call {
                    head: "Sqrt".to_string(),
                    args: vec![Value::Call {
                        head: "Power".to_string(),
                        args: vec![sym("y"), int(2)],
                    }],
                },
            ])
        );
    }

    #[test]
    fn test_refine_sign() {
        // Refine[Sign[x], x > 0] → 1
        let expr = Value::Call {
            head: "Sign".to_string(),
            args: vec![sym("x")],
        };
        let assum = Value::Call {
            head: "Greater".to_string(),
            args: vec![sym("x"), int(0)],
        };
        assert_eq!(builtin_refine(&[expr, assum], &Env::new()).unwrap(), int(1));

        // Refine[Sign[x], x < 0] → -1
        let expr2 = Value::Call {
            head: "Sign".to_string(),
            args: vec![sym("x")],
        };
        let assum2 = Value::Call {
            head: "Less".to_string(),
            args: vec![sym("x"), int(0)],
        };
        assert_eq!(
            builtin_refine(&[expr2, assum2], &Env::new()).unwrap(),
            int(-1)
        );
    }

    #[test]
    fn test_refine_no_assum() {
        // Refine[Sqrt[x^2]] with no assumption → Sqrt[x^2] (no change)
        let expr = Value::Call {
            head: "Sqrt".to_string(),
            args: vec![Value::Call {
                head: "Power".to_string(),
                args: vec![sym("x"), int(2)],
            }],
        };
        let env = Env::new();
        let result = builtin_refine(&[expr.clone()], &env).unwrap();
        assert_eq!(result, expr);
    }
}
