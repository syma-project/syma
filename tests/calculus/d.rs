use super::*;

// ── Basic rules ──────────────────────────────────────────────────────────────

#[test]
fn test_d_constant_int() {
    assert!(syma_eval("D[5, x]").contains("0"));
}

#[test]
fn test_d_constant_float() {
    let out = syma_eval("D[3.14, x]");
    assert!(out.contains("0"), "D[3.14, x] should be 0, got: {out}");
}

#[test]
fn test_d_identity() {
    assert!(syma_eval("D[x, x]").contains("1"));
}

#[test]
fn test_d_different_symbol() {
    let out = syma_eval("D[y, x]");
    assert!(out.contains("0"), "D[y, x] should be 0, got: {out}");
}

#[test]
fn test_d_linear_coefficient() {
    let out = syma_eval("D[3*x, x]");
    assert!(out.contains("3"), "D[3*x, x] should be 3, got: {out}");
}

#[test]
fn test_d_linear_symbolic_coefficient() {
    let out = syma_eval("D[a*x, x]");
    assert!(out.contains("a"), "D[a*x, x] should contain a, got: {out}");
}

// ── Power rule ───────────────────────────────────────────────────────────────

#[test]
fn test_d_power_integer() {
    let out = syma_eval("D[x^3, x]");
    contains_all(&out, &["3", "x"], "D[x^3, x]");
}

#[test]
fn test_d_power_square() {
    let out = syma_eval("D[x^2, x]");
    assert!(out.contains("2"), "D[x^2, x] should be 2*x, got: {out}");
}

#[test]
fn test_d_power_of_sum() {
    let out = syma_eval("D[(x + 1)^3, x]");
    contains_all(&out, &["3", "x"], "D[(x+1)^3, x]");
}

// ── Sum rule ─────────────────────────────────────────────────────────────────

#[test]
fn test_d_sum_two_terms() {
    let out = syma_eval("D[x^2 + 3*x, x]");
    contains_all(&out, &["2", "3"], "D[x^2+3*x, x]");
}

#[test]
fn test_d_sum_poly_trig() {
    let out = syma_eval("D[x^3 + Sin[x], x]");
    contains_all(&out, &["3", "Cos"], "D[x^3+Sin[x], x]");
}

#[test]
fn test_d_sum_constant_expr() {
    let out = syma_eval("D[x^2 + 5, x]");
    assert!(
        out.contains("2"),
        "D[x^2+5, x] should contain 2, got: {out}"
    );
    assert!(
        !out.contains("5"),
        "D[x^2+5, x] should not contain 5, got: {out}"
    );
}

// ── Product rule ─────────────────────────────────────────────────────────────

#[test]
fn test_d_product_sin_x() {
    let out = syma_eval("D[Sin[x]*x, x]");
    contains_all(&out, &["Cos", "Sin"], "D[Sin[x]*x, x]");
}

#[test]
fn test_d_product_exp_x() {
    let out = syma_eval("D[Exp[x]*x, x]");
    contains_all(&out, &["Exp", "x"], "D[Exp[x]*x, x]");
}

#[test]
fn test_d_product_two_trig() {
    let out = syma_eval("D[Sin[x]*Cos[x], x]");
    contains_all(&out, &["Cos", "Sin"], "D[Sin[x]*Cos[x], x]");
}

// ── Chain rule ───────────────────────────────────────────────────────────────

#[test]
fn test_d_chain_sin_power() {
    let out = syma_eval("D[Sin[x^2], x]");
    assert!(
        out.contains("Cos"),
        "D[Sin[x^2], x] should contain Cos, got: {out}"
    );
}

#[test]
fn test_d_chain_exp_sin() {
    let out = syma_eval("D[Exp[Sin[x]], x]");
    contains_all(&out, &["Exp", "Cos"], "D[Exp[Sin[x]], x]");
}

#[test]
fn test_d_chain_sin_cos() {
    let out = syma_eval("D[Sin[Cos[x]], x]");
    assert!(
        out.contains("Cos"),
        "D[Sin[Cos[x]], x] should contain Cos, got: {out}"
    );
}

#[test]
fn test_d_chain_log_power() {
    let out = syma_eval("D[Log[x^2], x]");
    assert!(
        out.contains("x") || out.contains("Power"),
        "D[Log[x^2], x] should contain x or Power, got: {out}"
    );
}

#[test]
fn test_d_chain_sqrt_arg() {
    let out = syma_eval("D[Sqrt[x^2 + 1], x]");
    assert!(
        out.contains("Sqrt") || out.contains("Power"),
        "D[Sqrt[x^2+1], x] should contain Sqrt or Power, got: {out}"
    );
}

#[test]
fn test_d_chain_unknown_function() {
    let out = syma_eval("D[f[g[x]], x]");
    assert!(
        !out.contains("error") && !out.contains("Error"),
        "D[f[g[x]], x] should not error, got: {out}"
    );
}

// ── Elementary functions ────────────────────────────────────────────────────

#[test]
fn test_d_sin() {
    assert!(syma_eval("D[Sin[x], x]").contains("Cos"));
}

#[test]
fn test_d_sin_2x() {
    let out = syma_eval("D[Sin[2*x], x]");
    contains_all(&out, &["Cos", "2"], "D[Sin[2*x], x]");
}

#[test]
fn test_d_cos() {
    assert!(syma_eval("D[Cos[x], x]").contains("Sin"));
}

#[test]
fn test_d_tan() {
    let out = syma_eval("D[Tan[x], x]");
    assert!(
        out.contains("Cos"),
        "D[Tan[x], x] should contain Cos, got: {out}"
    );
}

#[test]
fn test_d_exp() {
    assert!(syma_eval("D[Exp[x], x]").contains("Exp"));
}

#[test]
fn test_d_exp_3x() {
    let out = syma_eval("D[Exp[3*x], x]");
    contains_all(&out, &["Exp", "3"], "D[Exp[3*x], x]");
}

#[test]
fn test_d_log() {
    let out = syma_eval("D[Log[x], x]");
    assert!(
        out.contains("Power") || out.contains("^"),
        "D[Log[x], x] should be x^-1, got: {out}"
    );
}

#[test]
fn test_d_sqrt() {
    let out = syma_eval("D[Sqrt[x], x]");
    assert!(
        out.contains("Sqrt") || out.contains("Power"),
        "D[Sqrt[x], x] should contain Sqrt or Power, got: {out}"
    );
}

#[test]
fn test_d_arcsin() {
    let out = syma_eval("D[ArcSin[x], x]");
    assert!(
        !out.contains("error") && !out.contains("Error"),
        "D[ArcSin[x], x] should not error, got: {out}"
    );
}

#[test]
fn test_d_arccos() {
    let out = syma_eval("D[ArcCos[x], x]");
    assert!(
        !out.contains("error") && !out.contains("Error"),
        "D[ArcCos[x], x] should not error, got: {out}"
    );
}

#[test]
fn test_d_arctan() {
    let out = syma_eval("D[ArcTan[x], x]");
    assert!(
        out.contains("x") || out.contains("Power"),
        "D[ArcTan[x], x] should contain x or Power, got: {out}"
    );
}

// ── Higher-order derivatives ────────────────────────────────────────────────

#[test]
fn test_d_second_derivative() {
    let out = syma_eval("D[D[x^3, x], x]");
    contains_all(&out, &["6", "x"], "D[D[x^3, x], x]");
}

#[test]
fn test_d_third_derivative() {
    let out = syma_eval("D[D[D[x^3, x], x], x]");
    assert!(out.contains("6"), "D^3[x^3, x] should be 6, got: {out}");
}

#[test]
fn test_d_fourth_derivative_x4() {
    let out = syma_eval("d1 = D[x^4, x]; d2 = D[d1, x]; d3 = D[d2, x]; D[d3, x]");
    assert!(out.contains("24"), "D^4[x^4, x] should be 24, got: {out}");
}

#[test]
fn test_d_second_derivative_sin() {
    let out = syma_eval("D[D[Sin[x], x], x]");
    assert!(
        out.contains("Sin"),
        "D^2[Sin[x], x] should contain Sin, got: {out}"
    );
}

// ── Compound expressions ────────────────────────────────────────────────────

#[test]
fn test_d_sin_squared() {
    let out = syma_eval("D[Sin[x]^2, x]");
    contains_all(&out, &["Sin", "Cos", "2"], "D[Sin[x]^2, x]");
}

#[test]
fn test_d_exp_minus_x() {
    let out = syma_eval("D[Exp[-x], x]");
    assert!(
        out.contains("Exp"),
        "D[Exp[-x], x] should contain Exp, got: {out}"
    );
}

#[test]
fn test_d_reciprocal() {
    let out = syma_eval("D[Power[x, -1], x]");
    assert!(
        out.contains("Power") || out.contains("^"),
        "D[Power[x,-1], x] should contain Power, got: {out}"
    );
}

#[test]
fn test_d_polynomial_terms() {
    let out = syma_eval("D[3*x^3, x]");
    contains_all(&out, &["9", "x"], "D[3*x^3, x]");
    let out2 = syma_eval("D[2*x^2, x]");
    contains_all(&out2, &["4", "x"], "D[2*x^2, x]");
}

// ── Multi-term and multi-factor expressions ─────────────────────────────────

#[test]
fn test_d_polynomial_full() {
    let out = syma_eval("D[3*x^3 + 2*x^2 + 5*x + 7, x]");
    contains_all(&out, &["9", "4", "5"], "D[3*x^3+2*x^2+5*x+7, x]");
}

#[test]
fn test_d_triple_product() {
    let out = syma_eval("D[x*Sin[x]*Exp[x], x]");
    contains_all(&out, &["Sin", "Cos", "Exp"], "D[x*Sin[x]*Exp[x], x]");
}

#[test]
fn test_d_nested_power_trig() {
    let out = syma_eval("D[Cos[x]^3, x]");
    contains_all(&out, &["Cos", "Sin", "3"], "D[Cos[x]^3, x]");
}

// ── Edge cases ──────────────────────────────────────────────────────────────

#[test]
fn test_d_constant_in_expr() {
    let out = syma_eval("D[5*x^2, x]");
    contains_all(&out, &["10", "x"], "D[5*x^2, x]");
}

#[test]
fn test_d_zero_power() {
    let out = syma_eval("D[x^0, x]");
    assert!(
        out.contains("0") || out.contains("x"),
        "D[x^0, x] should be 0, got: {out}"
    );
}

#[test]
fn test_d_negative_power() {
    let out = syma_eval("D[x^(-2), x]");
    assert!(
        out.contains("2") && (out.contains("Power") || out.contains("^")),
        "D[x^(-2), x] should contain 2 and Power, got: {out}"
    );
}

// ── n-th order derivatives: D[f, {x, n}] ─────────────────────────────────────

#[test]
fn test_d_nth_order_0() {
    let out = syma_eval("D[x^3, {x, 0}]");
    assert!(
        out.contains("x"),
        "D[x^3, {{x, 0}}] should be x^3, got: {out}"
    );
}

#[test]
fn test_d_nth_order_1() {
    let out = syma_eval("D[x^3, {x, 1}]");
    contains_all(&out, &["3", "x"], "D[x^3, {x, 1}]");
}

#[test]
fn test_d_nth_order_2() {
    let out = syma_eval("D[x^3, {x, 2}]");
    contains_all(&out, &["6", "x"], "D[x^3, {x, 2}]");
}

#[test]
fn test_d_nth_order_3() {
    let out = syma_eval("D[x^3, {x, 3}]");
    assert!(
        out.contains("6"),
        "D[x^3, {{x, 3}}] should be 6, got: {}",
        out
    );
}

#[test]
fn test_d_nth_order_4_x4() {
    let out = syma_eval("D[x^4, {x, 4}]");
    assert!(
        out.contains("24"),
        "D[x^4, {{x, 4}}] should be 24, got: {}",
        out
    );
}

#[test]
fn test_d_nth_order_sin() {
    let out = syma_eval("D[Sin[x], {x, 4}]");
    assert!(
        out.contains("Sin"),
        "D^4[Sin[x], x] should be Sin[x], got: {out}"
    );
}

#[test]
fn test_d_nth_order_exp() {
    let out = syma_eval("D[Exp[x], {x, 5}]");
    assert!(
        out.contains("Exp"),
        "D^5[Exp[x], x] should be Exp[x], got: {out}"
    );
}

// ── Mixed partial derivatives: D[f, x, y] ───────────────────────────────────

#[test]
fn test_d_mixed_two_vars() {
    let out = syma_eval("D[x^2 * y, x, y]");
    contains_all(&out, &["2", "x"], "D[x^2*y, x, y]");
}

#[test]
fn test_d_mixed_reversed() {
    let out = syma_eval("D[x^2 * y, y, x]");
    contains_all(&out, &["2", "x"], "D[x^2*y, y, x]");
}

#[test]
fn test_d_mixed_three_vars() {
    let out = syma_eval("D[x*y*z, x, y, z]");
    assert!(
        out.contains("1"),
        "D[x*y*z, x, y, z] should be 1, got: {out}"
    );
}

#[test]
fn test_d_mixed_with_nth() {
    let out = syma_eval("D[x^3*y, {x, 2}, y]");
    contains_all(&out, &["6", "x"], "D[x^3*y, {x, 2}, y]");
}
