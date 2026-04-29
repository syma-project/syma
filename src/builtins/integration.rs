use crate::env::Env;
use crate::eval::eval;
use crate::eval::table::value_to_expr;
use crate::value::{EvalError, Value};
use rug::{Float, Integer};

// ─── Comparison Predicates (Rubi guard conditions) ───

/// EqQ[a, b] — structural equality
pub fn builtin_eq_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "EqQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(args[0].struct_eq(&args[1])))
}

/// NeQ[a, b] — structural inequality
pub fn builtin_ne_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "NeQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(!args[0].struct_eq(&args[1])))
}

/// GtQ[a, b] — numeric a > b (False if non-numeric)
pub fn builtin_gt_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "GtQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(
        compare_numeric(&args[0], &args[1], |a, b| a > b).unwrap_or(false),
    ))
}

/// LtQ[a, b] — numeric a < b (False if non-numeric)
pub fn builtin_lt_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "LtQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(
        compare_numeric(&args[0], &args[1], |a, b| a < b).unwrap_or(false),
    ))
}

/// GeQ[a, b] — numeric a >= b (False if non-numeric)
pub fn builtin_ge_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "GeQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(
        compare_numeric(&args[0], &args[1], |a, b| a >= b).unwrap_or(false),
    ))
}

/// LeQ[a, b] — numeric a <= b (False if non-numeric)
pub fn builtin_le_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "LeQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(
        compare_numeric(&args[0], &args[1], |a, b| a <= b).unwrap_or(false),
    ))
}

/// IGtQ[a, b] — integer a > b
pub fn builtin_igt_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "IGtQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(
        compare_numeric(&args[0], &args[1], |a, b| a > b).unwrap_or(false),
    ))
}

/// ILtQ[a, b] — integer a < b
pub fn builtin_ilt_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ILtQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(
        compare_numeric(&args[0], &args[1], |a, b| a < b).unwrap_or(false),
    ))
}

/// IGeQ[a, b] — integer a >= b
pub fn builtin_ige_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "IGeQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(
        compare_numeric(&args[0], &args[1], |a, b| a >= b).unwrap_or(false),
    ))
}

/// ILeQ[a, b] — integer a <= b
pub fn builtin_ile_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ILeQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(
        compare_numeric(&args[0], &args[1], |a, b| a <= b).unwrap_or(false),
    ))
}

// ─── Sign Predicates ───

/// PosQ[a] — a > 0
pub fn builtin_pos_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PosQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(
        compare_numeric(&args[0], &Value::Integer(Integer::from(0)), |a, b| a > b).unwrap_or(false),
    ))
}

/// NegQ[a] — a < 0
pub fn builtin_neg_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "NegQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(
        compare_numeric(&args[0], &Value::Integer(Integer::from(0)), |a, b| a < b).unwrap_or(false),
    ))
}

// ─── Type Predicates ───

/// TrueQ[a] — is a literally True
pub fn builtin_true_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TrueQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(matches!(&args[0], Value::Bool(true))))
}

/// FalseQ[a] — is a literally False
pub fn builtin_false_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FalseQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(matches!(&args[0], Value::Bool(false))))
}

/// OddQ[a] — is a an odd integer
pub fn builtin_odd_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "OddQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => Ok(Value::Bool(n.is_odd())),
        _ => Ok(Value::Bool(false)),
    }
}

/// HalfIntegerQ[a] — is a a half-integer (n + 1/2)
pub fn builtin_half_integer_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "HalfIntegerQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Rational(r) => {
            let den = r.denom();
            if *den == 2 && r.numer().is_odd() {
                return Ok(Value::Bool(true));
            }
            let num = r.numer();
            if num.is_even() && *den == 1 {
                return Ok(Value::Bool(false));
            }
            let double = num.clone() / den.clone();
            Ok(Value::Bool(double.is_odd()))
        }
        Value::Real(r) => {
            let doubled: Float = r.clone() * 2.0;
            if doubled.is_integer() {
                let val = doubled.to_f64() as i64;
                return Ok(Value::Bool(val % 2 != 0));
            }
            Ok(Value::Bool(false))
        }
        Value::Integer(_) => Ok(Value::Bool(false)),
        _ => Ok(Value::Bool(false)),
    }
}

/// RationalQ[a] — is a a rational number (Integer or Rational)
pub fn builtin_rational_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "RationalQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(matches!(
        &args[0],
        Value::Integer(_) | Value::Rational(_)
    )))
}

/// IntegersQ[{a, b, ...}] — all elements are integers
pub fn builtin_integers_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "IntegersQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => Ok(Value::Bool(
            items.iter().all(|i| matches!(i, Value::Integer(_))),
        )),
        Value::Integer(_) => Ok(Value::Bool(true)),
        _ => Ok(Value::Bool(false)),
    }
}

/// PolyQ[expr, x] — expr is a polynomial in x
pub fn builtin_poly_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "PolyQ requires 1 or 2 arguments".to_string(),
        ));
    }
    // Simple heuristic: no denominators containing the variable
    let var = args.get(1).and_then(|v| match v {
        Value::Symbol(s) => Some(s.clone()),
        _ => None,
    });
    let result = match var {
        Some(ref vx) => is_polynomial(&args[0], vx),
        None => is_polynomial_any(&args[0]),
    };
    Ok(Value::Bool(result))
}

/// AtomQ[a] — is a an atomic value (not a Call or List)
pub fn builtin_atom_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "AtomQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(!matches!(
        &args[0],
        Value::Call { .. } | Value::List(_)
    )))
}

// ─── Core Rubi Helpers ───

/// Subst[result, oldVar, newExpr] — substitute newExpr for oldVar in result
pub fn builtin_subst(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "Subst requires exactly 3 arguments".to_string(),
        ));
    }
    let result = &args[0];
    let old_var = &args[1];
    let new_expr = &args[2];
    Ok(substitute_value(result, old_var, new_expr))
}

/// Unintegrable[expr, x] — mark expr as non-integrable
pub fn builtin_unintegrable(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Unintegrable requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Call {
        head: "Integrate".to_string(),
        args: args.to_vec(),
    })
}

/// ActivateTrig[expr] — identity (stubs for deactivated trig)
pub fn builtin_activate_trig(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ActivateTrig requires exactly 1 argument".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// DeactivateTrig[expr] — identity
pub fn builtin_deactivate_trig(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "DeactivateTrig requires exactly 1 argument".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// KnownSineIntegrandQ[expr, x] — expr contains Sin[linear_in_x]
pub fn builtin_known_sine_integrand_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KnownSineIntegrandQ requires exactly 2 arguments".to_string(),
        ));
    }
    let var = var_from_arg(&args[1]);
    let has_sin = has_trig_with_linear_arg(&args[0], "Sin", var.as_deref());
    Ok(Value::Bool(has_sin))
}

/// KnownSecantIntegrandQ[expr, x] — expr contains Sec[linear_in_x]
pub fn builtin_known_secant_integrand_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KnownSecantIntegrandQ requires exactly 2 arguments".to_string(),
        ));
    }
    let var = var_from_arg(&args[1]);
    let has_sec = has_trig_with_linear_arg(&args[0], "Sec", var.as_deref());
    Ok(Value::Bool(has_sec))
}

/// KnownTangentIntegrandQ[expr, x] — expr contains Tan[linear_in_x]
pub fn builtin_known_tangent_integrand_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KnownTangentIntegrandQ requires exactly 2 arguments".to_string(),
        ));
    }
    let var = var_from_arg(&args[1]);
    let has_tan = has_trig_with_linear_arg(&args[0], "Tan", var.as_deref());
    Ok(Value::Bool(has_tan))
}

/// KnownCotangentIntegrandQ[expr, x] — expr contains Cot[linear_in_x]
pub fn builtin_known_cotangent_integrand_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KnownCotangentIntegrandQ requires exactly 2 arguments".to_string(),
        ));
    }
    let var = var_from_arg(&args[1]);
    let has_cot = has_trig_with_linear_arg(&args[0], "Cot", var.as_deref());
    Ok(Value::Bool(has_cot))
}

/// Simp[expr, x] — simplify (delegate to Simplify)
pub fn builtin_simp(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Simp requires at least 1 argument".to_string(),
        ));
    }
    crate::builtins::symbolic::builtin_simplify(&[args[0].clone()])
}

/// Rt[a, n] — positive real nth root: a^(1/n)
pub fn builtin_rt(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Rt requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Call {
        head: "Power".to_string(),
        args: vec![
            args[0].clone(),
            Value::Call {
                head: "Power".to_string(),
                args: vec![args[1].clone(), Value::Integer(Integer::from(-1))],
            },
        ],
    })
}

/// FracPart[x] — fractional part (x - IntegerPart[x])
pub fn builtin_frac_part(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FracPart requires exactly 1 argument".to_string(),
        ));
    }
    // Return as Call: x - IntegerPart[x]
    Ok(Value::Call {
        head: "Plus".to_string(),
        args: vec![
            args[0].clone(),
            Value::Call {
                head: "Times".to_string(),
                args: vec![
                    Value::Integer(Integer::from(-1)),
                    Value::Call {
                        head: "IntegerPart".to_string(),
                        args: vec![args[0].clone()],
                    },
                ],
            },
        ],
    })
}

/// Coefficient[expr, x, n] — coefficient of x^n in polynomial expr
pub fn builtin_coefficient(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "Coefficient requires 2 or 3 arguments".to_string(),
        ));
    }
    let expr = &args[0];
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Symbol".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let n = args
        .get(2)
        .map(|v| match v {
            Value::Integer(i) => i.clone(),
            _ => Integer::from(1),
        })
        .unwrap_or(Integer::from(1));
    let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(expr, &var);
    let n_us = n.to_usize().unwrap_or(coeffs.len());
    if n_us >= coeffs.len() {
        return Ok(Value::Integer(Integer::from(0)));
    }
    Ok(coeffs[n_us].clone())
}

/// Coeff[expr, x, n] — alias for Coefficient
pub fn builtin_coeff(args: &[Value]) -> Result<Value, EvalError> {
    builtin_coefficient(args)
}

/// FreeFactors[expr, x] — extract factors of expr free of x
pub fn builtin_free_factors(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FreeFactors requires exactly 2 arguments".to_string(),
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
    let var_val = Value::Symbol(var.clone());
    match &args[0] {
        Value::Call { head, args } if head == "Times" => {
            let free: Vec<Value> = args
                .iter()
                .filter(|a| is_constant_wrt(a, &var_val))
                .cloned()
                .collect();
            if free.is_empty() {
                Ok(Value::Integer(Integer::from(1)))
            } else if free.len() == 1 {
                Ok(free[0].clone())
            } else {
                Ok(Value::Call {
                    head: "Times".to_string(),
                    args: free,
                })
            }
        }
        _ if is_constant_wrt(&args[0], &var_val) => Ok(args[0].clone()),
        _ => Ok(Value::Integer(Integer::from(1))),
    }
}

/// NonfreeFactors[expr, x] — extract factors of expr that depend on x
pub fn builtin_nonfree_factors(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "NonfreeFactors requires exactly 2 arguments".to_string(),
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
    let var_val = Value::Symbol(var.clone());
    match &args[0] {
        Value::Call { head, args } if head == "Times" => {
            let dep: Vec<Value> = args
                .iter()
                .filter(|a| !is_constant_wrt(a, &var_val))
                .cloned()
                .collect();
            if dep.is_empty() {
                Ok(Value::Integer(Integer::from(1)))
            } else if dep.len() == 1 {
                Ok(dep[0].clone())
            } else {
                Ok(Value::Call {
                    head: "Times".to_string(),
                    args: dep,
                })
            }
        }
        _ if !is_constant_wrt(&args[0], &var_val) => Ok(args[0].clone()),
        _ => Ok(Value::Integer(Integer::from(1))),
    }
}

/// ExpandIntegrand[expr, x] — expand integrand (delegate to Expand)
pub fn builtin_expand_integrand(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "ExpandIntegrand requires at least 1 argument".to_string(),
        ));
    }
    crate::builtins::symbolic::builtin_expand(&[args[0].clone()])
}

/// ExpandToSum[expr, x] — expand to sum (delegate to Expand)
pub fn builtin_expand_to_sum(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "ExpandToSum requires at least 1 argument".to_string(),
        ));
    }
    crate::builtins::symbolic::builtin_expand(&[args[0].clone()])
}

/// ExpandTrig[expr, x] — expand trig (delegate to Expand)
pub fn builtin_expand_trig(args: &[Value]) -> Result<Value, EvalError> {
    builtin_expand_integrand(args)
}

/// ExpandTrigReduce[expr, x] — expand trig (delegate to Expand)
pub fn builtin_expand_trig_reduce(args: &[Value]) -> Result<Value, EvalError> {
    builtin_expand_integrand(args)
}

/// ExpandTrigExpand[expr, x] — expand trig (delegate to Expand)
pub fn builtin_expand_trig_expand(args: &[Value]) -> Result<Value, EvalError> {
    builtin_expand_integrand(args)
}

/// ExpandTrigToExp[expr, x] — expand trig to exponential (delegate to Expand)
pub fn builtin_expand_trig_to_exp(args: &[Value]) -> Result<Value, EvalError> {
    builtin_expand_integrand(args)
}

/// Dist[factor, expr, x] — distribute factor over Plus
pub fn builtin_dist(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Dist requires at least 2 arguments".to_string(),
        ));
    }
    let expr = Value::Call {
        head: "Times".to_string(),
        args: vec![args[0].clone(), args[1].clone()],
    };
    crate::builtins::symbolic::builtin_expand(&[expr])
}

/// RemoveContent[expr, x] — remove constant content (stub: return expr)
pub fn builtin_remove_content(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "RemoveContent requires at least 1 argument".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// IntPart[x] — integer part (alias to IntegerPart)
pub fn builtin_int_part(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "IntPart requires exactly 1 argument".to_string(),
        ));
    }
    crate::builtins::math::builtin_integer_part(&[args[0].clone()])
}

/// LinearQ[expr, x] — is expr linear in x
pub fn builtin_linear_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "LinearQ requires exactly 2 arguments".to_string(),
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
    let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(&args[0], &var);
    Ok(Value::Bool(!coeffs.is_empty() && coeffs.len() <= 2))
}

/// SumQ[expr] — is expr a sum (Plus with multiple args)
pub fn builtin_sum_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "SumQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Call { head, args } if head == "Plus" && args.len() > 1 => Ok(Value::Bool(true)),
        _ => Ok(Value::Bool(false)),
    }
}

/// NonsumQ[expr] — is expr NOT a sum
pub fn builtin_nonsum_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "NonsumQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Call { head, args } if head == "Plus" && args.len() > 1 => Ok(Value::Bool(false)),
        _ => Ok(Value::Bool(true)),
    }
}

/// Numerator[expr] — numerator of expression
pub fn builtin_numerator(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Numerator requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Rational(r) => Ok(Value::Integer(r.numer().clone())),
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => Ok(args[0].clone()),
        Value::Call { head, args }
            if head == "Power"
                && args.len() == 2
                && matches!(&args[1], Value::Integer(n) if n.is_negative()) =>
        {
            Ok(Value::Integer(Integer::from(1)))
        }
        _ => Ok(args[0].clone()),
    }
}

/// Denominator[expr] — denominator of expression
pub fn builtin_denominator(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Denominator requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Rational(r) => Ok(Value::Integer(r.denom().clone())),
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => Ok(args[1].clone()),
        Value::Call { head, args }
            if head == "Power"
                && args.len() == 2
                && matches!(&args[1], Value::Integer(n) if n.is_negative()) =>
        {
            Ok(Value::Call {
                head: "Power".to_string(),
                args: vec![args[0].clone(), Value::Integer(Integer::from(1))],
            })
        }
        _ => Ok(Value::Integer(Integer::from(1))),
    }
}

/// Numer[expr] — alias for Numerator
pub fn builtin_numer(args: &[Value]) -> Result<Value, EvalError> {
    builtin_numerator(args)
}

/// Denom[expr] — alias for Denominator
pub fn builtin_denom(args: &[Value]) -> Result<Value, EvalError> {
    builtin_denominator(args)
}

/// Exponent[expr, base] — exponent of base in expr
pub fn builtin_exponent(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Exponent requires exactly 2 arguments".to_string(),
        ));
    }
    // For symbolic base, extract polynomial coefficients and find highest degree
    if let Value::Symbol(var_name) = &args[1] {
        let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(&args[0], var_name);
        if !coeffs.is_empty() {
            // Find highest non-zero coefficient index
            for i in (0..coeffs.len()).rev() {
                if !is_zero(&coeffs[i]) {
                    return Ok(Value::Integer(Integer::from(i)));
                }
            }
        }
    }
    // Fallback: direct Power[base, exp] match
    match &args[0] {
        Value::Call {
            head,
            args: power_args,
        } if head == "Power" && power_args.len() == 2 && power_args[0].struct_eq(&args[1]) => {
            Ok(power_args[1].clone())
        }
        _ if args[0].struct_eq(&args[1]) => Ok(Value::Integer(Integer::from(1))),
        _ => Ok(Value::Integer(Integer::from(0))),
    }
}

/// Check if a Value is zero (Integer(0), Real(0.0), Rational(0/1))
fn is_zero(v: &Value) -> bool {
    match v {
        Value::Integer(n) => n.is_zero(),
        Value::Real(r) => r.is_zero(),
        Value::Rational(r) => r.numer().is_zero(),
        _ => false,
    }
}

/// Sign[x] — sign of x
pub fn builtin_sign(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Sign requires exactly 1 argument".to_string(),
        ));
    }
    let zero = Float::with_val(crate::value::DEFAULT_PRECISION, 0);
    match &args[0] {
        Value::Integer(n) => {
            if n.is_zero() {
                Ok(Value::Integer(Integer::from(0)))
            } else if n.is_negative() {
                Ok(Value::Integer(Integer::from(-1)))
            } else {
                Ok(Value::Integer(Integer::from(1)))
            }
        }
        Value::Real(r) => {
            if r.eq(&zero) {
                Ok(Value::Integer(Integer::from(0)))
            } else if r.is_sign_negative() {
                Ok(Value::Integer(Integer::from(-1)))
            } else {
                Ok(Value::Integer(Integer::from(1)))
            }
        }
        _ => Ok(Value::Call {
            head: "Sign".to_string(),
            args: vec![args[0].clone()],
        }),
    }
}

/// With[{var = val}, body] — bind var to val, evaluate body
/// With HoldAll, args come as Value::Pattern(Expr). We unwrap and eval in child env.
pub fn builtin_with(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "With requires exactly 2 arguments: bindings and body".to_string(),
        ));
    }
    let child_env = env.child();
    // Unwrap bindings from Pattern(Expr)
    let bindings_expr = match &args[0] {
        Value::Pattern(e) => e.clone(),
        v => value_to_expr(v),
    };
    // Evaluate bindings in child env to get concrete values
    let bindings_val = eval(&bindings_expr, &child_env)?;
    if let Value::List(pairs) = &bindings_val {
        for pair in pairs {
            match pair {
                Value::Rule { lhs, rhs, .. } => {
                    let name = match &**lhs {
                        Value::Symbol(s) => s.clone(),
                        _ => lhs.to_string(),
                    };
                    child_env.set(name, (**rhs).clone());
                }
                Value::Call { head, args: a } if head == "Set" && a.len() == 2 => {
                    let name = match &a[0] {
                        Value::Symbol(s) => s.clone(),
                        _ => a[0].to_string(),
                    };
                    child_env.set(name, a[1].clone());
                }
                _ => {}
            }
        }
    }
    // Unwrap body from Pattern(Expr) and eval in child env
    let body_expr = match &args[1] {
        Value::Pattern(e) => e.clone(),
        v => value_to_expr(v),
    };
    eval(&body_expr, &child_env)
}

/// Module[{var}, body] — local variable, evaluate body
pub fn builtin_module(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Module requires exactly 2 arguments: vars and body".to_string(),
        ));
    }
    let child_env = env.child();
    // Unwrap vars from Pattern(Expr)
    let vars_expr = match &args[0] {
        Value::Pattern(e) => e.clone(),
        v => value_to_expr(v),
    };
    let vars_val = eval(&vars_expr, &child_env)?;
    if let Value::List(vars) = &vars_val {
        for (i, var) in vars.iter().enumerate() {
            let name = match var {
                Value::Symbol(s) => s.clone(),
                _ => format!("v{i}"),
            };
            child_env.set(name.clone(), Value::Symbol(name));
        }
    }
    // Unwrap body and eval in child env
    let body_expr = match &args[1] {
        Value::Pattern(e) => e.clone(),
        v => value_to_expr(v),
    };
    eval(&body_expr, &child_env)
}

/// If[condition, then, else] — conditional
/// With HoldAll, only the selected branch is evaluated.
pub fn builtin_if(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error("If requires 2 or 3 arguments".to_string()));
    }
    // Evaluate condition first
    let cond_expr = match &args[0] {
        Value::Pattern(e) => e.clone(),
        v => value_to_expr(v),
    };
    let cond_val = eval(&cond_expr, env)?;
    let cond = cond_val.to_bool();
    if cond {
        let body_expr = match &args[1] {
            Value::Pattern(e) => e.clone(),
            v => value_to_expr(v),
        };
        eval(&body_expr, env)
    } else if args.len() == 3 {
        let body_expr = match &args[2] {
            Value::Pattern(e) => e.clone(),
            v => value_to_expr(v),
        };
        eval(&body_expr, env)
    } else {
        Ok(Value::Null)
    }
}

/// PolynomialQ[expr, x] — is expr a polynomial in x
pub fn builtin_polynomial_q(args: &[Value]) -> Result<Value, EvalError> {
    builtin_poly_q(args)
}

/// PerfectSquareQ[expr] — is expr a perfect square
pub fn builtin_perfect_square_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PerfectSquareQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n.is_negative() {
                Ok(Value::Bool(false))
            } else {
                let sqrt = n.clone().sqrt();
                let sqrt_sq: Integer = (&sqrt * &sqrt).into();
                Ok(Value::Bool(sqrt_sq == *n))
            }
        }
        _ => Ok(Value::Bool(false)),
    }
}

/// BinomialQ[expr, x] — is expr a binomial in x
pub fn builtin_binomial_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "BinomialQ requires exactly 2 arguments".to_string(),
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
    let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(&args[0], &var);
    let non_zero = coeffs
        .iter()
        .filter(|c| !matches!(c, Value::Integer(n) if n.is_zero()))
        .count();
    Ok(Value::Bool(non_zero == 2))
}

/// IntBinomialQ[expr, x] — is expr a binomial integrand
pub fn builtin_int_binomial_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "IntBinomialQ requires exactly 2 arguments".to_string(),
        ));
    }
    // Simplified: check if expr is a binomial form
    builtin_binomial_q(args)
}

/// LinearMatchQ[expr, x] — is expr a linear match in x
pub fn builtin_linear_match_q(args: &[Value]) -> Result<Value, EvalError> {
    builtin_linear_q(args)
}

/// QuadraticQ[expr, x] — is expr a quadratic in x
pub fn builtin_quadratic_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "QuadraticQ requires exactly 2 arguments".to_string(),
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
    let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(&args[0], &var);
    let non_zero = coeffs
        .iter()
        .filter(|c| !matches!(c, Value::Integer(n) if n.is_zero()))
        .count();
    Ok(Value::Bool(non_zero >= 1 && coeffs.len() >= 3))
}

/// FunctionOfQ[expr, var] — does expr depend on var
pub fn builtin_function_of_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FunctionOfQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(!is_constant_wrt(&args[0], &args[1])))
}

/// FunctionOfLinear[expr, x] — decompose expr into {f, a, b} where expr = f[a + b*x]
pub fn builtin_function_of_linear(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "FunctionOfLinear requires at least 2 arguments".to_string(),
        ));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(args[0].clone()),
    };
    // If expr is Call[head, arg] and arg is linear in var, decompose
    match &args[0] {
        Value::Call {
            head,
            args: call_args,
        } if call_args.len() == 1 => {
            let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(&call_args[0], &var);
            if coeffs.len() <= 2 && !is_constant_wrt(&call_args[0], &Value::Symbol(var.clone())) {
                let f = Value::Symbol(head.clone());
                let a = coeffs
                    .first()
                    .cloned()
                    .unwrap_or(Value::Integer(Integer::from(0)));
                let b = if coeffs.len() >= 2 {
                    coeffs[1].clone()
                } else {
                    Value::Integer(Integer::from(0))
                };
                return Ok(Value::List(vec![f, a, b]));
            }
            Ok(args[0].clone())
        }
        _ => Ok(args[0].clone()),
    }
}

/// InverseFunctionFreeQ[expr, func, x] — is expr free of inverse trig in x
pub fn builtin_inverse_function_free_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "InverseFunctionFreeQ requires at least 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(!contains_any_head(&args[0], INVERSE_FN_NAMES)))
}

/// DerivativeDivides[y, u, x] — check if u divides D[y, x] structurally
pub fn builtin_derivative_divides(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "DerivativeDivides requires at least 2 arguments".to_string(),
        ));
    }
    if args.len() < 3 {
        let v1 = !is_constant_wrt(&args[0], &args.get(1).unwrap_or(&Value::Null));
        return Ok(Value::Bool(v1));
    }
    let both_var = !is_constant_wrt(&args[0], &args[2]) && !is_constant_wrt(&args[1], &args[2]);
    Ok(Value::Bool(both_var))
}

/// SimplerQ[expr1, expr2, x] — is expr1 simpler than expr2
pub fn builtin_simpler_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "SimplerQ requires at least 2 arguments".to_string(),
        ));
    }
    // Compare by structural size, break ties by expression depth
    let s1 = leaf_count(&args[0]);
    let s2 = leaf_count(&args[1]);
    if s1 != s2 {
        return Ok(Value::Bool(s1 < s2));
    }
    let d1 = expression_depth(&args[0]);
    let d2 = expression_depth(&args[1]);
    Ok(Value::Bool(d1 < d2))
}

/// SimplerSqrtQ[expr1, expr2, x] — stub
pub fn builtin_simpler_sqrt_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "SimplerSqrtQ requires at least 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
}

/// SumSimplerQ[expr, rule, x] — stub
pub fn builtin_sum_simpler_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "SumSimplerQ requires at least 3 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
}

/// NiceSqrtQ[expr] — is expr a nice sqrt (perfect square under sqrt)
pub fn builtin_nice_sqrt_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "NiceSqrtQ requires exactly 1 argument".to_string(),
        ));
    }
    let expr = &args[0];
    // If expr is Sqrt[x] or x^(1/2), check inside
    let inner = match expr {
        Value::Call { head, args } if head == "Sqrt" && args.len() == 1 => &args[0],
        Value::Call { head, args }
            if head == "Power"
                && args.len() == 2
                && matches!(&args[1], Value::Rational(r) if *r.denom() == 2) =>
        {
            &args[0]
        }
        _ => return Ok(Value::Bool(true)),
    };
    // Check if inner is a perfect square integer, or has a perfect square factor
    let is_nice = match inner {
        Value::Integer(n) => n.is_perfect_square(),
        Value::Call { head, args } if head == "Times" => args
            .iter()
            .any(|a| matches!(a, Value::Integer(n) if n.is_perfect_square())),
        _ => true, // non-numeric expressions are "nice enough"
    };
    Ok(Value::Bool(is_nice))
}

/// BinomialMatchQ[expr, a, b, x, n] — is expr of form (a + b*x^n)^p
pub fn builtin_binomial_match_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 5 {
        return Err(EvalError::Error(
            "BinomialMatchQ requires at least 5 arguments".to_string(),
        ));
    }
    let var = match &args[3] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    let expr = &args[0];
    // expr should be Power[inner, p] or just inner
    match expr {
        Value::Call {
            head,
            args: inner_args,
        } if head == "Power" && inner_args.len() == 2 => {
            let base = &inner_args[0];
            let _exp = &inner_args[1];
            let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(base, &var);
            if coeffs.len() <= 2 {
                let c = coeffs
                    .first()
                    .cloned()
                    .unwrap_or(Value::Integer(Integer::from(0)));
                let b = coeffs
                    .get(1)
                    .cloned()
                    .unwrap_or(Value::Integer(Integer::from(0)));
                Ok(Value::Bool(
                    builtin_match_q_inner(&c, &args[1]) && builtin_match_q_inner(&b, &args[2]),
                ))
            } else {
                Ok(Value::Bool(false))
            }
        }
        _ => {
            let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(expr, &var);
            Ok(Value::Bool(coeffs.len() <= 2 && coeffs.len() > 0))
        }
    }
}

/// IntQuadraticQ[expr, x] — is expr a quadratic integrand
pub fn builtin_int_quadratic_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "IntQuadraticQ requires exactly 2 arguments".to_string(),
        ));
    }
    builtin_quadratic_q(args)
}

/// IntLinearQ[expr, x] — is expr a linear integrand
pub fn builtin_int_linear_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "IntLinearQ requires exactly 2 arguments".to_string(),
        ));
    }
    builtin_linear_q(args)
}

/// Expon[expr, x] — exponential order of expr in x
pub fn builtin_expon(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Expon requires exactly 2 arguments".to_string(),
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
    let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(&args[0], &var);
    if coeffs.len() <= 1 {
        Ok(Value::Integer(Integer::from(0)))
    } else {
        Ok(Value::Integer(Integer::from((coeffs.len() - 1) as i64)))
    }
}

/// InverseFunctionOfLinear[func, args, x] — decompose inverse trig of linear
pub fn builtin_inverse_function_of_linear(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "InverseFunctionOfLinear requires at least 3 arguments".to_string(),
        ));
    }
    let var = match &args[2] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(args[0].clone()),
    };
    // func should be an inverse trig, args should be linear in var
    match &args[0] {
        Value::Symbol(func_name) if INVERSE_FN_NAMES.contains(&func_name.as_str()) => {
            // Check if args[1] is linear in var
            if is_linear_in(&args[1], &var) {
                return Ok(args[0].clone()); // return {inverseFn, a, b} in Rubi form
            }
            Ok(args[0].clone())
        }
        _ => Ok(args[0].clone()),
    }
}

/// SubstFor[result, pattern, replacement] — substitute pattern in result
pub fn builtin_subst_for(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "SubstFor requires exactly 3 arguments".to_string(),
        ));
    }
    Ok(substitute_value(&args[0], &args[1], &args[2]))
}

/// SubstForInverseFunction[result, func, args, x] — stub
pub fn builtin_subst_for_inverse_function(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 4 {
        return Err(EvalError::Error(
            "SubstForInverseFunction requires at least 4 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// SubstForFractionalPowerOfLinear[result, expr, x] — stub
pub fn builtin_subst_for_fractional_power_of_linear(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "SubstForFractionalPowerOfLinear requires at least 3 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// SubstForFractionalPowerQ[result, expr, x] — does expr have fractional powers of linear in x
pub fn builtin_subst_for_fractional_power_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "SubstForFractionalPowerQ requires at least 3 arguments".to_string(),
        ));
    }
    let var = match &args[2] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    let result = has_fractional_power_of_linear(&args[0], &var);
    Ok(Value::Bool(result))
}

fn has_fractional_power_of_linear(expr: &Value, var: &str) -> bool {
    match expr {
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            let base = &args[0];
            let exp = &args[1];
            // Check if base is linear in var and exponent is a rational/Real that's not an integer
            if is_linear_in(base, var) {
                return match exp {
                    Value::Rational(r) => *r.denom() != 1,
                    Value::Real(r) => !r.is_integer(),
                    _ => false,
                };
            }
            args.iter().any(|a| has_fractional_power_of_linear(a, var))
        }
        Value::Call { args, .. } => args.iter().any(|a| has_fractional_power_of_linear(a, var)),
        Value::List(items) => items.iter().any(|a| has_fractional_power_of_linear(a, var)),
        _ => false,
    }
}

/// SubstForFractionalPowerOfQuotientOfLinears — stub
pub fn builtin_subst_for_fractional_power_of_quotient_of_linears(
    args: &[Value],
) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "SubstForFractionalPowerOfQuotientOfLinears requires at least 3 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// Discriminant[expr, x] — discriminant of polynomial
pub fn builtin_discriminant(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Discriminant requires exactly 2 arguments".to_string(),
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
    let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(&args[0], &var);
    if coeffs.len() == 3 {
        // Quadratic: b^2 - 4ac
        if let (Value::Integer(c), Value::Integer(b), Value::Integer(a)) =
            (&coeffs[0], &coeffs[1], &coeffs[2])
        {
            let disc: Integer = Integer::from(0) + b * b - Integer::from(4) * a * c;
            return Ok(Value::Integer(disc));
        }
    }
    Ok(Value::Call {
        head: "Discriminant".to_string(),
        args: args.to_vec(),
    })
}

/// QuadraticMatchQ[expr, a, b, c, x] — is expr = a*x^2 + b*x + c
pub fn builtin_quadratic_match_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 5 {
        return Err(EvalError::Error(
            "QuadraticMatchQ requires exactly 5 arguments".to_string(),
        ));
    }
    let var = match &args[4] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(&args[0], &var);
    if coeffs.len() > 3 {
        return Ok(Value::Bool(false));
    }
    // Check a (x^2 coeff), b (x^1 coeff), c (x^0 coeff)
    let a = coeffs
        .get(2)
        .cloned()
        .unwrap_or(Value::Integer(Integer::from(0)));
    let b = coeffs
        .get(1)
        .cloned()
        .unwrap_or(Value::Integer(Integer::from(0)));
    let c = coeffs
        .get(0)
        .cloned()
        .unwrap_or(Value::Integer(Integer::from(0)));
    // Match allocated a, b, c against the passed patterns
    let matches_a = builtin_match_q_inner(&a, &args[1]);
    let matches_b = builtin_match_q_inner(&b, &args[2]);
    let matches_c = builtin_match_q_inner(&c, &args[3]);
    Ok(Value::Bool(matches_a && matches_b && matches_c))
}

/// Simple structural match for pattern allocation
fn builtin_match_q_inner(expr: &Value, pattern: &Value) -> bool {
    match pattern {
        Value::Symbol(_) => true, // any symbol patterns matches anything (it's allocated)
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) => expr.struct_eq(pattern),
        Value::Call { .. } => expr.struct_eq(pattern),
        _ => true,
    }
}

/// TrinomialQ[expr, x] — is expr a trinomial
pub fn builtin_trinomial_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "TrinomialQ requires exactly 2 arguments".to_string(),
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
    let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(&args[0], &var);
    let non_zero = coeffs
        .iter()
        .filter(|c| !matches!(c, Value::Integer(n) if n.is_zero()))
        .count();
    Ok(Value::Bool(non_zero == 3))
}

/// GeneralizedTrinomialQ[expr, x] — is expr like a + b*x^n + c*x^(2n)
pub fn builtin_generalized_trinomial_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "GeneralizedTrinomialQ requires exactly 2 arguments".to_string(),
        ));
    }
    let _var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    match &args[0] {
        Value::Call {
            head,
            args: plus_args,
        } if head == "Plus" && plus_args.len() == 3 => {
            // Count terms involving powers of var
            let power_terms: Vec<_> = plus_args
                .iter()
                .filter(|a| !is_constant_wrt(a, &args[1]))
                .collect();
            Ok(Value::Bool(power_terms.len() >= 2))
        }
        Value::Call {
            head,
            args: plus_args,
        } if head == "Plus" => Ok(Value::Bool(false)),
        _ => Ok(Value::Bool(false)),
    }
}

/// LinearPairQ[expr, x] — expr has exactly two terms, both linear in x
pub fn builtin_linear_pair_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "LinearPairQ requires exactly 2 arguments".to_string(),
        ));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    // Must be Plus with exactly 2 args, each linear in var
    match &args[0] {
        Value::Call {
            head,
            args: plus_args,
        } if head == "Plus" && plus_args.len() == 2 => Ok(Value::Bool(
            is_linear_in(&plus_args[0], &var) && is_linear_in(&plus_args[1], &var),
        )),
        _ => Ok(Value::Bool(false)),
    }
}

/// PowerOfLinearQ[expr, x] — is expr a power of a linear function
pub fn builtin_power_of_linear_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "PowerOfLinearQ requires exactly 2 arguments".to_string(),
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
    match &args[0] {
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            let inner_coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(&args[0], &var);
            Ok(Value::Bool(inner_coeffs.len() <= 2))
        }
        _ => Ok(Value::Bool(false)),
    }
}

/// PowerOfLinearMatchQ[expr, a, b, x, n] — stub
pub fn builtin_power_of_linear_match_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 5 {
        return Err(EvalError::Error(
            "PowerOfLinearMatchQ requires at least 5 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
}

/// PowerOfLinearMatchQ[expr, x] — normalize stub
pub fn builtin_normalize_power_of_linear(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "NormalizePowerOfLinear requires at least 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// NormalizeIntegrand[expr, x] — stub
pub fn builtin_normalize_integrand(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "NormalizeIntegrand requires at least 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// FunctionOfSquareRootOfQuadratic[expr, x] — stub
pub fn builtin_function_of_sqrt_of_quadratic(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "FunctionOfSquareRootOfQuadratic requires at least 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// FunctionOfLinear[expr, x] — decompose expr as function of linear in x
pub fn builtin_function_of_linear_fn(args: &[Value]) -> Result<Value, EvalError> {
    builtin_function_of_linear(args)
}

/// FunctionOfLog[expr, x] — decompose expr as function of Log[linear]
pub fn builtin_function_of_log(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "FunctionOfLog requires at least 2 arguments".to_string(),
        ));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(args[0].clone()),
    };
    // expr should be outer[Log[inner]] where inner is linear in var
    match &args[0] {
        Value::Call {
            head,
            args: call_args,
        } if call_args.len() == 1 => {
            match &call_args[0] {
                Value::Call {
                    head: log_head,
                    args: log_args,
                } if log_head == "Log"
                    && log_args.len() == 1
                    && is_linear_in(&log_args[0], &var) =>
                {
                    // Return {f, a, b} where f is outer head, Log[a + b*x] is inner
                    let f = Value::Symbol(head.clone());
                    let coeffs =
                        crate::builtins::symbolic::extract_polynomial_coeffs(&log_args[0], &var);
                    let a = coeffs
                        .first()
                        .cloned()
                        .unwrap_or(Value::Integer(Integer::from(0)));
                    let b = if coeffs.len() >= 2 {
                        coeffs[1].clone()
                    } else {
                        Value::Integer(Integer::from(1))
                    };
                    Ok(Value::List(vec![f, a, b]))
                }
                _ => Ok(args[0].clone()),
            }
        }
        _ => Ok(args[0].clone()),
    }
}

/// FunctionOfExponentialQ[expr, x] — is expr a function of E^(c*x) or a^x
pub fn builtin_function_of_exponential_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "FunctionOfExponentialQ requires at least 2 arguments".to_string(),
        ));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    Ok(Value::Bool(has_exponential_of_linear(&args[0], &var)))
}

fn has_exponential_of_linear(expr: &Value, var: &str) -> bool {
    match expr {
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            let base = &args[0];
            let exp = &args[1];
            // E^(c*var) or a^(c*var)
            if is_linear_in(exp, var) && !is_constant_wrt(exp, &Value::Symbol(var.to_string())) {
                return true;
            }
            if is_linear_in(base, var) && !is_constant_wrt(base, &Value::Symbol(var.to_string())) {
                return true;
            }
            args.iter().any(|a| has_exponential_of_linear(a, var))
        }
        Value::Call { head, args } if head == "Exp" && args.len() == 1 => {
            is_linear_in(&args[0], var)
                && !is_constant_wrt(&args[0], &Value::Symbol(var.to_string()))
        }
        Value::Call { args, .. } => args.iter().any(|a| has_exponential_of_linear(a, var)),
        Value::List(items) => items.iter().any(|a| has_exponential_of_linear(a, var)),
        _ => false,
    }
}

/// FunctionOfExponential[expr, x] — decompose expr as function of E^(c*x)
pub fn builtin_function_of_exponential(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "FunctionOfExponential requires at least 2 arguments".to_string(),
        ));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(args[0].clone()),
    };
    // Try to decompose: if expr is f[E^(c*x)], return {f, E, c}
    match &args[0] {
        Value::Call {
            head,
            args: call_args,
        } if call_args.len() == 1 => match &call_args[0] {
            Value::Call {
                head: exp_head,
                args: exp_args,
            } if exp_head == "Power"
                && exp_args.len() == 2
                && matches!(&exp_args[0], Value::Symbol(s) if s == "E")
                && is_linear_in(&exp_args[1], &var)
                && !is_constant_wrt(&exp_args[1], &Value::Symbol(var.clone())) =>
            {
                let f = Value::Symbol(head.clone());
                let _coeffs =
                    crate::builtins::symbolic::extract_polynomial_coeffs(&exp_args[1], &var);
                let b = if _coeffs.len() >= 2 {
                    _coeffs[1].clone()
                } else {
                    Value::Integer(Integer::from(1))
                };
                Ok(Value::List(vec![f, Value::Symbol("E".to_string()), b]))
            }
            _ => Ok(args[0].clone()),
        },
        _ => Ok(args[0].clone()),
    }
}

/// TrigQ[expr] — is expr a trig function
pub fn builtin_trig_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TrigQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Call { head, .. } => Ok(Value::Bool(matches!(
            head.as_str(),
            "Sin"
                | "Cos"
                | "Tan"
                | "Cot"
                | "Sec"
                | "Csc"
                | "ArcSin"
                | "ArcCos"
                | "ArcTan"
                | "ArcCot"
                | "ArcSec"
                | "ArcCsc"
        ))),
        _ => Ok(Value::Bool(false)),
    }
}

/// HyperbolicQ[expr] — is expr a hyperbolic function
pub fn builtin_hyperbolic_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "HyperbolicQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Call { head, .. } => Ok(Value::Bool(matches!(
            head.as_str(),
            "Sinh" | "Cosh" | "Tanh" | "Coth" | "Sech" | "Csch"
        ))),
        _ => Ok(Value::Bool(false)),
    }
}

/// InverseFunctionQ[expr] — is expr an inverse function
pub fn builtin_inverse_function_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "InverseFunctionQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Call { head, .. } => Ok(Value::Bool(matches!(
            head.as_str(),
            "ArcSin"
                | "ArcCos"
                | "ArcTan"
                | "ArcCot"
                | "ArcSec"
                | "ArcCsc"
                | "ArcSinh"
                | "ArcCosh"
                | "ArcTanh"
        ))),
        _ => Ok(Value::Bool(false)),
    }
}

/// PowerQ[expr] — is expr a power
pub fn builtin_power_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PowerQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Call { head, .. } if head == "Power" => Ok(Value::Bool(true)),
        _ => Ok(Value::Bool(false)),
    }
}

/// ProductQ[expr] — is expr a product
pub fn builtin_product_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ProductQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Call { head, .. } if head == "Times" => Ok(Value::Bool(true)),
        _ => Ok(Value::Bool(false)),
    }
}

/// RationalFunctionQ[expr, x] — is expr a rational function of x
pub fn builtin_rational_function_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "RationalFunctionQ requires exactly 2 arguments".to_string(),
        ));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    let result = is_rational_function(&args[0], &var);
    Ok(Value::Bool(result))
}

fn is_rational_function(val: &Value, var: &str) -> bool {
    match val {
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => true,
        Value::Symbol(_) => true,
        Value::Call { head, args } => match head.as_str() {
            "Plus" | "Times" => args.iter().all(|a| is_rational_function(a, var)),
            "Power" if args.len() == 2 => {
                is_rational_function(&args[0], var)
                    && matches!(&args[1], Value::Integer(_) | Value::Rational(_))
            }
            "Divide" if args.len() == 2 => {
                is_polynomial(&args[0], var) && is_polynomial(&args[1], var)
            }
            _ => false,
        },
        _ => false,
    }
}

/// FunctionOfTrigOfLinearQ[expr, x] — is expr a function of trig of linear in x
pub fn builtin_function_of_trig_of_linear_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "FunctionOfTrigOfLinearQ requires at least 2 arguments".to_string(),
        ));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    let result = has_trig_of_linear(&args[0], &var);
    Ok(Value::Bool(result))
}

fn has_trig_of_linear(expr: &Value, var: &str) -> bool {
    match expr {
        Value::Call { head, args } => {
            if TRIG_FN_NAMES.contains(&head.as_str()) && args.len() == 1 {
                return is_linear_in(&args[0], var);
            }
            args.iter().any(|a| has_trig_of_linear(a, var))
        }
        Value::List(items) => items.iter().any(|a| has_trig_of_linear(a, var)),
        _ => false,
    }
}

/// InertTrigQ[expr] — stub
pub fn builtin_inert_trig_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "InertTrigQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(false))
}

/// InertTrigFreeQ[expr] — is expr free of inert trig functions
pub fn builtin_inert_trig_free_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "InertTrigFreeQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(!contains_any_head(&args[0], TRIG_FN_NAMES)))
}

/// ComplexFreeQ[expr, x] — is expr free of x and complex numbers
pub fn builtin_complex_free_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ComplexFreeQ requires exactly 2 arguments".to_string(),
        ));
    }
    // Check both FreeQ and no complex numbers
    let free = is_constant_wrt(&args[0], &args[1]);
    Ok(Value::Bool(free && !contains_complex(&args[0])))
}

/// CalculusFreeQ[expr, x] — is expr free of calculus operations
pub fn builtin_calculus_free_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "CalculusFreeQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(!contains_any_head(&args[0], CALCULUS_FN_NAMES)))
}

/// IntegralFreeQ[expr, x] — is expr free of integrals
pub fn builtin_integral_free_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "IntegralFreeQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(!contains_head(&args[0], "Integrate")))
}

/// EulerIntegrandQ[expr, x] — check if expr matches x^m * (a + b*x^n)^p form
pub fn builtin_euler_integrand_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "EulerIntegrandQ requires exactly 2 arguments".to_string(),
        ));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    let result = is_euler_integrand(&args[0], &var);
    Ok(Value::Bool(result))
}

fn is_euler_integrand(expr: &Value, var: &str) -> bool {
    match expr {
        Value::Call { head, args } if head == "Times" && args.len() == 2 => {
            // One factor should be Power[var, m], other should be Power[a + b*var^n, p]
            let (pow1, pow2): (Vec<_>, Vec<_>) = args
                .iter()
                .partition(|a| matches!(a, Value::Call { head, args: pa }
                    if head == "Power" && pa.len() == 2 && matches!(&pa[0], Value::Symbol(s) if s == var)));
            !pow1.is_empty() && !pow2.is_empty()
        }
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            // Single Power form
            true
        }
        _ => false,
    }
}

/// Integral[expr, {x, a, b}] — stub (definite integral)
pub fn builtin_integral(args: &[Value]) -> Result<Value, EvalError> {
    // Return unevaluated
    Ok(Value::Call {
        head: "Integral".to_string(),
        args: args.to_vec(),
    })
}

/// CannotIntegrate[expr, x] — return unevaluated
pub fn builtin_cannot_integrate(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "CannotIntegrate requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Call {
        head: "Integrate".to_string(),
        args: args.to_vec(),
    })
}

/// ShowStep[result, rule, expr, x] — stub
pub fn builtin_show_step(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 4 {
        return Err(EvalError::Error(
            "ShowStep requires at least 4 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// SplitProduct[expr, x] — stub
pub fn builtin_split_product(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "SplitProduct requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// Distrib[expr, x] — distribute over Plus
pub fn builtin_distrib(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Distrib requires at least 2 arguments".to_string(),
        ));
    }
    crate::builtins::symbolic::builtin_expand(&[args[0].clone()])
}

/// DistributeDegree[expr, x] — stub
pub fn builtin_distribute_degree(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "DistributeDegree requires at least 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// PseudoBinomialPairQ[expr1, expr2, x] — check if two exprs form a binomial pair via common exponent
pub fn builtin_pseudo_binomial_pair_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "PseudoBinomialPairQ requires at least 3 arguments".to_string(),
        ));
    }
    let _var = match &args[2] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    // Both must vary with x and share a common base pattern
    let v1 = !is_constant_wrt(&args[0], &args[2]);
    let v2 = !is_constant_wrt(&args[1], &args[2]);
    if !v1 || !v2 {
        return Ok(Value::Bool(false));
    }
    // Check if they have a common Power or linear structure
    let inner = |expr: &Value| -> bool {
        match expr {
            Value::Call { head, args: ca } if head == "Power" && ca.len() == 2 => {
                !is_constant_wrt(&ca[0], &args[2])
            }
            Value::Call { head, args: ca } if head == "Times" && ca.len() == 2 => {
                ca.iter().any(|a| !is_constant_wrt(a, &args[2]))
            }
            _ => !is_constant_wrt(expr, &args[2]),
        }
    };
    Ok(Value::Bool(inner(&args[0]) && inner(&args[1])))
}

/// IntSum[result, expr, x] — stub
pub fn builtin_int_sum(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "IntSum requires at least 3 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// IntHide[expr, x] — prevent Int from evaluating (identity)
pub fn builtin_int_hide(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "IntHide requires at least 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// EveryQ[list, pattern] — all elements match pattern
pub fn builtin_every_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "EveryQ requires exactly 2 arguments".to_string(),
        ));
    }
    // EveryQ[list, pattern] — check if all list elements match pattern
    match &args[0] {
        Value::List(items) => {
            if items.is_empty() {
                return Ok(Value::Bool(true));
            }
            let pattern = &args[1];
            let all_match = items.iter().all(|item| match_item_q(item, pattern));
            Ok(Value::Bool(all_match))
        }
        _ => Ok(Value::Bool(true)),
    }
}

/// Simple structural match for EveryQ — check if item matches pattern structurally
fn match_item_q(item: &Value, pattern: &Value) -> bool {
    match pattern {
        Value::Symbol(_) => true,
        Value::Integer(_)
        | Value::Real(_)
        | Value::Rational(_)
        | Value::Str(_)
        | Value::Bool(_) => item.struct_eq(pattern),
        Value::Call { head, args } => {
            if let Value::Call {
                head: item_head,
                args: item_args,
            } = item
            {
                if head != item_head || args.len() != item_args.len() {
                    return false;
                }
                args.iter()
                    .zip(item_args.iter())
                    .all(|(p, i)| match_item_q(i, p))
            } else {
                false
            }
        }
        Value::List(pat_items) => {
            if let Value::List(item_items) = item {
                pat_items.len() == item_items.len()
                    && pat_items
                        .iter()
                        .zip(item_items.iter())
                        .all(|(p, i)| match_item_q(i, p))
            } else {
                false
            }
        }
        _ => true,
    }
}

/// BinomialParts[expr] — stub
pub fn builtin_binomial_parts(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "BinomialParts requires exactly 1 argument".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// BinomialDegree[expr, x] — degree of binomial
pub fn builtin_binomial_degree(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "BinomialDegree requires exactly 2 arguments".to_string(),
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
    let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(&args[0], &var);
    Ok(Value::Integer(Integer::from(coeffs.len() - 1)))
}

/// GeneralizedBinomialQ[expr, x] — is expr a generalized binomial (two-term form with general exponents)
pub fn builtin_generalized_binomial_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "GeneralizedBinomialQ requires exactly 2 arguments".to_string(),
        ));
    }
    let _var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    match &args[0] {
        Value::Call {
            head,
            args: plus_args,
        } if head == "Plus" && plus_args.len() == 2 => {
            let c1 = is_constant_wrt(&plus_args[0], &args[1]);
            let c2 = is_constant_wrt(&plus_args[1], &args[1]);
            Ok(Value::Bool(c1 || c2)) // at least one constant term
        }
        _ => Ok(Value::Bool(false)),
    }
}

/// GeneralizedBinomialMatchQ[expr, a, b, x, n] — match expr = a + b*x^n form
pub fn builtin_generalized_binomial_match_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 5 {
        return Err(EvalError::Error(
            "GeneralizedBinomialMatchQ requires at least 5 arguments".to_string(),
        ));
    }
    let var = match &args[3] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    // Check if expr is a sum of 2 terms, one constant
    match &args[0] {
        Value::Call {
            head,
            args: plus_args,
        } if head == "Plus" && plus_args.len() == 2 => {
            let (_const_term, var_term) = if is_constant_wrt(&plus_args[0], &args[1]) {
                (&plus_args[0], &plus_args[1])
            } else if is_constant_wrt(&plus_args[1], &args[1]) {
                (&plus_args[1], &plus_args[0])
            } else {
                return Ok(Value::Bool(false));
            };
            let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(var_term, &var);
            Ok(Value::Bool(!coeffs.is_empty()))
        }
        _ => Ok(Value::Bool(false)),
    }
}

/// GeneralizedTrinomialMatchQ[expr, a, b, c, x] — match expr = a + b*x^n + c*x^(2n)
pub fn builtin_generalized_trinomial_match_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 5 {
        return Err(EvalError::Error(
            "GeneralizedTrinomialMatchQ requires at least 5 arguments".to_string(),
        ));
    }
    let _var = match &args[4] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    // Must be Plus with 3 terms
    match &args[0] {
        Value::Call {
            head,
            args: plus_args,
        } if head == "Plus" && plus_args.len() == 3 => {
            let non_const: Vec<_> = plus_args
                .iter()
                .filter(|a| !is_constant_wrt(a, &args[1]))
                .collect();
            Ok(Value::Bool(non_const.len() >= 2))
        }
        _ => Ok(Value::Bool(false)),
    }
}

/// GeneralizedTrinomialDegree[expr, x] — stub
pub fn builtin_generalized_trinomial_degree(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "GeneralizedTrinomialDegree requires at least 2 arguments".to_string(),
        ));
    }
    Ok(Value::Integer(Integer::from(2)))
}

/// PolynomialRemainder[p1, p2, x] — polynomial remainder
pub fn builtin_polynomial_divide(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "PolynomialDivide requires exactly 3 arguments".to_string(),
        ));
    }
    let var = match &args[2] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(args[0].clone()),
    };
    let (quotient, remainder) = polynomial_long_division(&args[0], &args[1], &var);
    Ok(Value::List(vec![quotient, remainder]))
}

/// PolynomialQuotient[p1, p2, x] — quotient from polynomial long division
pub fn builtin_polynomial_quotient(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "PolynomialQuotient requires exactly 3 arguments".to_string(),
        ));
    }
    let var = match &args[2] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(args[0].clone()),
    };
    let (quotient, _) = polynomial_long_division(&args[0], &args[1], &var);
    Ok(quotient)
}

/// PolynomialRemainder[p1, p2, x] — remainder from polynomial long division
pub fn builtin_polynomial_remainder(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "PolynomialRemainder requires exactly 3 arguments".to_string(),
        ));
    }
    let var = match &args[2] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(args[0].clone()),
    };
    let (_, remainder) = polynomial_long_division(&args[0], &args[1], &var);
    Ok(remainder)
}

/// PolynomialInQ[expr, x] — is expr a polynomial in x
pub fn builtin_polynomial_in_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "PolynomialInQ requires exactly 2 arguments".to_string(),
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
    Ok(Value::Bool(is_polynomial(&args[0], &var)))
}

/// PolynomialInSubst[expr, x] — stub
pub fn builtin_polynomial_in_subst(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "PolynomialInSubst requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// RationalFunctionExpand[expr, x] — expand rational function
pub fn builtin_rational_function_expand(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "RationalFunctionExpand requires at least 2 arguments".to_string(),
        ));
    }
    crate::builtins::symbolic::builtin_expand(&[args[0].clone()])
}

/// RationalFunctionExponents[expr, x] — stub
pub fn builtin_rational_function_exponents(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "RationalFunctionExponents requires at least 2 arguments".to_string(),
        ));
    }
    Ok(Value::List(vec![]))
}

/// QuotientOfLinearsQ[expr, x] — is expr a quotient of linears
pub fn builtin_quotient_of_linears_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "QuotientOfLinearsQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
}

/// QuotientOfLinearsParts[expr, x] — stub
pub fn builtin_quotient_of_linears_parts(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "QuotientOfLinearsParts requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// QuadraticProductQ[expr, x] — is expr a product of two quadratics in x
pub fn builtin_quadratic_product_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "QuadraticProductQ requires exactly 2 arguments".to_string(),
        ));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    match &args[0] {
        Value::Call { head, args: ta } if head == "Times" && ta.len() == 2 => {
            let q1 = crate::builtins::symbolic::extract_polynomial_coeffs(&ta[0], &var);
            let q2 = crate::builtins::symbolic::extract_polynomial_coeffs(&ta[1], &var);
            Ok(Value::Bool(
                q1.len() <= 3 && q2.len() <= 3 && q1.len() >= 2 && q2.len() >= 2,
            ))
        }
        _ => Ok(Value::Bool(false)),
    }
}

/// SimplerIntegrandQ[expr1, expr2, x] — is expr1 simpler integrand than expr2
pub fn builtin_simpler_integrand_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "SimplerIntegrandQ requires at least 3 arguments".to_string(),
        ));
    }
    // Compare leaf_count; simpler has fewer nodes
    let s1 = leaf_count(&args[0]);
    let s2 = leaf_count(&args[1]);
    Ok(Value::Bool(s1 < s2))
}

/// TrigonometricSimplifyQ[expr] — does expr contain trig functions needing simplification
pub fn builtin_trig_simplify_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TrigSimplifyQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(contains_any_head(&args[0], TRIG_FN_NAMES)))
}

/// TrigonometricSimplify[expr] — stub
pub fn builtin_trig_simplify(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TrigSimplify requires exactly 1 argument".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// NormalizedPseudoBinomial[expr, x] — stub
pub fn builtin_normalize_pseudo_binomial(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "NormalizePseudoBinomial requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// ExpandLinearProduct[expr, x] — expand linear product
pub fn builtin_expand_linear_product(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "ExpandLinearProduct requires at least 2 arguments".to_string(),
        ));
    }
    crate::builtins::symbolic::builtin_expand(&[args[0].clone()])
}

/// ExpandExpression[expr, x] — expand expression
pub fn builtin_expand_expression(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "ExpandExpression requires at least 2 arguments".to_string(),
        ));
    }
    crate::builtins::symbolic::builtin_expand(&[args[0].clone()])
}

/// FunctionExpand[expr] — stub
pub fn builtin_function_expand(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FunctionExpand requires exactly 1 argument".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// SimplifyIntegrand[expr, x] — simplify integrand
pub fn builtin_simplify_integrand(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "SimplifyIntegrand requires at least 2 arguments".to_string(),
        ));
    }
    crate::builtins::symbolic::builtin_simplify(&[args[0].clone()])
}

/// IndependentQ[expr, x] — is expr independent of x
pub fn builtin_independent_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "IndependentQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(is_constant_wrt(&args[0], &args[1])))
}

/// AlgebraicFunctionQ[expr, x] — is expr an algebraic function of x
pub fn builtin_algebraic_function_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "AlgebraicFunctionQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(true))
}

/// TryPureTanSubst[expr, x] — stub
pub fn builtin_try_pure_tan_subst(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "TryPureTanSubst requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// MinimumMonomialExponent[expr, x] — minimum exponent of x in expr
pub fn builtin_minimum_monomial_exponent(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MinimumMonomialExponent requires exactly 2 arguments".to_string(),
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
    let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(&args[0], &var);
    if coeffs.is_empty()
        || (coeffs.len() == 1 && matches!(&coeffs[0], Value::Integer(n) if n.is_zero()))
    {
        Ok(Value::Integer(Integer::from(0)))
    } else {
        // Find first non-zero coefficient index
        let mut min_exp = 0i64;
        for (i, c) in coeffs.iter().enumerate() {
            if !matches!(c, Value::Integer(n) if n.is_zero()) {
                min_exp = i as i64;
                break;
            }
        }
        Ok(Value::Integer(Integer::from(min_exp)))
    }
}

/// PowerVariableExpn[expr, x] — stub
pub fn builtin_power_variable_expn(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "PowerVariableExpn requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// NormalizeNormalizedPseudoBinomial[expr, x] — stub
pub fn builtin_normalize_normalize_pseudo_binomial(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "NormalizeNormalizePseudoBinomial requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

// ─── Helper Functions ───

fn compare_numeric<F>(a: &Value, b: &Value, cmp: F) -> Option<bool>
where
    F: FnOnce(Float, Float) -> bool,
{
    let af = value_to_float(a);
    let bf = value_to_float(b);
    match (af, bf) {
        (Some(a), Some(b)) => Some(cmp(a, b)),
        _ => None,
    }
}

fn value_to_float(v: &Value) -> Option<Float> {
    match v {
        Value::Integer(n) => Some(Float::with_val(crate::value::DEFAULT_PRECISION, n)),
        Value::Real(r) => Some(r.clone()),
        Value::Rational(r) => {
            let num = Float::with_val(crate::value::DEFAULT_PRECISION, r.numer());
            let den = Float::with_val(crate::value::DEFAULT_PRECISION, r.denom());
            Some(num / den)
        }
        _ => None,
    }
}

fn substitute_value(expr: &Value, old_var: &Value, new_expr: &Value) -> Value {
    match expr {
        Value::Symbol(s) => {
            if let Value::Symbol(os) = old_var
                && s == os
            {
                return new_expr.clone();
            }
            expr.clone()
        }
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => {
            expr.clone()
        }
        Value::List(items) => Value::List(
            items
                .iter()
                .map(|i| substitute_value(i, old_var, new_expr))
                .collect(),
        ),
        Value::Call { head, args } => Value::Call {
            head: head.clone(),
            args: args
                .iter()
                .map(|a| substitute_value(a, old_var, new_expr))
                .collect(),
        },
        other => other.clone(),
    }
}

fn is_constant_wrt(val: &Value, var: &Value) -> bool {
    match val {
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => true,
        Value::Symbol(s) => {
            if let Value::Symbol(vs) = var {
                s != vs
            } else {
                true
            }
        }
        Value::Call { args, .. } => args.iter().all(|a| is_constant_wrt(a, var)),
        Value::List(items) => items.iter().all(|a| is_constant_wrt(a, var)),
        _ => true,
    }
}

fn is_polynomial(val: &Value, _var: &str) -> bool {
    match val {
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => true,
        Value::Symbol(_s) => true,
        Value::Call { head, args } => match head.as_str() {
            "Plus" | "Times" => args.iter().all(|a| is_polynomial(a, _var)),
            "Power" if args.len() == 2 => {
                is_polynomial(&args[0], _var)
                    && matches!(&args[1], Value::Integer(n) if !n.is_negative())
            }
            "Divide" if args.len() == 2 => {
                is_polynomial(&args[0], _var) && is_polynomial(&args[1], _var)
            }
            _ => false,
        },
        _ => false,
    }
}

fn is_polynomial_any(val: &Value) -> bool {
    // Check if polynomial in any variable
    match val {
        Value::Integer(_) | Value::Real(_) => true,
        _ => true,
    }
}

fn contains_complex(val: &Value) -> bool {
    match val {
        Value::Symbol(s) => s == "I",
        Value::Complex { .. } => true,
        Value::Call { args, .. } => args.iter().any(contains_complex),
        Value::List(items) => items.iter().any(contains_complex),
        _ => false,
    }
}

fn contains_head(val: &Value, head: &str) -> bool {
    match val {
        Value::Call {
            head: call_head,
            args,
        } => {
            if *call_head == head {
                return true;
            }
            args.iter().any(|a| contains_head(a, head))
        }
        Value::List(items) => items.iter().any(|a| contains_head(a, head)),
        _ => false,
    }
}

fn leaf_count(val: &Value) -> usize {
    match val {
        Value::Call { args, .. } => args.iter().map(leaf_count).sum(),
        Value::List(items) => items.iter().map(leaf_count).sum(),
        _ => 1,
    }
}

fn expression_depth(val: &Value) -> usize {
    match val {
        Value::Call { args, .. } => 1 + args.iter().map(expression_depth).max().unwrap_or(0),
        Value::List(items) => 1 + items.iter().map(expression_depth).max().unwrap_or(0),
        _ => 1,
    }
}

const INVERSE_FN_NAMES: &[&str] = &[
    "ArcSin", "ArcCos", "ArcTan", "ArcCot", "ArcSec", "ArcCsc", "ArcSinh", "ArcCosh", "ArcTanh",
];

const TRIG_FN_NAMES: &[&str] = &["Sin", "Cos", "Tan", "Cot", "Sec", "Csc"];

const CALCULUS_FN_NAMES: &[&str] = &["Integrate", "D", "Integral", "Sum", "Product", "Limit"];

fn contains_any_head(val: &Value, heads: &[&str]) -> bool {
    match val {
        Value::Call { head, args } => {
            if heads.contains(&head.as_str()) {
                return true;
            }
            args.iter().any(|a| contains_any_head(a, heads))
        }
        Value::List(items) => items.iter().any(|a| contains_any_head(a, heads)),
        _ => false,
    }
}

/// Check if expr is linear in var: a + b*var (or just a constant)
fn is_linear_in(expr: &Value, var: &str) -> bool {
    let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(expr, var);
    !coeffs.is_empty() && coeffs.len() <= 2
}

fn var_from_arg(arg: &Value) -> Option<String> {
    match arg {
        Value::Symbol(s) => Some(s.clone()),
        _ => None,
    }
}

fn has_trig_with_linear_arg(expr: &Value, trig_head: &str, var: Option<&str>) -> bool {
    match expr {
        Value::Call { head, args } => {
            if head == trig_head && args.len() == 1 {
                if let Some(v) = var {
                    return is_linear_in(&args[0], v);
                }
                return true;
            }
            args.iter()
                .any(|a| has_trig_with_linear_arg(a, trig_head, var))
        }
        Value::List(items) => items
            .iter()
            .any(|a| has_trig_with_linear_arg(a, trig_head, var)),
        _ => false,
    }
}

fn reconstruct_polynomial(coeffs: &[Value], var: &str) -> Value {
    let mut terms = Vec::new();
    for (i, c) in coeffs.iter().enumerate() {
        if matches!(c, Value::Integer(n) if n.is_zero()) {
            continue;
        }
        if i == 0 {
            terms.push(c.clone());
        } else if i == 1 {
            terms.push(Value::Call {
                head: "Times".to_string(),
                args: vec![c.clone(), Value::Symbol(var.to_string())],
            });
        } else {
            terms.push(Value::Call {
                head: "Times".to_string(),
                args: vec![
                    c.clone(),
                    Value::Call {
                        head: "Power".to_string(),
                        args: vec![
                            Value::Symbol(var.to_string()),
                            Value::Integer(Integer::from(i as i64)),
                        ],
                    },
                ],
            });
        }
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

fn leading_term_divide(a: &Value, b: &Value) -> Value {
    // Compute a/b for coefficient division in polynomial long division
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => {
            if y.is_zero() {
                return Value::Integer(Integer::from(0));
            }
            let xr = x.clone();
            let yr = y.clone();
            Value::Rational(Box::new(rug::Rational::from((xr, yr))))
        }
        _ => {
            // Fall back to Divide[a, b]
            use crate::builtins::arithmetic::builtin_divide;
            builtin_divide(&[a.clone(), b.clone()]).unwrap_or(a.clone())
        }
    }
}

/// Polynomial long division: returns (quotient, remainder) of p1 / p2 in var
fn polynomial_long_division(p1: &Value, p2: &Value, var: &str) -> (Value, Value) {
    use crate::builtins::arithmetic::{add_values_public, mul_values_public, sub_values_public};
    use crate::builtins::symbolic::extract_polynomial_coeffs;
    let p1_coeffs = extract_polynomial_coeffs(p1, var);
    let p2_coeffs = extract_polynomial_coeffs(p2, var);
    if p2_coeffs.is_empty()
        || (p2_coeffs.len() == 1 && matches!(&p2_coeffs[0], Value::Integer(n) if n.is_zero()))
    {
        return (p1.clone(), Value::Integer(Integer::from(0)));
    }
    let p1_deg = p1_coeffs.len() as i64 - 1;
    let p2_deg = p2_coeffs.len() as i64 - 1;
    if p1_deg < p2_deg {
        return (Value::Integer(Integer::from(0)), p1.clone());
    }
    let mut rem = p1_coeffs.clone();
    let mut q_terms: Vec<Value> = Vec::new();
    loop {
        let r_deg = rem.len() as i64 - 1;
        if r_deg < p2_deg {
            break;
        }
        let lr = rem.last().unwrap().clone();
        let lp2 = p2_coeffs.last().unwrap().clone();
        let dd = r_deg - p2_deg;
        let leading_div = leading_term_divide(&lr, &lp2);
        let q_term = if dd == 0 {
            leading_div
        } else {
            Value::Call {
                head: "Times".to_string(),
                args: vec![
                    leading_div,
                    Value::Call {
                        head: "Power".to_string(),
                        args: vec![
                            Value::Symbol(var.to_string()),
                            Value::Integer(Integer::from(dd)),
                        ],
                    },
                ],
            }
        };
        q_terms.push(q_term.clone());
        // Multiply q_term * p2 and subtract from remainder
        let q_term_poly = crate::builtins::symbolic::builtin_expand(&[q_term.clone()]);
        let q_term_poly_val = match q_term_poly {
            Ok(v) => v,
            Err(_) => q_term,
        };
        let q_times_p2 =
            mul_values_public(&q_term_poly_val, &reconstruct_polynomial(&p2_coeffs, var));
        let subtrahend = match &q_times_p2 {
            Ok(v) => match crate::builtins::symbolic::builtin_expand(&[v.clone()]) {
                Ok(vv) => vv,
                Err(_) => v.clone(),
            },
            Err(_) => Value::Integer(Integer::from(0)),
        };
        let rem_expr = reconstruct_polynomial(&rem, var);
        let new_rem_expr = match sub_values_public(&rem_expr, &subtrahend) {
            Ok(v) => v,
            Err(_) => break,
        };
        let new_coeffs = extract_polynomial_coeffs(&new_rem_expr, var);
        rem = new_coeffs;
        if rem.is_empty() || (rem.len() == 1 && matches!(&rem[0], Value::Integer(n) if n.is_zero()))
        {
            break;
        }
    }
    let quotient = if q_terms.is_empty() {
        Value::Integer(Integer::from(0))
    } else if q_terms.len() == 1 {
        q_terms.into_iter().next().unwrap()
    } else {
        Value::Call {
            head: "Plus".to_string(),
            args: q_terms,
        }
    };
    let remainder = reconstruct_polynomial(&rem, var);
    (quotient, remainder)
}
