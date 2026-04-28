//! Hold attribute tests
//!
//! HoldAll, HoldFirst, HoldRest, HoldAllComplete
//! These control whether function arguments are evaluated before dispatch.

use super::syma_eval;

// ── HoldAll ──

#[test]
fn hold_all_prevents_evaluation() {
    let out = syma_eval("Hold[1 + 2]");
    assert!(
        out.contains("Hold"),
        "Hold should not evaluate the argument, got: {out}"
    );
}

#[test]
fn hold_all_multiple_args() {
    let out = syma_eval("Hold[1 + 2, 3 * 4]");
    assert!(
        out.contains("Hold"),
        "Hold with multiple args should hold all, got: {out}"
    );
}

#[test]
fn hold_all_preserves_symbol() {
    let out = syma_eval("Hold[unassignedSymbol]");
    assert!(
        out.contains("unassignedSymbol"),
        "Hold should preserve unevaluated symbols, got: {out}"
    );
}

#[test]
fn hold_all_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Hold], HoldAll]");
    assert!(out.contains("True"), "Hold should have HoldAll, got: {out}");
}

#[test]
fn hold_all_user_defined() {
    // Set HoldAll before any definition
    let out = syma_eval(
        "SetAttributes[f, HoldAll]; \
         f[1 + 2]",
    );
    assert!(
        out.contains("1 + 2"),
        "HoldAll user function should not evaluate args, got: {out}"
    );
}

// ── HoldComplete (HoldAllComplete) ──

#[test]
fn hold_complete_prevents_evaluation() {
    let out = syma_eval("HoldComplete[1 + 2]");
    assert!(
        out.contains("HoldComplete"),
        "HoldComplete should prevent evaluation, got: {out}"
    );
}

#[test]
fn hold_complete_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[HoldComplete], HoldAllComplete]");
    assert!(out.contains("True"), "HoldComplete should have HoldAllComplete, got: {out}");
}

// ── ReleaseHold ──

#[test]
fn release_hold_evaluates() {
    let out = syma_eval("ReleaseHold[Hold[1 + 2]]");
    assert!(
        out.contains("3"),
        "ReleaseHold should evaluate held expression, got: {out}"
    );
}

#[test]
fn release_hold_complete() {
    let out = syma_eval("ReleaseHold[HoldComplete[2 * 3]]");
    assert!(
        out.contains("6"),
        "ReleaseHold on HoldComplete should evaluate, got: {out}"
    );
}

#[test]
fn release_hold_non_held() {
    let out = syma_eval("ReleaseHold[42]");
    assert!(
        out.contains("42"),
        "ReleaseHold on non-held value is identity, got: {out}"
    );
}

#[test]
fn release_hold_nested() {
    let out = syma_eval("ReleaseHold[Hold[1 + 2]]");
    assert!(
        out.contains("3"),
        "ReleaseHold should evaluate, got: {out}"
    );
}

// ── HoldFirst ──

#[test]
fn hold_first_set() {
    let out = syma_eval("x = 5; x = 10; x");
    assert!(
        out.contains("10"),
        "Set should hold first arg (LHS), got: {out}"
    );
}

#[test]
fn hold_first_setdelayed() {
    let out = syma_eval("f[x_] := x + 1; f[5]");
    assert!(
        out.contains("6"),
        "SetDelayed should hold pattern, got: {out}"
    );
}

#[test]
fn hold_first_set_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Set], HoldFirst]");
    assert!(out.contains("True"), "Set should have HoldFirst, got: {out}");
}

#[test]
fn hold_first_user_defined() {
    // Set HoldFirst, then test with an assigned variable
    let out = syma_eval(
        "SetAttributes[g, HoldFirst]; \
         y = 3; \
         g[y]",
    );
    assert!(
        out.contains("y"),
        "HoldFirst should prevent first arg evaluation, got: {out}"
    );
}

// ── HoldRest ──

#[test]
fn hold_rest_user_defined() {
    let out = syma_eval(
        "SetAttributes[h, HoldRest]; \
         h[1, 2 + 3]",
    );
    assert!(
        out.contains("2 + 3"),
        "HoldRest should hold arguments after the first, got: {out}"
    );
}

// ── If (HoldAll) ──

#[test]
fn if_hold_all() {
    let out = syma_eval("MemberQ[Attributes[If], HoldAll]");
    assert!(out.contains("True"), "If should have HoldAll, got: {out}");
}

#[test]
fn if_basic_true() {
    let out = syma_eval("If[True, a, b]");
    assert!(out.contains("a"), "If True should return first branch, got: {out}");
}

#[test]
fn if_basic_false() {
    let out = syma_eval("If[False, a, b]");
    assert!(out.contains("b"), "If False should return second branch, got: {out}");
}

// ── Module/With/Block (HoldAll) ──

#[test]
fn module_hold_all() {
    let out = syma_eval("MemberQ[Attributes[Module], HoldAll]");
    assert!(out.contains("True"), "Module should have HoldAll, got: {out}");
}

#[test]
fn with_hold_all() {
    let out = syma_eval("MemberQ[Attributes[With], HoldAll]");
    assert!(out.contains("True"), "With should have HoldAll, got: {out}");
}

#[test]
fn block_hold_all() {
    let out = syma_eval("MemberQ[Attributes[Block], HoldAll]");
    assert!(out.contains("True"), "Block should have HoldAll, got: {out}");
}

// ── D and Integrate (HoldAll) ──

#[test]
fn d_hold_all() {
    let out = syma_eval("MemberQ[Attributes[D], HoldAll]");
    assert!(out.contains("True"), "D should have HoldAll, got: {out}");
}

#[test]
fn integrate_hold_all() {
    let out = syma_eval("MemberQ[Attributes[Integrate], HoldAll]");
    assert!(out.contains("True"), "Integrate should have HoldAll, got: {out}");
}
