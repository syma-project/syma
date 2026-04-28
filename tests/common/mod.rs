/// Shared test helpers for integration tests.
///
/// Each test module should import via:
/// ```rust
/// #[path = "../common.rs"]
/// mod common;
/// use common::*;
/// ```
use std::process::Command;

/// Run `cargo run --bin syma -- -e <expr>` and return trimmed stdout on success.
///
/// Panics if the command fails, printing stderr.
pub fn syma_eval(expr: &str) -> String {
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

/// Run `cargo run --bin syma -- <args...>` and return the full output.
///
/// Does NOT assert success; callers must check `output.status.success()` themselves.
pub fn syma_run(args: &[&str]) -> std::process::Output {
    Command::new("cargo")
        .args(["run", "--bin", "syma", "--"])
        .args(args)
        .output()
        .expect("failed to run syma")
}

/// Run `syma -e <expr>` with `SYMA_HOME` set to a fresh temp directory.
///
/// Returns `(trimmed_stdout, temp_dir_path)`.
pub fn syma_eval_with_temp_home(expr: &str) -> (String, String) {
    let tmp = std::env::temp_dir().join(format!("syma_test_{pid}", pid = std::process::id()));
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

/// Assert that `out` contains every string in `needles`.
///
/// Produces a clear per-substring assertion message.
pub fn contains_all(out: &str, needles: &[&str], expr: &str) {
    for needle in needles {
        assert!(
            out.contains(needle),
            "{expr} output should contain \"{needle}\", got: {out}"
        );
    }
}
