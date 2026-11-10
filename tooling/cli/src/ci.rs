//! `dispatchdev ci`: why the checks on the checkout's PR head failed. Each failed job with the
//! end of its failing step's log, as soon as the job ended, and whether `main`'s latest finished
//! run fails the same job: then the fix is `main`'s, in its own PR, and the change's job runs
//! again once it merged.
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
/// The newest run of the checks on `branch` for `event`, or the newest `finished` one.
fn latest(
    root: &Path,
    branch: &str,
    event: &str,
    finished: bool,
    runner: &dyn Runner,
) -> Result<Option<Run>> {
    let mut list = vec![
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
    ];
    if finished {
        list.extend(["--status", "completed"]);
    }
    let runs: Vec<Value> = serde_json::from_str(&gh(root, &list, runner)?)?;
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
    let branch = workspace::text(runner, &["git", "branch", "--show-current"], Some(root))?;
    if branch == "main" {
        return main(root, runner);
    }
    let (branch, head) = review::pushed(root, runner)?;
    let Some(run) =
        latest(root, &branch, "pull_request", false, runner)?.filter(|run| run.head == head)
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
    let main = latest(root, "main", "push", true, runner)?;
    let log = |job: u64| log(root, job, runner);
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
        let failed = lines(&log(job.id));
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
                // Main's only when everything that fails here fails there too.
                if ours.is_empty() || theirs.is_empty() {
                    unsure += 1;
                    "main's latest run fails it too; its log doesn't say whether the same way"
                } else if ours.iter().all(|test| theirs.contains(test)) {
                    shared += 1;
                    "main's latest run fails it the same way"
                } else {
                    own += 1;
                    "the change's own: main's latest run fails it, but not the same way"
                }
            }
        };
        println!("FAIL {} ({}): {verdict}", job.name, job.conclusion);
        for line in &failed[failed.len().saturating_sub(LINES)..] {
            println!("    {line}");
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
    if unsure > 0 {
        next.push(
            "Compare the logs where it couldn't: failing the same way as main is main's to \
             fix, otherwise the change's own"
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

/// `dispatchdev ci` on `main`: why main's latest run of the checks failed, which is main's to
/// fix at once.
fn main(root: &Path, runner: &dyn Runner) -> Result<()> {
    let Some(run) = latest(root, "main", "push", false, runner)? else {
        println!("main has no run of the checks yet.");
        return Ok(());
    };
    if run.failed.is_empty() {
        println!(
            "Nothing failed in main's latest run, on {}{}.",
            short(&run.head),
            if run.done {
                ""
            } else {
                " so far: the run goes on"
            }
        );
        return Ok(());
    }
    let others = run
        .failed
        .iter()
        .any(|job| job.name != crate::merge::REQUIRED);
    for job in &run.failed {
        if job.name == crate::merge::REQUIRED && others {
            println!("FAIL {}: it fails because the jobs above did", job.name);
            continue;
        }
        println!(
            "FAIL {} ({}) on {}",
            job.name,
            job.conclusion,
            short(&run.head)
        );
        let failed = lines(&log(root, job.id, runner));
        for line in &failed[failed.len().saturating_sub(LINES)..] {
            println!("    {line}");
        }
    }
    println!(
        "{}",
        workspace::next(&[format!(
            "It blocks every PR: fix it at once in its own PR off main, merged with no review; \
             a flake unrelated to its commit runs again once, `gh run rerun {} --failed`",
            run.id
        )])
    );
    Ok(())
}

/// A job's whole log, which GitHub hands out once the job ended: `gh run view --log-failed`
/// has none until every job of the run did. Empty when it can't be had.
fn log(root: &Path, job: u64, runner: &dyn Runner) -> String {
    workspace::text(
        runner,
        &[
            "gh",
            "api",
            "--allow-escape-sequences",
            &format!("repos/{REPOSITORY}/actions/jobs/{job}/logs"),
        ],
        Some(root),
    )
    .unwrap_or_default()
}
/// The error GitHub writes where a step fails.
const FAILED_STEP: &str = "##[error]Process completed with exit code";
/// A job's log as lines, up to where its first step failed: what follows is the job cleaning
/// up. Each without the timestamp it starts with and the colours within it.
fn lines(log: &str) -> Vec<String> {
    let mut lines: Vec<String> = log
        .trim_start_matches('\u{feff}')
        .lines()
        .map(|line| {
            let text = line.split_once(' ').map_or("", |(_, rest)| rest);
            plain(text).trim_end().to_owned()
        })
        .collect();
    if let Some(failed) = lines.iter().position(|line| line.starts_with(FAILED_STEP)) {
        lines.truncate(failed + 1);
    }
    lines
}
/// `text` without its terminal colours: each escape sequence up to the letter that ends it.
fn plain(text: &str) -> String {
    let mut plain = String::new();
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            plain.push(character);
        } else if characters.next() == Some('[') {
            characters.find(|character| character.is_ascii_alphabetic());
        }
    }
    plain
}
/// What a failed log says failed, each as its own line without what differs between runs (the
/// worker's number, the duration, the thread's id): failing tests, compiler and lint errors by
/// their code, and what `cargo audit` denies. A summary such as `[fail] test:ui`, or a bare
/// error, names nothing that failed, so it isn't one.
fn failing<Line: AsRef<str>>(lines: &[Line]) -> Vec<String> {
    // A worker's number, a duration, a thread's id: a bare count, or a number with a unit or in
    // brackets. A version such as 0.3.41 stays.
    let varies = |token: &str| {
        let inner = token.trim_matches(['(', ')', '[', ']']);
        let number = inner
            .strip_suffix("ms")
            .or_else(|| inner.strip_suffix('s'))
            .unwrap_or(inner);
        let numeric = !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit() || b == b'.');
        numeric && (number.bytes().all(|b| b.is_ascii_digit()) || number != inner || inner != token)
    };
    // The advisories `cargo audit denies:` lists, a `- ` line each, until another line.
    let mut denied = false;
    let mut failing: Vec<String> = lines
        .iter()
        .map(|line| line.as_ref().trim())
        .filter(|line| {
            denied = (denied && line.starts_with("- ")) || line.starts_with("cargo audit denies:");
            (denied && line.starts_with("- "))
                || line.starts_with('✘')
                || line.starts_with('✖')
                || line.starts_with("FAIL [")
                || line.starts_with("test at ")
                || line.starts_with("error[")
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
                        "2026-11-07T04:45:05.9Z   ✘  {worker} {test} (7.8s)\n\
                         2026-11-07T04:45:06.6Z     Error: disposed\n\
                         2026-11-07T04:45:07.0Z [fail] test:ui (41.6s)\n"
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
        assert_eq!(
            failing(
                &[
                    "cargo audit denies:",
                    "- RUSTSEC-2026-0012 on time 0.3.41",
                    "- yanked ring 0.17.9",
                    "- not after the list",
                ][..3]
            ),
            ["- RUSTSEC-2026-0012 on time 0.3.41", "- yanked ring 0.17.9"]
        );
        assert_eq!(
            failing(&[
                "error[E0308]: mismatched types",
                "error: could not compile `core`"
            ]),
            ["error[E0308]: mismatched types"]
        );
        assert!(failing(&lines("2026-11-07T04:45:07.0Z [fail] test:ui (41.6s)\n")).is_empty());
        // A job's log ends for the reader where its step failed, without its colours.
        assert_eq!(
            lines(
                "\u{feff}2026-11-10T20:22:39.3Z \u{1b}[31;1mFAIL\u{1b}[0m [ 1.256s] review::tests::x\n\
                 2026-11-10T20:22:39.4Z ##[error]Process completed with exit code 100.\n\
                 2026-11-10T20:22:40.0Z Post job cleanup.\n"
            ),
            [
                "FAIL [ 1.256s] review::tests::x",
                "##[error]Process completed with exit code 100."
            ]
        );
        assert_eq!(
            failing(&["thread 'week' (18381410) panicked at a.rs:3:5:"]),
            ["thread 'week' panicked at a.rs:3:5:"]
        );
    }
}
