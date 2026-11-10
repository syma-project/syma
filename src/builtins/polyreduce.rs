//! polynomial/boolean reduction: Reduce, Eliminate, Resolve, SolveAlways, GroebnerBasis, Roots, ToRules, FindInstance.
//!
//! Gap-fill module; see the individual builtin docs for semantics.
#![allow(clippy::needless_range_loop)]

use crate::ast::Expr;
use crate::builtins::symbolic::simplify_call;
use crate::env::Env;
use crate::polynomial;
use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::{Float, Integer, Rational};
use std::collections::BTreeMap;

const EPS: f64 = 1e-9;

// ── Generic value helpers ───────────────────────────────────────────────────

fn call(head: &str, args: Vec<Value>) -> Value {
    Value::Call {
        head: head.to_string(),
        args,
    }
}

fn sym(name: &str) -> Value {
    Value::Symbol(name.to_string())
}

fn int_value(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

fn is_zero_value(v: &Value) -> bool {
    match v {
        Value::Integer(n) => n.is_zero(),
        Value::Rational(r) => r.is_zero(),
        Value::Real(x) => x.is_zero(),
        _ => false,
    }
}

fn as_rat(v: &Value) -> Option<Rational> {
    match v {
        Value::Integer(n) => Some(Rational::from(n.clone())),
        Value::Rational(r) => Some((**r).clone()),
        _ => None,
    }
}

fn rat_to_value(r: &Rational) -> Value {
    if r.denom() == &Integer::from(1) {
        Value::Integer(r.numer().clone())
    } else {
        Value::Rational(Box::new(r.clone()))
    }
}

// ── Coefficient arithmetic (numeric fast path, symbolic fallback) ───────────

fn c_add(a: &Value, b: &Value) -> Value {
    if let (Some(x), Some(y)) = (as_rat(a), as_rat(b)) {
        return rat_to_value(&(x + y));
    }
    let v = simplify_call("Plus", &[a.clone(), b.clone()]);
    if is_zero_value(&v) { int_value(0) } else { v }
}

fn c_neg(a: &Value) -> Value {
    if let Some(x) = as_rat(a) {
        return rat_to_value(&(-x));
    }
    simplify_call("Times", &[int_value(-1), a.clone()])
}

fn c_sub(a: &Value, b: &Value) -> Value {
    c_add(a, &c_neg(b))
}

fn c_mul(a: &Value, b: &Value) -> Value {
    if let (Some(x), Some(y)) = (as_rat(a), as_rat(b)) {
        return rat_to_value(&(x * y));
    }
    let v = simplify_call("Times", &[a.clone(), b.clone()]);
    if is_zero_value(&v) { int_value(0) } else { v }
}

fn c_div(a: &Value, b: &Value) -> Value {
    if let (Some(x), Some(y)) = (as_rat(a), as_rat(b))
        && !y.is_zero()
    {
        return rat_to_value(&(x / y));
    }
    let v = simplify_call("Divide", &[a.clone(), b.clone()]);
    if is_zero_value(&v) { int_value(0) } else { v }
}

fn c_pow(a: &Value, e: i64) -> Option<Value> {
    if e == 0 {
        return Some(int_value(1));
    }
    if let Some(x) = as_rat(a) {
        if e < 0 && x.is_zero() {
            return None;
        }
        let mut acc = Rational::from(1);
        for _ in 0..e.abs() {
            acc *= &x;
        }
        if e < 0 {
            acc = Rational::from(1) / acc;
        }
        return Some(rat_to_value(&acc));
    }
    if e < 0 {
        let pos = c_pow(a, -e)?;
        return Some(c_div(&int_value(1), &pos));
    }
    if let Value::Call { head, args } = a {
        match head.as_str() {
            "Power" if args.len() == 2 => {
                if let Value::Integer(k) = &args[1] {
                    return c_pow(&args[0], k.to_i64()? * e);
                }
            }
            "Sqrt" if args.len() == 1 && e % 2 == 0 => {
                return c_pow(&args[0], e / 2);
            }
            "Times" => {
                let mut acc = int_value(1);
                for f in args {
                    acc = c_mul(&acc, &c_pow(f, e)?);
                }
                return Some(acc);
            }
            "Divide" if args.len() == 2 => {
                return Some(c_div(&c_pow(&args[0], e)?, &c_pow(&args[1], e)?));
            }
            _ => {}
        }
    }
    let mut acc = int_value(1);
    for _ in 0..e {
        acc = c_mul(&acc, a);
    }
    Some(acc)
}

// ── Held-argument handling (HoldAll) ────────────────────────────────────────

fn norm_head(h: &str) -> &str {
    if h == "Alternatives" { "Or" } else { h }
}

fn expr_to_value(expr: &Expr) -> Value {
    match expr {
        Expr::Integer(n) => Value::Integer(n.clone()),
        Expr::Real(r) => Value::Real(r.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Str(s) => Value::Str(s.clone()),
        Expr::Null => Value::Null,
        Expr::Symbol(s) => Value::Symbol(s.clone()),
        Expr::List(items) => Value::List(items.iter().map(expr_to_value).collect()),
        Expr::Call { head, args } => {
            let head_str = match head.as_ref() {
                Expr::Symbol(s) => norm_head(s).to_string(),
                _ => String::new(),
            };
            Value::Call {
                head: head_str,
                args: args.iter().map(expr_to_value).collect(),
            }
        }
        _ => Value::Pattern(expr.clone()),
    }
}

fn strip_pattern(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => expr_to_value(e),
        Value::List(items) => Value::List(items.iter().map(strip_pattern).collect()),
        _ => v.clone(),
    }
}

fn strip_args(args: &[Value]) -> Vec<Value> {
    args.iter().map(strip_pattern).collect()
}

// ── Symbol collection ───────────────────────────────────────────────────────

fn collect_symbols(e: &Value, out: &mut Vec<String>) {
    match e {
        Value::Symbol(s) => {
            if !out.contains(s) {
                out.push(s.clone());
            }
        }
        Value::Call { head, args } => {
            if !out.contains(head) {
                out.push(head.clone());
            }
            for a in args {
                collect_symbols(a, out);
            }
        }
        Value::List(items) => {
            for i in items {
                collect_symbols(i, out);
            }
        }
        _ => {}
    }
}

fn parse_vars(v: &Value) -> Option<Vec<String>> {
    match v {
        Value::Symbol(s) => Some(vec![s.clone()]),
        Value::List(items) => {
            let mut out = Vec::new();
            for i in items {
                if let Value::Symbol(s) = i {
                    out.push(s.clone());
                } else {
                    return None;
                }
            }
            Some(out)
        }
        _ => None,
    }
}

// ── Multivariate polynomial over Value coefficients ─────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
struct VPoly(BTreeMap<Vec<u32>, Value>);

fn mono_add(a: &[u32], b: &[u32]) -> Vec<u32> {
    a.iter().zip(b).map(|(x, y)| x + y).collect()
}

fn mono_sub(a: &[u32], b: &[u32]) -> Vec<u32> {
    a.iter().zip(b).map(|(x, y)| x - y).collect()
}

fn mono_divides(g: &[u32], f: &[u32]) -> bool {
    g.iter().zip(f).all(|(x, y)| x <= y)
}

fn mono_lcm(a: &[u32], b: &[u32]) -> Vec<u32> {
    a.iter().zip(b).map(|(x, y)| *x.max(y)).collect()
}

impl VPoly {
    fn insert(&mut self, mono: Vec<u32>, coeff: Value) {
        let entry = self.0.entry(mono.clone()).or_insert_with(|| int_value(0));
        *entry = c_add(entry, &coeff);
        if is_zero_value(entry) {
            self.0.remove(&mono);
        }
    }

    fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    fn is_constant(&self) -> bool {
        self.0.keys().all(|m| m.iter().all(|&e| e == 0))
    }

    fn plus(&self, other: &VPoly) -> VPoly {
        let mut r = self.clone();
        for (m, c) in &other.0 {
            r.insert(m.clone(), c.clone());
        }
        r
    }

    fn minus(&self, other: &VPoly) -> VPoly {
        let mut r = self.clone();
        for (m, c) in &other.0 {
            r.insert(m.clone(), c_neg(c));
        }
        r
    }

    fn times(&self, other: &VPoly) -> VPoly {
        let mut r = VPoly::default();
        for (m1, c1) in &self.0 {
            for (m2, c2) in &other.0 {
                r.insert(mono_add(m1, m2), c_mul(c1, c2));
            }
        }
        r
    }

    fn scale(&self, v: &Value) -> VPoly {
        let mut r = VPoly::default();
        for (m, c) in &self.0 {
            let nc = c_mul(c, v);
            if !is_zero_value(&nc) {
                r.0.insert(m.clone(), nc);
            }
        }
        r
    }

    fn shift(&self, delta: &[u32]) -> VPoly {
        let mut r = VPoly::default();
        for (m, c) in &self.0 {
            r.0.insert(mono_add(m, delta), c.clone());
        }
        r
    }

    fn lm(&self) -> Option<&Vec<u32>> {
        self.0.keys().next_back()
    }

    fn lc(&self) -> Option<&Value> {
        self.0.values().next_back()
    }

    fn monic(&self) -> VPoly {
        if let Some(lc) = self.lc() {
            let inv = c_div(&int_value(1), lc);
            self.scale(&inv)
        } else {
            self.clone()
        }
    }

    fn constant(&self) -> Option<&Value> {
        if self.is_constant() {
            self.0.values().next()
        } else {
            None
        }
    }

    fn involves_var(&self, vi: usize) -> bool {
        self.0.keys().any(|m| m[vi] > 0)
    }
}

fn is_constant_active(p: &VPoly, active: &[usize]) -> bool {
    p.0.keys().all(|m| active.iter().all(|&vi| m[vi] == 0))
}

fn is_linear_active(p: &VPoly, active: &[usize]) -> bool {
    p.0.keys()
        .all(|m| active.iter().map(|&vi| m[vi]).sum::<u32>() <= 1)
}

fn is_univariate_active(p: &VPoly, active: &[usize], vi: usize) -> bool {
    p.0.keys()
        .all(|m| active.iter().all(|&v| v == vi || m[v] == 0))
}

fn poly_coeffs_univariate(p: &VPoly, vi: usize) -> Vec<Value> {
    let deg = p.0.keys().map(|m| m[vi]).max().unwrap_or(0) as usize;
    let mut coeffs = vec![int_value(0); deg + 1];
    for (m, c) in &p.0 {
        coeffs[m[vi] as usize] = c_add(&coeffs[m[vi] as usize], c);
    }
    coeffs
}

fn substitute_poly(p: &VPoly, vi: usize, r: &Value) -> VPoly {
    let mut out = VPoly::default();
    for (m, c) in &p.0 {
        let e = m[vi];
        let nc = if e == 0 {
            c.clone()
        } else {
            match c_pow(r, e as i64) {
                Some(pw) => c_mul(c, &pw),
                None => c.clone(),
            }
        };
        if !is_zero_value(&nc) {
            let mut nm = m.clone();
            nm[vi] = 0;
            out.insert(nm, nc);
        }
    }
    out
}

// ── Parsing an expression into a VPoly ──────────────────────────────────────

fn involves_vars(e: &Value, vars: &[String]) -> bool {
    match e {
        Value::Symbol(s) => vars.iter().any(|v| v == s),
        Value::Call { args, .. } => args.iter().any(|a| involves_vars(a, vars)),
        Value::List(items) => items.iter().any(|i| involves_vars(i, vars)),
        _ => false,
    }
}

fn parse_poly(e: &Value, vars: &[String]) -> Option<VPoly> {
    match e {
        Value::Integer(n) => {
            let mut p = VPoly::default();
            p.insert(vec![0; vars.len()], Value::Integer(n.clone()));
            Some(p)
        }
        Value::Rational(r) => {
            let mut p = VPoly::default();
            p.insert(vec![0; vars.len()], Value::Rational(r.clone()));
            Some(p)
        }
        Value::Real(re) => {
            let r = Rational::from_f64(re.to_f64())?;
            let mut p = VPoly::default();
            p.insert(vec![0; vars.len()], rat_to_value(&r));
            Some(p)
        }
        Value::Symbol(s) => {
            let mut p = VPoly::default();
            if let Some(i) = vars.iter().position(|v| v == s) {
                let mut mono = vec![0u32; vars.len()];
                mono[i] = 1;
                p.insert(mono, int_value(1));
            } else {
                p.insert(vec![0; vars.len()], Value::Symbol(s.clone()));
            }
            Some(p)
        }
        Value::Call { head, args } => parse_poly_call(head, args, vars, e),
        _ => None,
    }
}

fn parse_poly_call(head: &str, args: &[Value], vars: &[String], whole: &Value) -> Option<VPoly> {
    match head {
        "Plus" => {
            let mut acc = VPoly::default();
            for a in args {
                let p = parse_poly(a, vars)?;
                acc = acc.plus(&p);
            }
            Some(acc)
        }
        "Times" => {
            let mut acc = VPoly::default();
            acc.insert(vec![0; vars.len()], int_value(1));
            for a in args {
                let p = parse_poly(a, vars)?;
                acc = acc.times(&p);
            }
            Some(acc)
        }
        "Power" if args.len() == 2 => {
            let base = parse_poly(&args[0], vars)?;
            let e = match &args[1] {
                Value::Integer(n) => n.to_i64()?,
                _ => return None,
            };
            if e >= 0 {
                let mut acc = VPoly::default();
                acc.insert(vec![0; vars.len()], int_value(1));
                for _ in 0..e {
                    acc = acc.times(&base);
                }
                Some(acc)
            } else {
                if base.is_constant() {
                    let c = base.constant()?.clone();
                    let pw = c_pow(&c, e)?;
                    let mut p = VPoly::default();
                    p.insert(vec![0; vars.len()], pw);
                    Some(p)
                } else {
                    // Negative power of a variable: keep `whole` as an
                    // opaque coefficient (rational-function term).
                    let mut p = VPoly::default();
                    p.insert(vec![0; vars.len()], whole.clone());
                    Some(p)
                }
            }
        }
        "Divide" if args.len() == 2 => {
            let num = parse_poly(&args[0], vars)?;
            let den = parse_poly(&args[1], vars)?;
            if den.is_constant() {
                let d = den.constant()?.clone();
                if is_zero_value(&d) {
                    return None;
                }
                Some(num.scale(&c_div(&int_value(1), &d)))
            } else {
                // Non-constant denominator: treat as opaque coefficient.
                let mut p = VPoly::default();
                p.insert(vec![0; vars.len()], whole.clone());
                Some(p)
            }
        }
        "Subtract" if args.len() == 2 => {
            let a = parse_poly(&args[0], vars)?;
            let b = parse_poly(&args[1], vars)?;
            Some(a.minus(&b))
        }
        "Minus" if args.len() == 1 => {
            let a = parse_poly(&args[0], vars)?;
            Some(a.scale(&int_value(-1)))
        }
        _ => {
            if involves_vars(whole, vars) {
                None
            } else {
                let mut p = VPoly::default();
                p.insert(vec![0; vars.len()], whole.clone());
                Some(p)
            }
        }
    }
}

// ── Rendering VPoles back to Value ──────────────────────────────────────────

fn monomial_expr(mono: &[u32], vars: &[String]) -> Value {
    let mut factors: Vec<Value> = Vec::new();
    for (i, &e) in mono.iter().enumerate() {
        if e == 0 {
            continue;
        }
        if e == 1 {
            factors.push(sym(&vars[i]));
        } else {
            factors.push(call("Power", vec![sym(&vars[i]), int_value(e as i64)]));
        }
    }
    if factors.is_empty() {
        int_value(1)
    } else {
        simplify_call("Times", &factors)
    }
}

fn poly_to_value(p: &VPoly, vars: &[String]) -> Value {
    if p.is_zero() {
        return int_value(0);
    }
    let mut terms: Vec<Value> = Vec::new();
    for (mono, coeff) in p.0.iter().rev() {
        if is_zero_value(coeff) {
            continue;
        }
        let mexp = monomial_expr(mono, vars);
        if mono.iter().all(|&e| e == 0) {
            terms.push(coeff.clone());
        } else if matches!(coeff, Value::Integer(n) if *n == 1) {
            terms.push(mexp);
        } else if matches!(coeff, Value::Integer(n) if *n == -1) {
            terms.push(simplify_call("Times", &[int_value(-1), mexp]));
        } else {
            terms.push(simplify_call("Times", &[coeff.clone(), mexp]));
        }
    }
    simplify_call("Plus", &terms)
}

/// Clear denominators, divide by content, normalise the leading sign.
fn primitive(p: &VPoly) -> VPoly {
    if p.is_zero() {
        return p.clone();
    }
    let mut lcm = Integer::from(1);
    for c in p.0.values() {
        if let Some(r) = as_rat(c) {
            let d = r.denom().clone();
            let g = lcm.clone().gcd(&d);
            lcm = lcm * d / g;
        }
    }
    let mut ints: BTreeMap<Vec<u32>, Integer> = BTreeMap::new();
    let mut gcd = Integer::from(0);
    for (m, c) in &p.0 {
        match as_rat(c) {
            Some(r) => {
                let scaled = r.numer().clone() * Integer::from(&lcm / r.denom());
                gcd = gcd.gcd(&scaled);
                ints.insert(m.clone(), scaled);
            }
            None => {
                // symbolic coefficient: stop normalising
                return p.clone();
            }
        }
    }
    if gcd.is_zero() {
        return p.clone();
    }
    let mut out = VPoly::default();
    for (m, v) in ints {
        out.0.insert(m, Value::Integer(v / &gcd));
    }
    if let Some(Value::Integer(n)) = out.lc()
        && n.is_negative()
    {
        return out.scale(&int_value(-1));
    }
    out
}

// ── Buchberger ──────────────────────────────────────────────────────────────

fn spoly(f: &VPoly, g: &VPoly) -> VPoly {
    match (f.lm().cloned(), g.lm().cloned()) {
        (Some(lmf), Some(lmg)) => {
            let lcm = mono_lcm(&lmf, &lmg);
            let sf = f.shift(&mono_sub(&lcm, &lmf));
            let sg = g.shift(&mono_sub(&lcm, &lmg));
            sf.minus(&sg)
        }
        _ => VPoly::default(),
    }
}

fn poly_reduce(f: &VPoly, basis: &[VPoly]) -> VPoly {
    let mut r = f.clone();
    while let Some(lm) = r.lm().cloned() {
        let mut reduced = false;
        for g in basis {
            if let Some(glm) = g.lm()
                && mono_divides(glm, &lm)
                && let (Some(rlc), Some(glc)) = (r.lc().cloned(), g.lc().cloned())
            {
                let factor = c_div(&rlc, &glc);
                let shift = mono_sub(&lm, glm);
                let sub = g.shift(&shift).scale(&factor);
                r = r.minus(&sub);
                reduced = true;
                break;
            }
        }
        if !reduced {
            break;
        }
    }
    r
}

fn buchberger(polys: &[VPoly], _nvars: usize) -> Option<Vec<VPoly>> {
    let mut g: Vec<VPoly> = Vec::new();
    for p in polys {
        if p.is_zero() {
            continue;
        }
        let m = p.monic();
        if !g.iter().any(|x| x == &m) {
            g.push(m);
        }
    }
    if g.is_empty() {
        return Some(vec![]);
    }
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for i in 0..g.len() {
        for j in (i + 1)..g.len() {
            pairs.push((i, j));
        }
    }
    let mut iters = 0usize;
    while let Some((i, j)) = pairs.pop() {
        iters += 1;
        if iters > 2000 {
            break;
        }
        if i >= g.len() || j >= g.len() {
            continue;
        }
        let s = spoly(&g[i], &g[j]);
        let r = poly_reduce(&s, &g);
        if !r.is_zero() {
            let rm = r.monic();
            let idx = g.len();
            g.push(rm);
            for k in 0..idx {
                pairs.push((k, idx));
            }
        }
    }
    // Interreduce.
    let n = g.len();
    let mut out: Vec<VPoly> = Vec::new();
    for i in 0..n {
        let others: Vec<VPoly> = (0..n).filter(|&j| j != i).map(|j| g[j].clone()).collect();
        let r = poly_reduce(&g[i], &others);
        if !r.is_zero() {
            let m = r.monic();
            if !out.iter().any(|x| x == &m) {
                out.push(m);
            }
        }
    }
    out.sort_by(|a, b| b.lm().cmp(&a.lm()));
    Some(out)
}

// ── Univariate root finding ─────────────────────────────────────────────────

fn divisors(n: &Integer) -> Vec<Integer> {
    let n = n.clone().abs();
    if n.is_zero() {
        return vec![Integer::from(1)];
    }
    match n.to_i64() {
        Some(v) => {
            let v = v.abs();
            let mut out = Vec::new();
            let mut i = 1i64;
            while i * i <= v {
                if v % i == 0 {
                    out.push(Integer::from(i));
                    if i != v / i {
                        out.push(Integer::from(v / i));
                    }
                }
                i += 1;
                if out.len() > 200 {
                    break;
                }
            }
            out
        }
        None => vec![Integer::from(1), n],
    }
}

fn poly_eval_value(coeffs: &[Rational], x: &Rational) -> Rational {
    polynomial::poly_eval(coeffs, x)
}

fn find_rational_root(coeffs: &[Rational]) -> Option<Rational> {
    // Clear denominators to integers.
    let mut lcm = Integer::from(1);
    for c in coeffs {
        let d = c.denom().clone();
        let g = lcm.clone().gcd(&d);
        lcm = lcm * d / g;
    }
    let ints: Vec<Integer> = coeffs
        .iter()
        .map(|c| c.numer().clone() * Integer::from(&lcm / c.denom()))
        .collect();
    let a0 = ints[0].clone();
    let an = ints[ints.len() - 1].clone();
    if a0.is_zero() {
        return Some(Rational::from(0));
    }
    let ps = divisors(&a0);
    let qs = divisors(&an);
    let mut candidates: Vec<Rational> = Vec::new();
    for p in &ps {
        for q in &qs {
            if q.is_zero() {
                continue;
            }
            let r = Rational::from((p.clone(), q.clone()));
            candidates.push(r.clone());
            candidates.push(-r);
        }
    }
    candidates.sort_by(|a, b| {
        a.to_f64()
            .abs()
            .partial_cmp(&b.to_f64().abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates
        .into_iter()
        .find(|r| poly_eval_value(coeffs, r).is_zero())
}

/// Divide polynomial `c` (ascending) exactly by `(x - r)`.
fn poly_div_linear(c: &[Rational], r: &Rational) -> Vec<Rational> {
    let deg = c.len() - 1;
    let mut b = vec![Rational::from(0); deg];
    let mut carry = Rational::from(0);
    for k in (1..=deg).rev() {
        let val = c[k].clone() + r.clone() * carry.clone();
        b[k - 1] = val.clone();
        carry = val;
    }
    b
}

fn quadratic_roots(c: &[Rational], out: &mut Vec<Value>) {
    // c = [c0, c1, c2]
    let disc = c[1].clone() * c[1].clone() - Rational::from(4) * c[2].clone() * c[0].clone();
    let neg_b = -c[1].clone();
    let two_a = Rational::from(2) * c[2].clone();
    if disc.is_zero() {
        out.push(rat_to_value(&(neg_b / two_a)));
        return;
    }
    if disc.is_positive() {
        if let Some(sq) = rational_sqrt(&disc) {
            out.push(rat_to_value(
                &((neg_b.clone() - sq.clone()) / two_a.clone()),
            ));
            out.push(rat_to_value(&((neg_b + sq) / two_a)));
        } else {
            let rad = call("Sqrt", vec![rat_to_value(&disc)]);
            out.push(simplify_call(
                "Divide",
                &[
                    simplify_call("Plus", &[rat_to_value(&neg_b), c_neg(&rad)]),
                    rat_to_value(&two_a),
                ],
            ));
            out.push(simplify_call(
                "Divide",
                &[
                    simplify_call("Plus", &[rat_to_value(&neg_b), rad]),
                    rat_to_value(&two_a),
                ],
            ));
        }
    } else {
        let pos = -disc;
        let rad = simplify_call("Times", &[sym("I"), call("Sqrt", vec![rat_to_value(&pos)])]);
        out.push(simplify_call(
            "Divide",
            &[
                simplify_call("Plus", &[rat_to_value(&neg_b), c_neg(&rad)]),
                rat_to_value(&two_a),
            ],
        ));
        out.push(simplify_call(
            "Divide",
            &[
                simplify_call("Plus", &[rat_to_value(&neg_b), rad]),
                rat_to_value(&two_a),
            ],
        ));
    }
}

fn rational_sqrt(r: &Rational) -> Option<Rational> {
    if r.is_negative() {
        return None;
    }
    let num = r.numer().clone();
    let den = r.denom().clone();
    let sn = num.clone().sqrt();
    let sd = den.clone().sqrt();
    if sn.clone() * sn.clone() == num && sd.clone() * sd.clone() == den {
        Some(Rational::from((sn, sd)))
    } else {
        None
    }
}

/// All exact roots of a rational-coefficient polynomial (ascending coeffs).
fn roots_rational(coeffs: &[Value]) -> Option<Vec<Value>> {
    let mut c: Vec<Rational> = Vec::new();
    for v in coeffs {
        c.push(as_rat(v)?);
    }
    while c.len() > 1 && c.last().unwrap().is_zero() {
        c.pop();
    }
    if c.len() == 1 {
        return Some(vec![]);
    }
    let mut roots: Vec<Value> = Vec::new();
    loop {
        let deg = c.len() - 1;
        if deg == 0 {
            break;
        }
        if deg == 1 {
            let r = -(c[0].clone()) / c[1].clone();
            roots.push(rat_to_value(&r));
            break;
        }
        if deg == 2 {
            quadratic_roots(&c, &mut roots);
            break;
        }
        if let Some(r) = find_rational_root(&c) {
            roots.push(rat_to_value(&r));
            c = poly_div_linear(&c, &r);
            continue;
        }
        for i in 1..=deg {
            roots.push(Value::Root {
                coeffs: c.clone(),
                index: i,
            });
        }
        break;
    }
    Some(roots)
}

fn approx_value(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Rational(r) => Some(r.to_f64()),
        Value::Real(x) => Some(x.to_f64()),
        Value::Complex { re, .. } => Some(*re),
        Value::Symbol(s) if s == "I" => Some(0.0),
        Value::Symbol(s) if s == "Pi" => Some(std::f64::consts::PI),
        Value::Symbol(s) if s == "E" => Some(std::f64::consts::E),
        Value::Call { head, args } => match head.as_str() {
            "Plus" => {
                let mut acc = 0.0;
                for a in args {
                    acc += approx_value(a)?;
                }
                Some(acc)
            }
            "Times" => {
                let mut acc = 1.0;
                for a in args {
                    acc *= approx_value(a)?;
                }
                Some(acc)
            }
            "Divide" if args.len() == 2 => Some(approx_value(&args[0])? / approx_value(&args[1])?),
            "Power" if args.len() == 2 => {
                Some(approx_value(&args[0])?.powf(approx_value(&args[1])?))
            }
            "Sqrt" if args.len() == 1 => Some(approx_value(&args[0])?.sqrt()),
            "Minus" if args.len() == 1 => Some(-approx_value(&args[0])?),
            "Abs" if args.len() == 1 => Some(approx_value(&args[0])?.abs()),
            _ => None,
        },
        Value::Root { coeffs, index } => {
            let roots = polynomial::find_polynomial_roots(coeffs);
            if *index > 0 && *index <= roots.len() {
                Some(roots[*index - 1].0)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn sort_roots(roots: &mut [Value]) {
    roots.sort_by(|a, b| {
        let av = approx_value(a).unwrap_or(f64::INFINITY);
        let bv = approx_value(b).unwrap_or(f64::INFINITY);
        av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Roots of a polynomial whose coefficients may be symbolic (linear only) or numeric.
fn roots_of_values(coeffs: &[Value]) -> Option<Vec<Value>> {
    let mut c: Vec<Value> = coeffs.to_vec();
    while c.len() > 1 && is_zero_value(c.last().unwrap()) {
        c.pop();
    }
    if c.len() == 1 {
        return Some(vec![]);
    }
    if c.iter().all(|x| as_rat(x).is_some()) {
        let mut roots = roots_rational(&c)?;
        sort_roots(&mut roots);
        return Some(roots);
    }
    if c.len() == 2 {
        let r = c_div(&c_neg(&c[0]), &c[1]);
        return Some(vec![r]);
    }
    let fl: Vec<f64> = c.iter().map(approx_value).collect::<Option<Vec<_>>>()?;
    let rats: Vec<Rational> = fl
        .iter()
        .map(|&x| Rational::from_f64(x).unwrap_or_else(|| Rational::from(0)))
        .collect();
    let mut roots = polynomial::find_polynomial_roots(&rats);
    roots.retain(|(_, im)| im.abs() < 1e-9);
    roots.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    Some(
        roots
            .iter()
            .map(|&(re, _)| Value::Real(Float::with_val(DEFAULT_PRECISION, re)))
            .collect(),
    )
}

// ── Linear algebra over Values ──────────────────────────────────────────────

fn rref_solve(mut a: Vec<Vec<Value>>, mut b: Vec<Value>, names: &[String]) -> Option<Vec<Value>> {
    let rows = a.len();
    let cols = names.len();
    let mut pivots: Vec<(usize, usize)> = Vec::new();
    let mut r = 0usize;
    for c in 0..cols {
        if r >= rows {
            break;
        }
        let mut pr = None;
        for i in r..rows {
            if !is_zero_value(&a[i][c]) {
                pr = Some(i);
                break;
            }
        }
        let pr = match pr {
            Some(x) => x,
            None => continue,
        };
        a.swap(r, pr);
        b.swap(r, pr);
        let piv = a[r][c].clone();
        for j in 0..cols {
            a[r][j] = c_div(&a[r][j], &piv);
        }
        b[r] = c_div(&b[r], &piv);
        for i in 0..rows {
            if i == r || is_zero_value(&a[i][c]) {
                continue;
            }
            let factor = a[i][c].clone();
            for j in 0..cols {
                let t = c_mul(&factor, &a[r][j]);
                a[i][j] = c_sub(&a[i][j], &t);
            }
            let t = c_mul(&factor, &b[r]);
            b[i] = c_sub(&b[i], &t);
        }
        pivots.push((c, r));
        r += 1;
    }
    for i in 0..rows {
        if a[i].iter().all(is_zero_value) && !is_zero_value(&b[i]) {
            return None;
        }
    }
    // Express pivot variables in terms of the free variables left in the
    // reduced rows (rows are in RREF, so a pivot row may still carry non-zero
    // coefficients in free columns).
    let free: Vec<usize> = (0..cols)
        .filter(|c| !pivots.iter().any(|(pc, _)| pc == c))
        .collect();
    let mut out = vec![int_value(0); cols];
    for c in 0..cols {
        if let Some((_, pr)) = pivots.iter().find(|(pc, _)| *pc == c) {
            let mut val = b[*pr].clone();
            for &fc in &free {
                if !is_zero_value(&a[*pr][fc]) {
                    let t = c_mul(&a[*pr][fc], &sym(&names[fc]));
                    val = c_sub(&val, &t);
                }
            }
            out[c] = val;
        } else {
            out[c] = sym(&names[c]);
        }
    }
    Some(out)
}

/// Substitute every occurrence of the symbol `name` in `e` with `val`.
fn subst_symbol(e: &Value, name: &str, val: &Value) -> Value {
    match e {
        Value::Symbol(s) if s == name => val.clone(),
        Value::Call { head, args } => Value::Call {
            head: head.clone(),
            args: args.iter().map(|a| subst_symbol(a, name, val)).collect(),
        },
        Value::List(items) => {
            Value::List(items.iter().map(|i| subst_symbol(i, name, val)).collect())
        }
        _ => e.clone(),
    }
}

/// Recursively fold an expression through the simplifier (numeric constants,
/// identity elements, nested arithmetic).
fn simplify_deep(v: &Value) -> Value {
    match v {
        Value::Call { head, args } => {
            let sargs: Vec<Value> = args.iter().map(simplify_deep).collect();
            simplify_call(head, &sargs)
        }
        Value::List(items) => Value::List(items.iter().map(simplify_deep).collect()),
        _ => v.clone(),
    }
}

fn row_for_poly(p: &VPoly, active: &[usize]) -> Option<(Vec<Value>, Value)> {
    let mut row = vec![int_value(0); active.len()];
    let mut constant = int_value(0);
    for (m, c) in &p.0 {
        let mut idx = None;
        for (k, &vi) in active.iter().enumerate() {
            let e = m[vi];
            if e > 1 {
                return None;
            }
            if e == 1 {
                if idx.is_some() {
                    return None;
                }
                idx = Some(k);
            }
        }
        match idx {
            Some(k) => row[k] = c_add(&row[k], c),
            None => constant = c_add(&constant, c),
        }
    }
    Some((row, constant))
}

// ── Boolean condition builders ──────────────────────────────────────────────

fn and(conds: Vec<Value>) -> Value {
    let mut out: Vec<Value> = Vec::new();
    for c in conds {
        match c {
            Value::Bool(true) => {}
            Value::Bool(false) => return Value::Bool(false),
            Value::Call { head, args } if head == "And" => out.extend(args),
            other => out.push(other),
        }
    }
    match out.len() {
        0 => Value::Bool(true),
        1 => out.pop().unwrap(),
        _ => call("And", out),
    }
}

fn or(conds: Vec<Value>) -> Value {
    let mut out: Vec<Value> = Vec::new();
    for c in conds {
        match c {
            Value::Bool(false) => {}
            Value::Bool(true) => return Value::Bool(true),
            Value::Call { head, args } if head == "Or" => out.extend(args),
            other => out.push(other),
        }
    }
    match out.len() {
        0 => Value::Bool(false),
        1 => out.pop().unwrap(),
        _ => call("Or", out),
    }
}

fn equal(lhs: Value, rhs: Value) -> Value {
    if let (Some(a), Some(b)) = (numeric_f64(&lhs), numeric_f64(&rhs)) {
        return Value::Bool(a == b);
    }
    call("Equal", vec![lhs, rhs])
}

fn numeric_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Rational(r) => Some(r.to_f64()),
        Value::Real(x) => Some(x.to_f64()),
        _ => None,
    }
}

// ── System solving (equations only) ─────────────────────────────────────────

fn linear_solution(polys: &[VPoly], vars: &[String], active: &[usize]) -> Option<Value> {
    let mut a: Vec<Vec<Value>> = Vec::new();
    let mut b: Vec<Value> = Vec::new();
    for p in polys {
        let (row, constant) = row_for_poly(p, active)?;
        a.push(row);
        b.push(c_neg(&constant));
    }
    let names: Vec<String> = active.iter().map(|&vi| vars[vi].clone()).collect();
    let sol = match rref_solve(a, b, &names) {
        Some(s) => s,
        None => return Some(Value::Bool(false)),
    };
    let mut conds: Vec<Value> = Vec::new();
    for (k, &_vi) in active.iter().enumerate() {
        let var = sym(&names[k]);
        if sol[k].struct_eq(&var) {
            continue;
        }
        conds.push(equal(var, sol[k].clone()));
    }
    Some(and(conds))
}

fn solve_for_var(
    polys: &[VPoly],
    vars: &[String],
    active: &[usize],
    vi: usize,
    p: &VPoly,
) -> Option<Value> {
    let coeffs = poly_coeffs_univariate(p, vi);
    let roots = roots_of_values(&coeffs)?;
    if roots.is_empty() {
        return Some(Value::Bool(false));
    }
    let sub_active: Vec<usize> = active.iter().copied().filter(|&x| x != vi).collect();
    let mut branches: Vec<Value> = Vec::new();
    for r in roots {
        let newps: Vec<VPoly> = polys.iter().map(|q| substitute_poly(q, vi, &r)).collect();
        let sub = reduce_active(&newps, vars, &sub_active)?;
        branches.push(and(vec![equal(sym(&vars[vi]), r), sub]));
    }
    Some(or(branches))
}

fn reduce_active(polys: &[VPoly], vars: &[String], active: &[usize]) -> Option<Value> {
    let mut extra: Vec<Value> = Vec::new();
    let mut ps: Vec<VPoly> = Vec::new();
    for p in polys {
        if p.is_zero() {
            continue;
        }
        if is_constant_active(p, active) {
            let total = p.0.values().fold(int_value(0), |acc, c| c_add(&acc, c));
            if is_zero_value(&total) {
                continue;
            }
            match approx_value(&total) {
                Some(ap) if ap.abs() < EPS => continue,
                Some(_) => return Some(Value::Bool(false)),
                None => {
                    extra.push(equal(total, int_value(0)));
                    continue;
                }
            }
        }
        ps.push(p.clone());
    }
    if ps.is_empty() {
        return Some(and(extra));
    }
    if active.is_empty() {
        return Some(Value::Bool(false));
    }
    let base = reduce_core(&ps, vars, active)?;
    extra.push(base);
    Some(and(extra))
}

fn reduce_core(ps: &[VPoly], vars: &[String], active: &[usize]) -> Option<Value> {
    if ps.iter().all(|p| is_linear_active(p, active)) {
        return linear_solution(ps, vars, active);
    }
    for &vi in active {
        if let Some(p) = ps
            .iter()
            .find(|p| is_univariate_active(p, active, vi) && !is_constant_active(p, active))
            .cloned()
        {
            return solve_for_var(ps, vars, active, vi, &p);
        }
    }
    let basis = buchberger(ps, vars.len())?;
    for &vi in active.iter().rev() {
        if let Some(p) = basis
            .iter()
            .find(|p| is_univariate_active(p, active, vi) && !is_constant_active(p, active))
            .cloned()
        {
            return solve_for_var(ps, vars, active, vi, &p);
        }
    }
    None
}

impl VPoly {
    fn all_constant_zero(&self) -> bool {
        self.is_constant() && self.constant().map(is_zero_value).unwrap_or(true)
    }
}

// ── Conjunct handling for Reduce ────────────────────────────────────────────

fn flatten_conjuncts(e: &Value, out: &mut Vec<Value>) {
    match e {
        Value::Call { head, args } if head == "And" => {
            for a in args {
                flatten_conjuncts(a, out);
            }
        }
        Value::List(items) => {
            for i in items {
                flatten_conjuncts(i, out);
            }
        }
        _ => out.push(e.clone()),
    }
}

// ── Univariate regions (equations + inequalities) ───────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Rel {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    Unequal,
}

fn rel_from_head(head: &str) -> Option<Rel> {
    match head {
        "Less" => Some(Rel::Less),
        "LessEqual" => Some(Rel::LessEqual),
        "Greater" => Some(Rel::Greater),
        "GreaterEqual" => Some(Rel::GreaterEqual),
        "Equal" => Some(Rel::Equal),
        "Unequal" => Some(Rel::Unequal),
        _ => None,
    }
}

fn rel_holds(r: Rel, v: f64) -> bool {
    match r {
        Rel::Less => v < -EPS,
        Rel::LessEqual => v <= EPS,
        Rel::Greater => v > EPS,
        Rel::GreaterEqual => v >= -EPS,
        Rel::Equal => v.abs() < EPS,
        Rel::Unequal => v.abs() > EPS,
    }
}

fn rel_holds_point(r: Rel, v: f64) -> bool {
    match r {
        Rel::Less => v < -EPS,
        Rel::LessEqual => v <= EPS,
        Rel::Greater => v > EPS,
        Rel::GreaterEqual => v >= -EPS,
        Rel::Equal => v.abs() < EPS,
        Rel::Unequal => v.abs() > EPS,
    }
}

fn eval_f64_poly(coeffs: &[f64], x: f64) -> f64 {
    let mut acc = 0.0;
    for c in coeffs.iter().rev() {
        acc = acc * x + c;
    }
    acc
}

/// Exact real roots of a poly with (possibly symbolic) coefficients, paired with approximation.
fn real_roots(p: &VPoly, vi: usize) -> Option<Vec<(Value, f64)>> {
    let coeffs = poly_coeffs_univariate(p, vi);
    let roots = roots_of_values(&coeffs)?;
    let mut out: Vec<(Value, f64)> = Vec::new();
    for r in roots {
        if let Some(a) = approx_value(&r) {
            out.push((r, a));
        }
    }
    out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    // Deduplicate by approximation.
    let mut dedup: Vec<(Value, f64)> = Vec::new();
    for (v, a) in out {
        if dedup
            .last()
            .map(|(_, la)| (la - a).abs() < 1e-7)
            .unwrap_or(false)
        {
            continue;
        }
        dedup.push((v, a));
    }
    Some(dedup)
}

fn region_condition(relations: &[(VPoly, Rel)], var: &str) -> Option<Value> {
    if relations.is_empty() {
        return Some(Value::Bool(true));
    }
    let vi = 0;
    let mut roots: Vec<(Value, f64)> = Vec::new();
    for (p, _) in relations {
        if p.is_zero() {
            continue;
        }
        for r in real_roots(p, vi)? {
            roots.push(r);
        }
    }
    roots.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut dedup: Vec<(Value, f64)> = Vec::new();
    for (v, a) in roots {
        if dedup
            .last()
            .map(|(_, la)| (la - a).abs() < 1e-7)
            .unwrap_or(false)
        {
            continue;
        }
        dedup.push((v, a));
    }
    let roots = dedup;
    let k = roots.len();

    // Approximate coefficient vectors for sampling.
    let mut numeric: Vec<Vec<f64>> = Vec::new();
    for (p, _) in relations {
        let coeffs = poly_coeffs_univariate(p, vi);
        let fl: Vec<f64> = coeffs
            .iter()
            .map(approx_value)
            .collect::<Option<Vec<_>>>()?;
        numeric.push(fl);
    }

    let point_ok = |x: f64| -> bool {
        relations.iter().zip(numeric.iter()).all(|((p, r), cf)| {
            if p.is_zero() {
                return true;
            }
            rel_holds_point(*r, eval_f64_poly(cf, x))
        })
    };
    let interval_ok = |x: f64| -> bool {
        relations.iter().zip(numeric.iter()).all(|((p, r), cf)| {
            if p.is_zero() {
                return true;
            }
            if *r == Rel::Equal {
                return eval_f64_poly(cf, x).abs() < EPS;
            }
            rel_holds(*r, eval_f64_poly(cf, x))
        })
    };

    let total_segments = 2 * k + 1;
    let mut included = vec![false; total_segments];
    for (s, inc) in included.iter_mut().enumerate() {
        if s % 2 == 1 {
            let ri = (s - 1) / 2;
            *inc = point_ok(roots[ri].1);
        } else {
            let gi = s / 2;
            let left = if gi == 0 {
                if k == 0 { 0.0 } else { roots[0].1 - 1.0 }
            } else {
                roots[gi - 1].1
            };
            let right = if gi == k {
                if k == 0 { 0.0 } else { roots[k - 1].1 + 1.0 }
            } else {
                roots[gi].1
            };
            let sample = if gi == 0 && k == 0 {
                0.0
            } else if gi == 0 {
                right - 1.0
            } else if gi == k {
                left + 1.0
            } else {
                (left + right) / 2.0
            };
            *inc = interval_ok(sample);
        }
    }

    // Build conditions for maximal included runs.
    let mut conds: Vec<Value> = Vec::new();
    let mut s = 0;
    let mut all_included = true;
    for inc in &included {
        all_included &= *inc;
    }
    if all_included {
        return Some(Value::Bool(true));
    }
    while s < total_segments {
        if !included[s] {
            s += 1;
            continue;
        }
        let start = s;
        while s < total_segments && included[s] {
            s += 1;
        }
        let end = s - 1;
        if start == end && start % 2 == 1 {
            let ri = (start - 1) / 2;
            conds.push(equal(sym(var), roots[ri].0.clone()));
            continue;
        }
        let mut parts: Vec<Value> = Vec::new();
        // Left boundary
        if start % 2 == 1 {
            let ri = (start - 1) / 2;
            parts.push(call("LessEqual", vec![roots[ri].0.clone(), sym(var)]));
        } else {
            let gi = start / 2;
            if gi > 0 {
                parts.push(call("Less", vec![roots[gi - 1].0.clone(), sym(var)]));
            }
        }
        // Right boundary
        if end % 2 == 1 {
            let ri = (end - 1) / 2;
            parts.push(call("LessEqual", vec![sym(var), roots[ri].0.clone()]));
        } else {
            let gi = end / 2;
            if gi < k {
                parts.push(call("Less", vec![sym(var), roots[gi].0.clone()]));
            }
        }
        conds.push(and(parts));
    }
    Some(or(conds))
}

fn flatten_chain(c: &Value, ops: &mut Vec<Value>, rels: &mut Vec<Rel>) {
    if let Value::Call { head, args } = c
        && args.len() == 2
        && let Some(rel) = rel_from_head(head)
    {
        flatten_chain(&args[0], ops, rels);
        ops.push(args[1].clone());
        rels.push(rel);
        return;
    }
    ops.push(c.clone());
}

fn reduce_univariate(conjuncts: &[Value], var: &str) -> Option<Value> {
    let mut relations: Vec<(VPoly, Rel)> = Vec::new();
    for c in conjuncts {
        match c {
            Value::Bool(true) => {}
            Value::Bool(false) => return Some(Value::Bool(false)),
            Value::Call { head, args } if args.len() == 2 && rel_from_head(head).is_some() => {
                let mut ops: Vec<Value> = Vec::new();
                let mut rels: Vec<Rel> = Vec::new();
                flatten_chain(c, &mut ops, &mut rels);
                for i in 0..rels.len() {
                    let p = parse_poly(&ops[i], std::slice::from_ref(&var.to_string()))?.minus(
                        &parse_poly(&ops[i + 1], std::slice::from_ref(&var.to_string()))?,
                    );
                    relations.push((p, rels[i]));
                }
            }
            _ => {
                let vars = vec![var.to_string()];
                let p = parse_poly(c, &vars)?;
                relations.push((p, Rel::Equal));
            }
        }
    }
    if let Some(v) = region_condition(&relations, var) {
        return Some(v);
    }
    // Symbolic-coefficient linear relations: case-split on the sign.
    let vs = vec![var.to_string()];
    if relations
        .iter()
        .all(|(p, _)| is_linear_active(p, &[0]) || p.is_zero())
        && let Some(v) = reduce_linear_rel(&relations, &vs, &[0])
    {
        return Some(v);
    }
    // Fallback for symbolic coefficients: solve equalities independently.
    let mut conds: Vec<Value> = Vec::new();
    let mut any_eq = false;
    for (p, rel) in &relations {
        if *rel != Rel::Equal {
            return None;
        }
        any_eq = true;
        if p.is_zero() {
            continue;
        }
        let coeffs = poly_coeffs_univariate(p, 0);
        if p.is_constant() {
            conds.push(equal(coeffs[0].clone(), int_value(0)));
            continue;
        }
        let roots = roots_of_values(&coeffs)?;
        if roots.is_empty() {
            conds.push(Value::Bool(false));
            continue;
        }
        conds.push(or(roots.into_iter().map(|r| equal(sym(var), r)).collect()));
    }
    if !any_eq {
        return None;
    }
    Some(and(conds))
}

// ── Multivariate linear relations (equalities + inequalities) ───────────────

/// Parse a conjunction of (possibly chained) relations into polynomial/relation
/// pairs. Returns `None` when a piece is not polynomial.
fn parse_relations(conjuncts: &[Value], vars: &[String]) -> Option<Vec<(VPoly, Rel)>> {
    let mut rels: Vec<(VPoly, Rel)> = Vec::new();
    for c in conjuncts {
        match c {
            Value::Bool(true) => {}
            Value::Bool(false) => return None,
            Value::Call { head, args } if args.len() == 2 && rel_from_head(head).is_some() => {
                let mut ops: Vec<Value> = Vec::new();
                let mut op_rels: Vec<Rel> = Vec::new();
                flatten_chain(c, &mut ops, &mut op_rels);
                for i in 0..op_rels.len() {
                    let p = parse_poly(&ops[i], vars)?.minus(&parse_poly(&ops[i + 1], vars)?);
                    rels.push((p, op_rels[i]));
                }
            }
            _ => {
                let p = parse_poly(c, vars)?;
                rels.push((p, Rel::Equal));
            }
        }
    }
    Some(rels)
}

fn rel_flip(r: Rel) -> Rel {
    match r {
        Rel::Less => Rel::Greater,
        Rel::LessEqual => Rel::GreaterEqual,
        Rel::Greater => Rel::Less,
        Rel::GreaterEqual => Rel::LessEqual,
        other => other,
    }
}

fn rel_head(r: Rel) -> &'static str {
    match r {
        Rel::Less => "Less",
        Rel::LessEqual => "LessEqual",
        Rel::Greater => "Greater",
        Rel::GreaterEqual => "GreaterEqual",
        Rel::Equal => "Equal",
        Rel::Unequal => "Unequal",
    }
}

/// Split `p = a*vars[vi] + g`, where `a` may itself be a polynomial in the
/// other variables (multilinear case), into `(a, g)`.
fn linear_split(p: &VPoly, vi: usize, vars: &[String]) -> (Value, VPoly) {
    let mut a_terms: Vec<Value> = Vec::new();
    let mut g = VPoly::default();
    for (m, c) in &p.0 {
        if m[vi] == 0 {
            g.insert(m.clone(), c.clone());
        } else {
            let mut rest = m.clone();
            rest[vi] = 0;
            let coeff = if rest.iter().all(|&e| e == 0) {
                c.clone()
            } else {
                c_mul(c, &monomial_expr(&rest, vars))
            };
            a_terms.push(coeff);
        }
    }
    let a = if a_terms.is_empty() {
        int_value(0)
    } else {
        simplify_call("Plus", &a_terms)
    };
    (a, g)
}

/// Build `value rel 0`, collapsing numeric values to a Boolean.
fn const_rel_value(v: &Value, r: Rel) -> Value {
    if let Some(x) = numeric_f64(v) {
        let b = match r {
            Rel::Less => x < 0.0,
            Rel::LessEqual => x <= 0.0,
            Rel::Greater => x > 0.0,
            Rel::GreaterEqual => x >= 0.0,
            Rel::Equal => x == 0.0,
            Rel::Unequal => x != 0.0,
        };
        return Value::Bool(b);
    }
    call(rel_head(r), vec![v.clone(), int_value(0)])
}

/// Reduce a conjunction of linear (in)equalities over `active`, eliminating the
/// variables one at a time. Requires numeric leading coefficients.
fn reduce_linear_rel(rels: &[(VPoly, Rel)], vars: &[String], active: &[usize]) -> Option<Value> {
    let mut work: Vec<(VPoly, Rel)> = Vec::new();
    let mut parts: Vec<Value> = Vec::new();
    for (p, r) in rels {
        if is_constant_active(p, active) {
            let total = p.0.values().fold(int_value(0), |acc, c| c_add(&acc, c));
            match const_rel_value(&total, *r) {
                Value::Bool(true) => {}
                Value::Bool(false) => return Some(Value::Bool(false)),
                other => parts.push(other),
            }
            continue;
        }
        work.push((p.clone(), *r));
    }
    if work.is_empty() {
        return Some(and(parts));
    }
    if active.is_empty() {
        return Some(Value::Bool(false));
    }
    let vi = active[0];
    let rest: Vec<usize> = active[1..].to_vec();
    if let Some(pos) = work
        .iter()
        .position(|(p, r)| *r == Rel::Equal && !is_zero_value(&linear_split(p, vi, vars).0))
    {
        // Eliminate `vi` through an equality, then continue on the rest.
        let (p, _) = work.remove(pos);
        let (a, g) = linear_split(&p, vi, vars);
        let value = c_div(&c_neg(&poly_to_value(&g, vars)), &a);
        parts.push(equal(sym(&vars[vi]), value.clone()));
        let mut sub_rels: Vec<(VPoly, Rel)> = Vec::new();
        for (q, r) in &work {
            let e = poly_to_value(q, vars);
            let es = subst_symbol(&e, &vars[vi], &value);
            sub_rels.push((parse_poly(&es, vars)?, *r));
        }
        parts.push(reduce_linear_rel(&sub_rels, vars, &rest)?);
        return Some(and(parts));
    }
    // Case-split on a coefficient with unknown sign.
    if let Some(pos) = work.iter().position(|(p, _)| {
        !is_constant_active(p, &[vi]) && as_rat(&linear_split(p, vi, vars).0).is_none()
    }) {
        let (p, r) = work[pos].clone();
        let (a, g) = linear_split(&p, vi, vars);
        let gv = poly_to_value(&g, vars);
        let isolate = |rel: Rel| -> Option<(VPoly, Rel)> {
            let bound = c_div(&c_neg(&gv), &a);
            let e = simplify_call("Plus", &[sym(&vars[vi]), c_neg(&bound)]);
            Some((parse_poly(&e, vars)?, rel))
        };
        let mut branches: Vec<Value> = Vec::new();
        if r == Rel::Equal {
            let mut modified = work.clone();
            modified[pos] = isolate(Rel::Equal)?;
            branches.push(and(vec![
                call("Unequal", vec![a.clone(), int_value(0)]),
                reduce_linear_rel(&modified, vars, active)?,
            ]));
            let mut modified0 = work.clone();
            modified0[pos] = (g.clone(), Rel::Equal);
            branches.push(and(vec![
                const_rel_value(&a, Rel::Equal),
                reduce_linear_rel(&modified0, vars, active)?,
            ]));
        } else {
            for sign in [Rel::Greater, Rel::Less, Rel::Equal] {
                let mut modified = work.clone();
                modified[pos] = if sign == Rel::Equal {
                    (g.clone(), r)
                } else {
                    let rel = if sign == Rel::Greater { r } else { rel_flip(r) };
                    isolate(rel)?
                };
                branches.push(and(vec![
                    const_rel_value(&a, sign),
                    reduce_linear_rel(&modified, vars, active)?,
                ]));
            }
        }
        parts.push(or(branches));
        return Some(and(parts));
    }
    let mut lower: Vec<(Value, bool)> = Vec::new();
    let mut upper: Vec<(Value, bool)> = Vec::new();
    let mut uneq: Vec<Value> = Vec::new();
    let mut residual: Vec<(VPoly, Rel)> = Vec::new();
    for (p, r) in &work {
        let (a, g) = linear_split(p, vi, vars);
        if is_zero_value(&a) {
            residual.push((g, *r));
            continue;
        }
        let ar = as_rat(&a)?;
        let bound = c_div(&c_neg(&poly_to_value(&g, vars)), &a);
        let rel = if ar.is_negative() { rel_flip(*r) } else { *r };
        match rel {
            Rel::Less => upper.push((bound, true)),
            Rel::LessEqual => upper.push((bound, false)),
            Rel::Greater => lower.push((bound, true)),
            Rel::GreaterEqual => lower.push((bound, false)),
            Rel::Unequal => uneq.push(bound),
            Rel::Equal => lower.push((bound, false)),
        }
    }
    // The upper/lower envelope exists iff every bound pair is consistent.
    for (l, ls) in &lower {
        for (u, us) in &upper {
            let diff = c_sub(l, u);
            let pr = parse_poly(&diff, vars)?;
            let r = if *ls || *us {
                Rel::Less
            } else {
                Rel::LessEqual
            };
            residual.push((pr, r));
        }
    }
    let sub = reduce_linear_rel(&residual, vars, &rest)?;
    let mut conds: Vec<Value> = parts;
    for (u, strict) in &upper {
        conds.push(call(
            if *strict { "Less" } else { "LessEqual" },
            vec![sym(&vars[vi]), u.clone()],
        ));
    }
    for (l, strict) in &lower {
        conds.push(call(
            if *strict { "Less" } else { "LessEqual" },
            vec![l.clone(), sym(&vars[vi])],
        ));
    }
    for b in &uneq {
        conds.push(call("Unequal", vec![sym(&vars[vi]), b.clone()]));
    }
    conds.push(sub);
    Some(and(conds))
}

// ── Multivariate reduction ──────────────────────────────────────────────────

fn reduce_conjuncts(conjuncts: &[Value], vars: &[String]) -> Option<Value> {
    if vars.is_empty() {
        for c in conjuncts {
            match c {
                Value::Bool(true) => {}
                Value::Bool(false) => return Some(Value::Bool(false)),
                _ => return Some(Value::Bool(false)),
            }
        }
        return Some(Value::Bool(true));
    }
    if vars.len() == 1 {
        return reduce_univariate(conjuncts, &vars[0]);
    }
    let active: Vec<usize> = (0..vars.len()).collect();
    let rels = parse_relations(conjuncts, vars);
    // Linear (in)equalities get a dedicated envelope-based reducer.
    if let Some(rels) = &rels {
        let all_linear = rels
            .iter()
            .all(|(p, _)| is_linear_active(p, &active) || p.is_zero());
        let has_ineq = rels.iter().any(|(_, r)| *r != Rel::Equal);
        if all_linear
            && has_ineq
            && let Some(v) = reduce_linear_rel(rels, vars, &active)
        {
            return Some(v);
        }
    }
    let mut polys: Vec<VPoly> = Vec::new();
    let mut polys_ok = true;
    for c in conjuncts {
        match c {
            Value::Bool(true) => {}
            Value::Bool(false) => return Some(Value::Bool(false)),
            Value::Call { head, args } if head == "Equal" && args.len() == 2 => {
                match (parse_poly(&args[0], vars), parse_poly(&args[1], vars)) {
                    (Some(a), Some(b)) => polys.push(a.minus(&b)),
                    _ => {
                        polys_ok = false;
                        break;
                    }
                }
            }
            _ => match parse_poly(c, vars) {
                Some(p) => polys.push(p),
                None => {
                    polys_ok = false;
                    break;
                }
            },
        }
    }
    if polys_ok && let Some(v) = reduce_active(&polys, vars, &active) {
        return Some(v);
    }
    // Fallback: systems that are multilinear in the variables (e.g. `x y == 1`)
    // can be solved by isolating one variable at a time.
    if let Some(rels) = rels
        && rels.iter().all(|(p, _)| is_multilinear(p, &active))
        && let Some(v) = reduce_linear_rel(&rels, vars, &active)
    {
        return Some(v);
    }
    None
}

/// True when every active variable occurs to power at most one in each term
/// (i.e. the polynomial is multilinear in the active variables).
fn is_multilinear(p: &VPoly, active: &[usize]) -> bool {
    p.0.keys().all(|m| active.iter().all(|&vi| m[vi] <= 1))
}

fn reduce_system_inputs(items: &[Value], vars: &[String]) -> Option<Value> {
    if let Some(pos) = items
        .iter()
        .position(|x| matches!(x, Value::Call { head, args } if head == "Or" && !args.is_empty()))
    {
        let mut branches: Vec<Value> = Vec::new();
        let rest: Vec<Value> = items
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != pos)
            .map(|(_, v)| v.clone())
            .collect();
        if let Value::Call { args, .. } = &items[pos] {
            for d in args {
                let mut combined = rest.clone();
                flatten_conjuncts(d, &mut combined);
                branches.push(reduce_system_inputs(&combined, vars)?);
            }
        }
        return Some(or(branches));
    }
    reduce_conjuncts(items, vars)
}

/// Rewrite `Abs[u] rel c` (with `c` a positive number) into equivalent
/// elementary relations so the polynomial machinery can consume them.
fn rewrite_abs(e: &Value) -> Value {
    match e {
        Value::Call { head, args } if head == "And" || head == "Or" => {
            call(head, args.iter().map(rewrite_abs).collect())
        }
        Value::Call { head, args } if args.len() == 2 => {
            let l = rewrite_abs(&args[0]);
            let r = rewrite_abs(&args[1]);
            match abs_relation(head, &l, &r) {
                Some(v) => v,
                None => call(head, vec![l, r]),
            }
        }
        Value::Call { head, args } => call(head, args.iter().map(rewrite_abs).collect()),
        _ => e.clone(),
    }
}

fn abs_relation(head: &str, l: &Value, r: &Value) -> Option<Value> {
    let (inner, c, flipped) = abs_split(l, r)?;
    let head = if flipped {
        flip_rel_head(head)
    } else {
        head.to_string()
    };
    let negc = c_neg(&c);
    let mk = |h: &str, a: Value, b: Value| call(h, vec![a, b]);
    match head.as_str() {
        "Less" => Some(and(vec![
            mk("Less", negc, inner.clone()),
            mk("Less", inner, c),
        ])),
        "LessEqual" => Some(and(vec![
            mk("LessEqual", negc, inner.clone()),
            mk("LessEqual", inner, c),
        ])),
        "Greater" => Some(or(vec![
            mk("Greater", inner.clone(), c),
            mk("Less", inner, negc),
        ])),
        "GreaterEqual" => Some(or(vec![
            mk("GreaterEqual", inner.clone(), c),
            mk("LessEqual", inner, negc),
        ])),
        "Equal" => Some(or(vec![
            mk("Equal", inner.clone(), c),
            mk("Equal", inner, negc),
        ])),
        _ => None,
    }
}

fn abs_split(l: &Value, r: &Value) -> Option<(Value, Value, bool)> {
    let abs_inner = |v: &Value| -> Option<Value> {
        if let Value::Call { head, args } = v
            && head == "Abs"
            && args.len() == 1
        {
            Some(args[0].clone())
        } else {
            None
        }
    };
    if let (Some(inner), Some(cv)) = (abs_inner(l), numeric_f64(r))
        && cv > 0.0
    {
        return Some((inner, r.clone(), false));
    }
    if let (Some(inner), Some(cv)) = (abs_inner(r), numeric_f64(l))
        && cv > 0.0
    {
        return Some((inner, l.clone(), true));
    }
    None
}

fn flip_rel_head(h: &str) -> String {
    match h {
        "Less" => "Greater",
        "LessEqual" => "GreaterEqual",
        "Greater" => "Less",
        "GreaterEqual" => "LessEqual",
        other => other,
    }
    .to_string()
}

fn reduce_value(e: &Value, vars: &[String]) -> Option<Value> {
    let rewritten = rewrite_abs(e);
    let e = &rewritten;
    match e {
        Value::Bool(true) => Some(Value::Bool(true)),
        Value::Bool(false) => Some(Value::Bool(false)),
        Value::Call { head, args } if head == "Or" && !args.is_empty() => {
            let mut branches: Vec<Value> = Vec::new();
            for a in args {
                branches.push(reduce_value(a, vars)?);
            }
            Some(or(branches))
        }
        _ => {
            let mut items: Vec<Value> = Vec::new();
            flatten_conjuncts(e, &mut items);
            reduce_system_inputs(&items, vars)
        }
    }
}

// ── Builtins ────────────────────────────────────────────────────────────────

/// GroebnerBasis[polys, vars] — reduced Groebner basis (lexicographic).
pub fn builtin_groebner_basis(args: &[Value]) -> Result<Value, EvalError> {
    let args = strip_args(args);
    if args.len() < 2 {
        return Ok(Value::Call {
            head: "GroebnerBasis".to_string(),
            args: args.clone(),
        });
    }
    let vars = match parse_vars(&args[1]) {
        Some(v) => v,
        None => {
            return Ok(Value::Call {
                head: "GroebnerBasis".to_string(),
                args: args.clone(),
            });
        }
    };
    let list: Vec<Value> = match &args[0] {
        Value::List(items) => items.clone(),
        other => vec![other.clone()],
    };
    let mut polys: Vec<VPoly> = Vec::new();
    for item in &list {
        match parse_poly(item, &vars) {
            Some(p) => polys.push(p),
            None => {
                return Ok(Value::Call {
                    head: "GroebnerBasis".to_string(),
                    args: args.clone(),
                });
            }
        }
    }
    let basis = match buchberger(&polys, vars.len()) {
        Some(b) => b,
        None => {
            return Ok(Value::Call {
                head: "GroebnerBasis".to_string(),
                args: args.clone(),
            });
        }
    };
    if basis.iter().any(|p| p.is_constant()) {
        return Ok(Value::List(vec![int_value(1)]));
    }
    Ok(Value::List(
        basis
            .iter()
            .map(|p| poly_to_value(&primitive(p), &vars))
            .collect(),
    ))
}

/// Roots[eqn, var] — disjunction of the roots of a univariate polynomial.
pub fn builtin_roots(args: &[Value]) -> Result<Value, EvalError> {
    let args = strip_args(args);
    if args.len() != 2 {
        return Ok(Value::Call {
            head: "Roots".to_string(),
            args: args.clone(),
        });
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Ok(Value::Call {
                head: "Roots".to_string(),
                args: args.clone(),
            });
        }
    };
    let vars = vec![var.clone()];
    let poly_expr = match &args[0] {
        Value::Call { head, args: ea } if head == "Equal" && ea.len() == 2 => {
            simplify_call("Plus", &[ea[0].clone(), c_neg(&ea[1])])
        }
        other => other.clone(),
    };
    let p = match parse_poly(&poly_expr, &vars) {
        Some(p) => p,
        None => {
            return Ok(Value::Call {
                head: "Roots".to_string(),
                args: args.clone(),
            });
        }
    };
    let coeffs = poly_coeffs_univariate(&p, 0);
    if !coeffs.iter().all(|c| as_rat(c).is_some()) {
        return Ok(Value::Call {
            head: "Roots".to_string(),
            args: args.clone(),
        });
    }
    if coeffs.iter().all(is_zero_value) {
        return Ok(Value::Bool(true));
    }
    let roots = match roots_of_values(&coeffs) {
        Some(r) => r,
        None => {
            return Ok(Value::Call {
                head: "Roots".to_string(),
                args: args.clone(),
            });
        }
    };
    if roots.is_empty() {
        return Ok(Value::Bool(false));
    }
    Ok(or(roots.into_iter().map(|r| equal(sym(&var), r)).collect()))
}

/// Reduce[expr, vars] — solve equations/inequalities, returning a condition.
pub fn builtin_reduce(args: &[Value]) -> Result<Value, EvalError> {
    let args = strip_args(args);
    if args.is_empty() {
        return Ok(Value::Call {
            head: "Reduce".to_string(),
            args: args.clone(),
        });
    }
    let vars = if args.len() >= 2 {
        match parse_vars(&args[1]) {
            Some(v) => v,
            None => {
                return Ok(Value::Call {
                    head: "Reduce".to_string(),
                    args: args.clone(),
                });
            }
        }
    } else {
        let mut syms = Vec::new();
        collect_symbols(&args[0], &mut syms);
        syms
    };
    match reduce_value(&args[0], &vars) {
        Some(v) => Ok(simplify_deep(&v)),
        None => Ok(Value::Call {
            head: "Reduce".to_string(),
            args: args.clone(),
        }),
    }
}

/// CylindricalDecomposition[expr, vars] — basic cylindrical decomposition.
pub fn builtin_cylindrical_decomposition(args: &[Value]) -> Result<Value, EvalError> {
    let args = strip_args(args);
    if args.is_empty() {
        return Ok(Value::Call {
            head: "CylindricalDecomposition".to_string(),
            args: args.clone(),
        });
    }
    let vars = if args.len() >= 2 {
        match parse_vars(&args[1]) {
            Some(v) => v,
            None => {
                return Ok(Value::Call {
                    head: "CylindricalDecomposition".to_string(),
                    args: args.clone(),
                });
            }
        }
    } else {
        let mut syms = Vec::new();
        collect_symbols(&args[0], &mut syms);
        syms
    };
    match reduce_value(&args[0], &vars) {
        Some(v) => Ok(simplify_deep(&v)),
        None => Ok(Value::Call {
            head: "CylindricalDecomposition".to_string(),
            args: args.clone(),
        }),
    }
}

/// Eliminate[eqns, vars] — eliminate variables from an equation system.
pub fn builtin_eliminate(args: &[Value]) -> Result<Value, EvalError> {
    let args = strip_args(args);
    if args.len() < 2 {
        return Ok(Value::Call {
            head: "Eliminate".to_string(),
            args: args.clone(),
        });
    }
    let elim = match parse_vars(&args[1]) {
        Some(v) => v,
        None => {
            return Ok(Value::Call {
                head: "Eliminate".to_string(),
                args: args.clone(),
            });
        }
    };
    let mut items: Vec<Value> = Vec::new();
    flatten_conjuncts(&args[0], &mut items);
    let equations = match equations_from(&items) {
        Some(e) => e,
        None => {
            return Ok(Value::Call {
                head: "Eliminate".to_string(),
                args: args.clone(),
            });
        }
    };
    let mut syms: Vec<String> = Vec::new();
    for e in &equations {
        collect_symbols(e, &mut syms);
    }
    let mut allvars = elim.clone();
    for s in syms {
        if !allvars.contains(&s) {
            allvars.push(s);
        }
    }
    let mut polys: Vec<VPoly> = Vec::new();
    for e in &equations {
        match parse_poly(e, &allvars) {
            Some(p) => polys.push(p),
            None => {
                return Ok(Value::Call {
                    head: "Eliminate".to_string(),
                    args: args.clone(),
                });
            }
        }
    }
    let basis = match buchberger(&polys, allvars.len()) {
        Some(b) => b,
        None => {
            return Ok(Value::Call {
                head: "Eliminate".to_string(),
                args: args.clone(),
            });
        }
    };
    if basis
        .iter()
        .any(|p| p.is_constant() && !p.all_constant_zero())
    {
        return Ok(Value::Bool(false));
    }
    let mut conds: Vec<Value> = Vec::new();
    for p in &basis {
        let depends_on_elim = elim.iter().any(|ev| {
            allvars
                .iter()
                .position(|v| v == ev)
                .map(|vi| p.involves_var(vi))
                .unwrap_or(false)
        });
        if !depends_on_elim && !p.is_constant() {
            conds.push(equal(poly_to_value(&primitive(p), &allvars), int_value(0)));
        }
    }
    Ok(and(conds))
}

fn equations_from(items: &[Value]) -> Option<Vec<Value>> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Value::Bool(true) => {}
            Value::Bool(false) => return Some(vec![int_value(1)]),
            Value::Call { head, args } if head == "Equal" && args.len() == 2 => {
                out.push(simplify_call("Plus", &[args[0].clone(), c_neg(&args[1])]));
            }
            other => out.push(other.clone()),
        }
    }
    Some(out)
}

/// SolveAlways[eqns, vars] — solve for parameters so eqns hold for all vars.
pub fn builtin_solve_always(args: &[Value]) -> Result<Value, EvalError> {
    let args = strip_args(args);
    if args.len() < 2 {
        return Ok(Value::Call {
            head: "SolveAlways".to_string(),
            args: args.clone(),
        });
    }
    let vars = match parse_vars(&args[1]) {
        Some(v) => v,
        None => {
            return Ok(Value::Call {
                head: "SolveAlways".to_string(),
                args: args.clone(),
            });
        }
    };
    let mut items: Vec<Value> = Vec::new();
    flatten_conjuncts(&args[0], &mut items);
    let equations = match equations_from(&items) {
        Some(e) => e,
        None => {
            return Ok(Value::Call {
                head: "SolveAlways".to_string(),
                args: args.clone(),
            });
        }
    };
    // Coefficient equations w.r.t. vars (parameters remain symbolic).
    let mut coeff_eqns: Vec<Value> = Vec::new();
    for e in &equations {
        let p = match parse_poly(e, &vars) {
            Some(p) => p,
            None => {
                return Ok(Value::Call {
                    head: "SolveAlways".to_string(),
                    args: args.clone(),
                });
            }
        };
        for c in p.0.values() {
            // A polynomial vanishes identically in `vars` iff each
            // coefficient (a function of the parameters only) vanishes; the
            // monomial factors are non-zero and can be dropped.
            coeff_eqns.push(c.clone());
        }
    }
    let mut syms: Vec<String> = Vec::new();
    for e in &coeff_eqns {
        collect_symbols(e, &mut syms);
    }
    let params: Vec<String> = syms.into_iter().filter(|s| !vars.contains(s)).collect();
    if params.is_empty() {
        for e in &coeff_eqns {
            let mut ps = Vec::new();
            collect_symbols(e, &mut ps);
            if is_zero_value(e) {
                continue;
            }
            return Ok(Value::List(vec![]));
        }
        return Ok(Value::List(vec![Value::List(vec![])]));
    }
    // Build a linear system in params.
    let active: Vec<usize> = (0..params.len()).collect();
    let mut a: Vec<Vec<Value>> = Vec::new();
    let mut b: Vec<Value> = Vec::new();
    for e in &coeff_eqns {
        let p = match parse_poly(e, &params) {
            Some(p) => p,
            None => {
                return Ok(Value::Call {
                    head: "SolveAlways".to_string(),
                    args: args.clone(),
                });
            }
        };
        if !is_linear_active(&p, &active) {
            return Ok(Value::Call {
                head: "SolveAlways".to_string(),
                args: args.clone(),
            });
        }
        let (row, constant) = match row_for_poly(&p, &active) {
            Some(r) => r,
            None => {
                return Ok(Value::Call {
                    head: "SolveAlways".to_string(),
                    args: args.clone(),
                });
            }
        };
        a.push(row);
        b.push(c_neg(&constant));
    }
    let sol = match rref_solve(a, b, &params) {
        Some(s) => s,
        None => return Ok(Value::List(vec![])),
    };
    let mut rules: Vec<Value> = Vec::new();
    for (k, name) in params.iter().enumerate() {
        if sol[k].struct_eq(&sym(name)) {
            continue;
        }
        rules.push(Value::Rule {
            lhs: Box::new(sym(name)),
            rhs: Box::new(sol[k].clone()),
            delayed: false,
        });
    }
    Ok(Value::List(vec![Value::List(rules)]))
}

/// FindInstance[eqns, vars, n] — find up to n (exact, where possible) solutions.
pub fn builtin_find_instance(args: &[Value]) -> Result<Value, EvalError> {
    let args = strip_args(args);
    if args.is_empty() || args.len() > 3 {
        return Ok(Value::Call {
            head: "FindInstance".to_string(),
            args: args.clone(),
        });
    }
    let n = if args.len() == 3 {
        match &args[2] {
            Value::Integer(i) => i.to_usize().unwrap_or(1).max(1),
            _ => 1,
        }
    } else {
        1
    };
    if let Value::Bool(true) = &args[0] {
        return Ok(Value::List(vec![Value::List(vec![])]));
    }
    if let Value::Bool(false) = &args[0] {
        return Ok(Value::List(vec![]));
    }
    let vars = if args.len() >= 2 {
        match parse_vars(&args[1]) {
            Some(v) => v,
            None => {
                return Ok(Value::Call {
                    head: "FindInstance".to_string(),
                    args: args.clone(),
                });
            }
        }
    } else {
        let mut syms = Vec::new();
        collect_symbols(&args[0], &mut syms);
        syms
    };
    let mut items: Vec<Value> = Vec::new();
    flatten_conjuncts(&args[0], &mut items);
    if let Some(pos) = items
        .iter()
        .position(|x| matches!(x, Value::Call { head, .. } if head == "Or"))
    {
        let mut rest: Vec<Value> = items
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != pos)
            .map(|(_, v)| v.clone())
            .collect();
        if let Value::Call { args: oargs, .. } = &items[pos] {
            for d in oargs {
                let combined = if rest.is_empty() {
                    d.clone()
                } else {
                    rest.push(d.clone());
                    call("And", rest.clone())
                };
                let mut newargs: Vec<Value> = vec![combined, args[1].clone()];
                if args.len() == 3 {
                    newargs.push(args[2].clone());
                }
                let r = builtin_find_instance(&newargs)?;
                if let Value::List(found) = &r
                    && !found.is_empty()
                {
                    return Ok(r);
                }
            }
        }
        return Ok(Value::List(vec![]));
    }
    let equations = match equations_from(&items) {
        Some(e) => e,
        None => {
            return Ok(Value::Call {
                head: "FindInstance".to_string(),
                args: args.clone(),
            });
        }
    };
    let mut polys: Vec<VPoly> = Vec::new();
    for e in &equations {
        match parse_poly(e, &vars) {
            Some(p) => polys.push(p),
            None => {
                return Ok(Value::Call {
                    head: "FindInstance".to_string(),
                    args: args.clone(),
                });
            }
        }
    }
    let active: Vec<usize> = (0..vars.len()).collect();
    let solutions = match solve_all(&polys, &vars, &active) {
        Some(s) => s,
        None => {
            return Ok(Value::Call {
                head: "FindInstance".to_string(),
                args: args.clone(),
            });
        }
    };
    if solutions.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let instances: Vec<Value> = solutions
        .into_iter()
        .take(n)
        .map(|sol| {
            let mut rules: Vec<Value> = Vec::new();
            for (vi, name) in vars.iter().enumerate() {
                let val = sol
                    .iter()
                    .find(|(idx, _)| *idx == vi)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| int_value(0));
                rules.push(Value::Rule {
                    lhs: Box::new(sym(name)),
                    rhs: Box::new(val),
                    delayed: false,
                });
            }
            Value::List(rules)
        })
        .collect();
    Ok(Value::List(instances))
}

fn solve_all(
    polys: &[VPoly],
    vars: &[String],
    active: &[usize],
) -> Option<Vec<Vec<(usize, Value)>>> {
    let mut ps: Vec<VPoly> = Vec::new();
    for p in polys {
        if p.is_zero() {
            continue;
        }
        if is_constant_active(p, active) {
            let total = p.0.values().fold(int_value(0), |acc, c| c_add(&acc, c));
            match approx_value(&total) {
                Some(ap) if ap.abs() < EPS => continue,
                Some(_) => return Some(vec![]),
                None => return Some(vec![]),
            }
        }
        ps.push(p.clone());
    }
    if ps.is_empty() {
        let sol: Vec<(usize, Value)> = active.iter().map(|&vi| (vi, int_value(0))).collect();
        return Some(vec![sol]);
    }
    if active.is_empty() {
        return Some(vec![]);
    }
    if ps.iter().all(|p| is_linear_active(p, active)) {
        let mut a: Vec<Vec<Value>> = Vec::new();
        let mut b: Vec<Value> = Vec::new();
        for p in &ps {
            let (row, constant) = row_for_poly(p, active)?;
            a.push(row);
            b.push(c_neg(&constant));
        }
        let names: Vec<String> = active.iter().map(|&vi| vars[vi].clone()).collect();
        let sol = match rref_solve(a, b, &names) {
            Some(s) => s,
            None => return Some(vec![]),
        };
        // Free variables come back as themselves; pin them to 0 and
        // back-substitute so every reported value is concrete.
        let mut vals = sol;
        for k in 0..names.len() {
            if vals[k].struct_eq(&sym(&names[k])) {
                for j in 0..vals.len() {
                    if j != k {
                        vals[j] = subst_symbol(&vals[j], &names[k], &int_value(0));
                    }
                }
                vals[k] = int_value(0);
            }
        }
        let out: Vec<(usize, Value)> = active
            .iter()
            .enumerate()
            .map(|(k, &vi)| (vi, simplify_deep(&vals[k])))
            .collect();
        return Some(vec![out]);
    }
    for &vi in active {
        if let Some(p) = ps
            .iter()
            .find(|p| is_univariate_active(p, active, vi) && !is_constant_active(p, active))
            .cloned()
        {
            let coeffs = poly_coeffs_univariate(&p, vi);
            let roots = roots_of_values(&coeffs)?;
            if roots.is_empty() {
                return Some(vec![]);
            }
            let sub_active: Vec<usize> = active.iter().copied().filter(|&x| x != vi).collect();
            let mut out: Vec<Vec<(usize, Value)>> = Vec::new();
            for r in roots {
                let newps: Vec<VPoly> = ps.iter().map(|q| substitute_poly(q, vi, &r)).collect();
                let subs = solve_all(&newps, vars, &sub_active)?;
                for mut s in subs {
                    s.push((vi, r.clone()));
                    out.push(s);
                }
            }
            return Some(out);
        }
    }
    let basis = buchberger(&ps, vars.len())?;
    for &vi in active.iter().rev() {
        if let Some(p) = basis
            .iter()
            .find(|p| is_univariate_active(p, active, vi) && !is_constant_active(p, active))
            .cloned()
        {
            let coeffs = poly_coeffs_univariate(&p, vi);
            let roots = roots_of_values(&coeffs)?;
            if roots.is_empty() {
                return Some(vec![]);
            }
            let sub_active: Vec<usize> = active.iter().copied().filter(|&x| x != vi).collect();
            let mut out: Vec<Vec<(usize, Value)>> = Vec::new();
            for r in roots {
                let newps: Vec<VPoly> = ps.iter().map(|q| substitute_poly(q, vi, &r)).collect();
                let subs = solve_all(&newps, vars, &sub_active)?;
                for mut s in subs {
                    s.push((vi, r.clone()));
                    out.push(s);
                }
            }
            return Some(out);
        }
    }
    None
}

/// Solve[eqns, vars] — exact solutions as a flat rule list whose right-hand
/// sides list every admissible value (so `var /. Solve[...]` yields the value
/// set, matching this engine's non-threading ReplaceAll).
pub fn builtin_solve(args: &[Value]) -> Result<Value, EvalError> {
    let a = strip_args(args);
    let uneval = |a: &[Value]| Value::Call {
        head: "Solve".to_string(),
        args: a.to_vec(),
    };
    if a.len() != 2 {
        return Ok(uneval(&a));
    }
    let vars = match parse_vars(&a[1]) {
        Some(v) if !v.is_empty() => v,
        _ => return Ok(uneval(&a)),
    };
    let mut items: Vec<Value> = Vec::new();
    flatten_conjuncts(&a[0], &mut items);
    if items.iter().any(|x| matches!(x, Value::Bool(false))) {
        return Ok(Value::List(vec![]));
    }
    if let Some(pos) = items
        .iter()
        .position(|x| matches!(x, Value::Call { head, args } if head == "Or" && !args.is_empty()))
    {
        let rest: Vec<Value> = items
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != pos)
            .map(|(_, v)| v.clone())
            .collect();
        let mut out: Vec<Value> = Vec::new();
        if let Value::Call { args: oargs, .. } = &items[pos] {
            for d in oargs {
                let mut combined = rest.clone();
                combined.push(d.clone());
                let e = if combined.len() == 1 {
                    combined.pop().unwrap()
                } else {
                    call("And", combined)
                };
                if let Value::List(found) = builtin_solve(&[e, a[1].clone()])? {
                    out.extend(found);
                }
            }
        }
        return Ok(Value::List(out));
    }
    let equations = match equations_from(&items) {
        Some(e) => e,
        None => return Ok(uneval(&a)),
    };
    let mut polys: Vec<VPoly> = Vec::new();
    for e in &equations {
        match parse_poly(e, &vars) {
            Some(p) => polys.push(p),
            None => return Ok(uneval(&a)),
        }
    }
    let active: Vec<usize> = (0..vars.len()).collect();
    let sols = match solve_all(&polys, &vars, &active) {
        Some(s) => s,
        None => return Ok(uneval(&a)),
    };
    Ok(solve_result_value(&sols, &vars))
}

/// Render solution vectors as `{v -> {values}}`, one rule for the first
/// variable (lists of vectors when the system has several solutions).
fn solve_result_value(sols: &[Vec<(usize, Value)>], vars: &[String]) -> Value {
    let value_for = |sol: &[(usize, Value)], i: usize| -> Value {
        sol.iter()
            .find(|(idx, _)| *idx == i)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| int_value(0))
    };
    let rhs = if vars.len() == 1 {
        Value::List(sols.iter().map(|s| value_for(s, 0)).collect())
    } else if sols.len() <= 1 {
        let vals: Vec<Value> = (0..vars.len())
            .map(|i| {
                sols.first()
                    .map(|s| value_for(s, i))
                    .unwrap_or_else(|| int_value(0))
            })
            .collect();
        Value::List(vals)
    } else {
        Value::List(
            sols.iter()
                .map(|s| Value::List((0..vars.len()).map(|i| value_for(s, i)).collect()))
                .collect(),
        )
    };
    Value::List(vec![Value::Rule {
        lhs: Box::new(sym(&vars[0])),
        rhs: Box::new(rhs),
        delayed: false,
    }])
}

/// SolveValues[eqns, vars] — the values of `vars` solving `eqns`, i.e. the
/// right-hand sides of `Solve`.
pub fn builtin_solve_values(args: &[Value]) -> Result<Value, EvalError> {
    let uneval = |a: &[Value]| Value::Call {
        head: "SolveValues".to_string(),
        args: a.to_vec(),
    };
    let a = strip_args(args);
    if a.len() != 2 {
        return Ok(uneval(&a));
    }
    let Some(vars) = parse_vars(&a[1]) else {
        return Ok(uneval(&a));
    };
    let sol = builtin_solve(&a)?;
    match sol {
        Value::Call { ref head, .. } if head == "Solve" => Ok(uneval(&a)),
        Value::Bool(_) => Ok(sol),
        Value::List(items) if items.is_empty() => Ok(Value::List(vec![])),
        Value::List(items) => {
            let mut out: Vec<Value> = Vec::new();
            for item in &items {
                let Value::Rule { rhs, .. } = item else {
                    return Ok(uneval(&a));
                };
                if vars.len() == 1 {
                    // rhs is already the list of values for the variable.
                    match &**rhs {
                        Value::List(vals) => out.extend(vals.iter().cloned()),
                        other => out.push(other.clone()),
                    }
                } else {
                    // rhs is either a single solution vector or a list of them.
                    match &**rhs {
                        Value::List(inner)
                            if inner
                                .first()
                                .map(|x| matches!(x, Value::List(_)))
                                .unwrap_or(false) =>
                        {
                            out.extend(inner.iter().cloned());
                        }
                        other => out.push(other.clone()),
                    }
                }
            }
            Ok(Value::List(out))
        }
        other => Ok(other),
    }
}

/// ReduceValues[expr, vars] — values of `vars` making `expr` true.
pub fn builtin_reduce_values(args: &[Value]) -> Result<Value, EvalError> {
    let uneval = |a: &[Value]| Value::Call {
        head: "ReduceValues".to_string(),
        args: a.to_vec(),
    };
    let a = strip_args(args);
    if a.len() != 2 {
        return Ok(uneval(&a));
    }
    let Some(vars) = parse_vars(&a[1]) else {
        return Ok(uneval(&a));
    };
    let red = builtin_reduce(&a)?;
    match red {
        Value::Call { ref head, .. } if head == "Reduce" => return Ok(uneval(&a)),
        Value::Bool(true) => return Ok(Value::List(vec![])),
        Value::Bool(false) => return Ok(Value::List(vec![])),
        _ => {}
    }
    let target = &vars[0];
    let mut out: Vec<Value> = Vec::new();
    collect_equal_values(&red, target, &mut out);
    if out.is_empty() {
        // Continuous range: `lo < x < hi` → `Interval[{lo, hi}]`.
        if let Some(bounds) = reduce_interval(&red, target) {
            return Ok(Value::List(vec![Value::Call {
                head: "Interval".to_string(),
                args: vec![Value::List(vec![bounds.0, bounds.1])],
            }]));
        }
        return Ok(uneval(&a));
    }
    Ok(Value::List(out))
}

/// Extract `(lo, hi)` from a condition of comparisons against `target`.
fn reduce_interval(cond: &Value, target: &str) -> Option<(Value, Value)> {
    let mut lo: Option<Value> = None;
    let mut hi: Option<Value> = None;
    let mut visit = |c: &Value| {
        let Value::Call { head, args } = c else {
            return;
        };
        let is_target = |v: &Value| matches!(v, Value::Symbol(s) if s == target);
        match (head.as_str(), args.len()) {
            // lo < x or lo <= x
            ("Less" | "LessEqual", 2) if is_target(&args[1]) => lo = Some(args[0].clone()),
            // x < hi or x <= hi
            ("Less" | "LessEqual", 2) if is_target(&args[0]) => hi = Some(args[1].clone()),
            // x > lo or x >= lo
            ("Greater" | "GreaterEqual", 2) if is_target(&args[0]) => lo = Some(args[1].clone()),
            // hi > x or hi >= x
            ("Greater" | "GreaterEqual", 2) if is_target(&args[1]) => hi = Some(args[0].clone()),
            ("Less" | "LessEqual", 3) if is_target(&args[1]) => {
                lo = Some(args[0].clone());
                hi = Some(args[2].clone());
            }
            _ => {}
        }
    };
    collect_conjuncts(cond, &mut visit);
    match (lo, hi) {
        (Some(lo), Some(hi)) => Some((lo, hi)),
        (Some(lo), None) => Some((lo, Value::Symbol("Infinity".to_string()))),
        (None, Some(hi)) => Some((
            Value::Call {
                head: "Times".to_string(),
                args: vec![
                    Value::Integer(rug::Integer::from(-1)),
                    Value::Symbol("Infinity".to_string()),
                ],
            },
            hi,
        )),
        (None, None) => None,
    }
}

fn collect_conjuncts(cond: &Value, f: &mut impl FnMut(&Value)) {
    match cond {
        Value::Call { head, args } if head == "And" || head == "List" => {
            for a in args {
                collect_conjuncts(a, f);
            }
        }
        other => f(other),
    }
}

/// Collect the right-hand sides of `Equal[target, v]` (either orientation)
/// appearing in a (possibly conjunctive) condition.
fn collect_equal_values(cond: &Value, target: &str, out: &mut Vec<Value>) {
    match cond {
        Value::Call { head, args } if head == "And" || head == "Or" || head == "List" => {
            for a in args {
                collect_equal_values(a, target, out);
            }
        }
        Value::Call { head, args } if head == "Equal" && args.len() == 2 => {
            if matches!(&args[0], Value::Symbol(s) if s == target) {
                out.push(args[1].clone());
            } else if matches!(&args[1], Value::Symbol(s) if s == target) {
                out.push(args[0].clone());
            }
        }
        _ => {}
    }
}

/// Resolve[expr, vars] — basic quantifier elimination.
pub fn builtin_resolve(args: &[Value]) -> Result<Value, EvalError> {
    let args = strip_args(args);
    if args.is_empty() {
        return Ok(Value::Call {
            head: "Resolve".to_string(),
            args: args.clone(),
        });
    }
    match resolve_value(&args[0]) {
        Some(v) => Ok(v),
        None => Ok(Value::Call {
            head: "Resolve".to_string(),
            args: args.clone(),
        }),
    }
}

fn resolve_value(e: &Value) -> Option<Value> {
    match e {
        Value::Call { head, args } if head == "Exists" && args.len() == 2 => {
            resolve_quantifier(true, &args[0], &args[1])
        }
        Value::Call { head, args } if head == "ForAll" && args.len() == 2 => {
            resolve_quantifier(false, &args[0], &args[1])
        }
        Value::Call { head, args } if head == "And" => {
            let mut out = Vec::new();
            for a in args {
                out.push(resolve_value(a)?);
            }
            Some(and(out))
        }
        Value::Call { head, args } if head == "Or" => {
            let mut out = Vec::new();
            for a in args {
                out.push(resolve_value(a)?);
            }
            Some(or(out))
        }
        _ => Some(e.clone()),
    }
}

fn resolve_quantifier(exists: bool, vars_expr: &Value, body: &Value) -> Option<Value> {
    let elim = parse_vars(vars_expr)?;
    let mut items: Vec<Value> = Vec::new();
    flatten_conjuncts(body, &mut items);
    let equations = equations_from(&items)?;
    let mut syms: Vec<String> = Vec::new();
    for e in &equations {
        collect_symbols(e, &mut syms);
    }
    let mut allvars = elim.clone();
    for s in syms {
        if !allvars.contains(&s) {
            allvars.push(s);
        }
    }
    let mut polys: Vec<VPoly> = Vec::new();
    for e in &equations {
        polys.push(parse_poly(e, &allvars)?);
    }
    if exists {
        let basis = buchberger(&polys, allvars.len())?;
        if basis
            .iter()
            .any(|p| p.is_constant() && !p.all_constant_zero())
        {
            return Some(Value::Bool(false));
        }
        let mut conds: Vec<Value> = Vec::new();
        for p in &basis {
            let depends = elim.iter().any(|ev| {
                allvars
                    .iter()
                    .position(|v| v == ev)
                    .map(|vi| p.involves_var(vi))
                    .unwrap_or(false)
            });
            if !depends && !p.is_constant() {
                conds.push(equal(poly_to_value(&primitive(p), &allvars), int_value(0)));
            }
        }
        Some(and(conds))
    } else {
        // ForAll: every coefficient w.r.t. elim must vanish.
        let mut conds: Vec<Value> = Vec::new();
        for p in &polys {
            for (mono, c) in &p.0 {
                let mut coeff = c.clone();
                for (i, v) in allvars.iter().enumerate() {
                    if !elim.contains(v) && mono[i] > 0 {
                        let pw = c_pow(&sym(v), mono[i] as i64)?;
                        coeff = c_mul(&coeff, &pw);
                    }
                }
                if !is_zero_value(&coeff) {
                    conds.push(equal(coeff, int_value(0)));
                }
            }
        }
        Some(and(conds))
    }
}

/// ToRules[expr] — convert a logical combination of equations into rules.
pub fn builtin_to_rules(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(Value::Call {
            head: "ToRules".to_string(),
            args: args.to_vec(),
        });
    }
    let v = match &args[0] {
        Value::Pattern(e) => held_to_value(e, env)?,
        other => other.clone(),
    };
    Ok(to_rules_value(&v))
}

fn held_to_value(expr: &Expr, env: &Env) -> Result<Value, EvalError> {
    match expr {
        Expr::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for i in items {
                out.push(held_to_value(i, env)?);
            }
            Ok(Value::List(out))
        }
        Expr::Call { head, args } => {
            let hname = match head.as_ref() {
                Expr::Symbol(s) => Some(norm_head(s)),
                _ => None,
            };
            if let Some(h) = hname
                && matches!(
                    h,
                    "And"
                        | "Or"
                        | "Equal"
                        | "Unequal"
                        | "Less"
                        | "Greater"
                        | "LessEqual"
                        | "GreaterEqual"
                        | "Not"
                )
            {
                let mut out = Vec::with_capacity(args.len());
                for a in args {
                    out.push(held_to_value(a, env)?);
                }
                return Ok(Value::Call {
                    head: h.to_string(),
                    args: out,
                });
            }
            crate::eval::eval(expr, env)
        }
        _ => crate::eval::eval(expr, env),
    }
}

fn to_rules_value(v: &Value) -> Value {
    match v {
        Value::Call { head, args } if head == "Equal" && args.len() == 2 => Value::Rule {
            lhs: Box::new(args[0].clone()),
            rhs: Box::new(args[1].clone()),
            delayed: false,
        },
        Value::Call { head, args } if (head == "And" || head == "Or") && !args.is_empty() => {
            let mut rules: Vec<Value> = Vec::new();
            for a in args {
                match to_rules_value(a) {
                    Value::List(items) => rules.extend(items),
                    other => rules.push(other),
                }
            }
            Value::List(rules)
        }
        Value::List(items) => Value::List(items.iter().map(to_rules_value).collect()),
        other => other.clone(),
    }
}

/// RootSum[f, form] — sum of form over the roots of the polynomial f.
pub fn builtin_root_sum(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(Value::Call {
            head: "RootSum".to_string(),
            args: args.to_vec(),
        });
    }
    let uneval = || Value::Call {
        head: "RootSum".to_string(),
        args: args.to_vec(),
    };
    let t = sym("$RootSumVar");
    let fapp = crate::eval::apply_function(&args[0], std::slice::from_ref(&t), env)?;
    let formapp = crate::eval::apply_function(&args[1], std::slice::from_ref(&t), env)?;
    let vars = vec!["$RootSumVar".to_string()];
    let fpoly = match parse_poly(&fapp, &vars) {
        Some(p) => p,
        None => return Ok(uneval()),
    };
    let coeffs = poly_coeffs_univariate(&fpoly, 0);
    if !coeffs.iter().all(|c| as_rat(c).is_some()) {
        return Ok(uneval());
    }
    let rats: Vec<Rational> = coeffs.iter().map(|c| as_rat(c).unwrap()).collect();
    let deg = {
        let mut d = rats.len() - 1;
        while d > 0 && rats[d].is_zero() {
            d -= 1;
        }
        if rats[d].is_zero() {
            return Ok(int_value(0));
        }
        d
    };
    // Try polynomial form for `form`.
    if let Some(fp) = parse_poly(&formapp, &vars) {
        let form_coeffs = poly_coeffs_univariate(&fp, 0);
        if form_coeffs.iter().all(|c| as_rat(c).is_some()) {
            let fc: Vec<Rational> = form_coeffs.iter().map(|c| as_rat(c).unwrap()).collect();
            let powers = newton_sums(&rats, deg, fc.len());
            let mut total = Rational::from(0);
            for (j, c) in fc.iter().enumerate() {
                if let Some(p) = powers.get(j) {
                    total += c.clone() * p.clone();
                }
            }
            return Ok(rat_to_value(&total));
        }
    }
    // Numeric fallback: sum form(root) over all (possibly complex) roots.
    let rats: Vec<Rational> = match coeffs.iter().map(as_rat).collect::<Option<Vec<_>>>() {
        Some(r) => r,
        None => return Ok(uneval()),
    };
    let croots = polynomial::find_polynomial_roots(&rats);
    let mut sum_re = 0.0;
    let mut sum_im = 0.0;
    for (re, im) in croots {
        let rv = Value::Complex { re, im };
        let val = crate::eval::apply_function(&args[1], std::slice::from_ref(&rv), env)?;
        match complex_approx(&val) {
            Some((vr, vi)) => {
                sum_re += vr;
                sum_im += vi;
            }
            None => return Ok(uneval()),
        }
    }
    if sum_im.abs() < EPS {
        Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, sum_re)))
    } else {
        Ok(Value::Complex {
            re: sum_re,
            im: sum_im,
        })
    }
}

/// Numeric real/imaginary parts of a (possibly complex) value.
fn complex_approx(v: &Value) -> Option<(f64, f64)> {
    match v {
        Value::Integer(n) => Some((n.to_f64(), 0.0)),
        Value::Rational(r) => Some((r.to_f64(), 0.0)),
        Value::Real(x) => Some((x.to_f64(), 0.0)),
        Value::Complex { re, im } => Some((*re, *im)),
        Value::Symbol(s) if s == "I" => Some((0.0, 1.0)),
        Value::Call { head, args } => match head.as_str() {
            "Plus" => {
                let mut re = 0.0;
                let mut im = 0.0;
                for a in args {
                    let (x, y) = complex_approx(a)?;
                    re += x;
                    im += y;
                }
                Some((re, im))
            }
            "Minus" if args.len() == 1 => {
                let (x, y) = complex_approx(&args[0])?;
                Some((-x, -y))
            }
            "Times" => {
                let mut re = 1.0;
                let mut im = 0.0;
                for a in args {
                    let (x, y) = complex_approx(a)?;
                    let nr = re * x - im * y;
                    let ni = re * y + im * x;
                    re = nr;
                    im = ni;
                }
                Some((re, im))
            }
            "Divide" if args.len() == 2 => {
                let (a, b) = complex_approx(&args[0])?;
                let (c, d) = complex_approx(&args[1])?;
                let den = c * c + d * d;
                if den == 0.0 {
                    return None;
                }
                Some(((a * c + b * d) / den, (b * c - a * d) / den))
            }
            "Power" if args.len() == 2 => {
                let base = complex_approx(&args[0])?;
                let e = complex_approx(&args[1])?;
                Some(complex_pow(base, e))
            }
            "Sqrt" if args.len() == 1 => {
                let (a, b) = complex_approx(&args[0])?;
                let m = (a * a + b * b).sqrt();
                Some((
                    ((m + a) / 2.0).sqrt(),
                    (((m - a) / 2.0).sqrt()) * b.signum(),
                ))
            }
            _ => None,
        },
        _ => approx_value(v).map(|x| (x, 0.0)),
    }
}

fn complex_pow(base: (f64, f64), e: (f64, f64)) -> (f64, f64) {
    let (br, bi) = base;
    if e.1 == 0.0 && e.0.fract() == 0.0 {
        let n = e.0 as i64;
        if n == 0 {
            return (1.0, 0.0);
        }
        let mut re = 1.0;
        let mut im = 0.0;
        for _ in 0..n.abs() {
            let nr = re * br - im * bi;
            let ni = re * bi + im * br;
            re = nr;
            im = ni;
        }
        if n < 0 {
            let den = re * re + im * im;
            (re / den, -im / den)
        } else {
            (re, im)
        }
    } else {
        let mag = (br * br + bi * bi).sqrt();
        let arg = bi.atan2(br);
        let ln = mag.ln();
        let r = (e.0 * ln - e.1 * arg).exp();
        let theta = e.0 * arg + e.1 * ln;
        (r * theta.cos(), r * theta.sin())
    }
}

/// Newton power sums p_0..p_(max) of the roots of a polynomial.
fn newton_sums(coeffs: &[Rational], deg: usize, max: usize) -> Vec<Rational> {
    // monic coefficients a_j = c_j / c_deg, j = 0..deg
    let lead = coeffs[deg].clone();
    let a: Vec<Rational> = (0..=deg)
        .map(|j| coeffs[j].clone() / lead.clone())
        .collect();
    let n = deg;
    let mut p: Vec<Rational> = vec![Rational::from(0); max + 1];
    if max >= 1 {
        p[0] = Rational::from(n as i64);
    }
    for k in 1..=max {
        let mut acc = Rational::from(0);
        if k <= n {
            for j in 1..k {
                // a_{n-j} * p_{k-j}
                acc += a[n - j].clone() * p[k - j].clone();
            }
            acc += Rational::from(k as i64) * a[n - k].clone();
            p[k] = -acc;
        } else {
            for j in 1..=n {
                acc += a[n - j].clone() * p[k - j].clone();
            }
            p[k] = -acc;
        }
    }
    p
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    use crate::builtins::{register_builtin, register_builtin_env};

    register_builtin(env, "GroebnerBasis", builtin_groebner_basis);
    register_builtin(env, "Roots", builtin_roots);
    register_builtin(env, "Reduce", builtin_reduce);
    register_builtin(env, "Solve", builtin_solve);
    register_builtin(
        env,
        "CylindricalDecomposition",
        builtin_cylindrical_decomposition,
    );
    register_builtin(env, "Eliminate", builtin_eliminate);
    register_builtin(env, "SolveAlways", builtin_solve_always);
    register_builtin(env, "SolveValues", builtin_solve_values);
    register_builtin(env, "ReduceValues", builtin_reduce_values);
    register_builtin(env, "FindInstance", builtin_find_instance);
    register_builtin(env, "Resolve", builtin_resolve);
    register_builtin_env(env, "ToRules", builtin_to_rules);
    register_builtin_env(env, "RootSum", builtin_root_sum);

    for name in [
        "GroebnerBasis",
        "Roots",
        "Reduce",
        "Solve",
        "SolveValues",
        "ReduceValues",
        "CylindricalDecomposition",
        "Eliminate",
        "SolveAlways",
        "FindInstance",
        "Resolve",
        "ToRules",
    ] {
        env.set_attributes(name, vec!["HoldAll".to_string(), "Protected".to_string()]);
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use crate::env::Env;
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

    #[test]
    fn test_groebner_basis_quadratic() {
        let out = eval_str("GroebnerBasis[{x^2 + y^2 - 1, x - y}, {x, y}]").to_string();
        assert!(out.contains("x"), "got {out}");
        assert!(out.contains("y"), "got {out}");
        // The elimination polynomial 2 y^2 - 1 must appear.
        assert!(out.contains("2"), "got {out}");
    }

    #[test]
    fn test_groebner_basis_ideal_is_one() {
        let out = eval_str("GroebnerBasis[{x, x - 1}, {x}]").to_string();
        assert_eq!(out, "{1}");
    }

    #[test]
    fn test_reduce_linear_system() {
        let out = eval_str("Reduce[{x + y == 3, x - y == 1}, {x, y}]").to_string();
        assert!(out.contains("Equal[x, 2]"), "got {out}");
        assert!(out.contains("Equal[y, 1]"), "got {out}");
    }

    #[test]
    fn test_reduce_quadratic() {
        let out = eval_str("Reduce[x^2 - 3 x + 2 == 0, x]").to_string();
        assert!(out.contains("Equal[x, 1]"), "got {out}");
        assert!(out.contains("Equal[x, 2]"), "got {out}");
    }

    #[test]
    fn test_reduce_inequality() {
        let out = eval_str("Reduce[x^2 - 3 x + 2 < 0, x]").to_string();
        assert!(out.contains("Less[1, x]"), "got {out}");
        assert!(out.contains("Less[x, 2]"), "got {out}");
    }

    #[test]
    fn test_reduce_identity_and_false() {
        assert_eq!(eval_str("Reduce[x == x, x]"), Value::Bool(true));
        assert_eq!(eval_str("Reduce[1 == 2, x]"), Value::Bool(false));
    }

    #[test]
    fn test_roots_quadratic() {
        let out = eval_str("Roots[x^2 - 5 x + 6, x]").to_string();
        assert!(out.contains("Equal[x, 2]"), "got {out}");
        assert!(out.contains("Equal[x, 3]"), "got {out}");
    }

    #[test]
    fn test_roots_irrational() {
        let out = eval_str("Roots[x^2 - 2, x]").to_string();
        assert!(out.contains("Sqrt"), "got {out}");
    }

    #[test]
    fn test_to_rules_from_reduce() {
        let out = eval_str("ToRules[Reduce[x^2 == 4, x]]").to_string();
        assert!(out.contains("x -> -2"), "got {out}");
        assert!(out.contains("x -> 2"), "got {out}");
    }

    #[test]
    fn test_to_rules_and() {
        let out = eval_str("ToRules[x == 1 && y == 2]").to_string();
        assert!(out.contains("x -> 1"), "got {out}");
        assert!(out.contains("y -> 2"), "got {out}");
    }

    #[test]
    fn test_solve_always() {
        let out = eval_str("SolveAlways[a x^2 + b x + c == x^2 + 1, x]").to_string();
        assert!(out.contains("a -> 1"), "got {out}");
        assert!(out.contains("b -> 0"), "got {out}");
        assert!(out.contains("c -> 1"), "got {out}");
    }

    #[test]
    fn test_eliminate() {
        let out = eval_str("Eliminate[{x + y == 1, x - y == 0}, x]").to_string();
        assert!(out.contains("y"), "got {out}");
        assert!(!out.contains("x"), "x should be eliminated: {out}");
    }

    #[test]
    fn test_resolve_exists_true() {
        assert_eq!(eval_str("Resolve[Exists[x, x^2 == 4]]"), Value::Bool(true));
    }

    #[test]
    fn test_resolve_forall_false() {
        assert_eq!(eval_str("Resolve[ForAll[x, x^2 == x]]"), Value::Bool(false));
    }

    #[test]
    fn test_find_instance_quadratic() {
        let out = eval_str("FindInstance[x^2 == 4, x]");
        match out {
            Value::List(items) => assert_eq!(items.len(), 1),
            other => panic!("expected list, got {other}"),
        }
    }

    #[test]
    fn test_find_instance_inconsistent() {
        assert_eq!(
            eval_str("FindInstance[{x == 1, x == 2}, x]"),
            Value::List(vec![])
        );
    }

    #[test]
    fn test_root_sum_vieta() {
        assert_eq!(eval_str("RootSum[#^2 - 3 # + 2 &, # &]").to_string(), "3");
    }

    #[test]
    fn test_root_sum_power() {
        assert_eq!(eval_str("RootSum[#^2 - 3 # + 2 &, #^2 &]").to_string(), "5");
    }

    #[test]
    fn test_cylindrical_decomposition() {
        let out = eval_str("CylindricalDecomposition[x^2 - 3 x + 2 < 0, x]").to_string();
        assert!(out.contains("Less[1, x]"), "got {out}");
        assert!(out.contains("Less[x, 2]"), "got {out}");
    }

    #[test]
    fn test_reduce_symbolic_param() {
        let out = eval_str("Reduce[a x == 1, x]").to_string();
        assert!(out.contains("Equal[x, Divide[1, a]]"), "got {out}");
        assert_eq!(eval_str("Reduce[a == 0, x]").to_string(), "Equal[a, 0]");
    }

    #[test]
    fn test_reduce_nonlinear_system() {
        let out = eval_str("Reduce[x^2 + y^2 == 1 && y == x, {x, y}]").to_string();
        assert!(out.contains("Sqrt[2]"), "got {out}");
        assert!(!out.contains("0.7"), "expected exact roots, got {out}");
    }

    #[test]
    fn test_reduce_or_and_chain() {
        assert_eq!(
            eval_str("ToRules[x == 1 || x == 2]").to_string(),
            "{x -> 1, x -> 2}"
        );
        let chained = eval_str("Reduce[1 < x < 2, x]").to_string();
        assert!(chained.contains("Less[1, x]"), "got {chained}");
        assert!(chained.contains("Less[x, 2]"), "got {chained}");
    }

    #[test]
    fn test_root_sum_reciprocal() {
        assert_eq!(eval_str("RootSum[#^2 + 1 &, 1/# &]").to_string(), "0");
    }

    #[test]
    fn test_find_instance_or() {
        let out = eval_str("FindInstance[x == 1 || x == 2, x]");
        match out {
            Value::List(items) => assert_eq!(items.len(), 1),
            other => panic!("expected list, got {other}"),
        }
    }

    #[test]
    fn test_unevaluated_fallback() {
        let out = eval_str("Reduce[Sin[x] == 0, x]");
        match out {
            Value::Call { head, .. } => assert_eq!(head, "Reduce"),
            other => panic!("expected unevaluated, got {other}"),
        }
    }

    #[test]
    fn test_reduce_multivar_linear_ineq() {
        let out = eval_str("Reduce[x + y < 1, {x, y}]").to_string();
        assert!(out.contains("Less[x,"), "got {out}");
    }

    #[test]
    fn test_reduce_multivar_linear_system_free() {
        // Under-determined system: pivot expressed in terms of the free var.
        let out = eval_str("Reduce[x + y == 1, {x, y}]").to_string();
        assert!(out.contains('y'), "got {out}");
        assert!(!out.contains("Equal[x, 1]"), "free var dropped: {out}");
    }

    #[test]
    fn test_reduce_two_ineq_system() {
        let out = eval_str("Reduce[{x + y > 1, x - y < 2}, {x, y}]").to_string();
        assert!(out.contains("Less") || out.contains("Greater"), "got {out}");
        assert!(!out.contains("Reduce["), "unreduced: {out}");
    }

    #[test]
    fn test_reduce_symbolic_linear_ineq() {
        let out = eval_str("Reduce[a x < 1, x]").to_string();
        assert!(out.contains('a'), "got {out}");
        assert!(out.contains("Less") || out.contains("Greater"), "got {out}");
        assert_ne!(out, "Reduce[a x < 1, x]", "got {out}");
    }

    #[test]
    fn test_solve_univariate_roots() {
        let out = eval_str("x /. Solve[x^2 - 1 == 0, x]").to_string();
        assert!(out.contains("-1") && out.contains('1'), "got {out}");
    }

    #[test]
    fn test_solve_system_vector() {
        let out = eval_str("x /. Solve[{x + y == 3, x - y == 1}, {x, y}][[1]]").to_string();
        assert!(out.contains('2') && out.contains('1'), "got {out}");
    }

    #[test]
    fn test_solve_always_linear_param_no_denominator() {
        let out = eval_str("SolveAlways[a x + b == 1, x]").to_string();
        assert!(out.contains("a -> 0"), "got {out}");
        assert!(out.contains("b -> 1"), "got {out}");
    }

    #[test]
    fn test_reduce_multivar_linear_system_determined() {
        let out = eval_str("Reduce[{x + y == 3, x - y == 1}, {x, y}]").to_string();
        assert!(out.contains("Equal[x, 2]"), "got {out}");
        assert!(out.contains("Equal[y, 1]"), "got {out}");
    }

    #[test]
    fn test_reduce_abs_inequality() {
        let out = eval_str("Reduce[Abs[x] < 1, x]").to_string();
        assert!(out.contains("Less[-1, x]"), "got {out}");
        assert!(out.contains("Less[x, 1]"), "got {out}");
        let out2 = eval_str("Reduce[Abs[x] > 2, x]").to_string();
        assert!(
            out2.contains("Less[2, x]") || out2.contains("Less[x, -2]"),
            "got {out2}"
        );
    }

    #[test]
    fn test_reduce_multilinear_reciprocal() {
        let out = eval_str("Reduce[x y == 1, {x, y}]").to_string();
        assert!(out.contains("Equal[x, Divide[1, y]]"), "got {out}");
    }
}
