//! `dispatchdev status`: the workspace at a glance: each worktree with its branch, state,
//! build, preview, newest `dispatchdev ci` run, PR and review, the open PRs, what Dev runs, and
//! the free disk space; `--json` writes the same as JSON.
use crate::{
    REPOSITORY, Result, Runner,
    change::Change,
    review,
    workspace::{self, Workspace},
};
use serde::Serialize;
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

/// The workspace as `status` shows it, and as `--json` writes it.
#[derive(Serialize)]
pub struct Status {
    worktrees: Vec<Tree>,
    /// The open PRs; none when GitHub didn't answer.
    pulls: Option<Vec<Open>>,
    dev: Dev,
    /// Bytes free on the worktrees' disk.
    free: u64,
}
#[derive(Serialize)]
struct Tree {
    name: String,
    branch: String,
    uncommitted: bool,
    /// Commits ahead of `origin/main`.
    ahead: u64,
    /// Where its newest `dispatchdev ci` run stands, as `ci_note` says it.
    ci: Option<String>,
    /// The bytes its build and its test builds take.
    build: Option<u64>,
    test_build: Option<u64>,
    /// The port its running preview serves on.
    preview: Option<u16>,
    /// Its open PR's number.
    pr: Option<u64>,
    review: Option<review::Progress>,
}
#[derive(Serialize)]
struct Open {
    number: u64,
    title: String,
    branch: String,
    draft: bool,
}
#[derive(Serialize)]
struct Dev {
    /// The commit Dev runs, from its public health where the workspace holds no Dev of its own,
    /// as on a Mac, and its updater's state, which only Dev's own workspace knows.
    runs: String,
    update: String,
    /// `origin/main`'s commit.
    main: String,
    /// What Dev's updater wrote while its updates fail.
    failure: Option<Value>,
}

/// Everything `status` shows, read once.
pub fn read(ws: &Workspace, runner: &dyn Runner) -> Result<Status> {
    let previews = crate::preview::running(ws.machine, runner).unwrap_or_default();
    let runs = branch_runs(runner);
    let pulls: Option<Vec<Open>> = workspace::text(
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
    .map(|pulls| {
        pulls
            .iter()
            .map(|pr| Open {
                number: pr["number"].as_u64().unwrap_or(0),
                title: pr["title"].as_str().unwrap_or("").to_owned(),
                branch: pr["headRefName"].as_str().unwrap_or("").to_owned(),
                draft: pr["isDraft"] == true,
            })
            .collect()
    });
    let mut worktrees = vec![];
    for name in ws.worktrees()? {
        let change = Change::named(ws, &name)?;
        let git = |args: &[&str]| change.git(runner, args);
        let branch = change.branch(runner).unwrap_or_else(|_| "?".into());
        let ci = git(&["rev-parse", "HEAD"])
            .ok()
            .and_then(|head| ci_note(&runs, &branch, &head));
        let pr = pulls.iter().flatten().find(|pr| pr.branch == branch);
        worktrees.push(Tree {
            uncommitted: change.dirty(runner).unwrap_or(false),
            ahead: git(&["rev-list", "--count", "origin/main..HEAD"])
                .ok()
                .and_then(|ahead| ahead.parse().ok())
                .unwrap_or(0),
            ci,
            build: workspace::size(runner, &change.path.join("target")),
            test_build: workspace::size(runner, &change.path.join(".test-build")),
            preview: previews
                .contains(&name)
                .then(|| crate::preview::port(&name).ok().flatten().unwrap_or(0)),
            pr: pr.map(|pr| pr.number),
            review: review::progress(&change),
            branch,
            name,
        });
    }
    let dev = ws.dev();
    let file = |path: &str| -> Option<Value> {
        serde_json::from_str(&std::fs::read_to_string(dev.join(path)).ok()?).ok()
    };
    let dev = Dev {
        runs: file(".build/tooling/build-info.json")
            .and_then(|info| Some(info["commit"].as_str()?.to_owned()))
            .or_else(|| crate::dev::public_commit(ws, runner))
            .and_then(|commit| Some(commit.get(..8)?.to_owned()))
            .unwrap_or_else(|| "unknown".into()),
        update: file("data/platform/dev-update.json")
            .and_then(|status| Some(status["status"].as_str()?.to_owned()))
            .unwrap_or_else(|| "unknown".into()),
        main: workspace::text(
            runner,
            &["git", "rev-parse", "--short=8", "origin/main"],
            Some(&dev),
        )
        .unwrap_or_default(),
        // Written by Dev's updater while its updates fail, and removed once one gets through.
        failure: file("data/platform/dev-update-failure.json"),
    };
    Ok(Status {
        worktrees,
        pulls,
        dev,
        free: workspace::worktrees_free(&ws.root)?,
    })
}

/// `status` as lines to read.
pub fn print(status: &Status) {
    println!("Worktrees:");
    if status.worktrees.is_empty() {
        println!("  none");
    }
    for tree in &status.worktrees {
        let mut notes = vec![];
        if tree.branch != tree.name {
            notes.push(format!("on {}", tree.branch));
        }
        if tree.uncommitted {
            notes.push("uncommitted changes".into());
        }
        if tree.ahead > 0 {
            let commits = if tree.ahead == 1 { "commit" } else { "commits" };
            notes.push(format!("{} {commits} ahead of main", tree.ahead));
        }
        notes.extend(tree.ci.clone());
        if let Some(bytes) = tree.build {
            notes.push(format!("build {}", workspace::human(bytes)));
        }
        if let Some(bytes) = tree.test_build {
            notes.push(format!("test build {}", workspace::human(bytes)));
        }
        if let Some(port) = tree.preview {
            notes.push(format!("preview running on {port}"));
        }
        if let Some(pr) = tree.pr {
            notes.push(format!("PR #{pr}"));
        }
        if let Some(review) = &tree.review {
            notes.push(format!(
                "review pass {} {}",
                review.pass,
                if review.answered {
                    "answered"
                } else {
                    "with its reviewer"
                }
            ));
        }
        println!("  {}  {}", tree.name, notes.join(" · "));
    }
    println!("Open PRs:");
    match &status.pulls {
        None => println!("  GitHub didn't answer"),
        Some(pulls) if pulls.is_empty() => println!("  none"),
        Some(pulls) => {
            for pr in pulls {
                println!(
                    "  #{} {} ({}{})",
                    pr.number,
                    pr.title,
                    pr.branch,
                    if pr.draft { ", draft" } else { "" }
                );
            }
        }
    }
    let dev = &status.dev;
    println!(
        "Dev: runs {} ({}){}",
        dev.runs,
        dev.update,
        if !dev.main.is_empty() && dev.main != dev.runs {
            format!("; main is at {}", dev.main)
        } else {
            String::new()
        }
    );
    if let Some(failure) = &dev.failure {
        println!(
            "  Dev's updates have failed since {} ({} attempts): {}",
            failure["since"].as_str().unwrap_or("unknown"),
            failure["attempts"],
            failure["error"].as_str().unwrap_or("no error recorded")
        );
    }
    println!("Disk: {} free.", workspace::human(status.free));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::Machine;
    use serde_json::json;
    use std::{fs, path::Path};

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

    #[test]
    fn without_devs_build_info_dev_is_read_from_its_public_health() {
        /// Dev's public health naming `0` as the commit it runs, or nothing while Dev doesn't
        /// answer; GitHub and git answer nothing either.
        struct Public(Option<&'static str>);
        impl Runner for Public {
            fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
                match (args, self.0) {
                    (["curl", "-sSf", "https://dev.test/api/health"], Some(commit)) => {
                        Ok(json!({"status": "ready", "source": {"commit": commit}})
                            .to_string()
                            .into_bytes())
                    }
                    _ => Err(format!("{args:?} failed").into()),
                }
            }
        }
        // A workspace without Dev's build, as on a Mac.
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("worktrees")).unwrap();
        fs::create_dir_all(temp.path().join("dev/config")).unwrap();
        let config = temp.path().join("dev/config/platform.env");
        fs::write(&config, "DISPATCH_ORIGIN=\"https://dev.test\"\n").unwrap();
        let ws = Workspace {
            root: temp.path().to_path_buf(),
            machine: Machine::Mac,
        };
        let dev = |answer| read(&ws, &Public(answer)).unwrap().dev;
        let read = dev(Some("3333333333333333333333333333333333333333"));
        assert_eq!(
            (read.runs.as_str(), read.update.as_str()),
            ("33333333", "unknown")
        );
        // Dev not answering, or no address for it, leaves it unknown.
        assert_eq!(dev(None).runs, "unknown");
        fs::remove_file(&config).unwrap();
        assert_eq!(
            dev(Some("3333333333333333333333333333333333333333")).runs,
            "unknown"
        );
    }
}
