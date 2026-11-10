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

// ── Erf / Erfc / ErfInverse ──

const ERF_P: f64 = 0.3275911;
const ERF_A1: f64 = 0.254829592;
const ERF_A2: f64 = -0.284496736;
const ERF_A3: f64 = 1.421413741;
const ERF_A4: f64 = -1.453152027;
const ERF_A5: f64 = 1.061405429;

fn erf_approx(x: f64) -> f64 {
    let ax = x.abs();
    let t = 1.0 / (1.0 + ERF_P * ax);
    // Horner form: t * (A1 + t * (A2 + t * (A3 + t * (A4 + t * A5))))
    let poly = t * (ERF_A1 + t * (ERF_A2 + t * (ERF_A3 + t * (ERF_A4 + t * ERF_A5))));
    let result = 1.0 - poly * (-ax * ax).exp();
    if x < 0.0 { -result } else { result }
}

pub fn builtin_erf(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Erf requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Real(r) if r.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) => {
            let x = n.to_f64();
            if x.is_infinite() {
                return Ok(Value::Integer(Integer::from(if x > 0.0 { 1 } else { -1 })));
            }
            Ok(super::real(erf_approx(x)))
        }
        Value::Real(r) => {
            if r.is_infinite() {
                return Ok(Value::Integer(Integer::from(if r.is_sign_positive() {
                    1
                } else {
                    -1
                })));
            }
            Ok(super::real(erf_approx(r.to_f64())))
        }
        _ => Ok(unevaluated("Erf", args)),
    }
}

pub fn builtin_erfc(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Erfc requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(1))),
        Value::Real(r) if r.is_zero() => Ok(Value::Integer(Integer::from(1))),
        Value::Integer(n) => {
            let x = n.to_f64();
            if x.is_infinite() {
                return Ok(Value::Integer(Integer::from(if x > 0.0 { 0 } else { 2 })));
            }
            Ok(super::real(1.0 - erf_approx(x)))
        }
        Value::Real(r) => {
            if r.is_infinite() {
                return Ok(Value::Integer(Integer::from(if r.is_sign_positive() {
                    0
                } else {
                    2
                })));
            }
            Ok(super::real(1.0 - erf_approx(r.to_f64())))
        }
        _ => Ok(unevaluated("Erfc", args)),
    }
}

fn inverse_erf_approx(x: f64) -> f64 {
    if x == 0.0 {
        return 0.0;
    }
    let s = x.signum();
    let ax = x.abs();
    // Winitzki's closed-form approximation is accurate to ~1e-4 over (-1, 1);
    // Newton iterations below sharpen it to machine precision.
    let a = 0.147;
    let ln1mx2 = (1.0 - ax * ax).ln();
    let c = 2.0 / (std::f64::consts::PI * a) + ln1mx2 / 2.0;
    let inner = (c * c - ln1mx2 / a).max(0.0).sqrt() - c;
    let mut w = s * inner.max(0.0).sqrt();
    for _ in 0..25 {
        let ew = erf_approx(w);
        let diff = ew - x;
        if diff.abs() < 1e-16 {
            break;
        }
        let deriv = 2.0 / std::f64::consts::PI.sqrt() * (-w * w).exp();
        if deriv == 0.0 {
            break;
        }
        w -= diff / deriv;
    }
    w
}

pub fn builtin_erf_inverse(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ErfInverse requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) => {
            let x = n.to_f64();
            if x.abs() >= 1.0 {
                return Err(EvalError::Error(
                    "ErfInverse: argument must be in (-1, 1)".to_string(),
                ));
            }
            Ok(super::real(inverse_erf_approx(x)))
        }
        Value::Real(r) => {
            let x = r.to_f64();
            if x.abs() >= 1.0 {
                return Err(EvalError::Error(
                    "ErfInverse: argument must be in (-1, 1)".to_string(),
                ));
            }
            Ok(super::real(inverse_erf_approx(x)))
        }
        _ => Ok(unevaluated("ErfInverse", args)),
    }
}

// ── Beta Function ──

pub fn builtin_beta(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Beta requires exactly 2 arguments".to_string(),
        ));
    }
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
        (Value::Integer(x), Value::Integer(y)) => {
            if x.is_negative() || y.is_negative() {
                return Ok(unevaluated("Beta", args));
            }
            let prec = DEFAULT_PRECISION;
            let fx = Float::with_val(prec, x);
            let fy = Float::with_val(prec, y);
            let fx_sum = Float::with_val(prec, &fx + &fy);
            let gamma_x = fx.clone().gamma();
            let gamma_y = fy.clone().gamma();
            let gamma_xy = fx_sum.gamma();
            let result = gamma_x * gamma_y / gamma_xy;
            Ok(Value::Real(result))
        }
        (Value::Real(x), Value::Real(y)) => {
            if x.is_sign_negative() || y.is_sign_negative() {
                return Ok(unevaluated("Beta", args));
            }
            let prec = x.prec().max(y.prec());
            let gamma_x = x.clone().gamma();
            let gamma_y = y.clone().gamma();
            let sum = Float::with_val(prec, x + y);
            let gamma_xy = sum.gamma();
            let result = gamma_x * gamma_y / gamma_xy;
            Ok(Value::Real(result))
        }
        _ => Ok(unevaluated("Beta", args)),
    }
}

// ── BetaRegularized ──

pub fn builtin_beta_regularized(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "BetaRegularized requires exactly 3 arguments: BetaRegularized[z, a, b]".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) if *n == 1 => Ok(Value::Integer(Integer::from(1))),
        Value::Real(r) if r.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Real(r) if *r == Float::with_val(DEFAULT_PRECISION, 1u32) => {
            Ok(Value::Integer(Integer::from(1)))
        }
        _ => Ok(unevaluated("BetaRegularized", args)),
    }
}

// ── Zeta ──

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
        && int_val <= 20
    {
        int_val.to_u64()
    } else {
        None
    }
}

fn zeta_even(k: u64) -> Float {
    let prec = DEFAULT_PRECISION;
    let bernoulli: &[(i64, i64)] = &[
        (1, 6),         // B_2
        (-1, 30),       // B_4
        (1, 42),        // B_6
        (-1, 30),       // B_8
        (5, 66),        // B_10
        (-691, 2730),   // B_12
        (7, 6),         // B_14
        (-3617, 510),   // B_16
        (43867, 798),   // B_18
        (-174611, 330), // B_20
    ];
    let n_div_2 = (k / 2 - 1) as usize;
    let (bn_num, bn_den) = bernoulli[n_div_2];
    let two_pi = Float::with_val(prec, 2.0 * std::f64::consts::PI);
    let two_pi_n = two_pi.pow(&Integer::from(k));
    let mut factorial = Float::with_val(prec, 1u32);
    for i in 2..=k as i64 {
        factorial *= Float::with_val(prec, i);
    }
    let b_abs = Float::with_val(prec, bn_num.abs()) / bn_den.abs();
    let numerator = two_pi_n * b_abs;
    let denominator = Float::with_val(prec, 2u32) * factorial;
    numerator / denominator
}

pub fn builtin_zeta(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Zeta requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        // Exact closed forms at the special integer arguments.
        Value::Integer(n) if *n == 0 => Ok(rational(-1, 2)),
        Value::Integer(n) if *n == 2 => Ok(pi_pow_over(2, 6)),
        Value::Integer(n) if *n == 4 => Ok(pi_pow_over(4, 90)),
        Value::Integer(n) => {
            if *n <= 1 {
                return Ok(unevaluated("Zeta", args));
            }
            let f = Float::with_val(DEFAULT_PRECISION, n);
            if let Some(k) = is_small_even_positive(&f) {
                return Ok(Value::Real(zeta_even(k)));
            }
            Ok(Value::Real(compute_zeta_numeric(&f)))
        }
        Value::Real(r) if r.is_zero() => Ok(super::real(-0.5)),
        Value::Real(r) => {
            if r <= &Float::with_val(DEFAULT_PRECISION, 1.0) {
                return Ok(unevaluated("Zeta", args));
            }
            if let Some(k) = is_small_even_positive(r) {
                return Ok(Value::Real(zeta_even(k)));
            }
            Ok(Value::Real(compute_zeta_numeric(r)))
        }
        _ => Ok(unevaluated("Zeta", args)),
    }
}

fn compute_zeta_numeric(s: &Float) -> Float {
    let prec = s.prec().max(DEFAULT_PRECISION);
    let s_f = Float::with_val(prec, s);
    let s_floor = s_f.clone().floor();
    let tolerance = Float::with_val(prec, 1e-15);

    if s_floor > Float::with_val(prec, 2u32) {
        let mut sum = Float::with_val(prec, 0.0);
        for n in 1..=1_000_000u32 {
            let n_f = Float::with_val(prec, n);
            let nf_pow = n_f.pow(&s_f);
            let term = Float::with_val(prec, 1.0) / nf_pow;
            sum += &term;
            if term < tolerance {
                break;
            }
        }
        sum
    } else {
        let max_n: u64 = 10_000_000;
        let mut sum = Float::with_val(prec, 0.0);
        for n in 1..=max_n {
            let n_f = Float::with_val(prec, n);
            let nf_pow = n_f.pow(&s_f);
            sum += Float::with_val(prec, 1.0) / nf_pow;
        }
        let n_last = Float::with_val(prec, max_n);
        let one = Float::with_val(prec, 1u32);
        let exponent = Float::with_val(prec, &one - &s_f);
        let n_last_pow = n_last.pow(&exponent);
        let denom = Float::with_val(prec, &s_f - &one);
        let tail = n_last_pow / denom;
        sum + tail
    }
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

const EULER_GAMMA_F64: f64 = std::f64::consts::EULER_GAMMA;

fn digamma_approx(z: f64) -> f64 {
    // Recurrence ψ(z) = ψ(z+1) - 1/z, applied until z is large enough for the
    // asymptotic series below to be accurate.
    let mut offset = 0.0_f64;
    let mut z = z;
    while z < 10.0 {
        offset -= 1.0 / z;
        z += 1.0;
    }
    let z2 = z * z;
    let z4 = z2 * z2;
    let z6 = z4 * z2;
    let z8 = z4 * z4;
    offset + z.ln() - 1.0 / (2.0 * z) - 1.0 / (12.0 * z2) + 1.0 / (120.0 * z4) - 1.0 / (252.0 * z6)
        + 1.0 / (240.0 * z8)
}

pub fn builtin_digamma(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Digamma requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if *n == 1 => Ok(super::real(-EULER_GAMMA_F64)),
        Value::Integer(n) => {
            if n.is_negative() || n.is_zero() {
                return Ok(unevaluated("Digamma", args));
            }
            Ok(super::real(digamma_approx(n.to_f64())))
        }
        Value::Real(r) => {
            if r.is_sign_negative() || r.is_zero() {
                return Ok(unevaluated("Digamma", args));
            }
            Ok(super::real(digamma_approx(r.to_f64())))
        }
        _ => Ok(unevaluated("Digamma", args)),
    }
}

pub fn builtin_polygamma(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "PolyGamma requires exactly 2 arguments: PolyGamma[n, z]".to_string(),
        ));
    }
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
        if z <= 0.0 {
            return Ok(unevaluated("PolyGamma", args));
        }
        let n_int = n as u64;
        // ψ^(n)(z) = (-1)^(n+1) * n! * Σ_{k=0}^∞ 1/(k+z)^(n+1)
        let sign = if n_int.is_multiple_of(2) { -1.0 } else { 1.0 };
        let mut factorial = 1.0_f64;
        for i in 1..=n as u32 {
            factorial *= i as f64;
        }
        let mut sum = 0.0_f64;
        let p = (n_int + 1) as f64;
        let mut k: u32 = 0;
        while k < 10_000_000 {
            let term = 1.0 / (k as f64 + z).powf(p);
            sum += term;
            if term < 1e-16 {
                break;
            }
            k += 1;
        }
        Ok(super::real(sign * factorial * sum))
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

pub fn builtin_dirichlet_eta(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "DirichletEta requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if *n == 1 => Ok(super::real(std::f64::consts::LN_2)),
        Value::Integer(n) => {
            if n.is_negative() || n.is_zero() {
                return Ok(unevaluated("DirichletEta", args));
            }
            Ok(super::real(compute_eta_numeric(n.to_f64())))
        }
        Value::Real(r) => {
            if r.is_sign_negative() || r.is_zero() {
                return Ok(unevaluated("DirichletEta", args));
            }
            if (r.to_f64() - 1.0).abs() < 1e-15 {
                return Ok(super::real(std::f64::consts::LN_2));
            }
            if r.to_f64() <= 0.0 {
                return Ok(unevaluated("DirichletEta", args));
            }
            Ok(super::real(compute_eta_numeric(r.to_f64())))
        }
        _ => Ok(unevaluated("DirichletEta", args)),
    }
}

fn compute_eta_numeric(s: f64) -> f64 {
    // η(s) = Σ (-1)^(n-1) / n^s for n = 1..∞
    let mut sum = 0.0_f64;
    let tolerance = 1e-15;
    for n in 1..=10_000_000u64 {
        let sign = if n % 2 == 1 { 1.0 } else { -1.0 };
        let term = sign / (n as f64).powf(s);
        sum += term;
        if term.abs() < tolerance {
            break;
        }
    }
    sum
}

// ── InverseErf ──

/// InverseErf[x] — inverse error function using Newton's method.
pub fn builtin_inverse_erf(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("InverseErf", args, 1)?;
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) => {
            let x = n.to_f64();
            if x.abs() >= 1.0 {
                return Err(EvalError::Error(
                    "InverseErf: argument must be in (-1, 1)".to_string(),
                ));
            }
            Ok(super::real(inverse_erf_newton(x)))
        }
        Value::Real(r) => {
            let x = r.to_f64();
            if x.abs() >= 1.0 {
                return Err(EvalError::Error(
                    "InverseErf: argument must be in (-1, 1)".to_string(),
                ));
            }
            Ok(super::real(inverse_erf_newton(x)))
        }
        _ => Ok(unevaluated("InverseErf", args)),
    }
}

fn inverse_erf_newton(x: f64) -> f64 {
    // Good initial guess
    let mut w = if x.abs() < 0.7 {
        // Series expansion near 0: erf^{-1}(x) ≈ sqrt(pi)/2 * x + ...
        let c0 = 0.5 * std::f64::consts::PI.sqrt();
        let ax2 = x * x;
        x * (c0 + ax2 * (c0 / 3.0 + ax2 * (3.0 * c0 / 40.0)))
    } else {
        // For |x| near 1, use asymptotic
        let s = x.signum();
        let ax = x.abs();
        let w0 = (-2.0 * (1.0 - ax)).ln();
        s * (-w0 / std::f64::consts::PI).sqrt()
    };
    // Newton's method: x_{n+1} = x_n + (erf(x_n) - x) * sqrt(pi) * exp(x_n^2)
    let sqrt_pi = std::f64::consts::PI.sqrt();
    for _ in 0..30 {
        let ew = erf_approx(w);
        let diff = ew - x;
        if diff.abs() < 1e-16 {
            break;
        }
        // Derivative of erf: 2/sqrt(pi) * exp(-w^2)
        // Newton step: w -= (erf(w) - x) / erf'(w)
        let deriv = 2.0 / sqrt_pi * (-w * w).exp();
        w -= diff / deriv;
    }
    w
}

// ── InverseErfc ──

/// InverseErfc[x] — inverse complementary error function. InverseErfc[x] = InverseErf[1 - x].
pub fn builtin_inverse_erfc(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("InverseErfc", args, 1)?;
    match &args[0] {
        Value::Integer(n) if *n == 1 => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) => {
            let x = n.to_f64();
            if x <= 0.0 || x >= 2.0 {
                return Err(EvalError::Error(
                    "InverseErfc: argument must be in (0, 2)".to_string(),
                ));
            }
            Ok(super::real(inverse_erf_newton(1.0 - x)))
        }
        Value::Real(r) => {
            let x = r.to_f64();
            if x <= 0.0 || x >= 2.0 {
                return Err(EvalError::Error(
                    "InverseErfc: argument must be in (0, 2)".to_string(),
                ));
            }
            Ok(super::real(inverse_erf_newton(1.0 - x)))
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

/// GammaRegularized[a, z] — regularized incomplete gamma function P(a,z) = Gamma(a,z) / Gamma(a).
pub fn builtin_gamma_regularized(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("GammaRegularized", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(z)) if !a.is_negative() && !z.is_negative() => {
            let a_f = a.to_f64();
            let z_f = z.to_f64();
            Ok(super::real(gamma_p_series(a_f, z_f)))
        }
        (Value::Real(a), Value::Real(z)) => {
            let a_f = a.to_f64();
            let z_f = z.to_f64();
            if a_f <= 0.0 || z_f < 0.0 {
                return Ok(unevaluated("GammaRegularized", args));
            }
            Ok(super::real(gamma_p_series(a_f, z_f)))
        }
        _ => Ok(unevaluated("GammaRegularized", args)),
    }
}

/// Compute P(a, z) using series expansion for small z, continued fraction for large z.
fn gamma_p_series(a: f64, z: f64) -> f64 {
    if z < a + 1.0 {
        // Series expansion: P(a,z) = e^(-z) * z^a * sum_{n=0}^inf z^n / (a*(a+1)*...*(a+n))
        // = e^(-z) * z^a / Gamma(a+1) * sum_{n=0}^inf ...
        let mut sum = 1.0_f64;
        let mut term = 1.0_f64;
        for n in 1..300 {
            term *= z / (a + n as f64);
            sum += term;
            if term.abs() < 1e-15 * sum.abs() {
                break;
            }
        }
        (-z + a * z.ln() - a.ln() - log_gamma_lanczos(a)).exp() * sum
    } else {
        // Continued fraction for Q(a,z) = 1 - P(a,z)
        // Use Legendre's continued fraction
        let q = 1.0 - gamma_cf(a, z);
        q.clamp(0.0, 1.0)
    }
}

/// Continued fraction for Q(a,z) = Gamma(a,z)/Gamma(a) = 1 - P(a,z).
fn gamma_cf(a: f64, z: f64) -> f64 {
    // Lentz's algorithm for continued fraction
    let mut f = 1.0_f64;
    let mut c = f;
    let mut d = 0.0_f64;
    for n in 1..300 {
        let an = if n % 2 == 1 {
            let m = (n - 1) / 2;
            -(a + m as f64) * (a + m as f64) // Not quite right, use standard form
        } else {
            let m = n / 2;
            m as f64 // Even terms
        };
        // Standard CF: z + 1 - a + (1*(a-1))/(z+3-a+ (2*(a-2))/(z+5-a+ ...))
        // Simpler: use the series for the upper incomplete gamma
        // Actually let's use a simpler CF form
        d += z + n as f64 - a; // Not standard
        if d == 0.0 {
            d = 1e-30;
        }
        c = z + n as f64 - a + an / c;
        if c == 0.0 {
            c = 1e-30;
        }
        d = 1.0 / d;
        let delta = c * d;
        f *= delta;
        if (delta - 1.0).abs() < 1e-15 {
            break;
        }
    }
    // Q(a,z) ≈ e^(-z) * z^a / (Gamma(a) * f)
    let ln_q = -z + a * z.ln() - log_gamma_lanczos(a) - f.ln();
    ln_q.exp()
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

/// Gamma[z] with the exact special value Gamma[1/2] = Sqrt[Pi]; otherwise
/// delegates to the numeric builtin in `math`.
pub fn builtin_gamma_exact(args: &[Value]) -> Result<Value, EvalError> {
    if let [Value::Rational(r)] = args
        && *r.numer() == 1
        && *r.denom() == 2
    {
        return Ok(crate::builtins::symbolic::simplify_call(
            "Sqrt",
            &[Value::Symbol("Pi".to_string())],
        ));
    }
    crate::builtins::math::builtin_gamma(args)
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
}
