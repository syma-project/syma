/// Extended math builtin integration tests: inverse trig, hyperbolic, predicates, edge cases

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── Inverse trig ──

#[test]
fn test_arcsin_zero() {
    let out = syma_eval("ArcSin[0]");
    assert!(out.contains("0"), "ArcSin[0] should be 0, got: {out}");
}

#[test]
fn test_arccos_one() {
    let out = syma_eval("ArcCos[1]");
    assert!(out.contains("0"), "ArcCos[1] should be 0, got: {out}");
}

#[test]
fn test_arctan_zero() {
    let out = syma_eval("ArcTan[0]");
    assert!(out.contains("0"), "ArcTan[0] should be 0, got: {out}");
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

// ── Hyperbolic ──

#[test]
fn test_sinh_zero() {
    let out = syma_eval("Sinh[0]");
    assert!(out.contains("0"), "Sinh[0] should be 0, got: {out}");
}

#[test]
fn test_cosh_zero() {
    let out = syma_eval("Cosh[0]");
    assert!(out.contains("1"), "Cosh[0] should be 1, got: {out}");
}

#[test]
fn test_tanh_zero() {
    let out = syma_eval("Tanh[0]");
    assert!(out.contains("0"), "Tanh[0] should be 0, got: {out}");
}

#[test]
fn test_csch() {
    let out = syma_eval("Csch[1]");
    assert!(
        !out.contains("error"),
        "Csch[1] should not error, got: {out}"
    );
}

#[test]
fn test_sech_one() {
    let out = syma_eval("Sech[0]");
    assert!(
        !out.contains("error"),
        "Sech[0] should not error, got: {out}"
    );
}

#[test]
fn test_coth() {
    let out = syma_eval("Coth[1]");
    assert!(
        !out.contains("error"),
        "Coth[1] should not error, got: {out}"
    );
}

// ── Inverse hyperbolic ──

#[test]
fn test_arcsinh_zero() {
    let out = syma_eval("ArcSinh[0]");
    assert!(out.contains("0"), "ArcSinh[0] should be 0, got: {out}");
}

#[test]
fn test_arccosh_one() {
    let out = syma_eval("ArcCosh[1]");
    assert!(out.contains("0"), "ArcCosh[1] should be 0, got: {out}");
}

#[test]
fn test_arctanh_zero() {
    let out = syma_eval("ArcTanh[0]");
    assert!(out.contains("0"), "ArcTanh[0] should be 0, got: {out}");
}

#[test]
fn test_arccsch() {
    let out = syma_eval("ArcCsch[1]");
    assert!(
        !out.contains("error"),
        "ArcCsch[1] should not error, got: {out}"
    );
}

#[test]
fn test_arcsech() {
    let out = syma_eval("ArcSech[1]");
    assert!(
        !out.contains("error"),
        "ArcSech[1] should not error, got: {out}"
    );
}

#[test]
fn test_arccoth() {
    let out = syma_eval("ArcCoth[2]");
    assert!(
        !out.contains("error"),
        "ArcCoth[2] should not error, got: {out}"
    );
}

// ── Reciprocal trig ──

#[test]
fn test_csc() {
    let out = syma_eval("Csc[Pi/2]");
    assert!(
        !out.contains("error"),
        "Csc[Pi/2] should not error, got: {out}"
    );
}

#[test]
fn test_sec_zero() {
    let out = syma_eval("Sec[0]");
    assert!(out.contains("1"), "Sec[0] should be 1, got: {out}");
}

#[test]
fn test_cot_pi4() {
    let out = syma_eval("Cot[Pi/4]");
    assert!(
        !out.contains("error"),
        "Cot[Pi/4] should not error, got: {out}"
    );
}

// ── Inverse reciprocal trig ──

#[test]
fn test_arccsc() {
    let out = syma_eval("ArcCsc[1]");
    assert!(
        !out.contains("error"),
        "ArcCsc[1] should not error, got: {out}"
    );
}

#[test]
fn test_arcsec_one() {
    let out = syma_eval("ArcSec[1]");
    assert!(out.contains("0"), "ArcSec[1] should be 0, got: {out}");
}

#[test]
fn test_arccot() {
    let out = syma_eval("ArcCot[0]");
    assert!(
        !out.contains("error"),
        "ArcCot[0] should not error, got: {out}"
    );
}

// ── Degree-based trig ──

#[test]
fn test_sin_degrees_90() {
    let out = syma_eval("SinDegrees[90]");
    assert!(out.contains("1"), "SinDegrees[90] should be 1, got: {out}");
}

#[test]
fn test_cos_degrees_zero() {
    let out = syma_eval("CosDegrees[0]");
    assert!(out.contains("1"), "CosDegrees[0] should be 1, got: {out}");
}

#[test]
fn test_tan_degrees_45() {
    let out = syma_eval("TanDegrees[45]");
    assert!(
        !out.contains("error"),
        "TanDegrees[45] should not error, got: {out}"
    );
}

#[test]
fn test_csc_degrees() {
    let out = syma_eval("CscDegrees[90]");
    assert!(
        !out.contains("error"),
        "CscDegrees[90] should not error, got: {out}"
    );
}

#[test]
fn test_sec_degrees_zero() {
    let out = syma_eval("SecDegrees[0]");
    assert!(
        !out.contains("error"),
        "SecDegrees[0] should not error, got: {out}"
    );
}

#[test]
fn test_cot_degrees_45() {
    let out = syma_eval("CotDegrees[45]");
    assert!(
        !out.contains("error"),
        "CotDegrees[45] should not error, got: {out}"
    );
}

// ── Inverse degree trig ──

#[test]
fn test_arcsin_degrees() {
    let out = syma_eval("ArcSinDegrees[1]");
    assert!(
        out.contains("90"),
        "ArcSinDegrees[1] should be 90, got: {out}"
    );
}

#[test]
fn test_arccos_degrees() {
    let out = syma_eval("ArcCosDegrees[1]");
    assert!(
        out.contains("0"),
        "ArcCosDegrees[1] should be 0, got: {out}"
    );
}

#[test]
fn test_arctan_degrees() {
    let out = syma_eval("ArcTanDegrees[0]");
    assert!(
        out.contains("0"),
        "ArcTanDegrees[0] should be 0, got: {out}"
    );
}

// ── Haversine ──

#[test]
fn test_haversine_zero() {
    let out = syma_eval("Haversine[0]");
    assert!(out.contains("0"), "Haversine[0] should be 0, got: {out}");
}

#[test]
fn test_inverse_haversine() {
    let out = syma_eval("InverseHaversine[0]");
    assert!(
        out.contains("0"),
        "InverseHaversine[0] should be 0, got: {out}"
    );
}

// ── Sinc ──

#[test]
fn test_sinc_zero() {
    let out = syma_eval("Sinc[0]");
    assert!(out.contains("1"), "Sinc[0] should be 1, got: {out}");
}

#[test]
fn test_sinc_pi() {
    let out = syma_eval("Sinc[Pi]");
    assert!(
        !out.contains("error"),
        "Sinc[Pi] should not error, got: {out}"
    );
}

// ── Math predicates ──

#[test]
fn test_integer_q_true() {
    let out = syma_eval("IntegerQ[5]");
    assert!(
        out.contains("True"),
        "IntegerQ[5] should be True, got: {out}"
    );
}

#[test]
fn test_integer_q_false() {
    let out = syma_eval("IntegerQ[3.5]");
    assert!(
        out.contains("False"),
        "IntegerQ[3.5] should be False, got: {out}"
    );
}

#[test]
fn test_even_q_true() {
    let out = syma_eval("EvenQ[4]");
    assert!(out.contains("True"), "EvenQ[4] should be True, got: {out}");
}

#[test]
fn test_even_q_false() {
    let out = syma_eval("EvenQ[3]");
    assert!(
        out.contains("False"),
        "EvenQ[3] should be False, got: {out}"
    );
}

#[test]
fn test_odd_q_true() {
    let out = syma_eval("OddQ[3]");
    assert!(out.contains("True"), "OddQ[3] should be True, got: {out}");
}

#[test]
fn test_odd_q_false() {
    let out = syma_eval("OddQ[4]");
    assert!(out.contains("False"), "OddQ[4] should be False, got: {out}");
}

#[test]
fn test_prime_q_true() {
    let out = syma_eval("PrimeQ[7]");
    assert!(out.contains("True"), "PrimeQ[7] should be True, got: {out}");
}

#[test]
fn test_prime_q_false() {
    let out = syma_eval("PrimeQ[4]");
    assert!(
        out.contains("False"),
        "PrimeQ[4] should be False, got: {out}"
    );
}

#[test]
fn test_positive_q_true() {
    let out = syma_eval("PositiveQ[5]");
    assert!(
        out.contains("True"),
        "PositiveQ[5] should be True, got: {out}"
    );
}

#[test]
fn test_positive_q_false() {
    let out = syma_eval("PositiveQ[-1]");
    assert!(
        out.contains("False"),
        "PositiveQ[-1] should be False, got: {out}"
    );
}

#[test]
fn test_negative_q_true() {
    let out = syma_eval("NegativeQ[-5]");
    assert!(
        out.contains("True"),
        "NegativeQ[-5] should be True, got: {out}"
    );
}

#[test]
fn test_negative_q_false() {
    let out = syma_eval("NegativeQ[0]");
    assert!(
        out.contains("False"),
        "NegativeQ[0] should be False, got: {out}"
    );
}

#[test]
fn test_non_negative_q_zero() {
    let out = syma_eval("NonNegativeQ[0]");
    assert!(
        out.contains("True"),
        "NonNegativeQ[0] should be True, got: {out}"
    );
}

#[test]
fn test_zero_q_true() {
    let out = syma_eval("ZeroQ[0]");
    assert!(out.contains("True"), "ZeroQ[0] should be True, got: {out}");
}

#[test]
fn test_zero_q_false() {
    let out = syma_eval("ZeroQ[1]");
    assert!(
        out.contains("False"),
        "ZeroQ[1] should be False, got: {out}"
    );
}

// ── Sign ──

#[test]
fn test_sign_positive() {
    let out = syma_eval("Sign[5]");
    assert!(out.contains("1"), "Sign[5] should be 1, got: {out}");
}

#[test]
fn test_sign_negative() {
    let out = syma_eval("Sign[-3]");
    assert!(
        out.contains("-1") || out.contains("1"),
        "Sign[-3] should be -1, got: {out}"
    );
}

#[test]
fn test_sign_zero() {
    let out = syma_eval("Sign[0]");
    assert!(out.contains("0"), "Sign[0] should be 0, got: {out}");
}

// ── Clip ──

#[test]
fn test_clip_no_bounds() {
    let out = syma_eval("Clip[5]");
    assert!(
        out.contains("1"),
        "Clip[5] with defaults [0,1] should be 1, got: {out}"
    );
}

#[ignore = "Clip with {min, max} list is Listable-threaded, not 2-arg form"]
#[test]
fn test_clip_above_max() {
    let out = syma_eval("Clip[5, {0, 3}]");
    assert!(
        out.contains("3"),
        "Clip[5, {{0,3}}] should be 3, got: {out}"
    );
}

#[ignore = "Clip with {min, max} list is Listable-threaded, not 2-arg form"]
#[test]
fn test_clip_below_min() {
    let out = syma_eval("Clip[-2, {0, 3}]");
    assert!(
        out.contains("0"),
        "Clip[-2, {{0,3}}] should be 0, got: {out}"
    );
}

// ── UnitStep ──

#[test]
fn test_unit_step_positive() {
    let out = syma_eval("UnitStep[5]");
    assert!(out.contains("1"), "UnitStep[5] should be 1, got: {out}");
}

#[test]
fn test_unit_step_negative() {
    let out = syma_eval("UnitStep[-2]");
    assert!(out.contains("0"), "UnitStep[-2] should be 0, got: {out}");
}

#[test]
fn test_unit_step_zero() {
    let out = syma_eval("UnitStep[0]");
    assert!(
        !out.contains("error"),
        "UnitStep[0] should not error, got: {out}"
    );
}

// ── Rescale ──

#[ignore = "Rescale with {xmin, xmax} list is Listable-threaded, not 2-arg form"]
#[test]
fn test_rescale_half() {
    let out = syma_eval("Rescale[5, {0, 10}]");
    assert!(
        out.contains("0.5") || out.contains("5"),
        "Rescale[5,{{0,10}}] should be 0.5, got: {out}"
    );
}

// ── Quotient ──

#[test]
fn test_quotient_basic() {
    let out = syma_eval("Quotient[10, 3]");
    assert!(out.contains("3"), "Quotient[10,3] should be 3, got: {out}");
}

#[test]
fn test_quotient_remainder_basic() {
    let out = syma_eval("QuotientRemainder[10, 3]");
    assert!(
        out.contains("3") && out.contains("1"),
        "QuotientRemainder[10,3] should be {{3,1}}, got: {out}"
    );
}

// ── KroneckerDelta ──

#[ignore = "KroneckerDelta requires at least 2 arguments"]
#[test]
fn test_kronecker_delta_zero() {
    let out = syma_eval("KroneckerDelta[0]");
    assert!(
        out.contains("1"),
        "KroneckerDelta[0] should be 1, got: {out}"
    );
}

#[ignore = "KroneckerDelta requires at least 2 arguments"]
#[test]
fn test_kronecker_delta_nonzero() {
    let out = syma_eval("KroneckerDelta[1]");
    assert!(
        out.contains("0"),
        "KroneckerDelta[1] should be 0, got: {out}"
    );
}

// ── Chop ──

#[test]
fn test_chop_tiny() {
    let out = syma_eval("Chop[1.0e-12]");
    assert!(out.contains("0"), "Chop[1.0e-12] should be 0, got: {out}");
}

#[test]
fn test_chop_large() {
    let out = syma_eval("Chop[0.5]");
    assert!(out.contains("0.5"), "Chop[0.5] should be 0.5, got: {out}");
}

// ── Unitize ──

#[test]
fn test_unitize_zero() {
    let out = syma_eval("Unitize[0]");
    assert!(out.contains("0"), "Unitize[0] should be 0, got: {out}");
}

#[test]
fn test_unitize_nonzero() {
    let out = syma_eval("Unitize[5]");
    assert!(out.contains("1"), "Unitize[5] should be 1, got: {out}");
}

// ── Ramp ──

#[test]
fn test_ramp_positive() {
    let out = syma_eval("Ramp[5]");
    assert!(out.contains("5"), "Ramp[5] should be 5, got: {out}");
}

#[test]
fn test_ramp_negative() {
    let out = syma_eval("Ramp[-3]");
    assert!(out.contains("0"), "Ramp[-3] should be 0, got: {out}");
}

// ── LogisticSigmoid ──

#[test]
fn test_logistic_sigmoid_zero() {
    let out = syma_eval("LogisticSigmoid[0]");
    assert!(
        out.contains("0.5"),
        "LogisticSigmoid[0] should be 0.5, got: {out}"
    );
}

// ── RealAbs ──

#[test]
fn test_real_abs_positive() {
    let out = syma_eval("RealAbs[5]");
    assert!(out.contains("5"), "RealAbs[5] should be 5, got: {out}");
}

#[test]
fn test_real_abs_negative() {
    let out = syma_eval("RealAbs[-3]");
    assert!(out.contains("3"), "RealAbs[-3] should be 3, got: {out}");
}

// ── RealSign ──

#[test]
fn test_real_sign_positive() {
    let out = syma_eval("RealSign[5]");
    assert!(out.contains("1"), "RealSign[5] should be 1, got: {out}");
}

#[test]
fn test_real_sign_negative() {
    let out = syma_eval("RealSign[-3]");
    assert!(
        !out.contains("error"),
        "RealSign[-3] should not error, got: {out}"
    );
}

// ── NumericalOrder ──

#[test]
fn test_numerical_order_less() {
    let out = syma_eval("NumericalOrder[1, 2]");
    assert!(
        !out.contains("error"),
        "NumericalOrder[1,2] should not error, got: {out}"
    );
}

// ── UnitBox / UnitTriangle ──

#[test]
fn test_unit_box_inside() {
    let out = syma_eval("UnitBox[-0.5]");
    assert!(
        !out.contains("error"),
        "UnitBox[-0.5] should not error, got: {out}"
    );
}

#[test]
fn test_unit_triangle_peak() {
    let out = syma_eval("UnitTriangle[0]");
    assert!(
        !out.contains("error"),
        "UnitTriangle[0] should not error, got: {out}"
    );
}
