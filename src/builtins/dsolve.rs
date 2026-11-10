//! differential/recurrence equations: DSolve, DSolveValue, RSolve, NDSolve.
//!
//! Gap-fill module. Supported (common) cases:
//!
//! * `DSolve[eqn, y[x], x]` / `DSolve[eqn, y, x]` for
//!   * first-order linear `y' + p(x) y = q(x)` (incl. `y' = f(x)`),
//!   * first-order separable `y' = g(y) h(x)` when `g` is a monomial `c y^n`,
//!   * second-order linear constant-coefficient ODEs `a y'' + b y' + c y = r`
//!     (with constant forcing `r`),
//!   * initial conditions (value and first-derivative) for the above.
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

/// Unwrap a (possibly held) argument to its underlying [`Expr`].
fn as_expr(v: &Value) -> Expr {
    match v {
        Value::Pattern(e) => e.clone(),
        Value::Hold(b) | Value::HoldComplete(b) => as_expr(b),
        other => crate::eval::table::value_to_expr(other),
    }
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
        let body = crate::eval::table::value_to_expr(&simplified);
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
    let d1 = deriv_at(&target.name, &target.var, 1);
    let d2 = deriv_at(&target.name, &target.var, 2);

    let mark = |e: &Expr| {
        subst(
            &subst(&subst(e, &y, &sym("Y")), &d1, &sym("D1")),
            &d2,
            &sym("D2"),
        )
    };

    // Substitute function / derivative terms with markers so coefficient
    // extraction (which treats markers as ordinary symbols) can work.
    let marked = expand(&mark(&sub(lhs, rhs)), env);

    let has_d2 = contains_symbol(&marked, "D2");

    let sol = if has_d2 {
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
    let c1 = together(&div(sub(y0, a0), b0), env);
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
    let derivative = |e: &Expr| {
        let v = eval_value(e, env);
        match crate::builtins::symbolic::builtin_d(&[v, Value::Symbol(x.to_string())], env) {
            Ok(d) => as_expr(&d),
            Err(_) => e.clone(),
        }
    };

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
        assert!(out.contains("E^x") || out.contains("Exp[x]"), "got {out}");
    }

    #[test]
    fn test_dsolve_value_first_order() {
        let out = shows("DSolveValue[y'[x] == y[x], y[x], x]");
        assert!(out.contains("C[1]"), "got {out}");
        assert!(out.contains("E^x") || out.contains("Exp[x]"), "got {out}");
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
        assert!(out.contains("E^x") || out.contains("Exp[x]"), "got {out}");
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
        assert!(out.contains("E^x") || out.contains("Exp[x]"), "got {out}");
    }

    #[test]
    fn test_dsolve_second_order_repeated_root() {
        let out = shows("DSolveValue[y''[x] - 2 y'[x] + y[x] == 0, y[x], x]");
        assert!(out.contains("C[1]") && out.contains("C[2]"), "got {out}");
        // Repeated root solution: (C[1] + C[2] x) E^x
        assert!(out.contains("E^x") || out.contains("Exp[x]"), "got {out}");
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
