//! The workspace: the folder holding `dev/`, the live Dev checkout, and `worktrees/`, one per
//! change. Every checkout shares Git's data in `dev/.git`, so the workspace is found from any
//! of them, or from the workspace folder itself.
use crate::{Result, Runner, require};
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub struct Workspace {
    pub root: PathBuf,
}
impl Workspace {
    /// The workspace `from` is in: a checkout's, or the folder holding `dev/` and `worktrees/`.
    pub fn find(from: &Path, runner: &dyn Runner) -> Result<Self> {
        let common = runner
            .command(
                &[
                    "git",
                    "rev-parse",
                    "--path-format=absolute",
                    "--git-common-dir",
                ],
                Some(from),
                30,
            )
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(|text| PathBuf::from(text.trim()));
        let candidates = common
            .iter()
            .filter_map(|git| git.parent()?.parent().map(Path::to_path_buf))
            .chain(from.ancestors().map(Path::to_path_buf));
        for root in candidates {
            if root.join("dev/.git").is_dir() && root.join("worktrees").is_dir() {
                return Ok(Self { root });
            }
        }
        Err("Run dispatchdev in the workspace: the folder holding dev/ and worktrees/, or a checkout in it.".into())
    }
    pub fn dev(&self) -> PathBuf {
        self.root.join("dev")
    }
    pub fn worktree(&self, name: &str) -> PathBuf {
        self.root.join("worktrees").join(name)
    }
    /// Every worktree's folder name, in order.
    pub fn worktrees(&self) -> Result<Vec<String>> {
        let mut names: Vec<String> = fs::read_dir(self.root.join("worktrees"))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().join(".git").exists())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        names.sort();
        Ok(names)
    }
}

/// A change's name, which is its worktree's folder, its branch and its preview's unit: lowercase
/// letters, digits and hyphens.
pub fn valid_name(name: &str) -> Result<()> {
    require(
        !name.is_empty()
            && name.len() <= 60
            && !name.starts_with('-')
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "A change's name is lowercase letters, digits and hyphens, such as audit-driver-match.",
    )
}
/// Where a change keeps everything it makes outside its worktree.
pub fn scratch(name: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/dispatch-{name}"))
}
/// The change's scratch folder, made private if it is new.
pub fn scratch_dir(name: &str) -> Result<PathBuf> {
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
/// Whether this user can run a command in a systemd scope of its own, as on this machine; CI's
/// runners can't.
fn scopes() -> bool {
    static SCOPES: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SCOPES.get_or_init(|| {
        Command::new("systemd-run")
            .args(["--user", "--scope", "--quiet", "--collect", "true"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}
/// Runs `command` through the shell in `cwd` with `env` added, its output going to `log`, and
/// answers whether it passed and how long it took. It runs below Dev: in a scope with a low CPU
/// weight and a soft memory ceiling, and first in line for the out-of-memory killer after the
/// previews, so tests running side by side never starve Dev.
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
            "-p",
            "CPUWeight=20",
            "-p",
            "MemoryHigh=3G",
            "--",
            "choom",
            "-n",
            "300",
            "--",
        ]);
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
/// Bytes free on the file system holding `path`.
pub fn free_bytes(path: &Path) -> Result<u64> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = CString::new(path.as_os_str().as_bytes())?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    require(
        unsafe { libc::statvfs(path.as_ptr(), &mut stat) } == 0,
        "Couldn't read the free disk space",
    )?;
    Ok(stat.f_bavail * stat.f_frsize)
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_folders_branches_and_units_at_once() {
        for good in ["audit-driver-match", "fix2", "a"] {
            assert!(valid_name(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "-x",
            "Audit",
            "a b",
            "a/b",
            "a.b",
            "a_b",
            &"a".repeat(61),
        ] {
            assert!(valid_name(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn the_workspace_is_found_from_it_or_any_checkout_in_it() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::create_dir_all(root.join("dev/.git")).unwrap();
        fs::create_dir_all(root.join("worktrees/one/src")).unwrap();
        fs::write(root.join("worktrees/one/.git"), "gitdir: elsewhere").unwrap();
        let none = |_: &[&str]| -> Result<Vec<u8>> { Err("no git".into()) };
        struct Fake<F: Fn(&[&str]) -> Result<Vec<u8>>>(F);
        impl<F: Fn(&[&str]) -> Result<Vec<u8>>> Runner for Fake<F> {
            fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
                (self.0)(args)
            }
        }
        for from in [root.clone(), root.join("worktrees/one/src")] {
            assert_eq!(Workspace::find(&from, &Fake(none)).unwrap().root, root);
        }
        let git = root.join("dev/.git");
        let shared = move |_: &[&str]| Ok(format!("{}\n", git.display()).into_bytes());
        let elsewhere = tempfile::tempdir().unwrap();
        assert_eq!(
            Workspace::find(elsewhere.path(), &Fake(shared))
                .unwrap()
                .root,
            root
        );
        assert!(Workspace::find(elsewhere.path(), &Fake(none)).is_err());
        assert_eq!(
            Workspace { root: root.clone() }.worktrees().unwrap(),
            ["one"]
        );
    }
    #[test]
    fn sizes_and_times_read_plainly() {
        assert_eq!(human(13_200_000_000), "13.2 GB");
        assert_eq!(human(830_000_000), "830 MB");
        assert_eq!(duration(Duration::from_secs(45)), "45s");
        assert_eq!(duration(Duration::from_secs(185)), "3m 05s");
    }
}
