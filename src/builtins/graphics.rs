//! Graphics package builtins.
//!
//! Provides SVG-based 2D graphics: ListPlot, ListLinePlot,
//! ExportGraphics, and the core SVG renderer.
//!
//! `Plot` requires evaluator access (to evaluate functions at sample
//! points) and is handled in eval.rs.

use crate::value::{EvalError, Value};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn as_list(v: &Value) -> Result<&Vec<Value>, EvalError> {
    match v {
        Value::List(items) => Ok(items),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: v.type_name().to_string(),
        }),
    }
}

fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        _ => None,
    }
}

// ── SVG Renderer ────────────────────────────────────────────────────────────

/// Parsed graphics options: (width, height, plot_range, show_axes).
type GraphicsOptions = (f64, f64, Option<((f64, f64), (f64, f64))>, bool);

/// Default image dimensions.
const DEFAULT_WIDTH: f64 = 600.0;
const DEFAULT_HEIGHT: f64 = 400.0;
const MARGIN: f64 = 50.0;

/// Render a list of graphics primitives to an SVG string.
///
/// Primitives supported:
/// - `Line[{{x1,y1},{x2,y2},...}]`
/// - `Point[{x,y}]`
/// - `Circle[{cx,cy}, r]`
/// - `Rectangle[{x1,y1}, {x2,y2}]`
pub fn render_svg(primitives: &Value, options: &Value) -> Result<String, EvalError> {
    let prims = as_list(primitives)?;

    // Extract options
    let (width, height, plot_range, show_axes) = parse_options(options)?;

    // Compute data bounds for auto-ranging
    let (mut x_min, mut x_max, mut y_min, mut y_max) =
        if let Some((xr, yr)) = &plot_range {
            (xr.0, xr.1, yr.0, yr.1)
        } else {
            compute_bounds(prims)?
        };

    // Add padding if auto-ranged
    if plot_range.is_none() {
        let x_pad = (x_max - x_min).abs() * 0.05;
        let y_pad = (y_max - y_min).abs() * 0.05;
        if x_pad > 0.0 {
            x_min -= x_pad;
            x_max += x_pad;
        }
        if y_pad > 0.0 {
            y_min -= y_pad;
            y_max += y_pad;
        }
        // Handle degenerate cases
        if (x_max - x_min).abs() < 1e-15 {
            x_min -= 1.0;
            x_max += 1.0;
        }
        if (y_max - y_min).abs() < 1e-15 {
            y_min -= 1.0;
            y_max += 1.0;
        }
    }

    let plot_w = width - 2.0 * MARGIN;
    let plot_h = height - 2.0 * MARGIN;

    // Coordinate transform: math → SVG
    let tx = |x: f64| MARGIN + (x - x_min) / (x_max - x_min) * plot_w;
    let ty = |y: f64| MARGIN + (1.0 - (y - y_min) / (y_max - y_min)) * plot_h;

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" \
         style=\"background:white\">\n",
        width as i32, height as i32
    );

    // Draw axes
    if show_axes {
        svg.push_str(&render_axes(x_min, x_max, y_min, y_max, width, height, &tx, &ty));
    }

    // Draw primitives
    for prim in prims {
        svg.push_str(&render_primitive(prim, &tx, &ty)?);
    }

    svg.push_str("</svg>\n");
    Ok(svg)
}

fn parse_options(options: &Value) -> Result<GraphicsOptions, EvalError> {
    let mut width = DEFAULT_WIDTH;
    let mut height = DEFAULT_HEIGHT;
    let mut plot_range: Option<((f64, f64), (f64, f64))> = None;
    let mut show_axes = true;

    if let Value::Assoc(map) = options {
        if let Some(v) = map.get("ImageSize")
            && let Value::List(size) = v
                && size.len() >= 2 {
                    width = to_f64(&size[0]).unwrap_or(DEFAULT_WIDTH);
                    height = to_f64(&size[1]).unwrap_or(DEFAULT_HEIGHT);
                }
        if let Some(v) = map.get("Axes") {
            show_axes = match v {
                Value::Bool(b) => *b,
                Value::Symbol(s) => s == "True",
                _ => true,
            };
        }
        if let Some(v) = map.get("PlotRange")
            && let Value::List(range) = v
                && range.len() >= 2
                    && let (Value::List(xr), Value::List(yr)) = (&range[0], &range[1])
                        && xr.len() >= 2 && yr.len() >= 2 {
                            plot_range = Some((
                                (
                                    to_f64(&xr[0]).unwrap_or(-1.0),
                                    to_f64(&xr[1]).unwrap_or(1.0),
                                ),
                                (
                                    to_f64(&yr[0]).unwrap_or(-1.0),
                                    to_f64(&yr[1]).unwrap_or(1.0),
                                ),
                            ));
                        }
    }

    Ok((width, height, plot_range, show_axes))
}

fn compute_bounds(
    prims: &[Value],
) -> Result<(f64, f64, f64, f64), EvalError> {
    let mut x_min = f64::INFINITY;
    let mut x_max = f64::NEG_INFINITY;
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;

    for prim in prims {
        match prim {
            Value::Call { head, args } if head == "Line" => {
                if let Some(pts) = args.first() {
                    expand_bounds_from_points(pts, &mut x_min, &mut x_max, &mut y_min, &mut y_max)?;
                }
            }
            Value::Call { head, args } if head == "Point" => {
                if let Some(pt) = args.first() {
                    expand_bounds_from_point(pt, &mut x_min, &mut x_max, &mut y_min, &mut y_max)?;
                }
            }
            Value::Call { head, args } if head == "Circle" => {
                if let Some(center) = args.first() {
                    let r = if args.len() > 1 {
                        to_f64(&args[1]).unwrap_or(1.0)
                    } else {
                        1.0
                    };
                    if let Value::List(c) = center
                        && c.len() >= 2 {
                            let cx = to_f64(&c[0]).unwrap_or(0.0);
                            let cy = to_f64(&c[1]).unwrap_or(0.0);
                            x_min = x_min.min(cx - r);
                            x_max = x_max.max(cx + r);
                            y_min = y_min.min(cy - r);
                            y_max = y_max.max(cy + r);
                        }
                }
            }
            Value::Call { head, args } if head == "Rectangle"
                && args.len() >= 2 => {
                    expand_bounds_from_point(&args[0], &mut x_min, &mut x_max, &mut y_min, &mut y_max)?;
                    expand_bounds_from_point(&args[1], &mut x_min, &mut x_max, &mut y_min, &mut y_max)?;
                }
            Value::List(items) => {
                // Could be a flat list of points
                for item in items {
                    expand_bounds_from_point(item, &mut x_min, &mut x_max, &mut y_min, &mut y_max)?;
                }
            }
            _ => {}
        }
    }

    // Fallback if no points found
    if x_min == f64::INFINITY {
        return Ok((-1.0, 1.0, -1.0, 1.0));
    }

    Ok((x_min, x_max, y_min, y_max))
}

fn expand_bounds_from_points(
    pts_val: &Value,
    x_min: &mut f64,
    x_max: &mut f64,
    y_min: &mut f64,
    y_max: &mut f64,
) -> Result<(), EvalError> {
    if let Value::List(pts) = pts_val {
        for pt in pts {
            expand_bounds_from_point(pt, x_min, x_max, y_min, y_max)?;
        }
    }
    Ok(())
}

fn expand_bounds_from_point(
    pt: &Value,
    x_min: &mut f64,
    x_max: &mut f64,
    y_min: &mut f64,
    y_max: &mut f64,
) -> Result<(), EvalError> {
    if let Value::List(coords) = pt
        && coords.len() >= 2 {
            let x = to_f64(&coords[0]).unwrap_or(0.0);
            let y = to_f64(&coords[1]).unwrap_or(0.0);
            *x_min = x_min.min(x);
            *x_max = x_max.max(x);
            *y_min = y_min.min(y);
            *y_max = y_max.max(y);
        }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_axes(
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    width: f64,
    height: f64,
    tx: &dyn Fn(f64) -> f64,
    ty: &dyn Fn(f64) -> f64,
) -> String {
    let mut s = String::new();
    let color = "#888";
    let stroke = "stroke-width=\"1\"";

    // X-axis (y=0)
    if y_min <= 0.0 && y_max >= 0.0 {
        let y_svg = ty(0.0);
        s.push_str(&format!(
            "<line x1=\"{}\" y1=\"{:.1}\" x2=\"{}\" y2=\"{:.1}\" stroke=\"{}\" {}/>\n",
            MARGIN as i32, y_svg, (width - MARGIN) as i32, y_svg, color, stroke
        ));
    }

    // Y-axis (x=0)
    if x_min <= 0.0 && x_max >= 0.0 {
        let x_svg = tx(0.0);
        s.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{}\" x2=\"{:.1}\" y2=\"{}\" stroke=\"{}\" {}/>\n",
            x_svg, MARGIN as i32, x_svg, (height - MARGIN) as i32, color, stroke
        ));
    }

    // Tick marks on X-axis
    let x_step = nice_step(x_max - x_min);
    let y_axis_y = if y_min <= 0.0 && y_max >= 0.0 {
        ty(0.0)
    } else {
        height - MARGIN
    };
    let mut x_tick = (x_min / x_step).ceil() * x_step;
    while x_tick <= x_max {
        let sx = tx(x_tick);
        s.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" {}/>\n",
            sx,
            y_axis_y - 4.0,
            sx,
            y_axis_y + 4.0,
            color,
            stroke
        ));
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
             font-size=\"10\" fill=\"{}\">{:.4}</text>\n",
            sx,
            y_axis_y + 16.0,
            color,
            x_tick
        ));
        x_tick += x_step;
    }

    // Tick marks on Y-axis
    let y_step = nice_step(y_max - y_min);
    let x_axis_x = if x_min <= 0.0 && x_max >= 0.0 {
        tx(0.0)
    } else {
        MARGIN
    };
    let mut y_tick = (y_min / y_step).ceil() * y_step;
    while y_tick <= y_max {
        let sy = ty(y_tick);
        s.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" {}/>\n",
            x_axis_x - 4.0,
            sy,
            x_axis_x + 4.0,
            sy,
            color,
            stroke
        ));
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" \
             font-size=\"10\" fill=\"{}\" dominant-baseline=\"middle\">{:.4}</text>\n",
            x_axis_x - 8.0,
            sy,
            color,
            y_tick
        ));
        y_tick += y_step;
    }

    // Border
    s.push_str(&format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
         fill=\"none\" stroke=\"{}\" {}/>\n",
        MARGIN as i32,
        MARGIN as i32,
        (width - 2.0 * MARGIN) as i32,
        (height - 2.0 * MARGIN) as i32,
        color,
        stroke
    ));

    s
}

/// Compute a "nice" step size for axis ticks.
fn nice_step(range: f64) -> f64 {
    let rough = range / 6.0;
    let exp = rough.log10().floor();
    let base = 10.0_f64.powf(exp);
    let frac = rough / base;
    let nice = if frac < 1.5 {
        1.0
    } else if frac < 3.5 {
        2.0
    } else if frac < 7.5 {
        5.0
    } else {
        10.0
    };
    nice * base
}

fn render_primitive(
    prim: &Value,
    tx: &dyn Fn(f64) -> f64,
    ty: &dyn Fn(f64) -> f64,
) -> Result<String, EvalError> {
    match prim {
        Value::Call { head, args } => match head.as_str() {
            "Line" => {
                if let Some(pts_val) = args.first() {
                    render_line(pts_val, tx, ty)
                } else {
                    Ok(String::new())
                }
            }
            "Point" => {
                if let Some(pt) = args.first() {
                    render_point(pt, tx, ty)
                } else {
                    Ok(String::new())
                }
            }
            "Circle" => {
                let center = args.first();
                let r = if args.len() > 1 {
                    to_f64(&args[1]).unwrap_or(1.0)
                } else {
                    1.0
                };
                if let Some(c) = center {
                    render_circle(c, r, tx, ty)
                } else {
                    Ok(String::new())
                }
            }
            "Rectangle" => {
                if args.len() >= 2 {
                    render_rectangle(&args[0], &args[1], tx, ty)
                } else {
                    Ok(String::new())
                }
            }
            _ => Ok(String::new()),
        },
        // Handle bare lists of points (e.g., from ListPlot)
        Value::List(items) if !items.is_empty() => {
            if let Value::List(coords) = &items[0]
                && coords.len() >= 2 {
                    // It's a list of points
                    let mut s = String::new();
                    for pt in items {
                        s.push_str(&render_point(pt, tx, ty)?);
                    }
                    return Ok(s);
                }
            Ok(String::new())
        }
        _ => Ok(String::new()),
    }
}

fn render_line(
    pts_val: &Value,
    tx: &dyn Fn(f64) -> f64,
    ty: &dyn Fn(f64) -> f64,
) -> Result<String, EvalError> {
    let pts = as_list(pts_val)?;
    if pts.is_empty() {
        return Ok(String::new());
    }
    let mut points_str = String::new();
    for pt in pts {
        if let Value::List(coords) = pt
            && coords.len() >= 2 {
                let x = to_f64(&coords[0]).unwrap_or(0.0);
                let y = to_f64(&coords[1]).unwrap_or(0.0);
                if !points_str.is_empty() {
                    points_str.push(' ');
                }
                points_str.push_str(&format!("{:.2},{:.2}", tx(x), ty(y)));
            }
    }
    Ok(format!(
        "<polyline points=\"{}\" fill=\"none\" stroke=\"#1a73e8\" stroke-width=\"2\"/>\n",
        points_str
    ))
}

fn render_point(
    pt: &Value,
    tx: &dyn Fn(f64) -> f64,
    ty: &dyn Fn(f64) -> f64,
) -> Result<String, EvalError> {
    if let Value::List(coords) = pt
        && coords.len() >= 2 {
            let x = to_f64(&coords[0]).unwrap_or(0.0);
            let y = to_f64(&coords[1]).unwrap_or(0.0);
            return Ok(format!(
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"3\" fill=\"#1a73e8\"/>\n",
                tx(x),
                ty(y)
            ));
        }
    Ok(String::new())
}

fn render_circle(
    center: &Value,
    r: f64,
    tx: &dyn Fn(f64) -> f64,
    ty: &dyn Fn(f64) -> f64,
) -> Result<String, EvalError> {
    if let Value::List(coords) = center
        && coords.len() >= 2 {
            let cx = to_f64(&coords[0]).unwrap_or(0.0);
            let cy = to_f64(&coords[1]).unwrap_or(0.0);
            // Scale radius using the x scale factor
            let scale = tx(1.0) - tx(0.0);
            return Ok(format!(
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" \
                 fill=\"none\" stroke=\"#1a73e8\" stroke-width=\"2\"/>\n",
                tx(cx),
                ty(cy),
                r * scale.abs()
            ));
        }
    Ok(String::new())
}

fn render_rectangle(
    p1: &Value,
    p2: &Value,
    tx: &dyn Fn(f64) -> f64,
    ty: &dyn Fn(f64) -> f64,
) -> Result<String, EvalError> {
    let coords1 = as_list(p1)?;
    let coords2 = as_list(p2)?;
    if coords1.len() >= 2 && coords2.len() >= 2 {
        let x1 = to_f64(&coords1[0]).unwrap_or(0.0);
        let y1 = to_f64(&coords1[1]).unwrap_or(0.0);
        let x2 = to_f64(&coords2[0]).unwrap_or(0.0);
        let y2 = to_f64(&coords2[1]).unwrap_or(0.0);
        let sx = tx(x1).min(tx(x2));
        let sy = ty(y1).min(ty(y2));
        let w = (tx(x2) - tx(x1)).abs();
        let h = (ty(y2) - ty(y1)).abs();
        return Ok(format!(
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" \
             fill=\"none\" stroke=\"#1a73e8\" stroke-width=\"2\"/>\n",
            sx, sy, w, h
        ));
    }
    Ok(String::new())
}

// ── Builtins ────────────────────────────────────────────────────────────────

/// ListPlot[data] — scatter plot from {x,y} pairs, returns SVG string.
pub fn builtin_list_plot(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "ListPlot requires 1 or 2 arguments".to_string(),
        ));
    }
    let data = as_list(&args[0])?;
    let options = if args.len() > 1 {
        args[1].clone()
    } else {
        Value::Assoc(std::collections::HashMap::new())
    };

    // Convert data to Point primitives
    let points: Vec<Value> = data
        .iter()
        .map(|pt| Value::Call {
            head: "Point".to_string(),
            args: vec![pt.clone()],
        })
        .collect();

    let svg = render_svg(&Value::List(points), &options)?;
    Ok(Value::Str(svg))
}

/// ListLinePlot[data] — line plot from {x,y} pairs, returns SVG string.
pub fn builtin_list_line_plot(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "ListLinePlot requires 1 or 2 arguments".to_string(),
        ));
    }
    let data = as_list(&args[0])?;
    let options = if args.len() > 1 {
        args[1].clone()
    } else {
        Value::Assoc(std::collections::HashMap::new())
    };

    // Wrap as a Line primitive
    let line = Value::Call {
        head: "Line".to_string(),
        args: vec![Value::List(data.clone())],
    };

    let svg = render_svg(&Value::List(vec![line]), &options)?;
    Ok(Value::Str(svg))
}

/// ExportGraphics[path, svg_string] — write SVG to file.
pub fn builtin_export_graphics(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ExportGraphics requires exactly 2 arguments".to_string(),
        ));
    }
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let svg = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String (SVG content)".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    std::fs::write(&path, &svg).map_err(|e| {
        EvalError::Error(format!("ExportGraphics: failed to write '{}': {}", path, e))
    })?;
    Ok(Value::Str(path))
}

/// Graphics[primitives, options] — wrap primitives into a Graphics value.
/// This is a simple constructor; rendering is done by ExportGraphics or Show.
pub fn builtin_graphics(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Graphics requires 1 or 2 arguments".to_string(),
        ));
    }
    let prims = args[0].clone();
    let opts = if args.len() > 1 {
        args[1].clone()
    } else {
        Value::Assoc(std::collections::HashMap::new())
    };
    Ok(Value::Call {
        head: "Graphics".to_string(),
        args: vec![prims, opts],
    })
}

// ── Registration ────────────────────────────────────────────────────────────

/// Stub for `Plot` — the real implementation is in eval.rs (requires Env access).
pub fn builtin_plot_stub(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "Plot should be handled by evaluator".to_string(),
    ))
}

/// Register all Graphics builtins in the environment.
pub fn register(env: &crate::env::Env) {
    use super::register_builtin;
    register_builtin(env, "ListPlot", builtin_list_plot);
    register_builtin(env, "ListLinePlot", builtin_list_line_plot);
    register_builtin(env, "ExportGraphics", builtin_export_graphics);
    register_builtin(env, "Graphics", builtin_graphics);
    // Plot is handled in eval.rs (needs evaluator access)
}

/// Symbol names exported by the Graphics package.
pub const SYMBOLS: &[&str] = &[
    "ListPlot", "ListLinePlot", "ExportGraphics", "Graphics",
    // Evaluator-dependent (handled in eval.rs):
    "Plot",
    // Syma-side wrappers (loaded from .syma file):
    "Show", "GraphicsGrid",
    "Line", "Point", "Circle", "Rectangle",
    "RGBColor", "Hue", "Thickness", "PointSize", "Opacity", "Directive",
];

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Float;
    use rug::Integer;

    fn real_val(v: f64) -> Value {
        Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, v))
    }

    fn list(vals: Vec<Value>) -> Value {
        Value::List(vals)
    }

    fn point(x: f64, y: f64) -> Value {
        list(vec![real_val(x), real_val(y)])
    }

    #[test]
    fn test_list_plot() {
        let data = list(vec![
            point(0.0, 0.0),
            point(1.0, 1.0),
            point(2.0, 4.0),
        ]);
        let result = builtin_list_plot(&[data]).unwrap();
        if let Value::Str(svg) = result {
            assert!(svg.contains("<svg"));
            assert!(svg.contains("<circle"));
            assert!(svg.contains("</svg>"));
        } else {
            panic!("Expected String");
        }
    }

    #[test]
    fn test_list_line_plot() {
        let data = list(vec![
            point(0.0, 0.0),
            point(1.0, 1.0),
            point(2.0, 4.0),
        ]);
        let result = builtin_list_line_plot(&[data]).unwrap();
        if let Value::Str(svg) = result {
            assert!(svg.contains("<svg"));
            assert!(svg.contains("<polyline"));
            assert!(svg.contains("</svg>"));
        } else {
            panic!("Expected String");
        }
    }

    #[test]
    fn test_render_svg_line() {
        let line = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(0.0, 0.0), point(1.0, 1.0)])],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![line]), &opts).unwrap();
        assert!(svg.contains("<polyline"));
        assert!(svg.contains("stroke"));
    }

    #[test]
    fn test_render_svg_point() {
        let pt = Value::Call {
            head: "Point".to_string(),
            args: vec![point(5.0, 3.0)],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![pt]), &opts).unwrap();
        assert!(svg.contains("<circle"));
    }

    #[test]
    fn test_render_svg_empty() {
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![]), &opts).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_nice_step() {
        assert!((nice_step(10.0) - 2.0).abs() < 1e-10);
        assert!((nice_step(1.0) - 0.2).abs() < 1e-10);
        assert!((nice_step(100.0) - 20.0).abs() < 1e-10);
    }

    #[test]
    fn test_export_graphics() {
        use std::fs;
        let path = "/tmp/test_syma_graphics.svg";
        let svg_content = "<svg xmlns=\"http://www.w3.org/2000/svg\"><circle cx=\"10\" cy=\"10\" r=\"5\"/></svg>";
        let result = builtin_export_graphics(&[
            Value::Str(path.to_string()),
            Value::Str(svg_content.to_string()),
        ]);
        assert!(result.is_ok());
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("<svg"));
        fs::remove_file(path).ok();
    }
}
