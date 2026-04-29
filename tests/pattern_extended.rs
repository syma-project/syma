/// Extended pattern matching integration tests

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── Cases ──

#[test]
fn test_cases_integers() {
    let out = syma_eval("Cases[{1, a, 2, b, 3}, _Integer]");
    assert!(
        out.contains("1") && out.contains("2") && out.contains("3"),
        "Cases should extract integers, got: {out}"
    );
}

#[test]
fn test_cases_with_condition() {
    let out = syma_eval("Cases[{1, 2, 3, 4, 5}, x_ /; x > 2]");
    assert!(
        !out.contains("error"),
        "Cases with condition should not error, got: {out}"
    );
}

// ── DeleteCases ──

#[test]
fn test_delete_cases_integers() {
    let out = syma_eval("DeleteCases[{1, a, 2, b, 3}, _Integer]");
    assert!(
        !out.contains("error"),
        "DeleteCases should not error, got: {out}"
    );
}

#[test]
fn test_delete_cases_even() {
    let out = syma_eval("DeleteCases[{1, 2, 3, 4}, _?EvenQ]");
    assert!(
        !out.contains("error"),
        "DeleteCases with EvenQ should not error, got: {out}"
    );
}

// ── FreeQ ──

#[test]
fn test_free_q_true() {
    let out = syma_eval("FreeQ[{1, 2, 3}, 4]");
    assert!(out.contains("True"), "FreeQ should be True, got: {out}");
}

#[test]
fn test_free_q_false() {
    let out = syma_eval("FreeQ[{1, 2, 3}, 2]");
    assert!(out.contains("False"), "FreeQ should be False, got: {out}");
}

#[test]
fn test_free_q_nested() {
    let out = syma_eval("FreeQ[{1, {2, 3}}, 3]");
    assert!(
        out.contains("False"),
        "FreeQ nested should be False, got: {out}"
    );
}

// ── Dispatch ──

#[test]
fn test_dispatch_basic() {
    let out = syma_eval("Dispatch[{a -> 1, b -> 2}]");
    assert!(
        !out.contains("error"),
        "Dispatch should not error, got: {out}"
    );
}

// ── Alternatives ──

#[ignore = "Pattern alternatives | not supported in parser"]
#[test]
fn test_match_q_alternatives_true() {
    let out = syma_eval("MatchQ[1, 1 | 2 | 3]");
    assert!(
        out.contains("True"),
        "MatchQ alternative true should be True, got: {out}"
    );
}

#[ignore = "Pattern alternatives | not supported in parser"]
#[test]
fn test_match_q_alternatives_false() {
    let out = syma_eval("MatchQ[4, 1 | 2 | 3]");
    assert!(
        out.contains("False"),
        "MatchQ alternative false should be False, got: {out}"
    );
}

#[ignore = "Pattern alternatives | not supported in parser"]
#[test]
fn test_cases_alternatives() {
    let out = syma_eval("Cases[{1, 2, 3, 4, 5}, 1 | 3 | 5]");
    assert!(
        !out.contains("error"),
        "Cases with alternatives should not error, got: {out}"
    );
}

// ── Sequence patterns ──

#[test]
fn test_match_q_blank_sequence() {
    let out = syma_eval("MatchQ[{1, 2, 3}, {x__}]");
    assert!(
        out.contains("True"),
        "Blank sequence should match, got: {out}"
    );
}

#[test]
fn test_blank_sequence_function() {
    let out = syma_eval("f[x__] := Plus[x]; f[1, 2, 3]");
    assert!(
        out.contains("6"),
        "Blank sequence function should be 6, got: {out}"
    );
}

// ── Pattern guards ──

#[test]
fn test_pattern_guard_true() {
    let out = syma_eval("f[x_ /; x > 0] := x; f[5]");
    assert!(out.contains("5"), "Pattern guard should match, got: {out}");
}

#[ignore = "Pattern guard condition not evaluated"]
#[test]
fn test_pattern_guard_false() {
    let out = syma_eval("f[x_ /; x > 0] := x; f[-3]");
    // Should stay unevaluated since guard fails
    assert!(
        !out.contains("error"),
        "Pattern guard fail should not error, got: {out}"
    );
}

// ── Optional arguments ──

#[test]
fn test_optional_default() {
    let out = syma_eval("g[x_:5] := x; g[]");
    assert!(
        out.contains("5"),
        "Optional default should be 5, got: {out}"
    );
}

#[test]
fn test_optional_explicit() {
    let out = syma_eval("g[x_:5] := x; g[7]");
    assert!(
        out.contains("7"),
        "Optional explicit arg should be 7, got: {out}"
    );
}

// ── Constrained head ──

#[test]
fn test_constrained_head_matches() {
    let out = syma_eval("h[x_Integer] := x^2; h[5]");
    assert!(
        out.contains("25"),
        "Constrained head should match, got: {out}"
    );
}

#[ignore = "Constrained head pattern returns error instead of unevaluated"]
#[test]
fn test_constrained_head_no_match() {
    let out = syma_eval("h[x_Integer] := x^2; h[\"a\"]");
    // Should stay unevaluated
    assert!(
        !out.contains("error"),
        "Constrained head no match should not error, got: {out}"
    );
}

// ── Complex ReplaceAll ──

#[test]
fn test_replace_all_multiple_rules() {
    let out = syma_eval("x /. {x -> y, y -> z}");
    assert!(
        out.contains("y"),
        "First matching rule should apply, got: {out}"
    );
}

#[test]
fn test_replace_repeated_convergence() {
    let out = syma_eval("x //. {x -> y, y -> z}");
    assert!(
        out.contains("z"),
        "ReplaceRepeated should converge to z, got: {out}"
    );
}

#[test]
fn test_replace_all_chained() {
    let out = syma_eval("x /. x -> y /. y -> z");
    assert!(
        out.contains("z"),
        "Chained ReplaceAll should converge to z, got: {out}"
    );
}

// ── Pattern with type constraint ──

#[test]
fn test_match_q_type_constrained() {
    let out = syma_eval("MatchQ[42, _Integer]");
    assert!(out.contains("True"), "42 should match _Integer, got: {out}");
}

// ── Head / TypeOf ──

#[test]
fn test_head_integer() {
    let out = syma_eval("Head[42]");
    assert!(
        out.contains("Integer"),
        "Head[42] should be Integer, got: {out}"
    );
}

#[test]
fn test_head_string() {
    let out = syma_eval(r#"Head["hello"]"#);
    assert!(
        out.contains("String"),
        "Head[hello] should be String, got: {out}"
    );
}

#[test]
fn test_head_list() {
    let out = syma_eval("Head[{1, 2, 3}]");
    assert!(
        out.contains("List"),
        "Head[{{1,2,3}}] should be List, got: {out}"
    );
}

#[test]
fn test_type_of_integer() {
    let out = syma_eval("TypeOf[42]");
    assert!(
        out.contains("Integer"),
        "TypeOf[42] should be Integer, got: {out}"
    );
}
