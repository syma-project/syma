use crate::env::Env;
use crate::eval::apply_function;
use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;

fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        Value::Rational(r) => Some(r.to_f64()),
        Value::Complex { re, im: 0.0 } => Some(*re),
        _ => None,
    }
}

fn real(v: f64) -> Value {
    Value::Real(Float::with_val(DEFAULT_PRECISION, v))
}

fn substitute_numeric(expr: &Value, var: &str, val: f64) -> Value {
    match expr {
        Value::Symbol(s) if s == var => real(val),
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
            let evaluated =
                apply_function(&Value::Symbol("Simplify".to_string()), &[expr.clone()], env)
                    .unwrap_or(expr.clone());
            to_f64(&evaluated).ok_or_else(|| {
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
        m_flag = if fb > fd { true } else { false };

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

fn golden_section<F: Fn(f64) -> f64>(f: F, a: f64, b: f64) -> (f64, f64) {
    let phi = (1.0 + 5.0f64.sqrt()) / 2.0;
    let tol = 1e-12;
    let mut a = a;
    let mut b = b;

    loop {
        if (b - a).abs() < tol {
            break;
        }
        let c = b - (b - a) / phi;
        let d = a + (b - a) / phi;
        if f(c) < f(d) {
            b = d;
        } else {
            a = c;
        }
    }

    let x_opt = (a + b) / 2.0;
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
            let (val, x) = golden_section(&f, lo, hi);
            if val < best_val {
                best_val = val;
                best_x = x;
            }
        }
    }

    let (val, x) = golden_section(&f, xmin, xmax);
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
            let rhs = to_f64(&eq_args[1]).ok_or_else(|| {
                EvalError::Error("Right-hand side of equation must be a number".to_string())
            })?;
            Ok((eq_args[0].clone(), rhs))
        }
        _ => Ok((arg.clone(), 0.0)),
    }
}

fn extract_var_symbol(val: &Value) -> Result<String, EvalError> {
    match val {
        Value::Symbol(s) => Ok(s.clone()),
        _ => Err(EvalError::Error("Variable must be a symbol".to_string())),
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

    let (expr, rhs) = parse_equation(&args[0])?;
    let search_spec = match &args[1] {
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

    let var = extract_var_symbol(&search_spec[0])?;

    if search_spec.len() == 2 {
        let x0 = to_f64(&search_spec[1]).ok_or_else(|| EvalError::TypeError {
            expected: "Number".to_string(),
            got: search_spec[1].type_name().to_string(),
        })?;
        let var_clone = var.clone();
        let f = move |x: f64| {
            let substituted = substitute_numeric(&expr, &var_clone, x);
            eval_expr_to_f64(&substituted, env).unwrap_or(0.0) - rhs
        };
        let root = newton_method(&f, x0)?;
        Ok(Value::Rule {
            lhs: Box::new(Value::Symbol(var)),
            rhs: Box::new(real(root)),
            delayed: false,
        })
    } else {
        let xmin = to_f64(&search_spec[1]).ok_or_else(|| EvalError::TypeError {
            expected: "Number".to_string(),
            got: search_spec[1].type_name().to_string(),
        })?;
        let xmax = to_f64(&search_spec[2]).ok_or_else(|| EvalError::TypeError {
            expected: "Number".to_string(),
            got: search_spec[2].type_name().to_string(),
        })?;
        let var_clone = var.clone();
        let f = move |x: f64| {
            let substituted = substitute_numeric(&expr, &var_clone, x);
            eval_expr_to_f64(&substituted, env).unwrap_or(0.0) - rhs
        };
        let root = brent_method(&f, xmin, xmax)?;
        Ok(Value::Rule {
            lhs: Box::new(Value::Symbol(var)),
            rhs: Box::new(real(root)),
            delayed: false,
        })
    }
}

/// FindMinimum[f, {x, xmin, xmax}] — find local minimum via golden section search.
pub fn builtin_find_minimum(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FindMinimum requires exactly 2 arguments".to_string(),
        ));
    }

    let spec = match &args[1] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Second argument must be a list: {var, xmin, xmax}".to_string(),
            ));
        }
    };

    if spec.len() != 3 {
        return Err(EvalError::Error(
            "Search specification must have 3 elements: {var, xmin, xmax}".to_string(),
        ));
    }

    let var = extract_var_symbol(&spec[0])?;
    let xmin = to_f64(&spec[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[1].type_name().to_string(),
    })?;
    let xmax = to_f64(&spec[2]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[2].type_name().to_string(),
    })?;

    let expr = args[0].clone();
    let var_clone = var.clone();
    let f = move |x: f64| {
        let substituted = substitute_numeric(&expr, &var_clone, x);
        eval_expr_to_f64(&substituted, env).unwrap_or(f64::INFINITY)
    };

    let (f_min, x_min) = golden_section(&f, xmin, xmax);

    Ok(Value::List(vec![
        real(f_min),
        Value::Rule {
            lhs: Box::new(Value::Symbol(var)),
            rhs: Box::new(real(x_min)),
            delayed: false,
        },
    ]))
}

/// FindMaximum[f, {x, xmin, xmax}] — find local maximum.
pub fn builtin_find_maximum(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FindMaximum requires exactly 2 arguments".to_string(),
        ));
    }

    let spec = match &args[1] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Second argument must be a list: {var, xmin, xmax}".to_string(),
            ));
        }
    };

    if spec.len() != 3 {
        return Err(EvalError::Error(
            "Search specification must have 3 elements: {var, xmin, xmax}".to_string(),
        ));
    }

    let var = extract_var_symbol(&spec[0])?;
    let xmin = to_f64(&spec[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[1].type_name().to_string(),
    })?;
    let xmax = to_f64(&spec[2]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[2].type_name().to_string(),
    })?;

    let expr = args[0].clone();
    let var_clone = var.clone();
    let f = move |x: f64| {
        let substituted = substitute_numeric(&expr, &var_clone, x);
        eval_expr_to_f64(&substituted, env).unwrap_or(f64::NEG_INFINITY)
    };

    let neg_f = move |x: f64| -f(x);
    let (neg_f_min, x_max) = golden_section(&neg_f, xmin, xmax);
    let f_max = -neg_f_min;

    Ok(Value::List(vec![
        real(f_max),
        Value::Rule {
            lhs: Box::new(Value::Symbol(var)),
            rhs: Box::new(real(x_max)),
            delayed: false,
        },
    ]))
}

/// NMinimize[f, {x, xmin, xmax}] — global numeric minimization.
pub fn builtin_nminimize(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "NMinimize requires exactly 2 arguments".to_string(),
        ));
    }

    let spec = match &args[1] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Second argument must be a list: {var, xmin, xmax}".to_string(),
            ));
        }
    };

    if spec.len() != 3 {
        return Err(EvalError::Error(
            "Search specification must have 3 elements: {var, xmin, xmax}".to_string(),
        ));
    }

    let var = extract_var_symbol(&spec[0])?;
    let xmin = to_f64(&spec[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[1].type_name().to_string(),
    })?;
    let xmax = to_f64(&spec[2]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[2].type_name().to_string(),
    })?;

    let expr = args[0].clone();
    let var_clone = var.clone();
    let f = move |x: f64| {
        let substituted = substitute_numeric(&expr, &var_clone, x);
        eval_expr_to_f64(&substituted, env).unwrap_or(f64::INFINITY)
    };

    let (f_min, x_min) = multi_restart_minimize(&f, xmin, xmax, 10);

    Ok(Value::List(vec![
        real(f_min),
        Value::Rule {
            lhs: Box::new(Value::Symbol(var)),
            rhs: Box::new(real(x_min)),
            delayed: false,
        },
    ]))
}

/// NMaximize[f, {x, xmin, xmax}] — global numeric maximization.
pub fn builtin_nmaximize(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "NMaximize requires exactly 2 arguments".to_string(),
        ));
    }

    let spec = match &args[1] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Second argument must be a list: {var, xmin, xmax}".to_string(),
            ));
        }
    };

    if spec.len() != 3 {
        return Err(EvalError::Error(
            "Search specification must have 3 elements: {var, xmin, xmax}".to_string(),
        ));
    }

    let var = extract_var_symbol(&spec[0])?;
    let xmin = to_f64(&spec[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[1].type_name().to_string(),
    })?;
    let xmax = to_f64(&spec[2]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[2].type_name().to_string(),
    })?;

    let expr = args[0].clone();
    let var_clone = var.clone();
    let f = move |x: f64| {
        let substituted = substitute_numeric(&expr, &var_clone, x);
        eval_expr_to_f64(&substituted, env).unwrap_or(f64::NEG_INFINITY)
    };

    let neg_f = move |x: f64| -f(x);
    let (neg_f_min, x_max) = multi_restart_minimize(&neg_f, xmin, xmax, 10);
    let f_max = -neg_f_min;

    Ok(Value::List(vec![
        real(f_max),
        Value::Rule {
            lhs: Box::new(Value::Symbol(var)),
            rhs: Box::new(real(x_max)),
            delayed: false,
        },
    ]))
}

/// ArgMin[f, {x, xmin, xmax}] — return just the minimizing x value.
pub fn builtin_argmin(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ArgMin requires exactly 2 arguments".to_string(),
        ));
    }

    let spec = match &args[1] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Second argument must be a list: {var, xmin, xmax}".to_string(),
            ));
        }
    };

    if spec.len() != 3 {
        return Err(EvalError::Error(
            "Search specification must have 3 elements: {var, xmin, xmax}".to_string(),
        ));
    }

    let var = extract_var_symbol(&spec[0])?;
    let xmin = to_f64(&spec[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[1].type_name().to_string(),
    })?;
    let xmax = to_f64(&spec[2]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[2].type_name().to_string(),
    })?;

    let expr = args[0].clone();
    let f = move |x: f64| {
        let substituted = substitute_numeric(&expr, &var, x);
        eval_expr_to_f64(&substituted, env).unwrap_or(f64::INFINITY)
    };

    let (_f_min, x_min) = multi_restart_minimize(&f, xmin, xmax, 10);

    Ok(real(x_min))
}

/// ArgMax[f, {x, xmin, xmax}] — return just the maximizing x value.
pub fn builtin_argmax(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ArgMax requires exactly 2 arguments".to_string(),
        ));
    }

    let spec = match &args[1] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Second argument must be a list: {var, xmin, xmax}".to_string(),
            ));
        }
    };

    if spec.len() != 3 {
        return Err(EvalError::Error(
            "Search specification must have 3 elements: {var, xmin, xmax}".to_string(),
        ));
    }

    let var = extract_var_symbol(&spec[0])?;
    let xmin = to_f64(&spec[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[1].type_name().to_string(),
    })?;
    let xmax = to_f64(&spec[2]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: spec[2].type_name().to_string(),
    })?;

    let expr = args[0].clone();
    let f = move |x: f64| {
        let substituted = substitute_numeric(&expr, &var, x);
        eval_expr_to_f64(&substituted, env).unwrap_or(f64::NEG_INFINITY)
    };

    let neg_f = move |x: f64| -f(x);
    let (_neg_f_min, x_max) = multi_restart_minimize(&neg_f, xmin, xmax, 10);

    Ok(real(x_max))
}

/// FindInstance[eqn, var, n] — find n numeric instances (solutions).
pub fn builtin_find_instance(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let (n_val, eqn_arg, var_arg) = if args.len() == 3 {
        (args[2].clone(), &args[0], &args[1])
    } else if args.len() == 2 {
        (Value::Integer(Integer::from(1)), &args[0], &args[1])
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

    let var = extract_var_symbol(var_arg)?;
    let var_clone = var.clone();
    let (expr, rhs) = parse_equation(eqn_arg)?;

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
                rhs: Box::new(real(root)),
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

    let var = extract_var_symbol(&args[1])?;
    let (expr, rhs) = parse_equation(&args[0])?;

    let poly_expr = if rhs == 0.0 {
        expr.clone()
    } else {
        Value::Call {
            head: "Plus".to_string(),
            args: vec![
                expr.clone(),
                Value::Call {
                    head: "Times".to_string(),
                    args: vec![real(-rhs)],
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
            while coeffs_trimmed.len() > 1
                && coeffs_trimmed.last().map_or(true, |&c| c.abs() < 1e-15)
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
                    rhs: Box::new(real(root)),
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
                        rhs: Box::new(real(root)),
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
                if fa * fb <= 0.0 {
                    if let Ok(root) = brent_method(&f, a, b) {
                        let is_dup = roots.iter().any(|&r| (r - root).abs() < 1e-6);
                        if !is_dup {
                            roots.push(root);
                        }
                    }
                }
            }

            for i in 0..grid_size {
                let start =
                    search_lo + ((i as f64 + 0.5) / grid_size as f64) * (search_hi - search_lo);
                if let Ok(root) = newton_method(&f, start) {
                    if f(root).abs() < 1e-6 {
                        let is_dup = roots.iter().any(|&r| (r - root).abs() < 1e-6);
                        if !is_dup {
                            roots.push(root);
                        }
                    }
                }
            }

            roots.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let solutions: Vec<Value> = roots
                .into_iter()
                .map(|root| {
                    Value::List(vec![Value::Rule {
                        lhs: Box::new(Value::Symbol(var.clone())),
                        rhs: Box::new(real(root)),
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
            to_f64(expr).map(|c| vec![c])
        }
        Value::Symbol(s) if s == var => Some(vec![0.0, 1.0]),
        Value::Symbol(_) => None,
        Value::Call { head, args } => match head.as_str() {
            "Plus" => {
                let mut coeffs: Vec<f64> = vec![0.0; 1];
                for arg in args {
                    if let Some(sub) = extract_polynomial_coeffs(arg, var) {
                        if sub.len() > coeffs.len() {
                            coeffs.resize(sub.len(), 0.0);
                        }
                        for (i, &c) in sub.iter().enumerate() {
                            coeffs[i] += c;
                        }
                    } else {
                        return None;
                    }
                }
                while coeffs.len() > 1 && coeffs.last().map_or(false, |&c| c.abs() < 1e-15) {
                    coeffs.pop();
                }
                Some(coeffs)
            }
            "Times" => {
                let mut numeric_factor = 1.0;
                let mut poly_part: Option<Vec<f64>> = None;

                for arg in args {
                    if let Some(c) = to_f64(arg) {
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
                            if let Some(sub_coeffs) = extract_polynomial_coeffs(arg, var) {
                                if let Some(ref mut pp) = poly_part {
                                    let multiplied = multiply_polys(pp, &sub_coeffs);
                                    *pp = multiplied;
                                } else {
                                    poly_part = Some(sub_coeffs);
                                }
                            } else {
                                return None;
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
                _ => to_f64(&args[0]).map(|c| vec![c]),
            },
            "Divide" if args.len() == 2 => {
                let num_coeffs = extract_polynomial_coeffs(&args[0], var)?;
                let denom = to_f64(&args[1])?;
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

fn qr_eigenvalues(matrix: &mut Vec<Vec<f64>>, n: usize) -> Vec<(f64, f64)> {
    let mut a = matrix.clone();

    hessenberg(&mut a, n);

    let max_iter = 1000;
    let tol = 1e-12;
    let mut converged = vec![false; n];
    let mut eigenvalues: Vec<(f64, f64)> = Vec::new();
    let m = n;

    for _ in 0..max_iter {
        let mut split = m;
        for i in (1..m).rev() {
            if a[i - 1][i].abs() < tol && !converged[i] {
                split = i;
                converged[i] = true;
            }
        }

        if split == 0 {
            break;
        }

        if split == 1 {
            eigenvalues.push((a[0][0], 0.0));
            converged[0] = true;
        } else if split == 2 {
            let tr = a[0][0] + a[1][1];
            let det = a[0][0] * a[1][1] - a[0][1] * a[1][0];
            let disc = tr * tr - 4.0 * det;
            if disc >= 0.0 {
                let sqrt_disc = disc.sqrt();
                eigenvalues.push(((tr + sqrt_disc) / 2.0, 0.0));
                eigenvalues.push(((tr - sqrt_disc) / 2.0, 0.0));
            } else {
                let re = tr / 2.0;
                let im = (-disc).sqrt() / 2.0;
                eigenvalues.push((re, im));
                eigenvalues.push((re, -im));
            }
            for i in 0..split {
                converged[i] = true;
            }
        } else {
            let mut shift = a[split - 1][split - 1];
            if a[split - 2][split - 2].abs() > a[split - 1][split - 1].abs() {
                let tr = a[split - 2][split - 2] + a[split - 1][split - 1];
                let det = a[split - 2][split - 2] * a[split - 1][split - 1]
                    - a[split - 2][split - 1] * a[split - 1][split - 2];
                let disc = tr * tr - 4.0 * det;
                if disc >= 0.0 {
                    let sqrt_disc = disc.sqrt();
                    let r1 = (tr + sqrt_disc) / 2.0;
                    let r2 = (tr - sqrt_disc) / 2.0;
                    shift = if (r1 - a[split - 1][split - 1]).abs()
                        < (r2 - a[split - 1][split - 1]).abs()
                    {
                        r1
                    } else {
                        r2
                    };
                }
            }

            let mut q = identity_matrix(split);
            let mut r = a[..split]
                .iter()
                .map(|row| row[..split].to_vec())
                .collect::<Vec<_>>();

            for i in 0..split {
                r[i][i] -= shift;
            }

            for i in 0..split - 1 {
                let rr = (r[i][i] * r[i][i] + r[i + 1][i] * r[i + 1][i]).sqrt();
                let c = if rr.abs() < 1e-30 { 0.0 } else { r[i][i] / rr };
                let s = if rr.abs() < 1e-30 {
                    1.0
                } else {
                    -r[i + 1][i] / rr
                };

                for j in 0..split {
                    let rt = r[i][j] * c + r[i + 1][j] * s;
                    r[i + 1][j] = -r[i][j] * s + r[i + 1][j] * c;
                    r[i][j] = rt;
                }

                for j in 0..split {
                    let qt = q[j][i] * c + q[j][i + 1] * s;
                    q[j][i + 1] = -q[j][i] * s + q[j][i + 1] * c;
                    q[j][i] = qt;
                }
            }

            let mut rq = zero_matrix(split);
            for i in 0..split {
                for j in 0..split {
                    for k in 0..split {
                        rq[i][j] += r[i][k] * q[k][j];
                    }
                }
            }

            for i in 0..split {
                rq[i][i] += shift;
            }

            for i in 0..split {
                for j in 0..split {
                    a[i][j] = rq[i][j];
                }
            }
        }
    }

    for i in 0..n {
        if !converged[i] {
            eigenvalues.push((a[i][i], 0.0));
        }
    }

    eigenvalues
}

fn identity_matrix(n: usize) -> Vec<Vec<f64>> {
    let mut m = vec![vec![0.0; n]; n];
    for i in 0..n {
        m[i][i] = 1.0;
    }
    m
}

fn zero_matrix(n: usize) -> Vec<Vec<f64>> {
    vec![vec![0.0; n]; n]
}

fn hessenberg(a: &mut Vec<Vec<f64>>, n: usize) {
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
            let factor = dot / hh_norm_sq;
            for i in k + 1..n {
                a[i][j] -= col[i - k - 1] * factor;
            }
        }

        for i in 0..n {
            let mut dot_right = 0.0;
            for j in k + 1..n {
                dot_right += col[j - k - 1] * a[i][j];
            }
            let factor = dot_right / hh_norm_sq;
            for j in k + 1..n {
                a[i][j] -= col[j - k - 1] * factor;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(to_f64(&Value::Integer(Integer::from(42))), Some(42.0));
        assert_eq!(
            to_f64(&Value::Real(Float::with_val(DEFAULT_PRECISION, 3.14))),
            Some(3.14)
        );
        assert_eq!(to_f64(&Value::Symbol("x".to_string())), None);
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
    fn test_multiply_polys() {
        let a = vec![1.0, 1.0];
        let b = vec![1.0, 1.0];
        let result = multiply_polys(&a, &b);
        assert_eq!(result, vec![1.0, 2.0, 1.0]);
    }
}
