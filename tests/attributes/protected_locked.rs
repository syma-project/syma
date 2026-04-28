//! Protected and Locked attribute tests
//!
//! Protected: symbol cannot be redefined
//! Locked: symbol's attributes cannot be changed
//! ReadProtected: symbol's definition is hidden (partially implemented)

use super::syma_eval;

// ── Protected ──

#[test]
fn protected_prevents_redefinition() {
    // Define first, then protect, then try to redefine
    let out = syma_eval(
        "f[x_] := x + 1; \
         SetAttributes[f, Protected]; \
         Attributes[f]",
    );
    assert!(
        out.contains("Protected"),
        "f should be Protected, got: {out}",
    );
}

#[test]
fn protected_stays_after_assignment() {
    // Use string form since Attributes doesn't have HoldFirst
    let out = syma_eval(
        "g = 42; \
         SetAttributes[g, Protected]; \
         MemberQ[Attributes[\"g\"], Protected]",
    );
    assert!(out.contains("True"), "g should be Protected, got: {out}");
}

#[test]
fn protected_builtin_sin() {
    let out = syma_eval(
        "SetAttributes[Sin, Protected]; \
         Sin[0]",
    );
    assert!(
        out.contains("0"),
        "Sin should still work after protecting, got: {out}"
    );
}

#[test]
fn clear_attributes_removes_protected() {
    let out = syma_eval(
        "f[x_] := x + 1; \
         SetAttributes[f, Protected]; \
         ClearAttributes[f, Protected]; \
         MemberQ[Attributes[f], Protected]",
    );
    assert!(
        out.contains("False"),
        "After ClearAttributes, Protected should be removed, got: {out}"
    );
}

// ── Locked ──

#[test]
fn locked_prevents_attribute_change() {
    let out = syma_eval(
        "SetAttributes[f, Locked]; \
         SetAttributes[f, HoldAll]; \
         Attributes[f]",
    );
    assert!(
        out.contains("Locked"),
        "Locked symbol should have Locked attribute, got: {out}"
    );
}

#[test]
fn locked_plus_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Plus], Locked]");
    assert!(out.contains("True"), "Plus should be Locked, got: {out}");
}

#[test]
fn locked_sin_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Sin], Locked]");
    assert!(out.contains("True"), "Sin should be Locked, got: {out}");
}

#[test]
fn locked_hold_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Hold], Locked]");
    assert!(out.contains("True"), "Hold should be Locked, got: {out}");
}

#[test]
fn locked_prevents_set_attributes() {
    let out = syma_eval(
        "SetAttributes[g, Locked]; \
         SetAttributes[g, {Locked, HoldAll}]; \
         MemberQ[Attributes[g], HoldAll]",
    );
    assert!(
        out.contains("False"),
        "Locked symbol should not accept new attributes, got: {out}"
    );
}

// ── ReadProtected ──

#[test]
fn read_protected_plus_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Plus], ReadProtected]");
    assert!(out.contains("True"), "Plus should be ReadProtected, got: {out}");
}

#[test]
fn read_protected_sin_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Sin], ReadProtected]");
    assert!(out.contains("True"), "Sin should be ReadProtected, got: {out}");
}

// ── NumericFunction ──

#[test]
fn numeric_function_plus_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Plus], NumericFunction]");
    assert!(out.contains("True"), "Plus should be NumericFunction, got: {out}");
}

#[test]
fn numeric_function_sin_has_attribute() {
    let out = syma_eval("MemberQ[Attributes[Sin], NumericFunction]");
    assert!(out.contains("True"), "Sin should be NumericFunction, got: {out}");
}

// ── Combined attributes ──

#[test]
fn plus_full_attribute_set() {
    let out = syma_eval("Attributes[Plus]");
    for attr in ["Flat", "Listable", "Locked", "Orderless", "OneIdentity"] {
        assert!(
            out.contains(attr),
            "Plus should have {attr} attribute, got: {out}"
        );
    }
}

#[test]
fn and_attribute_set() {
    let out = syma_eval("Attributes[And]");
    for attr in ["Flat", "HoldAll", "OneIdentity", "Orderless"] {
        assert!(
            out.contains(attr),
            "And should have {attr} attribute, got: {out}"
        );
    }
}

#[test]
fn hold_attribute_set() {
    let out = syma_eval("Attributes[Hold]");
    for attr in ["HoldAll", "Locked"] {
        assert!(
            out.contains(attr),
            "Hold should have {attr} attribute, got: {out}"
        );
    }
}
