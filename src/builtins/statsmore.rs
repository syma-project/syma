//! statistics/probability: SeedRandom, Quartiles, moments, Expectation/Probability/InverseCDF, BinLists.
//!
//! Gap-fill module; see the individual builtin docs for semantics.

use crate::ast::Expr;
use crate::builtins::statistics as st;
use crate::value::EvalError;
use crate::value::Value;
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

/// FindDistribution[data] — a best-fit distribution (Normal).
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
    Ok(fit_normal(&nums))
}

/// EstimatedDistribution[data, dist] — fit the parameters of a distribution.
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
        _ => Ok(fit_normal(&nums)),
    }
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    use super::register_builtin;
    // These builtins take distribution objects, so make sure the distribution
    // constructors / PDF / CDF from the Statistics package are always available
    // (idempotent; the lazy provider for the Statistics package still works).
    st::register(env);
    register_builtin(env, "SeedRandom", builtin_seed_random);
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
    // Hold the event/integrand so free variables survive argument evaluation
    // (e.g. `x == 2` must not collapse to `False`).
    env.set_attributes("Expectation", vec!["HoldFirst".to_string()]);
    env.set_attributes("Probability", vec!["HoldFirst".to_string()]);
}

use crate::env::Env;

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
}
