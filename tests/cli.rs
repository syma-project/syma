use std::process::Command;

/// Run `cargo run --bin syma -- -e <expr>` and return stdout on success.
fn syma_eval(expr: &str) -> String {
    let output = Command::new("cargo")
        .args(["run", "--bin", "syma", "--", "-e", expr])
        .output()
        .expect("failed to run syma -e");
    if !output.status.success() {
        panic!(
            "syma -e {expr:?} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Run `cargo run --bin syma -- <args...>` and return the output.
fn syma_run(args: &[&str]) -> std::process::Output {
    Command::new("cargo")
        .args(["run", "--bin", "syma", "--"])
        .args(args)
        .output()
        .expect("failed to run syma")
}

#[test]
fn test_run_basics_example() {
    let output = syma_run(&["examples/basics/01-basics.syma"]);
    assert!(
        output.status.success(),
        "basics example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("42"), "stdout: {stdout}");
    assert!(stdout.contains("hello world"), "stdout: {stdout}");
    assert!(
        stdout.contains("6"),
        "1+2+3 should yield 6, stdout: {stdout}"
    );
}

#[test]
fn test_run_functions_example() {
    let output = syma_run(&["examples/basics/02-functions.syma"]);
    assert!(
        output.status.success(),
        "functions example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_run_lists_example() {
    let output = syma_run(&["examples/basics/03-lists.syma"]);
    assert!(
        output.status.success(),
        "lists example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("1, 2, 3") || stdout.contains("{1, 2, 3}"),
        "stdout: {stdout}"
    );
}

#[test]
fn test_eval_simple() {
    let out = syma_eval("1 + 2");
    assert!(out.contains("3"), "got: {out}");
}

#[test]
fn test_eval_assignment() {
    let out = syma_eval("x = 10; x + 20");
    assert!(out.contains("30"), "got: {out}");
}

#[test]
fn test_eval_string() {
    let out = syma_eval(r#"StringJoin["a", "b"]"#);
    assert!(out.contains("ab"), "got: {out}");
}

#[test]
fn test_eval_list() {
    let out = syma_eval("{1, 2, 3}");
    assert!(out.contains("1,") || out.contains("1}"), "got: {out}");
}

#[test]
fn test_run_math_example() {
    let output = syma_run(&["examples/math/01-trig-and-log.syma"]);
    assert!(
        output.status.success(),
        "math example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── Format / display tests ──────────────────────────────────────────────────────

#[test]
fn test_cli_format_flag_inputform() {
    // --format inputform should show infix notation for Plus[a, b]
    let output = syma_run(&["-e", "Plus[a, b]", "--format", "inputform"]);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("a + b"),
        "expected 'a + b' in output, got: {stdout}"
    );
}

#[test]
fn test_cli_format_flag_fullform() {
    // --format fullform should show head notation for a + b with symbolic args
    let output = syma_run(&["-e", "Plus[a, b]", "--format", "fullform"]);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Plus"),
        "expected FullForm output containing 'Plus', got: {stdout}"
    );
}

#[test]
fn test_eval_syntax_q() {
    let out = syma_eval(r#"SyntaxQ["1 + 2"]"#);
    assert!(out.contains("True"), "got: {out}");
}

#[test]
fn test_short_builtin_via_eval() {
    // Short of a long list should show <<...>>
    let out = syma_eval("Short[Range[20]]");
    assert!(out.contains("<<"), "Short[Range[20]] should contain <<, got: {out}");
}

#[test]
fn test_grid_builtin_via_eval() {
    // Grid of a 2D list should produce tabular output
    let out = syma_eval("Grid[{{1, 2, 3}, {10, 20, 30}}]");
    assert!(out.contains("1"), "Grid output should contain values, got: {out}");
    assert!(out.contains("10"), "Grid output should contain values, got: {out}");
}

#[test]
fn test_base_form_via_eval() {
    let out = syma_eval("BaseForm[255, 16]");
    assert!(
        out.contains("ff(base 16)") || out.contains("FF(base 16)"),
        "BaseForm[255, 16] should show ff(base 16), got: {out}"
    );
}
