use crate::{
    Result,
    config::Config,
    github, hermes,
    jobs::Platform,
    require, verification,
    worktrees::{Worktree, command, is_sha},
};
use dispatch_core::support::api::types::{SupportJob, SupportProposal};
use fs2::FileExt;
use serde_json::{Value, json};
use std::{
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};

type Checkpoint = Arc<Mutex<Value>>;

/// One heavy job at a time protects the host, while every pending ticket remains durable.
/// Sources are rotated after every claim; no daily, token, runtime or attempt quota exists.
pub async fn run(config: Config) -> Result<()> {
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(config.runtime_root.join("worker.lock"))?;
    lock.try_lock_exclusive()
        .map_err(|_| "Another support worker is already running")?;
    let platforms = config
        .origins
        .iter()
        .map(Platform::new)
        .collect::<Result<Vec<_>>>()?;
    let mut first = 0;
    loop {
        let mut claimed = false;
        for offset in 0..platforms.len() {
            let index = (first + offset) % platforms.len();
            let platform = &platforms[index];
            match platform.claim().await {
                Ok(Some(job)) => {
                    claimed = true;
                    first = (index + 1) % platforms.len();
                    // Identity and origin are server supplied, and must agree with this source.
                    if job.origin.trim_end_matches('/') != platform.origin
                        || job.environment != platform.environment
                    {
                        eprintln!("Support claim refused: source origin mismatch");
                        continue;
                    }
                    if let Err(error) = leased(&config, platform, job).await {
                        eprintln!("Support job paused: {error}");
                    }
                    break;
                }
                Ok(None) => {}
                Err(_) => eprintln!("Support source unavailable; pending jobs remain durable"),
            }
        }
        if !claimed {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => return Ok(()),
                _ = tokio::time::sleep(Duration::from_secs(10)) => {}
            }
        }
    }
}

async fn leased(config: &Config, platform: &Platform, job: SupportJob) -> Result<()> {
    let checkpoint = Arc::new(Mutex::new(job.checkpoint.clone()));
    let heartbeat = async {
        loop {
            tokio::time::sleep(Duration::from_secs(25)).await;
            platform
                .update(&job, "heartbeat", json!({"checkpoint": saved(&checkpoint)}))
                .await?;
        }
        #[allow(unreachable_code)]
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    };
    let result = {
        let work = execute(config, platform, &job, &checkpoint);
        tokio::pin!(work, heartbeat);
        tokio::select! {
            result = &mut work => result,
            result = &mut heartbeat => result,
            _ = tokio::signal::ctrl_c() => Err("Support worker interrupted; job resumes after lease expiry".into()),
        }
    };
    if let Err(cleanup) = hermes::cleanup_job(config, &job, result.is_ok()) {
        eprintln!("Support private artifact cleanup requires attention: {cleanup}");
    }
    if let Err(error) = result {
        // Losing authorization/lease cancels execution before publishing. A failed wait is safe.
        if job.kind == "implement" {
            if let Err(cancellation) = github::cancel(config, &job, &saved(&checkpoint)).await {
                eprintln!("Support PR withdrawal requires attention: {cancellation}");
            }
        } else {
            // Analysis is durable in the platform; waiting attempts must not accumulate builds.
            if let Err(cleanup) = Worktree::cleanup_analysis_job(config, &job).await {
                eprintln!("Support analysis cleanup requires attention: {cleanup}");
            }
        }
        let reason = error.to_string();
        let public_reason = if reason.starts_with("Support API") {
            "Support authorization or service access changed"
        } else {
            "The agent is waiting for browser, provider or infrastructure access; private job diagnostics are available to the platform owner"
        };
        let _ = platform.update(&job, "wait", json!({"reason":public_reason,"checkpoint":saved(&checkpoint),"retryAfterMs":60_000})).await;
        return Err(error);
    }
    Ok(())
}

fn saved(checkpoint: &Checkpoint) -> Value {
    checkpoint
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

async fn progress(
    platform: &Platform,
    job: &SupportJob,
    checkpoint: &Checkpoint,
    status: &str,
    message: &str,
    fields: Value,
) -> Result<()> {
    let mut value = saved(checkpoint);
    if !value.is_object() {
        value = json!({});
    }
    if let Some(fields) = fields.as_object() {
        for (key, item) in fields {
            value[key] = item.clone();
        }
    }
    platform
        .update(
            job,
            "progress",
            json!({"status":status,"progress":message,"checkpoint":value}),
        )
        .await?;
    *checkpoint.lock().unwrap_or_else(|error| error.into_inner()) = value;
    Ok(())
}

async fn execute(
    config: &Config,
    platform: &Platform,
    job: &SupportJob,
    checkpoint: &Checkpoint,
) -> Result<()> {
    match job.kind.as_str() {
        "investigate" | "answer" => investigate(config, platform, job, checkpoint).await,
        "implement" => implement(config, platform, job, checkpoint).await,
        "release" => release(config, platform, job, checkpoint).await,
        _ => Err("Unknown support job kind".into()),
    }
}

fn rules(config: &Config) -> Result<String> {
    let root = &config.workspace_root;
    let mut instructions = fs::read_to_string(root.join("AGENTS.md"))?;
    let skill = root.join(".agents/skills/dispatch-development");
    for file in [
        "SKILL.md",
        "references/workflow.md",
        "references/writing-tests.md",
        "references/feature-creation.md",
        "references/mcp-tools.md",
    ] {
        instructions.push_str("\n\n");
        instructions.push_str(&fs::read_to_string(skill.join(file))?);
    }
    instructions.push_str("\n\nThe approved AI Support policy overrides interactive workflow steps: owner approval authorizes implementation, agent browser verification, PR creation and automatic existing-queue merge without another user preview or CodeRabbit. Production releases remain separately authorized. Do not message other users via external channels.");
    Ok(instructions)
}

fn browser(metadata: &Value) -> Result<Value> {
    let origin = metadata["origin"]
        .as_str()
        .ok_or("Read-only browser origin missing")?;
    let mut cookie = metadata["cookie"].clone();
    require(cookie.is_object(), "Read-only browser cookie missing")?;
    cookie["domain"] = url::Url::parse(origin)?
        .host_str()
        .ok_or("Browser cookie host missing")?
        .into();
    cookie["path"] = "/".into();
    cookie["expires"] = (-1).into();
    if let Some(object) = cookie.as_object_mut() {
        object.remove("url");
    }
    Ok(json!({"url":origin,"readOnly":true,"storageState":{"cookies":[cookie],"origins":[]}}))
}

async fn investigate(
    config: &Config,
    platform: &Platform,
    job: &SupportJob,
    checkpoint: &Checkpoint,
) -> Result<()> {
    require(
        is_sha(&job.detail.source_sha),
        "The submitted runtime source commit is unavailable; investigation must not guess a production version",
    )?;
    let tree = Worktree::prepare(config, job, Some(&job.detail.source_sha)).await?;
    let metadata = platform.metadata(job).await?;
    let observed_source = metadata["sourceSha"]
        .as_str()
        .filter(|source| is_sha(source))
        .ok_or("The observed runtime source is unavailable")?;
    let mut images = vec![];
    let image_root = config
        .runtime_root
        .join(&job.environment)
        .join(&job.detail.ticket.id)
        .join("inputs");
    fs::create_dir_all(&image_root)?;
    for image in job.detail.attachments.iter().filter(|image| !image.private) {
        let (kind, bytes) = platform.image(job, &image.id).await?;
        let path = image_root.join(&image.id);
        fs::write(&path, bytes)?;
        images.push(json!({"path":path,"mediaType":kind}));
    }
    let mut context = json!({"rules":rules(config)?,"headSha":observed_source,"pinnedSourceSha":job.detail.source_sha,"browser":browser(&metadata)?,"observedRuntime":observed_source,"diagnostics":platform.diagnostics(job).await?,"imageInputs":images});
    if job.kind == "investigate" {
        progress(
            platform,
            job,
            checkpoint,
            "investigating",
            "Inspecting the reported behavior in a real browser",
            json!({"phase":"production_browser","sourceSha":job.detail.source_sha}),
        )
        .await?;
    }
    let instruction = if job.kind == "answer" {
        "Answer the owner's private question identified by questionId in the messages using prior evidence and current read-only source/context. Return {answer:string}, adding a revised proposal only if the answer materially changes the plan or criteria AND no prUrl has been attached. Once a PR exists or the ticket is queued/merged/resolved, return answer only and describe any new scope as a follow-up request; the published approval is immutable. Investigate in the actual readonly browser when new observations are needed; do not invalidate an approved proposal merely to restate it."
    } else {
        "Use the real read-only browser first, then examine the pinned runtime source. Produce the proposal JSON contract. Explicitly identify reproduction steps that require mutations and will need an isolated baseline."
    };
    let readonly = hermes::run(config, job, &tree, "analyst", instruction, context.clone()).await?;
    if job.kind == "answer" {
        if let Some(proposal) = readonly
            .get("proposal")
            .filter(|_| job.detail.ticket.pr_url.is_none())
        {
            let mut proposal: SupportProposal = serde_json::from_value(proposal.clone())?;
            proposal.source_sha = job.detail.source_sha.clone();
            platform
                .update(job, "proposal", json!({"proposal":proposal}))
                .await?;
        }
        platform
            .update(
                job,
                "complete",
                json!({"result":{"answer":readonly["answer"]}}),
            )
            .await?;
        tree.cleanup_analysis(config).await?;
        return Ok(());
    }
    verification::investigation(config, platform, job, observed_source).await?;
    // A second browser uses the exact submitted source with isolated fixture data. This allows
    // save/sync reproduction without any production mutation or borrowed production credentials.
    let baseline = async {
        tree.build(config).await?;
        verification::NativePreview::start(config, &tree, None).await
    }
    .await;
    let preview = match baseline {
        Ok(preview) => Some(preview),
        Err(error) => {
            context["isolatedBaselineUnavailable"] = error.to_string().into();
            None
        }
    };
    if let Some(preview) = &preview {
        context["browser"] = json!({"url":preview.origin,"readOnly":false,"fixtureLogin":true});
        context["headSha"] = job.detail.source_sha.clone().into();
    }
    context["priorReadOnlyInvestigation"] = readonly;
    let result = hermes::run(config, job, &tree, "analyst", "Continue the prior browser-first investigation using the isolated baseline matching the pinned source. Reproduce mutations only here. Analyze even if reproduction fails, distinguish observations from inference, and return the final versioned proposal JSON contract. Keep reporter/DSP fixture differences explicit.", context).await?;
    verification::investigation(
        config,
        platform,
        job,
        if preview.is_some() {
            &job.detail.source_sha
        } else {
            observed_source
        },
    )
    .await?;
    if let Some(proposal) = result.get("proposal") {
        let mut proposal: SupportProposal = serde_json::from_value(proposal.clone())?;
        proposal.source_sha = job.detail.source_sha.clone();
        require(
            !proposal.acceptance_criteria.is_empty()
                && !proposal.production_verification.is_empty(),
            "Proposal has no verifiable implementation or readonly production criteria",
        )?;
        platform
            .update(job, "proposal", json!({"proposal":proposal}))
            .await?;
    }
    platform
        .update(
            job,
            "complete",
            json!({"result":{"answer":result["answer"]}}),
        )
        .await?;
    drop(preview);
    tree.cleanup_analysis(config).await?;
    Ok(())
}

async fn implement(
    config: &Config,
    platform: &Platform,
    job: &SupportJob,
    checkpoint: &Checkpoint,
) -> Result<()> {
    require(
        job.detail.ticket.approved_version == Some(job.version),
        "Implementation requires the exact approved proposal",
    )?;
    let proposal = job
        .detail
        .proposals
        .iter()
        .find(|proposal| proposal.version == job.version)
        .ok_or("Approved proposal not found")?;
    let tree = Worktree::prepare(config, job, None).await?;
    let mut failure = saved(checkpoint)["failure"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    'correction: loop {
        let current = saved(checkpoint);
        let resume_queue = current["phase"] == "queued"
            && current["headSha"] == tree.head().await?
            && current["prNumber"].is_u64();
        let (number, pr_url, head) = if resume_queue {
            (
                current["prNumber"].as_u64().ok_or("PR number missing")?,
                current["prUrl"]
                    .as_str()
                    .ok_or("PR URL missing")?
                    .to_owned(),
                tree.head().await?,
            )
        } else {
            progress(
                platform,
                job,
                checkpoint,
                "implementing",
                "Implementing the approved proposal",
                json!({"phase":"build"}),
            )
            .await?;
            let summary;
            let checks;
            loop {
                let initial = async {
                    tree.build(config).await?;
                    verification::NativePreview::start(config, &tree, None).await
                }
                .await;
                let initial = match initial {
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
                    context["browser"] =
                        json!({"url":initial.origin,"readOnly":false,"fixtureLogin":true});
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
                            && let Err(conflict) = tree.reconcile().await
                        {
                            failure.push_str(&format!("\nReconcile main conflict: {conflict}"));
                        }
                        continue;
                    }
                }
                progress(
                    platform,
                    job,
                    checkpoint,
                    "verifying",
                    "An independent agent is testing the committed change in a real browser",
                    json!({"phase":"verification","headSha":head}),
                )
                .await?;
                let collections = verification::collect(config, &tree, job, proposal).await?;
                for proof in &collections.proofs {
                    platform
                        .update(job, "evidence", json!({"evidence":proof}))
                        .await?;
                }
                // Every preview executes the committed artifact with no host network or secrets.
                tree.build(config).await?;
                let native =
                    verification::NativePreview::start(config, &tree, collections.state.as_ref())
                        .await?;
                let target = json!({"url":native.origin,"readOnly":false,"fixtureLogin":true});
                let context = json!({"rules":rules(config)?,"proposal":proposal,"acceptanceCriteria":proposal.acceptance_criteria,"headSha":head,"collectionResults":collections.summaries,"browser":target});
                let reviewed = hermes::run(config, job, &tree, "reviewer", "Independently verify every criterion in the actual changed application. Use real browser interactions and dispatch_browser assert for each exact criterion. Inspect persisted outcomes after refresh and relevant roles/layouts. Return the independent-verification JSON contract.", context).await?;
                if reviewed["passed"] != true {
                    failure = format!(
                        "Independent browser verification failed:\n{}",
                        reviewed["findings"]
                    );
                    progress(
                        platform,
                        job,
                        checkpoint,
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
            platform
                .update(job, "heartbeat", json!({"checkpoint":saved(checkpoint)}))
                .await?;
            let (number, url) = github::publish(config, job, &tree, &summary, &checks).await?;
            platform
                .update(job, "github", json!({"prUrl":url,"headSha":head}))
                .await?;
            github::register(config, job, &tree, number, &url).await?;
            progress(platform, job, checkpoint, "queued", "The verified change is entering the existing merge queue", json!({"phase":"queued","headSha":head,"prNumber":number,"prUrl":url,"failure":null})).await?;
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
            require(
                status["headRefOid"] == head,
                "The PR changed outside the verified support worktree; owner action is needed",
            )?;
            if status["isDraft"] == true {
                platform
                    .update(job, "github", json!({"prUrl":pr_url,"headSha":head}))
                    .await?;
                github::ready(config, &tree, number).await?;
                platform.update(job, "wait", json!({"reason":"Waiting for GitHub to admit the verified head","checkpoint":saved(checkpoint),"retryAfterMs":15_000})).await?;
                return Ok(());
            }
            if status["mergeable"] == "CONFLICTING" {
                failure = match tree.reconcile().await {
                    Ok(_) => "Main changed; verify the reconciled implementation again".into(),
                    Err(error) => format!("Resolve the main-branch merge conflict:\n{error}"),
                };
                progress(
                    platform,
                    job,
                    checkpoint,
                    "implementing",
                    "Reconciling the change with current main",
                    json!({"phase":"build","failure":failure}),
                )
                .await?;
                continue 'correction;
            }
            if let Some((run, logs)) = github::failure(config, &tree, number).await?
                && run > saved(checkpoint)["lastHandledRun"].as_u64().unwrap_or(0)
            {
                failure = format!("Merge queue required checks failed in run {run}:\n{logs}");
                progress(
                    platform,
                    job,
                    checkpoint,
                    "implementing",
                    "Fixing required merge-queue checks",
                    json!({"phase":"build","failure":failure,"lastHandledRun":run}),
                )
                .await?;
                continue 'correction;
            }
            if status["mergeQueueEntry"].is_null()
                && github::admitted(&status)
                && status["mergeable"] == "MERGEABLE"
            {
                platform
                    .update(job, "heartbeat", json!({"checkpoint":saved(checkpoint)}))
                    .await?;
                github::enqueue(&tree, &status, &head).await?;
            }
            // Remote CI does not monopolize the local supervisor. The exact verified head
            // and PR remain checkpointed while other tickets can be investigated/answered.
            tree.relieve_disk_pressure(config).await?;
            platform.update(job, "wait", json!({"reason":"Waiting for the existing merge queue checks","checkpoint":saved(checkpoint),"retryAfterMs":15_000})).await?;
            return Ok(());
        }
    }
}

async fn release(
    config: &Config,
    platform: &Platform,
    job: &SupportJob,
    checkpoint: &Checkpoint,
) -> Result<()> {
    let production = config
        .origins
        .iter()
        .find(|origin| origin.environment == "production")
        .ok_or("Production support origin is not configured")?;
    let metadata = if job.environment == "production" {
        platform.metadata(job).await?
    } else {
        Platform::new(production)?.deployment().await?
    };
    let source = metadata["sourceSha"]
        .as_str()
        .ok_or("Production runtime source is not available")?;
    let merge = job
        .detail
        .ticket
        .merge_sha
        .as_deref()
        .ok_or("Ticket has no confirmed merge")?;
    require(
        is_sha(source) && is_sha(merge),
        "Release correlation requires immutable source and merge commits",
    )?;
    let root = config.workspace_root.join("dev");
    command(&root, "git", &["fetch", "origin", source]).await?;
    if command(
        &root,
        "git",
        &["merge-base", "--is-ancestor", merge, source],
    )
    .await
    .is_err()
    {
        platform.update(job, "wait", json!({"reason":"The merged change is awaiting a production release","checkpoint":saved(checkpoint),"retryAfterMs":60_000})).await?;
        return Ok(());
    }
    let proposal = job
        .detail
        .proposals
        .iter()
        .find(|proposal| proposal.version == job.version)
        .ok_or("Approved proposal not found")?;
    let tree = Worktree::prepare(config, job, Some(source)).await?;
    let mut target = if job.environment == "production" {
        browser(&metadata)?
    } else {
        json!({"url":production.url,"readOnly":true})
    };
    let mut counterpart = None;
    if job.environment == "dev" {
        let deployment_criteria =
            vec!["Public production health identifies the deployed source commit".to_owned()];
        let context = json!({"rules":rules(config)?,"headSha":source,"acceptanceCriteria":deployment_criteria,"deploymentMetadata":metadata,"browser":target});
        let deployed = hermes::run(config, job, &tree, "reviewer", "Inspect the public production /api/health in the actual readonly browser. Assert the exact supplemental criterion using a text assertion containing deploymentMetadata.sourceSha. This proves deployment identity only; do not claim authenticated production access. Return independent-verification JSON.", context).await?;
        require(
            deployed["passed"] == true,
            "Production deployment identity was not browser verified",
        )?;
        verification::record(
            config,
            platform,
            job,
            &deployment_criteria,
            source,
            "release",
        )
        .await?;
        tree.build(config).await?;
        let preview = verification::NativePreview::start(config, &tree, None).await?;
        target = json!({"url":preview.origin,"readOnly":false,"fixtureLogin":true});
        counterpart = Some(preview);
    }
    let context = json!({"rules":rules(config)?,"proposal":proposal,"acceptanceCriteria":proposal.production_verification,"priorImplementationEvidence":job.detail.evidence,"deploymentMetadata":metadata,"headSha":source,"browser":target,"originEnvironment":job.environment,"verificationBasis":if counterpart.is_some() {"Isolated counterpart built from actual deployed production source; public production identity and ancestry were separately verified. Authenticated production behavior is unobserved."} else {"Actual production readonly browser session with the reporter's current access"}});
    let reviewed = hermes::run(config, job, &tree, "reviewer", "The confirmed merge is now in this production runtime source. Verify ONLY the separate immutable productionVerification readonly criteria. Prior mutations were proven before merge; never repeat production writes or invent authenticated access. For dev-origin tickets this browser is an ISOLATED EXACT-PRODUCTION-SOURCE COUNTERPART, and production observations consist only of the separately verified public runtime identity and ancestry. Keep that limitation explicit; reject any criterion requiring unavailable authenticated production observations. Return the independent-verification JSON contract.", context).await?;
    require(
        reviewed["passed"] == true,
        "Production browser verification found an unresolved problem",
    )?;
    verification::record(
        config,
        platform,
        job,
        &proposal.production_verification,
        source,
        "release",
    )
    .await?;
    let current_deployment = Platform::new(production)?.deployment().await?;
    require(
        current_deployment["sourceSha"] == source,
        "Production advanced during release verification; verify the new deployed head",
    )?;
    platform
        .update(
            job,
            "complete",
            json!({"result":{"release":metadata["release"],"mergeSha":merge,"headSha":source}}),
        )
        .await?;
    drop(counterpart);
    tree.cleanup_analysis(config).await?;
    Ok(())
}
