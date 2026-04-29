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

// ── Graphics Style State ────────────────────────────────────────────────────

/// Accumulated rendering state from graphics directives.
///
/// Directives like `RGBColor`, `Thickness`, `PointSize` set fields on this
/// struct as the renderer walks through the primitives list. Each subsequent
/// primitive inherits the current style.
#[derive(Clone, Debug)]
pub struct GraphicsStyle {
    /// Current stroke color (SVG format, e.g. "#1a73e8").
    pub color: String,
    /// Stroke width in SVG units.
    pub stroke_width: f64,
    /// Point radius for `Point` primitives, in SVG units.
    pub point_size: f64,
    /// Dash pattern (if set, e.g. Some(vec![4.0, 2.0])).
    pub dash_array: Option<Vec<f64>>,
    /// Opacity in [0, 1].
    pub opacity: f64,
}

impl Default for GraphicsStyle {
    fn default() -> Self {
        Self {
            color: "#1a73e8".to_string(),
            stroke_width: 2.0,
            point_size: 3.0,
            dash_array: None,
            opacity: 1.0,
        }
    }
}

// ── Directive helpers ───────────────────────────────────────────────────────

/// Return true if `val` is a graphics directive rather than a geometry primitive.
fn is_directive(val: &Value) -> bool {
    match val {
        Value::Call { head, .. } => matches!(
            head.as_str(),
            "RGBColor"
                | "Hue"
                | "GrayLevel"
                | "Lighter"
                | "Darker"
                | "Blend"
                | "ColorNegate"
                | "Thickness"
                | "AbsoluteThickness"
                | "PointSize"
                | "AbsolutePointSize"
                | "Dashing"
                | "AbsoluteDashing"
                | "Opacity"
                | "Directive"
                | "EdgeForm"
                | "FaceForm"
                | "Thick"
                | "Thin"
        ),
        Value::Symbol(s) => matches!(
            s.as_str(),
            "Red"
                | "Green"
                | "Blue"
                | "Black"
                | "White"
                | "Gray"
                | "Yellow"
                | "Cyan"
                | "Magenta"
                | "Orange"
                | "Purple"
                | "Brown"
                | "Pink"
        ),
        _ => false,
    }
}

/// Convert an RGBColor Call value to an SVG hex color string.
fn color_call_to_svg(val: &Value) -> Option<String> {
    let (args, head) = match val {
        Value::Call { head, args } => (args, head.as_str()),
        _ => return None,
    };
    match head {
        "RGBColor" if args.len() >= 3 => {
            let r = super::to_f64(&args[0])?;
            let g = super::to_f64(&args[1])?;
            let b = super::to_f64(&args[2])?;
            let ri = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
            let gi = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
            let bi = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
            Some(format!("#{:02x}{:02x}{:02x}", ri, gi, bi))
        }
        "Hue" if !args.is_empty() => {
            let h = super::to_f64(&args[0])?;
            let s = if args.len() > 1 {
                super::to_f64(&args[1])?
            } else {
                1.0
            };
            let v = if args.len() > 2 {
                super::to_f64(&args[2])?
            } else {
                1.0
            };
            Some(hsv_to_svg(h, s, v))
        }
        _ => None,
    }
}

/// Convert a named colour symbol to an SVG hex string.
fn named_color_to_svg(name: &str) -> Option<&'static str> {
    match name {
        "Red" => Some("#dc2626"),
        "Green" => Some("#16a34a"),
        "Blue" => Some("#2563eb"),
        "Black" => Some("#000000"),
        "White" => Some("#ffffff"),
        "Gray" | "Grey" => Some("#888888"),
        "Yellow" => Some("#eab308"),
        "Cyan" => Some("#06b6d4"),
        "Magenta" => Some("#d946ef"),
        "Orange" => Some("#ea580c"),
        "Purple" => Some("#9333ea"),
        "Brown" => Some("#8b5cf6"),
        "Pink" => Some("#ec4899"),
        _ => None,
    }
}

/// Apply a directive to the current style.
fn apply_directive(style: &mut GraphicsStyle, directive: &Value) {
    match directive {
        Value::Call { head, args } => match head.as_str() {
            "RGBColor" | "Hue" => {
                if let Some(c) = color_call_to_svg(directive) {
                    style.color = c;
                }
            }
            "Thickness" if !args.is_empty() => {
                if let Some(t) = super::to_f64(&args[0]) {
                    style.stroke_width = t.max(0.0) * 10.0; // WL relative thickness
                }
            }
            "AbsoluteThickness" if !args.is_empty() => {
                if let Some(t) = super::to_f64(&args[0]) {
                    style.stroke_width = t.max(0.0);
                }
            }
            "PointSize" if !args.is_empty() => {
                if let Some(s) = super::to_f64(&args[0]) {
                    style.point_size = s.max(0.0) * 20.0; // relative
                }
            }
            "AbsolutePointSize" if !args.is_empty() => {
                if let Some(s) = super::to_f64(&args[0]) {
                    style.point_size = s.max(0.0);
                }
            }
            "Dashing" if !args.is_empty() => {
                if let Value::List(dashes) = &args[0] {
                    let v: Vec<f64> = dashes.iter().filter_map(super::to_f64).collect();
                    if !v.is_empty() {
                        style.dash_array = Some(v);
                    }
                }
            }
            "AbsoluteDashing" if !args.is_empty() => {
                if let Value::List(dashes) = &args[0] {
                    let v: Vec<f64> = dashes.iter().filter_map(super::to_f64).collect();
                    if !v.is_empty() {
                        style.dash_array = Some(v);
                    }
                }
            }
            "Opacity" if !args.is_empty() => {
                if let Some(o) = super::to_f64(&args[0]) {
                    style.opacity = o.clamp(0.0, 1.0);
                }
            }
            "Directive" => {
                for arg in args {
                    apply_directive(style, arg);
                }
            }
            "Thick" => style.stroke_width = 4.0,
            "Thin" => style.stroke_width = 0.5,
            // EdgeForm / FaceForm — ignored for now (all primitives use the current color)
            _ => {}
        },
        Value::Symbol(s) => {
            if let Some(c) = named_color_to_svg(s) {
                style.color = c.to_string();
            }
        }
        _ => {}
    }
}

/// Build the SVG attribute string for stroked primitives (Line, Circle, Rectangle).
fn svg_stroke_attrs(style: &GraphicsStyle) -> String {
    let mut a = format!(
        "stroke=\"{}\" stroke-width=\"{:.1}\"",
        style.color, style.stroke_width
    );
    if let Some(ref dashes) = style.dash_array {
        let d: Vec<String> = dashes.iter().map(|x| format!("{:.1}", x)).collect();
        a.push_str(&format!(" stroke-dasharray=\"{}\"", d.join(",")));
    }
    if style.opacity < 1.0 {
        a.push_str(&format!(" stroke-opacity=\"{:.2}\"", style.opacity));
    }
    a
}

/// Build the SVG attribute string for filled primitives (Point, Disk).
fn svg_fill_attrs(style: &GraphicsStyle) -> String {
    let mut a = format!("fill=\"{}\"", style.color);
    if style.opacity < 1.0 {
        a.push_str(&format!(" fill-opacity=\"{:.2}\"", style.opacity));
    }
    a
}

// ── HSV → SVG hex ───────────────────────────────────────────────────────────

/// Simple HSV → hex conversion for the Hue directive.
fn hsv_to_svg(h: f64, s: f64, v: f64) -> String {
    let h = h - h.floor();
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let hi = (h * 6.0).floor() as i32 % 6;
    let f = h * 6.0 - (h * 6.0).floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match hi {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    let ri = (r * 255.0).round() as u8;
    let gi = (g * 255.0).round() as u8;
    let bi = (b * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", ri, gi, bi)
}

// ── Colormap ─────────────────────────────────────────────────────────────────

/// Map a normalized value t ∈ [0,1] to an RGB color using a viridis-like colormap.
fn viridis_color(t: f64) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    const KEYS: &[(f64, (u8, u8, u8))] = &[
        (0.0, (68, 1, 84)),
        (0.25, (59, 82, 139)),
        (0.5, (33, 145, 140)),
        (0.75, (94, 201, 98)),
        (1.0, (253, 231, 37)),
    ];
    for w in KEYS.windows(2) {
        let (t0, c0) = w[0];
        let (t1, c1) = w[1];
        if t <= t1 {
            let f = (t - t0) / (t1 - t0);
            let r = c0.0 as f64 + f * (c1.0 as f64 - c0.0 as f64);
            let g = c0.1 as f64 + f * (c1.1 as f64 - c0.1 as f64);
            let b = c0.2 as f64 + f * (c1.2 as f64 - c0.2 as f64);
            return (r.round() as u8, g.round() as u8, b.round() as u8);
        }
    }
    KEYS.last().unwrap().1
}

// ── Axis scale type ───────────────────────────────────────────────────────────

/// Linear or logarithmic axis scale.
#[derive(Clone, Copy, PartialEq)]
enum ScaleType {
    Linear,
    Log,
}

impl ScaleType {
    fn from_str(s: &str) -> Self {
        if s == "Log" { Self::Log } else { Self::Linear }
    }
}

// ── SVG Renderer ────────────────────────────────────────────────────────────

/// Parsed graphics options: (width, height, plot_range, show_axes, x_scale, y_scale).
type GraphicsOptions = (
    f64,
    f64,
    Option<((f64, f64), (f64, f64))>,
    bool,
    ScaleType,
    ScaleType,
);

/// Default image dimensions.
const DEFAULT_WIDTH: f64 = 600.0;
const DEFAULT_HEIGHT: f64 = 400.0;
const MARGIN: f64 = 50.0;

/// Render a list of graphics primitives and directives to an SVG string.
///
/// Supported geometry primitives:
/// - `Line[{{x1,y1},{x2,y2},...}]`
/// - `Point[{x,y}]`
/// - `Circle[{cx,cy}, r]`
/// - `Rectangle[{x1,y1}, {x2,y2}]`
///
/// Supported directives (interleaved with primitives):
/// - `RGBColor[r,g,b]`, `Hue[h,s,v]`
/// - `Thickness[t]`, `AbsoluteThickness[t]`
/// - `PointSize[s]`, `AbsolutePointSize[s]`
/// - `Dashing[{d1,d2,...}]`, `AbsoluteDashing[{d1,d2,...}]`
/// - `Opacity[o]`
/// - `Directive[d1, d2, ...]`
/// - Named colours: `Red`, `Blue`, `Green`, etc.
pub fn render_svg(primitives: &Value, options: &Value) -> Result<String, EvalError> {
    let prims = as_list(primitives)?;

    // Extract options
    let (width, height, plot_range, show_axes, x_scale, y_scale) = parse_options(options)?;

    // Compute data bounds for auto-ranging (skip directives)
    let (mut x_min, mut x_max, mut y_min, mut y_max) = if let Some((xr, yr)) = &plot_range {
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
        svg.push_str(&render_axes(
            x_min, x_max, y_min, y_max, width, height, &tx, &ty, x_scale, y_scale,
        ));
    }

    // Draw primitives with directive state
    let mut style = GraphicsStyle::default();
    for prim in prims {
        if is_directive(prim) {
            apply_directive(&mut style, prim);
        } else {
            svg.push_str(&render_primitive(prim, &tx, &ty, &style)?);
        }
    }

    svg.push_str("</svg>\n");
    Ok(svg)
}

fn parse_options(options: &Value) -> Result<GraphicsOptions, EvalError> {
    let mut width = DEFAULT_WIDTH;
    let mut height = DEFAULT_HEIGHT;
    let mut plot_range: Option<((f64, f64), (f64, f64))> = None;
    let mut show_axes = true;
    let mut x_scale = ScaleType::Linear;
    let mut y_scale = ScaleType::Linear;

    if let Value::Assoc(map) = options {
        if let Some(v) = map.get("ImageSize")
            && let Value::List(size) = v
            && size.len() >= 2
        {
            width = super::to_f64(&size[0]).unwrap_or(DEFAULT_WIDTH);
            height = super::to_f64(&size[1]).unwrap_or(DEFAULT_HEIGHT);
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
            && xr.len() >= 2
            && yr.len() >= 2
        {
            plot_range = Some((
                (
                    super::to_f64(&xr[0]).unwrap_or(-1.0),
                    super::to_f64(&xr[1]).unwrap_or(1.0),
                ),
                (
                    super::to_f64(&yr[0]).unwrap_or(-1.0),
                    super::to_f64(&yr[1]).unwrap_or(1.0),
                ),
            ));
        }
        if let Some(Value::List(scales)) = map.get("AxesScale")
            && scales.len() >= 2
        {
            if let Value::Str(s) = &scales[0] {
                x_scale = ScaleType::from_str(s);
            }
            if let Value::Str(s) = &scales[1] {
                y_scale = ScaleType::from_str(s);
            }
        }
    }

    Ok((width, height, plot_range, show_axes, x_scale, y_scale))
}

fn compute_bounds(prims: &[Value]) -> Result<(f64, f64, f64, f64), EvalError> {
    let mut x_min = f64::INFINITY;
    let mut x_max = f64::NEG_INFINITY;
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;

    for prim in prims {
        if is_directive(prim) {
            continue;
        }
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
                        super::to_f64(&args[1]).unwrap_or(1.0)
                    } else {
                        1.0
                    };
                    if let Value::List(c) = center
                        && c.len() >= 2
                    {
                        let cx = super::to_f64(&c[0]).unwrap_or(0.0);
                        let cy = super::to_f64(&c[1]).unwrap_or(0.0);
                        x_min = x_min.min(cx - r);
                        x_max = x_max.max(cx + r);
                        y_min = y_min.min(cy - r);
                        y_max = y_max.max(cy + r);
                    }
                }
            }
            Value::Call { head, args } if head == "Rectangle" && args.len() >= 2 => {
                expand_bounds_from_point(&args[0], &mut x_min, &mut x_max, &mut y_min, &mut y_max)?;
                expand_bounds_from_point(&args[1], &mut x_min, &mut x_max, &mut y_min, &mut y_max)?;
            }
            Value::Call { head, args } if head == "DensityRaster" && args.len() >= 4 => {
                // DensityRaster[vals, {xmin,xmax}, {ymin,ymax}, {nx,ny}, ...]
                if let (Value::List(xr), Value::List(yr)) = (&args[1], &args[2])
                    && xr.len() >= 2
                    && yr.len() >= 2
                {
                    x_min = x_min.min(super::to_f64(&xr[0]).unwrap_or(x_min));
                    x_max = x_max.max(super::to_f64(&xr[1]).unwrap_or(x_max));
                    y_min = y_min.min(super::to_f64(&yr[0]).unwrap_or(y_min));
                    y_max = y_max.max(super::to_f64(&yr[1]).unwrap_or(y_max));
                }
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
        && coords.len() >= 2
    {
        let x = super::to_f64(&coords[0]).unwrap_or(0.0);
        let y = super::to_f64(&coords[1]).unwrap_or(0.0);
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
    x_scale: ScaleType,
    y_scale: ScaleType,
) -> String {
    let mut s = String::new();
    let color = "#888";
    let stroke = "stroke-width=\"1\"";

    // X-axis line (at y=0 in data space, or at y_min for log y)
    let x_axis_y_data = if y_scale == ScaleType::Log {
        y_min
    } else {
        0.0
    };
    if y_min <= x_axis_y_data && y_max >= x_axis_y_data {
        let y_svg = ty(x_axis_y_data);
        s.push_str(&format!(
            "<line x1=\"{}\" y1=\"{:.1}\" x2=\"{}\" y2=\"{:.1}\" stroke=\"{}\" {}/>\n",
            MARGIN as i32,
            y_svg,
            (width - MARGIN) as i32,
            y_svg,
            color,
            stroke
        ));
    }

    // Y-axis line (at x=0 in data space, or at x_min for log x)
    let y_axis_x_data = if x_scale == ScaleType::Log {
        x_min
    } else {
        0.0
    };
    if x_min <= y_axis_x_data && x_max >= y_axis_x_data {
        let x_svg = tx(y_axis_x_data);
        s.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{}\" x2=\"{:.1}\" y2=\"{}\" stroke=\"{}\" {}/>\n",
            x_svg,
            MARGIN as i32,
            x_svg,
            (height - MARGIN) as i32,
            color,
            stroke
        ));
    }

    // Tick marks on X-axis
    let y_axis_y = if y_min <= x_axis_y_data && y_max >= x_axis_y_data {
        ty(x_axis_y_data)
    } else {
        height - MARGIN
    };
    for x_tick in axis_ticks(x_min, x_max, x_scale) {
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
        let label = if x_scale == ScaleType::Log {
            format_log_tick(x_tick)
        } else {
            let tmp = format!("{:.4}", x_tick);
            tmp.trim_end_matches('0').trim_end_matches('.').to_string()
        };
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" fill=\"{}\">{}</text>\n",
            sx, y_axis_y + 16.0, color, label
        ));
    }

    // Tick marks on Y-axis
    let x_axis_x = if x_min <= y_axis_x_data && x_max >= y_axis_x_data {
        tx(y_axis_x_data)
    } else {
        MARGIN
    };
    for y_tick in axis_ticks(y_min, y_max, y_scale) {
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
        let label = if y_scale == ScaleType::Log {
            format_log_tick(y_tick)
        } else {
            let tmp = format!("{:.4}", y_tick);
            tmp.trim_end_matches('0').trim_end_matches('.').to_string()
        };
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"10\" fill=\"{}\" dominant-baseline=\"middle\">{}</text>\n",
            x_axis_x - 8.0, sy, color, label
        ));
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

/// Produce tick positions (in data/log-space coordinates) for one axis.
fn axis_ticks(min: f64, max: f64, scale: ScaleType) -> Vec<f64> {
    match scale {
        ScaleType::Linear => {
            let step = nice_step(max - min);
            if step <= 0.0 {
                return vec![];
            }
            let mut ticks = Vec::new();
            let mut t = (min / step).ceil() * step;
            while t <= max + step * 1e-9 {
                ticks.push(t);
                t += step;
            }
            ticks
        }
        ScaleType::Log => {
            let lo = min.floor() as i32;
            let hi = max.ceil() as i32;
            (lo..=hi)
                .map(|n| n as f64)
                .filter(|&t| t >= min - 1e-9 && t <= max + 1e-9)
                .collect()
        }
    }
}

/// Format a tick label for log-scale axes (coordinate is log10 of the actual value).
fn format_log_tick(log_val: f64) -> String {
    let actual = 10.0_f64.powf(log_val);
    if (0.001..100_000.0).contains(&actual) {
        let s = format!("{:.5}", actual);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        format!("10^{}", log_val.round() as i32)
    }
}

/// Compute a "nice" step size for axis ticks.
fn nice_step(range: f64) -> f64 {
    if range <= 0.0 {
        return 1.0;
    }
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
    style: &GraphicsStyle,
) -> Result<String, EvalError> {
    match prim {
        Value::Call { head, args } => match head.as_str() {
            "Line" => {
                if let Some(pts_val) = args.first() {
                    render_line(pts_val, tx, ty, style)
                } else {
                    Ok(String::new())
                }
            }
            "Point" => {
                if let Some(pt) = args.first() {
                    render_point(pt, tx, ty, style)
                } else {
                    Ok(String::new())
                }
            }
            "Circle" => {
                let center = args.first();
                let r = if args.len() > 1 {
                    super::to_f64(&args[1]).unwrap_or(1.0)
                } else {
                    1.0
                };
                if let Some(c) = center {
                    render_circle(c, r, tx, ty, style)
                } else {
                    Ok(String::new())
                }
            }
            "Rectangle" => {
                if args.len() >= 2 {
                    render_rectangle(&args[0], &args[1], tx, ty, style)
                } else {
                    Ok(String::new())
                }
            }
            "DensityRaster" => render_density_raster(args, tx, ty),
            _ => Ok(String::new()),
        },
        // Handle bare lists of points (e.g., from ListPlot)
        Value::List(items) if !items.is_empty() => {
            if let Value::List(coords) = &items[0]
                && coords.len() >= 2
            {
                // It's a list of points
                let mut s = String::new();
                for pt in items {
                    s.push_str(&render_point(pt, tx, ty, style)?);
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
    style: &GraphicsStyle,
) -> Result<String, EvalError> {
    let pts = as_list(pts_val)?;
    if pts.is_empty() {
        return Ok(String::new());
    }
    let mut points_str = String::new();
    for pt in pts {
        if let Value::List(coords) = pt
            && coords.len() >= 2
        {
            let x = super::to_f64(&coords[0]).unwrap_or(0.0);
            let y = super::to_f64(&coords[1]).unwrap_or(0.0);
            if !points_str.is_empty() {
                points_str.push(' ');
            }
            points_str.push_str(&format!("{:.2},{:.2}", tx(x), ty(y)));
        }
    }
    Ok(format!(
        "<polyline points=\"{}\" fill=\"none\" {}/>\n",
        points_str,
        svg_stroke_attrs(style),
    ))
}

fn render_point(
    pt: &Value,
    tx: &dyn Fn(f64) -> f64,
    ty: &dyn Fn(f64) -> f64,
    style: &GraphicsStyle,
) -> Result<String, EvalError> {
    if let Value::List(coords) = pt
        && coords.len() >= 2
    {
        let x = super::to_f64(&coords[0]).unwrap_or(0.0);
        let y = super::to_f64(&coords[1]).unwrap_or(0.0);
        return Ok(format!(
            "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.1}\" stroke=\"none\" {}/>\n",
            tx(x),
            ty(y),
            style.point_size,
            svg_fill_attrs(style),
        ));
    }
    Ok(String::new())
}

fn render_circle(
    center: &Value,
    r: f64,
    tx: &dyn Fn(f64) -> f64,
    ty: &dyn Fn(f64) -> f64,
    style: &GraphicsStyle,
) -> Result<String, EvalError> {
    if let Value::List(coords) = center
        && coords.len() >= 2
    {
        let cx = super::to_f64(&coords[0]).unwrap_or(0.0);
        let cy = super::to_f64(&coords[1]).unwrap_or(0.0);
        // Scale radius using the x scale factor
        let scale = tx(1.0) - tx(0.0);
        return Ok(format!(
            "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"none\" {}/>\n",
            tx(cx),
            ty(cy),
            r * scale.abs(),
            svg_stroke_attrs(style),
        ));
    }
    Ok(String::new())
}

fn render_rectangle(
    p1: &Value,
    p2: &Value,
    tx: &dyn Fn(f64) -> f64,
    ty: &dyn Fn(f64) -> f64,
    style: &GraphicsStyle,
) -> Result<String, EvalError> {
    let coords1 = as_list(p1)?;
    let coords2 = as_list(p2)?;
    if coords1.len() >= 2 && coords2.len() >= 2 {
        let x1 = super::to_f64(&coords1[0]).unwrap_or(0.0);
        let y1 = super::to_f64(&coords1[1]).unwrap_or(0.0);
        let x2 = super::to_f64(&coords2[0]).unwrap_or(0.0);
        let y2 = super::to_f64(&coords2[1]).unwrap_or(0.0);
        let sx = tx(x1).min(tx(x2));
        let sy = ty(y1).min(ty(y2));
        let w = (tx(x2) - tx(x1)).abs();
        let h = (ty(y2) - ty(y1)).abs();
        return Ok(format!(
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" \
             fill=\"none\" {}/>\n",
            sx,
            sy,
            w,
            h,
            svg_stroke_attrs(style),
        ));
    }
    Ok(String::new())
}

fn render_density_raster(
    args: &[Value],
    tx: &dyn Fn(f64) -> f64,
    ty: &dyn Fn(f64) -> f64,
) -> Result<String, EvalError> {
    // DensityRaster[flat_vals, {xmin,xmax}, {ymin,ymax}, {nx,ny}, val_min, val_max]
    if args.len() < 6 {
        return Ok(String::new());
    }
    let flat = as_list(&args[0])?;
    let xr = as_list(&args[1])?;
    let yr = as_list(&args[2])?;
    let dims = as_list(&args[3])?;
    let val_min = super::to_f64(&args[4]).unwrap_or(0.0);
    let val_max = super::to_f64(&args[5]).unwrap_or(1.0);

    if xr.len() < 2 || yr.len() < 2 || dims.len() < 2 {
        return Ok(String::new());
    }

    let xmin = super::to_f64(&xr[0]).unwrap_or(0.0);
    let xmax = super::to_f64(&xr[1]).unwrap_or(1.0);
    let ymin = super::to_f64(&yr[0]).unwrap_or(0.0);
    let ymax = super::to_f64(&yr[1]).unwrap_or(1.0);
    let nx = match &dims[0] {
        Value::Integer(n) => n.to_usize().unwrap_or(50),
        _ => 50,
    };
    let ny = match &dims[1] {
        Value::Integer(n) => n.to_usize().unwrap_or(50),
        _ => 50,
    };

    if nx == 0 || ny == 0 || flat.len() < nx * ny {
        return Ok(String::new());
    }

    let range = val_max - val_min;
    let mut s = String::new();

    for j in 0..ny {
        let y_lo = ymin + (ymax - ymin) * j as f64 / ny as f64;
        let y_hi = ymin + (ymax - ymin) * (j + 1) as f64 / ny as f64;
        for i in 0..nx {
            let x_lo = xmin + (xmax - xmin) * i as f64 / nx as f64;
            let x_hi = xmin + (xmax - xmin) * (i + 1) as f64 / nx as f64;
            let v = super::to_f64(&flat[j * nx + i]).unwrap_or(val_min);
            let t = if range > 0.0 {
                (v - val_min) / range
            } else {
                0.5
            };
            let (r, g, b) = viridis_color(t);
            let sx = tx(x_lo);
            let sy = ty(y_hi); // SVG y is flipped
            let sw = (tx(x_hi) - tx(x_lo)).abs();
            let sh = (ty(y_lo) - ty(y_hi)).abs();
            s.push_str(&format!(
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" \
                 fill=\"#{:02x}{:02x}{:02x}\" stroke=\"none\"/>\n",
                sx, sy, sw, sh, r, g, b
            ));
        }
    }

    Ok(s)
}

// ── Color helpers ────────────────────────────────────────────────────────────

/// Extract RGB components (each in [0,1]) from an RGBColor Call value.
fn extract_rgb(color: &Value) -> Option<(f64, f64, f64)> {
    match color {
        Value::Call { head, args } if head == "RGBColor" && args.len() == 3 => {
            let r = super::to_f64(&args[0])?;
            let g = super::to_f64(&args[1])?;
            let b = super::to_f64(&args[2])?;
            Some((r, g, b))
        }
        _ => None,
    }
}

/// Create an RGBColor value, clamping each channel to [0, 1].
fn make_rgb(r: f64, g: f64, b: f64) -> Value {
    Value::Call {
        head: "RGBColor".to_string(),
        args: vec![
            super::real(r.clamp(0.0, 1.0)),
            super::real(g.clamp(0.0, 1.0)),
            super::real(b.clamp(0.0, 1.0)),
        ],
    }
}

// ── Builtins ────────────────────────────────────────────────────────────────

/// ListPlot[data] — scatter plot from {x,y} pairs, returns a Graphics object.
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
    let primitives = Value::List(points);

    Ok(Value::Call {
        head: "Graphics".to_string(),
        args: vec![primitives, options],
    })
}

/// ListLinePlot[data] — line plot from {x,y} pairs, returns a Graphics object.
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
    let primitives = Value::List(vec![line]);

    Ok(Value::Call {
        head: "Graphics".to_string(),
        args: vec![primitives, options],
    })
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

// ── Color Primitives ────────────────────────────────────────────────────────

/// RGBColor[r, g, b] — create an RGB color with components in [0, 1].
pub fn builtin_rgb_color(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("RGBColor", args, 3)?;
    let r = super::require_f64(&args[0], "RGBColor", 1)?;
    let g = super::require_f64(&args[1], "RGBColor", 2)?;
    let b = super::require_f64(&args[2], "RGBColor", 3)?;
    Ok(make_rgb(r, g, b))
}

/// Hue[h, s, b] — HSV to RGB conversion. h, s, b in [0, 1].
/// Hue[h] uses s=1, b=1. Hue[h, s] uses b=1.
pub fn builtin_hue(args: &[Value]) -> Result<Value, EvalError> {
    super::require_min_args("Hue", args, 1)?;
    if args.len() > 3 {
        return Err(EvalError::Error(
            "Hue requires 1 to 3 arguments".to_string(),
        ));
    }
    let h = super::require_f64(&args[0], "Hue", 1)?;
    let s = if args.len() > 1 {
        super::require_f64(&args[1], "Hue", 2)?
    } else {
        1.0
    };
    let v = if args.len() > 2 {
        super::require_f64(&args[2], "Hue", 3)?
    } else {
        1.0
    };

    // Standard HSV → RGB algorithm
    let h = h - h.floor(); // normalize to [0, 1)
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let hi = (h * 6.0).floor() as i32 % 6;
    let f = h * 6.0 - (h * 6.0).floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match hi {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Ok(make_rgb(r, g, b))
}

/// GrayLevel[g] — grayscale color with intensity g in [0, 1].
pub fn builtin_gray_level(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("GrayLevel", args, 1)?;
    let g = super::require_f64(&args[0], "GrayLevel", 1)?;
    Ok(make_rgb(g, g, g))
}

/// Lighter[color] — lighten by 1/3 (move each channel toward 1.0).
/// Lighter[color, amount] — lighten by amount in [0, 1].
pub fn builtin_lighter(args: &[Value]) -> Result<Value, EvalError> {
    super::require_min_args("Lighter", args, 1)?;
    if args.len() > 2 {
        return Err(EvalError::Error(
            "Lighter requires 1 or 2 arguments".to_string(),
        ));
    }
    let (r, g, b) = extract_rgb(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "an RGBColor value".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    let amount = if args.len() > 1 {
        super::require_f64(&args[1], "Lighter", 2)?.clamp(0.0, 1.0)
    } else {
        1.0 / 3.0
    };
    Ok(make_rgb(
        r + (1.0 - r) * amount,
        g + (1.0 - g) * amount,
        b + (1.0 - b) * amount,
    ))
}

/// Darker[color] — darken by 1/3 (move each channel toward 0.0).
/// Darker[color, amount] — darken by amount in [0, 1].
pub fn builtin_darker(args: &[Value]) -> Result<Value, EvalError> {
    super::require_min_args("Darker", args, 1)?;
    if args.len() > 2 {
        return Err(EvalError::Error(
            "Darker requires 1 or 2 arguments".to_string(),
        ));
    }
    let (r, g, b) = extract_rgb(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "an RGBColor value".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    let amount = if args.len() > 1 {
        super::require_f64(&args[1], "Darker", 2)?.clamp(0.0, 1.0)
    } else {
        1.0 / 3.0
    };
    Ok(make_rgb(r * (1.0 - amount), g * (1.0 - amount), b * (1.0 - amount)))
}

/// Blend[{c1, c2, ...}] — average colors equally.
/// Blend[{c1, c2, ...}, {w1, w2, ...}] — weighted average.
pub fn builtin_blend(args: &[Value]) -> Result<Value, EvalError> {
    super::require_min_args("Blend", args, 1)?;
    if args.len() > 2 {
        return Err(EvalError::Error(
            "Blend requires 1 or 2 arguments".to_string(),
        ));
    }
    let colors = match &args[0] {
        Value::List(v) => v,
        _ => {
            return Err(EvalError::TypeError {
                expected: "a list of colors".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    if colors.is_empty() {
        return Err(EvalError::Error(
            "Blend requires a non-empty list of colors".to_string(),
        ));
    }

    let weights: Vec<f64> = if args.len() > 1 {
        match &args[1] {
            Value::List(w) => {
                let ws: Vec<f64> = w
                    .iter()
                    .map(|v| super::to_f64(v).ok_or_else(|| EvalError::TypeError {
                        expected: "a number".to_string(),
                        got: v.type_name().to_string(),
                    }))
                    .collect::<Result<Vec<f64>, EvalError>>()?;
                if ws.len() != colors.len() {
                    return Err(EvalError::Error(format!(
                        "Blend: weight list length ({}) must match color list length ({})",
                        ws.len(),
                        colors.len()
                    )));
                }
                ws
            }
            _ => {
                return Err(EvalError::TypeError {
                    expected: "a list of weights".to_string(),
                    got: args[1].type_name().to_string(),
                });
            }
        }
    } else {
        vec![1.0; colors.len()]
    };

    let total_weight: f64 = weights.iter().sum();
    if total_weight == 0.0 {
        return Err(EvalError::Error(
            "Blend: total weight must be non-zero".to_string(),
        ));
    }

    let mut r_sum = 0.0;
    let mut g_sum = 0.0;
    let mut b_sum = 0.0;
    for (c, w) in colors.iter().zip(weights.iter()) {
        let (r, g, b) = extract_rgb(c).ok_or_else(|| EvalError::TypeError {
            expected: "an RGBColor value".to_string(),
            got: c.type_name().to_string(),
        })?;
        r_sum += r * w;
        g_sum += g * w;
        b_sum += b * w;
    }
    Ok(make_rgb(
        r_sum / total_weight,
        g_sum / total_weight,
        b_sum / total_weight,
    ))
}

/// ColorNegate[color] — invert an RGB color: (1-r, 1-g, 1-b).
pub fn builtin_color_negate(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("ColorNegate", args, 1)?;
    let (r, g, b) = extract_rgb(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "an RGBColor value".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    Ok(make_rgb(1.0 - r, 1.0 - g, 1.0 - b))
}

// ── Geometry Primitives ─────────────────────────────────────────────────────

/// Point[{x, y}] / Point[{x, y, z}] — point at given coordinates.
pub fn builtin_point(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Point", args, 1)?;
    match &args[0] {
        Value::List(coords) => {
            if coords.len() < 2 || coords.len() > 3 {
                return Err(EvalError::Error(
                    "Point requires 2 or 3 coordinates".to_string(),
                ));
            }
            for (i, c) in coords.iter().enumerate() {
                super::require_f64(c, "Point", i + 1)?;
            }
            Ok(Value::Call {
                head: "Point".to_string(),
                args: args.to_vec(),
            })
        }
        _ => Err(EvalError::TypeError {
            expected: "a list of coordinates".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// Line[{p1, p2, ...}] — line through a list of points.
pub fn builtin_line(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Line", args, 1)?;
    match &args[0] {
        Value::List(points) => {
            for (i, pt) in points.iter().enumerate() {
                match pt {
                    Value::List(coords) => {
                        if coords.len() < 2 {
                            return Err(EvalError::Error(format!(
                                "Line: point {} must have at least 2 coordinates",
                                i + 1
                            )));
                        }
                        for (j, c) in coords.iter().enumerate() {
                            super::require_f64(c, "Line", j + 1)?;
                        }
                    }
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: format!("a list of coordinates for point {}", i + 1),
                            got: pt.type_name().to_string(),
                        });
                    }
                }
            }
            Ok(Value::Call {
                head: "Line".to_string(),
                args: args.to_vec(),
            })
        }
        _ => Err(EvalError::TypeError {
            expected: "a list of points".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// Circle[{cx, cy}, r] — circle with center and radius.
/// Circle[{cx, cy}, {rx, ry}] — ellipse with semi-axes.
pub fn builtin_circle(args: &[Value]) -> Result<Value, EvalError> {
    super::require_min_args("Circle", args, 1)?;
    if args.len() > 2 {
        return Err(EvalError::Error(
            "Circle requires 1 or 2 arguments".to_string(),
        ));
    }
    // Validate center
    match &args[0] {
        Value::List(coords) => {
            if coords.len() < 2 {
                return Err(EvalError::Error(
                    "Circle: center must have at least 2 coordinates".to_string(),
                ));
            }
            for (i, c) in coords.iter().enumerate() {
                super::require_f64(c, "Circle", i + 1)?;
            }
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "a list of coordinates".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    }
    // Validate radius (if provided)
    if args.len() > 1 {
        match &args[1] {
            Value::List(axes) => {
                // Ellipse: {rx, ry}
                for (i, a) in axes.iter().enumerate() {
                    super::require_f64(a, "Circle", i + 1)?;
                }
            }
            other => {
                super::require_f64(other, "Circle", 2)?;
            }
        }
    }
    Ok(Value::Call {
        head: "Circle".to_string(),
        args: args.to_vec(),
    })
}

/// Disk[{cx, cy}, r] — filled circle with center and radius.
/// Disk[{cx, cy}, {rx, ry}] — filled ellipse.
pub fn builtin_disk(args: &[Value]) -> Result<Value, EvalError> {
    super::require_min_args("Disk", args, 1)?;
    if args.len() > 2 {
        return Err(EvalError::Error(
            "Disk requires 1 or 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::List(coords) => {
            if coords.len() < 2 {
                return Err(EvalError::Error(
                    "Disk: center must have at least 2 coordinates".to_string(),
                ));
            }
            for (i, c) in coords.iter().enumerate() {
                super::require_f64(c, "Disk", i + 1)?;
            }
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "a list of coordinates".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    }
    if args.len() > 1 {
        match &args[1] {
            Value::List(axes) => {
                for (i, a) in axes.iter().enumerate() {
                    super::require_f64(a, "Disk", i + 1)?;
                }
            }
            other => {
                super::require_f64(other, "Disk", 2)?;
            }
        }
    }
    Ok(Value::Call {
        head: "Disk".to_string(),
        args: args.to_vec(),
    })
}

/// Triangle[{p1, p2, p3}] — triangle from 3 vertices.
pub fn builtin_triangle(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Triangle", args, 1)?;
    match &args[0] {
        Value::List(points) => {
            if points.len() != 3 {
                return Err(EvalError::Error(format!(
                    "Triangle requires exactly 3 points, got {}",
                    points.len()
                )));
            }
            for (i, pt) in points.iter().enumerate() {
                match pt {
                    Value::List(coords) => {
                        if coords.len() < 2 {
                            return Err(EvalError::Error(format!(
                                "Triangle: point {} must have at least 2 coordinates",
                                i + 1
                            )));
                        }
                        for (j, c) in coords.iter().enumerate() {
                            super::require_f64(c, "Triangle", j + 1)?;
                        }
                    }
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: format!("a list of coordinates for vertex {}", i + 1),
                            got: pt.type_name().to_string(),
                        });
                    }
                }
            }
            Ok(Value::Call {
                head: "Triangle".to_string(),
                args: args.to_vec(),
            })
        }
        _ => Err(EvalError::TypeError {
            expected: "a list of 3 points".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// Rectangle[{xmin, ymin}, {xmax, ymax}] — rectangle from two opposite corners.
pub fn builtin_rectangle(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Rectangle", args, 2)?;
    for (pos, arg) in args.iter().enumerate() {
        match arg {
            Value::List(coords) => {
                if coords.len() < 2 {
                    return Err(EvalError::Error(format!(
                        "Rectangle: corner {} must have at least 2 coordinates",
                        pos + 1
                    )));
                }
                for (i, c) in coords.iter().enumerate() {
                    super::require_f64(c, "Rectangle", i + 1)?;
                }
            }
            _ => {
                return Err(EvalError::TypeError {
                    expected: format!("a list of coordinates for corner {}", pos + 1),
                    got: arg.type_name().to_string(),
                });
            }
        }
    }
    Ok(Value::Call {
        head: "Rectangle".to_string(),
        args: args.to_vec(),
    })
}

/// Polygon[{p1, p2, ...}] — polygon from a list of vertices.
pub fn builtin_polygon(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("Polygon", args, 1)?;
    match &args[0] {
        Value::List(points) => {
            if points.len() < 3 {
                return Err(EvalError::Error(format!(
                    "Polygon requires at least 3 vertices, got {}",
                    points.len()
                )));
            }
            for (i, pt) in points.iter().enumerate() {
                match pt {
                    Value::List(coords) => {
                        if coords.len() < 2 {
                            return Err(EvalError::Error(format!(
                                "Polygon: vertex {} must have at least 2 coordinates",
                                i + 1
                            )));
                        }
                        for (j, c) in coords.iter().enumerate() {
                            super::require_f64(c, "Polygon", j + 1)?;
                        }
                    }
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: format!("a list of coordinates for vertex {}", i + 1),
                            got: pt.type_name().to_string(),
                        });
                    }
                }
            }
            Ok(Value::Call {
                head: "Polygon".to_string(),
                args: args.to_vec(),
            })
        }
        _ => Err(EvalError::TypeError {
            expected: "a list of points".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// Sphere[{x, y, z}, r] — 3D sphere with center and radius.
pub fn builtin_sphere(args: &[Value]) -> Result<Value, EvalError> {
    super::require_min_args("Sphere", args, 1)?;
    if args.len() > 2 {
        return Err(EvalError::Error(
            "Sphere requires 1 or 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::List(coords) => {
            if coords.len() < 3 {
                return Err(EvalError::Error(
                    "Sphere: center must have at least 3 coordinates".to_string(),
                ));
            }
            for (i, c) in coords.iter().enumerate() {
                super::require_f64(c, "Sphere", i + 1)?;
            }
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "a list of coordinates".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    }
    if args.len() > 1 {
        super::require_f64(&args[1], "Sphere", 2)?;
    }
    Ok(Value::Call {
        head: "Sphere".to_string(),
        args: args.to_vec(),
    })
}

/// Cylinder[{p1, p2}, r] — cylinder between two 3D points with given radius.
pub fn builtin_cylinder(args: &[Value]) -> Result<Value, EvalError> {
    super::require_min_args("Cylinder", args, 1)?;
    if args.len() > 2 {
        return Err(EvalError::Error(
            "Cylinder requires 1 or 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::List(points) => {
            if points.len() != 2 {
                return Err(EvalError::Error(format!(
                    "Cylinder: endpoint list must have exactly 2 points, got {}",
                    points.len()
                )));
            }
            for (i, pt) in points.iter().enumerate() {
                match pt {
                    Value::List(coords) => {
                        if coords.len() < 3 {
                            return Err(EvalError::Error(format!(
                                "Cylinder: endpoint {} must have at least 3 coordinates",
                                i + 1
                            )));
                        }
                        for (j, c) in coords.iter().enumerate() {
                            super::require_f64(c, "Cylinder", j + 1)?;
                        }
                    }
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: format!("a list of coordinates for endpoint {}", i + 1),
                            got: pt.type_name().to_string(),
                        });
                    }
                }
            }
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "a list of two endpoints".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    }
    if args.len() > 1 {
        super::require_f64(&args[1], "Cylinder", 2)?;
    }
    Ok(Value::Call {
        head: "Cylinder".to_string(),
        args: args.to_vec(),
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

    // Color primitives
    register_builtin(env, "RGBColor", builtin_rgb_color);
    register_builtin(env, "Hue", builtin_hue);
    register_builtin(env, "GrayLevel", builtin_gray_level);
    register_builtin(env, "Lighter", builtin_lighter);
    register_builtin(env, "Darker", builtin_darker);
    register_builtin(env, "Blend", builtin_blend);
    register_builtin(env, "ColorNegate", builtin_color_negate);

    // Geometry primitives
    register_builtin(env, "Point", builtin_point);
    register_builtin(env, "Line", builtin_line);
    register_builtin(env, "Circle", builtin_circle);
    register_builtin(env, "Disk", builtin_disk);
    register_builtin(env, "Triangle", builtin_triangle);
    register_builtin(env, "Rectangle", builtin_rectangle);
    register_builtin(env, "Polygon", builtin_polygon);
    register_builtin(env, "Sphere", builtin_sphere);
    register_builtin(env, "Cylinder", builtin_cylinder);
}

/// Symbol names exported by the Graphics package.
pub const SYMBOLS: &[&str] = &[
    "ListPlot",
    "ListLinePlot",
    "ExportGraphics",
    "Graphics",
    // Evaluator-dependent (handled in eval.rs):
    "Plot",
    "LogPlot",
    "LogLogPlot",
    "LogLinearPlot",
    "ParametricPlot",
    "PolarPlot",
    "DiscretePlot",
    "DensityPlot",
    // Syma-side wrappers (loaded from .syma file):
    "Show",
    "GraphicsGrid",
    // Color primitives
    "RGBColor",
    "Hue",
    "GrayLevel",
    "Lighter",
    "Darker",
    "Blend",
    "ColorNegate",
    // Geometry primitives
    "Point",
    "Line",
    "Circle",
    "Disk",
    "Triangle",
    "Rectangle",
    "Polygon",
    "Sphere",
    "Cylinder",
    // Style directives
    "Thickness",
    "PointSize",
    "Opacity",
    "Directive",
];

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Float;

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
        let data = list(vec![point(0.0, 0.0), point(1.0, 1.0), point(2.0, 4.0)]);
        let result = builtin_list_plot(&[data]).unwrap();
        match &result {
            Value::Call { head, args } if head == "Graphics" && args.len() >= 1 => {
                assert!(matches!(&args[0], Value::List(prims) if prims.len() == 3));
            }
            _ => panic!("Expected Graphics call, got {:?}", result),
        }
    }

    #[test]
    fn test_list_line_plot() {
        let data = list(vec![point(0.0, 0.0), point(1.0, 1.0), point(2.0, 4.0)]);
        let result = builtin_list_line_plot(&[data]).unwrap();
        match &result {
            Value::Call { head, args } if head == "Graphics" && args.len() >= 1 => {
                assert!(matches!(&args[0], Value::List(prims) if prims.len() == 1));
            }
            _ => panic!("Expected Graphics call, got {:?}", result),
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
        let svg_content =
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><circle cx=\"10\" cy=\"10\" r=\"5\"/></svg>";
        let result = builtin_export_graphics(&[
            Value::Str(path.to_string()),
            Value::Str(svg_content.to_string()),
        ]);
        assert!(result.is_ok());
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("<svg"));
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_export_graphics_svg() {
        // Export a symbolic Graphics object to .svg and verify it renders
        use std::fs;
        let path = "/tmp/test_syma_graphics_export.svg";
        let line = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![
                point(0.0, 0.0),
                point(1.0, 1.0),
                point(2.0, 4.0),
            ])],
        };
        let primitives = Value::List(vec![line]);
        let options = Value::Assoc(std::collections::HashMap::new());
        let graphics = Value::Call {
            head: "Graphics".to_string(),
            args: vec![primitives, options],
        };

        let result = crate::builtins::io::builtin_export(&[Value::Str(path.to_string()), graphics]);
        assert!(result.is_ok(), "Export failed: {:?}", result);

        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("<svg"));
        assert!(content.contains("<polyline"));
        assert!(content.contains("</svg>"));
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_export_graphics_no_svg_extension() {
        // Exporting a Graphics object to non-.svg should still fall back to text
        use std::fs;
        let path = "/tmp/test_syma_graphics.txt";
        let graphics = Value::Call {
            head: "Graphics".to_string(),
            args: vec![
                Value::List(vec![]),
                Value::Assoc(std::collections::HashMap::new()),
            ],
        };

        let result = crate::builtins::io::builtin_export(&[Value::Str(path.to_string()), graphics]);
        assert!(result.is_ok());

        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("Graphics")); // should be text representation
        fs::remove_file(path).ok();
    }

    // ── Directive tests ────────────────────────────────────────────────────

    #[test]
    fn test_directive_rgb_color() {
        let red = Value::Call {
            head: "RGBColor".to_string(),
            args: vec![real_val(1.0), real_val(0.0), real_val(0.0)],
        };
        let line = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(0.0, 0.0), point(1.0, 1.0)])],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![red, line]), &opts).unwrap();
        assert!(
            svg.contains("stroke=\"#ff0000\""),
            "Expected red stroke, got: {}",
            svg
        );
    }

    #[test]
    fn test_directive_named_color() {
        let blue = Value::Symbol("Blue".to_string());
        let line = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(0.0, 0.0), point(1.0, 1.0)])],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![blue, line]), &opts).unwrap();
        assert!(
            svg.contains("stroke=\"#2563eb\""),
            "Expected blue stroke, got: {}",
            svg
        );
    }

    #[test]
    fn test_directive_thickness() {
        let thick = Value::Call {
            head: "Thickness".to_string(),
            args: vec![real_val(0.1)],
        };
        let line = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(0.0, 0.0), point(1.0, 1.0)])],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![thick, line]), &opts).unwrap();
        assert!(
            svg.contains("stroke-width=\"1.0\""),
            "Expected stroke-width 1.0, got: {}",
            svg
        );
    }

    #[test]
    fn test_directive_absolute_thickness() {
        let thick = Value::Call {
            head: "AbsoluteThickness".to_string(),
            args: vec![real_val(5.0)],
        };
        let line = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(0.0, 0.0), point(1.0, 1.0)])],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![thick, line]), &opts).unwrap();
        assert!(
            svg.contains("stroke-width=\"5.0\""),
            "Expected stroke-width 5.0, got: {}",
            svg
        );
    }

    #[test]
    fn test_directive_point_size() {
        let big_pts = Value::Call {
            head: "AbsolutePointSize".to_string(),
            args: vec![real_val(10.0)],
        };
        let pt = Value::Call {
            head: "Point".to_string(),
            args: vec![point(5.0, 3.0)],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![big_pts, pt]), &opts).unwrap();
        assert!(svg.contains("r=\"10.0\""), "Expected r=10.0, got: {}", svg);
    }

    #[test]
    fn test_directive_opacity() {
        let half = Value::Call {
            head: "Opacity".to_string(),
            args: vec![real_val(0.5)],
        };
        let line = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(0.0, 0.0), point(1.0, 1.0)])],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![half, line]), &opts).unwrap();
        assert!(
            svg.contains("opacity"),
            "Expected opacity attribute, got: {}",
            svg
        );
    }

    #[test]
    fn test_directive_hue() {
        let hue = Value::Call {
            head: "Hue".to_string(),
            args: vec![real_val(0.0)], // red
        };
        let line = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(0.0, 0.0), point(1.0, 1.0)])],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![hue, line]), &opts).unwrap();
        // Hue[0] should produce red-ish
        assert!(
            svg.contains("stroke=\"#"),
            "Expected stroke attribute, got: {}",
            svg
        );
    }

    #[test]
    fn test_directive_composite() {
        let comp = Value::Call {
            head: "Directive".to_string(),
            args: vec![
                Value::Call {
                    head: "RGBColor".to_string(),
                    args: vec![real_val(0.0), real_val(1.0), real_val(0.0)],
                },
                Value::Call {
                    head: "AbsoluteThickness".to_string(),
                    args: vec![real_val(3.0)],
                },
            ],
        };
        let line = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(0.0, 0.0), point(1.0, 1.0)])],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![comp, line]), &opts).unwrap();
        assert!(svg.contains("#00ff00"), "Expected green, got: {}", svg);
        assert!(
            svg.contains("stroke-width=\"3.0\""),
            "Expected width 3.0, got: {}",
            svg
        );
    }

    #[test]
    fn test_directive_state_persists() {
        // Directives should persist for subsequent primitives
        let red = Value::Call {
            head: "RGBColor".to_string(),
            args: vec![real_val(1.0), real_val(0.0), real_val(0.0)],
        };
        let line1 = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(0.0, 0.0), point(1.0, 1.0)])],
        };
        let line2 = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(2.0, 2.0), point(3.0, 3.0)])],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![red, line1, line2]), &opts).unwrap();
        // Both lines should be red
        assert_eq!(
            svg.matches("stroke=\"#ff0000\"").count(),
            2,
            "Both lines should be red"
        );
    }

    #[test]
    fn test_directive_red_overrides_green() {
        let green = Value::Call {
            head: "RGBColor".to_string(),
            args: vec![real_val(0.0), real_val(1.0), real_val(0.0)],
        };
        let red = Value::Call {
            head: "RGBColor".to_string(),
            args: vec![real_val(1.0), real_val(0.0), real_val(0.0)],
        };
        let line1 = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(0.0, 0.0), point(1.0, 1.0)])],
        };
        let line2 = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(2.0, 2.0), point(3.0, 3.0)])],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![green, line1, red, line2]), &opts).unwrap();
        // line1 green, line2 red
        assert!(svg.contains("#00ff00"), "line1 should be green");
        assert!(svg.contains("#ff0000"), "line2 should be red");
    }

    #[test]
    fn test_directive_dashing() {
        let dashed = Value::Call {
            head: "Dashing".to_string(),
            args: vec![list(vec![real_val(5.0), real_val(3.0)])],
        };
        let line = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(0.0, 0.0), point(1.0, 1.0)])],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![dashed, line]), &opts).unwrap();
        assert!(
            svg.contains("stroke-dasharray"),
            "Expected dasharray, got: {}",
            svg
        );
    }

    #[test]
    fn test_directive_thick_thin() {
        let thick = Value::Call {
            head: "Thick".to_string(),
            args: vec![],
        };
        let line = Value::Call {
            head: "Line".to_string(),
            args: vec![list(vec![point(0.0, 0.0), point(1.0, 1.0)])],
        };
        let opts = Value::Assoc(std::collections::HashMap::new());
        let svg = render_svg(&Value::List(vec![thick, line]), &opts).unwrap();
        assert!(
            svg.contains("stroke-width=\"4.0\""),
            "Thick should be 4.0, got: {}",
            svg
        );
    }

    // ── Color primitive tests ────────────────────────────────────────────────

    #[test]
    fn test_rgb_color() {
        let result = builtin_rgb_color(&[real_val(0.5), real_val(0.2), real_val(0.8)]).unwrap();
        match &result {
            Value::Call { head, args } if head == "RGBColor" && args.len() == 3 => {
                assert!((super::super::to_f64(&args[0]).unwrap() - 0.5).abs() < 1e-10);
                assert!((super::super::to_f64(&args[1]).unwrap() - 0.2).abs() < 1e-10);
                assert!((super::super::to_f64(&args[2]).unwrap() - 0.8).abs() < 1e-10);
            }
            _ => panic!("Expected RGBColor call, got {:?}", result),
        }
    }

    #[test]
    fn test_rgb_color_clamp() {
        let result = builtin_rgb_color(&[real_val(1.5), real_val(-0.1), real_val(0.5)]).unwrap();
        match &result {
            Value::Call { head, args } if head == "RGBColor" => {
                assert!((super::super::to_f64(&args[0]).unwrap() - 1.0).abs() < 1e-10);
                assert!((super::super::to_f64(&args[1]).unwrap() - 0.0).abs() < 1e-10);
                assert!((super::super::to_f64(&args[2]).unwrap() - 0.5).abs() < 1e-10);
            }
            _ => panic!("Expected RGBColor call"),
        }
    }

    #[test]
    fn test_rgb_color_wrong_args() {
        assert!(builtin_rgb_color(&[real_val(0.5)]).is_err());
        assert!(builtin_rgb_color(&[real_val(0.5), real_val(0.5)]).is_err());
    }

    #[test]
    fn test_hue_red() {
        // Hue[0] should be red
        let result = builtin_hue(&[real_val(0.0)]).unwrap();
        let (r, g, b) = extract_rgb(&result).unwrap();
        assert!((r - 1.0).abs() < 1e-10, "r should be 1.0, got {}", r);
        assert!((g - 0.0).abs() < 1e-10, "g should be 0.0, got {}", g);
        assert!((b - 0.0).abs() < 1e-10, "b should be 0.0, got {}", b);
    }

    #[test]
    fn test_hue_green() {
        // Hue[1/3] should be green
        let result = builtin_hue(&[real_val(1.0 / 3.0)]).unwrap();
        let (r, g, b) = extract_rgb(&result).unwrap();
        assert!((g - 1.0).abs() < 1e-10, "g should be 1.0, got {}", g);
        assert!(r < 0.01, "r should be ~0, got {}", r);
        assert!(b < 0.01, "b should be ~0, got {}", b);
    }

    #[test]
    fn test_hue_with_saturation_brightness() {
        // Hue[0.5, 0.5, 0.5]
        let result = builtin_hue(&[real_val(0.5), real_val(0.5), real_val(0.5)]).unwrap();
        let (r, g, b) = extract_rgb(&result).unwrap();
        // All channels should be in [0,1]
        assert!((0.0..=1.0).contains(&r));
        assert!((0.0..=1.0).contains(&g));
        assert!((0.0..=1.0).contains(&b));
    }

    #[test]
    fn test_gray_level() {
        let result = builtin_gray_level(&[real_val(0.4)]).unwrap();
        let (r, g, b) = extract_rgb(&result).unwrap();
        assert!((r - 0.4).abs() < 1e-10);
        assert!((g - 0.4).abs() < 1e-10);
        assert!((b - 0.4).abs() < 1e-10);
    }

    #[test]
    fn test_lighter_default() {
        let color = make_rgb(0.6, 0.4, 0.2);
        let result = builtin_lighter(&[color]).unwrap();
        let (r, g, b) = extract_rgb(&result).unwrap();
        // Lighter by 1/3: r + (1-r)*1/3 = 0.6 + 0.4/3 ≈ 0.7333
        assert!((r - (0.6 + 0.4 / 3.0)).abs() < 1e-10);
        assert!((g - (0.4 + 0.6 / 3.0)).abs() < 1e-10);
        assert!((b - (0.2 + 0.8 / 3.0)).abs() < 1e-10);
    }

    #[test]
    fn test_lighter_amount() {
        let color = make_rgb(0.0, 0.0, 0.0);
        let result = builtin_lighter(&[color, real_val(1.0)]).unwrap();
        let (r, g, b) = extract_rgb(&result).unwrap();
        // Lighter by 1.0: 0 + (1-0)*1 = 1.0
        assert!((r - 1.0).abs() < 1e-10);
        assert!((g - 1.0).abs() < 1e-10);
        assert!((b - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_darker_default() {
        let color = make_rgb(0.6, 0.9, 0.3);
        let result = builtin_darker(&[color]).unwrap();
        let (r, g, b) = extract_rgb(&result).unwrap();
        // Darker by 1/3: r * (1 - 1/3) = r * 2/3
        assert!((r - 0.6 * 2.0 / 3.0).abs() < 1e-10);
        assert!((g - 0.9 * 2.0 / 3.0).abs() < 1e-10);
        assert!((b - 0.3 * 2.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_blend_equal() {
        let c1 = make_rgb(1.0, 0.0, 0.0);
        let c2 = make_rgb(0.0, 0.0, 1.0);
        let result = builtin_blend(&[Value::List(vec![c1, c2])]).unwrap();
        let (r, g, b) = extract_rgb(&result).unwrap();
        assert!((r - 0.5).abs() < 1e-10);
        assert!((g - 0.0).abs() < 1e-10);
        assert!((b - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_blend_weighted() {
        let c1 = make_rgb(1.0, 0.0, 0.0);
        let c2 = make_rgb(0.0, 0.0, 1.0);
        let result = builtin_blend(&[
            Value::List(vec![c1, c2]),
            Value::List(vec![real_val(3.0), real_val(1.0)]),
        ])
        .unwrap();
        let (r, g, b) = extract_rgb(&result).unwrap();
        // weighted: (3*1+1*0)/4 = 0.75, (3*0+1*0)/4=0, (3*0+1*1)/4=0.25
        assert!((r - 0.75).abs() < 1e-10);
        assert!((g - 0.0).abs() < 1e-10);
        assert!((b - 0.25).abs() < 1e-10);
    }

    #[test]
    fn test_color_negate() {
        let color = make_rgb(0.2, 0.4, 0.6);
        let result = builtin_color_negate(&[color]).unwrap();
        let (r, g, b) = extract_rgb(&result).unwrap();
        assert!((r - 0.8).abs() < 1e-10);
        assert!((g - 0.6).abs() < 1e-10);
        assert!((b - 0.4).abs() < 1e-10);
    }

    // ── Geometry primitive tests ─────────────────────────────────────────────

    #[test]
    fn test_point_2d() {
        let result = builtin_point(&[point(1.0, 2.0)]).unwrap();
        match &result {
            Value::Call { head, args } if head == "Point" && args.len() == 1 => {}
            _ => panic!("Expected Point call, got {:?}", result),
        }
    }

    #[test]
    fn test_point_3d() {
        let pt3d = list(vec![real_val(1.0), real_val(2.0), real_val(3.0)]);
        let result = builtin_point(&[pt3d]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Point" => {}
            _ => panic!("Expected Point call"),
        }
    }

    #[test]
    fn test_point_wrong_args() {
        // Not a list
        assert!(builtin_point(&[real_val(1.0)]).is_err());
        // Only 1 coordinate
        assert!(builtin_point(&[list(vec![real_val(1.0)])]).is_err());
    }

    #[test]
    fn test_line() {
        let pts = list(vec![point(0.0, 0.0), point(1.0, 1.0), point(2.0, 0.0)]);
        let result = builtin_line(&[pts]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Line" => {}
            _ => panic!("Expected Line call"),
        }
    }

    #[test]
    fn test_circle_center_radius() {
        let center = point(0.0, 0.0);
        let result = builtin_circle(&[center, real_val(5.0)]).unwrap();
        match &result {
            Value::Call { head, args } if head == "Circle" && args.len() == 2 => {}
            _ => panic!("Expected Circle call"),
        }
    }

    #[test]
    fn test_circle_ellipse() {
        let center = point(0.0, 0.0);
        let axes = list(vec![real_val(3.0), real_val(1.5)]);
        let result = builtin_circle(&[center, axes]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Circle" => {}
            _ => panic!("Expected Circle call"),
        }
    }

    #[test]
    fn test_disk() {
        let center = point(1.0, 2.0);
        let result = builtin_disk(&[center, real_val(3.0)]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Disk" => {}
            _ => panic!("Expected Disk call"),
        }
    }

    #[test]
    fn test_triangle() {
        let tri = list(vec![point(0.0, 0.0), point(1.0, 0.0), point(0.5, 1.0)]);
        let result = builtin_triangle(&[tri]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Triangle" => {}
            _ => panic!("Expected Triangle call"),
        }
    }

    #[test]
    fn test_triangle_wrong_count() {
        let not_tri = list(vec![point(0.0, 0.0), point(1.0, 0.0)]);
        assert!(builtin_triangle(&[not_tri]).is_err());
    }

    #[test]
    fn test_rectangle() {
        let result = builtin_rectangle(&[point(0.0, 0.0), point(3.0, 2.0)]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Rectangle" => {}
            _ => panic!("Expected Rectangle call"),
        }
    }

    #[test]
    fn test_polygon() {
        let pentagon = list(vec![
            point(1.0, 0.0),
            point(0.3, 0.95),
            point(-0.8, 0.59),
            point(-0.8, -0.59),
            point(0.3, -0.95),
        ]);
        let result = builtin_polygon(&[pentagon]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Polygon" => {}
            _ => panic!("Expected Polygon call"),
        }
    }

    #[test]
    fn test_polygon_too_few() {
        assert!(builtin_polygon(&[list(vec![point(0.0, 0.0), point(1.0, 0.0)])]).is_err());
    }

    #[test]
    fn test_sphere() {
        let center = list(vec![real_val(0.0), real_val(0.0), real_val(0.0)]);
        let result = builtin_sphere(&[center, real_val(1.0)]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Sphere" => {}
            _ => panic!("Expected Sphere call"),
        }
    }

    #[test]
    fn test_sphere_no_radius() {
        let center = list(vec![real_val(0.0), real_val(0.0), real_val(0.0)]);
        let result = builtin_sphere(&[center]).unwrap();
        match &result {
            Value::Call { head, args } if head == "Sphere" && args.len() == 1 => {}
            _ => panic!("Expected Sphere call with 1 arg"),
        }
    }

    #[test]
    fn test_cylinder() {
        let p1 = list(vec![real_val(0.0), real_val(0.0), real_val(0.0)]);
        let p2 = list(vec![real_val(0.0), real_val(0.0), real_val(1.0)]);
        let ends = list(vec![p1, p2]);
        let result = builtin_cylinder(&[ends, real_val(0.5)]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Cylinder" => {}
            _ => panic!("Expected Cylinder call"),
        }
    }

    #[test]
    fn test_cylinder_no_radius() {
        let p1 = list(vec![real_val(0.0), real_val(0.0), real_val(0.0)]);
        let p2 = list(vec![real_val(1.0), real_val(1.0), real_val(1.0)]);
        let ends = list(vec![p1, p2]);
        let result = builtin_cylinder(&[ends]).unwrap();
        match &result {
            Value::Call { head, args } if head == "Cylinder" && args.len() == 1 => {}
            _ => panic!("Expected Cylinder call with 1 arg"),
        }
    }

    #[test]
    fn test_extract_rgb() {
        let color = make_rgb(0.3, 0.5, 0.7);
        let (r, g, b) = extract_rgb(&color).unwrap();
        assert!((r - 0.3).abs() < 1e-10);
        assert!((g - 0.5).abs() < 1e-10);
        assert!((b - 0.7).abs() < 1e-10);
    }

    #[test]
    fn test_extract_rgb_non_color() {
        assert!(extract_rgb(&real_val(1.0)).is_none());
        assert!(extract_rgb(&Value::Symbol("Red".to_string())).is_none());
    }
}
