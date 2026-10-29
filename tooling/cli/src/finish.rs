//! `dispatchdev finish <name>`: after a change merged, everything it made goes, then Dev is
//! waited for until it runs the merge. A change that didn't merge goes only with `--abandon`.
use crate::{Result, Runner, change::Change, dev, require, workspace::Workspace};

pub fn run(
    ws: &Workspace,
    name: &str,
    abandon: bool,
    runner: &dyn Runner,
    pause: &dyn Fn(u64),
) -> Result<()> {
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
            dev::live(ws, pr.merge.as_deref().unwrap_or(""), runner, pause)?
        );
    }
    Ok(())
}
