use super::*;

#[test]
fn run_basics_example() {
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
fn run_functions_example() {
    let output = syma_run(&["examples/basics/02-functions.syma"]);
    assert!(
        output.status.success(),
        "functions example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn run_lists_example() {
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
fn run_control_flow_example() {
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
fn run_map_fold_select_example() {
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
fn run_patterns_and_rules_example() {
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
fn run_applied_example() {
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
fn run_math_trig_example() {
    let output = syma_run(&["examples/math/01-trig-and-log.syma"]);
    assert!(
        output.status.success(),
        "math example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn run_pi_series_example() {
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
fn run_newtons_method_example() {
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
fn run_numerical_integration_example() {
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
fn run_taylor_series_example() {
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
fn run_monte_carlo_pi_example() {
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
    assert!(
        stdout.contains("3."),
        "monte carlo pi estimate should be ~3.x, stdout: {stdout}"
    );
}

#[test]
fn run_module_example() {
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
fn run_oop_example() {
    let output = syma_run(&["examples/advanced/02-oop.syma"]);
    assert!(
        output.status.success(),
        "OOP example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
