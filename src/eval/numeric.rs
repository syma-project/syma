use rug::Float;
/// Numeric evaluation — high-precision numeric evaluation of symbolic expressions.
use rug::ops::Pow;

use crate::ast::*;
use crate::env::Env;
use crate::value::*;

/// Evaluate an expression numerically at the given bit precision.
pub(super) fn numeric_eval_expr(
    expr: &Expr,
    prec_bits: u32,
    env: &Env,
) -> Result<Value, EvalError> {
    match expr {
        Expr::Symbol(s) => match s.as_str() {
            "Pi" => Ok(Value::Real(Float::with_val(
                prec_bits,
                rug::float::Constant::Pi,
            ))),
            "E" => {
                let one = Float::with_val(prec_bits, 1u32);
                Ok(Value::Real(one.exp()))
            }
            _ => {
                let v = super::eval(expr, env)?;
                coerce_to_float(v, prec_bits)
            }
        },
        Expr::Integer(n) => Ok(Value::Real(Float::with_val(prec_bits, n))),
        Expr::Real(r) => Ok(Value::Real(Float::with_val(prec_bits, r))),
        // Numerically evaluate each element of a list.
        Expr::List(items) => {
            let vals: Result<Vec<Value>, EvalError> = items
                .iter()
                .map(|it| numeric_eval_expr(it, prec_bits, env))
                .collect();
            Ok(Value::List(vals?))
        }
        // Recursively evaluate calls at the requested precision.
        Expr::Call { head, args } => {
            if let Expr::Symbol(name) = head.as_ref() {
                // `N` is a special form in `eval` (it never reaches
                // `apply_function`), so a *nested* `N[...]` — e.g. inside the
                // argument of an enclosing `N` — must be dispatched here too.
                // Otherwise the inner `N` would fall through to the generic
                // `apply_function` branch, which has no `N` builtin, and stay
                // unevaluated (`N[3.14..., 20.0]`).
                if name == "N" {
                    if args.is_empty() || args.len() > 2 {
                        return Err(EvalError::Error("N requires 1 or 2 arguments".to_string()));
                    }
                    let inner_bits = if args.len() == 2 {
                        match super::eval(&args[1], env)? {
                            Value::Integer(n) => {
                                let d = u32::try_from(&n).unwrap_or(DEFAULT_PRECISION);
                                ((d as f64) * std::f64::consts::LOG2_10).ceil() as u32
                            }
                            _ => DEFAULT_PRECISION,
                        }
                    } else {
                        DEFAULT_PRECISION
                    };
                    return numeric_eval_expr(&args[0], inner_bits.max(1), env);
                }

                // NHold* attributes: prevent numeric evaluation of certain arguments
                let nhold_all = env.has_attribute(name, "NHoldAll");
                let nhold_first = env.has_attribute(name, "NHoldFirst");
                let nhold_rest = env.has_attribute(name, "NHoldRest");

                let evaluated_args: Result<Vec<Value>, _> = args
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        if nhold_all || (nhold_first && i == 0) || (nhold_rest && i > 0) {
                            super::eval(a, env)
                        } else {
                            numeric_eval_expr(a, prec_bits, env)
                        }
                    })
                    .collect();
                let evaluated_args = evaluated_args?;
                match name.as_str() {
                    "Plus" => numeric_combine("Plus", evaluated_args, prec_bits, env, false),
                    "Times" => numeric_combine("Times", evaluated_args, prec_bits, env, true),
                    "Power" if evaluated_args.len() == 2 => {
                        try_power_float("Power", &evaluated_args, prec_bits, env)
                    }
                    "Divide" if evaluated_args.len() == 2 => {
                        try_binary_float("Divide", &evaluated_args, prec_bits, env, |a, b| {
                            a.clone() / b.clone()
                        })
                    }
                    "Sin" => try_unary_float("Sin", &evaluated_args, prec_bits, env, |f| f.sin()),
                    "Cos" => try_unary_float("Cos", &evaluated_args, prec_bits, env, |f| f.cos()),
                    "Log" if evaluated_args.len() == 1 => {
                        try_unary_float("Log", &evaluated_args, prec_bits, env, |f| f.ln())
                    }
                    "Log" if evaluated_args.len() == 2 => {
                        try_binary_float("Log", &evaluated_args, prec_bits, env, |a, b| {
                            b.ln() / a.ln()
                        })
                    }
                    "Sqrt" => try_sqrt_float("Sqrt", &evaluated_args, prec_bits, env),
                    "Abs" => try_unary_float("Abs", &evaluated_args, prec_bits, env, |f| f.abs()),
                    // `Part` indices must stay exact integers; numeric
                    // evaluation of `1` would otherwise yield `1.0`, which
                    // `Part` rejects. The indexed expression is still
                    // evaluated numerically.
                    "Part" if evaluated_args.len() >= 2 => {
                        let mut vals = Vec::with_capacity(args.len());
                        vals.push(evaluated_args[0].clone());
                        for a in &args[1..] {
                            vals.push(super::eval(a, env)?);
                        }
                        let head_val = super::eval(head, env)?;
                        coerce_to_float(super::apply_function(&head_val, &vals, env)?, prec_bits)
                    }
                    // Equality/inequality: keep the comparison symbolic when
                    // the operands are not all numeric and not structurally
                    // identical, so equation-based functions called inside
                    // `N` (e.g. `N[FindRoot[Cos[x] == x, {x, 1}]]`) can still
                    // inspect the equation instead of receiving `False`.
                    "Equal" | "Unequal"
                        if !evaluated_args
                            .iter()
                            .all(|a| to_float(a, prec_bits).is_some())
                            && !evaluated_args.windows(2).all(|w| w[0].struct_eq(&w[1])) =>
                    {
                        Ok(Value::Call {
                            head: name.clone(),
                            args: evaluated_args,
                        })
                    }
                    // For numeric evaluation of other functions, compute with the
                    // numeric args; if that fails (special forms handled in eval)
                    // fall back to normal evaluation and coerce the result.
                    _ => {
                        let head_val = super::eval(head, env)?;
                        match super::apply_function(&head_val, &evaluated_args, env) {
                            Ok(v) => coerce_to_float(v, prec_bits),
                            Err(_) => {
                                let v = super::eval(expr, env)?;
                                coerce_to_float(v, prec_bits)
                            }
                        }
                    }
                }
            } else {
                // Non-symbol head: evaluate normally
                let v = super::eval(expr, env)?;
                coerce_to_float(v, prec_bits)
            }
        }
        // Other expressions: evaluate normally and coerce
        _ => {
            let v = super::eval(expr, env)?;
            coerce_to_float(v, prec_bits)
        }
    }
}

/// Evaluate unary float function, fall back to symbolic eval if args aren't numeric.
fn try_unary_float(
    head: &str,
    args: &[Value],
    prec_bits: u32,
    env: &Env,
    op: fn(Float) -> Float,
) -> Result<Value, EvalError> {
    if args.len() == 1
        && let Some(f) = to_float(&args[0], prec_bits)
    {
        return Ok(Value::Real(op(f)));
    }
    crate::eval::apply_function(
        &super::eval(&Expr::Symbol(head.to_string()), env)?,
        args,
        env,
    )
}

/// Evaluate binary float function, fall back to symbolic eval if args aren't numeric.
fn try_binary_float(
    head: &str,
    args: &[Value],
    prec_bits: u32,
    env: &Env,
    op: fn(Float, Float) -> Float,
) -> Result<Value, EvalError> {
    if args.len() == 2 {
        let bf = to_float(&args[0], prec_bits);
        let ef = to_float(&args[1], prec_bits);
        if let (Some(a), Some(b)) = (bf, ef) {
            return Ok(Value::Real(op(a, b)));
        }
    }
    crate::eval::apply_function(
        &super::eval(&Expr::Symbol(head.to_string()), env)?,
        args,
        env,
    )
}

/// Evaluate `Power[a, b]`, producing a complex principal value when the base is
/// negative and the exponent is not an integer.
fn try_power_float(
    head: &str,
    args: &[Value],
    prec_bits: u32,
    env: &Env,
) -> Result<Value, EvalError> {
    if args.len() == 2
        && let (Some(b), Some(e)) = (to_float(&args[0], prec_bits), to_float(&args[1], prec_bits))
    {
        let bv = b.to_f64();
        let ev = e.to_f64();
        if bv < 0.0 && ev.fract() != 0.0 {
            // b^e = |b|^e * e^{i π e}
            let r = (-bv).powf(ev);
            let angle = std::f64::consts::PI * ev;
            return Ok(Value::Complex {
                re: r * angle.cos(),
                im: r * angle.sin(),
            });
        }
        return Ok(Value::Real(b.pow(e)));
    }
    crate::eval::apply_function(
        &super::eval(&Expr::Symbol(head.to_string()), env)?,
        args,
        env,
    )
}

/// Evaluate `Sqrt`, producing a complex principal value for negative arguments.
fn try_sqrt_float(
    head: &str,
    args: &[Value],
    prec_bits: u32,
    env: &Env,
) -> Result<Value, EvalError> {
    if args.len() == 1
        && let Some(f) = to_float(&args[0], prec_bits)
    {
        let fv = f.to_f64();
        if fv < 0.0 {
            return Ok(Value::Complex {
                re: 0.0,
                im: (-fv).sqrt(),
            });
        }
        return Ok(Value::Real(f.sqrt()));
    }
    crate::eval::apply_function(
        &super::eval(&Expr::Symbol(head.to_string()), env)?,
        args,
        env,
    )
}

/// Helper: convert a Value to an optional high-precision float.
fn to_float(v: &Value, prec_bits: u32) -> Option<Float> {
    match v {
        Value::Integer(n) => Some(Float::with_val(prec_bits, n)),
        Value::Real(r) => Some(Float::with_val(prec_bits, r)),
        Value::Rational(r) => {
            Some(Float::with_val(prec_bits, r.numer()) / Float::with_val(prec_bits, r.denom()))
        }
        Value::Root { coeffs, index } => {
            let roots = crate::polynomial::find_polynomial_roots(coeffs);
            if *index > 0 && *index <= roots.len() {
                let (re, im) = roots[*index - 1];
                if im.abs() < 1e-14 {
                    Some(Float::with_val(prec_bits, re))
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Numerically combine `Plus`/`Times` args when all are numeric, otherwise
/// rebuild the expression symbolically (evaluated args, e.g. `N[2 x + 2 y]`
/// → `2.0 x + 2.0 y`).
fn numeric_combine(
    head: &str,
    args: Vec<Value>,
    prec_bits: u32,
    env: &Env,
    multiply: bool,
) -> Result<Value, EvalError> {
    if !args.is_empty() && args.iter().all(|a| to_float(a, prec_bits).is_some()) {
        numeric_fold_op(
            &args,
            prec_bits,
            move |a, b| if multiply { a * b } else { a + b },
        )
    } else {
        super::apply_function(
            &super::eval(&Expr::Symbol(head.to_string()), env)?,
            &args,
            env,
        )
    }
}

/// Helper: fold a binary float operation over an all-numeric argument list.
fn numeric_fold_op<F>(args: &[Value], prec_bits: u32, op: F) -> Result<Value, EvalError>
where
    F: Fn(Float, Float) -> Float,
{
    let first = args.first().ok_or_else(|| {
        EvalError::Error("expected at least one argument for numeric fold".to_string())
    })?;
    let mut acc = to_float(first, prec_bits).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: first.type_name().to_string(),
    })?;
    for arg in &args[1..] {
        let f = to_float(arg, prec_bits).ok_or_else(|| EvalError::TypeError {
            expected: "Number".to_string(),
            got: arg.type_name().to_string(),
        })?;
        acc = op(acc, f);
    }
    Ok(Value::Real(acc))
}

/// Coerce a value to a high-precision float if possible.
fn coerce_to_float(v: Value, prec_bits: u32) -> Result<Value, EvalError> {
    match v {
        Value::Integer(n) => Ok(Value::Real(Float::with_val(prec_bits, n))),
        Value::Real(_) => Ok(v),
        Value::Rational(r) => Ok(Value::Real(
            Float::with_val(prec_bits, r.numer()) / Float::with_val(prec_bits, r.denom()),
        )),
        Value::Complex { re, im: 0.0 } => Ok(Value::Real(Float::with_val(prec_bits, re))),
        Value::Root { ref coeffs, index } => {
            let roots = crate::polynomial::find_polynomial_roots(coeffs);
            if index > 0 && index <= roots.len() {
                let (re, im) = roots[index - 1];
                if im.abs() < 1e-14 {
                    Ok(Value::Real(Float::with_val(prec_bits, re)))
                } else {
                    Ok(Value::Complex { re, im })
                }
            } else {
                Ok(v)
            }
        }
        _ => Ok(v),
    }
}
