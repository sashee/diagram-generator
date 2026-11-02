use std::env;
use std::ffi::OsString;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{self, Command};

fn fail(msg: impl AsRef<str>) -> ! {
    eprintln!("{}", msg.as_ref());
    process::exit(1);
}

const USAGE: &str = "usage: sandbox-run [--keep-env NAME]... -- <command> [args...]";

#[derive(Debug, PartialEq)]
struct Invocation {
    keep_env: Vec<String>,
    command: String,
    args: Vec<String>,
}

fn parse_args(args: &[String]) -> Result<Invocation, String> {
    match args {
        [flag, name, rest @ ..] if flag == "--keep-env" => {
            let invocation = parse_args(rest)?;
            Ok(Invocation {
                keep_env: [vec![name.clone()], invocation.keep_env].concat(),
                ..invocation
            })
        }
        [separator, command, rest @ ..] if separator == "--" => Ok(Invocation {
            keep_env: Vec::new(),
            command: command.clone(),
            args: rest.to_vec(),
        }),
        [separator] if separator == "--" => Err(format!("missing command: {USAGE}")),
        _ => Err(USAGE.to_string()),
    }
}

// The sandboxed command only gets PATH (to find commands) and the variables kept explicitly, so secrets in the
// caller's environment can't end up in its output. TMPDIR, TMP and TEMP are set to the private temp directory.
fn kept_env(keep_env: &[String]) -> Vec<(String, OsString)> {
    std::iter::once("PATH")
        .chain(keep_env.iter().map(String::as_str))
        .filter_map(|name| env::var_os(name).map(|value| (name.to_string(), value)))
        .collect()
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
    let invocation = match parse_args(&env::args().skip(1).collect::<Vec<_>>()) {
        Ok(invocation) => invocation,
        Err(err) => fail(err),
    };

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
        .env_clear()
        .envs(kept_env(&invocation.keep_env))
        .env("TMPDIR", &private_tmp_path)
        .env("TMP", &private_tmp_path)
        .env("TEMP", &private_tmp_path)
        .arg("--")
        .arg(&invocation.command)
        .args(&invocation.args)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn parses_kept_variables_and_command() {
        let invocation = parse_args(&strings(&[
            "--keep-env",
            "A",
            "--keep-env",
            "B",
            "--",
            "cmd",
            "--keep-env",
            "x",
        ]));
        assert_eq!(
            invocation,
            Ok(Invocation {
                keep_env: strings(&["A", "B"]),
                command: "cmd".to_string(),
                args: strings(&["--keep-env", "x"]),
            })
        );
    }

    #[test]
    fn rejects_invalid_arguments() {
        assert_eq!(parse_args(&strings(&["cmd"])), Err(USAGE.to_string()));
        assert_eq!(
            parse_args(&strings(&["--keep-env"])),
            Err(USAGE.to_string())
        );
        assert_eq!(
            parse_args(&strings(&["--"])),
            Err(format!("missing command: {USAGE}"))
        );
    }
}
