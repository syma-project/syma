//! Orderless attribute tests
//!
//! Orderless (commutativity): arguments are automatically sorted
//! - Pattern matching: tries permutations of value args

use super::syma_eval;

// ── Pattern matching ──

#[test]
fn orderless_basic_match() {
    let out = syma_eval("SetAttributes[f, Orderless]; MatchQ[f[2, 1], f[x_, y_]]");
    assert!(
        out.contains("True"),
        "f[2, 1] should match f[x_, y_] with Orderless, got: {out}"
    );
}

#[test]
fn orderless_binding_order() {
    let out = syma_eval(
        "SetAttributes[f, Orderless]; \
         f[2, 1] /. f[x_, y_] :> {x, y}",
    );
    assert!(
        out.contains("2") && out.contains("1"),
        "Orderless binding should contain both values, got: {out}"
    );
}

#[test]
fn orderless_without_attribute_fails() {
    let out = syma_eval("MatchQ[h[2, 1], h[1, x_]]");
    assert!(
        out.contains("False"),
        "Without Orderless, argument order matters, got: {out}"
    );
}

#[test]
fn orderless_three_args() {
    let out = syma_eval("SetAttributes[f, Orderless]; MatchQ[f[3, 1, 2], f[x_, y_, z_]]");
    assert!(
        out.contains("True"),
        "Orderless with 3 args should match, got: {out}"
    );
}

#[test]
fn orderless_with_type_constraint() {
    let out = syma_eval(
        "SetAttributes[f, Orderless]; \
         MatchQ[f[\"hello\", 42], f[x_Integer, y_String]]",
    );
    assert!(
        out.contains("True"),
        "Orderless should reorder to match type constraints, got: {out}"
    );
}

// ── Builtin Orderless attributes ──

#[test]
fn orderless_plus_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Plus], Orderless]");
    assert!(out.contains("True"), "Plus should have Orderless, got: {out}");
}

#[test]
fn orderless_times_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Times], Orderless]");
    assert!(out.contains("True"), "Times should have Orderless, got: {out}");
}

#[test]
fn orderless_and_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[And], Orderless]");
    assert!(out.contains("True"), "And should have Orderless, got: {out}");
}

#[test]
fn orderless_xor_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Xor], Orderless]");
    assert!(out.contains("True"), "Xor should have Orderless, got: {out}");
}

#[test]
fn orderless_equivalent_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Equivalent], Orderless]");
    assert!(out.contains("True"), "Equivalent should have Orderless, got: {out}");
}

// ── Orderless limit ──

#[test]
fn orderless_max_six_elements() {
    let out = syma_eval(
        "SetAttributes[f, Orderless]; \
         MatchQ[f[6,5,4,3,2,1], f[x_, y_, z_, w_, v_, u_]]",
    );
    assert!(
        out.contains("True"),
        "Orderless should match up to 6 elements, got: {out}"
    );
}
