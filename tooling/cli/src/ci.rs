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
                let theirs = failing(&lines(&log(theirs.id)));
                let ours = failing(&failed);
                // Main's only when every test that fails here fails there too.
                if !ours.is_empty() && ours.iter().all(|test| theirs.contains(test)) {
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
/// The failing tests a failed log names, each as its own line without what differs between
/// runs: the worker's number, the duration, the thread's id. A summary such as `[fail] test:ui`
/// or a bare error names no test, so it isn't one.
fn failing(lines: &[&str]) -> Vec<String> {
    let varies = |token: &str| {
        let token = token.trim_matches(['(', ')', '[', ']']);
        let number = token
            .strip_suffix("ms")
            .or_else(|| token.strip_suffix('s'))
            .unwrap_or(token);
        !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    };
    let mut failing: Vec<String> = lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| {
            line.starts_with('✘')
                || line.starts_with('✖')
                || line.starts_with("FAIL [")
                || line.starts_with("test at ")
                || line.contains(" panicked at ")
        })
        .map(|line| {
            line.split_whitespace()
                .filter(|token| !varies(token))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    failing.sort();
    failing.dedup();
    failing
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_failure_is_mains_only_when_main_fails_the_same_tests() {
        let log = |tests: &[(&str, &str)]| {
            tests
                .iter()
                .map(|(worker, test)| {
                    format!(
                        "browser (2)\tRun\t2026-11-07T04:45:05.9Z   ✘  {worker} {test} (7.8s)\n\
                         browser (2)\tRun\t2026-11-07T04:45:06.6Z     Error: disposed\n\
                         browser (2)\tRun\t2026-11-07T04:45:07.0Z [fail] test:ui (41.6s)\n"
                    )
                })
                .collect::<String>()
        };
        let reload = "meal-breaks.spec.ts:594:1 › survives a reload";
        let home = "home.spec.ts:3:1 › opens";
        let ours = failing(&lines(&log(&[("22", reload)])));
        assert_eq!(ours, [format!("✘ {reload}")]);
        let shared = |theirs: &[(&str, &str)]| {
            let theirs = failing(&lines(&log(theirs)));
            ours.iter().all(|test| theirs.contains(test))
        };
        assert!(
            shared(&[("7", reload), ("3", home)]),
            "another worker, another duration"
        );
        assert!(
            !shared(&[("3", home)]),
            "the same wrapper and error lines aren't the same test"
        );
        assert!(
            failing(&lines(
                "x\ty\t2026-11-07T04:45:07.0Z [fail] test:ui (41.6s)\n"
            ))
            .is_empty()
        );
        assert_eq!(
            failing(&["thread 'week' (18381410) panicked at a.rs:3:5:"]),
            ["thread 'week' panicked at a.rs:3:5:"]
        );
    }
}
