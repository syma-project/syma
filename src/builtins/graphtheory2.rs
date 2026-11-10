//! graph plotting and embeddings (Graph3D, LayeredGraphPlot, TreePlot,
//! GraphEmbedding, SpringEmbedding, SpringElectricalEmbedding, BarycentricEmbedding,
//! BarabasiAlbertGraphDistribution).
//!
//! Layouts are deterministic (no RNG) and best-effort. Plotting reuses the SVG
//! renderer in [`super::graphics`]; graph values use the same `Graph[...]` form
//! documented in `graphtheory.rs`. Inapplicable input stays unevaluated.

use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::Integer;
use std::collections::{HashMap, VecDeque};
use std::f64::consts::PI;

/// Build the unevaluated `head[args]` form used for inapplicable input.
fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

/// Unwrap the `Value::Pattern` operands produced for rule/edge arguments.
fn to_vertex(v: &Value) -> Value {
    match v {
        Value::Pattern(expr) => crate::pattern::unwrap_expr_to_value(expr),
        other => other.clone(),
    }
}

fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

fn coord(x: f64, y: f64) -> Value {
    Value::List(vec![super::real(x), super::real(y)])
}

// ── Graph model (mirrors the accepted forms in `graphtheory.rs`) ─────────────

/// A compact index-based graph with directedness flags on edges.
#[derive(Default)]
struct LayoutGraph {
    vertices: Vec<Value>,
    /// `(from, to, directed)`
    edges: Vec<(usize, usize, bool)>,
}

impl LayoutGraph {
    fn intern(&mut self, v: Value) -> usize {
        if let Some(i) = self.vertices.iter().position(|x| *x == v) {
            i
        } else {
            self.vertices.push(v);
            self.vertices.len() - 1
        }
    }

    /// Undirected adjacency lists.
    fn adjacency(&self) -> Vec<Vec<usize>> {
        let mut adj = vec![Vec::new(); self.vertices.len()];
        for &(a, b, _) in &self.edges {
            adj[a].push(b);
            adj[b].push(a);
        }
        adj
    }

    /// Edges with direction ignored, deduplicated per endpoint pair.
    fn undirected_edges(&self) -> Vec<(usize, usize)> {
        self.edges
            .iter()
            .filter(|(a, b, _)| a != b)
            .map(|&(a, b, _)| (a, b))
            .collect()
    }
}

/// Split an edge expression into `(pairs, directed)`.
fn edge_parts(e: &Value) -> Option<(Vec<(Value, Value)>, bool)> {
    match e {
        Value::Rule { lhs, rhs, .. } => Some((vec![(to_vertex(lhs), to_vertex(rhs))], true)),
        Value::Call { head, args } if head == "UndirectedEdge" && args.len() >= 2 => Some((
            args.windows(2)
                .map(|w| (to_vertex(&w[0]), to_vertex(&w[1])))
                .collect(),
            false,
        )),
        Value::Call { head, args } if head == "DirectedEdge" && args.len() >= 2 => Some((
            args.windows(2)
                .map(|w| (to_vertex(&w[0]), to_vertex(&w[1])))
                .collect(),
            true,
        )),
        _ => None,
    }
}

fn is_edge(v: &Value) -> bool {
    edge_parts(v).is_some()
}

fn add_edge(g: &mut LayoutGraph, e: &Value) -> Option<()> {
    let (pairs, directed) = edge_parts(e)?;
    for (a, b) in pairs {
        let ia = g.intern(a);
        let ib = g.intern(b);
        g.edges.push((ia, ib, directed));
    }
    Some(())
}

/// Interpret a value as a graph (a `Graph[...]` expression or a bare edge list).
fn parse_graph(v: &Value) -> Option<LayoutGraph> {
    if let Value::Call { head, args } = v
        && head == "Graph"
    {
        return parse_graph_args(args);
    }
    if let Value::List(items) = v
        && !items.is_empty()
        && items.iter().all(is_edge)
    {
        let mut g = LayoutGraph::default();
        for e in items {
            add_edge(&mut g, e)?;
        }
        return Some(g);
    }
    None
}

/// Build a graph from the argument list of a `Graph[...]` expression.
fn parse_graph_args(args: &[Value]) -> Option<LayoutGraph> {
    let lists: Vec<&Vec<Value>> = args
        .iter()
        .filter_map(|a| match a {
            Value::List(l) => Some(l),
            _ => None,
        })
        .collect();

    let (vlist, elist): (Option<&Vec<Value>>, Option<&Vec<Value>>) = match lists.len() {
        0 => (None, None),
        1 => {
            let l = lists[0];
            if !l.is_empty() && l.iter().all(is_edge) {
                (None, Some(l))
            } else {
                (Some(l), None)
            }
        }
        _ => (Some(lists[0]), Some(lists[1])),
    };

    let mut g = LayoutGraph::default();
    if let Some(vl) = vlist {
        for v in vl {
            g.intern(to_vertex(v));
        }
    }
    if let Some(el) = elist {
        for e in el {
            add_edge(&mut g, e)?;
        }
    }
    Some(g)
}

// ── Layout helpers (deterministic) ──────────────────────────────────────────

/// Initial positions on the unit circle (used to seed force layouts).
fn circle_positions(n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .map(|i| {
            let a = 2.0 * PI * (i as f64) / (n.max(1) as f64);
            (a.cos(), a.sin())
        })
        .collect()
}

/// Deterministic force-directed layout. `electrical` selects the
/// Fruchterman–Reingold (`SpringElectricalEmbedding`) force law; otherwise a
/// linear spring law (`SpringEmbedding`).
fn force_layout(n: usize, edges: &[(usize, usize)], electrical: bool) -> Vec<(f64, f64)> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![(0.0, 0.0)];
    }
    let mut pos = circle_positions(n);
    let k = (1.0 / n as f64).sqrt();
    let mut temp = 0.1f64;
    for _ in 0..200 {
        let mut disp = vec![(0.0f64, 0.0f64); n];
        for i in 0..n {
            for j in (i + 1)..n {
                let (dx, dy) = (pos[i].0 - pos[j].0, pos[i].1 - pos[j].1);
                let d = (dx * dx + dy * dy).sqrt().max(1e-6);
                let f = k * k / d;
                disp[i].0 += dx / d * f;
                disp[i].1 += dy / d * f;
                disp[j].0 -= dx / d * f;
                disp[j].1 -= dy / d * f;
            }
        }
        for &(a, b) in edges {
            let (dx, dy) = (pos[a].0 - pos[b].0, pos[a].1 - pos[b].1);
            let d = (dx * dx + dy * dy).sqrt().max(1e-6);
            let f = if electrical { d * d / k } else { d / k };
            disp[a].0 -= dx / d * f;
            disp[a].1 -= dy / d * f;
            disp[b].0 += dx / d * f;
            disp[b].1 += dy / d * f;
        }
        for i in 0..n {
            let d = (disp[i].0 * disp[i].0 + disp[i].1 * disp[i].1).sqrt();
            if d > 1e-9 {
                let lim = d.min(temp);
                pos[i].0 += disp[i].0 / d * lim;
                pos[i].1 += disp[i].1 / d * lim;
            }
        }
        temp *= 0.99;
        let cx = pos.iter().map(|p| p.0).sum::<f64>() / n as f64;
        let cy = pos.iter().map(|p| p.1).sum::<f64>() / n as f64;
        for p in &mut pos {
            p.0 -= cx;
            p.1 -= cy;
        }
    }
    // Keep coordinates finite even for degenerate (e.g. complete) graphs.
    for p in &mut pos {
        if !p.0.is_finite() {
            p.0 = 0.0;
        }
        if !p.1.is_finite() {
            p.1 = 0.0;
        }
    }
    pos
}

/// Deterministic 3D force-directed layout seeded on a Fibonacci sphere.
fn force_layout_3d(n: usize, edges: &[(usize, usize)]) -> Vec<(f64, f64, f64)> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![(0.0, 0.0, 0.0)];
    }
    let golden = PI * (1.0 + 5.0f64.sqrt());
    let mut pos: Vec<(f64, f64, f64)> = (0..n)
        .map(|i| {
            let t = (i as f64 + 0.5) / n as f64;
            let phi = (1.0 - 2.0 * t).acos();
            let theta = golden * i as f64;
            (phi.sin() * theta.cos(), phi.sin() * theta.sin(), phi.cos())
        })
        .collect();
    let k = (1.0 / n as f64).sqrt();
    let mut temp = 0.1f64;
    for _ in 0..200 {
        let mut disp = vec![(0.0f64, 0.0f64, 0.0f64); n];
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = pos[i].0 - pos[j].0;
                let dy = pos[i].1 - pos[j].1;
                let dz = pos[i].2 - pos[j].2;
                let d = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-6);
                let f = k * k / d;
                let (fx, fy, fz) = (dx / d * f, dy / d * f, dz / d * f);
                disp[i].0 += fx;
                disp[i].1 += fy;
                disp[i].2 += fz;
                disp[j].0 -= fx;
                disp[j].1 -= fy;
                disp[j].2 -= fz;
            }
        }
        for &(a, b) in edges {
            let dx = pos[a].0 - pos[b].0;
            let dy = pos[a].1 - pos[b].1;
            let dz = pos[a].2 - pos[b].2;
            let d = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-6);
            let f = d * d / k;
            let (fx, fy, fz) = (dx / d * f, dy / d * f, dz / d * f);
            disp[a].0 -= fx;
            disp[a].1 -= fy;
            disp[a].2 -= fz;
            disp[b].0 += fx;
            disp[b].1 += fy;
            disp[b].2 += fz;
        }
        for i in 0..n {
            let d = (disp[i].0 * disp[i].0 + disp[i].1 * disp[i].1 + disp[i].2 * disp[i].2).sqrt();
            if d > 1e-9 {
                let lim = d.min(temp);
                pos[i].0 += disp[i].0 / d * lim;
                pos[i].1 += disp[i].1 / d * lim;
                pos[i].2 += disp[i].2 / d * lim;
            }
        }
        temp *= 0.99;
    }
    pos
}

/// Orthographic-ish projection of 3D points to the plane.
fn project_3d(points: &[(f64, f64, f64)]) -> Vec<(f64, f64)> {
    let (ax, ay) = (0.5f64, 0.35f64);
    points
        .iter()
        .map(|&(x, y, z)| {
            let (sa, ca) = (ax.sin(), ax.cos());
            let (sb, cb) = (ay.sin(), ay.cos());
            let y1 = y * ca - z * sa;
            let z1 = y * sa + z * ca;
            let x2 = x * cb + z1 * sb;
            let z2 = -x * sb + z1 * cb;
            let d = 3.0;
            let s = d / (d + z2);
            (x2 * s, y1 * s)
        })
        .collect()
}

/// Layered (BFS-level) layout. `smoothing` reorders each layer by the
/// barycenter of its neighbours in the previous layer (`BarycentricEmbedding`).
fn layered_layout(g: &LayoutGraph, smoothing: bool) -> Vec<(f64, f64)> {
    let n = g.vertices.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![(0.0, 0.0)];
    }
    let adj = g.adjacency();
    let mut layer = vec![usize::MAX; n];
    let mut max_layer = 0usize;
    let mut base = 0usize;

    for s in 0..n {
        if layer[s] != usize::MAX {
            continue;
        }
        // Gather the component and pick its highest-degree vertex as root.
        let mut comp = Vec::new();
        let mut q = VecDeque::new();
        let mut seen = vec![false; n];
        q.push_back(s);
        seen[s] = true;
        while let Some(u) = q.pop_front() {
            comp.push(u);
            for &w in &adj[u] {
                if !seen[w] {
                    seen[w] = true;
                    q.push_back(w);
                }
            }
        }
        let root = *comp
            .iter()
            .max_by_key(|&&i| (adj[i].len(), std::cmp::Reverse(i)))
            .unwrap();
        let mut dq = VecDeque::new();
        layer[root] = base;
        dq.push_back(root);
        while let Some(u) = dq.pop_front() {
            for &w in &adj[u] {
                if layer[w] == usize::MAX {
                    layer[w] = layer[u] + 1;
                    dq.push_back(w);
                }
            }
        }
        let cmax = comp.iter().map(|&i| layer[i]).max().unwrap();
        max_layer = max_layer.max(cmax);
        base = max_layer + 2;
    }

    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); max_layer + 1];
    for (i, &l) in layer.iter().enumerate() {
        layers[l].push(i);
    }
    for l in layers.iter_mut() {
        l.sort_unstable();
    }

    if smoothing {
        for _ in 0..2 {
            for l in 1..layers.len() {
                let order: HashMap<usize, usize> = layers[l - 1]
                    .iter()
                    .enumerate()
                    .map(|(k, &v)| (v, k))
                    .collect();
                let mut keyed: Vec<(f64, usize)> = layers[l]
                    .iter()
                    .map(|&v| {
                        let mut sum = 0.0;
                        let mut cnt = 0usize;
                        for &w in &adj[v] {
                            if let Some(&p) = order.get(&w) {
                                sum += p as f64;
                                cnt += 1;
                            }
                        }
                        let bary = if cnt > 0 { sum / cnt as f64 } else { v as f64 };
                        (bary, v)
                    })
                    .collect();
                keyed.sort_by(|a, b| {
                    a.0.partial_cmp(&b.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.1.cmp(&b.1))
                });
                layers[l] = keyed.into_iter().map(|(_, v)| v).collect();
            }
        }
    }

    let mut coords = vec![(0.0f64, 0.0f64); n];
    for (l, items) in layers.iter().enumerate() {
        let len = items.len();
        for (k, &v) in items.iter().enumerate() {
            let y = k as f64 - (len.saturating_sub(1)) as f64 / 2.0;
            coords[v] = (l as f64, y);
        }
    }
    coords
}

/// True when the underlying undirected graph is a forest (acyclic).
fn is_forest(g: &LayoutGraph) -> bool {
    let n = g.vertices.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for &(a, b, _) in &g.edges {
        if a == b {
            return false;
        }
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra == rb {
            return false;
        }
        parent[ra] = rb;
    }
    true
}

// ── SVG rendering helper ────────────────────────────────────────────────────

/// Read an `ImageSize -> {w, h}` option from trailing arguments.
fn image_size(args: &[Value]) -> (f64, f64) {
    for a in args {
        if let Value::Rule { lhs, rhs, .. } = a
            && to_vertex(lhs) == Value::Symbol("ImageSize".to_string())
            && let Value::List(l) = to_vertex(rhs)
            && l.len() >= 2
            && let (Some(w), Some(h)) = (super::to_f64(&l[0]), super::to_f64(&l[1]))
        {
            return (w, h);
        }
    }
    (600.0, 400.0)
}

/// Render a graph layout to SVG via [`super::graphics::render_svg`].
fn graph_svg(g: &LayoutGraph, coords: &[(f64, f64)], size: (f64, f64)) -> Result<Value, EvalError> {
    let mut prims: Vec<Value> = Vec::new();
    for &(a, b, _) in &g.edges {
        if a < coords.len() && b < coords.len() {
            prims.push(Value::Call {
                head: "Line".to_string(),
                args: vec![Value::List(vec![
                    coord(coords[a].0, coords[a].1),
                    coord(coords[b].0, coords[b].1),
                ])],
            });
        }
    }
    for &(x, y) in coords {
        prims.push(Value::Call {
            head: "Point".to_string(),
            args: vec![coord(x, y)],
        });
    }
    let mut opts = HashMap::new();
    opts.insert(
        "ImageSize".to_string(),
        Value::List(vec![super::real(size.0), super::real(size.1)]),
    );
    opts.insert("Axes".to_string(), Value::Bool(false));
    let svg = super::graphics::render_svg(&Value::List(prims), &Value::Assoc(opts))?;
    Ok(Value::Str(svg))
}

// ── Embeddings ──────────────────────────────────────────────────────────────

/// Compute a layout for `g` under the named method.
fn embedding(g: &LayoutGraph, method: &str) -> Option<Vec<(f64, f64)>> {
    match method {
        "SpringEmbedding" | "Spring" => {
            Some(force_layout(g.vertices.len(), &g.undirected_edges(), false))
        }
        "SpringElectricalEmbedding" | "SpringElectrical" => {
            Some(force_layout(g.vertices.len(), &g.undirected_edges(), true))
        }
        "BarycentricEmbedding" | "Barycentric" => Some(layered_layout(g, true)),
        _ => None,
    }
}

/// Name of an embedding method from a `Symbol`, `String`, `Call` head, or the
/// `Builtin` value produced when the bare symbol is evaluated.
fn method_name(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) | Value::Symbol(s) => Some(s.clone()),
        Value::Builtin(n, _) => Some(n.clone()),
        Value::Call { head, .. } => Some(head.clone()),
        Value::Pattern(expr) => match crate::pattern::unwrap_expr_to_value(expr) {
            Value::Symbol(s) | Value::Str(s) => Some(s),
            _ => None,
        },
        _ => None,
    }
}

fn coords_value(coords: &[(f64, f64)]) -> Value {
    Value::List(coords.iter().map(|&(x, y)| coord(x, y)).collect())
}

/// GraphEmbedding[g] / GraphEmbedding[g, method]
pub fn builtin_graph_embedding(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(parse_graph) else {
        return Ok(unevaluated("GraphEmbedding", args));
    };
    let method = match args.get(1) {
        None => "SpringElectricalEmbedding".to_string(),
        Some(m) => match method_name(m) {
            Some(name) => name,
            None => return Ok(unevaluated("GraphEmbedding", args)),
        },
    };
    match embedding(&g, &method) {
        Some(coords) => Ok(coords_value(&coords)),
        None => Ok(unevaluated("GraphEmbedding", args)),
    }
}

fn embedding_head(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(parse_graph) else {
        return Ok(unevaluated(name, args));
    };
    match embedding(&g, name) {
        Some(coords) => Ok(coords_value(&coords)),
        None => Ok(unevaluated(name, args)),
    }
}

/// SpringEmbedding[g]
pub fn builtin_spring_embedding(args: &[Value]) -> Result<Value, EvalError> {
    embedding_head("SpringEmbedding", args)
}

/// SpringElectricalEmbedding[g]
pub fn builtin_spring_electrical_embedding(args: &[Value]) -> Result<Value, EvalError> {
    embedding_head("SpringElectricalEmbedding", args)
}

/// BarycentricEmbedding[g]
pub fn builtin_barycentric_embedding(args: &[Value]) -> Result<Value, EvalError> {
    embedding_head("BarycentricEmbedding", args)
}

// ── Plotting ────────────────────────────────────────────────────────────────

/// Graph3D[g, ...opts] — 3D force layout projected to an SVG.
pub fn builtin_graph3d(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(parse_graph) else {
        return Ok(unevaluated("Graph3D", args));
    };
    let coords3 = force_layout_3d(g.vertices.len(), &g.undirected_edges());
    let projected = project_3d(&coords3);
    graph_svg(&g, &projected, image_size(args))
}

/// LayeredGraphPlot[g, ...opts] — layered BFS layout rendered to SVG.
pub fn builtin_layered_graph_plot(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(parse_graph) else {
        return Ok(unevaluated("LayeredGraphPlot", args));
    };
    let coords = layered_layout(&g, false);
    graph_svg(&g, &coords, image_size(args))
}

/// TreePlot[g, ...opts] — barycentric tree layout rendered to SVG.
///
/// Stays unevaluated when the underlying undirected graph is cyclic.
pub fn builtin_tree_plot(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(parse_graph) else {
        return Ok(unevaluated("TreePlot", args));
    };
    if !is_forest(&g) {
        return Ok(unevaluated("TreePlot", args));
    }
    let coords = layered_layout(&g, true);
    graph_svg(&g, &coords, image_size(args))
}

// ── Barabasi–Albert graph distribution ──────────────────────────────────────

fn as_usize(v: &Value) -> Option<usize> {
    match v {
        Value::Integer(n) if n.cmp0() != std::cmp::Ordering::Less => n.to_usize(),
        _ => None,
    }
}

/// BarabasiAlbertGraphDistribution[n, m] — deterministic preferential-attachment
/// graph (best-effort sample). Starts from `m` isolated vertices and attaches
/// each new vertex to the `m` highest-degree vertices (ties broken by index).
pub fn builtin_barabasi_albert(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("BarabasiAlbertGraphDistribution", args));
    }
    let (Some(n), Some(m)) = (as_usize(&args[0]), as_usize(&args[1])) else {
        return Ok(unevaluated("BarabasiAlbertGraphDistribution", args));
    };
    if n < 1 || m < 1 || m > n {
        return Ok(unevaluated("BarabasiAlbertGraphDistribution", args));
    }

    let mut degree = vec![0usize; n];
    let base = m.min(n);
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for i in base..n {
        let mut targets: Vec<usize> = (0..i).collect();
        targets.sort_by(|&a, &b| degree[b].cmp(&degree[a]).then(a.cmp(&b)));
        targets.truncate(m.min(i));
        targets.sort_unstable();
        for &t in &targets {
            edges.push((t, i));
            degree[t] += 1;
            degree[i] += 1;
        }
    }

    let vlist = Value::List((1..=n).map(|i| int(i as i64)).collect());
    let elist = Value::List(
        edges
            .into_iter()
            .map(|(a, b)| Value::Call {
                head: "UndirectedEdge".to_string(),
                args: vec![int(a as i64 + 1), int(b as i64 + 1)],
            })
            .collect(),
    );
    Ok(Value::Call {
        head: "Graph".to_string(),
        args: vec![vlist, elist],
    })
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    use super::register_builtin;
    register_builtin(env, "Graph3D", builtin_graph3d);
    register_builtin(env, "LayeredGraphPlot", builtin_layered_graph_plot);
    register_builtin(env, "TreePlot", builtin_tree_plot);
    register_builtin(env, "GraphEmbedding", builtin_graph_embedding);
    register_builtin(env, "SpringEmbedding", builtin_spring_embedding);
    register_builtin(
        env,
        "SpringElectricalEmbedding",
        builtin_spring_electrical_embedding,
    );
    register_builtin(env, "BarycentricEmbedding", builtin_barycentric_embedding);
    register_builtin(
        env,
        "BarabasiAlbertGraphDistribution",
        builtin_barabasi_albert,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Env;

    fn eval_str(input: &str) -> Value {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let tokens = crate::lexer::tokenize(input).unwrap_or_else(|e| panic!("lex {input}: {e:?}"));
        let ast = crate::parser::parse(tokens).unwrap_or_else(|e| panic!("parse {input}: {e:?}"));
        crate::eval::eval_program(&ast, &env).unwrap_or_else(|e| panic!("eval {input}: {e:?}"))
    }

    fn ints(v: &Value) -> Vec<i64> {
        match v {
            Value::List(items) => items
                .iter()
                .map(|x| match x {
                    Value::Integer(n) => n.to_i64().unwrap(),
                    _ => panic!("expected integer, got {x:?}"),
                })
                .collect(),
            _ => panic!("expected list, got {v:?}"),
        }
    }

    /// Assert a value is a list of `{x, y}` numeric coordinate pairs.
    fn assert_coords(v: &Value, n: usize) {
        match v {
            Value::List(pts) => {
                assert_eq!(pts.len(), n, "expected {n} coordinates, got {}", pts.len());
                for p in pts {
                    match p {
                        Value::List(xy) => {
                            assert_eq!(xy.len(), 2);
                            assert!(super::super::to_f64(&xy[0]).is_some());
                            assert!(super::super::to_f64(&xy[1]).is_some());
                        }
                        other => panic!("expected coordinate pair, got {other:?}"),
                    }
                }
            }
            other => panic!("expected coordinate list, got {other:?}"),
        }
    }

    fn assert_svg(v: &Value) {
        match v {
            Value::Str(s) => {
                assert!(s.contains("<svg"), "missing <svg: {s}");
                assert!(s.contains("</svg>"), "missing </svg: {s}");
                assert!(s.contains("<circle") || s.contains("<polyline"));
            }
            other => panic!("expected svg, got {other:?}"),
        }
    }

    #[test]
    fn test_graph_embedding_default_and_methods() {
        let g = "Graph[{1<->2, 2<->3, 3<->1, 3<->4}]";
        assert_coords(&eval_str(&format!("GraphEmbedding[{g}]")), 4);
        assert_coords(
            &eval_str(&format!("GraphEmbedding[{g}, \"SpringEmbedding\"]")),
            4,
        );
        assert_coords(
            &eval_str(&format!("GraphEmbedding[{g}, \"BarycentricEmbedding\"]")),
            4,
        );
        // Method may be passed as the bare (evaluated) head symbol.
        assert_coords(
            &eval_str(&format!("GraphEmbedding[{g}, SpringEmbedding]")),
            4,
        );
    }

    #[test]
    fn test_named_embedding_heads() {
        let g = "Graph[{1<->2, 2<->3, 3<->1}]";
        assert_coords(&eval_str(&format!("SpringEmbedding[{g}]")), 3);
        assert_coords(&eval_str(&format!("SpringElectricalEmbedding[{g}]")), 3);
        assert_coords(&eval_str(&format!("BarycentricEmbedding[{g}]")), 3);
    }

    #[test]
    fn test_embedding_deterministic() {
        let g = "Graph[{1<->2, 2<->3, 3<->1, 4<->1}]";
        let a = eval_str(&format!("SpringEmbedding[{g}]"));
        let b = eval_str(&format!("SpringEmbedding[{g}]"));
        assert_eq!(a.to_string(), b.to_string());
    }

    #[test]
    fn test_embedding_single_vertex() {
        assert_coords(&eval_str("GraphEmbedding[Graph[{1}]]"), 1);
    }

    #[test]
    fn test_graph3d_svg() {
        assert_svg(&eval_str("Graph3D[Graph[{1<->2, 2<->3, 3<->1}]]"));
    }

    #[test]
    fn test_layered_graph_plot_svg() {
        assert_svg(&eval_str("LayeredGraphPlot[Graph[{1->2, 1->3, 2->4}]]"));
    }

    #[test]
    fn test_tree_plot_svg() {
        assert_svg(&eval_str("TreePlot[Graph[{1<->2, 1<->3, 2<->4, 2<->5}]]"));
    }

    #[test]
    fn test_tree_plot_rejects_cycles() {
        match eval_str("TreePlot[Graph[{1<->2, 2<->3, 3<->1}]]") {
            Value::Call { head, .. } => assert_eq!(head, "TreePlot"),
            other => panic!("expected unevaluated TreePlot, got {other:?}"),
        }
    }

    #[test]
    fn test_barabasi_albert_structure() {
        let g = eval_str("BarabasiAlbertGraphDistribution[6, 2]");
        let count = eval_str("VertexCount[BarabasiAlbertGraphDistribution[6, 2]]");
        assert_eq!(count, int(6));
        // (n - m) * m = 4 * 2 = 8 edges.
        let edges = eval_str("EdgeCount[BarabasiAlbertGraphDistribution[6, 2]]");
        assert_eq!(edges, int(8));
        // Vertices are 1..=6.
        assert_eq!(
            ints(&eval_str(
                "VertexList[BarabasiAlbertGraphDistribution[6, 2]]"
            )),
            vec![1, 2, 3, 4, 5, 6]
        );
        // Deterministic: repeated construction is identical.
        assert_eq!(
            g.to_string(),
            eval_str("BarabasiAlbertGraphDistribution[6, 2]").to_string()
        );
    }

    #[test]
    fn test_unevaluated_on_bad_input() {
        for expr in [
            "Graph3D[5]",
            "LayeredGraphPlot[5]",
            "TreePlot[5]",
            "GraphEmbedding[5]",
            "SpringEmbedding[5]",
            "GraphEmbedding[Graph[{1<->2}], \"Bogus\"]",
            "BarabasiAlbertGraphDistribution[1]",
            "BarabasiAlbertGraphDistribution[3, 0]",
        ] {
            let head = expr.split('[').next().unwrap();
            assert!(
                matches!(eval_str(expr), Value::Call { head: h, .. } if h == head),
                "{expr} should stay unevaluated"
            );
        }
    }
}
