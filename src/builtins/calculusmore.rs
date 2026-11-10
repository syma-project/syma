//! calculus extras: InverseSeries, ComposeSeries, FunctionRange, Singularities,
//! Mellin/Fourier sin-cos transforms, a `SeriesData` constructor, and an
//! evaluator for `Residue`.
//!
//! Gap-fill module; see the individual builtin docs for semantics. Every
//! builtin returns an unevaluated `Value::Call` when its input is outside the
//! supported class.

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::Integer;

// ── Construction / evaluation helpers ──────────────────────────────────────

fn call(head: &str, args: Vec<Value>) -> Value {
    Value::Call {
        head: head.to_string(),
        args,
    }
}

fn sym(name: &str) -> Value {
    Value::Symbol(name.to_string())
}

fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

fn pow(base: Value, exp: Value) -> Value {
    call("Power", vec![base, exp])
}

fn times(mut args: Vec<Value>) -> Value {
    if args.iter().any(is_zero) {
        return int(0);
    }
    args.retain(|a| !is_one(a));
    match args.len() {
        0 => int(1),
        1 => args.pop().unwrap(),
        _ => call("Times", args),
    }
}

fn plus(mut args: Vec<Value>) -> Value {
    args.retain(|a| !is_zero(a));
    match args.len() {
        0 => int(0),
        1 => args.pop().unwrap(),
        _ => call("Plus", args),
    }
}

fn is_one(v: &Value) -> bool {
    matches!(v, Value::Integer(n) if *n == 1)
}

/// `Power[base, exp]`, flattening a nested `Power[Power[b, e], exp]`.
fn npow(base: Value, exp: Value) -> Value {
    if let Value::Call { head, args } = &base
        && head == "Power"
        && args.len() == 2
    {
        return pow(args[0].clone(), times(vec![args[1].clone(), exp]));
    }
    pow(base, exp)
}

/// Rewrite `Divide[a, b]` as `Times[a, Power[b, -1]]` throughout `v`.
fn normalize_div(v: &Value) -> Value {
    match v {
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => times(vec![
            normalize_div(&args[0]),
            npow(normalize_div(&args[1]), int(-1)),
        ]),
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            pow(normalize_div(&args[0]), normalize_div(&args[1]))
        }
        Value::Call { head, args } => call(head, args.iter().map(normalize_div).collect()),
        Value::List(items) => Value::List(items.iter().map(normalize_div).collect()),
        _ => v.clone(),
    }
}

fn neg(v: Value) -> Value {
    times(vec![int(-1), v])
}

fn div(a: Value, b: Value) -> Value {
    call("Divide", vec![a, b])
}

fn inf() -> Value {
    sym("Infinity")
}

fn neg_inf() -> Value {
    times(vec![int(-1), inf()])
}

fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

/// Convert a `Value` into an `Expr`, preserving `Rational`/`Complex` at depth.
fn value_to_expr_local(v: &Value) -> Expr {
    match v {
        Value::Rational(r) => Expr::Call {
            head: Box::new(Expr::Symbol("Divide".to_string())),
            args: vec![
                Expr::Integer(r.numer().clone()),
                Expr::Integer(r.denom().clone()),
            ],
        },
        Value::Complex { re, im } => Expr::Complex { re: *re, im: *im },
        Value::Call { head, args } => Expr::Call {
            head: Box::new(Expr::Symbol(head.clone())),
            args: args.iter().map(value_to_expr_local).collect(),
        },
        Value::List(items) => Expr::List(items.iter().map(value_to_expr_local).collect()),
        Value::Sequence(items) => Expr::Call {
            head: Box::new(Expr::Symbol("Sequence".to_string())),
            args: items.iter().map(value_to_expr_local).collect(),
        },
        _ => crate::eval::table::value_to_expr(v),
    }
}

/// Evaluate a constructed value, returning the original on failure.
fn eval_value(v: Value, env: &Env) -> Value {
    let expr = value_to_expr_local(&v);
    crate::eval::eval(&expr, env).unwrap_or(v)
}

/// Zero out floating-point parts below a tolerance (used for complex roots).
fn chop(v: Value) -> Value {
    const TOL: f64 = 1e-12;
    match v {
        Value::Real(r) => {
            if r.to_f64().abs() < TOL {
                int(0)
            } else {
                Value::Real(r)
            }
        }
        Value::Complex { re, im } => {
            let re = if re.abs() < TOL { 0.0 } else { re };
            let im = if im.abs() < TOL { 0.0 } else { im };
            if re == 0.0 && im == 0.0 {
                int(0)
            } else if im == 0.0 {
                Value::Real(rug::Float::with_val(crate::value::DEFAULT_PRECISION, re))
            } else {
                Value::Complex { re, im }
            }
        }
        other => other,
    }
}

/// Unwrap `Value::Pattern`/`Hold` produced by `HoldAll` attributes.
fn unwrap(v: &Value) -> Value {
    match v {
        Value::Pattern(p) => crate::pattern::unwrap_expr_to_value(p),
        Value::Hold(inner) | Value::HoldComplete(inner) => (**inner).clone(),
        _ => v.clone(),
    }
}

fn type_err(expected: &str, got: &Value) -> EvalError {
    EvalError::TypeError {
        expected: expected.to_string(),
        got: got.type_name().to_string(),
    }
}

fn as_symbol(v: &Value, what: &str) -> Result<String, EvalError> {
    match unwrap(v) {
        Value::Symbol(s) => Ok(s),
        other => Err(type_err(what, &other)),
    }
}

// ── Small predicates ───────────────────────────────────────────────────────

fn is_zero(v: &Value) -> bool {
    match v {
        Value::Integer(n) => n.is_zero(),
        Value::Real(r) => r.is_zero(),
        Value::Rational(r) => r.numer().is_zero(),
        Value::Complex { re, im } => *re == 0.0 && *im == 0.0,
        _ => false,
    }
}

fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => n.to_f64().into(),
        Value::Real(r) => r.to_f64().into(),
        Value::Rational(r) => r.to_f64().into(),
        _ => None,
    }
}

fn sign_of(v: &Value) -> Option<i32> {
    let f = to_f64(v)?;
    if f > 0.0 {
        Some(1)
    } else if f < 0.0 {
        Some(-1)
    } else {
        Some(0)
    }
}

fn is_const_symbol(s: &str) -> bool {
    matches!(
        s,
        "Pi" | "E"
            | "I"
            | "Degree"
            | "Infinity"
            | "EulerGamma"
            | "GoldenRatio"
            | "Catalan"
            | "Khinchin"
            | "True"
            | "False"
            | "Null"
    )
}

fn contains_var(v: &Value, name: &str) -> bool {
    match v {
        Value::Symbol(s) => s == name,
        Value::Call { args, .. } => args.iter().any(|a| contains_var(a, name)),
        Value::List(items) | Value::Sequence(items) => items.iter().any(|a| contains_var(a, name)),
        Value::Rule { lhs, rhs, .. } => contains_var(lhs, name) || contains_var(rhs, name),
        _ => false,
    }
}

fn contains_head(v: &Value, head: &str) -> bool {
    match v {
        Value::Call { head: h, args } => h == head || args.iter().any(|a| contains_head(a, head)),
        Value::List(items) | Value::Sequence(items) => items.iter().any(|a| contains_head(a, head)),
        _ => false,
    }
}

fn collect_free(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Symbol(s) if !is_const_symbol(s) => {
            if !out.contains(s) {
                out.push(s.clone());
            }
        }
        Value::Call { args, .. } => {
            for a in args {
                collect_free(a, out);
            }
        }
        Value::List(items) | Value::Sequence(items) => {
            for a in items {
                collect_free(a, out);
            }
        }
        _ => {}
    }
}

/// The unique free symbol of `v`, if there is exactly one.
fn free_symbol(v: &Value) -> Option<String> {
    let mut out = Vec::new();
    collect_free(v, &mut out);
    if out.len() == 1 { out.pop() } else { None }
}

fn is_any_infinity(v: &Value) -> bool {
    match v {
        Value::Symbol(s) => s == "Infinity" || s == "ComplexInfinity" || s == "Indeterminate",
        Value::Call { head, args } if head == "Times" && args.len() == 2 => {
            (matches!(&args[0], Value::Integer(n) if *n == -1)
                && matches!(&args[1], Value::Symbol(s) if s == "Infinity"))
                || (matches!(&args[1], Value::Integer(n) if *n == -1)
                    && matches!(&args[0], Value::Symbol(s) if s == "Infinity"))
        }
        _ => false,
    }
}

// ── Polynomial helpers ─────────────────────────────────────────────────────

fn expand(v: &Value) -> Value {
    crate::builtins::symbolic::builtin_expand(std::slice::from_ref(v)).unwrap_or_else(|_| v.clone())
}

fn is_polynomial_in(expr: &Value, var: &str) -> bool {
    if !contains_var(expr, var) {
        return true;
    }
    match expr {
        Value::Symbol(s) => s == var,
        Value::Call { head, args } if head == "Plus" || head == "Times" => {
            args.iter().all(|a| is_polynomial_in(a, var))
        }
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            matches!(&args[1], Value::Integer(k) if !k.is_negative())
                && is_polynomial_in(&args[0], var)
        }
        _ => false,
    }
}

/// Coefficient list (constant term first) of `expr` in `var`, when polynomial.
fn poly_coeffs(expr: &Value, var: &str) -> Option<Vec<Value>> {
    let e = expand(expr);
    if !is_polynomial_in(&e, var) {
        return None;
    }
    match crate::builtins::symbolic::builtin_coefficient_list(&[e, sym(var)]) {
        Ok(Value::List(items)) => Some(items),
        _ => None,
    }
}

/// `(m, c)` when `expr == m*var + c` with `m, c` free of `var`. Handles
/// non-polynomial coefficients (e.g. `a*x` with symbolic `a`) and `-x`.
fn affine(expr: &Value, var: &str) -> Option<(Value, Value)> {
    affine_rec(&expand(expr), var)
}

fn affine_rec(e: &Value, var: &str) -> Option<(Value, Value)> {
    if !contains_var(e, var) {
        return Some((int(0), e.clone()));
    }
    match e {
        Value::Symbol(s) if s == var => Some((int(1), int(0))),
        Value::Call { head, args } if head == "Plus" => {
            let mut m = int(0);
            let mut c = int(0);
            for t in args {
                let (mt, ct) = affine_rec(t, var)?;
                m = plus(vec![m, mt]);
                c = plus(vec![c, ct]);
            }
            Some((m, c))
        }
        Value::Call { head, args } if head == "Times" => {
            let mut coef = int(1);
            let mut seen = false;
            for f in args {
                if matches!(f, Value::Symbol(s) if s == var) {
                    if seen {
                        return None;
                    }
                    seen = true;
                } else if contains_var(f, var) {
                    return None;
                } else {
                    coef = times(vec![coef, f.clone()]);
                }
            }
            if seen { Some((coef, int(0))) } else { None }
        }
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => {
            if contains_var(&args[1], var) {
                return None;
            }
            let (m, c) = affine_rec(&args[0], var)?;
            Some((div(m, args[1].clone()), div(c, args[1].clone())))
        }
        _ => None,
    }
}

// ── SeriesData plumbing ────────────────────────────────────────────────────

/// A power series with an explicit lowest exponent `base`.
struct PSeries {
    base: i32,
    c: Vec<Value>,
}

/// Multiply two power series, truncating to exponents `< limit`.
fn smul(a: &PSeries, b: &PSeries, limit: i32, env: &Env) -> PSeries {
    let base = a.base + b.base;
    if base >= limit {
        return PSeries {
            base,
            c: Vec::new(),
        };
    }
    let len = (limit - base) as usize;
    let mut out = vec![int(0); len];
    for (i, av) in a.c.iter().enumerate() {
        if i >= len {
            break;
        }
        for (j, bv) in b.c.iter().enumerate() {
            if i + j >= len {
                break;
            }
            if is_zero(av) {
                break;
            }
            let p = eval_value(times(vec![av.clone(), bv.clone()]), env);
            out[i + j] = eval_value(plus(vec![out[i + j].clone(), p]), env);
        }
    }
    PSeries { base, c: out }
}

/// `b^p` truncated to exponents `< limit`.
fn spow(b: &PSeries, p: usize, limit: i32, env: &Env) -> PSeries {
    let mut acc = PSeries {
        base: 0,
        c: vec![int(1)],
    };
    for _ in 0..p {
        acc = smul(&acc, b, limit, env);
        if acc.c.is_empty() {
            break;
        }
    }
    acc
}

fn add_scaled(dst: &mut PSeries, src: &PSeries, factor: &Value, env: &Env) {
    for (i, cv) in src.c.iter().enumerate() {
        if is_zero(cv) {
            continue;
        }
        let e = src.base + i as i32;
        let idx = (e - dst.base) as usize;
        if idx >= dst.c.len() {
            continue;
        }
        let p = eval_value(times(vec![cv.clone(), factor.clone()]), env);
        dst.c[idx] = eval_value(plus(vec![dst.c[idx].clone(), p]), env);
    }
}

/// Raw view of a `Value::SeriesData`.
struct RawSeries {
    var: Value,
    x0: Value,
    coeffs: Vec<Value>,
    min: i32,
    max: i32,
    den: i32,
}

fn as_series(v: &Value) -> Option<RawSeries> {
    match v {
        Value::SeriesData {
            variable,
            expansion_point,
            coefficients,
            min_exponent,
            max_exponent,
            denominator,
        } => Some(RawSeries {
            var: (**variable).clone(),
            x0: (**expansion_point).clone(),
            coeffs: coefficients.clone(),
            min: *min_exponent,
            max: *max_exponent,
            den: *denominator,
        }),
        _ => None,
    }
}

fn raw_into_value(s: RawSeries) -> Value {
    Value::SeriesData {
        variable: Box::new(s.var),
        expansion_point: Box::new(s.x0),
        coefficients: s.coeffs,
        min_exponent: s.min,
        max_exponent: s.max,
        denominator: s.den,
    }
}

// ── SeriesData constructor ─────────────────────────────────────────────────

/// `SeriesData[x, x0, {a0, a1, ...}, nmin, nmax, den]` — build a series value.
fn builtin_series_data(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 5 && args.len() != 6 {
        return Err(EvalError::Error(
            "SeriesData requires 5 or 6 arguments".to_string(),
        ));
    }
    let var = match &args[0] {
        Value::Symbol(s) => s.clone(),
        other => return Err(type_err("Symbol", other)),
    };
    let coeffs = match &args[2] {
        Value::List(items) => items.clone(),
        other => return Err(type_err("List of coefficients", other)),
    };
    let min = args[3]
        .to_integer()
        .ok_or_else(|| type_err("Integer exponent", &args[3]))?;
    let max = args[4]
        .to_integer()
        .ok_or_else(|| type_err("Integer exponent", &args[4]))?;
    let den = match args.get(5) {
        Some(d) => d
            .to_integer()
            .ok_or_else(|| type_err("Integer denominator", d))?,
        None => 1,
    };
    Ok(Value::SeriesData {
        variable: Box::new(sym(&var)),
        expansion_point: Box::new(args[1].clone()),
        coefficients: coeffs,
        min_exponent: min as i32,
        max_exponent: max as i32,
        denominator: den as i32,
    })
}

// ── InverseSeries ──────────────────────────────────────────────────────────

/// `InverseSeries[series]` / `InverseSeries[series, x]` — series reversion.
fn builtin_inverse_series(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "InverseSeries requires 1 or 2 arguments".to_string(),
        ));
    }
    let out_var = if args.len() == 2 {
        Some(as_symbol(&args[1], "Symbol")?)
    } else {
        None
    };

    let series = match &args[0] {
        Value::SeriesData { .. } => args[0].clone(),
        expr => {
            let var = match &out_var {
                Some(v) => v.clone(),
                None => match free_symbol(expr) {
                    Some(s) => s,
                    None => return Ok(unevaluated("InverseSeries", args)),
                },
            };
            let spec = Value::List(vec![sym(&var), int(0), int(8)]);
            let s = eval_value(call("Series", vec![unwrap(expr), spec]), env);
            if !matches!(s, Value::SeriesData { .. }) {
                return Ok(unevaluated("InverseSeries", args));
            }
            s
        }
    };

    let raw = match as_series(&series) {
        Some(r) => r,
        None => return Ok(unevaluated("InverseSeries", args)),
    };
    if raw.min != 0 || raw.den != 1 || raw.coeffs.len() < 2 {
        return Ok(unevaluated("InverseSeries", args));
    }
    let a1 = raw.coeffs[1].clone();
    if is_zero(&a1) {
        return Ok(unevaluated("InverseSeries", args));
    }

    let n = raw.coeffs.len();
    // b_i for i in 1..n; b_0 = 0.
    let mut g = vec![int(0); n];
    g[1] = eval_value(div(int(1), a1.clone()), env);
    for k in 2..n {
        let mut total = int(0);
        for j in 2..=k {
            let aj = raw.coeffs.get(j).cloned().unwrap_or_else(|| int(0));
            if is_zero(&aj) {
                continue;
            }
            let gk = spow(
                &PSeries {
                    base: 0,
                    c: g.clone(),
                },
                j,
                k as i32 + 1,
                env,
            );
            let term = eval_value(times(vec![aj, gk.c[k].clone()]), env);
            total = eval_value(plus(vec![total, term]), env);
        }
        g[k] = eval_value(div(neg(total), a1.clone()), env);
    }

    let mut out_coeffs = g[1..n].to_vec();
    while out_coeffs.len() > 1 && is_zero(out_coeffs.last().unwrap()) {
        out_coeffs.pop();
    }
    let out_variable = match out_var {
        Some(v) => sym(&v),
        None => raw.var.clone(),
    };
    Ok(Value::SeriesData {
        variable: Box::new(out_variable),
        expansion_point: Box::new(eval_value(raw.coeffs[0].clone(), env)),
        coefficients: out_coeffs,
        min_exponent: 1,
        max_exponent: raw.max,
        denominator: 1,
    })
}

// ── ComposeSeries ──────────────────────────────────────────────────────────

fn compose_two(f: &RawSeries, h: &RawSeries, env: &Env) -> Option<RawSeries> {
    if f.den != 1 || h.den != 1 {
        return None;
    }
    if f.min < 0 {
        return None;
    }
    if !f.var.struct_eq(&h.var) || !f.x0.struct_eq(&h.x0) {
        return None;
    }
    // Effective leading exponent of `h`; the constant term must vanish.
    let start = h.coeffs.iter().position(|c| !is_zero(c))?;
    let h_base = h.min + start as i32;
    if h_base < 1 {
        return None;
    }
    let result_min = f.min.checked_mul(h_base)?;
    let result_max = (f.max - f.min).checked_mul(h_base)?.checked_add(f.min)?;
    if result_max <= result_min {
        return None;
    }
    let limit = result_max;
    let hh = PSeries {
        base: h_base,
        c: h.coeffs[start..].to_vec(),
    };
    let mut result = PSeries {
        base: result_min,
        c: vec![int(0); (result_max - result_min) as usize],
    };
    for (j, av) in f.coeffs.iter().enumerate() {
        if is_zero(av) {
            continue;
        }
        let p = (f.min + j as i32).max(0) as usize;
        let hp = spow(&hh, p, limit, env);
        add_scaled(&mut result, &hp, av, env);
    }
    Some(RawSeries {
        var: f.var.clone(),
        x0: f.x0.clone(),
        coeffs: result.c,
        min: result_min,
        max: result_max,
        den: 1,
    })
}

/// `ComposeSeries[s1, s2, ...]` — composition of power series.
fn builtin_compose_series(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "ComposeSeries requires at least 2 arguments".to_string(),
        ));
    }
    let mut acc = match as_series(&args[0]) {
        Some(s) => s,
        None => return Ok(unevaluated("ComposeSeries", args)),
    };
    for a in &args[1..] {
        let h = match as_series(a) {
            Some(s) => s,
            None => return Ok(unevaluated("ComposeSeries", args)),
        };
        acc = match compose_two(&acc, &h, env) {
            Some(r) => r,
            None => return Ok(unevaluated("ComposeSeries", args)),
        };
    }
    Ok(raw_into_value(acc))
}

// ── FunctionRange ──────────────────────────────────────────────────────────

fn interval(lo: Value, hi: Value) -> Value {
    call("Interval", vec![Value::List(vec![lo, hi])])
}

/// `FunctionRange[f, x]` — the range of `f` as a real function of `x`.
fn builtin_function_range(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "FunctionRange requires 2 or 3 arguments".to_string(),
        ));
    }
    let f = unwrap(&args[0]);
    let x = as_symbol(&args[1], "Symbol")?;
    if !contains_var(&f, &x) {
        return Ok(Value::List(vec![eval_value(f, env)]));
    }

    if let Value::Call { head, args: a } = &f
        && a.len() == 1
        && let Some((m, _c)) = affine(&a[0], &x)
        && !is_zero(&m)
    {
        match head.as_str() {
            "Exp" => {
                return Ok(if sign_of(&m).unwrap_or(0) > 0 {
                    interval(int(0), inf())
                } else {
                    interval(int(0), int(1))
                });
            }
            "Sin" | "Cos" => return Ok(interval(int(-1), int(1))),
            "Abs" => return Ok(interval(int(0), inf())),
            "Sqrt" if sign_of(&m).unwrap_or(0) > 0 => return Ok(interval(int(0), inf())),
            "Log" | "Log2" | "Log10" => return Ok(sym("Reals")),
            _ => {}
        }
    }

    if let Some(c) = poly_coeffs(&f, &x) {
        match c.len() {
            1 => return Ok(Value::List(vec![c[0].clone()])),
            2 => return Ok(sym("Reals")),
            3 => {
                let a = &c[2];
                let b = &c[1];
                let cc = &c[0];
                if is_zero(a) {
                    return Ok(sym("Reals"));
                }
                let vertex = eval_value(
                    plus(vec![
                        cc.clone(),
                        neg(div(pow(b.clone(), int(2)), times(vec![int(4), a.clone()]))),
                    ]),
                    env,
                );
                match sign_of(a) {
                    Some(s) if s > 0 => return Ok(interval(vertex, inf())),
                    Some(s) if s < 0 => return Ok(interval(neg_inf(), vertex)),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    Ok(unevaluated("FunctionRange", args))
}

// ── Singularities ──────────────────────────────────────────────────────────

fn extract_solve_roots(v: &Value) -> Option<Vec<Value>> {
    if let Value::List(outer) = v
        && let Some(Value::Rule { rhs, .. }) = outer.first()
        && let Value::List(vals) = rhs.as_ref()
    {
        return Some(vals.clone());
    }
    None
}

/// Roots of a polynomial (or, failing that, of `den` via `Solve`).
fn roots_of(den: &Value, x: &str, env: &Env) -> Option<Vec<Value>> {
    if let Some(c) = poly_coeffs(den, x) {
        match c.len() {
            0 | 1 => return Some(Vec::new()),
            2 => {
                let r = eval_value(div(neg(c[0].clone()), c[1].clone()), env);
                return Some(vec![r]);
            }
            3 => {
                let a = &c[2];
                let b = &c[1];
                let cc = &c[0];
                let disc = eval_value(
                    plus(vec![
                        pow(b.clone(), int(2)),
                        neg(times(vec![int(4), a.clone(), cc.clone()])),
                    ]),
                    env,
                );
                let sq = eval_value(call("Sqrt", vec![disc]), env);
                let two_a = times(vec![int(2), a.clone()]);
                let r1 = eval_value(
                    div(plus(vec![neg(b.clone()), sq.clone()]), two_a.clone()),
                    env,
                );
                let r2 = eval_value(div(plus(vec![neg(b.clone()), neg(sq)]), two_a), env);
                return Some(vec![r1, r2]);
            }
            _ => {}
        }
    }
    let eq = call("Equal", vec![den.clone(), int(0)]);
    let sol = eval_value(call("Solve", vec![eq, sym(x)]), env);
    extract_solve_roots(&sol)
}

/// `Singularities[f, x]` — points where `f` is not analytic.
fn builtin_singularities(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Singularities requires exactly 2 arguments".to_string(),
        ));
    }
    let f = unwrap(&args[0]);
    let x = as_symbol(&args[1], "Symbol")?;
    let g = eval_value(call("Together", vec![f.clone()]), env);

    if let Value::Call { head, args: a } = &g
        && a.len() == 1
        && a[0].struct_eq(&sym(&x))
    {
        let half_pi = div(sym("Pi"), int(2));
        match head.as_str() {
            "Log" | "Log2" | "Log10" | "Sqrt" | "ArcTanh" => {
                return Ok(Value::List(vec![int(0)]));
            }
            "Tan" | "Sec" => {
                return Ok(Value::List(vec![plus(vec![
                    half_pi,
                    times(vec![sym("k"), sym("Pi")]),
                ])]));
            }
            "Cot" | "Csc" => return Ok(Value::List(vec![times(vec![sym("k"), sym("Pi")])])),
            _ => {}
        }
    }

    let den = eval_value(call("Denominator", vec![g]), env);
    if !contains_var(&den, &x) {
        return Ok(Value::List(Vec::new()));
    }
    if let Some(roots) = roots_of(&den, &x, env) {
        let mut out: Vec<Value> = Vec::new();
        for r in roots {
            if !out.iter().any(|o| o.struct_eq(&r)) {
                out.push(r);
            }
        }
        return Ok(Value::List(out));
    }
    Ok(unevaluated("Singularities", args))
}

// ── Integral-transform plumbing ────────────────────────────────────────────

/// `u` when `factor` is `E^u` / `Exp[u]`.
fn exp_argument(factor: &Value) -> Option<Value> {
    match factor {
        Value::Call { head, args } if head == "Exp" && args.len() == 1 => Some(args[0].clone()),
        Value::Call { head, args }
            if head == "Power"
                && args.len() == 2
                && matches!(&args[0], Value::Symbol(s) if s == "E") =>
        {
            Some(args[1].clone())
        }
        _ => None,
    }
}

fn factors_of(f: &Value) -> Vec<Value> {
    match f {
        Value::Call { head, args } if head == "Times" => args.clone(),
        other => vec![other.clone()],
    }
}

/// Split into `(constant-factor, s-dependent factors)`.
fn split_constant(f: &Value, var: &str, env: &Env) -> (Value, Vec<Value>) {
    let mut coeff = int(1);
    let mut rest = Vec::new();
    for fac in factors_of(f) {
        if contains_var(&fac, var) {
            rest.push(fac);
        } else {
            coeff = eval_value(times(vec![coeff, fac]), env);
        }
    }
    (coeff, rest)
}

/// True when `exp == -var` (e.g. `Times[-1, s]`).
#[allow(dead_code)]
fn is_neg_var(exp: &Value, var: &str) -> bool {
    match exp {
        Value::Call { head, args } if head == "Times" => {
            let mut has_neg = false;
            let mut has_var = false;
            for a in args {
                if matches!(a, Value::Integer(n) if *n == -1) {
                    has_neg = true;
                } else if matches!(a, Value::Symbol(n) if n == var) {
                    has_var = true;
                } else {
                    return false;
                }
            }
            has_neg && has_var
        }
        _ => false,
    }
}

// ── MellinTransform ────────────────────────────────────────────────────────

fn half() -> Value {
    Value::Rational(Box::new(rug::Rational::from((1, 2))))
}

/// Exponent `p` when `factor == x^p` with `p` free of `x`.
fn power_exponent(factor: &Value, x: &str) -> Option<Value> {
    if matches!(factor, Value::Symbol(s) if s == x) {
        return Some(int(1));
    }
    if let Value::Call { head, args } = factor
        && head == "Power"
        && args.len() == 2
        && args[0].struct_eq(&sym(x))
        && !contains_var(&args[1], x)
    {
        return Some(args[1].clone());
    }
    None
}

/// Coefficient `c` when `u == c*x^2` (`c` free of `x`).
fn squared_coeff(u: &Value, x: &str) -> Option<Value> {
    let c = poly_coeffs(&expand(u), x)?;
    if c.len() == 3 && is_zero(&c[1]) && !is_zero(&c[2]) {
        Some(c[2].clone())
    } else {
        None
    }
}

/// Exact table for `MellinTransform[f, x, s]`.
fn mellin_term(f: &Value, x: &str, s: &str, env: &Env) -> Option<Value> {
    let (coeff, rest) = split_constant(f, x, env);
    if rest.is_empty() {
        return Some(int(0));
    }
    let mut power = int(0);
    let mut exp_a: Option<Value> = None;
    let mut gauss_a: Option<Value> = None;
    let mut cauchy = false;
    let mut trig: Option<(&'static str, Value)> = None;
    for r in &rest {
        if let Some(p) = power_exponent(r, x) {
            if !is_zero(&power) {
                return None;
            }
            power = p;
            continue;
        }
        if let Some(u) = exp_argument(r) {
            if let Some(c) = squared_coeff(&u, x) {
                if gauss_a.is_some() || exp_a.is_some() {
                    return None;
                }
                let a = neg(c);
                if sign_of(&a).map(|s| s <= 0).unwrap_or(false) {
                    return None;
                }
                gauss_a = Some(a);
                continue;
            }
            if let Some((m, c)) = affine(&u, x)
                && is_zero(&c)
                && sign_of(&m).map(|s| s < 0).unwrap_or(true)
            {
                if exp_a.is_some() || gauss_a.is_some() {
                    return None;
                }
                exp_a = Some(neg(m));
                continue;
            }
            return None;
        }
        if let Value::Call { head, args } = r
            && head == "Power"
            && args.len() == 2
            && args[1] == int(-1)
            && args[0].struct_eq(&plus(vec![int(1), sym(x)]))
        {
            cauchy = true;
            continue;
        }
        if let Value::Call { head, args } = r
            && args.len() == 1
            && (head == "Sin" || head == "Cos")
        {
            let (m, c) = affine(&args[0], x)?;
            if !is_zero(&c) {
                return None;
            }
            trig = Some((if head == "Sin" { "Sin" } else { "Cos" }, m));
            continue;
        }
        return None;
    }

    let base = if cauchy {
        times(vec![
            sym("Pi"),
            call("Csc", vec![times(vec![sym("Pi"), sym(s)])]),
        ])
    } else if let Some(a) = gauss_a {
        if !is_zero(&power) {
            return None;
        }
        times(vec![
            half(),
            pow(a, neg(div(sym(s), int(2)))),
            call("Gamma", vec![div(sym(s), int(2))]),
        ])
    } else if let Some(a) = exp_a {
        let sp = plus(vec![sym(s), power.clone()]);
        times(vec![call("Gamma", vec![sp.clone()]), pow(a, neg(sp))])
    } else if let Some((kind, w)) = trig {
        if !is_zero(&power) {
            return None;
        }
        let arg = times(vec![half(), sym("Pi"), sym(s)]);
        times(vec![
            call("Gamma", vec![sym(s)]),
            pow(w, neg(sym(s))),
            call(kind, vec![arg]),
        ])
    } else {
        // Pure power t^p (p != 0) transforms to 0.
        return Some(int(0));
    };
    Some(eval_value(times(vec![coeff, base]), env))
}

/// `MellinTransform[f, x, s]` = `Integrate[f x^(s-1), {x, 0, Infinity}]`.
fn builtin_mellin_transform(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "MellinTransform requires exactly 3 arguments".to_string(),
        ));
    }
    let f = normalize_div(&unwrap(&args[0]));
    let x = as_symbol(&args[1], "Symbol")?;
    let s = as_symbol(&args[2], "Symbol")?;

    let terms = match &f {
        Value::Call { head, args: a } if head == "Plus" => a.clone(),
        other => vec![other.clone()],
    };
    let mut out = Vec::with_capacity(terms.len());
    for t in &terms {
        match mellin_term(t, &x, &s, env) {
            Some(r) => out.push(r),
            None => return Ok(unevaluated("MellinTransform", args)),
        }
    }
    Ok(eval_value(plus(out), env))
}

// ── InverseMellinTransform ─────────────────────────────────────────────────

/// True when `exp == -(s + p)` up to factor ordering.
fn is_neg_shift(exp: &Value, s: &str, p: &Value) -> bool {
    let target = plus(vec![sym(s), p.clone()]);
    match exp {
        Value::Call { head, args } if head == "Times" => {
            let mut has_neg = false;
            let mut has_target = false;
            for a in args {
                if matches!(a, Value::Integer(n) if *n == -1) {
                    has_neg = true;
                } else if a.struct_eq(&target) {
                    has_target = true;
                } else {
                    return false;
                }
            }
            has_neg && has_target
        }
        _ => false,
    }
}

fn inverse_mellin_term(g: &Value, s: &str, x: &str, env: &Env) -> Option<Value> {
    let (coeff, rest) = split_constant(g, s, env);
    if rest.is_empty() {
        return None;
    }
    let mut gammas: Vec<Value> = Vec::new();
    let mut powers: Vec<(Value, Value)> = Vec::new();
    let mut csc = false;
    for r in &rest {
        match r {
            Value::Call { head, args } if head == "Gamma" && args.len() == 1 => {
                gammas.push(args[0].clone());
            }
            Value::Call { head, args } if head == "Csc" && args.len() == 1 => {
                if args[0].struct_eq(&times(vec![sym("Pi"), sym(s)])) {
                    csc = true;
                } else {
                    return None;
                }
            }
            Value::Call { head, args } if head == "Power" && args.len() == 2 => {
                if contains_var(&args[0], s) {
                    return None;
                }
                powers.push((args[0].clone(), args[1].clone()));
            }
            _ => return None,
        }
    }
    if csc {
        let c = eval_value(div(coeff, sym("Pi")), env);
        return Some(eval_value(
            times(vec![c, div(int(1), plus(vec![int(1), sym(x)]))]),
            env,
        ));
    }
    if gammas.len() > 1 || powers.len() > 1 {
        return None;
    }
    let p = match gammas.first() {
        Some(arg) => {
            let (m, c) = affine(arg, s)?;
            if !is_one(&m) {
                return None;
            }
            c
        }
        None => int(0),
    };
    let b = match powers.first() {
        Some((base, exp)) => {
            if is_neg_shift(exp, s, &p) {
                base.clone()
            } else {
                return None;
            }
        }
        None => int(1),
    };
    let res = times(vec![
        coeff,
        pow(sym(x), p),
        pow(sym("E"), neg(div(sym(x), b))),
    ]);
    Some(eval_value(res, env))
}

/// `InverseMellinTransform[g, s, x]` — inverse Mellin transform (table based).
fn builtin_inverse_mellin_transform(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "InverseMellinTransform requires exactly 3 arguments".to_string(),
        ));
    }
    let g = normalize_div(&unwrap(&args[0]));
    let s = as_symbol(&args[1], "Symbol")?;
    let x = as_symbol(&args[2], "Symbol")?;

    let terms = match &g {
        Value::Call { head, args: a } if head == "Plus" => a.clone(),
        other => vec![other.clone()],
    };
    let mut out = Vec::with_capacity(terms.len());
    for t in &terms {
        match inverse_mellin_term(t, &s, &x, env) {
            Some(r) => out.push(r),
            None => return Ok(unevaluated("InverseMellinTransform", args)),
        }
    }
    Ok(eval_value(plus(out), env))
}

// ── Fourier sin / cos transforms ───────────────────────────────────────────

/// Exact table for the half-line Fourier sine/cosine transforms.
fn fourier_term(f: &Value, x: &str, k: &str, is_sin: bool, env: &Env) -> Option<Value> {
    let (coeff, rest) = split_constant(f, x, env);
    if rest.len() != 1 {
        return None;
    }
    let u = exp_argument(&rest[0])?;
    let (m, c) = affine(&u, x)?;
    if !is_zero(&c) || sign_of(&m).map(|s| s >= 0).unwrap_or(false) {
        return None;
    }
    let a = neg(m);
    let den = plus(vec![pow(a.clone(), int(2)), pow(sym(k), int(2))]);
    let body = if is_sin {
        div(sym(k), den)
    } else {
        div(a, den)
    };
    let pref = call("Sqrt", vec![div(int(2), sym("Pi"))]);
    Some(eval_value(times(vec![coeff, pref, body]), env))
}

fn fourier_sin_cos(
    args: &[Value],
    env: &Env,
    name: &str,
    is_sin: bool,
) -> Result<Value, EvalError> {
    if args.len() < 3 || args.len() > 4 {
        return Err(EvalError::Error(format!(
            "{name} requires 3 or 4 arguments"
        )));
    }
    let f = normalize_div(&unwrap(&args[0]));
    let x = as_symbol(&args[1], "Symbol")?;
    let k = as_symbol(&args[2], "Symbol")?;
    let terms = match &f {
        Value::Call { head, args: a } if head == "Plus" => a.clone(),
        other => vec![other.clone()],
    };
    let mut out = Vec::with_capacity(terms.len());
    for t in &terms {
        match fourier_term(t, &x, &k, is_sin, env) {
            Some(r) => out.push(r),
            None => return Ok(unevaluated(name, args)),
        }
    }
    Ok(eval_value(plus(out), env))
}

/// `FourierSinTransform[f, x, k]` = `Sqrt[2/Pi] Integrate[f Sin[k x], {x, 0, Infinity}]`.
fn builtin_fourier_sin_transform(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    fourier_sin_cos(args, env, "FourierSinTransform", true)
}

/// `FourierCosTransform[f, x, k]` = `Sqrt[2/Pi] Integrate[f Cos[k x], {x, 0, Infinity}]`.
fn builtin_fourier_cos_transform(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    fourier_sin_cos(args, env, "FourierCosTransform", false)
}

// ── Residue (extended) ─────────────────────────────────────────────────────

fn limit_is_finite(v: &Value, x: &str) -> bool {
    !contains_var(v, x) && !is_any_infinity(v) && !contains_head(v, "Limit")
}

/// Substitute `var` with `val` throughout `expr`.
fn subst_value(expr: &Value, var: &str, val: &Value) -> Value {
    match expr {
        Value::Symbol(s) if s == var => val.clone(),
        Value::Call { head, args } => Value::Call {
            head: head.clone(),
            args: args.iter().map(|a| subst_value(a, var, val)).collect(),
        },
        Value::List(items) => Value::List(items.iter().map(|a| subst_value(a, var, val)).collect()),
        _ => expr.clone(),
    }
}

/// Split `expr` into `(numerator, denominator)` by pulling out `Divide` and
/// negative integer powers.
fn num_den(expr: &Value) -> (Value, Value) {
    match expr {
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => {
            let (nn, nd) = num_den(&args[0]);
            let (dn, dd) = num_den(&args[1]);
            (times(vec![nn, dd]), times(vec![nd, dn]))
        }
        Value::Call { head, args }
            if head == "Power"
                && args.len() == 2
                && let Value::Integer(k) = &args[1]
                && k.is_negative() =>
        {
            let (bn, bd) = num_den(&args[0]);
            let e = -k.clone();
            (
                pow(bd, Value::Integer(e.clone())),
                pow(bn, Value::Integer(e)),
            )
        }
        Value::Call { head, args } if head == "Times" => {
            let mut n = int(1);
            let mut d = int(1);
            for a in args {
                let (an, ad) = num_den(a);
                n = times(vec![n, an]);
                d = times(vec![d, ad]);
            }
            (n, d)
        }
        _ => (expr.clone(), int(1)),
    }
}

/// Horner evaluation of an ascending coefficient list at `a`.
fn horner(coeffs: &[Value], a: &Value, env: &Env) -> Value {
    let mut acc = int(0);
    for c in coeffs.iter().rev() {
        acc = eval_value(times(vec![acc, a.clone()]), env);
        acc = eval_value(plus(vec![acc, c.clone()]), env);
    }
    acc
}

/// Rebuild `sum coeffs[i] * x^i`.
fn poly_from_coeffs(coeffs: &[Value], x: &str) -> Value {
    let mut terms = Vec::new();
    for (i, c) in coeffs.iter().enumerate() {
        if is_zero(c) {
            continue;
        }
        terms.push(match i {
            0 => c.clone(),
            1 => times(vec![c.clone(), sym(x)]),
            _ => times(vec![c.clone(), pow(sym(x), int(i as i64))]),
        });
    }
    plus(terms)
}

/// Multiplicity of the root `x0` in `den`, together with `den/(x-x0)^m`.
fn root_multiplicity(den: &Value, x0: &Value, x: &str, env: &Env) -> Option<(i64, Value)> {
    let mut coeffs = poly_coeffs(den, x)?;
    let mut m = 0i64;
    loop {
        if coeffs.is_empty() {
            break;
        }
        let rem = chop(horner(&coeffs, x0, env));
        if !is_zero(&rem) {
            break;
        }
        let n = coeffs.len() - 1;
        if n == 0 {
            break;
        }
        let mut q = vec![int(0); n];
        q[n - 1] = coeffs[n].clone();
        for i in (1..n).rev() {
            let t = eval_value(times(vec![x0.clone(), q[i].clone()]), env);
            q[i - 1] = eval_value(plus(vec![coeffs[i].clone(), t]), env);
        }
        coeffs = q;
        m += 1;
        if m > 16 {
            break;
        }
    }
    Some((m, poly_from_coeffs(&coeffs, x)))
}

/// `Residue[expr, {x, x0}]` — residue of `expr` at the point `x0`.
///
/// For expressions with a polynomial denominator this uses
/// `1/(m-1)! d^(m-1)/dx^(m-1) [(x-x0)^m f]|_{x=x0}`, `m` being the multiplicity
/// of `x0` in the denominator. Otherwise it falls back to a limiting form.
fn builtin_residue_ext(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Residue requires exactly 2 arguments: Residue[expr, {x, x0}]".to_string(),
        ));
    }
    let (x, x0) = match &args[1] {
        Value::List(items) if items.len() >= 2 && items.len() <= 3 => match &items[0] {
            Value::Symbol(s) => (s.clone(), items[1].clone()),
            other => return Err(type_err("Symbol", other)),
        },
        other => {
            return Err(EvalError::TypeError {
                expected: "{x, x0}".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };
    let expr = unwrap(&args[0]);

    let (num, den) = num_den(&expr);
    let den = eval_value(den, env);
    if !contains_var(&den, &x) {
        return Ok(int(0));
    }
    if let Some((m, red_den)) = root_multiplicity(&den, &x0, &x, env) {
        if m == 0 {
            return Ok(int(0));
        }
        let h = eval_value(div(num, red_den), env);
        if m == 1 {
            return Ok(chop(eval_value(subst_value(&h, &x, &x0), env)));
        }
        let spec = Value::List(vec![sym(&x), int(m - 1)]);
        let derivative = match crate::builtins::symbolic::builtin_d(&[h, spec], env) {
            Ok(d) => d,
            Err(_) => return Ok(unevaluated("Residue", args)),
        };
        let at = chop(eval_value(subst_value(&derivative, &x, &x0), env));
        let fact = Value::Integer(crate::builtins::math::factorial(m - 1));
        return Ok(eval_value(div(at, fact), env));
    }

    // Fallback for a non-polynomial denominator: limiting form.
    let rule = Value::Rule {
        lhs: Box::new(sym(&x)),
        rhs: Box::new(x0.clone()),
        delayed: false,
    };
    for m in 1..=8i64 {
        let h = eval_value(
            times(vec![
                pow(plus(vec![sym(&x), neg(x0.clone())]), int(m)),
                expr.clone(),
            ]),
            env,
        );
        let lim = crate::builtins::symbolicmanip::builtin_limit(&[h.clone(), rule.clone()])
            .unwrap_or_else(|_| unevaluated("Limit", &[h.clone(), rule.clone()]));
        if !limit_is_finite(&lim, &x) {
            continue;
        }
        if m == 1 {
            return Ok(eval_value(lim, env));
        }
        let spec = Value::List(vec![sym(&x), int(m - 1)]);
        let derivative = match crate::builtins::symbolic::builtin_d(&[h, spec], env) {
            Ok(d) => d,
            Err(_) => return Ok(unevaluated("Residue", args)),
        };
        let dlim =
            crate::builtins::symbolicmanip::builtin_limit(&[derivative.clone(), rule.clone()])
                .unwrap_or_else(|_| unevaluated("Limit", &[derivative, rule.clone()]));
        if !limit_is_finite(&dlim, &x) {
            continue;
        }
        let fact = Value::Integer(crate::builtins::math::factorial(m - 1));
        return Ok(eval_value(div(dlim, fact), env));
    }
    Ok(unevaluated("Residue", args))
}

// ── Registration ───────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "SeriesData", builtin_series_data);
    super::register_builtin_env(env, "InverseSeries", builtin_inverse_series);
    super::register_builtin_env(env, "ComposeSeries", builtin_compose_series);
    super::register_builtin_env(env, "FunctionRange", builtin_function_range);
    super::register_builtin_env(env, "Singularities", builtin_singularities);
    super::register_builtin_env(env, "MellinTransform", builtin_mellin_transform);
    super::register_builtin_env(
        env,
        "InverseMellinTransform",
        builtin_inverse_mellin_transform,
    );
    super::register_builtin_env(env, "FourierSinTransform", builtin_fourier_sin_transform);
    super::register_builtin_env(env, "FourierCosTransform", builtin_fourier_cos_transform);
    // Overrides the symbolic `Residue` stub with an evaluating implementation.
    super::register_builtin_env(env, "Residue", builtin_residue_ext);

    for name in [
        "SeriesData",
        "InverseSeries",
        "ComposeSeries",
        "FunctionRange",
        "Singularities",
        "MellinTransform",
        "InverseMellinTransform",
        "FourierSinTransform",
        "FourierCosTransform",
        "Residue",
    ] {
        env.set_attributes(
            name,
            vec!["Protected".to_string(), "ReadProtected".to_string()],
        );
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Env {
        let e = Env::new();
        crate::builtins::register_builtins(&e);
        e
    }

    fn eval_str(s: &str, env: &Env) -> String {
        let toks = crate::lexer::tokenize(s).unwrap();
        let parsed = crate::parser::parse_with_suppress(toks).unwrap();
        let mut last = Value::Null;
        for (e, _) in parsed {
            last = crate::eval::eval(&e, env)
                .unwrap_or_else(|err| Value::Symbol(format!("ERR:{err}")));
        }
        last.to_string()
    }

    fn assert_eval_eq(actual: &str, expected: &str, env: &Env) {
        assert_eq!(eval_str(actual, env), eval_str(expected, env));
    }

    #[test]
    fn residue_simple_and_higher_poles() {
        let env = env();
        assert_eq!(eval_str("Residue[1/x, {x, 0}]", &env), "1");
        assert_eq!(eval_str("Residue[1/x^2, {x, 0}]", &env), "0");
        assert_eq!(eval_str("Residue[1/x^3, {x, 0}]", &env), "0");
        assert_eval_eq("Residue[1/(x - 1), {x, 1}]", "1", &env);
        assert_eval_eq("Residue[1/(x^2 - 1), {x, 1}]", "1/2", &env);
        assert_eq!(eval_str("Residue[x^2, {x, 0}]", &env), "0");
    }

    #[test]
    fn residue_complex_and_transcendental() {
        let env = env();
        // 1/(x^2+1) at x = I  ->  1/(2 I)
        assert_eval_eq("Residue[1/(x^2 + 1), {x, I}]", "1/(2 I)", &env);
        // E^x / x at 0  ->  1
        assert_eq!(eval_str("Residue[E^x/x, {x, 0}]", &env), "1");
        // Cos[x]/x^3 at 0  ->  -1/2
        assert_eval_eq("Residue[Cos[x]/x^3, {x, 0}]", "-1/2", &env);
    }

    #[test]
    fn inverse_series_reversion() {
        let env = env();
        assert_eq!(
            eval_str("InverseSeries[Series[Sin[x], {x, 0, 5}]]", &env),
            "SeriesData[x, 0, {1, 0, 1/6, 0, 3/40}, 1, 6, 1]"
        );
        // Inverse of the identity series is itself.
        assert_eq!(
            eval_str("InverseSeries[Series[x, {x, 0, 4}]]", &env),
            "SeriesData[x, 0, {1}, 1, 5, 1]"
        );
        // Optional target variable.
        assert_eq!(
            eval_str("InverseSeries[Series[Sin[x], {x, 0, 3}], y]", &env),
            "SeriesData[y, 0, {1, 0, 1/6}, 1, 4, 1]"
        );
    }

    #[test]
    fn inverse_series_inapplicable() {
        let env = env();
        // Zero linear term cannot be reverted.
        assert_eq!(
            eval_str("InverseSeries[Series[x^2, {x, 0, 4}]]", &env),
            "InverseSeries[SeriesData[x, 0, {0, 0, 1, 0, 0}, 0, 5, 1]]"
        );
    }

    #[test]
    fn compose_series_matches_exp_sin() {
        let env = env();
        assert_eq!(
            eval_str(
                "ComposeSeries[Series[Exp[x], {x, 0, 5}], Series[Sin[x], {x, 0, 5}]]",
                &env
            ),
            "SeriesData[x, 0, {1, 1, 1/2, 0, -1/8, -1/15}, 0, 6, 1]"
        );
        // Composing with identity leaves the series unchanged.
        assert_eq!(
            eval_str(
                "ComposeSeries[Series[Exp[x], {x, 0, 3}], Series[x, {x, 0, 3}]]",
                &env
            ),
            "SeriesData[x, 0, {1, 1, 1/2, 1/6}, 0, 4, 1]"
        );
    }

    #[test]
    fn compose_series_unevaluated_on_nonzero_constant() {
        let env = env();
        // h with a nonzero constant term is unsupported -> unevaluated.
        assert_eq!(
            eval_str(
                "ComposeSeries[Series[Exp[x], {x, 0, 3}], Series[1 + x, {x, 0, 3}]]",
                &env
            ),
            "ComposeSeries[SeriesData[x, 0, {1, 1, 1/2, 1/6}, 0, 4, 1], SeriesData[x, 0, {1, 1, 0, 0}, 0, 4, 1]]"
        );
    }

    #[test]
    fn series_data_constructor() {
        let env = env();
        assert_eq!(
            eval_str("SeriesData[x, 0, {1, 2, 3}, 0, 3, 1]", &env),
            "SeriesData[x, 0, {1, 2, 3}, 0, 3, 1]"
        );
        // A missing final denominator defaults to 1.
        assert_eq!(
            eval_str("SeriesData[x, 0, {1, 2}, 0, 2]", &env),
            "SeriesData[x, 0, {1, 2}, 0, 2, 1]"
        );
    }

    #[test]
    fn function_range_basic() {
        let env = env();
        assert_eq!(
            eval_str("FunctionRange[x^2, x]", &env),
            "Interval[{0, Infinity}]"
        );
        assert_eq!(
            eval_str("FunctionRange[2 x - x^2, x]", &env),
            "Interval[{Times[-1, Infinity], 1}]"
        );
        assert_eq!(eval_str("FunctionRange[3 x + 1, x]", &env), "Reals");
        assert_eq!(eval_str("FunctionRange[5, x]", &env), "{5}");
        assert_eq!(
            eval_str("FunctionRange[Sin[x], x]", &env),
            "Interval[{-1, 1}]"
        );
        assert_eq!(
            eval_str("FunctionRange[Cos[x], x]", &env),
            "Interval[{-1, 1}]"
        );
    }

    #[test]
    fn function_range_inapplicable() {
        let env = env();
        assert_eq!(
            eval_str("FunctionRange[x^3 + x, x]", &env),
            "FunctionRange[Plus[Power[x, 3], x], x]"
        );
        assert_eq!(
            eval_str("FunctionRange[Gamma[x], x]", &env),
            "FunctionRange[Gamma[x], x]"
        );
    }

    #[test]
    fn singularities_rational_and_branch() {
        let env = env();
        assert_eq!(eval_str("Singularities[1/x, x]", &env), "{0}");
        assert_eval_eq("Singularities[1/(x^2 - 1), x]", "{1, -1}", &env);
        assert_eval_eq("Singularities[1/(x^2 + 1), x]", "{I, -I}", &env);
        assert_eq!(eval_str("Singularities[Log[x], x]", &env), "{0}");
        assert_eq!(eval_str("Singularities[x^2, x]", &env), "{}");
    }

    #[test]
    fn singularities_trig_family() {
        let env = env();
        assert_eq!(
            eval_str("Singularities[Tan[x], x]", &env),
            "{Plus[Divide[Pi, 2], Times[k, Pi]]}"
        );
        assert_eq!(eval_str("Singularities[Cot[x], x]", &env), "{Times[k, Pi]}");
    }

    #[test]
    fn mellin_transform_table_and_integral() {
        let env = env();
        assert_eq!(eval_str("MellinTransform[E^(-x), x, s]", &env), "Gamma[s]");
        assert_eq!(
            eval_str("MellinTransform[E^(-a x), x, s]", &env),
            "Times[Gamma[s], Power[a, Times[-1, s]]]"
        );
        assert_eq!(
            eval_str("MellinTransform[1/(1 + x), x, s]", &env),
            "Times[Pi, Csc[Times[Pi, s]]]"
        );
    }

    #[test]
    fn inverse_mellin_transform_table() {
        let env = env();
        assert_eval_eq("InverseMellinTransform[Gamma[s], s, x]", "E^(-x)", &env);
        assert_eq!(
            eval_str("InverseMellinTransform[Gamma[s]/a^s, s, x]", &env),
            "Power[E, Times[-1, Divide[x, a]]]"
        );
        assert_eval_eq(
            "InverseMellinTransform[Pi Csc[Pi s], s, x]",
            "1/(1 + x)",
            &env,
        );
    }

    #[test]
    fn fourier_sin_cos_transform_table() {
        let env = env();
        assert_eq!(
            eval_str("FourierCosTransform[E^(-a x), x, k]", &env),
            "Times[Sqrt[Divide[2, Pi]], Divide[a, Plus[Power[a, 2], Power[k, 2]]]]"
        );
        assert_eq!(
            eval_str("FourierSinTransform[E^(-a x), x, k]", &env),
            "Times[Sqrt[Divide[2, Pi]], Divide[k, Plus[Power[a, 2], Power[k, 2]]]]"
        );
    }

    #[test]
    fn fourier_sin_cos_inapplicable() {
        let env = env();
        assert_eq!(
            eval_str("FourierSinTransform[Sin[x^2], x, k]", &env),
            "FourierSinTransform[Sin[Power[x, 2]], x, k]"
        );
    }

    #[test]
    fn transforms_are_registered_and_protected() {
        let env = env();
        for name in [
            "InverseSeries",
            "ComposeSeries",
            "FunctionRange",
            "Singularities",
            "MellinTransform",
            "InverseMellinTransform",
            "FourierSinTransform",
            "FourierCosTransform",
            "SeriesData",
            "Residue",
        ] {
            assert!(env.has_attribute(name, "Protected"), "{name} not protected");
        }
    }
}
