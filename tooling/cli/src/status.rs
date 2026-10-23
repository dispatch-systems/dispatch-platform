//! `dispatchdev status`: the workspace at a glance: each worktree with its branch, state,
//! build, preview and newest `dispatchdev ci` run, the open PRs, what Dev runs, and the free
//! disk space.
use crate::{
    REPOSITORY, Result, Runner,
    change::Change,
    workspace::{self, Workspace},
};
use serde_json::Value;

/// The recent runs of the checks outside the queue, newest first; none when GitHub doesn't
/// answer.
fn branch_runs(runner: &dyn Runner) -> Vec<Value> {
    let endpoint = format!(
        "repos/{REPOSITORY}/actions/workflows/checks.yml/runs?event=workflow_dispatch&per_page=100"
    );
    workspace::text(runner, &["gh", "api", &endpoint], None)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|runs| runs["workflow_runs"].as_array().cloned())
        .unwrap_or_default()
}

/// Where the newest of `branch`'s runs stands, and whether it checked `head`.
fn ci_note(runs: &[Value], branch: &str, head: &str) -> Option<String> {
    let run = runs
        .iter()
        .filter(|run| run["head_branch"] == branch)
        .max_by_key(|run| run["id"].as_u64().unwrap_or(0))?;
    let state = match (run["status"].as_str(), run["conclusion"].as_str()) {
        (Some("completed"), Some("success")) => "passed",
        (Some("completed"), Some("cancelled")) => "cancelled",
        (Some("completed"), _) => "failed",
        _ => "running",
    };
    Some(if run["head_sha"] == head {
        format!("ci {state}")
    } else {
        format!("ci {state} on an older commit")
    })
}

pub fn run(ws: &Workspace, runner: &dyn Runner) -> Result<()> {
    let previews = crate::preview::running(runner).unwrap_or_default();
    let runs = branch_runs(runner);
    println!("Worktrees:");
    let names = ws.worktrees()?;
    if names.is_empty() {
        println!("  none");
    }
    for name in &names {
        let change = Change::named(ws, name)?;
        let path = &change.path;
        let git = |args: &[&str]| change.git(runner, args);
        let branch = change.branch(runner).unwrap_or_else(|_| "?".into());
        let mut notes = vec![];
        if branch != *name {
            notes.push(format!("on {branch}"));
        }
        if change.dirty(runner).unwrap_or(false) {
            notes.push("uncommitted changes".into());
        }
        if let Ok(ahead) = git(&["rev-list", "--count", "origin/main..HEAD"])
            && ahead != "0"
        {
            let commits = if ahead == "1" { "commit" } else { "commits" };
            notes.push(format!("{ahead} {commits} ahead of main"));
        }
        if let Ok(head) = git(&["rev-parse", "HEAD"])
            && let Some(note) = ci_note(&runs, &branch, &head)
        {
            notes.push(note);
        }
        if let Some(bytes) = workspace::size(runner, &path.join("target")) {
            notes.push(format!("build {}", workspace::human(bytes)));
        }
        if let Some(bytes) = workspace::size(runner, &path.join(".test-build")) {
            notes.push(format!("test build {}", workspace::human(bytes)));
        }
        if previews.contains(name) {
            let port = crate::preview::port(name).ok().flatten().unwrap_or(0);
            notes.push(format!("preview running on {port}"));
        }
        println!("  {name}  {}", notes.join(" · "));
    }
    println!("Open PRs:");
    match workspace::text(
        runner,
        &[
            "gh",
            "pr",
            "list",
            "--repo",
            REPOSITORY,
            "--state",
            "open",
            "--json",
            "number,title,headRefName,isDraft",
        ],
        None,
    )
    .ok()
    .and_then(|text| serde_json::from_str::<Vec<Value>>(&text).ok())
    {
        None => println!("  GitHub didn't answer"),
        Some(pulls) if pulls.is_empty() => println!("  none"),
        Some(pulls) => {
            for pr in pulls {
                println!(
                    "  #{} {} ({}{})",
                    pr["number"],
                    pr["title"].as_str().unwrap_or(""),
                    pr["headRefName"].as_str().unwrap_or(""),
                    if pr["isDraft"] == true { ", draft" } else { "" }
                );
            }
        }
    }
    let dev = ws.dev();
    let read = |path: &str| -> Option<Value> {
        serde_json::from_str(&std::fs::read_to_string(dev.join(path)).ok()?).ok()
    };
    let commit = read(".build/tooling/build-info.json")
        .and_then(|info| Some(info["commit"].as_str()?.get(..8)?.to_owned()))
        .unwrap_or_else(|| "unknown".into());
    let update = read("data/platform/dev-update.json")
        .and_then(|status| Some(status["status"].as_str()?.to_owned()))
        .unwrap_or_else(|| "unknown".into());
    let main = workspace::text(
        runner,
        &["git", "rev-parse", "--short=8", "origin/main"],
        Some(&dev),
    )
    .unwrap_or_default();
    println!(
        "Dev: runs {commit} ({update}){}",
        if !main.is_empty() && main != commit {
            format!("; main is at {main}")
        } else {
            String::new()
        }
    );
    // Written by Dev's updater while its updates fail, and removed once one gets through.
    if let Some(failure) = read("data/platform/dev-update-failure.json") {
        println!(
            "  Dev's updates have failed since {} ({} attempts): {}",
            failure["since"].as_str().unwrap_or("unknown"),
            failure["attempts"],
            failure["error"].as_str().unwrap_or("no error recorded")
        );
    }
    println!(
        "Disk: {} free.",
        workspace::human(workspace::worktrees_free(&ws.root)?)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_worktree_shows_its_branch_newest_run_and_whether_it_checked_the_head() {
        let runs = [
            json!({"id": 1, "head_branch": "a", "head_sha": "old", "status": "completed", "conclusion": "failure"}),
            json!({"id": 2, "head_branch": "a", "head_sha": "new", "status": "completed", "conclusion": "success"}),
            json!({"id": 3, "head_branch": "b", "head_sha": "x", "status": "in_progress", "conclusion": null}),
        ];
        assert_eq!(ci_note(&runs, "a", "new").as_deref(), Some("ci passed"));
        assert_eq!(
            ci_note(&runs, "a", "newer").as_deref(),
            Some("ci passed on an older commit")
        );
        assert_eq!(ci_note(&runs, "b", "x").as_deref(), Some("ci running"));
        assert_eq!(ci_note(&runs, "c", "y"), None);
    }
}
