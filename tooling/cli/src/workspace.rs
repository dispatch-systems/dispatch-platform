//! The workspace: the folder holding `dev/`, the live Dev checkout, and `worktrees/`, one per
//! change, on the machine that decides what runs there beside GitHub. Every checkout shares
//! Git's data in `dev/.git`, so the workspace is found from any of them, or from the workspace
//! folder itself.
use crate::{Result, Runner, machine::Machine, require};
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
    pub machine: Machine,
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
                return Ok(Self {
                    root,
                    machine: Machine::THIS,
                });
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

/// Whether `feature` names one of the checkout's features at `root`: a folder of `features/`
/// whose name is lowercase letters, digits and underscores.
pub fn has_feature(root: &Path, feature: &str) -> bool {
    !feature.is_empty()
        && feature
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && root.join("features").join(feature).is_dir()
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
/// The name of the checkout at `root`: its folder's, which is its change's.
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
/// Time as a wait keeps to it: the time now, on a monotonic clock, and a pause of some seconds
/// on it.
pub struct Clock<'a> {
    pub now: &'a dyn Fn() -> Instant,
    pub pause: &'a dyn Fn(u64),
}
/// Seconds before a `gh` call GitHub answered with a passing server error is made again.
const RETRY: u64 = 5;
/// `runner`, with a `gh` call GitHub answered with a passing server error, HTTP 502, 503 or 504,
/// made again up to twice, `RETRY` seconds after each, all within the time the call was given,
/// on `clock`: a try gets what is left of it, and none starts without some left after its pause.
pub struct Retried<'a> {
    pub runner: &'a dyn Runner,
    pub clock: &'a Clock<'a>,
}
impl Runner for Retried<'_> {
    fn command(&self, args: &[&str], cwd: Option<&Path>, timeout: u64) -> Result<Vec<u8>> {
        let deadline = (self.clock.now)() + Duration::from_secs(timeout);
        let left = || {
            deadline
                .saturating_duration_since((self.clock.now)())
                .as_secs()
        };
        let (mut tries, mut given) = (0, timeout);
        loop {
            match self.runner.command(args, cwd, given) {
                Err(error)
                    if args.first() == Some(&"gh")
                        && tries < 2
                        && left() > RETRY
                        && ["HTTP 502", "HTTP 503", "HTTP 504"]
                            .iter()
                            .any(|code| error.to_string().contains(code)) =>
                {
                    tries += 1;
                    (self.clock.pause)(RETRY);
                    given = left();
                }
                answer => return answer,
            }
        }
    }
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
/// first in line for the out-of-memory killer, ahead of the agents' sessions and the previews:
/// a command that fails can run again. The scope is bound to the unit running this, so an agent's
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
/// Bytes free where the workspace `root`'s worktrees build: `worktrees/`, or the disk it links to.
pub fn worktrees_free(root: &Path) -> Result<u64> {
    free_bytes(&root.join("worktrees"))
}
/// Bytes free on the file system holding `path`, or what it links to.
pub fn free_bytes(path: &Path) -> Result<u64> {
    fs2::available_space(path).map_err(|_| "Couldn't read the free disk space".into())
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
    fn a_passing_server_error_from_github_is_tried_again_twice() {
        /// Fails each command with `error` its first `fails` times.
        struct Flaky {
            error: &'static str,
            fails: u32,
            calls: std::cell::Cell<u32>,
        }
        impl Runner for Flaky {
            fn command(&self, _: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
                self.calls.set(self.calls.get() + 1);
                if self.calls.get() <= self.fails {
                    return Err(self.error.into());
                }
                Ok(b"done".to_vec())
            }
        }
        let flaky = |error, fails| Flaky {
            error,
            fails,
            calls: std::cell::Cell::new(0),
        };
        let paused = std::cell::Cell::new(0);
        let start = Instant::now();
        let clock = Clock {
            now: &|| start,
            pause: &|seconds| paused.set(paused.get() + seconds),
        };
        let call = |runner: &Flaky, args: &[&str]| {
            Retried {
                runner,
                clock: &clock,
            }
            .command(args, None, 60)
        };
        let gateway = "gh workflow run failed: HTTP 504: Gateway Timeout";
        let runner = flaky(gateway, 2);
        assert_eq!(call(&runner, &["gh", "workflow", "run"]).unwrap(), b"done");
        assert_eq!((runner.calls.get(), paused.get()), (3, 10));
        // A third in a row is the answer, and so is any other error, or one from git.
        assert!(call(&flaky(gateway, 3), &["gh", "workflow", "run"]).is_err());
        let missing = flaky("gh api failed: HTTP 404: Not Found", 1);
        assert!(call(&missing, &["gh", "api", "x"]).is_err());
        assert_eq!(missing.calls.get(), 1);
        let git = flaky("git push failed: HTTP 503", 1);
        assert!(call(&git, &["git", "push"]).is_err());
        assert_eq!(git.calls.get(), 1);
    }
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
        let ws = Workspace {
            root: root.clone(),
            machine: Machine::Linux,
        };
        assert_eq!(ws.worktrees().unwrap(), ["one"]);
    }
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
