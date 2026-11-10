//! `dispatchdev merge`: the checkout's PR, squash-merged once nothing stops it. Its required
//! check passed on the pushed head, the body on GitHub is the one written, and its review is
//! done at that head, or at one that head only merges `main` into, or says why it had none.
//! Then it prints what to tell the user.
use crate::{REPOSITORY, Result, Runner, pr, require, review, workspace};
use serde_json::Value;
use std::path::Path;

/// The one check a PR needs to pass to merge.
pub const REQUIRED: &str = "platform";

/// A required check's state on a PR's head.
#[derive(Debug, PartialEq, Eq)]
pub enum Check {
    Passed,
    Running,
    Failed,
    /// It hasn't started on this head.
    Missing,
}
/// The open PR of a branch, as GitHub has it.
pub struct Pull {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub body: String,
    pub head: String,
    pub required: Check,
}
/// The branch's open PR, if it has one.
pub fn pull(root: &Path, branch: &str, runner: &dyn Runner) -> Result<Option<Pull>> {
    let text = workspace::text(
        runner,
        &[
            "gh",
            "pr",
            "list",
            "--repo",
            REPOSITORY,
            "--head",
            branch,
            "--state",
            "open",
            "--json",
            "number,url,title,body,headRefOid,statusCheckRollup",
        ],
        Some(root),
    )?;
    parsed(&text)
}
/// The first PR of `gh pr list --json` output.
fn parsed(text: &str) -> Result<Option<Pull>> {
    let pulls: Vec<Value> = serde_json::from_str(text)?;
    let Some(found) = pulls.first() else {
        return Ok(None);
    };
    let text = |key: &str| found[key].as_str().unwrap_or_default().to_owned();
    let required = found["statusCheckRollup"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|check| check["name"] == REQUIRED)
        .map_or(Check::Missing, |check| {
            match (check["status"].as_str(), check["conclusion"].as_str()) {
                (Some("COMPLETED"), Some("SUCCESS")) => Check::Passed,
                (Some("COMPLETED"), _) => Check::Failed,
                _ => Check::Running,
            }
        });
    Ok(Some(Pull {
        number: found["number"].as_u64().ok_or("A PR without a number")?,
        url: text("url"),
        title: text("title"),
        body: text("body"),
        head: text("headRefOid"),
        required,
    }))
}

/// Why the review of the change at `head` doesn't let it merge, if anything: from the Review
/// line its body opens with.
pub fn unreviewed(
    root: &Path,
    body: &str,
    head: &str,
    runner: &dyn Runner,
) -> Result<Option<String>> {
    let line = pr::section(body, "Review")
        .and_then(|review| review.lines().next())
        .unwrap_or_default();
    if line.starts_with("Not reviewed:") {
        return Ok(None);
    }
    if line == "Tier light · not reviewed" {
        let judged = review::judged(root, runner)?;
        return Ok((judged.tier != review::tier::Tier::Light).then(|| {
            format!(
                "The change is {} now, not light: `dispatchdev review prepare --pass 1`, or say \
                 `Not reviewed:` and why.",
                judged.tier
            )
        }));
    }
    Ok(match review::progress(root)? {
        review::Progress::Recorded {
            done: true,
            head: at,
            ..
        } if !at.is_empty()
            && review::later(root, &at, head, runner)? == review::tier::Later::Nothing =>
        {
            None
        }
        review::Progress::Recorded {
            done: true, pass, ..
        } => Some(format!(
            "Pass {pass} was recorded before the latest commits: `dispatchdev review prepare \
             --pass {}` reviews them.",
            pass + 1
        )),
        review::Progress::Recorded { pass, .. } => Some(format!(
            "Pass {pass} asks for another: `dispatchdev review prepare --pass {}`.",
            pass + 1
        )),
        review::Progress::Answering(pass) => Some(format!(
            "Pass {pass}'s findings aren't answered: `dispatchdev review take --pass {pass}`, \
             then `review record`."
        )),
        review::Progress::Prepared(pass) => Some(format!(
            "Pass {pass}'s reviewers haven't answered: wait for them, then `dispatchdev review \
             take --pass {pass}`."
        )),
        review::Progress::NotStarted => Some(
            "It isn't reviewed: `dispatchdev review prepare --pass 1`, or say `Not reviewed:` \
             and why."
                .to_owned(),
        ),
    })
}

/// `dispatchdev merge`.
pub fn run(root: &Path, runner: &dyn Runner) -> Result<()> {
    let (branch, head) = review::pushed(root, runner)?;
    let pull = pull(root, &branch, runner)?
        .ok_or("The branch has no open PR: `dispatchdev pr` opens it.")?;
    require(
        pull.head == head,
        "GitHub's PR head isn't this commit yet: wait a moment, or `dispatchdev pr`.",
    )?;
    let written = std::fs::read_to_string(workspace::checkout_scratch(root)?.join("pr-body.md"))?;
    let (title, body) = pr::titled(&written);
    require(
        title == Some(pull.title.as_str()) && body.trim() == pull.body.trim(),
        "The PR's title or body on GitHub isn't the one in pr-body.md: `dispatchdev pr` updates it.",
    )?;
    match pull.required {
        Check::Passed => {}
        Check::Running | Check::Missing => {
            return Err(format!(
                "`{REQUIRED}` hasn't finished on {}: wait for it.",
                short(&head)
            )
            .into());
        }
        Check::Failed => {
            return Err(format!(
                "`{REQUIRED}` failed on {}: `dispatchdev ci` says why.",
                short(&head)
            )
            .into());
        }
    }
    if let Some(why) = unreviewed(root, body, &head, runner)? {
        return Err(why.into());
    }
    workspace::text(
        runner,
        &[
            "gh",
            "pr",
            "merge",
            &pull.number.to_string(),
            "--repo",
            REPOSITORY,
            "--squash",
            "--match-head-commit",
            &head,
        ],
        Some(root),
    )?;
    println!("**Merged: {}** (#{})", pull.title, pull.number);
    if let Some(line) = pr::section(body, "Review").and_then(|review| review.lines().next()) {
        println!("Reviewed: {line}");
    }
    for outcome in review::left(root)? {
        println!("{}: {}", outcome.tag, outcome.what);
    }
    println!(
        "{}",
        workspace::next(&[
            "Tell the user what merged; Dev installs it once main's run publishes its build"
                .to_owned(),
            format!(
                "From outside the worktree, `dispatchdev finish {branch} --plan`, then \
                 `dispatchdev finish {branch}`"
            ),
        ])
    );
    Ok(())
}

pub fn short(sha: &str) -> &str {
    &sha[..sha.len().min(10)]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_required_check_is_read_from_the_heads_rollup() {
        let pull = |checks: &str| {
            parsed(&format!(
                r#"[{{"number":833,"url":"u","title":"t","body":"b","headRefOid":"abc",
                    "statusCheckRollup":[{{"name":"changes","status":"COMPLETED","conclusion":"FAILURE"}}{checks}]}}]"#
            ))
            .unwrap()
            .unwrap()
            .required
        };
        assert_eq!(
            pull(""),
            Check::Missing,
            "another job failing isn't the gate"
        );
        assert_eq!(
            pull(r#",{"name":"platform","status":"IN_PROGRESS","conclusion":""}"#),
            Check::Running
        );
        assert_eq!(
            pull(r#",{"name":"platform","status":"COMPLETED","conclusion":"SUCCESS"}"#),
            Check::Passed
        );
        assert_eq!(
            pull(r#",{"name":"platform","status":"COMPLETED","conclusion":"CANCELLED"}"#),
            Check::Failed
        );
        assert!(parsed("[]").unwrap().is_none());
    }
}
