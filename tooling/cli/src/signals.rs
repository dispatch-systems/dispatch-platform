//! Name the sender before retaining the CLI's usual signal termination. Only the first signal
//! wakes the reporter; the handler does no allocation, lookup or logging.
use crate::Result;
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{fs::OpenOptionsExt, net::UnixStream},
    },
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

static CLAIMED: AtomicBool = AtomicBool::new(false);
static SIGNAL: AtomicI32 = AtomicI32::new(0);
static PID: AtomicI32 = AtomicI32::new(0);
static UID: AtomicU32 = AtomicU32::new(0);
static WAKE: AtomicI32 = AtomicI32::new(-1);
static LOG: Mutex<Option<PathBuf>> = Mutex::new(None);

extern "C" fn received(signal: i32, info: *mut libc::siginfo_t, _: *mut libc::c_void) {
    if CLAIMED.swap(true, Ordering::SeqCst) {
        return;
    }
    // SA_SIGINFO supplies this record on Linux and macOS. These accessors only read it.
    unsafe {
        PID.store((*info).si_pid(), Ordering::Relaxed);
        UID.store((*info).si_uid(), Ordering::Relaxed);
    }
    SIGNAL.store(signal, Ordering::Release);
    // One byte, once, into an otherwise empty socket. write is async-signal-safe.
    unsafe { libc::write(WAKE.load(Ordering::Relaxed), b"!".as_ptr().cast(), 1) };
}

/// Install before starting any command or its children, using only the existing libc crate.
pub fn install() -> Result<()> {
    let (mut reader, writer) = UnixStream::pair()?;
    writer.set_nonblocking(true)?;
    WAKE.store(writer.as_raw_fd(), Ordering::Relaxed);
    std::thread::Builder::new()
        .name("signal-sender".into())
        .spawn(move || {
            // Keep the write descriptor alive; both descriptors are close-on-exec.
            let _writer = writer;
            if reader.read_exact(&mut [0]).is_ok() {
                report();
            }
        })?;
    for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        // Zero initialises the platform's sigaction layout; libc owns those differences.
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = received as *const () as usize;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
        unsafe {
            libc::sigemptyset(&mut action.sa_mask);
            if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
    }
    Ok(())
}

/// A command whose child ended from the same group signal must not exit ahead of its reporter.
pub fn finish() {
    if CLAIMED.load(Ordering::SeqCst) {
        loop {
            std::thread::park();
        }
    }
}

/// The first step's log already names the run, including while it waits for a build slot.
pub(crate) fn run_log(path: PathBuf) -> RunLog {
    RunLog(
        LOG.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .replace(path),
    )
}
pub(crate) struct RunLog(Option<PathBuf>);
impl Drop for RunLog {
    fn drop(&mut self) {
        // Preserve the active log on an error path while its signal is being reported.
        finish();
        *LOG.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = self.0.take();
    }
}

fn report() {
    // A closed stderr pipe must not replace the original signal with SIGPIPE. This mask
    // belongs only to the reporter thread; commands and their children keep theirs.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGPIPE);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
    let signal = SIGNAL.load(Ordering::Acquire);
    let pid = PID.load(Ordering::Relaxed);
    let uid = UID.load(Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let (command, parent) = sender(pid);
    let name = match signal {
        libc::SIGTERM => "SIGTERM",
        libc::SIGINT => "SIGINT",
        libc::SIGHUP => "SIGHUP",
        _ => unreachable!(),
    };
    // JSON quoting keeps arbitrary command-line bytes on one plain line.
    let line = format!(
        "dispatchdev: received {name} ({signal}) at {}.{:03} UTC (unix); sender pid={pid} uid={uid} ppid={} command={}\n",
        timestamp.as_secs(),
        timestamp.subsec_millis(),
        parent.map_or_else(|| "unknown".into(), |pid| pid.to_string()),
        serde_json::to_string(&command.unwrap_or_else(|| "unknown".into())).unwrap(),
    );
    // Avoid the stderr lock: the interrupted thread could hold it while reporting a failure.
    unsafe { libc::write(libc::STDERR_FILENO, line.as_ptr().cast(), line.len()) };
    let log = LOG
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(path) = log
        && let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)
    {
        let _ = file.write_all(line.as_bytes());
    }
    // Preserve signal death (and shell status 128+n). No forwarding: group delivery and
    // the kernel's descriptor/lock cleanup remain exactly as they were.
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
        libc::_exit(128 + signal);
    }
}

#[cfg(target_os = "linux")]
fn sender(pid: i32) -> (Option<String>, Option<u32>) {
    let command = std::fs::read(format!("/proc/{pid}/cmdline"))
        .ok()
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .trim_end_matches('\0')
                .replace('\0', " ")
        });
    // The name in stat can itself contain spaces and parentheses; the last ')' ends it.
    let parent = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            stat.rsplit_once(')')?
                .1
                .split_whitespace()
                .nth(1)?
                .parse()
                .ok()
        });
    (command, parent)
}

#[cfg(target_os = "macos")]
fn sender(pid: i32) -> (Option<String>, Option<u32>) {
    // libproc reads the parent without launching ps. A denied lookup stays unknown.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&info) as i32;
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    let parent = (read == size).then_some(info.pbi_ppid);
    // KERN_PROCARGS2 returns argc, executable path, padding, then argv (and environment).
    // Stop at argc: environment variables must never enter the diagnostic.
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut bytes = vec![0u8; 1024 * 1024];
    let mut length = bytes.len();
    let read = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            bytes.as_mut_ptr().cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    let command = if read == 0 && length >= 4 {
        bytes.truncate(length);
        let argc = i32::from_ne_bytes(bytes[..4].try_into().unwrap());
        let payload = &bytes[4..];
        payload.iter().position(|b| *b == 0).and_then(|end| {
            let args = &payload[end + 1..];
            let start = args.iter().position(|b| *b != 0)?;
            let args: Vec<_> = args[start..]
                .split(|b| *b == 0)
                .take(argc.max(0) as usize)
                .map(String::from_utf8_lossy)
                .collect();
            (!args.is_empty()).then(|| args.join(" "))
        })
    } else {
        None
    };
    (command, parent)
}
