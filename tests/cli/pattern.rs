use super::*;

#[test]
fn pattern_integer_q() {
    let out = syma_eval("MatchQ[5, _?IntegerQ]");
    assert!(
        out.contains("True"),
        "5 should match _?IntegerQ, got: {out}"
    );
}

#[test]
fn pattern_not_integer_q() {
    let out = syma_eval("MatchQ[3.14, _?IntegerQ]");
    assert!(
        out.contains("False"),
        "3.14 should not match _?IntegerQ, got: {out}"
    );
}

#[test]
fn optional_default_value() {
    let out = syma_eval("f[x_:5] := x; f[]");
    assert!(out.contains("5"), "f[] should return 5, got: {out}");
}

#[test]
fn integrate_basic() {
    let out = syma_eval("Integrate[x^2 + Sin[x], x]");
    assert!(
        out.contains("Cos[x]"),
        "integrate should contain Cos[x], got: {out}"
    );
    assert!(
        out.contains("x^3"),
        "integrate should contain x^3, got: {out}"
    );
}
