/// Symbolic manipulation builtin integration tests

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── Simplify ──

#[test]
fn test_simplify_numeric() {
    let out = syma_eval("Simplify[2 + 2]");
    assert!(out.contains("4"), "Simplify[2+2] should be 4, got: {out}");
}

#[test]
fn test_simplify_linear() {
    let out = syma_eval("Simplify[x + 2*x]");
    assert!(
        !out.contains("error"),
        "Simplify[x+2x] should not error, got: {out}"
    );
}

// ── Expand ──

#[test]
fn test_expand_square() {
    let out = syma_eval("Expand[(x + 1)^2]");
    assert!(
        !out.contains("error"),
        "Expand[(x+1)^2] should not error, got: {out}"
    );
}

#[test]
fn test_expand_cube() {
    let out = syma_eval("Expand[(x + y)^3]");
    assert!(
        !out.contains("error"),
        "Expand[(x+y)^3] should not error, got: {out}"
    );
}

// ── Factor ──

#[test]
fn test_factor_difference_squares() {
    let out = syma_eval("Factor[x^2 - 1]");
    assert!(
        !out.contains("error"),
        "Factor[x^2-1] should not error, got: {out}"
    );
}

// ── Solve ──

#[test]
fn test_solve_linear() {
    let out = syma_eval("Solve[x + 2 == 5, x]");
    assert!(
        out.contains("x"),
        "Solve[x+2==5,x] should mention x, got: {out}"
    );
}

#[test]
fn test_solve_quadratic() {
    let out = syma_eval("Solve[x^2 == 4, x]");
    assert!(
        !out.contains("error"),
        "Solve[x^2==4,x] should not error, got: {out}"
    );
}

// ── Coefficient ──

#[test]
fn test_coefficient_quadratic() {
    let out = syma_eval("Coefficient[3*x^2 + 2*x + 1, x, 2]");
    assert!(out.contains("3"), "Coefficient should be 3, got: {out}");
}

#[test]
fn test_coefficient_linear() {
    let out = syma_eval("Coefficient[x^3 + 2*x, x, 1]");
    assert!(out.contains("2"), "Coefficient should be 2, got: {out}");
}

// ── Collect ──

#[test]
fn test_collect_basic() {
    let out = syma_eval("Collect[a*x + b*x, x]");
    assert!(
        !out.contains("error"),
        "Collect should not error, got: {out}"
    );
}

// ── Apart / Together ──

#[test]
fn test_apart_basic() {
    let out = syma_eval("Apart[1/(x*(x+1)), x]");
    assert!(!out.contains("error"), "Apart should not error, got: {out}");
}

#[test]
fn test_together_basic() {
    let out = syma_eval("Together[1/x + 1/(x+1)]");
    assert!(
        !out.contains("error"),
        "Together should not error, got: {out}"
    );
}

// ── Cancel ──

#[test]
fn test_cancel_basic() {
    let out = syma_eval("Cancel[(x^2 - 1)/(x + 1)]");
    assert!(
        !out.contains("error"),
        "Cancel should not error, got: {out}"
    );
}

// ── Limit ──

#[test]
fn test_limit_polynomial() {
    let out = syma_eval("Limit[x^2, x -> 3]");
    assert!(
        !out.contains("error"),
        "Limit[x^2,x->3] should not error, got: {out}"
    );
}

// ── Series ──

#[test]
fn test_series_sin() {
    let out = syma_eval("Series[Sin[x], {x, 0, 4}]");
    assert!(
        !out.contains("error"),
        "Series[Sin[x],{{x,0,4}}] should not error, got: {out}"
    );
}
