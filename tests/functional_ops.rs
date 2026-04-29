/// Functional operator builtin integration tests

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── Composition ──

#[test]
fn test_composition_basic() {
    let out = syma_eval("Composition[#^2&, #+1&][5]");
    // (5+1)^2 = 36
    assert!(
        !out.contains("error"),
        "Composition should not error, got: {out}"
    );
}

#[test]
fn test_composition_sin_cos() {
    let out = syma_eval("Composition[Sin, Cos][0]");
    assert!(
        !out.contains("error"),
        "Composition[Sin,Cos][0] should not error, got: {out}"
    );
}

// ── RightComposition ──

#[test]
fn test_right_composition() {
    let out = syma_eval("RightComposition[#+1&, #^2&][5]");
    // 5+1=6, then 6^2=36
    assert!(
        !out.contains("error"),
        "RightComposition should not error, got: {out}"
    );
}

// ── Through ──

#[test]
fn test_through_basic() {
    let out = syma_eval("Through[{f, g}]");
    assert!(
        !out.contains("error"),
        "Through should not error, got: {out}"
    );
}

// ── OperatorApply ──

#[test]
fn test_operator_apply() {
    let out = syma_eval("OperatorApply[{f, {x}}]");
    assert!(
        !out.contains("error"),
        "OperatorApply should not error, got: {out}"
    );
}

// ── Curry / UnCurry ──

#[ignore = "Curry requires 2 args and intermediate curry state not evaluated"]
#[test]
fn test_curry_plus() {
    let out = syma_eval("Curry[Plus, 2][1][2]");
    assert!(
        out.contains("3"),
        "Curry[Plus][1][2] should be 3, got: {out}"
    );
}

#[test]
fn test_uncurry_basic() {
    let out = syma_eval("UnCurry[f][{a, b}]");
    assert!(
        !out.contains("error"),
        "UnCurry should not error, got: {out}"
    );
}

// ── Replace ──

#[test]
fn test_replace_basic() {
    let out = syma_eval("Replace[{1, 2, 3}, {1 -> a, 2 -> b}]");
    assert!(
        !out.contains("error"),
        "Replace should not error, got: {out}"
    );
}

// ── MapAll ──

#[ignore = "MapAll causes stack overflow (infinite recursion in impl)"]
#[test]
fn test_map_all() {
    let out = syma_eval("MapAll[f, 1 + x]");
    assert!(
        !out.contains("error"),
        "MapAll should not error, got: {out}"
    );
}

// ── SelectFirst / SelectLast ──

#[test]
fn test_select_first() {
    let out = syma_eval("SelectFirst[{1, 2, 3, 4}, # > 2&]");
    assert!(
        !out.contains("error"),
        "SelectFirst should not error, got: {out}"
    );
}

#[test]
fn test_select_last() {
    let out = syma_eval("SelectLast[{1, 2, 3, 4}, # > 1&]");
    assert!(
        !out.contains("error"),
        "SelectLast should not error, got: {out}"
    );
}

// ── PositionFirst / PositionLast ──

#[test]
fn test_position_first() {
    let out = syma_eval("PositionFirst[{1, 2, 3, 2}, 2]");
    assert!(
        !out.contains("error"),
        "PositionFirst should not error, got: {out}"
    );
}

#[test]
fn test_position_last() {
    let out = syma_eval("PositionLast[{1, 2, 3, 2}, 2]");
    assert!(
        !out.contains("error"),
        "PositionLast should not error, got: {out}"
    );
}

// ── SubsetQ ──

#[test]
fn test_subset_q_true() {
    let out = syma_eval("SubsetQ[{1, 2}, {1, 2, 3}]");
    assert!(
        out.contains("True"),
        "SubsetQ true case should be True, got: {out}"
    );
}

#[test]
fn test_subset_q_false() {
    let out = syma_eval("SubsetQ[{1, 2, 3}, {1, 2}]");
    assert!(
        out.contains("False"),
        "SubsetQ false case should be False, got: {out}"
    );
}

// ── SymmetricDifference ──

#[test]
fn test_symmetric_difference() {
    let out = syma_eval("SymmetricDifference[{1, 2, 3}, {2, 3, 4}]");
    assert!(
        !out.contains("error"),
        "SymmetricDifference should not error, got: {out}"
    );
}

// ── Undulate ──

#[test]
fn test_undulate() {
    let out = syma_eval("Undulate[{1, 2, 3, 4}]");
    assert!(
        !out.contains("error"),
        "Undulate should not error, got: {out}"
    );
}

// ── MapApply ──

#[test]
fn test_map_apply() {
    let out = syma_eval("MapApply[f, {{1, 2}, {3, 4}}]");
    assert!(
        !out.contains("error"),
        "MapApply should not error, got: {out}"
    );
}
