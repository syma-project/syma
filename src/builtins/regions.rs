//! Geometric regions: Area/Volume/RegionMeasure, centroid/bounds,
//! set operations, and basic discretization of primitives.
//!
//! Gap-fill module. Measures are returned exactly (`π` stays symbolic) for
//! exact inputs and numerically for real inputs; inapplicable arguments fall
//! back to an unevaluated call.

use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::{Integer, Rational};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn unevaluated(name: &str, args: &[Value]) -> Value {
    Value::Call {
        head: name.to_string(),
        args: args.to_vec(),
    }
}

fn as_rat(v: &Value) -> Option<Rational> {
    match v {
        Value::Integer(i) => Some(Rational::from(i.clone())),
        Value::Rational(r) => Some((**r).clone()),
        _ => None,
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(i) => Some(i.to_f64()),
        Value::Rational(r) => Some(r.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        _ => None,
    }
}

fn rat_val(r: Rational) -> Value {
    // Collapse to an integer when the denominator is 1.
    if r.denom() == &Integer::from(1) {
        Value::Integer(r.numer().clone())
    } else {
        Value::Rational(Box::new(r))
    }
}

/// Build `coeff · π^pi_power`.
fn pi_expr(coeff: Rational, pi_power: u32) -> Value {
    let mut factors: Vec<Value> = Vec::new();
    if coeff != 1 {
        if coeff == -1 {
            factors.push(Value::Integer(Integer::from(-1)));
        } else {
            factors.push(rat_val(coeff));
        }
    }
    for _ in 0..pi_power {
        factors.push(Value::Symbol("Pi".to_string()));
    }
    match factors.len() {
        0 => Value::Integer(Integer::from(1)),
        1 => factors.pop().unwrap(),
        _ => Value::Call {
            head: "Times".to_string(),
            args: factors,
        },
    }
}

/// Numeric value of `coeff · π^pi` for real inputs.
fn numeric(coeff: f64, pi_power: u32) -> Value {
    super::real(coeff * std::f64::consts::PI.powi(pi_power as i32))
}

fn list_of(v: &Value) -> Option<&[Value]> {
    match v {
        Value::List(items) => Some(items),
        _ => None,
    }
}

fn point(v: &Value) -> Option<Vec<f64>> {
    list_of(v)?.iter().map(as_f64).collect()
}

/// Exact coordinate value if all components are exact, else a float.
fn coord(v: &Value) -> Option<Coord> {
    match v {
        Value::Integer(_) | Value::Rational(_) => as_rat(v).map(Coord::Exact),
        Value::Real(r) => Some(Coord::Real(r.to_f64())),
        _ => None,
    }
}

#[derive(Clone)]
enum Coord {
    Exact(Rational),
    Real(f64),
}

impl Coord {
    fn to_f64(&self) -> f64 {
        match self {
            Coord::Exact(r) => r.to_f64(),
            Coord::Real(x) => *x,
        }
    }
}

fn point_coords(v: &Value) -> Option<Vec<Coord>> {
    list_of(v)?.iter().map(coord).collect()
}

// ── Area ────────────────────────────────────────────────────────────────────

/// Compute a measure for a primitive of dimension `dim` (2 → area, 3 → volume)
/// where the leading constant is `π^pi`. Returns `None` when unknown.
fn primitive_measure(head: &str, args: &[Value], dim: u32) -> Option<Value> {
    match (head, dim) {
        ("Rectangle", 2) => {
            let (c0, c1) = rectangle_corners(args)?;
            let (dx, dy) = (diff(&c0[0], &c1[0]), diff(&c0[1], &c1[1]));
            Some(mul_abs(dx, dy))
        }
        ("Triangle", 2) => {
            let p = point_list(args)?;
            if p.len() != 3 {
                return None;
            }
            let area = cross2(&p[1], &p[2], &p[0]);
            Some(halve_abs(area))
        }
        ("Polygon", 2) => {
            let pts = list_of(&args[0])?;
            let coords: Vec<Vec<Coord>> = pts.iter().map(point_coords).collect::<Option<_>>()?;
            Some(shoelace(&coords))
        }
        ("Disk", 2) | ("Circle", 2) => {
            let r = args
                .get(1)
                .map(coord)
                .unwrap_or(Some(Coord::Exact(Rational::from(1))))?;
            match r {
                Coord::Exact(r) => Some(pi_expr(r.clone() * r.clone(), 1)),
                Coord::Real(r) => Some(numeric(r * r, 1)),
            }
        }
        ("MeshRegion", 2) => region_mesh_area(&args[1]),
        ("Sphere", 3) | ("Ball", 3) => {
            let r = args
                .get(1)
                .map(coord)
                .unwrap_or(Some(Coord::Exact(Rational::from(1))))?;
            match r {
                Coord::Exact(r) => Some(pi_expr(
                    r.clone() * r.clone() * r.clone() * Rational::from((4, 3)),
                    1,
                )),
                Coord::Real(r) => Some(numeric(4.0 / 3.0 * r * r * r, 1)),
            }
        }
        ("Cuboid", 3) => {
            let (c0, c1) = cuboid_corners(args)?;
            let mut acc = Some(Coord::Exact(Rational::from(1)));
            for i in 0..3 {
                acc = Some(mul_coord_ref(acc?, diff(&c0[i], &c1[i])));
            }
            Some(coord_to_value_abs(acc?))
        }
        ("Cylinder", 3) => {
            let (base, top, r) = endpoints_radius(args)?;
            let h = norm(&top, &base);
            match (h, r) {
                (Coord::Exact(h), Coord::Exact(r)) => Some(pi_expr(h * r.clone() * r, 1)),
                (h, r) => Some(numeric(h.to_f64() * r.to_f64().powi(2), 1)),
            }
        }
        ("Cone", 3) => {
            let (base, top, r) = endpoints_radius(args)?;
            let h = norm(&top, &base);
            match (h, r) {
                (Coord::Exact(h), Coord::Exact(r)) => {
                    Some(pi_expr(h * r.clone() * r / Rational::from(3), 1))
                }
                (h, r) => Some(numeric(h.to_f64() * r.to_f64().powi(2) / 3.0, 1)),
            }
        }
        ("Tetrahedron", 3) => {
            let p = point_list(args)?;
            if p.len() != 4 {
                return None;
            }
            Some(tet_volume(&p))
        }
        _ => None,
    }
}

fn region_mesh_area(cells: &Value) -> Option<Value> {
    let cell = list_of(cells)?.first()?;
    if let Value::Call { head, args } = cell
        && head == "Polygon"
    {
        return primitive_measure("Polygon", args, 2);
    }
    None
}

fn rectangle_corners(args: &[Value]) -> Option<(Vec<Coord>, Vec<Coord>)> {
    match (args.len(), &args[0]) {
        (1, Value::List(_)) => {
            let c = point_coords(&args[0])?;
            let zero = vec![Coord::Exact(Rational::from(0)); c.len()];
            Some((zero, c))
        }
        (2, Value::List(_)) => Some((point_coords(&args[0])?, point_coords(&args[1])?)),
        _ => None,
    }
}

fn cuboid_corners(args: &[Value]) -> Option<(Vec<Coord>, Vec<Coord>)> {
    match args.len() {
        0 => Some((
            vec![Coord::Exact(Rational::from(0)); 3],
            vec![Coord::Exact(Rational::from(1)); 3],
        )),
        1 => {
            let c = point_coords(&args[0])?;
            if c.len() != 3 {
                return None;
            }
            let zero = vec![Coord::Exact(Rational::from(0)); 3];
            Some((zero, c))
        }
        2 => Some((point_coords(&args[0])?, point_coords(&args[1])?)),
        _ => None,
    }
}

/// Parse a list of points, either spread across the arguments or carried in a
/// single list argument (as the graphics primitives store them).
fn point_list(args: &[Value]) -> Option<Vec<Vec<Coord>>> {
    if args.len() == 1
        && let Value::List(items) = &args[0]
        && !items.is_empty()
        && items.iter().all(|x| matches!(x, Value::List(_)))
    {
        return items.iter().map(point_coords).collect();
    }
    args.iter().map(point_coords).collect()
}

/// Endpoints and radius of a `Cylinder`/`Cone`: `{{p1, p2}, r?}`.
fn endpoints_radius(args: &[Value]) -> Option<(Vec<Coord>, Vec<Coord>, Coord)> {
    let eps = list_of(args.first()?)?;
    if eps.len() != 2 {
        return None;
    }
    let p0 = point_coords(&eps[0])?;
    let p1 = point_coords(&eps[1])?;
    let r = args
        .get(1)
        .map(coord)
        .unwrap_or(Some(Coord::Exact(Rational::from(1))))?;
    Some((p0, p1, r))
}

fn diff(a: &Coord, b: &Coord) -> Coord {
    match (a, b) {
        (Coord::Exact(x), Coord::Exact(y)) => Coord::Exact(x.clone() - y.clone()),
        _ => Coord::Real(a.to_f64() - b.to_f64()),
    }
}

fn mul_coord_ref(a: Coord, b: Coord) -> Coord {
    match (a, b) {
        (Coord::Exact(x), Coord::Exact(y)) => Coord::Exact(x * y),
        (x, y) => Coord::Real(x.to_f64() * y.to_f64()),
    }
}

fn coord_to_value_abs(c: Coord) -> Value {
    match c {
        Coord::Exact(r) => rat_val(r.abs()),
        Coord::Real(x) => super::real(x.abs()),
    }
}

fn mul_abs(a: Coord, b: Coord) -> Value {
    let c = mul_coord_ref(a, b);
    coord_to_value_abs(c)
}

fn halve_abs(c: Coord) -> Value {
    match c {
        Coord::Exact(r) => rat_val((r.abs()) / Rational::from(2)),
        Coord::Real(x) => super::real(x.abs() / 2.0),
    }
}

fn rat_sqrt(r: &Rational) -> Option<Rational> {
    let f = r.to_f64();
    if f < 0.0 {
        return None;
    }
    let cand = Rational::from((f.sqrt().round() as i64).max(0));
    if cand.clone() * cand.clone() == *r {
        Some(cand)
    } else {
        None
    }
}

fn norm(a: &[Coord], b: &[Coord]) -> Coord {
    let mut acc = Rational::from(0);
    let mut all_exact = true;
    for i in 0..a.len() {
        if let (Coord::Exact(x), Coord::Exact(y)) = (&a[i], &b[i]) {
            let d = x.clone() - y.clone();
            acc += d.clone() * d;
        } else {
            all_exact = false;
            break;
        }
    }
    if all_exact && let Some(root) = rat_sqrt(&acc) {
        return Coord::Exact(root);
    }
    let dx = diff(&a[0], &b[0]).to_f64();
    let dy = diff(&a[1], &b[1]).to_f64();
    let dz = if a.len() > 2 {
        diff(&a[2], &b[2]).to_f64()
    } else {
        0.0
    };
    Coord::Real((dx * dx + dy * dy + dz * dz).sqrt())
}

fn cross2(a: &[Coord], b: &[Coord], o: &[Coord]) -> Coord {
    let ux = diff(&a[0], &o[0]);
    let uy = diff(&a[1], &o[1]);
    let vx = diff(&b[0], &o[0]);
    let vy = diff(&b[1], &o[1]);
    match (&ux, &uy, &vx, &vy) {
        (Coord::Exact(ux), Coord::Exact(uy), Coord::Exact(vx), Coord::Exact(vy)) => {
            Coord::Exact(ux.clone() * vy.clone() - uy.clone() * vx.clone())
        }
        _ => Coord::Real(ux.to_f64() * vy.to_f64() - uy.to_f64() * vx.to_f64()),
    }
}

fn shoelace(pts: &[Vec<Coord>]) -> Value {
    if pts.len() < 3 {
        return Value::Integer(Integer::from(0));
    }
    let mut acc: Option<Coord> = Some(Coord::Exact(Rational::from(0)));
    for i in 0..pts.len() {
        let a = &pts[i];
        let b = &pts[(i + 1) % pts.len()];
        let term = match (&a[0], &a[1], &b[0], &b[1]) {
            (Coord::Exact(ax), Coord::Exact(ay), Coord::Exact(bx), Coord::Exact(by)) => {
                Coord::Exact(ax.clone() * by.clone() - ay.clone() * bx.clone())
            }
            _ => Coord::Real(a[0].to_f64() * b[1].to_f64() - a[1].to_f64() * b[0].to_f64()),
        };
        acc = Some(mul_coord_ref_zero(acc.unwrap(), term));
    }
    halve_abs(acc.unwrap())
}

fn mul_coord_ref_zero(a: Coord, b: Coord) -> Coord {
    match (a, b) {
        (Coord::Exact(x), Coord::Exact(y)) => Coord::Exact(x + y),
        (x, y) => Coord::Real(x.to_f64() + y.to_f64()),
    }
}

fn tet_volume(p: &[Vec<Coord>]) -> Value {
    // Absolute value of the scalar triple product / 6.
    let a: Vec<f64> = (0..3).map(|i| diff(&p[1][i], &p[0][i]).to_f64()).collect();
    let b: Vec<f64> = (0..3).map(|i| diff(&p[2][i], &p[0][i]).to_f64()).collect();
    let c: Vec<f64> = (0..3).map(|i| diff(&p[3][i], &p[0][i]).to_f64()).collect();
    let det = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
        + a[2] * (b[0] * c[1] - b[1] * c[0]);
    super::real((det / 6.0).abs())
}

/// Area[region] — area of a planar region.
pub fn builtin_area(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("Area", args));
    }
    match &args[0] {
        Value::Call { head, args: a } => match primitive_measure(head, a, 2) {
            Some(v) => Ok(v),
            None => Ok(unevaluated("Area", args)),
        },
        _ => Ok(unevaluated("Area", args)),
    }
}

/// Volume[region] — volume of a solid region.
pub fn builtin_volume(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("Volume", args));
    }
    match &args[0] {
        Value::Call { head, args: a } => match primitive_measure(head, a, 3) {
            Some(v) => Ok(v),
            None => Ok(unevaluated("Volume", args)),
        },
        _ => Ok(unevaluated("Volume", args)),
    }
}

/// RegionMeasure[region] — area/volume depending on the region's dimension.
pub fn builtin_region_measure(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("RegionMeasure", args));
    }
    if let Value::Call { head, args: a } = &args[0] {
        if let Some(v) = primitive_measure(head, a, 2) {
            return Ok(v);
        }
        if let Some(v) = primitive_measure(head, a, 3) {
            return Ok(v);
        }
    }
    Ok(unevaluated("RegionMeasure", args))
}

/// RegionCentroid[region] — centre of mass for common primitives.
pub fn builtin_region_centroid(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("RegionCentroid", args));
    }
    let Value::Call { head, args: a } = &args[0] else {
        return Ok(unevaluated("RegionCentroid", args));
    };
    let centroid = match head.as_str() {
        "Disk" | "Circle" | "Sphere" | "Ball" => a.first().cloned(),
        "Rectangle" => rectangle_centroid(a),
        "Cuboid" => cuboid_centroid(a),
        "Cylinder" => midpoint(a),
        "Cone" => cone_centroid(a),
        "Polygon" | "Triangle" => vertex_mean(a),
        _ => None,
    };
    match centroid {
        Some(c) => Ok(c),
        None => Ok(unevaluated("RegionCentroid", args)),
    }
}

fn coord_value(c: &Coord) -> Value {
    match c {
        Coord::Exact(r) => rat_val(r.clone()),
        Coord::Real(x) => super::real(*x),
    }
}

fn rectangle_centroid(args: &[Value]) -> Option<Value> {
    let (c0, c1) = rectangle_corners(args)?;
    let mid: Vec<Value> = c0
        .iter()
        .zip(c1.iter())
        .map(|(x, y)| coord_value(&mid(x, y)))
        .collect();
    Some(Value::List(mid))
}

fn cuboid_centroid(args: &[Value]) -> Option<Value> {
    let (c0, c1) = cuboid_corners(args)?;
    Some(Value::List(
        c0.iter()
            .zip(c1.iter())
            .map(|(x, y)| coord_value(&mid(x, y)))
            .collect(),
    ))
}

fn midpoint(args: &[Value]) -> Option<Value> {
    let (a, b, _) = endpoints_radius(args)?;
    Some(Value::List(
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| coord_value(&mid(x, y)))
            .collect(),
    ))
}

fn cone_centroid(args: &[Value]) -> Option<Value> {
    let (base, top, _) = endpoints_radius(args)?;
    // Centroid lies a quarter of the way from the base to the apex.
    Some(Value::List(
        base.iter()
            .zip(top.iter())
            .map(|(x, y)| {
                let m = match (x, y) {
                    (Coord::Exact(x), Coord::Exact(y)) => {
                        Coord::Exact(x.clone() + (y.clone() - x.clone()) / Rational::from(4))
                    }
                    _ => Coord::Real(x.to_f64() + (y.to_f64() - x.to_f64()) / 4.0),
                };
                coord_value(&m)
            })
            .collect(),
    ))
}

fn vertex_mean(args: &[Value]) -> Option<Value> {
    let pts: Vec<Vec<Coord>> = args.iter().map(point_coords).collect::<Option<_>>()?;
    if pts.is_empty() {
        return None;
    }
    let dim = pts[0].len();
    let n = Rational::from(pts.len() as i64);
    let mut out = Vec::with_capacity(dim);
    for i in 0..dim {
        let mut acc = Coord::Exact(Rational::from(0));
        for p in &pts {
            acc = mul_coord_ref_zero(acc, p[i].clone());
        }
        let v = match acc {
            Coord::Exact(r) => Coord::Exact(r / n.clone()),
            Coord::Real(x) => Coord::Real(x / pts.len() as f64),
        };
        out.push(coord_value(&v));
    }
    Some(Value::List(out))
}

fn mid(a: &Coord, b: &Coord) -> Coord {
    match (a, b) {
        (Coord::Exact(x), Coord::Exact(y)) => {
            Coord::Exact((x.clone() + y.clone()) / Rational::from(2))
        }
        _ => Coord::Real((a.to_f64() + b.to_f64()) / 2.0),
    }
}

/// RegionBounds[region] — minimal bounding box `{min, max}`.
pub fn builtin_region_bounds(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("RegionBounds", args));
    }
    let Value::Call { head, args: a } = &args[0] else {
        return Ok(unevaluated("RegionBounds", args));
    };
    match bounds_of(head, a) {
        Some(v) => Ok(v),
        None => Ok(unevaluated("RegionBounds", args)),
    }
}

fn bounds_of(head: &str, a: &[Value]) -> Option<Value> {
    let (lo, hi): (Vec<Coord>, Vec<Coord>) = match head {
        "Disk" | "Circle" | "Sphere" | "Ball" => {
            let c = point_coords(a.first()?)?;
            let r = a
                .get(1)
                .map(coord)?
                .unwrap_or(Coord::Exact(Rational::from(1)));
            let lo = c.iter().map(|x| diff(x, &r)).collect();
            let hi = c
                .iter()
                .map(|x| mul_coord_ref_zero(x.clone(), r.clone()))
                .collect();
            (lo, hi)
        }
        "Rectangle" => rect_bounds(a)?,
        "Cuboid" => {
            let (c0, c1) = cuboid_corners(a)?;
            minmax(&c0, &c1)?
        }
        "Cylinder" | "Cone" => {
            let (p0, p1, r) = endpoints_radius(a)?;
            let dim = p0.len();
            let lo: Vec<Coord> = (0..dim)
                .map(|i| Coord::Real(p0[i].to_f64().min(p1[i].to_f64()) - r.to_f64()))
                .collect();
            let hi: Vec<Coord> = (0..dim)
                .map(|i| Coord::Real(p0[i].to_f64().max(p1[i].to_f64()) + r.to_f64()))
                .collect();
            (lo, hi)
        }
        "Polygon" => {
            let pts = list_of(&a[0])?;
            let coords: Vec<Vec<Coord>> = pts.iter().map(point_coords).collect::<Option<_>>()?;
            bb_of_points(&coords)?
        }
        "Triangle" | "Tetrahedron" => {
            let coords: Vec<Vec<Coord>> = a.iter().map(point_coords).collect::<Option<_>>()?;
            bb_of_points(&coords)?
        }
        "MeshRegion" => {
            let pts = list_of(&a[0])?;
            let coords: Vec<Vec<Coord>> = pts.iter().map(point_coords).collect::<Option<_>>()?;
            bb_of_points(&coords)?
        }
        _ => return None,
    };
    Some(Value::List(vec![
        Value::List(lo.iter().map(coord_value).collect()),
        Value::List(hi.iter().map(coord_value).collect()),
    ]))
}

fn rect_bounds(a: &[Value]) -> Option<(Vec<Coord>, Vec<Coord>)> {
    let (c0, c1) = rectangle_corners(a)?;
    minmax(&c0, &c1)
}

fn minmax(c0: &[Coord], c1: &[Coord]) -> Option<(Vec<Coord>, Vec<Coord>)> {
    let dim = c0.len().min(c1.len());
    let mut lo = Vec::with_capacity(dim);
    let mut hi = Vec::with_capacity(dim);
    for i in 0..dim {
        let (a, b) = (c0[i].to_f64(), c1[i].to_f64());
        if a <= b {
            lo.push(c0[i].clone());
            hi.push(c1[i].clone());
        } else {
            lo.push(c1[i].clone());
            hi.push(c0[i].clone());
        }
    }
    Some((lo, hi))
}

fn bb_of_points(pts: &[Vec<Coord>]) -> Option<(Vec<Coord>, Vec<Coord>)> {
    let dim = pts.first()?.len();
    let mut lo = pts[0].clone();
    let mut hi = pts[0].clone();
    for p in pts {
        for i in 0..dim {
            if p[i].to_f64() < lo[i].to_f64() {
                lo[i] = p[i].clone();
            }
            if p[i].to_f64() > hi[i].to_f64() {
                hi[i] = p[i].clone();
            }
        }
    }
    Some((lo, hi))
}

/// RegionUnion[...] / RegionIntersection[...] — flattened boolean regions.
pub fn builtin_region_union(args: &[Value]) -> Result<Value, EvalError> {
    region_bool("RegionUnion", args)
}

pub fn builtin_region_intersection(args: &[Value]) -> Result<Value, EvalError> {
    region_bool("RegionIntersection", args)
}

fn region_bool(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(unevaluated(name, args));
    }
    if args.len() == 1 {
        return Ok(args[0].clone());
    }
    let mut flat = Vec::new();
    for a in args {
        if let Value::Call { head, args: inner } = a
            && head == name
        {
            flat.extend(inner.iter().cloned());
        } else {
            flat.push(a.clone());
        }
    }
    Ok(Value::Call {
        head: name.to_string(),
        args: flat,
    })
}

/// DiscretizeRegion[region] — coarse `MeshRegion` approximation.
pub fn builtin_discretize_region(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("DiscretizeRegion", args));
    }
    let Value::Call { head, args: a } = &args[0] else {
        return Ok(unevaluated("DiscretizeRegion", args));
    };
    match head.as_str() {
        "Disk" | "Circle" => {
            let c = match a.first().and_then(point) {
                Some(c) => c,
                None => vec![0.0, 0.0],
            };
            let r = a.get(1).and_then(as_f64).unwrap_or(1.0);
            let (cx, cy) = (c[0], c[1]);
            let n = 60;
            let pts: Vec<Value> = (0..n)
                .map(|i| {
                    let t = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                    Value::List(vec![
                        super::real(cx + r * t.cos()),
                        super::real(cy + r * t.sin()),
                    ])
                })
                .collect();
            Ok(mesh_region(Value::List(pts)))
        }
        "Rectangle" => {
            let (c0, c1) = match rectangle_corners(a) {
                Some(c) => c,
                None => return Ok(unevaluated("DiscretizeRegion", args)),
            };
            let pts = vec![
                pt2(c0[0].to_f64(), c0[1].to_f64()),
                pt2(c1[0].to_f64(), c0[1].to_f64()),
                pt2(c1[0].to_f64(), c1[1].to_f64()),
                pt2(c0[0].to_f64(), c1[1].to_f64()),
            ];
            Ok(mesh_region(Value::List(pts)))
        }
        "Polygon" => {
            let Some(pts) = list_of(&a[0]) else {
                return Ok(unevaluated("DiscretizeRegion", args));
            };
            Ok(mesh_region(Value::List(pts.to_vec())))
        }
        "Cuboid" => {
            let (c0, c1) = match cuboid_corners(a) {
                Some(c) => c,
                None => return Ok(unevaluated("DiscretizeRegion", args)),
            };
            let corners: Vec<Value> = (0..8)
                .map(|i| {
                    Value::List(vec![
                        super::real(if i & 1 == 0 {
                            c0[0].to_f64()
                        } else {
                            c1[0].to_f64()
                        }),
                        super::real(if i & 2 == 0 {
                            c0[1].to_f64()
                        } else {
                            c1[1].to_f64()
                        }),
                        super::real(if i & 4 == 0 {
                            c0[2].to_f64()
                        } else {
                            c1[2].to_f64()
                        }),
                    ])
                })
                .collect();
            Ok(mesh_region(Value::List(corners)))
        }
        _ => Ok(unevaluated("DiscretizeRegion", args)),
    }
}

fn pt2(x: f64, y: f64) -> Value {
    Value::List(vec![super::real(x), super::real(y)])
}

fn mesh_region(pts: Value) -> Value {
    let cell = Value::Call {
        head: "Polygon".to_string(),
        args: vec![pts.clone()],
    };
    Value::Call {
        head: "MeshRegion".to_string(),
        args: vec![pts, Value::List(vec![cell])],
    }
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "Area", builtin_area);
    super::register_builtin(env, "Volume", builtin_volume);
    super::register_builtin(env, "RegionMeasure", builtin_region_measure);
    super::register_builtin(env, "RegionCentroid", builtin_region_centroid);
    super::register_builtin(env, "RegionBounds", builtin_region_bounds);
    super::register_builtin(env, "RegionUnion", builtin_region_union);
    super::register_builtin(env, "RegionIntersection", builtin_region_intersection);
    super::register_builtin(env, "DiscretizeRegion", builtin_discretize_region);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn i(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn call(head: &str, args: Vec<Value>) -> Value {
        Value::Call {
            head: head.to_string(),
            args,
        }
    }

    #[test]
    fn test_area_rectangle() {
        let r = call(
            "Rectangle",
            vec![Value::List(vec![i(0), i(0)]), Value::List(vec![i(2), i(3)])],
        );
        assert_eq!(builtin_area(&[r]).unwrap(), i(6));
    }

    #[test]
    fn test_area_disk_pi() {
        let d = call("Disk", vec![Value::List(vec![i(0), i(0)]), i(2)]);
        let out = builtin_area(&[d]).unwrap().to_string();
        assert!(out.contains("Pi"), "got {out}");
        assert!(out.contains('4'), "got {out}");
    }

    #[test]
    fn test_volume_sphere() {
        let s = call("Sphere", vec![Value::List(vec![i(0), i(0), i(0)]), i(1)]);
        let out = builtin_volume(&[s]).unwrap().to_string();
        assert!(out.contains("Pi"), "got {out}");
    }

    #[test]
    fn test_region_bounds_rectangle() {
        let r = call(
            "Rectangle",
            vec![Value::List(vec![i(1), i(1)]), Value::List(vec![i(3), i(4)])],
        );
        let out = builtin_region_bounds(&[r]).unwrap().to_string();
        assert_eq!(out, "{{1, 1}, {3, 4}}");
    }

    #[test]
    fn test_region_centroid_disk() {
        let d = call("Disk", vec![Value::List(vec![i(1), i(2)]), i(5)]);
        assert_eq!(builtin_region_centroid(&[d]).unwrap().to_string(), "{1, 2}");
    }

    #[test]
    fn test_region_union_flatten() {
        let a = call("Disk", vec![Value::List(vec![i(0), i(0)]), i(1)]);
        let b = call("Disk", vec![Value::List(vec![i(2), i(0)]), i(1)]);
        let out = builtin_region_union(&[a, b]).unwrap();
        assert!(
            matches!(&out, Value::Call { head, args } if head == "RegionUnion" && args.len() == 2)
        );
    }

    #[test]
    fn test_discretize_region_head() {
        let d = call("Disk", vec![Value::List(vec![i(0), i(0)]), i(1)]);
        let out = builtin_discretize_region(&[d]).unwrap();
        assert!(matches!(&out, Value::Call { head, .. } if head == "MeshRegion"));
    }
}
