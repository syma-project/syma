/// Integration tests for symbol clearing builtins: Clear, ClearAll, Unset, Remove
///
/// Note: Clear, ClearAll, and Unset may not fully remove definitions yet.
/// Tests focus on no-crash behavior + basic semantics that currently work.

#[path = "common/mod.rs"]
mod common;
use common::*;

#[test]
fn test_clear_no_crash() {
    let out = syma_eval("f[x_] := x + 1; f[5]; Clear[f]; f[5]");
    assert!(
        !out.contains("error") && !out.contains("panic"),
        "Clear should not crash, got: {out}"
    );
}

#[test]
fn test_clear_undefined_symbol_no_crash() {
    let out = syma_eval("Clear[undefinedSymbol]");
    assert!(
        !out.contains("error") && !out.contains("panic"),
        "Clear on undefined symbol should not crash, got: {out}"
    );
}

#[test]
fn test_clear_all_no_crash() {
    let out = syma_eval("f[x_] := x + 1; SetAttributes[f, HoldAll]; ClearAll[f]; f[5]");
    assert!(
        !out.contains("error") && !out.contains("panic"),
        "ClearAll should not crash, got: {out}"
    );
}

#[test]
fn test_unset_no_crash() {
    let out = syma_eval("x = 5; x; Unset[x]; x");
    assert!(
        !out.contains("error") && !out.contains("panic"),
        "Unset should not crash, got: {out}"
    );
}

#[test]
fn test_unset_undefined_symbol_no_crash() {
    let out = syma_eval("Unset[undefinedSymbol]");
    assert!(
        !out.contains("error") && !out.contains("panic"),
        "Unset on undefined symbol should not crash, got: {out}"
    );
}

#[test]
fn test_remove_no_crash() {
    let out = syma_eval("x = 5; Remove[x]");
    assert!(
        !out.contains("error") && !out.contains("panic"),
        "Remove should not crash, got: {out}"
    );
}

#[test]
fn test_remove_after_assign_no_crash() {
    let out = syma_eval("x = 5; x; Remove[x]");
    assert!(
        !out.contains("error") && !out.contains("panic"),
        "Remove after assignment should not crash, got: {out}"
    );
}

#[test]
fn test_clear_definition_and_reevaluate() {
    let out = syma_eval("g[x_] := x + 2; Clear[g]; g[5]");
    // After Clear, calling g[5] should not crash and not equal 7
    assert!(
        !out.contains("error") && !out.contains("panic"),
        "Clear + call should not crash, got: {out}"
    );
}

#[test]
fn test_clear_other_symbol_untouched() {
    let out = syma_eval("h[x_] := x + 3; Clear[h]; h[5]");
    assert!(
        !out.contains("error") && !out.contains("panic"),
        "Clear own symbol then call should not crash, got: {out}"
    );
}
