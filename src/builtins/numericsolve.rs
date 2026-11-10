use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::Integer;

fn substitute_numeric(expr: &Value, var: &str, val: f64) -> Value {
    match expr {
        Value::Symbol(s) if s == var => super::real(val),
        Value::List(items) => Value::List(
            items
                .iter()
                .map(|a| substitute_numeric(a, var, val))
                .collect(),
        ),
        Value::Call { head, args } => {
            let new_args: Vec<Value> = args
                .iter()
                .map(|a| substitute_numeric(a, var, val))
                .collect();
            Value::Call {
                head: head.clone(),
                args: new_args,
            }
        }
        Value::Rule { lhs, rhs, delayed } => Value::Rule {
            lhs: Box::new(substitute_numeric(lhs, var, val)),
            rhs: Box::new(substitute_numeric(rhs, var, val)),
            delayed: *delayed,
        },
        _ => expr.clone(),
    }
}

fn eval_expr_to_f64(expr: &Value, env: &Env) -> Result<f64, EvalError> {
    match expr {
        Value::Integer(n) => Ok(n.to_f64()),
        Value::Real(r) => Ok(r.to_f64()),
        Value::Rational(r) => Ok(r.to_f64()),
        Value::Complex { re, im: 0.0 } => Ok(*re),
        _ => {
            // Fully evaluate the (already variable-substituted) expression
            // rather than only simplifying it: `Simplify` does not always
            // perform arithmetic, which previously made every residual `0`.
            let as_expr = crate::eval::table::value_to_expr(expr);
            let evaluated = crate::eval::eval(&as_expr, env).unwrap_or_else(|_| expr.clone());
            super::to_f64(&evaluated).ok_or_else(|| {
                EvalError::Error("Cannot evaluate expression to a real number".to_string())
            })
        }
    }
}

fn numerical_derivative(f: &dyn Fn(f64) -> f64, x: f64) -> f64 {
    let h = 1e-8;
    (f(x + h) - f(x - h)) / (2.0 * h)
}

fn newton_method(f: &dyn Fn(f64) -> f64, mut x: f64) -> Result<f64, EvalError> {
    for _ in 0..100 {
        let fx = f(x);
        if fx.abs() < 1e-12 {
            return Ok(x);
        }
        let dfx = numerical_derivative(f, x);
        if dfx.abs() < 1e-15 {
            return Err(EvalError::Error(
                "Newton's method: derivative near zero".to_string(),
            ));
        }
        let x_new = x - fx / dfx;
        if (x_new - x).abs() < 1e-12 {
            return Ok(x_new);
        }
        x = x_new;
    }
    Err(EvalError::Error(
        "Newton's method: did not converge in 100 iterations".to_string(),
    ))
}

fn brent_method(f: &dyn Fn(f64) -> f64, a: f64, b: f64) -> Result<f64, EvalError> {
    let fa = f(a);
    let fb = f(b);
    if fa * fb > 0.0 {
        return Err(EvalError::Error(
            "Signs at endpoints are the same; root not bracketed".to_string(),
        ));
    }
    if fa.abs() < 1e-12 {
        return Ok(a);
    }
    if fb.abs() < 1e-12 {
        return Ok(b);
    }

    let mut a = a;
    let mut b = b;
    let mut c = a;
    let mut fa = fa;
    let mut fb = fb;
    let mut fc = fa;
    let mut m_flag = true;

    for _ in 0..100 {
        let mid = (a + b) / 2.0;
        if (b - a).abs() < 1e-12 || fb.abs() < 1e-12 {
            break;
        }

        let d = if fa != fc && fb != fc {
            let s = fb / fa;
            let t = fb / fc;
            let num = mid * s * (fc - fb) + a * t * (fb - fa) + c * s * t * (fa - fc);
            let den = (fc - fb) + (fb - fa) + s * t * (fa - fc);
            if den.abs() < 1e-30 { mid } else { num / den }
        } else {
            mid
        };

        let d_clamped = if (d - mid).abs() > (b - a).abs() / 2.0
            || (m_flag && d >= ((b + a) / 2.0).max(b))
            || (!m_flag && d <= ((b + a) / 2.0).min(b))
        {
            mid
        } else {
            d
        };

        let fd = f(d_clamped);
        m_flag = fb > fd;

        if fd * fb < 0.0 {
            a = b;
            fa = fb;
            c = d_clamped;
            fc = fd;
        } else {
            c = b;
            fc = fb;
        }
        b = d_clamped;
        fb = fd;

        if fa.abs() < fb.abs() {
            let tmp_a = a;
            let tmp_fa = fa;
            a = b;
            b = tmp_a;
            fa = fb;
            fb = tmp_fa;
            c = a;
            fc = fa;
        }
    }

    Ok(if fa.abs() < fb.abs() { a } else { b })
}

/// Local minimiser from a starting point: Newton's method on the derivative.
fn newton_minimize(f: &dyn Fn(f64) -> f64, mut x: f64) -> f64 {
    let h = 1e-6;
    for _ in 0..100 {
        let d1 = (f(x + h) - f(x - h)) / (2.0 * h);
        let d2 = (f(x + h) - 2.0 * f(x) + f(x - h)) / (h * h);
        if d2.abs() < 1e-15 {
            break;
        }
        let x_new = x - d1 / d2;
        if (x_new - x).abs() < 1e-12 {
            return x_new;
        }
        x = x_new;
    }
    x
}

/// Expand an interval `[a, b, c]` until the middle is a local minimum
/// (`f(b) <= f(a)` and `f(b) <= f(c)`). Works from an arbitrary start point.
fn bracket_min(f: &dyn Fn(f64) -> f64, x0: f64) -> (f64, f64, f64) {
    const GOLD: f64 = std::f64::consts::GOLDEN_RATIO;
    let mut a = x0;
    let mut b = x0 + 1.0;
    let mut fa = f(a);
    let mut fb = f(b);
    if fb > fa {
        std::mem::swap(&mut a, &mut b);
        std::mem::swap(&mut fa, &mut fb);
    }
    let mut c = b + GOLD * (b - a);
    let mut fc = f(c);
    for _ in 0..200 {
        if fb <= fc {
            return (a, b, c);
        }
        a = b;
        b = c;
        fb = fc;
        c = b + GOLD * (b - a);
        fc = f(c);
    }
    (a, b, c)
}

/// Robust local minimiser from a start point: bracket the minimum, then run
/// golden-section search. Falls back to Newton's method if bracketing stalls.
fn local_minimize(f: &dyn Fn(f64) -> f64, x0: f64) -> f64 {
    let (a, _b, c) = bracket_min(f, x0);
    if !(a.is_finite() && c.is_finite()) || c == a {
        return newton_minimize(f, x0);
    }
    let (lo, hi) = (a.min(c), a.max(c));
    let (_v, x) = golden_section(f, lo, hi);
    if x.is_finite() {
        x
    } else {
        newton_minimize(f, x0)
    }
}

/// Find a stationary/nearby extremum from a start point. Newton on the
/// derivative converges quickly for smooth functions; if the result is not a
/// stationary point (or Newton stalls), fall back to a bracketing local search.
fn stationary_or_local(f: &dyn Fn(f64) -> f64, x0: f64) -> f64 {
    let x = newton_minimize(f, x0);
    if x.is_finite() && f(x).is_finite() && numerical_derivative(f, x).abs() < 1e-4 {
        x
    } else {
        local_minimize(f, x0)
    }
}

fn golden_section<F: Fn(f64) -> f64>(f: F, a: f64, b: f64) -> (f64, f64) {
    let inv_phi = 2.0 / (1.0 + 5.0f64.sqrt());
    let mut lo = a;
    let mut hi = b;
    let mut c = hi - (hi - lo) * inv_phi;
    let mut d = lo + (hi - lo) * inv_phi;
    let mut fc = f(c);
    let mut fd = f(d);

    for _ in 0..200 {
        if (hi - lo) < 1e-14 {
            break;
        }
        if fc < fd {
            hi = d;
            d = c;
            fd = fc;
            c = hi - (hi - lo) * inv_phi;
            fc = f(c);
        } else if fd < fc {
            lo = c;
            c = d;
            fc = fd;
            d = lo + (hi - lo) * inv_phi;
            fd = f(d);
        } else {
            // fc == fd (within floating point): narrow from both sides
            lo = c;
            hi = d;
            c = hi - (hi - lo) * inv_phi;
            d = lo + (hi - lo) * inv_phi;
            fc = f(c);
            fd = f(d);
        }
    }

    let x_opt = (lo + hi) / 2.0;
    (f(x_opt), x_opt)
}

fn multi_restart_minimize(
    f: &dyn Fn(f64) -> f64,
    xmin: f64,
    xmax: f64,
    num_restarts: usize,
) -> (f64, f64) {
    let range = xmax - xmin;
    let mut best_val = f64::INFINITY;
    let mut best_x = xmin;

    for i in 0..num_restarts {
        let frac = (i as f64 + 0.5) / num_restarts as f64;
        let center = xmin + frac * range;
        let half = range / (num_restarts as f64 * 2.0);
        let lo = (center - half).max(xmin);
        let hi = (center + half).min(xmax);
        if hi - lo < 1e-14 {
            let val = f(center);
            if val < best_val {
                best_val = val;
                best_x = center;
            }
        } else {
            let (val, x) = golden_section(f, lo, hi);
            if val < best_val {
                best_val = val;
                best_x = x;
            }
        }
    }

    let (val, x) = golden_section(f, xmin, xmax);
    if val < best_val {
        best_val = val;
        best_x = x;
    }

    (best_val, best_x)
}

/// Extract equation args. Returns (expr_clone, rhs_value, eq_is_equal).
fn parse_equation(arg: &Value) -> Result<(Value, f64), EvalError> {
    match arg {
        Value::Call {
            head,
            args: eq_args,
        } if head == "Equal" && eq_args.len() == 2 => {
            let rhs = super::to_f64(&eq_args[1]).ok_or_else(|| {
                EvalError::Error("Right-hand side of equation must be a number".to_string())
            })?;
            Ok((eq_args[0].clone(), rhs))
        }
        _ => Ok((arg.clone(), 0.0)),
    }
}

/// Like [`parse_equation`] but keeps a symbolic right-hand side, so equations
/// such as `Cos[x] == x` are handled correctly. A bare expression `f` is
/// treated as `f == 0`.
fn parse_equation_sym(arg: &Value) -> (Value, Value) {
    match arg {
        Value::Call {
            head,
            args: eq_args,
        } if head == "Equal" && eq_args.len() == 2 => (eq_args[0].clone(), eq_args[1].clone()),
        _ => (arg.clone(), super::real(0.0)),
    }
}

/// Collect the free symbols (function heads excluded) in `expr`.
fn expr_free_symbols(expr: &Value, out: &mut Vec<String>) {
    match expr {
        Value::Symbol(s) => {
            if !out.contains(s) {
                out.push(s.clone());
            }
        }
        Value::List(items) => items.iter().for_each(|i| expr_free_symbols(i, out)),
        Value::Call { args, .. } => args.iter().for_each(|a| expr_free_symbols(a, out)),
        Value::Rule { lhs, rhs, .. } => {
            expr_free_symbols(lhs, out);
            expr_free_symbols(rhs, out);
        }
        _ => {}
    }
}

/// Determine the optimization/solve variable. When the held variable spec was
/// resolved to a value (because the symbol was bound), recover the name from
/// the free symbols of the expression.
fn resolve_var(spec0: &Value, exprs: &[&Value]) -> Result<String, EvalError> {
    if let Value::Symbol(s) = spec0 {
        return Ok(s.clone());
    }
    let mut syms = Vec::new();
    for e in exprs {
        expr_free_symbols(e, &mut syms);
    }
    match syms.len() {
        0 => Ok("x".to_string()),
        _ => Ok(syms.remove(0)),
    }
}

/// Strip a `HoldAll`/`Hold*` wrapper (`Value::Pattern` / `Value::Hold`) from an
/// argument so builtins work whether or not their attributes evaluate args.
/// The held expression is *not* evaluated, which is what keeps equations such
/// as `Cos[x] == x` intact (instead of collapsing to `False`) and keeps the
/// optimization variable symbolic.
fn unheld(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => crate::pattern::unwrap_expr_to_value(e),
        Value::Hold(b) | Value::HoldComplete(b) => (**b).clone(),
        other => other.clone(),
    }
}

/// Deeply strip held wrappers (used for variables/specs nested in a list).
fn deep_unheld(v: &Value) -> Value {
    match unheld(v) {
        Value::List(items) => Value::List(items.iter().map(deep_unheld).collect()),
        other => other,
    }
}

/// Extract a numeric spec entry, evaluating a held/compound form (e.g. `-5`,
/// which arrives as an unevaluated `Times[-1, 5]`).
fn spec_number(v: &Value, name: &str, pos: usize, env: &Env) -> Result<f64, EvalError> {
    if let Some(x) = super::to_f64(v) {
        return Ok(x);
    }
    if let Some(evaluated) = eval_value(v, env)
        && let Some(x) = super::to_f64(&evaluated)
    {
        return Ok(x);
    }
    super::require_f64(v, name, pos)
}

/// Evaluate an (unheld) value by converting it back to an expression.
fn eval_value(v: &Value, env: &Env) -> Option<Value> {
    if let Some(x) = super::to_f64(v) {
        return Some(super::real(x));
    }
    let e = crate::eval::table::value_to_expr(v);
    crate::eval::eval(&e, env).ok()
}

/// Solve `d/dvar expr == 0` symbolically and return the real critical points.
/// Returns an empty vector when the derivative is constant or cannot be solved.
fn critical_points(expr: &Value, var: &str, env: &Env) -> Vec<f64> {
    let derivative = Value::Call {
        head: "D".to_string(),
        args: vec![expr.clone(), Value::Symbol(var.to_string())],
    };
    let deriv = match eval_value(&derivative, env) {
        Some(d) => d,
        None => return Vec::new(),
    };
    // A constant (or vanishing) derivative has no isolated critical point; the
    // caller falls back to a numeric scan in that case.
    if super::to_f64(&deriv).is_some() && !matches!(&deriv, Value::Call { .. }) {
        return Vec::new();
    }

    let equation = Value::Call {
        head: "Equal".to_string(),
        args: vec![deriv, super::real(0.0)],
    };
    let solved = Value::Call {
        head: "Solve".to_string(),
        args: vec![equation, Value::Symbol(var.to_string())],
    };
    let solutions = match eval_value(&solved, env) {
        Some(s) => s,
        None => return Vec::new(),
    };

    let mut points = Vec::new();
    if let Value::List(rows) = solutions {
        for row in rows {
            if let Value::List(rules) = row {
                for rule in rules {
                    if let Value::Rule { rhs, .. } = rule
                        && let Some(x) = super::to_f64(&rhs)
                        && x.is_finite()
                    {
                        points.push(x);
                    }
                }
            }
        }
    }
    points
}

/// Return the smaller/larger of `(v, x)` according to `maximize`.
fn pick(best: &mut Option<(f64, f64)>, v: f64, x: f64, maximize: bool) {
    if !v.is_finite() || !x.is_finite() {
        return;
    }
    match *best {
        None => *best = Some((v, x)),
        Some((bv, _)) if maximize => {
            if v > bv {
                *best = Some((v, x))
            }
        }
        Some((bv, _)) if v < bv => *best = Some((v, x)),
        _ => {}
    }
}

/// FindRoot[eqn, {var, x0}] — Newton's method from starting point.
/// FindRoot[eqn, {var, xmin, xmax}] — Brent's method on interval.
pub fn builtin_find_root(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FindRoot requires exactly 2 arguments".to_string(),
        ));
    }

    let eqn = unheld(&args[0]);
    let spec_val = deep_unheld(&args[1]);
    let (lhs, rhs) = parse_equation_sym(&eqn);
    let search_spec = match &spec_val {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Second argument must be a list: {var, x0} or {var, xmin, xmax}".to_string(),
            ));
        }
    };

    if search_spec.len() < 2 || search_spec.len() > 3 {
        return Err(EvalError::Error(
            "Search specification must have 2 or 3 elements".to_string(),
        ));
    }

    let var = resolve_var(&search_spec[0], &[&lhs, &rhs])?;
    let var_clone = var.clone();

    // Evaluate `lhs - rhs` at a candidate `x`. Both sides may be symbolic.
    let residual = move |x: f64| {
        let l = substitute_numeric(&lhs, &var_clone, x);
        let r = substitute_numeric(&rhs, &var_clone, x);
        eval_expr_to_f64(&l, env).unwrap_or(0.0) - eval_expr_to_f64(&r, env).unwrap_or(0.0)
    };

    let root = if search_spec.len() == 2 {
        let x0 = spec_number(&search_spec[1], "FindRoot", 1, env)?;
        // Newton first; if it stalls (e.g. a horizontal tangent), fall back to
        // scanning for a sign change and refining with Brent's method.
        newton_method(&residual, x0).or_else(|_| scan_for_root(&residual, x0))?
    } else {
        let xmin = spec_number(&search_spec[1], "FindRoot", 1, env)?;
        let xmax = spec_number(&search_spec[2], "FindRoot", 2, env)?;
        if xmin >= xmax {
            return Err(EvalError::Error(
                "FindRoot: interval must satisfy xmin < xmax".to_string(),
            ));
        }
        brent_method(&residual, xmin, xmax).or_else(|_| scan_for_root(&residual, xmin))?
    };
    // Mathematica returns a list of rules: `{x -> root}`.
    Ok(Value::List(vec![Value::Rule {
        lhs: Box::new(Value::Symbol(var)),
        rhs: Box::new(super::real(root)),
        delayed: false,
    }]))
}

/// Scan outward from `x0` for a sign change of `f`, then refine with Brent.
fn scan_for_root(f: &dyn Fn(f64) -> f64, x0: f64) -> Result<f64, EvalError> {
    let mut step = 0.5_f64.max(x0.abs() * 0.1);
    let mut prev_x = x0;
    let mut prev_f = f(x0);
    for _ in 0..80 {
        let x = x0 + step;
        let fx = f(x);
        if prev_f == 0.0 {
            return Ok(prev_x);
        }
        if prev_f * fx < 0.0 || fx == 0.0 {
            return brent_method(f, prev_x.min(x), prev_x.max(x));
        }
        prev_x = x;
        prev_f = fx;
        step *= 1.5;
    }
    Err(EvalError::Error(
        "FindRoot: could not bracket a root".to_string(),
    ))
}

/// FindMinimum[f, {x, xmin, xmax}] — find local minimum via golden section search.
/// FindMinimum[f, {x, x0}] — local minimum near the start point `x0`.
pub fn builtin_find_minimum(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FindMinimum requires exactly 2 arguments".to_string(),
        ));
    }

    let expr = unheld(&args[0]);
    let spec_val = deep_unheld(&args[1]);
    let spec = match &spec_val {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Second argument must be a list: {var, x0} or {var, xmin, xmax}".to_string(),
            ));
        }
    };

    if spec.len() != 2 && spec.len() != 3 {
        return Err(EvalError::Error(
            "Search specification must have 2 or 3 elements: {var, x0} or {var, xmin, xmax}"
                .to_string(),
        ));
    }

    let var = resolve_var(&spec[0], &[&expr])?;
    let var_clone = var.clone();
    let f = move |x: f64| {
        let substituted = substitute_numeric(&expr, &var_clone, x);
        eval_expr_to_f64(&substituted, env).unwrap_or(f64::INFINITY)
    };

    let (f_min, x_min) = if spec.len() == 2 {
        let x0 = spec_number(&spec[1], "FindMinimum", 1, env)?;
        let x_min = stationary_or_local(&f, x0);
        (f(x_min), x_min)
    } else {
        let xmin = spec_number(&spec[1], "FindMinimum", 1, env)?;
        let xmax = spec_number(&spec[2], "FindMinimum", 2, env)?;
        if xmin >= xmax {
            return Err(EvalError::Error(
                "FindMinimum: interval must satisfy xmin < xmax".to_string(),
            ));
        }
        golden_section(&f, xmin, xmax)
    };

    Ok(Value::List(vec![
        super::real(f_min),
        Value::List(vec![Value::Rule {
            lhs: Box::new(Value::Symbol(var)),
            rhs: Box::new(super::real(x_min)),
            delayed: false,
        }]),
    ]))
}

/// FindMaximum[f, {x, xmin, xmax}] — find local maximum.
pub fn builtin_find_maximum(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FindMaximum requires exactly 2 arguments".to_string(),
        ));
    }

    let expr = unheld(&args[0]);
    let spec_val = deep_unheld(&args[1]);
    let spec = match &spec_val {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Second argument must be a list: {var, xmin, xmax}".to_string(),
            ));
        }
    };

    if spec.len() != 2 && spec.len() != 3 {
        return Err(EvalError::Error(
            "Search specification must have 2 or 3 elements: {var, x0} or {var, xmin, xmax}"
                .to_string(),
        ));
    }

    let var = resolve_var(&spec[0], &[&expr])?;
    let var_clone = var.clone();
    let f = move |x: f64| {
        let substituted = substitute_numeric(&expr, &var_clone, x);
        eval_expr_to_f64(&substituted, env).unwrap_or(f64::NEG_INFINITY)
    };

    let neg_f = |x: f64| -f(x);
    let (f_max, x_max) = if spec.len() == 2 {
        let x0 = spec_number(&spec[1], "FindMaximum", 1, env)?;
        let x_max = stationary_or_local(&neg_f, x0);
        (f(x_max), x_max)
    } else {
        let xmin = spec_number(&spec[1], "FindMaximum", 1, env)?;
        let xmax = spec_number(&spec[2], "FindMaximum", 2, env)?;
        if xmin >= xmax {
            return Err(EvalError::Error(
                "FindMaximum: interval must satisfy xmin < xmax".to_string(),
            ));
        }
        let (neg_f_min, x_max) = golden_section(neg_f, xmin, xmax);
        (-neg_f_min, x_max)
    };

    Ok(Value::List(vec![
        super::real(f_max),
        Value::List(vec![Value::Rule {
            lhs: Box::new(Value::Symbol(var)),
            rhs: Box::new(super::real(x_max)),
            delayed: false,
        }]),
    ]))
}

/// Shared `Minimize`/`Maximize` implementation: candidates from the symbolic
/// critical points (`D` + `Solve`), interval endpoints, and a numeric scan,
/// then returns the Wolfram-style `{value, {x -> x0}}` pair.
fn symbolic_extremum(
    args: &[Value],
    env: &Env,
    maximize: bool,
    name: &str,
) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(format!(
            "{name} requires exactly 2 arguments"
        )));
    }

    let first = unheld(&args[0]);
    let second = deep_unheld(&args[1]);

    // `Minimize[{f, cons}, spec]` — use the objective and ignore the constraint.
    let expr = match &first {
        Value::List(items)
            if items.len() == 2
                && matches!(
                    &items[1],
                    Value::Call { head, .. }
                        if matches!(
                            head.as_str(),
                            "Equal"
                                | "Unequal"
                                | "Less"
                                | "LessEqual"
                                | "Greater"
                                | "GreaterEqual"
                                | "And"
                                | "Or"
                        )
                ) =>
        {
            items[0].clone()
        }
        _ => first,
    };

    let (var, bounds) = match &second {
        Value::Symbol(s) => (s.clone(), None),
        Value::List(items) if !items.is_empty() => {
            let var = resolve_var(&items[0], &[&expr])?;
            let bounds = if items.len() >= 3 {
                match (
                    spec_number(&items[1], name, 1, env),
                    spec_number(&items[2], name, 2, env),
                ) {
                    (Ok(lo), Ok(hi)) => Some((lo.min(hi), lo.max(hi))),
                    _ => None,
                }
            } else {
                None
            };
            (var, bounds)
        }
        // The bare variable symbol was resolved to its bound value.
        _ => (resolve_var(&second, &[&expr])?, None),
    };

    let var_clone = var.clone();
    let f = |x: f64| {
        let substituted = substitute_numeric(&expr, &var_clone, x);
        eval_expr_to_f64(&substituted, env)
    };

    let mut best: Option<(f64, f64)> = None;

    // Symbolic critical points.
    for cp in critical_points(&expr, &var, env) {
        if let Ok(v) = f(cp) {
            pick(&mut best, v, cp, maximize);
        }
    }

    // Bounds are always candidates.
    if let Some((lo, hi)) = bounds {
        for x in [lo, hi] {
            if let Ok(v) = f(x) {
                pick(&mut best, v, x, maximize);
            }
        }
    }

    // Numeric scan / local refinement: polish around the best candidate, or
    // fall back to a scan over the natural range when nothing was symbolic.
    let (scan_lo, scan_hi) = match (bounds, best) {
        (Some((lo, hi)), _) => (lo, hi),
        (None, Some((_, bx))) => (bx - 10.0, bx + 10.0),
        (None, None) => (-100.0, 100.0),
    };
    let g = |x: f64| {
        let v = f(x).unwrap_or(if maximize {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
        if maximize { -v } else { v }
    };
    let seed = best.map(|(_, x)| x).unwrap_or((scan_lo + scan_hi) / 2.0);
    let x_ref = if bounds.is_some() {
        let (v, x) = golden_section(g, scan_lo, scan_hi);
        let v = if maximize { -v } else { v };
        pick(&mut best, v, x, maximize);
        x
    } else {
        stationary_or_local(&g, seed)
    };
    let v_ref = if maximize { -g(x_ref) } else { g(x_ref) };
    pick(&mut best, v_ref, x_ref, maximize);

    if best.is_none() {
        // Last-resort numeric sweep.
        let (v, x) = multi_restart_minimize(&g, scan_lo, scan_hi, 20);
        let v = if maximize { -v } else { v };
        pick(&mut best, v, x, maximize);
    }

    match best {
        Some((value, x0)) => Ok(Value::List(vec![
            super::real(value),
            Value::List(vec![Value::Rule {
                lhs: Box::new(Value::Symbol(var)),
                rhs: Box::new(super::real(x0)),
                delayed: false,
            }]),
        ])),
        None => Err(EvalError::Error(format!("{name}: no extremum found"))),
    }
}

/// Minimize[f, x] / Minimize[f, {x, xmin, xmax}] — global symbolic minimum.
pub fn builtin_minimize(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    symbolic_extremum(args, env, false, "Minimize")
}

/// Maximize[f, x] / Maximize[f, {x, xmin, xmax}] — global symbolic maximum.
pub fn builtin_maximize(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    symbolic_extremum(args, env, true, "Maximize")
}

/// NMinimize[f, {x, xmin, xmax}] — global numeric minimization.
pub fn builtin_nminimize(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "NMinimize requires exactly 2 arguments".to_string(),
        ));
    }

    let expr = unheld(&args[0]);
    let spec_val = deep_unheld(&args[1]);
    let spec = match &spec_val {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Second argument must be a list: {var, x0} or {var, xmin, xmax}".to_string(),
            ));
        }
    };

    if spec.len() != 2 && spec.len() != 3 {
        return Err(EvalError::Error(
            "Search specification must have 2 or 3 elements: {var, x0} or {var, xmin, xmax}"
                .to_string(),
        ));
    }

    let var = resolve_var(&spec[0], &[&expr])?;
    let var_clone = var.clone();
    let f = move |x: f64| {
        let substituted = substitute_numeric(&expr, &var_clone, x);
        eval_expr_to_f64(&substituted, env).unwrap_or(f64::INFINITY)
    };

    let (f_min, x_min) = if spec.len() == 2 {
        let x0 = spec_number(&spec[1], "NMinimize", 1, env)?;
        let x_min = stationary_or_local(&f, x0);
        (f(x_min), x_min)
    } else {
        let xmin = spec_number(&spec[1], "NMinimize", 1, env)?;
        let xmax = spec_number(&spec[2], "NMinimize", 2, env)?;
        if xmin >= xmax {
            return Err(EvalError::Error(
                "NMinimize: interval must satisfy xmin < xmax".to_string(),
            ));
        }
        multi_restart_minimize(&f, xmin, xmax, 10)
    };

    Ok(Value::List(vec![
        super::real(f_min),
        Value::List(vec![Value::Rule {
            lhs: Box::new(Value::Symbol(var)),
            rhs: Box::new(super::real(x_min)),
            delayed: false,
        }]),
    ]))
}

/// NMaximize[f, {x, xmin, xmax}] — global numeric maximization.
pub fn builtin_nmaximize(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "NMaximize requires exactly 2 arguments".to_string(),
        ));
    }

    let expr = unheld(&args[0]);
    let spec_val = deep_unheld(&args[1]);
    let spec = match &spec_val {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Second argument must be a list: {var, x0} or {var, xmin, xmax}".to_string(),
            ));
        }
    };

    if spec.len() != 2 && spec.len() != 3 {
        return Err(EvalError::Error(
            "Search specification must have 2 or 3 elements: {var, x0} or {var, xmin, xmax}"
                .to_string(),
        ));
    }

    let var = resolve_var(&spec[0], &[&expr])?;
    let var_clone = var.clone();
    let f = move |x: f64| {
        let substituted = substitute_numeric(&expr, &var_clone, x);
        eval_expr_to_f64(&substituted, env).unwrap_or(f64::NEG_INFINITY)
    };

    let neg_f = |x: f64| -f(x);
    let (f_max, x_max) = if spec.len() == 2 {
        let x0 = spec_number(&spec[1], "NMaximize", 1, env)?;
        let x_max = stationary_or_local(&neg_f, x0);
        (f(x_max), x_max)
    } else {
        let xmin = spec_number(&spec[1], "NMaximize", 1, env)?;
        let xmax = spec_number(&spec[2], "NMaximize", 2, env)?;
        if xmin >= xmax {
            return Err(EvalError::Error(
                "NMaximize: interval must satisfy xmin < xmax".to_string(),
            ));
        }
        let (neg_f_min, x_max) = multi_restart_minimize(&neg_f, xmin, xmax, 10);
        (-neg_f_min, x_max)
    };

    Ok(Value::List(vec![
        super::real(f_max),
        Value::List(vec![Value::Rule {
            lhs: Box::new(Value::Symbol(var)),
            rhs: Box::new(super::real(x_max)),
            delayed: false,
        }]),
    ]))
}

/// ArgMin[f, {x, xmin, xmax}] — return just the minimizing x value.
pub fn builtin_argmin(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ArgMin requires exactly 2 arguments".to_string(),
        ));
    }

    let expr = unheld(&args[0]);
    let spec_val = deep_unheld(&args[1]);
    let spec = match &spec_val {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Second argument must be a list: {var, x0} or {var, xmin, xmax}".to_string(),
            ));
        }
    };

    if spec.len() != 2 && spec.len() != 3 {
        return Err(EvalError::Error(
            "Search specification must have 2 or 3 elements: {var, x0} or {var, xmin, xmax}"
                .to_string(),
        ));
    }

    let var = resolve_var(&spec[0], &[&expr])?;
    let f = move |x: f64| {
        let substituted = substitute_numeric(&expr, &var, x);
        eval_expr_to_f64(&substituted, env).unwrap_or(f64::INFINITY)
    };

    let x_min = if spec.len() == 2 {
        let x0 = spec_number(&spec[1], "ArgMin", 1, env)?;
        stationary_or_local(&f, x0)
    } else {
        let xmin = spec_number(&spec[1], "ArgMin", 1, env)?;
        let xmax = spec_number(&spec[2], "ArgMin", 2, env)?;
        if xmin >= xmax {
            return Err(EvalError::Error(
                "ArgMin: interval must satisfy xmin < xmax".to_string(),
            ));
        }
        let (_f_min, x_min) = multi_restart_minimize(&f, xmin, xmax, 10);
        x_min
    };

    Ok(super::real(x_min))
}

/// ArgMax[f, {x, xmin, xmax}] — return just the maximizing x value.
pub fn builtin_argmax(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ArgMax requires exactly 2 arguments".to_string(),
        ));
    }

    let expr = unheld(&args[0]);
    let spec_val = deep_unheld(&args[1]);
    let spec = match &spec_val {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Second argument must be a list: {var, x0} or {var, xmin, xmax}".to_string(),
            ));
        }
    };

    if spec.len() != 2 && spec.len() != 3 {
        return Err(EvalError::Error(
            "Search specification must have 2 or 3 elements: {var, x0} or {var, xmin, xmax}"
                .to_string(),
        ));
    }

    let var = resolve_var(&spec[0], &[&expr])?;
    let f = move |x: f64| {
        let substituted = substitute_numeric(&expr, &var, x);
        eval_expr_to_f64(&substituted, env).unwrap_or(f64::NEG_INFINITY)
    };

    let neg_f = |x: f64| -f(x);
    let x_max = if spec.len() == 2 {
        let x0 = spec_number(&spec[1], "ArgMax", 1, env)?;
        stationary_or_local(&neg_f, x0)
    } else {
        let xmin = spec_number(&spec[1], "ArgMax", 1, env)?;
        let xmax = spec_number(&spec[2], "ArgMax", 2, env)?;
        if xmin >= xmax {
            return Err(EvalError::Error(
                "ArgMax: interval must satisfy xmin < xmax".to_string(),
            ));
        }
        let (_neg_f_min, x_max) = multi_restart_minimize(&neg_f, xmin, xmax, 10);
        x_max
    };

    Ok(super::real(x_max))
}

/// FindInstance[eqn, var, n] — find n numeric instances (solutions).
pub fn builtin_find_instance(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let (n_val, eqn_arg, var_arg) = if args.len() == 3 {
        (unheld(&args[2]), unheld(&args[0]), deep_unheld(&args[1]))
    } else if args.len() == 2 {
        (
            Value::Integer(Integer::from(1)),
            unheld(&args[0]),
            deep_unheld(&args[1]),
        )
    } else {
        return Err(EvalError::Error(
            "FindInstance requires 2 or 3 arguments".to_string(),
        ));
    };

    let n = match &n_val {
        Value::Integer(i) => {
            let v = i.to_usize().unwrap_or(1);
            if v == 0 { 1 } else { v }
        }
        _ => 1,
    };

    let (expr, rhs) = parse_equation(&eqn_arg)?;
    let var = resolve_var(&var_arg, &[&expr])?;
    let var_clone = var.clone();

    let f = move |x: f64| {
        let substituted = substitute_numeric(&expr, &var_clone, x);
        eval_expr_to_f64(&substituted, env).unwrap_or(f64::INFINITY) - rhs
    };

    let mut solutions: Vec<f64> = Vec::new();
    let search_lo = -10.0;
    let search_hi = 10.0;
    let range = search_hi - search_lo;

    for i in 0..n.max(20) {
        let start = search_lo + (i as f64 / 20.0) * range;
        if let Ok(root) = newton_method(&f, start) {
            if f(root).abs() < 1e-6 {
                let is_duplicate = solutions.iter().any(|&s| (s - root).abs() < 1e-6);
                if !is_duplicate && root >= search_lo && root <= search_hi {
                    solutions.push(root);
                }
            }
            if solutions.len() >= n {
                break;
            }
        }
    }

    let instances: Vec<Value> = solutions
        .into_iter()
        .map(|root| {
            Value::List(vec![Value::Rule {
                lhs: Box::new(Value::Symbol(var.clone())),
                rhs: Box::new(super::real(root)),
                delayed: false,
            }])
        })
        .collect();

    Ok(Value::List(instances))
}

/// NSolve[eqn, var] — numeric solve for all roots.
pub fn builtin_nsolve(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "NSolve requires exactly 2 arguments: NSolve[eqn, var]".to_string(),
        ));
    }

    let eqn = unheld(&args[0]);
    let var_val = deep_unheld(&args[1]);
    let (expr, rhs) = parse_equation(&eqn)?;
    let var = resolve_var(&var_val, &[&expr])?;

    let poly_expr = if rhs == 0.0 {
        expr.clone()
    } else {
        Value::Call {
            head: "Plus".to_string(),
            args: vec![
                expr.clone(),
                Value::Call {
                    head: "Times".to_string(),
                    args: vec![super::real(-rhs)],
                },
            ],
        }
    };

    let coeffs = extract_polynomial_coeffs(&poly_expr, &var);

    match coeffs {
        Some(coeffs) => {
            if coeffs.is_empty() || coeffs.len() == 1 {
                return Ok(Value::List(Vec::new()));
            }

            let mut coeffs_trimmed = coeffs;
            while coeffs_trimmed.len() > 1 && coeffs_trimmed.last().is_none_or(|&c| c.abs() < 1e-15)
            {
                coeffs_trimmed.pop();
            }

            let degree = coeffs_trimmed.len() - 1;

            if degree == 0 {
                return Ok(Value::List(Vec::new()));
            }

            if degree == 1 {
                let c0 = coeffs_trimmed[0];
                let c1 = coeffs_trimmed[1];
                if c1.abs() < 1e-15 {
                    return Ok(Value::List(Vec::new()));
                }
                let root = -c0 / c1;
                return Ok(Value::List(vec![Value::List(vec![Value::Rule {
                    lhs: Box::new(Value::Symbol(var)),
                    rhs: Box::new(super::real(root)),
                    delayed: false,
                }])]));
            }

            let c_n = coeffs_trimmed[coeffs_trimmed.len() - 1];
            if c_n.abs() < 1e-15 {
                return Ok(Value::List(Vec::new()));
            }

            let normalized: Vec<f64> = coeffs_trimmed.iter().map(|&c| c / c_n).collect();

            let n = degree;
            let mut matrix = vec![vec![0.0; n]; n];

            for i in 0..n - 1 {
                matrix[i][i + 1] = 1.0;
            }

            for j in 0..n {
                matrix[n - 1][j] = -normalized[j];
            }

            let eigenvalues = qr_eigenvalues(&mut matrix, n);

            let mut real_roots: Vec<f64> = Vec::new();
            for (re, im) in eigenvalues {
                if im.abs() < 1e-8 {
                    let is_dup = real_roots.iter().any(|&r| (r - re).abs() < 1e-8);
                    if !is_dup {
                        real_roots.push(re);
                    }
                }
            }

            real_roots.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let solutions: Vec<Value> = real_roots
                .into_iter()
                .map(|root| {
                    Value::List(vec![Value::Rule {
                        lhs: Box::new(Value::Symbol(var.clone())),
                        rhs: Box::new(super::real(root)),
                        delayed: false,
                    }])
                })
                .collect();

            Ok(Value::List(solutions))
        }
        None => {
            let var_clone = var.clone();
            let expr_clone = expr.clone();
            let rhs_clone = rhs;
            let f = move |x: f64| {
                let substituted = substitute_numeric(&expr_clone, &var_clone, x);
                eval_expr_to_f64(&substituted, env).unwrap_or(f64::INFINITY) - rhs_clone
            };

            let mut roots: Vec<f64> = Vec::new();

            let grid_size = 20;
            let search_lo = -10.0;
            let search_hi = 10.0;
            let step = (search_hi - search_lo) / grid_size as f64;

            for i in 0..grid_size {
                let a = search_lo + (i as f64) * step;
                let b = a + step;
                let fa = f(a);
                let fb = f(b);
                if fa * fb <= 0.0
                    && let Ok(root) = brent_method(&f, a, b)
                {
                    let is_dup = roots.iter().any(|&r| (r - root).abs() < 1e-6);
                    if !is_dup {
                        roots.push(root);
                    }
                }
            }

            for i in 0..grid_size {
                let start =
                    search_lo + ((i as f64 + 0.5) / grid_size as f64) * (search_hi - search_lo);
                if let Ok(root) = newton_method(&f, start)
                    && f(root).abs() < 1e-6
                {
                    let is_dup = roots.iter().any(|&r| (r - root).abs() < 1e-6);
                    if !is_dup {
                        roots.push(root);
                    }
                }
            }

            roots.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let solutions: Vec<Value> = roots
                .into_iter()
                .map(|root| {
                    Value::List(vec![Value::Rule {
                        lhs: Box::new(Value::Symbol(var.clone())),
                        rhs: Box::new(super::real(root)),
                        delayed: false,
                    }])
                })
                .collect();

            Ok(Value::List(solutions))
        }
    }
}

fn extract_polynomial_coeffs(expr: &Value, var: &str) -> Option<Vec<f64>> {
    match expr {
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) | Value::Complex { .. } => {
            super::to_f64(expr).map(|c| vec![c])
        }
        Value::Symbol(s) if s == var => Some(vec![0.0, 1.0]),
        Value::Symbol(_) => None,
        Value::Call { head, args } => match head.as_str() {
            "Plus" => {
                let mut coeffs: Vec<f64> = vec![0.0; 1];
                for arg in args {
                    {
                        let sub = extract_polynomial_coeffs(arg, var)?;
                        if sub.len() > coeffs.len() {
                            coeffs.resize(sub.len(), 0.0);
                        }
                        for (i, &c) in sub.iter().enumerate() {
                            coeffs[i] += c;
                        }
                    }
                }
                while coeffs.len() > 1 && coeffs.last().is_some_and(|&c| c.abs() < 1e-15) {
                    coeffs.pop();
                }
                Some(coeffs)
            }
            "Times" => {
                let mut numeric_factor = 1.0;
                let mut poly_part: Option<Vec<f64>> = None;

                for arg in args {
                    if let Some(c) = super::to_f64(arg) {
                        numeric_factor *= c;
                    } else if let Value::Symbol(s) = arg {
                        if s == var {
                            poly_part = Some(vec![0.0, 1.0]);
                        } else {
                            return None;
                        }
                    } else if let Value::Call {
                        head: inner_head,
                        args: inner_args,
                    } = arg
                    {
                        if inner_head == "Power" && inner_args.len() == 2 {
                            match (&inner_args[0], &inner_args[1]) {
                                (Value::Symbol(s), Value::Integer(exp))
                                    if s == var && !exp.is_negative() =>
                                {
                                    let exp_usize = exp.to_usize().unwrap_or(0);
                                    let mut p = vec![0.0; exp_usize + 1];
                                    p[exp_usize] = 1.0;
                                    poly_part = Some(p);
                                }
                                (Value::Symbol(s), _) if s == var => {
                                    return None;
                                }
                                _ => {
                                    return None;
                                }
                            }
                        } else if inner_head == "Times" {
                            {
                                let sub_coeffs = extract_polynomial_coeffs(arg, var)?;
                                if let Some(ref mut pp) = poly_part {
                                    let multiplied = multiply_polys(pp, &sub_coeffs);
                                    *pp = multiplied;
                                } else {
                                    poly_part = Some(sub_coeffs);
                                }
                            }
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }

                if let Some(mut pp) = poly_part {
                    for c in pp.iter_mut() {
                        *c *= numeric_factor;
                    }
                    Some(pp)
                } else {
                    Some(vec![numeric_factor])
                }
            }
            "Power" if args.len() == 2 => match (&args[0], &args[1]) {
                (Value::Symbol(s), Value::Integer(exp)) if s == var && !exp.is_negative() => {
                    let exp_usize = exp.to_usize().unwrap_or(0);
                    let mut coeffs = vec![0.0; exp_usize + 1];
                    coeffs[exp_usize] = 1.0;
                    Some(coeffs)
                }
                (Value::Symbol(s), _) if s == var => None,
                _ => super::to_f64(&args[0]).map(|c| vec![c]),
            },
            "Divide" if args.len() == 2 => {
                let num_coeffs = extract_polynomial_coeffs(&args[0], var)?;
                let denom = super::to_f64(&args[1])?;
                if denom.abs() < 1e-15 {
                    return None;
                }
                Some(num_coeffs.iter().map(|&c| c / denom).collect())
            }
            _ => None,
        },
        _ => None,
    }
}

fn multiply_polys(a: &[f64], b: &[f64]) -> Vec<f64> {
    let len = a.len() + b.len() - 1;
    let mut result = vec![0.0; len];
    for (i, &ai) in a.iter().enumerate() {
        for (j, &bj) in b.iter().enumerate() {
            result[i + j] += ai * bj;
        }
    }
    result
}

/// Eigenvalues of a square matrix.
///
/// Reduces the matrix to upper Hessenberg form, then iterates the implicitly
/// shifted QR algorithm, deflating converged 1x1 (real) and 2x2 (possibly
/// complex) trailing blocks. Exactly `n` `(re, im)` pairs are returned, so a
/// repeated eigenvalue appears once per multiplicity.
#[allow(clippy::needless_range_loop)]
pub(crate) fn qr_eigenvalues(matrix: &mut [Vec<f64>], n: usize) -> Vec<(f64, f64)> {
    let mut a = matrix.to_owned();
    let mut eigenvalues: Vec<(f64, f64)> = Vec::with_capacity(n);

    if n == 0 {
        return eigenvalues;
    }

    hessenberg(&mut a, n);

    let tol = 1e-12;
    let max_iter = 2000 * n;
    // The active block is `a[lo..hi][lo..hi]`; everything at or above `hi` is done.
    let mut hi = n;
    let mut iter = 0;

    while hi > 0 {
        if hi == 1 {
            eigenvalues.push((a[0][0], 0.0));
            break;
        }

        // Split off the trailing block at the first negligible subdiagonal entry.
        let mut lo = hi - 1;
        while lo > 0 {
            let scale = a[lo - 1][lo - 1].abs() + a[lo][lo].abs();
            if a[lo][lo - 1].abs() <= tol * scale.max(1.0) {
                break;
            }
            lo -= 1;
        }

        match hi - lo {
            1 => {
                eigenvalues.push((a[hi - 1][hi - 1], 0.0));
                hi -= 1;
                iter = 0;
            }
            2 => {
                eigenvalues.extend(eig2x2(
                    a[lo][lo],
                    a[lo][lo + 1],
                    a[lo + 1][lo],
                    a[lo + 1][lo + 1],
                ));
                hi -= 2;
                iter = 0;
            }
            _ => {
                iter += 1;
                if iter > max_iter {
                    eigenvalues.extend((lo..hi).map(|i| (a[i][i], 0.0)));
                    hi = lo;
                    iter = 0;
                    continue;
                }

                let mut shift = wilkinson_shift(
                    a[hi - 2][hi - 2],
                    a[hi - 2][hi - 1],
                    a[hi - 1][hi - 2],
                    a[hi - 1][hi - 1],
                );
                // A zero shift leaves an orthogonal block invariant; nudge it off.
                let scale = a[hi - 2][hi - 2].abs()
                    + a[hi - 1][hi - 1].abs()
                    + a[hi - 2][hi - 1].abs()
                    + a[hi - 1][hi - 2].abs();
                if shift.abs() <= 1e-14 * scale.max(1.0) {
                    shift += 0.75 * scale.max(1.0);
                }
                qr_step(&mut a, lo, hi, shift);
            }
        }
    }

    // Every eigenvalue is recorded exactly once even if a step never converged.
    eigenvalues.resize(n, (0.0, 0.0));
    eigenvalues.truncate(n);
    eigenvalues
}

/// Eigenvalues of a real 2x2 block, as `(re, im)` pairs.
fn eig2x2(a: f64, b: f64, c: f64, d: f64) -> [(f64, f64); 2] {
    let tr = a + d;
    let det = a * d - b * c;
    let disc = tr * tr - 4.0 * det;
    if disc >= 0.0 {
        let sqrt_disc = disc.sqrt();
        [((tr + sqrt_disc) / 2.0, 0.0), ((tr - sqrt_disc) / 2.0, 0.0)]
    } else {
        let re = tr / 2.0;
        let im = (-disc).sqrt() / 2.0;
        [(re, im), (re, -im)]
    }
}

/// Wilkinson shift: the eigenvalue of the trailing 2x2 block nearest its
/// bottom-right entry, or the block mean when the pair is complex.
fn wilkinson_shift(a: f64, b: f64, c: f64, d: f64) -> f64 {
    let tr = a + d;
    let det = a * d - b * c;
    let disc = tr * tr - 4.0 * det;
    if disc >= 0.0 {
        let sqrt_disc = disc.sqrt();
        let r1 = (tr + sqrt_disc) / 2.0;
        let r2 = (tr - sqrt_disc) / 2.0;
        if (r1 - d).abs() < (r2 - d).abs() {
            r1
        } else {
            r2
        }
    } else {
        tr / 2.0
    }
}

/// One implicitly shifted QR step on the Hessenberg block `a[lo..hi][lo..hi]`.
#[allow(clippy::needless_range_loop)] // row/column index arithmetic reads clearer here.
fn qr_step(a: &mut [Vec<f64>], lo: usize, hi: usize, shift: f64) {
    let m = hi - lo;
    let mut q = identity_matrix(m);
    let mut r = zero_matrix(m);
    for i in 0..m {
        for j in 0..m {
            r[i][j] = a[lo + i][lo + j];
        }
        r[i][i] -= shift;
    }

    // Reduce `r` to upper triangular with Givens rotations, accumulating them in `q`.
    for i in 0..m - 1 {
        let x = r[i][i];
        let y = r[i + 1][i];
        let norm = (x * x + y * y).sqrt();
        let (c, s) = if norm < 1e-300 {
            (1.0, 0.0)
        } else {
            (x / norm, y / norm)
        };
        for j in 0..m {
            let t = r[i][j] * c + r[i + 1][j] * s;
            r[i + 1][j] = -r[i][j] * s + r[i + 1][j] * c;
            r[i][j] = t;
        }
        for j in 0..m {
            let t = q[j][i] * c + q[j][i + 1] * s;
            q[j][i + 1] = -q[j][i] * s + q[j][i + 1] * c;
            q[j][i] = t;
        }
    }

    // `a[block] = R Q + shift I`, similar to the original block.
    for i in 0..m {
        for j in 0..m {
            let mut acc = 0.0;
            for k in 0..m {
                acc += r[i][k] * q[k][j];
            }
            a[lo + i][lo + j] = acc;
        }
        a[lo + i][lo + i] += shift;
    }
}

fn identity_matrix(n: usize) -> Vec<Vec<f64>> {
    let mut m = vec![vec![0.0; n]; n];
    for (i, row) in m.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    m
}

fn zero_matrix(n: usize) -> Vec<Vec<f64>> {
    vec![vec![0.0; n]; n]
}

#[allow(clippy::needless_range_loop)] // Householder column/row index arithmetic.
fn hessenberg(a: &mut [Vec<f64>], n: usize) {
    for k in 0..n - 2 {
        let mut col = Vec::new();
        for i in k + 1..n {
            col.push(a[i][k]);
        }

        let norm = col.iter().map(|&x| x * x).sum::<f64>().sqrt();
        if norm < 1e-30 {
            continue;
        }

        let sign = if col[0] >= 0.0 { 1.0 } else { -1.0 };
        col[0] += sign * norm;
        let hh_norm_sq: f64 = col.iter().map(|&x| x * x).sum();
        if hh_norm_sq < 1e-60 {
            continue;
        }

        for j in 0..n {
            let dot: f64 = col
                .iter()
                .zip(a.iter().skip(k + 1))
                .map(|(&c, row)| c * row[j])
                .sum();
            let factor = 2.0 * dot / hh_norm_sq;
            for i in k + 1..n {
                a[i][j] -= col[i - k - 1] * factor;
            }
        }

        for i in 0..n {
            let mut dot_right = 0.0;
            for j in k + 1..n {
                dot_right += col[j - k - 1] * a[i][j];
            }
            let factor = 2.0 * dot_right / hh_norm_sq;
            for j in k + 1..n {
                a[i][j] -= col[j - k - 1] * factor;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;
    use crate::value::DEFAULT_PRECISION;
    use rug::Float;
    use rug::Integer;

    fn env_with_builtins() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    fn eval_str(input: &str) -> Value {
        let env = env_with_builtins();
        let tokens = crate::lexer::tokenize(input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap_or_else(|e| panic!("parse {input}: {e:?}"));
        crate::eval::eval_program(&ast, &env).unwrap_or_else(|e| panic!("eval {input}: {e:?}"))
    }

    fn num(input: &str) -> f64 {
        super::super::to_f64(&eval_str(input)).unwrap_or_else(|| panic!("not numeric: {input}"))
    }

    fn assert_close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-7, "expected {b}, got {a}");
    }

    #[test]
    fn test_local_minimize_from_start() {
        let f = |x: f64| (x - 2.0) * (x - 2.0);
        let x = local_minimize(&f, 0.0);
        assert!((x - 2.0).abs() < 1e-7, "expected 2, got {x}");
    }

    #[test]
    fn test_bracket_min_encloses_minimum() {
        let f = |x: f64| (x - 5.0) * (x - 5.0);
        let (a, b, c) = bracket_min(&f, 0.0);
        assert!(a <= 5.0 && 5.0 <= c, "bracket {a}..{c} misses 5");
        assert!(f(b) <= f(a) && f(b) <= f(c));
    }

    #[test]
    fn test_scan_for_root_cos() {
        let f = |x: f64| x.cos() - x;
        let root = scan_for_root(&f, 0.0).unwrap();
        assert!((root - 0.739_085_133_215_160_6).abs() < 1e-8);
    }

    #[test]
    fn test_unheld_strips_pattern() {
        let pattern = Value::Pattern(Expr::Call {
            head: Box::new(Expr::Symbol("Power".to_string())),
            args: vec![
                Expr::Symbol("x".to_string()),
                Expr::Integer(Integer::from(2)),
            ],
        });
        let value = unheld(&pattern);
        assert!(matches!(value, Value::Call { ref head, .. } if head == "Power"));
    }

    #[test]
    fn test_eval_find_minimum_start_point() {
        assert_close(num("First[FindMinimum[(x - 2)^2, {x, 0}]]"), 0.0);
        // Result has the Wolfram shape `{value, {x -> x0}}`.
        assert_close(num("FindMinimum[(x - 2)^2, {x, 0}][[2, 1, 2]]"), 2.0);
    }

    #[test]
    fn test_eval_find_minimum_interval() {
        assert_close(num("First[FindMinimum[(x - 3)^2 + 1, {x, 0, 10}]]"), 1.0);
    }

    #[test]
    fn test_eval_find_maximum_start_point() {
        assert_close(num("First[FindMaximum[3 - (x - 1)^2, {x, 0}]]"), 3.0);
        assert_close(num("First[FindMaximum[3 - (x - 1)^2, {x, -5, 5}]]"), 3.0);
    }

    #[test]
    fn test_eval_minimize_maximize_symbolic() {
        assert_close(num("First[Minimize[x^2 - 2 x, x]]"), -1.0);
        assert_close(num("Minimize[x^2 - 2 x, x][[2, 1, 2]]"), 1.0);
        assert_close(num("First[Maximize[-(x - 1)^2 + 3, x]]"), 3.0);
        assert_close(num("First[Minimize[(x - 4)^2, {x, 0, 10}]]"), 0.0);
    }

    #[test]
    fn test_eval_nminimize_nmaximize() {
        assert_close(num("First[NMinimize[x^2 - 4 x + 7, {x, -20, 20}]]"), 3.0);
        assert_close(num("First[NMaximize[4 x - x^2 + 1, {x, -20, 20}]]"), 5.0);
    }

    #[test]
    fn test_eval_find_root_equation() {
        assert_close(
            num("FindRoot[Cos[x] == x, {x, 1}][[1, 2]]"),
            0.739_085_133_215_160_6,
        );
        // Bare expression form is treated as `f == 0`.
        assert_close(num("FindRoot[x^2 - 4, {x, 3}][[1, 2]]"), 2.0);
        // Interval form uses Brent's method.
        assert_close(num("FindRoot[x^2 - 4, {x, 1, 4}][[1, 2]]"), 2.0);
    }

    #[test]
    fn test_eval_nsolve_equation() {
        // `==` must survive into NSolve (requires HoldAll).
        assert_close(num("x /. NSolve[x^2 - 4 == 0, x][[1]]"), -2.0);
        assert_close(num("x /. NSolve[x^2 - 4 == 0, x][[2]]"), 2.0);
        assert_close(num("x /. NSolve[2 x + 1 == 0, x][[1]]"), -0.5);
    }

    #[test]
    fn test_golden_section_simple() {
        let f = |x: f64| x * x;
        let (f_min, x_min) = golden_section(f, -1.0, 1.0);
        assert!(
            (x_min - 0.0).abs() < 1e-10,
            "Expected x_min near 0, got {}",
            x_min
        );
        assert!(f_min.abs() < 1e-10, "Expected f_min near 0, got {}", f_min);
    }

    #[test]
    fn test_golden_section_quadratic() {
        let f = |x: f64| (x - 2.0) * (x - 2.0) + 3.0;
        let (f_min, x_min) = golden_section(f, 0.0, 4.0);
        assert!(
            (x_min - 2.0).abs() < 1e-10,
            "Expected x_min near 2, got {}",
            x_min
        );
        assert!(
            (f_min - 3.0).abs() < 1e-10,
            "Expected f_min near 3, got {}",
            f_min
        );
    }

    #[test]
    fn test_golden_section_offset_offcentre() {
        // A large constant offset makes `f(c)` and `f(d)` tie once the bracket
        // is tiny; the minimiser must keep the true minimum in the bracket
        // instead of drifting away from it.
        let f = |x: f64| (x - 0.7) * (x - 0.7) + 17.0;
        let (f_min, x_min) = golden_section(f, 0.0, 1.0);
        assert!(
            (x_min - 0.7).abs() < 1e-7,
            "Expected x_min near 0.7, got {}",
            x_min
        );
        assert!(
            (f_min - 17.0).abs() < 1e-10,
            "Expected f_min near 17, got {}",
            f_min
        );
    }

    #[test]
    fn test_newton_method_simple() {
        let f = |x: f64| x * x - 4.0;
        let root = newton_method(&f, 3.0).unwrap();
        assert!(
            (root - 2.0).abs() < 1e-10,
            "Expected root near 2, got {}",
            root
        );
    }

    #[test]
    fn test_newton_method_cos() {
        let f = |x: f64| x.cos();
        let root = newton_method(&f, 1.0).unwrap();
        assert!(
            (root - std::f64::consts::PI / 2.0).abs() < 1e-10,
            "Expected root near pi/2, got {}",
            root
        );
    }

    #[test]
    fn test_brent_method_simple() {
        let f = |x: f64| x * x - 4.0;
        let root = brent_method(&f, 1.0, 3.0).unwrap();
        assert!(
            (root - 2.0).abs() < 1e-10,
            "Expected root near 2, got {}",
            root
        );
    }

    #[test]
    fn test_brent_method_negative_root() {
        let f = |x: f64| x * x - 4.0;
        let root = brent_method(&f, -3.0, -1.0).unwrap();
        assert!(
            (root + 2.0).abs() < 1e-10,
            "Expected root near -2, got {}",
            root
        );
    }

    #[test]
    fn test_brent_method_sign_check() {
        let f = |x: f64| x * x + 1.0;
        let result = brent_method(&f, 0.0, 1.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_numerical_derivative() {
        let f = |x: f64| x * x * x;
        let df2 = numerical_derivative(&f, 2.0);
        assert!(
            (df2 - 12.0).abs() < 1e-6,
            "Expected derivative 12 at x=2, got {}",
            df2
        );
    }

    #[test]
    fn test_multi_restart_minimize() {
        let f = |x: f64| x.sin();
        let (f_min, x_min) = multi_restart_minimize(&f, 0.0, 2.0 * std::f64::consts::PI, 10);
        assert!(
            (x_min - 3.0 * std::f64::consts::PI / 2.0).abs() < 1e-4,
            "Expected x_min near 3*pi/2, got {}",
            x_min
        );
        assert!(
            (f_min + 1.0).abs() < 1e-4,
            "Expected f_min near -1, got {}",
            f_min
        );
    }

    #[test]
    fn test_substitute_numeric() {
        let expr = Value::Call {
            head: "Sin".to_string(),
            args: vec![Value::Symbol("x".to_string())],
        };
        let result = substitute_numeric(&expr, "x", 3.0);
        match result {
            Value::Call { head, ref args } => {
                assert_eq!(head, "Sin");
                assert!(matches!(&args[0], Value::Real(_)));
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_to_f64() {
        assert_eq!(
            super::super::to_f64(&Value::Integer(Integer::from(42))),
            Some(42.0)
        );
        assert_eq!(
            super::super::to_f64(&Value::Real(Float::with_val(
                DEFAULT_PRECISION,
                std::f64::consts::PI
            ))),
            Some(std::f64::consts::PI)
        );
        assert_eq!(super::super::to_f64(&Value::Symbol("x".to_string())), None);
    }

    #[test]
    fn test_extract_polynomial_coeffs_linear() {
        let expr = Value::Call {
            head: "Plus".to_string(),
            args: vec![
                Value::Call {
                    head: "Times".to_string(),
                    args: vec![
                        Value::Integer(Integer::from(2)),
                        Value::Symbol("x".to_string()),
                    ],
                },
                Value::Integer(Integer::from(3)),
            ],
        };
        let coeffs = extract_polynomial_coeffs(&expr, "x").unwrap();
        assert_eq!(coeffs, vec![3.0, 2.0]);
    }

    #[test]
    fn test_extract_polynomial_coeffs_quadratic() {
        let expr = Value::Call {
            head: "Plus".to_string(),
            args: vec![
                Value::Call {
                    head: "Power".to_string(),
                    args: vec![
                        Value::Symbol("x".to_string()),
                        Value::Integer(Integer::from(2)),
                    ],
                },
                Value::Call {
                    head: "Times".to_string(),
                    args: vec![
                        Value::Integer(Integer::from(-5)),
                        Value::Symbol("x".to_string()),
                    ],
                },
                Value::Integer(Integer::from(6)),
            ],
        };
        let coeffs = extract_polynomial_coeffs(&expr, "x").unwrap();
        assert_eq!(coeffs, vec![6.0, -5.0, 1.0]);
    }

    #[test]
    fn test_parse_equation() {
        let eqn = Value::Call {
            head: "Equal".to_string(),
            args: vec![
                Value::Call {
                    head: "Sin".to_string(),
                    args: vec![Value::Symbol("x".to_string())],
                },
                Value::Integer(Integer::from(5)),
            ],
        };
        let (expr, rhs) = parse_equation(&eqn).unwrap();
        assert_eq!(rhs, 5.0);
        assert!(matches!(expr, Value::Call { ref head, .. } if head == "Sin"));

        let bare = Value::Call {
            head: "Sin".to_string(),
            args: vec![Value::Symbol("x".to_string())],
        };
        let (expr, rhs) = parse_equation(&bare).unwrap();
        assert_eq!(rhs, 0.0);
        assert!(matches!(expr, Value::Call { ref head, .. } if head == "Sin"));
    }

    #[test]
    fn test_qr_eigenvalues_symmetric() {
        let mut m = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let evals = qr_eigenvalues(&mut m, 2);
        assert_eq!(evals.len(), 2);
        for (re, im) in &evals {
            assert!(
                (re.abs() - 1.0).abs() < 1e-8,
                "Expected eigenvalue 1, got {}",
                re
            );
            assert!(im.abs() < 1e-8, "Expected imaginary 0, got {}", im);
        }
    }

    #[test]
    fn test_qr_eigenvalues_companion() {
        // Companion matrix of x^3 - 6x^2 + 11x - 6 = (x-1)(x-2)(x-3).
        let mut m = vec![
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
            vec![6.0, -11.0, 6.0],
        ];
        let mut evals = qr_eigenvalues(&mut m, 3);
        evals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        assert_eq!(evals.len(), 3);
        for ((re, im), want) in evals.iter().zip([1.0, 2.0, 3.0]) {
            assert!((re - want).abs() < 1e-8, "Expected {}, got {}", want, re);
            assert!(im.abs() < 1e-8, "Expected real eigenvalue, got im {}", im);
        }
    }

    #[test]
    fn test_qr_eigenvalues_quartic() {
        // A 4x4 companion matrix needing Hessenberg reduction and several
        // deflations: roots are 1, 2, 3, 4.
        let mut m = vec![
            vec![0.0, 1.0, 0.0, 0.0],
            vec![0.0, 0.0, 1.0, 0.0],
            vec![0.0, 0.0, 0.0, 1.0],
            vec![-24.0, 50.0, -35.0, 10.0],
        ];
        let mut evals = qr_eigenvalues(&mut m, 4);
        evals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        assert_eq!(evals.len(), 4);
        for ((re, im), want) in evals.iter().zip([1.0, 2.0, 3.0, 4.0]) {
            assert!((re - want).abs() < 1e-8, "Expected {}, got {}", want, re);
            assert!(im.abs() < 1e-8, "Expected real eigenvalue, got im {}", im);
        }
    }

    #[test]
    fn test_qr_eigenvalues_complex_pair() {
        // A rotation has eigenvalues ±i and must not stall on a zero shift.
        let mut m = vec![vec![0.0, -1.0], vec![1.0, 0.0]];
        let evals = qr_eigenvalues(&mut m, 2);
        assert_eq!(evals.len(), 2);
        for (re, im) in &evals {
            assert!(re.abs() < 1e-8, "Expected real part 0, got {}", re);
            assert!(
                (im.abs() - 1.0).abs() < 1e-8,
                "Expected imaginary ±1, got {}",
                im
            );
        }
    }

    #[test]
    fn test_multiply_polys() {
        let a = vec![1.0, 1.0];
        let b = vec![1.0, 1.0];
        let result = multiply_polys(&a, &b);
        assert_eq!(result, vec![1.0, 2.0, 1.0]);
    }
}
