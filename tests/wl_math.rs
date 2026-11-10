//! Mathematica parity conformance corpus for *mathematical* computation.
//!
//! Each case asserts `Max[Abs[N[lhs - rhs]]] < tol`, i.e. lhs and rhs agree
//! numerically. These are the hard correctness targets for the "math depth"
//! workstreams. Failing cases are the backlog.

#[path = "common/mod.rs"]
mod common;
use common::*;

/// Assert two expressions agree (numeric first, else `Simplify[... ] == 0`).
fn eq(expr: &str, expected: &str) {
    // Numeric path (no Max, so a symbolic result doesn't error).
    let d = syma_eval(&format!("N[({expr}) - ({expected})]"));
    let dt = d.trim();
    if let Ok(v) = dt.parse::<f64>() {
        assert!(v.abs() < 1e-8, "`{expr}` != `{expected}` (residual {v})");
        return;
    }
    if dt.starts_with('{') {
        let inner = dt.trim_matches(|c| c == '{' || c == '}');
        let mut all_numeric = true;
        for tok in inner.split(',') {
            match tok.trim().parse::<f64>() {
                Ok(v) if v.abs() < 1e-8 => {}
                _ => {
                    all_numeric = false;
                    break;
                }
            }
        }
        if all_numeric {
            return;
        }
    }
    // Symbolic path: require the simplified difference to be all zeros.
    let s = syma_eval(&format!("Simplify[({expr}) - ({expected})]"));
    let stripped: String = s.chars().filter(|c| !" {},".contains(*c)).collect();
    assert!(
        !stripped.is_empty() && stripped.chars().all(|c| c == '0'),
        "`{expr}` != `{expected}` (simplified residual `{s}`)"
    );
}

// ── differentiation ──
#[test]
fn d_basic() {
    eq("D[x^3, x]", "3 x^2");
    eq("D[Sin[x], x]", "Cos[x]");
    eq("D[x^2 y, x]", "2 x y");
    eq("D[Log[x], x]", "1/x");
    eq("D[Exp[x], x]", "Exp[x]");
    eq("D[x^5, x, 2]", "20 x^3");
    eq("D[Tan[x], x]", "Sec[x]^2");
    eq("D[Cos[x^2], x]", "-2 x Sin[x^2]");
}

// ── indefinite integration ──
#[test]
fn integrate_indefinite() {
    eq("D[Integrate[x^2, x], x]", "x^2");
    eq("D[Integrate[Sin[x], x], x]", "Sin[x]");
    eq("D[Integrate[Cos[x], x], x]", "Cos[x]");
    eq("D[Integrate[1/x, x], x]", "1/x");
    eq("D[Integrate[Exp[x], x], x]", "Exp[x]");
    eq("D[Integrate[x Exp[x], x], x]", "x Exp[x]");
    eq("D[Integrate[1/(1 + x^2), x], x]", "1/(1 + x^2)");
    eq("D[Integrate[Log[x], x], x]", "Log[x]");
}

// ── definite integration ──
#[test]
fn integrate_definite() {
    eq("Integrate[Sin[x], {x, 0, Pi}]", "2");
    eq("Integrate[x^2, {x, 0, 1}]", "1/3");
    eq("Integrate[1/(1 + x^2), {x, 0, 1}]", "Pi/4");
    eq("Integrate[Exp[-x], {x, 0, Infinity}]", "1");
    eq("Integrate[x Exp[-x], {x, 0, Infinity}]", "1");
}

// ── limits ──
#[test]
fn limits() {
    eq("Limit[Sin[x]/x, x -> 0]", "1");
    eq("Limit[(1 + 1/n)^n, n -> Infinity]", "E");
    eq("Limit[Exp[x], x -> -Infinity]", "0");
    eq("Limit[Log[x], x -> Infinity]", "Infinity");
    eq("Limit[(x^2 - 1)/(x - 1), x -> 1]", "2");
}

// ── series ──
#[test]
fn series_coeffs() {
    eq("SeriesCoefficient[Exp[x], {x, 0, 3}]", "1/6");
    eq("SeriesCoefficient[Sin[x], {x, 0, 5}]", "1/120");
    eq("SeriesCoefficient[1/(1 - x), {x, 0, n}]", "1");
}

// ── sums / products ──
#[test]
fn sums_products() {
    eq("Sum[k, {k, 1, n}]", "n (n + 1)/2");
    eq("Sum[k^2, {k, 1, n}]", "n (n + 1) (2 n + 1)/6");
    eq("Sum[1/2^k, {k, 1, Infinity}]", "1");
    eq("Product[k, {k, 1, 5}]", "120");
}

// ── solve / reduce ──
#[test]
fn solve_basic() {
    eq("x /. Solve[x^2 - 1 == 0, x]", "{-1, 1}");
    eq("x /. Solve[x^2 - 4 == 0, x]", "{-2, 2}");
    eq("x /. Solve[a x + b == 0, x]", "{-b/a}");
    eq(
        "x /. Solve[{x + y == 3, x - y == 1}, {x, y}][[1]]",
        "{2, 1}",
    );
}

// ── simplification ──
#[test]
fn simplify_basic() {
    eq("Simplify[Sin[x]^2 + Cos[x]^2]", "1");
    eq("FullSimplify[(x^2 - 1)/(x - 1)]", "x + 1");
    eq("Simplify[(x^2 + 2 x + 1)/(x + 1)]", "x + 1");
    eq("Simplify[Cos[2 x] - (Cos[x]^2 - Sin[x]^2)]", "0");
    eq("Expand[(x + 1)^3]", "x^3 + 3 x^2 + 3 x + 1");
    eq("Factor[x^2 - 1]", "(x - 1) (x + 1)");
    eq("Together[1/x + 1/y]", "(x + y)/(x y)");
}

// ── assumptions / refine ──
#[test]
fn refine_assumptions() {
    eq("Simplify[Sqrt[x^2], Assumptions -> x > 0]", "x");
    eq("Refine[Sqrt[x^2], x > 0]", "x");
    eq("Refine[Abs[x], x > 0]", "x");
}

// ── special functions (exact values) ──
#[test]
fn special_values() {
    eq("Gamma[5]", "24");
    eq("Gamma[1/2]", "Sqrt[Pi]");
    eq("Beta[2, 3]", "1/12");
    eq("Zeta[2]", "Pi^2/6");
    eq("Zeta[4]", "Pi^4/90");
    eq("Zeta[0]", "-1/2");
    eq("Erf[0]", "0");
    eq("PolyLog[2, 1]", "Pi^2/6");
    eq("BesselJ[1/2, x]", "Sqrt[2/(Pi x)] Sin[x]");
    eq("AiryAi[0]", "1/(3^(2/3) Gamma[2/3])");
    eq("HarmonicNumber[Infinity]", "Infinity");
}

// ── number theory ──
#[test]
fn number_theory() {
    eq("EulerPhi[10]", "4");
    eq("DivisorSigma[1, 6]", "12");
    eq("DivisorSigma[0, 12]", "6");
    eq("MoebiusMu[30]", "-1");
    eq("PrimePi[100]", "25");
    eq("NextPrime[100]", "101");
    eq("GCD[12, 18]", "6");
    eq("LCM[4, 6]", "12");
    eq("PowerMod[2, 10, 1000]", "24");
    eq("Binomial[10, 3]", "120");
    eq("CatalanNumber[5]", "42");
    eq("Fibonacci[10]", "55");
}

// ── linear algebra ──
#[test]
fn linear_algebra() {
    eq("Det[{{1, 2}, {3, 4}}]", "-2");
    eq("Tr[{{1, 2}, {3, 4}}]", "5");
    eq("Inverse[{{1, 2}, {3, 4}}]", "{{-2, 1}, {3/2, -1/2}}");
    eq("LinearSolve[{{2, 0}, {0, 4}}, {2, 8}]", "{1, 2}");
    eq("MatrixPower[{{1, 1}, {0, 1}}, 3]", "{{1, 3}, {0, 1}}");
    eq("Eigenvalues[{{2, 0}, {0, 3}}]", "{2, 3}");
    eq("Norm[{3, 4}]", "5");
}

// ── numeric ──
#[test]
fn numeric_accuracy() {
    eq("N[Pi, 20]", "3.14159265358979323846");
    eq("N[Sqrt[2], 20]", "1.41421356237309504880");
    eq("N[E, 20]", "2.71828182845904523536");
    eq("NIntegrate[Sin[x], {x, 0, Pi}]", "2");
    eq(
        "FindRoot[Cos[x] == x, {x, 1}][[1, 2]]",
        "0.7390851332151606416",
    );
}
