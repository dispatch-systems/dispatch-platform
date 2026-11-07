//! `dispatchdev ci`: why the checks on the checkout's PR head failed. Each failed job with the
//! end of its failing step's log, and whether `main`'s latest run fails the same job: then the
//! fix is `main`'s, in its own PR, and the change's job runs again once it merged.
use crate::{REPOSITORY, Result, Runner, merge::short, review, workspace};
use serde_json::Value;
use std::path::Path;

/// The workflow whose jobs a PR needs.
const WORKFLOW: &str = "checks.yml";
/// How much of a failed job's log is shown: its last lines.
const LINES: usize = 40;

/// A run of the checks, and its failed jobs by name and id.
struct Run {
    id: u64,
    head: String,
    done: bool,
    failed: Vec<(String, u64)>,
}
fn gh(root: &Path, args: &[&str], runner: &dyn Runner) -> Result<String> {
    let mut command = vec!["gh"];
    command.extend_from_slice(args);
    command.extend_from_slice(&["--repo", REPOSITORY]);
    workspace::text(runner, &command, Some(root))
}
/// The newest run of the checks on `branch` for `event`.
fn latest(root: &Path, branch: &str, event: &str, runner: &dyn Runner) -> Result<Option<Run>> {
    let runs: Vec<Value> = serde_json::from_str(&gh(
        root,
        &[
            "run",
            "list",
            "--workflow",
            WORKFLOW,
            "--branch",
            branch,
            "--event",
            event,
            "--limit",
            "1",
            "--json",
            "databaseId,headSha,status",
        ],
        runner,
    )?)?;
    let Some(run) = runs.first() else {
        return Ok(None);
    };
    let id = run["databaseId"].as_u64().ok_or("A run without an id")?;
    let jobs: Value = serde_json::from_str(&gh(
        root,
        &["run", "view", &id.to_string(), "--json", "jobs"],
        runner,
    )?)?;
    let failed = jobs["jobs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|job| matches!(job["conclusion"].as_str(), Some("failure" | "timed_out")))
        // The gate fails for the others; it says nothing of its own.
        .filter(|job| job["name"] != crate::merge::REQUIRED)
        .filter_map(|job| {
            Some((
                job["name"].as_str()?.to_owned(),
                job["databaseId"].as_u64()?,
            ))
        })
        .collect();
    Ok(Some(Run {
        id,
        head: run["headSha"].as_str().unwrap_or_default().to_owned(),
        done: run["status"] == "completed",
        failed,
    }))
}

pub fn run(root: &Path, runner: &dyn Runner) -> Result<()> {
    let (branch, head) = review::pushed(root, runner)?;
    let Some(run) = latest(root, &branch, "pull_request", runner)?.filter(|run| run.head == head)
    else {
        println!("No checks have started on {} yet.", short(&head));
        return Ok(());
    };
    if run.failed.is_empty() {
        println!(
            "Nothing failed on {}{}.",
            short(&head),
            if run.done {
                ""
            } else {
                " so far: the run goes on"
            }
        );
        return Ok(());
    }
    let main = latest(root, "main", "push", runner)?;
    let mut own = 0;
    for (name, job) in &run.failed {
        let shared = main
            .as_ref()
            .is_some_and(|main| main.failed.iter().any(|(failed, _)| failed == name));
        own += usize::from(!shared);
        println!(
            "FAIL {name}: {}",
            match shared {
                true => "main's latest run fails it too",
                false => "the change's own",
            }
        );
        let log = gh(
            root,
            &["run", "view", "--job", &job.to_string(), "--log-failed"],
            runner,
        )
        .unwrap_or_default();
        let lines: Vec<&str> = log.lines().collect();
        for line in &lines[lines.len().saturating_sub(LINES)..] {
            // Each line starts with the job's and the step's names, then a timestamp.
            let text = line.splitn(3, '\t').nth(2).unwrap_or(line);
            println!(
                "    {}",
                text.split_once(' ').map_or(text, |(_, rest)| rest)
            );
        }
    }
    let mut next = vec![];
    if own > 0 {
        next.push(
            "Fix what is the change's own, commit, and push with `dispatchdev pr`; a flake \
                   unrelated to the diff runs again once"
                .to_owned(),
        );
    }
    if own < run.failed.len() {
        next.push(format!(
            "What main fails too is main's to fix, at once, in its own PR; once it merged, `gh \
             run rerun {} --failed`",
            run.id
        ));
    }
    println!("{}", workspace::next(&next));
    Ok(())
}
