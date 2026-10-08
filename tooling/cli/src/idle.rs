//! Idle worktrees' builds. A worktree keeps its build in `target/`, 3–7 GB, until `dispatchdev
//! finish`. Before `start` and `preview`, the builds of worktrees unused for days go, and with
//! little disk free those unused for hours too, oldest first. A worktree whose preview runs, or
//! whose build Cargo is making, keeps its own. Only the build goes: the worktree's source,
//! commits and packages stay, and its next build starts over or reuses the build cache.
use crate::{
    Result, Runner,
    workspace::{self, Workspace},
};
use fs2::FileExt;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// A build unused this long goes before any `start` or `preview`.
pub const IDLE: Duration = Duration::from_secs(3 * 24 * 60 * 60);
/// With less than `start::LOW_DISK` free, a build unused this long goes too.
pub const PRESSED: Duration = Duration::from_secs(12 * 60 * 60);

/// How long the worktree at `path` has gone unused: since its Git index changed, as every
/// commit, checkout and staging does, or since it last built, whichever is later.
pub fn unused(path: &Path) -> Option<Duration> {
    let index = fs::read_to_string(path.join(".git"))
        .ok()
        .and_then(|text| {
            Some(
                path.join(text.strip_prefix("gitdir:")?.trim())
                    .join("index"),
            )
        })
        .and_then(|index| newest(&index, 0));
    let used = [
        index,
        newest(&path.join("target"), 3),
        newest(&path.join(".test-build"), 1),
    ]
    .into_iter()
    .flatten()
    .max()?;
    Some(SystemTime::now().duration_since(used).unwrap_or_default())
}
/// When `path` or anything up to `depth` levels below it last changed, without following links.
fn newest(path: &Path, depth: usize) -> Option<SystemTime> {
    let own = fs::symlink_metadata(path).ok()?;
    let mut latest = own.modified().ok();
    if depth > 0 && own.is_dir() {
        for entry in fs::read_dir(path).ok()?.filter_map(|entry| entry.ok()) {
            latest = latest.max(newest(&entry.path(), depth - 1));
        }
    }
    latest
}
/// The locks Cargo holds on the build folders in `dir` while it builds, up to `depth` levels
/// below it: `target/debug/.cargo-lock`, or one in a target folder of a run's own.
fn locks(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return vec![];
    };
    let mut found = vec![];
    for entry in entries.filter_map(|entry| entry.ok()) {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_file() && entry.file_name() == ".cargo-lock" {
            found.push(entry.path());
        } else if kind.is_dir() && depth > 1 {
            found.extend(locks(&entry.path(), depth - 1));
        }
    }
    found
}
/// Cargo's locks on `target`, taken so no build starts in it, or none when Cargo is building
/// there or a lock can't be taken.
fn take(target: &Path) -> Option<Vec<fs::File>> {
    locks(target, 3)
        .into_iter()
        .map(|lock| {
            let file = fs::File::open(lock).ok()?;
            file.try_lock_exclusive().ok()?;
            Some(file)
        })
        .collect()
}
/// A span as people read it: 13 days, 20 hours.
pub fn span(idle: Duration) -> String {
    match idle.as_secs() / 3600 {
        hours @ 48.. => format!("{} days", hours / 24),
        1 => "1 hour".to_owned(),
        hours => format!("{hours} hours"),
    }
}
/// Deletes the builds of the worktrees in `ws` that went unused, but `keep`'s and those whose
/// preview runs, and answers each with how long it went unused and its size. Every build unused
/// for `IDLE` goes; then, while `free` answers less than `LOW_DISK`, those unused for `PRESSED`,
/// oldest first.
fn clear(
    ws: &Workspace,
    keep: &str,
    previews: &[String],
    free: &dyn Fn() -> Result<u64>,
    runner: &dyn Runner,
) -> Result<Vec<(String, Duration, u64)>> {
    let mut builds: Vec<(String, Duration)> = ws
        .worktrees()?
        .into_iter()
        .filter(|name| name != keep && !previews.contains(name))
        .filter(|name| ws.worktree(name).join("target").is_dir())
        .filter_map(|name| {
            let idle = unused(&ws.worktree(&name))?;
            Some((name, idle))
        })
        .collect();
    builds.sort_by(|a, b| b.1.cmp(&a.1));
    let mut gone = vec![];
    for (name, idle) in builds {
        if idle < IDLE && (idle < PRESSED || free()? >= crate::start::LOW_DISK) {
            continue;
        }
        let path = ws.worktree(&name);
        let target = path.join("target");
        // Held until the build is gone, so none starts in it meanwhile.
        let Some(_locks) = take(&target) else {
            continue;
        };
        let bytes = [&target, &path.join(".test-build")]
            .iter()
            .filter_map(|dir| workspace::size(runner, dir))
            .sum();
        fs::remove_dir_all(&target)?;
        crate::test::sweep(&path.join(".test-build"));
        gone.push((name, idle, bytes));
    }
    Ok(gone)
}
/// Before a command that builds: deletes the idle worktrees' builds, but `keep`'s, and says
/// what went. Nothing stops the command; a build that couldn't go stays.
pub fn evict(ws: &Workspace, keep: &str, runner: &dyn Runner) {
    let gone = crate::preview::running(runner).and_then(|previews| {
        clear(
            ws,
            keep,
            &previews,
            &|| workspace::free_bytes(&ws.root),
            runner,
        )
    });
    match gone {
        Ok(gone) if gone.is_empty() => {}
        Ok(gone) => println!(
            "Deleted idle builds, {}: {}.",
            workspace::human(gone.iter().map(|(_, _, bytes)| bytes).sum()),
            gone.iter()
                .map(|(name, idle, _)| format!("worktrees/{name}/target (unused {})", span(*idle)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Err(error) => println!("Kept the idle worktrees' builds: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const HOUR: Duration = Duration::from_secs(60 * 60);
    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    #[test]
    fn only_the_builds_of_worktrees_nobody_uses_go() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::create_dir_all(root.join("dev/.git")).unwrap();
        // A worktree with its source, its Git index and a build, all unused for `age`.
        let worktree = |name: &str, age: Duration| {
            let path = root.join("worktrees").join(name);
            let git = root.join("dev/.git/worktrees").join(name);
            fs::create_dir_all(path.join("target/debug/deps")).unwrap();
            fs::create_dir_all(&git).unwrap();
            fs::write(path.join(".git"), format!("gitdir: {}\n", git.display())).unwrap();
            fs::write(path.join("main.rs"), "").unwrap();
            fs::write(path.join("target/debug/.cargo-lock"), "").unwrap();
            fs::write(path.join("target/debug/deps/libx.rlib"), "x").unwrap();
            fs::write(git.join("index"), "").unwrap();
            let then = SystemTime::now() - age;
            for changed in [
                git.join("index"),
                path.join("target/debug/deps/libx.rlib"),
                path.join("target/debug/deps"),
                path.join("target/debug/.cargo-lock"),
                path.join("target/debug"),
                path.join("target"),
            ] {
                fs::File::open(changed).unwrap().set_modified(then).unwrap();
            }
        };
        worktree("idle", 4 * DAY);
        worktree("older", 2 * DAY);
        worktree("recent", DAY);
        worktree("fresh", HOUR);
        worktree("previewed", 10 * DAY);
        worktree("building", 10 * DAY);
        worktree("current", 10 * DAY);
        let cargo =
            fs::File::open(root.join("worktrees/building/target/debug/.cargo-lock")).unwrap();
        cargo.lock_exclusive().unwrap();
        let ws = Workspace { root: root.clone() };
        let previews = ["previewed".to_owned()];
        let names = |gone: Vec<(String, Duration, u64)>| -> Vec<String> {
            gone.into_iter().map(|(name, ..)| name).collect()
        };
        let built = |name: &str| root.join("worktrees").join(name).join("target").exists();

        // With disk to spare, only a build unused for days goes, and the worktree stays.
        let plenty = || Ok(u64::MAX);
        let gone = clear(&ws, "current", &previews, &plenty, &crate::Native).unwrap();
        assert_eq!(names(gone), ["idle"]);
        assert!(root.join("worktrees/idle/main.rs").exists());
        assert!(!built("idle"));

        // Short of disk, builds unused for hours go too, oldest first, until enough is free.
        let looks = Cell::new(0);
        let short = || {
            looks.set(looks.get() + 1);
            Ok(if looks.get() == 1 { 0 } else { u64::MAX })
        };
        let gone = clear(&ws, "current", &previews, &short, &crate::Native).unwrap();
        assert_eq!(names(gone), ["older"]);
        for kept in ["recent", "fresh", "previewed", "building", "current"] {
            assert!(built(kept), "{kept}");
        }

        // Once Cargo is done with it, the build it held goes too.
        drop(cargo);
        let gone = clear(&ws, "current", &previews, &plenty, &crate::Native).unwrap();
        assert_eq!(names(gone), ["building"]);
    }
}
