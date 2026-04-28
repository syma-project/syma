use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;

// Helper: convert a Value to f64.
fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        Value::Rational(r) => Some(r.to_f64()),
        _ => None,
    }
}

// Helper: create a Real value from f64.
fn real(v: f64) -> Value {
    Value::Real(Float::with_val(DEFAULT_PRECISION, v))
}

// Helper: create an unevaluated Call node.
fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

// ── Erf / Erfc / ErfInverse ──

const ERf_P: f64 = 0.3275911;
const ERf_A1: f64 = 0.254829592;
const ERf_A2: f64 = -0.284496736;
const ERf_A3: f64 = 1.421413741;
const ERf_A4: f64 = -1.453152027;
const ERf_A5: f64 = 1.061405429;

fn erf_approx(x: f64) -> f64 {
    let ax = x.abs();
    let t = 1.0 / (1.0 + ERf_P * ax);
    let poly = ERf_A1*t + (ERf_A2*t + (ERf_A3*t + (ERf_A4*t + ERf_A5*t))*t)*t;
    let result = 1.0 - poly * (-ax * ax).exp();
    if x < 0.0 { -result } else { result }
}

pub fn builtin_erf(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Erf requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) => {
            let x = n.to_f64();
            if x.is_infinite() {
                return Ok(Value::Integer(Integer::from(if x > 0.0 { 1 } else { -1 })));
            }
            Ok(real(erf_approx(x)))
        }
        Value::Real(r) => {
            if r.is_infinite() {
                return Ok(Value::Integer(Integer::from(if r.is_sign_positive() { 1 } else { -1 })));
            }
            Ok(real(erf_approx(r.to_f64())))
        }
        _ => Ok(unevaluated("Erf", args)),
    }
}

pub fn builtin_erfc(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Erfc requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(1))),
        Value::Integer(n) => {
            let x = n.to_f64();
            if x.is_infinite() {
                return Ok(Value::Integer(Integer::from(if x > 0.0 { 0 } else { 2 })));
            }
            Ok(real(1.0 - erf_approx(x)))
        }
        Value::Real(r) => {
            if r.is_infinite() {
                return Ok(Value::Integer(Integer::from(if r.is_sign_positive() { 0 } else { 2 })));
            }
            Ok(real(1.0 - erf_approx(r.to_f64())))
        }
        _ => Ok(unevaluated("Erfc", args)),
    }
}

fn inverse_erf_approx(x: f64) -> f64 {
    if x.abs() < 1e-10 {
        return x / std::f64::consts::PI * (2.0 + std::f64::consts::PI * x * x);
    }
    let s = x.signum();
    let ax = x.abs();
    if ax < 0.7 {
        let c0 = 0.5 * std::f64::consts::PI;
        let c1 = c0 / 3.0;
        let c2 = 3.0 * c0 / 40.0;
        let c3 = c0 / 105.0;
        let c4 = 3.0 * c0 / 3763.0;
        let ax2 = ax * ax;
        s * (ax * (c0 + c1*ax2 + c2*ax2*ax2 + c3*ax2*ax2*ax2*ax2 + c4*ax2*ax2*ax2*ax2*ax2*ax2))
    } else {
        let mut w = s * (-2.0 * (1.0 - ax) * (1.0 + 0.5 * (1.0 - ax))).ln() / std::f64::consts::PI;
        for _ in 0..20 {
            let ew = erf_approx(w);
            let diff = ew - x;
            if diff.abs() < 1e-16 { break; }
            let deriv = 2.0 / std::f64::consts::PI.sqrt() * (-w * w).exp();
            w -= diff / deriv;
        }
        w
    }
}
        w
    }
}

fn abs_diff_break(diff: f64, w: f64, _x: f64) {
    if diff.abs() < 1e-16 { /* signal handled by loop */ }
    let _ = (w, );
}

pub fn builtin_erf_inverse(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("ErfInverse requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) => {
            let x = n.to_f64();
            if x.abs() >= 1.0 {
                return Err(EvalError::Error("ErfInverse: argument must be in (-1, 1)".to_string()));
            }
            Ok(real(inverse_erf_approx(x)))
        }
        Value::Real(r) => {
            let x = r.to_f64();
            if x.abs() >= 1.0 {
                return Err(EvalError::Error("ErfInverse: argument must be in (-1, 1)".to_string()));
            }
            Ok(real(inverse_erf_approx(x)))
        }
        _ => Ok(unevaluated("ErfInverse", args)),
    }
}

// ── Beta Function ──

pub fn builtin_beta(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Beta requires exactly 2 arguments".to_string()));
    }
    match (&args[0], &args[1]) {
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
            Ok(Value::Real((&gamma_x * &gamma_y) / gamma_xy))
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
            Ok(Value::Real((&gamma_x * &gamma_y) / gamma_xy))
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
        Value::Integer(n) if *n == Integer::from(0) => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) if *n == Integer::from(1) => Ok(Value::Integer(Integer::from(1))),
        Value::Real(r) if r.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Real(r) if *r == Float::with_val(DEFAULT_PRECISION, 1u32) => {
            Ok(Value::Integer(Integer::from(1)))
        }
        _ => Ok(unevaluated("BetaRegularized", args)),
    }
}

// ── Zeta ──

fn is_small_even_positive(n: &Float) -> Option<u64> {
    let rounded = n.round();
    let diff = Float::with_val(n.prec(), n - &rounded).abs();
    if diff > 1e-10 {
        return None;
    }
    let int_val = rounded.to_integer();
    if !int_val.is_negative()
        && int_val % Integer::from(2) == Integer::from(0)
        && int_val > Integer::from(1)
        && int_val <= Integer::from(20)
    {
        Some(int_val.to_u64())
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
    let two_pi_n = two_pi.pow(k);
    let mut factorial = Float::with_val(prec, 1u32);
    for i in 2..=k as i64 {
        factorial *= Float::with_val(prec, i);
    }
    let b_abs = Float::with_val(prec, bn_num.abs()) / bn_den.abs();
    (&two_pi_n * &b_abs) / (Float::with_val(prec, 2u32) * factorial)
}

pub fn builtin_zeta(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Zeta requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) => {
            if *n <= Integer::from(1) {
                return Ok(unevaluated("Zeta", args));
            }
            let f = Float::with_val(DEFAULT_PRECISION, n);
            if let Some(k) = is_small_even_positive(&f) {
                return Ok(Value::Real(zeta_even(k)));
            }
            Ok(Value::Real(compute_zeta_numeric(&f)))
        }
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
    let s_floor = s_f.floor();
    let tolerance = Float::with_val(prec, 1e-15);

    if s_floor > Float::with_val(prec, 2u32) {
        let mut sum = Float::with_val(prec, 0.0);
        for n in 1..=1_000_000u32 {
            let n_f = Float::with_val(prec, n);
            let term = Float::with_val(prec, 1.0) / n_f.pow(s_f.clone());
            sum += term.clone();
            if term < tolerance.clone() {
                break;
            }
        }
        sum
    } else {
        let max_n: u64 = 10_000_000;
        let mut sum = Float::with_val(prec, 0.0);
        for n in 1..=max_n {
            let n_f = Float::with_val(prec, n);
            sum += Float::with_val(prec, 1.0) / n_f.pow(s_f.clone());
        }
        let n_last = Float::with_val(prec, max_n);
        let one = Float::with_val(prec, 1u32);
        let tail = (&n_last.pow(&one - &s_f)) / (&s_f - &one);
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

    if let (Value::Integer(s), _) = (s_val, z_val) {
        if *s == Integer::from(0) {
            // Li_0(z) = z / (1 - z)
            if let Some(z) = to_f64(z_val) {
                if (z - 1.0).abs() < 1e-15 {
                    return Ok(unevaluated("PolyLog", args));
                }
                return Ok(real(z / (1.0 - z)));
            }
            return Ok(unevaluated("PolyLog", args));
        }
        if *s == Integer::from(1) {
            // Li_1(z) = -Log(1 - z)
            if let Some(z) = to_f64(z_val) {
                if z >= 1.0 {
                    return Ok(unevaluated("PolyLog", args));
                }
                return Ok(real(-(1.0 - z).ln()));
            }
            return Ok(unevaluated("PolyLog", args));
        }
    }

    if let (Some(s), Some(z)) = (to_f64(s_val), to_f64(z_val)) {
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
        return Ok(real(sum));
    }

    Ok(unevaluated("PolyLog", args))
}

// ── Digamma / PolyGamma ──

const EULER_GAMMA_F64: f64 = 0.57721566490153286060;

fn digamma_approx(z: f64) -> f64 {
    let mut offset = 0.0_f64;
    let mut z = z;
    while z < 10.0 {
        offset += 1.0 / z;
        z += 1.0;
    }
    // Undo last increment
    offset -= 1.0 / z;
    z -= 1.0;
    let z2 = z * z;
    let z4 = z2 * z2;
    let z6 = z4 * z2;
    let z8 = z4 * z4;
    offset + z.ln() - 1.0 / (2.0 * z) - 1.0 / (12.0 * z2)
        + 1.0 / (120.0 * z4) - 1.0 / (252.0 * z6) + 691.0 / (240.0 * z8)
}

pub fn builtin_digamma(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Digamma requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) if *n == Integer::from(1) => {
            Ok(real(-EULER_GAMMA_F64))
        }
        Value::Integer(n) => {
            if n.is_negative() || *n == Integer::from(0) {
                return Ok(unevaluated("Digamma", args));
            }
            Ok(real(digamma_approx(n.to_f64())))
        }
        Value::Real(r) => {
            if r.is_sign_negative() || r.is_zero() {
                return Ok(unevaluated("Digamma", args));
            }
            Ok(real(digamma_approx(r.to_f64())))
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
        if *n == Integer::from(0) {
            return builtin_digamma(&[z_val.clone()]);
        }
        if n.is_negative() {
            return Ok(unevaluated("PolyGamma", args));
        }
    }

    if let (Some(n), Some(z)) = (to_f64(n_val), to_f64(z_val)) {
        if z <= 0.0 {
            return Ok(unevaluated("PolyGamma", args));
        }
        let n_int = n as u64;
        // ψ^(n)(z) = (-1)^(n+1) * n! * Σ_{k=0}^∞ 1/(k+z)^(n+1)
        let sign = if n_int % 2 == 0 { -1.0 } else { 1.0 };
        let mut factorial = 1.0_f64;
        for i in 1..=n as u32 {
            factorial *= i as f64;
        }
        let mut sum = 0.0_f64;
        let p = f64::from(n_int + 1);
        let mut k: u32 = 0;
        while k < 10_000_000 {
            let term = 1.0 / (k as f64 + z).powf(p);
            sum += term;
            if term < 1e-16 {
                break;
            }
            k += 1;
        }
        Ok(real(sign * factorial * sum))
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
            let ai0 = 0.35502805388781723587;
            let ai0_prime = -0.25932985380908484339;
            // Series: Ai(z) = ai0 * Σ (3^k * z^(3k)) / (3^k * k! * 2^k * 3^k * ...)
            // Simpler: use the known series form
            // Ai(z) = ai0 * S0(z) + ai0_prime * S1(z)
            // where S0 = 1 + z^3/6 + 7z^6/360 + ...
            //       S1 = z + z^4/12 + z^7/720 + ...
            let mut s0 = 1.0_f64;
            let mut s1 = z;
            let mut z3 = z * z * z;
            let mut z7 = z3 * z * z * z;
            let mut term0 = z3 / 6.0;
            let mut term1 = z7 / 12.0;
            s0 += term0;
            s1 += term1;
            // Additional terms
            let z9 = z7 * z * z;
            let z12 = z9 * z * z * z;
            s0 += 7.0 * z9 / 360.0;
            s1 += z12 / 720.0;
            ai0 * s0 + ai0_prime * s1
        } else {
            // Asymptotic expansion for z >= 0.5
            let z32 = z.sqrt() * z; // z^(3/2)
            let exp_factor = (-2.0 / 3.0 * z32).exp();
            let z14 = z.sqrt().sqrt(); // z^(1/4)
            let leading = exp_factor / (2.0 * std::f64::consts::PI.sqrt() * z14);
            // First few terms of the asymptotic series
            let u = 1.0 / (z32);
            let p = 1.0 - 5.0/144.0*u + 2005.0/21504.0*u*u - 156205.0/2129920.0*u*u*u;
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
        let p = 1.0 - 5.0/144.0*u + 2005.0/21504.0*u*u;
        leading * p * theta.sin()
    }
}

pub fn builtin_airy_ai(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("AiryAi requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => {
            Ok(real(0.3550280538878172))
        }
        Value::Integer(n) => Ok(real(airy_ai_approx(n.to_f64()))),
        Value::Real(r) => Ok(real(airy_ai_approx(r.to_f64()))),
        _ => Ok(unevaluated("AiryAi", args)),
    }
}

// ── BesselJ ──

pub fn builtin_bessel_j(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "BesselJ requires exactly 2 arguments: BesselJ[n, z]".to_string(),
        ));
    }
    let n_val = &args[0];
    let z_val = &args[1];

    // J_n(0) special cases
    if let Value::Integer(z) = z_val {
        if z.is_zero() {
            if let Value::Integer(n) = n_val {
                if *n == Integer::from(0) {
                    return Ok(Value::Integer(Integer::from(1)));
                }
                return Ok(Value::Integer(Integer::from(0)));
            }
        }
    }
    if let Value::Real(z) = z_val {
        if z.is_zero() {
            if let Value::Integer(n) = n_val {
                if *n == Integer::from(0) {
                    return Ok(Value::Integer(Integer::from(1)));
                }
                return Ok(Value::Integer(Integer::from(0)));
            }
        }
    }

    match (n_val, z_val) {
        (Value::Integer(n), Value::Integer(z)) | (Value::Integer(n), Value::Real(z))
            if !n.is_negative() =>
        {
            if let (Some(n), Some(z)) = (n.to_u64(), to_f64(z_val)) {
                return Ok(real(bessel_j_series(n, z)));
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
    let neg = if n % 2 == 0 { 1.0 } else { -1.0 };
    sum += neg * z_power / gamma_nk1;
    for k in 1..=100 {
        fact_k *= k as f64;
        gamma_nk1 *= (n + k) as f64;
        z_power *= half_z * half_z;
        let sign = if (k + n) % 2 == 0 { 1.0 } else { -1.0 };
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
    match (&args[0], &args[1]) {
        (Value::Integer(n), _) | (_, Value::Integer(_)) | (_, Value::Real(_))
            if matches!(&args[0], Value::Integer(n) if !n.is_negative()) =>
        {
            if let (Some(n), Some(z)) = (to_f64(&args[0]), to_f64(&args[1])) {
                return Ok(real(bessel_i_series(n as u64, z)));
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
    const NEG_INV_E: f64 = -1.0 / std::f64::consts::E;
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) => {
            let z = n.to_f64();
            if z < NEG_INV_E {
                return Err(EvalError::Error(
                    "LambertW: argument must be >= -1/e".to_string(),
                ));
            }
            Ok(real(lambert_w_approx(z)))
        }
        Value::Real(r) => {
            let z = r.to_f64();
            if z < NEG_INV_E {
                return Err(EvalError::Error(
                    "LambertW: argument must be >= -1/e".to_string(),
                ));
            }
            Ok(real(lambert_w_approx(z)))
        }
        _ => Ok(unevaluated("LambertW", args)),
    }
}

fn lambert_w_approx(z: f64) -> f64 {
    if z == 0.0 {
        return 0.0;
    }
    if (z - NEG_INV_E).abs() < 1e-15 {
        return -1.0;
    }
    // Initial guess
    let mut w = if z > 0.0 {
        z.ln()
    } else {
        -1.0
    };
    // Halley's method
    for _ in 0..30 {
        let ew = (-w).exp();
        let f = w * ew - z;
        let fp = ew * (w + 1.0);
        let fp2 = ew * (w + 2.0);
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
        Value::Integer(n) if *n == Integer::from(1) => {
            Ok(real(std::f64::consts::LN_2))
        }
        Value::Integer(n) => {
            if n.is_negative() || *n == Integer::from(0) {
                return Ok(unevaluated("DirichletEta", args));
            }
            Ok(real(compute_eta_numeric(n.to_f64())))
        }
        Value::Real(r) => {
            if r.is_sign_negative() || r.is_zero() {
                return Ok(unevaluated("DirichletEta", args));
            }
            if let Some(one) = to_f64(&Value::Integer(Integer::from(1))) {
                if (r.to_f64() - one).abs() < 1e-15 {
                    return Ok(real(std::f64::consts::LN_2));
                }
            }
            if r.to_f64() <= 0.0 {
                return Ok(unevaluated("DirichletEta", args));
            }
            Ok(real(compute_eta_numeric(r.to_f64())))
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

// ── Registration helper ──

pub fn register_sfs(env: &crate::env::Env) {
    use crate::builtins::{register_builtin, register_builtin_env};

    register_builtin(env, "Erf", builtin_erf);
    register_builtin(env, "Erfc", builtin_erfc);
    register_builtin(env, "ErfInverse", builtin_erf_inverse);
    register_builtin(env, "Beta", builtin_beta);
    register_builtin(env, "BetaRegularized", builtin_beta_regularized);
    register_builtin(env, "Zeta", builtin_zeta);
    register_builtin(env, "PolyLog", builtin_polylog);
    register_builtin(env, "Digamma", builtin_digamma);
    register_builtin(env, "PolyGamma", builtin_polygamma);
    register_builtin(env, "AiryAi", builtin_airy_ai);
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
        assert!(matches!(result, Value::Integer(ref n) if *n == Integer::from(0)));
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
        assert!(matches!(result, Value::Integer(ref n) if *n == Integer::from(1)));
    }

    #[test]
    fn test_erfc_erf_complement() {
        let erf_val = builtin_erf(&[Value::Real(Float::with_val(DEFAULT_PRECISION, 0.5))]).unwrap();
        let erfc_val = builtin_erfc(&[Value::Real(Float::with_val(DEFAULT_PRECISION, 0.5))]).unwrap();
        if let (Value::Real(e), Value::Real(c)) = (erf_val, erfc_val) {
            let sum = e.to_f64() + c.to_f64();
            assert!((sum - 1.0).abs() < 1e-10);
        }
    }

    #[test]
    fn test_erf_inverse_zero() {
        let result = builtin_erf_inverse(&[Value::Integer(Integer::from(0))]).unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == Integer::from(0)));
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
        if let Value::Real(r) = result {
            // B(2,3) = 1! * 2! / 4! = 2/24 = 1/12 ≈ 0.08333
            assert!((r.to_f64() - 1.0/12.0).abs() < 1e-8);
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
        assert!(matches!(result, Value::Integer(ref n) if *n == Integer::from(0)));
    }

    #[test]
    fn test_beta_regularized_one() {
        let result = builtin_beta_regularized(&[
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
        ])
        .unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == Integer::from(1)));
    }

    #[test]
    fn test_zeta_even() {
        let result = builtin_zeta(&[Value::Integer(Integer::from(2))]).unwrap();
        if let Value::Real(r) = result {
            let expected = std::f64::consts::PI * std::f64::consts::PI / 6.0;
            assert!((r.to_f64() - expected).abs() < 1e-6);
        }
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
            assert!((r.to_f64() - 0.693147).abs() < 1e-4);
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
        let result = builtin_airy_ai(&[Value::Integer(Integer::from(0))]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 0.355028).abs() < 1e-5);
        }
    }

    #[test]
    fn test_bessel_j_0_0() {
        let result = builtin_bessel_j(&[
            Value::Integer(Integer::from(0)),
            Value::Integer(Integer::from(0)),
        ])
        .unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == Integer::from(1)));
    }

    #[test]
    fn test_bessel_j_1_0() {
        let result = builtin_bessel_j(&[
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(0)),
        ])
        .unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == Integer::from(0)));
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
        assert!(matches!(result, Value::Integer(Integer(ref n)) if *n == Integer::from(1)));
    }

    #[test]
    fn test_lambert_w_zero() {
        let result = builtin_lambert_w(&[Value::Integer(Integer::from(0))]).unwrap();
        assert!(matches!(result, Value::Integer(ref n) if *n == Integer::from(0)));
    }

    #[test]
    fn test_lambert_w_e() {
        let result = builtin_lambert_w(&[
            Value::Real(Float::with_val(DEFAULT_PRECISION, std::f64::consts::E)),
        ])
        .unwrap();
        if let Value::Real(r) = result {
            // W(e) = 1
            assert!((r.to_f64() - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_lambert_w_neg_inv_e() {
        let result = builtin_lambert_w(&[
            Value::Real(Float::with_val(DEFAULT_PRECISION, -1.0 / std::f64::consts::E)),
        ])
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
