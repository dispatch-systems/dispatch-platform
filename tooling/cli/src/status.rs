//! `dispatchdev status`: where the checkout's change stands, from branch to merge, and what
//! comes next: what is committed and pushed, its PR and required check, its tier and review,
//! and whether what it shows was approved. Checks that haven't ended say how long they have
//! gone on, or that GitHub never started them. `owed` reads the same state for every change.
use crate::{
    Result, Runner,
    change::Repository,
    ci::{self, Pending},
    merge::{self, Check, Pull, short},
    preview::{self, Approval},
    review::{self, Progress, tier::Judged},
    workspace,
};
use serde::Serialize;
use std::{path::Path, time::SystemTime};

/// Whose move the change waits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Whose {
    /// The agent making it has work to do.
    Agent,
    /// The user has to approve what it shows.
    Owner,
    /// GitHub has its checks to run.
    Github,
    /// Its reviewers have a pass to answer.
    Reviewers,
    /// Nothing stops it merging, which may wait on the user's word.
    Ready,
}

/// Where the change in a checkout stands.
pub enum State {
    /// The checkout is on `main`, or on no branch: a change has a branch of its own.
    Unbranched {
        main: bool,
    },
    Change(Box<Change>),
}
/// A change on its branch.
pub struct Change {
    pub branch: String,
    pub head: String,
    pub dirty: bool,
    pub pushed: bool,
    pub judged: Judged,
    pub approval: Approval,
    /// Its open PR.
    pub pull: Option<Pull>,
    /// How its checks are getting on, while its required one hasn't ended on this commit.
    pub pending: Option<Pending>,
    pub progress: Progress,
    /// What comes next, in order.
    pub next: Vec<String>,
    pub whose: Whose,
}
impl State {
    /// What comes next, in order.
    pub fn next(&self) -> Vec<String> {
        match self {
            Self::Unbranched { .. } => vec!["`dispatchdev start <name>` makes one".to_owned()],
            Self::Change(change) => change.next.clone(),
        }
    }
    pub fn whose(&self) -> Whose {
        match self {
            Self::Unbranched { .. } => Whose::Agent,
            Self::Change(change) => change.whose,
        }
    }
}

/// Where the change in the checkout at `root` stands at `now`.
pub fn state(root: &Path, now: SystemTime, runner: &dyn Runner) -> Result<State> {
    let git = |args: &[&str]| workspace::git(runner, root, args);
    let branch = git(&["branch", "--show-current"])?;
    if branch.is_empty() || branch == "main" {
        return Ok(State::Unbranched {
            main: !branch.is_empty(),
        });
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
    let judged = review::judged(root, runner)?;
    let approval = preview::approval(root, runner)?;
    let repository = Repository::of(root, runner)?;
    let pull = merge::pull(root, repository, &branch, runner)?;
    let pending = match &pull {
        Some(pull)
            if pull.head == head
                && matches!(pull.required, Some(Check::Running | Check::Missing)) =>
        {
            ci::pending(root, &branch, &head, now, runner)?
        }
        _ => None,
    };
    let progress = review::progress(root)?;
    let standing = Standing {
        branch: &branch,
        head: &head,
        dirty,
        pushed,
        // Nothing checks the agent docs before their PR, nor on it.
        checked: repository == Repository::Platform,
        approval: &approval,
        shown: preview::shown(root, runner)?,
        pull: pull.as_ref(),
        pending: pending.as_ref(),
        progress: &progress,
    };
    let (next, whose) = standing.next(|pull| merge::unreviewed(root, &pull.body, &head, runner))?;
    Ok(State::Change(Box::new(Change {
        branch,
        head,
        dirty,
        pushed,
        judged,
        approval,
        pull,
        pending,
        progress,
        next,
        whose,
    })))
}

/// What decides a change's next step.
struct Standing<'a> {
    branch: &'a str,
    head: &'a str,
    dirty: bool,
    pushed: bool,
    /// Whether its repository checks the branch before its PR and on it.
    checked: bool,
    approval: &'a Approval,
    /// Whether `preview` showed the UI files as committed now.
    shown: bool,
    pull: Option<&'a Pull>,
    pending: Option<&'a Pending>,
    progress: &'a Progress,
}
impl Standing<'_> {
    /// What comes next, in order, and whose move it is. `unreviewed` says why the review of its
    /// pushed PR doesn't let it merge, asked only once the rest lets it.
    fn next(
        &self,
        unreviewed: impl FnOnce(&Pull) -> Result<Option<String>>,
    ) -> Result<(Vec<String>, Whose)> {
        let checked = self.checked;
        let dropped = matches!(self.pending, Some(Pending::Dropped { .. }));
        let (next, whose) = match (self.pull, self.dirty, self.pushed) {
            (_, true, _) if checked => (
                "Commit it, then `dispatchdev check`".to_owned(),
                Whose::Agent,
            ),
            (_, true, _) => ("Commit it, then `dispatchdev pr`".to_owned(), Whose::Agent),
            (None, _, _) if matches!(self.approval, Approval::Missing | Approval::Changed(_)) => (
                "Show the user what it looks like with `dispatchdev preview`; once they approve, \
                 `dispatchdev preview approve`"
                    .to_owned(),
                // Shown, the user has it to answer; not yet, showing it is the agent's.
                if self.shown {
                    Whose::Owner
                } else {
                    Whose::Agent
                },
            ),
            (None, _, _) | (_, _, false) if checked => (
                "`dispatchdev check`, then `dispatchdev pr`".to_owned(),
                Whose::Agent,
            ),
            (None, _, _) | (_, _, false) => ("`dispatchdev pr`".to_owned(), Whose::Agent),
            (Some(pull), _, true) => {
                let current = pull.head == self.head;
                let failed = current && pull.required == Some(Check::Failed);
                match unreviewed(pull)? {
                    // What fails or never starts on GitHub is the agent's to see to, whoever
                    // has the review.
                    Some(why) => match self.progress {
                        Progress::Prepared(_) if !failed && !dropped => (why, Whose::Reviewers),
                        _ => (why, Whose::Agent),
                    },
                    None => match (&pull.required, current) {
                        (Some(Check::Failed), true) => (
                            "`dispatchdev ci` says why its checks failed".to_owned(),
                            Whose::Agent,
                        ),
                        (None | Some(Check::Passed), true) => {
                            ("`dispatchdev merge`".to_owned(), Whose::Ready)
                        }
                        (None, false) => (
                            "GitHub's PR head isn't this commit yet: wait a moment".to_owned(),
                            Whose::Github,
                        ),
                        (Some(_), _) if dropped => (
                            "`dispatchdev merge`, once its checks passed".to_owned(),
                            Whose::Agent,
                        ),
                        (Some(_), _) => (
                            format!("Its checks are running: {}/checks", pull.url),
                            Whose::Github,
                        ),
                    },
                }
            }
        };
        // A run GitHub dropped comes first: nothing that follows starts the checks.
        let restart = dropped.then(|| ci::restart(self.branch));
        Ok((restart.into_iter().chain([next]).collect(), whose))
    }
}

pub fn run(root: &Path, now: SystemTime, runner: &dyn Runner) -> Result<()> {
    let change = match state(root, now, runner)? {
        State::Unbranched { main } => {
            println!(
                "This checkout is on {}: a change has a branch of its own.",
                if main { "main" } else { "no branch" }
            );
            println!(
                "{}",
                workspace::next(&["`dispatchdev start <name>` makes one".to_owned()])
            );
            return Ok(());
        }
        State::Change(change) => change,
    };
    let Change {
        branch,
        head,
        dirty,
        pushed,
        judged,
        approval,
        pull,
        pending,
        progress,
        next,
        whose: _,
    } = *change;
    println!(
        "{branch} at {}: {}",
        short(&head),
        match (dirty, pushed) {
            (true, _) => "uncommitted changes",
            (false, false) => "committed, not pushed",
            (false, true) => "committed and pushed",
        }
    );
    println!(
        "Tier: {}{}",
        judged.tier,
        match judged.reasons.is_empty() {
            true => String::new(),
            false => format!(" ({})", judged.reasons.join(", ")),
        }
    );
    println!(
        "UI: {}",
        match &approval {
            Approval::None => "nothing a page shows".to_owned(),
            Approval::Approved => "approved".to_owned(),
            Approval::Missing => "not shown to the user yet".to_owned(),
            Approval::Changed(paths) => format!("changed since approved: {}", paths.join(", ")),
        }
    );
    if let Some(server) = preview::server(&workspace::checkout_scratch(root)?, runner) {
        println!("{}", server.line());
    }
    match &pull {
        None => println!("PR: none open"),
        Some(pull) => println!(
            "PR: #{} {}{}",
            pull.number,
            pull.url,
            match (&pull.required, pull.head == head) {
                (_, false) => ", not on this commit yet".to_owned(),
                (None, true) => String::new(),
                (Some(check), true) => format!(
                    ", {}: {}",
                    merge::REQUIRED,
                    match (check, &pending) {
                        (Check::Passed, _) => "passed".to_owned(),
                        (Check::Failed, _) => "failed".to_owned(),
                        (_, Some(Pending::Dropped { run, waited })) => format!(
                            "never started, GitHub has left run {run} queued without a job for {}",
                            workspace::duration(*waited)
                        ),
                        (_, Some(Pending::Running(waited))) =>
                            format!("running for {}", workspace::duration(*waited)),
                        (Check::Running, None) => "running".to_owned(),
                        (Check::Missing, None) => "not started".to_owned(),
                    }
                ),
            }
        ),
    }
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
    println!("{}", workspace::next(&next));
    Ok(())
}

#[cfg(test)]
mod tests;
