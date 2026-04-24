/// Plot — sample an expression and return a symbolic Graphics object.
use std::collections::HashMap;

use rug::Float;
use rug::Integer;

use crate::ast::*;
use crate::env::Env;
use crate::value::*;

/// Plot[f, {x, xmin, xmax}] — sample f and return a Graphics object.
///
/// The returned value is a symbolic `Graphics[primitives, options]` expression
/// that gets rendered to SVG only when exported via Export["file.svg", graphics].
pub(super) fn eval_plot(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Plot requires at least 2 arguments: f, {x, xmin, xmax}".to_string(),
        ));
    }
    let expr = &args[0];

    // Evaluate the iterator spec: {x, xmin, xmax}
    let iter_val = super::eval(&args[1], env)?;
    let iter_items = match &iter_val {
        Value::List(items) if items.len() == 3 || items.len() == 4 => items,
        _ => {
            return Err(EvalError::Error(
                "Plot iterator must be {x, xmin, xmax} or {x, xmin, xmax, step}".to_string(),
            ));
        }
    };

    let var_name = match &iter_items[0] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Symbol".to_string(),
                got: iter_items[0].type_name().to_string(),
            });
        }
    };

    let xmin = to_f64_val(&iter_items[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: iter_items[1].type_name().to_string(),
    })?;
    let xmax = to_f64_val(&iter_items[2]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: iter_items[2].type_name().to_string(),
    })?;

    if xmin >= xmax {
        return Err(EvalError::Error(
            "Plot: xmin must be less than xmax".to_string(),
        ));
    }

    // Evaluate remaining option args
    let plot_opts: Vec<Value> = args[2..]
        .iter()
        .map(|a| super::eval(a, env))
        .collect::<Result<_, _>>()?;

    // Sample the expression at n_points
    let n_points = 200usize;
    let step = (xmax - xmin) / (n_points - 1) as f64;
    let mut points = Vec::with_capacity(n_points);

    for i in 0..n_points {
        let x = if i == n_points - 1 {
            xmax
        } else {
            xmin + step * i as f64
        };
        let child_env = env.child();
        child_env.set(
            var_name.clone(),
            Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, x)),
        );
        match super::eval(expr, &child_env) {
            Ok(val) => {
                if let Some(y) = to_f64_val(&val)
                    && y.is_finite()
                {
                    points.push(Value::List(vec![
                        Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, x)),
                        Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, y)),
                    ]));
                }
            }
            Err(_) => continue,
        }
    }

    if points.is_empty() {
        return Err(EvalError::Error(
            "Plot: no valid points were sampled".to_string(),
        ));
    }

    // Build Line primitive
    let line = Value::Call {
        head: "Line".to_string(),
        args: vec![Value::List(points)],
    };
    let primitives = Value::List(vec![line]);

    // Build options: default ImageSize and Axes
    let mut opt_map = std::collections::HashMap::new();
    opt_map.insert(
        "ImageSize".to_string(),
        Value::List(vec![
            Value::Integer(Integer::from(400)),
            Value::Integer(Integer::from(300)),
        ]),
    );
    opt_map.insert("Axes".to_string(), Value::Bool(true));
    for opt in &plot_opts {
        if let Value::Rule { lhs, rhs, .. } = opt
            && let Value::Symbol(k) | Value::Str(k) = lhs.as_ref()
        {
            opt_map.insert(k.clone(), rhs.as_ref().clone());
        }
    }
    let options = Value::Assoc(opt_map);

    Ok(Value::Call {
        head: "Graphics".to_string(),
        args: vec![primitives, options],
    })
}

/// LogPlot[f, {x, xmin, xmax}] — logarithmic y-axis.
pub(super) fn eval_log_plot(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    log_plot_impl(args, env, false, true)
}

/// LogLogPlot[f, {x, xmin, xmax}] — logarithmic x and y axes.
pub(super) fn eval_log_log_plot(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    log_plot_impl(args, env, true, true)
}

/// LogLinearPlot[f, {x, xmin, xmax}] — logarithmic x-axis.
pub(super) fn eval_log_linear_plot(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    log_plot_impl(args, env, true, false)
}

fn log_plot_impl(
    args: &[Expr],
    env: &Env,
    log_x: bool,
    log_y: bool,
) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Log*Plot requires at least 2 arguments: f, {x, xmin, xmax}".to_string(),
        ));
    }
    let expr = &args[0];
    let iter_val = super::eval(&args[1], env)?;
    let (var_name, xmin_raw, xmax_raw) = parse_iter_spec_val(&iter_val)?;

    if xmin_raw >= xmax_raw {
        return Err(EvalError::Error(
            "Log*Plot: xmin must be less than xmax".to_string(),
        ));
    }
    if log_x && (xmin_raw <= 0.0 || xmax_raw <= 0.0) {
        return Err(EvalError::Error(
            "Log*Plot: x range must be positive for log x-axis".to_string(),
        ));
    }

    // Sample evenly in log space for log x-axis, linearly otherwise
    let (coord_min, coord_max) = if log_x {
        (xmin_raw.log10(), xmax_raw.log10())
    } else {
        (xmin_raw, xmax_raw)
    };

    let n_points = 200usize;
    let step = (coord_max - coord_min) / (n_points - 1) as f64;
    let mut points = Vec::with_capacity(n_points);

    for i in 0..n_points {
        let x_coord = if i == n_points - 1 {
            coord_max
        } else {
            coord_min + step * i as f64
        };
        let x_real = if log_x { 10.0_f64.powf(x_coord) } else { x_coord };

        let child_env = env.child();
        child_env.set(
            var_name.clone(),
            Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, x_real)),
        );

        match super::eval(expr, &child_env) {
            Ok(val) => {
                if let Some(y_raw) = to_f64_val(&val) {
                    let y_coord = if log_y {
                        if y_raw <= 0.0 {
                            continue;
                        }
                        y_raw.log10()
                    } else {
                        y_raw
                    };
                    if x_coord.is_finite() && y_coord.is_finite() {
                        points.push(Value::List(vec![make_real(x_coord), make_real(y_coord)]));
                    }
                }
            }
            Err(_) => continue,
        }
    }

    if points.is_empty() {
        return Err(EvalError::Error(
            "Log*Plot: no valid points were sampled".to_string(),
        ));
    }

    let x_scale = if log_x { "Log" } else { "Linear" };
    let y_scale = if log_y { "Log" } else { "Linear" };
    let mut extra = HashMap::new();
    extra.insert(
        "AxesScale".to_string(),
        Value::List(vec![
            Value::Str(x_scale.to_string()),
            Value::Str(y_scale.to_string()),
        ]),
    );

    build_line_graphics(points, &args[2..], env, extra)
}

/// ParametricPlot[{fx, fy}, {t, tmin, tmax}] — 2D parametric curve.
pub(super) fn eval_parametric_plot(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "ParametricPlot requires at least 2 arguments: {fx, fy}, {t, tmin, tmax}".to_string(),
        ));
    }

    let (fx_expr, fy_expr) = match &args[0] {
        Expr::List(items) if items.len() == 2 => (&items[0], &items[1]),
        _ => {
            return Err(EvalError::Error(
                "ParametricPlot: first argument must be {fx, fy}".to_string(),
            ));
        }
    };

    let iter_val = super::eval(&args[1], env)?;
    let (var_name, tmin, tmax) = parse_iter_spec_val(&iter_val)?;

    if tmin >= tmax {
        return Err(EvalError::Error(
            "ParametricPlot: tmin must be less than tmax".to_string(),
        ));
    }

    let n_points = 300usize;
    let step = (tmax - tmin) / (n_points - 1) as f64;
    let mut points = Vec::with_capacity(n_points);

    for i in 0..n_points {
        let t = if i == n_points - 1 {
            tmax
        } else {
            tmin + step * i as f64
        };
        let child_env = env.child();
        child_env.set(
            var_name.clone(),
            Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, t)),
        );

        let x_res = super::eval(fx_expr, &child_env)
            .ok()
            .and_then(|v| to_f64_val(&v));
        let y_res = super::eval(fy_expr, &child_env)
            .ok()
            .and_then(|v| to_f64_val(&v));

        if let (Some(x), Some(y)) = (x_res, y_res) && x.is_finite() && y.is_finite() {
            points.push(Value::List(vec![make_real(x), make_real(y)]));
        }
    }

    if points.is_empty() {
        return Err(EvalError::Error(
            "ParametricPlot: no valid points were sampled".to_string(),
        ));
    }

    build_line_graphics(points, &args[2..], env, HashMap::new())
}

/// PolarPlot[r, {θ, θmin, θmax}] — polar coordinate plot.
pub(super) fn eval_polar_plot(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "PolarPlot requires at least 2 arguments: r, {θ, θmin, θmax}".to_string(),
        ));
    }

    let expr = &args[0];
    let iter_val = super::eval(&args[1], env)?;
    let (var_name, theta_min, theta_max) = parse_iter_spec_val(&iter_val)?;

    let n_points = 360usize;
    let step = (theta_max - theta_min) / (n_points - 1) as f64;
    let mut points = Vec::with_capacity(n_points);

    for i in 0..n_points {
        let theta = if i == n_points - 1 {
            theta_max
        } else {
            theta_min + step * i as f64
        };
        let child_env = env.child();
        child_env.set(
            var_name.clone(),
            Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, theta)),
        );

        match super::eval(expr, &child_env) {
            Ok(val) => {
                if let Some(r) = to_f64_val(&val) {
                    let x = r * theta.cos();
                    let y = r * theta.sin();
                    if x.is_finite() && y.is_finite() {
                        points.push(Value::List(vec![make_real(x), make_real(y)]));
                    }
                }
            }
            Err(_) => continue,
        }
    }

    if points.is_empty() {
        return Err(EvalError::Error(
            "PolarPlot: no valid points were sampled".to_string(),
        ));
    }

    build_line_graphics(points, &args[2..], env, HashMap::new())
}

/// DiscretePlot[f, {n, nmin, nmax}] — stem plot for a discrete function.
pub(super) fn eval_discrete_plot(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "DiscretePlot requires at least 2 arguments: f, {n, nmin, nmax}".to_string(),
        ));
    }

    let expr = &args[0];
    let iter_val = super::eval(&args[1], env)?;
    let (var_name, n_min_f, n_max_f) = parse_iter_spec_val(&iter_val)?;

    let n_min = n_min_f.round() as i64;
    let n_max = n_max_f.round() as i64;

    if n_min > n_max {
        return Err(EvalError::Error(
            "DiscretePlot: nmin must be ≤ nmax".to_string(),
        ));
    }

    let mut primitives: Vec<Value> = Vec::new();

    for n in n_min..=n_max {
        let child_env = env.child();
        child_env.set(
            var_name.clone(),
            Value::Integer(Integer::from(n)),
        );

        match super::eval(expr, &child_env) {
            Ok(val) => {
                if let Some(y) = to_f64_val(&val).filter(|f| f.is_finite()) {
                    let x = n as f64;
                    // Vertical stem line from baseline (y=0) to value
                    let stem = Value::Call {
                        head: "Line".to_string(),
                        args: vec![Value::List(vec![
                            Value::List(vec![make_real(x), make_real(0.0)]),
                            Value::List(vec![make_real(x), make_real(y)]),
                        ])],
                    };
                    let dot = Value::Call {
                        head: "Point".to_string(),
                        args: vec![Value::List(vec![make_real(x), make_real(y)])],
                    };
                    primitives.push(stem);
                    primitives.push(dot);
                }
            }
            Err(_) => continue,
        }
    }

    if primitives.is_empty() {
        return Err(EvalError::Error(
            "DiscretePlot: no valid points were sampled".to_string(),
        ));
    }

    let mut opt_map = HashMap::new();
    opt_map.insert(
        "ImageSize".to_string(),
        Value::List(vec![make_int(400), make_int(300)]),
    );
    opt_map.insert("Axes".to_string(), Value::Bool(true));
    for a in &args[2..] {
        if let Ok(Value::Rule { lhs, rhs, .. }) = super::eval(a, env)
            && let Value::Symbol(k) | Value::Str(k) = lhs.as_ref()
        {
            opt_map.insert(k.clone(), rhs.as_ref().clone());
        }
    }

    Ok(Value::Call {
        head: "Graphics".to_string(),
        args: vec![Value::List(primitives), Value::Assoc(opt_map)],
    })
}

/// DensityPlot[f, {x, xmin, xmax}, {y, ymin, ymax}] — 2D density heatmap.
pub(super) fn eval_density_plot(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "DensityPlot requires at least 3 arguments: f, {x, xmin, xmax}, {y, ymin, ymax}"
                .to_string(),
        ));
    }

    let expr = &args[0];
    let x_spec = super::eval(&args[1], env)?;
    let y_spec = super::eval(&args[2], env)?;

    let (x_var, xmin, xmax) = parse_iter_spec_val(&x_spec)?;
    let (y_var, ymin, ymax) = parse_iter_spec_val(&y_spec)?;

    let nx = 50usize;
    let ny = 50usize;

    let mut val_min = f64::INFINITY;
    let mut val_max = f64::NEG_INFINITY;

    // Sample on nx×ny grid; grid[j][i] = f at (x_i, y_j)
    let mut grid: Vec<Vec<f64>> = Vec::with_capacity(ny);
    for j in 0..ny {
        let y_val = ymin + (ymax - ymin) * j as f64 / (ny - 1) as f64;
        let mut row = Vec::with_capacity(nx);
        for i in 0..nx {
            let x_val = xmin + (xmax - xmin) * i as f64 / (nx - 1) as f64;
            let child_env = env.child();
            child_env.set(x_var.clone(), make_real(x_val));
            child_env.set(y_var.clone(), make_real(y_val));
            let v = match super::eval(expr, &child_env) {
                Ok(val) => to_f64_val(&val).filter(|f| f.is_finite()).unwrap_or(0.0),
                Err(_) => 0.0,
            };
            val_min = val_min.min(v);
            val_max = val_max.max(v);
            row.push(v);
        }
        grid.push(row);
    }

    if !val_min.is_finite() {
        val_min = 0.0;
    }
    if !val_max.is_finite() || (val_max - val_min).abs() < 1e-15 {
        val_max = val_min + 1.0;
    }

    let flat_vals: Vec<Value> = grid.into_iter().flatten().map(make_real).collect();

    // DensityRaster[flat_vals, {xmin,xmax}, {ymin,ymax}, {nx,ny}, val_min, val_max]
    let raster = Value::Call {
        head: "DensityRaster".to_string(),
        args: vec![
            Value::List(flat_vals),
            Value::List(vec![make_real(xmin), make_real(xmax)]),
            Value::List(vec![make_real(ymin), make_real(ymax)]),
            Value::List(vec![
                Value::Integer(Integer::from(nx as i64)),
                Value::Integer(Integer::from(ny as i64)),
            ]),
            make_real(val_min),
            make_real(val_max),
        ],
    };

    let mut opt_map = HashMap::new();
    opt_map.insert(
        "ImageSize".to_string(),
        Value::List(vec![make_int(400), make_int(400)]),
    );
    opt_map.insert("Axes".to_string(), Value::Bool(true));
    for a in &args[3..] {
        if let Ok(Value::Rule { lhs, rhs, .. }) = super::eval(a, env)
            && let Value::Symbol(k) | Value::Str(k) = lhs.as_ref()
        {
            opt_map.insert(k.clone(), rhs.as_ref().clone());
        }
    }

    Ok(Value::Call {
        head: "Graphics".to_string(),
        args: vec![Value::List(vec![raster]), Value::Assoc(opt_map)],
    })
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn make_real(v: f64) -> Value {
    Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, v))
}

fn make_int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

fn parse_iter_spec_val(v: &Value) -> Result<(String, f64, f64), EvalError> {
    match v {
        Value::List(items) if items.len() >= 3 => {
            let name = match &items[0] {
                Value::Symbol(s) => s.clone(),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "Symbol".to_string(),
                        got: items[0].type_name().to_string(),
                    });
                }
            };
            let min = to_f64_val(&items[1]).ok_or_else(|| EvalError::TypeError {
                expected: "Number".to_string(),
                got: items[1].type_name().to_string(),
            })?;
            let max = to_f64_val(&items[2]).ok_or_else(|| EvalError::TypeError {
                expected: "Number".to_string(),
                got: items[2].type_name().to_string(),
            })?;
            Ok((name, min, max))
        }
        _ => Err(EvalError::Error(
            "Iterator spec must be {var, min, max}".to_string(),
        )),
    }
}

fn build_line_graphics(
    points: Vec<Value>,
    extra_opts: &[Expr],
    env: &Env,
    mut opt_map: HashMap<String, Value>,
) -> Result<Value, EvalError> {
    let line = Value::Call {
        head: "Line".to_string(),
        args: vec![Value::List(points)],
    };
    let primitives = Value::List(vec![line]);

    opt_map
        .entry("ImageSize".to_string())
        .or_insert_with(|| Value::List(vec![make_int(400), make_int(300)]));
    opt_map
        .entry("Axes".to_string())
        .or_insert_with(|| Value::Bool(true));

    for a in extra_opts {
        if let Ok(Value::Rule { lhs, rhs, .. }) = super::eval(a, env)
            && let Value::Symbol(k) | Value::Str(k) = lhs.as_ref()
        {
            opt_map.insert(k.clone(), rhs.as_ref().clone());
        }
    }

    Ok(Value::Call {
        head: "Graphics".to_string(),
        args: vec![primitives, Value::Assoc(opt_map)],
    })
}

/// Helper: convert a Value to f64 if possible.
fn to_f64_val(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        Value::Complex { re, im: 0.0 } => Some(*re),
        _ => None,
    }
}
