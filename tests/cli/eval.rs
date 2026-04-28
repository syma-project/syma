use super::*;

#[test]
fn eval_simple_arithmetic() {
    let out = syma_eval("1 + 2");
    assert!(out.contains("3"), "got: {out}");
}

#[test]
fn eval_assignment() {
    let out = syma_eval("x = 10; x + 20");
    assert!(out.contains("30"), "got: {out}");
}

#[test]
fn eval_string_join() {
    let out = syma_eval(r#"StringJoin["a", "b"]"#);
    assert!(out.contains("ab"), "got: {out}");
}

#[test]
fn eval_list() {
    let out = syma_eval("{1, 2, 3}");
    assert!(out.contains("1,") || out.contains("1}"), "got: {out}");
}

#[test]
fn eval_syntax_q() {
    let out = syma_eval(r#"SyntaxQ["1 + 2"]"#);
    assert!(out.contains("True"), "got: {out}");
}

#[test]
fn eval_short_builtin() {
    let out = syma_eval("Short[Range[20]]");
    assert!(
        out.contains("<<"),
        "Short[Range[20]] should contain <<, got: {out}"
    );
}

#[test]
fn eval_grid_builtin() {
    let out = syma_eval("Grid[{{1, 2, 3}, {10, 20, 30}}]");
    assert!(out.contains("1"), "Grid output should contain values, got: {out}");
    assert!(out.contains("10"), "Grid output should contain values, got: {out}");
}

#[test]
fn eval_base_form() {
    let out = syma_eval("BaseForm[255, 16]");
    assert!(
        out.contains("ff(base 16)") || out.contains("FF(base 16)"),
        "BaseForm[255, 16] should show ff(base 16), got: {out}"
    );
}
