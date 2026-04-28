//! Flat attribute tests
//!
//! Flat (associativity): f[f[a, b], c] = f[a, b, c]
//! - Pattern matching: flattens both pattern and value args
//! - Evaluator: normalizes results to flatten nested calls

use super::syma_eval;

// ── Pattern matching ──

#[test]
fn flat_pattern_matching_basic() {
    let out = syma_eval("SetAttributes[f, Flat]; MatchQ[f[1, f[2, 3]], f[x_, y_, z_]]");
    assert!(
        out.contains("True"),
        "Flat pattern match failed, got: {out}"
    );
}

#[test]
fn flat_pattern_matching_deep() {
    let out = syma_eval("SetAttributes[f, Flat]; MatchQ[f[f[1, 2], f[3, 4]], f[x_, y_, z_, w_]]");
    assert!(
        out.contains("True"),
        "Deeply nested Flat match failed, got: {out}"
    );
}

#[test]
fn flat_pattern_without_attribute_fails() {
    let out = syma_eval("MatchQ[h[1, h[2, 3]], h[x_, y_, z_]]");
    assert!(
        out.contains("False"),
        "Without Flat, nested calls should not flatten, got: {out}"
    );
}

// ── Result normalization ──

#[test]
fn flat_plus_result() {
    let out = syma_eval("Plus[Plus[a, b], c]");
    assert!(
        !out.contains("Plus[Plus"),
        "Plus result should be flattened, got: {out}"
    );
}

#[test]
fn flat_times_result() {
    let out = syma_eval("Times[Times[a, b], c]");
    assert!(
        !out.contains("Times[Times"),
        "Times result should be flattened, got: {out}"
    );
}

#[test]
fn flat_and_result() {
    let out = syma_eval("And[And[a, b], c]");
    assert!(
        !out.contains("And[And"),
        "And result should be flattened, got: {out}"
    );
}

#[test]
fn flat_or_result() {
    let out = syma_eval("Or[Or[a, b], c]");
    assert!(
        !out.contains("Or[Or"),
        "Or result should be flattened, got: {out}"
    );
}

#[test]
fn flat_deeply_nested_result() {
    let out = syma_eval("Plus[Plus[Plus[a, b], c], d]");
    assert!(
        !out.contains("Plus[Plus"),
        "Deeply nested Plus should be fully flattened, got: {out}"
    );
}

#[test]
fn flat_user_defined_result() {
    let out = syma_eval("SetAttributes[f, Flat]; f[a, f[b, c]]");
    assert!(
        !out.contains("f[f"),
        "User Flat function result should be flattened, got: {out}"
    );
}

// ── Builtin Flat attributes ──

#[test]
fn flat_plus_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Plus], Flat]");
    assert!(out.contains("True"), "Plus should have Flat, got: {out}");
}

#[test]
fn flat_times_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Times], Flat]");
    assert!(out.contains("True"), "Times should have Flat, got: {out}");
}

#[test]
fn flat_noncommutative_multiply_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[NonCommutativeMultiply], Flat]");
    assert!(out.contains("True"), "NCCM should have Flat, got: {out}");
}

// ── Flat with function definitions ──

#[test]
fn flat_user_function_definition() {
    let out = syma_eval(
        "SetAttributes[f, Flat]; \
         f[x_, y_] := x * y; \
         f[2, 3]",
    );
    assert!(
        out.contains("6"),
        "Flat user function f[2,3] should evaluate, got: {out}"
    );
}

// ── Known limitation: Flat+Orderless combined pattern matching in evaluator
// does not sort args before dispatch, so MatchQ with user functions returns
// unevaluated form. Pattern engine handles it correctly via permutations. ──

#[test]
fn flat_orderless_user_function_eval() {
    // Flat+Orderless: f[2, 1] should evaluate same as f[1, 2] when orderless
    // Currently: pattern matching works, evaluator dispatch does NOT sort
    let out = syma_eval(
        "SetAttributes[f, {Flat, Orderless}]; \
         f[x_, y_] := x + y; \
         f[2, 1]",
    );
    assert!(
        out.contains("3"),
        "Flat+Orderless f[2,1] should give 3, got: {out}"
    );
}
