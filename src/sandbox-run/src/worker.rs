use std::env;
use std::error::Error;
use std::io::ErrorKind;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
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

fn is_operation_not_permitted_error(err: &(dyn Error + 'static)) -> bool {
    let mut current = Some(err);
    while let Some(e) = current {
        if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
            if io_err.kind() == ErrorKind::PermissionDenied || io_err.raw_os_error() == Some(1) {
                return true;
            }
        }
        let message = e.to_string();
        if message.contains("Operation not permitted") || message.contains("os error 1") {
            return true;
        }
        current = e.source();
    }
    false
}

fn child_private_tmp_dir() -> Result<PathBuf, String> {
    env::var_os("TMPDIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| "missing absolute TMPDIR for child sandbox process".to_string())
}

fn harden_process_privileges() -> Result<(), String> {
    for capability in caps::all() {
        if caps::has_cap(None, CapSet::Bounding, capability).unwrap_or(false) {
            if let Err(err) = caps::drop(None, CapSet::Bounding, capability) {
                if !is_operation_not_permitted_error(&err) {
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

fn apply_landlock(private_tmp_dir: &PathBuf) -> Result<(), String> {
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
        // io_uring runs socket operations (socket, connect, sendmsg, ...) without the syscalls above,
        // so seccomp can't see them. Landlock only covers TCP below ABI 9 (UNIX sockets) and 10 (UDP)
        "io_uring_setup",
        "io_uring_enter",
        "io_uring_register",
    ];

    for name in denied {
        // fail closed: skipping an unknown name (e.g. with an older libseccomp) would silently drop the rule
        let syscall = ScmpSyscall::from_name(name).map_err(|e| {
            format!("failed to resolve syscall '{name}' for the seccomp filter: {e}")
        })?;

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
    let mut args = env::args().skip(1);

    match args.next() {
        Some(sep) if sep == "--" => {}
        _ => fail("usage: sandbox-run-worker -- <command> [args...]"),
    }

    let private_tmp_dir = match child_private_tmp_dir() {
        Ok(path) => path,
        Err(err) => fail(err),
    };

    if let Err(err) = harden_process_privileges() {
        fail(err);
    }

    if let Err(err) = apply_landlock(&private_tmp_dir) {
        fail(err);
    }

    if let Err(err) = apply_seccomp_network_block() {
        fail(err);
    }

    let command = args
        .next()
        .unwrap_or_else(|| fail("missing command: sandbox-run-worker -- <command> [args...]"));
    let rest = args.collect::<Vec<_>>();

    let err = Command::new(&command).args(rest).exec();
    fail(format!("failed to exec '{}': {err}", command));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::net::{TcpListener, TcpStream, UdpSocket};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::thread;

    // Runs `attempt` on a new thread, sandboxed like the worker's command. Capabilities, no_new_privs, Landlock and
    // seccomp all apply per thread, so the test thread stays unrestricted and can check what reached its listeners.
    fn sandboxed<T: Send + 'static>(attempt: impl FnOnce() -> T + Send + 'static) -> T {
        let private_tmp_dir = tempfile::tempdir().expect("should create private temp dir");
        let private_tmp_path = private_tmp_dir.path().to_path_buf();
        thread::spawn(move || {
            harden_process_privileges().expect("should harden privileges");
            apply_landlock(&private_tmp_path).expect("should apply Landlock");
            apply_seccomp_network_block().expect("should load seccomp filter");
            attempt()
        })
        .join()
        .expect("sandboxed thread should not panic")
    }

    fn arrived<T>(result: std::io::Result<T>) -> bool {
        match result {
            Ok(_) => true,
            Err(err) if err.kind() == ErrorKind::WouldBlock => false,
            Err(err) => panic!("listener failed: {err}"),
        }
    }

    #[test]
    fn tcp_connections_are_blocked() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("should bind TCP listener");
        listener
            .set_nonblocking(true)
            .expect("should make listener nonblocking");
        let addr = listener.local_addr().expect("should get listener address");

        TcpStream::connect(addr).expect("should connect outside the sandbox");
        assert!(
            arrived(listener.accept()),
            "control connection should arrive"
        );

        let result = sandboxed(move || TcpStream::connect(addr).map(|_| ()));
        assert!(result.is_err(), "TCP connect should fail in the sandbox");
        assert!(
            !arrived(listener.accept()),
            "TCP connection from the sandbox reached the listener"
        );
    }

    #[test]
    fn udp_datagrams_are_blocked() {
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("should bind UDP receiver");
        receiver
            .set_nonblocking(true)
            .expect("should make receiver nonblocking");
        let addr = receiver.local_addr().expect("should get receiver address");
        let send =
            move || UdpSocket::bind("127.0.0.1:0").and_then(|socket| socket.send_to(b"leak", addr));
        let mut buf = [0u8; 16];

        send().expect("should send outside the sandbox");
        assert!(
            arrived(receiver.recv_from(&mut buf)),
            "control datagram should arrive"
        );

        let result = sandboxed(send);
        assert!(result.is_err(), "UDP send should fail in the sandbox");
        assert!(
            !arrived(receiver.recv_from(&mut buf)),
            "UDP datagram from the sandbox reached the receiver"
        );
    }

    #[test]
    fn unix_socket_connections_are_blocked() {
        let socket_dir = tempfile::tempdir().expect("should create socket dir");
        let path = socket_dir.path().join("listener.sock");
        let listener = UnixListener::bind(&path).expect("should bind UNIX listener");
        listener
            .set_nonblocking(true)
            .expect("should make listener nonblocking");

        UnixStream::connect(&path).expect("should connect outside the sandbox");
        assert!(
            arrived(listener.accept()),
            "control connection should arrive"
        );

        let result = sandboxed(move || UnixStream::connect(path).map(|_| ()));
        assert!(
            result.is_err(),
            "UNIX socket connect should fail in the sandbox"
        );
        assert!(
            !arrived(listener.accept()),
            "UNIX socket connection from the sandbox reached the listener"
        );
    }

    // io_uring can create sockets, connect and send without the syscalls the seccomp filter denies
    #[test]
    fn io_uring_is_blocked() {
        if let Err(err) = io_uring::IoUring::new(8) {
            eprintln!(
                "skipping: io_uring is unavailable on this host even without the sandbox ({err})"
            );
            return;
        }

        let result = sandboxed(|| {
            io_uring::IoUring::new(8)
                .err()
                .map(|err| err.raw_os_error())
        });
        assert_eq!(
            result,
            Some(Some(1)),
            "io_uring_setup should fail with EPERM in the sandbox"
        );
    }
}
