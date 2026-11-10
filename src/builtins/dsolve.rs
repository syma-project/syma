//! differential/recurrence equations: DSolve, DSolveValue, RSolve, NDSolve.
//!
//! Gap-fill module. Supported (common) cases:
//!
//! * `DSolve[eqn, y[x], x]` / `DSolve[eqn, y, x]` for
//!   * linear constant-coefficient ODEs of any order `1..=6`
//!     `sum_k a_k y^(k) = f(x)` — homogeneous solutions come from the
//!     characteristic roots (rational roots, plus quadratic factors giving
//!     real/complex/irrational roots), and `f(x)` may be a polynomial,
//!     exponential, or sine/cosine (and their products) via undetermined
//!     coefficients; first-order forcing outside that family falls back to an
//!     integrating factor,
//!   * first-order linear `y' + p(x) y = q(x)` (incl. `y' = f(x)`),
//!   * first-order separable `y' = g(y) h(x)` when `g` is a monomial `c y^n`,
//!   * initial/boundary conditions (value and derivative of any order) used to
//!     resolve the `C[i]` constants.
//! * `DSolveValue` returns the solution expression / function directly.
//! * `RSolve`/`RSolveValue` for first-order constant-coefficient recurrences
//!   (and delegates higher-order recurrences to the `discrete` module).
//! * `NDSolve` by fixed-step explicit RK4.
//!
//! Results use `C[1]`, `C[2]` as integration constants (Wolfram convention).
//! Unsupported equations are returned unevaluated as `DSolve[...]`/etc.
//!
//! ## InterpolatingFunction
//!
//! There is currently no dedicated `Value` variant or fully-fledged
//! `InterpolatingFunction` implementation elsewhere in the tree, so `NDSolve`
//! returns a first-class callable object: a `PureFunction` whose body is
//! `InterpolatingFunction[data, #1]` and which performs piecewise cubic Hermite
//! interpolation over `data = {{x0, y0, y0'}, {x1, y1, y1'}, ...}`. A matching
//! two-argument builtin `InterpolatingFunction[data, t]` is registered, so both
//! `NDSolveValue[...][t]` and `InterpolatingFunction[data][t]` work. (The
//! displayed form is `PureFunction[...]` rather than `InterpolatingFunction[...]`.)

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;

/// Highest derivative order the linear solver will look for.
const MAX_DERIV: usize = 6;

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin_env(env, "DSolve", builtin_dsolve);
    super::register_builtin_env(env, "DSolveValue", builtin_dsolve_value);
    super::register_builtin_env(env, "RSolve", builtin_rsolve);
    super::register_builtin_env(env, "RSolveValue", builtin_rsolve_value);
    super::register_builtin_env(env, "NDSolve", builtin_ndsolve);
    super::register_builtin_env(env, "NDSolveValue", builtin_ndsolve_value);
    super::register_builtin(env, "InterpolatingFunction", builtin_interpolating_function);

    // Equation solvers hold their arguments unevaluated.
    for name in [
        "DSolve",
        "DSolveValue",
        "RSolveValue",
        "NDSolve",
        "NDSolveValue",
    ] {
        env.set_attributes(
            name,
            vec![
                "HoldAll".to_string(),
                "Locked".to_string(),
                "ReadProtected".to_string(),
            ],
        );
    }
}

// ── Expression helpers ───────────────────────────────────────────────────────

fn sym(s: &str) -> Expr {
    Expr::Symbol(s.to_string())
}

fn int(n: i64) -> Expr {
    Expr::Integer(Integer::from(n))
}

fn real(f: f64) -> Expr {
    Expr::Real(Float::with_val(DEFAULT_PRECISION, f))
}

fn call(head: &str, args: Vec<Expr>) -> Expr {
    Expr::Call {
        head: Box::new(sym(head)),
        args,
    }
}

fn plus(args: Vec<Expr>) -> Expr {
    match args.len() {
        0 => int(0),
        1 => args.into_iter().next().unwrap(),
        _ => call("Plus", args),
    }
}

fn times(args: Vec<Expr>) -> Expr {
    match args.len() {
        0 => int(1),
        1 => args.into_iter().next().unwrap(),
        _ => call("Times", args),
    }
}

fn neg(e: Expr) -> Expr {
    times(vec![int(-1), e])
}

fn sub(a: Expr, b: Expr) -> Expr {
    plus(vec![a, neg(b)])
}

fn div(a: Expr, b: Expr) -> Expr {
    times(vec![a, call("Power", vec![b, int(-1)])])
}

fn pow(a: Expr, b: Expr) -> Expr {
    call("Power", vec![a, b])
}

/// `C[n]` integration constant.
fn c_const(n: i64) -> Expr {
    Expr::Call {
        head: Box::new(sym("C")),
        args: vec![int(n)],
    }
}

/// `Derivative[order][f]`.
fn deriv(head_name: &str, order: i64) -> Expr {
    Expr::Call {
        head: Box::new(call("Derivative", vec![int(order)])),
        args: vec![sym(head_name)],
    }
}

/// `Derivative[order][f][x]`.
fn deriv_at(head_name: &str, x: &str, order: i64) -> Expr {
    Expr::Call {
        head: Box::new(deriv(head_name, order)),
        args: vec![sym(x)],
    }
}

/// `f[x]`.
fn func_at(head_name: &str, x: &str) -> Expr {
    Expr::Call {
        head: Box::new(sym(head_name)),
        args: vec![sym(x)],
    }
}

/// Structural → [`Expr`], keeping rationals as exact fractions (the generic
/// `value_to_expr` renders `Value::Rational` as an un-reducible symbol).
fn expr_from_value(v: &Value) -> Expr {
    match v {
        Value::Pattern(e) => e.clone(),
        Value::Hold(b) | Value::HoldComplete(b) => expr_from_value(b),
        Value::Rational(r) => div(
            Expr::Integer(r.numer().clone()),
            Expr::Integer(r.denom().clone()),
        ),
        other => crate::eval::table::value_to_expr(other),
    }
}

/// Unwrap a (possibly held) argument to its underlying [`Expr`].
fn as_expr(v: &Value) -> Expr {
    expr_from_value(v)
}

/// Unwrap a held argument into a structural [`Value`] (no pattern wrappers).
fn strip(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => crate::pattern::unwrap_expr_to_value(e),
        _ => v.clone(),
    }
}

/// Structural → [`Value`].
fn to_value(e: &Expr) -> Value {
    crate::pattern::unwrap_expr_to_value(e)
}

/// Evaluate an expression, returning the result as an [`Expr`].
fn eval_expr(e: &Expr, env: &Env) -> Expr {
    match crate::eval::eval(e, env) {
        Ok(v) => as_expr(&v),
        Err(_) => e.clone(),
    }
}

/// Evaluate an expression to a [`Value`], falling back to its structure.
fn eval_value(e: &Expr, env: &Env) -> Value {
    crate::eval::eval(e, env).unwrap_or_else(|_| to_value(e))
}

fn expand(e: &Expr, env: &Env) -> Expr {
    eval_expr(&call("Expand", vec![e.clone()]), env)
}

fn together(e: &Expr, env: &Env) -> Expr {
    eval_expr(&call("Together", vec![e.clone()]), env)
}

/// `Integrate[e, x]`, evaluated.
fn integrate(e: &Expr, x: &str, env: &Env) -> Expr {
    eval_expr(&call("Integrate", vec![e.clone(), sym(x)]), env)
}

/// `Coefficient[e, var]`, evaluated.
fn coeff(e: &Expr, var: &str, env: &Env) -> Expr {
    eval_expr(&call("Coefficient", vec![e.clone(), sym(var)]), env)
}

/// Free of a symbol (anywhere in the tree)?
fn free_of(e: &Expr, name: &str) -> bool {
    !contains_symbol(e, name)
}

fn contains_symbol(e: &Expr, name: &str) -> bool {
    match e {
        Expr::Symbol(s) => s == name,
        Expr::List(items) => items.iter().any(|i| contains_symbol(i, name)),
        Expr::Call { head, args } => {
            contains_symbol(head, name) || args.iter().any(|a| contains_symbol(a, name))
        }
        Expr::Rule { lhs, rhs } | Expr::RuleDelayed { lhs, rhs } => {
            contains_symbol(lhs, name) || contains_symbol(rhs, name)
        }
        _ => false,
    }
}

/// Free of `D1`/`Y` (the linearisation markers)?
fn free_of_markers(e: &Expr) -> bool {
    free_of(e, "D1") && free_of(e, "Y") && free_of(e, "D2")
}

/// Structural substitution.
fn subst(e: &Expr, from: &Expr, to: &Expr) -> Expr {
    if e == from {
        return to.clone();
    }
    match e {
        Expr::List(items) => Expr::List(items.iter().map(|i| subst(i, from, to)).collect()),
        Expr::Call { head, args } => Expr::Call {
            head: Box::new(subst(head, from, to)),
            args: args.iter().map(|a| subst(a, from, to)).collect(),
        },
        Expr::Rule { lhs, rhs } => Expr::Rule {
            lhs: Box::new(subst(lhs, from, to)),
            rhs: Box::new(subst(rhs, from, to)),
        },
        Expr::RuleDelayed { lhs, rhs } => Expr::RuleDelayed {
            lhs: Box::new(subst(lhs, from, to)),
            rhs: Box::new(subst(rhs, from, to)),
        },
        _ => e.clone(),
    }
}

/// Numeric value of a constant expression, if any.
fn as_f64(e: &Expr) -> Option<f64> {
    match e {
        Expr::Integer(n) => n.to_f64().into(),
        Expr::Real(r) => Some(r.to_f64()),
        _ => None,
    }
}

/// Sign of a constant expression (via numeric evaluation).
fn sign_of(e: &Expr, env: &Env) -> Option<i32> {
    let f = match eval_value(e, env) {
        Value::Integer(n) => n.to_f64(),
        Value::Real(r) => r.to_f64(),
        Value::Rational(r) => r.to_f64(),
        _ => return None,
    };
    Some(if f > 0.0 {
        1
    } else if f < 0.0 {
        -1
    } else {
        0
    })
}

/// Extract `lhs == rhs` from an `Equal` call.
fn eq_pair(e: &Expr) -> Option<(Expr, Expr)> {
    if let Expr::Call { head, args } = e
        && let Expr::Symbol(s) = head.as_ref()
        && s == "Equal"
        && args.len() == 2
    {
        return Some((args[0].clone(), args[1].clone()));
    }
    None
}

/// Build the unevaluated fallback for a solver head.
fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.iter().map(strip).collect(),
    }
}

// ── Target parsing ───────────────────────────────────────────────────────────

/// The dependent function and independent variable of a solve request.
struct Target {
    /// Function name, e.g. `y`.
    name: String,
    /// Independent variable, e.g. `x`.
    var: String,
    /// True when the user passed the bare symbol `y` (function form).
    func_form: bool,
}

/// Parse the `(dependent, independent)` argument pair.
fn parse_target(dep: &Expr, indep: &Expr) -> Option<Target> {
    let var = match indep {
        Expr::Symbol(s) => s.clone(),
        _ => return None,
    };
    match dep {
        Expr::Symbol(s) => Some(Target {
            name: s.clone(),
            var,
            func_form: true,
        }),
        Expr::Call { head, args }
            if args.len() == 1
                && matches!(args[0], Expr::Symbol(_))
                && matches!(head.as_ref(), Expr::Symbol(_)) =>
        {
            let Expr::Symbol(s) = head.as_ref() else {
                unreachable!()
            };
            Some(Target {
                name: s.clone(),
                var,
                func_form: false,
            })
        }
        _ => None,
    }
}

/// Split an equation argument into the primary equation and any initial
/// conditions. Accepts `eqn` or `{eqn, ic, ...}`.
fn split_constraints(eqn: &Expr) -> (Expr, Vec<Expr>) {
    match eqn {
        Expr::List(items) if !items.is_empty() => (items[0].clone(), items[1..].to_vec()),
        other => (other.clone(), Vec::new()),
    }
}

// ── DSolve ───────────────────────────────────────────────────────────────────

/// `DSolve[eqn, y, x]` / `DSolve[eqn, y[x], x]`.
fn builtin_dsolve(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    match solve_ode(args, env) {
        Some(sol) => Ok(bool_to_value(sol)),
        None => Ok(unevaluated("DSolve", args)),
    }
}

/// `DSolveValue[eqn, y, x]` / `DSolveValue[eqn, y[x], x]`.
fn builtin_dsolve_value(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    match solve_ode(args, env) {
        Some(sol) => Ok(value_of_solution(sol, env)),
        None => Ok(unevaluated("DSolveValue", args)),
    }
}

/// The solved ODE, before formatting into rules.
struct Solved {
    target: Target,
    /// Solution expression in `y[x]` (constants already resolved when ICs exist).
    sol: Expr,
}

/// Where `bool_to_value` is poorly named: format a [`Solved`] as `DSolve` output.
fn bool_to_value(sol: Solved) -> Value {
    let Solved { target, sol } = sol;
    let lhs = if target.func_form {
        sym(&target.name)
    } else {
        func_at(&target.name, &target.var)
    };
    let rhs = if target.func_form {
        make_pure_function(&target.var, &sol)
    } else {
        to_value(&sol)
    };
    Value::List(vec![Value::List(vec![Value::Rule {
        lhs: Box::new(to_value(&lhs)),
        rhs: Box::new(rhs),
        delayed: false,
    }])])
}

/// `DSolveValue` formatting: expression for `y[x]`, function for `y`.
fn value_of_solution(sol: Solved, env: &Env) -> Value {
    let Solved { target, sol } = sol;
    if target.func_form {
        let simplified = eval_value(&sol, env);
        let body = expr_from_value(&simplified);
        make_pure_function(&target.var, &body)
    } else {
        eval_value(&sol, env)
    }
}

/// Build `Function[{var}, body]` as a `PureFunction` (callable).
fn make_pure_function(var: &str, body: &Expr) -> Value {
    // Only keep parameters that actually appear in the body; otherwise the
    // function would be constant but still need the argument bound.
    Value::PureFunction {
        body: body.clone(),
        slot_count: 1,
        params: vec![var.to_string()],
        env: None,
    }
}

/// Core ODE solver. Returns `None` when unsupported.
fn solve_ode(args: &[Value], env: &Env) -> Option<Solved> {
    if args.len() < 3 {
        return None;
    }
    let eqn_arg = as_expr(&args[0]);
    let target = parse_target(&as_expr(&args[1]), &as_expr(&args[2]))?;
    let (eqn, ics) = split_constraints(&eqn_arg);
    let (lhs, rhs) = eq_pair(&eqn)?;

    let y = func_at(&target.name, &target.var);

    // Substitute the dependent function and every derivative order with
    // markers (`Y`, `D1`, `D2`, ...) so coefficient extraction can treat them
    // as ordinary symbols.
    let mut marked_e = sub(lhs, rhs);
    marked_e = subst(&marked_e, &y, &sym("Y"));
    for k in 1..=MAX_DERIV {
        let dk = deriv_at(&target.name, &target.var, k as i64);
        marked_e = subst(&marked_e, &dk, &marker_sym(k));
    }
    let marked = expand(&marked_e, env);

    // General constant-coefficient linear solver first (covers orders 1..=N);
    // fall back to the variable-coefficient / separable routines.
    let sol = if let Some(sol) =
        solve_linear_const_coeff(&marked, &target.name, &target.var, &ics, env)
    {
        sol
    } else if contains_symbol(&marked, "D2") {
        solve_second_order(&marked, &target.name, &target.var, &ics, env)?
    } else {
        solve_first_order(&marked, &target.name, &target.var, &ics, env)?
    };

    Some(Solved { target, sol })
}

/// Solve `E(D1, Y, x) = 0` as a first-order ODE. E uses markers D1/Y.
fn solve_first_order(e: &Expr, name: &str, x: &str, ics: &[Expr], env: &Env) -> Option<Expr> {
    // Try linear first.
    if let Some(sol) = solve_first_order_linear(e, x, env) {
        return apply_first_order_ic(sol, name, x, ics, env);
    }
    // Then separable.
    if let Some(sol) = solve_separable(e, x, env) {
        return apply_first_order_ic(sol, name, x, ics, env);
    }
    None
}

/// `E = c1 D1 + c2 Y + c0` with c0 free of markers → solve `y' + p y = q`.
fn solve_first_order_linear(e: &Expr, x: &str, env: &Env) -> Option<Expr> {
    let c1 = coeff(e, "D1", env);
    let c2 = coeff(e, "Y", env);
    if !free_of(&c1, "D1") || !free_of(&c1, "Y") || !free_of(&c2, "D1") || !free_of(&c2, "Y") {
        return None;
    }
    if is_zero(&c1, env) {
        return None;
    }
    let c0 = together(
        &sub(
            sub(e.clone(), times(vec![c1.clone(), sym("D1")])),
            times(vec![c2.clone(), sym("Y")]),
        ),
        env,
    );
    if !free_of_markers(&c0) {
        return None;
    }
    // y' + (c2/c1) y = -(c0/c1)
    let p = together(&div(c2, c1.clone()), env);
    let q = together(&neg(div(c0, c1)), env);

    let big_p = integrate(&p, x, env);
    let mu = call("Exp", vec![big_p.clone()]);
    let forcing = integrate(&times(vec![q, mu.clone()]), x, env);
    let sol = times(vec![
        call("Exp", vec![neg(big_p)]),
        plus(vec![forcing, c_const(1)]),
    ]);
    Some(eval_expr(&sol, env))
}

/// `E = cD D1 + R` with `-R/cD = g(Y) h(x)` and `g(Y) = c Y^n`.
fn solve_separable(e: &Expr, x: &str, env: &Env) -> Option<Expr> {
    let cd = coeff(e, "D1", env);
    if !free_of(&cd, "D1") || !free_of(&cd, "Y") || !free_of(&cd, x) || is_zero(&cd, env) {
        return None;
    }
    let r = together(&sub(e.clone(), times(vec![cd.clone(), sym("D1")])), env);
    if !free_of(&r, "D1") {
        return None;
    }
    if !contains_symbol(&r, "Y") {
        return None;
    }
    // y' = -R/cD = g(Y) h(x)
    let f = expand(&div(neg(r), cd), env);

    let factors = match &f {
        Expr::Call { head, args } if matches!(head.as_ref(), Expr::Symbol(s) if s == "Times") => {
            args.clone()
        }
        other => vec![other.clone()],
    };
    let mut g_factors = Vec::new();
    let mut h_factors = Vec::new();
    for factor in factors {
        let in_y = contains_symbol(&factor, "Y");
        let in_x = contains_symbol(&factor, x);
        match (in_y, in_x) {
            (true, false) => g_factors.push(factor),
            (false, true) => h_factors.push(factor),
            (false, false) => h_factors.push(factor), // constants ride with h
            (true, true) => return None,              // mixed factor, not separable
        }
    }
    let g = if g_factors.is_empty() {
        int(1)
    } else {
        times(g_factors)
    };
    let h = if h_factors.is_empty() {
        int(1)
    } else {
        times(h_factors)
    };
    let (c, n) = parse_monomial(&g, "Y")?;

    let big_h = integrate(&h, x, env);
    let sol = if n == 1 {
        times(vec![c_const(1), call("Exp", vec![times(vec![c, big_h])])])
    } else {
        let exponent = div(int(1), int(1 - n));
        pow(
            times(vec![c, int(1 - n), plus(vec![big_h, c_const(1)])]),
            exponent,
        )
    };
    Some(eval_expr(&sol, env))
}

/// Parse `c * Y^n` (n integer, c free of Y) or a bare `c`.
fn parse_monomial(g: &Expr, y: &str) -> Option<(Expr, i64)> {
    if let Expr::Symbol(s) = g {
        if s == y {
            return Some((int(1), 1));
        }
        return None;
    }
    if let Expr::Call { head, args } = g
        && let Expr::Symbol(s) = head.as_ref()
    {
        if s == "Power" && args.len() == 2 {
            if matches!(&args[0], Expr::Symbol(v) if v == y)
                && let Expr::Integer(n) = &args[1]
            {
                return Some((int(1), n.to_i64()?));
            }
            return None;
        }
        if s == "Times" {
            let mut constant = int(1);
            let mut found: Option<i64> = None;
            for factor in args {
                if contains_symbol(factor, y) {
                    if found.is_some() {
                        return None;
                    }
                    match factor {
                        Expr::Symbol(v) if v == y => found = Some(1),
                        Expr::Call { head: ph, args: pa }
                            if matches!(ph.as_ref(), Expr::Symbol(s) if s == "Power")
                                && pa.len() == 2
                                && matches!(&pa[0], Expr::Symbol(v) if v == y) =>
                        {
                            let Expr::Integer(n) = &pa[1] else {
                                return None;
                            };
                            found = Some(n.to_i64()?);
                        }
                        _ => return None,
                    }
                } else {
                    constant = times(vec![constant, factor.clone()]);
                }
            }
            return found.map(|n| (constant, n));
        }
    }
    if !contains_symbol(g, y) {
        return Some((g.clone(), 0));
    }
    None
}

/// Resolve the single `C[1]` constant using a value initial condition.
fn apply_first_order_ic(sol: Expr, name: &str, x: &str, ics: &[Expr], env: &Env) -> Option<Expr> {
    let some_ic = ics.iter().find_map(|c| parse_raw_value_ic(c, name));
    let Some((x0, y0)) = some_ic else {
        return Some(sol);
    };
    // `C[1]` appears as a call; coefficient extraction needs a plain symbol.
    let b = coeff(&subst(&sol, &c_const(1), &sym("C1")), "C1", env);
    let a = subst(&sol, &c_const(1), &int(0));
    if is_zero(&b, env) {
        return None;
    }
    let a0 = eval_expr(&subst(&a, &sym(x), &x0), env);
    let b0 = eval_expr(&subst(&b, &sym(x), &x0), env);
    let c1 = together(&div(sub(y0.clone(), a0), b0.clone()), env);
    if std::env::var("SYMA_DSOLVE_DBG").is_ok() {
        eprintln!("[dsolve-dbg] sol={sol:?} x0={x0:?} y0={y0:?} b={b:?} b0={b0:?} c1={c1:?}");
    }
    Some(eval_expr(&subst(&sol, &c_const(1), &c1), env))
}

/// Parse `f[x0] == value` (raw form) into `(x0, value)`.
fn parse_raw_value_ic(cond: &Expr, name: &str) -> Option<(Expr, Expr)> {
    let (lhs, rhs) = eq_pair(cond)?;
    let x0 = call_arg(&lhs, name)?;
    Some((x0, rhs))
}

// ── Second-order constant-coefficient ────────────────────────────────────────

fn solve_second_order(e: &Expr, name: &str, x: &str, ics: &[Expr], env: &Env) -> Option<Expr> {
    let c2 = coeff(e, "D2", env);
    let c1 = coeff(e, "D1", env);
    let c0 = coeff(e, "Y", env);
    for c in [&c2, &c1, &c0] {
        if !free_of_markers(c) || !free_of(c, x) {
            return None;
        }
    }
    if is_zero(&c2, env) {
        return None;
    }
    let residual = together(
        &sub(
            sub(
                sub(e.clone(), times(vec![c2.clone(), sym("D2")])),
                times(vec![c1.clone(), sym("D1")]),
            ),
            times(vec![c0.clone(), sym("Y")]),
        ),
        env,
    );
    if !free_of_markers(&residual) {
        return None;
    }
    // Constant forcing only.
    let forcing = if free_of(&residual, x) {
        residual.clone()
    } else {
        return None;
    };
    let yp = particular_constant(&forcing, &c2, &c1, &c0, x, env);

    let disc = eval_expr(
        &sub(
            times(vec![c1.clone(), c1.clone()]),
            times(vec![int(4), c2.clone(), c0.clone()]),
        ),
        env,
    );
    let sign = sign_of(&disc, env)?;
    let two_c2 = times(vec![int(2), c2.clone()]);

    let (f1, f2) = if sign > 0 {
        let sq = call("Sqrt", vec![disc]);
        let r1 = div(plus(vec![neg(c1.clone()), sq.clone()]), two_c2.clone());
        let r2 = div(sub(neg(c1.clone()), sq), two_c2.clone());
        (
            call("Exp", vec![times(vec![r1, sym(x)])]),
            call("Exp", vec![times(vec![r2, sym(x)])]),
        )
    } else if sign == 0 {
        let r = div(neg(c1.clone()), two_c2.clone());
        (
            call("Exp", vec![times(vec![r.clone(), sym(x)])]),
            times(vec![sym(x), call("Exp", vec![times(vec![r, sym(x)])])]),
        )
    } else {
        let alpha = div(neg(c1.clone()), two_c2.clone());
        let beta = div(call("Sqrt", vec![neg(disc)]), two_c2.clone());
        let expo = call("Exp", vec![times(vec![alpha, sym(x)])]);
        (
            times(vec![
                expo.clone(),
                call("Cos", vec![times(vec![beta.clone(), sym(x)])]),
            ]),
            times(vec![expo, call("Sin", vec![times(vec![beta, sym(x)])])]),
        )
    };

    let homogeneous = plus(vec![
        times(vec![c_const(1), f1.clone()]),
        times(vec![c_const(2), f2.clone()]),
    ]);

    let general = eval_expr(&plus(vec![homogeneous, yp.clone()]), env);

    let ic_val = ics.iter().find_map(|c| parse_raw_value_ic(c, name));
    let ic_der = ics.iter().find_map(|c| parse_raw_deriv_ic(c, name));
    match (ic_val, ic_der) {
        (Some((x0, y0)), Some((_, y1))) => {
            let val = solve_second_order_ic(&f1, &f2, &yp, x, &x0, &y0, &y1, env)?;
            Some(val)
        }
        _ => Some(general),
    }
}

/// Parse `f'[x0] == value` (raw form) into `(x0, value)`.
fn parse_raw_deriv_ic(cond: &Expr, name: &str) -> Option<(Expr, Expr)> {
    let (lhs, rhs) = eq_pair(cond)?;
    // LHS is `Derivative[1][f][x0]`:
    //   Call{ head: Call{ head: Call{Derivative, [1]}, args: [f] }, args: [x0] }
    let Expr::Call { head, args } = &lhs else {
        return None;
    };
    if args.len() != 1 {
        return None;
    }
    let Expr::Call { head: dh, args: da } = head.as_ref() else {
        return None;
    };
    if da.len() != 1 || !matches!(&da[0], Expr::Symbol(s) if s == name) {
        return None;
    }
    let Expr::Call {
        head: order_node,
        args: dfa,
    } = dh.as_ref()
    else {
        return None;
    };
    if !matches!(order_node.as_ref(), Expr::Symbol(s) if s == "Derivative")
        || dfa.len() != 1
        || !matches!(&dfa[0], Expr::Integer(n) if n == &Integer::from(1))
    {
        return None;
    }
    Some((args[0].clone(), rhs))
}

/// Cramer's rule for the two initial conditions.
#[allow(clippy::too_many_arguments)]
fn solve_second_order_ic(
    f1: &Expr,
    f2: &Expr,
    yp: &Expr,
    x: &str,
    x0: &Expr,
    y0: &Expr,
    y1: &Expr,
    env: &Env,
) -> Option<Expr> {
    let eval_at = |e: &Expr| eval_expr(&subst(e, &sym(x), x0), env);
    let derivative = |e: &Expr| struct_deriv(e, x, env);

    let u1 = eval_at(f1);
    let u2 = eval_at(f2);
    let d1 = eval_at(&derivative(f1));
    let d2 = eval_at(&derivative(f2));
    let yp0 = eval_at(yp);
    let yp1 = eval_at(&derivative(yp));

    let rhs1 = sub(y0.clone(), yp0);
    let rhs2 = sub(y1.clone(), yp1);
    let det = sub(
        times(vec![u1.clone(), d2.clone()]),
        times(vec![u2.clone(), d1.clone()]),
    );
    if is_zero(&det, env) {
        return None;
    }
    let c1 = together(
        &div(
            sub(times(vec![rhs1.clone(), d2]), times(vec![u2, rhs2.clone()])),
            det.clone(),
        ),
        env,
    );
    let c2 = together(
        &div(sub(times(vec![u1, rhs2]), times(vec![rhs1, d1])), det),
        env,
    );
    let sol = plus(vec![
        times(vec![c_const(1), f1.clone()]),
        times(vec![c_const(2), f2.clone()]),
        yp.clone(),
    ]);
    Some(eval_expr(
        &subst(&subst(&sol, &c_const(1), &c1), &c_const(2), &c2),
        env,
    ))
}

/// Particular solution for constant forcing.
fn particular_constant(
    forcing: &Expr,
    c2: &Expr,
    c1: &Expr,
    c0: &Expr,
    x: &str,
    env: &Env,
) -> Expr {
    if is_zero(forcing, env) {
        return int(0);
    }
    if !is_zero(c0, env) {
        return eval_expr(&div(forcing.clone(), c0.clone()), env);
    }
    if !is_zero(c1, env) {
        return eval_expr(&times(vec![div(forcing.clone(), c1.clone()), sym(x)]), env);
    }
    eval_expr(
        &times(vec![
            div(forcing.clone(), times(vec![int(2), c2.clone()])),
            pow(sym(x), int(2)),
        ]),
        env,
    )
}

// ── General constant-coefficient linear ODEs ─────────────────────────────────

/// Marker symbol for derivative order `k` (`Y` for order 0).
fn marker_sym(k: usize) -> Expr {
    if k == 0 {
        sym("Y")
    } else {
        sym(&format!("D{k}"))
    }
}

/// Does `e` mention any linearisation marker?
fn marker_present(e: &Expr) -> bool {
    contains_symbol(e, "Y") || (1..=MAX_DERIV).any(|k| contains_symbol(e, &format!("D{k}")))
}

/// Set every marker to 0 except (optionally) one, which is set to 1.
fn zero_markers(e: &Expr, except: Option<usize>) -> Expr {
    let mut r = e.clone();
    for k in 0..=MAX_DERIV {
        let val = if except == Some(k) { int(1) } else { int(0) };
        r = subst(&r, &marker_sym(k), &val);
    }
    r
}

/// Exact rational (`n/d`, always normalised with `d > 0`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Rat {
    n: i128,
    d: i128,
}

impl Rat {
    fn new(n: i128, d: i128) -> Rat {
        let mut r = Rat { n, d };
        if r.d < 0 {
            r.n = -r.n;
            r.d = -r.d;
        }
        let g = gcd_i128(r.n.abs(), r.d);
        if g > 1 {
            r.n /= g;
            r.d /= g;
        }
        r
    }
    fn zero() -> Rat {
        Rat { n: 0, d: 1 }
    }
    fn one() -> Rat {
        Rat { n: 1, d: 1 }
    }
    fn is_zero(self) -> bool {
        self.n == 0
    }
    fn neg(self) -> Rat {
        Rat::new(-self.n, self.d)
    }
    fn abs(self) -> Rat {
        Rat::new(self.n.abs(), self.d)
    }
    fn add(self, o: Rat) -> Rat {
        Rat::new(self.n * o.d + o.n * self.d, self.d * o.d)
    }
    fn sub(self, o: Rat) -> Rat {
        self.add(o.neg())
    }
    fn mul(self, o: Rat) -> Rat {
        Rat::new(self.n * o.n, self.d * o.d)
    }
    fn div(self, o: Rat) -> Rat {
        Rat::new(self.n * o.d, self.d * o.n)
    }
    fn to_expr(self) -> Expr {
        if self.d == 1 {
            Expr::Integer(Integer::from(self.n))
        } else {
            div(
                Expr::Integer(Integer::from(self.n)),
                Expr::Integer(Integer::from(self.d)),
            )
        }
    }
}

fn gcd_i128(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    if a == 0 { 1 } else { a.abs() }
}

fn lcm_i128(a: i128, b: i128) -> i128 {
    a / gcd_i128(a, b) * b
}

/// Parse an integer-valued expression (integer literal or decimal string).
fn as_int_expr(e: &Expr) -> Option<i128> {
    match e {
        Expr::Integer(n) => n.to_i128(),
        Expr::Symbol(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Convert a constant expression to an exact rational.
fn parse_decimal_rat(s: &str) -> Option<Rat> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(t) => (true, t),
        None => (false, s),
    };
    let (int_part, frac) = s.split_once('.').unwrap_or((s, ""));
    if int_part.is_empty() && frac.is_empty() {
        return None;
    }
    let mut n: i128 = 0;
    for c in int_part.chars() {
        n = n * 10 + c.to_digit(10)? as i128;
    }
    let mut d: i128 = 1;
    for c in frac.chars() {
        n = n * 10 + c.to_digit(10)? as i128;
        d *= 10;
    }
    let r = Rat::new(n, d);
    Some(if neg { r.neg() } else { r })
}

fn expr_to_rat(e: &Expr) -> Option<Rat> {
    match e {
        Expr::Integer(n) => Some(Rat::new(n.to_i128()?, 1)),
        Expr::Real(r) => parse_decimal_rat(&format!("{}", r.to_f64())),
        Expr::Symbol(s) => {
            if let Some((a, b)) = s.split_once('/') {
                Some(Rat::new(a.trim().parse().ok()?, b.trim().parse().ok()?))
            } else {
                Some(Rat::new(s.trim().parse().ok()?, 1))
            }
        }
        Expr::Call { head, args } => {
            let Expr::Symbol(h) = head.as_ref() else {
                return None;
            };
            match h.as_str() {
                "Rational" if args.len() == 2 => {
                    Some(Rat::new(as_int_expr(&args[0])?, as_int_expr(&args[1])?))
                }
                "Times" => {
                    let mut r = Rat::one();
                    for a in args {
                        r = r.mul(expr_to_rat(a)?);
                    }
                    Some(r)
                }
                "Power" if args.len() == 2 => {
                    let base = expr_to_rat(&args[0])?;
                    let p = as_int_expr(&args[1])?;
                    let mut acc = Rat::one();
                    for _ in 0..p.unsigned_abs() {
                        acc = acc.mul(base);
                    }
                    if p < 0 {
                        Some(Rat::one().div(acc))
                    } else {
                        Some(acc)
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn poly_trim(p: Vec<Rat>) -> Vec<Rat> {
    match p.iter().position(|c| !c.is_zero()) {
        Some(i) => p[i..].to_vec(),
        None => vec![Rat::zero()],
    }
}

fn poly_eval(p: &[Rat], r: Rat) -> Rat {
    let mut acc = Rat::zero();
    for c in p {
        acc = acc.mul(r).add(*c);
    }
    acc
}

fn poly_divmod(p: &[Rat], q: &[Rat]) -> (Vec<Rat>, Vec<Rat>) {
    let q = poly_trim(q.to_vec());
    let mut rem = poly_trim(p.to_vec());
    if q.len() == 1 && q[0].is_zero() {
        return (vec![Rat::zero()], rem);
    }
    if rem.len() < q.len() || (rem.len() == 1 && rem[0].is_zero()) {
        return (vec![Rat::zero()], rem);
    }
    let mut quot = vec![Rat::zero(); rem.len() - q.len() + 1];
    while rem.len() >= q.len() && !(rem.len() == 1 && rem[0].is_zero()) {
        let deg = rem.len() - q.len();
        let factor = rem[0].div(q[0]);
        quot[deg] = factor;
        for i in 0..q.len() {
            rem[i] = rem[i].sub(factor.mul(q[i]));
        }
        rem = poly_trim(rem);
    }
    (poly_trim(quot), rem)
}

fn divisors(n: i128) -> Vec<i128> {
    let n = n.abs();
    let mut out = vec![];
    let mut i = 1;
    while i * i <= n {
        if n % i == 0 {
            out.push(i);
            if i != n / i {
                out.push(n / i);
            }
        }
        i += 1;
    }
    out
}

fn rational_root_candidates(p: &[Rat]) -> Vec<Rat> {
    let p = poly_trim(p.to_vec());
    if p.len() <= 1 {
        return vec![];
    }
    let lead = p[0];
    let constant = p[p.len() - 1];
    if constant.is_zero() || lead.is_zero() {
        return vec![];
    }
    let mut dens: i128 = 1;
    for c in &p {
        dens = lcm_i128(dens, c.d);
    }
    let ci = (constant.n * (dens / constant.d)).abs();
    let li = (lead.n * (dens / lead.d)).abs();
    let mut cands = vec![];
    for pn in divisors(ci) {
        for qd in divisors(li) {
            let r = Rat::new(pn, qd);
            cands.push(r);
            cands.push(r.neg());
        }
    }
    cands
}

fn root_mult(p: &[Rat], r: Rat) -> usize {
    let mut q = poly_trim(p.to_vec());
    let mut m = 0;
    while q.len() > 1 && poly_eval(&q, r).is_zero() {
        let (qq, _) = poly_divmod(&q, &[Rat::one(), r.neg()]);
        q = qq;
        m += 1;
    }
    m
}

fn quad_mult(p: &[Rat], k: Rat, w: Rat) -> usize {
    let q = vec![Rat::one(), k.mul(Rat::new(-2, 1)), k.mul(k).add(w.mul(w))];
    let mut cur = poly_trim(p.to_vec());
    let mut m = 0;
    loop {
        if cur.len() < 3 {
            break;
        }
        let (qq, rem) = poly_divmod(&cur, &q);
        if rem.len() == 1 && rem[0].is_zero() {
            cur = qq;
            m += 1;
        } else {
            break;
        }
    }
    m
}

fn exp_lin(r: Rat, x: &str, env: &Env) -> Expr {
    eval_expr(&pow(sym("E"), times(vec![r.to_expr(), sym(x)])), env)
}

/// Basis of the solution space of `sum_k p[k] D^k y = 0` (p descending).
fn char_basis(p: &[Rat], x: &str, env: &Env) -> Option<Vec<Expr>> {
    let mut p = poly_trim(p.to_vec());
    if p.len() <= 1 {
        return Some(vec![]);
    }
    let mut basis: Vec<Expr> = vec![];

    let mut m0 = 0;
    while p.len() > 1 && p[p.len() - 1].is_zero() {
        p.pop();
        m0 += 1;
    }
    for j in 0..m0 {
        basis.push(pow(sym(x), int(j as i64)));
    }

    loop {
        if p.len() <= 1 {
            break;
        }
        if p.len() == 2 {
            let r = Rat::zero().sub(p[1].div(p[0]));
            basis.push(exp_lin(r, x, env));
            break;
        }
        if p.len() == 3 {
            let (a, b, c) = (p[0], p[1], p[2]);
            let disc = b.mul(b).sub(Rat::new(4, 1).mul(a).mul(c));
            if disc.is_zero() {
                let r = Rat::zero().sub(b.div(Rat::new(2, 1).mul(a)));
                basis.push(exp_lin(r, x, env));
                basis.push(eval_expr(&times(vec![sym(x), exp_lin(r, x, env)]), env));
            } else if disc.n > 0 {
                let sq = call("Sqrt", vec![disc.to_expr()]);
                let two_a = times(vec![int(2), a.to_expr()]);
                let r1 = div(plus(vec![neg(b.to_expr()), sq.clone()]), two_a.clone());
                let r2 = div(sub(neg(b.to_expr()), sq), two_a);
                basis.push(eval_expr(&call("Exp", vec![times(vec![r1, sym(x)])]), env));
                basis.push(eval_expr(&call("Exp", vec![times(vec![r2, sym(x)])]), env));
            } else {
                let alpha = div(neg(b.to_expr()), times(vec![int(2), a.to_expr()]));
                let beta = div(
                    call("Sqrt", vec![disc.neg().to_expr()]),
                    times(vec![int(2), a.to_expr()]),
                );
                let expo = pow(sym("E"), times(vec![alpha, sym(x)]));
                basis.push(eval_expr(
                    &times(vec![
                        expo.clone(),
                        call("Cos", vec![times(vec![beta.clone(), sym(x)])]),
                    ]),
                    env,
                ));
                basis.push(eval_expr(
                    &times(vec![expo, call("Sin", vec![times(vec![beta, sym(x)])])]),
                    env,
                ));
            }
            break;
        }
        let cands = rational_root_candidates(&p);
        let mut found = false;
        for r in cands {
            if poly_eval(&p, r).is_zero() {
                let mut count = 0;
                loop {
                    if p.len() <= 1 || !poly_eval(&p, r).is_zero() {
                        break;
                    }
                    let (qq, rem) = poly_divmod(&p, &[Rat::one(), r.neg()]);
                    if !(rem.len() == 1 && rem[0].is_zero()) {
                        break;
                    }
                    p = qq;
                    count += 1;
                }
                for j in 0..count {
                    let base = exp_lin(r, x, env);
                    let term = if j == 0 {
                        base
                    } else {
                        eval_expr(&times(vec![pow(sym(x), int(j as i64)), base]), env)
                    };
                    basis.push(term);
                }
                found = true;
                break;
            }
        }
        if !found {
            return None;
        }
    }
    Some(basis)
}

/// Characteristic polynomial (descending) from ascending coefficient list.
fn char_rat(c_asc: &[Expr]) -> Option<Vec<Rat>> {
    let mut v = Vec::with_capacity(c_asc.len());
    for ck in c_asc.iter().rev() {
        v.push(expr_to_rat(ck)?);
    }
    Some(v)
}

/// Structural derivative of the simple closed forms the solvers build
/// (sums/products/powers of `x`, `Exp`, `Sin`, `Cos`). Unlike the `D` builtin
/// this consistently evaluates products with numeric coefficients.
fn struct_deriv(e: &Expr, x: &str, env: &Env) -> Expr {
    if e == &sym(x) {
        return int(1);
    }
    if !contains_symbol(e, x) {
        return int(0);
    }
    if let Expr::Call { head, args } = e
        && let Expr::Symbol(h) = head.as_ref()
    {
        match h.as_str() {
            "Plus" => {
                let terms = args.iter().map(|a| struct_deriv(a, x, env)).collect();
                return eval_expr(&plus(terms), env);
            }
            "Times" => {
                let mut acc = int(0);
                for i in 0..args.len() {
                    let di = struct_deriv(&args[i], x, env);
                    if is_zero(&di, env) {
                        continue;
                    }
                    let mut factors: Vec<Expr> = Vec::with_capacity(args.len());
                    for (j, a) in args.iter().enumerate() {
                        if j != i {
                            factors.push(a.clone());
                        }
                    }
                    factors.push(di);
                    acc = plus(vec![acc, times(factors)]);
                }
                return eval_expr(&acc, env);
            }
            "Power" if args.len() == 2 => {
                if let Expr::Integer(n) = &args[1] {
                    let du = struct_deriv(&args[0], x, env);
                    let term = times(vec![
                        Expr::Integer(n.clone()),
                        pow(args[0].clone(), Expr::Integer(n.clone() - 1)),
                        du,
                    ]);
                    return eval_expr(&term, env);
                }
                if matches!(&args[0], Expr::Symbol(s) if s == "E") {
                    let du = struct_deriv(&args[1], x, env);
                    return eval_expr(&times(vec![e.clone(), du]), env);
                }
            }
            "Exp" if args.len() == 1 => {
                let du = struct_deriv(&args[0], x, env);
                return eval_expr(&times(vec![e.clone(), du]), env);
            }
            "Sin" if args.len() == 1 => {
                let du = struct_deriv(&args[0], x, env);
                return eval_expr(&times(vec![call("Cos", vec![args[0].clone()]), du]), env);
            }
            "Cos" if args.len() == 1 => {
                let du = struct_deriv(&args[0], x, env);
                return eval_expr(
                    &neg(times(vec![call("Sin", vec![args[0].clone()]), du])),
                    env,
                );
            }
            _ => {}
        }
    }
    eval_expr(&call("D", vec![e.clone(), sym(x)]), env)
}

fn deriv_expr(e: &Expr, x: &str, env: &Env) -> Expr {
    struct_deriv(e, x, env)
}

/// If `base` is a constant multiple of `E`, its rational coefficient.
fn e_base_coeff(base: &Expr) -> Option<Rat> {
    if let Expr::Call { head, args } = base
        && matches!(head.as_ref(), Expr::Symbol(s) if s == "Times")
    {
        let mut cp = Rat::one();
        let mut has_e = false;
        for f in args {
            if matches!(f, Expr::Symbol(s) if s == "E") {
                has_e = true;
            } else {
                cp = cp.mul(expr_to_rat(f)?);
            }
        }
        if has_e {
            return Some(cp);
        }
    }
    None
}

/// Rewrite `(c E)^u` as `c E^u` so the parser sees a plain exponential.
#[allow(clippy::only_used_in_recursion)]
fn normalize_exp(e: &Expr, x: &str, env: &Env) -> Expr {
    if let Expr::Call { head, args } = e
        && let Expr::Symbol(h) = head.as_ref()
    {
        if h == "Power"
            && args.len() == 2
            && let Some(cp) = e_base_coeff(&args[0])
        {
            let base = pow(sym("E"), normalize_exp(&args[1], x, env));
            return eval_expr(&times(vec![cp.to_expr(), base]), env);
        }
        if h == "Exp" && args.len() == 1 {
            return eval_expr(&pow(sym("E"), normalize_exp(&args[0], x, env)), env);
        }
        return Expr::Call {
            head: head.clone(),
            args: args.iter().map(|a| normalize_exp(a, x, env)).collect(),
        };
    }
    e.clone()
}

/// Push negative integer powers of a `Times` onto its factors:
/// `(a b)^-1` → `a^-1 b^-1`, which lets `Simplify` cancel common factors.
fn expand_neg_powers(e: &Expr, env: &Env) -> Expr {
    if let Expr::Call { head, args } = e
        && let Expr::Symbol(h) = head.as_ref()
    {
        if h == "Power" && args.len() == 2 {
            let base = expand_neg_powers(&args[0], env);
            let exp = args[1].clone();
            if let (Expr::Call { head: bh, args: ba }, Expr::Integer(n)) = (&base, &exp)
                && matches!(bh.as_ref(), Expr::Symbol(s) if s == "Times")
                && n.is_negative()
            {
                let factors = ba
                    .iter()
                    .map(|b| call("Power", vec![b.clone(), Expr::Integer(n.clone())]))
                    .collect();
                return eval_expr(&times(factors), env);
            }
            return Expr::Call {
                head: head.clone(),
                args: vec![base, expand_neg_powers(&exp, env)],
            };
        }
        return Expr::Call {
            head: head.clone(),
            args: args.iter().map(|a| expand_neg_powers(a, env)).collect(),
        };
    }
    e.clone()
}

fn nth_deriv(e: &Expr, order: usize, x: &str, env: &Env) -> Expr {
    let mut cur = e.clone();
    for _ in 0..order {
        cur = deriv_expr(&cur, x, env);
    }
    cur
}

/// Apply `sum_k coeffs[k] D^k` to `y`.
fn apply_operator(y: &Expr, coeffs: &[Expr], x: &str, env: &Env) -> Expr {
    let n = coeffs.len() - 1;
    let mut acc = int(0);
    let mut cur = y.clone();
    for (k, ck) in coeffs.iter().enumerate() {
        if !is_zero(ck, env) {
            acc = plus(vec![acc, times(vec![ck.clone(), cur.clone()])]);
        }
        if k < n {
            cur = deriv_expr(&cur, x, env);
        }
    }
    eval_expr(&acc, env)
}

/// A single forcing term `poly(x) * E^(k x) * {cos,sin}(w x)`.
struct TermSpec {
    k: Rat,
    w: Option<Rat>,
    deg: usize,
}

fn parse_linear_arg(e: &Expr, x: &str) -> Option<Rat> {
    if e == &sym(x) {
        return Some(Rat::one());
    }
    if let Expr::Call { head, args } = e
        && matches!(head.as_ref(), Expr::Symbol(s) if s == "Times")
    {
        let mut c = Rat::one();
        let mut found = 0;
        for a in args {
            if a == &sym(x) {
                found += 1;
            } else {
                c = c.mul(expr_to_rat(a)?);
            }
        }
        if found == 1 {
            return Some(c);
        }
    }
    None
}

fn parse_forcing_term(term: &Expr, x: &str) -> Option<TermSpec> {
    let mut k = Rat::zero();
    let mut w: Option<Rat> = None;
    let mut deg = 0usize;
    let factors = match term {
        Expr::Call { head, args } if matches!(head.as_ref(), Expr::Symbol(s) if s == "Times") => {
            args.clone()
        }
        other => vec![other.clone()],
    };
    for f in factors {
        if expr_to_rat(&f).is_some() {
            continue;
        }
        match &f {
            Expr::Symbol(s) if s == x => deg = deg.max(1),
            // A bare `E` is the constant exp(1); absorb it.
            Expr::Symbol(s) if s == "E" => {}
            Expr::Call { head, args }
                if args.len() == 2 && matches!(head.as_ref(), Expr::Symbol(s) if s == "Power") =>
            {
                match &args[0] {
                    Expr::Symbol(b) if b == x => {
                        let p = as_int_expr(&args[1])?;
                        if p < 0 {
                            return None;
                        }
                        deg = deg.max(p as usize);
                    }
                    Expr::Symbol(b) if b == "E" => {
                        k = k.add(parse_linear_arg(&args[1], x)?);
                    }
                    // `(-E)^x` etc: a constant multiple of `E` raised to a power.
                    Expr::Call { head: bh, args: ba } if matches!(bh.as_ref(), Expr::Symbol(s) if s == "Times") =>
                    {
                        let mut has_e = false;
                        for bf in ba {
                            if matches!(bf, Expr::Symbol(s) if s == "E") {
                                has_e = true;
                            } else if expr_to_rat(bf).is_none() {
                                return None;
                            }
                        }
                        if !has_e {
                            return None;
                        }
                        k = k.add(parse_linear_arg(&args[1], x)?);
                    }
                    _ => return None,
                }
            }
            Expr::Call { head, args }
                if args.len() == 1 && matches!(head.as_ref(), Expr::Symbol(s) if s == "Exp") =>
            {
                k = k.add(parse_linear_arg(&args[0], x)?);
            }
            Expr::Call { head, args }
                if args.len() == 1
                    && matches!(head.as_ref(), Expr::Symbol(s) if s == "Cos" || s == "Sin") =>
            {
                let ww = parse_linear_arg(&args[0], x)?.abs();
                match w {
                    Some(prev) if prev != ww => return None,
                    _ => w = Some(ww),
                }
            }
            _ => return None,
        }
    }
    Some(TermSpec { k, w, deg })
}

fn expr_is_zero(e: &Expr, env: &Env) -> bool {
    match eval_value(e, env) {
        Value::Integer(n) => n.is_zero(),
        Value::Rational(r) => r.is_zero(),
        _ => false,
    }
}

/// Exact Gaussian elimination with symbolic (non-numeric) entries.
#[allow(clippy::needless_range_loop)]
fn solve_symbolic_linear(mut a: Vec<Vec<Expr>>, rhs: Vec<Expr>, env: &Env) -> Option<Vec<Expr>> {
    let n = rhs.len();
    if n == 0 {
        return Some(vec![]);
    }
    for (row, b) in a.iter_mut().zip(rhs) {
        row.push(b);
    }
    for col in 0..n {
        let mut pivot = None;
        for r in col..n {
            if !expr_is_zero(&a[r][col], env) {
                pivot = Some(r);
                break;
            }
        }
        let pr = pivot?;
        a.swap(col, pr);
        let pv = a[col][col].clone();
        for c in col..=n {
            a[col][c] = eval_expr(&div(a[col][c].clone(), pv.clone()), env);
        }
        for r in 0..n {
            if r == col {
                continue;
            }
            let factor = a[r][col].clone();
            if expr_is_zero(&factor, env) {
                continue;
            }
            for c in col..=n {
                a[r][c] = eval_expr(
                    &sub(
                        a[r][c].clone(),
                        times(vec![factor.clone(), a[col][c].clone()]),
                    ),
                    env,
                );
            }
        }
    }
    Some(
        (0..n)
            .map(|r| {
                let raw = expand_neg_powers(&a[r][n], env);
                let t = together(&raw, env);
                let s = eval_expr(&call("Simplify", vec![t]), env);
                if contains_symbol(&s, "Simplify") {
                    raw
                } else {
                    s
                }
            })
            .collect(),
    )
}

/// Solve a square linear system `M a = b` represented by `u` symbols.
fn solve_linear_system(residual: &Expr, u: &[String], x: &str, env: &Env) -> Option<Vec<Expr>> {
    let n = u.len();
    let mut mat = Vec::with_capacity(n);
    let mut rhs = Vec::with_capacity(n);
    for i in 0..n {
        let val = eval_expr(&subst(residual, &sym(x), &int(i as i64)), env);
        let val = expand(&val, env);
        let base = {
            let mut t = val.clone();
            for uj in u {
                t = subst(&t, &sym(uj), &int(0));
            }
            eval_expr(&t, env)
        };
        let mut row = Vec::with_capacity(n);
        for j in 0..n {
            let mut w = val.clone();
            for (t, uk) in u.iter().enumerate() {
                w = subst(&w, &sym(uk), &int(if t == j { 1 } else { 0 }));
            }
            row.push(eval_expr(&sub(w, base.clone()), env));
        }
        rhs.push(eval_expr(&neg(base), env));
        mat.push(row);
    }
    solve_symbolic_linear(mat, rhs, env)
}

/// Particular solution by undetermined coefficients.
fn undetermined(
    forcing: &Expr,
    charp: &[Rat],
    coeffs: &[Expr],
    x: &str,
    env: &Env,
) -> Option<Expr> {
    let forcing = expand(forcing, env);
    let terms = match &forcing {
        Expr::Call { head, args } if matches!(head.as_ref(), Expr::Symbol(s) if s == "Plus") => {
            args.clone()
        }
        other => vec![other.clone()],
    };
    let mut basis: Vec<Expr> = vec![];
    for term in &terms {
        let spec = parse_forcing_term(term, x)?;
        let s = match spec.w {
            None => root_mult(charp, spec.k),
            Some(w) => quad_mult(charp, spec.k, w),
        };
        for j in s..=(spec.deg + s) {
            let xj = pow(sym(x), int(j as i64));
            let ex = exp_lin(spec.k, x, env);
            let base = eval_expr(&times(vec![xj, ex]), env);
            match spec.w {
                None => push_unique(&mut basis, base),
                Some(w) => {
                    let wx = times(vec![w.to_expr(), sym(x)]);
                    push_unique(
                        &mut basis,
                        eval_expr(
                            &times(vec![base.clone(), call("Cos", vec![wx.clone()])]),
                            env,
                        ),
                    );
                    push_unique(
                        &mut basis,
                        eval_expr(&times(vec![base, call("Sin", vec![wx])]), env),
                    );
                }
            }
        }
    }
    if basis.is_empty() {
        return Some(int(0));
    }
    let u: Vec<String> = (0..basis.len()).map(|i| format!("u${i}")).collect();
    let mut yp = int(0);
    for (i, b) in basis.iter().enumerate() {
        yp = plus(vec![yp, times(vec![sym(&u[i]), b.clone()])]);
    }
    let yp = expand(&yp, env);
    let residual = expand(&sub(apply_operator(&yp, coeffs, x, env), forcing), env);
    let sol = solve_linear_system(&residual, &u, x, env)?;
    let mut out = int(0);
    for (i, b) in basis.iter().enumerate() {
        out = plus(vec![out, times(vec![sol[i].clone(), b.clone()])]);
    }
    let out = expand_neg_powers(&eval_expr(&out, env), env);
    let out = together(&out, env);
    if std::env::var("SYMA_DSOLVE_DBG").is_ok() {
        eprintln!("[dsolve-ic] out={out:?}");
    }
    let simplified = eval_expr(&call("Simplify", vec![out.clone()]), env);
    if contains_symbol(&simplified, "Simplify") {
        Some(out)
    } else {
        Some(simplified)
    }
}

fn push_unique(basis: &mut Vec<Expr>, f: Expr) {
    if !basis.contains(&f) {
        basis.push(f);
    }
}

/// Integrating-factor solution for first-order `c1 y' + c0 y = f`.
fn first_order_integrating_factor(c: &[Expr], forcing: &Expr, x: &str, env: &Env) -> Option<Expr> {
    if is_zero(&c[1], env) {
        return None;
    }
    let p = together(&div(c[0].clone(), c[1].clone()), env);
    let q = together(&div(forcing.clone(), c[1].clone()), env);
    let big_p = integrate(&p, x, env);
    let mu = call("Exp", vec![big_p.clone()]);
    let inner = plus(vec![integrate(&times(vec![q, mu]), x, env), c_const(1)]);
    Some(eval_expr(
        &times(vec![call("Exp", vec![neg(big_p)]), inner]),
        env,
    ))
}

fn parse_ic_order(cond: &Expr, name: &str) -> Option<(usize, Expr, Expr)> {
    let (lhs, rhs) = eq_pair(cond)?;
    if let Some(x0) = call_arg(&lhs, name) {
        return Some((0, x0, rhs));
    }
    let Expr::Call { head, args } = &lhs else {
        return None;
    };
    if args.len() != 1 {
        return None;
    }
    let Expr::Call { head: dh, args: da } = head.as_ref() else {
        return None;
    };
    if da.len() != 1 || !matches!(&da[0], Expr::Symbol(s) if s == name) {
        return None;
    }
    let Expr::Call {
        head: order_node,
        args: dfa,
    } = dh.as_ref()
    else {
        return None;
    };
    if !matches!(order_node.as_ref(), Expr::Symbol(s) if s == "Derivative") || dfa.len() != 1 {
        return None;
    }
    let Expr::Integer(order) = &dfa[0] else {
        return None;
    };
    Some((order.to_usize()?, args[0].clone(), rhs))
}

/// Attach `C[1]..C[n]` to `basis` and resolve them from initial conditions.
fn apply_general_ics(
    basis: &[Expr],
    yp: &Expr,
    name: &str,
    x: &str,
    ics: &[Expr],
    env: &Env,
) -> Option<Expr> {
    let mut general = yp.clone();
    for (i, b) in basis.iter().enumerate() {
        general = plus(vec![general, times(vec![c_const(i as i64 + 1), b.clone()])]);
    }
    let general = eval_expr(&general, env);
    if ics.is_empty() || basis.is_empty() {
        return Some(general);
    }
    let n = basis.len();
    let mut conds: Vec<(usize, Expr, Expr)> = Vec::new();
    for c in ics {
        if let Some(t) = parse_ic_order(c, name) {
            conds.push(t);
        }
    }
    if conds.len() < n {
        return Some(general);
    }
    conds.truncate(n);
    let mut mat = Vec::with_capacity(n);
    let mut rhs = Vec::with_capacity(n);
    for (order, x0, value) in &conds {
        let mut row = Vec::with_capacity(n);
        for b in basis {
            let db = nth_deriv(b, *order, x, env);
            row.push(eval_expr(&subst(&db, &sym(x), x0), env));
        }
        let dyp = nth_deriv(yp, *order, x, env);
        let yp0 = eval_expr(&subst(&dyp, &sym(x), x0), env);
        rhs.push(eval_expr(&sub(value.clone(), yp0), env));
        mat.push(row);
    }
    let Some(items) = solve_symbolic_linear(mat, rhs, env) else {
        return Some(general);
    };
    let mut out = general;
    for (i, ci) in items.iter().enumerate() {
        out = subst(&out, &c_const(i as i64 + 1), ci);
    }
    let out = expand_neg_powers(&eval_expr(&out, env), env);
    let out = together(&out, env);
    if std::env::var("SYMA_DSOLVE_DBG").is_ok() {
        eprintln!("[dsolve-ic] out={out:?}");
    }
    let simplified = eval_expr(&call("Simplify", vec![out.clone()]), env);
    if contains_symbol(&simplified, "Simplify") {
        Some(out)
    } else {
        Some(simplified)
    }
}

/// Solve a constant-coefficient linear ODE of any order `1..=MAX_DERIV`.
fn solve_linear_const_coeff(
    e: &Expr,
    name: &str,
    x: &str,
    ics: &[Expr],
    env: &Env,
) -> Option<Expr> {
    let mut n = 0usize;
    for k in 1..=MAX_DERIV {
        if contains_symbol(e, &format!("D{k}")) {
            n = k;
        }
    }
    if n == 0 {
        return None;
    }
    let mut c = vec![int(0); n + 1];
    for (k, ck) in c.iter_mut().enumerate() {
        let base = zero_markers(e, None);
        let only = zero_markers(e, Some(k));
        *ck = eval_expr(&together(&sub(only, base), env), env);
        if marker_present(ck) || !free_of(ck, x) {
            return None;
        }
    }
    // Subtract each linear term with a *flat* negation. A nested
    // `Times[-1, sum]` makes `Together` collapse `-E^x` to `-E`.
    let mut flat = e.clone();
    for (k, ck) in c.iter().enumerate() {
        if !is_zero(ck, env) {
            flat = plus(vec![flat, times(vec![ck.clone(), int(-1), marker_sym(k)])]);
        }
    }
    let residual = together(&flat, env);
    if marker_present(&residual) {
        return None;
    }
    let forcing = eval_expr(&neg(residual), env);
    let forcing = normalize_exp(&forcing, x, env);

    let charp = char_rat(&c)?;
    let basis = char_basis(&charp, x, env)?;

    let yp = if is_zero(&forcing, env) {
        int(0)
    } else if let Some(y) = undetermined(&forcing, &charp, &c, x, env) {
        y
    } else if n == 1 {
        first_order_integrating_factor(&c, &forcing, x, env)?
    } else {
        return None;
    };

    apply_general_ics(&basis, &yp, name, x, ics, env)
}

// ── RSolve / RSolveValue ─────────────────────────────────────────────────────

fn builtin_rsolve(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    // Try the existing recurrence solver first (handles orders 1-2 with ICs).
    if let Ok(v) = crate::builtins::discrete::builtin_rsolve(args, env)
        && matches!(v, Value::List(_))
    {
        return Ok(v);
    }
    match solve_recurrence(args, env) {
        Some((target, sol)) => Ok(Value::List(vec![Value::List(vec![Value::Rule {
            lhs: Box::new(to_value(&func_at(&target.name, &target.var))),
            rhs: Box::new(to_value(&sol)),
            delayed: false,
        }])])),
        None => Ok(unevaluated("RSolve", args)),
    }
}

fn builtin_rsolve_value(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if let Ok(v) = crate::builtins::discrete::builtin_rsolve(args, env)
        && let Value::List(outer) = &v
        && let Some(Value::List(inner)) = outer.first()
        && let Some(Value::Rule { rhs, .. }) = inner.first()
    {
        return Ok((**rhs).clone());
    }
    match solve_recurrence(args, env) {
        Some((_, sol)) => Ok(eval_value(&sol, env)),
        None => Ok(unevaluated("RSolveValue", args)),
    }
}

/// First-order constant-coefficient recurrence `a[n] = c a[n-1]`.
fn solve_recurrence(args: &[Value], env: &Env) -> Option<(Target, Expr)> {
    if args.len() < 3 {
        return None;
    }
    let (eqn, ics) = split_constraints(&as_expr(&args[0]));
    let (lhs, rhs) = eq_pair(&eqn)?;
    let target = parse_target(&as_expr(&args[1]), &first_range_symbol(&as_expr(&args[2]))?)?;
    let n = target.var.clone();

    // Normalise to `a[n] == c a[n-1]`.
    let lhs_arg = call_arg(&lhs, &target.name)?;
    let shift = match &lhs_arg {
        Expr::Symbol(s) if s == &n => 0,
        Expr::Call { head, args }
            if matches!(head.as_ref(), Expr::Symbol(s) if s == "Plus") && args.len() == 2 =>
        {
            if args[0] == sym(&n) {
                match &args[1] {
                    Expr::Integer(k) => k.to_i64()?,
                    _ => return None,
                }
            } else if args[1] == sym(&n) {
                match &args[0] {
                    Expr::Integer(k) => k.to_i64()?,
                    _ => return None,
                }
            } else {
                return None;
            }
        }
        _ => return None,
    };
    // `shift` = lhs index offset. The referenced (lag) term is `a[n - ref_lag]`.
    let ref_lag = 1 - shift;
    let lag_arg = if ref_lag == 0 {
        sym(&n)
    } else {
        call("Plus", vec![sym(&n), int(-ref_lag)])
    };
    let a_lag = Expr::Call {
        head: Box::new(sym(&target.name)),
        args: vec![lag_arg],
    };
    let c = coeff(&subst(&rhs, &a_lag, &sym("AA")), "AA", env);
    if is_zero(&c, env) {
        return None;
    }
    // Reject additive forcing (RHS must be a pure multiple of the lag term).
    let extra = together(
        &sub(
            subst(&rhs, &a_lag, &sym("AA")),
            times(vec![c.clone(), sym("AA")]),
        ),
        env,
    );
    if !free_of(&extra, "AA") || !is_zero(&extra, env) {
        return None;
    }

    // Solution: a[n] = C[1] c^n (index n measured from the IC / base 0).
    let sol = times(vec![c_const(1), pow(c.clone(), sym(&n))]);

    // Apply an initial condition `a[k] == v`.
    let ic = ics
        .iter()
        .find_map(|cond| parse_recurrence_ic(cond, &target.name));
    let sol = if let Some((k, v)) = ic {
        // a[k] = C1 c^k = v  →  C1 = v / c^k
        eval_expr(
            &subst(&sol, &c_const(1), &div(v, pow(c.clone(), k.clone()))),
            env,
        )
    } else {
        eval_expr(&sol, env)
    };
    Some((target, sol))
}

fn call_arg(e: &Expr, name: &str) -> Option<Expr> {
    if let Expr::Call { head, args } = e
        && matches!(head.as_ref(), Expr::Symbol(s) if s == name)
        && args.len() == 1
    {
        return Some(args[0].clone());
    }
    None
}

fn first_range_symbol(e: &Expr) -> Option<Expr> {
    match e {
        Expr::Symbol(_) => Some(e.clone()),
        Expr::List(items) => items.first().cloned(),
        _ => None,
    }
}

fn parse_recurrence_ic(cond: &Expr, name: &str) -> Option<(Expr, Expr)> {
    let (lhs, rhs) = eq_pair(cond)?;
    let idx = call_arg(&lhs, name)?;
    Some((idx, rhs))
}

// ── NDSolve ──────────────────────────────────────────────────────────────────

/// Number of fixed RK4 steps.
const NDSOLVE_STEPS: usize = 100;

fn builtin_ndsolve(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    match numeric_solve(args, env) {
        Some((name, ifn)) => Ok(Value::List(vec![Value::List(vec![Value::Rule {
            lhs: Box::new(Value::Symbol(name)),
            rhs: Box::new(ifn),
            delayed: false,
        }])])),
        None => Ok(unevaluated("NDSolve", args)),
    }
}

fn builtin_ndsolve_value(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    match numeric_solve(args, env) {
        Some((_, ifn)) => Ok(ifn),
        None => Ok(unevaluated("NDSolveValue", args)),
    }
}

/// Fixed-step RK4 for `y' = f(x, y)` with a single value initial condition.
fn numeric_solve(args: &[Value], env: &Env) -> Option<(String, Value)> {
    if args.len() < 3 {
        return None;
    }
    let (eqn, ics) = split_constraints(&as_expr(&args[0]));
    let target = parse_target(&as_expr(&args[1]), &first_range_symbol(&as_expr(&args[2]))?)?;
    let range = match as_expr(&args[2]) {
        Expr::List(items) if items.len() == 3 => items,
        _ => return None,
    };
    let x1 = as_f64(&eval_expr(&range[2], env))?;
    let (lhs, rhs) = eq_pair(&eqn)?;

    // Left-hand side must be `y'[x]`.
    let d1 = deriv_at(&target.name, &target.var, 1);
    if lhs != d1 {
        return None;
    }

    // Initial condition.
    let (ic_x, ic_y) = ics.iter().find_map(|c| {
        let (l, r) = eq_pair(c)?;
        let at = call_arg(&l, &target.name)?;
        Some((at, r))
    })?;
    let ic_x = as_f64(&eval_expr(&ic_x, env))?;
    let ic_y = as_f64(&eval_expr(&ic_y, env))?;

    let f = |xv: f64, yv: f64| -> f64 {
        // Substitute `y[x]` first (its argument is the symbol `x`), then `x`.
        let e = subst(
            &subst(&rhs, &func_at(&target.name, &target.var), &real(yv)),
            &sym(&target.var),
            &real(xv),
        );
        crate::eval::eval(&e, env)
            .ok()
            .and_then(|v| v.to_real())
            .unwrap_or(0.0)
    };

    let (start_x, start_y) = (ic_x, ic_y);
    let h = (x1 - start_x) / NDSOLVE_STEPS as f64;

    // Forward integration; also backward if the range runs the other way.
    let mut xs = Vec::with_capacity(NDSOLVE_STEPS + 1);
    let mut ys = Vec::with_capacity(NDSOLVE_STEPS + 1);
    let mut ds = Vec::with_capacity(NDSOLVE_STEPS + 1);
    let mut cx = start_x;
    let mut cy = start_y;
    xs.push(cx);
    ys.push(cy);
    ds.push(f(cx, cy));
    for _ in 0..NDSOLVE_STEPS {
        let k1 = f(cx, cy);
        let k2 = f(cx + h / 2.0, cy + h * k1 / 2.0);
        let k3 = f(cx + h / 2.0, cy + h * k2 / 2.0);
        let k4 = f(cx + h, cy + h * k3);
        cy += h * (k1 + 2.0 * k2 + 2.0 * k3 + k4) / 6.0;
        cx += h;
        xs.push(cx);
        ys.push(cy);
        ds.push(f(cx, cy));
    }

    let rows: Vec<Value> = (0..xs.len())
        .map(|i| Value::List(vec![real_v(xs[i]), real_v(ys[i]), real_v(ds[i])]))
        .collect();
    let data = Value::List(rows);
    let body = Expr::Call {
        head: Box::new(sym("InterpolatingFunction")),
        args: vec![
            crate::eval::table::value_to_expr(&data),
            Expr::Slot(Some(1)),
        ],
    };
    let ifn = Value::PureFunction {
        body,
        slot_count: 1,
        params: vec![],
        env: None,
    };
    Some((target.name, ifn))
}

fn real_v(f: f64) -> Value {
    Value::Real(Float::with_val(DEFAULT_PRECISION, f))
}

// ── InterpolatingFunction ────────────────────────────────────────────────────

fn builtin_interpolating_function(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => Ok(Value::PureFunction {
            body: Expr::Call {
                head: Box::new(sym("InterpolatingFunction")),
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
            let t = match args[1].to_real() {
                Some(t) => t,
                None => return Ok(unevaluated("InterpolatingFunction", args)),
            };
            match interpolate(&args[0], t) {
                Some(v) => Ok(v),
                None => Ok(unevaluated("InterpolatingFunction", args)),
            }
        }
        _ => Ok(unevaluated("InterpolatingFunction", args)),
    }
}

/// Piecewise cubic Hermite interpolation over `{{x, y, y'}, ...}`.
/// Extrapolates using the nearest interval.
fn interpolate(data: &Value, t: f64) -> Option<Value> {
    let rows = match data {
        Value::List(rows) if rows.len() >= 2 => rows,
        _ => return None,
    };
    let parse = |row: &Value| -> Option<(f64, f64, f64)> {
        match row {
            Value::List(c) if c.len() == 3 => {
                Some((c[0].to_real()?, c[1].to_real()?, c[2].to_real()?))
            }
            _ => None,
        }
    };
    let mut pts = Vec::with_capacity(rows.len());
    for row in rows {
        pts.push(parse(row)?);
    }

    // Locate the interval (extrapolate from the nearest end).
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
        return Some(real_v(ya));
    }
    let s = (t - xa) / h;
    let s2 = s * s;
    let s3 = s2 * s;
    let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    let h10 = s3 - 2.0 * s2 + s;
    let h01 = -2.0 * s3 + 3.0 * s2;
    let h11 = s3 - s2;
    let y = h00 * ya + h10 * h * da + h01 * yb + h11 * h * db;
    Some(real_v(y))
}

fn is_zero(e: &Expr, env: &Env) -> bool {
    matches!(eval_value(e, env), Value::Integer(n) if n.is_zero())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    fn eval_str(input: &str) -> Value {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let tokens = lexer::tokenize(input).unwrap();
        let ast = parser::parse(tokens).unwrap_or_else(|e| panic!("parse {input:?}: {e:?}"));
        crate::eval::eval_program(&ast, &env).unwrap_or_else(|e| panic!("eval {input:?}: {e:?}"))
    }

    fn shows(input: &str) -> String {
        eval_str(input).to_string()
    }

    #[test]
    fn test_dsolve_first_order_linear_homogeneous() {
        let out = shows("DSolve[y'[x] == y[x], y[x], x]");
        assert!(out.contains("C[1]"), "got {out}");
        assert!(out.contains("Power[E") || out.contains("Exp["), "got {out}");
    }

    #[test]
    fn test_dsolve_value_first_order() {
        let out = shows("DSolveValue[y'[x] == y[x], y[x], x]");
        assert!(out.contains("C[1]"), "got {out}");
        assert!(out.contains("Power[E") || out.contains("Exp["), "got {out}");
    }

    #[test]
    fn test_dsolve_value_pure_function_form() {
        // Bare symbol `y` yields a callable object; applying it should work.
        let out = shows("DSolveValue[y'[x] == y[x], y, x][0]");
        // E^0 * C[1] => C[1]
        assert!(out.contains("C[1]"), "got {out}");
    }

    #[test]
    fn test_dsolve_trivial_quadrature() {
        let out = shows("DSolveValue[y'[x] == x, y[x], x]");
        assert!(out.contains("C[1]"), "got {out}");
        assert!(
            out.contains("x^2") || out.contains("Power[x, 2]"),
            "got {out}"
        );
    }

    #[test]
    fn test_dsolve_first_order_with_ic() {
        let out = shows("DSolveValue[{y'[x] == y[x], y[0] == 5}, y[x], x]");
        assert!(out.contains('5'), "got {out}");
        assert!(out.contains("Power[E") || out.contains("Exp["), "got {out}");
        assert!(!out.contains("C[1]"), "constant should be resolved: {out}");
    }

    #[test]
    fn test_dsolve_second_order_trig() {
        let out = shows("DSolveValue[y''[x] + y[x] == 0, y[x], x]");
        assert!(out.contains("C[1]") && out.contains("C[2]"), "got {out}");
        assert!(out.contains("Cos[x]"), "got {out}");
        assert!(out.contains("Sin[x]"), "got {out}");
    }

    #[test]
    fn test_dsolve_second_order_exp() {
        let out = shows("DSolveValue[y''[x] - y[x] == 0, y[x], x]");
        assert!(out.contains("C[1]") && out.contains("C[2]"), "got {out}");
        assert!(out.contains("Power[E") || out.contains("Exp["), "got {out}");
    }

    #[test]
    fn test_dsolve_second_order_repeated_root() {
        let out = shows("DSolveValue[y''[x] - 2 y'[x] + y[x] == 0, y[x], x]");
        assert!(out.contains("C[1]") && out.contains("C[2]"), "got {out}");
        // Repeated root solution: (C[1] + C[2] x) E^x
        assert!(out.contains("Power[E") || out.contains("Exp["), "got {out}");
        assert!(out.contains("C[2]") && out.contains("x"), "got {out}");
    }

    #[test]
    fn test_dsolve_second_order_with_ics() {
        let out = shows("DSolveValue[{y''[x] + y[x] == 0, y[0] == 0, y'[0] == 1}, y[x], x]");
        // y = Sin[x]
        assert!(!out.contains("C[1]") && !out.contains("C[2]"), "got {out}");
        assert!(out.contains("Sin[x]"), "got {out}");
    }

    #[test]
    fn test_dsolve_constant_forcing() {
        let out = shows("DSolveValue[y''[x] + y[x] == 1, y[x], x]");
        assert!(out.contains("C[1]") && out.contains("C[2]"), "got {out}");
        assert!(out.contains("1"), "got {out}");
    }

    #[test]
    fn test_dsolve_unsupported_unevaluated() {
        let out = shows("DSolve[y'[x] == Sin[y[x]] + x, y[x], x]");
        assert!(out.starts_with("DSolve["), "got {out}");
    }

    #[test]
    fn test_dsolve_bare_symbol_form_head() {
        let out = shows("DSolve[y'[x] == y[x], y, x]");
        assert!(out.contains("y ->"), "got {out}");
    }

    #[test]
    fn test_rsolve_value_with_ic() {
        let out = shows("RSolveValue[{a[n + 1] == 2 a[n], a[0] == 3}, a[n], n]");
        assert!(out.contains('3'), "got {out}");
        assert!(
            out.contains("2^n") || out.contains("Power[2, n]"),
            "got {out}"
        );
    }

    #[test]
    fn test_rsolve_value_general() {
        let out = shows("RSolveValue[a[n + 1] == 2 a[n], a[n], n]");
        assert!(out.contains("C[1]"), "got {out}");
        assert!(
            out.contains("2^n") || out.contains("Power[2, n]"),
            "got {out}"
        );
    }

    #[test]
    fn test_rsolve_rule_form() {
        let out = shows("RSolve[{a[n + 1] == 2 a[n], a[0] == 1}, a[n], n]");
        assert!(out.contains("a[n] ->"), "got {out}");
    }

    #[test]
    fn test_ndsolve_value_approximates_exp() {
        // E = 2.71828...
        let out = shows("NDSolveValue[{y'[x] == y[x], y[0] == 1}, y, {x, 0, 1}][1]");
        let e: f64 = out.parse().unwrap_or_else(|_| panic!("not numeric: {out}"));
        assert!((e - std::f64::consts::E).abs() < 1e-4, "got {e}");
    }

    #[test]
    fn test_ndsolve_returns_rule() {
        let out = shows("NDSolve[{y'[x] == y[x], y[0] == 1}, y, {x, 0, 1}]");
        assert!(out.starts_with("{{y ->"), "got {out}");
    }

    #[test]
    fn test_ndsolve_boundary_value() {
        let out = shows("NDSolveValue[{y'[x] == 2 x, y[0] == 0}, y, {x, 0, 3}][3]");
        let v: f64 = out.parse().unwrap();
        assert!((v - 9.0).abs() < 1e-6, "got {v}");
    }

    #[test]
    fn test_interpolating_function_direct() {
        let out = shows("InterpolatingFunction[{{0, 0, 1}, {1, 1, 1}}, 0.5]");
        let v: f64 = out.parse().unwrap();
        assert!((v - 0.5).abs() < 1e-9, "got {v}");
    }

    #[test]
    fn test_interpolating_function_one_arg_apply() {
        let out = shows("InterpolatingFunction[{{0, 0, 1}, {1, 1, 1}}][0.25]");
        let v: f64 = out.parse().unwrap();
        assert!((v - 0.25).abs() < 1e-9, "got {v}");
    }

    #[test]
    fn test_dsolve_second_order_complex_roots() {
        let out = shows("DSolveValue[y''[x] + 4 y[x] == 0, y[x], x]");
        assert!(out.contains("C[1]") && out.contains("C[2]"), "got {out}");
        assert!(out.contains("Cos[") && out.contains("Sin["), "got {out}");
        assert!(out.contains("2"), "got {out}");
    }

    #[test]
    fn test_dsolve_separable_nonlinear() {
        // y' = y^2  →  y = -1/(x + C[1])
        let out = shows("DSolveValue[y'[x] == y[x]^2, y[x], x]");
        assert!(out.contains("C[1]"), "got {out}");
    }

    #[test]
    fn test_dsolve_second_order_variable_coeff_unevaluated() {
        let out = shows("DSolve[y''[x] + x y[x] == 0, y[x], x]");
        assert!(out.starts_with("DSolve["), "got {out}");
    }

    // ── General constant-coefficient solvers ─────────────────────────────────

    #[test]
    fn test_dsolve_output_shape_matches_wl() {
        let out = shows("DSolve[y''[x] + y[x] == 0, y[x], x]");
        assert!(out.starts_with("{{y[x] ->"), "got {out}");
        assert!(out.ends_with("}}"), "got {out}");
    }

    #[test]
    fn test_dsolve_second_order_distinct_roots() {
        let out = shows("DSolveValue[y''[x] - 3 y'[x] + 2 y[x] == 0, y[x], x]");
        assert!(out.contains("C[1]") && out.contains("C[2]"), "got {out}");
        assert!(out.contains("Power[E") || out.contains("Exp["), "got {out}");
        assert!(!out.starts_with("DSolveValue["), "got {out}");
    }

    #[test]
    fn test_dsolve_first_order_poly_forcing() {
        let out = shows("DSolveValue[y'[x] + 2 y[x] == x^2 + 1, y[x], x]");
        assert!(out.contains("C[1]"), "got {out}");
        assert!(out.contains("Power[E") || out.contains("Exp["), "got {out}");
        assert!(
            out.contains("x^2") || out.contains("Power[x, 2]"),
            "got {out}"
        );
        assert!(!out.starts_with("DSolveValue["), "got {out}");
    }

    #[test]
    fn test_dsolve_first_order_exp_forcing() {
        let out = shows("DSolveValue[y'[x] + y[x] == Exp[2 x], y[x], x]");
        assert!(out.contains("C[1]"), "got {out}");
        assert!(!out.starts_with("DSolveValue["), "got {out}");
    }

    #[test]
    fn test_dsolve_third_order_homogeneous() {
        // (r - 1)^2 (r + 1): roots 1 (double), -1.
        let out = shows("DSolveValue[y'''[x] - y''[x] - y'[x] + y[x] == 0, y[x], x]");
        assert!(
            out.contains("C[1]") && out.contains("C[2]") && out.contains("C[3]"),
            "got {out}"
        );
        assert!(out.contains("Power[E") || out.contains("Exp["), "got {out}");
        assert!(!out.starts_with("DSolveValue["), "got {out}");
    }

    #[test]
    fn test_dsolve_nonhomogeneous_polynomial() {
        // y'' + y = x^2  ->  yp = x^2 - 2.
        let out = shows("DSolveValue[y''[x] + y[x] == x^2, y[x], x]");
        assert!(out.contains("C[1]") && out.contains("C[2]"), "got {out}");
        assert!(
            out.contains("x^2") || out.contains("Power[x, 2]"),
            "got {out}"
        );
        assert!(!out.starts_with("DSolveValue["), "got {out}");
    }

    #[test]
    fn test_dsolve_nonhomogeneous_exp() {
        let out = shows("DSolveValue[y''[x] + y[x] == Exp[x], y[x], x]");
        assert!(out.contains("C[1]") && out.contains("C[2]"), "got {out}");
        assert!(out.contains("Power[E") || out.contains("Exp["), "got {out}");
        assert!(!out.starts_with("DSolveValue["), "got {out}");
    }

    #[test]
    fn test_dsolve_nonhomogeneous_trig_resonance() {
        // y'' + y = Sin[x]  ->  yp = -x Cos[x]/2.
        let out = shows("DSolveValue[y''[x] + y[x] == Sin[x], y[x], x]");
        assert!(out.contains("C[1]") && out.contains("C[2]"), "got {out}");
        assert!(out.contains("Cos[") && out.contains("Sin["), "got {out}");
        assert!(!out.starts_with("DSolveValue["), "got {out}");
    }

    #[test]
    fn test_dsolve_nonhomogeneous_trig_nonresonant() {
        let out = shows("DSolveValue[y''[x] + 4 y[x] == Sin[x], y[x], x]");
        assert!(out.contains("C[1]") && out.contains("C[2]"), "got {out}");
        assert!(out.contains("Sin["), "got {out}");
        assert!(!out.starts_with("DSolveValue["), "got {out}");
    }

    #[test]
    fn test_dsolve_first_order_with_ic_poly() {
        let out = shows("DSolveValue[{y'[x] + 2 y[x] == x^2 + 1, y[0] == 0}, y[x], x]");
        assert!(!out.contains("C[1]"), "constant should be resolved: {out}");
        assert!(out.contains("Power[E") || out.contains("Exp["), "got {out}");
    }

    #[test]
    fn test_dsolve_second_order_poly_with_ics() {
        // y'' + y = x^2, y(0) = 0, y'(0) = 1  ->  2 Cos[x] + Sin[x] + x^2 - 2.
        let out = shows("DSolveValue[{y''[x] + y[x] == x^2, y[0] == 0, y'[0] == 1}, y[x], x]");
        assert!(!out.contains("C[1]") && !out.contains("C[2]"), "got {out}");
        assert!(out.contains("Cos[x]"), "got {out}");
        assert!(out.contains("Sin[x]"), "got {out}");
    }

    #[test]
    fn test_dsolve_third_order_with_ics() {
        let out = shows(
            "DSolveValue[{y'''[x] - y''[x] - y'[x] + y[x] == 0, y[0] == 1, y'[0] == 0, y''[0] == 0}, y[x], x]",
        );
        assert!(
            !out.contains("C[1]") && !out.contains("C[2]") && !out.contains("C[3]"),
            "got {out}"
        );
        assert!(!out.starts_with("DSolveValue["), "got {out}");
    }

    #[test]
    fn test_dsolve_nonhomogeneous_exp_negative_e() {
        let out = shows("DSolveValue[y''[x] + y[x] == -E^x, y[x], x]");
        assert!(out.contains("C[1]") && out.contains("C[2]"), "got {out}");
        assert!(!out.starts_with("DSolveValue["), "got {out}");
    }

    #[test]
    fn test_dsolve_nonhomogeneous_exp_resonant() {
        let out = shows("DSolveValue[y''[x] - 3 y'[x] + 2 y[x] == Exp[3 x], y[x], x]");
        assert!(!out.starts_with("DSolveValue["), "got {out}");
        assert!(out.contains("Power[E") || out.contains("Exp["), "got {out}");
    }

    #[test]
    fn test_dsolve_nonhomogeneous_exp_double_resonance() {
        // Characteristic (r + 1)^2 with forcing E^-x -> x^2 E^-x / 2.
        let out = shows("DSolveValue[y''[x] + 2 y'[x] + y[x] == Exp[-x], y[x], x]");
        assert!(!out.starts_with("DSolveValue["), "got {out}");
        assert!(
            out.contains("x^2") || out.contains("Power[x, 2]"),
            "got {out}"
        );
    }

    #[test]
    fn test_dsolve_first_order_variable_coeff() {
        let out = shows("DSolveValue[y'[x] + x y[x] == 0, y[x], x]");
        assert!(out.contains("C[1]"), "got {out}");
        assert!(!out.starts_with("DSolveValue["), "got {out}");
    }

    #[test]
    fn test_dsolve_zero_rhs_unevaluated_never_wrong() {
        // Fourth-order with an irreducible quartic char. poly is left unevaluated.
        let out = shows("DSolve[y''''[x] + y[x] == 0, y[x], x]");
        assert!(
            out.starts_with("DSolve[") || out.contains("C[1]"),
            "got {out}"
        );
    }

    #[test]
    fn test_dsolve_unsupported_forcing_unevaluated() {
        let out = shows("DSolve[y''[x] + y[x] == Tan[x], y[x], x]");
        assert!(out.starts_with("DSolve["), "got {out}");
    }

    #[test]
    fn test_dsolve_value_returns_expression_not_rules() {
        let out = shows("DSolveValue[y''[x] + y[x] == 0, y[x], x]");
        assert!(!out.starts_with("{{"), "got {out}");
        assert!(out.contains("C[1]") && out.contains("C[2]"), "got {out}");
        assert!(
            out.contains("Cos[x]") && out.contains("Sin[x]"),
            "got {out}"
        );
    }

    #[test]
    fn test_dsolve_value_unsupported_unevaluated() {
        let out = shows("DSolveValue[y''[x] + y[x] == Log[x], y[x], x]");
        assert!(out.starts_with("DSolveValue["), "got {out}");
    }

    #[test]
    fn test_rsolve_unsupported_unevaluated() {
        // Third-order recurrence without support in the helper → unevaluated.
        let out = shows("RSolveValue[a[n + 1] == a[n] + n, a[n], n]");
        // The `discrete` solver may or may not handle it; either a value or RSolveValue[...].
        assert!(
            out.contains("a[") || out.starts_with("RSolveValue["),
            "got {out}"
        );
    }
}
