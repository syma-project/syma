//! LinearAlgebra package builtins.
//!
//! Provides core matrix and vector operations: Det, Inverse, Dot,
//! Transpose, IdentityMatrix, Dimensions, Tr, Norm, Cross, LinearSolve,
//! MatrixMultiply.

#![allow(clippy::needless_range_loop)]

use crate::value::{EvalError, Value, rational_value};
use rug::Float;
use rug::Integer;
use rug::Rational;

use super::arithmetic::{
    add_values_public, builtin_divide, builtin_plus, builtin_times, mul_values_public,
    sub_values_public,
};

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Extract a list of values, or error.
fn as_list(v: &Value) -> Result<&Vec<Value>, EvalError> {
    match v {
        Value::List(items) => Ok(items),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: v.type_name().to_string(),
        }),
    }
}

/// Convert a Value to an f64 for numerical computation.
fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        Value::Rational(r) => {
            let num = r.numer().to_f64();
            let den = r.denom().to_f64();
            if den != 0.0 { Some(num / den) } else { None }
        }
        _ => None,
    }
}

/// Create a Real value from f64.
fn real(v: f64) -> Value {
    Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, v))
}

/// Create an Integer value.
fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

/// Check if a list is a vector (all elements are scalars, not lists).
fn is_vector(items: &[Value]) -> bool {
    items.iter().all(|v| !matches!(v, Value::List(_)))
}

/// Get matrix dimensions as (rows, cols).
fn matrix_dims(m: &[Value]) -> Option<(usize, usize)> {
    if m.is_empty() {
        return Some((0, 0));
    }
    let rows = m.len();
    if let Value::List(row) = &m[0] {
        Some((rows, row.len()))
    } else {
        None // not a matrix
    }
}

// ── Rational helpers ────────────────────────────────────────────────────────

/// Convert a Value to a rug::Rational (accepts Integer and Rational variants).
fn value_to_rational(v: &Value) -> Result<Rational, EvalError> {
    match v {
        Value::Integer(i) => Ok(Rational::from(i)),
        Value::Rational(r) => Ok(r.as_ref().clone()),
        _ => Err(EvalError::TypeError {
            expected: "Integer or Rational".to_string(),
            got: v.type_name().to_string(),
        }),
    }
}

/// Convert a rug::Rational back to Value, canonicalizing to Integer when den==1.
fn rational_to_value(r: Rational) -> Value {
    let (num, den) = r.into_numer_denom();
    rational_value(num, den)
}

/// Convert a matrix Value (Vec of Lists) to Vec<Vec<Rational>>.
fn value_to_rational_matrix(m: &[Value]) -> Result<Vec<Vec<Rational>>, EvalError> {
    let rows = m.len();
    let mut out = Vec::with_capacity(rows);
    for row_val in m {
        let row = as_list(row_val)?;
        let mut rrow = Vec::with_capacity(row.len());
        for v in row {
            rrow.push(value_to_rational(v)?);
        }
        out.push(rrow);
    }
    Ok(out)
}

/// Convert a Vec<Vec<Rational>> matrix back to Value (List of Lists).
fn rational_matrix_to_value(m: &[Vec<Rational>]) -> Value {
    Value::List(
        m.iter()
            .map(|row| Value::List(row.iter().map(|r| rational_to_value(r.clone())).collect()))
            .collect(),
    )
}

// ── Builtins ────────────────────────────────────────────────────────────────

/// Dimensions[m] — returns {rows, cols} for a matrix or {n} for a vector.
pub fn builtin_dimensions(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Dimensions requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) if items.is_empty() => Ok(Value::List(vec![int(0)])),
        Value::List(items) => {
            if let Value::List(row) = &items[0] {
                // Matrix
                Ok(Value::List(vec![
                    int(items.len() as i64),
                    int(row.len() as i64),
                ]))
            } else {
                // Vector
                Ok(Value::List(vec![int(items.len() as i64)]))
            }
        }
        _ => Ok(Value::List(vec![int(0)])),
    }
}

/// Dot[a, b] — generalized dot product / matrix multiply (exact arithmetic).
pub fn builtin_dot(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Dot requires exactly 2 arguments".to_string(),
        ));
    }
    let a = as_list(&args[0])?;
    let b = as_list(&args[1])?;

    if a.is_empty() || b.is_empty() {
        return Ok(Value::List(vec![]));
    }

    let a_is_vec = is_vector(a);
    let b_is_vec = is_vector(b);

    match (a_is_vec, b_is_vec) {
        (true, true) => {
            // Vector dot product → scalar
            if a.len() != b.len() {
                return Err(EvalError::Error(format!(
                    "Dot: vectors must have same length (got {} and {})",
                    a.len(),
                    b.len()
                )));
            }
            let mut sum = int(0);
            for (ai, bi) in a.iter().zip(b.iter()) {
                let prod = mul_values_public(ai, bi)?;
                sum = add_values_public(&sum, &prod)?;
            }
            Ok(sum)
        }
        (true, false) => {
            // Vector × matrix: treat vector as 1×n row matrix
            let n = a.len();
            let cols = b[0..1]
                .iter()
                .map(|row| as_list(row).map(|r| r.len()))
                .collect::<Result<Vec<_>, _>>()?;
            let num_cols = cols[0];
            let mut result = Vec::with_capacity(num_cols);
            for j in 0..num_cols {
                let mut sum = int(0);
                for i in 0..n {
                    let row = as_list(&b[i])?;
                    let prod = mul_values_public(&a[i], &row[j])?;
                    sum = add_values_public(&sum, &prod)?;
                }
                result.push(sum);
            }
            Ok(Value::List(result))
        }
        (false, true) => {
            // Matrix × vector: m×n dot n → m vector
            let m = a.len();
            let mut result = Vec::with_capacity(m);
            for i in 0..m {
                let row = as_list(&a[i])?;
                if row.len() != b.len() {
                    return Err(EvalError::Error(format!(
                        "Dot: incompatible dimensions (row {} has {} elements, vector has {})",
                        i,
                        row.len(),
                        b.len()
                    )));
                }
                let mut sum = int(0);
                for (rij, bj) in row.iter().zip(b.iter()) {
                    let prod = mul_values_public(rij, bj)?;
                    sum = add_values_public(&sum, &prod)?;
                }
                result.push(sum);
            }
            Ok(Value::List(result))
        }
        (false, false) => {
            // Matrix × matrix
            let (m, k1) = matrix_dims(a).ok_or_else(|| {
                EvalError::Error("Dot: first argument is not a valid matrix".to_string())
            })?;
            let (k2, n) = matrix_dims(b).ok_or_else(|| {
                EvalError::Error("Dot: second argument is not a valid matrix".to_string())
            })?;
            if k1 != k2 {
                return Err(EvalError::Error(format!(
                    "Dot: incompatible matrix dimensions ({}×{} and {}×{})",
                    m, k1, k2, n
                )));
            }
            let mut result = Vec::with_capacity(m);
            // Transpose b for efficient column access: bt[j][i] = b[i][j]
            let bt: Vec<Vec<&Value>> = (0..n)
                .map(|j| (0..k1).map(|i| &as_list(&b[i]).unwrap()[j]).collect())
                .collect();
            for i in 0..m {
                let row_a = as_list(&a[i])?;
                let mut new_row = Vec::with_capacity(n);
                for j in 0..n {
                    let mut sum = int(0);
                    for p in 0..k1 {
                        let prod = mul_values_public(&row_a[p], bt[j][p])?;
                        sum = add_values_public(&sum, &prod)?;
                    }
                    new_row.push(sum);
                }
                result.push(Value::List(new_row));
            }
            Ok(Value::List(result))
        }
    }
}

/// MatrixMultiply[a, b] — alias for Dot.
pub fn builtin_matrix_multiply(args: &[Value]) -> Result<Value, EvalError> {
    builtin_dot(args)
}

/// IdentityMatrix[n] — n×n identity matrix.
pub fn builtin_identity_matrix(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "IdentityMatrix requires exactly 1 argument".to_string(),
        ));
    }
    let n = match &args[0] {
        Value::Integer(i) => i.to_usize().ok_or_else(|| {
            EvalError::Error("IdentityMatrix: argument must be a non-negative integer".to_string())
        })?,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let zero = int(0);
    let one = int(1);
    let mut matrix = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = Vec::with_capacity(n);
        for j in 0..n {
            if i == j {
                row.push(one.clone());
            } else {
                row.push(zero.clone());
            }
        }
        matrix.push(Value::List(row));
    }
    Ok(Value::List(matrix))
}

/// Det[m] — determinant via recursive cofactor expansion.
pub fn builtin_det(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Det requires exactly 1 argument".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    if m.is_empty() {
        return Ok(int(1));
    }
    let (rows, cols) = matrix_dims(m)
        .ok_or_else(|| EvalError::Error("Det: argument must be a matrix".to_string()))?;
    if rows != cols {
        return Err(EvalError::Error(format!(
            "Det: matrix must be square (got {}×{})",
            rows, cols
        )));
    }
    det_recursive(m)
}

fn det_recursive(m: &[Value]) -> Result<Value, EvalError> {
    let n = m.len();
    match n {
        0 => Ok(int(1)),
        1 => {
            let row = as_list(&m[0])?;
            Ok(row[0].clone())
        }
        2 => {
            let row0 = as_list(&m[0])?;
            let row1 = as_list(&m[1])?;
            // det = a*d - b*c
            let ad = builtin_times(&[row0[0].clone(), row1[1].clone()])?;
            let bc = builtin_times(&[row0[1].clone(), row1[0].clone()])?;
            let neg_bc = builtin_times(&[int(-1), bc])?;
            builtin_plus(&[ad, neg_bc])
        }
        _ => {
            // Cofactor expansion along first row
            let row0 = as_list(&m[0])?;
            let mut sum = int(0);
            for j in 0..n {
                let minor = minor_matrix(m, 0, j)?;
                let cofactor = det_recursive(&minor)?;
                let term = builtin_times(&[row0[j].clone(), cofactor])?;
                if j % 2 == 1 {
                    let neg_term = builtin_times(&[int(-1), term])?;
                    sum = builtin_plus(&[sum, neg_term])?;
                } else {
                    sum = builtin_plus(&[sum, term])?;
                }
            }
            Ok(sum)
        }
    }
}

/// Get the minor matrix by removing row `r` and column `c`.
fn minor_matrix(m: &[Value], r: usize, c: usize) -> Result<Vec<Value>, EvalError> {
    let mut result = Vec::with_capacity(m.len() - 1);
    for (i, row_val) in m.iter().enumerate() {
        if i == r {
            continue;
        }
        let row = as_list(row_val)?;
        let mut new_row = Vec::with_capacity(row.len() - 1);
        for (j, val) in row.iter().enumerate() {
            if j == c {
                continue;
            }
            new_row.push(val.clone());
        }
        result.push(Value::List(new_row));
    }
    Ok(result)
}

/// Transpose[m] — transpose a matrix.
pub fn builtin_linalg_transpose(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Transpose requires exactly 1 argument".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    if m.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let (rows, cols) = matrix_dims(m)
        .ok_or_else(|| EvalError::Error("Transpose: argument must be a matrix".to_string()))?;
    let mut result = Vec::with_capacity(cols);
    for j in 0..cols {
        let mut new_row = Vec::with_capacity(rows);
        for i in 0..rows {
            let row = as_list(&m[i])?;
            new_row.push(row[j].clone());
        }
        result.push(Value::List(new_row));
    }
    Ok(Value::List(result))
}

/// Inverse[m] — matrix inverse via adjugate method.
pub fn builtin_inverse(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Inverse requires exactly 1 argument".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    let (rows, cols) = matrix_dims(m)
        .ok_or_else(|| EvalError::Error("Inverse: argument must be a matrix".to_string()))?;
    if rows != cols {
        return Err(EvalError::Error(format!(
            "Inverse: matrix must be square (got {}×{})",
            rows, cols
        )));
    }
    let d = det_recursive(m)?;
    // Check for zero determinant
    match &d {
        Value::Integer(n) if n.is_zero() => {
            return Err(EvalError::Error(
                "Inverse: singular matrix (determinant is zero)".to_string(),
            ));
        }
        Value::Real(r) if r.is_zero() => {
            return Err(EvalError::Error(
                "Inverse: singular matrix (determinant is zero)".to_string(),
            ));
        }
        _ => {}
    }
    // Compute cofactor matrix
    let n = rows;
    let mut cofactors = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = Vec::with_capacity(n);
        for j in 0..n {
            let minor = minor_matrix(m, i, j)?;
            let minor_det = det_recursive(&minor)?;
            let sign = if (i + j) % 2 == 0 { int(1) } else { int(-1) };
            let cofactor = builtin_times(&[sign, minor_det])?;
            row.push(cofactor);
        }
        cofactors.push(Value::List(row));
    }
    // Adjugate = transpose of cofactor matrix
    let adj = builtin_linalg_transpose(&[Value::List(cofactors)])?;
    // Inverse = adjugate / det
    let adj_list = as_list(&adj)?;
    let mut result = Vec::with_capacity(n);
    for row_val in adj_list {
        let row = as_list(row_val)?;
        let mut new_row = Vec::with_capacity(n);
        for val in row {
            new_row.push(builtin_divide(&[val.clone(), d.clone()])?);
        }
        result.push(Value::List(new_row));
    }
    Ok(Value::List(result))
}

/// Tr[m] — trace (sum of diagonal elements).
pub fn builtin_tr(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Tr requires exactly 1 argument".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    if m.is_empty() {
        return Ok(int(0));
    }
    let (rows, cols) = matrix_dims(m)
        .ok_or_else(|| EvalError::Error("Tr: argument must be a matrix".to_string()))?;
    let k = rows.min(cols);
    let mut sum = int(0);
    for i in 0..k {
        let row = as_list(&m[i])?;
        sum = builtin_plus(&[sum, row[i].clone()])?;
    }
    Ok(sum)
}

/// Norm[v] — Euclidean norm for vectors, Frobenius norm for matrices.
pub fn builtin_norm(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Norm requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Ok(int(0));
    }
    let sum_sq = if is_vector(items) {
        // Vector norm
        let mut s = 0.0f64;
        for v in items {
            let f = to_f64(v).ok_or_else(|| EvalError::TypeError {
                expected: "Number".to_string(),
                got: v.type_name().to_string(),
            })?;
            s += f * f;
        }
        s
    } else {
        // Frobenius norm (matrix)
        let mut s = 0.0f64;
        for row_val in items {
            let row = as_list(row_val)?;
            for v in row {
                let f = to_f64(v).ok_or_else(|| EvalError::TypeError {
                    expected: "Number".to_string(),
                    got: v.type_name().to_string(),
                })?;
                s += f * f;
            }
        }
        s
    };
    Ok(real(sum_sq.sqrt()))
}

/// Cross[a, b] — 3D cross product (exact arithmetic).
pub fn builtin_cross(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Cross requires exactly 2 arguments".to_string(),
        ));
    }
    let a = as_list(&args[0])?;
    let b = as_list(&args[1])?;
    if a.len() != 3 || b.len() != 3 {
        return Err(EvalError::Error(
            "Cross requires two 3-element vectors".to_string(),
        ));
    }
    // Cross product: a × b = (a1*b2 - a2*b1, a2*b0 - a0*b2, a0*b1 - a1*b0)
    let r0 = sub_values_public(
        &mul_values_public(&a[1], &b[2])?,
        &mul_values_public(&a[2], &b[1])?,
    )?;
    let r1 = sub_values_public(
        &mul_values_public(&a[2], &b[0])?,
        &mul_values_public(&a[0], &b[2])?,
    )?;
    let r2 = sub_values_public(
        &mul_values_public(&a[0], &b[1])?,
        &mul_values_public(&a[1], &b[0])?,
    )?;
    Ok(Value::List(vec![r0, r1, r2]))
}

/// LinearSolve[A, b] — solve Ax = b via exact rational Gaussian elimination.
pub fn builtin_linear_solve(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "LinearSolve requires exactly 2 arguments".to_string(),
        ));
    }
    let a = as_list(&args[0])?;
    let b_list = as_list(&args[1])?;

    let (n, cols) = matrix_dims(a).ok_or_else(|| {
        EvalError::Error("LinearSolve: first argument must be a matrix".to_string())
    })?;
    if n != cols {
        return Err(EvalError::Error(format!(
            "LinearSolve: coefficient matrix must be square (got {}×{})",
            n, cols
        )));
    }
    if b_list.len() != n {
        return Err(EvalError::Error(format!(
            "LinearSolve: RHS vector length ({}) must match matrix size ({})",
            b_list.len(),
            n
        )));
    }

    // Build augmented matrix [A|b] as Vec<Vec<Rational>>
    let mut aug: Vec<Vec<Rational>> = Vec::with_capacity(n);
    for i in 0..n {
        let row = as_list(&a[i])?;
        let mut aug_row = Vec::with_capacity(n + 1);
        for j in 0..n {
            aug_row.push(value_to_rational(&row[j])?);
        }
        aug_row.push(value_to_rational(&b_list[i])?);
        aug.push(aug_row);
    }

    // Forward elimination with exact pivot detection
    for col in 0..n {
        // Find pivot: first non-zero element at or below current row
        let mut pivot_row = None;
        for row in col..n {
            if aug[row][col] != 0 {
                pivot_row = Some(row);
                break;
            }
        }
        let pr = pivot_row
            .ok_or_else(|| EvalError::Error("LinearSolve: singular matrix".to_string()))?;
        aug.swap(col, pr);

        // Eliminate below
        for row in (col + 1)..n {
            if aug[row][col] == 0 {
                continue;
            }
            // factor = aug[row][col] / aug[col][col]
            let factor = Rational::from(&aug[row][col] / &aug[col][col]);
            for j in col..=n {
                let sub = Rational::from(&factor * &aug[col][j]);
                aug[row][j] = Rational::from(&aug[row][j] - &sub);
            }
        }
    }

    // Back substitution
    let mut x = vec![Rational::from(0); n];
    for i in (0..n).rev() {
        let mut sum = Rational::from(&aug[i][n]);
        for j in (i + 1)..n {
            let prod = Rational::from(&aug[i][j] * &x[j]);
            sum = Rational::from(&sum - &prod);
        }
        x[i] = Rational::from(&sum / &aug[i][i]);
    }

    let result: Vec<Value> = x.into_iter().map(rational_to_value).collect();
    Ok(Value::List(result))
}

// ── f64 matrix helpers (used by eigenvalue / QR routines) ───────────────────

/// Multiply two n×n f64 matrices.
fn mat_mul_f64(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = a.len();
    let mut c = vec![vec![0.0f64; n]; n];
    for i in 0..n {
        for k in 0..n {
            if a[i][k] == 0.0 {
                continue;
            }
            for j in 0..n {
                c[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    c
}

/// Extract an n×n matrix as `Vec<Vec<f64>>`, returning an error on type mismatch.
fn matrix_to_f64(m: &[Value]) -> Result<Vec<Vec<f64>>, EvalError> {
    let n = m.len();
    let mut out = Vec::with_capacity(n);
    for row_val in m {
        let row = as_list(row_val)?;
        let mut frow = Vec::with_capacity(row.len());
        for v in row {
            frow.push(to_f64(v).ok_or_else(|| EvalError::TypeError {
                expected: "Number".into(),
                got: v.type_name().into(),
            })?);
        }
        out.push(frow);
    }
    Ok(out)
}

/// QR decomposition via classical Gram-Schmidt.
/// Returns (Q, R) where A = Q·R, Q orthogonal, R upper-triangular.
fn qr_decompose(a: &[Vec<f64>]) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let n = a.len();
    // Extract columns of A
    let cols: Vec<Vec<f64>> = (0..n).map(|j| (0..n).map(|i| a[i][j]).collect()).collect();

    let mut orth: Vec<Vec<f64>> = Vec::with_capacity(n); // orthonormal columns of Q
    let mut r = vec![vec![0.0f64; n]; n];

    for j in 0..n {
        let mut v = cols[j].clone();
        // Subtract projections onto previous orthonormal columns
        for i in 0..j {
            let dot_val: f64 = cols[j].iter().zip(orth[i].iter()).map(|(a, b)| a * b).sum();
            r[i][j] = dot_val;
            for k in 0..n {
                v[k] -= dot_val * orth[i][k];
            }
        }
        let norm: f64 = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        r[j][j] = norm;
        if norm > 1e-14 {
            orth.push(v.iter().map(|x| x / norm).collect());
        } else {
            // Linearly dependent — fall back to a standard basis vector orthogonalized
            let mut u = vec![0.0f64; n];
            u[j] = 1.0;
            for i in 0..j {
                let d: f64 = u.iter().zip(orth[i].iter()).map(|(a, b)| a * b).sum();
                for k in 0..n {
                    u[k] -= d * orth[i][k];
                }
            }
            let un: f64 = u.iter().map(|x| x * x).sum::<f64>().sqrt();
            if un > 1e-14 {
                orth.push(u.iter().map(|x| x / un).collect());
            } else {
                orth.push(vec![0.0; n]);
            }
        }
    }

    // Build Q: Q[i][j] = orth[j][i]
    let mut q = vec![vec![0.0f64; n]; n];
    for j in 0..n {
        for i in 0..n {
            q[i][j] = orth[j][i];
        }
    }
    (q, r)
}

/// QR iteration to compute eigenvalues and the accumulated eigenvector matrix.
/// Returns (eigenvalues, eigenvector_matrix) where columns of eigenvector_matrix
/// approximate the eigenvectors.
fn qr_iteration(a: &[Vec<f64>]) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut ak: Vec<Vec<f64>> = a.to_vec();
    // Accumulate Q: starts as identity
    let mut q_acc: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            let mut row = vec![0.0f64; n];
            row[i] = 1.0;
            row
        })
        .collect();

    for _ in 0..1000 {
        let (q, r) = qr_decompose(&ak);
        ak = mat_mul_f64(&r, &q);
        q_acc = mat_mul_f64(&q_acc, &q);

        // Check convergence: max absolute off-diagonal element
        let max_off = (0..n)
            .flat_map(|i| (0..n).filter(move |&j| i != j).map(move |j| (i, j)))
            .map(|(i, j)| ak[i][j].abs())
            .fold(0.0f64, f64::max);
        if max_off < 1e-10 {
            break;
        }
    }

    let eigenvalues: Vec<f64> = (0..n).map(|i| ak[i][i]).collect();
    (eigenvalues, q_acc)
}

// ── New builtins ─────────────────────────────────────────────────────────────

/// MatrixPower[m, n] — raise a square matrix to an integer power via binary exponentiation.
/// n=0 returns IdentityMatrix, n<0 inverts after computing the positive power.
pub fn builtin_matrix_power(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MatrixPower requires exactly 2 arguments".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    let (rows, cols) = matrix_dims(m).ok_or_else(|| {
        EvalError::Error("MatrixPower: first argument must be a square matrix".to_string())
    })?;
    if rows != cols {
        return Err(EvalError::Error(format!(
            "MatrixPower: matrix must be square (got {}×{})",
            rows, cols
        )));
    }
    let exp = match &args[1] {
        Value::Integer(i) => i
            .to_i64()
            .ok_or_else(|| EvalError::Error("MatrixPower: exponent out of range".to_string()))?,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };

    if exp == 0 {
        return builtin_identity_matrix(&[int(rows as i64)]);
    }

    let (mut e, invert) = if exp < 0 {
        ((-exp) as u64, true)
    } else {
        (exp as u64, false)
    };

    // Binary exponentiation: result = I, accumulate powers of base
    let mut result = builtin_identity_matrix(&[int(rows as i64)])?;
    let mut base = Value::List(m.to_vec());
    while e > 0 {
        if e & 1 == 1 {
            result = builtin_dot(&[result, base.clone()])?;
        }
        base = builtin_dot(&[base.clone(), base.clone()])?;
        e >>= 1;
    }

    if invert {
        builtin_inverse(&[result])
    } else {
        Ok(result)
    }
}

/// Eigenvalues[m] — numerical eigenvalues via QR iteration.
/// Returns a list of real eigenvalues ordered by descending absolute value.
pub fn builtin_eigenvalues(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Eigenvalues requires exactly 1 argument".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    let (rows, cols) = matrix_dims(m).ok_or_else(|| {
        EvalError::Error("Eigenvalues: argument must be a square matrix".to_string())
    })?;
    if rows != cols {
        return Err(EvalError::Error(format!(
            "Eigenvalues: matrix must be square (got {}×{})",
            rows, cols
        )));
    }
    let a = matrix_to_f64(m)?;
    let (mut eigenvalues, _) = qr_iteration(&a);
    // Sort by descending absolute value (Wolfram convention)
    eigenvalues.sort_by(|a, b| {
        b.abs()
            .partial_cmp(&a.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(Value::List(eigenvalues.into_iter().map(real).collect()))
}

/// Eigenvectors[m] — numerical eigenvectors via QR iteration.
/// Returns a matrix whose rows are the eigenvectors (same ordering as Eigenvalues).
pub fn builtin_eigenvectors(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Eigenvectors requires exactly 1 argument".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    let (rows, cols) = matrix_dims(m).ok_or_else(|| {
        EvalError::Error("Eigenvectors: argument must be a square matrix".to_string())
    })?;
    if rows != cols {
        return Err(EvalError::Error(format!(
            "Eigenvectors: matrix must be square (got {}×{})",
            rows, cols
        )));
    }
    let a = matrix_to_f64(m)?;
    let n = rows;
    let (eigenvalues, q_acc) = qr_iteration(&a);

    // Sort indices by descending |eigenvalue|
    let mut indices: Vec<usize> = (0..n).collect();
    indices.sort_by(|&i, &j| {
        eigenvalues[j]
            .abs()
            .partial_cmp(&eigenvalues[i].abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Columns of q_acc are eigenvectors; return as rows in sorted order
    let result = indices
        .into_iter()
        .map(|col_idx| {
            Value::List(
                (0..n)
                    .map(|row_idx| real(q_acc[row_idx][col_idx]))
                    .collect(),
            )
        })
        .collect();
    Ok(Value::List(result))
}

/// ArrayFlatten[blocks] — assemble a block matrix from a matrix-of-matrices.
/// E.g. ArrayFlatten[{{A, B}, {C, D}}] → single flat matrix.
pub fn builtin_array_flatten(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ArrayFlatten requires exactly 1 argument".to_string(),
        ));
    }
    let block_rows = as_list(&args[0])?;
    if block_rows.is_empty() {
        return Ok(Value::List(vec![]));
    }

    let mut result_rows: Vec<Value> = Vec::new();

    for block_row_val in block_rows {
        let block_row = as_list(block_row_val)?;
        if block_row.is_empty() {
            continue;
        }
        // Each element of block_row must be a matrix; collect their rows
        // All matrices in a block-row must have the same number of rows
        let first_block = as_list(&block_row[0])?;
        let num_rows = first_block.len();

        for row_idx in 0..num_rows {
            let mut combined_row: Vec<Value> = Vec::new();
            for block_val in block_row {
                let block = as_list(block_val)?;
                if block.len() != num_rows {
                    return Err(EvalError::Error(format!(
                        "ArrayFlatten: inconsistent block row heights ({} vs {})",
                        block.len(),
                        num_rows
                    )));
                }
                let block_matrix_row = as_list(&block[row_idx])?;
                combined_row.extend_from_slice(block_matrix_row);
            }
            result_rows.push(Value::List(combined_row));
        }
    }

    Ok(Value::List(result_rows))
}

/// ZeroMatrix[n] or ZeroMatrix[{m, n}] — matrix of zeros.
pub fn builtin_zero_matrix(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ZeroMatrix requires exactly 1 argument".to_string(),
        ));
    }
    let (nrows, ncols) = match &args[0] {
        Value::Integer(i) => {
            let n = i.to_usize().ok_or_else(|| {
                EvalError::Error("ZeroMatrix: argument must be a non-negative integer".to_string())
            })?;
            (n, n)
        }
        Value::List(dims) if dims.len() == 2 => {
            let r = match &dims[0] {
                Value::Integer(i) => i.to_usize().ok_or_else(|| {
                    EvalError::Error(
                        "ZeroMatrix: dimensions must be non-negative integers".to_string(),
                    )
                })?,
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "Integer".into(),
                        got: dims[0].type_name().into(),
                    });
                }
            };
            let c = match &dims[1] {
                Value::Integer(i) => i.to_usize().ok_or_else(|| {
                    EvalError::Error(
                        "ZeroMatrix: dimensions must be non-negative integers".to_string(),
                    )
                })?,
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "Integer".into(),
                        got: dims[1].type_name().into(),
                    });
                }
            };
            (r, c)
        }
        _ => {
            return Err(EvalError::Error(
                "ZeroMatrix: argument must be an integer or {m, n} list".to_string(),
            ));
        }
    };
    let zero = int(0);
    let matrix = (0..nrows)
        .map(|_| Value::List(vec![zero.clone(); ncols]))
        .collect();
    Ok(Value::List(matrix))
}

/// DiagonalMatrix[v] — n×n matrix with vector v on the diagonal.
pub fn builtin_diagonal_matrix(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "DiagonalMatrix requires exactly 1 argument".to_string(),
        ));
    }
    let v = as_list(&args[0])?;
    let n = v.len();
    let zero = int(0);
    let matrix = (0..n)
        .map(|i| {
            let row = (0..n)
                .map(|j| if i == j { v[i].clone() } else { zero.clone() })
                .collect();
            Value::List(row)
        })
        .collect();
    Ok(Value::List(matrix))
}

/// UnitVector[n, k] — length-n vector with 1 at position k (1-indexed).
pub fn builtin_unit_vector(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "UnitVector requires exactly 2 arguments".to_string(),
        ));
    }
    let n = match &args[0] {
        Value::Integer(i) => i.to_usize().ok_or_else(|| {
            EvalError::Error("UnitVector: first argument must be a positive integer".to_string())
        })?,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".into(),
                got: args[0].type_name().into(),
            });
        }
    };
    let k = match &args[1] {
        Value::Integer(i) => i.to_usize().ok_or_else(|| {
            EvalError::Error("UnitVector: second argument must be a positive integer".to_string())
        })?,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".into(),
                got: args[1].type_name().into(),
            });
        }
    };
    if k == 0 || k > n {
        return Err(EvalError::Error(format!(
            "UnitVector: index {} out of range for length-{} vector (1-indexed)",
            k, n
        )));
    }
    let zero = int(0);
    let one = int(1);
    let v = (1..=n)
        .map(|i| if i == k { one.clone() } else { zero.clone() })
        .collect();
    Ok(Value::List(v))
}

/// RowReduce[m] — reduced row echelon form via exact rational Gaussian elimination.
/// Returns the RREF of matrix m using exact rational arithmetic.
pub fn builtin_row_reduce(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "RowReduce requires exactly 1 argument".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    if m.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let (rows, cols) = matrix_dims(m)
        .ok_or_else(|| EvalError::Error("RowReduce: argument must be a matrix".to_string()))?;
    let a = value_to_rational_matrix(m)?;
    let (rref, _) = rref_rational(&a, rows, cols);
    Ok(rational_matrix_to_value(&rref))
}

/// MatrixRank[m] — rank of a matrix (number of linearly independent rows/columns).
/// Computed via exact rational Gaussian elimination.
pub fn builtin_matrix_rank(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "MatrixRank requires exactly 1 argument".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    if m.is_empty() {
        return Ok(int(0));
    }
    let dims = matrix_dims(m)
        .ok_or_else(|| EvalError::Error("MatrixRank: argument must be a matrix".to_string()))?;
    let a = value_to_rational_matrix(m)?;
    let (_, rank) = rref_rational(&a, dims.0, dims.1);
    Ok(int(rank as i64))
}

/// Helper: compute RREF and rank for a Vec<Vec<Rational>> matrix using exact arithmetic.
/// Pivot detection is exact: first non-zero element.
fn rref_rational(a: &[Vec<Rational>], rows: usize, cols: usize) -> (Vec<Vec<Rational>>, usize) {
    let mut m = a.to_vec();
    let mut rank = 0;

    for col in 0..cols {
        // Find pivot: first non-zero element at or below current row
        let mut pivot_row = None;
        for r in rank..rows {
            if m[r][col] != 0 {
                pivot_row = Some(r);
                break;
            }
        }

        if let Some(pr) = pivot_row {
            m.swap(rank, pr);

            // Scale pivot row to make pivot = 1
            let pivot = m[rank][col].clone();
            for j in col..cols {
                m[rank][j] = Rational::from(&m[rank][j] / &pivot);
            }

            // Eliminate in all other rows
            for r in 0..rows {
                if r != rank && m[r][col] != 0 {
                    let factor = m[r][col].clone();
                    for j in col..cols {
                        let prod = Rational::from(&factor * &m[rank][j]);
                        let diff = Rational::from(&m[r][j] - &prod);
                        m[r][j] = diff;
                    }
                }
            }

            rank += 1;
        }
    }

    (m, rank)
}

/// NullSpace[m] — basis vectors for the null space of matrix m.
/// Returns exact rational basis vectors satisfying m·v = 0.
pub fn builtin_null_space(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "NullSpace requires exactly 1 argument".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    if m.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let (rows, cols) = matrix_dims(m)
        .ok_or_else(|| EvalError::Error("NullSpace: argument must be a matrix".to_string()))?;
    let a = value_to_rational_matrix(m)?;
    let (rref, rank) = rref_rational(&a, rows, cols);

    if rank == cols {
        // Full column rank — null space is trivial
        return Ok(Value::List(vec![]));
    }

    // Identify pivot columns
    let mut pivot_cols = vec![false; cols];
    let mut r = 0;
    for c in 0..cols {
        if r < rows && rref[r][c] == 1 {
            pivot_cols[c] = true;
            r += 1;
        }
    }

    // Build basis vectors
    let mut basis = Vec::new();
    for free_col in 0..cols {
        if pivot_cols[free_col] {
            continue;
        }
        let mut v = vec![Rational::from(0); cols];
        v[free_col] = Rational::from(1);
        // For each pivot row, back-substitute
        r = 0;
        for c in 0..cols {
            if pivot_cols[c] {
                if r < rows {
                    // v[c] = -rref[r][free_col]
                    v[c] = Rational::from(&Rational::from(0) - &rref[r][free_col]);
                }
                r += 1;
            }
        }
        basis.push(Value::List(v.into_iter().map(rational_to_value).collect()));
    }

    Ok(Value::List(basis))
}

/// Row[m, i] — extract the i-th row (1-indexed).
pub fn builtin_row(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Row requires exactly 2 arguments: matrix and index".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    let idx = match &args[1] {
        Value::Integer(i) => {
            let n = i.to_usize().ok_or_else(|| {
                EvalError::Error("Row: index must be a positive integer".to_string())
            })?;
            if n == 0 || n > m.len() {
                return Err(EvalError::Error(format!(
                    "Row: index {} out of range (matrix has {} rows, 1-indexed)",
                    n,
                    m.len()
                )));
            }
            n - 1
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    Ok(m[idx].clone())
}

/// Column[m, j] — extract the j-th column (1-indexed) as a vector.
pub fn builtin_column(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Column requires exactly 2 arguments: matrix and index".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    if m.is_empty() {
        return Err(EvalError::Error("Column: matrix is empty".to_string()));
    }
    let (rows, cols) = matrix_dims(m)
        .ok_or_else(|| EvalError::Error("Column: first argument must be a matrix".to_string()))?;
    let idx = match &args[1] {
        Value::Integer(i) => {
            let n = i.to_usize().ok_or_else(|| {
                EvalError::Error("Column: index must be a positive integer".to_string())
            })?;
            if n == 0 || n > cols {
                return Err(EvalError::Error(format!(
                    "Column: index {} out of range (matrix has {} columns, 1-indexed)",
                    n, cols
                )));
            }
            n - 1
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let col: Vec<Value> = (0..rows)
        .map(|i| {
            let row = as_list(&m[i]).unwrap();
            row[idx].clone()
        })
        .collect();
    Ok(Value::List(col))
}

/// KroneckerProduct[a, b] — Kronecker product of two matrices (exact arithmetic).
/// Returns a block matrix where a[i][j] * b is the (i,j)-th block.
pub fn builtin_kronecker_product(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KroneckerProduct requires exactly 2 arguments".to_string(),
        ));
    }
    let a_list = as_list(&args[0])?;
    let b_list = as_list(&args[1])?;
    let (ar, ac) = matrix_dims(a_list).ok_or_else(|| {
        EvalError::Error("KroneckerProduct: arguments must be matrices".to_string())
    })?;
    let (br, bc) = matrix_dims(b_list).ok_or_else(|| {
        EvalError::Error("KroneckerProduct: arguments must be matrices".to_string())
    })?;

    let mut result = Vec::with_capacity(ar * br);
    for i in 0..ar {
        let a_row = as_list(&a_list[i])?;
        for bi in 0..br {
            let mut new_row = Vec::with_capacity(ac * bc);
            for j in 0..ac {
                for bj in 0..bc {
                    new_row.push(mul_values_public(&a_row[j], &as_list(&b_list[bi])?[bj])?);
                }
            }
            result.push(Value::List(new_row));
        }
    }
    Ok(Value::List(result))
}

// ── Rational matrix helpers (for PseudoInverse) ──────────────────────────

/// Multiply two matrices of Rational values.
fn mat_mul_rational(a: &[Vec<Rational>], b: &[Vec<Rational>]) -> Vec<Vec<Rational>> {
    let m = a.len();
    if m == 0 {
        return vec![];
    }
    let k = b.len();
    if k == 0 {
        return (0..m).map(|_| Vec::new()).collect();
    }
    let n = b[0].len();
    let mut c = vec![vec![Rational::from(0); n]; m];
    for i in 0..m {
        for p in 0..k {
            if a[i][p] == 0 {
                continue;
            }
            for j in 0..n {
                let prod = Rational::from(&a[i][p] * &b[p][j]);
                c[i][j] = Rational::from(&c[i][j] + &prod);
            }
        }
    }
    c
}

/// Transpose a matrix of Rational values.
fn transpose_rational(a: &[Vec<Rational>]) -> Vec<Vec<Rational>> {
    if a.is_empty() {
        return vec![];
    }
    let rows = a.len();
    let cols = a[0].len();
    (0..cols)
        .map(|j| (0..rows).map(|i| a[i][j].clone()).collect())
        .collect()
}

/// PseudoInverse[m] — Moore-Penrose pseudoinverse (exact rational arithmetic).
/// For full-rank matrices, computes A⁺ = (AᵀA)⁻¹Aᵀ.
pub fn builtin_pseudo_inverse(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PseudoInverse requires exactly 1 argument".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    if m.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let (_rows, _cols) = matrix_dims(m)
        .ok_or_else(|| EvalError::Error("PseudoInverse: argument must be a matrix".to_string()))?;

    // Convert to Vec<Vec<Rational>>
    let a = value_to_rational_matrix(m)?;

    // Compute A^T (cols × rows)
    let at = transpose_rational(&a);

    // Compute A^T * A (cols × cols)
    let ata = mat_mul_rational(&at, &a);

    // Compute (A^T A)^(-1) using exact rational Gaussian elimination
    let ata_inv = matrix_inverse_rational(&ata).ok_or_else(|| {
        EvalError::Error("PseudoInverse: AᵀA is singular, cannot compute pseudoinverse".to_string())
    })?;

    // A⁺ = (A^T A)^(-1) * A^T  (cols × rows)
    let result = mat_mul_rational(&ata_inv, &at);

    Ok(rational_matrix_to_value(&result))
}

/// Compute the inverse of a square matrix via exact rational Gaussian elimination.
fn matrix_inverse_rational(a: &[Vec<Rational>]) -> Option<Vec<Vec<Rational>>> {
    let n = a.len();
    if n == 0 {
        return None;
    }
    let zero = Rational::from(0);
    let one = Rational::from(1);

    // Augment with identity: [A | I]
    let mut aug = vec![vec![zero.clone(); 2 * n]; n];
    for i in 0..n {
        for j in 0..n {
            aug[i][j] = a[i][j].clone();
        }
        aug[i][n + i] = one.clone();
    }

    // Forward elimination with exact pivot detection
    for col in 0..n {
        // Find pivot: first non-zero element at or below current row
        let mut pivot_row = None;
        for row in col..n {
            if aug[row][col] != 0 {
                pivot_row = Some(row);
                break;
            }
        }
        let pr = pivot_row?; // Singular
        aug.swap(col, pr);

        // Scale pivot row
        let pivot = aug[col][col].clone();
        for j in 0..2 * n {
            aug[col][j] = Rational::from(&aug[col][j] / &pivot);
        }

        // Eliminate in other rows
        for row in 0..n {
            if row != col && aug[row][col] != 0 {
                let factor = aug[row][col].clone();
                for j in 0..2 * n {
                    let prod = Rational::from(&factor * &aug[col][j]);
                    aug[row][j] = Rational::from(&aug[row][j] - &prod);
                }
            }
        }
    }

    // Extract inverse from the right half
    let mut inv = vec![vec![zero; n]; n];
    for i in 0..n {
        for j in 0..n {
            inv[i][j] = aug[i][n + j].clone();
        }
    }
    Some(inv)
}

/// VectorAngle[u, v] — angle between two vectors in radians.
/// Returns arccos((u·v) / (|u|·|v|)).
pub fn builtin_vector_angle(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "VectorAngle requires exactly 2 arguments".to_string(),
        ));
    }
    let u = as_list(&args[0])?;
    let v = as_list(&args[1])?;
    if u.len() != v.len() {
        return Err(EvalError::Error(format!(
            "VectorAngle: vectors must have the same length (got {} and {})",
            u.len(),
            v.len()
        )));
    }
    if u.is_empty() {
        return Err(EvalError::Error(
            "VectorAngle: vectors must not be empty".to_string(),
        ));
    }

    // Convert to f64
    let u_f64: Result<Vec<f64>, _> = u
        .iter()
        .map(|x| {
            to_f64(x).ok_or_else(|| EvalError::TypeError {
                expected: "Number".into(),
                got: x.type_name().into(),
            })
        })
        .collect();
    let u_f64 = u_f64?;
    let v_f64: Result<Vec<f64>, _> = v
        .iter()
        .map(|x| {
            to_f64(x).ok_or_else(|| EvalError::TypeError {
                expected: "Number".into(),
                got: x.type_name().into(),
            })
        })
        .collect();
    let v_f64 = v_f64?;

    let dot: f64 = u_f64.iter().zip(v_f64.iter()).map(|(a, b)| a * b).sum();
    let norm_u: f64 = u_f64.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_v: f64 = v_f64.iter().map(|x| x * x).sum::<f64>().sqrt();

    if norm_u < 1e-15 || norm_v < 1e-15 {
        return Err(EvalError::Error(
            "VectorAngle: zero-length vector".to_string(),
        ));
    }

    let cos_theta = (dot / (norm_u * norm_v)).clamp(-1.0, 1.0);
    Ok(real(cos_theta.acos()))
}

/// Minors[m] — matrix of minors (determinant of submatrix after removing row i, column j).
/// Returns a matrix of the same dimensions.
pub fn builtin_minors(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Minors requires exactly 1 argument".to_string(),
        ));
    }
    let m = as_list(&args[0])?;
    if m.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let (rows, cols) = matrix_dims(m)
        .ok_or_else(|| EvalError::Error("Minors: argument must be a matrix".to_string()))?;

    let mut result = Vec::with_capacity(rows);
    for i in 0..rows {
        let mut new_row = Vec::with_capacity(cols);
        for j in 0..cols {
            let minor = minor_matrix(m, i, j)?;
            let d = det_recursive(&minor)?;
            new_row.push(d);
        }
        result.push(Value::List(new_row));
    }
    Ok(Value::List(result))
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register all LinearAlgebra builtins in the environment.
pub fn register(env: &crate::env::Env) {
    use super::register_builtin;
    register_builtin(env, "Dimensions", builtin_dimensions);
    register_builtin(env, "Dot", builtin_dot);
    register_builtin(env, "MatrixMultiply", builtin_matrix_multiply);
    register_builtin(env, "IdentityMatrix", builtin_identity_matrix);
    register_builtin(env, "Det", builtin_det);
    register_builtin(env, "Inverse", builtin_inverse);
    register_builtin(env, "Transpose", builtin_linalg_transpose);
    register_builtin(env, "Tr", builtin_tr);
    register_builtin(env, "Norm", builtin_norm);
    register_builtin(env, "Cross", builtin_cross);
    register_builtin(env, "LinearSolve", builtin_linear_solve);
    register_builtin(env, "MatrixPower", builtin_matrix_power);
    register_builtin(env, "Eigenvalues", builtin_eigenvalues);
    register_builtin(env, "Eigenvectors", builtin_eigenvectors);
    register_builtin(env, "ArrayFlatten", builtin_array_flatten);
    register_builtin(env, "ZeroMatrix", builtin_zero_matrix);
    register_builtin(env, "DiagonalMatrix", builtin_diagonal_matrix);
    register_builtin(env, "UnitVector", builtin_unit_vector);
    register_builtin(env, "RowReduce", builtin_row_reduce);
    register_builtin(env, "MatrixRank", builtin_matrix_rank);
    register_builtin(env, "NullSpace", builtin_null_space);
    register_builtin(env, "Row", builtin_row);
    register_builtin(env, "Column", builtin_column);
    register_builtin(env, "KroneckerProduct", builtin_kronecker_product);
    register_builtin(env, "VectorAngle", builtin_vector_angle);
    register_builtin(env, "PseudoInverse", builtin_pseudo_inverse);
    register_builtin(env, "Minors", builtin_minors);
}

/// Symbol names exported by the LinearAlgebra package.
pub const SYMBOLS: &[&str] = &[
    "Dimensions",
    "Dot",
    "MatrixMultiply",
    "IdentityMatrix",
    "Det",
    "Inverse",
    "Transpose",
    "Tr",
    "Norm",
    "Cross",
    "LinearSolve",
    "MatrixPower",
    "Eigenvalues",
    "Eigenvectors",
    "ArrayFlatten",
    "ZeroMatrix",
    "DiagonalMatrix",
    "UnitVector",
    "RowReduce",
    "MatrixRank",
    "NullSpace",
    "Row",
    "Column",
    "KroneckerProduct",
    "VectorAngle",
    "PseudoInverse",
    "Minors",
];

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn int_val(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn list(vals: Vec<Value>) -> Value {
        Value::List(vals)
    }

    fn matrix(data: Vec<Vec<i64>>) -> Value {
        list(
            data.into_iter()
                .map(|row| list(row.into_iter().map(int_val).collect()))
                .collect(),
        )
    }

    #[test]
    fn test_dimensions_matrix() {
        let m = matrix(vec![vec![1, 2, 3], vec![4, 5, 6]]);
        let result = builtin_dimensions(&[m]).unwrap();
        assert_eq!(result, list(vec![int_val(2), int_val(3)]));
    }

    #[test]
    fn test_dimensions_vector() {
        let v = list(vec![int_val(1), int_val(2), int_val(3)]);
        let result = builtin_dimensions(&[v]).unwrap();
        assert_eq!(result, list(vec![int_val(3)]));
    }

    #[test]
    fn test_identity_matrix() {
        let result = builtin_identity_matrix(&[int_val(3)]).unwrap();
        let expected = matrix(vec![vec![1, 0, 0], vec![0, 1, 0], vec![0, 0, 1]]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_dot_vector() {
        let a = list(vec![int_val(1), int_val(2), int_val(3)]);
        let b = list(vec![int_val(4), int_val(5), int_val(6)]);
        let result = builtin_dot(&[a, b]).unwrap();
        assert_eq!(result, int_val(32)); // 1*4 + 2*5 + 3*6 = 32
    }

    #[test]
    fn test_dot_matrix_identity() {
        let m = matrix(vec![vec![1, 2], vec![3, 4]]);
        let id = matrix(vec![vec![1, 0], vec![0, 1]]);
        let result = builtin_dot(&[m, id]).unwrap();
        let expected = matrix(vec![vec![1, 2], vec![3, 4]]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_dot_matrix_multiply() {
        let a = matrix(vec![vec![1, 2], vec![3, 4]]);
        let b = matrix(vec![vec![5, 6], vec![7, 8]]);
        let result = builtin_dot(&[a, b]).unwrap();
        // [[1*5+2*7, 1*6+2*8], [3*5+4*7, 3*6+4*8]] = [[19, 22], [43, 50]]
        let expected = matrix(vec![vec![19, 22], vec![43, 50]]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_dot_matrix_vector() {
        let m = matrix(vec![vec![1, 2], vec![3, 4]]);
        let v = list(vec![int_val(5), int_val(6)]);
        let result = builtin_dot(&[m, v]).unwrap();
        // [1*5+2*6, 3*5+4*6] = [17, 39]
        let expected = list(vec![int_val(17), int_val(39)]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_det_2x2() {
        let m = matrix(vec![vec![1, 2], vec![3, 4]]);
        let result = builtin_det(&[m]).unwrap();
        assert_eq!(result, int_val(-2)); // 1*4 - 2*3 = -2
    }

    #[test]
    fn test_det_3x3() {
        let m = matrix(vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]]);
        let result = builtin_det(&[m]).unwrap();
        assert_eq!(result, int_val(0)); // singular matrix
    }

    #[test]
    fn test_det_3x3_nonsingular() {
        let m = matrix(vec![vec![2, 1, 3], vec![0, -1, 2], vec![1, 0, 1]]);
        let result = builtin_det(&[m]).unwrap();
        // det = 2*(-1*1 - 2*0) - 1*(0*1 - 2*1) + 3*(0*0 - (-1)*1)
        //     = 2*(-1) - 1*(-2) + 3*(1) = -2 + 2 + 3 = 3
        assert_eq!(result, int_val(3));
    }

    #[test]
    fn test_transpose() {
        let m = matrix(vec![vec![1, 2, 3], vec![4, 5, 6]]);
        let result = builtin_linalg_transpose(&[m]).unwrap();
        let expected = matrix(vec![vec![1, 4], vec![2, 5], vec![3, 6]]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_inverse_2x2() {
        let m = matrix(vec![vec![1, 2], vec![3, 4]]);
        let result = builtin_inverse(&[m]).unwrap();
        // Inverse = 1/(-2) * [[4, -2], [-3, 1]] = [[-2, 1], [1.5, -0.5]]
        let result_list = as_list(&result).unwrap();
        assert_eq!(result_list.len(), 2);
        let row0 = as_list(&result_list[0]).unwrap();
        assert_eq!(row0[0], int_val(-2));
        assert_eq!(row0[1], int_val(1));
    }

    #[test]
    fn test_inverse_identity() {
        let id = matrix(vec![vec![1, 0], vec![0, 1]]);
        let result = builtin_inverse(&[id.clone()]).unwrap();
        assert_eq!(result, id);
    }

    #[test]
    fn test_inverse_singular() {
        let m = matrix(vec![vec![1, 2], vec![2, 4]]);
        assert!(builtin_inverse(&[m]).is_err());
    }

    #[test]
    fn test_tr() {
        let m = matrix(vec![vec![1, 2], vec![3, 4]]);
        let result = builtin_tr(&[m]).unwrap();
        assert_eq!(result, int_val(5)); // 1 + 4
    }

    #[test]
    fn test_norm_vector() {
        let v = list(vec![int_val(3), int_val(4)]);
        let result = builtin_norm(&[v]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 5.0).abs() < 1e-10);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_cross() {
        let a = list(vec![int_val(1), int_val(0), int_val(0)]);
        let b = list(vec![int_val(0), int_val(1), int_val(0)]);
        let result = builtin_cross(&[a, b]).unwrap();
        // i×j = k = (0,0,1)
        let expected = list(vec![int_val(0), int_val(0), int_val(1)]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_linear_solve() {
        // Solve [[2,0],[0,3]] x = [4, 9] → x = [2, 3] (exact)
        let a = matrix(vec![vec![2, 0], vec![0, 3]]);
        let b = list(vec![int_val(4), int_val(9)]);
        let result = builtin_linear_solve(&[a, b]).unwrap();
        let result_list = as_list(&result).unwrap();
        assert_eq!(result_list.len(), 2);
        assert_eq!(result_list[0], int_val(2));
        assert_eq!(result_list[1], int_val(3));
    }

    #[test]
    fn test_matrix_power_zero() {
        let m = matrix(vec![vec![1, 2], vec![3, 4]]);
        let result = builtin_matrix_power(&[m, int_val(0)]).unwrap();
        assert_eq!(result, matrix(vec![vec![1, 0], vec![0, 1]]));
    }

    #[test]
    fn test_matrix_power_one() {
        let m = matrix(vec![vec![1, 2], vec![3, 4]]);
        let result = builtin_matrix_power(&[m.clone(), int_val(1)]).unwrap();
        assert_eq!(result, m);
    }

    #[test]
    fn test_matrix_power_two() {
        let m = matrix(vec![vec![1, 1], vec![0, 1]]);
        let result = builtin_matrix_power(&[m, int_val(2)]).unwrap();
        // [[1,1],[0,1]]^2 = [[1,2],[0,1]]
        assert_eq!(result, matrix(vec![vec![1, 2], vec![0, 1]]));
    }

    #[test]
    fn test_matrix_power_negative() {
        let m = matrix(vec![vec![2, 0], vec![0, 4]]);
        let result = builtin_matrix_power(&[m, int_val(-1)]).unwrap();
        // Inverse of diagonal [[2,0],[0,4]] = [[0.5,0],[0,0.25]]
        let rows = as_list(&result).unwrap();
        let r0 = as_list(&rows[0]).unwrap();
        let r1 = as_list(&rows[1]).unwrap();
        assert!((to_f64(&r0[0]).unwrap() - 0.5).abs() < 1e-10);
        assert!((to_f64(&r0[1]).unwrap()).abs() < 1e-10);
        assert!((to_f64(&r1[0]).unwrap()).abs() < 1e-10);
        assert!((to_f64(&r1[1]).unwrap() - 0.25).abs() < 1e-10);
    }

    #[test]
    fn test_eigenvalues_diagonal() {
        // Diagonal matrix: eigenvalues are the diagonal entries
        let m = list(vec![
            list(vec![real(3.0), real(0.0)]),
            list(vec![real(0.0), real(1.0)]),
        ]);
        let result = builtin_eigenvalues(&[m]).unwrap();
        let evals = as_list(&result).unwrap();
        assert_eq!(evals.len(), 2);
        let v0 = to_f64(&evals[0]).unwrap();
        let v1 = to_f64(&evals[1]).unwrap();
        // Sorted by descending abs value: 3.0, 1.0
        assert!((v0 - 3.0).abs() < 1e-6);
        assert!((v1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_eigenvalues_symmetric() {
        // [[2,1],[1,2]] has eigenvalues 3 and 1
        let m = list(vec![
            list(vec![real(2.0), real(1.0)]),
            list(vec![real(1.0), real(2.0)]),
        ]);
        let result = builtin_eigenvalues(&[m]).unwrap();
        let evals = as_list(&result).unwrap();
        assert_eq!(evals.len(), 2);
        let v0 = to_f64(&evals[0]).unwrap();
        let v1 = to_f64(&evals[1]).unwrap();
        assert!((v0 - 3.0).abs() < 1e-6);
        assert!((v1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_array_flatten() {
        // Block matrix {{I2, 2*I2}, {3*I2, 4*I2}} → 4×4
        let i2 = matrix(vec![vec![1, 0], vec![0, 1]]);
        let a = matrix(vec![vec![2, 0], vec![0, 2]]);
        let b = matrix(vec![vec![3, 0], vec![0, 3]]);
        let c = matrix(vec![vec![4, 0], vec![0, 4]]);
        let blocks = list(vec![list(vec![i2.clone(), a]), list(vec![b, c])]);
        let result = builtin_array_flatten(&[blocks]).unwrap();
        let rows = as_list(&result).unwrap();
        assert_eq!(rows.len(), 4);
        // First row: [1, 0, 2, 0]
        let r0 = as_list(&rows[0]).unwrap();
        assert_eq!(r0.len(), 4);
        assert_eq!(r0[0], int_val(1));
        assert_eq!(r0[2], int_val(2));
    }

    #[test]
    fn test_zero_matrix_square() {
        let result = builtin_zero_matrix(&[int_val(2)]).unwrap();
        assert_eq!(result, matrix(vec![vec![0, 0], vec![0, 0]]));
    }

    #[test]
    fn test_zero_matrix_rect() {
        let result = builtin_zero_matrix(&[list(vec![int_val(2), int_val(3)])]).unwrap();
        let rows = as_list(&result).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(as_list(&rows[0]).unwrap().len(), 3);
    }

    #[test]
    fn test_diagonal_matrix() {
        let v = list(vec![int_val(1), int_val(2), int_val(3)]);
        let result = builtin_diagonal_matrix(&[v]).unwrap();
        let expected = matrix(vec![vec![1, 0, 0], vec![0, 2, 0], vec![0, 0, 3]]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_unit_vector() {
        let result = builtin_unit_vector(&[int_val(3), int_val(2)]).unwrap();
        assert_eq!(result, list(vec![int_val(0), int_val(1), int_val(0)]));
    }

    #[test]
    fn test_unit_vector_out_of_range() {
        assert!(builtin_unit_vector(&[int_val(3), int_val(0)]).is_err());
        assert!(builtin_unit_vector(&[int_val(3), int_val(4)]).is_err());
    }
}
