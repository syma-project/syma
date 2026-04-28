use std::process::Command;

use super::*;

#[test]
fn localsymbol_write_then_read() {
    let (out, _) = syma_eval_with_temp_home(r#"LocalSymbol["test_int"] = 42; LocalSymbol["test_int"]"#);
    assert!(out.contains("42"), "Should read back 42, got: {out}");
}

#[test]
fn localsymbol_read_missing_null() {
    let (out, _) = syma_eval_with_temp_home(r#"LocalSymbol["nonexistent"]"#);
    assert!(
        out.contains("Null") || out.is_empty(),
        "Missing key should yield Null, got: {out}"
    );
}

#[test]
fn localsymbol_read_missing_default() {
    let (out, _) = syma_eval_with_temp_home(r#"LocalSymbol["nope", "fallback"]"#);
    assert!(
        out.contains("fallback"),
        "Default value should be returned, got: {out}"
    );
}

#[test]
fn localsymbol_write_string() {
    let (out, _) = syma_eval_with_temp_home(
        r#"LocalSymbol["greeting"] = "hello world"; LocalSymbol["greeting"]"#,
    );
    assert!(
        out.contains("hello world"),
        "Should read back the string, got: {out}"
    );
}

#[test]
fn localsymbol_persists_across_calls() {
    let tmp = std::env::temp_dir().join(format!("syma_test_persist_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);

    let output1 = Command::new("cargo")
        .args(["run", "--bin", "syma", "--", "-e", r#"LocalSymbol["persist_key"] = 99"#])
        .env("SYMA_HOME", &tmp)
        .output()
        .expect("failed first call");
    assert!(
        output1.status.success(),
        "first call failed: {}",
        String::from_utf8_lossy(&output1.stderr)
    );

    let output2 = Command::new("cargo")
        .args(["run", "--bin", "syma", "--", "-e", r#"LocalSymbol["persist_key"]"#])
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

    let _ = std::fs::remove_dir_all(&tmp);
}
