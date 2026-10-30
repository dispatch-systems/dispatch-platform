//! What every command shares, in any checkout: its scratch folder, private files, the commands
//! it runs and how it reports them.
use crate::{Result, Runner};
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Whether `feature` names one of the checkout's features at `root`: a folder of `features/`
/// whose name is lowercase letters, digits and underscores.
pub fn has_feature(root: &Path, feature: &str) -> bool {
    !feature.is_empty()
        && feature
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && root.join("features").join(feature).is_dir()
}

/// Where the checkout named `name` keeps everything it makes outside itself.
pub fn scratch(name: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/dispatch-{name}"))
}
/// The name of the checkout at `root`: its folder's.
pub fn checkout_name(root: &Path) -> Result<&str> {
    root.file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "A checkout without a folder name".into())
}
/// The scratch folder of the checkout at `root`, made private if it is new, by its real path:
/// the stores tests make refuse a path through a symlink, and macOS keeps `/tmp` behind one.
pub fn checkout_scratch(root: &Path) -> Result<PathBuf> {
    Ok(fs::canonicalize(scratch_dir(checkout_name(root)?)?)?)
}
/// The scratch folder of the checkout named `name`, made private if it is new.
fn scratch_dir(name: &str) -> Result<PathBuf> {
    let dir = scratch(name);
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    Ok(dir)
}
/// A file only this user can read, replaced whole.
pub fn write_private(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }
    let partial = path.with_extension(format!("{}.partial", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&partial)?;
    file.write_all(text.as_bytes())?;
    fs::rename(partial, path)?;
    Ok(())
}
/// A command's output, trimmed.
pub fn text(runner: &dyn Runner, args: &[&str], cwd: Option<&Path>) -> Result<String> {
    Ok(String::from_utf8(runner.command(args, cwd, 120)?)?
        .trim()
        .to_owned())
}
/// The slice every command `logged` runs goes in, from every worktree at once. Between them they
/// have a low CPU weight and one memory ceiling: at `MemoryMax` they swap, and past the swap
/// they may use, the kernel ends one of them, never anything outside.
const SLICE: &str = "app-dispatchdev.slice";
const CEILINGS: [&str; 3] = ["CPUWeight=20", "MemoryMax=4G", "MemorySwapMax=2G"];
/// Whether this user's service manager runs commands in scopes of their own, as on this
/// machine, answered by setting `SLICE`'s ceilings until the next boot; CI's runners have none.
fn scopes() -> bool {
    static SCOPES: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SCOPES.get_or_init(|| {
        Command::new("systemctl")
            .args(["--user", "set-property", "--runtime", SLICE])
            .args(CEILINGS)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}
/// The unit of this user's service manager running the process whose `/proc/<pid>/cgroup` reads
/// `cgroup`, such as an agent's session; none for a login's, which the system's manager runs.
fn unit_of(cgroup: &str) -> Option<&str> {
    let path = cgroup.lines().find_map(|line| line.strip_prefix("0::"))?;
    let mut parts = path.split('/');
    parts.find(|part| part.starts_with("user@") && part.ends_with(".service"))?;
    parts
        .next_back()
        .filter(|unit| unit.ends_with(".scope") || unit.ends_with(".service"))
}
/// Runs `command` through the shell in `cwd` with `env` added, its output going to `log`, and
/// answers whether it passed and how long it took. It runs below Dev, in a scope of `SLICE`, and
/// first in line for the out-of-memory killer, ahead of the agents' sessions: a command that
/// fails can run again. The scope is bound to the unit running this, so an agent's
/// session that ends, even one killed for memory, ends the commands it started.
pub fn logged(
    command: &str,
    cwd: &Path,
    log: &Path,
    env: &[(&str, &std::ffi::OsStr)],
) -> Result<(bool, Duration)> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(log)?;
    let mut process = if scopes() {
        let mut process = Command::new("systemd-run");
        process.args([
            "--user",
            "--scope",
            "--quiet",
            "--collect",
            "--slice",
            SLICE,
        ]);
        let cgroup = fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
        if let Some(unit) = unit_of(&cgroup) {
            process.args([
                "-p",
                &format!("BindsTo={unit}"),
                "-p",
                &format!("After={unit}"),
            ]);
        }
        process.args(["--", "choom", "-n", "900", "--"]);
        process
    } else {
        let mut process = Command::new("nice");
        process.args(["-n", "10"]);
        process
    };
    let started = Instant::now();
    let status = process
        .args(["sh", "-c", command])
        .current_dir(cwd)
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .stdout(file.try_clone()?)
        .stderr(file)
        .status()?;
    Ok((status.success(), started.elapsed()))
}
/// The size of everything under `path`, or none if it doesn't exist.
pub fn size(runner: &dyn Runner, path: &Path) -> Option<u64> {
    if !path.exists() {
        return None;
    }
    let path = path.to_str()?;
    text(runner, &["du", "-sb", path], None)
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}
/// Bytes as people read them: 13.2 GB, 830 MB.
pub fn human(bytes: u64) -> String {
    const GB: f64 = 1_000_000_000.0;
    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.1} GB", bytes / GB)
    } else {
        format!("{:.0} MB", bytes / 1_000_000.0)
    }
}
/// Seconds as people read them: 45s, 3m 05s.
pub fn duration(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}
/// What to do after a command, as the scaffolder prints it: `Next:`, then a line each.
pub fn next(notes: &[String]) -> String {
    std::iter::once("Next:".to_owned())
        .chain(notes.iter().map(|note| format!("  - {note}")))
        .collect::<Vec<_>>()
        .join("\n")
}
/// An argument as the shell reads it back: as it is when that's safe, else in single quotes.
pub fn quote(arg: &str) -> String {
    let plain = |b: u8| b.is_ascii_alphanumeric() || b"@%+=:,./_-".contains(&b);
    if !arg.is_empty() && arg.bytes().all(plain) {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_command_is_bound_to_the_unit_of_this_users_manager_running_it() {
        let uid = 1001;
        let agent = format!(
            "0::/user.slice/user-{uid}.slice/user@{uid}.service/app.slice/app-t3code.slice/t3code-claude-d0fa7f0d.scope\n"
        );
        assert_eq!(unit_of(&agent), Some("t3code-claude-d0fa7f0d.scope"));
        // A login's session belongs to the system's manager, which this user's can't bind to.
        let login = format!("0::/user.slice/user-{uid}.slice/session-3.scope\n");
        assert_eq!(unit_of(&login), None);
    }
    #[test]
    fn sizes_and_times_read_plainly() {
        assert_eq!(human(13_200_000_000), "13.2 GB");
        assert_eq!(human(830_000_000), "830 MB");
        assert_eq!(duration(Duration::from_secs(45)), "45s");
        assert_eq!(duration(Duration::from_secs(185)), "3m 05s");
    }
}
