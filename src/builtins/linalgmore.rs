//! linear algebra: decompositions, characteristic polynomial, structured matrices, *MatrixQ, Normalize.
//!
//! Gap-fill module; see the individual builtin docs for semantics.

#![allow(clippy::needless_range_loop)]

use crate::env::Env;
use crate::value::{EvalError, Value, rational_value};
use rug::Integer;
use rug::Rational;

use super::arithmetic::{
    add_values_public, builtin_divide, builtin_times, mul_values_public, sub_values_public,
};

/// Tolerance for floating point comparisons in this module.
const EPS: f64 = 1e-10;

// ── Generic scalar abstraction ──────────────────────────────────────────────
//
// Allows the exact (rational) and floating paths to share the decomposition /
// determinant / characteristic-polynomial code.

trait Scalar: Clone {
    fn zero() -> Self;
    fn one() -> Self;
    fn from_i64(n: i64) -> Self;
    fn is_zero(&self) -> bool;
    fn abs(&self) -> Self;
    fn abs_gt(&self, other: &Self) -> bool;
    fn add(&self, other: &Self) -> Self;
    fn sub(&self, other: &Self) -> Self;
    fn mul(&self, other: &Self) -> Self;
    fn div(&self, other: &Self) -> Self;
    fn neg(&self) -> Self;
    fn to_f64(&self) -> f64;
    fn to_value(&self) -> Value;
    fn from_value(v: &Value) -> Option<Self>;
}

impl Scalar for f64 {
    fn zero() -> Self {
        0.0
    }
    fn one() -> Self {
        1.0
    }
    fn from_i64(n: i64) -> Self {
        n as f64
    }
    fn is_zero(&self) -> bool {
        *self == 0.0
    }
    fn abs(&self) -> Self {
        f64::abs(*self)
    }
    fn abs_gt(&self, other: &Self) -> bool {
        self.abs() > other.abs()
    }
    fn add(&self, other: &Self) -> Self {
        *self + *other
    }
    fn sub(&self, other: &Self) -> Self {
        *self - *other
    }
    fn mul(&self, other: &Self) -> Self {
        *self * *other
    }
    fn div(&self, other: &Self) -> Self {
        *self / *other
    }
    fn neg(&self) -> Self {
        -*self
    }
    fn to_f64(&self) -> f64 {
        *self
    }
    fn to_value(&self) -> Value {
        super::real(*self)
    }
    fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::Integer(i) => Some(i.to_f64()),
            Value::Real(r) => Some(r.to_f64()),
            Value::Rational(r) => Some(r.to_f64()),
            Value::Complex { re, im } if *im == 0.0 => Some(*re),
            _ => None,
        }
    }
}

impl Scalar for Rational {
    fn zero() -> Self {
        Rational::from(0)
    }
    fn one() -> Self {
        Rational::from(1)
    }
    fn from_i64(n: i64) -> Self {
        Rational::from(n)
    }
    fn is_zero(&self) -> bool {
        self.cmp0() == std::cmp::Ordering::Equal
    }
    fn abs(&self) -> Self {
        if self.cmp0() == std::cmp::Ordering::Less {
            self.neg()
        } else {
            self.clone()
        }
    }
    fn abs_gt(&self, other: &Self) -> bool {
        self.abs() > other.abs()
    }
    fn add(&self, other: &Self) -> Self {
        Rational::from(self + other)
    }
    fn sub(&self, other: &Self) -> Self {
        Rational::from(self - other)
    }
    fn mul(&self, other: &Self) -> Self {
        Rational::from(self * other)
    }
    fn div(&self, other: &Self) -> Self {
        Rational::from(self / other)
    }
    fn neg(&self) -> Self {
        Rational::from(&Rational::from(0) - self)
    }
    fn to_f64(&self) -> f64 {
        Rational::to_f64(self)
    }
    fn to_value(&self) -> Value {
        rational_to_value(self.clone())
    }
    fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::Integer(i) => Some(Rational::from(i)),
            Value::Rational(r) => Some(r.as_ref().clone()),
            _ => None,
        }
    }
}

// ── Small helpers ───────────────────────────────────────────────────────────

fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

fn as_list(v: &Value) -> Result<&Vec<Value>, EvalError> {
    match v {
        Value::List(items) => Ok(items),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: v.type_name().to_string(),
        }),
    }
}

fn unevaluated(name: &str, args: &[Value]) -> Value {
    Value::Call {
        head: name.to_string(),
        args: args.to_vec(),
    }
}

fn is_vector(items: &[Value]) -> bool {
    items.iter().all(|v| !matches!(v, Value::List(_)))
}

/// Return (rows, cols) if `m` is a rectangular matrix.
fn matrix_dims(m: &[Value]) -> Option<(usize, usize)> {
    if m.is_empty() {
        return Some((0, 0));
    }
    let rows = m.len();
    if let Value::List(row) = &m[0] {
        let cols = row.len();
        if m.iter()
            .all(|r| matches!(r, Value::List(c) if c.len() == cols))
        {
            Some((rows, cols))
        } else {
            None
        }
    } else {
        None
    }
}

fn rational_to_value(r: Rational) -> Value {
    let (num, den) = r.into_numer_denom();
    rational_value(num, den)
}

/// True if every entry of the (list-of-lists) matrix is an exact number.
fn matrix_is_exact(m: &[Value]) -> bool {
    m.iter().all(|row| match row {
        Value::List(cells) => cells
            .iter()
            .all(|v| matches!(v, Value::Integer(_) | Value::Rational(_))),
        _ => false,
    })
}

/// True if every entry is a (possibly complex) number.
fn matrix_is_numeric(m: &[Value]) -> bool {
    m.iter().all(|row| match row {
        Value::List(cells) => cells.iter().all(|v| scalar_complex(v).is_some()),
        _ => false,
    })
}

fn scalar_complex(v: &Value) -> Option<(f64, f64)> {
    match v {
        Value::Integer(i) => Some((i.to_f64(), 0.0)),
        Value::Real(r) => Some((r.to_f64(), 0.0)),
        Value::Rational(r) => Some((r.to_f64(), 0.0)),
        Value::Complex { re, im } => Some((*re, *im)),
        _ => None,
    }
}

fn value_matrix_to_scalar<T: Scalar>(m: &[Value]) -> Result<Vec<Vec<T>>, EvalError> {
    let mut out = Vec::with_capacity(m.len());
    for row_val in m {
        let row = as_list(row_val)?;
        let mut rrow = Vec::with_capacity(row.len());
        for v in row {
            rrow.push(T::from_value(v).ok_or_else(|| EvalError::TypeError {
                expected: "Number".to_string(),
                got: v.type_name().to_string(),
            })?);
        }
        out.push(rrow);
    }
    Ok(out)
}

fn scalar_matrix_to_value<T: Scalar>(m: &[Vec<T>]) -> Value {
    Value::List(
        m.iter()
            .map(|row| Value::List(row.iter().map(|x| x.to_value()).collect()))
            .collect(),
    )
}

fn matrix_to_f64(m: &[Value]) -> Result<Vec<Vec<f64>>, EvalError> {
    let mut out = Vec::with_capacity(m.len());
    for row_val in m {
        let row = as_list(row_val)?;
        let mut rrow = Vec::with_capacity(row.len());
        for v in row {
            rrow.push(super::to_f64(v).ok_or_else(|| EvalError::TypeError {
                expected: "Number".to_string(),
                got: v.type_name().to_string(),
            })?);
        }
        out.push(rrow);
    }
    Ok(out)
}

fn matrix_to_complex(m: &[Value]) -> Result<Vec<Vec<(f64, f64)>>, EvalError> {
    let mut out = Vec::with_capacity(m.len());
    for row_val in m {
        let row = as_list(row_val)?;
        let mut rrow = Vec::with_capacity(row.len());
        for v in row {
            rrow.push(scalar_complex(v).ok_or_else(|| EvalError::TypeError {
                expected: "Number".to_string(),
                got: v.type_name().to_string(),
            })?);
        }
        out.push(rrow);
    }
    Ok(out)
}

fn f64_matrix_to_value(m: &[Vec<f64>]) -> Value {
    Value::List(
        m.iter()
            .map(|row| Value::List(row.iter().map(|x| super::real(*x)).collect()))
            .collect(),
    )
}

fn complex_value(re: f64, im: f64) -> Value {
    if im.abs() < EPS {
        super::real(re)
    } else {
        Value::Complex { re, im }
    }
}

// ── Generic determinant / LU ────────────────────────────────────────────────

fn minor_scalar<T: Scalar>(a: &[Vec<T>], r: usize, c: usize) -> Vec<Vec<T>> {
    let mut out = Vec::with_capacity(a.len().saturating_sub(1));
    for (i, row) in a.iter().enumerate() {
        if i == r {
            continue;
        }
        let mut nrow = Vec::with_capacity(row.len().saturating_sub(1));
        for (j, v) in row.iter().enumerate() {
            if j == c {
                continue;
            }
            nrow.push(v.clone());
        }
        out.push(nrow);
    }
    out
}

fn det_generic<T: Scalar>(a: &[Vec<T>]) -> T {
    let n = a.len();
    match n {
        0 => T::one(),
        1 => a[0][0].clone(),
        2 => a[0][0].mul(&a[1][1]).sub(&a[0][1].mul(&a[1][0])),
        _ => {
            let mut sum = T::zero();
            for j in 0..n {
                let co = det_generic(&minor_scalar(a, 0, j));
                let term = a[0][j].mul(&co);
                if j.is_multiple_of(2) {
                    sum = sum.add(&term);
                } else {
                    sum = sum.sub(&term);
                }
            }
            sum
        }
    }
}

/// Combined LU factorization with partial pivoting. Returns `(lu, perm)` where
/// `a[perm[i]]` equals `(L·U)[i]`; `L` is unit lower-triangular (stored below
/// the diagonal) and `U` is upper-triangular (stored on/above the diagonal).
fn lu_decompose<T: Scalar>(a: &[Vec<T>]) -> (Vec<Vec<T>>, Vec<usize>) {
    let n = a.len();
    let mut lu = a.to_vec();
    let mut perm: Vec<usize> = (0..n).collect();
    for k in 0..n {
        let mut p = k;
        for r in (k + 1)..n {
            if lu[r][k].abs_gt(&lu[p][k]) {
                p = r;
            }
        }
        if lu[p][k].is_zero() {
            continue;
        }
        if p != k {
            lu.swap(k, p);
            perm.swap(k, p);
        }
        let piv = lu[k][k].clone();
        for i in (k + 1)..n {
            let f = lu[i][k].div(&piv);
            lu[i][k] = f.clone();
            for j in (k + 1)..n {
                let sub = f.mul(&lu[k][j]);
                lu[i][j] = lu[i][j].sub(&sub);
            }
        }
    }
    (lu, perm)
}

fn lu_solve<T: Scalar>(lu: &[Vec<T>], perm: &[usize], b: &[T]) -> Vec<T> {
    let n = lu.len();
    let mut y = vec![T::zero(); n];
    for i in 0..n {
        let mut s = b[perm[i]].clone();
        for j in 0..i {
            s = s.sub(&lu[i][j].mul(&y[j]));
        }
        y[i] = s;
    }
    let mut x = vec![T::zero(); n];
    for i in (0..n).rev() {
        if lu[i][i].is_zero() {
            x[i] = T::zero();
            continue;
        }
        let mut s = y[i].clone();
        for j in (i + 1)..n {
            s = s.sub(&lu[i][j].mul(&x[j]));
        }
        x[i] = s.div(&lu[i][i]);
    }
    x
}

/// Estimate of the L∞ condition number of `a` from its LU factors.
fn lu_condition<T: Scalar>(a: &[Vec<T>], lu: &[Vec<T>], perm: &[usize]) -> f64 {
    let n = a.len();
    if n == 0 {
        return 1.0;
    }
    let norm_a = a
        .iter()
        .map(|row| row.iter().map(|x| x.abs().to_f64()).sum::<f64>())
        .fold(0.0f64, f64::max);
    let mut norm_inv = 0.0f64;
    for j in 0..n {
        let mut b = vec![T::zero(); n];
        b[j] = T::one();
        let x = lu_solve(lu, perm, &b);
        let s: f64 = x.iter().map(|v| v.abs().to_f64()).sum();
        norm_inv = norm_inv.max(s);
    }
    norm_a * norm_inv
}

/// Symbolic/numeric determinant using the value-level arithmetic helpers.
fn det_value(m: &[Value]) -> Result<Value, EvalError> {
    let n = m.len();
    match n {
        0 => Ok(int(1)),
        1 => {
            let row = as_list(&m[0])?;
            Ok(row[0].clone())
        }
        2 => {
            let r0 = as_list(&m[0])?;
            let r1 = as_list(&m[1])?;
            let ad = mul_values_public(&r0[0], &r1[1])?;
            let bc = mul_values_public(&r0[1], &r1[0])?;
            sub_values_public(&ad, &bc)
        }
        _ => {
            let row0 = as_list(&m[0])?;
            let mut sum = int(0);
            for j in 0..n {
                let minor = remove_row_col(m, 0, j)?;
                let co = det_value(&minor)?;
                let term = mul_values_public(&row0[j], &co)?;
                let term = if !j.is_multiple_of(2) {
                    mul_values_public(&int(-1), &term)?
                } else {
                    term
                };
                sum = add_values_public(&sum, &term)?;
            }
            Ok(sum)
        }
    }
}

fn remove_row_col(m: &[Value], r: usize, c: usize) -> Result<Vec<Value>, EvalError> {
    let mut out = Vec::with_capacity(m.len().saturating_sub(1));
    for (i, row_val) in m.iter().enumerate() {
        if i == r {
            continue;
        }
        let row = as_list(row_val)?;
        let mut nrow = Vec::with_capacity(row.len().saturating_sub(1));
        for (j, v) in row.iter().enumerate() {
            if j != c {
                nrow.push(v.clone());
            }
        }
        out.push(Value::List(nrow));
    }
    Ok(out)
}

/// Determinant of a numeric or symbolic matrix, preferring exact arithmetic.
fn determinant_value(m: &[Value]) -> Result<Value, EvalError> {
    if m.is_empty() {
        return Ok(int(1));
    }
    if matrix_is_exact(m) {
        let a = value_matrix_to_scalar::<Rational>(m)?;
        Ok(det_generic(&a).to_value())
    } else if matrix_is_numeric(m) {
        let a = value_matrix_to_scalar::<f64>(m)?;
        Ok(det_generic(&a).to_value())
    } else {
        det_value(m)
    }
}

// ── Decompositions ──────────────────────────────────────────────────────────

/// LUDecomposition[m] — returns {lu, p, c}: combined LU factors, the row
/// permutation vector, and an L∞ condition-number estimate.
pub fn builtin_lu_decomposition(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("LUDecomposition", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("LUDecomposition", args)),
    };
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("LUDecomposition", args)),
    };
    if rows != cols || rows == 0 {
        return Ok(unevaluated("LUDecomposition", args));
    }
    let (lu_val, p_val, c) = if matrix_is_exact(m) {
        let a = value_matrix_to_scalar::<Rational>(m)?;
        let (lu, perm) = lu_decompose(&a);
        let c = lu_condition(&a, &lu, &perm);
        (scalar_matrix_to_value(&lu), permutation_value(&perm), c)
    } else if matrix_is_numeric(m) {
        let a = value_matrix_to_scalar::<f64>(m)?;
        let (lu, perm) = lu_decompose(&a);
        let c = lu_condition(&a, &lu, &perm);
        (scalar_matrix_to_value(&lu), permutation_value(&perm), c)
    } else {
        return Ok(unevaluated("LUDecomposition", args));
    };
    Ok(Value::List(vec![lu_val, p_val, super::real(c)]))
}

fn permutation_value(perm: &[usize]) -> Value {
    Value::List(perm.iter().map(|&i| int(i as i64 + 1)).collect())
}

/// QRDecomposition[m] — returns {q, r} with A = q.r, q having orthonormal
/// columns and r upper-triangular (classical Gram–Schmidt).
pub fn builtin_qr_decomposition(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("QRDecomposition", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("QRDecomposition", args)),
    };
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("QRDecomposition", args)),
    };
    if rows == 0 || cols == 0 {
        return Ok(unevaluated("QRDecomposition", args));
    }
    let a = matrix_to_f64(m)?;
    let (q, r) = qr_decompose(&a);
    Ok(Value::List(vec![
        f64_matrix_to_value(&q),
        f64_matrix_to_value(&r),
    ]))
}

fn qr_decompose(a: &[Vec<f64>]) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let m = a.len();
    let n = a[0].len();
    let k = m.min(n);
    // Columns of A.
    let cols: Vec<Vec<f64>> = (0..n).map(|j| (0..m).map(|i| a[i][j]).collect()).collect();
    let mut q_cols: Vec<Vec<f64>> = Vec::with_capacity(k);
    let mut r = vec![vec![0.0f64; n]; k];
    for j in 0..n {
        let mut v = cols[j].clone();
        for i in 0..q_cols.len().min(j) {
            let dot: f64 = cols[j]
                .iter()
                .zip(q_cols[i].iter())
                .map(|(x, y)| x * y)
                .sum();
            r[i][j] = dot;
            for t in 0..m {
                v[t] -= dot * q_cols[i][t];
            }
        }
        if j < k {
            let norm: f64 = v.iter().map(|x| x * x).sum::<f64>().sqrt();
            if norm > EPS {
                r[j][j] = norm;
                q_cols.push(v.iter().map(|x| x / norm).collect());
            } else {
                // Linearly dependent column: orthogonalize a standard basis vector.
                let mut u = vec![0.0f64; m];
                if j < m {
                    u[j] = 1.0;
                } else {
                    u[0] = 1.0;
                }
                for qi in &q_cols {
                    let d: f64 = u.iter().zip(qi.iter()).map(|(x, y)| x * y).sum();
                    for t in 0..m {
                        u[t] -= d * qi[t];
                    }
                }
                let un: f64 = u.iter().map(|x| x * x).sum::<f64>().sqrt();
                if un > EPS {
                    q_cols.push(u.iter().map(|x| x / un).collect());
                    r[j][j] = 0.0;
                } else {
                    q_cols.push(vec![0.0; m]);
                    r[j][j] = 0.0;
                }
            }
        }
    }
    // Assemble Q (m × q_cols.len()) from columns.
    let qk = q_cols.len();
    let mut q = vec![vec![0.0f64; qk]; m];
    for j in 0..qk {
        for i in 0..m {
            q[i][j] = q_cols[j][i];
        }
    }
    r.truncate(qk);
    (q, r)
}

// ── Eigen system ────────────────────────────────────────────────────────────

fn cadd(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 + b.0, a.1 + b.1)
}
fn csub(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 - b.0, a.1 - b.1)
}
fn cmul(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}
fn cdiv(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let d = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
}
fn cabs2(a: (f64, f64)) -> f64 {
    a.0 * a.0 + a.1 * a.1
}

/// A nonzero vector in the (numerical) null space of a square complex matrix.
fn complex_null_vector(m: &[Vec<(f64, f64)>]) -> Vec<(f64, f64)> {
    let n = m.len();
    let mut a = m.to_vec();
    let tol = 1e-9;
    let mut pivots: Vec<usize> = Vec::new();
    let mut row = 0;
    for col in 0..n {
        if row >= n {
            break;
        }
        let mut p = row;
        let mut best = cabs2(a[row][col]);
        for r in (row + 1)..n {
            let v = cabs2(a[r][col]);
            if v > best {
                best = v;
                p = r;
            }
        }
        if best < tol {
            continue;
        }
        a.swap(row, p);
        let piv = a[row][col];
        for j in col..n {
            a[row][j] = cdiv(a[row][j], piv);
        }
        for r in 0..n {
            if r == row {
                continue;
            }
            let f = a[r][col];
            if cabs2(f) < tol {
                continue;
            }
            for j in col..n {
                a[r][j] = csub(a[r][j], cmul(f, a[row][j]));
            }
        }
        pivots.push(col);
        row += 1;
    }

    let mut is_pivot = vec![false; n];
    for &p in &pivots {
        is_pivot[p] = true;
    }
    let free = (0..n).find(|&c| !is_pivot[c]).unwrap_or_else(|| {
        // Full rank: treat the pivot with the smallest magnitude as free.
        let mut best_i = 0;
        let mut bv = f64::INFINITY;
        for (i, &c) in pivots.iter().enumerate() {
            let v = cabs2(a[i][c]);
            if v < bv {
                bv = v;
                best_i = i;
            }
        }
        pivots.get(best_i).copied().unwrap_or(0)
    });

    let mut x = vec![(0.0f64, 0.0f64); n];
    x[free] = (1.0, 0.0);
    for (i, &c) in pivots.iter().enumerate() {
        if c == free {
            continue;
        }
        let mut s = (0.0f64, 0.0f64);
        for j in 0..n {
            if !is_pivot[j] {
                s = cadd(s, cmul(a[i][j], x[j]));
            }
        }
        if is_pivot[free] {
            s = cadd(s, cmul(a[i][free], x[free]));
        }
        x[c] = csub((0.0, 0.0), s);
    }

    // Normalize and fix the phase so the largest component is real-positive.
    let norm: f64 = x.iter().map(|v| cabs2(*v)).sum::<f64>().sqrt();
    if norm > 0.0 {
        for v in x.iter_mut() {
            *v = (v.0 / norm, v.1 / norm);
        }
    }
    let mut max_i = 0;
    let mut max_mag = 0.0f64;
    for (i, v) in x.iter().enumerate() {
        let mag = cabs2(*v);
        if mag > max_mag {
            max_mag = mag;
            max_i = i;
        }
    }
    if max_mag > 0.0 {
        let lead = x[max_i];
        if lead.0 < 0.0 || (lead.0.abs() < EPS && lead.1 < 0.0) {
            for v in x.iter_mut() {
                *v = (-v.0, -v.1);
            }
        }
    }
    x
}

/// Eigensystem[m] — returns {values, vectors} for a square numeric matrix.
pub fn builtin_eigensystem(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("Eigensystem", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("Eigensystem", args)),
    };
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("Eigensystem", args)),
    };
    if rows != cols || rows == 0 {
        return Ok(unevaluated("Eigensystem", args));
    }
    if !matrix_is_numeric(m) {
        return Ok(unevaluated("Eigensystem", args));
    }
    let n = rows;
    let mut a = matrix_to_f64(m)?;
    let eigenvalues = super::numericsolve::qr_eigenvalues(&mut a, n);
    // Sort by descending absolute value, keeping eigenvectors aligned.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| {
        let ai = eigenvalues[i].0 * eigenvalues[i].0 + eigenvalues[i].1 * eigenvalues[i].1;
        let aj = eigenvalues[j].0 * eigenvalues[j].0 + eigenvalues[j].1 * eigenvalues[j].1;
        aj.partial_cmp(&ai).unwrap_or(std::cmp::Ordering::Equal)
    });
    let acx = matrix_to_complex(m)?;

    let mut vals = Vec::with_capacity(n);
    let mut vecs = Vec::with_capacity(n);
    for idx in order {
        let (re, im) = eigenvalues[idx];
        vals.push(complex_value(re, im));
        // Build (A − λ I) and find a null vector.
        let mut shifted = vec![vec![(0.0f64, 0.0f64); n]; n];
        for i in 0..n {
            for j in 0..n {
                shifted[i][j] = acx[i][j];
            }
            shifted[i][i] = (shifted[i][i].0 - re, shifted[i][i].1 - im);
        }
        let v = complex_null_vector(&shifted);
        vecs.push(Value::List(
            v.iter().map(|&(re, im)| complex_value(re, im)).collect(),
        ));
    }
    Ok(Value::List(vec![Value::List(vals), Value::List(vecs)]))
}

// ── Characteristic polynomial / minors / cofactors ──────────────────────────

/// Characteristic polynomial coefficients via the Faddeev–LeVerrier algorithm.
/// Returns coefficients of `det(m − x·I)` in ascending power order.
fn charpoly_coeffs<T: Scalar>(a: &[Vec<T>]) -> Vec<T> {
    let n = a.len();
    if n == 0 {
        return vec![T::one()];
    }
    // p[k] for k = 1..=n; M_k recurrence.
    let mut m_k = a.to_vec();
    let mut p = vec![T::zero(); n + 1];
    for k in 1..=n {
        let trace = {
            let mut t = T::zero();
            for i in 0..n {
                t = t.add(&m_k[i][i]);
            }
            t
        };
        p[k] = trace.div(&T::from_i64(k as i64));
        if k == n {
            break;
        }
        // M_{k+1} = A·(M_k − p_k·I)
        let mut adjusted = m_k.clone();
        for i in 0..n {
            adjusted[i][i] = adjusted[i][i].sub(&p[k]);
        }
        let mut next = vec![vec![T::zero(); n]; n];
        for i in 0..n {
            for j in 0..n {
                let mut s = T::zero();
                for t in 0..n {
                    s = s.add(&a[i][t].mul(&adjusted[t][j]));
                }
                next[i][j] = s;
            }
        }
        m_k = next;
    }
    // det(xI − m) = x^n − p1 x^{n-1} − … − pn.
    let sign = if n.is_multiple_of(2) {
        T::one()
    } else {
        T::one().neg()
    };
    let mut coeffs = vec![T::zero(); n + 1];
    for k in 0..=n {
        let c = if n - k == 0 { T::one() } else { p[n - k].neg() };
        coeffs[k] = c.mul(&sign);
    }
    coeffs
}

/// CharacteristicPolynomial[m, x] — the polynomial det(m − x·I).
pub fn builtin_characteristic_polynomial(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("CharacteristicPolynomial", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("CharacteristicPolynomial", args)),
    };
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("CharacteristicPolynomial", args)),
    };
    if rows != cols {
        return Ok(unevaluated("CharacteristicPolynomial", args));
    }
    if rows == 0 {
        return Ok(int(1));
    }
    let co = if matrix_is_exact(m) {
        let a = value_matrix_to_scalar::<Rational>(m)?;
        charpoly_coeffs(&a)
            .into_iter()
            .map(|c| c.to_value())
            .collect::<Vec<Value>>()
    } else if matrix_is_numeric(m) {
        let a = value_matrix_to_scalar::<f64>(m)?;
        charpoly_coeffs(&a)
            .into_iter()
            .map(|c| c.to_value())
            .collect::<Vec<Value>>()
    } else {
        return Ok(unevaluated("CharacteristicPolynomial", args));
    };
    let var = args[1].clone();
    let mut terms: Vec<Value> = Vec::new();
    for (k, c) in co.iter().enumerate() {
        let zero = match c {
            Value::Integer(i) => i.is_zero(),
            Value::Real(r) => r.is_zero(),
            _ => false,
        };
        if zero {
            continue;
        }
        if k == 0 {
            terms.push(c.clone());
        } else {
            let xpow = if k == 1 {
                var.clone()
            } else {
                Value::Call {
                    head: "Power".to_string(),
                    args: vec![var.clone(), int(k as i64)],
                }
            };
            terms.push(match c {
                Value::Integer(i) if i.to_i64() == Some(1) => xpow,
                Value::Integer(i) if i.to_i64() == Some(-1) => Value::Call {
                    head: "Times".to_string(),
                    args: vec![int(-1), xpow],
                },
                _ => Value::Call {
                    head: "Times".to_string(),
                    args: vec![c.clone(), xpow],
                },
            });
        }
    }
    Ok(match terms.len() {
        0 => int(0),
        1 => terms.pop().unwrap(),
        _ => Value::Call {
            head: "Plus".to_string(),
            args: terms,
        },
    })
}

/// Minors[m] — matrix whose (i,j) entry is the determinant of `m` with row `i`
/// and column `j` removed.
pub fn builtin_minors_more(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("Minors", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("Minors", args)),
    };
    if m.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("Minors", args)),
    };
    let mut result = Vec::with_capacity(rows);
    for i in 0..rows {
        let mut new_row = Vec::with_capacity(cols);
        for j in 0..cols {
            let minor = remove_row_col(m, i, j)?;
            new_row.push(determinant_value(&minor)?);
        }
        result.push(Value::List(new_row));
    }
    Ok(Value::List(result))
}

/// Cofactor[m, {i, j}] (or Cofactor[m, i, j]) — signed (i,j) minor.
pub fn builtin_cofactor(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 && args.len() != 3 {
        return Ok(unevaluated("Cofactor", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("Cofactor", args)),
    };
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("Cofactor", args)),
    };
    let (i, j) = if args.len() == 3 {
        (index_arg(&args[1]), index_arg(&args[2]))
    } else {
        match &args[1] {
            Value::List(pair) if pair.len() == 2 => (index_arg(&pair[0]), index_arg(&pair[1])),
            _ => return Ok(unevaluated("Cofactor", args)),
        }
    };
    let (i, j) = match (i, j) {
        (Some(i), Some(j)) if i >= 1 && i <= rows && j >= 1 && j <= cols => (i - 1, j - 1),
        _ => return Ok(unevaluated("Cofactor", args)),
    };
    let minor = remove_row_col(m, i, j)?;
    let d = determinant_value(&minor)?;
    if !(i + j).is_multiple_of(2) {
        Ok(builtin_times(&[int(-1), d])?)
    } else {
        Ok(d)
    }
}

fn index_arg(v: &Value) -> Option<usize> {
    match v {
        Value::Integer(i) => i.to_usize(),
        _ => None,
    }
}

// ── MatrixExp ───────────────────────────────────────────────────────────────

fn mat_mul(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = a.len();
    let k = b.len();
    let m = if k == 0 { 0 } else { b[0].len() };
    let mut c = vec![vec![0.0f64; m]; n];
    for i in 0..n {
        for t in 0..k {
            if a[i][t] == 0.0 {
                continue;
            }
            for j in 0..m {
                c[i][j] += a[i][t] * b[t][j];
            }
        }
    }
    c
}

fn identity_f64(n: usize) -> Vec<Vec<f64>> {
    let mut m = vec![vec![0.0f64; n]; n];
    for (i, row) in m.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    m
}

/// MatrixExp[m] — matrix exponential via scaling and squaring with a Taylor series.
pub fn builtin_matrix_exp(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("MatrixExp", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("MatrixExp", args)),
    };
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("MatrixExp", args)),
    };
    if rows != cols || rows == 0 {
        return Ok(unevaluated("MatrixExp", args));
    }
    let a = matrix_to_f64(m)?;
    let n = rows;
    let norm = a
        .iter()
        .map(|row| row.iter().map(|x| x.abs()).sum::<f64>())
        .fold(0.0f64, f64::max);
    let s = if norm <= 1.0 {
        0u32
    } else {
        norm.log2().ceil() as u32
    };
    let scale = 2f64.powi(s as i32);
    let b: Vec<Vec<f64>> = a
        .iter()
        .map(|row| row.iter().map(|x| x / scale).collect())
        .collect();

    let mut result = identity_f64(n);
    let mut term = identity_f64(n);
    for k in 1..=30u32 {
        term = mat_mul(&term, &b);
        let inv = 1.0 / k as f64;
        for i in 0..n {
            for j in 0..n {
                term[i][j] *= inv;
                result[i][j] += term[i][j];
            }
        }
    }
    for _ in 0..s {
        result = mat_mul(&result, &result);
    }
    Ok(f64_matrix_to_value(&result))
}

// ── Structured matrices ─────────────────────────────────────────────────────

/// HilbertMatrix[n] (or HilbertMatrix[{m, n}]) — entries 1/(i+j−1).
pub fn builtin_hilbert_matrix(args: &[Value]) -> Result<Value, EvalError> {
    let (m, n, offset) = match parse_dims_offset(args) {
        Some(v) => v,
        None => return Ok(unevaluated("HilbertMatrix", args)),
    };
    let mut matrix = Vec::with_capacity(m);
    for i in 0..m {
        let mut row = Vec::with_capacity(n);
        for j in 0..n {
            let den = (i + j) as i64 + offset;
            row.push(rational_value(Integer::from(1), Integer::from(den)));
        }
        matrix.push(Value::List(row));
    }
    Ok(Value::List(matrix))
}

/// Parse `n` or `{m, n}` plus an optional integer offset.
fn parse_dims_offset(args: &[Value]) -> Option<(usize, usize, i64)> {
    if args.is_empty() || args.len() > 2 {
        return None;
    }
    let (m, n) = match &args[0] {
        Value::Integer(i) => {
            let n = i.to_usize()?;
            (n, n)
        }
        Value::List(dims) if dims.len() == 2 => {
            let r = match &dims[0] {
                Value::Integer(i) => i.to_usize()?,
                _ => return None,
            };
            let c = match &dims[1] {
                Value::Integer(i) => i.to_usize()?,
                _ => return None,
            };
            (r, c)
        }
        _ => return None,
    };
    let offset = if args.len() == 2 {
        match &args[1] {
            Value::Integer(i) => i.to_i64()?,
            _ => return None,
        }
    } else {
        1
    };
    Some((m, n, offset))
}

/// VandermondeMatrix[list] — matrix with entries list[[i]]^(j−1).
pub fn builtin_vandermonde_matrix(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("VandermondeMatrix", args));
    }
    let entries = match &args[0] {
        Value::List(items) => items,
        _ => return Ok(unevaluated("VandermondeMatrix", args)),
    };
    let n = entries.len();
    let cols = if args.len() == 2 {
        match &args[1] {
            Value::Integer(i) => match i.to_usize() {
                Some(c) => c,
                None => return Ok(unevaluated("VandermondeMatrix", args)),
            },
            _ => return Ok(unevaluated("VandermondeMatrix", args)),
        }
    } else {
        n
    };
    let mut matrix = Vec::with_capacity(n);
    for entry in entries {
        let mut row = Vec::with_capacity(cols);
        for j in 0..cols {
            row.push(super::arithmetic::builtin_power(&[
                entry.clone(),
                int(j as i64),
            ])?);
        }
        matrix.push(Value::List(row));
    }
    Ok(Value::List(matrix))
}

/// ToeplitzMatrix[list] — constant-diagonal matrix; ToeplitzMatrix[{m, n}, list]
/// builds a rectangular variant.
pub fn builtin_toeplitz_matrix(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => {
            let v = match &args[0] {
                Value::List(items) if is_vector(items) => items,
                _ => return Ok(unevaluated("ToeplitzMatrix", args)),
            };
            let n = v.len();
            if n == 0 {
                return Ok(Value::List(vec![]));
            }
            let mut matrix = Vec::with_capacity(n);
            for i in 0..n {
                let mut row = Vec::with_capacity(n);
                for j in 0..n {
                    let idx = (i as i64 - j as i64).unsigned_abs() as usize;
                    if idx >= n {
                        return Ok(unevaluated("ToeplitzMatrix", args));
                    }
                    row.push(v[idx].clone());
                }
                matrix.push(Value::List(row));
            }
            Ok(Value::List(matrix))
        }
        2 => {
            let (m, n) = match &args[0] {
                Value::List(dims) if dims.len() == 2 => {
                    let r = index_arg(&dims[0]);
                    let c = index_arg(&dims[1]);
                    match (r, c) {
                        (Some(r), Some(c)) => (r, c),
                        _ => return Ok(unevaluated("ToeplitzMatrix", args)),
                    }
                }
                _ => return Ok(unevaluated("ToeplitzMatrix", args)),
            };
            let v = match &args[1] {
                Value::List(items) if is_vector(items) => items,
                _ => return Ok(unevaluated("ToeplitzMatrix", args)),
            };
            if m == 0 || n == 0 {
                return Ok(Value::List(vec![]));
            }
            let need = m + n - 1;
            if v.len() < need {
                return Ok(unevaluated("ToeplitzMatrix", args));
            }
            let mut matrix = Vec::with_capacity(m);
            for i in 0..m {
                let mut row = Vec::with_capacity(n);
                for j in 0..n {
                    let idx = (n as i64 - 1) + i as i64 - j as i64;
                    row.push(v[idx as usize].clone());
                }
                matrix.push(Value::List(row));
            }
            Ok(Value::List(matrix))
        }
        _ => Ok(unevaluated("ToeplitzMatrix", args)),
    }
}

fn triangularize(args: &[Value], name: &str, lower: bool) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated(name, args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated(name, args)),
    };
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated(name, args)),
    };
    let k = if args.len() == 2 {
        match &args[1] {
            Value::Integer(i) => match i.to_i64() {
                Some(k) => k,
                None => return Ok(unevaluated(name, args)),
            },
            _ => return Ok(unevaluated(name, args)),
        }
    } else {
        0
    };
    let mut result = Vec::with_capacity(rows);
    for i in 0..rows {
        let row = as_list(&m[i])?;
        let mut new_row = Vec::with_capacity(cols);
        for j in 0..cols {
            let keep = if lower {
                j as i64 <= i as i64 + k
            } else {
                j as i64 >= i as i64 + k
            };
            if keep {
                new_row.push(row[j].clone());
            } else {
                new_row.push(int(0));
            }
        }
        result.push(Value::List(new_row));
    }
    Ok(Value::List(result))
}

/// LowerTriangularize[m] (or LowerTriangularize[m, k]).
pub fn builtin_lower_triangularize(args: &[Value]) -> Result<Value, EvalError> {
    triangularize(args, "LowerTriangularize", true)
}

/// UpperTriangularize[m] (or UpperTriangularize[m, k]).
pub fn builtin_upper_triangularize(args: &[Value]) -> Result<Value, EvalError> {
    triangularize(args, "UpperTriangularize", false)
}

// ── Predicates ──────────────────────────────────────────────────────────────

fn is_square_matrix(m: &[Value]) -> Option<usize> {
    let (r, c) = matrix_dims(m)?;
    if r == c { Some(r) } else { None }
}

/// SymmetricMatrixQ[m].
pub fn builtin_symmetric_matrix_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("SymmetricMatrixQ", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("SymmetricMatrixQ", args)),
    };
    let n = match is_square_matrix(m) {
        Some(n) => n,
        None => return Ok(Value::Bool(false)),
    };
    let mut sym = true;
    'outer: for i in 0..n {
        let ri = as_list(&m[i])?;
        for j in 0..n {
            let rj = as_list(&m[j])?;
            if ri[j] != rj[i] {
                sym = false;
                break 'outer;
            }
        }
    }
    Ok(Value::Bool(sym))
}

/// HermitianMatrixQ[m].
pub fn builtin_hermitian_matrix_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("HermitianMatrixQ", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("HermitianMatrixQ", args)),
    };
    let n = match is_square_matrix(m) {
        Some(n) => n,
        None => return Ok(Value::Bool(false)),
    };
    let mut herm = true;
    'outer: for i in 0..n {
        let ri = as_list(&m[i])?;
        for j in 0..n {
            let rj = as_list(&m[j])?;
            let conj = super::arithmetic::builtin_conjugate(&[rj[i].clone()])?;
            if ri[j] != conj {
                herm = false;
                break 'outer;
            }
        }
    }
    Ok(Value::Bool(herm))
}

/// OrthogonalMatrixQ[m] — square and m·Transpose[m] ≈ Identity.
pub fn builtin_orthogonal_matrix_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("OrthogonalMatrixQ", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("OrthogonalMatrixQ", args)),
    };
    let n = match is_square_matrix(m) {
        Some(n) => n,
        None => return Ok(Value::Bool(false)),
    };
    if !matrix_is_numeric(m) {
        return Ok(unevaluated("OrthogonalMatrixQ", args));
    }
    let a = matrix_to_complex(m)?;
    // (A Aᵀ)_{ij} = Σ_k a[i][k] · conj(a[j][k])
    for i in 0..n {
        for j in 0..n {
            let mut s = (0.0f64, 0.0f64);
            for k in 0..n {
                let c = (a[j][k].0, -a[j][k].1);
                s = cadd(s, cmul(a[i][k], c));
            }
            let want = if i == j { 1.0 } else { 0.0 };
            if (s.0 - want).abs() > 1e-8 || s.1.abs() > 1e-8 {
                return Ok(Value::Bool(false));
            }
        }
    }
    Ok(Value::Bool(true))
}

/// PositiveDefiniteMatrixQ[m] — symmetric/Hermitian with positive leading minors.
pub fn builtin_positive_definite_matrix_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("PositiveDefiniteMatrixQ", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("PositiveDefiniteMatrixQ", args)),
    };
    let n = match is_square_matrix(m) {
        Some(n) => n,
        None => return Ok(Value::Bool(false)),
    };
    if n == 0 {
        return Ok(Value::Bool(true));
    }
    // Must be symmetric (real) or Hermitian (complex).
    let scattered = m
        .iter()
        .any(|row| matches!(row, Value::List(cells) if cells.iter().any(|v| matches!(v, Value::Complex { .. }))));
    if scattered {
        if builtin_hermitian_matrix_q(&[args[0].clone()])? != Value::Bool(true) {
            return Ok(Value::Bool(false));
        }
    } else if builtin_symmetric_matrix_q(&[args[0].clone()])? != Value::Bool(true) {
        return Ok(Value::Bool(false));
    }
    // Sylvester's criterion on the leading principal minors.
    for k in 1..=n {
        let mut sub = Vec::with_capacity(k);
        for i in 0..k {
            let row = as_list(&m[i])?;
            sub.push(Value::List(row[..k].to_vec()));
        }
        let d = determinant_value(&sub)?;
        let val = match super::to_f64(&d) {
            Some(v) => v,
            None => return Ok(unevaluated("PositiveDefiniteMatrixQ", args)),
        };
        if val <= 0.0 {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

// ── Normalize ───────────────────────────────────────────────────────────────

/// Normalize[v] — v divided by its Euclidean norm.
/// Normalize[v, f] — v divided by f[v].
pub fn builtin_normalize(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("Normalize", args));
    }
    let v = match &args[0] {
        Value::List(items) if is_vector(items) => items,
        Value::List(_) => return Ok(unevaluated("Normalize", args)),
        _ => return Ok(unevaluated("Normalize", args)),
    };
    let vval = args[0].clone();
    let norm = if args.len() == 2 {
        let f = &args[1];
        match f {
            Value::Symbol(s) if s == "Norm" => super::linalg::builtin_norm(&[vval.clone()])?,
            Value::Symbol(s) if s == "Total" => super::list::builtin_total(&[vval.clone()])?,
            Value::Symbol(s) if s == "Max" => super::math::builtin_max(&[vval.clone()])?,
            _ => crate::eval::apply_function(f, &[vval.clone()], env)?,
        }
    } else {
        super::linalg::builtin_norm(&[vval.clone()])?
    };
    let mut out = Vec::with_capacity(v.len());
    for x in v {
        out.push(builtin_divide(&[x.clone(), norm.clone()])?);
    }
    Ok(Value::List(out))
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    #[allow(clippy::type_complexity)]
    let funcs: &[(&str, fn(&[Value]) -> Result<Value, EvalError>)] = &[
        ("LUDecomposition", builtin_lu_decomposition),
        ("QRDecomposition", builtin_qr_decomposition),
        ("Eigensystem", builtin_eigensystem),
        (
            "CharacteristicPolynomial",
            builtin_characteristic_polynomial,
        ),
        ("Minors", builtin_minors_more),
        ("Cofactor", builtin_cofactor),
        ("MatrixExp", builtin_matrix_exp),
        ("HilbertMatrix", builtin_hilbert_matrix),
        ("VandermondeMatrix", builtin_vandermonde_matrix),
        ("ToeplitzMatrix", builtin_toeplitz_matrix),
        ("LowerTriangularize", builtin_lower_triangularize),
        ("UpperTriangularize", builtin_upper_triangularize),
        ("SymmetricMatrixQ", builtin_symmetric_matrix_q),
        ("HermitianMatrixQ", builtin_hermitian_matrix_q),
        ("OrthogonalMatrixQ", builtin_orthogonal_matrix_q),
        (
            "PositiveDefiniteMatrixQ",
            builtin_positive_definite_matrix_q,
        ),
    ];
    for (name, func) in funcs {
        super::register_builtin(env, name, *func);
        env.set_attributes(name, vec!["Protected".to_string()]);
    }
    super::register_builtin_env(env, "Normalize", builtin_normalize);
    env.set_attributes("Normalize", vec!["Protected".to_string()]);
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn int_val(n: i64) -> Value {
        int(n)
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

    fn as_f64_matrix(v: &Value) -> Vec<Vec<f64>> {
        match v {
            Value::List(rows) => rows
                .iter()
                .map(|r| match r {
                    Value::List(cells) => cells
                        .iter()
                        .map(|c| super::super::to_f64(c).unwrap())
                        .collect(),
                    _ => panic!("expected row"),
                })
                .collect(),
            _ => panic!("expected matrix"),
        }
    }

    #[test]
    fn test_lu_decomposition_exact() {
        let m = matrix(vec![vec![2, 1], vec![4, 3]]);
        let result = builtin_lu_decomposition(&[m]).unwrap();
        let parts = as_list(&result).unwrap();
        assert_eq!(parts.len(), 3);
        // Partial pivoting swaps the rows; p must record that permutation.
        let p = as_list(&parts[1]).unwrap();
        assert_eq!(p, &vec![int_val(2), int_val(1)]);
        let expected_lu = list(vec![
            list(vec![int_val(4), int_val(3)]),
            list(vec![
                rational_value(Integer::from(1), Integer::from(2)),
                rational_value(Integer::from(-1), Integer::from(2)),
            ]),
        ]);
        assert_eq!(parts[0], expected_lu);
    }

    #[test]
    fn test_lu_decomposition_permuted_product() {
        let m = matrix(vec![vec![2, 1], vec![4, 3]]);
        let result = builtin_lu_decomposition(&[m]).unwrap();
        let parts = as_list(&result).unwrap();
        let lu = as_f64_matrix(&parts[0]);
        // l·u with L unit-lower from lu lower part.
        let l = vec![vec![1.0, 0.0], vec![lu[1][0], 1.0]];
        let u = vec![vec![lu[0][0], lu[0][1]], vec![0.0, lu[1][1]]];
        let prod = mat_mul(&l, &u);
        // m[[p]] = [[4,3],[2,1]]
        assert!((prod[0][0] - 4.0).abs() < 1e-12);
        assert!((prod[0][1] - 3.0).abs() < 1e-12);
        assert!((prod[1][0] - 2.0).abs() < 1e-12);
        assert!((prod[1][1] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_qr_decomposition_reconstructs() {
        let m = matrix(vec![vec![1, 2], vec![3, 4]]);
        let result = builtin_qr_decomposition(&[m]).unwrap();
        let parts = as_list(&result).unwrap();
        let q = as_f64_matrix(&parts[0]);
        let r = as_f64_matrix(&parts[1]);
        let prod = {
            // 2x2 times 2x2
            let mut c = vec![vec![0.0f64; 2]; 2];
            for i in 0..2 {
                for j in 0..2 {
                    for t in 0..2 {
                        c[i][j] += q[i][t] * r[t][j];
                    }
                }
            }
            c
        };
        assert!((prod[0][0] - 1.0).abs() < 1e-10);
        assert!((prod[0][1] - 2.0).abs() < 1e-10);
        assert!((prod[1][0] - 3.0).abs() < 1e-10);
        assert!((prod[1][1] - 4.0).abs() < 1e-10);
        // r must be upper triangular
        assert!(r[1][0].abs() < 1e-10);
    }

    #[test]
    fn test_eigensystem_diagonal() {
        let m = list(vec![
            list(vec![super::super::real(2.0), super::super::real(0.0)]),
            list(vec![super::super::real(0.0), super::super::real(5.0)]),
        ]);
        let result = builtin_eigensystem(&[m]).unwrap();
        let parts = as_list(&result).unwrap();
        let vals = as_list(&parts[0]).unwrap();
        assert_eq!(vals.len(), 2);
        let v0 = super::super::to_f64(&vals[0]).unwrap();
        assert!((v0 - 5.0).abs() < 1e-8);
    }

    #[test]
    fn test_eigensystem_rotation_complex() {
        let m = list(vec![
            list(vec![super::super::real(0.0), super::super::real(-1.0)]),
            list(vec![super::super::real(1.0), super::super::real(0.0)]),
        ]);
        let result = builtin_eigensystem(&[m]).unwrap();
        let parts = as_list(&result).unwrap();
        let vals = as_list(&parts[0]).unwrap();
        assert_eq!(vals.len(), 2);
        for v in vals {
            assert!(matches!(v, Value::Complex { .. }));
        }
    }

    #[test]
    fn test_characteristic_polynomial_2x2() {
        let m = matrix(vec![vec![1, 2], vec![3, 4]]);
        let x = Value::Symbol("x".to_string());
        let result = builtin_characteristic_polynomial(&[m, x.clone()]).unwrap();
        let expected = Value::Call {
            head: "Plus".to_string(),
            args: vec![
                int_val(-2),
                Value::Call {
                    head: "Times".to_string(),
                    args: vec![int_val(-5), x.clone()],
                },
                Value::Call {
                    head: "Power".to_string(),
                    args: vec![x.clone(), int_val(2)],
                },
            ],
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_minors_2x2() {
        let m = matrix(vec![vec![1, 2], vec![3, 4]]);
        let result = builtin_minors_more(&[m]).unwrap();
        assert_eq!(result, matrix(vec![vec![4, 3], vec![2, 1]]));
    }

    #[test]
    fn test_cofactor_sign() {
        let m = matrix(vec![vec![1, 2], vec![3, 4]]);
        let result = builtin_cofactor(&[m, list(vec![int_val(1), int_val(2)])]).unwrap();
        assert_eq!(result, int_val(-3));
        let m2 = matrix(vec![vec![1, 2], vec![3, 4]]);
        let result2 = builtin_cofactor(&[m2, int_val(2), int_val(1)]).unwrap();
        assert_eq!(result2, int_val(-2));
    }

    #[test]
    fn test_matrix_exp_nilpotent() {
        let m = matrix(vec![vec![0, 1], vec![0, 0]]);
        let result = builtin_matrix_exp(&[m]).unwrap();
        let a = as_f64_matrix(&result);
        assert!((a[0][0] - 1.0).abs() < 1e-10);
        assert!((a[0][1] - 1.0).abs() < 1e-10);
        assert!(a[1][0].abs() < 1e-10);
        assert!((a[1][1] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_hilbert_matrix_exact() {
        let result = builtin_hilbert_matrix(&[int_val(2)]).unwrap();
        let expected = list(vec![
            list(vec![
                int_val(1),
                rational_value(Integer::from(1), Integer::from(2)),
            ]),
            list(vec![
                rational_value(Integer::from(1), Integer::from(2)),
                rational_value(Integer::from(1), Integer::from(3)),
            ]),
        ]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_vandermonde_matrix() {
        let result = builtin_vandermonde_matrix(&[list(vec![int_val(1), int_val(2)])]).unwrap();
        assert_eq!(result, matrix(vec![vec![1, 1], vec![1, 2]]));
    }

    #[test]
    fn test_toeplitz_matrix() {
        let result =
            builtin_toeplitz_matrix(&[list(vec![int_val(1), int_val(2), int_val(3)])]).unwrap();
        assert_eq!(
            result,
            matrix(vec![vec![1, 2, 3], vec![2, 1, 2], vec![3, 2, 1]])
        );
    }

    #[test]
    fn test_lower_upper_triangularize() {
        let m = matrix(vec![vec![1, 2], vec![3, 4]]);
        assert_eq!(
            builtin_lower_triangularize(&[m.clone()]).unwrap(),
            matrix(vec![vec![1, 0], vec![3, 4]])
        );
        assert_eq!(
            builtin_upper_triangularize(&[m]).unwrap(),
            matrix(vec![vec![1, 2], vec![0, 4]])
        );
    }

    #[test]
    fn test_symmetric_matrix_q() {
        assert_eq!(
            builtin_symmetric_matrix_q(&[matrix(vec![vec![1, 2], vec![2, 3]])]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_symmetric_matrix_q(&[matrix(vec![vec![1, 2], vec![3, 4]])]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_orthogonal_matrix_q() {
        assert_eq!(
            builtin_orthogonal_matrix_q(&[matrix(vec![vec![0, -1], vec![1, 0]])]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_orthogonal_matrix_q(&[matrix(vec![vec![1, 2], vec![3, 4]])]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_positive_definite_matrix_q() {
        assert_eq!(
            builtin_positive_definite_matrix_q(&[matrix(vec![vec![2, 1], vec![1, 2]])]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_positive_definite_matrix_q(&[matrix(vec![vec![-1, 0], vec![0, -1]])]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_normalize() {
        let env = Env::new();
        let v = list(vec![int_val(3), int_val(4)]);
        let result = builtin_normalize(&[v.clone()], &env).unwrap();
        let items = as_list(&result).unwrap();
        assert!((super::super::to_f64(&items[0]).unwrap() - 0.6).abs() < 1e-12);
        assert!((super::super::to_f64(&items[1]).unwrap() - 0.8).abs() < 1e-12);

        let result2 = builtin_normalize(&[v, Value::Symbol("Total".to_string())], &env).unwrap();
        let items2 = as_list(&result2).unwrap();
        assert!((super::super::to_f64(&items2[0]).unwrap() - 3.0 / 7.0).abs() < 1e-12);
    }

    #[test]
    fn test_characteristic_polynomial_identity() {
        let m = matrix(vec![vec![1, 0], vec![0, 1]]);
        let x = Value::Symbol("x".to_string());
        let result = builtin_characteristic_polynomial(&[m, x.clone()]).unwrap();
        let expected = Value::Call {
            head: "Plus".to_string(),
            args: vec![
                int_val(1),
                Value::Call {
                    head: "Times".to_string(),
                    args: vec![int_val(-2), x.clone()],
                },
                Value::Call {
                    head: "Power".to_string(),
                    args: vec![x.clone(), int_val(2)],
                },
            ],
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_eigensystem_eigenpair() {
        // Symmetric matrix: eigenvalues 3 and 1 with orthogonal eigenvectors.
        let m = list(vec![
            list(vec![super::super::real(2.0), super::super::real(1.0)]),
            list(vec![super::super::real(1.0), super::super::real(2.0)]),
        ]);
        let result = builtin_eigensystem(&[m]).unwrap();
        let parts = as_list(&result).unwrap();
        let vals = as_list(&parts[0]).unwrap();
        let vecs = as_list(&parts[1]).unwrap();
        let a = [[2.0, 1.0], [1.0, 2.0]];
        for (v, vec_val) in vals.iter().zip(vecs.iter()) {
            let lambda = scalar_complex(v).unwrap();
            let evec: Vec<(f64, f64)> = as_list(vec_val)
                .unwrap()
                .iter()
                .map(|c| scalar_complex(c).unwrap())
                .collect();
            for i in 0..2 {
                let mut av = (0.0f64, 0.0f64);
                for k in 0..2 {
                    av = cadd(av, cmul((a[i][k], 0.0), evec[k]));
                }
                let lv = cmul(lambda, evec[i]);
                let diff = csub(av, lv);
                assert!(
                    diff.0.abs() < 1e-8 && diff.1.abs() < 1e-8,
                    "eigenpair residual too large: {:?}",
                    diff
                );
            }
        }
    }

    #[test]
    fn test_hermitian_matrix_q_complex() {
        let hermitian = list(vec![
            list(vec![int_val(1), Value::Complex { re: 0.0, im: 1.0 }]),
            list(vec![Value::Complex { re: 0.0, im: -1.0 }, int_val(1)]),
        ]);
        assert_eq!(
            builtin_hermitian_matrix_q(&[hermitian]).unwrap(),
            Value::Bool(true)
        );
        let not_hermitian = list(vec![
            list(vec![int_val(1), Value::Complex { re: 0.0, im: 1.0 }]),
            list(vec![Value::Complex { re: 0.0, im: 1.0 }, int_val(1)]),
        ]);
        assert_eq!(
            builtin_hermitian_matrix_q(&[not_hermitian]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_unevaluated_on_bad_input() {
        let result = builtin_lu_decomposition(&[int_val(5)]).unwrap();
        assert!(matches!(
            result,
            Value::Call { ref head, .. } if head == "LUDecomposition"
        ));
    }
}
