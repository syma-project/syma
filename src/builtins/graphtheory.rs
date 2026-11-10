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
    register_builtin(env, "ConnectedComponents", builtin_connected_components);
    register_builtin(env, "FindShortestPath", builtin_find_shortest_path);
    register_builtin(env, "GraphDistance", builtin_graph_distance);
    register_builtin(env, "NeighborhoodGraph", builtin_neighborhood_graph);
    register_builtin(env, "GraphPlot", builtin_graph_plot);

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
}
