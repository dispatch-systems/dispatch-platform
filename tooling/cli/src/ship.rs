//! `dispatchdev ship <name>`: a change from its committed worktree to Dev. It checks what
//! takes seconds first: the PR's body in the change's scratch folder, that it can wait for Dev
//! once merged, the branch, that it merges cleanly into main, and the push. It opens or updates the PR, adds it to the merge
//! queue as soon as GitHub admits it and waits until GitHub merges it, the rule checks running
//! here beside it on a machine that runs the local checks, unless the checks on GitHub passed
//! on this commit already; their failure takes it out of the queue. Then it removes all the
//! change made, and waits until Dev runs the merge. Run again after a stop, it picks up where it
//! stopped: a change whose PR merged with what its worktree holds goes straight to the clean-up.
//!
//! The queue runs the checks on the exact squash commit it will push, so nothing but the
//! admission check runs on the PR itself. A head that failed the checks `dispatchdev ci` ran on
//! its branch is not queued, and those still running are cancelled when it is. Everything is
//! read from GitHub's API, never from a command's text: a newer push is queued in its turn, and
//! the wait stops with the reason when the PR closes or leaves the queue unmerged, naming the
//! failed jobs of its queue run.
use crate::{
    REPOSITORY, Result, Runner,
    change::Change,
    check, finish, pr, require,
    workspace::{self, Workspace},
};
use serde_json::Value;
use std::{thread::ScopedJoinHandle, time::Instant};

/// A stop with an exit code of its own: `OWNER` when the owner has to decide, `MERGED` once the
/// PR merged, so it's never shipped again. Any other stop exits with 1: fix it and ship again.
#[derive(Debug)]
pub struct Exit {
    pub code: i32,
    pub message: String,
}
pub const OWNER: i32 = 2;
pub const MERGED: i32 = 3;
impl std::fmt::Display for Exit {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Exit {}
fn stop(code: i32, message: String) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(Exit { code, message })
}

/// `dispatchdev ship <name>`, saying each step's outcome once.
pub fn change(
    ws: &Workspace,
    name: &str,
    runner: &dyn Runner,
    pause: &dyn Fn(u64),
    say: &mut dyn FnMut(&str),
) -> Result<()> {
    let change = Change::existing(ws, name)?;
    require(
        !change.dirty(runner)?,
        "Commit the change before shipping it.",
    )?;
    let head = change.git(runner, &["rev-parse", "HEAD"])?;
    let body = pr::body_file(&change);
    let text = std::fs::read_to_string(&body).map_err(|_| {
        format!(
            "Write the PR's body to {} first: dispatchdev pr {name} --skeleton --title <title> \
             starts it.",
            body.display()
        )
    })?;
    let (title, rest) = pr::titled(&text);
    let title = title
        .ok_or_else(|| {
            format!(
                "{} has no title: start it with a line # <title>.",
                body.display()
            )
        })?
        .to_owned();
    let landed = change
        .pull(runner)?
        .filter(|pr| pr.merged() && pr.head == head);
    let (number, merge) = match landed {
        Some(pr) => {
            say(&format!("#{} merged already.", pr.number));
            (pr.number, pr.merge.unwrap_or_default())
        }
        None => {
            // What stops it in seconds comes before what takes a minute.
            pr::check_body(&title, rest)?;
            finish::ready(ws)?;
            require(
                change.branch(runner)? == name,
                &format!("worktrees/{name} isn't on its branch."),
            )?;
            change.git(runner, &["fetch", "-q", "origin", "main"])?;
            require(
                change
                    .git(
                        runner,
                        &["merge-tree", "--write-tree", "origin/main", "HEAD"],
                    )
                    .is_ok(),
                &format!(
                    "{name} conflicts with main: merge origin/main, commit and ship it again."
                ),
            )?;
            change.git(
                runner,
                &["push", "-q", "--force-with-lease", "-u", "origin", name],
            )?;
            // The queue runs every check again on the squash commit: here, only the rule
            // checks, beside it, on a machine that runs them, unless the checks on GitHub
            // passed on this very commit.
            let passed = matches!(
                branch_run(runner, &head),
                Some(run) if run["conclusion"] == "success"
            );
            if passed {
                say(&format!(
                    "The checks passed on {} on GitHub already.",
                    &head[..head.len().min(7)]
                ));
            }
            let rules = if passed {
                None
            } else {
                check::beside(ws.machine)
            };
            let number = pr::run(ws, name, None, &body, runner)?;
            let queued = Instant::now();
            let path = &change.path;
            let merge = std::thread::scope(|scope| {
                let mut local = rules.map(|rules| scope.spawn(move || rules(path)));
                run(number, &mut local, runner, pause, say)
            })?;
            say(&format!(
                "Merged #{number} {title} as {} after {} in the queue.",
                merge.get(..8).unwrap_or(&merge),
                workspace::duration(queued.elapsed())
            ));
            (number, merge)
        }
    };
    let merged = |what: String| {
        stop(
            MERGED,
            format!(
                "#{number} merged as {}, but {what}. Don't ship it again: dispatchdev finish \
                 {name} completes it.",
                merge.get(..8).unwrap_or(&merge)
            ),
        )
    };
    let removed = change
        .clean(ws, runner)
        .map_err(|error| merged(format!("removing what it made stopped: {error}")))?;
    if !removed.is_empty() {
        say(&format!("Removed: {}.", removed.join(", ")));
    }
    let waited = Instant::now();
    let live = finish::live(ws, &merge, runner, pause)
        .map_err(|error| merged(format!("Dev didn't install it: {error}")))?;
    say(&format!(
        "Live on Dev at {live} after {}.",
        workspace::duration(waited.elapsed())
    ));
    Ok(())
}

/// Seconds between looks at the PR.
pub(crate) const PAUSE: u64 = 10;
/// The looks that fit in `seconds`.
pub(crate) const fn looks(seconds: u64) -> u32 {
    (seconds / PAUSE) as u32
}
/// Looks that bound the whole wait: 90 minutes.
const LOOKS: u32 = looks(90 * 60);
/// Consecutive failed API calls, or refused additions to the queue, before giving up: about
/// a minute and a half.
pub(crate) const ATTEMPTS: u32 = looks(100);

/// The check the ruleset expects on a PR head before the queue admits it, which
/// `queue-admission.yml` reports, and the queue's own gate.
const REQUIRED: &str = "platform";
const LOOK: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){id state isDraft headRefOid mergeable mergeCommit{oid} mergeQueueEntry{state position} commits(last:1){nodes{commit{oid statusCheckRollup{contexts(first:50){nodes{__typename ...on CheckRun{name conclusion}}}}}}} timelineItems(last:1,itemTypes:[REMOVED_FROM_MERGE_QUEUE_EVENT]){nodes{...on RemovedFromMergeQueueEvent{reason}}}}}}";
const ENQUEUE: &str = "mutation($id:ID!,$head:GitObjectID!){enqueuePullRequest(input:{pullRequestId:$id,expectedHeadOid:$head}){mergeQueueEntry{position}}}";
const DEQUEUE: &str =
    "mutation($id:ID!){dequeuePullRequest(input:{id:$id}){mergeQueueEntry{position}}}";

fn graphql(runner: &dyn Runner, query: &str, variables: &[String]) -> Result<Value> {
    let query = format!("query={query}");
    let mut args = vec!["gh", "api", "graphql", "-f", &query];
    for variable in variables {
        // Numbers go as typed fields, everything else as strings.
        let typed = variable.starts_with("number=");
        args.extend([if typed { "-F" } else { "-f" }, variable]);
    }
    let reply: Value = serde_json::from_slice(&runner.command(&args, None, 60)?)?;
    if let Some(errors) = reply.get("errors") {
        return Err(format!("GitHub refused: {errors}").into());
    }
    Ok(reply["data"].clone())
}

pub(crate) fn rest(runner: &dyn Runner, endpoint: &str) -> Result<Value> {
    let endpoint = format!("repos/{REPOSITORY}/{endpoint}");
    Ok(serde_json::from_slice(&runner.command(
        &["gh", "api", &endpoint],
        None,
        60,
    )?)?)
}

fn look(runner: &dyn Runner, number: u64) -> Result<Value> {
    let (owner, name) = REPOSITORY.split_once('/').ok_or("Invalid repository")?;
    let data = graphql(
        runner,
        LOOK,
        &[
            format!("owner={owner}"),
            format!("name={name}"),
            format!("number={number}"),
        ],
    )?;
    let pr = data["repository"]["pullRequest"].clone();
    if pr.is_object() {
        Ok(pr)
    } else {
        Err(format!("#{number} is not a pull request of {REPOSITORY}").into())
    }
}

/// What the admission check says about the PR's current head.
#[derive(Debug, PartialEq)]
enum Admission {
    Pending,
    Passed,
    Failed,
}

fn admission(pr: &Value) -> Admission {
    let commit = &pr["commits"]["nodes"][0]["commit"];
    if commit["oid"] != pr["headRefOid"] {
        return Admission::Pending;
    }
    let contexts = &commit["statusCheckRollup"]["contexts"]["nodes"];
    match contexts
        .as_array()
        .into_iter()
        .flatten()
        .find(|context| context["__typename"] == "CheckRun" && context["name"] == REQUIRED)
        .and_then(|context| context["conclusion"].as_str())
    {
        Some("SUCCESS") => Admission::Passed,
        Some("FAILURE" | "CANCELLED" | "TIMED_OUT" | "STARTUP_FAILURE") => Admission::Failed,
        _ => Admission::Pending,
    }
}

/// The newest queue run for `number`, if any. The queue names its branch after the PR, which
/// is how the run is found. Any API trouble here only costs detail, never the verdict.
fn queue_run(runner: &dyn Runner, number: u64) -> Option<Value> {
    let prefix = format!("gh-readonly-queue/main/pr-{number}-");
    let runs = rest(
        runner,
        "actions/workflows/checks.yml/runs?event=merge_group&per_page=30",
    )
    .ok()?;
    runs["workflow_runs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|run| {
            run["head_branch"]
                .as_str()
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .max_by_key(|run| run["id"].as_u64().unwrap_or(0))
        .cloned()
}

/// The newest run of the checks on `head` outside the queue: the one `dispatchdev ci` started
/// on the PR's branch. Any API trouble here only costs the look, never the verdict.
fn branch_run(runner: &dyn Runner, head: &str) -> Option<Value> {
    let runs = rest(
        runner,
        &format!(
            "actions/workflows/checks.yml/runs?event=workflow_dispatch&head_sha={head}&per_page=20"
        ),
    )
    .ok()?;
    runs["workflow_runs"]
        .as_array()?
        .iter()
        .max_by_key(|run| run["id"].as_u64().unwrap_or(0))
        .cloned()
}

/// Jobs of `checks.yml` that may fail without failing the gate, its `continue-on-error` jobs;
/// `tooling/tests/build_pipeline_test.py` holds this list to the workflow.
const ADVISORY: &[&str] = &["tools"];

/// The failed jobs of `run`. With `gating`, only the failures that already fail the gate,
/// while the rest of the run is still going.
pub(crate) fn failed_jobs(runner: &dyn Runner, run: &Value, gating: bool) -> Vec<Value> {
    let Ok(jobs) = rest(
        runner,
        &format!("actions/runs/{}/jobs?per_page=100", run["id"]),
    ) else {
        return vec![];
    };
    let failed: Vec<&Value> = jobs["jobs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|job| match job["conclusion"].as_str() {
            Some("failure" | "timed_out" | "startup_failure") => {
                // A matrix job is named after its job and its matrix value, as `browser (3)`.
                let name = job["name"].as_str().unwrap_or("");
                !gating || !ADVISORY.contains(&name.split(" (").next().unwrap_or(name))
            }
            Some("cancelled") => !gating,
            _ => false,
        })
        .collect();
    // The gate fails whenever a suite did, and a failing suite reports it failed to the queue
    // before the suite's own job ends, listed among the jobs: the gate is named only when it
    // failed alone at the end.
    let suites = failed.iter().any(|job| job["name"] != REQUIRED);
    failed
        .into_iter()
        .filter(|job| job["name"] != REQUIRED || (!suites && !gating))
        .cloned()
        .collect()
}

/// `jobs` as "name: conclusion link" lines, each failure followed by the lines of its log that
/// say why, indented. A cancelled job's log says nothing of a failure.
pub(crate) fn report(runner: &dyn Runner, jobs: &[Value]) -> Vec<String> {
    jobs.iter()
        .map(|job| {
            let mut text = format!(
                "{}: {} {}",
                job["name"].as_str().unwrap_or("job"),
                job["conclusion"].as_str().unwrap_or(""),
                job["html_url"].as_str().unwrap_or("")
            );
            if job["conclusion"] != "cancelled" {
                for line in failing_lines(runner, &job["id"]) {
                    text.push_str(&format!("\n  {line}"));
                }
            }
            text
        })
        .collect()
}

/// The lines of job `id`'s log that say why it failed, none while GitHub has no log for it.
/// They come from the step that failed, from its `Run` header to the first error GitHub
/// reports, so the steps setting up the job and those after the failure stay out.
fn failing_lines(runner: &dyn Runner, id: &Value) -> Vec<String> {
    let endpoint = format!("repos/{REPOSITORY}/actions/jobs/{id}/logs");
    // A log keeps its commands' colour codes, which `gh` prints only when allowed to.
    let Ok(log) = runner.command(
        &["gh", "api", "--allow-escape-sequences", &endpoint],
        None,
        60,
    ) else {
        return vec![];
    };
    let log = String::from_utf8_lossy(&log);
    let lines: Vec<&str> = log.lines().map(untimed).collect();
    let step = match lines.iter().position(|line| line.starts_with("##[error]")) {
        Some(end) => {
            let start = lines[..end]
                .iter()
                .rposition(|line| line.starts_with("##[group]Run "))
                .unwrap_or(0);
            &lines[start..=end]
        }
        None => &lines[..],
    };
    check::failure(&step.join("\n"))
}

/// A job log's line without the time GitHub starts it with, as `2026-10-20T05:24:41.9021338Z`.
fn untimed(line: &str) -> &str {
    let line = line.trim_start_matches('\u{feff}');
    let (at, text) = line.split_once(' ').unwrap_or((line, ""));
    if at.get(10..11) == Some("T") && at.ends_with('Z') {
        text
    } else {
        line
    }
}

fn short(head: &Value) -> &str {
    head.as_str().map_or("", |head| &head[..head.len().min(7)])
}

/// Queue `number` and wait for the merge; returns the squash commit. The rule checks here,
/// `local`, run beside it, and their failure takes it out of the queue. `pause` waits between
/// looks and `say` reports each change of progress once.
pub fn run(
    number: u64,
    local: &mut Option<ScopedJoinHandle<Result<()>>>,
    runner: &dyn Runner,
    pause: &dyn Fn(u64),
    say: &mut dyn FnMut(&str),
) -> Result<String> {
    let mut last = String::new();
    let mut note = |text: String| {
        if text != last {
            say(&text);
            last = text;
        }
    };
    // The head this run queued or found queued, and how often it was then seen outside it.
    let mut queued: Option<Value> = None;
    let mut outside = 0;
    // The newest queue run of this PR before this run queued it: only a later run is its own.
    let mut earlier: Option<u64> = None;
    // The head whose checks on the branch were last looked at.
    let mut looked: Option<String> = None;
    let (mut seen, mut unanswered, mut refused) = (false, 0, 0);
    for _ in 0..LOOKS {
        let pr = match look(runner, number) {
            Ok(pr) => {
                (seen, unanswered) = (true, 0);
                pr
            }
            Err(error) => {
                // Failing on the first look is a wrong number or missing access, not an outage.
                unanswered += 1;
                if unanswered >= ATTEMPTS || !seen {
                    return Err(error);
                }
                pause(PAUSE);
                continue;
            }
        };
        let head = &pr["headRefOid"];
        match pr["state"].as_str() {
            Some("MERGED") => {
                return Ok(pr["mergeCommit"]["oid"].as_str().unwrap_or("").into());
            }
            Some("CLOSED") => {
                return Err(stop(OWNER, format!("#{number} was closed without merging")));
            }
            _ => {}
        }
        // The queue runs the rule checks too, so their failure here would fail its run: the
        // entry goes, rather than wait for that.
        if local.as_ref().is_some_and(ScopedJoinHandle::is_finished)
            && let Err(error) = crate::ci::joined(local.take())
        {
            let id = format!("id={}", pr["id"].as_str().unwrap_or(""));
            let out = (queued.is_some() || pr["mergeQueueEntry"].is_object())
                && graphql(runner, DEQUEUE, &[id]).is_ok();
            return Err(format!(
                "{error} Fix it, commit and ship it again{}.",
                if out {
                    format!("; #{number} was taken out of the merge queue")
                } else {
                    String::new()
                }
            )
            .into());
        }
        if pr["isDraft"] == true {
            return Err(stop(
                OWNER,
                format!("#{number} is a draft; mark it ready for review, then ship it"),
            ));
        }
        let entry = &pr["mergeQueueEntry"];
        if entry.is_object() {
            // One failed job fails the gate, so the verdict is out before the run ends. An
            // entry this run did not see start, found already failing, is waited out: after a
            // fix is pushed, the new head is queued once GitHub has taken the old one out.
            let failed = match queue_run(runner, number) {
                Some(run) if run["id"].as_u64() > earlier || earlier.is_none() => {
                    failed_jobs(runner, &run, true)
                }
                _ => vec![],
            };
            if !failed.is_empty() {
                if queued.as_ref() == Some(head) {
                    return Err(format!(
                        "#{number} failed its queue run, and GitHub takes it out of the queue at once, or once the run ends if another PR is queued ahead of it. Fix it, push and ship it again; shipping waits for it to leave the queue first. Failed jobs so far:\n- {}",
                        report(runner, &failed).join("\n- ")
                    )
                    .into());
                }
                note(format!(
                    "Waiting for GitHub to take #{number} out of the queue after its failed run"
                ));
                pause(PAUSE);
                continue;
            }
            queued = Some(head.clone());
            outside = 0;
            note(format!(
                "#{number} is in the merge queue at position {}: {}",
                entry["position"],
                entry["state"].as_str().unwrap_or("").to_lowercase()
            ));
            pause(PAUSE);
            continue;
        }
        if queued.as_ref() == Some(head) {
            // Just queued or just merged, GitHub can briefly show neither; a second look decides.
            outside += 1;
            if outside >= 2 {
                let reason = pr["timelineItems"]["nodes"][0]["reason"]
                    .as_str()
                    .unwrap_or("removed")
                    .to_owned();
                let failed = match queue_run(runner, number) {
                    Some(run) if run["id"].as_u64() > earlier || earlier.is_none() => {
                        failed_jobs(runner, &run, false)
                    }
                    _ => vec![],
                };
                return Err(if failed.is_empty() {
                    format!(
                        "#{number} left the merge queue without merging: {reason}. Queue runs: https://github.com/{REPOSITORY}/actions?query=event%3Amerge_group"
                    )
                } else {
                    format!(
                        "#{number} left the merge queue without merging: {reason}. Failed jobs of its queue run:\n- {}",
                        report(runner, &failed).join("\n- ")
                    )
                }
                .into());
            }
            pause(PAUSE);
            continue;
        }
        // The queue admits a head only once its admission check passed and GitHub knows it
        // merges cleanly; queued before that, GitHub drops it as an invalid merge commit.
        if pr["mergeable"] == "CONFLICTING" {
            return Err(format!(
                "#{number} conflicts with main; merge origin/main, push and ship it again"
            )
            .into());
        }
        match admission(&pr) {
            Admission::Failed => {
                return Err(format!(
                    "The admission check failed on {} of #{number}; rerun it, or fix queue-admission.yml",
                    short(head)
                )
                .into());
            }
            Admission::Passed if pr["mergeable"] == "MERGEABLE" => {}
            _ => {
                note(format!(
                    "Waiting for the admission check on {} of #{number}",
                    short(head)
                ));
                pause(PAUSE);
                continue;
            }
        }
        if earlier.is_none() {
            earlier = Some(
                queue_run(runner, number)
                    .and_then(|run| run["id"].as_u64())
                    .unwrap_or(0),
            );
        }
        let id = pr["id"].as_str().unwrap_or("");
        let head = head.as_str().unwrap_or("");
        // The checks `dispatchdev ci` ran on this head: a failure stops it before it costs a
        // queue run, and a run still going gives way to the queue's.
        if looked.as_deref() != Some(head) {
            looked = Some(head.into());
            if let Some(run) = branch_run(runner, head) {
                let done = run["status"] == "completed";
                let failed = if done && run["conclusion"] != "failure" {
                    vec![]
                } else {
                    failed_jobs(runner, &run, !done)
                };
                if !failed.is_empty() {
                    return Err(format!(
                        "The checks on the branch failed on {} of #{number}. Fix them, push and ship it again. Failed jobs{}:\n- {}",
                        &head[..head.len().min(7)],
                        if done { "" } else { " so far" },
                        report(runner, &failed).join("\n- ")
                    )
                    .into());
                }
                if !done {
                    let cancel = format!("repos/{REPOSITORY}/actions/runs/{}/cancel", run["id"]);
                    if runner
                        .command(&["gh", "api", "-X", "POST", &cancel], None, 60)
                        .is_ok()
                    {
                        note(format!(
                            "Cancelled the checks still running on the branch at {}; the queue runs them on its squash commit",
                            &head[..head.len().min(7)]
                        ));
                    }
                }
            }
        }
        match graphql(
            runner,
            ENQUEUE,
            &[format!("id={id}"), format!("head={head}")],
        ) {
            Ok(_) => {
                refused = 0;
                queued = Some(head.into());
                note(format!(
                    "Added #{number} to the merge queue at {}; the queue runs the checks on its squash commit",
                    short(&head.into())
                ));
            }
            // Refused when the head moved meanwhile, or while GitHub catches up with the
            // admission check: the next look follows the new head or tries again.
            Err(error) => {
                refused += 1;
                if refused >= ATTEMPTS {
                    return Err(error);
                }
            }
        }
        pause(PAUSE);
    }
    Err(format!(
        "Gave up after {} minutes; #{number} has not merged",
        u64::from(LOOKS) * PAUSE / 60
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{cell::RefCell, collections::BTreeMap, collections::VecDeque, path::Path};

    const OLD: &str = "1111111111111111111111111111111111111111";
    const NEW: &str = "2222222222222222222222222222222222222222";

    /// GitHub as a script: each look answers with the next PR state, the last one repeating,
    /// and each REST endpoint likewise; a job's log is there once it is in `logs`.
    #[derive(Default)]
    struct GitHub {
        looks: RefCell<VecDeque<Result<Value>>>,
        refusals: RefCell<VecDeque<String>>,
        queued: RefCell<Vec<String>>,
        rest: RefCell<BTreeMap<String, VecDeque<Value>>>,
        logs: RefCell<BTreeMap<String, &'static str>>,
        cancelled: RefCell<Vec<String>>,
        dequeued: RefCell<Vec<String>>,
    }
    impl Runner for GitHub {
        fn command(&self, args: &[&str], _cwd: Option<&Path>, timeout: u64) -> Result<Vec<u8>> {
            assert_eq!(&args[..2], ["gh", "api"]);
            assert_eq!(timeout, 60);
            if args.get(2..4) == Some(&["-X", "POST"][..]) {
                self.cancelled.borrow_mut().push(args[4].into());
                return Ok(vec![]);
            }
            if args[2] == "--allow-escape-sequences" {
                let endpoint = args[3].strip_prefix(&format!("repos/{REPOSITORY}/"));
                let logs = self.logs.borrow();
                let log = endpoint.and_then(|endpoint| logs.get(endpoint));
                return Ok(log.ok_or("not found")?.as_bytes().to_vec());
            }
            if let Some(endpoint) = args[2].strip_prefix(&format!("repos/{REPOSITORY}/")) {
                assert_eq!(args.len(), 3);
                let mut rest = self.rest.borrow_mut();
                let replies = rest.get_mut(endpoint).ok_or("not found")?;
                let reply = if replies.len() > 1 {
                    replies.pop_front().unwrap()
                } else {
                    replies.front().ok_or("not found")?.clone()
                };
                return Ok(serde_json::to_vec(&reply)?);
            }
            assert_eq!(&args[2..4], ["graphql", "-f"]);
            if args[4].contains("dequeuePullRequest") {
                self.dequeued.borrow_mut().push(args[6].into());
                return Ok(br#"{"data":{"dequeuePullRequest":{"mergeQueueEntry":null}}}"#.to_vec());
            }
            if args[4].contains("enqueuePullRequest") {
                assert_eq!(args[5..8], ["-f", "id=PR_1", "-f"]);
                if let Some(error) = self.refusals.borrow_mut().pop_front() {
                    return Err(error.into());
                }
                self.queued.borrow_mut().push(args[8].into());
                return Ok(
                    br#"{"data":{"enqueuePullRequest":{"mergeQueueEntry":{"position":1}}}}"#
                        .to_vec(),
                );
            }
            assert!(args.contains(&"number=7") && args.contains(&"-F"));
            let mut looks = self.looks.borrow_mut();
            let next = if looks.len() > 1 {
                looks.pop_front().unwrap()
            } else {
                match looks.front().unwrap() {
                    Ok(value) => Ok(value.clone()),
                    Err(error) => Err(error.to_string().into()),
                }
            };
            Ok(serde_json::to_vec(
                &json!({"data":{"repository":{"pullRequest":next?}}}),
            )?)
        }
    }
    fn github(looks: Vec<Value>) -> GitHub {
        GitHub {
            looks: RefCell::new(looks.into_iter().map(Ok).collect()),
            ..Default::default()
        }
    }
    /// An open PR whose head the admission check passed and which merges cleanly.
    fn open(head: &str) -> Value {
        let mut value = unadmitted(head);
        value["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["contexts"]["nodes"][0]["conclusion"] =
            "SUCCESS".into();
        value
    }
    /// An open PR whose admission check has not reported yet.
    fn unadmitted(head: &str) -> Value {
        json!({"id":"PR_1","state":"OPEN","isDraft":false,"headRefOid":head,"mergeable":"MERGEABLE",
            "mergeCommit":null,"mergeQueueEntry":null,
            "commits":{"nodes":[{"commit":{"oid":head,"statusCheckRollup":{"contexts":{"nodes":[
                {"__typename":"CheckRun","name":REQUIRED,"conclusion":null}]}}}}]},
            "timelineItems":{"nodes":[]}})
    }
    fn in_queue(head: &str) -> Value {
        let mut value = open(head);
        value["mergeQueueEntry"] = json!({"state":"AWAITING_CHECKS","position":1});
        value
    }
    fn removed(head: &str, reason: &str) -> Value {
        let mut value = open(head);
        value["timelineItems"]["nodes"] = json!([{"reason":reason}]);
        value
    }
    fn merged(head: &str) -> Value {
        let mut value = open(head);
        value["state"] = "MERGED".into();
        value["mergeCommit"] = json!({"oid":"3333333"});
        value
    }
    fn ship(github: &GitHub) -> (Result<String>, Vec<String>, usize) {
        checked(github, None)
    }
    /// Ships with the rule checks here beside the queue, ended already with `rules`.
    fn checked(github: &GitHub, rules: Option<Result<()>>) -> (Result<String>, Vec<String>, usize) {
        let pauses = RefCell::new(0);
        let mut said = vec![];
        let result = std::thread::scope(|scope| {
            let mut local = rules.map(|rules| scope.spawn(|| rules));
            while local.as_ref().is_some_and(|local| !local.is_finished()) {
                std::thread::yield_now();
            }
            run(
                7,
                &mut local,
                github,
                &|seconds| {
                    assert_eq!(seconds, PAUSE);
                    *pauses.borrow_mut() += 1;
                },
                &mut |text| said.push(text.to_owned()),
            )
        });
        (result, said, pauses.into_inner())
    }
    const RUNS: &str = "actions/workflows/checks.yml/runs?event=merge_group&per_page=30";
    fn runs(ids: &[u64]) -> Value {
        let runs: Vec<_> = ids
            .iter()
            .map(|id| json!({"id":id,"head_branch":format!("gh-readonly-queue/main/pr-7-{id}")}))
            .chain([json!({"id":99,"head_branch":"gh-readonly-queue/main/pr-70-aaaa"})])
            .collect();
        json!({ "workflow_runs": runs })
    }
    fn jobs() -> Value {
        json!({"jobs":[
            {"name":"build","conclusion":"success","html_url":"https://github.com/job/1"},
            {"name":"core","conclusion":"failure","html_url":"https://github.com/job/2"},
            {"name":"platform","conclusion":"failure","html_url":"https://github.com/job/3"}]})
    }
    /// A queue run still going, whose only failure so far is one the gate allows.
    fn running() -> Value {
        json!({"jobs":[
            {"name":"build","conclusion":"success","html_url":"https://github.com/job/1"},
            {"name":"tools","conclusion":"failure","html_url":"https://github.com/job/4"},
            {"name":"browser (3)","conclusion":null,"html_url":"https://github.com/job/5"},
            {"name":"platform","conclusion":null,"html_url":"https://github.com/job/3"}]})
    }
    /// The same run once a browser shard's failing step reported the gate failed to the queue,
    /// before the shard's own job ended.
    fn reported() -> Value {
        let mut value = running();
        value["jobs"][3]["conclusion"] = "failure".into();
        value
    }
    /// The same run once a browser shard failed and the rest was cancelled.
    fn failing() -> Value {
        let mut value = running();
        value["jobs"][2]["conclusion"] = "failure".into();
        value["jobs"][3]["conclusion"] = "cancelled".into();
        value
    }

    #[test]
    fn queues_an_admitted_head_at_once_and_returns_the_merge() {
        let github = github(vec![
            unadmitted(OLD),
            unadmitted(OLD),
            open(OLD),
            in_queue(OLD),
            in_queue(OLD),
            merged(OLD),
        ]);
        let (result, said, _) = ship(&github);
        assert_eq!(result.unwrap(), "3333333");
        assert_eq!(*github.queued.borrow(), [format!("head={OLD}")]);
        // Each change of progress is reported once.
        assert_eq!(
            said,
            [
                "Waiting for the admission check on 1111111 of #7",
                "Added #7 to the merge queue at 1111111; the queue runs the checks on its squash commit",
                "#7 is in the merge queue at position 1: awaiting_checks",
            ]
        );
    }

    #[test]
    fn a_head_that_failed_its_branch_checks_is_not_queued_and_running_ones_give_way() {
        let branch = format!(
            "actions/workflows/checks.yml/runs?event=workflow_dispatch&head_sha={OLD}&per_page=20"
        );
        // Failed on the branch: it stops before costing a queue run, naming the failures.
        let failed = github(vec![open(OLD)]);
        failed.rest.borrow_mut().extend([
            (
                branch.clone(),
                VecDeque::from([json!({"workflow_runs":[
                    {"id":4,"status":"completed","conclusion":"success"},
                    {"id":5,"status":"completed","conclusion":"failure"}]})]),
            ),
            (
                "actions/runs/5/jobs?per_page=100".into(),
                VecDeque::from([jobs()]),
            ),
        ]);
        let error = ship(&failed).0.unwrap_err().to_string();
        assert!(
            error.starts_with("The checks on the branch failed on 1111111 of #7.")
                && error.contains("core: failure https://github.com/job/2"),
            "{error}"
        );
        assert!(failed.queued.borrow().is_empty());
        // Still running with nothing failed: queued, and the branch run cancelled.
        let running_run = github(vec![open(OLD), in_queue(OLD), merged(OLD)]);
        running_run.rest.borrow_mut().extend([
            (
                branch,
                VecDeque::from([
                    json!({"workflow_runs":[{"id":6,"status":"in_progress","conclusion":null}]}),
                ]),
            ),
            (
                "actions/runs/6/jobs?per_page=100".into(),
                VecDeque::from([running()]),
            ),
        ]);
        assert_eq!(ship(&running_run).0.unwrap(), "3333333");
        assert_eq!(*running_run.queued.borrow(), [format!("head={OLD}")]);
        assert_eq!(
            *running_run.cancelled.borrow(),
            [format!("repos/{REPOSITORY}/actions/runs/6/cancel")]
        );
    }

    /// A failed job's log as GitHub serves it: each line timed, the step that failed between
    /// the job's setup and a step that runs after a failure.
    const LOG: &str = "\u{feff}2026-10-20T05:24:14.0026243Z ##[group]Run actions/cache/restore@55cc834\n\
        2026-10-20T05:24:15.0443080Z Received 50078371 of 50078371 (100.0%), 59.5 MBs/sec\n\
        2026-10-20T05:24:41.9021338Z ##[group]Run npm run check:ci -- core\n\
        2026-10-20T05:24:41.9089623Z ##[endgroup]\n\
        2026-10-20T05:27:28.1000000Z test pay::rounds_to_the_cent ... FAILED\n\
        2026-10-20T05:27:28.2000000Z thread 'pay::rounds_to_the_cent' panicked at core/pay/tests/backend/pay.rs:12:5:\n\
        2026-10-20T05:27:28.3000000Z   left: 3\n\
        2026-10-20T05:27:28.4000000Z  right: 4\n\
        2026-10-20T05:27:29.5876193Z ##[error]Process completed with exit code 1.\n\
        2026-10-20T05:27:29.6013804Z ##[group]Run ./.github/actions/fail-queue-entry\n\
        2026-10-20T05:27:29.6100000Z Reported the platform check failed, so the merge queue takes the PR out now.\n";

    #[test]
    fn each_failed_job_is_reported_with_the_lines_of_its_failing_step() {
        let github = github(vec![open(OLD)]);
        github.rest.borrow_mut().extend([
            (
                format!(
                    "actions/workflows/checks.yml/runs?event=workflow_dispatch&head_sha={OLD}&per_page=20"
                ),
                VecDeque::from([
                    json!({"workflow_runs":[{"id":5,"status":"completed","conclusion":"failure"}]}),
                ]),
            ),
            (
                "actions/runs/5/jobs?per_page=100".into(),
                VecDeque::from([json!({"jobs":[
                    {"id":2,"name":"core","conclusion":"failure","html_url":"https://github.com/job/2"},
                    {"id":5,"name":"browser (3)","conclusion":"failure","html_url":"https://github.com/job/5"},
                    {"id":6,"name":"browser (4)","conclusion":"cancelled","html_url":"https://github.com/job/6"}]})]),
            ),
        ]);
        // The second job's log can't be read yet, and the cancelled one's says nothing of a
        // failure: their lines stay alone.
        github.logs.borrow_mut().extend([
            ("actions/jobs/2/logs".to_owned(), LOG),
            ("actions/jobs/6/logs".to_owned(), LOG),
        ]);
        let error = ship(&github).0.unwrap_err().to_string();
        assert_eq!(
            error.split_once("Failed jobs:\n").map(|(_, jobs)| jobs),
            Some(
                [
                    "- core: failure https://github.com/job/2",
                    "  test pay::rounds_to_the_cent ... FAILED",
                    "  thread 'pay::rounds_to_the_cent' panicked at core/pay/tests/backend/pay.rs:12:5:",
                    "  left: 3",
                    "  right: 4",
                    "- browser (3): failure https://github.com/job/5",
                    "- browser (4): cancelled https://github.com/job/6",
                ]
                .join("\n")
                .as_str()
            ),
            "{error}"
        );
    }

    #[test]
    fn unknown_mergeability_waits_and_a_conflict_or_failed_admission_stops_it() {
        let mut unknown = open(OLD);
        unknown["mergeable"] = "UNKNOWN".into();
        let github = github(vec![unknown, open(OLD), in_queue(OLD), merged(OLD)]);
        let (result, said, _) = ship(&github);
        assert_eq!(result.unwrap(), "3333333");
        assert_eq!(said[0], "Waiting for the admission check on 1111111 of #7");
        let mut conflicting = open(OLD);
        conflicting["mergeable"] = "CONFLICTING".into();
        let github = self::github(vec![conflicting]);
        let error = ship(&github).0.unwrap_err().to_string();
        assert!(error.contains("conflicts with main"), "{error}");
        assert!(github.queued.borrow().is_empty());
        let mut failed = open(OLD);
        failed["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["contexts"]["nodes"][0]["conclusion"] =
            "FAILURE".into();
        let github = self::github(vec![failed]);
        let error = ship(&github).0.unwrap_err().to_string();
        assert!(error.contains("admission check failed"), "{error}");
        // Checks attached to another commit than the head are not the head's.
        let mut stale = open(OLD);
        stale["headRefOid"] = NEW.into();
        assert_eq!(admission(&stale), Admission::Pending);
        assert_eq!(admission(&open(OLD)), Admission::Passed);
    }

    #[test]
    fn a_newer_push_is_queued_in_its_turn() {
        let github = github(vec![
            open(OLD),
            in_queue(OLD),
            open(NEW),
            in_queue(NEW),
            merged(NEW),
        ]);
        let (result, said, _) = ship(&github);
        assert_eq!(result.unwrap(), "3333333");
        assert_eq!(
            *github.queued.borrow(),
            [format!("head={OLD}"), format!("head={NEW}")]
        );
        assert!(said.iter().any(|text| text.contains("at 2222222")));
    }

    #[test]
    fn an_already_queued_pr_is_not_queued_again() {
        let github = github(vec![in_queue(OLD), merged(OLD)]);
        assert_eq!(ship(&github).0.unwrap(), "3333333");
        assert!(github.queued.borrow().is_empty());
    }

    #[test]
    fn leaving_the_queue_unmerged_reports_the_reason_and_the_failed_jobs_of_its_own_run() {
        // Queued here: only a run newer than the one before counts as this attempt's.
        let github = github(vec![
            open(OLD),
            in_queue(OLD),
            removed(OLD, "failed_checks"),
            removed(OLD, "failed_checks"),
        ]);
        github
            .rest
            .borrow_mut()
            .insert(RUNS.into(), VecDeque::from([runs(&[39]), runs(&[39, 40])]));
        github.rest.borrow_mut().insert(
            "actions/runs/40/jobs?per_page=100".into(),
            VecDeque::from([running(), jobs()]),
        );
        let error = ship(&github).0.unwrap_err().to_string();
        assert!(
            error.contains("#7 left the merge queue without merging: failed_checks"),
            "{error}"
        );
        assert!(
            error.contains("- core: failure https://github.com/job/2"),
            "{error}"
        );
        assert!(!error.contains("build"), "{error}");
        // Removed for another reason, the reason is the message and no stale run is blamed.
        let github = self::github(vec![
            open(OLD),
            in_queue(OLD),
            removed(OLD, "invalid_merge_commit"),
            removed(OLD, "invalid_merge_commit"),
        ]);
        github
            .rest
            .borrow_mut()
            .insert(RUNS.into(), VecDeque::from([runs(&[39])]));
        github.rest.borrow_mut().insert(
            "actions/runs/39/jobs?per_page=100".into(),
            VecDeque::from([jobs()]),
        );
        let error = ship(&github).0.unwrap_err().to_string();
        assert!(error.contains("invalid_merge_commit"), "{error}");
        assert!(!error.contains("core"), "{error}");
        assert!(error.contains("query=event%3Amerge_group"), "{error}");
        // Found already queued, the newest run is its own.
        let github = self::github(vec![
            in_queue(OLD),
            removed(OLD, "failed_checks"),
            removed(OLD, "failed_checks"),
        ]);
        github
            .rest
            .borrow_mut()
            .insert(RUNS.into(), VecDeque::from([runs(&[40])]));
        github.rest.borrow_mut().insert(
            "actions/runs/40/jobs?per_page=100".into(),
            VecDeque::from([running(), jobs()]),
        );
        let error = ship(&github).0.unwrap_err().to_string();
        assert!(error.contains("- core: failure"), "{error}");
        assert!(github.queued.borrow().is_empty());
        // One look outside it right after queueing is GitHub catching up, not a failure.
        let github = self::github(vec![open(OLD), open(OLD), in_queue(OLD), merged(OLD)]);
        assert_eq!(ship(&github).0.unwrap(), "3333333");
        assert_eq!(github.queued.borrow().len(), 1);
    }

    #[test]
    fn a_failed_job_stops_it_while_the_run_is_still_going() {
        let github = github(vec![open(OLD), in_queue(OLD), in_queue(OLD)]);
        github
            .rest
            .borrow_mut()
            .insert(RUNS.into(), VecDeque::from([runs(&[39]), runs(&[39, 40])]));
        github.rest.borrow_mut().insert(
            "actions/runs/40/jobs?per_page=100".into(),
            VecDeque::from([running(), reported(), failing()]),
        );
        let (result, said, _) = ship(&github);
        let error = result.unwrap_err().to_string();
        assert!(error.contains("#7 failed its queue run"), "{error}");
        // The gate's early report is not named; the shard that failed is, once its job ends.
        assert!(!error.contains("platform"), "{error}");
        assert!(
            error.contains("- browser (3): failure https://github.com/job/5"),
            "{error}"
        );
        // The gate allows the tools job to fail, and cancellations follow the failure.
        assert!(
            !error.contains("tools") && !error.contains("cancelled"),
            "{error}"
        );
        assert!(said.iter().any(|text| text.contains("in the merge queue")));
        // A failure the gate allows alone keeps it waiting for the merge.
        let github = self::github(vec![open(OLD), in_queue(OLD), merged(OLD)]);
        github
            .rest
            .borrow_mut()
            .insert(RUNS.into(), VecDeque::from([runs(&[39]), runs(&[39, 40])]));
        github.rest.borrow_mut().insert(
            "actions/runs/40/jobs?per_page=100".into(),
            VecDeque::from([running()]),
        );
        assert_eq!(ship(&github).0.unwrap(), "3333333");
        // An earlier attempt's failed run is not this one's.
        let github = self::github(vec![open(OLD), in_queue(OLD), merged(OLD)]);
        github
            .rest
            .borrow_mut()
            .insert(RUNS.into(), VecDeque::from([runs(&[40])]));
        github.rest.borrow_mut().insert(
            "actions/runs/40/jobs?per_page=100".into(),
            VecDeque::from([failing()]),
        );
        assert_eq!(ship(&github).0.unwrap(), "3333333");
    }

    #[test]
    fn a_failing_entry_found_in_the_queue_is_waited_out_and_the_head_queued_again() {
        // A fix pushed while the failed attempt is still queued goes in once it has left.
        let github = github(vec![
            in_queue(NEW),
            in_queue(NEW),
            open(NEW),
            in_queue(NEW),
            merged(NEW),
        ]);
        github
            .rest
            .borrow_mut()
            .insert(RUNS.into(), VecDeque::from([runs(&[40])]));
        github.rest.borrow_mut().insert(
            "actions/runs/40/jobs?per_page=100".into(),
            VecDeque::from([failing()]),
        );
        let (result, said, _) = ship(&github);
        assert_eq!(result.unwrap(), "3333333");
        assert_eq!(*github.queued.borrow(), [format!("head={NEW}")]);
        assert_eq!(
            said[0],
            "Waiting for GitHub to take #7 out of the queue after its failed run"
        );
    }

    #[test]
    fn closed_and_draft_prs_stop_at_once() {
        let mut closed = open(OLD);
        closed["state"] = "CLOSED".into();
        let error = ship(&github(vec![closed])).0.unwrap_err();
        assert_eq!(error.to_string(), "#7 was closed without merging");
        // The owner decides what becomes of a closed or draft PR.
        assert_eq!(error.downcast_ref::<Exit>().unwrap().code, OWNER);
        let mut draft = open(OLD);
        draft["isDraft"] = true.into();
        let github = github(vec![draft]);
        let error = ship(&github).0.unwrap_err();
        assert!(error.to_string().contains("is a draft"));
        assert_eq!(error.downcast_ref::<Exit>().unwrap().code, OWNER);
        assert!(github.queued.borrow().is_empty());
    }

    #[test]
    fn brief_api_failures_are_retried_and_lasting_ones_stop_it() {
        let github = GitHub {
            looks: RefCell::new(VecDeque::from([
                Ok(open(OLD)),
                Err("offline".into()),
                Err("offline".into()),
                Ok(in_queue(OLD)),
                Ok(merged(OLD)),
            ])),
            ..Default::default()
        };
        assert_eq!(ship(&github).0.unwrap(), "3333333");
        let github = GitHub {
            looks: RefCell::new(VecDeque::from([Ok(open(OLD)), Err("offline".into())])),
            ..Default::default()
        };
        let (result, _, pauses) = ship(&github);
        assert_eq!(result.unwrap_err().to_string(), "offline");
        assert_eq!(pauses, ATTEMPTS as usize);
        // A PR that cannot be read at all stops at once: a wrong number or missing access.
        let github = GitHub {
            looks: RefCell::new(VecDeque::from([Err("no such pull request".into())])),
            ..Default::default()
        };
        let (result, _, pauses) = ship(&github);
        assert_eq!(result.unwrap_err().to_string(), "no such pull request");
        assert_eq!(pauses, 0);
    }

    #[test]
    fn a_refused_addition_is_retried_and_a_lasting_refusal_stops_it() {
        let github = github(vec![open(OLD), open(OLD), in_queue(OLD), merged(OLD)]);
        github
            .refusals
            .borrow_mut()
            .push_back("mergeability unknown".into());
        assert_eq!(ship(&github).0.unwrap(), "3333333");
        assert_eq!(github.queued.borrow().len(), 1);
        let github = self::github(vec![open(OLD)]);
        github
            .refusals
            .borrow_mut()
            .extend((0..ATTEMPTS).map(|_| "Pull request is not mergeable".to_owned()));
        assert_eq!(
            ship(&github).0.unwrap_err().to_string(),
            "Pull request is not mergeable"
        );
    }

    #[test]
    fn the_rule_checks_failing_here_take_it_out_of_the_queue() {
        let failed = || -> Option<Result<()>> { Some(Err("The rule checks failed.".into())) };
        let github = github(vec![in_queue(OLD)]);
        let (result, said, _) = checked(&github, failed());
        assert_eq!(
            result.unwrap_err().to_string(),
            "The rule checks failed. Fix it, commit and ship it again; #7 was taken out of the \
             merge queue."
        );
        assert_eq!(*github.dequeued.borrow(), ["id=PR_1"]);
        assert!(said.is_empty());
        // Still waiting for its admission check, it is never queued.
        let github = self::github(vec![unadmitted(OLD)]);
        assert_eq!(
            checked(&github, failed()).0.unwrap_err().to_string(),
            "The rule checks failed. Fix it, commit and ship it again."
        );
        assert!(github.queued.borrow().is_empty() && github.dequeued.borrow().is_empty());
        // Passing, they change nothing, and a merge stands whatever they found.
        let github = self::github(vec![open(OLD), in_queue(OLD), merged(OLD)]);
        assert_eq!(checked(&github, Some(Ok(()))).0.unwrap(), "3333333");
        let github = self::github(vec![merged(OLD)]);
        assert_eq!(checked(&github, failed()).0.unwrap(), "3333333");
        assert!(github.dequeued.borrow().is_empty());
    }

    #[test]
    fn it_gives_up_after_ninety_minutes() {
        let (result, _, pauses) = ship(&github(vec![in_queue(OLD)]));
        assert_eq!(
            result.unwrap_err().to_string(),
            "Gave up after 90 minutes; #7 has not merged"
        );
        assert_eq!(pauses, LOOKS as usize);
        // So does a head the admission check never reports on.
        let (result, _, pauses) = ship(&github(vec![unadmitted(OLD)]));
        assert!(result.unwrap_err().to_string().contains("Gave up"));
        assert_eq!(pauses, LOOKS as usize);
    }
}
