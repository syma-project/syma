//! 3D, field and list plots, plus the `Graphics3D` primitives they build on.
//!
//! Surface/field plots: `Plot3D`, `ParametricPlot3D`, `ContourPlot3D`,
//! `SliceContourPlot3D`, `DensityPlot3D`, `VectorPlot`, `VectorPlot3D`,
//! `StreamPlot`, `StreamDensityPlot`, `VectorDensityPlot`. 2D list plots:
//! `ListContourPlot`, `ListDensityPlot`, `ListStreamPlot`, `ListPolarPlot`.
//! `GraphicsComplex` resolves shared point indices; `Graphics3D` is the
//! Wolfram-compatible container head and is stored symbolically.
//!
//! These builtins produce symbolic `Graphics` / `Graphics3D` values. The
//! plot-like functions need evaluator access to sample their integrand at grid
//! points (hence the `HoldAll` attribute), and then emit 2D-projected
//! primitives so the resulting `Graphics` renders through the SVG pipeline in
//! [`super::graphics`].
//!
//! `DensityPlot` is dispatched by the evaluator (see `eval::plot`); it is
//! documented here for discoverability but not re-implemented.

use std::collections::HashMap;

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, Value};

// ── Value helpers ───────────────────────────────────────────────────────────

/// Sample-grid resolution for surface plots.
const SURFACE_GRID: usize = 25;
/// Sample-grid resolution for vector fields.
const FIELD_GRID: usize = 12;
/// Seed-grid resolution for streamlines.
const STREAM_SEEDS: usize = 9;
/// Sample-grid resolution for parametric surfaces.
const PARAM3D_GRID: usize = 20;
/// Sample count for parametric space curves.
const PARAM3D_CURVE: usize = 160;
/// Sample-grid resolution for 3D iso-surfaces.
const CONTOUR3D_GRID: usize = 12;
/// Sample-grid resolution for slice contours.
const SLICE3D_GRID: usize = 14;
/// Sample-grid resolution for 3D density clouds.
const DENSITY3D_GRID: usize = 10;
/// Sample-grid resolution for 3D predicate regions.
const REGION3D_GRID: usize = 14;
/// Sample-grid resolution for 3D vector fields.
const VECTOR3D_GRID: usize = 6;
/// Background resolution for density overlays.
const DENSITY_BG: usize = 36;

/// Convert a possibly-held argument to an `Expr` for sampling.
fn to_expr(v: &Value) -> Expr {
    match v {
        Value::Pattern(e) => e.clone(),
        other => crate::eval::table::value_to_expr(other),
    }
}

/// Unwrap a possibly-held argument to its unevaluated symbolic `Value`.
fn to_value(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => crate::pattern::unwrap_expr_to_value(e),
        other => other.clone(),
    }
}

/// Evaluate a possibly-held argument (used for iterator specs and options).
fn eval_arg(v: &Value, env: &Env) -> Value {
    match v {
        Value::Pattern(e) => {
            crate::eval::eval(e, env).unwrap_or_else(|_| crate::pattern::unwrap_expr_to_value(e))
        }
        other => other.clone(),
    }
}

fn point2(x: f64, y: f64) -> Value {
    Value::List(vec![super::real(x), super::real(y)])
}

fn line2(a: (f64, f64), b: (f64, f64)) -> Value {
    Value::Call {
        head: "Line".to_string(),
        args: vec![Value::List(vec![point2(a.0, a.1), point2(b.0, b.1)])],
    }
}

fn rgb(r: f64, g: f64, b: f64) -> Value {
    Value::Call {
        head: "RGBColor".to_string(),
        args: vec![super::real(r), super::real(g), super::real(b)],
    }
}

/// Return the call unchanged (with held wrappers stripped) for inapplicable input.
fn unevaluated(name: &str, args: &[Value]) -> Value {
    Value::Call {
        head: name.to_string(),
        args: args.iter().map(to_value).collect(),
    }
}

fn is_function_value(v: &Value) -> bool {
    matches!(
        v,
        Value::PureFunction { .. }
            | Value::Function(_)
            | Value::BytecodeFunction(_)
            | Value::Builtin(..)
    )
}

/// Sample `expr` at the given variable bindings.
///
/// Supports both symbolic expressions in the iterator variables and pure
/// functions (`expr &`, `Function[...]`), which are applied to the sample point.
fn sample(expr: &Expr, vars: &[(&str, f64)], env: &Env) -> Option<f64> {
    let child = env.child();
    for (name, value) in vars {
        child.set((*name).to_string(), super::real(*value));
    }
    let val = crate::eval::eval(expr, &child).ok()?;
    if let Some(f) = super::to_f64(&val) {
        return Some(f);
    }
    if is_function_value(&val) {
        let call_args: Vec<Value> = vars.iter().map(|(_, v)| super::real(*v)).collect();
        let res = crate::eval::apply_function(&val, &call_args, env).ok()?;
        return super::to_f64(&res);
    }
    None
}

/// Sample a boolean predicate at the given variable bindings.
///
/// Mirrors [`sample`], but coerces the evaluated value through
/// [`Value::to_bool`] instead of requiring a number, and applies pure-function
/// predicates to the sample point.
fn sample_pred(expr: &Expr, vars: &[(&str, f64)], env: &Env) -> bool {
    let child = env.child();
    for (name, value) in vars {
        child.set((*name).to_string(), super::real(*value));
    }
    let Ok(val) = crate::eval::eval(expr, &child) else {
        return false;
    };
    if is_function_value(&val) {
        let call_args: Vec<Value> = vars.iter().map(|(_, v)| super::real(*v)).collect();
        return match crate::eval::apply_function(&val, &call_args, env) {
            Ok(res) => res.to_bool(),
            Err(_) => false,
        };
    }
    val.to_bool()
}

// ── Iterator specs ──────────────────────────────────────────────────────────

struct IterSpec {
    var: String,
    min: f64,
    max: f64,
}

impl IterSpec {
    /// Parse `{var, min, max}` from a *held* spec so a globally-bound iterator
    /// variable does not get substituted. Only the bounds are evaluated.
    fn parse(v: &Value, env: &Env) -> Option<Self> {
        match v {
            Value::List(items) if items.len() >= 3 => {
                let var = match &items[0] {
                    Value::Symbol(s) => s.clone(),
                    other => other.to_string(),
                };
                let min = eval_num(&items[1], env)?;
                let max = eval_num(&items[2], env)?;
                if min >= max {
                    return None;
                }
                Some(Self { var, min, max })
            }
            _ => None,
        }
    }

    fn lerp(&self, t: f64) -> f64 {
        self.min + (self.max - self.min) * t
    }
}

/// Evaluate a numeric bound, which may be a literal or a symbolic expression.
fn eval_num(v: &Value, env: &Env) -> Option<f64> {
    if let Some(f) = super::to_f64(v) {
        return Some(f);
    }
    let expr = crate::eval::table::value_to_expr(v);
    super::to_f64(&crate::eval::eval(&expr, env).ok()?)
}

// ── Options ─────────────────────────────────────────────────────────────────

/// Build a `Graphics` options association, merging user rules over defaults.
fn graphics_options(extra: &[Value], env: &Env) -> Value {
    let mut map: HashMap<String, Value> = HashMap::new();
    map.insert(
        "ImageSize".to_string(),
        Value::List(vec![super::real(400.0), super::real(400.0)]),
    );
    map.insert("Axes".to_string(), Value::Bool(true));
    for arg in extra {
        if let Value::Rule { lhs, rhs, .. } = eval_arg(arg, env)
            && let Value::Symbol(k) | Value::Str(k) = lhs.as_ref()
        {
            map.insert(k.clone(), rhs.as_ref().clone());
        }
    }
    Value::Assoc(map)
}

// ── Projection ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct V3 {
    x: f64,
    y: f64,
    z: f64,
}

/// Orthographic projection from world space to (screen_x, screen_y, depth).
///
/// The camera looks from a fixed azimuth/elevation; `depth` is the distance
/// toward the viewer (used to order segments back-to-front).
fn project(p: V3, center: V3, scale: f64) -> (f64, f64, f64) {
    let az = std::f64::consts::FRAC_PI_4;
    let el = 0.30 * std::f64::consts::PI;
    let (sa, ca) = az.sin_cos();
    let (se, ce) = el.sin_cos();
    let px = (p.x - center.x) * scale;
    let py = (p.y - center.y) * scale;
    let pz = (p.z - center.z) * scale;
    let sx = -sa * px + ca * py;
    let sy = -se * ca * px - se * sa * py + ce * pz;
    let depth = ce * ca * px + ce * sa * py + se * pz;
    (sx, sy, depth)
}

// ── Plot3D ──────────────────────────────────────────────────────────────────

/// `Plot3D[f, {x, xmin, xmax}, {y, ymin, ymax}]` — surface plot.
///
/// Samples `f` on a grid and projects the resulting mesh to 2D. Segments are
/// drawn back-to-front and shaded by depth, giving a hidden-line-like look
/// that the existing SVG renderer can consume.
pub fn builtin_plot3d(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = "Plot3D";
    if args.len() < 3 {
        return Ok(unevaluated(name, args));
    }
    let f_expr = to_expr(&args[0]);
    let Some(x_spec) = IterSpec::parse(&to_value(&args[1]), env) else {
        return Ok(unevaluated(name, args));
    };
    let Some(y_spec) = IterSpec::parse(&to_value(&args[2]), env) else {
        return Ok(unevaluated(name, args));
    };

    let n = SURFACE_GRID;
    let mut grid = vec![vec![f64::NAN; n]; n];
    let (mut zmin, mut zmax) = (f64::INFINITY, f64::NEG_INFINITY);
    for (j, row) in grid.iter_mut().enumerate() {
        let y = y_spec.lerp(j as f64 / (n - 1) as f64);
        for (i, cell) in row.iter_mut().enumerate() {
            let x = x_spec.lerp(i as f64 / (n - 1) as f64);
            if let Some(z) = sample(&f_expr, &[(&x_spec.var, x), (&y_spec.var, y)], env)
                .filter(|z| z.is_finite())
            {
                *cell = z;
                zmin = zmin.min(z);
                zmax = zmax.max(z);
            }
        }
    }
    if !zmin.is_finite() {
        return Ok(unevaluated(name, args));
    }

    let center = V3 {
        x: (x_spec.min + x_spec.max) / 2.0,
        y: (y_spec.min + y_spec.max) / 2.0,
        z: (zmin + zmax) / 2.0,
    };
    let extent = (x_spec.max - x_spec.min)
        .max(y_spec.max - y_spec.min)
        .max((zmax - zmin).abs())
        .max(1e-12);
    let scale = 1.0 / extent;

    let mut segments: Vec<(f64, Value)> = Vec::new();
    {
        let mut push = |a: V3, b: V3| {
            let (ax, ay, ad) = project(a, center, scale);
            let (bx, by, bd) = project(b, center, scale);
            segments.push(((ad + bd) / 2.0, line2((ax, ay), (bx, by))));
        };

        #[allow(clippy::needless_range_loop)]
        for j in 0..n {
            let y = y_spec.lerp(j as f64 / (n - 1) as f64);
            for i in 1..n {
                let (z0, z1) = (grid[j][i - 1], grid[j][i]);
                if z0.is_finite() && z1.is_finite() {
                    let x0 = x_spec.lerp((i - 1) as f64 / (n - 1) as f64);
                    let x1 = x_spec.lerp(i as f64 / (n - 1) as f64);
                    push(V3 { x: x0, y, z: z0 }, V3 { x: x1, y, z: z1 });
                }
            }
        }
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let x = x_spec.lerp(i as f64 / (n - 1) as f64);
            for j in 1..n {
                let (z0, z1) = (grid[j - 1][i], grid[j][i]);
                if z0.is_finite() && z1.is_finite() {
                    let y0 = y_spec.lerp((j - 1) as f64 / (n - 1) as f64);
                    let y1 = y_spec.lerp(j as f64 / (n - 1) as f64);
                    push(V3 { x, y: y0, z: z0 }, V3 { x, y: y1, z: z1 });
                }
            }
        }
    }

    if segments.is_empty() {
        return Ok(unevaluated(name, args));
    }

    // Painter's algorithm: far segments first so near ones draw on top.
    segments.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let dmin = segments.last().map(|s| s.0).unwrap_or(0.0);
    let dmax = segments.first().map(|s| s.0).unwrap_or(1.0);
    let drange = (dmax - dmin).max(1e-12);

    let mut prims: Vec<Value> = Vec::with_capacity(segments.len() * 2);
    for (depth, line) in segments {
        let t = ((depth - dmin) / drange).clamp(0.0, 1.0);
        prims.push(rgb(0.85 - 0.75 * t, 0.88 - 0.70 * t, 0.95 - 0.45 * t));
        prims.push(line);
    }

    Ok(Value::Call {
        head: "Graphics".to_string(),
        args: vec![Value::List(prims), graphics_options(&args[3..], env)],
    })
}

// ── VectorPlot ──────────────────────────────────────────────────────────────

/// A sampled arrow: `(x, y, vx, vy, magnitude)`.
type Arrow = (f64, f64, f64, f64, f64);

/// Sample a 2D vector field on the standard grid.
fn collect_arrows(
    vx_expr: &Expr,
    vy_expr: &Expr,
    x_spec: &IterSpec,
    y_spec: &IterSpec,
    env: &Env,
) -> (Vec<Arrow>, f64) {
    let n = FIELD_GRID;
    let mut arrows: Vec<Arrow> = Vec::new();
    let mut max_mag = 0.0_f64;
    for j in 0..n {
        let y = y_spec.lerp(j as f64 / (n - 1) as f64);
        for i in 0..n {
            let x = x_spec.lerp(i as f64 / (n - 1) as f64);
            let vx = sample(vx_expr, &[(&x_spec.var, x), (&y_spec.var, y)], env)
                .filter(|f| f.is_finite());
            let vy = sample(vy_expr, &[(&x_spec.var, x), (&y_spec.var, y)], env)
                .filter(|f| f.is_finite());
            if let (Some(vx), Some(vy)) = (vx, vy) {
                let mag = (vx * vx + vy * vy).sqrt();
                if mag > 1e-12 {
                    max_mag = max_mag.max(mag);
                    arrows.push((x, y, vx, vy, mag));
                }
            }
        }
    }
    (arrows, max_mag)
}

/// Append shaft-and-head `Line` primitives for a set of sampled arrows.
fn push_arrows(
    prims: &mut Vec<Value>,
    arrows: &[Arrow],
    max_mag: f64,
    x_spec: &IterSpec,
    y_spec: &IterSpec,
) {
    let cell = ((x_spec.max - x_spec.min) / (FIELD_GRID - 1) as f64)
        .min((y_spec.max - y_spec.min) / (FIELD_GRID - 1) as f64);
    let max_len = 0.9 * cell;
    let head_angle = 2.6_f64; // ≈ 150°

    for &(x, y, vx, vy, mag) in arrows {
        let len = max_len * mag / max_mag;
        let (ux, uy) = (vx / mag, vy / mag);
        let tip = (x + ux * len, y + uy * len);
        prims.push(line2((x, y), tip));

        let hl = (len * 0.35).max(1e-9);
        for sign in [head_angle, -head_angle] {
            let (sn, cs) = sign.sin_cos();
            let hx = ux * cs - uy * sn;
            let hy = ux * sn + uy * cs;
            prims.push(line2(tip, (tip.0 + hx * hl, tip.1 + hy * hl)));
        }
    }
}

/// `VectorPlot[{vx, vy}, {x, xmin, xmax}, {y, ymin, ymax}]` — arrow field.
pub fn builtin_vector_plot(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = "VectorPlot";
    if args.len() < 3 {
        return Ok(unevaluated(name, args));
    }
    let (vx_expr, vy_expr) = match to_expr(&args[0]) {
        Expr::List(items) if items.len() == 2 => (items[0].clone(), items[1].clone()),
        _ => return Ok(unevaluated(name, args)),
    };
    let Some(x_spec) = IterSpec::parse(&to_value(&args[1]), env) else {
        return Ok(unevaluated(name, args));
    };
    let Some(y_spec) = IterSpec::parse(&to_value(&args[2]), env) else {
        return Ok(unevaluated(name, args));
    };

    let (arrows, max_mag) = collect_arrows(&vx_expr, &vy_expr, &x_spec, &y_spec, env);
    if arrows.is_empty() {
        return Ok(unevaluated(name, args));
    }

    let mut prims = vec![rgb(0.10, 0.45, 0.91)];
    push_arrows(&mut prims, &arrows, max_mag, &x_spec, &y_spec);
    Ok(finish_prims(prims, &args[3..], env))
}

// ── StreamPlot ──────────────────────────────────────────────────────────────

/// `StreamPlot[{vx, vy}, {x, xmin, xmax}, {y, ymin, ymax}]` — streamlines.
pub fn builtin_stream_plot(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = "StreamPlot";
    if args.len() < 3 {
        return Ok(unevaluated(name, args));
    }
    let (vx_expr, vy_expr) = match to_expr(&args[0]) {
        Expr::List(items) if items.len() == 2 => (items[0].clone(), items[1].clone()),
        _ => return Ok(unevaluated(name, args)),
    };
    let Some(x_spec) = IterSpec::parse(&to_value(&args[1]), env) else {
        return Ok(unevaluated(name, args));
    };
    let Some(y_spec) = IterSpec::parse(&to_value(&args[2]), env) else {
        return Ok(unevaluated(name, args));
    };

    let field = |x: f64, y: f64| -> Option<(f64, f64)> {
        let vx = sample(&vx_expr, &[(&x_spec.var, x), (&y_spec.var, y)], env)?;
        let vy = sample(&vy_expr, &[(&x_spec.var, x), (&y_spec.var, y)], env)?;
        if vx.is_finite() && vy.is_finite() {
            Some((vx, vy))
        } else {
            None
        }
    };

    let h = ((x_spec.max - x_spec.min).min(y_spec.max - y_spec.min)) / 60.0;
    let bounds = (x_spec.min, x_spec.max, y_spec.min, y_spec.max);

    let mut prims = vec![rgb(0.10, 0.45, 0.91)];
    for sj in 0..STREAM_SEEDS {
        let y0 = y_spec.lerp((sj as f64 + 0.5) / STREAM_SEEDS as f64);
        for si in 0..STREAM_SEEDS {
            let x0 = x_spec.lerp((si as f64 + 0.5) / STREAM_SEEDS as f64);
            for dir in [1.0_f64, -1.0] {
                let pts = streamline(&field, x0, y0, h, dir, bounds);
                if pts.len() >= 3 {
                    prims.push(Value::Call {
                        head: "Line".to_string(),
                        args: vec![Value::List(
                            pts.into_iter().map(|(x, y)| point2(x, y)).collect(),
                        )],
                    });
                }
            }
        }
    }

    if prims.len() <= 1 {
        return Ok(unevaluated(name, args));
    }

    Ok(Value::Call {
        head: "Graphics".to_string(),
        args: vec![Value::List(prims), graphics_options(&args[3..], env)],
    })
}

/// Integrate one streamline with RK4 using the normalised field direction.
fn streamline(
    field: &impl Fn(f64, f64) -> Option<(f64, f64)>,
    x0: f64,
    y0: f64,
    h: f64,
    dir: f64,
    bounds: (f64, f64, f64, f64),
) -> Vec<(f64, f64)> {
    let (xmin, xmax, ymin, ymax) = bounds;
    let eps = 1e-9;
    let unit = |x: f64, y: f64| -> Option<(f64, f64)> {
        let (vx, vy) = field(x, y)?;
        let mag = (vx * vx + vy * vy).sqrt();
        if mag > 1e-12 {
            Some((vx / mag, vy / mag))
        } else {
            None
        }
    };

    let mut pts = vec![(x0, y0)];
    let (mut x, mut y) = (x0, y0);
    for _ in 0..800 {
        let Some((k1x, k1y)) = unit(x, y) else { break };
        let Some((k2x, k2y)) = unit(x + dir * h * 0.5 * k1x, y + dir * h * 0.5 * k1y) else {
            break;
        };
        let Some((k3x, k3y)) = unit(x + dir * h * 0.5 * k2x, y + dir * h * 0.5 * k2y) else {
            break;
        };
        let Some((k4x, k4y)) = unit(x + dir * h * k3x, y + dir * h * k3y) else {
            break;
        };
        let nx = x + dir * h * (k1x + 2.0 * k2x + 2.0 * k3x + k4x) / 6.0;
        let ny = y + dir * h * (k1y + 2.0 * k2y + 2.0 * k3y + k4y) / 6.0;
        if !(xmin - eps..=xmax + eps).contains(&nx) || !(ymin - eps..=ymax + eps).contains(&ny) {
            break;
        }
        if (nx - x).abs() < 1e-12 && (ny - y).abs() < 1e-12 {
            break;
        }
        x = nx;
        y = ny;
        pts.push((x, y));
    }
    pts
}

// ── Graphics3D ──────────────────────────────────────────────────────────────

/// `Graphics3D[primitives, options]` — symbolic 3D graphics container.
pub fn builtin_graphics3d(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 || !matches!(&args[0], Value::List(_)) {
        return Ok(unevaluated("Graphics3D", args));
    }
    let opts = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| Value::Assoc(HashMap::new()));
    Ok(Value::Call {
        head: "Graphics3D".to_string(),
        args: vec![args[0].clone(), opts],
    })
}

/// `Cuboid[{x1, y1, z1}]` or `Cuboid[{x1, y1, z1}, {x2, y2, z2}]` — axis-aligned box.
pub fn builtin_cuboid(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("Cuboid", args));
    }
    for (pos, arg) in args.iter().enumerate() {
        match arg {
            Value::List(coords) if coords.len() >= 3 => {
                for c in coords {
                    super::require_f64(c, "Cuboid", pos + 1)?;
                }
            }
            _ => return Ok(unevaluated("Cuboid", args)),
        }
    }
    Ok(Value::Call {
        head: "Cuboid".to_string(),
        args: args.to_vec(),
    })
}

// ── Projection helpers ──────────────────────────────────────────────────────

/// Fixed screen-space projector over an axis-aligned world bounding box.
#[derive(Clone, Copy)]
struct Projector {
    center: V3,
    scale: f64,
}

impl Projector {
    /// Build a projector from a world bounding box, normalising the largest
    /// axis extent to unit screen size.
    fn from_bounds(min: V3, max: V3) -> Self {
        let center = V3 {
            x: (min.x + max.x) / 2.0,
            y: (min.y + max.y) / 2.0,
            z: (min.z + max.z) / 2.0,
        };
        let extent = (max.x - min.x)
            .abs()
            .max((max.y - min.y).abs())
            .max((max.z - min.z).abs())
            .max(1e-12);
        Self {
            center,
            scale: 1.0 / extent,
        }
    }

    fn screen(&self, p: V3) -> (f64, f64, f64) {
        project(p, self.center, self.scale)
    }

    /// Project a world segment into a depth-tagged 2D `Line`.
    fn segment(&self, a: V3, b: V3) -> (f64, Value) {
        let (ax, ay, ad) = self.screen(a);
        let (bx, by, bd) = self.screen(b);
        ((ad + bd) / 2.0, line2((ax, ay), (bx, by)))
    }
}

/// Axis-aligned bounding box of a point cloud.
fn bounds_of(points: &[V3]) -> Option<(V3, V3)> {
    if points.is_empty() {
        return None;
    }
    let mut min = V3 {
        x: f64::INFINITY,
        y: f64::INFINITY,
        z: f64::INFINITY,
    };
    let mut max = V3 {
        x: f64::NEG_INFINITY,
        y: f64::NEG_INFINITY,
        z: f64::NEG_INFINITY,
    };
    for p in points {
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        min.z = min.z.min(p.z);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
        max.z = max.z.max(p.z);
    }
    Some((min, max))
}

/// Assemble a depth-sorted, depth-shaded `Graphics` from projected segments.
///
/// Mirrors the painter's-algorithm tail of [`builtin_plot3d`] so all projected
/// surface meshes share the same hidden-line-like shading.
fn finish_projected(mut segments: Vec<(f64, Value)>, extra_opts: &[Value], env: &Env) -> Value {
    segments.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let dmin = segments.last().map(|s| s.0).unwrap_or(0.0);
    let dmax = segments.first().map(|s| s.0).unwrap_or(1.0);
    let drange = (dmax - dmin).max(1e-12);

    let mut prims: Vec<Value> = Vec::with_capacity(segments.len() * 2);
    for (depth, line) in segments {
        let t = ((depth - dmin) / drange).clamp(0.0, 1.0);
        prims.push(rgb(0.85 - 0.75 * t, 0.88 - 0.70 * t, 0.95 - 0.45 * t));
        prims.push(line);
    }

    Value::Call {
        head: "Graphics".to_string(),
        args: vec![Value::List(prims), graphics_options(extra_opts, env)],
    }
}

/// Wrap already-projected primitives in a `Graphics` with merged options.
fn finish_prims(prims: Vec<Value>, extra_opts: &[Value], env: &Env) -> Value {
    Value::Call {
        head: "Graphics".to_string(),
        args: vec![Value::List(prims), graphics_options(extra_opts, env)],
    }
}

/// Sample a 3-component function of the given variables.
fn sample3(fx: &Expr, fy: &Expr, fz: &Expr, vars: &[(&str, f64)], env: &Env) -> Option<V3> {
    let x = sample(fx, vars, env)?;
    let y = sample(fy, vars, env)?;
    let z = sample(fz, vars, env)?;
    if x.is_finite() && y.is_finite() && z.is_finite() {
        Some(V3 { x, y, z })
    } else {
        None
    }
}

/// Evaluate an option `key -> number` from a slice of rules (default `None`).
fn option_number(extra: &[Value], key: &str, env: &Env) -> Option<f64> {
    for arg in extra {
        if let Value::Rule { lhs, rhs, .. } = eval_arg(arg, env) {
            let matches = match lhs.as_ref() {
                Value::Symbol(k) | Value::Str(k) => k == key,
                _ => false,
            };
            if matches {
                return super::to_f64(rhs.as_ref());
            }
        }
    }
    None
}

/// Iso-contour level for a contour plot (default 0).
fn contour_level(extra: &[Value], env: &Env) -> f64 {
    option_number(extra, "Contours", env)
        .or_else(|| option_number(extra, "ContourLevels", env))
        .unwrap_or(0.0)
}

/// Marching-squares iso-contour extraction.
///
/// `grid[row][col]` is a scalar field; returned segment endpoints are in
/// *grid index* coordinates `(col, row)` so callers can map them linearly.
fn marching_squares_segments(grid: &[Vec<f64>], level: f64) -> Vec<((f64, f64), (f64, f64))> {
    let mut segs = Vec::new();
    if grid.len() < 2 {
        return segs;
    }
    let ny = grid.len() - 1;
    for j in 0..ny {
        if grid[j].len() < 2 || grid[j + 1].len() < 2 {
            continue;
        }
        let nx = grid[j].len().min(grid[j + 1].len()) - 1;
        for i in 0..nx {
            let v00 = grid[j][i];
            let v10 = grid[j][i + 1];
            let v11 = grid[j + 1][i + 1];
            let v01 = grid[j + 1][i];
            let interp = |a: f64, b: f64| -> f64 {
                if (b - a).abs() < 1e-15 {
                    0.5
                } else {
                    (level - a) / (b - a)
                }
            };
            let mut pts: Vec<(f64, f64)> = Vec::new();
            if (v00 < level) != (v10 < level) {
                pts.push((interp(v00, v10), 0.0));
            }
            if (v10 < level) != (v11 < level) {
                pts.push((1.0, interp(v10, v11)));
            }
            if (v01 < level) != (v11 < level) {
                pts.push((interp(v01, v11), 1.0));
            }
            if (v00 < level) != (v01 < level) {
                pts.push((0.0, interp(v00, v01)));
            }
            let mut emit = |a: (f64, f64), b: (f64, f64)| {
                segs.push((
                    (i as f64 + a.0, j as f64 + a.1),
                    (i as f64 + b.0, j as f64 + b.1),
                ));
            };
            match pts.len() {
                2 => emit(pts[0], pts[1]),
                4 => {
                    let center = (v00 + v10 + v11 + v01) / 4.0;
                    let pairs = if (center < level) == (v00 < level) {
                        [(0, 1), (2, 3)]
                    } else {
                        [(0, 3), (1, 2)]
                    };
                    for (p, q) in pairs {
                        emit(pts[p], pts[q]);
                    }
                }
                _ => {}
            }
        }
    }
    segs
}

/// Sample `f` over `a_spec × b_spec` with a third variable held fixed.
#[allow(clippy::too_many_arguments)]
fn sample_face(
    f: &Expr,
    a_spec: &IterSpec,
    b_spec: &IterSpec,
    fixed: (&str, f64),
    n: usize,
    env: &Env,
) -> Vec<Vec<f64>> {
    let mut grid = vec![vec![f64::NAN; n]; n];
    for (r, row) in grid.iter_mut().enumerate() {
        let b = b_spec.lerp(r as f64 / (n - 1) as f64);
        for (c, cell) in row.iter_mut().enumerate() {
            let a = a_spec.lerp(c as f64 / (n - 1) as f64);
            if let Some(v) = sample(
                f,
                &[(a_spec.var.as_str(), a), (b_spec.var.as_str(), b), fixed],
                env,
            )
            .filter(|v| v.is_finite())
            {
                *cell = v;
            }
        }
    }
    grid
}

// ── ParametricPlot3D ────────────────────────────────────────────────────────

/// `ParametricPlot3D[{fx, fy, fz}, {u, umin, umax}]` — space curve, or with an
/// extra iterator spec `ParametricPlot3D[{fx, fy, fz}, {u, ..}, {v, ..}]` a
/// parametric surface. Both are projected to a 2D `Graphics`.
pub fn builtin_parametric_plot3d(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = "ParametricPlot3D";
    if args.len() < 2 {
        return Ok(unevaluated(name, args));
    }
    let (fx, fy, fz) = match to_expr(&args[0]) {
        Expr::List(items) if items.len() == 3 => {
            (items[0].clone(), items[1].clone(), items[2].clone())
        }
        _ => return Ok(unevaluated(name, args)),
    };
    let Some(u_spec) = IterSpec::parse(&to_value(&args[1]), env) else {
        return Ok(unevaluated(name, args));
    };

    // A parseable second iterator spec makes this a surface; otherwise the
    // third argument (if any) is treated as an option rule.
    let v_spec = args.get(2).and_then(|a| IterSpec::parse(&to_value(a), env));
    let opt_start = if v_spec.is_some() { 3 } else { 2 };

    if let Some(v_spec) = v_spec {
        let n = PARAM3D_GRID;
        let mut mesh = vec![vec![None::<V3>; n]; n];
        let mut all: Vec<V3> = Vec::new();
        for (j, row) in mesh.iter_mut().enumerate() {
            let v = v_spec.lerp(j as f64 / (n - 1) as f64);
            for (i, cell) in row.iter_mut().enumerate() {
                let u = u_spec.lerp(i as f64 / (n - 1) as f64);
                if let Some(p) = sample3(&fx, &fy, &fz, &[(&u_spec.var, u), (&v_spec.var, v)], env)
                {
                    *cell = Some(p);
                    all.push(p);
                }
            }
        }
        let Some((min, max)) = bounds_of(&all) else {
            return Ok(unevaluated(name, args));
        };
        let proj = Projector::from_bounds(min, max);

        let mut segs = Vec::new();
        #[allow(clippy::needless_range_loop)]
        for j in 0..n {
            for i in 1..n {
                if let (Some(a), Some(b)) = (mesh[j][i - 1], mesh[j][i]) {
                    segs.push(proj.segment(a, b));
                }
            }
        }
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            for j in 1..n {
                if let (Some(a), Some(b)) = (mesh[j - 1][i], mesh[j][i]) {
                    segs.push(proj.segment(a, b));
                }
            }
        }
        if segs.is_empty() {
            return Ok(unevaluated(name, args));
        }
        return Ok(finish_projected(segs, &args[opt_start..], env));
    }

    // Space curve.
    let m = PARAM3D_CURVE;
    let mut pts: Vec<V3> = Vec::with_capacity(m);
    for i in 0..m {
        let u = u_spec.lerp(i as f64 / (m - 1) as f64);
        if let Some(p) = sample3(&fx, &fy, &fz, &[(&u_spec.var, u)], env) {
            pts.push(p);
        }
    }
    let Some((min, max)) = bounds_of(&pts) else {
        return Ok(unevaluated(name, args));
    };
    if pts.len() < 2 {
        return Ok(unevaluated(name, args));
    }
    let proj = Projector::from_bounds(min, max);
    let screen: Vec<Value> = pts
        .iter()
        .map(|p| {
            let (sx, sy, _) = proj.screen(*p);
            point2(sx, sy)
        })
        .collect();
    let prims = vec![Value::Call {
        head: "Line".to_string(),
        args: vec![Value::List(screen)],
    }];
    Ok(finish_prims(prims, &args[opt_start..], env))
}

// ── ContourPlot3D ───────────────────────────────────────────────────────────

/// `ContourPlot3D[f, {x, ..}, {y, ..}, {z, ..}]` — basic iso-surface.
///
/// The surface `f == level` (default 0) is approximated by marching-squares
/// contours on every axis-aligned grid slice, then projected to 2D.
#[allow(clippy::needless_range_loop)]
pub fn builtin_contour_plot3d(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = "ContourPlot3D";
    if args.len() < 4 {
        return Ok(unevaluated(name, args));
    }
    let f = to_expr(&args[0]);
    let (Some(xs), Some(ys), Some(zs)) = (
        IterSpec::parse(&to_value(&args[1]), env),
        IterSpec::parse(&to_value(&args[2]), env),
        IterSpec::parse(&to_value(&args[3]), env),
    ) else {
        return Ok(unevaluated(name, args));
    };
    let level = contour_level(&args[4..], env);

    let n = CONTOUR3D_GRID;
    let mut grid = vec![vec![vec![f64::NAN; n]; n]; n];
    let mut any = false;
    #[allow(clippy::needless_range_loop)]
    for k in 0..n {
        let z = zs.lerp(k as f64 / (n - 1) as f64);
        for j in 0..n {
            let y = ys.lerp(j as f64 / (n - 1) as f64);
            for i in 0..n {
                let x = xs.lerp(i as f64 / (n - 1) as f64);
                if let Some(v) = sample(&f, &[(&xs.var, x), (&ys.var, y), (&zs.var, z)], env)
                    .filter(|v| v.is_finite())
                {
                    grid[k][j][i] = v;
                    any = true;
                }
            }
        }
    }
    if !any {
        return Ok(unevaluated(name, args));
    }
    let proj = Projector::from_bounds(
        V3 {
            x: xs.min,
            y: ys.min,
            z: zs.min,
        },
        V3 {
            x: xs.max,
            y: ys.max,
            z: zs.max,
        },
    );

    let mut segs = Vec::new();
    // Slice normal to z: contour over (x, y).
    for k in 0..n {
        let z = zs.lerp(k as f64 / (n - 1) as f64);
        let slice: Vec<Vec<f64>> = (0..n).map(|j| grid[k][j].clone()).collect();
        for (a, b) in marching_squares_segments(&slice, level) {
            segs.push(proj.segment(
                V3 {
                    x: xs.lerp(a.0 / (n - 1) as f64),
                    y: ys.lerp(a.1 / (n - 1) as f64),
                    z,
                },
                V3 {
                    x: xs.lerp(b.0 / (n - 1) as f64),
                    y: ys.lerp(b.1 / (n - 1) as f64),
                    z,
                },
            ));
        }
    }
    // Slice normal to x: contour over (y, z).
    for i in 0..n {
        let x = xs.lerp(i as f64 / (n - 1) as f64);
        let slice: Vec<Vec<f64>> = (0..n)
            .map(|k| (0..n).map(|j| grid[k][j][i]).collect())
            .collect();
        for (a, b) in marching_squares_segments(&slice, level) {
            segs.push(proj.segment(
                V3 {
                    x,
                    y: ys.lerp(a.0 / (n - 1) as f64),
                    z: zs.lerp(a.1 / (n - 1) as f64),
                },
                V3 {
                    x,
                    y: ys.lerp(b.0 / (n - 1) as f64),
                    z: zs.lerp(b.1 / (n - 1) as f64),
                },
            ));
        }
    }
    // Slice normal to y: contour over (x, z).
    for j in 0..n {
        let y = ys.lerp(j as f64 / (n - 1) as f64);
        let slice: Vec<Vec<f64>> = (0..n)
            .map(|k| (0..n).map(|i| grid[k][j][i]).collect())
            .collect();
        for (a, b) in marching_squares_segments(&slice, level) {
            segs.push(proj.segment(
                V3 {
                    x: xs.lerp(a.0 / (n - 1) as f64),
                    y,
                    z: zs.lerp(a.1 / (n - 1) as f64),
                },
                V3 {
                    x: xs.lerp(b.0 / (n - 1) as f64),
                    y,
                    z: zs.lerp(b.1 / (n - 1) as f64),
                },
            ));
        }
    }

    if segs.is_empty() {
        return Ok(unevaluated(name, args));
    }
    Ok(finish_projected(segs, &args[4..], env))
}

// ── SliceContourPlot3D ──────────────────────────────────────────────────────

/// `SliceContourPlot3D[f, {x, ..}, {y, ..}, {z, ..}]` — contours drawn on the
/// six boundary faces of the sampling box.
pub fn builtin_slice_contour_plot3d(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = "SliceContourPlot3D";
    if args.len() < 4 {
        return Ok(unevaluated(name, args));
    }
    let f = to_expr(&args[0]);
    let (Some(xs), Some(ys), Some(zs)) = (
        IterSpec::parse(&to_value(&args[1]), env),
        IterSpec::parse(&to_value(&args[2]), env),
        IterSpec::parse(&to_value(&args[3]), env),
    ) else {
        return Ok(unevaluated(name, args));
    };
    let level = contour_level(&args[4..], env);

    let n = SLICE3D_GRID;
    let proj = Projector::from_bounds(
        V3 {
            x: xs.min,
            y: ys.min,
            z: zs.min,
        },
        V3 {
            x: xs.max,
            y: ys.max,
            z: zs.max,
        },
    );
    let mut segs = Vec::new();

    // z = const faces.
    for zf in [zs.min, zs.max] {
        let slice = sample_face(&f, &xs, &ys, (&zs.var, zf), n, env);
        for (a, b) in marching_squares_segments(&slice, level) {
            segs.push(proj.segment(
                V3 {
                    x: xs.lerp(a.0 / (n - 1) as f64),
                    y: ys.lerp(a.1 / (n - 1) as f64),
                    z: zf,
                },
                V3 {
                    x: xs.lerp(b.0 / (n - 1) as f64),
                    y: ys.lerp(b.1 / (n - 1) as f64),
                    z: zf,
                },
            ));
        }
    }
    // x = const faces (contour over (y, z)).
    for xf in [xs.min, xs.max] {
        let slice = sample_face(&f, &ys, &zs, (&xs.var, xf), n, env);
        for (a, b) in marching_squares_segments(&slice, level) {
            segs.push(proj.segment(
                V3 {
                    x: xf,
                    y: ys.lerp(a.0 / (n - 1) as f64),
                    z: zs.lerp(a.1 / (n - 1) as f64),
                },
                V3 {
                    x: xf,
                    y: ys.lerp(b.0 / (n - 1) as f64),
                    z: zs.lerp(b.1 / (n - 1) as f64),
                },
            ));
        }
    }
    // y = const faces (contour over (x, z)).
    for yf in [ys.min, ys.max] {
        let slice = sample_face(&f, &xs, &zs, (&ys.var, yf), n, env);
        for (a, b) in marching_squares_segments(&slice, level) {
            segs.push(proj.segment(
                V3 {
                    x: xs.lerp(a.0 / (n - 1) as f64),
                    y: yf,
                    z: zs.lerp(a.1 / (n - 1) as f64),
                },
                V3 {
                    x: xs.lerp(b.0 / (n - 1) as f64),
                    y: yf,
                    z: zs.lerp(b.1 / (n - 1) as f64),
                },
            ));
        }
    }

    if segs.is_empty() {
        return Ok(unevaluated(name, args));
    }
    Ok(finish_projected(segs, &args[4..], env))
}

// ── DensityPlot3D ───────────────────────────────────────────────────────────

/// Colour ramp for density plots (blue → green → red).
fn density_color(t: f64) -> Value {
    let t = t.clamp(0.0, 1.0);
    let r = (1.6 * t - 0.3).clamp(0.0, 1.0);
    let g = (1.0 - (2.0 * t - 1.0).abs()).clamp(0.0, 1.0);
    let b = (0.9 - 1.4 * t).clamp(0.0, 1.0);
    rgb(r, g, b)
}

/// `DensityPlot3D[f, {x, ..}, {y, ..}, {z, ..}]` — volumetric value cloud.
///
/// Each sampled cell becomes a depth-sorted, value-shaded projected square.
pub fn builtin_density_plot3d(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = "DensityPlot3D";
    if args.len() < 4 {
        return Ok(unevaluated(name, args));
    }
    let f = to_expr(&args[0]);
    let (Some(xs), Some(ys), Some(zs)) = (
        IterSpec::parse(&to_value(&args[1]), env),
        IterSpec::parse(&to_value(&args[2]), env),
        IterSpec::parse(&to_value(&args[3]), env),
    ) else {
        return Ok(unevaluated(name, args));
    };

    let n = DENSITY3D_GRID;
    let mut cells: Vec<(V3, f64)> = Vec::new();
    let (mut vmin, mut vmax) = (f64::INFINITY, f64::NEG_INFINITY);
    #[allow(clippy::needless_range_loop)]
    for k in 0..n {
        let z = zs.lerp(k as f64 / (n - 1) as f64);
        for j in 0..n {
            let y = ys.lerp(j as f64 / (n - 1) as f64);
            for i in 0..n {
                let x = xs.lerp(i as f64 / (n - 1) as f64);
                if let Some(v) = sample(&f, &[(&xs.var, x), (&ys.var, y), (&zs.var, z)], env)
                    .filter(|v| v.is_finite())
                {
                    cells.push((V3 { x, y, z }, v));
                    vmin = vmin.min(v);
                    vmax = vmax.max(v);
                }
            }
        }
    }
    if cells.is_empty() {
        return Ok(unevaluated(name, args));
    }
    let range = (vmax - vmin).max(1e-12);
    let proj = Projector::from_bounds(
        V3 {
            x: xs.min,
            y: ys.min,
            z: zs.min,
        },
        V3 {
            x: xs.max,
            y: ys.max,
            z: zs.max,
        },
    );
    let half = 0.5 / (n - 1) as f64;

    let mut rects: Vec<(f64, Value)> = Vec::with_capacity(cells.len());
    for (p, v) in cells {
        let (sx, sy, depth) = proj.screen(p);
        let t = (v - vmin) / range;
        let mut prims = vec![density_color(t)];
        prims.push(Value::Call {
            head: "Rectangle".to_string(),
            args: vec![point2(sx - half, sy - half), point2(sx + half, sy + half)],
        });
        rects.push((depth, Value::List(prims)));
    }

    // Far cells first so nearer ones overwrite them.
    rects.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let prims: Vec<Value> = rects
        .into_iter()
        .flat_map(|(_, v)| match v {
            Value::List(items) => items,
            other => vec![other],
        })
        .collect();
    Ok(finish_prims(prims, &args[4..], env))
}

// ── RegionPlot3D ────────────────────────────────────────────────────────────

/// `RegionPlot3D[pred, {x, ..}, {y, ..}, {z, ..}]` — filled voxel region.
///
/// The predicate is sampled on a voxel grid; every cell whose centre satisfies
/// it is kept and projected as a depth-sorted, depth-shaded square, so the
/// region renders as a solid cloud through the existing SVG pipeline.
pub fn builtin_region_plot3d(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = "RegionPlot3D";
    if args.len() < 4 {
        return Ok(unevaluated(name, args));
    }
    let pred = to_expr(&args[0]);
    let (Some(xs), Some(ys), Some(zs)) = (
        IterSpec::parse(&to_value(&args[1]), env),
        IterSpec::parse(&to_value(&args[2]), env),
        IterSpec::parse(&to_value(&args[3]), env),
    ) else {
        return Ok(unevaluated(name, args));
    };

    let n = REGION3D_GRID;
    let proj = Projector::from_bounds(
        V3 {
            x: xs.min,
            y: ys.min,
            z: zs.min,
        },
        V3 {
            x: xs.max,
            y: ys.max,
            z: zs.max,
        },
    );
    let half = 0.5 / (n - 1) as f64;

    let mut cells: Vec<(f64, f64, f64)> = Vec::new();
    let (mut dmin, mut dmax) = (f64::INFINITY, f64::NEG_INFINITY);
    #[allow(clippy::needless_range_loop)]
    for k in 0..n {
        let z = zs.lerp(k as f64 / (n - 1) as f64);
        for j in 0..n {
            let y = ys.lerp(j as f64 / (n - 1) as f64);
            for i in 0..n {
                let x = xs.lerp(i as f64 / (n - 1) as f64);
                if sample_pred(&pred, &[(&xs.var, x), (&ys.var, y), (&zs.var, z)], env) {
                    let (sx, sy, depth) = proj.screen(V3 { x, y, z });
                    dmin = dmin.min(depth);
                    dmax = dmax.max(depth);
                    cells.push((sx, sy, depth));
                }
            }
        }
    }
    if cells.is_empty() {
        return Ok(unevaluated(name, args));
    }

    // Far cells first so nearer ones overwrite them.
    cells.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
    let drange = (dmax - dmin).max(1e-12);

    let mut prims: Vec<Value> = Vec::with_capacity(cells.len() * 2);
    for (sx, sy, depth) in cells {
        let t = ((depth - dmin) / drange).clamp(0.0, 1.0);
        prims.push(rgb(0.30 + 0.35 * t, 0.55 + 0.30 * t, 0.85 + 0.10 * t));
        prims.push(Value::Call {
            head: "Rectangle".to_string(),
            args: vec![point2(sx - half, sy - half), point2(sx + half, sy + half)],
        });
    }
    Ok(finish_prims(prims, &args[4..], env))
}

// ── VectorPlot3D ────────────────────────────────────────────────────────────

/// `VectorPlot3D[{vx, vy, vz}, {x, ..}, {y, ..}, {z, ..}]` — 3D arrow field.
pub fn builtin_vector_plot3d(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = "VectorPlot3D";
    if args.len() < 4 {
        return Ok(unevaluated(name, args));
    }
    let (vx, vy, vz) = match to_expr(&args[0]) {
        Expr::List(items) if items.len() == 3 => {
            (items[0].clone(), items[1].clone(), items[2].clone())
        }
        _ => return Ok(unevaluated(name, args)),
    };
    let (Some(xs), Some(ys), Some(zs)) = (
        IterSpec::parse(&to_value(&args[1]), env),
        IterSpec::parse(&to_value(&args[2]), env),
        IterSpec::parse(&to_value(&args[3]), env),
    ) else {
        return Ok(unevaluated(name, args));
    };

    let n = VECTOR3D_GRID;
    let mut arrows: Vec<(V3, V3)> = Vec::new();
    let mut max_mag = 0.0_f64;
    #[allow(clippy::needless_range_loop)]
    for k in 0..n {
        let z = zs.lerp(k as f64 / (n - 1) as f64);
        for j in 0..n {
            let y = ys.lerp(j as f64 / (n - 1) as f64);
            for i in 0..n {
                let x = xs.lerp(i as f64 / (n - 1) as f64);
                let vars = [
                    (xs.var.as_str(), x),
                    (ys.var.as_str(), y),
                    (zs.var.as_str(), z),
                ];
                let (vx, vy, vz) = (
                    sample(&vx, &vars, env),
                    sample(&vy, &vars, env),
                    sample(&vz, &vars, env),
                );
                if let (Some(a), Some(b), Some(c)) = (vx, vy, vz)
                    && a.is_finite()
                    && b.is_finite()
                    && c.is_finite()
                {
                    let mag = (a * a + b * b + c * c).sqrt();
                    if mag > 1e-12 {
                        max_mag = max_mag.max(mag);
                        arrows.push((V3 { x, y, z }, V3 { x: a, y: b, z: c }));
                    }
                }
            }
        }
    }
    if arrows.is_empty() {
        return Ok(unevaluated(name, args));
    }

    let proj = Projector::from_bounds(
        V3 {
            x: xs.min,
            y: ys.min,
            z: zs.min,
        },
        V3 {
            x: xs.max,
            y: ys.max,
            z: zs.max,
        },
    );
    let cell = ((xs.max - xs.min).min(ys.max - ys.min).min(zs.max - zs.min)) / (n - 1) as f64;
    let max_len = 0.9 * cell;
    let head_angle = 2.6_f64;

    let mut prims = vec![rgb(0.10, 0.45, 0.91)];
    for (base, dir) in arrows {
        let mag = (dir.x * dir.x + dir.y * dir.y + dir.z * dir.z).sqrt();
        let len = max_len * mag / max_mag;
        let tip3 = V3 {
            x: base.x + dir.x / mag * len,
            y: base.y + dir.y / mag * len,
            z: base.z + dir.z / mag * len,
        };
        let (bx, by, _) = proj.screen(base);
        let (tx, ty, _) = proj.screen(tip3);
        prims.push(line2((bx, by), (tx, ty)));
        let (dx, dy) = (tx - bx, ty - by);
        let dlen = (dx * dx + dy * dy).sqrt().max(1e-9);
        let (ux, uy) = (dx / dlen, dy / dlen);
        let hl = (dlen * 0.35).max(1e-9);
        for sign in [head_angle, -head_angle] {
            let (sn, cs) = sign.sin_cos();
            let hx = ux * cs - uy * sn;
            let hy = ux * sn + uy * cs;
            prims.push(line2((tx, ty), (tx + hx * hl, ty + hy * hl)));
        }
    }
    Ok(finish_prims(prims, &args[4..], env))
}

// ── Density overlays (2D) ───────────────────────────────────────────────────

/// Build a `DensityRaster` primitive from a row-major value grid.
#[allow(clippy::too_many_arguments)]
fn density_raster(
    vals: &[f64],
    nx: usize,
    ny: usize,
    xr: (f64, f64),
    yr: (f64, f64),
    vmin: f64,
    vmax: f64,
) -> Value {
    Value::Call {
        head: "DensityRaster".to_string(),
        args: vec![
            Value::List(vals.iter().map(|v| super::real(*v)).collect()),
            Value::List(vec![super::real(xr.0), super::real(xr.1)]),
            Value::List(vec![super::real(yr.0), super::real(yr.1)]),
            Value::List(vec![
                Value::Integer(rug::Integer::from(nx as i64)),
                Value::Integer(rug::Integer::from(ny as i64)),
            ]),
            super::real(vmin),
            super::real(vmax),
        ],
    }
}

/// Magnitude background over `x_spec × y_spec` as a `DensityRaster`.
fn magnitude_raster(
    field: &impl Fn(f64, f64) -> Option<(f64, f64)>,
    x_spec: &IterSpec,
    y_spec: &IterSpec,
) -> Value {
    let n = DENSITY_BG;
    let mut flat = vec![0.0; n * n];
    let (mut vmin, mut vmax) = (f64::INFINITY, f64::NEG_INFINITY);
    for j in 0..n {
        let y = y_spec.lerp(j as f64 / (n - 1) as f64);
        for i in 0..n {
            let x = x_spec.lerp(i as f64 / (n - 1) as f64);
            let mag = field(x, y)
                .map(|(a, b)| (a * a + b * b).sqrt())
                .unwrap_or(0.0);
            flat[j * n + i] = mag;
            vmin = vmin.min(mag);
            vmax = vmax.max(mag);
        }
    }
    let vmax = if (vmax - vmin).abs() < 1e-12 {
        vmin + 1.0
    } else {
        vmax
    };
    density_raster(
        &flat,
        n,
        n,
        (x_spec.min, x_spec.max),
        (y_spec.min, y_spec.max),
        vmin,
        vmax,
    )
}

/// `StreamDensityPlot[{vx, vy}, {x, ..}, {y, ..}]` — streamlines over a
/// magnitude-density background.
pub fn builtin_stream_density_plot(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = "StreamDensityPlot";
    if args.len() < 3 {
        return Ok(unevaluated(name, args));
    }
    let (vx, vy) = match to_expr(&args[0]) {
        Expr::List(items) if items.len() == 2 => (items[0].clone(), items[1].clone()),
        _ => return Ok(unevaluated(name, args)),
    };
    let (Some(xs), Some(ys)) = (
        IterSpec::parse(&to_value(&args[1]), env),
        IterSpec::parse(&to_value(&args[2]), env),
    ) else {
        return Ok(unevaluated(name, args));
    };

    let field = |x: f64, y: f64| -> Option<(f64, f64)> {
        let a = sample(&vx, &[(&xs.var, x), (&ys.var, y)], env)?;
        let b = sample(&vy, &[(&xs.var, x), (&ys.var, y)], env)?;
        if a.is_finite() && b.is_finite() {
            Some((a, b))
        } else {
            None
        }
    };

    let mut prims = vec![magnitude_raster(&field, &xs, &ys), rgb(0.05, 0.05, 0.2)];
    let h = ((xs.max - xs.min).min(ys.max - ys.min)) / 60.0;
    let bounds = (xs.min, xs.max, ys.min, ys.max);
    for sj in 0..STREAM_SEEDS {
        let y0 = ys.lerp((sj as f64 + 0.5) / STREAM_SEEDS as f64);
        for si in 0..STREAM_SEEDS {
            let x0 = xs.lerp((si as f64 + 0.5) / STREAM_SEEDS as f64);
            for dir in [1.0_f64, -1.0] {
                let pts = streamline(&field, x0, y0, h, dir, bounds);
                if pts.len() >= 3 {
                    prims.push(Value::Call {
                        head: "Line".to_string(),
                        args: vec![Value::List(
                            pts.into_iter().map(|(x, y)| point2(x, y)).collect(),
                        )],
                    });
                }
            }
        }
    }
    Ok(finish_prims(prims, &args[3..], env))
}

/// `VectorDensityPlot[{vx, vy}, {x, ..}, {y, ..}]` — arrows over a
/// magnitude-density background.
pub fn builtin_vector_density_plot(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = "VectorDensityPlot";
    if args.len() < 3 {
        return Ok(unevaluated(name, args));
    }
    let (vx, vy) = match to_expr(&args[0]) {
        Expr::List(items) if items.len() == 2 => (items[0].clone(), items[1].clone()),
        _ => return Ok(unevaluated(name, args)),
    };
    let (Some(xs), Some(ys)) = (
        IterSpec::parse(&to_value(&args[1]), env),
        IterSpec::parse(&to_value(&args[2]), env),
    ) else {
        return Ok(unevaluated(name, args));
    };

    let field = |x: f64, y: f64| -> Option<(f64, f64)> {
        let a = sample(&vx, &[(&xs.var, x), (&ys.var, y)], env)?;
        let b = sample(&vy, &[(&xs.var, x), (&ys.var, y)], env)?;
        if a.is_finite() && b.is_finite() {
            Some((a, b))
        } else {
            None
        }
    };
    let (arrows, max_mag) = collect_arrows(&vx, &vy, &xs, &ys, env);
    if arrows.is_empty() {
        return Ok(unevaluated(name, args));
    }

    let mut prims = vec![magnitude_raster(&field, &xs, &ys), rgb(0.05, 0.05, 0.2)];
    push_arrows(&mut prims, &arrows, max_mag, &xs, &ys);
    Ok(finish_prims(prims, &args[3..], env))
}

// ── GraphicsComplex ─────────────────────────────────────────────────────────

/// Heads whose integer arguments are treated as point indices.
fn is_geometry_head(h: &str) -> bool {
    matches!(
        h,
        "Line"
            | "Point"
            | "Polygon"
            | "Tube"
            | "Arrow"
            | "Cuboid"
            | "Rectangle"
            | "BSplineCurve"
            | "BezierCurve"
            | "BSplineSurface"
            | "BezierSurface"
            | "GraphicsComplex"
    )
}

/// Replace integer indices inside a primitive with the referenced point.
fn resolve_complex(pts: &[Value], v: &Value, depth: usize) -> Option<Value> {
    if depth > 64 {
        return Some(v.clone());
    }
    match v {
        Value::Integer(i) => {
            let idx = i.to_i64()?;
            if idx >= 1 && (idx as usize) <= pts.len() {
                Some(pts[(idx - 1) as usize].clone())
            } else {
                Some(v.clone())
            }
        }
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                out.push(resolve_complex(pts, it, depth + 1)?);
            }
            Some(Value::List(out))
        }
        Value::Call { head, args } if is_geometry_head(head) => {
            let mut out = Vec::with_capacity(args.len());
            for a in args {
                out.push(resolve_complex(pts, a, depth + 1)?);
            }
            Some(Value::Call {
                head: head.clone(),
                args: out,
            })
        }
        other => Some(other.clone()),
    }
}

/// `GraphicsComplex[pts, primitives]` — resolve point indices into coordinates.
///
/// Index-only geometry such as `Line[{1, 2}]` is not valid standalone, so the
/// arguments are held: the points are evaluated and the primitives fall back to
/// their raw form when they do not validate on their own. The result is a flat
/// primitive list so `Graphics[GraphicsComplex[...]]` renders through the
/// existing SVG pipeline (which does not know the head).
pub fn builtin_graphics_complex(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = "GraphicsComplex";
    if args.len() != 2 {
        return Ok(unevaluated(name, args));
    }
    let points = eval_arg(&args[0], env);
    let Value::List(pts) = &points else {
        return Ok(unevaluated(name, args));
    };
    let prims = eval_arg(&args[1], env);
    let Some(resolved) = resolve_complex(pts, &prims, 0) else {
        return Ok(unevaluated(name, args));
    };
    Ok(match resolved {
        Value::List(items) => Value::List(items),
        other => Value::List(vec![other]),
    })
}

// ── List plots (2D) ─────────────────────────────────────────────────────────

/// Merge option rules without evaluator access (arguments already evaluated).
fn graphics_options_pure(extra: &[Value]) -> Value {
    let mut map: HashMap<String, Value> = HashMap::new();
    map.insert(
        "ImageSize".to_string(),
        Value::List(vec![super::real(400.0), super::real(400.0)]),
    );
    map.insert("Axes".to_string(), Value::Bool(true));
    for arg in extra {
        if let Value::Rule { lhs, rhs, .. } = arg {
            let key = match lhs.as_ref() {
                Value::Symbol(k) | Value::Str(k) => k.clone(),
                _ => continue,
            };
            map.insert(key, rhs.as_ref().clone());
        }
    }
    Value::Assoc(map)
}

fn finish_prims_pure(prims: Vec<Value>, extra: &[Value]) -> Value {
    Value::Call {
        head: "Graphics".to_string(),
        args: vec![Value::List(prims), graphics_options_pure(extra)],
    }
}

/// Parse a rectangular numeric array into rows of `f64`.
fn numeric_grid(data: &Value) -> Option<Vec<Vec<f64>>> {
    let Value::List(rows) = data else {
        return None;
    };
    if rows.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let Value::List(cells) = row else {
            return None;
        };
        if cells.is_empty() {
            return None;
        }
        let mut vals = Vec::with_capacity(cells.len());
        for c in cells {
            vals.push(super::to_f64(c)?);
        }
        out.push(vals);
    }
    Some(out)
}

fn grid_minmax(rows: &[Vec<f64>]) -> Option<(f64, f64)> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for row in rows {
        for &v in row {
            min = min.min(v);
            max = max.max(v);
        }
    }
    if min.is_finite() && max.is_finite() {
        Some((min, max))
    } else {
        None
    }
}

/// `ListContourPlot[array]` — contour lines from a grid of `z` values.
pub fn builtin_list_contour_plot(args: &[Value]) -> Result<Value, EvalError> {
    let name = "ListContourPlot";
    let Some(data) = args.first() else {
        return Ok(unevaluated(name, args));
    };
    let Some(rows) = numeric_grid(data) else {
        return Ok(unevaluated(name, args));
    };
    let (ny, nx) = (rows.len(), rows[0].len());
    if ny < 2 || nx < 2 {
        return Ok(unevaluated(name, args));
    }
    let Some((min, max)) = grid_minmax(&rows) else {
        return Ok(unevaluated(name, args));
    };
    let range = (max - min).max(1e-12);

    let colors = [
        rgb(0.10, 0.45, 0.91),
        rgb(0.92, 0.26, 0.21),
        rgb(0.20, 0.66, 0.33),
        rgb(0.60, 0.15, 0.70),
        rgb(0.95, 0.60, 0.10),
    ];
    let levels = 5;
    let mut prims = Vec::new();
    for (li, color) in colors.iter().enumerate() {
        let level = min + (li as f64 + 0.5) * range / levels as f64;
        let mut at_level: Vec<Value> = Vec::new();
        for (a, b) in marching_squares_segments(&rows, level) {
            at_level.push(line2(
                (a.0 / (nx - 1) as f64, a.1 / (ny - 1) as f64),
                (b.0 / (nx - 1) as f64, b.1 / (ny - 1) as f64),
            ));
        }
        if !at_level.is_empty() {
            prims.push(color.clone());
            prims.extend(at_level);
        }
    }
    if prims.is_empty() {
        return Ok(unevaluated(name, args));
    }
    Ok(finish_prims_pure(prims, &args[1..]))
}

/// `ListDensityPlot[array]` — value heatmap from a grid of `z` values.
pub fn builtin_list_density_plot(args: &[Value]) -> Result<Value, EvalError> {
    let name = "ListDensityPlot";
    let Some(data) = args.first() else {
        return Ok(unevaluated(name, args));
    };
    let Some(rows) = numeric_grid(data) else {
        return Ok(unevaluated(name, args));
    };
    let (ny, nx) = (rows.len(), rows[0].len());
    if ny < 2 || nx < 2 {
        return Ok(unevaluated(name, args));
    }
    let Some((min, max)) = grid_minmax(&rows) else {
        return Ok(unevaluated(name, args));
    };
    let max = if (max - min).abs() < 1e-12 {
        min + 1.0
    } else {
        max
    };
    let flat: Vec<f64> = rows.into_iter().flatten().collect();
    let raster = density_raster(&flat, nx, ny, (0.0, nx as f64), (0.0, ny as f64), min, max);
    Ok(finish_prims_pure(vec![raster], &args[1..]))
}

/// `ListStreamPlot[data]` — streamlines from sampled vector data.
///
/// Accepts either a rectangular array of `{vx, vy}` pairs (unit-spaced grid)
/// or a list of `{x, y, vx, vy}` samples (nearest-neighbour field).
pub fn builtin_list_stream_plot(args: &[Value]) -> Result<Value, EvalError> {
    let name = "ListStreamPlot";
    let Some(data) = args.first() else {
        return Ok(unevaluated(name, args));
    };
    let Value::List(rows) = data else {
        return Ok(unevaluated(name, args));
    };
    if rows.is_empty() {
        return Ok(unevaluated(name, args));
    }

    // Scattered {x, y, vx, vy} form: a flat numeric row distinguishes it from
    // the grid form, whose rows hold `{vx, vy}` pairs.
    if let Value::List(first) = &rows[0]
        && first.len() >= 4
        && first.iter().all(|c| super::to_f64(c).is_some())
    {
        let mut samples: Vec<(f64, f64, f64, f64)> = Vec::new();
        for r in rows {
            if let Value::List(c) = r
                && c.len() >= 4
                && let (Some(x), Some(y), Some(vx), Some(vy)) = (
                    super::to_f64(&c[0]),
                    super::to_f64(&c[1]),
                    super::to_f64(&c[2]),
                    super::to_f64(&c[3]),
                )
            {
                samples.push((x, y, vx, vy));
            }
        }
        if samples.len() < 2 {
            return Ok(unevaluated(name, args));
        }
        let (mut xmin, mut xmax) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY);
        for &(x, y, _, _) in &samples {
            xmin = xmin.min(x);
            xmax = xmax.max(x);
            ymin = ymin.min(y);
            ymax = ymax.max(y);
        }
        let field = |x: f64, y: f64| -> Option<(f64, f64)> {
            let mut best = None;
            let mut best_d = f64::INFINITY;
            for &(sx, sy, vx, vy) in &samples {
                let d = (sx - x) * (sx - x) + (sy - y) * (sy - y);
                if d < best_d {
                    best_d = d;
                    best = Some((vx, vy));
                }
            }
            best
        };
        return Ok(stream_lines(&field, xmin, xmax, ymin, ymax, &args[1..]));
    }

    // Rectangular grid of {vx, vy} pairs.
    let mut vxs: Vec<Vec<f64>> = Vec::with_capacity(rows.len());
    let mut vys: Vec<Vec<f64>> = Vec::with_capacity(rows.len());
    for r in rows {
        let Value::List(cells) = r else {
            return Ok(unevaluated(name, args));
        };
        let mut rx = Vec::with_capacity(cells.len());
        let mut ry = Vec::with_capacity(cells.len());
        for cell in cells {
            let Value::List(pair) = cell else {
                return Ok(unevaluated(name, args));
            };
            if pair.len() < 2 {
                return Ok(unevaluated(name, args));
            }
            let (Some(a), Some(b)) = (super::to_f64(&pair[0]), super::to_f64(&pair[1])) else {
                return Ok(unevaluated(name, args));
            };
            rx.push(a);
            ry.push(b);
        }
        vxs.push(rx);
        vys.push(ry);
    }
    let ny = vxs.len();
    let nx = vxs[0].len();
    if ny < 2 || nx < 2 {
        return Ok(unevaluated(name, args));
    }
    let field = |x: f64, y: f64| -> Option<(f64, f64)> {
        let fx = x.clamp(0.0, (nx - 1) as f64);
        let fy = y.clamp(0.0, (ny - 1) as f64);
        let i0 = (fx.floor() as usize).min(nx - 2);
        let j0 = (fy.floor() as usize).min(ny - 2);
        let tx = fx - i0 as f64;
        let ty = fy - j0 as f64;
        let lerp = |g: &Vec<Vec<f64>>| -> f64 {
            let a = g[j0][i0] * (1.0 - tx) + g[j0][i0 + 1] * tx;
            let b = g[j0 + 1][i0] * (1.0 - tx) + g[j0 + 1][i0 + 1] * tx;
            a * (1.0 - ty) + b * ty
        };
        Some((lerp(&vxs), lerp(&vys)))
    };
    Ok(stream_lines(
        &field,
        0.0,
        (nx - 1) as f64,
        0.0,
        (ny - 1) as f64,
        &args[1..],
    ))
}

/// Shared streamline rendering over a domain.
fn stream_lines(
    field: &impl Fn(f64, f64) -> Option<(f64, f64)>,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
    extra: &[Value],
) -> Value {
    let h = ((xmax - xmin).min(ymax - ymin)) / 60.0;
    let bounds = (xmin, xmax, ymin, ymax);
    let mut prims = vec![rgb(0.10, 0.45, 0.91)];
    for sj in 0..STREAM_SEEDS {
        let y0 = ymin + (ymax - ymin) * (sj as f64 + 0.5) / STREAM_SEEDS as f64;
        for si in 0..STREAM_SEEDS {
            let x0 = xmin + (xmax - xmin) * (si as f64 + 0.5) / STREAM_SEEDS as f64;
            for dir in [1.0_f64, -1.0] {
                let pts = streamline(field, x0, y0, h, dir, bounds);
                if pts.len() >= 3 {
                    prims.push(Value::Call {
                        head: "Line".to_string(),
                        args: vec![Value::List(
                            pts.into_iter().map(|(x, y)| point2(x, y)).collect(),
                        )],
                    });
                }
            }
        }
    }
    finish_prims_pure(prims, extra)
}

/// `ListPolarPlot[{{θ, r}, ...}]` — plot points in polar coordinates.
pub fn builtin_list_polar_plot(args: &[Value]) -> Result<Value, EvalError> {
    let name = "ListPolarPlot";
    let Some(data) = args.first() else {
        return Ok(unevaluated(name, args));
    };
    let Value::List(items) = data else {
        return Ok(unevaluated(name, args));
    };
    let mut pts = Vec::with_capacity(items.len());
    for it in items {
        if let Value::List(p) = it
            && p.len() >= 2
            && let (Some(theta), Some(r)) = (super::to_f64(&p[0]), super::to_f64(&p[1]))
        {
            pts.push(point2(r * theta.cos(), r * theta.sin()));
        }
    }
    if pts.is_empty() {
        return Ok(unevaluated(name, args));
    }
    let prims = vec![Value::Call {
        head: "Line".to_string(),
        args: vec![Value::List(pts)],
    }];
    Ok(finish_prims_pure(prims, &args[1..]))
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    use super::{register_builtin, register_builtin_env};

    register_builtin_env(env, "Plot3D", builtin_plot3d);
    register_builtin_env(env, "VectorPlot", builtin_vector_plot);
    register_builtin_env(env, "StreamPlot", builtin_stream_plot);
    register_builtin_env(env, "ParametricPlot3D", builtin_parametric_plot3d);
    register_builtin_env(env, "ContourPlot3D", builtin_contour_plot3d);
    register_builtin_env(env, "DensityPlot3D", builtin_density_plot3d);
    register_builtin_env(env, "RegionPlot3D", builtin_region_plot3d);
    register_builtin_env(env, "VectorPlot3D", builtin_vector_plot3d);
    register_builtin_env(env, "StreamDensityPlot", builtin_stream_density_plot);
    register_builtin_env(env, "VectorDensityPlot", builtin_vector_density_plot);
    register_builtin_env(env, "SliceContourPlot3D", builtin_slice_contour_plot3d);
    register_builtin(env, "Graphics3D", builtin_graphics3d);
    register_builtin_env(env, "GraphicsComplex", builtin_graphics_complex);
    register_builtin(env, "Cuboid", builtin_cuboid);
    register_builtin(env, "ListContourPlot", builtin_list_contour_plot);
    register_builtin(env, "ListDensityPlot", builtin_list_density_plot);
    register_builtin(env, "ListStreamPlot", builtin_list_stream_plot);
    register_builtin(env, "ListPolarPlot", builtin_list_polar_plot);

    // Arguments must not be pre-evaluated: the integrand and iterator specs are
    // sampled with the iterator variables shadowed in a child environment.
    for name in [
        "Plot3D",
        "VectorPlot",
        "StreamPlot",
        "ParametricPlot3D",
        "ContourPlot3D",
        "DensityPlot3D",
        "RegionPlot3D",
        "VectorPlot3D",
        "StreamDensityPlot",
        "VectorDensityPlot",
        "SliceContourPlot3D",
        // Held so index-only primitives such as `Line[{1, 2}]` survive.
        "GraphicsComplex",
    ] {
        env.set_attributes(name, vec!["HoldAll".to_string(), "Protected".to_string()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_str(input: &str) -> Value {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let tokens = crate::lexer::tokenize(input).unwrap_or_else(|e| panic!("lex {input}: {e:?}"));
        let ast = crate::parser::parse(tokens).unwrap_or_else(|e| panic!("parse {input}: {e:?}"));
        crate::eval::eval_program(&ast, &env).unwrap_or_else(|e| panic!("eval {input}: {e:?}"))
    }

    fn count_lines(v: &Value) -> usize {
        match v {
            Value::List(items) => items
                .iter()
                .filter(|i| matches!(i, Value::Call { head, .. } if head == "Line"))
                .count(),
            _ => 0,
        }
    }

    fn graphics_parts(v: &Value) -> (&Value, Value) {
        match v {
            Value::Call { head, args } if head == "Graphics" && args.len() == 2 => {
                (&args[0], args[1].clone())
            }
            _ => panic!("expected Graphics, got {v:?}"),
        }
    }

    #[test]
    fn test_graphics3d_head() {
        let v = eval_str("Graphics3D[{Point[{0, 0, 0}], Line[{{0, 0, 0}, {1, 1, 1}}]}]");
        match &v {
            Value::Call { head, args } if head == "Graphics3D" && args.len() == 2 => {
                assert!(matches!(&args[0], Value::List(p) if p.len() == 2));
            }
            _ => panic!("expected Graphics3D, got {v:?}"),
        }
    }

    #[test]
    fn test_graphics3d_inapplicable() {
        // Non-list first argument → returned unevaluated with the same head.
        let v = builtin_graphics3d(&[Value::Integer(rug::Integer::from(1))]).unwrap();
        assert!(matches!(&v, Value::Call { head, .. } if head == "Graphics3D"));
    }

    #[test]
    fn test_cuboid() {
        let v = eval_str("Cuboid[{0, 0, 0}, {1, 1, 1}]");
        assert!(matches!(&v, Value::Call { head, args } if head == "Cuboid" && args.len() == 2));
    }

    #[test]
    fn test_cuboid_bad_arity() {
        let v = builtin_cuboid(&[Value::List(vec![super::super::real(1.0)])]).unwrap();
        assert!(matches!(&v, Value::Call { head, .. } if head == "Cuboid"));
    }

    #[test]
    fn test_plot3d_graduates_to_graphics() {
        let v = eval_str("Plot3D[Sin[x] + Cos[y], {x, -2, 2}, {y, -2, 2}]");
        let (prims, opts) = graphics_parts(&v);
        assert!(count_lines(prims) > 100, "expected a dense mesh");
        let svg = crate::builtins::graphics::render_svg(prims, &opts).unwrap();
        assert!(svg.contains("<polyline"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_plot3d_holdall_keeps_iterator_variable_local() {
        // The iterator variable must not be substituted by a global binding:
        // if it were, `{x, 0, 1}` would become `{100, 0, 1}` and the spec
        // would fail to parse.
        let v = eval_str("x = 100; Plot3D[x, {x, 0, 1}, {y, 0, 1}]");
        let (prims, _) = graphics_parts(&v);
        assert!(count_lines(prims) > 0);
    }

    #[test]
    fn test_plot3d_inapplicable() {
        let v = eval_str("Plot3D[x, {x, 1, 1}, {y, 0, 1}]");
        assert!(matches!(&v, Value::Call { head, .. } if head == "Plot3D"));
    }

    #[test]
    fn test_vector_plot_graduates_to_graphics() {
        let v = eval_str("VectorPlot[{y, -x}, {x, -1, 1}, {y, -1, 1}]");
        let (prims, opts) = graphics_parts(&v);
        // Each arrow is a shaft plus two head strokes.
        assert!(count_lines(prims) > 20);
        let svg = crate::builtins::graphics::render_svg(prims, &opts).unwrap();
        assert!(svg.contains("<polyline"));
    }

    #[test]
    fn test_stream_plot_graduates_to_graphics() {
        let v = eval_str("StreamPlot[{y, -x}, {x, -2, 2}, {y, -2, 2}]");
        let (prims, opts) = graphics_parts(&v);
        assert!(count_lines(prims) > 0, "expected at least one streamline");
        let svg = crate::builtins::graphics::render_svg(prims, &opts).unwrap();
        assert!(svg.contains("<polyline"));
    }

    #[test]
    fn test_stream_plot_falls_back_to_unevaluated() {
        let v = eval_str("StreamPlot[{y, -x}, {x, 5, 5}, {y, -1, 1}]");
        assert!(matches!(&v, Value::Call { head, .. } if head == "StreamPlot"));
    }

    #[test]
    fn test_density_plot_present() {
        let v = eval_str("DensityPlot[x + y, {x, 0, 1}, {y, 0, 1}]");
        match &v {
            Value::Call { head, args } if head == "Graphics" && !args.is_empty() => {
                assert!(
                    matches!(&args[0], Value::List(p) if p.iter().any(|i| matches!(i, Value::Call { head, .. } if head == "DensityRaster")))
                );
            }
            _ => panic!("expected Graphics with DensityRaster, got {v:?}"),
        }
    }

    #[test]
    fn test_project_is_centered() {
        let c = V3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        };
        let (sx, sy, d) = project(c, c, 1.0);
        assert!(sx.abs() < 1e-12 && sy.abs() < 1e-12 && d.abs() < 1e-12);
    }

    #[test]
    fn test_iter_spec_parse() {
        let env = Env::new();
        let spec = IterSpec::parse(
            &Value::List(vec![
                Value::Symbol("x".to_string()),
                super::super::real(0.0),
                super::super::real(2.0),
            ]),
            &env,
        )
        .unwrap();
        assert_eq!(spec.var, "x");
        assert!((spec.lerp(0.5) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_iter_spec_rejects_degenerate() {
        let env = Env::new();
        let spec = IterSpec::parse(
            &Value::List(vec![
                Value::Symbol("x".to_string()),
                super::super::real(1.0),
                super::super::real(1.0),
            ]),
            &env,
        );
        assert!(spec.is_none());
    }

    #[test]
    fn test_iter_spec_evaluates_symbolic_bound() {
        // Bounds may be symbolic expressions; they get evaluated, the iterator
        // variable itself does not.
        let env = Env::new();
        env.set("hi".to_string(), super::super::real(3.0));
        let spec = IterSpec::parse(
            &Value::List(vec![
                Value::Symbol("x".to_string()),
                super::super::real(0.0),
                Value::Symbol("hi".to_string()),
            ]),
            &env,
        )
        .unwrap();
        assert!((spec.max - 3.0).abs() < 1e-12);
    }

    // ── Extended 3D / field plots ───────────────────────────────────────────

    fn count_head(v: &Value, name: &str) -> usize {
        match v {
            Value::List(items) => items
                .iter()
                .filter(|i| matches!(i, Value::Call { head, .. } if head == name))
                .count(),
            _ => 0,
        }
    }

    /// Render the first `Graphics` argument through the SVG pipeline.
    fn svg_of(v: &Value) -> String {
        let (prims, opts) = graphics_parts(v);
        crate::builtins::graphics::render_svg(prims, &opts).unwrap()
    }

    /// Coordinates of the first `Line` in a primitive list.
    fn first_line_points(v: &Value) -> Vec<(f64, f64)> {
        let Value::List(items) = v else {
            return Vec::new();
        };
        for i in items {
            if let Value::Call { head, args } = i
                && head == "Line"
                && let Some(Value::List(pts)) = args.first()
            {
                return pts
                    .iter()
                    .filter_map(|p| match p {
                        Value::List(c) if c.len() >= 2 => {
                            Some((super::super::to_f64(&c[0])?, super::super::to_f64(&c[1])?))
                        }
                        _ => None,
                    })
                    .collect();
            }
        }
        Vec::new()
    }

    #[test]
    fn test_parametric_plot3d_curve() {
        let v = eval_str("ParametricPlot3D[{Cos[t], Sin[t], t}, {t, 0, 6}]");
        let (prims, _) = graphics_parts(&v);
        assert!(count_lines(prims) >= 1);
        assert!(svg_of(&v).contains("<polyline"));
    }

    #[test]
    fn test_parametric_plot3d_surface() {
        let v = eval_str(
            "ParametricPlot3D[{Cos[u] (1 + Cos[v]/2), Sin[u] (1 + Cos[v]/2), Sin[v]/2}, \
             {u, 0, 6.28}, {v, 0, 6.28}]",
        );
        let (prims, _) = graphics_parts(&v);
        assert!(count_lines(prims) > 100, "expected a dense surface mesh");
    }

    #[test]
    fn test_parametric_plot3d_inapplicable() {
        let v = eval_str("ParametricPlot3D[{x}, {u, 0, 1}]");
        assert!(matches!(&v, Value::Call { head, .. } if head == "ParametricPlot3D"));
    }

    #[test]
    fn test_contour_plot3d_basic() {
        let v =
            eval_str("ContourPlot3D[x^2 + y^2 + z^2 - 1/2, {x, -1, 1}, {y, -1, 1}, {z, -1, 1}]");
        let (prims, _) = graphics_parts(&v);
        assert!(count_lines(prims) > 0, "expected iso-surface segments");
        assert!(svg_of(&v).contains("</svg>"));
    }

    #[test]
    fn test_slice_contour_plot3d_faces() {
        // A plane passes through all six boundary faces, giving a contour on each.
        let v = eval_str("SliceContourPlot3D[x + y + z, {x, -1, 1}, {y, -1, 1}, {z, -1, 1}]");
        let (prims, _) = graphics_parts(&v);
        assert!(count_lines(prims) > 0, "expected contours on box faces");
    }

    #[test]
    fn test_density_plot3d_cloud() {
        let v = eval_str("DensityPlot3D[x + y + z, {x, 0, 1}, {y, 0, 1}, {z, 0, 1}]");
        let (prims, _) = graphics_parts(&v);
        assert!(count_head(prims, "Rectangle") > 0, "expected density cells");
        assert!(svg_of(&v).contains("<rect") || svg_of(&v).contains("</svg>"));
    }

    #[test]
    fn test_vector_plot3d_arrows() {
        let v = eval_str("VectorPlot3D[{y, -x, z}, {x, -1, 1}, {y, -1, 1}, {z, -1, 1}]");
        let (prims, _) = graphics_parts(&v);
        assert!(count_lines(prims) > 3, "expected 3D arrows");
    }

    #[test]
    fn test_stream_density_plot_has_raster_and_lines() {
        let v = eval_str("StreamDensityPlot[{y, -x}, {x, -2, 2}, {y, -2, 2}]");
        let (prims, _) = graphics_parts(&v);
        assert!(count_head(prims, "DensityRaster") == 1);
        assert!(count_lines(prims) > 0);
    }

    #[test]
    fn test_vector_density_plot_has_raster_and_arrows() {
        let v = eval_str("VectorDensityPlot[{y, -x}, {x, -1, 1}, {y, -1, 1}]");
        let (prims, _) = graphics_parts(&v);
        assert!(count_head(prims, "DensityRaster") == 1);
        assert!(count_lines(prims) > 3);
    }

    #[test]
    fn test_graphics_complex_resolves_indices() {
        // Index-only `Line[{1, 2}]` is invalid standalone but valid in a complex.
        let v = eval_str("GraphicsComplex[{{0, 0}, {2, 2}}, Line[{1, 2}]]");
        let pts = first_line_points(&v);
        assert_eq!(pts.len(), 2);
        assert!((pts[0].0 - 0.0).abs() < 1e-12 && (pts[0].1 - 0.0).abs() < 1e-12);
        assert!((pts[1].0 - 2.0).abs() < 1e-12 && (pts[1].1 - 2.0).abs() < 1e-12);
    }

    #[test]
    fn test_graphics_complex_nested_in_graphics_renders() {
        let v = eval_str("Graphics[GraphicsComplex[{{0, 0}, {2, 2}}, Line[{1, 2}]]]");
        let (prims, opts) = graphics_parts(&v);
        let svg = crate::builtins::graphics::render_svg(prims, &opts).unwrap();
        assert!(svg.contains("<polyline"));
    }

    #[test]
    fn test_list_contour_plot() {
        let v = eval_str(
            "ListContourPlot[{{0, 1, 2, 1, 0}, {1, 2, 3, 2, 1}, {2, 3, 4, 3, 2}, \
             {1, 2, 3, 2, 1}, {0, 1, 2, 1, 0}}]",
        );
        let (prims, _) = graphics_parts(&v);
        assert!(count_lines(prims) > 0);
    }

    #[test]
    fn test_list_density_plot() {
        let v = eval_str("ListDensityPlot[{{0, 1}, {2, 3}}]");
        let (prims, _) = graphics_parts(&v);
        assert!(count_head(prims, "DensityRaster") == 1);
    }

    #[test]
    fn test_list_stream_plot_grid() {
        let v = eval_str("ListStreamPlot[Table[{2 - i, j - 2}, {i, 0, 4}, {j, 0, 4}]]");
        let (prims, _) = graphics_parts(&v);
        assert!(count_lines(prims) > 0);
    }

    #[test]
    fn test_list_stream_plot_scattered() {
        let v =
            eval_str("ListStreamPlot[{{0, 0, 1, 0}, {1, 0, 1, 0}, {0, 1, 1, 0}, {1, 1, 1, 0}}]");
        let (prims, _) = graphics_parts(&v);
        assert!(count_lines(prims) > 0);
    }

    #[test]
    fn test_list_polar_plot() {
        let v = eval_str("ListPolarPlot[{{0, 1}, {1.5708, 2}, {3.1416, 1}}]");
        let (prims, _) = graphics_parts(&v);
        let pts = first_line_points(prims);
        assert_eq!(pts.len(), 3);
        // θ = 0 → (r, 0).
        assert!((pts[0].0 - 1.0).abs() < 1e-9 && pts[0].1.abs() < 1e-9);
    }

    #[test]
    fn test_list_polar_plot_inapplicable() {
        let v = builtin_list_polar_plot(&[Value::Integer(rug::Integer::from(1))]).unwrap();
        assert!(matches!(&v, Value::Call { head, .. } if head == "ListPolarPlot"));
    }

    #[test]
    fn test_region_plot3d_filled_ball() {
        let v = eval_str("RegionPlot3D[x^2 + y^2 + z^2 < 1/2, {x, -1, 1}, {y, -1, 1}, {z, -1, 1}]");
        let (prims, _) = graphics_parts(&v);
        assert!(count_head(prims, "Rectangle") > 0, "expected filled voxels");
        assert!(svg_of(&v).contains("</svg>"));
    }

    #[test]
    fn test_region_plot3d_pure_function_predicate() {
        let v = eval_str(
            "RegionPlot3D[(#1^2 + #2^2 + #3^2 < 1/2) &, {x, -1, 1}, {y, -1, 1}, {z, -1, 1}]",
        );
        let (prims, _) = graphics_parts(&v);
        assert!(count_head(prims, "Rectangle") > 0);
    }

    #[test]
    fn test_region_plot3d_empty_region_unevaluated() {
        // No sampled point can satisfy the impossible predicate.
        let v = eval_str("RegionPlot3D[x^2 + y^2 + z^2 < 0, {x, -1, 1}, {y, -1, 1}, {z, -1, 1}]");
        assert!(matches!(&v, Value::Call { head, .. } if head == "RegionPlot3D"));
    }

    #[test]
    fn test_region_plot3d_bad_arity_unevaluated() {
        let v = eval_str("RegionPlot3D[x < 1, {x, 0, 1}]");
        assert!(matches!(&v, Value::Call { head, .. } if head == "RegionPlot3D"));
    }

    #[test]
    fn test_marching_squares_circle() {
        // Sign-change field around a circle produces contour segments.
        let n = 12;
        let grid: Vec<Vec<f64>> = (0..n)
            .map(|j| {
                (0..n)
                    .map(|i| {
                        let x = i as f64 / (n - 1) as f64 - 0.5;
                        let y = j as f64 / (n - 1) as f64 - 0.5;
                        x * x + y * y - 0.09
                    })
                    .collect()
            })
            .collect();
        let segs = marching_squares_segments(&grid, 0.0);
        assert!(segs.len() >= 8, "expected contour ring, got {}", segs.len());
    }
}
