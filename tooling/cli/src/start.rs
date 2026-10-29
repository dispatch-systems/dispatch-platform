//! `dispatchdev start <name>`: a worktree for one change, on a branch of the same name from
//! `origin/main` (or `--from`), with its packages installed and its scratch folder made, and with
//! `--preview` its preview building, on dispatch-dev alone. First, the changes that merged go, as
//! `finish` would remove them.
use crate::{
    REPOSITORY, Result, Runner,
    change::Change,
    require,
    workspace::{self, Workspace},
};
use serde_json::Value;

/// Below this, a new worktree's build could fill the disk: a preview's takes about 3 GB, and
/// 7 GB once tested.
pub const LOW_DISK: u64 = 20_000_000_000;

pub fn run(
    ws: &Workspace,
    name: &str,
    from: &str,
    preview: bool,
    runner: &dyn Runner,
) -> Result<()> {
    let change = Change::named(ws, name)?;
    if preview {
        // Previews run on dispatch-dev alone, and there, a missing address stops it before
        // anything is made.
        crate::preview::here(ws.machine)?;
        crate::preview::host()?;
    }
    let path = &change.path;
    require(!path.exists(), &format!("worktrees/{name} exists already."))?;
    sweep(ws, runner);
    let dev = ws.dev();
    let git = |args: &[&str]| workspace::text(runner, &[&["git"][..], args].concat(), Some(&dev));
    require(
        git(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ])
        .is_err(),
        &format!("A branch {name} exists already."),
    )?;
    git(&["fetch", "-q", "origin"])?;
    require(
        git(&["ls-remote", "--heads", "origin", name])?.is_empty(),
        &format!("origin has a branch {name} already."),
    )?;
    let base = git(&["rev-parse", "--short", from])?;
    git(&[
        "worktree",
        "add",
        "-q",
        path.to_str().ok_or("Non-UTF8 path")?,
        "-b",
        name,
        from,
    ])?;
    let scratch = change.scratch_dir()?;
    let log = scratch.join("npm-ci.log");
    let (installed, took) = workspace::logged(
        "npm ci --silent",
        path,
        &log,
        &[("TMPDIR", scratch.as_os_str())],
    )?;
    require(installed, &format!("npm ci failed; see {}", log.display()))?;
    println!(
        "Started {name}: worktrees/{name} on branch {name} from {from} at {base}; packages installed in {}.",
        workspace::duration(took)
    );
    println!("Scratch: {}", scratch.display());
    if preview {
        let port = crate::preview::begin(ws, name, false, runner)?;
        println!(
            "Its preview is building on port {port}; dispatchdev preview {name} --page <page> \
             prints its link once it's up."
        );
    }
    disk(ws, runner)?;
    // A change's name with hyphens is a feature's with underscores.
    let feature = name.replace('-', "_");
    println!(
        "{}",
        workspace::next(&[
            format!("`cd worktrees/{name}`"),
            format!(
                "`dispatchdev new feature {feature} --dry-run` shows what a new feature would write; \
                 `dispatchdev doctor` whether the checkout is ready."
            ),
        ])
    );
    Ok(())
}
/// The free space on the worktrees' disk, and when it's low, what each worktree's build takes.
pub fn disk(ws: &Workspace, runner: &dyn Runner) -> Result<()> {
    let free = workspace::worktrees_free(&ws.root)?;
    if free >= LOW_DISK {
        println!("Disk: {} free.", workspace::human(free));
        return Ok(());
    }
    println!(
        "Disk: only {} free. Builds can take 3–7 GB each; a merged change's go with dispatchdev finish:",
        workspace::human(free)
    );
    for name in ws.worktrees()? {
        if let Some(bytes) = workspace::size(runner, &ws.worktree(&name).join("target")) {
            println!("  worktrees/{name}/target  {}", workspace::human(bytes));
        }
    }
    Ok(())
}

/// Removes each change whose PR merged with exactly what its worktree holds, committed, so a
/// change that merged without `finish` keeps no build or preview. The support worker finishes
/// its own `support-*` changes. Any trouble is said and skipped: the new change starts anyway.
fn sweep(ws: &Workspace, runner: &dyn Runner) {
    let merged = workspace::text(
        runner,
        &[
            "gh",
            "pr",
            "list",
            "--repo",
            REPOSITORY,
            "--state",
            "merged",
            "--limit",
            "100",
            "--json",
            "number,headRefName,headRefOid",
        ],
        None,
    )
    .and_then(|text| Ok(serde_json::from_str::<Vec<Value>>(&text)?));
    let merged = match merged {
        Ok(merged) => merged,
        Err(error) => {
            println!("Couldn't look for merged changes to remove: {error}");
            return;
        }
    };
    let heads: Vec<(String, String)> = ws
        .worktrees()
        .unwrap_or_default()
        .into_iter()
        .filter(|name| !name.starts_with("support-"))
        .filter_map(|name| {
            let change = Change::named(ws, &name).ok()?;
            let settled = !change.dirty(runner).ok()? && change.branch(runner).ok()? == name;
            let head = change.git(runner, &["rev-parse", "HEAD"]).ok()?;
            settled.then_some((name, head))
        })
        .collect();
    let mut removed = vec![];
    for (name, number) in landed(&heads, &merged) {
        match Change::named(ws, &name).and_then(|change| change.clean(ws, runner)) {
            Ok(_) => removed.push(format!("{name} (#{number})")),
            Err(error) => println!("Couldn't remove {name}, merged as #{number}: {error}"),
        }
    }
    if !removed.is_empty() {
        println!("Removed merged changes: {}.", removed.join(", "));
    }
}

/// The worktrees, given by name and head, whose PR among `merged` merged that very head, each
/// with its PR's number.
fn landed(heads: &[(String, String)], merged: &[Value]) -> Vec<(String, u64)> {
    heads
        .iter()
        .filter_map(|(name, head)| {
            let pr = merged.iter().find(|pr| {
                pr["headRefName"] == name.as_str() && pr["headRefOid"] == head.as_str()
            })?;
            Some((name.clone(), pr["number"].as_u64()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_a_worktree_holding_exactly_its_merged_head_is_removed() {
        let merged = [
            json!({"number": 7, "headRefName": "fix-week", "headRefOid": "aaa"}),
            json!({"number": 8, "headRefName": "add-total", "headRefOid": "bbb"}),
        ];
        let heads = [
            ("fix-week".to_owned(), "aaa".to_owned()),
            // A commit after the merge, or an old PR of a name used again, keeps it.
            ("add-total".to_owned(), "ccc".to_owned()),
            ("audit".to_owned(), "ddd".to_owned()),
        ];
        assert_eq!(landed(&heads, &merged), [("fix-week".to_owned(), 7)]);
    }
}
