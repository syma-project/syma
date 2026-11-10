//! Charting package builtins.
//!
//! Provides SVG-based chart primitives: BarChart, Histogram, PieChart,
//! DonutChart, BubbleChart, WaterfallChart, RegionPlot, and ContourPlot.

use crate::value::{EvalError, Value};

// ── Color Palette ──────────────────────────────────────────────────────────────

const DEFAULT_PALETTE: &[&str] = &[
    "#4285F4", "#EA4335", "#FBBC05", "#34A853", "#FF6D01", "#46BDC6", "#9C27B0", "#795548",
    "#F44336", "#E91E63", "#9E9E9E", "#00BCD4", "#FF5722", "#8BC34A", "#CDDC39", "#607D8B",
];

/// Get a color from the palette by index (wraps around).
fn palette_color(i: usize) -> String {
    DEFAULT_PALETTE[i % DEFAULT_PALETTE.len()].to_string()
}

// ── Helpers ────────────────────────────────────────────────────────────────────

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

/// Escape text for safe inclusion in SVG content/attributes.
fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Widen a degenerate `[lo, hi]` interval so coordinate transforms stay finite.
fn guard_range(lo: f64, hi: f64) -> (f64, f64) {
    if !lo.is_finite() || !hi.is_finite() {
        return (-1.0, 1.0);
    }
    let (lo, hi) = if hi < lo { (hi, lo) } else { (lo, hi) };
    if (hi - lo).abs() < 1e-12 {
        let c = (lo + hi) / 2.0;
        (c - 1.0, c + 1.0)
    } else {
        (lo, hi)
    }
}

/// Build a complete SVG wrapper with axes for cartesian charts.
#[allow(clippy::too_many_arguments)]
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
    grid_lines: bool,
) -> String {
    // Guard against degenerate spans so the coordinate transform is finite.
    let (x_min, x_max) = guard_range(x_min, x_max);
    let (y_min, y_max) = guard_range(y_min, y_max);

    let plot_w = width - MARGIN_LEFT - MARGIN_RIGHT;
    let plot_h = height - MARGIN_TOP - MARGIN_BOTTOM;

    let tx = |x: f64| MARGIN_LEFT + (x - x_min) / (x_max - x_min) * plot_w;
    let ty = |y: f64| MARGIN_TOP + (1.0 - (y - y_min) / (y_max - y_min)) * plot_h;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" style=\"background:white\">\n",
        width as i32, height as i32, width as i32, height as i32
    ));

    if grid_lines {
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
            escape_xml(x_label)
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
            escape_xml(y_label)
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
#[allow(clippy::too_many_arguments)]
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
    let (x_min, x_max) = guard_range(x_min, x_max);
    let (y_min, y_max) = guard_range(y_min, y_max);
    let plot_w = width - MARGIN_LEFT - MARGIN_RIGHT;
    let plot_h = height - MARGIN_TOP - MARGIN_BOTTOM;
    let sx = MARGIN_LEFT + (x - x_min) / (x_max - x_min) * plot_w;
    let sy = MARGIN_TOP + (1.0 - (y - y_min) / (y_max - y_min)) * plot_h;
    (sx, sy)
}

/// True if a trailing argument is an options carrier (rule or association).
fn is_option_value(v: &Value) -> bool {
    matches!(v, Value::Rule { .. } | Value::Assoc(_))
}

/// Extract chart options from an Assoc value.
fn parse_chart_options(opts: &Value) -> (f64, f64, bool) {
    let mut width = DEFAULT_WIDTH;
    let mut height = DEFAULT_HEIGHT;
    let mut grid_lines = false;

    if let Value::Assoc(map) = opts {
        if let Some(v) = map.get("ImageSize")
            && let Value::List(size) = v
            && size.len() >= 2
        {
            width = super::to_f64(&size[0])
                .filter(|w| w.is_finite() && *w > 0.0)
                .unwrap_or(DEFAULT_WIDTH);
            height = super::to_f64(&size[1])
                .filter(|h| h.is_finite() && *h > 0.0)
                .unwrap_or(DEFAULT_HEIGHT);
        }
        if let Some(v) = map.get("GridLines") {
            grid_lines = match v {
                Value::Bool(b) => *b,
                Value::Symbol(s) => s == "True",
                Value::List(_) => true,
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

    let options = super::graphics::merge_options(&args[1..]);
    let (width, height, grid_lines) = parse_chart_options(&options);

    // Parse data as either {v1, v2, ...} or {{x1, y1}, {x2, y2}, ...}
    let bars: Vec<(f64, f64)> = match &data[0] {
        Value::List(pair) if pair.len() >= 2 => {
            let mut result = Vec::new();
            for item in data {
                if let Value::List(p) = item
                    && let (Some(x), Some(y)) = (super::to_f64(&p[0]), super::to_f64(&p[1]))
                {
                    result.push((x, y));
                }
            }
            result
        }
        _ => data
            .iter()
            .enumerate()
            .filter_map(|(i, v)| super::to_f64(v).map(|y| (i as f64 + 1.0, y)))
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
        grid_lines,
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

    // Trailing option args start after the optional bin spec.
    let opt_start = if args.len() >= 2 && !is_option_value(&args[1]) {
        2
    } else {
        1
    };
    let options = super::graphics::merge_options(&args[opt_start..]);
    let (width, height, grid_lines) = parse_chart_options(&options);

    let nums: Vec<f64> = data.iter().filter_map(super::to_f64).collect();
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
                let edge_vals: Vec<f64> = edges.iter().filter_map(super::to_f64).collect();
                if edge_vals.len() < 2 {
                    return Err(EvalError::Error(
                        "Histogram: need at least 2 bin edges".to_string(),
                    ));
                }
                return build_histogram_svg(&nums, &edge_vals, width, height, grid_lines);
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

    build_histogram_svg(&nums, &edges, width, height, grid_lines)
}

fn build_histogram_svg(
    nums: &[f64],
    edges: &[f64],
    width: f64,
    height: f64,
    grid_lines: bool,
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
        &content, width, height, x_min, x_max, 0.0, y_max, "", "Count", grid_lines,
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

    let options = super::graphics::merge_options(&args[1..]);
    let (width, height, _grid_lines) = parse_chart_options(&options);

    struct PieSlice {
        value: f64,
        label: Option<String>,
    }

    let slices: Vec<PieSlice> = match &data[0] {
        Value::List(pair) if pair.len() >= 2 => {
            let mut result = Vec::new();
            for item in data {
                if let Value::List(p) = item {
                    let val = super::to_f64(&p[0]).unwrap_or(0.0);
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
                super::to_f64(v).map(|val| PieSlice {
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

    let options = super::graphics::merge_options(&args[1..]);
    let (width, height, _grid_lines) = parse_chart_options(&options);

    let inner_ratio = if let Value::Assoc(map) = &options {
        if let Some(v) = map.get("InnerRadius") {
            super::to_f64(v).unwrap_or(0.4)
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
                    let val = super::to_f64(&p[0]).unwrap_or(0.0);
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
                super::to_f64(v).map(|val| PieSlice {
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

    let options = super::graphics::merge_options(&args[1..]);
    let (width, height, grid_lines) = parse_chart_options(&options);

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
                    let x = super::to_f64(&p[0])?;
                    let y = super::to_f64(&p[1])?;
                    let r = if p.len() >= 3 {
                        super::to_f64(&p[2]).unwrap_or(1.0)
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
        &content, width, height, x_min_p, x_max_p, y_min_p, y_max_p, "", "", grid_lines,
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

    let options = super::graphics::merge_options(&args[1..]);
    let (width, height, grid_lines) = parse_chart_options(&options);

    let values: Vec<f64> = data.iter().filter_map(super::to_f64).collect();
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
        &content, width, height, x_min_p, x_max_p, y_min_p, y_max_p, "", "", grid_lines,
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
    if args.len() < 3 {
        return Err(EvalError::Error(
            "RegionPlot requires at least 3 arguments: REGION, {x, xmin, xmax}, {y, ymin, ymax}."
                .to_string(),
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
        &content, width, height, xmin, xmax, ymin, ymax, "", "", false,
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
#[allow(clippy::too_many_arguments)]
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
            "", width, height, xmin, xmax, ymin, ymax, "", "", false,
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
            color,
            width,
            height,
        );
    }

    Ok(svg_with_axes(
        &content, width, height, xmin, xmax, ymin, ymax, "", "", false,
    ))
}

#[allow(clippy::too_many_arguments)]
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
            let r = super::to_f64(&args[0]).unwrap_or(0.0);
            let g = super::to_f64(&args[1]).unwrap_or(0.0);
            let b = super::to_f64(&args[2]).unwrap_or(0.0);
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

// ── Extended chart helpers ─────────────────────────────────────────────────────

/// Parse a value as an `(x, y)` pair: accepts `{x, y}` lists or bare numbers
/// (bare numbers receive an automatic 1-based index as their x coordinate).
fn to_xy_pairs(data: &[Value]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let pairs = matches!(data.first(), Some(Value::List(p)) if p.len() >= 2);
    for (i, item) in data.iter().enumerate() {
        if pairs {
            if let Value::List(p) = item
                && let (Some(x), Some(y)) = (super::to_f64(&p[0]), super::to_f64(&p[1]))
            {
                out.push((x, y));
            }
        } else if let Some(y) = super::to_f64(item) {
            out.push((i as f64 + 1.0, y));
        }
    }
    out
}

/// Parse a value as a complex number: accepts `Complex`, `{re, im}` or a scalar.
fn to_complex(v: &Value) -> Option<(f64, f64)> {
    match v {
        Value::Complex { re, im } => Some((*re, *im)),
        Value::List(l) if l.len() >= 2 => Some((super::to_f64(&l[0])?, super::to_f64(&l[1])?)),
        other => super::to_f64(other).map(|x| (x, 0.0)),
    }
}

/// Bounds of a point set with small relative padding on each axis.
fn padded_bounds(points: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let x_min = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let x_max = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let y_min = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let y_max = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let xr = if (x_max - x_min).abs() > 1e-12 {
        x_max - x_min
    } else {
        2.0
    };
    let yr = if (y_max - y_min).abs() > 1e-12 {
        y_max - y_min
    } else {
        2.0
    };
    (
        x_min - xr * 0.06,
        x_max + xr * 0.06,
        y_min - yr * 0.08,
        y_max + yr * 0.08,
    )
}

/// Render a series of points as an optional polyline plus optional markers.
#[allow(clippy::too_many_arguments)]
fn render_series(
    points: &[(f64, f64)],
    bounds: (f64, f64, f64, f64),
    width: f64,
    height: f64,
    color: &str,
    connect: bool,
    markers: bool,
) -> String {
    let (x_min, x_max, y_min, y_max) = bounds;
    let mut content = String::new();

    if connect && points.len() >= 2 {
        let coords: Vec<String> = points
            .iter()
            .map(|&(x, y)| {
                let (sx, sy) = map_to_svg(x, y, x_min, x_max, y_min, y_max, width, height);
                format!("{:.1},{:.1}", sx, sy)
            })
            .collect();
        content.push_str(&format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"2\"/>\n",
            coords.join(" "),
            color
        ));
    }

    if markers {
        for &(x, y) in points {
            let (sx, sy) = map_to_svg(x, y, x_min, x_max, y_min, y_max, width, height);
            content.push_str(&format!(
                "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"3\" fill=\"{}\"/>\n",
                sx, sy, color
            ));
        }
    }

    content
}

/// Apply optional per-axis base-10 log transforms, dropping non-positive samples.
fn log_transform(points: &[(f64, f64)], xlog: bool, ylog: bool) -> Vec<(f64, f64)> {
    points
        .iter()
        .filter_map(|&(x, y)| {
            if xlog && x <= 0.0 {
                return None;
            }
            if ylog && y <= 0.0 {
                return None;
            }
            Some((
                if xlog { x.log10() } else { x },
                if ylog { y.log10() } else { y },
            ))
        })
        .collect()
}

/// Convert a color value (string, RGBColor, RGB triple or named color) to hex.
fn color_value_to_hex(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Symbol(s) => named_color(s).unwrap_or("#888888").to_string(),
        Value::Call { head, .. } if head == "RGBColor" => apply_color_function(v, 0.5),
        Value::List(l) if l.len() >= 3 => {
            let r = super::to_f64(&l[0]).unwrap_or(0.0);
            let g = super::to_f64(&l[1]).unwrap_or(0.0);
            let b = super::to_f64(&l[2]).unwrap_or(0.0);
            format!(
                "#{:02x}{:02x}{:02x}",
                (r.clamp(0.0, 1.0) * 255.0).round() as u8,
                (g.clamp(0.0, 1.0) * 255.0).round() as u8,
                (b.clamp(0.0, 1.0) * 255.0).round() as u8
            )
        }
        _ => "#888888".to_string(),
    }
}

/// Resolve a small set of named colors to hex.
fn named_color(name: &str) -> Option<&'static str> {
    Some(match name {
        "Red" => "#ff0000",
        "Green" => "#008000",
        "Blue" => "#0000ff",
        "Black" => "#000000",
        "White" => "#ffffff",
        "Orange" => "#ffa500",
        "Purple" => "#800080",
        "Cyan" => "#00ffff",
        "Magenta" => "#ff00ff",
        "Yellow" => "#ffff00",
        "Gray" | "Grey" => "#808080",
        _ => return None,
    })
}

/// Convert an HSV triple to a hex color.
fn hsv_to_hex(h: f64, s: f64, v: f64) -> String {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    format!(
        "#{:02x}{:02x}{:02x}",
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8
    )
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> f64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe - 719468) as f64
}

/// Parse a date-like value (`{y, m, d, ...}`, `"YYYY-MM-DD"` or a number) to a
/// serial day number.
fn date_to_serial(v: &Value) -> Option<f64> {
    match v {
        Value::List(parts) if !parts.is_empty() => {
            let nums: Vec<f64> = parts
                .iter()
                .map(super::to_f64)
                .collect::<Option<Vec<_>>>()?;
            if nums.len() < 3 {
                return None;
            }
            let base = days_from_civil(nums[0] as i64, nums[1] as i64, nums[2] as i64);
            let frac = nums.get(3).copied().unwrap_or(0.0) / 24.0
                + nums.get(4).copied().unwrap_or(0.0) / 1440.0
                + nums.get(5).copied().unwrap_or(0.0) / 86400.0;
            Some(base + frac)
        }
        Value::Str(s) => {
            let sep = if s.contains('-') { '-' } else { '/' };
            let parts: Vec<i64> = s
                .split(sep)
                .filter_map(|p| p.trim().parse::<i64>().ok())
                .collect();
            if parts.len() < 3 {
                return None;
            }
            Some(days_from_civil(parts[0], parts[1], parts[2]))
        }
        other => super::to_f64(other),
    }
}

/// Linear-interpolated quantile (type-7) of an already sorted slice.
fn quantile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let h = (sorted.len() - 1) as f64 * p.clamp(0.0, 1.0);
    let lo = h.floor() as usize;
    let hi = h.ceil() as usize;
    let frac = h - lo as f64;
    sorted[lo] + (sorted[hi] - sorted[lo]) * frac
}

/// Parse a rectangular numeric grid, rejecting ragged input.
fn parse_grid(v: &Value) -> Result<Vec<Vec<f64>>, EvalError> {
    let rows = get_list(v)?;
    if rows.is_empty() {
        return Err(EvalError::Error("expected a non-empty 2D grid".to_string()));
    }
    let mut grid = Vec::with_capacity(rows.len());
    let mut width = 0usize;
    for row in rows {
        let cells = get_list(row)?;
        let nums: Vec<f64> = cells.iter().filter_map(super::to_f64).collect();
        if nums.len() != cells.len() {
            return Err(EvalError::Error(
                "grid must contain only numbers".to_string(),
            ));
        }
        if width == 0 {
            width = nums.len();
        } else if nums.len() != width {
            return Err(EvalError::Error(
                "grid rows must all have the same length".to_string(),
            ));
        }
        grid.push(nums);
    }
    Ok(grid)
}

/// Extract the `ColorFunction` option, if present and not `Automatic`.
fn option_color_function(opts: &Value) -> Option<Value> {
    if let Value::Assoc(map) = opts
        && let Some(cf) = map.get("ColorFunction")
        && !matches!(cf, Value::Symbol(s) if s == "Automatic")
    {
        return Some(cf.clone());
    }
    None
}

/// Pick a color for a normalized value using a color function or default ramp.
fn ramp_color(color_fn: &Option<Value>, t: f64) -> String {
    match color_fn {
        Some(cf) => apply_color_function(cf, t),
        None => apply_color_function(&Value::Symbol("Gradient".to_string()), t),
    }
}

// ── DateListPlot ───────────────────────────────────────────────────────────────

pub fn builtin_date_list_plot(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "DateListPlot requires at least 1 argument".to_string(),
        ));
    }
    let data = get_list(&args[0])?;
    if data.is_empty() {
        return Err(EvalError::Error(
            "DateListPlot: data must not be empty".to_string(),
        ));
    }
    let options = super::graphics::merge_options(&args[1..]);
    let (width, height, grid_lines) = parse_chart_options(&options);

    let points: Vec<(f64, f64)> = data
        .iter()
        .filter_map(|item| {
            if let Value::List(p) = item
                && p.len() >= 2
            {
                let x = date_to_serial(&p[0])?;
                let y = super::to_f64(&p[1])?;
                Some((x, y))
            } else {
                None
            }
        })
        .collect();

    if points.is_empty() {
        return Err(EvalError::Error(
            "DateListPlot: no valid {date, value} points".to_string(),
        ));
    }

    let bounds = padded_bounds(&points);
    let content = render_series(&points, bounds, width, height, "#1a73e8", true, true);
    Ok(Value::Str(svg_with_axes(
        &content, width, height, bounds.0, bounds.1, bounds.2, bounds.3, "Date", "", grid_lines,
    )))
}

// ── BoxWhiskerChart ────────────────────────────────────────────────────────────

pub fn builtin_box_whisker_chart(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "BoxWhiskerChart requires at least 1 argument".to_string(),
        ));
    }
    let data = get_list(&args[0])?;
    if data.is_empty() {
        return Err(EvalError::Error(
            "BoxWhiskerChart: data must not be empty".to_string(),
        ));
    }
    let options = super::graphics::merge_options(&args[1..]);
    let (width, height, grid_lines) = parse_chart_options(&options);

    // A list of lists yields multiple boxes; otherwise treat as a single dataset.
    let multiple = matches!(data.first(), Some(Value::List(_)));
    let mut datasets: Vec<Vec<f64>> = Vec::new();
    if multiple {
        for item in data {
            if let Value::List(vals) = item {
                let nums: Vec<f64> = vals.iter().filter_map(super::to_f64).collect();
                if !nums.is_empty() {
                    datasets.push(nums);
                }
            }
        }
    } else {
        let nums: Vec<f64> = data.iter().filter_map(super::to_f64).collect();
        if !nums.is_empty() {
            datasets.push(nums);
        }
    }

    if datasets.is_empty() {
        return Err(EvalError::Error(
            "BoxWhiskerChart: no numeric data".to_string(),
        ));
    }

    let all_min = datasets
        .iter()
        .flat_map(|d| d.iter())
        .cloned()
        .fold(f64::INFINITY, f64::min);
    let all_max = datasets
        .iter()
        .flat_map(|d| d.iter())
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);
    let span = if (all_max - all_min).abs() > 1e-12 {
        all_max - all_min
    } else {
        2.0
    };
    let y_min = all_min - span * 0.1;
    let y_max = all_max + span * 0.1;
    let x_min = 0.4_f64;
    let x_max = datasets.len() as f64 + 0.6;

    let mut content = String::new();
    let box_w = 0.5_f64;

    for (i, dataset) in datasets.iter().enumerate() {
        let mut sorted = dataset.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let q1 = quantile(&sorted, 0.25);
        let median = quantile(&sorted, 0.5);
        let q3 = quantile(&sorted, 0.75);
        let iqr = q3 - q1;
        let low_fence = q1 - 1.5 * iqr;
        let high_fence = q3 + 1.5 * iqr;
        let whisker_lo = sorted
            .iter()
            .cloned()
            .filter(|&v| v >= low_fence)
            .fold(f64::INFINITY, f64::min);
        let whisker_hi = sorted
            .iter()
            .cloned()
            .filter(|&v| v <= high_fence)
            .fold(f64::NEG_INFINITY, f64::max);
        let color = palette_color(i);
        let cx = i as f64 + 1.0;

        let mapx = |x: f64| map_to_svg(x, 0.0, x_min, x_max, y_min, y_max, width, height).0;
        let mapy = |y: f64| map_to_svg(0.0, y, x_min, x_max, y_min, y_max, width, height).1;

        let bx = mapx(cx - box_w / 2.0);
        let bw = (mapx(cx + box_w / 2.0) - bx).abs();
        let by = mapy(q3);
        let bh = (mapy(q1) - by).abs().max(1.0);
        content.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\" fill-opacity=\"0.5\" stroke=\"{}\" stroke-width=\"1.5\"/>\n",
            bx, by, bw, bh, color, color
        ));

        let mid = mapx(cx);
        content.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"2\"/>\n",
            mapx(cx - box_w / 2.0),
            mapy(median),
            mapx(cx + box_w / 2.0),
            mapy(median),
            color
        ));

        if whisker_hi.is_finite() {
            content.push_str(&format!(
                "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1.5\"/>\n",
                mid, mapy(q3), mid, mapy(whisker_hi), color
            ));
            content.push_str(&format!(
                "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1.5\"/>\n",
                mapx(cx - box_w / 4.0), mapy(whisker_hi), mapx(cx + box_w / 4.0), mapy(whisker_hi), color
            ));
        }
        if whisker_lo.is_finite() {
            content.push_str(&format!(
                "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1.5\"/>\n",
                mid, mapy(q1), mid, mapy(whisker_lo), color
            ));
            content.push_str(&format!(
                "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1.5\"/>\n",
                mapx(cx - box_w / 4.0), mapy(whisker_lo), mapx(cx + box_w / 4.0), mapy(whisker_lo), color
            ));
        }

        for &v in &sorted {
            if v < low_fence || v > high_fence {
                content.push_str(&format!(
                    "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2.5\" fill=\"{}\" fill-opacity=\"0.8\"/>\n",
                    mid, mapy(v), color
                ));
            }
        }
    }

    Ok(Value::Str(svg_with_axes(
        &content, width, height, x_min, x_max, y_min, y_max, "", "", grid_lines,
    )))
}

// ── ComplexPlot ────────────────────────────────────────────────────────────────

pub fn builtin_complex_plot(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "ComplexPlot requires at least 1 argument".to_string(),
        ));
    }

    // A list of samples is rendered directly; a function symbol is returned
    // unevaluated for the evaluator to handle.
    let Some(data) = (match &args[0] {
        Value::List(items) if !items.is_empty() => Some(items),
        _ => None,
    }) else {
        return Ok(Value::Call {
            head: "ComplexPlot".to_string(),
            args: args.to_vec(),
        });
    };

    let options = super::graphics::merge_options(&args[1..]);
    let (width, height, grid_lines) = parse_chart_options(&options);

    struct CPoint {
        re: f64,
        im: f64,
    }
    let mut points: Vec<CPoint> = Vec::new();
    for item in data {
        if let Some((re, im)) = to_complex(item)
            && re.is_finite()
            && im.is_finite()
        {
            points.push(CPoint { re, im });
        }
    }
    if points.is_empty() {
        return Err(EvalError::Error(
            "ComplexPlot: no valid complex data points".to_string(),
        ));
    }

    let xy: Vec<(f64, f64)> = points.iter().map(|p| (p.re, p.im)).collect();
    let bounds = padded_bounds(&xy);

    let mut content = String::new();
    for p in &points {
        let phase = p.im.atan2(p.re).rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU;
        let color = hsv_to_hex(phase, 0.85, 0.9);
        let (sx, sy) = map_to_svg(
            p.re, p.im, bounds.0, bounds.1, bounds.2, bounds.3, width, height,
        );
        content.push_str(&format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"3.5\" fill=\"{}\" stroke=\"{}\" stroke-width=\"0.75\"/>\n",
            sx, sy, color, DARK_TEXT
        ));
    }

    Ok(Value::Str(svg_with_axes(
        &content, width, height, bounds.0, bounds.1, bounds.2, bounds.3, "Re", "Im", grid_lines,
    )))
}

// ── ListLogPlot / ListLogLogPlot ───────────────────────────────────────────────

fn log_plot_impl(name: &str, args: &[Value], xlog: bool, ylog: bool) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(format!(
            "{} requires at least 1 argument",
            name
        )));
    }
    let data = get_list(&args[0])?;
    if data.is_empty() {
        return Err(EvalError::Error(format!(
            "{}: data must not be empty",
            name
        )));
    }
    let options = super::graphics::merge_options(&args[1..]);
    let (width, height, grid_lines) = parse_chart_options(&options);

    let raw = to_xy_pairs(data);
    let points = log_transform(&raw, xlog, ylog);
    if points.is_empty() {
        return Err(EvalError::Error(format!(
            "{}: no positive data points for log scale",
            name
        )));
    }

    let bounds = padded_bounds(&points);
    let content = render_series(&points, bounds, width, height, "#1a73e8", true, true);
    Ok(Value::Str(svg_with_axes(
        &content,
        width,
        height,
        bounds.0,
        bounds.1,
        bounds.2,
        bounds.3,
        if xlog { "Log x" } else { "" },
        if ylog { "Log y" } else { "" },
        grid_lines,
    )))
}

pub fn builtin_list_log_plot(args: &[Value]) -> Result<Value, EvalError> {
    log_plot_impl("ListLogPlot", args, false, true)
}

pub fn builtin_list_log_log_plot(args: &[Value]) -> Result<Value, EvalError> {
    log_plot_impl("ListLogLogPlot", args, true, true)
}

// ── ListPolarPlot ──────────────────────────────────────────────────────────────

pub fn builtin_list_polar_plot(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "ListPolarPlot requires at least 1 argument".to_string(),
        ));
    }
    let data = get_list(&args[0])?;
    if data.is_empty() {
        return Err(EvalError::Error(
            "ListPolarPlot: data must not be empty".to_string(),
        ));
    }
    let options = super::graphics::merge_options(&args[1..]);
    let (width, height, grid_lines) = parse_chart_options(&options);

    // Each item is {r, theta}; a bare number is a radius with zero angle steps.
    let mut theta_acc = 0.0_f64;
    let mut points: Vec<(f64, f64)> = Vec::new();
    for item in data {
        let (r, theta) = if let Value::List(p) = item {
            if p.len() < 2 {
                continue;
            }
            match (super::to_f64(&p[0]), super::to_f64(&p[1])) {
                (Some(r), Some(t)) => (r, t),
                _ => continue,
            }
        } else if let Some(r) = super::to_f64(item) {
            theta_acc += std::f64::consts::TAU / data.len() as f64;
            (r, theta_acc)
        } else {
            continue;
        };
        points.push((r * theta.cos(), r * theta.sin()));
    }
    if points.is_empty() {
        return Err(EvalError::Error(
            "ListPolarPlot: no valid {r, theta} points".to_string(),
        ));
    }

    let bounds = padded_bounds(&points);
    let content = render_series(&points, bounds, width, height, "#4285F4", true, true);
    Ok(Value::Str(svg_with_axes(
        &content, width, height, bounds.0, bounds.1, bounds.2, bounds.3, "", "", grid_lines,
    )))
}

// ── ListStreamPlot ─────────────────────────────────────────────────────────────

pub fn builtin_list_stream_plot(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "ListStreamPlot requires at least 1 argument".to_string(),
        ));
    }
    let data = get_list(&args[0])?;
    if data.is_empty() {
        return Err(EvalError::Error(
            "ListStreamPlot: data must not be empty".to_string(),
        ));
    }
    let options = super::graphics::merge_options(&args[1..]);
    let (width, height, grid_lines) = parse_chart_options(&options);

    struct Field {
        x: f64,
        y: f64,
        vx: f64,
        vy: f64,
    }
    let mut field: Vec<Field> = Vec::new();
    for item in data {
        if let Value::List(pair) = item
            && pair.len() >= 2
            && let (Value::List(pos), Value::List(vec)) = (&pair[0], &pair[1])
            && pos.len() >= 2
            && vec.len() >= 2
            && let (Some(x), Some(y), Some(vx), Some(vy)) = (
                super::to_f64(&pos[0]),
                super::to_f64(&pos[1]),
                super::to_f64(&vec[0]),
                super::to_f64(&vec[1]),
            )
        {
            field.push(Field { x, y, vx, vy });
        }
    }
    if field.is_empty() {
        return Err(EvalError::Error(
            "ListStreamPlot: no valid {{x, y}, {vx, vy}} samples".to_string(),
        ));
    }

    let xy: Vec<(f64, f64)> = field.iter().map(|f| (f.x, f.y)).collect();
    let bounds = padded_bounds(&xy);
    let (bx0, bx1, by0, by1) = bounds;
    let spacing = ((bx1 - bx0).min(by1 - by0) / (field.len() as f64).sqrt()).max(1e-6);
    let arrow_len = spacing * 0.7;

    let scale = |v: f64| (v - 1.0) / 4.0;

    let mut content = String::new();
    for f in &field {
        let mag = (f.vx * f.vx + f.vy * f.vy).sqrt();
        if mag < 1e-12 {
            continue;
        }
        let ux = f.vx / mag;
        let uy = f.vy / mag;
        let ex = f.x + ux * arrow_len;
        let ey = f.y + uy * arrow_len;
        let (sx1, sy1) = map_to_svg(f.x, f.y, bx0, bx1, by0, by1, width, height);
        let (sx2, sy2) = map_to_svg(ex, ey, bx0, bx1, by0, by1, width, height);
        let color = ramp_color(&None, scale(mag));
        content.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1.2\"/>\n",
            sx1, sy1, sx2, sy2, color
        ));
        // Arrow head: two short lines back from the tip.
        let head = (arrow_len * 0.35).max(1.0);
        let angle = (sy2 - sy1).atan2(sx2 - sx1);
        for da in [angle + 2.6, angle - 2.6] {
            let hx = sx2 + head * da.cos();
            let hy = sy2 + head * da.sin();
            content.push_str(&format!(
                "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1.2\"/>\n",
                sx2, sy2, hx, hy, color
            ));
        }
    }

    Ok(Value::Str(svg_with_axes(
        &content, width, height, bx0, bx1, by0, by1, "", "", grid_lines,
    )))
}

// ── ListContourPlot ────────────────────────────────────────────────────────────

pub fn builtin_list_contour_plot(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "ListContourPlot requires at least 1 argument".to_string(),
        ));
    }
    let grid = parse_grid(&args[0])?;
    let options = super::graphics::merge_options(&args[1..]);
    let (width, height, grid_lines) = parse_chart_options(&options);

    let ny = grid.len();
    let nx = grid[0].len();
    let x_min = 1.0_f64;
    let x_max = nx as f64;
    let y_min = 1.0_f64;
    let y_max = ny as f64;

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

    let mut content = String::new();
    if range > 1e-15 && nx >= 2 && ny >= 2 {
        let num_levels = 5usize;
        let colors = ["#1a73e8", "#ea4335", "#34a853", "#ff6d01", "#9c27b0"];
        for li in 0..num_levels {
            let level = val_min + (li as f64 + 0.5) * range / num_levels as f64;
            marching_squares(
                &grid,
                x_min,
                x_max,
                y_min,
                y_max,
                level,
                &mut content,
                colors[li % colors.len()],
                width,
                height,
            );
        }
    }

    Ok(Value::Str(svg_with_axes(
        &content, width, height, x_min, x_max, y_min, y_max, "", "", grid_lines,
    )))
}

// ── ListDensityPlot ────────────────────────────────────────────────────────────

pub fn builtin_list_density_plot(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "ListDensityPlot requires at least 1 argument".to_string(),
        ));
    }
    let grid = parse_grid(&args[0])?;
    let options = super::graphics::merge_options(&args[1..]);
    let (width, height, grid_lines) = parse_chart_options(&options);
    let color_fn = option_color_function(&options);

    let ny = grid.len();
    let nx = grid[0].len();
    let x_min = 1.0_f64;
    let x_max = nx as f64;
    let y_min = 1.0_f64;
    let y_max = ny as f64;

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

    let mut content = String::new();
    for (j, row) in grid.iter().enumerate() {
        for (i, &v) in row.iter().enumerate() {
            let t = if range > 1e-15 {
                (v - val_min) / range
            } else {
                0.5
            };
            let color = ramp_color(&color_fn, t);
            let (sx, sy) = map_to_svg(
                i as f64 + 1.0,
                j as f64 + 1.0,
                x_min - 0.5,
                x_max + 0.5,
                y_min - 0.5,
                y_max + 0.5,
                width,
                height,
            );
            let (sx2, sy2) = map_to_svg(
                i as f64 + 2.0,
                j as f64 + 2.0,
                x_min - 0.5,
                x_max + 0.5,
                y_min - 0.5,
                y_max + 0.5,
                width,
                height,
            );
            let sw = (sx2 - sx).abs().max(1.0);
            let sh = (sy2 - sy).abs().max(1.0);
            let sy_top = sy.min(sy2);
            content.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\" stroke=\"none\"/>\n",
                sx, sy_top, sw, sh, color
            ));
        }
    }

    Ok(Value::Str(svg_with_axes(
        &content,
        width,
        height,
        x_min - 0.5,
        x_max + 0.5,
        y_min - 0.5,
        y_max + 0.5,
        "",
        "",
        grid_lines,
    )))
}

// ── BarLegend / SwatchLegend ───────────────────────────────────────────────────

/// Parse a `{min, max}` numeric range.
fn range_of(v: &Value) -> Option<(f64, f64)> {
    if let Value::List(l) = v
        && l.len() == 2
        && let (Some(a), Some(b)) = (super::to_f64(&l[0]), super::to_f64(&l[1]))
    {
        Some((a, b))
    } else {
        None
    }
}

/// Build normalized gradient stops for a legend color source.
fn legend_stops(source: &Value) -> Vec<(f64, String)> {
    match source {
        Value::List(items) if !items.is_empty() => {
            let n = items.len();
            items
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let pos = if n == 1 {
                        0.0
                    } else {
                        i as f64 / (n - 1) as f64
                    };
                    (pos, color_value_to_hex(c))
                })
                .collect()
        }
        Value::Call { head, .. } if head == "RGBColor" => {
            let hex = apply_color_function(source, 0.5);
            vec![(0.0, hex.clone()), (1.0, hex)]
        }
        _ => (0..=8)
            .map(|i| {
                let t = i as f64 / 8.0;
                (
                    t,
                    apply_color_function(&Value::Symbol("Gradient".to_string()), t),
                )
            })
            .collect(),
    }
}

pub fn builtin_bar_legend(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "BarLegend requires at least 1 argument".to_string(),
        ));
    }
    let options = super::graphics::merge_options(&args[1..]);
    let (ow, oh, _) = parse_chart_options(&options);
    let width = if (ow - DEFAULT_WIDTH).abs() < 1e-9 {
        140.0
    } else {
        ow
    };
    let height = if (oh - DEFAULT_HEIGHT).abs() < 1e-9 {
        380.0
    } else {
        oh
    };

    // Accept {colorSource, {min, max}} or a bare color source, with the range
    // optionally supplied as a separate argument.
    let parsed = match &args[0] {
        Value::List(items) if items.len() == 2 => {
            range_of(&items[1]).map(|r| (items[0].clone(), r))
        }
        _ => None,
    };
    let (source, (min, max)) = match parsed {
        Some((source, range)) => (source, range),
        None => {
            let range = args.get(1).and_then(range_of).unwrap_or((0.0, 1.0));
            (args[0].clone(), range)
        }
    };

    let bar_x = 40.0_f64;
    let bar_w = 30.0_f64;
    let top = 20.0_f64;
    let bar_h = (height - 60.0).max(20.0);

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" style=\"background:white\">\n",
        width as i32, height as i32, width as i32, height as i32
    ));
    svg.push_str("<defs><linearGradient id=\"legend_grad\" x1=\"0\" y1=\"1\" x2=\"0\" y2=\"0\">\n");
    for (pos, color) in legend_stops(&source) {
        svg.push_str(&format!(
            "<stop offset=\"{:.3}\" stop-color=\"{}\"/>\n",
            pos, color
        ));
    }
    svg.push_str("</linearGradient></defs>\n");
    svg.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"url(#legend_grad)\" stroke=\"{}\" stroke-width=\"1\"/>\n",
        bar_x, top, bar_w, bar_h, AXIS_COLOR
    ));
    // Range labels at top, middle and bottom.
    for (frac, value) in [(0.0, max), (0.5, (min + max) / 2.0), (1.0, min)] {
        let y = top + frac * bar_h;
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"{}\" dominant-baseline=\"middle\">{}</text>\n",
            bar_x + bar_w + 6.0,
            y,
            DARK_TEXT,
            format_tick(value)
        ));
    }
    svg.push_str("</svg>\n");
    Ok(Value::Str(svg))
}

pub fn builtin_swatch_legend(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "SwatchLegend requires at least 1 argument".to_string(),
        ));
    }
    let colors = get_list(&args[0])?;
    if colors.is_empty() {
        return Err(EvalError::Error(
            "SwatchLegend: colors must not be empty".to_string(),
        ));
    }
    let labels: Option<&Vec<Value>> = match args.get(1) {
        Some(Value::List(l)) => Some(l),
        _ => None,
    };

    let row_h = 26.0_f64;
    let count = colors.len();
    let width = 180.0_f64;
    let height = (count as f64 * row_h + 20.0).max(40.0);

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" style=\"background:white\">\n",
        width as i32, height as i32, width as i32, height as i32
    ));
    for (i, color) in colors.iter().enumerate() {
        let y = 10.0 + i as f64 * row_h;
        let hex = color_value_to_hex(color);
        svg.push_str(&format!(
            "<rect x=\"8\" y=\"{:.1}\" width=\"18\" height=\"18\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\"/>\n",
            y, hex, AXIS_COLOR
        ));
        let label = labels
            .and_then(|l| l.get(i))
            .map(|v| match v {
                Value::Str(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_else(|| format!("{}", i + 1));
        svg.push_str(&format!(
            "<text x=\"34\" y=\"{:.1}\" font-size=\"12\" fill=\"{}\" dominant-baseline=\"middle\">{}</text>\n",
            y + 14.0,
            DARK_TEXT,
            escape_xml(&label)
        ));
    }
    svg.push_str("</svg>\n");
    Ok(Value::Str(svg))
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
    register_builtin(env, "DateListPlot", builtin_date_list_plot);
    register_builtin(env, "BoxWhiskerChart", builtin_box_whisker_chart);
    register_builtin(env, "ComplexPlot", builtin_complex_plot);
    register_builtin(env, "ListLogPlot", builtin_list_log_plot);
    register_builtin(env, "ListLogLogPlot", builtin_list_log_log_plot);
    register_builtin(env, "ListPolarPlot", builtin_list_polar_plot);
    register_builtin(env, "ListStreamPlot", builtin_list_stream_plot);
    register_builtin(env, "ListContourPlot", builtin_list_contour_plot);
    register_builtin(env, "ListDensityPlot", builtin_list_density_plot);
    register_builtin(env, "BarLegend", builtin_bar_legend);
    register_builtin(env, "SwatchLegend", builtin_swatch_legend);
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
    "DateListPlot",
    "BoxWhiskerChart",
    "ComplexPlot",
    "ListLogPlot",
    "ListLogLogPlot",
    "ListPolarPlot",
    "ListStreamPlot",
    "ListContourPlot",
    "ListDensityPlot",
    "BarLegend",
    "SwatchLegend",
    "ChartLayout",
    "ChartElements",
    "BarOrigin",
    "ColorFunction",
];

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;
    use std::collections::HashMap;

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
        assert!((super::super::to_f64(&int_val(42)).unwrap() - 42.0).abs() < 1e-12);
        assert!(
            (super::super::to_f64(&real_val(std::f64::consts::PI)).unwrap() - std::f64::consts::PI)
                .abs()
                < 1e-12
        );
        assert!(super::super::to_f64(&Value::Str("abc".to_string())).is_none());
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
        let val = Value::Integer(Integer::from(1));
        let result = get_list(&val);
        assert!(result.is_err());
    }

    #[test]
    fn test_svg_with_axes() {
        let svg = svg_with_axes("", 400.0, 300.0, 0.0, 10.0, 0.0, 100.0, "", "", true);
        assert!(svg.contains("<svg"), "Expected SVG open tag");
        assert!(svg.contains("</svg>"), "Expected SVG close tag");
        assert!(svg.contains("<line"), "Expected axis lines");
        assert!(svg.contains("<text"), "Expected tick labels");
    }

    #[test]
    fn test_svg_with_axes_labels() {
        let svg = svg_with_axes(
            "", 400.0, 300.0, 0.0, 10.0, 0.0, 100.0, "X Label", "Y Label", false,
        );
        assert!(svg.contains("X Label"), "Expected X axis label");
        assert!(svg.contains("Y Label"), "Expected Y axis label");
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("a<b>&\"c'"), "a&lt;b&gt;&amp;&quot;c&apos;");
        assert_eq!(escape_xml("plain"), "plain");
    }

    #[test]
    fn test_guard_range() {
        let (lo, hi) = guard_range(5.0, 5.0);
        assert!(hi > lo);
        let (lo, hi) = guard_range(9.0, 2.0);
        assert!(lo < hi);
        let (lo, hi) = guard_range(f64::NEG_INFINITY, f64::NAN);
        assert!((lo + 1.0).abs() < 1e-12 && (hi - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_svg_with_axes_grid_lines_toggle() {
        let with_grid = svg_with_axes("", 400.0, 300.0, 0.0, 10.0, 0.0, 100.0, "", "", true);
        assert!(with_grid.contains(EEE), "grid lines expected when enabled");
        let without_grid = svg_with_axes("", 400.0, 300.0, 0.0, 10.0, 0.0, 100.0, "", "", false);
        assert!(
            !without_grid.contains(EEE),
            "no grid lines expected when disabled"
        );
    }

    #[test]
    fn test_svg_with_axes_escapes_labels() {
        let svg = svg_with_axes("", 400.0, 300.0, 0.0, 10.0, 0.0, 100.0, "x<y", "a&b", false);
        assert!(svg.contains("x&lt;y"));
        assert!(svg.contains("a&amp;b"));
    }

    #[test]
    fn test_svg_with_axes_degenerate_range_finite() {
        let svg = svg_with_axes("", 400.0, 300.0, 3.0, 3.0, 7.0, 7.0, "", "", true);
        assert!(
            !svg.contains("NaN"),
            "degenerate range produced NaN: {}",
            svg
        );
    }

    #[test]
    fn test_bar_chart_grid_lines_option() {
        let data = Value::List(vec![int_val(10), int_val(20), int_val(30)]);
        let mut map = HashMap::new();
        map.insert("GridLines".to_string(), Value::Bool(true));
        let result = builtin_bar_chart(&[data, Value::Assoc(map)]).unwrap();
        match result {
            Value::Str(svg) => assert!(svg.contains(EEE), "grid lines expected"),
            _ => panic!("Expected SVG string"),
        }
    }

    #[test]
    fn test_histogram_degenerate_data_finite() {
        // All-identical data must not divide by zero.
        let data = Value::List(vec![int_val(5), int_val(5), int_val(5)]);
        let result = builtin_histogram(&[data]).unwrap();
        match result {
            Value::Str(svg) => assert!(!svg.contains("NaN")),
            _ => panic!("Expected SVG string"),
        }
    }

    fn option_rule(key: &str, rhs: crate::ast::Expr) -> Value {
        Value::Rule {
            lhs: Box::new(Value::Pattern(crate::ast::Expr::Symbol(key.to_string()))),
            rhs: Box::new(Value::Pattern(rhs)),
            delayed: false,
        }
    }

    #[test]
    fn test_bar_chart_grid_lines_rule_option() {
        let data = Value::List(vec![int_val(10), int_val(20), int_val(30)]);
        let rule = option_rule("GridLines", crate::ast::Expr::Bool(true));
        let result = builtin_bar_chart(&[data, rule]).unwrap();
        match result {
            Value::Str(svg) => assert!(svg.contains(EEE), "grid lines expected"),
            _ => panic!("Expected SVG string"),
        }
    }

    #[test]
    fn test_histogram_grid_lines_rule_option() {
        let data = Value::List(vec![int_val(1), int_val(2), int_val(2), int_val(3)]);
        let rule = option_rule("GridLines", crate::ast::Expr::Bool(true));
        let result = builtin_histogram(&[data, rule]).unwrap();
        match result {
            Value::Str(svg) => assert!(svg.contains(EEE), "grid lines expected"),
            _ => panic!("Expected SVG string"),
        }
    }

    // ── Extended charts ─────────────────────────────────────────────────────

    fn complex_val(re: f64, im: f64) -> Value {
        Value::Complex { re, im }
    }

    #[test]
    fn test_date_list_plot_basic() {
        let data = Value::List(vec![
            Value::List(vec![
                Value::List(vec![int_val(2020), int_val(1), int_val(1)]),
                real_val(10.0),
            ]),
            Value::List(vec![
                Value::List(vec![int_val(2020), int_val(6), int_val(1)]),
                real_val(20.0),
            ]),
        ]);
        let result = builtin_date_list_plot(&[data]).unwrap();
        match result {
            Value::Str(svg) => {
                assert!(svg.contains("<polyline"), "expected connecting line");
                assert!(svg.contains("Date"), "expected Date axis label");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_date_list_plot_string_dates() {
        let data = Value::List(vec![
            Value::List(vec![Value::Str("2020-01-01".to_string()), real_val(1.0)]),
            Value::List(vec![Value::Str("2020-02-01".to_string()), real_val(2.0)]),
        ]);
        let result = builtin_date_list_plot(&[data]).unwrap();
        assert!(matches!(result, Value::Str(svg) if svg.contains("<svg")));
    }

    #[test]
    fn test_date_list_plot_empty() {
        assert!(builtin_date_list_plot(&[Value::List(vec![])]).is_err());
    }

    #[test]
    fn test_days_from_civil_epoch() {
        assert!((days_from_civil(1970, 1, 1) - 0.0).abs() < 1e-9);
        assert!(days_from_civil(1970, 1, 2) > days_from_civil(1970, 1, 1));
    }

    #[test]
    fn test_box_whisker_chart_single() {
        let data = Value::List(vec![
            real_val(1.0),
            real_val(2.0),
            real_val(3.0),
            real_val(4.0),
            real_val(100.0),
        ]);
        let result = builtin_box_whisker_chart(&[data]).unwrap();
        match result {
            Value::Str(svg) => {
                assert!(svg.contains("<rect"), "expected box");
                assert!(svg.contains("<line"), "expected whiskers/median");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_box_whisker_chart_multiple_datasets() {
        let data = Value::List(vec![
            Value::List(vec![real_val(1.0), real_val(2.0), real_val(3.0)]),
            Value::List(vec![real_val(5.0), real_val(6.0), real_val(7.0)]),
        ]);
        let result = builtin_box_whisker_chart(&[data]).unwrap();
        assert!(matches!(result, Value::Str(svg) if svg.contains("<rect")));
    }

    #[test]
    fn test_box_whisker_chart_empty() {
        assert!(builtin_box_whisker_chart(&[Value::List(vec![])]).is_err());
    }

    #[test]
    fn test_quantile() {
        let sorted = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((quantile(&sorted, 0.5) - 3.0).abs() < 1e-12);
        assert!((quantile(&sorted, 0.0) - 1.0).abs() < 1e-12);
        assert!((quantile(&sorted, 1.0) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn test_complex_plot_basic() {
        let data = Value::List(vec![
            complex_val(1.0, 1.0),
            complex_val(-1.0, 0.5),
            complex_val(0.0, -2.0),
        ]);
        let result = builtin_complex_plot(&[data]).unwrap();
        match result {
            Value::Str(svg) => {
                assert!(svg.contains("<circle"), "expected complex plane points");
                assert!(svg.contains(">Re<"), "expected Re axis label");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_complex_plot_symbolic_fallback() {
        let result = builtin_complex_plot(&[Value::Symbol("f".to_string())]).unwrap();
        assert!(matches!(result, Value::Call { head, .. } if head == "ComplexPlot"));
    }

    #[test]
    fn test_list_log_plot_basic() {
        let data = Value::List(vec![int_val(1), int_val(10), int_val(100), int_val(1000)]);
        let result = builtin_list_log_plot(&[data]).unwrap();
        match result {
            Value::Str(svg) => {
                assert!(svg.contains("<polyline"), "expected log line plot");
                assert!(svg.contains("Log y"), "expected log axis label");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_list_log_plot_drops_nonpositive() {
        let data = Value::List(vec![int_val(0), int_val(10), int_val(100)]);
        let result = builtin_list_log_plot(&[data]).unwrap();
        assert!(matches!(result, Value::Str(svg) if svg.contains("<polyline")));
    }

    #[test]
    fn test_list_log_log_plot_basic() {
        let data = Value::List(vec![
            Value::List(vec![real_val(1.0), real_val(1.0)]),
            Value::List(vec![real_val(10.0), real_val(100.0)]),
            Value::List(vec![real_val(100.0), real_val(10000.0)]),
        ]);
        let result = builtin_list_log_log_plot(&[data]).unwrap();
        assert!(matches!(result, Value::Str(svg) if svg.contains("<polyline")));
    }

    #[test]
    fn test_list_polar_plot_basic() {
        let data = Value::List(vec![
            Value::List(vec![real_val(1.0), real_val(0.0)]),
            Value::List(vec![real_val(1.0), real_val(std::f64::consts::FRAC_PI_2)]),
        ]);
        let result = builtin_list_polar_plot(&[data]).unwrap();
        assert!(matches!(result, Value::Str(svg) if svg.contains("<circle")));
    }

    #[test]
    fn test_list_polar_plot_empty() {
        assert!(builtin_list_polar_plot(&[Value::List(vec![])]).is_err());
    }

    #[test]
    fn test_list_stream_plot_basic() {
        let data = Value::List(vec![
            Value::List(vec![
                Value::List(vec![real_val(0.0), real_val(0.0)]),
                Value::List(vec![real_val(1.0), real_val(0.0)]),
            ]),
            Value::List(vec![
                Value::List(vec![real_val(1.0), real_val(1.0)]),
                Value::List(vec![real_val(0.0), real_val(1.0)]),
            ]),
        ]);
        let result = builtin_list_stream_plot(&[data]).unwrap();
        match result {
            Value::Str(svg) => assert!(svg.contains("<line"), "expected arrow segments"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_list_stream_plot_bad_data() {
        let data = Value::List(vec![int_val(1), int_val(2)]);
        assert!(builtin_list_stream_plot(&[data]).is_err());
    }

    #[test]
    fn test_list_contour_plot_basic() {
        let grid = Value::List(vec![
            Value::List(vec![real_val(0.0), real_val(1.0), real_val(2.0)]),
            Value::List(vec![real_val(1.0), real_val(2.0), real_val(3.0)]),
            Value::List(vec![real_val(2.0), real_val(3.0), real_val(4.0)]),
        ]);
        let result = builtin_list_contour_plot(&[grid]).unwrap();
        assert!(matches!(result, Value::Str(svg) if svg.contains("<line")));
    }

    #[test]
    fn test_list_contour_plot_ragged_error() {
        let grid = Value::List(vec![
            Value::List(vec![real_val(0.0), real_val(1.0)]),
            Value::List(vec![real_val(0.0)]),
        ]);
        assert!(builtin_list_contour_plot(&[grid]).is_err());
    }

    #[test]
    fn test_list_density_plot_basic() {
        let grid = Value::List(vec![
            Value::List(vec![real_val(0.0), real_val(0.5)]),
            Value::List(vec![real_val(0.5), real_val(1.0)]),
        ]);
        let result = builtin_list_density_plot(&[grid]).unwrap();
        assert!(matches!(result, Value::Str(svg) if svg.contains("<rect")));
    }

    #[test]
    fn test_list_density_plot_empty_error() {
        assert!(builtin_list_density_plot(&[Value::List(vec![])]).is_err());
    }

    #[test]
    fn test_bar_legend_gradient() {
        let spec = Value::List(vec![
            Value::Symbol("ColorFunction".to_string()),
            Value::List(vec![real_val(0.0), real_val(100.0)]),
        ]);
        let result = builtin_bar_legend(&[spec]).unwrap();
        match result {
            Value::Str(svg) => {
                assert!(svg.contains("linearGradient"), "expected gradient");
                assert!(svg.contains("100"), "expected range label");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_bar_legend_color_list() {
        let colors = Value::List(vec![
            Value::Str("#0000ff".to_string()),
            Value::Str("#ff0000".to_string()),
        ]);
        let result = builtin_bar_legend(&[colors]).unwrap();
        assert!(matches!(result, Value::Str(svg) if svg.contains("stop-color")));
    }

    #[test]
    fn test_bar_legend_empty() {
        assert!(builtin_bar_legend(&[]).is_err());
    }

    #[test]
    fn test_swatch_legend_basic() {
        let colors = Value::List(vec![
            Value::Str("#ff0000".to_string()),
            Value::Str("#00ff00".to_string()),
        ]);
        let labels = Value::List(vec![
            Value::Str("A".to_string()),
            Value::Str("B".to_string()),
        ]);
        let result = builtin_swatch_legend(&[colors, labels]).unwrap();
        match result {
            Value::Str(svg) => {
                assert!(svg.contains("<rect"), "expected swatches");
                assert!(
                    svg.contains(">A<") && svg.contains(">B<"),
                    "expected labels"
                );
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_swatch_legend_empty() {
        assert!(builtin_swatch_legend(&[Value::List(vec![])]).is_err());
    }

    #[test]
    fn test_color_value_to_hex_variants() {
        assert_eq!(
            color_value_to_hex(&Value::Str("#abcdef".to_string())),
            "#abcdef"
        );
        assert_eq!(
            color_value_to_hex(&Value::Symbol("Red".to_string())),
            "#ff0000"
        );
        let rgb = Value::Call {
            head: "RGBColor".to_string(),
            args: vec![real_val(1.0), real_val(0.0), real_val(0.0)],
        };
        assert_eq!(color_value_to_hex(&rgb), "#ff0000");
    }

    #[test]
    fn test_named_color_unknown() {
        assert_eq!(named_color("Chartreuse"), None);
    }

    #[test]
    fn test_to_complex_forms() {
        assert_eq!(to_complex(&complex_val(2.0, -3.0)), Some((2.0, -3.0)));
        assert_eq!(
            to_complex(&Value::List(vec![real_val(1.0), real_val(2.0)])),
            Some((1.0, 2.0))
        );
        assert_eq!(to_complex(&int_val(5)), Some((5.0, 0.0)));
    }

    #[test]
    fn test_date_to_serial_numeric_and_list() {
        assert_eq!(date_to_serial(&real_val(3.5)), Some(3.5));
        let d = Value::List(vec![int_val(1970), int_val(1), int_val(1)]);
        assert!(date_to_serial(&d).unwrap().abs() < 1e-9);
    }
}
