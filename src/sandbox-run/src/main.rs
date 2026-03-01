use std::env;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{self, Command};

fn fail(msg: impl AsRef<str>) -> ! {
    eprintln!("{}", msg.as_ref());
    process::exit(1);
}

fn parse_command() -> (String, Vec<String>) {
    let mut args = env::args().skip(1);

    match args.next() {
        Some(sep) if sep == "--" => {}
        _ => fail("usage: sandbox-run -- <command> [args... ]"),
    }

    let command = args
        .next()
        .unwrap_or_else(|| fail("missing command: sandbox-run -- <command> [args...]"));
    let rest = args.collect::<Vec<_>>();
    (command, rest)
}

fn tmp_dir() -> PathBuf {
    env::var_os("TMPDIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn create_private_tmp_dir() -> Result<tempfile::TempDir, String> {
    let base = tmp_dir();
    tempfile::Builder::new()
        .prefix("sandbox-run-")
        .tempdir_in(&base)
        .map_err(|e| {
            format!(
                "failed to create private temp directory under '{}': {e}",
                base.display()
            )
        })
}

fn worker_bin_path() -> Result<PathBuf, String> {
    let exe_path =
        env::current_exe().map_err(|e| format!("failed to get current executable path: {e}"))?;
    let exe_dir = exe_path.parent().ok_or_else(|| {
        format!(
            "failed to get parent directory of executable: {}",
            exe_path.display()
        )
    })?;
    let worker_path = exe_dir.join("sandbox-run-worker");
    if !worker_path.exists() {
        return Err(format!(
            "worker binary not found at expected path: {}",
            worker_path.display()
        ));
    }
    Ok(worker_path)
}

fn main() {
    let (command, args) = parse_command();

    let private_tmp_dir = match create_private_tmp_dir() {
        Ok(dir) => dir,
        Err(err) => fail(err),
    };

    let private_tmp_path = private_tmp_dir.path().to_path_buf();

    let worker_path = match worker_bin_path() {
        Ok(path) => path,
        Err(err) => fail(err),
    };

    let mut child = match Command::new(&worker_path)
        .env("TMPDIR", &private_tmp_path)
        .env("TMP", &private_tmp_path)
        .env("TEMP", &private_tmp_path)
        .arg("--")
        .arg(&command)
        .args(&args)
        .spawn()
    {
        Ok(child) => child,
        Err(err) => fail(format!("failed to spawn sandbox worker: {err}")),
    };

    let status = match child.wait() {
        Ok(status) => status,
        Err(err) => fail(format!("failed to wait for sandbox worker: {err}")),
    };

    let exit_code = match status.code() {
        Some(code) => code,
        None => status.signal().map_or(1, |signal| 128 + signal),
    };

    drop(private_tmp_dir);
    process::exit(exit_code);
}
