use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;

/// Numeric equality that promotes across the numeric types.
///
/// Returns `None` when either operand is not a number, so callers can fall
/// back to structural comparison.
fn numeric_eq(a: &Value, b: &Value) -> Option<bool> {
    use Value::*;
    let rat_to_float = |r: &rug::Rational| -> Float {
        Float::with_val(DEFAULT_PRECISION, r.numer())
            / Float::with_val(DEFAULT_PRECISION, r.denom())
    };
    match (a, b) {
        (Integer(x), Integer(y)) => Some(x == y),
        (Rational(x), Rational(y)) => Some(x == y),
        (Real(x), Real(y)) => Some(x == y),
        (Integer(x), Rational(y)) | (Rational(y), Integer(x)) => {
            Some(y.numer() == &rug::Integer::from(x * y.denom()))
        }
        (Integer(x), Real(y)) | (Real(y), Integer(x)) => {
            Some(Float::with_val(DEFAULT_PRECISION, x) == *y)
        }
        (Rational(x), Real(y)) | (Real(y), Rational(x)) => Some(rat_to_float(x) == *y),
        (Complex { re: r1, im: i1 }, Complex { re: r2, im: i2 }) => Some(r1 == r2 && i1 == i2),
        (Complex { re, im }, other) | (other, Complex { re, im }) if *im == 0.0 => {
            other.to_real().map(|o| *re == o)
        }
        _ => None,
    }
}

/// Equality used by `Equal`/`Unequal`: numbers compare by value (so `1 == 1.0`)
/// while everything else compares structurally.
fn values_equal(a: &Value, b: &Value) -> bool {
    // `Formatted` is only a display hint; compare the underlying values.
    if let Value::Formatted { value, .. } = a {
        return values_equal(value, b);
    }
    if let Value::Formatted { value, .. } = b {
        return values_equal(a, value);
    }
    if let Some(eq) = numeric_eq(a, b) {
        return eq;
    }
    match (a, b) {
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| values_equal(p, q))
        }
        (Value::Call { head: h1, args: a1 }, Value::Call { head: h2, args: a2 }) => {
            h1 == h2
                && a1.len() == a2.len()
                && a1.iter().zip(a2.iter()).all(|(p, q)| values_equal(p, q))
        }
        _ => a.struct_eq(b),
    }
}

/// Helper: compare two values numerically, return Some(bool) or None for incomparable
fn compare_two<F: Fn(&Float, &Float) -> bool>(a: &Value, b: &Value, op: F) -> Option<bool> {
    match (a, b) {
        (Value::Integer(a), Value::Integer(b)) => Some(op(
            &Float::with_val(DEFAULT_PRECISION, a),
            &Float::with_val(DEFAULT_PRECISION, b),
        )),
        (Value::Real(a), Value::Real(b)) => Some(op(a, b)),
        (Value::Integer(a), Value::Real(b)) => Some(op(&Float::with_val(DEFAULT_PRECISION, a), b)),
        (Value::Real(a), Value::Integer(b)) => Some(op(a, &Float::with_val(DEFAULT_PRECISION, b))),
        (Value::Rational(a), Value::Rational(b)) => Some(op(
            &(Float::with_val(DEFAULT_PRECISION, a.numer())
                / Float::with_val(DEFAULT_PRECISION, a.denom())),
            &(Float::with_val(DEFAULT_PRECISION, b.numer())
                / Float::with_val(DEFAULT_PRECISION, b.denom())),
        )),
        (Value::Rational(a), Value::Integer(b)) => {
            let a_f = Float::with_val(DEFAULT_PRECISION, a.numer())
                / Float::with_val(DEFAULT_PRECISION, a.denom());
            Some(op(&a_f, &Float::with_val(DEFAULT_PRECISION, b)))
        }
        (Value::Integer(a), Value::Rational(b)) => {
            let b_f = Float::with_val(DEFAULT_PRECISION, b.numer())
                / Float::with_val(DEFAULT_PRECISION, b.denom());
            Some(op(&Float::with_val(DEFAULT_PRECISION, a), &b_f))
        }
        (Value::Rational(a), Value::Real(b)) => {
            let a_f = Float::with_val(DEFAULT_PRECISION, a.numer())
                / Float::with_val(DEFAULT_PRECISION, a.denom());
            Some(op(&a_f, b))
        }
        (Value::Real(a), Value::Rational(b)) => {
            let b_f = Float::with_val(DEFAULT_PRECISION, b.numer())
                / Float::with_val(DEFAULT_PRECISION, b.denom());
            Some(op(a, &b_f))
        }
        _ => None,
    }
}

/// Equal[a, b, c, ...] — chain comparison: all adjacent pairs equal
pub fn builtin_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Equal requires at least 2 arguments".to_string(),
        ));
    }
    for pair in args.windows(2) {
        if !values_equal(&pair[0], &pair[1]) {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

/// Unequal[a, b, c, ...] — pairs are not all equal
pub fn builtin_unequal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Unequal requires at least 2 arguments".to_string(),
        ));
    }
    // Unequal[a, b, c] is True if all pairs are distinct
    for i in 0..args.len() {
        for j in (i + 1)..args.len() {
            if values_equal(&args[i], &args[j]) {
                return Ok(Value::Bool(false));
            }
        }
    }
    Ok(Value::Bool(true))
}

/// SameQ[a, b, c, ...] — structural equality (===), all identical
pub fn builtin_same_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "SameQ requires at least 2 arguments".to_string(),
        ));
    }
    for pair in args.windows(2) {
        if !pair[0].struct_eq(&pair[1]) {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

/// Order[a, b] — canonical ordering: -1, 0, or 1
pub fn builtin_order(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Order requires exactly 2 arguments".to_string(),
        ));
    }
    // Use numeric comparison first
    if let Some(true) = compare_two(&args[0], &args[1], |x, y| x < y) {
        return Ok(Value::Integer(rug::Integer::from(-1)));
    }
    if let Some(true) = compare_two(&args[0], &args[1], |x, y| x > y) {
        return Ok(Value::Integer(rug::Integer::from(1)));
    }
    if let Some(true) = compare_two(&args[0], &args[1], |x, y| x == y) {
        return Ok(Value::Integer(rug::Integer::from(0)));
    }
    // String comparison
    if let (Value::Str(a), Value::Str(b)) = (&args[0], &args[1]) {
        if a < b {
            return Ok(Value::Integer(rug::Integer::from(-1)));
        }
        if a > b {
            return Ok(Value::Integer(rug::Integer::from(1)));
        }
        return Ok(Value::Integer(rug::Integer::from(0)));
    }
    // Structural comparison as final fallback
    if args[0].struct_eq(&args[1]) {
        Ok(Value::Integer(rug::Integer::from(0)))
    } else {
        Ok(Value::Integer(rug::Integer::from(-1)))
    }
}

/// Less[a, b, c, ...] — strict increasing chain: a < b < c
pub fn builtin_less(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Less requires at least 2 arguments".to_string(),
        ));
    }
    for pair in args.windows(2) {
        match compare_two(&pair[0], &pair[1], |x, y| x < y) {
            Some(true) => continue,
            Some(false) => return Ok(Value::Bool(false)),
            None => match (&pair[0], &pair[1]) {
                (Value::Str(a), Value::Str(b)) => {
                    if a >= b {
                        return Ok(Value::Bool(false));
                    }
                }
                _ => {
                    return Ok(Value::Call {
                        head: "Less".to_string(),
                        args: args.to_vec(),
                    });
                }
            },
        }
    }
    Ok(Value::Bool(true))
}

/// Greater[a, b, c, ...] — strict decreasing chain: a > b > c
pub fn builtin_greater(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Greater requires at least 2 arguments".to_string(),
        ));
    }
    for pair in args.windows(2) {
        match compare_two(&pair[0], &pair[1], |x, y| x > y) {
            Some(true) => continue,
            Some(false) => return Ok(Value::Bool(false)),
            None => match (&pair[0], &pair[1]) {
                (Value::Str(a), Value::Str(b)) => {
                    if a <= b {
                        return Ok(Value::Bool(false));
                    }
                }
                _ => {
                    return Ok(Value::Call {
                        head: "Greater".to_string(),
                        args: args.to_vec(),
                    });
                }
            },
        }
    }
    Ok(Value::Bool(true))
}

/// LessEqual[a, b, c, ...] — non-decreasing chain: a <= b <= c
pub fn builtin_less_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "LessEqual requires at least 2 arguments".to_string(),
        ));
    }
    for pair in args.windows(2) {
        match compare_two(&pair[0], &pair[1], |x, y| x <= y) {
            Some(true) => continue,
            Some(false) => return Ok(Value::Bool(false)),
            None => match (&pair[0], &pair[1]) {
                (Value::Str(a), Value::Str(b)) => {
                    if a > b {
                        return Ok(Value::Bool(false));
                    }
                }
                _ => {
                    return Ok(Value::Call {
                        head: "LessEqual".to_string(),
                        args: args.to_vec(),
                    });
                }
            },
        }
    }
    Ok(Value::Bool(true))
}

/// GreaterEqual[a, b, c, ...] — non-increasing chain: a >= b >= c
pub fn builtin_greater_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "GreaterEqual requires at least 2 arguments".to_string(),
        ));
    }
    for pair in args.windows(2) {
        match compare_two(&pair[0], &pair[1], |x, y| x >= y) {
            Some(true) => continue,
            Some(false) => return Ok(Value::Bool(false)),
            None => match (&pair[0], &pair[1]) {
                (Value::Str(a), Value::Str(b)) => {
                    if a < b {
                        return Ok(Value::Bool(false));
                    }
                }
                _ => {
                    return Ok(Value::Call {
                        head: "GreaterEqual".to_string(),
                        args: args.to_vec(),
                    });
                }
            },
        }
    }
    Ok(Value::Bool(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn real(r: f64) -> Value {
        Value::Real(rug::Float::with_val(DEFAULT_PRECISION, r))
    }
    fn rational(n: i64, d: i64) -> Value {
        Value::Rational(Box::new(rug::Rational::from((n, d))))
    }

    // ── Equal tests ──
    #[test]
    fn test_equal_integers() {
        assert_eq!(builtin_equal(&[int(5), int(5)]).unwrap(), Value::Bool(true));
        assert_eq!(
            builtin_equal(&[int(5), int(3)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_equal_mixed_numeric() {
        assert_eq!(
            builtin_equal(&[int(1), real(1.0)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[int(1), real(1.5)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_equal_strings() {
        assert_eq!(
            builtin_equal(&[Value::Str("hi".into()), Value::Str("hi".into())]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[Value::Str("hi".into()), Value::Str("bye".into())]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_equal_numeric_promotion() {
        assert_eq!(
            builtin_equal(&[int(1), real(1.0)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[real(1.0), int(1)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[rational(1, 2), real(0.5)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[int(2), rational(4, 2)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[int(1), real(1.5)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_equal_nested_numeric_promotion() {
        let a = Value::List(vec![int(1), real(2.5)]);
        let b = Value::List(vec![real(1.0), rational(5, 2)]);
        assert_eq!(builtin_equal(&[a, b]).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_equal_symbolic_stays_structural() {
        let x = Value::Symbol("x".into());
        let y = Value::Symbol("y".into());
        assert_eq!(builtin_equal(&[x.clone(), x]).unwrap(), Value::Bool(true));
        assert_eq!(
            builtin_equal(&[Value::Symbol("x".into()), y]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_unequal_numeric_promotion() {
        assert_eq!(
            builtin_unequal(&[int(1), real(1.0)]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_unequal(&[int(1), real(1.5)]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_equal_complex_zero_imag() {
        let c = Value::Complex { re: 2.0, im: 0.0 };
        assert_eq!(builtin_equal(&[c, real(2.0)]).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_equal_chain() {
        assert_eq!(
            builtin_equal(&[int(1), int(1), int(1)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[int(1), int(1), int(2)]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_equal(&[int(1), int(2), int(1)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── Unequal tests ──
    #[test]
    fn test_unequal_integers() {
        assert_eq!(
            builtin_unequal(&[int(5), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_unequal(&[int(5), int(5)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_unequal_chain() {
        assert_eq!(
            builtin_unequal(&[int(1), int(2), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_unequal(&[int(1), int(1), int(2)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── SameQ tests ──
    #[test]
    fn test_same_q() {
        assert_eq!(
            builtin_same_q(&[int(5), int(5)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_same_q(&[int(5), int(3)]).unwrap(),
            Value::Bool(false)
        );
        // SameQ is structural: 1 !== 1.0
        assert_eq!(
            builtin_same_q(&[int(1), real(1.0)]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_same_q(&[int(1), int(1), int(1)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_same_q(&[int(1), int(1), int(2)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── Order tests ──
    #[test]
    fn test_order() {
        assert_eq!(builtin_order(&[int(3), int(5)]).unwrap(), int(-1));
        assert_eq!(builtin_order(&[int(5), int(3)]).unwrap(), int(1));
        assert_eq!(builtin_order(&[int(5), int(5)]).unwrap(), int(0));
    }

    // ── Less tests ──
    #[test]
    fn test_less_integers() {
        assert_eq!(builtin_less(&[int(3), int(5)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_less(&[int(5), int(3)]).unwrap(), Value::Bool(false));
        assert_eq!(builtin_less(&[int(5), int(5)]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_less_mixed_types() {
        assert_eq!(
            builtin_less(&[int(3), real(5.0)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less(&[real(5.0), int(3)]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_less(&[int(1), rational(3, 2)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less(&[real(1.5), rational(1, 2)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_less_strings() {
        assert_eq!(
            builtin_less(&[Value::Str("a".into()), Value::Str("b".into())]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_less_chain() {
        assert_eq!(
            builtin_less(&[int(1), int(2), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less(&[int(1), int(3), int(2)]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_less(&[int(1), int(2), int(2)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── Greater tests ──
    #[test]
    fn test_greater_integers() {
        assert_eq!(
            builtin_greater(&[int(5), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater(&[int(3), int(5)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_greater_mixed() {
        assert_eq!(
            builtin_greater(&[int(5), real(3.0)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater(&[rational(5, 2), int(2)]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_greater_chain() {
        assert_eq!(
            builtin_greater(&[int(3), int(2), int(1)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater(&[int(3), int(1), int(2)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── LessEqual tests ──
    #[test]
    fn test_less_equal() {
        assert_eq!(
            builtin_less_equal(&[int(3), int(5)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less_equal(&[int(5), int(5)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less_equal(&[int(5), int(3)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_less_equal_chain() {
        assert_eq!(
            builtin_less_equal(&[int(1), int(2), int(2)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less_equal(&[int(1), int(2), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less_equal(&[int(2), int(1), int(3)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── GreaterEqual tests ──
    #[test]
    fn test_greater_equal() {
        assert_eq!(
            builtin_greater_equal(&[int(5), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater_equal(&[int(5), int(5)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater_equal(&[int(3), int(5)]).unwrap(),
            Value::Bool(false)
        );
    }
}
