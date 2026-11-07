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
    failed: Vec<Failed>,
}
/// A job that ended without passing: failed, timed out or cancelled.
struct Failed {
    name: String,
    id: u64,
    conclusion: String,
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
        .filter(|job| {
            matches!(
                job["conclusion"].as_str(),
                Some("failure" | "timed_out" | "cancelled" | "startup_failure")
            )
        })
        .filter_map(|job| {
            Some(Failed {
                name: job["name"].as_str()?.to_owned(),
                id: job["databaseId"].as_u64()?,
                conclusion: job["conclusion"].as_str()?.to_owned(),
            })
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
    let main = latest(root, "main", "push", runner)?.filter(|main| main.done);
    let log = |job: u64| {
        gh(
            root,
            &["run", "view", "--job", &job.to_string(), "--log-failed"],
            runner,
        )
        .unwrap_or_default()
    };
    let others = run
        .failed
        .iter()
        .any(|job| job.name != crate::merge::REQUIRED);
    let (mut own, mut shared, mut unsure) = (0, 0, 0);
    for job in &run.failed {
        // The gate fails whenever another job does; alone, its own steps failed.
        if job.name == crate::merge::REQUIRED && others {
            println!("FAIL {}: it fails because the jobs above did", job.name);
            continue;
        }
        let text = log(job.id);
        let failed = lines(&text);
        let verdict = match main
            .as_ref()
            .and_then(|main| main.failed.iter().find(|failed| failed.name == job.name))
        {
            None if main.is_none() => {
                unsure += 1;
                "main has no finished run to compare with"
            }
            None => {
                own += 1;
                "the change's own: main's latest run passes it"
            }
            Some(theirs) => {
                let theirs = log(theirs.id);
                let theirs = signature(&lines(&theirs));
                if signature(&failed).iter().any(|line| theirs.contains(line)) {
                    shared += 1;
                    "main's latest run fails it the same way"
                } else {
                    unsure += 1;
                    "main's latest run fails it too, but not the same way"
                }
            }
        };
        println!("FAIL {} ({}): {verdict}", job.name, job.conclusion);
        for line in &failed[failed.len().saturating_sub(LINES)..] {
            println!("    {line}");
        }
    }
    let mut next = vec![];
    if own + unsure > 0 {
        next.push(
            "Fix what is the change's own, commit, and push with `dispatchdev pr`; a flake \
             unrelated to the diff runs again once"
                .to_owned(),
        );
    }
    if shared > 0 {
        next.push(format!(
            "What main fails the same way is main's to fix, at once, in its own PR; once it \
             merged, `gh run rerun {} --failed`",
            run.id
        ));
    }
    println!("{}", workspace::next(&next));
    Ok(())
}

/// A failed step's log as lines, without the job's and step's names and the timestamp each
/// line starts with.
fn lines(log: &str) -> Vec<&str> {
    log.lines()
        .map(|line| {
            let text = line.splitn(3, '\t').nth(2).unwrap_or(line);
            text.split_once(' ')
                .map_or(text, |(_, rest)| rest)
                .trim_end()
        })
        .collect()
}
/// The lines of a failed log that name what failed: a failing test or check, or an error.
fn signature<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| {
            ["✘", "✖", "FAIL", "[fail]", "panicked", "error[", "Error:"]
                .iter()
                .any(|sign| line.contains(sign))
        })
        // A test's own line, without its duration, which differs between runs.
        .map(|line| line.rsplit_once(" (").map_or(line, |(line, _)| line))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_failure_is_mains_only_when_its_log_fails_the_same_way() {
        let log = |test: &str| {
            format!(
                "browser (2)\tRun\t2026-11-07T04:45:05.9Z   ✘  22 {test} (7.8s)\n\
                 browser (2)\tRun\t2026-11-07T04:45:06.6Z     Error: Response has been disposed\n"
            )
        };
        let ours = log("meal-breaks.spec.ts:594:1 › survives a reload");
        let ours = signature(&lines(&ours));
        let shared = |theirs: &str| {
            let theirs = signature(&lines(theirs));
            ours.iter().any(|line| theirs.contains(line))
        };
        assert!(shared(
            &log("meal-breaks.spec.ts:594:1 › survives a reload").replace("(7.8s)", "(9.1s)")
        ));
        assert!(!shared(
            "browser (2)\tRun\t2026-11-07T04:45:05.9Z   ✘  3 home.spec.ts:3:1 › opens (30.1s)\n"
        ));
        assert_eq!(
            lines(&log("x")),
            ["  ✘  22 x (7.8s)", "    Error: Response has been disposed"]
        );
    }
}
