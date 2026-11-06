mod docker_api;
mod docker_file;
#[allow(dead_code)]
mod docker_read_protocol;

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Stdout, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::{self, ExitCode};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use docker_api::{Cancel, Sink};
use docker_read_protocol::{
    ALLOW_FILE, DockerRead, FrameKind, ListTarget, MAX_CONTAINERS, MAX_FRAME_BYTES,
    MAX_REQUEST_BYTES, Request, TARGETS_ENV, parse_targets, write_exit, write_frame,
};

const MAX_STREAMS: usize = MAX_CONTAINERS + 16;
const THREAD_STACK: usize = 256 * 1024;

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("daroyan-docker-read: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<u8, String> {
    let args = env::args_os()
        .skip(1)
        .map(|arg| arg.into_string().map_err(|_| "arguments must be UTF-8".to_string()))
        .collect::<Result<Vec<String>, String>>()?;

    if args == ["serve"] {
        lock_down()?;
        serve();
    }

    let targets = load_targets()?;
    let read = DockerRead::parse(&args, &targets)?;
    lock_down()?;
    let code = docker_api::run(&read, &targets, &Cancel::default(), &mut StdSink);

    Ok(u8::try_from(code).unwrap_or(1))
}

fn load_targets() -> Result<Vec<ListTarget>, String> {
    let is_root = nix::unistd::geteuid().is_root();
    if !is_root && let Ok(text) = env::var(TARGETS_ENV) {
        return Ok(parse_targets(&text));
    }

    let allow_file = Path::new(ALLOW_FILE);
    check_owned_by_root(allow_file)?;
    let text = fs::read_to_string(allow_file)
        .map_err(|error| format!("read {ALLOW_FILE}: {error}"))?;

    Ok(parse_targets(&text))
}

fn check_owned_by_root(path: &Path) -> Result<(), String> {
    let directory = path.parent().ok_or("the allow file has no folder")?;
    for checked in [path, directory] {
        let metadata = fs::symlink_metadata(checked)
            .map_err(|error| format!("read {}: {error}", checked.display()))?;
        if metadata.file_type().is_symlink()
            || metadata.uid() != 0
            || metadata.mode() & 0o022 != 0
        {
            return Err(format!(
                "{} must be owned by root and writable only by root",
                checked.display()
            ));
        }
    }

    Ok(())
}

struct StdSink;

impl Sink for StdSink {
    fn out(&mut self, data: &[u8]) -> io::Result<()> {
        io::stdout().write_all(data)
    }

    fn err(&mut self, data: &[u8]) -> io::Result<()> {
        io::stderr().write_all(data)
    }
}

type Output = Arc<Mutex<Stdout>>;

struct FrameSink {
    stream: u32,
    output: Output,
}

impl FrameSink {
    fn send(&self, kind: FrameKind, data: &[u8]) -> io::Result<()> {
        for chunk in data.chunks(MAX_FRAME_BYTES) {
            write_frame(&mut *lock(&self.output), self.stream, kind, chunk)?;
        }

        Ok(())
    }
}

impl Sink for FrameSink {
    fn out(&mut self, data: &[u8]) -> io::Result<()> {
        self.send(FrameKind::Stdout, data)
    }

    fn err(&mut self, data: &[u8]) -> io::Result<()> {
        self.send(FrameKind::Stderr, data)
    }
}

fn serve() -> ! {
    let output: Output = Arc::new(Mutex::new(io::stdout()));
    let open: Arc<Mutex<HashMap<u32, Arc<Cancel>>>> = Arc::default();
    let mut input = BufReader::new(io::stdin().lock());
    let mut line = Vec::new();

    loop {
        line.clear();
        let read = (&mut input)
            .take((MAX_REQUEST_BYTES + 1) as u64)
            .read_until(b'\n', &mut line);
        match read {
            Ok(0) | Err(_) => process::exit(0),
            Ok(_) if line.len() > MAX_REQUEST_BYTES => process::exit(1),
            Ok(_) => {}
        }

        let request = String::from_utf8(line.clone())
            .map_err(|_| "requests must be UTF-8".to_string())
            .and_then(|text| Request::parse(&text));
        match request {
            Ok(Request::Open { stream, args }) => {
                open_stream(stream, args, &open, &output);
            }
            Ok(Request::Close { stream }) => {
                if let Some(cancel) = lock(&open).remove(&stream) {
                    cancel.cancel();
                }
            }
            Err(_) => process::exit(1),
        }
    }
}

fn open_stream(
    stream: u32,
    args: Vec<String>,
    open: &Arc<Mutex<HashMap<u32, Arc<Cancel>>>>,
    output: &Output,
) {
    let cancel = Arc::new(Cancel::default());
    let is_full = {
        let mut open = lock(open);
        let is_full = open.len() >= MAX_STREAMS || open.contains_key(&stream);
        if !is_full {
            open.insert(stream, Arc::clone(&cancel));
        }
        is_full
    };
    let mut sink = FrameSink { stream, output: Arc::clone(output) };
    if is_full {
        let _ = sink.err(b"daroyan-docker-read: too many open reads\n");
        finish(output, stream, 1);
        return;
    }

    let task_open = Arc::clone(open);
    let task_output = Arc::clone(output);
    let spawned = thread::Builder::new()
        .name(format!("read-{stream}"))
        .stack_size(THREAD_STACK)
        .spawn(move || {
            let code = match load_targets().and_then(|targets| {
                DockerRead::parse(&args, &targets).map(|read| (read, targets))
            }) {
                Ok((read, targets)) => {
                    docker_api::run(&read, &targets, &cancel, &mut sink)
                }
                Err(error) => {
                    let _ =
                        sink.err(format!("daroyan-docker-read: {error}\n").as_bytes());
                    1
                }
            };
            lock(&task_open).remove(&stream);
            finish(&task_output, stream, code);
        });

    if spawned.is_err() {
        lock(open).remove(&stream);
        finish(output, stream, 1);
    }
}

fn finish(output: &Output, stream: u32, code: i32) {
    if write_exit(&mut *lock(output), stream, code).is_err() {
        process::exit(0);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(target_os = "linux")]
fn lock_down() -> Result<(), String> {
    use nix::libc;
    use nix::sys::prctl;
    use seccompiler::{BpfProgram, SeccompAction, SeccompFilter};

    const DENIED_SYSCALLS: [i64; 22] = [
        libc::SYS_execve,
        libc::SYS_execveat,
        libc::SYS_ptrace,
        libc::SYS_process_vm_readv,
        libc::SYS_process_vm_writev,
        libc::SYS_pidfd_getfd,
        libc::SYS_open_by_handle_at,
        libc::SYS_name_to_handle_at,
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
        libc::SYS_kexec_load,
        libc::SYS_init_module,
        libc::SYS_finit_module,
        libc::SYS_delete_module,
    ];

    prctl::set_dumpable(false)
        .map_err(|error| format!("mark the helper not dumpable: {error}"))?;
    let rules = DENIED_SYSCALLS.iter().map(|&syscall| (syscall, Vec::new())).collect();
    let arch = env::consts::ARCH
        .try_into()
        .map_err(|error| format!("seccomp does not know this machine: {error:?}"))?;
    let filter = SeccompFilter::new(
        rules,
        SeccompAction::Allow,
        SeccompAction::Errno(libc::EPERM as u32),
        arch,
    )
    .map_err(|error| format!("build the seccomp filter: {error}"))?;
    let program: BpfProgram = filter
        .try_into()
        .map_err(|error| format!("compile the seccomp filter: {error:?}"))?;

    seccompiler::apply_filter(&program)
        .map_err(|error| format!("apply the seccomp filter: {error}"))
}

#[cfg(not(target_os = "linux"))]
fn lock_down() -> Result<(), String> {
    Ok(())
}
