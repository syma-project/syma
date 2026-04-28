/// Comparison and logical operations integration tests.

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── Equal ──

#[test]
fn test_equal_true() {
    let out = syma_eval("1 == 1");
    assert!(out.contains("True"), "1 == 1 should be True, got: {out}");
}

#[test]
fn test_equal_false() {
    let out = syma_eval("1 == 2");
    assert!(out.contains("False"), "1 == 2 should be False, got: {out}");
}

// ── Unequal ──

#[test]
fn test_unequal_true() {
    let out = syma_eval("1 != 2");
    assert!(out.contains("True"), "1 != 2 should be True, got: {out}");
}

#[test]
fn test_unequal_false() {
    let out = syma_eval("1 != 1");
    assert!(out.contains("False"), "1 != 1 should be False, got: {out}");
}

// ── Less / Greater ──

#[test]
fn test_less_true() {
    let out = syma_eval("1 < 2");
    assert!(out.contains("True"), "1 < 2 should be True, got: {out}");
}

#[test]
fn test_less_false() {
    let out = syma_eval("2 < 1");
    assert!(out.contains("False"), "2 < 1 should be False, got: {out}");
}

#[test]
fn test_greater_true() {
    let out = syma_eval("3 > 2");
    assert!(out.contains("True"), "3 > 2 should be True, got: {out}");
}

#[test]
fn test_greater_false() {
    let out = syma_eval("2 > 3");
    assert!(out.contains("False"), "2 > 3 should be False, got: {out}");
}

// ── And ──

#[test]
fn test_and_true_true() {
    let out = syma_eval("True && True");
    assert!(out.contains("True"), "True && True should be True, got: {out}");
}

#[test]
fn test_and_true_false() {
    let out = syma_eval("True && False");
    assert!(out.contains("False"), "True && False should be False, got: {out}");
}

// ── Or ──

#[test]
fn test_or_false_true() {
    let out = syma_eval("False || True");
    assert!(out.contains("True"), "False || True should be True, got: {out}");
}

#[test]
fn test_or_false_false() {
    let out = syma_eval("False || False");
    assert!(out.contains("False"), "False || False should be False, got: {out}");
}

// ── Not ──

#[test]
fn test_not_true() {
    let out = syma_eval("!True");
    assert!(out.contains("False"), "!True should be False, got: {out}");
}

#[test]
fn test_not_false() {
    let out = syma_eval("!False");
    assert!(out.contains("True"), "!False should be True, got: {out}");
}

// ── Chained comparisons ──

#[test]
fn test_chained_comparison_and() {
    let out = syma_eval("1 < 2 && 3 > 2");
    assert!(out.contains("True"), "1 < 2 && 3 > 2 should be True, got: {out}");
}

#[test]
fn test_chained_comparison_mixed() {
    let out = syma_eval("1 < 2 && 3 < 2");
    assert!(out.contains("False"), "1 < 2 && 3 < 2 should be False, got: {out}");
}

// ── Comparison with arithmetic ──

#[test]
fn test_comparison_with_arithmetic() {
    let out = syma_eval("1 + 1 == 2");
    assert!(out.contains("True"), "1 + 1 == 2 should be True, got: {out}");
}

#[test]
fn test_comparison_with_variables() {
    let out = syma_eval("x = 5; x > 3");
    assert!(out.contains("True"), "x=5; x > 3 should be True, got: {out}");
}

// ── String comparison ──

#[test]
fn test_string_equal() {
    let out = syma_eval(r#""hello" == "hello""#);
    assert!(out.contains("True"), "\"hello\" == \"hello\" should be True, got: {out}");
}

#[test]
fn test_string_not_equal() {
    let out = syma_eval(r#""hello" == "world""#);
    assert!(out.contains("False"), "\"hello\" == \"world\" should be False, got: {out}");
}
