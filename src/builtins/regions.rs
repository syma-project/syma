//! Geometric regions: Area/Volume/RegionMeasure, centroid/bounds,
//! set operations, and basic discretization of primitives.
//!
//! Gap-fill module. Measures are returned exactly (`π` stays symbolic) for
//! exact inputs and numerically for real inputs; inapplicable arguments fall
//! back to an unevaluated call.

use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::{Integer, Rational};
use std::collections::HashMap;

/// Tolerance for float geometric comparisons.
const EPS: f64 = 1e-9;

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

fn add_coord(a: Coord, b: Coord) -> Coord {
    match (a, b) {
        (Coord::Exact(x), Coord::Exact(y)) => Coord::Exact(x + y),
        (x, y) => Coord::Real(x.to_f64() + y.to_f64()),
    }
}

fn sub_coord(a: &Coord, b: &Coord) -> Coord {
    match (a, b) {
        (Coord::Exact(x), Coord::Exact(y)) => Coord::Exact(x.clone() - y.clone()),
        (x, y) => Coord::Real(x.to_f64() - y.to_f64()),
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
        match head.as_str() {
            "RegionIntersection" if a.len() == 2 => {
                if let Some(r) = box_intersection(&a[0], &a[1]) {
                    return builtin_region_measure(&[r]);
                }
            }
            "RegionUnion" => {
                let mut total: Option<Value> = None;
                for r in a {
                    let v = builtin_region_measure(std::slice::from_ref(r))?;
                    total = Some(match total {
                        None => v,
                        Some(t) => match add_measure(&t, &v) {
                            Some(s) => s,
                            None => return Ok(unevaluated("RegionMeasure", args)),
                        },
                    });
                }
                if let Some(t) = total {
                    return Ok(t);
                }
            }
            _ => {}
        }
    }
    Ok(unevaluated("RegionMeasure", args))
}

fn add_measure(a: &Value, b: &Value) -> Option<Value> {
    match (as_rat(a), as_rat(b)) {
        (Some(x), Some(y)) => Some(rat_val(x + y)),
        _ => Some(super::real(as_f64(a)? + as_f64(b)?)),
    }
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
    let pts: Vec<Vec<Coord>> = point_list(args)?;
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

// ── Membership predicates ───────────────────────────────────────────────────

fn feq(a: f64, b: f64) -> bool {
    (a - b).abs() <= EPS * (1.0 + a.abs().max(b.abs()))
}

fn coord_in(x: f64, lo: f64, hi: f64) -> bool {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    x >= lo - EPS * (1.0 + lo.abs().max(x.abs())) && x <= hi + EPS * (1.0 + hi.abs().max(x.abs()))
}

fn coords_equal(a: &[Coord], b: &[Coord]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| feq(x.to_f64(), y.to_f64()))
}

/// Signed area of triangle `(a, b, c)` (positive when counter-clockwise).
fn orient2(a: &[Coord], b: &[Coord], c: &[Coord]) -> f64 {
    let (ax, ay) = (a[0].to_f64(), a[1].to_f64());
    let (bx, by) = (b[0].to_f64(), b[1].to_f64());
    let (cx, cy) = (c[0].to_f64(), c[1].to_f64());
    (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
}

fn point_in_triangle(p: &[Coord], t: &[Vec<Coord>]) -> bool {
    if p.len() < 2 {
        return false;
    }
    let d1 = orient2(&t[0], &t[1], p);
    let d2 = orient2(&t[1], &t[2], p);
    let d3 = orient2(&t[2], &t[0], p);
    let neg = d1 < -EPS || d2 < -EPS || d3 < -EPS;
    let pos = d1 > EPS || d2 > EPS || d3 > EPS;
    !(neg && pos)
}

/// Signed volume of tetrahedron `(a, b, c, d)`.
fn orient3(a: &[Coord], b: &[Coord], c: &[Coord], d: &[Coord]) -> f64 {
    let ab: Vec<f64> = (0..3).map(|i| b[i].to_f64() - a[i].to_f64()).collect();
    let ac: Vec<f64> = (0..3).map(|i| c[i].to_f64() - a[i].to_f64()).collect();
    let ad: Vec<f64> = (0..3).map(|i| d[i].to_f64() - a[i].to_f64()).collect();
    let cx = ac[1] * ad[2] - ac[2] * ad[1];
    let cy = ac[2] * ad[0] - ac[0] * ad[2];
    let cz = ac[0] * ad[1] - ac[1] * ad[0];
    ab[0] * cx + ab[1] * cy + ab[2] * cz
}

fn point_in_tetrahedron(p: &[Coord], t: &[Vec<Coord>]) -> bool {
    if p.len() < 3 {
        return false;
    }
    // The four sub-tetrahedra formed with `p` partition the total; `p` is
    // inside iff every sub-volume has the same sign as the whole.
    let total = orient3(&t[0], &t[1], &t[2], &t[3]);
    if feq(total, 0.0) {
        return false;
    }
    let sgn = total.signum();
    let subs = [
        orient3(p, &t[1], &t[2], &t[3]),
        orient3(&t[0], p, &t[2], &t[3]),
        orient3(&t[0], &t[1], p, &t[3]),
        orient3(&t[0], &t[1], &t[2], p),
    ];
    subs.iter().all(|d| d * sgn >= -EPS)
}

fn on_segment(p: &[Coord], a: &[Coord], b: &[Coord]) -> bool {
    if p.len() < 2 || a.len() < 2 || b.len() < 2 {
        return false;
    }
    let cross = orient2(a, b, p);
    if !feq(cross, 0.0) {
        return false;
    }
    coord_in(p[0].to_f64(), a[0].to_f64(), b[0].to_f64())
        && coord_in(p[1].to_f64(), a[1].to_f64(), b[1].to_f64())
}

fn point_on_polyline(p: &[Coord], pts: &[Vec<Coord>]) -> bool {
    pts.windows(2).any(|w| on_segment(p, &w[0], &w[1]))
}

fn point_in_polygon(p: &[Coord], pts: &[Vec<Coord>]) -> bool {
    if pts.len() < 3 || p.len() < 2 {
        return false;
    }
    if point_on_polyline(p, pts) || on_segment(p, &pts[pts.len() - 1], &pts[0]) {
        return true;
    }
    let (px, py) = (p[0].to_f64(), p[1].to_f64());
    let mut inside = false;
    let n = pts.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (pts[i][0].to_f64(), pts[i][1].to_f64());
        let (xj, yj) = (pts[j][0].to_f64(), pts[j][1].to_f64());
        if (yi > py) != (yj > py) {
            let xint = (xj - xi) * (py - yi) / (yj - yi) + xi;
            if px < xint {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

fn radius_arg(a: &[Value]) -> Option<Coord> {
    match a.get(1) {
        Some(v) => coord(v),
        None => Some(Coord::Exact(Rational::from(1))),
    }
}

/// Parse `{p, v}` (either nested or as two arguments) for infinite lines.
fn line_pv(a: &[Value]) -> Option<(Vec<Coord>, Vec<Coord>)> {
    match a.len() {
        1 => {
            let l = list_of(&a[0])?;
            if l.len() != 2 {
                return None;
            }
            Some((point_coords(&l[0])?, point_coords(&l[1])?))
        }
        2 => Some((point_coords(&a[0])?, point_coords(&a[1])?)),
        _ => None,
    }
}

fn cylinder_member(p: &[Coord], base: &[Coord], top: &[Coord], r: f64, cone: bool) -> bool {
    if p.len() != base.len() || base.len() != top.len() {
        return false;
    }
    let d: Vec<f64> = (0..base.len())
        .map(|i| top[i].to_f64() - base[i].to_f64())
        .collect();
    let dd: f64 = d.iter().map(|x| x * x).sum();
    if dd == 0.0 {
        return false;
    }
    let t: f64 = (0..base.len())
        .map(|i| (p[i].to_f64() - base[i].to_f64()) * d[i])
        .sum::<f64>()
        / dd;
    if t < -EPS || t > 1.0 + EPS {
        return false;
    }
    let radial: f64 = (0..base.len())
        .map(|i| {
            let proj = base[i].to_f64() + t * d[i];
            let diff = p[i].to_f64() - proj;
            diff * diff
        })
        .sum::<f64>()
        .sqrt();
    let rr = if cone { r * (1.0 - t) } else { r };
    radial <= rr.abs() + EPS
}

fn region_member_geom(head: &str, a: &[Value], p: &[Coord]) -> Option<bool> {
    match head {
        "Disk" | "Circle" | "Ball" | "Sphere" => {
            let c = point_coords(a.first()?)?;
            if p.len() != c.len() {
                return None;
            }
            let r = radius_arg(a)?.to_f64();
            let d2: f64 = p
                .iter()
                .zip(&c)
                .map(|(x, y)| {
                    let d = x.to_f64() - y.to_f64();
                    d * d
                })
                .sum();
            Some(d2 <= r * r + EPS * (1.0 + r * r))
        }
        "Rectangle" => {
            let (c0, c1) = rectangle_corners(a)?;
            if p.len() != c0.len() {
                return None;
            }
            Some((0..p.len()).all(|i| coord_in(p[i].to_f64(), c0[i].to_f64(), c1[i].to_f64())))
        }
        "Cuboid" => {
            let (c0, c1) = cuboid_corners(a)?;
            if p.len() != 3 {
                return None;
            }
            Some((0..3).all(|i| coord_in(p[i].to_f64(), c0[i].to_f64(), c1[i].to_f64())))
        }
        "Triangle" => {
            let t = point_list(a)?;
            if t.len() != 3 {
                return None;
            }
            Some(point_in_triangle(p, &t))
        }
        "Polygon" => {
            let pts = list_of(a.first()?)?;
            let coords: Vec<Vec<Coord>> = pts.iter().map(point_coords).collect::<Option<_>>()?;
            Some(point_in_polygon(p, &coords))
        }
        "Tetrahedron" => {
            let t = point_list(a)?;
            if t.len() != 4 {
                return None;
            }
            Some(point_in_tetrahedron(p, &t))
        }
        "Cylinder" => {
            let (base, top, r) = endpoints_radius(a)?;
            Some(cylinder_member(p, &base, &top, r.to_f64(), false))
        }
        "Cone" => {
            let (base, top, r) = endpoints_radius(a)?;
            Some(cylinder_member(p, &base, &top, r.to_f64(), true))
        }
        "Point" => {
            let c = point_coords(a.first()?)?;
            Some(coords_equal(p, &c))
        }
        "Line" => {
            let pts = list_of(a.first()?)?;
            let coords: Vec<Vec<Coord>> = pts.iter().map(point_coords).collect::<Option<_>>()?;
            Some(point_on_polyline(p, &coords))
        }
        "InfiniteLine" => {
            let (q, v) = line_pv(a)?;
            if p.len() != q.len() || p.len() != v.len() || p.len() < 2 {
                return None;
            }
            let diff: Vec<Coord> = (0..p.len()).map(|i| sub_coord(&p[i], &q[i])).collect();
            // All 2x2 minors of [v, p - q] vanish for collinear points.
            for i in 0..p.len() {
                for j in (i + 1)..p.len() {
                    let minor = sub_coord(
                        &mul_coord_ref(v[i].clone(), diff[j].clone()),
                        &mul_coord_ref(v[j].clone(), diff[i].clone()),
                    );
                    if !feq(minor.to_f64(), 0.0) {
                        return Some(false);
                    }
                }
            }
            Some(true)
        }
        "HalfPlane" => {
            let (q, v) = line_pv(a)?;
            if p.len() < 2 || q.len() < 2 || v.len() < 2 {
                return None;
            }
            let det = v[0].to_f64() * (p[1].to_f64() - q[1].to_f64())
                - v[1].to_f64() * (p[0].to_f64() - q[0].to_f64());
            let scale = 1.0 + v[0].to_f64().abs() * p[1].to_f64().abs();
            Some(det >= -EPS * scale)
        }
        _ => None,
    }
}

fn substitute_value(v: &Value, subs: &HashMap<String, Value>) -> Value {
    match v {
        Value::Symbol(s) => subs.get(s).cloned().unwrap_or_else(|| v.clone()),
        Value::List(items) => {
            Value::List(items.iter().map(|x| substitute_value(x, subs)).collect())
        }
        Value::Call { head, args } => Value::Call {
            head: head.clone(),
            args: args.iter().map(|x| substitute_value(x, subs)).collect(),
        },
        _ => v.clone(),
    }
}

fn implicit_member(a: &[Value], point: &Value, env: &Env) -> Option<bool> {
    if a.len() != 2 {
        return None;
    }
    let vars = list_of(&a[1])?;
    let vals: Vec<Value> = match point {
        Value::List(items) => items.clone(),
        other => vec![other.clone()],
    };
    if vars.len() != vals.len() {
        return None;
    }
    let mut subs = HashMap::new();
    for (v, val) in vars.iter().zip(&vals) {
        if let Value::Symbol(s) = v {
            subs.insert(s.clone(), val.clone());
        } else {
            return None;
        }
    }
    let substituted = substitute_value(&a[0], &subs);
    let expr = crate::eval::table::value_to_expr(&substituted);
    let out = crate::eval::eval(&expr, env).ok()?;
    match out {
        Value::Bool(b) => Some(b),
        Value::Symbol(s) if s == "True" => Some(true),
        Value::Symbol(s) if s == "False" => Some(false),
        _ => None,
    }
}

fn region_member_value(region: &Value, point: &Value, env: &Env) -> Option<bool> {
    let Value::Call { head, args: a } = region else {
        return None;
    };
    match head.as_str() {
        "EmptyRegion" => Some(false),
        "RegionUnion" => {
            let mut any = false;
            for r in a {
                any |= region_member_value(r, point, env)?;
            }
            Some(any)
        }
        "RegionIntersection" => {
            let mut all = true;
            for r in a {
                all &= region_member_value(r, point, env)?;
            }
            Some(all)
        }
        "RegionDifference" => {
            if a.len() != 2 {
                return None;
            }
            let first = region_member_value(&a[0], point, env)?;
            let second = region_member_value(&a[1], point, env)?;
            Some(first && !second)
        }
        "ImplicitRegion" => implicit_member(a, point, env),
        "Interval" => {
            let iv = list_of(a.first()?)?;
            if iv.len() != 2 {
                return None;
            }
            let x = coord(point)?.to_f64();
            Some(coord_in(
                x,
                coord(&iv[0])?.to_f64(),
                coord(&iv[1])?.to_f64(),
            ))
        }
        _ => {
            let p = point_coords(point)?;
            region_member_geom(head, a, &p)
        }
    }
}

/// RegionMember[region, point] — True when `point` lies in `region`.
pub fn builtin_region_member(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("RegionMember", args));
    }
    match region_member_value(&args[0], &args[1], env) {
        Some(b) => Ok(Value::Bool(b)),
        None => Ok(unevaluated("RegionMember", args)),
    }
}

// ── Distance / nearest ──────────────────────────────────────────────────────

fn clamp_coord(c: &Coord, lo: &Coord, hi: &Coord) -> Coord {
    let (x, lof, hif) = (c.to_f64(), lo.to_f64(), hi.to_f64());
    let (lo, hi) = if lof <= hif { (lo, hi) } else { (hi, lo) };
    let (lof, hif) = (lof.min(hif), lof.max(hif));
    if x < lof {
        lo.clone()
    } else if x > hif {
        hi.clone()
    } else {
        c.clone()
    }
}

fn dist_point_segment(p: &[Coord], a: &[Coord], b: &[Coord]) -> f64 {
    let d: Vec<f64> = (0..p.len())
        .map(|i| b[i].to_f64() - a[i].to_f64())
        .collect();
    let dd: f64 = d.iter().map(|x| x * x).sum();
    let t = if dd == 0.0 {
        0.0
    } else {
        ((0..p.len())
            .map(|i| (p[i].to_f64() - a[i].to_f64()) * d[i])
            .sum::<f64>()
            / dd)
            .clamp(0.0, 1.0)
    };
    (0..p.len())
        .map(|i| {
            let diff = p[i].to_f64() - (a[i].to_f64() + t * d[i]);
            diff * diff
        })
        .sum::<f64>()
        .sqrt()
}

fn zero_coord() -> Coord {
    Coord::Exact(Rational::from(0))
}

fn abs_coord(c: Coord) -> Coord {
    match c {
        Coord::Exact(r) => Coord::Exact(r.abs()),
        Coord::Real(x) => Coord::Real(x.abs()),
    }
}

fn segment_nearest(p: &[Coord], a: &[Coord], b: &[Coord]) -> Vec<Coord> {
    let d: Vec<f64> = (0..p.len())
        .map(|i| b[i].to_f64() - a[i].to_f64())
        .collect();
    let dd: f64 = d.iter().map(|x| x * x).sum();
    let t = if dd == 0.0 {
        0.0
    } else {
        ((0..p.len())
            .map(|i| (p[i].to_f64() - a[i].to_f64()) * d[i])
            .sum::<f64>()
            / dd)
            .clamp(0.0, 1.0)
    };
    (0..p.len())
        .map(|i| Coord::Real(a[i].to_f64() + t * d[i]))
        .collect()
}

fn region_distance_value(region: &Value, point: &Value) -> Option<Value> {
    let Value::Call { head, args: a } = region else {
        return None;
    };
    match head.as_str() {
        "Point" => {
            let c = point_coords(a.first()?)?;
            let p = point_coords(point)?;
            Some(coord_value(&norm(&p, &c)))
        }
        "Disk" | "Circle" | "Ball" | "Sphere" => {
            let c = point_coords(a.first()?)?;
            let p = point_coords(point)?;
            if p.len() != c.len() {
                return None;
            }
            let r = radius_arg(a)?;
            let d = sub_coord(&norm(&p, &c), &r);
            Some(coord_value(&if d.to_f64() < 0.0 {
                zero_coord()
            } else {
                d
            }))
        }
        "Rectangle" | "Cuboid" => {
            let (c0, c1) = if head == "Rectangle" {
                rectangle_corners(a)?
            } else {
                cuboid_corners(a)?
            };
            let p = point_coords(point)?;
            if p.len() != c0.len() {
                return None;
            }
            let clamped: Vec<Coord> = (0..p.len())
                .map(|i| clamp_coord(&p[i], &c0[i], &c1[i]))
                .collect();
            Some(coord_value(&norm(&p, &clamped)))
        }
        "Interval" => {
            let iv = list_of(a.first()?)?;
            if iv.len() != 2 {
                return None;
            }
            let x = coord(point)?;
            let (lo, hi) = (coord(&iv[0])?, coord(&iv[1])?);
            let c = clamp_coord(&x, &lo, &hi);
            Some(coord_value(&abs_coord(sub_coord(&x, &c))))
        }
        "Line" => {
            let pts = list_of(a.first()?)?;
            let coords: Vec<Vec<Coord>> = pts.iter().map(point_coords).collect::<Option<_>>()?;
            let p = point_coords(point)?;
            let d = coords
                .windows(2)
                .map(|w| dist_point_segment(&p, &w[0], &w[1]))
                .fold(f64::INFINITY, f64::min);
            Some(super::real(d))
        }
        "InfiniteLine" => {
            let (q, v) = line_pv(a)?;
            let p = point_coords(point)?;
            if p.len() != q.len() {
                return None;
            }
            let diff: Vec<f64> = (0..p.len())
                .map(|i| p[i].to_f64() - q[i].to_f64())
                .collect();
            let vf: Vec<f64> = v.iter().map(|x| x.to_f64()).collect();
            let dd: f64 = vf.iter().map(|x| x * x).sum();
            if dd == 0.0 {
                return None;
            }
            let t: f64 = diff.iter().zip(&vf).map(|(a, b)| a * b).sum::<f64>() / dd;
            let resid: f64 = diff
                .iter()
                .zip(&vf)
                .map(|(a, b)| {
                    let r = a - t * b;
                    r * r
                })
                .sum::<f64>()
                .sqrt();
            Some(super::real(resid))
        }
        "HalfPlane" => {
            let (q, v) = line_pv(a)?;
            let p = point_coords(point)?;
            if p.len() < 2 {
                return None;
            }
            let det = v[0].to_f64() * (p[1].to_f64() - q[1].to_f64())
                - v[1].to_f64() * (p[0].to_f64() - q[0].to_f64());
            let vn = (v[0].to_f64().powi(2) + v[1].to_f64().powi(2)).sqrt();
            if vn == 0.0 {
                return None;
            }
            Some(super::real((-det).max(0.0) / vn))
        }
        "RegionUnion" => {
            let mut best: Option<Value> = None;
            let mut best_f = f64::INFINITY;
            for r in a {
                let v = region_distance_value(r, point)?;
                let f = as_f64(&v)?;
                if f < best_f {
                    best_f = f;
                    best = Some(v);
                }
            }
            best
        }
        "RegionIntersection" => {
            let mut worst = f64::NEG_INFINITY;
            for r in a {
                let f = as_f64(&region_distance_value(r, point)?)?;
                worst = worst.max(f);
            }
            Some(super::real(worst))
        }
        "RegionDifference" => {
            if a.len() != 2 {
                return None;
            }
            if region_member_geom_of(&a[1], point) == Some(true) {
                Some(coord_value(&zero_coord()))
            } else {
                region_distance_value(&a[0], point)
            }
        }
        _ => None,
    }
}

/// Membership without an environment (used by pure distance routines).
fn region_member_geom_of(region: &Value, point: &Value) -> Option<bool> {
    let Value::Call { head, args: a } = region else {
        return None;
    };
    match head.as_str() {
        "ImplicitRegion" | "RegionUnion" | "RegionIntersection" | "RegionDifference" => None,
        "Interval" => {
            let iv = list_of(a.first()?)?;
            let x = coord(point)?.to_f64();
            Some(coord_in(
                x,
                coord(&iv[0])?.to_f64(),
                coord(&iv[1])?.to_f64(),
            ))
        }
        _ => region_member_geom(head, a, &point_coords(point)?),
    }
}

/// RegionDistance[region, point] — Euclidean distance to `region`.
pub fn builtin_region_distance(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("RegionDistance", args));
    }
    match region_distance_value(&args[0], &args[1]) {
        Some(v) => Ok(v),
        None => Ok(unevaluated("RegionDistance", args)),
    }
}

fn region_nearest_value(region: &Value, point: &Value) -> Option<Value> {
    let Value::Call { head, args: a } = region else {
        return None;
    };
    // `Interval` points are scalars, so handle it before parsing a coordinate list.
    if head == "Interval" {
        let iv = list_of(a.first()?)?;
        if iv.len() != 2 {
            return None;
        }
        let x = coord(point)?;
        return Some(coord_value(&clamp_coord(
            &x,
            &coord(&iv[0])?,
            &coord(&iv[1])?,
        )));
    }
    let p = point_coords(point)?;
    match head.as_str() {
        "Point" => Some(a.first()?.clone()),
        "Disk" | "Circle" | "Ball" | "Sphere" => {
            let c = point_coords(a.first()?)?;
            if p.len() != c.len() {
                return None;
            }
            let r = radius_arg(a)?.to_f64();
            let d = norm(&p, &c).to_f64();
            if d <= r + EPS || d == 0.0 {
                return Some(Value::List(p.iter().map(coord_value).collect()));
            }
            let k = r / d;
            Some(Value::List(
                (0..p.len())
                    .map(|i| super::real(c[i].to_f64() + (p[i].to_f64() - c[i].to_f64()) * k))
                    .collect(),
            ))
        }
        "Rectangle" | "Cuboid" => {
            let (c0, c1) = if head == "Rectangle" {
                rectangle_corners(a)?
            } else {
                cuboid_corners(a)?
            };
            if p.len() != c0.len() {
                return None;
            }
            Some(Value::List(
                (0..p.len())
                    .map(|i| coord_value(&clamp_coord(&p[i], &c0[i], &c1[i])))
                    .collect(),
            ))
        }
        "Interval" => {
            let iv = list_of(a.first()?)?;
            let x = coord(point)?;
            Some(coord_value(&clamp_coord(
                &x,
                &coord(&iv[0])?,
                &coord(&iv[1])?,
            )))
        }
        "Line" => {
            let pts = list_of(a.first()?)?;
            let coords: Vec<Vec<Coord>> = pts.iter().map(point_coords).collect::<Option<_>>()?;
            let mut best = None;
            let mut best_d = f64::INFINITY;
            for w in coords.windows(2) {
                let d = dist_point_segment(&p, &w[0], &w[1]);
                if d < best_d {
                    best_d = d;
                    best = Some(Value::List(
                        segment_nearest(&p, &w[0], &w[1])
                            .iter()
                            .map(coord_value)
                            .collect(),
                    ));
                }
            }
            best
        }
        "InfiniteLine" => {
            let (q, v) = line_pv(a)?;
            project_onto_line(&p, &q, &v)
        }
        "HalfPlane" => {
            let (q, v) = line_pv(a)?;
            if p.len() < 2 {
                return None;
            }
            let det = v[0].to_f64() * (p[1].to_f64() - q[1].to_f64())
                - v[1].to_f64() * (p[0].to_f64() - q[0].to_f64());
            if det >= 0.0 {
                return Some(Value::List(p.iter().map(coord_value).collect()));
            }
            let vv = v[0].to_f64().powi(2) + v[1].to_f64().powi(2);
            let k = det / vv;
            Some(Value::List(vec![
                super::real(p[0].to_f64() - 2.0 * k * v[0].to_f64()),
                super::real(p[1].to_f64() - 2.0 * k * v[1].to_f64()),
            ]))
        }
        "RegionUnion" => {
            let mut best: Option<Value> = None;
            let mut best_f = f64::INFINITY;
            for r in a {
                let v = region_nearest_value(r, point)?;
                let f = as_f64(&region_distance_value(r, point)?)?;
                if f < best_f {
                    best_f = f;
                    best = Some(v);
                }
            }
            best
        }
        _ => None,
    }
}

fn project_onto_line(p: &[Coord], q: &[Coord], v: &[Coord]) -> Option<Value> {
    let vf: Vec<f64> = v.iter().map(|x| x.to_f64()).collect();
    let dd: f64 = vf.iter().map(|x| x * x).sum();
    if dd == 0.0 || p.len() != q.len() {
        return None;
    }
    let t: f64 = (0..p.len())
        .map(|i| (p[i].to_f64() - q[i].to_f64()) * vf[i])
        .sum::<f64>()
        / dd;
    Some(Value::List(
        (0..p.len())
            .map(|i| super::real(q[i].to_f64() + t * vf[i]))
            .collect(),
    ))
}

/// RegionNearest[region, point] — closest point of `region` to `point`.
pub fn builtin_region_nearest(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("RegionNearest", args));
    }
    match region_nearest_value(&args[0], &args[1]) {
        Some(v) => Ok(v),
        None => Ok(unevaluated("RegionNearest", args)),
    }
}

fn is_region(v: &Value) -> bool {
    matches!(v, Value::Call { head, .. } if matches!(head.as_str(),
        "Disk" | "Circle" | "Ball" | "Sphere" | "Rectangle" | "Cuboid" | "Triangle"
        | "Polygon" | "Tetrahedron" | "Cylinder" | "Cone" | "Point" | "Interval"
        | "Line" | "InfiniteLine" | "HalfPlane" | "ImplicitRegion" | "RegionUnion"
        | "RegionIntersection" | "RegionDifference" | "EmptyRegion"))
}

/// Nearest[region, point] — region-aware overload of `Nearest`.
pub fn builtin_nearest(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 2 && is_region(&args[0]) {
        return builtin_region_nearest(args);
    }
    super::list::builtin_nearest(args)
}

// ── Box boolean operations ──────────────────────────────────────────────────

fn box_corners(head: &str, a: &[Value]) -> Option<(Vec<Coord>, Vec<Coord>)> {
    match head {
        "Rectangle" => rectangle_corners(a),
        "Cuboid" => cuboid_corners(a),
        _ => None,
    }
}

fn empty_region(dim: usize) -> Value {
    Value::Call {
        head: "EmptyRegion".to_string(),
        args: vec![Value::Integer(Integer::from(dim as i64))],
    }
}

/// Intersection of two axis-aligned boxes, or `None` when inapplicable.
fn box_intersection(r1: &Value, r2: &Value) -> Option<Value> {
    let Value::Call { head: h1, args: a1 } = r1 else {
        return None;
    };
    let Value::Call { head: h2, args: a2 } = r2 else {
        return None;
    };
    if h1 != h2 {
        return None;
    }
    let (c0, c1) = box_corners(h1, a1)?;
    let (d0, d1) = box_corners(h2, a2)?;
    if c0.len() != d0.len() {
        return None;
    }
    let dim = c0.len();
    let mut lo = Vec::with_capacity(dim);
    let mut hi = Vec::with_capacity(dim);
    for i in 0..dim {
        let a = c0[i].to_f64().max(d0[i].to_f64());
        let b = c1[i].to_f64().min(d1[i].to_f64());
        if a > b + EPS {
            return Some(empty_region(dim));
        }
        lo.push(pick_coord(&c0[i], &d0[i], true));
        hi.push(pick_coord(&c1[i], &d1[i], false));
    }
    Some(Value::Call {
        head: h1.clone(),
        args: vec![
            Value::List(lo.iter().map(coord_value).collect()),
            Value::List(hi.iter().map(coord_value).collect()),
        ],
    })
}

fn pick_coord(a: &Coord, b: &Coord, max: bool) -> Coord {
    let take_a = if max {
        a.to_f64() >= b.to_f64()
    } else {
        a.to_f64() <= b.to_f64()
    };
    if take_a { a.clone() } else { b.clone() }
}

fn region_dim(v: &Value) -> Option<usize> {
    let Value::Call { head, args: a } = v else {
        return None;
    };
    match head.as_str() {
        "Disk" | "Circle" | "Rectangle" | "Triangle" => Some(2),
        "Ball" | "Sphere" | "Cuboid" | "Tetrahedron" | "Cylinder" | "Cone" => Some(3),
        "Polygon" => Some(list_of(a.first()?)?.first().and_then(point_coords)?.len()),
        "Point" => Some(point_coords(a.first()?).map(|p| p.len())?),
        "Line" => Some(point_coords(list_of(a.first()?)?.first()?).map(|p| p.len())?),
        "Interval" => Some(1),
        _ => None,
    }
}

// ── Geometric transformation ────────────────────────────────────────────────

fn to_matrix(rows: &[Value]) -> Option<Vec<Vec<Coord>>> {
    rows.iter()
        .map(|r| {
            list_of(r)?
                .iter()
                .map(coord)
                .collect::<Option<Vec<Coord>>>()
        })
        .collect()
}

/// Parse an affine transform into `(matrix, offset)` for a `dim`-dimensional region.
fn parse_transform(t: &Value, dim: usize) -> Option<(Vec<Vec<Coord>>, Vec<Coord>)> {
    let items = list_of(t)?;
    // Affine form `{matrix, offset}`: the first element is a matrix (list of
    // rows) and the second a flat offset vector.
    if items.len() == 2
        && let (Value::List(mrows), Value::List(off)) = (&items[0], &items[1])
        && !mrows.is_empty()
        && mrows.iter().all(|r| matches!(r, Value::List(_)))
        && off.iter().all(|x| !matches!(x, Value::List(_)))
    {
        let m = to_matrix(mrows)?;
        if m.len() == dim && m.iter().all(|r| r.len() == dim) {
            let off: Vec<Coord> = off.iter().map(coord).collect::<Option<_>>()?;
            if off.len() == dim {
                return Some((m, off));
            }
        }
    }
    if items.iter().all(|x| matches!(x, Value::List(_))) {
        let rows = to_matrix(items)?;
        let n = rows.len();
        if n == dim && rows.iter().all(|r| r.len() == dim) {
            return Some((rows, vec![Coord::Exact(Rational::from(0)); dim]));
        }
        if n == dim + 1 && rows.iter().all(|r| r.len() == dim + 1) {
            if rows[dim][dim].to_f64() != 1.0 {
                return None;
            }
            let m: Vec<Vec<Coord>> = rows[..dim].iter().map(|r| r[..dim].to_vec()).collect();
            let v: Vec<Coord> = (0..dim).map(|i| rows[i][dim].clone()).collect();
            return Some((m, v));
        }
        if n == dim + 1 && rows.iter().all(|r| r.len() == dim) {
            let m: Vec<Vec<Coord>> = rows[..dim].to_vec();
            let v: Vec<Coord> = rows[dim].clone();
            return Some((m, v));
        }
    }
    None
}

fn apply_affine(m: &[Vec<Coord>], v: &[Coord], p: &[Coord]) -> Vec<Coord> {
    (0..m.len())
        .map(|i| {
            let mut acc = Coord::Exact(Rational::from(0));
            for (j, x) in p.iter().enumerate() {
                acc = add_coord(acc, mul_coord_ref(m[i][j].clone(), x.clone()));
            }
            add_coord(acc, v[i].clone())
        })
        .collect()
}

fn transform_points(pts: &[Value], m: &[Vec<Coord>], v: &[Coord]) -> Option<Vec<Value>> {
    pts.iter()
        .map(|pt| {
            let c = point_coords(pt)?;
            if c.len() != m.len() {
                return None;
            }
            Some(Value::List(
                apply_affine(m, v, &c).iter().map(coord_value).collect(),
            ))
        })
        .collect()
}

/// GeometricTransformation[region, transform] — apply an affine map to a region.
pub fn builtin_geometric_transformation(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("GeometricTransformation", args));
    }
    let Value::Call { head, args: a } = &args[0] else {
        return Ok(unevaluated("GeometricTransformation", args));
    };
    let Some(dim) = region_dim(&args[0]) else {
        return Ok(unevaluated("GeometricTransformation", args));
    };
    let Some((m, v)) = parse_transform(&args[1], dim) else {
        return Ok(unevaluated("GeometricTransformation", args));
    };
    let rebuild = |new_args: Vec<Value>| Value::Call {
        head: head.clone(),
        args: new_args,
    };
    let out = match head.as_str() {
        "Disk" | "Circle" | "Ball" | "Sphere" => {
            let Some(c) = a.first().and_then(point_coords) else {
                return Ok(unevaluated("GeometricTransformation", args));
            };
            if c.len() != dim {
                return Ok(unevaluated("GeometricTransformation", args));
            }
            let tc = Value::List(apply_affine(&m, &v, &c).iter().map(coord_value).collect());
            let mut new_args = vec![tc];
            if let Some(r) = a.get(1) {
                new_args.push(r.clone());
            }
            rebuild(new_args)
        }
        "Rectangle" | "Cuboid" => {
            let Some((c0, c1)) = box_corners(head, a) else {
                return Ok(unevaluated("GeometricTransformation", args));
            };
            let t0 = apply_affine(&m, &v, &c0);
            let t1 = apply_affine(&m, &v, &c1);
            let lo: Vec<Value> = (0..dim)
                .map(|i| coord_value(&pick_coord(&t0[i], &t1[i], false)))
                .collect();
            let hi: Vec<Value> = (0..dim)
                .map(|i| coord_value(&pick_coord(&t0[i], &t1[i], true)))
                .collect();
            rebuild(vec![Value::List(lo), Value::List(hi)])
        }
        "Interval" => {
            let Some(iv) = a.first().and_then(list_of) else {
                return Ok(unevaluated("GeometricTransformation", args));
            };
            if iv.len() != 2 {
                return Ok(unevaluated("GeometricTransformation", args));
            }
            let pts = vec![iv[0].clone(), iv[1].clone()];
            let Some(t) = transform_points(&pts, &m, &v) else {
                return Ok(unevaluated("GeometricTransformation", args));
            };
            rebuild(vec![Value::List(t)])
        }
        "Polygon" => {
            let Some(pts) = a.first().and_then(list_of) else {
                return Ok(unevaluated("GeometricTransformation", args));
            };
            let Some(t) = transform_points(pts, &m, &v) else {
                return Ok(unevaluated("GeometricTransformation", args));
            };
            rebuild(vec![Value::List(t)])
        }
        "Triangle" | "Tetrahedron" => {
            let Some(t) = transform_points(a, &m, &v) else {
                return Ok(unevaluated("GeometricTransformation", args));
            };
            rebuild(t)
        }
        "Point" => {
            let Some(t) = transform_points(a, &m, &v) else {
                return Ok(unevaluated("GeometricTransformation", args));
            };
            rebuild(t)
        }
        "Line" => {
            let Some(pts) = a.first().and_then(list_of) else {
                return Ok(unevaluated("GeometricTransformation", args));
            };
            let Some(t) = transform_points(pts, &m, &v) else {
                return Ok(unevaluated("GeometricTransformation", args));
            };
            rebuild(vec![Value::List(t)])
        }
        _ => return Ok(unevaluated("GeometricTransformation", args)),
    };
    Ok(out)
}

/// RegionUnion[...] / RegionIntersection[...] — flattened boolean regions.
pub fn builtin_region_union(args: &[Value]) -> Result<Value, EvalError> {
    region_bool("RegionUnion", args)
}

pub fn builtin_region_intersection(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 2
        && let Some(b) = box_intersection(&args[0], &args[1])
    {
        return Ok(b);
    }
    region_bool("RegionIntersection", args)
}

/// RegionDifference[r1, r2] — set difference, simplified for boxes.
pub fn builtin_region_difference(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("RegionDifference", args));
    }
    if args[0] == args[1] {
        return Ok(empty_region(region_dim(&args[0]).unwrap_or(2)));
    }
    if let Value::Call { head, .. } = &args[0]
        && matches!(head.as_str(), "Rectangle" | "Cuboid")
        && let Some(inter) = box_intersection(&args[0], &args[1])
        && matches!(&inter, Value::Call { head, .. } if head == "EmptyRegion")
    {
        // The subtrahend does not touch the minuend.
        return Ok(args[0].clone());
    }
    Ok(unevaluated("RegionDifference", args))
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

/// DiscretizeRegion[region, opts...] — `MeshRegion` approximation.
///
/// Supports a `MaxCellMeasure -> m` option, which sets the target edge length
/// for the circular primitives.
pub fn builtin_discretize_region(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(unevaluated("DiscretizeRegion", args));
    }
    let Value::Call { head, args: a } = &args[0] else {
        return Ok(unevaluated("DiscretizeRegion", args));
    };
    let mcm = max_cell_measure(&args[1..]);
    let ring = |r: f64| -> usize {
        match mcm {
            Some(m) if m > 0.0 && r > 0.0 => (2.0 * std::f64::consts::PI * r / m)
                .ceil()
                .clamp(8.0, 360.0) as usize,
            _ => 60,
        }
    };
    match head.as_str() {
        "Disk" | "Circle" => {
            let c = a.first().and_then(point).unwrap_or_else(|| vec![0.0, 0.0]);
            let r = a.get(1).and_then(as_f64).unwrap_or(1.0);
            let (cx, cy) = (c[0], c[1]);
            let n = ring(r);
            let mut pts: Vec<Value> = vec![pt2(cx, cy)];
            for i in 0..n {
                let t = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                pts.push(pt2(cx + r * t.cos(), cy + r * t.sin()));
            }
            let cells: Vec<Value> = (0..n)
                .map(|i| {
                    polygon(vec![
                        pts[0].clone(),
                        pts[i + 1].clone(),
                        pts[(i + 1) % n + 1].clone(),
                    ])
                })
                .collect();
            Ok(mesh_region_cells(Value::List(pts), cells))
        }
        "Ball" | "Sphere" => {
            let c = a
                .first()
                .and_then(point)
                .unwrap_or_else(|| vec![0.0, 0.0, 0.0]);
            let r = a.get(1).and_then(as_f64).unwrap_or(1.0);
            let n = ring(r).clamp(8, 48);
            let bands = 12usize;
            let mut pts: Vec<Value> = Vec::new();
            for i in 0..=bands {
                let theta =
                    -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / bands as f64;
                for j in 0..n {
                    let phi = 2.0 * std::f64::consts::PI * j as f64 / n as f64;
                    pts.push(Value::List(vec![
                        super::real(c[0] + r * theta.cos() * phi.cos()),
                        super::real(c[1] + r * theta.cos() * phi.sin()),
                        super::real(c[2] + r * theta.sin()),
                    ]));
                }
            }
            let idx = |i: usize, j: usize| i * n + (j % n);
            let mut cells = Vec::new();
            for i in 0..bands {
                for j in 0..n {
                    cells.push(polygon(vec![
                        pts[idx(i, j)].clone(),
                        pts[idx(i, j + 1)].clone(),
                        pts[idx(i + 1, j + 1)].clone(),
                        pts[idx(i + 1, j)].clone(),
                    ]));
                }
            }
            Ok(mesh_region_cells(Value::List(pts), cells))
        }
        "Cylinder" | "Cone" => {
            let Some((base, top, r)) = endpoints_radius(a) else {
                return Ok(unevaluated("DiscretizeRegion", args));
            };
            if base.len() != 3 || top.len() != 3 {
                return Ok(unevaluated("DiscretizeRegion", args));
            }
            let r = r.to_f64();
            let n = ring(r).clamp(8, 48);
            // Axis-perpendicular basis.
            let axis = [
                top[0].to_f64() - base[0].to_f64(),
                top[1].to_f64() - base[1].to_f64(),
                top[2].to_f64() - base[2].to_f64(),
            ];
            let an = (axis[0].powi(2) + axis[1].powi(2) + axis[2].powi(2)).sqrt();
            if an == 0.0 {
                return Ok(unevaluated("DiscretizeRegion", args));
            }
            let az = [axis[0] / an, axis[1] / an, axis[2] / an];
            let helper = if az[0].abs() < 0.9 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let mut u = [
                az[1] * helper[2] - az[2] * helper[1],
                az[2] * helper[0] - az[0] * helper[2],
                az[0] * helper[1] - az[1] * helper[0],
            ];
            let un = (u[0].powi(2) + u[1].powi(2) + u[2].powi(2)).sqrt();
            u = [u[0] / un, u[1] / un, u[2] / un];
            let v = [
                az[1] * u[2] - az[2] * u[1],
                az[2] * u[0] - az[0] * u[2],
                az[0] * u[1] - az[1] * u[0],
            ];
            let ring_at = |c: &[Coord], rr: f64| -> Vec<Value> {
                (0..n)
                    .map(|j| {
                        let t = 2.0 * std::f64::consts::PI * j as f64 / n as f64;
                        let (ct, st) = (t.cos(), t.sin());
                        Value::List(vec![
                            super::real(c[0].to_f64() + rr * (ct * u[0] + st * v[0])),
                            super::real(c[1].to_f64() + rr * (ct * u[1] + st * v[1])),
                            super::real(c[2].to_f64() + rr * (ct * u[2] + st * v[2])),
                        ])
                    })
                    .collect()
            };
            let mut pts = ring_at(&base, r);
            // Cone apex collapses the top ring.
            let top_r = if head == "Cone" { 0.0 } else { r };
            let top_pts = ring_at(&top, top_r);
            pts.extend(top_pts.iter().cloned());
            let idx = |i: usize, j: usize| i * n + (j % n);
            let mut cells = Vec::new();
            for j in 0..n {
                cells.push(polygon(vec![
                    pts[idx(0, j)].clone(),
                    pts[idx(0, j + 1)].clone(),
                    pts[idx(1, j + 1)].clone(),
                    pts[idx(1, j)].clone(),
                ]));
            }
            Ok(mesh_region_cells(Value::List(pts), cells))
        }
        "Rectangle" => {
            let Some((c0, c1)) = rectangle_corners(a) else {
                return Ok(unevaluated("DiscretizeRegion", args));
            };
            let pts = vec![
                pt2(c0[0].to_f64(), c0[1].to_f64()),
                pt2(c1[0].to_f64(), c0[1].to_f64()),
                pt2(c1[0].to_f64(), c1[1].to_f64()),
                pt2(c0[0].to_f64(), c1[1].to_f64()),
            ];
            Ok(mesh_region(Value::List(pts)))
        }
        "Cuboid" => {
            let Some((c0, c1)) = cuboid_corners(a) else {
                return Ok(unevaluated("DiscretizeRegion", args));
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
        "Triangle" => {
            let Some(pts) = point_list(a) else {
                return Ok(unevaluated("DiscretizeRegion", args));
            };
            Ok(mesh_region(Value::List(
                pts.iter()
                    .map(|p| Value::List(p.iter().map(coord_value).collect()))
                    .collect(),
            )))
        }
        "Polygon" => {
            let Some(pts) = list_of(&a[0]) else {
                return Ok(unevaluated("DiscretizeRegion", args));
            };
            Ok(mesh_region(Value::List(pts.to_vec())))
        }
        "Line" | "Point" => {
            let Some(pts) = list_of(&a[0]) else {
                return Ok(unevaluated("DiscretizeRegion", args));
            };
            Ok(mesh_region(Value::List(pts.to_vec())))
        }
        _ => Ok(unevaluated("DiscretizeRegion", args)),
    }
}

/// Extract `MaxCellMeasure -> m` from a trailing option list.
fn max_cell_measure(opts: &[Value]) -> Option<f64> {
    fn unwrap_side(v: &Value) -> Value {
        match v {
            Value::Pattern(e) => crate::pattern::unwrap_expr_to_value(e),
            other => other.clone(),
        }
    }
    for o in opts {
        let (lhs, rhs): (Value, Value) = match o {
            Value::Rule { lhs, rhs, .. } => (unwrap_side(lhs), unwrap_side(rhs)),
            Value::Pattern(crate::ast::Expr::Rule { lhs, rhs })
            | Value::Pattern(crate::ast::Expr::RuleDelayed { lhs, rhs }) => (
                crate::pattern::unwrap_expr_to_value(lhs),
                crate::pattern::unwrap_expr_to_value(rhs),
            ),
            _ => continue,
        };
        let name = match &lhs {
            Value::Symbol(s) => s.clone(),
            Value::Str(s) => s.clone(),
            _ => continue,
        };
        if name == "MaxCellMeasure" {
            return as_f64(&rhs);
        }
    }
    None
}

fn pt2(x: f64, y: f64) -> Value {
    Value::List(vec![super::real(x), super::real(y)])
}

fn polygon(pts: Vec<Value>) -> Value {
    Value::Call {
        head: "Polygon".to_string(),
        args: vec![Value::List(pts)],
    }
}

fn mesh_region(pts: Value) -> Value {
    let cell = polygon(pts.as_ref_list());
    Value::Call {
        head: "MeshRegion".to_string(),
        args: vec![pts, Value::List(vec![cell])],
    }
}

fn mesh_region_cells(pts: Value, cells: Vec<Value>) -> Value {
    Value::Call {
        head: "MeshRegion".to_string(),
        args: vec![pts, Value::List(cells)],
    }
}

impl Value {
    fn as_ref_list(&self) -> Vec<Value> {
        match self {
            Value::List(items) => items.clone(),
            other => vec![other.clone()],
        }
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
    super::register_builtin(env, "RegionDifference", builtin_region_difference);
    super::register_builtin(env, "RegionDistance", builtin_region_distance);
    super::register_builtin(env, "RegionNearest", builtin_region_nearest);
    super::register_builtin_env(env, "RegionMember", builtin_region_member);
    super::register_builtin(env, "ImplicitRegion", builtin_implicit_region);
    super::register_builtin(
        env,
        "GeometricTransformation",
        builtin_geometric_transformation,
    );
    // `Nearest` is normally owned by `list`; this region-aware overload falls
    // back to the numeric implementation for non-region arguments.
    super::register_builtin(env, "Nearest", builtin_nearest);
    super::register_builtin(env, "DiscretizeRegion", builtin_discretize_region);
}

/// ImplicitRegion[f, vars] — symbolic wrapper kept unevaluated.
pub fn builtin_implicit_region(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("ImplicitRegion", args));
    }
    Ok(unevaluated("ImplicitRegion", args))
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

    // ── helpers ─────────────────────────────────────────────────────────────

    fn eval_str(input: &str) -> Value {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let tokens = crate::lexer::tokenize(input).unwrap_or_else(|e| panic!("lex {input}: {e:?}"));
        let ast = crate::parser::parse(tokens).unwrap_or_else(|e| panic!("parse {input}: {e:?}"));
        crate::eval::eval_program(&ast, &env).unwrap_or_else(|e| panic!("eval {input}: {e:?}"))
    }

    fn close_num(v: &Value, expected: f64) {
        let got = as_f64(v).unwrap_or_else(|| panic!("not numeric: {v}"));
        assert!(
            (got - expected).abs() < 1e-9,
            "expected {expected}, got {got}"
        );
    }

    fn pi_str(input: &str) -> String {
        let s = eval_str(input).to_string();
        assert!(s.contains("Pi"), "`{input}` -> `{s}` lacks Pi");
        s
    }

    fn t() -> Value {
        Value::Bool(true)
    }
    fn f() -> Value {
        Value::Bool(false)
    }

    // ── measures ────────────────────────────────────────────────────────────

    #[test]
    fn test_area_exact_primitives() {
        assert_eq!(eval_str("Area[Rectangle[{0,0},{3,4}]]"), i(12));
        assert_eq!(eval_str("Area[Rectangle[{1,1},{4,5}]]"), i(12));
        assert_eq!(eval_str("Area[Triangle[{{0,0},{4,0},{0,3}}]]"), i(6));
        assert_eq!(eval_str("Area[Polygon[{{0,0},{2,0},{2,2},{0,2}}]]"), i(4));
        assert_eq!(
            eval_str("Area[Disk[{0,0},3]]"),
            pi_expr(Rational::from(9), 1)
        );
        assert_eq!(eval_str("Area[Disk[]]"), pi_expr(Rational::from(1), 1));
        assert_eq!(
            eval_str("Area[Circle[{0,0},2]]"),
            pi_expr(Rational::from(4), 1)
        );
        assert_eq!(
            eval_str("Area[Disk[{1,1},1/2]]"),
            pi_expr(Rational::from((1, 4)), 1)
        );
    }

    #[test]
    fn test_volume_exact_primitives() {
        assert_eq!(eval_str("Volume[Cuboid[{0,0,0},{2,3,4}]]"), i(24));
        assert_eq!(eval_str("Volume[Cuboid[{2,3,4}]]"), i(24));
        assert_eq!(
            eval_str("Volume[Ball[{0,0,0},2]]"),
            pi_expr(Rational::from((32, 3)), 1)
        );
        assert_eq!(
            eval_str("Volume[Sphere[{0,0,0},1]]"),
            pi_expr(Rational::from((4, 3)), 1)
        );
        assert_eq!(
            eval_str("Volume[Cylinder[{{0,0,0},{0,0,3}},2]]"),
            pi_expr(Rational::from(12), 1)
        );
        assert_eq!(
            eval_str("Volume[Cone[{{0,0,0},{0,0,3}},2]]"),
            pi_expr(Rational::from(4), 1)
        );
        close_num(
            &eval_str("Volume[Tetrahedron[{{0,0,0},{1,0,0},{0,1,0},{0,0,1}}]]"),
            1.0 / 6.0,
        );
    }

    #[test]
    fn test_region_measure_dispatch() {
        assert_eq!(eval_str("RegionMeasure[Rectangle[{0,0},{3,4}]]"), i(12));
        assert_eq!(
            eval_str("RegionMeasure[Triangle[{{0,0},{4,0},{0,3}}]]"),
            i(6)
        );
        assert_eq!(eval_str("RegionMeasure[Cuboid[{0,0,0},{2,3,4}]]"), i(24));
        assert_eq!(
            eval_str("RegionMeasure[Circle[{0,0},5]]"),
            pi_expr(Rational::from(25), 1)
        );
        assert!(pi_str("RegionMeasure[Sphere[{0,0,0},5]]").contains("Pi"));
    }

    #[test]
    fn test_region_measure_boolean() {
        // Two disjoint unit boxes union to area 2.
        assert_eq!(
            eval_str("RegionMeasure[RegionUnion[Rectangle[{0,0},{1,1}], Rectangle[{2,0},{3,1}]]]"),
            i(2)
        );
        assert_eq!(
            eval_str(
                "RegionMeasure[RegionIntersection[Rectangle[{0,0},{2,2}], Rectangle[{1,1},{3,3}]]]"
            ),
            i(1)
        );
    }

    // ── membership ──────────────────────────────────────────────────────────

    #[test]
    fn test_member_round_primitives() {
        assert_eq!(eval_str("RegionMember[Disk[{0,0},2], {0,0}]"), t());
        assert_eq!(eval_str("RegionMember[Disk[{0,0},2], {2,0}]"), t());
        assert_eq!(eval_str("RegionMember[Disk[{0,0},2], {3,0}]"), f());
        assert_eq!(eval_str("RegionMember[Circle[{0,0},2], {1,0}]"), t());
        assert_eq!(eval_str("RegionMember[Ball[{0,0,0},1], {0,0,0}]"), t());
        assert_eq!(eval_str("RegionMember[Ball[{0,0,0},1], {0,0,2}]"), f());
        assert_eq!(eval_str("RegionMember[Sphere[{0,0,0},1], {1,0,0}]"), t());
    }

    #[test]
    fn test_member_boxes_and_simplices() {
        assert_eq!(eval_str("RegionMember[Rectangle[{0,0},{2,3}], {1,2}]"), t());
        assert_eq!(eval_str("RegionMember[Rectangle[{0,0},{2,3}], {3,2}]"), f());
        assert_eq!(
            eval_str("RegionMember[Cuboid[{0,0,0},{2,3,4}], {1,2,3}]"),
            t()
        );
        assert_eq!(
            eval_str("RegionMember[Cuboid[{0,0,0},{2,3,4}], {1,2,5}]"),
            f()
        );
        assert_eq!(
            eval_str("RegionMember[Triangle[{{0,0},{4,0},{0,4}}], {1,1}]"),
            t()
        );
        assert_eq!(
            eval_str("RegionMember[Triangle[{{0,0},{4,0},{0,4}}], {3,3}]"),
            f()
        );
        assert_eq!(
            eval_str("RegionMember[Polygon[{{0,0},{2,0},{2,2},{0,2}}], {1,1}]"),
            t()
        );
        assert_eq!(
            eval_str("RegionMember[Polygon[{{0,0},{2,0},{2,2},{0,2}}], {3,1}]"),
            f()
        );
        assert_eq!(
            eval_str("RegionMember[Tetrahedron[{{0,0,0},{1,0,0},{0,1,0},{0,0,1}}], {1/4,1/4,1/4}]"),
            t()
        );
        assert_eq!(
            eval_str("RegionMember[Tetrahedron[{{0,0,0},{1,0,0},{0,1,0},{0,0,1}}], {1,1,1}]"),
            f()
        );
    }

    #[test]
    fn test_member_solids() {
        assert_eq!(
            eval_str("RegionMember[Cylinder[{{0,0,0},{0,0,4}},1], {1,0,2}]"),
            t()
        );
        assert_eq!(
            eval_str("RegionMember[Cylinder[{{0,0,0},{0,0,4}},1], {2,0,2}]"),
            f()
        );
        assert_eq!(
            eval_str("RegionMember[Cylinder[{{0,0,0},{0,0,4}},1], {0,0,5}]"),
            f()
        );
        assert_eq!(
            eval_str("RegionMember[Cone[{{0,0,0},{0,0,4}},2], {0,0,0}]"),
            t()
        );
        assert_eq!(
            eval_str("RegionMember[Cone[{{0,0,0},{0,0,4}},2], {2,0,0}]"),
            t()
        );
        assert_eq!(
            eval_str("RegionMember[Cone[{{0,0,0},{0,0,4}},2], {0,0,4}]"),
            t()
        );
    }

    #[test]
    fn test_member_lower_dimensional() {
        assert_eq!(eval_str("RegionMember[Point[{1,2}], {1,2}]"), t());
        assert_eq!(eval_str("RegionMember[Point[{1,2}], {1,3}]"), f());
        assert_eq!(eval_str("RegionMember[Interval[{1,5}], 3]"), t());
        assert_eq!(eval_str("RegionMember[Interval[{1,5}], 6]"), f());
        assert_eq!(eval_str("RegionMember[Line[{{0,0},{2,0}}], {1,0}]"), t());
        assert_eq!(eval_str("RegionMember[Line[{{0,0},{2,0}}], {1,1}]"), f());
        assert_eq!(
            eval_str("RegionMember[InfiniteLine[{{0,0},{1,1}}], {2,2}]"),
            t()
        );
        assert_eq!(
            eval_str("RegionMember[InfiniteLine[{{0,0},{1,1}}], {2,3}]"),
            f()
        );
        assert_eq!(
            eval_str("RegionMember[HalfPlane[{{0,0},{0,1}}], {-1,0}]"),
            t()
        );
        assert_eq!(
            eval_str("RegionMember[HalfPlane[{{0,0},{0,1}}], {1,0}]"),
            f()
        );
    }

    #[test]
    fn test_member_implicit_region() {
        assert_eq!(
            eval_str("RegionMember[ImplicitRegion[x^2 + y^2 <= 4, {x, y}], {0, 0}]"),
            t()
        );
        assert_eq!(
            eval_str("RegionMember[ImplicitRegion[x^2 + y^2 <= 4, {x, y}], {3, 0}]"),
            f()
        );
        assert_eq!(
            eval_str("RegionMember[ImplicitRegion[x >= 0 && y >= 0, {x, y}], {1, 1}]"),
            t()
        );
    }

    #[test]
    fn test_member_booleans() {
        assert_eq!(
            eval_str("RegionMember[RegionUnion[Disk[{0,0},1], Disk[{3,0},1]], {3,0}]"),
            t()
        );
        assert_eq!(
            eval_str("RegionMember[RegionUnion[Disk[{0,0},1], Disk[{3,0},1]], {3/2,0}]"),
            f()
        );
        assert_eq!(
            eval_str("RegionMember[RegionIntersection[Disk[{0,0},2], Disk[{1,0},2]], {1,0}]"),
            t()
        );
        assert_eq!(
            eval_str("RegionMember[RegionIntersection[Disk[{0,0},2], Disk[{1,0},2]], {3,0}]"),
            f()
        );
        assert_eq!(
            eval_str(
                "RegionMember[RegionDifference[Rectangle[{0,0},{2,2}], Rectangle[{1,1},{2,2}]], {0,0}]"
            ),
            t()
        );
        assert_eq!(
            eval_str(
                "RegionMember[RegionDifference[Rectangle[{0,0},{2,2}], Rectangle[{1,1},{2,2}]], {3/2,3/2}]"
            ),
            f()
        );
    }

    // ── centroid / bounds ───────────────────────────────────────────────────

    #[test]
    fn test_centroid_common() {
        assert_eq!(
            eval_str("RegionCentroid[Rectangle[{0,0},{2,4}]]").to_string(),
            "{1, 2}"
        );
        assert_eq!(
            eval_str("RegionCentroid[Cuboid[{0,0,0},{2,4,6}]]").to_string(),
            "{1, 2, 3}"
        );
        assert_eq!(
            eval_str("RegionCentroid[Triangle[{{0,0},{3,0},{0,3}}]]").to_string(),
            "{1, 1}"
        );
        assert_eq!(
            eval_str("RegionCentroid[Ball[{1,2,3},5]]").to_string(),
            "{1, 2, 3}"
        );
        assert_eq!(
            eval_str("RegionCentroid[Cylinder[{{0,0,0},{0,0,4}},1]]").to_string(),
            "{0, 0, 2}"
        );
    }

    #[test]
    fn test_bounds_common() {
        assert_eq!(
            eval_str("RegionBounds[Ball[{1,2,3},2]]").to_string(),
            "{{-1, 0, 1}, {3, 4, 5}}"
        );
        assert_eq!(
            eval_str("RegionBounds[Cuboid[{0,0,0},{2,3,4}]]").to_string(),
            "{{0, 0, 0}, {2, 3, 4}}"
        );
        assert_eq!(
            eval_str("RegionBounds[Polygon[{{0,0},{2,0},{2,2}}]]").to_string(),
            "{{0, 0}, {2, 2}}"
        );
    }

    // ── distance / nearest ──────────────────────────────────────────────────

    #[test]
    fn test_region_distance_exact() {
        assert_eq!(eval_str("RegionDistance[Point[{0,0}], {3,4}]"), i(5));
        assert_eq!(
            eval_str("RegionDistance[Rectangle[{0,0},{1,1}], {2,1}]"),
            i(1)
        );
        assert_eq!(
            eval_str("RegionDistance[Rectangle[{0,0},{1,1}], {0,0}]"),
            i(0)
        );
        assert_eq!(eval_str("RegionDistance[Interval[{1,5}], 6]"), i(1));
        assert_eq!(eval_str("RegionDistance[Interval[{1,5}], 3]"), i(0));
        assert_eq!(eval_str("RegionDistance[Disk[{0,0},2], {5,0}]"), i(3));
        assert_eq!(eval_str("RegionDistance[Disk[{0,0},2], {1,0}]"), i(0));
    }

    #[test]
    fn test_region_distance_numeric() {
        close_num(
            &eval_str("RegionDistance[HalfPlane[{{0,0},{0,1}}], {3,0}]"),
            3.0,
        );
        close_num(&eval_str("RegionDistance[Line[{{0,0},{2,0}}], {1,3}]"), 3.0);
        close_num(
            &eval_str("RegionDistance[InfiniteLine[{{0,0},{1,1}}], {1,0}]"),
            1.0 / 2f64.sqrt(),
        );
    }

    #[test]
    fn test_region_nearest() {
        assert_eq!(
            eval_str("RegionNearest[Rectangle[{0,0},{1,1}], {2,2}]").to_string(),
            "{1, 1}"
        );
        assert_eq!(eval_str("RegionNearest[Interval[{1,5}], 0]"), i(1));
        assert_eq!(
            eval_str("RegionNearest[Point[{3,4}], {0,0}]").to_string(),
            "{3, 4}"
        );
        assert_eq!(
            eval_str("RegionNearest[Disk[{0,0},2], {5,0}]").to_string(),
            "{2.0, 0}"
        );
        assert_eq!(
            eval_str("RegionNearest[Line[{{0,0},{2,0}}], {1,3}]").to_string(),
            "{1.0, 0}"
        );
        // Nearest is region-aware but still delegates for numeric lists.
        assert_eq!(
            eval_str("Nearest[Disk[{0,0},2], {5,0}]").to_string(),
            "{2.0, 0}"
        );
        assert_eq!(eval_str("Nearest[{1,2,3}, 2.2]"), i(2));
    }

    // ── boolean regions ─────────────────────────────────────────────────────

    #[test]
    fn test_region_intersection_and_difference() {
        assert_eq!(
            eval_str("RegionIntersection[Rectangle[{0,0},{2,2}], Rectangle[{1,1},{3,3}]]")
                .to_string(),
            "Rectangle[{1, 1}, {2, 2}]"
        );
        assert_eq!(
            eval_str("RegionIntersection[Rectangle[{0,0},{1,1}], Rectangle[{2,2},{3,3}]]")
                .to_string(),
            "EmptyRegion[2]"
        );
        assert_eq!(
            eval_str("RegionDifference[Rectangle[{0,0},{2,2}], Rectangle[{0,0},{2,2}]]")
                .to_string(),
            "EmptyRegion[2]"
        );
        assert_eq!(
            eval_str("RegionDifference[Rectangle[{0,0},{1,1}], Rectangle[{3,3},{4,4}]]")
                .to_string(),
            "Rectangle[{0, 0}, {1, 1}]"
        );
        assert!(matches!(
            eval_str("RegionDifference[Rectangle[{0,0},{2,2}], Rectangle[{1,1},{3,3}]]"),
            Value::Call { head, .. } if head == "RegionDifference"
        ));
        // RegionUnion flattens nested unions.
        let u = eval_str("RegionUnion[Disk[{0,0},1], RegionUnion[Disk[{2,0},1], Disk[{4,0},1]]]");
        assert!(
            matches!(&u, Value::Call { head, args } if head == "RegionUnion" && args.len() == 3)
        );
    }

    // ── transformations ─────────────────────────────────────────────────────

    #[test]
    fn test_geometric_transformation() {
        assert_eq!(
            eval_str(
                "GeometricTransformation[Polygon[{{0,0},{1,0},{1,1}}], TranslationTransform[{2,3}]]"
            )
            .to_string(),
            "Polygon[{{2, 3}, {3, 3}, {3, 4}}]"
        );
        assert_eq!(
            eval_str(
                "GeometricTransformation[Rectangle[{0,0},{1,1}], TranslationTransform[{1,2}]]"
            )
            .to_string(),
            "Rectangle[{1, 2}, {2, 3}]"
        );
        assert_eq!(
            eval_str("GeometricTransformation[Disk[{0,0},2], TranslationTransform[{1,1}]]")
                .to_string(),
            "Disk[{1, 1}, 2]"
        );
        assert_eq!(
            eval_str("GeometricTransformation[Point[{1,1}], TranslationTransform[{1,0}]]")
                .to_string(),
            "Point[{2, 1}]"
        );
        assert_eq!(
            eval_str(
                "GeometricTransformation[Cuboid[{0,0,0},{1,1,1}], TranslationTransform[{1,1,1}]]"
            )
            .to_string(),
            "Cuboid[{1, 1, 1}, {2, 2, 2}]"
        );
        // A raw matrix also works.
        assert_eq!(
            eval_str("GeometricTransformation[Point[{1,1}], {{2,0},{0,2}}]").to_string(),
            "Point[{2, 2}]"
        );
        // Affine `{matrix, offset}` form.
        assert_eq!(
            eval_str("GeometricTransformation[Point[{1,1}], {{{1,0},{0,1}}, {4,5}}]").to_string(),
            "Point[{5, 6}]"
        );
        // Inapplicable region -> unevaluated.
        assert!(matches!(
            eval_str("GeometricTransformation[Foo[], {{1,0},{0,1}}]"),
            Value::Call { head, .. } if head == "GeometricTransformation"
        ));
    }

    // ── discretization ──────────────────────────────────────────────────────

    #[test]
    fn test_discretize_region_improved() {
        let out = eval_str("DiscretizeRegion[Disk[{0,0},1]]");
        let Value::Call { head, args } = &out else {
            panic!("expected MeshRegion, got {out}");
        };
        assert_eq!(head, "MeshRegion");
        let verts = list_of(&args[0]).map(<[Value]>::len).unwrap_or(0);
        let cells = list_of(&args[1]).map(<[Value]>::len).unwrap_or(0);
        assert!(verts > 10 && cells > 10, "verts={verts} cells={cells}");

        // MaxCellMeasure controls the resolution.
        let coarse = eval_str("DiscretizeRegion[Disk[{0,0},1], MaxCellMeasure -> 0.5]");
        let Value::Call { args: cargs, .. } = &coarse else {
            panic!("expected MeshRegion");
        };
        assert!(list_of(&cargs[0]).unwrap().len() < verts);

        for e in [
            "DiscretizeRegion[Triangle[{{0,0},{2,0},{0,2}}]]",
            "DiscretizeRegion[Cylinder[{{0,0,0},{0,0,1}},1]]",
            "DiscretizeRegion[Cone[{{0,0,0},{0,0,1}},1]]",
            "DiscretizeRegion[Sphere[{0,0,0},1]]",
            "DiscretizeRegion[Line[{{0,0},{1,1},{2,0}}]]",
        ] {
            assert!(
                matches!(eval_str(e), Value::Call { head, .. } if head == "MeshRegion"),
                "{e}"
            );
        }
    }

    // ── unevaluated fallbacks ───────────────────────────────────────────────

    #[test]
    fn test_unevaluated_when_inapplicable() {
        for e in [
            "Area[Foo[]]",
            "Volume[Foo[]]",
            "RegionMeasure[Foo[]]",
            "RegionCentroid[Foo[]]",
            "RegionBounds[Foo[]]",
            "RegionDistance[Foo[], {0,0}]",
            "RegionNearest[Foo[], {0,0}]",
            "RegionMember[Foo[], {0,0}]",
            "DiscretizeRegion[Foo[]]",
        ] {
            let out = eval_str(e);
            assert!(
                matches!(&out, Value::Call { .. }),
                "{e} -> {out} should stay unevaluated"
            );
        }
    }
}
