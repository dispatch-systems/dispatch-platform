use crate::{
    Result,
    config::Config,
    hermes::atomic_json,
    require,
    worktrees::{Worktree, command, is_sha},
};
use dispatch_core::support::api::types::SupportJob;
use serde_json::{Value, json};
use std::fs;

const LOOK: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){id state isDraft headRefOid mergeable mergeCommit{oid} mergeQueueEntry{state position} labels(first:20){nodes{name}} commits(last:1){nodes{commit{statusCheckRollup{contexts(first:100){nodes{__typename ...on CheckRun{name conclusion status} ...on StatusContext{context state}}}}}}} timelineItems(last:1,itemTypes:[REMOVED_FROM_MERGE_QUEUE_EVENT]){nodes{...on RemovedFromMergeQueueEvent{reason}}}}}}";

/// Redact known identities before any public GitHub mutation, including case variations.
pub fn redact_identities(summary: &str, identities: &[&str]) -> String {
    let mut text = summary.to_owned();
    for identity in identities.iter().filter(|name| !name.trim().is_empty()) {
        let needle = identity.to_lowercase();
        let mut output = String::new();
        let mut offset = 0;
        while offset < text.len() {
            let mut folded = String::new();
            let mut end = offset;
            for (position, character) in text[offset..].char_indices() {
                folded.extend(character.to_lowercase());
                end = offset + position + character.len_utf8();
                if folded.len() >= needle.len() {
                    break;
                }
            }
            if folded == needle {
                output.push_str("the affected account");
                offset = end;
            } else {
                let character = text[offset..].chars().next().expect("valid UTF-8 boundary");
                output.push(character);
                offset += character.len_utf8();
            }
        }
        text = output;
    }
    text
}

pub async fn look(config: &Config, tree: &Worktree, number: u64) -> Result<Value> {
    let (owner, name) = config
        .repository
        .split_once('/')
        .ok_or("Invalid repository")?;
    let output = command(
        &tree.path,
        "gh",
        &[
            "api",
            "graphql",
            "-f",
            &format!("query={LOOK}"),
            "-f",
            &format!("owner={owner}"),
            "-f",
            &format!("name={name}"),
            "-F",
            &format!("number={number}"),
        ],
    )
    .await?;
    let result: Value = serde_json::from_str(&output)?;
    require(result["errors"].is_null(), "GitHub status query failed")?;
    let pr = result["data"]["repository"]["pullRequest"].clone();
    require(pr.is_object(), "Support PR is unavailable")?;
    Ok(pr)
}

pub async fn publish(
    config: &Config,
    job: &SupportJob,
    tree: &Worktree,
    summary: &str,
    checks: &[String],
) -> Result<(u64, String)> {
    let summary = redact_identities(
        summary,
        &[
            &job.detail.ticket.reporter_name,
            job.detail.ticket.dsp_name.as_deref().unwrap_or(""),
        ],
    );
    let title = format!(
        "{}: implement support ticket {}",
        if job.detail.ticket.kind == "bug" {
            "fix"
        } else {
            "feat"
        },
        job.detail
            .ticket
            .id
            .strip_prefix("support_")
            .unwrap_or(&job.detail.ticket.id)
    );
    let kind = if job.detail.ticket.kind == "bug" {
        "bug fix"
    } else {
        "feature"
    };
    let verification = checks
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    // Reports, reporter identity, credentials and private screenshots stay in the platform.
    let body = if kind == "bug fix" {
        format!(
            "## Summary\n\n{summary}\n\n## Symptom\n\nThe approved support proposal identified behavior requiring a fix.\n\n## Cause\n\nSee the scoped implementation and regression test in this patch.\n\n## Fix\n\nImplements the exact owner-approved proposal version {}.\n\n## Verification\n\nAn independent agent exercised every acceptance criterion in a real browser on this exact head.\n\n{verification}\n\n## Review\n\nIndependent browser review completed. CodeRabbit omitted under the owner’s support workflow policy. Screenshots and traces remain in the private AI Support ticket to preserve reporter and DSP privacy.\n",
            job.version
        )
    } else {
        format!(
            "## Summary\n\n{summary}\n\n## Problem\n\nThe owner approved a support proposal for this feature.\n\n## Change\n\nImplements the exact approved proposal version {}.\n\n## Rollout\n\nThe merge reaches Dev after checks; production requires a separately published release.\n\n## Verification\n\nAn independent agent exercised every acceptance criterion in a real browser on this exact head.\n\n{verification}\n\n## Review\n\nIndependent browser review completed. CodeRabbit omitted under the owner’s support workflow policy. Screenshots and traces remain in the private AI Support ticket to preserve reporter and DSP privacy.\n",
            job.version
        )
    };
    let scratch = std::path::PathBuf::from(format!("/tmp/dispatch-{}", tree.name));
    fs::create_dir_all(&scratch)?;
    let file = scratch.join("pr-body.md");
    fs::write(&file, body)?;
    command(
        &tree.path,
        "dispatchdev",
        &[
            "pr",
            &tree.name,
            "--title",
            &title,
            "--body",
            file.to_str().ok_or("Non UTF8 PR body path")?,
        ],
    )
    .await?;
    let result = command(
        &tree.path,
        "gh",
        &[
            "pr",
            "view",
            "--repo",
            &config.repository,
            "--json",
            "number,url,headRefOid",
        ],
    )
    .await?;
    let result: Value = serde_json::from_str(&result)?;
    let number = result["number"].as_u64().ok_or("PR number missing")?;
    let url = result["url"].as_str().ok_or("PR URL missing")?.to_owned();
    let head = tree.head().await?;
    require(
        result["headRefOid"] == head,
        "Published PR does not match verified head",
    )?;
    Ok((number, url))
}

pub async fn register(
    config: &Config,
    job: &SupportJob,
    tree: &Worktree,
    number: u64,
    url: &str,
) -> Result<()> {
    let head = tree.head().await?;
    let registrations = config.runtime_root.join("github/registrations");
    fs::create_dir_all(&registrations)?;
    atomic_json(
        &registrations.join(format!("{}-{}.json", job.environment, job.detail.ticket.id)),
        &json!({"sourceOrigin":job.origin,"ticketId":job.detail.ticket.id,"proposalVersion":job.version,"branch":tree.name,"prNumber":number,"prUrl":url,"headSha":head}),
    )?;
    Ok(())
}

/// Withdraw only this ticket's exact registered head when its lease/approval is lost.
pub async fn cancel(config: &Config, job: &SupportJob, checkpoint: &Value) -> Result<()> {
    let Some(number) = checkpoint["prNumber"].as_u64() else {
        return Ok(());
    };
    let head = checkpoint["headSha"]
        .as_str()
        .ok_or("Cancellation head missing")?;
    let registration = config
        .runtime_root
        .join("github/registrations")
        .join(format!("{}-{}.json", job.environment, job.detail.ticket.id));
    let value: Value = serde_json::from_slice(&fs::read(registration)?)?;
    require(
        value["ticketId"] == job.detail.ticket.id
            && value["proposalVersion"] == job.version
            && value["prNumber"] == number
            && value["headSha"] == head,
        "Cancellation registration mismatch",
    )?;
    let name = value["branch"]
        .as_str()
        .ok_or("Cancellation branch missing")?
        .to_owned();
    let tree = Worktree {
        path: config.workspace_root.join("worktrees").join(&name),
        name,
    };
    let status = look(config, &tree, number).await?;
    require(
        status["headRefOid"] == head,
        "Cancellation refuses a PR changed outside this ticket",
    )?;
    if status["state"] == "MERGED" {
        return Err(
            "Approval changed after GitHub already confirmed merge; owner review is required"
                .into(),
        );
    }
    if status["state"] == "CLOSED" {
        return Ok(());
    }
    if status["mergeQueueEntry"].is_object() {
        let id = status["id"].as_str().ok_or("PR node id missing")?;
        let query = "mutation($id:ID!){dequeuePullRequest(input:{id:$id}){clientMutationId}}";
        command(
            &tree.path,
            "gh",
            &[
                "api",
                "graphql",
                "-f",
                &format!("query={query}"),
                "-f",
                &format!("id={id}"),
            ],
        )
        .await?;
    }
    if status["isDraft"] != true {
        command(
            &tree.path,
            "gh",
            &[
                "pr",
                "ready",
                &number.to_string(),
                "--undo",
                "--repo",
                &config.repository,
            ],
        )
        .await?;
    }
    Ok(())
}

pub async fn ready(config: &Config, tree: &Worktree, number: u64) -> Result<()> {
    command(
        &tree.path,
        "gh",
        &[
            "pr",
            "ready",
            &number.to_string(),
            "--repo",
            &config.repository,
        ],
    )
    .await?;
    Ok(())
}

pub async fn enqueue(tree: &Worktree, status: &Value, head: &str) -> Result<()> {
    require(
        is_sha(head) && status["headRefOid"] == head,
        "Cannot queue a different or unverified head",
    )?;
    require(
        status["state"] == "OPEN"
            && status["isDraft"] == false
            && status["mergeable"] == "MERGEABLE",
        "PR is not ready for the merge queue",
    )?;
    let no_review = status["labels"]["nodes"]
        .as_array()
        .is_none_or(|labels| labels.iter().all(|label| label["name"] != "ai-review"));
    require(
        no_review,
        "Owner review policy changed; CodeRabbit review needs an owner decision",
    )?;
    let query = "mutation($id:ID!,$head:GitObjectID!){enqueuePullRequest(input:{pullRequestId:$id,expectedHeadOid:$head}){mergeQueueEntry{position}}}";
    let id = status["id"].as_str().ok_or("PR node id missing")?;
    let output = command(
        &tree.path,
        "gh",
        &[
            "api",
            "graphql",
            "-f",
            &format!("query={query}"),
            "-f",
            &format!("id={id}"),
            "-f",
            &format!("head={head}"),
        ],
    )
    .await?;
    let result: Value = serde_json::from_str(&output)?;
    require(
        result["errors"].is_null(),
        "GitHub did not admit the verified support PR",
    )
}

pub fn admitted(status: &Value) -> bool {
    status["commits"]["nodes"]
        .as_array()
        .is_some_and(|commits| {
            commits.iter().any(|commit| {
                commit["commit"]["statusCheckRollup"]["contexts"]["nodes"]
                    .as_array()
                    .is_some_and(|checks| {
                        checks.iter().any(|check| {
                            (check["name"] == "platform" && check["conclusion"] == "SUCCESS")
                                || (check["context"] == "platform" && check["state"] == "SUCCESS")
                        })
                    })
            })
        })
}

/// Where the PR's newest queue run stands.
pub enum QueueRun {
    /// None yet, or one that ended without failing.
    Clear,
    /// Still going. GitHub takes a PR out on its run's first failure, so a PR out of the queue
    /// may have one still failing.
    Going,
    /// Failed: the run, and the end of its failed jobs' logs.
    Failed(u64, String),
}

pub async fn queue_run(config: &Config, tree: &Worktree, number: u64) -> Result<QueueRun> {
    let output = command(
        &tree.path,
        "gh",
        &[
            "run",
            "list",
            "--repo",
            &config.repository,
            "--event",
            "merge_group",
            "--limit",
            "100",
            "--json",
            "databaseId,headBranch,status,conclusion",
        ],
    )
    .await?;
    let runs: Vec<Value> = serde_json::from_str(&output)?;
    let prefix = format!("gh-readonly-queue/main/pr-{number}-");
    let Some(run) = runs.iter().find(|run| {
        run["headBranch"]
            .as_str()
            .is_some_and(|branch| branch.starts_with(&prefix))
    }) else {
        return Ok(QueueRun::Clear);
    };
    if run["status"] != "completed" {
        return Ok(QueueRun::Going);
    }
    if run["conclusion"] == "failure" {
        let id = run["databaseId"].as_u64().ok_or("Queue run id missing")?;
        let logs = command(
            &tree.path,
            "gh",
            &[
                "run",
                "view",
                &id.to_string(),
                "--repo",
                &config.repository,
                "--log-failed",
            ],
        )
        .await?;
        return Ok(QueueRun::Failed(
            id,
            logs.chars()
                .rev()
                .take(100_000)
                .collect::<String>()
                .chars()
                .rev()
                .collect(),
        ));
    }
    Ok(QueueRun::Clear)
}
