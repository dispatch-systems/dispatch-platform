//! `dispatchdev status`: where the checkout's change stands, from branch to merge, and what
//! comes next: what is committed and pushed, its PR and required check, its tier and review,
//! and whether what it shows was approved.
use crate::{
    Result, Runner,
    merge::{self, Check, short},
    preview::{self, Approval},
    review::{self, Progress},
    workspace,
};
use std::path::Path;

pub fn run(root: &Path, runner: &dyn Runner) -> Result<()> {
    let git = |args: &[&str]| {
        let mut command = vec!["git"];
        command.extend_from_slice(args);
        workspace::text(runner, &command, Some(root))
    };
    let branch = git(&["branch", "--show-current"])?;
    if branch.is_empty() || branch == "main" {
        println!(
            "This checkout is on {}: a change has a branch of its own.",
            if branch.is_empty() {
                "no branch"
            } else {
                "main"
            }
        );
        println!(
            "{}",
            workspace::next(&["`dispatchdev start <name>` makes one".to_owned()])
        );
        return Ok(());
    }
    let head = git(&["rev-parse", "HEAD"])?;
    let dirty = !git(&["status", "--porcelain"])?.is_empty();
    let pushed = git(&[
        "rev-parse",
        "--verify",
        "-q",
        &format!("refs/remotes/origin/{branch}"),
    ])
    .unwrap_or_default()
        == head;
    println!(
        "{branch} at {}: {}",
        short(&head),
        match (dirty, pushed) {
            (true, _) => "uncommitted changes",
            (false, false) => "committed, not pushed",
            (false, true) => "committed and pushed",
        }
    );
    let judged = review::judged(root, runner)?;
    println!(
        "Tier: {}{}",
        judged.tier,
        match judged.reasons.is_empty() {
            true => String::new(),
            false => format!(" ({})", judged.reasons.join(", ")),
        }
    );
    let approval = preview::approval(root, runner)?;
    println!(
        "UI: {}",
        match &approval {
            Approval::None => "nothing a page shows".to_owned(),
            Approval::Approved => "approved".to_owned(),
            Approval::Missing => "not shown to the user yet".to_owned(),
            Approval::Changed(paths) => format!("changed since approved: {}", paths.join(", ")),
        }
    );
    let pull = merge::pull(root, &branch, runner)?;
    match &pull {
        None => println!("PR: none open"),
        Some(pull) => println!(
            "PR: #{} {}, {}: {}",
            pull.number,
            pull.url,
            merge::REQUIRED,
            match (&pull.required, pull.head == head) {
                (_, false) => "not on this commit yet",
                (Check::Passed, true) => "passed",
                (Check::Running, true) => "running",
                (Check::Failed, true) => "failed",
                (Check::Missing, true) => "not started",
            }
        ),
    }
    let progress = review::progress(root)?;
    println!(
        "Review: {}",
        match &progress {
            Progress::NotStarted => "not started".to_owned(),
            Progress::Prepared(pass) => format!("pass {pass} is with its reviewers"),
            Progress::Answering(pass) => format!("pass {pass}'s findings are being answered"),
            Progress::Recorded {
                pass, done: true, ..
            } => format!("done at pass {pass}"),
            Progress::Recorded { pass, .. } => format!("pass {pass} recorded; it asks for another"),
        }
    );
    let next = match (&pull, dirty, pushed) {
        (_, true, _) => "Commit it, then `dispatchdev check`".to_owned(),
        (None, _, _) if matches!(approval, Approval::Missing | Approval::Changed(_)) => {
            "Show the user what it looks like with `dispatchdev preview`; once they approve, \
             `dispatchdev preview approve`"
                .to_owned()
        }
        (None, _, _) | (_, _, false) => "`dispatchdev check`, then `dispatchdev pr`".to_owned(),
        (Some(pull), _, true) => match merge::unreviewed(root, &pull.body, &head, runner)? {
            Some(why) => why,
            None => match (&pull.required, pull.head == head) {
                (Check::Failed, true) => "`dispatchdev ci` says why its checks failed".to_owned(),
                (Check::Passed, true) => "`dispatchdev merge`".to_owned(),
                _ => format!("Its checks are running: {}/checks", pull.url),
            },
        },
    };
    println!("{}", workspace::next(&[next]));
    Ok(())
}
