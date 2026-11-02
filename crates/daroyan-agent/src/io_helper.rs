use std::fs;
use std::io::{self, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use nix::libc;
use nix::sys::prctl;
use nix::sys::socket::{getsockopt, sockopt};
use nix::unistd::User;
use seccompiler::{BpfProgram, SeccompAction, SeccompFilter};

use crate::collect::process_io::SOCKET_PATH;

const AGENT_USER: &str = "daroyan";
const SNAPSHOT_TTL: Duration = Duration::from_secs(1);
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

const DENIED_SYSCALLS: [i64; 26] = [
    libc::SYS_ptrace,
    libc::SYS_process_vm_readv,
    libc::SYS_process_vm_writev,
    libc::SYS_pidfd_getfd,
    libc::SYS_open_by_handle_at,
    libc::SYS_name_to_handle_at,
    libc::SYS_execve,
    libc::SYS_execveat,
    libc::SYS_socket,
    libc::SYS_connect,
    libc::SYS_mount,
    libc::SYS_umount2,
    libc::SYS_pivot_root,
    libc::SYS_chroot,
    libc::SYS_unshare,
    libc::SYS_setns,
    libc::SYS_bpf,
    libc::SYS_perf_event_open,
    libc::SYS_userfaultfd,
    libc::SYS_io_uring_setup,
    libc::SYS_keyctl,
    libc::SYS_add_key,
    libc::SYS_kexec_load,
    libc::SYS_init_module,
    libc::SYS_finit_module,
    libc::SYS_delete_module,
];

pub fn serve() -> Result<()> {
    let listener = open_listener()?;
    let agent = User::from_name(AGENT_USER)?
        .with_context(|| format!("the {AGENT_USER} user does not exist"))?;
    lock_down()?;

    let mut snapshot = String::new();
    let mut taken_at: Option<Instant> = None;
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if !is_allowed(&stream, agent.uid.as_raw()) {
            continue;
        }

        if taken_at.is_none_or(|at| at.elapsed() >= SNAPSHOT_TTL) {
            snapshot = read_snapshot();
            taken_at = Some(Instant::now());
        }

        stream.set_write_timeout(Some(WRITE_TIMEOUT)).ok();
        stream.write_all(snapshot.as_bytes()).ok();
    }
    Ok(())
}

fn open_listener() -> Result<UnixListener> {
    if let Some(listener) = take_activated_socket() {
        return Ok(listener);
    }

    let path = Path::new(SOCKET_PATH);
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_socket()) {
        fs::remove_file(path).with_context(|| format!("remove {SOCKET_PATH}"))?;
    }
    let listener =
        UnixListener::bind(path).with_context(|| format!("bind {SOCKET_PATH}"))?;
    // The folder's group decides who can connect, so the socket itself is open.
    fs::set_permissions(path, fs::Permissions::from_mode(0o666))
        .with_context(|| format!("chmod {SOCKET_PATH}"))?;
    Ok(listener)
}

fn take_activated_socket() -> Option<UnixListener> {
    let fd = io::stdin().as_fd().try_clone_to_owned().ok()?;
    let listener = UnixListener::from(fd);
    listener.local_addr().ok()?;
    Some(listener)
}

fn is_allowed(stream: &UnixStream, agent_uid: u32) -> bool {
    getsockopt(stream, sockopt::PeerCredentials)
        .is_ok_and(|credentials| credentials.uid() == 0 || credentials.uid() == agent_uid)
}

fn lock_down() -> Result<()> {
    prctl::set_dumpable(false).context("mark the helper not dumpable")?;

    let rules = DENIED_SYSCALLS.iter().map(|&syscall| (syscall, Vec::new())).collect();
    let filter = SeccompFilter::new(
        rules,
        SeccompAction::Allow,
        SeccompAction::Errno(libc::EPERM as u32),
        std::env::consts::ARCH.try_into()?,
    )?;
    let program: BpfProgram = filter.try_into()?;
    seccompiler::apply_filter(&program).context("apply the seccomp filter")?;
    Ok(())
}

fn read_snapshot() -> String {
    let Ok(entries) = fs::read_dir("/proc") else { return String::new() };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| {
            let io = fs::read_to_string(format!("/proc/{pid}/io")).ok()?;
            let (read, write) = parse_io(&io)?;
            Some(format!("{pid} {read} {write}\n"))
        })
        .collect()
}

fn parse_io(text: &str) -> Option<(u64, u64)> {
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name)?.trim().parse().ok())
    };
    Some((field("read_bytes:")?, field("write_bytes:")?))
}

#[cfg(test)]
mod tests {
    use super::parse_io;
    use crate::collect::process_io::parse_totals;

    #[test]
    fn reads_storage_bytes_from_proc_io() {
        let io = "rchar: 9\nwchar: 9\nsyscr: 1\nsyscw: 1\nread_bytes: 4096\nwrite_bytes: 8192\ncancelled_write_bytes: 0\n";
        assert_eq!(parse_io(io), Some((4096, 8192)));
        assert_eq!(parse_io("rchar: 9\n"), None);
    }

    #[test]
    fn snapshot_lines_parse_back_on_the_agent() {
        let (read, write) = parse_io("read_bytes: 10\nwrite_bytes: 20\n").unwrap();
        let totals = parse_totals(&format!("123 {read} {write}\n"));
        assert_eq!(totals[&123], (10, 20));
    }
}
