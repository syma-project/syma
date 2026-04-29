/// Association builtin integration tests

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── Association basics ──

#[test]
fn test_association_basic() {
    // Just confirm <| |> syntax parses and evaluates
    let out = syma_eval("<|\"a\" -> 1, \"b\" -> 2|>");
    assert!(
        !out.contains("error"),
        "Association literal should not error, got: {out}"
    );
}

// ── Keys / Values ──

#[test]
fn test_keys_basic() {
    let out = syma_eval("Keys[<|\"a\" -> 1, \"b\" -> 2|>]");
    assert!(!out.contains("error"), "Keys should not error, got: {out}");
}

#[test]
fn test_values_basic() {
    let out = syma_eval("Values[<|\"a\" -> 1, \"b\" -> 2|>]");
    assert!(
        !out.contains("error"),
        "Values should not error, got: {out}"
    );
}

// ── Lookup ──

#[test]
fn test_lookup_exists() {
    let out = syma_eval("Lookup[<|\"a\" -> 1|>, \"a\"]");
    assert!(out.contains("1"), "Lookup should return 1, got: {out}");
}

#[test]
fn test_lookup_missing() {
    let out = syma_eval("Lookup[<|\"a\" -> 1|>, \"b\"]");
    assert!(
        !out.contains("error"),
        "Lookup missing key should not error, got: {out}"
    );
}

// ── KeyExistsQ / AssociationQ ──

#[test]
fn test_key_exists_q_true() {
    let out = syma_eval("KeyExistsQ[<|\"a\" -> 1|>, \"a\"]");
    assert!(
        out.contains("True"),
        "KeyExistsQ should be True, got: {out}"
    );
}

#[test]
fn test_key_exists_q_false() {
    let out = syma_eval("KeyExistsQ[<|\"a\" -> 1|>, \"b\"]");
    assert!(
        out.contains("False"),
        "KeyExistsQ should be False, got: {out}"
    );
}

#[test]
fn test_association_q_true() {
    let out = syma_eval("AssociationQ[<|\"a\" -> 1|>]");
    assert!(
        out.contains("True"),
        "AssociationQ should be True, got: {out}"
    );
}

#[test]
fn test_association_q_false() {
    let out = syma_eval("AssociationQ[5]");
    assert!(
        out.contains("False"),
        "AssociationQ[5] should be False, got: {out}"
    );
}

// ── Normal ──

#[test]
fn test_normal_association() {
    let out = syma_eval("Normal[<|\"a\" -> 1|>]");
    assert!(
        !out.contains("error"),
        "Normal should not error, got: {out}"
    );
}

// ── KeySort ──

#[test]
fn test_key_sort_basic() {
    let out = syma_eval("KeySort[<|\"b\" -> 2, \"a\" -> 1|>]");
    assert!(
        !out.contains("error"),
        "KeySort should not error, got: {out}"
    );
}

// ── KeyDrop / KeyTake ──

#[test]
fn test_key_drop() {
    let out = syma_eval("KeyDrop[<|\"a\" -> 1, \"b\" -> 2|>, \"a\"]");
    assert!(
        !out.contains("error"),
        "KeyDrop should not error, got: {out}"
    );
}

#[test]
fn test_key_take() {
    let out = syma_eval("KeyTake[<|\"a\" -> 1, \"b\" -> 2|>, {\"a\"}]");
    assert!(
        !out.contains("error"),
        "KeyTake should not error, got: {out}"
    );
}

// ── Counts / GroupBy ──

#[test]
fn test_counts_basic() {
    let out = syma_eval("Counts[{1, 2, 1, 3, 1}]");
    assert!(
        !out.contains("error"),
        "Counts should not error, got: {out}"
    );
}

#[test]
fn test_group_by() {
    let out = syma_eval("GroupBy[{1, 2, 3, 4}, EvenQ]");
    assert!(
        !out.contains("error"),
        "GroupBy should not error, got: {out}"
    );
}

// ── KeyFreeQ / KeyMemberQ ──

#[test]
fn test_key_free_q_true() {
    let out = syma_eval("KeyFreeQ[<|\"a\" -> 1|>, \"b\"]");
    assert!(
        out.contains("True"),
        "KeyFreeQ missing key should be True, got: {out}"
    );
}

#[test]
fn test_key_free_q_false() {
    let out = syma_eval("KeyFreeQ[<|\"a\" -> 1|>, \"a\"]");
    assert!(
        out.contains("False"),
        "KeyFreeQ existing key should be False, got: {out}"
    );
}

// ── Merge ──

#[test]
fn test_merge_basic() {
    let out = syma_eval("Merge[{<|\"a\" -> 1|>, <|\"a\" -> 2|>}, Total]");
    assert!(!out.contains("error"), "Merge should not error, got: {out}");
}

// ── KeyUnion / KeyIntersection ──

#[test]
fn test_key_union() {
    let out = syma_eval("KeyUnion[{<|\"a\" -> 1|>, <|\"b\" -> 2|>}]");
    assert!(
        !out.contains("error"),
        "KeyUnion should not error, got: {out}"
    );
}

#[test]
fn test_key_intersection() {
    let out =
        syma_eval("KeyIntersection[{<|\"a\" -> 1, \"b\" -> 2|>, <|\"a\" -> 3, \"c\" -> 4|>}]");
    assert!(
        !out.contains("error"),
        "KeyIntersection should not error, got: {out}"
    );
}

// ── AssociateTo ──

#[test]
fn test_associate_to() {
    let out = syma_eval("AssociateTo[<|\"a\" -> 1|>, \"b\" -> 2]");
    assert!(
        !out.contains("error"),
        "AssociateTo should not error, got: {out}"
    );
}

// ── KeyDropFrom ──

#[test]
fn test_key_drop_from() {
    let out = syma_eval("KeyDropFrom[<|\"a\" -> 1, \"b\" -> 2|>, \"a\"]");
    assert!(
        !out.contains("error"),
        "KeyDropFrom should not error, got: {out}"
    );
}

// ── CountsBy ──

#[test]
fn test_counts_by_basic() {
    let out = syma_eval("CountsBy[{1, 2, 3, 4, 5, 6}, EvenQ]");
    assert!(
        !out.contains("error"),
        "CountsBy should not error, got: {out}"
    );
}

// ── KeyComplement ──

#[test]
fn test_key_complement() {
    let out = syma_eval("KeyComplement[<|\"a\" -> 1, \"b\" -> 2|>, <|\"a\" -> 3|>]");
    assert!(
        !out.contains("error"),
        "KeyComplement should not error, got: {out}"
    );
}
