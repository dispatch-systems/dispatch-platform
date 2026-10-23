//! `dispatchdev finish <name>`: after a change merged, everything it made goes, then Dev is
//! waited for until it runs the merge. A change that didn't merge goes only with `--abandon`.
use crate::{Result, Runner, change::Change, require, workspace::Workspace};
use serde_json::Value;

pub fn run(ws: &Workspace, name: &str, abandon: bool, runner: &dyn Runner) -> Result<()> {
    let change = Change::named(ws, name)?;
    if change.path.exists() {
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
    let done = change.clean(ws, runner)?;
    println!(
        "Finished {name}: {}.",
        if done.is_empty() {
            "nothing was left".to_owned()
        } else {
            done.join(", ")
        }
    );
    if let Some(pr) = merged {
        println!(
            "#{} is live on Dev at {}.",
            pr.number,
            live(ws, pr.merge.as_deref().unwrap_or(""), runner)?
        );
    }
    Ok(())
}

/// Waits until Dev runs `commit`, or one containing it, and returns the commit it runs, short.
pub fn live(ws: &Workspace, commit: &str, runner: &dyn Runner) -> Result<String> {
    let dev = ws.dev();
    let host = dev.join(".runtime/management/dispatch-host");
    require(
        host.is_file(),
        "Dev's host manager is missing; check Dev by hand.",
    )?;
    let live = runner.command(
        &[
            host.to_str().ok_or("Non-UTF8 path")?,
            "host",
            "dev",
            "--root",
            dev.to_str().ok_or("Non-UTF8 path")?,
            "--wait",
            commit,
        ],
        None,
        16 * 60,
    )?;
    let status: Value = serde_json::from_slice(&live).unwrap_or(Value::Null);
    let runs = status["commit"].as_str().unwrap_or(commit);
    Ok(runs.get(..8).unwrap_or(runs).to_owned())
}
