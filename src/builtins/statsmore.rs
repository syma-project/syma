//! statistics/probability: SeedRandom, Quartiles, moments, Expectation/Probability/InverseCDF, BinLists.
//!
//! Gap-fill module; see the individual builtin docs for semantics.

#![allow(clippy::needless_range_loop)]

use crate::ast::Expr;
use crate::builtins::statistics as st;
use crate::env::Env;
use crate::value::EvalError;
use crate::value::Value;
use crate::value::rational_value;
use rug::Integer;
use std::collections::HashMap;

/// Convert an AST `Expr` to a structural `Value` without evaluating it.
fn expr_to_value(expr: &Expr) -> Value {
    match expr {
        Expr::Integer(n) => Value::Integer(n.clone()),
        Expr::Real(r) => Value::Real(r.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Str(s) => Value::Str(s.clone()),
        Expr::Null => Value::Null,
        Expr::Symbol(s) => Value::Symbol(s.clone()),
        Expr::List(items) => Value::List(items.iter().map(expr_to_value).collect()),
        Expr::Call { head, args } => Value::Call {
            head: match head.as_ref() {
                Expr::Symbol(s) => s.clone(),
                _ => String::new(),
            },
            args: args.iter().map(expr_to_value).collect(),
        },
        _ => Value::Pattern(expr.clone()),
    }
}

/// Unwrap a `HoldFirst`-held argument (`Value::Pattern`) into a plain value.
fn held_value(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => expr_to_value(e),
        other => other.clone(),
    }
}

// ── Small helpers ───────────────────────────────────────────────────────────

fn extract_nums(items: &[Value]) -> Result<Vec<f64>, EvalError> {
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

fn int(n: i64) -> Value {
    Value::Integer(rug::Integer::from(n))
}

/// Return an exact `Integer` when `f` is integral, otherwise a `Real`.
fn num(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e15 {
        int(f as i64)
    } else {
        super::real(f)
    }
}

fn unevaluated(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::NoMatch {
        head: name.to_string(),
        args: Box::new(args.to_vec()),
    })
}

/// Borrow `args[idx]` as a list, returning the call unevaluated otherwise.
fn data_list<'a>(name: &str, args: &'a [Value], idx: usize) -> Result<&'a Vec<Value>, EvalError> {
    match &args[idx] {
        Value::List(items) => Ok(items),
        _ => Err(EvalError::NoMatch {
            head: name.to_string(),
            args: Box::new(args.to_vec()),
        }),
    }
}

/// Extract the `f64` value of a `Value::Real`/`Value::Integer`.
fn as_f64(v: &Value) -> Option<f64> {
    super::to_f64(v)
}

// ── Distribution plumbing ───────────────────────────────────────────────────

fn dist_type(dist: &HashMap<String, Value>) -> Option<&str> {
    dist.get("Distribution").and_then(|v| match v {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    })
}

fn is_discrete(dist: &HashMap<String, Value>) -> bool {
    matches!(
        dist_type(dist),
        Some("Poisson" | "Binomial" | "Bernoulli" | "DiscreteUniform")
    )
}

fn pmf_at(dist: &HashMap<String, Value>, x: f64) -> Option<f64> {
    match st::builtin_pdf(&[Value::Assoc(dist.clone()), super::real(x)]) {
        Ok(v) => as_f64(&v),
        Err(_) => None,
    }
}

fn cdf_at(dist: &HashMap<String, Value>, x: f64) -> Option<f64> {
    match st::builtin_cdf(&[Value::Assoc(dist.clone()), super::real(x)]) {
        Ok(v) => as_f64(&v),
        Err(_) => None,
    }
}

/// A finite bracket containing essentially all of the distribution's mass.
fn support(dist: &HashMap<String, Value>) -> Option<(f64, f64)> {
    let ty = dist_type(dist)?;
    let p = |k: &str| dist.get(k).and_then(as_f64);
    match ty {
        "Normal" => {
            let mu = p("Mean")?;
            let sd = p("SD")?;
            Some((mu - 12.0 * sd, mu + 12.0 * sd))
        }
        "Uniform" => Some((p("Min")?, p("Max")?)),
        "Exponential" => {
            let lambda = p("Lambda")?;
            Some((0.0, 60.0 / lambda))
        }
        "Gamma" | "ChiSquare" => {
            let m = st::distribution_mean_f64(dist)?;
            let v = st::distribution_variance_f64(dist)?;
            let sd = v.max(0.0).sqrt();
            Some((0.0, m + 60.0 * sd))
        }
        "Beta" => Some((0.0, 1.0)),
        "LogNormal" => {
            let mu = p("Mu")?;
            let sigma = p("Sigma")?;
            Some((0.0, (mu + 20.0 * sigma).exp()))
        }
        "Cauchy" => {
            let x0 = p("Location")?;
            let g = p("Scale")?;
            Some((x0 - 1.0e5 * g, x0 + 1.0e5 * g))
        }
        "StudentT" => {
            let nu = p("Nu")?;
            let m = st::distribution_mean_f64(dist).unwrap_or(0.0);
            let v = st::distribution_variance_f64(dist).unwrap_or(nu);
            let sd = v.max(0.0).sqrt().max(1.0);
            Some((m - 1.0e4 * sd, m + 1.0e4 * sd))
        }
        "Poisson" => {
            let lambda = p("Lambda")?;
            Some((0.0, lambda + 20.0 * lambda.sqrt() + 30.0))
        }
        "Binomial" => Some((0.0, p("N")?)),
        "Bernoulli" => Some((0.0, 1.0)),
        "DiscreteUniform" => Some((p("Min")?, p("Max")?)),
        _ => None,
    }
}

/// Composite Simpson's rule with `n` (forced even) subintervals.
fn simpson(f: impl Fn(f64) -> f64, a: f64, b: f64, n: usize) -> f64 {
    if b <= a {
        return 0.0;
    }
    let n = if n.is_multiple_of(2) { n } else { n + 1 };
    let h = (b - a) / n as f64;
    let mut s = f(a) + f(b);
    for i in 1..n {
        let x = a + i as f64 * h;
        s += if i % 2 == 1 { 4.0 } else { 2.0 } * f(x);
    }
    s * h / 3.0
}

/// E[g(X)] for a distribution, computed by quadrature (continuous) or summation (discrete).
fn expectation_with(dist: &HashMap<String, Value>, g: impl Fn(f64) -> f64) -> Option<f64> {
    let (lo, hi) = support(dist)?;
    if is_discrete(dist) {
        let start = lo.floor() as i64;
        let end = hi.ceil() as i64;
        let mut acc = 0.0;
        for k in start..=end {
            if let Some(p) = pmf_at(dist, k as f64) {
                acc += g(k as f64) * p;
            }
        }
        Some(acc)
    } else {
        let dens = |x: f64| -> f64 {
            match st::builtin_pdf(&[Value::Assoc(dist.clone()), super::real(x)]) {
                Ok(v) => as_f64(&v).unwrap_or(0.0),
                Err(_) => 0.0,
            }
        };
        Some(simpson(move |x| g(x) * dens(x), lo, hi, 4000))
    }
}

// ── Expression evaluation over a single variable ────────────────────────────

const CONST_SYMBOLS: &[&str] = &["Pi", "E", "I", "Degree", "Infinity", "True", "False"];

/// Find the (last) free variable of an expression, ignoring known constants.
fn find_variable(expr: &Value) -> Option<String> {
    fn scan(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Symbol(s) => {
                if !CONST_SYMBOLS.contains(&s.as_str()) {
                    out.push(s.clone());
                }
            }
            Value::Call { args, .. } => {
                for a in args {
                    scan(a, out);
                }
            }
            Value::List(items) => {
                for a in items {
                    scan(a, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    scan(expr, &mut out);
    out.pop()
}

/// Numerically evaluate a single-variable expression at `x`.
fn eval_numeric(expr: &Value, var: &Option<String>, x: f64) -> Option<f64> {
    match expr {
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) => as_f64(expr),
        Value::Complex { re, im } if *im == 0.0 => Some(*re),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::Symbol(s) => match s.as_str() {
            "Pi" => Some(std::f64::consts::PI),
            "E" => Some(std::f64::consts::E),
            "Degree" => Some(std::f64::consts::PI / 180.0),
            _ => {
                if Some(s) == var.as_ref() {
                    Some(x)
                } else {
                    None
                }
            }
        },
        Value::Call { head, args } => {
            let vals: Option<Vec<f64>> = args
                .iter()
                .map(|a| eval_numeric(a, var, x))
                .collect::<Option<Vec<f64>>>();
            match head.as_str() {
                "Plus" => Some(vals?.iter().sum()),
                "Times" => Some(vals?.iter().product()),
                "Power" if args.len() == 2 => {
                    let v = vals?;
                    Some(v[0].powf(v[1]))
                }
                "Exp" if args.len() == 1 => Some(eval_numeric(&args[0], var, x)?.exp()),
                "Log" if args.len() == 1 => Some(eval_numeric(&args[0], var, x)?.ln()),
                "Sqrt" if args.len() == 1 => Some(eval_numeric(&args[0], var, x)?.sqrt()),
                "Sin" if args.len() == 1 => Some(eval_numeric(&args[0], var, x)?.sin()),
                "Cos" if args.len() == 1 => Some(eval_numeric(&args[0], var, x)?.cos()),
                "Tan" if args.len() == 1 => Some(eval_numeric(&args[0], var, x)?.tan()),
                "Abs" if args.len() == 1 => Some(eval_numeric(&args[0], var, x)?.abs()),
                _ => None,
            }
        }
        _ => None,
    }
}

// ── SeedRandom ──────────────────────────────────────────────────────────────

/// SeedRandom[n] / SeedRandom[] — (re)seed the shared random number generator.
pub fn builtin_seed_random(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        0 => {
            crate::builtins::random::reseed_entropy();
            Ok(Value::Null)
        }
        1 => {
            let seed = match &args[0] {
                Value::Integer(n) => n.to_u64().unwrap_or(1),
                Value::Real(r) => r.to_f64() as i64 as u64,
                Value::Str(s) => fnv1a(s),
                _ => return unevaluated("SeedRandom", args),
            };
            crate::builtins::random::seed_rng(seed);
            Ok(Value::Null)
        }
        _ => unevaluated("SeedRandom", args),
    }
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// ── Quartiles ───────────────────────────────────────────────────────────────

/// Quartiles[list] — {q1, q2, q3} using the same interpolation as Quantile.
pub fn builtin_quartiles(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return unevaluated("Quartiles", args);
    }
    if let Value::Assoc(m) = &args[0]
        && m.contains_key("Distribution")
    {
        let mut out = Vec::with_capacity(3);
        for q in [0.25, 0.5, 0.75] {
            match inverse_cdf_f(m, q) {
                Some(v) => out.push(num(v)),
                None => return unevaluated("Quartiles", args),
            }
        }
        return Ok(Value::List(out));
    }
    let items = data_list("Quartiles", args, 0)?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "Quartiles: list must not be empty".to_string(),
        ));
    }
    let mut out = Vec::with_capacity(3);
    for q in [0.25, 0.5, 0.75] {
        out.push(st::builtin_quantile(&[args[0].clone(), super::real(q)])?);
    }
    Ok(Value::List(out))
}

// ── Raw and central moments ─────────────────────────────────────────────────

/// Moment[list, r] — raw moment about 0. Moment[dist, r] — E[X^r].
pub fn builtin_moment(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("Moment", args);
    }
    let r = match as_f64(&args[1]) {
        Some(r) => r,
        None => return unevaluated("Moment", args),
    };
    if let Value::Assoc(m) = &args[0]
        && m.contains_key("Distribution")
    {
        return match expectation_with(m, |x| x.powf(r)) {
            Some(v) => Ok(num(v)),
            None => unevaluated("Moment", args),
        };
    }
    let items = data_list("Moment", args, 0)?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "Moment: list must not be empty".to_string(),
        ));
    }
    let nums = extract_nums(items)?;
    let n = nums.len() as f64;
    let m: f64 = nums.iter().map(|x| x.powf(r)).sum::<f64>() / n;
    Ok(num(m))
}

/// CentralMoment[list, r] — moment about the mean. CentralMoment[dist, r] — E[(X-mean)^r].
pub fn builtin_central_moment(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("CentralMoment", args);
    }
    let r = match as_f64(&args[1]) {
        Some(r) => r,
        None => return unevaluated("CentralMoment", args),
    };
    if let Value::Assoc(m) = &args[0]
        && m.contains_key("Distribution")
    {
        let mu = st::distribution_mean_f64(m).unwrap_or(0.0);
        return match expectation_with(m, move |x| (x - mu).powf(r)) {
            Some(v) => Ok(num(v)),
            None => unevaluated("CentralMoment", args),
        };
    }
    let items = data_list("CentralMoment", args, 0)?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "CentralMoment: list must not be empty".to_string(),
        ));
    }
    let nums = extract_nums(items)?;
    let n = nums.len() as f64;
    let mean = nums.iter().sum::<f64>() / n;
    let m: f64 = nums.iter().map(|x| (x - mean).powf(r)).sum::<f64>() / n;
    Ok(num(m))
}

// ── Expectation ─────────────────────────────────────────────────────────────

/// Build an unevaluated `Expectation[expr, dist]` call.
fn expect_call(expr: &Value, dist: &Value) -> Value {
    Value::Call {
        head: "Expectation".to_string(),
        args: vec![expr.clone(), dist.clone()],
    }
}

/// Expectation[expr, dist] — expected value of `expr` for a distribution.
pub fn builtin_expectation(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("Expectation", args);
    }
    let expr = held_value(&args[0]);
    let dist = match &args[1] {
        Value::Assoc(m) if m.contains_key("Distribution") => m,
        _ => return Ok(expect_call(&expr, &args[1])),
    };
    let var = find_variable(&expr);
    let v = var.clone();
    if var.is_none() {
        // Constant integrand.
        return match eval_numeric(&expr, &v, 0.0) {
            Some(c) => Ok(num(c)),
            None => Ok(expect_call(&expr, &args[1])),
        };
    }
    // Probe the expression; if it cannot be evaluated numerically, bail out.
    if eval_numeric(&expr, &v, 0.0).is_none() {
        return Ok(expect_call(&expr, &args[1]));
    }
    let integrand = expr.clone();
    match expectation_with(dist, move |x| {
        eval_numeric(&integrand, &v, x).unwrap_or(0.0)
    }) {
        Some(val) => Ok(num(val)),
        None => Ok(expect_call(&expr, &args[1])),
    }
}

// ── Probability ─────────────────────────────────────────────────────────────

fn is_cmp(head: &str) -> bool {
    matches!(
        head,
        "Less" | "LessEqual" | "Greater" | "GreaterEqual" | "Equal"
    )
}

fn reverse_op(op: &str) -> &str {
    match op {
        "Less" => "Greater",
        "LessEqual" => "GreaterEqual",
        "Greater" => "Less",
        "GreaterEqual" => "LessEqual",
        other => other,
    }
}

fn rightmost(v: &Value) -> &Value {
    match v {
        Value::Call { head, args } if is_cmp(head) && !args.is_empty() => {
            rightmost(args.last().unwrap())
        }
        _ => v,
    }
}

struct Bounds {
    lo: Option<(f64, bool)>,
    hi: Option<(f64, bool)>,
    eq: Option<f64>,
}

impl Bounds {
    fn add(&mut self, op: &str, val: f64) {
        match op {
            "Less" => self.set_hi(val, false),
            "LessEqual" => self.set_hi(val, true),
            "Greater" => self.set_lo(val, false),
            "GreaterEqual" => self.set_lo(val, true),
            "Equal" => self.eq = Some(val),
            _ => {}
        }
    }

    fn set_hi(&mut self, val: f64, inc: bool) {
        match self.hi {
            None => self.hi = Some((val, inc)),
            Some((v, _)) => {
                if val < v || (val == v && !inc) {
                    self.hi = Some((val, inc));
                }
            }
        }
    }

    fn set_lo(&mut self, val: f64, inc: bool) {
        match self.lo {
            None => self.lo = Some((val, inc)),
            Some((v, _)) => {
                if val > v || (val == v && !inc) {
                    self.lo = Some((val, inc));
                }
            }
        }
    }
}

fn parse_event(expr: &Value, var: &str, b: &mut Bounds) -> bool {
    match expr {
        Value::Bool(true) => true,
        Value::Call { head, args } if head == "And" => args.iter().all(|a| parse_event(a, var, b)),
        Value::Call { head, args } if is_cmp(head) && args.len() >= 2 => {
            let mut left = &args[0];
            for right in &args[1..] {
                if let Value::Call { head: h, .. } = left
                    && is_cmp(h)
                {
                    if !parse_event(left, var, b) {
                        return false;
                    }
                    let operand = rightmost(left).clone();
                    if !apply_pair(head, &operand, right, var, b) {
                        return false;
                    }
                } else if !apply_pair(head, left, right, var, b) {
                    return false;
                }
                left = right;
            }
            true
        }
        _ => false,
    }
}

fn apply_pair(op: &str, l: &Value, r: &Value, var: &str, b: &mut Bounds) -> bool {
    let lv = as_f64(l);
    let rv = as_f64(r);
    let l_is_var = matches!(l, Value::Symbol(s) if s.as_str() == var);
    let r_is_var = matches!(r, Value::Symbol(s) if s.as_str() == var);
    if l_is_var && let Some(v) = rv {
        b.add(op, v);
        true
    } else if r_is_var && let Some(v) = lv {
        b.add(reverse_op(op), v);
        true
    } else {
        lv.is_some() && rv.is_some()
    }
}

/// Build an unevaluated `Probability[event, dist]` call.
fn prob_call(event: &Value, dist: &Value) -> Value {
    Value::Call {
        head: "Probability".to_string(),
        args: vec![event.clone(), dist.clone()],
    }
}

/// Probability[event, dist] — probability that a random variate satisfies `event`.
pub fn builtin_probability(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("Probability", args);
    }
    let event = held_value(&args[0]);
    let dist = match &args[1] {
        Value::Assoc(m) if m.contains_key("Distribution") => m,
        _ => return Ok(prob_call(&event, &args[1])),
    };
    let var = match find_variable(&event) {
        Some(v) => v,
        None => {
            return match &event {
                Value::Bool(true) => Ok(int(1)),
                Value::Bool(false) => Ok(int(0)),
                _ => Ok(prob_call(&event, &args[1])),
            };
        }
    };
    let mut b = Bounds {
        lo: None,
        hi: None,
        eq: None,
    };
    if !parse_event(&event, &var, &mut b) {
        return Ok(prob_call(&event, &args[1]));
    }
    let discrete = is_discrete(dist);
    let p = if let Some(c) = b.eq {
        if discrete {
            match pmf_at(dist, c) {
                Some(v) => v,
                None => return Ok(prob_call(&event, &args[1])),
            }
        } else {
            0.0
        }
    } else {
        let lower = match b.lo {
            None => 0.0,
            Some((a, inc)) => {
                let f = match cdf_at(dist, a) {
                    Some(f) => f,
                    None => return Ok(prob_call(&event, &args[1])),
                };
                if inc && discrete {
                    f - pmf_at(dist, a).unwrap_or(0.0)
                } else {
                    f
                }
            }
        };
        let upper = match b.hi {
            None => 1.0,
            Some((z, inc)) => {
                let f = match cdf_at(dist, z) {
                    Some(f) => f,
                    None => return Ok(prob_call(&event, &args[1])),
                };
                if !inc && discrete {
                    f - pmf_at(dist, z).unwrap_or(0.0)
                } else {
                    f
                }
            }
        };
        upper - lower
    };
    Ok(num(p.clamp(0.0, 1.0)))
}

// ── InverseCDF ──────────────────────────────────────────────────────────────

fn inverse_cdf_f(dist: &HashMap<String, Value>, q: f64) -> Option<f64> {
    let (lo, hi) = support(dist)?;
    if q <= 0.0 {
        return Some(lo);
    }
    if q >= 1.0 {
        return Some(hi);
    }
    if is_discrete(dist) {
        let start = lo.floor() as i64;
        let end = hi.ceil() as i64;
        for k in start..=end {
            if cdf_at(dist, k as f64)? >= q {
                return Some(k as f64);
            }
        }
        return Some(hi);
    }
    let mut a = lo;
    let mut z = hi;
    for _ in 0..200 {
        let m = 0.5 * (a + z);
        if cdf_at(dist, m)? < q {
            a = m;
        } else {
            z = m;
        }
    }
    Some(0.5 * (a + z))
}

/// InverseCDF[dist, q] — q-th quantile of a distribution.
pub fn builtin_inverse_cdf(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("InverseCDF", args);
    }
    let dist = match &args[0] {
        Value::Assoc(m) if m.contains_key("Distribution") => m,
        _ => return unevaluated("InverseCDF", args),
    };
    let q = match as_f64(&args[1]) {
        Some(q) if (0.0..=1.0).contains(&q) => q,
        _ => return unevaluated("InverseCDF", args),
    };
    match inverse_cdf_f(dist, q) {
        Some(v) => Ok(num(v)),
        None => unevaluated("InverseCDF", args),
    }
}

// ── MomentGeneratingFunction / CharacteristicFunction ───────────────────────

/// Builders for symbolic results (no simplification beyond numeric folding).
fn b_add(a: Value, b: Value) -> Value {
    if let (Some(x), Some(y)) = (as_f64(&a), as_f64(&b)) {
        return num(x + y);
    }
    if as_f64(&a) == Some(0.0) {
        return b;
    }
    if as_f64(&b) == Some(0.0) {
        return a;
    }
    Value::Call {
        head: "Plus".to_string(),
        args: vec![a, b],
    }
}

fn b_mul(a: Value, b: Value) -> Value {
    if let (Some(x), Some(y)) = (as_f64(&a), as_f64(&b)) {
        return num(x * y);
    }
    if as_f64(&a) == Some(0.0) {
        return num(0.0);
    }
    if as_f64(&b) == Some(0.0) {
        return num(0.0);
    }
    if as_f64(&a) == Some(1.0) {
        return b;
    }
    if as_f64(&b) == Some(1.0) {
        return a;
    }
    Value::Call {
        head: "Times".to_string(),
        args: vec![a, b],
    }
}

fn b_neg(a: Value) -> Value {
    if let Some(x) = as_f64(&a) {
        return num(-x);
    }
    b_mul(num(-1.0), a)
}

fn b_pow(a: Value, b: Value) -> Value {
    if let (Some(x), Some(y)) = (as_f64(&a), as_f64(&b))
        && (x >= 0.0 || y.fract() == 0.0)
    {
        return num(x.powf(y));
    }
    Value::Call {
        head: "Power".to_string(),
        args: vec![a, b],
    }
}

fn big_e() -> Value {
    Value::Symbol("E".to_string())
}

/// MomentGeneratingFunction[dist, t] — M(t) = E[e^{t X}].
pub fn builtin_moment_generating_function(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("MomentGeneratingFunction", args);
    }
    let dist = match &args[0] {
        Value::Assoc(m) if m.contains_key("Distribution") => m,
        _ => return unevaluated("MomentGeneratingFunction", args),
    };
    let p = |k: &str| dist.get(k).and_then(as_f64);
    let t_val = as_f64(&args[1]);
    let ty = match dist_type(dist) {
        Some(ty) => ty,
        None => return unevaluated("MomentGeneratingFunction", args),
    };
    let t = args[1].clone();

    let result: Value = match ty {
        "Normal" => {
            let mu = p("Mean").unwrap_or(0.0);
            let sd = p("SD").unwrap_or(1.0);
            match t_val {
                Some(tv) => super::real((mu * tv + 0.5 * sd * sd * tv * tv).exp()),
                None => {
                    let z = b_add(
                        b_mul(num(mu), t.clone()),
                        b_mul(num(0.5 * sd * sd), b_pow(t.clone(), num(2.0))),
                    );
                    b_pow(big_e(), z)
                }
            }
        }
        "Uniform" => {
            let lo = p("Min").unwrap_or(0.0);
            let hi = p("Max").unwrap_or(1.0);
            match t_val {
                Some(tv) => {
                    if tv == 0.0 {
                        super::real(1.0)
                    } else {
                        super::real(((tv * hi).exp() - (tv * lo).exp()) / (tv * (hi - lo)))
                    }
                }
                None => {
                    let numr = b_add(
                        b_pow(big_e(), b_mul(t.clone(), num(hi))),
                        b_neg(b_pow(big_e(), b_mul(t.clone(), num(lo)))),
                    );
                    let den = b_mul(t.clone(), num(hi - lo));
                    b_mul(numr, b_pow(den, num(-1.0)))
                }
            }
        }
        "Exponential" => {
            let lambda = p("Lambda").unwrap_or(1.0);
            match t_val {
                Some(tv) => super::real(lambda / (lambda - tv)),
                None => b_mul(
                    num(lambda),
                    b_pow(b_add(num(lambda), b_neg(t.clone())), num(-1.0)),
                ),
            }
        }
        "Poisson" => {
            let lambda = p("Lambda").unwrap_or(1.0);
            match t_val {
                Some(tv) => super::real((lambda * (tv.exp() - 1.0)).exp()),
                None => {
                    let inner = b_mul(num(lambda), b_add(b_pow(big_e(), t.clone()), num(-1.0)));
                    b_pow(big_e(), inner)
                }
            }
        }
        "Binomial" => {
            let n = p("N").unwrap_or(1.0);
            let prob = p("P").unwrap_or(0.5);
            match t_val {
                Some(tv) => super::real((1.0 - prob + prob * tv.exp()).powf(n)),
                None => {
                    let base = b_add(num(1.0 - prob), b_mul(num(prob), b_pow(big_e(), t.clone())));
                    b_pow(base, num(n))
                }
            }
        }
        "Bernoulli" => {
            let prob = p("P").unwrap_or(0.5);
            match t_val {
                Some(tv) => super::real(1.0 - prob + prob * tv.exp()),
                None => b_add(num(1.0 - prob), b_mul(num(prob), b_pow(big_e(), t.clone()))),
            }
        }
        _ => return unevaluated("MomentGeneratingFunction", args),
    };
    Ok(result)
}

/// CharacteristicFunction[dist, t] — φ(t) = E[e^{i t X}] (numeric `t` only).
pub fn builtin_characteristic_function(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("CharacteristicFunction", args);
    }
    let dist = match &args[0] {
        Value::Assoc(m) if m.contains_key("Distribution") => m,
        _ => return unevaluated("CharacteristicFunction", args),
    };
    let t = match as_f64(&args[1]) {
        Some(t) => t,
        None => return unevaluated("CharacteristicFunction", args),
    };
    let p = |k: &str| dist.get(k).and_then(as_f64);
    let cexp = |re: f64| (re.cos(), re.sin());
    let (re, im): (f64, f64) = match dist_type(dist) {
        Some("Normal") => {
            let mu = p("Mean").unwrap_or(0.0);
            let sd = p("SD").unwrap_or(1.0);
            let (c, s) = cexp(mu * t);
            let g = (-0.5 * sd * sd * t * t).exp();
            (g * c, g * s)
        }
        Some("Uniform") => {
            let lo = p("Min").unwrap_or(0.0);
            let hi = p("Max").unwrap_or(1.0);
            if t == 0.0 {
                (1.0, 0.0)
            } else {
                // (e^{itb} - e^{ita}) / (i t (b-a))
                let (c1, s1) = cexp(t * hi);
                let (c2, s2) = cexp(t * lo);
                let numr = (c1 - c2, s1 - s2);
                let den = (-t * (hi - lo), t * (hi - lo)); // i*t*(b-a) = (0, t(b-a))
                cdiv(numr, den)
            }
        }
        Some("Exponential") => {
            let lambda = p("Lambda").unwrap_or(1.0);
            // λ / (λ - i t)
            cdiv((lambda, 0.0), (lambda, -t))
        }
        Some("Poisson") => {
            let lambda = p("Lambda").unwrap_or(1.0);
            let er = (lambda * (t.cos() - 1.0)).exp();
            let (wr, wi) = cexp(lambda * t.sin());
            (er * wr, er * wi)
        }
        Some("Binomial") => {
            let n = p("N").unwrap_or(1.0);
            let prob = p("P").unwrap_or(0.5);
            let (c, s) = cexp(t);
            // 1 - p + p e^{it}
            let base = (1.0 - prob + prob * c, prob * s);
            cpow(base, n)
        }
        Some("Bernoulli") => {
            let prob = p("P").unwrap_or(0.5);
            let (c, s) = cexp(t);
            (1.0 - prob + prob * c, prob * s)
        }
        _ => return unevaluated("CharacteristicFunction", args),
    };
    if im.abs() < 1e-15 {
        Ok(super::real(re))
    } else {
        Ok(Value::Complex { re, im })
    }
}

fn cdiv(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let d = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
}

fn cpow(a: (f64, f64), n: f64) -> (f64, f64) {
    let r = (a.0 * a.0 + a.1 * a.1).sqrt();
    let th = a.1.atan2(a.0);
    let rn = r.powf(n);
    (rn * (n * th).cos(), rn * (n * th).sin())
}

// ── BinLists ────────────────────────────────────────────────────────────────

/// BinLists[list, spec] — partition values into bins (mirrors BinCounts binning).
pub fn builtin_bin_lists(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("BinLists", args);
    }
    let items = data_list("BinLists", args, 0)?;
    if items.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let nums = extract_nums(items)?;
    let data_min = nums.iter().cloned().fold(f64::INFINITY, f64::min);
    let data_max = nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let (bin_min, bin_max, dx) = match &args[1] {
        Value::Integer(_) | Value::Real(_) => {
            let dx = as_f64(&args[1]).unwrap();
            (data_min, data_max, dx)
        }
        Value::List(spec) => match spec.len() {
            1 => {
                let dx = as_f64(&spec[0]).ok_or_else(|| {
                    EvalError::Error("BinLists: bin spec must contain numbers".to_string())
                })?;
                (data_min, data_max, dx)
            }
            3 => {
                let lo = as_f64(&spec[0]).ok_or_else(|| {
                    EvalError::Error("BinLists: bin spec must contain numbers".to_string())
                })?;
                let hi = as_f64(&spec[1]).ok_or_else(|| {
                    EvalError::Error("BinLists: bin spec must contain numbers".to_string())
                })?;
                let dx = as_f64(&spec[2]).ok_or_else(|| {
                    EvalError::Error("BinLists: bin spec must contain numbers".to_string())
                })?;
                (lo, hi, dx)
            }
            _ => {
                return Err(EvalError::Error(
                    "BinLists: bin spec must be {dx} or {min, max, dx}".to_string(),
                ));
            }
        },
        _ => {
            return Err(EvalError::Error(
                "BinLists: second argument must be a number or list".to_string(),
            ));
        }
    };
    if dx <= 0.0 || bin_min >= bin_max {
        return Err(EvalError::Error(
            "BinLists: invalid bin range or width".to_string(),
        ));
    }
    let n_bins = (((bin_max - bin_min) / dx).ceil() as usize).max(1);
    let mut bins: Vec<Vec<Value>> = vec![Vec::new(); n_bins];
    for (item, &x) in items.iter().zip(nums.iter()) {
        if x < bin_min || x > bin_max {
            continue;
        }
        let idx = if x >= bin_max {
            n_bins - 1
        } else {
            (((x - bin_min) / dx).floor() as usize).min(n_bins - 1)
        };
        bins[idx].push(item.clone());
    }
    Ok(Value::List(bins.into_iter().map(Value::List).collect()))
}

// ── ExponentialMovingAverage ────────────────────────────────────────────────

/// ExponentialMovingAverage[list, a] — s_1 = x_1, s_i = a x_i + (1-a) s_{i-1}.
pub fn builtin_exponential_moving_average(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("ExponentialMovingAverage", args);
    }
    let items = data_list("ExponentialMovingAverage", args, 0)?;
    let alpha = match as_f64(&args[1]) {
        Some(a) => a,
        None => return unevaluated("ExponentialMovingAverage", args),
    };
    if items.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let nums = extract_nums(items)?;
    let mut out = Vec::with_capacity(nums.len());
    let mut prev = nums[0];
    out.push(num(prev));
    for &x in &nums[1..] {
        prev = alpha * x + (1.0 - alpha) * prev;
        out.push(num(prev));
    }
    Ok(Value::List(out))
}

// ── SmoothHistogram ─────────────────────────────────────────────────────────

/// SmoothHistogram[data] — Gaussian kernel density estimate rendered as SVG.
pub fn builtin_smooth_histogram(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return unevaluated("SmoothHistogram", args);
    }
    let items = data_list("SmoothHistogram", args, 0)?;
    let nums = extract_nums(items)?;
    if nums.len() < 2 {
        return Err(EvalError::Error(
            "SmoothHistogram: need at least 2 data points".to_string(),
        ));
    }
    let n = nums.len() as f64;
    let mean = nums.iter().sum::<f64>() / n;
    let var = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    let sd = var.sqrt();
    let mut h = 1.06 * sd * n.powf(-0.2);
    if h <= 0.0 {
        h = 1.0;
    }
    let data_min = nums.iter().cloned().fold(f64::INFINITY, f64::min);
    let data_max = nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let x_lo = data_min - 3.0 * h;
    let x_hi = data_max + 3.0 * h;

    let steps = 200usize;
    let mut y_max = 0.0_f64;
    let mut pts: Vec<(f64, f64)> = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let x = x_lo + (x_hi - x_lo) * i as f64 / steps as f64;
        let mut d = 0.0;
        for &xi in &nums {
            let z = (x - xi) / h;
            d += (-0.5 * z * z).exp();
        }
        d /= n * h * (2.0 * std::f64::consts::PI).sqrt();
        y_max = y_max.max(d);
        pts.push((x, d));
    }
    if y_max <= 0.0 {
        y_max = 1.0;
    }

    const W: f64 = 400.0;
    const H: f64 = 300.0;
    const ML: f64 = 40.0;
    const MR: f64 = 10.0;
    const MT: f64 = 10.0;
    const MB: f64 = 30.0;
    let sx = |x: f64| ML + (x - x_lo) / (x_hi - x_lo) * (W - ML - MR);
    let sy = |y: f64| MT + (1.0 - y / y_max) * (H - MT - MB);

    let mut poly = String::new();
    for (x, y) in &pts {
        if !poly.is_empty() {
            poly.push(' ');
        }
        poly.push_str(&format!("{:.2},{:.2}", sx(*x), sy(*y)));
    }
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{W}\" height=\"{H}\" viewBox=\"0 0 {W} {H}\" style=\"background:white\">\n\
         <polyline points=\"{poly}\" fill=\"none\" stroke=\"steelblue\" stroke-width=\"2\"/>\n\
         <line x1=\"{ML}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#333\" stroke-width=\"1.5\"/>\n\
         </svg>",
        H - MB,
        W - MR,
        H - MB
    );
    Ok(Value::Str(svg))
}

// ── FindDistribution / EstimatedDistribution ────────────────────────────────

fn make_normal(mu: f64, sigma: f64) -> Value {
    let mut m = HashMap::new();
    m.insert("Distribution".to_string(), Value::Str("Normal".to_string()));
    m.insert("Mean".to_string(), super::real(mu));
    m.insert("SD".to_string(), super::real(sigma.max(1e-12)));
    Value::Assoc(m)
}

/// Fit a normal distribution to `nums`: sample mean and (MLE) standard deviation.
fn fit_normal(nums: &[f64]) -> Value {
    let n = nums.len() as f64;
    let mean = nums.iter().sum::<f64>() / n;
    let var = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    make_normal(mean, var.sqrt())
}

/// An exponential distribution object with rate `lambda`.
fn make_exponential(lambda: f64) -> Value {
    let mut m = HashMap::new();
    m.insert(
        "Distribution".to_string(),
        Value::Str("Exponential".to_string()),
    );
    m.insert("Lambda".to_string(), super::real(lambda.max(1e-12)));
    Value::Assoc(m)
}

/// Fit an exponential distribution by maximum likelihood (`λ = 1/mean`).
fn fit_exponential(nums: &[f64]) -> Option<Value> {
    if nums.iter().any(|&x| x <= 0.0) {
        return None;
    }
    let mean = nums.iter().sum::<f64>() / nums.len() as f64;
    (mean > 0.0).then(|| make_exponential(1.0 / mean))
}

/// `FindDistribution[data]` — a best-fit distribution.
///
/// Picks an exponential fit for strictly positive, markedly right-skewed data
/// and a normal fit otherwise.
pub fn builtin_find_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return unevaluated("FindDistribution", args);
    }
    let items = data_list("FindDistribution", args, 0)?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "FindDistribution: list must not be empty".to_string(),
        ));
    }
    let nums = extract_nums(items)?;
    if nums.len() >= 3 {
        let n = nums.len() as f64;
        let mean = nums.iter().sum::<f64>() / n;
        let m2 = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
        let m3 = nums.iter().map(|x| (x - mean).powi(3)).sum::<f64>() / n;
        let skew = if m2 > 0.0 { m3 / m2.powf(1.5) } else { 0.0 };
        if skew > 0.8
            && let Some(exp) = fit_exponential(&nums)
        {
            return Ok(exp);
        }
    }
    Ok(fit_normal(&nums))
}

/// `EstimatedDistribution[data, dist]` — fit the parameters of a distribution.
pub fn builtin_estimated_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("EstimatedDistribution", args);
    }
    let template = match &args[1] {
        Value::Assoc(m) if m.contains_key("Distribution") => m,
        _ => return unevaluated("EstimatedDistribution", args),
    };
    let items = data_list("EstimatedDistribution", args, 0)?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "EstimatedDistribution: list must not be empty".to_string(),
        ));
    }
    let nums = extract_nums(items)?;
    match dist_type(template) {
        Some("Normal") => Ok(fit_normal(&nums)),
        Some("Exponential") => match fit_exponential(&nums) {
            Some(v) => Ok(v),
            None => unevaluated("EstimatedDistribution", args),
        },
        _ => Ok(fit_normal(&nums)),
    }
}

// ── Survival, hazard, and inverse survival functions ────────────────────────

/// Survival function P(X > x) = 1 − CDF(x).
fn survival_f(dist: &HashMap<String, Value>, x: f64) -> Option<f64> {
    Some((1.0 - cdf_at(dist, x)?).clamp(0.0, 1.0))
}

/// SurvivalFunction[dist, x] — probability that a variate exceeds `x`.
pub fn builtin_survival_function(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("SurvivalFunction", args);
    }
    let dist = match &args[0] {
        Value::Assoc(m) if m.contains_key("Distribution") => m,
        _ => return unevaluated("SurvivalFunction", args),
    };
    let x = match as_f64(&args[1]) {
        Some(x) => x,
        None => return unevaluated("SurvivalFunction", args),
    };
    match survival_f(dist, x) {
        Some(v) => Ok(num(v)),
        None => unevaluated("SurvivalFunction", args),
    }
}

/// HazardFunction[dist, x] — PDF(x) / SurvivalFunction(x).
pub fn builtin_hazard_function(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("HazardFunction", args);
    }
    let dist = match &args[0] {
        Value::Assoc(m) if m.contains_key("Distribution") => m,
        _ => return unevaluated("HazardFunction", args),
    };
    let x = match as_f64(&args[1]) {
        Some(x) => x,
        None => return unevaluated("HazardFunction", args),
    };
    let (Some(pdf), Some(sf)) = (pmf_at(dist, x), survival_f(dist, x)) else {
        return unevaluated("HazardFunction", args);
    };
    if sf <= 0.0 {
        Ok(super::real(f64::INFINITY))
    } else {
        Ok(num(pdf / sf))
    }
}

/// InverseSurvivalFunction[dist, q] — x with SurvivalFunction(x) = q.
pub fn builtin_inverse_survival_function(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("InverseSurvivalFunction", args);
    }
    let dist = match &args[0] {
        Value::Assoc(m) if m.contains_key("Distribution") => m,
        _ => return unevaluated("InverseSurvivalFunction", args),
    };
    let q = match as_f64(&args[1]) {
        Some(q) if (0.0..=1.0).contains(&q) => q,
        _ => return unevaluated("InverseSurvivalFunction", args),
    };
    match inverse_cdf_f(dist, 1.0 - q) {
        Some(v) => Ok(num(v)),
        None => unevaluated("InverseSurvivalFunction", args),
    }
}

// ── ConditionalProbability / ExpectedValue ──────────────────────────────────

/// Evaluate a possibly held (`HoldAll`) argument with the environment.
fn eval_arg(v: &Value, env: &Env) -> Value {
    match v {
        Value::Pattern(e) => crate::eval::eval(e, env).unwrap_or_else(|_| expr_to_value(e)),
        other => other.clone(),
    }
}

/// ConditionalProbability[e1, e2, dist] — P(e1 ∧ e2) / P(e2).
pub fn builtin_conditional_probability(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return unevaluated("ConditionalProbability", args);
    }
    let e1 = held_value(&args[0]);
    let e2 = held_value(&args[1]);
    let dist = eval_arg(&args[2], env);
    match &dist {
        Value::Assoc(m) if m.contains_key("Distribution") => {}
        _ => return unevaluated("ConditionalProbability", args),
    }
    let joint = Value::Call {
        head: "And".to_string(),
        args: vec![e1.clone(), e2.clone()],
    };
    let p_joint = match builtin_probability(&[joint, dist.clone()]) {
        Ok(v) => match as_f64(&v) {
            Some(p) => p,
            None => return unevaluated("ConditionalProbability", args),
        },
        Err(_) => return unevaluated("ConditionalProbability", args),
    };
    let p_cond = match builtin_probability(&[e2, dist]) {
        Ok(v) => match as_f64(&v) {
            Some(p) => p,
            None => return unevaluated("ConditionalProbability", args),
        },
        Err(_) => return unevaluated("ConditionalProbability", args),
    };
    if p_cond <= 0.0 {
        return unevaluated("ConditionalProbability", args);
    }
    Ok(num(p_joint / p_cond))
}

/// ExpectedValue[expr, dist] — alias for Expectation.
pub fn builtin_expected_value(args: &[Value]) -> Result<Value, EvalError> {
    builtin_expectation(args)
}

// ── Commonest ───────────────────────────────────────────────────────────────

/// Commonest[list] / Commonest[list, n] — most frequently occurring elements.
pub fn builtin_commonest(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return unevaluated("Commonest", args);
    }
    let items = data_list("Commonest", args, 0)?;
    if items.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let take = if args.len() == 2 {
        match &args[1] {
            Value::Integer(n) => n.to_i64().unwrap_or(0).max(0) as usize,
            Value::Real(r) => r.to_f64().max(0.0) as usize,
            _ => return unevaluated("Commonest", args),
        }
    } else {
        usize::MAX
    };
    let mut counts: Vec<(Value, usize)> = Vec::new();
    'outer: for item in items {
        for (val, c) in counts.iter_mut() {
            if val == item {
                *c += 1;
                continue 'outer;
            }
        }
        counts.push((item.clone(), 1));
    }
    counts.sort_by_key(|(_, c)| std::cmp::Reverse(*c)); // stable: ties keep first-occurrence order
    let max_count = counts.first().map(|(_, c)| *c).unwrap_or(0);
    let iter = counts.into_iter();
    let out: Vec<Value> = if take == usize::MAX {
        iter.filter(|(_, c)| *c == max_count)
            .map(|(v, _)| v)
            .collect()
    } else {
        iter.take(take).map(|(v, _)| v).collect()
    };
    Ok(Value::List(out))
}

// ── Rank correlations ───────────────────────────────────────────────────────

/// Average ranks (1-based) of `nums`, ties sharing the mean rank.
fn ranks_of(nums: &[f64]) -> Vec<f64> {
    let n = nums.len();
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| {
        nums[a]
            .partial_cmp(&nums[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut ranks = vec![0.0; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && nums[idx[j + 1]] == nums[idx[i]] {
            j += 1;
        }
        let avg = (i + j) as f64 / 2.0 + 1.0;
        for k in i..=j {
            ranks[idx[k]] = avg;
        }
        i = j + 1;
    }
    ranks
}

fn has_ties(sorted: &[f64]) -> bool {
    sorted.windows(2).any(|w| w[0] == w[1])
}

/// SpearmanRho[list1, list2] — Spearman's rank correlation coefficient.
pub fn builtin_spearman_rho(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("SpearmanRho", args);
    }
    let a = extract_nums(data_list("SpearmanRho", args, 0)?)?;
    let b = extract_nums(data_list("SpearmanRho", args, 1)?)?;
    if a.len() != b.len() || a.len() < 2 {
        return unevaluated("SpearmanRho", args);
    }
    let mut sa = a.clone();
    sa.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let mut sb = b.clone();
    sb.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let ra = ranks_of(&a);
    let rb = ranks_of(&b);
    if !has_ties(&sa) && !has_ties(&sb) {
        // Closed form 1 − 6·Σd² / (n(n²−1)), exact for tied-free input.
        let s: f64 = ra.iter().zip(&rb).map(|(x, y)| (x - y) * (x - y)).sum();
        let nn = a.len() as i64;
        let den = nn * (nn * nn - 1);
        let numer = den - 6 * s.round() as i64;
        return Ok(rational_value(Integer::from(numer), Integer::from(den)));
    }
    Ok(super::real(pearson(&ra, &rb)))
}

/// Pearson correlation of two equal-length samples.
fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let ma = a.iter().sum::<f64>() / n;
    let mb = b.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut da = 0.0;
    let mut db = 0.0;
    for (x, y) in a.iter().zip(b.iter()) {
        num += (x - ma) * (y - mb);
        da += (x - ma) * (x - ma);
        db += (y - mb) * (y - mb);
    }
    if da <= 0.0 || db <= 0.0 {
        return 0.0;
    }
    num / (da * db).sqrt()
}

/// KendallTau[list1, list2] — Kendall's rank correlation (tau-b).
pub fn builtin_kendall_tau(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("KendallTau", args);
    }
    let a = extract_nums(data_list("KendallTau", args, 0)?)?;
    let b = extract_nums(data_list("KendallTau", args, 1)?)?;
    let n = a.len();
    if b.len() != n || n < 2 {
        return unevaluated("KendallTau", args);
    }
    let (mut conc, mut disc, mut tie_x, mut tie_y) = (0i64, 0i64, 0i64, 0i64);
    for i in 0..n {
        for j in (i + 1)..n {
            let dx = a[i] - a[j];
            let dy = b[i] - b[j];
            if dx == 0.0 {
                if dy != 0.0 {
                    tie_x += 1;
                }
            } else if dy == 0.0 {
                tie_y += 1;
            } else if (dx > 0.0) == (dy > 0.0) {
                conc += 1;
            } else {
                disc += 1;
            }
        }
    }
    if tie_x == 0 && tie_y == 0 {
        let nn = n as i64;
        let den = nn * (nn - 1);
        return Ok(rational_value(
            Integer::from(2 * (conc - disc)),
            Integer::from(den),
        ));
    }
    let denom = (((conc + disc + tie_x) as f64) * ((conc + disc + tie_y) as f64)).sqrt();
    if denom <= 0.0 {
        return Ok(super::real(0.0));
    }
    Ok(super::real((conc - disc) as f64 / denom))
}

// ── FindDistributionParameters ──────────────────────────────────────────────

fn rule(lhs: &str, rhs: Value) -> Value {
    Value::Rule {
        lhs: Box::new(Value::Symbol(lhs.to_string())),
        rhs: Box::new(rhs),
        delayed: false,
    }
}

/// FindDistributionParameters[data, dist] — maximum-likelihood parameters.
///
/// Only `Normal` is supported: returns `{Mean -> μ, SD -> σ}` with the maximum
/// likelihood estimates (sample mean and `÷n`-normalised standard deviation).
pub fn builtin_find_distribution_parameters(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("FindDistributionParameters", args);
    }
    let template = match &args[1] {
        Value::Assoc(m) if m.contains_key("Distribution") => m,
        _ => return unevaluated("FindDistributionParameters", args),
    };
    if dist_type(template) != Some("Normal") {
        return unevaluated("FindDistributionParameters", args);
    }
    let nums = extract_nums(data_list("FindDistributionParameters", args, 0)?)?;
    if nums.is_empty() {
        return Err(EvalError::Error(
            "FindDistributionParameters: list must not be empty".to_string(),
        ));
    }
    let n = nums.len() as f64;
    let mean = nums.iter().sum::<f64>() / n;
    let var = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    let sd = var.sqrt().max(1e-12);
    Ok(Value::List(vec![
        rule("Mean", super::real(mean)),
        rule("SD", super::real(sd)),
    ]))
}

// ── Goodness-of-fit tests ───────────────────────────────────────────────────

/// CDF of a chi-square distribution with `df` degrees of freedom.
fn chi_square_cdf(df: f64, x: f64) -> f64 {
    let mut m = HashMap::new();
    m.insert(
        "Distribution".to_string(),
        Value::Str("ChiSquare".to_string()),
    );
    m.insert("K".to_string(), super::real(df));
    match st::builtin_cdf(&[Value::Assoc(m), super::real(x)]) {
        Ok(v) => as_f64(&v).unwrap_or(0.0),
        Err(_) => 0.0,
    }
}

/// Asymptotic Kolmogorov–Smirnov survival function Q(λ).
fn ks_pvalue(d: f64, n: f64) -> f64 {
    if d <= 0.0 {
        return 1.0;
    }
    let lambda = (n.sqrt() + 0.12 + 0.11 / n.sqrt()) * d;
    let p = if lambda < 0.755 {
        // Series converges quickly for small λ.
        let mut sum = 0.0;
        let mut k = 1u32;
        loop {
            let t = (-((2 * k - 1) as f64).powi(2) * std::f64::consts::PI.powi(2)
                / (8.0 * lambda * lambda))
                .exp();
            sum += t;
            if t < 1e-14 || k > 100 {
                break;
            }
            k += 1;
        }
        1.0 - (2.0 * std::f64::consts::PI).sqrt() / lambda * sum
    } else {
        let mut sum = 0.0;
        let mut sign = 1.0;
        for j in 1..=100u32 {
            let t = (-2.0 * (j as f64).powi(2) * lambda * lambda).exp();
            sum += sign * t;
            sign = -sign;
            if t < 1e-14 {
                break;
            }
        }
        2.0 * sum
    };
    p.clamp(0.0, 1.0)
}

/// Anderson–Darling p-value (D'Agostino & Stephens 1986 approximation).
fn ad_pvalue(a2: f64, n: f64) -> f64 {
    let astar = a2 * (1.0 + 0.75 / n + 2.25 / (n * n));
    let p = if astar >= 0.6 {
        (1.2937 - 5.709 * astar + 0.0186 * astar * astar).exp()
    } else if astar > 0.34 {
        (0.9177 - 4.279 * astar - 1.38 * astar * astar).exp()
    } else {
        1.0 - (-8.318 + 42.796 * astar - 59.938 * astar * astar).exp()
    };
    p.clamp(0.0, 1.0)
}

/// Extract the distribution argument (`args[1]`) of a `<test>[data, dist]` call.
fn dist_arg(args: &[Value]) -> Option<&HashMap<String, Value>> {
    match args.get(1) {
        Some(Value::Assoc(m)) if m.contains_key("Distribution") => Some(m),
        _ => None,
    }
}

/// ChiSquareTest[data, dist] — chi-square goodness-of-fit p-value.
///
/// Data are binned into `⌈√n⌉` equiprobable bins under `dist`; the statistic
/// `Σ (O−E)²/E` is referred to a chi-square distribution with `bins − 1` df.
pub fn builtin_chi_square_test(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("ChiSquareTest", args);
    }
    let Some(dist) = dist_arg(args) else {
        return unevaluated("ChiSquareTest", args);
    };
    let mut nums = extract_nums(data_list("ChiSquareTest", args, 0)?)?;
    let n = nums.len();
    if n < 2 {
        return unevaluated("ChiSquareTest", args);
    }
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let k = (n as f64).sqrt().ceil() as usize;
    let k = k.clamp(2, n);
    let mut edges = Vec::with_capacity(k + 1);
    for i in 0..=k {
        match inverse_cdf_f(dist, i as f64 / k as f64) {
            Some(v) => edges.push(v),
            None => return unevaluated("ChiSquareTest", args),
        }
    }
    let mut obs = vec![0.0_f64; k];
    for &x in &nums {
        let idx = edges
            .partition_point(|&e| e < x)
            .saturating_sub(1)
            .min(k - 1);
        obs[idx] += 1.0;
    }
    let exp = n as f64 / k as f64;
    let chi2: f64 = obs.iter().map(|o| (o - exp) * (o - exp) / exp).sum();
    let p = 1.0 - chi_square_cdf((k - 1) as f64, chi2);
    Ok(num(p.clamp(0.0, 1.0)))
}

/// KolmogorovSmirnovTest[data, dist] — KS goodness-of-fit p-value.
pub fn builtin_kolmogorov_smirnov_test(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("KolmogorovSmirnovTest", args);
    }
    let Some(dist) = dist_arg(args) else {
        return unevaluated("KolmogorovSmirnovTest", args);
    };
    let mut nums = extract_nums(data_list("KolmogorovSmirnovTest", args, 0)?)?;
    let n = nums.len();
    if n < 2 {
        return unevaluated("KolmogorovSmirnovTest", args);
    }
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let nf = n as f64;
    let mut d = 0.0_f64;
    for (i, &x) in nums.iter().enumerate() {
        let f = match cdf_at(dist, x) {
            Some(f) => f,
            None => return unevaluated("KolmogorovSmirnovTest", args),
        };
        let hi = (i + 1) as f64 / nf - f;
        let lo = f - i as f64 / nf;
        d = d.max(hi).max(lo);
    }
    Ok(num(ks_pvalue(d, nf)))
}

/// AndersonDarlingTest[data, dist] — Anderson–Darling goodness-of-fit p-value.
pub fn builtin_anderson_darling_test(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("AndersonDarlingTest", args);
    }
    let Some(dist) = dist_arg(args) else {
        return unevaluated("AndersonDarlingTest", args);
    };
    let mut nums = extract_nums(data_list("AndersonDarlingTest", args, 0)?)?;
    let n = nums.len();
    if n < 2 {
        return unevaluated("AndersonDarlingTest", args);
    }
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let nf = n as f64;
    let eps = 1e-12;
    let mut s = 0.0_f64;
    for i in 0..n {
        let zi = match cdf_at(dist, nums[i]) {
            Some(z) => z.clamp(eps, 1.0 - eps),
            None => return unevaluated("AndersonDarlingTest", args),
        };
        let zj = match cdf_at(dist, nums[n - 1 - i]) {
            Some(z) => z.clamp(eps, 1.0 - eps),
            None => return unevaluated("AndersonDarlingTest", args),
        };
        s += (2.0 * i as f64 + 1.0) * (zi.ln() + (1.0 - zj).ln());
    }
    let a2 = -nf - s / nf;
    Ok(num(ad_pvalue(a2, nf)))
}

/// DistributionFitTest[data, dist] — goodness-of-fit p-value (Anderson–Darling).
pub fn builtin_distribution_fit_test(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        2 => builtin_anderson_darling_test(args),
        3 => {
            let name = match &args[2] {
                Value::Str(s) => s.clone(),
                _ => return unevaluated("DistributionFitTest", args),
            };
            let which = if name.contains("Kolmogorov") || name.contains("KS") {
                builtin_kolmogorov_smirnov_test
            } else if name.contains("ChiSquare") {
                builtin_chi_square_test
            } else {
                builtin_anderson_darling_test
            };
            which(&args[..2])
        }
        _ => unevaluated("DistributionFitTest", args),
    }
}

// ── Hypothesis tests: location and variance ─────────────────────────────────

/// Standard-normal upper-tail probability `P(Z > z)`.
fn normal_sf(z: f64) -> f64 {
    let mut m = HashMap::new();
    m.insert("Distribution".to_string(), Value::Str("Normal".to_string()));
    m.insert("Mean".to_string(), super::real(0.0));
    m.insert("SD".to_string(), super::real(1.0));
    match st::builtin_cdf(&[Value::Assoc(m), super::real(z)]) {
        Ok(v) => (1.0 - as_f64(&v).unwrap_or(0.5)).clamp(0.0, 1.0),
        Err(_) => 0.5,
    }
}

/// Student's-`t` upper-tail probability `P(T_ν > t)`.
fn student_t_sf(nu: f64, t: f64) -> Option<f64> {
    let mut m = HashMap::new();
    m.insert(
        "Distribution".to_string(),
        Value::Str("StudentT".to_string()),
    );
    m.insert("Nu".to_string(), super::real(nu));
    let c = as_f64(&st::builtin_cdf(&[Value::Assoc(m), super::real(t)]).ok()?)?;
    Some((1.0 - c).clamp(0.0, 1.0))
}

/// Chi-square upper-tail probability `P(X_k > x)`.
fn chi_square_sf(k: f64, x: f64) -> f64 {
    (1.0 - chi_square_cdf(k, x)).clamp(0.0, 1.0)
}

/// F upper-tail probability `P(F_{d1,d2} > x)`.
///
/// Uses `P(F > x) = I_{d2/(d2 + d1 x)}(d2/2, d1/2)`, i.e. a Beta CDF.
fn f_sf(d1: f64, d2: f64, x: f64) -> Option<f64> {
    if x <= 0.0 {
        return Some(1.0);
    }
    if !x.is_finite() {
        return Some(0.0);
    }
    let s = d2 / (d2 + d1 * x);
    let mut m = HashMap::new();
    m.insert("Distribution".to_string(), Value::Str("Beta".to_string()));
    m.insert("Alpha".to_string(), super::real(d2 / 2.0));
    m.insert("Beta".to_string(), super::real(d1 / 2.0));
    as_f64(&st::builtin_cdf(&[Value::Assoc(m), super::real(s)]).ok()?)
}

/// Sample mean and unbiased (n−1) variance.
fn mean_var(nums: &[f64]) -> (f64, f64) {
    let n = nums.len() as f64;
    let mean = nums.iter().sum::<f64>() / n;
    let var = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, var)
}

/// Two-sided p-value `2·min(p_lo, p_hi)`, capped at 1.
fn two_sided(p_lo: f64, p_hi: f64) -> f64 {
    (2.0 * p_lo.min(p_hi)).clamp(0.0, 1.0)
}

/// Split an optional trailing `"Test"` string off the argument list.
fn split_test_name(args: &[Value]) -> (&[Value], &str) {
    match args.last() {
        Some(Value::Str(s)) if args.len() >= 2 => (&args[..args.len() - 1], s.as_str()),
        _ => (args, ""),
    }
}

/// One-sample location test against a hypothesized center `mu`.
fn one_sample_location(name: &str, nums: &[f64], mu: f64, which: &str) -> Result<Value, EvalError> {
    let n = nums.len();
    if n < 2 {
        return unevaluated(name, &[]);
    }
    let nf = n as f64;
    let (mean, var) = mean_var(nums);
    match which {
        "" | "T" | "StudentT" => {
            let se = (var / nf).sqrt();
            let t = if se > 0.0 { (mean - mu) / se } else { 0.0 };
            match student_t_sf(nf - 1.0, t) {
                Some(sf) => Ok(super::real(two_sided(sf, 1.0 - sf))),
                None => unevaluated(name, &[]),
            }
        }
        "Z" | "Normal" => {
            let se = (var / nf).sqrt();
            if se <= 0.0 {
                return Ok(super::real(if mean == mu { 1.0 } else { 0.0 }));
            }
            let z = (mean - mu) / se;
            Ok(super::real((2.0 * normal_sf(z.abs())).clamp(0.0, 1.0)))
        }
        "SignedRank" | "Wilcoxon" => match signed_rank_p(nums, mu) {
            Some(p) => Ok(super::real(p)),
            None => unevaluated(name, &[]),
        },
        _ => unevaluated(name, &[]),
    }
}

/// Wilcoxon signed-rank two-sided p-value (normal approximation).
fn signed_rank_p(nums: &[f64], mu: f64) -> Option<f64> {
    let diffs: Vec<f64> = nums.iter().map(|x| x - mu).filter(|d| *d != 0.0).collect();
    let n = diffs.len();
    if n < 1 {
        return None;
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| {
        diffs[i]
            .abs()
            .partial_cmp(&diffs[j].abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut ranks = vec![0.0_f64; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && diffs[order[j + 1]].abs() == diffs[order[i]].abs() {
            j += 1;
        }
        let avg = (i + j) as f64 / 2.0 + 1.0;
        for k in i..=j {
            ranks[order[k]] = avg;
        }
        i = j + 1;
    }
    let w_plus: f64 = (0..n).filter(|&k| diffs[k] > 0.0).map(|k| ranks[k]).sum();
    let nf = n as f64;
    let mean = nf * (nf + 1.0) / 4.0;
    let var = nf * (nf + 1.0) * (2.0 * nf + 1.0) / 24.0;
    if var <= 0.0 {
        return Some(1.0);
    }
    let diff = w_plus - mean;
    let z = (diff - 0.5 * diff.signum()) / var.sqrt();
    Some((2.0 * normal_sf(z.abs())).clamp(0.0, 1.0))
}

/// Two-sample location test (Welch's t) between two numeric samples.
fn two_sample_location(name: &str, a: &[f64], b: &[f64], which: &str) -> Result<Value, EvalError> {
    let (n1, n2) = (a.len(), b.len());
    if n1 < 2 || n2 < 2 {
        return unevaluated(name, &[]);
    }
    let (f1, f2) = (n1 as f64, n2 as f64);
    let (m1, v1) = mean_var(a);
    let (m2, v2) = mean_var(b);
    let se2 = v1 / f1 + v2 / f2;
    if se2 <= 0.0 {
        return Ok(super::real(if m1 == m2 { 1.0 } else { 0.0 }));
    }
    match which {
        "Z" | "Normal" => {
            let z = (m1 - m2) / se2.sqrt();
            Ok(super::real((2.0 * normal_sf(z.abs())).clamp(0.0, 1.0)))
        }
        "" | "T" | "StudentT" => {
            let t = (m1 - m2) / se2.sqrt();
            // Welch–Satterthwaite degrees of freedom.
            let df = se2 * se2 / ((v1 / f1).powi(2) / (f1 - 1.0) + (v2 / f2).powi(2) / (f2 - 1.0));
            match student_t_sf(df, t) {
                Some(sf) => Ok(super::real(two_sided(sf, 1.0 - sf))),
                None => unevaluated(name, &[]),
            }
        }
        _ => unevaluated(name, &[]),
    }
}

/// Shared argument parsing/dispatch for `MeanTest` and `LocationTest`.
fn location_test_impl(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    let (pos, named) = split_test_name(args);
    let which = named.to_string();
    match pos.len() {
        // `test[{a, b}, ...]` — two-sample list form.
        1 => {
            if let Value::List(outer) = &pos[0]
                && outer.len() == 2
                && let (Value::List(a), Value::List(b)) = (&outer[0], &outer[1])
            {
                let a = extract_nums(a)?;
                let b = extract_nums(b)?;
                return two_sample_location(name, &a, &b, &which);
            }
            unevaluated(name, args)
        }
        // `test[data, mu]` or `test[data1, data2]`.
        2 => {
            let a = match &pos[0] {
                Value::List(items) => extract_nums(items)?,
                _ => return unevaluated(name, args),
            };
            if let Value::List(b) = &pos[1] {
                let b = extract_nums(b)?;
                return two_sample_location(name, &a, &b, &which);
            }
            match as_f64(&pos[1]) {
                Some(mu) => one_sample_location(name, &a, mu, &which),
                None => unevaluated(name, args),
            }
        }
        _ => unevaluated(name, args),
    }
}

/// MeanTest[data, mu] — p-value for the population mean.
///
/// Also supports the two-sample forms `MeanTest[{a, b}]` and `MeanTest[a, b]`
/// (Welch's unequal-variance t-test) and an optional trailing test name
/// (`"T"`, `"Z"`, `"SignedRank"`).
pub fn builtin_mean_test(args: &[Value]) -> Result<Value, EvalError> {
    location_test_impl("MeanTest", args)
}

/// LocationTest[data, mu] — p-value for the population location.
///
/// Same argument forms and tests as [`builtin_mean_test`].
pub fn builtin_location_test(args: &[Value]) -> Result<Value, EvalError> {
    location_test_impl("LocationTest", args)
}

/// VarianceTest[data, var] — p-value for the population variance.
///
/// One-sample chi-square form `VarianceTest[data, σ0]`; two-sample forms
/// `VarianceTest[{a, b}]` / `VarianceTest[a, b]` (F-test on the variance
/// ratio). An optional trailing test name (`"ChiSquare"`, `"F"`) selects the
/// statistic.
pub fn builtin_variance_test(args: &[Value]) -> Result<Value, EvalError> {
    let (pos, named) = split_test_name(args);
    let which = named.to_string();
    match pos.len() {
        1 => {
            if let Value::List(outer) = &pos[0]
                && outer.len() == 2
                && let (Value::List(a), Value::List(b)) = (&outer[0], &outer[1])
            {
                let a = extract_nums(a)?;
                let b = extract_nums(b)?;
                return variance_two_sample(&a, &b, &which);
            }
            unevaluated("VarianceTest", args)
        }
        2 => {
            let a = match &pos[0] {
                Value::List(items) => extract_nums(items)?,
                _ => return unevaluated("VarianceTest", args),
            };
            if let Value::List(b) = &pos[1] {
                let b = extract_nums(b)?;
                return variance_two_sample(&a, &b, &which);
            }
            let var0 = match as_f64(&pos[1]) {
                Some(v) if v > 0.0 => v,
                _ => return unevaluated("VarianceTest", args),
            };
            variance_one_sample(&a, var0, &which)
        }
        _ => unevaluated("VarianceTest", args),
    }
}

/// One-sample chi-square variance test against `var0`.
fn variance_one_sample(nums: &[f64], var0: f64, which: &str) -> Result<Value, EvalError> {
    let n = nums.len();
    if n < 2 {
        return unevaluated("VarianceTest", &[]);
    }
    if !matches!(which, "" | "ChiSquare" | "ChiSquared" | "Variance") {
        return unevaluated("VarianceTest", &[]);
    }
    let (_, var) = mean_var(nums);
    let df = (n - 1) as f64;
    let stat = df * var / var0;
    let lo = chi_square_cdf(df, stat);
    let hi = chi_square_sf(df, stat);
    Ok(super::real(two_sided(lo, hi)))
}

/// Two-sample F variance-ratio test.
fn variance_two_sample(a: &[f64], b: &[f64], which: &str) -> Result<Value, EvalError> {
    if a.len() < 2 || b.len() < 2 {
        return unevaluated("VarianceTest", &[]);
    }
    if !matches!(which, "" | "F" | "FRatio" | "Variance") {
        return unevaluated("VarianceTest", &[]);
    }
    let d1 = (a.len() - 1) as f64;
    let d2 = (b.len() - 1) as f64;
    let (_, v1) = mean_var(a);
    let (_, v2) = mean_var(b);
    if v1 <= 0.0 || v2 <= 0.0 {
        return Ok(super::real(1.0));
    }
    let f = v1 / v2;
    let hi = match f_sf(d1, d2, f) {
        Some(p) => p,
        None => return unevaluated("VarianceTest", &[]),
    };
    let lo = 1.0 - hi;
    Ok(super::real(two_sided(lo, hi)))
}

// ── Quantile interpolation methods ──────────────────────────────────────────

/// Round half to even (banker's rounding), used by the Hyndman–Fan type-3
/// estimator.
fn round_half_even(x: f64) -> i64 {
    let f = x.floor();
    let frac = x - f;
    if (frac - 0.5).abs() < 1e-12 {
        let fi = f as i64;
        if fi % 2 == 0 { fi } else { fi + 1 }
    } else {
        (f + if frac > 0.5 { 1.0 } else { 0.0 }) as i64
    }
}

/// Hyndman–Fan quantile estimator of type `method` (1..=9) on sorted `data`.
///
/// The eight continuous estimators (types 4–9) share the form
/// `x_{⌊h⌋} + γ·(x_{⌈h⌉} − x_{⌊h⌋})` with `γ = h − ⌊h⌋`; types 1–3 are the
/// discontinuous inverse-ECDF variants. `h` is clamped to `[1, n]`.
fn quantile_hf(data: &[f64], q: f64, method: u32) -> Option<f64> {
    let n = data.len();
    if n == 0 {
        return None;
    }
    let nf = n as f64;
    let clamp = |h: f64| h.clamp(1.0, nf);
    match method {
        1 => {
            let h = clamp(nf * q + 0.5);
            Some(data[(h.ceil() as usize).clamp(1, n) - 1])
        }
        2 => {
            let h = clamp(nf * q + 0.5);
            let lo = (h.floor() as usize).clamp(1, n);
            let hi = (h.ceil() as usize).clamp(1, n);
            Some(0.5 * (data[lo - 1] + data[hi - 1]))
        }
        3 => {
            let h = clamp(nf * q - 0.5);
            let r = round_half_even(h).clamp(1, n as i64);
            Some(data[r as usize - 1])
        }
        4..=9 => {
            let h_raw = match method {
                4 => nf * q,
                5 => nf * q + 0.5,
                6 => (nf + 1.0) * q,
                7 => (nf - 1.0) * q + 1.0,
                8 => (nf + 1.0 / 3.0) * q + 1.0 / 3.0,
                _ => (nf + 0.25) * q + 0.375,
            };
            let h = clamp(h_raw);
            let fl = h.floor();
            let gamma = h - fl;
            let i = (fl as usize).clamp(1, n);
            let j = ((fl as usize) + 1).clamp(1, n);
            Some(data[i - 1] + gamma * (data[j - 1] - data[i - 1]))
        }
        _ => None,
    }
}

/// Parse a `Quantile` method argument (an integer/`Type7`-style string).
fn parse_quantile_method(v: &Value) -> Option<u32> {
    match v {
        Value::Integer(n) => {
            let m = n.to_i64()?;
            (1..=9).contains(&m).then_some(m as u32)
        }
        Value::Real(r) => {
            let f = r.to_f64();
            (f.fract() == 0.0 && (1.0..=9.0).contains(&f)).then_some(f as u32)
        }
        Value::Str(s) => match s.as_str() {
            "InverseCDF" | "Type1" => Some(1),
            "AveragedInverseCDF" | "Type2" => Some(2),
            "NearestOrder" | "Round" | "Type3" => Some(3),
            "Type4" => Some(4),
            "Type5" => Some(5),
            "Type6" => Some(6),
            "LinearInterpolation" | "InterpolatedInverseCDF" | "Spline" | "Type7" => Some(7),
            "Type8" => Some(8),
            "Type9" => Some(9),
            _ => None,
        },
        Value::List(items) if items.len() == 1 => parse_quantile_method(&items[0]),
        _ => None,
    }
}

/// `Quantile[data, q]` (default linear interpolation) with an optional third
/// argument selecting Hyndman–Fan type 1–9 (`Quantile[data, q, "Type7"]`).
pub fn builtin_quantile_method(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 2 {
        return st::builtin_quantile(args);
    }
    if args.len() != 3 {
        return unevaluated("Quantile", args);
    }
    let items = data_list("Quantile", args, 0)?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "Quantile: list must not be empty".to_string(),
        ));
    }
    let q = match as_f64(&args[1]) {
        Some(q) if (0.0..=1.0).contains(&q) => q,
        _ => return unevaluated("Quantile", args),
    };
    let method = match parse_quantile_method(&args[2]) {
        Some(m) => m,
        None => return unevaluated("Quantile", args),
    };
    let mut nums = extract_nums(items)?;
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    match quantile_hf(&nums, q, method) {
        Some(v) => Ok(num(v)),
        None => unevaluated("Quantile", args),
    }
}

// ── QuartileDeviation / TrimmedMean / WinsorizedMean ────────────────────────

/// `QuartileDeviation[list]` — half the interquartile range, `(Q3 − Q1)/2`.
pub fn builtin_quartile_deviation(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return unevaluated("QuartileDeviation", args);
    }
    let items = data_list("QuartileDeviation", args, 0)?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "QuartileDeviation: list must not be empty".to_string(),
        ));
    }
    let q1 = st::builtin_quantile(&[args[0].clone(), super::real(0.25)])?;
    let q3 = st::builtin_quantile(&[args[0].clone(), super::real(0.75)])?;
    let (a, b) = match (as_f64(&q1), as_f64(&q3)) {
        (Some(a), Some(b)) => (a, b),
        _ => return unevaluated("QuartileDeviation", args),
    };
    Ok(num((b - a) / 2.0))
}

/// Parse the trimming fractions of a `TrimmedMean`/`WinsorizedMean` call.
///
/// Accepts `f` or `{f_low, f_high}`, each in `[0, 0.5)`; returns the number of
/// elements to drop from the low and high ends of an `n`-element sample.
fn parse_trim_fractions(spec: &Value, n: usize) -> Option<(usize, usize)> {
    let (flow, fhigh) = match spec {
        Value::List(items) if items.len() == 2 => (as_f64(&items[0])?, as_f64(&items[1])?),
        other => {
            let f = as_f64(other)?;
            (f, f)
        }
    };
    if !(0.0..=0.5).contains(&flow) || !(0.0..=0.5).contains(&fhigh) {
        return None;
    }
    let l = (flow * n as f64).floor() as usize;
    let h = (fhigh * n as f64).floor() as usize;
    (l + h < n).then_some((l, h))
}

/// `TrimmedMean[list, f]` / `TrimmedMean[list, {f_low, f_high}]` — mean of the
/// sample after removing the lowest and highest fraction(s) of the data.
pub fn builtin_trimmed_mean(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("TrimmedMean", args);
    }
    let items = data_list("TrimmedMean", args, 0)?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "TrimmedMean: list must not be empty".to_string(),
        ));
    }
    let mut nums = extract_nums(items)?;
    let (lo, hi) = match parse_trim_fractions(&args[1], nums.len()) {
        Some(t) => t,
        None => return unevaluated("TrimmedMean", args),
    };
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let kept = &nums[lo..nums.len() - hi];
    let mean = kept.iter().sum::<f64>() / kept.len() as f64;
    Ok(num(mean))
}

/// `WinsorizedMean[list, f]` / `WinsorizedMean[list, {f_low, f_high}]` — mean
/// after clamping the extremes to the first retained order statistics.
pub fn builtin_winsorized_mean(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("WinsorizedMean", args);
    }
    let items = data_list("WinsorizedMean", args, 0)?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "WinsorizedMean: list must not be empty".to_string(),
        ));
    }
    let mut nums = extract_nums(items)?;
    let (lo, hi) = match parse_trim_fractions(&args[1], nums.len()) {
        Some(t) => t,
        None => return unevaluated("WinsorizedMean", args),
    };
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let low = nums[lo];
    let high = nums[nums.len() - 1 - hi];
    let len = nums.len();
    for x in nums.iter_mut().take(lo) {
        *x = low;
    }
    for x in nums.iter_mut().skip(len - hi) {
        *x = high;
    }
    let mean = nums.iter().sum::<f64>() / nums.len() as f64;
    Ok(num(mean))
}

// ── PearsonChiSquareTest ────────────────────────────────────────────────────

/// Pearson chi-square goodness-of-fit p-value, sharing the equiprobable binning
/// of [`builtin_chi_square_test`].
fn pearson_chi_square_p(nums: &[f64], dist: &HashMap<String, Value>) -> Option<f64> {
    let n = nums.len();
    if n < 2 {
        return None;
    }
    let mut sorted = nums.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let k = ((n as f64).sqrt().ceil() as usize).clamp(2, n);
    let mut edges = Vec::with_capacity(k + 1);
    for i in 0..=k {
        edges.push(inverse_cdf_f(dist, i as f64 / k as f64)?);
    }
    let mut obs = vec![0.0_f64; k];
    for &x in &sorted {
        let idx = edges
            .partition_point(|&e| e < x)
            .saturating_sub(1)
            .min(k - 1);
        obs[idx] += 1.0;
    }
    let exp = n as f64 / k as f64;
    let chi2: f64 = obs.iter().map(|o| (o - exp) * (o - exp) / exp).sum();
    Some(chi_square_sf((k - 1) as f64, chi2))
}

/// `PearsonChiSquareTest[data, dist]` — Pearson chi-square goodness-of-fit
/// p-value. The one-argument form tests normality against the fitted
/// mean/standard deviation.
pub fn builtin_pearson_chi_square_test(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        2 => {
            let Some(dist) = dist_arg(args) else {
                return unevaluated("PearsonChiSquareTest", args);
            };
            let nums = extract_nums(data_list("PearsonChiSquareTest", args, 0)?)?;
            match pearson_chi_square_p(&nums, dist) {
                Some(p) => Ok(num(p)),
                None => unevaluated("PearsonChiSquareTest", args),
            }
        }
        1 => {
            let nums = extract_nums(data_list("PearsonChiSquareTest", args, 0)?)?;
            if nums.len() < 3 {
                return unevaluated("PearsonChiSquareTest", args);
            }
            let n = nums.len() as f64;
            let mean = nums.iter().sum::<f64>() / n;
            let var = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
            let dist = make_normal(mean, var.sqrt());
            let m = match &dist {
                Value::Assoc(m) => m,
                _ => return unevaluated("PearsonChiSquareTest", args),
            };
            match pearson_chi_square_p(&nums, m) {
                Some(p) => Ok(num(p)),
                None => unevaluated("PearsonChiSquareTest", args),
            }
        }
        _ => unevaluated("PearsonChiSquareTest", args),
    }
}

// ── Non-parametric density distributions ────────────────────────────────────

/// Cumulative standard-normal CDF via the Abramowitz–Stegun `erf` bound.
fn normal_cdf(x: f64) -> f64 {
    fn erf(x: f64) -> f64 {
        let t = 1.0 / (1.0 + 0.3275911 * x.abs());
        let poly = ((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592;
        let y = 1.0 - poly * t * (-x * x).exp();
        if x >= 0.0 { y } else { -y }
    }
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

fn normal_pdf(x: f64) -> f64 {
    (-0.5 * x * x).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

/// Data payload of a non-parametric distribution object.
fn dist_data(dist: &HashMap<String, Value>) -> Option<Vec<f64>> {
    match dist.get("Data")? {
        Value::List(items) => extract_nums(items).ok(),
        _ => None,
    }
}

fn assoc(kind: &str) -> HashMap<String, Value> {
    let mut m = HashMap::new();
    m.insert("Distribution".to_string(), Value::Str(kind.to_string()));
    m
}

fn reals(nums: &[f64]) -> Value {
    Value::List(nums.iter().map(|&x| super::real(x)).collect())
}

/// Silverman's rule-of-thumb bandwidth (with a robust IQR scale).
fn silverman_bw(nums: &[f64]) -> f64 {
    let n = nums.len() as f64;
    if n < 2.0 {
        return 1.0;
    }
    let mean = nums.iter().sum::<f64>() / n;
    let sd = (nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    let mut sorted = nums.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let iqr =
        quantile_hf(&sorted, 0.75, 7).unwrap_or(sd) - quantile_hf(&sorted, 0.25, 7).unwrap_or(sd);
    let a = sd;
    let b = iqr / 1.34;
    let scale = match (a > 0.0, b > 0.0) {
        (true, true) => a.min(b),
        (true, false) => a,
        (false, true) => b,
        (false, false) => 1.0,
    };
    (1.06 * scale * n.powf(-0.2)).max(1e-12)
}

/// `SmoothKernelDistribution[data]` / `[data, bw]` — Gaussian KDE object.
pub fn builtin_smooth_kernel_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return unevaluated("SmoothKernelDistribution", args);
    }
    let nums = extract_nums(data_list("SmoothKernelDistribution", args, 0)?)?;
    if nums.len() < 2 {
        return unevaluated("SmoothKernelDistribution", args);
    }
    let bw = if args.len() == 2 {
        match as_f64(&args[1]) {
            Some(h) if h > 0.0 => h,
            _ => return unevaluated("SmoothKernelDistribution", args),
        }
    } else {
        silverman_bw(&nums)
    };
    let mut m = assoc("SmoothKernel");
    m.insert("Data".to_string(), reals(&nums));
    m.insert("Bandwidth".to_string(), super::real(bw));
    Ok(Value::Assoc(m))
}

/// `KernelMixtureDistribution[data]` / `[data, bw]` — Gaussian kernel mixture.
pub fn builtin_kernel_mixture_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return unevaluated("KernelMixtureDistribution", args);
    }
    let nums = extract_nums(data_list("KernelMixtureDistribution", args, 0)?)?;
    if nums.len() < 2 {
        return unevaluated("KernelMixtureDistribution", args);
    }
    let bw = if args.len() == 2 {
        match as_f64(&args[1]) {
            Some(h) if h > 0.0 => h,
            _ => return unevaluated("KernelMixtureDistribution", args),
        }
    } else {
        silverman_bw(&nums)
    };
    let mut m = assoc("KernelMixture");
    m.insert("Data".to_string(), reals(&nums));
    m.insert("Bandwidth".to_string(), super::real(bw));
    Ok(Value::Assoc(m))
}

/// `EmpiricalDistribution[data]` — discrete distribution with mass `1/n` at
/// each data point.
pub fn builtin_empirical_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return unevaluated("EmpiricalDistribution", args);
    }
    let nums = extract_nums(data_list("EmpiricalDistribution", args, 0)?)?;
    if nums.is_empty() {
        return Err(EvalError::Error(
            "EmpiricalDistribution: list must not be empty".to_string(),
        ));
    }
    let mut m = assoc("Empirical");
    m.insert("Data".to_string(), reals(&nums));
    Ok(Value::Assoc(m))
}

/// Bin edges and per-bin densities of a histogram distribution payload.
fn histogram_payload(dist: &HashMap<String, Value>) -> Option<(Vec<f64>, Vec<f64>)> {
    let edges = match dist.get("Edges")? {
        Value::List(items) => extract_nums(items).ok()?,
        _ => return None,
    };
    let heights = match dist.get("Heights")? {
        Value::List(items) => extract_nums(items).ok()?,
        _ => return None,
    };
    (edges.len() == heights.len() + 1).then_some((edges, heights))
}

/// `HistogramDistribution[data]` / `[data, n]` — equal-width binned density.
pub fn builtin_histogram_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return unevaluated("HistogramDistribution", args);
    }
    let nums = extract_nums(data_list("HistogramDistribution", args, 0)?)?;
    if nums.len() < 2 {
        return unevaluated("HistogramDistribution", args);
    }
    let k = if args.len() == 2 {
        let spec = match &args[1] {
            Value::Integer(n) => n.to_i64().filter(|k| *k >= 1).map(|k| k as usize),
            Value::Real(r) => {
                let f = r.to_f64();
                (f.fract() == 0.0 && f >= 1.0).then_some(f as usize)
            }
            _ => None,
        };
        match spec {
            Some(k) => k,
            None => return unevaluated("HistogramDistribution", args),
        }
    } else {
        (nums.len() as f64).sqrt().ceil() as usize
    };
    let k = k.clamp(1, nums.len());
    let lo = nums.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let width = if hi > lo { (hi - lo) / k as f64 } else { 1.0 };
    let edges: Vec<f64> = (0..=k).map(|i| lo + width * i as f64).collect();
    let n = nums.len() as f64;
    let mut heights = vec![0.0_f64; k];
    for &x in &nums {
        let idx = ((x - lo) / width).floor() as usize;
        heights[idx.min(k - 1)] += 1.0;
    }
    for h in heights.iter_mut() {
        *h /= n * width;
    }
    let mut m = assoc("Histogram");
    m.insert("Edges".to_string(), reals(&edges));
    m.insert("Heights".to_string(), reals(&heights));
    Ok(Value::Assoc(m))
}

/// Density of a non-parametric distribution payload at `x`.
fn npar_pdf(dist: &HashMap<String, Value>, x: f64) -> Option<f64> {
    match dist_type(dist)? {
        "Empirical" => {
            let d = dist_data(dist)?;
            let n = d.len() as f64;
            let c = d.iter().filter(|&&v| v == x).count() as f64;
            Some(c / n)
        }
        "SmoothKernel" | "KernelMixture" => {
            let d = dist_data(dist)?;
            let h = dist.get("Bandwidth").and_then(as_f64)?;
            if h <= 0.0 {
                return None;
            }
            let s: f64 = d.iter().map(|&xi| normal_pdf((x - xi) / h)).sum();
            Some(s / (d.len() as f64 * h))
        }
        "Histogram" => {
            let (edges, heights) = histogram_payload(dist)?;
            if x < edges[0] || x > *edges.last()? {
                return Some(0.0);
            }
            let idx = edges
                .partition_point(|&e| e <= x)
                .saturating_sub(1)
                .min(heights.len() - 1);
            Some(heights[idx])
        }
        _ => None,
    }
}

/// CDF of a non-parametric distribution payload at `x`.
fn npar_cdf(dist: &HashMap<String, Value>, x: f64) -> Option<f64> {
    match dist_type(dist)? {
        "Empirical" => {
            let d = dist_data(dist)?;
            let n = d.len() as f64;
            let c = d.iter().filter(|&&v| v <= x).count() as f64;
            Some((c / n).clamp(0.0, 1.0))
        }
        "SmoothKernel" | "KernelMixture" => {
            let d = dist_data(dist)?;
            let h = dist.get("Bandwidth").and_then(as_f64)?;
            if h <= 0.0 {
                return None;
            }
            let s: f64 = d.iter().map(|&xi| normal_cdf((x - xi) / h)).sum();
            Some((s / d.len() as f64).clamp(0.0, 1.0))
        }
        "Histogram" => {
            let (edges, heights) = histogram_payload(dist)?;
            if x <= edges[0] {
                return Some(0.0);
            }
            if x >= *edges.last()? {
                return Some(1.0);
            }
            let mut acc = 0.0;
            for b in 0..heights.len() {
                let (a, c) = (edges[b], edges[b + 1]);
                if x >= c {
                    acc += heights[b] * (c - a);
                } else if x > a {
                    acc += heights[b] * (x - a);
                    break;
                }
            }
            Some(acc.clamp(0.0, 1.0))
        }
        _ => None,
    }
}

/// `PDF[dist, x]` — dispatch non-parametric distributions, else delegate.
pub fn builtin_pdf_dispatch(args: &[Value]) -> Result<Value, EvalError> {
    if let (2, Value::Assoc(m)) = (args.len(), &args[0])
        && let Some(x) = as_f64(&args[1])
        && let Some(p) = npar_pdf(m, x)
    {
        return Ok(super::real(p));
    }
    st::builtin_pdf(args)
}

/// `CDF[dist, x]` — dispatch non-parametric distributions, else delegate.
pub fn builtin_cdf_dispatch(args: &[Value]) -> Result<Value, EvalError> {
    if let (2, Value::Assoc(m)) = (args.len(), &args[0])
        && let Some(x) = as_f64(&args[1])
        && let Some(p) = npar_cdf(m, x)
    {
        return Ok(super::real(p));
    }
    st::builtin_cdf(args)
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    use super::register_builtin;
    use super::register_builtin_env;
    // These builtins take distribution objects, so make sure the distribution
    // constructors / PDF / CDF from the Statistics package are always available
    // (idempotent; the lazy provider for the Statistics package still works).
    st::register(env);
    // `SeedRandom` is owned by `random` (registered later).
    register_builtin(env, "Quartiles", builtin_quartiles);
    register_builtin(env, "CentralMoment", builtin_central_moment);
    register_builtin(env, "Moment", builtin_moment);
    register_builtin(env, "Expectation", builtin_expectation);
    register_builtin(env, "Probability", builtin_probability);
    register_builtin(env, "InverseCDF", builtin_inverse_cdf);
    register_builtin(
        env,
        "MomentGeneratingFunction",
        builtin_moment_generating_function,
    );
    register_builtin(
        env,
        "CharacteristicFunction",
        builtin_characteristic_function,
    );
    register_builtin(env, "BinLists", builtin_bin_lists);
    register_builtin(
        env,
        "ExponentialMovingAverage",
        builtin_exponential_moving_average,
    );
    register_builtin(env, "SmoothHistogram", builtin_smooth_histogram);
    register_builtin(env, "FindDistribution", builtin_find_distribution);
    register_builtin(env, "EstimatedDistribution", builtin_estimated_distribution);
    // Survival / hazard / conditional / expected value
    register_builtin(env, "SurvivalFunction", builtin_survival_function);
    register_builtin(env, "HazardFunction", builtin_hazard_function);
    register_builtin(
        env,
        "InverseSurvivalFunction",
        builtin_inverse_survival_function,
    );
    register_builtin_env(
        env,
        "ConditionalProbability",
        builtin_conditional_probability,
    );
    register_builtin(env, "ExpectedValue", builtin_expected_value);
    // Descriptive / rank statistics
    register_builtin(env, "Commonest", builtin_commonest);
    register_builtin(env, "SpearmanRho", builtin_spearman_rho);
    register_builtin(env, "KendallTau", builtin_kendall_tau);
    // Parameter estimation and goodness-of-fit
    register_builtin(
        env,
        "FindDistributionParameters",
        builtin_find_distribution_parameters,
    );
    register_builtin(env, "ChiSquareTest", builtin_chi_square_test);
    register_builtin(
        env,
        "KolmogorovSmirnovTest",
        builtin_kolmogorov_smirnov_test,
    );
    register_builtin(env, "AndersonDarlingTest", builtin_anderson_darling_test);
    register_builtin(env, "DistributionFitTest", builtin_distribution_fit_test);
    // Location and variance hypothesis tests
    register_builtin(env, "MeanTest", builtin_mean_test);
    register_builtin(env, "LocationTest", builtin_location_test);
    register_builtin(env, "VarianceTest", builtin_variance_test);
    // Extended summaries: interpolation methods, trimmed/Winsorized means.
    // `Quantile`/`PDF`/`CDF` are re-registered here to add the method argument
    // and the non-parametric distribution kinds; all other cases delegate to
    // the Statistics implementations.
    register_builtin(env, "Quantile", builtin_quantile_method);
    register_builtin(env, "QuartileDeviation", builtin_quartile_deviation);
    register_builtin(env, "TrimmedMean", builtin_trimmed_mean);
    register_builtin(env, "WinsorizedMean", builtin_winsorized_mean);
    // Pearson chi-square goodness-of-fit (named form of the chi-square test)
    register_builtin(env, "PearsonChiSquareTest", builtin_pearson_chi_square_test);
    // Non-parametric density distributions
    register_builtin(env, "EmpiricalDistribution", builtin_empirical_distribution);
    register_builtin(env, "HistogramDistribution", builtin_histogram_distribution);
    register_builtin(
        env,
        "SmoothKernelDistribution",
        builtin_smooth_kernel_distribution,
    );
    register_builtin(
        env,
        "KernelMixtureDistribution",
        builtin_kernel_mixture_distribution,
    );
    register_builtin(env, "PDF", builtin_pdf_dispatch);
    register_builtin(env, "CDF", builtin_cdf_dispatch);
    // Hold the event/integrand so free variables survive argument evaluation
    // (e.g. `x == 2` must not collapse to `False`).
    env.set_attributes("Expectation", vec!["HoldFirst".to_string()]);
    env.set_attributes("Probability", vec!["HoldFirst".to_string()]);
    env.set_attributes("ExpectedValue", vec!["HoldFirst".to_string()]);
    // All three arguments of ConditionalProbability are held; the distribution
    // is evaluated explicitly inside the builtin.
    env.set_attributes("ConditionalProbability", vec!["HoldAll".to_string()]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn iv(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn lst(items: Vec<Value>) -> Value {
        Value::List(items)
    }

    fn normal(mu: f64, sd: f64) -> Value {
        make_normal(mu, sd)
    }

    fn as_f(v: &Value) -> f64 {
        as_f64(v).unwrap()
    }

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-3, "expected {b}, got {a}");
    }

    #[test]
    fn test_seed_random_reproducible() {
        assert!(matches!(
            builtin_seed_random(&[iv(42)]).unwrap(),
            Value::Null
        ));
        let a = as_f(&crate::builtins::random::builtin_random_real(&[]).unwrap());
        assert!(matches!(
            builtin_seed_random(&[iv(42)]).unwrap(),
            Value::Null
        ));
        let b = as_f(&crate::builtins::random::builtin_random_real(&[]).unwrap());
        assert_eq!(a, b);
    }

    #[test]
    fn test_seed_random_no_args() {
        assert!(matches!(builtin_seed_random(&[]).unwrap(), Value::Null));
    }

    #[test]
    fn test_quartiles() {
        let v = builtin_quartiles(&[lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)])]).unwrap();
        match v {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
                approx(as_f(&items[0]), 2.0);
                approx(as_f(&items[1]), 3.0);
                approx(as_f(&items[2]), 4.0);
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_central_moment() {
        let v =
            builtin_central_moment(&[lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]), iv(2)]).unwrap();
        approx(as_f(&v), 2.0);
    }

    #[test]
    fn test_moment() {
        let v = builtin_moment(&[lst(vec![iv(1), iv(2), iv(3)]), iv(1)]).unwrap();
        approx(as_f(&v), 2.0);
    }

    #[test]
    fn test_expectation_normal_constant() {
        let v = builtin_expectation(&[iv(5), normal(0.0, 1.0)]).unwrap();
        approx(as_f(&v), 5.0);
    }

    #[test]
    fn test_expectation_normal_x_squared() {
        let expr = Value::Call {
            head: "Power".to_string(),
            args: vec![Value::Symbol("x".to_string()), iv(2)],
        };
        let v = builtin_expectation(&[expr, normal(0.0, 1.0)]).unwrap();
        approx(as_f(&v), 1.0);
    }

    #[test]
    fn test_probability_less() {
        let event = Value::Call {
            head: "Less".to_string(),
            args: vec![Value::Symbol("x".to_string()), iv(0)],
        };
        let v = builtin_probability(&[event, normal(0.0, 1.0)]).unwrap();
        approx(as_f(&v), 0.5);
    }

    #[test]
    fn test_probability_interval() {
        // 0 < x < 1 for N(0,1) ~ 0.34134
        let event = Value::Call {
            head: "Less".to_string(),
            args: vec![
                Value::Call {
                    head: "Less".to_string(),
                    args: vec![iv(0), Value::Symbol("x".to_string())],
                },
                iv(1),
            ],
        };
        let v = builtin_probability(&[event, normal(0.0, 1.0)]).unwrap();
        approx(as_f(&v), 0.34134);
    }

    #[test]
    fn test_inverse_cdf_normal_median() {
        let v = builtin_inverse_cdf(&[normal(0.0, 1.0), super::super::real(0.5)]).unwrap();
        approx(as_f(&v), 0.0);
    }

    #[test]
    fn test_inverse_cdf_uniform() {
        let mut m = HashMap::new();
        m.insert(
            "Distribution".to_string(),
            Value::Str("Uniform".to_string()),
        );
        m.insert("Min".to_string(), super::super::real(0.0));
        m.insert("Max".to_string(), super::super::real(10.0));
        let v = builtin_inverse_cdf(&[Value::Assoc(m), super::super::real(0.25)]).unwrap();
        approx(as_f(&v), 2.5);
    }

    #[test]
    fn test_mgf_normal_numeric() {
        let v = builtin_moment_generating_function(&[normal(0.0, 1.0), super::super::real(0.5)])
            .unwrap();
        approx(as_f(&v), (0.125_f64).exp());
    }

    #[test]
    fn test_mgf_exponential_numeric() {
        let mut m = HashMap::new();
        m.insert(
            "Distribution".to_string(),
            Value::Str("Exponential".to_string()),
        );
        m.insert("Lambda".to_string(), super::super::real(2.0));
        let v = builtin_moment_generating_function(&[Value::Assoc(m), super::super::real(1.0)])
            .unwrap();
        approx(as_f(&v), 2.0 / 1.0);
    }

    #[test]
    fn test_mgf_normal_symbolic() {
        let v =
            builtin_moment_generating_function(&[normal(0.0, 1.0), Value::Symbol("t".to_string())])
                .unwrap();
        assert!(matches!(v, Value::Call { .. }));
    }

    #[test]
    fn test_cf_normal_numeric() {
        let v =
            builtin_characteristic_function(&[normal(0.0, 1.0), super::super::real(0.5)]).unwrap();
        approx(as_f(&v), (-0.125_f64).exp());
    }

    #[test]
    fn test_cf_uniform_real_zero() {
        let mut m = HashMap::new();
        m.insert(
            "Distribution".to_string(),
            Value::Str("Uniform".to_string()),
        );
        m.insert("Min".to_string(), super::super::real(0.0));
        m.insert("Max".to_string(), super::super::real(1.0));
        let v = builtin_characteristic_function(&[Value::Assoc(m), iv(0)]).unwrap();
        approx(as_f(&v), 1.0);
    }

    #[test]
    fn test_bin_lists() {
        let v = builtin_bin_lists(&[
            lst(vec![iv(0), iv(1), iv(2), iv(3)]),
            lst(vec![iv(0), iv(4), iv(2)]),
        ])
        .unwrap();
        match v {
            Value::List(bins) => {
                assert_eq!(bins.len(), 2);
                if let Value::List(b0) = &bins[0] {
                    assert_eq!(b0.len(), 2); // 0, 1
                } else {
                    panic!();
                }
                if let Value::List(b1) = &bins[1] {
                    assert_eq!(b1.len(), 2); // 2, 3
                } else {
                    panic!();
                }
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_exponential_moving_average() {
        let v =
            builtin_exponential_moving_average(&[lst(vec![iv(1), iv(2), iv(3)]), iv(1)]).unwrap();
        match v {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
                approx(as_f(&items[0]), 1.0);
                approx(as_f(&items[1]), 2.0);
                approx(as_f(&items[2]), 3.0);
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_smooth_histogram_svg() {
        let v = builtin_smooth_histogram(&[lst(vec![iv(1), iv(2), iv(2), iv(3), iv(4)])]).unwrap();
        assert!(matches!(v, Value::Str(s) if s.contains("<svg") && s.contains("polyline")));
    }

    #[test]
    fn test_find_distribution_mean() {
        let v = builtin_find_distribution(&[lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)])]).unwrap();
        let m = match v {
            Value::Assoc(m) => m,
            _ => panic!("expected distribution assoc"),
        };
        approx(as_f64(m.get("Mean").unwrap()).unwrap(), 3.0);
    }

    #[test]
    fn test_estimated_distribution() {
        let v = builtin_estimated_distribution(&[lst(vec![iv(2), iv(4), iv(6)]), normal(0.0, 1.0)])
            .unwrap();
        let m = match v {
            Value::Assoc(m) => m,
            _ => panic!("expected distribution assoc"),
        };
        approx(as_f64(m.get("Mean").unwrap()).unwrap(), 4.0);
    }

    #[test]
    fn test_inapplicable_returns_unevaluated() {
        assert!(matches!(
            builtin_quartiles(&[Value::Symbol("z".to_string())]),
            Err(EvalError::NoMatch { .. })
        ));
        assert!(matches!(
            builtin_expectation(&[iv(1)]),
            Err(EvalError::NoMatch { .. })
        ));
    }

    // ── Survival / hazard / inverse survival ─────────────────────────────────

    fn uniform(lo: f64, hi: f64) -> Value {
        let mut m = HashMap::new();
        m.insert(
            "Distribution".to_string(),
            Value::Str("Uniform".to_string()),
        );
        m.insert("Min".to_string(), super::super::real(lo));
        m.insert("Max".to_string(), super::super::real(hi));
        Value::Assoc(m)
    }

    fn exponential(lambda: f64) -> Value {
        let mut m = HashMap::new();
        m.insert(
            "Distribution".to_string(),
            Value::Str("Exponential".to_string()),
        );
        m.insert("Lambda".to_string(), super::super::real(lambda));
        Value::Assoc(m)
    }

    #[test]
    fn test_survival_function_normal() {
        let v = builtin_survival_function(&[normal(0.0, 1.0), super::super::real(0.0)]).unwrap();
        approx(as_f(&v), 0.5);
    }

    #[test]
    fn test_survival_function_uniform() {
        let v = builtin_survival_function(&[uniform(0.0, 4.0), super::super::real(1.0)]).unwrap();
        approx(as_f(&v), 0.75);
    }

    #[test]
    fn test_hazard_function_exponential_constant() {
        // The hazard rate of Exponential(λ) is the constant λ.
        let v = builtin_hazard_function(&[exponential(2.0), super::super::real(1.0)]).unwrap();
        approx(as_f(&v), 2.0);
    }

    #[test]
    fn test_inverse_survival_function_normal_median() {
        let v = builtin_inverse_survival_function(&[normal(0.0, 1.0), super::super::real(0.5)])
            .unwrap();
        approx(as_f(&v), 0.0);
    }

    #[test]
    fn test_inverse_survival_function_roundtrip() {
        // SF(InverseSurvivalFunction(dist, q)) ≈ q.
        let q = 0.25;
        let x =
            builtin_inverse_survival_function(&[uniform(0.0, 4.0), super::super::real(q)]).unwrap();
        let sf = builtin_survival_function(&[uniform(0.0, 4.0), x]).unwrap();
        approx(as_f(&sf), q);
    }

    // ── ConditionalProbability / ExpectedValue ───────────────────────────────

    #[test]
    fn test_conditional_probability_normal() {
        let env = Env::new();
        let e1 = Value::Call {
            head: "Less".to_string(),
            args: vec![Value::Symbol("x".to_string()), iv(1)],
        };
        let e2 = Value::Call {
            head: "Greater".to_string(),
            args: vec![Value::Symbol("x".to_string()), iv(0)],
        };
        let v = builtin_conditional_probability(&[e1, e2, normal(0.0, 1.0)], &env).unwrap();
        // P(x<1 | x>0) = (Φ(1) − 0.5) / 0.5 ≈ 0.6826895
        let phi1 = 0.8413447460685429_f64;
        approx(as_f(&v), (phi1 - 0.5) / 0.5);
    }

    #[test]
    fn test_expected_value_alias() {
        let v = builtin_expected_value(&[iv(5), normal(0.0, 1.0)]).unwrap();
        approx(as_f(&v), 5.0);
        let expr = Value::Call {
            head: "Power".to_string(),
            args: vec![Value::Symbol("x".to_string()), iv(2)],
        };
        let v = builtin_expected_value(&[expr, normal(0.0, 1.0)]).unwrap();
        approx(as_f(&v), 1.0);
    }

    // ── Commonest ────────────────────────────────────────────────────────────

    #[test]
    fn test_commonest_single_mode() {
        let v = builtin_commonest(&[lst(vec![iv(1), iv(2), iv(2), iv(3)])]).unwrap();
        match v {
            Value::List(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0], iv(2));
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_commonest_tie_preserves_order() {
        let v = builtin_commonest(&[lst(vec![iv(3), iv(3), iv(1), iv(1), iv(2)])]).unwrap();
        match v {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], iv(3));
                assert_eq!(items[1], iv(1));
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_commonest_take_n() {
        let v = builtin_commonest(&[lst(vec![iv(1), iv(1), iv(2), iv(2), iv(3)]), iv(1)]).unwrap();
        match v {
            Value::List(items) => assert_eq!(items.len(), 1),
            _ => panic!("expected list"),
        }
    }

    // ── Rank correlations ────────────────────────────────────────────────────

    #[test]
    fn test_spearman_rho_monotone() {
        let inc = lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]);
        let v = builtin_spearman_rho(&[inc.clone(), inc.clone()]).unwrap();
        approx(as_f(&v), 1.0);
        let dec = lst(vec![iv(5), iv(4), iv(3), iv(2), iv(1)]);
        let v = builtin_spearman_rho(&[inc, dec]).unwrap();
        approx(as_f(&v), -1.0);
    }

    #[test]
    fn test_spearman_rho_known() {
        let a = lst(vec![iv(1), iv(2), iv(3)]);
        let b = lst(vec![iv(1), iv(3), iv(2)]);
        let v = builtin_spearman_rho(&[a, b]).unwrap();
        approx(as_f(&v), 0.5);
    }

    #[test]
    fn test_kendall_tau_monotone() {
        let inc = lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]);
        let v = builtin_kendall_tau(&[inc.clone(), inc.clone()]).unwrap();
        approx(as_f(&v), 1.0);
        let dec = lst(vec![iv(5), iv(4), iv(3), iv(2), iv(1)]);
        let v = builtin_kendall_tau(&[inc, dec]).unwrap();
        approx(as_f(&v), -1.0);
    }

    #[test]
    fn test_kendall_tau_known() {
        let a = lst(vec![iv(1), iv(2), iv(3)]);
        let b = lst(vec![iv(1), iv(3), iv(2)]);
        let v = builtin_kendall_tau(&[a, b]).unwrap();
        approx(as_f(&v), 1.0 / 3.0);
    }

    // ── FindDistributionParameters ───────────────────────────────────────────

    #[test]
    fn test_find_distribution_parameters_normal() {
        let v = builtin_find_distribution_parameters(&[
            lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]),
            normal(0.0, 1.0),
        ])
        .unwrap();
        let items = match v {
            Value::List(items) => items,
            _ => panic!("expected list of rules"),
        };
        let mut mean = None;
        let mut sd = None;
        for r in items {
            if let Value::Rule { lhs, rhs, .. } = r
                && let Value::Symbol(s) = lhs.as_ref()
            {
                if s == "Mean" {
                    mean = as_f64(rhs.as_ref());
                } else if s == "SD" {
                    sd = as_f64(rhs.as_ref());
                }
            }
        }
        approx(mean.unwrap(), 3.0);
        approx(sd.unwrap(), 2.0_f64.sqrt());
    }

    // ── Goodness-of-fit tests ────────────────────────────────────────────────

    fn grid(n: usize) -> Value {
        // Midpoints of n equal-probability bins of Uniform(0, 1).
        let items: Vec<Value> = (0..n)
            .map(|i| super::super::real((i as f64 + 0.5) / n as f64))
            .collect();
        lst(items)
    }

    #[test]
    fn test_chi_square_test_match() {
        // Three equal bins each holding 3 points -> chi² = 0 -> p = 1.
        let mut items = Vec::new();
        for x in [0.5, 1.5, 2.5] {
            for _ in 0..3 {
                items.push(super::super::real(x));
            }
        }
        let v = builtin_chi_square_test(&[lst(items), uniform(0.0, 3.0)]).unwrap();
        approx(as_f(&v), 1.0);
    }

    #[test]
    fn test_chi_square_test_mismatch() {
        let items: Vec<Value> = (0..20).map(|_| super::super::real(0.1)).collect();
        let v = builtin_chi_square_test(&[lst(items), uniform(0.0, 10.0)]).unwrap();
        assert!(as_f(&v) < 0.01, "p = {}", as_f(&v));
    }

    #[test]
    fn test_kolmogorov_smirnov_test_match() {
        let v = builtin_kolmogorov_smirnov_test(&[grid(10), uniform(0.0, 1.0)]).unwrap();
        assert!(as_f(&v) > 0.9, "p = {}", as_f(&v));
    }

    #[test]
    fn test_kolmogorov_smirnov_test_mismatch() {
        let v = builtin_kolmogorov_smirnov_test(&[grid(10), uniform(0.0, 2.0)]).unwrap();
        assert!(as_f(&v) < 0.1, "p = {}", as_f(&v));
    }

    #[test]
    fn test_anderson_darling_test_match() {
        let v = builtin_anderson_darling_test(&[grid(20), uniform(0.0, 1.0)]).unwrap();
        assert!(as_f(&v) > 0.05, "p = {}", as_f(&v));
    }

    #[test]
    fn test_anderson_darling_test_mismatch() {
        let v = builtin_anderson_darling_test(&[grid(20), uniform(0.0, 2.0)]).unwrap();
        assert!(as_f(&v) < 0.05, "p = {}", as_f(&v));
    }

    #[test]
    fn test_distribution_fit_test_default_and_named() {
        let data = grid(20);
        let ad = builtin_anderson_darling_test(&[data.clone(), uniform(0.0, 1.0)]).unwrap();
        let default = builtin_distribution_fit_test(&[data.clone(), uniform(0.0, 1.0)]).unwrap();
        approx(as_f(&default), as_f(&ad));
        let ks = builtin_kolmogorov_smirnov_test(&[data.clone(), uniform(0.0, 1.0)]).unwrap();
        let named = builtin_distribution_fit_test(&[
            data,
            uniform(0.0, 1.0),
            Value::Str("KolmogorovSmirnovTest".to_string()),
        ])
        .unwrap();
        approx(as_f(&named), as_f(&ks));
    }

    #[test]
    fn test_new_builtins_unevaluated_on_bad_args() {
        assert!(matches!(
            builtin_survival_function(&[iv(1)]),
            Err(EvalError::NoMatch { .. })
        ));
        assert!(matches!(
            builtin_commonest(&[Value::Symbol("z".to_string())]),
            Err(EvalError::NoMatch { .. })
        ));
        assert!(matches!(
            builtin_spearman_rho(&[lst(vec![iv(1)])]),
            Err(EvalError::NoMatch { .. })
        ));
        assert!(matches!(
            builtin_find_distribution_parameters(&[lst(vec![iv(1)]), uniform(0.0, 1.0)]),
            Err(EvalError::NoMatch { .. })
        ));
    }

    // ── Hypothesis tests ─────────────────────────────────────────────────────

    #[test]
    fn test_mean_test_centered() {
        // Sample mean equals mu -> p = 1.
        let v = builtin_mean_test(&[lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]), iv(3)]).unwrap();
        approx(as_f(&v), 1.0);
    }

    #[test]
    fn test_mean_test_shifted_tiny_p() {
        let v = builtin_mean_test(&[lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]), iv(0)]).unwrap();
        let p = as_f(&v);
        assert!((0.0..0.05).contains(&p), "p = {p}");
    }

    #[test]
    fn test_mean_test_known_p_value() {
        // t = 4.2426, df = 4 -> two-sided p ~ 0.01313.
        let v = builtin_mean_test(&[
            lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]),
            super::super::real(0.0),
        ])
        .unwrap();
        assert!((as_f(&v) - 0.0131).abs() < 0.002, "p = {}", as_f(&v));
    }

    #[test]
    fn test_mean_test_two_sample_list_form() {
        let outer = lst(vec![
            lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]),
            lst(vec![iv(6), iv(7), iv(8), iv(9), iv(10)]),
        ]);
        let v = builtin_mean_test(&[outer]).unwrap();
        assert!(as_f(&v) < 0.01, "p = {}", as_f(&v));
    }

    #[test]
    fn test_mean_test_two_sample_flat_form() {
        let a = lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]);
        let b = lst(vec![iv(6), iv(7), iv(8), iv(9), iv(10)]);
        let v = builtin_mean_test(&[a, b]).unwrap();
        assert!(as_f(&v) < 0.01, "p = {}", as_f(&v));
    }

    #[test]
    fn test_mean_test_z_and_signed_rank() {
        let data = lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]);
        let z = builtin_mean_test(&[data.clone(), iv(0), Value::Str("Z".to_string())]).unwrap();
        assert!(as_f(&z) < 0.05, "z p = {}", as_f(&z));
        let sr = builtin_mean_test(&[
            lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5), iv(6)]),
            iv(0),
            Value::Str("SignedRank".to_string()),
        ])
        .unwrap();
        assert!(as_f(&sr) < 0.05, "signed-rank p = {}", as_f(&sr));
    }

    #[test]
    fn test_location_test_matches_mean_test() {
        let data = lst(vec![iv(2), iv(4), iv(6), iv(8)]);
        let a = builtin_mean_test(&[data.clone(), iv(3)]).unwrap();
        let b = builtin_location_test(&[data, iv(3)]).unwrap();
        approx(as_f(&a), as_f(&b));
    }

    #[test]
    fn test_variance_test_matching() {
        // s^2 = 2.5, stat = 4·2.5/2.5 = 4, df = 4 -> two-sided p ~ 0.812.
        let v = builtin_variance_test(&[
            lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]),
            super::super::real(2.5),
        ])
        .unwrap();
        assert!((as_f(&v) - 0.81).abs() < 0.05, "p = {}", as_f(&v));
    }

    #[test]
    fn test_variance_test_two_sample() {
        let outer = lst(vec![
            lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]),
            lst(vec![iv(10), iv(20), iv(30), iv(40), iv(50)]),
        ]);
        let v = builtin_variance_test(&[outer]).unwrap();
        assert!(as_f(&v) < 0.05, "p = {}", as_f(&v));
    }

    #[test]
    fn test_hypothesis_tests_unevaluated() {
        assert!(matches!(
            builtin_mean_test(&[Value::Symbol("z".to_string())]),
            Err(EvalError::NoMatch { .. })
        ));
        assert!(matches!(
            builtin_variance_test(&[lst(vec![iv(1)]), iv(2)]),
            Err(EvalError::NoMatch { .. })
        ));
        // Unknown named test.
        assert!(matches!(
            builtin_location_test(&[
                lst(vec![iv(1), iv(2), iv(3)]),
                iv(0),
                Value::Str("Bogus".to_string())
            ]),
            Err(EvalError::NoMatch { .. })
        ));
    }

    // ── Quantile interpolation methods 1–9 ───────────────────────────────────

    fn ten() -> Value {
        lst((1..=10).map(iv).collect())
    }

    #[test]
    fn test_quantile_methods_hf() {
        // Reference values from Hyndman & Fan (1996) for data 1..10 at q = 0.25.
        let expected = [
            (1i64, 3.0),
            (2, 3.0),
            (3, 2.0),
            (4, 2.5),
            (5, 3.0),
            (6, 2.75),
            (7, 3.25),
            (8, 2.916_666_666_666_666_5),
            (9, 2.9375),
        ];
        for (m, e) in expected {
            let v = builtin_quantile_method(&[ten(), super::super::real(0.25), iv(m)]).unwrap();
            approx(as_f(&v), e);
        }
    }

    #[test]
    fn test_quantile_method_named_and_default() {
        let d = lst(vec![iv(1), iv(2), iv(3), iv(4)]);
        let default = builtin_quantile_method(&[d.clone(), super::super::real(0.5)]).unwrap();
        approx(as_f(&default), 2.5);
        let named = builtin_quantile_method(&[
            d,
            super::super::real(0.5),
            Value::Str("LinearInterpolation".to_string()),
        ])
        .unwrap();
        approx(as_f(&named), 2.5);
    }

    #[test]
    fn test_quantile_method_endpoints_and_unevaluated() {
        let lo = builtin_quantile_method(&[ten(), iv(0), iv(7)]).unwrap();
        approx(as_f(&lo), 1.0);
        let hi = builtin_quantile_method(&[ten(), iv(1), iv(7)]).unwrap();
        approx(as_f(&hi), 10.0);
        assert!(matches!(
            builtin_quantile_method(&[ten(), super::super::real(0.5), iv(11)]),
            Err(EvalError::NoMatch { .. })
        ));
        assert!(matches!(
            builtin_quantile_method(&[
                ten(),
                super::super::real(0.5),
                Value::Str("Bogus".to_string())
            ]),
            Err(EvalError::NoMatch { .. })
        ));
    }

    #[test]
    fn test_quartile_deviation() {
        // Q1 = 2, Q3 = 4 for 1..5 -> (Q3 - Q1)/2 = 1.
        let v =
            builtin_quartile_deviation(&[lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)])]).unwrap();
        approx(as_f(&v), 1.0);
    }

    #[test]
    fn test_trimmed_mean_uniform_and_asymmetric() {
        // Drop the single lowest and highest of 1..10 -> mean(2..9) = 5.5.
        let v = builtin_trimmed_mean(&[ten(), super::super::real(0.1)]).unwrap();
        approx(as_f(&v), 5.5);
        // Drop 1 low, 2 high -> mean(2..8) = 5.
        let v = builtin_trimmed_mean(&[
            ten(),
            lst(vec![super::super::real(0.1), super::super::real(0.2)]),
        ])
        .unwrap();
        approx(as_f(&v), 5.0);
    }

    #[test]
    fn test_winsorized_mean() {
        // Clamp 1->2 and 10->9, then mean: (2+2+3+...+9+9)/10 = 5.5.
        let v = builtin_winsorized_mean(&[ten(), super::super::real(0.1)]).unwrap();
        approx(as_f(&v), 5.5);
    }

    #[test]
    fn test_trim_winsor_unevaluated() {
        assert!(matches!(
            builtin_trimmed_mean(&[ten(), super::super::real(0.6)]),
            Err(EvalError::NoMatch { .. })
        ));
        assert!(matches!(
            builtin_winsorized_mean(&[lst(vec![]), super::super::real(0.1)]),
            Err(EvalError::Error(_))
        ));
    }

    // ── Extended summaries (Statistics-owned builtins) ───────────────────────

    #[test]
    fn test_statistics_summaries() {
        use crate::builtins::statistics as stt;
        let data = lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]);
        approx(
            as_f(&stt::builtin_interquartile_range(&[data.clone()]).unwrap()),
            2.0,
        );
        approx(as_f(&stt::builtin_skewness(&[data.clone()]).unwrap()), 0.0);
        // m4 / m2^2 for 1..5 = 6.8 / 4 = 1.7.
        approx(as_f(&stt::builtin_kurtosis(&[data]).unwrap()), 1.7);

        let a = lst(vec![iv(1), iv(2), iv(3)]);
        let b = lst(vec![iv(2), iv(4), iv(6)]);
        approx(
            as_f(&stt::builtin_covariance(&[a.clone(), b.clone()]).unwrap()),
            2.0,
        );
        approx(as_f(&stt::builtin_correlation(&[a, b]).unwrap()), 1.0);
    }

    #[test]
    fn test_histogram_list_bin_forms() {
        use crate::builtins::statistics as stt;
        let data = lst(vec![iv(1), iv(2), iv(2), iv(3), iv(4), iv(5)]);
        for spec in [iv(3), iv(5)] {
            let v = stt::builtin_histogram_list(&[data.clone(), spec]).unwrap();
            let top = match v {
                Value::List(x) => x,
                _ => panic!("expected list"),
            };
            assert_eq!(top.len(), 2, "HistogramList returns {{edges, counts}}");
            let counts = match &top[1] {
                Value::List(c) => c,
                _ => panic!("expected counts list"),
            };
            let total: i64 = counts.iter().map(|v| as_f64(v).unwrap_or(0.0) as i64).sum();
            assert_eq!(total, 6);
        }
        // One-argument form uses Sturges' rule and still counts every point.
        let v = stt::builtin_histogram_list(&[data]).unwrap();
        let top = match v {
            Value::List(x) => x,
            _ => panic!("expected list"),
        };
        if let Value::List(c) = &top[1] {
            let total: i64 = c.iter().map(|v| as_f64(v).unwrap_or(0.0) as i64).sum();
            assert_eq!(total, 6);
        } else {
            panic!("expected counts list");
        }
    }

    // ── Pearson chi-square goodness-of-fit ───────────────────────────────────

    #[test]
    fn test_pearson_chi_square_match_and_mismatch() {
        let mut items = Vec::new();
        for x in [0.5, 1.5, 2.5] {
            for _ in 0..3 {
                items.push(super::super::real(x));
            }
        }
        let v = builtin_pearson_chi_square_test(&[lst(items), uniform(0.0, 3.0)]).unwrap();
        assert!(as_f(&v) > 0.9, "p = {}", as_f(&v));

        let flat: Vec<Value> = (0..20).map(|_| super::super::real(0.1)).collect();
        let v = builtin_pearson_chi_square_test(&[lst(flat), uniform(0.0, 10.0)]).unwrap();
        assert!(as_f(&v) < 0.01, "p = {}", as_f(&v));
    }

    #[test]
    fn test_pearson_chi_square_normality_form() {
        // Uniform midpoints; normality is a plausible-but-imperfect fit.
        let v = builtin_pearson_chi_square_test(&[grid(30)]).unwrap();
        assert!((0.0..=1.0).contains(&as_f(&v)));
    }

    #[test]
    fn test_pearson_chi_square_unevaluated() {
        assert!(matches!(
            builtin_pearson_chi_square_test(&[lst(vec![iv(1)])]),
            Err(EvalError::NoMatch { .. })
        ));
        assert!(matches!(
            builtin_pearson_chi_square_test(&[lst(vec![iv(1), iv(2)]), iv(3)]),
            Err(EvalError::NoMatch { .. })
        ));
    }

    #[test]
    fn test_distribution_fit_test_chi_square_named() {
        let mut items = Vec::new();
        for x in [0.5, 1.5, 2.5] {
            for _ in 0..3 {
                items.push(super::super::real(x));
            }
        }
        let data = lst(items);
        let direct = builtin_chi_square_test(&[data.clone(), uniform(0.0, 3.0)]).unwrap();
        let named = builtin_distribution_fit_test(&[
            data,
            uniform(0.0, 3.0),
            Value::Str("PearsonChiSquare".to_string()),
        ])
        .unwrap();
        approx(as_f(&named), as_f(&direct));
    }

    // ── Non-parametric density distributions ─────────────────────────────────

    #[test]
    fn test_empirical_distribution_pdf_cdf() {
        let d = builtin_empirical_distribution(&[lst(vec![iv(1), iv(2), iv(3), iv(4)])]).unwrap();
        approx(
            as_f(&builtin_cdf_dispatch(&[d.clone(), super::super::real(2.5)]).unwrap()),
            0.5,
        );
        approx(
            as_f(&builtin_pdf_dispatch(&[d.clone(), iv(3)]).unwrap()),
            0.25,
        );
        approx(
            as_f(&builtin_pdf_dispatch(&[d, super::super::real(2.5)]).unwrap()),
            0.0,
        );
    }

    #[test]
    fn test_histogram_distribution_pdf_cdf() {
        let d = builtin_histogram_distribution(&[lst(vec![iv(0), iv(0), iv(1), iv(1)]), iv(2)])
            .unwrap();
        // Two equal bins over [0,1], each density 1.
        approx(
            as_f(&builtin_pdf_dispatch(&[d.clone(), super::super::real(0.25)]).unwrap()),
            1.0,
        );
        approx(
            as_f(&builtin_cdf_dispatch(&[d.clone(), super::super::real(0.25)]).unwrap()),
            0.25,
        );
        approx(
            as_f(&builtin_cdf_dispatch(&[d, super::super::real(1.0)]).unwrap()),
            1.0,
        );
    }

    #[test]
    fn test_smooth_kernel_distribution_symmetry() {
        let d = builtin_smooth_kernel_distribution(&[
            lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]),
            super::super::real(0.5),
        ])
        .unwrap();
        let p2 = as_f(&builtin_pdf_dispatch(&[d.clone(), iv(2)]).unwrap());
        let p4 = as_f(&builtin_pdf_dispatch(&[d.clone(), iv(4)]).unwrap());
        approx(p2, p4);
        approx(
            as_f(&builtin_cdf_dispatch(&[d.clone(), iv(3)]).unwrap()),
            0.5,
        );
        assert!(as_f(&builtin_pdf_dispatch(&[d, iv(3)]).unwrap()) > 0.0);
    }

    #[test]
    fn test_kernel_mixture_distribution_positive() {
        let d = builtin_kernel_mixture_distribution(&[
            lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)]),
            super::super::real(0.4),
        ])
        .unwrap();
        assert!(as_f(&builtin_pdf_dispatch(&[d.clone(), iv(3)]).unwrap()) > 0.0);
        let p1 = as_f(&builtin_pdf_dispatch(&[d.clone(), iv(1)]).unwrap());
        let p5 = as_f(&builtin_pdf_dispatch(&[d, iv(5)]).unwrap());
        approx(p1, p5);
    }

    #[test]
    fn test_pdf_dispatch_delegates_for_normal() {
        // Non-parametric dispatch must fall through to the Statistics PDF.
        let v = builtin_pdf_dispatch(&[normal(0.0, 1.0), super::super::real(0.0)]).unwrap();
        approx(as_f(&v), 1.0 / (2.0 * std::f64::consts::PI).sqrt());
    }

    #[test]
    fn test_find_and_estimated_distribution_families() {
        let n = builtin_find_distribution(&[lst(vec![iv(1), iv(2), iv(3), iv(4), iv(5)])]).unwrap();
        if let Value::Assoc(m) = &n {
            approx(as_f64(m.get("Mean").unwrap()).unwrap(), 3.0);
        } else {
            panic!("expected distribution assoc");
        }
        let skew_data = lst(vec![iv(1), iv(2), iv(4), iv(8), iv(16)]);
        let e = builtin_find_distribution(&[skew_data]).unwrap();
        if let Value::Assoc(m) = &e {
            assert_eq!(dist_type(m), Some("Exponential"));
        } else {
            panic!("expected distribution assoc");
        }
        // MLE for Exponential over {2,4,6}: lambda = 1/mean = 0.25.
        let ed =
            builtin_estimated_distribution(&[lst(vec![iv(2), iv(4), iv(6)]), exponential(1.0)])
                .unwrap();
        if let Value::Assoc(m) = &ed {
            approx(as_f64(m.get("Lambda").unwrap()).unwrap(), 0.25);
        } else {
            panic!("expected distribution assoc");
        }
    }

    #[test]
    fn test_density_builtins_unevaluated() {
        assert!(matches!(
            builtin_smooth_kernel_distribution(&[lst(vec![iv(1)])]),
            Err(EvalError::NoMatch { .. })
        ));
        assert!(matches!(
            builtin_kernel_mixture_distribution(&[
                lst(vec![iv(1), iv(2)]),
                super::super::real(-1.0)
            ]),
            Err(EvalError::NoMatch { .. })
        ));
        assert!(matches!(
            builtin_histogram_distribution(&[lst(vec![iv(1), iv(2)]), iv(0)]),
            Err(EvalError::NoMatch { .. })
        ));
        assert!(matches!(
            builtin_empirical_distribution(&[lst(vec![])]),
            Err(EvalError::Error(_))
        ));
        assert!(matches!(
            builtin_estimated_distribution(&[lst(vec![iv(-1), iv(2), iv(3)]), exponential(1.0)]),
            Err(EvalError::NoMatch { .. })
        ));
    }
}
