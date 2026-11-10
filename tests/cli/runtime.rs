//! End-to-end tests for runtime/infra surfaces: `--check`, `--dap`, `--kernel`,
//! and `--format` argument handling.
//!
//! These drive the compiled `syma` binary directly (via `CARGO_BIN_EXE_syma`)
//! rather than through `cargo run`, so they are fast and can pipe stdin.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn syma_bin() -> &'static str {
    env!("CARGO_BIN_EXE_syma")
}

/// Create a fresh temp directory unique to the test tag and process.
fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("syma_runtime_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_file_in(dir: &std::path::Path, contents: &str, name: &str) -> std::path::PathBuf {
    let file = dir.join(name);
    std::fs::write(&file, contents).unwrap();
    file
}

/// Spawn `syma --kernel` with `input` on stdin, close stdin, and collect output.
fn run_kernel(input: &str) -> Output {
    let mut child = Command::new(syma_bin())
        .arg("--kernel")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn syma --kernel");
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(input.as_bytes())
        .expect("write kernel request");
    child.wait_with_output().expect("wait for syma --kernel")
}

// ── --check ───────────────────────────────────────────────────────────────────

#[test]
fn check_flag_accepts_valid_file() {
    let dir = temp_dir("check_ok");
    let file = run_file_in(&dir, "x = 1\n", "ok.syma");
    let out = Command::new(syma_bin())
        .args(["--check", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_flag_rejects_invalid_file() {
    let dir = temp_dir("check_bad");
    let file = run_file_in(&dir, "1 +\n", "bad.syma");
    let out = Command::new(syma_bin())
        .args(["--check", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_flag_accepts_file_named_like_subcommand() {
    // Regression: a file literally named `run` used to be filtered out as if it
    // were the `run` subcommand, producing a bogus "Usage" error.
    let dir = temp_dir("check_named_run");
    let file = run_file_in(&dir, "x = 1\n", "run");
    let out = Command::new(syma_bin())
        .args(["--check", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "file named `run` should be accepted; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_flag_requires_file_operand() {
    let out = Command::new(syma_bin()).arg("--check").output().unwrap();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("Usage"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ── --dap ─────────────────────────────────────────────────────────────────────

#[test]
fn dap_emits_initialized_event() {
    let dir = temp_dir("dap");
    let file = run_file_in(&dir, "x = 1\n", "prog.syma");
    let out = Command::new(syma_bin())
        .args(["--dap", file.to_str().unwrap()])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(r#""event":"initialized""#),
        "expected initialized event, stdout: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── --kernel ──────────────────────────────────────────────────────────────────

#[test]
fn kernel_mode_roundtrips_json() {
    let out = run_kernel("{\"input\": \"2+2\"}\n");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().next().unwrap_or("");
    let parsed: serde_json::Value = serde_json::from_str(line).expect("valid JSON response");
    assert_eq!(parsed["success"], true, "stdout: {stdout}");
    assert_eq!(parsed["results"][0]["output"], "4");
}

#[test]
fn kernel_mode_frames_multiple_requests_one_line_each() {
    let out = run_kernel("{\"input\": \"1+1\"}\n{\"input\": \"3+4\"}\n");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "expected one response per request: {stdout}"
    );
    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(first["results"][0]["output"], "2");
    assert_eq!(second["results"][0]["output"], "7");
}

#[test]
fn kernel_mode_reports_malformed_json() {
    let out = run_kernel("this is not json\n");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.lines().next().unwrap_or("")).unwrap();
    assert_eq!(parsed["success"], false);
    assert!(
        parsed["error"]
            .as_str()
            .unwrap_or("")
            .contains("Invalid JSON"),
        "response: {stdout}"
    );
}

#[test]
fn kernel_mode_skips_blank_lines() {
    let out = run_kernel("\n\n{\"input\": \"5+5\"}\n\n");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "stdout: {stdout}");
    let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(parsed["results"][0]["output"], "10");
}

// ── --format ──────────────────────────────────────────────────────────────────

#[test]
fn format_unknown_value_fails() {
    let out = Command::new(syma_bin())
        .args(["-e", "1+1", "--format", "bogus"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("unknown --format"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn format_missing_value_fails() {
    let out = Command::new(syma_bin())
        .args(["-e", "1+1", "--format"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--format requires a value"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
