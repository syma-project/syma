#![allow(clippy::needless_range_loop)]

use crate::env::Env;
use crate::eval::apply_function;
use crate::polynomial;
use crate::value::{EvalError, Value, rational_value};
use rug::ops::Pow;
use rug::{Integer, Rational};

// ── Root constructor ────────────────────────────────────────────────────────────────────────────

pub fn builtin_root(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Root requires exactly 2 arguments: Root[{coeffs}, index]".to_string(),
        ));
    }

    let coeffs = root_arg_coeffs(&args[0], env)?;

    let index = match &args[1] {
        Value::Integer(n) => {
            // An index that does not fit in `usize` is certainly larger than
            // the degree, so map it to `usize::MAX` to trigger the bound check.
            let v = n.to_usize().unwrap_or(usize::MAX);
            if v == 0 {
                return Err(EvalError::Error("Root index must be >= 1".to_string()));
            }
            v
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };

    let deg = polynomial::poly_degree(&coeffs);
    if deg == 0 {
        return Err(EvalError::Error(
            "Root: polynomial must have degree > 0".to_string(),
        ));
    }
    if index > deg {
        return Err(EvalError::Error(format!(
            "Root: index {} exceeds degree {}",
            index, deg
        )));
    }

    // Make monic and canonical (positive leading coefficient)
    let mut norm = polynomial::make_monic(&coeffs);
    let lead = norm[deg].clone();
    if lead.is_negative() {
        for c in norm.iter_mut() {
            *c = -(c.clone());
        }
    }

    // If linear, return exact rational
    if deg == 1 {
        let neg = -norm[0].clone();
        let denom = norm[1].clone();
        let result = neg / denom;
        let (num, denom) = result.into_numer_denom();
        if denom == 1 {
            return Ok(Value::Integer(num));
        } else {
            return Ok(rational_value(num, denom));
        }
    }

    Ok(Value::Root {
        coeffs: norm,
        index,
    })
}

/// Extract polynomial coefficients from the first argument of `Root`.
///
/// Accepts either an explicit coefficient list `{c0, c1, …, cn}` or a pure
/// function such as `#^3 - # - 1 &`, whose polynomial is recovered by sampling
/// and rational interpolation.
fn root_arg_coeffs(arg: &Value, env: &Env) -> Result<Vec<Rational>, EvalError> {
    match arg {
        Value::List(items) => {
            let mut c = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::Integer(n) => c.push(Rational::from(n.clone())),
                    Value::Rational(r) => c.push((**r).clone()),
                    Value::Real(r) => {
                        let f = r.to_f64();
                        // Best-effort: convert to string and parse as Rational
                        c.push(rug::Rational::from_f64(f).unwrap_or(rug::Rational::from(0)));
                    }
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: "Number".to_string(),
                            got: item.type_name().to_string(),
                        });
                    }
                }
            }
            Ok(c)
        }
        Value::Function(_) | Value::PureFunction { .. } | Value::BytecodeFunction(_) => {
            coeffs_from_function(arg, env)
        }
        _ => Err(EvalError::TypeError {
            expected: "List or pure function".to_string(),
            got: arg.type_name().to_string(),
        }),
    }
}

/// Recover the coefficients of a polynomial pure function by evaluating it at
/// enough integer points and interpolating exactly over the rationals.
fn coeffs_from_function(f: &Value, env: &Env) -> Result<Vec<Rational>, EvalError> {
    // Supports polynomials up to degree 16.
    const N: usize = 16;
    let mut ys: Vec<Rational> = Vec::with_capacity(N + 1);
    for i in 0..=N {
        let val = apply_function(f, &[Value::Integer(Integer::from(i as i64))], env)?;
        let r = value_as_rational(&val).ok_or_else(|| {
            EvalError::Error("Root: function did not yield rational values".to_string())
        })?;
        ys.push(r);
    }
    Ok(interpolate_at_integers(&ys))
}

fn value_as_rational(v: &Value) -> Option<Rational> {
    match v {
        Value::Integer(n) => Some(Rational::from(n.clone())),
        Value::Rational(r) => Some((**r).clone()),
        Value::Real(r) => Rational::from_f64(r.to_f64()),
        _ => None,
    }
}

/// Lagrange interpolation through the points `(i, ys[i])` for `i = 0..ys.len()`.
fn interpolate_at_integers(ys: &[Rational]) -> Vec<Rational> {
    let n = ys.len();
    let mut coeffs = vec![Rational::from(0); n];
    for i in 0..n {
        if ys[i].is_zero() {
            continue;
        }
        // numerator = ∏_{j≠i} (x − j)
        let mut num = vec![Rational::from(1)];
        let mut den = Rational::from(1);
        for j in 0..n {
            if i == j {
                continue;
            }
            num = mul_linear_rational(&num, &Rational::from(-(j as i64)));
            den *= Rational::from(i as i64 - j as i64);
        }
        let scale = ys[i].clone() / den;
        if coeffs.len() < num.len() {
            coeffs.resize(num.len(), Rational::from(0));
        }
        for (k, c) in num.iter().enumerate() {
            coeffs[k] += c.clone() * scale.clone();
        }
    }
    strip_rationals(coeffs)
}

/// Multiply a polynomial by the linear factor `(x + s)`.
fn mul_linear_rational(p: &[Rational], s: &Rational) -> Vec<Rational> {
    let mut out = vec![Rational::from(0); p.len() + 1];
    for (k, c) in p.iter().enumerate() {
        out[k] += c.clone() * s.clone();
        out[k + 1] += c.clone();
    }
    out
}

// ── MinimalPolynomial ───────────────────────────────────────────────────────────

pub fn builtin_minimal_polynomial(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "MinimalPolynomial requires at least 1 argument".to_string(),
        ));
    }
    let expr = &args[0];

    // A radical/algebraic expression reduces to its minimal polynomial.
    if let Some(alg) = to_algebraic(expr) {
        return Ok(match alg {
            Alg::Rat(r) => {
                let (num, den) = r.into_numer_denom();
                Value::List(vec![Value::Integer(-num), Value::Integer(den)])
            }
            Alg::Root { coeffs, .. } => polynomial::coeffs_to_value(&coeffs),
        });
    }

    match expr {
        Value::Root { coeffs, .. } => {
            let deg = polynomial::poly_degree(coeffs);
            if deg == 0 {
                return Ok(expr.clone());
            }
            Ok(polynomial::coeffs_to_value(coeffs))
        }
        Value::Integer(n) => Ok(Value::List(vec![
            Value::Integer(-n.clone()),
            Value::Integer(Integer::from(1)),
        ])),
        Value::Rational(r) => {
            let (num, den) = r.clone().into_numer_denom();
            Ok(Value::List(vec![Value::Integer(-num), Value::Integer(den)]))
        }
        _ => Ok(Value::Call {
            head: "MinimalPolynomial".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── Algebraic numbers ───────────────────────────────────────────────────────────

/// An exact algebraic number, always carrying a monic, square-free, minimal
/// polynomial and a 1-based index selecting the intended root (ordered by
/// real part then imaginary part).
#[derive(Clone)]
enum Alg {
    Rat(Rational),
    Root { coeffs: Vec<Rational>, index: usize },
}

impl Alg {
    fn numeric(&self) -> (f64, f64) {
        match self {
            Alg::Rat(r) => (r.to_f64(), 0.0),
            Alg::Root { coeffs, index } => {
                let roots = polynomial::find_polynomial_roots(coeffs);
                roots
                    .get(index.saturating_sub(1))
                    .copied()
                    .unwrap_or((0.0, 0.0))
            }
        }
    }

    fn poly(&self) -> Vec<Rational> {
        match self {
            Alg::Rat(r) => vec![-r.clone(), Rational::from(1)],
            Alg::Root { coeffs, .. } => coeffs.clone(),
        }
    }

    fn degree(&self) -> usize {
        match self {
            Alg::Rat(_) => 1,
            Alg::Root { coeffs, .. } => polynomial::poly_degree(coeffs),
        }
    }
}

/// Build an `Alg` from a vanishing polynomial and a numeric enclosure of the
/// intended root. Reduces the polynomial to the minimal polynomial of that root.
fn alg_from_poly_target(poly: &[Rational], target: (f64, f64)) -> Alg {
    let poly = strip_rationals(poly.to_vec());
    if polynomial::poly_degree(&poly) == 0 {
        return Alg::Rat(Rational::from(0));
    }
    let minp = minimal_poly_containing(&poly, target);
    let d = polynomial::poly_degree(&minp);
    if d == 0 {
        return Alg::Rat(Rational::from(0));
    }
    if d == 1 {
        return Alg::Rat(-minp[0].clone() / minp[1].clone());
    }
    let roots = polynomial::find_polynomial_roots(&minp);
    let idx = nearest_root_index(&roots, target) + 1;
    Alg::Root {
        coeffs: minp,
        index: idx,
    }
}

/// Reduce `poly` (which has `target` as a root) to the minimal polynomial of
/// `target` by extracting the smallest rational-coefficient factor that keeps
/// the root.
fn minimal_poly_containing(poly: &[Rational], target: (f64, f64)) -> Vec<Rational> {
    let poly = strip_rationals(poly.to_vec());
    let deg = polynomial::poly_degree(&poly);
    if deg <= 1 {
        return normalize_monic(&poly);
    }
    let sf = square_free_monic(&poly);
    let sfdeg = polynomial::poly_degree(&sf);
    if sfdeg <= 1 {
        return sf;
    }
    let roots = polynomial::find_polynomial_roots(&sf);
    let n = roots.len();
    if n == 0 {
        return sf;
    }
    // Subset enumeration is exponential; bail out for very high degrees.
    if n > 16 {
        return sf;
    }
    let j = nearest_root_index(&roots, target);
    let total: u32 = 1u32 << n;
    for size in 1..=n {
        let mut mask: u32 = 0;
        while mask < total {
            if (mask.count_ones() as usize) == size && (mask & (1u32 << j)) != 0 {
                let q_f = subset_product(&roots, mask);
                if let Some(q) = rationalize_poly(&q_f)
                    && polynomial::poly_degree(&q) < polynomial::poly_degree(&sf)
                    && poly_rem(&sf, &q).iter().all(|c| c.is_zero())
                {
                    return q;
                }
            }
            mask += 1;
        }
    }
    sf
}

/// Product of `(x − root_i)` over the selected mask, with complex f64 coeffs.
fn subset_product(roots: &[(f64, f64)], mask: u32) -> Vec<(f64, f64)> {
    let mut poly: Vec<(f64, f64)> = vec![(1.0, 0.0)];
    for (i, &r) in roots.iter().enumerate() {
        if (mask & (1u32 << i)) == 0 {
            continue;
        }
        let mut next = vec![(0.0, 0.0); poly.len() + 1];
        for (k, &c) in poly.iter().enumerate() {
            // (x − r) * poly = x*poly − r*poly
            next[k + 1] = cx_add(next[k + 1], c);
            next[k] = cx_sub(next[k], cx_mul(c, r));
        }
        poly = next;
    }
    poly
}

/// Attempt to write a complex-coefficient monic polynomial as a rational,
/// monic polynomial.
fn rationalize_poly(coeffs: &[(f64, f64)]) -> Option<Vec<Rational>> {
    let lead = *coeffs.last()?;
    if lead.1.abs() > 1e-6 || lead.0.abs() < 1e-9 {
        return None;
    }
    let lead_r = rationalize(lead.0, 100_000)?;
    if lead_r.is_zero() {
        return None;
    }
    let mut out = Vec::with_capacity(coeffs.len());
    for &(re, im) in coeffs {
        if im.abs() > 1e-6 * (1.0 + re.abs()) {
            return None;
        }
        let r = rationalize(re, 100_000)?;
        out.push(r / lead_r.clone());
    }
    let out = strip_rationals(out);
    if polynomial::poly_degree(&out) == 0 {
        None
    } else {
        Some(out)
    }
}

/// Rational reconstruction of a float via continued fractions.
fn rationalize(x: f64, max_den: i64) -> Option<Rational> {
    if !x.is_finite() {
        return None;
    }
    let neg = x < 0.0;
    let mut v = x.abs();
    let (mut p0, mut p1) = (0i128, 1i128);
    let (mut q0, mut q1) = (1i128, 0i128);
    for _ in 0..64 {
        let a = v.floor();
        if !a.is_finite() {
            break;
        }
        let ai = a as i128;
        let p2 = ai * p1 + p0;
        let q2 = ai * q1 + q0;
        if q2 > max_den as i128 {
            break;
        }
        p0 = p1;
        p1 = p2;
        q0 = q1;
        q1 = q2;
        let frac = v - a;
        if frac.abs() < 1e-15 {
            break;
        }
        v = 1.0 / frac;
    }
    if q1 == 0 {
        return None;
    }
    let mut num = p1;
    if neg {
        num = -num;
    }
    let num = i64::try_from(num).ok()?;
    let den = i64::try_from(q1).ok()?;
    Some(Rational::from(num) / Rational::from(den))
}

fn nearest_root_index(roots: &[(f64, f64)], target: (f64, f64)) -> usize {
    let mut best = 0usize;
    let mut best_d = f64::INFINITY;
    for (i, &(re, im)) in roots.iter().enumerate() {
        let d = (re - target.0).powi(2) + (im - target.1).powi(2);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

// ── Rational polynomial helpers ─────────────────────────────────────────────────

fn strip_rationals(mut c: Vec<Rational>) -> Vec<Rational> {
    while c.len() > 1 && c.last().is_some_and(|x| x.is_zero()) {
        c.pop();
    }
    if c.is_empty() {
        c.push(Rational::from(0));
    }
    c
}

fn is_zero_poly(c: &[Rational]) -> bool {
    c.iter().all(|x| x.is_zero())
}

fn normalize_monic(p: &[Rational]) -> Vec<Rational> {
    let p = strip_rationals(p.to_vec());
    if polynomial::poly_degree(&p) == 0 {
        return p;
    }
    let mut m = polynomial::make_monic(&p);
    if m.last().is_some_and(|c| c.is_negative()) {
        for c in m.iter_mut() {
            *c = -(c.clone());
        }
    }
    strip_rationals(m)
}

/// Remainder of polynomial division `a mod b`.
fn poly_rem(a: &[Rational], b: &[Rational]) -> Vec<Rational> {
    let b = strip_rationals(b.to_vec());
    let db = polynomial::poly_degree(&b);
    if db == 0 {
        return if b[0].is_zero() {
            strip_rationals(a.to_vec())
        } else {
            vec![Rational::from(0)]
        };
    }
    let lead = b[db].clone();
    let mut r = strip_rationals(a.to_vec());
    let mut guard = 0;
    while polynomial::poly_degree(&r) >= db && !is_zero_poly(&r) {
        let dr = polynomial::poly_degree(&r);
        let factor = r[dr].clone() / lead.clone();
        let shift = dr - db;
        for i in 0..=db {
            if i + shift < r.len() {
                let sub = factor.clone() * b[i].clone();
                r[i + shift] -= sub;
            }
        }
        r = strip_rationals(r);
        guard += 1;
        if guard > 100_000 {
            break;
        }
    }
    r
}

fn poly_gcd_pub(mut a: Vec<Rational>, mut b: Vec<Rational>) -> Vec<Rational> {
    a = strip_rationals(a);
    b = strip_rationals(b);
    if is_zero_poly(&b) {
        return normalize_monic(&a);
    }
    while !is_zero_poly(&b) {
        let r = poly_rem(&a, &b);
        a = b;
        b = strip_rationals(r);
    }
    normalize_monic(&a)
}

/// Square-free part of a polynomial, made monic.
fn square_free_monic(p: &[Rational]) -> Vec<Rational> {
    let p = normalize_monic(p);
    let deg = polynomial::poly_degree(&p);
    if deg <= 1 {
        return p;
    }
    let deriv = polynomial::poly_derivative(&p);
    let g = poly_gcd_pub(p.clone(), deriv);
    if polynomial::poly_degree(&g) == 0 {
        return p;
    }
    let (q, r) = poly_div(&p, &g);
    if !is_zero_poly(&r) {
        return p;
    }
    normalize_monic(&q)
}

/// Polynomial long division, returning `(quotient, remainder)`.
fn poly_div(a: &[Rational], b: &[Rational]) -> (Vec<Rational>, Vec<Rational>) {
    let b = strip_rationals(b.to_vec());
    let db = polynomial::poly_degree(&b);
    if db == 0 {
        if b[0].is_zero() {
            return (strip_rationals(a.to_vec()), vec![Rational::from(0)]);
        }
        let q: Vec<Rational> = a.iter().map(|c| c.clone() / b[0].clone()).collect();
        return (strip_rationals(q), vec![Rational::from(0)]);
    }
    let lead = b[db].clone();
    let mut q = vec![Rational::from(0); a.len().saturating_sub(db) + 1];
    let mut r = strip_rationals(a.to_vec());
    let mut guard = 0;
    while polynomial::poly_degree(&r) >= db && !is_zero_poly(&r) {
        let dr = polynomial::poly_degree(&r);
        let shift = dr - db;
        let factor = r[dr].clone() / lead.clone();
        if shift >= q.len() {
            q.resize(shift + 1, Rational::from(0));
        }
        q[shift] = factor.clone();
        for i in 0..=db {
            if i + shift < r.len() {
                let sub = factor.clone() * b[i].clone();
                r[i + shift] -= sub;
            }
        }
        r = strip_rationals(r);
        guard += 1;
        if guard > 100_000 {
            break;
        }
    }
    (strip_rationals(q), r)
}

// ── Conversion of expressions to algebraic numbers ──────────────────────────────

fn to_algebraic(v: &Value) -> Option<Alg> {
    match v {
        Value::Integer(n) => Some(Alg::Rat(Rational::from(n.clone()))),
        Value::Rational(r) => Some(Alg::Rat((**r).clone())),
        Value::Real(r) => Rational::from_f64(r.to_f64()).map(Alg::Rat),
        Value::Complex { re, im } if *im == 0.0 => Rational::from_f64(*re).map(Alg::Rat),
        Value::Complex { re, im } => {
            // Treat a + b·I with I = √−1 as an algebraic number.
            let i_alg = alg_from_poly_target(
                &[Rational::from(1), Rational::from(0), Rational::from(1)],
                (0.0, 1.0),
            );
            let re_alg = Alg::Rat(Rational::from_f64(*re)?);
            let im_alg = Alg::Rat(Rational::from_f64(*im)?);
            Some(alg_add(&re_alg, &alg_mul(&im_alg, &i_alg)))
        }
        Value::Root { coeffs, index } => {
            let monic = normalize_monic(coeffs);
            let target = root_numeric(&monic, *index)?;
            Some(alg_from_poly_target(&monic, target))
        }
        Value::Call { head, args } => match head.as_str() {
            "Plus" => {
                let mut acc = Alg::Rat(Rational::from(0));
                for a in args {
                    acc = alg_add(&acc, &to_algebraic(a)?);
                }
                Some(acc)
            }
            "Times" => {
                let mut acc = Alg::Rat(Rational::from(1));
                for a in args {
                    acc = alg_mul(&acc, &to_algebraic(a)?);
                }
                Some(acc)
            }
            "Divide" if args.len() == 2 => {
                let a = to_algebraic(&args[0])?;
                let b = to_algebraic(&args[1])?;
                Some(alg_mul(&a, &alg_recip(&b)))
            }
            "Minus" if args.len() == 1 => {
                let a = to_algebraic(&args[0])?;
                Some(alg_mul(&Alg::Rat(Rational::from(-1)), &a))
            }
            "Sqrt" if args.len() == 1 => alg_nth_root(&to_algebraic(&args[0])?, 2),
            "Power" if args.len() == 2 => {
                let base = to_algebraic(&args[0])?;
                match &args[1] {
                    Value::Integer(n) => Some(alg_pow(&base, n.to_i64()?)),
                    Value::Rational(r) => {
                        let p = r.numer().to_i64()?;
                        let q = r.denom().to_i64()?;
                        let root = alg_nth_root(&base, q as usize)?;
                        Some(alg_pow(&root, p))
                    }
                    _ => None,
                }
            }
            _ => None,
        },
        _ => None,
    }
}

fn root_numeric(coeffs: &[Rational], index: usize) -> Option<(f64, f64)> {
    if index == 0 {
        return None;
    }
    let roots = polynomial::find_polynomial_roots(coeffs);
    roots.get(index - 1).copied()
}

fn alg_add(a: &Alg, b: &Alg) -> Alg {
    if let (Alg::Rat(x), Alg::Rat(y)) = (a, b) {
        return Alg::Rat(x.clone() + y.clone());
    }
    // Resultant-based vanishing polynomial, then reduce to the minimal one.
    // (Computed locally because the shared helper applies a square-free step
    // that can over-reduce when `poly_gcd` returns a spurious factor.)
    let vanishing = vanishing_poly_add(&a.poly(), &b.poly());
    alg_from_poly_target(&vanishing, cx_add(a.numeric(), b.numeric()))
}

fn alg_mul(a: &Alg, b: &Alg) -> Alg {
    if let (Alg::Rat(x), Alg::Rat(y)) = (a, b) {
        return Alg::Rat(x.clone() * y.clone());
    }
    let target = cx_mul(a.numeric(), b.numeric());
    // Scaling by a rational is cheaper and more stable than a full resultant.
    if let Alg::Rat(r) = a {
        let scaled = polynomial::min_poly_scale(&b.poly(), 1, r);
        return alg_from_poly_target(&scaled, target);
    }
    if let Alg::Rat(r) = b {
        let scaled = polynomial::min_poly_scale(&a.poly(), 1, r);
        return alg_from_poly_target(&scaled, target);
    }
    let vanishing = vanishing_poly_mul(&a.poly(), &b.poly());
    alg_from_poly_target(&vanishing, target)
}

// ── Resultant in the working variable (self-contained) ──────────────────────────

/// Vanishing polynomial of `alpha + beta` where `alpha` is a root of `p` and
/// `beta` a root of `q`: `Res_y(q(y), p(z − y))`.
fn vanishing_poly_add(p: &[Rational], q: &[Rational]) -> Vec<Rational> {
    let n = polynomial::poly_degree(p);
    let m = polynomial::poly_degree(q);
    let p_sub: Vec<Vec<Rational>> = (0..=n)
        .map(|j| {
            let mut poly_z = vec![Rational::from(0); n + 1];
            for k in j..=n {
                let mut coeff = p[k].clone() * binomial_rational(k, j);
                if j % 2 == 1 {
                    coeff = -coeff;
                }
                poly_z[k - j] += coeff;
            }
            poly_z
        })
        .collect();
    normalize_monic(&sylvester_resultant_z(q, &p_sub, n, m))
}

/// Vanishing polynomial of `alpha * beta` where `alpha` is a root of `p` and
/// `beta` a root of `q`: `Res_y(q(y), p(z / y) yⁿ)`.
fn vanishing_poly_mul(p: &[Rational], q: &[Rational]) -> Vec<Rational> {
    let n = polynomial::poly_degree(p);
    let m = polynomial::poly_degree(q);
    let p_z_beta: Vec<Vec<Rational>> = (0..=n)
        .map(|j| {
            let mut poly = vec![Rational::from(0); n + 1];
            poly[n - j] = p[n - j].clone();
            poly
        })
        .collect();
    normalize_monic(&sylvester_resultant_z(q, &p_z_beta, n, m))
}

fn binomial_rational(n: usize, k: usize) -> Rational {
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

/// `Res_y(q(y), A(y))` where the coefficients of `A` are polynomials in the
/// output variable `z` (entries of `a_expanded` are indexed by `y` power).
fn sylvester_resultant_z(
    q: &[Rational],
    a_expanded: &[Vec<Rational>],
    n: usize,
    m: usize,
) -> Vec<Rational> {
    let size = n + m;
    let mut matrix: Vec<Vec<Vec<Rational>>> =
        vec![vec![vec![Rational::from(0); n + 1]; size]; size];
    // First `m` rows: coefficients of `A` in `y`, shifted.
    for i in 0..m {
        for (k, entry) in a_expanded.iter().enumerate() {
            matrix[i][i + k] = entry.clone();
        }
    }
    // Next `n` rows: coefficients of `q` in `y`, shifted.
    for i in 0..n {
        for (j, c) in q.iter().enumerate().take(m + 1) {
            if !c.is_zero() {
                let mut entry = vec![Rational::from(0); n + 1];
                entry[0] = c.clone();
                matrix[m + i][i + j] = entry;
            }
        }
    }
    poly_matrix_det_z(&matrix, size)
}

/// Fraction-free (Bareiss) determinant of a matrix of polynomials in `z`.
fn poly_matrix_det_z(matrix: &[Vec<Vec<Rational>>], n: usize) -> Vec<Rational> {
    if n == 0 {
        return vec![Rational::from(1)];
    }
    if n == 1 {
        return strip_rationals(matrix[0][0].clone());
    }
    let mut mat: Vec<Vec<Vec<Rational>>> = matrix.to_vec();
    let mut sign = 1i64;
    let mut prev = vec![Rational::from(1)];
    for k in 0..n - 1 {
        if is_zero_poly(&mat[k][k]) {
            let mut swap = None;
            for (r, row) in mat.iter().enumerate().skip(k + 1) {
                if !is_zero_poly(&row[k]) {
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
                let cross = poly_sub_rational(
                    &poly_mul_rational(&mat[i][j], &pivot),
                    &poly_mul_rational(&mat[i][k], &mat[k][j]),
                );
                mat[i][j] = poly_div_exact(&cross, &prev);
            }
        }
        prev = pivot;
        for row in mat.iter_mut().take(n).skip(k + 1) {
            row[k] = vec![Rational::from(0)];
        }
    }
    let det = strip_rationals(mat[n - 1][n - 1].clone());
    if sign < 0 {
        det.into_iter().map(|c| -c).collect()
    } else {
        det
    }
}

fn poly_mul_rational(a: &[Rational], b: &[Rational]) -> Vec<Rational> {
    if is_zero_poly(a) || is_zero_poly(b) {
        return vec![Rational::from(0)];
    }
    let da = polynomial::poly_degree(a);
    let db = polynomial::poly_degree(b);
    let mut out = vec![Rational::from(0); da + db + 1];
    for i in 0..=da {
        if a[i].is_zero() {
            continue;
        }
        for j in 0..=db {
            out[i + j] += a[i].clone() * b[j].clone();
        }
    }
    strip_rationals(out)
}

fn poly_sub_rational(a: &[Rational], b: &[Rational]) -> Vec<Rational> {
    let len = a.len().max(b.len());
    let mut out = vec![Rational::from(0); len];
    for (i, c) in a.iter().enumerate() {
        out[i] += c.clone();
    }
    for (i, c) in b.iter().enumerate() {
        out[i] -= c.clone();
    }
    strip_rationals(out)
}

fn poly_div_exact(a: &[Rational], b: &[Rational]) -> Vec<Rational> {
    poly_div(a, b).0
}

fn alg_recip(a: &Alg) -> Alg {
    match a {
        Alg::Rat(r) => {
            if r.is_zero() {
                Alg::Rat(Rational::from(0))
            } else {
                Alg::Rat(Rational::from(1) / r.clone())
            }
        }
        Alg::Root { coeffs, .. } => {
            // If α is a root of p(x), then 1/α is a root of x^n p(1/x).
            let rev: Vec<Rational> = coeffs.iter().rev().cloned().collect();
            let (re, im) = a.numeric();
            let d = re * re + im * im;
            let target = (re / d, -im / d);
            alg_from_poly_target(&rev, target)
        }
    }
}

fn alg_pow(a: &Alg, e: i64) -> Alg {
    if e == 0 {
        return Alg::Rat(Rational::from(1));
    }
    if e < 0 {
        return alg_recip(&alg_pow(a, -e));
    }
    let mut result = Alg::Rat(Rational::from(1));
    let mut base = a.clone();
    let mut n = e as u64;
    while n > 0 {
        if n & 1 == 1 {
            result = alg_mul(&result, &base);
        }
        n >>= 1;
        if n > 0 {
            base = alg_mul(&base, &base);
        }
    }
    result
}

/// The principal `q`-th root of an algebraic number.
fn alg_nth_root(a: &Alg, q: usize) -> Option<Alg> {
    if q == 0 {
        return None;
    }
    if q == 1 {
        return Some(a.clone());
    }
    let target = cx_nth_root(a.numeric(), q as f64);
    if let Alg::Rat(r) = a {
        // Exact rational root when possible.
        if !r.is_negative()
            && let Some(exact) = rational_nth_root(r, q)
        {
            return Some(Alg::Rat(exact));
        }
        let lin = vec![-r.clone(), Rational::from(1)];
        let poly = compose_power(&lin, q);
        return Some(alg_from_poly_target(&poly, target));
    }
    let poly = compose_power(&a.poly(), q);
    Some(alg_from_poly_target(&poly, target))
}

/// Substitute `y^q` for `x` in `p(x)`: the result vanishes at every `q`-th
/// root of every root of `p`.
fn compose_power(p: &[Rational], q: usize) -> Vec<Rational> {
    let deg = polynomial::poly_degree(p);
    let mut out = vec![Rational::from(0); deg * q + 1];
    for (k, c) in p.iter().enumerate().take(deg + 1) {
        out[k * q] = c.clone();
    }
    out
}

fn rational_nth_root(r: &Rational, q: usize) -> Option<Rational> {
    if q == 0 {
        return None;
    }
    let num = r.numer();
    let den = r.denom();
    let rn = integer_nth_root(num, q)?;
    let rd = integer_nth_root(den, q)?;
    if &rn.clone().pow(q as u32) == num && &rd.clone().pow(q as u32) == den {
        Some(Rational::from(rn) / Rational::from(rd))
    } else {
        None
    }
}

fn integer_nth_root(n: &Integer, q: usize) -> Option<Integer> {
    if n.is_negative() {
        return None;
    }
    if n.is_zero() {
        return Some(Integer::from(0));
    }
    let f = n.to_f64();
    if !f.is_finite() {
        return None;
    }
    let mut guess = (f.powf(1.0 / q as f64)).round() as i64;
    if guess < 0 {
        guess = 0;
    }
    // Adjust the floating guess to the exact integer root when it exists.
    for cand in [guess - 1, guess, guess + 1] {
        if cand < 0 {
            continue;
        }
        let c = Integer::from(cand);
        if c.clone().pow(q as u32) == *n {
            return Some(c);
        }
    }
    None
}

// ── RootReduce ──────────────────────────────────────────────────────────────────

pub fn builtin_root_reduce(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "RootReduce requires 1 argument".to_string(),
        ));
    }
    Ok(root_reduce_value(&args[0], env))
}

fn root_reduce_value(v: &Value, env: &Env) -> Value {
    match v {
        Value::Integer(_) | Value::Rational(_) | Value::Real(_) => v.clone(),
        Value::Call { head, args } if head == "Plus" || head == "Times" => {
            let reduced: Vec<Value> = args.iter().map(|a| root_reduce_value(a, env)).collect();
            if let Some(alg) = combine_all(&reduced, head == "Plus")
                && alg.degree() <= 3
            {
                return alg_to_result(alg, env);
            }
            Value::Call {
                head: head.clone(),
                args: reduced,
            }
        }
        _ => match to_algebraic(v) {
            Some(alg) => alg_to_result(alg, env),
            None => v.clone(),
        },
    }
}

fn combine_all(values: &[Value], add: bool) -> Option<Alg> {
    let mut acc = if add {
        Alg::Rat(Rational::from(0))
    } else {
        Alg::Rat(Rational::from(1))
    };
    for v in values {
        let a = to_algebraic(v)?;
        acc = if add {
            alg_add(&acc, &a)
        } else {
            alg_mul(&acc, &a)
        };
    }
    Some(acc)
}

fn alg_to_result(alg: Alg, env: &Env) -> Value {
    match alg {
        Alg::Rat(r) => value_from_rational(&r),
        Alg::Root { coeffs, index } => {
            let rv = Value::Root {
                coeffs: coeffs.clone(),
                index,
            };
            if polynomial::poly_degree(&coeffs) <= 3
                && let Ok(built) = to_radicals_value(&rv)
            {
                return eval_radical(built, env).unwrap_or(rv);
            }
            rv
        }
    }
}

fn value_from_rational(r: &Rational) -> Value {
    let (num, den) = r.clone().into_numer_denom();
    rational_value(num, den)
}

// ── ToRadicals ──────────────────────────────────────────────────────────────────

pub fn builtin_to_radicals(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "ToRadicals requires 1 argument".to_string(),
        ));
    }
    let built = to_radicals_value(&args[0])?;
    eval_radical(built, env)
}

/// Evaluate a freshly built radical expression so arithmetic and `Sqrt`
/// simplify (e.g. `Sqrt[8]` becomes `2 Sqrt[2]`, `Times[-1, 0]` becomes `0`).
fn eval_radical(v: Value, env: &Env) -> Result<Value, EvalError> {
    match v {
        Value::Call { head, args } => {
            let args: Result<Vec<Value>, EvalError> =
                args.into_iter().map(|a| eval_radical(a, env)).collect();
            apply_function(&Value::Symbol(head), &args?, env)
        }
        Value::List(items) => {
            let items: Result<Vec<Value>, EvalError> =
                items.into_iter().map(|i| eval_radical(i, env)).collect();
            Ok(Value::List(items?))
        }
        other => Ok(other),
    }
}

fn to_radicals_value(v: &Value) -> Result<Value, EvalError> {
    match v {
        Value::Root { coeffs, index } => {
            let monic = normalize_monic(coeffs);
            match polynomial::poly_degree(&monic) {
                1 => {
                    let r = -monic[0].clone() / monic[1].clone();
                    Ok(value_from_rational(&r))
                }
                2 => quadratic_to_radicals(&monic, *index),
                3 => cubic_to_radicals(&monic, *index),
                4 => quartic_to_radicals(&monic, *index),
                _ => Ok(v.clone()),
            }
        }
        Value::Integer(_) | Value::Rational(_) | Value::Real(_) | Value::Complex { .. } => {
            Ok(v.clone())
        }
        Value::Call { head, args: sub } => {
            let converted: Result<Vec<Value>, _> = sub.iter().map(to_radicals_value).collect();
            Ok(Value::Call {
                head: head.clone(),
                args: converted?,
            })
        }
        Value::List(items) => {
            let converted: Result<Vec<Value>, _> = items.iter().map(to_radicals_value).collect();
            Ok(Value::List(converted?))
        }
        _ => Ok(v.clone()),
    }
}

fn sqrt_expr(arg: Value) -> Value {
    Value::Call {
        head: "Sqrt".to_string(),
        args: vec![arg],
    }
}

fn times_expr(args: Vec<Value>) -> Value {
    Value::Call {
        head: "Times".to_string(),
        args,
    }
}

fn plus_expr(args: Vec<Value>) -> Value {
    Value::Call {
        head: "Plus".to_string(),
        args,
    }
}

fn neg_expr(v: Value) -> Value {
    times_expr(vec![Value::Integer(Integer::from(-1)), v])
}

fn power_expr(base: Value, p: i64, q: i64) -> Value {
    Value::Call {
        head: "Power".to_string(),
        args: vec![
            base,
            value_from_rational(&(Rational::from(p) / Rational::from(q))),
        ],
    }
}

// ax² + bx + c with coeffs = [c, b, a]
// roots: (-b ± sqrt(b²-4ac)) / (2a)
fn quadratic_to_radicals(coeffs: &[Rational], index: usize) -> Result<Value, EvalError> {
    let c = &coeffs[0];
    let b = &coeffs[1];
    let a = &coeffs[2];

    let b_sq: Rational = b.clone() * b.clone();
    let four: Rational = Rational::from(4_i64);
    let four_ac: Rational = four * a.clone() * c.clone();
    let disc: Rational = b_sq - four_ac;

    let two_a = Rational::from(2_i64) * a.clone();
    if disc.is_zero() {
        let result = -b.clone() / two_a;
        return Ok(value_from_rational(&result));
    }

    let roots = polynomial::find_polynomial_roots(coeffs);
    let target = roots
        .get(index.saturating_sub(1))
        .copied()
        .ok_or_else(|| EvalError::Error("ToRadicals: invalid root index".to_string()))?;

    // Root = -b/(2a) ± sqrt(disc)/(2a); scale the radical directly so no
    // residual `Divide` survives simplification.
    let real_part = -b.clone() / two_a.clone();
    let radical = if disc.is_negative() {
        // Complex conjugate pair: the imaginary sign selects the root.
        let sign = if target.1 >= 0.0 { 1 } else { -1 };
        let coef = Rational::from(sign) / two_a.clone();
        times_expr(vec![
            value_from_rational(&coef),
            Value::Complex { re: 0.0, im: 1.0 },
            sqrt_expr(value_from_rational(&(-disc.clone()))),
        ])
    } else {
        let base = real_part.to_f64();
        let sign = if target.0 >= base { 1 } else { -1 };
        let coef = Rational::from(sign) / two_a.clone();
        times_expr(vec![
            value_from_rational(&coef),
            sqrt_expr(value_from_rational(&disc)),
        ])
    };

    Ok(plus_expr(vec![value_from_rational(&real_part), radical]))
}

/// Real cube root used by the Cardano formula (principal branch).
fn cubic_to_radicals(coeffs: &[Rational], index: usize) -> Result<Value, EvalError> {
    if polynomial::poly_degree(coeffs) != 3 {
        return Ok(Value::Root {
            coeffs: coeffs.to_vec(),
            index,
        });
    }
    let a = coeffs[2].clone();
    let b = coeffs[1].clone();
    let c = coeffs[0].clone();

    let three = Rational::from(3);
    let two = Rational::from(2);
    let twentyseven = Rational::from(27);

    // Depressed cubic t³ + p t + q after x = t − a/3.
    let a2 = a.clone() * a.clone();
    let a3 = a2.clone() * a.clone();
    let p = b.clone() - a2.clone() / three.clone();
    let q = two.clone() * a3.clone() / twentyseven.clone() - a.clone() * b.clone() / three.clone()
        + c.clone();

    let roots = polynomial::find_polynomial_roots(coeffs);
    let target = roots
        .get(index.saturating_sub(1))
        .copied()
        .ok_or_else(|| EvalError::Error("ToRadicals: invalid root index".to_string()))?;

    // A = −q/2 + sqrt(v), B = −q/2 − sqrt(v), v = (q/2)² + (p/3)³.
    let half_q = q.clone() / two.clone();
    let v = half_q.clone() * half_q.clone()
        + (p.clone() / three.clone()) * (p.clone() / three.clone()) * (p.clone() / three.clone());
    let neg_half_q = -half_q;
    let a_expr = plus_expr(vec![
        value_from_rational(&neg_half_q),
        sqrt_expr(value_from_rational(&v)),
    ]);
    let b_expr = plus_expr(vec![
        value_from_rational(&neg_half_q),
        neg_expr(sqrt_expr(value_from_rational(&v))),
    ]);
    let cbrt_a = power_expr(a_expr, 1, 3);
    let cbrt_b = power_expr(b_expr, 1, 3);

    // Choose which of the three Cardano branches matches the requested root.
    let a_f = a.to_f64();
    let p_f = p.to_f64();
    let q_f = q.to_f64();
    let half = -q_f / 2.0;
    let v_f = (q_f / 2.0).powi(2) + (p_f / 3.0).powi(3);
    let sq = if v_f >= 0.0 {
        (v_f.sqrt(), 0.0)
    } else {
        (0.0, (-v_f).sqrt())
    };
    let cxa = (half + sq.0, sq.1);
    let cxb = (half - sq.0, -sq.1);
    let omega = (-0.5, 0.5f64 * 3f64.sqrt());
    let mut chosen = 0usize;
    let mut best_d = f64::INFINITY;
    for j in 0..3 {
        let wj = cx_powi(omega, j as i32);
        let wmj = cx_powi(omega, -(j as i32));
        let t = cx_add(
            cx_mul(wj, cx_nth_root(cxa, 3.0)),
            cx_mul(wmj, cx_nth_root(cxb, 3.0)),
        );
        let x = (t.0 - a_f / 3.0, t.1);
        let d = (x.0 - target.0).powi(2) + (x.1 - target.1).powi(2);
        if d < best_d {
            best_d = d;
            chosen = j;
        }
    }

    let half = value_from_rational(&(Rational::from(1) / Rational::from(2)));
    let omega_expr = times_expr(vec![
        half.clone(),
        plus_expr(vec![
            Value::Integer(Integer::from(-1)),
            times_expr(vec![
                Value::Complex { re: 0.0, im: 1.0 },
                sqrt_expr(Value::Integer(Integer::from(3))),
            ]),
        ]),
    ]);
    let omega2_expr = times_expr(vec![
        half,
        plus_expr(vec![
            Value::Integer(Integer::from(-1)),
            times_expr(vec![
                Value::Integer(Integer::from(-1)),
                Value::Complex { re: 0.0, im: 1.0 },
                sqrt_expr(Value::Integer(Integer::from(3))),
            ]),
        ]),
    ]);

    let (term_a, term_b) = match chosen {
        0 => (cbrt_a, cbrt_b),
        1 => (
            times_expr(vec![omega_expr.clone(), cbrt_a]),
            times_expr(vec![omega2_expr.clone(), cbrt_b]),
        ),
        _ => (
            times_expr(vec![omega2_expr, cbrt_a]),
            times_expr(vec![omega_expr, cbrt_b]),
        ),
    };

    let shift = -a.clone() / three.clone();
    Ok(plus_expr(vec![term_a, term_b, value_from_rational(&shift)]))
}

/// Ferrari's method for the quartic. Requires a rational root of the resolvent
/// cubic; otherwise falls back to a `Root` object.
fn quartic_to_radicals(coeffs: &[Rational], index: usize) -> Result<Value, EvalError> {
    let original = Value::Root {
        coeffs: coeffs.to_vec(),
        index,
    };
    if polynomial::poly_degree(coeffs) != 4 {
        return Ok(original);
    }
    let a = coeffs[3].clone();
    let b = coeffs[2].clone();
    let c = coeffs[1].clone();
    let d = coeffs[0].clone();

    let two = Rational::from(2);
    let four = Rational::from(4);
    let eight = Rational::from(8);
    let three = Rational::from(3);
    let sixteen = Rational::from(16);
    let twofivesix = Rational::from(256);

    // Depressed quartic y⁴ + p y² + q y + r via x = y − a/4.
    let a2 = a.clone() * a.clone();
    let a3 = a2.clone() * a.clone();
    let a4 = a3.clone() * a.clone();
    let p = b.clone() - three.clone() * a2.clone() / eight.clone();
    let q = c.clone() - a.clone() * b.clone() / two.clone() + a3.clone() / eight.clone();
    let r = d.clone() - a.clone() * c.clone() / four.clone()
        + a2.clone() * b.clone() / sixteen.clone()
        - three.clone() * a4.clone() / twofivesix.clone();

    // Resolvent cubic: m³ − (p/2) m² − r m + (4 p r − q²)/8 = 0.
    let resolvent = vec![
        (four.clone() * p.clone() * r.clone() - q.clone() * q.clone()) / eight.clone(),
        -r.clone(),
        -p.clone() / two.clone(),
        Rational::from(1),
    ];
    let m0 = match find_rational_root(&resolvent) {
        Some(m) => m,
        None => return Ok(original),
    };
    let s2 = two.clone() * m0.clone() - p.clone();

    let roots = polynomial::find_polynomial_roots(coeffs);
    let target = roots
        .get(index.saturating_sub(1))
        .copied()
        .ok_or_else(|| EvalError::Error("ToRadicals: invalid root index".to_string()))?;

    // Numeric resolution of which quadratic factor and sign matches the root.
    let s_val = cx_nth_root((s2.to_f64(), 0.0), 2.0);
    let s_inv = cx_recip(s_val);
    let m0_f = m0.to_f64();
    let q_f = q.to_f64();
    let c1 = (m0_f + s_inv.0 * q_f / 2.0, s_inv.1 * q_f / 2.0);
    let c2 = (m0_f - s_inv.0 * q_f / 2.0, -s_inv.1 * q_f / 2.0);
    let b1 = (-s_val.0, -s_val.1);
    let b2 = s_val;

    let mut best_quad = 0usize;
    let mut best_sign = -1i64;
    let mut best_d = f64::INFINITY;
    for (quad, (bq, cq)) in [(b1, c1), (b2, c2)].iter().enumerate() {
        let disc = (
            bq.0 * bq.0 - bq.1 * bq.1 - 4.0 * cq.0,
            2.0 * bq.0 * bq.1 - 4.0 * cq.1,
        );
        let sd = cx_nth_root(disc, 2.0);
        for sign in [-1i64, 1] {
            let root = (
                (-bq.0 + sign as f64 * sd.0) / 2.0,
                (-bq.1 + sign as f64 * sd.1) / 2.0,
            );
            let d = (root.0 - target.0).powi(2) + (root.1 - target.1).powi(2);
            if d < best_d {
                best_d = d;
                best_quad = quad;
                best_sign = sign;
            }
        }
    }

    let s_expr = sqrt_expr(value_from_rational(&s2));
    let sinv_expr = power_expr(s_expr.clone(), -1, 1);
    let half_q = q.clone() / two.clone();
    let c1_expr = plus_expr(vec![
        value_from_rational(&m0),
        times_expr(vec![value_from_rational(&half_q), sinv_expr.clone()]),
    ]);
    let c2_expr = plus_expr(vec![
        value_from_rational(&m0),
        neg_expr(times_expr(vec![value_from_rational(&half_q), sinv_expr])),
    ]);
    let (b_expr, c_expr) = if best_quad == 0 {
        (neg_expr(s_expr.clone()), c1_expr)
    } else {
        (s_expr, c2_expr)
    };

    let disc_expr = plus_expr(vec![
        times_expr(vec![b_expr.clone(), b_expr.clone()]),
        times_expr(vec![Value::Integer(Integer::from(-4)), c_expr]),
    ]);
    let root_expr = times_expr(vec![
        value_from_rational(&(Rational::from(1) / Rational::from(2))),
        plus_expr(vec![
            neg_expr(b_expr),
            times_expr(vec![
                Value::Integer(Integer::from(best_sign)),
                sqrt_expr(disc_expr),
            ]),
        ]),
    ]);
    let shift = -a / four;
    Ok(plus_expr(vec![root_expr, value_from_rational(&shift)]))
}

fn find_rational_root(p: &[Rational]) -> Option<Rational> {
    let roots = polynomial::find_polynomial_roots(p);
    for (re, im) in roots {
        if im.abs() > 1e-9 {
            continue;
        }
        if let Some(r) = rationalize(re, 100_000)
            && polynomial::poly_eval(p, &r).is_zero()
        {
            return Some(r);
        }
    }
    None
}

// ── Complex f64 helpers ─────────────────────────────────────────────────────────

fn cx_add(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 + b.0, a.1 + b.1)
}

fn cx_sub(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 - b.0, a.1 - b.1)
}

fn cx_mul(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

fn cx_recip(a: (f64, f64)) -> (f64, f64) {
    let d = a.0 * a.0 + a.1 * a.1;
    if d < 1e-300 {
        (0.0, 0.0)
    } else {
        (a.0 / d, -a.1 / d)
    }
}

fn cx_powi(a: (f64, f64), n: i32) -> (f64, f64) {
    if n == 0 {
        return (1.0, 0.0);
    }
    if n < 0 {
        return cx_powi(cx_recip(a), -n);
    }
    let mut result = (1.0, 0.0);
    let mut base = a;
    let mut m = n;
    while m > 0 {
        if m & 1 == 1 {
            result = cx_mul(result, base);
        }
        m >>= 1;
        if m > 0 {
            base = cx_mul(base, base);
        }
    }
    result
}

fn cx_nth_root(a: (f64, f64), q: f64) -> (f64, f64) {
    let r = (a.0 * a.0 + a.1 * a.1).sqrt();
    if r < 1e-300 {
        return (0.0, 0.0);
    }
    let theta = a.1.atan2(a.0);
    let rr = r.powf(1.0 / q);
    let tt = theta / q;
    (rr * tt.cos(), rr * tt.sin())
}

// ── IsolatingInterval ───────────────────────────────────────────────────────────

pub fn builtin_isolating_interval(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "IsolatingInterval requires 1 argument".to_string(),
        ));
    }
    let (coeffs, index) = match &args[0] {
        Value::Root { coeffs, index } => (coeffs.clone(), *index),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Root".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };

    let deg = polynomial::poly_degree(&coeffs);
    if deg == 0 {
        return Err(EvalError::Error(
            "IsolatingInterval: polynomial must have degree > 0".to_string(),
        ));
    }

    let roots = polynomial::find_polynomial_roots(&coeffs);
    if index > roots.len() {
        return Err(EvalError::IndexOutOfBounds {
            index: index as i64,
            length: roots.len(),
        });
    }
    let (approx_re, approx_im) = roots[index - 1];

    if approx_im.abs() < 1e-10 {
        isolating_interval_real(&coeffs, approx_re)
    } else {
        Err(EvalError::Error(
            "IsolatingInterval: complex roots not yet supported".to_string(),
        ))
    }
}

fn isolating_interval_real(coeffs: &[Rational], approx: f64) -> Result<Value, EvalError> {
    let mut lo: Rational = Rational::from(-1_000_000_i64);
    let mut hi: Rational = Rational::from(1_000_000_i64);

    // Ensure the root is in [lo, hi]
    while lo.to_f64() > approx - 1.0 {
        lo *= Rational::from(10_i64);
    }
    while hi.to_f64() < approx + 1.0 {
        hi *= Rational::from(10_i64);
    }

    // Bisection
    for _ in 0..200 {
        let lo_clone = lo.clone();
        let hi_clone = hi.clone();
        let mid: Rational = (lo_clone + hi_clone) / Rational::from(2_i64);
        let count = polynomial::count_real_roots_in(coeffs, &lo, &mid);
        if count == 1 {
            hi = mid;
        } else {
            lo = mid;
        }
        let diff: Rational = hi.clone() - lo.clone();
        if diff.to_f64() < 1e-15 {
            break;
        }
    }

    Ok(Value::List(vec![rat_value(&lo), rat_value(&hi)]))
}

fn rat_value(r: &Rational) -> Value {
    let (num, den) = r.clone().into_numer_denom();
    if den == 1 {
        Value::Integer(num)
    } else {
        rational_value(num, den)
    }
}

// ── Registration ────────────────────────────────────────────────────────────────

pub fn register(env: &Env) {
    use crate::builtins::{register_builtin, register_builtin_env};

    register_builtin_env(env, "Root", builtin_root);
    register_builtin_env(env, "MinimalPolynomial", builtin_minimal_polynomial);
    register_builtin_env(env, "RootReduce", builtin_root_reduce);
    register_builtin_env(env, "ToRadicals", builtin_to_radicals);
    register_builtin(env, "IsolatingInterval", builtin_isolating_interval);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use crate::eval::eval_program;
    use crate::lexer;
    use crate::parser;

    fn eval_str(input: &str) -> Value {
        let env = Env::new();
        builtins::register_builtins(&env);
        let tokens = lexer::tokenize(input).unwrap();
        let ast = parser::parse(tokens).unwrap();
        eval_program(&ast, &env).unwrap()
    }

    fn sqrt2_root() -> Value {
        Value::Root {
            coeffs: vec![Rational::from(-2), Rational::from(0), Rational::from(1)],
            index: 2,
        }
    }

    fn sqrt2_root_index(index: usize) -> Value {
        Value::Root {
            coeffs: vec![Rational::from(-2), Rational::from(0), Rational::from(1)],
            index,
        }
    }

    #[test]
    fn test_root_reduce_times_keeps_all_scalars() {
        // RootReduce[x*y*Root[...]] must keep both symbolic factors.
        let env = Env::new();
        builtins::register_builtins(&env);
        let expr = Value::Call {
            head: "Times".to_string(),
            args: vec![
                Value::Symbol("x".to_string()),
                Value::Symbol("y".to_string()),
                sqrt2_root(),
            ],
        };
        let result = root_reduce_value(&expr, &env);
        let rendered = result.to_string();
        assert!(rendered.contains('x'), "lost factor x: {rendered}");
        assert!(rendered.contains('y'), "lost factor y: {rendered}");
    }

    #[test]
    fn test_root_index_out_of_range_errors() {
        let env = Env::new();
        builtins::register_builtins(&env);
        let coeffs = Value::List(vec![
            Value::Integer(Integer::from(-2)),
            Value::Integer(Integer::from(0)),
            Value::Integer(Integer::from(1)),
        ]);
        // Fits in usize but exceeds degree 2.
        let small = Value::Integer(Integer::from(5));
        assert!(builtin_root(&[coeffs.clone(), small], &env).is_err());

        // Does not even fit in usize.
        let huge = Value::Integer(Integer::from(10).pow(30));
        assert!(builtin_root(&[coeffs, huge], &env).is_err());
    }

    #[test]
    fn test_root_reduce_sqrt_squared_is_two() {
        // RootReduce[Sqrt[2]^2] → 2. (Sqrt[2]^2 is Power[Sqrt[2], 2].)
        let out = eval_str("RootReduce[Sqrt[2]^2]");
        assert_eq!(out, Value::Integer(Integer::from(2)), "got {out}");
    }

    #[test]
    fn test_root_reduce_product_of_radicals() {
        let out = eval_str("RootReduce[Sqrt[2] Sqrt[3]]");
        let s = out.to_string();
        assert!(s.contains("Sqrt[6]"), "expected Sqrt[6], got {s}");
    }

    #[test]
    fn test_root_reduce_denests_nested_radical() {
        // Sqrt[3 + 2 Sqrt[2]] = 1 + Sqrt[2].
        let out = eval_str("RootReduce[Sqrt[3 + 2 Sqrt[2]]]");
        let s = out.to_string();
        assert!(s.contains("Sqrt[2]"), "expected a denested form, got {s}");
        let diff = eval_str("N[RootReduce[Sqrt[3 + 2 Sqrt[2]]] - (1 + Sqrt[2])]");
        assert!(abs_approx(&diff) < 1e-9, "not denested: {out}");
    }

    #[test]
    fn test_root_reduce_reduces_root_to_minimal() {
        // x^3 − 2x = x(x² − 2); its root index 1 is −Sqrt[2] and its minimal
        // polynomial must drop to x² − 2.
        let out = eval_str("RootReduce[Root[{0, -2, 0, 1}, 1]]");
        let diff = eval_str("N[RootReduce[Root[{0, -2, 0, 1}, 1]] + Sqrt[2]]");
        assert!(abs_approx(&diff) < 1e-9, "wrong value: {out}");
        let minp = eval_str("MinimalPolynomial[RootReduce[Root[{0, -2, 0, 1}, 1]]]");
        assert_eq!(
            minp,
            Value::List(vec![
                Value::Integer(Integer::from(-2)),
                Value::Integer(Integer::from(0)),
                Value::Integer(Integer::from(1)),
            ]),
            "not reduced to minimal polynomial: {minp}"
        );
    }

    #[test]
    fn test_root_pure_function_form() {
        // Root[#^3 − # − 1 &, 1] must build a Root object (not an error).
        let out = eval_str("Root[#^3 - # - 1 &, 1]");
        match out {
            Value::Root { coeffs, index } => {
                assert_eq!(index, 1);
                assert_eq!(coeffs.len(), 4);
                assert_eq!(coeffs[0], Rational::from(-1));
                assert_eq!(coeffs[1], Rational::from(-1));
                assert_eq!(coeffs[2], Rational::from(0));
                assert_eq!(coeffs[3], Rational::from(1));
            }
            other => panic!("expected Root, got {other:?}"),
        }
    }

    #[test]
    fn test_to_radicals_cubic_is_valid() {
        // ToRadicals[Root[#^3 − # − 1 &, k]] must return a radical agreeing
        // with the corresponding root for every index.
        for k in 1..=3 {
            let expr = format!("ToRadicals[Root[#^3 - # - 1 &, {k}]]");
            let out = eval_str(&expr);
            let s = out.to_string();
            assert!(
                !s.starts_with("Root[") && !s.starts_with("ToRadicals["),
                "expected a radical, got {s}"
            );
            assert!(s.contains("Power") || s.contains("Sqrt"), "got {s}");
            let diff = eval_str(&format!(
                "N[ToRadicals[Root[#^3 - # - 1 &, {k}]] - Root[#^3 - # - 1 &, {k}]]"
            ));
            assert!(abs_approx(&diff) < 1e-8, "index {k} mismatch: {out}");
        }
    }

    #[test]
    fn test_to_radicals_quadratic_is_built_and_simplified() {
        let env = Env::new();
        builtins::register_builtins(&env);
        // Root 1 of x^2 - 2 is -Sqrt[2]; root 2 is +Sqrt[2].
        let neg = builtin_to_radicals(&[sqrt2_root_index(1)], &env).unwrap();
        let neg_s = neg.to_string();
        assert!(neg_s.contains("Sqrt[2]"), "got {neg_s}");
        assert!(!neg_s.contains('0'), "unsimplified zero artifact: {neg_s}");
        let pos = builtin_to_radicals(&[sqrt2_root_index(2)], &env).unwrap();
        assert!(pos.to_string().contains("Sqrt[2]"));
    }

    #[test]
    fn test_root_linear_exact() {
        let env = Env::new();
        builtins::register_builtins(&env);
        // Root of x - 3 is the exact integer 3.
        let coeffs = Value::List(vec![
            Value::Integer(Integer::from(-3)),
            Value::Integer(Integer::from(1)),
        ]);
        let result = builtin_root(&[coeffs, Value::Integer(Integer::from(1))], &env).unwrap();
        assert_eq!(result, Value::Integer(Integer::from(3)));
    }

    #[test]
    fn test_to_radicals_quartic_is_valid() {
        // x^4 − 10x² + 1 has roots ±Sqrt[2] ± Sqrt[3]; the resolvent has a
        // rational root, so ToRadicals must produce an agreeing radical.
        for k in 1..=4 {
            let diff = eval_str(&format!(
                "N[ToRadicals[Root[{{1, 0, -10, 0, 1}}, {k}]] - Root[{{1, 0, -10, 0, 1}}, {k}]]"
            ));
            assert!(
                abs_approx(&diff) < 1e-8,
                "quartic root {k} mismatch: {diff}"
            );
        }
    }

    #[test]
    fn test_root_reduce_radical_identity() {
        // RootReduce of a bare radical keeps it (numerically) unchanged.
        let diff = eval_str("N[RootReduce[Sqrt[2]] - Sqrt[2]]");
        assert!(abs_approx(&diff) < 1e-12, "got {diff}");
    }

    /// Magnitude of a numeric value (used to compare against zero).
    fn abs_approx(v: &Value) -> f64 {
        match v {
            Value::Integer(n) => n.to_f64().abs(),
            Value::Rational(r) => r.to_f64().abs(),
            Value::Real(r) => r.to_f64().abs(),
            Value::Complex { re, im } => (re * re + im * im).sqrt(),
            _ => f64::NAN,
        }
    }
}
