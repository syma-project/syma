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

/// Extract the two sub-lists of a `MeijerG` parameter group.
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

/// Hypergeometric0F1[b, z] — `_0F_1(b; z) = Σ z^k / ((b)_k k!)`.
pub fn builtin_hypergeometric_0f1(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Hypergeometric0F1", args, 2)?;
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

/// MeijerG[{{...}, {...}}, {{...}, {...}}, z].
pub fn builtin_meijer_g(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("MeijerG", args, 3)?;
    let (Some((a_up, a_low)), Some((b_up, b_low)), Some(z)) = (
        two_lists(&args[0]),
        two_lists(&args[1]),
        super::to_f64(&args[2]),
    ) else {
        return Ok(unevaluated("MeijerG", args));
    };
    match meijer_g(&a_up, &a_low, &b_up, &b_low, z) {
        Some(v) if v.is_finite() => Ok(super::real(v)),
        _ => Ok(unevaluated("MeijerG", args)),
    }
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
    let (Some(s), Some(a)) = (super::to_f64(&args[0]), super::to_f64(&args[1])) else {
        return Ok(unevaluated("HurwitzZeta", args));
    };
    if (s - 1.0).abs() < 1e-14 {
        return Ok(unevaluated("HurwitzZeta", args));
    }
    if a == 1.0 {
        // ζ(s, 1) = ζ(s); recompute through the same kernel for consistency.
        let v = hurwitz_zeta(s, 1.0);
        return if v.is_finite() {
            Ok(super::real(v))
        } else {
            Ok(unevaluated("HurwitzZeta", args))
        };
    }
    if s <= 0.0 || a <= 0.0 {
        return Ok(unevaluated("HurwitzZeta", args));
    }
    let v = hurwitz_zeta(s, a);
    if v.is_finite() {
        Ok(super::real(v))
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

/// FresnelS[z] = `∫_0^z sin(π t²/2) dt`.
pub fn builtin_fresnel_s(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("FresnelS", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("FresnelS", args));
    };
    let half_pi = 0.5 * std::f64::consts::PI;
    let mut sum = 0.0_f64;
    let mut term = half_pi * z.powi(3) / 3.0; // k = 0 term
    let mut k = 0u32;
    loop {
        sum += term;
        // signed ratio t_{k+1}/t_k = -(π/2)² z⁴ (4k+3) / ((2k+2)(2k+3)(4k+7))
        let ratio = -(half_pi * half_pi) * z.powi(4) * (4 * k + 3) as f64
            / (((2 * k + 2) as f64) * ((2 * k + 3) as f64) * ((4 * k + 7) as f64));
        term *= ratio;
        k += 1;
        if term.abs() < 1e-18 * sum.abs().max(1.0) || k > 500 {
            break;
        }
    }
    Ok(super::real(sum))
}

/// FresnelC[z] = `∫_0^z cos(π t²/2) dt`.
pub fn builtin_fresnel_c(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("FresnelC", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("FresnelC", args));
    };
    let half_pi = 0.5 * std::f64::consts::PI;
    let mut sum = 0.0_f64;
    let mut term = z; // k = 0 term: z/(0!·1)
    let mut k = 0u32;
    loop {
        sum += term;
        // signed ratio t_{k+1}/t_k = -(π/2)² z⁴ · (4k+1) / ((2k+1)(2k+2)(4k+5))
        let ratio = -(half_pi * half_pi) * z.powi(4) * (4 * k + 1) as f64
            / (((2 * k + 1) as f64) * ((2 * k + 2) as f64) * ((4 * k + 5) as f64));
        term *= ratio;
        k += 1;
        if term.abs() < 1e-18 * sum.abs().max(1.0) || k > 500 {
            break;
        }
    }
    Ok(super::real(sum))
}

/// DawsonF[z] = `e^{-z²} ∫_0^z e^{t²} dt`.
pub fn builtin_dawson_f(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("DawsonF", args, 1)?;
    let Some(z) = super::to_f64(&args[0]) else {
        return Ok(unevaluated("DawsonF", args));
    };
    let mut sum = 0.0_f64;
    let mut term = z;
    let mut k = 0u32;
    loop {
        sum += term;
        // t_{k+1} / t_k = -2 z² / (2(k+1)+1).
        term *= -2.0 * z * z / (2 * k + 3) as f64;
        k += 1;
        if term.abs() < 1e-18 * sum.abs().max(1.0) || k > 500 {
            break;
        }
    }
    Ok(super::real(sum))
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

// ── Registration ────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
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
}
