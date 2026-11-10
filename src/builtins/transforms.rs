//! integral transforms and numeric calculus: Laplace/Fourier/Z, Dt, NIntegrate/NSum/NProduct, FunctionDomain.
//!
//! Gap-fill module; see the individual builtin docs for semantics.

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::Integer;

// ── Small construction / inspection helpers ─────────────────────────────────

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

fn is_zero(v: &Value) -> bool {
    match v {
        Value::Integer(n) => n.is_zero(),
        Value::Real(r) => r.is_zero(),
        Value::Rational(r) => r.numer().is_zero(),
        _ => false,
    }
}

fn is_one(v: &Value) -> bool {
    matches!(v, Value::Integer(n) if *n == 1)
}

fn is_negative(v: &Value) -> bool {
    match v {
        Value::Integer(n) => n.is_negative(),
        Value::Real(r) => r.is_sign_negative() && !r.is_zero(),
        Value::Rational(r) => r.numer().is_negative(),
        Value::Call { head, args } if head == "Times" => args.iter().any(is_negative),
        _ => false,
    }
}

fn negate(v: &Value) -> Value {
    times(vec![int(-1), v.clone()])
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

fn pow(base: Value, exp: Value) -> Value {
    call("Power", vec![base, exp])
}

fn div(a: Value, b: Value) -> Value {
    call("Divide", vec![a, b])
}

/// Convert a `Value` into an `Expr`, handling the variants `table::value_to_expr`
/// drops (notably `Rational`) at any depth.
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
    match crate::eval::eval(&expr, env) {
        Ok(r) => r,
        Err(_) => v,
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

// ── Expression traversal ────────────────────────────────────────────────────

fn contains_var(v: &Value, name: &str) -> bool {
    match v {
        Value::Symbol(s) => s == name,
        Value::Call { args, .. } => args.iter().any(|a| contains_var(a, name)),
        Value::List(items) | Value::Sequence(items) => items.iter().any(|a| contains_var(a, name)),
        Value::Rule { lhs, rhs, .. } => contains_var(lhs, name) || contains_var(rhs, name),
        _ => false,
    }
}

fn contains_any_symbol(v: &Value) -> bool {
    match v {
        Value::Symbol(s) => !is_const_symbol(s),
        Value::Call { args, .. } => args.iter().any(contains_any_symbol),
        Value::List(items) | Value::Sequence(items) => items.iter().any(contains_any_symbol),
        Value::Rule { lhs, rhs, .. } => contains_any_symbol(lhs) || contains_any_symbol(rhs),
        _ => false,
    }
}

/// True when `v` depends on the differentiation variable (`None` = any symbol).
fn depends_on(v: &Value, xvar: Option<&str>) -> bool {
    match xvar {
        Some(x) => contains_var(v, x),
        None => contains_any_symbol(v),
    }
}

/// Split an expression into multiplicative factors.
fn factors_of(f: &Value) -> Vec<Value> {
    match f {
        Value::Call { head, args } if head == "Times" => args.clone(),
        other => vec![other.clone()],
    }
}

/// If `factor == var^k` with an integer `k >= 0`, return `k`.
fn var_power(factor: &Value, var: &str) -> Option<i64> {
    if matches!(factor, Value::Symbol(s) if s == var) {
        return Some(1);
    }
    if let Value::Call { head, args } = factor
        && head == "Power"
        && args.len() == 2
        && matches!(&args[0], Value::Symbol(s) if s == var)
        && let Value::Integer(k) = &args[1]
        && *k >= 0
    {
        return k.to_i64();
    }
    None
}

/// If `u == a*var` with `a` free of `var`, return `a` (0 when `u` is constant).
fn linear_coeff(u: &Value, var: &str) -> Option<Value> {
    if matches!(u, Value::Symbol(s) if s == var) {
        return Some(int(1));
    }
    if !contains_var(u, var) {
        return Some(int(0));
    }
    match u {
        Value::Call { head, args } if head == "Times" => {
            let mut rest = Vec::new();
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
                    rest.push(f.clone());
                }
            }
            if seen { Some(times(rest)) } else { None }
        }
        _ => None,
    }
}

/// If `factor` is `E^u` or `Exp[u]` with `u` linear in `var`, return the
/// coefficient `a` such that `u == a*var`.
fn exp_shift(factor: &Value, var: &str) -> Option<Value> {
    let u = match factor {
        Value::Call { head, args } if head == "Exp" && args.len() == 1 => &args[0],
        Value::Call { head, args }
            if head == "Power"
                && args.len() == 2
                && matches!(&args[0], Value::Symbol(s) if s == "E") =>
        {
            &args[1]
        }
        _ => return None,
    };
    linear_coeff(u, var)
}

/// If `factor` is `Sin/Cos/Sinh/Cosh[a*var]`, return `(head, a)`.
fn trig_linear(factor: &Value, var: &str) -> Option<(&'static str, Value)> {
    let (head, arg) = match factor {
        Value::Call { head, args } if args.len() == 1 => (head.as_str(), &args[0]),
        _ => return None,
    };
    let kind = match head {
        "Sin" => "Sin",
        "Cos" => "Cos",
        "Sinh" => "Sinh",
        "Cosh" => "Cosh",
        _ => return None,
    };
    let a = linear_coeff(arg, var)?;
    if is_zero(&a) { None } else { Some((kind, a)) }
}

/// Distribute a term-wise transform over a top-level `Plus`.
fn distribute<F>(f: &Value, env: &Env, term: &F) -> Option<Value>
where
    F: Fn(&Value) -> Option<Value>,
{
    if let Value::Call { head, args } = f
        && head == "Plus"
    {
        let mut outs = Vec::with_capacity(args.len());
        for a in args {
            outs.push(term(a)?);
        }
        return Some(eval_value(call("Plus", outs), env));
    }
    term(f)
}

// ── Laplace transform ───────────────────────────────────────────────────────

/// If `factor == var^e` with `e` free of `var`, return `e`.
fn var_exponent(factor: &Value, var: &str) -> Option<Value> {
    if matches!(factor, Value::Symbol(s) if s == var) {
        return Some(int(1));
    }
    if let Value::Call { head, args } = factor
        && head == "Power"
        && args.len() == 2
        && matches!(&args[0], Value::Symbol(s) if s == var)
        && !contains_var(&args[1], var)
    {
        return Some(args[1].clone());
    }
    None
}

/// `n! / (s - shift)^(n+1)` (as an unevaluated `Divide`), generalized so the
/// exponent may be symbolic: `Gamma[n+1] / (s - shift)^(n+1)`.
fn laplace_monomial(n: &Value, shift: &Value, s: &str) -> Value {
    let denom = if is_zero(shift) {
        sym(s)
    } else {
        plus(vec![sym(s), negate(shift)])
    };
    let exp = match n {
        Value::Integer(k) => int(k.to_i64().unwrap_or(0) + 1),
        other => plus(vec![other.clone(), int(1)]),
    };
    div(call("Gamma", vec![exp.clone()]), pow(denom, exp))
}

fn laplace_trig(kind: &str, a: &Value, sbase: &Value) -> Value {
    let a2 = pow(a.clone(), int(2));
    let den = match kind {
        "Sin" | "Cos" => plus(vec![pow(sbase.clone(), int(2)), a2.clone()]),
        _ => plus(vec![pow(sbase.clone(), int(2)), negate(&a2)]),
    };
    match kind {
        "Cos" | "Cosh" => div(sbase.clone(), den),
        _ => div(a.clone(), den),
    }
}

fn laplace_term(f: &Value, t: &str, s: &str, env: &Env) -> Option<Value> {
    let mut n: i64 = 0;
    let mut n_sym: Option<Value> = None;
    let mut shift = int(0);
    let mut have_shift = false;
    let mut delta: Option<Value> = None;
    let mut coeff = int(1);
    let mut trig: Option<(&'static str, Value)> = None;

    for factor in &factors_of(f) {
        if let Value::Call { head, args } = factor
            && head == "DiracDelta"
            && args.len() == 1
        {
            let (m, c) = affine(&args[0], t)?;
            if !is_one(&m) || delta.is_some() {
                return None;
            }
            delta = Some(c);
            continue;
        }
        if let Some(e) = var_exponent(factor, t) {
            match &e {
                Value::Integer(k) if *k >= 0 => n += k.to_i64()?,
                _ => {
                    if n_sym.is_some() || n != 0 {
                        return None;
                    }
                    n_sym = Some(e);
                }
            }
            continue;
        }
        if let Some(a) = exp_shift(factor, t) {
            if is_zero(&a) {
                coeff = times(vec![coeff, factor.clone()]);
            } else if have_shift {
                return None;
            } else {
                shift = a;
                have_shift = true;
            }
            continue;
        }
        if let Some((kind, a)) = trig_linear(factor, t) {
            if trig.is_some() {
                return None;
            }
            trig = Some((kind, a));
            continue;
        }
        if !contains_var(factor, t) {
            coeff = times(vec![coeff, factor.clone()]);
            continue;
        }
        return None;
    }

    let sbase = if is_zero(&shift) {
        sym(s)
    } else {
        plus(vec![sym(s), negate(&shift)])
    };

    // `DiracDelta[t - c]` contributes `E^(c*s)` (with `s` shifted by `shift`).
    if let Some(c) = delta {
        let base = if is_zero(&c) {
            int(1)
        } else {
            pow(sym("E"), times(vec![c, sbase]))
        };
        return Some(eval_value(times(vec![coeff, base]), env));
    }

    let base = match trig {
        Some((kind, a)) => {
            if n != 0 || n_sym.is_some() {
                return None;
            }
            laplace_trig(kind, &a, &sbase)
        }
        None => match n_sym {
            Some(e) => laplace_monomial(&e, &shift, s),
            None => laplace_monomial(&int(n), &shift, s),
        },
    };

    Some(eval_value(times(vec![coeff, base]), env))
}

/// Laplace transform with the environment needed for simplification.
fn laplace(f: &Value, t: &str, s: &str, env: &Env) -> Value {
    let f = normalize(f);
    match distribute(&f, env, &|x| laplace_term(x, t, s, env)) {
        Some(r) => eval_value(r, env),
        None => call("LaplaceTransform", vec![f.clone(), sym(t), sym(s)]),
    }
}

/// Interpret a numeric exponent (`Integer` or `Rational`) as a rational.
fn as_exponent_ratio(v: &Value) -> Option<rug::Rational> {
    match v {
        Value::Integer(i) => Some(rug::Rational::from(i.clone())),
        Value::Rational(r) => Some((**r).clone()),
        _ => None,
    }
}

/// Render a rational back to an `Integer` (when integral) or `Rational`.
fn ratio_to_value(r: rug::Rational) -> Value {
    if r.denom() == &Integer::from(1) {
        Value::Integer(r.numer().clone())
    } else {
        Value::Rational(Box::new(r))
    }
}

/// Build a `Power`, flattening `Power[Power[x, m], n]` when both exponents are
/// numeric (integer or rational).
fn mk_pow(base: Value, exp: Value) -> Value {
    if let Value::Call { head, args } = &base
        && head == "Power"
        && args.len() == 2
        && let Some(m) = as_exponent_ratio(&args[1])
        && let Some(n) = as_exponent_ratio(&exp)
    {
        return pow(args[0].clone(), ratio_to_value(m * n));
    }
    pow(base, exp)
}

/// Rewrite `Divide[a, b]` as `Times[a, Power[b, -1]]` throughout an expression
/// so the transform matchers can reason about numerators and denominators.
fn normalize(v: &Value) -> Value {
    match v {
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => {
            let num = normalize(&args[0]);
            let den = normalize(&args[1]);
            times(vec![num, mk_pow(den, int(-1))])
        }
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            let base = normalize(&args[0]);
            let exp = normalize(&args[1]);
            mk_pow(base, exp)
        }
        Value::Call { head, args } if head == "Sqrt" && args.len() == 1 => mk_pow(
            normalize(&args[0]),
            Value::Rational(Box::new(rug::Rational::from((1, 2)))),
        ),
        Value::Call { head, args } => Value::Call {
            head: head.clone(),
            args: args.iter().map(normalize).collect(),
        },
        Value::List(items) => Value::List(items.iter().map(normalize).collect()),
        _ => v.clone(),
    }
}

// ── Inverse Laplace transform ────────────────────────────────────────────────

/// Decompose `e` as `m*var + c` with `m, c` free of `var`.
fn affine(e: &Value, var: &str) -> Option<(Value, Value)> {
    if matches!(e, Value::Symbol(s) if s == var) {
        return Some((int(1), int(0)));
    }
    if !contains_var(e, var) {
        return Some((int(0), e.clone()));
    }
    match e {
        Value::Call { head, args } if head == "Plus" => {
            let mut m = int(0);
            let mut c = int(0);
            for a in args {
                let (am, ac) = affine(a, var)?;
                m = plus(vec![m, am]);
                c = plus(vec![c, ac]);
            }
            Some((m, c))
        }
        Value::Call { head, args } if head == "Times" => {
            let nonconst: Vec<&Value> = args.iter().filter(|a| contains_var(a, var)).collect();
            if nonconst.len() != 1 {
                return None;
            }
            let (am, ac) = affine(nonconst[0], var)?;
            let consts: Vec<Value> = args
                .iter()
                .filter(|a| !contains_var(a, var))
                .cloned()
                .collect();
            let cs = times(consts);
            Some((times(vec![am, cs.clone()]), times(vec![ac, cs])))
        }
        Value::Call { head, args }
            if head == "Divide" && args.len() == 2 && !contains_var(&args[1], var) =>
        {
            let (am, ac) = affine(&args[0], var)?;
            Some((div(am, args[1].clone()), div(ac, args[1].clone())))
        }
        _ => None,
    }
}

fn is_var_squared(v: &Value, var: &str) -> bool {
    matches!(
        v,
        Value::Call { head, args }
            if head == "Power"
                && args.len() == 2
                && matches!(&args[0], Value::Symbol(s) if s == var)
                && matches!(&args[1], Value::Integer(k) if *k == 2)
    )
}

/// If `d == var^2 + q` with `q` free of `var`, return `q`.
fn quad_const(d: &Value, var: &str) -> Option<Value> {
    if !contains_var(d, var) {
        return None;
    }
    match d {
        Value::Call { head, args } if head == "Plus" => {
            let mut q = int(0);
            let mut seen = false;
            for a in args {
                if !contains_var(a, var) {
                    q = plus(vec![q, a.clone()]);
                } else if is_var_squared(a, var) {
                    if seen {
                        return None;
                    }
                    seen = true;
                } else {
                    return None;
                }
            }
            if seen { Some(q) } else { None }
        }
        _ if is_var_squared(d, var) => Some(int(0)),
        _ => None,
    }
}

fn sqrt_of_square(a2: &Value, env: &Env) -> Value {
    let a2 = eval_value(a2.clone(), env);
    if let Value::Call { head, args } = &a2
        && head == "Power"
        && args.len() == 2
        && matches!(&args[1], Value::Integer(k) if *k == 2)
    {
        return args[0].clone();
    }
    eval_value(call("Sqrt", vec![a2]), env)
}

/// If `n == c*var`, return `c`.
fn scalar_multiple_of_var(n: &Value, var: &str) -> Option<Value> {
    if matches!(n, Value::Symbol(s) if s == var) {
        return Some(int(1));
    }
    match n {
        Value::Call { head, args } if head == "Times" => {
            let mut rest = Vec::new();
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
                    rest.push(f.clone());
                }
            }
            if seen { Some(times(rest)) } else { None }
        }
        _ => None,
    }
}

fn inverse_laplace_term(f: &Value, s: &str, t: &str, env: &Env) -> Option<Value> {
    let mut denoms: Vec<(Value, Value)> = Vec::new();
    let mut numerator = int(1);
    for factor in &factors_of(f) {
        if let Value::Call { head, args } = factor
            && head == "Power"
            && args.len() == 2
            && is_negative(&args[1])
        {
            denoms.push((args[0].clone(), eval_value(negate(&args[1]), env)));
            continue;
        }
        numerator = times(vec![numerator, factor.clone()]);
    }

    if denoms.is_empty() {
        if !contains_var(f, s) {
            return Some(times(vec![f.clone(), call("DiracDelta", vec![sym(t)])]));
        }
        return None;
    }
    if denoms.len() != 1 {
        return None;
    }
    let (d, nu) = denoms.pop().unwrap();

    if let Some((m, c0)) = affine(&d, s)
        && !is_zero(&m)
        && !contains_var(&numerator, s)
    {
        let a = div(negate(&c0), m.clone());
        let scale = div(numerator, pow(m, nu.clone()));
        let tap = div(
            pow(sym(t), plus(vec![nu.clone(), int(-1)])),
            call("Gamma", vec![nu]),
        );
        let ex = if is_zero(&a) {
            int(1)
        } else {
            pow(sym("E"), times(vec![a, sym(t)]))
        };
        return Some(eval_value(times(vec![scale, tap, ex]), env));
    }

    if is_one(&nu)
        && let Some(q) = quad_const(&d, s)
    {
        let (a2, hyper) = if is_negative(&q) {
            (negate(&q), true)
        } else {
            (q, false)
        };
        let a = sqrt_of_square(&a2, env);
        let arg = times(vec![a.clone(), sym(t)]);
        if !contains_var(&numerator, s) {
            let fn_head = if hyper { "Sinh" } else { "Sin" };
            let wave = call(fn_head, vec![arg]);
            if numerator.struct_eq(&a) {
                return Some(eval_value(wave, env));
            }
            return Some(eval_value(times(vec![numerator, div(wave, a)]), env));
        }
        if let Some(c1) = scalar_multiple_of_var(&numerator, s) {
            let fn_head = if hyper { "Cosh" } else { "Cos" };
            let wave = call(fn_head, vec![arg]);
            return Some(eval_value(times(vec![c1, wave]), env));
        }
    }

    None
}

fn inverse_laplace(f: &Value, s: &str, t: &str, env: &Env) -> Value {
    let f = normalize(f);
    match distribute(&f, env, &|x| inverse_laplace_term(x, s, t, env)) {
        Some(r) => eval_value(r, env),
        None => call("InverseLaplaceTransform", vec![f.clone(), sym(s), sym(t)]),
    }
}

// ── Fourier transform (default {0, 1} parameters) ───────────────────────────

fn dirac(x: Value) -> Value {
    call("DiracDelta", vec![x])
}

/// If `u == c*var^2`, return `c`.
///
/// Also accepts square-of-linear forms such as `(-t)^2` or `(2 a t)^2`, which
/// arise from parsing `E^(-t^2)`.
fn squared_coeff(u: &Value, var: &str) -> Option<Value> {
    if is_var_squared(u, var) {
        return Some(int(1));
    }
    match u {
        Value::Call { head, args } if head == "Times" => {
            let mut rest = Vec::new();
            let mut seen = false;
            for f in args {
                if !contains_var(f, var) {
                    rest.push(f.clone());
                } else {
                    let c = squared_coeff(f, var)?;
                    if seen {
                        return None;
                    }
                    seen = true;
                    rest.push(c);
                }
            }
            if seen { Some(times(rest)) } else { None }
        }
        Value::Call { head, args }
            if head == "Power"
                && args.len() == 2
                && matches!(&args[1], Value::Integer(k) if *k == 2) =>
        {
            // base must be linear in `var`: base = c*var.
            let c = linear_coeff(&args[0], var)?;
            if is_zero(&c) {
                return None;
            }
            Some(pow(c, int(2)))
        }
        _ => None,
    }
}

/// If `u == I*a*var` (with `a` free of `var`), return `a`.
///
/// The `I` may be nested inside a coefficient (`E^(I a t)` parses as
/// `E^(Times[Times[I, a], t])`), so extract the linear coefficient first and
/// then strip a top-level `I` factor.
fn i_linear_coeff(u: &Value, var: &str) -> Option<Value> {
    let c = linear_coeff(u, var)?;
    if is_zero(&c) {
        return None;
    }
    strip_i(&c)
}

/// If `c == I*a` (with `I` as a multiplicative factor), return `a`.
fn strip_i(c: &Value) -> Option<Value> {
    match c {
        Value::Symbol(s) if s == "I" => Some(int(1)),
        Value::Call { head, args } if head == "Times" => {
            let mut rest = Vec::new();
            let mut saw_i = false;
            for f in args {
                if matches!(f, Value::Symbol(s) if s == "I") {
                    saw_i = true;
                } else {
                    rest.push(f.clone());
                }
            }
            if saw_i { Some(times(rest)) } else { None }
        }
        _ => None,
    }
}

fn sqrt_half(x: &Value) -> Value {
    div(int(1), call("Sqrt", vec![x.clone()]))
}

fn gauss_result(a: &Value, wval: &Value) -> Value {
    // E^(-a t^2) -> 1/Sqrt[2a] E^(-w^2/(4a))
    let amp = sqrt_half(&times(vec![int(2), a.clone()]));
    let expo = negate(&div(
        pow(wval.clone(), int(2)),
        times(vec![int(4), a.clone()]),
    ));
    times(vec![amp, pow(sym("E"), expo)])
}

fn super_sqrt_two_pi() -> Value {
    call("Sqrt", vec![times(vec![int(2), sym("Pi")])])
}

fn fourier_term(f: &Value, t: &str, wval: &Value, env: &Env) -> Option<Value> {
    let mut coeff = int(1);
    let mut special: Option<Value> = None;

    for factor in &factors_of(f) {
        // E^(±a*var^2) gaussian or E^(I a var)
        if let Some(u) = exp_argument(factor) {
            if let Some(c) = squared_coeff(&u, t) {
                let a = negate(&c);
                set_special(&mut special, gauss_result(&a, wval))?;
                continue;
            }
            if let Some(a) = i_linear_coeff(&u, t) {
                set_special(
                    &mut special,
                    times(vec![
                        super_sqrt_two_pi(),
                        dirac(plus(vec![wval.clone(), a])),
                    ]),
                )?;
                continue;
            }
        }
        if let Value::Call { head, args } = factor
            && head == "DiracDelta"
            && args.len() == 1
            && let Some((m, c)) = affine(&args[0], t)
            && is_one(&m)
        {
            // ∫ δ(t-c) E^(I w t) dt = E^(I w c); scaled by 1/Sqrt[2π].
            let amp = sqrt_half(&times(vec![int(2), sym("Pi")]));
            let ex = if is_zero(&c) {
                amp
            } else {
                times(vec![
                    amp,
                    pow(sym("E"), times(vec![sym("I"), c, wval.clone()])),
                ])
            };
            set_special(&mut special, ex)?;
            continue;
        }
        if let Some((kind, a)) = trig_linear(factor, t) {
            let amp = call("Sqrt", vec![div(sym("Pi"), int(2))]);
            let d1 = dirac(plus(vec![wval.clone(), negate(&a)]));
            let d2 = dirac(plus(vec![wval.clone(), a.clone()]));
            let body = match kind {
                "Cos" => times(vec![amp, plus(vec![d1, d2])]),
                "Sin" => times(vec![sym("I"), amp, plus(vec![d1, negate(&d2)])]),
                _ => return None,
            };
            set_special(&mut special, body)?;
            continue;
        }
        if !contains_var(factor, t) {
            coeff = times(vec![coeff, factor.clone()]);
            continue;
        }
        return None;
    }

    let base = match special {
        Some(s) => s,
        None => times(vec![super_sqrt_two_pi(), dirac(wval.clone())]),
    };
    Some(eval_value(times(vec![coeff, base]), env))
}

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

fn set_special(slot: &mut Option<Value>, v: Value) -> Option<()> {
    if slot.is_some() {
        None
    } else {
        *slot = Some(v);
        Some(())
    }
}

// ── Z transform ─────────────────────────────────────────────────────────────

fn geometric_z(a: &Value, k: i64, z: &str) -> Option<Value> {
    let zz = sym(z);
    let denom = plus(vec![zz.clone(), negate(a)]);
    match k {
        0 => Some(div(zz, denom)),
        1 => Some(times(vec![a.clone(), div(zz, pow(denom, int(2)))])),
        2 => Some(times(vec![
            a.clone(),
            div(
                times(vec![zz.clone(), plus(vec![zz, a.clone()])]),
                pow(denom, int(3)),
            ),
        ])),
        _ => None,
    }
}

fn eulerian(n: usize, k: usize) -> i64 {
    // A(n, k) for k = 0..n-1, with A(0, 0) = 1.
    let mut prev = vec![1i64];
    for i in 1..=n {
        let mut cur = vec![0i64; i];
        for (j, slot) in cur.iter_mut().enumerate() {
            let left = if j >= 1 {
                (i - j) as i64 * prev.get(j - 1).copied().unwrap_or(0)
            } else {
                0
            };
            let right = (j as i64 + 1) * prev.get(j).copied().unwrap_or(0);
            *slot = left + right;
        }
        prev = cur;
    }
    if n == 0 {
        if k == 0 { 1 } else { 0 }
    } else {
        prev.get(k).copied().unwrap_or(0)
    }
}

fn power_z(k: i64, z: &str) -> Value {
    let zz = sym(z);
    let denom = pow(plus(vec![zz.clone(), int(-1)]), int(k + 1));
    let mut terms = Vec::new();
    for j in 0..=k {
        let e = eulerian(k as usize, j as usize);
        if e == 0 {
            continue;
        }
        terms.push(times(vec![int(e), pow(zz.clone(), int(j + 1))]));
    }
    div(plus(terms), denom)
}

fn z_trig(fn_head: &str, a: &Value, z: &str) -> Value {
    let zz = sym(z);
    let ca = call(fn_head, vec![a.clone()]);
    let den = plus(vec![
        pow(zz.clone(), int(2)),
        negate(&times(vec![int(2), zz.clone(), ca.clone()])),
        int(1),
    ]);
    if fn_head == "Cos" || fn_head == "Cosh" {
        div(times(vec![zz.clone(), plus(vec![zz, negate(&ca)])]), den)
    } else {
        div(times(vec![zz, ca]), den)
    }
}

fn z_term(f: &Value, n: &str, z: &str, env: &Env) -> Option<Value> {
    let mut coeff = int(1);
    let mut k: i64 = 0;
    let mut geom: Option<Value> = None;
    let mut trig: Option<(&'static str, Value)> = None;
    for factor in &factors_of(f) {
        if let Some(p) = var_power(factor, n) {
            k += p;
            continue;
        }
        if let Value::Call { head, args } = factor
            && head == "Power"
            && args.len() == 2
            && matches!(&args[1], Value::Symbol(s) if s == n)
            && !contains_var(&args[0], n)
        {
            if geom.is_some() {
                return None;
            }
            geom = Some(args[0].clone());
            continue;
        }
        if let Some((kind, a)) = trig_linear(factor, n) {
            if trig.is_some() {
                return None;
            }
            trig = Some((kind, a));
            continue;
        }
        if !contains_var(factor, n) {
            coeff = times(vec![coeff, factor.clone()]);
            continue;
        }
        return None;
    }

    let base = if let Some((kind, a)) = trig {
        if k != 0 || geom.is_some() {
            return None;
        }
        z_trig(kind, &a, z)
    } else {
        match geom {
            Some(a) => geometric_z(&a, k, z)?,
            None => power_z(k, z),
        }
    };
    Some(eval_value(times(vec![coeff, base]), env))
}

fn ztransform(f: &Value, n: &str, z: &str, env: &Env) -> Value {
    let f = normalize(f);
    match distribute(&f, env, &|x| z_term(x, n, z, env)) {
        Some(r) => eval_value(r, env),
        None => call("ZTransform", vec![f.clone(), sym(n), sym(z)]),
    }
}

fn inverse_z_term(f: &Value, z: &str, n: &str, env: &Env) -> Option<Value> {
    let mut denoms: Vec<(Value, i64)> = Vec::new();
    let mut numerator = int(1);
    for factor in &factors_of(f) {
        if let Value::Call { head, args } = factor
            && head == "Power"
            && args.len() == 2
            && let Value::Integer(k) = &args[1]
            && k.is_negative()
        {
            denoms.push((args[0].clone(), -k.to_i64()?));
            continue;
        }
        numerator = times(vec![numerator, factor.clone()]);
    }

    if denoms.is_empty() {
        if !contains_var(f, z) {
            return Some(times(vec![f.clone(), call("KroneckerDelta", vec![sym(n)])]));
        }
        return None;
    }
    if denoms.len() != 1 {
        return None;
    }
    let (d, k) = denoms.pop().unwrap();

    // numerator must be a numeric coefficient times z (or a pure constant)
    let (c, has_z) = strip_z(&numerator, z)?;
    let (m, c0) = affine(&d, z)?;
    if !is_one(&m) {
        return None;
    }
    let a = negate(&c0);
    let shift = if has_z { 0 } else { -1 };
    let base = match k {
        1 => pow(a.clone(), plus(vec![sym(n), int(shift)])),
        2 => times(vec![
            sym(n),
            pow(a.clone(), plus(vec![sym(n), int(shift - 1)])),
        ]),
        3 => times(vec![
            div(times(vec![sym(n), plus(vec![sym(n), int(-1)])]), int(2)),
            pow(a.clone(), plus(vec![sym(n), int(shift - 2)])),
        ]),
        _ => return None,
    };
    Some(eval_value(times(vec![c, base]), env))
}

/// If `v == c*z`, return `(c, true)`; if `v == c`, return `(c, false)`.
fn strip_z(v: &Value, z: &str) -> Option<(Value, bool)> {
    if matches!(v, Value::Symbol(s) if s == z) {
        return Some((int(1), true));
    }
    if !contains_var(v, z) {
        return Some((v.clone(), false));
    }
    match v {
        Value::Call { head, args } if head == "Times" => {
            let mut rest = Vec::new();
            let mut seen = false;
            for f in args {
                if matches!(f, Value::Symbol(s) if s == z) {
                    if seen {
                        return None;
                    }
                    seen = true;
                } else if contains_var(f, z) {
                    return None;
                } else {
                    rest.push(f.clone());
                }
            }
            if seen {
                Some((times(rest), true))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn inverse_ztransform(f: &Value, z: &str, n: &str, env: &Env) -> Value {
    let f = normalize(f);
    match distribute(&f, env, &|x| inverse_z_term(x, z, n, env)) {
        Some(r) => eval_value(r, env),
        None => call("InverseZTransform", vec![f.clone(), sym(z), sym(n)]),
    }
}

// ── Dt: total derivative ────────────────────────────────────────────────────

fn dhead(expr: &Value, xvar: Option<&str>) -> Value {
    match xvar {
        Some(x) => call("Dt", vec![expr.clone(), sym(x)]),
        None => call("Dt", vec![expr.clone()]),
    }
}

fn dt(expr: &Value, xvar: Option<&str>, env: &Env) -> Value {
    match expr {
        Value::Integer(_)
        | Value::Real(_)
        | Value::Rational(_)
        | Value::Complex { .. }
        | Value::Bool(_)
        | Value::Str(_)
        | Value::Null => int(0),
        Value::Symbol(s) => {
            if is_const_symbol(s) {
                int(0)
            } else if xvar == Some(s.as_str()) {
                int(1)
            } else {
                dhead(expr, xvar)
            }
        }
        Value::List(items) => Value::List(items.iter().map(|a| dt(a, xvar, env)).collect()),
        Value::Call { head, args } => dt_call(head, args, expr, xvar, env),
        _ => dhead(expr, xvar),
    }
}

fn dt_call(head: &str, args: &[Value], whole: &Value, xvar: Option<&str>, env: &Env) -> Value {
    match head {
        "Plus" => {
            let terms: Vec<Value> = args.iter().map(|a| dt(a, xvar, env)).collect();
            eval_value(plus(terms), env)
        }
        "Times" => {
            if args.is_empty() {
                return int(0);
            }
            let mut terms = Vec::new();
            for i in 0..args.len() {
                let mut factors = Vec::with_capacity(args.len());
                for (j, a) in args.iter().enumerate() {
                    factors.push(if i == j { dt(a, xvar, env) } else { a.clone() });
                }
                terms.push(times(factors));
            }
            eval_value(plus(terms), env)
        }
        "Power" if args.len() == 2 => {
            let (u, v) = (&args[0], &args[1]);
            if matches!(u, Value::Symbol(s) if s == "E") {
                // E^u — avoid the Log[E] artifact of the general rule.
                eval_value(times(vec![whole.clone(), dt(v, xvar, env)]), env)
            } else if !depends_on(v, xvar) {
                let inner = dt(u, xvar, env);
                eval_value(
                    times(vec![
                        v.clone(),
                        pow(u.clone(), plus(vec![v.clone(), int(-1)])),
                        inner,
                    ]),
                    env,
                )
            } else {
                // u^v * d(v Log[u])
                let logu = call("Log", vec![u.clone()]);
                let exponent = times(vec![v.clone(), logu]);
                let inner = dt(&exponent, xvar, env);
                eval_value(times(vec![whole.clone(), inner]), env)
            }
        }
        "Exp" if args.len() == 1 => {
            eval_value(times(vec![whole.clone(), dt(&args[0], xvar, env)]), env)
        }
        "Log" if args.len() == 1 => eval_value(div(dt(&args[0], xvar, env), args[0].clone()), env),
        "Log" if args.len() == 2 => {
            let inner = div(dt(&args[1], xvar, env), args[1].clone());
            eval_value(div(inner, call("Log", vec![args[0].clone()])), env)
        }
        "Sin" => unary_chain(whole, args, xvar, env, "Cos"),
        "Cos" => {
            let inner = dt(&args[0], xvar, env);
            let neg = times(vec![int(-1), call("Sin", vec![args[0].clone()]), inner]);
            eval_value(neg, env)
        }
        "Tan" => {
            let inner = dt(&args[0], xvar, env);
            eval_value(
                div(inner, pow(call("Cos", vec![args[0].clone()]), int(2))),
                env,
            )
        }
        "Sinh" => unary_chain(whole, args, xvar, env, "Cosh"),
        "Cosh" => unary_chain(whole, args, xvar, env, "Sinh"),
        "Tanh" => {
            let inner = dt(&args[0], xvar, env);
            eval_value(
                div(inner, pow(call("Cosh", vec![args[0].clone()]), int(2))),
                env,
            )
        }
        "Sqrt" if args.len() == 1 => {
            let inner = dt(&args[0], xvar, env);
            eval_value(div(inner, times(vec![int(2), whole.clone()])), env)
        }
        "ArcSin" => arc_chain(args, xvar, env, true),
        "ArcCos" => arc_chain(args, xvar, env, false),
        "ArcTan" => {
            let inner = dt(&args[0], xvar, env);
            eval_value(
                div(inner, plus(vec![int(1), pow(args[0].clone(), int(2))])),
                env,
            )
        }
        _ => dhead(whole, xvar),
    }
}

fn unary_chain(
    _whole: &Value,
    args: &[Value],
    xvar: Option<&str>,
    env: &Env,
    target: &str,
) -> Value {
    let inner = dt(&args[0], xvar, env);
    eval_value(times(vec![call(target, vec![args[0].clone()]), inner]), env)
}

fn arc_chain(args: &[Value], xvar: Option<&str>, env: &Env, positive: bool) -> Value {
    let inner = dt(&args[0], xvar, env);
    let denom = call(
        "Sqrt",
        vec![plus(vec![int(1), negate(&pow(args[0].clone(), int(2)))])],
    );
    let body = div(inner, denom);
    if positive {
        eval_value(body, env)
    } else {
        eval_value(times(vec![int(-1), body]), env)
    }
}

// ── FunctionDomain ──────────────────────────────────────────────────────────

fn ge_zero(u: Value) -> Value {
    call("GreaterEqual", vec![u, int(0)])
}

fn gt_zero(u: Value) -> Value {
    call("Greater", vec![u, int(0)])
}

fn ne_zero(u: Value) -> Value {
    call(
        "Alternatives",
        vec![call("Less", vec![u.clone(), int(0)]), gt_zero(u)],
    )
}

fn collect_domain(f: &Value, x: &str, acc: &mut Vec<Value>) {
    match f {
        Value::Call { head, args } => match head.as_str() {
            "Plus" | "Times" => {
                for a in args {
                    collect_domain(a, x, acc);
                }
            }
            "Divide" if args.len() == 2 => {
                collect_domain(&args[0], x, acc);
                collect_domain(&args[1], x, acc);
                if contains_var(&args[1], x) {
                    acc.push(ne_zero(args[1].clone()));
                }
            }
            "Power" if args.len() == 2 => {
                collect_domain(&args[0], x, acc);
                match &args[1] {
                    Value::Integer(k) => {
                        if k.is_negative() && contains_var(&args[0], x) {
                            acc.push(ne_zero(args[0].clone()));
                        }
                        if k.is_even() && k.is_negative() {
                            acc.push(ge_zero(args[0].clone()));
                        }
                    }
                    Value::Rational(r) => {
                        if contains_var(&args[0], x) {
                            acc.push(ne_zero(args[0].clone()));
                        }
                        if r.denom().to_i32().unwrap_or(0) != 1 && r.denom().is_even() {
                            acc.push(ge_zero(args[0].clone()));
                        }
                    }
                    _ => {}
                }
            }
            "Sqrt" if args.len() == 1 => {
                collect_domain(&args[0], x, acc);
                acc.push(ge_zero(args[0].clone()));
            }
            "Log" => {
                for a in args {
                    collect_domain(a, x, acc);
                }
                if let Some(a) = args.first() {
                    acc.push(gt_zero(a.clone()));
                }
            }
            "Exp" | "Sin" | "Cos" | "Tan" | "Sinh" | "Cosh" | "Tanh" | "ArcSin" | "ArcCos"
            | "ArcTan" | "Abs" | "Sign" => {
                for a in args {
                    collect_domain(a, x, acc);
                }
            }
            _ => {
                for a in args {
                    collect_domain(a, x, acc);
                }
            }
        },
        Value::List(items) => {
            for a in items {
                collect_domain(a, x, acc);
            }
        }
        _ => {}
    }
}

fn function_domain(f: &Value, x: &str, env: &Env) -> Value {
    if !contains_var(f, x) {
        return sym("Reals");
    }
    let mut acc = Vec::new();
    collect_domain(f, x, &mut acc);
    let mut constraints: Vec<Value> = Vec::new();
    for c in acc {
        let ev = eval_value(c, env);
        match &ev {
            Value::Bool(true) => {}
            Value::Bool(false) => return Value::Bool(false),
            _ => constraints.push(ev),
        }
    }
    match constraints.len() {
        0 => sym("Reals"),
        1 => constraints.pop().unwrap(),
        _ => call("And", constraints),
    }
}

// ── Numeric calculus ────────────────────────────────────────────────────────

fn parse_spec(spec: &Value, name: &str) -> Result<(String, Value, Value, f64), EvalError> {
    let items = match spec {
        Value::List(items) => items,
        other => return Err(type_err("List iterator specification", other)),
    };
    if items.len() < 2 || items.len() > 4 {
        return Err(EvalError::Error(format!(
            "{name}: iterator spec must have 2-4 elements"
        )));
    }
    let var = match &items[0] {
        Value::Symbol(s) => s.clone(),
        other => return Err(type_err("Symbol iterator variable", other)),
    };
    let (lo, hi) = if items.len() == 2 {
        (int(1), items[1].clone())
    } else {
        (items[1].clone(), items[2].clone())
    };
    let step = if items.len() == 4 {
        match &items[3] {
            Value::Integer(n) => n.to_f64(),
            Value::Real(r) => r.to_f64(),
            other => return Err(type_err("numeric step", other)),
        }
    } else {
        1.0
    };
    Ok((var, lo, hi, step))
}

fn eval_at_num(f: &Value, var: &str, x: f64, env: &Env) -> Result<f64, EvalError> {
    let child = env.child();
    let binding = if x.fract() == 0.0 && x.abs() < 1e15 {
        Value::Integer(Integer::from(x as i64))
    } else {
        super::real(x)
    };
    child.set(var.to_string(), binding);
    let expr = value_to_expr_local(f);
    let val = crate::eval::eval(&expr, &child)?;
    if let Some(y) = super::to_f64(&val) {
        return Ok(y);
    }
    let n = eval_value(call("N", vec![val.clone()]), &child);
    super::to_f64(&n).ok_or_else(|| EvalError::Error(format!("{f} is not numeric at {var}={x}")))
}

fn num_to_value(x: f64) -> Value {
    if x.is_finite() && x.fract() == 0.0 && x.abs() < 1e15 {
        int(x as i64)
    } else {
        super::real(x)
    }
}

fn eval_bound(v: &Value, env: &Env) -> Result<f64, EvalError> {
    let ev = eval_value(v.clone(), env);
    if let Some(x) = super::to_f64(&ev) {
        return Ok(x);
    }
    let n = eval_value(call("N", vec![ev]), env);
    super::to_f64(&n).ok_or_else(|| type_err("numeric bound", &n))
}

fn is_infinity(v: &Value) -> bool {
    matches!(v, Value::Symbol(s) if s == "Infinity")
}

fn builtin_nsum(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "NSum requires at least 2 arguments".to_string(),
        ));
    }
    let f = unwrap(&args[0]);
    let (var, lo, hi, step) = parse_spec(&unwrap(&args[1]), "NSum")?;
    let lo = eval_bound(&lo, env)?;
    let sum = if is_infinity(&unwrap(&hi)) {
        let mut total = 0.0f64;
        let mut i = lo;
        let mut count = 0usize;
        loop {
            let term = eval_at_num(&f, &var, i, env)?;
            total += term;
            count += 1;
            if count > 4 && term.abs() < 1e-13 * (1.0 + total.abs()) {
                break;
            }
            if count > 5_000_000 {
                break;
            }
            i += step;
        }
        total
    } else {
        let hi = eval_bound(&hi, env)?;
        let mut total = 0.0f64;
        let mut i = lo;
        let mut guard = 0usize;
        while (step > 0.0 && i <= hi + 1e-12) || (step < 0.0 && i >= hi - 1e-12) {
            total += eval_at_num(&f, &var, i, env)?;
            i += step;
            guard += 1;
            if guard > 10_000_000 {
                break;
            }
        }
        total
    };
    Ok(num_to_value(sum))
}
fn builtin_nproduct(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "NProduct requires at least 2 arguments".to_string(),
        ));
    }
    let f = unwrap(&args[0]);
    let (var, lo, hi, step) = parse_spec(&unwrap(&args[1]), "NProduct")?;
    let lo = eval_bound(&lo, env)?;
    let product = if is_infinity(&unwrap(&hi)) {
        let mut total = 1.0f64;
        let mut i = lo;
        let mut count = 0usize;
        loop {
            let term = eval_at_num(&f, &var, i, env)?;
            total *= term;
            count += 1;
            if count > 4 && (term - 1.0).abs() < 1e-13 {
                break;
            }
            if count > 5_000_000 {
                break;
            }
            i += step;
        }
        total
    } else {
        let hi = eval_bound(&hi, env)?;
        let mut total = 1.0f64;
        let mut i = lo;
        let mut guard = 0usize;
        while (step > 0.0 && i <= hi + 1e-12) || (step < 0.0 && i >= hi - 1e-12) {
            total *= eval_at_num(&f, &var, i, env)?;
            i += step;
            guard += 1;
            if guard > 10_000_000 {
                break;
            }
        }
        total
    };
    Ok(num_to_value(product))
}

fn builtin_nintegrate(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "NIntegrate requires at least 2 arguments".to_string(),
        ));
    }
    let f = unwrap(&args[0]);
    let spec = unwrap(&args[1]);
    let items = match &spec {
        Value::List(items) if items.len() == 3 => items,
        other => return Err(type_err("List {x, a, b} integration limits", other)),
    };
    let var = match &items[0] {
        Value::Symbol(s) => s.clone(),
        other => return Err(type_err("Symbol integration variable", other)),
    };
    let a = eval_bound(&items[1], env)?;
    let b = eval_bound(&items[2], env)?;
    let value = adaptive_simpson(&f, &var, a, b, env)?;
    Ok(super::real(value))
}

const NINT_TOL: f64 = 1e-10;
const NINT_MAX_DEPTH: usize = 50;

fn adaptive_simpson(f: &Value, var: &str, a: f64, b: f64, env: &Env) -> Result<f64, EvalError> {
    let fa = eval_at_num(f, var, a, env)?;
    let fb = eval_at_num(f, var, b, env)?;
    let m = 0.5 * (a + b);
    let fm = eval_at_num(f, var, m, env)?;
    let whole = simpson(a, b, fa, fm, fb);
    simpson_rec(f, var, a, b, fa, fm, fb, whole, 0, env)
}

#[allow(clippy::too_many_arguments)]
fn simpson_rec(
    f: &Value,
    var: &str,
    a: f64,
    b: f64,
    fa: f64,
    fm: f64,
    fb: f64,
    whole: f64,
    depth: usize,
    env: &Env,
) -> Result<f64, EvalError> {
    let m = 0.5 * (a + b);
    let lm = 0.5 * (a + m);
    let rm = 0.5 * (m + b);
    let flm = eval_at_num(f, var, lm, env)?;
    let frm = eval_at_num(f, var, rm, env)?;
    let left = simpson(a, m, fa, flm, fm);
    let right = simpson(m, b, fm, frm, fb);
    let delta = left + right - whole;
    if depth >= NINT_MAX_DEPTH || delta.abs() <= 15.0 * NINT_TOL {
        return Ok(left + right + delta / 15.0);
    }
    let l = simpson_rec(f, var, a, m, fa, flm, fm, left, depth + 1, env)?;
    let r = simpson_rec(f, var, m, b, fm, frm, fb, right, depth + 1, env)?;
    Ok(l + r)
}

fn simpson(a: f64, b: f64, fa: f64, fm: f64, fb: f64) -> f64 {
    (b - a) / 6.0 * (fa + 4.0 * fm + fb)
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    use super::register_builtin_env;

    register_builtin_env(env, "LaplaceTransform", builtin_laplace_transform);
    register_builtin_env(
        env,
        "InverseLaplaceTransform",
        builtin_inverse_laplace_transform,
    );
    register_builtin_env(env, "FourierTransform", builtin_fourier_transform);
    register_builtin_env(
        env,
        "InverseFourierTransform",
        builtin_inverse_fourier_transform,
    );
    register_builtin_env(env, "ZTransform", builtin_ztransform);
    register_builtin_env(env, "InverseZTransform", builtin_inverse_ztransform);
    register_builtin_env(env, "Dt", builtin_dt);
    register_builtin_env(env, "FunctionDomain", builtin_function_domain);
    register_builtin_env(env, "NIntegrate", builtin_nintegrate);
    register_builtin_env(env, "NSum", builtin_nsum);
    register_builtin_env(env, "NProduct", builtin_nproduct);

    for name in [
        "LaplaceTransform",
        "InverseLaplaceTransform",
        "FourierTransform",
        "InverseFourierTransform",
        "ZTransform",
        "InverseZTransform",
        "Dt",
        "FunctionDomain",
        "NIntegrate",
        "NSum",
        "NProduct",
    ] {
        env.set_attributes(name, vec!["HoldAll".to_string()]);
    }

    for name in ["FourierTransform", "InverseFourierTransform"] {
        env.set_option(
            name,
            "FourierParameters".to_string(),
            Value::List(vec![int(0), int(1)]),
        );
    }
}

fn builtin_laplace_transform(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "LaplaceTransform requires exactly 3 arguments".to_string(),
        ));
    }
    let f = unwrap(&args[0]);
    let t = as_symbol(&args[1], "Symbol")?;
    let s = as_symbol(&args[2], "Symbol")?;
    Ok(laplace(&f, &t, &s, env))
}

fn builtin_inverse_laplace_transform(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "InverseLaplaceTransform requires exactly 3 arguments".to_string(),
        ));
    }
    let f = unwrap(&args[0]);
    let s = as_symbol(&args[1], "Symbol")?;
    let t = as_symbol(&args[2], "Symbol")?;
    Ok(inverse_laplace(&f, &s, &t, env))
}

/// Split a `Rule`/`RuleDelayed` — possibly still wrapped in a held
/// `Pattern` — into its unwrapped `(key, value)` parts.
fn rule_parts(v: &Value) -> Option<(Value, Value)> {
    match v {
        Value::Rule { lhs, rhs, .. } => Some((unwrap(lhs), unwrap(rhs))),
        Value::Pattern(Expr::Rule { lhs, rhs } | Expr::RuleDelayed { lhs, rhs }) => Some((
            crate::pattern::unwrap_expr_to_value(lhs),
            crate::pattern::unwrap_expr_to_value(rhs),
        )),
        _ => None,
    }
}

/// Given `FourierParameters -> {p, q}` (or a list containing it), return
/// `(p, q)`. The option argument is held, so unwrap patterns first.
fn parse_fourier_parameters(arg: &Value) -> Option<(Value, Value)> {
    let v = unwrap(arg);
    let candidates: Vec<&Value> = match &v {
        Value::List(items) => items.iter().collect(),
        other => vec![other],
    };
    for c in candidates {
        if let Some((lhs, rhs)) = rule_parts(c)
            && matches!(&lhs, Value::Symbol(s) if s == "FourierParameters")
            && let Value::List(items) = rhs
            && items.len() == 2
        {
            return Some((items[0].clone(), items[1].clone()));
        }
    }
    None
}

/// Frequency variable used in the output: `w` for the default `q = 1`,
/// otherwise `q*w`.
fn freq_var(q: &Value, w: &str) -> Value {
    if is_one(q) {
        sym(w)
    } else {
        times(vec![q.clone(), sym(w)])
    }
}

/// Overall amplitude factor `Sqrt[Abs[q] (2 Pi)^p]` for the generalized
/// Fourier integrator (see `FourierParameters`).
fn fourier_scale(p: &Value, q: &Value) -> Value {
    let absq = call("Abs", vec![q.clone()]);
    let two_pi = times(vec![int(2), sym("Pi")]);
    let pw = if is_zero(p) {
        int(1)
    } else {
        pow(two_pi, p.clone())
    };
    call("Sqrt", vec![times(vec![absq, pw])])
}

fn apply_fourier_scale(r: Value, p: &Value, q: &Value, env: &Env) -> Value {
    if is_zero(p) && is_one(q) {
        return r;
    }
    eval_value(times(vec![r, fourier_scale(p, q)]), env)
}

/// Reconstruct a `FourierParameters -> {p, q}` rule for unevaluated output.
fn fourier_option(p: &Value, q: &Value) -> Value {
    Value::Rule {
        lhs: Box::new(sym("FourierParameters")),
        rhs: Box::new(Value::List(vec![p.clone(), q.clone()])),
        delayed: false,
    }
}

/// Parse the common 3-or-4 argument Fourier signature, returning
/// `(f, t, w, p, q)`.
fn fourier_args(
    args: &[Value],
    name: &str,
) -> Result<(Value, String, String, Value, Value), EvalError> {
    if args.len() < 3 || args.len() > 4 {
        return Err(EvalError::Error(format!(
            "{name} requires 3 or 4 arguments"
        )));
    }
    let f = unwrap(&args[0]);
    let t = as_symbol(&args[1], "Symbol")?;
    let w = as_symbol(&args[2], "Symbol")?;
    let (p, q) = if args.len() == 4 {
        parse_fourier_parameters(&args[3]).unwrap_or((int(0), int(1)))
    } else {
        (int(0), int(1))
    };
    Ok((f, t, w, p, q))
}

fn builtin_fourier_transform(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let (f, t, w, p, q) = fourier_args(args, "FourierTransform")?;
    let f = normalize(&f);
    let wval = freq_var(&q, &w);
    match distribute(&f, env, &|x| fourier_term(x, &t, &wval, env)) {
        Some(r) => {
            let r = eval_value(r, env);
            Ok(apply_fourier_scale(r, &p, &q, env))
        }
        None => {
            let mut out = vec![f, sym(&t), sym(&w)];
            if args.len() == 4 {
                out.push(fourier_option(&p, &q));
            }
            Ok(call("FourierTransform", out))
        }
    }
}

fn builtin_inverse_fourier_transform(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let (f, w, t, p, q) = fourier_args(args, "InverseFourierTransform")?;
    let f = normalize(&f);
    let tval = freq_var(&q, &t);
    match distribute(&f, env, &|x| inverse_fourier_term(x, &w, &tval, env)) {
        Some(r) => {
            let r = eval_value(r, env);
            // Inverse amplitude uses `(2 Pi)^(-p)` rather than `(2 Pi)^p`.
            Ok(apply_fourier_scale(r, &negate(&p), &q, env))
        }
        None => {
            let mut out = vec![f, sym(&w), sym(&t)];
            if args.len() == 4 {
                out.push(fourier_option(&p, &q));
            }
            Ok(call("InverseFourierTransform", out))
        }
    }
}

fn inverse_fourier_term(f: &Value, w: &str, tval: &Value, env: &Env) -> Option<Value> {
    let mut coeff = int(1);
    let mut special: Option<Value> = None;
    for factor in &factors_of(f) {
        if let Some(u) = exp_argument(factor) {
            if let Some(c) = squared_coeff(&u, w) {
                let a = negate(&c);
                set_special(&mut special, gauss_result(&a, tval))?;
                continue;
            }
            if let Some(a) = i_linear_coeff(&u, w) {
                // Inverse kernel is E^(-I t w): a sign flip versus forward.
                set_special(
                    &mut special,
                    times(vec![
                        call("Sqrt", vec![times(vec![int(2), sym("Pi")])]),
                        dirac(plus(vec![tval.clone(), negate(&a)])),
                    ]),
                )?;
                continue;
            }
        }
        if let Value::Call { head, args } = factor
            && head == "DiracDelta"
            && args.len() == 1
            && let Some((m, c)) = affine(&args[0], w)
            && is_one(&m)
        {
            let amp = sqrt_half(&times(vec![int(2), sym("Pi")]));
            let ex = if is_zero(&c) {
                amp
            } else {
                times(vec![
                    amp,
                    pow(sym("E"), times(vec![negate(&sym("I")), c, tval.clone()])),
                ])
            };
            set_special(&mut special, ex)?;
            continue;
        }
        if let Some((kind, a)) = trig_linear(factor, w) {
            let amp = call("Sqrt", vec![div(sym("Pi"), int(2))]);
            let d1 = dirac(plus(vec![tval.clone(), negate(&a)]));
            let d2 = dirac(plus(vec![tval.clone(), a.clone()]));
            let body = match kind {
                "Cos" => times(vec![amp, plus(vec![d1, d2])]),
                "Sin" => times(vec![
                    times(vec![int(-1), sym("I")]),
                    amp,
                    plus(vec![d1, negate(&d2)]),
                ]),
                _ => return None,
            };
            set_special(&mut special, body)?;
            continue;
        }
        if !contains_var(factor, w) {
            coeff = times(vec![coeff, factor.clone()]);
            continue;
        }
        return None;
    }
    let base = match special {
        Some(s) => s,
        None => times(vec![
            call("Sqrt", vec![times(vec![int(2), sym("Pi")])]),
            dirac(tval.clone()),
        ]),
    };
    Some(eval_value(times(vec![coeff, base]), env))
}

fn builtin_ztransform(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "ZTransform requires exactly 3 arguments".to_string(),
        ));
    }
    let f = unwrap(&args[0]);
    let n = as_symbol(&args[1], "Symbol")?;
    let z = as_symbol(&args[2], "Symbol")?;
    Ok(ztransform(&f, &n, &z, env))
}

fn builtin_inverse_ztransform(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "InverseZTransform requires exactly 3 arguments".to_string(),
        ));
    }
    let f = unwrap(&args[0]);
    let z = as_symbol(&args[1], "Symbol")?;
    let n = as_symbol(&args[2], "Symbol")?;
    Ok(inverse_ztransform(&f, &z, &n, env))
}

fn builtin_dt(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error("Dt requires 1 or 2 arguments".to_string()));
    }
    let expr = unwrap(&args[0]);
    let xvar = if args.len() == 2 {
        Some(as_symbol(&args[1], "Symbol")?)
    } else {
        None
    };
    Ok(dt(&expr, xvar.as_deref(), env))
}

fn builtin_function_domain(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 3 {
        return Err(EvalError::Error(
            "FunctionDomain requires 2 or 3 arguments".to_string(),
        ));
    }
    let f = unwrap(&args[0]);
    let x = as_symbol(&args[1], "Symbol")?;
    Ok(function_domain(&f, &x, env))
}

// ── Tests ───────────────────────────────────────────────────────────────────

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

    #[test]
    fn laplace_basic_table() {
        let env = env();
        assert_eq!(eval_str("LaplaceTransform[1, t, s]", &env), "Divide[1, s]");
        assert_eq!(
            eval_str("LaplaceTransform[t, t, s]", &env),
            "Divide[1, Power[s, 2]]"
        );
        assert_eq!(
            eval_str("LaplaceTransform[t^2, t, s]", &env),
            "Divide[2, Power[s, 3]]"
        );
    }

    #[test]
    fn laplace_exponential_and_shift() {
        let env = env();
        assert_eq!(
            eval_str("LaplaceTransform[E^(a t), t, s]", &env),
            "Divide[1, Plus[s, Times[-1, a]]]"
        );
        assert_eq!(
            eval_str("LaplaceTransform[t^3 E^(2 t), t, s]", &env),
            "Divide[6, Power[Plus[s, -2], 4]]"
        );
    }

    #[test]
    fn laplace_trig() {
        let env = env();
        assert_eq!(
            eval_str("LaplaceTransform[Sin[a t], t, s]", &env),
            "Divide[a, Plus[Power[s, 2], Power[a, 2]]]"
        );
        assert_eq!(
            eval_str("LaplaceTransform[Cos[a t], t, s]", &env),
            "Divide[s, Plus[Power[s, 2], Power[a, 2]]]"
        );
        assert_eq!(
            eval_str("LaplaceTransform[Sinh[a t], t, s]", &env),
            "Divide[a, Plus[Power[s, 2], Times[-1, Power[a, 2]]]]"
        );
        assert_eq!(
            eval_str("LaplaceTransform[Cosh[a t], t, s]", &env),
            "Divide[s, Plus[Power[s, 2], Times[-1, Power[a, 2]]]]"
        );
    }

    #[test]
    fn laplace_linearity() {
        let env = env();
        assert_eq!(
            eval_str("LaplaceTransform[1 + 2 t, t, s]", &env),
            "Plus[Divide[1, s], Times[2, Divide[1, Power[s, 2]]]]"
        );
        assert_eq!(
            eval_str("LaplaceTransform[3 Sin[2 t], t, s]", &env),
            "Times[3, Divide[2, Plus[Power[s, 2], 4]]]"
        );
    }

    #[test]
    fn laplace_unevaluated() {
        let env = env();
        assert_eq!(
            eval_str("LaplaceTransform[Sin[t^2], t, s]", &env),
            "LaplaceTransform[Sin[Power[t, 2]], t, s]"
        );
    }

    #[test]
    fn inverse_laplace_table() {
        let env = env();
        assert_eq!(eval_str("InverseLaplaceTransform[1/s, s, t]", &env), "1");
        assert_eq!(eval_str("InverseLaplaceTransform[1/s^2, s, t]", &env), "t");
        assert_eq!(
            eval_str("InverseLaplaceTransform[1/(s - a), s, t]", &env),
            "Power[E, Times[a, t]]"
        );
    }

    #[test]
    fn inverse_laplace_more() {
        let env = env();
        assert_eq!(
            eval_str("InverseLaplaceTransform[a/(s^2 + a^2), s, t]", &env),
            "Sin[Times[a, t]]"
        );
        assert_eq!(
            eval_str("InverseLaplaceTransform[s/(s^2 + a^2), s, t]", &env),
            "Cos[Times[a, t]]"
        );
        assert_eq!(
            eval_str("InverseLaplaceTransform[1/(s - 2)^3, s, t]", &env),
            "Times[Divide[Power[t, 2], 2], Power[E, Times[2, t]]]"
        );
        assert_eq!(
            eval_str("InverseLaplaceTransform[a/(s^2 - a^2), s, t]", &env),
            "Sinh[Times[a, t]]"
        );
    }

    #[test]
    fn fourier_basic() {
        let env = env();
        assert_eq!(
            eval_str("FourierTransform[DiracDelta[t], t, w]", &env),
            "Divide[1, Sqrt[Times[2, Pi]]]"
        );
        assert_eq!(
            eval_str("FourierTransform[E^(-a t^2), t, w]", &env),
            "Times[Divide[1, Sqrt[Times[2, a]]], Power[E, Times[-1, Divide[Power[w, 2], Times[4, a]]]]]"
        );
    }

    #[test]
    fn fourier_trig_and_inverse() {
        let env = env();
        let s = eval_str("FourierTransform[Cos[a t], t, w]", &env);
        assert!(
            s.contains("DiracDelta[Plus[w, a]]") && s.contains("DiracDelta[Plus[w, Times[-1, a]]]"),
            "{s}"
        );
        assert_eq!(
            eval_str("InverseFourierTransform[DiracDelta[w], w, t]", &env),
            "Divide[1, Sqrt[Times[2, Pi]]]"
        );
    }

    #[test]
    fn ztransform_table() {
        let env = env();
        assert_eq!(
            eval_str("ZTransform[1, n, z]", &env),
            "Divide[z, Plus[z, -1]]"
        );
        assert_eq!(
            eval_str("ZTransform[a^n, n, z]", &env),
            "Divide[z, Plus[z, Times[-1, a]]]"
        );
        assert_eq!(
            eval_str("ZTransform[n, n, z]", &env),
            "Divide[z, Power[Plus[z, -1], 2]]"
        );
    }

    #[test]
    fn inverse_ztransform_table() {
        let env = env();
        assert_eq!(
            eval_str("InverseZTransform[z/(z - a), z, n]", &env),
            "Power[a, n]"
        );
        assert_eq!(eval_str("InverseZTransform[z/(z - 1), z, n]", &env), "1");
        assert_eq!(eval_str("InverseZTransform[z/(z - 1)^2, z, n]", &env), "n");
    }

    #[test]
    fn dt_basic() {
        let env = env();
        assert_eq!(eval_str("Dt[x^2]", &env), "Times[2, x, Dt[x]]");
        assert_eq!(eval_str("Dt[x^2, x]", &env), "Times[2, x]");
        assert_eq!(eval_str("Dt[Sin[x], x]", &env), "Cos[x]");
        assert_eq!(eval_str("Dt[5, x]", &env), "0");
    }

    #[test]
    fn dt_chain_and_product() {
        let env = env();
        assert_eq!(eval_str("Dt[x y, x]", &env), "Plus[y, Times[x, Dt[y, x]]]");
        let got = eval_str("Dt[E^(a t), t]", &env);
        assert!(got.contains("Dt[a, t]"), "{got}");
        assert!(got.contains("Power[E, Times[a, t]]"), "{got}");
    }

    #[test]
    fn function_domain_cases() {
        let env = env();
        assert_eq!(
            eval_str("FunctionDomain[Sqrt[x], x]", &env),
            "GreaterEqual[x, 0]"
        );
        assert_eq!(eval_str("FunctionDomain[Log[x], x]", &env), "Greater[x, 0]");
        assert_eq!(
            eval_str("FunctionDomain[1/x, x]", &env),
            "Alternatives[Less[x, 0], Greater[x, 0]]"
        );
        assert_eq!(eval_str("FunctionDomain[1, x]", &env), "Reals");
        assert_eq!(
            eval_str("FunctionDomain[Sqrt[x] + 1/(x - 2), x]", &env),
            "And[GreaterEqual[x, 0], Alternatives[Less[Plus[x, -2], 0], Greater[Plus[x, -2], 0]]]"
        );
    }

    #[test]
    fn nintegrate_simpson() {
        let env = env();
        let raw = eval_str("NIntegrate[x^2, {x, 0, 1}]", &env);
        let got: f64 = raw.parse().unwrap_or_else(|_| panic!("raw={raw:?}"));
        assert!((got - 1.0 / 3.0).abs() < 1e-8, "got {got}");
        let raw2 = eval_str("NIntegrate[Sin[x], {x, 0, Pi}]", &env);
        let got2: f64 = raw2.parse().unwrap_or_else(|_| panic!("raw2={raw2:?}"));
        assert!((got2 - 2.0).abs() < 1e-8, "got {got2}");
    }

    #[test]
    fn nsum_and_nproduct() {
        let env = env();
        assert_eq!(eval_str("NSum[k, {k, 1, 5}]", &env), "15");
        let s: f64 = eval_str("NSum[1/n^2, {n, 1, Infinity}]", &env)
            .parse()
            .unwrap();
        assert!(
            (s - std::f64::consts::PI.powi(2) / 6.0).abs() < 1e-6,
            "got {s}"
        );
        assert_eq!(eval_str("NProduct[k, {k, 1, 5}]", &env), "120");
    }

    #[test]
    fn default_attribute_hold() {
        let env = env();
        assert!(env.has_attribute("Dt", "HoldAll"));
        assert!(env.has_attribute("FunctionDomain", "HoldAll"));
    }

    #[test]
    fn laplace_extended_table() {
        let env = env();
        // Dirac delta and shift.
        assert_eq!(eval_str("LaplaceTransform[DiracDelta[t], t, s]", &env), "1");
        assert_eq!(
            eval_str("LaplaceTransform[DiracDelta[t - a], t, s]", &env),
            "Power[E, Times[-1, a, s]]"
        );
        // Damped oscillator.
        assert_eq!(
            eval_str("LaplaceTransform[Cos[b t] E^(a t), t, s]", &env),
            "Divide[Plus[s, Times[-1, a]], Plus[Power[Plus[s, Times[-1, a]], 2], Power[b, 2]]]"
        );
        // Fractional powers via Gamma.
        let sq = eval_str("LaplaceTransform[1/Sqrt[t], t, s]", &env);
        assert!(sq.contains("Sqrt[Pi]"), "{sq}");
        let inv = eval_str("LaplaceTransform[Sqrt[t], t, s]", &env);
        assert!(inv.contains("Gamma"), "{inv}");
    }

    #[test]
    fn laplace_damped_unevaluated_forms() {
        // Linearity is preserved for a sum of table entries.
        let env = env();
        assert_eq!(
            eval_str("LaplaceTransform[2 + 3 E^(a t), t, s]", &env),
            "Plus[Times[2, Divide[1, s]], Times[3, Divide[1, Plus[s, Times[-1, a]]]]]"
        );
    }

    #[test]
    fn inverse_laplace_fractional() {
        let env = env();
        let r = eval_str("InverseLaplaceTransform[1/Sqrt[s], s, t]", &env);
        assert!(r.contains("Sqrt[Pi]"), "{r}");
    }

    #[test]
    fn fourier_imaginary_exponential() {
        let env = env();
        assert_eq!(
            eval_str("FourierTransform[E^(I a t), t, w]", &env),
            "Times[Sqrt[Times[2, Pi]], DiracDelta[Plus[w, a]]]"
        );
        // Inverse flips the sign of the delta argument.
        let inv = eval_str("InverseFourierTransform[E^(I a w), w, t]", &env);
        assert!(inv.contains("DiracDelta[Plus[t, Times[-1, a]]]"), "{inv}");
    }

    #[test]
    fn fourier_parameters_option() {
        let env = env();
        // {0, 1} is the default: unchanged delta amplitude.
        assert_eq!(
            eval_str(
                "FourierTransform[DiracDelta[t], t, w, FourierParameters -> {0, 1}]",
                &env
            ),
            "Divide[1, Sqrt[Times[2, Pi]]]"
        );
        // Non-default parameters rescale the result.
        let scaled = eval_str(
            "FourierTransform[E^(I a t), t, w, FourierParameters -> {1, 1}]",
            &env,
        );
        assert!(scaled.contains("Sqrt[Times[2, Pi]]"), "{scaled}");
        // The option is registered as a default.
        assert!(
            env.get_options("FourierTransform")
                .iter()
                .any(|(k, _)| k == "FourierParameters")
        );
    }

    #[test]
    fn ztransform_trig() {
        let env = env();
        assert_eq!(
            eval_str("ZTransform[Cos[a n], n, z]", &env),
            "Divide[Times[z, Plus[z, Times[-1, Cos[a]]]], Plus[Power[z, 2], Times[-2, z, Cos[a]], 1]]"
        );
        assert_eq!(
            eval_str("ZTransform[Sin[a n], n, z]", &env),
            "Divide[Times[z, Sin[a]], Plus[Power[z, 2], Times[-2, z, Sin[a]], 1]]"
        );
    }

    #[test]
    fn inverse_ztransform_cubic() {
        let env = env();
        assert_eq!(
            eval_str("InverseZTransform[z/(z - 1)^3, z, n]", &env),
            "Divide[Times[n, Plus[n, -1]], 2]"
        );
    }
}
