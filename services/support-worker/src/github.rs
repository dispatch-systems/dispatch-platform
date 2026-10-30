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

const LOOK: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){id state isDraft headRefOid mergeable mergeCommit{oid} autoMergeRequest{enabledAt}}}}";

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
            "## Summary\n\n{summary}\n\n## Symptom\n\nThe approved support proposal identified behavior requiring a fix.\n\n## Cause\n\nSee the scoped implementation and regression test in this patch.\n\n## Fix\n\nImplements the exact owner-approved proposal version {}.\n\n## Verification\n\nAn independent agent exercised every acceptance criterion in a real browser on this exact head.\n\n{verification}\n\n## Review\n\nIndependent browser review completed. Code review omitted under the owner’s support workflow policy. Screenshots and traces remain in the private AI Support ticket to preserve reporter and DSP privacy.\n",
            job.version
        )
    } else {
        format!(
            "## Summary\n\n{summary}\n\n## Problem\n\nThe owner approved a support proposal for this feature.\n\n## Change\n\nImplements the exact approved proposal version {}.\n\n## Rollout\n\nThe merge reaches Dev after checks; production requires a separately published release.\n\n## Verification\n\nAn independent agent exercised every acceptance criterion in a real browser on this exact head.\n\n{verification}\n\n## Review\n\nIndependent browser review completed. Code review omitted under the owner’s support workflow policy. Screenshots and traces remain in the private AI Support ticket to preserve reporter and DSP privacy.\n",
            job.version
        )
    };
    let scratch = std::path::PathBuf::from(format!("/tmp/dispatch-{}", tree.name));
    fs::create_dir_all(&scratch)?;
    let file = scratch.join("pr-body.md");
    fs::write(&file, body)?;
    let file = file.to_str().ok_or("Non UTF8 PR body path")?;
    // The branch's PR: opened the first time, its title and body kept current after a correction.
    command(
        &tree.path,
        "git",
        &[
            "push",
            "-q",
            "--force-with-lease",
            "-u",
            "origin",
            &tree.name,
        ],
    )
    .await?;
    let repository = &config.repository;
    let open = command(
        &tree.path,
        "gh",
        &[
            "pr",
            "list",
            "--repo",
            repository,
            "--head",
            &tree.name,
            "--state",
            "open",
            "--json",
            "number",
            "--jq",
            ".[0].number // empty",
        ],
    )
    .await?;
    if open.is_empty() {
        command(
            &tree.path,
            "gh",
            &[
                "pr",
                "create",
                "--repo",
                repository,
                "--base",
                "main",
                "--head",
                &tree.name,
                "--title",
                &title,
                "--body-file",
                file,
            ],
        )
        .await?;
    } else {
        command(
            &tree.path,
            "gh",
            &[
                "pr",
                "edit",
                &open,
                "--repo",
                repository,
                "--title",
                &title,
                "--body-file",
                file,
            ],
        )
        .await?;
    }
    let result = command(
        &tree.path,
        "gh",
        &[
            "pr",
            "view",
            &tree.name,
            "--repo",
            repository,
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
        path: config.worktrees.join(&name),
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
    hold(&tree, &status).await?;
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

/// Sets the verified head to squash-merge once the PR's required checks pass.
pub async fn merge_when_checks_pass(tree: &Worktree, status: &Value, head: &str) -> Result<()> {
    require(
        is_sha(head) && status["headRefOid"] == head,
        "Cannot merge a different or unverified head",
    )?;
    require(
        status["state"] == "OPEN"
            && status["isDraft"] == false
            && status["mergeable"] == "MERGEABLE",
        "PR is not ready to merge",
    )?;
    let query = "mutation($id:ID!,$head:GitObjectID!){enablePullRequestAutoMerge(input:{pullRequestId:$id,expectedHeadOid:$head,mergeMethod:SQUASH}){pullRequest{number}}}";
    let id = status["id"].as_str().ok_or("PR node id missing")?;
    let result = graphql(tree, query, &[("id", id), ("head", head)]).await?;
    require(
        result["errors"].is_null(),
        "GitHub did not set the verified support PR to merge",
    )
}

/// Takes the PR's merge back, so nothing merges until the worker sets it again.
pub async fn hold(tree: &Worktree, status: &Value) -> Result<()> {
    if status["autoMergeRequest"].is_null() {
        return Ok(());
    }
    let query = "mutation($id:ID!){disablePullRequestAutoMerge(input:{pullRequestId:$id}){clientMutationId}}";
    let id = status["id"].as_str().ok_or("PR node id missing")?;
    let result = graphql(tree, query, &[("id", id)]).await?;
    require(
        result["errors"].is_null(),
        "GitHub did not take back the support PR's merge",
    )
}

async fn graphql(tree: &Worktree, query: &str, variables: &[(&str, &str)]) -> Result<Value> {
    let mut args = vec![
        "api".to_owned(),
        "graphql".to_owned(),
        "-f".to_owned(),
        format!("query={query}"),
    ];
    for (name, value) in variables {
        args.extend(["-f".to_owned(), format!("{name}={value}")]);
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    Ok(serde_json::from_str(
        &command(&tree.path, "gh", &args).await?,
    )?)
}

/// The newest run of the checks on the PR's `head`, if it failed: the run, and the end of its
/// failed jobs' logs. None while there is none, it runs, or it passed.
pub async fn failed_checks(
    config: &Config,
    tree: &Worktree,
    head: &str,
) -> Result<Option<(u64, String)>> {
    let output = command(
        &tree.path,
        "gh",
        &[
            "run",
            "list",
            "--repo",
            &config.repository,
            "--workflow",
            "checks.yml",
            "--event",
            "pull_request",
            "--commit",
            head,
            "--limit",
            "1",
            "--json",
            "databaseId,status,conclusion",
        ],
    )
    .await?;
    let runs: Vec<Value> = serde_json::from_str(&output)?;
    let Some(run) = runs
        .first()
        .filter(|run| run["status"] == "completed" && run["conclusion"] == "failure")
    else {
        return Ok(None);
    };
    let id = run["databaseId"].as_u64().ok_or("Checks run id missing")?;
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
    Ok(Some((
        id,
        logs.chars()
            .rev()
            .take(100_000)
            .collect::<String>()
            .chars()
            .rev()
            .collect(),
    )))
}
