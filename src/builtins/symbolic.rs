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
    {
        if head == "Plus" {
            let terms: Vec<Value> = plus_args
                .iter()
                .map(|term| simplify_call("Times", &[left.clone(), term.clone()]))
                .collect();
            return simplify_call("Plus", &terms);
        }
    }
    if let Value::Call {
        head,
        args: plus_args,
    } = left
    {
        if head == "Plus" {
            let terms: Vec<Value> = plus_args
                .iter()
                .map(|term| simplify_call("Times", &[term.clone(), right.clone()]))
                .collect();
            return simplify_call("Plus", &terms);
        }
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
    if let Value::Integer(n) = exp {
        if let Some(n_i64) = n.to_i64() {
            if let Value::Call {
                head,
                args: plus_args,
            } = base
            {
                if head == "Plus" && plus_args.len() == 2 && n_i64 >= 0 && n_i64 <= 10 {
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
            }
        }
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
pub fn builtin_d(args: &[Value]) -> Result<Value, EvalError> {
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
    Ok(differentiate(&args[0], &var))
}

pub fn differentiate(expr: &Value, var: &str) -> Value {
    match expr {
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => {
            Value::Integer(Integer::from(0))
        }
        Value::Symbol(s) => {
            if s == var {
                Value::Integer(Integer::from(1))
            } else {
                Value::Integer(Integer::from(0))
            }
        }
        Value::Call { head, args } => match head.as_str() {
            "Plus" => {
                let terms: Vec<Value> = args.iter().map(|arg| differentiate(arg, var)).collect();
                simplify_call("Plus", &terms)
            }
            "Times" => {
                if args.len() == 2 {
                    let (u, v) = (&args[0], &args[1]);
                    let du = differentiate(u, var);
                    let dv = differentiate(v, var);
                    simplify_call(
                        "Plus",
                        &[
                            simplify_call("Times", &[du, v.clone()]),
                            simplify_call("Times", &[u.clone(), dv]),
                        ],
                    )
                } else if args.len() == 1 {
                    differentiate(&args[0], var)
                } else {
                    let mut result = args[0].clone();
                    for i in 1..args.len() {
                        result = simplify_call("Times", &[result, args[i].clone()]);
                    }
                    differentiate(&result, var)
                }
            }
            "Power" if args.len() == 2 => {
                let (base, exp) = (&args[0], &args[1]);
                let dbase = differentiate(base, var);
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
                        let dexp = differentiate(exp, var);
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
                    differentiate(&args[0], var),
                ],
            ),
            "Cos" if args.len() == 1 => simplify_call(
                "Times",
                &[
                    Value::Integer(Integer::from(-1)),
                    simplify_call("Sin", &[args[0].clone()]),
                    differentiate(&args[0], var),
                ],
            ),
            "Tan" if args.len() == 1 => simplify_call(
                "Times",
                &[
                    differentiate(&args[0], var),
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
                    differentiate(&args[0], var),
                ],
            ),
            "Log" if args.len() == 1 => simplify_call(
                "Times",
                &[
                    differentiate(&args[0], var),
                    simplify_call(
                        "Power",
                        &[args[0].clone(), Value::Integer(Integer::from(-1))],
                    ),
                ],
            ),
            "Sqrt" if args.len() == 1 => simplify_call(
                "Times",
                &[
                    differentiate(&args[0], var),
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
                    differentiate(&args[0], var),
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
                    differentiate(&args[0], var),
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
                    differentiate(&args[0], var),
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
                    let var_part = integrate(&vars[0], var);
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
    Ok(args[0].clone())
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
pub fn builtin_series(args: &[Value]) -> Result<Value, EvalError> {
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
        derivative = differentiate(&derivative, &var);
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
