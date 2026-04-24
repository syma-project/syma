//! LinearAlgebra package builtins.
//!
//! Provides core matrix and vector operations: Det, Inverse, Dot,
//! Transpose, IdentityMatrix, Dimensions, Tr, Norm, Cross, LinearSolve,
//! MatrixMultiply.

#![allow(clippy::needless_range_loop)]

use crate::value::{EvalError, Value};
use rug::Float;
use rug::Integer;

use super::arithmetic::{builtin_divide, builtin_plus, builtin_times};

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
                Ok(Value::List(vec![int(items.len() as i64), int(row.len() as i64)]))
            } else {
                // Vector
                Ok(Value::List(vec![int(items.len() as i64)]))
            }
        }
        _ => Ok(Value::List(vec![int(0)])),
    }
}

/// Dot[a, b] — generalized dot product / matrix multiply.
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
            let mut sum = 0.0f64;
            for (ai, bi) in a.iter().zip(b.iter()) {
                let af = to_f64(ai).ok_or_else(|| EvalError::TypeError {
                    expected: "Number".to_string(),
                    got: ai.type_name().to_string(),
                })?;
                let bf = to_f64(bi).ok_or_else(|| EvalError::TypeError {
                    expected: "Number".to_string(),
                    got: bi.type_name().to_string(),
                })?;
                sum += af * bf;
            }
            // Try to return integer if exact
            if sum.fract() == 0.0 && sum.abs() < i64::MAX as f64 {
                Ok(int(sum as i64))
            } else {
                Ok(real(sum))
            }
        }
        (true, false) => {
            // Vector × matrix: treat vector as 1×n row matrix
            // Result is a vector of dot products with each column
            let n = a.len();
            let cols = b[0..1].iter().map(|row| as_list(row).map(|r| r.len())).collect::<Result<Vec<_>, _>>()?;
            let num_cols = cols[0];
            let mut result = Vec::with_capacity(num_cols);
            for j in 0..num_cols {
                let mut sum = 0.0f64;
                for i in 0..n {
                    let row = as_list(&b[i])?;
                    let af = to_f64(&a[i]).ok_or_else(|| EvalError::TypeError {
                        expected: "Number".to_string(),
                        got: a[i].type_name().to_string(),
                    })?;
                    let bf = to_f64(&row[j]).ok_or_else(|| EvalError::TypeError {
                        expected: "Number".to_string(),
                        got: row[j].type_name().to_string(),
                    })?;
                    sum += af * bf;
                }
                if sum.fract() == 0.0 && sum.abs() < i64::MAX as f64 {
                    result.push(int(sum as i64));
                } else {
                    result.push(real(sum));
                }
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
                let mut sum = 0.0f64;
                for (rij, bj) in row.iter().zip(b.iter()) {
                    let rf = to_f64(rij).ok_or_else(|| EvalError::TypeError {
                        expected: "Number".to_string(),
                        got: rij.type_name().to_string(),
                    })?;
                    let bf = to_f64(bj).ok_or_else(|| EvalError::TypeError {
                        expected: "Number".to_string(),
                        got: bj.type_name().to_string(),
                    })?;
                    sum += rf * bf;
                }
                if sum.fract() == 0.0 && sum.abs() < i64::MAX as f64 {
                    result.push(int(sum as i64));
                } else {
                    result.push(real(sum));
                }
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
            for i in 0..m {
                let row_a = as_list(&a[i])?;
                let mut new_row = Vec::with_capacity(n);
                for j in 0..n {
                    let mut sum = 0.0f64;
                    for p in 0..k1 {
                        let row_b = as_list(&b[p])?;
                        let af = to_f64(&row_a[p]).ok_or_else(|| EvalError::TypeError {
                            expected: "Number".to_string(),
                            got: row_a[p].type_name().to_string(),
                        })?;
                        let bf = to_f64(&row_b[j]).ok_or_else(|| EvalError::TypeError {
                            expected: "Number".to_string(),
                            got: row_b[j].type_name().to_string(),
                        })?;
                        sum += af * bf;
                    }
                    if sum.fract() == 0.0 && sum.abs() < i64::MAX as f64 {
                        new_row.push(int(sum as i64));
                    } else {
                        new_row.push(real(sum));
                    }
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
    let (rows, cols) = matrix_dims(m).ok_or_else(|| {
        EvalError::Error("Det: argument must be a matrix".to_string())
    })?;
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
    let (rows, cols) = matrix_dims(m).ok_or_else(|| {
        EvalError::Error("Transpose: argument must be a matrix".to_string())
    })?;
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
    let (rows, cols) = matrix_dims(m).ok_or_else(|| {
        EvalError::Error("Inverse: argument must be a matrix".to_string())
    })?;
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
    let (rows, cols) = matrix_dims(m).ok_or_else(|| {
        EvalError::Error("Tr: argument must be a matrix".to_string())
    })?;
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

/// Cross[a, b] — 3D cross product.
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
    let (a0, a1, a2) = (
        to_f64(&a[0]).ok_or_else(|| EvalError::TypeError { expected: "Number".into(), got: a[0].type_name().into() })?,
        to_f64(&a[1]).ok_or_else(|| EvalError::TypeError { expected: "Number".into(), got: a[1].type_name().into() })?,
        to_f64(&a[2]).ok_or_else(|| EvalError::TypeError { expected: "Number".into(), got: a[2].type_name().into() })?,
    );
    let (b0, b1, b2) = (
        to_f64(&b[0]).ok_or_else(|| EvalError::TypeError { expected: "Number".into(), got: b[0].type_name().into() })?,
        to_f64(&b[1]).ok_or_else(|| EvalError::TypeError { expected: "Number".into(), got: b[1].type_name().into() })?,
        to_f64(&b[2]).ok_or_else(|| EvalError::TypeError { expected: "Number".into(), got: b[2].type_name().into() })?,
    );
    let r0 = a1 * b2 - a2 * b1;
    let r1 = a2 * b0 - a0 * b2;
    let r2 = a0 * b1 - a1 * b0;
    Ok(Value::List(vec![real(r0), real(r1), real(r2)]))
}

/// LinearSolve[A, b] — solve Ax = b via Gaussian elimination.
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

    // Build augmented matrix [A|b] as f64
    let mut aug: Vec<Vec<f64>> = Vec::with_capacity(n);
    for i in 0..n {
        let row = as_list(&a[i])?;
        let mut aug_row = Vec::with_capacity(n + 1);
        for j in 0..n {
            aug_row.push(to_f64(&row[j]).ok_or_else(|| EvalError::TypeError {
                expected: "Number".into(),
                got: row[j].type_name().into(),
            })?);
        }
        aug_row.push(to_f64(&b_list[i]).ok_or_else(|| EvalError::TypeError {
            expected: "Number".into(),
            got: b_list[i].type_name().into(),
        })?);
        aug.push(aug_row);
    }

    // Forward elimination with partial pivoting
    for col in 0..n {
        // Find pivot
        let mut max_val = aug[col][col].abs();
        let mut max_row = col;
        for row in (col + 1)..n {
            if aug[row][col].abs() > max_val {
                max_val = aug[row][col].abs();
                max_row = row;
            }
        }
        if max_val < 1e-15 {
            return Err(EvalError::Error(
                "LinearSolve: singular matrix".to_string(),
            ));
        }
        aug.swap(col, max_row);

        // Eliminate below
        let pivot = aug[col][col];
        for row in (col + 1)..n {
            let factor = aug[row][col] / pivot;
            for j in col..=n {
                aug[row][j] -= factor * aug[col][j];
            }
        }
    }

    // Back substitution
    let mut x = vec![0.0f64; n];
    for i in (0..n).rev() {
        let mut sum = aug[i][n];
        for j in (i + 1)..n {
            sum -= aug[i][j] * x[j];
        }
        x[i] = sum / aug[i][i];
    }

    let result: Vec<Value> = x.iter().map(|&v| real(v)).collect();
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
}

/// Symbol names exported by the LinearAlgebra package.
pub const SYMBOLS: &[&str] = &[
    "Dimensions", "Dot", "MatrixMultiply", "IdentityMatrix", "Det",
    "Inverse", "Transpose", "Tr", "Norm", "Cross", "LinearSolve",
    // Syma-side stubs (loaded from .syma file):
    "Eigenvalues", "MatrixPower", "ArrayFlatten",
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
        list(data
            .into_iter()
            .map(|row| list(row.into_iter().map(int_val).collect()))
            .collect())
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
        let row1 = as_list(&result_list[1]).unwrap();
        assert_eq!(row0[0], int_val(-2));
        assert_eq!(row0[1], int_val(1));
        // row1[0] = 3/2 and row1[1] = -1/2 (as Divide nodes)
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
        let expected = list(vec![real(0.0), real(0.0), real(1.0)]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_linear_solve() {
        // Solve [[2,0],[0,3]] x = [4, 9] → x = [2, 3]
        let a = matrix(vec![vec![2, 0], vec![0, 3]]);
        let b = list(vec![int_val(4), int_val(9)]);
        let result = builtin_linear_solve(&[a, b]).unwrap();
        let result_list = as_list(&result).unwrap();
        assert_eq!(result_list.len(), 2);
        if let (Value::Real(x0), Value::Real(x1)) = (&result_list[0], &result_list[1]) {
            assert!((x0.to_f64() - 2.0).abs() < 1e-10);
            assert!((x1.to_f64() - 3.0).abs() < 1e-10);
        } else {
            panic!("Expected Real values");
        }
    }
}
