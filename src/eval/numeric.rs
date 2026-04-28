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
        // Recursively evaluate calls at the requested precision.
        Expr::Call { head, args } => {
            if let Expr::Symbol(name) = head.as_ref() {
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
                    "Plus" => numeric_fold_op(evaluated_args, prec_bits, |a, b| a + b),
                    "Times" => numeric_fold_op(evaluated_args, prec_bits, |a, b| a * b),
                    "Power" if evaluated_args.len() == 2 => {
                        let (b, e) = (&evaluated_args[0], &evaluated_args[1]);
                        let bf = to_float(b, prec_bits);
                        let ef = to_float(e, prec_bits);
                        match (bf, ef) {
                            (Some(b), Some(e)) => Ok(Value::Real(b.pow(e))),
                            _ => crate::eval::apply_function(
                                &super::eval(head, env)?,
                                &evaluated_args,
                                env,
                            ),
                        }
                    }
                    "Divide" if evaluated_args.len() == 2 => {
                        let bf = to_float(&evaluated_args[0], prec_bits);
                        let ef = to_float(&evaluated_args[1], prec_bits);
                        match (bf, ef) {
                            (Some(a), Some(b)) => Ok(Value::Real(a / b)),
                            _ => crate::eval::apply_function(
                                &super::eval(head, env)?,
                                &evaluated_args,
                                env,
                            ),
                        }
                    }
                    "Sin" if evaluated_args.len() == 1 => {
                        if let Some(f) = to_float(&evaluated_args[0], prec_bits) {
                            Ok(Value::Real(f.sin()))
                        } else {
                            crate::eval::apply_function(
                                &super::eval(head, env)?,
                                &evaluated_args,
                                env,
                            )
                        }
                    }
                    "Cos" if evaluated_args.len() == 1 => {
                        if let Some(f) = to_float(&evaluated_args[0], prec_bits) {
                            Ok(Value::Real(f.cos()))
                        } else {
                            crate::eval::apply_function(
                                &super::eval(head, env)?,
                                &evaluated_args,
                                env,
                            )
                        }
                    }
                    "Log" if evaluated_args.len() == 1 => {
                        if let Some(f) = to_float(&evaluated_args[0], prec_bits) {
                            Ok(Value::Real(f.ln()))
                        } else {
                            crate::eval::apply_function(
                                &super::eval(head, env)?,
                                &evaluated_args,
                                env,
                            )
                        }
                    }
                    "Log" if evaluated_args.len() == 2 => {
                        let bf = to_float(&evaluated_args[0], prec_bits);
                        let ef = to_float(&evaluated_args[1], prec_bits);
                        match (bf, ef) {
                            (Some(a), Some(b)) => Ok(Value::Real(b.ln() / a.ln())),
                            _ => crate::eval::apply_function(
                                &super::eval(head, env)?,
                                &evaluated_args,
                                env,
                            ),
                        }
                    }
                    "Sqrt" if evaluated_args.len() == 1 => {
                        if let Some(f) = to_float(&evaluated_args[0], prec_bits) {
                            Ok(Value::Real(f.sqrt()))
                        } else {
                            crate::eval::apply_function(
                                &super::eval(head, env)?,
                                &evaluated_args,
                                env,
                            )
                        }
                    }
                    "Abs" if evaluated_args.len() == 1 => {
                        if let Some(f) = to_float(&evaluated_args[0], prec_bits) {
                            Ok(Value::Real(f.abs()))
                        } else {
                            crate::eval::apply_function(
                                &super::eval(head, env)?,
                                &evaluated_args,
                                env,
                            )
                        }
                    }
                    // For numeric evaluation of other functions, fall back to normal evaluation
                    // and coerce the result to a float.
                    _ => {
                        let v = crate::eval::apply_function(
                            &super::eval(head, env)?,
                            &evaluated_args,
                            env,
                        )?;
                        coerce_to_float(v, prec_bits)
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

/// Helper: fold a binary float operation over argument list.
fn numeric_fold_op<F>(args: Vec<Value>, prec_bits: u32, op: F) -> Result<Value, EvalError>
where
    F: Fn(Float, Float) -> Float,
{
    let mut iter = args.into_iter();
    let first = iter.next().ok_or_else(|| {
        EvalError::Error("expected at least one argument for numeric fold".to_string())
    })?;
    let mut acc = to_float(&first, prec_bits).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: first.type_name().to_string(),
    })?;
    for arg in iter {
        let f = to_float(&arg, prec_bits).ok_or_else(|| EvalError::TypeError {
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
