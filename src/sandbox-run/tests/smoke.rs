use std::process::Command;

fn sandbox_run_bin() -> &'static str {
    env!("CARGO_BIN_EXE_sandbox-run")
}

#[test]
fn runs_simple_command_successfully() {
    let out = Command::new(sandbox_run_bin())
        .args(["--", "true"])
        .output()
        .expect("sandbox-run should execute");

    assert!(
        out.status.success(),
        "expected success, got status {:?}, stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn rejects_missing_separator() {
    let out = Command::new(sandbox_run_bin())
        .arg("true")
        .output()
        .expect("sandbox-run should execute");

    assert!(!out.status.success(), "missing '--' should fail");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("usage: sandbox-run --"),
        "expected usage message, got stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
