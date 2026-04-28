/// List operations integration tests.

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── Length ──

#[test]
fn test_length_basic() {
    let out = syma_eval("Length[{1, 2, 3}]");
    assert!(out.contains("3"), "got: {out}");
}

#[test]
fn test_length_empty() {
    let out = syma_eval("Length[{}]");
    assert!(out.contains("0"), "got: {out}");
}

// ── First / Last ──

#[test]
fn test_first_basic() {
    let out = syma_eval("First[{1, 2, 3}]");
    assert!(out.contains("1"), "got: {out}");
}

#[test]
fn test_last_basic() {
    let out = syma_eval("Last[{1, 2, 3}]");
    assert!(out.contains("3"), "got: {out}");
}

// ── Rest ──

#[test]
fn test_rest_basic() {
    let out = syma_eval("Rest[{1, 2, 3}]");
    assert!(out.contains("2"), "got: {out}");
    assert!(out.contains("3"), "got: {out}");
    // Should not contain 1
    assert!(!out.contains("{1,"), "should not contain first element, got: {out}");
}

// ── Append ──

#[test]
fn test_append_basic() {
    let out = syma_eval("Append[{1, 2}, 3]");
    assert!(out.contains("1"), "got: {out}");
    assert!(out.contains("2"), "got: {out}");
    assert!(out.contains("3"), "got: {out}");
}

// ── Join ──

#[test]
fn test_join_basic() {
    let out = syma_eval("Join[{1, 2}, {3, 4}]");
    assert!(out.contains("1"), "got: {out}");
    assert!(out.contains("4"), "got: {out}");
}

// ── Flatten ──

#[test]
fn test_flatten_nested() {
    let out = syma_eval("Flatten[{{1, 2}, {3, 4}}]");
    assert!(out.contains("1"), "got: {out}");
    assert!(out.contains("4"), "got: {out}");
    // Should be a flat list, not nested
    assert!(!out.contains("{{"), "should not contain nested braces, got: {out}");
}

// ── Sort ──

#[test]
fn test_sort_basic() {
    let out = syma_eval("Sort[{3, 1, 2}]");
    assert!(out.contains("1"), "got: {out}");
    assert!(out.contains("2"), "got: {out}");
    assert!(out.contains("3"), "got: {out}");
    // Check order: 1 should come before 2, which should come before 3
    let pos1 = out.find('1').unwrap();
    let pos2 = out.find('2').unwrap();
    let pos3 = out.find('3').unwrap();
    assert!(pos1 < pos2 && pos2 < pos3, "Sort order wrong: {out}");
}

// ── Reverse ──

#[test]
fn test_reverse_basic() {
    let out = syma_eval("Reverse[{1, 2, 3}]");
    assert!(out.contains("1"), "got: {out}");
    assert!(out.contains("3"), "got: {out}");
    // 3 should come before 1
    let pos3 = out.find('3').unwrap();
    let pos1 = out.find('1').unwrap();
    assert!(pos3 < pos1, "Reverse order wrong: {out}");
}

// ── Part ──

#[test]
fn test_part_basic() {
    let out = syma_eval("Part[{a, b, c}, 2]");
    assert!(out.contains("b"), "got: {out}");
}

// ── Range ──

#[test]
fn test_range_basic() {
    let out = syma_eval("Range[5]");
    assert!(out.contains("1"), "got: {out}");
    assert!(out.contains("5"), "got: {out}");
}

#[test]
fn test_range_single() {
    let out = syma_eval("Range[1]");
    assert!(out.contains("1"), "got: {out}");
}

// ── Take / Drop ──

#[test]
fn test_take_basic() {
    let out = syma_eval("Take[{1, 2, 3, 4}, 2]");
    assert!(out.contains("1"), "got: {out}");
    assert!(out.contains("2"), "got: {out}");
    assert!(!out.contains("3,"), "should not contain 3, got: {out}");
}

#[test]
fn test_drop_basic() {
    let out = syma_eval("Drop[{1, 2, 3, 4}, 2]");
    assert!(out.contains("3"), "got: {out}");
    assert!(out.contains("4"), "got: {out}");
    // Should not contain 1 or 2 as elements
    assert!(!out.contains("{1,"), "should not contain 1, got: {out}");
}

// ── Total ──

#[test]
fn test_total_basic() {
    let out = syma_eval("Total[{1, 2, 3}]");
    assert!(out.contains("6"), "got: {out}");
}

// ── MemberQ ──

#[test]
fn test_member_q_true() {
    let out = syma_eval("MemberQ[{1, 2, 3}, 2]");
    assert!(out.contains("True"), "got: {out}");
}

#[test]
fn test_member_q_false() {
    let out = syma_eval("MemberQ[{1, 2, 3}, 5]");
    assert!(out.contains("False"), "got: {out}");
}

// ── Count ──

#[test]
fn test_count_basic() {
    let out = syma_eval("Count[{1, 2, 1}, 1]");
    assert!(out.contains("2"), "got: {out}");
}

// ── Union ──

#[test]
fn test_union_basic() {
    let out = syma_eval("Union[{1, 2}, {2, 3}]");
    assert!(out.contains("1"), "got: {out}");
    assert!(out.contains("2"), "got: {out}");
    assert!(out.contains("3"), "got: {out}");
}

// ── Intersection ──

#[test]
fn test_intersection_basic() {
    let out = syma_eval("Intersection[{1, 2}, {2, 3}]");
    assert!(out.contains("2"), "got: {out}");
    assert!(!out.contains("1,"), "should not contain 1, got: {out}");
    assert!(!out.contains("3"), "should not contain 3, got: {out}");
}

// ── Transpose ──

#[test]
fn test_transpose_basic() {
    let out = syma_eval("Transpose[{{1, 2}, {3, 4}}]");
    assert!(out.contains("1"), "got: {out}");
    assert!(out.contains("4"), "got: {out}");
}
