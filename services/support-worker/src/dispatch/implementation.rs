use crate::{
    Result,
    checkpoint::Checkpoint,
    collection_verification,
    config::Config,
    github,
    hermes::{self, rules},
    jobs::Platform,
    preview::NativePreview,
    process::command,
    require, verification,
    worktrees::{Worktree, is_sha},
};
use dispatch_core::support::api::types::{SupportJob, SupportProposal};
use serde_json::json;

pub(super) async fn run(
    config: &Config,
    platform: &Platform,
    job: &SupportJob,
    checkpoint: &Checkpoint,
) -> Result<()> {
    require(
        job.detail.ticket.approved_version == Some(job.version),
        "Implementation requires the exact approved proposal",
    )?;
    let proposal = super::approved_proposal(job)?;
    let tree = Worktree::prepare(config, job, None).await?;
    let mut failure = checkpoint.failure();
    'correction: loop {
        let current = checkpoint.published_pr();
        // `queued` is the platform's status for a published PR waiting to merge.
        let merging = current.queued
            && current.head_sha.as_deref() == Some(tree.head().await?.as_str())
            && current.number.is_some();
        let (number, pr_url, head) = if merging {
            (
                current.number.ok_or("PR number missing")?,
                current.url.ok_or("PR URL missing")?,
                tree.head().await?,
            )
        } else {
            checkpoint
                .progress(
                    platform,
                    job,
                    "implementing",
                    "Implementing the approved proposal",
                    json!({"phase":"build"}),
                )
                .await?;
            let summary;
            let checks;
            loop {
                let initial = match NativePreview::start(config, &tree, None).await {
                    Ok(preview) => Some(preview),
                    Err(error) => {
                        failure = format!(
                            "Current branch cannot be built; repair it before verification:\n{error}\n{failure}"
                        );
                        None
                    }
                };
                let mut context = json!({"rules":rules(config)?,"proposal":proposal,"approvedVersion":job.version,"failure":failure,"headSha":tree.head().await?});
                if let Some(initial) = &initial {
                    context["browser"] = initial.browser();
                }
                let built = hermes::run(config, job, &tree, "builder", "Implement the approved proposal completely or fix the supplied failure. Return the approved-implementation JSON contract. Do not change approval scope. The host runs checks, commits, then independent browser verification.", context).await?;
                drop(initial);
                if built["requiresNewApproval"] == true {
                    let mut revised: SupportProposal =
                        serde_json::from_value(built["revisedProposal"].clone())?;
                    revised.source_sha = job.detail.source_sha.clone();
                    require(
                        !revised.acceptance_criteria.is_empty()
                            && !revised.production_verification.is_empty(),
                        "Scope escalation requires a complete revised proposal",
                    )?;
                    platform
                        .update(job, "proposal", json!({"proposal":revised}))
                        .await?;
                    return Ok(());
                }
                let head = tree.commit(job).await?;
                match tree.check(config).await {
                    Ok(_) => {}
                    Err(error) => {
                        failure = format!("Required local check failed:\n{error}");
                        if failure.contains("origin/main has advanced")
                            && let Err(conflict) = tree.reconcile(config).await?
                        {
                            failure.push_str(&format!("\nReconcile main conflict: {conflict}"));
                        }
                        continue;
                    }
                }
                checkpoint
                    .progress(
                        platform,
                        job,
                        "verifying",
                        "An independent agent is testing the committed change in a real browser",
                        json!({"phase":"verification","headSha":head}),
                    )
                    .await?;
                let collections =
                    collection_verification::collect(config, &tree, job, proposal).await?;
                for proof in &collections.proofs {
                    platform
                        .update(job, "evidence", json!({"evidence":proof}))
                        .await?;
                }
                // Every preview executes the committed artifact with no host network or secrets.
                let native =
                    NativePreview::start(config, &tree, collections.state.as_ref()).await?;
                let context = json!({"rules":rules(config)?,"proposal":proposal,"acceptanceCriteria":proposal.acceptance_criteria,"headSha":head,"collectionResults":collections.summaries,"browser":native.browser()});
                let reviewed = hermes::run(config, job, &tree, "reviewer", "Independently verify every criterion in the actual changed application. Use real browser interactions and dispatch_browser assert for each exact criterion. Inspect persisted outcomes after refresh and relevant roles/layouts. Return the independent-verification JSON contract.", context).await?;
                if reviewed["passed"] != true {
                    failure = format!(
                        "Independent browser verification failed:\n{}",
                        reviewed["findings"]
                    );
                    checkpoint
                        .progress(
                            platform,
                            job,
                            "implementing",
                            "Fixing issues found by independent browser verification",
                            json!({"phase":"build","failure":failure}),
                        )
                        .await?;
                    continue;
                }
                if let Err(error) = verification::record(
                    config,
                    platform,
                    job,
                    &proposal.acceptance_criteria,
                    &head,
                    "verification",
                )
                .await
                {
                    failure = format!("Browser proof incomplete: {error}");
                    continue;
                }
                require(
                    tree.head().await? == head
                        && command(&tree.path, "git", &["status", "--porcelain"])
                            .await?
                            .is_empty(),
                    "Verified source changed during review",
                )?;
                summary = built["summary"]
                    .as_str()
                    .unwrap_or("Implement the approved support proposal.")
                    .replace(['\n', '\r'], " ");
                checks = [
                    vec![
                        "dispatchdev check passed".to_owned(),
                        "Independent Chromium verification passed every approved criterion"
                            .to_owned(),
                    ],
                    collections.summaries,
                ]
                .concat();
                break;
            }
            let head = tree.head().await?;
            // Recheck lease/approval before the trusted publisher can mutate GitHub.
            checkpoint.heartbeat(platform, job).await?;
            let (number, url) = github::publish(config, job, &tree, &summary, &checks).await?;
            platform
                .update(job, "github", json!({"prUrl":url,"headSha":head}))
                .await?;
            github::register(config, job, &tree, number, &url).await?;
            checkpoint
                .queue_pr(platform, job, &head, number, &url)
                .await?;
            (number, url, head)
        };
        {
            let status = github::look(config, &tree, number).await?;
            if status["state"] == "MERGED" {
                let merge = status["mergeCommit"]["oid"]
                    .as_str()
                    .ok_or("GitHub merge commit missing")?;
                require(is_sha(merge), "Invalid GitHub merge confirmation")?;
                platform
                    .update(
                        job,
                        "complete",
                        json!({"result":{"prUrl":pr_url,"headSha":head,"mergeSha":merge}}),
                    )
                    .await?;
                tree.cleanup(config).await?;
                return Ok(());
            }
            require(
                status["state"] == "OPEN",
                "Support PR was closed without merging; owner action is needed",
            )?;
            if status["headRefOid"] != head {
                return Err(
                    "The PR changed outside the verified support worktree; owner action is needed"
                        .into(),
                );
            }
            if status["isDraft"] == true {
                platform
                    .update(job, "github", json!({"prUrl":pr_url,"headSha":head}))
                    .await?;
                github::ready(config, &tree, number).await?;
                let reason = "Waiting for GitHub to admit the verified head";
                checkpoint.wait(platform, job, reason, 15_000).await?;
                return Ok(());
            }
            if status["mergeable"] == "CONFLICTING" {
                failure = match tree.reconcile(config).await? {
                    Ok(_) => "Main changed; verify the reconciled implementation again".into(),
                    Err(error) => format!("Resolve the main-branch merge conflict:\n{error}"),
                };
                checkpoint
                    .progress(
                        platform,
                        job,
                        "implementing",
                        "Reconciling the change with current main",
                        json!({"phase":"build","failure":failure}),
                    )
                    .await?;
                continue 'correction;
            }
            match github::checks(config, &tree, &head).await? {
                github::Checks::Failed(run, logs) if run > checkpoint.last_handled_run() => {
                    failure = format!("The PR's required checks failed in run {run}:\n{logs}");
                    checkpoint
                        .progress(
                            platform,
                            job,
                            "implementing",
                            "Fixing the PR's required checks",
                            json!({"phase":"build","failure":failure,"lastHandledRun":run}),
                        )
                        .await?;
                    continue 'correction;
                }
                github::Checks::Passed if status["mergeable"] == "MERGEABLE" => {
                    checkpoint.heartbeat(platform, job).await?;
                    github::merge(&tree, &status, &head).await?;
                }
                _ => {}
            }
            // Remote CI does not monopolize the local supervisor. The exact verified head
            // and PR remain checkpointed while other tickets can be investigated/answered.
            tree.relieve_disk_pressure(config).await?;
            checkpoint
                .wait(platform, job, "Waiting for the PR's checks", 15_000)
                .await?;
            return Ok(());
        }
    }
}
