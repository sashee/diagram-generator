use std::env;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use caps::CapSet;
use landlock::{
    Access, AccessFs, AccessNet, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset,
    RulesetAttr, RulesetCreatedAttr, Scope, ABI,
};
use libseccomp::{ScmpAction, ScmpFilterContext, ScmpSyscall};

const NIX_STORE_DIR: &str = env!("NIX_STORE_DIR");

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

fn create_private_tmp_dir() -> Result<PathBuf, String> {
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
        .map(tempfile::TempDir::keep)
}

fn harden_process_privileges() -> Result<(), String> {
    for capability in caps::all() {
        if caps::has_cap(None, CapSet::Bounding, capability).unwrap_or(false) {
            if let Err(err) = caps::drop(None, CapSet::Bounding, capability) {
                if !err.to_string().contains("Operation not permitted") {
                    return Err(format!(
                        "failed to drop capability '{capability:?}' from bounding set: {err}"
                    ));
                }
            }
        }
    }

    caps::clear(None, CapSet::Ambient)
        .map_err(|err| format!("failed to clear ambient capabilities: {err}"))?;
    caps::clear(None, CapSet::Effective)
        .map_err(|err| format!("failed to clear effective capabilities: {err}"))?;
    caps::clear(None, CapSet::Permitted)
        .map_err(|err| format!("failed to clear permitted capabilities: {err}"))?;
    caps::clear(None, CapSet::Inheritable)
        .map_err(|err| format!("failed to clear inheritable capabilities: {err}"))?;

    rustix::thread::set_no_new_privs(true)
        .map_err(|err| format!("failed to set no_new_privs: {err}"))?;

    Ok(())
}

fn apply_landlock(private_tmp_dir: &Path) -> Result<(), String> {
    let abi = ABI::V6;

    let store_fd = PathFd::new(NIX_STORE_DIR)
        .map_err(|e| format!("failed to open Nix store path '{}': {e}", NIX_STORE_DIR))?;
    let tmp_fd = PathFd::new(private_tmp_dir).map_err(|e| {
        format!(
            "failed to open private temp directory '{}': {e}",
            private_tmp_dir.display()
        )
    })?;
    let dev_random_fd =
        PathFd::new("/dev/random").map_err(|e| format!("failed to open /dev/random: {e}"))?;
    let dev_urandom_fd =
        PathFd::new("/dev/urandom").map_err(|e| format!("failed to open /dev/urandom: {e}"))?;
    let dev_null_fd =
        PathFd::new("/dev/null").map_err(|e| format!("failed to open /dev/null: {e}"))?;
    let proc_fd = PathFd::new("/proc").map_err(|e| format!("failed to open /proc: {e}"))?;

    let fs_all = AccessFs::from_all(abi);
    let fs_ro = AccessFs::from_read(abi) | AccessFs::Execute;
    let fs_read_only = AccessFs::from_read(abi);
    let fs_rw = AccessFs::from_read(abi) | AccessFs::from_write(abi) | AccessFs::Execute;
    let fs_read_file = AccessFs::ReadFile;
    let fs_dev_file = AccessFs::from_file(abi);

    Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(fs_all)
        .map_err(|e| format!("failed to configure filesystem restrictions: {e}"))?
        .handle_access(AccessNet::from_all(abi))
        .map_err(|e| format!("failed to configure network restrictions: {e}"))?
        .scope(Scope::from_all(abi))
        .map_err(|e| format!("failed to configure scope restrictions: {e}"))?
        .create()
        .map_err(|e| format!("failed to create Landlock ruleset: {e}"))?
        .add_rule(PathBeneath::new(store_fd, fs_ro))
        .map_err(|e| format!("failed to add Nix store rule: {e}"))?
        .add_rule(PathBeneath::new(tmp_fd, fs_rw))
        .map_err(|e| format!("failed to add temp directory rule: {e}"))?
        .add_rule(PathBeneath::new(dev_random_fd, fs_read_file))
        .map_err(|e| format!("failed to add /dev/random rule: {e}"))?
        .add_rule(PathBeneath::new(dev_urandom_fd, fs_read_file))
        .map_err(|e| format!("failed to add /dev/urandom rule: {e}"))?
        .add_rule(PathBeneath::new(dev_null_fd, fs_dev_file))
        .map_err(|e| format!("failed to add /dev/null rule: {e}"))?
        .add_rule(PathBeneath::new(proc_fd, fs_read_only))
        .map_err(|e| format!("failed to add /proc rule: {e}"))?
        .restrict_self()
        .map_err(|e| format!("failed to enforce Landlock ruleset: {e}"))?;

    Ok(())
}

fn apply_seccomp_network_block() -> Result<(), String> {
    let mut filter = ScmpFilterContext::new(ScmpAction::Allow)
        .map_err(|e| format!("failed to initialize seccomp filter: {e}"))?;

    let denied = [
        "socket",
        "socketpair",
        "connect",
        "bind",
        "listen",
        "accept",
        "accept4",
        "sendto",
        "sendmsg",
        "sendmmsg",
        "recvfrom",
        "recvmsg",
        "recvmmsg",
        "shutdown",
        "getsockname",
        "getpeername",
        "setsockopt",
        "getsockopt",
    ];

    for name in denied {
        let syscall = match ScmpSyscall::from_name(name) {
            Ok(syscall) => syscall,
            Err(_) => continue,
        };

        filter
            .add_rule(ScmpAction::Errno(1), syscall)
            .map_err(|e| format!("failed to add seccomp rule for '{name}': {e}"))?;
    }

    filter
        .load()
        .map_err(|e| format!("failed to load seccomp filter: {e}"))?;

    Ok(())
}

fn main() {
    let (command, args) = parse_command();

    let private_tmp_dir = match create_private_tmp_dir() {
        Ok(path) => path,
        Err(err) => fail(err),
    };
    env::set_var("TMPDIR", &private_tmp_dir);
    env::set_var("TMP", &private_tmp_dir);
    env::set_var("TEMP", &private_tmp_dir);

    if let Err(err) = harden_process_privileges() {
        fail(err);
    }

    if let Err(err) = apply_landlock(&private_tmp_dir) {
        fail(err);
    }

    if let Err(err) = apply_seccomp_network_block() {
        fail(err);
    }

    let err = Command::new(&command).args(args).exec();
    fail(format!("failed to exec '{}': {err}", command));
}
