//! Tensor algebra: TensorProduct, TensorContract, TensorTranspose, tensor
//! symmetries, higher-rank Dot/Inner/Tr, and structured-matrix neighbours.
//!
//! Tensors are represented as (possibly nested) rectangular lists, exactly as
//! they appear in Wolfram Language. Operations are exact: entries are combined
//! with the shared `arithmetic` helpers so integer/rational/symbolic results
//! stay unevaluated rather than being coerced to floating point.
//!
//! Anything with a mismatched shape (bad permutation, unequal contractions,
//! incompatible Dot dimensions, …) is returned *unevaluated* as a
//! `Value::Call`, matching Wolfram Language semantics.

#![allow(clippy::needless_range_loop)]

use crate::env::Env;
use crate::eval::apply_function;
use crate::value::{EvalError, Value, rational_value};
use rug::Integer;

use super::arithmetic::{add_values_public, mul_values_public};

// ── Small helpers ────────────────────────────────────────────────────────────

/// Create an `Integer` value.
fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

/// Build an unevaluated call `head[args...]`.
fn unevaluated(name: &str, args: &[Value]) -> Value {
    Value::Call {
        head: name.to_string(),
        args: args.to_vec(),
    }
}

/// Borrow the items of a list, or `None` for non-lists.
fn as_list(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::List(items) => Some(items),
        _ => None,
    }
}

/// Convert a (non-negative) integer value to `usize`.
fn int_of(v: &Value) -> Option<usize> {
    match v {
        Value::Integer(i) => i.to_usize(),
        _ => None,
    }
}

/// Rectangular dimensions of a nested-list tensor. Atoms have rank 0 (`[]`).
fn tensor_dims(v: &Value) -> Vec<usize> {
    let mut dims = Vec::new();
    let mut cur = v;
    while let Value::List(items) = cur {
        dims.push(items.len());
        if items.is_empty() {
            break;
        }
        match &items[0] {
            Value::List(first) => {
                let n = first.len();
                if !items
                    .iter()
                    .all(|it| matches!(it, Value::List(s) if s.len() == n))
                {
                    break;
                }
                cur = &items[0];
            }
            _ => break,
        }
    }
    dims
}

/// Fetch the element of a nested list at `idx`.
fn tensor_get<'a>(v: &'a Value, idx: &[usize]) -> Option<&'a Value> {
    let mut cur = v;
    for &i in idx {
        match cur {
            Value::List(items) => cur = items.get(i)?,
            _ => return None,
        }
    }
    Some(cur)
}

/// Enumerate every multi-index of `dims` in row-major order.
fn indices(dims: &[usize]) -> Vec<Vec<usize>> {
    if dims.is_empty() {
        return vec![vec![]];
    }
    let total: usize = dims.iter().product();
    let mut out = Vec::with_capacity(total);
    let mut idx = vec![0usize; dims.len()];
    for _ in 0..total {
        out.push(idx.clone());
        for k in (0..dims.len()).rev() {
            idx[k] += 1;
            if idx[k] < dims[k] {
                break;
            }
            idx[k] = 0;
        }
    }
    out
}

/// Rebuild a nested list of shape `dims` from row-major flattened values.
fn build_from_flat(dims: &[usize], flat: &mut std::vec::IntoIter<Value>) -> Value {
    if dims.is_empty() {
        return flat.next().unwrap_or(Value::Null);
    }
    let n = dims[0];
    Value::List((0..n).map(|_| build_from_flat(&dims[1..], flat)).collect())
}

// ── Permutation helpers ──────────────────────────────────────────────────────

/// All permutations of `0..n` (as image vectors).
fn permutations(n: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut cur: Vec<usize> = (0..n).collect();
    permute(&mut cur, 0, &mut out);
    out
}

fn permute(cur: &mut [usize], k: usize, out: &mut Vec<Vec<usize>>) {
    if k == cur.len() {
        out.push(cur.to_vec());
        return;
    }
    for i in k..cur.len() {
        cur.swap(k, i);
        permute(cur, k + 1, out);
        cur.swap(k, i);
    }
}

/// Sign of a permutation given as an image vector.
fn signature(p: &[usize]) -> i64 {
    let mut inv = 0i64;
    for i in 0..p.len() {
        for j in (i + 1)..p.len() {
            if p[i] > p[j] {
                inv += 1;
            }
        }
    }
    if inv % 2 == 0 { 1 } else { -1 }
}

fn factorial(n: usize) -> i64 {
    (1..=n as i64).product::<i64>().max(1)
}

// ── Elementwise negation ─────────────────────────────────────────────────────

fn negate_value(v: &Value) -> Result<Value, EvalError> {
    scale_value(v, &int(-1))
}

/// Multiply every leaf of `v` by the scalar `factor`.
fn scale_value(v: &Value, factor: &Value) -> Result<Value, EvalError> {
    match v {
        Value::List(items) => Ok(Value::List(
            items
                .iter()
                .map(|i| scale_value(i, factor))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        _ => mul_values_public(factor, v),
    }
}

// ── TensorProduct ────────────────────────────────────────────────────────────

/// Outer product of two tensors with `Times`: `a`'s indices come first.
fn outer_times(a: &Value, b: &Value) -> Result<Value, EvalError> {
    match (a, b) {
        (Value::List(xs), _) => Ok(Value::List(
            xs.iter()
                .map(|x| outer_times(x, b))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        (_, Value::List(ys)) => Ok(Value::List(
            ys.iter()
                .map(|y| outer_times(a, y))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        _ => mul_values_public(a, b),
    }
}

/// TensorProduct[t1, t2, ...] — outer product of the arguments.
pub fn builtin_tensor_product(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(int(1));
    }
    let mut acc = args[0].clone();
    for b in &args[1..] {
        acc = outer_times(&acc, b)?;
    }
    Ok(acc)
}

// ── TensorContract ───────────────────────────────────────────────────────────

/// Parse `{i, j}` or `{{i1,j1}, ...}` into 0-based disjoint index pairs.
fn parse_pairs(v: &Value, rank: usize) -> Option<Vec<(usize, usize)>> {
    let outer = as_list(v)?;
    let all_ints = outer.iter().all(|x| matches!(x, Value::Integer(_)));
    if all_ints {
        if outer.len() != 2 {
            return None;
        }
        let i = int_of(&outer[0])?;
        let j = int_of(&outer[1])?;
        return check_pair(i, j, rank).map(|p| vec![p]);
    }
    let mut pairs = Vec::new();
    let mut used = vec![false; rank];
    for e in outer {
        let pair = as_list(e)?;
        if pair.len() != 2 {
            return None;
        }
        let i = int_of(&pair[0])?;
        let j = int_of(&pair[1])?;
        let (a, b) = check_pair(i, j, rank)?;
        if used[a] || used[b] {
            return None;
        }
        used[a] = true;
        used[b] = true;
        pairs.push((a, b));
    }
    Some(pairs)
}

/// Validate a 1-based pair, returning 0-based indices.
fn check_pair(i: usize, j: usize, rank: usize) -> Option<(usize, usize)> {
    if i < 1 || j < 1 || i > rank || j > rank || i == j {
        return None;
    }
    Some((i - 1, j - 1))
}

/// Contract `pairs` of equal indices, summing over each shared value.
fn contract_impl(t: &Value, pairs: &[(usize, usize)]) -> Result<Value, EvalError> {
    let dims = tensor_dims(t);
    let rank = dims.len();
    if pairs.is_empty() {
        return Ok(t.clone());
    }
    let mut contracted = vec![false; rank];
    for &(i, j) in pairs {
        contracted[i] = true;
        contracted[j] = true;
    }
    let rem_pos: Vec<usize> = (0..rank).filter(|&k| !contracted[k]).collect();
    let result_dims: Vec<usize> = rem_pos.iter().map(|&k| dims[k]).collect();
    let pair_dims: Vec<usize> = pairs.iter().map(|&(i, _)| dims[i]).collect();

    let mut flat = Vec::new();
    for ridx in indices(&result_dims) {
        let mut full = vec![0usize; rank];
        for (a, &pos) in rem_pos.iter().enumerate() {
            full[pos] = ridx[a];
        }
        let mut sum = int(0);
        for pidx in indices(&pair_dims) {
            for (p, &(i, j)) in pairs.iter().enumerate() {
                full[i] = pidx[p];
                full[j] = pidx[p];
            }
            if let Some(elem) = tensor_get(t, &full) {
                sum = add_values_public(&sum, elem)?;
            }
        }
        flat.push(sum);
    }
    Ok(build_from_flat(&result_dims, &mut flat.into_iter()))
}

/// TensorContract[tensor, {i, j}] / TensorContract[tensor, {{i1,j1}, ...}].
pub fn builtin_tensor_contract(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("TensorContract", args));
    }
    let t = &args[0];
    let dims = tensor_dims(t);
    let rank = dims.len();
    if rank < 2 {
        return Ok(unevaluated("TensorContract", args));
    }
    let pairs = match parse_pairs(&args[1], rank) {
        Some(p) => p,
        None => return Ok(unevaluated("TensorContract", args)),
    };
    for &(i, j) in &pairs {
        if dims[i] != dims[j] {
            return Ok(unevaluated("TensorContract", args));
        }
    }
    contract_impl(t, &pairs)
}

// ── TensorTranspose ──────────────────────────────────────────────────────────

/// Parse a 1-based permutation of `rank` axes into 0-based form.
fn parse_perm(v: &Value, rank: usize) -> Option<Vec<usize>> {
    let l = as_list(v)?;
    if l.len() != rank {
        return None;
    }
    let mut p = Vec::with_capacity(rank);
    let mut seen = vec![false; rank];
    for e in l {
        let x = int_of(e)?;
        if x < 1 || x > rank || seen[x - 1] {
            return None;
        }
        seen[x - 1] = true;
        p.push(x - 1);
    }
    Some(p)
}

/// Permute the axes of `t` according to `perm` (0-based, `perm[k]` feeds axis `k`).
fn transpose_value(t: &Value, dims: &[usize], perm: &[usize]) -> Value {
    let rank = dims.len();
    let rd: Vec<usize> = perm.iter().map(|&p| dims[p]).collect();
    let mut flat = Vec::new();
    for ridx in indices(&rd) {
        let mut inv = vec![0usize; rank];
        for k in 0..rank {
            inv[perm[k]] = ridx[k];
        }
        flat.push(tensor_get(t, &inv).cloned().unwrap_or(Value::Null));
    }
    build_from_flat(&rd, &mut flat.into_iter())
}

/// TensorTranspose[tensor] / TensorTranspose[tensor, perm].
pub fn builtin_tensor_transpose(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("TensorTranspose", args));
    }
    let t = &args[0];
    let dims = tensor_dims(t);
    let rank = dims.len();
    if rank < 2 {
        return Ok(t.clone());
    }
    let perm = if args.len() == 2 {
        match parse_perm(&args[1], rank) {
            Some(p) => p,
            None => return Ok(unevaluated("TensorTranspose", args)),
        }
    } else {
        // Default: swap the first two indices.
        let mut p: Vec<usize> = (0..rank).collect();
        p.swap(0, 1);
        p
    };
    Ok(transpose_value(t, &dims, &perm))
}

// ── Symmetrize / Antisymmetrize ──────────────────────────────────────────────

/// Parse an explicit list of (≥2) 1-based index positions.
fn parse_positions(v: &Value, rank: usize) -> Option<Vec<usize>> {
    let l = as_list(v)?;
    if l.len() < 2 {
        return None;
    }
    let mut out = Vec::with_capacity(l.len());
    let mut seen = vec![false; rank];
    for e in l {
        let x = int_of(e)?;
        if x < 1 || x > rank || seen[x - 1] {
            return None;
        }
        seen[x - 1] = true;
        out.push(x - 1);
    }
    Some(out)
}

fn symmetrize_impl(args: &[Value], anti: bool) -> Result<Value, EvalError> {
    let name = if anti { "Antisymmetrize" } else { "Symmetrize" };
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated(name, args));
    }
    let t = &args[0];
    let dims = tensor_dims(t);
    let rank = dims.len();
    if rank < 2 {
        return Ok(t.clone());
    }
    let positions = if args.len() == 2 {
        match parse_positions(&args[1], rank) {
            Some(p) => p,
            None => return Ok(unevaluated(name, args)),
        }
    } else {
        (0..rank).collect()
    };
    let d0 = dims[positions[0]];
    if positions.iter().any(|&p| dims[p] != d0) {
        return Ok(unevaluated(name, args));
    }
    let perms = permutations(positions.len());
    let mut acc = int(0);
    for p in &perms {
        let mut axis_perm: Vec<usize> = (0..rank).collect();
        for (k, &pos) in positions.iter().enumerate() {
            axis_perm[pos] = positions[p[k]];
        }
        let tr = transpose_value(t, &dims, &axis_perm);
        let v = if anti && signature(p) < 0 {
            negate_value(&tr)?
        } else {
            tr
        };
        acc = add_values_public(&acc, &v)?;
    }
    let inv_fact = rational_value(Integer::from(1), Integer::from(factorial(positions.len())));
    scale_value(&acc, &inv_fact)
}

/// Symmetrize[tensor] / Symmetrize[tensor, {i1, ..., ik}].
pub fn builtin_symmetrize(args: &[Value]) -> Result<Value, EvalError> {
    symmetrize_impl(args, false)
}

/// Antisymmetrize[tensor] / Antisymmetrize[tensor, {i1, ..., ik}].
pub fn builtin_antisymmetrize(args: &[Value]) -> Result<Value, EvalError> {
    symmetrize_impl(args, true)
}

// ── TensorSymmetry ───────────────────────────────────────────────────────────

fn cycles_value(i: usize, j: usize) -> Value {
    Value::Call {
        head: "Cycles".to_string(),
        args: vec![Value::List(vec![Value::List(vec![
            int((i + 1) as i64),
            int((j + 1) as i64),
        ])])],
    }
}

/// TensorSymmetry[tensor] — generators (as `Cycles`) of transpositions that
/// leave the tensor invariant up to an overall sign.
pub fn builtin_tensor_symmetry(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("TensorSymmetry", args));
    }
    let t = &args[0];
    let dims = tensor_dims(t);
    let rank = dims.len();
    if rank < 2 {
        return Ok(Value::List(vec![]));
    }
    let neg = negate_value(t)?;
    let mut gens = Vec::new();
    for i in 0..rank {
        for j in (i + 1)..rank {
            if dims[i] != dims[j] {
                continue;
            }
            let mut perm: Vec<usize> = (0..rank).collect();
            perm.swap(i, j);
            let tr = transpose_value(t, &dims, &perm);
            if tr.struct_eq(t) || tr.struct_eq(&neg) {
                gens.push(cycles_value(i, j));
            }
        }
    }
    Ok(Value::List(gens))
}

// ── Rank / dimensions ────────────────────────────────────────────────────────

/// TensorRank[tensor] — number of indices.
pub fn builtin_tensor_rank(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("TensorRank", args));
    }
    Ok(int(tensor_dims(&args[0]).len() as i64))
}

/// TensorDimensions[tensor] — dimensions of each index.
pub fn builtin_tensor_dimensions(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("TensorDimensions", args));
    }
    Ok(Value::List(
        tensor_dims(&args[0])
            .into_iter()
            .map(|d| int(d as i64))
            .collect(),
    ))
}

// ── Dot / Inner (higher rank) ────────────────────────────────────────────────

/// Contract the last index of `a` with the first index of `b`.
fn dot_pair(a: &Value, b: &Value) -> Result<Option<Value>, EvalError> {
    if as_list(a).is_none() || as_list(b).is_none() {
        return Ok(None);
    }
    let da = tensor_dims(a);
    let db = tensor_dims(b);
    let p = da.len();
    let q = db.len();
    if p == 0 || q == 0 {
        return Ok(None);
    }
    let n = da[p - 1];
    if n != db[0] {
        return Ok(None);
    }
    let mut rd: Vec<usize> = da[..p - 1].to_vec();
    rd.extend_from_slice(&db[1..]);
    let ra = p - 1;
    let mut flat = Vec::new();
    for ridx in indices(&rd) {
        let mut sum = int(0);
        for k in 0..n {
            let mut ai = Vec::with_capacity(p);
            ai.extend_from_slice(&ridx[..ra]);
            ai.push(k);
            let mut bi = Vec::with_capacity(q);
            bi.push(k);
            bi.extend_from_slice(&ridx[ra..]);
            let av = tensor_get(a, &ai).cloned().unwrap_or(Value::Null);
            let bv = tensor_get(b, &bi).cloned().unwrap_or(Value::Null);
            let prod = mul_values_public(&av, &bv)?;
            sum = add_values_public(&sum, &prod)?;
        }
        flat.push(sum);
    }
    Ok(Some(build_from_flat(&rd, &mut flat.into_iter())))
}

/// Dot[a, b, ...] — generalized contraction over the last/first indices.
pub fn builtin_dot(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(unevaluated("Dot", args));
    }
    let mut acc = args[0].clone();
    for b in &args[1..] {
        match dot_pair(&acc, b)? {
            Some(v) => acc = v,
            None => return Ok(unevaluated("Dot", args)),
        }
    }
    Ok(acc)
}

/// Inner[f, g, t1, t2] / Inner[f, t1, t2] — generalized inner product.
pub fn builtin_inner(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let (f, g, a, b) = match args.len() {
        3 => (
            &args[0],
            &Value::Symbol("Plus".to_string()),
            &args[1],
            &args[2],
        ),
        4 => (&args[0], &args[1], &args[2], &args[3]),
        _ => return Ok(unevaluated("Inner", args)),
    };
    if as_list(a).is_none() || as_list(b).is_none() {
        return Ok(unevaluated("Inner", args));
    }
    let da = tensor_dims(a);
    let db = tensor_dims(b);
    let p = da.len();
    let q = db.len();
    if p == 0 || q == 0 {
        return Ok(unevaluated("Inner", args));
    }
    let n = da[p - 1];
    if n != db[0] {
        return Ok(unevaluated("Inner", args));
    }
    let mut rd: Vec<usize> = da[..p - 1].to_vec();
    rd.extend_from_slice(&db[1..]);
    let ra = p - 1;
    let mut flat = Vec::new();
    for ridx in indices(&rd) {
        let mut terms = Vec::with_capacity(n);
        for k in 0..n {
            let mut ai = Vec::with_capacity(p);
            ai.extend_from_slice(&ridx[..ra]);
            ai.push(k);
            let mut bi = Vec::with_capacity(q);
            bi.push(k);
            bi.extend_from_slice(&ridx[ra..]);
            let av = tensor_get(a, &ai).cloned().unwrap_or(Value::Null);
            let bv = tensor_get(b, &bi).cloned().unwrap_or(Value::Null);
            terms.push(apply_function(f, &[av, bv], env)?);
        }
        flat.push(apply_function(g, &terms, env)?);
    }
    Ok(build_from_flat(&rd, &mut flat.into_iter()))
}

// ── Generalized Tr ───────────────────────────────────────────────────────────

/// Tr[tensor] — trace over the first two indices; `Tr[m, f]` combines the
/// diagonal entries of a matrix with `f` instead of `Plus`.
pub fn builtin_tr(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let t = match args.first() {
        Some(t) => t,
        None => return Ok(unevaluated("Tr", args)),
    };
    let dims = tensor_dims(t);
    let rank = dims.len();
    if rank == 0 {
        return Ok(t.clone());
    }
    if rank == 1 {
        return super::list::builtin_total(std::slice::from_ref(t));
    }
    if args.len() == 2 && rank == 2 {
        let n = dims[0].min(dims[1]);
        let entries: Vec<Value> = (0..n)
            .map(|i| tensor_get(t, &[i, i]).cloned().unwrap_or(Value::Null))
            .collect();
        return apply_function(&args[1], &entries, env);
    }
    contract_impl(t, &[(0, 1)])
}

// ── Structured-matrix neighbours (delegates) ─────────────────────────────────

/// KroneckerProduct[a, b] — delegate to the LinearAlgebra implementation.
pub fn builtin_kronecker_product(args: &[Value]) -> Result<Value, EvalError> {
    super::linalg::builtin_kronecker_product(args)
}

/// IdentityMatrix[n] — delegate to the LinearAlgebra implementation.
pub fn builtin_identity_matrix(args: &[Value]) -> Result<Value, EvalError> {
    super::linalg::builtin_identity_matrix(args)
}

/// DiagonalMatrix[v] — delegate to the LinearAlgebra implementation.
pub fn builtin_diagonal_matrix(args: &[Value]) -> Result<Value, EvalError> {
    super::linalg::builtin_diagonal_matrix(args)
}

// ── Registration ─────────────────────────────────────────────────────────────

/// Register the tensor-algebra builtins.
pub fn register(env: &Env) {
    #[allow(clippy::type_complexity)]
    let funcs: &[(&str, fn(&[Value]) -> Result<Value, EvalError>)] = &[
        ("TensorProduct", builtin_tensor_product),
        ("TensorContract", builtin_tensor_contract),
        ("TensorTranspose", builtin_tensor_transpose),
        ("TensorSymmetry", builtin_tensor_symmetry),
        ("Symmetrize", builtin_symmetrize),
        ("Antisymmetrize", builtin_antisymmetrize),
        ("TensorRank", builtin_tensor_rank),
        ("TensorDimensions", builtin_tensor_dimensions),
        ("Dot", builtin_dot),
        ("KroneckerProduct", builtin_kronecker_product),
        ("IdentityMatrix", builtin_identity_matrix),
        ("DiagonalMatrix", builtin_diagonal_matrix),
    ];
    for (name, func) in funcs {
        super::register_builtin(env, name, *func);
        env.set_attributes(name, vec!["Protected".to_string()]);
    }
    super::register_builtin_env(env, "Tr", builtin_tr);
    env.set_attributes("Tr", vec!["Protected".to_string()]);
    super::register_builtin_env(env, "Inner", builtin_inner);
    env.set_attributes("Inner", vec!["Protected".to_string()]);
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::rational_value;

    fn ilist(v: Vec<i64>) -> Value {
        Value::List(v.into_iter().map(int).collect())
    }

    fn mat(rows: Vec<Vec<i64>>) -> Value {
        Value::List(
            rows.into_iter()
                .map(|r| Value::List(r.into_iter().map(int).collect()))
                .collect(),
        )
    }

    fn rat(n: i64, d: i64) -> Value {
        rational_value(Integer::from(n), Integer::from(d))
    }

    fn call_head(v: &Value) -> Option<&str> {
        match v {
            Value::Call { head, .. } => Some(head),
            _ => None,
        }
    }

    #[test]
    fn tensor_product_vectors_is_outer() {
        let r = builtin_tensor_product(&[ilist(vec![1, 2]), ilist(vec![3, 4])]).unwrap();
        assert!(r.struct_eq(&mat(vec![vec![3, 4], vec![6, 8]])));
    }

    #[test]
    fn tensor_product_rank_adds() {
        let r = builtin_tensor_product(&[
            mat(vec![vec![1, 2], vec![3, 4]]),
            mat(vec![vec![0, 1], vec![1, 1]]),
        ])
        .unwrap();
        assert!(r.struct_eq(&Value::List(vec![
            Value::List(vec![
                Value::List(vec![
                    Value::List(vec![int(0), int(1)]),
                    Value::List(vec![int(1), int(1)])
                ]),
                Value::List(vec![
                    Value::List(vec![int(0), int(2)]),
                    Value::List(vec![int(2), int(2)])
                ]),
            ]),
            Value::List(vec![
                Value::List(vec![
                    Value::List(vec![int(0), int(3)]),
                    Value::List(vec![int(3), int(3)])
                ]),
                Value::List(vec![
                    Value::List(vec![int(0), int(4)]),
                    Value::List(vec![int(4), int(4)])
                ]),
            ]),
        ])));
        assert_eq!(tensor_dims(&r), vec![2, 2, 2, 2]);
    }

    #[test]
    fn tensor_contract_matrix_trace() {
        let r = builtin_tensor_contract(&[mat(vec![vec![1, 2], vec![3, 4]]), ilist(vec![1, 2])])
            .unwrap();
        assert!(r.struct_eq(&int(5)));
    }

    #[test]
    fn tensor_contract_rank3_pair() {
        // t[i,j,k] with t[0] = {{1,2},{3,4}}, t[1] = {{5,6},{7,8}}.
        let t = Value::List(vec![
            mat(vec![vec![1, 2], vec![3, 4]]),
            mat(vec![vec![5, 6], vec![7, 8]]),
        ]);
        let r = builtin_tensor_contract(&[t, ilist(vec![2, 3])]).unwrap();
        assert!(r.struct_eq(&ilist(vec![5, 13])));
    }

    #[test]
    fn tensor_contract_shape_mismatch_unevaluated() {
        // Rank-3 tensor with dims {2,2,3}: contracting indices 1 and 3 mismatches.
        let t = Value::List(vec![
            Value::List(vec![ilist(vec![1, 2, 3]), ilist(vec![4, 5, 6])]),
            Value::List(vec![ilist(vec![7, 8, 9]), ilist(vec![1, 1, 1])]),
        ]);
        let r = builtin_tensor_contract(&[t, ilist(vec![1, 3])]).unwrap();
        assert_eq!(call_head(&r), Some("TensorContract"));
    }

    #[test]
    fn tensor_transpose_explicit_perm() {
        let r = builtin_tensor_transpose(&[mat(vec![vec![1, 2], vec![3, 4]]), ilist(vec![2, 1])])
            .unwrap();
        assert!(r.struct_eq(&mat(vec![vec![1, 3], vec![2, 4]])));
    }

    #[test]
    fn tensor_transpose_default_swaps_first_two() {
        let t = Value::List(vec![
            mat(vec![vec![1, 2], vec![3, 4]]),
            mat(vec![vec![5, 6], vec![7, 8]]),
        ]);
        let r = builtin_tensor_transpose(&[t.clone()]).unwrap();
        // R[a,b,c] = t[b,a,c].
        let expected = Value::List(vec![
            mat(vec![vec![1, 2], vec![5, 6]]),
            mat(vec![vec![3, 4], vec![7, 8]]),
        ]);
        assert!(r.struct_eq(&expected));
    }

    #[test]
    fn tensor_transpose_bad_perm_unevaluated() {
        let r = builtin_tensor_transpose(&[mat(vec![vec![1, 2], vec![3, 4]]), ilist(vec![1, 1])])
            .unwrap();
        assert_eq!(call_head(&r), Some("TensorTranspose"));
    }

    #[test]
    fn tensor_rank_and_dimensions() {
        let t = Value::List(vec![
            mat(vec![vec![1, 2], vec![3, 4]]),
            mat(vec![vec![5, 6], vec![7, 8]]),
        ]);
        assert!(
            builtin_tensor_rank(&[t.clone()])
                .unwrap()
                .struct_eq(&int(3))
        );
        assert!(
            builtin_tensor_dimensions(&[t])
                .unwrap()
                .struct_eq(&ilist(vec![2, 2, 2]))
        );
        assert!(builtin_tensor_rank(&[int(7)]).unwrap().struct_eq(&int(0)));
        assert!(
            builtin_tensor_dimensions(&[int(7)])
                .unwrap()
                .struct_eq(&Value::List(vec![]))
        );
    }

    #[test]
    fn symmetrize_and_antisymmetrize_matrix() {
        let m = mat(vec![vec![1, 2], vec![3, 4]]);
        let s = builtin_symmetrize(&[m.clone()]).unwrap();
        assert!(s.struct_eq(&Value::List(vec![
            Value::List(vec![int(1), rat(5, 2)]),
            Value::List(vec![rat(5, 2), int(4)]),
        ])));
        let a = builtin_antisymmetrize(&[m]).unwrap();
        assert!(a.struct_eq(&Value::List(vec![
            Value::List(vec![int(0), rat(-1, 2)]),
            Value::List(vec![rat(1, 2), int(0)]),
        ])));
    }

    #[test]
    fn antisymmetrize_levi_civita_is_fixed() {
        let levi = super::super::linalgmore::builtin_levi_civita_tensor(&[int(3)]).unwrap();
        let r = builtin_antisymmetrize(&[levi.clone()]).unwrap();
        assert!(r.struct_eq(&levi));
    }

    #[test]
    fn symmetrize_subset_positions() {
        // Symmetrize over indices 1 and 3 only.
        let t = Value::List(vec![
            mat(vec![vec![1, 2], vec![3, 4]]),
            mat(vec![vec![5, 6], vec![7, 8]]),
        ]);
        let r = builtin_symmetrize(&[t, ilist(vec![1, 3])]).unwrap();
        // Average of t and its (1,3)-swap.
        let swapped = builtin_tensor_transpose(&[
            Value::List(vec![
                mat(vec![vec![1, 2], vec![3, 4]]),
                mat(vec![vec![5, 6], vec![7, 8]]),
            ]),
            ilist(vec![3, 2, 1]),
        ])
        .unwrap();
        let orig = Value::List(vec![
            mat(vec![vec![1, 2], vec![3, 4]]),
            mat(vec![vec![5, 6], vec![7, 8]]),
        ]);
        let expected =
            scale_value(&add_values_public(&orig, &swapped).unwrap(), &rat(1, 2)).unwrap();
        assert!(r.struct_eq(&expected));
    }

    #[test]
    fn tensor_symmetry_reports_generators() {
        let sym = builtin_tensor_symmetry(&[mat(vec![vec![1, 2], vec![2, 3]])]).unwrap();
        let items = as_list(&sym).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(call_head(&items[0]), Some("Cycles"));
        let levi = super::super::linalgmore::builtin_levi_civita_tensor(&[int(3)]).unwrap();
        let lsym = builtin_tensor_symmetry(&[levi]).unwrap();
        assert_eq!(as_list(&lsym).unwrap().len(), 3);
        let none = builtin_tensor_symmetry(&[mat(vec![vec![1, 2], vec![3, 4]])]).unwrap();
        assert!(as_list(&none).unwrap().is_empty());
    }

    #[test]
    fn tr_generalizes_to_higher_rank() {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let t = Value::List(vec![
            mat(vec![vec![1, 2], vec![3, 4]]),
            mat(vec![vec![5, 6], vec![7, 8]]),
        ]);
        let r = builtin_tr(&[t], &env).unwrap();
        // sum_i t[i,i,k] = {1+7, 2+8}
        assert!(r.struct_eq(&ilist(vec![8, 10])));
        // Matrix trace with an explicit combiner.
        let r2 = builtin_tr(
            &[
                mat(vec![vec![1, 2], vec![3, 4]]),
                Value::Symbol("Times".to_string()),
            ],
            &env,
        )
        .unwrap();
        assert!(r2.struct_eq(&int(4)));
    }

    #[test]
    fn dot_higher_rank_and_mismatch() {
        let t = Value::List(vec![
            mat(vec![vec![1, 2], vec![3, 4]]),
            mat(vec![vec![5, 6], vec![7, 8]]),
        ]);
        let r = builtin_dot(&[t, ilist(vec![1, 1])]).unwrap();
        // Contract last axis with vector {1,1}: t[...,0] + t[...,1].
        assert!(r.struct_eq(&mat(vec![vec![3, 7], vec![11, 15]])));
        let bad = builtin_dot(&[mat(vec![vec![1, 2], vec![3, 4]]), ilist(vec![1, 2, 3])]).unwrap();
        assert_eq!(call_head(&bad), Some("Dot"));
    }

    #[test]
    fn inner_generalizes() {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let r = builtin_inner(
            &[
                Value::Symbol("Times".to_string()),
                ilist(vec![1, 2, 3]),
                ilist(vec![4, 5, 6]),
            ],
            &env,
        )
        .unwrap();
        assert!(r.struct_eq(&int(32)));
        let r2 = builtin_inner(
            &[
                Value::Symbol("Times".to_string()),
                Value::Symbol("Plus".to_string()),
                mat(vec![vec![1, 2], vec![3, 4]]),
                ilist(vec![1, 1]),
            ],
            &env,
        )
        .unwrap();
        assert!(r2.struct_eq(&ilist(vec![3, 7])));
    }

    #[test]
    fn structured_matrix_delegates() {
        let id = builtin_identity_matrix(&[int(2)]).unwrap();
        assert!(id.struct_eq(&mat(vec![vec![1, 0], vec![0, 1]])));
        let d = builtin_diagonal_matrix(&[ilist(vec![1, 2])]).unwrap();
        assert!(d.struct_eq(&mat(vec![vec![1, 0], vec![0, 2]])));
        let k = builtin_kronecker_product(&[
            mat(vec![vec![1, 0], vec![0, 1]]),
            mat(vec![vec![1, 2], vec![3, 4]]),
        ])
        .unwrap();
        assert!(k.struct_eq(&mat(vec![
            vec![1, 2, 0, 0],
            vec![3, 4, 0, 0],
            vec![0, 0, 1, 2],
            vec![0, 0, 3, 4]
        ])));
    }
}
