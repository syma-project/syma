use crate::value::{EvalError, Value};
use rug::Complete;
use rug::Float;
use std::cmp::Ordering;

/// Exact ordering of two real-valued numbers.
///
/// Integers and rationals are compared exactly (cross-multiplied), and an
/// inexact `Real` is compared against an exact number through its exact
/// dyadic rational value, so values with more bits than the working float
/// precision (e.g. `2^200` vs `2^200 + 1`) compare correctly.
///
/// Returns `None` when either operand is not a comparable real number, or when
/// a `Real` is `NaN`.
fn numeric_cmp(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Some(x.cmp(y)),
        (Value::Rational(x), Value::Rational(y)) => Some(x.cmp(y)),
        (Value::Integer(x), Value::Rational(y)) => Some((x * y.denom()).complete().cmp(y.numer())),
        (Value::Rational(x), Value::Integer(y)) => Some(x.numer().cmp(&(y * x.denom()).complete())),
        (Value::Real(x), Value::Real(y)) => x.partial_cmp(y),
        (Value::Real(x), other) => cmp_real_exact(x, other),
        (other, Value::Real(y)) => cmp_real_exact(y, other).map(Ordering::reverse),
        _ => None,
    }
}

/// Compare a `Real` against an exact integer/rational without loss of precision.
fn cmp_real_exact(f: &Float, other: &Value) -> Option<Ordering> {
    let other_rat = match other {
        Value::Integer(n) => rug::Rational::from(n.clone()),
        Value::Rational(r) => (**r).clone(),
        _ => return None,
    };
    if f.is_nan() {
        return None;
    }
    if f.is_infinite() {
        return Some(if f.is_sign_positive() {
            Ordering::Greater
        } else {
            Ordering::Less
        });
    }
    Some(f.to_rational()?.cmp(&other_rat))
}

/// Numeric equality that promotes across the numeric types.
///
/// Returns `None` when either operand is not a number, so callers can fall
/// back to structural comparison.
fn numeric_eq(a: &Value, b: &Value) -> Option<bool> {
    if let Some(ord) = numeric_cmp(a, b) {
        return Some(ord == Ordering::Equal);
    }
    // Complex numbers are outside the real ordering.
    match (a, b) {
        (Value::Complex { re: re1, im: im1 }, Value::Complex { re: re2, im: im2 }) => {
            Some(re1 == re2 && im1 == im2)
        }
        (Value::Complex { re, im }, other) | (other, Value::Complex { re, im }) if *im == 0.0 => {
            other.to_real().map(|o| *re == o)
        }
        _ => None,
    }
}

/// Equality used by `Equal`/`Unequal`: numbers compare by value (so `1 == 1.0`)
/// while everything else compares structurally, descending into lists and
/// calls so that numeric promotion applies element-wise.
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
        _ => a == b,
    }
}

/// Structural equality used by `SameQ`/`UnsameQ`: identical type *and*
/// contents, so `1 =!= 1.0`. Falls back to `Value`'s structural `PartialEq`
/// for variants (`Association`, `Rule`, objects, …) that `struct_eq` does not
/// cover.
fn structural_eq(a: &Value, b: &Value) -> bool {
    a.struct_eq(b) || a == b
}

/// Compare a chain `a op b op c ...`: every adjacent pair must satisfy `op`.
///
/// Falls back to an unevaluated call when a pair is not comparable (for
/// example two distinct symbols), matching Wolfram Language behaviour.
fn compare_chain(
    name: &str,
    args: &[Value],
    satisfied: impl Fn(Ordering) -> bool,
) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(Value::Bool(true));
    }
    for pair in args.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let ord = match (a, b) {
            (Value::Str(x), Value::Str(y)) => Some(x.cmp(y)),
            _ => numeric_cmp(a, b),
        };
        match ord {
            Some(o) => {
                if !satisfied(o) {
                    return Ok(Value::Bool(false));
                }
            }
            None => {
                return Ok(Value::Call {
                    head: name.to_string(),
                    args: args.to_vec(),
                });
            }
        }
    }
    Ok(Value::Bool(true))
}

/// Equal[a, b, c, ...] — chain comparison: all adjacent pairs equal.
pub fn builtin_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(Value::Bool(true));
    }
    for pair in args.windows(2) {
        if !values_equal(&pair[0], &pair[1]) {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

/// Unequal[a, b, c, ...] — True when all arguments are pairwise distinct.
pub fn builtin_unequal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(Value::Bool(true));
    }
    for i in 0..args.len() {
        for j in (i + 1)..args.len() {
            if values_equal(&args[i], &args[j]) {
                return Ok(Value::Bool(false));
            }
        }
    }
    Ok(Value::Bool(true))
}

/// SameQ[a, b, c, ...] — structural equality (===), all identical.
pub fn builtin_same_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(Value::Bool(true));
    }
    for pair in args.windows(2) {
        if !structural_eq(&pair[0], &pair[1]) {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

/// UnsameQ[a, b, c, ...] — structural inequality (=!=), all pairwise distinct.
pub fn builtin_unsame_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(Value::Bool(true));
    }
    for i in 0..args.len() {
        for j in (i + 1)..args.len() {
            if structural_eq(&args[i], &args[j]) {
                return Ok(Value::Bool(false));
            }
        }
    }
    Ok(Value::Bool(true))
}

/// Rank of a numeric representation, used to break ties in canonical order
/// when two numbers are numerically equal but structurally distinct
/// (e.g. `Order[1/2, .5]` is `-1`, since `0.5` precedes `1/2`).
fn num_repr_rank(v: &Value) -> u8 {
    match v {
        Value::Real(_) => 0,
        Value::Rational(_) => 1,
        Value::Integer(_) => 2,
        Value::Complex { .. } => 3,
        _ => 4,
    }
}

/// Coarse type rank so that different kinds of expressions have a stable,
/// antisymmetric ordering.
fn kind_rank(v: &Value) -> u8 {
    match v {
        Value::Integer(_)
        | Value::Real(_)
        | Value::Rational(_)
        | Value::Complex { .. }
        | Value::Root { .. } => 0,
        Value::Symbol(_) => 1,
        Value::Str(_) => 2,
        Value::Bool(_) => 3,
        Value::Null => 4,
        Value::List(_) => 5,
        Value::Call { .. } => 6,
        _ => 7,
    }
}

/// Compare two sequences length-first (shorter before longer), then
/// element-wise, as canonical order does.
fn cmp_seq(x: &[Value], y: &[Value]) -> Ordering {
    x.len().cmp(&y.len()).then_with(|| {
        for (p, q) in x.iter().zip(y.iter()) {
            let o = canonical_cmp(p, q);
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    })
}

/// Canonical order used by `Order` (structure-first, numbers by value).
///
/// Returns `Ordering::Less` when `a` comes before `b`. This is antisymmetric:
/// `canonical_cmp(a, b) == canonical_cmp(b, a).reverse()`.
pub(crate) fn canonical_cmp(a: &Value, b: &Value) -> Ordering {
    if let Value::Formatted { value, .. } = a {
        return canonical_cmp(value, b);
    }
    if let Value::Formatted { value, .. } = b {
        return canonical_cmp(a, value);
    }
    if let Some(ord) = numeric_cmp(a, b) {
        if ord != Ordering::Equal {
            return ord;
        }
        if a.struct_eq(b) {
            return Ordering::Equal;
        }
        // Numerically equal but distinct representation: order by type.
        return num_repr_rank(a).cmp(&num_repr_rank(b));
    }
    match (a, b) {
        (Value::Complex { re: r1, im: i1 }, Value::Complex { re: r2, im: i2 }) => r1
            .partial_cmp(r2)
            .unwrap_or(Ordering::Equal)
            .then_with(|| i1.abs().partial_cmp(&i2.abs()).unwrap_or(Ordering::Equal)),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Symbol(x), Value::Symbol(y)) => x.cmp(y),
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::List(x), Value::List(y)) => cmp_seq(x, y),
        (Value::Call { head: h1, args: a1 }, Value::Call { head: h2, args: a2 }) => {
            h1.cmp(h2).then_with(|| cmp_seq(a1, a2))
        }
        _ => kind_rank(a)
            .cmp(&kind_rank(b))
            .then_with(|| a.type_name().cmp(b.type_name()))
            .then_with(|| a.to_string().cmp(&b.to_string())),
    }
}

/// Order[a, b] — canonical ordering: 1 if `a` precedes `b`, -1 if it follows,
/// 0 if the two are identical.
pub fn builtin_order(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Order requires exactly 2 arguments".to_string(),
        ));
    }
    let result = match canonical_cmp(&args[0], &args[1]) {
        Ordering::Less => 1,
        Ordering::Equal => 0,
        Ordering::Greater => -1,
    };
    Ok(Value::Integer(rug::Integer::from(result)))
}

/// Less[a, b, c, ...] — strict increasing chain: a < b < c.
pub fn builtin_less(args: &[Value]) -> Result<Value, EvalError> {
    compare_chain("Less", args, |o| o == Ordering::Less)
}

/// Greater[a, b, c, ...] — strict decreasing chain: a > b > c.
pub fn builtin_greater(args: &[Value]) -> Result<Value, EvalError> {
    compare_chain("Greater", args, |o| o == Ordering::Greater)
}

/// LessEqual[a, b, c, ...] — non-decreasing chain: a <= b <= c.
pub fn builtin_less_equal(args: &[Value]) -> Result<Value, EvalError> {
    compare_chain("LessEqual", args, |o| o != Ordering::Greater)
}

/// GreaterEqual[a, b, c, ...] — non-increasing chain: a >= b >= c.
pub fn builtin_greater_equal(args: &[Value]) -> Result<Value, EvalError> {
    compare_chain("GreaterEqual", args, |o| o != Ordering::Less)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::DEFAULT_PRECISION;
    use rug::Integer;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn big(s: &str) -> Value {
        Value::Integer(s.parse::<Integer>().unwrap())
    }
    fn real(r: f64) -> Value {
        Value::Real(Float::with_val(DEFAULT_PRECISION, r))
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
    fn test_equal_exact_big_integers() {
        // Beyond the working float precision, so an f64/Float promotion would
        // collapse them together.
        let a = big("1606938044258990275541962092341162602522202993782792835301376"); // 2^200
        let b = big("1606938044258990275541962092341162602522202993782792835301377"); // 2^200 + 1
        assert_eq!(
            builtin_equal(&[a.clone(), a.clone()]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[a.clone(), b.clone()]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_less(&[a.clone(), b.clone()]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(builtin_greater(&[b, a]).unwrap(), Value::Bool(true));
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
    fn test_equal_reflexive_compound() {
        let l = Value::List(vec![int(1), Value::Symbol("x".into())]);
        assert_eq!(builtin_equal(&[l.clone(), l]).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_equal_unary_and_empty() {
        assert_eq!(builtin_equal(&[]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_equal(&[int(7)]).unwrap(), Value::Bool(true));
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
    fn test_equal_complex_nonzero_imag() {
        let c = Value::Complex { re: 2.0, im: 1.0 };
        assert_eq!(
            builtin_equal(&[c.clone(), real(2.0)]).unwrap(),
            Value::Bool(false)
        );
        let c2 = Value::Complex { re: 2.0, im: 1.0 };
        assert_eq!(builtin_equal(&[c, c2]).unwrap(), Value::Bool(true));
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
        // Pairwise, not just adjacent: 1 and 3 must also differ.
        assert_eq!(
            builtin_unequal(&[int(1), int(2), int(1)]).unwrap(),
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

    #[test]
    fn test_same_q_lists() {
        let a = Value::List(vec![int(1), real(1.0)]);
        let b = Value::List(vec![int(1), real(1.0)]);
        let c = Value::List(vec![int(1), int(1)]);
        assert_eq!(builtin_same_q(&[a.clone(), b]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_same_q(&[a, c]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_same_q_association() {
        use std::collections::HashMap;
        let mut m1 = HashMap::new();
        m1.insert("a".to_string(), int(1));
        let mut m2 = HashMap::new();
        m2.insert("a".to_string(), int(1));
        let mut m3 = HashMap::new();
        m3.insert("a".to_string(), int(2));
        assert_eq!(
            builtin_same_q(&[Value::Assoc(m1.clone()), Value::Assoc(m2)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_same_q(&[Value::Assoc(m1), Value::Assoc(m3)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── UnsameQ tests ──
    #[test]
    fn test_unsame_q() {
        assert_eq!(
            builtin_unsame_q(&[int(5), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_unsame_q(&[int(5), int(5)]).unwrap(),
            Value::Bool(false)
        );
        // Structural: 1 =!= 1.0
        assert_eq!(
            builtin_unsame_q(&[int(1), real(1.0)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(builtin_unsame_q(&[int(1)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_unsame_q(&[]).unwrap(), Value::Bool(true));
        assert_eq!(
            builtin_unsame_q(&[int(1), int(2), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_unsame_q(&[int(1), int(2), int(1)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── Order tests ──
    #[test]
    fn test_order_numbers() {
        // Canonical order: 1 when the first argument precedes the second.
        assert_eq!(builtin_order(&[int(3), int(5)]).unwrap(), int(1));
        assert_eq!(builtin_order(&[int(5), int(3)]).unwrap(), int(-1));
        assert_eq!(builtin_order(&[int(5), int(5)]).unwrap(), int(0));
        // Numerically equal but structurally distinct.
        assert_eq!(
            builtin_order(&[rational(1, 2), real(0.5)]).unwrap(),
            int(-1)
        );
        assert_eq!(builtin_order(&[real(0.5), rational(1, 2)]).unwrap(), int(1));
    }

    #[test]
    fn test_order_symbols_and_expressions() {
        assert_eq!(
            builtin_order(&[Value::Symbol("a".into()), Value::Symbol("b".into())]).unwrap(),
            int(1)
        );
        assert_eq!(
            builtin_order(&[Value::Symbol("b".into()), Value::Symbol("a".into())]).unwrap(),
            int(-1)
        );
        // Structural ordering is antisymmetric.
        let x = Value::Symbol("x".into());
        let y = Value::Symbol("y".into());
        let xy = builtin_order(&[x.clone(), y.clone()]).unwrap();
        let yx = builtin_order(&[y, x]).unwrap();
        assert_eq!(xy, int(1));
        assert_eq!(yx, int(-1));
    }

    #[test]
    fn test_order_cross_type_antisymmetric() {
        let a = int(6);
        let b = Value::Call {
            head: "Sqrt".into(),
            args: vec![int(2)],
        };
        assert_eq!(builtin_order(&[a.clone(), b.clone()]).unwrap(), int(1));
        assert_eq!(builtin_order(&[b, a]).unwrap(), int(-1));
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
        // Rational / integer cross-multiplication.
        assert_eq!(
            builtin_less(&[rational(1, 3), int(1)]).unwrap(),
            Value::Bool(true)
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
    fn test_less_symbolic_stays_unevaluated() {
        let x = Value::Symbol("x".into());
        let y = Value::Symbol("y".into());
        assert_eq!(
            builtin_less(&[x.clone(), y.clone()]).unwrap(),
            Value::Call {
                head: "Less".into(),
                args: vec![x, y],
            }
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

    #[test]
    fn test_less_unary_and_empty() {
        assert_eq!(builtin_less(&[]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_less(&[int(7)]).unwrap(), Value::Bool(true));
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
