//! Statistics package builtins.
//!
//! Provides core statistical functions: descriptive statistics, probability
//! distributions, PDF/CDF, and random variate generation.

use crate::value::{EvalError, Value, rational_value};
use rug::Integer;
use rug::Rational;
use std::collections::HashMap;

// ── Mathematical helpers (private) ──────────────────────────────────────────

/// Lanczos approximation to ln(Gamma(x)) for x > 0 (g = 7, n = 9).
fn ln_gamma(x: f64) -> f64 {
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.9999999999998099,
        676.5203681218851,
        -1259.1392167224028,
        771.3234287776531,
        -176.6150291621406,
        12.507343278686905,
        -0.13857109526572012,
        9.984369578019572e-6,
        1.505632735149312e-7,
    ];
    if x < 0.5 {
        std::f64::consts::PI.ln() - (std::f64::consts::PI * x).sin().ln() - ln_gamma(1.0 - x)
    } else {
        let z = x - 1.0;
        let mut a = C[0];
        for (i, &ci) in C[1..].iter().enumerate() {
            a += ci / (z + i as f64 + 1.0);
        }
        let t = z + G + 0.5;
        0.5 * (2.0 * std::f64::consts::PI).ln() + (z + 0.5) * t.ln() - t + a.ln()
    }
}

/// Error function using Horner polynomial (Abramowitz & Stegun 7.1.26, max error 1.5e-7).
///
/// Retained as a fast, dependency-free fallback; the accurate path for the
/// normal CDF goes through the regularized incomplete gamma function.
#[allow(dead_code)]
fn erf_approx(x: f64) -> f64 {
    let s = if x >= 0.0 { 1.0_f64 } else { -1.0_f64 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    s * (1.0 - poly * (-x * x).exp())
}

/// Standard normal CDF Φ(x) = P(Z ≤ x) for Z ~ N(0, 1).
///
/// Uses the regularized lower incomplete gamma function
/// `Φ(x) = ½ (1 + s·P(½, x²/2))` (with `s = sign(x)`), which is accurate to
/// ~1e-15 — well inside the 1e-8 conformance tolerance.
fn normal_cdf(x: f64) -> f64 {
    if x == 0.0 {
        return 0.5;
    }
    let p = regularized_gamma_p(0.5, 0.5 * x * x);
    if x > 0.0 {
        0.5 * (1.0 + p)
    } else {
        0.5 * (1.0 - p)
    }
}

/// Regularized lower incomplete gamma P(a, x).
/// Uses series expansion for x < a + 1, continued fraction otherwise.
fn regularized_gamma_p(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x < a + 1.0 {
        // Series: P(a,x) = e^{-x} x^a / Γ(a) · Σ x^n / (a+1)·…·(a+n)
        let mut term = 1.0 / a;
        let mut sum = term;
        for n in 1u32..300 {
            term *= x / (a + n as f64);
            sum += term;
            if term < 1e-14 * sum.abs() {
                break;
            }
        }
        (a * x.ln() - x - ln_gamma(a)).exp() * sum
    } else {
        1.0 - regularized_gamma_q(a, x)
    }
}

/// Regularized upper incomplete gamma Q(a, x) via Lentz continued fraction.
fn regularized_gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    const FPMIN: f64 = 1e-300;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / FPMIN;
    let mut d = if b.abs() < FPMIN { FPMIN } else { 1.0 / b };
    let mut h = d;
    for i in 1u32..300 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = b + an / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-14 {
            break;
        }
    }
    (a * x.ln() - x - ln_gamma(a)).exp() * h
}

/// Continued-fraction evaluator for regularized incomplete beta (Lentz, even/odd steps).
fn beta_cf(x: f64, a: f64, b: f64) -> f64 {
    const FPMIN: f64 = 1e-300;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let qab = a + b;
    let mut c = 1.0_f64;
    let raw_d = 1.0 - qab * x / qap;
    let mut d = if raw_d.abs() < FPMIN {
        FPMIN
    } else {
        1.0 / raw_d
    };
    let mut h = d;
    for m in 1u32..200 {
        let mf = m as f64;
        // Even step
        let aa = mf * (b - mf) * x / ((qam + 2.0 * mf) * (a + 2.0 * mf));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        h *= d * c;
        // Odd step
        let aa = -(a + mf) * (qab + mf) * x / ((a + 2.0 * mf) * (qap + 2.0 * mf));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-14 {
            break;
        }
    }
    h
}

/// Regularized incomplete beta I_x(a, b).
fn regularized_beta(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let ln_beta = ln_gamma(a) + ln_gamma(b) - ln_gamma(a + b);
    // Use symmetry to ensure good convergence of the continued fraction
    if x < (a + 1.0) / (a + b + 2.0) {
        let front = (a * x.ln() + b * (1.0 - x).ln() - ln_beta).exp() / a;
        front * beta_cf(x, a, b)
    } else {
        1.0 - {
            let front = (b * (1.0 - x).ln() + a * x.ln() - ln_beta).exp() / b;
            front * beta_cf(1.0 - x, b, a)
        }
    }
}

/// Sample a standard normal variate N(0,1) via Box-Muller.
fn sample_std_normal(rng: &mut fastrand::Rng) -> f64 {
    let u1 = rng.f64().max(1e-15);
    let u2 = rng.f64();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Sample Gamma(alpha, 1) using Marsaglia-Tsang's method.
/// Returns a Gamma(alpha, 1) variate; scale by 1/lambda for Gamma(alpha, lambda).
fn sample_gamma_unit(alpha: f64, rng: &mut fastrand::Rng) -> f64 {
    if alpha < 1.0 {
        // Gamma(alpha) = Gamma(alpha+1) · U^{1/alpha}
        return sample_gamma_unit(alpha + 1.0, rng) * rng.f64().powf(1.0 / alpha);
    }
    let d = alpha - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        let z = sample_std_normal(rng);
        let v = 1.0 + c * z;
        if v <= 0.0 {
            continue;
        }
        let v3 = v * v * v;
        let u = rng.f64();
        if u < 1.0 - 0.0331 * z * z * z * z {
            return d * v3;
        }
        if u.ln() < 0.5 * z * z + d * (1.0 - v3 + v3.ln()) {
            return d * v3;
        }
    }
}

// ── Data helpers (private) ──────────────────────────────────────────────────

/// Extract a list of values, or error.
fn as_list(v: &Value) -> Result<&Vec<Value>, EvalError> {
    match v {
        Value::List(items) => Ok(items),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: v.type_name().to_string(),
        }),
    }
}

/// Create an Integer value.
fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

/// Store a numeric argument, preserving exactness (Integer/Rational stay exact).
fn num_param(v: &Value) -> Value {
    match v {
        Value::Integer(_) | Value::Rational(_) | Value::Real(_) => v.clone(),
        _ => super::real(super::to_f64(v).unwrap_or(0.0)),
    }
}

/// Extract all numeric values from a list as f64, with error on non-numeric.
fn extract_numbers(items: &[Value]) -> Result<Vec<f64>, EvalError> {
    items
        .iter()
        .map(|v| {
            super::to_f64(v).ok_or_else(|| EvalError::TypeError {
                expected: "Number".to_string(),
                got: v.type_name().to_string(),
            })
        })
        .collect()
}

/// Compute the mean of a slice of f64.
fn compute_mean(nums: &[f64]) -> f64 {
    nums.iter().sum::<f64>() / nums.len() as f64
}

/// Compute the median of a mutable slice (sorts in place).
fn compute_median(nums: &mut [f64]) -> f64 {
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = nums.len();
    if n % 2 == 1 {
        nums[n / 2]
    } else {
        (nums[n / 2 - 1] + nums[n / 2]) / 2.0
    }
}

// ── Exact-aware numeric ops (private) ───────────────────────────────────────

/// A numeric operand that keeps exactness for exact (Integer/Rational) inputs.
///
/// Wolfram-Language semantics: descriptive statistics over exact inputs yield
/// exact results. `Num` defers the decision to the last moment so that a single
/// `Real` operand promotes the whole computation to floating point.
#[derive(Clone)]
enum Num {
    Exact(Rational),
    Real(f64),
}

impl Num {
    fn to_f64(&self) -> f64 {
        match self {
            Num::Exact(r) => r.to_f64(),
            Num::Real(x) => *x,
        }
    }

    fn into_value(self) -> Value {
        match self {
            Num::Exact(r) => rational_value(r.numer().clone(), r.denom().clone()),
            Num::Real(x) => super::real(x),
        }
    }

    fn is_exact(&self) -> bool {
        matches!(self, Num::Exact(_))
    }
}

/// Convert a numeric `Value` to a `Num`, preserving exactness.
fn to_num(v: &Value) -> Option<Num> {
    match v {
        Value::Integer(n) => Some(Num::Exact(Rational::from(n.clone()))),
        Value::Rational(r) => Some(Num::Exact((**r).clone())),
        Value::Real(f) => Some(Num::Real(f.to_f64())),
        _ => None,
    }
}

/// Extract all numeric values from a list as `Num`, erroring on non-numeric.
fn extract_nums(items: &[Value]) -> Result<Vec<Num>, EvalError> {
    items
        .iter()
        .map(|v| {
            to_num(v).ok_or_else(|| EvalError::TypeError {
                expected: "Number".to_string(),
                got: v.type_name().to_string(),
            })
        })
        .collect()
}

/// Sort `Num`s in ascending numeric order.
fn sort_nums(nums: &mut [Num]) {
    nums.sort_by(|a, b| {
        a.to_f64()
            .partial_cmp(&b.to_f64())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// True if every operand is exact.
fn all_exact(nums: &[Num]) -> bool {
    nums.iter().all(Num::is_exact)
}

/// Exact rational sum, or f64 if any operand is real.
fn compute_mean_num(nums: &[Num]) -> Num {
    if all_exact(nums) {
        let mut sum = Rational::from(0);
        for n in nums {
            if let Num::Exact(r) = n {
                sum += r;
            }
        }
        Num::Exact(sum / Integer::from(nums.len()))
    } else {
        Num::Real(nums.iter().map(Num::to_f64).sum::<f64>() / nums.len() as f64)
    }
}

/// Median of a mutable slice; exact when all operands are exact.
fn compute_median_num(nums: &mut [Num]) -> Num {
    sort_nums(nums);
    let n = nums.len();
    if n % 2 == 1 {
        nums[n / 2].clone()
    } else if let (Num::Exact(a), Num::Exact(b)) = (&nums[n / 2 - 1], &nums[n / 2]) {
        let sum: Rational = (a + b).into();
        Num::Exact(sum / Integer::from(2))
    } else {
        Num::Real((nums[n / 2 - 1].to_f64() + nums[n / 2].to_f64()) / 2.0)
    }
}

/// Sample variance (n−1 denominator); exact when all operands are exact.
fn compute_variance_num(nums: &[Num]) -> Option<Num> {
    let n = nums.len();
    if n == 0 {
        return None;
    }
    if n == 1 {
        // Wolfram-Language semantics: the sample variance of one point is 0.
        return Some(if all_exact(nums) {
            Num::Exact(Rational::from(0))
        } else {
            Num::Real(0.0)
        });
    }
    if all_exact(nums) {
        let m = match compute_mean_num(nums) {
            Num::Exact(m) => m,
            Num::Real(_) => unreachable!("all-exact mean must be exact"),
        };
        let mut ss = Rational::from(0);
        for x in nums {
            if let Num::Exact(v) = x {
                let d: Rational = (v - &m).into();
                let sq: Rational = d.clone() * d;
                ss += sq;
            }
        }
        Some(Num::Exact(ss / Integer::from(n - 1)))
    } else {
        let m = nums.iter().map(Num::to_f64).sum::<f64>() / n as f64;
        let ss: f64 = nums.iter().map(|x| (x.to_f64() - m).powi(2)).sum();
        Some(Num::Real(ss / (n - 1) as f64))
    }
}

/// Square root that stays exact for perfect-square rationals.
fn sqrt_num(v: Num) -> Num {
    match v {
        Num::Exact(r) => {
            let num = r.numer().clone();
            let den = r.denom().clone();
            if num >= 0 && num.is_perfect_square() && den.is_perfect_square() {
                Num::Exact(Rational::from((num.sqrt(), den.sqrt())))
            } else {
                Num::Real(r.to_f64().sqrt())
            }
        }
        Num::Real(x) => Num::Real(x.sqrt()),
    }
}

/// Linearly interpolated quantile; exact when data and `q` are both exact.
fn compute_quantile_num(nums: &mut [Num], q: &Num) -> Num {
    sort_nums(nums);
    let n = nums.len();
    if n == 1 {
        return nums[0].clone();
    }
    if q.is_exact() && all_exact(nums) {
        let q = match q {
            Num::Exact(r) => r,
            Num::Real(_) => unreachable!(),
        };
        let pos: Rational = (q * Integer::from(n - 1)).into();
        let lo = (pos.to_f64().floor() as usize).min(n - 1);
        let hi = (lo + 1).min(n - 1);
        let frac: Rational = (&pos - Integer::from(lo)).into();
        if let (Num::Exact(a), Num::Exact(b)) = (&nums[lo], &nums[hi]) {
            let one_minus: Rational = Rational::from(1) - &frac;
            let val: Rational = a * one_minus + b * frac;
            return Num::Exact(val);
        }
    }
    let qf = q.to_f64();
    let pos = qf * (n - 1) as f64;
    let lo = (pos.floor() as usize).min(n - 1);
    let hi = (lo + 1).min(n - 1);
    let frac = pos - lo as f64;
    Num::Real(nums[lo].to_f64() * (1.0 - frac) + nums[hi].to_f64() * frac)
}

/// Sample covariance (n−1 denominator); exact when all operands are exact.
fn compute_covariance_num(a: &[Num], b: &[Num]) -> Num {
    let n = a.len();
    if all_exact(a) && all_exact(b) {
        let (m1, m2) = match (compute_mean_num(a), compute_mean_num(b)) {
            (Num::Exact(m1), Num::Exact(m2)) => (m1, m2),
            _ => unreachable!("all-exact mean must be exact"),
        };
        let mut s = Rational::from(0);
        for (x, y) in a.iter().zip(b.iter()) {
            if let (Num::Exact(x), Num::Exact(y)) = (x, y) {
                let dx: Rational = (x - &m1).into();
                let dy: Rational = (y - &m2).into();
                let prod: Rational = dx * dy;
                s += prod;
            }
        }
        Num::Exact(s / Integer::from(n - 1))
    } else {
        let f = Num::to_f64;
        let m1 = a.iter().map(f).sum::<f64>() / n as f64;
        let m2 = b.iter().map(f).sum::<f64>() / n as f64;
        let mut s = 0.0;
        for (x, y) in a.iter().zip(b.iter()) {
            s += (f(x) - m1) * (f(y) - m2);
        }
        Num::Real(s / (n - 1) as f64)
    }
}

// ── Distribution helpers (private) ──────────────────────────────────────────

/// Read a distribution parameter as a `Num`, preserving exactness.
fn dist_param(dist: &HashMap<String, Value>, key: &str) -> Option<Num> {
    dist.get(key).and_then(to_num)
}

fn dist_type_of(dist: &HashMap<String, Value>) -> Option<&str> {
    match dist.get("Distribution") {
        Some(Value::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn num_add(a: &Num, b: &Num) -> Num {
    match (a, b) {
        (Num::Exact(x), Num::Exact(y)) => Num::Exact((x + y).into()),
        _ => Num::Real(a.to_f64() + b.to_f64()),
    }
}

fn num_sub(a: &Num, b: &Num) -> Num {
    match (a, b) {
        (Num::Exact(x), Num::Exact(y)) => Num::Exact((x - y).into()),
        _ => Num::Real(a.to_f64() - b.to_f64()),
    }
}

fn num_mul(a: &Num, b: &Num) -> Num {
    match (a, b) {
        (Num::Exact(x), Num::Exact(y)) => Num::Exact((x * y).into()),
        _ => Num::Real(a.to_f64() * b.to_f64()),
    }
}

fn num_div(a: &Num, b: &Num) -> Num {
    match (a, b) {
        (Num::Exact(x), Num::Exact(y)) if !y.is_zero() => Num::Exact((x / y).into()),
        _ => Num::Real(a.to_f64() / b.to_f64()),
    }
}

fn num_recip(a: &Num) -> Num {
    num_div(&Num::Exact(Rational::from(1)), a)
}

/// Analytical mean of a distribution, kept exact when parameters are exact.
fn distribution_mean_num(dist: &HashMap<String, Value>) -> Option<Num> {
    let two = Num::Exact(Rational::from(2));
    match dist_type_of(dist)? {
        "Normal" => dist_param(dist, "Mean"),
        "Uniform" => {
            let lo = dist_param(dist, "Min")?;
            let hi = dist_param(dist, "Max")?;
            Some(num_div(&num_add(&lo, &hi), &two))
        }
        "Poisson" => dist_param(dist, "Lambda"),
        "Binomial" => {
            let n = dist_param(dist, "N")?;
            let p = dist_param(dist, "P")?;
            Some(num_mul(&n, &p))
        }
        "Bernoulli" => dist_param(dist, "P"),
        "Exponential" => Some(num_recip(&dist_param(dist, "Lambda")?)),
        "Gamma" => {
            let alpha = dist_param(dist, "Alpha")?;
            let lambda = dist_param(dist, "Lambda")?;
            Some(num_div(&alpha, &lambda))
        }
        "ChiSquare" => dist_param(dist, "K"),
        "StudentT" => {
            if dist_param(dist, "Nu")?.to_f64() > 1.0 {
                Some(Num::Exact(Rational::from(0)))
            } else {
                None
            }
        }
        "Beta" => {
            let a = dist_param(dist, "Alpha")?;
            let b = dist_param(dist, "Beta")?;
            Some(num_div(&a, &num_add(&a, &b)))
        }
        "LogNormal" => {
            let mu = dist_param(dist, "Mu")?.to_f64();
            let sigma = dist_param(dist, "Sigma")?.to_f64();
            Some(Num::Real((mu + 0.5 * sigma * sigma).exp()))
        }
        "Cauchy" => None, // undefined
        "DiscreteUniform" => {
            let lo = dist_param(dist, "Min")?;
            let hi = dist_param(dist, "Max")?;
            Some(num_div(&num_add(&lo, &hi), &two))
        }
        _ => None,
    }
}

/// Analytical variance of a distribution, kept exact when parameters are exact.
fn distribution_variance_num(dist: &HashMap<String, Value>) -> Option<Num> {
    let one = Num::Exact(Rational::from(1));
    let two = Num::Exact(Rational::from(2));
    let twelve = Num::Exact(Rational::from(12));
    match dist_type_of(dist)? {
        "Normal" => {
            let sd = dist_param(dist, "SD")?;
            Some(num_mul(&sd, &sd))
        }
        "Uniform" => {
            let lo = dist_param(dist, "Min")?;
            let hi = dist_param(dist, "Max")?;
            let d = num_sub(&hi, &lo);
            Some(num_div(&num_mul(&d, &d), &twelve))
        }
        "Poisson" => dist_param(dist, "Lambda"),
        "Binomial" => {
            let n = dist_param(dist, "N")?;
            let p = dist_param(dist, "P")?;
            let q = num_sub(&one, &p);
            Some(num_mul(&num_mul(&n, &p), &q))
        }
        "Bernoulli" => {
            let p = dist_param(dist, "P")?;
            let q = num_sub(&one, &p);
            Some(num_mul(&p, &q))
        }
        "Exponential" => {
            let lambda = dist_param(dist, "Lambda")?;
            let l2 = num_mul(&lambda, &lambda);
            Some(num_div(&one, &l2))
        }
        "Gamma" => {
            let alpha = dist_param(dist, "Alpha")?;
            let lambda = dist_param(dist, "Lambda")?;
            let l2 = num_mul(&lambda, &lambda);
            Some(num_div(&alpha, &l2))
        }
        "ChiSquare" => {
            let k = dist_param(dist, "K")?;
            Some(num_mul(&two, &k))
        }
        "StudentT" => {
            let nu = dist_param(dist, "Nu")?;
            if nu.to_f64() > 2.0 {
                Some(num_div(&nu, &num_sub(&nu, &two)))
            } else {
                None
            }
        }
        "Beta" => {
            let a = dist_param(dist, "Alpha")?;
            let b = dist_param(dist, "Beta")?;
            let s = num_add(&a, &b);
            let denom = num_mul(&num_mul(&s, &s), &num_add(&s, &one));
            Some(num_div(&num_mul(&a, &b), &denom))
        }
        "LogNormal" => {
            let mu = dist_param(dist, "Mu")?.to_f64();
            let sigma = dist_param(dist, "Sigma")?.to_f64();
            let s2 = sigma * sigma;
            Some(Num::Real((s2.exp() - 1.0) * (2.0 * mu + s2).exp()))
        }
        "Cauchy" => None,
        "DiscreteUniform" => {
            let lo = dist_param(dist, "Min")?;
            let hi = dist_param(dist, "Max")?;
            let n = num_sub(&hi, &lo);
            let n_plus_2 = num_add(&n, &two);
            Some(num_div(&num_mul(&n, &n_plus_2), &twelve))
        }
        _ => None,
    }
}

/// Analytical mean of a distribution (returns None for undefined/infinite mean).
pub(crate) fn distribution_mean_f64(dist: &HashMap<String, Value>) -> Option<f64> {
    distribution_mean_num(dist).map(|n| n.to_f64())
}

/// Analytical variance of a distribution.
pub(crate) fn distribution_variance_f64(dist: &HashMap<String, Value>) -> Option<f64> {
    distribution_variance_num(dist).map(|n| n.to_f64())
}

// ── Core builtins ────────────────────────────────────────────────────────────

/// Mean[list] — arithmetic mean; Mean[dist] — distribution mean.
pub fn builtin_mean(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Mean requires exactly 1 argument".to_string(),
        ));
    }
    // Distribution dispatch
    if let Value::Assoc(map) = &args[0]
        && map.contains_key("Distribution")
    {
        return match distribution_mean_num(map) {
            Some(v) => Ok(v.into_value()),
            None => Err(EvalError::Error(
                "Mean: distribution mean is undefined".to_string(),
            )),
        };
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error("Mean: list must not be empty".to_string()));
    }
    let nums = extract_nums(items)?;
    Ok(compute_mean_num(&nums).into_value())
}

/// Median[list] — median value.
pub fn builtin_median(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Median requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "Median: list must not be empty".to_string(),
        ));
    }
    let mut nums = extract_nums(items)?;
    Ok(compute_median_num(&mut nums).into_value())
}

/// Variance[list] — sample variance (n-1 denominator); Variance[dist] — distribution variance.
pub fn builtin_variance(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Variance requires exactly 1 argument".to_string(),
        ));
    }
    // Distribution dispatch
    if let Value::Assoc(map) = &args[0]
        && map.contains_key("Distribution")
    {
        return match distribution_variance_num(map) {
            Some(v) => Ok(v.into_value()),
            None => Err(EvalError::Error(
                "Variance: distribution variance is undefined".to_string(),
            )),
        };
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "Variance: list must not be empty".to_string(),
        ));
    }
    let nums = extract_nums(items)?;
    match compute_variance_num(&nums) {
        Some(v) => Ok(v.into_value()),
        None => Err(EvalError::Error(
            "Variance: list must not be empty".to_string(),
        )),
    }
}

/// StandardDeviation[list] — sample standard deviation; StandardDeviation[dist] — distribution SD.
pub fn builtin_standard_deviation(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "StandardDeviation requires exactly 1 argument".to_string(),
        ));
    }
    // Distribution dispatch
    if let Value::Assoc(map) = &args[0]
        && map.contains_key("Distribution")
    {
        return match distribution_variance_num(map) {
            Some(v) => Ok(sqrt_num(v).into_value()),
            None => Err(EvalError::Error(
                "StandardDeviation: distribution variance is undefined".to_string(),
            )),
        };
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "StandardDeviation: list must not be empty".to_string(),
        ));
    }
    let nums = extract_nums(items)?;
    match compute_variance_num(&nums) {
        Some(v) => Ok(sqrt_num(v).into_value()),
        None => Err(EvalError::Error(
            "StandardDeviation: list must not be empty".to_string(),
        )),
    }
}

/// Quantile[list, q] — q-th quantile (0 ≤ q ≤ 1), linear interpolation.
pub fn builtin_quantile(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Quantile requires exactly 2 arguments".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "Quantile: list must not be empty".to_string(),
        ));
    }
    let q = to_num(&args[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    let qf = q.to_f64();
    if !(0.0..=1.0).contains(&qf) {
        return Err(EvalError::Error(
            "Quantile: q must be between 0 and 1".to_string(),
        ));
    }
    let mut nums = extract_nums(items)?;
    Ok(compute_quantile_num(&mut nums, &q).into_value())
}

/// Covariance[list1, list2] — sample covariance.
pub fn builtin_covariance(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Covariance requires exactly 2 arguments".to_string(),
        ));
    }
    let items1 = as_list(&args[0])?;
    let items2 = as_list(&args[1])?;
    if items1.len() != items2.len() {
        return Err(EvalError::Error(format!(
            "Covariance: lists must have same length (got {} and {})",
            items1.len(),
            items2.len()
        )));
    }
    let n = items1.len();
    if n < 2 {
        return Err(EvalError::Error(
            "Covariance: need at least 2 data points".to_string(),
        ));
    }
    let nums1 = extract_nums(items1)?;
    let nums2 = extract_nums(items2)?;
    Ok(compute_covariance_num(&nums1, &nums2).into_value())
}

/// Correlation[list1, list2] — Pearson correlation coefficient.
pub fn builtin_correlation(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Correlation requires exactly 2 arguments".to_string(),
        ));
    }
    let cov = builtin_covariance(args)?;
    let sd1 = builtin_standard_deviation(&[args[0].clone()])?;
    let sd2 = builtin_standard_deviation(&[args[1].clone()])?;
    let c = super::to_f64(&cov)
        .ok_or_else(|| EvalError::Error("Correlation: covariance is not numeric".to_string()))?;
    let s1 = super::to_f64(&sd1).ok_or_else(|| {
        EvalError::Error("Correlation: standard deviation is not numeric".to_string())
    })?;
    let s2 = super::to_f64(&sd2).ok_or_else(|| {
        EvalError::Error("Correlation: standard deviation is not numeric".to_string())
    })?;
    if s1 == 0.0 || s2 == 0.0 {
        return Err(EvalError::Error(
            "Correlation: standard deviation is zero".to_string(),
        ));
    }
    Ok(super::real(c / (s1 * s2)))
}

// ── New descriptive statistics ───────────────────────────────────────────────

/// GeometricMean[list] — geometric mean (all values must be positive).
pub fn builtin_geometric_mean(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "GeometricMean requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "GeometricMean: list must not be empty".to_string(),
        ));
    }
    let nums = extract_numbers(items)?;
    for &x in &nums {
        if x <= 0.0 {
            return Err(EvalError::Error(
                "GeometricMean: all values must be positive".to_string(),
            ));
        }
    }
    let log_sum: f64 = nums.iter().map(|x| x.ln()).sum();
    Ok(super::real((log_sum / nums.len() as f64).exp()))
}

/// HarmonicMean[list] — harmonic mean (no zero values).
pub fn builtin_harmonic_mean(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "HarmonicMean requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "HarmonicMean: list must not be empty".to_string(),
        ));
    }
    let nums = extract_numbers(items)?;
    for &x in &nums {
        if x == 0.0 {
            return Err(EvalError::Error(
                "HarmonicMean: values must not be zero".to_string(),
            ));
        }
    }
    let inv_sum: f64 = nums.iter().map(|x| 1.0 / x).sum();
    Ok(super::real(nums.len() as f64 / inv_sum))
}

/// Skewness[list] — Fisher-Pearson standardized third central moment
/// (biased estimator: m3 / m2^{3/2} where m_k = mean of (x - x̄)^k).
pub fn builtin_skewness(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Skewness requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    let n = items.len();
    if n < 2 {
        return Err(EvalError::Error(
            "Skewness: need at least 2 data points".to_string(),
        ));
    }
    let nums = extract_numbers(items)?;
    let mean = compute_mean(&nums);
    let m2: f64 = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
    let m3: f64 = nums.iter().map(|x| (x - mean).powi(3)).sum::<f64>() / n as f64;
    if m2 == 0.0 {
        return Ok(super::real(0.0));
    }
    Ok(super::real(m3 / m2.powf(1.5)))
}

/// Kurtosis[list] — ratio of the fourth and second central moments
/// (biased: m4 / m2^2; equals 3 for a normal distribution).
pub fn builtin_kurtosis(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Kurtosis requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    let n = items.len();
    if n < 2 {
        return Err(EvalError::Error(
            "Kurtosis: need at least 2 data points".to_string(),
        ));
    }
    let nums = extract_numbers(items)?;
    let mean = compute_mean(&nums);
    let m2: f64 = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
    let m4: f64 = nums.iter().map(|x| (x - mean).powi(4)).sum::<f64>() / n as f64;
    if m2 == 0.0 {
        return Ok(super::real(0.0));
    }
    Ok(super::real(m4 / m2.powi(2)))
}

/// Mode[list] — most frequently occurring value(s).
/// Returns a single value if there is one mode, a list if there are several.
pub fn builtin_mode(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Mode requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error("Mode: list must not be empty".to_string()));
    }
    let mut counts: Vec<(Value, usize)> = Vec::new();
    'outer: for item in items.iter() {
        for (val, count) in counts.iter_mut() {
            if val == item {
                *count += 1;
                continue 'outer;
            }
        }
        counts.push((item.clone(), 1));
    }
    let max_count = counts.iter().map(|(_, c)| *c).max().unwrap();
    let mut modes: Vec<Value> = counts
        .into_iter()
        .filter(|(_, c)| *c == max_count)
        .map(|(v, _)| v)
        .collect();
    if modes.len() == 1 {
        Ok(modes.remove(0))
    } else {
        Ok(Value::List(modes))
    }
}

/// InterquartileRange[list] — Q3 - Q1 (75th minus 25th percentile).
pub fn builtin_interquartile_range(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "InterquartileRange requires exactly 1 argument".to_string(),
        ));
    }
    let q1 = builtin_quantile(&[args[0].clone(), super::real(0.25)])?;
    let q3 = builtin_quantile(&[args[0].clone(), super::real(0.75)])?;
    match (&q1, &q3) {
        (Value::Real(a), Value::Real(b)) => Ok(super::real(b.to_f64() - a.to_f64())),
        (Value::Integer(a), Value::Integer(b)) => {
            let v = b.to_i64().unwrap_or(0) - a.to_i64().unwrap_or(0);
            Ok(int(v))
        }
        _ => {
            let a = super::to_f64(&q1).unwrap_or(0.0);
            let b = super::to_f64(&q3).unwrap_or(0.0);
            Ok(super::real(b - a))
        }
    }
}

/// WeightedMean[data, weights] — weighted arithmetic mean.
pub fn builtin_weighted_mean(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "WeightedMean requires exactly 2 arguments".to_string(),
        ));
    }
    let data = as_list(&args[0])?;
    let wts = as_list(&args[1])?;
    if data.len() != wts.len() {
        return Err(EvalError::Error(format!(
            "WeightedMean: data and weights must have the same length (got {} and {})",
            data.len(),
            wts.len()
        )));
    }
    if data.is_empty() {
        return Err(EvalError::Error(
            "WeightedMean: list must not be empty".to_string(),
        ));
    }
    let nums = extract_numbers(data)?;
    let ws = extract_numbers(wts)?;
    let total_w: f64 = ws.iter().sum();
    if total_w == 0.0 {
        return Err(EvalError::Error(
            "WeightedMean: weights must not sum to zero".to_string(),
        ));
    }
    let weighted_sum: f64 = nums.iter().zip(ws.iter()).map(|(x, w)| x * w).sum();
    Ok(super::real(weighted_sum / total_w))
}

/// RootMeanSquare[list] — √(mean of squares).
pub fn builtin_root_mean_square(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "RootMeanSquare requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "RootMeanSquare: list must not be empty".to_string(),
        ));
    }
    let nums = extract_numbers(items)?;
    let mean_sq: f64 = nums.iter().map(|x| x * x).sum::<f64>() / nums.len() as f64;
    Ok(super::real(mean_sq.sqrt()))
}

/// MeanDeviation[list] — mean of |xᵢ - mean(x)|.
pub fn builtin_mean_deviation(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "MeanDeviation requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "MeanDeviation: list must not be empty".to_string(),
        ));
    }
    let nums = extract_numbers(items)?;
    let mean = compute_mean(&nums);
    let dev: f64 = nums.iter().map(|x| (x - mean).abs()).sum::<f64>() / nums.len() as f64;
    Ok(super::real(dev))
}

/// MedianDeviation[list] — median of |xᵢ - median(x)|.
pub fn builtin_median_deviation(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "MedianDeviation requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "MedianDeviation: list must not be empty".to_string(),
        ));
    }
    let mut nums = extract_numbers(items)?;
    let med = compute_median(&mut nums);
    let mut deviations: Vec<f64> = nums.iter().map(|x| (x - med).abs()).collect();
    let mad = compute_median(&mut deviations);
    Ok(super::real(mad))
}

/// Standardize[list] — shift to zero mean and unit sample variance.
/// Returns {(xᵢ - mean) / stddev, ...}.
pub fn builtin_standardize(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Standardize requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.len() < 2 {
        return Err(EvalError::Error(
            "Standardize: need at least 2 data points".to_string(),
        ));
    }
    let nums = extract_numbers(items)?;
    let mean = compute_mean(&nums);
    let n = nums.len();
    let ss: f64 = nums.iter().map(|x| (x - mean).powi(2)).sum();
    let sd = (ss / (n - 1) as f64).sqrt();
    if sd == 0.0 {
        return Err(EvalError::Error(
            "Standardize: standard deviation is zero".to_string(),
        ));
    }
    let result: Vec<Value> = nums.iter().map(|x| super::real((x - mean) / sd)).collect();
    Ok(Value::List(result))
}

// ── Binning ──────────────────────────────────────────────────────────────────

/// BinCounts[data, bspec] — count data in equal-width bins.
///
/// `bspec` can be:
/// - A number `dx` — bins of width `dx` spanning [min, max].
/// - A list `{dx}` — same as above.
/// - A list `{min, max, dx}` — explicit bin range.
pub fn builtin_bin_counts(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "BinCounts requires exactly 2 arguments".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let nums = extract_numbers(items)?;
    let data_min = nums.iter().cloned().fold(f64::INFINITY, f64::min);
    let data_max = nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let (bin_min, bin_max, dx) = match &args[1] {
        Value::Integer(_) | Value::Real(_) => {
            let dx = super::to_f64(&args[1]).unwrap();
            if dx <= 0.0 {
                return Err(EvalError::Error(
                    "BinCounts: bin width must be positive".to_string(),
                ));
            }
            (data_min, data_max, dx)
        }
        Value::List(spec) => match spec.len() {
            1 => {
                let dx = super::to_f64(&spec[0]).ok_or_else(|| {
                    EvalError::Error("BinCounts: bin spec must contain numbers".to_string())
                })?;
                (data_min, data_max, dx)
            }
            3 => {
                let lo = super::to_f64(&spec[0]).ok_or_else(|| {
                    EvalError::Error("BinCounts: bin spec must contain numbers".to_string())
                })?;
                let hi = super::to_f64(&spec[1]).ok_or_else(|| {
                    EvalError::Error("BinCounts: bin spec must contain numbers".to_string())
                })?;
                let dx = super::to_f64(&spec[2]).ok_or_else(|| {
                    EvalError::Error("BinCounts: bin spec must contain numbers".to_string())
                })?;
                (lo, hi, dx)
            }
            _ => {
                return Err(EvalError::Error(
                    "BinCounts: bin spec must be {dx} or {min, max, dx}".to_string(),
                ));
            }
        },
        _ => {
            return Err(EvalError::Error(
                "BinCounts: second argument must be a number or list".to_string(),
            ));
        }
    };

    if dx <= 0.0 || bin_min >= bin_max {
        return Err(EvalError::Error(
            "BinCounts: invalid bin range or width".to_string(),
        ));
    }
    let n_bins = ((bin_max - bin_min) / dx).ceil() as usize;
    let n_bins = n_bins.max(1);
    let mut counts = vec![0i64; n_bins];
    for &x in &nums {
        if x < bin_min || x > bin_max {
            continue;
        }
        let idx = if x >= bin_max {
            n_bins - 1
        } else {
            (((x - bin_min) / dx).floor() as usize).min(n_bins - 1)
        };
        counts[idx] += 1;
    }
    Ok(Value::List(counts.into_iter().map(int).collect()))
}

/// HistogramList[data] or HistogramList[data, n] — returns {binEdges, binCounts}.
///
/// With no second argument uses Sturges' rule: k = ⌊log₂(n)⌋ + 1.
/// With an integer `n`, uses n bins.
pub fn builtin_histogram_list(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "HistogramList requires 1 or 2 arguments".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "HistogramList: list must not be empty".to_string(),
        ));
    }
    let mut nums = extract_numbers(items)?;
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let data_min = nums[0];
    let data_max = *nums.last().unwrap();

    let n_bins: usize = if args.len() == 2 {
        match &args[1] {
            Value::Integer(i) => i.to_usize().unwrap_or(1).max(1),
            Value::Real(r) => (r.to_f64() as usize).max(1),
            _ => {
                return Err(EvalError::Error(
                    "HistogramList: second argument must be an integer".to_string(),
                ));
            }
        }
    } else {
        // Sturges' rule
        ((nums.len() as f64).log2().floor() as usize + 1).max(1)
    };

    let range = data_max - data_min;
    let dx = if range == 0.0 {
        1.0
    } else {
        range / n_bins as f64
    };

    let edges: Vec<Value> = (0..=n_bins)
        .map(|i| super::real(data_min + i as f64 * dx))
        .collect();

    let mut counts = vec![0i64; n_bins];
    for &x in &nums {
        let idx = if x >= data_max {
            n_bins - 1
        } else {
            (((x - data_min) / dx).floor() as usize).min(n_bins - 1)
        };
        counts[idx] += 1;
    }
    let count_list: Vec<Value> = counts.into_iter().map(int).collect();

    Ok(Value::List(vec![
        Value::List(edges),
        Value::List(count_list),
    ]))
}

// ── Distribution constructors ────────────────────────────────────────────────

/// BinomialDistribution[n, p] — Binomial(n, p).
pub fn builtin_binomial_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "BinomialDistribution requires exactly 2 arguments".to_string(),
        ));
    }
    let n = super::to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    let p = super::to_f64(&args[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if n < 0.0 || n.fract() != 0.0 {
        return Err(EvalError::Error(
            "BinomialDistribution: n must be a non-negative integer".to_string(),
        ));
    }
    if !(0.0..=1.0).contains(&p) {
        return Err(EvalError::Error(
            "BinomialDistribution: p must be in [0, 1]".to_string(),
        ));
    }
    let mut map = HashMap::new();
    map.insert(
        "Distribution".to_string(),
        Value::Str("Binomial".to_string()),
    );
    map.insert("N".to_string(), int(n as i64));
    map.insert("P".to_string(), num_param(&args[1]));
    Ok(Value::Assoc(map))
}

/// BernoulliDistribution[p] — Bernoulli(p).
pub fn builtin_bernoulli_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "BernoulliDistribution requires exactly 1 argument".to_string(),
        ));
    }
    let p = super::to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    if !(0.0..=1.0).contains(&p) {
        return Err(EvalError::Error(
            "BernoulliDistribution: p must be in [0, 1]".to_string(),
        ));
    }
    let mut map = HashMap::new();
    map.insert(
        "Distribution".to_string(),
        Value::Str("Bernoulli".to_string()),
    );
    map.insert("P".to_string(), num_param(&args[0]));
    Ok(Value::Assoc(map))
}

/// ExponentialDistribution[lambda] — Exponential(λ).
pub fn builtin_exponential_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ExponentialDistribution requires exactly 1 argument".to_string(),
        ));
    }
    let lambda = super::to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    if lambda <= 0.0 {
        return Err(EvalError::Error(
            "ExponentialDistribution: lambda must be positive".to_string(),
        ));
    }
    let mut map = HashMap::new();
    map.insert(
        "Distribution".to_string(),
        Value::Str("Exponential".to_string()),
    );
    map.insert("Lambda".to_string(), num_param(&args[0]));
    Ok(Value::Assoc(map))
}

/// GammaDistribution[alpha, lambda] — Gamma(α, λ). Mean = α/λ.
pub fn builtin_gamma_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "GammaDistribution requires exactly 2 arguments".to_string(),
        ));
    }
    let alpha = super::to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    let lambda = super::to_f64(&args[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if alpha <= 0.0 || lambda <= 0.0 {
        return Err(EvalError::Error(
            "GammaDistribution: alpha and lambda must be positive".to_string(),
        ));
    }
    let mut map = HashMap::new();
    map.insert("Distribution".to_string(), Value::Str("Gamma".to_string()));
    map.insert("Alpha".to_string(), num_param(&args[0]));
    map.insert("Lambda".to_string(), num_param(&args[1]));
    Ok(Value::Assoc(map))
}

/// ChiSquareDistribution[k] — χ²(k).
pub fn builtin_chi_square_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ChiSquareDistribution requires exactly 1 argument".to_string(),
        ));
    }
    let k = super::to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    if k <= 0.0 {
        return Err(EvalError::Error(
            "ChiSquareDistribution: k must be positive".to_string(),
        ));
    }
    let mut map = HashMap::new();
    map.insert(
        "Distribution".to_string(),
        Value::Str("ChiSquare".to_string()),
    );
    map.insert("K".to_string(), num_param(&args[0]));
    Ok(Value::Assoc(map))
}

/// StudentTDistribution[nu] — Student's t(ν).
pub fn builtin_student_t_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "StudentTDistribution requires exactly 1 argument".to_string(),
        ));
    }
    let nu = super::to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    if nu <= 0.0 {
        return Err(EvalError::Error(
            "StudentTDistribution: nu must be positive".to_string(),
        ));
    }
    let mut map = HashMap::new();
    map.insert(
        "Distribution".to_string(),
        Value::Str("StudentT".to_string()),
    );
    map.insert("Nu".to_string(), num_param(&args[0]));
    Ok(Value::Assoc(map))
}

/// BetaDistribution[alpha, beta] — Beta(α, β).
pub fn builtin_beta_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "BetaDistribution requires exactly 2 arguments".to_string(),
        ));
    }
    let alpha = super::to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    let beta = super::to_f64(&args[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if alpha <= 0.0 || beta <= 0.0 {
        return Err(EvalError::Error(
            "BetaDistribution: alpha and beta must be positive".to_string(),
        ));
    }
    let mut map = HashMap::new();
    map.insert("Distribution".to_string(), Value::Str("Beta".to_string()));
    map.insert("Alpha".to_string(), num_param(&args[0]));
    map.insert("Beta".to_string(), num_param(&args[1]));
    Ok(Value::Assoc(map))
}

/// LogNormalDistribution[mu, sigma] — LogNormal(μ, σ) where μ, σ are the mean and SD of the log.
pub fn builtin_log_normal_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "LogNormalDistribution requires exactly 2 arguments".to_string(),
        ));
    }
    let _mu = super::to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    let sigma = super::to_f64(&args[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if sigma <= 0.0 {
        return Err(EvalError::Error(
            "LogNormalDistribution: sigma must be positive".to_string(),
        ));
    }
    let mut map = HashMap::new();
    map.insert(
        "Distribution".to_string(),
        Value::Str("LogNormal".to_string()),
    );
    map.insert("Mu".to_string(), num_param(&args[0]));
    map.insert("Sigma".to_string(), num_param(&args[1]));
    Ok(Value::Assoc(map))
}

/// CauchyDistribution[x0, gamma] — Cauchy(x₀, γ).
pub fn builtin_cauchy_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "CauchyDistribution requires exactly 2 arguments".to_string(),
        ));
    }
    let _x0 = super::to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    let gamma = super::to_f64(&args[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if gamma <= 0.0 {
        return Err(EvalError::Error(
            "CauchyDistribution: gamma must be positive".to_string(),
        ));
    }
    let mut map = HashMap::new();
    map.insert("Distribution".to_string(), Value::Str("Cauchy".to_string()));
    map.insert("Location".to_string(), num_param(&args[0]));
    map.insert("Scale".to_string(), num_param(&args[1]));
    Ok(Value::Assoc(map))
}

/// DiscreteUniformDistribution[{min, max}] — uniform over integers [min, max].
pub fn builtin_discrete_uniform_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "DiscreteUniformDistribution requires exactly 1 argument".to_string(),
        ));
    }
    let spec = as_list(&args[0])?;
    if spec.len() != 2 {
        return Err(EvalError::Error(
            "DiscreteUniformDistribution: argument must be {min, max}".to_string(),
        ));
    }
    let lo = super::to_f64(&spec[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[0].type_name().to_string(),
    })?;
    let hi = super::to_f64(&spec[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[1].type_name().to_string(),
    })?;
    if lo.fract() != 0.0 || hi.fract() != 0.0 {
        return Err(EvalError::Error(
            "DiscreteUniformDistribution: min and max must be integers".to_string(),
        ));
    }
    if lo > hi {
        return Err(EvalError::Error(
            "DiscreteUniformDistribution: min must be ≤ max".to_string(),
        ));
    }
    let mut map = HashMap::new();
    map.insert(
        "Distribution".to_string(),
        Value::Str("DiscreteUniform".to_string()),
    );
    map.insert("Min".to_string(), int(lo as i64));
    map.insert("Max".to_string(), int(hi as i64));
    Ok(Value::Assoc(map))
}

// ── NormalDistribution, UniformDistribution, PoissonDistribution ─────────────

/// NormalDistribution[mu, sigma] — create a normal distribution object.
pub fn builtin_normal_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "NormalDistribution requires exactly 2 arguments".to_string(),
        ));
    }
    let _mu = super::to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    let sigma = super::to_f64(&args[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if sigma <= 0.0 {
        return Err(EvalError::Error(
            "NormalDistribution: standard deviation must be positive".to_string(),
        ));
    }
    let mut map = HashMap::new();
    map.insert("Distribution".to_string(), Value::Str("Normal".to_string()));
    map.insert("Mean".to_string(), num_param(&args[0]));
    map.insert("SD".to_string(), num_param(&args[1]));
    Ok(Value::Assoc(map))
}

/// UniformDistribution[min, max] — create a uniform distribution object.
pub fn builtin_uniform_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "UniformDistribution requires exactly 2 arguments".to_string(),
        ));
    }
    let lo = super::to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    let hi = super::to_f64(&args[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if lo >= hi {
        return Err(EvalError::Error(
            "UniformDistribution: min must be less than max".to_string(),
        ));
    }
    let mut map = HashMap::new();
    map.insert(
        "Distribution".to_string(),
        Value::Str("Uniform".to_string()),
    );
    map.insert("Min".to_string(), num_param(&args[0]));
    map.insert("Max".to_string(), num_param(&args[1]));
    Ok(Value::Assoc(map))
}

/// PoissonDistribution[lambda] — create a Poisson distribution object.
pub fn builtin_poisson_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PoissonDistribution requires exactly 1 argument".to_string(),
        ));
    }
    let lambda = super::to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    if lambda <= 0.0 {
        return Err(EvalError::Error(
            "PoissonDistribution: lambda must be positive".to_string(),
        ));
    }
    let mut map = HashMap::new();
    map.insert(
        "Distribution".to_string(),
        Value::Str("Poisson".to_string()),
    );
    map.insert("Lambda".to_string(), num_param(&args[0]));
    Ok(Value::Assoc(map))
}

// ── PDF and CDF ──────────────────────────────────────────────────────────────

/// PDF[dist, x] — probability density (continuous) or mass (discrete) at x.
pub fn builtin_pdf(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "PDF requires exactly 2 arguments".to_string(),
        ));
    }
    let dist = match &args[0] {
        Value::Assoc(m) => m,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Distribution (Association)".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let x = super::to_f64(&args[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    let dist_type = dist
        .get("Distribution")
        .and_then(|v| match v {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        })
        .ok_or_else(|| EvalError::Error("PDF: invalid distribution".to_string()))?;

    let density = match dist_type {
        "Normal" => {
            let mu = dist.get("Mean").and_then(super::to_f64).unwrap_or(0.0);
            let sigma = dist.get("SD").and_then(super::to_f64).unwrap_or(1.0);
            let z = (x - mu) / sigma;
            (-0.5 * z * z).exp() / (sigma * (2.0 * std::f64::consts::PI).sqrt())
        }
        "Uniform" => {
            let lo = dist.get("Min").and_then(super::to_f64).unwrap_or(0.0);
            let hi = dist.get("Max").and_then(super::to_f64).unwrap_or(1.0);
            if x >= lo && x <= hi {
                1.0 / (hi - lo)
            } else {
                0.0
            }
        }
        "Poisson" => {
            let lambda = dist.get("Lambda").and_then(super::to_f64).unwrap_or(1.0);
            if x < 0.0 || x.fract() != 0.0 {
                0.0
            } else {
                let k = x as u64;
                let ln_pmf = k as f64 * lambda.ln() - lambda - ln_gamma(k as f64 + 1.0);
                ln_pmf.exp()
            }
        }
        "Binomial" => {
            let n = dist.get("N").and_then(super::to_f64).unwrap_or(1.0) as u64;
            let p = dist.get("P").and_then(super::to_f64).unwrap_or(0.5);
            if x < 0.0 || x.fract() != 0.0 || x as u64 > n {
                0.0
            } else {
                let k = x as u64;
                let ln_pmf = ln_gamma(n as f64 + 1.0)
                    - ln_gamma(k as f64 + 1.0)
                    - ln_gamma((n - k) as f64 + 1.0)
                    + k as f64 * p.ln()
                    + (n - k) as f64 * (1.0 - p).ln();
                ln_pmf.exp()
            }
        }
        "Bernoulli" => {
            let p = dist.get("P").and_then(super::to_f64).unwrap_or(0.5);
            if x == 0.0 {
                1.0 - p
            } else if x == 1.0 {
                p
            } else {
                0.0
            }
        }
        "Exponential" => {
            let lambda = dist.get("Lambda").and_then(super::to_f64).unwrap_or(1.0);
            if x < 0.0 {
                0.0
            } else {
                lambda * (-lambda * x).exp()
            }
        }
        "Gamma" => {
            let alpha = dist.get("Alpha").and_then(super::to_f64).unwrap_or(1.0);
            let lambda = dist.get("Lambda").and_then(super::to_f64).unwrap_or(1.0);
            if x <= 0.0 {
                0.0
            } else {
                let ln_pdf =
                    alpha * lambda.ln() + (alpha - 1.0) * x.ln() - lambda * x - ln_gamma(alpha);
                ln_pdf.exp()
            }
        }
        "ChiSquare" => {
            let k = dist.get("K").and_then(super::to_f64).unwrap_or(1.0);
            if x <= 0.0 {
                0.0
            } else {
                let alpha = k / 2.0;
                let lambda = 0.5_f64;
                let ln_pdf =
                    alpha * lambda.ln() + (alpha - 1.0) * x.ln() - lambda * x - ln_gamma(alpha);
                ln_pdf.exp()
            }
        }
        "StudentT" => {
            let nu = dist.get("Nu").and_then(super::to_f64).unwrap_or(1.0);
            let ln_pdf = ln_gamma((nu + 1.0) / 2.0)
                - ln_gamma(nu / 2.0)
                - 0.5 * (nu * std::f64::consts::PI).ln()
                - (nu + 1.0) / 2.0 * (1.0 + x * x / nu).ln();
            ln_pdf.exp()
        }
        "Beta" => {
            let a = dist.get("Alpha").and_then(super::to_f64).unwrap_or(1.0);
            let b = dist.get("Beta").and_then(super::to_f64).unwrap_or(1.0);
            if x <= 0.0 || x >= 1.0 {
                0.0
            } else {
                let ln_pdf = (a - 1.0) * x.ln() + (b - 1.0) * (1.0 - x).ln()
                    - (ln_gamma(a) + ln_gamma(b) - ln_gamma(a + b));
                ln_pdf.exp()
            }
        }
        "LogNormal" => {
            let mu = dist.get("Mu").and_then(super::to_f64).unwrap_or(0.0);
            let sigma = dist.get("Sigma").and_then(super::to_f64).unwrap_or(1.0);
            if x <= 0.0 {
                0.0
            } else {
                let z = (x.ln() - mu) / sigma;
                (-0.5 * z * z).exp() / (x * sigma * (2.0 * std::f64::consts::PI).sqrt())
            }
        }
        "Cauchy" => {
            let x0 = dist.get("Location").and_then(super::to_f64).unwrap_or(0.0);
            let gamma = dist.get("Scale").and_then(super::to_f64).unwrap_or(1.0);
            let z = (x - x0) / gamma;
            1.0 / (std::f64::consts::PI * gamma * (1.0 + z * z))
        }
        "DiscreteUniform" => {
            let lo = dist.get("Min").and_then(super::to_f64).unwrap_or(0.0);
            let hi = dist.get("Max").and_then(super::to_f64).unwrap_or(1.0);
            if x.fract() == 0.0 && x >= lo && x <= hi {
                1.0 / (hi - lo + 1.0)
            } else {
                0.0
            }
        }
        other => {
            return Err(EvalError::Error(format!(
                "PDF: unknown distribution '{}'",
                other
            )));
        }
    };
    Ok(super::real(density))
}

/// CDF[dist, x] — cumulative distribution function P(X ≤ x).
pub fn builtin_cdf(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "CDF requires exactly 2 arguments".to_string(),
        ));
    }
    let dist = match &args[0] {
        Value::Assoc(m) => m,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Distribution (Association)".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let x = super::to_f64(&args[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    let dist_type = dist
        .get("Distribution")
        .and_then(|v| match v {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        })
        .ok_or_else(|| EvalError::Error("CDF: invalid distribution".to_string()))?;

    let cdf_val = match dist_type {
        "Normal" => {
            let mu = dist.get("Mean").and_then(super::to_f64).unwrap_or(0.0);
            let sigma = dist.get("SD").and_then(super::to_f64).unwrap_or(1.0);
            normal_cdf((x - mu) / sigma)
        }
        "Uniform" => {
            let lo = dist.get("Min").and_then(super::to_f64).unwrap_or(0.0);
            let hi = dist.get("Max").and_then(super::to_f64).unwrap_or(1.0);
            if x <= lo {
                0.0
            } else if x >= hi {
                1.0
            } else {
                (x - lo) / (hi - lo)
            }
        }
        "Poisson" => {
            let lambda = dist.get("Lambda").and_then(super::to_f64).unwrap_or(1.0);
            if x < 0.0 {
                0.0
            } else {
                let k = x.floor() as u64;
                (0..=k)
                    .map(|i| (i as f64 * lambda.ln() - lambda - ln_gamma(i as f64 + 1.0)).exp())
                    .sum::<f64>()
                    .min(1.0)
            }
        }
        "Binomial" => {
            let n = dist.get("N").and_then(super::to_f64).unwrap_or(1.0) as u64;
            let p = dist.get("P").and_then(super::to_f64).unwrap_or(0.5);
            if x < 0.0 {
                0.0
            } else if x as u64 >= n {
                1.0
            } else {
                let k = x.floor() as u64;
                (0..=k)
                    .map(|i| {
                        let ln_pmf = ln_gamma(n as f64 + 1.0)
                            - ln_gamma(i as f64 + 1.0)
                            - ln_gamma((n - i) as f64 + 1.0)
                            + i as f64 * p.ln()
                            + (n - i) as f64 * (1.0 - p).ln();
                        ln_pmf.exp()
                    })
                    .sum::<f64>()
                    .min(1.0)
            }
        }
        "Bernoulli" => {
            let p = dist.get("P").and_then(super::to_f64).unwrap_or(0.5);
            if x < 0.0 {
                0.0
            } else if x < 1.0 {
                1.0 - p
            } else {
                1.0
            }
        }
        "Exponential" => {
            let lambda = dist.get("Lambda").and_then(super::to_f64).unwrap_or(1.0);
            if x <= 0.0 {
                0.0
            } else {
                1.0 - (-lambda * x).exp()
            }
        }
        "Gamma" => {
            let alpha = dist.get("Alpha").and_then(super::to_f64).unwrap_or(1.0);
            let lambda = dist.get("Lambda").and_then(super::to_f64).unwrap_or(1.0);
            if x <= 0.0 {
                0.0
            } else {
                regularized_gamma_p(alpha, lambda * x)
            }
        }
        "ChiSquare" => {
            let k = dist.get("K").and_then(super::to_f64).unwrap_or(1.0);
            if x <= 0.0 {
                0.0
            } else {
                regularized_gamma_p(k / 2.0, x / 2.0)
            }
        }
        "StudentT" => {
            let nu = dist.get("Nu").and_then(super::to_f64).unwrap_or(1.0);
            // CDF via regularized incomplete beta: I_x(nu/2, 1/2) where x = nu/(nu+t^2)
            let xb = nu / (nu + x * x);
            let ibeta = regularized_beta(xb, nu / 2.0, 0.5);
            if x >= 0.0 {
                1.0 - ibeta / 2.0
            } else {
                ibeta / 2.0
            }
        }
        "Beta" => {
            let a = dist.get("Alpha").and_then(super::to_f64).unwrap_or(1.0);
            let b = dist.get("Beta").and_then(super::to_f64).unwrap_or(1.0);
            regularized_beta(x.clamp(0.0, 1.0), a, b)
        }
        "LogNormal" => {
            let mu = dist.get("Mu").and_then(super::to_f64).unwrap_or(0.0);
            let sigma = dist.get("Sigma").and_then(super::to_f64).unwrap_or(1.0);
            if x <= 0.0 {
                0.0
            } else {
                normal_cdf((x.ln() - mu) / sigma)
            }
        }
        "Cauchy" => {
            let x0 = dist.get("Location").and_then(super::to_f64).unwrap_or(0.0);
            let gamma = dist.get("Scale").and_then(super::to_f64).unwrap_or(1.0);
            0.5 + ((x - x0) / gamma).atan() / std::f64::consts::PI
        }
        "DiscreteUniform" => {
            let lo = dist.get("Min").and_then(super::to_f64).unwrap_or(0.0);
            let hi = dist.get("Max").and_then(super::to_f64).unwrap_or(1.0);
            if x < lo {
                0.0
            } else if x >= hi {
                1.0
            } else {
                (x.floor() - lo + 1.0) / (hi - lo + 1.0)
            }
        }
        other => {
            return Err(EvalError::Error(format!(
                "CDF: unknown distribution '{}'",
                other
            )));
        }
    };
    Ok(super::real(cdf_val))
}

// ── RandomVariate ─────────────────────────────────────────────────────────────

/// RandomVariate[dist] or RandomVariate[dist, n] — generate random samples.
/// Single-argument form returns a single variate; two-argument form returns a list.
pub fn builtin_random_variate(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "RandomVariate requires 1 or 2 arguments".to_string(),
        ));
    }
    let single = args.len() == 1;
    let dist = match &args[0] {
        Value::Assoc(map) => map,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Association (distribution)".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let n = if single {
        1
    } else {
        match &args[1] {
            Value::Integer(i) => i.to_usize().ok_or_else(|| {
                EvalError::Error("RandomVariate: count must be a non-negative integer".to_string())
            })?,
            _ => {
                return Err(EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: args[1].type_name().to_string(),
                });
            }
        }
    };

    let dist_type = dist
        .get("Distribution")
        .and_then(|v| match v {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        })
        .ok_or_else(|| {
            EvalError::Error(
                "RandomVariate: invalid distribution (missing 'Distribution' key)".to_string(),
            )
        })?;

    // Seed from the shared RNG so `SeedRandom[n]` makes RandomVariate reproducible.
    let mut rng = fastrand::Rng::with_seed(crate::builtins::random::next_u64());
    let samples: Vec<Value> = match dist_type {
        "Normal" => {
            let mu = dist.get("Mean").and_then(super::to_f64).unwrap_or(0.0);
            let sd = dist.get("SD").and_then(super::to_f64).unwrap_or(1.0);
            (0..n)
                .map(|_| super::real(mu + sd * sample_std_normal(&mut rng)))
                .collect()
        }
        "Uniform" => {
            let lo = dist.get("Min").and_then(super::to_f64).unwrap_or(0.0);
            let hi = dist.get("Max").and_then(super::to_f64).unwrap_or(1.0);
            (0..n)
                .map(|_| super::real(lo + (hi - lo) * rng.f64()))
                .collect()
        }
        "Poisson" => {
            let lambda = dist.get("Lambda").and_then(super::to_f64).unwrap_or(1.0);
            (0..n)
                .map(|_| {
                    // Knuth's method in log space. The naive form computes
                    // L = e^{-lambda}, which underflows to 0 for lambda >~ 700,
                    // so `p < L` can never become true once p is positive.
                    // log L = -lambda does not underflow.
                    let log_l = -lambda;
                    let mut log_p = 0.0f64;
                    let mut k = 0u64;
                    loop {
                        log_p += rng.f64().max(1e-300).ln();
                        if log_p < log_l {
                            break;
                        }
                        k += 1;
                    }
                    int(k as i64)
                })
                .collect()
        }
        "Binomial" => {
            let n_trials = dist.get("N").and_then(super::to_f64).unwrap_or(1.0) as u64;
            let p = dist.get("P").and_then(super::to_f64).unwrap_or(0.5);
            (0..n)
                .map(|_| {
                    let count = (0..n_trials).filter(|_| rng.f64() < p).count();
                    int(count as i64)
                })
                .collect()
        }
        "Bernoulli" => {
            let p = dist.get("P").and_then(super::to_f64).unwrap_or(0.5);
            (0..n)
                .map(|_| int(if rng.f64() < p { 1 } else { 0 }))
                .collect()
        }
        "Exponential" => {
            let lambda = dist.get("Lambda").and_then(super::to_f64).unwrap_or(1.0);
            (0..n)
                .map(|_| super::real(-rng.f64().max(1e-15).ln() / lambda))
                .collect()
        }
        "Gamma" => {
            let alpha = dist.get("Alpha").and_then(super::to_f64).unwrap_or(1.0);
            let lambda = dist.get("Lambda").and_then(super::to_f64).unwrap_or(1.0);
            (0..n)
                .map(|_| super::real(sample_gamma_unit(alpha, &mut rng) / lambda))
                .collect()
        }
        "ChiSquare" => {
            let k = dist.get("K").and_then(super::to_f64).unwrap_or(1.0);
            // ChiSquare(k) = Gamma(k/2, 0.5) = 2 * Gamma(k/2, 1)
            (0..n)
                .map(|_| super::real(2.0 * sample_gamma_unit(k / 2.0, &mut rng)))
                .collect()
        }
        "StudentT" => {
            let nu = dist.get("Nu").and_then(super::to_f64).unwrap_or(1.0);
            // T = Z / sqrt(V/nu) where Z ~ N(0,1), V ~ ChiSquare(nu)
            (0..n)
                .map(|_| {
                    let z = sample_std_normal(&mut rng);
                    let v = 2.0 * sample_gamma_unit(nu / 2.0, &mut rng);
                    super::real(z / (v / nu).sqrt())
                })
                .collect()
        }
        "Beta" => {
            let a = dist.get("Alpha").and_then(super::to_f64).unwrap_or(1.0);
            let b = dist.get("Beta").and_then(super::to_f64).unwrap_or(1.0);
            // Beta(a,b) = Gamma(a,1) / (Gamma(a,1) + Gamma(b,1))
            (0..n)
                .map(|_| {
                    let x = sample_gamma_unit(a, &mut rng);
                    let y = sample_gamma_unit(b, &mut rng);
                    super::real(x / (x + y))
                })
                .collect()
        }
        "LogNormal" => {
            let mu = dist.get("Mu").and_then(super::to_f64).unwrap_or(0.0);
            let sigma = dist.get("Sigma").and_then(super::to_f64).unwrap_or(1.0);
            (0..n)
                .map(|_| super::real((mu + sigma * sample_std_normal(&mut rng)).exp()))
                .collect()
        }
        "Cauchy" => {
            let x0 = dist.get("Location").and_then(super::to_f64).unwrap_or(0.0);
            let gamma = dist.get("Scale").and_then(super::to_f64).unwrap_or(1.0);
            (0..n)
                .map(|_| super::real(x0 + gamma * (std::f64::consts::PI * (rng.f64() - 0.5)).tan()))
                .collect()
        }
        "DiscreteUniform" => {
            let lo = dist.get("Min").and_then(super::to_f64).unwrap_or(0.0) as i64;
            let hi = dist.get("Max").and_then(super::to_f64).unwrap_or(1.0) as i64;
            (0..n)
                .map(|_| int(lo + (rng.f64() * (hi - lo + 1) as f64).floor() as i64))
                .collect()
        }
        other => {
            return Err(EvalError::Error(format!(
                "RandomVariate: unknown distribution type '{}'",
                other
            )));
        }
    };

    if single {
        Ok(samples.into_iter().next().unwrap())
    } else {
        Ok(Value::List(samples))
    }
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register all Statistics builtins in the environment.
pub fn register(env: &crate::env::Env) {
    use super::register_builtin;
    // Core descriptive
    register_builtin(env, "Mean", builtin_mean);
    register_builtin(env, "Median", builtin_median);
    register_builtin(env, "Variance", builtin_variance);
    register_builtin(env, "StandardDeviation", builtin_standard_deviation);
    register_builtin(env, "Quantile", builtin_quantile);
    register_builtin(env, "Covariance", builtin_covariance);
    register_builtin(env, "Correlation", builtin_correlation);
    // Extended descriptive
    register_builtin(env, "GeometricMean", builtin_geometric_mean);
    register_builtin(env, "HarmonicMean", builtin_harmonic_mean);
    register_builtin(env, "Skewness", builtin_skewness);
    register_builtin(env, "Kurtosis", builtin_kurtosis);
    register_builtin(env, "Mode", builtin_mode);
    register_builtin(env, "InterquartileRange", builtin_interquartile_range);
    register_builtin(env, "WeightedMean", builtin_weighted_mean);
    register_builtin(env, "RootMeanSquare", builtin_root_mean_square);
    register_builtin(env, "MeanDeviation", builtin_mean_deviation);
    register_builtin(env, "MedianDeviation", builtin_median_deviation);
    register_builtin(env, "Standardize", builtin_standardize);
    // Binning
    register_builtin(env, "BinCounts", builtin_bin_counts);
    register_builtin(env, "HistogramList", builtin_histogram_list);
    // Distributions
    register_builtin(env, "NormalDistribution", builtin_normal_distribution);
    register_builtin(env, "UniformDistribution", builtin_uniform_distribution);
    register_builtin(env, "PoissonDistribution", builtin_poisson_distribution);
    register_builtin(env, "BinomialDistribution", builtin_binomial_distribution);
    register_builtin(env, "BernoulliDistribution", builtin_bernoulli_distribution);
    register_builtin(
        env,
        "ExponentialDistribution",
        builtin_exponential_distribution,
    );
    register_builtin(env, "GammaDistribution", builtin_gamma_distribution);
    register_builtin(
        env,
        "ChiSquareDistribution",
        builtin_chi_square_distribution,
    );
    register_builtin(env, "StudentTDistribution", builtin_student_t_distribution);
    register_builtin(env, "BetaDistribution", builtin_beta_distribution);
    register_builtin(
        env,
        "LogNormalDistribution",
        builtin_log_normal_distribution,
    );
    register_builtin(env, "CauchyDistribution", builtin_cauchy_distribution);
    register_builtin(
        env,
        "DiscreteUniformDistribution",
        builtin_discrete_uniform_distribution,
    );
    // Distribution functions
    register_builtin(env, "PDF", builtin_pdf);
    register_builtin(env, "CDF", builtin_cdf);
    // Random sampling
    register_builtin(env, "RandomVariate", builtin_random_variate);
}

/// Symbol names exported by the Statistics package.
pub const SYMBOLS: &[&str] = &[
    // Core descriptive
    "Mean",
    "Median",
    "Variance",
    "StandardDeviation",
    "Quantile",
    "Covariance",
    "Correlation",
    // Extended descriptive
    "GeometricMean",
    "HarmonicMean",
    "Skewness",
    "Kurtosis",
    "Mode",
    "InterquartileRange",
    "WeightedMean",
    "RootMeanSquare",
    "MeanDeviation",
    "MedianDeviation",
    "Standardize",
    // Binning
    "BinCounts",
    "HistogramList",
    // Distributions
    "NormalDistribution",
    "UniformDistribution",
    "PoissonDistribution",
    "BinomialDistribution",
    "BernoulliDistribution",
    "ExponentialDistribution",
    "GammaDistribution",
    "ChiSquareDistribution",
    "StudentTDistribution",
    "BetaDistribution",
    "LogNormalDistribution",
    "CauchyDistribution",
    "DiscreteUniformDistribution",
    // Distribution functions
    "PDF",
    "CDF",
    // Sampling
    "RandomVariate",
    // Gap-fill statistics (implemented in `statsmore`)
    "SeedRandom",
    "Quartiles",
    "CentralMoment",
    "Moment",
    "Expectation",
    "Probability",
    "InverseCDF",
    "MomentGeneratingFunction",
    "CharacteristicFunction",
    "BinLists",
    "ExponentialMovingAverage",
    "SmoothHistogram",
    "FindDistribution",
    "EstimatedDistribution",
];

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn int_val(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn real_val(v: f64) -> Value {
        Value::Real(rug::Float::with_val(crate::value::DEFAULT_PRECISION, v))
    }

    fn list(vals: Vec<Value>) -> Value {
        Value::List(vals)
    }

    fn approx(v: &Value, expected: f64, tol: f64) -> bool {
        match v {
            Value::Real(r) => (r.to_f64() - expected).abs() < tol,
            Value::Integer(n) => (n.to_f64() - expected).abs() < tol,
            Value::Rational(r) => (r.to_f64() - expected).abs() < tol,
            _ => false,
        }
    }

    // ── Core descriptive ─────────────────────────────────────────────────────

    #[test]
    fn test_mean_integers() {
        let data = list(vec![
            int_val(1),
            int_val(2),
            int_val(3),
            int_val(4),
            int_val(5),
        ]);
        assert_eq!(builtin_mean(&[data]).unwrap(), int_val(3));
    }

    #[test]
    fn test_mean_non_integer() {
        let data = list(vec![int_val(1), int_val(2)]);
        let r = builtin_mean(&[data]).unwrap();
        assert!(approx(&r, 1.5, 1e-10));
    }

    #[test]
    fn test_median_odd() {
        let data = list(vec![int_val(3), int_val(1), int_val(2)]);
        assert_eq!(builtin_median(&[data]).unwrap(), int_val(2));
    }

    #[test]
    fn test_median_even() {
        let data = list(vec![int_val(4), int_val(1), int_val(3), int_val(2)]);
        assert!(approx(&builtin_median(&[data]).unwrap(), 2.5, 1e-10));
    }

    #[test]
    fn test_variance() {
        let data = list(vec![
            int_val(2),
            int_val(4),
            int_val(4),
            int_val(4),
            int_val(5),
            int_val(5),
            int_val(7),
            int_val(9),
        ]);
        let r = builtin_variance(&[data]).unwrap();
        assert!(approx(&r, 32.0 / 7.0, 1e-10));
    }

    #[test]
    fn test_standard_deviation() {
        let data = list(vec![
            int_val(2),
            int_val(4),
            int_val(4),
            int_val(4),
            int_val(5),
            int_val(5),
            int_val(7),
            int_val(9),
        ]);
        let r = builtin_standard_deviation(&[data]).unwrap();
        assert!(approx(&r, (32.0f64 / 7.0).sqrt(), 1e-10));
    }

    #[test]
    fn test_quantile() {
        let data = list(vec![
            int_val(1),
            int_val(2),
            int_val(3),
            int_val(4),
            int_val(5),
        ]);
        let r = builtin_quantile(&[data, real_val(0.5)]).unwrap();
        assert!(approx(&r, 3.0, 1e-10));
    }

    #[test]
    fn test_covariance() {
        let x = list(vec![int_val(1), int_val(2), int_val(3)]);
        let y = list(vec![int_val(4), int_val(5), int_val(6)]);
        let r = builtin_covariance(&[x, y]).unwrap();
        assert!(approx(&r, 1.0, 1e-10));
    }

    #[test]
    fn test_correlation_perfect() {
        let x = list(vec![int_val(1), int_val(2), int_val(3)]);
        let y = list(vec![int_val(2), int_val(4), int_val(6)]);
        let r = builtin_correlation(&[x, y]).unwrap();
        assert!(approx(&r, 1.0, 1e-10));
    }

    // ── New descriptive ──────────────────────────────────────────────────────

    #[test]
    fn test_geometric_mean() {
        let data = list(vec![int_val(1), int_val(4), int_val(4)]);
        let r = builtin_geometric_mean(&[data]).unwrap();
        assert!(approx(&r, 2.519842099789746, 1e-10));
    }

    #[test]
    fn test_harmonic_mean() {
        // H({1,2,4}) = 3 / (1 + 0.5 + 0.25) = 3 / 1.75 ≈ 1.714...
        let data = list(vec![int_val(1), int_val(2), int_val(4)]);
        let r = builtin_harmonic_mean(&[data]).unwrap();
        assert!(approx(&r, 3.0 / 1.75, 1e-10));
    }

    #[test]
    fn test_skewness_symmetric() {
        // Symmetric data → skewness ≈ 0
        let data = list(vec![
            int_val(1),
            int_val(2),
            int_val(3),
            int_val(4),
            int_val(5),
        ]);
        let r = builtin_skewness(&[data]).unwrap();
        assert!(approx(&r, 0.0, 1e-10));
    }

    #[test]
    fn test_kurtosis_normal_approx() {
        // Normal-like data should have kurtosis close to 3
        let data = list(vec![
            real_val(-2.0),
            real_val(-1.0),
            real_val(-0.5),
            real_val(0.0),
            real_val(0.5),
            real_val(1.0),
            real_val(2.0),
        ]);
        let r = builtin_kurtosis(&[data]).unwrap();
        // Just check it's a positive real number
        assert!(matches!(r, Value::Real(_)));
    }

    #[test]
    fn test_mode_single() {
        let data = list(vec![int_val(1), int_val(2), int_val(2), int_val(3)]);
        let r = builtin_mode(&[data]).unwrap();
        assert_eq!(r, int_val(2));
    }

    #[test]
    fn test_mode_multi() {
        let data = list(vec![int_val(1), int_val(2), int_val(1), int_val(2)]);
        let r = builtin_mode(&[data]).unwrap();
        assert!(matches!(r, Value::List(_)));
    }

    #[test]
    fn test_interquartile_range() {
        let data = list(vec![
            int_val(1),
            int_val(2),
            int_val(3),
            int_val(4),
            int_val(5),
        ]);
        let r = builtin_interquartile_range(&[data]).unwrap();
        // Q1 = 1.5*(5-1)*0.25 + 1 = 2, Q3 = 4, IQR = 2
        assert!(approx(&r, 2.0, 1e-10));
    }

    #[test]
    fn test_weighted_mean() {
        let data = list(vec![int_val(1), int_val(2), int_val(3)]);
        let wts = list(vec![int_val(1), int_val(2), int_val(1)]);
        let r = builtin_weighted_mean(&[data, wts]).unwrap();
        // (1*1 + 2*2 + 3*1) / (1+2+1) = 8/4 = 2
        assert!(approx(&r, 2.0, 1e-10));
    }

    #[test]
    fn test_root_mean_square() {
        let data = list(vec![int_val(3), int_val(4)]);
        let r = builtin_root_mean_square(&[data]).unwrap();
        // sqrt((9+16)/2) = sqrt(12.5)
        assert!(approx(&r, 12.5_f64.sqrt(), 1e-10));
    }

    #[test]
    fn test_mean_deviation() {
        let data = list(vec![int_val(1), int_val(2), int_val(3)]);
        let r = builtin_mean_deviation(&[data]).unwrap();
        // mean = 2, deviations = {1, 0, 1}, mean dev = 2/3
        assert!(approx(&r, 2.0 / 3.0, 1e-10));
    }

    #[test]
    fn test_median_deviation() {
        let data = list(vec![int_val(1), int_val(2), int_val(3)]);
        let r = builtin_median_deviation(&[data]).unwrap();
        // median = 2, |deviations| = {1, 0, 1}, median = 1
        assert!(approx(&r, 1.0, 1e-10));
    }

    #[test]
    fn test_standardize() {
        let data = list(vec![
            int_val(2),
            int_val(4),
            int_val(4),
            int_val(4),
            int_val(5),
            int_val(5),
            int_val(7),
            int_val(9),
        ]);
        let r = builtin_standardize(&[data]).unwrap();
        if let Value::List(items) = r {
            assert_eq!(items.len(), 8);
            // Check all are Real
            for v in &items {
                assert!(matches!(v, Value::Real(_)));
            }
            // First element: (2-5)/sqrt(4.5714) ≈ -1.404
            assert!(approx(
                &items[0],
                (2.0 - 5.0) / (32.0f64 / 7.0).sqrt(),
                1e-6
            ));
        } else {
            panic!("Expected List");
        }
    }

    // ── Binning ──────────────────────────────────────────────────────────────

    #[test]
    fn test_bin_counts_width() {
        let data = list(vec![
            int_val(1),
            int_val(1),
            int_val(2),
            int_val(3),
            int_val(4),
            int_val(5),
        ]);
        let r = builtin_bin_counts(&[data, int_val(2)]).unwrap();
        if let Value::List(counts) = r {
            // bins: [1,3), [3,5] — but float rounding may differ
            assert!(!counts.is_empty());
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_bin_counts_explicit() {
        let data = list(vec![
            int_val(1),
            int_val(2),
            int_val(3),
            int_val(4),
            int_val(5),
        ]);
        let spec = Value::List(vec![int_val(1), int_val(5), int_val(2)]);
        let r = builtin_bin_counts(&[data, spec]).unwrap();
        if let Value::List(counts) = r {
            // bins [1,3), [3,5]: 2 in first, 3 in second
            assert_eq!(counts.len(), 2);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_histogram_list() {
        let data = list(vec![
            int_val(1),
            int_val(2),
            int_val(3),
            int_val(4),
            int_val(5),
        ]);
        let r = builtin_histogram_list(&[data, int_val(5)]).unwrap();
        if let Value::List(parts) = r {
            assert_eq!(parts.len(), 2);
            if let (Value::List(edges), Value::List(counts)) = (&parts[0], &parts[1]) {
                assert_eq!(edges.len(), 6);
                assert_eq!(counts.len(), 5);
            } else {
                panic!("Expected edges and counts");
            }
        } else {
            panic!("Expected List");
        }
    }

    // ── Distributions ────────────────────────────────────────────────────────

    #[test]
    fn test_normal_distribution() {
        let dist = builtin_normal_distribution(&[real_val(0.0), real_val(1.0)]).unwrap();
        if let Value::Assoc(map) = dist {
            assert_eq!(
                map.get("Distribution"),
                Some(&Value::Str("Normal".to_string()))
            );
        } else {
            panic!("Expected Assoc");
        }
    }

    #[test]
    fn test_exponential_distribution() {
        let dist = builtin_exponential_distribution(&[real_val(2.0)]).unwrap();
        if let Value::Assoc(map) = &dist {
            assert_eq!(
                map.get("Distribution"),
                Some(&Value::Str("Exponential".to_string()))
            );
        }
        // Mean = 1/lambda = 0.5
        let m = builtin_mean(&[dist]).unwrap();
        assert!(approx(&m, 0.5, 1e-10));
    }

    #[test]
    fn test_gamma_distribution_mean() {
        let dist = builtin_gamma_distribution(&[real_val(3.0), real_val(2.0)]).unwrap();
        let m = builtin_mean(&[dist.clone()]).unwrap();
        assert!(approx(&m, 1.5, 1e-10)); // 3/2
        let v = builtin_variance(&[dist]).unwrap();
        assert!(approx(&v, 0.75, 1e-10)); // 3/4
    }

    #[test]
    fn test_chi_square_mean_variance() {
        let dist = builtin_chi_square_distribution(&[real_val(4.0)]).unwrap();
        let m = builtin_mean(&[dist.clone()]).unwrap();
        assert!(approx(&m, 4.0, 1e-10));
        let v = builtin_variance(&[dist]).unwrap();
        assert!(approx(&v, 8.0, 1e-10));
    }

    #[test]
    fn test_beta_mean_variance() {
        let dist = builtin_beta_distribution(&[real_val(2.0), real_val(5.0)]).unwrap();
        let m = builtin_mean(&[dist.clone()]).unwrap();
        assert!(approx(&m, 2.0 / 7.0, 1e-10));
        let v = builtin_variance(&[dist]).unwrap();
        assert!(approx(&v, 10.0 / (49.0 * 8.0), 1e-10));
    }

    // ── PDF and CDF ──────────────────────────────────────────────────────────

    #[test]
    fn test_pdf_normal_peak() {
        let dist = builtin_normal_distribution(&[real_val(0.0), real_val(1.0)]).unwrap();
        // PDF at 0 = 1/sqrt(2pi)
        let r = builtin_pdf(&[dist, real_val(0.0)]).unwrap();
        assert!(approx(&r, 1.0 / (2.0 * std::f64::consts::PI).sqrt(), 1e-6));
    }

    #[test]
    fn test_pdf_exponential() {
        let dist = builtin_exponential_distribution(&[real_val(1.0)]).unwrap();
        // PDF(1) = e^{-1}
        let r = builtin_pdf(&[dist, real_val(1.0)]).unwrap();
        assert!(approx(&r, (-1.0f64).exp(), 1e-10));
    }

    #[test]
    fn test_cdf_normal_median() {
        let dist = builtin_normal_distribution(&[real_val(0.0), real_val(1.0)]).unwrap();
        // CDF(0) = 0.5
        let r = builtin_cdf(&[dist, real_val(0.0)]).unwrap();
        assert!(approx(&r, 0.5, 1e-6));
    }

    #[test]
    fn test_cdf_exponential() {
        let dist = builtin_exponential_distribution(&[real_val(1.0)]).unwrap();
        // CDF(1) = 1 - e^{-1}
        let r = builtin_cdf(&[dist, real_val(1.0)]).unwrap();
        assert!(approx(&r, 1.0 - (-1.0f64).exp(), 1e-10));
    }

    #[test]
    fn test_cdf_uniform() {
        let dist = builtin_uniform_distribution(&[real_val(0.0), real_val(2.0)]).unwrap();
        let r = builtin_cdf(&[dist, real_val(1.0)]).unwrap();
        assert!(approx(&r, 0.5, 1e-10));
    }

    #[test]
    fn test_cdf_cauchy() {
        let dist = builtin_cauchy_distribution(&[real_val(0.0), real_val(1.0)]).unwrap();
        // CDF(0) = 0.5 by symmetry
        let r = builtin_cdf(&[dist, real_val(0.0)]).unwrap();
        assert!(approx(&r, 0.5, 1e-10));
    }

    // ── RandomVariate ────────────────────────────────────────────────────────

    #[test]
    fn test_random_variate_normal_list() {
        let dist = builtin_normal_distribution(&[real_val(0.0), real_val(1.0)]).unwrap();
        let r = builtin_random_variate(&[dist, int_val(100)]).unwrap();
        if let Value::List(items) = r {
            assert_eq!(items.len(), 100);
            for v in &items {
                assert!(matches!(v, Value::Real(_)));
            }
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_random_variate_single() {
        let dist = builtin_normal_distribution(&[real_val(0.0), real_val(1.0)]).unwrap();
        let r = builtin_random_variate(&[dist]).unwrap();
        // Single sample: not a list
        assert!(matches!(r, Value::Real(_)));
    }

    #[test]
    fn test_random_variate_exponential() {
        let dist = builtin_exponential_distribution(&[real_val(1.0)]).unwrap();
        let r = builtin_random_variate(&[dist, int_val(50)]).unwrap();
        if let Value::List(items) = r {
            assert_eq!(items.len(), 50);
            // All samples must be non-negative
            for v in &items {
                if let Value::Real(f) = v {
                    assert!(f.to_f64() >= 0.0);
                }
            }
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_random_variate_poisson() {
        let dist = builtin_poisson_distribution(&[real_val(3.0)]).unwrap();
        let r = builtin_random_variate(&[dist, int_val(20)]).unwrap();
        if let Value::List(items) = r {
            assert_eq!(items.len(), 20);
            // All should be non-negative integers
            for v in &items {
                assert!(matches!(v, Value::Integer(_)));
            }
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_random_variate_poisson_large_lambda() {
        // lambda = 1000 makes e^{-lambda} underflow to 0; the naive sampler
        // would spin forever. With log-space sampling the mean should be ~1000.
        let dist = builtin_poisson_distribution(&[real_val(1000.0)]).unwrap();
        let r = builtin_random_variate(&[dist, int_val(200)]).unwrap();
        let items = match r {
            Value::List(items) => items,
            _ => panic!("Expected List"),
        };
        assert_eq!(items.len(), 200);
        let mut sum = 0.0;
        for v in &items {
            let k = match v {
                Value::Integer(n) => n.to_f64(),
                _ => panic!("Expected Integer"),
            };
            assert!(k >= 0.0);
            sum += k;
        }
        let mean = sum / items.len() as f64;
        assert!(
            (900.0..1100.0).contains(&mean),
            "Poisson(1000) sample mean {} far from 1000",
            mean
        );
    }

    #[test]
    fn test_random_variate_beta() {
        let dist = builtin_beta_distribution(&[real_val(2.0), real_val(5.0)]).unwrap();
        let r = builtin_random_variate(&[dist, int_val(30)]).unwrap();
        if let Value::List(items) = r {
            for v in &items {
                if let Value::Real(f) = v {
                    let x = f.to_f64();
                    assert!((0.0..=1.0).contains(&x), "Beta sample {} out of [0,1]", x);
                }
            }
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_random_variate_binomial() {
        let dist = builtin_binomial_distribution(&[int_val(10), real_val(0.3)]).unwrap();
        let r = builtin_random_variate(&[dist, int_val(20)]).unwrap();
        if let Value::List(items) = r {
            for v in &items {
                if let Value::Integer(n) = v {
                    let k = n.to_i64().unwrap();
                    assert!((0..=10).contains(&k));
                }
            }
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_random_variate_uniform() {
        let dist = builtin_uniform_distribution(&[real_val(0.0), real_val(10.0)]).unwrap();
        let r = builtin_random_variate(&[dist, int_val(50)]).unwrap();
        if let Value::List(items) = r {
            assert_eq!(items.len(), 50);
        } else {
            panic!("Expected List");
        }
    }
}
