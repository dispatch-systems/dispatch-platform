//! A change, as `dispatchdev` finds it by its name: the name is its worktree's folder under
//! `worktrees/`, its branch and its scratch folder's, and its PR is the newest one GitHub has
//! from that branch.
use crate::{
    REPOSITORY, Result, Runner, require,
    workspace::{self, Workspace},
};
use serde_json::Value;
use std::{fs, path::PathBuf};

pub struct Change {
    pub name: String,
    /// Its worktree, which may not exist.
    pub path: PathBuf,
}

/// A change's PR, as GitHub has it.
#[derive(Debug, PartialEq)]
pub struct Pull {
    pub number: u64,
    pub url: String,
    /// `OPEN`, `CLOSED` or `MERGED`.
    pub state: String,
    /// The commit its branch was at, last.
    pub head: String,
    /// Its squash commit on `main`, once it merged.
    pub merge: Option<String>,
}
impl Pull {
    pub fn open(&self) -> bool {
        self.state == "OPEN"
    }
    pub fn merged(&self) -> bool {
        self.state == "MERGED"
    }
}

impl Change {
    /// The change `name` names, with or without a worktree.
    pub fn named(ws: &Workspace, name: &str) -> Result<Self> {
        workspace::valid_name(name)?;
        Ok(Self {
            name: name.to_owned(),
            path: ws.worktree(name),
        })
    }
    /// The change `name` names, whose worktree must exist.
    pub fn existing(ws: &Workspace, name: &str) -> Result<Self> {
        let change = Self::named(ws, name)?;
        require(
            change.path.is_dir(),
            &format!("worktrees/{name} doesn't exist. Start it with dispatchdev start {name}."),
        )?;
        Ok(change)
    }
    /// Where it keeps everything it makes outside its worktree.
    pub fn scratch(&self) -> PathBuf {
        workspace::scratch(&self.name)
    }
    /// Its scratch folder, made private if it is new.
    pub fn scratch_dir(&self) -> Result<PathBuf> {
        workspace::scratch_dir(&self.name)
    }
    /// `git` with `args` in its worktree, its output trimmed.
    pub fn git(&self, runner: &dyn Runner, args: &[&str]) -> Result<String> {
        workspace::text(runner, &[&["git"][..], args].concat(), Some(&self.path))
    }
    /// Whether its worktree holds changes not yet committed.
    pub fn dirty(&self, runner: &dyn Runner) -> Result<bool> {
        Ok(!self.git(runner, &["status", "--porcelain"])?.is_empty())
    }
    /// The branch its worktree is on, `HEAD` when on none.
    pub fn branch(&self, runner: &dyn Runner) -> Result<String> {
        self.git(runner, &["rev-parse", "--abbrev-ref", "HEAD"])
    }
    /// Its newest PR, open, closed or merged, if it has one.
    pub fn pull(&self, runner: &dyn Runner) -> Result<Option<Pull>> {
        let list: Vec<Value> = serde_json::from_str(&workspace::text(
            runner,
            &[
                "gh",
                "pr",
                "list",
                "--repo",
                REPOSITORY,
                "--head",
                &self.name,
                "--state",
                "all",
                "--json",
                "number,url,state,headRefOid,mergeCommit",
                "--limit",
                "1",
            ],
            None,
        )?)?;
        Ok(list.first().map(|pr| Pull {
            number: pr["number"].as_u64().unwrap_or(0),
            url: pr["url"].as_str().unwrap_or("").to_owned(),
            state: pr["state"].as_str().unwrap_or("").to_owned(),
            head: pr["headRefOid"].as_str().unwrap_or("").to_owned(),
            merge: pr["mergeCommit"]["oid"].as_str().map(str::to_owned),
        }))
    }
    /// Everything the change made goes: its worktree with its build, its branch here and on
    /// GitHub, and its scratch folder. Returns what went, in that order.
    pub fn clean(&self, ws: &Workspace, runner: &dyn Runner) -> Result<Vec<String>> {
        let name = &self.name;
        let dev = ws.dev();
        let git =
            |args: &[&str]| workspace::text(runner, &[&["git"][..], args].concat(), Some(&dev));
        let mut done = vec![];
        if self.path.exists() {
            let build = workspace::size(runner, &self.path.join("target"));
            git(&[
                "worktree",
                "remove",
                self.path.to_str().ok_or("Non-UTF8 path")?,
            ])?;
            done.push(match build {
                Some(bytes) => format!(
                    "worktrees/{name} removed with its {} build",
                    workspace::human(bytes)
                ),
                None => format!("worktrees/{name} removed"),
            });
        }
        git(&["worktree", "prune"])?;
        if git(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ])
        .is_ok()
        {
            git(&["branch", "-D", name])?;
            done.push("its branch deleted".to_owned());
        }
        // GitHub deletes a merged PR's branch itself; this catches the rest. One GitHub
        // deleted meanwhile refuses the push, and is gone all the same.
        let remote = || git(&["ls-remote", "--heads", "origin", name]);
        if !remote()?.is_empty() {
            match git(&["push", "-q", "origin", "--delete", name]) {
                Ok(_) => done.push("its branch on GitHub deleted".to_owned()),
                Err(_) if remote()?.is_empty() => {}
                Err(error) => return Err(error),
            }
        }
        let scratch = self.scratch();
        if scratch.exists() {
            fs::remove_dir_all(&scratch)?;
            done.push(format!("{} removed", scratch.display()));
        }
        git(&["fetch", "-q", "--prune", "origin"])?;
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    struct GitHub(&'static str);
    impl Runner for GitHub {
        fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
            assert_eq!(&args[..3], ["gh", "pr", "list"]);
            assert!(args.windows(2).any(|pair| pair == ["--head", "fix-week"]));
            Ok(self.0.as_bytes().to_vec())
        }
    }

    #[test]
    fn a_change_is_found_by_its_name_and_its_pr_by_its_branch() {
        let ws = Workspace {
            root: PathBuf::from("/workspace"),
        };
        let change = Change::named(&ws, "fix-week").unwrap();
        assert_eq!(change.path, Path::new("/workspace/worktrees/fix-week"));
        assert_eq!(change.scratch(), Path::new("/tmp/dispatch-fix-week"));
        assert!(Change::named(&ws, "Fix week").is_err());
        assert!(
            Change::existing(&ws, "fix-week")
                .err()
                .unwrap()
                .to_string()
                .starts_with("worktrees/fix-week doesn't exist.")
        );

        let merged = GitHub(
            r#"[{"number":7,"url":"https://github.com/pull/7","state":"MERGED","headRefOid":"1111111","mergeCommit":{"oid":"3333333"}}]"#,
        );
        let pull = change.pull(&merged).unwrap().unwrap();
        assert!(pull.merged() && !pull.open());
        assert_eq!(pull.merge.as_deref(), Some("3333333"));
        assert_eq!(pull.head, "1111111");
        let open = GitHub(
            r#"[{"number":8,"url":"https://github.com/pull/8","state":"OPEN","headRefOid":"2222222","mergeCommit":null}]"#,
        );
        let pull = change.pull(&open).unwrap().unwrap();
        assert!(pull.open() && pull.merge.is_none());
        assert_eq!(pull.url, "https://github.com/pull/8");
        assert_eq!(change.pull(&GitHub("[]")).unwrap(), None);
    }

    #[test]
    fn a_branch_github_deletes_while_it_is_cleaned_up_is_gone_all_the_same() {
        /// GitHub listing the branch until asked to delete it, then refusing, having deleted
        /// it itself, or with `kept` still holding it.
        struct Raced {
            asked: std::cell::Cell<bool>,
            kept: bool,
        }
        impl Runner for Raced {
            fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
                match args {
                    ["git", "push", ..] => {
                        self.asked.set(true);
                        Err("git push -q failed: cannot lock ref 'refs/heads/raced'".into())
                    }
                    ["git", "ls-remote", ..] if self.kept || !self.asked.get() => {
                        Ok(b"1111111\trefs/heads/raced".to_vec())
                    }
                    ["git", "rev-parse", ..] => Err("no such branch here".into()),
                    _ => Ok(vec![]),
                }
            }
        }
        let ws = Workspace {
            root: PathBuf::from("/workspace"),
        };
        let change = Change::named(&ws, "clean-raced").unwrap();
        let raced = |kept| Raced {
            asked: std::cell::Cell::new(false),
            kept,
        };
        assert_eq!(
            change.clean(&ws, &raced(false)).unwrap(),
            Vec::<String>::new()
        );
        assert!(change.clean(&ws, &raced(true)).is_err());
    }
}
