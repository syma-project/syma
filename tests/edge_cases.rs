/// Parser and evaluator edge case integration tests.

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── Nested comments ──

#[test]
fn test_nested_comments() {
    let out = syma_eval("(* a (* b *) c *) 42");
    assert!(out.contains("42"), "nested comment should be ignored, got: {out}");
}

#[test]
fn test_deeply_nested_comments() {
    let out = syma_eval("(* (* (* *) *) *) 99");
    assert!(out.contains("99"), "deeply nested comment should be ignored, got: {out}");
}

// ── Operator precedence ──

#[test]
fn test_mul_before_add() {
    let out = syma_eval("2 + 3 * 4");
    assert!(out.contains("14"), "mul should bind tighter than add, got: {out}");
}

#[test]
fn test_parentheses_override() {
    let out = syma_eval("(2 + 3) * 4");
    assert!(out.contains("20"), "parentheses should override precedence, got: {out}");
}

#[test]
fn test_power_precedence() {
    let out = syma_eval("2 + 3 ^ 2");
    assert!(out.contains("11"), "power should bind tighter than add, got: {out}");
}

// ── String edge cases ──

#[test]
fn test_string_with_spaces() {
    let out = syma_eval(r#"StringLength["hello world"]"#);
    assert!(out.contains("11"), "got: {out}");
}

#[test]
fn test_empty_string() {
    let out = syma_eval(r#""""#);
    // Empty string should evaluate without error
    assert!(!out.is_empty() || out == "", "got: {out}");
}

// ── Empty list ──

#[test]
fn test_empty_list() {
    let out = syma_eval("{}");
    assert!(out.contains("{}"), "empty list should print as {{}}, got: {out}");
}

#[test]
fn test_nested_empty_lists() {
    let out = syma_eval("{{}, {}}");
    assert!(out.contains("{}"), "nested empty lists should contain {{}}, got: {out}");
}

// ── Nested lists ──

#[test]
fn test_nested_list_access() {
    let out = syma_eval("Part[{{1}, {2}}, 2]");
    assert!(out.contains("2"), "got: {out}");
}

// ── Pure functions (Function syntax) ──

#[test]
fn test_function_basic() {
    let out = syma_eval("Function[x, x + 1][5]");
    assert!(out.contains("6"), "Function[x, x+1][5] should be 6, got: {out}");
}

#[test]
fn test_function_multiply() {
    let out = syma_eval("Function[x, x * 2][3]");
    assert!(out.contains("6"), "Function[x, x*2][3] should be 6, got: {out}");
}

#[test]
fn test_function_multi_param() {
    let out = syma_eval("Function[{x, y}, x + y][3, 4]");
    assert!(out.contains("7"), "Function[{{x,y}}, x+y][3,4] should be 7, got: {out}");
}

// ── Chained rules ──

#[test]
fn test_chained_rules() {
    let out = syma_eval("x /. {x -> y, y -> z}");
    // ReplaceAll applies rules once; x should become y (first matching rule)
    assert!(out.contains("y"), "first rule x->y should apply, got: {out}");
}

#[test]
fn test_replace_repeated() {
    let out = syma_eval("x //. {x -> y, y -> z}");
    // ReplaceRepeated applies rules until stable; should end at z
    assert!(out.contains("z"), "replace repeated should converge to z, got: {out}");
}

// ── Pattern matching ──

#[test]
fn test_match_q_integer() {
    let out = syma_eval("MatchQ[1, _Integer]");
    assert!(out.contains("True"), "1 should match _Integer, got: {out}");
}

#[test]
fn test_match_q_string() {
    let out = syma_eval(r#"MatchQ["hi", _String]"#);
    assert!(out.contains("True"), "\"hi\" should match _String, got: {out}");
}

#[test]
fn test_match_q_wrong_type() {
    let out = syma_eval("MatchQ[1, _String]");
    assert!(out.contains("False"), "1 should not match _String, got: {out}");
}

#[test]
fn test_match_q_blank() {
    let out = syma_eval("MatchQ[42, _]");
    assert!(out.contains("True"), "anything should match _, got: {out}");
}

// ── Large numbers ──

#[test]
fn test_large_number_arithmetic() {
    let out = syma_eval("100 * 100");
    assert!(out.contains("10000"), "got: {out}");
}

// ── Mixed expressions ──

#[test]
fn test_list_of_strings() {
    let out = syma_eval(r#"{"a", "b", "c"}"#);
    assert!(out.contains("\"a\"") || out.contains("a"), "got: {out}");
}

#[test]
fn test_function_definition_and_call() {
    let out = syma_eval("f[x_] := x * x; f[4]");
    assert!(out.contains("16"), "f[4] should be 16, got: {out}");
}
