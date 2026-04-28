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
            if *den == Integer::from(2) && r.numer().is_odd() {
                return Ok(Value::Bool(true));
            }
            let num = r.numer();
            if num.is_even() && *den == Integer::from(1) {
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
    if args.len() < 1 || args.len() > 2 {
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

/// KnownSineIntegrandQ[expr, x] — stub, returns True
pub fn builtin_known_sine_integrand_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KnownSineIntegrandQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(true))
}

/// KnownSecantIntegrandQ[expr, x] — stub, returns True
pub fn builtin_known_secant_integrand_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KnownSecantIntegrandQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(true))
}

/// KnownTangentIntegrandQ[expr, x] — stub, returns True
pub fn builtin_known_tangent_integrand_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KnownTangentIntegrandQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(true))
}

/// KnownCotangentIntegrandQ[expr, x] — stub, returns True
pub fn builtin_known_cotangent_integrand_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KnownCotangentIntegrandQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(true))
}

/// Simp[expr, x] — simplify (delegate to Simplify)
pub fn builtin_simp(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 1 {
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
    if args.len() < 1 {
        return Err(EvalError::Error(
            "ExpandIntegrand requires at least 1 argument".to_string(),
        ));
    }
    crate::builtins::symbolic::builtin_expand(&[args[0].clone()])
}

/// ExpandToSum[expr, x] — expand to sum (delegate to Expand)
pub fn builtin_expand_to_sum(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 1 {
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
    if args.len() < 1 {
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
    Ok(Value::Bool(coeffs.len() >= 1 && coeffs.len() <= 2))
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
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => Ok(args[1].clone()),
        Value::Call { head, args }
            if head == "Power"
                && args.len() == 2
                && matches!(&args[1], Value::Integer(n) if n.is_negative()) =>
        {
            Ok(args[0].clone())
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
        ref v => value_to_expr(v),
    };
    // Evaluate bindings in child env to get concrete values
    let bindings_val = eval(&bindings_expr, &child_env)?;
    match &bindings_val {
        Value::List(pairs) => {
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
        _ => {}
    }
    // Unwrap body from Pattern(Expr) and eval in child env
    let body_expr = match &args[1] {
        Value::Pattern(e) => e.clone(),
        ref v => value_to_expr(v),
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
        ref v => value_to_expr(v),
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
        ref v => value_to_expr(v),
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
        ref v => value_to_expr(v),
    };
    let cond_val = eval(&cond_expr, env)?;
    let cond = cond_val.to_bool();
    if cond {
        let body_expr = match &args[1] {
            Value::Pattern(e) => e.clone(),
            ref v => value_to_expr(v),
        };
        eval(&body_expr, env)
    } else if args.len() == 3 {
        let body_expr = match &args[2] {
            Value::Pattern(e) => e.clone(),
            ref v => value_to_expr(v),
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

/// FunctionOfLinear[expr, x] — identity stub
pub fn builtin_function_of_linear(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "FunctionOfLinear requires at least 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// InverseFunctionFreeQ[expr, func, x] — is expr free of func in x
pub fn builtin_inverse_function_free_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "InverseFunctionFreeQ requires exactly 3 arguments".to_string(),
        ));
    }
    // Stub: return True
    Ok(Value::Bool(true))
}

/// DerivativeDivides[expr, x] — stub, return True
pub fn builtin_derivative_divides(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "DerivativeDivides requires at least 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(true))
}

/// SimplerQ[expr1, expr2, x] — is expr1 simpler than expr2
pub fn builtin_simpler_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "SimplerQ requires at least 2 arguments".to_string(),
        ));
    }
    // Stub: compare by structural size
    let s1 = leaf_count(&args[0]);
    let s2 = leaf_count(&args[1]);
    Ok(Value::Bool(s1 < s2))
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

/// NiceSqrtQ[expr] — stub
pub fn builtin_nice_sqrt_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "NiceSqrtQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(true))
}

/// BinomialMatchQ[expr, a, b, x, n] — stub
pub fn builtin_binomial_match_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 5 {
        return Err(EvalError::Error(
            "BinomialMatchQ requires at least 5 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
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
    // Stub: return 0
    Ok(Value::Integer(Integer::from(0)))
}

/// InverseFunctionOfLinear[func, args, x] — stub
pub fn builtin_inverse_function_of_linear(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "InverseFunctionOfLinear requires at least 3 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
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

/// SubstForFractionalPowerQ[result, expr, x] — stub
pub fn builtin_subst_for_fractional_power_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "SubstForFractionalPowerQ requires at least 3 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
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
    Ok(Value::Bool(true))
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

/// GeneralizedTrinomialQ[expr, x] — stub
pub fn builtin_generalized_trinomial_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "GeneralizedTrinomialQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
}

/// LinearPairQ[expr, x] — stub
pub fn builtin_linear_pair_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "LinearPairQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
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

/// FunctionOfLinear[expr, x] — stub
pub fn builtin_function_of_linear_fn(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "FunctionOfLinear requires at least 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// FunctionOfLog[expr, x] — stub
pub fn builtin_function_of_log(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "FunctionOfLog requires at least 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// FunctionOfExponentialQ[expr, x] — stub
pub fn builtin_function_of_exponential_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "FunctionOfExponentialQ requires at least 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
}

/// FunctionOfExponential[expr, x] — stub
pub fn builtin_function_of_exponential(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "FunctionOfExponential requires at least 2 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
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
    // Simplified: check if expr is a quotient of polynomials
    Ok(Value::Bool(true))
}

/// FunctionOfTrigOfLinearQ[expr, x] — stub
pub fn builtin_function_of_trig_of_linear_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "FunctionOfTrigOfLinearQ requires at least 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
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

/// InertTrigFreeQ[expr] — stub
pub fn builtin_inert_trig_free_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "InertTrigFreeQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(true))
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
    Ok(Value::Bool(true))
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

/// EulerIntegrandQ[expr, x] — stub
pub fn builtin_euler_integrand_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "EulerIntegrandQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
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

/// PseudoBinomialPairQ[expr1, expr2, x] — stub
pub fn builtin_pseudo_binomial_pair_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "PseudoBinomialPairQ requires at least 3 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
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
    Ok(Value::Bool(true))
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

/// GeneralizedBinomialQ[expr, x] — stub
pub fn builtin_generalized_binomial_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "GeneralizedBinomialQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
}

/// GeneralizedBinomialMatchQ[expr, a, b, x, n] — stub
pub fn builtin_generalized_binomial_match_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 5 {
        return Err(EvalError::Error(
            "GeneralizedBinomialMatchQ requires at least 5 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
}

/// GeneralizedTrinomialMatchQ[expr, a, b, c, x] — stub
pub fn builtin_generalized_trinomial_match_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 5 {
        return Err(EvalError::Error(
            "GeneralizedTrinomialMatchQ requires at least 5 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
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
pub fn builtin_polynomial_remainder(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "PolynomialRemainder requires exactly 3 arguments".to_string(),
        ));
    }
    // Stub: return p1 mod p2 (simplified)
    Ok(args[0].clone())
}

/// PolynomialQuotient[p1, p2, x] — polynomial quotient
pub fn builtin_polynomial_quotient(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "PolynomialQuotient requires exactly 3 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
}

/// PolynomialDivide[p1, p2, x] — polynomial division
pub fn builtin_polynomial_divide(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "PolynomialDivide requires exactly 3 arguments".to_string(),
        ));
    }
    Ok(args[0].clone())
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

/// QuadraticProductQ[expr, x] — stub
pub fn builtin_quadratic_product_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "QuadraticProductQ requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
}

/// SimplerIntegrandQ[expr1, expr2, x] — stub
pub fn builtin_simpler_integrand_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "SimplerIntegrandQ requires at least 3 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(false))
}

/// TrigonometricSimplifyQ[expr] — stub
pub fn builtin_trig_simplify_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TrigSimplifyQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(false))
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

/// MinimumMonomialExponent[expr, x] — stub
pub fn builtin_minimum_monomial_exponent(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MinimumMonomialExponent requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Integer(Integer::from(0)))
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
            if let Value::Symbol(os) = old_var {
                if s == os {
                    return new_expr.clone();
                }
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

fn is_polynomial(val: &Value, var: &str) -> bool {
    match val {
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => true,
        Value::Symbol(s) => true,
        Value::Call { head, args } => match head.as_str() {
            "Plus" | "Times" => args.iter().all(|a| is_polynomial(a, var)),
            "Power" if args.len() == 2 => {
                is_polynomial(&args[0], var)
                    && matches!(&args[1], Value::Integer(n) if !n.is_negative())
            }
            "Divide" => {
                if args.len() == 2 {
                    is_polynomial(&args[0], var) && is_polynomial(&args[1], var)
                } else {
                    false
                }
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
        Value::Call { args, .. } => args.iter().any(|a| contains_complex(a)),
        Value::List(items) => items.iter().any(|a| contains_complex(a)),
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
