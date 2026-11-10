//! graphs: Graph, vertices/edges, adjacency, connectivity, shortest paths, GraphPlot.
//!
//! A graph is represented as `Value::Call { head: "Graph", args: [..] }`, where the
//! arguments are a vertex list and/or an edge list, followed by options. Edges may be
//! written as `UndirectedEdge[a, b]` (parsed from `a <-> b`), `DirectedEdge[a, b]`, or
//! a rule `a -> b`. The helpers below build a compact index-based [`Graph`] structure
//! from that expression form; the public builtins convert back to ordinary
//! expressions and stay unevaluated when given inapplicable input.

use crate::env::{Env, Fixity, OperatorInfo};
use crate::value::{EvalError, Value};
use rug::Integer;

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

fn int(n: usize) -> Value {
    Value::Integer(Integer::from(n as i64))
}

// ── Graph model ─────────────────────────────────────────────────────────────

/// Directedness of a single edge.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Directedness {
    Undirected,
    Directed,
}

/// An edge as a pair of vertex indices plus directedness.
struct Edge {
    a: usize,
    b: usize,
    directed: bool,
}

/// Compact index-based graph.
struct Graph {
    vertices: Vec<Value>,
    edges: Vec<Edge>,
}

impl Graph {
    fn new() -> Self {
        Graph {
            vertices: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Add a vertex if not already present; returns its index.
    fn intern(&mut self, v: Value) -> usize {
        if let Some(i) = self.vertices.iter().position(|x| *x == v) {
            i
        } else {
            self.vertices.push(v);
            self.vertices.len() - 1
        }
    }

    fn index_of(&self, v: &Value) -> Option<usize> {
        let v = to_vertex(v);
        self.vertices.iter().position(|x| *x == v)
    }

    /// Out-neighbour indices for every vertex (undirected edges contribute both ways).
    fn out_neighbors(&self) -> Vec<Vec<usize>> {
        let mut adj = vec![Vec::new(); self.vertices.len()];
        for e in &self.edges {
            adj[e.a].push(e.b);
            if !e.directed {
                adj[e.b].push(e.a);
            }
        }
        adj
    }

    /// Neighbour indices ignoring direction (used for connectivity).
    fn weak_neighbors(&self) -> Vec<Vec<usize>> {
        let mut adj = vec![Vec::new(); self.vertices.len()];
        for e in &self.edges {
            adj[e.a].push(e.b);
            adj[e.b].push(e.a);
        }
        adj
    }

    /// True if the graph contains at least one directed edge.
    fn is_directed(&self) -> bool {
        self.edges.iter().any(|e| e.directed)
    }
}

/// Split an edge expression into `(pairs, directedness)`.
///
/// `a -> b` and `DirectedEdge[a, b]` are directed; `UndirectedEdge[a, b]` (also the
/// chain form `a <-> b <-> c`) is undirected.
fn edge_parts(e: &Value) -> Option<(Vec<(Value, Value)>, Directedness)> {
    match e {
        Value::Rule { lhs, rhs, .. } => Some((
            vec![(to_vertex(lhs), to_vertex(rhs))],
            Directedness::Directed,
        )),
        Value::Call { head, args } if head == "UndirectedEdge" && args.len() >= 2 => Some((
            args.windows(2)
                .map(|w| (to_vertex(&w[0]), to_vertex(&w[1])))
                .collect(),
            Directedness::Undirected,
        )),
        Value::Call { head, args } if head == "DirectedEdge" && args.len() >= 2 => Some((
            args.windows(2)
                .map(|w| (to_vertex(&w[0]), to_vertex(&w[1])))
                .collect(),
            Directedness::Directed,
        )),
        _ => None,
    }
}

fn is_edge(v: &Value) -> bool {
    edge_parts(v).is_some()
}

/// Interpret a value as a graph. Accepts either a `Graph[...]` expression or a bare
/// list of edge expressions.
fn as_graph(v: &Value) -> Option<Graph> {
    if let Value::Call { head, args } = v
        && head == "Graph"
    {
        return parse_graph_args(args);
    }
    if let Value::List(items) = v
        && !items.is_empty()
        && items.iter().all(is_edge)
    {
        let mut g = Graph::new();
        for e in items {
            add_edge_expr(&mut g, e)?;
        }
        return Some(g);
    }
    None
}

/// Build a graph from the argument list of a `Graph[...]` expression.
///
/// Leading list arguments are the vertex list (and optional edge list); a single list
/// argument is treated as an edge list when all of its elements look like edges,
/// otherwise as a vertex list.
fn parse_graph_args(args: &[Value]) -> Option<Graph> {
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

    let mut g = Graph::new();
    if let Some(vl) = vlist {
        for v in vl {
            g.intern(to_vertex(v));
        }
    }
    if let Some(el) = elist {
        for e in el {
            add_edge_expr(&mut g, e)?;
        }
    }
    Some(g)
}

fn add_edge_expr(g: &mut Graph, e: &Value) -> Option<()> {
    let (pairs, dir) = edge_parts(e)?;
    for (a, b) in pairs {
        let ia = g.intern(a);
        let ib = g.intern(b);
        g.edges.push(Edge {
            a: ia,
            b: ib,
            directed: dir == Directedness::Directed,
        });
    }
    Some(())
}

// ── Basic accessors ─────────────────────────────────────────────────────────

/// Graph[{vertices, edges, ...opts}] — canonical graph expression.
pub fn builtin_graph(args: &[Value]) -> Result<Value, EvalError> {
    // Validate that the data portion is well formed; keep options untouched.
    if parse_graph_args(args).is_none() {
        return Ok(unevaluated("Graph", args));
    }
    Ok(Value::Call {
        head: "Graph".to_string(),
        args: args.to_vec(),
    })
}

/// GraphQ[expr] — True when `expr` denotes a well-formed graph.
pub fn builtin_graph_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("GraphQ", args));
    }
    Ok(Value::Bool(as_graph(&args[0]).is_some()))
}

/// VertexList[g]
pub fn builtin_vertex_list(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("VertexList", args));
    };
    Ok(Value::List(g.vertices.clone()))
}

/// EdgeList[g]
pub fn builtin_edge_list(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("EdgeList", args));
    };
    let out = g
        .edges
        .iter()
        .map(|e| Value::Call {
            head: if e.directed {
                "DirectedEdge".to_string()
            } else {
                "UndirectedEdge".to_string()
            },
            args: vec![g.vertices[e.a].clone(), g.vertices[e.b].clone()],
        })
        .collect();
    Ok(Value::List(out))
}

/// VertexCount[g]
pub fn builtin_vertex_count(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("VertexCount", args));
    };
    Ok(int(g.vertices.len()))
}

/// EdgeCount[g]
pub fn builtin_edge_count(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("EdgeCount", args));
    };
    Ok(int(g.edges.len()))
}

/// AdjacencyMatrix[g]
pub fn builtin_adjacency_matrix(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("AdjacencyMatrix", args));
    };
    let n = g.vertices.len();
    let mut m = vec![vec![0usize; n]; n];
    for e in &g.edges {
        m[e.a][e.b] += 1;
        if !e.directed {
            m[e.b][e.a] += 1;
        }
    }
    let rows = m
        .into_iter()
        .map(|row| Value::List(row.into_iter().map(int).collect()))
        .collect();
    Ok(Value::List(rows))
}

/// AdjacencyList[g]
pub fn builtin_adjacency_list(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("AdjacencyList", args));
    };
    let adj = g.out_neighbors();
    let out = adj
        .into_iter()
        .map(|ns| Value::List(ns.into_iter().map(|j| g.vertices[j].clone()).collect()))
        .collect();
    Ok(Value::List(out))
}

// ── Degree ──────────────────────────────────────────────────────────────────

/// Compute in/out/total degree vectors. `mode` selects the kind of degree.
fn degrees(g: &Graph, mode: &str) -> Vec<usize> {
    let n = g.vertices.len();
    let mut out = vec![0usize; n];
    let mut inc = vec![0usize; n];
    for e in &g.edges {
        if e.directed {
            out[e.a] += 1;
            inc[e.b] += 1;
        } else {
            out[e.a] += 1;
            out[e.b] += 1;
        }
    }
    match mode {
        "Out" => out,
        "In" if g.is_directed() => inc,
        "In" => out, // undirected: in-degree equals degree
        _ => out.iter().zip(inc.iter()).map(|(a, b)| a + b).collect(),
    }
}

fn degree_dispatch(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated(name, args));
    };
    for spec in &args[1..] {
        if let Value::Str(s) = spec
            && !matches!(s.as_str(), "In" | "Out" | "Total")
        {
            return Ok(unevaluated(name, args));
        }
    }
    let mode = args
        .iter()
        .find_map(|a| match a {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or("Total");
    let deg = degrees(&g, mode);

    // Optional vertex argument: degree for one vertex.
    let vert = args[1..]
        .iter()
        .find(|a| !matches!(a, Value::Str(_)))
        .map(to_vertex);
    match vert {
        None => Ok(Value::List(deg.into_iter().map(int).collect())),
        Some(v) => match g.vertices.iter().position(|x| *x == v) {
            Some(i) => Ok(int(deg[i])),
            None => Ok(unevaluated(name, args)),
        },
    }
}

/// Degree[g] / Degree[g, v] — also `Degree[]` gives the angle constant π/180.
pub fn builtin_degree(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Real(rug::Float::with_val(
            crate::value::DEFAULT_PRECISION,
            std::f64::consts::PI / 180.0,
        )));
    }
    degree_dispatch("Degree", args)
}

/// VertexDegree[g] / VertexDegree[g, v]
pub fn builtin_vertex_degree(args: &[Value]) -> Result<Value, EvalError> {
    degree_dispatch("VertexDegree", args)
}

/// DegreeList[g] / DegreeList[g, v] — the degree sequence (alias of `Degree[g]`).
pub fn builtin_degree_list(args: &[Value]) -> Result<Value, EvalError> {
    degree_dispatch("DegreeList", args)
}

// ── Connectivity and paths ──────────────────────────────────────────────────

/// ConnectedComponents[g] — weakly connected components.
pub fn builtin_connected_components(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("ConnectedComponents", args));
    };
    let adj = g.weak_neighbors();
    let n = g.vertices.len();
    let mut seen = vec![false; n];
    let mut comps = Vec::new();
    for start in 0..n {
        if seen[start] {
            continue;
        }
        let mut queue = vec![start];
        seen[start] = true;
        let mut comp = Vec::new();
        while let Some(u) = queue.pop() {
            comp.push(u);
            for &w in &adj[u] {
                if !seen[w] {
                    seen[w] = true;
                    queue.push(w);
                }
            }
        }
        comp.sort_unstable();
        comps.push(Value::List(
            comp.into_iter().map(|i| g.vertices[i].clone()).collect(),
        ));
    }
    Ok(Value::List(comps))
}

/// BFS returning parent pointers from `source`.
fn bfs(g: &Graph, adj: &[Vec<usize>], source: usize) -> (Vec<i64>, Vec<usize>) {
    let n = g.vertices.len();
    let mut dist = vec![-1i64; n];
    let mut parent = vec![usize::MAX; n];
    dist[source] = 0;
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(source);
    while let Some(u) = queue.pop_front() {
        for &w in &adj[u] {
            if dist[w] < 0 {
                dist[w] = dist[u] + 1;
                parent[w] = u;
                queue.push_back(w);
            }
        }
    }
    (dist, parent)
}

/// Resolve a stop vertex to an index, or `None` when absent.
fn resolve(g: &Graph, v: &Value) -> Option<usize> {
    g.index_of(v)
}

/// FindShortestPath[g, s, t]
pub fn builtin_find_shortest_path(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Ok(unevaluated("FindShortestPath", args));
    }
    let Some(g) = as_graph(&args[0]) else {
        return Ok(unevaluated("FindShortestPath", args));
    };
    let sval = to_vertex(&args[1]);
    let tval = to_vertex(&args[2]);
    if sval == tval {
        return Ok(Value::List(vec![sval]));
    }
    let (Some(si), Some(ti)) = (resolve(&g, &sval), resolve(&g, &tval)) else {
        return Ok(Value::List(vec![]));
    };
    let adj = g.out_neighbors();
    let (dist, parent) = bfs(&g, &adj, si);
    if dist[ti] < 0 {
        return Ok(Value::List(vec![]));
    }
    let mut path = vec![ti];
    let mut cur = ti;
    while cur != si {
        cur = parent[cur];
        path.push(cur);
    }
    path.reverse();
    Ok(Value::List(
        path.into_iter().map(|i| g.vertices[i].clone()).collect(),
    ))
}

/// GraphDistance[g, s, t] or GraphDistance[g, s]
pub fn builtin_graph_distance(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 && args.len() != 3 {
        return Ok(unevaluated("GraphDistance", args));
    }
    let Some(g) = as_graph(&args[0]) else {
        return Ok(unevaluated("GraphDistance", args));
    };
    let adj = g.out_neighbors();
    let infinity = || Value::Symbol("Infinity".to_string());
    let sval = to_vertex(&args[1]);
    if args.len() == 2 {
        let Some(si) = resolve(&g, &sval) else {
            return Ok(unevaluated("GraphDistance", args));
        };
        let (dist, _) = bfs(&g, &adj, si);
        let out = dist
            .into_iter()
            .map(|d| if d < 0 { infinity() } else { int(d as usize) })
            .collect();
        return Ok(Value::List(out));
    }
    let tval = to_vertex(&args[2]);
    if sval == tval {
        return Ok(int(0));
    }
    let (Some(si), Some(ti)) = (resolve(&g, &sval), resolve(&g, &tval)) else {
        return Ok(infinity());
    };
    let (dist, _) = bfs(&g, &adj, si);
    if dist[ti] < 0 {
        Ok(infinity())
    } else {
        Ok(int(dist[ti] as usize))
    }
}

/// NeighborhoodGraph[g, v] / NeighborhoodGraph[g, {v1, ...}]
pub fn builtin_neighborhood_graph(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Ok(unevaluated("NeighborhoodGraph", args));
    }
    let Some(g) = as_graph(&args[0]) else {
        return Ok(unevaluated("NeighborhoodGraph", args));
    };
    let seeds: Vec<Value> = match &args[1] {
        Value::List(items) => items.iter().map(to_vertex).collect(),
        other => vec![to_vertex(other)],
    };
    let adj = g.out_neighbors();
    let mut include = vec![false; g.vertices.len()];
    for s in &seeds {
        let Some(i) = g.vertices.iter().position(|x| x == s) else {
            continue;
        };
        include[i] = true;
        for &w in &adj[i] {
            include[w] = true;
        }
    }
    if !include.iter().any(|b| *b) {
        return Ok(unevaluated("NeighborhoodGraph", args));
    }

    // Preserve original vertex order, then induced edges.
    let kept: Vec<usize> = (0..g.vertices.len()).filter(|i| include[*i]).collect();
    let vlist = Value::List(kept.iter().map(|&i| g.vertices[i].clone()).collect());
    let elist = Value::List(
        g.edges
            .iter()
            .filter(|e| include[e.a] && include[e.b])
            .map(|e| Value::Call {
                head: if e.directed {
                    "DirectedEdge".to_string()
                } else {
                    "UndirectedEdge".to_string()
                },
                args: vec![g.vertices[e.a].clone(), g.vertices[e.b].clone()],
            })
            .collect(),
    );
    Ok(Value::Call {
        head: "Graph".to_string(),
        args: vec![vlist, elist],
    })
}

// ── GraphPlot ───────────────────────────────────────────────────────────────

fn svg_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn label_of(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 8 {
        s.chars().take(8).collect()
    } else {
        s
    }
}

/// Whether a single option disables vertex labels (`VertexLabels -> None`).
fn rule_disables_labels(a: &Value) -> bool {
    if let Value::Rule { lhs, rhs, .. } = a
        && to_vertex(lhs) == Value::Symbol("VertexLabels".to_string())
    {
        let r = to_vertex(rhs);
        return matches!(r, Value::Symbol(ref s) if s == "None") || matches!(r, Value::Null);
    }
    false
}

/// Whether vertex labels should be drawn. Options may appear on `GraphPlot` itself
/// or on the inner `Graph` expression.
fn labels_enabled(args: &[Value]) -> bool {
    let outer = args.iter().any(rule_disables_labels);
    let inner = args.first().is_some_and(|a| match a {
        Value::Call { head, args: inner } if head == "Graph" => {
            inner.iter().any(rule_disables_labels)
        }
        _ => false,
    });
    !(outer || inner)
}

/// GraphPlot[g, ...opts] — circular-layout SVG rendering.
pub fn builtin_graph_plot(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("GraphPlot", args));
    };
    let n = g.vertices.len();
    let width = 600.0f64;
    let height = 400.0f64;
    let cx = width / 2.0;
    let cy = height / 2.0;
    let radius = 150.0f64;
    let vr = 13.0f64;

    let positions: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let angle = -std::f64::consts::FRAC_PI_2
                + 2.0 * std::f64::consts::PI * (i as f64) / (n.max(1) as f64);
            (cx + radius * angle.cos(), cy + radius * angle.sin())
        })
        .collect();

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\" style=\"background:white\">\n"
    ));

    // Edges first so vertices sit on top.
    for e in &g.edges {
        let (x1, y1) = positions[e.a];
        let (x2, y2) = positions[e.b];
        svg.push_str(&format!(
            "<line x1=\"{x1:.2}\" y1=\"{y1:.2}\" x2=\"{x2:.2}\" y2=\"{y2:.2}\" \
             stroke=\"#888\" stroke-width=\"1.5\"/>\n"
        ));
        if e.directed {
            let (dx, dy) = (x2 - x1, y2 - y1);
            let len = (dx * dx + dy * dy).sqrt().max(1e-9);
            let (ux, uy) = (dx / len, dy / len);
            let (tx, ty) = (x2 - ux * vr, y2 - uy * vr);
            let (px, py) = (-uy, ux);
            let size = 7.0;
            let ax = tx - ux * size;
            let ay = ty - uy * size;
            svg.push_str(&format!(
                "<polygon points=\"{tx:.2},{ty:.2} {:.2},{:.2} {:.2},{:.2}\" fill=\"#888\"/>\n",
                ax + px * size * 0.5,
                ay + py * size * 0.5,
                ax - px * size * 0.5,
                ay - py * size * 0.5,
            ));
        }
    }

    let show_labels = labels_enabled(args);
    for (i, (x, y)) in positions.iter().enumerate() {
        svg.push_str(&format!(
            "<circle cx=\"{x:.2}\" cy=\"{y:.2}\" r=\"{vr}\" fill=\"#4C78A8\" \
             stroke=\"#ffffff\" stroke-width=\"1.5\"/>\n"
        ));
        if show_labels {
            svg.push_str(&format!(
                "<text x=\"{x:.2}\" y=\"{y:.2}\" dy=\"0.35em\" text-anchor=\"middle\" \
                 font-size=\"11\" font-family=\"monospace\" fill=\"#ffffff\">{}</text>\n",
                svg_escape(&label_of(&g.vertices[i]))
            ));
        }
    }

    svg.push_str("</svg>\n");
    Ok(Value::Str(svg))
}

// ── Extended algorithms ─────────────────────────────────────────────────────

/// The symbolic `Infinity` used for unreachable distances.
fn infinity() -> Value {
    Value::Symbol("Infinity".to_string())
}

/// Build a machine-precision real value.
fn real(x: f64) -> Value {
    Value::Real(rug::Float::with_val(crate::value::DEFAULT_PRECISION, x))
}

/// Render one edge as an `UndirectedEdge`/`DirectedEdge` expression.
fn edge_value(e: &Edge, verts: &[Value]) -> Value {
    Value::Call {
        head: if e.directed {
            "DirectedEdge".to_string()
        } else {
            "UndirectedEdge".to_string()
        },
        args: vec![verts[e.a].clone(), verts[e.b].clone()],
    }
}

/// Render a graph as `Graph[{vertices}, {edges}]`.
fn graph_value(g: &Graph) -> Value {
    let vlist = Value::List(g.vertices.clone());
    let elist = Value::List(g.edges.iter().map(|e| edge_value(e, &g.vertices)).collect());
    Value::Call {
        head: "Graph".to_string(),
        args: vec![vlist, elist],
    }
}

/// All-pairs shortest-path lengths; `-1` marks an unreachable pair.
fn all_pairs(g: &Graph) -> Vec<Vec<i64>> {
    let adj = g.out_neighbors();
    (0..g.vertices.len()).map(|s| bfs(g, &adj, s).0).collect()
}

/// Eccentricity of every vertex (`None` = Infinity, i.e. a disconnected graph).
fn eccentricities(g: &Graph) -> Vec<Option<usize>> {
    all_pairs(g)
        .into_iter()
        .map(|row| {
            let mut m = 0usize;
            for x in row {
                if x < 0 {
                    return None;
                }
                m = m.max(x as usize);
            }
            Some(m)
        })
        .collect()
}

/// Coerce an argument that may be a vertex or list of vertices into a value list.
fn vertex_spec(v: &Value) -> Vec<Value> {
    match v {
        Value::List(items) => items.iter().map(to_vertex).collect(),
        other => vec![to_vertex(other)],
    }
}

/// True when an edge expression matches an edge (ignoring orientation when undirected).
fn edge_matches(g: &Graph, e: &Edge, spec: &Value) -> bool {
    let Some((pairs, dir)) = edge_parts(spec) else {
        return false;
    };
    let directed = dir == Directedness::Directed;
    if directed != e.directed {
        return false;
    }
    let (va, vb) = (&g.vertices[e.a], &g.vertices[e.b]);
    pairs.iter().any(|(a, b)| {
        if directed {
            to_vertex(a) == *va && to_vertex(b) == *vb
        } else {
            (to_vertex(a) == *va && to_vertex(b) == *vb)
                || (to_vertex(a) == *vb && to_vertex(b) == *va)
        }
    })
}

/// GraphDistanceMatrix[g]
pub fn builtin_graph_distance_matrix(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("GraphDistanceMatrix", args));
    };
    let rows = all_pairs(&g)
        .into_iter()
        .map(|row| {
            Value::List(
                row.into_iter()
                    .map(|d| if d < 0 { infinity() } else { int(d as usize) })
                    .collect(),
            )
        })
        .collect();
    Ok(Value::List(rows))
}

/// GraphDiameter[g]
pub fn builtin_graph_diameter(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("GraphDiameter", args));
    };
    let eccs = eccentricities(&g);
    if eccs.iter().any(|e| e.is_none()) {
        return Ok(infinity());
    }
    Ok(int(eccs.into_iter().flatten().max().unwrap_or(0)))
}

/// GraphRadius[g]
pub fn builtin_graph_radius(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("GraphRadius", args));
    };
    let eccs = eccentricities(&g);
    if eccs.iter().any(|e| e.is_none()) {
        return Ok(infinity());
    }
    Ok(int(eccs.into_iter().flatten().min().unwrap_or(0)))
}

/// Vertices attaining the extreme eccentricity (`extreme` = min for center, max for periphery).
fn extreme_vertices(name: &str, args: &[Value], want_max: bool) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated(name, args));
    };
    let eccs = eccentricities(&g);
    if eccs.iter().any(|e| e.is_none()) {
        return Ok(Value::List(g.vertices.clone()));
    }
    let target = if want_max {
        eccs.iter().flatten().copied().max()
    } else {
        eccs.iter().flatten().copied().min()
    };
    let out = eccs
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_some() && e.unwrap() == target.unwrap_or(0))
        .map(|(i, _)| g.vertices[i].clone())
        .collect();
    Ok(Value::List(out))
}

/// GraphCenter[g]
pub fn builtin_graph_center(args: &[Value]) -> Result<Value, EvalError> {
    extreme_vertices("GraphCenter", args, false)
}

/// GraphPeriphery[g]
pub fn builtin_graph_periphery(args: &[Value]) -> Result<Value, EvalError> {
    extreme_vertices("GraphPeriphery", args, true)
}

/// VertexIndex[g, v]
pub fn builtin_vertex_index(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("VertexIndex", args));
    }
    let Some(g) = as_graph(&args[0]) else {
        return Ok(unevaluated("VertexIndex", args));
    };
    match g.index_of(&args[1]) {
        Some(i) => Ok(int(i + 1)),
        None => Ok(infinity()),
    }
}

/// EdgeIndex[g, e]
pub fn builtin_edge_index(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("EdgeIndex", args));
    }
    let Some(g) = as_graph(&args[0]) else {
        return Ok(unevaluated("EdgeIndex", args));
    };
    match g.edges.iter().position(|e| edge_matches(&g, e, &args[1])) {
        Some(i) => Ok(int(i + 1)),
        None => Ok(infinity()),
    }
}

/// IncidenceMatrix[g]
pub fn builtin_incidence_matrix(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("IncidenceMatrix", args));
    };
    let n = g.vertices.len();
    let mut m = vec![vec![0i64; g.edges.len()]; n];
    for (col, e) in g.edges.iter().enumerate() {
        if e.directed {
            m[e.a][col] -= 1;
            m[e.b][col] += 1;
        } else if e.a == e.b {
            m[e.a][col] += 2;
        } else {
            m[e.a][col] += 1;
            m[e.b][col] += 1;
        }
    }
    let rows = m
        .into_iter()
        .map(|row| {
            Value::List(
                row.into_iter()
                    .map(|x| Value::Integer(Integer::from(x)))
                    .collect(),
            )
        })
        .collect();
    Ok(Value::List(rows))
}

/// Subgraph[g, {v1, ...}]
pub fn builtin_subgraph(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("Subgraph", args));
    }
    let Some(g) = as_graph(&args[0]) else {
        return Ok(unevaluated("Subgraph", args));
    };
    let requested = vertex_spec(&args[1]);
    let mut kept = Vec::new();
    for v in requested {
        if let Some(i) = g.index_of(&v)
            && !kept.contains(&i)
        {
            kept.push(i);
        }
    }
    let set: std::collections::HashSet<usize> = kept.iter().copied().collect();
    let vertices = kept.iter().map(|&i| g.vertices[i].clone()).collect();
    let edges = g
        .edges
        .iter()
        .filter(|e| set.contains(&e.a) && set.contains(&e.b))
        .map(|e| edge_value(e, &g.vertices))
        .collect();
    Ok(Value::Call {
        head: "Graph".to_string(),
        args: vec![Value::List(vertices), Value::List(edges)],
    })
}

/// VertexDelete[g, v] / VertexDelete[g, {v1, ...}]
pub fn builtin_vertex_delete(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("VertexDelete", args));
    }
    let Some(g) = as_graph(&args[0]) else {
        return Ok(unevaluated("VertexDelete", args));
    };
    let removed: Vec<Value> = vertex_spec(&args[1]);
    let keep: Vec<usize> = (0..g.vertices.len())
        .filter(|i| !removed.contains(&g.vertices[*i]))
        .collect();
    let set: std::collections::HashSet<usize> = keep.iter().copied().collect();
    let vertices = keep.iter().map(|&i| g.vertices[i].clone()).collect();
    let edges = g
        .edges
        .iter()
        .filter(|e| set.contains(&e.a) && set.contains(&e.b))
        .map(|e| edge_value(e, &g.vertices))
        .collect();
    Ok(Value::Call {
        head: "Graph".to_string(),
        args: vec![Value::List(vertices), Value::List(edges)],
    })
}

/// VertexAdd[g, v] / VertexAdd[g, {v1, ...}]
pub fn builtin_vertex_add(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("VertexAdd", args));
    }
    let Some(mut g) = as_graph(&args[0]) else {
        return Ok(unevaluated("VertexAdd", args));
    };
    for v in vertex_spec(&args[1]) {
        g.intern(v);
    }
    Ok(graph_value(&g))
}

/// EdgeDelete[g, e] / EdgeDelete[g, {e1, ...}]
pub fn builtin_edge_delete(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("EdgeDelete", args));
    }
    let Some(g) = as_graph(&args[0]) else {
        return Ok(unevaluated("EdgeDelete", args));
    };
    let specs: Vec<Value> = match &args[1] {
        Value::List(items) => items.clone(),
        other => vec![other.clone()],
    };
    let kept: Vec<&Edge> = g
        .edges
        .iter()
        .filter(|e| !specs.iter().any(|s| edge_matches(&g, e, s)))
        .collect();
    // Vertex set stays; rebuild graph with remaining edges.
    let mut out = Graph::new();
    for v in &g.vertices {
        out.intern(v.clone());
    }
    for e in kept {
        out.edges.push(Edge {
            a: e.a,
            b: e.b,
            directed: e.directed,
        });
    }
    Ok(graph_value(&out))
}

/// EdgeAdd[g, e] / EdgeAdd[g, {e1, ...}]
pub fn builtin_edge_add(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("EdgeAdd", args));
    }
    let Some(mut g) = as_graph(&args[0]) else {
        return Ok(unevaluated("EdgeAdd", args));
    };
    let specs: Vec<Value> = match &args[1] {
        Value::List(items) => items.clone(),
        other => vec![other.clone()],
    };
    for s in &specs {
        if add_edge_expr(&mut g, s).is_none() {
            return Ok(unevaluated("EdgeAdd", args));
        }
    }
    Ok(graph_value(&g))
}

/// VertexContract[g, {v1, ...}]
pub fn builtin_vertex_contract(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("VertexContract", args));
    }
    let Some(g) = as_graph(&args[0]) else {
        return Ok(unevaluated("VertexContract", args));
    };
    let set: Vec<Value> = vertex_spec(&args[1]);
    let indices: Vec<usize> = set.iter().filter_map(|v| g.index_of(v)).collect();
    if indices.len() < 2 {
        return Ok(graph_value(&g));
    }
    let rep = indices[0];
    let repval = g.vertices[rep].clone();
    let mut out = Graph::new();
    for (i, v) in g.vertices.iter().enumerate() {
        if i != rep && indices.contains(&i) {
            continue;
        }
        out.intern(v.clone());
    }
    for e in &g.edges {
        let a = if indices.contains(&e.a) {
            repval.clone()
        } else {
            g.vertices[e.a].clone()
        };
        let b = if indices.contains(&e.b) {
            repval.clone()
        } else {
            g.vertices[e.b].clone()
        };
        if a == b {
            continue;
        }
        let ia = out.intern(a);
        let ib = out.intern(b);
        out.edges.push(Edge {
            a: ia,
            b: ib,
            directed: e.directed,
        });
    }
    Ok(graph_value(&out))
}

/// Extract `lhs -> rhs` rules from a rule or list of rules.
fn rules_from(v: &Value) -> Option<Vec<(Value, Value)>> {
    match v {
        Value::Rule { lhs, rhs, .. } => Some(vec![(to_vertex(lhs), to_vertex(rhs))]),
        Value::List(items) => {
            let mut out = Vec::new();
            for it in items {
                if let Value::Rule { lhs, rhs, .. } = it {
                    out.push((to_vertex(lhs), to_vertex(rhs)));
                } else {
                    return None;
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// VertexReplace[g, v -> w] / VertexReplace[g, {rules}]
pub fn builtin_vertex_replace(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("VertexReplace", args));
    }
    let Some(g) = as_graph(&args[0]) else {
        return Ok(unevaluated("VertexReplace", args));
    };
    let Some(rules) = rules_from(&args[1]) else {
        return Ok(unevaluated("VertexReplace", args));
    };
    let map = |v: &Value| -> Value {
        for (a, b) in &rules {
            if *v == to_vertex(a) {
                return b.clone();
            }
        }
        v.clone()
    };
    let mut out = Graph::new();
    for v in &g.vertices {
        out.intern(map(v));
    }
    for e in &g.edges {
        let ia = out.intern(map(&g.vertices[e.a]));
        let ib = out.intern(map(&g.vertices[e.b]));
        out.edges.push(Edge {
            a: ia,
            b: ib,
            directed: e.directed,
        });
    }
    Ok(graph_value(&out))
}

/// NeighborhoodVertices[g, v]
pub fn builtin_neighborhood_vertices(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("NeighborhoodVertices", args));
    }
    let Some(g) = as_graph(&args[0]) else {
        return Ok(unevaluated("NeighborhoodVertices", args));
    };
    let v = to_vertex(&args[1]);
    let Some(i) = g.index_of(&v) else {
        return Ok(unevaluated("NeighborhoodVertices", args));
    };
    let adj = g.out_neighbors();
    let mut seen = vec![false; g.vertices.len()];
    seen[i] = true;
    let mut out = vec![g.vertices[i].clone()];
    for &w in &adj[i] {
        if !seen[w] {
            seen[w] = true;
            out.push(g.vertices[w].clone());
        }
    }
    Ok(Value::List(out))
}

/// GraphComplement[g]
pub fn builtin_graph_complement(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("GraphComplement", args));
    };
    let n = g.vertices.len();
    let directed = g.is_directed();
    let mut present = vec![vec![false; n]; n];
    for e in &g.edges {
        present[e.a][e.b] = true;
        if !e.directed {
            present[e.b][e.a] = true;
        }
    }
    let mut edges = Vec::new();
    for (a, row) in present.iter().enumerate() {
        for (b, present_ab) in row.iter().enumerate() {
            if a == b || *present_ab {
                continue;
            }
            if directed {
                edges.push(Edge {
                    a,
                    b,
                    directed: true,
                });
            } else if a < b {
                edges.push(Edge {
                    a,
                    b,
                    directed: false,
                });
            }
        }
    }
    Ok(graph_value(&Graph {
        vertices: g.vertices.clone(),
        edges,
    }))
}

/// Collect the graph-typed leading arguments.
fn graphs_from(args: &[Value]) -> Option<Vec<Graph>> {
    let mut out = Vec::new();
    for a in args {
        out.push(as_graph(a)?);
    }
    Some(out)
}

/// GraphUnion[g1, g2, ...]
pub fn builtin_graph_union(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(unevaluated("GraphUnion", args));
    }
    let Some(gs) = graphs_from(args) else {
        return Ok(unevaluated("GraphUnion", args));
    };
    let mut out = Graph::new();
    for g in &gs {
        for v in &g.vertices {
            out.intern(v.clone());
        }
        for e in &g.edges {
            let ia = out.intern(g.vertices[e.a].clone());
            let ib = out.intern(g.vertices[e.b].clone());
            out.edges.push(Edge {
                a: ia,
                b: ib,
                directed: e.directed,
            });
        }
    }
    Ok(graph_value(&out))
}

/// GraphIntersection[g1, g2, ...]
pub fn builtin_graph_intersection(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(unevaluated("GraphIntersection", args));
    }
    let Some(gs) = graphs_from(args) else {
        return Ok(unevaluated("GraphIntersection", args));
    };
    let common: Vec<Value> = gs[0]
        .vertices
        .iter()
        .filter(|v| gs.iter().all(|g| g.vertices.contains(v)))
        .cloned()
        .collect();
    let mut out = Graph::new();
    for v in &common {
        out.intern(v.clone());
    }
    for e in &gs[0].edges {
        let va = &gs[0].vertices[e.a];
        let vb = &gs[0].vertices[e.b];
        if !common.contains(va) || !common.contains(vb) {
            continue;
        }
        let in_all = gs[1..].iter().all(|g| {
            g.edges
                .iter()
                .any(|oe| edge_matches(g, oe, &edge_value(e, &gs[0].vertices)))
        });
        if in_all {
            let ia = out.intern(va.clone());
            let ib = out.intern(vb.clone());
            out.edges.push(Edge {
                a: ia,
                b: ib,
                directed: e.directed,
            });
        }
    }
    Ok(graph_value(&out))
}

/// GraphDifference[g1, g2]
pub fn builtin_graph_difference(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("GraphDifference", args));
    }
    let (Some(g1), Some(g2)) = (as_graph(&args[0]), as_graph(&args[1])) else {
        return Ok(unevaluated("GraphDifference", args));
    };
    let mut used = vec![false; g2.edges.len()];
    let mut keep: Vec<usize> = Vec::new();
    for (i, e) in g1.edges.iter().enumerate() {
        let spec = edge_value(e, &g1.vertices);
        match g2
            .edges
            .iter()
            .enumerate()
            .position(|(j, oe)| !used[j] && edge_matches(&g2, oe, &spec))
        {
            Some(j) => used[j] = true,
            None => keep.push(i),
        }
    }
    let mut out = Graph::new();
    for v in &g1.vertices {
        out.intern(v.clone());
    }
    for &i in &keep {
        let e = &g1.edges[i];
        out.edges.push(Edge {
            a: e.a,
            b: e.b,
            directed: e.directed,
        });
    }
    Ok(graph_value(&out))
}

/// GraphDisjointUnion[g1, g2, ...] — relabel vertices to consecutive integers.
pub fn builtin_graph_disjoint_union(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(unevaluated("GraphDisjointUnion", args));
    }
    let Some(gs) = graphs_from(args) else {
        return Ok(unevaluated("GraphDisjointUnion", args));
    };
    let mut out = Graph::new();
    for g in &gs {
        let mut mapping = Vec::with_capacity(g.vertices.len());
        for _ in &g.vertices {
            mapping.push(out.intern(int(out.vertices.len() + 1)));
        }
        for e in &g.edges {
            out.edges.push(Edge {
                a: mapping[e.a],
                b: mapping[e.b],
                directed: e.directed,
            });
        }
    }
    Ok(graph_value(&out))
}

/// FindCycle[g] — a single cycle as a list of vertices, or `{}` when acyclic.
pub fn builtin_find_cycle(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("FindCycle", args));
    };
    let n = g.vertices.len();
    let mut adj = vec![Vec::new(); n];
    for (id, e) in g.edges.iter().enumerate() {
        adj[e.a].push((e.b, id));
        if !e.directed {
            adj[e.b].push((e.a, id));
        }
    }
    let mut visited = vec![false; n];
    let mut parent = vec![usize::MAX; n];
    let mut parent_edge = vec![usize::MAX; n];
    for s in 0..n {
        if visited[s] {
            continue;
        }
        visited[s] = true;
        let mut stack = vec![(s, 0usize)];
        while let Some(&(v, idx)) = stack.last() {
            if idx < adj[v].len() {
                stack.last_mut().unwrap().1 += 1;
                let (w, eid) = adj[v][idx];
                if !visited[w] {
                    visited[w] = true;
                    parent[w] = v;
                    parent_edge[w] = eid;
                    stack.push((w, 0));
                } else if eid != parent_edge[v] {
                    let mut cyc = vec![v];
                    let mut cur = v;
                    while cur != w && cur != usize::MAX {
                        cur = parent[cur];
                        cyc.push(cur);
                    }
                    if cur == w {
                        cyc.reverse();
                        return Ok(Value::List(
                            cyc.into_iter().map(|i| g.vertices[i].clone()).collect(),
                        ));
                    }
                }
            } else {
                stack.pop();
            }
        }
    }
    Ok(Value::List(vec![]))
}

/// ConnectedGraphQ[g]
pub fn builtin_connected_graph_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("ConnectedGraphQ", args));
    };
    let n = g.vertices.len();
    if n == 0 {
        return Ok(Value::Bool(true));
    }
    let adj = g.weak_neighbors();
    let mut seen = vec![false; n];
    let mut queue = vec![0usize];
    seen[0] = true;
    while let Some(u) = queue.pop() {
        for &w in &adj[u] {
            if !seen[w] {
                seen[w] = true;
                queue.push(w);
            }
        }
    }
    Ok(Value::Bool(seen.iter().all(|b| *b)))
}

/// A BFS spanning tree (forest) over the weak adjacency of `g`.
fn spanning_tree(g: &Graph) -> Graph {
    let n = g.vertices.len();
    let adj = g.weak_neighbors();
    let mut seen = vec![false; n];
    let mut tree_edges = Vec::new();
    for s in 0..n {
        if seen[s] {
            continue;
        }
        seen[s] = true;
        let mut queue = vec![s];
        while let Some(u) = queue.pop() {
            for &w in &adj[u] {
                if !seen[w] {
                    seen[w] = true;
                    tree_edges.push((u, w));
                    queue.push(w);
                }
            }
        }
    }
    let mut out = Graph::new();
    for v in &g.vertices {
        out.intern(v.clone());
    }
    for (a, b) in tree_edges {
        out.edges.push(Edge {
            a,
            b,
            directed: false,
        });
    }
    out
}

/// FindSpanningTree[g] — a BFS spanning tree (forest) over the weak adjacency.
pub fn builtin_find_spanning_tree(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("FindSpanningTree", args));
    };
    Ok(graph_value(&spanning_tree(&g)))
}

/// KruskalTree[g] — alias of `FindSpanningTree`.
pub fn builtin_kruskal_tree(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("KruskalTree", args));
    };
    Ok(graph_value(&spanning_tree(&g)))
}

/// Union-find for Kruskal's algorithm.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) -> bool {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return false;
        }
        if self.rank[ra] < self.rank[rb] {
            self.parent[ra] = rb;
        } else if self.rank[ra] > self.rank[rb] {
            self.parent[rb] = ra;
        } else {
            self.parent[rb] = ra;
            self.rank[ra] += 1;
        }
        true
    }
}

/// Kruskal[g] — a minimum spanning forest as a list of edges.
pub fn builtin_kruskal(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("Kruskal", args));
    };
    let mut uf = UnionFind::new(g.vertices.len());
    let mut out = Vec::new();
    for e in &g.edges {
        if uf.union(e.a, e.b) {
            out.push(edge_value(e, &g.vertices));
        }
    }
    Ok(Value::List(out))
}

/// Adjacency (neighbour sets, self excluded) for the clique routines.
fn neighbor_sets(g: &Graph) -> Vec<Vec<usize>> {
    g.weak_neighbors()
        .into_iter()
        .enumerate()
        .map(|(i, mut ns)| {
            ns.retain(|&w| w != i);
            ns.sort_unstable();
            ns.dedup();
            ns
        })
        .collect()
}

fn clique_expand(
    adj: &[Vec<usize>],
    r: &mut Vec<usize>,
    p: &mut Vec<usize>,
    x: &mut Vec<usize>,
    best: &mut Vec<usize>,
) {
    if p.is_empty() && x.is_empty() {
        if r.len() > best.len() {
            *best = r.clone();
        }
        return;
    }
    let pv = p.clone();
    for v in pv {
        let mut np: Vec<usize> = p
            .iter()
            .copied()
            .filter(|u| *u != v && adj[v].contains(u))
            .collect();
        let mut nx: Vec<usize> = x.iter().copied().filter(|u| adj[v].contains(u)).collect();
        r.push(v);
        clique_expand(adj, r, &mut np, &mut nx, best);
        r.pop();
        p.retain(|u| *u != v);
        if !x.contains(&v) {
            x.push(v);
        }
    }
}

/// FindClique[g] — a largest clique as a list of vertices.
pub fn builtin_find_clique(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("FindClique", args));
    };
    let adj = neighbor_sets(&g);
    let mut best = Vec::new();
    let mut r = Vec::new();
    let mut p: Vec<usize> = (0..adj.len()).collect();
    let mut x = Vec::new();
    clique_expand(&adj, &mut r, &mut p, &mut x, &mut best);
    best.sort_unstable();
    Ok(Value::List(
        best.into_iter().map(|i| g.vertices[i].clone()).collect(),
    ))
}

/// FindKClique[g, k] — a clique of size `k`.
pub fn builtin_find_k_clique(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("FindKClique", args));
    }
    let Some(g) = as_graph(&args[0]) else {
        return Ok(unevaluated("FindKClique", args));
    };
    let Some(k) = args[1].to_integer().filter(|k| *k >= 0) else {
        return Ok(unevaluated("FindKClique", args));
    };
    let k = k as usize;
    let adj = neighbor_sets(&g);

    fn rec(adj: &[Vec<usize>], k: usize, start: usize, cur: &mut Vec<usize>) -> bool {
        if cur.len() == k {
            return true;
        }
        for v in start..adj.len() {
            if cur.iter().all(|u| adj[v].contains(u)) {
                cur.push(v);
                if rec(adj, k, v + 1, cur) {
                    return true;
                }
                cur.pop();
            }
        }
        false
    }

    let mut cur = Vec::new();
    if rec(&adj, k, 0, &mut cur) {
        Ok(Value::List(
            cur.into_iter().map(|i| g.vertices[i].clone()).collect(),
        ))
    } else {
        Ok(Value::List(vec![]))
    }
}

fn hp_dfs(adj: &[Vec<usize>], v: usize, path: &mut Vec<usize>, used: &mut [bool]) -> bool {
    if path.len() == adj.len() {
        return true;
    }
    for &w in &adj[v] {
        if !used[w] {
            used[w] = true;
            path.push(w);
            if hp_dfs(adj, w, path, used) {
                return true;
            }
            path.pop();
            used[w] = false;
        }
    }
    false
}

/// HamiltonianPath[g] — a Hamiltonian path as a list of vertices.
pub fn builtin_hamiltonian_path(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("HamiltonianPath", args));
    };
    let adj = neighbor_sets(&g);
    let n = adj.len();
    if n == 0 {
        return Ok(Value::List(vec![]));
    }
    for s in 0..n {
        let mut used = vec![false; n];
        used[s] = true;
        let mut path = vec![s];
        if hp_dfs(&adj, s, &mut path, &mut used) {
            return Ok(Value::List(
                path.into_iter().map(|i| g.vertices[i].clone()).collect(),
            ));
        }
    }
    Ok(Value::List(vec![]))
}

/// HamiltonianCycle[g] / FindHamiltonianCycle[g] — a closed cycle of vertices.
pub fn builtin_hamiltonian_cycle(args: &[Value]) -> Result<Value, EvalError> {
    let name = "HamiltonianCycle";
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated(name, args));
    };
    let adj = neighbor_sets(&g);
    let n = adj.len();
    if n == 0 {
        return Ok(Value::List(vec![]));
    }
    for s in 0..n {
        let mut used = vec![false; n];
        used[s] = true;
        let mut path = vec![s];
        if hc_dfs(&adj, s, s, &mut path, &mut used) {
            path.push(s);
            return Ok(Value::List(
                path.into_iter().map(|i| g.vertices[i].clone()).collect(),
            ));
        }
    }
    Ok(Value::List(vec![]))
}

fn hc_dfs(
    adj: &[Vec<usize>],
    start: usize,
    v: usize,
    path: &mut Vec<usize>,
    used: &mut [bool],
) -> bool {
    if path.len() == adj.len() {
        return adj[v].contains(&start);
    }
    for &w in &adj[v] {
        if !used[w] {
            used[w] = true;
            path.push(w);
            if hc_dfs(adj, start, w, path, used) {
                return true;
            }
            path.pop();
            used[w] = false;
        }
    }
    false
}

/// FindHamiltonianCycle[g]
pub fn builtin_find_hamiltonian_cycle(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("FindHamiltonianCycle", args));
    };
    let adj = neighbor_sets(&g);
    let n = adj.len();
    if n == 0 {
        return Ok(Value::List(vec![]));
    }
    for s in 0..n {
        let mut used = vec![false; n];
        used[s] = true;
        let mut path = vec![s];
        if hc_dfs(&adj, s, s, &mut path, &mut used) {
            path.push(s);
            return Ok(Value::List(
                path.into_iter().map(|i| g.vertices[i].clone()).collect(),
            ));
        }
    }
    Ok(Value::List(vec![]))
}

/// EulerianGraphQ[g]
pub fn builtin_eulerian_graph_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("EulerianGraphQ", args));
    };
    if g.edges.is_empty() {
        return Ok(Value::Bool(true));
    }
    let n = g.vertices.len();
    let adj = g.weak_neighbors();
    let start = (0..n).find(|&i| !adj[i].is_empty()).unwrap();
    let mut seen = vec![false; n];
    let mut queue = vec![start];
    seen[start] = true;
    while let Some(u) = queue.pop() {
        for &w in &adj[u] {
            if !seen[w] {
                seen[w] = true;
                queue.push(w);
            }
        }
    }
    for i in 0..n {
        if !adj[i].is_empty() && !seen[i] {
            return Ok(Value::Bool(false));
        }
    }
    let deg = degrees(&g, "Total");
    if g.is_directed() {
        let inc = degrees(&g, "In");
        let out = degrees(&g, "Out");
        Ok(Value::Bool((0..n).all(|i| inc[i] == out[i])))
    } else {
        Ok(Value::Bool(deg.iter().all(|d| d % 2 == 0)))
    }
}

/// FindEulerianCycle[g] — an Eulerian circuit as a closed list of vertices, or `{}`.
pub fn builtin_find_eulerian_cycle(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("FindEulerianCycle", args));
    };
    let n = g.vertices.len();
    let mut adj = vec![Vec::new(); n];
    for (id, e) in g.edges.iter().enumerate() {
        adj[e.a].push((e.b, id));
        if !e.directed {
            adj[e.b].push((e.a, id));
        }
    }
    let start = match (0..n).find(|&i| !adj[i].is_empty()) {
        Some(i) => i,
        None => return Ok(Value::List(vec![])),
    };
    let mut used = vec![false; g.edges.len()];
    let mut stack = vec![start];
    let mut circuit = Vec::new();
    while let Some(&v) = stack.last() {
        if let Some(pos) = adj[v].iter().position(|&(_, id)| !used[id]) {
            let (w, id) = adj[v][pos];
            used[id] = true;
            stack.push(w);
        } else {
            circuit.push(v);
            stack.pop();
        }
    }
    if !used.iter().all(|b| *b) {
        return Ok(Value::List(vec![]));
    }
    circuit.reverse();
    if circuit.first() != circuit.last() {
        return Ok(Value::List(vec![]));
    }
    Ok(Value::List(
        circuit.into_iter().map(|i| g.vertices[i].clone()).collect(),
    ))
}

// ── Centralities ────────────────────────────────────────────────────────────

/// DegreeCentrality[g]
pub fn builtin_degree_centrality(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("DegreeCentrality", args));
    };
    let n = g.vertices.len();
    let deg = degrees(&g, "Total");
    let denom = (n as f64 - 1.0).max(1.0);
    Ok(Value::List(
        deg.into_iter().map(|d| real(d as f64 / denom)).collect(),
    ))
}

/// ClosenessCentrality[g]
pub fn builtin_closeness_centrality(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("ClosenessCentrality", args));
    };
    let rows = all_pairs(&g);
    let out = rows
        .into_iter()
        .map(|row| {
            let mut sum = 0usize;
            let mut reach = 0usize;
            for d in row {
                if d > 0 {
                    sum += d as usize;
                    reach += 1;
                }
            }
            if sum == 0 {
                real(0.0)
            } else {
                real(reach as f64 / sum as f64)
            }
        })
        .collect();
    Ok(Value::List(out))
}

/// BetweennessCentrality[g] — Brandes' algorithm.
pub fn builtin_betweenness_centrality(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("BetweennessCentrality", args));
    };
    let n = g.vertices.len();
    let adj = g.out_neighbors();
    let mut cb = vec![0.0f64; n];
    for s in 0..n {
        let mut stack = Vec::new();
        let mut pred = vec![Vec::new(); n];
        let mut sigma = vec![0.0f64; n];
        sigma[s] = 1.0;
        let mut dist = vec![-1i64; n];
        dist[s] = 0;
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(s);
        while let Some(v) = queue.pop_front() {
            stack.push(v);
            for &w in &adj[v] {
                if dist[w] < 0 {
                    dist[w] = dist[v] + 1;
                    queue.push_back(w);
                }
                if dist[w] == dist[v] + 1 {
                    sigma[w] += sigma[v];
                    pred[w].push(v);
                }
            }
        }
        let mut delta = vec![0.0f64; n];
        while let Some(w) = stack.pop() {
            for &v in &pred[w] {
                delta[v] += sigma[v] / sigma[w] * (1.0 + delta[w]);
            }
            if w != s {
                cb[w] += delta[w];
            }
        }
    }
    if !g.is_directed() {
        for x in cb.iter_mut() {
            *x /= 2.0;
        }
    }
    Ok(Value::List(cb.into_iter().map(real).collect()))
}

/// EigenvectorCentrality[g] — power iteration on the adjacency matrix.
pub fn builtin_eigenvector_centrality(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("EigenvectorCentrality", args));
    };
    let n = g.vertices.len();
    let adj = g.out_neighbors();
    let mut v = vec![1.0 / (n.max(1) as f64); n];
    for _ in 0..200 {
        let mut w = vec![0.0f64; n];
        for i in 0..n {
            for &j in &adj[i] {
                w[j] += v[i];
            }
        }
        let norm = w.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm <= 0.0 {
            break;
        }
        for x in w.iter_mut() {
            *x /= norm;
        }
        v = w;
    }
    Ok(Value::List(v.into_iter().map(real).collect()))
}

/// PageRankCentrality[g] — PageRank with damping 0.85.
pub fn builtin_pagerank_centrality(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("PageRankCentrality", args));
    };
    let n = g.vertices.len();
    if n == 0 {
        return Ok(Value::List(vec![]));
    }
    let adj = g.out_neighbors();
    let d = 0.85f64;
    let mut pr = vec![1.0 / n as f64; n];
    for _ in 0..100 {
        let mut dangling = 0.0;
        for i in 0..n {
            if adj[i].is_empty() {
                dangling += pr[i];
            }
        }
        let mut np = vec![(1.0 - d) / n as f64; n];
        let add = d * dangling / n as f64;
        for x in np.iter_mut() {
            *x += add;
        }
        for i in 0..n {
            if !adj[i].is_empty() {
                let share = d * pr[i] / adj[i].len() as f64;
                for &j in &adj[i] {
                    np[j] += share;
                }
            }
        }
        pr = np;
    }
    Ok(Value::List(pr.into_iter().map(real).collect()))
}

/// Local clustering coefficient of every vertex (0 for fewer than two neighbours).
fn local_clustering(g: &Graph) -> Vec<f64> {
    let adj = neighbor_sets(g);
    (0..g.vertices.len())
        .map(|i| {
            let ns = &adj[i];
            let k = ns.len();
            if k < 2 {
                return 0.0;
            }
            let mut links = 0usize;
            for a in 0..k {
                for b in a + 1..k {
                    if adj[ns[a]].contains(&ns[b]) {
                        links += 1;
                    }
                }
            }
            2.0 * links as f64 / (k * (k - 1)) as f64
        })
        .collect()
}

/// Mean of the local clustering coefficients.
fn mean_clustering(g: &Graph) -> f64 {
    let n = g.vertices.len();
    if n == 0 {
        return 0.0;
    }
    local_clustering(g).iter().sum::<f64>() / n as f64
}

/// LocalClusteringCoefficient[g] / LocalClusteringCoefficient[g, v]
pub fn builtin_local_clustering_coefficient(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("LocalClusteringCoefficient", args));
    };
    let coeff = local_clustering(&g);
    if args.len() >= 2
        && let Some(i) = g.index_of(&args[1])
    {
        return Ok(real(coeff[i]));
    }
    Ok(Value::List(coeff.into_iter().map(real).collect()))
}

/// GlobalClusteringCoefficient[g] — mean local clustering coefficient.
pub fn builtin_global_clustering_coefficient(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("GlobalClusteringCoefficient", args));
    };
    Ok(real(mean_clustering(&g)))
}

/// ClusteringCoefficient[g] — average local clustering coefficient.
pub fn builtin_clustering_coefficient(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("ClusteringCoefficient", args));
    };
    Ok(real(mean_clustering(&g)))
}

/// GraphAssortativity[g] — degree assortativity (Pearson correlation).
pub fn builtin_graph_assortativity(args: &[Value]) -> Result<Value, EvalError> {
    let Some(g) = args.first().and_then(as_graph) else {
        return Ok(unevaluated("GraphAssortativity", args));
    };
    let deg = degrees(&g, "Total");
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for e in &g.edges {
        xs.push(deg[e.a] as f64);
        ys.push(deg[e.b] as f64);
        if !e.directed {
            xs.push(deg[e.b] as f64);
            ys.push(deg[e.a] as f64);
        }
    }
    let m = xs.len();
    if m == 0 {
        return Ok(real(0.0));
    }
    let mf = m as f64;
    let mx = xs.iter().sum::<f64>() / mf;
    let my = ys.iter().sum::<f64>() / mf;
    let mut cov = 0.0;
    let mut vx = 0.0;
    let mut vy = 0.0;
    for i in 0..m {
        cov += (xs[i] - mx) * (ys[i] - my);
        vx += (xs[i] - mx).powi(2);
        vy += (ys[i] - my).powi(2);
    }
    if vx <= 0.0 || vy <= 0.0 {
        return Ok(real(0.0));
    }
    Ok(real(cov / (vx * vy).sqrt()))
}

/// RandomGraph[{n, m}] / RandomGraph[{n, p}] — Erdős–Rényi, seeded via the shared RNG.
pub fn builtin_random_graph(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("RandomGraph", args));
    }
    let Value::List(spec) = &args[0] else {
        return Ok(unevaluated("RandomGraph", args));
    };
    if spec.len() != 2 {
        return Ok(unevaluated("RandomGraph", args));
    }
    let Some(n) = spec[0].to_integer().filter(|n| *n >= 0) else {
        return Ok(unevaluated("RandomGraph", args));
    };
    let n = n as usize;
    let mut g = Graph::new();
    for i in 1..=n {
        g.intern(int(i));
    }
    let pairs: Vec<(usize, usize)> = (0..n)
        .flat_map(|a| ((a + 1)..n).map(move |b| (a, b)))
        .collect();
    match &spec[1] {
        Value::Integer(mv) => {
            let Some(m) = mv.to_i64().filter(|m| *m >= 0) else {
                return Ok(unevaluated("RandomGraph", args));
            };
            let m = m as usize;
            if m > pairs.len() {
                return Ok(unevaluated("RandomGraph", args));
            }
            let mut idx: Vec<usize> = (0..pairs.len()).collect();
            for i in 0..m {
                let j = i + (super::random::next_u64() as usize) % (pairs.len() - i);
                idx.swap(i, j);
            }
            idx.truncate(m);
            idx.sort_unstable();
            for i in idx {
                let (a, b) = pairs[i];
                g.edges.push(Edge {
                    a,
                    b,
                    directed: false,
                });
            }
        }
        Value::Real(_) | Value::Rational(_) => {
            let p = spec[1].to_real().unwrap_or(0.0);
            for &(a, b) in &pairs {
                let r = (super::random::next_u64() as f64) / (u64::MAX as f64);
                if r < p {
                    g.edges.push(Edge {
                        a,
                        b,
                        directed: false,
                    });
                }
            }
        }
        _ => return Ok(unevaluated("RandomGraph", args)),
    }
    Ok(graph_value(&g))
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    use super::register_builtin;
    register_builtin(env, "Graph", builtin_graph);
    register_builtin(env, "GraphQ", builtin_graph_q);
    register_builtin(env, "VertexList", builtin_vertex_list);
    register_builtin(env, "EdgeList", builtin_edge_list);
    register_builtin(env, "VertexCount", builtin_vertex_count);
    register_builtin(env, "EdgeCount", builtin_edge_count);
    register_builtin(env, "AdjacencyMatrix", builtin_adjacency_matrix);
    register_builtin(env, "AdjacencyList", builtin_adjacency_list);
    register_builtin(env, "VertexDegree", builtin_vertex_degree);
    register_builtin(env, "DegreeList", builtin_degree_list);
    register_builtin(env, "ConnectedComponents", builtin_connected_components);
    register_builtin(env, "FindShortestPath", builtin_find_shortest_path);
    register_builtin(env, "GraphDistance", builtin_graph_distance);
    register_builtin(env, "NeighborhoodGraph", builtin_neighborhood_graph);
    register_builtin(env, "GraphPlot", builtin_graph_plot);

    register_builtin(env, "GraphDistanceMatrix", builtin_graph_distance_matrix);
    register_builtin(env, "GraphDiameter", builtin_graph_diameter);
    register_builtin(env, "GraphRadius", builtin_graph_radius);
    register_builtin(env, "GraphCenter", builtin_graph_center);
    register_builtin(env, "GraphPeriphery", builtin_graph_periphery);
    register_builtin(env, "VertexIndex", builtin_vertex_index);
    register_builtin(env, "EdgeIndex", builtin_edge_index);
    register_builtin(env, "IncidenceMatrix", builtin_incidence_matrix);
    register_builtin(env, "Subgraph", builtin_subgraph);
    register_builtin(env, "VertexDelete", builtin_vertex_delete);
    register_builtin(env, "VertexAdd", builtin_vertex_add);
    register_builtin(env, "EdgeDelete", builtin_edge_delete);
    register_builtin(env, "EdgeAdd", builtin_edge_add);
    register_builtin(env, "VertexContract", builtin_vertex_contract);
    register_builtin(env, "VertexReplace", builtin_vertex_replace);
    register_builtin(env, "NeighborhoodVertices", builtin_neighborhood_vertices);
    register_builtin(env, "GraphComplement", builtin_graph_complement);
    register_builtin(env, "GraphUnion", builtin_graph_union);
    register_builtin(env, "GraphIntersection", builtin_graph_intersection);
    register_builtin(env, "GraphDifference", builtin_graph_difference);
    register_builtin(env, "GraphDisjointUnion", builtin_graph_disjoint_union);
    register_builtin(env, "FindCycle", builtin_find_cycle);
    register_builtin(env, "ConnectedGraphQ", builtin_connected_graph_q);
    register_builtin(env, "FindSpanningTree", builtin_find_spanning_tree);
    register_builtin(env, "Kruskal", builtin_kruskal);
    register_builtin(env, "KruskalTree", builtin_kruskal_tree);
    register_builtin(env, "FindClique", builtin_find_clique);
    register_builtin(env, "FindKClique", builtin_find_k_clique);
    register_builtin(env, "HamiltonianPath", builtin_hamiltonian_path);
    register_builtin(env, "HamiltonianCycle", builtin_hamiltonian_cycle);
    register_builtin(env, "FindHamiltonianCycle", builtin_find_hamiltonian_cycle);
    register_builtin(env, "EulerianGraphQ", builtin_eulerian_graph_q);
    register_builtin(env, "FindEulerianCycle", builtin_find_eulerian_cycle);
    register_builtin(env, "DegreeCentrality", builtin_degree_centrality);
    register_builtin(env, "ClosenessCentrality", builtin_closeness_centrality);
    register_builtin(env, "BetweennessCentrality", builtin_betweenness_centrality);
    register_builtin(env, "EigenvectorCentrality", builtin_eigenvector_centrality);
    register_builtin(env, "PageRankCentrality", builtin_pagerank_centrality);
    register_builtin(
        env,
        "LocalClusteringCoefficient",
        builtin_local_clustering_coefficient,
    );
    register_builtin(
        env,
        "GlobalClusteringCoefficient",
        builtin_global_clustering_coefficient,
    );
    register_builtin(env, "ClusteringCoefficient", builtin_clustering_coefficient);
    register_builtin(env, "GraphAssortativity", builtin_graph_assortativity);
    register_builtin(env, "RandomGraph", builtin_random_graph);

    // `Degree` is also the angle constant π/180 (a `Value::Real` installed by the
    // core constant table). Because a value cannot simultaneously be a callable and
    // a number, register the graph implementation under a private head and route
    // `Degree[...]` to it through the operator table. `Degree` used bare stays the
    // constant, `Degree[g]` returns vertex degrees.
    register_builtin(env, "DegreeGraph", builtin_degree);
    env.register_operator(
        "Degree",
        OperatorInfo {
            head: "DegreeGraph".to_string(),
            precedence: 670,
            fixity: Fixity::Prefix,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Env;

    fn test_env() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    fn eval_str(input: &str) -> Value {
        let env = test_env();
        let tokens = crate::lexer::tokenize(input).unwrap();
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

    #[test]
    fn test_undirected_edge_parsing() {
        // `1 <-> 2` must parse to UndirectedEdge.
        let v = eval_str("1<->2");
        assert!(
            matches!(&v, Value::Call { head, .. } if head == "UndirectedEdge"),
            "got {v:?}"
        );
    }

    #[test]
    fn test_vertex_list_and_count() {
        assert_eq!(
            ints(&eval_str("VertexList[Graph[{1<->2, 2<->3, 4<->5}]]")),
            vec![1, 2, 3, 4, 5]
        );
        assert_eq!(eval_str("VertexCount[Graph[{1<->2, 2<->3}]]"), int(3));
        assert_eq!(eval_str("EdgeCount[Graph[{1<->2, 2<->3}]]"), int(2));
    }

    #[test]
    fn test_vertex_edges_arguments() {
        let v = eval_str("VertexList[Graph[{\"a\", \"b\"}, {\"a\"->\"b\"}]]");
        match v {
            Value::List(items) => {
                assert_eq!(items, vec![Value::Str("a".into()), Value::Str("b".into())]);
            }
            _ => panic!("expected list, got {v:?}"),
        }
    }

    #[test]
    fn test_edge_list_head_preserved() {
        let v = eval_str("EdgeList[Graph[{1<->2, 2->3}]]");
        match v {
            Value::List(items) => {
                assert!(matches!(&items[0], Value::Call { head, .. } if head == "UndirectedEdge"));
                assert!(matches!(&items[1], Value::Call { head, .. } if head == "DirectedEdge"));
            }
            _ => panic!("expected list, got {v:?}"),
        }
    }

    #[test]
    fn test_adjacency_matrix_undirected() {
        let m = eval_str("AdjacencyMatrix[Graph[{1<->2, 2<->3}]]");
        let rows: Vec<Vec<i64>> = match m {
            Value::List(rs) => rs.iter().map(ints).collect(),
            _ => panic!("expected matrix"),
        };
        assert_eq!(rows, vec![vec![0, 1, 0], vec![1, 0, 1], vec![0, 1, 0]]);
    }

    #[test]
    fn test_adjacency_matrix_self_loop() {
        let m = eval_str("AdjacencyMatrix[Graph[{1<->1}]]");
        let rows: Vec<Vec<i64>> = match m {
            Value::List(rs) => rs.iter().map(ints).collect(),
            _ => panic!("expected matrix"),
        };
        assert_eq!(rows, vec![vec![2]]);
    }

    #[test]
    fn test_adjacency_list_directed() {
        let al = eval_str("AdjacencyList[Graph[{1->2, 1->3, 2->3}]]");
        match al {
            Value::List(items) => {
                assert_eq!(ints(&items[0]), vec![2, 3]);
                assert_eq!(ints(&items[1]), vec![3]);
                assert_eq!(ints(&items[2]), Vec::<i64>::new());
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_vertex_degree() {
        assert_eq!(
            ints(&eval_str("VertexDegree[Graph[{1<->2, 2<->3, 3<->1}]]")),
            vec![2, 2, 2]
        );
        assert_eq!(
            eval_str("VertexDegree[Graph[{1<->2, 2<->3, 3<->1}], 2]"),
            int(2)
        );
        // self-loop counts twice
        assert_eq!(ints(&eval_str("VertexDegree[Graph[{1<->1}]]")), vec![2]);
    }

    #[test]
    fn test_connected_components() {
        let cc = eval_str("ConnectedComponents[Graph[{1<->2, 3<->4, 5<->5}]]");
        match cc {
            Value::List(comps) => {
                assert_eq!(comps.len(), 3);
                assert_eq!(ints(&comps[0]), vec![1, 2]);
                assert_eq!(ints(&comps[1]), vec![3, 4]);
                assert_eq!(ints(&comps[2]), vec![5]);
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_find_shortest_path() {
        assert_eq!(
            ints(&eval_str(
                "FindShortestPath[Graph[{1<->2, 2<->3, 1<->3}], 1, 3]"
            )),
            vec![1, 3]
        );
        assert_eq!(
            ints(&eval_str("FindShortestPath[Graph[{1<->2, 2<->3}], 1, 3]")),
            vec![1, 2, 3]
        );
        assert_eq!(
            eval_str("FindShortestPath[Graph[{1<->2, 3<->4}], 1, 4]"),
            Value::List(vec![])
        );
    }

    #[test]
    fn test_graph_distance() {
        assert_eq!(
            eval_str("GraphDistance[Graph[{1<->2, 2<->3}], 1, 3]"),
            int(2)
        );
        assert_eq!(eval_str("GraphDistance[Graph[{1<->2}], 1, 2]"), int(1));
        assert_eq!(
            eval_str("GraphDistance[Graph[{1<->2, 3<->4}], 1, 4]"),
            Value::Symbol("Infinity".to_string())
        );
    }

    #[test]
    fn test_neighborhood_graph() {
        let v = eval_str("VertexList[NeighborhoodGraph[Graph[{1<->2, 2<->3, 3<->4}], 2]]");
        assert_eq!(ints(&v), vec![1, 2, 3]);
        // induced edges among {1,2,3}
        let e = eval_str("EdgeCount[NeighborhoodGraph[Graph[{1<->2, 2<->3, 3<->4}], 2]]");
        assert_eq!(e, int(2));
    }

    #[test]
    fn test_graph_plot_svg() {
        match eval_str("GraphPlot[Graph[{1<->2, 2<->3}]]") {
            Value::Str(s) => {
                assert!(s.contains("<svg"));
                assert!(s.contains("<circle"));
                assert!(s.contains("<line"));
                assert!(s.contains("</svg>"));
            }
            other => panic!("expected svg string, got {other:?}"),
        }
    }

    #[test]
    fn test_degree_alias_routing() {
        // `Degree` is the angle constant when bare but routes `Degree[g]` to the
        // graph implementation via the operator table.
        assert_eq!(
            ints(&eval_str("Degree[Graph[{1<->2, 2<->3, 3<->1}]]")),
            vec![2, 2, 2]
        );
        assert_eq!(eval_str("Degree[Graph[{1<->2, 2<->3}], 2]"), int(2));
        assert!(matches!(eval_str("Degree"), Value::Real(_)));
        assert!(matches!(eval_str("2 Degree"), Value::Real(_)));
    }

    #[test]
    fn test_directed_degrees() {
        assert_eq!(
            ints(&eval_str("VertexDegree[Graph[{1->2, 1->3, 2->3}]]")),
            vec![2, 2, 2]
        );
        assert_eq!(
            ints(&eval_str(
                "VertexDegree[Graph[{1->2, 1->3, 2->3}], \"Out\"]"
            )),
            vec![2, 1, 0]
        );
        assert_eq!(
            ints(&eval_str("VertexDegree[Graph[{1->2, 1->3, 2->3}], \"In\"]")),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn test_graph_plot_options() {
        match eval_str("GraphPlot[Graph[{1<->2}]]") {
            Value::Str(s) => assert!(s.contains("<text")),
            other => panic!("expected svg, got {other:?}"),
        }
        // VertexLabels -> None (on the inner graph) suppresses labels.
        match eval_str("GraphPlot[Graph[{1<->2}, VertexLabels -> None]]") {
            Value::Str(s) => assert!(!s.contains("<text")),
            other => panic!("expected svg, got {other:?}"),
        }
        // Directed edges draw an arrow head.
        match eval_str("GraphPlot[Graph[{1->2}]]") {
            Value::Str(s) => assert!(s.contains("<polygon")),
            other => panic!("expected svg, got {other:?}"),
        }
    }

    #[test]
    fn test_graph_plot_bare_edges() {
        match eval_str("GraphPlot[{1->2, 2->3, 3->1}]") {
            Value::Str(s) => assert!(s.contains("<svg") && s.contains("<line")),
            other => panic!("expected svg, got {other:?}"),
        }
    }

    #[test]
    fn test_unevaluated_on_bad_input() {
        assert!(
            matches!(eval_str("VertexList[5]"), Value::Call { head, .. } if head == "VertexList")
        );
        assert!(
            matches!(eval_str("EdgeCount[3]"), Value::Call { head, .. } if head == "EdgeCount")
        );
        assert_eq!(eval_str("GraphQ[Graph[{1<->2}]]"), Value::Bool(true));
        assert_eq!(eval_str("GraphQ[42]"), Value::Bool(false));
    }

    // ── Extended algorithm tests ────────────────────────────────────────────

    fn reals(v: &Value) -> Vec<f64> {
        match v {
            Value::List(items) => items.iter().map(|x| x.to_real().unwrap()).collect(),
            _ => panic!("expected list of reals, got {v:?}"),
        }
    }

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "{a} !~= {b}");
    }

    #[test]
    fn test_graph_distance_matrix() {
        let m = eval_str("GraphDistanceMatrix[Graph[{1<->2, 2<->3}]]");
        match m {
            Value::List(rows) => {
                let rr: Vec<Vec<i64>> = rows.iter().map(ints).collect();
                assert_eq!(rr, vec![vec![0, 1, 2], vec![1, 0, 1], vec![2, 1, 0]]);
            }
            _ => panic!("expected matrix"),
        }
        // Unreachable pairs are Infinity.
        let m = eval_str("GraphDistanceMatrix[Graph[{1<->2, 3<->4}]]");
        match m {
            Value::List(rows) => {
                assert_eq!(
                    rows[0].clone(),
                    Value::List(vec![int(0), int(1), infinity(), infinity()])
                );
            }
            _ => panic!("expected matrix"),
        }
    }

    #[test]
    fn test_graph_diameter_radius_center_periphery() {
        assert_eq!(eval_str("GraphDiameter[Graph[{1<->2, 2<->3}]]"), int(2));
        assert_eq!(
            eval_str("GraphDiameter[Graph[{1<->2, 2<->3, 3<->1}]]"),
            int(1)
        );
        assert_eq!(
            eval_str("GraphDiameter[Graph[{1<->2, 3<->4}]]"),
            Value::Symbol("Infinity".to_string())
        );
        assert_eq!(
            eval_str("GraphRadius[Graph[{1<->2, 2<->3, 3<->4}]]"),
            int(2)
        );
        assert_eq!(
            ints(&eval_str("GraphCenter[Graph[{1<->2, 2<->3, 3<->4}]]")),
            vec![2, 3]
        );
        assert_eq!(
            ints(&eval_str("GraphPeriphery[Graph[{1<->2, 2<->3, 3<->4}]]")),
            vec![1, 4]
        );
    }

    #[test]
    fn test_vertex_and_edge_index() {
        assert_eq!(
            eval_str("VertexIndex[Graph[{\"a\", \"b\"}, {\"a\"->\"b\"}], \"b\"]"),
            int(2)
        );
        assert_eq!(eval_str("EdgeIndex[Graph[{1<->2, 2<->3}], 2<->3]"), int(2));
        assert_eq!(eval_str("EdgeIndex[Graph[{1<->2}], 2<->1]"), int(1));
        assert_eq!(
            eval_str("VertexIndex[Graph[{1<->2}], 5]"),
            Value::Symbol("Infinity".to_string())
        );
    }

    #[test]
    fn test_incidence_matrix() {
        let m = eval_str("IncidenceMatrix[Graph[{1<->2, 2<->3}]]");
        match m {
            Value::List(rows) => {
                let rr: Vec<Vec<i64>> = rows.iter().map(ints).collect();
                assert_eq!(rr, vec![vec![1, 0], vec![1, 1], vec![0, 1]]);
            }
            _ => panic!("expected matrix"),
        }
        // Directed edges record source/target.
        let m = eval_str("IncidenceMatrix[Graph[{1->2}]]");
        match m {
            Value::List(rows) => {
                let rr: Vec<Vec<i64>> = rows.iter().map(ints).collect();
                assert_eq!(rr, vec![vec![-1], vec![1]]);
            }
            _ => panic!("expected matrix"),
        }
    }

    #[test]
    fn test_subgraph_and_edits() {
        assert_eq!(
            eval_str("EdgeCount[Subgraph[Graph[{1<->2, 2<->3, 3<->4}], {1, 2}]]"),
            int(1)
        );
        assert_eq!(
            ints(&eval_str(
                "VertexList[VertexDelete[Graph[{1<->2, 2<->3}], 2]]"
            )),
            vec![1, 3]
        );
        assert_eq!(
            eval_str("EdgeCount[EdgeDelete[Graph[{1<->2, 2<->3, 3<->1}], 1<->2]]"),
            int(2)
        );
        assert_eq!(
            ints(&eval_str("VertexList[VertexAdd[Graph[{1<->2}], 3]]")),
            vec![1, 2, 3]
        );
        assert_eq!(
            eval_str("EdgeCount[EdgeAdd[Graph[{1<->2}], 2<->3]]"),
            int(2)
        );
        // EdgeAdd introduces new vertices.
        assert_eq!(
            eval_str("VertexCount[EdgeAdd[Graph[{1<->2}], 3<->4]]"),
            int(4)
        );
    }

    #[test]
    fn test_vertex_contract_and_replace() {
        let g = eval_str("VertexContract[Graph[{1<->2, 2<->3, 3<->1}], {1, 2}]");
        assert_eq!(eval_str(&format!("VertexCount[{}]", g)), int(2));
        let r = eval_str("VertexList[VertexReplace[Graph[{1<->2, 2<->3}], 2 -> 5]]");
        assert_eq!(ints(&r), vec![1, 5, 3]);
    }

    #[test]
    fn test_neighborhood_vertices() {
        assert_eq!(
            ints(&eval_str(
                "NeighborhoodVertices[Graph[{1<->2, 2<->3, 3<->4}], 2]"
            )),
            vec![2, 1, 3]
        );
    }

    #[test]
    fn test_complement_union_intersection_difference() {
        assert_eq!(
            eval_str("EdgeCount[GraphComplement[Graph[{1<->2, 2<->3, 3<->1}]]]"),
            int(0)
        );
        assert_eq!(
            eval_str("EdgeCount[GraphComplement[Graph[{1<->2, 2<->3}]]]"),
            int(1)
        );
        assert_eq!(
            eval_str("VertexCount[GraphUnion[Graph[{1<->2}], Graph[{2<->3}]]]"),
            int(3)
        );
        assert_eq!(
            eval_str("EdgeCount[GraphUnion[Graph[{1<->2}], Graph[{2<->3}]]]"),
            int(2)
        );
        assert_eq!(
            eval_str("EdgeCount[GraphIntersection[Graph[{1<->2, 2<->3}], Graph[{2<->3, 3<->4}]]]"),
            int(1)
        );
        assert_eq!(
            eval_str("EdgeCount[GraphDifference[Graph[{1<->2, 2<->3}], Graph[{2<->3}]]]"),
            int(1)
        );
        assert_eq!(
            eval_str("VertexCount[GraphDisjointUnion[Graph[{1<->2}], Graph[{1<->2}]]]"),
            int(4)
        );
    }

    #[test]
    fn test_cycles_and_connectivity() {
        match eval_str("FindCycle[Graph[{1<->2, 2<->3, 3<->1}]]") {
            Value::List(items) => assert_eq!(items.len(), 3),
            _ => panic!("expected list"),
        }
        assert_eq!(
            eval_str("FindCycle[Graph[{1<->2, 2<->3}]]"),
            Value::List(vec![])
        );
        assert_eq!(
            eval_str("ConnectedGraphQ[Graph[{1<->2, 2<->3}]]"),
            Value::Bool(true)
        );
        assert_eq!(
            eval_str("ConnectedGraphQ[Graph[{1<->2, 3<->4}]]"),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_spanning_tree_and_kruskal() {
        assert_eq!(
            eval_str("EdgeCount[FindSpanningTree[Graph[{1<->2, 2<->3, 3<->1}]]]"),
            int(2)
        );
        assert_eq!(
            eval_str("VertexCount[FindSpanningTree[Graph[{1<->2, 2<->3, 3<->1}]]]"),
            int(3)
        );
        match eval_str("Kruskal[Graph[{1<->2, 2<->3, 3<->1}]]") {
            Value::List(items) => assert_eq!(items.len(), 2),
            _ => panic!("expected edge list"),
        }
    }

    #[test]
    fn test_cliques() {
        assert_eq!(
            eval_str("Length[FindClique[Graph[{1<->2, 2<->3, 3<->1}]]]"),
            int(3)
        );
        assert_eq!(
            eval_str("Length[FindKClique[Graph[{1<->2, 2<->3, 3<->1, 1<->4}], 3]]"),
            int(3)
        );
    }

    #[test]
    fn test_hamiltonian() {
        assert_eq!(
            eval_str("Length[HamiltonianPath[Graph[{1<->2, 2<->3, 3<->4}]]]"),
            int(4)
        );
        let cyc = eval_str("FindHamiltonianCycle[Graph[{1<->2, 2<->3, 3<->4, 4<->1}]]");
        match cyc {
            Value::List(items) => {
                assert_eq!(items.len(), 5);
                assert_eq!(items.first(), items.last());
            }
            _ => panic!("expected list"),
        }
        assert_eq!(
            eval_str("HamiltonianPath[Graph[{1<->2, 3<->4}]]"),
            Value::List(vec![])
        );
    }

    #[test]
    fn test_eulerian() {
        assert_eq!(
            eval_str("EulerianGraphQ[Graph[{1<->2, 2<->3, 3<->1}]]"),
            Value::Bool(true)
        );
        assert_eq!(
            eval_str("EulerianGraphQ[Graph[{1<->2, 2<->3}]]"),
            Value::Bool(false)
        );
        match eval_str("FindEulerianCycle[Graph[{1<->2, 2<->3, 3<->1}]]") {
            Value::List(items) => {
                assert_eq!(items.len(), 4);
                assert_eq!(items.first(), items.last());
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_centralities() {
        let dc = reals(&eval_str("DegreeCentrality[Graph[{1<->2, 2<->3, 3<->1}]]"));
        for x in dc {
            approx(x, 1.0);
        }
        let cc = reals(&eval_str(
            "ClosenessCentrality[Graph[{1<->2, 2<->3, 3<->1}]]",
        ));
        for x in cc {
            approx(x, 1.0);
        }
        // Betweenness on a star concentrates on the hub.
        let bc = reals(&eval_str(
            "BetweennessCentrality[Graph[{1<->2, 1<->3, 1<->4}]]",
        ));
        assert!(bc[0] > bc[1]);
        // Eigenvector centrality on a path peaks in the middle.
        let ec = reals(&eval_str(
            "EigenvectorCentrality[Graph[{1<->2, 2<->3, 3<->4}]]",
        ));
        assert!(ec[1] > ec[0]);
        // PageRank is a probability distribution.
        let pr = reals(&eval_str(
            "PageRankCentrality[Graph[{1<->2, 2<->3, 3<->1}]]",
        ));
        approx(pr.iter().sum::<f64>(), 1.0);
    }

    #[test]
    fn test_clustering_and_assortativity() {
        let lcc = reals(&eval_str(
            "LocalClusteringCoefficient[Graph[{1<->2, 2<->3, 3<->1}]]",
        ));
        for x in lcc {
            approx(x, 1.0);
        }
        approx(
            eval_str("GlobalClusteringCoefficient[Graph[{1<->2, 2<->3, 3<->1}]]")
                .to_real()
                .unwrap(),
            1.0,
        );
        let a = eval_str("GraphAssortativity[Graph[{1<->2, 2<->3, 3<->4}]]")
            .to_real()
            .unwrap();
        assert!((-1.0..=1.0).contains(&a));
    }

    #[test]
    fn test_clustering_coefficient_alias() {
        approx(
            eval_str("ClusteringCoefficient[Graph[{1<->2, 2<->3, 3<->1}]]")
                .to_real()
                .unwrap(),
            1.0,
        );
        // A star has no links among neighbours of any vertex.
        approx(
            eval_str("ClusteringCoefficient[Graph[{1<->2, 1<->3, 1<->4}]]")
                .to_real()
                .unwrap(),
            0.0,
        );
        assert!(
            matches!(eval_str("ClusteringCoefficient[5]"), Value::Call { head, .. } if head == "ClusteringCoefficient")
        );
    }

    #[test]
    fn test_degree_list_alias() {
        assert_eq!(
            ints(&eval_str("DegreeList[Graph[{1<->2, 2<->3, 3<->1}]]")),
            vec![2, 2, 2]
        );
        assert_eq!(eval_str("DegreeList[Graph[{1<->2, 2<->3}], 2]"), int(2));
        assert!(
            matches!(eval_str("DegreeList[5]"), Value::Call { head, .. } if head == "DegreeList")
        );
    }

    #[test]
    fn test_kruskal_tree_alias() {
        assert_eq!(
            eval_str("EdgeCount[KruskalTree[Graph[{1<->2, 2<->3, 3<->1}]]]"),
            int(2)
        );
        assert_eq!(
            eval_str("VertexCount[KruskalTree[Graph[{1<->2, 2<->3, 3<->1}]]]"),
            int(3)
        );
        assert_eq!(
            eval_str("EdgeCount[KruskalTree[Graph[{1<->2, 3<->4}]]]"),
            int(2)
        );
        assert!(
            matches!(eval_str("KruskalTree[5]"), Value::Call { head, .. } if head == "KruskalTree")
        );
    }

    #[test]
    fn test_random_graph_seeded() {
        let a = eval_str("SeedRandom[7]; EdgeList[RandomGraph[{5, 4}]]");
        let b = eval_str("SeedRandom[7]; EdgeList[RandomGraph[{5, 4}]]");
        assert_eq!(a, b);
        assert_eq!(eval_str("VertexCount[RandomGraph[{5, 4}]]"), int(5));
        assert_eq!(eval_str("EdgeCount[RandomGraph[{5, 4}]]"), int(4));
        // G(n, p) with p = 0 gives the empty graph on n vertices.
        assert_eq!(eval_str("EdgeCount[RandomGraph[{5, 0.0}]]"), int(0));
    }

    #[test]
    fn test_extended_unevaluated() {
        assert!(
            matches!(eval_str("GraphDiameter[5]"), Value::Call { head, .. } if head == "GraphDiameter")
        );
        assert!(
            matches!(eval_str("RandomGraph[5]"), Value::Call { head, .. } if head == "RandomGraph")
        );
    }
}
