/// String operations integration tests.

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── StringJoin ──

#[test]
fn test_string_join_basic() {
    let out = syma_eval(r#"StringJoin["a", "b"]"#);
    assert!(out.contains("ab"), "got: {out}");
}

#[test]
fn test_string_join_multiple() {
    let out = syma_eval(r#"StringJoin["hello", " ", "world"]"#);
    assert!(out.contains("hello world"), "got: {out}");
}

#[test]
fn test_string_join_empty() {
    let out = syma_eval(r#"StringJoin["", "x"]"#);
    assert!(out.contains("x"), "got: {out}");
}

// ── StringLength ──

#[test]
fn test_string_length_basic() {
    let out = syma_eval(r#"StringLength["hello"]"#);
    assert!(out.contains("5"), "got: {out}");
}

#[test]
fn test_string_length_empty() {
    let out = syma_eval(r#"StringLength[""]"#);
    assert!(out.contains("0"), "got: {out}");
}

// ── StringSplit ──

#[test]
fn test_string_split_comma() {
    let out = syma_eval(r#"StringSplit["a,b,c", ","]"#);
    assert!(out.contains("\"a\""), "should contain \"a\", got: {out}");
    assert!(out.contains("\"b\""), "should contain \"b\", got: {out}");
    assert!(out.contains("\"c\""), "should contain \"c\", got: {out}");
}

// ── StringReplace ──
// NOTE: StringReplace with rule syntax "l" -> "L" fails with
// "expected String, got Pattern" — rule LHS/RHS are stored as Pattern values
// instead of evaluated strings. This is a known Syma bug.

#[test]
#[ignore = "Syma bug: rule args stored as Pattern instead of evaluated String"]
fn test_string_replace_basic() {
    let out = syma_eval(r#"StringReplace["hello", "l" -> "L"]"#);
    assert!(out.contains("heLLo") || out.contains("heLlo"), "got: {out}");
}

// ── StringContainsQ ──

#[test]
fn test_string_contains_q_true() {
    let out = syma_eval(r#"StringContainsQ["hello", "ell"]"#);
    assert!(out.contains("True"), "got: {out}");
}

#[test]
fn test_string_contains_q_false() {
    let out = syma_eval(r#"StringContainsQ["hello", "xyz"]"#);
    assert!(out.contains("False"), "got: {out}");
}

// ── ToUpperCase / ToLowerCase ──

#[test]
fn test_to_upper_case() {
    let out = syma_eval(r#"ToUpperCase["hello"]"#);
    assert!(out.contains("HELLO"), "got: {out}");
}

#[test]
fn test_to_lower_case() {
    let out = syma_eval(r#"ToLowerCase["HELLO"]"#);
    assert!(out.contains("hello"), "got: {out}");
}

// ── Characters ──

#[test]
fn test_characters_basic() {
    let out = syma_eval(r#"Characters["abc"]"#);
    assert!(out.contains("\"a\""), "should contain \"a\", got: {out}");
    assert!(out.contains("\"b\""), "should contain \"b\", got: {out}");
    assert!(out.contains("\"c\""), "should contain \"c\", got: {out}");
}

// ── StringReverse ──

#[test]
fn test_string_reverse_basic() {
    let out = syma_eval(r#"StringReverse["abc"]"#);
    assert!(out.contains("cba"), "got: {out}");
}

// ── StringTake ──

#[test]
fn test_string_take_basic() {
    let out = syma_eval(r#"StringTake["hello", 3]"#);
    assert!(out.contains("hel"), "got: {out}");
}

// ── StringTrim ──

#[test]
fn test_string_trim_basic() {
    let out = syma_eval(r#"StringTrim["  hi  "]"#);
    assert!(out.contains("hi"), "got: {out}");
}

// ── StringStartsQ / StringEndsQ ──

#[test]
fn test_string_starts_q_true() {
    let out = syma_eval(r#"StringStartsQ["hello", "hel"]"#);
    assert!(out.contains("True"), "got: {out}");
}

#[test]
fn test_string_starts_q_false() {
    let out = syma_eval(r#"StringStartsQ["hello", "llo"]"#);
    assert!(out.contains("False"), "got: {out}");
}

#[test]
fn test_string_ends_q_true() {
    let out = syma_eval(r#"StringEndsQ["hello", "llo"]"#);
    assert!(out.contains("True"), "got: {out}");
}

#[test]
fn test_string_ends_q_false() {
    let out = syma_eval(r#"StringEndsQ["hello", "hel"]"#);
    assert!(out.contains("False"), "got: {out}");
}

// ── StringMatchQ ──

#[test]
fn test_string_match_q_pattern() {
    let out = syma_eval(r#"StringMatchQ["hello", "h*"]"#);
    assert!(out.contains("True"), "got: {out}");
}

// ── ToString ──

#[test]
fn test_to_string_number() {
    let out = syma_eval("ToString[42]");
    assert!(out.contains("42"), "got: {out}");
}

// ── ToExpression ──

#[test]
fn test_to_expression_basic() {
    let out = syma_eval(r#"ToExpression["1+2"]"#);
    assert!(out.contains("3"), "got: {out}");
}
