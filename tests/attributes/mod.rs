/// Shared helper for attribute integration tests.
/// Each submodule tests one or more related attributes.
use std::process::Command;

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

mod flat;
mod hold;
mod listable;
mod one_identity;
mod orderless;
mod protected_locked;
