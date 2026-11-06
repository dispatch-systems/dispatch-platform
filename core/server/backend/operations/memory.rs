use crate::Result;
use std::{collections::HashSet, path::Path};

/// This process's resident memory, and its descendants' together.
pub fn memory() -> (u64, u64) {
    fn descendants(pid: u32, seen: &mut HashSet<u32>) -> u64 {
        let mut total = 0;
        for child in process::children(pid) {
            if seen.insert(child) {
                total += process::rss(child) + descendants(child, seen);
            }
        }
        total
    }
    let pid = std::process::id();
    (
        process::rss(pid),
        descendants(pid, &mut HashSet::from([pid])),
    )
}
/// A process's resident memory and children, as Linux's `/proc` tells them.
#[cfg(target_os = "linux")]
mod process {
    use std::fs;
    pub fn rss(pid: u32) -> u64 {
        fs::read_to_string(format!("/proc/{pid}/status"))
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("VmRSS:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .unwrap_or(0)
            * 1024
    }
    pub fn children(pid: u32) -> Vec<u32> {
        let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) else {
            return vec![];
        };
        tasks
            .flatten()
            .filter_map(|task| fs::read_to_string(task.path().join("children")).ok())
            .flat_map(|children| {
                children
                    .split_whitespace()
                    .filter_map(|c| c.parse().ok())
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}
/// A process's resident memory and children, as macOS's libproc tells them, for a Mac's dev
/// server.
#[cfg(target_os = "macos")]
mod process {
    use std::mem::size_of;
    pub fn rss(pid: u32) -> u64 {
        let Ok(pid) = libc::c_int::try_from(pid) else {
            return 0;
        };
        let size = size_of::<libc::proc_taskinfo>() as libc::c_int;
        // SAFETY: proc_pidinfo writes at most `size` bytes into `info`, which is that size.
        let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
        let read = unsafe {
            libc::proc_pidinfo(pid, libc::PROC_PIDTASKINFO, 0, (&raw mut info).cast(), size)
        };
        if read == size {
            info.pti_resident_size
        } else {
            0
        }
    }
    pub fn children(pid: u32) -> Vec<u32> {
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return vec![];
        };
        let mut pids: Vec<libc::pid_t> = vec![0; 1024];
        let bytes = (pids.len() * size_of::<libc::pid_t>()) as libc::c_int;
        // SAFETY: proc_listchildpids writes at most `bytes` bytes into `pids` and returns how
        // many pids it wrote.
        let count = unsafe { libc::proc_listchildpids(pid, pids.as_mut_ptr().cast(), bytes) };
        pids.truncate(usize::try_from(count).unwrap_or(0));
        pids.into_iter()
            .filter_map(|p| u32::try_from(p).ok())
            .filter(|&p| p > 0)
            .collect()
    }
}
pub fn available_space(path: &Path) -> Result<u64> {
    Ok(fs2::available_space(path)?)
}
