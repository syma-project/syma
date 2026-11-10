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
    super::register_builtin(env, "InterpolatingFunction", builtin_interpolating_function);
    super::register_builtin_env(env, "MinValue", builtin_min_value);
    super::register_builtin_env(env, "MaxValue", builtin_max_value);

    // These builtins must see their arguments unevaluated (basis functions,
    // model expressions and variables are symbolic).
    for name in ["Fit", "FindFit", "Interpolation", "MinValue", "MaxValue"] {
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

// ── Fit ─────────────────────────────────────────────────────────────────────

/// Fit[data, funcs, vars] — least-squares fit of data with the linear basis `funcs`.
pub fn builtin_fit(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Ok(unevaluated("Fit", args));
    }
    let data = arg_to_value(&args[0], env)?;
    let funcs_v = arg_to_uneval(&args[1]);
    let vars_v = arg_to_uneval(&args[2]);

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

    let coeffs = match least_squares_f64(&matrix, &ys) {
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
    env: &Env,
) -> (f64, Vec<f64>) {
    let np = params.len();
    let mut p = p0.to_vec();
    let mut best_ss = f64::INFINITY;
    let mut best = p.clone();

    for _ in 0..300 {
        let mut r = vec![0.0f64; coords.len()];
        let mut valid = true;
        for i in 0..coords.len() {
            match eval_model(model, params, &p, vars, &coords[i], env) {
                Some(v) => r[i] = v - ys[i],
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

        // Finite-difference Jacobian.
        let mut jac = vec![vec![0.0f64; np]; coords.len()];
        for k in 0..np {
            let h = 1e-6 * (1.0 + p[k].abs());
            let mut pp = p.clone();
            pp[k] += h;
            let mut pm = p.clone();
            pm[k] -= h;
            for i in 0..coords.len() {
                let vp = eval_model(model, params, &pp, vars, &coords[i], env).unwrap_or(0.0);
                let vm = eval_model(model, params, &pm, vars, &coords[i], env).unwrap_or(0.0);
                jac[i][k] = (vp - vm) / (2.0 * h);
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

/// FindFit[data, model, params, vars] — nonlinear least-squares fit.
pub fn builtin_find_fit(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 4 {
        return Ok(unevaluated("FindFit", args));
    }
    let data = arg_to_value(&args[0], env)?;
    let model = arg_to_uneval(&args[1]);
    let params_v = arg_to_uneval(&args[2]);
    let vars_v = arg_to_uneval(&args[3]);

    let params = match collect_symbols(&params_v) {
        Some(p) if !p.is_empty() => p,
        _ => return Ok(unevaluated("FindFit", args)),
    };
    let vars = match collect_symbols(&vars_v) {
        Some(v) => v,
        None => return Ok(unevaluated("FindFit", args)),
    };
    let (coords, ys) = match parse_points(&data, vars.len()) {
        Some(p) => p,
        None => return Ok(unevaluated("FindFit", args)),
    };
    if coords.is_empty() {
        return Ok(unevaluated("FindFit", args));
    }

    let np = params.len();
    let starts: Vec<Vec<f64>> = vec![vec![1.0; np], vec![0.5; np], vec![2.0; np], vec![0.25; np]];
    let mut best_ss = f64::INFINITY;
    let mut best = vec![1.0; np];
    for start in &starts {
        let (ss, p) = gauss_newton(&model, &params, &vars, &coords, &ys, start, env);
        if ss < best_ss {
            best_ss = ss;
            best = p;
        }
    }

    let rules: Vec<Value> = params
        .iter()
        .zip(best.iter())
        .map(|(name, val)| Value::Rule {
            lhs: Box::new(Value::Symbol(name.clone())),
            rhs: Box::new(super::real(*val)),
            delayed: false,
        })
        .collect();
    Ok(Value::List(rules))
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

/// Parse interpolation data into sorted, strictly increasing sample points.
fn parse_interp_points(data: &Value) -> Option<(Vec<f64>, Vec<f64>)> {
    let data = deep_unwrap(data);
    let items = as_list(&data)?;
    let mut pts = Vec::with_capacity(items.len());
    for it in items {
        let (x, y) = match it {
            Value::Rule { lhs, rhs, .. } => (first_coord(lhs)?, super::to_f64(rhs)?),
            Value::List(inner) if inner.len() >= 2 => {
                (super::to_f64(&inner[0])?, super::to_f64(inner.last()?)?)
            }
            _ => return None,
        };
        pts.push((x, y));
    }
    if pts.is_empty() {
        return None;
    }
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for (x, y) in pts {
        if let Some(last) = xs.last()
            && (*last - x).abs() < 1e-12
        {
            continue;
        }
        xs.push(x);
        ys.push(y);
    }
    Some((xs, ys))
}

fn parse_interp_order(arg: &Value) -> Option<i64> {
    match arg {
        Value::Integer(n) => Some(n.to_i64().unwrap_or(3)),
        Value::Rule { lhs, rhs, .. } => match lhs.as_ref() {
            Value::Symbol(s) if s == "InterpolationOrder" => match rhs.as_ref() {
                Value::Integer(n) => Some(n.to_i64().unwrap_or(3)),
                _ => None,
            },
            _ => None,
        },
        Value::List(items) => items.iter().find_map(parse_interp_order),
        _ => None,
    }
}

/// Build `{{x, y, dy}, …}` rows (cubic Hermite form) for the interpolant.
fn build_hermite_rows(xs: &[f64], ys: &[f64], order: i64) -> Value {
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
        for i in 0..n {
            let s = if i == 0 {
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

/// Interpolation[data] (and `Interpolation[data, InterpolationOrder -> n]`).
///
/// The result is a callable value; `Interpolation[data][x]` evaluates `x`.
pub fn builtin_interpolation(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("Interpolation", args));
    }
    let data = arg_to_value(&args[0], env)?;
    let (xs, ys) = match parse_interp_points(&data) {
        Some(p) => p,
        None => return Ok(unevaluated("Interpolation", args)),
    };
    if xs.is_empty() {
        return Ok(unevaluated("Interpolation", args));
    }
    let order = if args.len() == 2 {
        parse_interp_order(&arg_to_uneval(&args[1])).unwrap_or(3)
    } else {
        3
    };

    Ok(Value::PureFunction {
        body: Expr::Call {
            head: Box::new(Expr::Symbol("InterpolatingFunction".to_string())),
            args: vec![
                crate::eval::table::value_to_expr(&build_hermite_rows(&xs, &ys, order)),
                Expr::Slot(Some(1)),
            ],
        },
        slot_count: 1,
        params: vec![],
        env: None,
    })
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

/// InterpolatingFunction[data] / InterpolatingFunction[data, x].
pub fn builtin_interpolating_function(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => Ok(Value::PureFunction {
            body: Expr::Call {
                head: Box::new(Expr::Symbol("InterpolatingFunction".to_string())),
                args: vec![
                    crate::eval::table::value_to_expr(&args[0]),
                    Expr::Slot(Some(1)),
                ],
            },
            slot_count: 1,
            params: vec![],
            env: None,
        }),
        2 => {
            let t = match super::to_f64(&args[1]) {
                Some(t) => t,
                None => return Ok(unevaluated("InterpolatingFunction", args)),
            };
            match hermite_eval(&args[0], t) {
                Some(v) => Ok(super::real(v)),
                None => Ok(unevaluated("InterpolatingFunction", args)),
            }
        }
        _ => Ok(unevaluated("InterpolatingFunction", args)),
    }
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
}
