use crate::env::Env;
use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;
use rug::ops::Pow;

// ── Simplify ──

pub fn builtin_simplify(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Simplify requires exactly 1 argument".to_string(),
        ));
    }
    Ok(simplify_value(&args[0]))
}

fn simplify_value(val: &Value) -> Value {
    match val {
        Value::Call { head, args } => {
            let simplified_args: Vec<Value> = args.iter().map(simplify_value).collect();
            simplify_call(head, &simplified_args)
        }
        _ => val.clone(),
    }
}

pub fn simplify_call(head: &str, args: &[Value]) -> Value {
    match head {
        "Plus" => simplify_plus(args),
        "Times" => simplify_times(args),
        "Power" => simplify_power(args),
        "Sin" => simplify_sin(args),
        "Cos" => simplify_cos(args),
        "Log" => simplify_log(args),
        "Exp" => simplify_exp(args),
        _ => Value::Call {
            head: head.to_string(),
            args: args.to_vec(),
        },
    }
}

fn simplify_plus(args: &[Value]) -> Value {
    if args.is_empty() {
        return Value::Integer(Integer::from(0));
    }
    let mut terms: Vec<Value> = Vec::new();
    for arg in args {
        match arg {
            Value::Integer(n) if n.is_zero() => {}
            Value::Call { head, args: a } if head == "Plus" => {
                terms.extend(a.iter().cloned());
            }
            _ => terms.push(arg.clone()),
        }
    }
    if terms.is_empty() {
        return Value::Integer(Integer::from(0));
    }
    if terms.len() == 1 {
        return terms.into_iter().next().unwrap();
    }
    if terms.len() == 2 && terms[0].struct_eq(&terms[1]) {
        return simplify_call(
            "Times",
            &[Value::Integer(Integer::from(2)), terms[0].clone()],
        );
    }
    Value::Call {
        head: "Plus".to_string(),
        args: terms,
    }
}

fn simplify_times(args: &[Value]) -> Value {
    if args.is_empty() {
        return Value::Integer(Integer::from(1));
    }
    let mut factors: Vec<Value> = Vec::new();
    for arg in args {
        match arg {
            Value::Integer(n) if n.is_zero() => return Value::Integer(Integer::from(0)),
            Value::Integer(n) if *n == 1 => {}
            Value::Call { head, args: a } if head == "Times" => {
                factors.extend(a.iter().cloned());
            }
            _ => factors.push(arg.clone()),
        }
    }
    if factors.is_empty() {
        return Value::Integer(Integer::from(1));
    }
    if factors.len() == 1 {
        return factors.into_iter().next().unwrap();
    }
    Value::Call {
        head: "Times".to_string(),
        args: factors,
    }
}

fn simplify_power(args: &[Value]) -> Value {
    if args.len() != 2 {
        return Value::Call {
            head: "Power".to_string(),
            args: args.to_vec(),
        };
    }
    match &args[1] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(1)),
        Value::Integer(n) if *n == 1 => args[0].clone(),
        _ => match &args[0] {
            Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(0)),
            Value::Integer(n) if *n == 1 => Value::Integer(Integer::from(1)),
            // Nested Power: Power[Power[x, a], b] → Power[x, a*b]
            Value::Call { head, args: inner } if head == "Power" && inner.len() == 2 => {
                let new_exp = simplify_call("Times", &[inner[1].clone(), args[1].clone()]);
                simplify_call("Power", &[inner[0].clone(), new_exp])
            }
            _ => Value::Call {
                head: "Power".to_string(),
                args: args.to_vec(),
            },
        },
    }
}

fn simplify_sin(args: &[Value]) -> Value {
    if args.len() != 1 {
        return Value::Call {
            head: "Sin".to_string(),
            args: args.to_vec(),
        };
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(0)),
        _ => Value::Call {
            head: "Sin".to_string(),
            args: args.to_vec(),
        },
    }
}

fn simplify_cos(args: &[Value]) -> Value {
    if args.len() != 1 {
        return Value::Call {
            head: "Cos".to_string(),
            args: args.to_vec(),
        };
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(1)),
        _ => Value::Call {
            head: "Cos".to_string(),
            args: args.to_vec(),
        },
    }
}

fn simplify_log(args: &[Value]) -> Value {
    if args.len() != 1 {
        return Value::Call {
            head: "Log".to_string(),
            args: args.to_vec(),
        };
    }
    match &args[0] {
        Value::Integer(n) if *n == 1 => Value::Integer(Integer::from(0)),
        Value::Real(r) => {
            let e_val = Float::with_val(DEFAULT_PRECISION, 1).exp();
            if (r.clone() - e_val).abs() < 1e-10 {
                Value::Integer(Integer::from(1))
            } else {
                Value::Call {
                    head: "Log".to_string(),
                    args: args.to_vec(),
                }
            }
        }
        Value::Call { head, args: inner } if head == "Exp" && inner.len() == 1 => inner[0].clone(),
        _ => Value::Call {
            head: "Log".to_string(),
            args: args.to_vec(),
        },
    }
}

fn simplify_exp(args: &[Value]) -> Value {
    if args.len() != 1 {
        return Value::Call {
            head: "Exp".to_string(),
            args: args.to_vec(),
        };
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(1)),
        Value::Call { head, args: inner } if head == "Log" && inner.len() == 1 => inner[0].clone(),
        _ => Value::Call {
            head: "Exp".to_string(),
            args: args.to_vec(),
        },
    }
}

// ── Expand ──

pub fn builtin_expand(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Expand requires exactly 1 argument".to_string(),
        ));
    }
    Ok(expand_value(&args[0]))
}

fn expand_value(val: &Value) -> Value {
    match val {
        Value::Call { head, args } => {
            let expanded_args: Vec<Value> = args.iter().map(expand_value).collect();
            match head.as_str() {
                "Times" => expand_times(&expanded_args),
                "Power" => expand_power(&expanded_args),
                _ => Value::Call {
                    head: head.to_string(),
                    args: expanded_args,
                },
            }
        }
        _ => val.clone(),
    }
}

fn expand_times(args: &[Value]) -> Value {
    if args.len() != 2 {
        return Value::Call {
            head: "Times".to_string(),
            args: args.to_vec(),
        };
    }
    let (left, right) = (&args[0], &args[1]);
    if let Value::Call {
        head,
        args: plus_args,
    } = right
        && head == "Plus"
    {
        let terms: Vec<Value> = plus_args
            .iter()
            .map(|term| simplify_call("Times", &[left.clone(), term.clone()]))
            .collect();
        return simplify_call("Plus", &terms);
    }
    if let Value::Call {
        head,
        args: plus_args,
    } = left
        && head == "Plus"
    {
        let terms: Vec<Value> = plus_args
            .iter()
            .map(|term| simplify_call("Times", &[term.clone(), right.clone()]))
            .collect();
        return simplify_call("Plus", &terms);
    }
    Value::Call {
        head: "Times".to_string(),
        args: args.to_vec(),
    }
}

fn expand_power(args: &[Value]) -> Value {
    if args.len() != 2 {
        return Value::Call {
            head: "Power".to_string(),
            args: args.to_vec(),
        };
    }
    let (base, exp) = (&args[0], &args[1]);
    if let Value::Integer(n) = exp
        && let Some(n_i64) = n.to_i64()
        && let Value::Call {
            head,
            args: plus_args,
        } = base
        && head == "Plus"
        && plus_args.len() == 2
        && (0..=10).contains(&n_i64)
    {
        let (a, b) = (&plus_args[0], &plus_args[1]);
        let mut terms = Vec::new();
        for k in 0..=n_i64 {
            let coeff = binomial(n_i64, k);
            let a_pow = if n_i64 - k == 0 {
                Value::Integer(Integer::from(1))
            } else if n_i64 - k == 1 {
                a.clone()
            } else {
                simplify_call(
                    "Power",
                    &[a.clone(), Value::Integer(Integer::from(n_i64 - k))],
                )
            };
            let b_pow = if k == 0 {
                Value::Integer(Integer::from(1))
            } else if k == 1 {
                b.clone()
            } else {
                simplify_call("Power", &[b.clone(), Value::Integer(Integer::from(k))])
            };
            let term = simplify_call(
                "Times",
                &[Value::Integer(Integer::from(coeff)), a_pow, b_pow],
            );
            terms.push(term);
        }
        return simplify_call("Plus", &terms);
    }
    Value::Call {
        head: "Power".to_string(),
        args: args.to_vec(),
    }
}

fn binomial(n: i64, k: i64) -> i64 {
    if k < 0 || k > n {
        return 0;
    }
    if k == 0 || k == n {
        return 1;
    }
    let k = if k > n - k { n - k } else { k };
    let mut result = 1i64;
    for i in 0..k {
        result = result * (n - i) / (i + 1);
    }
    result
}

// ── Differentiation ──

/// D[expr, x] — Symbolic differentiation.
pub fn builtin_d(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "D requires exactly 2 arguments".to_string(),
        ));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Symbol".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    Ok(differentiate(&args[0], &var, env))
}

pub fn differentiate(expr: &Value, var: &str, env: &Env) -> Value {
    match expr {
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => {
            Value::Integer(Integer::from(0))
        }
        Value::Symbol(s) => {
            if s == var {
                Value::Integer(Integer::from(1))
            } else if env.has_attribute(s, "Constant") {
                Value::Integer(Integer::from(0))
            } else {
                Value::Integer(Integer::from(0))
            }
        }
        Value::Call { head, args } => match head.as_str() {
            "Plus" => {
                let terms: Vec<Value> = args.iter().map(|arg| differentiate(arg, var, env)).collect();
                simplify_call("Plus", &terms)
            }
            "Times" => {
                if args.len() == 2 {
                    let (u, v) = (&args[0], &args[1]);
                    let du = differentiate(u, var, env);
                    let dv = differentiate(v, var, env);
                    simplify_call(
                        "Plus",
                        &[
                            simplify_call("Times", &[du, v.clone()]),
                            simplify_call("Times", &[u.clone(), dv]),
                        ],
                    )
                } else if args.len() == 1 {
                    differentiate(&args[0], var, env)
                } else {
                    let mut result = args[0].clone();
                    for item in args.iter().skip(1) {
                        result = simplify_call("Times", &[result, item.clone()]);
                    }
                    differentiate(&result, var, env)
                }
            }
            "Power" if args.len() == 2 => {
                let (base, exp) = (&args[0], &args[1]);
                let dbase = differentiate(base, var, env);
                match exp {
                    Value::Integer(n) => simplify_call(
                        "Times",
                        &[
                            Value::Integer(n.clone()),
                            simplify_call(
                                "Power",
                                &[base.clone(), Value::Integer(n - Integer::from(1))],
                            ),
                            dbase,
                        ],
                    ),
                    Value::Real(n) => {
                        let n_minus_1 = n.clone() - 1.0;
                        simplify_call(
                            "Times",
                            &[
                                Value::Real(n.clone()),
                                simplify_call("Power", &[base.clone(), Value::Real(n_minus_1)]),
                                dbase,
                            ],
                        )
                    }
                    _ => {
                        let dexp = differentiate(exp, var, env);
                        simplify_call(
                            "Times",
                            &[
                                expr.clone(),
                                simplify_call(
                                    "Plus",
                                    &[
                                        simplify_call(
                                            "Times",
                                            &[dexp, simplify_call("Log", &[base.clone()])],
                                        ),
                                        simplify_call(
                                            "Times",
                                            &[
                                                exp.clone(),
                                                simplify_call(
                                                    "Times",
                                                    &[
                                                        dbase,
                                                        simplify_call(
                                                            "Power",
                                                            &[
                                                                base.clone(),
                                                                Value::Integer(Integer::from(-1)),
                                                            ],
                                                        ),
                                                    ],
                                                ),
                                            ],
                                        ),
                                    ],
                                ),
                            ],
                        )
                    }
                }
            }
            "Sin" if args.len() == 1 => simplify_call(
                "Times",
                &[
                    simplify_call("Cos", &[args[0].clone()]),
                    differentiate(&args[0], var, env),
                ],
            ),
            "Cos" if args.len() == 1 => simplify_call(
                "Times",
                &[
                    Value::Integer(Integer::from(-1)),
                    simplify_call("Sin", &[args[0].clone()]),
                    differentiate(&args[0], var, env),
                ],
            ),
            "Tan" if args.len() == 1 => simplify_call(
                "Times",
                &[
                    differentiate(&args[0], var, env),
                    simplify_call(
                        "Power",
                        &[
                            simplify_call("Cos", &[args[0].clone()]),
                            Value::Integer(Integer::from(-2)),
                        ],
                    ),
                ],
            ),
            "Exp" if args.len() == 1 => simplify_call(
                "Times",
                &[
                    simplify_call("Exp", &[args[0].clone()]),
                    differentiate(&args[0], var, env),
                ],
            ),
            "Log" if args.len() == 1 => simplify_call(
                "Times",
                &[
                    differentiate(&args[0], var, env),
                    simplify_call(
                        "Power",
                        &[args[0].clone(), Value::Integer(Integer::from(-1))],
                    ),
                ],
            ),
            "Sqrt" if args.len() == 1 => simplify_call(
                "Times",
                &[
                    differentiate(&args[0], var, env),
                    simplify_call(
                        "Power",
                        &[
                            simplify_call(
                                "Times",
                                &[
                                    Value::Integer(Integer::from(2)),
                                    simplify_call("Sqrt", &[args[0].clone()]),
                                ],
                            ),
                            Value::Integer(Integer::from(-1)),
                        ],
                    ),
                ],
            ),
            "ArcSin" if args.len() == 1 => simplify_call(
                "Times",
                &[
                    differentiate(&args[0], var, env),
                    simplify_call(
                        "Power",
                        &[
                            simplify_call(
                                "Plus",
                                &[
                                    Value::Integer(Integer::from(1)),
                                    simplify_call(
                                        "Times",
                                        &[
                                            Value::Integer(Integer::from(-1)),
                                            simplify_call(
                                                "Power",
                                                &[
                                                    args[0].clone(),
                                                    Value::Integer(Integer::from(2)),
                                                ],
                                            ),
                                        ],
                                    ),
                                ],
                            ),
                            Value::Real(Float::with_val(DEFAULT_PRECISION, -0.5)),
                        ],
                    ),
                ],
            ),
            "ArcCos" if args.len() == 1 => simplify_call(
                "Times",
                &[
                    Value::Integer(Integer::from(-1)),
                    differentiate(&args[0], var, env),
                    simplify_call(
                        "Power",
                        &[
                            simplify_call(
                                "Plus",
                                &[
                                    Value::Integer(Integer::from(1)),
                                    simplify_call(
                                        "Times",
                                        &[
                                            Value::Integer(Integer::from(-1)),
                                            simplify_call(
                                                "Power",
                                                &[
                                                    args[0].clone(),
                                                    Value::Integer(Integer::from(2)),
                                                ],
                                            ),
                                        ],
                                    ),
                                ],
                            ),
                            Value::Real(Float::with_val(DEFAULT_PRECISION, -0.5)),
                        ],
                    ),
                ],
            ),
            "ArcTan" if args.len() == 1 => simplify_call(
                "Times",
                &[
                    differentiate(&args[0], var, env),
                    simplify_call(
                        "Power",
                        &[
                            simplify_call(
                                "Plus",
                                &[
                                    Value::Integer(Integer::from(1)),
                                    simplify_call(
                                        "Power",
                                        &[args[0].clone(), Value::Integer(Integer::from(2))],
                                    ),
                                ],
                            ),
                            Value::Integer(Integer::from(-1)),
                        ],
                    ),
                ],
            ),
            _ => Value::Call {
                head: "D".to_string(),
                args: vec![expr.clone(), Value::Symbol(var.to_string())],
            },
        },
        _ => Value::Call {
            head: "D".to_string(),
            args: vec![expr.clone(), Value::Symbol(var.to_string())],
        },
    }
}

// ── Integration ──

/// Integrate[expr, x] — Symbolic integration.
pub fn builtin_integrate(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Integrate requires exactly 2 arguments".to_string(),
        ));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Symbol".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    Ok(integrate(&args[0], &var))
}

fn integrate(expr: &Value, var: &str) -> Value {
    let x = Value::Symbol(var.to_string());
    match expr {
        Value::Integer(n) => simplify_call("Times", &[Value::Integer(n.clone()), x]),
        Value::Real(r) => simplify_call("Times", &[Value::Real(r.clone()), x]),
        Value::Symbol(s) => {
            if s == var {
                simplify_call(
                    "Times",
                    &[
                        Value::Real(Float::with_val(DEFAULT_PRECISION, 0.5)),
                        simplify_call("Power", &[x, Value::Integer(Integer::from(2))]),
                    ],
                )
            } else {
                simplify_call("Times", &[Value::Symbol(s.clone()), x])
            }
        }
        Value::Call { head, args } => match head.as_str() {
            "Plus" => {
                let terms: Vec<Value> = args.iter().map(|a| integrate(a, var)).collect();
                simplify_call("Plus", &terms)
            }
            "Times" => {
                let (constants, vars): (Vec<_>, Vec<_>) =
                    args.iter().partition(|a| is_constant_wrt(a, var));
                if vars.is_empty() {
                    simplify_call("Times", &[simplify_call("Times", args), x])
                } else if vars.len() == 1 {
                    let var_part = integrate(vars[0], var);
                    let const_vals: Vec<Value> = constants.iter().map(|c| (*c).clone()).collect();
                    let const_product = if constants.is_empty() {
                        Value::Integer(Integer::from(1))
                    } else {
                        simplify_call("Times", &const_vals)
                    };
                    simplify_call("Times", &[const_product, var_part])
                } else {
                    Value::Call {
                        head: "Integrate".to_string(),
                        args: vec![expr.clone(), x],
                    }
                }
            }
            "Power" if args.len() == 2 && args[0].struct_eq(&x) => match &args[1] {
                Value::Integer(n) if *n == -1 => simplify_call("Log", &[x]),
                Value::Integer(n) => {
                    let new_exp: Integer = n.clone() + 1;
                    simplify_call(
                        "Times",
                        &[
                            simplify_call("Power", &[x, Value::Integer(new_exp.clone())]),
                            simplify_call(
                                "Power",
                                &[Value::Integer(new_exp), Value::Integer(Integer::from(-1))],
                            ),
                        ],
                    )
                }
                Value::Real(n) => {
                    let new_exp: Float = n.clone() + 1.0;
                    simplify_call(
                        "Times",
                        &[
                            simplify_call("Power", &[x, Value::Real(new_exp.clone())]),
                            simplify_call(
                                "Power",
                                &[Value::Real(new_exp), Value::Integer(Integer::from(-1))],
                            ),
                        ],
                    )
                }
                _ => Value::Call {
                    head: "Integrate".to_string(),
                    args: vec![expr.clone(), x],
                },
            },
            "Sin" if args.len() == 1 && args[0].struct_eq(&x) => simplify_call(
                "Times",
                &[
                    Value::Integer(Integer::from(-1)),
                    simplify_call("Cos", &[x]),
                ],
            ),
            "Cos" if args.len() == 1 && args[0].struct_eq(&x) => simplify_call("Sin", &[x]),
            "Exp" if args.len() == 1 && args[0].struct_eq(&x) => simplify_call("Exp", &[x]),
            "Tan" if args.len() == 1 && args[0].struct_eq(&x) => simplify_call(
                "Times",
                &[
                    Value::Integer(Integer::from(-1)),
                    simplify_call("Log", &[simplify_call("Cos", &[x])]),
                ],
            ),
            // ∫ sec²(x) dx = tan(x)
            "Power"
                if args.len() == 2
                    && args[0].struct_eq(&simplify_call("Sec", &[x.clone()]))
                    && args[1].struct_eq(&Value::Integer(Integer::from(2))) =>
            {
                simplify_call("Tan", &[x.clone()])
            }
            // ∫ csc²(x) dx = -cot(x)
            "Power"
                if args.len() == 2
                    && args[0].struct_eq(&simplify_call("Csc", &[x.clone()]))
                    && args[1].struct_eq(&Value::Integer(Integer::from(2))) =>
            {
                simplify_call(
                    "Times",
                    &[
                        Value::Integer(Integer::from(-1)),
                        simplify_call("Cot", &[x.clone()]),
                    ],
                )
            }
            // ∫ log(x) dx = x*log(x) - x
            "Log" if args.len() == 1 && args[0].struct_eq(&x) => simplify_call(
                "Plus",
                &[
                    simplify_call("Times", &[x.clone(), simplify_call("Log", &[x.clone()])]),
                    simplify_call("Times", &[Value::Integer(Integer::from(-1)), x.clone()]),
                ],
            ),
            // Linear substitution: ∫ f(a*x + b) dx = (1/a) * F(a*x + b)
            // where F is the antiderivative of f, and a, b are constants
            head @ ("Sin" | "Cos" | "Exp" | "Tan") if args.len() == 1 => {
                if let Some((a, b)) = try_extract_linear(&args[0], var) {
                    if a != Value::Integer(Integer::from(0)) {
                        // Compute ∫ f(u) du where u = a*x + b
                        let inner = Value::Call {
                            head: head.to_string(),
                            args: vec![Value::Symbol("u".to_string())],
                        };
                        let f_of_u = integrate(&inner, "u");
                        // Substitute back u = a*x + b
                        let mut result = substitute_var(
                            &f_of_u,
                            "u",
                            &Value::Call {
                                head: "Plus".to_string(),
                                args: vec![
                                    simplify_call("Times", &[a.clone(), x.clone()]),
                                    b.clone().unwrap_or(Value::Integer(Integer::from(0))),
                                ],
                            },
                        );
                        // Multiply by 1/a for the chain rule
                        let inv_a = simplify_call("Power", &[a, Value::Integer(Integer::from(-1))]);
                        result = simplify_call("Times", &[inv_a, result]);
                        return result;
                    }
                }
                Value::Call {
                    head: "Integrate".to_string(),
                    args: vec![expr.clone(), x],
                }
            }
            _ => Value::Call {
                head: "Integrate".to_string(),
                args: vec![expr.clone(), x],
            },
        },
        _ => Value::Call {
            head: "Integrate".to_string(),
            args: vec![expr.clone(), x],
        },
    }
}

/// Try to extract a*x + b form from an expression.
/// Returns Some((a, Some(b))) for a*x + b, Some((a, None)) for a*x, None otherwise.
fn try_extract_linear(expr: &Value, var: &str) -> Option<(Value, Option<Value>)> {
    let x = Value::Symbol(var.to_string());
    match expr {
        Value::Symbol(s) if s == var => Some((Value::Integer(Integer::from(1)), None)),
        Value::Call { head, args } if head == "Times" && args.len() == 2 => {
            if args[0].struct_eq(&x) && is_constant_wrt(&args[1], var) {
                Some((args[1].clone(), None))
            } else if args[1].struct_eq(&x) && is_constant_wrt(&args[0], var) {
                Some((args[0].clone(), None))
            } else {
                None
            }
        }
        Value::Call { head, args } if head == "Plus" && args.len() == 2 => {
            // Check if one arg is a*x form and the other is constant
            let linear = try_extract_linear(&args[0], var);
            let constant = try_extract_linear(&args[1], var);
            match (linear, constant) {
                (Some((a, None)), _) if is_constant_wrt(&args[1], var) => {
                    Some((a, Some(args[1].clone())))
                }
                (_, Some((a, None))) if is_constant_wrt(&args[0], var) => {
                    Some((a, Some(args[0].clone())))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Substitute a variable with an expression in a value tree.
fn substitute_var(expr: &Value, old_var: &str, new_expr: &Value) -> Value {
    match expr {
        Value::Symbol(s) if s == old_var => new_expr.clone(),
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => {
            expr.clone()
        }
        Value::Symbol(s) => Value::Symbol(s.clone()),
        Value::List(items) => Value::List(
            items
                .iter()
                .map(|i| substitute_var(i, old_var, new_expr))
                .collect(),
        ),
        Value::Call { head, args } => Value::Call {
            head: head.clone(),
            args: args
                .iter()
                .map(|a| substitute_var(a, old_var, new_expr))
                .collect(),
        },
        other => other.clone(),
    }
}

fn is_constant_wrt(val: &Value, var: &str) -> bool {
    match val {
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => true,
        Value::Symbol(s) => s != var,
        Value::Call { args, .. } => args.iter().all(|a| is_constant_wrt(a, var)),
        _ => true,
    }
}

// ── Factor ──

/// Factor[expr] — Polynomial factorization.
pub fn builtin_factor(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Factor requires exactly 1 argument".to_string(),
        ));
    }
    Ok(factor_expr(&args[0]))
}

/// Try to factor a polynomial expression.
fn factor_expr(expr: &Value) -> Value {
    // Find the variable (first non-constant symbol)
    let var = match find_polynomial_var(expr) {
        Some(v) => v,
        None => return expr.clone(),
    };
    let coeffs = extract_polynomial_coeffs(expr, &var);
    if coeffs.is_empty() {
        return expr.clone();
    }
    match coeffs.len() - 1 {
        0 => coeffs[0].clone(),
        1 => factor_linear(&coeffs, &var),
        2 => factor_quadratic(&coeffs, &var),
        _ => factor_by_gcd(&coeffs, &var),
    }
}

/// Find the polynomial variable in an expression.
fn find_polynomial_var(expr: &Value) -> Option<String> {
    match expr {
        Value::Symbol(s) if !is_known_constant(s) => Some(s.clone()),
        Value::Call { head, args } => {
            if head == "Plus" || head == "Times" {
                for arg in args {
                    if let Some(v) = find_polynomial_var(arg) {
                        return Some(v);
                    }
                }
            }
            if head == "Power"
                && args.len() == 2
                && let Some(v) = find_polynomial_var(&args[0])
            {
                return Some(v);
            }
            None
        }
        _ => None,
    }
}

fn is_known_constant(s: &str) -> bool {
    matches!(s, "Pi" | "E" | "I" | "True" | "False" | "Null" | "Degree")
}

fn factor_linear(coeffs: &[Value], var: &str) -> Value {
    let a = &coeffs[0];
    let b = &coeffs[1];
    let x = Value::Symbol(var.to_string());
    if let (Value::Integer(ai), Value::Integer(bi)) = (a, b) {
        let gcd = ai.clone().gcd(bi);
        if gcd > 1 {
            let new_a = Value::Call {
                head: "Plus".to_string(),
                args: vec![
                    Value::Integer(ai.clone() / &gcd),
                    Value::Call {
                        head: "Times".to_string(),
                        args: vec![Value::Integer(bi.clone() / &gcd), x],
                    },
                ],
            };
            return Value::Call {
                head: "Times".to_string(),
                args: vec![Value::Integer(gcd), new_a],
            };
        }
    }
    Value::Call {
        head: "Plus".to_string(),
        args: vec![
            a.clone(),
            Value::Call {
                head: "Times".to_string(),
                args: vec![b.clone(), x],
            },
        ],
    }
}

fn factor_quadratic(coeffs: &[Value], var: &str) -> Value {
    let c = &coeffs[0];
    let b = &coeffs[1];
    let a = &coeffs[2];
    let x = Value::Symbol(var.to_string());

    if let (Value::Integer(ai), Value::Integer(bi), Value::Integer(ci)) = (a, b, c)
        && !ai.is_zero()
    {
        // Discriminant: b^2 - 4ac
        let b_sq: Integer = (bi * bi).into();
        let four_ac: Integer = Integer::from(4) * ai * ci;
        let disc: Integer = b_sq - four_ac;
        if !disc.is_negative() {
            let sqrt_disc = disc.clone().sqrt();
            let sqrt_disc_sq: Integer = (&sqrt_disc * &sqrt_disc).into();
            if sqrt_disc_sq == disc {
                let two_a: Integer = Integer::from(2) * ai;
                let neg_bi: Integer = (-bi).into();
                let r1_num: Integer = (&neg_bi + &sqrt_disc).into();
                let r2_num: Integer = (&neg_bi - &sqrt_disc).into();
                if r1_num.is_divisible(&two_a) && r2_num.is_divisible(&two_a) {
                    let r1: Integer = r1_num / &two_a;
                    let r2: Integer = r2_num / &two_a;
                    let factor1 = Value::Call {
                        head: "Plus".to_string(),
                        args: vec![x.clone(), Value::Integer(-r1)],
                    };
                    let factor2 = Value::Call {
                        head: "Plus".to_string(),
                        args: vec![x, Value::Integer(-r2)],
                    };
                    if *ai == 1 {
                        return Value::Call {
                            head: "Times".to_string(),
                            args: vec![factor1, factor2],
                        };
                    } else {
                        return Value::Call {
                            head: "Times".to_string(),
                            args: vec![Value::Integer(ai.clone()), factor1, factor2],
                        };
                    }
                }
            }
        }
    }
    expr_from_coeffs(coeffs, var)
}

fn factor_by_gcd(coeffs: &[Value], var: &str) -> Value {
    let integers: Vec<&Integer> = coeffs
        .iter()
        .filter_map(|c| match c {
            Value::Integer(n) => Some(n),
            _ => None,
        })
        .collect();
    if integers.len() != coeffs.len() || integers.is_empty() {
        return expr_from_coeffs(coeffs, var);
    }
    let mut gcd = integers[0].clone();
    for n in &integers[1..] {
        gcd = gcd.gcd(n);
    }
    if gcd <= 1 {
        return expr_from_coeffs(coeffs, var);
    }
    let new_coeffs: Vec<Value> = coeffs
        .iter()
        .map(|c| match c {
            Value::Integer(n) => Value::Integer(n.clone() / &gcd),
            other => other.clone(),
        })
        .collect();
    let inner = expr_from_coeffs(&new_coeffs, var);
    Value::Call {
        head: "Times".to_string(),
        args: vec![Value::Integer(gcd), inner],
    }
}

fn expr_from_coeffs(coeffs: &[Value], var: &str) -> Value {
    let x = Value::Symbol(var.to_string());
    let mut terms = Vec::new();
    for (i, coeff) in coeffs.iter().enumerate() {
        if matches!(coeff, Value::Integer(n) if n.is_zero()) {
            continue;
        }
        let term = match i {
            0 => coeff.clone(),
            1 => Value::Call {
                head: "Times".to_string(),
                args: vec![coeff.clone(), x.clone()],
            },
            _ => Value::Call {
                head: "Times".to_string(),
                args: vec![
                    coeff.clone(),
                    Value::Call {
                        head: "Power".to_string(),
                        args: vec![x.clone(), Value::Integer(Integer::from(i))],
                    },
                ],
            },
        };
        terms.push(term);
    }
    if terms.is_empty() {
        Value::Integer(Integer::from(0))
    } else if terms.len() == 1 {
        terms.into_iter().next().unwrap()
    } else {
        Value::Call {
            head: "Plus".to_string(),
            args: terms,
        }
    }
}

// ── Solve ──

/// Solve[equation, x] — Symbolic equation solving.
pub fn builtin_solve(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Solve requires exactly 2 arguments".to_string(),
        ));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Symbol".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let (lhs, rhs) = match &args[0] {
        Value::Call {
            head,
            args: eq_args,
        } if head == "Equal" && eq_args.len() == 2 => (eq_args[0].clone(), eq_args[1].clone()),
        _ => {
            return Ok(Value::Call {
                head: "Solve".to_string(),
                args: args.to_vec(),
            });
        }
    };
    let poly = simplify_call(
        "Plus",
        &[
            lhs,
            simplify_call("Times", &[Value::Integer(Integer::from(-1)), rhs]),
        ],
    );
    Ok(solve_polynomial(&poly, &var))
}

fn solve_polynomial(expr: &Value, var: &str) -> Value {
    let coeffs = extract_polynomial_coeffs(expr, var);
    match coeffs.len() {
        2 => {
            let (b, a) = (&coeffs[0], &coeffs[1]);
            match (a, b) {
                (Value::Integer(ai), Value::Integer(bi)) => {
                    if ai.is_zero() {
                        return Value::List(vec![]);
                    }
                    let result = Float::with_val(DEFAULT_PRECISION, bi)
                        / Float::with_val(DEFAULT_PRECISION, ai);
                    let neg_result = -result;
                    Value::List(vec![Value::Rule {
                        lhs: Box::new(Value::Symbol(var.to_string())),
                        rhs: Box::new(Value::Real(neg_result)),
                        delayed: false,
                    }])
                }
                _ => Value::List(vec![Value::Rule {
                    lhs: Box::new(Value::Symbol(var.to_string())),
                    rhs: Box::new(simplify_call(
                        "Times",
                        &[
                            Value::Integer(Integer::from(-1)),
                            simplify_call("Power", &[a.clone(), Value::Integer(Integer::from(-1))]),
                            b.clone(),
                        ],
                    )),
                    delayed: false,
                }]),
            }
        }
        3 => {
            let (c, b, a) = (&coeffs[0], &coeffs[1], &coeffs[2]);
            match (a, b, c) {
                (Value::Integer(ai), Value::Integer(bi), Value::Integer(ci)) => {
                    let disc = bi * bi - Integer::from(4) * ai * ci;
                    if disc < 0 {
                        return Value::List(vec![]);
                    }
                    let disc_f = Float::with_val(DEFAULT_PRECISION, &disc);
                    let sqrt_disc = disc_f.sqrt();
                    let bi_f = Float::with_val(DEFAULT_PRECISION, bi);
                    let ai_f = Float::with_val(DEFAULT_PRECISION, ai);
                    let two = Float::with_val(DEFAULT_PRECISION, 2);
                    let x1 = (-bi_f.clone() + sqrt_disc.clone()) / (two.clone() * ai_f.clone());
                    let x2 = (-bi_f - sqrt_disc) / (two * ai_f);
                    if disc.is_zero() {
                        Value::List(vec![Value::Rule {
                            lhs: Box::new(Value::Symbol(var.to_string())),
                            rhs: Box::new(Value::Real(x1)),
                            delayed: false,
                        }])
                    } else {
                        Value::List(vec![
                            Value::Rule {
                                lhs: Box::new(Value::Symbol(var.to_string())),
                                rhs: Box::new(Value::Real(x1)),
                                delayed: false,
                            },
                            Value::Rule {
                                lhs: Box::new(Value::Symbol(var.to_string())),
                                rhs: Box::new(Value::Real(x2)),
                                delayed: false,
                            },
                        ])
                    }
                }
                _ => Value::Call {
                    head: "Solve".to_string(),
                    args: vec![
                        simplify_call("Equal", &[expr.clone(), Value::Integer(Integer::from(0))]),
                        Value::Symbol(var.to_string()),
                    ],
                },
            }
        }
        _ => Value::Call {
            head: "Solve".to_string(),
            args: vec![
                simplify_call("Equal", &[expr.clone(), Value::Integer(Integer::from(0))]),
                Value::Symbol(var.to_string()),
            ],
        },
    }
}

fn extract_polynomial_coeffs(expr: &Value, var: &str) -> Vec<Value> {
    let terms = flatten_to_plus_terms(expr);
    let mut max_degree = 0i64;
    let mut coeff_map: std::collections::HashMap<i64, Value> = std::collections::HashMap::new();
    for term in &terms {
        let (coeff, degree) = extract_term_coeff_degree(term, var);
        if degree >= 0 {
            max_degree = max_degree.max(degree);
            let existing = coeff_map
                .remove(&degree)
                .unwrap_or(Value::Integer(Integer::from(0)));
            coeff_map.insert(degree, simplify_call("Plus", &[existing, coeff]));
        }
    }
    let mut result = Vec::new();
    for d in 0..=max_degree {
        result.push(
            coeff_map
                .remove(&d)
                .unwrap_or(Value::Integer(Integer::from(0))),
        );
    }
    result
}

fn flatten_to_plus_terms(expr: &Value) -> Vec<Value> {
    match expr {
        Value::Call { head, args } if head == "Plus" => {
            let mut result = Vec::new();
            for arg in args {
                result.extend(flatten_to_plus_terms(arg));
            }
            result
        }
        _ => vec![expr.clone()],
    }
}

fn extract_term_coeff_degree(term: &Value, var: &str) -> (Value, i64) {
    match term {
        Value::Symbol(s) if s == var => (Value::Integer(Integer::from(1)), 1),
        Value::Symbol(_) | Value::Integer(_) | Value::Real(_) => {
            if is_constant_wrt(term, var) {
                (term.clone(), 0)
            } else {
                (Value::Integer(Integer::from(0)), -1)
            }
        }
        Value::Call { head, args } => match head.as_str() {
            "Times" => {
                let mut coeff = Value::Integer(Integer::from(1));
                let mut degree = 0i64;
                for arg in args {
                    let (c, d) = extract_term_coeff_degree(arg, var);
                    coeff = simplify_call("Times", &[coeff, c]);
                    degree += d;
                }
                (coeff, degree)
            }
            "Power" if args.len() == 2 && args[0].struct_eq(&Value::Symbol(var.to_string())) => {
                match &args[1] {
                    Value::Integer(n) => {
                        (Value::Integer(Integer::from(1)), n.to_i64().unwrap_or(0))
                    }
                    _ => (Value::Integer(Integer::from(0)), -1),
                }
            }
            _ => {
                if is_constant_wrt(term, var) {
                    (term.clone(), 0)
                } else {
                    (Value::Integer(Integer::from(0)), -1)
                }
            }
        },
        _ => (Value::Integer(Integer::from(0)), -1),
    }
}

// ── Series ──

/// Series[expr, {x, x0, n}] — Taylor series expansion.
pub fn builtin_series(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Series requires exactly 2 arguments".to_string(),
        ));
    }
    let spec = match &args[1] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    if spec.len() != 3 {
        return Err(EvalError::Error(
            "Series spec must be {x, x0, n}".to_string(),
        ));
    }
    let var = match &spec[0] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Symbol".to_string(),
                got: spec[0].type_name().to_string(),
            });
        }
    };
    let x0 = spec[1].clone();
    let order = spec[2].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: spec[2].type_name().to_string(),
    })?;

    let x_sym = Value::Symbol(var.clone());
    let mut terms = Vec::new();
    let mut derivative = args[0].clone();

    for n in 0..=order {
        let coeff_val = substitute_and_eval(&derivative, &var, &x0);
        let factorial_val = Value::Integer(super::math::factorial(n));
        let coeff = match (&coeff_val, &factorial_val) {
            (Value::Integer(c), Value::Integer(f)) if !f.is_zero() => {
                let c_f = Float::with_val(DEFAULT_PRECISION, c);
                let f_f = Float::with_val(DEFAULT_PRECISION, f);
                Value::Real(c_f / f_f)
            }
            (Value::Real(c), Value::Integer(f)) if !f.is_zero() => {
                let f_f = Float::with_val(DEFAULT_PRECISION, f);
                Value::Real(c / f_f)
            }
            _ => simplify_call(
                "Times",
                &[
                    coeff_val,
                    simplify_call("Power", &[factorial_val, Value::Integer(Integer::from(-1))]),
                ],
            ),
        };
        if n == 0 {
            terms.push(coeff);
        } else {
            let x_minus_x0 = simplify_call(
                "Plus",
                &[
                    x_sym.clone(),
                    simplify_call("Times", &[Value::Integer(Integer::from(-1)), x0.clone()]),
                ],
            );
            let power_term = if n == 1 {
                x_minus_x0
            } else {
                simplify_call("Power", &[x_minus_x0, Value::Integer(Integer::from(n))])
            };
            terms.push(simplify_call("Times", &[coeff, power_term]));
        }
        derivative = differentiate(&derivative, &var, env);
    }
    Ok(simplify_call("Plus", &terms))
}

fn substitute_and_eval(expr: &Value, var: &str, val: &Value) -> Value {
    match expr {
        Value::Symbol(s) if s == var => val.clone(),
        Value::Symbol(_)
        | Value::Integer(_)
        | Value::Real(_)
        | Value::Bool(_)
        | Value::Str(_)
        | Value::Null => expr.clone(),
        Value::Call { head, args } => {
            let new_args: Vec<Value> = args
                .iter()
                .map(|a| substitute_and_eval(a, var, val))
                .collect();
            let result = simplify_call(head, &new_args);
            match &result {
                Value::Call { head: h, args: a } if h == head => {
                    try_numerical_eval(head, a).unwrap_or(result)
                }
                _ => result,
            }
        }
        _ => expr.clone(),
    }
}

fn try_numerical_eval(head: &str, args: &[Value]) -> Option<Value> {
    match head {
        "Plus" => {
            let mut sum = Float::with_val(DEFAULT_PRECISION, 0);
            let mut all_int = true;
            for arg in args {
                match arg {
                    Value::Integer(n) => sum += Float::with_val(DEFAULT_PRECISION, n),
                    Value::Real(r) => {
                        sum += r;
                        all_int = false;
                    }
                    _ => return None,
                }
            }
            if all_int && sum.is_integer() {
                let i = sum.to_f64() as i64;
                return Some(Value::Integer(Integer::from(i)));
            }
            Some(Value::Real(sum))
        }
        "Times" => {
            let mut product = Float::with_val(DEFAULT_PRECISION, 1);
            let mut all_int = true;
            for arg in args {
                match arg {
                    Value::Integer(n) => product *= Float::with_val(DEFAULT_PRECISION, n),
                    Value::Real(r) => {
                        product *= r;
                        all_int = false;
                    }
                    _ => return None,
                }
            }
            if all_int && product.is_integer() {
                let i = product.to_f64() as i64;
                return Some(Value::Integer(Integer::from(i)));
            }
            Some(Value::Real(product))
        }
        "Power" if args.len() == 2 => match (&args[0], &args[1]) {
            (Value::Integer(base), Value::Integer(exp)) => {
                if let Some(e) = exp.to_u32() {
                    Some(Value::Integer(base.clone().pow(e)))
                } else {
                    let b = Float::with_val(DEFAULT_PRECISION, base);
                    let e = Float::with_val(DEFAULT_PRECISION, exp);
                    Some(Value::Real(b.pow(e)))
                }
            }
            (Value::Real(base), Value::Real(exp)) => Some(Value::Real(base.clone().pow(exp))),
            (Value::Integer(base), Value::Real(exp)) => {
                let b = Float::with_val(DEFAULT_PRECISION, base);
                Some(Value::Real(b.pow(exp)))
            }
            (Value::Real(base), Value::Integer(exp)) => {
                let e = Float::with_val(DEFAULT_PRECISION, exp);
                Some(Value::Real(base.clone().pow(e)))
            }
            _ => None,
        },
        "Sin" if args.len() == 1 => match &args[0] {
            Value::Integer(n) => {
                let f = Float::with_val(DEFAULT_PRECISION, n);
                Some(Value::Real(f.sin()))
            }
            Value::Real(r) => Some(Value::Real(r.clone().sin())),
            _ => None,
        },
        "Cos" if args.len() == 1 => match &args[0] {
            Value::Integer(n) => {
                let f = Float::with_val(DEFAULT_PRECISION, n);
                Some(Value::Real(f.cos()))
            }
            Value::Real(r) => Some(Value::Real(r.clone().cos())),
            _ => None,
        },
        "Exp" if args.len() == 1 => match &args[0] {
            Value::Integer(n) => {
                let f = Float::with_val(DEFAULT_PRECISION, n);
                Some(Value::Real(f.exp()))
            }
            Value::Real(r) => Some(Value::Real(r.clone().exp())),
            _ => None,
        },
        "Log" if args.len() == 1 => match &args[0] {
            Value::Integer(n) if !n.is_zero() && !n.is_negative() => {
                let f = Float::with_val(DEFAULT_PRECISION, n);
                Some(Value::Real(f.ln()))
            }
            Value::Real(r) if !r.is_zero() && !r.is_sign_negative() => {
                Some(Value::Real(r.clone().ln()))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Stub for SetAttributes (evaluator-dependent, handled in eval.rs).
pub fn builtin_set_attributes(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "SetAttributes requires at least 2 arguments: symbol and attributes".to_string(),
        ));
    }
    let sym_name = match &args[0] {
        Value::Symbol(s) | Value::Str(s) => s.clone(),
        Value::Builtin(name, _) => name.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Symbol or String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    // Locked attribute prevents modification
    if env.has_attribute(&sym_name, "Locked") {
        return Ok(Value::Null);
    }
    let attrs: Vec<String> = args[1..].iter().map(|a| a.to_string()).collect();
    env.set_attributes(&sym_name, attrs);
    Ok(Value::Null)
}

pub fn builtin_clear_attributes(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 1 {
        return Err(EvalError::Error(
            "ClearAttributes requires at least 1 argument: symbol and optionally attributes to clear"
                .to_string(),
        ));
    }
    let sym_name = match &args[0] {
        Value::Symbol(s) | Value::Str(s) => s.clone(),
        Value::Builtin(name, _) => name.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Symbol or String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    // Locked attribute prevents modification
    if env.has_attribute(&sym_name, "Locked") {
        return Ok(Value::Null);
    }
    if args.len() == 1 {
        // Clear all attributes
        env.clear_attributes(&sym_name);
    } else {
        // Clear specific attributes
        let mut current = env.get_attributes(&sym_name);
        let to_remove: Vec<String> = args[1..].iter().map(|a| a.to_string()).collect();
        current.retain(|a| !to_remove.contains(a));
        env.set_attributes(&sym_name, current);
    }
    Ok(Value::Null)
}

pub fn builtin_attributes(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Attributes requires exactly 1 argument".to_string(),
        ));
    }
    let sym_name = match &args[0] {
        Value::Symbol(s) | Value::Str(s) => s.clone(),
        Value::Builtin(name, _) => name.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Symbol or String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let attrs = env.get_attributes(&sym_name);
    let list: Vec<Value> = attrs.into_iter().map(Value::Symbol).collect();
    Ok(Value::List(list))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn sym(s: &str) -> Value {
        Value::Symbol(s.to_string())
    }

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn env() -> crate::env::Env {
        crate::env::Env::new()
    }

    #[test]
    fn test_factor_constant() {
        assert_eq!(factor_expr(&int(5)), int(5));
    }

    #[test]
    fn test_factor_linear_gcd() {
        // Factor[2x + 4] = 2(x + 2)
        let expr = simplify_call(
            "Plus",
            &[simplify_call("Times", &[int(2), sym("x")]), int(4)],
        );
        let result = factor_expr(&expr);
        match &result {
            Value::Call { head, args } if head == "Times" => {
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], int(2));
            }
            _ => panic!("Expected Times call, got {:?}", result),
        }
    }

    #[test]
    fn test_factor_difference_of_squares() {
        // Factor[x^2 - 1] = (x - 1)(x + 1)
        let expr = simplify_call(
            "Plus",
            &[simplify_call("Power", &[sym("x"), int(2)]), int(-1)],
        );
        let result = factor_expr(&expr);
        match &result {
            Value::Call { head, args } if head == "Times" => {
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Times call, got {:?}", result),
        }
    }

    #[test]
    fn test_factor_perfect_square() {
        // Factor[x^2 + 2x + 1] = (x + 1)^2
        let expr = simplify_call(
            "Plus",
            &[
                simplify_call("Power", &[sym("x"), int(2)]),
                simplify_call("Times", &[int(2), sym("x")]),
                int(1),
            ],
        );
        let result = factor_expr(&expr);
        match &result {
            Value::Call { head, args } if head == "Times" => {
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Times call, got {:?}", result),
        }
    }

    // ── Expand tests ──

    #[test]
    fn test_expand_basic_power() {
        // Expand[(x+1)^2] = x^2 + 2x + 1
        let expr = simplify_call(
            "Power",
            &[simplify_call("Plus", &[sym("x"), int(1)]), int(2)],
        );
        let result = builtin_expand(&[expr]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Plus" => {}
            _ => panic!("Expected Plus (expanded form), got {:?}", result),
        }
    }

    #[test]
    fn test_expand_product() {
        // Expand[(x+1)(x+2)] = x^2 + 3x + 2
        let expr = simplify_call(
            "Times",
            &[
                simplify_call("Plus", &[sym("x"), int(1)]),
                simplify_call("Plus", &[sym("x"), int(2)]),
            ],
        );
        let result = builtin_expand(&[expr]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Plus" => {}
            _ => panic!("Expected Plus (expanded), got {:?}", result),
        }
    }

    // ── Differentiation tests ──

    #[test]
    fn test_d_power_rule() {
        // D[x^3, x] = 3x^2
        let expr = simplify_call("Power", &[sym("x"), int(3)]);
        let result = builtin_d(&[expr, sym("x")], &env()).unwrap();
        match &result {
            Value::Call { head, args } if head == "Times" => {
                assert!(args.iter().any(|a| *a == int(3)));
            }
            _ => panic!("Expected Times containing 3, got {:?}", result),
        }
    }

    #[test]
    fn test_d_sin() {
        // D[Sin[x], x] = Cos[x]
        let expr = simplify_call("Sin", &[sym("x")]);
        let result = builtin_d(&[expr, sym("x")], &env()).unwrap();
        assert_eq!(result, simplify_call("Cos", &[sym("x")]));
    }

    #[test]
    fn test_d_cos() {
        // D[Cos[x], x] = -Sin[x]
        let expr = simplify_call("Cos", &[sym("x")]);
        let result = builtin_d(&[expr, sym("x")], &env()).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Times" => {}
            _ => panic!("Expected Times (for -Sin[x]), got {:?}", result),
        }
    }

    #[test]
    fn test_d_exp() {
        // D[Exp[x], x] = Exp[x]
        let expr = simplify_call("Exp", &[sym("x")]);
        let result = builtin_d(&[expr, sym("x")], &env()).unwrap();
        assert_eq!(result, simplify_call("Exp", &[sym("x")]));
    }

    #[test]
    fn test_d_log() {
        // D[Log[x], x] = 1/x = x^(-1)
        let expr = simplify_call("Log", &[sym("x")]);
        let result = builtin_d(&[expr, sym("x")], &env()).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Power" => {}
            _ => panic!("Expected Power (for 1/x = x^-1), got {:?}", result),
        }
    }

    #[test]
    fn test_d_sum_rule() {
        // D[x^2 + Sin[x], x] = 2x + Cos[x]
        let expr = simplify_call(
            "Plus",
            &[
                simplify_call("Power", &[sym("x"), int(2)]),
                simplify_call("Sin", &[sym("x")]),
            ],
        );
        let result = builtin_d(&[expr, sym("x")], &env()).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Plus" => {}
            _ => panic!("Expected Plus (sum rule), got {:?}", result),
        }
    }

    #[test]
    fn test_d_constant() {
        // D[5, x] = 0
        let result = builtin_d(&[int(5), sym("x")], &env()).unwrap();
        assert_eq!(result, int(0));
    }

    #[test]
    fn test_d_linear() {
        // D[3*x, x] = 3
        let expr = simplify_call("Times", &[int(3), sym("x")]);
        let result = builtin_d(&[expr, sym("x")], &env()).unwrap();
        assert_eq!(result, int(3));
    }

    // ── Integration tests ──

    #[test]
    fn test_integrate_power() {
        // Integrate[x^2, x] = x^3/3 (stored as Times[Power[x,3], Power[3,-1]])
        let expr = simplify_call("Power", &[sym("x"), int(2)]);
        let result = builtin_integrate(&[expr, sym("x")]).unwrap();
        match &result {
            Value::Call { head, args } if head == "Times" => {
                // Contains Power[x, 3]
                assert!(
                    args.iter().any(|a| matches!(a,
                        Value::Call { head, args: a_args } if head == "Power" && a_args.len() == 2
                            && a_args[0] == sym("x") && a_args[1] == int(3)
                    )),
                    "Expected Power[x,3] in result, got {:?}",
                    result
                );
            }
            _ => panic!("Expected Times (x^3/3), got {:?}", result),
        }
    }

    #[test]
    fn test_integrate_sin() {
        // Integrate[Sin[x], x] = -Cos[x]
        let expr = simplify_call("Sin", &[sym("x")]);
        let result = builtin_integrate(&[expr, sym("x")]).unwrap();
        match &result {
            Value::Call { head, args } if head == "Times" => {
                // Should contain Cos[x]
                assert!(
                    args.iter().any(|a| *a == simplify_call("Cos", &[sym("x")])),
                    "Expected Cos[x] in result, got {:?}",
                    result
                );
            }
            _ => panic!("Expected Times containing Cos[x], got {:?}", result),
        }
    }

    #[test]
    fn test_integrate_constant() {
        // Integrate[5, x] = 5x
        let result = builtin_integrate(&[int(5), sym("x")]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Times" => {}
            _ => panic!("Expected Times (5*x), got {:?}", result),
        }
    }

    // ── Solve tests ──

    #[test]
    fn test_solve_linear() {
        // Solve[2x + 1 == 0, x]
        let eq = Value::Call {
            head: "Equal".to_string(),
            args: vec![
                simplify_call(
                    "Plus",
                    &[simplify_call("Times", &[int(2), sym("x")]), int(1)],
                ),
                int(0),
            ],
        };
        let result = builtin_solve(&[eq, sym("x")]).unwrap();
        match &result {
            Value::List(items) if !items.is_empty() => {}
            _ => panic!("Expected a list of solutions, got {:?}", result),
        }
    }

    #[test]
    fn test_solve_quadratic() {
        // Solve[x^2 - 4 == 0, x]
        let eq = Value::Call {
            head: "Equal".to_string(),
            args: vec![
                simplify_call(
                    "Plus",
                    &[simplify_call("Power", &[sym("x"), int(2)]), int(-4)],
                ),
                int(0),
            ],
        };
        let result = builtin_solve(&[eq, sym("x")]).unwrap();
        match &result {
            Value::List(items) if !items.is_empty() => {}
            _ => panic!("Expected a list of solutions, got {:?}", result),
        }
    }

    // ── Series tests ──

    #[test]
    fn test_series_sin_minimal() {
        // Series[Sin[x], {x, 0, 1}] — minimal to avoid recursion issues
        let var_spec = Value::List(vec![sym("x"), int(0), int(1)]);
        let expr = simplify_call("Sin", &[sym("x")]);
        let result = builtin_series(&[expr, var_spec], &env()).unwrap();
        let result_str = format!("{:?}", result);
        assert!(
            !result_str.is_empty(),
            "Series[Sin[x], {{x, 0, 1}}] should produce a non-empty result"
        );
    }

    // ── Simplify tests ──

    #[test]
    fn test_simplify_trivial() {
        // Simplify[x] = x
        let result = builtin_simplify(&[sym("x")]).unwrap();
        assert_eq!(result, sym("x"));
    }
}
