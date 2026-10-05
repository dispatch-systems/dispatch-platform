//! `dispatchdev finish <name>`: after a change merged, everything it made goes, then Dev is
//! waited for until it runs the merge. A change that didn't merge goes only with `--abandon`.
use crate::{
    REPOSITORY, Result, Runner, require,
    workspace::{self, Workspace},
};
use serde_json::Value;
use std::{fs, path::Path};

/// The change's PR, if it has one: whether it merged, and its merge commit.
fn pull(name: &str, runner: &dyn Runner) -> Result<Option<(u64, bool, String)>> {
    let list: Vec<Value> = serde_json::from_str(&workspace::text(
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
            "number,state,mergeCommit",
            "--limit",
            "1",
        ],
        None,
    )?)?;
    Ok(list.first().map(|pr| {
        (
            pr["number"].as_u64().unwrap_or(0),
            pr["state"] == "MERGED",
            pr["mergeCommit"]["oid"].as_str().unwrap_or("").to_owned(),
        )
    }))
}

pub fn run(ws: &Workspace, name: &str, abandon: bool, runner: &dyn Runner) -> Result<()> {
    workspace::valid_name(name)?;
    let path = ws.worktree(name);
    let dev = ws.dev();
    let git = |args: &[&str], cwd: &Path| {
        workspace::text(runner, &[&["git"][..], args].concat(), Some(cwd))
    };
    if path.exists() {
        require(
            git(&["status", "--porcelain"], &path)?.is_empty(),
            &format!("worktrees/{name} holds uncommitted changes; commit or drop them first."),
        )?;
        let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"], &path)?;
        require(
            branch == name,
            &format!(
                "worktrees/{name} is on {branch}, not {name}; it isn't this change's to remove."
            ),
        )?;
    }
    let pr = pull(name, runner)?;
    let merged = pr.as_ref().filter(|(_, merged, _)| *merged);
    require(
        merged.is_some() || abandon,
        &match &pr {
            Some((number, ..)) => format!(
                "#{number} hasn't merged. Ship it first, or drop the change with --abandon."
            ),
            None => format!("{name} has no PR. Drop the change with --abandon."),
        },
    )?;
    let mut done = vec![];
    if crate::preview::running(runner)?
        .iter()
        .any(|running| running == name)
    {
        let unit = crate::preview::unit(name);
        runner.command(&["systemctl", "--user", "stop", &unit], None, 120)?;
        done.push("its preview stopped".to_owned());
    }
    let config = std::path::PathBuf::from(std::env::var("HOME")?)
        .join(format!(".config/dispatch-preview/{name}.env"));
    if fs::remove_file(&config).is_ok() {
        done.push("its port released".to_owned());
    }
    if path.exists() {
        let build = workspace::size(runner, &path.join("target"));
        git(
            &["worktree", "remove", path.to_str().ok_or("Non-UTF8 path")?],
            &dev,
        )?;
        done.push(match build {
            Some(bytes) => format!(
                "worktrees/{name} removed with its {} build",
                workspace::human(bytes)
            ),
            None => format!("worktrees/{name} removed"),
        });
    }
    if git(
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ],
        &dev,
    )
    .is_ok()
    {
        git(&["branch", "-D", name], &dev)?;
        done.push("its branch deleted".to_owned());
    }
    if !git(&["ls-remote", "--heads", "origin", name], &dev)?.is_empty() {
        git(&["push", "-q", "origin", "--delete", name], &dev)?;
        done.push("its branch on GitHub deleted".to_owned());
    }
    let scratch = workspace::scratch(name);
    if scratch.exists() {
        fs::remove_dir_all(&scratch)?;
        done.push(format!("{} removed", scratch.display()));
    }
    git(&["fetch", "-q", "--prune", "origin"], &dev)?;
    println!(
        "Finished {name}: {}.",
        if done.is_empty() {
            "nothing was left".to_owned()
        } else {
            done.join(", ")
        }
    );
    if let Some((number, _, commit)) = merged {
        let host = dev.join(".runtime/management/dispatch-host");
        require(
            host.is_file(),
            "Dev's host manager is missing; check Dev by hand.",
        )?;
        let dev_root = dev.to_str().ok_or("Non-UTF8 path")?;
        // The host manager returns once Dev runs the commit, or one containing it.
        let live = runner.command(
            &[
                host.to_str().ok_or("Non-UTF8 path")?,
                "host",
                "dev",
                "--root",
                dev_root,
                "--wait",
                commit,
            ],
            None,
            16 * 60,
        )?;
        let status: Value = serde_json::from_slice(&live).unwrap_or(Value::Null);
        println!(
            "#{number} is live on Dev at {}.",
            status["commit"]
                .as_str()
                .unwrap_or(commit)
                .get(..8)
                .unwrap_or(commit)
        );
    }
    Ok(())
}
