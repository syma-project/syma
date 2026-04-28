//! OneIdentity attribute tests
//!
//! OneIdentity: f[x] is treated as x for pattern matching.
//! - Does NOT affect evaluation (f[1] still returns f[1] if no rules match)
//! - Only affects pattern matching: MatchQ[f[1], _Integer] → True
//!
//! Common on: Plus, Times, Min, Max, And, Or, Xor, Equivalent, NonCommutativeMultiply

use super::syma_eval;

// ── Core behavior ──

#[test]
fn one_identity_does_not_affect_evaluation() {
    // OneIdentity only affects pattern matching, not evaluation
    let out = syma_eval("SetAttributes[f, OneIdentity]; f[1]");
    assert!(
        out.contains("f[1]"),
        "OneIdentity should not change evaluation, got: {out}"
    );
}

#[test]
fn one_identity_integer_match() {
    // Core: f[42] matches _Integer when f has OneIdentity
    let out = syma_eval("SetAttributes[f, OneIdentity]; MatchQ[f[42], _Integer]");
    assert!(
        out.contains("True"),
        "f[42] should match _Integer with OneIdentity, got: {out}"
    );
}

#[test]
fn one_identity_real_match() {
    let out = syma_eval("SetAttributes[f, OneIdentity]; MatchQ[f[3.14], _Real]");
    assert!(
        out.contains("True"),
        "f[3.14] should match _Real with OneIdentity, got: {out}"
    );
}

#[test]
fn one_identity_string_match() {
    let out = syma_eval("SetAttributes[f, OneIdentity]; MatchQ[f[\"hello\"], _String]");
    assert!(
        out.contains("True"),
        "f[\"hello\"] should match _String with OneIdentity, got: {out}"
    );
}

#[test]
fn one_identity_without_attribute_fails() {
    // Without OneIdentity, g[42] should NOT match _Integer
    let out = syma_eval("g[x_] := x; MatchQ[g[42], _Integer]");
    assert!(
        out.contains("False"),
        "Without OneIdentity, g[42] should not match _Integer, got: {out}"
    );
}

#[test]
fn one_identity_named_blank() {
    let out = syma_eval("SetAttributes[f, OneIdentity]; MatchQ[f[42], x_Integer]");
    assert!(
        out.contains("True"),
        "f[42] should match named blank x_Integer with OneIdentity, got: {out}"
    );
}

#[test]
fn one_identity_multi_arg_no_match() {
    // OneIdentity only works for single-arg calls
    let out = syma_eval("SetAttributes[f, OneIdentity]; MatchQ[f[1, 2], _Integer]");
    assert!(
        out.contains("False"),
        "f[1, 2] should not match _Integer (multi-arg), got: {out}"
    );
}

// ── Builtin OneIdentity attributes ──

#[test]
fn one_identity_plus_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Plus], OneIdentity]");
    assert!(out.contains("True"), "Plus should have OneIdentity, got: {out}");
}

#[test]
fn one_identity_times_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Times], OneIdentity]");
    assert!(out.contains("True"), "Times should have OneIdentity, got: {out}");
}

#[test]
fn one_identity_and_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[And], OneIdentity]");
    assert!(out.contains("True"), "And should have OneIdentity, got: {out}");
}

#[test]
fn one_identity_xor_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Xor], OneIdentity]");
    assert!(out.contains("True"), "Xor should have OneIdentity, got: {out}");
}

#[test]
fn one_identity_equivalent_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Equivalent], OneIdentity]");
    assert!(out.contains("True"), "Equivalent should have OneIdentity, got: {out}");
}

#[test]
fn one_identity_nccm_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[NonCommutativeMultiply], OneIdentity]");
    assert!(out.contains("True"), "NonCommutativeMultiply should have OneIdentity, got: {out}");
}

#[test]
fn one_identity_min_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Min], OneIdentity]");
    assert!(out.contains("True"), "Min should have OneIdentity, got: {out}");
}

#[test]
fn one_identity_max_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Max], OneIdentity]");
    assert!(out.contains("True"), "Max should have OneIdentity, got: {out}");
}

// ── OneIdentity with rules ──

#[test]
fn one_identity_replace_rule() {
    // With OneIdentity on Plus, Plus[3] should match patterns for 3
    let out = syma_eval("Plus[3] /. x_Integer :> x * 2");
    assert!(
        out.contains("6"),
        "Plus[3] should match x_Integer rule with OneIdentity, got: {out}"
    );
}

// ── OneIdentity + Flat combo ──

#[test]
fn one_identity_with_flat() {
    let out = syma_eval(
        "SetAttributes[f, {Flat, OneIdentity}]; \
         MatchQ[f[5], _Integer]",
    );
    assert!(
        out.contains("True"),
        "OneIdentity should work with Flat, got: {out}"
    );
}
