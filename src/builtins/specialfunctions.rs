use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;
use rug::ops::Pow;

// Helper: create an unevaluated Call node.
fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

// Helper: build an exact rational value n/d.
fn rational(n: i64, d: i64) -> Value {
    Value::Rational(Box::new(rug::Rational::from((n, d))))
}

// Helper: build the exact closed form Pi^deg / den.
fn pi_pow_over(deg: u32, den: i64) -> Value {
    let pi_pow = crate::builtins::symbolic::simplify_call(
        "Power",
        &[
            Value::Symbol("Pi".to_string()),
            Value::Integer(Integer::from(deg)),
        ],
    );
    crate::builtins::symbolic::simplify_call("Times", &[pi_pow, rational(1, den)])
}

// ── Erf / Erfc / Erfi / ErfInverse ──

/// Erf evaluated with MPFR's correctly-rounded kernel, so results are accurate
/// to the full working precision (the old rational approximation was only good
/// to ~1e-7).
pub fn builtin_erf(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Erf", args, 1)?;
    let prec = DEFAULT_PRECISION;
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Real(r) if r.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) => Ok(Value::Real(Float::with_val(prec, n).erf())),
        Value::Real(r) if r.is_infinite() => {
            Ok(Value::Integer(Integer::from(if r.is_sign_positive() {
                1
            } else {
                -1
            })))
        }
        Value::Real(r) => Ok(Value::Real(r.clone().erf())),
        _ => Ok(unevaluated("Erf", args)),
    }
}

pub fn builtin_erfc(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Erfc", args, 1)?;
    let prec = DEFAULT_PRECISION;
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(1))),
        Value::Real(r) if r.is_zero() => Ok(Value::Integer(Integer::from(1))),
        Value::Integer(n) => Ok(Value::Real(Float::with_val(prec, n).erfc())),
        Value::Real(r) if r.is_infinite() => {
            Ok(Value::Integer(Integer::from(if r.is_sign_positive() {
                0
            } else {
                2
            })))
        }
        Value::Real(r) => Ok(Value::Real(r.clone().erfc())),
        _ => Ok(unevaluated("Erfc", args)),
    }
}

/// `erfi(x) = -i Erf(i x)`. Uses the everywhere-convergent Taylor series
/// (all terms positive, so no cancellation) for moderate `|x|`, and the
/// large-argument asymptotic expansion `erfi(x) ~ e^{x^2}/(x Sqrt[Pi])
/// (1 + 1/(2x^2) + 3/(4x^4) + ...)` beyond, all in full precision.
pub(crate) fn erfi_float(x: &Float) -> Float {
    let prec = x.prec().max(DEFAULT_PRECISION);
    let x = Float::with_val(prec, x);
    if x.is_zero() {
        return Float::with_val(prec, 0u32);
    }
    let neg = x.is_sign_negative();
    let ax = Float::with_val(prec, &x).abs();
    let sqrt_pi = Float::with_val(prec, rug::float::Constant::Pi).sqrt();
    let two_over_sqrt_pi = Float::with_val(prec, 2u32) / &sqrt_pi;
    let mut result = if ax < Float::with_val(prec, 9u32) {
        let a2 = Float::with_val(prec, &ax * &ax);
        let mut term = Float::with_val(prec, &ax);
        let mut sum = Float::with_val(prec, &ax);
        let mut k: u32 = 0;
        loop {
            k += 1;
            term *= &a2;
            term *= Float::with_val(prec, 2 * k - 1);
            term /= Float::with_val(prec, k) * Float::with_val(prec, 2 * k + 1);
            sum += &term;
            if term < Float::with_val(prec, 1e-40) || k > 20_000 {
                break;
            }
        }
        sum * two_over_sqrt_pi
    } else {
        let x2 = Float::with_val(prec, &ax * &ax);
        let inv2x2 = Float::with_val(prec, 1u32) / (Float::with_val(prec, 2u32) * &x2);
        let mut term = Float::with_val(prec, 1u32);
        let mut sum = Float::with_val(prec, 1u32);
        for k in 1..400u32 {
            term *= Float::with_val(prec, 2 * k - 1) * &inv2x2;
            sum += &term;
            if term < Float::with_val(prec, 1e-42) {
                break;
            }
        }
        sum * x2.exp() / (&ax * sqrt_pi)
    };
    if neg {
        result = -result;
    }
    result
}

pub fn builtin_erfi(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Erfi", args, 1)?;
    let prec = DEFAULT_PRECISION;
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Real(r) if r.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Real(r) if r.is_infinite() => Ok(Value::Symbol(
            if r.is_sign_positive() {
                "Infinity"
            } else {
                "-Infinity"
            }
            .to_string(),
        )),
        Value::Integer(n) => {
            let v = erfi_float(&Float::with_val(prec, n));
            if v.is_finite() {
                Ok(Value::Real(v))
            } else {
                Ok(Value::Symbol(
                    if n.is_negative() {
                        "-Infinity"
                    } else {
                        "Infinity"
                    }
                    .to_string(),
                ))
            }
        }
        Value::Real(r) => {
            let v = erfi_float(r);
            if v.is_finite() {
                Ok(Value::Real(v))
            } else {
                Ok(Value::Symbol(
                    if r.is_sign_negative() {
                        "-Infinity"
                    } else {
                        "Infinity"
                    }
                    .to_string(),
                ))
            }
        }
        _ => Ok(unevaluated("Erfi", args)),
    }
}

/// f64 initial guess for the inverse error function (series near 0, Winitzki
/// elsewhere); Newton in full precision below sharpens it to ~1e-30.
fn inverse_erf_init(x: f64) -> f64 {
    if x == 0.0 {
        return 0.0;
    }
    if x.abs() < 0.7 {
        let c0 = 0.5 * std::f64::consts::PI.sqrt();
        let ax2 = x * x;
        return x * (c0 + ax2 * (c0 / 3.0 + ax2 * (3.0 * c0 / 40.0)));
    }
    let s = x.signum();
    let ax = x.abs();
    let a = 0.147_f64;
    let ln1mx2 = (1.0 - ax * ax).ln();
    let c = 2.0 / (std::f64::consts::PI * a) + ln1mx2 / 2.0;
    let inner = (c * c - ln1mx2 / a).max(0.0).sqrt() - c;
    let w = s * inner.max(0.0).sqrt();
    if w.is_finite() {
        w
    } else {
        s * (-2.0 * (1.0 - ax)).ln().max(0.0).sqrt()
    }
}

/// Inverse of erf on (-1, 1) via Newton iteration in full precision.
fn inverse_erf_float(x: &Float) -> Float {
    let prec = x.prec().max(DEFAULT_PRECISION);
    let xf = Float::with_val(prec, x);
    let mut w = Float::with_val(prec, inverse_erf_init(xf.to_f64()));
    let two_over_sqrt_pi =
        Float::with_val(prec, 2u32) / Float::with_val(prec, rug::float::Constant::Pi).sqrt();
    for _ in 0..50 {
        let ew = Float::with_val(prec, &w).erf();
        let diff = Float::with_val(prec, &ew - &xf);
        if diff.to_f64().abs() < 1e-30 {
            break;
        }
        let w2 = Float::with_val(prec, &w * &w);
        let deriv = Float::with_val(prec, &two_over_sqrt_pi) * (-w2).exp();
        if deriv.is_zero() {
            break;
        }
        w -= Float::with_val(prec, &diff / &deriv);
    }
    w
}

pub fn builtin_erf_inverse(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("ErfInverse", args, 1)?;
    let prec = DEFAULT_PRECISION;
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) => {
            if n.to_f64().abs() >= 1.0 {
                return Err(EvalError::Error(
                    "ErfInverse: argument must be in (-1, 1)".to_string(),
                ));
            }
            Ok(Value::Real(inverse_erf_float(&Float::with_val(prec, n))))
        }
        Value::Real(r) => {
            if r.to_f64().abs() >= 1.0 {
                return Err(EvalError::Error(
                    "ErfInverse: argument must be in (-1, 1)".to_string(),
                ));
            }
            Ok(Value::Real(inverse_erf_float(r)))
        }
        _ => Ok(unevaluated("ErfInverse", args)),
    }
}

// ── Beta Function ──

/// Symbolic `Sqrt[Pi]`.
fn sqrt_pi() -> Value {
    crate::builtins::symbolic::simplify_call("Sqrt", &[Value::Symbol("Pi".to_string())])
}

/// True for the exact rational 1/2.
fn is_one_half(v: &Value) -> bool {
    matches!(v, Value::Rational(r) if *r.numer() == 1 && *r.denom() == 2)
}

/// Exact `B(1/2, n) = 2^n (n-1)! / (2n-1)!!` for a positive integer `n`.
fn half_beta_value(n: &Integer) -> Value {
    let n = n.to_u32().unwrap_or(1);
    let mut num = Integer::from(2).pow(n);
    for i in 2..n {
        num *= i; // (n-1)!
    }
    let mut den = Integer::from(1);
    let mut k = 2 * n as i64 - 1;
    while k >= 1 {
        den *= k;
        k -= 2;
    }
    Value::Rational(Box::new(rug::Rational::from((num, den))))
}

/// Numeric Beta via the Gamma quotient at full precision.
fn beta_numeric(x: &Float, y: &Float) -> Float {
    let prec = x.prec().max(y.prec()).max(DEFAULT_PRECISION);
    let fx = Float::with_val(prec, x);
    let fy = Float::with_val(prec, y);
    let sum = Float::with_val(prec, &fx + &fy);
    fx.gamma() * fy.gamma() / sum.gamma()
}

pub fn builtin_beta(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Beta", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Integer(x), Value::Integer(y)) if x.is_positive() && y.is_positive() => {
            // Exact: B(x, y) = (x-1)! (y-1)! / (x+y-1)! for positive integers.
            let x = x.to_i64().unwrap_or(1);
            let y = y.to_i64().unwrap_or(1);
            let mut num = Integer::from(1);
            for i in 2..x {
                num *= i;
            }
            for i in 2..y {
                num *= i;
            }
            let mut den = Integer::from(1);
            for i in 2..(x + y) {
                den *= i;
            }
            Ok(Value::Rational(Box::new(rug::Rational::from((num, den)))))
        }
        // B(1/2, 1/2) = Pi.
        (a, b) if is_one_half(a) && is_one_half(b) => Ok(Value::Symbol("Pi".to_string())),
        // B(1/2, n) = 2^n (n-1)! / (2n-1)!! (and symmetrically).
        (Value::Rational(_), Value::Integer(n)) if n.is_positive() => Ok(half_beta_value(n)),
        (Value::Integer(n), Value::Rational(_)) if n.is_positive() => Ok(half_beta_value(n)),
        _ => {
            let to_f = |v: &Value| -> Option<Float> {
                match v {
                    Value::Integer(n) => Some(Float::with_val(DEFAULT_PRECISION, n)),
                    Value::Real(r) => Some(r.clone()),
                    Value::Rational(r) => Some(Float::with_val(DEFAULT_PRECISION, &**r)),
                    _ => None,
                }
            };
            match (to_f(&args[0]), to_f(&args[1])) {
                (Some(x), Some(y)) if x > 0 && y > 0 => Ok(Value::Real(beta_numeric(&x, &y))),
                _ => Ok(unevaluated("Beta", args)),
            }
        }
    }
}

// ── BetaRegularized ──

/// BetaRegularized[z, a, b] — regularized incomplete beta `I_z(a, b)`.
pub fn builtin_beta_regularized(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "BetaRegularized requires exactly 3 arguments: BetaRegularized[z, a, b]".to_string(),
        ));
    }
    let prec = DEFAULT_PRECISION;
    match &args[0] {
        Value::Integer(n) if n.is_zero() => return Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) if *n == 1 => return Ok(Value::Integer(Integer::from(1))),
        Value::Real(r) if r.is_zero() => return Ok(Value::Integer(Integer::from(0))),
        Value::Real(r) if *r == Float::with_val(prec, 1u32) => {
            return Ok(Value::Integer(Integer::from(1)));
        }
        _ => {}
    }
    let to_f = |v: &Value| -> Option<Float> {
        match v {
            Value::Integer(n) => Some(Float::with_val(prec, n)),
            Value::Real(r) => Some(r.clone()),
            _ => None,
        }
    };
    let (Some(x), Some(a), Some(b)) = (to_f(&args[0]), to_f(&args[1]), to_f(&args[2])) else {
        return Ok(unevaluated("BetaRegularized", args));
    };
    if a <= 0 || b <= 0 || !(0..=1).contains(&x) {
        return Ok(unevaluated("BetaRegularized", args));
    }
    if x.is_zero() {
        return Ok(Value::Integer(Integer::from(0)));
    }
    if x == Float::with_val(prec, 1u32) {
        return Ok(Value::Integer(Integer::from(1)));
    }
    Ok(Value::Real(incomplete_beta(&x, &a, &b)))
}

/// Continued fraction for the incomplete beta (Numerical Recipes `betacf`).
fn beta_cf(a: &Float, b: &Float, x: &Float, prec: u32) -> Float {
    let tiny = Float::with_val(prec, 1e-40);
    let qab = Float::with_val(prec, a) + b;
    let qap = Float::with_val(prec, a) + 1u32;
    let qam = Float::with_val(prec, a) - 1u32;
    let mut c = Float::with_val(prec, 1u32);
    let mut d = Float::with_val(prec, 1u32) - Float::with_val(prec, &qab * x) / &qap;
    if d.clone().abs() < tiny {
        d = tiny.clone();
    }
    d = d.recip();
    let mut h = d.clone();
    for m in 1..=400u32 {
        let m2 = 2 * m;
        // Even step.
        let e1 = Float::with_val(prec, Float::with_val(prec, b) - m);
        let mut aa = Float::with_val(prec, m) * e1;
        aa *= x;
        let d1 = Float::with_val(prec, &qam + m2);
        let d2 = Float::with_val(prec, Float::with_val(prec, a) + m2);
        aa /= Float::with_val(prec, d1 * d2);
        d = Float::with_val(prec, 1u32) + Float::with_val(prec, &aa * &d);
        if d.clone().abs() < tiny {
            d = tiny.clone();
        }
        c = Float::with_val(prec, 1u32) + Float::with_val(prec, &aa / &c);
        if c.clone().abs() < tiny {
            c = tiny.clone();
        }
        d = d.recip();
        h *= Float::with_val(prec, &d * &c);
        // Odd step.
        let t1 = Float::with_val(prec, Float::with_val(prec, a) + m);
        let t2 = Float::with_val(prec, &qab + m);
        let mut aa = -(t1 * t2);
        aa *= x;
        let d1 = Float::with_val(prec, Float::with_val(prec, a) + m2);
        let d2 = Float::with_val(prec, &qap + m2);
        aa /= Float::with_val(prec, d1 * d2);
        d = Float::with_val(prec, 1u32) + Float::with_val(prec, &aa * &d);
        if d.clone().abs() < tiny {
            d = tiny.clone();
        }
        c = Float::with_val(prec, 1u32) + Float::with_val(prec, &aa / &c);
        if c.clone().abs() < tiny {
            c = tiny.clone();
        }
        d = d.recip();
        let del = Float::with_val(prec, &d * &c);
        h *= &del;
        if (del - Float::with_val(prec, 1u32)).abs() < Float::with_val(prec, 1e-40) {
            break;
        }
    }
    h
}

/// Regularized incomplete beta `I_x(a, b)`.
fn incomplete_beta(x: &Float, a: &Float, b: &Float) -> Float {
    let prec = a.prec().max(b.prec()).max(x.prec()).max(DEFAULT_PRECISION);
    let x = Float::with_val(prec, x);
    let a = Float::with_val(prec, a);
    let b = Float::with_val(prec, b);
    let one = Float::with_val(prec, 1u32);
    let ab = Float::with_val(prec, &a + &b);
    let ln_beta = ab.ln_gamma() - a.clone().ln_gamma() - b.clone().ln_gamma();
    let ln_front = ln_beta
        + Float::with_val(prec, &a) * x.clone().ln()
        + Float::with_val(prec, &b) * (Float::with_val(prec, &one - &x)).ln();
    let bt = ln_front.exp();
    let one_plus = Float::with_val(prec, &a) + 1u32;
    let ab2 = Float::with_val(prec, &a + &b) + 2u32;
    if x < one_plus / ab2 {
        bt * beta_cf(&a, &b, &x, prec) / &a
    } else {
        Float::with_val(prec, &one)
            - bt * beta_cf(&b, &a, &(Float::with_val(prec, &one - &x)), prec) / &b
    }
}

// ── Zeta ──

/// Bernoulli numbers `B_{2r}` for `r = 1..=20` as exact (numerator, denominator).
const BERNOULLI_EVEN: &[(i128, i128)] = &[
    (1, 6),
    (-1, 30),
    (1, 42),
    (-1, 30),
    (5, 66),
    (-691, 2730),
    (7, 6),
    (-3617, 510),
    (43867, 798),
    (-174611, 330),
    (854513, 138),
    (-236364091, 2730),
    (8553103, 6),
    (-23749461029, 870),
    (8615841276005, 14322),
    (-7709321041217, 510),
    (2577687858367, 6),
    (-26315271553053477373, 1919190),
    (2929993913841559, 6),
    (-261082718496449122051, 13530),
];

/// Build an exact rational `n / d`.
fn rational_int(n: i128, d: i128) -> Value {
    Value::Rational(Box::new(rug::Rational::from((
        Integer::from(n),
        Integer::from(d),
    ))))
}

fn bernoulli_float(r: usize, prec: u32) -> Float {
    let (n, d) = BERNOULLI_EVEN[r - 1];
    Float::with_val(prec, Integer::from(n)) / Float::with_val(prec, Integer::from(d))
}

/// True when `n` is a positive even integer no larger than 40.
fn is_small_even_positive(n: &Float) -> Option<u64> {
    let rounded = Float::with_val(n.prec(), n).round();
    let diff = Float::with_val(n.prec(), n - &rounded).abs();
    if diff > 1e-10 {
        return None;
    }
    let int_val = rounded.to_integer()?;
    if !int_val.is_negative()
        && int_val.is_divisible(&Integer::from(2))
        && int_val > 1
        && int_val <= 40
    {
        int_val.to_u64()
    } else {
        None
    }
}

/// Exact `Zeta[k] = Pi^k * r` for positive even `k <= 40`, using
/// `ζ(2n) = (-1)^{n+1} B_{2n} (2π)^{2n} / (2 (2n)!)`.
fn zeta_even_value(k: u64) -> Option<Value> {
    if !(2..=40).contains(&k) || !k.is_multiple_of(2) {
        return None;
    }
    let n = (k / 2) as usize;
    let (num, den) = BERNOULLI_EVEN[n - 1];
    let mut factorial = Integer::from(1);
    for i in 2..=(2 * n as i64) {
        factorial *= i;
    }
    // |B_{2n}| * 2^{2n-1} / (2n)!
    let coef_num = Integer::from(num.abs()) * Integer::from(2).pow(2 * n as u32 - 1);
    let coef_den = Integer::from(den) * factorial;
    let coef = Value::Rational(Box::new(rug::Rational::from((coef_num, coef_den))));
    let pi_pow = crate::builtins::symbolic::simplify_call(
        "Power",
        &[
            Value::Symbol("Pi".to_string()),
            Value::Integer(Integer::from(k)),
        ],
    );
    Some(crate::builtins::symbolic::simplify_call(
        "Times",
        &[pi_pow, coef],
    ))
}

/// Numeric `ζ(k)` for small positive even `k` (used by `PolyLog[s, 1]`).
fn zeta_even(k: u64) -> Float {
    let prec = DEFAULT_PRECISION;
    let n = (k / 2) as usize;
    let (num, den) = BERNOULLI_EVEN[n - 1];
    let two_pi = Float::with_val(prec, rug::float::Constant::Pi) * 2u32;
    let two_pi_k = two_pi.pow(k);
    let mut factorial = Float::with_val(prec, 1u32);
    for i in 2..=k {
        factorial *= Float::with_val(prec, i);
    }
    let b_abs =
        Float::with_val(prec, Integer::from(num.abs())) / Float::with_val(prec, Integer::from(den));
    two_pi_k * b_abs / (Float::with_val(prec, 2u32) * factorial)
}

/// Exact `ζ(k)` for non-positive integers `k` (trivial zeros and the
/// Bernoulli closed form `ζ(1-2m) = -B_{2m}/(2m)`), or `None` past the table.
fn zeta_nonpositive_integer(k: i64) -> Option<Value> {
    if k == 0 {
        return Some(rational(-1, 2));
    }
    if k % 2 == 0 {
        // Negative even: trivial zero.
        return Some(Value::Integer(Integer::from(0)));
    }
    // k = -(2m - 1), m = (1 - k)/2.
    let m = ((1 - k) / 2) as usize;
    if m == 0 || m > BERNOULLI_EVEN.len() {
        return None;
    }
    let (num, den) = BERNOULLI_EVEN[m - 1];
    // ζ(-(2m-1)) = -B_{2m}/(2m).
    Some(rational_int(-num, den * (2 * m as i128)))
}

/// Euler–Maclaurin evaluation of the Hurwitz zeta function
/// `ζ(s, a) = Σ_{k≥0} (a+k)^{-s}` for real `s != 1` and `a > 0`, at full
/// working precision.
pub(crate) fn hurwitz_zeta_em(s: &Float, a: &Float) -> Float {
    let prec = s.prec().max(a.prec()).max(DEFAULT_PRECISION);
    let s = Float::with_val(prec, s);
    let a = Float::with_val(prec, a);
    // Larger shift for negative `s`, where the tail term decays more slowly.
    let n_terms: u32 = if s < 0 {
        let extra = (-s.to_f64()).ceil() as u32;
        (16 + extra).min(600)
    } else {
        16
    };
    let neg_s = -Float::with_val(prec, &s);
    let one = Float::with_val(prec, 1u32);

    let mut sum = Float::with_val(prec, 0u32);
    for k in 0..n_terms {
        let base = Float::with_val(prec, &a) + k;
        sum += base.pow(&neg_s);
    }

    let an = Float::with_val(prec, &a) + n_terms;
    let one_minus_s = Float::with_val(prec, &one - &s);
    sum += Float::with_val(
        prec,
        an.clone().pow(&one_minus_s) / Float::with_val(prec, &s - &one),
    );
    sum += Float::with_val(prec, an.clone().pow(&neg_s)) / Float::with_val(prec, 2u32);

    let an2 = Float::with_val(prec, &an * &an);
    let neg_s_minus_one = Float::with_val(prec, &neg_s - &one);
    let mut rising = Float::with_val(prec, &s); // (s)_1
    let mut fact = Float::with_val(prec, 2u32); // 2!
    let mut pow = an.pow(&neg_s_minus_one); // (a+n)^{-s-1}
    for r in 1..=BERNOULLI_EVEN.len() {
        let b = bernoulli_float(r, prec);
        let mut term = Float::with_val(prec, &b);
        term /= &fact;
        term *= &rising;
        term *= &pow;
        sum += term;
        let rr = r as i32;
        let t1 = Float::with_val(prec, &s) + (2 * rr - 1);
        let t2 = Float::with_val(prec, &s) + (2 * rr);
        rising *= t1;
        rising *= t2;
        fact *= Float::with_val(prec, (2 * r + 1) as u32);
        fact *= Float::with_val(prec, (2 * r + 2) as u32);
        pow /= &an2;
    }
    sum
}

/// Riemann zeta for real `s != 1`: Euler–Maclaurin directly for `s > 0`, and
/// the functional equation for `s <= 0`.
fn zeta_em(s: &Float) -> Float {
    let prec = s.prec().max(DEFAULT_PRECISION);
    let s = Float::with_val(prec, s);
    let one = Float::with_val(prec, 1u32);
    if s.is_zero() {
        return Float::with_val(prec, -0.5);
    }
    if s > Float::with_val(prec, 0u32) {
        return hurwitz_zeta_em(&s, &one);
    }
    // Reflection: ζ(s) = 2^s π^{s-1} Sin(πs/2) Γ(1-s) ζ(1-s).
    let pi = Float::with_val(prec, rug::float::Constant::Pi);
    let one_minus_s = Float::with_val(prec, &one - &s);
    let z = hurwitz_zeta_em(&one_minus_s, &one);
    let sin = (Float::with_val(prec, &pi * &s) / 2u32).sin();
    let gamma = one_minus_s.gamma();
    let s_minus_one = Float::with_val(prec, &s - &one);
    Float::with_val(prec, 2u32).pow(&s) * pi.pow(&s_minus_one) * sin * gamma * z
}

pub fn builtin_zeta(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Zeta", args, 1)?;
    let prec = DEFAULT_PRECISION;
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(rational(-1, 2)),
        Value::Integer(n) => {
            let k = n.to_i64().unwrap_or(i64::MIN);
            if k == 1 {
                return Ok(unevaluated("Zeta", args));
            }
            if k <= 0 {
                if let Some(v) = zeta_nonpositive_integer(k) {
                    return Ok(v);
                }
            } else if let Some(v) = zeta_even_value(k as u64) {
                return Ok(v);
            }
            Ok(Value::Real(zeta_em(&Float::with_val(prec, n))))
        }
        Value::Real(r) if r.is_zero() => Ok(super::real(-0.5)),
        Value::Real(r) => {
            let one = Float::with_val(prec, 1u32);
            if r == &one {
                return Ok(unevaluated("Zeta", args));
            }
            if r.is_integer() {
                if let Some(k) = r.to_integer().and_then(|i| i.to_i64()) {
                    if k == 1 {
                        return Ok(unevaluated("Zeta", args));
                    }
                    if k <= 0
                        && let Some(v) = zeta_nonpositive_integer(k)
                    {
                        return Ok(v);
                    }
                    if k >= 2
                        && let Some(v) = zeta_even_value(k as u64)
                    {
                        return Ok(v);
                    }
                }
            } else if let Some(k) = is_small_even_positive(r)
                && let Some(v) = zeta_even_value(k)
            {
                return Ok(v);
            }
            Ok(Value::Real(zeta_em(r)))
        }
        _ => Ok(unevaluated("Zeta", args)),
    }
}

fn compute_zeta_numeric(s: &Float) -> Float {
    zeta_em(s)
}

// ── PolyLog ──

pub fn builtin_polylog(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "PolyLog requires exactly 2 arguments: PolyLog[s, z]".to_string(),
        ));
    }
    let s_val = &args[0];
    let z_val = &args[1];

    // Exact closed forms.
    match (s_val, z_val) {
        // Li_2(1) = Pi^2/6
        (Value::Integer(s), Value::Integer(z)) if *s == 2 && *z == 1 => {
            return Ok(pi_pow_over(2, 6));
        }
        // Li_2(-1) = -Pi^2/12
        (Value::Integer(s), Value::Integer(z)) if *s == 2 && *z == -1 => {
            return Ok(crate::builtins::symbolic::simplify_call(
                "Times",
                &[rational(-1, 12), pi_pow_over(2, 1)],
            ));
        }
        _ => {}
    }

    if let (Value::Integer(s), _) = (s_val, z_val) {
        if s.is_zero() {
            // Li_0(z) = z / (1 - z)
            if let Some(z) = super::to_f64(z_val) {
                if (z - 1.0).abs() < 1e-15 {
                    return Ok(unevaluated("PolyLog", args));
                }
                return Ok(super::real(z / (1.0 - z)));
            }
            return Ok(unevaluated("PolyLog", args));
        }
        if *s == 1 {
            // Li_1(z) = -Log(1 - z)
            if let Some(z) = super::to_f64(z_val) {
                if z >= 1.0 {
                    return Ok(unevaluated("PolyLog", args));
                }
                return Ok(super::real(-(1.0 - z).ln()));
            }
            return Ok(unevaluated("PolyLog", args));
        }
    }

    if let (Some(s), Some(z)) = (super::to_f64(s_val), super::to_f64(z_val)) {
        // Li_s(1) = Zeta[s] (converges for s > 1); use the zeta machinery so
        // numeric evaluation under `N` stays accurate (the z-series converges
        // too slowly at z = 1).
        if (z - 1.0).abs() < 1e-15 && s > 1.0 {
            let f = Float::with_val(DEFAULT_PRECISION, s);
            if let Some(k) = is_small_even_positive(&f) {
                return Ok(Value::Real(zeta_even(k)));
            }
            return Ok(Value::Real(compute_zeta_numeric(&f)));
        }
        if z.abs() >= 1.0 {
            return Ok(unevaluated("PolyLog", args));
        }
        let mut sum = 0.0_f64;
        let mut z_power = z;
        let tolerance = 1e-15;
        for n in 1..=1_000_000u64 {
            let term = z_power / (n as f64).powf(s);
            sum += term;
            z_power *= z;
            if term.abs() < tolerance {
                break;
            }
        }
        return Ok(super::real(sum));
    }

    Ok(unevaluated("PolyLog", args))
}

// ── Digamma / PolyGamma ──

#[allow(dead_code)]
const EULER_GAMMA_F64: f64 = std::f64::consts::EULER_GAMMA;

/// True when `r` is a non-positive integer (a pole of digamma/polygamma).
fn is_npn_integer(r: &Float) -> bool {
    r <= &Float::with_val(r.prec().max(DEFAULT_PRECISION), 0u32) && r.is_integer()
}

/// Digamma via MPFR's correctly-rounded kernel (full precision, and valid for
/// negative non-integer arguments too).
pub fn builtin_digamma(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Digamma", args, 1)?;
    let prec = DEFAULT_PRECISION;
    match &args[0] {
        Value::Integer(n) => {
            if n.is_negative() || n.is_zero() {
                return Ok(unevaluated("Digamma", args));
            }
            Ok(Value::Real(Float::with_val(prec, n).digamma()))
        }
        Value::Real(r) => {
            if is_npn_integer(r) {
                return Ok(unevaluated("Digamma", args));
            }
            Ok(Value::Real(r.clone().digamma()))
        }
        _ => Ok(unevaluated("Digamma", args)),
    }
}

/// `ψ^{(n)}(z) = (-1)^{n+1} n! ζ(n+1, z)` for `z > 0` (Euler–Maclaurin).
fn polygamma_positive(n: u32, z: &Float) -> Float {
    let prec = z.prec().max(DEFAULT_PRECISION);
    let z = Float::with_val(prec, z);
    if n == 0 {
        return z.digamma();
    }
    let s = Float::with_val(prec, n + 1);
    let zeta = hurwitz_zeta_em(&s, &z);
    let mut factorial = Float::with_val(prec, 1u32);
    for i in 2..=n {
        factorial *= Float::with_val(prec, i);
    }
    let sign = if n % 2 == 1 { 1i32 } else { -1i32 };
    let mut result = factorial * zeta;
    if sign < 0 {
        result = -result;
    }
    result
}

pub fn builtin_polygamma(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("PolyGamma", args, 2)?;
    let n_val = &args[0];
    let z_val = &args[1];

    // PolyGamma[0, z] = Digamma[z]
    if let (Value::Integer(n), _) = (n_val, z_val) {
        if n.is_zero() {
            return builtin_digamma(&[z_val.clone()]);
        }
        if n.is_negative() {
            return Ok(unevaluated("PolyGamma", args));
        }
    }

    if let (Some(n), Some(z)) = (super::to_f64(n_val), super::to_f64(z_val)) {
        if z <= 0.0 || n < 0.0 || n.fract() != 0.0 {
            return Ok(unevaluated("PolyGamma", args));
        }
        let n_int = n as u32;
        if n_int == 0 {
            return builtin_digamma(&[z_val.clone()]);
        }
        let zf = match z_val {
            Value::Real(r) => r.clone(),
            _ => Float::with_val(DEFAULT_PRECISION, z),
        };
        Ok(Value::Real(polygamma_positive(n_int, &zf)))
    } else {
        Ok(unevaluated("PolyGamma", args))
    }
}

// ── AiryAi ──

fn airy_ai_approx(z: f64) -> f64 {
    if z >= 0.0 {
        // Asymptotic for positive z (NIST DLMF 9.7.5)
        // Ai(z) ~ (1/(2*sqrt(pi))) * z^(-1/4) * e^(-2/3 * z^(3/2))
        //        * (sum1 - z^(-3/2) * sum2 + ...)
        // Use series expansion for small z and asymptotic for large z.
        if z < 0.5 {
            // Power series: Ai(z) = c0 * (1 + z^3/6 + 7*z^6/360 + ...)
            //                   - c1 * (z + z^4/12 + z^7/720 + ...)
            // Ai(0) = c0 ≈ 0.355028, Ai'(0) = -c1 ≈ -0.259329
            let ai0 = 0.355_028_053_887_817_2;
            let ai0_prime = -0.259_329_853_809_084_85;
            // Series: Ai(z) = ai0 * Σ (3^k * z^(3k)) / (3^k * k! * 2^k * 3^k * ...)
            // Simpler: use the known series form
            // Ai(z) = ai0 * S0(z) + ai0_prime * S1(z)
            // where S0 = 1 + z^3/6 + 7z^6/360 + ...
            //       S1 = z + z^4/12 + z^7/720 + ...
            let z2 = z * z;
            let z3 = z2 * z;
            let z6 = z3 * z3;
            // S0 = 1 + z^3/6 + 7 z^6/360 + ...
            let s0 = 1.0 + z3 / 6.0 + 7.0 * z6 / 360.0;
            // S1 = z + z^4/12 + z^7/720 + ...
            let s1 = z + z * z3 / 12.0 + z * z6 / 720.0;
            ai0 * s0 + ai0_prime * s1
        } else {
            // Asymptotic expansion for z >= 0.5
            let z32 = z.sqrt() * z; // z^(3/2)
            let exp_factor = (-2.0 / 3.0 * z32).exp();
            let z14 = z.sqrt().sqrt(); // z^(1/4)
            let leading = exp_factor / (2.0 * std::f64::consts::PI.sqrt() * z14);
            // First few terms of the asymptotic series
            let u = 1.0 / (z32);
            let p =
                1.0 - 5.0 / 144.0 * u + 2005.0 / 21504.0 * u * u - 156205.0 / 2129920.0 * u * u * u;
            leading * p
        }
    } else {
        // For negative z, oscillatory behavior (NIST DLMF 9.6.1)
        let az = -z;
        let az32 = az.sqrt() * az; // |z|^(3/2)
        let theta = 2.0 / 3.0 * az32 + std::f64::consts::PI / 4.0;
        let az14 = az.sqrt().sqrt(); // |z|^(1/4)
        let leading = 1.0 / (std::f64::consts::PI.sqrt() * az14);
        let u = 1.0 / (az32);
        let p = 1.0 - 5.0 / 144.0 * u + 2005.0 / 21504.0 * u * u;
        leading * p * theta.sin()
    }
}

/// Exact Ai(0) = 1 / (3^(2/3) Gamma[2/3]).
fn airy_ai_zero_exact() -> Value {
    let sc = crate::builtins::symbolic::simplify_call;
    let three_pow = sc(
        "Power",
        &[Value::Integer(Integer::from(3)), rational(-2, 3)],
    );
    let gamma_23 = sc("Gamma", &[rational(2, 3)]);
    let gamma_inv = sc("Power", &[gamma_23, Value::Integer(Integer::from(-1))]);
    sc("Times", &[three_pow, gamma_inv])
}

pub fn builtin_airy_ai(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "AiryAi requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        // Ai(0) = 1 / (3^(2/3) Gamma[2/3])
        Value::Integer(n) if n.is_zero() => Ok(airy_ai_zero_exact()),
        // Under `N` the argument arrives as a real; use the numeric value so it
        // stays consistent with the numerically-evaluated expected form.
        Value::Real(r) if r.is_zero() => Ok(super::real(airy_ai_approx(0.0))),
        Value::Integer(n) => Ok(super::real(airy_ai_approx(n.to_f64()))),
        Value::Real(r) => Ok(super::real(airy_ai_approx(r.to_f64()))),
        _ => Ok(unevaluated("AiryAi", args)),
    }
}

// ── BesselJ ──

/// Sqrt[2/(pi z)] — the half-integer Bessel prefactor.
fn sqrt_two_over_pi_z(z: &Value, pi: &Value, two: &Value) -> Value {
    let sc = crate::builtins::symbolic::simplify_call;
    let pi_z = sc("Times", &[pi.clone(), z.clone()]);
    sc("Sqrt", &[sc("Divide", &[two.clone(), pi_z])])
}

/// Exact closed form of J_{numer/2}(z) for an odd integer `numer`.
///
/// Uses J_{1/2}(z) = Sqrt[2/(Pi z)] Sin[z], J_{-1/2}(z) = Sqrt[2/(Pi z)] Cos[z]
/// and the recurrence J_{v+1}(z) = (2v/z) J_v(z) - J_{v-1}(z). When `exact` is
/// false the prefactor constants are numeric reals, matching the shape produced
/// by `N` (which numericises the order `1/2` to `0.5` before dispatch).
fn bessel_j_half_symbolic(numer: i64, z: &Value, exact: bool) -> Value {
    let sc = crate::builtins::symbolic::simplify_call;
    let (pi, two, neg_one) = if exact {
        (
            Value::Symbol("Pi".to_string()),
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(-1)),
        )
    } else {
        (
            Value::Real(Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi)),
            super::real(2.0),
            super::real(-1.0),
        )
    };
    let zinv = sc("Power", &[z.clone(), neg_one.clone()]);
    let minus_one = Value::Integer(Integer::from(-1));
    // Coefficients (a, b) with J_v(z) = prefactor * (a Sin[z] + b Cos[z]).
    let (mut a, mut b);
    if numer > 0 {
        a = Value::Integer(Integer::from(1));
        b = Value::Integer(Integer::from(0));
        let mut prev_a = Value::Integer(Integer::from(0));
        let mut prev_b = Value::Integer(Integer::from(1));
        for i in 0..(numer - 1) / 2 {
            let coeff = sc(
                "Times",
                &[Value::Integer(Integer::from(2 * i + 1)), zinv.clone()],
            );
            let na = sc(
                "Plus",
                &[
                    sc("Times", &[coeff.clone(), a.clone()]),
                    sc("Times", &[minus_one.clone(), prev_a.clone()]),
                ],
            );
            let nb = sc(
                "Plus",
                &[
                    sc("Times", &[coeff, b.clone()]),
                    sc("Times", &[minus_one.clone(), prev_b.clone()]),
                ],
            );
            prev_a = a;
            prev_b = b;
            a = na;
            b = nb;
        }
    } else {
        a = Value::Integer(Integer::from(0));
        b = Value::Integer(Integer::from(1));
        let mut next_a = Value::Integer(Integer::from(1));
        let mut next_b = Value::Integer(Integer::from(0));
        for i in 0..((-numer) - 1) / 2 {
            let coeff = sc(
                "Times",
                &[Value::Integer(Integer::from(-(2 * i + 1))), zinv.clone()],
            );
            let na = sc(
                "Plus",
                &[
                    sc("Times", &[coeff.clone(), a.clone()]),
                    sc("Times", &[minus_one.clone(), next_a.clone()]),
                ],
            );
            let nb = sc(
                "Plus",
                &[
                    sc("Times", &[coeff, b.clone()]),
                    sc("Times", &[minus_one.clone(), next_b.clone()]),
                ],
            );
            next_a = a;
            next_b = b;
            a = na;
            b = nb;
        }
    }
    let aterm = sc("Times", &[a, sc("Sin", &[z.clone()])]);
    let bterm = sc("Times", &[b, sc("Cos", &[z.clone()])]);
    let combo = sc("Plus", &[aterm, bterm]);
    sc("Times", &[sqrt_two_over_pi_z(z, &pi, &two), combo])
}

/// Extract an odd numerator `m` from a half-integer order value (m/2), where
/// the order may be an exact `Rational` or a numeric `Real`.
fn half_integer_bessel_numer(n: &Value) -> Option<i64> {
    match n {
        Value::Rational(r) if *r.denom() == 2 && r.numer().is_odd() => r.numer().to_i64(),
        Value::Real(r) => {
            let two_x = 2.0 * r.to_f64();
            let rounded = two_x.round();
            if (two_x - rounded).abs() < 1e-12 && (rounded as i64) % 2 != 0 {
                Some(rounded as i64)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn builtin_bessel_j(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "BesselJ requires exactly 2 arguments: BesselJ[n, z]".to_string(),
        ));
    }
    let n_val = &args[0];
    let z_val = &args[1];

    // J_n(0) special cases
    if let Value::Integer(z) = z_val
        && z.is_zero()
        && let Value::Integer(n) = n_val
    {
        if n.is_zero() {
            return Ok(Value::Integer(Integer::from(1)));
        }
        return Ok(Value::Integer(Integer::from(0)));
    }
    if let Value::Real(z) = z_val
        && z.is_zero()
        && let Value::Integer(n) = n_val
    {
        if n.is_zero() {
            return Ok(Value::Integer(Integer::from(1)));
        }
        return Ok(Value::Integer(Integer::from(0)));
    }

    // Half-integer order: exact closed form via elementary functions.
    if let Some(numer) = half_integer_bessel_numer(n_val) {
        let exact = !matches!(n_val, Value::Real(_));
        return Ok(bessel_j_half_symbolic(numer, z_val, exact));
    }

    match (n_val, z_val) {
        (Value::Integer(n), _) if !n.is_negative() => {
            if let (Some(n_u64), Some(z)) = (n.to_u64(), super::to_f64(z_val)) {
                return Ok(super::real(bessel_j_series(n_u64, z)));
            }
        }
        _ => {}
    }

    Ok(unevaluated("BesselJ", args))
}

fn bessel_j_series(n: u64, z: f64) -> f64 {
    if z == 0.0 {
        return if n == 0 { 1.0 } else { 0.0 };
    }
    // J_n(z) = Σ_{k=0}^∞ (-1)^k * (z/2)^(2k+n) / (k! * Gamma(n + k + 1))
    let half_z = z / 2.0;
    let half_z_n = half_z.powi(n as i32);
    let tolerance = 1e-15;
    let mut sum = 0.0_f64;
    let mut fact_k = 1.0_f64;
    // Gamma(n + k + 1) = (n+k)! for integer n
    let mut gamma_nk1 = 1.0_f64;
    for j in 1..=n {
        gamma_nk1 *= j as f64;
    }
    let mut z_power = half_z_n;
    // The alternating sign depends only on k, not on n.
    sum += z_power / gamma_nk1;
    for k in 1..=100 {
        fact_k *= k as f64;
        gamma_nk1 *= (n + k) as f64;
        z_power *= half_z * half_z;
        let sign = if k.is_multiple_of(2) { 1.0 } else { -1.0 };
        let term = sign * z_power / (fact_k * gamma_nk1);
        sum += term;
        if term.abs() < tolerance {
            break;
        }
    }
    sum
}

// ── BesselY (stub: unevaluated) ──

pub fn builtin_bessel_y(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "BesselY requires exactly 2 arguments: BesselY[n, z]".to_string(),
        ));
    }
    // BesselY is complex (requires limit computation); return unevaluated
    Ok(unevaluated("BesselY", args))
}

// ── BesselI ──

pub fn builtin_bessel_i(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "BesselI requires exactly 2 arguments: BesselI[n, z]".to_string(),
        ));
    }
    // I_n(0): 1 for n == 0, 0 otherwise (exact).
    if let (Value::Integer(n), z) = (&args[0], &args[1])
        && super::to_f64(z).is_some_and(|z| z == 0.0)
    {
        if *n == 0 {
            return Ok(Value::Integer(Integer::from(1)));
        }
        return Ok(Value::Integer(Integer::from(0)));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(n), &_) if !n.is_negative() => {
            if let (Some(n), Some(z)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) {
                return Ok(super::real(bessel_i_series(n as u64, z)));
            }
        }
        _ => {}
    }
    Ok(unevaluated("BesselI", args))
}

fn bessel_i_series(n: u64, z: f64) -> f64 {
    // I_n(z) = Σ_{k=0}^∞ (z/2)^(2k+n) / (k! * Gamma(n + k + 1))
    if z == 0.0 {
        return if n == 0 { 1.0 } else { 0.0 };
    }
    let half_z = z / 2.0;
    let half_z_n = half_z.powi(n as i32);
    let tolerance = 1e-15;
    let mut sum = 0.0_f64;
    let mut fact_k = 1.0_f64;
    let mut gamma_nk1 = 1.0_f64;
    for j in 1..=n {
        gamma_nk1 *= j as f64;
    }
    let mut z_power = half_z_n;
    sum += z_power / gamma_nk1;
    for k in 1..=100 {
        fact_k *= k as f64;
        gamma_nk1 *= (n + k) as f64;
        z_power *= half_z * half_z;
        let term = z_power / (fact_k * gamma_nk1);
        sum += term;
        if term.abs() < tolerance {
            break;
        }
    }
    sum
}

// ── BesselK (stub: unevaluated) ──

pub fn builtin_bessel_k(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "BesselK requires exactly 2 arguments: BesselK[n, z]".to_string(),
        ));
    }
    Ok(unevaluated("BesselK", args))
}

// ── LambertW ──

pub fn builtin_lambert_w(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "LambertW requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) => {
            let z = n.to_f64();
            if z < NEG_INV_E {
                return Err(EvalError::Error(
                    "LambertW: argument must be >= -1/e".to_string(),
                ));
            }
            Ok(super::real(lambert_w_approx(z)))
        }
        Value::Real(r) => {
            let z = r.to_f64();
            if z < NEG_INV_E {
                return Err(EvalError::Error(
                    "LambertW: argument must be >= -1/e".to_string(),
                ));
            }
            Ok(super::real(lambert_w_approx(z)))
        }
        _ => Ok(unevaluated("LambertW", args)),
    }
}

const NEG_INV_E: f64 = -1.0 / std::f64::consts::E;

fn lambert_w_approx(z: f64) -> f64 {
    if z == 0.0 {
        return 0.0;
    }
    if (z - NEG_INV_E).abs() < 1e-15 {
        return -1.0;
    }
    // Initial guess: ln(z) is a good starting point for z > 0; the branch point
    // -1/e (handled above) is the sensible start for z < 0.
    let mut w = if z > 0.0 { z.ln() } else { -1.0 };
    // Halley's method on f(w) = w - z e^{-w} = 0 (the defining equation w e^w = z).
    for _ in 0..50 {
        let e_neg_w = (-w).exp();
        let f = w - z * e_neg_w;
        let fp = 1.0 + z * e_neg_w;
        let fp2 = -z * e_neg_w;
        if fp == 0.0 {
            break;
        }
        let delta = f / fp * (1.0 + f * fp2 / (2.0 * fp * fp));
        w -= delta;
        if delta.abs() < 1e-15 {
            break;
        }
    }
    w
}

// ── DirichletEta ──

/// Exact `η(k)` for non-positive integers via `η(k) = (1 - 2^{1-k}) ζ(k)`.
fn eta_nonpositive_integer(k: i64) -> Option<Value> {
    if k == 0 {
        return Some(rational(1, 2));
    }
    if k % 2 == 0 {
        // ζ(k) = 0 at negative even integers.
        return Some(Value::Integer(Integer::from(0)));
    }
    let m = ((1 - k) / 2) as usize; // k = -(2m - 1)
    if m == 0 || m > BERNOULLI_EVEN.len() {
        return None;
    }
    let (num, den) = BERNOULLI_EVEN[m - 1];
    // η(k) = (1 - 2^{2m}) * (-B_{2m}/(2m)).
    let factor = Integer::from(1) - Integer::from(2).pow(2 * m as u32);
    Some(Value::Rational(Box::new(rug::Rational::from((
        factor * Integer::from(-num),
        Integer::from(den) * Integer::from(2 * m as i64),
    )))))
}

/// η(s) = (1 - 2^{1-s}) ζ(s) at full precision.
fn eta_em(s: &Float) -> Float {
    let prec = s.prec().max(DEFAULT_PRECISION);
    let s = Float::with_val(prec, s);
    let one = Float::with_val(prec, 1u32);
    let two = Float::with_val(prec, 2u32);
    let factor = Float::with_val(prec, &one - two.pow(&Float::with_val(prec, &one - &s)));
    zeta_em(&s) * factor
}

pub fn builtin_dirichlet_eta(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("DirichletEta", args, 1)?;
    let prec = DEFAULT_PRECISION;
    let ln2 = || Value::Real(Float::with_val(prec, 2u32).ln());
    match &args[0] {
        Value::Integer(n) => {
            let k = n.to_i64().unwrap_or(0);
            if k == 1 {
                return Ok(ln2());
            }
            if k <= 0 {
                if let Some(v) = eta_nonpositive_integer(k) {
                    return Ok(v);
                }
            } else if k % 2 == 0
                && let Some(zeta) = zeta_even_value(k as u64)
            {
                let p = Integer::from(2).pow(k as u32 - 1);
                let factor = Value::Rational(Box::new(rug::Rational::from((
                    p.clone() - Integer::from(1),
                    p,
                ))));
                return Ok(crate::builtins::symbolic::simplify_call(
                    "Times",
                    &[zeta, factor],
                ));
            }
            Ok(Value::Real(eta_em(&Float::with_val(prec, n))))
        }
        Value::Real(r) if r.is_zero() => Ok(rational(1, 2)),
        Value::Real(r) => {
            if r == &Float::with_val(prec, 1u32) {
                return Ok(ln2());
            }
            if r.is_integer()
                && let Some(k) = r.to_integer().and_then(|i| i.to_i64())
            {
                if k <= 0
                    && let Some(v) = eta_nonpositive_integer(k)
                {
                    return Ok(v);
                }
                if k >= 2
                    && k % 2 == 0
                    && let Some(zeta) = zeta_even_value(k as u64)
                {
                    let p = Integer::from(2).pow(k as u32 - 1);
                    let factor = Value::Rational(Box::new(rug::Rational::from((
                        p.clone() - Integer::from(1),
                        p,
                    ))));
                    return Ok(crate::builtins::symbolic::simplify_call(
                        "Times",
                        &[zeta, factor],
                    ));
                }
            }
            Ok(Value::Real(eta_em(r)))
        }
        _ => Ok(unevaluated("DirichletEta", args)),
    }
}

// ── InverseErf ──

/// InverseErf[x] — inverse error function (alias of ErfInverse) at full
/// precision via Newton iteration.
pub fn builtin_inverse_erf(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("InverseErf", args, 1)?;
    let prec = DEFAULT_PRECISION;
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) => {
            if n.to_f64().abs() >= 1.0 {
                return Err(EvalError::Error(
                    "InverseErf: argument must be in (-1, 1)".to_string(),
                ));
            }
            Ok(Value::Real(inverse_erf_float(&Float::with_val(prec, n))))
        }
        Value::Real(r) => {
            if r.to_f64().abs() >= 1.0 {
                return Err(EvalError::Error(
                    "InverseErf: argument must be in (-1, 1)".to_string(),
                ));
            }
            Ok(Value::Real(inverse_erf_float(r)))
        }
        _ => Ok(unevaluated("InverseErf", args)),
    }
}

// ── InverseErfc ──

/// InverseErfc[x] — inverse complementary error function. InverseErfc[x] = InverseErf[1 - x].
pub fn builtin_inverse_erfc(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("InverseErfc", args, 1)?;
    let prec = DEFAULT_PRECISION;
    match &args[0] {
        Value::Integer(n) if *n == 1 => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) => {
            let x = n.to_f64();
            if x <= 0.0 || x >= 2.0 {
                return Err(EvalError::Error(
                    "InverseErfc: argument must be in (0, 2)".to_string(),
                ));
            }
            let one_minus_x = Float::with_val(prec, 1u32) - Float::with_val(prec, n);
            Ok(Value::Real(inverse_erf_float(&one_minus_x)))
        }
        Value::Real(r) => {
            let x = r.to_f64();
            if x <= 0.0 || x >= 2.0 {
                return Err(EvalError::Error(
                    "InverseErfc: argument must be in (0, 2)".to_string(),
                ));
            }
            let one_minus_x = Float::with_val(prec, 1u32) - Float::with_val(prec, r);
            Ok(Value::Real(inverse_erf_float(&one_minus_x)))
        }
        _ => Ok(unevaluated("InverseErfc", args)),
    }
}

// ── LogGamma ──

/// LogGamma[z] — natural log of the Gamma function.
/// Uses Stirling's approximation with Lanczos coefficients for positive reals.
pub fn builtin_log_gamma(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("LogGamma", args, 1)?;
    match &args[0] {
        Value::Integer(n) if *n == 1 => Ok(super::real(0.0)),
        Value::Integer(n) if *n > 0 => {
            // LogGamma[n] = Log[(n-1)!]
            let n_i64 = n.to_i64().unwrap_or(1);
            let mut log_fact = 0.0_f64;
            for i in 1..n_i64 {
                log_fact += (i as f64).ln();
            }
            Ok(super::real(log_fact))
        }
        Value::Integer(_) => {
            // Negative integer: use reflection formula
            Ok(unevaluated("LogGamma", args))
        }
        Value::Real(r) => {
            let z = r.to_f64();
            if z <= 0.0 && z == z.floor() {
                // Pole at non-positive integer
                return Ok(unevaluated("LogGamma", args));
            }
            Ok(super::real(log_gamma_lanczos(z)))
        }
        _ => Ok(unevaluated("LogGamma", args)),
    }
}

fn log_gamma_lanczos(z: f64) -> f64 {
    // Lanczos coefficients (g=7, n=9)
    const G: f64 = 7.0;
    const COEFF: [f64; 9] = [
        0.999_999_999_999_81,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];

    if z < 0.5 {
        // Reflection formula: Gamma(z) * Gamma(1-z) = pi / sin(pi*z)
        // LogGamma(z) = log(pi) - log(sin(pi*z)) - LogGamma(1-z)
        let sin_pz = (std::f64::consts::PI * z).sin();
        std::f64::consts::PI.ln() - sin_pz.abs().ln() - log_gamma_lanczos(1.0 - z)
    } else {
        let z1 = z - 1.0;
        let mut x = COEFF[0];
        for (i, coeff) in COEFF.iter().enumerate().skip(1) {
            x += coeff / (z1 + i as f64);
        }
        let t = z1 + G + 0.5;
        0.5 * (2.0 * std::f64::consts::PI).ln() + (z1 + 0.5) * t.ln() - t + x.ln()
    }
}

// ── GammaRegularized ──

/// GammaRegularized[a, z] — lower regularized incomplete gamma `P(a, z)`.
pub fn builtin_gamma_regularized(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("GammaRegularized", args, 2)?;
    let prec = DEFAULT_PRECISION;
    let (a, z) = match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(z)) => {
            (Float::with_val(prec, a), Float::with_val(prec, z))
        }
        (Value::Real(a), Value::Real(z)) => (a.clone(), z.clone()),
        (Value::Integer(a), Value::Real(z)) => (Float::with_val(prec, a), z.clone()),
        (Value::Real(a), Value::Integer(z)) => (a.clone(), Float::with_val(prec, z)),
        _ => return Ok(unevaluated("GammaRegularized", args)),
    };
    if a <= 0 || z < 0 {
        return Ok(unevaluated("GammaRegularized", args));
    }
    if z.is_zero() {
        return Ok(Value::Integer(Integer::from(0)));
    }
    Ok(Value::Real(gamma_p(&a, &z)))
}

/// `P(a, z)`: series (all-positive terms) for `z < a+1`, otherwise `1 - Q`
/// with `Q` from MPFR's upper incomplete gamma to avoid cancellation.
fn gamma_p(a: &Float, z: &Float) -> Float {
    let prec = a.prec().max(z.prec()).max(DEFAULT_PRECISION);
    let a = Float::with_val(prec, a);
    let z = Float::with_val(prec, z);
    if z < Float::with_val(prec, &a) + 1u32 {
        let mut sum = Float::with_val(prec, 1u32);
        let mut term = Float::with_val(prec, 1u32);
        for n in 1..2000u32 {
            term *= &z;
            term /= Float::with_val(prec, &a) + n;
            sum += &term;
            if term < Float::with_val(prec, 1e-42) * &sum {
                break;
            }
        }
        let mut result = (-Float::with_val(prec, &z)).exp();
        result *= z.clone().pow(&a);
        result /= (Float::with_val(prec, &a) + 1u32).gamma();
        result * sum
    } else {
        let gamma_a = Float::with_val(prec, &a).gamma();
        let upper = Float::with_val(prec, &a).gamma_inc(&z); // Γ(a, z)
        Float::with_val(prec, 1u32) - upper / gamma_a
    }
}

// ── AiryBi ──

/// AiryBi[z] — Airy function Bi(z).
pub fn builtin_airy_bi(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("AiryBi", args, 1)?;
    match &args[0] {
        Value::Integer(n) if n.is_zero() => {
            // Bi(0) = 1/(3^(2/3) * Gamma(2/3))
            Ok(super::real(airy_bi_approx(0.0)))
        }
        Value::Integer(n) => Ok(super::real(airy_bi_approx(n.to_f64()))),
        Value::Real(r) => Ok(super::real(airy_bi_approx(r.to_f64()))),
        _ => Ok(unevaluated("AiryBi", args)),
    }
}

fn airy_bi_approx(z: f64) -> f64 {
    // Bi(z) = (1/pi) * integral_0^inf exp(t^3/3 + z*t) dt  [not directly used]
    // Use series expansion for small z, asymptotic for large z.
    if z >= 0.0 {
        if z < 2.0 {
            // Series: Bi(z) = bi0 * S0(z) + bi0_prime * S1(z)
            // where bi0 = Bi(0) ≈ 0.6149266274460007
            //       bi0' = Bi'(0) ≈ 0.4482883573538264
            let bi0 = 0.6149266274460007;
            let bi0_prime = 0.4482883573538264;
            // S0 = 1 + z^3/6 + 7z^6/360 + ...
            // S1 = z + z^4/12 + z^7/720 + ...
            let z2 = z * z;
            let z3 = z2 * z;
            let z6 = z3 * z3;
            let z9 = z6 * z3;
            let mut s0 = 1.0 + z3 / 6.0 + 7.0 * z6 / 360.0;
            let mut s1 = z + z3 * z / 12.0 + z6 * z / 720.0;
            // Higher terms
            let z12 = z9 * z3;
            s0 += 7.0 * z9 / 25920.0;
            s1 += 7.0 * z12 / 30240.0;
            bi0 * s0 + bi0_prime * s1
        } else {
            // Asymptotic for large positive z (NIST DLMF 9.7.6)
            // Bi(z) ~ (1/sqrt(pi)) * z^(-1/4) * exp(2/3 * z^(3/2))
            //        * (1 + sum_{k=1} c_k * (2/3 * z^(3/2))^(-k))
            let z32 = z.powf(1.5);
            let z14 = z.sqrt().sqrt();
            let exp_factor = (2.0 / 3.0 * z32).exp();
            let leading = exp_factor / (std::f64::consts::PI.sqrt() * z14);
            let u = 1.0 / (2.0 / 3.0 * z32);
            let p = 1.0 + 5.0 / 48.0 * u - 7.0 / 128.0 * u * u + 135.0 / 1024.0 * u * u * u;
            leading * p
        }
    } else {
        // Negative z: oscillatory
        let az = -z;
        let az32 = az.powf(1.5);
        let theta = 2.0 / 3.0 * az32 + std::f64::consts::PI / 4.0;
        let az14 = az.sqrt().sqrt();
        let leading = 1.0 / (std::f64::consts::PI.sqrt() * az14);
        let u = 1.0 / (2.0 / 3.0 * az32);
        let p = 1.0 - 5.0 / 48.0 * u + 7.0 / 128.0 * u * u;
        leading * p * theta.sin()
    }
}

// ── Gamma (exact special values) ──

/// Exact `Gamma[p/2]` for odd `p` using `Γ(1/2) = Sqrt[Pi]` and the
/// reflection/recurrence identities, or `None` for other arguments.
fn gamma_half_integer(r: &rug::Rational) -> Option<Value> {
    if *r.denom() != 2 {
        return None;
    }
    let p = r.numer().to_i64()?;
    if p % 2 == 0 {
        return None;
    }
    let (num, den): (i128, i128) = if p > 0 {
        // Γ(p/2) = (p-2)!! / 2^{(p-1)/2}
        let mut double_fact: i128 = 1;
        let mut k = p - 2;
        while k >= 1 {
            double_fact *= k as i128;
            k -= 2;
        }
        (double_fact, 1i128 << ((p - 1) / 2))
    } else {
        // p = -(2m-1): Γ(-m+1/2) = (-2)^m / (2m-1)!!
        let m = (1 - p) / 2;
        let mut double_fact: i128 = 1;
        let mut k = 2 * m - 1;
        while k >= 1 {
            double_fact *= k as i128;
            k -= 2;
        }
        let sign: i128 = if m % 2 == 0 { 1 } else { -1 };
        (sign * (1i128 << m), double_fact)
    };
    let coef = Value::Rational(Box::new(rug::Rational::from((
        Integer::from(num),
        Integer::from(den),
    ))));
    Some(crate::builtins::symbolic::simplify_call(
        "Times",
        &[coef, sqrt_pi()],
    ))
}

/// Gamma[z] with exact values at half-integers (`Sqrt[Pi]` times a rational);
/// otherwise delegates to the numeric builtin in `math`.
pub fn builtin_gamma_exact(args: &[Value]) -> Result<Value, EvalError> {
    if let [Value::Rational(r)] = args
        && let Some(v) = gamma_half_integer(r)
    {
        return Ok(v);
    }
    crate::builtins::math::builtin_gamma(args)
}

// ── BarnesG ──

/// ζ'(-1), the constant in the Barnes G asymptotic expansion.
const ZETA_PRIME_NEG1: f64 = -0.165_421_143_700_450_93;

/// `ln G(x+1)` asymptotic series (DLMF 5.17.5).
fn barnes_g_ln_asym(x: &Float) -> Float {
    let prec = x.prec().max(DEFAULT_PRECISION);
    let x = Float::with_val(prec, x);
    let pi = Float::with_val(prec, rug::float::Constant::Pi);
    let x2 = Float::with_val(prec, &x * &x);
    let mut s =
        (Float::with_val(prec, &x2) / 2u32 - Float::with_val(prec, 1u32) / 12u32) * x.clone().ln();
    s -= Float::with_val(prec, 3u32) * &x2 / 4u32;
    s += Float::with_val(prec, &x) / 2u32 * (Float::with_val(prec, 2u32) * pi).ln();
    s += Float::with_val(prec, ZETA_PRIME_NEG1);
    let x2inv = Float::with_val(prec, 1u32) / &x2;
    let mut pow = Float::with_val(prec, 1u32);
    for (k, &(num, den)) in BERNOULLI_EVEN.iter().enumerate().skip(1) {
        pow *= &x2inv;
        // B_{2k+2}
        let b =
            Float::with_val(prec, Integer::from(num)) / Float::with_val(prec, Integer::from(den));
        s += b / Float::with_val(prec, 4 * k as u32 * (k as u32 + 1)) * &pow;
    }
    s
}

/// BarnesG at a real argument: recurrence up to a large argument plus the
/// asymptotic series, keeping the (correct) sign through the Γ divisions.
fn barnes_g_real(z: &Float) -> Float {
    let prec = z.prec().max(DEFAULT_PRECISION);
    let z = Float::with_val(prec, z);
    let target = Float::with_val(prec, 30u32);
    let m = if z >= target {
        0u32
    } else {
        Float::with_val(prec, &target - &z)
            .ceil()
            .to_integer()
            .and_then(|i| i.to_u32())
            .unwrap_or(0)
    };
    let w = Float::with_val(prec, &z) + m;
    let mut g = barnes_g_ln_asym(&(Float::with_val(prec, &w - 1u32))).exp();
    for j in 0..m {
        g /= (Float::with_val(prec, &z) + j).gamma();
    }
    g
}

/// BarnesG[n] = `Π_{j=1}^{n-2} j!` for `n >= 3`, exact for small `n`.
fn barnes_g_integer(n: &Integer) -> Value {
    let k = n.to_i64().unwrap_or(0);
    if k <= 0 {
        return Value::Integer(Integer::from(0));
    }
    if k <= 2 {
        return Value::Integer(Integer::from(1));
    }
    if k > 2000 {
        return Value::Real(barnes_g_real(&Float::with_val(DEFAULT_PRECISION, n)));
    }
    let mut prod = Integer::from(1);
    let mut fact = Integer::from(1);
    for j in 1..=(k - 2) {
        fact *= j;
        prod *= &fact;
    }
    Value::Integer(prod)
}

pub fn builtin_barnes_g(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("BarnesG", args, 1)?;
    match &args[0] {
        Value::Integer(n) => Ok(barnes_g_integer(n)),
        Value::Real(r) => {
            if r.is_integer() && r <= &Float::with_val(r.prec().max(DEFAULT_PRECISION), 0u32) {
                return Ok(Value::Integer(Integer::from(0)));
            }
            Ok(Value::Real(barnes_g_real(r)))
        }
        _ => Ok(unevaluated("BarnesG", args)),
    }
}

// ── HarmonicNumber (value at Infinity) ──

/// HarmonicNumber[Infinity] = Infinity; otherwise delegates to `combinatorics`.
pub fn builtin_harmonic_number_exact(args: &[Value]) -> Result<Value, EvalError> {
    if let [Value::Symbol(s)] = args
        && s == "Infinity"
    {
        return Ok(Value::Symbol("Infinity".to_string()));
    }
    crate::builtins::combinatorics::builtin_harmonic_number(args)
}

// ── Registration helper ──

pub fn register_sfs(env: &crate::env::Env) {
    use crate::builtins::register_builtin;

    register_builtin(env, "Erf", builtin_erf);
    register_builtin(env, "Erfc", builtin_erfc);
    register_builtin(env, "Erfi", builtin_erfi);
    register_builtin(env, "BarnesG", builtin_barnes_g);
    register_builtin(env, "ErfInverse", builtin_erf_inverse);
    register_builtin(env, "InverseErf", builtin_inverse_erf);
    register_builtin(env, "InverseErfc", builtin_inverse_erfc);
    register_builtin(env, "Beta", builtin_beta);
    register_builtin(env, "BetaRegularized", builtin_beta_regularized);
    register_builtin(env, "Gamma", builtin_gamma_exact);
    register_builtin(env, "HarmonicNumber", builtin_harmonic_number_exact);
    register_builtin(env, "LogGamma", builtin_log_gamma);
    register_builtin(env, "GammaRegularized", builtin_gamma_regularized);
    register_builtin(env, "Zeta", builtin_zeta);
    register_builtin(env, "PolyLog", builtin_polylog);
    register_builtin(env, "Digamma", builtin_digamma);
    register_builtin(env, "PolyGamma", builtin_polygamma);
    register_builtin(env, "AiryAi", builtin_airy_ai);
    register_builtin(env, "AiryBi", builtin_airy_bi);
    register_builtin(env, "BesselJ", builtin_bessel_j);
    register_builtin(env, "BesselY", builtin_bessel_y);
    register_builtin(env, "BesselI", builtin_bessel_i);
    register_builtin(env, "BesselK", builtin_bessel_k);
    register_builtin(env, "LambertW", builtin_lambert_w);
    register_builtin(env, "DirichletEta", builtin_dirichlet_eta);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_erf_zero() {
        let result = builtin_erf(&[Value::Integer(Integer::from(0))]).unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == 0));
    }

    #[test]
    fn test_erf_small() {
        let result = builtin_erf(&[Value::Real(Float::with_val(DEFAULT_PRECISION, 0.5))]).unwrap();
        if let Value::Real(r) = result {
            let v = r.to_f64();
            assert!((v - 0.5205).abs() < 0.001);
        } else {
            panic!("expected Real");
        }
    }

    #[test]
    fn test_erf_symmetric() {
        let pos = builtin_erf(&[Value::Real(Float::with_val(DEFAULT_PRECISION, 1.0))]).unwrap();
        let neg = builtin_erf(&[Value::Real(Float::with_val(DEFAULT_PRECISION, -1.0))]).unwrap();
        if let (Value::Real(p), Value::Real(n)) = (pos, neg) {
            let pv = p.to_f64();
            let nv = n.to_f64();
            assert!((pv + nv).abs() < 1e-10);
        }
    }

    #[test]
    fn test_erf_symbolic() {
        let result = builtin_erf(&[Value::Symbol("x".to_string())]).unwrap();
        assert!(matches!(result, Value::Call { ref head, .. } if head == "Erf"));
    }

    #[test]
    fn test_erfc_zero() {
        let result = builtin_erfc(&[Value::Integer(Integer::from(0))]).unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == 1));
    }

    #[test]
    fn test_erfc_erf_complement() {
        let erf_val = builtin_erf(&[Value::Real(Float::with_val(DEFAULT_PRECISION, 0.5))]).unwrap();
        let erfc_val =
            builtin_erfc(&[Value::Real(Float::with_val(DEFAULT_PRECISION, 0.5))]).unwrap();
        if let (Value::Real(e), Value::Real(c)) = (erf_val, erfc_val) {
            let sum = e.to_f64() + c.to_f64();
            assert!((sum - 1.0).abs() < 1e-10);
        }
    }

    #[test]
    fn test_erf_inverse_zero() {
        let result = builtin_erf_inverse(&[Value::Integer(Integer::from(0))]).unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == 0));
    }

    #[test]
    fn test_erf_inverse_roundtrip() {
        let erf_val = builtin_erf(&[Value::Real(Float::with_val(DEFAULT_PRECISION, 0.5))]).unwrap();
        if let Value::Real(r) = erf_val {
            let inv = builtin_erf_inverse(&[Value::Real(r.clone())]).unwrap();
            if let Value::Real(result) = inv {
                assert!((result.to_f64() - 0.5).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn test_beta_positive_integers() {
        let result = builtin_beta(&[
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(3)),
        ])
        .unwrap();
        // B(2,3) = 1! * 2! / 4! = 2/24 = 1/12 (exact rational)
        if let Value::Rational(r) = result {
            assert_eq!(*r.numer(), Integer::from(1));
            assert_eq!(*r.denom(), Integer::from(12));
        } else {
            panic!("expected exact rational, got {result:?}");
        }
    }

    #[test]
    fn test_beta_symbolic() {
        let result = builtin_beta(&[
            Value::Symbol("a".to_string()),
            Value::Symbol("b".to_string()),
        ])
        .unwrap();
        assert!(matches!(result, Value::Call { ref head, .. } if head == "Beta"));
    }

    #[test]
    fn test_beta_regularized_zero() {
        let result = builtin_beta_regularized(&[
            Value::Integer(Integer::from(0)),
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
        ])
        .unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == 0));
    }

    #[test]
    fn test_beta_regularized_one() {
        let result = builtin_beta_regularized(&[
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
        ])
        .unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == 1));
    }

    #[test]
    fn test_zeta_even() {
        // Zeta[2] = Pi^2/6 (exact symbolic form).
        let result = builtin_zeta(&[Value::Integer(Integer::from(2))]).unwrap();
        assert!(
            matches!(result, Value::Call { ref head, .. } if head == "Times"),
            "expected symbolic Pi^2/6, got {result:?}"
        );
    }

    #[test]
    fn test_zeta_zero() {
        // Zeta[0] = -1/2 (exact).
        let result = builtin_zeta(&[Value::Integer(Integer::from(0))]).unwrap();
        if let Value::Rational(r) = result {
            assert_eq!(*r.numer(), Integer::from(-1));
            assert_eq!(*r.denom(), Integer::from(2));
        } else {
            panic!("expected exact rational -1/2, got {result:?}");
        }
    }

    #[test]
    fn test_zeta_four() {
        // Zeta[4] = Pi^4/90 (exact symbolic form).
        let result = builtin_zeta(&[Value::Integer(Integer::from(4))]).unwrap();
        assert!(
            matches!(result, Value::Call { ref head, .. } if head == "Times"),
            "expected symbolic Pi^4/90, got {result:?}"
        );
    }

    #[test]
    fn test_zeta_odd() {
        let result = builtin_zeta(&[Value::Integer(Integer::from(3))]).unwrap();
        if let Value::Real(r) = result {
            // ζ(3) ≈ 1.2020569 (Apéry's constant)
            assert!((r.to_f64() - 1.2020569).abs() < 0.001);
        }
    }

    #[test]
    fn test_zeta_one_unevaluated() {
        let result = builtin_zeta(&[Value::Integer(Integer::from(1))]).unwrap();
        assert!(matches!(result, Value::Call { ref head, .. } if head == "Zeta"));
    }

    #[test]
    fn test_polylog_li0() {
        let result = builtin_polylog(&[
            Value::Integer(Integer::from(0)),
            Value::Real(Float::with_val(DEFAULT_PRECISION, 0.5)),
        ])
        .unwrap();
        if let Value::Real(r) = result {
            // Li_0(0.5) = 0.5 / 0.5 = 1.0
            assert!((r.to_f64() - 1.0).abs() < 1e-10);
        }
    }

    #[test]
    fn test_polylog_li1() {
        let result = builtin_polylog(&[
            Value::Integer(Integer::from(1)),
            Value::Real(Float::with_val(DEFAULT_PRECISION, 0.5)),
        ])
        .unwrap();
        if let Value::Real(r) = result {
            // Li_1(0.5) = -ln(0.5) = ln(2)
            assert!((r.to_f64() - std::f64::consts::LN_2).abs() < 1e-4);
        }
    }

    #[test]
    fn test_digamma_one() {
        let result = builtin_digamma(&[Value::Integer(Integer::from(1))]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() + EULER_GAMMA_F64).abs() < 1e-10);
        }
    }

    #[test]
    fn test_digamma_five() {
        // Regression: the recurrence used the wrong sign, making ψ small-int
        // wildly wrong (ψ(5) ≈ 1.5061176684).
        let result = builtin_digamma(&[Value::Integer(Integer::from(5))]).unwrap();
        if let Value::Real(r) = result {
            assert!(
                (r.to_f64() - 1.5061176684).abs() < 1e-6,
                "got {}",
                r.to_f64()
            );
        } else {
            panic!("expected Real");
        }
    }

    #[test]
    fn test_digamma_large() {
        let result = builtin_digamma(&[Value::Integer(Integer::from(100))]).unwrap();
        if let Value::Real(r) = result {
            // ψ(100) ≈ ln(100) - 1/200 ≈ 4.60517
            assert!((r.to_f64() - 4.6).abs() < 0.05);
        }
    }

    #[test]
    fn test_polygamma_zero_is_digamma() {
        let dg = builtin_digamma(&[Value::Integer(Integer::from(5))]).unwrap();
        let pg = builtin_polygamma(&[
            Value::Integer(Integer::from(0)),
            Value::Integer(Integer::from(5)),
        ])
        .unwrap();
        if let (Value::Real(d), Value::Real(p)) = (dg, pg) {
            assert!((d.to_f64() - p.to_f64()).abs() < 1e-10);
        }
    }

    #[test]
    fn test_airy_ai_zero() {
        // Ai(0) = 1/(3^(2/3) Gamma[2/3]) (exact symbolic form).
        let result = builtin_airy_ai(&[Value::Integer(Integer::from(0))]).unwrap();
        assert!(
            matches!(result, Value::Call { ref head, .. } if head == "Times"),
            "expected symbolic Ai(0), got {result:?}"
        );
    }

    #[test]
    fn test_airy_ai_small_positive() {
        // Regression: the power series used wrong powers (z^6/z^9 vs z^4/z^7),
        // so Ai(0.4) was substantially off. Ai(0.4) ≈ 0.254556.
        let result =
            builtin_airy_ai(&[Value::Real(Float::with_val(DEFAULT_PRECISION, 0.4))]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 0.254556).abs() < 1e-4, "got {}", r.to_f64());
        } else {
            panic!("expected Real");
        }
    }

    #[test]
    fn test_bessel_j_0_0() {
        let result = builtin_bessel_j(&[
            Value::Integer(Integer::from(0)),
            Value::Integer(Integer::from(0)),
        ])
        .unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == 1));
    }

    #[test]
    fn test_bessel_j_1_0() {
        let result = builtin_bessel_j(&[
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(0)),
        ])
        .unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == 0));
    }

    #[test]
    fn test_bessel_j_one_one() {
        // Regression: the series applied (-1)^(k+n) instead of (-1)^k, flipping
        // the sign of J_n for odd n. J_1(1) ≈ 0.4400505857.
        let result = builtin_bessel_j(&[
            Value::Integer(Integer::from(1)),
            Value::Real(Float::with_val(DEFAULT_PRECISION, 1.0)),
        ])
        .unwrap();
        if let Value::Real(r) = result {
            assert!(
                (r.to_f64() - 0.4400505857).abs() < 1e-6,
                "got {}",
                r.to_f64()
            );
        } else {
            panic!("expected Real");
        }
    }

    #[test]
    fn test_bessel_j_positive() {
        let result = builtin_bessel_j(&[
            Value::Integer(Integer::from(0)),
            Value::Real(Float::with_val(DEFAULT_PRECISION, 1.0)),
        ])
        .unwrap();
        if let Value::Real(r) = result {
            // J_0(1) ≈ 0.7651977
            assert!((r.to_f64() - 0.765198).abs() < 1e-4);
        }
    }

    #[test]
    fn test_bessel_i_0_0() {
        let result = builtin_bessel_i(&[
            Value::Integer(Integer::from(0)),
            Value::Integer(Integer::from(0)),
        ])
        .unwrap();
        assert!(matches!(result, Value::Integer(n) if n == 1));
    }

    #[test]
    fn test_bessel_i_1_0() {
        let result = builtin_bessel_i(&[
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(0)),
        ])
        .unwrap();
        assert!(matches!(result, Value::Integer(n) if n == 0));
    }

    #[test]
    fn test_gamma_half_exact() {
        // Gamma[1/2] = Sqrt[Pi].
        let result = builtin_gamma_exact(&[rational(1, 2)]).unwrap();
        assert!(
            matches!(result, Value::Call { ref head, .. } if head == "Sqrt"),
            "expected Sqrt[Pi], got {result:?}"
        );
    }

    #[test]
    fn test_gamma_integer_delegates() {
        // Gamma[5] = 24 still works through the numeric builtin.
        let result = builtin_gamma_exact(&[Value::Integer(Integer::from(5))]).unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == 24));
    }

    #[test]
    fn test_polylog_two_one_exact() {
        // PolyLog[2, 1] = Pi^2/6.
        let result = builtin_polylog(&[
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(1)),
        ])
        .unwrap();
        assert!(
            matches!(result, Value::Call { ref head, .. } if head == "Times"),
            "expected symbolic Pi^2/6, got {result:?}"
        );
    }

    #[test]
    fn test_erf_real_zero_exact() {
        // Erf[0.] = 0 exactly (no 1e-9 approximation residue).
        let result = builtin_erf(&[Value::Real(Float::with_val(DEFAULT_PRECISION, 0.0))]).unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == 0));
    }

    #[test]
    fn test_harmonic_number_infinity() {
        let result = builtin_harmonic_number_exact(&[Value::Symbol("Infinity".to_string())]);
        assert!(
            matches!(result.unwrap(), Value::Symbol(ref s) if s == "Infinity"),
            "expected Infinity"
        );
    }

    #[test]
    fn test_bessel_j_half_symbolic() {
        // J_{1/2}(x) = Sqrt[2/(Pi x)] Sin[x].
        let result = builtin_bessel_j(&[rational(1, 2), Value::Symbol("x".to_string())]).unwrap();
        assert!(
            matches!(result, Value::Call { ref head, .. } if head == "Times"),
            "expected symbolic half-order Bessel, got {result:?}"
        );
    }

    #[test]
    fn test_lambert_w_zero() {
        let result = builtin_lambert_w(&[Value::Integer(Integer::from(0))]).unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == 0));
    }

    #[test]
    fn test_lambert_w_e() {
        let result = builtin_lambert_w(&[Value::Real(Float::with_val(
            DEFAULT_PRECISION,
            std::f64::consts::E,
        ))])
        .unwrap();
        if let Value::Real(r) = result {
            // W(e) = 1
            assert!((r.to_f64() - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_lambert_w_neg_inv_e() {
        let result = builtin_lambert_w(&[Value::Real(Float::with_val(
            DEFAULT_PRECISION,
            -1.0 / std::f64::consts::E,
        ))])
        .unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() + 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_dirichlet_eta_one() {
        let result = builtin_dirichlet_eta(&[Value::Integer(Integer::from(1))]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - std::f64::consts::LN_2).abs() < 1e-10);
        }
    }

    #[test]
    fn test_dirichlet_eta_two() {
        let result = builtin_dirichlet_eta(&[Value::Integer(Integer::from(2))]).unwrap();
        if let Value::Real(r) = result {
            // η(2) = (1 - 2^(-1)) * ζ(2) = π²/12 ≈ 0.822467
            let expected = std::f64::consts::PI * std::f64::consts::PI / 12.0;
            assert!((r.to_f64() - expected).abs() < 0.001);
        }
    }

    #[test]
    fn test_all_functions_arg_count() {
        assert_eq!(
            builtin_erf(&[]).unwrap_err().to_string(),
            "Erf requires exactly 1 argument"
        );
        assert_eq!(
            builtin_bessel_j(&[]).unwrap_err().to_string(),
            "BesselJ requires exactly 2 arguments: BesselJ[n, z]"
        );
    }

    // ── Hardening: high-precision accuracy (target 1e-13) ──

    fn i(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn r(x: f64) -> Value {
        Value::Real(Float::with_val(DEFAULT_PRECISION, x))
    }
    /// Compare numerically with a relative tolerance (absolute near zero).
    fn approx(v: Value, expected: f64, tol: f64) {
        let got = match &v {
            Value::Real(x) => x.to_f64(),
            Value::Integer(n) => n.to_f64(),
            Value::Rational(x) => x.to_f64(),
            other => panic!("expected number, got {other:?}"),
        };
        if got.is_nan() {
            panic!("NaN for expected {expected}");
        }
        let scale = expected.abs().max(1.0);
        assert!(
            (got - expected).abs() < tol * scale,
            "got {got}, expected {expected}"
        );
    }
    fn is_unevaluated(v: &Value, head: &str) -> bool {
        matches!(v, Value::Call { head: h, .. } if h == head)
    }

    #[test]
    fn test_erf_erfc_high_precision() {
        approx(
            builtin_erf(&[r(1.0)]).unwrap(),
            0.842_700_792_949_714_9,
            1e-15,
        );
        approx(
            builtin_erf(&[r(-2.5)]).unwrap(),
            -0.999_593_047_982_555,
            1e-14,
        );
        approx(
            builtin_erfc(&[r(5.0)]).unwrap(),
            1.537_459_794_428_035e-12,
            1e-13,
        );
        approx(builtin_erfc(&[r(-5.0)]).unwrap(), 1.9999999999984625, 1e-14);
        // Boundaries stay exact / non-NaN.
        assert!(matches!(builtin_erf(&[i(0)]).unwrap(), Value::Integer(ref n) if *n == 0));
        approx(builtin_erf(&[r(1000.0)]).unwrap(), 1.0, 1e-15);
    }

    #[test]
    fn test_erfi_series_and_asymptotic() {
        approx(
            builtin_erfi(&[r(2.0)]).unwrap(),
            18.564_802_414_575_553,
            1e-13,
        );
        approx(
            builtin_erfi(&[r(-3.0)]).unwrap(),
            -1629.9946226015657,
            1e-13,
        );
        // Large argument → asymptotic branch.
        approx(
            builtin_erfi(&[r(10.0)]).unwrap(),
            1.524_307_422_708_669_6e42,
            1e-13,
        );
        assert!(matches!(builtin_erfi(&[i(0)]).unwrap(), Value::Integer(ref n) if *n == 0));
        assert!(is_unevaluated(
            &builtin_erfi(&[Value::Symbol("x".into())]).unwrap(),
            "Erfi"
        ));
    }

    #[test]
    fn test_inverse_erf_precision() {
        approx(
            builtin_inverse_erf(&[r(0.5)]).unwrap(),
            0.476_936_276_204_469_9,
            1e-13,
        );
        approx(
            builtin_erf_inverse(&[r(0.9)]).unwrap(),
            1.163_087_153_676_674,
            1e-12,
        );
        approx(
            builtin_inverse_erfc(&[r(0.5)]).unwrap(),
            0.476_936_276_204_469_9,
            1e-13,
        );
        // Round-trips.
        if let Value::Real(y) = builtin_inverse_erf(&[r(0.3)]).unwrap() {
            approx(builtin_erf(&[Value::Real(y)]).unwrap(), 0.3, 1e-13);
        }
    }

    #[test]
    fn test_zeta_exact_and_numeric() {
        // Exact trivial zeros / Bernoulli closed form at non-positive integers.
        assert!(matches!(builtin_zeta(&[i(-2)]).unwrap(), Value::Integer(ref n) if *n == 0));
        if let Value::Rational(v) = builtin_zeta(&[i(-1)]).unwrap() {
            assert_eq!(*v.numer(), Integer::from(-1));
            assert_eq!(*v.denom(), Integer::from(12));
        }
        if let Value::Rational(v) = builtin_zeta(&[i(-3)]).unwrap() {
            assert_eq!(*v.numer(), Integer::from(1));
            assert_eq!(*v.denom(), Integer::from(120));
        }
        // Exact even positive closed form.
        assert!(matches!(
            builtin_zeta(&[i(6)]).unwrap(),
            Value::Call { ref head, .. } if head == "Times"
        ));
        // Numeric Euler–Maclaurin, including 0 < s < 1 and s > 1.
        approx(
            builtin_zeta(&[r(1.5)]).unwrap(),
            2.612_375_348_685_488,
            1e-13,
        );
        approx(builtin_zeta(&[r(0.5)]).unwrap(), -1.4603545088095868, 1e-13);
        approx(
            builtin_zeta(&[r(-0.5)]).unwrap(),
            -0.20788622497735456,
            1e-13,
        );
        // Pole stays unevaluated.
        assert!(is_unevaluated(&builtin_zeta(&[i(1)]).unwrap(), "Zeta"));
    }

    #[test]
    fn test_digamma_polygamma_precision() {
        approx(
            builtin_digamma(&[r(0.5)]).unwrap(),
            -1.963_510_026_021_423_5,
            1e-14,
        );
        approx(
            builtin_polygamma(&[i(1), i(1)]).unwrap(),
            std::f64::consts::PI * std::f64::consts::PI / 6.0,
            1e-14,
        );
        approx(
            builtin_polygamma(&[i(2), r(0.5)]).unwrap(),
            -16.828_796_644_234_32,
            1e-13,
        );
        // n = 0 routes to digamma; negative n stays unevaluated.
        if let (Value::Real(d), Value::Real(p)) = (
            builtin_digamma(&[i(5)]).unwrap(),
            builtin_polygamma(&[i(0), i(5)]).unwrap(),
        ) {
            assert!((d.to_f64() - p.to_f64()).abs() < 1e-12);
        } else {
            panic!("expected reals");
        }
        assert!(is_unevaluated(
            &builtin_polygamma(&[i(-1), i(2)]).unwrap(),
            "PolyGamma"
        ));
    }

    #[test]
    fn test_dirichlet_eta_exact_and_numeric() {
        approx(builtin_dirichlet_eta(&[i(-1)]).unwrap(), 0.25, 1e-14);
        assert!(matches!(
            builtin_dirichlet_eta(&[i(-2)]).unwrap(),
            Value::Integer(ref n) if *n == 0
        ));
        approx(
            builtin_dirichlet_eta(&[i(1)]).unwrap(),
            std::f64::consts::LN_2,
            1e-14,
        );
        approx(
            builtin_dirichlet_eta(&[r(3.0)]).unwrap(),
            0.901_542_677_369_695_7,
            1e-13,
        );
    }

    #[test]
    fn test_barnes_g() {
        assert!(matches!(builtin_barnes_g(&[i(5)]).unwrap(), Value::Integer(ref n) if *n == 12));
        assert!(matches!(builtin_barnes_g(&[i(6)]).unwrap(), Value::Integer(ref n) if *n == 288));
        assert!(matches!(builtin_barnes_g(&[i(0)]).unwrap(), Value::Integer(ref n) if *n == 0));
        approx(
            builtin_barnes_g(&[r(0.5)]).unwrap(),
            0.603_244_281_209_446_2,
            1e-13,
        );
        approx(
            builtin_barnes_g(&[r(7.5)]).unwrap(),
            733_746.383_952_146_4,
            1e-13,
        );
        assert!(is_unevaluated(
            &builtin_barnes_g(&[Value::Symbol("z".into())]).unwrap(),
            "BarnesG"
        ));
    }

    #[test]
    fn test_gamma_beta_exact_breadth() {
        // Gamma[3/2] = Sqrt[Pi]/2, Gamma[-1/2] = -2 Sqrt[Pi].
        assert!(matches!(
            builtin_gamma_exact(&[rational(3, 2)]).unwrap(),
            Value::Call { ref head, .. } if head == "Times"
        ));
        assert!(matches!(
            builtin_gamma_exact(&[rational(-1, 2)]).unwrap(),
            Value::Call { ref head, .. } if head == "Times"
        ));
        assert!(
            matches!(builtin_beta(&[rational(1, 2), rational(1, 2)]).unwrap(), Value::Symbol(ref s) if s == "Pi")
        );
        approx(
            builtin_beta(&[rational(1, 2), i(2)]).unwrap(),
            4.0 / 3.0,
            1e-14,
        );
        approx(builtin_beta(&[r(2.5), r(2.0)]).unwrap(), 4.0 / 35.0, 1e-14);
    }

    #[test]
    fn test_incomplete_regularized() {
        approx(
            builtin_gamma_regularized(&[i(3), r(2.0)]).unwrap(),
            0.323_323_583_816_936_54,
            1e-13,
        );
        approx(
            builtin_beta_regularized(&[r(0.5), i(2), i(3)]).unwrap(),
            0.6875,
            1e-13,
        );
        // Endpoints exact.
        assert!(matches!(
            builtin_beta_regularized(&[i(0), i(2), i(3)]).unwrap(),
            Value::Integer(ref n) if *n == 0
        ));
        assert!(matches!(
            builtin_beta_regularized(&[i(1), i(2), i(3)]).unwrap(),
            Value::Integer(ref n) if *n == 1
        ));
    }

    #[test]
    fn test_fresnel_dawson_hurwitz() {
        approx(
            crate::builtins::specialfuncmore::builtin_fresnel_s(&[r(3.0)]).unwrap(),
            0.496_312_998_967_375,
            1e-13,
        );
        approx(
            crate::builtins::specialfuncmore::builtin_fresnel_c(&[r(5.0)]).unwrap(),
            0.563_631_188_704_012_2,
            1e-13,
        );
        approx(
            crate::builtins::specialfuncmore::builtin_fresnel_s(&[r(-5.0)]).unwrap(),
            -0.499_191_381_917_116_87,
            1e-13,
        );
        approx(
            crate::builtins::specialfuncmore::builtin_dawson_f(&[r(5.0)]).unwrap(),
            0.102_134_074_424_276_84,
            1e-13,
        );
        approx(
            crate::builtins::specialfuncmore::builtin_hurwitz_zeta(&[i(2), rational(1, 3)])
                .unwrap(),
            10.095_597_125_427_094,
            1e-13,
        );
    }
}
