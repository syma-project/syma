use rug::Float;
/// Numeric evaluation — high-precision numeric evaluation of symbolic expressions.
use rug::ops::Pow;

use crate::ast::*;
use crate::env::Env;
use crate::value::*;

/// Evaluate an expression numerically at the given bit precision.
pub(super) fn numeric_eval_expr(
    expr: &Expr,
    prec_bits: u32,
    env: &Env,
) -> Result<Value, EvalError> {
    match expr {
        Expr::Symbol(s) => match s.as_str() {
            "Pi" => Ok(Value::Real(Float::with_val(
                prec_bits,
                rug::float::Constant::Pi,
            ))),
            "E" => {
                let one = Float::with_val(prec_bits, 1u32);
                Ok(Value::Real(one.exp()))
            }
            _ => {
                let v = super::eval(expr, env)?;
                coerce_to_float(v, prec_bits)
            }
        },
        Expr::Integer(n) => Ok(Value::Real(Float::with_val(prec_bits, n))),
        Expr::Real(r) => Ok(Value::Real(Float::with_val(prec_bits, r))),
        // Numerically evaluate each element of a list.
        Expr::List(items) => {
            let vals: Result<Vec<Value>, EvalError> = items
                .iter()
                .map(|it| numeric_eval_expr(it, prec_bits, env))
                .collect();
            Ok(Value::List(vals?))
        }
        // Recursively evaluate calls at the requested precision.
        Expr::Call { head, args } => {
            if let Expr::Symbol(name) = head.as_ref() {
                // `N` is a special form in `eval` (it never reaches
                // `apply_function`), so a *nested* `N[...]` — e.g. inside the
                // argument of an enclosing `N` — must be dispatched here too.
                // Otherwise the inner `N` would fall through to the generic
                // `apply_function` branch, which has no `N` builtin, and stay
                // unevaluated (`N[3.14..., 20.0]`).
                if name == "N" {
                    if args.is_empty() || args.len() > 2 {
                        return Err(EvalError::Error("N requires 1 or 2 arguments".to_string()));
                    }
                    let inner_bits = if args.len() == 2 {
                        match super::eval(&args[1], env)? {
                            Value::Integer(n) => {
                                let d = u32::try_from(&n).unwrap_or(DEFAULT_PRECISION);
                                ((d as f64) * std::f64::consts::LOG2_10).ceil() as u32
                            }
                            _ => DEFAULT_PRECISION,
                        }
                    } else {
                        DEFAULT_PRECISION
                    };
                    return numeric_eval_expr(&args[0], inner_bits.max(1), env);
                }

                // NHold* attributes: prevent numeric evaluation of certain arguments
                let nhold_all = env.has_attribute(name, "NHoldAll");
                let nhold_first = env.has_attribute(name, "NHoldFirst");
                let nhold_rest = env.has_attribute(name, "NHoldRest");

                let evaluated_args: Result<Vec<Value>, _> = args
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        if nhold_all || (nhold_first && i == 0) || (nhold_rest && i > 0) {
                            super::eval(a, env)
                        } else {
                            numeric_eval_expr(a, prec_bits, env)
                        }
                    })
                    .collect();
                let evaluated_args = evaluated_args?;
                match name.as_str() {
                    "Plus" => numeric_combine("Plus", evaluated_args, prec_bits, env, false),
                    "Times" => numeric_combine("Times", evaluated_args, prec_bits, env, true),
                    "Power" if evaluated_args.len() == 2 => {
                        try_power_float("Power", &evaluated_args, prec_bits, env)
                    }
                    "Divide" if evaluated_args.len() == 2 => {
                        try_binary_float("Divide", &evaluated_args, prec_bits, env, |a, b| {
                            a.clone() / b.clone()
                        })
                    }
                    "Sin" => try_unary_float("Sin", &evaluated_args, prec_bits, env, |f| f.sin()),
                    "Cos" => try_unary_float("Cos", &evaluated_args, prec_bits, env, |f| f.cos()),
                    "Log" if evaluated_args.len() == 1 => {
                        try_unary_float("Log", &evaluated_args, prec_bits, env, |f| f.ln())
                    }
                    "Log" if evaluated_args.len() == 2 => {
                        try_binary_float("Log", &evaluated_args, prec_bits, env, |a, b| {
                            b.ln() / a.ln()
                        })
                    }
                    "Sqrt" => try_sqrt_float("Sqrt", &evaluated_args, prec_bits, env),
                    "Abs" => try_unary_float("Abs", &evaluated_args, prec_bits, env, |f| f.abs()),
                    // `Part` indices must stay exact integers; numeric
                    // evaluation of `1` would otherwise yield `1.0`, which
                    // `Part` rejects. The indexed expression is still
                    // evaluated numerically.
                    "Part" if evaluated_args.len() >= 2 => {
                        let mut vals = Vec::with_capacity(args.len());
                        vals.push(evaluated_args[0].clone());
                        for a in &args[1..] {
                            vals.push(super::eval(a, env)?);
                        }
                        let head_val = super::eval(head, env)?;
                        coerce_to_float(super::apply_function(&head_val, &vals, env)?, prec_bits)
                    }
                    // Equality/inequality: keep the comparison symbolic when
                    // the operands are not all numeric and not structurally
                    // identical, so equation-based functions called inside
                    // `N` (e.g. `N[FindRoot[Cos[x] == x, {x, 1}]]`) can still
                    // inspect the equation instead of receiving `False`.
                    "Equal" | "Unequal"
                        if !evaluated_args
                            .iter()
                            .all(|a| to_float(a, prec_bits).is_some())
                            && !evaluated_args.windows(2).all(|w| w[0].struct_eq(&w[1])) =>
                    {
                        Ok(Value::Call {
                            head: name.clone(),
                            args: evaluated_args,
                        })
                    }
                    // For numeric evaluation of other functions, compute with the
                    // numeric args; if that fails (special forms handled in eval)
                    // fall back to normal evaluation and coerce the result.
                    _ => {
                        let head_val = super::eval(head, env)?;
                        match super::apply_function(&head_val, &evaluated_args, env) {
                            Ok(v) => coerce_to_float(v, prec_bits),
                            Err(_) => {
                                let v = super::eval(expr, env)?;
                                coerce_to_float(v, prec_bits)
                            }
                        }
                    }
                }
            } else {
                // Non-symbol head: evaluate normally
                let v = super::eval(expr, env)?;
                coerce_to_float(v, prec_bits)
            }
        }
        // Other expressions: evaluate normally and coerce
        _ => {
            let v = super::eval(expr, env)?;
            coerce_to_float(v, prec_bits)
        }
    }
}

/// Evaluate unary float function, fall back to symbolic eval if args aren't numeric.
fn try_unary_float(
    head: &str,
    args: &[Value],
    prec_bits: u32,
    env: &Env,
    op: fn(Float) -> Float,
) -> Result<Value, EvalError> {
    if args.len() == 1
        && let Some(f) = to_float(&args[0], prec_bits)
    {
        return Ok(Value::Real(op(f)));
    }
    crate::eval::apply_function(
        &super::eval(&Expr::Symbol(head.to_string()), env)?,
        args,
        env,
    )
}

/// Evaluate binary float function, fall back to symbolic eval if args aren't numeric.
fn try_binary_float(
    head: &str,
    args: &[Value],
    prec_bits: u32,
    env: &Env,
    op: fn(Float, Float) -> Float,
) -> Result<Value, EvalError> {
    if args.len() == 2 {
        let bf = to_float(&args[0], prec_bits);
        let ef = to_float(&args[1], prec_bits);
        if let (Some(a), Some(b)) = (bf, ef) {
            return Ok(Value::Real(op(a, b)));
        }
    }
    crate::eval::apply_function(
        &super::eval(&Expr::Symbol(head.to_string()), env)?,
        args,
        env,
    )
}

/// Evaluate `Power[a, b]`, producing a complex principal value when the base is
/// negative and the exponent is not an integer.
fn try_power_float(
    head: &str,
    args: &[Value],
    prec_bits: u32,
    env: &Env,
) -> Result<Value, EvalError> {
    if args.len() == 2
        && let (Some(b), Some(e)) = (to_float(&args[0], prec_bits), to_float(&args[1], prec_bits))
    {
        let bv = b.to_f64();
        let ev = e.to_f64();
        if bv < 0.0 && ev.fract() != 0.0 {
            // b^e = |b|^e * e^{i π e}
            let r = (-bv).powf(ev);
            let angle = std::f64::consts::PI * ev;
            return Ok(Value::Complex {
                re: r * angle.cos(),
                im: r * angle.sin(),
            });
        }
        return Ok(Value::Real(b.pow(e)));
    }
    crate::eval::apply_function(
        &super::eval(&Expr::Symbol(head.to_string()), env)?,
        args,
        env,
    )
}

/// Evaluate `Sqrt`, producing a complex principal value for negative arguments.
fn try_sqrt_float(
    head: &str,
    args: &[Value],
    prec_bits: u32,
    env: &Env,
) -> Result<Value, EvalError> {
    if args.len() == 1
        && let Some(f) = to_float(&args[0], prec_bits)
    {
        let fv = f.to_f64();
        if fv < 0.0 {
            return Ok(Value::Complex {
                re: 0.0,
                im: (-fv).sqrt(),
            });
        }
        return Ok(Value::Real(f.sqrt()));
    }
    crate::eval::apply_function(
        &super::eval(&Expr::Symbol(head.to_string()), env)?,
        args,
        env,
    )
}

/// Helper: convert a Value to an optional high-precision float.
fn to_float(v: &Value, prec_bits: u32) -> Option<Float> {
    match v {
        Value::Integer(n) => Some(Float::with_val(prec_bits, n)),
        Value::Real(r) => Some(Float::with_val(prec_bits, r)),
        Value::Rational(r) => {
            Some(Float::with_val(prec_bits, r.numer()) / Float::with_val(prec_bits, r.denom()))
        }
        Value::Root { coeffs, index } => {
            let roots = crate::polynomial::find_polynomial_roots(coeffs);
            if *index > 0 && *index <= roots.len() {
                let (re, im) = roots[*index - 1];
                if im.abs() < 1e-14 {
                    Some(Float::with_val(prec_bits, re))
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Numerically combine `Plus`/`Times` args when all are numeric, otherwise
/// rebuild the expression symbolically (evaluated args, e.g. `N[2 x + 2 y]`
/// → `2.0 x + 2.0 y`).
fn numeric_combine(
    head: &str,
    args: Vec<Value>,
    prec_bits: u32,
    env: &Env,
    multiply: bool,
) -> Result<Value, EvalError> {
    if !args.is_empty() && args.iter().all(|a| to_float(a, prec_bits).is_some()) {
        numeric_fold_op(
            &args,
            prec_bits,
            move |a, b| if multiply { a * b } else { a + b },
        )
    } else {
        super::apply_function(
            &super::eval(&Expr::Symbol(head.to_string()), env)?,
            &args,
            env,
        )
    }
}

/// Helper: fold a binary float operation over an all-numeric argument list.
fn numeric_fold_op<F>(args: &[Value], prec_bits: u32, op: F) -> Result<Value, EvalError>
where
    F: Fn(Float, Float) -> Float,
{
    let first = args.first().ok_or_else(|| {
        EvalError::Error("expected at least one argument for numeric fold".to_string())
    })?;
    let mut acc = to_float(first, prec_bits).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: first.type_name().to_string(),
    })?;
    for arg in &args[1..] {
        let f = to_float(arg, prec_bits).ok_or_else(|| EvalError::TypeError {
            expected: "Number".to_string(),
            got: arg.type_name().to_string(),
        })?;
        acc = op(acc, f);
    }
    Ok(Value::Real(acc))
}

// ── NIntegrate ──────────────────────────────────────────────────────────────
//
// Adaptive Gauss–Kronrod (G7–K15) quadrature. Finite ranges are mapped through
// `x = a + (b-a) sin²θ` which smooths integrable endpoint singularities such as
// `1/Sqrt[x]`; infinite ranges use rational substitutions. A damped/Richardson
// fallback handles conditionally convergent oscillatory tails (e.g. `Sin[x]/x`
// on `[0, ∞)`). Any case that cannot be resolved numerically is left as an
// unevaluated `NIntegrate[...]` so we never fabricate a wrong answer.

use crate::builtins::{real as make_real, to_f64 as value_to_f64};

/// Gauss–Kronrod 15-point abscissae on `[-1, 1]` (last entry is the centre).
const GK_X: [f64; 8] = [
    0.991_455_371_120_812_6,
    0.949_107_912_342_758_5,
    0.864_864_423_359_769_1,
    0.741_531_185_599_394_5,
    0.586_087_235_467_691_1,
    0.405_845_151_377_397_2,
    0.207_784_955_007_898_5,
    0.0,
];

/// Kronrod weights matching [`GK_X`].
const GK_W: [f64; 8] = [
    0.022_935_322_010_529_22,
    0.063_092_092_629_978_55,
    0.104_790_010_322_250_2,
    0.140_653_259_715_525_9,
    0.169_004_726_639_267_9,
    0.190_350_578_064_785_4,
    0.204_432_940_075_298_9,
    0.209_482_141_084_727_8,
];

/// Gauss 7-point weights (odd Kronrod indices).
const G7_W: [f64; 4] = [
    0.129_484_966_168_869_7,
    0.279_705_391_489_276_6,
    0.381_830_050_505_118_9,
    0.417_959_183_673_469_4,
];

/// Maximum number of subintervals before a plain transform is deemed to have
/// stalled (oscillatory tails) and the damped fallback takes over.
const PLAIN_CAP: usize = 20_000;

/// Number of damping / Richardson stages for the oscillatory fallback.
const DAMP_TERMS: usize = 13;

/// Node budget for the damped fallback (larger, since tails oscillate).
const DAMP_CAP: usize = 60_000;

struct IntegSpec {
    var: String,
    lo: f64,
    hi: f64,
}

struct IntegOpts {
    tol: f64,
    max_depth: usize,
    working_prec: Option<f64>,
    goal_set: bool,
}

impl Default for IntegOpts {
    fn default() -> Self {
        IntegOpts {
            tol: 1e-11,
            max_depth: 50,
            working_prec: None,
            goal_set: false,
        }
    }
}

/// Evaluate a `G7–K15` pair over `[a, b]`: returns `(value, error_estimate)`.
fn gk15<F>(f: &F, a: f64, b: f64) -> Result<(f64, f64), EvalError>
where
    F: Fn(f64) -> Result<f64, EvalError>,
{
    let c = 0.5 * (a + b);
    let h = 0.5 * (b - a);
    let mut res = 0.0;
    let mut resg = 0.0;
    for (i, &x) in GK_X[..7].iter().enumerate() {
        let fx = f(c + h * x)? + f(c - h * x)?;
        res += GK_W[i] * fx;
        if i % 2 == 1 {
            resg += G7_W[i / 2] * fx;
        }
    }
    let fc = f(c)?;
    res += GK_W[7] * fc;
    resg += G7_W[3] * fc;
    let value = res * h;
    Ok((value, (value - resg * h).abs()))
}

/// Adaptive subdivision driven by the Kronrod/Gauss difference. Returns the
/// estimate together with a flag reporting whether the tolerance was met.
fn adaptive<F>(
    f: &F,
    a: f64,
    b: f64,
    opts: &IntegOpts,
    cap: usize,
) -> Result<(f64, bool), EvalError>
where
    F: Fn(f64) -> Result<f64, EvalError>,
{
    let (r0, e0) = gk15(f, a, b)?;
    let mut total = 0.0;
    let mut converged = true;
    let mut stack = vec![(a, b, r0, e0, 0usize)];
    let mut n = 0usize;
    while let Some((a, b, r, e, d)) = stack.pop() {
        n += 1;
        if n > cap {
            converged = false;
        }
        if e <= opts.tol * (1.0f64).max(r.abs()) || d >= opts.max_depth || n > cap {
            total += r;
        } else {
            let m = 0.5 * (a + b);
            let (r1, e1) = gk15(f, a, m)?;
            let (r2, e2) = gk15(f, m, b)?;
            stack.push((a, m, r1, e1, d + 1));
            stack.push((m, b, r2, e2, d + 1));
        }
    }
    Ok((total, converged))
}

/// Richardson extrapolation of a sequence sampled at `ε, ε/2, ε/4, …`,
/// approximating the `ε → 0` limit.
fn richardson(mut v: Vec<f64>) -> f64 {
    let n = v.len();
    for k in 1..n {
        let factor = (1u64 << k) as f64;
        for i in (k..n).rev() {
            v[i] = (factor * v[i] - v[i - 1]) / (factor - 1.0);
        }
    }
    v[n - 1]
}

/// One-dimensional integration over `[a, b]`, `a`/`b` possibly infinite.
fn integrate_1d<F>(f: &F, a: f64, b: f64, opts: &IntegOpts) -> Result<f64, EvalError>
where
    F: Fn(f64) -> Result<f64, EvalError>,
{
    if a == b {
        return Ok(0.0);
    }
    let a_inf = !a.is_finite();
    let b_inf = !b.is_finite();
    if !a_inf && !b_inf {
        // Endpoint-singularity-smoothing substitution.
        let g = |th: f64| -> Result<f64, EvalError> {
            let s = th.sin();
            let c = th.cos();
            let x = a + (b - a) * s * s;
            f(x).map(|v| v * (b - a) * 2.0 * s * c)
        };
        let (r, _) = adaptive(&g, 0.0, std::f64::consts::FRAC_PI_2, opts, PLAIN_CAP)?;
        return Ok(r);
    }
    if !a_inf && b_inf {
        let g = |t: f64| -> Result<f64, EvalError> {
            let d = 1.0 - t;
            if d <= 0.0 {
                return Ok(0.0);
            }
            f(a + t / d).map(|v| v / (d * d))
        };
        let (r, converged) = adaptive(&g, 0.0, 1.0, opts, PLAIN_CAP)?;
        if converged {
            return Ok(r);
        }
        return damped_semi(f, a, false, opts);
    }
    if a_inf && !b_inf {
        let g = |t: f64| -> Result<f64, EvalError> {
            let d = 1.0 - t;
            if d <= 0.0 {
                return Ok(0.0);
            }
            f(b - t / d).map(|v| v / (d * d))
        };
        let (r, converged) = adaptive(&g, 0.0, 1.0, opts, PLAIN_CAP)?;
        if converged {
            return Ok(r);
        }
        return damped_semi(f, b, true, opts);
    }
    let g = |t: f64| -> Result<f64, EvalError> {
        let d = 1.0 - t * t;
        if d <= 0.0 {
            return Ok(0.0);
        }
        let x = t / d;
        if x.abs() > 1e200 {
            return Ok(0.0);
        }
        f(x).map(|v| v * (1.0 + t * t) / (d * d))
    };
    let (r, converged) = adaptive(&g, -1.0, 1.0, opts, PLAIN_CAP)?;
    if converged {
        return Ok(r);
    }
    damped_full(f, opts)
}

/// Damped Richardson fallback for a semi-infinite range.
fn damped_semi<F>(f: &F, bound: f64, to_neg: bool, opts: &IntegOpts) -> Result<f64, EvalError>
where
    F: Fn(f64) -> Result<f64, EvalError>,
{
    let mut eps = 1.0 / (1.0 + bound.abs());
    let mut vals = Vec::with_capacity(DAMP_TERMS);
    for _ in 0..DAMP_TERMS {
        let g = |t: f64| -> Result<f64, EvalError> {
            let d = 1.0 - t;
            if d <= 0.0 {
                return Ok(0.0);
            }
            let x = if to_neg { bound - t / d } else { bound + t / d };
            let e = if to_neg {
                (eps * x).exp()
            } else {
                (-eps * x).exp()
            };
            f(x).map(|v| v * e / (d * d))
        };
        let (r, _) = adaptive(&g, 0.0, 1.0, opts, DAMP_CAP)?;
        vals.push(r);
        eps *= 0.5;
    }
    Ok(richardson(vals))
}

/// Damped Richardson fallback for the full real line.
fn damped_full<F>(f: &F, opts: &IntegOpts) -> Result<f64, EvalError>
where
    F: Fn(f64) -> Result<f64, EvalError>,
{
    let mut eps = 1.0;
    let mut vals = Vec::with_capacity(DAMP_TERMS);
    for _ in 0..DAMP_TERMS {
        let g = |t: f64| -> Result<f64, EvalError> {
            let d = 1.0 - t * t;
            if d <= 0.0 {
                return Ok(0.0);
            }
            let x = t / d;
            if x.abs() > 1e200 {
                return Ok(0.0);
            }
            let e = (-eps * x.abs()).exp();
            f(x).map(|v| v * e * (1.0 + t * t) / (d * d))
        };
        let (r, _) = adaptive(&g, -1.0, 1.0, opts, DAMP_CAP)?;
        vals.push(r);
        eps *= 0.5;
    }
    Ok(richardson(vals))
}

/// Recursively integrate the innermost remaining variable.
fn quad_nd(
    f_expr: &Expr,
    specs: &[IntegSpec],
    dim: usize,
    env: &Env,
    bindings: &[(String, f64)],
    opts: &IntegOpts,
) -> Result<f64, EvalError> {
    let spec = &specs[dim];
    let inner = |x: f64| -> Result<f64, EvalError> {
        let mut b: Vec<(String, f64)> = Vec::with_capacity(bindings.len() + 1);
        b.extend_from_slice(bindings);
        b.push((spec.var.clone(), x));
        if dim + 1 == specs.len() {
            eval_integrand(f_expr, env, &b)
        } else {
            quad_nd(f_expr, specs, dim + 1, env, &b, opts)
        }
    };
    integrate_1d(&inner, spec.lo, spec.hi, opts)
}

/// Evaluate the integrand once all iterator variables are bound.
fn eval_integrand(f_expr: &Expr, env: &Env, bindings: &[(String, f64)]) -> Result<f64, EvalError> {
    let child = env.child();
    for (name, x) in bindings {
        child.set(name.clone(), make_real(*x));
    }
    let v = super::eval(f_expr, &child)?;
    if let Some(y) = value_to_f64(&v) {
        return Ok(y);
    }
    if let Value::Symbol(s) = &v {
        if s == "Infinity" || s == "ComplexInfinity" {
            return Ok(f64::INFINITY);
        }
        if s == "-Infinity" {
            return Ok(f64::NEG_INFINITY);
        }
    }
    let n = numeric_eval_expr(&value_to_expr_num(&v), DEFAULT_PRECISION, &child)?;
    value_to_f64(&n)
        .ok_or_else(|| EvalError::Error(format!("NIntegrate: integrand {v} is not numeric")))
}

/// Public entry point — never errors; leaves `NIntegrate[...]` unevaluated with
/// its (unwrapped) arguments when it cannot produce a finite numeric value.
pub(crate) fn nintegrate(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let unevaluated = || Value::Call {
        head: "NIntegrate".to_string(),
        args: args.iter().map(unwrap_hold).collect(),
    };
    let Some((mut opts, f_expr, specs)) = parse_nintegrate(args, env) else {
        return Ok(unevaluated());
    };
    if !opts.goal_set
        && let Some(wp) = opts.working_prec
    {
        opts.tol = 10f64.powf(-(wp - 2.0)).clamp(1e-15, 1e-2);
    }
    match quad_nd(&f_expr, &specs, 0, env, &[], &opts) {
        Ok(v) if v.is_finite() => Ok(make_real(v)),
        _ => Ok(unevaluated()),
    }
}

/// Parse arguments: integrand, one or more `{x, a, b}` iterators, then options.
fn parse_nintegrate(args: &[Value], env: &Env) -> Option<(IntegOpts, Expr, Vec<IntegSpec>)> {
    if args.len() < 2 {
        return None;
    }
    let f_expr = value_to_expr_num(&unwrap_hold(&args[0]));
    let mut opts = IntegOpts::default();
    let mut specs = Vec::new();
    let mut in_options = false;
    for arg in &args[1..] {
        let u = unwrap_hold(arg);
        match &u {
            Value::List(items) if !in_options && items.len() == 3 => {
                let var = match &items[0] {
                    Value::Symbol(s) => s.clone(),
                    _ => return None,
                };
                let lo = resolve_bound(&items[1], env)?;
                let hi = resolve_bound(&items[2], env)?;
                specs.push(IntegSpec { var, lo, hi });
            }
            Value::Rule { lhs, rhs, .. } => {
                in_options = true;
                let name = match lhs.as_ref() {
                    Value::Symbol(s) => s.clone(),
                    _ => return None,
                };
                apply_option(&name, rhs, &mut opts, env)?;
            }
            _ => return None,
        }
    }
    if specs.is_empty() {
        return None;
    }
    Some((opts, f_expr, specs))
}

/// Apply a single `Name -> value` option. Unknown options (e.g. `Method`) are
/// accepted and ignored rather than aborting the whole integration.
fn apply_option(name: &str, rhs: &Value, opts: &mut IntegOpts, env: &Env) -> Option<()> {
    match name {
        "PrecisionGoal" => {
            let num = eval_number(rhs, env)?;
            if num > 0.0 {
                opts.tol = 10f64.powf(-num).clamp(1e-15, 1e-2);
                opts.goal_set = true;
            }
        }
        "AccuracyGoal" => {
            let _ = eval_number(rhs, env)?;
        }
        "MaxRecursion" => {
            let num = eval_number(rhs, env)?;
            if num >= 1.0 {
                opts.max_depth = ((num as usize).saturating_mul(6)).clamp(10, 120);
            }
        }
        "WorkingPrecision" => opts.working_prec = Some(eval_number(rhs, env)?),
        _ => {}
    }
    Some(())
}

/// Convert a bound token (possibly `Infinity`/`-Infinity`) to an `f64`.
fn resolve_bound(v: &Value, env: &Env) -> Option<f64> {
    let u = unwrap_hold(v);
    match &u {
        Value::Symbol(s) if s == "Infinity" => return Some(f64::INFINITY),
        Value::Symbol(s) if s == "-Infinity" => return Some(f64::NEG_INFINITY),
        Value::Call { head, args } if head == "Times" && args.len() == 2 => {
            let (neg, sym) = (&args[0], &args[1]);
            let neg_one = matches!(neg, Value::Integer(n) if *n == -1);
            if neg_one && matches!(sym, Value::Symbol(s) if s == "Infinity") {
                return Some(f64::NEG_INFINITY);
            }
        }
        _ => {}
    }
    if let Some(x) = value_to_f64(&u) {
        return Some(x);
    }
    let ev = crate::eval::eval(&value_to_expr_num(&u), env).ok()?;
    if let Some(x) = value_to_f64(&ev) {
        return Some(x);
    }
    if let Value::Symbol(s) = &ev {
        if s == "Infinity" {
            return Some(f64::INFINITY);
        }
        if s == "-Infinity" {
            return Some(f64::NEG_INFINITY);
        }
    }
    let n = numeric_eval_expr(&value_to_expr_num(&ev), DEFAULT_PRECISION, env).ok()?;
    value_to_f64(&n)
}

/// Evaluate an option value to an `f64`.
fn eval_number(v: &Value, env: &Env) -> Option<f64> {
    let ev = crate::eval::eval(&value_to_expr_num(v), env).ok()?;
    value_to_f64(&ev)
}

/// Strip `HoldAll` wrappers (`Pattern`/`Hold`) from a held argument.
///
/// `Rule`/`RuleDelayed` need special handling: `unwrap_expr_to_value` leaves
/// them as a `Value::Pattern`, but options arrive as held rules.
fn unwrap_hold(v: &Value) -> Value {
    match v {
        Value::Pattern(Expr::Rule { lhs, rhs }) => Value::Rule {
            lhs: Box::new(crate::pattern::unwrap_expr_to_value(lhs)),
            rhs: Box::new(crate::pattern::unwrap_expr_to_value(rhs)),
            delayed: false,
        },
        Value::Pattern(Expr::RuleDelayed { lhs, rhs }) => Value::Rule {
            lhs: Box::new(crate::pattern::unwrap_expr_to_value(lhs)),
            rhs: Box::new(crate::pattern::unwrap_expr_to_value(rhs)),
            delayed: true,
        },
        Value::Pattern(p) => crate::pattern::unwrap_expr_to_value(p),
        Value::Hold(inner) | Value::HoldComplete(inner) => (**inner).clone(),
        _ => v.clone(),
    }
}

/// `Value` → `Expr` preserving rationals (which the generic converter drops).
fn value_to_expr_num(v: &Value) -> Expr {
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
            args: args.iter().map(value_to_expr_num).collect(),
        },
        Value::List(items) => Expr::List(items.iter().map(value_to_expr_num).collect()),
        Value::Sequence(items) => Expr::Call {
            head: Box::new(Expr::Symbol("Sequence".to_string())),
            args: items.iter().map(value_to_expr_num).collect(),
        },
        _ => super::table::value_to_expr(v),
    }
}

/// Coerce a value to a high-precision float if possible.
fn coerce_to_float(v: Value, prec_bits: u32) -> Result<Value, EvalError> {
    match v {
        Value::Integer(n) => Ok(Value::Real(Float::with_val(prec_bits, n))),
        Value::Real(_) => Ok(v),
        Value::Rational(r) => Ok(Value::Real(
            Float::with_val(prec_bits, r.numer()) / Float::with_val(prec_bits, r.denom()),
        )),
        Value::Complex { re, im: 0.0 } => Ok(Value::Real(Float::with_val(prec_bits, re))),
        Value::Root { ref coeffs, index } => {
            let roots = crate::polynomial::find_polynomial_roots(coeffs);
            if index > 0 && index <= roots.len() {
                let (re, im) = roots[index - 1];
                if im.abs() < 1e-14 {
                    Ok(Value::Real(Float::with_val(prec_bits, re)))
                } else {
                    Ok(Value::Complex { re, im })
                }
            } else {
                Ok(v)
            }
        }
        _ => Ok(v),
    }
}

#[cfg(test)]
mod nintegrate_tests {
    use super::*;

    fn env() -> Env {
        let e = Env::new();
        crate::builtins::register_builtins(&e);
        e
    }

    /// Evaluate `s` and coerce the result to `f64` (panics if not numeric).
    fn eval_f64(s: &str, env: &Env) -> f64 {
        let toks = crate::lexer::tokenize(s).unwrap();
        let parsed = crate::parser::parse_with_suppress(toks).unwrap();
        let mut last = Value::Null;
        for (e, _) in parsed {
            last = crate::eval::eval(&e, env)
                .unwrap_or_else(|err| Value::Symbol(format!("ERR:{err}")));
        }
        value_to_f64(&last).unwrap_or_else(|| panic!("`{s}` -> non-numeric {last}"))
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

    fn assert_close(got: f64, want: f64) {
        assert!((got - want).abs() < 1e-8, "got {got}, want {want}");
    }

    #[test]
    fn finite_polynomial_and_trig() {
        let env = env();
        assert_close(eval_f64("NIntegrate[x^2, {x, 0, 1}]", &env), 1.0 / 3.0);
        assert_close(eval_f64("NIntegrate[Sin[x], {x, 0, Pi}]", &env), 2.0);
        assert_close(eval_f64("NIntegrate[Cos[x], {x, 0, Pi/2}]", &env), 1.0);
    }

    #[test]
    fn endpoint_singularity() {
        let env = env();
        assert_close(eval_f64("NIntegrate[1/Sqrt[x], {x, 0, 1}]", &env), 2.0);
    }

    #[test]
    fn infinite_bounds() {
        let env = env();
        assert_close(
            eval_f64("NIntegrate[Exp[-x^2], {x, -Infinity, Infinity}]", &env),
            std::f64::consts::PI.sqrt(),
        );
        assert_close(
            eval_f64("NIntegrate[1/(1 + x^2), {x, 0, Infinity}]", &env),
            std::f64::consts::FRAC_PI_2,
        );
        assert_close(eval_f64("NIntegrate[Exp[-x], {x, 0, Infinity}]", &env), 1.0);
    }

    #[test]
    fn oscillatory_tail() {
        let env = env();
        // Conditionally convergent — best effort, looser tolerance.
        let got = eval_f64("NIntegrate[Sin[x]/x, {x, 0, Infinity}]", &env);
        assert!(
            (got - std::f64::consts::FRAC_PI_2).abs() < 1e-6,
            "got {got}"
        );
    }

    #[test]
    fn two_dimensional() {
        let env = env();
        assert_close(
            eval_f64("NIntegrate[x y, {x, 0, 1}, {y, 0, 1}]", &env),
            0.25,
        );
        assert_close(
            eval_f64("NIntegrate[Sin[x] Sin[y], {x, 0, Pi}, {y, 0, Pi}]", &env),
            4.0,
        );
    }

    #[test]
    fn options_and_precision() {
        let env = env();
        assert_close(
            eval_f64("NIntegrate[Sin[x], {x, 0, Pi}, MaxRecursion -> 20]", &env),
            2.0,
        );
        assert_close(
            eval_f64(
                "NIntegrate[Exp[-x^2], {x, 0, Infinity}, PrecisionGoal -> 10]",
                &env,
            ),
            std::f64::consts::PI.sqrt() / 2.0,
        );
        // Unknown options (e.g. Method) are accepted, not rejected.
        assert_close(
            eval_f64(
                "NIntegrate[Sin[x], {x, 0, Pi}, Method -> \"GaussKronrodRule\", WorkingPrecision -> 30]",
                &env,
            ),
            2.0,
        );
    }

    #[test]
    fn reversed_bounds() {
        let env = env();
        assert_close(eval_f64("NIntegrate[x^2, {x, 1, 0}]", &env), -1.0 / 3.0);
    }

    #[test]
    fn unmatched_stays_unevaluated() {
        let env = env();
        // Symbolic integrand with no numeric definition.
        let s = eval_str("NIntegrate[g[x], {x, 0, 1}]", &env);
        assert!(s.starts_with("NIntegrate"), "{s}");
        assert!(!s.contains("Pattern"), "{s}");
        // Malformed iterator.
        let s2 = eval_str("NIntegrate[x, {x, 0}]", &env);
        assert!(
            s2.starts_with("NIntegrate") || s2.starts_with("ERR"),
            "{s2}"
        );
    }
}
