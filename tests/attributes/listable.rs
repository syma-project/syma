//! Listable attribute tests

use super::syma_eval;

// ── Listable on Plus ──

#[test]
fn listable_plus_scalar() {
    let out = syma_eval("{1, 2, 3} + 10");
    assert!(
        out.contains("11") && out.contains("12"),
        "list plus scalar should thread, got: {out}"
    );
}

#[test]
fn listable_plus_two_lists() {
    let out = syma_eval("{1, 2} + {10, 20}");
    assert!(
        out.contains("11"),
        "list plus two lists should thread, got: {out}"
    );
}

// ── Listable on Times ──

#[test]
fn listable_times_scalar() {
    let out = syma_eval("{1, 2, 3} * 2");
    assert!(
        out.contains("2") && out.contains("4"),
        "list times scalar should thread, got: {out}"
    );
}

#[test]
fn listable_times_two_lists() {
    let out = syma_eval("{1, 2} * {3, 4}");
    assert!(
        out.contains("3") && out.contains("8"),
        "list times two lists should thread, got: {out}"
    );
}

// ── Listable on math functions ──

#[test]
fn listable_sin() {
    let out = syma_eval("Sin[{0}]");
    assert!(out.contains("0"), "Sin list should thread, got: {out}");
}

#[test]
fn listable_cos() {
    let out = syma_eval("Cos[{0}]");
    assert!(out.contains("1"), "Cos list should thread, got: {out}");
}

#[test]
fn listable_exp() {
    let out = syma_eval("Exp[{0}]");
    assert!(out.contains("1"), "Exp list should thread, got: {out}");
}

#[test]
fn listable_log() {
    let out = syma_eval("Log[{1}]");
    assert!(out.contains("0"), "Log list should thread, got: {out}");
}

#[test]
fn listable_sqrt() {
    let out = syma_eval("Sqrt[{4, 9}]");
    assert!(
        out.contains("2") && out.contains("3"),
        "Sqrt list should thread, got: {out}"
    );
}

// ── Listable on logical functions ──

#[test]
fn listable_boole() {
    let out = syma_eval("Boole[{True, False}]");
    assert!(
        out.contains("1") && out.contains("0"),
        "Boole list should thread, got: {out}"
    );
}

#[test]
fn listable_xor() {
    let out = syma_eval("Xor[{True, False}, True]");
    assert!(
        out.contains("False") && out.contains("True"),
        "Xor list should thread, got: {out}"
    );
}

// ── Listable attribute verification ──

#[test]
fn listable_plus_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Plus], Listable]");
    assert!(
        out.contains("True"),
        "Plus should have Listable, got: {out}"
    );
}

#[test]
fn listable_times_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Times], Listable]");
    assert!(
        out.contains("True"),
        "Times should have Listable, got: {out}"
    );
}

#[test]
fn listable_sin_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Sin], Listable]");
    assert!(out.contains("True"), "Sin should have Listable, got: {out}");
}

#[test]
fn listable_power_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Power], Listable]");
    assert!(
        out.contains("True"),
        "Power should have Listable, got: {out}"
    );
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
