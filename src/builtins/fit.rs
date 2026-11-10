//! numeric fitting/interpolation/optimization: Fit, FindFit, LeastSquares, LinearOptimization, Interpolation, MinValue/MaxValue.
//!
//! Gap-fill module; see the individual builtin docs for semantics.

#![allow(clippy::needless_range_loop)]

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::Integer;

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin_env(env, "Fit", builtin_fit);
    super::register_builtin_env(env, "FindFit", builtin_find_fit);
    super::register_builtin(env, "LeastSquares", builtin_least_squares);
    super::register_builtin(env, "LinearOptimization", builtin_linear_optimization);
    super::register_builtin(env, "LinearProgramming", builtin_linear_optimization);
    super::register_builtin_env(env, "Interpolation", builtin_interpolation);
    // `InterpolatingFunction` is owned by `dsolve::register`, which runs later.
    super::register_builtin_env(env, "MinValue", builtin_min_value);
    super::register_builtin_env(env, "MaxValue", builtin_max_value);
    super::register_builtin_env(env, "NonlinearModelFit", builtin_nonlinear_model_fit);
    super::register_builtin_env(env, "LinearModelFit", builtin_linear_model_fit);
    super::register_builtin_env(env, "FunctionInterpolation", builtin_function_interpolation);
    // Internal head used by `Interpolation`/`FunctionInterpolation` result
    // objects so that symbolic points (e.g. `Pi/2`) resolve numerically.
    super::register_builtin_env(env, "FitInterpolationFunction", builtin_fit_interp_eval);
    super::register_builtin_env(env, "FitInterpFunc2D", builtin_fit_interp_2d_eval);
    // `FittedModel` is the callable fit object returned by `NonlinearModelFit`;
    // `FitResiduals`/`Property` expose and dispatch its properties.
    super::register_builtin_env(env, "FittedModel", builtin_fitted_model);
    super::register_builtin_env(env, "FitResiduals", builtin_fit_residuals);
    super::register_builtin_env(env, "DesignMatrix", builtin_design_matrix);
    super::register_builtin_env(env, "Property", builtin_property);

    // These builtins must see their arguments unevaluated (basis functions,
    // model expressions and variables are symbolic).
    for name in [
        "Fit",
        "FindFit",
        "Interpolation",
        "MinValue",
        "MaxValue",
        "NonlinearModelFit",
        "LinearModelFit",
        "FunctionInterpolation",
    ] {
        env.set_attributes(name, vec!["HoldAll".to_string()]);
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn unevaluated(name: &str, args: &[Value]) -> Value {
    Value::Call {
        head: name.to_string(),
        args: args.to_vec(),
    }
}

fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

fn as_list(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::List(items) => Some(items),
        _ => None,
    }
}

/// Recover a runtime value from a possibly held/pattern-wrapped argument.
fn arg_to_value(v: &Value, env: &Env) -> Result<Value, EvalError> {
    match v {
        Value::Pattern(e) => crate::eval::eval(e, env),
        Value::Hold(b) | Value::HoldComplete(b) => Ok((**b).clone()),
        other => Ok(other.clone()),
    }
}

/// Recover an unevaluated value from a possibly held/pattern-wrapped argument.
fn arg_to_uneval(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => crate::pattern::unwrap_expr_to_value(e),
        Value::Hold(b) | Value::HoldComplete(b) => (**b).clone(),
        other => other.clone(),
    }
}

/// Recursively strip `Pattern`/`Hold` wrappers (introduced by HoldAll and by
/// rule evaluation) so numeric parsing sees plain values.
fn deep_unwrap(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => deep_unwrap(&crate::pattern::unwrap_expr_to_value(e)),
        Value::Hold(b) | Value::HoldComplete(b) => deep_unwrap(b),
        Value::List(items) => Value::List(items.iter().map(deep_unwrap).collect()),
        Value::Rule { lhs, rhs, delayed } => Value::Rule {
            lhs: Box::new(deep_unwrap(lhs)),
            rhs: Box::new(deep_unwrap(rhs)),
            delayed: *delayed,
        },
        _ => v.clone(),
    }
}

/// Substitute every occurrence of the symbol `name` in `expr` with `val`.
fn substitute(expr: &Value, name: &str, val: &Value) -> Value {
    match expr {
        Value::Symbol(s) if s == name => val.clone(),
        Value::List(items) => Value::List(items.iter().map(|a| substitute(a, name, val)).collect()),
        Value::Call { head, args } => Value::Call {
            head: head.clone(),
            args: args.iter().map(|a| substitute(a, name, val)).collect(),
        },
        _ => expr.clone(),
    }
}

/// Heuristic: does this value look like a (comparison) constraint?
fn is_constraint(v: &Value) -> bool {
    matches!(
        v,
        Value::Call { head, .. }
            if matches!(
                head.as_str(),
                "Greater"
                    | "Less"
                    | "GreaterEqual"
                    | "LessEqual"
                    | "Equal"
                    | "Unequal"
                    | "Inequality"
                    | "Element"
                    | "And"
                    | "Or"
            )
    )
}

/// Numerically evaluate an expression to `f64` (via N), returning `None` if the
/// result is not a real number.
fn eval_scalar(expr: &Value, env: &Env) -> Option<f64> {
    if let Some(x) = super::to_f64(expr) {
        return Some(x);
    }
    let n_expr = Expr::Call {
        head: Box::new(Expr::Symbol("N".to_string())),
        args: vec![crate::eval::table::value_to_expr(expr)],
    };
    match crate::eval::eval(&n_expr, env) {
        Ok(v) => super::to_f64(&v),
        Err(_) => None,
    }
}

/// Collect variable names from a symbol or a list of symbols.
fn collect_symbols(v: &Value) -> Option<Vec<String>> {
    match v {
        Value::Symbol(s) => Some(vec![s.clone()]),
        Value::List(items) => items
            .iter()
            .map(|it| match it {
                Value::Symbol(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

/// Extract a single coordinate (or the first coordinate of a point).
fn first_coord(v: &Value) -> Option<f64> {
    match v {
        Value::List(items) => items.first().and_then(super::to_f64),
        other => super::to_f64(other),
    }
}

/// Parse fitting data into (coordinates, value) pairs. Each data point is either
/// a rule `x -> y` / `{x1, …, xn} -> y` or a list `{x1, …, xn, y}`.
fn parse_points(data: &Value, nvars: usize) -> Option<(Vec<Vec<f64>>, Vec<f64>)> {
    let data = deep_unwrap(data);
    let items = as_list(&data)?;
    let mut coords = Vec::with_capacity(items.len());
    let mut ys = Vec::with_capacity(items.len());
    for it in items {
        let (c, y) = match it {
            Value::Rule { lhs, rhs, .. } => {
                let c = match lhs.as_ref() {
                    Value::List(v) => v.iter().map(super::to_f64).collect::<Option<Vec<_>>>()?,
                    other => vec![super::to_f64(other)?],
                };
                (c, super::to_f64(rhs)?)
            }
            Value::List(inner) if inner.len() >= 2 => {
                let y = super::to_f64(inner.last()?)?;
                let c = inner[..inner.len() - 1]
                    .iter()
                    .map(super::to_f64)
                    .collect::<Option<Vec<_>>>()?;
                (c, y)
            }
            _ => return None,
        };
        if c.len() != nvars {
            return None;
        }
        coords.push(c);
        ys.push(y);
    }
    Some((coords, ys))
}

// ── Linear algebra helpers ──────────────────────────────────────────────────

/// Solve a square `n×n` system `a x = b` by Gaussian elimination with partial
/// pivoting. `a` and `b` are consumed.
fn solve_linear_f64(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = a.len();
    if n == 0 {
        return Some(vec![]);
    }
    if b.len() != n {
        return None;
    }
    for col in 0..n {
        let mut pivot = col;
        let mut best = a[col][col].abs();
        for row in col + 1..n {
            if a[row][col].abs() > best {
                best = a[row][col].abs();
                pivot = row;
            }
        }
        if best < 1e-14 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let p = a[col][col];
        for j in col..n {
            a[col][j] /= p;
        }
        b[col] /= p;
        for row in 0..n {
            if row == col {
                continue;
            }
            let f = a[row][col];
            if f != 0.0 {
                for j in col..n {
                    a[row][j] -= f * a[col][j];
                }
                b[row] -= f * b[col];
            }
        }
    }
    Some(b)
}

/// Least-squares solution of `a x ≈ b` via normal equations with a tiny ridge
/// fallback for rank-deficient systems.
fn least_squares_f64(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let m = a.len();
    if m == 0 {
        return Some(vec![]);
    }
    let n = a[0].len();
    let mut jtj = vec![vec![0.0f64; n]; n];
    let mut jtr = vec![0.0f64; n];
    for i in 0..m {
        if a[i].len() != n {
            return None;
        }
        for p in 0..n {
            jtr[p] += a[i][p] * b[i];
            for q in 0..n {
                jtj[p][q] += a[i][p] * a[i][q];
            }
        }
    }
    if let Some(x) = solve_linear_f64(jtj.clone(), jtr.clone()) {
        return Some(x);
    }
    let mut scale = 0.0f64;
    for p in 0..n {
        scale = scale.max(jtj[p][p]);
    }
    let ridge = if scale > 0.0 { scale * 1e-10 } else { 1e-10 };
    for p in 0..n {
        jtj[p][p] += ridge;
    }
    solve_linear_f64(jtj, jtr)
}

/// Scale each row of `a` and entry of `b` by `sqrt(w_i)`, turning a weighted
/// least-squares problem into an ordinary one.
fn scale_by_weights(a: &[Vec<f64>], b: &[f64], w: &[f64]) -> Option<(Vec<Vec<f64>>, Vec<f64>)> {
    if w.len() != a.len() || b.len() != a.len() {
        return None;
    }
    let mut aa = Vec::with_capacity(a.len());
    let mut bb = Vec::with_capacity(b.len());
    for i in 0..a.len() {
        let s = w[i].max(0.0).sqrt();
        aa.push(a[i].iter().map(|x| x * s).collect());
        bb.push(b[i] * s);
    }
    Some((aa, bb))
}

/// Weighted least-squares solution of `a x ≈ b` with weights `w` (as the
/// objective `Σ w_i (a_i·x - b_i)²`).
fn weighted_least_squares_f64(a: &[Vec<f64>], b: &[f64], w: &[f64]) -> Option<Vec<f64>> {
    let (aa, bb) = scale_by_weights(a, b, w)?;
    least_squares_f64(&aa, &bb)
}

/// Construct a `name -> value` rule.
fn rule(name: &str, value: Value) -> Value {
    Value::Rule {
        lhs: Box::new(Value::Symbol(name.to_string())),
        rhs: Box::new(value),
        delayed: false,
    }
}

/// Find the RHS of a `name -> …` option in an option list (or a bare rule).
fn find_option(v: &Value, name: &str) -> Option<Value> {
    match v {
        Value::Rule { lhs, rhs, .. } => match lhs.as_ref() {
            Value::Symbol(s) if s == name => Some((**rhs).clone()),
            _ => None,
        },
        Value::List(items) => items.iter().find_map(|it| find_option(it, name)),
        _ => None,
    }
}

/// Parse a `Weights -> {…}` option into one weight per data point.
///
/// Returns `None` when the option is present but malformed, `Some(None)` when
/// absent (or `Automatic`), and `Some(Some(w))` for an explicit list.
#[allow(clippy::option_option)]
fn parse_weights_option(opts: &Value, n: usize, env: &Env) -> Option<Option<Vec<f64>>> {
    let raw = match find_option(opts, "Weights") {
        Some(r) => r,
        None => return Some(None),
    };
    let mut raw = deep_unwrap(&raw);
    if as_list(&raw).is_none() {
        // The option RHS is held unevaluated; resolve a bound symbol if needed.
        if let Ok(v) = crate::eval::eval(&crate::eval::table::value_to_expr(&raw), env) {
            raw = v;
        }
    }
    if matches!(&raw, Value::Symbol(s) if s == "Automatic") {
        return Some(None);
    }
    let items = as_list(&raw)?;
    if items.len() != n {
        return None;
    }
    let vals = items
        .iter()
        .map(super::to_f64)
        .collect::<Option<Vec<f64>>>()?;
    Some(Some(vals))
}

/// Maximum slot index appearing in `expr` (`#` counts as 1).
fn count_slots(expr: &Expr) -> usize {
    match expr {
        Expr::Slot(None) => 1,
        Expr::Slot(Some(n)) => *n,
        Expr::List(items) => items.iter().map(count_slots).max().unwrap_or(0),
        Expr::Call { head, args } => args
            .iter()
            .map(count_slots)
            .chain(std::iter::once(count_slots(head)))
            .max()
            .unwrap_or(0),
        Expr::Rule { lhs, rhs } | Expr::RuleDelayed { lhs, rhs } => {
            count_slots(lhs).max(count_slots(rhs))
        }
        Expr::Pure { body } | Expr::Function { body, .. } => count_slots(body),
        _ => 0,
    }
}

/// Like [`crate::pattern::unwrap_expr_to_value`] but also converts rule and
/// pure-function nodes (which the generic unwrapper leaves as `Pattern`s).
fn deep_unwrap_expr(expr: &Expr) -> Value {
    match expr {
        Expr::Symbol(s) => Value::Symbol(s.clone()),
        Expr::Integer(n) => Value::Integer(n.clone()),
        Expr::Real(r) => Value::Real(r.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Str(s) => Value::Str(s.clone()),
        Expr::Null => Value::Null,
        Expr::List(items) => Value::List(items.iter().map(deep_unwrap_expr).collect()),
        Expr::Call { head, args } => {
            let h = deep_unwrap_expr(head);
            let a: Vec<Value> = args.iter().map(deep_unwrap_expr).collect();
            match h {
                Value::Symbol(name) => Value::Call {
                    head: name,
                    args: a,
                },
                other => Value::Call {
                    head: other.to_string(),
                    args: a,
                },
            }
        }
        Expr::Rule { lhs, rhs } => Value::Rule {
            lhs: Box::new(deep_unwrap_expr(lhs)),
            rhs: Box::new(deep_unwrap_expr(rhs)),
            delayed: false,
        },
        Expr::RuleDelayed { lhs, rhs } => Value::Rule {
            lhs: Box::new(deep_unwrap_expr(lhs)),
            rhs: Box::new(deep_unwrap_expr(rhs)),
            delayed: true,
        },
        Expr::Pure { body } => Value::PureFunction {
            body: body.as_ref().clone(),
            slot_count: count_slots(body),
            params: vec![],
            env: None,
        },
        Expr::Function { params, body } => Value::PureFunction {
            body: body.as_ref().clone(),
            slot_count: count_slots(body),
            params: params.clone(),
            env: None,
        },
        other => Value::Pattern(other.clone()),
    }
}

/// Recover an unevaluated argument, converting nested rule/pure-function nodes.
fn arg_to_uneval_deep(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => deep_unwrap_expr(e),
        Value::Hold(b) | Value::HoldComplete(b) => (**b).clone(),
        other => other.clone(),
    }
}

/// Combine trailing option arguments into a single option list.
fn collect_options(args: &[Value]) -> Value {
    let items: Vec<Value> = args.iter().map(arg_to_uneval_deep).collect();
    match items.len() {
        0 => Value::List(vec![]),
        1 => items.into_iter().next().unwrap_or(Value::List(vec![])),
        _ => Value::List(items),
    }
}

// ── Fit ─────────────────────────────────────────────────────────────────────

/// Fit[data, funcs, vars] — least-squares fit of data with the linear basis `funcs`.
pub fn builtin_fit(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Ok(unevaluated("Fit", args));
    }
    let data = arg_to_value(&args[0], env)?;
    let funcs_v = arg_to_uneval(&args[1]);
    let vars_v = arg_to_uneval(&args[2]);
    let opts = collect_options(&args[3..]);

    let funcs = match as_list(&funcs_v) {
        Some(f) => f.clone(),
        None => return Ok(unevaluated("Fit", args)),
    };
    let vars = match collect_symbols(&vars_v) {
        Some(v) => v,
        None => return Ok(unevaluated("Fit", args)),
    };
    if funcs.is_empty() {
        return Ok(int(0));
    }
    let (coords, ys) = match parse_points(&data, vars.len()) {
        Some(p) => p,
        None => return Ok(unevaluated("Fit", args)),
    };
    if coords.is_empty() {
        return Ok(int(0));
    }

    let weights = match parse_weights_option(&opts, coords.len(), env) {
        Some(w) => w,
        None => return Ok(unevaluated("Fit", args)),
    };

    let mut matrix = vec![vec![0.0f64; funcs.len()]; coords.len()];
    for (i, point) in coords.iter().enumerate() {
        for (j, func) in funcs.iter().enumerate() {
            let mut sub = func.clone();
            for (name, c) in vars.iter().zip(point.iter()) {
                sub = substitute(&sub, name, &super::real(*c));
            }
            matrix[i][j] = eval_scalar(&sub, env).unwrap_or(0.0);
        }
    }

    let coeffs = match &weights {
        Some(w) => weighted_least_squares_f64(&matrix, &ys, w),
        None => least_squares_f64(&matrix, &ys),
    };
    let coeffs = match coeffs {
        Some(c) => c,
        None => return Ok(unevaluated("Fit", args)),
    };

    if coeffs.len() != funcs.len() {
        return Ok(unevaluated("Fit", args));
    }
    let mut terms = Vec::with_capacity(funcs.len());
    for (c, f) in coeffs.iter().zip(funcs.iter()) {
        terms.push(super::arithmetic::mul_values_public(&super::real(*c), f)?);
    }
    super::arithmetic::builtin_plus(&terms)
}

// ── FindFit ─────────────────────────────────────────────────────────────────

/// Evaluate `model` with the given parameter and variable values.
fn eval_model(
    model: &Value,
    params: &[String],
    pvals: &[f64],
    vars: &[String],
    coords: &[f64],
    env: &Env,
) -> Option<f64> {
    let mut m = model.clone();
    for (name, val) in params.iter().zip(pvals.iter()) {
        m = substitute(&m, name, &super::real(*val));
    }
    for (name, c) in vars.iter().zip(coords.iter()) {
        m = substitute(&m, name, &super::real(*c));
    }
    eval_scalar(&m, env)
}

/// Gauss-Newton refinement of `p0`; returns the smallest residual sum of
/// squares seen and the corresponding parameters.
#[allow(clippy::too_many_arguments)]
fn gauss_newton(
    model: &Value,
    params: &[String],
    vars: &[String],
    coords: &[Vec<f64>],
    ys: &[f64],
    p0: &[f64],
    weights: &[f64],
    env: &Env,
) -> (f64, Vec<f64>) {
    let np = params.len();
    let mut p = p0.to_vec();
    let mut best_ss = f64::INFINITY;
    let mut best = p.clone();

    for _ in 0..300 {
        // Weighted residuals `sqrt(w_i) (model(x_i) - y_i)`.
        let mut r = vec![0.0f64; coords.len()];
        let mut valid = true;
        for i in 0..coords.len() {
            let sw = weights[i].max(0.0).sqrt();
            match eval_model(model, params, &p, vars, &coords[i], env) {
                Some(v) => r[i] = sw * (v - ys[i]),
                None => {
                    valid = false;
                    break;
                }
            }
        }
        if !valid {
            break;
        }
        let ss: f64 = r.iter().map(|x| x * x).sum();
        if ss < best_ss {
            best_ss = ss;
            best = p.clone();
        }
        if ss < 1e-24 {
            break;
        }

        // Finite-difference Jacobian (weighted rows).
        let mut jac = vec![vec![0.0f64; np]; coords.len()];
        for k in 0..np {
            let h = 1e-6 * (1.0 + p[k].abs());
            let mut pp = p.clone();
            pp[k] += h;
            let mut pm = p.clone();
            pm[k] -= h;
            for i in 0..coords.len() {
                let sw = weights[i].max(0.0).sqrt();
                let vp = eval_model(model, params, &pp, vars, &coords[i], env).unwrap_or(0.0);
                let vm = eval_model(model, params, &pm, vars, &coords[i], env).unwrap_or(0.0);
                jac[i][k] = sw * (vp - vm) / (2.0 * h);
            }
        }

        let delta = match least_squares_f64(&jac, &r) {
            Some(d) => d,
            None => break,
        };
        for k in 0..np {
            p[k] -= delta[k];
        }
        let step: f64 = delta.iter().map(|d| d * d).sum::<f64>().sqrt();
        if step < 1e-12 {
            break;
        }
    }
    (best_ss, best)
}

/// Best nonlinear least-squares fit produced by `nonlinear_fit_core`.
struct NonlinearFit {
    params: Vec<String>,
    best: Vec<f64>,
    vars: Vec<String>,
    residuals: Vec<f64>,
    ys: Vec<f64>,
}

/// Shared core for `FindFit`/`NonlinearModelFit`: parse the data and model, run
/// a multi-start Gauss–Newton search and return the best parameters together
/// with the signed residuals `y_i - model(x_i)`.
fn nonlinear_fit_core(
    data: &Value,
    model: &Value,
    params_v: &Value,
    vars_v: &Value,
    opts: Option<&Value>,
    env: &Env,
) -> Option<NonlinearFit> {
    let params = match collect_symbols(params_v) {
        Some(p) if !p.is_empty() => p,
        _ => return None,
    };
    let vars = collect_symbols(vars_v)?;
    let (coords, ys) = parse_points(data, vars.len())?;
    if coords.is_empty() {
        return None;
    }

    let weights = match opts {
        Some(o) => parse_weights_option(o, coords.len(), env)?,
        None => None,
    };
    let ones;
    let weights: &[f64] = match &weights {
        Some(w) => w,
        None => {
            ones = vec![1.0f64; coords.len()];
            &ones
        }
    };

    let np = params.len();
    let starts: Vec<Vec<f64>> = vec![vec![1.0; np], vec![0.5; np], vec![2.0; np], vec![0.25; np]];
    let mut best_ss = f64::INFINITY;
    let mut best = vec![1.0; np];
    for start in &starts {
        let (ss, p) = gauss_newton(model, &params, &vars, &coords, &ys, start, weights, env);
        if ss < best_ss {
            best_ss = ss;
            best = p;
        }
    }

    let residuals: Vec<f64> = coords
        .iter()
        .enumerate()
        .map(|(i, c)| ys[i] - eval_model(model, &params, &best, &vars, c, env).unwrap_or(f64::NAN))
        .collect();

    Some(NonlinearFit {
        params,
        best,
        vars,
        residuals,
        ys,
    })
}

/// FindFit[data, model, params, vars] — nonlinear least-squares fit.
pub fn builtin_find_fit(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 4 {
        return Ok(unevaluated("FindFit", args));
    }
    let data = arg_to_value(&args[0], env)?;
    let model = arg_to_uneval(&args[1]);
    let params_v = arg_to_uneval(&args[2]);
    let vars_v = arg_to_uneval(&args[3]);
    let opts = collect_options(&args[4..]);

    let fit = match nonlinear_fit_core(&data, &model, &params_v, &vars_v, Some(&opts), env) {
        Some(f) => f,
        None => return Ok(unevaluated("FindFit", args)),
    };

    let rules: Vec<Value> = fit
        .params
        .iter()
        .zip(fit.best.iter())
        .map(|(name, val)| Value::Rule {
            lhs: Box::new(Value::Symbol(name.clone())),
            rhs: Box::new(super::real(*val)),
            delayed: false,
        })
        .collect();
    Ok(Value::List(rules))
}

/// Collect the free symbols appearing in `v` (used to infer model variables).
fn collect_free_symbols(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Symbol(s) => {
            if !out.contains(s) {
                out.push(s.clone());
            }
        }
        Value::List(items) => {
            for it in items {
                collect_free_symbols(it, out);
            }
        }
        Value::Call { args, .. } => {
            for a in args {
                collect_free_symbols(a, out);
            }
        }
        _ => {}
    }
}

/// Build a `FittedModel[...]` pure-function object from fit components.
///
/// Fixed arguments are `rules`, `residuals`, `model`, `vars` and `meta`; the
/// object's slot receives the property/evaluation request.
fn make_fitted_model(
    model: Value,
    params: &[String],
    vals: &[f64],
    vars: &[String],
    residuals: &[f64],
    meta: Value,
) -> Value {
    let rules: Vec<Value> = params
        .iter()
        .zip(vals.iter())
        .map(|(name, val)| rule(name, super::real(*val)))
        .collect();
    let residuals = Value::List(residuals.iter().map(|r| super::real(*r)).collect());
    let vars = Value::List(vars.iter().map(|s| Value::Symbol(s.clone())).collect());

    Value::PureFunction {
        body: Expr::Call {
            head: Box::new(Expr::Symbol("FittedModel".to_string())),
            args: vec![
                crate::eval::table::value_to_expr(&Value::List(rules)),
                crate::eval::table::value_to_expr(&residuals),
                crate::eval::table::value_to_expr(&model),
                crate::eval::table::value_to_expr(&vars),
                crate::eval::table::value_to_expr(&meta),
                Expr::Slot(Some(1)),
            ],
        },
        slot_count: 1,
        params: vec![],
        env: None,
    }
}

/// NonlinearModelFit[data, model, params, vars] — nonlinear least-squares fit
/// returning a callable fit object.
///
/// The result is applied as `fit[x]` (value at `x`) or `fit[coords]` (value at a
/// coordinate vector), and exposes properties via
/// `fit["BestFitParameters"]`, `fit["FitResiduals"]`, `fit["Fit"]`,
/// `fit[Property["..."]]` and `fit[FitResiduals]`.
pub fn builtin_nonlinear_model_fit(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Ok(unevaluated("NonlinearModelFit", args));
    }
    let data = arg_to_value(&args[0], env)?;
    let model = arg_to_uneval(&args[1]);
    let params_v = arg_to_uneval(&args[2]);
    let vars_v = if args.len() >= 4 {
        arg_to_uneval(&args[3])
    } else {
        // Infer the variables as the free symbols of the model minus parameters.
        let mut free = Vec::new();
        collect_free_symbols(&model, &mut free);
        if let Some(ps) = collect_symbols(&params_v) {
            free.retain(|s| !ps.contains(s));
        }
        Value::List(free.into_iter().map(Value::Symbol).collect())
    };
    let opts = if args.len() >= 5 {
        collect_options(&args[4..])
    } else {
        Value::List(vec![])
    };

    let fit = match nonlinear_fit_core(&data, &model, &params_v, &vars_v, Some(&opts), env) {
        Some(f) => f,
        None => return Ok(unevaluated("NonlinearModelFit", args)),
    };

    let response = Value::List(fit.ys.iter().map(|y| super::real(*y)).collect());
    let meta = Value::List(vec![
        rule("Response", response),
        rule("NumParams", int(fit.params.len() as i64)),
    ]);
    Ok(make_fitted_model(
        model,
        &fit.params,
        &fit.best,
        &fit.vars,
        &fit.residuals,
        meta,
    ))
}

/// LinearModelFit[data, funcs, vars] — least-squares fit with a linear basis,
/// returning a callable fit object that exposes fit statistics.
pub fn builtin_linear_model_fit(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Ok(unevaluated("LinearModelFit", args));
    }
    let data = arg_to_value(&args[0], env)?;
    let funcs_v = arg_to_uneval_deep(&args[1]);
    let vars_v = arg_to_uneval_deep(&args[2]);
    let opts = collect_options(&args[3..]);

    let funcs = match as_list(&funcs_v) {
        Some(f) if !f.is_empty() => f.clone(),
        _ => return Ok(unevaluated("LinearModelFit", args)),
    };
    let vars = match collect_symbols(&vars_v) {
        Some(v) => v,
        None => return Ok(unevaluated("LinearModelFit", args)),
    };
    let (coords, ys) = match parse_points(&data, vars.len()) {
        Some(p) => p,
        None => return Ok(unevaluated("LinearModelFit", args)),
    };
    if coords.is_empty() {
        return Ok(unevaluated("LinearModelFit", args));
    }
    let weights = match parse_weights_option(&opts, coords.len(), env) {
        Some(w) => w,
        None => return Ok(unevaluated("LinearModelFit", args)),
    };

    let mut matrix = vec![vec![0.0f64; funcs.len()]; coords.len()];
    for (i, point) in coords.iter().enumerate() {
        for (j, func) in funcs.iter().enumerate() {
            let mut sub = func.clone();
            for (name, c) in vars.iter().zip(point.iter()) {
                sub = substitute(&sub, name, &super::real(*c));
            }
            matrix[i][j] = eval_scalar(&sub, env).unwrap_or(0.0);
        }
    }

    let coeffs = match &weights {
        Some(w) => weighted_least_squares_f64(&matrix, &ys, w),
        None => least_squares_f64(&matrix, &ys),
    };
    let coeffs = match coeffs {
        Some(c) => c,
        None => return Ok(unevaluated("LinearModelFit", args)),
    };

    let params: Vec<String> = (0..funcs.len()).map(|j| format!("c{}", j + 1)).collect();
    let mut terms = Vec::with_capacity(funcs.len());
    for (name, f) in params.iter().zip(funcs.iter()) {
        terms.push(super::arithmetic::mul_values_public(
            &Value::Symbol(name.clone()),
            f,
        )?);
    }
    let model = super::arithmetic::builtin_plus(&terms)?;

    let mut residuals = Vec::with_capacity(coords.len());
    for (i, point) in coords.iter().enumerate() {
        let mut m = substitute_params(&model, &params, &coeffs);
        for (name, c) in vars.iter().zip(point.iter()) {
            m = substitute(&m, name, &super::real(*c));
        }
        match eval_scalar(&m, env) {
            Some(v) => residuals.push(ys[i] - v),
            None => return Ok(unevaluated("LinearModelFit", args)),
        }
    }

    let design = Value::List(
        matrix
            .iter()
            .map(|row| Value::List(row.iter().map(|x| super::real(*x)).collect()))
            .collect(),
    );
    let mut meta_items = vec![
        rule("DesignMatrix", design),
        rule(
            "Response",
            Value::List(ys.iter().map(|y| super::real(*y)).collect()),
        ),
        rule("NumParams", int(funcs.len() as i64)),
    ];
    if let Some(w) = &weights {
        meta_items.push(rule(
            "Weights",
            Value::List(w.iter().map(|x| super::real(*x)).collect()),
        ));
    }
    Ok(make_fitted_model(
        model,
        &params,
        &coeffs,
        &vars,
        &residuals,
        Value::List(meta_items),
    ))
}

/// Extract the property name from a `FittedModel` request argument.
fn property_name(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.clone()),
        Value::Symbol(s) => Some(s.clone()),
        Value::Builtin(name, _) => Some(name.clone()),
        Value::Call { head, args } if head == "Property" && args.len() == 1 => {
            property_name(&args[0])
        }
        _ => None,
    }
}

/// Extract `(name, value)` pairs from a list of parameter rules.
fn extract_param_rules(v: &Value) -> (Vec<String>, Vec<f64>) {
    let mut names = Vec::new();
    let mut vals = Vec::new();
    if let Value::List(items) = v {
        for it in items {
            if let Value::Rule { lhs, rhs, .. } = it
                && let Value::Symbol(s) = lhs.as_ref()
                && let Some(x) = super::to_f64(rhs)
            {
                names.push(s.clone());
                vals.push(x);
            }
        }
    }
    (names, vals)
}

/// Substitute parameter names with their fitted values.
fn substitute_params(model: &Value, names: &[String], vals: &[f64]) -> Value {
    let mut m = model.clone();
    for (n, v) in names.iter().zip(vals.iter()) {
        m = substitute(&m, n, &super::real(*v));
    }
    m
}

/// Collect variable names from a symbol or a list of symbols.
fn var_names(v: &Value) -> Vec<String> {
    match v {
        Value::List(items) => items
            .iter()
            .filter_map(|it| match it {
                Value::Symbol(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        Value::Symbol(s) => vec![s.clone()],
        _ => vec![],
    }
}

/// Substitute a symbol in an expression with an arbitrary replacement expression.
fn expr_substitute(expr: &Expr, name: &str, repl: &Expr) -> Expr {
    match expr {
        Expr::Symbol(s) if s == name => repl.clone(),
        Expr::List(items) => Expr::List(
            items
                .iter()
                .map(|e| expr_substitute(e, name, repl))
                .collect(),
        ),
        Expr::Call { head, args } => Expr::Call {
            head: Box::new(expr_substitute(head, name, repl)),
            args: args
                .iter()
                .map(|e| expr_substitute(e, name, repl))
                .collect(),
        },
        other => other.clone(),
    }
}

/// Parse a list of real numbers.
fn parse_real_list(v: &Value) -> Option<Vec<f64>> {
    as_list(v)?.iter().map(super::to_f64).collect()
}

/// Extract `(ssres, sstot, n, p)` from the residuals and fit metadata.
fn fit_stats(residuals: &Value, meta: &Value) -> Option<(f64, f64, usize, usize)> {
    let r = parse_real_list(residuals)?;
    let ys = parse_real_list(&find_option(meta, "Response")?)?;
    let n = r.len();
    if n == 0 || ys.len() != n {
        return None;
    }
    let p = super::to_f64(&find_option(meta, "NumParams")?)? as usize;
    let ssres: f64 = r.iter().map(|x| x * x).sum();
    let ybar = ys.iter().sum::<f64>() / (n as f64);
    let sstot: f64 = ys.iter().map(|y| (y - ybar) * (y - ybar)).sum();
    Some((ssres, sstot, n, p))
}

/// `Indeterminate` as a value.
fn indeterminate() -> Value {
    Value::Symbol("Indeterminate".to_string())
}

/// EstimatedVariance = SSres / (n - p).
fn estimated_variance(residuals: &Value, meta: &Value) -> Option<Value> {
    let (ssres, _, n, p) = fit_stats(residuals, meta)?;
    Some(if n > p {
        super::real(ssres / ((n - p) as f64))
    } else if ssres.abs() < 1e-12 {
        super::real(0.0)
    } else {
        indeterminate()
    })
}

/// RSquared = 1 - SSres / SStot.
fn r_squared(residuals: &Value, meta: &Value) -> Option<Value> {
    let (ssres, sstot, _, _) = fit_stats(residuals, meta)?;
    Some(if sstot.abs() < 1e-15 {
        indeterminate()
    } else {
        super::real(1.0 - ssres / sstot)
    })
}

/// AdjustedRSquared, accounting for the number of parameters.
fn adjusted_r_squared(residuals: &Value, meta: &Value) -> Option<Value> {
    let (ssres, sstot, n, p) = fit_stats(residuals, meta)?;
    if n <= p || sstot.abs() < 1e-15 {
        return Some(indeterminate());
    }
    let r2 = 1.0 - ssres / sstot;
    Some(super::real(
        1.0 - (1.0 - r2) * ((n - 1) as f64) / ((n - p) as f64),
    ))
}

/// FittedModel[rules, residuals, model, vars, meta, request] — dispatch a fit object.
fn builtin_fitted_model(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 6 {
        return Ok(unevaluated("FittedModel", args));
    }
    // The evaluator wraps rule components and symbolic args in `Pattern`; strip
    // those so numeric parsing and substitution see plain values.
    let rules_v = deep_unwrap(&args[0]);
    let residuals = deep_unwrap(&args[1]);
    let model = deep_unwrap(&args[2]);
    let vars_v = deep_unwrap(&args[3]);
    let meta = deep_unwrap(&args[4]);
    let request = deep_unwrap(&args[5]);

    let (param_names, param_vals) = extract_param_rules(&rules_v);
    let vars = var_names(&vars_v);
    let model = &model;

    if let Some(prop) = property_name(&request) {
        let computed: Option<Value> = match prop.as_str() {
            "BestFitParameters" | "ParameterRules" => Some(rules_v.clone()),
            "FitResiduals" | "Residuals" => Some(residuals.clone()),
            "Fit" | "BestFit" => Some(substitute_params(model, &param_names, &param_vals)),
            "FitFunction" => {
                let m = substitute_params(model, &param_names, &param_vals);
                let mut body = crate::eval::table::value_to_expr(&m);
                for (i, name) in vars.iter().enumerate() {
                    body = expr_substitute(&body, name, &Expr::Slot(Some(i + 1)));
                }
                Some(Value::PureFunction {
                    body,
                    slot_count: vars.len(),
                    params: vec![],
                    env: None,
                })
            }
            "DesignMatrix" => find_option(&meta, "DesignMatrix"),
            "Response" => find_option(&meta, "Response"),
            "Weights" => find_option(&meta, "Weights"),
            "NumberOfParameters" => find_option(&meta, "NumParams"),
            "EstimatedVariance" => estimated_variance(&residuals, &meta),
            "RSquared" => r_squared(&residuals, &meta),
            "AdjustedRSquared" => adjusted_r_squared(&residuals, &meta),
            _ => None,
        };
        return Ok(match computed {
            Some(v) => v,
            None => unevaluated("FittedModel", args),
        });
    }

    // Numeric evaluation at a scalar or an explicit coordinate vector.
    let coords: Option<Vec<f64>> = if vars.len() == 1 {
        eval_scalar(&request, env).map(|x| vec![x])
    } else if let Value::List(items) = &request {
        if items.len() == vars.len() {
            items.iter().map(|it| eval_scalar(it, env)).collect()
        } else {
            None
        }
    } else {
        None
    };

    if let Some(coords) = coords {
        let mut m = substitute_params(model, &param_names, &param_vals);
        for (name, c) in vars.iter().zip(coords.iter()) {
            m = substitute(&m, name, &super::real(*c));
        }
        return Ok(match eval_scalar(&m, env) {
            Some(v) => super::real(v),
            None => unevaluated("FittedModel", args),
        });
    }

    Ok(unevaluated("FittedModel", args))
}

/// FitResiduals[fit] — residuals of a fit object.
pub fn builtin_fit_residuals(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("FitResiduals", args));
    }
    if let Ok(v) =
        crate::eval::apply_function(&args[0], &[Value::Str("FitResiduals".to_string())], env)
        && !matches!(&v, Value::Call { head, .. } if head == "FittedModel")
    {
        return Ok(v);
    }
    Ok(unevaluated("FitResiduals", args))
}

/// DesignMatrix[fit] — design matrix of a (linear) model fit object.
pub fn builtin_design_matrix(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("DesignMatrix", args));
    }
    if let Ok(v) =
        crate::eval::apply_function(&args[0], &[Value::Str("DesignMatrix".to_string())], env)
        && !matches!(&v, Value::Call { head, .. } if head == "FittedModel")
    {
        return Ok(v);
    }
    Ok(unevaluated("DesignMatrix", args))
}

/// Property[obj, name] — access a property of an object; `Property[name]` stays
/// notationally symbolic so that `obj[Property[name]]` works.
pub fn builtin_property(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() == 2
        && let Ok(v) = crate::eval::apply_function(&args[0], std::slice::from_ref(&args[1]), env)
        && !matches!(&v, Value::Call { head, .. } if head == "FittedModel")
    {
        return Ok(v);
    }
    Ok(unevaluated("Property", args))
}

// ── LeastSquares ────────────────────────────────────────────────────────────

fn is_exact_scalar(v: &Value) -> bool {
    matches!(v, Value::Integer(_) | Value::Rational(_))
}

fn matrix_is_exact(m: &[Value]) -> bool {
    m.iter().all(|row| match row {
        Value::List(items) => items.iter().all(is_exact_scalar),
        _ => false,
    })
}

fn value_to_f64_matrix(m: &[Value]) -> Option<Vec<Vec<f64>>> {
    m.iter()
        .map(|row| match row {
            Value::List(items) => items.iter().map(super::to_f64).collect(),
            _ => None,
        })
        .collect()
}

/// LeastSquares[a, b] — least-squares solution of `a x ≈ b`.
pub fn builtin_least_squares(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("LeastSquares", args));
    }
    let a_items = match as_list(&args[0]) {
        Some(v) => v,
        None => return Ok(unevaluated("LeastSquares", args)),
    };
    let b_items = match as_list(&args[1]) {
        Some(v) => v,
        None => return Ok(unevaluated("LeastSquares", args)),
    };

    // Exact hand-off when both operands are exact and non-degenerate.
    if matrix_is_exact(a_items)
        && b_items.iter().all(is_exact_scalar)
        && let Ok(pinv) = super::linalg::builtin_pseudo_inverse(std::slice::from_ref(&args[0]))
        && let Ok(sol) = super::linalg::builtin_dot(&[pinv, args[1].clone()])
    {
        return Ok(sol);
    }

    // Numerical fallback.
    let a = match value_to_f64_matrix(a_items) {
        Some(a) => a,
        None => return Ok(unevaluated("LeastSquares", args)),
    };
    let b = match b_items
        .iter()
        .map(super::to_f64)
        .collect::<Option<Vec<_>>>()
    {
        Some(b) => b,
        None => return Ok(unevaluated("LeastSquares", args)),
    };
    if a.len() != b.len() {
        return Ok(unevaluated("LeastSquares", args));
    }
    match least_squares_f64(&a, &b) {
        Some(x) => Ok(Value::List(x.into_iter().map(super::real).collect())),
        None => Ok(unevaluated("LeastSquares", args)),
    }
}

// ── LinearOptimization / LinearProgramming ──────────────────────────────────

/// Simplex tableau pivot.
fn pivot(
    t: &mut [Vec<f64>],
    basis: &mut [usize],
    m: usize,
    ncols: usize,
    leave: usize,
    enter: usize,
) {
    let p = t[leave][enter];
    for j in 0..=ncols {
        t[leave][j] /= p;
    }
    for i in 0..=m {
        if i == leave {
            continue;
        }
        let f = t[i][enter];
        if f != 0.0 {
            for j in 0..=ncols {
                t[i][j] -= f * t[leave][j];
            }
        }
    }
    basis[leave] = enter;
}

/// Rebuild the objective (reduced-cost) row for the given basis.
fn build_objective(t: &mut [Vec<f64>], basis: &[usize], cost: &[f64], m: usize, ncols: usize) {
    for j in 0..=ncols {
        t[m][j] = 0.0;
    }
    for j in 0..=ncols {
        let mut val = if j < ncols { cost[j] } else { 0.0 };
        for i in 0..m {
            val -= cost[basis[i]] * t[i][j];
        }
        t[m][j] = val;
    }
}

/// Run simplex pivots until optimal. Returns `false` if unbounded.
fn run_simplex(
    t: &mut [Vec<f64>],
    basis: &mut [usize],
    m: usize,
    ncols: usize,
    phase2: bool,
) -> bool {
    let eps = 1e-9;
    loop {
        // Bland's rule: first negative reduced cost.
        let mut enter = None;
        for j in 0..ncols {
            if phase2 && j >= ncols - m {
                continue; // skip artificials in phase 2
            }
            if t[m][j] < -eps {
                enter = Some(j);
                break;
            }
        }
        let enter = match enter {
            Some(e) => e,
            None => return true,
        };

        let mut leave = None;
        let mut best_ratio = f64::INFINITY;
        for i in 0..m {
            if t[i][enter] > eps {
                let ratio = t[i][ncols] / t[i][enter];
                if ratio < best_ratio - 1e-12 {
                    best_ratio = ratio;
                    leave = Some(i);
                }
            }
        }
        let leave = match leave {
            Some(l) => l,
            None => return false,
        };
        pivot(t, basis, m, ncols, leave, enter);
    }
}

/// Minimize `c·x` subject to `a x >= b`, `x >= 0` via the two-phase simplex.
fn simplex_minimize(c: &[f64], a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let m = a.len();
    let n = c.len();
    if m == 0 {
        return Some(vec![0.0; n]);
    }
    if b.len() != m || a.iter().any(|r| r.len() != n) {
        return None;
    }

    // Normalize to b >= 0.
    let mut aa = a.to_vec();
    let mut bb = b.to_vec();
    for i in 0..m {
        if bb[i] < 0.0 {
            for j in 0..n {
                aa[i][j] = -aa[i][j];
            }
            bb[i] = -bb[i];
        }
    }

    let ncols = n + m + m; // x, surplus, artificial
    let mut t = vec![vec![0.0f64; ncols + 1]; m + 1];
    for i in 0..m {
        for j in 0..n {
            t[i][j] = aa[i][j];
        }
        t[i][n + i] = -1.0;
        t[i][n + m + i] = 1.0;
        t[i][ncols] = bb[i];
    }
    let mut basis: Vec<usize> = (0..m).map(|i| n + m + i).collect();

    let mut phase1_cost = vec![0.0f64; ncols];
    for i in 0..m {
        phase1_cost[n + m + i] = 1.0;
    }
    build_objective(&mut t, &basis, &phase1_cost, m, ncols);
    if !run_simplex(&mut t, &mut basis, m, ncols, false) {
        return None;
    }

    // Drive artificial variables out of the basis where possible.
    for i in 0..m {
        if basis[i] >= n + m {
            let mut found = None;
            for j in 0..n + m {
                if t[i][j].abs() > 1e-9 {
                    found = Some(j);
                    break;
                }
            }
            if let Some(j) = found {
                pivot(&mut t, &mut basis, m, ncols, i, j);
            }
        }
    }

    let mut phase2_cost = vec![0.0f64; ncols];
    phase2_cost[..n].copy_from_slice(c);
    build_objective(&mut t, &basis, &phase2_cost, m, ncols);
    if !run_simplex(&mut t, &mut basis, m, ncols, true) {
        return None;
    }

    let mut x = vec![0.0f64; n];
    for i in 0..m {
        if basis[i] < n {
            x[basis[i]] = t[i][ncols];
        }
    }
    Some(x)
}

/// LinearOptimization[c, a, b] — minimize `c·x` with `a x >= b`, `x >= 0`.
pub fn builtin_linear_optimization(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Ok(unevaluated("LinearOptimization", args));
    }
    let c = match as_list(&args[0])
        .and_then(|v| v.iter().map(super::to_f64).collect::<Option<Vec<f64>>>())
    {
        Some(c) => c,
        None => return Ok(unevaluated("LinearOptimization", args)),
    };
    let a = match as_list(&args[1]).and_then(|v| value_to_f64_matrix(v)) {
        Some(a) => a,
        None => return Ok(unevaluated("LinearOptimization", args)),
    };
    let b = match as_list(&args[2])
        .and_then(|v| v.iter().map(super::to_f64).collect::<Option<Vec<f64>>>())
    {
        Some(b) => b,
        None => return Ok(unevaluated("LinearOptimization", args)),
    };
    match simplex_minimize(&c, &a, &b) {
        Some(x) => Ok(Value::List(x.into_iter().map(super::real).collect())),
        None => Ok(unevaluated("LinearOptimization", args)),
    }
}

// ── Interpolation ───────────────────────────────────────────────────────────

/// Which dimension the interpolation data lives in.
enum InterpData {
    OneD(Vec<f64>, Vec<f64>),
    TwoD(TwoD),
}

/// 2D interpolation data (either a tensor grid or scattered points).
enum TwoD {
    Grid {
        xs: Vec<f64>,
        ys: Vec<f64>,
        /// `z[i][j] = f(xs[i], ys[j])`.
        z: Vec<Vec<f64>>,
    },
    Scatter(Vec<(f64, f64, f64)>),
}

/// Group scattered triples into a grid when they form a full tensor product.
fn build_two_d(mut pts: Vec<(f64, f64, f64)>) -> Option<TwoD> {
    if pts.is_empty() {
        return None;
    }
    pts.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for (x, y, _) in &pts {
        if !xs.iter().any(|v| (*v - *x).abs() < 1e-12) {
            xs.push(*x);
        }
        if !ys.iter().any(|v| (*v - *y).abs() < 1e-12) {
            ys.push(*y);
        }
    }

    if xs.len().saturating_mul(ys.len()) == pts.len() {
        let mut z = vec![vec![0.0f64; ys.len()]; xs.len()];
        let mut filled = vec![vec![false; ys.len()]; xs.len()];
        let mut ok = true;
        for (x, y, v) in &pts {
            let i = xs.iter().position(|t| (t - x).abs() < 1e-9);
            let j = ys.iter().position(|t| (t - y).abs() < 1e-9);
            match (i, j) {
                (Some(i), Some(j)) if !filled[i][j] => {
                    z[i][j] = *v;
                    filled[i][j] = true;
                }
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            return Some(TwoD::Grid { xs, ys, z });
        }
    }
    Some(TwoD::Scatter(pts))
}

/// Parse 1D or 2D interpolation data into sorted sample points.
fn parse_interp_data(data: &Value) -> Option<InterpData> {
    let data = deep_unwrap(data);
    let items = as_list(&data)?;
    if items.is_empty() {
        return None;
    }
    let dim = match &items[0] {
        Value::Rule { lhs, .. } => match lhs.as_ref() {
            Value::List(v) if v.len() >= 2 => 2,
            _ => 1,
        },
        Value::List(inner) if inner.len() >= 3 => 2,
        _ => 1,
    };

    if dim == 1 {
        let mut pts = Vec::with_capacity(items.len());
        for it in items {
            let (x, y) = match it {
                Value::Rule { lhs, rhs, .. } => (first_coord(lhs)?, super::to_f64(rhs)?),
                Value::List(inner) if inner.len() == 2 => {
                    (super::to_f64(&inner[0])?, super::to_f64(&inner[1])?)
                }
                _ => return None,
            };
            pts.push((x, y));
        }
        pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut xs: Vec<f64> = Vec::new();
        let mut ys: Vec<f64> = Vec::new();
        for (x, y) in pts {
            if xs
                .last()
                .is_some_and(|last: &f64| (*last - x).abs() < 1e-12)
            {
                continue;
            }
            xs.push(x);
            ys.push(y);
        }
        if xs.is_empty() {
            return None;
        }
        Some(InterpData::OneD(xs, ys))
    } else {
        let mut pts = Vec::with_capacity(items.len());
        for it in items {
            let (x, y, z) = match it {
                Value::Rule { lhs, rhs, .. } => match lhs.as_ref() {
                    Value::List(v) if v.len() >= 2 => (
                        super::to_f64(&v[0])?,
                        super::to_f64(&v[1])?,
                        super::to_f64(rhs)?,
                    ),
                    _ => return None,
                },
                Value::List(inner) if inner.len() >= 3 => (
                    super::to_f64(&inner[0])?,
                    super::to_f64(&inner[1])?,
                    super::to_f64(inner.last()?)?,
                ),
                _ => return None,
            };
            pts.push((x, y, z));
        }
        Some(InterpData::TwoD(build_two_d(pts)?))
    }
}

/// How to handle evaluation outside the interpolation domain.
enum Extrap {
    Automatic,
    Indeterminate,
    Handler(Expr),
}

/// Options controlling an interpolant.
struct InterpOptions {
    order: i64,
    periodic: bool,
    extrap: Extrap,
}

impl Default for InterpOptions {
    fn default() -> Self {
        InterpOptions {
            order: 3,
            periodic: false,
            extrap: Extrap::Automatic,
        }
    }
}

/// Interpret an `ExtrapolationHandler` RHS.
fn parse_interp_handler(rhs: &Value) -> Extrap {
    match rhs {
        Value::Symbol(s) if s == "Automatic" => Extrap::Automatic,
        Value::Symbol(s) if s == "Indeterminate" => Extrap::Indeterminate,
        Value::PureFunction { body, .. } => Extrap::Handler(body.clone()),
        Value::Symbol(s) => Extrap::Handler(Expr::Call {
            head: Box::new(Expr::Symbol(s.clone())),
            args: vec![Expr::Slot(Some(1))],
        }),
        Value::Builtin(s, _) => Extrap::Handler(Expr::Call {
            head: Box::new(Expr::Symbol(s.clone())),
            args: vec![Expr::Slot(Some(1))],
        }),
        _ => Extrap::Automatic,
    }
}

fn truthy(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Symbol(s) if s == "True" => Some(true),
        Value::Symbol(s) if s == "False" => Some(false),
        _ => None,
    }
}

/// Parse `InterpolationOrder` / `PeriodicInterpolation` / `ExtrapolationHandler`.
fn parse_interp_options(arg: &Value) -> InterpOptions {
    fn walk(v: &Value, o: &mut InterpOptions) {
        match v {
            Value::Rule { lhs, rhs, .. } => {
                if let Value::Symbol(name) = lhs.as_ref() {
                    match name.as_str() {
                        "InterpolationOrder" => {
                            if let Value::Integer(n) = rhs.as_ref() {
                                o.order = n.to_i64().unwrap_or(3);
                            }
                        }
                        "PeriodicInterpolation" => {
                            if let Some(b) = truthy(rhs) {
                                o.periodic = b;
                            }
                        }
                        "ExtrapolationHandler" => o.extrap = parse_interp_handler(rhs),
                        _ => {}
                    }
                }
            }
            Value::List(items) => {
                for it in items {
                    walk(it, o);
                }
            }
            _ => {}
        }
    }
    let mut o = InterpOptions::default();
    walk(arg, &mut o);
    o
}

/// The `{periodic, extrapolation, order}` metadata carried by an interpolant.
fn interp_meta_expr(o: &InterpOptions) -> Expr {
    let extrap = match &o.extrap {
        Extrap::Automatic => Expr::Symbol("Automatic".to_string()),
        Extrap::Indeterminate => Expr::Symbol("Indeterminate".to_string()),
        Extrap::Handler(e) => e.clone(),
    };
    Expr::List(vec![
        Expr::Bool(o.periodic),
        extrap,
        Expr::Integer(Integer::from(o.order)),
    ])
}

fn meta_list_get(meta: &Value, i: usize) -> Option<&Value> {
    match meta {
        Value::List(items) => items.get(i),
        _ => None,
    }
}

fn real_list(v: &[f64]) -> Value {
    Value::List(v.iter().map(|x| super::real(*x)).collect())
}

/// Build `{{x, y, dy}, …}` rows (Hermite form) for the interpolant.
fn build_hermite_rows(xs: &[f64], ys: &[f64], order: i64, periodic: bool) -> Value {
    let n = xs.len();
    let mut rows: Vec<Value> = Vec::new();
    if n == 1 {
        rows.push(Value::List(vec![
            super::real(xs[0]),
            super::real(ys[0]),
            super::real(0.0),
        ]));
        rows.push(rows[0].clone());
        return Value::List(rows);
    }
    if order <= 1 {
        // Linear: emit both endpoints of every segment with the segment slope.
        for i in 0..n - 1 {
            let s = (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i]);
            rows.push(Value::List(vec![
                super::real(xs[i]),
                super::real(ys[i]),
                super::real(s),
            ]));
            rows.push(Value::List(vec![
                super::real(xs[i + 1]),
                super::real(ys[i + 1]),
                super::real(s),
            ]));
        }
    } else {
        let period = xs[n - 1] - xs[0];
        for i in 0..n {
            let s = if periodic && period > 0.0 {
                // Wrap the neighbours so the slope matches across the seam.
                let ip = if i == 0 { n - 1 } else { i - 1 };
                let inx = if i == n - 1 { 0 } else { i + 1 };
                let xp = if ip == n - 1 { xs[ip] - period } else { xs[ip] };
                let xn = if inx == 0 { xs[inx] + period } else { xs[inx] };
                (ys[inx] - ys[ip]) / (xn - xp)
            } else if i == 0 {
                (ys[1] - ys[0]) / (xs[1] - xs[0])
            } else if i == n - 1 {
                (ys[n - 1] - ys[n - 2]) / (xs[n - 1] - xs[n - 2])
            } else {
                (ys[i + 1] - ys[i - 1]) / (xs[i + 1] - xs[i - 1])
            };
            rows.push(Value::List(vec![
                super::real(xs[i]),
                super::real(ys[i]),
                super::real(s),
            ]));
        }
    }
    Value::List(rows)
}

/// Interpolation[data] (and options such as `InterpolationOrder -> n`,
/// `PeriodicInterpolation -> True`, `ExtrapolationHandler -> …`).
///
/// The result is a callable value; `Interpolation[data][x]` evaluates `x`.
pub fn builtin_interpolation(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(unevaluated("Interpolation", args));
    }
    let data = arg_to_value(&args[0], env)?;
    let opts = parse_interp_options(&collect_options(&args[1..]));
    let parsed = match parse_interp_data(&data) {
        Some(p) => p,
        None => return Ok(unevaluated("Interpolation", args)),
    };
    let meta = interp_meta_expr(&opts);

    let body = match parsed {
        InterpData::OneD(xs, ys) => Expr::Call {
            head: Box::new(Expr::Symbol("FitInterpolationFunction".to_string())),
            args: vec![
                crate::eval::table::value_to_expr(&build_hermite_rows(
                    &xs,
                    &ys,
                    opts.order,
                    opts.periodic,
                )),
                meta,
                Expr::Slot(Some(1)),
            ],
        },
        InterpData::TwoD(td) => Expr::Call {
            head: Box::new(Expr::Symbol("FitInterpFunc2D".to_string())),
            args: vec![
                crate::eval::table::value_to_expr(&two_d_value(&td)),
                meta,
                Expr::Slot(Some(1)),
            ],
        },
    };

    Ok(Value::PureFunction {
        body,
        slot_count: 1,
        params: vec![],
        env: None,
    })
}

/// Serialize 2D interpolation data for embedding in an expression.
fn two_d_value(td: &TwoD) -> Value {
    match td {
        TwoD::Grid { xs, ys, z } => Value::List(vec![
            Value::Str("Grid".to_string()),
            real_list(xs),
            real_list(ys),
            Value::List(z.iter().map(|row| real_list(row)).collect()),
        ]),
        TwoD::Scatter(pts) => Value::List(vec![
            Value::Str("Scatter".to_string()),
            Value::List(
                pts.iter()
                    .map(|(x, y, z)| {
                        Value::List(vec![super::real(*x), super::real(*y), super::real(*z)])
                    })
                    .collect(),
            ),
        ]),
    }
}

/// Domain bounds of a 1D interpolant as `(xlo, xhi)`.
fn rows_domain(data: &Value) -> Option<(f64, f64)> {
    let rows = as_list(data)?;
    let xa = match rows.first()? {
        Value::List(c) if c.len() == 3 => super::to_f64(&c[0])?,
        _ => return None,
    };
    let xb = match rows.last()? {
        Value::List(c) if c.len() == 3 => super::to_f64(&c[0])?,
        _ => return None,
    };
    Some((xa, xb))
}

/// FitInterpolationFunction[rows, meta, t] — evaluate a Hermite interpolant,
/// resolving the point numerically (handles symbolic constants such as `Pi/2`).
pub fn builtin_fit_interp_eval(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Ok(unevaluated("FitInterpolationFunction", args));
    }
    let mut t = match eval_scalar(&args[2], env) {
        Some(t) => t,
        None => return Ok(unevaluated("FitInterpolationFunction", args)),
    };
    let meta = deep_unwrap(&args[1]);
    let periodic = matches!(meta_list_get(&meta, 0), Some(Value::Bool(true)));
    let (lo, hi) = match rows_domain(&args[0]) {
        Some(d) => d,
        None => return Ok(unevaluated("FitInterpolationFunction", args)),
    };
    if periodic && hi > lo {
        t = lo + (t - lo).rem_euclid(hi - lo);
    } else if t < lo || t > hi {
        match meta_list_get(&meta, 1) {
            Some(Value::Symbol(s)) if s == "Automatic" => {}
            Some(Value::Symbol(s)) if s == "Indeterminate" => return Ok(indeterminate()),
            Some(v) => return Ok(v.clone()),
            None => {}
        }
    }
    match hermite_eval(&args[0], t) {
        Some(v) => Ok(super::real(v)),
        None => Ok(unevaluated("FitInterpolationFunction", args)),
    }
}

/// Domain bounds `(xlo, xhi, ylo, yhi)` of 2D interpolation data.
fn two_d_domain(data: &Value) -> Option<(f64, f64, f64, f64)> {
    let items = as_list(data)?;
    match items.first()? {
        Value::Str(s) if s == "Grid" => {
            let xs = parse_real_list(items.get(1)?)?;
            let ys = parse_real_list(items.get(2)?)?;
            Some((*xs.first()?, *xs.last()?, *ys.first()?, *ys.last()?))
        }
        Value::Str(s) if s == "Scatter" => {
            let pts = as_list(items.get(1)?)?;
            let (mut xlo, mut xhi, mut ylo, mut yhi) = (
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            );
            let mut any = false;
            for p in pts {
                if let Value::List(c) = p {
                    let x = super::to_f64(&c[0])?;
                    let y = super::to_f64(&c[1])?;
                    xlo = xlo.min(x);
                    xhi = xhi.max(x);
                    ylo = ylo.min(y);
                    yhi = yhi.max(y);
                    any = true;
                }
            }
            if any {
                Some((xlo, xhi, ylo, yhi))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Barycentric interpolation over the three nearest scattered points.
fn scatter_eval(pts: &[(f64, f64, f64)], x: f64, y: f64) -> Option<f64> {
    if pts.is_empty() {
        return None;
    }
    let dist2 = |p: &(f64, f64, f64)| (p.0 - x) * (p.0 - x) + (p.1 - y) * (p.1 - y);
    let mut idx: Vec<usize> = (0..pts.len()).collect();
    idx.sort_by(|&a, &b| {
        dist2(&pts[a])
            .partial_cmp(&dist2(&pts[b]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if pts.len() >= 3 {
        let (x1, y1, z1) = pts[idx[0]];
        let (x2, y2, z2) = pts[idx[1]];
        let (x3, y3, z3) = pts[idx[2]];
        let det = (y2 - y3) * (x1 - x3) + (x3 - x2) * (y1 - y3);
        if det.abs() > 1e-12 {
            let l1 = ((y2 - y3) * (x - x3) + (x3 - x2) * (y - y3)) / det;
            let l2 = ((y3 - y1) * (x - x3) + (x1 - x3) * (y - y3)) / det;
            let l3 = 1.0 - l1 - l2;
            return Some(l1 * z1 + l2 * z2 + l3 * z3);
        }
    }
    // Inverse-distance fallback (exact at nodes).
    let mut num = 0.0;
    let mut den = 0.0;
    for p in pts {
        let d2 = dist2(p);
        if d2 < 1e-24 {
            return Some(p.2);
        }
        let w = 1.0 / d2;
        num += w * p.2;
        den += w;
    }
    if den > 0.0 { Some(num / den) } else { None }
}

/// Evaluate 2D interpolation data (grid via successive 1D interpolation).
fn two_d_eval(data: &Value, x: f64, y: f64, order: i64, periodic: bool) -> Option<f64> {
    let items = as_list(data)?;
    match items.first()? {
        Value::Str(s) if s == "Grid" => {
            let xs = parse_real_list(items.get(1)?)?;
            let ys = parse_real_list(items.get(2)?)?;
            let zrows = as_list(items.get(3)?)?;
            let mut z: Vec<Vec<f64>> = Vec::with_capacity(zrows.len());
            for row in zrows {
                z.push(parse_real_list(row)?);
            }
            if z.len() != xs.len() {
                return None;
            }
            if xs.len() >= 2 && ys.len() >= 2 {
                let mut g = Vec::with_capacity(ys.len());
                for j in 0..ys.len() {
                    let col: Vec<f64> = (0..xs.len()).map(|i| z[i][j]).collect();
                    let rows = build_hermite_rows(&xs, &col, order, periodic);
                    g.push(hermite_eval(&rows, x)?);
                }
                let rows = build_hermite_rows(&ys, &g, order, periodic);
                hermite_eval(&rows, y)
            } else if ys.len() == 1 {
                let col: Vec<f64> = (0..xs.len()).map(|i| z[i][0]).collect();
                let rows = build_hermite_rows(&xs, &col, order, periodic);
                hermite_eval(&rows, x)
            } else if xs.len() == 1 {
                let rows = build_hermite_rows(&ys, &z[0], order, periodic);
                hermite_eval(&rows, y)
            } else {
                None
            }
        }
        Value::Str(s) if s == "Scatter" => {
            let ptsv = as_list(items.get(1)?)?;
            let mut pts = Vec::with_capacity(ptsv.len());
            for p in ptsv {
                if let Value::List(c) = p {
                    pts.push((
                        super::to_f64(&c[0])?,
                        super::to_f64(&c[1])?,
                        super::to_f64(&c[2])?,
                    ));
                }
            }
            scatter_eval(&pts, x, y)
        }
        _ => None,
    }
}

/// FitInterpFunc2D[data, meta, {x, y}] — evaluate a 2D interpolant.
pub fn builtin_fit_interp_2d_eval(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Ok(unevaluated("FitInterpFunc2D", args));
    }
    let request = deep_unwrap(&args[2]);
    let (mut x, mut y) = match &request {
        Value::List(v) if v.len() == 2 => {
            match (eval_scalar(&v[0], env), eval_scalar(&v[1], env)) {
                (Some(a), Some(b)) => (a, b),
                _ => return Ok(unevaluated("FitInterpFunc2D", args)),
            }
        }
        _ => return Ok(unevaluated("FitInterpFunc2D", args)),
    };
    let data = deep_unwrap(&args[0]);
    let meta = deep_unwrap(&args[1]);
    let periodic = matches!(meta_list_get(&meta, 0), Some(Value::Bool(true)));
    let order = meta_list_get(&meta, 2)
        .and_then(super::to_f64)
        .map(|v| v as i64)
        .unwrap_or(3);
    let (xlo, xhi, ylo, yhi) = match two_d_domain(&data) {
        Some(d) => d,
        None => return Ok(unevaluated("FitInterpFunc2D", args)),
    };
    if periodic && xhi > xlo && yhi > ylo {
        x = xlo + (x - xlo).rem_euclid(xhi - xlo);
        y = ylo + (y - ylo).rem_euclid(yhi - ylo);
    } else if x < xlo || x > xhi || y < ylo || y > yhi {
        match meta_list_get(&meta, 1) {
            Some(Value::Symbol(s)) if s == "Automatic" => {}
            Some(Value::Symbol(s)) if s == "Indeterminate" => return Ok(indeterminate()),
            Some(v) => return Ok(v.clone()),
            None => {}
        }
    }
    match two_d_eval(&data, x, y, order, periodic) {
        Some(v) => Ok(super::real(v)),
        None => Ok(unevaluated("FitInterpFunc2D", args)),
    }
}

/// Piecewise cubic Hermite interpolation over `{{x, y, y'}, …}`.
fn hermite_eval(data: &Value, t: f64) -> Option<f64> {
    let rows = match data {
        Value::List(rows) if rows.len() >= 2 => rows,
        _ => return None,
    };
    let mut pts = Vec::with_capacity(rows.len());
    for row in rows {
        match row {
            Value::List(c) if c.len() == 3 => {
                pts.push((
                    super::to_f64(&c[0])?,
                    super::to_f64(&c[1])?,
                    super::to_f64(&c[2])?,
                ));
            }
            _ => return None,
        }
    }

    let mut idx = 0usize;
    for i in 0..pts.len() - 1 {
        if t >= pts[i].0 && t <= pts[i + 1].0 {
            idx = i;
            break;
        }
        idx = i;
    }
    let (xa, ya, da) = pts[idx];
    let (xb, yb, db) = pts[idx + 1];
    let h = xb - xa;
    if h == 0.0 {
        return Some(ya);
    }
    let s = (t - xa) / h;
    let s2 = s * s;
    let s3 = s2 * s;
    let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    let h10 = s3 - 2.0 * s2 + s;
    let h01 = -2.0 * s3 + 3.0 * s2;
    let h11 = s3 - s2;
    Some(h00 * ya + h10 * h * da + h01 * yb + h11 * h * db)
}

// ── FunctionInterpolation ───────────────────────────────────────────────────

/// FunctionInterpolation[expr, {x, xmin, xmax}] — sample `expr` on a grid and
/// build a piecewise-Hermite interpolant over the samples.
///
/// `FunctionInterpolation[expr, {x, xmin, xmax}, InterpolationOrder -> n]`
/// selects the interpolation order.
pub fn builtin_function_interpolation(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(unevaluated("FunctionInterpolation", args));
    }
    let expr = deep_unwrap(&arg_to_uneval(&args[0]));
    let spec = match deep_unwrap(&arg_to_uneval(&args[1])) {
        Value::List(items) if items.len() >= 3 => items,
        _ => return Ok(unevaluated("FunctionInterpolation", args)),
    };
    let var = match &spec[0] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(unevaluated("FunctionInterpolation", args)),
    };
    let (lo, hi) = match (eval_scalar(&spec[1], env), eval_scalar(&spec[2], env)) {
        (Some(a), Some(b)) => (a, b),
        _ => return Ok(unevaluated("FunctionInterpolation", args)),
    };
    if hi <= lo {
        return Ok(unevaluated("FunctionInterpolation", args));
    }

    let n = 100usize;
    let mut xs = Vec::with_capacity(n + 1);
    let mut ys = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let x = lo + (hi - lo) * (i as f64) / (n as f64);
        let sub = substitute(&expr, &var, &super::real(x));
        match eval_scalar(&sub, env) {
            Some(y) => {
                xs.push(x);
                ys.push(y);
            }
            None => return Ok(unevaluated("FunctionInterpolation", args)),
        }
    }

    let opts = parse_interp_options(&collect_options(&args[2..]));

    Ok(Value::PureFunction {
        body: Expr::Call {
            head: Box::new(Expr::Symbol("FitInterpolationFunction".to_string())),
            args: vec![
                crate::eval::table::value_to_expr(&build_hermite_rows(
                    &xs,
                    &ys,
                    opts.order,
                    opts.periodic,
                )),
                interp_meta_expr(&opts),
                Expr::Slot(Some(1)),
            ],
        },
        slot_count: 1,
        params: vec![],
        env: None,
    })
}

// ── MinValue / MaxValue ─────────────────────────────────────────────────────

/// Evaluate `expr` at each candidate and return the extremal value.
fn extremum(args: &[Value], env: &Env, maximize: bool, name: &str) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(unevaluated(name, args));
    }
    let first = arg_to_uneval(&args[0]);
    let second = arg_to_uneval(&args[1]);

    // `MinValue[{f, cons}, spec]` — take f and ignore the constraint body.
    let (expr, spec) = match &first {
        Value::List(items)
            if items.len() == 2
                && (matches!(second, Value::List(_)) || is_constraint(&items[1])) =>
        {
            (items[0].clone(), second.clone())
        }
        _ => (first.clone(), second.clone()),
    };

    let (var, bounds) = match &spec {
        Value::Symbol(s) => (s.clone(), None),
        Value::List(items) if !items.is_empty() => {
            let var = match &items[0] {
                Value::Symbol(s) => s.clone(),
                _ => return Ok(unevaluated(name, args)),
            };
            let bounds = if items.len() >= 3 {
                match (super::to_f64(&items[1]), super::to_f64(&items[2])) {
                    (Some(lo), Some(hi)) => Some((lo, hi)),
                    _ => None,
                }
            } else {
                None
            };
            (var, bounds)
        }
        _ => return Ok(unevaluated(name, args)),
    };

    let mut best: Option<f64> = None;
    let considering = |v: f64, best: &mut Option<f64>| {
        *best = Some(match *best {
            None => v,
            Some(b) if maximize => b.max(v),
            Some(b) => b.min(v),
        });
    };

    // Bounds and symbolic critical points (via D + Solve) as candidates.
    // Bounds provide exact endpoint candidates; the numerical scan handles the
    // interior (the symbolic D+Solve path is skipped because derivatives of
    // nested products can carry an unevaluated D term).
    let mut candidates: Vec<f64> = Vec::new();
    if let Some((lo, hi)) = bounds {
        candidates.push(lo);
        candidates.push(hi);
    }
    for c in &candidates {
        if let Some(v) = eval_scalar(&substitute(&expr, &var, &super::real(*c)), env) {
            considering(v, &mut best);
        }
    }

    // Dense numerical scan plus local refinement.
    let (lo, hi) = bounds.unwrap_or((-1000.0, 1000.0));
    if let Some(v) = numeric_extremum(&expr, &var, lo, hi, maximize, env) {
        considering(v, &mut best);
    }

    match best {
        Some(v) => Ok(super::real(v)),
        None => Ok(unevaluated(name, args)),
    }
}

/// Golden-section search for the minimum of `f` on `[a, b]`.
fn golden_min<F: Fn(f64) -> f64>(f: F, a: f64, b: f64) -> f64 {
    let inv_phi = 2.0 / (1.0 + 5.0f64.sqrt());
    let mut lo = a;
    let mut hi = b;
    let mut c = hi - (hi - lo) * inv_phi;
    let mut d = lo + (hi - lo) * inv_phi;
    let mut fc = f(c);
    let mut fd = f(d);
    for _ in 0..200 {
        if hi - lo < 1e-12 {
            break;
        }
        if fc < fd {
            hi = d;
            d = c;
            fd = fc;
            c = hi - (hi - lo) * inv_phi;
            fc = f(c);
        } else {
            lo = c;
            c = d;
            fc = fd;
            d = lo + (hi - lo) * inv_phi;
            fd = f(d);
        }
    }
    let x = (lo + hi) / 2.0;
    f(x)
}

/// Dense sample + local golden-section refinement for an extremum on `[lo, hi]`.
fn numeric_extremum(
    expr: &Value,
    var: &str,
    lo: f64,
    hi: f64,
    maximize: bool,
    env: &Env,
) -> Option<f64> {
    if hi <= lo {
        return eval_scalar(&substitute(expr, var, &super::real(lo)), env);
    }
    let f = |x: f64| {
        eval_scalar(&substitute(expr, var, &super::real(x)), env).unwrap_or(if maximize {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        })
    };
    let n = 2000usize;
    let mut best = f(lo);
    let mut bx = lo;
    for i in 1..=n {
        let x = lo + (hi - lo) * (i as f64) / (n as f64);
        let v = f(x);
        let better = if maximize { v > best } else { v < best };
        if better {
            best = v;
            bx = x;
        }
    }
    let step = (hi - lo) / (n as f64);
    let a = (bx - step).max(lo);
    let b = (bx + step).min(hi);
    if b > a {
        let refined = if maximize {
            -golden_min(|x| -f(x), a, b)
        } else {
            golden_min(f, a, b)
        };
        let better = if maximize {
            refined > best
        } else {
            refined < best
        };
        if better {
            best = refined;
        }
    }
    Some(best)
}

/// MinValue[f, x] — global minimum value of `f` in the variable `x`.
pub fn builtin_min_value(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    extremum(args, env, false, "MinValue")
}

/// MaxValue[f, x] — global maximum value of `f` in the variable `x`.
pub fn builtin_max_value(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    extremum(args, env, true, "MaxValue")
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::to_f64;

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
        to_f64(&eval_str(input)).unwrap_or_else(|| panic!("not numeric: {input}"))
    }

    fn assert_close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "expected {b}, got {a}");
    }

    #[test]
    fn test_least_squares_overdetermined() {
        let result = eval_str("LeastSquares[{{1, 0}, {0, 1}, {1, 1}}, {1, 1, 2}]");
        let expected = Value::List(vec![int(1), int(1)]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_least_squares_numeric() {
        let v = eval_str("LeastSquares[{{1., 0.}, {0., 1.}, {1., 2.}}, {1., 2., 3.}]");
        let items = match v {
            Value::List(items) => items,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_fit_linear() {
        let v = eval_str("CoefficientList[Fit[{{0, 1}, {1, 3}, {2, 5}, {3, 7}}, {1, x}, x], x]");
        let c = match v {
            Value::List(items) => items,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(c.len(), 2);
        assert_close(to_f64(&c[0]).unwrap(), 1.0);
        assert_close(to_f64(&c[1]).unwrap(), 2.0);
    }

    #[test]
    fn test_fit_quadratic() {
        let v =
            eval_str("CoefficientList[Fit[{{0, 1}, {1, 2}, {2, 5}, {3, 10}}, {1, x, x^2}, x], x]");
        let c = match v {
            Value::List(items) => items,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(c.len(), 3);
        assert_close(to_f64(&c[0]).unwrap(), 1.0);
        assert_close(to_f64(&c[1]).unwrap(), 0.0);
        assert_close(to_f64(&c[2]).unwrap(), 1.0);
    }

    #[test]
    fn test_find_fit_linear() {
        let a = num("a /. FindFit[{{0, 1}, {1, 3}, {2, 5}}, a x + b, {a, b}, x]");
        let b = num("b /. FindFit[{{0, 1}, {1, 3}, {2, 5}}, a x + b, {a, b}, x]");
        assert_close(a, 2.0);
        assert_close(b, 1.0);
    }

    #[test]
    fn test_find_fit_exponential() {
        let a = num("a /. FindFit[{{0, 1}, {1, 2.718281828}, {2, 7.389056099}}, a Exp[x], {a}, x]");
        assert_close(a, 1.0);
    }

    #[test]
    fn test_linear_optimization_basic() {
        let v = eval_str("LinearOptimization[{1, 1}, {{1, 2}, {1, 3}}, {4, 5}]");
        let items = match v {
            Value::List(items) => items,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(items.len(), 2);
        assert_close(to_f64(&items[0]).unwrap(), 0.0);
        assert_close(to_f64(&items[1]).unwrap(), 2.0);
    }

    #[test]
    fn test_linear_optimization_feasibility() {
        let v = eval_str("LinearOptimization[{1, 1}, {{1, 1}}, {3}]");
        let sum = match v {
            Value::List(items) => items.iter().filter_map(to_f64).sum::<f64>(),
            other => panic!("expected list, got {other:?}"),
        };
        assert_close(sum, 3.0);
    }

    #[test]
    fn test_linear_optimization_vertex() {
        let v = eval_str("LinearOptimization[{2, 3}, {{1, 1}, {1, 2}}, {2, 3}]");
        let items = match v {
            Value::List(items) => items,
            other => panic!("expected list, got {other:?}"),
        };
        assert_close(to_f64(&items[0]).unwrap(), 1.0);
        assert_close(to_f64(&items[1]).unwrap(), 1.0);
    }

    #[test]
    fn test_fit_rule_data() {
        let v = eval_str("CoefficientList[Fit[{0 -> 1, 1 -> 3, 2 -> 5}, {1, x}, x], x]");
        let c = match v {
            Value::List(items) => items,
            other => panic!("expected list, got {other:?}"),
        };
        assert_close(to_f64(&c[0]).unwrap(), 1.0);
        assert_close(to_f64(&c[1]).unwrap(), 2.0);
    }

    #[test]
    fn test_interpolation_quadratic() {
        let v = num("Interpolation[{{0, 0}, {1, 1}, {2, 3}}][1.5]");
        assert_close(v, 1.9375);
    }

    #[test]
    fn test_interpolation_cubic_reproduces_quadratic() {
        // Not-a-knot spline reproduces a quadratic exactly through its samples.
        let v = num("Interpolation[{{0, 0}, {1, 1}, {2, 4}, {3, 9}, {4, 16}}][2.5]");
        assert_close(v, 6.25);
    }

    #[test]
    fn test_interpolation_linear_order() {
        let v = num("Interpolation[{{0, 0}, {1, 2}, {2, 4}}, InterpolationOrder -> 1][0.5]");
        assert_close(v, 1.0);
    }

    #[test]
    fn test_debug() {
        eprintln!(
            "IF@0.5 => {:?}",
            eval_str("Interpolation[{{0, 0}, {1, 1}}][0.5]")
        );
        eprintln!("Dx2 => {:?}", eval_str("D[x^2, x]"));
        eprintln!("D4x => {:?}", eval_str("D[4 x, x]"));
        eprintln!("Dfull => {:?}", eval_str("D[x^2 - 4 x + 7, x]"));
        eprintln!(
            "NMin => {:?}",
            eval_str("NMinimize[x^2 - 4 x + 7, {x, -20, 20}]")
        );
        eprintln!(
            "NMax => {:?}",
            eval_str("NMaximize[-x^2 + 4 x + 1, {x, -20, 20}]")
        );
        eprintln!(
            "SimplifyD => {:?}",
            eval_str("Simplify[D[x^2 - 4 x + 7, x]]")
        );
        eprintln!("MinV => {:?}", eval_str("MinValue[x^2 - 4 x + 7, x]"));
        eprintln!("MinV2 => {:?}", eval_str("MinValue[x^2, x]"));
        eprintln!("subst => {:?}", eval_str("(x^2 - 4 x + 7) /. x -> 2"));
        eprintln!("Nsubst => {:?}", eval_str("N[(x^2 - 4 x + 7) /. x -> 2]"));
        eprintln!(
            "FitOut => {:?}",
            eval_str("Fit[{{0, 1}, {1, 3}, {2, 5}}, {1, x}, x]")
        );
        eprintln!(
            "FitAt => {:?}",
            eval_str("(Fit[{{0, 1}, {1, 3}, {2, 5}}, {1, x}, x]) /. x -> 4")
        );
        eprintln!(
            "FitEval => {:?}",
            eval_str("Evaluate[(Fit[{{0, 1}, {1, 3}, {2, 5}}, {1, x}, x]) /. x -> 4]")
        );
        let e = eval_str("x^2 - 4 x + 7");
        let env = env_with_builtins();
        for x in [2.0f64, 100.0, -100.0, 1.0] {
            eprintln!(
                "f({x})={:?}",
                eval_scalar(&substitute(&e, "x", &crate::builtins::real(x)), &env)
            );
        }
        eprintln!("evalExpr={:?}", e);
        eprintln!("repl => {:?}", eval_str("x + 1 /. x -> 4"));
        eprintln!("replN => {:?}", eval_str("N[x + 1 /. x -> 4]"));
        eprintln!("replP => {:?}", eval_str("(x + 1) /. x -> 4"));
        eprintln!("replPipe => {:?}", eval_str("x + 1 /. x -> 4 // N"));
        eprintln!("coef => {:?}", eval_str("CoefficientList[1.0 + 2.0 x, x]"));
        let em = eval_str("-x^2 + 4 x + 1");
        eprintln!("em={em:?}");
        eprintln!("p1={:?}", eval_str("4 x - x^2 + 1"));
        eprintln!("p2={:?}", eval_str("1 - x^2 + 4 x"));
        eprintln!("MX={:?}", eval_str("MaxValue[4 x - x^2, {x, -100, 100}]"));
        eprintln!("MX2={:?}", eval_str("MaxValue[1 - x^2 + 4 x, x]"));
        for x in [2.0f64, 3.0, 1.0] {
            eprintln!(
                "fm({x})={:?}",
                eval_scalar(&substitute(&em, "x", &crate::builtins::real(x)), &env)
            );
        }
        eprintln!(
            "nummax={:?}",
            numeric_extremum(&em, "x", -1000.0, 1000.0, true, &env)
        );
        eprintln!(
            "nummin={:?}",
            numeric_extremum(&em, "x", -1000.0, 1000.0, false, &env)
        );
    }

    #[test]
    fn test_interpolation_single_point() {
        let v = num("Interpolation[{{2, 5}}][3]");
        assert_close(v, 5.0);
    }

    #[test]
    fn test_interpolation_rule_data() {
        let v = num("Interpolation[{0 -> 0, 1 -> 1, 2 -> 3}][1.5]");
        assert_close(v, 1.9375);
    }

    #[test]
    fn test_min_value_constraint_form() {
        let v = num("MinValue[{x^2 + 1, x > 0}, x]");
        assert_close(v, 1.0);
    }

    #[test]
    fn test_min_value_quadratic() {
        let v = num("MinValue[x^2 - 4 x + 7, x]");
        assert_close(v, 3.0);
    }

    #[test]
    fn test_max_value_quadratic() {
        let v = num("MaxValue[1 - x^2 + 4 x, x]");
        assert_close(v, 5.0);
    }

    #[test]
    fn test_min_value_bounded() {
        let v = num("MinValue[x^2, {x, -3, 1}]");
        assert_close(v, 0.0);
    }

    #[test]
    fn test_nonlinear_model_fit_linear() {
        // The returned fit object is callable at a point.
        let v = num("NonlinearModelFit[{{0, 1}, {1, 3}, {2, 5}}, a x + b, {a, b}, x][4]");
        assert_close(v, 9.0);
    }

    #[test]
    fn test_nonlinear_model_fit_best_fit_parameters() {
        let a = num(
            "a /. NonlinearModelFit[{{0, 1}, {1, 3}, {2, 5}}, a x + b, {a, b}, x][\"BestFitParameters\"]",
        );
        let b = num(
            "b /. NonlinearModelFit[{{0, 1}, {1, 3}, {2, 5}}, a x + b, {a, b}, x][\"BestFitParameters\"]",
        );
        assert_close(a, 2.0);
        assert_close(b, 1.0);
    }

    #[test]
    fn test_nonlinear_model_fit_residuals() {
        let res = eval_str(
            "NonlinearModelFit[{{0, 1}, {1, 3}, {2, 5}}, a x + b, {a, b}, x][\"FitResiduals\"]",
        );
        let items = match res {
            Value::List(items) => items,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(items.len(), 3);
        for it in &items {
            assert_close(to_f64(it).unwrap(), 0.0);
        }
    }

    #[test]
    fn test_nonlinear_model_fit_property_call() {
        let a = num(
            "a /. NonlinearModelFit[{{0, 1}, {1, 3}, {2, 5}}, a x + b, {a, b}, x][Property[\"BestFitParameters\"]]",
        );
        assert_close(a, 2.0);
    }

    #[test]
    fn test_fit_residuals_builtin() {
        let res = eval_str(
            "FitResiduals[NonlinearModelFit[{{0, 1}, {1, 3}, {2, 5}}, a x + b, {a, b}, x]]",
        );
        let items = match res {
            Value::List(items) => items,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(items.len(), 3);
        for it in &items {
            assert_close(to_f64(it).unwrap(), 0.0);
        }
    }

    #[test]
    fn test_nonlinear_model_fit_exponential() {
        let a = num(
            "a /. NonlinearModelFit[{{0, 1}, {1, 2.718281828}, {2, 7.389056099}}, a Exp[x], {a}, x][\"BestFitParameters\"]",
        );
        assert_close(a, 1.0);
    }

    #[test]
    fn test_nonlinear_model_fit_inapplicable() {
        // Non-numeric data stays unevaluated.
        let v = eval_str("NonlinearModelFit[{{x, y}}, a x + b, {a, b}, x]");
        assert!(matches!(v, Value::Call { .. }));
    }

    #[test]
    fn test_function_interpolation_basic() {
        let v = num("FunctionInterpolation[x^2, {x, 0, 2}][1.0]");
        assert_close(v, 1.0);
    }

    #[test]
    fn test_function_interpolation_trig() {
        let v = num("FunctionInterpolation[Sin[x], {x, 0, Pi}][Pi/2]");
        assert_close(v, 1.0);
    }

    #[test]
    fn test_function_interpolation_inapplicable() {
        let v = eval_str("FunctionInterpolation[x^2, x]");
        assert!(matches!(v, Value::Call { .. }));
    }

    #[test]
    fn test_nonlinear_model_fit_fit_property() {
        // "Fit" yields the model with fitted parameters substituted in.
        // "Fit" yields the model with fitted parameters substituted in.
        let v = eval_str(
            "CoefficientList[NonlinearModelFit[{{0, 1}, {1, 3}, {2, 5}}, a x + b, {a, b}, x][\"Fit\"], x]",
        );
        let c = match v {
            Value::List(items) => items,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(c.len(), 2);
        assert_close(to_f64(&c[0]).unwrap(), 1.0);
        assert_close(to_f64(&c[1]).unwrap(), 2.0);
    }

    #[test]
    fn test_nonlinear_model_fit_fitfunction_property() {
        let f = "NonlinearModelFit[{{0, 1}, {1, 3}, {2, 5}}, a x + b, {a, b}, x][\"FitFunction\"]";
        let v = num(&format!("({f})[4]"));
        assert_close(v, 9.0);
    }

    #[test]
    fn test_fit_residuals_symbol_property() {
        let res = eval_str(
            "NonlinearModelFit[{{0, 1}, {1, 3}, {2, 5}}, a x + b, {a, b}, x][FitResiduals]",
        );
        let items = match res {
            Value::List(items) => items,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(items.len(), 3);
        for it in &items {
            assert_close(to_f64(it).unwrap(), 0.0);
        }
    }

    #[test]
    fn test_function_interpolation_order_option() {
        let v = num("FunctionInterpolation[x, {x, 0, 2}, InterpolationOrder -> 1][0.5]");
        assert_close(v, 0.5);
    }

    #[test]
    fn test_nonlinear_model_fit_infers_vars() {
        // The variable list may be omitted and is inferred from the model.
        let v = num("NonlinearModelFit[{{0, 1}, {1, 3}, {2, 5}}, a x + b, {a, b}][4]");
        assert_close(v, 9.0);
    }

    // ── Weighted fits ───────────────────────────────────────────────────────

    #[test]
    fn test_fit_weights_option() {
        // Emphasizing the last point shifts the slope away from the unweighted
        // least-squares value of 2.
        let v = num(
            "CoefficientList[Fit[{{0, 0}, {1, 1}, {2, 4}}, {1, x}, x, Weights -> {1, 1, 10}], x][[2]]",
        );
        assert_close(v, 111.0 / 51.0);
        let u = num("CoefficientList[Fit[{{0, 0}, {1, 1}, {2, 4}}, {1, x}, x], x][[2]]");
        assert_close(u, 2.0);
    }

    #[test]
    fn test_fit_weights_invalid_unevaluated() {
        let v = eval_str("Fit[{{0, 0}, {1, 1}}, {1, x}, x, Weights -> {1, 2, 3}]");
        assert!(matches!(v, Value::Call { .. }));
    }

    #[test]
    fn test_fit_weights_bound_variable() {
        let v = num(
            "w = {1, 1, 10}; CoefficientList[Fit[{{0, 0}, {1, 1}, {2, 4}}, {1, x}, x, Weights -> w], x][[2]]",
        );
        assert_close(v, 111.0 / 51.0);
    }

    #[test]
    fn test_find_fit_weights_option() {
        // Emphasizing the last point drags the linear fit toward it.
        let a = num(
            "a /. FindFit[{{0, 1}, {1, 2}, {2, 4}}, a x + b, {a, b}, x, Weights -> {1, 1, 100}]",
        );
        let b = num(
            "b /. FindFit[{{0, 1}, {1, 2}, {2, 4}}, a x + b, {a, b}, x, Weights -> {1, 1, 100}]",
        );
        assert_close(a, 801.0 / 501.0);
        assert_close(b, 401.0 / 501.0);
    }

    // ── LinearModelFit ──────────────────────────────────────────────────────

    const LINEAR_MODEL: &str = "LinearModelFit[{{0, 1}, {1, 3}, {2, 5}, {3, 7}}, {1, x}, x]";

    #[test]
    fn test_linear_model_fit_eval() {
        let v = num(&format!("({LINEAR_MODEL})[4]"));
        assert_close(v, 9.0);
    }

    #[test]
    fn test_linear_model_fit_best_fit_parameters() {
        let a = num(&format!("c1 /. ({LINEAR_MODEL})[\"BestFitParameters\"]"));
        let b = num(&format!("c2 /. ({LINEAR_MODEL})[\"BestFitParameters\"]"));
        assert_close(a, 1.0);
        assert_close(b, 2.0);
    }

    #[test]
    fn test_linear_model_fit_residuals() {
        let res = eval_str(&format!("({LINEAR_MODEL})[\"FitResiduals\"]"));
        let items = match res {
            Value::List(items) => items,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(items.len(), 4);
        for it in &items {
            assert_close(to_f64(it).unwrap(), 0.0);
        }
        let via_builtin = eval_str(&format!("FitResiduals[{LINEAR_MODEL}]"));
        assert!(matches!(via_builtin, Value::List(_)));
    }

    #[test]
    fn test_linear_model_fit_statistics() {
        assert_close(num(&format!("({LINEAR_MODEL})[\"RSquared\"]")), 1.0);
        assert_close(num(&format!("({LINEAR_MODEL})[\"AdjustedRSquared\"]")), 1.0);
        assert_close(
            num(&format!("({LINEAR_MODEL})[\"EstimatedVariance\"]")),
            0.0,
        );
        assert_close(
            num(&format!("({LINEAR_MODEL})[\"NumberOfParameters\"]")),
            2.0,
        );
    }

    #[test]
    fn test_linear_model_fit_design_matrix() {
        let dm = eval_str(&format!("DesignMatrix[{LINEAR_MODEL}]"));
        let rows = match dm {
            Value::List(rows) => rows,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(rows.len(), 4);
        let expected = [[1.0, 0.0], [1.0, 1.0], [1.0, 2.0], [1.0, 3.0]];
        for (row, exp) in rows.iter().zip(expected.iter()) {
            let cells = match row {
                Value::List(cells) => cells,
                other => panic!("expected row, got {other:?}"),
            };
            assert_eq!(cells.len(), 2);
            assert_close(to_f64(&cells[0]).unwrap(), exp[0]);
            assert_close(to_f64(&cells[1]).unwrap(), exp[1]);
        }
    }

    #[test]
    fn test_linear_model_fit_weights() {
        let b = num(
            "c2 /. (LinearModelFit[{{0, 0}, {1, 1}, {2, 4}}, {1, x}, x, Weights -> {1, 1, 10}][\"BestFitParameters\"])",
        );
        assert_close(b, 111.0 / 51.0);
    }

    #[test]
    fn test_linear_model_fit_inapplicable() {
        let v = eval_str("LinearModelFit[{{x, y}}, {1, x}, x]");
        assert!(matches!(v, Value::Call { .. }));
    }

    // ── Interpolation options / domains ─────────────────────────────────────

    #[test]
    fn test_interpolation_order_option_nontrivial() {
        // Order 1 must genuinely be linear on non-collinear data.
        let v = num("Interpolation[{{0, 0}, {1, 1}, {2, 0}}, InterpolationOrder -> 1][0.5]");
        assert_close(v, 0.5);
        let d = num("Interpolation[{{0, 0}, {1, 1}, {2, 0}}][0.5]");
        assert!(d > 0.5);
    }

    #[test]
    fn test_interpolation_default_extrapolation() {
        // Extrapolation continues the nearest polynomial piece.
        let v = num("Interpolation[{{0, 0}, {1, 1}, {2, 4}}][3]");
        assert_close(v, 5.0);
    }

    #[test]
    fn test_interpolation_extrapolation_handler_indeterminate() {
        let v = eval_str(
            "Interpolation[{{0, 0}, {1, 1}, {2, 4}}, ExtrapolationHandler -> Indeterminate][3]",
        );
        assert_eq!(v, Value::Symbol("Indeterminate".to_string()));
    }

    #[test]
    fn test_interpolation_extrapolation_handler_function() {
        let v = num("Interpolation[{{0, 0}, {1, 1}, {2, 4}}, ExtrapolationHandler -> (#^2 &)][3]");
        assert_close(v, 9.0);
        // Inside the domain the interpolant is still used.
        let w =
            num("Interpolation[{{0, 0}, {1, 1}, {2, 4}}, ExtrapolationHandler -> (#^2 &)][1.5]");
        assert_close(w, 2.375);
    }

    #[test]
    fn test_interpolation_periodic() {
        let v = num(
            "Interpolation[{{0, 0}, {1, 1}, {2, 0}}, PeriodicInterpolation -> True, InterpolationOrder -> 1][3]",
        );
        assert_close(v, 1.0);
        let w = num(
            "Interpolation[{{0, 0}, {1, 1}, {2, 0}}, PeriodicInterpolation -> True, InterpolationOrder -> 1][-1]",
        );
        assert_close(w, 1.0);
    }

    #[test]
    fn test_interpolation_2d_grid() {
        let v = num(
            "Interpolation[Flatten[Table[{i, j, i + j}, {i, 0, 3}, {j, 0, 3}], 1]][{1.5, 2.5}]",
        );
        assert_close(v, 4.0);
        let q = num(
            "Interpolation[Flatten[Table[{i, j, i^2 + j^2}, {i, 0, 4}, {j, 0, 4}], 1]][{1.5, 2.5}]",
        );
        assert_close(q, 8.5);
    }

    #[test]
    fn test_interpolation_2d_scatter() {
        let v = num("Interpolation[{{0, 0, 0}, {1, 0, 1}, {0, 1, 1}}][{0.5, 0.5}]");
        assert_close(v, 1.0);
    }

    #[test]
    fn test_interpolation_2d_periodic() {
        let v = num(
            "Interpolation[Flatten[Table[{i, j, i}, {i, 0, 2}, {j, 0, 1}], 1], PeriodicInterpolation -> True, InterpolationOrder -> 1][{3, 0.5}]",
        );
        assert_close(v, 1.0);
    }

    #[test]
    fn test_interpolation_inapplicable() {
        let v = eval_str("Interpolation[x]");
        assert!(matches!(v, Value::Call { .. }));
    }
}
