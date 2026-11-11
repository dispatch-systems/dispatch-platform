//! A change from start to finish, around git and gh: `start` makes its worktree beside the clone,
//! off `origin/main`, with its packages and its scratch folder, and `finish` removes them once
//! its PR merged, or with `--abandon` for one that never will, and brings the clone up to
//! `main`. Both work from any checkout of the repository: the platform's, or the agent docs'.
use crate::{REPOSITORY, Result, Runner, require, workspace};
use std::{
    fs,
    path::{Path, PathBuf},
};
mod finish;
pub use finish::{FinishPlan, finish, finish_plan};

/// The agent docs' name on GitHub: `AGENTS.md` and the skills.
const AGENT_DOCS: &str = "dispatch-systems/dispatch-agents";

/// The repository on GitHub a checkout is of, which its origin names: the platform, or the
/// agent docs, which hold no source, so nothing checks them and Dev never serves them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repository {
    Platform,
    AgentDocs,
}
impl Repository {
    /// The repository of the checkout at `root`.
    pub fn of(root: &Path, runner: &dyn Runner) -> Result<Self> {
        let url = workspace::text(runner, &["git", "remote", "get-url", "origin"], Some(root))?;
        Self::at(&url).ok_or_else(|| {
            "Origin must name one of Dispatch's GitHub repositories: the platform or the agent \
             docs."
                .into()
        })
    }
    /// The repository a remote's URL names on GitHub, over any transport it serves.
    pub fn at(url: &str) -> Option<Self> {
        let name = if let Some(name) = url.strip_prefix("https://github.com/") {
            name
        } else {
            // `ssh://<user>@<host>/<name>`, or `<user>@<host>:<name>` as scp writes it.
            let (user, rest) = url.strip_prefix("ssh://").unwrap_or(url).split_once('@')?;
            let (host, name) = rest.split_once(match url.starts_with("ssh://") {
                true => '/',
                false => ':',
            })?;
            (user == "git" && host == "github.com").then_some(name)?
        };
        let name = name.strip_suffix(".git").unwrap_or(name);
        if dispatch_shared::REPOSITORIES.contains(&name) {
            Some(Self::Platform)
        } else {
            (name == AGENT_DOCS).then_some(Self::AgentDocs)
        }
    }
    /// Its name on GitHub, where its PRs are.
    pub fn name(self) -> &'static str {
        match self {
            Self::Platform => REPOSITORY,
            Self::AgentDocs => AGENT_DOCS,
        }
    }
}

/// The clone a checkout shares Git's data with, and the folder its changes' worktrees go in:
/// `worktrees/` beside a clone named `repo`, which has a folder around it for them, and
/// `<clone>-worktrees/` beside one with a name of its own, such as `dispatch-agents`.
pub struct Clone {
    pub root: PathBuf,
    pub worktrees: PathBuf,
}
impl Clone {
    /// The clone of the checkout at `from`, the clone itself or one of its worktrees.
    pub fn of(from: &Path, runner: &dyn Runner) -> Result<Self> {
        let git = workspace::shared_git(from, runner)?;
        require(
            git.file_name().is_some_and(|name| name == ".git"),
            "Run it in a clone of the repository, or one of its worktrees.",
        )?;
        let root = git
            .parent()
            .ok_or("A clone without a folder")?
            .to_path_buf();
        let around = root.parent().ok_or("A clone without a folder around it")?;
        let worktrees = match workspace::checkout_name(&root)? {
            "repo" => around.join("worktrees"),
            name => around.join(format!("{name}-worktrees")),
        };
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
/// off `base`, with its packages installed, where it has any, and its scratch folder made.
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
    // The agent docs have no packages, and nothing to serve.
    let packages = path.join("package-lock.json").is_file();
    if packages {
        runner.command(&["npm", "ci", "--no-audit", "--no-fund"], Some(&path), 900)?;
    }
    let scratch = workspace::checkout_scratch(&path)?;
    let at = clone.git(runner, &["rev-parse", "--short", base])?;
    println!(
        "Started {name}: {} on branch {name}, from {base} at {at}{}; its scratch folder is {}.",
        path.display(),
        if packages {
            ", its packages installed"
        } else {
            ""
        },
        scratch.display()
    );
    let mut next = vec![format!("`cd {}`", workspace::quote(target))];
    if packages {
        next.push("`npm run dev` serves it; `dispatchdev shot` and `api` look at it".to_owned());
    }
    next.push("`dispatchdev pr --skeleton --title \"<title>\"` starts its PR's body".to_owned());
    println!("{}", workspace::next(&next));
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

    /// A runner that installs nothing: a checkout without packages asks for none.
    struct NoPackages;
    impl Runner for NoPackages {
        fn command(&self, args: &[&str], cwd: Option<&Path>, timeout: u64) -> Result<Vec<u8>> {
            assert_ne!(args.first(), Some(&"npm"), "{args:?}");
            Native.command(args, cwd, timeout)
        }
    }

    #[test]
    fn a_clone_with_a_name_of_its_own_keeps_its_worktrees_beside_it() {
        let temp = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(temp.path()).unwrap();
        let docs = home.join("dispatch-agents");
        let git = |cwd: &Path, args: &[&str]| {
            let done = Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
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
        let origin = home.join("origin.git");
        git(
            &home,
            &[
                "init",
                "-q",
                "--bare",
                "-b",
                "main",
                origin.to_str().unwrap(),
            ],
        );
        git(
            &home,
            &[
                "clone",
                "-q",
                origin.to_str().unwrap(),
                docs.to_str().unwrap(),
            ],
        );
        git(&docs, &["commit", "-q", "--allow-empty", "-m", "x"]);
        git(&docs, &["push", "-q", "-u", "origin", "HEAD:main"]);
        // The folder's name stands in for a change's: its scratch folder is made by it.
        let name = format!(
            "start-test-{}-{}",
            std::process::id(),
            home.file_name()
                .unwrap()
                .to_string_lossy()
                .replace('.', "")
                .to_lowercase()
        );
        start(&docs, &name, "origin/main", &NoPackages).unwrap();
        let scratch = workspace::scratch(&name);
        let made = scratch.is_dir();
        let _ = fs::remove_dir_all(&scratch);
        assert!(made, "its scratch folder");
        let tree = home.join("dispatch-agents-worktrees").join(&name);
        let clone = Clone::of(&tree, &Native).unwrap();
        assert_eq!(clone.root, docs);
        assert_eq!(clone.worktrees, home.join("dispatch-agents-worktrees"));
    }
}
