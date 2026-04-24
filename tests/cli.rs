use std::process::Command;

/// Run `cargo run -- <args...>` and return the output.
fn syma_run(args: &[&str]) -> std::process::Output {
    Command::new("cargo")
        .args(["run", "--"])
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
fn test_run_math_example() {
    let output = syma_run(&["examples/math/01-trig-and-log.syma"]);
    assert!(
        output.status.success(),
        "math example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
