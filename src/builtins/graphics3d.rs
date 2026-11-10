//! 3D and field plots: Graphics3D, Plot3D, StreamPlot, VectorPlot, DensityPlot.
//!
//! These builtins produce symbolic `Graphics` / `Graphics3D` values. The
//! plot-like functions need evaluator access to sample their integrand at grid
//! points (hence the `HoldAll` attribute), and then emit 2D-projected
//! primitives so the resulting `Graphics` renders through the SVG pipeline in
//! [`super::graphics`]. `Graphics3D` is the Wolfram-compatible container head
//! and is stored symbolically.
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

    let n = FIELD_GRID;
    let mut arrows: Vec<(f64, f64, f64, f64, f64)> = Vec::new();
    let mut max_mag = 0.0_f64;
    for j in 0..n {
        let y = y_spec.lerp(j as f64 / (n - 1) as f64);
        for i in 0..n {
            let x = x_spec.lerp(i as f64 / (n - 1) as f64);
            let vx = sample(&vx_expr, &[(&x_spec.var, x), (&y_spec.var, y)], env)
                .filter(|f| f.is_finite());
            let vy = sample(&vy_expr, &[(&x_spec.var, x), (&y_spec.var, y)], env)
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
    if arrows.is_empty() {
        return Ok(unevaluated(name, args));
    }

    let cell = ((x_spec.max - x_spec.min) / (n - 1) as f64)
        .min((y_spec.max - y_spec.min) / (n - 1) as f64);
    let max_len = 0.9 * cell;
    let head_angle = 2.6_f64; // ≈ 150°

    let mut prims = vec![rgb(0.10, 0.45, 0.91)];
    for (x, y, vx, vy, mag) in arrows {
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

    Ok(Value::Call {
        head: "Graphics".to_string(),
        args: vec![Value::List(prims), graphics_options(&args[3..], env)],
    })
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

// ── Registration ────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    use super::{register_builtin, register_builtin_env};

    register_builtin_env(env, "Plot3D", builtin_plot3d);
    register_builtin_env(env, "VectorPlot", builtin_vector_plot);
    register_builtin_env(env, "StreamPlot", builtin_stream_plot);
    register_builtin(env, "Graphics3D", builtin_graphics3d);
    register_builtin(env, "Cuboid", builtin_cuboid);

    // Arguments must not be pre-evaluated: the integrand and iterator specs are
    // sampled with the iterator variables shadowed in a child environment.
    for name in ["Plot3D", "VectorPlot", "StreamPlot"] {
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
}
