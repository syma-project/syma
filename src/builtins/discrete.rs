use crate::ast::Expr;
use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;

/// Convert an AST Expr to a Value without evaluation, producing structural Values
/// (Call nodes become Value::Call, not wrapped in Pattern).
fn expr_to_value(expr: &Expr) -> Value {
    match expr {
        Expr::Integer(n) => Value::Integer(n.clone()),
        Expr::Real(r) => Value::Real(r.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Str(s) => Value::Str(s.clone()),
        Expr::Null => Value::Null,
        Expr::Symbol(s) => Value::Symbol(s.clone()),
        Expr::List(items) => Value::List(items.iter().map(expr_to_value).collect()),
        Expr::Call { head, args } => {
            let head_str = match head.as_ref() {
                Expr::Symbol(s) => s.clone(),
                _ => String::new(),
            };
            Value::Call {
                head: head_str,
                args: args.iter().map(expr_to_value).collect(),
            }
        }
        _ => Value::Pattern(expr.clone()),
    }
}

// ── DiscreteDelta ────────────────────────────────────────────────────────────

/// DiscreteDelta[n1, n2, ...] — 1 if all arguments are zero, 0 otherwise.
pub fn builtin_discrete_delta(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "DiscreteDelta requires at least 1 argument".to_string(),
        ));
    }
    for arg in args {
        match arg {
            Value::Integer(n) if !n.is_zero() => return Ok(Value::Integer(Integer::from(0))),
            Value::Real(r) if r.to_f64() != 0.0 => return Ok(Value::Integer(Integer::from(0))),
            Value::Rational(r) if r.to_f64() != 0.0 => return Ok(Value::Integer(Integer::from(0))),
            Value::Integer(_) | Value::Real(_) | Value::Rational(_) => continue,
            _ => {
                // Non-numeric argument — return symbolic
                return Ok(Value::Call {
                    head: "DiscreteDelta".to_string(),
                    args: args.to_vec(),
                });
            }
        }
    }
    Ok(Value::Integer(Integer::from(1)))
}

// ── DiscreteShift ────────────────────────────────────────────────────────────

/// DiscreteShift[expr, n] — symbolic forward shift operator.
/// DiscreteShift[expr, n, h] — shift by step h.
pub fn builtin_discrete_shift(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 && args.len() != 3 {
        return Err(EvalError::Error(
            "DiscreteShift requires 2 or 3 arguments: DiscreteShift[expr, n] or DiscreteShift[expr, n, h]"
                .to_string(),
        ));
    }
    Ok(Value::Call {
        head: "DiscreteShift".to_string(),
        args: args.to_vec(),
    })
}

// ── DiscreteRatio ────────────────────────────────────────────────────────────

/// DiscreteRatio[expr, n] — symbolic ratio operator.
/// DiscreteRatio[expr, n, h] — ratio with step h.
pub fn builtin_discrete_ratio(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 && args.len() != 3 {
        return Err(EvalError::Error(
            "DiscreteRatio requires 2 or 3 arguments: DiscreteRatio[expr, n] or DiscreteRatio[expr, n, h]"
                .to_string(),
        ));
    }
    Ok(Value::Call {
        head: "DiscreteRatio".to_string(),
        args: args.to_vec(),
    })
}

// ── FactorialPower ───────────────────────────────────────────────────────────

/// FactorialPower[x, n] — falling factorial x * (x-1) * ... * (x-n+1).
/// FactorialPower[x, n, h] — falling factorial with step h.
pub fn builtin_factorial_power(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 && args.len() != 3 {
        return Err(EvalError::Error(
            "FactorialPower requires 2 or 3 arguments: FactorialPower[x, n] or FactorialPower[x, n, h]"
                .to_string(),
        ));
    }

    let n = match &args[1] {
        Value::Integer(n) if !n.is_negative() => n
            .to_usize()
            .ok_or_else(|| EvalError::Error("FactorialPower: n too large".to_string()))?,
        _ => {
            return Ok(Value::Call {
                head: "FactorialPower".to_string(),
                args: args.to_vec(),
            });
        }
    };

    if n == 0 {
        return Ok(Value::Integer(Integer::from(1)));
    }

    let h: i64 = if args.len() == 3 {
        match &args[2] {
            Value::Integer(h) => h
                .to_i64()
                .ok_or_else(|| EvalError::Error("FactorialPower: h too large".to_string()))?,
            _ => {
                return Ok(Value::Call {
                    head: "FactorialPower".to_string(),
                    args: args.to_vec(),
                });
            }
        }
    } else {
        1
    };

    match &args[0] {
        Value::Integer(x) => {
            let mut result = Integer::from(1);
            for i in 0..n {
                let term = Integer::from(x.to_i64().unwrap_or(0) - (i as i64) * h);
                result *= term;
            }
            Ok(Value::Integer(result))
        }
        Value::Real(x) => {
            let xf = x.to_f64();
            let mut result = 1.0;
            for i in 0..n {
                result *= xf - (i as f64) * (h as f64);
            }
            Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, result)))
        }
        _ => Ok(Value::Call {
            head: "FactorialPower".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── BernoulliB ───────────────────────────────────────────────────────────────

/// BernoulliB[n] — n-th Bernoulli number.
/// B_0 = 1, B_1 = -1/2, B_n = 0 for odd n > 1.
/// For even n > 0, compute via Akiyama-Tanigawa algorithm.
pub fn builtin_bernoulli_b(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "BernoulliB requires exactly 1 argument".to_string(),
        ));
    }
    let n = match &args[0] {
        Value::Integer(n) if !n.is_negative() => n
            .to_usize()
            .ok_or_else(|| EvalError::Error("BernoulliB: n too large".to_string()))?,
        _ => {
            return Ok(Value::Call {
                head: "BernoulliB".to_string(),
                args: args.to_vec(),
            });
        }
    };

    match n {
        0 => Ok(Value::Integer(Integer::from(1))),
        1 => {
            // Return -1/2 as a rational Call: Divide[-1, 2]
            Ok(Value::Call {
                head: "Divide".to_string(),
                args: vec![
                    Value::Integer(Integer::from(-1)),
                    Value::Integer(Integer::from(2)),
                ],
            })
        }
        _ if n % 2 == 1 => Ok(Value::Integer(Integer::from(0))),
        _ => {
            // Akiyama-Tanigawa algorithm for even n
            // B_n = a[0] where a[m] = 1/(m+1) and a[j-1] = j * (a[j-1] - a[j])
            let mut a = vec![Float::with_val(DEFAULT_PRECISION, 0.0); n + 1];
            for m in 0..=n {
                a[m] = Float::with_val(DEFAULT_PRECISION, 1.0)
                    / Float::with_val(DEFAULT_PRECISION, (m + 1) as f64);
                for j in (1..=m).rev() {
                    let diff = Float::with_val(DEFAULT_PRECISION, &a[j - 1] - &a[j]);
                    a[j - 1] = Float::with_val(DEFAULT_PRECISION, j as f64) * diff;
                }
            }
            let result = a[0].clone();
            Ok(Value::Real(result))
        }
    }
}

// ── LinearRecurrence ─────────────────────────────────────────────────────────

/// LinearRecurrence[kernel, init, n] — n-th term of a linear recurrence.
/// The kernel specifies coefficients (length k), init specifies initial values (length k).
/// n is 1-indexed.
pub fn builtin_linear_recurrence(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "LinearRecurrence requires 3 arguments: LinearRecurrence[kernel, init, n]".to_string(),
        ));
    }

    let kernel = match &args[0] {
        Value::List(k) => k,
        _ => {
            return Ok(Value::Call {
                head: "LinearRecurrence".to_string(),
                args: args.to_vec(),
            });
        }
    };

    let init = match &args[1] {
        Value::List(init) => init,
        _ => {
            return Ok(Value::Call {
                head: "LinearRecurrence".to_string(),
                args: args.to_vec(),
            });
        }
    };

    let n = match &args[2] {
        Value::Integer(n) if n.is_positive() => n
            .to_usize()
            .ok_or_else(|| EvalError::Error("LinearRecurrence: n too large".to_string()))?,
        _ => {
            return Ok(Value::Call {
                head: "LinearRecurrence".to_string(),
                args: args.to_vec(),
            });
        }
    };

    let k = kernel.len();
    if k == 0 {
        return Err(EvalError::Error(
            "LinearRecurrence: kernel must be non-empty".to_string(),
        ));
    }
    if init.len() != k {
        return Err(EvalError::Error(
            "LinearRecurrence: kernel and init must have the same length".to_string(),
        ));
    }

    // n is 1-indexed — if within initial values, return directly
    if n <= init.len() {
        return Ok(init[n - 1].clone());
    }

    // Extend the sequence iteratively
    let mut seq: Vec<Value> = init.to_vec();
    while seq.len() < n {
        let idx = seq.len();
        let mut next = Value::Integer(Integer::from(0));
        for j in 0..k {
            let term =
                crate::builtins::arithmetic::mul_values_public(&kernel[j], &seq[idx - k + j])?;
            next = crate::builtins::arithmetic::add_values_public(&next, &term)?;
        }
        seq.push(next);
    }

    Ok(seq[n - 1].clone())
}

// ── RSolve ───────────────────────────────────────────────────────────────────

/// RSolve[{recurrenceEq, initCond}, func, {var, min, max}] — solve recurrence equations.
/// Handles constant-coefficient linear recurrences of order 1 and 2.
/// Returns {{func[var] -> closed_form_solution}}.
/// Strip Value::Pattern wrapper (from HoldAll) if present, recursively.
fn strip_pattern(v: &Value) -> Value {
    match v {
        Value::Pattern(unevaluated) => expr_to_value(unevaluated),
        Value::List(items) => Value::List(items.iter().map(strip_pattern).collect()),
        _ => v.clone(),
    }
}

/// Handles constant-coefficient linear recurrences of order 1 and 2.
/// Returns {{func[var] -> closed_form_solution}}.
pub fn builtin_rsolve(args: &[Value], _env: &crate::env::Env) -> Result<Value, EvalError> {
    let args: Vec<Value> = args.iter().map(strip_pattern).collect();
    if args.len() != 3 {
        return Err(EvalError::Error(
            "RSolve requires 3 arguments: RSolve[{eqn, ic}, func, {var, min, max}]".to_string(),
        ));
    }

    // args[0]: List of {equation, initial_conditions}
    let constraints = match &args[0] {
        Value::List(items) if !items.is_empty() => items,
        _ => {
            return Ok(Value::Call {
                head: "RSolve".to_string(),
                args: args.to_vec(),
            });
        }
    };

    // args[1]: The function (e.g., Symbol "a" or Call "a[n]")
    let _func_name = match &args[1] {
        Value::Symbol(s) => s.clone(),
        Value::Call { head, .. } => head.clone(),
        _ => {
            return Ok(Value::Call {
                head: "RSolve".to_string(),
                args: args.to_vec(),
            });
        }
    };

    // args[2]: {var, min, max}
    let var_range = match &args[2] {
        Value::List(items) if !items.is_empty() => items,
        _ => {
            return Ok(Value::Call {
                head: "RSolve".to_string(),
                args: args.to_vec(),
            });
        }
    };

    let var_name = match &var_range[0] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Ok(Value::Call {
                head: "RSolve".to_string(),
                args: args.to_vec(),
            });
        }
    };

    // Parse the equation from constraints[0]
    let (lhs, rhs) = match &constraints[0] {
        Value::Call {
            head,
            args: eq_args,
        } if head == "Equal" && eq_args.len() == 2 => (eq_args[0].clone(), eq_args[1].clone()),
        _ => {
            return Ok(Value::Call {
                head: "RSolve".to_string(),
                args: args.to_vec(),
            });
        }
    };

    // Parse function name from LHS: must be a[n] form
    let (func_name, _) = match parse_func_name_offset(&lhs, &var_name) {
        Some(result) => result,
        None => {
            return Ok(Value::Call {
                head: "RSolve".to_string(),
                args: args.to_vec(),
            });
        }
    };

    // Parse initial conditions from remaining constraints
    let mut init_conds: Vec<(i64, Value)> = Vec::new();
    for cond in &constraints[1..] {
        if let Some((idx, val)) = parse_init_condition(cond) {
            init_conds.push((idx, val));
        }
    }

    // Parse RHS terms: collect (func_name, offset, coefficient) for each term
    let terms = parse_recurrence_rhs(&rhs, &var_name);
    if terms.is_empty() {
        return Ok(Value::Call {
            head: "RSolve".to_string(),
            args: args.to_vec(),
        });
    }

    // Validate: all terms must use the same function name and have positive offsets
    for (fname, order, _) in &terms {
        if fname != &func_name || *order <= 0 {
            return Ok(Value::Call {
                head: "RSolve".to_string(),
                args: args.to_vec(),
            });
        }
    }

    let order = terms.iter().map(|(_, o, _)| *o).max().unwrap_or(0) as usize;
    if init_conds.len() != order {
        return Ok(Value::Call {
            head: "RSolve".to_string(),
            args: args.to_vec(),
        });
    }

    // Extract coefficients: coefficient[k] = coefficient of a[n-k]
    let mut coefficient: Vec<Value> = vec![Value::Integer(Integer::from(0)); order + 1];
    for (_, order_k, coeff) in &terms {
        coefficient[*order_k as usize] = coeff.clone();
    }

    // Solve based on order
    match order as i64 {
        1 => {
            // Characteristic equation: r - c1 = 0 => root = c1
            let root = coefficient[1].clone();
            let solution = build_first_order_solution(&coefficient, &init_conds, &root);
            let lhs_call = Value::Call {
                head: func_name.clone(),
                args: vec![Value::Symbol(var_name.clone())],
            };
            Ok(Value::List(vec![Value::List(vec![Value::Rule {
                lhs: Box::new(lhs_call),
                rhs: Box::new(solution),
                delayed: false,
            }])]))
        }
        2 => {
            // Characteristic equation: r^2 - c1*r - c2 = 0
            let c1 = &coefficient[1];
            let c2 = &coefficient[2];
            let disc = eval_numeric(c1) * eval_numeric(c1)
                + Float::with_val(DEFAULT_PRECISION, 4.0) * eval_numeric(c2);
            if disc.is_sign_negative() {
                return Ok(Value::Call {
                    head: "RSolve".to_string(),
                    args: args.to_vec(),
                });
            }
            let sqrt_disc = disc.sqrt();
            let two = Float::with_val(DEFAULT_PRECISION, 2.0);
            let r1 = (eval_numeric(c1) + sqrt_disc.clone()) / two.clone();
            let r2 = (eval_numeric(c1) - sqrt_disc) / two;
            let (v1, v2) = match (try_as_value(&r1), try_as_value(&r2)) {
                (Some(a), Some(b)) => (a, b),
                _ => {
                    return Ok(Value::Call {
                        head: "RSolve".to_string(),
                        args: args.to_vec(),
                    });
                }
            };
            let solution = build_second_order_solution(&init_conds, &v1, &v2);
            let lhs_call = Value::Call {
                head: func_name.clone(),
                args: vec![Value::Symbol(var_name.clone())],
            };
            Ok(Value::List(vec![Value::List(vec![Value::Rule {
                lhs: Box::new(lhs_call),
                rhs: Box::new(solution),
                delayed: false,
            }])]))
        }
        _ => {
            // Orders > 2 not yet implemented
            Ok(Value::Call {
                head: "RSolve".to_string(),
                args: args.to_vec(),
            })
        }
    }
}

// ── RSolve helpers ───────────────────────────────────────────────────────────

/// Evaluate a simple numeric Call to an integer (e.g., Times[-1, 1] → -1).
fn call_to_int(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(k) => k.to_i64(),
        Value::Call { head, args } if head == "Times" && args.len() == 2 => {
            let a = call_to_int(&args[0])?;
            let b = call_to_int(&args[1])?;
            Some(a * b)
        }
        Value::Call { head, args } if head == "Plus" && args.len() == 2 => {
            let a = call_to_int(&args[0])?;
            let b = call_to_int(&args[1])?;
            Some(a + b)
        }
        _ => None,
    }
}

/// Extract (function_name, offset) from a[n] or a[n+k] form.
fn parse_func_name_offset(expr: &Value, var_name: &str) -> Option<(String, i64)> {
    if let Value::Call { head, args } = expr
        && args.len() == 1
    {
        let arg_expr = &args[0];
        let offset = match arg_expr {
            Value::Symbol(s) if s == var_name => Some(0),
            Value::Call {
                head,
                args: plus_args,
            } if head == "Plus" && plus_args.len() == 2 => {
                if plus_args[0].struct_eq(&Value::Symbol(var_name.to_string())) {
                    call_to_int(&plus_args[1])
                } else if plus_args[1].struct_eq(&Value::Symbol(var_name.to_string())) {
                    call_to_int(&plus_args[0])
                } else {
                    None
                }
            }
            _ => None,
        };
        return offset.map(|o| (head.clone(), o));
    }
    None
}

/// Flatten nested Times calls into a single list of arguments.
fn flatten_times(v: &Value) -> Vec<Value> {
    match v {
        Value::Call { head, args } if head == "Times" => {
            let mut result = Vec::new();
            for arg in args {
                if let Value::Call {
                    head: h2,
                    args: _a2,
                } = arg
                {
                    if h2 == "Times" {
                        result.extend(flatten_times(arg));
                    } else {
                        result.push(arg.clone());
                    }
                } else {
                    result.push(arg.clone());
                }
            }
            result
        }
        _ => vec![v.clone()],
    }
}

/// Extract (function_name, order, coefficient) from a single RHS term.
/// Order is the lag: a[n-k] has order k (so offset -k becomes order k).
fn parse_term(expr: &Value, var_name: &str) -> Option<(String, i64, Value)> {
    let args_flat = flatten_times(expr);
    let args_clone = args_flat.clone();
    for arg in &args_clone {
        if let Value::Call { .. } = arg
            && let Some((name, offset)) = parse_func_name_offset(arg, var_name)
        {
            let coeff_values: Vec<Value> = args_flat
                .into_iter()
                .filter(|a| !a.struct_eq(arg))
                .collect();
            let coeff = match coeff_values.len() {
                0 => Value::Integer(Integer::from(1)),
                1 => coeff_values.into_iter().next().unwrap(),
                _ => Value::Call {
                    head: "Times".to_string(),
                    args: coeff_values,
                },
            };
            return Some((name, -offset, coeff));
        }
    }
    None
}

/// Parse all terms from the RHS of a recurrence equation.
fn parse_recurrence_rhs(rhs: &Value, var_name: &str) -> Vec<(String, i64, Value)> {
    match rhs {
        Value::Call { head, args } if head == "Plus" => {
            let mut terms = Vec::new();
            for arg in args {
                if let Some(t) = parse_term(arg, var_name) {
                    terms.push(t);
                }
            }
            terms
        }
        _ => parse_term(rhs, var_name).into_iter().collect(),
    }
}

/// Parse an initial condition a[k] == v into (k, v).
fn parse_init_condition(expr: &Value) -> Option<(i64, Value)> {
    if let Value::Call { head, args } = expr
        && head == "Equal"
        && args.len() == 2
        && let Value::Call {
            args: call_args, ..
        } = &args[0]
        && call_args.len() == 1
        && let Value::Integer(k) = &call_args[0]
    {
        return k.to_i64().map(|k| (k, args[1].clone()));
    }
    None
}

/// Convert a Float to a Value (Integer if exact, otherwise Real).
fn try_as_value(f: &Float) -> Option<Value> {
    if f.is_integer() {
        f.to_integer()
            .and_then(|i| i.to_i64())
            .map(|i| Value::Integer(Integer::from(i)))
    } else {
        Some(Value::Real(f.clone()))
    }
}

/// Evaluate a numeric Value to a Float (handles simple Times/Plus calls).
fn eval_numeric(v: &Value) -> Float {
    match v {
        Value::Integer(n) => Float::with_val(DEFAULT_PRECISION, n),
        Value::Real(r) => r.clone(),
        Value::Rational(r) => Float::with_val(DEFAULT_PRECISION, &**r),
        Value::Call { head, args } if head == "Times" => {
            let mut result = Float::with_val(DEFAULT_PRECISION, 1);
            for arg in args {
                result *= eval_numeric(arg);
            }
            result
        }
        Value::Call { head, args } if head == "Plus" => {
            let mut result = Float::with_val(DEFAULT_PRECISION, 0);
            for arg in args {
                result += eval_numeric(arg);
            }
            result
        }
        _ => Float::with_val(DEFAULT_PRECISION, 0),
    }
}

/// Build the solution for a first-order recurrence.
/// Solution: a[n] = a0 * root^(n - k0)
fn build_first_order_solution(_coeffs: &[Value], init: &[(i64, Value)], root: &Value) -> Value {
    let (k0, a0) = &init[0];
    // Simplified: a0 * root^n (assuming k0==0 for the common case)
    if *k0 == 0 {
        simplify_times(vec![a0.clone(), power_expr(root, "n")])
    } else {
        simplify_times(vec![a0.clone(), power_expr(root, &format!("n - {}", k0))])
    }
}

/// Build the solution for a second-order recurrence.
/// Solution: a[n] = c1*r1^n + c2*r2^n (distinct roots)
/// or: a[n] = (c1 + c2*n)*r^n (equal roots)
fn build_second_order_solution(init: &[(i64, Value)], r1: &Value, r2: &Value) -> Value {
    let v1 = eval_numeric(r1);
    let v2 = eval_numeric(r2);

    // Check for equal roots (within numerical tolerance)
    let diff = (v1.clone() - &v2).abs();
    let tol = Float::with_val(DEFAULT_PRECISION, 1e-9);

    if diff < tol {
        // Equal roots: a[n] = (c1 + c2*n) * r^n
        build_equal_root_solution(init, r1, &v1)
    } else {
        // Distinct roots: a[n] = c1*r1^n + c2*r2^n
        build_distinct_root_solution(init, r1, r2, &v1, &v2)
    }
}

fn build_distinct_root_solution(
    init: &[(i64, Value)],
    r1: &Value,
    r2: &Value,
    v1: &Float,
    v2: &Float,
) -> Value {
    let (k1, a1_val) = &init[0];
    let (k2, a2_val) = &init[1];
    let a1_f = eval_numeric(a1_val);
    let a2_f = eval_numeric(a2_val);

    // Use f64 for Cramer's rule computation to avoid rug incomplete types
    let v1_f = v1.to_f64();
    let v2_f = v2.to_f64();
    let a1_d = a1_f.to_f64();
    let a2_d = a2_f.to_f64();

    // Cramer's rule: |r1^k1  r2^k1| |c1|   |a1|
    //                |r1^k2  r2^k2| |c2| = |a2|
    let r1_k1 = v1_f.powi(*k1 as i32);
    let r2_k1 = v2_f.powi(*k1 as i32);
    let r1_k2 = v1_f.powi(*k2 as i32);
    let r2_k2 = v2_f.powi(*k2 as i32);

    let det = r1_k1 * r2_k2 - r2_k1 * r1_k2;
    if det.abs() < 1e-12 {
        return Value::Call {
            head: "RSolve".to_string(),
            args: vec![],
        };
    }

    // Cramer's rule:
    let c1_num = a1_d * r2_k2 - a2_d * r2_k1;
    let c2_num = r1_k1 * a2_d - r1_k2 * a1_d;
    let c1 = c1_num / det;
    let c2 = c2_num / det;

    let c1_val = if (c1 - c1.round()).abs() < 1e-9 {
        Value::Integer(Integer::from(c1.round() as i64))
    } else {
        Value::Real(Float::with_val(DEFAULT_PRECISION, c1))
    };
    let c2_val = if (c2 - c2.round()).abs() < 1e-9 {
        Value::Integer(Integer::from(c2.round() as i64))
    } else {
        Value::Real(Float::with_val(DEFAULT_PRECISION, c2))
    };

    let term1 = simplify_times(vec![c1_val, power_expr(r1, "n")]);
    let term2 = simplify_times(vec![c2_val, power_expr(r2, "n")]);

    Value::Call {
        head: "Plus".to_string(),
        args: vec![term1, term2],
    }
}

fn build_equal_root_solution(init: &[(i64, Value)], r: &Value, v: &Float) -> Value {
    let (k1, a1_val) = &init[0];
    let (k2, a2_val) = &init[1];
    let a1_f = eval_numeric(a1_val);
    let a2_f = eval_numeric(a2_val);

    // Use f64 for arithmetic
    let v_f = v.to_f64();
    let a1_d = a1_f.to_f64();
    let a2_d = a2_f.to_f64();

    // a[n] = (c1 + c2*n) * r^n
    // a[k1] = (c1 + c2*k1) * r^k1 = a1
    // a[k2] = (c1 + c2*k2) * r^k2 = a2
    let r_k1 = v_f.powi(*k1 as i32);
    let r_k2 = v_f.powi(*k2 as i32);

    // c1 + c2*k1 = a1/r^k1
    // c1 + c2*k2 = a2/r^k2
    let rhs1 = a1_d / r_k1;
    let rhs2 = a2_d / r_k2;

    // c2 = (rhs2 - rhs1) / (k2 - k1)
    let k_diff = (*k2 - *k1) as f64;
    let c2 = if k_diff.abs() > 1e-12 {
        (rhs2 - rhs1) / k_diff
    } else {
        0.0
    };
    let c1 = rhs1 - c2 * (*k1 as f64);

    let c1_val = if (c1 - c1.round()).abs() < 1e-9 {
        Value::Integer(Integer::from(c1.round() as i64))
    } else {
        Value::Real(Float::with_val(DEFAULT_PRECISION, c1))
    };
    let c2_val = if (c2 - c2.round()).abs() < 1e-9 {
        Value::Integer(Integer::from(c2.round() as i64))
    } else {
        Value::Real(Float::with_val(DEFAULT_PRECISION, c2))
    };

    // (c1 + c2*n) * r^n
    let inner = if c2_val == Value::Integer(Integer::from(0)) {
        c1_val
    } else {
        Value::Call {
            head: "Plus".to_string(),
            args: vec![
                c1_val,
                simplify_times(vec![c2_val, Value::Symbol("n".to_string())]),
            ],
        }
    };

    simplify_times(vec![inner, power_expr(r, "n")])
}

/// Build Power[root, n] expression.
fn power_expr(base: &Value, var: &str) -> Value {
    Value::Call {
        head: "Power".to_string(),
        args: vec![base.clone(), Value::Symbol(var.to_string())],
    }
}

/// Simplify a Times call by removing 1s and collapsing single-element lists.
fn simplify_times(mut args: Vec<Value>) -> Value {
    let mut negative = false;
    args.retain(|a| {
        if *a == Value::Integer(Integer::from(-1)) {
            negative = !negative;
            false
        } else {
            *a != Value::Integer(Integer::from(1))
        }
    });
    let result = match args.len() {
        0 => Value::Integer(Integer::from(if negative { -1 } else { 1 })),
        1 => args.into_iter().next().unwrap(),
        _ => Value::Call {
            head: "Times".to_string(),
            args,
        },
    };
    if negative {
        Value::Call {
            head: "Times".to_string(),
            args: vec![Value::Integer(Integer::from(-1)), result],
        }
    } else {
        result
    }
}

/// RecurrenceTable stub — handled by evaluator as a special form.
pub fn builtin_recurrence_table(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "RecurrenceTable should be handled by evaluator".to_string(),
    ))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn real(f: f64) -> Value {
        Value::Real(Float::with_val(DEFAULT_PRECISION, f))
    }

    fn list(items: Vec<Value>) -> Value {
        Value::List(items)
    }

    // ── DiscreteDelta ──

    #[test]
    fn test_discrete_delta_all_zero() {
        assert_eq!(builtin_discrete_delta(&[int(0), int(0)]).unwrap(), int(1));
    }

    #[test]
    fn test_discrete_delta_nonzero() {
        assert_eq!(builtin_discrete_delta(&[int(0), int(1)]).unwrap(), int(0));
    }

    #[test]
    fn test_discrete_delta_symbolic() {
        let result = builtin_discrete_delta(&[Value::Symbol("x".to_string()), int(0)]).unwrap();
        match result {
            Value::Call { head, .. } => assert_eq!(head, "DiscreteDelta"),
            _ => panic!("Expected symbolic Call"),
        }
    }

    #[test]
    fn test_discrete_delta_empty_error() {
        assert!(builtin_discrete_delta(&[]).is_err());
    }

    // ── DiscreteShift ──

    #[test]
    fn test_discrete_shift_symbolic() {
        let result = builtin_discrete_shift(&[
            Value::Symbol("f".to_string()),
            Value::Symbol("n".to_string()),
        ])
        .unwrap();
        match result {
            Value::Call { head, .. } => assert_eq!(head, "DiscreteShift"),
            _ => panic!("Expected symbolic Call"),
        }
    }

    #[test]
    fn test_discrete_shift_bad_args() {
        assert!(builtin_discrete_shift(&[Value::Symbol("f".to_string())]).is_err());
    }

    // ── DiscreteRatio ──

    #[test]
    fn test_discrete_ratio_symbolic() {
        let result = builtin_discrete_ratio(&[
            Value::Symbol("f".to_string()),
            Value::Symbol("n".to_string()),
        ])
        .unwrap();
        match result {
            Value::Call { head, .. } => assert_eq!(head, "DiscreteRatio"),
            _ => panic!("Expected symbolic Call"),
        }
    }

    // ── FactorialPower ──

    #[test]
    fn test_factorial_power_basic() {
        assert_eq!(
            builtin_factorial_power(&[int(10), int(3)]).unwrap(),
            int(720)
        );
    }

    #[test]
    fn test_factorial_power_zero() {
        assert_eq!(builtin_factorial_power(&[int(10), int(0)]).unwrap(), int(1));
    }

    #[test]
    fn test_factorial_power_step() {
        // 10 * 8 * 6 = 480
        assert_eq!(
            builtin_factorial_power(&[int(10), int(3), int(2)]).unwrap(),
            int(480)
        );
    }

    #[test]
    fn test_factorial_power_real() {
        let result = builtin_factorial_power(&[real(5.0), int(3)]).unwrap();
        // 5 * 4 * 3 = 60
        match result {
            Value::Real(r) => assert!((r.to_f64() - 60.0).abs() < 1e-10),
            _ => panic!("Expected Real"),
        }
    }

    #[test]
    fn test_factorial_power_symbolic() {
        let result = builtin_factorial_power(&[Value::Symbol("x".to_string()), int(3)]).unwrap();
        match result {
            Value::Call { head, .. } => assert_eq!(head, "FactorialPower"),
            _ => panic!("Expected symbolic Call"),
        }
    }

    // ── BernoulliB ──

    #[test]
    fn test_bernoulli_b_0() {
        assert_eq!(builtin_bernoulli_b(&[int(0)]).unwrap(), int(1));
    }

    #[test]
    fn test_bernoulli_b_1() {
        let result = builtin_bernoulli_b(&[int(1)]).unwrap();
        match result {
            Value::Call { head, .. } => assert_eq!(head, "Divide"),
            _ => panic!("Expected Divide call for B_1 = -1/2"),
        }
    }

    #[test]
    fn test_bernoulli_b_odd() {
        assert_eq!(builtin_bernoulli_b(&[int(3)]).unwrap(), int(0));
        assert_eq!(builtin_bernoulli_b(&[int(5)]).unwrap(), int(0));
    }

    #[test]
    fn test_bernoulli_b_2() {
        let result = builtin_bernoulli_b(&[int(2)]).unwrap();
        match result {
            Value::Real(r) => assert!((r.to_f64() - 1.0 / 6.0).abs() < 1e-10),
            _ => panic!("Expected Real"),
        }
    }

    #[test]
    fn test_bernoulli_b_4() {
        let result = builtin_bernoulli_b(&[int(4)]).unwrap();
        match result {
            Value::Real(r) => assert!((r.to_f64() - (-1.0 / 30.0)).abs() < 1e-10),
            _ => panic!("Expected Real"),
        }
    }

    // ── LinearRecurrence ──

    #[test]
    fn test_linear_recurrence_fib() {
        // Fibonacci: kernel={1,1}, init={0,1}
        assert_eq!(
            builtin_linear_recurrence(&[
                list(vec![int(1), int(1)]),
                list(vec![int(0), int(1)]),
                int(6)
            ])
            .unwrap(),
            int(5)
        );
    }

    #[test]
    fn test_linear_recurrence_within_init() {
        let result = builtin_linear_recurrence(&[
            list(vec![int(1), int(1)]),
            list(vec![int(0), int(1)]),
            int(1),
        ])
        .unwrap();
        assert_eq!(result, int(0));
    }

    #[test]
    fn test_linear_recurrence_geometric() {
        // a[n] = 2 * a[n-1], init={1}
        assert_eq!(
            builtin_linear_recurrence(&[list(vec![int(2)]), list(vec![int(1)]), int(4)]).unwrap(),
            int(8)
        );
    }

    #[test]
    fn test_linear_recurrence_symbolic() {
        let result = builtin_linear_recurrence(&[
            Value::Symbol("x".to_string()),
            list(vec![int(1)]),
            int(3),
        ])
        .unwrap();
        match result {
            Value::Call { head, .. } => assert_eq!(head, "LinearRecurrence"),
            _ => panic!("Expected symbolic Call"),
        }
    }

    // ── RSolve ──

    fn call_syma(head: &str, args: Vec<Value>) -> Value {
        Value::Call {
            head: head.to_string(),
            args,
        }
    }

    fn sym(s: &str) -> Value {
        Value::Symbol(s.to_string())
    }

    #[test]
    fn test_rsolve_parse_helpers() {
        // Test parse_func_name_offset
        let a_n = call_syma("a", vec![sym("n")]);
        let result = parse_func_name_offset(&a_n, "n");
        assert_eq!(result, Some(("a".to_string(), 0)));

        // Test parse_func_name_offset with offset (returns raw offset, negated by parse_term)
        let a_n_minus_1 = call_syma("a", vec![call_syma("Plus", vec![sym("n"), int(-1)])]);
        let result = parse_func_name_offset(&a_n_minus_1, "n");
        assert_eq!(result, Some(("a".to_string(), -1)));

        // Test parse_term for Times (negates offset to get order)
        let times_term = call_syma(
            "Times",
            vec![
                int(2),
                call_syma("a", vec![call_syma("Plus", vec![sym("n"), int(-1)])]),
            ],
        );
        let result = parse_term(&times_term, "n");
        assert_eq!(
            result,
            Some(("a".to_string(), 1, Value::Integer(Integer::from(2))))
        );

        // Test parse_init_condition
        let ic = call_syma("Equal", vec![call_syma("a", vec![int(0)]), int(3)]);
        let result = parse_init_condition(&ic);
        assert_eq!(result, Some((0, int(3))));
    }

    #[test]
    fn test_rsolve_first_order() {
        // RSolve[{a[n] == 2*a[n-1], a[0] == 3}, a, {n, 0, 10}]
        // Expected: a[n] = 3 * 2^n
        let eq = call_syma(
            "Equal",
            vec![
                call_syma("a", vec![sym("n")]),
                call_syma(
                    "Times",
                    vec![
                        int(2),
                        call_syma("a", vec![call_syma("Plus", vec![sym("n"), int(-1)])]),
                    ],
                ),
            ],
        );
        let ic = call_syma("Equal", vec![call_syma("a", vec![int(0)]), int(3)]);
        let result = builtin_rsolve(
            &[
                list(vec![eq, ic]),
                sym("a"),
                list(vec![sym("n"), int(0), int(10)]),
            ],
            &crate::env::Env::new(),
        )
        .unwrap();

        // Should return a List containing a List containing a Rule
        match result {
            Value::List(outer) => {
                assert_eq!(outer.len(), 1);
                if let Value::List(inner) = &outer[0] {
                    assert_eq!(inner.len(), 1);
                    if let Value::Rule { lhs, .. } = &inner[0] {
                        // lhs should be a[n]
                        if let Value::Call { head, args } = &**lhs {
                            assert_eq!(head, "a");
                            assert_eq!(args.len(), 1);
                        } else {
                            panic!("Expected Call as lhs of rule");
                        }
                    } else {
                        panic!("Expected Rule");
                    }
                } else {
                    panic!("Expected inner List");
                }
            }
            _ => panic!("Expected outer List, got {:?}", result),
        }
    }

    #[test]
    fn test_rsolve_second_order() {
        // RSolve[{a[n] == 5*a[n-1] - 6*a[n-2], a[0] == 1, a[1] == 4}, a, {n}]
        // Expected: a[n] = -2^n + 2*3^n
        let term1 = call_syma(
            "Times",
            vec![
                int(5),
                call_syma("a", vec![call_syma("Plus", vec![sym("n"), int(-1)])]),
            ],
        );
        let term2 = call_syma(
            "Times",
            vec![
                int(-6),
                call_syma("a", vec![call_syma("Plus", vec![sym("n"), int(-2)])]),
            ],
        );
        let eq = call_syma(
            "Equal",
            vec![
                call_syma("a", vec![sym("n")]),
                call_syma("Plus", vec![term1, term2]),
            ],
        );
        let ic1 = call_syma("Equal", vec![call_syma("a", vec![int(0)]), int(1)]);
        let ic2 = call_syma("Equal", vec![call_syma("a", vec![int(1)]), int(4)]);
        let result = builtin_rsolve(
            &[
                list(vec![eq, ic1, ic2]),
                sym("a"),
                list(vec![sym("n"), int(0), int(20)]),
            ],
            &crate::env::Env::new(),
        )
        .unwrap();

        // Should return a List containing a List containing a Rule
        match result {
            Value::List(outer) => {
                assert_eq!(outer.len(), 1);
                if let Value::List(inner) = &outer[0] {
                    assert_eq!(inner.len(), 1);
                    if let Value::Rule { lhs, .. } = &inner[0] {
                        if let Value::Call { head, args } = &**lhs {
                            assert_eq!(head, "a");
                            assert_eq!(args.len(), 1);
                        } else {
                            panic!("Expected Call as lhs of rule");
                        }
                    } else {
                        panic!("Expected Rule");
                    }
                } else {
                    panic!("Expected inner List");
                }
            }
            _ => panic!("Expected outer List, got {:?}", result),
        }
    }

    #[test]
    fn test_rsolve_bad_args_returns_unevaluated() {
        // RSolve with wrong number of args should return unevaluated
        let result = builtin_rsolve(&[sym("a"), sym("n")], &crate::env::Env::new()).unwrap_err();
        assert!(matches!(result, EvalError::Error(_)));
    }

    #[test]
    fn test_rsolve_non_linear_returns_unevaluated() {
        // RSolve with non-linear recurrence should return unevaluated
        let eq = call_syma(
            "Equal",
            vec![
                call_syma("a", vec![sym("n")]),
                call_syma(
                    "Power",
                    vec![
                        call_syma("a", vec![call_syma("Plus", vec![sym("n"), int(-1)])]),
                        int(2),
                    ],
                ),
            ],
        );
        let ic = call_syma("Equal", vec![call_syma("a", vec![int(0)]), int(1)]);
        let result = builtin_rsolve(
            &[
                list(vec![eq, ic]),
                sym("a"),
                list(vec![sym("n"), int(0), int(10)]),
            ],
            &crate::env::Env::new(),
        )
        .unwrap();

        // Should return unevaluated Call
        match result {
            Value::Call { head, .. } => assert_eq!(head, "RSolve"),
            _ => panic!("Expected RSolve Call, got {:?}", result),
        }
    }
}
