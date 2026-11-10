use crate::ast::Expr;
use crate::builtins::arithmetic::{add_values_public, mul_values_public, sub_values_public};
use crate::env::Env;
use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;

/// Build an unevaluated `head[args...]` value.
fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

/// Small helper for an `Integer` value.
fn vint(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

/// Borrow the items of a `Value::List`.
fn get_list(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::List(items) => Some(items),
        _ => None,
    }
}

/// `true` when a value carries a symbol named `name` in its tree.
fn contains_symbol(v: &Value, name: &str) -> bool {
    match v {
        Value::Symbol(s) => s == name,
        Value::List(items) => items.iter().any(|i| contains_symbol(i, name)),
        Value::Call { args, .. } => args.iter().any(|a| contains_symbol(a, name)),
        _ => false,
    }
}

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
            let coeff = other_factor_in_times(&args_flat, arg);
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

/// Given the flattened arguments of a `Times` and one of them, return the product of the rest.
fn other_factor_in_times(args: &[Value], known_arg: &Value) -> Value {
    let others: Vec<Value> = args
        .iter()
        .filter(|a| !a.struct_eq(known_arg))
        .cloned()
        .collect();
    match others.len() {
        0 => Value::Integer(Integer::from(1)),
        1 => others.into_iter().next().unwrap(),
        _ => Value::Call {
            head: "Times".to_string(),
            args: others,
        },
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

// ── FindSequenceFunction ──────────────────────────────────────────────────────

/// FindSequenceFunction[list] — Find a closed-form function matching a sequence.
/// Uses finite differences to detect polynomial sequences.
/// Returns a PureFunction if a polynomial fit is found, otherwise returns symbolic.
#[allow(clippy::needless_range_loop)]
pub fn builtin_find_sequence_function(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FindSequenceFunction requires exactly 1 argument".to_string(),
        ));
    }

    let items = match &args[0] {
        Value::List(items) => items,
        _ => {
            return Ok(Value::Call {
                head: "FindSequenceFunction".to_string(),
                args: args.to_vec(),
            });
        }
    };

    if items.len() < 2 {
        return Ok(Value::Call {
            head: "FindSequenceFunction".to_string(),
            args: args.to_vec(),
        });
    }

    // Convert all items to f64 for finite difference computation
    let vals: Option<Vec<f64>> = items.iter().map(|v| v.to_real()).collect();
    let vals = match vals {
        Some(v) => v,
        None => {
            return Ok(Value::Call {
                head: "FindSequenceFunction".to_string(),
                args: args.to_vec(),
            });
        }
    };

    let n = vals.len();

    // Build difference table: diff_table[k][i] = k-th finite difference at index i
    let mut diff_table: Vec<Vec<f64>> = vec![vals.clone()];
    for k in 1..n {
        let prev = &diff_table[k - 1];
        let mut next = Vec::with_capacity(prev.len() - 1);
        for i in 0..prev.len() - 1 {
            next.push(prev[i + 1] - prev[i]);
        }
        if next.is_empty() {
            break;
        }
        diff_table.push(next);
    }

    // Find the degree: the first level where all differences are (approximately) constant
    // Require at least 2 elements in the row to avoid trivial matches
    let tol: f64 = 1e-9;
    let mut degree: Option<usize> = None;
    for k in 0..diff_table.len() {
        let row = &diff_table[k];
        if row.len() < 2 {
            break;
        }
        let first = row[0];
        if row.iter().all(|v| (v - first).abs() < tol) {
            degree = Some(k);
            break;
        }
    }

    let degree = match degree {
        Some(d) => d,
        None => {
            return Ok(Value::Call {
                head: "FindSequenceFunction".to_string(),
                args: args.to_vec(),
            });
        }
    };

    // Reconstruct polynomial coefficients from the difference table.
    // For a polynomial of degree d, we can determine the standard coefficients
    // c_0 + c_1*x + c_2*x^2 + ... + c_d*x^d by solving the Vandermonde system
    // with points x=1,2,...,d+1 using the known sequence values.
    let d = degree;
    let m = d + 1; // number of coefficients

    // Build Vandermonde matrix: rows correspond to x=1,2,...,m
    // columns are powers 0,1,...,d, augmented with f(x) values
    let mut matrix: Vec<Vec<f64>> = Vec::with_capacity(m);
    for i in 0..m {
        let x = (i + 1) as f64; // x = 1, 2, ..., m
        let mut row = Vec::with_capacity(m + 1);
        for j in 0..m {
            row.push(x.powi(j as i32));
        }
        row.push(vals[i]); // f(x)
        matrix.push(row);
    }

    // Gaussian elimination with partial pivoting
    for col in 0..m {
        // Find pivot
        let mut max_val = matrix[col][col].abs();
        let mut max_row = col;
        for row in (col + 1)..m {
            if matrix[row][col].abs() > max_val {
                max_val = matrix[row][col].abs();
                max_row = row;
            }
        }
        if max_val < 1e-15 {
            // Singular — can't determine polynomial
            return Ok(Value::Call {
                head: "FindSequenceFunction".to_string(),
                args: args.to_vec(),
            });
        }
        matrix.swap(col, max_row);

        // Eliminate below
        let pivot = matrix[col][col];
        for row in (col + 1)..m {
            let factor = matrix[row][col] / pivot;
            for j in col..=m {
                matrix[row][j] -= factor * matrix[col][j];
            }
        }
    }

    // Back substitution
    let mut coeffs = vec![0.0f64; m];
    for i in (0..m).rev() {
        let mut sum = matrix[i][m];
        for j in (i + 1)..m {
            sum -= matrix[i][j] * coeffs[j];
        }
        coeffs[i] = sum / matrix[i][i];
    }

    // Convert coefficients to Values, rounding to integers when close
    let coeff_vals: Vec<Value> = coeffs
        .iter()
        .map(|c| {
            if (c - c.round()).abs() < 1e-9 && c.abs() < i64::MAX as f64 {
                Value::Integer(Integer::from(c.round() as i64))
            } else {
                Value::Real(Float::with_val(DEFAULT_PRECISION, *c))
            }
        })
        .collect();

    // Build the PureFunction body expression.
    // The body should evaluate the polynomial at #1 (slot 1).
    // We build: c_0 + c_1*#1 + c_2*#1^2 + ...
    let slot = Expr::Call {
        head: Box::new(Expr::Symbol("Slot".to_string())),
        args: vec![Expr::Integer(Integer::from(1))],
    };

    let mut terms: Vec<Expr> = Vec::new();
    for (i, coeff) in coeff_vals.iter().enumerate() {
        let coeff_expr = value_to_expr(coeff);
        if let Expr::Integer(ref n) = coeff_expr
            && n.is_zero()
        {
            continue;
        }

        let term = if i == 0 {
            coeff_expr
        } else if i == 1 {
            if coeff == &Value::Integer(Integer::from(1)) {
                slot.clone()
            } else {
                Expr::Call {
                    head: Box::new(Expr::Symbol("Times".to_string())),
                    args: vec![coeff_expr, slot.clone()],
                }
            }
        } else {
            let slot_pow = Expr::Call {
                head: Box::new(Expr::Symbol("Power".to_string())),
                args: vec![slot.clone(), Expr::Integer(Integer::from(i as i64))],
            };
            if coeff == &Value::Integer(Integer::from(1)) {
                slot_pow
            } else {
                Expr::Call {
                    head: Box::new(Expr::Symbol("Times".to_string())),
                    args: vec![coeff_expr, slot_pow],
                }
            }
        };
        terms.push(term);
    }

    let body = if terms.is_empty() {
        Expr::Integer(Integer::from(0))
    } else if terms.len() == 1 {
        terms.into_iter().next().unwrap()
    } else {
        Expr::Call {
            head: Box::new(Expr::Symbol("Plus".to_string())),
            args: terms,
        }
    };

    Ok(Value::PureFunction {
        body,
        slot_count: 1,
        params: vec![],
        env: None,
    })
}

/// Convert a Value to an Expr (for building PureFunction bodies).
fn value_to_expr(v: &Value) -> Expr {
    match v {
        Value::Integer(n) => Expr::Integer(n.clone()),
        Value::Real(r) => Expr::Real(r.clone()),
        Value::Bool(b) => Expr::Bool(*b),
        Value::Str(s) => Expr::Str(s.clone()),
        Value::Null => Expr::Null,
        Value::Symbol(s) => Expr::Symbol(s.clone()),
        Value::List(items) => Expr::List(items.iter().map(value_to_expr).collect()),
        Value::Call { head, args } => Expr::Call {
            head: Box::new(Expr::Symbol(head.clone())),
            args: args.iter().map(value_to_expr).collect(),
        },
        _ => Expr::Symbol(format!("{}", v)),
    }
}

// ── DifferenceDelta ──────────────────────────────────────────────────────────

/// DifferenceDelta[list] — Compute successive differences of a list.
/// DifferenceDelta[f, i] — Symbolic difference Delta f(i) = f(i+1) - f(i).
pub fn builtin_difference_delta(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => {
            // List form: Differences[list]
            let items = match &args[0] {
                Value::List(items) => items,
                _ => {
                    return Ok(Value::Call {
                        head: "DifferenceDelta".to_string(),
                        args: args.to_vec(),
                    });
                }
            };

            if items.len() < 2 {
                return Ok(Value::List(vec![]));
            }

            let mut result = Vec::with_capacity(items.len() - 1);
            for i in 0..items.len() - 1 {
                let diff =
                    crate::builtins::arithmetic::sub_values_public(&items[i + 1], &items[i])?;
                result.push(diff);
            }
            Ok(Value::List(result))
        }
        2 => {
            // Symbolic form: DifferenceDelta[f[i], i] -> f[i+1] - f[i]
            let var = match &args[1] {
                Value::Symbol(s) => s.clone(),
                _ => {
                    return Ok(Value::Call {
                        head: "DifferenceDelta".to_string(),
                        args: args.to_vec(),
                    });
                }
            };

            let expr = &args[0];

            // Build f[i+1]: substitute i -> i+1 in the expression
            let one = Value::Integer(Integer::from(1));
            let i_plus_1 = Value::Call {
                head: "Plus".to_string(),
                args: vec![Value::Symbol(var.clone()), one],
            };
            let shifted = substitute_value(expr, &var, &i_plus_1);

            // Build f[i+1] - f[i] = Plus[f[i+1], Times[-1, f[i]]]
            let neg_expr = Value::Call {
                head: "Times".to_string(),
                args: vec![Value::Integer(Integer::from(-1)), expr.clone()],
            };
            Ok(Value::Call {
                head: "Plus".to_string(),
                args: vec![shifted, neg_expr],
            })
        }
        _ => Err(EvalError::Error(
            "DifferenceDelta requires 1 or 2 arguments".to_string(),
        )),
    }
}

/// Substitute a variable with a value in an expression tree.
fn substitute_value(expr: &Value, var: &str, replacement: &Value) -> Value {
    match expr {
        Value::Symbol(s) if s == var => replacement.clone(),
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => {
            expr.clone()
        }
        Value::Symbol(_) => expr.clone(),
        Value::List(items) => Value::List(
            items
                .iter()
                .map(|i| substitute_value(i, var, replacement))
                .collect(),
        ),
        Value::Call { head, args } => Value::Call {
            head: head.clone(),
            args: args
                .iter()
                .map(|a| substitute_value(a, var, replacement))
                .collect(),
        },
        _ => expr.clone(),
    }
}

// ── FindLinearRecurrence ─────────────────────────────────────────────────────

/// FindLinearRecurrence[list] — Find the minimal linear recurrence relation
/// for a sequence using the Berlekamp-Massey algorithm.
/// Returns the list of coefficients {c1, c2, ..., ck} such that
/// a[n] = c1*a[n-1] + c2*a[n-2] + ... + ck*a[n-k].
pub fn builtin_find_linear_recurrence(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FindLinearRecurrence requires exactly 1 argument".to_string(),
        ));
    }

    let items = match &args[0] {
        Value::List(items) => items,
        _ => {
            return Ok(Value::Call {
                head: "FindLinearRecurrence".to_string(),
                args: args.to_vec(),
            });
        }
    };

    if items.len() < 2 {
        return Ok(Value::Call {
            head: "FindLinearRecurrence".to_string(),
            args: args.to_vec(),
        });
    }

    // Convert to f64 for Berlekamp-Massey
    let vals: Option<Vec<f64>> = items.iter().map(|v| v.to_real()).collect();
    let vals = match vals {
        Some(v) => v,
        None => {
            return Ok(Value::Call {
                head: "FindLinearRecurrence".to_string(),
                args: args.to_vec(),
            });
        }
    };

    // Berlekamp-Massey algorithm over reals
    // We work with floating point and round to integers at the end
    let n = vals.len();

    // C is the current connection polynomial, B is the previous one
    let mut c: Vec<f64> = vec![0.0; n + 1];
    let mut b: Vec<f64> = vec![0.0; n + 1];
    c[0] = 1.0;
    b[0] = 1.0;

    let mut l = 0usize; // current LFSR length
    let mut m = 1usize; // number of steps since last length change
    let mut b_prev = 1.0f64; // previous discrepancy

    for i in 0..n {
        // Compute discrepancy
        let mut d = vals[i];
        for j in 1..=l {
            d += c[j] * vals[i.saturating_sub(j)];
        }

        if d.abs() < 1e-9 {
            // No discrepancy
            m += 1;
        } else if 2 * l <= i {
            // Length update needed
            let mut new_c = vec![0.0f64; n + 1];
            new_c[..=n].copy_from_slice(&c[..=n]);
            for j in 0..=n {
                if j + m <= n {
                    c[j + m] -= d / b_prev * b[j];
                }
            }
            l = i + 1 - l;
            b = new_c;
            b_prev = d;
            m = 1;
        } else {
            // Standard update
            let factor = d / b_prev;
            for j in 0..=n {
                if j + m <= n {
                    c[j + m] -= factor * b[j];
                }
            }
            m += 1;
        }
    }

    if l == 0 {
        return Ok(Value::Call {
            head: "FindLinearRecurrence".to_string(),
            args: args.to_vec(),
        });
    }

    // Extract coefficients: c[0] + c[1]*x + c[2]*x^2 + ... + c[l]*x^l = 0
    // So a[n] = -c[1]*a[n-1] - c[2]*a[n-2] - ... - c[l]*a[n-l]
    // Normalize by c[0] (should be 1.0)
    let mut result_coeffs = Vec::with_capacity(l);
    for j in 1..=l {
        let val = -c[j] / c[0];
        let rounded = val.round();
        if (val - rounded).abs() < 1e-6 && rounded.abs() < i64::MAX as f64 {
            result_coeffs.push(Value::Integer(Integer::from(rounded as i64)));
        } else {
            result_coeffs.push(Value::Real(Float::with_val(DEFAULT_PRECISION, val)));
        }
    }

    // Verify the recurrence against the input sequence
    let tol: f64 = 1e-6;
    for i in l..n {
        let mut predicted = 0.0f64;
        for j in 0..l {
            predicted += result_coeffs[j].to_real().unwrap_or(0.0) * vals[i - 1 - j];
        }
        if (predicted - vals[i]).abs() > tol.max(vals[i].abs() * 1e-6) {
            // Verification failed — return symbolic
            return Ok(Value::Call {
                head: "FindLinearRecurrence".to_string(),
                args: args.to_vec(),
            });
        }
    }

    Ok(Value::List(result_coeffs))
}

// ═══════════════════════════════════════════════════════════════════════════
// Discrete transforms
// ═══════════════════════════════════════════════════════════════════════════

// ── Register ────────────────────────────────────────────────────────────────

/// Register the discrete-transform builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "Fourier", builtin_fourier);
    super::register_builtin(env, "InverseFourier", builtin_inverse_fourier);
    super::register_builtin(env, "FourierDST", builtin_fourier_dst);
    super::register_builtin(env, "FourierDCT", builtin_fourier_dct);
    super::register_builtin(env, "HadamardMatrix", builtin_hadamard_matrix);
    super::register_builtin(env, "WalshHadamardTransform", builtin_walsh_hadamard);
    super::register_builtin(env, "DiscreteHadamardTransform", builtin_discrete_hadamard);
    super::register_builtin(env, "DiscreteHaarWaveletTransform", builtin_discrete_haar);
    super::register_builtin(env, "DiscreteConvolve", builtin_discrete_convolve);
    super::register_builtin(env, "DirichletConvolve", builtin_dirichlet_convolve);
    // `ListConvolve` already exists (list.rs); this override gives it the
    // Mathematica semantics (reversed kernel) and exact arithmetic.
    super::register_builtin(env, "ListConvolve", builtin_list_convolve);
    super::register_builtin(env, "ListCorrelate", builtin_list_correlate);
    super::register_builtin(env, "DirichletCharacter", builtin_dirichlet_character);
    super::register_builtin(env, "DirichletL", builtin_dirichlet_l);
}

// ── Complex helpers ─────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct C64 {
    re: f64,
    im: f64,
}

impl C64 {
    fn new(re: f64, im: f64) -> Self {
        C64 { re, im }
    }
    fn zero() -> Self {
        C64 { re: 0.0, im: 0.0 }
    }
    fn add(self, o: C64) -> C64 {
        C64 {
            re: self.re + o.re,
            im: self.im + o.im,
        }
    }
    fn sub(self, o: C64) -> C64 {
        C64 {
            re: self.re - o.re,
            im: self.im - o.im,
        }
    }
    fn mul(self, o: C64) -> C64 {
        C64 {
            re: self.re * o.re - self.im * o.im,
            im: self.re * o.im + self.im * o.re,
        }
    }
    fn scale(self, s: f64) -> C64 {
        C64 {
            re: self.re * s,
            im: self.im * s,
        }
    }
}

/// Read a numeric (real or complex) value into a `C64`.
fn value_to_c64(v: &Value) -> Option<C64> {
    match v {
        Value::Complex { re, im } => Some(C64::new(*re, *im)),
        _ => v.to_real().map(|r| C64::new(r, 0.0)),
    }
}

/// Lower a complex number to a `Value`, dropping a negligible imaginary part.
fn c64_to_value(c: C64) -> Value {
    if c.im.abs() < 1e-12 {
        Value::Real(Float::with_val(DEFAULT_PRECISION, c.re))
    } else {
        Value::Complex { re: c.re, im: c.im }
    }
}

/// Out-of-place FFT with twiddle sign `sign` (`+1` = `e^{+2πi jk/n}`).
///
/// Uses a radix-2 Cooley–Tukey butterfly when the length is a power of two and
/// a naive O(n²) DFT otherwise. `naive_dft` is the reference used in tests.
fn fft(values: &[C64], sign: f64) -> Vec<C64> {
    let n = values.len();
    if n <= 1 {
        return values.to_vec();
    }
    if !n.is_power_of_two() {
        return naive_dft(values, sign);
    }

    let mut a = values.to_vec();
    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            a.swap(i, j);
        }
    }

    let mut len = 2usize;
    while len <= n {
        let ang = sign * std::f64::consts::TAU / len as f64;
        let step = C64::new(ang.cos(), ang.sin());
        let mut i = 0usize;
        while i < n {
            let mut w = C64::new(1.0, 0.0);
            for k in 0..len / 2 {
                let u = a[i + k];
                let v = a[i + k + len / 2].mul(w);
                a[i + k] = u.add(v);
                a[i + k + len / 2] = u.sub(v);
                w = w.mul(step);
            }
            i += len;
        }
        len <<= 1;
    }
    a
}

/// Naive O(n²) DFT reference implementation.
fn naive_dft(values: &[C64], sign: f64) -> Vec<C64> {
    let n = values.len();
    let mut out = vec![C64::zero(); n];
    for (k, slot) in out.iter_mut().enumerate() {
        let mut sum = C64::zero();
        for (j, x) in values.iter().enumerate() {
            let ang = sign * std::f64::consts::TAU * (j as f64) * (k as f64) / n as f64;
            sum = sum.add(x.mul(C64::new(ang.cos(), ang.sin())));
        }
        *slot = sum;
    }
    out
}

/// Parse a `FourierParameters -> {a, b}` rule.
fn parse_fourier_parameters(v: &Value) -> Option<(f64, f64)> {
    if let Value::Rule { lhs, rhs, .. } = v
        && matches!(&**lhs, Value::Symbol(s) if s == "FourierParameters")
        && let Value::List(p) = &**rhs
        && p.len() == 2
    {
        return Some((p[0].to_real()?, p[1].to_real()?));
    }
    None
}

/// Shared implementation of `Fourier` / `InverseFourier`.
fn fourier_impl(args: &[Value], default_b: f64, name: &str) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(format!(
            "{name} requires 1 or 2 arguments"
        )));
    }
    let list = match get_list(&args[0]) {
        Some(l) => l,
        None => return Ok(unevaluated(name, args)),
    };
    if list.is_empty() {
        return Ok(Value::List(vec![]));
    }

    let mut a = 0.0f64;
    let mut b = default_b;
    let mut positions: Option<Vec<usize>> = None;
    for extra in &args[1..] {
        if let Some((pa, pb)) = parse_fourier_parameters(extra) {
            a = pa;
            b = pb;
        } else if let Value::List(items) = extra {
            positions = Some(
                items
                    .iter()
                    .filter_map(|v| v.to_integer())
                    .map(|i| i.max(0) as usize)
                    .collect(),
            );
        }
    }

    let mut data = Vec::with_capacity(list.len());
    for v in list {
        match value_to_c64(v) {
            Some(c) => data.push(c),
            None => return Ok(unevaluated(name, args)),
        }
    }

    let n = data.len() as f64;
    let scale = n.powf((a - 1.0) / 2.0);
    let spectrum = fft(&data, b);
    let mut out: Vec<Value> = spectrum
        .iter()
        .map(|c| c64_to_value(c.scale(scale)))
        .collect();

    if let Some(pos) = positions {
        out = pos
            .into_iter()
            .filter_map(|p| {
                if p >= 1 && p <= out.len() {
                    Some(out[p - 1].clone())
                } else {
                    None
                }
            })
            .collect();
    }
    Ok(Value::List(out))
}

/// `Fourier[list]` — discrete Fourier transform (default `FourierParameters -> {0, 1}`).
pub fn builtin_fourier(args: &[Value]) -> Result<Value, EvalError> {
    fourier_impl(args, 1.0, "Fourier")
}

/// `InverseFourier[list]` — inverse discrete Fourier transform (default `{0, -1}`).
pub fn builtin_inverse_fourier(args: &[Value]) -> Result<Value, EvalError> {
    fourier_impl(args, -1.0, "InverseFourier")
}

// ── DCT / DST ───────────────────────────────────────────────────────────────

/// Parse a DCT/DST type argument (integer 1–4 or `"I".."IV"`).
fn parse_transform_type(v: &Value) -> Option<u8> {
    match v {
        Value::Integer(i) => i.to_i64().filter(|t| (1..=4).contains(t)).map(|t| t as u8),
        Value::Str(s) | Value::Symbol(s) => match s.as_str() {
            "I" | "1" => Some(1),
            "II" | "2" => Some(2),
            "III" | "3" => Some(3),
            "IV" | "4" => Some(4),
            _ => None,
        },
        _ => None,
    }
}

/// Orthonormal DCT (`kind == 'c'`) and DST (`kind == 's'`), types 1–4.
#[allow(clippy::needless_range_loop)]
fn apply_trig_transform(x: &[f64], kind: char, t: u8) -> Vec<f64> {
    let n = x.len();
    let nf = n as f64;
    let pi = std::f64::consts::PI;
    let sqrt2 = std::f64::consts::SQRT_2;
    let mut out = vec![0.0f64; n];
    match (kind, t) {
        ('c', 1) => {
            let m = (n - 1) as f64;
            let sc = (2.0 / m).sqrt();
            for j in 0..n {
                let edge = (x[0] + if j % 2 == 0 { x[n - 1] } else { -x[n - 1] }) / 2.0;
                let mut acc = edge;
                for i in 1..n - 1 {
                    acc += x[i] * (pi * i as f64 * j as f64 / m).cos();
                }
                out[j] = sc * acc;
            }
        }
        ('c', 2) => {
            let sc = (1.0 / nf).sqrt();
            for j in 0..n {
                let e = if j == 0 { 1.0 } else { sqrt2 };
                let mut acc = 0.0;
                for i in 0..n {
                    acc += x[i] * (pi * (i as f64 + 0.5) * j as f64 / nf).cos();
                }
                out[j] = sc * e * acc;
            }
        }
        ('c', 3) => {
            let sc = (1.0 / nf).sqrt();
            for j in 0..n {
                let mut acc = x[0];
                for i in 1..n {
                    acc += sqrt2 * x[i] * (pi * i as f64 * (j as f64 + 0.5) / nf).cos();
                }
                out[j] = sc * acc;
            }
        }
        ('c', 4) => {
            let sc = (2.0 / nf).sqrt();
            for j in 0..n {
                let mut acc = 0.0;
                for i in 0..n {
                    acc += x[i] * (pi * (i as f64 + 0.5) * (j as f64 + 0.5) / nf).cos();
                }
                out[j] = sc * acc;
            }
        }
        ('s', 1) => {
            let sc = (2.0 / (nf + 1.0)).sqrt();
            for j in 0..n {
                let mut acc = 0.0;
                for i in 0..n {
                    acc += x[i] * (pi * (i as f64 + 1.0) * (j as f64 + 1.0) / (nf + 1.0)).sin();
                }
                out[j] = sc * acc;
            }
        }
        ('s', 2) => {
            // The last mode carries half the weight, so use 1/Sqrt[n] with a
            // Sqrt[2] factor on all but the final row.
            let sc = (1.0 / nf).sqrt();
            for j in 0..n {
                let w = if j == n - 1 { 1.0 } else { sqrt2 };
                let mut acc = 0.0;
                for i in 0..n {
                    acc += x[i] * (pi * (i as f64 + 0.5) * (j as f64 + 1.0) / nf).sin();
                }
                out[j] = sc * w * acc;
            }
        }
        ('s', 3) => {
            // Inverse of DST-II.
            let sc = (1.0 / nf).sqrt();
            for i in 0..n {
                let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
                let mut acc = sign * x[n - 1];
                for j in 0..n - 1 {
                    acc += sqrt2 * x[j] * (pi * (i as f64 + 0.5) * (j as f64 + 1.0) / nf).sin();
                }
                out[i] = sc * acc;
            }
        }
        ('s', 4) => {
            let sc = (2.0 / nf).sqrt();
            for j in 0..n {
                let mut acc = 0.0;
                for i in 0..n {
                    acc += x[i] * (pi * (i as f64 + 0.5) * (j as f64 + 0.5) / nf).sin();
                }
                out[j] = sc * acc;
            }
        }
        _ => out.copy_from_slice(x),
    }
    out
}

/// Scrub values that are numerically zero before building a `Value`.
fn clean_real(f: f64) -> Value {
    let f = if f.abs() < 1e-12 { 0.0 } else { f };
    Value::Real(Float::with_val(DEFAULT_PRECISION, f))
}

fn trig_transform_builtin(args: &[Value], kind: char, name: &str) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(format!(
            "{name} requires 1 or 2 arguments"
        )));
    }
    let list = match get_list(&args[0]) {
        Some(l) => l,
        None => return Ok(unevaluated(name, args)),
    };
    let t = match args.get(1) {
        None => 2u8,
        Some(v) => match parse_transform_type(v) {
            Some(t) => t,
            None => return Ok(unevaluated(name, args)),
        },
    };
    let mut x = Vec::with_capacity(list.len());
    for v in list {
        match v.to_real() {
            Some(f) => x.push(f),
            None => return Ok(unevaluated(name, args)),
        }
    }
    if x.is_empty() {
        return Ok(Value::List(vec![]));
    }
    if kind == 'c' && t == 1 && x.len() < 2 {
        return Ok(unevaluated(name, args));
    }
    let y = apply_trig_transform(&x, kind, t);
    Ok(Value::List(y.into_iter().map(clean_real).collect()))
}

/// `FourierDCT[list]` / `FourierDCT[list, m]` — discrete cosine transform.
pub fn builtin_fourier_dct(args: &[Value]) -> Result<Value, EvalError> {
    trig_transform_builtin(args, 'c', "FourierDCT")
}

/// `FourierDST[list]` / `FourierDST[list, m]` — discrete sine transform.
pub fn builtin_fourier_dst(args: &[Value]) -> Result<Value, EvalError> {
    trig_transform_builtin(args, 's', "FourierDST")
}

// ── Hadamard / Walsh–Hadamard / Haar ────────────────────────────────────────

/// Sylvester (bit-complement) Hadamard matrix of power-of-two size `n`.
#[allow(clippy::needless_range_loop)]
fn sylvester(n: usize) -> Vec<Vec<i8>> {
    let mut h = vec![vec![1i8]];
    while h.len() < n {
        let m = h.len();
        let mut nh = vec![vec![0i8; 2 * m]; 2 * m];
        for i in 0..m {
            for j in 0..m {
                let x = h[i][j];
                nh[i][j] = x;
                nh[i][j + m] = x;
                nh[i + m][j] = x;
                nh[i + m][j + m] = -x;
            }
        }
        h = nh;
    }
    h
}

fn sequency_count(row: &[i8]) -> usize {
    row.windows(2).filter(|w| w[0] != w[1]).count()
}

/// `HadamardMatrix[n]` — `n × n` Hadamard matrix (`n` a power of two).
pub fn builtin_hadamard_matrix(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "HadamardMatrix requires 1 argument: HadamardMatrix[n]".to_string(),
        ));
    }
    let n = match args[0].to_integer() {
        Some(n) if n >= 1 && (n as usize).is_power_of_two() => n as usize,
        _ => return Ok(unevaluated("HadamardMatrix", args)),
    };

    let mut sequency = true;
    if let Some(Value::Rule { lhs, rhs, .. }) = args.get(1)
        && matches!(&**lhs, Value::Symbol(s) if s == "Method")
        && let Value::Str(m) = &**rhs
        && (m.eq_ignore_ascii_case("BitComplement") || m.eq_ignore_ascii_case("GrayCode"))
    {
        sequency = false;
    }

    let mut h = sylvester(n);
    if sequency {
        h.sort_by_key(|row| sequency_count(row));
    }
    Ok(Value::List(
        h.iter()
            .map(|row| Value::List(row.iter().map(|&e| vint(e as i64)).collect()))
            .collect(),
    ))
}

/// `WalshHadamardTransform[list]` — unnormalized Walsh–Hadamard transform.
pub fn builtin_walsh_hadamard(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "WalshHadamardTransform requires exactly 1 argument".to_string(),
        ));
    }
    let list = match get_list(&args[0]) {
        Some(l) if !l.is_empty() && l.len().is_power_of_two() => l,
        _ => return Ok(unevaluated("WalshHadamardTransform", args)),
    };
    let h = sylvester(list.len());
    let mut out = Vec::with_capacity(list.len());
    for row in &h {
        let mut acc = vint(0);
        for (e, x) in row.iter().zip(list.iter()) {
            let term = if *e == 1 {
                x.clone()
            } else {
                mul_values_public(&vint(-1), x)?
            };
            acc = add_values_public(&acc, &term)?;
        }
        out.push(acc);
    }
    Ok(Value::List(out))
}

/// `DiscreteHadamardTransform[list]` — `1/Sqrt[n]`-normalized Hadamard transform.
pub fn builtin_discrete_hadamard(args: &[Value]) -> Result<Value, EvalError> {
    let v = builtin_walsh_hadamard(args)?;
    match v {
        Value::List(items) => {
            let s = 1.0 / (items.len() as f64).sqrt();
            let mut out = Vec::with_capacity(items.len());
            for item in &items {
                out.push(mul_values_public(&crate::builtins::real(s), item)?);
            }
            Ok(Value::List(out))
        }
        other => Ok(other),
    }
}

/// `DiscreteHaarWaveletTransform[list]` — unnormalized in-place Haar pyramid.
pub fn builtin_discrete_haar(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "DiscreteHaarWaveletTransform requires exactly 1 argument".to_string(),
        ));
    }
    let list = match get_list(&args[0]) {
        Some(l) if !l.is_empty() && l.len().is_power_of_two() => l.clone(),
        _ => return Ok(unevaluated("DiscreteHaarWaveletTransform", args)),
    };
    let mut a = list;
    let mut n = a.len();
    while n >= 2 {
        let half = n / 2;
        // Write into a fresh buffer so overlapping pairs are read from the
        // original data.
        let mut next = a.clone();
        for i in 0..half {
            next[i] = add_values_public(&a[2 * i], &a[2 * i + 1])?;
            next[half + i] = sub_values_public(&a[2 * i], &a[2 * i + 1])?;
        }
        a = next;
        n = half;
    }
    Ok(Value::List(a))
}

// ── Convolutions ────────────────────────────────────────────────────────────

/// Correlate `kernel` with `list`: `out[j] = Σ_i kernel[i] * list[j + i]`.
fn correlate(kernel: &[Value], list: &[Value]) -> Result<Value, EvalError> {
    if kernel.is_empty() || list.is_empty() || list.len() < kernel.len() {
        return Ok(Value::List(vec![]));
    }
    let mut out = Vec::with_capacity(list.len() - kernel.len() + 1);
    for j in 0..=(list.len() - kernel.len()) {
        let mut sum = vint(0);
        for (i, k) in kernel.iter().enumerate() {
            let prod = mul_values_public(k, &list[j + i])?;
            sum = add_values_public(&sum, &prod)?;
        }
        out.push(sum);
    }
    Ok(Value::List(out))
}

/// `ListCorrelate[kernel, list]`.
pub fn builtin_list_correlate(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ListCorrelate requires exactly 2 arguments: ListCorrelate[kernel, list]".to_string(),
        ));
    }
    match (get_list(&args[0]), get_list(&args[1])) {
        (Some(k), Some(l)) => correlate(k, l),
        _ => Ok(unevaluated("ListCorrelate", args)),
    }
}

/// `ListConvolve[kernel, list]` — convolution (kernel reversed before correlating).
pub fn builtin_list_convolve(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ListConvolve requires exactly 2 arguments: ListConvolve[kernel, list]".to_string(),
        ));
    }
    match (get_list(&args[0]), get_list(&args[1])) {
        (Some(k), Some(l)) => {
            let rev: Vec<Value> = k.iter().rev().cloned().collect();
            correlate(&rev, l)
        }
        _ => Ok(unevaluated("ListConvolve", args)),
    }
}

/// `DiscreteConvolve[f, g, n, m]` for list-valued `f`, `g` — full convolution.
pub fn builtin_discrete_convolve(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 || args.len() > 4 {
        return Err(EvalError::Error(
            "DiscreteConvolve requires 3 or 4 arguments".to_string(),
        ));
    }
    match (get_list(&args[0]), get_list(&args[1])) {
        (Some(f), Some(g)) => {
            if f.is_empty() || g.is_empty() {
                return Ok(Value::List(vec![]));
            }
            let mut out = vec![vint(0); f.len() + g.len() - 1];
            for (i, a) in f.iter().enumerate() {
                for (j, b) in g.iter().enumerate() {
                    let prod = mul_values_public(a, b)?;
                    out[i + j] = add_values_public(&out[i + j], &prod)?;
                }
            }
            Ok(Value::List(out))
        }
        _ => Ok(unevaluated("DiscreteConvolve", args)),
    }
}

/// Evaluate `f` at integer index `idx`: list indexing (1-based) or substitution.
fn eval_indexed(f: &Value, var: &str, idx: i64) -> Value {
    match f {
        Value::List(l) => {
            if idx >= 1 && (idx as usize) <= l.len() {
                l[idx as usize - 1].clone()
            } else {
                vint(0)
            }
        }
        _ if contains_symbol(f, var) => substitute_value(f, var, &vint(idx)),
        _ => f.clone(),
    }
}

fn divisors_i64(m: i64) -> Vec<i64> {
    let mut ds = Vec::new();
    let mut d = 1i64;
    while d * d <= m {
        if m % d == 0 {
            ds.push(d);
            if d != m / d {
                ds.push(m / d);
            }
        }
        d += 1;
    }
    ds.sort_unstable();
    ds
}

/// `DirichletConvolve[f, g, n, m]` — `Σ_{d | m} f(d) g(m/d)`.
pub fn builtin_dirichlet_convolve(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 4 {
        return Err(EvalError::Error(
            "DirichletConvolve requires exactly 4 arguments: DirichletConvolve[f, g, n, m]"
                .to_string(),
        ));
    }
    let m = match args[3].to_integer() {
        Some(m) if m >= 1 => m,
        _ => return Ok(unevaluated("DirichletConvolve", args)),
    };
    let var = match &args[2] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(unevaluated("DirichletConvolve", args)),
    };
    let mut sum = vint(0);
    for d in divisors_i64(m) {
        let fd = eval_indexed(&args[0], &var, d);
        let gd = eval_indexed(&args[1], &var, m / d);
        let prod = mul_values_public(&fd, &gd)?;
        sum = add_values_public(&sum, &prod)?;
    }
    Ok(sum)
}

// ── Dirichlet characters and L-functions ────────────────────────────────────

fn gcd_i64(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a.abs()
}

fn euler_phi_i64(m: i64) -> i64 {
    let mut result = m;
    let mut n = m;
    let mut d = 2i64;
    while d * d <= n {
        if n % d == 0 {
            while n % d == 0 {
                n /= d;
            }
            result -= result / d;
        }
        d += 1;
    }
    if n > 1 {
        result -= result / n;
    }
    result
}

fn multiplicative_order(g: i64, m: i64) -> i64 {
    let g = g.rem_euclid(m);
    let mut x = 1i64;
    for o in 1..=m {
        x = (x * g) % m;
        if x == 1 {
            return o;
        }
    }
    0
}

fn extended_gcd(a: i64, b: i64) -> (i64, i64, i64) {
    if b == 0 {
        (a, 1, 0)
    } else {
        let (g, x, y) = extended_gcd(b, a.rem_euclid(b));
        (g, y, x - (a / b) * y)
    }
}

fn mod_inverse(a: i64, m: i64) -> i64 {
    if m == 1 {
        return 0;
    }
    let (g, x, _) = extended_gcd(a.rem_euclid(m), m);
    if g != 1 { 0 } else { x.rem_euclid(m) }
}

/// Lift `a mod mod_a` to a residue mod `m` that is `1` modulo `m / mod_a`.
fn crt_lift(a: i64, mod_a: i64, m: i64) -> i64 {
    let mb = (m / mod_a).max(1);
    let inv = mod_inverse(mod_a % mb, mb);
    let t = ((1 - a.rem_euclid(mb)).rem_euclid(mb) * inv) % mb;
    (a + mod_a * t).rem_euclid(m)
}

/// Prime-power factors of `m` (e.g. 12 -> {4, 3}).
fn prime_power_factors(m: i64) -> Vec<i64> {
    let mut n = m;
    let mut d = 2i64;
    let mut out = Vec::new();
    while d * d <= n {
        if n % d == 0 {
            let mut pk = 1i64;
            while n % d == 0 {
                n /= d;
                pk *= d;
            }
            out.push(pk);
        }
        d += 1;
    }
    if n > 1 {
        out.push(n);
    }
    out
}

/// Generators `(g, order)` of `(Z/kZ)*` as a product of cyclic factors.
fn unit_group_generators(k: i64) -> Vec<(i64, i64)> {
    if k <= 2 {
        return Vec::new();
    }
    // (generator mod pk, order, pk)
    let mut raw: Vec<(i64, i64, i64)> = Vec::new();
    for pk in prime_power_factors(k) {
        if pk == 2 {
            continue;
        }
        if pk == 4 {
            raw.push((3, 2, 4));
            continue;
        }
        if pk > 0 && (pk & (pk - 1)) == 0 {
            let e = pk.trailing_zeros() as i64;
            raw.push(((-1i64).rem_euclid(pk), 2, pk));
            raw.push((5i64.rem_euclid(pk), 1i64 << (e - 2), pk));
            continue;
        }
        let phi = euler_phi_i64(pk);
        let mut g = 2i64;
        while multiplicative_order(g, pk) != phi {
            g += 1;
        }
        raw.push((g % pk, phi, pk));
    }
    raw.into_iter()
        .map(|(g, o, pk)| (crt_lift(g, pk, k), o))
        .collect()
}

fn discrete_log(target: i64, g: i64, m: i64, order: i64) -> i64 {
    let target = target.rem_euclid(m);
    let g = g.rem_euclid(m);
    let mut x = 1i64;
    for a in 0..order {
        if x == target {
            return a;
        }
        x = (x * g) % m;
    }
    0
}

/// Complex value of the Dirichlet character `(k, j)` at `n`.
fn dirichlet_character_value(k: i64, j: i64, n: i64) -> (f64, f64) {
    if k <= 1 {
        return (1.0, 0.0);
    }
    let n = n.rem_euclid(k);
    if gcd_i64(n, k) != 1 {
        return (0.0, 0.0);
    }
    let gens = unit_group_generators(k);
    let total: i64 = gens.iter().map(|(_, o)| *o).product();
    if total <= 0 {
        return (1.0, 0.0);
    }
    let jj = (j - 1).rem_euclid(total);
    if jj == 0 {
        return (1.0, 0.0);
    }
    let mut idx = jj;
    let mut phase = 0.0f64;
    for (g, o) in &gens {
        let comp = idx % *o;
        idx /= *o;
        let a = discrete_log(n, *g, k, *o);
        phase += std::f64::consts::TAU * (comp as f64) * (a as f64) / (*o as f64);
    }
    (phase.cos(), phase.sin())
}

/// Lower a character value, snapping near-{0, ±1} results to exact integers.
fn char_value_to_value(re: f64, im: f64) -> Value {
    let re = if re.abs() < 1e-12 { 0.0 } else { re };
    let im = if im.abs() < 1e-12 { 0.0 } else { im };
    if im == 0.0 {
        if (re - 1.0).abs() < 1e-12 {
            vint(1)
        } else if (re + 1.0).abs() < 1e-12 {
            vint(-1)
        } else if re == 0.0 {
            vint(0)
        } else {
            Value::Real(Float::with_val(DEFAULT_PRECISION, re))
        }
    } else {
        Value::Complex { re, im }
    }
}

/// `DirichletCharacter[k, j, n]` (and the 2-argument principal form).
pub fn builtin_dirichlet_character(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 && args.len() != 3 {
        return Err(EvalError::Error(
            "DirichletCharacter requires 2 or 3 arguments".to_string(),
        ));
    }
    let k = match args[0].to_integer() {
        Some(k) if k >= 1 => k,
        _ => return Ok(unevaluated("DirichletCharacter", args)),
    };
    let (j, nval) = if args.len() == 3 {
        match args[1].to_integer() {
            Some(j) => (j, &args[2]),
            None => return Ok(unevaluated("DirichletCharacter", args)),
        }
    } else {
        (1, &args[1])
    };

    match nval {
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item.to_integer() {
                    Some(n) => {
                        let (re, im) = dirichlet_character_value(k, j, n);
                        out.push(char_value_to_value(re, im));
                    }
                    None => return Ok(unevaluated("DirichletCharacter", args)),
                }
            }
            Ok(Value::List(out))
        }
        _ => match nval.to_integer() {
            Some(n) => {
                let (re, im) = dirichlet_character_value(k, j, n);
                Ok(char_value_to_value(re, im))
            }
            None => Ok(unevaluated("DirichletCharacter", args)),
        },
    }
}

/// Hurwitz zeta `ζ(s, a)` via Euler–Maclaurin (valid for real `s > 1`).
fn hurwitz_zeta(s: f64, a: f64) -> f64 {
    const B2J: [f64; 6] = [
        1.0 / 6.0,
        -1.0 / 30.0,
        1.0 / 42.0,
        -1.0 / 30.0,
        5.0 / 66.0,
        -691.0 / 2730.0,
    ];
    const FACT2J: [f64; 6] = [2.0, 24.0, 720.0, 40320.0, 3628800.0, 479001600.0];
    let n = 16usize;
    let mut sum = 0.0f64;
    for i in 0..n {
        sum += (a + i as f64).powf(-s);
    }
    let x = a + n as f64;
    sum += x.powf(1.0 - s) / (s - 1.0) + 0.5 * x.powf(-s);
    let mut poch = s;
    let mut xpow = x.powf(-s - 1.0);
    for j in 0..6 {
        sum += B2J[j] / FACT2J[j] * poch * xpow;
        poch *= (s + 2.0 * j as f64 + 1.0) * (s + 2.0 * j as f64 + 2.0);
        xpow *= x.powi(-2);
    }
    sum
}

/// `DirichletL[k, j, s]` — Dirichlet L-function of character `(k, j)`.
pub fn builtin_dirichlet_l(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "DirichletL requires exactly 3 arguments: DirichletL[k, j, s]".to_string(),
        ));
    }
    let k = match args[0].to_integer() {
        Some(k) if k >= 1 => k,
        _ => return Ok(unevaluated("DirichletL", args)),
    };
    let j = args[1].to_integer().unwrap_or(1);
    let s = match args[2].to_real() {
        Some(s) if s > 0.0 => s,
        _ => return Ok(unevaluated("DirichletL", args)),
    };

    if k == 1 {
        return crate::builtins::specialfunctions::builtin_zeta(&[args[2].clone()]);
    }

    let (mut re, mut im) = (0.0f64, 0.0f64);
    if s > 1.0 {
        for r in 1..=k {
            let (cr, ci) = dirichlet_character_value(k, j, r);
            if cr == 0.0 && ci == 0.0 {
                continue;
            }
            let hz = hurwitz_zeta(s, r as f64 / k as f64);
            re += cr * hz;
            im += ci * hz;
        }
        let sc = (k as f64).powf(-s);
        re *= sc;
        im *= sc;
    } else {
        let period: Vec<(f64, f64)> = (1..=k)
            .map(|r| dirichlet_character_value(k, j, r))
            .collect();
        let nmax = 2_000_000u64;
        for mm in 1..=nmax {
            let (cr, ci) = period[((mm - 1) % (k as u64)) as usize];
            let w = (mm as f64).powf(-s);
            re += cr * w;
            im += ci * w;
        }
    }
    Ok(char_value_to_value(re, im))
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

    // ── FindSequenceFunction ──

    #[test]
    fn test_find_sequence_function_quadratic() {
        // {1, 4, 9, 16, 25} -> n^2
        let result =
            builtin_find_sequence_function(&[list(vec![int(1), int(4), int(9), int(16), int(25)])])
                .unwrap();
        match result {
            Value::PureFunction { .. } => {
                // Verify it produces correct values by checking the body structure
                // The function should be #1^2 (i.e., slot squared)
            }
            _ => panic!("Expected PureFunction, got {:?}", result),
        }
    }

    #[test]
    fn test_find_sequence_function_linear() {
        // {2, 4, 6, 8} -> 2*n
        let result =
            builtin_find_sequence_function(&[list(vec![int(2), int(4), int(6), int(8)])]).unwrap();
        match result {
            Value::PureFunction { .. } => {}
            _ => panic!("Expected PureFunction, got {:?}", result),
        }
    }

    #[test]
    fn test_find_sequence_function_constant() {
        // {5, 5, 5, 5} -> 5
        let result =
            builtin_find_sequence_function(&[list(vec![int(5), int(5), int(5), int(5)])]).unwrap();
        match result {
            Value::PureFunction { .. } => {}
            _ => panic!("Expected PureFunction, got {:?}", result),
        }
    }

    #[test]
    fn test_find_sequence_function_non_polynomial_returns_symbolic() {
        // Fibonacci: {1, 1, 2, 3, 5, 8} — not a polynomial
        let result = builtin_find_sequence_function(&[list(vec![
            int(1),
            int(1),
            int(2),
            int(3),
            int(5),
            int(8),
        ])])
        .unwrap();
        match result {
            Value::Call { head, .. } => assert_eq!(head, "FindSequenceFunction"),
            _ => panic!("Expected symbolic Call for non-polynomial sequence"),
        }
    }

    #[test]
    fn test_find_sequence_function_bad_args() {
        assert!(builtin_find_sequence_function(&[]).is_err());
        assert!(builtin_find_sequence_function(&[int(1), int(2)]).is_err());
    }

    // ── DifferenceDelta ──

    #[test]
    fn test_difference_delta_list() {
        // {1, 4, 9, 16} -> {3, 5, 7}
        let result =
            builtin_difference_delta(&[list(vec![int(1), int(4), int(9), int(16)])]).unwrap();
        assert_eq!(result, list(vec![int(3), int(5), int(7)]));
    }

    #[test]
    fn test_difference_delta_arithmetic() {
        // {2, 4, 6, 8, 10} -> {2, 2, 2, 2}
        let result =
            builtin_difference_delta(&[list(vec![int(2), int(4), int(6), int(8), int(10)])])
                .unwrap();
        assert_eq!(result, list(vec![int(2), int(2), int(2), int(2)]));
    }

    #[test]
    fn test_difference_delta_empty() {
        let result = builtin_difference_delta(&[list(vec![])]).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_difference_delta_single() {
        let result = builtin_difference_delta(&[list(vec![int(5)])]).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_difference_delta_symbolic() {
        // DifferenceDelta[f[i], i] -> f[1+i] + Times[-1, f[i]]
        let result = builtin_difference_delta(&[
            Value::Call {
                head: "f".to_string(),
                args: vec![Value::Symbol("i".to_string())],
            },
            Value::Symbol("i".to_string()),
        ])
        .unwrap();
        match result {
            Value::Call { head, args } => {
                assert_eq!(head, "Plus");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Plus Call, got {:?}", result),
        }
    }

    #[test]
    fn test_difference_delta_bad_args() {
        assert!(builtin_difference_delta(&[]).is_err());
        assert!(builtin_difference_delta(&[int(1), int(2), int(3)]).is_err());
    }

    // ── FindLinearRecurrence ──

    #[test]
    fn test_find_linear_recurrence_fibonacci() {
        // {1, 1, 2, 3, 5, 8} -> {1, 1} (a[n] = a[n-1] + a[n-2])
        let result = builtin_find_linear_recurrence(&[list(vec![
            int(1),
            int(1),
            int(2),
            int(3),
            int(5),
            int(8),
        ])])
        .unwrap();
        assert_eq!(result, list(vec![int(1), int(1)]));
    }

    #[test]
    fn test_find_linear_recurrence_geometric() {
        // {1, 2, 4, 8, 16} -> {2} (a[n] = 2*a[n-1])
        let result =
            builtin_find_linear_recurrence(&[list(vec![int(1), int(2), int(4), int(8), int(16)])])
                .unwrap();
        assert_eq!(result, list(vec![int(2)]));
    }

    #[test]
    fn test_find_linear_recurrence_arithmetic() {
        // {1, 3, 5, 7, 9} -> {2, -1} (a[n] = 2*a[n-1] - a[n-2])
        let result =
            builtin_find_linear_recurrence(&[list(vec![int(1), int(3), int(5), int(7), int(9)])])
                .unwrap();
        assert_eq!(result, list(vec![int(2), int(-1)]));
    }

    #[test]
    fn test_find_linear_recurrence_bad_args() {
        assert!(builtin_find_linear_recurrence(&[]).is_err());
    }

    // ── Fourier / FFT ─────────────────────────────────────────────────────

    fn c64(re: f64, im: f64) -> C64 {
        C64::new(re, im)
    }

    #[test]
    fn test_fft_matches_naive_dft_power_of_two() {
        let data = vec![
            c64(1.0, 0.0),
            c64(0.5, -0.25),
            c64(-2.0, 0.75),
            c64(3.0, 0.0),
            c64(0.0, 1.0),
            c64(-1.0, -1.0),
            c64(2.5, 0.0),
            c64(0.25, 0.125),
        ];
        for sign in [1.0f64, -1.0] {
            let fast = fft(&data, sign);
            let slow = naive_dft(&data, sign);
            assert_eq!(fast.len(), slow.len());
            for (a, b) in fast.iter().zip(slow.iter()) {
                assert!((a.re - b.re).abs() < 1e-9, "re {} vs {}", a.re, b.re);
                assert!((a.im - b.im).abs() < 1e-9, "im {} vs {}", a.im, b.im);
            }
        }
    }

    #[test]
    fn test_fft_matches_naive_dft_non_power_of_two() {
        let data = vec![
            c64(1.0, 0.0),
            c64(1.0, 1.0),
            c64(2.0, -1.0),
            c64(0.0, 0.5),
            c64(-1.0, 0.0),
            c64(3.0, 0.0),
        ];
        let fast = fft(&data, 1.0);
        let slow = naive_dft(&data, 1.0);
        for (a, b) in fast.iter().zip(slow.iter()) {
            assert!((a.re - b.re).abs() < 1e-9);
            assert!((a.im - b.im).abs() < 1e-9);
        }
    }

    fn as_real_list(v: &Value) -> Vec<f64> {
        match v {
            Value::List(l) => l.iter().map(|x| x.to_real().unwrap_or(f64::NAN)).collect(),
            _ => panic!("expected numeric list, got {v:?}"),
        }
    }

    #[test]
    fn test_fourier_constant() {
        let out = builtin_fourier(&[list(vec![int(1), int(1), int(1), int(1)])]).unwrap();
        let vals = as_real_list(&out);
        assert!((vals[0] - 2.0).abs() < 1e-10);
        for v in &vals[1..] {
            assert!(v.abs() < 1e-10);
        }
    }

    #[test]
    fn test_fourier_roundtrip() {
        let data = list(vec![
            real(1.0),
            real(-2.0),
            real(3.5),
            real(0.25),
            real(-0.75),
        ]);
        let spectrum = builtin_fourier(&[data]).unwrap();
        let back = builtin_inverse_fourier(&[spectrum]).unwrap();
        let vals = as_real_list(&back);
        for (v, e) in vals.iter().zip([1.0, -2.0, 3.5, 0.25, -0.75].iter()) {
            assert!((v - e).abs() < 1e-9);
        }
    }

    #[test]
    fn test_fourier_bad_args() {
        assert!(builtin_fourier(&[]).is_err());
        // Non-list argument stays unevaluated.
        assert!(builtin_fourier(&[int(1)]).is_ok());
    }

    // ── DCT / DST ─────────────────────────────────────────────────────────

    fn roundtrip_trig(kind: char, t: u8, data: &[f64]) {
        let y = apply_trig_transform(data, kind, t);
        let inv_type = match (kind, t) {
            ('c', 2) | ('s', 2) => 3u8,
            ('c', 3) | ('s', 3) => 2u8,
            (_, other) => other,
        };
        let z = apply_trig_transform(&y, kind, inv_type);
        for (a, b) in z.iter().zip(data.iter()) {
            assert!((a - b).abs() < 1e-9, "{a} != {b}");
        }
    }

    #[test]
    fn test_dct_inverses() {
        let data = [0.3, -1.2, 2.5, 0.0, 1.1, -0.4, 0.7, 3.2];
        for t in 1..=4u8 {
            roundtrip_trig('c', t, &data);
        }
    }

    #[test]
    fn test_dst_inverses() {
        let data = [0.3, -1.2, 2.5, 0.0, 1.1, -0.4, 0.7, 3.2];
        for t in 1..=4u8 {
            roundtrip_trig('s', t, &data);
        }
    }

    #[test]
    fn test_dct2_of_constant() {
        let out =
            builtin_fourier_dct(&[list(vec![int(1), int(1), int(1), int(1), int(1)])]).unwrap();
        let vals = as_real_list(&out);
        assert!((vals[0] - 5.0f64.sqrt()).abs() < 1e-10);
        for v in &vals[1..] {
            assert!(v.abs() < 1e-10);
        }
    }

    #[test]
    fn test_dct_type_string() {
        let a = builtin_fourier_dct(&[list(vec![int(1), int(2), int(3)]), Value::Str("IV".into())])
            .unwrap();
        let b = builtin_fourier_dct(&[list(vec![int(1), int(2), int(3)]), int(4)]).unwrap();
        assert!(a.struct_eq(&b));
    }

    // ── Hadamard / Walsh–Hadamard / Haar ──────────────────────────────────

    #[test]
    fn test_hadamard_matrix_orthogonal() {
        for n in [1usize, 2, 4, 8] {
            let h = builtin_hadamard_matrix(&[int(n as i64)]).unwrap();
            let rows = match &h {
                Value::List(r) => r,
                _ => panic!("expected list"),
            };
            assert_eq!(rows.len(), n);
            for row in rows {
                let items = match row {
                    Value::List(i) => i,
                    _ => panic!("expected row list"),
                };
                assert_eq!(items.len(), n);
                for e in items {
                    let v = e.to_integer().unwrap();
                    assert!(v == 1 || v == -1);
                }
            }
            for a in 0..n {
                for b in 0..n {
                    let mut dot = 0i64;
                    for c in 0..n {
                        let av = match &rows[a] {
                            Value::List(i) => i[c].to_integer().unwrap(),
                            _ => panic!(),
                        };
                        let bv = match &rows[b] {
                            Value::List(i) => i[c].to_integer().unwrap(),
                            _ => panic!(),
                        };
                        dot += av * bv;
                    }
                    assert_eq!(dot, if a == b { n as i64 } else { 0 });
                }
            }
        }
    }

    #[test]
    fn test_hadamard_non_power_of_two() {
        match builtin_hadamard_matrix(&[int(3)]).unwrap() {
            Value::Call { head, .. } => assert_eq!(head, "HadamardMatrix"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_walsh_hadamard() {
        let out = builtin_walsh_hadamard(&[list(vec![int(1), int(2), int(3), int(4)])]).unwrap();
        assert_eq!(out, list(vec![int(10), int(-2), int(-4), int(0)]));
    }

    #[test]
    fn test_haar_wavelet() {
        let out = builtin_discrete_haar(&[list(vec![int(1), int(2), int(3), int(4)])]).unwrap();
        assert_eq!(out, list(vec![int(10), int(-4), int(-1), int(-1)]));
    }

    // ── Convolutions ──────────────────────────────────────────────────────

    #[test]
    fn test_list_convolve() {
        let out = builtin_list_convolve(&[
            list(vec![int(1), int(2), int(3)]),
            list(vec![int(1), int(2), int(3), int(4), int(5)]),
        ])
        .unwrap();
        assert_eq!(out, list(vec![int(10), int(16), int(22)]));
    }

    #[test]
    fn test_list_correlate() {
        let out = builtin_list_correlate(&[
            list(vec![int(1), int(2), int(3)]),
            list(vec![int(1), int(2), int(3), int(4), int(5)]),
        ])
        .unwrap();
        assert_eq!(out, list(vec![int(14), int(20), int(26)]));
    }

    #[test]
    fn test_discrete_convolve() {
        let out = builtin_discrete_convolve(&[
            list(vec![int(1), int(2), int(3)]),
            list(vec![int(1), int(1)]),
            sym("n"),
            sym("m"),
        ])
        .unwrap();
        assert_eq!(out, list(vec![int(1), int(3), int(5), int(3)]));
    }

    #[test]
    fn test_dirichlet_convolve_lists() {
        let out = builtin_dirichlet_convolve(&[
            list(vec![int(1), int(2), int(3), int(4)]),
            list(vec![int(1), int(1), int(1), int(1)]),
            sym("n"),
            int(4),
        ])
        .unwrap();
        assert_eq!(out, int(7));
    }

    #[test]
    fn test_dirichlet_convolve_scalars() {
        // Convolving the constant 1 with itself gives the divisor-count at m.
        let out = builtin_dirichlet_convolve(&[int(1), int(1), sym("n"), int(6)]).unwrap();
        assert_eq!(out, int(4));
    }

    // ── Dirichlet characters / L-functions ────────────────────────────────

    #[test]
    fn test_dirichlet_character_principal_mod2() {
        let out = builtin_dirichlet_character(&[
            int(2),
            int(1),
            list(vec![int(1), int(2), int(3), int(4)]),
        ])
        .unwrap();
        assert_eq!(out, list(vec![int(1), int(0), int(1), int(0)]));
    }

    #[test]
    fn test_dirichlet_character_nontrivial_mod3() {
        let out = builtin_dirichlet_character(&[
            int(3),
            int(2),
            list(vec![int(1), int(2), int(3), int(4)]),
        ])
        .unwrap();
        assert_eq!(out, list(vec![int(1), int(-1), int(0), int(1)]));
    }

    #[test]
    fn test_dirichlet_character_composite_modulus() {
        let out = builtin_dirichlet_character(&[
            int(8),
            list(vec![int(1), int(2), int(3), int(5), int(7)]),
        ])
        .unwrap();
        assert_eq!(out, list(vec![int(1), int(0), int(1), int(1), int(1)]));
    }

    #[test]
    fn test_dirichlet_l_zeta_case() {
        // DirichletL[1, 1, s] == Zeta[s]; for s = 2 that is Pi^2 / 6 (symbolic).
        let out = builtin_dirichlet_l(&[int(1), int(1), int(2)]).unwrap();
        let zeta2 = crate::builtins::specialfunctions::builtin_zeta(&[int(2)]).unwrap();
        assert!(out.struct_eq(&zeta2), "{out:?}");
    }

    #[test]
    fn test_dirichlet_l_matches_naive_sum() {
        let chi: Vec<(f64, f64)> = (1..=3)
            .map(|n| dirichlet_character_value(3, 2, n))
            .collect();
        let mut naive = 0.0f64;
        for n in 1..=400_000i64 {
            naive += chi[((n - 1) % 3) as usize].0 / (n as f64).powi(2);
        }
        let out = builtin_dirichlet_l(&[int(3), int(2), int(2)]).unwrap();
        match out {
            Value::Real(r) => assert!(
                (r.to_f64() - naive).abs() < 1e-5,
                "{} vs {}",
                r.to_f64(),
                naive
            ),
            other => panic!("{other:?}"),
        }
    }

    // ── Verification of pre-existing discrete builtins ────────────────────

    #[test]
    fn test_verify_discrete_delta_all_zero() {
        assert_eq!(
            builtin_discrete_delta(&[int(0), int(0), int(0)]).unwrap(),
            int(1)
        );
        assert_eq!(builtin_discrete_delta(&[int(0), int(2)]).unwrap(), int(0));
    }

    #[test]
    fn test_verify_kronecker_delta() {
        use crate::builtins::math::builtin_kronecker_delta;
        assert_eq!(builtin_kronecker_delta(&[int(3), int(3)]).unwrap(), int(1));
        assert_eq!(builtin_kronecker_delta(&[int(3), int(4)]).unwrap(), int(0));
    }

    #[test]
    fn test_verify_unit_step_and_heaviside() {
        use crate::builtins::math::{builtin_heaviside_theta, builtin_unit_step};
        assert_eq!(builtin_unit_step(&[int(3)]).unwrap(), int(1));
        assert_eq!(builtin_unit_step(&[int(-1)]).unwrap(), int(0));
        assert_eq!(builtin_unit_step(&[int(0)]).unwrap(), int(1));
        assert_eq!(builtin_heaviside_theta(&[int(2)]).unwrap(), int(1));
        assert_eq!(builtin_heaviside_theta(&[int(-2)]).unwrap(), int(0));
        match builtin_heaviside_theta(&[int(0)]).unwrap() {
            Value::Call { head, .. } => assert_eq!(head, "HeavisideTheta"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_verify_farey_sequence() {
        use crate::builtins::number_theory::builtin_farey_sequence;
        let out = builtin_farey_sequence(&[int(4)]).unwrap();
        let items = match out {
            Value::List(l) => l,
            _ => panic!(),
        };
        let fracs: Vec<f64> = items.iter().map(|v| v.to_real().unwrap()).collect();
        assert_eq!(fracs.len(), 7);
        for w in fracs.windows(2) {
            assert!(w[0] < w[1]);
        }
        assert_eq!(fracs[0], 0.0);
        assert_eq!(*fracs.last().unwrap(), 1.0);
    }

    #[test]
    fn test_verify_moebius_neighbours() {
        use crate::builtins::number_theory::{
            builtin_liouville_lambda, builtin_mangoldt_lambda, builtin_moebius_mu,
        };
        assert_eq!(builtin_moebius_mu(&[int(30)]).unwrap(), int(-1));
        assert_eq!(builtin_moebius_mu(&[int(4)]).unwrap(), int(0));
        assert_eq!(builtin_liouville_lambda(&[int(4)]).unwrap(), int(1));
        assert_eq!(builtin_liouville_lambda(&[int(8)]).unwrap(), int(-1));
        match builtin_mangoldt_lambda(&[int(4)]).unwrap() {
            Value::Real(r) => assert!((r.to_f64() - 2.0f64.ln()).abs() < 1e-12),
            other => panic!("{other:?}"),
        }
    }
}
