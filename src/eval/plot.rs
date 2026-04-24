/// Plot — sample an expression and return a symbolic Graphics object.
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

/// Helper: convert a Value to f64 if possible.
fn to_f64_val(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        Value::Complex { re, im: 0.0 } => Some(*re),
        _ => None,
    }
}
