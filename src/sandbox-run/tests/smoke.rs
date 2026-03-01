use std::process::Command;

fn sandbox_run_bin() -> &'static str {
    env!("CARGO_BIN_EXE_sandbox-run")
}

fn sandbox_run_worker_bin() -> &'static str {
    env!("CARGO_BIN_EXE_sandbox-run-worker")
}

fn temp_base_entries(path: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(path)
        .expect("should list temp base contents")
        .collect::<Result<Vec<_>, _>>()
        .expect("should read temp base entries")
        .into_iter()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
}

fn assert_temp_base_is_empty(path: &std::path::Path) {
    let entries = temp_base_entries(path);
    assert!(
        entries.is_empty(),
        "expected no leftover sandbox directories, found: {:?}",
        entries
    );
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

#[test]
fn cleans_up_private_temp_directory() {
    let tmp_base = tempfile::tempdir().expect("should create test temp base");

    let out = Command::new(sandbox_run_bin())
        .env("TMPDIR", tmp_base.path())
        .args(["--", "true"])
        .output()
        .expect("sandbox-run should execute");

    assert!(
        out.status.success(),
        "expected success, got status {:?}, stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    assert_temp_base_is_empty(tmp_base.path());
}

#[test]
fn propagates_nonzero_exit_code() {
    let out = Command::new(sandbox_run_bin())
        .args(["--", "false"])
        .output()
        .expect("sandbox-run should execute");

    assert!(!out.status.success(), "expected non-zero exit status");
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected wrapper to propagate child exit code 1"
    );
}

#[test]
fn propagates_signal_termination() {
    let out = Command::new(sandbox_run_bin())
        .args(["--", "sh", "-c", "kill -TERM $$"])
        .output()
        .expect("sandbox-run should execute");

    assert!(!out.status.success(), "expected signal-terminated status");
    assert_eq!(
        out.status.code(),
        Some(143),
        "expected wrapper to exit with 128 + SIGTERM"
    );
}

#[test]
fn requires_absolute_tmpdir_in_child_mode() {
    let out = Command::new(sandbox_run_worker_bin())
        .env("TMPDIR", "relative-tmp")
        .args(["--", "true"])
        .output()
        .expect("sandbox-run-worker should execute");

    assert!(!out.status.success(), "expected worker setup to fail");
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("missing absolute TMPDIR for child sandbox process"),
        "expected TMPDIR validation error, got stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cleanup_on_failing_command() {
    let tmp_base = tempfile::tempdir().expect("should create test temp base");

    let out = Command::new(sandbox_run_bin())
        .env("TMPDIR", tmp_base.path())
        .args(["--", "false"])
        .output()
        .expect("sandbox-run should execute");

    assert!(!out.status.success(), "expected failing command status");
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected wrapper to propagate child exit code 1"
    );

    assert_temp_base_is_empty(tmp_base.path());
}

#[test]
fn cleanup_on_exec_error() {
    let tmp_base = tempfile::tempdir().expect("should create test temp base");

    let out = Command::new(sandbox_run_bin())
        .env("TMPDIR", tmp_base.path())
        .args(["--", "definitely-not-a-real-command-8f0e3f95"])
        .output()
        .expect("sandbox-run should execute");

    assert!(!out.status.success(), "expected exec failure status");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("failed to exec"),
        "expected exec failure stderr, got: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_temp_base_is_empty(tmp_base.path());
}

#[test]
fn child_gets_private_tmpdir_under_parent_tmpdir() {
    let tmp_base = tempfile::tempdir().expect("should create test temp base");

    let out = Command::new(sandbox_run_bin())
        .env("TMPDIR", tmp_base.path())
        .args(["--", "printenv", "TMPDIR"])
        .output()
        .expect("sandbox-run should execute");

    assert!(
        out.status.success(),
        "expected success, got status {:?}, stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    let child_tmpdir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(!child_tmpdir.is_empty(), "expected child TMPDIR in stdout");

    let child_path = std::path::Path::new(&child_tmpdir);
    assert!(
        child_path.is_absolute(),
        "expected absolute child TMPDIR, got: {child_tmpdir}"
    );
    assert!(
        child_path.starts_with(tmp_base.path()),
        "expected child TMPDIR '{child_tmpdir}' under parent TMPDIR '{}'",
        tmp_base.path().display()
    );
    assert_ne!(
        child_path,
        tmp_base.path(),
        "expected child TMPDIR to be a subdirectory, not the parent directory"
    );

    let child_name = child_path
        .file_name()
        .expect("child TMPDIR should have final path component")
        .to_string_lossy();
    assert!(
        child_name.starts_with("sandbox-run-"),
        "expected child TMPDIR directory to use sandbox-run- prefix, got: {child_name}"
    );

    assert_temp_base_is_empty(tmp_base.path());
}
