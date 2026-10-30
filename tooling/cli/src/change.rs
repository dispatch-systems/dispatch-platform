//! A change from start to finish, around git and gh: `start` makes its worktree beside the clone,
//! off `origin/main`, with its packages and its scratch folder, and `finish` removes them once
//! its PR merged, or with `--abandon` for one that never will. Both work from any checkout of
//! the repository.
use crate::{REPOSITORY, Result, Runner, require, workspace};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The clone a checkout shares Git's data with, and the folder beside it, `worktrees/`, its
/// changes' worktrees go in.
pub struct Clone {
    pub root: PathBuf,
    pub worktrees: PathBuf,
}
impl Clone {
    /// The clone of the checkout at `from`, the clone itself or one of its worktrees.
    pub fn of(from: &Path, runner: &dyn Runner) -> Result<Self> {
        let git = workspace::text(
            runner,
            &[
                "git",
                "rev-parse",
                "--path-format=absolute",
                "--git-common-dir",
            ],
            Some(from),
        )?;
        let git = PathBuf::from(git);
        require(
            git.file_name().is_some_and(|name| name == ".git"),
            "Run it in a clone of the repository, or one of its worktrees.",
        )?;
        let root = git
            .parent()
            .ok_or("A clone without a folder")?
            .to_path_buf();
        let worktrees = root
            .parent()
            .ok_or("A clone without a folder around it")?
            .join("worktrees");
        Ok(Self { root, worktrees })
    }
    fn git(&self, runner: &dyn Runner, args: &[&str]) -> Result<String> {
        let mut command = vec!["git"];
        command.extend_from_slice(args);
        workspace::text(runner, &command, Some(&self.root))
    }
}

/// A change's name, which is its worktree's folder, its branch and its scratch folder's:
/// lowercase letters, digits and hyphens, starting with a letter or a digit.
pub fn valid_name(name: &str) -> Result<()> {
    require(
        name.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && name.bytes().next().is_some_and(|b| b != b'-')
            && name != "main",
        "A change's name is lowercase letters, digits and hyphens, such as fix-week-paging.",
    )
}

/// `dispatchdev start <name>`: the change's worktree beside the clone, on a branch of its name
/// off `base`, with its packages installed and its scratch folder made.
pub fn start(from: &Path, name: &str, base: &str, runner: &dyn Runner) -> Result<()> {
    valid_name(name)?;
    let clone = Clone::of(from, runner)?;
    let path = clone.worktrees.join(name);
    require(
        !path.exists(),
        &format!("{} exists already.", path.display()),
    )?;
    clone.git(runner, &["fetch", "-q", "origin"])?;
    require(
        clone.git(runner, &["branch", "--list", name])?.is_empty(),
        &format!("A branch {name} exists already: finish it, or name this change differently."),
    )?;
    fs::create_dir_all(&clone.worktrees)?;
    let target = path.to_str().ok_or("A worktree path that isn't UTF-8")?;
    clone.git(runner, &["worktree", "add", "-q", target, "-b", name, base])?;
    runner.command(&["npm", "ci", "--no-audit", "--no-fund"], Some(&path), 900)?;
    let scratch = workspace::checkout_scratch(&path)?;
    let at = clone.git(runner, &["rev-parse", "--short", base])?;
    println!(
        "Started {name}: {} on branch {name}, from {base} at {at}, its packages installed; its \
         scratch folder is {}.",
        path.display(),
        scratch.display()
    );
    println!(
        "{}",
        workspace::next(&[
            format!("`cd {}`", workspace::quote(target)),
            "`npm run dev` serves it; `dispatchdev shot` and `api` look at it".to_owned(),
            "`dispatchdev pr --skeleton --title \"<title>\"` starts its PR's body".to_owned(),
        ])
    );
    Ok(())
}

/// The newest PR of the branch `name`: its number and state.
fn pull(clone: &Clone, name: &str, runner: &dyn Runner) -> Result<Option<(u64, String)>> {
    let newest = workspace::text(
        runner,
        &[
            "gh",
            "pr",
            "list",
            "--repo",
            REPOSITORY,
            "--head",
            name,
            "--state",
            "all",
            "--json",
            "number,state",
            "--jq",
            ".[0] | select(.) | \"\\(.number) \\(.state)\"",
        ],
        Some(&clone.root),
    )?;
    Ok(match newest.split_once(' ') {
        Some((number, state)) => Some((number.parse()?, state.to_owned())),
        None => None,
    })
}

/// `dispatchdev finish <name>`: once the change's PR merged, or with `abandon` for one that
/// never will, its worktree, its branches and its scratch folder go.
pub fn finish(from: &Path, name: &str, abandon: bool, runner: &dyn Runner) -> Result<()> {
    valid_name(name)?;
    let clone = Clone::of(from, runner)?;
    match (pull(&clone, name, runner)?, abandon) {
        (Some((_, state)), false) if state == "MERGED" => {}
        (Some((number, state)), false) => {
            return Err(format!(
                "#{number} is {}, not merged: finish it once it merges, or --abandon it.",
                state.to_lowercase()
            )
            .into());
        }
        (None, false) => {
            return Err(
                format!("{name} has no PR: --abandon drops a change that never had one.").into(),
            );
        }
        (Some((number, state)), true) if state == "OPEN" => {
            return Err(
                format!("#{number} is open: close it first (gh pr close {number}).").into(),
            );
        }
        (_, true) => {}
    }
    let path = clone.worktrees.join(name);
    if path.exists() {
        require(
            !std::env::current_dir()?.starts_with(&path),
            "Run it from outside the worktree it removes.",
        )?;
        require(
            workspace::text(runner, &["git", "status", "--porcelain"], Some(&path))?.is_empty(),
            &format!(
                "{} holds uncommitted changes: commit or drop them first.",
                path.display()
            ),
        )?;
        let target = path.to_str().ok_or("A worktree path that isn't UTF-8")?;
        clone.git(runner, &["worktree", "remove", target])?;
    }
    if !clone.git(runner, &["branch", "--list", name])?.is_empty() {
        clone.git(runner, &["branch", "-D", name])?;
    }
    if !clone
        .git(runner, &["ls-remote", "--heads", "origin", name])?
        .is_empty()
    {
        clone.git(runner, &["push", "-q", "origin", "--delete", name])?;
    }
    let scratch = workspace::scratch(name);
    if scratch.exists() {
        fs::remove_dir_all(&scratch)?;
    }
    println!(
        "Finished {name}: its worktree, branches and scratch folder are gone{}.",
        if abandon { ", unmerged" } else { "" }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Native;
    use std::process::Command;

    #[test]
    fn a_checkout_finds_its_clone_and_the_worktrees_beside_it() {
        let temp = tempfile::tempdir().unwrap();
        let platform = fs::canonicalize(temp.path()).unwrap();
        let repo = platform.join("repo");
        let git = |cwd: &Path, args: &[&str]| {
            let done = Command::new("git")
                .args(args)
                .current_dir(cwd)
                .output()
                .unwrap();
            assert!(
                done.status.success(),
                "{}",
                String::from_utf8_lossy(&done.stderr)
            );
        };
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(
            &repo,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "x",
            ],
        );
        let tree = platform.join("worktrees/fix-week");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                tree.to_str().unwrap(),
                "-b",
                "fix-week",
            ],
        );
        for from in [&repo, &tree] {
            let clone = Clone::of(from, &Native).unwrap();
            assert_eq!(clone.root, repo);
            assert_eq!(clone.worktrees, platform.join("worktrees"));
        }
    }
}
