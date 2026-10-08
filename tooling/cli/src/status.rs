//! `dispatchdev status`: the workspace at a glance: each worktree with its branch, state,
//! build and preview, the open PRs, what Dev runs, and the free disk space.
use crate::{
    REPOSITORY, Result, Runner,
    workspace::{self, Workspace},
};
use serde_json::Value;

/// A build unused this long says so, as it nears `idle::IDLE`.
const DAY: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

pub fn run(ws: &Workspace, runner: &dyn Runner) -> Result<()> {
    let previews = crate::preview::running(runner).unwrap_or_default();
    println!("Worktrees:");
    let names = ws.worktrees()?;
    if names.is_empty() {
        println!("  none");
    }
    for name in &names {
        let path = ws.worktree(name);
        let git =
            |args: &[&str]| workspace::text(runner, &[&["git"][..], args].concat(), Some(&path));
        let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|_| "?".into());
        let mut notes = vec![];
        if branch != *name {
            notes.push(format!("on {branch}"));
        }
        if git(&["status", "--porcelain"]).is_ok_and(|changes| !changes.is_empty()) {
            notes.push("uncommitted changes".into());
        }
        if let Ok(ahead) = git(&["rev-list", "--count", "origin/main..HEAD"])
            && ahead != "0"
        {
            let commits = if ahead == "1" { "commit" } else { "commits" };
            notes.push(format!("{ahead} {commits} ahead of main"));
        }
        if let Some(bytes) = workspace::size(runner, &path.join("target")) {
            let mut build = format!("build {}", workspace::human(bytes));
            if let Some(idle) = crate::idle::unused(&path).filter(|idle| *idle >= DAY) {
                build.push_str(&format!(", unused {}", crate::idle::span(idle)));
            }
            notes.push(build);
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
    println!(
        "Disk: {} free.",
        workspace::human(workspace::free_bytes(&ws.root)?)
    );
    Ok(())
}
