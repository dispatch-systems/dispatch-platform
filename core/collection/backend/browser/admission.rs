//! Admission is conservative: reserve 1 GiB per browser plus 512 MiB for the
//! platform/host. MemAvailable already includes resident browser pages, so only
//! their remaining growth allowance is reserved again. Never kill active work.
pub use crate::platform_owner::api::types::BrowserAdmission as Admission;
use std::{
    fs,
    path::{Component, Path},
};
pub const BROWSER_BYTES: u64 = 1024 * 1024 * 1024;
const HEADROOM_BYTES: u64 = 512 * 1024 * 1024;
impl Admission {
    pub fn new(available: Option<u64>, residents: impl Iterator<Item = u64>) -> Self {
        let required = residents.fold(BROWSER_BYTES + HEADROOM_BYTES, |total, resident| {
            total.saturating_add(BROWSER_BYTES.saturating_sub(resident))
        });
        Self {
            available_bytes: available,
            required_bytes: required,
            can_start: available.is_some_and(|bytes| bytes >= required),
        }
    }
}
fn number(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
fn constrained(mut available: u64, root: &Path, group: &str) -> Option<u64> {
    // cgroup v2 limits can be inherited. Check every visible ancestor; a
    // namespace-relative cgroup root is also covered by the final iteration.
    if !Path::new(group)
        .components()
        .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
    {
        return None;
    }
    let mut path = root.join(group.trim_start_matches('/'));
    loop {
        match fs::read_to_string(path.join("memory.max")) {
            Ok(value) if value.trim() == "max" => (),
            Ok(value) => {
                let limit = value.trim().parse::<u64>().ok()?;
                let current = number(&path.join("memory.current"))?;
                available = available.min(limit.saturating_sub(current));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return None,
        }
        if path == root || !path.pop() {
            break;
        }
    }
    Some(available)
}
pub fn available() -> Option<u64> {
    let mem = fs::read_to_string("/proc/meminfo").ok()?;
    let available = mem
        .lines()
        .find_map(|line| line.strip_prefix("MemAvailable:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)?;
    let cgroup = fs::read_to_string("/proc/self/cgroup").ok()?;
    match cgroup.lines().find_map(|line| line.strip_prefix("0::")) {
        Some(group) => constrained(available, Path::new("/sys/fs/cgroup"), group),
        None => Some(available),
    }
}
#[cfg(test)]
#[path = "../../tests/backend/browser/admission.rs"]
mod tests;
