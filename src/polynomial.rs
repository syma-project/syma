#![allow(clippy::needless_range_loop)]

/// Polynomial arithmetic helpers for algebraic number support.
///
/// Provides root finding, resultants, GCD, Sturm sequences, and
/// minimal-polynomial composition needed by `Value::Root`.
use rug::{Integer, Rational};
use std::cmp::Ordering;

/// Complex number (f64 precision) for internal root-finding.
#[derive(Clone, Copy, Debug)]
struct Cmplx {
    re: f64,
    im: f64,
}

impl Cmplx {
    fn add(self, other: Cmplx) -> Cmplx {
        Cmplx {
            re: self.re + other.re,
            im: self.im + other.im,
        }
    }
    fn sub(self, other: Cmplx) -> Cmplx {
        Cmplx {
            re: self.re - other.re,
            im: self.im - other.im,
        }
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
            return Cmplx {
                re: f64::INFINITY,
                im: f64::INFINITY,
            };
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
    let mut result = Cmplx {
        re: coeffs[n - 1],
        im: 0.0,
    };
    for i in (0..n - 1).rev() {
        result = result.mul(x);
        result = result.add(Cmplx {
            re: coeffs[i],
            im: 0.0,
        });
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
    for c in &coeffs[..n - 1] {
        if *c < 0.0 {
            sum_neg += c.abs();
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

    // Convert to f64 and make monic. The Durand-Kerner iteration requires a
    // monic polynomial, so divide by the leading coefficient even when it is
    // -1 (otherwise every update overshoots and the iteration diverges).
    let mut cf: Vec<f64> = coeffs.iter().map(|c| c.to_f64()).collect();
    let lead = cf[deg];
    if lead == 0.0 {
        // Degenerate: highest "nonzero" coefficient underflowed to 0.
        return vec![];
    }
    if lead != 1.0 {
        for c in cf.iter_mut() {
            *c /= lead;
        }
    }
    // Guard against underflow turning the true leading term into zero.
    cf[deg] = 1.0;

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
    let mut final_roots: Vec<(f64, f64)> = roots.into_iter().map(|z| (z.re, z.im)).collect();

    let df_coeff = poly_derivative_f64(&cf);
    for root in &mut final_roots {
        let mut z = Cmplx {
            re: root.0,
            im: root.1,
        };
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
    final_roots.sort_by(
        |a, b| match a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal) {
            Ordering::Equal => a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal),
            ord => ord,
        },
    );

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
    coeffs.iter().map(|c| c.clone() / lead.clone()).collect()
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
            let mut c = coeffs[i].clone();
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
    // Plain Euclidean algorithm: gcd(a, b) = gcd(b, a mod b). Terminate only
    // when the remainder is the zero polynomial; a nonzero constant remainder
    // is a unit and must still be divided by, otherwise the algorithm would
    // report the previous dividend instead of a constant.
    let mut a = strip_zeros(a);
    let mut b = strip_zeros(b);
    while !is_zero_poly(&b) {
        let rem = poly_remainder(&a, &b);
        a = b;
        b = rem;
    }
    if is_zero_poly(&a) {
        return vec![Rational::from(0)];
    }
    if poly_degree(&a) == 0 {
        // A nonzero constant gcd is a unit, normalized to 1.
        return vec![Rational::from(1)];
    }
    make_monic(&a)
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

/// Square-free part of a polynomial: `f / gcd(f, f')`, made monic.
/// This keeps every distinct root exactly once. The zero polynomial maps to
/// itself; constants map to themselves.
pub fn square_free_part(coeffs: &[Rational]) -> Vec<Rational> {
    let p = strip_zeros(coeffs);
    if is_zero_poly(&p) || poly_degree(&p) <= 1 {
        return p;
    }
    let deriv = poly_derivative(&p);
    let g = poly_gcd(&p, &deriv);
    if poly_degree(&g) == 0 {
        return make_monic(&p);
    }
    let (q, _) = poly_divmod(&p, &g);
    make_monic(&q)
}

/// Full square-free decomposition (Yun's algorithm).
///
/// Returns a list of `(factor, multiplicity)` where the factors are monic,
/// square-free, pairwise coprime, and their product (with multiplicities) is
/// the original polynomial up to a constant.
pub fn square_free_decomposition(coeffs: &[Rational]) -> Vec<(Vec<Rational>, usize)> {
    let p = strip_zeros(coeffs);
    if is_zero_poly(&p) {
        return vec![(p, 1)];
    }
    let mut f = make_monic(&p);
    let n = poly_degree(&f);
    if n <= 1 {
        return vec![(f, 1)];
    }

    // Musser's algorithm: repeatedly peel off the square-free radical. At
    // step i, `f` is `prod_{j>=i} p_j^{j-i+1}`; its radical is
    // `sf = prod_{j>=i} p_j` and the factors of multiplicity exactly `i` are
    // recovered as `sf / rad(f / sf)`.
    let mut result = Vec::new();
    let mut i = 1usize;
    while poly_degree(&f) > 0 {
        let sf = square_free_part(&f);
        if poly_degree(&sf) == 0 {
            break;
        }
        let next = poly_div_exact(&f, &sf);
        let sf_next = if poly_degree(&next) > 0 {
            square_free_part(&next)
        } else {
            vec![Rational::from(1)]
        };
        let (p_i, _) = poly_divmod(&sf, &sf_next);
        if poly_degree(&p_i) > 0 {
            result.push((make_monic(&p_i), i));
        }
        f = next;
        i += 1;
    }
    result
}

/// Discriminant of a polynomial: `(-1)^{n(n-1)/2} / lc * Res(f, f')`.
///
/// A polynomial has a repeated root (over the algebraic closure) iff its
/// discriminant is zero. By convention the discriminant of a constant is 1
/// and of a linear polynomial is 1.
pub fn discriminant(coeffs: &[Rational]) -> Rational {
    let p = strip_zeros(coeffs);
    let n = poly_degree(&p);
    if n == 0 {
        return Rational::from(1);
    }
    if is_zero_poly(&p) {
        return Rational::from(0);
    }
    if n == 1 {
        return Rational::from(1);
    }
    let deriv = poly_derivative(&p);
    let mut res = resultant(&p, &deriv);
    // res has degree n*(n-1) in the roots; divide by the leading coeff.
    res /= p[n].clone();
    if (n * (n - 1) / 2) % 2 == 1 {
        -res
    } else {
        res
    }
}

/// Distinct rational roots of a polynomial, sorted ascending.
///
/// Uses the rational-root theorem: any rational root `p/q` (in lowest terms)
/// has `p | a_0` and `q | a_n`.
pub fn rational_roots(coeffs: &[Rational]) -> Vec<Rational> {
    let mut p = strip_zeros(coeffs);
    if is_zero_poly(&p) {
        return vec![];
    }
    let look_deg = poly_degree(&p);
    if look_deg == 0 {
        return vec![];
    }

    let mut roots: Vec<Rational> = Vec::new();

    // Factor out any root at zero.
    let mut zero_mult = 0usize;
    while !p.is_empty() && p[0].is_zero() && poly_degree(&p) > 0 {
        p.remove(0);
        zero_mult += 1;
    }
    if zero_mult > 0 {
        roots.push(Rational::from(0));
    }
    if poly_degree(&p) == 0 {
        roots.sort();
        return roots;
    }

    // Scale to primitive integer coefficients so the theorem applies.
    let int_poly = to_primitive_integer_poly(&p);
    let deg = poly_degree_int(&int_poly);
    if deg == 0 {
        roots.sort();
        return roots;
    }
    let a0 = int_poly[0].clone();
    let an = int_poly[deg].clone();
    if a0.is_zero() {
        // Shouldn't happen after stripping, but be safe.
        roots.sort();
        return roots;
    }

    let num_div = integer_divisors(&a0);
    let den_div = integer_divisors(&an);

    let mut candidates: Vec<Rational> = Vec::new();
    for n in &num_div {
        for d in &den_div {
            candidates.push(Rational::from(n.clone()) / Rational::from(d.clone()));
        }
    }
    for cand in candidates {
        let neg = -cand.clone();
        if rational_poly_eval(&int_poly, &cand).is_zero() && !roots.contains(&cand) {
            roots.push(cand);
        }
        if rational_poly_eval(&int_poly, &neg).is_zero() && !roots.contains(&neg) {
            roots.push(neg);
        }
    }
    roots.sort();
    roots
}

/// Evaluate an integer-coefficient polynomial at a rational point exactly.
fn rational_poly_eval(coeffs: &[Integer], x: &Rational) -> Rational {
    if coeffs.is_empty() {
        return Rational::from(0);
    }
    let mut result = Rational::from(coeffs[coeffs.len() - 1].clone());
    for c in coeffs[..coeffs.len() - 1].iter().rev() {
        result = result * x + Rational::from(c.clone());
    }
    result
}

/// Convert a rational polynomial to a primitive integer polynomial with the
/// same roots (content and leading sign normalized to be positive).
fn to_primitive_integer_poly(p: &[Rational]) -> Vec<Integer> {
    let deg = poly_degree(p);
    let mut den = Integer::from(1);
    for c in p.iter().take(deg + 1) {
        if !c.is_zero() {
            den = lcm_int(den, c.denom().clone());
        }
    }
    let mut ints: Vec<Integer> = p
        .iter()
        .take(deg + 1)
        .map(|c| (c * Rational::from(den.clone())).numer().clone())
        .collect();
    // Normalize sign so the leading coefficient is positive.
    if let Some(lead) = ints.get(deg)
        && lead.is_negative()
    {
        for c in ints.iter_mut() {
            *c = -c.clone();
        }
    }
    strip_zeros_int(&ints)
}

fn strip_zeros_int(coeffs: &[Integer]) -> Vec<Integer> {
    let mut d = coeffs.len();
    while d > 0 && coeffs[d - 1].is_zero() {
        d -= 1;
    }
    if d == 0 {
        vec![Integer::from(0)]
    } else {
        coeffs[..d].to_vec()
    }
}

/// Degree of an integer polynomial (index of highest nonzero coefficient).
fn poly_degree_int(coeffs: &[Integer]) -> usize {
    let mut d = coeffs.len();
    while d > 0 {
        if !coeffs[d - 1].is_zero() {
            return d - 1;
        }
        d -= 1;
    }
    0
}

/// Positive divisors of a nonzero integer, in increasing order.
fn integer_divisors(n: &Integer) -> Vec<Integer> {
    let n = n.clone().abs();
    if n.is_zero() {
        return vec![Integer::from(1)];
    }
    let mut small: Vec<Integer> = Vec::new();
    let mut large: Vec<Integer> = Vec::new();
    let mut d = Integer::from(1);
    let one = Integer::from(1);
    loop {
        let d_sq = d.clone() * d.clone();
        if d_sq > n {
            break;
        }
        if n.is_divisible(&d) {
            small.push(d.clone());
            let q = n.clone() / &d;
            if q != d {
                large.push(q);
            }
        }
        d += &one;
    }
    large.reverse();
    small.extend(large);
    small
}

/// For a quadratic `a x^2 + b x + c`, return `(r, s, d)` such that the two
/// roots are exactly `r ± s*sqrt(d)`. `d` may be negative (imaginary roots).
/// Returns `None` when the degree is not 2.
pub fn quadratic_roots_radical(coeffs: &[Rational]) -> Option<(Rational, Rational, Rational)> {
    if poly_degree(coeffs) != 2 {
        return None;
    }
    let a = coeffs[2].clone();
    let b = coeffs[1].clone();
    let c = coeffs[0].clone();
    let four: Rational = Rational::from(4_i64);
    let disc: Rational = b.clone() * b.clone() - four * a.clone() * c;
    let two_a: Rational = Rational::from(2_i64) * a;
    let center: Rational = -b / two_a.clone();
    let scale: Rational = Rational::from(1_i64) / two_a;
    Some((center, scale, disc))
}

/// Subresultant polynomial remainder sequence of `a` and `b`.
///
/// Over the rationals this is the exact Euclidean remainder sequence starting
/// with `[a, b]` and ending at the last nonzero remainder; the last element is
/// the (monic) GCD of `a` and `b`. It is the basis for exact resultant and GCD
/// computations.
pub fn subresultants(a: &[Rational], b: &[Rational]) -> Vec<Vec<Rational>> {
    let mut prev = strip_zeros(a);
    let mut cur = strip_zeros(b);
    if is_zero_poly(&cur) {
        return if is_zero_poly(&prev) {
            vec![]
        } else {
            vec![prev]
        };
    }
    let mut seq = vec![prev.clone(), cur.clone()];
    loop {
        let rem = poly_remainder(&prev, &cur);
        if is_zero_poly(&rem) {
            break;
        }
        seq.push(rem.clone());
        prev = cur;
        cur = rem;
    }
    seq
}

/// An exact description of a real or complex algebraic root.
#[derive(Clone, Debug, PartialEq)]
pub enum ExactRoot {
    /// A rational root.
    Rational(Rational),
    /// One of the two roots `center ± scale * sqrt(disc)`. `disc` may be
    /// negative, representing an imaginary radical.
    Quadratic {
        center: Rational,
        scale: Rational,
        disc: Rational,
    },
}

impl ExactRoot {
    /// Numerically evaluate this root as a complex `(re, im)` pair.
    pub fn to_f64(&self) -> (f64, f64) {
        match self {
            ExactRoot::Rational(r) => (r.to_f64(), 0.0),
            ExactRoot::Quadratic {
                center,
                scale,
                disc,
            } => {
                let (cre, cim) = (center.to_f64(), 0.0);
                let (sre, sim) = (scale.to_f64(), 0.0);
                let d = disc.to_f64();
                if d >= 0.0 {
                    let sq = d.sqrt();
                    (cre + sre * sq, cim + sim * sq)
                } else {
                    let sq = (-d).sqrt();
                    (cre, cim + sre * sq)
                }
            }
        }
    }
}

/// Exact rational square root, if one exists.
fn rational_sqrt(r: &Rational) -> Option<Rational> {
    if r.is_negative() {
        return None;
    }
    let (num, den) = r.clone().into_numer_denom();
    let sn = integer_sqrt_exact(&num)?;
    let sd = integer_sqrt_exact(&den)?;
    Some(Rational::from(sn) / Rational::from(sd))
}

/// Exact integer square root, if `n` is a perfect square.
fn integer_sqrt_exact(n: &Integer) -> Option<Integer> {
    if n.is_negative() {
        return None;
    }
    let root = n.clone().sqrt();
    let square = root.clone() * root.clone();
    if square == *n { Some(root) } else { None }
}

/// Extract all roots exactly when they are rational or quadratic radicals.
///
/// Returns `None` when the degree exceeds 3, or when a cubic has no rational
/// root (full casus-irreducibilis Cardano is left to the caller). Otherwise
/// every root is returned, in ascending order of real part where meaningful.
pub fn exact_roots(coeffs: &[Rational]) -> Option<Vec<ExactRoot>> {
    let p = strip_zeros(coeffs);
    let deg = poly_degree(&p);
    if is_zero_poly(&p) {
        return Some(vec![]);
    }
    match deg {
        0 => Some(vec![]),
        1 => Some(vec![ExactRoot::Rational(-p[0].clone() / p[1].clone())]),
        2 => quadratic_exact_roots(&p),
        3 => {
            let rr = rational_roots(&p);
            let root = rr.into_iter().next()?;
            let factor = vec![-root.clone(), Rational::from(1)];
            let (q, _) = poly_divmod(&p, &factor);
            let mut roots = quadratic_exact_roots(&q)?;
            roots.push(ExactRoot::Rational(root));
            Some(roots)
        }
        _ => None,
    }
}

/// Two exact roots of a quadratic, as rationals when the discriminant is a
/// perfect square, otherwise as quadratic radicals.
fn quadratic_exact_roots(p: &[Rational]) -> Option<Vec<ExactRoot>> {
    let (center, scale, disc) = quadratic_roots_radical(p)?;
    if let Some(sq) = rational_sqrt(&disc) {
        return Some(vec![
            ExactRoot::Rational(center.clone() + scale.clone() * sq.clone()),
            ExactRoot::Rational(center - scale * sq),
        ]);
    }
    Some(vec![
        ExactRoot::Quadratic {
            center: center.clone(),
            scale: scale.clone(),
            disc: disc.clone(),
        },
        ExactRoot::Quadratic {
            center,
            scale: -scale,
            disc,
        },
    ])
}

/// Greatest common divisor via the subresultant remainder sequence.
pub fn subresultant_gcd(a: &[Rational], b: &[Rational]) -> Vec<Rational> {
    let seq = subresultants(a, b);
    match seq.last() {
        Some(last) => {
            if is_zero_poly(last) {
                vec![Rational::from(0)]
            } else if poly_degree(last) == 0 {
                vec![Rational::from(1)]
            } else {
                make_monic(last)
            }
        }
        None => vec![Rational::from(0)],
    }
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
                rem[i + shift] -= factor.clone() * b[i].clone();
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
    if m == 0 && n == 0 {
        // Both are (nonzero) constants: the empty product convention gives 1.
        return Rational::from(1);
    }
    if m == 0 {
        // resultant(constant, b) = constant^n
        let mut r = a[0].clone();
        for _ in 1..n {
            r *= a[0].clone();
        }
        return r;
    }
    if n == 0 {
        let mut r = b[0].clone();
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
        for (j, aj) in a.iter().enumerate().take(m + 1) {
            matrix[i][i + j] = aj.clone();
        }
    }
    // Next m rows: coefficients of b shifted
    for i in 0..m {
        for (j, bj) in b.iter().enumerate().take(n + 1) {
            matrix[n + i][i + j] = bj.clone();
        }
    }

    let det = determinant_rational(&matrix);
    // Sylvester matrix convention: multiply by (-1)^(m*n)
    if (m * n) % 2 == 1 { -det } else { det }
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
        for (i, mat_row) in mat[col..].iter().enumerate() {
            if !mat_row[col].is_zero() {
                pivot_row = col + i;
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
        let piv = mat[col][col].clone();
        det *= piv.clone();
        // Eliminate below
        for row in col + 1..n {
            if mat[row][col].is_zero() {
                continue;
            }
            let factor = mat[row][col].clone() / piv.clone();
            #[allow(clippy::needless_range_loop)]
            for j in col..n {
                let col_j = mat[col][j].clone();
                mat[row][j] -= factor.clone() * col_j;
            }
        }
    }

    if sign == -1 {
        let neg: Rational = (-&det).into();
        det = neg;
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
        let neg_rem: Vec<Rational> = rem.iter().map(|c| -c.clone()).collect();
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
            if lc.is_positive() { -1 } else { 1 }
        } else {
            if lc.is_positive() { 1 } else { -1 }
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
pub fn min_poly_scale_int(p: &[Rational], _root_idx: usize, c: i64) -> Vec<Rational> {
    if c == 1 {
        return p.to_vec();
    }
    if c == -1 {
        // p(-x): alternate sign coefficients
        return p
            .iter()
            .enumerate()
            .map(|(i, coef)| {
                if i % 2 == 1 {
                    Rational::from(-&coef.clone())
                } else {
                    coef.clone()
                }
            })
            .collect();
    }
    let deg = poly_degree(p);
    let abs_c = c.abs();
    // (px)^n + c1(px)^(n-1)*|c| + ... + cn*|c|^n  where px = x*sign(c)
    let mut result = Vec::with_capacity(deg + 1);
    let sign_c = if c < 0 { -1 } else { 1 };
    for (i, pi) in p.iter().enumerate().take(deg + 1) {
        let mut coef = pi.clone();
        let power = deg - i;
        for _ in 0..power {
            coef *= abs_c;
        }
        if sign_c < 0 && i % 2 == 1 && power % 2 == 1 {
            // sign correction
        }
        if sign_c < 0 && (deg - i) % 2 == 1 {
            coef = Rational::from(-&coef.clone());
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
            #[allow(clippy::needless_range_loop)]
            for k in j..=n {
                let binom_kj = binomial(k, j);
                let z_power = k - j;
                let sign = if j % 2 == 1 { -1 } else { 1 };
                let mut coeff = p[k].clone();
                coeff *= binom_kj;
                if sign < 0 {
                    coeff = Rational::from(-&coeff.clone());
                }
                if subtract {
                    // z = alpha - beta, so alpha = z + beta
                    // p(z + y) = sum p[k] * (z+y)^k
                    // coeff of y^j = sum_{k=j..n} p[k] * binom(k,j) * z^(k-j)
                    // (all positive, no (-1)^j factor, but sign depends on subtract)
                    if !subtract {
                        // already done above for add
                    }
                    // For subtract: alpha = z + beta, p(z+y)
                    // The coefficients are all positive (no (-1)^j)
                    if j % 2 == 1 {
                        coeff = Rational::from(-&coeff.clone()); // negate back the previous -1^j
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
        #[allow(clippy::needless_range_loop)]
        for j in 0..=n {
            for k in j..=n {
                let binom_kj = binomial(k, j);
                let z_power = k - j;
                let mut coeff = p[k].clone();
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
        matrix[i][i..=i + n].clone_from_slice(p_expanded);
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
    if is_zero_poly(&det) {
        return vec![Rational::from(0)];
    }
    // The resultant may contain a root with multiplicity (e.g. multiplying
    // conjugates can produce a repeated factor); the minimal polynomial is
    // square-free, so drop repeated factors.
    remove_content(&square_free_part(&det))
}

/// Multiply two polynomials (coefficient vectors).
pub fn poly_mul(a: &[Rational], b: &[Rational]) -> Vec<Rational> {
    if a.is_empty() || b.is_empty() {
        return vec![Rational::from(0)];
    }
    if is_zero_poly(a) || is_zero_poly(b) {
        return vec![Rational::from(0)];
    }
    let da = poly_degree(a);
    let db = poly_degree(b);
    let mut out = vec![Rational::from(0); da + db + 1];
    for i in 0..=da {
        if a[i].is_zero() {
            continue;
        }
        for j in 0..=db {
            if b[j].is_zero() {
                continue;
            }
            out[i + j] += a[i].clone() * b[j].clone();
        }
    }
    strip_zeros(&out)
}

/// Add two polynomials (coefficient vectors).
pub fn poly_add(a: &[Rational], b: &[Rational]) -> Vec<Rational> {
    let n = a.len().max(b.len());
    let mut out = vec![Rational::from(0); n];
    for (i, c) in a.iter().enumerate() {
        out[i] += c.clone();
    }
    for (i, c) in b.iter().enumerate() {
        out[i] += c.clone();
    }
    strip_zeros(&out)
}

/// Subtract two polynomials (coefficient vectors).
pub fn poly_sub(a: &[Rational], b: &[Rational]) -> Vec<Rational> {
    let n = a.len().max(b.len());
    let mut out = vec![Rational::from(0); n];
    for (i, c) in a.iter().enumerate() {
        out[i] += c.clone();
    }
    for (i, c) in b.iter().enumerate() {
        out[i] -= c.clone();
    }
    strip_zeros(&out)
}

/// Polynomial division with remainder: returns (quotient, remainder) with
/// `a = quotient * b + remainder`.
pub fn poly_divmod(a: &[Rational], b: &[Rational]) -> (Vec<Rational>, Vec<Rational>) {
    let b = strip_zeros(b);
    if is_zero_poly(&b) {
        return (vec![Rational::from(0)], strip_zeros(a));
    }
    let db = poly_degree(&b);
    let lc = b[db].clone();

    let mut rem = strip_zeros(a);
    if is_zero_poly(&rem) || poly_degree(&rem) < db {
        return (vec![Rational::from(0)], rem);
    }

    let mut quo = vec![Rational::from(0); poly_degree(&rem) - db + 1];
    while !is_zero_poly(&rem) && poly_degree(&rem) >= db {
        let dr = poly_degree(&rem);
        let shift = dr - db;
        if shift >= quo.len() {
            quo.resize(shift + 1, Rational::from(0));
        }
        let factor = rem[dr].clone() / lc.clone();
        quo[shift] += factor.clone();
        for i in 0..=db {
            rem[i + shift] -= factor.clone() * b[i].clone();
        }
        rem = strip_zeros(&rem);
    }
    (strip_zeros(&quo), rem)
}

/// Exact polynomial division (assumes the remainder is zero).
fn poly_div_exact(a: &[Rational], b: &[Rational]) -> Vec<Rational> {
    poly_divmod(a, b).0
}

/// Determinant of a matrix whose entries are polynomials (Vec<Rational>).
/// Each polynomial is represented as coefficients [c0, c1, ..., ...].
///
/// Uses the fraction-free Bareiss algorithm, which stays entirely exact
/// (no floating point, no interpolation error).
fn poly_matrix_det(matrix: &[Vec<Vec<Rational>>], n: usize, _max_deg: usize) -> Vec<Rational> {
    if n == 0 {
        return vec![Rational::from(1)];
    }
    if n == 1 {
        return strip_zeros_poly(&matrix[0][0]);
    }

    let mut mat: Vec<Vec<Vec<Rational>>> = matrix.iter().map(|row| row.to_vec()).collect();
    let mut sign = 1i64;
    // `prev` is the previous pivot; Bareiss divides each 2x2 minor by it.
    let mut prev = vec![Rational::from(1)];

    for k in 0..n - 1 {
        if is_zero_poly(&mat[k][k]) {
            let mut swap = None;
            for r in k + 1..n {
                if !is_zero_poly(&mat[r][k]) {
                    swap = Some(r);
                    break;
                }
            }
            match swap {
                Some(r) => {
                    mat.swap(k, r);
                    sign = -sign;
                }
                None => return vec![Rational::from(0)],
            }
        }
        let pivot = mat[k][k].clone();
        for i in k + 1..n {
            for j in k + 1..n {
                let cross = poly_sub(
                    &poly_mul(&mat[i][j], &pivot),
                    &poly_mul(&mat[i][k], &mat[k][j]),
                );
                mat[i][j] = poly_div_exact(&cross, &prev);
            }
        }
        prev = pivot;
        for row in mat.iter_mut().take(n).skip(k + 1) {
            row[k] = vec![Rational::from(0)];
        }
    }

    let det = strip_zeros_poly(&mat[n - 1][n - 1]);
    if sign < 0 {
        det.into_iter().map(|c| -c).collect()
    } else {
        det
    }
}

/// Interpolate a polynomial from values at integer points 0..=max_deg.
/// Returns coefficient vector. The evaluations should have len = max_deg + 1.
pub fn interpolate_from_integers(evals: &[(i64, i64)], _max_deg: usize) -> Vec<Rational> {
    let n = evals.len(); // = max_deg + 1
    let mut coeffs = vec![Rational::from(0); n];

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
            // Multiply li_num_poly by (x - x_j); the helper multiplies by (x + s).
            li_num_poly = poly_multiply_by_linear(li_num_poly, -evals[j].0);
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
        if c.clone().abs().to_f64() < 1e-6 {
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
fn min_poly_mul(p: &[Rational], _root_p: usize, q: &[Rational], _root_q: usize) -> Vec<Rational> {
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
fn min_poly_div(p: &[Rational], _root_p: usize, q: &[Rational], _root_q: usize) -> Vec<Rational> {
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
    let deg = if poly.is_empty() {
        0
    } else {
        poly_degree(poly)
    };
    if deg == 0 {
        return poly.to_vec();
    }

    // Bail out if every coefficient is zero.
    let gcd_set = poly.iter().any(|c| !c.is_zero());
    if !gcd_set {
        return vec![Rational::from(0)];
    }

    // Make content an integer gcd (take rational content, then integer part)
    // For simplicity, just divide by the first nonzero coefficient's abs to make monic-ish
    let lead = poly[deg].clone();
    let mut result: Vec<Rational> = poly.iter().map(|c| c.clone() / lead.clone()).collect();

    // Ensure leading coefficient is positive
    if !result[deg].is_zero() && result[deg].is_negative() {
        result = result.into_iter().map(|c| -c).collect();
    }

    // Clear denominators if all coefficients have same denominator
    let common_den = {
        let mut den = Integer::from(1);
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

    if common_den != 1 {
        result = result.into_iter().map(|c| c * &common_den).collect();
        // Check if all are now integers
        let all_int = result.iter().all(|c| c.denom() == &Integer::from(1));
        if !all_int {
            // denominators differ; keep as-is
        }
    }

    strip_zeros_poly(&result)
}

fn lcm_int(a: Integer, b: Integer) -> Integer {
    // lcm(a, b) = |a*b| / gcd(a, b), for integers
    let prod = a.clone() * b.clone();
    let g = a.gcd(&b);
    prod / g
}

fn strip_zeros_poly(coeffs: &[Rational]) -> Vec<Rational> {
    let deg = if coeffs.is_empty() {
        0
    } else {
        poly_degree(coeffs)
    };
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
                Value::Rational(r) => coeffs.push((**r).clone()),
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
                    Value::Rational(Box::new(c.clone()))
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
        Value::Rational(Box::new(r.clone()))
    }
}

/// Remove content and normalize — public wrapper around remove_content.
pub fn remove_content_poly(poly: &[Rational]) -> Vec<Rational> {
    remove_content(poly)
}

/// Scale a Root by a Rational: if alpha is root of p, find min poly of c*alpha.
pub fn min_poly_scale(p: &[Rational], _root_idx: usize, scale: &Rational) -> Vec<Rational> {
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
    for (k, pk) in p.iter().enumerate().take(deg + 1) {
        let mut c = pk.clone() * scale.clone();
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
            poly_degree(&[Rational::from(1), Rational::from(0), Rational::from(3)]),
            2
        );
    }

    #[test]
    fn test_poly_eval_basic() {
        // x^2 - 2 = -2 + 0*x + 1*x^2, coeffs = [-2, 0, 1]
        let coeffs = vec![Rational::from(-2), Rational::from(0), Rational::from(1)];
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
        let coeffs = vec![Rational::from(-2), Rational::from(0), Rational::from(1)];
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
    fn test_resultant_swapped_linear() {
        // Anti-symmetry: resultant(p, q) = (-1)^(mn) resultant(q, p).
        // resultant(x - 2, x - 1) = 1
        let a = vec![Rational::from(-2), Rational::from(1)];
        let b = vec![Rational::from(-1), Rational::from(1)];
        assert_eq!(resultant(&a, &b), Rational::from(1));
    }

    #[test]
    fn test_resultant_constant_degree() {
        // resultant(const, q) = const^deg(q)
        assert_eq!(
            resultant(
                &[Rational::from(2)],
                &[Rational::from(-1), Rational::from(1)]
            ),
            Rational::from(2)
        );
        // resultant(p, const) = const^deg(p)
        assert_eq!(
            resultant(
                &[Rational::from(-1), Rational::from(1)],
                &[Rational::from(2)]
            ),
            Rational::from(2)
        );
    }

    #[test]
    fn test_resultant_both_constants_is_one() {
        assert_eq!(
            resultant(&[Rational::from(2)], &[Rational::from(3)]),
            Rational::from(1)
        );
    }

    #[test]
    fn test_resultant_shared_root_is_zero() {
        // x^2 - 1 and x - 1 share the root 1.
        let p = vec![Rational::from(-1), Rational::from(0), Rational::from(1)];
        let q = vec![Rational::from(-1), Rational::from(1)];
        assert_eq!(resultant(&p, &q), Rational::from(0));
    }

    #[test]
    fn test_resultant_quadratic_linear() {
        // resultant(x^2 - 2, x - 1) = q(sqrt2) * q(-sqrt2) = -1
        let p = vec![Rational::from(-2), Rational::from(0), Rational::from(1)];
        let q = vec![Rational::from(-1), Rational::from(1)];
        assert_eq!(resultant(&p, &q), Rational::from(-1));
    }

    #[test]
    fn test_resultant_quadratic_quadratic() {
        // resultant(x^2 - 1, x^2 - 4) = q(1) * q(-1) = (-3)(-3) = 9
        let p = vec![Rational::from(-1), Rational::from(0), Rational::from(1)];
        let q = vec![Rational::from(-4), Rational::from(0), Rational::from(1)];
        assert_eq!(resultant(&p, &q), Rational::from(9));
    }

    #[test]
    fn test_interpolate_from_integers_linear() {
        // Values of x + 1 at x = 0, 1 -> coeffs [1, 1].
        let evals = vec![(0i64, 1i64), (1i64, 2i64)];
        let coeffs = interpolate_from_integers(&evals, 1);
        assert_eq!(coeffs, vec![Rational::from(1), Rational::from(1)]);
    }

    #[test]
    fn test_interpolate_from_integers_quadratic() {
        // Values of x^2 at x = 0, 1, 2 -> coeffs [0, 0, 1].
        let evals = vec![(0i64, 0i64), (1i64, 1i64), (2i64, 4i64)];
        let coeffs = interpolate_from_integers(&evals, 2);
        assert_eq!(
            coeffs,
            vec![Rational::from(0), Rational::from(0), Rational::from(1)]
        );
    }

    #[test]
    fn test_poly_derivative() {
        // d/dx(-2 + 0*x + 1*x^2) = 0 + 2*x = [0, 2]
        let coeffs = vec![Rational::from(-2), Rational::from(0), Rational::from(1)];
        let deriv = poly_derivative(&coeffs);
        assert_eq!(deriv.len(), 2);
        assert_eq!(deriv[0], Rational::from(0));
        assert_eq!(deriv[1], Rational::from(2));
    }

    #[test]
    fn test_find_roots_negative_leading() {
        // -x^2 + 2 = 0 has roots +/- sqrt(2); leading coeff is -1.
        let coeffs = vec![Rational::from(2), Rational::from(0), Rational::from(-1)];
        let roots = find_polynomial_roots(&coeffs);
        assert_eq!(roots.len(), 2);
        for (_, im) in &roots {
            assert!(im.abs() < 1e-6, "spurious imaginary part {im}");
        }
        let xs: Vec<f64> = roots.iter().map(|r| r.0).collect();
        assert!(
            xs.iter()
                .any(|x| (x + std::f64::consts::SQRT_2).abs() < 1e-6),
            "missing -sqrt2: {xs:?}"
        );
        assert!(
            xs.iter()
                .any(|x| (x - std::f64::consts::SQRT_2).abs() < 1e-6),
            "missing +sqrt2: {xs:?}"
        );
    }

    #[test]
    fn test_min_poly_add_sqrt2_sqrt3() {
        // sqrt2 + sqrt3 is a root of x^4 - 10x^2 + 1.
        let p = vec![Rational::from(-2), Rational::from(0), Rational::from(1)];
        let q = vec![Rational::from(-3), Rational::from(0), Rational::from(1)];
        let mp = min_poly_operation(&p, 2, &q, 2, AlgebraicOp::Add);
        let expected = vec![
            Rational::from(1),
            Rational::from(0),
            Rational::from(-10),
            Rational::from(0),
            Rational::from(1),
        ];
        assert_eq!(mp, expected, "got {mp:?}");
    }

    #[test]
    fn test_min_poly_mul_sqrt2_sqrt3() {
        // sqrt2 * sqrt3 = sqrt6 is a root of x^2 - 6. The resultant is
        // (x^2 - 6)^2 because both sign combinations give sqrt6, so the
        // square-free reduction is essential.
        let p = vec![Rational::from(-2), Rational::from(0), Rational::from(1)];
        let q = vec![Rational::from(-3), Rational::from(0), Rational::from(1)];
        let mp = min_poly_operation(&p, 2, &q, 2, AlgebraicOp::Mul);
        let expected = vec![Rational::from(-6), Rational::from(0), Rational::from(1)];
        assert_eq!(mp, expected, "got {mp:?}");
    }

    #[test]
    fn test_min_poly_sub_sqrt2_sqrt2() {
        // sqrt2 - sqrt2 = 0: resultant of x^2-2 with itself under subtraction
        // is x^2 (x^2 - 8), whose square-free part is x (x^2 - 8).
        let p = vec![Rational::from(-2), Rational::from(0), Rational::from(1)];
        let mp = min_poly_operation(&p, 2, &p, 2, AlgebraicOp::Sub);
        // 0 must be a root.
        assert!(poly_eval(&mp, &Rational::from(0)).is_zero(), "got {mp:?}");
    }

    #[test]
    fn test_poly_divmod() {
        // (x^2 - 1) / (x - 1) = x + 1
        let a = vec![Rational::from(-1), Rational::from(0), Rational::from(1)];
        let b = vec![Rational::from(-1), Rational::from(1)];
        let (q, r) = poly_divmod(&a, &b);
        assert_eq!(q, vec![Rational::from(1), Rational::from(1)]);
        assert!(is_zero_poly(&r));
        // (x^2) / (x - 1) = x + 1 remainder 1
        let a2 = vec![Rational::from(0), Rational::from(0), Rational::from(1)];
        let (q2, r2) = poly_divmod(&a2, &b);
        assert_eq!(q2, vec![Rational::from(1), Rational::from(1)]);
        assert_eq!(r2, vec![Rational::from(1)]);
    }

    #[test]
    fn test_poly_mul() {
        let a = vec![Rational::from(-1), Rational::from(1)]; // x - 1
        let b = vec![Rational::from(2), Rational::from(1)]; // x + 2
        assert_eq!(
            poly_mul(&a, &b),
            vec![Rational::from(-2), Rational::from(1), Rational::from(1)]
        );
    }

    #[test]
    fn test_poly_gcd() {
        // gcd(x^2 - 1, x^2 - 2x + 1) = x - 1
        let a = vec![Rational::from(-1), Rational::from(0), Rational::from(1)];
        let b = vec![Rational::from(1), Rational::from(-2), Rational::from(1)];
        let g = poly_gcd(&a, &b);
        assert_eq!(g, vec![Rational::from(-1), Rational::from(1)]);
        // gcd of coprime polynomials is a nonzero constant (monic -> 1)
        let c = vec![Rational::from(1), Rational::from(0), Rational::from(1)];
        assert_eq!(poly_gcd(&a, &c), vec![Rational::from(1)]);
    }

    #[test]
    fn test_discriminant() {
        // disc(x^2 - 2) = 8; disc(x^2 + 1) = -4
        assert_eq!(
            discriminant(&[Rational::from(-2), Rational::from(0), Rational::from(1)]),
            Rational::from(8)
        );
        assert_eq!(
            discriminant(&[Rational::from(1), Rational::from(0), Rational::from(1)]),
            Rational::from(-4)
        );
        // General quadratic: disc(a x^2 + b x + c) = b^2 - 4ac.
        assert_eq!(
            discriminant(&[Rational::from(3), Rational::from(5), Rational::from(7)]),
            Rational::from(25) - Rational::from(84)
        );
        // Repeated root -> zero discriminant.
        assert_eq!(
            discriminant(&[Rational::from(1), Rational::from(-2), Rational::from(1)]),
            Rational::from(0)
        );
        // Cubic with a repeated root: x^3 - 3x + 2 = (x-1)^2 (x+2).
        assert_eq!(
            discriminant(&[
                Rational::from(2),
                Rational::from(-3),
                Rational::from(0),
                Rational::from(1)
            ]),
            Rational::from(0)
        );
        // disc(x^3 - 2) = -108.
        assert_eq!(
            discriminant(&[
                Rational::from(-2),
                Rational::from(0),
                Rational::from(0),
                Rational::from(1)
            ]),
            Rational::from(-108)
        );
        // disc(x^4 - 1) = disc((x-1)(x+1)(x^2+1)) = -256.
        assert_eq!(
            discriminant(&[
                Rational::from(-1),
                Rational::from(0),
                Rational::from(0),
                Rational::from(0),
                Rational::from(1)
            ]),
            Rational::from(-256)
        );
    }

    #[test]
    fn test_rational_roots() {
        // 2x^2 - 3x + 1 = (2x - 1)(x - 1) -> roots 1/2 and 1
        let p = vec![Rational::from(1), Rational::from(-3), Rational::from(2)];
        assert_eq!(
            rational_roots(&p),
            vec![Rational::from(1) / Rational::from(2), Rational::from(1)]
        );
        // x^3 - 6x^2 + 11x - 6 -> 1, 2, 3
        let cubic = vec![
            Rational::from(-6),
            Rational::from(11),
            Rational::from(-6),
            Rational::from(1),
        ];
        assert_eq!(
            rational_roots(&cubic),
            vec![Rational::from(1), Rational::from(2), Rational::from(3)]
        );
        // x^2 + 1 has no rational roots.
        assert!(
            rational_roots(&[Rational::from(1), Rational::from(0), Rational::from(1)]).is_empty()
        );
        // Root at zero.
        let with_zero = vec![Rational::from(0), Rational::from(-1), Rational::from(1)];
        assert_eq!(
            rational_roots(&with_zero),
            vec![Rational::from(0), Rational::from(1)]
        );
        // Non-integer leading coefficient: 3x - 2 -> 2/3
        assert_eq!(
            rational_roots(&[Rational::from(-2), Rational::from(3)]),
            vec![Rational::from(2) / Rational::from(3)]
        );
    }

    #[test]
    fn test_min_poly_mul_cbrt2() {
        // cbrt(2) * cbrt(2) = cbrt(4) is a root of x^3 - 4. The raw resultant
        // is (x^3 - 4)^3 (each of the 3 conjugate products repeats 3 times).
        let p = vec![
            Rational::from(-2),
            Rational::from(0),
            Rational::from(0),
            Rational::from(1),
        ];
        let mp = min_poly_operation(&p, 1, &p, 1, AlgebraicOp::Mul);
        assert_eq!(
            mp,
            vec![
                Rational::from(-4),
                Rational::from(0),
                Rational::from(0),
                Rational::from(1)
            ]
        );
    }

    #[test]
    fn test_min_poly_div_sqrt2() {
        // sqrt2 / sqrt2 = 1, so 1 is a root of the composed polynomial.
        let p = vec![Rational::from(-2), Rational::from(0), Rational::from(1)];
        let mp = min_poly_operation(&p, 2, &p, 2, AlgebraicOp::Div);
        assert!(poly_eval(&mp, &Rational::from(1)).is_zero(), "got {mp:?}");
    }

    #[test]
    fn test_min_poly_scale_int() {
        // 2 * sqrt2 is a root of x^2 - 8.
        let p = vec![Rational::from(-2), Rational::from(0), Rational::from(1)];
        let q = min_poly_scale_int(&p, 2, 2);
        assert_eq!(
            q,
            vec![Rational::from(-8), Rational::from(0), Rational::from(1)]
        );
        // -3 * sqrt2 is a root of x^2 - 18.
        let q2 = min_poly_scale_int(&p, 2, -3);
        assert_eq!(
            q2,
            vec![Rational::from(-18), Rational::from(0), Rational::from(1)]
        );
    }

    #[test]
    fn test_is_square_free() {
        assert!(is_square_free(&[
            Rational::from(-2),
            Rational::from(0),
            Rational::from(1)
        ]));
        // (x - 1)^2 is not square-free.
        assert!(!is_square_free(&[
            Rational::from(1),
            Rational::from(-2),
            Rational::from(1)
        ]));
        // x^3 - 3x + 2 = (x-1)^2 (x+2) is not square-free.
        assert!(!is_square_free(&[
            Rational::from(2),
            Rational::from(-3),
            Rational::from(0),
            Rational::from(1)
        ]));
    }

    #[test]
    fn test_square_free_part() {
        // (x - 1)^3 (x + 2)^2 -> (x - 1)(x + 2) = x^2 + x - 2
        let f = poly_mul(
            &poly_mul(
                &[Rational::from(-1), Rational::from(1)],
                &[Rational::from(-1), Rational::from(1)],
            ),
            &[Rational::from(-1), Rational::from(1)],
        );
        let g = poly_mul(
            &[Rational::from(2), Rational::from(1)],
            &[Rational::from(2), Rational::from(1)],
        );
        let full = poly_mul(&f, &g);
        let sf = square_free_part(&full);
        assert_eq!(
            sf,
            vec![Rational::from(-2), Rational::from(1), Rational::from(1)]
        );
        assert!(is_square_free(&sf));
    }

    #[test]
    fn test_square_free_decomposition() {
        // (x - 1)^3 (x + 2)^2
        let f = vec![Rational::from(-1), Rational::from(1)]; // x - 1
        let g = vec![Rational::from(2), Rational::from(1)]; // x + 2
        let p = poly_mul(&poly_mul(&poly_mul(&f, &f), &f), &poly_mul(&g, &g));
        let mut decomp = square_free_decomposition(&p);
        decomp.sort_by_key(|(_, m)| *m);
        assert_eq!(decomp.len(), 2);
        assert_eq!(decomp[0], (g, 2));
        assert_eq!(decomp[1], (f, 3));
        // Product of factors^multiplicity reconstructs the polynomial.
        let mut recon = vec![Rational::from(1)];
        for (factor, mult) in &decomp {
            for _ in 0..*mult {
                recon = poly_mul(&recon, factor);
            }
        }
        assert_eq!(make_monic(&recon), make_monic(&p));
    }

    #[test]
    fn test_subresultant_gcd() {
        let a = vec![Rational::from(-1), Rational::from(0), Rational::from(1)];
        let b = vec![Rational::from(1), Rational::from(-2), Rational::from(1)];
        assert_eq!(
            subresultant_gcd(&a, &b),
            vec![Rational::from(-1), Rational::from(1)]
        );
        assert!(!subresultants(&a, &b).is_empty());
    }

    #[test]
    fn test_quadratic_roots_radical() {
        // x^2 - 2 -> center 0, scale 1/2, disc 8
        let (r, s, d) =
            quadratic_roots_radical(&[Rational::from(-2), Rational::from(0), Rational::from(1)])
                .unwrap();
        assert_eq!(r, Rational::from(0));
        assert_eq!(s, Rational::from(1) / Rational::from(2));
        assert_eq!(d, Rational::from(8));
        // Verify the roots really satisfy the polynomial: represent
        // r + s*sqrt(d) by squaring (r - x)^2 = s^2 d.
        // For x^2 - 2 with center 0, scale 1/2, disc 8 this gives
        // x^2 = s^2 d = 2. Good.
    }

    #[test]
    fn test_exact_roots() {
        // 2x^2 - 3x + 1 = (2x-1)(x-1): rational roots 1 and 1/2.
        let r = exact_roots(&[Rational::from(1), Rational::from(-3), Rational::from(2)]).unwrap();
        assert_eq!(
            r,
            vec![
                ExactRoot::Rational(Rational::from(1)),
                ExactRoot::Rational(Rational::from(1) / Rational::from(2)),
            ]
        );

        // x^2 - 2: quadratic radicals with disc 8.
        let r = exact_roots(&[Rational::from(-2), Rational::from(0), Rational::from(1)]).unwrap();
        assert_eq!(r.len(), 2);
        match &r[0] {
            ExactRoot::Quadratic { center, disc, .. } => {
                assert_eq!(*center, Rational::from(0));
                assert_eq!(*disc, Rational::from(8));
            }
            other => panic!("expected quadratic radical, got {other:?}"),
        }

        // x^2 + 1: imaginary quadratic radicals (disc -4).
        let r = exact_roots(&[Rational::from(1), Rational::from(0), Rational::from(1)]).unwrap();
        match &r[0] {
            ExactRoot::Quadratic { disc, .. } => assert_eq!(*disc, Rational::from(-4)),
            other => panic!("expected quadratic radical, got {other:?}"),
        }

        // x^3 - 6x^2 + 11x - 6: all roots rational (1, 2, 3).
        let r = exact_roots(&[
            Rational::from(-6),
            Rational::from(11),
            Rational::from(-6),
            Rational::from(1),
        ])
        .unwrap();
        let mut rats: Vec<Rational> = r
            .iter()
            .filter_map(|e| match e {
                ExactRoot::Rational(x) => Some(x.clone()),
                _ => None,
            })
            .collect();
        rats.sort();
        assert_eq!(
            rats,
            vec![Rational::from(1), Rational::from(2), Rational::from(3)]
        );
    }

    #[test]
    fn test_exact_roots_to_f64() {
        // sqrt2 radicals evaluate to ±1.414..., matching the numeric solver.
        let r = exact_roots(&[Rational::from(-2), Rational::from(0), Rational::from(1)]).unwrap();
        let xs: Vec<f64> = r.iter().map(|e| e.to_f64().0).collect();
        let hi = xs.iter().cloned().fold(f64::MIN, f64::max);
        let lo = xs.iter().cloned().fold(f64::MAX, f64::min);
        assert!((hi - std::f64::consts::SQRT_2).abs() < 1e-12);
        assert!((lo + std::f64::consts::SQRT_2).abs() < 1e-12);
    }

    #[test]
    fn test_resultant_antisymmetry_and_multiplicativity() {
        let p = vec![Rational::from(-2), Rational::from(0), Rational::from(1)]; // x^2 - 2
        let q = vec![Rational::from(-3), Rational::from(0), Rational::from(1)]; // x^2 - 3
        let r = vec![Rational::from(-1), Rational::from(1)]; // x - 1
        // Anti-symmetry: res(p, q) = (-1)^{mn} res(q, p).
        let pq = resultant(&p, &q);
        let qp = resultant(&q, &p);
        assert_eq!(pq, qp); // mn = 4 even
        // Multiplicativity: res(p, q*r) = res(p, q) * res(p, r).
        let qr = poly_mul(&q, &r);
        assert_eq!(resultant(&p, &qr), resultant(&p, &q) * resultant(&p, &r));
    }

    #[test]
    fn test_count_real_roots() {
        // x^2 - 2 has 2 real roots
        let coeffs = vec![Rational::from(-2), Rational::from(0), Rational::from(1)];
        assert_eq!(count_real_roots(&coeffs), 2);

        // x^2 + 1 has 0 real roots
        let coeffs = vec![Rational::from(1), Rational::from(0), Rational::from(1)];
        assert_eq!(count_real_roots(&coeffs), 0);
    }
}
