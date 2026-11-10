use super::{Clone, Repository, valid_name};
use crate::{Result, Runner, dev, require, review, workspace};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pull {
    pub number: u64,
    pub state: String,
    pub head_ref_oid: String,
    pub head_ref_name: String,
    pub is_cross_repository: bool,
}

/// An exact cleanup snapshot. Planning never creates scratch state or deletes anything.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FinishPlan {
    pub name: String,
    pub root: PathBuf,
    pub worktree: PathBuf,
    pub scratch: PathBuf,
    pub fetch_url: String,
    pub push_url: String,
    pub abandon: bool,
    pub pull: Option<Pull>,
    pub local_head: Option<String>,
    pub remote_head: Option<String>,
    pub worktree_present: bool,
    pub ignored: Vec<String>,
    /// The worktree's running dev server, which `finish` stops first.
    pub dev_server: Option<i32>,
    pub scratch_entries: Vec<String>,
    pub blockers: Vec<String>,
    pub can_finish: bool,
}
impl FinishPlan {
    pub fn can_finish(&self) -> bool {
        self.blockers.is_empty()
    }

    pub fn print(&self) {
        println!(
            "{} {}.",
            if self.can_finish() {
                "Can finish"
            } else {
                "Cannot finish"
            },
            self.name
        );
        // What stops it first, before the long lists.
        for blocker in &self.blockers {
            println!("  {blocker}");
        }
        if let Some(pid) = self.dev_server {
            println!("Stop its dev server: {pid}");
        }
        if self.worktree_present {
            println!("Remove worktree: {}", self.worktree.display());
        }
        if let Some(head) = &self.local_head {
            println!("Remove local branch: {} at {head}", self.name);
        }
        if let Some(head) = &self.remote_head {
            println!("Remove origin branch: {} at {head}", self.name);
        }
        println!("Scratch: {}", self.scratch.display());
        for entry in &self.scratch_entries {
            println!("  {entry}");
        }
        for entry in &self.ignored {
            println!("Ignored worktree data: {entry}");
        }
    }
}

fn inventory(path: &Path) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let mut entries = vec![];
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        entries.push(format!(
            "{}{}",
            entry.file_name().to_string_lossy(),
            if entry.file_type()?.is_dir() { "/" } else { "" }
        ));
    }
    entries.sort();
    Ok(entries)
}

pub fn finish_plan(
    from: &Path,
    name: &str,
    abandon: bool,
    runner: &dyn Runner,
) -> Result<FinishPlan> {
    valid_name(name)?;
    let clone = Clone::of(from, runner)?;
    let fetch_url = clone.git(runner, &["remote", "get-url", "origin"])?;
    let push_url = clone.git(runner, &["remote", "get-url", "--push", "origin"])?;
    let repository = Repository::at(&fetch_url)
        .filter(|fetched| Repository::at(&push_url) == Some(*fetched))
        .ok_or(
            "Origin must name Dispatch's GitHub repository, the platform's or the agent docs', \
             to fetch from and to push to; inspect both URLs before cleanup.",
        )?;
    let pulls: Vec<Pull> = serde_json::from_str(&workspace::text(
        runner,
        &[
            "gh",
            "pr",
            "list",
            "--repo",
            repository.name(),
            "--head",
            name,
            "--state",
            "all",
            "--json",
            "number,state,headRefOid,headRefName,isCrossRepository",
        ],
        Some(&clone.root),
    )?)?;
    let pull = pulls
        .into_iter()
        .find(|pull| pull.head_ref_name == name && !pull.is_cross_repository);
    let branch = format!("refs/heads/{name}");
    let refs = clone.git(
        runner,
        &["for-each-ref", "--format=%(refname) %(objectname)", &branch],
    )?;
    let local_head = refs
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{branch} ")).map(str::to_owned));
    let remote = clone.git(runner, &["ls-remote", "--heads", &fetch_url, &branch])?;
    let remote_head = remote.lines().find_map(|line| {
        let (head, reference) = line.split_once('\t')?;
        (reference == branch).then(|| head.to_owned())
    });
    let worktree = clone.worktrees.join(name);
    let scratch = workspace::scratch(name);
    let mut plan = FinishPlan {
        name: name.to_owned(),
        root: clone.root.clone(),
        worktree,
        scratch,
        fetch_url,
        push_url,
        abandon,
        pull,
        local_head,
        remote_head,
        worktree_present: false,
        ignored: vec![],
        dev_server: None,
        scratch_entries: vec![],
        blockers: vec![],
        can_finish: false,
    };
    match (&plan.pull, abandon) {
        (Some(pull), false) if pull.state == "MERGED" => {
            for (place, head) in [("Local", &plan.local_head), ("Origin", &plan.remote_head)] {
                if head.as_ref().is_some_and(|head| head != &pull.head_ref_oid) {
                    plan.blockers.push(format!("{place} branch differs from merged PR #{}'s head: preserve its unshipped work.", pull.number));
                }
            }
        }
        (Some(pull), false) => plan.blockers.push(format!(
            "#{} is {}, not merged: finish it once it merges, or --abandon it.",
            pull.number,
            pull.state.to_lowercase()
        )),
        (None, false) => plan.blockers.push(format!(
            "{name} has no PR: --abandon drops a change that never had one."
        )),
        (Some(pull), true) if pull.state == "OPEN" => plan.blockers.push(format!(
            "#{} is open: close it first (gh pr close {}).",
            pull.number, pull.number
        )),
        _ => {}
    }
    let registered = clone.git(runner, &["worktree", "list", "--porcelain", "-z"])?;
    if let Ok(metadata) = fs::symlink_metadata(&plan.worktree) {
        if metadata.file_type().is_symlink() {
            plan.blockers
                .push("The worktree path is a symlink: inspect it before cleanup.".into());
        } else {
            let real = fs::canonicalize(&plan.worktree)?;
            let found = registered.split("\0\0").find(|record| {
                record.split('\0').any(|field| {
                    field
                        .strip_prefix("worktree ")
                        .is_some_and(|path| fs::canonicalize(path).is_ok_and(|path| path == real))
                })
            });
            match found {
                Some(record) if record.split('\0').any(|field| field == format!("branch {branch}")) => {
                    plan.worktree_present = true;
                    if fs::canonicalize(std::env::current_dir()?)?.starts_with(&real) {
                        plan.blockers.push("Run it from outside the worktree it removes.".into());
                    }
                    let status = workspace::text(runner, &["git", "--no-optional-locks", "status", "--porcelain", "-z"], Some(&real))?;
                    if !status.is_empty() { plan.blockers.push("The worktree holds uncommitted changes: commit or drop them first.".into()); }
                    let ignored = workspace::text(runner, &["git", "ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z"], Some(&real))?;
                    plan.ignored = ignored.split('\0').filter(|name| !name.is_empty()).map(str::to_owned).collect();
                    plan.dev_server = dev::status(&real, runner)
                        .ok()
                        .filter(|status| status.running)
                        .and_then(|status| status.process.map(|process| process.pid));
                }
                _ => plan.blockers.push("The directory is not this branch's registered worktree: inspect it before cleanup.".into()),
            }
        }
    }
    // A branch checked out elsewhere must not be deleted even if this expected path is absent.
    if !plan.worktree_present
        && registered
            .split('\0')
            .any(|field| field == format!("branch {branch}"))
    {
        plan.blockers
            .push("The branch is checked out in another worktree: preserve it.".into());
    }
    if fs::symlink_metadata(&plan.scratch).is_ok_and(|meta| meta.file_type().is_symlink()) {
        plan.blockers
            .push("The scratch path is a symlink: inspect it before cleanup.".into());
    } else {
        plan.scratch_entries = inventory(&plan.scratch)?;
    }
    plan.can_finish = plan.can_finish();
    Ok(plan)
}

#[cfg(test)]
mod tests;

pub fn finish(from: &Path, name: &str, abandon: bool, runner: &dyn Runner) -> Result<()> {
    let plan = finish_plan(from, name, abandon, runner)?;
    plan.print();
    require(
        plan.can_finish(),
        "Cleanup refused: resolve the blockers above, then plan it again.",
    )?;
    let current = finish_plan(from, name, abandon, runner)?;
    require(
        plan == current,
        "Cleanup changed during inspection: plan it again before removing anything.",
    )?;
    let clone = Clone::of(from, runner)?;
    let branch = format!("refs/heads/{name}");
    if let Some(head) = &plan.remote_head {
        clone.git(
            runner,
            &[
                "push",
                "-q",
                &format!("--force-with-lease={branch}:{head}"),
                &plan.push_url,
                &format!(":{branch}"),
            ],
        )?;
    }
    if plan.dev_server.is_some() {
        dev::stop(&plan.worktree, runner)?;
    }
    if plan.worktree_present {
        clone.git(
            runner,
            &[
                "worktree",
                "remove",
                plan.worktree.to_str().ok_or("Non-UTF8 worktree path")?,
            ],
        )?;
    }
    if let Some(head) = &plan.local_head {
        clone.git(runner, &["update-ref", "-d", &branch, head])?;
    }
    if plan.scratch.exists() {
        require(
            !fs::symlink_metadata(&plan.scratch)?
                .file_type()
                .is_symlink(),
            "Scratch changed into a symlink: preserve it.",
        )?;
        // A review pass never taken left its reviewers' checkout there, which git knows of.
        review::discard(&clone.root, &plan.scratch, runner)?;
        fs::remove_dir_all(&plan.scratch)?;
    }
    println!(
        "Finished {name}: its worktree, branches and scratch folder are gone{}.",
        if abandon { ", unmerged" } else { "" }
    );
    println!("{}", updated(&clone, runner));
    Ok(())
}

/// Brings the clone up to `origin/main`, and says so, or why it was left as it is. A merge on
/// GitHub doesn't move the clone, which is where every session reads the agent docs from, and
/// where a checkout without the CLI runs it from. Only a clone on `main` with nothing
/// uncommitted, a new file included, moves, and only forward; the cleanup is done by now, so
/// nothing here fails it.
fn updated(clone: &Clone, runner: &dyn Runner) -> String {
    let at = clone.root.display();
    let moved = || -> Result<Option<String>> {
        // Fetched first: the clone is looked at once the network is done with, just before it
        // moves, so what another session did to it meanwhile is seen.
        clone.git(runner, &["fetch", "-q", "origin", "main"])?;
        let on = clone.git(runner, &["branch", "--show-current"])?;
        if on != "main" {
            return Ok(Some(match on.is_empty() {
                true => "it is on no branch".to_owned(),
                false => format!("it is on {on}"),
            }));
        }
        // A new file counts, whatever the clone's configuration hides of them.
        let changed = clone.git(
            runner,
            &["status", "--porcelain", "--untracked-files=normal"],
        )?;
        if !changed.is_empty() {
            return Ok(Some("it holds uncommitted changes".to_owned()));
        }
        clone.git(runner, &["merge", "-q", "--ff-only", "origin/main"])?;
        Ok(None)
    };
    match moved() {
        Ok(None) => format!("Its clone, {at}, is at origin's main."),
        Ok(Some(why)) => format!("Left its clone, {at}, where it is: {why}."),
        Err(error) => format!("Couldn't bring its clone, {at}, up to origin's main: {error}"),
    }
}
