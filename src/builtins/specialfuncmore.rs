//! hypergeometric and higher special functions: Hypergeometric0F1/1F1/2F1/PFQ, Zeta family, integral functions, Fresnel, Struve, Kelvin, AppellF1.
//!
//! Gap-fill module; see the individual builtin docs for semantics.
//!
//! All functions evaluate numerically for real (and real-compatible) arguments
//! and fall back to an unevaluated `Call` node for symbolic or inapplicable
//! input, mirroring the Wolfram Language convention in the sibling
//! `specialfunctions` module.

use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::Float;

/// Extract a high-precision real from a numeric value.
fn float_val(v: &Value, prec: u32) -> Option<Float> {
    match v {
        Value::Integer(n) => Some(Float::with_val(prec, n)),
        Value::Real(r) => Some(r.clone()),
        Value::Rational(r) => Some(Float::with_val(prec, &**r)),
        _ => None,
    }
}

/// Build an unevaluated call node, preserving argument order.
fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

/// Extract a list of `f64` from a `Value::List`; `None` if not a numeric list.
fn f64_list(v: &Value) -> Option<Vec<f64>> {
    match v {
        Value::List(items) => items.iter().map(super::to_f64).collect(),
        _ => None,
    }
}

/// Convert a slice of values to `f64`; `None` if any is non-numeric.
fn vals_to_f64(vs: &[Value]) -> Option<Vec<f64>> {
    vs.iter().map(super::to_f64).collect()
}

/// Extract the two sub-lists of a `MeijerG` parameter group.
#[allow(dead_code)]
fn two_lists(v: &Value) -> Option<(Vec<f64>, Vec<f64>)> {
    match v {
        Value::List(outer) if outer.len() == 2 => {
            let up = f64_list(&outer[0])?;
            let low = f64_list(&outer[1])?;
            Some((up, low))
        }
        _ => None,
    }
}

/// Extract the two sub-lists of a `MeijerG` parameter group as raw values, so
/// that symbolic parameters are preserved for the exact-specialisation table.
fn two_value_lists(v: &Value) -> Option<(Vec<Value>, Vec<Value>)> {
    match v {
        Value::List(outer) if outer.len() == 2 => match (&outer[0], &outer[1]) {
            (Value::List(up), Value::List(low)) => Some((up.clone(), low.clone())),
            _ => None,
        },
        _ => None,
    }
}

/// Build an exact rational value `n/d`.
fn rational(n: i64, d: i64) -> Value {
    Value::Rational(Box::new(rug::Rational::from((n, d))))
}

/// `true` when `v` is the integer `n` (exact or numeric).
fn is_int(v: &Value, n: i64) -> bool {
    match v {
        Value::Integer(k) => *k == n,
        Value::Real(r) => r.to_f64() == n as f64,
        _ => false,
    }
}

/// `true` when `v` is the rational `n/d` (exact or numeric).
fn is_rat(v: &Value, n: i64, d: i64) -> bool {
    match v {
        Value::Rational(r) => *r.numer() == n && *r.denom() == d,
        Value::Integer(k) => d != 0 && n % d == 0 && *k == n / d,
        Value::Real(r) => (r.to_f64() - n as f64 / d as f64).abs() < 1e-12,
        _ => false,
    }
}

/// Structural or numeric equality of two values.
fn num_eq(x: &Value, y: &Value) -> bool {
    if x == y {
        return true;
    }
    match (super::to_f64(x), super::to_f64(y)) {
        (Some(a), Some(b)) => (a - b).abs() < 1e-12,
        _ => false,
    }
}

/// Shortcut for the symbolic simplifier.
fn sc(head: &str, args: &[Value]) -> Value {
    crate::builtins::symbolic::simplify_call(head, args)
}

/// `true` when every value converts to `f64`.
fn all_numeric(vals: &[&Value]) -> bool {
    vals.iter().all(|v| super::to_f64(v).is_some())
}

// ── Scalar special-function kernels ─────────────────────────────────────────

const EULER_GAMMA: f64 = std::f64::consts::EULER_GAMMA;

/// Stirling/Lanczos approximation to `Γ(x)` for real `x` (reflection for `x<0.5`).
fn gamma_fn(x: f64) -> f64 {
    if x < 0.5 {
        std::f64::consts::PI / ((std::f64::consts::PI * x).sin() * gamma_fn(1.0 - x))
    } else {
        const G: f64 = 7.0;
        const C: [f64; 9] = [
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
        let z = x - 1.0;
        let mut sum = C[0];
        for (i, c) in C.iter().enumerate().skip(1) {
            sum += c / (z + i as f64);
        }
        let t = z + G + 0.5;
        (2.0 * std::f64::consts::PI).sqrt() * t.powf(z + 0.5) * (-t).exp() * sum
    }
}

/// Rising factorial `(a)_n = a(a+1)···(a+n-1)`.
#[allow(dead_code)]
fn rising(a: f64, n: u32) -> f64 {
    let mut p = 1.0;
    for k in 0..n {
        p *= a + k as f64;
    }
    p
}

/// Generalized hypergeometric series `_pF_q(a; b; z)` evaluated by direct
/// summation, with Aitken acceleration when the terms do not decay (e.g.
/// `_1F_0` on the unit circle, summable in the Abel sense).
fn hyper_pfq(a: &[f64], b: &[f64], z: f64) -> f64 {
    let mut term = 1.0_f64;
    // Three most recent partial sums, for Aitken Δ² acceleration.
    let mut p0 = 0.0_f64;
    let mut p1 = 0.0_f64;
    let mut p2 = 1.0_f64;
    let mut converged = false;
    for k in 1..=2000u32 {
        let mut num = z;
        for &ai in a {
            num *= ai + (k - 1) as f64;
        }
        let mut den = k as f64;
        for &bj in b {
            den *= bj + (k - 1) as f64;
        }
        term *= num / den;
        p0 = p1;
        p1 = p2;
        p2 += term;
        if !term.is_finite() {
            break;
        }
        if term.abs() < 1e-17 * p2.abs().max(1.0) {
            converged = true;
            break;
        }
    }
    if converged {
        return p2;
    }
    let d = p2 - 2.0 * p1 + p0;
    if d.abs() > 1e-300 {
        p2 - (p2 - p1) * (p2 - p1) / d
    } else {
        p2
    }
}

/// True if `x` is a non-positive integer within tolerance.
fn is_nonpos_int(x: f64) -> bool {
    x <= 0.0 && (x - x.round()).abs() < 1e-12
}

/// Digamma `ψ(x) = d/dx ln Γ(x)` for real `x`, by recurrence plus the
/// Bernoulli asymptotic series (accurate to ~1e-15).
fn digamma_fn(mut x: f64) -> f64 {
    let mut result = 0.0;
    while x < 8.0 {
        result -= 1.0 / x;
        x += 1.0;
    }
    let inv = 1.0 / x;
    let inv2 = inv * inv;
    result + x.ln()
        - 0.5 * inv
        - inv2
            * (1.0 / 12.0
                - inv2 * (1.0 / 120.0 - inv2 * (1.0 / 252.0 - inv2 * (1.0 / 240.0 - inv2 / 132.0))))
}

/// `n!` for small non-negative integer `n`.
fn factorial_small(n: u32) -> f64 {
    let mut p = 1.0_f64;
    for k in 2..=n {
        p *= k as f64;
    }
    p
}

/// Gauss hypergeometric `_2F_1(a, b; c; z)` using the series plus Pfaff and
/// linear transformations to bring the argument into the unit disk. Handles
/// terminating series, `z = 1`, `z < 0` and `z` near `1` (where the direct
/// series converges too slowly to be accurate).
fn hyper_2f1(a: f64, b: f64, c: f64, z: f64) -> f64 {
    if z == 0.0 {
        return 1.0;
    }
    // A non-positive integer numerator terminates the series for every z.
    if is_nonpos_int(a) || is_nonpos_int(b) {
        return hyper_pfq(&[a, b], &[c], z);
    }
    if z == 1.0 {
        let s = c - a - b;
        if s > 1e-12 {
            // Gauss summation, valid when c - a - b > 0.
            return gamma_fn(c) * gamma_fn(s) / (gamma_fn(c - a) * gamma_fn(c - b));
        }
        if s.abs() <= 1e-9 {
            // c = a + b: the series diverges logarithmically; report the sign.
            let coeff = gamma_fn(c) / (gamma_fn(c - a) * gamma_fn(c - b));
            return if coeff >= 0.0 {
                f64::INFINITY
            } else {
                f64::NEG_INFINITY
            };
        }
        return f64::NAN;
    }
    if z < 0.0 {
        // Pfaff: 2F1(a,b;c;z) = (1-z)^-a 2F1(a, c-b; c; z/(z-1));
        // z/(z-1) lies in (0,1), so convergence is recovered.
        let w = z / (z - 1.0);
        return (1.0 - z).powf(-a) * hyper_2f1(a, c - b, c, w);
    }
    if z <= 0.9 {
        return hyper_pfq(&[a, b], &[c], z);
    }
    if z < 1.0 {
        return hyper_2f1_near_one(a, b, c, z);
    }
    // z > 1: linear transformation to argument 1 - z (real for integer c-a-b).
    let s = c - a - b;
    let y = 1.0 - z;
    let front = gamma_fn(c) * gamma_fn(s) / (gamma_fn(c - a) * gamma_fn(c - b));
    let second = gamma_fn(c) * gamma_fn(-s) / (gamma_fn(a) * gamma_fn(b));
    front * hyper_pfq(&[a, b], &[1.0 - s], y)
        + second * y.powf(s) * hyper_pfq(&[c - a, c - b], &[1.0 + s], y)
}

/// `_2F_1(a,b;c;z)` for `0.9 < z < 1`, where the direct series is slow.
///
/// When `c - a - b` is an integer the limiting (possibly logarithmic) formula
/// DLMF 15.8.10/15.8.12 is used; otherwise the standard linear transformation
/// with argument `1 - z` is applied.
fn hyper_2f1_near_one(a: f64, b: f64, c: f64, z: f64) -> f64 {
    if is_nonpos_int(a) || is_nonpos_int(b) {
        return hyper_pfq(&[a, b], &[c], z);
    }
    let s = c - a - b;
    let m = s.round();
    if (s - m).abs() < 1e-9 {
        if (0.0..60.0).contains(&m) {
            return hyper_2f1_int(a, b, m as u32, z);
        }
        if m < 0.0 {
            // DLMF 15.8.12: reduce to the non-negative integer case.
            let mm = (-m) as u32;
            return (1.0 - z).powf(s) * hyper_2f1_int(a + s, b + s, mm, z);
        }
    }
    // Otherwise apply (Erdelyi) with argument 1 - z; the two series converge
    // quickly since |1 - z| is small.
    let y = 1.0 - z;
    let t1 = gamma_fn(c) * gamma_fn(s) / (gamma_fn(c - a) * gamma_fn(c - b))
        * hyper_pfq(&[a, b], &[1.0 - s], y);
    let t2 = gamma_fn(c) * gamma_fn(-s) / (gamma_fn(a) * gamma_fn(b))
        * y.powf(s)
        * hyper_pfq(&[c - a, c - b], &[1.0 + s], y);
    t1 + t2
}

/// DLMF 15.8.10: `_2F_1(a, b; a+b+m; z)` for integer `m ≥ 0`, `|z-1| < 1`.
fn hyper_2f1_int(a: f64, b: f64, m: u32, z: f64) -> f64 {
    if is_nonpos_int(a) || is_nonpos_int(b) {
        return hyper_pfq(&[a, b], &[a + b + m as f64], z);
    }
    let c = a + b + m as f64;
    let mf = m as f64;
    let y = 1.0 - z;
    if m == 0 {
        // Logarithmic case c = a + b.
        let pref = gamma_fn(c) / (gamma_fn(a) * gamma_fn(b));
        let mut acc = 0.0_f64;
        let mut t = 1.0_f64;
        for k in 0..400_000u32 {
            let kf = k as f64;
            let bracket =
                2.0 * digamma_fn(kf + 1.0) - digamma_fn(a + kf) - digamma_fn(b + kf) - y.ln();
            acc += t * bracket;
            t *= (a + kf) * (b + kf) / ((kf + 1.0) * (kf + 1.0)) * y;
            if (t * bracket).abs() < 1e-18 * acc.abs().max(1e-300) {
                break;
            }
        }
        return pref * acc;
    }
    // Finite polynomial part.
    let pref1 = gamma_fn(c) / (gamma_fn(a + mf) * gamma_fn(b + mf));
    let mut acc1 = 0.0_f64;
    for k in 0..m {
        acc1 += rising(a, k) * rising(b, k) * factorial_small(m - k - 1) / factorial_small(k)
            * (z - 1.0).powi(k as i32);
    }
    acc1 *= pref1;
    // Logarithmic tail.
    let pref2 = gamma_fn(c) / (gamma_fn(a) * gamma_fn(b));
    let mut acc2 = 0.0_f64;
    let mut t = 1.0 / factorial_small(m);
    for n in 0..800_000u32 {
        let nf = n as f64;
        let bracket = y.ln() - digamma_fn(nf + 1.0) - digamma_fn(nf + mf + 1.0)
            + digamma_fn(a + nf + mf)
            + digamma_fn(b + nf + mf);
        acc2 += t * bracket;
        t *= (a + mf + nf) * (b + mf + nf) / ((nf + 1.0) * (nf + mf + 1.0)) * y;
        if (t * bracket).abs() < 1e-18 * acc2.abs().max(1e-300) {
            break;
        }
    }
    acc1 - pref2 * (z - 1.0).powi(m as i32) * acc2
}

/// Asymptotic series `Σ (p)_n (q)_n / n! x^{-n}` used by the large-`|z|`
/// confluent expansion (DLMF 13.5.1).
fn asym_series(p: f64, q: f64, x: f64) -> f64 {
    let mut sum = 1.0_f64;
    let mut term = 1.0_f64;
    for n in 1..=40u32 {
        let nf = (n - 1) as f64;
        term *= (p + nf) * (q + nf) / (n as f64) / x;
        sum += term;
        if term.abs() < 1e-18 * sum.abs().max(1e-300) {
            break;
        }
        if term.abs() > 1e15 {
            break;
        }
    }
    sum
}

/// `_1F_1(a;b;z)` evaluated with Kummer's transformation for `z < 0` (which
/// avoids the catastrophic cancellation of the alternating series) and a
/// large-`|z|` asymptotic fallback.
fn confluent_1f1(a: f64, b: f64, z: f64) -> f64 {
    if z >= 0.0 {
        return hyper_pfq(&[a], &[b], z);
    }
    let k = z.exp() * hyper_pfq(&[b - a], &[b], -z);
    if k.is_finite() {
        k
    } else {
        hyper_1f1_asym(a, b, z)
    }
}

/// Large-`|z|` asymptotic evaluation of `_1F_1(a;b;z)` for real `z`.
///
/// For `z < 0` Kummer's transformation is combined with the dominant
/// exponential so that `e^{z}` and `e^{-z}` need not both be finite.
fn hyper_1f1_asym(a: f64, b: f64, z: f64) -> f64 {
    if z >= 0.0 {
        let first =
            gamma_fn(b) / gamma_fn(a) * z.exp() * z.powf(a - b) * asym_series(b - a, 1.0 - a, z);
        let second = gamma_fn(b) / gamma_fn(b - a) * z.powf(-a) * asym_series(a, 1.0 + a - b, -z);
        first + second
    } else {
        let t = -z;
        gamma_fn(b) / gamma_fn(b - a) * t.powf(-a) * asym_series(a, 1.0 + a - b, t)
    }
}

// ── Complex arithmetic helpers (for Riemann–Siegel / Kelvin) ────────────────

#[derive(Clone, Copy, Debug)]
struct Cx {
    re: f64,
    im: f64,
}

impl Cx {
    const fn new(re: f64, im: f64) -> Self {
        Cx { re, im }
    }
    const fn zero() -> Self {
        Cx { re: 0.0, im: 0.0 }
    }
    fn add(self, o: Cx) -> Cx {
        Cx::new(self.re + o.re, self.im + o.im)
    }
    fn sub(self, o: Cx) -> Cx {
        Cx::new(self.re - o.re, self.im - o.im)
    }
    fn mul(self, o: Cx) -> Cx {
        Cx::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
    fn div(self, o: Cx) -> Cx {
        let d = o.re * o.re + o.im * o.im;
        Cx::new(
            (self.re * o.re + self.im * o.im) / d,
            (self.im * o.re - self.re * o.im) / d,
        )
    }
    fn scale(self, s: f64) -> Cx {
        Cx::new(self.re * s, self.im * s)
    }
    fn abs(self) -> f64 {
        (self.re * self.re + self.im * self.im).sqrt()
    }
    fn exp(self) -> Cx {
        let e = self.re.exp();
        Cx::new(e * self.im.cos(), e * self.im.sin())
    }
    fn ln(self) -> Cx {
        Cx::new(self.abs().ln(), self.im.atan2(self.re))
    }
    fn sin(self) -> Cx {
        Cx::new(
            self.re.sin() * self.im.cosh(),
            self.re.cos() * self.im.sinh(),
        )
    }
}

/// `base^s` for a positive real `base` and complex exponent `s`.
fn cpow_real(base: f64, s: Cx) -> Cx {
    s.scale(base.ln()).exp()
}

const LANCZOS_G: f64 = 7.0;
const LANCZOS_C: [f64; 9] = [
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

/// Complex log-gamma via Lanczos with reflection for `Re z < 0.5`.
fn cloggamma(z: Cx) -> Cx {
    let pi = std::f64::consts::PI;
    if z.re < 0.5 {
        let sinpz = Cx::new(pi, 0.0).mul(z).sin();
        Cx::new(pi.ln(), 0.0)
            .sub(sinpz.ln())
            .sub(cloggamma(Cx::new(1.0 - z.re, -z.im)))
    } else {
        let z1 = Cx::new(z.re - 1.0, z.im);
        let mut x = Cx::new(LANCZOS_C[0], 0.0);
        for (i, &c) in LANCZOS_C.iter().enumerate().skip(1) {
            x = x.add(Cx::new(c, 0.0).div(Cx::new(z1.re + i as f64, z1.im)));
        }
        let t = Cx::new(z1.re + LANCZOS_G + 0.5, z1.im);
        Cx::new((2.0 * pi).ln(), 0.0)
            .scale(0.5)
            .add(Cx::new(z1.re + 0.5, z1.im).mul(t.ln()))
            .sub(t)
            .add(x.ln())
    }
}

/// Riemann zeta for complex `s` with `Re s > 0` via Euler–Maclaurin.
fn zeta_complex(s: Cx) -> Cx {
    let n_terms = 16usize.max((s.im.abs() / (2.0 * std::f64::consts::PI)).ceil() as usize + 8);
    let mut sum = Cx::zero();
    for n in 1..n_terms {
        sum = sum.add(cpow_real(n as f64, s.scale(-1.0)));
    }
    let nf = n_terms as f64;
    let inv_n = 1.0 / nf;
    let n_neg_s = cpow_real(nf, s.scale(-1.0));
    let n_1_neg_s = cpow_real(nf, Cx::new(1.0 - s.re, -s.im));
    sum = sum.add(n_1_neg_s.div(Cx::new(s.re - 1.0, s.im)));
    sum = sum.add(n_neg_s.scale(0.5));

    const BERNOULLI: [f64; 12] = [
        1.0 / 6.0,
        -1.0 / 30.0,
        1.0 / 42.0,
        -1.0 / 30.0,
        5.0 / 66.0,
        -691.0 / 2730.0,
        7.0 / 6.0,
        -3617.0 / 510.0,
        43867.0 / 798.0,
        -174611.0 / 330.0,
        854513.0 / 138.0,
        -236364091.0 / 2730.0,
    ];
    let mut rising_c = s;
    let mut fact = 2.0_f64;
    let mut nf_pow = n_neg_s.scale(inv_n);
    for (r0, &b) in BERNOULLI.iter().enumerate() {
        sum = sum.add(rising_c.scale(b / fact).mul(nf_pow));
        let rr = (r0 + 1) as f64;
        rising_c = rising_c
            .mul(Cx::new(s.re + 2.0 * rr - 1.0, s.im))
            .mul(Cx::new(s.re + 2.0 * rr, s.im));
        fact *= (2.0 * rr + 1.0) * (2.0 * rr + 2.0);
        nf_pow = nf_pow.scale(inv_n * inv_n);
    }
    sum
}

/// Riemann–Siegel theta `ϑ(t) = Im LogΓ(1/4 + i t/2) - (t/2) ln π`.
fn riemann_siegel_theta(t: f64) -> f64 {
    let z = Cx::new(0.25, 0.5 * t);
    cloggamma(z).im - 0.5 * t * std::f64::consts::PI.ln()
}

/// Riemann–Siegel `Z(t) = Re[e^{iϑ(t)} ζ(1/2 + i t)]`.
fn riemann_siegel_z(t: f64) -> f64 {
    let theta = riemann_siegel_theta(t);
    let s = Cx::new(0.5, t);
    let zeta = zeta_complex(s);
    theta.cos() * zeta.re - theta.sin() * zeta.im
}

/// `E₁(z)` for `z > 0` via the convergent series.
fn exp_integral_e1(z: f64) -> f64 {
    let mut sum = 0.0_f64;
    let mut term = 1.0_f64; // (-z)^k / k!  with k starting at 1
    for k in 1..=500u32 {
        term *= -z / k as f64;
        sum += term / k as f64;
        if term.abs() < 1e-17 * sum.abs().max(1.0) {
            break;
        }
    }
    -EULER_GAMMA - z.ln() - sum
}

// ── Hypergeometric functions ────────────────────────────────────────────────

/// Exact symbolic specialisations for `_0F_1(b; z)` when at least one argument
/// is non-numeric. Returns `None` when the numeric path should be used or the
/// pattern is unrecognised.
fn hyper_0f1_symbolic(b: &Value, z: &Value) -> Option<Value> {
    if all_numeric(&[b, z]) {
        return None;
    }
    let bf = super::to_f64(b)?;
    if bf < 1.0 || (bf - bf.round()).abs() > 1e-12 {
        return None;
    }
    let bi = bf.round() as i64;
    // Negative argument: 0F1(;b;-w) = Gamma[b] w^{(1-b)/2} BesselJ[b-1, 2 Sqrt[w]].
    // This picks the Bessel-J form, numerically equal to the I form below.
    let neg_arg = match z {
        Value::Call { head, args }
            if head == "Times" && args.len() == 2 && is_int(&args[0], -1) =>
        {
            Some(args[1].clone())
        }
        _ => None,
    };
    let (arg, head) = match &neg_arg {
        Some(w) => (w.clone(), "BesselJ"),
        None => (z.clone(), "BesselI"),
    };
    let e = if (1 - bi) % 2 == 0 {
        int_value((1 - bi) / 2)
    } else {
        rational(1 - bi, 2)
    };
    let zp = sc("Power", &[arg.clone(), e]);
    let bessel = Value::Call {
        head: head.to_string(),
        args: vec![
            Value::Integer(rug::Integer::from(bi - 1)),
            sc(
                "Times",
                &[Value::Integer(rug::Integer::from(2)), sqrt_value(&arg)],
            ),
        ],
    };
    Some(sc("Times", &[sc("Gamma", &[b.clone()]), zp, bessel]))
}

/// `Sqrt[z]` as a symbolic node.
fn sqrt_value(z: &Value) -> Value {
    sc("Sqrt", &[z.clone()])
}

/// Exact symbolic specialisations for `_1F_1(a; b; z)`.
fn hyper_1f1_symbolic(a: &Value, b: &Value, z: &Value) -> Option<Value> {
    if all_numeric(&[a, b, z]) {
        return None;
    }
    // 1F1(a; a; z) = E^z.
    if num_eq(a, b) {
        return Some(sc("Exp", &[z.clone()]));
    }
    // 1F1(1; 2; z) = (E^z - 1)/z.
    if is_int(a, 1) && is_int(b, 2) {
        let num = sc(
            "Plus",
            &[
                sc("Exp", &[z.clone()]),
                Value::Integer(rug::Integer::from(-1)),
            ],
        );
        return Some(sc("Divide", &[num, z.clone()]));
    }
    None
}

/// Exact symbolic specialisations for `_2F_1(a; b; c; z)`.
fn hyper_2f1_symbolic(a: &Value, b: &Value, c: &Value, z: &Value) -> Option<Value> {
    if all_numeric(&[a, b, c, z]) {
        return None;
    }
    // 2F1(1, 1; 2; z) = -Log[1-z]/z.
    if is_int(a, 1) && is_int(b, 1) && is_int(c, 2) {
        let log = sc(
            "Log",
            &[sc(
                "Plus",
                &[
                    Value::Integer(rug::Integer::from(1)),
                    sc(
                        "Times",
                        &[Value::Integer(rug::Integer::from(-1)), z.clone()],
                    ),
                ],
            )],
        );
        return Some(sc(
            "Times",
            &[
                Value::Integer(rug::Integer::from(-1)),
                log,
                sc(
                    "Power",
                    &[z.clone(), Value::Integer(rug::Integer::from(-1))],
                ),
            ],
        ));
    }
    // 2F1(1/2, 1; 3/2; z) = ArcTanh[Sqrt[z]]/Sqrt[z].
    if is_rat(a, 1, 2) && is_int(b, 1) && is_rat(c, 3, 2) {
        let sq = sqrt_value(z);
        return Some(sc(
            "Times",
            &[
                sc("ArcTanh", &[sq.clone()]),
                sc("Power", &[sq, Value::Integer(rug::Integer::from(-1))]),
            ],
        ));
    }
    // 2F1(1/2, 1/2; 3/2; z) = ArcSin[Sqrt[z]]/Sqrt[z].
    if is_rat(a, 1, 2) && is_rat(b, 1, 2) && is_rat(c, 3, 2) {
        let sq = sqrt_value(z);
        return Some(sc(
            "Times",
            &[
                sc("ArcSin", &[sq.clone()]),
                sc("Power", &[sq, Value::Integer(rug::Integer::from(-1))]),
            ],
        ));
    }
    // 2F1(a, b; b; z) = (1-z)^{-a}.
    if num_eq(b, c) {
        let base = sc(
            "Plus",
            &[
                Value::Integer(rug::Integer::from(1)),
                sc(
                    "Times",
                    &[Value::Integer(rug::Integer::from(-1)), z.clone()],
                ),
            ],
        );
        let neg_a = sc(
            "Times",
            &[Value::Integer(rug::Integer::from(-1)), a.clone()],
        );
        return Some(sc("Power", &[base, neg_a]));
    }
    None
}

/// Hypergeometric0F1[b, z] — `_0F_1(b; z) = Σ z^k / ((b)_k k!)`.
pub fn builtin_hypergeometric_0f1(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Hypergeometric0F1", args, 2)?;
    if let Some(v) = hyper_0f1_symbolic(&args[0], &args[1]) {
        return Ok(v);
    }
    let (Some(b), Some(z)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("Hypergeometric0F1", args));
    };
    if z == 0.0 {
        return Ok(Value::Integer(rug::Integer::from(1)));
    }
    let v = hyper_pfq(&[], &[b], z);
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("Hypergeometric0F1", args))
    }
}

/// Hypergeometric1F1[a, b, z] — Kummer's confluent hypergeometric function.
pub fn builtin_hypergeometric_1f1(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Hypergeometric1F1", args, 3)?;
    if let Some(v) = hyper_1f1_symbolic(&args[0], &args[1], &args[2]) {
        return Ok(v);
    }
    let (Some(a), Some(b), Some(z)) = (
        super::to_f64(&args[0]),
        super::to_f64(&args[1]),
        super::to_f64(&args[2]),
    ) else {
        return Ok(unevaluated("Hypergeometric1F1", args));
    };
    if z == 0.0 {
        return Ok(Value::Integer(rug::Integer::from(1)));
    }
    // For z < 0 the plain series suffers catastrophic cancellation; Kummer's
    // transformation 1F1(a;b;z) = e^z 1F1(b-a;b;-z) evaluates the same value
    // with positive argument instead.
    let v = confluent_1f1(a, b, z);
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("Hypergeometric1F1", args))
    }
}

/// Hypergeometric2F1[a, b, c, z] — Gauss hypergeometric function.
pub fn builtin_hypergeometric_2f1(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Hypergeometric2F1", args, 4)?;
    if let Some(v) = hyper_2f1_symbolic(&args[0], &args[1], &args[2], &args[3]) {
        return Ok(v);
    }
    let (Some(a), Some(b), Some(c), Some(z)) = (
        super::to_f64(&args[0]),
        super::to_f64(&args[1]),
        super::to_f64(&args[2]),
        super::to_f64(&args[3]),
    ) else {
        return Ok(unevaluated("Hypergeometric2F1", args));
    };
    if z == 0.0 {
        return Ok(Value::Integer(rug::Integer::from(1)));
    }
    let v = hyper_2f1(a, b, c, z);
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("Hypergeometric2F1", args))
    }
}

/// HypergeometricPFQ[{a1, ...}, {b1, ...}, z] — generalized hypergeometric.
pub fn builtin_hypergeometric_pfq(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("HypergeometricPFQ", args, 3)?;
    // Delegate the symbolic `_1F_1`-shaped cases for consistency.
    if let (Value::List(av), Value::List(bv)) = (&args[0], &args[1])
        && av.len() == 1
        && bv.len() == 1
        && let Some(v) = hyper_1f1_symbolic(&av[0], &bv[0], &args[2])
    {
        return Ok(v);
    }
    if let (Value::List(av), Value::List(bv)) = (&args[0], &args[1])
        && av.is_empty()
        && bv.is_empty()
        && !all_numeric(&[&args[2]])
    {
        return Ok(sc("Exp", &[args[2].clone()]));
    }
    let (Some(a), Some(b), Some(z)) = (
        f64_list(&args[0]),
        f64_list(&args[1]),
        super::to_f64(&args[2]),
    ) else {
        return Ok(unevaluated("HypergeometricPFQ", args));
    };
    let v = if a.len() == 1 && b.len() == 1 {
        confluent_1f1(a[0], b[0], z)
    } else {
        hyper_pfq(&a, &b, z)
    };
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("HypergeometricPFQ", args))
    }
}

/// HypergeometricPFQRegularized[{a1, ...}, {b1, ...}, z] — generalized
/// hypergeometric divided by `Π Γ(b_j)`.
pub fn builtin_hypergeometric_pfq_regularized(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("HypergeometricPFQRegularized", args, 3)?;
    let (Some(a), Some(b), Some(z)) = (
        f64_list(&args[0]),
        f64_list(&args[1]),
        super::to_f64(&args[2]),
    ) else {
        return Ok(unevaluated("HypergeometricPFQRegularized", args));
    };
    let mut denom = 1.0_f64;
    for &bj in &b {
        denom *= gamma_fn(bj);
    }
    let base = if a.len() == 1 && b.len() == 1 {
        confluent_1f1(a[0], b[0], z)
    } else {
        hyper_pfq(&a, &b, z)
    };
    let v = base / denom;
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("HypergeometricPFQRegularized", args))
    }
}

// ── Meijer G ────────────────────────────────────────────────────────────────

/// `MeijerG[{{a_1..a_n}, {a_{n+1}..a_p}}, {{b_1..b_m}, {b_{m+1}..b_q}}, z]`.
///
/// Evaluated with the two standard Mellin–Barnes series expansions
/// (DLMF 16.17.1 and 16.17.2); each is a finite sum of hypergeometric terms.
#[allow(clippy::too_many_arguments)]
fn meijer_g(a_up: &[f64], a_low: &[f64], b_up: &[f64], b_low: &[f64], z: f64) -> Option<f64> {
    let n = a_up.len();
    let m = b_up.len();
    let p = n + a_low.len();
    let q = m + b_low.len();
    if p == 0 && q == 0 {
        return Some(1.0);
    }
    if p == 0 {
        // G^{m,0}_{0,q}: only the b-series can apply.
        return exp_series_16_17_2(a_up, a_low, b_up, b_low, z);
    }
    if m == 0 {
        return a_series_16_17_1(a_up, a_low, b_up, b_low, z);
    }
    if p < q || (p == q && z.abs() < 1.0) {
        exp_series_16_17_2(a_up, a_low, b_up, b_low, z)
    } else {
        a_series_16_17_1(a_up, a_low, b_up, b_low, z)
    }
}

/// DLMF 16.17.2: sum over the `m` upper b-parameters.
fn exp_series_16_17_2(
    a_up: &[f64],
    a_low: &[f64],
    b_up: &[f64],
    b_low: &[f64],
    z: f64,
) -> Option<f64> {
    let n = a_up.len();
    let m = b_up.len();
    let a: Vec<f64> = a_up.iter().chain(a_low).copied().collect();
    let p = a.len();
    let q = m + b_low.len();
    let mut total = 0.0_f64;
    for k in 0..m {
        let bk = b_up[k];
        let mut coeff = 1.0_f64;
        for (j, &bj) in b_up.iter().enumerate() {
            if j != k {
                coeff *= gamma_fn(bj - bk);
            }
        }
        for &aj in a_up.iter().take(n) {
            coeff *= gamma_fn(1.0 - aj + bk);
        }
        for &bj in b_low.iter() {
            coeff /= gamma_fn(1.0 - bj + bk);
        }
        for &aj in a_low.iter() {
            coeff /= gamma_fn(aj - bk);
        }
        let num: Vec<f64> = a.iter().map(|&aj| 1.0 - aj + bk).collect();
        let den: Vec<f64> = b_up
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != k)
            .map(|(_, &bj)| 1.0 - bj + bk)
            .chain(b_low.iter().map(|&bj| 1.0 - bj + bk))
            .collect();
        let sign = if (p as i64 - m as i64 - n as i64) % 2 == 0 {
            1.0
        } else {
            -1.0
        };
        let h = hyper_pfq(&num, &den, sign * z);
        total += coeff * z.powf(bk) * h;
    }
    let _ = q;
    if total.is_finite() { Some(total) } else { None }
}

/// DLMF 16.17.1: sum over the `n` upper a-parameters.
fn a_series_16_17_1(
    a_up: &[f64],
    a_low: &[f64],
    b_up: &[f64],
    b_low: &[f64],
    z: f64,
) -> Option<f64> {
    let a: Vec<f64> = a_up.iter().chain(a_low).copied().collect();
    let n = a_up.len();
    let m = b_up.len();
    let p = a.len();
    let q = m + b_low.len();
    let mut total = 0.0_f64;
    for k in 0..n {
        let ak = a_up[k];
        let mut coeff = 1.0_f64;
        for (j, &aj) in a_up.iter().enumerate() {
            if j != k {
                coeff *= gamma_fn(aj - ak);
            }
        }
        for &bj in b_up.iter() {
            coeff *= gamma_fn(1.0 - ak + bj);
        }
        for &aj in a_low.iter() {
            coeff /= gamma_fn(ak - aj);
        }
        for &bj in b_low.iter() {
            coeff /= gamma_fn(1.0 + ak - bj);
        }
        let num: Vec<f64> = b_up.iter().chain(b_low).map(|&bj| 1.0 - ak + bj).collect();
        let den: Vec<f64> = a_up
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != k)
            .map(|(_, &aj)| 1.0 - ak + aj)
            .chain(a_low.iter().map(|&aj| 1.0 - ak + aj))
            .collect();
        let sign = if (q as i64 - m as i64 - n as i64) % 2 == 0 {
            1.0
        } else {
            -1.0
        };
        let h = hyper_pfq(&num, &den, sign / z);
        total += coeff * z.powf(ak - 1.0) * h;
    }
    let _ = p;
    if total.is_finite() { Some(total) } else { None }
}

/// Exact integer value helper.
fn int_value(n: i64) -> Value {
    Value::Integer(rug::Integer::from(n))
}

/// `Power[base, exp]` with simplifier.
fn pow_value(base: Value, exp: Value) -> Value {
    sc("Power", &[base, exp])
}

/// `-v`.
fn neg_value(v: &Value) -> Value {
    sc("Times", &[int_value(-1), v.clone()])
}

/// `Plus[..]` with simplifier.
fn plus_value(terms: &[Value]) -> Value {
    sc("Plus", terms)
}

/// `Times[..]` with simplifier.
fn times_value(factors: &[Value]) -> Value {
    sc("Times", factors)
}

/// `1 + z`.
fn one_plus_z(z: &Value) -> Value {
    plus_value(&[int_value(1), z.clone()])
}

/// `I_ν(x)` for real `ν`, `x ≥ 0`, by the ascending series.
fn bessel_i_real(nu: f64, x: f64) -> f64 {
    if x == 0.0 {
        return if nu == 0.0 {
            1.0
        } else if nu > 0.0 {
            0.0
        } else {
            f64::INFINITY
        };
    }
    let h = x / 2.0;
    h.powf(nu) / gamma_fn(nu + 1.0) * hyper_pfq(&[], &[nu + 1.0], h * h)
}

/// `K_1(x)` for `x > 0` via the DLMF 10.31.1 series.
fn bessel_k1_real(x: f64) -> f64 {
    let h = x / 2.0;
    // I_1(x) = (x/2) Σ (x²/4)^k / (k! (k+1)!).
    let mut it = h;
    let mut i1 = h;
    let mut k = 1u32;
    loop {
        it *= (h * h) / (k as f64 * (k as f64 + 1.0));
        i1 += it;
        k += 1;
        if it.abs() < 1e-21 * i1.abs().max(1.0) || k > 800 {
            break;
        }
    }
    // Σ (x²/4)^k [ψ(k+1)+ψ(k+2)] / (k! (k+1)!).
    let mut term = 1.0_f64;
    let mut s = 0.0_f64;
    let mut m = 0u32;
    loop {
        let bracket = digamma_fn(m as f64 + 1.0) + digamma_fn(m as f64 + 2.0);
        s += term * bracket;
        m += 1;
        term *= (h * h) / (m as f64 * (m as f64 + 1.0));
        if (term * bracket).abs() < 1e-21 * s.abs().max(1e-300) || m > 800 {
            break;
        }
    }
    1.0 / x + h.ln() * i1 - (x / 4.0) * s
}

/// `K_ν(x)` for real `ν` and `x > 0`.
fn bessel_k_real(nu: f64, x: f64) -> f64 {
    let ni = nu.round();
    if (nu - ni).abs() < 1e-12 {
        let n = ni.abs() as u32;
        if n == 0 {
            return bessel_k0_c(Cx::new(x, 0.0)).re;
        }
        if n == 1 {
            return bessel_k1_real(x);
        }
        let mut km1 = bessel_k0_c(Cx::new(x, 0.0)).re;
        let mut kk = bessel_k1_real(x);
        for j in 1..n {
            let kp = km1 + (2.0 * j as f64 / x) * kk;
            km1 = kk;
            kk = kp;
        }
        return kk;
    }
    (std::f64::consts::PI / 2.0) / (std::f64::consts::PI * nu).sin()
        * (bessel_i_real(-nu, x) - bessel_i_real(nu, x))
}

/// Upper incomplete gamma `Γ(a, z) = ∫_z^∞ t^{a-1} e^{-t} dt` for `a ≥ 0`,
/// `z > 0`.
fn incomplete_gamma_fn(a: f64, z: f64) -> f64 {
    if a == 0.0 {
        return exp_integral_e1(z);
    }
    gamma_fn(a) - z.powf(a) / a * confluent_1f1(a, a + 1.0, -z)
}

/// Recognised small closed forms of the Meijer G-function. Handles the
/// degenerate parameter configurations (coincident parameters) that the raw
/// DLMF series mishandles.
fn meijer_g_closed(
    a_up: &[f64],
    a_low: &[f64],
    b_up: &[f64],
    b_low: &[f64],
    z: f64,
) -> Option<f64> {
    // G^{2,0}_{1,2}(z | 1; 0, a) = Γ(a, z) — incomplete gamma.
    if a_up.is_empty()
        && a_low.len() == 1
        && a_low[0] == 1.0
        && b_low.is_empty()
        && b_up.len() == 2
        && b_up[0] == 0.0
        && z > 0.0
    {
        let v = incomplete_gamma_fn(b_up[1], z);
        if v.is_finite() {
            return Some(v);
        }
    }
    // G^{1,1}_{1,1}(z | a; b) = Γ(1-a+b) z^b (1+z)^{a-1-b}.
    if a_up.len() == 1 && a_low.is_empty() && b_up.len() == 1 && b_low.is_empty() {
        let (a, b) = (a_up[0], b_up[0]);
        let s = 1.0 - a + b;
        let v = gamma_fn(s) * z.powf(b) * (1.0 + z).powf(a - 1.0 - b);
        if v.is_finite() {
            return Some(v);
        }
        // Γ has a pole when `1-a+b` is a non-positive integer; the value
        // (and its parameter limit) is then infinite.
        if s <= 0.0 && (s - s.round()).abs() < 1e-12 {
            return Some(f64::INFINITY);
        }
    }
    // G^{1,2}_{2,2}(z | a1, a2; b, c) in terms of a 2F1.
    if a_up.len() == 2 && a_low.is_empty() && b_up.len() == 1 && b_low.len() == 1 {
        let (a1, a2, b, c) = (a_up[0], a_up[1], b_up[0], b_low[0]);
        let v = gamma_fn(1.0 + b - a1) * gamma_fn(1.0 + b - a2) / gamma_fn(1.0 + b - c)
            * z.powf(b)
            * hyper_2f1(1.0 + b - a1, 1.0 + b - a2, 1.0 + b - c, -z);
        if v.is_finite() {
            return Some(v);
        }
    }
    // Exponentials / Gaussians: G^{1,0}_{0,1}(z|b) = z^b e^{-z}.
    if a_up.is_empty() && a_low.is_empty() && b_low.is_empty() && b_up.len() == 1 {
        let v = z.powf(b_up[0]) * (-z).exp();
        if v.is_finite() {
            return Some(v);
        }
    }
    // Bessel K: G^{2,0}_{0,2}(z|b1,b2) = 2 z^{(b1+b2)/2} K_{b1-b2}(2 Sqrt[z]).
    if a_up.is_empty() && a_low.is_empty() && b_low.is_empty() && b_up.len() == 2 && z > 0.0 {
        let (b1, b2) = (b_up[0], b_up[1]);
        let v = 2.0 * z.powf((b1 + b2) / 2.0) * bessel_k_real(b1 - b2, 2.0 * z.sqrt());
        if v.is_finite() {
            return Some(v);
        }
    }
    // Bessel J: G^{1,0}_{0,2}(z|b1;b2) = Γ(1+b1-b2) z^{(b1+b2)/2} J_{b1-b2}(2 Sqrt[z]).
    if a_up.is_empty() && a_low.is_empty() && b_up.len() == 1 && b_low.len() == 1 && z > 0.0 {
        let (b1, b2) = (b_up[0], b_low[0]);
        let v = gamma_fn(1.0 + b1 - b2)
            * z.powf((b1 + b2) / 2.0)
            * bessel_j_real(b1 - b2, 2.0 * z.sqrt());
        if v.is_finite() {
            return Some(v);
        }
    }
    None
}

/// Mellin–Barnes quadrature fallback (DLMF 16.17.1, case (i)). Used only for
/// parameter configurations the series cannot resolve (coincident parameters)
/// and which satisfy the straight-contour convergence condition.
fn meijer_g_quadrature(
    a_up: &[f64],
    a_low: &[f64],
    b_up: &[f64],
    b_low: &[f64],
    z: f64,
) -> Option<f64> {
    let n = a_up.len();
    let m = b_up.len();
    let p = n + a_low.len();
    let q = m + b_low.len();
    if m + n == 0 {
        return Some(1.0);
    }
    if p + q >= 2 * (m + n) {
        return None;
    }
    let delta = (2 * (m + n) - p - q) as f64;
    let amax = a_up.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let bmin = b_up.iter().cloned().fold(f64::INFINITY, f64::min);
    let mut g0 = if n == 0 {
        bmin - 0.5
    } else if m == 0 {
        amax - 1.5
    } else {
        (amax - 1.0 + bmin) / 2.0
    };
    // Avoid landing exactly on a pole (resonant/coincident configuration).
    let on_pole = |g: f64| {
        b_up.iter().any(|&b| (b - g).abs() < 1e-9)
            || a_up.iter().any(|&a| (1.0 - a + g).abs() < 1e-9)
    };
    if on_pole(g0) {
        g0 -= 0.1;
    }
    let lnz = z.abs().ln();
    let zang = if z < 0.0 { std::f64::consts::PI } else { 0.0 };
    let integrand = |t: f64| -> Cx {
        let mut val = Cx::new(1.0, 0.0);
        for &b in b_up {
            val = val.mul(cloggamma(Cx::new(b - g0, -t)).exp());
        }
        for &a in a_up {
            val = val.mul(cloggamma(Cx::new(1.0 - a + g0, t)).exp());
        }
        for &b in b_low {
            val = val.div(cloggamma(Cx::new(1.0 - b + g0, t)).exp());
        }
        for &a in a_low {
            val = val.div(cloggamma(Cx::new(a - g0, -t)).exp());
        }
        let re = lnz * g0 - t * zang;
        let im = lnz * t + g0 * zang;
        val.mul(Cx::new(re, im).exp())
    };
    let tmax = (40.0 / delta).max(15.0);
    let h = 0.02_f64;
    // Simpson's rule needs an even number of intervals.
    let mut steps = (2.0 * tmax / h).ceil() as usize;
    if steps % 2 == 1 {
        steps += 1;
    }
    let mut acc = 0.0_f64;
    for i in 0..=steps {
        let t = -tmax + h * i as f64;
        let w = if i == 0 || i == steps {
            1.0
        } else if i % 2 == 1 {
            4.0
        } else {
            2.0
        };
        acc += w * integrand(t).re;
    }
    let g = acc * h / 3.0 / (2.0 * std::f64::consts::PI);
    if g.is_finite() { Some(g) } else { None }
}

/// Symbolic counterpart of [`meijer_g_closed`] for the recognised small
/// patterns, preserving exact parameters and the free variable `z`.
fn meijer_g_symbolic(
    a_up: &[Value],
    a_low: &[Value],
    b_up: &[Value],
    b_low: &[Value],
    z: &Value,
) -> Option<Value> {
    if a_up.is_empty() && a_low.is_empty() {
        // G^{1,0}_{0,1}(z|b) = z^b E^{-z}.
        if b_low.is_empty() && b_up.len() == 1 {
            return Some(times_value(&[
                pow_value(z.clone(), b_up[0].clone()),
                sc("Exp", &[neg_value(z)]),
            ]));
        }
        // G^{2,0}_{0,2}(z|b1,b2) = 2 z^{(b1+b2)/2} K_{b1-b2}(2 Sqrt[z]).
        if b_low.is_empty() && b_up.len() == 2 {
            let ord = plus_value(&[b_up[0].clone(), neg_value(&b_up[1])]);
            let half = times_value(&[
                rational(1, 2),
                plus_value(&[b_up[0].clone(), b_up[1].clone()]),
            ]);
            let arg = times_value(&[int_value(2), sqrt_value(z)]);
            return Some(times_value(&[
                int_value(2),
                pow_value(z.clone(), half),
                Value::Call {
                    head: "BesselK".to_string(),
                    args: vec![ord, arg],
                },
            ]));
        }
        // G^{1,0}_{0,2}(z|b;c) = Γ(1+b-c) z^{(b+c)/2} J_{b-c}(2 Sqrt[z]).
        if b_up.len() == 1 && b_low.len() == 1 {
            let ord = plus_value(&[b_up[0].clone(), neg_value(&b_low[0])]);
            let g = sc("Gamma", &[plus_value(&[int_value(1), ord.clone()])]);
            let half = times_value(&[
                rational(1, 2),
                plus_value(&[b_up[0].clone(), b_low[0].clone()]),
            ]);
            let arg = times_value(&[int_value(2), sqrt_value(z)]);
            return Some(times_value(&[
                g,
                pow_value(z.clone(), half),
                Value::Call {
                    head: "BesselJ".to_string(),
                    args: vec![ord, arg],
                },
            ]));
        }
    }
    // G^{2,0}_{1,2}(z | 1; 0, a) = Γ(a, z) = Γ(a) (1 - P(a, z)), where the
    // codebase's `GammaRegularized` is the lower regularized P.
    if a_up.is_empty()
        && a_low.len() == 1
        && is_int(&a_low[0], 1)
        && b_low.is_empty()
        && b_up.len() == 2
        && is_int(&b_up[0], 0)
    {
        let a = b_up[1].clone();
        let reg = Value::Call {
            head: "GammaRegularized".to_string(),
            args: vec![a.clone(), z.clone()],
        };
        let one_minus = plus_value(&[int_value(1), neg_value(&reg)]);
        return Some(times_value(&[sc("Gamma", &[a]), one_minus]));
    }
    // G^{1,1}_{1,1}(z | a; b) = Γ(1-a+b) z^b (1+z)^{a-1-b}.
    if a_low.is_empty() && a_up.len() == 1 && b_up.len() == 1 && b_low.is_empty() {
        let (a, b) = (&a_up[0], &b_up[0]);
        let g = sc(
            "Gamma",
            &[plus_value(&[int_value(1), neg_value(a), b.clone()])],
        );
        let exp = plus_value(&[a.clone(), neg_value(b), int_value(-1)]);
        return Some(times_value(&[
            g,
            pow_value(z.clone(), b.clone()),
            pow_value(one_plus_z(z), exp),
        ]));
    }
    // G^{1,2}_{2,2}(z | a1, a2; b, c).
    if a_low.is_empty() && a_up.len() == 2 && b_up.len() == 1 && b_low.len() == 1 {
        let (a1, a2, b, c) = (&a_up[0], &a_up[1], &b_up[0], &b_low[0]);
        let g = times_value(&[
            sc(
                "Gamma",
                &[plus_value(&[int_value(1), b.clone(), neg_value(a1)])],
            ),
            sc(
                "Gamma",
                &[plus_value(&[int_value(1), b.clone(), neg_value(a2)])],
            ),
            sc(
                "Power",
                &[
                    sc(
                        "Gamma",
                        &[plus_value(&[int_value(1), b.clone(), neg_value(c)])],
                    ),
                    int_value(-1),
                ],
            ),
        ]);
        let h = Value::Call {
            head: "Hypergeometric2F1".to_string(),
            args: vec![
                plus_value(&[int_value(1), b.clone(), neg_value(a1)]),
                plus_value(&[int_value(1), b.clone(), neg_value(a2)]),
                plus_value(&[int_value(1), b.clone(), neg_value(c)]),
                neg_value(z),
            ],
        };
        return Some(times_value(&[g, pow_value(z.clone(), b.clone()), h]));
    }
    None
}

/// MeijerG[{{...}, {...}}, {{...}, {...}}, z].
pub fn builtin_meijer_g(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("MeijerG", args, 3)?;
    let (Some(a_up_v), Some(b_up_v)) = (two_value_lists(&args[0]), two_value_lists(&args[1]))
    else {
        return Ok(unevaluated("MeijerG", args));
    };
    let (a_up_c, a_low_c) = a_up_v;
    let (b_up_c, b_low_c) = b_up_v;
    #[allow(clippy::type_complexity)]
    let numeric: Option<(Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, f64)> = (|| {
        Some((
            vals_to_f64(&a_up_c)?,
            vals_to_f64(&a_low_c)?,
            vals_to_f64(&b_up_c)?,
            vals_to_f64(&b_low_c)?,
            super::to_f64(&args[2])?,
        ))
    })();
    if let Some((a_up, a_low, b_up, b_low, z)) = numeric {
        // Recognised closed forms are exact and may legitimately be infinite
        // (parameter pole), so return them even when not finite.
        if let Some(v) = meijer_g_closed(&a_up, &a_low, &b_up, &b_low, z) {
            return Ok(super::real(v));
        }
        let v = meijer_g(&a_up, &a_low, &b_up, &b_low, z)
            .or_else(|| meijer_g_quadrature(&a_up, &a_low, &b_up, &b_low, z));
        if let Some(v) = v
            && v.is_finite()
        {
            return Ok(super::real(v));
        }
        // Numeric parameters but no finite value: fall through to the exact
        // symbolic table only if some parameter is symbolic, otherwise stay
        // unevaluated so the numeric convention is preserved.
    }
    if let Some(v) = meijer_g_symbolic(&a_up_c, &a_low_c, &b_up_c, &b_low_c, &args[2]) {
        return Ok(v);
    }
    Ok(unevaluated("MeijerG", args))
}

// ── Zeta family ─────────────────────────────────────────────────────────────

const BERNOULLI_ALL: [f64; 10] = [
    1.0 / 6.0,
    -1.0 / 30.0,
    1.0 / 42.0,
    -1.0 / 30.0,
    5.0 / 66.0,
    -691.0 / 2730.0,
    7.0 / 6.0,
    -3617.0 / 510.0,
    43867.0 / 798.0,
    -174611.0 / 330.0,
];

/// Hurwitz zeta `ζ(s, a) = Σ_{k≥0} (a+k)^{-s}` via Euler–Maclaurin.
fn hurwitz_zeta(s: f64, a: f64) -> f64 {
    let n = 12usize;
    let mut sum = 0.0_f64;
    for k in 0..n {
        sum += (a + k as f64).powf(-s);
    }
    let an = a + n as f64;
    sum += an.powf(1.0 - s) / (s - 1.0);
    sum += 0.5 * an.powf(-s);
    let mut rising_term = s; // (s)_{2r-1}
    let mut fact = 2.0_f64;
    let mut pow = an.powf(-s - 1.0);
    for (r0, &b) in BERNOULLI_ALL.iter().enumerate() {
        sum += b / fact * rising_term * pow;
        let rr = (r0 + 1) as f64;
        rising_term *= (s + 2.0 * rr - 1.0) * (s + 2.0 * rr);
        fact *= (2.0 * rr + 1.0) * (2.0 * rr + 2.0);
        pow /= an * an;
    }
    sum
}

/// Alternating Dirichlet eta `η(s) = Σ (-1)^{n-1}/n^s` for `s > 0`.
fn dirichlet_eta(s: f64) -> f64 {
    let mut sum = 0.0_f64;
    for n in 1..=10_000_000u64 {
        let sign = if n % 2 == 1 { 1.0 } else { -1.0 };
        let term = sign / (n as f64).powf(s);
        sum += term;
        if term.abs() < 1e-17 {
            break;
        }
    }
    sum
}

/// HurwitzZeta[s, a].
pub fn builtin_hurwitz_zeta(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("HurwitzZeta", args, 2)?;
    let prec = crate::value::DEFAULT_PRECISION;
    let (Some(s), Some(a)) = (float_val(&args[0], prec), float_val(&args[1], prec)) else {
        return Ok(unevaluated("HurwitzZeta", args));
    };
    if a <= 0 {
        return Ok(unevaluated("HurwitzZeta", args));
    }
    let one = Float::with_val(prec, 1u32);
    if (Float::with_val(prec, &s - &one)).abs() < Float::with_val(prec, 1e-30) {
        return Ok(unevaluated("HurwitzZeta", args));
    }
    let v = super::specialfunctions::hurwitz_zeta_em(&s, &a);
    if v.is_finite() {
        Ok(Value::Real(v))
    } else {
        Ok(unevaluated("HurwitzZeta", args))
    }
}

/// LerchPhi[z, s, a] = `Σ_{k≥0} z^k / (k+a)^s`.
pub fn builtin_lerch_phi(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("LerchPhi", args, 3)?;
    let (Some(z), Some(s), Some(a)) = (
        super::to_f64(&args[0]),
        super::to_f64(&args[1]),
        super::to_f64(&args[2]),
    ) else {
        return Ok(unevaluated("LerchPhi", args));
    };
    if a <= 0.0 {
        return Ok(unevaluated("LerchPhi", args));
    }
    if z == 1.0 {
        // LerchPhi[1, s, a] = HurwitzZeta[s, a].
        let v = hurwitz_zeta(s, a);
        return if v.is_finite() {
            Ok(super::real(v))
        } else {
            Ok(unevaluated("LerchPhi", args))
        };
    }
    if z.abs() > 1.0 {
        return Ok(unevaluated("LerchPhi", args));
    }
    if z.abs() == 1.0 && s <= 0.0 {
        return Ok(unevaluated("LerchPhi", args));
    }
    let mut sum = 0.0_f64;
    let mut z_pow = 1.0_f64;
    for k in 0..=50_000_000u64 {
        let term = z_pow / (k as f64 + a).powf(s);
        sum += term;
        z_pow *= z;
        if term.abs() < 1e-17 || z_pow.abs() < 1e-18 {
            break;
        }
    }
    if sum.is_finite() {
        Ok(super::real(sum))
    } else {
        Ok(unevaluated("LerchPhi", args))
    }
}

/// DirichletLambda[s] = `(1 - 2^{-s}) ζ(s) = Σ_{k≥0} (2k+1)^{-s}`.
pub fn builtin_dirichlet_lambda(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("DirichletLambda", args, 1)?;
    let Some(s) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("DirichletLambda", args));
    };
    if s <= 0.0 {
        return Ok(unevaluated("DirichletLambda", args));
    }
    if (s - 1.0).abs() < 1e-14 {
        return Ok(super::real(std::f64::consts::LN_2));
    }
    let v = if s > 1.5 {
        // Directly sum the odd terms for fast convergence.
        let mut sum = 0.0_f64;
        for k in 0..50_000_000u64 {
            let term = ((2 * k + 1) as f64).powf(-s);
            sum += term;
            if term < 1e-17 {
                break;
            }
        }
        sum
    } else {
        // λ(s) = (1 - 2^{-s}) η(s) / (1 - 2^{1-s}) for 0 < s < ∞, s ≠ 1.
        (1.0 - 2f64.powf(-s)) * dirichlet_eta(s) / (1.0 - 2f64.powf(1.0 - s))
    };
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("DirichletLambda", args))
    }
}

/// RiemannSiegelTheta[t].
pub fn builtin_riemann_siegel_theta(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("RiemannSiegelTheta", args, 1)?;
    let Some(t) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("RiemannSiegelTheta", args));
    };
    if t == 0.0 {
        return Ok(Value::Integer(rug::Integer::from(0)));
    }
    Ok(super::real(riemann_siegel_theta(t)))
}

/// RiemannSiegelZ[t].
pub fn builtin_riemann_siegel_z(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("RiemannSiegelZ", args, 1)?;
    let Some(t) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("RiemannSiegelZ", args));
    };
    let v = riemann_siegel_z(t);
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("RiemannSiegelZ", args))
    }
}

// ── Exponential / logarithmic integrals ─────────────────────────────────────

/// ExpIntegralE[n, z] — `E_n(z) = ∫_1^∞ e^{-z t} t^{-n} dt` for `z > 0`.
pub fn builtin_exp_integral_e(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("ExpIntegralE", args, 2)?;
    let (Some(n), Some(z)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("ExpIntegralE", args));
    };
    if z <= 0.0 || n < 0.0 {
        return Ok(unevaluated("ExpIntegralE", args));
    }
    if n == 0.0 {
        // E_0(z) = e^{-z}/z.
        return Ok(super::real((-z).exp() / z));
    }
    // Integer order via stable upward recurrence from E_1.
    let n_int = n.round();
    if (n - n_int).abs() > 1e-12 {
        return Ok(unevaluated("ExpIntegralE", args));
    }
    let mut e = exp_integral_e1(z);
    let mut k = 1.0_f64;
    while k < n_int {
        e = ((-z).exp() - z * e) / k;
        k += 1.0;
    }
    if e.is_finite() {
        Ok(super::real(e))
    } else {
        Ok(unevaluated("ExpIntegralE", args))
    }
}

/// ExpIntegralEi[z] — exponential integral `Ei(z)`.
pub fn builtin_exp_integral_ei(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("ExpIntegralEi", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("ExpIntegralEi", args));
    };
    if z == 0.0 {
        return Ok(super::real(f64::NEG_INFINITY));
    }
    // γ + ln|z| + Σ_{k≥1} z^k/(k·k!)  — valid for z > 0 and z < 0.
    let mut sum = 0.0_f64;
    let mut term = 1.0_f64;
    for k in 1..=500u32 {
        term *= z / k as f64;
        sum += term / k as f64;
        if term.abs() < 1e-20 * sum.abs().max(1.0) {
            break;
        }
    }
    let v = EULER_GAMMA + z.abs().ln() + sum;
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("ExpIntegralEi", args))
    }
}

/// LogIntegral[z] — logarithmic integral `li(z) = Ei(ln z)` for `z > 0`.
pub fn builtin_log_integral(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("LogIntegral", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("LogIntegral", args));
    };
    if z <= 0.0 {
        return Ok(unevaluated("LogIntegral", args));
    }
    if z == 1.0 {
        return Ok(super::real(f64::NEG_INFINITY));
    }
    let lz = z.ln();
    let mut sum = 0.0_f64;
    let mut term = 1.0_f64;
    for k in 1..=500u32 {
        term *= lz / k as f64;
        sum += term / k as f64;
        if term.abs() < 1e-20 * sum.abs().max(1.0) {
            break;
        }
    }
    let v = EULER_GAMMA + lz.abs().ln() + sum;
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("LogIntegral", args))
    }
}

// ── Trigonometric integrals ─────────────────────────────────────────────────

/// SinIntegral[z] = `∫_0^z sin(t)/t dt`.
pub fn builtin_sin_integral(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("SinIntegral", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("SinIntegral", args));
    };
    let mut sum = 0.0_f64;
    let mut term = z; // z^{2k+1}/(2k+1)! adjusted below
    let mut k = 0u32;
    loop {
        let denom = (2 * k + 1) as f64;
        sum += term / denom;
        let next_term = -term * z * z / ((2 * k + 2) as f64 * (2 * k + 3) as f64);
        term = next_term;
        k += 1;
        if term.abs() < 1e-18 * sum.abs().max(1.0) || k > 500 {
            break;
        }
    }
    Ok(super::real(sum))
}

/// CosIntegral[z] = `γ + ln|z| + ∫_0^z (cos t - 1)/t dt`.
pub fn builtin_cos_integral(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("CosIntegral", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("CosIntegral", args));
    };
    if z == 0.0 {
        return Ok(super::real(f64::NEG_INFINITY));
    }
    let mut sum = 0.0_f64;
    let mut term = 1.0_f64; // becomes (-1)^k z^{2k}/(2k)!
    let mut k = 1u32;
    loop {
        let d = (2 * k) as f64;
        term *= -z * z / (((2 * k - 1) as f64) * d);
        sum += term / d;
        k += 1;
        if term.abs() < 1e-18 * sum.abs().max(1.0) || k > 500 {
            break;
        }
    }
    let v = EULER_GAMMA + z.abs().ln() + sum;
    Ok(super::real(v))
}

/// SinhIntegral[z] = `∫_0^z sinh(t)/t dt`.
pub fn builtin_sinh_integral(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("SinhIntegral", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("SinhIntegral", args));
    };
    let mut sum = 0.0_f64;
    let mut term = z;
    let mut k = 0u32;
    loop {
        let denom = (2 * k + 1) as f64;
        sum += term / denom;
        term = term * z * z / ((2 * k + 2) as f64 * (2 * k + 3) as f64);
        k += 1;
        if term.abs() < 1e-18 * sum.abs().max(1.0) || k > 500 {
            break;
        }
    }
    Ok(super::real(sum))
}

/// CoshIntegral[z] = `γ + ln|z| + ∫_0^z (cosh t - 1)/t dt`.
pub fn builtin_cosh_integral(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("CoshIntegral", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("CoshIntegral", args));
    };
    if z == 0.0 {
        return Ok(super::real(f64::NEG_INFINITY));
    }
    let mut sum = 0.0_f64;
    let mut term = 1.0_f64; // becomes z^{2k}/(2k)!
    let mut k = 1u32;
    loop {
        let d = (2 * k) as f64;
        term *= z * z / (((2 * k - 1) as f64) * d);
        sum += term / d;
        k += 1;
        if term.abs() < 1e-18 * sum.abs().max(1.0) || k > 500 {
            break;
        }
    }
    let v = EULER_GAMMA + z.abs().ln() + sum;
    Ok(super::real(v))
}

// ── Fresnel integrals ───────────────────────────────────────────────────────

/// `(S(z), C(z))`: convergent series for `|z| < 4.2` (in full precision, so the
/// alternating cancellation is harmless) and the optimal-truncation asymptotic
/// beyond, where the series blows up.
fn fresnel_sc(z: f64) -> (f64, f64) {
    if z == 0.0 {
        return (0.0, 0.0);
    }
    let prec = crate::value::DEFAULT_PRECISION;
    let az = z.abs();
    let (s, c) = if az < 4.2 {
        fresnel_series(az, prec)
    } else {
        fresnel_asymptotic(az, prec)
    };
    if z < 0.0 { (-s, -c) } else { (s, c) }
}

/// Series `S(z) = z Σ (-1)^m c_{2m+1}`, `C(z) = z Σ (-1)^m c_{2m}` with
/// `c_n = b^n / (n! (2n+1))`, `b = π z² / 2`.
fn fresnel_series(az: f64, prec: u32) -> (f64, f64) {
    let pi = Float::with_val(prec, rug::float::Constant::Pi);
    let b = Float::with_val(prec, &pi) / 2u32 * (az * az);
    let a = Float::with_val(prec, az);
    let mut c = Float::with_val(prec, 1u32);
    let mut sum_s = Float::with_val(prec, 0u32);
    let mut sum_c = c.clone();
    let mut sign_s: i32 = 1; // n = 1 term of S is +
    let mut sign_c: i32 = -1; // n = 2 term of C is -
    let mut n: u32 = 1;
    loop {
        c *= &b;
        c *= Float::with_val(prec, 2 * n - 1);
        c /= Float::with_val(prec, n) * Float::with_val(prec, 2 * n + 1);
        if n % 2 == 1 {
            let term = if sign_s < 0 { -c.clone() } else { c.clone() };
            sum_s += term;
            sign_s = -sign_s;
        } else {
            let term = if sign_c < 0 { -c.clone() } else { c.clone() };
            sum_c += term;
            sign_c = -sign_c;
        }
        if c < Float::with_val(prec, 1e-45) || n > 5000 {
            break;
        }
        n += 1;
    }
    ((a.clone() * sum_s).to_f64(), (a * sum_c).to_f64())
}

/// Large-argument asymptotic (DLMF 7.5.8) with optimal truncation.
fn fresnel_asymptotic(az: f64, prec: u32) -> (f64, f64) {
    let pi = Float::with_val(prec, rug::float::Constant::Pi);
    let a = Float::with_val(prec, az);
    let t = Float::with_val(prec, &pi) * (az * az);
    let theta = Float::with_val(prec, &t) / 2u32;
    let mut d = Float::with_val(prec, 1u32); // d_0
    let mut sum_f = Float::with_val(prec, 1u32);
    let mut sum_g = Float::with_val(prec, 0u32);
    let mut sign_f: i32 = -1; // first even (k = 2) term is -
    let mut sign_g: i32 = 1; // first odd (k = 1) term is +
    let mut k: u32 = 1;
    loop {
        let prev = d.clone();
        d *= Float::with_val(prec, 2 * k - 1);
        d /= &t;
        if k.is_multiple_of(2) {
            let term = if sign_f < 0 { -d.clone() } else { d.clone() };
            sum_f += term;
            sign_f = -sign_f;
        } else {
            let term = if sign_g < 0 { -d.clone() } else { d.clone() };
            sum_g += term;
            sign_g = -sign_g;
        }
        if d.clone().abs() > prev.abs() && k > 2 {
            break;
        }
        k += 1;
        if k > 500 {
            break;
        }
    }
    let pis = Float::with_val(prec, &pi) * &a;
    let f = sum_f / &pis;
    let g = sum_g / &pis;
    let (st, ct) = (theta.clone().sin(), theta.cos());
    let s = Float::with_val(prec, 0.5)
        - Float::with_val(prec, &f * &ct)
        - Float::with_val(prec, &g * &st);
    let c = Float::with_val(prec, 0.5) + Float::with_val(prec, &f * &st)
        - Float::with_val(prec, &g * &ct);
    (s.to_f64(), c.to_f64())
}

/// FresnelS[z] = `∫_0^z sin(π t²/2) dt`.
pub fn builtin_fresnel_s(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("FresnelS", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("FresnelS", args));
    };
    let (s, _) = fresnel_sc(z);
    Ok(super::real(s))
}

/// FresnelC[z] = `∫_0^z cos(π t²/2) dt`.
pub fn builtin_fresnel_c(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("FresnelC", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("FresnelC", args));
    };
    let (_, c) = fresnel_sc(z);
    Ok(super::real(c))
}

/// DawsonF[z] = `e^{-z²} ∫_0^z e^{t²} dt`. Computed as
/// `Sqrt[Pi]/2 e^{-z²} Erfi[z]`, using the high-precision `erfi` kernel.
pub fn builtin_dawson_f(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("DawsonF", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("DawsonF", args));
    };
    let prec = crate::value::DEFAULT_PRECISION;
    let x = Float::with_val(prec, z);
    let erfi = super::specialfunctions::erfi_float(&x);
    let half_sqrt_pi = Float::with_val(prec, rug::float::Constant::Pi).sqrt() / 2u32;
    let x2 = Float::with_val(prec, &x * &x);
    let result = half_sqrt_pi * (-x2).exp() * erfi;
    Ok(super::real(result.to_f64()))
}

// ── Struve functions ────────────────────────────────────────────────────────

fn struve(n: f64, z: f64, modified: bool) -> f64 {
    let mut sum = 0.0_f64;
    // term_k = (z/2)^{2k+n+1} / (Γ(k+3/2) Γ(k+n+3/2))
    let mut k = 0u32;
    loop {
        let numer = (z / 2.0).powf(2.0 * k as f64 + n + 1.0);
        let denom = gamma_fn(k as f64 + 1.5) * gamma_fn(k as f64 + n + 1.5);
        let mut term = numer / denom;
        if !modified && k % 2 == 1 {
            term = -term;
        }
        sum += term;
        k += 1;
        let next_numer = (z / 2.0).powf(2.0 * k as f64 + n + 1.0);
        let next_denom = gamma_fn(k as f64 + 1.5) * gamma_fn(k as f64 + n + 1.5);
        if (next_numer / next_denom).abs() < 1e-18 * sum.abs().max(1.0) || k > 500 {
            break;
        }
    }
    sum
}

/// StruveH[n, z].
pub fn builtin_struve_h(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("StruveH", args, 2)?;
    let (Some(n), Some(z)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("StruveH", args));
    };
    let v = struve(n, z, false);
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("StruveH", args))
    }
}

/// StruveL[n, z].
pub fn builtin_struve_l(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("StruveL", args, 2)?;
    let (Some(n), Some(z)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("StruveL", args));
    };
    let v = struve(n, z, true);
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("StruveL", args))
    }
}

// ── Kelvin functions ────────────────────────────────────────────────────────

/// `I_0(w)` for complex `w` (power series).
fn bessel_i0_c(w: Cx) -> Cx {
    let hw2 = w.scale(0.5).mul(w.scale(0.5));
    let mut term = Cx::new(1.0, 0.0);
    let mut sum = term;
    for k in 1..=300u32 {
        term = term.mul(hw2).div(Cx::new((k * k) as f64, 0.0));
        sum = sum.add(term);
        if term.abs() < 1e-18 * sum.abs().max(1.0) {
            break;
        }
    }
    sum
}

/// `K_0(w)` for complex `w` via `-(ln(w/2)+γ) I_0(w) + Σ (w/2)^{2k} H_k/(k!)²`.
fn bessel_k0_c(w: Cx) -> Cx {
    let hw = w.scale(0.5);
    let hw2 = hw.mul(hw);
    let mut term = Cx::new(1.0, 0.0);
    let mut i0 = term;
    let mut harmonic = Cx::zero();
    let mut h = 0.0_f64;
    for k in 1..=300u32 {
        term = term.mul(hw2).div(Cx::new((k * k) as f64, 0.0));
        i0 = i0.add(term);
        h += 1.0 / k as f64;
        harmonic = harmonic.add(term.scale(h));
        if term.abs() < 1e-18 * i0.abs().max(1.0) {
            break;
        }
    }
    let log_part = hw.ln().add(Cx::new(EULER_GAMMA, 0.0));
    log_part.scale(-1.0).mul(i0).add(harmonic)
}

fn kelvin_parts(z: f64) -> (Cx, Cx) {
    let j_arg = Cx::new(z, 0.0).mul(Cx::new(
        (-3.0 * std::f64::consts::PI / 4.0).cos(),
        (-3.0 * std::f64::consts::PI / 4.0).sin(),
    ));
    let k_arg = Cx::new(z, 0.0).mul(Cx::new(
        (std::f64::consts::PI / 4.0).cos(),
        (std::f64::consts::PI / 4.0).sin(),
    ));
    (bessel_i0_c(j_arg), bessel_k0_c(k_arg))
}

/// `ber(z) = Re J_0(z e^{3πi/4})`.
pub fn builtin_kelvin_ber(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("KelvinBer", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("KelvinBer", args));
    };
    Ok(super::real(kelvin_parts(z).0.re))
}

/// `bei(z) = Im J_0(z e^{3πi/4})`.
pub fn builtin_kelvin_bei(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("KelvinBei", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("KelvinBei", args));
    };
    Ok(super::real(kelvin_parts(z).0.im))
}

/// `ker(z) = Re K_0(z e^{iπ/4})`.
pub fn builtin_kelvin_ker(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("KelvinKer", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("KelvinKer", args));
    };
    if z == 0.0 {
        return Ok(super::real(f64::INFINITY));
    }
    Ok(super::real(kelvin_parts(z).1.re))
}

/// `kei(z) = Im K_0(z e^{iπ/4})`.
pub fn builtin_kelvin_kei(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("KelvinKei", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("KelvinKei", args));
    };
    Ok(super::real(kelvin_parts(z).1.im))
}

// ── Appell F1 ───────────────────────────────────────────────────────────────

/// AppellF1[a, b1, b2, c, x, y] — Appell's first hypergeometric function.
pub fn builtin_appell_f1(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("AppellF1", args, 6)?;
    let vals: Option<Vec<f64>> = args.iter().map(super::to_f64).collect::<Option<Vec<_>>>();
    let Some(v) = vals else {
        return Ok(unevaluated("AppellF1", args));
    };
    let (a, b1, b2, c, x, y) = (v[0], v[1], v[2], v[3], v[4], v[5]);
    if x == 0.0 && y == 0.0 {
        return Ok(Value::Integer(rug::Integer::from(1)));
    }
    if c == 0.0 {
        return Ok(unevaluated("AppellF1", args));
    }
    // Double series built with recurrences to avoid factorial overflow:
    //   F1 = Σ_{m,n≥0} (a)_{m+n}(b1)_m(b2)_n / ((c)_{m+n} m! n!) x^m y^n.
    let mut sum = 0.0_f64;
    let mut term_m0 = 1.0_f64; // term(m, 0)
    for m in 0..=300u32 {
        let mut t = term_m0;
        sum += t;
        for n in 1..=300u32 {
            t *= (a + (m + n - 1) as f64) * (b2 + (n - 1) as f64)
                / ((c + (m + n - 1) as f64) * n as f64)
                * y;
            sum += t;
            if t.abs() < 1e-18 * sum.abs().max(1.0) {
                break;
            }
        }
        term_m0 *= (a + m as f64) * (b1 + m as f64) / ((c + m as f64) * (m + 1) as f64) * x;
        if term_m0.abs() < 1e-18 * sum.abs().max(1.0) && m > 3 {
            break;
        }
    }
    if sum.is_finite() {
        Ok(super::real(sum))
    } else {
        Ok(unevaluated("AppellF1", args))
    }
}

// ── q-Pochhammer ────────────────────────────────────────────────────────────

/// QPochhammer[a, q] = `(a; q)_∞`; QPochhammer[a, q, n] = `(a; q)_n`.
pub fn builtin_qpochhammer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 1 {
        return Ok(unevaluated("QPochhammer", args));
    }
    if args.len() != 2 && args.len() != 3 {
        return Err(EvalError::Error(
            "QPochhammer requires 2 or 3 arguments".to_string(),
        ));
    }
    let (Some(a), Some(q)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("QPochhammer", args));
    };
    if args.len() == 3 {
        let Some(n) = super::to_f64(&args[2]) else {
            return Ok(unevaluated("QPochhammer", args));
        };
        let n = n.round() as u32;
        let mut prod = 1.0_f64;
        for k in 0..n {
            prod *= 1.0 - a * q.powi(k as i32);
        }
        return Ok(super::real(prod));
    }
    if q.abs() >= 1.0 {
        return Ok(unevaluated("QPochhammer", args));
    }
    let mut prod = 1.0_f64;
    let mut q_pow = 1.0_f64;
    for _ in 0..100_000u32 {
        let factor = 1.0 - a * q_pow;
        prod *= factor;
        q_pow *= q;
        if (a * q_pow).abs() < 1e-20 {
            break;
        }
    }
    if prod.is_finite() {
        Ok(super::real(prod))
    } else {
        Ok(unevaluated("QPochhammer", args))
    }
}

// ── Bessel Y / Hankel functions ─────────────────────────────────────────────

/// `J_n(z)` for real order `n` and real `z` via the ascending series, with the
/// integer-parity reflection `J_{-n} = (-1)^n J_n` and `J_n(-z) = (-1)^n J_n`.
fn bessel_j_real(n: f64, z: f64) -> f64 {
    if n < 0.0 && (n - n.round()).abs() < 1e-12 {
        let m = -n.round();
        let sign = if (m as i64) % 2 == 0 { 1.0 } else { -1.0 };
        return sign * bessel_j_real(m, z);
    }
    if z == 0.0 {
        return if n == 0.0 {
            1.0
        } else if n > 0.0 {
            0.0
        } else {
            f64::INFINITY
        };
    }
    if z < 0.0 && (n - n.round()).abs() < 1e-12 {
        let sign = if (n.round() as i64) % 2 == 0 {
            1.0
        } else {
            -1.0
        };
        return sign * bessel_j_real(n, -z);
    }
    let half = z / 2.0;
    let mut term = half.powf(n) / gamma_fn(n + 1.0);
    let mut sum = term;
    let mut k = 1u32;
    while k <= 1000 {
        term *= -(half * half) / (k as f64 * (n + k as f64));
        sum += term;
        if term.abs() < 1e-19 * sum.abs().max(1e-300) {
            break;
        }
        k += 1;
    }
    sum
}

/// `Y_n(z)` for non-negative integer `n` via DLMF 10.8.2, valid for `z > 0`.
fn bessel_y_integer(n: u32, z: f64) -> f64 {
    let nf = n as f64;
    let half = z / 2.0;
    let jn = bessel_j_real(nf, z);
    let mut finite = 0.0_f64;
    for k in 0..n {
        finite += factorial_small(n - k - 1) / factorial_small(k) * half.powf(2.0 * k as f64 - nf);
    }
    let mut term = half.powi(n as i32) / factorial_small(n);
    let mut acc = 0.0_f64;
    let mut k = 0u32;
    while k <= 1000 {
        let bracket = digamma_fn(k as f64 + 1.0) + digamma_fn(nf + k as f64 + 1.0);
        let t = term * bracket;
        acc += t;
        term *= -(half * half) / ((k + 1) as f64 * (nf + k as f64 + 1.0));
        k += 1;
        if t.abs() < 1e-19 * acc.abs().max(1e-300) {
            break;
        }
    }
    (2.0 / std::f64::consts::PI) * half.ln() * jn
        - finite / std::f64::consts::PI
        - acc / std::f64::consts::PI
}

/// `Y_n(z)` for real order `n` and `z > 0`: integer orders use the explicit
/// series, other orders the limit formula `(J_n cos nπ − J_{-n}) / sin nπ`.
fn bessel_y_real(n: f64, z: f64) -> f64 {
    if n < 0.0 && (n - n.round()).abs() < 1e-12 {
        let m = -n.round();
        let sign = if (m as i64) % 2 == 0 { 1.0 } else { -1.0 };
        return sign * bessel_y_real(m, z);
    }
    if z < 0.0 && (n - n.round()).abs() < 1e-12 {
        let sign = if (n.round() as i64) % 2 == 0 {
            1.0
        } else {
            -1.0
        };
        return sign * bessel_y_real(n, -z);
    }
    let ni = n.round();
    if (n - ni).abs() < 1e-10 && ni >= 0.0 {
        return bessel_y_integer(ni as u32, z);
    }
    (bessel_j_real(n, z) * (n * std::f64::consts::PI).cos() - bessel_j_real(-n, z))
        / (n * std::f64::consts::PI).sin()
}

/// Build a `Value::Complex`, collapsing a zero imaginary part to a real.
fn complex(re: f64, im: f64) -> Value {
    if im == 0.0 {
        super::real(re)
    } else {
        Value::Complex { re, im }
    }
}

/// BesselH1[n, z] = HankelH1[n, z] = `J_n(z) + i Y_n(z)`.
pub fn builtin_bessel_h1(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("BesselH1", args, 2)?;
    let (Some(n), Some(z)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("BesselH1", args));
    };
    let (j, y) = (bessel_j_real(n, z), bessel_y_real(n, z));
    if j.is_finite() && y.is_finite() {
        Ok(complex(j, y))
    } else {
        Ok(unevaluated("BesselH1", args))
    }
}

/// BesselH2[n, z] = HankelH2[n, z] = `J_n(z) - i Y_n(z)`.
pub fn builtin_bessel_h2(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("BesselH2", args, 2)?;
    let (Some(n), Some(z)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("BesselH2", args));
    };
    let (j, y) = (bessel_j_real(n, z), bessel_y_real(n, z));
    if j.is_finite() && y.is_finite() {
        Ok(complex(j, -y))
    } else {
        Ok(unevaluated("BesselH2", args))
    }
}

// ── Chebyshev polynomials ───────────────────────────────────────────────────

/// `T_n(x)` for integer `n` via the recurrence `T_{n+1} = 2x T_n − T_{n-1}`.
fn chebyshev_t_int(n: i64, x: f64) -> f64 {
    let n = n.unsigned_abs();
    match n {
        0 => 1.0,
        1 => x,
        _ => {
            let (mut a, mut b) = (1.0_f64, x);
            for _ in 2..=n {
                let c = 2.0 * x * b - a;
                a = b;
                b = c;
            }
            b
        }
    }
}

/// `U_n(x)` for integer `n` via the recurrence `U_{n+1} = 2x U_n − U_{n-1}`.
fn chebyshev_u_int(n: i64, x: f64) -> f64 {
    match n {
        -1 => 0.0,
        n if n < 0 => -chebyshev_u_int(-n - 2, x),
        0 => 1.0,
        1 => 2.0 * x,
        _ => {
            let (mut a, mut b) = (1.0_f64, 2.0 * x);
            for _ in 2..=n {
                let c = 2.0 * x * b - a;
                a = b;
                b = c;
            }
            b
        }
    }
}

/// `T_n(x)` for real order `n` and `|x| ≤ 1`: `cos(n arccos x)`.
fn chebyshev_t_real(n: f64, x: f64) -> f64 {
    if x.abs() <= 1.0 {
        (n * x.acos()).cos()
    } else {
        let t = x.abs().acosh();
        let v = (n * t).cosh();
        if x < 0.0 && (n - n.round()).abs() < 1e-12 && (n.round() as i64) % 2 != 0 {
            -v
        } else {
            v
        }
    }
}

/// `U_n(x)` for real order `n` and `|x| ≤ 1`: `sin((n+1) arccos x)/sin(arccos x)`.
fn chebyshev_u_real(n: f64, x: f64) -> f64 {
    if x.abs() < 1.0 {
        let th = x.acos();
        ((n + 1.0) * th).sin() / th.sin()
    } else {
        let t = x.abs().acosh();
        let v = ((n + 1.0) * t).sinh() / t.sinh();
        if x < 0.0 && (n - n.round()).abs() < 1e-12 && (n.round() as i64) % 2 != 0 {
            -v
        } else {
            v
        }
    }
}

/// ChebyshevT[n, x] — Chebyshev polynomial of the first kind.
pub fn builtin_chebyshev_t(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("ChebyshevT", args, 2)?;
    let (Some(n), Some(x)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("ChebyshevT", args));
    };
    let ni = n.round();
    let v = if (n - ni).abs() < 1e-12 {
        chebyshev_t_int(ni as i64, x)
    } else {
        chebyshev_t_real(n, x)
    };
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("ChebyshevT", args))
    }
}

/// ChebyshevU[n, x] — Chebyshev polynomial of the second kind.
pub fn builtin_chebyshev_u(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("ChebyshevU", args, 2)?;
    let (Some(n), Some(x)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("ChebyshevU", args));
    };
    let ni = n.round();
    let v = if (n - ni).abs() < 1e-12 {
        chebyshev_u_int(ni as i64, x)
    } else {
        chebyshev_u_real(n, x)
    };
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("ChebyshevU", args))
    }
}

// ── Gegenbauer polynomials ──────────────────────────────────────────────────

/// `C_n^a(x)` via `(n+1) C_{n+1} = 2(n+a) x C_n − (n+2a−1) C_{n-1}`.
fn gegenbauer_c(n: i64, a: f64, x: f64) -> f64 {
    let n = n.unsigned_abs();
    match n {
        0 => 1.0,
        1 => 2.0 * a * x,
        _ => {
            let (mut p0, mut p1) = (1.0_f64, 2.0 * a * x);
            for k in 1..n {
                let kf = k as f64;
                let p2 = (2.0 * (kf + a) * x * p1 - (kf + 2.0 * a - 1.0) * p0) / (kf + 1.0);
                p0 = p1;
                p1 = p2;
            }
            p1
        }
    }
}

/// GegenbauerC[n, a, x] — Gegenbauer (ultraspherical) polynomial.
pub fn builtin_gegenbauer_c(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("GegenbauerC", args, 3)?;
    let vals: Option<Vec<f64>> = args.iter().map(super::to_f64).collect::<Option<Vec<_>>>();
    let Some(v) = vals else {
        return Ok(unevaluated("GegenbauerC", args));
    };
    let (n, a, x) = (v[0], v[1], v[2]);
    if (n - n.round()).abs() > 1e-12 {
        return Ok(unevaluated("GegenbauerC", args));
    }
    let val = gegenbauer_c(n.round() as i64, a, x);
    if val.is_finite() {
        Ok(super::real(val))
    } else {
        Ok(unevaluated("GegenbauerC", args))
    }
}

/// GegenbauerCP[n, a, x] = `d/dx C_n^a(x) = 2a C_{n-1}^{a+1}(x)`.
pub fn builtin_gegenbauer_cp(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("GegenbauerCP", args, 3)?;
    let vals: Option<Vec<f64>> = args.iter().map(super::to_f64).collect::<Option<Vec<_>>>();
    let Some(v) = vals else {
        return Ok(unevaluated("GegenbauerCP", args));
    };
    let (n, a, x) = (v[0], v[1], v[2]);
    if (n - n.round()).abs() > 1e-12 {
        return Ok(unevaluated("GegenbauerCP", args));
    }
    let val = 2.0 * a * gegenbauer_c(n.round() as i64 - 1, a + 1.0, x);
    if val.is_finite() {
        Ok(super::real(val))
    } else {
        Ok(unevaluated("GegenbauerCP", args))
    }
}

// ── Jacobi polynomials ──────────────────────────────────────────────────────

/// `P_n^{(a,b)}(x)` via the three-term recurrence (DLMF 18.9.2).
fn jacobi_p(n: i64, a: f64, b: f64, x: f64) -> f64 {
    let n = n.unsigned_abs();
    if n == 0 {
        return 1.0;
    }
    let p1 = 0.5 * ((a - b) + (a + b + 2.0) * x);
    if n == 1 {
        return p1;
    }
    let (mut p0, mut p1) = (1.0_f64, p1);
    for k in 1..n {
        let kf = k as f64;
        let s = 2.0 * kf + a + b;
        let p2 = ((s + 1.0) * (s * (s + 2.0) * x + a * a - b * b) * p1
            - 2.0 * (kf + a) * (kf + b) * (s + 2.0) * p0)
            / (2.0 * (kf + 1.0) * (kf + a + b + 1.0) * s);
        p0 = p1;
        p1 = p2;
    }
    p1
}

/// JacobiP[n, a, b, x] — Jacobi polynomial.
pub fn builtin_jacobi_p(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("JacobiP", args, 4)?;
    let vals: Option<Vec<f64>> = args.iter().map(super::to_f64).collect::<Option<Vec<_>>>();
    let Some(v) = vals else {
        return Ok(unevaluated("JacobiP", args));
    };
    let (n, a, b, x) = (v[0], v[1], v[2], v[3]);
    if (n - n.round()).abs() > 1e-12 {
        return Ok(unevaluated("JacobiP", args));
    }
    let val = jacobi_p(n.round() as i64, a, b, x);
    if val.is_finite() {
        Ok(super::real(val))
    } else {
        Ok(unevaluated("JacobiP", args))
    }
}

// ── Spherical harmonics ─────────────────────────────────────────────────────

/// Associated Legendre function `P_l^m(x)` (Condon–Shortley phase), `0 ≤ m ≤ l`.
fn assoc_legendre(l: i64, m: i64, x: f64) -> f64 {
    let mut pmm = 1.0_f64;
    if m > 0 {
        let somx2 = ((1.0 - x) * (1.0 + x)).sqrt();
        let mut fact = 1.0_f64;
        for _ in 1..=m {
            pmm *= -fact * somx2;
            fact += 2.0;
        }
    }
    if l == m {
        return pmm;
    }
    let mut pmmp1 = x * (2 * m + 1) as f64 * pmm;
    if l == m + 1 {
        return pmmp1;
    }
    let mut pll = 0.0_f64;
    for ll in (m + 2)..=l {
        pll = ((2 * ll - 1) as f64 * x * pmmp1 - (ll + m - 1) as f64 * pmm) / (ll - m) as f64;
        pmm = pmmp1;
        pmmp1 = pll;
    }
    pll
}

/// SphericalHarmonicY[l, m, theta, phi] — spherical harmonic `Y_l^m(θ, φ)`.
pub fn builtin_spherical_harmonic_y(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("SphericalHarmonicY", args, 4)?;
    let vals: Option<Vec<f64>> = args.iter().map(super::to_f64).collect::<Option<Vec<_>>>();
    let Some(v) = vals else {
        return Ok(unevaluated("SphericalHarmonicY", args));
    };
    let (l, m, theta, phi) = (v[0], v[1], v[2], v[3]);
    if (l - l.round()).abs() > 1e-12 || (m - m.round()).abs() > 1e-12 {
        return Ok(unevaluated("SphericalHarmonicY", args));
    }
    let (li, mi) = (l.round() as i64, m.round() as i64);
    if li < 0 || mi.abs() > li {
        return Ok(unevaluated("SphericalHarmonicY", args));
    }
    let ma = mi.abs();
    let p = assoc_legendre(li, ma, theta.cos());
    let fact_ratio = factorial_small((li - ma) as u32) / factorial_small((li + ma) as u32);
    let norm = ((2 * li + 1) as f64 / (4.0 * std::f64::consts::PI) * fact_ratio).sqrt();
    let mut re = norm * p * (mi as f64 * phi).cos();
    let mut im = norm * p * (mi as f64 * phi).sin();
    if mi < 0 {
        let sign = if ma % 2 == 0 { 1.0 } else { -1.0 };
        re *= sign;
        im *= sign;
    }
    if re.is_finite() && im.is_finite() {
        Ok(complex(re, im))
    } else {
        Ok(unevaluated("SphericalHarmonicY", args))
    }
}

// ── Airy function derivatives ───────────────────────────────────────────────

/// `Ai'(z)` (`bi = false`) and `Bi'(z)` (`bi = true`) via the two power
/// series of DLMF 9.4.1/9.4.2 differentiated term by term.
fn airy_prime(z: f64, bi: bool) -> f64 {
    let c0 = 1.0 / (3f64.powf(2.0 / 3.0) * gamma_fn(2.0 / 3.0));
    let c1 = 1.0 / (3f64.powf(1.0 / 3.0) * gamma_fn(1.0 / 3.0));
    let mut a = 1.0_f64;
    let mut f = 1.0_f64;
    let mut fp = 0.0_f64;
    let mut b = 1.0_f64;
    let mut g = z;
    let mut gp = 1.0_f64;
    let mut k = 0u32;
    loop {
        k += 1;
        let kf = k as f64;
        a *= 3.0 * (1.0 / 3.0 + kf - 1.0) / ((3.0 * kf - 2.0) * (3.0 * kf - 1.0) * (3.0 * kf));
        f += a * z.powf(3.0 * kf);
        fp += 3.0 * kf * a * z.powf(3.0 * kf - 1.0);
        b *= 3.0 * (2.0 / 3.0 + kf - 1.0) / ((3.0 * kf - 1.0) * (3.0 * kf) * (3.0 * kf + 1.0));
        g += b * z.powf(3.0 * kf + 1.0);
        gp += (3.0 * kf + 1.0) * b * z.powf(3.0 * kf);
        let fa = (a * z.powf(3.0 * kf)).abs();
        let gb = (b * z.powf(3.0 * kf + 1.0)).abs();
        if (fa < 1e-22 * f.abs().max(1.0) && gb < 1e-22 * g.abs().max(1.0)) || k > 2000 {
            break;
        }
    }
    if bi {
        3f64.sqrt() * (c0 * fp + c1 * gp)
    } else {
        c0 * fp - c1 * gp
    }
}

/// AiryAiPrime[z] — derivative of the Airy function `Ai`.
pub fn builtin_airy_ai_prime(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("AiryAiPrime", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("AiryAiPrime", args));
    };
    let v = airy_prime(z, false);
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("AiryAiPrime", args))
    }
}

/// AiryBiPrime[z] — derivative of the Airy function `Bi`.
pub fn builtin_airy_bi_prime(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("AiryBiPrime", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("AiryBiPrime", args));
    };
    let v = airy_prime(z, true);
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("AiryBiPrime", args))
    }
}

// ── Symmetric eigen-decomposition (cyclic Jacobi) ───────────────────────────

/// Cyclic Jacobi eigen-decomposition of a real symmetric matrix. Returns
/// `(eigenvalues, eigenvectors)` where `eigenvectors[i]` is a unit-norm
/// eigenvector for `eigenvalues[i]`.
///
/// Follows the Numerical Recipes `jacobi` scheme (threshold skipping plus an
/// accumulated diagonal correction `z`) so that large matrices converge
/// accurately.
#[allow(clippy::needless_range_loop)]
fn jacobi_eigen_sym(mat: &[Vec<f64>]) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = mat.len();
    let mut a = mat.to_vec();
    for i in 0..n {
        for j in i..n {
            a[i][j] = mat[i][j];
            a[j][i] = mat[i][j];
        }
    }
    let mut v: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            let mut row = vec![0.0; n];
            row[i] = 1.0;
            row
        })
        .collect();
    if n <= 1 {
        return ((0..n).map(|i| a[i][i]).collect(), v);
    }
    let rotate = |a: &mut [Vec<f64>], i: usize, j: usize, k: usize, l: usize, s: f64, tau: f64| {
        let g = a[i][j];
        let h = a[k][l];
        a[i][j] = g - s * (h + g * tau);
        a[k][l] = h + s * (g - h * tau);
    };
    for sweep in 0..300 {
        let mut sm = 0.0;
        for p in 0..n {
            for q in (p + 1)..n {
                sm += a[p][q] * a[p][q];
            }
        }
        if sm <= 1e-302 {
            break;
        }
        let thresh = if sweep < 3 {
            0.2 * sm / (n * n) as f64
        } else {
            0.0
        };
        for p in 0..n {
            for q in (p + 1)..n {
                let g = 100.0 * a[p][q].abs();
                if sweep > 3
                    && (a[p][p].abs() + g) == a[p][p].abs()
                    && (a[q][q].abs() + g) == a[q][q].abs()
                {
                    a[p][q] = 0.0;
                    continue;
                }
                if a[p][q].abs() <= thresh {
                    continue;
                }
                let h = a[q][q] - a[p][p];
                let mut t = if (h.abs() + g) == h.abs() {
                    a[p][q] / h
                } else {
                    let theta = 0.5 * h / a[p][q];
                    let mut tt = 1.0 / (theta.abs() + (1.0 + theta * theta).sqrt());
                    if theta < 0.0 {
                        tt = -tt;
                    }
                    tt
                };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = t * c;
                let tau = s / (1.0 + c);
                t *= a[p][q];
                a[p][p] -= t;
                a[q][q] += t;
                a[p][q] = 0.0;
                for j in 0..p {
                    rotate(&mut a, j, p, j, q, s, tau);
                }
                for j in (p + 1)..q {
                    rotate(&mut a, p, j, j, q, s, tau);
                }
                for j in (q + 1)..n {
                    rotate(&mut a, p, j, q, j, s, tau);
                }
                for j in 0..n {
                    let gv = v[j][p];
                    let hv = v[j][q];
                    v[j][p] = gv - s * (hv + gv * tau);
                    v[j][q] = hv + s * (gv - hv * tau);
                }
            }
        }
    }
    if n == 3 {
        eprintln!("A final = {a:?}");
        eprintln!("V = {v:?}");
    }
    let evals: Vec<f64> = (0..n).map(|i| a[i][i]).collect();
    let evecs: Vec<Vec<f64>> = (0..n).map(|j| (0..n).map(|i| v[i][j]).collect()).collect();
    (evals, evecs)
}

/// Gauss–Laguerre nodes and weights for `∫_0^∞ e^{-x} f(x) dx` via the
/// Golub–Welsch construction, using a symmetric tridiagonal Jacobi matrix.
fn gauss_laguerre(n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut mat = vec![vec![0.0; n]; n];
    for i in 0..n {
        mat[i][i] = (2 * i + 1) as f64;
        if i + 1 < n {
            mat[i][i + 1] = (i + 1) as f64;
            mat[i + 1][i] = (i + 1) as f64;
        }
    }
    let (evals, evecs) = jacobi_eigen_sym(&mat);
    let mut pairs: Vec<(f64, f64)> = evals
        .into_iter()
        .zip(evecs)
        .map(|(e, vec)| (e, vec[0] * vec[0]))
        .collect();
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    (
        pairs.iter().map(|p| p.0).collect(),
        pairs.iter().map(|p| p.1).collect(),
    )
}

// ── HypergeometricU ─────────────────────────────────────────────────────────

/// `U(a, b, z)` via the two-`_1F_1` connection formula (assumes `b` is not a
/// positive integer, so that neither `Γ(1-b)` nor `Γ(b-1)` is singular).
fn hyper_u_1f1(a: f64, b: f64, z: f64) -> f64 {
    let t1 = gamma_fn(1.0 - b) / gamma_fn(1.0 + a - b) * confluent_1f1(a, b, z);
    let t2 =
        gamma_fn(b - 1.0) / gamma_fn(a) * z.powf(1.0 - b) * confluent_1f1(1.0 + a - b, 2.0 - b, z);
    t1 + t2
}

/// `U(a, b, z)` by numerical integration, valid for `a > 0`, `z > 0`.
///
/// The weight is rescaled to `e^{-s}` (`t = s/z`) so the slowly decaying
/// `(1+t)^{b-a-1}` factor stays polynomial, avoiding the exponential
/// blow-up of a naive `e^{-(z-1)t}` integrand.
fn hyper_u_integral(a: f64, b: f64, z: f64) -> f64 {
    let (nodes, weights) = gauss_laguerre(100);
    let mut sum = 0.0_f64;
    for (s, w) in nodes.iter().zip(weights.iter()) {
        let t = s / z;
        sum += w * t.powf(a - 1.0) * (1.0 + t).powf(b - a - 1.0);
    }
    sum / (gamma_fn(a) * z)
}

/// HypergeometricU[a, b, z] — confluent hypergeometric function of the second
/// kind (Tricomi function). Falls back to `HypergeometricU[a, b, z]`.
pub fn builtin_hypergeometric_u(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("HypergeometricU", args, 3)?;
    let (Some(a), Some(b), Some(z)) = (
        super::to_f64(&args[0]),
        super::to_f64(&args[1]),
        super::to_f64(&args[2]),
    ) else {
        return Ok(unevaluated("HypergeometricU", args));
    };
    let b_is_int = (b - b.round()).abs() < 1e-12;
    // z == 0: U(a, b, 0) = Γ(1-b)/Γ(1+a-b) for Re(b) < 1.
    if z == 0.0 {
        if b < 1.0 && !b_is_int {
            let v = gamma_fn(1.0 - b) / gamma_fn(1.0 + a - b);
            if v.is_finite() {
                return Ok(super::real(v));
            }
        }
        return Ok(unevaluated("HypergeometricU", args));
    }
    let v = if a > 0.0 && z > 0.0 {
        hyper_u_integral(a, b, z)
    } else if !b_is_int {
        hyper_u_1f1(a, b, z)
    } else {
        // Integer b (and a ≤ 0 or z < 0): nudge b off the integer so the
        // connection formula is non-singular.
        hyper_u_1f1(a, b + 1e-9, z)
    };
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("HypergeometricU", args))
    }
}

// ── Mathieu characteristic values ───────────────────────────────────────────

/// Smallest eigenvalue of a symmetric tridiagonal matrix given by `diag` and
/// `off`. `which` selects the 0-based eigenvalue in ascending order.
fn tridiag_eigenvalue(diag: &[f64], off: &[f64], which: usize) -> Option<f64> {
    let n = diag.len();
    if which >= n {
        return None;
    }
    let mut mat = vec![vec![0.0; n]; n];
    for i in 0..n {
        mat[i][i] = diag[i];
        if i + 1 < n {
            mat[i][i + 1] = off[i];
            mat[i + 1][i] = off[i];
        }
    }
    let (mut evals, _) = jacobi_eigen_sym(&mat);
    evals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Some(evals[which])
}

/// Characteristic value of the Mathieu equation, computed as an eigenvalue of
/// the truncated Fourier system. `even` = `a_n(q)`, otherwise `b_n(q)`.
fn mathieu_char(even: bool, n: i64, q: f64) -> Option<f64> {
    if n == 0 {
        // a_0(q); b_0 is defined as a_0 in the common convention.
        let k = 40usize;
        let diag: Vec<f64> = (0..=k).map(|j| ((2 * j) as f64).powi(2)).collect();
        let mut off = vec![q; k];
        off[0] = std::f64::consts::SQRT_2 * q;
        return tridiag_eigenvalue(&diag, &off, 0);
    }
    // Enough terms that the truncation tail is negligible.
    let extra = 40 + (q.abs().sqrt() as usize) * 4;
    if n % 2 == 1 {
        let m = ((n - 1) / 2) as usize;
        let k = m + extra;
        let mut diag: Vec<f64> = (0..=k).map(|j| ((2 * j + 1) as f64).powi(2)).collect();
        if even {
            diag[0] += q;
        } else {
            diag[0] -= q;
        }
        let off = vec![-q; k];
        tridiag_eigenvalue(&diag, &off, m)
    } else if even {
        let m = (n / 2) as usize;
        let k = m + extra;
        let diag: Vec<f64> = (0..=k).map(|j| ((2 * j) as f64).powi(2)).collect();
        let mut off = vec![-q; k];
        off[0] = -std::f64::consts::SQRT_2 * q;
        tridiag_eigenvalue(&diag, &off, m)
    } else {
        let m = (n / 2) as usize;
        let k = m + extra;
        // Basis sin(2j z), j = 1..k.
        let diag: Vec<f64> = (1..=k).map(|j| ((2 * j) as f64).powi(2)).collect();
        let off = vec![-q; k - 1];
        tridiag_eigenvalue(&diag, &off, m - 1)
    }
}

/// MathieuCharacteristicA[r, q] — even Mathieu characteristic value.
pub fn builtin_mathieu_characteristic_a(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("MathieuCharacteristicA", args, 2)?;
    let (Some(r), Some(q)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("MathieuCharacteristicA", args));
    };
    if (r - r.round()).abs() > 1e-12 || r < 0.0 {
        return Ok(unevaluated("MathieuCharacteristicA", args));
    }
    let v = mathieu_char(true, r.round() as i64, q);
    match v {
        Some(v) if v.is_finite() => Ok(super::real(v)),
        _ => Ok(unevaluated("MathieuCharacteristicA", args)),
    }
}

/// MathieuCharacteristicB[r, q] — odd Mathieu characteristic value.
pub fn builtin_mathieu_characteristic_b(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("MathieuCharacteristicB", args, 2)?;
    let (Some(r), Some(q)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("MathieuCharacteristicB", args));
    };
    if (r - r.round()).abs() > 1e-12 || r < 0.0 {
        return Ok(unevaluated("MathieuCharacteristicB", args));
    }
    let n = r.round() as i64;
    let v = mathieu_char(false, n, q);
    match v {
        Some(v) if v.is_finite() => Ok(super::real(v)),
        _ => Ok(unevaluated("MathieuCharacteristicB", args)),
    }
}

// ── SpheroidalS1 ────────────────────────────────────────────────────────────

/// Norm squared of the associated Legendre function `P_l^m` on `[-1, 1]`:
/// `2/(2l+1) · (l+m)!/(l-m)!`.
fn assoc_legendre_norm2(l: i64, m: i64) -> f64 {
    2.0 / (2 * l + 1) as f64 * factorial_small((l + m) as u32) / factorial_small((l - m) as u32)
}

/// Angular prolate spheroidal function `S_{mn}(γ, η)` of the first kind, via
/// diagonalisation of the spheroidal operator in the associated Legendre basis.
fn spheroidal_s1(n: i64, m: i64, gamma: f64, eta: f64) -> f64 {
    if gamma == 0.0 {
        return assoc_legendre(n, m, eta);
    }
    let kmax = (n - m + 24) as usize;
    let size = kmax + 1;
    let mut mat = vec![vec![0.0; size]; size];
    // Diagonal: eigenvalue of the (unperturbed) angular operator.
    for (i, row) in mat.iter_mut().enumerate() {
        let l = (m + i as i64) as f64;
        row[i] = l * (l + 1.0);
    }
    // Off-diagonal ↦ c² η² in the *normalised* Legendre basis. Using
    // η P_l^m = a_l P_{l+1}^m + b_l P_{l-1}^m with
    // a_l = (l-m+1)/(2l+1), b_l = (l+m)/(2l+1).
    let c2 = gamma * gamma;
    for (i, row) in mat.iter_mut().enumerate() {
        let l = m + i as i64;
        let a_l = (l - m + 1) as f64 / (2 * l + 1) as f64;
        let b_l = (l + m) as f64 / (2 * l + 1) as f64;
        let a_lp1 = (l + 1 - m + 1) as f64 / (2 * (l + 1) + 1) as f64;
        let b_lp1 = (l + 1 + m) as f64 / (2 * (l + 1) + 1) as f64;
        let a_lm1 = (l - 1 - m + 1) as f64 / (2 * (l - 1) + 1) as f64;
        let b_lm1 = (l - 1 + m) as f64 / (2 * (l - 1) + 1) as f64;
        // η² P_l → c_{l+2} P_{l+2} + c_l P_l + c_{l-2} P_{l-2}
        let c2v = a_l * a_lp1;
        let c0v = a_l * b_lp1 + b_l * a_lm1;
        let cm2v = b_l * b_lm1;
        for (k, ck) in [(l + 2, c2v), (l, c0v), (l - 2, cm2v)] {
            if ck == 0.0 || k < m {
                continue;
            }
            let j = (k - m) as usize;
            if j >= size {
                continue;
            }
            // <P_k|η²|P_l>_norm = ck * ||P_k||/||P_l||
            let ratio = (assoc_legendre_norm2(k, m) / assoc_legendre_norm2(l, m)).sqrt();
            row[j] += c2 * ck * ratio;
        }
    }
    let (evals, evecs) = jacobi_eigen_sym(&mat);
    let mut order: Vec<usize> = (0..size).collect();
    order.sort_by(|&a, &b| evals[a].partial_cmp(&evals[b]).unwrap());
    let want = (n - m) as usize;
    if want >= size {
        return assoc_legendre(n, m, eta);
    }
    let idx = order[want];
    let vec = &evecs[idx];
    let pivot = vec[want];
    if pivot.abs() < 1e-14 {
        return assoc_legendre(n, m, eta);
    }
    let mut sum = 0.0_f64;
    for (i, &vi) in vec.iter().enumerate() {
        let l = m + i as i64;
        sum += (vi / pivot) * assoc_legendre(l, m, eta) / assoc_legendre_norm2(l, m).sqrt();
    }
    // Rescale so that the η-independent normalisation matches P_n^m at γ = 0.
    sum * assoc_legendre_norm2(n, m).sqrt()
}

/// SpheroidalS1[n, m, γ, η] — angular prolate spheroidal function of the first
/// kind. Falls back to `SpheroidalS1[n, m, γ, η]`.
pub fn builtin_spheroidal_s1(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("SpheroidalS1", args, 4)?;
    let (Some(n), Some(m), Some(g), Some(eta)) = (
        super::to_f64(&args[0]),
        super::to_f64(&args[1]),
        super::to_f64(&args[2]),
        super::to_f64(&args[3]),
    ) else {
        // For γ = 0 the function degenerates to the associated Legendre
        // function, which we can return symbolically.
        if let (Value::Integer(ni), Value::Integer(mi), Some(gv)) =
            (&args[0], &args[1], super::to_f64(&args[2]))
            && gv == 0.0
            && let (Some(ni), Some(mi)) = (ni.to_i64(), mi.to_i64())
            && ni >= 0
            && mi >= 0
            && mi <= ni
        {
            return Ok(Value::Call {
                head: "LegendreP".to_string(),
                args: vec![
                    Value::Integer(rug::Integer::from(ni)),
                    Value::Integer(rug::Integer::from(mi)),
                    args[3].clone(),
                ],
            });
        }
        return Ok(unevaluated("SpheroidalS1", args));
    };
    if (n - n.round()).abs() > 1e-12 || (m - m.round()).abs() > 1e-12 || n < 0.0 || m < 0.0 || m > n
    {
        return Ok(unevaluated("SpheroidalS1", args));
    }
    let v = spheroidal_s1(n.round() as i64, m.round() as i64, g, eta);
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("SpheroidalS1", args))
    }
}

// ── CoulombF ────────────────────────────────────────────────────────────────

/// `|Γ(1 + iη)| = sqrt(πη / sinh(πη))`.
fn gamma_abs_1_i_eta(eta: f64) -> f64 {
    if eta == 0.0 {
        return 1.0;
    }
    let x = std::f64::consts::PI * eta;
    (x / x.sinh()).abs().sqrt()
}

/// Regular Coulomb wave function `F_L(η, ρ)` via the Taylor series
/// (Abramowitz & Stegun 14.1).
fn coulomb_f(l: i64, eta: f64, rho: f64) -> f64 {
    let lf = l as f64;
    // Normalisation constant C_L(η).
    let mut norm = 2.0_f64.powi(l as i32)
        * (-(std::f64::consts::PI) * eta / 2.0).exp()
        * gamma_abs_1_i_eta(eta)
        / gamma_fn(2.0 * lf + 2.0);
    for j in 1..=l {
        let jf = j as f64;
        norm *= (jf * jf + eta * eta).sqrt();
    }
    // Series Σ a_k ρ^k with a_0 = 1, a_1 = η/(L+1) and
    // a_{k+1} = (2η k a_k - a_{k-1}) / ((k+1)(k+2L+2)).
    let mut prev = 1.0_f64; // a_0
    let mut cur = eta / (lf + 1.0); // a_1
    let mut sum = 1.0 + cur * rho; // includes a_0 ρ^0 and a_1 ρ^1
    let mut kk = 1.0_f64;
    loop {
        kk += 1.0;
        let next = (2.0 * eta * (kk - 1.0) * cur - prev) / (kk * (kk + 2.0 * lf + 1.0));
        let term = next * rho.powi(kk as i32);
        sum += term;
        prev = cur;
        cur = next;
        // Zero coefficients interleave for η = 0, so require the whole tail
        // (`prev`, `cur`, `next`) to be negligible before stopping.
        if (next.abs() < 1e-18 && cur.abs() < 1e-18 && prev.abs() < 1e-18) || kk > 500.0 {
            break;
        }
    }
    norm * rho.powf(lf + 1.0) * sum
}

/// CoulombF[l, η, ρ] — regular Coulomb wave function.
pub fn builtin_coulomb_f(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("CoulombF", args, 3)?;
    let (Some(l), Some(eta), Some(rho)) = (
        super::to_f64(&args[0]),
        super::to_f64(&args[1]),
        super::to_f64(&args[2]),
    ) else {
        return Ok(unevaluated("CoulombF", args));
    };
    if (l - l.round()).abs() > 1e-12 || l < 0.0 {
        return Ok(unevaluated("CoulombF", args));
    }
    let v = coulomb_f(l.round() as i64, eta, rho);
    if v.is_finite() {
        Ok(super::real(v))
    } else {
        Ok(unevaluated("CoulombF", args))
    }
}

// ── ZernikeR ────────────────────────────────────────────────────────────────

/// ZernikeR[n, m, ρ] — Zernike radial polynomial `R_n^m(ρ)`.
pub fn builtin_zernike_r(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("ZernikeR", args, 3)?;
    let ni = match &args[0] {
        Value::Integer(n) => n.to_i64().unwrap_or(-1),
        _ => return Ok(unevaluated("ZernikeR", args)),
    };
    let mi = match &args[1] {
        Value::Integer(m) => m.to_i64().unwrap_or(-1),
        _ => return Ok(unevaluated("ZernikeR", args)),
    };
    if ni < 0 || mi < 0 || mi > ni || (ni - mi) % 2 != 0 {
        return Ok(unevaluated("ZernikeR", args));
    }
    // Coefficients of R_n^m as an exact rational polynomial in ρ, indexed by
    // power of ρ.
    let half = (ni - mi) / 2;
    let mut coeffs: Vec<rug::Rational> = vec![rug::Rational::from(0); (ni + 1) as usize];
    for k in 0..=half {
        let num = factorial_rat(ni - k);
        let den =
            factorial_rat(k) * factorial_rat((ni + mi) / 2 - k) * factorial_rat((ni - mi) / 2 - k);
        let mut c = num / den;
        if k % 2 == 1 {
            c = -c;
        }
        coeffs[(ni - 2 * k) as usize] = c;
    }
    if let Some(rho) = super::to_f64(&args[2]) {
        // Numeric evaluation via Horner-free direct sum.
        let mut sum = 0.0_f64;
        for (p, c) in coeffs.iter().enumerate() {
            if !c.is_zero() {
                sum += c.to_f64() * rho.powi(p as i32);
            }
        }
        return Ok(super::real(sum));
    }
    // Symbolic: build a polynomial in the given variable.
    let var = args[2].clone();
    let mut terms: Vec<Value> = Vec::new();
    for (p, c) in coeffs.iter().enumerate() {
        if c.is_zero() {
            continue;
        }
        let rho_pow = if p == 0 {
            Value::Integer(rug::Integer::from(1))
        } else if p == 1 {
            var.clone()
        } else {
            Value::Call {
                head: "Power".to_string(),
                args: vec![var.clone(), Value::Integer(rug::Integer::from(p as i64))],
            }
        };
        let coeff = Value::Rational(Box::new(c.clone()));
        let term = if p == 0 {
            coeff
        } else {
            Value::Call {
                head: "Times".to_string(),
                args: vec![coeff, rho_pow],
            }
        };
        terms.push(term);
    }
    let result = if terms.is_empty() {
        Value::Integer(rug::Integer::from(0))
    } else if terms.len() == 1 {
        terms.pop().unwrap()
    } else {
        Value::Call {
            head: "Plus".to_string(),
            args: terms,
        }
    };
    // Reduce to canonical form using the evaluator's simplifier.
    Ok(simplify_plus_times(result))
}

/// Factorial of a non-negative integer as an exact rational.
fn factorial_rat(n: i64) -> rug::Rational {
    let mut p = rug::Integer::from(1);
    for k in 2..=n {
        p *= rug::Integer::from(k);
    }
    rug::Rational::from(p)
}

/// Best-effort canonicalisation of a `Plus`/`Times` polynomial so that numeric
/// coefficients collapse (e.g. `Times[-1, Power[ρ, 2]]` → `-ρ^2`).
fn simplify_plus_times(v: Value) -> Value {
    match v {
        Value::Call { head, args } if head == "Plus" => {
            let mapped: Vec<Value> = args.into_iter().map(simplify_plus_times).collect();
            Value::Call {
                head: "Plus".to_string(),
                args: mapped,
            }
        }
        Value::Call { head, args } if head == "Times" => {
            let mapped: Vec<Value> = args.into_iter().map(simplify_plus_times).collect();
            Value::Call {
                head: "Times".to_string(),
                args: mapped,
            }
        }
        other => other,
    }
}

// ── Bessel functions of general real order ──────────────────────────────────

/// `true` when `v` is an unevaluated call to `head`.
fn is_call(v: &Value, head: &str) -> bool {
    matches!(v, Value::Call { head: h, .. } if h == head)
}

/// BesselJ[n, z] — Bessel function of the first kind. Exact/symbolic
/// half-integer and integer cases defer to the sibling `specialfunctions`
/// module; general real orders evaluate numerically.
pub fn builtin_bessel_j(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("BesselJ", args, 2)?;
    let exact = super::specialfunctions::builtin_bessel_j(args)?;
    if !is_call(&exact, "BesselJ") {
        return Ok(exact);
    }
    let (Some(n), Some(z)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("BesselJ", args));
    };
    if z == 0.0 {
        return Ok(int_value(if n == 0.0 { 1 } else { 0 }));
    }
    let v = bessel_j_real(n, z);
    if v.is_nan() {
        Ok(unevaluated("BesselJ", args))
    } else {
        Ok(super::real(v))
    }
}

/// BesselI[n, z] — modified Bessel function of the first kind.
pub fn builtin_bessel_i(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("BesselI", args, 2)?;
    let exact = super::specialfunctions::builtin_bessel_i(args)?;
    if !is_call(&exact, "BesselI") {
        return Ok(exact);
    }
    let (Some(n), Some(z)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("BesselI", args));
    };
    if z == 0.0 {
        return Ok(int_value(if n == 0.0 { 1 } else { 0 }));
    }
    if z < 0.0 {
        return Ok(unevaluated("BesselI", args));
    }
    let v = bessel_i_real(n, z);
    if v.is_nan() {
        Ok(unevaluated("BesselI", args))
    } else {
        Ok(super::real(v))
    }
}

/// BesselY[n, z] — Bessel function of the second kind.
pub fn builtin_bessel_y(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("BesselY", args, 2)?;
    let (Some(n), Some(z)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("BesselY", args));
    };
    if z == 0.0 && n >= 0.0 {
        return Ok(super::real(f64::NEG_INFINITY));
    }
    let v = bessel_y_real(n, z);
    if v.is_nan() {
        Ok(unevaluated("BesselY", args))
    } else {
        Ok(super::real(v))
    }
}

/// BesselK[n, z] — modified Bessel function of the second kind.
pub fn builtin_bessel_k(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("BesselK", args, 2)?;
    let (Some(n), Some(z)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("BesselK", args));
    };
    if z == 0.0 && n >= 0.0 {
        return Ok(super::real(f64::INFINITY));
    }
    if z <= 0.0 {
        return Ok(unevaluated("BesselK", args));
    }
    let v = bessel_k_real(n, z);
    if v.is_nan() {
        Ok(unevaluated("BesselK", args))
    } else {
        Ok(super::real(v))
    }
}

/// BesselJZero[n, k] — k-th positive zero of `J_n`.
pub fn builtin_bessel_j_zero(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("BesselJZero", args, 2)?;
    let (Some(n), Some(k)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("BesselJZero", args));
    };
    if (n - n.round()).abs() > 1e-12 || n < 0.0 || (k - k.round()).abs() > 1e-12 || k < 1.0 {
        return Ok(unevaluated("BesselJZero", args));
    }
    match bessel_j_zero(n.round() as u32, k.round() as u32) {
        Some(v) => Ok(super::real(v)),
        None => Ok(unevaluated("BesselJZero", args)),
    }
}

/// Locate the `k`-th positive zero of `J_n` by scanning for sign changes and
/// then bisecting.
fn bessel_j_zero(n: u32, k: u32) -> Option<f64> {
    let nf = n as f64;
    let step = 0.25_f64;
    let mut prev = 1e-6_f64;
    let mut fx = bessel_j_real(nf, prev);
    let mut count = 0u32;
    let mut it = 0u32;
    while prev < 500.0 && it < 100_000 {
        it += 1;
        let next = prev + step;
        let fnx = bessel_j_real(nf, next);
        if fnx == 0.0 || (fx < 0.0) != (fnx < 0.0) {
            count += 1;
            if count == k {
                return Some(bisect_zero(nf, prev, next));
            }
        }
        fx = fnx;
        prev = next;
    }
    None
}

fn bisect_zero(n: f64, mut a: f64, mut b: f64) -> f64 {
    let mut fa = bessel_j_real(n, a);
    for _ in 0..200 {
        let mid = 0.5 * (a + b);
        let fm = bessel_j_real(n, mid);
        if fm == 0.0 {
            return mid;
        }
        if (fa < 0.0) != (fm < 0.0) {
            b = mid;
        } else {
            a = mid;
            fa = fm;
        }
    }
    0.5 * (a + b)
}

/// SphericalBesselJ[n, z] = Sqrt[Pi/(2 z)] BesselJ[n + 1/2, z].
pub fn builtin_spherical_bessel_j(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("SphericalBesselJ", args, 2)?;
    let (Some(n), Some(z)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("SphericalBesselJ", args));
    };
    if (n - n.round()).abs() > 1e-12 || n < 0.0 {
        return Ok(unevaluated("SphericalBesselJ", args));
    }
    let ni = n.round();
    if z == 0.0 {
        return Ok(int_value(if ni == 0.0 { 1 } else { 0 }));
    }
    if z < 0.0 {
        return Ok(unevaluated("SphericalBesselJ", args));
    }
    let v = (std::f64::consts::PI / (2.0 * z)).sqrt() * bessel_j_real(ni + 0.5, z);
    if v.is_nan() {
        Ok(unevaluated("SphericalBesselJ", args))
    } else {
        Ok(super::real(v))
    }
}

/// SphericalBesselY[n, z] = Sqrt[Pi/(2 z)] BesselY[n + 1/2, z].
pub fn builtin_spherical_bessel_y(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("SphericalBesselY", args, 2)?;
    let (Some(n), Some(z)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("SphericalBesselY", args));
    };
    if (n - n.round()).abs() > 1e-12 || n < 0.0 {
        return Ok(unevaluated("SphericalBesselY", args));
    }
    let ni = n.round();
    if z == 0.0 {
        return Ok(super::real(f64::NEG_INFINITY));
    }
    if z < 0.0 {
        return Ok(unevaluated("SphericalBesselY", args));
    }
    let v = (std::f64::consts::PI / (2.0 * z)).sqrt() * bessel_y_real(ni + 0.5, z);
    if v.is_nan() {
        Ok(unevaluated("SphericalBesselY", args))
    } else {
        Ok(super::real(v))
    }
}

// ── Hermite / Laguerre / Legendre polynomials ───────────────────────────────

/// `H_n(x)` (physicists' Hermite polynomial) via
/// `H_{n+1} = 2x H_n − 2n H_{n-1}`.
fn hermite_h(n: i64, x: f64) -> f64 {
    let n = n.unsigned_abs();
    match n {
        0 => 1.0,
        1 => 2.0 * x,
        _ => {
            let (mut h0, mut h1) = (1.0_f64, 2.0 * x);
            for k in 1..n {
                let h2 = 2.0 * x * h1 - 2.0 * k as f64 * h0;
                h0 = h1;
                h1 = h2;
            }
            h1
        }
    }
}

/// HermiteH[n, x].
pub fn builtin_hermite_h(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("HermiteH", args, 2)?;
    let (Some(n), Some(x)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("HermiteH", args));
    };
    if (n - n.round()).abs() > 1e-12 || n < 0.0 {
        return Ok(unevaluated("HermiteH", args));
    }
    Ok(super::real(hermite_h(n.round() as i64, x)))
}

/// `L_n^{(k)}(x)` via `(n+1) L_{n+1} = (2n+k+1−x) L_n − (n+k) L_{n-1}`.
fn laguerre_l(n: i64, k: f64, x: f64) -> f64 {
    let n = n.unsigned_abs();
    match n {
        0 => 1.0,
        1 => 1.0 + k - x,
        _ => {
            let (mut l0, mut l1) = (1.0_f64, 1.0 + k - x);
            for i in 1..n {
                let f = i as f64;
                let l2 = ((2.0 * f + k + 1.0 - x) * l1 - (f + k) * l0) / (f + 1.0);
                l0 = l1;
                l1 = l2;
            }
            l1
        }
    }
}

/// LaguerreL[n, x] and LaguerreL[n, k, x].
pub fn builtin_laguerre_l(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 && args.len() != 3 {
        return Err(EvalError::Error(
            "LaguerreL requires 2 or 3 arguments".to_string(),
        ));
    }
    let Some(n) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("LaguerreL", args));
    };
    let k = if args.len() == 3 {
        let Some(k) = super::to_f64(&args[1]) else {
            return Ok(unevaluated("LaguerreL", args));
        };
        k
    } else {
        0.0
    };
    let Some(x) = super::to_f64(args.last().unwrap()) else {
        return Ok(unevaluated("LaguerreL", args));
    };
    if (n - n.round()).abs() > 1e-12 || n < 0.0 {
        return Ok(unevaluated("LaguerreL", args));
    }
    Ok(super::real(laguerre_l(n.round() as i64, k, x)))
}

/// `P_n(x)` via the Bonnet recurrence; accepts negative degree through
/// `P_{−n−1} = P_n`.
fn legendre_p_int(n: i64, x: f64) -> f64 {
    let n = if n < 0 { -n - 1 } else { n };
    match n {
        0 => 1.0,
        1 => x,
        _ => {
            let (mut p0, mut p1) = (1.0_f64, x);
            for k in 1..n {
                let f = k as f64;
                let p2 = ((2.0 * f + 1.0) * x * p1 - f * p0) / (f + 1.0);
                p0 = p1;
                p1 = p2;
            }
            p1
        }
    }
}

/// `P_n^m(x)` with the Condon–Shortley phase, honouring negative order.
fn assoc_legendre_signed(n: i64, m: i64, x: f64) -> f64 {
    if m >= 0 {
        if m > n { 0.0 } else { assoc_legendre(n, m, x) }
    } else {
        let mm = -m;
        if mm > n {
            0.0
        } else {
            let sign = if mm % 2 == 0 { 1.0 } else { -1.0 };
            let ratio = factorial_small((n - mm) as u32) / factorial_small((n + mm) as u32);
            sign * ratio * assoc_legendre(n, mm, x)
        }
    }
}

/// `Q_n(x)` (Legendre function of the second kind) via the convolution
/// representation, valid for `|x| < 1` and `|x| > 1`.
fn legendre_q(n: i64, x: f64) -> f64 {
    let n = n.unsigned_abs();
    let q0 = if x.abs() < 1.0 {
        x.atanh()
    } else {
        0.5 * ((x + 1.0) / (x - 1.0)).ln()
    };
    if n == 0 {
        return q0;
    }
    let (mut qm1, mut qn) = (q0, x * q0 - 1.0);
    for k in 1..n {
        let f = k as f64;
        let qp = ((2.0 * f + 1.0) * x * qn - f * qm1) / (f + 1.0);
        qm1 = qn;
        qn = qp;
    }
    qn
}

/// `Q_n^m(x)` (associated, type 2) with the Condon–Shortley phase, using the
/// derivative recurrence derived from Legendre's differential equation.
fn assoc_legendre_q(n: i64, m: i64, x: f64) -> f64 {
    let nn = n.unsigned_abs();
    if m < 0 {
        let mm = -m;
        let sign = if mm % 2 == 0 { 1.0 } else { -1.0 };
        let ratio =
            factorial_small((n - mm).unsigned_abs() as u32) / factorial_small((n + mm) as u32);
        return sign * ratio * assoc_legendre_q(n, mm, x);
    }
    let q0 = legendre_q(n, x);
    if m == 0 {
        return q0;
    }
    let denom = 1.0 - x * x;
    let qprev = if nn == 0 { 0.0 } else { legendre_q(n - 1, x) };
    let nf = nn as f64;
    let mut qk = q0;
    let mut qk1 = nf * (qprev - x * q0) / denom;
    for k in 0..(m as usize - 1) {
        let f = k as f64;
        let coeff = nf * (nf + 1.0) - f * (f + 1.0) - 2.0 * f;
        let qk2 = (2.0 * x * (f + 1.0) * qk1 - coeff * qk) / denom;
        qk = qk1;
        qk1 = qk2;
    }
    let sign = if m % 2 == 0 { 1.0 } else { -1.0 };
    sign * (denom).powf(m as f64 / 2.0) * qk1
}

/// LegendreP[n, x], LegendreP[n, m, x] and LegendreP[n, m, a, x].
pub fn builtin_legendre_p(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 4 {
        return Err(EvalError::Error(
            "LegendreP requires 2, 3 or 4 arguments".to_string(),
        ));
    }
    let nv = &args[0];
    let Some(n) = super::to_f64(nv) else {
        return Ok(unevaluated("LegendreP", args));
    };
    if args.len() == 2 {
        let Some(x) = super::to_f64(&args[1]) else {
            return Ok(unevaluated("LegendreP", args));
        };
        let v = if (n - n.round()).abs() < 1e-12 {
            legendre_p_int(n.round() as i64, x)
        } else {
            hyper_2f1(-n, n + 1.0, 1.0, (1.0 - x) / 2.0)
        };
        return Ok(if v.is_nan() {
            unevaluated("LegendreP", args)
        } else {
            super::real(v)
        });
    }
    // 3- or 4-argument: associated functions need an integer degree/order.
    let Some(m) = super::to_f64(&args[1]) else {
        return Ok(unevaluated("LegendreP", args));
    };
    if (n - n.round()).abs() > 1e-12 || (m - m.round()).abs() > 1e-12 {
        return Ok(unevaluated("LegendreP", args));
    }
    let (ni, mi) = (n.round() as i64, m.round() as i64);
    if args.len() == 4 {
        let Some(a) = super::to_f64(&args[2]) else {
            return Ok(unevaluated("LegendreP", args));
        };
        if !(is_int(&args[2], 1) || is_int(&args[2], 2) || is_int(&args[2], 3)) {
            return Ok(unevaluated("LegendreP", args));
        }
        let _ = a;
        if is_int(&args[2], 2) || is_int(&args[2], 3) {
            let Some(x) = super::to_f64(&args[3]) else {
                return Ok(unevaluated("LegendreP", args));
            };
            return Ok(super::real(assoc_legendre_q(ni, mi, x)));
        }
        let Some(x) = super::to_f64(&args[3]) else {
            return Ok(unevaluated("LegendreP", args));
        };
        return Ok(super::real(assoc_legendre_signed(ni, mi, x)));
    }
    let Some(x) = super::to_f64(&args[2]) else {
        return Ok(unevaluated("LegendreP", args));
    };
    Ok(super::real(assoc_legendre_signed(ni, mi, x)))
}

/// AssociatedLegendreP[n, m, x] — alias for the associated `LegendreP`.
pub fn builtin_associated_legendre_p(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("AssociatedLegendreP", args, 3)?;
    builtin_legendre_p(args)
}

/// LegendreQ[n, x] and LegendreQ[n, m, x].
pub fn builtin_legendre_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 && args.len() != 3 {
        return Err(EvalError::Error(
            "LegendreQ requires 2 or 3 arguments".to_string(),
        ));
    }
    let Some(n) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("LegendreQ", args));
    };
    if (n - n.round()).abs() > 1e-12 || n < 0.0 {
        return Ok(unevaluated("LegendreQ", args));
    }
    let ni = n.round() as i64;
    if args.len() == 2 {
        let Some(x) = super::to_f64(&args[1]) else {
            return Ok(unevaluated("LegendreQ", args));
        };
        let v = legendre_q(ni, x);
        return Ok(if v.is_nan() {
            unevaluated("LegendreQ", args)
        } else {
            super::real(v)
        });
    }
    let Some(m) = super::to_f64(&args[1]) else {
        return Ok(unevaluated("LegendreQ", args));
    };
    if (m - m.round()).abs() > 1e-12 {
        return Ok(unevaluated("LegendreQ", args));
    }
    let Some(x) = super::to_f64(&args[2]) else {
        return Ok(unevaluated("LegendreQ", args));
    };
    let v = assoc_legendre_q(ni, m.round() as i64, x);
    Ok(if v.is_nan() {
        unevaluated("LegendreQ", args)
    } else {
        super::real(v)
    })
}

// ── Elliptic integrals and Jacobi elliptic functions ────────────────────────

/// Composite Simpson quadrature on `[a, b]` with `n` (even) panels.
fn simpson<F: Fn(f64) -> f64>(f: F, a: f64, b: f64, n: usize) -> f64 {
    let h = (b - a) / n as f64;
    let mut s = f(a) + f(b);
    for i in 1..n {
        s += if i % 2 == 1 { 4.0 } else { 2.0 } * f(a + i as f64 * h);
    }
    s * h / 3.0
}

/// Complete elliptic integrals `K(m)` and `E(m)` (parameter `m`) via the AGM.
fn agm_k_e(m: f64) -> (f64, f64) {
    let mut a = 1.0_f64;
    let mut b = (1.0 - m).sqrt();
    let mut c = m.sqrt();
    let mut sum_e = c * c / 2.0;
    let mut pow2 = 1.0_f64;
    for _ in 0..60 {
        if c.abs() <= 1e-17 * a {
            break;
        }
        let an = 0.5 * (a + b);
        let bn = (a * b).sqrt();
        let cn = 0.5 * (a - b);
        a = an;
        b = bn;
        c = cn;
        sum_e += pow2 * c * c;
        pow2 *= 2.0;
    }
    let k = std::f64::consts::PI / (2.0 * a);
    (k, k * (1.0 - sum_e))
}

fn ellip_f_real(phi: f64, m: f64) -> f64 {
    simpson(
        |t| 1.0 / (1.0 - m * t.sin() * t.sin()).sqrt(),
        0.0,
        phi,
        8192,
    )
}

fn ellip_pi_real(n: f64, m: f64) -> f64 {
    simpson(
        |t| {
            let s = t.sin();
            1.0 / ((1.0 - n * s * s) * (1.0 - m * s * s).sqrt())
        },
        0.0,
        std::f64::consts::FRAC_PI_2,
        16384,
    )
}

/// Exact `Pi/2` value.
fn half_pi_exact() -> Value {
    sc("Divide", &[Value::Symbol("Pi".to_string()), int_value(2)])
}

/// `true` when `v` is an exact zero integer/rational.
fn is_exact_zero(v: &Value) -> bool {
    matches!(v, Value::Integer(_) | Value::Rational(_)) && super::to_f64(v) == Some(0.0)
}

/// EllipticK[m] — complete elliptic integral of the first kind.
pub fn builtin_elliptic_k(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("EllipticK", args, 1)?;
    if is_exact_zero(&args[0]) {
        return Ok(half_pi_exact());
    }
    let Some(m) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("EllipticK", args));
    };
    if m == 1.0 {
        return Ok(super::real(f64::INFINITY));
    }
    if m > 1.0 {
        return Ok(unevaluated("EllipticK", args));
    }
    Ok(super::real(agm_k_e(m).0))
}

/// EllipticE[m] — complete elliptic integral of the second kind.
pub fn builtin_elliptic_e(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("EllipticE", args, 1)?;
    if is_exact_zero(&args[0]) {
        return Ok(half_pi_exact());
    }
    if is_int(&args[0], 1) {
        return Ok(int_value(1));
    }
    let Some(m) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("EllipticE", args));
    };
    if m > 1.0 {
        return Ok(unevaluated("EllipticE", args));
    }
    Ok(super::real(agm_k_e(m).1))
}

/// EllipticPi[n, m] — complete elliptic integral of the third kind.
pub fn builtin_elliptic_pi(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("EllipticPi", args, 2)?;
    let (Some(n), Some(m)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("EllipticPi", args));
    };
    if m > 1.0 {
        return Ok(unevaluated("EllipticPi", args));
    }
    Ok(super::real(ellip_pi_real(n, m)))
}

/// EllipticF[phi, m] — incomplete elliptic integral of the first kind.
pub fn builtin_elliptic_f(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("EllipticF", args, 2)?;
    if is_exact_zero(&args[1]) {
        return Ok(args[0].clone());
    }
    let (Some(phi), Some(m)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("EllipticF", args));
    };
    if m > 1.0 {
        return Ok(unevaluated("EllipticF", args));
    }
    Ok(super::real(ellip_f_real(phi, m)))
}

/// Jacobi elliptic functions `sn(u, m)`, `cn(u, m)`, `dn(u, m)` via the
/// descending Landen transformation.
fn jacobi_elliptic(u: f64, m: f64) -> Option<(f64, f64, f64)> {
    if !(0.0..=1.0).contains(&m) {
        return None;
    }
    let mut a = vec![1.0_f64];
    let mut b = vec![(1.0 - m).sqrt()];
    let mut c = vec![m.sqrt()];
    for _ in 0..60 {
        let last = a.len() - 1;
        if c[last].abs() <= 1e-20 * a[last] {
            break;
        }
        let an = 0.5 * (a[last] + b[last]);
        let bn = (a[last] * b[last]).sqrt();
        let cn = 0.5 * (a[last] - b[last]);
        a.push(an);
        b.push(bn);
        c.push(cn);
    }
    let n = a.len() - 1;
    let mut phi = (2.0_f64).powi(n as i32) * a[n] * u;
    for k in (1..=n).rev() {
        phi = 0.5 * (phi + ((c[k] / a[k]) * phi.sin()).asin());
    }
    let sn = phi.sin();
    let cn = phi.cos();
    let dn = (1.0 - m * sn * sn).sqrt();
    Some((sn, cn, dn))
}

/// Shared driver for JacobiSN/CN/DN.
fn jacobi_builtin(
    name: &str,
    args: &[Value],
    pick: fn((f64, f64, f64)) -> f64,
) -> Result<Value, EvalError> {
    super::require_args(name, args, 2)?;
    let (Some(u), Some(m)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated(name, args));
    };
    match jacobi_elliptic(u, m) {
        Some(vals) => Ok(super::real(pick(vals))),
        None => Ok(unevaluated(name, args)),
    }
}

/// JacobiSN[u, m].
pub fn builtin_jacobi_sn(args: &[Value]) -> Result<Value, EvalError> {
    jacobi_builtin("JacobiSN", args, |(sn, _, _)| sn)
}

/// JacobiCN[u, m].
pub fn builtin_jacobi_cn(args: &[Value]) -> Result<Value, EvalError> {
    jacobi_builtin("JacobiCN", args, |(_, cn, _)| cn)
}

/// JacobiDN[u, m].
pub fn builtin_jacobi_dn(args: &[Value]) -> Result<Value, EvalError> {
    jacobi_builtin("JacobiDN", args, |(_, _, dn)| dn)
}

/// EllipticNomeQ[m] = Exp[-Pi EllipticK[1-m] / EllipticK[m]].
pub fn builtin_elliptic_nome_q(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("EllipticNomeQ", args, 1)?;
    let Some(m) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("EllipticNomeQ", args));
    };
    if m == 0.0 {
        return Ok(int_value(0));
    }
    if !(0.0..1.0).contains(&m) {
        return Ok(unevaluated("EllipticNomeQ", args));
    }
    let k = agm_k_e(m).0;
    let k1 = agm_k_e(1.0 - m).0;
    Ok(super::real((-std::f64::consts::PI * k1 / k).exp()))
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "HypergeometricU", builtin_hypergeometric_u);
    super::register_builtin(
        env,
        "MathieuCharacteristicA",
        builtin_mathieu_characteristic_a,
    );
    super::register_builtin(
        env,
        "MathieuCharacteristicB",
        builtin_mathieu_characteristic_b,
    );
    super::register_builtin(env, "SpheroidalS1", builtin_spheroidal_s1);
    super::register_builtin(env, "CoulombF", builtin_coulomb_f);
    super::register_builtin(env, "ZernikeR", builtin_zernike_r);
    super::register_builtin(env, "Hypergeometric0F1", builtin_hypergeometric_0f1);
    super::register_builtin(env, "Hypergeometric1F1", builtin_hypergeometric_1f1);
    super::register_builtin(env, "Hypergeometric2F1", builtin_hypergeometric_2f1);
    super::register_builtin(env, "HypergeometricPFQ", builtin_hypergeometric_pfq);
    super::register_builtin(
        env,
        "HypergeometricPFQRegularized",
        builtin_hypergeometric_pfq_regularized,
    );
    super::register_builtin(env, "MeijerG", builtin_meijer_g);
    super::register_builtin(env, "HurwitzZeta", builtin_hurwitz_zeta);
    super::register_builtin(env, "LerchPhi", builtin_lerch_phi);
    super::register_builtin(env, "DirichletLambda", builtin_dirichlet_lambda);
    super::register_builtin(env, "RiemannSiegelTheta", builtin_riemann_siegel_theta);
    super::register_builtin(env, "RiemannSiegelZ", builtin_riemann_siegel_z);
    super::register_builtin(env, "ExpIntegralE", builtin_exp_integral_e);
    super::register_builtin(env, "ExpIntegralEi", builtin_exp_integral_ei);
    super::register_builtin(env, "LogIntegral", builtin_log_integral);
    super::register_builtin(env, "SinIntegral", builtin_sin_integral);
    super::register_builtin(env, "CosIntegral", builtin_cos_integral);
    super::register_builtin(env, "SinhIntegral", builtin_sinh_integral);
    super::register_builtin(env, "CoshIntegral", builtin_cosh_integral);
    super::register_builtin(env, "FresnelS", builtin_fresnel_s);
    super::register_builtin(env, "FresnelC", builtin_fresnel_c);
    super::register_builtin(env, "DawsonF", builtin_dawson_f);
    super::register_builtin(env, "StruveH", builtin_struve_h);
    super::register_builtin(env, "StruveL", builtin_struve_l);
    super::register_builtin(env, "KelvinBer", builtin_kelvin_ber);
    super::register_builtin(env, "KelvinBei", builtin_kelvin_bei);
    super::register_builtin(env, "KelvinKer", builtin_kelvin_ker);
    super::register_builtin(env, "KelvinKei", builtin_kelvin_kei);
    super::register_builtin(env, "AppellF1", builtin_appell_f1);
    super::register_builtin(env, "QPochhammer", builtin_qpochhammer);
    super::register_builtin(env, "BesselH1", builtin_bessel_h1);
    super::register_builtin(env, "BesselH2", builtin_bessel_h2);
    super::register_builtin(env, "HankelH1", builtin_bessel_h1);
    super::register_builtin(env, "HankelH2", builtin_bessel_h2);
    super::register_builtin(env, "ChebyshevT", builtin_chebyshev_t);
    super::register_builtin(env, "ChebyshevU", builtin_chebyshev_u);
    super::register_builtin(env, "GegenbauerC", builtin_gegenbauer_c);
    super::register_builtin(env, "GegenbauerCP", builtin_gegenbauer_cp);
    super::register_builtin(env, "JacobiP", builtin_jacobi_p);
    super::register_builtin(env, "SphericalHarmonicY", builtin_spherical_harmonic_y);
    super::register_builtin(env, "AiryAiPrime", builtin_airy_ai_prime);
    super::register_builtin(env, "AiryBiPrime", builtin_airy_bi_prime);
    // General-order Bessel family and spherical variants.
    super::register_builtin(env, "BesselJ", builtin_bessel_j);
    super::register_builtin(env, "BesselY", builtin_bessel_y);
    super::register_builtin(env, "BesselI", builtin_bessel_i);
    super::register_builtin(env, "BesselK", builtin_bessel_k);
    super::register_builtin(env, "BesselJZero", builtin_bessel_j_zero);
    super::register_builtin(env, "SphericalBesselJ", builtin_spherical_bessel_j);
    super::register_builtin(env, "SphericalBesselY", builtin_spherical_bessel_y);
    // Airy functions (aliases onto the sibling `specialfunctions`).
    super::register_builtin(env, "AiryAi", super::specialfunctions::builtin_airy_ai);
    super::register_builtin(env, "AiryBi", super::specialfunctions::builtin_airy_bi);
    // Classical orthogonal polynomials.
    super::register_builtin(env, "HermiteH", builtin_hermite_h);
    super::register_builtin(env, "LaguerreL", builtin_laguerre_l);
    super::register_builtin(env, "LegendreP", builtin_legendre_p);
    super::register_builtin(env, "LegendreQ", builtin_legendre_q);
    super::register_builtin(env, "AssociatedLegendreP", builtin_associated_legendre_p);
    // Elliptic integrals and Jacobi elliptic functions.
    super::register_builtin(env, "EllipticK", builtin_elliptic_k);
    super::register_builtin(env, "EllipticE", builtin_elliptic_e);
    super::register_builtin(env, "EllipticPi", builtin_elliptic_pi);
    super::register_builtin(env, "EllipticF", builtin_elliptic_f);
    super::register_builtin(env, "JacobiSN", builtin_jacobi_sn);
    super::register_builtin(env, "JacobiCN", builtin_jacobi_cn);
    super::register_builtin(env, "JacobiDN", builtin_jacobi_dn);
    super::register_builtin(env, "EllipticNomeQ", builtin_elliptic_nome_q);
    // Aliases onto sibling implementations in `specialfunctions`.
    super::register_builtin(
        env,
        "ProductLog",
        super::specialfunctions::builtin_lambert_w,
    );
    super::register_builtin(env, "ErfInv", super::specialfunctions::builtin_inverse_erf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Float;

    fn r(x: f64) -> Value {
        Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, x))
    }
    fn i(n: i64) -> Value {
        Value::Integer(rug::Integer::from(n))
    }
    fn lst(xs: &[f64]) -> Value {
        Value::List(xs.iter().map(|&x| r(x)).collect())
    }

    fn close(a: &Value, want: f64, tol: f64) {
        if let Value::Real(v) = a {
            let got = v.to_f64();
            assert!(
                (got - want).abs() < tol,
                "got {got}, want {want} (tol {tol})"
            );
        } else {
            panic!("expected Real, got {a:?}");
        }
    }

    fn close_cx(a: &Value, wre: f64, wim: f64, tol: f64) {
        match a {
            Value::Complex { re, im } => assert!(
                (re - wre).abs() < tol && (im - wim).abs() < tol,
                "got {re}+{im}i, want {wre}+{wim}i (tol {tol})"
            ),
            Value::Real(r) if wim == 0.0 => {
                assert!((r.to_f64() - wre).abs() < tol, "got {r}, want {wre}");
            }
            _ => panic!("expected Complex, got {a:?}"),
        }
    }

    fn is_unevaluated(v: &Value, head: &str) {
        assert!(
            matches!(v, Value::Call { head: h, .. } if h == head),
            "expected unevaluated {head}, got {v:?}"
        );
    }

    #[test]
    fn test_registered() {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        for name in [
            "Hypergeometric0F1",
            "Hypergeometric1F1",
            "Hypergeometric2F1",
            "HypergeometricPFQ",
            "HypergeometricPFQRegularized",
            "MeijerG",
            "HurwitzZeta",
            "LerchPhi",
            "DirichletLambda",
            "RiemannSiegelTheta",
            "RiemannSiegelZ",
            "ExpIntegralE",
            "ExpIntegralEi",
            "LogIntegral",
            "SinIntegral",
            "CosIntegral",
            "SinhIntegral",
            "CoshIntegral",
            "FresnelS",
            "FresnelC",
            "DawsonF",
            "StruveH",
            "StruveL",
            "KelvinBer",
            "KelvinBei",
            "KelvinKer",
            "KelvinKei",
            "AppellF1",
            "QPochhammer",
            "BesselH1",
            "BesselH2",
            "HankelH1",
            "HankelH2",
            "ChebyshevT",
            "ChebyshevU",
            "GegenbauerC",
            "GegenbauerCP",
            "JacobiP",
            "SphericalHarmonicY",
            "AiryAiPrime",
            "AiryBiPrime",
            "AiryAi",
            "AiryBi",
            "BesselJ",
            "BesselY",
            "BesselI",
            "BesselK",
            "BesselJZero",
            "SphericalBesselJ",
            "SphericalBesselY",
            "HermiteH",
            "LaguerreL",
            "LegendreP",
            "LegendreQ",
            "AssociatedLegendreP",
            "EllipticK",
            "EllipticE",
            "EllipticPi",
            "EllipticF",
            "JacobiSN",
            "JacobiCN",
            "JacobiDN",
            "EllipticNomeQ",
            "ProductLog",
            "ErfInv",
        ] {
            assert!(
                matches!(env.get(name), Some(Value::Builtin(..))),
                "{name} not registered"
            );
        }
    }

    #[test]
    fn test_0f1_value() {
        // 0F1(1; 1) = Σ 1/(k!)² = I_0(2).
        close(
            &builtin_hypergeometric_0f1(&[i(1), i(1)]).unwrap(),
            2.2795853023,
            1e-7,
        );
        // 0F1(2; 1) = I_1(2).
        let v = builtin_hypergeometric_0f1(&[i(2), i(1)]).unwrap();
        close(&v, 1.5906368546, 1e-6);
    }

    #[test]
    fn test_0f1_zero() {
        assert!(matches!(
            builtin_hypergeometric_0f1(&[i(1), i(0)]).unwrap(),
            Value::Integer(ref n) if *n == 1
        ));
    }

    #[test]
    fn test_1f1_e() {
        // 1F1(1; 1; z) = e^z.
        close(
            &builtin_hypergeometric_1f1(&[i(1), i(1), i(1)]).unwrap(),
            std::f64::consts::E,
            1e-10,
        );
        // 1F1(1; 2; z) = (e^z - 1)/z.
        close(
            &builtin_hypergeometric_1f1(&[i(1), i(2), i(1)]).unwrap(),
            std::f64::consts::E - 1.0,
            1e-8,
        );
    }

    #[test]
    fn test_1f1_symbolic() {
        is_unevaluated(
            &builtin_hypergeometric_1f1(&[Value::Symbol("a".into()), i(1), i(1)]).unwrap(),
            "Hypergeometric1F1",
        );
    }

    #[test]
    fn test_2f1_log() {
        // 2F1(1,1;2;z) = -ln(1-z)/z.
        close(
            &builtin_hypergeometric_2f1(&[i(1), i(1), i(2), r(0.5)]).unwrap(),
            2.0 * std::f64::consts::LN_2,
            1e-9,
        );
        // z < 0 uses the Pfaff transformation.
        close(
            &builtin_hypergeometric_2f1(&[i(1), i(1), i(2), r(-1.0)]).unwrap(),
            std::f64::consts::LN_2,
            1e-9,
        );
    }

    #[test]
    fn test_2f1_atanh() {
        // 2F1(1/2,1;3/2;z) = atanh(√z)/√z.
        let z = 0.25;
        close(
            &builtin_hypergeometric_2f1(&[r(0.5), i(1), r(1.5), r(z)]).unwrap(),
            0.5 * ((1.0 + z.sqrt()) / (1.0 - z.sqrt())).ln() / z.sqrt(),
            1e-9,
        );
    }

    #[test]
    fn test_2f1_near_one() {
        // 2F1(1,1;2;z) = -ln(1-z)/z, accurate right up to z = 1.
        for &z in &[0.9_f64, 0.99, 0.999_999, 0.999_999_9] {
            close(
                &builtin_hypergeometric_2f1(&[i(1), i(1), i(2), r(z)]).unwrap(),
                -(1.0 - z).ln() / z,
                1e-8 * (1.0 / (1.0 - z)).max(1.0),
            );
        }
        // Degenerate denominator c = a + b reduces to 1/(1-z).
        close(
            &builtin_hypergeometric_2f1(&[i(1), i(1), i(1), r(0.9)]).unwrap(),
            10.0,
            1e-9,
        );
    }

    #[test]
    fn test_2f1_c_eq_a_plus_b_is_infinite_at_one() {
        // c = a + b -> logarithmic divergence at z = 1.
        assert_eq!(hyper_2f1(1.0, 1.0, 2.0, 1.0), f64::INFINITY);
        assert_eq!(hyper_2f1(2.0, 3.0, 5.0, 1.0), f64::INFINITY);
    }

    #[test]
    fn test_2f1_terminating_and_noninteger_s() {
        // Terminating series: 2F1(-2,1;1;z) = (1 - z)^2.
        for &z in &[-3.0_f64, 0.5, 2.5] {
            close(
                &builtin_hypergeometric_2f1(&[i(-2), i(1), i(1), r(z)]).unwrap(),
                (1.0 - z).powi(2),
                1e-10,
            );
        }
        // Non-integer c-a-b near z = 1 should match the direct series.
        let (a, b, c, z) = (1.3, 2.1, 4.6, 0.95);
        close(
            &builtin_hypergeometric_2f1(&[r(a), r(b), r(c), r(z)]).unwrap(),
            hyper_pfq(&[a, b], &[c], z),
            1e-9,
        );
    }

    #[test]
    fn test_1f1_negative_argument_accuracy() {
        // 1F1(1;2;z) = (e^z - 1)/z; the alternating series must not lose the
        // small result to cancellation.
        for &z in &[-1.0_f64, -10.0, -50.0, -100.0] {
            close(
                &builtin_hypergeometric_1f1(&[i(1), i(2), r(z)]).unwrap(),
                (z.exp() - 1.0) / z,
                1e-12 * ((z.exp() - 1.0) / z).abs().max(1.0),
            );
        }
        // Large negative argument via the asymptotic expansion.
        close(
            &builtin_hypergeometric_1f1(&[i(1), i(2), r(-1000.0)]).unwrap(),
            0.001,
            1e-12,
        );
    }

    #[test]
    fn test_digamma() {
        // ψ(1) = -γ, ψ(2) = 1 - γ, ψ(1/2) = -γ - 2 ln 2.
        close(&r(digamma_fn(1.0)), -EULER_GAMMA, 1e-12);
        close(&r(digamma_fn(2.0)), 1.0 - EULER_GAMMA, 1e-12);
        close(
            &r(digamma_fn(0.5)),
            -EULER_GAMMA - 2.0 * std::f64::consts::LN_2,
            1e-12,
        );
    }

    #[test]
    fn test_pfq() {
        // 0F0(; ; z) = e^z.
        close(
            &builtin_hypergeometric_pfq(&[lst(&[]), lst(&[]), i(1)]).unwrap(),
            std::f64::consts::E,
            1e-10,
        );
        // 1F0(1; ; z) = 1/(1-z).
        close(
            &builtin_hypergeometric_pfq(&[lst(&[1.0]), lst(&[]), r(0.5)]).unwrap(),
            2.0,
            1e-9,
        );
        // Confluent case with negative z avoids cancellation: 1F1(1;2;-100).
        close(
            &builtin_hypergeometric_pfq(&[lst(&[1.0]), lst(&[2.0]), i(-100)]).unwrap(),
            0.01,
            1e-12,
        );
    }

    #[test]
    fn test_pfq_regularized() {
        // Regularized 1F1(1;2;z)/Γ(2) = (e^z - 1)/z.
        close(
            &builtin_hypergeometric_pfq_regularized(&[lst(&[1.0]), lst(&[2.0]), i(1)]).unwrap(),
            std::f64::consts::E - 1.0,
            1e-8,
        );
    }

    #[test]
    fn test_meijer_g_exp() {
        // MeijerG[{{}, {}}, {{0}, {}}, z] = e^{-z}.
        let v = builtin_meijer_g(&[
            Value::List(vec![Value::List(vec![]), Value::List(vec![])]),
            Value::List(vec![Value::List(vec![i(0)]), Value::List(vec![])]),
            i(1),
        ])
        .unwrap();
        close(&v, (-1.0_f64).exp(), 1e-8);
    }

    #[test]
    fn test_meijer_g_rational() {
        // MeijerG[{{0}, {}}, {{0}, {}}, z] = 1/(1+z).
        let v = builtin_meijer_g(&[
            Value::List(vec![Value::List(vec![i(0)]), Value::List(vec![])]),
            Value::List(vec![Value::List(vec![i(0)]), Value::List(vec![])]),
            i(1),
        ])
        .unwrap();
        close(&v, 0.5, 1e-8);
    }

    #[test]
    fn test_hurwitz_zeta() {
        // ζ(2, 3) = ζ(2) - 1 - 1/4.
        let want = std::f64::consts::PI * std::f64::consts::PI / 6.0 - 1.25;
        close(&builtin_hurwitz_zeta(&[i(2), i(3)]).unwrap(), want, 1e-7);
        close(
            &builtin_hurwitz_zeta(&[i(3), i(1)]).unwrap(),
            1.2020569032,
            1e-7,
        );
    }

    #[test]
    fn test_hurwitz_zeta_one_unevaluated() {
        is_unevaluated(&builtin_hurwitz_zeta(&[i(1), i(1)]).unwrap(), "HurwitzZeta");
    }

    #[test]
    fn test_lerch_phi() {
        // LerchPhi(z, 1, 1) = -ln(1-z)/z.
        close(
            &builtin_lerch_phi(&[r(0.5), i(1), i(1)]).unwrap(),
            2.0 * std::f64::consts::LN_2,
            1e-8,
        );
        // LerchPhi(1, s, a) = HurwitzZeta[s, a].
        close(
            &builtin_lerch_phi(&[i(1), i(2), i(1)]).unwrap(),
            std::f64::consts::PI * std::f64::consts::PI / 6.0,
            1e-6,
        );
    }

    #[test]
    fn test_dirichlet_lambda() {
        // λ(2) = π²/8.
        close(
            &builtin_dirichlet_lambda(&[i(2)]).unwrap(),
            std::f64::consts::PI * std::f64::consts::PI / 8.0,
            1e-8,
        );
        close(
            &builtin_dirichlet_lambda(&[i(1)]).unwrap(),
            std::f64::consts::LN_2,
            1e-10,
        );
    }

    #[test]
    fn test_riemann_siegel_theta() {
        // Odd function, and matches the large-t asymptotic.
        let a = builtin_riemann_siegel_theta(&[r(3.0)]).unwrap();
        let b = builtin_riemann_siegel_theta(&[r(-3.0)]).unwrap();
        if let (Value::Real(x), Value::Real(y)) = (&a, &b) {
            assert!((x.to_f64() + y.to_f64()).abs() < 1e-10);
        }
        let t = 100.0;
        let asym = 0.5 * t * (t / (2.0 * std::f64::consts::PI)).ln()
            - 0.5 * t
            - std::f64::consts::PI / 8.0
            + 1.0 / (48.0 * t)
            + 7.0 / (5760.0 * t.powi(3));
        close(&builtin_riemann_siegel_theta(&[r(t)]).unwrap(), asym, 1e-6);
        assert!(matches!(
            builtin_riemann_siegel_theta(&[i(0)]).unwrap(),
            Value::Integer(ref n) if *n == 0
        ));
    }

    #[test]
    fn test_riemann_siegel_z() {
        // Z(0) = ζ(1/2).
        close(
            &builtin_riemann_siegel_z(&[i(0)]).unwrap(),
            -1.4603545088,
            1e-7,
        );
        // Z(14.134725) is (very close to) the first non-trivial zero.
        let v = builtin_riemann_siegel_z(&[r(14.134_725)]).unwrap();
        if let Value::Real(x) = v {
            assert!(
                x.to_f64().abs() < 5e-3,
                "Z near first zero = {}",
                x.to_f64()
            );
        }
    }

    #[test]
    fn test_exp_integral_e() {
        close(
            &builtin_exp_integral_e(&[i(1), i(1)]).unwrap(),
            0.2193839344,
            1e-8,
        );
        // E_2(1) = e^{-1} - E_1(1).
        close(
            &builtin_exp_integral_e(&[i(2), i(1)]).unwrap(),
            (-1.0_f64).exp() - 0.2193839344,
            1e-8,
        );
    }

    #[test]
    fn test_exp_integral_ei() {
        close(
            &builtin_exp_integral_ei(&[i(1)]).unwrap(),
            1.8951178164,
            1e-7,
        );
        close(
            &builtin_exp_integral_ei(&[i(-1)]).unwrap(),
            -0.2193839344,
            1e-7,
        );
    }

    #[test]
    fn test_log_integral() {
        close(&builtin_log_integral(&[i(2)]).unwrap(), 1.0451637801, 1e-7);
        close(&builtin_log_integral(&[r(0.5)]).unwrap(), -0.37867104, 1e-5);
    }

    #[test]
    fn test_sin_integral() {
        close(&builtin_sin_integral(&[i(1)]).unwrap(), 0.9460830704, 1e-9);
        close(
            &builtin_sin_integral(&[r(std::f64::consts::PI)]).unwrap(),
            1.8519370519,
            1e-8,
        );
    }

    #[test]
    fn test_cos_integral() {
        close(&builtin_cos_integral(&[i(1)]).unwrap(), 0.3374039229, 1e-8);
        // Ci is even.
        close(
            &builtin_cos_integral(&[i(-2)]).unwrap(),
            builtin_cos_integral(&[i(2)]).unwrap().to_real().unwrap(),
            1e-12,
        );
    }

    #[test]
    fn test_hyperbolic_integrals() {
        close(&builtin_sinh_integral(&[i(1)]).unwrap(), 1.0572508754, 1e-8);
        close(&builtin_cosh_integral(&[i(1)]).unwrap(), 0.8378669409, 1e-8);
    }

    #[test]
    fn test_fresnel() {
        close(&builtin_fresnel_s(&[i(1)]).unwrap(), 0.4382591474, 1e-9);
        close(&builtin_fresnel_c(&[i(1)]).unwrap(), 0.7798934004, 1e-9);
    }

    #[test]
    fn test_dawson_f() {
        close(&builtin_dawson_f(&[i(1)]).unwrap(), 0.5380795069, 1e-8);
        // F(z) ≈ z for small z.
        close(&builtin_dawson_f(&[r(0.01)]).unwrap(), 0.01, 1e-6);
    }

    #[test]
    fn test_struve() {
        close(&builtin_struve_h(&[i(0), i(1)]).unwrap(), 0.5686566, 1e-6);
        close(&builtin_struve_l(&[i(0), i(1)]).unwrap(), 0.7102431, 1e-6);
    }

    #[test]
    fn test_kelvin() {
        close(&builtin_kelvin_ber(&[i(1)]).unwrap(), 0.9843817802, 1e-6);
        close(&builtin_kelvin_bei(&[i(1)]).unwrap(), 0.2495660400, 1e-6);
        close(&builtin_kelvin_ker(&[i(1)]).unwrap(), 0.2867062087, 1e-5);
        close(&builtin_kelvin_kei(&[i(1)]).unwrap(), -0.4949946365, 1e-5);
        // ber(0) = 1, bei(0) = 0.
        close(&builtin_kelvin_ber(&[i(0)]).unwrap(), 1.0, 1e-12);
        close(&builtin_kelvin_bei(&[i(0)]).unwrap(), 0.0, 1e-12);
    }

    #[test]
    fn test_appell_f1() {
        // AppellF1[a, b, c...; y=0] reduces to 2F1(a, b1; c; x).
        close(
            &builtin_appell_f1(&[i(1), i(1), i(1), i(2), r(0.5), i(0)]).unwrap(),
            2.0 * std::f64::consts::LN_2,
            1e-8,
        );
        // x = y = 0 gives the exact integer 1.
        assert!(matches!(
            builtin_appell_f1(&[i(1), i(1), i(1), i(2), i(0), i(0)]).unwrap(),
            Value::Integer(ref n) if *n == 1
        ));
    }

    #[test]
    fn test_qpochhammer() {
        // (1/2; 1/2)_∞ ≈ 0.2887880951.
        close(
            &builtin_qpochhammer(&[r(0.5), r(0.5)]).unwrap(),
            0.2887880951,
            1e-8,
        );
        // Finite product: (a; q)_2 = (1-a)(1-aq).
        close(
            &builtin_qpochhammer(&[r(0.5), r(0.5), i(2)]).unwrap(),
            0.5 * 0.75,
            1e-12,
        );
        // (1; q)_∞ = 0.
        close(&builtin_qpochhammer(&[i(1), r(0.5)]).unwrap(), 0.0, 1e-12);
    }

    #[test]
    fn test_symbolic_fallbacks() {
        is_unevaluated(
            &builtin_hypergeometric_0f1(&[Value::Symbol("b".into()), Value::Symbol("z".into())])
                .unwrap(),
            "Hypergeometric0F1",
        );
        let s = Value::Symbol("x".to_string());
        is_unevaluated(
            &builtin_meijer_g(&[s.clone(), s.clone(), s]).unwrap(),
            "MeijerG",
        );
        is_unevaluated(
            &builtin_qpochhammer(&[Value::Symbol("a".into())]).unwrap(),
            "QPochhammer",
        );
        is_unevaluated(
            &builtin_struve_h(&[i(0), Value::Symbol("z".into())]).unwrap(),
            "StruveH",
        );
    }

    #[test]
    fn test_arg_count_errors() {
        assert!(builtin_hypergeometric_1f1(&[]).is_err());
        assert!(builtin_appell_f1(&[i(1)]).is_err());
        assert!(builtin_qpochhammer(&[i(1), i(2), i(3), i(4)]).is_err());
    }

    #[test]
    fn test_bessel_hankel() {
        // H_n^(1)(z) = J_n(z) + i Y_n(z).
        close_cx(
            &builtin_bessel_h1(&[i(0), i(1)]).unwrap(),
            0.7651976865579666,
            0.088256964215677,
            1e-9,
        );
        close_cx(
            &builtin_bessel_h1(&[i(1), i(1)]).unwrap(),
            0.4400505857449336,
            -0.7812128213002888,
            1e-9,
        );
        // H_n^(2)(z) = conj H_n^(1)(z) for real z.
        close_cx(
            &builtin_bessel_h2(&[i(0), i(1)]).unwrap(),
            0.7651976865579666,
            -0.088256964215677,
            1e-9,
        );
        close_cx(
            &builtin_bessel_h2(&[i(1), i(1)]).unwrap(),
            0.4400505857449336,
            0.7812128213002888,
            1e-9,
        );
    }

    #[test]
    fn test_chebyshev() {
        // T_3(x) = 4x^3 - 3x, so T_3(1/2) = -1 exactly.
        close(&builtin_chebyshev_t(&[i(3), r(0.5)]).unwrap(), -1.0, 1e-12);
        // T_2(x) = 2x^2 - 1.
        close(&builtin_chebyshev_t(&[i(2), r(0.3)]).unwrap(), -0.82, 1e-12);
        // Non-integer order: T_{1/2}(1/2) = cos(acos(1/2)/2) = sqrt(3)/2.
        close(
            &builtin_chebyshev_t(&[r(0.5), r(0.5)]).unwrap(),
            0.5 * 3.0_f64.sqrt(),
            1e-12,
        );
        // U_3(x) = 8x^3 - 4x, so U_3(1/2) = -1 exactly.
        close(&builtin_chebyshev_u(&[i(3), r(0.5)]).unwrap(), -1.0, 1e-12);
        // U_2(x) = 4x^2 - 1.
        close(&builtin_chebyshev_u(&[i(2), r(0.3)]).unwrap(), -0.64, 1e-12);
    }

    #[test]
    fn test_gegenbauer() {
        // C_n^1(x) = U_n(x).
        close(
            &builtin_gegenbauer_c(&[i(3), i(1), r(0.5)]).unwrap(),
            -1.0,
            1e-12,
        );
        // C_2^a(x) = 2a(a+1) x^2 - a; with a = 3/2 this is 7.5 x^2 - 1.5.
        close(
            &builtin_gegenbauer_c(&[i(2), r(1.5), r(0.4)]).unwrap(),
            -0.3,
            1e-12,
        );
        // d/dx C_2^{3/2}(x) = 15x, so at x = 0.4 the value is 6.
        close(
            &builtin_gegenbauer_cp(&[i(2), r(1.5), r(0.4)]).unwrap(),
            6.0,
            1e-12,
        );
    }

    #[test]
    fn test_jacobi() {
        // P_n^{(0,0)} is the Legendre polynomial: P_3(x) = (5x^3 - 3x)/2.
        close(
            &builtin_jacobi_p(&[i(3), i(0), i(0), r(0.3)]).unwrap(),
            -0.3825,
            1e-12,
        );
        close(
            &builtin_jacobi_p(&[i(2), r(0.5), r(0.5), r(0.3)]).unwrap(),
            -0.4,
            1e-12,
        );
    }

    #[test]
    fn test_spherical_harmonic_y() {
        // Y_0^0 = 1/(2 sqrt(pi)).
        close(
            &builtin_spherical_harmonic_y(&[i(0), i(0), r(0.7), r(0.3)]).unwrap(),
            1.0 / (2.0 * std::f64::consts::PI.sqrt()),
            1e-12,
        );
        // Y_1^0(theta, phi) = sqrt(3/(4 pi)) cos(theta).
        close(
            &builtin_spherical_harmonic_y(&[i(1), i(0), r(0.5), i(0)]).unwrap(),
            0.4287890441418358,
            1e-12,
        );
        // Y_1^1(pi/2, pi/2) = -sqrt(3/(8 pi)) i.
        close_cx(
            &builtin_spherical_harmonic_y(&[
                i(1),
                i(1),
                r(std::f64::consts::FRAC_PI_2),
                r(std::f64::consts::FRAC_PI_2),
            ])
            .unwrap(),
            0.0,
            -0.3454941494713355,
            1e-12,
        );
        // Y_l^{-m} = (-1)^m conj Y_l^m.
        close_cx(
            &builtin_spherical_harmonic_y(&[i(2), i(-1), r(0.8), r(0.2)]).unwrap(),
            {
                let up = builtin_spherical_harmonic_y(&[i(2), i(1), r(0.8), r(0.2)]).unwrap();
                match up {
                    Value::Complex { re, .. } => -re,
                    _ => panic!(),
                }
            },
            {
                let up = builtin_spherical_harmonic_y(&[i(2), i(1), r(0.8), r(0.2)]).unwrap();
                match up {
                    Value::Complex { im, .. } => im,
                    _ => panic!(),
                }
            },
            1e-12,
        );
    }

    #[test]
    fn test_airy_primes() {
        // Ai'(0) = -1/(3^{1/3} Gamma(1/3)).
        close(
            &builtin_airy_ai_prime(&[i(0)]).unwrap(),
            -0.25881940379280677,
            1e-12,
        );
        close(
            &builtin_airy_ai_prime(&[i(1)]).unwrap(),
            -0.15914744129679323,
            1e-9,
        );
        // Bi'(0) = sqrt(3)/(3^{1/3} Gamma(1/3)).
        close(
            &builtin_airy_bi_prime(&[i(0)]).unwrap(),
            0.4482883573538263,
            1e-12,
        );
        close(
            &builtin_airy_bi_prime(&[i(1)]).unwrap(),
            0.9324359333927752,
            1e-8,
        );
    }

    #[test]
    fn test_product_log_alias() {
        assert!(matches!(
            builtin_product_log_check(&[i(0)]).unwrap(),
            Value::Integer(ref n) if *n == 0
        ));
        close(
            &builtin_product_log_check(&[i(1)]).unwrap(),
            0.5671432904097838,
            1e-9,
        );
    }

    /// Evaluate `ProductLog` through the same alias the registry uses.
    fn builtin_product_log_check(args: &[Value]) -> Result<Value, EvalError> {
        super::super::specialfunctions::builtin_lambert_w(args)
    }

    #[test]
    fn test_erf_inv_alias() {
        // The sibling `erf_approx` is accurate to ~1e-7, so allow for that.
        close(
            &super::super::specialfunctions::builtin_inverse_erf(&[r(0.5)]).unwrap(),
            0.4769362762044699,
            1e-6,
        );
        close(
            &super::super::specialfunctions::builtin_inverse_erfc(&[r(0.5)]).unwrap(),
            0.4769362762044699,
            1e-6,
        );
    }

    #[test]
    fn test_new_symbolic_fallbacks() {
        let s = Value::Symbol("z".to_string());
        is_unevaluated(&builtin_bessel_h1(&[i(0), s.clone()]).unwrap(), "BesselH1");
        is_unevaluated(
            &builtin_chebyshev_t(&[s.clone(), s.clone()]).unwrap(),
            "ChebyshevT",
        );
        is_unevaluated(&builtin_airy_ai_prime(&[s.clone()]).unwrap(), "AiryAiPrime");
        // |m| > l is out of range.
        is_unevaluated(
            &builtin_spherical_harmonic_y(&[i(1), i(2), r(0.5), i(0)]).unwrap(),
            "SphericalHarmonicY",
        );
    }

    #[test]
    fn test_jacobi_eigen_exact_for_small_matrix() {
        let m = vec![
            vec![4.0, 1.0, 0.0],
            vec![1.0, 3.0, 1.0],
            vec![0.0, 1.0, 2.0],
        ];
        let (ev, evec) = jacobi_eigen_sym(&m);
        let mut sorted = ev.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // Characteristic roots: 3 ± √3 and 3.
        assert!((sorted[0] - (3.0 - 3.0_f64.sqrt())).abs() < 1e-10);
        assert!((sorted[1] - 3.0).abs() < 1e-10);
        assert!((sorted[2] - (3.0 + 3.0_f64.sqrt())).abs() < 1e-10);
        for (i, e) in ev.iter().enumerate() {
            let mut r = 0.0;
            for row in 0..3 {
                let mut av = 0.0;
                for c in 0..3 {
                    av += m[row][c] * evec[i][c];
                }
                r += (av - e * evec[i][row]).abs();
            }
            assert!(r < 1e-10, "residual {r}");
        }
    }

    #[test]
    fn test_gauss_laguerre_moments() {
        for n in [4usize, 16, 64] {
            let (nodes, weights) = gauss_laguerre(n);
            let sum: f64 = weights.iter().sum();
            let m1: f64 = nodes.iter().zip(&weights).map(|(x, w)| x * w).sum();
            assert!((sum - 1.0).abs() < 1e-9);
            assert!((m1 - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn test_hypergeometric_u() {
        // U(1, 2, z) = 1/z.
        close(
            &builtin_hypergeometric_u(&[r(1.0), r(2.0), r(4.0)]).unwrap(),
            0.25,
            1e-9,
        );
        // U(2, 3, z) = 1/z².
        close(
            &builtin_hypergeometric_u(&[r(2.0), r(3.0), r(2.0)]).unwrap(),
            0.25,
            1e-9,
        );
        // U(1, 1, 1) = e·E₁(1).
        close(
            &builtin_hypergeometric_u(&[r(1.0), r(1.0), r(1.0)]).unwrap(),
            0.596347362323194,
            1e-9,
        );
        // U(1, 1, z) = e^z E₁(z): a small-parameter numeric sanity check.
        let want = 0.5_f64.exp() * 0.5597735947761608;
        close(
            &builtin_hypergeometric_u(&[r(1.0), r(1.0), r(0.5)]).unwrap(),
            want,
            1e-7,
        );
    }

    /// Evaluate `builtin_meijer_g` on a structured argument, expecting a real.
    fn meijer_num(a_up: &[f64], a_low: &[f64], b_up: &[f64], b_low: &[f64], z: f64) -> f64 {
        let grp = |up: &[f64], low: &[f64]| {
            Value::List(vec![
                Value::List(up.iter().map(|&x| r(x)).collect()),
                Value::List(low.iter().map(|&x| r(x)).collect()),
            ])
        };
        let v = builtin_meijer_g(&[grp(a_up, a_low), grp(b_up, b_low), r(z)]).unwrap();
        match v {
            Value::Real(x) => x.to_f64(),
            other => panic!("expected Real, got {other:?}"),
        }
    }

    #[test]
    fn test_meijer_g_closed_forms() {
        // G^{1,0}_{0,1}(z|0) = e^{-z}.
        for &z in &[0.3_f64, 1.0, 2.5] {
            assert!((meijer_num(&[], &[], &[0.0], &[], z) - (-z).exp()).abs() < 1e-11);
        }
        // G^{1,0}_{0,1}(z|1/2) = sqrt(z) e^{-z}.
        assert!((meijer_num(&[], &[], &[0.5], &[], 4.0) - 2.0 * (-4.0_f64).exp()).abs() < 1e-11);
        // G^{1,1}_{1,1}(z|0;0) = 1/(1+z).
        assert!((meijer_num(&[0.0], &[], &[0.0], &[], 0.5) - 1.0 / 1.5).abs() < 1e-11);
        // General rational form G^{1,1}_{1,1}(z|a;b) = Γ(1-a+b) z^b (1+z)^{a-1-b}.
        let want = gamma_fn(1.5) * 0.3_f64.powf(1.0) * 1.3_f64.powf(-1.5);
        assert!((meijer_num(&[0.5], &[], &[1.0], &[], 0.3) - want).abs() < 1e-11);
        // G^{1,2}_{2,2}(z|0,0;0,0) = 1/(1+z).
        assert!((meijer_num(&[0.0, 0.0], &[], &[0.0], &[0.0], 0.3) - 1.0 / 1.3).abs() < 1e-11);
        // G^{1,2}_{2,2}(z|1,1;1,0) = Log[1+z].
        assert!((meijer_num(&[1.0, 1.0], &[], &[1.0], &[0.0], 0.3) - 1.3_f64.ln()).abs() < 1e-11);
        // Resonant G^{1,1}_{1,1}(z|1;0): the Γ(0) parameter pole is infinite.
        assert!(meijer_num(&[1.0], &[], &[0.0], &[], 0.3).is_infinite());
    }

    #[test]
    fn test_meijer_g_bessel_forms() {
        // G^{1,0}_{0,2}(z|0;0) = J₀(2 Sqrt[z]).
        let j = meijer_num(&[], &[], &[0.0], &[0.0], 0.09);
        assert!((j - 0.9120048634972108).abs() < 1e-11, "got {j}");
        // G^{2,0}_{0,2}(z|0,0) = 2 K₀(2 Sqrt[z]).
        let k = meijer_num(&[], &[], &[0.0, 0.0], &[], 0.25);
        assert!((k - 0.8420488764814167).abs() < 1e-10, "got {k}");
        // G^{2,0}_{0,2}(z|0,1) = 2 z^{1/2} K₁(2 Sqrt[z]) = K₁(1) at z = 1/4.
        let k1 = meijer_num(&[], &[], &[0.0, 1.0], &[], 0.25);
        assert!((k1 - 0.6019072301972346).abs() < 1e-10, "got {k1}");
        // Gaussian: G^{2,0}_{0,2}(z|0,1/2) = Sqrt[Pi] e^{-2 Sqrt[z]}.
        let g = meijer_num(&[], &[], &[0.0, 0.5], &[], 0.25);
        assert!(
            (g - std::f64::consts::PI.sqrt() * (-1.0_f64).exp()).abs() < 1e-10,
            "got {g}"
        );
        // Negative argument for the Bessel-J form (quadrature path): J₀(2i√0.3) = I₀(2√0.3).
        let jn = meijer_num(&[], &[], &[0.0], &[0.0], -0.3);
        assert!((jn - 1.3232642326649002).abs() < 1e-9, "got {jn}");
    }

    #[test]
    fn test_meijer_g_incomplete_gamma() {
        // G^{2,0}_{1,2}(z | 1; 0, a) = Γ(a, z); a = 1/2 gives Sqrt[Pi] Erfc[Sqrt[z]].
        let v = meijer_num(&[], &[1.0], &[0.0, 0.5], &[], 0.3);
        assert!((v - 0.7773593112498081).abs() < 1e-10, "got {v}");
        // Γ(2, z) = (1+z) e^{-z}.
        let v2 = meijer_num(&[], &[1.0], &[0.0, 2.0], &[], 0.3);
        assert!((v2 - 1.3 * (-0.3_f64).exp()).abs() < 1e-10, "got {v2}");
    }

    #[test]
    fn test_meijer_g_symbolic_forms() {
        // Exact symbolic outputs for a free variable, checked numerically.
        assert!((n_at("MeijerG[{{}, {}}, {{0}, {}}, z]", 0.5) - (-0.5_f64).exp()).abs() < 1e-10);
        assert!(
            (n_at("MeijerG[{{}, {}}, {{0}, {0}}, z]", 0.09) - 0.9120048634972108).abs() < 1e-10
        );
        assert!((n_at("MeijerG[{{0}, {}}, {{0}, {}}, z]", 0.5) - 1.0 / 1.5).abs() < 1e-10);
        assert!((n_at("MeijerG[{{1, 1}, {}}, {{1}, {0}}, z]", 0.3) - 1.3_f64.ln()).abs() < 1e-10);
        // Bessel relation with the scaled argument: J₀(2 Sqrt[z²/4]) = J₀(z).
        assert!(
            (n_at("MeijerG[{{}, {}}, {{0}, {0}}, z^2/4]", 0.6) - 0.9120048634972108).abs() < 1e-10
        );
        // Incomplete gamma: Γ(1/2, z) = Sqrt[Pi] Erfc[Sqrt[z]].
        assert!(
            (n_at("MeijerG[{{}, {1}}, {{0, 1/2}, {}}, z]", 0.3) - 0.7773593112498081).abs() < 1e-10
        );
    }

    #[test]
    fn test_hypergeometric_symbolic_values() {
        // 1F1(1;1;z) = E^z.
        assert!((n_at("Hypergeometric1F1[1, 1, z]", 1.0) - std::f64::consts::E).abs() < 1e-10);
        // 1F1(1;2;z) = (E^z - 1)/z.
        assert!(
            (n_at("Hypergeometric1F1[1, 2, z]", 1.0) - (std::f64::consts::E - 1.0)).abs() < 1e-10
        );
        // 2F1(1,1;2;z) = -Log[1-z]/z, at z = -1 gives Log[2].
        assert!(
            (n_at("Hypergeometric2F1[1, 1, 2, z]", -1.0) - std::f64::consts::LN_2).abs() < 1e-10
        );
        // 2F1(1/2,1;3/2;z) = ArcTanh[Sqrt[z]]/Sqrt[z].
        let z = 0.25_f64;
        let want = 0.5 * ((1.0 + z.sqrt()) / (1.0 - z.sqrt())).ln() / z.sqrt();
        assert!((n_at("Hypergeometric2F1[1/2, 1, 3/2, z]", z) - want).abs() < 1e-10);
        // 2F1(1/2,1/2;3/2;z) = ArcSin[Sqrt[z]]/Sqrt[z].
        let want2 = z.sqrt().asin() / z.sqrt();
        assert!((n_at("Hypergeometric2F1[1/2, 1/2, 3/2, z]", z) - want2).abs() < 1e-10);
        // 0F1(1;z) = BesselI[0, 2 Sqrt[z]].
        assert!((n_at("Hypergeometric0F1[1, z]", 0.09) - 1.0920453643173393).abs() < 1e-10);
        // 0F1(1;-z) = BesselJ[0, 2 Sqrt[z]].
        assert!((n_at("Hypergeometric0F1[1, -z]", 0.09) - 0.9120048634972108).abs() < 1e-10);
        // PFQ[{1},{2},z] routes through the confluent specialisation.
        assert!(
            (n_at("HypergeometricPFQ[{1}, {2}, z]", 1.0) - (std::f64::consts::E - 1.0)).abs()
                < 1e-10
        );
        // PFQ and 1F1 agree at a negative argument too.
        let pfq = n_at("HypergeometricPFQ[{1}, {2}, z]", -5.0);
        let kum = n_at("Hypergeometric1F1[1, 2, z]", -5.0);
        assert!((pfq - kum).abs() < 1e-12);
    }

    /// Evaluate `expr` symbolically, substitute `z -> val`, and evaluate the
    /// result numerically: exercises the exact symbolic specialisations.
    fn n_at(expr: &str, val: f64) -> f64 {
        use crate::builtins;
        use crate::env::Env;
        use crate::eval::eval_program;
        use crate::lexer;
        use crate::parser;
        let env = Env::new();
        builtins::register_builtins(&env);
        let tokens = lexer::tokenize(expr).unwrap();
        let ast = parser::parse(tokens).unwrap();
        let symbolic = eval_program(&ast, &env).unwrap();
        let substituted = subst_z(&symbolic, val);
        let inner = crate::eval::table::value_to_expr(&substituted);
        let to_num = |v: Value| -> Option<f64> {
            match v {
                Value::Real(x) => Some(x.to_f64()),
                Value::Integer(n) => Some(n.to_f64()),
                _ => None,
            }
        };
        if let Some(x) = eval_value(&inner, &env).and_then(to_num) {
            return x;
        }
        // Some heads (e.g. `GammaRegularized`) only numericise under `N`.
        let n_expr = crate::ast::Expr::Call {
            head: Box::new(crate::ast::Expr::Symbol("N".to_string())),
            args: vec![inner],
        };
        match eval_value(&n_expr, &env).and_then(to_num) {
            Some(x) => x,
            None => panic!("{expr} did not evaluate numerically"),
        }
    }

    fn eval_value(expr: &crate::ast::Expr, env: &crate::env::Env) -> Option<Value> {
        crate::eval::eval(expr, env).ok()
    }

    /// Replace the symbol `z` by a real value throughout a tree, and lower
    /// exact rationals to values `value_to_expr` understands.
    fn subst_z(v: &Value, val: f64) -> Value {
        match v {
            Value::Symbol(s) if s == "z" => r(val),
            Value::Rational(q) if *q.denom() == 1 => Value::Integer(q.numer().clone()),
            Value::Rational(q) => r(q.to_f64()),
            Value::Call { head, args } => Value::Call {
                head: head.clone(),
                args: args.iter().map(|a| subst_z(a, val)).collect(),
            },
            Value::List(xs) => Value::List(xs.iter().map(|x| subst_z(x, val)).collect()),
            other => other.clone(),
        }
    }

    #[test]
    fn test_mathieu_characteristic_small_q() {
        // a₀(0) = 0, a₁(0) = 1, a₂(0) = 4.
        close(
            &builtin_mathieu_characteristic_a(&[r(0.0), r(0.0)]).unwrap(),
            0.0,
            1e-12,
        );
        close(
            &builtin_mathieu_characteristic_a(&[r(1.0), r(0.0)]).unwrap(),
            1.0,
            1e-12,
        );
        close(
            &builtin_mathieu_characteristic_a(&[r(2.0), r(0.0)]).unwrap(),
            4.0,
            1e-12,
        );
        // a₀(q) ≈ -q²/2 + 7q⁴/128 for small q; b₂(q) ≈ 4 - q²/12.
        close(
            &builtin_mathieu_characteristic_a(&[r(0.0), r(0.1)]).unwrap(),
            -0.005 + 7.0 * 1e-4 / 128.0,
            1e-6,
        );
        close(
            &builtin_mathieu_characteristic_b(&[r(2.0), r(0.1)]).unwrap(),
            4.0 - 0.01 / 12.0,
            1e-6,
        );
    }

    #[test]
    fn test_spheroidal_s1_limit() {
        // At γ = 0 the function is the associated Legendre function.
        close(
            &builtin_spheroidal_s1(&[i(2), i(0), r(0.0), r(0.5)]).unwrap(),
            -0.125,
            1e-9,
        );
        close(
            &builtin_spheroidal_s1(&[i(1), i(1), r(0.0), r(0.5)]).unwrap(),
            -0.8660254037844386,
            1e-9,
        );
    }

    #[test]
    fn test_coulomb_f() {
        // F₀(0, ρ) = sin ρ; F₁(0, ρ) = sin ρ/ρ - cos ρ.
        close(
            &builtin_coulomb_f(&[i(0), r(0.0), r(1.0)]).unwrap(),
            1.0_f64.sin(),
            1e-9,
        );
        close(
            &builtin_coulomb_f(&[i(1), r(0.0), r(1.0)]).unwrap(),
            1.0_f64.sin() - 1.0_f64.cos(),
            1e-9,
        );
    }

    #[test]
    fn test_zernike_r() {
        eq_zernike("ZernikeR[2, 0, 1/2]", -0.5);
        eq_zernike("ZernikeR[4, 2, 1/2]", -0.5);
        // R_3^1(ρ) = 3ρ³ - 2ρ.
        close(
            &builtin_zernike_r(&[i(3), i(1), r(0.5)]).unwrap(),
            -0.625,
            1e-12,
        );
    }

    fn eq_zernike(expr: &str, want: f64) {
        use crate::builtins;
        use crate::env::Env;
        use crate::eval::eval_program;
        use crate::lexer;
        use crate::parser;
        let env = Env::new();
        builtins::register_builtins(&env);
        let tokens = lexer::tokenize(expr).unwrap();
        let ast = parser::parse(tokens).unwrap();
        let v = eval_program(&ast, &env).unwrap();
        if let Value::Real(x) = v {
            assert!((x.to_f64() - want).abs() < 1e-12, "{expr} => {x}");
        } else {
            panic!("{expr} => {v:?}");
        }
    }

    #[test]
    fn test_bessel_general_order() {
        // Reference values from mpmath (20 digits) at order 0.3 and integers.
        close(
            &builtin_bessel_j(&[r(0.3), r(2.0)]).unwrap(),
            0.4256940619814137,
            1e-12,
        );
        close(
            &builtin_bessel_j(&[r(-0.3), r(2.0)]).unwrap(),
            -0.04384707707327879,
            1e-12,
        );
        // Integer order delegates to the sibling module (closed series).
        close(
            &builtin_bessel_j(&[i(0), i(1)]).unwrap(),
            0.7651976865579666,
            1e-12,
        );
        close(
            &builtin_bessel_j(&[i(1), i(1)]).unwrap(),
            0.44005058574493355,
            1e-12,
        );
        close(
            &builtin_bessel_y(&[r(0.3), r(2.0)]).unwrap(),
            0.3634828078260922,
            1e-12,
        );
        close(
            &builtin_bessel_y(&[i(0), i(1)]).unwrap(),
            0.08825696421567696,
            1e-11,
        );
        close(
            &builtin_bessel_y(&[i(1), i(1)]).unwrap(),
            -0.7812128213002887,
            1e-11,
        );
        close(
            &builtin_bessel_i(&[r(0.3), r(2.0)]).unwrap(),
            2.177637989553738,
            1e-12,
        );
        close(
            &builtin_bessel_i(&[i(1), i(1)]).unwrap(),
            0.565159103992485,
            1e-12,
        );
        close(
            &builtin_bessel_k(&[r(0.3), r(2.0)]).unwrap(),
            0.11603697434811926,
            1e-12,
        );
        close(
            &builtin_bessel_k(&[i(0), i(1)]).unwrap(),
            0.42102443824070834,
            1e-12,
        );
        close(
            &builtin_bessel_k(&[i(2), r(1.5)]).unwrap(),
            0.5836559632566508,
            1e-12,
        );
    }

    #[test]
    fn test_bessel_j_zeros() {
        close(
            &builtin_bessel_j_zero(&[i(0), i(1)]).unwrap(),
            2.404825557695773,
            1e-10,
        );
        close(
            &builtin_bessel_j_zero(&[i(1), i(1)]).unwrap(),
            3.8317059702075125,
            1e-10,
        );
        close(
            &builtin_bessel_j_zero(&[i(0), i(2)]).unwrap(),
            5.520078110286311,
            1e-10,
        );
        close(
            &builtin_bessel_j_zero(&[i(2), i(1)]).unwrap(),
            5.135622301840683,
            1e-10,
        );
    }

    #[test]
    fn test_spherical_bessel() {
        close(
            &builtin_spherical_bessel_j(&[i(0), i(1)]).unwrap(),
            0.8414709848078965,
            1e-12,
        );
        close(
            &builtin_spherical_bessel_j(&[i(1), i(1)]).unwrap(),
            0.3011686789397568,
            1e-12,
        );
        close(
            &builtin_spherical_bessel_j(&[i(2), i(1)]).unwrap(),
            0.06203505201137386,
            1e-12,
        );
        close(
            &builtin_spherical_bessel_y(&[i(0), i(1)]).unwrap(),
            -0.5403023058681397,
            1e-12,
        );
    }

    #[test]
    fn test_hermite_laguerre() {
        // H_3(x) = 8x³ − 12x, H_4(x) = 16x⁴ − 48x² + 12.
        close(&builtin_hermite_h(&[i(3), r(0.5)]).unwrap(), -5.0, 1e-12);
        close(&builtin_hermite_h(&[i(4), r(0.5)]).unwrap(), 1.0, 1e-12);
        close(&builtin_hermite_h(&[i(2), r(0.3)]).unwrap(), -1.64, 1e-12);
        // L_2(x) = 1 − 2x + x²/2; L_2^{(1)}(x) = 3 − 3x + x²/2.
        close(&builtin_laguerre_l(&[i(2), r(0.5)]).unwrap(), 0.125, 1e-12);
        close(
            &builtin_laguerre_l(&[i(2), i(1), r(0.5)]).unwrap(),
            1.625,
            1e-12,
        );
        close(
            &builtin_laguerre_l(&[i(3), i(2), r(0.7)]).unwrap(),
            4.167833333333333,
            1e-12,
        );
    }

    #[test]
    fn test_legendre_family() {
        close(
            &builtin_legendre_p(&[i(3), r(0.3)]).unwrap(),
            -0.3825,
            1e-12,
        );
        close(&builtin_legendre_p(&[i(-1), r(0.3)]).unwrap(), 1.0, 1e-12);
        close(
            &builtin_legendre_p(&[i(2), i(1), r(0.4)]).unwrap(),
            -1.0998181667894016,
            1e-12,
        );
        close(
            &builtin_associated_legendre_p(&[i(2), i(1), r(0.4)]).unwrap(),
            -1.0998181667894016,
            1e-12,
        );
        close(
            &builtin_legendre_q(&[i(0), r(0.5)]).unwrap(),
            0.5493061443340548,
            1e-12,
        );
        close(
            &builtin_legendre_q(&[i(2), r(0.3)]).unwrap(),
            -0.5629746555341358,
            1e-12,
        );
        close(
            &builtin_legendre_q(&[i(2), i(1), r(0.4)]).unwrap(),
            1.1925191760257238,
            1e-12,
        );
    }

    #[test]
    fn test_elliptic_integrals() {
        close(
            &builtin_elliptic_k(&[r(0.5)]).unwrap(),
            1.8540746773013719,
            1e-12,
        );
        close(
            &builtin_elliptic_e(&[r(0.5)]).unwrap(),
            1.3506438810476755,
            1e-12,
        );
        close(
            &builtin_elliptic_pi(&[r(0.3), r(0.5)]).unwrap(),
            2.250376821943947,
            1e-12,
        );
        close(
            &builtin_elliptic_f(&[r(0.7), r(0.5)]).unwrap(),
            0.7287703057181902,
            1e-12,
        );
        // EllipticF[phi, 0] = phi.
        close(&builtin_elliptic_f(&[r(0.7), i(0)]).unwrap(), 0.7, 1e-12);
    }

    #[test]
    fn test_jacobi_elliptic() {
        close(
            &builtin_jacobi_sn(&[i(1), r(0.5)]).unwrap(),
            0.8030018248956439,
            1e-12,
        );
        close(
            &builtin_jacobi_cn(&[i(1), r(0.5)]).unwrap(),
            0.5959765676721407,
            1e-12,
        );
        close(
            &builtin_jacobi_dn(&[i(1), r(0.5)]).unwrap(),
            0.8231610016315963,
            1e-12,
        );
        // m = 0 reduces to the circular functions.
        close(
            &builtin_jacobi_sn(&[r(0.7), i(0)]).unwrap(),
            0.7_f64.sin(),
            1e-15,
        );
        close(
            &builtin_jacobi_cn(&[r(0.7), i(0)]).unwrap(),
            0.7_f64.cos(),
            1e-15,
        );
        close(&builtin_jacobi_dn(&[r(0.7), i(0)]).unwrap(), 1.0, 1e-15);
    }

    #[test]
    fn test_elliptic_nome_q() {
        close(
            &builtin_elliptic_nome_q(&[r(0.5)]).unwrap(),
            0.04321391826377225,
            1e-12,
        );
        assert!(matches!(
            builtin_elliptic_nome_q(&[i(0)]).unwrap(),
            Value::Integer(ref n) if *n == 0
        ));
    }

    #[test]
    fn test_elliptic_k_zero_exact() {
        // EllipticK[0] = Pi/2 exactly (symbolic), numeric value π/2.
        let exact = builtin_elliptic_k(&[i(0)]).unwrap();
        assert!(
            !matches!(exact, Value::Real(_)),
            "expected symbolic, got {exact:?}"
        );
        assert!((n_at("EllipticK[0]", 0.0) - std::f64::consts::FRAC_PI_2).abs() < 1e-15);
        assert!((n_at("EllipticE[0]", 0.0) - std::f64::consts::FRAC_PI_2).abs() < 1e-15);
    }

    #[test]
    fn test_env_dispatch_of_new_builtins() {
        // Exercises registration precedence: the general-order Bessel surface
        // must be the one installed in the environment.
        assert!((n_at("BesselJ[0.3, 2.0]", 0.0) - 0.4256940619814137).abs() < 1e-12);
        assert!(
            (n_at("BesselJ[1/2, 1]", 0.0)
                - (2.0f64.sqrt() * 1.0f64.sin()) / std::f64::consts::PI.sqrt())
            .abs()
                < 1e-12
        );
        assert!((n_at("BesselY[1, 1.0]", 0.0) + 0.7812128213002887).abs() < 1e-10);
        assert!((n_at("BesselK[0, 1.0]", 0.0) - 0.42102443824070834).abs() < 1e-12);
        assert!((n_at("HermiteH[3, 0.5]", 0.0) + 5.0).abs() < 1e-12);
        assert!((n_at("LegendreP[3, 0.3]", 0.0) + 0.3825).abs() < 1e-12);
        assert!((n_at("JacobiSN[1, 1/2]", 0.0) - 0.8030018248956439).abs() < 1e-12);
        assert!((n_at("EllipticK[1/2]", 0.0) - 1.8540746773013719).abs() < 1e-12);
    }

    #[test]
    fn test_new_family_symbolic_fallbacks() {
        let s = Value::Symbol("z".to_string());
        is_unevaluated(
            &builtin_bessel_j(&[s.clone(), s.clone()]).unwrap(),
            "BesselJ",
        );
        is_unevaluated(
            &builtin_hermite_h(&[s.clone(), s.clone()]).unwrap(),
            "HermiteH",
        );
        is_unevaluated(
            &builtin_legendre_p(&[s.clone(), s.clone()]).unwrap(),
            "LegendreP",
        );
        is_unevaluated(&builtin_elliptic_k(&[s.clone()]).unwrap(), "EllipticK");
        is_unevaluated(
            &builtin_jacobi_sn(&[s.clone(), s.clone()]).unwrap(),
            "JacobiSN",
        );
    }
}
