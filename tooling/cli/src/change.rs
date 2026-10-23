//! A change, as `dispatchdev` finds it by its name: the name is its worktree's folder under
//! `worktrees/`, its branch, its preview's unit and its scratch folder's, and its PR is the
//! newest one GitHub has from that branch.
use crate::{
    REPOSITORY, Result, Runner, preview, require,
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
    /// Everything the change made goes: its preview and port, its worktree with its build, its
    /// branch here and on GitHub, and its scratch folder. Returns what went, in that order.
    pub fn clean(&self, ws: &Workspace, runner: &dyn Runner) -> Result<Vec<String>> {
        let name = &self.name;
        let dev = ws.dev();
        let git =
            |args: &[&str]| workspace::text(runner, &[&["git"][..], args].concat(), Some(&dev));
        let mut done = vec![];
        let unit = preview::unit(name);
        if preview::running(runner)?
            .iter()
            .any(|running| running == name)
        {
            runner.command(&["systemctl", "--user", "stop", &unit], None, 120)?;
            done.push("its preview stopped".to_owned());
        }
        // A preview that failed stays listed as failed until told otherwise, which a unit that
        // never failed refuses.
        let _ = runner.command(&["systemctl", "--user", "reset-failed", &unit], None, 120);
        if fs::remove_file(preview::env_file(name)?).is_ok() {
            done.push("its port released".to_owned());
        }
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
        // GitHub deletes a merged PR's branch itself; this catches the rest.
        if !git(&["ls-remote", "--heads", "origin", name])?.is_empty() {
            git(&["push", "-q", "origin", "--delete", name])?;
            done.push("its branch on GitHub deleted".to_owned());
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
}
