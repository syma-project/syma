//! Pseudorandom number builtins.
//!
//! All randomness funnels through a single thread-local RNG (`RNG`) so that
//! `SeedRandom[n]` makes every downstream draw — `RandomReal`, `RandomInteger`,
//! `RandomChoice`, `RandomComplex`, `RandomVariate`, `RandomSample`,
//! `RandomPermutation`, `RandomPrime`, `RandomGraph`, … — reproducible.
//!
//! The generators take care to be **shape-correct** (dimension specs produce
//! properly nested arrays) and **big-integer safe** (`RandomInteger` /
//! `RandomPrime` sample uniformly across arbitrary-precision ranges instead of
//! truncating to `u64`).

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    /// Thread-local RNG shared by all random builtins (RandomReal, RandomChoice,
    /// RandomInteger, RandomVariate, …). Seeded deterministically so that
    /// `SeedRandom[n]` makes subsequent draws reproducible.
    static RNG: RefCell<fastrand::Rng> =
        const { RefCell::new(fastrand::Rng::with_seed(0x9E37_79B9_7F4A_7C15)) };
}

/// Reseed the shared RNG with an explicit seed (`SeedRandom[n]`).
pub fn seed_rng(seed: u64) {
    RNG.with(|cell| *cell.borrow_mut() = fastrand::Rng::with_seed(seed));
}

/// Reseed the shared RNG from system entropy (`SeedRandom[]`).
pub fn reseed_entropy() {
    RNG.with(|cell| *cell.borrow_mut() = fastrand::Rng::new());
}

/// Run `f` with mutable access to the shared RNG.
pub fn with_rng<T>(f: impl FnOnce(&mut fastrand::Rng) -> T) -> T {
    RNG.with(|cell| f(&mut cell.borrow_mut()))
}

/// Draw the next raw 64-bit value from the shared RNG.
pub fn next_u64() -> u64 {
    with_rng(|rng| rng.u64(..))
}

/// Clone the current RNG state (used by `BlockRandom`).
fn save_rng() -> fastrand::Rng {
    RNG.with(|cell| cell.borrow().clone())
}

/// Restore a previously saved RNG state.
fn restore_rng(saved: fastrand::Rng) {
    RNG.with(|cell| *cell.borrow_mut() = saved);
}

/// A uniform `f64` in `[0, 1]`.
fn uniform01() -> f64 {
    (next_u64() as f64) / (u64::MAX as f64)
}

/// Draw a uniform `f64` in `[min, max]`.
fn uniform_real(min: f64, max: f64) -> f64 {
    min + uniform01() * (max - min)
}

/// Uniform `Integer` in `[0, n)` for an arbitrary-precision `n`.
///
/// Uses rejection sampling on a whole number of 64-bit words, so the result is
/// unbiased even when `n` exceeds `u64::MAX`.
fn random_below(n: &Integer) -> Integer {
    if *n <= 1 {
        return Integer::from(0);
    }
    let bits = n.significant_bits();
    loop {
        let mut candidate = Integer::new();
        let mut have = 0u32;
        while have < bits {
            candidate <<= 64;
            candidate |= Integer::from(next_u64());
            have += 64;
        }
        candidate >>= have - bits;
        if candidate < *n {
            return candidate;
        }
    }
}

/// A random `Value::Integer` uniform in `[min, max]` (inclusive).
fn random_integer_value(min: &Integer, max: &Integer) -> Value {
    let range = max.clone() - min.clone() + Integer::from(1);
    Value::Integer(min.clone() + random_below(&range))
}

// ── Generic helpers ───────────────────────────────────────────────────────────

fn int_value(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

fn no_match(head: &str, args: &[Value]) -> EvalError {
    EvalError::NoMatch {
        head: head.to_string(),
        args: Box::new(args.to_vec()),
    }
}

fn type_error(expected: &str, got: &Value) -> EvalError {
    EvalError::TypeError {
        expected: expected.to_string(),
        got: got.type_name().to_string(),
    }
}

/// Parse a dimension spec: integer `n` → 1-D `[n]`, list of ints → n-D.
///
/// An integer `0` yields an empty list; a list with zeros yields (possibly
/// empty) nested levels.
fn parse_dims(val: &Value) -> Result<Vec<usize>, EvalError> {
    match val {
        Value::Integer(n) => {
            let n_i64 = n
                .to_i64()
                .ok_or_else(|| type_error("non-negative integer dimension", val))?;
            if n_i64 < 0 {
                return Err(EvalError::Error(
                    "dimension must be non-negative".to_string(),
                ));
            }
            Ok(vec![n_i64 as usize])
        }
        Value::List(items) => {
            let mut dims = Vec::with_capacity(items.len());
            for item in items {
                let n = item
                    .to_integer()
                    .ok_or_else(|| type_error("Integer", item))?;
                if n < 0 {
                    return Err(EvalError::Error(
                        "dimension must be non-negative".to_string(),
                    ));
                }
                dims.push(n as usize);
            }
            Ok(dims)
        }
        _ => Err(type_error("Integer or List of integers", val)),
    }
}

/// Recursively build a nested list with the given dimensions, calling `f` for
/// every leaf. An empty `dims` means `f` is the whole result (a single value).
fn build_array<F: FnMut() -> Value>(dims: &[usize], f: &mut F) -> Value {
    match dims.split_first() {
        None => f(),
        Some((&count, rest)) => Value::List((0..count).map(|_| build_array(rest, f)).collect()),
    }
}

/// Extract an f64 from a `Value`, numericising simple constant expressions
/// (`Pi`, `E`, `Degree`, `Plus`/`Times`/`Power`/`Sqrt`) so bounds such as
/// `Pi/2` work without an explicit `N[...]`.
fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        Value::Rational(r) => Some(r.to_f64()),
        Value::Complex { re, im: 0.0 } => Some(*re),
        Value::Symbol(s) if s == "Pi" => Some(std::f64::consts::PI),
        Value::Symbol(s) if s == "E" => Some(std::f64::consts::E),
        Value::Symbol(s) if s == "Degree" => Some(std::f64::consts::PI / 180.0),
        Value::Call { head, args } => match head.as_str() {
            "Plus" => args.iter().try_fold(0.0, |acc, a| Some(acc + as_f64(a)?)),
            "Times" => args.iter().try_fold(1.0, |acc, a| Some(acc * as_f64(a)?)),
            "Power" if args.len() == 2 => Some(as_f64(&args[0])?.powf(as_f64(&args[1])?)),
            "Divide" if args.len() == 2 => Some(as_f64(&args[0])? / as_f64(&args[1])?),
            "Subtract" if args.len() == 2 => Some(as_f64(&args[0])? - as_f64(&args[1])?),
            "Minus" if args.len() == 1 => Some(-as_f64(&args[0])?),
            "Sqrt" if args.len() == 1 => Some(as_f64(&args[0])?.sqrt()),
            "Rational" if args.len() == 2 => {
                let den = as_f64(&args[1])?;
                Some(as_f64(&args[0])? / den)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Extract an arbitrary-precision integer from a `Value`.
fn as_bigint(v: &Value) -> Option<Integer> {
    match v {
        Value::Integer(n) => Some(n.clone()),
        Value::Real(r) => r.to_integer_round(rug::float::Round::Zero).map(|(i, _)| i),
        Value::Rational(r) if *r.denom() == 1 => Some(r.numer().clone()),
        _ => as_f64(v).map(|f| Integer::from(f as i64)),
    }
}

// ── RandomReal ────────────────────────────────────────────────────────────────

/// Generate a single random real in `[min, max]`.
fn value_random_real(min: f64, max: f64) -> Value {
    Value::Real(Float::with_val(DEFAULT_PRECISION, uniform_real(min, max)))
}

/// Parse a real range spec: `{min, max}` or a bare `max` (→ `[0, max]`).
fn parse_real_range(v: &Value) -> Result<(f64, f64), EvalError> {
    match v {
        Value::List(items) if items.len() == 2 => {
            let min = as_f64(&items[0]).ok_or_else(|| type_error("Number", &items[0]))?;
            let max = as_f64(&items[1]).ok_or_else(|| type_error("Number", &items[1]))?;
            Ok((min, max))
        }
        Value::List(_) => Err(EvalError::Error(
            "range must have the form {min, max}".to_string(),
        )),
        other => {
            let max = as_f64(other).ok_or_else(|| type_error("{min, max} or max", other))?;
            Ok((0.0, max))
        }
    }
}

pub fn builtin_random_real(args: &[Value]) -> Result<Value, EvalError> {
    match args {
        [] => Ok(value_random_real(0.0, 1.0)),
        [spec] => {
            let (min, max) = parse_real_range(spec)?;
            Ok(value_random_real(min, max))
        }
        [spec, dims] => {
            let (min, max) = parse_real_range(spec)?;
            let dims = parse_dims(dims)?;
            Ok(build_array(&dims, &mut || value_random_real(min, max)))
        }
        _ => Err(no_match("RandomReal", args)),
    }
}

// ── RandomInteger ─────────────────────────────────────────────────────────────

/// Parse an integer range spec: `{min, max}` or bare `max` (→ `[0, max]`).
fn parse_integer_range(v: &Value) -> Result<(Integer, Integer), EvalError> {
    match v {
        Value::Integer(n) => {
            if *n < 0 {
                return Err(EvalError::Error(
                    "RandomInteger: max must be non-negative".to_string(),
                ));
            }
            Ok((Integer::from(0), n.clone()))
        }
        Value::List(items) if items.len() == 2 => {
            let min = as_bigint(&items[0]).ok_or_else(|| type_error("Integer", &items[0]))?;
            let max = as_bigint(&items[1]).ok_or_else(|| type_error("Integer", &items[1]))?;
            if min > max {
                return Err(EvalError::Error(
                    "RandomInteger: min must be <= max".to_string(),
                ));
            }
            Ok((min, max))
        }
        _ => Err(type_error("Integer or {min, max}", v)),
    }
}

pub fn builtin_random_integer(args: &[Value]) -> Result<Value, EvalError> {
    match args {
        [] => Ok(int_value((next_u64() & 1) as i64)),
        [spec] => {
            let (min, max) = parse_integer_range(spec)?;
            Ok(random_integer_value(&min, &max))
        }
        [spec, dims] => {
            let (min, max) = parse_integer_range(spec)?;
            let dims = parse_dims(dims)?;
            Ok(build_array(&dims, &mut || random_integer_value(&min, &max)))
        }
        _ => Err(no_match("RandomInteger", args)),
    }
}

// ── RandomChoice ──────────────────────────────────────────────────────────────

/// Structural `Expr` → `Value` conversion for held rule operands.
fn expr_to_value(expr: &Expr) -> Value {
    match expr {
        Expr::Integer(n) => Value::Integer(n.clone()),
        Expr::Real(r) => Value::Real(r.clone()),
        Expr::Complex { re, im } => Value::Complex { re: *re, im: *im },
        Expr::Str(s) => Value::Str(s.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Null => Value::Null,
        Expr::Symbol(s) => Value::Symbol(s.clone()),
        Expr::List(items) => Value::List(items.iter().map(expr_to_value).collect()),
        other => Value::Pattern(other.clone()),
    }
}

/// Materialise a held rule operand (`Value::Pattern`) into a plain `Value`.
fn materialize(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => expr_to_value(e),
        other => other.clone(),
    }
}

/// Split a `RandomChoice` source into `(values, weights?)`.
///
/// Accepted forms: a plain list of values, or a `weights -> values` rule. Rule
/// operands arrive as `Value::Pattern` (held by the evaluator) and are
/// materialised here.
fn choice_source(v: &Value) -> Result<(Vec<Value>, Option<Vec<f64>>), EvalError> {
    match v {
        Value::List(values) => {
            if values.is_empty() {
                return Err(EvalError::Error(
                    "RandomChoice: empty list of choices".to_string(),
                ));
            }
            Ok((values.clone(), None))
        }
        Value::Rule { lhs, rhs, .. } => {
            let values = materialize(rhs);
            let Value::List(values) = values else {
                return Err(type_error("List of choices", &values));
            };
            if values.is_empty() {
                return Err(EvalError::Error(
                    "RandomChoice: empty list of choices".to_string(),
                ));
            }
            let weights = materialize(lhs);
            let Value::List(weights_raw) = weights else {
                return Err(type_error("List of weights", &weights));
            };
            if weights_raw.len() != values.len() {
                return Err(EvalError::Error(
                    "RandomChoice: weights and choices must have the same length".to_string(),
                ));
            }
            let mut weights = Vec::with_capacity(weights_raw.len());
            for w in &weights_raw {
                let wf = as_f64(w).ok_or_else(|| type_error("Number", w))?;
                if wf < 0.0 {
                    return Err(EvalError::Error(
                        "RandomChoice: weights must be non-negative".to_string(),
                    ));
                }
                weights.push(wf);
            }
            Ok((values, Some(weights)))
        }
        _ => Err(type_error("List or weights -> List", v)),
    }
}

/// Pick one element, optionally according to `weights`.
fn pick_choice(values: &[Value], weights: Option<&[f64]>) -> Value {
    match weights {
        None => values[(next_u64() as usize) % values.len()].clone(),
        Some(w) => {
            let total: f64 = w.iter().copied().sum();
            if total <= 0.0 || total.is_nan() {
                return values[(next_u64() as usize) % values.len()].clone();
            }
            let r = uniform01() * total;
            let mut acc = 0.0;
            for (i, weight) in w.iter().enumerate() {
                acc += *weight;
                if r < acc {
                    return values[i].clone();
                }
            }
            values[values.len() - 1].clone()
        }
    }
}

pub fn builtin_random_choice(args: &[Value]) -> Result<Value, EvalError> {
    match args {
        [source] => {
            let (values, weights) = choice_source(source)?;
            Ok(pick_choice(&values, weights.as_deref()))
        }
        [source, dims] => {
            let (values, weights) = choice_source(source)?;
            match dims {
                Value::Integer(_) | Value::List(_) => {
                    let dims = parse_dims(dims)?;
                    Ok(build_array(&dims, &mut || {
                        pick_choice(&values, weights.as_deref())
                    }))
                }
                _ => Err(type_error("Integer or List of integers", dims)),
            }
        }
        _ => Err(no_match("RandomChoice", args)),
    }
}

// ── RandomComplex ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum ComplexMethod {
    Rect,
    Polar,
}

/// Recognise a method argument (`"Polar"`, `"Rectangular"`, or `Method -> name`).
fn parse_complex_method(v: &Value) -> Option<ComplexMethod> {
    let name = match v {
        Value::Str(s) => s.as_str(),
        Value::Rule { lhs, rhs, .. } => match (&**lhs, &**rhs) {
            (Value::Str(k), Value::Str(s)) if k == "Method" => s.as_str(),
            _ => return None,
        },
        _ => return None,
    };
    match name {
        "Polar" => Some(ComplexMethod::Polar),
        "Rectangular" | "Rect" => Some(ComplexMethod::Rect),
        _ => None,
    }
}

/// Parse a complex range spec into `(remin, remax, imin, imax)`.
fn parse_complex_spec(v: &Value) -> Option<(f64, f64, f64, f64)> {
    match v {
        Value::List(items) if items.len() == 2 => match (&items[0], &items[1]) {
            // {{remin, remax}, {imin, imax}}
            (Value::List(re), Value::List(im)) if re.len() == 2 && im.len() == 2 => Some((
                as_f64(&re[0])?,
                as_f64(&re[1])?,
                as_f64(&im[0])?,
                as_f64(&im[1])?,
            )),
            // {zmin, zmax} with complex corners.
            (Value::Complex { re: r0, im: i0 }, Value::Complex { re: r1, im: i1 }) => {
                Some((*r0, *r1, *i0, *i1))
            }
            // {min, max} → square [min, max] × [min, max].
            _ => {
                let min = as_f64(&items[0])?;
                let max = as_f64(&items[1])?;
                Some((min, max, min, max))
            }
        },
        // A single complex corner → rectangle from the origin.
        Value::Complex { re, im } => Some((0.0, *re, 0.0, *im)),
        // A single real max → square [0, max] × [0, max].
        _ => {
            let max = as_f64(v)?;
            Some((0.0, max, 0.0, max))
        }
    }
}

fn random_complex_value(rem: f64, rex: f64, imm: f64, imx: f64, method: ComplexMethod) -> Value {
    match method {
        ComplexMethod::Rect => Value::Complex {
            re: uniform_real(rem, rex),
            im: uniform_real(imm, imx),
        },
        ComplexMethod::Polar => {
            let r = uniform_real(rem, rex);
            let theta = uniform_real(imm, imx);
            Value::Complex {
                re: r * theta.cos(),
                im: r * theta.sin(),
            }
        }
    }
}

/// `RandomComplex[spec?, dims?, method?]` — a pseudorandom complex number.
pub fn builtin_random_complex(args: &[Value]) -> Result<Value, EvalError> {
    let mut spec: Option<&Value> = None;
    let mut dims: Option<Vec<usize>> = None;
    let mut method = ComplexMethod::Rect;
    for arg in args {
        if let Some(m) = parse_complex_method(arg) {
            method = m;
            continue;
        }
        if spec.is_none() {
            spec = Some(arg);
            continue;
        }
        if dims.is_none() && matches!(arg, Value::Integer(_) | Value::List(_)) {
            dims = Some(parse_dims(arg)?);
            continue;
        }
        return Err(no_match("RandomComplex", args));
    }

    let (rem, rex, imm, imx) = match spec {
        Some(s) => parse_complex_spec(s).ok_or_else(|| type_error("range spec", s))?,
        None => (0.0, 1.0, 0.0, 1.0),
    };

    match dims {
        Some(d) => Ok(build_array(&d, &mut || {
            random_complex_value(rem, rex, imm, imx, method)
        })),
        None => Ok(random_complex_value(rem, rex, imm, imx, method)),
    }
}

// ── RandomVariate ─────────────────────────────────────────────────────────────

/// Sample a standard normal variate N(0,1) via Box-Muller.
fn sample_std_normal() -> f64 {
    let u1 = uniform01().max(1e-15);
    let u2 = uniform01();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Sample Gamma(alpha, 1) using Marsaglia-Tsang's method.
fn sample_gamma_unit(alpha: f64) -> f64 {
    if alpha < 1.0 {
        return sample_gamma_unit(alpha + 1.0) * uniform01().powf(1.0 / alpha);
    }
    let d = alpha - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        let z = sample_std_normal();
        let v = 1.0 + c * z;
        if v <= 0.0 {
            continue;
        }
        let v3 = v * v * v;
        let u = uniform01();
        if u < 1.0 - 0.0331 * z * z * z * z {
            return d * v3;
        }
        if u.ln() < 0.5 * z * z + d * (1.0 - v3 + v3.ln()) {
            return d * v3;
        }
    }
}

fn dist_param(dist: &HashMap<String, Value>, key: &str, default: f64) -> f64 {
    dist.get(key).and_then(as_f64).unwrap_or(default)
}

/// Generate one variate from a distribution association.
fn random_variate_one(dist: &Value) -> Result<Value, EvalError> {
    let Value::Assoc(map) = dist else {
        return Err(type_error("Association (distribution)", dist));
    };
    let dist_type = match map.get("Distribution") {
        Some(Value::Str(s)) => s.as_str(),
        _ => {
            return Err(EvalError::Error(
                "RandomVariate: invalid distribution (missing 'Distribution' key)".to_string(),
            ));
        }
    };
    let value = match dist_type {
        "Normal" => {
            let mu = dist_param(map, "Mean", 0.0);
            let sd = dist_param(map, "SD", 1.0);
            super::real(mu + sd * sample_std_normal())
        }
        "Uniform" => {
            let lo = dist_param(map, "Min", 0.0);
            let hi = dist_param(map, "Max", 1.0);
            super::real(uniform_real(lo, hi))
        }
        "Poisson" => {
            // Knuth's method in log space so lambda >~ 700 doesn't underflow.
            let lambda = dist_param(map, "Lambda", 1.0);
            let log_l = -lambda;
            let mut log_p = 0.0f64;
            let mut k = 0u64;
            loop {
                log_p += uniform01().max(1e-300).ln();
                if log_p < log_l {
                    break;
                }
                k += 1;
            }
            int_value(k as i64)
        }
        "Binomial" => {
            let trials = dist_param(map, "N", 1.0).max(0.0) as u64;
            let p = dist_param(map, "P", 0.5);
            let count = (0..trials).filter(|_| uniform01() < p).count();
            int_value(count as i64)
        }
        "Bernoulli" => {
            let p = dist_param(map, "P", 0.5);
            int_value(if uniform01() < p { 1 } else { 0 })
        }
        "Exponential" => {
            let lambda = dist_param(map, "Lambda", 1.0);
            super::real(-uniform01().max(1e-15).ln() / lambda)
        }
        "Gamma" => {
            let alpha = dist_param(map, "Alpha", 1.0);
            let lambda = dist_param(map, "Lambda", 1.0);
            super::real(sample_gamma_unit(alpha) / lambda)
        }
        "ChiSquare" => {
            let k = dist_param(map, "K", 1.0);
            super::real(2.0 * sample_gamma_unit(k / 2.0))
        }
        "StudentT" => {
            let nu = dist_param(map, "Nu", 1.0);
            let z = sample_std_normal();
            let v = 2.0 * sample_gamma_unit(nu / 2.0);
            super::real(z / (v / nu).sqrt())
        }
        "Beta" => {
            let a = dist_param(map, "Alpha", 1.0);
            let b = dist_param(map, "Beta", 1.0);
            let x = sample_gamma_unit(a);
            let y = sample_gamma_unit(b);
            super::real(x / (x + y))
        }
        "LogNormal" => {
            let mu = dist_param(map, "Mu", 0.0);
            let sigma = dist_param(map, "Sigma", 1.0);
            super::real((mu + sigma * sample_std_normal()).exp())
        }
        "Cauchy" => {
            let x0 = dist_param(map, "Location", 0.0);
            let gamma = dist_param(map, "Scale", 1.0);
            super::real(x0 + gamma * (std::f64::consts::PI * (uniform01() - 0.5)).tan())
        }
        "DiscreteUniform" => {
            let lo = dist_param(map, "Min", 0.0).floor();
            let hi = dist_param(map, "Max", 1.0).floor();
            int_value((lo + (uniform01() * (hi - lo + 1.0)).floor()) as i64)
        }
        // ── Broadened families ──────────────────────────────────────────────
        "FRatio" => {
            let num_df = dist_param(map, "N", 1.0);
            let den_df = dist_param(map, "D", 1.0);
            let g1 = sample_gamma_unit(num_df / 2.0);
            let g2 = sample_gamma_unit(den_df / 2.0);
            super::real(g1 * den_df / (g2 * num_df))
        }
        "Laplace" | "Logistic" | "Weibull" | "Pareto" => super::real(
            crate::builtins::statistics::distribution_quantile_f64(map, uniform01()).unwrap_or(0.0),
        ),
        "Geometric" | "NegativeBinomial" | "Pascal" | "Hypergeometric" => int_value(
            crate::builtins::statistics::distribution_quantile_f64(map, uniform01()).unwrap_or(0.0)
                as i64,
        ),
        "Multinomial" => {
            let trials = dist_param(map, "N", 0.0) as usize;
            let ps: Vec<f64> = match map.get("P") {
                Some(Value::List(l)) => l.iter().filter_map(as_f64).collect(),
                _ => vec![],
            };
            let mut counts = vec![0i64; ps.len()];
            for _ in 0..trials {
                let u = uniform01();
                let mut acc = 0.0;
                let mut chosen = ps.len().saturating_sub(1);
                for (i, p) in ps.iter().enumerate() {
                    acc += p;
                    if u < acc {
                        chosen = i;
                        break;
                    }
                }
                counts[chosen] += 1;
            }
            Value::List(counts.into_iter().map(int_value).collect())
        }
        other => {
            return Err(EvalError::Error(format!(
                "RandomVariate: unknown distribution type '{}'",
                other
            )));
        }
    };
    Ok(value)
}

/// Recursively expand `dist` into a nested array of variates.
fn random_variate_array(dist: &Value, dims: &[usize]) -> Value {
    build_array(dims, &mut || {
        random_variate_one(dist).unwrap_or(Value::Null)
    })
}

/// `RandomVariate[dist]`, `RandomVariate[dist, n]`, `RandomVariate[dist, dims]`.
pub fn builtin_random_variate(args: &[Value]) -> Result<Value, EvalError> {
    match args {
        [dist] => random_variate_one(dist),
        [dist, dims] => {
            // Validate the distribution before expanding so errors surface even
            // for empty dimension lists.
            random_variate_one(dist)?;
            let dims = match dims {
                Value::List(d) if d.is_empty() => vec![],
                other => parse_dims(other)?,
            };
            Ok(random_variate_array(dist, &dims))
        }
        _ => Err(no_match("RandomVariate", args)),
    }
}

/// `RandomArray[dist, dims?]` — array of pseudorandom variates.
pub fn builtin_random_array(args: &[Value]) -> Result<Value, EvalError> {
    match args {
        [dist] => random_variate_one(dist),
        [dist, dims] => {
            random_variate_one(dist)?;
            let dims = match dims {
                Value::List(d) if d.is_empty() => vec![],
                other => parse_dims(other)?,
            };
            Ok(random_variate_array(dist, &dims))
        }
        _ => Err(EvalError::Error(
            "RandomArray requires 1 or 2 arguments".to_string(),
        )),
    }
}

// ── RandomSample / RandomPermutation ──────────────────────────────────────────

/// Distinct indices `[0, len)` of length `take` via partial Fisher-Yates.
fn sample_indices(len: usize, take: usize) -> Vec<usize> {
    let take = take.min(len);
    let mut idx: Vec<usize> = (0..len).collect();
    for i in 0..take {
        let j = i + (next_u64() as usize) % (len - i);
        idx.swap(i, j);
    }
    idx.truncate(take);
    idx
}

/// `RandomSample[list]`, `RandomSample[list, n]`, `RandomSample[list, n, repl]`.
pub fn builtin_random_sample(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 3 {
        return Err(no_match("RandomSample", args));
    }
    let Value::List(items) = &args[0] else {
        return Err(type_error("List", &args[0]));
    };

    let replacement = match args.get(2) {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(other) => return Err(type_error("Boolean", other)),
    };

    let take = match args.get(1) {
        None => items.len(),
        Some(Value::Integer(_)) => {
            let n = args[1]
                .to_integer()
                .ok_or_else(|| type_error("non-negative integer", &args[1]))?;
            if n < 0 {
                return Err(EvalError::Error(
                    "RandomSample: count must be non-negative".to_string(),
                ));
            }
            n as usize
        }
        Some(Value::List(_)) => {
            // Dimension form: each leaf is one element sampled with replacement.
            if items.is_empty() {
                return Err(EvalError::Error(
                    "RandomSample: cannot sample from an empty list".to_string(),
                ));
            }
            let dims = parse_dims(&args[1])?;
            return Ok(build_array(&dims, &mut || {
                items[(next_u64() as usize) % items.len()].clone()
            }));
        }
        Some(other) => return Err(type_error("Integer, List of integers, or Boolean", other)),
    };

    if !replacement && take > items.len() {
        return Err(EvalError::Error(format!(
            "RandomSample: cannot take {} samples without replacement from {} elements",
            take,
            items.len()
        )));
    }

    if replacement {
        if items.is_empty() && take > 0 {
            return Err(EvalError::Error(
                "RandomSample: cannot sample from an empty list".to_string(),
            ));
        }
        Ok(Value::List(
            (0..take)
                .map(|_| items[(next_u64() as usize) % items.len()].clone())
                .collect(),
        ))
    } else {
        Ok(Value::List(
            sample_indices(items.len(), take)
                .into_iter()
                .map(|i| items[i].clone())
                .collect(),
        ))
    }
}

/// `RandomPermutation[n]` (of `{1, …, n}`) or `RandomPermutation[list]`.
pub fn builtin_random_permutation(args: &[Value]) -> Result<Value, EvalError> {
    match args {
        [Value::Integer(n)] => {
            let n = n
                .to_usize()
                .ok_or_else(|| type_error("non-negative integer", &args[0]))?;
            let perm: Vec<Value> = {
                let idx = sample_indices(n, n);
                idx.into_iter().map(|i| int_value(i as i64 + 1)).collect()
            };
            Ok(Value::List(perm))
        }
        [Value::List(items)] => {
            let idx = sample_indices(items.len(), items.len());
            Ok(Value::List(
                idx.into_iter().map(|i| items[i].clone()).collect(),
            ))
        }
        _ => Err(type_error("Integer or List", &args[0])),
    }
}

// ── RandomPrime ───────────────────────────────────────────────────────────────

/// `RandomPrime[max]`, `RandomPrime[{min, max}]`, optionally repeated `n` times.
///
/// Uniform across the whole (possibly arbitrary-precision) range: candidates are
/// drawn with `random_below` rather than truncated to `u64`.
pub fn builtin_random_prime(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(no_match("RandomPrime", args));
    }
    let (lo, hi) = match &args[0] {
        Value::Integer(n) if *n >= 2 => (Integer::from(2), n.clone()),
        Value::List(items) if items.len() == 2 => {
            let lo = as_bigint(&items[0]).ok_or_else(|| type_error("Integer", &items[0]))?;
            let hi = as_bigint(&items[1]).ok_or_else(|| type_error("Integer", &items[1]))?;
            (lo.max(Integer::from(2)), hi)
        }
        other => return Err(type_error("Integer or {min, max}", other)),
    };
    if lo > hi {
        return Err(EvalError::Error(
            "RandomPrime: min must be <= max".to_string(),
        ));
    }
    let count = match args.get(1) {
        None => 1usize,
        Some(Value::Integer(k)) if k.is_positive() => k.to_usize().unwrap_or(1),
        Some(other) => return Err(type_error("positive integer", other)),
    };

    let range = hi.clone() - lo.clone() + Integer::from(1);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let candidate = lo.clone() + random_below(&range);
        out.push(Value::Integer(nearest_prime(&candidate, &lo, &hi)));
    }
    if count == 1 {
        Ok(out.pop().unwrap())
    } else {
        Ok(Value::List(out))
    }
}

/// The prime in `[lo, hi]` nearest above `candidate` (falling back below `hi`).
fn nearest_prime(candidate: &Integer, lo: &Integer, hi: &Integer) -> Integer {
    let mut c = candidate.clone();
    let mut tries = 0u32;
    loop {
        if c > *hi {
            // Walk back from `hi` until a prime is found.
            let mut b = hi.clone();
            let mut back = 0u32;
            while b >= *lo {
                if is_prime(&b) {
                    return b;
                }
                if back > 4096 {
                    return lo.clone();
                }
                b = b.prev_prime();
                back += 1;
            }
            return lo.clone();
        }
        if is_prime(&c) {
            return c;
        }
        if tries > 4096 {
            // Sparse fallback: step down from `hi`.
            let mut b = hi.clone();
            while b >= *lo && !is_prime(&b) {
                b = b.prev_prime();
            }
            return if b < *lo { lo.clone() } else { b };
        }
        c = c.next_prime();
        tries += 1;
    }
}

/// Probabilistic primality test for arbitrary-precision integers.
fn is_prime(n: &Integer) -> bool {
    *n >= 2 && n.is_probably_prime(30) != rug::integer::IsPrime::No
}

// ── SeedRandom / BlockRandom ──────────────────────────────────────────────────

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Derive a `u64` seed from a `Value` (integer, real, rational, or string).
fn seed_from(v: &Value) -> Result<u64, EvalError> {
    match v {
        Value::Integer(n) => Ok(n.to_u64().unwrap_or_else(|| fnv1a(&n.to_string()))),
        Value::Real(r) => Ok(r.to_f64().to_bits()),
        Value::Rational(r) => Ok(fnv1a(&r.to_string())),
        Value::Str(s) => Ok(fnv1a(s)),
        _ => Err(type_error("Integer, Real, Rational, or String", v)),
    }
}

/// Recognise a method spec (`Method -> "name"` or `"name"`).
fn seed_method(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.clone()),
        Value::Rule { lhs, rhs, .. } => match (&**lhs, &**rhs) {
            (Value::Str(k), Value::Str(s)) if k == "Method" => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// `SeedRandom[]`, `SeedRandom[seed]`, `SeedRandom[seed, method]`.
///
/// The method name is folded into the seed so distinct generators give distinct
/// (but reproducible) streams.
pub fn builtin_seed_random(args: &[Value]) -> Result<Value, EvalError> {
    match args {
        [] => {
            reseed_entropy();
            Ok(Value::Null)
        }
        [seed] => {
            seed_rng(seed_from(seed)?);
            Ok(Value::Null)
        }
        [seed, method] => {
            let base = seed_from(seed)?;
            let mixed = match seed_method(method) {
                Some(name) => base ^ fnv1a(&name).rotate_left(17) ^ 0x9E37_79B9_7F4A_7C15,
                None => base,
            };
            seed_rng(mixed);
            Ok(Value::Null)
        }
        _ => Err(no_match("SeedRandom", args)),
    }
}

/// `BlockRandom[expr]` / `BlockRandom[expr, seed]` — evaluate `expr` against a
/// local RNG state that is restored afterwards.
pub fn builtin_block_random(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(no_match("BlockRandom", args));
    }
    // The optional seed argument is held along with the body, so evaluate it
    // first (before localizing the state) to derive the seed.
    let seed = match args.get(1) {
        Some(v) => Some(seed_from(&crate::eval::eval(&held_expr(v), env)?)?),
        None => None,
    };
    let saved = save_rng();
    if let Some(seed) = seed {
        seed_rng(seed);
    }
    let result = crate::eval::eval(&held_expr(&args[0]), env);
    restore_rng(saved);
    result
}

/// Recover the held expression from a Pattern-wrapped argument.
fn held_expr(v: &Value) -> Expr {
    match v {
        Value::Pattern(e) => e.clone(),
        other => crate::eval::table::value_to_expr(other),
    }
}

// ── Registration ──────────────────────────────────────────────────────────────

/// Register the random builtins implemented in this module.
pub fn register(env: &Env) {
    use super::{register_builtin, register_builtin_env};
    register_builtin(env, "RandomReal", builtin_random_real);
    register_builtin(env, "RandomInteger", builtin_random_integer);
    register_builtin(env, "RandomChoice", builtin_random_choice);
    register_builtin(env, "RandomComplex", builtin_random_complex);
    register_builtin(env, "RandomVariate", builtin_random_variate);
    register_builtin(env, "RandomArray", builtin_random_array);
    register_builtin(env, "RandomSample", builtin_random_sample);
    register_builtin(env, "RandomPermutation", builtin_random_permutation);
    register_builtin(env, "RandomPrime", builtin_random_prime);
    register_builtin(env, "SeedRandom", builtin_seed_random);
    register_builtin_env(env, "BlockRandom", builtin_block_random);
    // `RandomGraph` is implemented in `graphtheory`; register it here too so the
    // random subsystem is the single seed-driven authority for graph generators.
    register_builtin(env, "RandomGraph", super::graphtheory::builtin_random_graph);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::ops::Pow;

    fn int(n: i64) -> Value {
        int_value(n)
    }

    fn real(f: f64) -> Value {
        super::super::real(f)
    }

    fn list(items: Vec<Value>) -> Value {
        Value::List(items)
    }

    fn test_env() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    fn eval_str(input: &str) -> Value {
        let env = test_env();
        let tokens = crate::lexer::tokenize(input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap_or_else(|e| panic!("parse {input}: {e:?}"));
        crate::eval::eval_program(&ast, &env).unwrap_or_else(|e| panic!("eval {input}: {e:?}"))
    }

    fn as_f64(v: &Value) -> f64 {
        v.to_real().expect("expected a real")
    }

    // ── RandomReal ────────────────────────────────────────────────────────────

    #[test]
    fn test_random_real_0_args() {
        assert!(matches!(builtin_random_real(&[]).unwrap(), Value::Real(_)));
    }

    #[test]
    fn test_random_real_1_arg_range() {
        let result = builtin_random_real(&[list(vec![real(0.0), real(1.0)])]).unwrap();
        assert!(matches!(result, Value::Real(_)));
    }

    #[test]
    fn test_random_real_2_arg_integer_dim() {
        let result = builtin_random_real(&[list(vec![real(0.0), real(1.0)]), int(5)]).unwrap();
        assert!(matches!(&result, Value::List(items) if items.len() == 5));
    }

    #[test]
    fn test_random_real_2_arg_list_dim() {
        let result =
            builtin_random_real(&[list(vec![real(0.0), real(1.0)]), list(vec![int(3), int(4)])])
                .unwrap();
        let Value::List(rows) = &result else {
            panic!("expected list");
        };
        assert_eq!(rows.len(), 3);
        assert!(
            rows.iter()
                .all(|row| matches!(row, Value::List(cols) if cols.len() == 4))
        );
    }

    #[test]
    fn test_random_real_max_and_dims() {
        let result = builtin_random_real(&[int(3), list(vec![int(2), int(2)])]).unwrap();
        let Value::List(rows) = result else {
            panic!("expected list");
        };
        assert_eq!(rows.len(), 2);
        for row in rows {
            assert!(matches!(row, Value::List(cols) if cols.len() == 2));
        }
    }

    #[test]
    fn test_random_real_zero_dim() {
        let result = builtin_random_real(&[list(vec![real(0.0), real(1.0)]), int(0)]).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_random_real_too_many_args_returns_no_match() {
        let result = builtin_random_real(&[list(vec![real(0.0), real(1.0)]), int(5), int(3)]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));
    }

    #[test]
    fn test_random_real_wrong_dims_type() {
        let result = builtin_random_real(&[list(vec![real(0.0), real(1.0)]), real(5.0)]);
        assert!(matches!(result, Err(EvalError::TypeError { .. })));
    }

    #[test]
    fn test_random_real_max_argument() {
        let v = builtin_random_real(&[int(3)]).unwrap();
        let f = as_f64(&v);
        assert!((0.0..=3.0).contains(&f));
    }

    #[test]
    fn test_random_real_seed_reproducible() {
        seed_rng(42);
        let a = builtin_random_real(&[]).unwrap();
        seed_rng(42);
        let b = builtin_random_real(&[]).unwrap();
        assert_eq!(a, b);
    }

    // ── RandomInteger ─────────────────────────────────────────────────────────

    #[test]
    fn test_random_integer_range_and_dims() {
        let v = builtin_random_integer(&[list(vec![int(-2), int(2)]), list(vec![int(2), int(3)])])
            .unwrap();
        let Value::List(rows) = &v else {
            panic!("expected list");
        };
        assert_eq!(rows.len(), 2);
        for row in rows {
            let Value::List(cols) = row else {
                panic!("expected nested list");
            };
            assert_eq!(cols.len(), 3);
            for c in cols {
                let Value::Integer(n) = c else {
                    panic!("expected integer");
                };
                let n = n.to_i64().unwrap();
                assert!((-2..=2).contains(&n));
            }
        }
    }

    #[test]
    fn test_random_integer_one_arg_inclusive() {
        seed_rng(1);
        for _ in 0..50 {
            let Value::Integer(n) = builtin_random_integer(&[int(1)]).unwrap() else {
                panic!("expected integer");
            };
            let n = n.to_i64().unwrap();
            assert!(n == 0 || n == 1);
        }
    }

    #[test]
    fn test_random_integer_big_safe() {
        let hi = Integer::from(10).pow(30);
        let hi_val = Value::Integer(hi.clone());
        seed_rng(7);
        for _ in 0..20 {
            let Value::Integer(n) =
                builtin_random_integer(&[list(vec![int(0), hi_val.clone()])]).unwrap()
            else {
                panic!("expected integer");
            };
            assert!(n >= 0 && n <= hi);
        }
    }

    // ── RandomChoice ──────────────────────────────────────────────────────────

    #[test]
    fn test_random_choice_basic() {
        let v = builtin_random_choice(&[list(vec![int(1), int(2), int(3)])]).unwrap();
        assert!(matches!(v, Value::Integer(_)));
    }

    #[test]
    fn test_random_choice_count() {
        let v = builtin_random_choice(&[list(vec![int(1), int(2)]), int(10)]).unwrap();
        let Value::List(items) = v else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 10);
        assert!(items.iter().all(|i| *i == int(1) || *i == int(2)));
    }

    #[test]
    fn test_random_choice_weighted_zero() {
        let rule = Value::Rule {
            lhs: Box::new(list(vec![int(1), int(0)])),
            rhs: Box::new(list(vec![Value::Str("a".into()), Value::Str("b".into())])),
            delayed: false,
        };
        seed_rng(3);
        for _ in 0..30 {
            assert_eq!(
                builtin_random_choice(&[rule.clone()]).unwrap(),
                Value::Str("a".into())
            );
        }
    }

    #[test]
    fn test_random_choice_weighted_length_mismatch() {
        let rule = Value::Rule {
            lhs: Box::new(list(vec![int(1)])),
            rhs: Box::new(list(vec![int(1), int(2)])),
            delayed: false,
        };
        assert!(matches!(
            builtin_random_choice(&[rule]),
            Err(EvalError::Error(_))
        ));
    }

    #[test]
    fn test_random_choice_array_dims() {
        let v = builtin_random_choice(&[list(vec![int(1), int(2)]), list(vec![int(2), int(4)])])
            .unwrap();
        let Value::List(rows) = v else {
            panic!("expected list");
        };
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .all(|r| matches!(r, Value::List(c) if c.len() == 4))
        );
    }

    // ── RandomComplex ─────────────────────────────────────────────────────────

    #[test]
    fn test_random_complex_bounds() {
        let v = builtin_random_complex(&[int(2)]).unwrap();
        let Value::Complex { re, im } = v else {
            panic!("expected complex");
        };
        assert!((0.0..=2.0).contains(&re) && (0.0..=2.0).contains(&im));
    }

    #[test]
    fn test_random_complex_array() {
        let v = builtin_random_complex(&[int(2), list(vec![int(2), int(3)])]).unwrap();
        let Value::List(rows) = v else {
            panic!("expected list");
        };
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .all(|r| matches!(r, Value::List(c) if c.len() == 3))
        );
    }

    #[test]
    fn test_random_complex_rect_corners() {
        seed_rng(9);
        for _ in 0..20 {
            let spec = list(vec![
                Value::Complex { re: 1.0, im: 2.0 },
                Value::Complex { re: 3.0, im: 4.0 },
            ]);
            let Value::Complex { re, im } = builtin_random_complex(&[spec]).unwrap() else {
                panic!("expected complex");
            };
            assert!((1.0..=3.0).contains(&re) && (2.0..=4.0).contains(&im));
        }
    }

    #[test]
    fn test_random_complex_polar_pi_spec() {
        seed_rng(13);
        let v = eval_str("RandomComplex[{{1, 2}, {0, Pi/2}}, \"Polar\"]");
        let Value::Complex { re, im } = v else {
            panic!("expected complex");
        };
        let modulus = (re * re + im * im).sqrt();
        assert!((1.0 - 1e-9..=2.0 + 1e-9).contains(&modulus));
    }

    #[test]
    fn test_random_real_pi_bounds() {
        let v = eval_str("RandomReal[{0, Pi}]");
        let f = as_f64(&v);
        assert!((0.0..=std::f64::consts::PI).contains(&f));
    }

    #[test]
    fn test_random_complex_polar_modulus() {
        seed_rng(11);
        let spec = list(vec![
            list(vec![real(1.0), real(2.0)]),
            list(vec![real(0.0), real(std::f64::consts::TAU)]),
        ]);
        for _ in 0..20 {
            let v = builtin_random_complex(&[spec.clone(), Value::Str("Polar".into())]).unwrap();
            let Value::Complex { re, im } = v else {
                panic!("expected complex");
            };
            let modulus = (re * re + im * im).sqrt();
            assert!((1.0 - 1e-9..=2.0 + 1e-9).contains(&modulus));
        }
    }

    // ── RandomVariate ─────────────────────────────────────────────────────────

    fn normal_dist() -> Value {
        let mut m = HashMap::new();
        m.insert("Distribution".to_string(), Value::Str("Normal".to_string()));
        m.insert("Mean".to_string(), real(0.0));
        m.insert("SD".to_string(), real(1.0));
        Value::Assoc(m)
    }

    #[test]
    fn test_random_variate_single_and_list() {
        let dist = normal_dist();
        assert!(matches!(
            builtin_random_variate(&[dist.clone()]).unwrap(),
            Value::Real(_)
        ));
        let v = builtin_random_variate(&[dist, int(7)]).unwrap();
        let Value::List(items) = v else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 7);
    }

    #[test]
    fn test_random_variate_array_dims() {
        let dist = normal_dist();
        let v = builtin_random_variate(&[dist, list(vec![int(2), int(5)])]).unwrap();
        let Value::List(rows) = v else {
            panic!("expected list");
        };
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .all(|r| matches!(r, Value::List(c) if c.len() == 5))
        );
    }

    #[test]
    fn test_random_variate_all_distributions() {
        let cases = [
            "NormalDistribution[0, 1]",
            "UniformDistribution[0, 1]",
            "PoissonDistribution[4]",
            "BinomialDistribution[10, 0.3]",
            "BernoulliDistribution[0.4]",
            "ExponentialDistribution[2]",
            "GammaDistribution[2, 1]",
            "ChiSquareDistribution[3]",
            "StudentTDistribution[5]",
            "BetaDistribution[2, 3]",
            "LogNormalDistribution[0, 1]",
            "CauchyDistribution[0, 1]",
            "DiscreteUniformDistribution[{0, 9}]",
        ];
        for case in cases {
            let expr = format!("RandomVariate[{case}, 6]");
            let v = eval_str(&expr);
            let Value::List(items) = v else {
                panic!("expected list for {case}");
            };
            assert_eq!(items.len(), 6, "wrong length for {case}");
        }
    }

    #[test]
    fn test_random_variate_seed_reproducible() {
        seed_rng(123);
        let a = eval_str("RandomVariate[NormalDistribution[0, 1], 5]");
        seed_rng(123);
        let b = eval_str("RandomVariate[NormalDistribution[0, 1], 5]");
        assert_eq!(a, b);
    }

    #[test]
    fn test_random_array_dims() {
        let v = builtin_random_array(&[normal_dist(), list(vec![int(2), int(3)])]).unwrap();
        let Value::List(rows) = v else {
            panic!("expected list");
        };
        assert_eq!(rows.len(), 2);
        assert!(matches!(&rows[0], Value::List(cols) if cols.len() == 3));
    }

    // ── RandomSample / RandomPermutation ──────────────────────────────────────

    #[test]
    fn test_random_sample_full_permutation() {
        let v = builtin_random_sample(&[list(vec![int(1), int(2), int(3), int(4)])]).unwrap();
        let Value::List(items) = v else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 4);
        let mut sorted: Vec<i64> = items.iter().map(|v| v.to_integer().unwrap()).collect();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_random_sample_without_replacement_distinct() {
        seed_rng(5);
        for _ in 0..20 {
            let v = builtin_random_sample(&[
                list(vec![int(1), int(2), int(3), int(4), int(5)]),
                int(3),
            ])
            .unwrap();
            let Value::List(items) = v else {
                panic!("expected list");
            };
            assert_eq!(items.len(), 3);
            let mut sorted: Vec<i64> = items.iter().map(|v| v.to_integer().unwrap()).collect();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), 3);
        }
    }

    #[test]
    fn test_random_sample_with_replacement() {
        let v = builtin_random_sample(&[
            list(vec![int(1), int(2), int(3)]),
            int(10),
            Value::Bool(true),
        ])
        .unwrap();
        let Value::List(items) = v else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 10);
    }

    #[test]
    fn test_random_sample_empty() {
        assert_eq!(
            builtin_random_sample(&[list(vec![])]).unwrap(),
            Value::List(vec![])
        );
        assert!(matches!(
            builtin_random_sample(&[list(vec![]), list(vec![int(2)])]),
            Err(EvalError::Error(_))
        ));
    }

    #[test]
    fn test_random_sample_too_many_without_replacement() {
        let r = builtin_random_sample(&[list(vec![int(1), int(2)]), int(5)]);
        assert!(matches!(r, Err(EvalError::Error(_))));
    }

    #[test]
    fn test_random_permutation() {
        seed_rng(2);
        let v = builtin_random_permutation(&[int(6)]).unwrap();
        let Value::List(items) = v else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 6);
        let mut sorted: Vec<i64> = items.iter().map(|v| v.to_integer().unwrap()).collect();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![1, 2, 3, 4, 5, 6]);
    }

    // ── RandomPrime ───────────────────────────────────────────────────────────

    #[test]
    fn test_random_prime_single() {
        seed_rng(4);
        for _ in 0..20 {
            let Value::Integer(n) = builtin_random_prime(&[int(100)]).unwrap() else {
                panic!("expected integer");
            };
            assert!((2..=100).contains(&n) && is_prime(&n));
        }
    }

    #[test]
    fn test_random_prime_range_count() {
        let v = builtin_random_prime(&[list(vec![int(10), int(50)]), int(5)]).unwrap();
        let Value::List(items) = v else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 5);
        for it in items {
            let Value::Integer(n) = it else {
                panic!("expected integer");
            };
            assert!((10..=50).contains(&n) && is_prime(&n));
        }
    }

    #[test]
    fn test_random_prime_big_safe() {
        let lo = Integer::from(10).pow(30);
        let hi = lo.clone() + Integer::from(100_000);
        seed_rng(17);
        for _ in 0..5 {
            let Value::Integer(n) = builtin_random_prime(&[list(vec![
                Value::Integer(lo.clone()),
                Value::Integer(hi.clone()),
            ])])
            .unwrap() else {
                panic!("expected integer");
            };
            assert!(n >= lo && n <= hi && is_prime(&n));
        }
    }

    // ── SeedRandom / BlockRandom ──────────────────────────────────────────────

    #[test]
    fn test_seed_random_with_method_reproducible() {
        let method = Value::Rule {
            lhs: Box::new(Value::Str("Method".into())),
            rhs: Box::new(Value::Str("MersenneTwister".into())),
            delayed: false,
        };
        builtin_seed_random(&[int(9), method.clone()]).unwrap();
        let a = builtin_random_real(&[]).unwrap();
        builtin_seed_random(&[int(9), method]).unwrap();
        let b = builtin_random_real(&[]).unwrap();
        assert_eq!(a, b);

        // A different method yields a different stream for the same seed.
        builtin_seed_random(&[int(9), Value::Str("Other".into())]).unwrap();
        let c = builtin_random_real(&[]).unwrap();
        assert_ne!(a, c);
    }

    #[test]
    fn test_block_random_restores_state() {
        seed_rng(7);
        let expected = eval_str("RandomReal[]");
        seed_rng(7);
        let blocked = eval_str("BlockRandom[RandomReal[]]");
        let after = eval_str("RandomReal[]");
        assert_eq!(blocked, expected);
        assert_eq!(after, expected);
    }

    #[test]
    fn test_block_random_returns_value() {
        let v = eval_str("BlockRandom[RandomInteger[{0, 5}]]");
        assert!(matches!(v, Value::Integer(_)));
    }

    #[test]
    fn test_block_random_seed_argument() {
        // A seed argument makes the localized stream reproducible.
        let a = eval_str("BlockRandom[RandomReal[], 5]");
        let b = eval_str("BlockRandom[RandomReal[], 5]");
        assert_eq!(a, b);
    }

    #[test]
    fn test_seed_random_via_eval_method() {
        let a = eval_str("SeedRandom[3, Method -> \"MersenneTwister\"]; RandomReal[]");
        let b = eval_str("SeedRandom[3, Method -> \"MersenneTwister\"]; RandomReal[]");
        assert_eq!(a, b);
    }

    #[test]
    fn test_random_choice_sample_via_eval() {
        let c = eval_str("RandomChoice[{4, 1} -> {1, 2}, 6]");
        assert!(matches!(&c, Value::List(items) if items.len() == 6));
        let s = eval_str("RandomSample[Range[10], 4]");
        assert!(matches!(&s, Value::List(items) if items.len() == 4));
        let p = eval_str("RandomPermutation[5]");
        assert!(matches!(&p, Value::List(items) if items.len() == 5));
    }

    // ── RandomGraph ───────────────────────────────────────────────────────────

    #[test]
    fn test_random_graph_consistency() {
        seed_rng(7);
        let a = eval_str("EdgeList[RandomGraph[{5, 4}]]");
        seed_rng(7);
        let b = eval_str("EdgeList[RandomGraph[{5, 4}]]");
        assert_eq!(a, b);
        assert_eq!(eval_str("VertexCount[RandomGraph[{5, 4}]]"), int(5));
        assert_eq!(eval_str("EdgeCount[RandomGraph[{5, 4}]]"), int(4));
        assert_eq!(eval_str("EdgeCount[RandomGraph[{5, 0.0}]]"), int(0));
    }
}
