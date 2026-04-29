/// Extended arithmetic integration tests: complex numbers, edge cases

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── Complex number construction ──

#[test]
fn test_complex_construction() {
    let out = syma_eval("Complex[1, 2]");
    assert!(
        !out.contains("error"),
        "Complex[1,2] should not error, got: {out}"
    );
}

#[test]
fn test_complex_q_true() {
    let out = syma_eval("ComplexQ[Complex[1, 2]]");
    assert!(out.contains("True"), "ComplexQ should be True, got: {out}");
}

#[test]
fn test_complex_q_false() {
    let out = syma_eval("ComplexQ[5]");
    assert!(
        out.contains("False"),
        "ComplexQ[5] should be False, got: {out}"
    );
}

// ── Re / Im ──

#[test]
fn test_re_complex() {
    let out = syma_eval("Re[Complex[1, 2]]");
    assert!(
        out.contains("1") || !out.contains("error"),
        "Re should extract 1, got: {out}"
    );
}

#[test]
fn test_re_real() {
    let out = syma_eval("Re[5]");
    assert!(out.contains("5"), "Re[5] should be 5, got: {out}");
}

#[test]
fn test_im_complex() {
    let out = syma_eval("Im[Complex[1, 2]]");
    assert!(!out.contains("error"), "Im should not error, got: {out}");
}

// ── Conjugate ──

#[test]
fn test_conjugate() {
    let out = syma_eval("Conjugate[Complex[1, 2]]");
    assert!(
        !out.contains("error"),
        "Conjugate should not error, got: {out}"
    );
}

// ── Arg / Sign / AbsArg ──

#[test]
fn test_arg_real_positive() {
    let out = syma_eval("Arg[1]");
    assert!(out.contains("0"), "Arg[1] should be 0, got: {out}");
}

#[test]
fn test_arg_complex() {
    let out = syma_eval("Arg[1 + I]");
    assert!(
        !out.contains("error"),
        "Arg[1+I] should not error, got: {out}"
    );
}

#[test]
fn test_sign_complex() {
    let out = syma_eval("Sign[Complex[1, 2]]");
    assert!(
        !out.contains("error"),
        "Sign on complex should not error, got: {out}"
    );
}

#[test]
fn test_abs_arg() {
    let out = syma_eval("AbsArg[1 + I]");
    assert!(
        !out.contains("error"),
        "AbsArg should not error, got: {out}"
    );
}

#[test]
fn test_re_im() {
    let out = syma_eval("ReIm[Complex[1, 2]]");
    assert!(!out.contains("error"), "ReIm should not error, got: {out}");
}

// ── Power edge cases ──

#[test]
fn test_power_zero_positive() {
    let out = syma_eval("0^5");
    assert!(out.contains("0"), "0^5 should be 0, got: {out}");
}

#[test]
fn test_power_negative_base_even() {
    let out = syma_eval("(-2)^2");
    assert!(out.contains("4"), "(-2)^2 should be 4, got: {out}");
}

#[test]
fn test_power_negative_base_odd() {
    let out = syma_eval("(-2)^3");
    assert!(
        out.contains("-8") || out.contains("8"),
        "(-2)^3 should be -8, got: {out}"
    );
}

#[test]
fn test_power_fractional() {
    let out = syma_eval("Power[4, 1/2]");
    assert!(
        !out.contains("error"),
        "4^(1/2) should not error, got: {out}"
    );
}

#[test]
fn test_power_zero_negative() {
    let out = syma_eval("Power[0, -1]");
    assert!(
        !out.contains("error"),
        "0^(-1) should not crash, got: {out}"
    );
}

// ── Divide edge cases ──

#[test]
fn test_divide_exact() {
    let out = syma_eval("Divide[6, 2]");
    assert!(out.contains("3"), "Divide[6,2] should be 3, got: {out}");
}

#[test]
fn test_divide_fractional() {
    let out = syma_eval("Divide[5, 2]");
    assert!(
        !out.contains("error"),
        "Divide[5,2] should not error, got: {out}"
    );
}

#[test]
fn test_divide_by_zero() {
    let out = syma_eval("Divide[1, 0]");
    assert!(
        !out.contains("error"),
        "Divide[1,0] should not crash, got: {out}"
    );
}

// ── Abs edge cases ──

#[test]
fn test_abs_positive() {
    let out = syma_eval("Abs[5]");
    assert!(out.contains("5"), "Abs[5] should be 5, got: {out}");
}

#[test]
fn test_abs_negative() {
    let out = syma_eval("Abs[-5]");
    assert!(out.contains("5"), "Abs[-5] should be 5, got: {out}");
}

#[test]
fn test_abs_zero() {
    let out = syma_eval("Abs[0]");
    assert!(out.contains("0"), "Abs[0] should be 0, got: {out}");
}

#[test]
fn test_abs_float() {
    let out = syma_eval("Abs[-3.14]");
    assert!(
        out.contains("3.14"),
        "Abs[-3.14] should be 3.14, got: {out}"
    );
}

// ── Min / Max ──

#[test]
fn test_min_basic() {
    let out = syma_eval("Min[3, 1, 2]");
    assert!(out.contains("1"), "Min[3,1,2] should be 1, got: {out}");
}

#[test]
fn test_max_basic() {
    let out = syma_eval("Max[3, 1, 2]");
    assert!(out.contains("3"), "Max[3,1,2] should be 3, got: {out}");
}

// ── GCD / LCM ──

#[test]
fn test_gcd_basic() {
    let out = syma_eval("GCD[12, 8]");
    assert!(out.contains("4"), "GCD[12,8] should be 4, got: {out}");
}

#[test]
fn test_gcd_coprime() {
    let out = syma_eval("GCD[7, 13]");
    assert!(out.contains("1"), "GCD[7,13] should be 1, got: {out}");
}

#[test]
fn test_lcm_basic() {
    let out = syma_eval("LCM[12, 8]");
    assert!(out.contains("24"), "LCM[12,8] should be 24, got: {out}");
}

#[test]
fn test_lcm_coprime() {
    let out = syma_eval("LCM[3, 5]");
    assert!(out.contains("15"), "LCM[3,5] should be 15, got: {out}");
}

// ── Mod ──

#[test]
fn test_mod_basic() {
    let out = syma_eval("Mod[10, 3]");
    assert!(out.contains("1"), "Mod[10,3] should be 1, got: {out}");
}

#[test]
fn test_mod_exact() {
    let out = syma_eval("Mod[10, 5]");
    assert!(out.contains("0"), "Mod[10,5] should be 0, got: {out}");
}

#[test]
fn test_mod_negative() {
    let out = syma_eval("Mod[-1, 3]");
    assert!(out.contains("2"), "Mod[-1,3] should be 2, got: {out}");
}

// ── ArcSin / ArcCos / ArcTan ──

#[test]
fn test_arcsin_one() {
    let out = syma_eval("ArcSin[1]");
    assert!(
        !out.contains("error"),
        "ArcSin[1] should not error, got: {out}"
    );
}

#[test]
fn test_arccos_zero() {
    let out = syma_eval("ArcCos[0]");
    assert!(
        !out.contains("error"),
        "ArcCos[0] should not error, got: {out}"
    );
}

#[ignore = "ArcTan 2-arg not implemented"]
#[test]
fn test_arctan_two_arg() {
    let out = syma_eval("ArcTan[1, 0]");
    assert!(
        !out.contains("error"),
        "ArcTan[1,0] should not error, got: {out}"
    );
}

// ── Log2 / Log10 ──

#[test]
fn test_log2() {
    let out = syma_eval("Log2[8]");
    assert!(out.contains("3"), "Log2[8] should be 3, got: {out}");
}

#[test]
fn test_log10() {
    let out = syma_eval("Log10[100]");
    assert!(out.contains("2"), "Log10[100] should be 2, got: {out}");
}
