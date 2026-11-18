//! What every command shares, in any checkout: its scratch folder, private files, the commands
//! it runs and how it reports them.
use crate::{Result, Runner};
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Whether `feature` names one of the checkout's features at `root`: a folder of `features/`
/// whose name is lowercase letters, digits and underscores.
pub fn has_feature(root: &Path, feature: &str) -> bool {
    feature_directory(root, feature).is_some()
}
/// Resolves the stable feature id to its ordinary or DSP-owned folder.
pub fn feature_directory(root: &Path, feature: &str) -> Option<PathBuf> {
    if feature.is_empty()
        || !feature
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return None;
    }
    let ordinary = root.join("features").join(feature);
    if feature != "custom" && ordinary.is_dir() {
        return Some(ordinary);
    }
    let custom = root.join("features/custom");
    for owner in fs::read_dir(custom).ok()?.flatten() {
        let code = owner.file_name();
        let code = code.to_str()?;
        if let Some(folder) = feature.strip_prefix(&format!("{code}_")) {
            let dir = owner.path().join(folder);
            if dir.join("feature.rs").is_file() {
                return Some(dir);
            }
        }
    }
    None
}

/// Where the checkout named `name` keeps everything it makes outside itself: a folder of
/// `~/dispatch-platform/scratch`, by that folder's real path. The stores tests make refuse a
/// path through a symlink, and the MacBook keeps `~/dispatch-platform` behind one.
pub fn scratch(name: &str) -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or("HOME is required for the scratch folders.")?;
    Ok(real(&Path::new(&home).join("dispatch-platform/scratch"))?.join(name))
}
/// The scratch folder of the change named `name` as it is now: where `scratch` keeps it, unless
/// only the one it kept in `/tmp` before is there, which no command has moved yet.
pub fn current_scratch(name: &str) -> Result<PathBuf> {
    let dir = scratch(name)?;
    let old = left_in_tmp(name);
    Ok(match fs::symlink_metadata(&dir) {
        Err(_) if is_real_dir(&old) => old,
        _ => dir,
    })
}
/// Where a change kept its scratch folder before they moved home: macOS clears `/tmp`.
fn left_in_tmp(name: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/dispatch-{name}"))
}
/// Removes the link a move left at the change's old `/tmp` folder, once its folder is gone; a
/// real folder there, or a link to anything else, is left alone.
pub fn forget_left_in_tmp(name: &str, dir: &Path) -> Result<()> {
    let old = left_in_tmp(name);
    if fs::symlink_metadata(&old).is_ok_and(|meta| meta.file_type().is_symlink())
        && fs::read_link(&old)? == dir
    {
        fs::remove_file(old)?;
    }
    Ok(())
}
/// `path` by its real path as far as it is there, and as it is named past that.
fn real(path: &Path) -> Result<PathBuf> {
    match fs::canonicalize(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match (path.parent(), path.file_name()) {
                (Some(parent), Some(name)) => Ok(real(parent)?.join(name)),
                _ => Err(error.into()),
            }
        }
        real => Ok(real?),
    }
}
/// The name of the checkout at `root`: its folder's.
pub fn checkout_name(root: &Path) -> Result<&str> {
    root.file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "A checkout without a folder name".into())
}
/// The scratch folder of the checkout at `root`, by its real path: the one it kept in `/tmp`
/// moved into place if it has none yet, else made private if it is new. A folder in `/tmp` that
/// only a copy could move, onto another disk, stays where it is until `finish`: a command still
/// writing to it would lose what it wrote after the copy.
pub fn checkout_scratch(root: &Path) -> Result<PathBuf> {
    let name = checkout_name(root)?;
    let dir = scratch(name)?;
    let old = left_in_tmp(name);
    let there = fs::symlink_metadata(&dir).is_ok();
    if !move_into_place(&old, &dir, |from, to| fs::rename(from, to))? && !there && is_real_dir(&old)
    {
        return Ok(fs::canonicalize(old)?);
    }
    private_dir(&dir)?;
    Ok(fs::canonicalize(dir)?)
}
/// Whether `path` is a folder itself, never what a link names.
fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
}
/// A folder only this user can open, with the folders it is in, if it is new.
fn private_dir(dir: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    Ok(())
}
/// Moves the scratch folder `old` to `dir` by `rename`, once: only while `dir` isn't there and
/// `old` is a folder itself. It leaves a link at `old` and says so. False when there was
/// nothing to move, or the move would cross disks, which a rename can't.
fn move_into_place(
    old: &Path,
    dir: &Path,
    rename: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<bool> {
    if fs::symlink_metadata(dir).is_ok() || !is_real_dir(old) {
        return Ok(false);
    }
    private_dir(dir.parent().ok_or("A scratch folder without a parent")?)?;
    match rename(old, dir) {
        Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => return Ok(false),
        // Another command moved it meanwhile.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && dir.is_dir() => {
            return Ok(false);
        }
        moved => moved?,
    }
    crate::review::reconnect(dir, &crate::Native)?;
    // A reviewer reading a pass's checkout, or a brief naming it, still finds it by its old
    // path until `finish` removes the link with the folder.
    std::os::unix::fs::symlink(dir, old)?;
    eprintln!(
        "Moved the scratch folder {} to {}.",
        old.display(),
        dir.display()
    );
    Ok(true)
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
/// What `git` with `args` prints in `root`, trimmed.
pub fn git(runner: &dyn Runner, root: &Path, args: &[&str]) -> Result<String> {
    text(runner, &[&["git"][..], args].concat(), Some(root))
}
/// A path with its `.` and `..` worked out, so a file reached two ways is named one way.
pub fn normal(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                normal.pop();
            }
            part => normal.push(part),
        }
    }
    normal
}
/// The repository's Git folder that every worktree shares, by its absolute path.
pub fn shared_git(root: &Path, runner: &dyn Runner) -> Result<PathBuf> {
    let args = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
    Ok(PathBuf::from(git(runner, root, &args)?))
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
        // A signal diagnostic appended by the CLI must survive any last child output.
        // OpenOptions rejects append + truncate; the native flags support both atomically.
        .write(true)
        .custom_flags(libc::O_APPEND)
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
/// The disk everything under `path` takes, or none if it doesn't exist: in the kilobytes every
/// `du` counts, where only GNU's counts bytes.
pub fn size(runner: &dyn Runner, path: &Path) -> Option<u64> {
    if !path.exists() {
        return None;
    }
    let path = path.to_str()?;
    let kilobytes: u64 = text(runner, &["du", "-sk", path], None)
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(kilobytes * 1024)
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
    #[test]
    fn a_folders_size_is_measured_wherever_du_is() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("built"), vec![1u8; 8192]).unwrap();
        let measured = size(&crate::Native, temp.path());
        assert!(measured.is_some_and(|bytes| bytes >= 8192), "{measured:?}");
        assert_eq!(size(&crate::Native, &temp.path().join("gone")), None);
    }
    #[test]
    fn a_scratch_folder_moved_from_tmp_keeps_its_review_checkout_one_of_the_clones() {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let clone = base.join("repo");
        fs::create_dir(&clone).unwrap();
        let git = |args: &[&str]| {
            let identity = ["-c", "user.name=t", "-c", "user.email=t@t"];
            git(&crate::Native, &clone, &[&identity[..], args].concat()).unwrap()
        };
        git(&["init", "-q"]);
        git(&["commit", "-q", "--allow-empty", "-m", "x"]);
        let old = base.join("tmp/dispatch-change");
        let reading = old.join("review/1/checkout");
        git(&[
            "worktree",
            "add",
            "-q",
            "--detach",
            reading.to_str().unwrap(),
        ]);
        let dir = base.join("scratch/change");
        let rename = |from: &Path, to: &Path| fs::rename(from, to);
        assert!(move_into_place(&old, &dir, rename).unwrap());
        // The old path is a link to the new folder, so a reviewer reading it still can.
        assert_eq!(fs::read_link(&old).unwrap(), dir);
        assert!(old.join("review/1/checkout").is_dir());
        // A second command finds the link and moves nothing.
        assert!(!move_into_place(&old, &dir, rename).unwrap());
        assert!(dir.join("review/1/checkout").is_dir());
        // `take` and `finish` remove it through git, by where it is now.
        let moved = dir.join("review/1/checkout");
        git(&["worktree", "remove", "--force", moved.to_str().unwrap()]);
        assert!(!moved.exists());
    }
    #[test]
    fn finishing_removes_only_the_link_a_move_left_in_tmp() {
        let temp = tempfile::tempdir().unwrap();
        let name = format!("forget-{}", std::process::id());
        let old = left_in_tmp(&name);
        let dir = fs::canonicalize(temp.path()).unwrap().join(&name);
        // A real folder there is another change's to keep.
        fs::create_dir(&old).unwrap();
        forget_left_in_tmp(&name, &dir).unwrap();
        assert!(old.is_dir());
        fs::remove_dir(&old).unwrap();
        // So is a link to anything but this change's folder.
        std::os::unix::fs::symlink(temp.path().join("other"), &old).unwrap();
        forget_left_in_tmp(&name, &dir).unwrap();
        assert!(fs::symlink_metadata(&old).is_ok());
        fs::remove_file(&old).unwrap();
        std::os::unix::fs::symlink(&dir, &old).unwrap();
        forget_left_in_tmp(&name, &dir).unwrap();
        assert!(fs::symlink_metadata(&old).is_err());
    }
    #[test]
    fn a_folder_only_a_copy_could_move_stays_where_it_is() {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let old = base.join("tmp/dispatch-change");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("check.log"), "still written").unwrap();
        let dir = base.join("scratch/change");
        let across = |_: &Path, _: &Path| Err(std::io::ErrorKind::CrossesDevices.into());
        assert!(!move_into_place(&old, &dir, across).unwrap());
        assert_eq!(
            fs::read_to_string(old.join("check.log")).unwrap(),
            "still written"
        );
        assert!(!dir.exists());
    }
}
