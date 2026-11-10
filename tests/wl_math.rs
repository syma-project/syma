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
    // Exponential / logarithmic dominance over polynomials.
    eq("Limit[Exp[x]/x, x -> Infinity]", "Infinity");
    eq("Limit[Log[x]/x, x -> Infinity]", "0");
    eq("Limit[x^2/Exp[x], x -> Infinity]", "0");
    eq("Limit[x^n/E^x, x -> Infinity]", "0");
    eq("Limit[E^x/x^n, x -> Infinity]", "Infinity");
    // (c + w)^v family and iterated L'Hôpital.
    eq("Limit[(1 + a/x)^x, x -> Infinity]", "E^a");
    eq("Limit[(Cos[x] - 1)/x^2, x -> 0]", "-1/2");
    // One-sided limits.
    eq("Limit[1/x, x -> 0, Direction -> 1]", "Infinity");
    eq("Limit[1/x, x -> 0, Direction -> -1]", "-Infinity");
}

// ── series ──
#[test]
fn series_coeffs() {
    eq("SeriesCoefficient[Exp[x], {x, 0, 3}]", "1/6");
    eq("SeriesCoefficient[Sin[x], {x, 0, 5}]", "1/120");
    eq("SeriesCoefficient[1/(1 - x), {x, 0, n}]", "1");
    eq("SeriesCoefficient[Sin[x]/x, {x, 0, 2}]", "-1/6");
    eq("SeriesCoefficient[Sin[x]/x, {x, 0, 4}]", "1/120");
    eq("SeriesCoefficient[Exp[x], {x, 0, 5}]", "1/120");
    eq("SeriesCoefficient[1/(1 - x), {x, 0, 5}]", "1");
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
    eq("Simplify[Cos[x]^2 - Sin[x]^2]", "Cos[2 x]");
    eq("Simplify[2 Sin[x] Cos[x]]", "Sin[2 x]");
    eq("Simplify[1 - Sin[x]^2]", "Cos[x]^2");
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
    eq("Simplify[Sqrt[x^2], Assumptions -> x < 0]", "-x");
    eq("FullSimplify[Sqrt[x^2], x > 0]", "x");
    eq("Assuming[x > 0, Simplify[Sqrt[x^2]]]", "x");
    eq("Assuming[x < 0, FullSimplify[Abs[x]]]", "-x");
    eq("Refine[Sqrt[x^2], x > 0]", "x");
    eq("Refine[Sqrt[x^2], x < 0]", "-x");
    eq("Refine[Abs[x], x > 0]", "x");
    eq("Refine[Log[Exp[x]], Element[x, Reals]]", "x");
    // Without assumptions these must not collapse.
    eq("Simplify[Sqrt[x^2]]", "Sqrt[x^2]");
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
    eq("Eigenvalues[{{2, 0}, {0, 3}}]", "{3, 2}");
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

/// String/structural equality helper.
fn eqs(expr: &str, expected: &str) {
    let a = syma_eval(expr);
    let b = syma_eval(expected);
    assert_eq!(a, b, "`{expr}` != `{expected}`");
}

// ── elementary math ──
#[test]
fn math_elementary() {
    eq("Mod[10, 3]", "1");
    eq("Quotient[10, 3]", "3");
    eq("GCD[24, 36]", "12");
    eq("Floor[-2.5]", "-3");
    eq("Ceiling[-2.5]", "-2");
    eq("Sign[-3]", "-1");
    eq("Max[3, 1, 2]", "3");
    eq("Total[{1, 2, 3}]", "6");
    eq("Sqrt[16]", "4");
    eq("2^10", "1024");
    eq("Log[1]", "0");
    eq("Sin[0]", "0");
}

// ── number theory ──
#[test]
fn number_theory_x() {
    eq("Prime[10]", "29");
    eq("NextPrime[100]", "101");
    eq("EulerPhi[10]", "4");
    eq("MoebiusMu[30]", "-1");
    eq("DivisorSigma[1, 6]", "12");
    eq("FromDigits[{1, 2, 3}]", "123");
    eq("PrimePi[100]", "25");
    eq("Divisors[12]", "{1, 2, 3, 4, 6, 12}");
    eq("IntegerDigits[1234]", "{1, 2, 3, 4}");
}

// ── combinatorics ──
#[test]
fn combinatorics_x() {
    eq("Binomial[10, 3]", "120");
    eq("CatalanNumber[5]", "42");
    eq("Fibonacci[10]", "55");
    eq("Factorial[5]", "120");
    eq("Multinomial[2, 3]", "10");
    eq("StirlingS2[5, 2]", "15");
    eq("PartitionsP[5]", "7");
}

// ── statistics ──
#[test]
fn statistics_basic() {
    eq("Mean[{1, 2, 3, 4}]", "5/2");
    eq("Median[{1, 2, 3, 4, 5}]", "3");
    eq("Variance[{1, 2, 3}]", "1");
    eq("StandardDeviation[{1, 2, 3}]", "1");
    eq("Quantile[{1, 2, 3, 4}, 0.5]", "5/2");
}

// ── probability ──
#[test]
fn probability_basic() {
    eq("PDF[NormalDistribution[0, 1], 0]", "0.3989422804014327");
    eq("CDF[NormalDistribution[0, 1], 0]", "1/2");
    eq("Mean[NormalDistribution[2, 3]]", "2");
    eq("Variance[NormalDistribution[2, 3]]", "9");
    eq("Probability[x > 0, NormalDistribution[0, 1]]", "0.5");
}

// ── integral transforms ──
#[test]
fn transforms_laplace() {
    eq("LaplaceTransform[t^2, t, s]", "2/s^3");
    eq("LaplaceTransform[Sin[a t], t, s]", "a/(a^2 + s^2)");
    eq("LaplaceTransform[Exp[-a t], t, s]", "1/(s + a)");
    eq("InverseLaplaceTransform[1/s^2, s, t]", "t");
}

// ── hypergeometric ──
#[test]
fn hypergeometric_values() {
    eq("Hypergeometric1F1[1, 2, 1]", "E - 1");
    eq("Hypergeometric2F1[1, 1, 2, 1/2]", "2 Log[2]");
    eq("Hypergeometric0F1[1, 1]", "BesselI[0, 2]");
}

// ── optimization ──
#[test]
fn optimize_basic() {
    eq("First[FindMinimum[(x - 2)^2, {x, 0}]]", "0");
    eq("First[Minimize[x^2 - 2 x, x]]", "-1");
    eq("First[Maximize[-(x - 1)^2 + 3, x]]", "3");
}

// ── strings ──
#[test]
fn strings_basic() {
    eqs("StringJoin[\"a\", \"b\"]", "\"ab\"");
    eqs("StringLength[\"abc\"]", "3");
    eqs("StringReverse[\"abc\"]", "\"cba\"");
    eqs("ToUpperCase[\"abc\"]", "\"ABC\"");
    eqs("StringTake[\"abcde\", 3]", "\"abc\"");
    eqs("StringCount[\"abcabc\", \"bc\"]", "2");
}

// ── graph basics ──
#[test]
fn graph_basic() {
    eq("VertexCount[Graph[{1 <-> 2, 2 <-> 3}]]", "3");
    eq("EdgeCount[Graph[{1 <-> 2, 2 <-> 3}]]", "2");
    eq("GraphDistance[Graph[{1 <-> 2, 2 <-> 3}], 1, 3]", "2");
    eq(
        "Length[ConnectedComponents[Graph[{1 <-> 2, 3 <-> 4}]]]",
        "2",
    );
    // Complement of a 2-path on 3 vertices has exactly the missing edge.
    eq(
        "VertexCount[GraphComplement[Graph[{1 <-> 2, 2 <-> 3}]]]",
        "3",
    );
    eq("EdgeCount[GraphComplement[Graph[{1 <-> 2, 2 <-> 3}]]]", "1");
    // A spanning tree of the triangle keeps 3 vertices and 2 edges.
    eq(
        "EdgeCount[FindSpanningTree[Graph[{1 <-> 2, 2 <-> 3, 3 <-> 1}]]]",
        "2",
    );
}

// ── linear algebra: decompositions ──
#[test]
fn linalg_decompositions() {
    // Cholesky: M == L . Transpose[L].
    eq(
        "Norm[{{4, 1}, {1, 3}} - Dot[CholeskyDecomposition[{{4, 1}, {1, 3}}], Transpose[CholeskyDecomposition[{{4, 1}, {1, 3}}]]]]",
        "0",
    );
    // QR: M == q . r.
    eq(
        "Norm[{{1, 2}, {3, 4}} - (QRDecomposition[{{1, 2}, {3, 4}}][[1]] . QRDecomposition[{{1, 2}, {3, 4}}][[2]])]",
        "0",
    );
    // SVD: M == u . DiagonalMatrix[s] . Transpose[v].
    eq(
        "Norm[{{1, 2}, {3, 4}} - Dot[Dot[SingularValueDecomposition[{{1, 2}, {3, 4}}][[1]], DiagonalMatrix[SingularValueDecomposition[{{1, 2}, {3, 4}}][[2]]]], Transpose[SingularValueDecomposition[{{1, 2}, {3, 4}}][[3]]]]]",
        "0",
    );
    eq("Adjugate[{{1, 2}, {3, 4}}]", "{{4, -2}, {-3, 1}}");
    eq(
        "ConjugateTranspose[{{1, 2 I}, {3, 4}}]",
        "{{1, 3}, {-2 I, 4}}",
    );
    eq("Dimensions[LeviCivitaTensor[3]]", "{3, 3, 3}");
    // Fully antisymmetric: even/odd permutations of (1,2,3) / repeated index.
    eq("LeviCivitaTensor[3][[1, 2, 3]]", "1");
    eq("LeviCivitaTensor[3][[2, 3, 1]]", "1");
    eq("LeviCivitaTensor[3][[2, 1, 3]]", "-1");
    eq("LeviCivitaTensor[3][[1, 1, 2]]", "0");
}

// ── statistics extras ──
#[test]
fn stats_extra() {
    eq("SurvivalFunction[ExponentialDistribution[2], 0]", "1");
    eq("Commonest[{1, 2, 2, 3, 3}]", "{2, 3}");
    eq("SpearmanRho[{1, 2, 3}, {1, 2, 3}]", "1");
    eq("SpearmanRho[{1, 2, 3}, {3, 2, 1}]", "-1");
}

// ── combinatorics extras ──
#[test]
fn combinatorics_extra() {
    eq("BellNumber[4]", "15");
    eq("BellB[5]", "52");
    eq("Derangements[4]", "9");
    eq("EulerE[10]", "-50521");
    eq("PermutationSignature[{2, 3, 1}]", "1");
    eq("Signature[{1, 3, 2}]", "-1");
    eq("Signature[{1, 2, 3}]", "1");
}

// ── special functions extras ──
#[test]
fn special_extra() {
    eq("ChebyshevT[3, 1/2]", "-1");
    eq("ChebyshevT[4, 1/2]", "-1/2");
    eq("ProductLog[1]", "0.5671432904097838");
    // Y_1^0(θ, φ) is proportional to Cos[θ], which vanishes at θ = π/2.
    eq("N[SphericalHarmonicY[1, 0, Pi/2, 0]]", "0");
    eq("AiryAiPrime[0]", "-1/3^(1/3)/Gamma[1/3]");
}

// ── units ──
#[test]
fn units_extra() {
    eq(
        "QuantityMagnitude[UnitConvert[Quantity[1, \"Mile\"], \"Kilometer\"]]",
        "1.609344",
    );
    eq(
        "QuantityMagnitude[UnitConvert[Quantity[0, \"Celsius\"], \"Kelvin\"]]",
        "273.15",
    );
    eq(
        "QuantityMagnitude[UnitConvert[Quantity[1, \"Hour\"], \"Second\"]]",
        "3600",
    );
}

// ── strings extras ──
#[test]
fn strings_extra() {
    eqs("LowerCase[\"ABC\", 2]", "\"AbC\"");
    eqs("ToTitleCase[\"hello wORLD\"]", "\"Hello World\"");
    // `bc` occurs twice in `abcabc` (positions {2,3} and {5,6}).
    eq("Length[StringPosition[\"abcabc\", \"bc\"]]", "2");
    eqs("StringPosition[\"abcabc\", \"bc\"]", "{{2, 3}, {5, 6}}");
}

// ── associations extras ──
#[test]
fn assoc_extra() {
    eq("Lookup[<|\"a\" -> 1|>, \"a\"]", "1");
    eqs("Values[<|\"x\" -> 1|>]", "{1}");
    // Counts keys are stored in canonical string form, so numeric keys are
    // looked up by value (`Lookup[..., 1]` finds the `"1"` key).
    eq("Lookup[Counts[{1, 1, 2}], 1]", "2");
    eq("Lookup[Counts[{1, 1, 2}], 2]", "1");
    eq("Length[Counts[{1, 1, 2}]]", "2");
}

// ── calculus extras ──
#[test]
fn calculus_extra() {
    eq("SeriesCoefficient[1/(1 - x), {x, 0, n}]", "1");
    // InverseSeries of Sin: x + x^3/6 + 3 x^5/40 + O[x]^6.
    eq(
        "SeriesCoefficient[InverseSeries[Series[Sin[x], {x, 0, 5}]], {x, 0, 3}]",
        "1/6",
    );
    eq(
        "SeriesCoefficient[InverseSeries[Series[Sin[x], {x, 0, 5}]], {x, 0, 5}]",
        "3/40",
    );
    eqs(
        "Normal[InverseSeries[Series[Sin[x], {x, 0, 5}]]]",
        "x + 1/6 x^3 + 3/40 x^5",
    );
    eq("LaplaceTransform[t^3, t, s]", "6/s^4");
    eq("ZTransform[a^n, n, z]", "z/(z - a)");
}
