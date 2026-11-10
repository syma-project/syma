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

// ── Complex helpers for the additional decompositions ───────────────────────

/// Complex scalar used by the eigen / SVD / Schur helpers.
type C = (f64, f64);

/// A complex matrix, row-major.
type Cmat = Vec<Vec<C>>;

fn cconj(a: C) -> C {
    (a.0, -a.1)
}

fn cdot(a: &[C], b: &[C]) -> C {
    let mut s = (0.0f64, 0.0f64);
    for i in 0..a.len() {
        s = cadd(s, cmul(cconj(a[i]), b[i]));
    }
    s
}

fn cmat_mul(a: &[Vec<C>], b: &[Vec<C>]) -> Vec<Vec<C>> {
    let n = a.len();
    let k = b.len();
    let m = if k == 0 { 0 } else { b[0].len() };
    let mut c = vec![vec![(0.0f64, 0.0f64); m]; n];
    for i in 0..n {
        for t in 0..k {
            if cabs2(a[i][t]) == 0.0 {
                continue;
            }
            for j in 0..m {
                c[i][j] = cadd(c[i][j], cmul(a[i][t], b[t][j]));
            }
        }
    }
    c
}

/// Complex Gauss–Jordan matrix inverse. `None` when (numerically) singular.
fn cmat_inv(a: &[Vec<C>]) -> Option<Vec<Vec<C>>> {
    let n = a.len();
    if n == 0 {
        return Some(vec![]);
    }
    let mut m: Vec<Vec<C>> = a
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut r = row.clone();
            for j in 0..n {
                r.push(if i == j { (1.0, 0.0) } else { (0.0, 0.0) });
            }
            r
        })
        .collect();
    for col in 0..n {
        let mut p = col;
        let mut best = cabs2(m[col][col]);
        for r in (col + 1)..n {
            let v = cabs2(m[r][col]);
            if v > best {
                best = v;
                p = r;
            }
        }
        if best < 1e-14 {
            return None;
        }
        m.swap(col, p);
        let piv = m[col][col];
        for j in 0..2 * n {
            m[col][j] = cdiv(m[col][j], piv);
        }
        for r in 0..n {
            if r == col {
                continue;
            }
            let f = m[r][col];
            if cabs2(f) < 1e-30 {
                continue;
            }
            for j in 0..2 * n {
                m[r][j] = csub(m[r][j], cmul(f, m[col][j]));
            }
        }
    }
    Some(m.into_iter().map(|row| row[n..].to_vec()).collect())
}

/// A basis (column vectors) of the numerical null space of a square matrix.
fn cnull_basis(a: &[Vec<C>]) -> Vec<Vec<C>> {
    let n = a.len();
    if n == 0 {
        return vec![];
    }
    let mut m = a.to_vec();
    let mut pivots: Vec<usize> = Vec::new();
    let mut row = 0;
    for col in 0..n {
        if row >= n {
            break;
        }
        let mut p = row;
        let mut best = cabs2(m[row][col]);
        for r in (row + 1)..n {
            let v = cabs2(m[r][col]);
            if v > best {
                best = v;
                p = r;
            }
        }
        if best < 1e-12 {
            continue;
        }
        m.swap(row, p);
        let piv = m[row][col];
        for j in col..n {
            m[row][j] = cdiv(m[row][j], piv);
        }
        for r in 0..n {
            if r == row {
                continue;
            }
            let f = m[r][col];
            if cabs2(f) < 1e-12 {
                continue;
            }
            for j in col..n {
                m[r][j] = csub(m[r][j], cmul(f, m[row][j]));
            }
        }
        pivots.push(col);
        row += 1;
    }
    let mut is_piv = vec![false; n];
    for &p in &pivots {
        is_piv[p] = true;
    }
    let mut basis = Vec::new();
    for free in 0..n {
        if is_piv[free] {
            continue;
        }
        let mut x = vec![(0.0f64, 0.0f64); n];
        x[free] = (1.0, 0.0);
        for (i, &c) in pivots.iter().enumerate() {
            x[c] = csub((0.0, 0.0), m[i][free]);
        }
        basis.push(x);
    }
    basis
}

/// Solve `a x = b` (complex) for a consistent system, or `None` if inconsistent.
fn csolve(a: &[Vec<C>], b: &[C]) -> Option<Vec<C>> {
    let n = a.len();
    let mut m: Vec<Vec<C>> = a
        .iter()
        .zip(b)
        .map(|(row, &bb)| {
            let mut r = row.clone();
            r.push(bb);
            r
        })
        .collect();
    let mut pivots: Vec<usize> = Vec::new();
    let mut row = 0;
    for col in 0..n {
        if row >= n {
            break;
        }
        let mut p = row;
        let mut best = cabs2(m[row][col]);
        for r in (row + 1)..n {
            let v = cabs2(m[r][col]);
            if v > best {
                best = v;
                p = r;
            }
        }
        if best < 1e-12 {
            continue;
        }
        m.swap(row, p);
        let piv = m[row][col];
        for j in col..=n {
            m[row][j] = cdiv(m[row][j], piv);
        }
        for r in 0..n {
            if r == row {
                continue;
            }
            let f = m[r][col];
            if cabs2(f) < 1e-12 {
                continue;
            }
            for j in col..=n {
                m[r][j] = csub(m[r][j], cmul(f, m[row][j]));
            }
        }
        pivots.push(col);
        row += 1;
    }
    for r in 0..n {
        let all_zero = (0..n).all(|j| cabs2(m[r][j]) < 1e-9);
        if all_zero && cabs2(m[r][n]) > 1e-9 {
            return None;
        }
    }
    let mut x = vec![(0.0f64, 0.0f64); n];
    for (i, &c) in pivots.iter().enumerate() {
        x[c] = m[i][n];
    }
    Some(x)
}

/// Orthonormalize `x` against `basis` and append it when non-degenerate.
fn cortho_push(basis: &mut Vec<Vec<C>>, x: &[C]) {
    let mut v = x.to_vec();
    for _ in 0..2 {
        for b in basis.iter() {
            let d = cdot(b, &v);
            for i in 0..v.len() {
                v[i] = csub(v[i], cmul(d, b[i]));
            }
        }
    }
    let nrm: f64 = v.iter().map(|z| cabs2(*z)).sum::<f64>().sqrt();
    if nrm > 1e-9 {
        for z in v.iter_mut() {
            *z = (z.0 / nrm, z.1 / nrm);
        }
        basis.push(v);
    }
}

fn cindep(basis: &[Vec<C>], x: &[C]) -> bool {
    let mut tmp = basis.to_vec();
    cortho_push(&mut tmp, x);
    tmp.len() > basis.len()
}

fn cmatrix_to_value(m: &[Vec<C>]) -> Value {
    Value::List(
        m.iter()
            .map(|row| Value::List(row.iter().map(|&(re, im)| complex_value(re, im)).collect()))
            .collect(),
    )
}

// ── Symmetric Jacobi eigensolver ────────────────────────────────────────────

/// Cyclic Jacobi eigen-decomposition of a real symmetric matrix.
/// Returns eigenvalues (descending) and eigenvectors as *columns*.
fn jacobi_eigen_symmetric(a: &[Vec<f64>]) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    if n == 0 {
        return (vec![], vec![]);
    }
    let mut aa = a.to_vec();
    let mut v = identity_f64(n);
    for _ in 0..100 {
        let mut off = 0.0f64;
        for p in 0..n {
            for q in (p + 1)..n {
                off += aa[p][q] * aa[p][q];
            }
        }
        if off.sqrt() < 1e-14 {
            break;
        }
        for p in 0..n {
            for q in (p + 1)..n {
                if aa[p][q].abs() < 1e-16 {
                    continue;
                }
                let theta = (aa[q][q] - aa[p][p]) / (2.0 * aa[p][q]);
                let t = if theta >= 0.0 {
                    1.0 / (theta + (theta * theta + 1.0).sqrt())
                } else {
                    -1.0 / (-theta + (theta * theta + 1.0).sqrt())
                };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in aa.iter_mut() {
                    let rp = row[p];
                    let rq = row[q];
                    row[p] = c * rp - s * rq;
                    row[q] = s * rp + c * rq;
                }
                for k in 0..n {
                    let apk = aa[p][k];
                    let aqk = aa[q][k];
                    aa[p][k] = c * apk - s * aqk;
                    aa[q][k] = s * apk + c * aqk;
                }
                for row in v.iter_mut() {
                    let rp = row[p];
                    let rq = row[q];
                    row[p] = c * rp - s * rq;
                    row[q] = s * rp + c * rq;
                }
            }
        }
    }
    let mut pairs: Vec<(f64, Vec<f64>)> = (0..n)
        .map(|i| (aa[i][i], (0..n).map(|k| v[k][i]).collect::<Vec<f64>>()))
        .collect();
    pairs.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
    let vals = pairs.iter().map(|p| p.0).collect();
    let vecs = pairs.into_iter().map(|p| p.1).collect();
    (vals, vecs)
}

/// Convert a list of *column* vectors into a row-major matrix.
fn cols_to_matrix(cols: &[Vec<f64>]) -> Vec<Vec<f64>> {
    if cols.is_empty() {
        return vec![];
    }
    let rows = cols[0].len();
    (0..rows)
        .map(|i| cols.iter().map(|col| col[i]).collect())
        .collect()
}

/// Gram–Schmidt completion of `init` (columns) to `d` orthonormal columns.
fn orthonormal_completion(init: &[Vec<f64>], d: usize) -> Vec<Vec<f64>> {
    fn add(basis: &mut Vec<Vec<f64>>, x_in: &[f64], d: usize) {
        let mut x = x_in.to_vec();
        for _ in 0..2 {
            for b in basis.iter() {
                let dot: f64 = x.iter().zip(b.iter()).map(|(u, w)| u * w).sum();
                for i in 0..d {
                    x[i] -= dot * b[i];
                }
            }
        }
        let nrm: f64 = x.iter().map(|z| z * z).sum::<f64>().sqrt();
        if nrm > 1e-9 {
            for z in x.iter_mut() {
                *z /= nrm;
            }
            basis.push(x);
        }
    }
    let mut basis: Vec<Vec<f64>> = Vec::new();
    for col in init {
        add(&mut basis, col, d);
    }
    let mut e = 0;
    while basis.len() < d {
        let mut v = vec![0.0f64; d];
        v[e] = 1.0;
        e += 1;
        add(&mut basis, &v, d);
    }
    basis
}

// ── Singular value decomposition ────────────────────────────────────────────

/// `(u, sigma, v)` with `a == u · DiagonalMatrix[sigma] · Transpose[v]`.
fn svd_decompose(a: &[Vec<f64>]) -> (Vec<Vec<f64>>, Vec<f64>, Vec<Vec<f64>>) {
    let m = a.len();
    let n = a[0].len();
    if m >= n {
        let mut ata = vec![vec![0.0f64; n]; n];
        for i in 0..n {
            for j in 0..n {
                let mut s = 0.0;
                for k in 0..m {
                    s += a[k][i] * a[k][j];
                }
                ata[i][j] = s;
            }
        }
        let (vals, v) = jacobi_eigen_symmetric(&ata);
        let mut sigma = vec![0.0f64; n];
        let mut ucols: Vec<Vec<f64>> = Vec::with_capacity(n);
        for i in 0..n {
            let sv = vals[i].max(0.0).sqrt();
            sigma[i] = sv;
            let mut col = vec![0.0f64; m];
            if sv > 1e-12 {
                for k in 0..m {
                    let mut s = 0.0;
                    for j in 0..n {
                        s += a[k][j] * v[j][i];
                    }
                    col[k] = s / sv;
                }
            }
            ucols.push(col);
        }
        (
            cols_to_matrix(&orthonormal_completion(&ucols, m)),
            sigma,
            cols_to_matrix(&v),
        )
    } else {
        let mut aat = vec![vec![0.0f64; m]; m];
        for i in 0..m {
            for j in 0..m {
                let mut s = 0.0;
                for k in 0..n {
                    s += a[i][k] * a[j][k];
                }
                aat[i][j] = s;
            }
        }
        let (vals, u) = jacobi_eigen_symmetric(&aat);
        let mut sigma = vec![0.0f64; m];
        let mut vcols: Vec<Vec<f64>> = Vec::with_capacity(m);
        for i in 0..m {
            let sv = vals[i].max(0.0).sqrt();
            sigma[i] = sv;
            let mut col = vec![0.0f64; n];
            if sv > 1e-12 {
                for k in 0..n {
                    let mut s = 0.0;
                    for j in 0..m {
                        s += a[j][k] * u[j][i];
                    }
                    col[k] = s / sv;
                }
            }
            vcols.push(col);
        }
        (
            cols_to_matrix(&u),
            sigma,
            cols_to_matrix(&orthonormal_completion(&vcols, n)),
        )
    }
}

/// SingularValueDecomposition[m] — {u, sigma, v} with m = u·DiagonalMatrix[σ]·vᵀ.
pub fn builtin_singular_value_decomposition(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("SingularValueDecomposition", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("SingularValueDecomposition", args)),
    };
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("SingularValueDecomposition", args)),
    };
    if rows == 0 || cols == 0 || !matrix_is_numeric(m) {
        return Ok(unevaluated("SingularValueDecomposition", args));
    }
    let a = matrix_to_f64(m)?;
    let (u, sigma, v) = svd_decompose(&a);
    Ok(Value::List(vec![
        f64_matrix_to_value(&u),
        Value::List(sigma.into_iter().map(super::real).collect()),
        f64_matrix_to_value(&v),
    ]))
}

// ── Cholesky ────────────────────────────────────────────────────────────────

/// Lower-triangular `l` with `l · Transpose[l] == a`, or `None`.
fn cholesky_decompose(a: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = a.len();
    let mut l = vec![vec![0.0f64; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = a[i][j];
            for k in 0..j {
                s -= l[i][k] * l[j][k];
            }
            if i == j {
                if s < -1e-10 {
                    return None;
                }
                l[i][j] = s.max(0.0).sqrt();
            } else {
                if l[j][j].abs() < 1e-15 {
                    return None;
                }
                l[i][j] = s / l[j][j];
            }
        }
    }
    Some(l)
}

/// CholeskyDecomposition[m] — lower triangular l with m == l·Transpose[l].
pub fn builtin_cholesky_decomposition(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("CholeskyDecomposition", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("CholeskyDecomposition", args)),
    };
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("CholeskyDecomposition", args)),
    };
    if rows != cols || rows == 0 || !matrix_is_numeric(m) {
        return Ok(unevaluated("CholeskyDecomposition", args));
    }
    let a = matrix_to_f64(m)?;
    match cholesky_decompose(&a) {
        Some(l) => Ok(f64_matrix_to_value(&l)),
        None => Ok(unevaluated("CholeskyDecomposition", args)),
    }
}

// ── Schur ───────────────────────────────────────────────────────────────────

/// Real Schur decomposition `(q, t)` with `a == q · t · Transpose[q]` and `t`
/// quasi-upper-triangular (unshifted/shifted QR iteration, real arithmetic).
fn schur_decompose(a: &[Vec<f64>]) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut t = a.to_vec();
    let mut q = identity_f64(n);
    if n == 0 {
        return (q, t);
    }
    for _ in 0..200 {
        let mut maxb = 0.0f64;
        for i in 0..n {
            for j in 0..i {
                maxb = maxb.max(t[i][j].abs());
            }
        }
        if maxb < 1e-13 {
            break;
        }
        let shift = t[n - 1][n - 1];
        let mut s = t.clone();
        for i in 0..n {
            s[i][i] -= shift;
        }
        let (qq, rr) = qr_decompose(&s);
        if qq.len() != n || qq[0].len() != n {
            break;
        }
        t = mat_mul(&rr, &qq);
        for i in 0..n {
            t[i][i] += shift;
        }
        q = mat_mul(&q, &qq);
    }
    // Zero out numerically negligible below-diagonal entries.
    for i in 0..n {
        for j in 0..i {
            if t[i][j].abs() < 1e-12 {
                t[i][j] = 0.0;
            }
        }
    }
    (q, t)
}

/// SchurDecomposition[m] — {q, t} with q orthogonal and t quasi-triangular.
pub fn builtin_schur_decomposition(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("SchurDecomposition", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("SchurDecomposition", args)),
    };
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("SchurDecomposition", args)),
    };
    if rows != cols || rows == 0 || !matrix_is_numeric(m) {
        return Ok(unevaluated("SchurDecomposition", args));
    }
    let a = matrix_to_f64(m)?;
    let (q, t) = schur_decompose(&a);
    Ok(Value::List(vec![
        f64_matrix_to_value(&q),
        f64_matrix_to_value(&t),
    ]))
}

// ── Hessenberg ──────────────────────────────────────────────────────────────

/// `(p, h)` with `a == p · h · Transpose[p]`, p orthogonal and h upper Hessenberg.
fn hessenberg_decompose(a: &[Vec<f64>]) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut h = a.to_vec();
    let mut p = identity_f64(n);
    if n <= 2 {
        return (p, h);
    }
    for k in 0..(n - 2) {
        let mut v = vec![0.0f64; n];
        for i in (k + 1)..n {
            v[i] = h[i][k];
        }
        let norm: f64 = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm < EPS {
            continue;
        }
        let beta = if v[k + 1] >= 0.0 { norm } else { -norm };
        v[k + 1] += beta;
        let vnorm2: f64 = v.iter().map(|x| x * x).sum();
        if vnorm2 < 1e-30 {
            continue;
        }
        let f = 2.0 / vnorm2;
        // h <- H h
        let mut tmp = vec![vec![0.0f64; n]; n];
        for i in (k + 1)..n {
            for j in 0..n {
                let dot: f64 = ((k + 1)..n).map(|m| v[m] * h[m][j]).sum();
                tmp[i][j] = h[i][j] - f * v[i] * dot;
            }
        }
        h[(k + 1)..n].clone_from_slice(&tmp[(k + 1)..n]);
        // h <- h H
        for i in 0..n {
            let mut new_row = vec![0.0f64; n];
            for j in 0..n {
                let dot: f64 = ((k + 1)..n).map(|m| h[i][m] * v[m]).sum();
                new_row[j] = h[i][j] - f * dot * v[j];
            }
            h[i] = new_row;
        }
        // p <- p H
        for i in 0..n {
            let mut new_row = vec![0.0f64; n];
            for j in 0..n {
                let dot: f64 = ((k + 1)..n).map(|m| p[i][m] * v[m]).sum();
                new_row[j] = p[i][j] - f * dot * v[j];
            }
            p[i] = new_row;
        }
    }
    (p, h)
}

/// HessenbergDecomposition[m] — {p, h} with m = p·h·Transpose[p].
pub fn builtin_hessenberg_decomposition(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("HessenbergDecomposition", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("HessenbergDecomposition", args)),
    };
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("HessenbergDecomposition", args)),
    };
    if rows != cols || rows == 0 || !matrix_is_numeric(m) {
        return Ok(unevaluated("HessenbergDecomposition", args));
    }
    let a = matrix_to_f64(m)?;
    let (p, h) = hessenberg_decompose(&a);
    Ok(Value::List(vec![
        f64_matrix_to_value(&p),
        f64_matrix_to_value(&h),
    ]))
}

// ── Jordan (basic) ──────────────────────────────────────────────────────────

/// `(s, j)` with `a == s · j · Inverse[s]`, j in (basic) Jordan form.
fn jordan_decompose(a: &[Vec<f64>]) -> Option<(Cmat, Cmat)> {
    let n = a.len();
    if n == 0 {
        return Some((vec![], vec![]));
    }
    let mut af = a.to_vec();
    let mut vals = super::numericsolve::qr_eigenvalues(&mut af, n);
    vals.sort_by(|x, y| {
        x.0.partial_cmp(&y.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    let ac: Vec<Vec<C>> = a
        .iter()
        .map(|row| row.iter().map(|&x| (x, 0.0)).collect())
        .collect();

    let mut groups: Vec<(C, usize)> = Vec::new();
    for &lam in &vals {
        if let Some(g) = groups
            .iter_mut()
            .find(|(l, _)| (l.0 - lam.0).abs() < 1e-8 && (l.1 - lam.1).abs() < 1e-8)
        {
            g.1 += 1;
        } else {
            groups.push((lam, 1));
        }
    }

    let mut s_cols: Vec<Vec<C>> = Vec::new();
    let mut onb: Vec<Vec<C>> = Vec::new();
    let mut jblocks: Vec<(C, usize)> = Vec::new();

    for (lam, mult) in groups {
        let mut nn = ac.clone();
        for i in 0..n {
            nn[i][i] = csub(nn[i][i], lam);
        }
        let basis = cnull_basis(&nn);
        let mut group_cols = 0usize;
        for e in &basis {
            if group_cols >= mult {
                break;
            }
            let mut chain: Vec<Vec<C>> = vec![e.clone()];
            while chain.len() < mult - group_cols {
                let top = chain.last().unwrap().clone();
                match csolve(&nn, &top) {
                    Some(x) => {
                        let mut refs = onb.clone();
                        for c in &chain {
                            cortho_push(&mut refs, c);
                        }
                        if cindep(&refs, &x) {
                            chain.push(x);
                        } else {
                            break;
                        }
                    }
                    None => break,
                }
            }
            for v in chain.iter() {
                cortho_push(&mut onb, v);
                s_cols.push(v.clone());
            }
            jblocks.push((lam, chain.len()));
            group_cols += chain.len();
        }
        if group_cols < mult {
            return None;
        }
    }
    if s_cols.len() != n {
        return None;
    }
    let mut s = vec![vec![(0.0f64, 0.0f64); n]; n];
    let mut j = vec![vec![(0.0f64, 0.0f64); n]; n];
    let mut col = 0usize;
    for (lam, size) in jblocks {
        for k in 0..size {
            for i in 0..n {
                s[i][col] = s_cols[col][i];
            }
            j[col][col] = lam;
            if k + 1 < size {
                j[col][col + 1] = (1.0, 0.0);
            }
            col += 1;
        }
    }
    Some((s, j))
}

/// JordanDecomposition[m] — {s, j} with m = s·j·Inverse[s] (basic form).
pub fn builtin_jordan_decomposition(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("JordanDecomposition", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("JordanDecomposition", args)),
    };
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("JordanDecomposition", args)),
    };
    if rows != cols || rows == 0 || !matrix_is_numeric(m) {
        return Ok(unevaluated("JordanDecomposition", args));
    }
    let a = matrix_to_f64(m)?;
    match jordan_decompose(&a) {
        Some((s, j)) => Ok(Value::List(vec![
            cmatrix_to_value(&s),
            cmatrix_to_value(&j),
        ])),
        None => Ok(unevaluated("JordanDecomposition", args)),
    }
}

// ── MatrixLog ───────────────────────────────────────────────────────────────

/// Real Taylor series for log(a) after scaling to a/c (c chosen large).
fn matrix_log_series(a: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = a.len();
    let norm = a
        .iter()
        .map(|row| row.iter().map(|x| x.abs()).sum::<f64>())
        .fold(0.0f64, f64::max)
        .max(1.0);
    let c = 2.0 * norm;
    let mut b: Vec<Vec<f64>> = a
        .iter()
        .map(|row| row.iter().map(|x| x / c).collect())
        .collect();
    for i in 0..n {
        b[i][i] -= 1.0;
    }
    let mut result = vec![vec![0.0f64; n]; n];
    let mut term = b.clone();
    for k in 1..=200u32 {
        let sign = if k % 2 == 1 { 1.0 } else { -1.0 };
        let f = sign / k as f64;
        for i in 0..n {
            for j in 0..n {
                result[i][j] += f * term[i][j];
            }
        }
        term = mat_mul(&term, &b);
    }
    let lc = c.ln();
    for i in 0..n {
        result[i][i] += lc;
    }
    result
}

/// MatrixLog[m] — principal matrix logarithm, via the eigen-decomposition
/// (falling back to a scaled Taylor series for defective inputs).
pub fn builtin_matrix_log(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("MatrixLog", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("MatrixLog", args)),
    };
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("MatrixLog", args)),
    };
    if rows != cols || rows == 0 || !matrix_is_numeric(m) {
        return Ok(unevaluated("MatrixLog", args));
    }
    let n = rows;
    let a = matrix_to_f64(m)?;
    let mut af = a.clone();
    let vals = super::numericsolve::qr_eigenvalues(&mut af, n);
    let ac = matrix_to_complex(m)?;
    let mut s = vec![vec![(0.0f64, 0.0f64); n]; n];
    for (col, &(re, im)) in vals.iter().enumerate() {
        let mut shifted = ac.clone();
        for i in 0..n {
            shifted[i][i] = (shifted[i][i].0 - re, shifted[i][i].1 - im);
        }
        let v = complex_null_vector(&shifted);
        for i in 0..n {
            s[i][col] = v[i];
        }
    }
    if let Some(sinv) = cmat_inv(&s) {
        let mut d = vec![vec![(0.0f64, 0.0f64); n]; n];
        for i in 0..n {
            let (re, im) = vals[i];
            d[i][i] = (re.hypot(im).ln(), im.atan2(re));
        }
        let l = cmat_mul(&cmat_mul(&s, &d), &sinv);
        return Ok(cmatrix_to_value(&l));
    }
    Ok(f64_matrix_to_value(&matrix_log_series(&a)))
}

// ── ConjugateTranspose / Adjugate / LeviCivitaTensor ────────────────────────

/// ConjugateTranspose[m] — conjugate transpose of a matrix (or vector).
pub fn builtin_conjugate_transpose(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("ConjugateTranspose", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("ConjugateTranspose", args)),
    };
    if m.is_empty() {
        return Ok(Value::List(vec![]));
    }
    if is_vector(m) {
        let mut row = Vec::with_capacity(m.len());
        for v in m {
            row.push(super::arithmetic::builtin_conjugate(&[v.clone()])?);
        }
        return Ok(Value::List(vec![Value::List(row)]));
    }
    let (rows, cols) = match matrix_dims(m) {
        Some(d) => d,
        None => return Ok(unevaluated("ConjugateTranspose", args)),
    };
    let mut out = Vec::with_capacity(cols);
    for j in 0..cols {
        let mut row = Vec::with_capacity(rows);
        for i in 0..rows {
            let cell = as_list(&m[i])?[j].clone();
            row.push(super::arithmetic::builtin_conjugate(&[cell])?);
        }
        out.push(Value::List(row));
    }
    Ok(Value::List(out))
}

/// Adjugate[m] — transpose of the cofactor matrix (classical adjugate).
pub fn builtin_adjugate(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("Adjugate", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("Adjugate", args)),
    };
    let n = match is_square_matrix(m) {
        Some(n) => n,
        None => return Ok(unevaluated("Adjugate", args)),
    };
    if n == 0 {
        return Ok(Value::List(vec![]));
    }
    let mut adj = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = Vec::with_capacity(n);
        for j in 0..n {
            let minor = remove_row_col(m, j, i)?;
            let d = determinant_value(&minor)?;
            let val = if !(i + j).is_multiple_of(2) {
                builtin_times(&[int(-1), d])?
            } else {
                d
            };
            row.push(val);
        }
        adj.push(Value::List(row));
    }
    Ok(Value::List(adj))
}

fn levi_civita_leaf(idx: &[usize]) -> Value {
    let rank = idx.len();
    let mut sign = 1i64;
    for i in 0..rank {
        for j in (i + 1)..rank {
            if idx[i] == idx[j] {
                sign = 0;
            } else if idx[i] > idx[j] {
                sign = -sign;
            }
        }
    }
    int(sign)
}

fn levi_civita_build(dim: usize, rank: usize, idx: &mut Vec<usize>) -> Value {
    if idx.len() == rank {
        return levi_civita_leaf(idx);
    }
    let mut items = Vec::with_capacity(dim);
    for k in 0..dim {
        idx.push(k);
        items.push(levi_civita_build(dim, rank, idx));
        idx.pop();
    }
    Value::List(items)
}

/// LeviCivitaTensor[n] — the fully antisymmetric rank-n tensor of dimension n.
pub fn builtin_levi_civita_tensor(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("LeviCivitaTensor", args));
    }
    let n = match &args[0] {
        Value::Integer(i) => i.to_usize(),
        Value::Real(r) if r.is_integer() => r.to_integer().and_then(|i| i.to_usize()),
        _ => None,
    };
    let n = match n {
        Some(n) if n >= 1 => n,
        _ => return Ok(unevaluated("LeviCivitaTensor", args)),
    };
    let mut idx = Vec::with_capacity(n);
    Ok(levi_civita_build(n, n, &mut idx))
}

// ── Transform matrices ──────────────────────────────────────────────────────

fn negate(v: &Value) -> Result<Value, EvalError> {
    builtin_times(&[int(-1), v.clone()])
}

/// RotationMatrix[θ] / RotationMatrix[θ, w] — 2-D (or 3-D axis) rotation.
pub fn builtin_rotation_matrix(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("RotationMatrix", args));
    }
    if args.len() == 1 {
        let c = super::math::builtin_cos(&[args[0].clone()])?;
        let s = super::math::builtin_sin(&[args[0].clone()])?;
        let ns = negate(&s)?;
        return Ok(Value::List(vec![
            Value::List(vec![c.clone(), ns]),
            Value::List(vec![s, c]),
        ]));
    }
    let theta = super::to_f64(&args[0]);
    let w = match &args[1] {
        Value::List(items) if items.len() == 3 => items,
        _ => return Ok(unevaluated("RotationMatrix", args)),
    };
    let (theta, wx, wy, wz) = match (
        theta,
        super::to_f64(&w[0]),
        super::to_f64(&w[1]),
        super::to_f64(&w[2]),
    ) {
        (Some(t), Some(x), Some(y), Some(z)) => (t, x, y, z),
        _ => return Ok(unevaluated("RotationMatrix", args)),
    };
    let nrm = (wx * wx + wy * wy + wz * wz).sqrt();
    if nrm < EPS {
        return Ok(unevaluated("RotationMatrix", args));
    }
    let (x, y, z) = (wx / nrm, wy / nrm, wz / nrm);
    let (c, s) = (theta.cos(), theta.sin());
    let ci = 1.0 - c;
    let r = vec![
        vec![c + x * x * ci, x * y * ci - z * s, x * z * ci + y * s],
        vec![y * x * ci + z * s, c + y * y * ci, y * z * ci - x * s],
        vec![z * x * ci - y * s, z * y * ci + x * s, c + z * z * ci],
    ];
    Ok(f64_matrix_to_value(&r))
}

/// TranslationTransform[v] — augmented (n+1)×(n+1) affine translation matrix.
pub fn builtin_translation_transform(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("TranslationTransform", args));
    }
    let v = match &args[0] {
        Value::List(items) if is_vector(items) => items,
        _ => return Ok(unevaluated("TranslationTransform", args)),
    };
    let n = v.len();
    let mut m = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let mut row = Vec::with_capacity(n + 1);
        for j in 0..=n {
            if i < n && j == n {
                row.push(v[i].clone());
            } else if i == j {
                row.push(int(1));
            } else {
                row.push(int(0));
            }
        }
        m.push(Value::List(row));
    }
    Ok(Value::List(m))
}

/// ScalingMatrix[{s1, …}] / ScalingMatrix[s, n] — diagonal scaling matrix.
pub fn builtin_scaling_matrix(args: &[Value]) -> Result<Value, EvalError> {
    let factors: Vec<Value> = match args.len() {
        1 => match &args[0] {
            Value::List(items) if !items.is_empty() && is_vector(items) => items.clone(),
            _ => return Ok(unevaluated("ScalingMatrix", args)),
        },
        2 => {
            let n = match &args[1] {
                Value::Integer(i) => match i.to_usize() {
                    Some(n) => n,
                    None => return Ok(unevaluated("ScalingMatrix", args)),
                },
                _ => return Ok(unevaluated("ScalingMatrix", args)),
            };
            vec![args[0].clone(); n]
        }
        _ => return Ok(unevaluated("ScalingMatrix", args)),
    };
    let n = factors.len();
    let mut m = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = Vec::with_capacity(n);
        for j in 0..n {
            row.push(if i == j { factors[i].clone() } else { int(0) });
        }
        m.push(Value::List(row));
    }
    Ok(Value::List(m))
}

/// ReflectionMatrix[v] — Householder reflection with normal vector v.
pub fn builtin_reflection_matrix(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("ReflectionMatrix", args));
    }
    let v = match &args[0] {
        Value::List(items) if !items.is_empty() && is_vector(items) => items,
        _ => return Ok(unevaluated("ReflectionMatrix", args)),
    };
    let n = v.len();
    let mut dot = int(0);
    for x in v {
        dot = add_values_public(&dot, &mul_values_public(x, x)?)?;
    }
    let mut m = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = Vec::with_capacity(n);
        for j in 0..n {
            let prod = mul_values_public(&v[i], &v[j])?;
            let two = mul_values_public(&int(2), &prod)?;
            let t = builtin_divide(&[two, dot.clone()])?;
            let base = if i == j { int(1) } else { int(0) };
            row.push(sub_values_public(&base, &t)?);
        }
        m.push(Value::List(row));
    }
    Ok(Value::List(m))
}

/// RotationTransform[θ] / RotationTransform[θ, w] — augmented rotation matrix.
pub fn builtin_rotation_transform(args: &[Value]) -> Result<Value, EvalError> {
    match builtin_rotation_matrix(args) {
        Ok(Value::Call { .. }) => Ok(unevaluated("RotationTransform", args)),
        Ok(inner) => {
            let r = as_list(&inner)?;
            let n = r.len();
            let mut m = Vec::with_capacity(n + 1);
            for i in 0..n {
                let mut row = as_list(&r[i])?.clone();
                row.push(int(0));
                m.push(Value::List(row));
            }
            let mut last = vec![int(0); n];
            last.push(int(1));
            m.push(Value::List(last));
            Ok(Value::List(m))
        }
        Err(e) => Err(e),
    }
}

// ── Additional predicates ───────────────────────────────────────────────────

/// UnitaryMatrixQ[m] — square and m·ConjugateTranspose[m] ≈ Identity.
pub fn builtin_unitary_matrix_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("UnitaryMatrixQ", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("UnitaryMatrixQ", args)),
    };
    let n = match is_square_matrix(m) {
        Some(n) => n,
        None => return Ok(Value::Bool(false)),
    };
    if !matrix_is_numeric(m) {
        return Ok(unevaluated("UnitaryMatrixQ", args));
    }
    let a = matrix_to_complex(m)?;
    for i in 0..n {
        for j in 0..n {
            let mut s = (0.0f64, 0.0f64);
            for k in 0..n {
                s = cadd(s, cmul(a[i][k], cconj(a[j][k])));
            }
            let want = if i == j { 1.0 } else { 0.0 };
            if (s.0 - want).abs() > 1e-8 || s.1.abs() > 1e-8 {
                return Ok(Value::Bool(false));
            }
        }
    }
    Ok(Value::Bool(true))
}

/// AntisymmetricMatrixQ[m] — square with m == -Transpose[m].
pub fn builtin_antisymmetric_matrix_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("AntisymmetricMatrixQ", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("AntisymmetricMatrixQ", args)),
    };
    let n = match is_square_matrix(m) {
        Some(n) => n,
        None => return Ok(Value::Bool(false)),
    };
    if !matrix_is_numeric(m) {
        return Ok(unevaluated("AntisymmetricMatrixQ", args));
    }
    for i in 0..n {
        for j in 0..n {
            let a = scalar_complex(&as_list(&m[i])?[j]).unwrap();
            let b = scalar_complex(&as_list(&m[j])?[i]).unwrap();
            let s = cadd(a, b);
            if s.0.abs() > 1e-10 || s.1.abs() > 1e-10 {
                return Ok(Value::Bool(false));
            }
        }
    }
    Ok(Value::Bool(true))
}

/// PositiveSemidefiniteMatrixQ[m] — symmetric/Hermitian with non-negative spectrum.
pub fn builtin_positive_semidefinite_matrix_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("PositiveSemidefiniteMatrixQ", args));
    }
    let m = match &args[0] {
        Value::List(_) => as_list(&args[0])?,
        _ => return Ok(unevaluated("PositiveSemidefiniteMatrixQ", args)),
    };
    let n = match is_square_matrix(m) {
        Some(n) => n,
        None => return Ok(Value::Bool(false)),
    };
    if n == 0 {
        return Ok(Value::Bool(true));
    }
    let has_complex = m.iter().any(
        |row| matches!(row, Value::List(cells) if cells.iter().any(|v| matches!(v, Value::Complex { .. }))),
    );
    if has_complex {
        if builtin_hermitian_matrix_q(&[args[0].clone()])? != Value::Bool(true) {
            return Ok(Value::Bool(false));
        }
        // All principal minors must be non-negative (characterizes PSD).
        for mask in 1u32..(1u32 << n) {
            let idx: Vec<usize> = (0..n).filter(|&i| mask & (1 << i) != 0).collect();
            let mut sub = Vec::with_capacity(idx.len());
            for &i in &idx {
                let row = as_list(&m[i])?;
                sub.push(Value::List(idx.iter().map(|&j| row[j].clone()).collect()));
            }
            let d = determinant_value(&sub)?;
            if let Some(val) = super::to_f64(&d)
                && val < -1e-9
            {
                return Ok(Value::Bool(false));
            }
        }
        return Ok(Value::Bool(true));
    }
    if builtin_symmetric_matrix_q(&[args[0].clone()])? != Value::Bool(true) {
        return Ok(Value::Bool(false));
    }
    let a = matrix_to_f64(m)?;
    let (vals, _) = jacobi_eigen_symmetric(&a);
    Ok(Value::Bool(vals.iter().all(|&v| {
        v >= -1e-9
            * a.iter()
                .map(|r| r.iter().map(|x| x.abs()).sum::<f64>())
                .fold(1.0f64, f64::max)
    })))
}

// ── Registration ────────────────────────────────────────────────────────────

// ── Sparse arrays ───────────────────────────────────────────────────────────

/// Is `v` a (symbolic) `SparseArray`?
pub fn is_sparse(v: &Value) -> bool {
    matches!(v, Value::Call { head, .. } if head == "SparseArray")
}

fn is_zero_value(v: &Value) -> bool {
    matches!(v, Value::Integer(i) if i.is_zero())
        || matches!(v, Value::Rational(r) if r.is_zero())
        || matches!(v, Value::Real(r) if r.is_zero())
}

/// Split a `SparseArray[rules, dims, default?]` into its parts.
fn sparse_parts(v: &Value) -> Option<(Vec<Value>, Vec<usize>, Value)> {
    let Value::Call { head, args } = v else {
        return None;
    };
    if head != "SparseArray" || args.len() < 2 {
        return None;
    }
    let rules = match &args[0] {
        Value::List(r) => r
            .iter()
            .filter_map(|x| {
                rule_position(x).map(|(pos, val)| Value::Rule {
                    lhs: Box::new(Value::List(pos)),
                    rhs: Box::new(val),
                    delayed: false,
                })
            })
            .collect(),
        _ => return None,
    };
    let dims = dims_of(&unwrap_value(&args[1]))?;
    let default = args
        .get(2)
        .map(unwrap_value)
        .unwrap_or(Value::Integer(Integer::from(0)));
    Some((rules, dims, default))
}

fn dims_of(v: &Value) -> Option<Vec<usize>> {
    match v {
        Value::Integer(i) => {
            let n = i.to_usize()?;
            if n == 0 { None } else { Some(vec![n]) }
        }
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                out.push(usize::try_from(it.to_integer()?).ok()?);
            }
            Some(out)
        }
        _ => None,
    }
}

/// Infer the dimensions from a rule set's positions.
fn infer_dims(rules: &[Value]) -> Option<Vec<usize>> {
    let mut dims: Vec<usize> = Vec::new();
    for r in rules {
        let Some((pos, _)) = rule_position(r) else {
            continue;
        };
        for (i, c) in pos.iter().enumerate() {
            let idx = usize::try_from(c.to_integer()?).ok()?;
            if i >= dims.len() {
                dims.push(idx);
            } else if idx > dims[i] {
                dims[i] = idx;
            }
        }
    }
    if dims.is_empty() {
        Some(vec![0])
    } else {
        Some(dims)
    }
}

/// Recursively collect positional rules of a dense (ragged) list.
fn dense_rules(v: &Value, prefix: &mut Vec<Value>, out: &mut Vec<Value>) {
    match v {
        Value::List(items) => {
            for (i, it) in items.iter().enumerate() {
                prefix.push(Value::Integer(Integer::from(i as i64 + 1)));
                dense_rules(it, prefix, out);
                prefix.pop();
            }
        }
        _ => {
            if !is_zero_value(v) {
                out.push(Value::Rule {
                    lhs: Box::new(Value::List(prefix.clone())),
                    rhs: Box::new(v.clone()),
                    delayed: false,
                });
            }
        }
    }
}

fn list_shape(v: &Value) -> Vec<usize> {
    let mut shape = Vec::new();
    let mut cur = v;
    while let Value::List(items) = cur {
        shape.push(items.len());
        if items.is_empty() {
            break;
        }
        cur = &items[0];
    }
    shape
}

/// Unwrap a `Value::Pattern` (parsed rule operands) into a plain value.
fn unwrap_value(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => crate::pattern::unwrap_expr_to_value(e),
        _ => v.clone(),
    }
}

/// Normalise a rule into `(position list, value)`; `None` if not a position
/// rule.
fn rule_position(r: &Value) -> Option<(Vec<Value>, Value)> {
    let Value::Rule { lhs, rhs, .. } = r else {
        return None;
    };
    match unwrap_value(lhs) {
        Value::List(pos) => Some((pos, unwrap_value(rhs))),
        _ => None,
    }
}

/// SparseArray[rules] / SparseArray[rules, dims] / SparseArray[dense, ...].
pub fn builtin_sparse_array(args: &[Value]) -> Result<Value, EvalError> {
    let uneval = || unevaluated("SparseArray", args);
    if args.is_empty() || args.len() > 3 {
        return Ok(uneval());
    }
    let default = args
        .get(2)
        .map(unwrap_value)
        .unwrap_or(Value::Integer(Integer::from(0)));
    // First argument: rules or dense array.
    let (rules_raw, dims) = match &args[0] {
        Value::List(items) if items.iter().all(|x| matches!(x, Value::Rule { .. })) => {
            let dims = match args.get(1) {
                Some(d) => match dims_of(&unwrap_value(d)) {
                    Some(d) => d,
                    None => return Ok(uneval()),
                },
                None => match infer_dims(items) {
                    Some(d) => d,
                    None => return Ok(uneval()),
                },
            };
            (items.clone(), dims)
        }
        Value::List(_) => {
            let dims = match args.get(1) {
                Some(d) => match dims_of(&unwrap_value(d)) {
                    Some(d) => d,
                    None => return Ok(uneval()),
                },
                None => list_shape(&args[0]),
            };
            let mut rules = Vec::new();
            let mut prefix = Vec::new();
            dense_rules(&args[0], &mut prefix, &mut rules);
            (rules, dims)
        }
        _ => return Ok(uneval()),
    };
    // Normalise rules (unwrap parsed `Pattern` operands).
    let mut rules = Vec::with_capacity(rules_raw.len());
    for r in &rules_raw {
        match rule_position(r) {
            Some((pos, val)) => rules.push(Value::Rule {
                lhs: Box::new(Value::List(pos)),
                rhs: Box::new(val),
                delayed: false,
            }),
            None => return Ok(uneval()),
        }
    }
    let mut out = vec![Value::List(rules), dims_value(&dims)];
    if !is_zero_value(&default) {
        out.push(default);
    }
    Ok(Value::Call {
        head: "SparseArray".to_string(),
        args: out,
    })
}

fn dims_value(dims: &[usize]) -> Value {
    Value::List(
        dims.iter()
            .map(|d| Value::Integer(Integer::from(*d as i64)))
            .collect(),
    )
}

/// SparseArrayQ[expr] — True if `expr` is a SparseArray.
pub fn builtin_sparse_array_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "SparseArrayQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(is_sparse(&args[0])))
}

/// ArrayRules[expr] — positional rules for a sparse or dense array.
pub fn builtin_array_rules(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ArrayRules requires exactly 1 argument".to_string(),
        ));
    }
    if let Some((rules, dims, default)) = sparse_parts(&args[0]) {
        let mut out = rules;
        // A trailing default rule with blank positions, as Wolfram emits.
        let rank = dims.len().max(1);
        let blanks: Vec<Value> = (0..rank)
            .map(|_| {
                Value::Pattern(crate::ast::Expr::Blank {
                    type_constraint: None,
                })
            })
            .collect();
        out.push(Value::Rule {
            lhs: Box::new(Value::List(blanks)),
            rhs: Box::new(default),
            delayed: false,
        });
        return Ok(Value::List(out));
    }
    // Dense array.
    let mut rules = Vec::new();
    let mut prefix = Vec::new();
    dense_rules(&args[0], &mut prefix, &mut rules);
    Ok(Value::List(rules))
}

/// Materialise a sparse array into a dense nested list.
pub fn normal_dense(v: &Value) -> Option<Value> {
    let (rules, dims, default) = sparse_parts(v)?;
    fn build(dims: &[usize], default: &Value) -> Value {
        if dims.is_empty() {
            return default.clone();
        }
        Value::List((0..dims[0]).map(|_| build(&dims[1..], default)).collect())
    }
    let mut dense = build(&dims, &default);
    for r in &rules {
        let Some((pos, val)) = rule_position(r) else {
            continue;
        };
        if let Some(slot) = list_slot_mut(&mut dense, &pos) {
            *slot = val;
        }
    }
    Some(dense)
}

fn list_slot_mut<'a>(mut v: &'a mut Value, pos: &[Value]) -> Option<&'a mut Value> {
    for p in pos {
        let idx = usize::try_from(p.to_integer()?).ok()?;
        let Value::List(items) = v else {
            return None;
        };
        let idx = idx.checked_sub(1)?;
        v = items.get_mut(idx)?;
    }
    Some(v)
}

/// Numeric product of two values when both are numeric.
fn num_mul(a: &Value, b: &Value) -> Option<Value> {
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Some(Value::Integer(x.clone() * y.clone())),
        (Value::Rational(x), Value::Rational(y)) => {
            Some(Value::Rational(Box::new((**x).clone() * (**y).clone())))
        }
        _ => {
            let x = super::to_f64(a)?;
            let y = super::to_f64(b)?;
            Some(super::real(x * y))
        }
    }
}

/// Numeric sum of a list of values when all are numeric.
fn num_sum(vals: &[Value]) -> Option<Value> {
    let mut acc: Option<Value> = None;
    for v in vals {
        acc = Some(match (acc, v) {
            (None, v) => v.clone(),
            (Some(Value::Integer(a)), Value::Integer(b)) => Value::Integer(a + b),
            (Some(Value::Rational(a)), Value::Rational(b)) => {
                Value::Rational(Box::new((*a).clone() + (**b).clone()))
            }
            (Some(a), b) => super::real(super::to_f64(&a)? + super::to_f64(b)?),
        });
    }
    acc
}

/// SparseMatrixMultiply[a, b] — matrix product of two (sparse or dense) arrays,
/// returned as a `SparseArray`.
pub fn builtin_sparse_matrix_multiply(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "SparseMatrixMultiply requires exactly 2 arguments".to_string(),
        ));
    }
    let to_dense = |v: &Value| -> Option<Value> {
        if is_sparse(v) {
            normal_dense(v)
        } else {
            Some(v.clone())
        }
    };
    let (Some(a), Some(b)) = (to_dense(&args[0]), to_dense(&args[1])) else {
        return Ok(unevaluated("SparseMatrixMultiply", args));
    };
    let (Value::List(ar), Value::List(br)) = (&a, &b) else {
        return Ok(unevaluated("SparseMatrixMultiply", args));
    };
    let n = ar.len();
    let m = br
        .first()
        .map(|r| match r {
            Value::List(c) => c.len(),
            _ => 0,
        })
        .unwrap_or(0);
    if m == 0 {
        return Ok(unevaluated("SparseMatrixMultiply", args));
    }
    let mut rows: Vec<Value> = Vec::with_capacity(n);
    for i in 0..n {
        let Value::List(arow) = &ar[i] else {
            return Ok(unevaluated("SparseMatrixMultiply", args));
        };
        let mut row: Vec<Value> = Vec::with_capacity(m);
        for j in 0..m {
            let mut terms: Vec<Value> = Vec::new();
            for (k, av) in arow.iter().enumerate() {
                let bkj = match br.get(k) {
                    Some(Value::List(brow)) => brow.get(j).cloned(),
                    _ => None,
                };
                if let Some(bv) = bkj {
                    if is_zero_value(av) || is_zero_value(&bv) {
                        continue;
                    }
                    terms.push(match num_mul(av, &bv) {
                        Some(v) => v,
                        None => Value::Call {
                            head: "Times".to_string(),
                            args: vec![av.clone(), bv],
                        },
                    });
                }
            }
            row.push(if terms.is_empty() {
                Value::Integer(Integer::from(0))
            } else if terms.len() == 1 {
                terms.pop().unwrap()
            } else {
                num_sum(&terms).unwrap_or(Value::Call {
                    head: "Plus".to_string(),
                    args: terms,
                })
            });
        }
        rows.push(Value::List(row));
    }
    builtin_sparse_array(&[Value::List(rows)])
}

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    #[allow(clippy::type_complexity)]
    let funcs: &[(&str, fn(&[Value]) -> Result<Value, EvalError>)] = &[
        ("SparseArray", builtin_sparse_array),
        ("SparseArrayQ", builtin_sparse_array_q),
        ("ArrayRules", builtin_array_rules),
        ("SparseMatrixMultiply", builtin_sparse_matrix_multiply),
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
        ("UnitaryMatrixQ", builtin_unitary_matrix_q),
        ("AntisymmetricMatrixQ", builtin_antisymmetric_matrix_q),
        (
            "PositiveDefiniteMatrixQ",
            builtin_positive_definite_matrix_q,
        ),
        (
            "PositiveSemidefiniteMatrixQ",
            builtin_positive_semidefinite_matrix_q,
        ),
        (
            "SingularValueDecomposition",
            builtin_singular_value_decomposition,
        ),
        ("CholeskyDecomposition", builtin_cholesky_decomposition),
        ("SchurDecomposition", builtin_schur_decomposition),
        ("HessenbergDecomposition", builtin_hessenberg_decomposition),
        ("JordanDecomposition", builtin_jordan_decomposition),
        ("MatrixLog", builtin_matrix_log),
        ("ConjugateTranspose", builtin_conjugate_transpose),
        ("Adjugate", builtin_adjugate),
        ("LeviCivitaTensor", builtin_levi_civita_tensor),
        // `KroneckerProduct` is owned by `tensor`.
        ("RotationMatrix", builtin_rotation_matrix),
        ("TranslationTransform", builtin_translation_transform),
        ("ScalingMatrix", builtin_scaling_matrix),
        ("ReflectionMatrix", builtin_reflection_matrix),
        ("RotationTransform", builtin_rotation_transform),
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

    // ── Additional decomposition tests ──────────────────────────────────────

    fn real_matrix(data: Vec<Vec<f64>>) -> Value {
        list(
            data.into_iter()
                .map(|row| list(row.into_iter().map(super::super::real).collect()))
                .collect(),
        )
    }

    fn f64_entry(v: &Value) -> f64 {
        super::super::to_f64(v).unwrap()
    }

    fn mmul(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
        let n = a.len();
        let k = b.len();
        let m = if k == 0 { 0 } else { b[0].len() };
        let mut c = vec![vec![0.0f64; m]; n];
        for i in 0..n {
            for t in 0..k {
                for j in 0..m {
                    c[i][j] += a[i][t] * b[t][j];
                }
            }
        }
        c
    }

    fn mtranspose(a: &[Vec<f64>]) -> Vec<Vec<f64>> {
        if a.is_empty() {
            return vec![];
        }
        (0..a[0].len())
            .map(|j| (0..a.len()).map(|i| a[i][j]).collect())
            .collect()
    }

    fn close(a: &[Vec<f64>], b: &[Vec<f64>], tol: f64) -> bool {
        a.len() == b.len()
            && a.iter().zip(b).all(|(ra, rb)| {
                ra.len() == rb.len() && ra.iter().zip(rb).all(|(x, y)| (x - y).abs() < tol)
            })
    }

    #[test]
    fn test_svd_reconstructs() {
        let a = [[1.0, 2.0], [3.0, 4.0]];
        let m = real_matrix(a.iter().map(|r| r.to_vec()).collect());
        let result = builtin_singular_value_decomposition(&[m]).unwrap();
        let parts = as_list(&result).unwrap();
        let u = as_f64_matrix(&parts[0]);
        let sigma: Vec<f64> = as_list(&parts[1]).unwrap().iter().map(f64_entry).collect();
        let v = as_f64_matrix(&parts[2]);
        // sigma descending and positive.
        assert!(sigma[0] >= sigma[1] && sigma[1] >= 0.0);
        // Reconstruct u · diag(sigma) · vᵀ.
        let mut d = vec![vec![0.0f64; 2]; 2];
        d[0][0] = sigma[0];
        d[1][1] = sigma[1];
        let rec = mmul(&mmul(&u, &d), &mtranspose(&v));
        assert!(close(
            &rec,
            &a.iter().map(|r| r.to_vec()).collect::<Vec<_>>(),
            1e-8
        ));
        // u and v orthonormal.
        assert!(close(&mmul(&u, &mtranspose(&u)), &identity_f64(2), 1e-8));
        assert!(close(&mmul(&v, &mtranspose(&v)), &identity_f64(2), 1e-8));
    }

    #[test]
    fn test_svd_rectangular() {
        let a = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let m = real_matrix(a.iter().map(|r| r.to_vec()).collect());
        let result = builtin_singular_value_decomposition(&[m]).unwrap();
        let parts = as_list(&result).unwrap();
        let u = as_f64_matrix(&parts[0]);
        let sigma: Vec<f64> = as_list(&parts[1]).unwrap().iter().map(f64_entry).collect();
        let v = as_f64_matrix(&parts[2]);
        assert_eq!(u.len(), 2);
        assert_eq!(v.len(), 3);
        assert_eq!(sigma.len(), 2);
        let mut d = vec![vec![0.0f64; 3]; 2];
        d[0][0] = sigma[0];
        d[1][1] = sigma[1];
        let rec = mmul(&mmul(&u, &d), &mtranspose(&v));
        assert!(close(
            &rec,
            &a.iter().map(|r| r.to_vec()).collect::<Vec<_>>(),
            1e-8
        ));
    }

    #[test]
    fn test_cholesky_decomposition() {
        let m = matrix(vec![vec![4, 2], vec![2, 2]]);
        let result = builtin_cholesky_decomposition(&[m]).unwrap();
        let l = as_f64_matrix(&result);
        assert!(l[0][1].abs() < 1e-12);
        let rec = mmul(&l, &mtranspose(&l));
        assert!(close(
            &rec,
            &[[4.0, 2.0], [2.0, 2.0]]
                .iter()
                .map(|r| r.to_vec())
                .collect::<Vec<_>>(),
            1e-10
        ));
    }

    #[test]
    fn test_schur_decomposition() {
        let m = matrix(vec![vec![1, 2], vec![3, 4]]);
        let result = builtin_schur_decomposition(&[m]).unwrap();
        let parts = as_list(&result).unwrap();
        let q = as_f64_matrix(&parts[0]);
        let t = as_f64_matrix(&parts[1]);
        assert!(t[1][0].abs() < 1e-8, "t not upper triangular: {:?}", t);
        let a = [[1.0, 2.0], [3.0, 4.0]];
        let rec = mmul(&mmul(&q, &t), &mtranspose(&q));
        assert!(close(
            &rec,
            &a.iter().map(|r| r.to_vec()).collect::<Vec<_>>(),
            1e-8
        ));
        assert!(close(&mmul(&q, &mtranspose(&q)), &identity_f64(2), 1e-8));
    }

    #[test]
    fn test_hessenberg_decomposition() {
        let a = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
        let m = real_matrix(a.iter().map(|r| r.to_vec()).collect());
        let result = builtin_hessenberg_decomposition(&[m]).unwrap();
        let parts = as_list(&result).unwrap();
        let p = as_f64_matrix(&parts[0]);
        let h = as_f64_matrix(&parts[1]);
        assert!(h[2][0].abs() < 1e-10, "h not Hessenberg: {:?}", h);
        let rec = mmul(&mmul(&p, &h), &mtranspose(&p));
        assert!(close(
            &rec,
            &a.iter().map(|r| r.to_vec()).collect::<Vec<_>>(),
            1e-8
        ));
        assert!(close(&mmul(&p, &mtranspose(&p)), &identity_f64(3), 1e-8));
    }

    #[test]
    fn test_jordan_decomposition_defective() {
        let m = matrix(vec![vec![2, 1], vec![0, 2]]);
        let result = builtin_jordan_decomposition(&[m]).unwrap();
        let parts = as_list(&result).unwrap();
        let s = as_f64_matrix(&parts[0]);
        let j = as_f64_matrix(&parts[1]);
        assert!((j[0][0] - 2.0).abs() < 1e-8);
        assert!((j[1][1] - 2.0).abs() < 1e-8);
        assert!((j[0][1] - 1.0).abs() < 1e-8);
        assert!(j[1][0].abs() < 1e-8);
        let sinv = cmat_inv(
            &s.iter()
                .map(|r| r.iter().map(|&x| (x, 0.0)).collect())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let sinv_f: Vec<Vec<f64>> = sinv
            .iter()
            .map(|r| r.iter().map(|c| c.0).collect())
            .collect();
        let rec = mmul(&mmul(&s, &j), &sinv_f);
        assert!(close(
            &rec,
            &[[2.0, 1.0], [0.0, 2.0]]
                .iter()
                .map(|r| r.to_vec())
                .collect::<Vec<_>>(),
            1e-8
        ));
    }

    #[test]
    fn test_jordan_decomposition_diagonalizable() {
        let m = matrix(vec![vec![2, 0], vec![0, 3]]);
        let result = builtin_jordan_decomposition(&[m]).unwrap();
        let parts = as_list(&result).unwrap();
        let j = as_f64_matrix(&parts[1]);
        assert!(j[0][1].abs() < 1e-8 && j[1][0].abs() < 1e-8);
        let diag: Vec<f64> = vec![j[0][0], j[1][1]];
        assert!((diag[0] - 3.0).abs() < 1e-8 || (diag[0] - 2.0).abs() < 1e-8);
    }

    #[test]
    fn test_matrix_log_reconstructs() {
        let m = matrix(vec![vec![2, 0], vec![0, 3]]);
        let result = builtin_matrix_log(&[m]).unwrap();
        let log = as_f64_matrix(&result);
        assert!((log[0][0] - 2f64.ln()).abs() < 1e-10);
        assert!((log[1][1] - 3f64.ln()).abs() < 1e-10);
        // exp(log(a)) == a
        let back = builtin_matrix_exp(&[result]).unwrap();
        let back = as_f64_matrix(&back);
        assert!(close(
            &back,
            &[[2.0, 0.0], [0.0, 3.0]]
                .iter()
                .map(|r| r.to_vec())
                .collect::<Vec<_>>(),
            1e-8
        ));
    }

    #[test]
    fn test_matrix_log_identity() {
        let m = matrix(vec![vec![1, 0], vec![0, 1]]);
        let result = builtin_matrix_log(&[m]).unwrap();
        let log = as_f64_matrix(&result);
        assert!(close(
            &log,
            &identity_f64(2)
                .iter()
                .map(|r| r.iter().map(|_| 0.0).collect())
                .collect::<Vec<_>>(),
            1e-10
        ));
    }

    #[test]
    fn test_conjugate_transpose() {
        let m = list(vec![
            list(vec![int_val(1), Value::Complex { re: 0.0, im: 1.0 }]),
            list(vec![int_val(2), int_val(3)]),
        ]);
        let result = builtin_conjugate_transpose(&[m]).unwrap();
        let expected = list(vec![
            list(vec![int_val(1), int_val(2)]),
            list(vec![Value::Complex { re: 0.0, im: -1.0 }, int_val(3)]),
        ]);
        assert_eq!(result, expected);
        // Vector → 1×n matrix.
        assert_eq!(
            builtin_conjugate_transpose(&[list(vec![int_val(1), int_val(2)])]).unwrap(),
            list(vec![list(vec![int_val(1), int_val(2)])])
        );
    }

    #[test]
    fn test_adjugate() {
        assert_eq!(
            builtin_adjugate(&[matrix(vec![vec![1, 2], vec![3, 4]])]).unwrap(),
            matrix(vec![vec![4, -2], vec![-3, 1]])
        );
        // m · adjugate[m] == det(m) · I
        let m = matrix(vec![vec![2, 1, 0], vec![1, 3, 1], vec![0, 1, 2]]);
        let adj = builtin_adjugate(&[m.clone()]).unwrap();
        assert_eq!(
            adj,
            matrix(vec![vec![5, -2, 1], vec![-2, 4, -2], vec![1, -2, 5]])
        );
    }

    #[test]
    fn test_levi_civita_tensor() {
        assert_eq!(
            builtin_levi_civita_tensor(&[int_val(2)]).unwrap(),
            matrix(vec![vec![0, 1], vec![-1, 0]])
        );
        let t = builtin_levi_civita_tensor(&[int_val(3)]).unwrap();
        let t = as_list(&t).unwrap();
        let row0 = as_list(&t[0]).unwrap();
        let middle = as_list(&row0[1]).unwrap();
        assert_eq!(middle[2], int_val(1));
        let last = as_list(&row0[2]).unwrap();
        assert_eq!(last[1], int_val(-1));
    }

    #[test]
    fn test_kronecker_product() {
        let result = super::super::linalg::builtin_kronecker_product(&[
            matrix(vec![vec![1, 2], vec![3, 4]]),
            matrix(vec![vec![0, 1], vec![1, 0]]),
        ])
        .unwrap();
        assert_eq!(
            result,
            matrix(vec![
                vec![0, 1, 0, 2],
                vec![1, 0, 2, 0],
                vec![0, 3, 0, 4],
                vec![3, 0, 4, 0],
            ])
        );
    }

    #[test]
    fn test_rotation_matrix() {
        assert_eq!(
            builtin_rotation_matrix(&[int_val(0)]).unwrap(),
            matrix(vec![vec![1, 0], vec![0, 1]])
        );
        let r = builtin_rotation_matrix(&[super::super::real(0.5)]).unwrap();
        let r = as_f64_matrix(&r);
        assert!((r[0][0] - 0.5f64.cos()).abs() < 1e-12);
        assert!((r[0][1] + 0.5f64.sin()).abs() < 1e-12);
        let r3 = builtin_rotation_matrix(&[
            super::super::real(0.0),
            list(vec![int_val(0), int_val(0), int_val(1)]),
        ])
        .unwrap();
        assert!(close(&as_f64_matrix(&r3), &identity_f64(3), 1e-12));
    }

    #[test]
    fn test_translation_transform() {
        assert_eq!(
            builtin_translation_transform(&[list(vec![int_val(1), int_val(2)])]).unwrap(),
            matrix(vec![vec![1, 0, 1], vec![0, 1, 2], vec![0, 0, 1]])
        );
    }

    #[test]
    fn test_scaling_and_reflection_matrices() {
        assert_eq!(
            builtin_scaling_matrix(&[list(vec![int_val(2), int_val(3)])]).unwrap(),
            matrix(vec![vec![2, 0], vec![0, 3]])
        );
        assert_eq!(
            builtin_reflection_matrix(&[list(vec![int_val(1), int_val(0)])]).unwrap(),
            matrix(vec![vec![-1, 0], vec![0, 1]])
        );
    }

    #[test]
    fn test_rotation_transform() {
        assert_eq!(
            builtin_rotation_transform(&[int_val(0)]).unwrap(),
            matrix(vec![vec![1, 0, 0], vec![0, 1, 0], vec![0, 0, 1],])
        );
    }

    #[test]
    fn test_unitary_matrix_q() {
        assert_eq!(
            builtin_unitary_matrix_q(&[matrix(vec![vec![0, -1], vec![1, 0]])]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_unitary_matrix_q(&[matrix(vec![vec![1, 2], vec![3, 4]])]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_antisymmetric_matrix_q() {
        assert_eq!(
            builtin_antisymmetric_matrix_q(&[matrix(vec![vec![0, 1], vec![-1, 0]])]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_antisymmetric_matrix_q(&[matrix(vec![vec![0, 1], vec![1, 0]])]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_positive_semidefinite_matrix_q() {
        assert_eq!(
            builtin_positive_semidefinite_matrix_q(&[matrix(vec![vec![2, 1], vec![1, 2]])])
                .unwrap(),
            Value::Bool(true)
        );
        // Singular but PSD.
        assert_eq!(
            builtin_positive_semidefinite_matrix_q(&[matrix(vec![vec![1, 1], vec![1, 1]])])
                .unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_positive_semidefinite_matrix_q(&[matrix(vec![vec![-1, 0], vec![0, -1]])])
                .unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_sparse_array_roundtrip() {
        let sa = builtin_sparse_array(&[Value::List(vec![
            Value::Rule {
                lhs: Box::new(Value::List(vec![int(1), int(1)])),
                rhs: Box::new(int(5)),
                delayed: false,
            },
            Value::Rule {
                lhs: Box::new(Value::List(vec![int(2), int(2)])),
                rhs: Box::new(int(7)),
                delayed: false,
            },
        ])])
        .unwrap();
        assert!(is_sparse(&sa));
        assert_eq!(
            builtin_sparse_array_q(&[sa.clone()]).unwrap(),
            Value::Bool(true)
        );
        let dense = normal_dense(&sa).unwrap();
        assert_eq!(dense.to_string(), "{{5, 0}, {0, 7}}");
    }

    #[test]
    fn test_array_rules_dense() {
        let dense = Value::List(vec![int(3), int(0), int(4)]);
        let rules = builtin_array_rules(&[dense]).unwrap().to_string();
        assert!(rules.contains("3"), "got {rules}");
        assert!(rules.contains("4"), "got {rules}");
    }

    #[test]
    fn test_sparse_matrix_multiply() {
        let a = builtin_sparse_array(&[Value::List(vec![Value::Rule {
            lhs: Box::new(Value::List(vec![int(1), int(1)])),
            rhs: Box::new(int(2)),
            delayed: false,
        }])])
        .unwrap();
        let b = builtin_sparse_array(&[Value::List(vec![Value::Rule {
            lhs: Box::new(Value::List(vec![int(1), int(1)])),
            rhs: Box::new(int(3)),
            delayed: false,
        }])])
        .unwrap();
        let prod = builtin_sparse_matrix_multiply(&[a, b]).unwrap();
        let dense = normal_dense(&prod).unwrap();
        assert_eq!(dense.to_string(), "{{6}}");
    }
}
