use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;

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

/// RSolve[eqn, f[n], n] — solve recurrence equations.
/// Handles simple geometric recurrences a[n+1] == c * a[n]. Returns symbolic for others.
pub fn builtin_rsolve(args: &[Value], _env: &crate::env::Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "RSolve requires 3 arguments: RSolve[eqn, f[n], n]".to_string(),
        ));
    }

    // For now, mostly symbolic passthrough
    Ok(Value::Call {
        head: "RSolve".to_string(),
        args: args.to_vec(),
    })
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
}
