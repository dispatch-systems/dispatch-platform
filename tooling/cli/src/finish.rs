//! `dispatchdev finish <name>`: after a change merged, everything it made goes, then Dev is
//! waited for until it runs the merge. A change that didn't merge goes only with `--abandon`.
use crate::{
    Result, Runner,
    change::Change,
    require,
    workspace::{self, Workspace},
};
use serde_json::Value;
use std::fs;

pub fn run(ws: &Workspace, name: &str, abandon: bool, runner: &dyn Runner) -> Result<()> {
    let change = Change::named(ws, name)?;
    let path = &change.path;
    let dev = ws.dev();
    let git = |args: &[&str]| workspace::text(runner, &[&["git"][..], args].concat(), Some(&dev));
    if path.exists() {
        require(
            !change.dirty(runner)?,
            &format!("worktrees/{name} holds uncommitted changes; commit or drop them first."),
        )?;
        let branch = change.branch(runner)?;
        require(
            branch == name,
            &format!(
                "worktrees/{name} is on {branch}, not {name}; it isn't this change's to remove."
            ),
        )?;
    }
    let pr = change.pull(runner)?;
    let merged = pr.as_ref().filter(|pr| pr.merged());
    require(
        merged.is_some() || abandon,
        &match &pr {
            Some(pr) => format!(
                "#{} hasn't merged. Ship it first, or drop the change with --abandon.",
                pr.number
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
        git(&["worktree", "remove", path.to_str().ok_or("Non-UTF8 path")?])?;
        done.push(match build {
            Some(bytes) => format!(
                "worktrees/{name} removed with its {} build",
                workspace::human(bytes)
            ),
            None => format!("worktrees/{name} removed"),
        });
    }
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
    if !git(&["ls-remote", "--heads", "origin", name])?.is_empty() {
        git(&["push", "-q", "origin", "--delete", name])?;
        done.push("its branch on GitHub deleted".to_owned());
    }
    let scratch = change.scratch();
    if scratch.exists() {
        fs::remove_dir_all(&scratch)?;
        done.push(format!("{} removed", scratch.display()));
    }
    git(&["fetch", "-q", "--prune", "origin"])?;
    println!(
        "Finished {name}: {}.",
        if done.is_empty() {
            "nothing was left".to_owned()
        } else {
            done.join(", ")
        }
    );
    if let Some(pr) = merged {
        let commit = pr.merge.as_deref().unwrap_or("");
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
            "#{} is live on Dev at {}.",
            pr.number,
            status["commit"]
                .as_str()
                .unwrap_or(commit)
                .get(..8)
                .unwrap_or(commit)
        );
    }
    Ok(())
}
