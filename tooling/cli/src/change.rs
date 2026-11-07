//! A change from start to finish, around git and gh: `start` makes its worktree beside the clone,
//! off `origin/main`, with its packages and its scratch folder, and `finish` removes them once
//! its PR merged, or with `--abandon` for one that never will. Both work from any checkout of
//! the repository.
use crate::{Result, Runner, require, workspace};
use std::{
    fs,
    path::{Path, PathBuf},
};
mod finish;
pub use finish::{FinishPlan, finish, finish_plan};

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
