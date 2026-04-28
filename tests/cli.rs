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

/// Run `syma -e <expr>` with SYMA_HOME set to a temp directory.
/// Returns (stdout, temp_dir_path).
fn syma_eval_with_temp_home(expr: &str) -> (String, String) {
    let tmp = std::env::temp_dir().join(format!("syma_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let output = Command::new("cargo")
        .args(["run", "--bin", "syma", "--", "-e", expr])
        .env("SYMA_HOME", &tmp)
        .output()
        .expect("failed to run syma -e");
    if !output.status.success() {
        panic!(
            "syma -e {expr:?} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (stdout, tmp.to_string_lossy().to_string())
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
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    assert!(
        out.contains("<<"),
        "Short[Range[20]] should contain <<, got: {out}"
    );
}

#[test]
fn test_grid_builtin_via_eval() {
    // Grid of a 2D list should produce tabular output
    let out = syma_eval("Grid[{{1, 2, 3}, {10, 20, 30}}]");
    assert!(
        out.contains("1"),
        "Grid output should contain values, got: {out}"
    );
    assert!(
        out.contains("10"),
        "Grid output should contain values, got: {out}"
    );
}

#[test]
fn test_base_form_via_eval() {
    let out = syma_eval("BaseForm[255, 16]");
    assert!(
        out.contains("ff(base 16)") || out.contains("FF(base 16)"),
        "BaseForm[255, 16] should show ff(base 16), got: {out}"
    );
}

// ── LocalSymbol tests ──────────────────────────────────────────────────────────

#[test]
fn test_local_symbol_write_then_read() {
    let (out, _) =
        syma_eval_with_temp_home(r#"LocalSymbol["test_int"] = 42; LocalSymbol["test_int"]"#);
    assert!(out.contains("42"), "Should read back 42, got: {out}");
}

#[test]
fn test_local_symbol_read_missing_null() {
    let (out, _) = syma_eval_with_temp_home(r#"LocalSymbol["nonexistent"]"#);
    assert!(
        out.contains("Null") || out.is_empty(),
        "Missing key should yield Null, got: {out}"
    );
}

#[test]
fn test_local_symbol_read_missing_default() {
    let (out, _) = syma_eval_with_temp_home(r#"LocalSymbol["nope", "fallback"]"#);
    assert!(
        out.contains("fallback"),
        "Default value should be returned, got: {out}"
    );
}

#[test]
fn test_local_symbol_write_string() {
    let (out, _) = syma_eval_with_temp_home(
        r#"LocalSymbol["greeting"] = "hello world"; LocalSymbol["greeting"]"#,
    );
    assert!(
        out.contains("hello world"),
        "Should read back the string, got: {out}"
    );
}

#[test]
fn test_local_symbol_persists_across_calls() {
    let tmp = std::env::temp_dir().join(format!("syma_test_persist_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);

    // First call: write
    let output1 = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "syma",
            "--",
            "-e",
            r#"LocalSymbol["persist_key"] = 99"#,
        ])
        .env("SYMA_HOME", &tmp)
        .output()
        .expect("failed first call");
    assert!(
        output1.status.success(),
        "first call failed: {}",
        String::from_utf8_lossy(&output1.stderr)
    );

    // Second call: read (separate process, should read from disk)
    let output2 = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "syma",
            "--",
            "-e",
            r#"LocalSymbol["persist_key"]"#,
        ])
        .env("SYMA_HOME", &tmp)
        .output()
        .expect("failed second call");
    assert!(
        output2.status.success(),
        "second call failed: {}",
        String::from_utf8_lossy(&output2.stderr)
    );
    let stdout = String::from_utf8_lossy(&output2.stdout);
    assert!(
        stdout.contains("99"),
        "Should persist 99 across processes, got: {stdout}"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
}

// ── Pattern/Attribute tests ─────────────────────────────────────────────────────────

#[test]
fn test_pattern_test_integer_q() {
    // _?IntegerQ matches only integers
    let out = syma_eval("MatchQ[5, _?IntegerQ]");
    assert!(
        out.contains("True"),
        "5 should match _?IntegerQ, got: {out}"
    );
}

#[test]
fn test_pattern_test_not_integer_q() {
    // _?IntegerQ should not match a real
    let out = syma_eval("MatchQ[3.14, _?IntegerQ]");
    assert!(
        out.contains("False"),
        "3.14 should not match _?IntegerQ, got: {out}"
    );
}

#[test]
fn test_optional_default_value() {
    // f[x_:5] := x; f[] should return 5
    let out = syma_eval("f[x_:5] := x; f[]");
    assert!(out.contains("5"), "f[] should return 5, got: {out}");
}

#[test]
fn test_flat_attribute_matching() {
    // With Flat, f[1, f[2, 3]] should match f[x_, y_, z_]
    let out = syma_eval("SetAttributes[f, Flat]; MatchQ[f[1, f[2, 3]], f[x_, y_, z_]]");
    assert!(
        out.contains("True"),
        "f[1, f[2, 3]] should match f[x_, y_, z_] with Flat, got: {out}"
    );
}

#[test]
fn test_orderless_attribute_matching() {
    // With Orderless, f[2, 1] should match f[x_, y_]
    let out = syma_eval("SetAttributes[f, Orderless]; MatchQ[f[2, 1], f[x_, y_]]");
    assert!(
        out.contains("True"),
        "f[2, 1] should match f[x_, y_] with Orderless, got: {out}"
    );
}

#[test]
fn test_one_identity_attribute_matching() {
    // With OneIdentity, f[42] should match _Integer
    let out = syma_eval("SetAttributes[f, OneIdentity]; MatchQ[f[42], _Integer]");
    assert!(
        out.contains("True"),
        "f[42] should match _Integer with OneIdentity, got: {out}"
    );
}

#[test]
fn test_integrate_basic() {
    let out = syma_eval("Integrate[x^2 + Sin[x], x]");
    assert!(
        out.contains("Cos[x]"),
        "Integrate[x^2 + Sin[x], x] should contain Cos[x], got: {out}"
    );
    assert!(
        out.contains("x^3"),
        "Integrate[x^2 + Sin[x], x] should contain x^3, got: {out}"
    );
}

// ── Integration tests for untested example files ──────────────────────

#[test]
fn test_run_control_flow_example() {
    let output = syma_run(&["examples/basics/04-control-flow.syma"]);
    assert!(
        output.status.success(),
        "control-flow example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("yes"), "stdout: {stdout}");
    assert!(stdout.contains("math works"), "stdout: {stdout}");
    assert!(
        stdout.contains("7"),
        "abs[-7] should yield 7, stdout: {stdout}"
    );
}

#[test]
fn test_run_map_fold_select_example() {
    let output = syma_run(&["examples/functional/01-map-fold-select.syma"]);
    assert!(
        output.status.success(),
        "map-fold-select example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("15"),
        "Sum 1..5 should be 15, stdout: {stdout}"
    );
    assert!(
        stdout.contains("120"),
        "Product 1..5 should be 120, stdout: {stdout}"
    );
}

#[test]
fn test_run_patterns_and_rules_example() {
    let output = syma_run(&["examples/functional/02-patterns-and-rules.syma"]);
    assert!(
        output.status.success(),
        "patterns-and-rules example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("55"),
        "fib[10] should be 55, stdout: {stdout}"
    );
    assert!(
        stdout.contains("120"),
        "fact[5] should be 120, stdout: {stdout}"
    );
}

#[test]
fn test_run_applied_example() {
    let output = syma_run(&["examples/applied/01-real-world.syma"]);
    assert!(
        output.status.success(),
        "applied example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("32"),
        "celsiusToF[0] should be 32, stdout: {stdout}"
    );
    assert!(
        stdout.contains("5"),
        "dist[0,0,3,4] should be 5, stdout: {stdout}"
    );
    assert!(
        stdout.contains("3628800"),
        "fact[10] should be 3628800, stdout: {stdout}"
    );
}

#[test]
fn test_run_pi_series_example() {
    let output = syma_run(&["examples/math/02-pi-series.syma"]);
    assert!(
        output.status.success(),
        "pi-series example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("3.1"),
        "pi series should approximate 3.1, stdout: {stdout}"
    );
}

#[test]
fn test_run_newtons_method_example() {
    let output = syma_run(&["examples/math/03-newtons-method.syma"]);
    assert!(
        output.status.success(),
        "newtons-method example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("1.414"),
        "sqrt(2) approx should be 1.414, stdout: {stdout}"
    );
}

#[test]
fn test_run_numerical_integration_example() {
    let output = syma_run(&["examples/math/04-numerical-integration.syma"]);
    assert!(
        output.status.success(),
        "numerical-integration example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("3.14"),
        "numerical integration of pi should be ~3.14, stdout: {stdout}"
    );
}

#[test]
fn test_run_taylor_series_example() {
    let output = syma_run(&["examples/math/05-taylor-series.syma"]);
    assert!(
        output.status.success(),
        "taylor-series example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("2.718"),
        "e^1 Taylor approx should be 2.718, stdout: {stdout}"
    );
}

#[test]
fn test_run_monte_carlo_pi_example() {
    let output = syma_run(&["examples/math/06-monte-carlo-pi.syma"]);
    assert!(
        output.status.success(),
        "monte-carlo-pi example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Estimate"),
        "monte carlo should produce an estimate, stdout: {stdout}"
    );
    // The estimate should be a number (non-deterministic, just check it ran)
    assert!(
        stdout.contains("3."),
        "monte carlo pi estimate should be ~3.x, stdout: {stdout}"
    );
}

#[test]
fn test_run_module_example() {
    // Module/import are fully implemented
    let output = syma_run(&["examples/advanced/01-modules.syma"]);
    assert!(
        output.status.success(),
        "Module example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("25") && stdout.contains("8"),
        "Module example should compute square[5] = 25 and rectArea[2,4] = 8, stdout: {stdout}"
    );
}

#[test]
fn test_run_oop_example() {
    // OOP with classes, inheritance, mixins — fully implemented
    // The example file is a specification with all usage in comments,
    // so we just verify it parses and evaluates without error.
    let output = syma_run(&["examples/advanced/02-oop.syma"]);
    assert!(
        output.status.success(),
        "OOP example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── Flat attribute result normalization tests ──────────────────────

#[test]
fn test_flat_plus_result_normalization() {
    // Plus[Plus[a,b], c] should be flattened to Plus[a, b, c]
    let out = syma_eval("Plus[Plus[a, b], c]");
    assert!(
        !out.contains("Plus[Plus"),
        "Plus result should be flattened, got: {out}"
    );
}

#[test]
fn test_flat_times_result_normalization() {
    // Times[Times[a,b], c] should be flattened
    let out = syma_eval("Times[Times[a, b], c]");
    assert!(
        !out.contains("Times[Times"),
        "Times result should be flattened, got: {out}"
    );
}

#[test]
fn test_flat_user_defined_result() {
    // User-defined Flat function should flatten results
    let out = syma_eval("SetAttributes[f, Flat]; f[a, f[b, c]]");
    assert!(
        !out.contains("f[f"),
        "User Flat function result should be flattened, got: {out}"
    );
}

#[test]
fn test_flat_and_result_normalization() {
    // And[And[a, b], c] should be flattened
    let out = syma_eval("And[And[a, b], c]");
    assert!(
        !out.contains("And[And"),
        "And result should be flattened, got: {out}"
    );
}

#[test]
fn test_flat_or_result_normalization() {
    // Or[Or[a, b], c] should be flattened
    let out = syma_eval("Or[Or[a, b], c]");
    assert!(
        !out.contains("Or[Or"),
        "Or result should be flattened, got: {out}"
    );
}

#[test]
fn test_flat_deeply_nested() {
    // Deeply nested Plus should be fully flattened
    let out = syma_eval("Plus[Plus[Plus[a, b], c], d]");
    assert!(
        !out.contains("Plus[Plus"),
        "Deeply nested Plus should be fully flattened, got: {out}"
    );
}

