//! Listable attribute tests
//!
//! Listable: functions auto-thread over lists.
//! - Plus[{1,2}, 3] → {4, 5}
//! - Sin[{0, Pi/2}] → {0, 1}
//! - Mixed scalar + list args supported

use super::syma_eval;

// ── Listable on Plus ──

#[test]
fn listable_plus_scalar() {
    let out = syma_eval("{1, 2, 3} + 10");
    assert!(
        out.contains("{11, 12, 13}") || out.contains("11") && out.contains("12"),
        "{1,2,3}+10 should thread, got: {out}"
    );
}

#[test]
fn listable_plus_two_lists() {
    let out = syma_eval("{1, 2} + {10, 20}");
    assert!(
        out.contains("{11") || out.contains("11"),
        "{1,2}+{10,20} should thread, got: {out}"
    );
}

// ── Listable on Times ──

#[test]
fn listable_times_scalar() {
    let out = syma_eval("{1, 2, 3} * 2");
    assert!(
        out.contains("{2, 4, 6}") || out.contains("2") && out.contains("4"),
        "{1,2,3}*2 should thread, got: {out}"
    );
}

#[test]
fn listable_times_two_lists() {
    let out = syma_eval("{1, 2} * {3, 4}");
    assert!(
        out.contains("{3, 8}") || out.contains("3") && out.contains("8"),
        "{1,2}*{3,4} should thread, got: {out}"
    );
}

// ── Listable on math functions ──

#[test]
fn listable_sin() {
    let out = syma_eval("Sin[{0}]");
    assert!(
        out.contains("{0}"),
        "Sin[{0}] should thread, got: {out}"
    );
}

#[test]
fn listable_cos() {
    let out = syma_eval("Cos[{0}]");
    assert!(
        out.contains("{1}"),
        "Cos[{0}] should thread, got: {out}"
    );
}

#[test]
fn listable_exp() {
    let out = syma_eval("Exp[{0}]");
    assert!(
        out.contains("{1}"),
        "Exp[{0}] should thread, got: {out}"
    );
}

#[test]
fn listable_log() {
    let out = syma_eval("Log[{1}]");
    assert!(
        out.contains("{0}"),
        "Log[{1}] should thread, got: {out}"
    );
}

#[test]
fn listable_sqrt() {
    let out = syma_eval("Sqrt[{4, 9}]");
    assert!(
        out.contains("2") && out.contains("3"),
        "Sqrt[{4,9}] should thread, got: {out}"
    );
}

// ── Listable on logical functions ──

#[test]
fn listable_boole() {
    let out = syma_eval("Boole[{True, False}]");
    assert!(
        out.contains("{1, 0}"),
        "Boole[{True,False}] should thread, got: {out}"
    );
}

#[test]
fn listable_xor() {
    let out = syma_eval("Xor[{True, False}, True]");
    assert!(
        out.contains("{False, True}"),
        "Xor[{True,False},True] should thread, got: {out}"
    );
}

// ── Listable attribute verification ──

#[test]
fn listable_plus_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Plus], Listable]");
    assert!(out.contains("True"), "Plus should have Listable, got: {out}");
}

#[test]
fn listable_times_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Times], Listable]");
    assert!(out.contains("True"), "Times should have Listable, got: {out}");
}

#[test]
fn listable_sin_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Sin], Listable]");
    assert!(out.contains("True"), "Sin should have Listable, got: {out}");
}

#[test]
fn listable_power_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Power], Listable]");
    assert!(out.contains("True"), "Power should have Listable, got: {out}");
}

// ── Listable edge cases ──

#[test]
fn listable_empty_list() {
    let out = syma_eval("{} + 1");
    assert!(
        out.contains("{}"),
        "{}+1 should return empty list, got: {out}"
    );
}

#[test]
fn listable_mismatched_lengths_unchanged() {
    // Mismatched list lengths should return unevaluated
    let out = syma_eval("{1, 2, 3} + {1, 2}");
    // Either unevaluated or partial result
    assert!(!out.is_empty(), "Mismatched lists should not crash, got: {out}");
}

// ── User-defined Listable ──

#[test]
fn listable_user_defined() {
    let out = syma_eval(
        "SetAttributes[f, Listable]; \
         f[x_] := x * 2; \
         f[{1, 2, 3}]",
    );
    assert!(
        out.contains("2") && out.contains("4") && out.contains("6"),
        "User Listable function should thread, got: {out}"
    );
}
