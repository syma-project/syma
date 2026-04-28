//! Charting package builtins.
//!
//! Provides SVG-based chart primitives: BarChart, Histogram, PieChart,
//! DonutChart, BubbleChart, WaterfallChart, RegionPlot, and ContourPlot.

use crate::value::{EvalError, Value};
use std::collections::HashMap;

// ── Color Palette ──────────────────────────────────────────────────────────────

const DEFAULT_PALETTE: &[&str] = &[
    "#4285F4", "#EA4335", "#FBBC05", "#34A853", "#FF6D01", "#46BDC6", "#9C27B0", "#795548",
    "#607D8B", "#F44336", "#E91E63", "#9E9E9E",
];

/// Get a color from the palette by index (wraps around).
fn palette_color(i: usize) -> String {
    DEFAULT_PALETTE[i % DEFAULT_PALETTE.len()].to_string()
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        Value::Rational(r) => Some(r.to_f64()),
        _ => None,
    }
}

fn get_list(val: &Value) -> Result<&Vec<Value>, EvalError> {
    match val {
        Value::List(items) => Ok(items),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: val.type_name().to_string(),
        }),
    }
}

const DEFAULT_WIDTH: f64 = 600.0;
const DEFAULT_HEIGHT: f64 = 400.0;
const MARGIN_LEFT: f64 = 60.0;
const MARGIN_RIGHT: f64 = 20.0;
const MARGIN_TOP: f64 = 20.0;
const MARGIN_BOTTOM: f64 = 50.0;
const EEE: &str = "#eee";
const FFF: &str = "#fff";
const WHITE: &str = "#ffffff";
const BLUE: &str = "#1a73e8";
const DARK_TEXT: &str = "#333";
const AXIS_COLOR: &str = "#888";

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

fn axis_ticks(min: f64, max: f64) -> Vec<f64> {
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

fn format_tick(v: f64) -> String {
    let s = format!("{:.4}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Build a complete SVG wrapper with axes for cartesian charts.
fn svg_with_axes(
    content: &str,
    width: f64,
    height: f64,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    x_label: &str,
    y_label: &str,
) -> String {
    let plot_w = width - MARGIN_LEFT - MARGIN_RIGHT;
    let plot_h = height - MARGIN_TOP - MARGIN_BOTTOM;

    let tx = |x: f64| MARGIN_LEFT + (x - x_min) / (x_max - x_min) * plot_w;
    let ty = |y: f64| MARGIN_TOP + (1.0 - (y - y_min) / (y_max - y_min)) * plot_h;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" style=\"background:white\">\n",
        width as i32, height as i32, width as i32, height as i32
    ));

    // Grid lines (horizontal)
    for tick in &axis_ticks(y_min, y_max) {
        let sy = ty(*tick);
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1\"/>\n",
            MARGIN_LEFT, sy, MARGIN_LEFT + plot_w, sy, EEE
        ));
    }

    // Grid lines (vertical)
    for tick in &axis_ticks(x_min, x_max) {
        let sx = tx(*tick);
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1\"/>\n",
            sx, MARGIN_TOP, sx, MARGIN_TOP + plot_h, EEE
        ));
    }

    // Plot content
    svg.push_str(content);

    // Axes
    let y0 = if y_min <= 0.0 && y_max >= 0.0 {
        ty(0.0)
    } else {
        MARGIN_TOP + plot_h
    };
    svg.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1.5\"/>\n",
        MARGIN_LEFT, y0, MARGIN_LEFT + plot_w, y0, AXIS_COLOR
    ));

    let x0 = if x_min <= 0.0 && x_max >= 0.0 {
        tx(0.0)
    } else {
        MARGIN_LEFT
    };
    svg.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1.5\"/>\n",
        x0, MARGIN_TOP, x0, MARGIN_TOP + plot_h, AXIS_COLOR
    ));

    // Tick labels (X)
    let x_label_y = if y_label.is_empty() {
        MARGIN_TOP + plot_h + 18.0
    } else {
        MARGIN_TOP + plot_h + 40.0
    };
    for tick in axis_ticks(x_min, x_max) {
        let sx = tx(tick);
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1\"/>\n",
            sx, y0 - 3.0, sx, y0 + 3.0, AXIS_COLOR
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" fill=\"{}\">{}</text>\n",
            sx, x_label_y, AXIS_COLOR, format_tick(tick)
        ));
    }

    // Tick labels (Y)
    let x_label_x = if x_label.is_empty() {
        MARGIN_LEFT - 8.0
    } else {
        MARGIN_LEFT - 32.0
    };
    for tick in axis_ticks(y_min, y_max) {
        let sy = ty(tick);
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1\"/>\n",
            x0 - 3.0, sy, x0 + 3.0, sy, AXIS_COLOR
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"10\" fill=\"{}\" dominant-baseline=\"middle\">{}</text>\n",
            x_label_x, sy, AXIS_COLOR, format_tick(tick)
        ));
    }

    // Axis labels
    if !x_label.is_empty() {
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12\" fill=\"{}\">{}</text>\n",
            MARGIN_LEFT + plot_w / 2.0,
            MARGIN_TOP + plot_h + 45.0,
            AXIS_COLOR,
            x_label
        ));
    }
    if !y_label.is_empty() {
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12\" fill=\"{}\" transform=\"rotate(-90 {:.1} {:.1})\">{}</text>\n",
            14.0,
            MARGIN_TOP + plot_h / 2.0,
            AXIS_COLOR,
            14.0,
            MARGIN_TOP + plot_h / 2.0,
            y_label
        ));
    }

    // Border
    svg.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1\"/>\n",
        MARGIN_LEFT, MARGIN_TOP, plot_w, plot_h, AXIS_COLOR
    ));

    svg.push_str("</svg>\n");
    svg
}

/// Map data coordinates to SVG coordinates.
fn map_to_svg(
    x: f64,
    y: f64,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    width: f64,
    height: f64,
) -> (f64, f64) {
    let plot_w = width - MARGIN_LEFT - MARGIN_RIGHT;
    let plot_h = height - MARGIN_TOP - MARGIN_BOTTOM;
    let sx = MARGIN_LEFT + (x - x_min) / (x_max - x_min) * plot_w;
    let sy = MARGIN_TOP + (1.0 - (y - y_min) / (y_max - y_min)) * plot_h;
    (sx, sy)
}

/// Extract chart options from an Assoc value.
fn parse_chart_options(opts: &Value) -> (f64, f64, bool) {
    let mut width = DEFAULT_WIDTH;
    let mut height = DEFAULT_HEIGHT;
    let mut grid_lines = false;

    if let Value::Assoc(map) = opts {
        if let Some(v) = map.get("ImageSize") {
            if let Value::List(size) = v {
                if size.len() >= 2 {
                    width = to_f64(&size[0]).unwrap_or(DEFAULT_WIDTH);
                    height = to_f64(&size[1]).unwrap_or(DEFAULT_HEIGHT);
                }
            }
        }
        if let Some(v) = map.get("GridLines") {
            grid_lines = match v {
                Value::Bool(b) => *b,
                Value::Symbol(s) => s == "True",
                _ => false,
            };
        }
    }

    (width, height, grid_lines)
}

/// Auto select number of histogram bins using Sturges' rule.
fn auto_bins(n: usize) -> usize {
    ((n as f64).log2().ceil() as usize + 1).max(1)
}

// ── BarChart ───────────────────────────────────────────────────────────────────

pub fn builtin_bar_chart(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "BarChart requires at least 1 argument".to_string(),
        ));
    }

    let data = get_list(&args[0])?;
    if data.is_empty() {
        return Err(EvalError::Error(
            "BarChart: data must not be empty".to_string(),
        ));
    }

    let options = if args.len() > 1 {
        &args[1]
    } else {
        &Value::Assoc(HashMap::new())
    };
    let (width, height, _) = parse_chart_options(options);

    // Parse data as either {v1, v2, ...} or {{x1, y1}, {x2, y2}, ...}
    let bars: Vec<(f64, f64)> = match &data[0] {
        Value::List(pair) if pair.len() >= 2 => {
            let mut result = Vec::new();
            for item in data {
                if let Value::List(p) = item {
                    if let (Some(x), Some(y)) = (to_f64(&p[0]), to_f64(&p[1])) {
                        result.push((x, y));
                    }
                }
            }
            result
        }
        _ => data
            .iter()
            .enumerate()
            .filter_map(|(i, v)| to_f64(v).map(|y| (i as f64 + 1.0, y)))
            .collect(),
    };

    if bars.is_empty() {
        return Err(EvalError::Error(
            "BarChart: no valid data points".to_string(),
        ));
    }

    // Compute bounds
    let x_vals: Vec<f64> = bars.iter().map(|(x, _)| *x).collect();
    let y_vals: Vec<f64> = bars.iter().map(|(_, y)| *y).collect();
    let x_min_data = x_vals.iter().cloned().fold(f64::INFINITY, f64::min);
    let x_max_data = x_vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let y_max = y_vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let y_min = if y_vals.iter().all(|&y| y >= 0.0) {
        0.0
    } else {
        y_vals.iter().cloned().fold(f64::INFINITY, f64::min)
    };

    let y_range = y_max - y_min;
    let y_min_padded = y_min - y_range * 0.05;
    let y_max_padded = y_max + y_range * 0.05;

    let x_range = x_max_data - x_min_data;
    let x_pad = if x_range > 0.0 { x_range * 0.15 } else { 1.5 };
    let x_min_padded = x_min_data - x_pad;
    let x_max_padded = x_max_data + x_pad;

    let bar_width = if x_range > 1.0 {
        (x_range / bars.len() as f64) * 0.6
    } else {
        0.6
    };

    let mut content = String::new();
    for (i, (bx, by)) in bars.iter().enumerate() {
        let color = palette_color(i);
        let base_y = if *by >= 0.0 {
            y_min_padded
        } else {
            y_max_padded
        };

        let (sx_left, _) = map_to_svg(
            bx - bar_width / 2.0,
            0.0,
            x_min_padded,
            x_max_padded,
            y_min_padded,
            y_max_padded,
            width,
            height,
        );
        let (sx_right, _) = map_to_svg(
            bx + bar_width / 2.0,
            0.0,
            x_min_padded,
            x_max_padded,
            y_min_padded,
            y_max_padded,
            width,
            height,
        );
        let (_, sy_base) = map_to_svg(
            0.0,
            base_y,
            x_min_padded,
            x_max_padded,
            y_min_padded,
            y_max_padded,
            width,
            height,
        );
        let (_, sy_top) = map_to_svg(
            0.0,
            *by,
            x_min_padded,
            x_max_padded,
            y_min_padded,
            y_max_padded,
            width,
            height,
        );

        let sw = (sx_right - sx_left).abs();
        let sh = (sy_top - sy_base).abs().max(0.0);
        let sy_min = sy_base.min(sy_top);

        content.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\"/>\n",
            sx_left, sy_min, sw, sh, color, FFF
        ));
    }

    Ok(Value::Str(svg_with_axes(
        &content,
        width,
        height,
        x_min_padded,
        x_max_padded,
        y_min_padded,
        y_max_padded,
        "",
        "",
    )))
}

/// Generate a 3D bar chart — for now returns symbolic unevaluated expression.
pub fn builtin_bar_chart_3d(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "BarChart3D requires at least 1 argument".to_string(),
        ));
    }
    Ok(Value::Call {
        head: "Chart3D".to_string(),
        args: args.to_vec(),
    })
}

// ── Histogram ──────────────────────────────────────────────────────────────────

pub fn builtin_histogram(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Histogram requires at least 1 argument".to_string(),
        ));
    }

    let data = get_list(&args[0])?;
    if data.is_empty() {
        return Err(EvalError::Error(
            "Histogram: data must not be empty".to_string(),
        ));
    }

    let options = if args.len() > 2 {
        &args[2]
    } else {
        if args.len() == 2 {
            match &args[1] {
                Value::Assoc(_) => &args[1],
                _ => &Value::Assoc(HashMap::new()),
            }
        } else {
            &Value::Assoc(HashMap::new())
        }
    };
    let (width, height, _) = parse_chart_options(options);

    let nums: Vec<f64> = data.iter().filter_map(|v| to_f64(v)).collect();
    if nums.is_empty() {
        return Err(EvalError::Error("Histogram: no numeric data".to_string()));
    }

    let data_min = nums.iter().cloned().fold(f64::INFINITY, f64::min);
    let data_max = nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let bins = if args.len() >= 2 {
        match &args[1] {
            Value::Integer(n) => n.to_usize().unwrap_or(10).max(1),
            Value::Real(r) => (r.to_f64() as usize).max(1),
            Value::List(edges) => {
                let edge_vals: Vec<f64> = edges.iter().filter_map(|e| to_f64(e)).collect();
                if edge_vals.len() < 2 {
                    return Err(EvalError::Error(
                        "Histogram: need at least 2 bin edges".to_string(),
                    ));
                }
                return build_histogram_svg(&nums, &edge_vals, width, height);
            }
            _ => auto_bins(nums.len()),
        }
    } else {
        auto_bins(nums.len())
    };

    let range = data_max - data_min;
    let dx = if range == 0.0 {
        1.0
    } else {
        range / bins as f64
    };
    let edges: Vec<f64> = (0..=bins).map(|i| data_min + i as f64 * dx).collect();

    build_histogram_svg(&nums, &edges, width, height)
}

fn build_histogram_svg(
    nums: &[f64],
    edges: &[f64],
    width: f64,
    height: f64,
) -> Result<Value, EvalError> {
    let n_bins = edges.len() - 1;
    if n_bins == 0 {
        return Err(EvalError::Error(
            "Histogram: need at least 1 bin".to_string(),
        ));
    }

    let mut counts = vec![0i64; n_bins];
    for &x in nums {
        if x >= edges[edges.len() - 1] {
            counts[n_bins - 1] += 1;
        } else {
            for i in 0..n_bins {
                if x >= edges[i] && x < edges[i + 1] {
                    counts[i] += 1;
                    break;
                }
            }
        }
    }

    let max_count = counts.iter().cloned().fold(0i64, i64::max);
    let y_max = if max_count > 0 {
        (max_count as f64) * 1.1
    } else {
        1.0
    };
    let x_min = edges[0];
    let x_max = edges[edges.len() - 1];

    let mut content = String::new();
    for (i, &count) in counts.iter().enumerate() {
        let x_lo = edges[i];
        let x_hi = edges[i + 1];
        let y_top = count as f64;
        let color = palette_color(i);

        let (sx_lo, _) = map_to_svg(x_lo, 0.0, x_min, x_max, 0.0, y_max, width, height);
        let (sx_hi, _) = map_to_svg(x_hi, 0.0, x_min, x_max, 0.0, y_max, width, height);
        let (_, sy_top) = map_to_svg(0.0, y_top, x_min, x_max, 0.0, y_max, width, height);
        let (_, sy_base) = map_to_svg(0.0, 0.0, x_min, x_max, 0.0, y_max, width, height);

        let sw = (sx_hi - sx_lo).abs();
        let sh = (sy_base - sy_top).abs();

        content.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\" fill-opacity=\"0.7\" stroke=\"{}\" stroke-width=\"1\"/>\n",
            sx_lo, sy_top, sw, sh, color, FFF
        ));
    }

    Ok(Value::Str(svg_with_axes(
        &content, width, height, x_min, x_max, 0.0, y_max, "", "Count",
    )))
}

/// HistogramList returns {edges, counts} — delegates to statistics.
pub fn builtin_histogram_list(args: &[Value]) -> Result<Value, EvalError> {
    crate::builtins::statistics::builtin_histogram_list(args)
}

// ── PieChart ───────────────────────────────────────────────────────────────────

pub fn builtin_pie_chart(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "PieChart requires at least 1 argument".to_string(),
        ));
    }

    let data = get_list(&args[0])?;
    if data.is_empty() {
        return Err(EvalError::Error(
            "PieChart: data must not be empty".to_string(),
        ));
    }

    let options = if args.len() > 1 {
        &args[1]
    } else {
        &Value::Assoc(HashMap::new())
    };
    let (width, height, _) = parse_chart_options(options);

    struct PieSlice {
        value: f64,
        label: Option<String>,
    }

    let slices: Vec<PieSlice> = match &data[0] {
        Value::List(pair) if pair.len() >= 2 => {
            let mut result = Vec::new();
            for item in data {
                if let Value::List(p) = item {
                    let val = to_f64(&p[0]).unwrap_or(0.0);
                    let label = if p.len() > 1 {
                        match &p[1] {
                            Value::Str(s) => Some(s.clone()),
                            v => Some(v.to_string()),
                        }
                    } else {
                        None
                    };
                    result.push(PieSlice { value: val, label });
                }
            }
            result
        }
        _ => data
            .iter()
            .filter_map(|v| {
                to_f64(v).map(|val| PieSlice {
                    value: val,
                    label: None,
                })
            })
            .collect(),
    };

    let total: f64 = slices.iter().map(|s| s.value).sum();
    if total <= 0.0 {
        return Err(EvalError::Error(
            "PieChart: total must be positive".to_string(),
        ));
    }

    let cx = width / 2.0;
    let cy = height / 2.0;
    let r = (width.min(height) / 2.0) * 0.75;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" style=\"background:white\">\n",
        width as i32, height as i32, width as i32, height as i32
    ));

    let mut angle = 0.0_f64;
    let start_angle = -std::f64::consts::PI / 2.0;

    for (i, slice) in slices.iter().enumerate() {
        let fraction = slice.value / total;
        let slice_angle = 2.0 * std::f64::consts::PI * fraction;
        let a_start = start_angle + angle;
        let a_end = a_start + slice_angle;
        let color = palette_color(i);

        let r_inner = r * 0.98;
        let (x1, y1) = (cx + r_inner * a_start.cos(), cy - r_inner * a_start.sin());
        let (x2, y2) = (cx + r_inner * a_end.cos(), cy - r_inner * a_end.sin());
        let large_arc = if slice_angle > std::f64::consts::PI {
            1
        } else {
            0
        };

        if fraction.abs() >= 1e-12 {
            svg.push_str(&format!(
                "<path d=\"M {:.1} {:.1} L {:.1} {:.1} A {:.1} {:.1} 0 {} 1 {:.1} {:.1} Z\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1.5\"/>\n",
                cx, cy, x2, y2, r_inner, r_inner, large_arc, x1, y1, color, FFF
            ));
        }

        // Labels
        if let Some(ref label) = slice.label {
            let mid_angle = a_start + slice_angle / 2.0;
            let label_r = r_inner * 0.6;
            let lx = cx + label_r * mid_angle.cos();
            let ly = cy - label_r * mid_angle.sin();
            svg.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" fill=\"{}\" font-weight=\"bold\" dominance=\"central\">{}</text>\n",
                lx, ly, DARK_TEXT, label
            ));
        }

        angle += slice_angle;
    }

    svg.push_str("</svg>\n");
    Ok(Value::Str(svg))
}

// ── DonutChart ─────────────────────────────────────────────────────────────────

pub fn builtin_donut_chart(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "DonutChart requires at least 1 argument".to_string(),
        ));
    }

    let data = get_list(&args[0])?;
    if data.is_empty() {
        return Err(EvalError::Error(
            "DonutChart: data must not be empty".to_string(),
        ));
    }

    let options = if args.len() > 1 {
        &args[1]
    } else {
        &Value::Assoc(HashMap::new())
    };
    let (width, height, _) = parse_chart_options(options);

    let inner_ratio = if let Value::Assoc(map) = options {
        if let Some(v) = map.get("InnerRadius") {
            to_f64(v).unwrap_or(0.4)
        } else {
            0.4
        }
    } else {
        0.4
    };

    struct PieSlice {
        value: f64,
        label: Option<String>,
    }

    let slices: Vec<PieSlice> = match &data[0] {
        Value::List(pair) if pair.len() >= 2 => {
            let mut result = Vec::new();
            for item in data {
                if let Value::List(p) = item {
                    let val = to_f64(&p[0]).unwrap_or(0.0);
                    let label = if p.len() > 1 {
                        match &p[1] {
                            Value::Str(s) => Some(s.clone()),
                            v => Some(v.to_string()),
                        }
                    } else {
                        None
                    };
                    result.push(PieSlice { value: val, label });
                }
            }
            result
        }
        _ => data
            .iter()
            .filter_map(|v| {
                to_f64(v).map(|val| PieSlice {
                    value: val,
                    label: None,
                })
            })
            .collect(),
    };

    let total: f64 = slices.iter().map(|s| s.value).sum();
    if total <= 0.0 {
        return Err(EvalError::Error(
            "DonutChart: total must be positive".to_string(),
        ));
    }

    let cx = width / 2.0;
    let cy = height / 2.0;
    let r_outer = (width.min(height) / 2.0) * 0.75;
    let r_inner = r_outer * inner_ratio;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" style=\"background:white\">\n",
        width as i32, height as i32, width as i32, height as i32
    ));

    let mut angle = 0.0_f64;
    let start_angle = -std::f64::consts::PI / 2.0;

    for (i, slice) in slices.iter().enumerate() {
        let fraction = slice.value / total;
        let slice_angle = 2.0 * std::f64::consts::PI * fraction;
        let a_start = start_angle + angle;
        let a_end = a_start + slice_angle;
        let color = palette_color(i);

        if fraction.abs() < 1e-12 {
            angle += slice_angle;
            continue;
        }

        let (ox1, oy1) = (cx + r_outer * a_start.cos(), cy - r_outer * a_start.sin());
        let (ox2, oy2) = (cx + r_outer * a_end.cos(), cy - r_outer * a_end.sin());
        let (ix1, iy1) = (cx + r_inner * a_end.cos(), cy - r_inner * a_end.sin());
        let (ix2, iy2) = (cx + r_inner * a_start.cos(), cy - r_inner * a_start.sin());
        let large_arc = if slice_angle > std::f64::consts::PI {
            1
        } else {
            0
        };

        svg.push_str(&format!(
            "<path d=\"M {:.1} {:.1} A {:.1} {:.1} 0 {} 1 {:.1} {:.1} L {:.1} {:.1} A {:.1} {:.1} 0 {} 0 {:.1} {:.1} Z\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\"/>\n",
            ox1, oy1,
            r_outer, r_outer, large_arc,
            ox2, oy2,
            ix1, iy1,
            r_inner, r_inner, large_arc,
            ix2, iy2,
            color, FFF
        ));

        if let Some(ref label) = slice.label {
            let mid_angle = a_start + slice_angle / 2.0;
            let label_r = (r_outer + r_inner) / 2.0;
            let lx = cx + label_r * mid_angle.cos();
            let ly = cy - label_r * mid_angle.sin();
            svg.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" fill=\"{}\" font-weight=\"bold\" dominance=\"central\">{}</text>\n",
                lx, ly, WHITE, label
            ));
        }

        angle += slice_angle;
    }

    svg.push_str("</svg>\n");
    Ok(Value::Str(svg))
}

// ── BubbleChart ────────────────────────────────────────────────────────────────

pub fn builtin_bubble_chart(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "BubbleChart requires at least 1 argument".to_string(),
        ));
    }

    let data = get_list(&args[0])?;
    if data.is_empty() {
        return Err(EvalError::Error(
            "BubbleChart: data must not be empty".to_string(),
        ));
    }

    let options = if args.len() > 1 {
        &args[1]
    } else {
        &Value::Assoc(HashMap::new())
    };
    let (width, height, _) = parse_chart_options(options);

    struct Bubble {
        x: f64,
        y: f64,
        r: f64,
    }

    let bubbles: Vec<Bubble> = data
        .iter()
        .filter_map(|item| {
            if let Value::List(p) = item {
                if p.len() >= 2 {
                    let x = to_f64(&p[0])?;
                    let y = to_f64(&p[1])?;
                    let r = if p.len() >= 3 {
                        to_f64(&p[2]).unwrap_or(1.0)
                    } else {
                        1.0
                    };
                    Some(Bubble { x, y, r })
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    if bubbles.is_empty() {
        return Err(EvalError::Error(
            "BubbleChart: no valid data points".to_string(),
        ));
    }

    let x_vals: Vec<f64> = bubbles.iter().map(|b| b.x).collect();
    let y_vals: Vec<f64> = bubbles.iter().map(|b| b.y).collect();
    let x_min = x_vals.iter().cloned().fold(f64::INFINITY, f64::min);
    let x_max = x_vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let y_min = y_vals.iter().cloned().fold(f64::INFINITY, f64::min);
    let y_max = y_vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let x_range = if x_max - x_min > 0.0 {
        x_max - x_min
    } else {
        2.0
    };
    let y_range = if y_max - y_min > 0.0 {
        y_max - y_min
    } else {
        2.0
    };
    let x_min_p = x_min - x_range * 0.08;
    let x_max_p = x_max + x_range * 0.08;
    let y_min_p = y_min - y_range * 0.08;
    let y_max_p = y_max + y_range * 0.08;

    let max_r = bubbles
        .iter()
        .map(|b| b.r)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_r = bubbles.iter().map(|b| b.r).fold(f64::INFINITY, f64::min);
    let r_range = if max_r - min_r > 0.0 {
        max_r - min_r
    } else {
        1.0
    };
    let max_plot_r =
        (width - MARGIN_LEFT - MARGIN_RIGHT).min(height - MARGIN_TOP - MARGIN_BOTTOM) * 0.06;
    let min_plot_r = max_plot_r * 0.3;

    let scale_radius = |r: f64| -> f64 {
        if r_range < 1e-12 {
            min_plot_r + (max_plot_r - min_plot_r) / 2.0
        } else {
            min_plot_r + (max_plot_r - min_plot_r) * (r - min_r) / r_range
        }
    };

    let mut content = String::new();
    for (i, bubble) in bubbles.iter().enumerate() {
        let (sx, sy) = map_to_svg(
            bubble.x, bubble.y, x_min_p, x_max_p, y_min_p, y_max_p, width, height,
        );
        let sr = scale_radius(bubble.r);
        let color = palette_color(i);

        content.push_str(&format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"{:.1}\" fill=\"{}\" fill-opacity=\"0.6\" stroke=\"{}\" stroke-width=\"1.5\"/>\n",
            sx, sy, sr, color, color
        ));
    }

    Ok(Value::Str(svg_with_axes(
        &content, width, height, x_min_p, x_max_p, y_min_p, y_max_p, "", "",
    )))
}

// ── WaterfallChart ─────────────────────────────────────────────────────────────

pub fn builtin_waterfall_chart(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "WaterfallChart requires at least 1 argument".to_string(),
        ));
    }

    let data = get_list(&args[0])?;
    if data.is_empty() {
        return Err(EvalError::Error(
            "WaterfallChart: data must not be empty".to_string(),
        ));
    }

    let options = if args.len() > 1 {
        &args[1]
    } else {
        &Value::Assoc(HashMap::new())
    };
    let (width, height, _) = parse_chart_options(options);

    let values: Vec<f64> = data.iter().filter_map(|v| to_f64(v)).collect();
    if values.is_empty() {
        return Err(EvalError::Error(
            "WaterfallChart: no numeric data".to_string(),
        ));
    }

    struct WaterfallBar {
        x: f64,
        from: f64,
        to: f64,
        positive: bool,
    }

    let mut bars: Vec<WaterfallBar> = Vec::new();
    let mut cum = 0.0_f64;
    for (i, &v) in values.iter().enumerate() {
        let from = cum;
        let to = cum + v;
        bars.push(WaterfallBar {
            x: (i + 1) as f64,
            from,
            to,
            positive: v >= 0.0,
        });
        cum = to;
    }

    let y_max = bars
        .iter()
        .map(|b| b.from.max(b.to))
        .fold(f64::NEG_INFINITY, f64::max);
    let y_min = bars
        .iter()
        .map(|b| b.from.min(b.to))
        .fold(f64::INFINITY, f64::min);
    let y_range = if y_max - y_min > 0.0 {
        y_max - y_min
    } else {
        1.0
    };
    let y_min_p = y_min - y_range * 0.08;
    let y_max_p = y_max + y_range * 0.08;
    let x_min_p = 0.5_f64;
    let x_max_p = (bars.len() + 1) as f64;

    let bar_width = 0.6_f64;
    let pos_color = "#34A853";
    let neg_color = "#EA4335";
    let total_color = "#4285F4";

    let mut content = String::new();
    for (i, bar) in bars.iter().enumerate() {
        let color = if i == bars.len() - 1 && values.len() > 1 {
            total_color
        } else if bar.positive {
            pos_color
        } else {
            neg_color
        };

        let (sx, _) = map_to_svg(
            bar.x - bar_width / 2.0,
            0.0,
            x_min_p,
            x_max_p,
            y_min_p,
            y_max_p,
            width,
            height,
        );
        let (_, sy_from) = map_to_svg(
            0.0, bar.from, x_min_p, x_max_p, y_min_p, y_max_p, width, height,
        );
        let (_, sy_to) = map_to_svg(
            0.0, bar.to, x_min_p, x_max_p, y_min_p, y_max_p, width, height,
        );
        let (_, sx_r) = map_to_svg(
            bar.x + bar_width / 2.0,
            0.0,
            x_min_p,
            x_max_p,
            y_min_p,
            y_max_p,
            width,
            height,
        );

        let sw = (sx_r - sx).abs();
        let sh = (sy_to - sy_from).abs().max(0.0);
        let sy_top = sy_from.min(sy_to);

        content.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\" fill-opacity=\"0.8\"/>\n",
            sx, sy_top, sw, sh, color, FFF
        ));
    }

    Ok(Value::Str(svg_with_axes(
        &content, width, height, x_min_p, x_max_p, y_min_p, y_max_p, "", "",
    )))
}

// ── TreeMap (symbolic) ─────────────────────────────────────────────────────

pub fn builtin_tree_map(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Call {
        head: "TreeMap".to_string(),
        args: args.to_vec(),
    })
}

// ── Dendrogram (symbolic) ────────────────────────────────────────────────────

pub fn builtin_dendrogram(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Call {
        head: "Dendrogram".to_string(),
        args: args.to_vec(),
    })
}

// ── RegionPlot ────────────────────────────────────────────────────────────────

pub fn builtin_region_plot(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 4 {
        return Err(EvalError::Error(
            "RegionPlot requires at least 4 arguments: REGION, {x, xmin, xmax}, {y, ymin, ymax}, options.".to_string(),
        ));
    }
    Ok(Value::Call {
        head: "RegionPlot".to_string(),
        args: args.to_vec(),
    })
}

/// RegionPlot with evaluation — called from eval.rs.
pub fn builtin_region_plot_eval(
    func: &dyn Fn(f64, f64) -> f64,
    x_range: (f64, f64),
    y_range: (f64, f64),
    nx: usize,
    ny: usize,
    width: f64,
    height: f64,
) -> Result<String, EvalError> {
    let (xmin, xmax) = x_range;
    let (ymin, ymax) = y_range;
    let dx = (xmax - xmin) / nx as f64;
    let dy = (ymax - ymin) / ny as f64;

    let mut content = String::new();
    for j in 0..ny {
        for i in 0..nx {
            let x = xmin + (i as f64 + 0.5) * dx;
            let y = ymin + (j as f64 + 0.5) * dy;
            let val = func(x, y);
            if val > 0.0 {
                let x_lo = xmin + i as f64 * dx;
                let y_lo = ymin + j as f64 * dy;
                let (sx, sy) = map_to_svg(x_lo, y_lo, xmin, xmax, ymin, ymax, width, height);
                let (_, sy_lo) = map_to_svg(x_lo, y_lo + dy, xmin, xmax, ymin, ymax, width, height);
                let sw = (map_to_svg(x_lo + dx, y_lo, xmin, xmax, ymin, ymax, width, height).0
                    - sx)
                    .abs();
                let sh = (sy - sy_lo).abs();
                content.push_str(&format!(
                    "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\" fill-opacity=\"0.5\"/>\n",
                    sx, sy_lo, sw, sh, BLUE
                ));
            }
        }
    }

    Ok(svg_with_axes(
        &content, width, height, xmin, xmax, ymin, ymax, "", "",
    ))
}

// ── ContourPlot ────────────────────────────────────────────────────────────────

pub fn builtin_contour_plot(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::Error(
            "ContourPlot requires at least 3 arguments: f, {x, xmin, xmax}, {y, ymin, ymax}."
                .to_string(),
        ));
    }
    Ok(Value::Call {
        head: "ContourPlot".to_string(),
        args: args.to_vec(),
    })
}

/// ContourPlot with evaluation — called from eval.rs.
pub fn builtin_contour_plot_eval(
    func: &dyn Fn(f64, f64) -> f64,
    x_range: (f64, f64),
    y_range: (f64, f64),
    nx: usize,
    ny: usize,
    num_contours: usize,
    width: f64,
    height: f64,
) -> Result<String, EvalError> {
    let (xmin, xmax) = x_range;
    let (ymin, ymax) = y_range;

    let grid: Vec<Vec<f64>> = (0..=ny)
        .map(|j| {
            (0..=nx)
                .map(|i| {
                    let x = xmin + i as f64 * (xmax - xmin) / nx as f64;
                    let y = ymin + j as f64 * (ymax - ymin) / ny as f64;
                    func(x, y)
                })
                .collect()
        })
        .collect();

    let val_min = grid
        .iter()
        .flat_map(|r| r.iter())
        .cloned()
        .fold(f64::INFINITY, f64::min);
    let val_max = grid
        .iter()
        .flat_map(|r| r.iter())
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    let range = val_max - val_min;
    if range < 1e-15 {
        return Ok(svg_with_axes(
            "", width, height, xmin, xmax, ymin, ymax, "", "",
        ));
    }
    let num_contours = num_contours.max(3);
    let levels: Vec<f64> = (0..num_contours)
        .map(|i| val_min + (i as f64 + 0.5) * range / num_contours as f64)
        .collect();

    let mut content = String::new();
    let colors = ["#1a73e8", "#ea4335", "#34a853", "#4285f4", "#9c27b0"];

    for (li, &level) in levels.iter().enumerate() {
        let color = colors[li % colors.len()];
        marching_squares(
            &grid,
            xmin,
            xmax,
            ymin,
            ymax,
            level,
            &mut content,
            &color,
            width,
            height,
        );
    }

    Ok(svg_with_axes(
        &content, width, height, xmin, xmax, ymin, ymax, "", "",
    ))
}

fn marching_squares(
    grid: &[Vec<f64>],
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
    level: f64,
    content: &mut String,
    color: &str,
    width: f64,
    height: f64,
) {
    let ny = grid.len() - 1;
    let nx = grid[0].len() - 1;
    let dx = (xmax - xmin) / nx as f64;
    let dy = (ymax - ymin) / ny as f64;

    let interpolate = |v0: f64, v1: f64, t0: f64, t1: f64| -> f64 {
        if (v1 - v0).abs() < 1e-15 {
            t0
        } else {
            t0 + (level - v0) / (v1 - v0) * (t1 - t0)
        }
    };

    for j in 0..ny {
        for i in 0..nx {
            let v00 = grid[j][i];
            let v10 = grid[j][i + 1];
            let v01 = grid[j + 1][i];
            let v11 = grid[j + 1][i + 1];

            let x0 = xmin + i as f64 * dx;
            let x1 = x0 + dx;
            let y0 = ymin + j as f64 * dy;
            let y1 = y0 + dy;

            // Determine which corners are above the level
            let mut above = 0u8;
            if v00 >= level {
                above |= 1;
            }
            if v10 >= level {
                above |= 2;
            }
            if v11 >= level {
                above |= 4;
            }
            if v01 >= level {
                above |= 8;
            }

            if above == 0 || above == 15 {
                continue;
            }

            let top = (interpolate(v00, v10, x0, x1), y0);
            let right = (x1, interpolate(v10, v11, y0, y1));
            let bottom = (interpolate(v01, v11, x0, x1), y1);
            let left = (x0, interpolate(v00, v01, y0, y1));

            // Render individual line segments for each case
            let mut draw_segment = |a: (f64, f64), b: (f64, f64)| {
                let (sx1, sy1) = map_to_svg(a.0, a.1, xmin, xmax, ymin, ymax, width, height);
                let (sx2, sy2) = map_to_svg(b.0, b.1, xmin, xmax, ymin, ymax, width, height);
                content.push_str(&format!(
                    "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1.5\" stroke-opacity=\"0.8\"/>\n",
                    sx1, sy1, sx2, sy2, color
                ));
            };

            match above {
                1 => {
                    draw_segment(top, left);
                    draw_segment(left, bottom);
                }
                2 => {
                    draw_segment(top, left);
                    draw_segment(top, right);
                }
                3 => {
                    draw_segment(left, bottom);
                    draw_segment(top, right);
                }
                4 => {
                    draw_segment(right, left);
                    draw_segment(right, bottom);
                }
                5 => {
                    draw_segment(top, left);
                    draw_segment(right, bottom);
                }
                6 => {
                    draw_segment(left, bottom);
                    draw_segment(right, bottom);
                    draw_segment(top, left);
                }
                7 => {
                    draw_segment(top, left);
                    draw_segment(right, left);
                }
                8 => {
                    draw_segment(top, right);
                    draw_segment(top, left);
                }
                9 => {
                    draw_segment(top, right);
                    draw_segment(bottom, left);
                }
                10 => {
                    draw_segment(top, right);
                    draw_segment(left, bottom);
                    draw_segment(right, bottom);
                }
                11 => {
                    draw_segment(bottom, left);
                    draw_segment(right, bottom);
                    draw_segment(top, right);
                }
                12 => {
                    draw_segment(left, bottom);
                    draw_segment(top, right);
                    draw_segment(right, left);
                }
                13 => {
                    draw_segment(top, left);
                    draw_segment(right, left);
                }
                14 => {
                    draw_segment(left, bottom);
                    draw_segment(right, left);
                }
                _ => {}
            }
        }
    }
}

// ── ColorFunction support ──────────────────────────────────────────────────────

/// Apply a color function to a normalized value, returning an SVG hex color.
pub fn apply_color_function(color_fn: &Value, t: f64) -> String {
    match color_fn {
        Value::Call { head: h, args } if h == "RGBColor" && args.len() >= 3 => {
            let r = to_f64(&args[0]).unwrap_or(0.0);
            let g = to_f64(&args[1]).unwrap_or(0.0);
            let b = to_f64(&args[2]).unwrap_or(0.0);
            format!(
                "#{:02x}{:02x}{:02x}",
                (r.clamp(0.0, 1.0) * 255.0).round() as u8,
                (g.clamp(0.0, 1.0) * 255.0).round() as u8,
                (b.clamp(0.0, 1.0) * 255.0).round() as u8
            )
        }
        _ => {
            let t = t.clamp(0.0, 1.0);
            let r = (t * 255.0).round() as u8;
            let g = ((1.0 - t.abs() * 2.0).max(0.0) * 255.0).round() as u8;
            let b = ((1.0 - t) * 255.0).round() as u8;
            format!("#{:02x}{:02x}{:02x}", r, g, b)
        }
    }
}

// ── Registration ─────────────────────────────────────────────────────────────

/// Register all Charting builtins.
pub fn register(env: &crate::env::Env) {
    use super::register_builtin;
    register_builtin(env, "BarChart", builtin_bar_chart);
    register_builtin(env, "BarChart3D", builtin_bar_chart_3d);
    register_builtin(env, "Histogram", builtin_histogram);
    register_builtin(env, "HistogramList", builtin_histogram_list);
    register_builtin(env, "PieChart", builtin_pie_chart);
    register_builtin(env, "DonutChart", builtin_donut_chart);
    register_builtin(env, "BubbleChart", builtin_bubble_chart);
    register_builtin(env, "WaterfallChart", builtin_waterfall_chart);
    register_builtin(env, "TreeMap", builtin_tree_map);
    register_builtin(env, "Dendrogram", builtin_dendrogram);
    register_builtin(env, "RegionPlot", builtin_region_plot);
    register_builtin(env, "ContourPlot", builtin_contour_plot);
}

/// Symbol names exported by the Charting package.
pub const SYMBOLS: &[&str] = &[
    "BarChart",
    "BarChart3D",
    "Histogram",
    "HistogramList",
    "PieChart",
    "DonutChart",
    "BubbleChart",
    "WaterfallChart",
    "TreeMap",
    "Dendrogram",
    "RegionPlot",
    "ContourPlot",
    "ChartLayout",
    "ChartElements",
    "BarOrigin",
    "ColorFunction",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn real_val(v: f64) -> Value {
        Value::Real(rug::Float::with_val(crate::value::DEFAULT_PRECISION, v))
    }

    fn int_val(v: i64) -> Value {
        Value::Integer(Integer::from(v))
    }

    #[test]
    fn test_palette_color() {
        assert_eq!(palette_color(0), "#4285F4");
        assert_eq!(palette_color(1), "#EA4335");
        assert_eq!(palette_color(6), "#9C27B0");
        assert_eq!(palette_color(15), "#607D8B");
    }

    #[test]
    fn test_to_f64() {
        assert!((to_f64(&int_val(42)).unwrap() - 42.0).abs() < 1e-12);
        assert!((to_f64(&real_val(3.14)).unwrap() - 3.14).abs() < 1e-12);
        assert!(to_f64(&Value::Str("abc".to_string())).is_none());
    }

    #[test]
    fn test_bar_chart_basic() {
        let data = Value::List(vec![int_val(10), int_val(20), int_val(30)]);
        let result = builtin_bar_chart(&[data]).unwrap();
        match &result {
            Value::Str(svg) => {
                assert!(svg.contains("<svg"), "Expected SVG output");
                assert!(svg.contains("<rect"), "Expected rect elements for bars");
                assert!(svg.contains("</svg>"), "Expected closing SVG tag");
            }
            _ => panic!("Expected string result, got {:?}", result.type_name()),
        }
    }

    #[test]
    fn test_bar_chart_xy_pairs() {
        let data = Value::List(vec![
            Value::List(vec![real_val(1.0), real_val(10.0)]),
            Value::List(vec![real_val(3.0), real_val(20.0)]),
            Value::List(vec![real_val(5.0), real_val(15.0)]),
        ]);
        let result = builtin_bar_chart(&[data]).unwrap();
        match &result {
            Value::Str(svg) => {
                assert!(
                    svg.contains("<rect"),
                    "Expected bars at specified x positions"
                );
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_bar_chart_3d_symbolic() {
        let data = Value::List(vec![int_val(1), int_val(2), int_val(3)]);
        let result = builtin_bar_chart_3d(&[data]).unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "Chart3D"));
    }

    #[test]
    fn test_bar_chart_empty() {
        let result = builtin_bar_chart(&[Value::List(vec![])]);
        assert!(result.is_err());
    }

    #[test]
    fn test_histogram_basic() {
        let data = Value::List(vec![
            int_val(1),
            int_val(2),
            int_val(2),
            int_val(3),
            int_val(3),
            int_val(3),
            int_val(4),
            int_val(5),
        ]);
        let result = builtin_histogram(&[data]).unwrap();
        match &result {
            Value::Str(svg) => {
                assert!(svg.contains("<rect"), "Expected bars");
                assert!(svg.contains("Count"), "Expected y-axis label");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_histogram_with_bin_count() {
        let data = Value::List(vec![
            real_val(1.5),
            real_val(2.3),
            real_val(2.7),
            real_val(3.1),
            real_val(4.0),
            real_val(4.5),
        ]);
        let result = builtin_histogram(&[data.clone(), int_val(3)]).unwrap();
        assert!(matches!(&result, Value::Str(svg) if svg.contains("<rect")));
    }

    #[test]
    fn test_histogram_with_custom_edges() {
        let data = Value::List(vec![real_val(1.0), real_val(2.0), real_val(3.0)]);
        let edges = Value::List(vec![
            real_val(0.0),
            real_val(1.5),
            real_val(3.0),
            real_val(4.0),
        ]);
        let result = builtin_histogram(&[data, edges]).unwrap();
        assert!(matches!(&result, Value::Str(svg) if svg.contains("<rect")));
    }

    #[test]
    fn test_pie_chart_basic() {
        let data = Value::List(vec![int_val(30), int_val(20), int_val(50)]);
        let result = builtin_pie_chart(&[data]).unwrap();
        match &result {
            Value::Str(svg) => {
                assert!(svg.contains("<path"), "Expected path elements");
                assert!(svg.contains("Z\""), "Expected closed paths");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_pie_chart_with_labels() {
        let data = Value::List(vec![
            Value::List(vec![int_val(30), Value::Str("A".to_string())]),
            Value::List(vec![int_val(20), Value::Str("B".to_string())]),
            Value::List(vec![int_val(50), Value::Str("C".to_string())]),
        ]);
        let result = builtin_pie_chart(&[data]).unwrap();
        match &result {
            Value::Str(svg) => {
                assert!(svg.contains("A"), "Expected label A");
                assert!(svg.contains("B"), "Expected label B");
                assert!(svg.contains("C"), "Expected label C");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_donut_chart_basic() {
        let data = Value::List(vec![int_val(40), int_val(30), int_val(30)]);
        let result = builtin_donut_chart(&[data]).unwrap();
        match &result {
            Value::Str(svg) => {
                assert!(svg.contains("<path"), "Expected path elements");
                assert!(svg.contains("A "), "Expected arc commands");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_bubble_chart_basic() {
        let data = Value::List(vec![
            Value::List(vec![real_val(1.0), real_val(2.0), real_val(0.5)]),
            Value::List(vec![real_val(3.0), real_val(4.0), real_val(1.5)]),
            Value::List(vec![real_val(5.0), real_val(1.0), real_val(1.0)]),
        ]);
        let result = builtin_bubble_chart(&[data]).unwrap();
        match &result {
            Value::Str(svg) => {
                assert!(svg.contains("<circle"), "Expected circles");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_bubble_chart_no_radius() {
        let data = Value::List(vec![
            Value::List(vec![real_val(1.0), real_val(2.0)]),
            Value::List(vec![real_val(3.0), real_val(4.0)]),
        ]);
        let result = builtin_bubble_chart(&[data]).unwrap();
        assert!(matches!(&result, Value::Str(svg) if svg.contains("<circle")));
    }

    #[test]
    fn test_waterfall_chart_basic() {
        let data = Value::List(vec![int_val(10), int_val(5), int_val(-3), int_val(8)]);
        let result = builtin_waterfall_chart(&[data]).unwrap();
        match &result {
            Value::Str(svg) => {
                assert!(svg.contains("<rect"), "Expected bars");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_tree_map_symbolic() {
        let result = builtin_tree_map(&[]).unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "TreeMap"));
    }

    #[test]
    fn test_dendrogram_symbolic() {
        let result = builtin_dendrogram(&[]).unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "Dendrogram"));
    }

    #[test]
    fn test_region_plot_symbolic() {
        let result = builtin_region_plot(&[
            Value::Symbol("f".to_string()),
            Value::List(vec![
                Value::Symbol("x".to_string()),
                int_val(-1),
                int_val(1),
            ]),
            Value::List(vec![
                Value::Symbol("y".to_string()),
                int_val(-1),
                int_val(1),
            ]),
        ])
        .unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "RegionPlot"));
    }

    #[test]
    fn test_contour_plot_symbolic() {
        let result = builtin_contour_plot(&[
            Value::Symbol("f".to_string()),
            Value::List(vec![
                Value::Symbol("x".to_string()),
                int_val(0),
                int_val(10),
            ]),
            Value::List(vec![
                Value::Symbol("y".to_string()),
                int_val(0),
                int_val(10),
            ]),
        ])
        .unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "ContourPlot"));
    }

    #[test]
    fn test_region_plot_eval() {
        let func = |x: f64, y: f64| -> f64 { 1.0 - (x * x + y * y) };
        let svg = builtin_region_plot_eval(&func, (-1.5, 1.5), (-1.5, 1.5), 50, 50, 400.0, 400.0)
            .unwrap();
        assert!(svg.contains("<rect"), "Expected filled rectangles");
        assert!(svg.contains("</svg>"), "Expected SVG wrapper");
    }

    #[test]
    fn test_contour_plot_eval() {
        let func = |x: f64, y: f64| -> f64 { (x * x + y * y).sqrt() };
        let svg =
            builtin_contour_plot_eval(&func, (0.01, 10.0), (0.01, 10.0), 50, 50, 5, 400.0, 400.0)
                .unwrap();
        assert!(svg.contains("<line"), "Expected contour lines");
        assert!(svg.contains("</svg>"), "Expected SVG wrapper");
    }

    #[test]
    fn test_nice_step() {
        assert!((nice_step(10.0) - 2.0).abs() < 1e-10);
        assert!((nice_step(1.0) - 0.2).abs() < 1e-10);
        assert!((nice_step(100.0) - 20.0).abs() < 1e-10);
    }

    #[test]
    fn test_parse_chart_options() {
        let mut map = HashMap::new();
        map.insert(
            "ImageSize".to_string(),
            Value::List(vec![int_val(800), int_val(600)]),
        );
        let opts = Value::Assoc(map);
        let (w, h, _) = parse_chart_options(&opts);
        assert!((w - 800.0).abs() < 1e-12);
        assert!((h - 600.0).abs() < 1e-12);
    }

    #[test]
    fn test_apply_color_function_rgb() {
        let rgb = Value::Call {
            head: "RGBColor".to_string(),
            args: vec![real_val(1.0), real_val(0.0), real_val(0.0)],
        };
        assert_eq!(apply_color_function(&rgb, 0.5), "#ff0000");
    }

    #[test]
    fn test_apply_color_function_default() {
        let result = apply_color_function(&Value::Symbol("Gradient".to_string()), 0.5);
        assert!(
            result.starts_with('#'),
            "Expected hex color, got {}",
            result
        );
    }

    #[test]
    fn test_auto_bins() {
        assert!(auto_bins(1) >= 1);
        assert!(auto_bins(10) >= 4);
        assert!(auto_bins(100) >= 7);
    }

    #[test]
    fn test_get_list_error() {
        let result = get_list(&Value::Integer(Integer::from(1)));
        assert!(result.is_err());
    }

    #[test]
    fn test_svg_with_axes() {
        let svg = svg_with_axes("", 400.0, 300.0, 0.0, 10.0, 0.0, 100.0, "", "");
        assert!(svg.contains("<svg"), "Expected SVG open tag");
        assert!(svg.contains("</svg>"), "Expected SVG close tag");
        assert!(svg.contains("<line"), "Expected axis lines");
        assert!(svg.contains("<text"), "Expected tick labels");
    }

    #[test]
    fn test_svg_with_axes_labels() {
        let svg = svg_with_axes(
            "", 400.0, 300.0, 0.0, 10.0, 0.0, 100.0, "X Label", "Y Label",
        );
        assert!(svg.contains("X Label"), "Expected X axis label");
        assert!(svg.contains("Y Label"), "Expected Y axis label");
    }
}
