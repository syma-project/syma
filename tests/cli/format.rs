use super::*;

#[test]
fn format_flag_inputform() {
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
fn format_flag_fullform() {
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
