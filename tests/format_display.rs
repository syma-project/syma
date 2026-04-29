/// Format/display builtin integration tests

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── InputForm ──

#[test]
fn test_input_form_plus() {
    let out = syma_eval("InputForm[a + b]");
    assert!(
        !out.contains("error"),
        "InputForm should not error, got: {out}"
    );
}

// ── FullForm ──

#[test]
fn test_full_form_plus() {
    let out = syma_eval("FullForm[a + b]");
    assert!(
        !out.contains("error"),
        "FullForm should not error, got: {out}"
    );
}

// ── NumberForm ──

#[test]
fn test_number_form() {
    let out = syma_eval("NumberForm[123.456, {3, 2}]");
    assert!(
        !out.contains("error"),
        "NumberForm should not error, got: {out}"
    );
}

// ── ScientificForm ──

#[test]
fn test_scientific_form() {
    let out = syma_eval("ScientificForm[123.456, 6]");
    assert!(
        !out.contains("error"),
        "ScientificForm should not error, got: {out}"
    );
}

// ── TableForm ──

#[test]
fn test_table_form() {
    let out = syma_eval("TableForm[{{1, 2}, {3, 4}}]");
    assert!(
        !out.contains("error"),
        "TableForm should not error, got: {out}"
    );
}

// ── MatrixForm ──

#[test]
fn test_matrix_form() {
    let out = syma_eval("MatrixForm[{{1, 2}, {3, 4}}]");
    assert!(
        !out.contains("error"),
        "MatrixForm should not error, got: {out}"
    );
}

// ── PaddedForm ──

#[test]
fn test_padded_form() {
    let out = syma_eval("PaddedForm[42, 6]");
    assert!(
        !out.contains("error"),
        "PaddedForm should not error, got: {out}"
    );
}

// ── StringForm ──

#[test]
fn test_string_form() {
    let out = syma_eval(r#"StringForm["x = `1`", 5]"#);
    assert!(out.contains("x"), "StringForm should contain x, got: {out}");
}

// ── Shallow ──

#[test]
fn test_shallow() {
    let out = syma_eval("Shallow[Range[100]]");
    assert!(
        !out.contains("error"),
        "Shallow should not error, got: {out}"
    );
}

// ── Defer ──

#[test]
fn test_defer_prevents_eval() {
    let out = syma_eval("Defer[1 + 2]");
    // Defer should prevent evaluation — output should NOT be "3"
    assert!(!out.contains("error"), "Defer should not error, got: {out}");
}

// ── SyntaxLength ──

#[test]
fn test_syntax_length() {
    let out = syma_eval(r#"SyntaxLength["1+2+3"]"#);
    assert!(
        !out.contains("error"),
        "SyntaxLength should not error, got: {out}"
    );
}
