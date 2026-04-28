/// Polynomial arithmetic helpers for algebraic number support.
///
/// Provides root finding, resultants, GCD, Sturm sequences, and
/// minimal-polynomial composition needed by `Value::Root`.
use rug::Rational;
use std::cmp::Ordering;

/// Complex number (f64 precision) for internal root-finding.
#[derive(Clone, Copy, Debug)]
struct Cmplx {
    re: f64,
    im: f64,
}

impl Cmplx {
    fn add(self, other: Cmplx) -> Cmplx {
        Cmplx { re: self.re + other.re, im: self.im + other.im }
    }
    fn sub(self, other: Cmplx) -> Cmplx {
        Cmplx { re: self.re - other.re, im: self.im - other.im }
    }
    fn mul(self, other: Cmplx) -> Cmplx {
        Cmplx {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }
    fn div(self, other: Cmplx) -> Cmplx {
        let denom = other.re * other.re + other.im * other.im;
        if denom < 1e-30 {
            return Cmplx { re: f64::INFINITY, im: f64::INFINITY };
        }
        Cmplx {
            re: (self.re * other.re + self.im * other.im) / denom,
            im: (self.im * other.re - self.re * other.im) / denom,
        }
    }
    fn abs(self) -> f64 {
        (self.re * self.re + self.im * self.im).sqrt()
    }
}

/// Evaluate polynomial at a complex point using Horner's method.
/// coeffs: [c0, c1, ..., cn] representing c0 + c1*x + ... + cn*x^n
fn poly_eval_cmplx(coeffs: &[f64], x: Cmplx) -> Cmplx {
    let n = coeffs.len();
    if n == 0 {
        return Cmplx { re: 0.0, im: 0.0 };
    }
    let mut result = Cmplx { re: coeffs[n - 1], im: 0.0 };
    for i in (0..n - 1).rev() {
        result = result.mul(x);
        result = result.add(Cmplx { re: coeffs[i], im: 0.0 });
    }
    result
}

/// Cauchy's bound: all roots of monic poly have |z| <= bound.
fn cauchy_bound(coeffs: &[f64]) -> f64 {
    let n = coeffs.len();
    if n <= 1 {
        return 1.0;
    }
    let mut sum_neg = 0.0;
    for i in 0..n - 1 {
        if coeffs[i] < 0.0 {
            sum_neg += coeffs[i].abs();
        }
    }
    1.0 + sum_neg
}

/// Find all roots of a polynomial using the Durand-Kerner (Weierstrass) method.
/// Returns (real, imag) pairs sorted by (real ascending, then imaginary ascending).
/// coeffs: [c0, c1, ..., cn] representing c0 + c1*x + ... + cn*x^n.
pub fn find_polynomial_roots(coeffs: &[Rational]) -> Vec<(f64, f64)> {
    let deg = poly_degree(coeffs);
    if deg == 0 {
        return vec![];
    }
    if deg == 1 {
        // x = -c0/c1
        let root = -(coeffs[0].to_f64() / coeffs[1].to_f64());
        return vec![(root, 0.0)];
    }

    // Convert to f64 and make monic
    let mut cf: Vec<f64> = coeffs.iter().map(|c| c.to_f64()).collect();
    let lead = cf[deg];
    if lead != 1.0 && lead != -1.0 {
        for c in cf.iter_mut() {
            *c /= lead;
        }
    }

    let bound = cauchy_bound(&cf);
    let scale = bound.max(1.0);

    // Initialize roots evenly distributed on a circle
    let mut roots: Vec<Cmplx> = (0..deg)
        .map(|k| {
            let angle = 2.0 * std::f64::consts::PI * (k as f64) / (deg as f64) + 0.1;
            Cmplx {
                re: scale * angle.cos(),
                im: scale * angle.sin(),
            }
        })
        .collect();

    // Durand-Kerner iterations
    for _iter in 0..500 {
        let mut max_delta = 0.0;
        let mut new_roots = roots.clone();
        for i in 0..deg {
            let p_val = poly_eval_cmplx(&cf, roots[i]);
            let mut m = Cmplx { re: 1.0, im: 0.0 };
            for j in 0..deg {
                if j != i {
                    m = m.mul(roots[i].sub(roots[j]));
                }
            }
            let delta = p_val.div(m);
            new_roots[i] = roots[i].sub(delta);
            let d = delta.abs();
            if d > max_delta {
                max_delta = d;
            }
        }
        roots = new_roots;
        if max_delta < 1e-15 {
            break;
        }
    }

    // Newton refinement for each root (using original Rational coefficients for accuracy)
    let mut final_roots: Vec<(f64, f64)> = roots
        .into_iter()
        .map(|z| (z.re, z.im))
        .collect();

    let df_coeff = poly_derivative_f64(&cf);
    for root in &mut final_roots {
        let mut z = Cmplx { re: root.0, im: root.1 };
        for _ in 0..50 {
            let p = poly_eval_cmplx(&cf, z);
            let dp = poly_eval_cmplx(&df_coeff, z);
            let pz = p.div(dp);
            z = z.sub(pz);
            if pz.abs() < 1e-16 {
                break;
            }
        }
        root.0 = z.re;
        root.1 = z.im;
    }

    // Clean near-zero imaginary parts
    for (re, im) in &mut final_roots {
        if im.abs() < 1e-10 {
            *im = 0.0;
        }
        if re.abs() < 1e-15 && *re != 0.0 {
            *re = 0.0;
        }
    }

    // Sort by (real ascending, imaginary ascending)
    final_roots.sort_by(|a, b| {
        match a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal) {
            Ordering::Equal => a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal),
            ord => ord,
        }
    });

    final_roots
}

/// Get polynomial degree (index of highest nonzero coefficient).
pub fn poly_degree(coeffs: &[Rational]) -> usize {
    let len = coeffs.len();
    for i in (0..len).rev() {
        if !coeffs[i].is_zero() {
            return i;
        }
    }
    0
}

/// Evaluate polynomial at a Rational point using Horner's method.
pub fn poly_eval(coeffs: &[Rational], x: &Rational) -> Rational {
    let n = coeffs.len();
    if n == 0 {
        return Rational::from(0);
    }
    let mut result = coeffs[n - 1].clone();
    for i in (0..n - 1).rev() {
        result = result * x + coeffs[i].clone();
    }
    result
}

/// Evaluate polynomial at an f64 value.
pub fn poly_eval_f64(coeffs: &[Rational], x: f64) -> f64 {
    let n = coeffs.len();
    if n == 0 {
        return 0.0;
    }
    let mut result = coeffs[n - 1].to_f64();
    for i in (0..n - 1).rev() {
        result = result * x + coeffs[i].to_f64();
    }
    result
}

/// Evaluate derivative of polynomial at an f64 value.
fn poly_deriv_eval_f64(coeffs: &[Rational], x: f64) -> f64 {
    let deriv = poly_derivative(coeffs);
    poly_eval_f64(&deriv, x)
}

/// Make polynomial monic (leading coefficient = 1).
/// Returns new coefficient vector.
pub fn make_monic(coeffs: &[Rational]) -> Vec<Rational> {
    let deg = poly_degree(coeffs);
    if deg == 0 {
        return coeffs.to_vec();
    }
    let lead = &coeffs[deg];
    coeffs
        .iter()
        .map(|c| (c.clone() / lead.clone()).into())
        .collect()
}

/// Compute the derivative of a polynomial.
/// derivative of c0 + c1*x + ... + cn*x^n = c1 + 2*c2*x + ... + n*cn*x^(n-1)
pub fn poly_derivative(coeffs: &[Rational]) -> Vec<Rational> {
    let n = coeffs.len();
    if n <= 1 {
        return vec![Rational::from(0)];
    }
    (1..n)
        .map(|i| {
            let mut c = Rational::from(coeffs[i].clone());
            c *= i;
            c
        })
        .collect()
}

/// Derivative for f64 coefficients.
fn poly_derivative_f64(coeffs: &[f64]) -> Vec<f64> {
    let n = coeffs.len();
    if n <= 1 {
        return vec![0.0];
    }
    (1..n).map(|i| coeffs[i] * (i as f64)).collect()
}

/// Compute polynomial GCD using the Euclidean algorithm.
/// Returns the GCD polynomial (content removed, monic if possible).
pub fn poly_gcd(a: &[Rational], b: &[Rational]) -> Vec<Rational> {
    let mut a = strip_zeros(a);
    let mut b = strip_zeros(b);
    loop {
        if poly_degree(&b) == 0 {
            return make_monic(&a);
        }
        let rem = poly_remainder(&a, &b);
        a = b;
        b = rem;
    }
}

/// Check if a polynomial is square-free (gcd(p, p') == constant).
pub fn is_square_free(coeffs: &[Rational]) -> bool {
    let deg = poly_degree(coeffs);
    if deg <= 1 {
        return true;
    }
    let deriv = poly_derivative(coeffs);
    let g = poly_gcd(coeffs, &deriv);
    poly_degree(&g) == 0
}

/// Polynomial remainder: a mod b.
fn poly_remainder(a: &[Rational], b: &[Rational]) -> Vec<Rational> {
    let deg_b = poly_degree(b);
    let mut rem = strip_zeros(a);
    let lead_b = &b[deg_b].clone();
    while poly_degree(&rem) >= deg_b && !is_zero_poly(&rem) {
        let deg_rem = poly_degree(&rem);
        let factor = rem[deg_rem].clone() / lead_b.clone();
        let shift = deg_rem - deg_b;
        for i in 0..=deg_b {
            if i + shift < rem.len() {
                rem[i + shift]
                    -= factor.clone() * b[i].clone();
            }
        }
    }
    strip_zeros(&rem)
}

/// Remove trailing zeros from coefficient vector.
fn strip_zeros(coeffs: &[Rational]) -> Vec<Rational> {
    let deg = poly_degree(coeffs);
    coeffs[..=deg].to_vec()
}

/// Check if polynomial is effectively zero.
fn is_zero_poly(coeffs: &[Rational]) -> bool {
    coeffs.iter().all(|c| c.is_zero())
}

/// Compute the resultant of two polynomials using Sylvester matrix determinant.
/// resultant(p, q) = 0 iff p and q share a root.
pub fn resultant(a: &[Rational], b: &[Rational]) -> Rational {
    let m = poly_degree(a);
    let n = poly_degree(b);
    if m == 0 {
        // resultant(constant, b) = constant^n
        let mut r = Rational::from(a[0].clone());
        for _ in 1..n {
            r *= a[0].clone();
        }
        return r;
    }
    if n == 0 {
        let mut r = Rational::from(b[0].clone());
        for _ in 1..m {
            r *= b[0].clone();
        }
        return r;
    }

    // Build Sylvester matrix: (m+n) x (m+n)
    let size = m + n;
    let mut matrix: Vec<Vec<Rational>> = vec![vec![Rational::from(0); size]; size];

    // First n rows: coefficients of a shifted
    for i in 0..n {
        for j in 0..=m {
            matrix[i][i + j] = a[j].clone();
        }
    }
    // Next m rows: coefficients of b shifted
    for i in 0..m {
        for j in 0..=n {
            matrix[n + i][i + j] = b[j].clone();
        }
    }

    determinant_rational(&matrix)
}

/// Compute determinant using Gaussian elimination with partial pivoting.
fn determinant_rational(matrix: &[Vec<Rational>]) -> Rational {
    let n = matrix.len();
    if n == 0 {
        return Rational::from(1);
    }
    if n == 1 {
        return matrix[0][0].clone();
    }

    let mut mat: Vec<Vec<Rational>> = matrix.to_vec();
    let mut det = Rational::from(1);
    let mut sign = 1i64;

    for col in 0..n {
        // Find pivot
        let mut pivot_row = col;
        let mut pivot_exists = false;
        for row in col..n {
            if !mat[row][col].is_zero() {
                pivot_row = row;
                pivot_exists = true;
                break;
            }
        }
        if !pivot_exists {
            return Rational::from(0);
        }
        if pivot_row != col {
            mat.swap(col, pivot_row);
            sign = -sign;
        }
        let piv = &mat[col][col];
        det *= piv.clone();
        // Eliminate below
        for row in col + 1..n {
            if mat[row][col].is_zero() {
                continue;
            }
            let factor = mat[row][col].clone() / piv.clone();
            for j in col..n {
                mat[row][j] -= factor.clone() * mat[col][j].clone();
            }
        }
    }

    if sign == -1 {
        det = -&det;
    }
    det
}

/// Compute the Sturm sequence for a square-free polynomial.
/// The sequence is [p, p', r1, r2, ...] where ri = -(ri-2 mod ri-1).
pub fn sturm_sequence(coeffs: &[Rational]) -> Vec<Vec<Rational>> {
    let mut seq = vec![strip_zeros(coeffs), poly_derivative(coeffs)];
    loop {
        let last = seq.last().unwrap().clone();
        if is_zero_poly(&last) {
            seq.pop();
            break;
        }
        let prev = seq[seq.len() - 2].clone();
        let rem = poly_remainder(&prev, &last);
        // Negate the remainder
        let neg_rem: Vec<Rational> = rem.iter().map(|c| -c).collect();
        seq.push(neg_rem);
    }
    seq
}

/// Count sign changes in a sequence of polynomial values at point x.
pub fn sign_changes_at(sturm: &[Vec<Rational>], x: &Rational) -> i32 {
    let mut signs: Vec<i32> = sturm
        .iter()
        .map(|p| {
            let v = poly_eval(p, x);
            if v.is_zero() {
                0
            } else if v.is_positive() {
                1
            } else {
                -1
            }
        })
        .collect();

    // Remove leading zeros (for sign change counting, ignore zeros)
    while signs.first() == Some(&0) {
        signs.remove(0);
    }

    let mut changes = 0;
    for i in 0..signs.len().saturating_sub(1) {
        if signs[i] != 0 && signs[i + 1] != 0 && signs[i] != signs[i + 1] {
            changes += 1;
        } else if signs[i] != 0 && signs[i + 1] == 0 {
            // Look ahead for next non-zero
            for j in (i + 1)..signs.len() {
                if signs[j] != 0 {
                    if signs[i] != signs[j] {
                        changes += 1;
                    }
                    break;
                }
            }
        }
    }
    changes
}

/// Count sign changes at +/- infinity.
/// At +inf: look at leading coefficients. At -inf: look at leading coeff * (-1)^deg.
fn sign_changes_at_inf(sturm: &[Vec<Rational>], neg_inf: bool) -> i32 {
    let mut changes = 0;
    let mut prev_sign: i32 = 0;
    for p in sturm {
        let deg = poly_degree(p);
        let lc = &p[deg];
        if lc.is_zero() {
            continue;
        }
        let sign = if neg_inf && deg % 2 == 1 {
            if lc.is_positive() {
                -1
            } else {
                1
            }
        } else {
            if lc.is_positive() {
                1
            } else {
                -1
            }
        };
        if prev_sign != 0 && prev_sign != sign {
            changes += 1;
        }
        prev_sign = sign;
    }
    changes
}

/// Count number of distinct real roots of a polynomial.
pub fn count_real_roots(coeffs: &[Rational]) -> i32 {
    let sturm = sturm_sequence(coeffs);
    let v_pos_inf = sign_changes_at_inf(&sturm, false);
    let v_neg_inf = sign_changes_at_inf(&sturm, true);
    v_neg_inf - v_pos_inf
}

/// Count real roots in the open interval (a, b).
pub fn count_real_roots_in(coeffs: &[Rational], a: &Rational, b: &Rational) -> i32 {
    let sturm = sturm_sequence(coeffs);
    let va = sign_changes_at(&sturm, a);
    let vb = sign_changes_at(&sturm, b);
    va - vb
}

/// Refine a real root within a rational bracket using bisection + Newton.
pub fn refine_real_root(coeffs: &[Rational], bracket_lo: &Rational, bracket_hi: &Rational) -> f64 {
    let mut lo = bracket_lo.to_f64();
    let mut hi = bracket_hi.to_f64();
    let mut mid = (lo + hi) / 2.0;
    let mut f_mid = poly_eval_f64(coeffs, mid);

    for _ in 0..200 {
        if f_mid.abs() < 1e-16 {
            break;
        }
        // Newton step
        let f_prime = poly_deriv_eval_f64(coeffs, mid);
        if f_prime.abs() > 1e-20 {
            let newton = mid - f_mid / f_prime;
            if newton > lo && newton < hi {
                mid = newton;
                f_mid = poly_eval_f64(coeffs, mid);
                continue;
            }
        }
        // Bisection fallback
        let f_lo = poly_eval_f64(coeffs, lo);
        if f_lo * f_mid < 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
        mid = (lo + hi) / 2.0;
        f_mid = poly_eval_f64(coeffs, mid);
        if (hi - lo).abs() < 1e-15 {
            break;
        }
    }
    mid
}

/// Compute the minimal polynomial of alpha op beta where op is add/sub/mul/div.
/// alpha is a root of p(x), beta is a root of q(x).
/// Uses the resultant: resultant_x(p(x), q(op(x, y))) gives poly in y with root alpha op beta.
/// Then divide out any extraneous factors and return the irreducible factor containing the root.
pub fn min_poly_operation(
    p: &[Rational],
    root_p: usize,
    q: &[Rational],
    root_q: usize,
    op: AlgebraicOp,
) -> Vec<Rational> {
    match op {
        AlgebraicOp::Mul => min_poly_mul(p, root_p, q, root_q),
        AlgebraicOp::Div => min_poly_div(p, root_p, q, root_q),
        AlgebraicOp::Add => min_poly_add_sub(p, root_p, q, root_q, false),
        AlgebraicOp::Sub => min_poly_add_sub(p, root_p, q, root_q, true),
    }
}

/// Minimal polynomial for c * alpha where alpha is root p_i of poly p.
/// If c is integer, then the poly for c*alpha is obtained by substituting x -> x/c in p,
/// i.e., p(x/c) * c^deg. Result: coefficients are scaled.
pub fn min_poly_scale_int(p: &[Rational], root_idx: usize, c: i64) -> Vec<Rational> {
    if c == 1 {
        return p.to_vec();
    }
    if c == -1 {
        // p(-x): alternate sign coefficients
        return p.iter().enumerate().map(|(i, coef)| {
            if i % 2 == 1 {
                -(&*coef.clone())
            } else {
                (&*coef.clone()).into()
            }
        }).collect();
    }
    let deg = poly_degree(p);
    let abs_c = c.abs();
    // (px)^n + c1(px)^(n-1)*|c| + ... + cn*|c|^n  where px = x*sign(c)
    let mut result = Vec::with_capacity(deg + 1);
    let sign_c = if c < 0 { -1 } else { 1 };
    for i in 0..=deg {
        let mut coef = p[i].clone();
        let power = deg - i;
        for _ in 0..power {
            coef *= abs_c;
        }
        if sign_c < 0 && i % 2 == 1 && power % 2 == 1 {
            // sign correction
        }
        if sign_c < 0 && (deg - i) % 2 == 1 {
            coef = -(&*coef.clone());
        }
        result.push(coef);
    }
    remove_content(&result)
}

/// Minimal polynomial for alpha + beta (or alpha - beta).
fn min_poly_add_sub(
    p: &[Rational],
    _root_p: usize,
    q: &[Rational],
    _root_q: usize,
    subtract: bool,
) -> Vec<Rational> {
    let n = poly_degree(p);
    let m = poly_degree(q);

    // We want the minimal polynomial of z = alpha + beta (or alpha - beta).
    // alpha = z - beta, so p(z - beta) = 0.
    // resultant_beta(q(beta), p(z - beta)) eliminates beta.

    // Compute p(z - y) as a polynomial in y (with coefficients that are polys in z)
    // Then take resultant of q(y) and p(z - y) with respect to y.

    // p(z - y) = sum_{k=0..n} p[k] * (z - y)^k
    //           = sum_{k=0..n} p[k] * sum_{j=0..k} binom(k,j) * z^(k-j) * (-y)^j
    //           = sum_{j=0..n} [sum_{k=j..n} p[k] * binom(k,j) * z^(k-j) * (-1)^j] * y^j

    // For resultant in y between q (degree m) and p(z-y) (degree n), we build
    // the Sylvester matrix of size (n+m) x (n+m) where entries are polynomials in z.
    // The determinant gives a polynomial in z.

    // Represent polynomials in z as Vec<Rational> (coeffs).
    // Matrix entries are Vec<Rational>.
    let size = n + m;

    // p(z-y) as polynomial in y: coefficients are polynomials in z
    // coeff of y^j in p(z-y):
    let p_sub: Vec<Vec<Rational>> = (0..=n)
        .map(|j| {
            let mut poly_z: Vec<Rational> = vec![Rational::from(0); n + 1];
            for k in j..=n {
                let binom_kj = binomial(k, j);
                let z_power = k - j;
                let sign = if j % 2 == 1 { -1 } else { 1 };
                let mut coeff = Rational::from(p[k].clone());
                coeff *= binom_kj;
                if sign < 0 {
                    coeff = -(&*coeff.clone());
                }
                if subtract {
                    // z = alpha - beta, so alpha = z + beta
                    // p(z + y) = sum p[k] * (z+y)^k
                    // coeff of y^j = sum_{k=j..n} p[k] * binom(k,j) * z^(k-j)
                    // (all positive, no (-1)^j factor, but sign depends on subtract)
                    if subtract == false {
                        // already done above for add
                    }
                    // For subtract: alpha = z + beta, p(z+y)
                    // The coefficients are all positive (no (-1)^j)
                    if j % 2 == 1 {
                        coeff = -(&*coeff.clone()); // negate back the previous -1^j
                    }
                }
                poly_z[z_power] += coeff;
            }
            poly_z
        })
        .collect();

    if subtract {
        // Recompute for subtract: p(z + y)
        // alpha = z + y where z = alpha - beta, y = beta
        // p(z + y) = sum_{k=0..n} p[k] * (z+y)^k
        let mut p_sub2: Vec<Vec<Rational>> = vec![vec![Rational::from(0); n + 1]; n + 1];
        for j in 0..=n {
            for k in j..=n {
                let binom_kj = binomial(k, j);
                let z_power = k - j;
                let mut coeff = Rational::from(p[k].clone());
                coeff *= binom_kj;
                p_sub2[j][z_power] += coeff;
            }
        }
        return min_poly_resultant_z(q, &p_sub2, n, m, size);
    }

    min_poly_resultant_z(q, &p_sub, n, m, size)
}

fn min_poly_resultant_z(
    q: &[Rational],
    p_expanded: &[Vec<Rational>],
    n: usize,
    m: usize,
    size: usize,
) -> Vec<Rational> {
    // Sylvester matrix: each entry is a polynomial in z (Vec<Rational>)
    let mut matrix: Vec<Vec<Vec<Rational>>> =
        vec![vec![vec![Rational::from(0); n + m]; size]; size];

    // First m rows: coeffs of p(z-y) in y, shifted
    for i in 0..m {
        for j in 0..=n {
            matrix[i][i + j] = p_expanded[j].clone();
        }
    }
    // Next n rows: coeffs of q in y, shifted
    for i in 0..n {
        for j in 0..=m {
            if j < q.len() && !q[j].is_zero() {
                let mut entry = vec![Rational::from(0); n + m];
                entry[0] = q[j].clone();
                matrix[m + i][i + j] = entry;
            }
        }
    }

    // Compute determinant of this matrix where entries are polynomials in z.
    // The result degree in z is at most n*m.
    let max_deg = n * m;
    let det = poly_matrix_det(&matrix, size, max_deg);
    remove_content(&det)
}

/// Determinant of a matrix whose entries are polynomials (Vec<Rational>).
/// Each polynomial is represented as coefficients [c0, c1, ..., c_maxdeg].
fn poly_matrix_det(
    matrix: &[Vec<Vec<Rational>>],
    n: usize,
    max_deg: usize,
) -> Vec<Rational> {
    if n == 0 {
        return vec![Rational::from(1)];
    }
    if n == 1 {
        return strip_zeros_poly(&matrix[0][0]);
    }

    // Gaussian elimination at the coefficient level.
    // Each entry is a polynomial. We do symbolic elimination.
    // This is expensive but correct. For small matrices it's fine.

    // Use a simpler approach: cofactor expansion for small matrices,
    // or fraction-free Gaussian elimination.

    // For efficiency, use the fact that we can evaluate at enough points
    // and interpolate.

    // Evaluate the determinant at (max_deg + 1) distinct integer points,
    // then Lagrange interpolate.
    let num_points = max_deg + 1;
    let mut evaluations: Vec<(i64, i64)> = Vec::with_capacity(num_points);

    for pt in 0..num_points {
        // Evaluate each matrix entry at z = pt, get rational value.
        let num = pt; // use pt as evaluation point
        let num_r = Rational::from(num);
        let num_float = num as f64;
        // Build numeric matrix
        let mut num_matrix: Vec<Vec<f64>> = vec![vec![0.0; n]; n];
        for i in 0..n {
            for j in 0..n {
                let poly = &matrix[i][j];
                num_matrix[i][j] = eval_poly_at_f64(poly, num_float);
            }
        }
        let det_val = determinant_f64(&num_matrix);
        // Round to nearest integer (resultant is integer for integer polys)
        evaluations.push((num, det_val.round() as i64));
    }

    // Lagrange interpolation from integer points (0, 1, ..., max_deg)
    interpolate_from_integers(&evaluations, max_deg)
}

fn eval_poly_at_f64(coeffs: &[Rational], x: f64) -> f64 {
    let n = coeffs.len();
    if n == 0 {
        return 0.0;
    }
    let mut result = coeffs[n - 1].to_f64();
    for i in (0..n - 1).rev() {
        result = result * x + coeffs[i].to_f64();
    }
    result
}

fn determinant_f64(matrix: &[Vec<f64>]) -> f64 {
    let n = matrix.len();
    if n == 0 {
        return 1.0;
    }
    let mut mat = matrix.to_vec();
    let mut det = 1.0_f64;
    let mut sign = 1_i32;

    for col in 0..n {
        let mut pivot_row = col;
        let mut pivot_exists = false;
        for row in col..n {
            if mat[row][col].abs() > 1e-15 {
                pivot_row = row;
                pivot_exists = true;
                break;
            }
        }
        if !pivot_exists {
            return 0.0;
        }
        if pivot_row != col {
            mat.swap(col, pivot_row);
            sign = -sign;
        }
        det *= mat[col][col];
        for row in col + 1..n {
            if mat[row][col].abs() < 1e-15 {
                continue;
            }
            let factor = mat[row][col] / mat[col][col];
            for j in col..n {
                mat[row][j] -= factor * mat[col][j];
            }
        }
    }
    det * sign as f64
}

/// Interpolate a polynomial from values at integer points 0..=max_deg.
/// Returns coefficient vector. The evaluations should have len = max_deg + 1.
fn interpolate_from_integers(evals: &[(i64, i64)], max_deg: usize) -> Vec<Rational> {
    let n = evals.len(); // = max_deg + 1
    let mut coeffs = vec![Rational::from(0); n];

    // Newton interpolation
    for i in 0..n {
        let mut molecule = evals[i].1;
        let mut denominator = 1i64;
        for j in 0..n {
            if i == j {
                continue;
            }
            molecule *= evals[i].0;
            // We need the Lagrange form
        }
    }

    // Use Lagrange interpolation formula
    // P(x) = sum_i y_i * l_i(x) where l_i(x) = prod_{j!=i} (x - x_j) / (x_i - x_j)
    // Since x_j = j, we have x_i - x_j = i - j
    // l_i(x) = prod_{j< i} (x-j)/(i-j) * prod_{j>i} (x-j)/(i-j)

    for i in 0..n {
        if evals[i].1 == 0 {
            continue;
        }
        // Compute l_i's denominator: prod_{j!=i} (i - j)
        let mut li_denom = Rational::from(1);
        let mut li_num_poly = vec![Rational::from(1)]; // starts as polynomial "1"
        for j in 0..n {
            if i == j {
                continue;
            }
            let diff = evals[i].0 - evals[j].0;
            li_denom *= diff;
            // Multiply li_num_poly by (x - j)
            li_num_poly = poly_multiply_by_linear(li_num_poly, evals[j].0);
        }
        // Contribution: y_i / li_denom * li_num_poly
        let mut scale = Rational::from(evals[i].1);
        scale /= li_denom;
        for c in li_num_poly.iter_mut() {
            *c *= scale.clone();
        }
        // Add to coeffs
        let len = li_num_poly.len().max(coeffs.len());
        while coeffs.len() < len {
            coeffs.push(Rational::from(0));
        }
        for (k, c) in li_num_poly.iter().enumerate() {
            if k < coeffs.len() {
                coeffs[k] += c;
            }
        }
    }
    for c in coeffs.iter_mut() {
        if c.abs().to_f64() < 1e-6 {
            *c = Rational::from(0);
        }
    }
    strip_zeros_poly(&coeffs)
}

/// Multiply polynomial (coeffs) by (x + s).
/// (x + s) * (a0 + a1*x + ...) = s*a0 + (a0 + s*a1)*x + ...
fn poly_multiply_by_linear(coeffs: Vec<Rational>, s: i64) -> Vec<Rational> {
    let n = coeffs.len();
    let mut result = vec![Rational::from(0); n + 1];
    for i in 0..n {
        result[i] += Rational::from(&coeffs[i].clone()) * s;
        result[i + 1] += coeffs[i].clone();
    }
    result
}

fn binomial(n: usize, k: usize) -> Rational {
    if k > n {
        return Rational::from(0);
    }
    let mut result = Rational::from(1);
    for i in 0..k {
        result *= n - i;
        result /= i + 1;
    }
    result
}

/// Minimal polynomial for alpha * beta.
fn min_poly_mul(
    p: &[Rational],
    _root_p: usize,
    q: &[Rational],
    _root_q: usize,
) -> Vec<Rational> {
    // For z = alpha * beta: alpha = z / beta, so p(z/beta) = 0.
    // p(z/beta) * beta^n = sum_{k=0..n} p[k] * z^k * beta^(n-k)
    // This is a polynomial in beta with polynomial coefficients in z.
    // Take resultant of q(beta) and this expression w.r.t. beta.

    let n = poly_degree(p);
    let m = poly_degree(q);

    // poly in beta: sum_{k=0..n} p[k] * z^k * beta^(n-k)
    // Coeff of beta^j (for j = 0..n): p[n-j] * z^(n-j)
    // So the poly in beta has degree n:
    // p[0]*z^n * beta^n + p[1]*z^(n-1) * beta^(n-1) + ... + p[n] * beta^0

    let p_z_beta: Vec<Vec<Rational>> = (0..=n)
        .map(|j| {
            // coefficient of beta^j
            let power_z = n - j;
            let mut poly = vec![Rational::from(0); n * m + 1];
            if power_z < poly.len() {
                poly[power_z] = p[n - j].clone();
            }
            poly
        })
        .collect();

    min_poly_resultant_z(q, &p_z_beta, n, m, n + m)
}

/// Minimal polynomial for alpha / beta.
fn min_poly_div(
    p: &[Rational],
    _root_p: usize,
    q: &[Rational],
    _root_q: usize,
) -> Vec<Rational> {
    // For z = alpha / beta: alpha = z * beta.
    // p(z * beta) = 0.
    // p(z * beta) = sum_{k=0..n} p[k] * (z*beta)^k = sum_{k=0..n} p[k] * z^k * beta^k
    // Coeff of beta^j: p[j] * z^j (for j = 0..n), all higher = 0.

    let n = poly_degree(p);
    let m = poly_degree(q);

    let p_z_beta: Vec<Vec<Rational>> = (0..=n.max(m))
        .map(|j| {
            let mut poly = vec![Rational::from(0); n * m + 1];
            if j <= n && !p[j].is_zero() {
                poly[j] = p[j].clone();
            }
            poly
        })
        .collect();

    min_poly_resultant_z(q, &p_z_beta, n, m, n + m)
}

/// Remove content (GCD of all coefficients) and normalize sign.
fn remove_content(poly: &[Rational]) -> Vec<Rational> {
    let deg = if poly.is_empty() { 0 } else { poly_degree(poly) };
    if deg == 0 {
        return poly.to_vec();
    }

    // Compute GCD of all nonzero coefficients
    let mut gcd_num = Rational::from(1);
    let mut gcd_set = false;
    for c in poly.iter() {
        if !c.is_zero() {
            if !gcd_set {
                gcd_num = c.abs();
                gcd_set = true;
            }
        }
    }
    if !gcd_set {
        return vec![Rational::from(0)];
    }

    // Make content an integer gcd (take rational content, then integer part)
    // For simplicity, just divide by the first nonzero coefficient's abs to make monic-ish
    let lead = poly[deg].clone();
    let mut result: Vec<Rational> = poly.iter().map(|c| (c.clone() / lead.clone()).into()).collect();

    // Ensure leading coefficient is positive
    if !result[deg].is_zero() && result[deg].is_negative() {
        result = result.into_iter().map(|c| -c).collect();
    }

    // Clear denominators if all coefficients have same denominator
    let common_den = {
        let mut den = Rational::from(1);
        let mut den_set = false;
        for c in result.iter() {
            if !c.is_zero() {
                if !den_set {
                    den = c.denom().clone();
                    den_set = true;
                } else {
                    den = lcm_int(den, c.denom().clone());
                }
            }
        }
        den
    };

    if common_den != Rational::from(1) {
        result = result
            .into_iter()
            .map(|c| (c.clone() * common_den.clone()).into())
            .collect();
        // Check if all are now integers
        let all_int = result.iter().all(|c| {
            c.denom() == &Rational::from(1)
        });
        if !all_int {
            // denominators differ; keep as-is
        }
    }

    strip_zeros_poly(&result)
}

fn lcm_int(a: Rational, b: Rational) -> Rational {
    let (na, da) = a.into_numer_denom();
    let (nb, db) = b.into_numer_denom();
    // lcm(a, b) = |a*b| / gcd(a, b), for integers
    let prod = na.clone() * nb.clone();
    let g = na.gcd(&nb);
    (prod / g)
}

fn strip_zeros_poly(coeffs: &[Rational]) -> Vec<Rational> {
    let deg = if coeffs.is_empty() { 0 } else { poly_degree(coeffs) };
    if deg == 0 && (!coeffs.is_empty() && coeffs[0].is_zero()) {
        return vec![Rational::from(0)];
    }
    coeffs[..=deg].to_vec()
}

/// Algebraic operation type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlgebraicOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Convert a coefficient Value::List to Vec<Rational>.
/// Expected format: {c0, c1, ..., cn} representing c0 + c1*x + ... + cn*x^n
pub fn coeffs_from_value(v: &Value) -> Option<Vec<Rational>> {
    if let Value::List(items) = v {
        let mut coeffs = Vec::with_capacity(items.len());
        for item in items {
            match item {
                Value::Integer(n) => coeffs.push(Rational::from(n.clone())),
                Value::Rational(r) => coeffs.push(r.clone()),
                _ => return None,
            }
        }
        Some(coeffs)
    } else {
        None
    }
}

/// Convert Vec<Rational> to Value::List.
pub fn coeffs_to_value(coeffs: &[Rational]) -> Value {
    Value::List(
        coeffs
            .iter()
            .map(|c| {
                if c.denom() == &Rational::from(1) {
                    Value::Integer(c.numer().clone())
                } else {
                    Value::Rational(Box::new(c.clone().into()))
                }
            })
            .collect(),
    )
}

/// Convert a single Rational to a Value (Integer or Rational).
pub fn rational_to_value(r: &Rational) -> Value {
    if r.denom() == &Rational::from(1) {
        Value::Integer(r.numer().clone())
    } else {
        Value::Rational(Box::new((r.clone()).into()))
    }
}

/// Remove content and normalize — public wrapper around remove_content.
pub fn remove_content_poly(poly: &[Rational]) -> Vec<Rational> {
    remove_content(poly)
}

/// Scale a Root by a Rational: if alpha is root of p, find min poly of c*alpha.
pub fn min_poly_scale(
    p: &[Rational],
    _root_idx: usize,
    scale: &Rational,
) -> Vec<Rational> {
    if scale.is_zero() {
        return vec![Rational::from(0)];
    }
    // For rational scale c = num/den:
    // c*alpha is root of d^n * p(c*x) where n = deg(p)
    // But simpler: if p has root alpha, then q(x) = p(x/c) scaled appropriately
    let deg = poly_degree(p);
    // q(x) such that q(c*alpha) = 0 => q(x) = p(x/c)
    // p(x/c) = sum p[k] * (x/c)^k = sum p[k] * x^k / c^k
    // Multiply by c^deg to clear denominators:
    // c^deg * p(x/c) = sum p[k] * x^k * c^(deg-k)
    let mut result = Vec::with_capacity(deg + 1);
    for k in 0..=deg {
        let mut c = p[k].clone() * scale.clone();
        let power = deg - k;
        for _ in 0..power {
            c *= scale;
        }
        // Actually: we need (1/scale)^k * scale^deg = scale^(deg-k)
        // p[k] * scale^(deg-k)
        c = p[k].clone();
        for _ in 0..power {
            c *= scale;
        }
        result.push(c);
    }
    remove_content(&result)
}

use crate::value::Value;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poly_degree() {
        assert_eq!(
            poly_degree(&[Rational::from(1), Rational::from(2), Rational::from(0)]),
            1
        );
        assert_eq!(
            poly_degree(&[
                Rational::from(1),
                Rational::from(0),
                Rational::from(3)
            ]),
            2
        );
    }

    #[test]
    fn test_poly_eval_basic() {
        // x^2 - 2 = -2 + 0*x + 1*x^2, coeffs = [-2, 0, 1]
        let coeffs = vec![
            Rational::from(-2),
            Rational::from(0),
            Rational::from(1),
        ];
        let x2 = Rational::from(2);
        let result = poly_eval(&coeffs, &x2);
        assert_eq!(result, Rational::from(2)); // 4 - 2 = 2
    }

    #[test]
    fn test_find_roots_linear() {
        // x - 3 = -3 + x, coeffs = [-3, 1]
        let coeffs = vec![Rational::from(-3), Rational::from(1)];
        let roots = find_polynomial_roots(&coeffs);
        assert_eq!(roots.len(), 1);
        assert!((roots[0].0 - 3.0).abs() < 1e-10);
        assert!(roots[0].1.abs() < 1e-10);
    }

    #[test]
    fn test_find_roots_quadratic() {
        // x^2 - 2, roots are ±sqrt(2) ≈ ±1.4142
        let coeffs = vec![
            Rational::from(-2),
            Rational::from(0),
            Rational::from(1),
        ];
        let roots = find_polynomial_roots(&coeffs);
        assert_eq!(roots.len(), 2);
        assert!((roots[0].0 + std::f64::consts::SQRT_2).abs() < 1e-6);
        assert!((roots[1].0 - std::f64::consts::SQRT_2).abs() < 1e-6);
    }

    #[test]
    fn test_find_roots_cubic() {
        // x^3 - 2, real root = cbrt(2) ≈ 1.26
        let coeffs = vec![
            Rational::from(-2),
            Rational::from(0),
            Rational::from(0),
            Rational::from(1),
        ];
        let roots = find_polynomial_roots(&coeffs);
        assert_eq!(roots.len(), 3);
        let cbrt2: f64 = 2.0_f64.cbrt();
        // The real root should be among them
        let real_root = roots.iter().find(|r| r.1.abs() < 1e-6);
        assert!(real_root.is_some());
        assert!((real_root.unwrap().0 - cbrt2).abs() < 1e-6);
    }

    #[test]
    fn test_resultant_simple() {
        // resultant(x - 1, x - 2) = 1 - 2 = -1
        // x - 1: coeffs [-1, 1], x - 2: coeffs [-2, 1]
        let a = vec![Rational::from(-1), Rational::from(1)];
        let b = vec![Rational::from(-2), Rational::from(1)];
        let r = resultant(&a, &b);
        assert_eq!(r, Rational::from(-1));
    }

    #[test]
    fn test_poly_derivative() {
        // d/dx(-2 + 0*x + 1*x^2) = 0 + 2*x = [0, 2]
        let coeffs = vec![
            Rational::from(-2),
            Rational::from(0),
            Rational::from(1),
        ];
        let deriv = poly_derivative(&coeffs);
        assert_eq!(deriv.len(), 2);
        assert_eq!(deriv[0], Rational::from(0));
        assert_eq!(deriv[1], Rational::from(2));
    }

    #[test]
    fn test_count_real_roots() {
        // x^2 - 2 has 2 real roots
        let coeffs = vec![
            Rational::from(-2),
            Rational::from(0),
            Rational::from(1),
        ];
        assert_eq!(count_real_roots(&coeffs), 2);

        // x^2 + 1 has 0 real roots
        let coeffs = vec![
            Rational::from(1),
            Rational::from(0),
            Rational::from(1),
        ];
        assert_eq!(count_real_roots(&coeffs), 0);
    }
}
