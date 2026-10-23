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
    instructions.push_str("\n\nThe approved AI Support policy overrides interactive workflow steps: owner approval authorizes implementation, agent browser verification, PR creation and automatic existing-queue merge without another user preview or code review. Production releases remain separately authorized. Do not message other users via external channels.");
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

async fn analysis_progress(
    platform: &Platform,
    job: &SupportJob,
    checkpoint: &Checkpoint,
    message: &str,
    fields: Value,
) -> Result<()> {
    if job.kind != "answer" {
        let status = if fields["phase"] == "routing" {
            "triaging"
        } else {
            "investigating"
        };
        return progress(platform, job, checkpoint, status, message, fields).await;
    }
    // Owner answers must preserve approval/queued status while selecting their tools.
    let mut value = saved(checkpoint);
    if !value.is_object() {
        value = json!({});
    }
    for (key, item) in fields.as_object().ok_or("Invalid analysis checkpoint")? {
        value[key] = item.clone();
    }
    platform
        .update(job, "heartbeat", json!({"checkpoint":value}))
        .await?;
    *checkpoint.lock().unwrap_or_else(|error| error.into_inner()) = value;
    Ok(())
}

async fn report_images(config: &Config, platform: &Platform, job: &SupportJob) -> Result<Value> {
    let root = config
        .runtime_root
        .join(&job.environment)
        .join(&job.detail.ticket.id)
        .join("inputs");
    fs::create_dir_all(&root)?;
    let mut images = vec![];
    for image in job.detail.attachments.iter().filter(|image| !image.private) {
        let (kind, bytes) = platform.image(job, &image.id).await?;
        let path = root.join(&image.id);
        fs::write(&path, bytes)?;
        images.push(json!({"path":path,"mediaType":kind}));
    }
    Ok(json!(images))
}

async fn investigate(
    config: &Config,
    platform: &Platform,
    job: &SupportJob,
    checkpoint: &Checkpoint,
) -> Result<()> {
    use crate::hermes::AnalystStage;
    use crate::intake::{Analysis, Decision, NextStep, Outcome, Route};

    // This is intentionally before source preparation, diagnostics, screenshots and grants.
    let decision: Decision = if let Some(value) = saved(checkpoint).get("routingDecision") {
        serde_json::from_value(value.clone())?
    } else {
        analysis_progress(
            platform,
            job,
            checkpoint,
            "Choosing the most useful response",
            json!({"phase":"routing"}),
        )
        .await?;
        let mut result = hermes::analyze(
            config,
            job,
            None,
            AnalystStage::Intake,
            crate::intake::ROUTING,
            json!({}),
        )
        .await?;
        let activity = result
            .as_object_mut()
            .ok_or("Invalid intake result")?
            .remove("toolActivity");
        let observations = result
            .as_object_mut()
            .ok_or("Invalid intake result")?
            .remove("toolObservations");
        require(
            activity
                .as_ref()
                .is_some_and(|value| value.as_array().is_some_and(Vec::is_empty))
                && observations
                    .as_ref()
                    .is_some_and(|value| value.as_array().is_some_and(Vec::is_empty)),
            "Tool-free intake attempted a tool",
        )?;
        let decision: Decision = serde_json::from_value(result)?;
        decision.validate()?;
        analysis_progress(
            platform,
            job,
            checkpoint,
            "Selected the useful investigation steps",
            json!({"phase":"routing","routingDecision":decision,"routingSchema":1}),
        )
        .await?;
        decision
    };
    decision.validate()?;
    if let Some(completion) = decision.completion() {
        let result = if job.kind == "answer" {
            json!({"answer":decision.reply})
        } else {
            completion
        };
        platform
            .update(job, "complete", json!({"result":result}))
            .await?;
        return Ok(());
    }

    analysis_progress(
        platform,
        job,
        checkpoint,
        "Preparing the selected investigation",
        json!({"phase":"source_preparation"}),
    )
    .await?;
    let mut context = json!({"rules":rules(config)?,"pinnedSourceSha":job.detail.source_sha,"headSha":job.detail.source_sha,"routing":decision});
    let tree = if is_sha(&job.detail.source_sha) {
        match Worktree::prepare(config, job, Some(&job.detail.source_sha)).await {
            Ok(tree) => Some(tree),
            Err(error) => {
                context["sourceUnavailable"] = error.to_string().into();
                None
            }
        }
    } else {
        context["sourceUnavailable"] = "The submitted runtime source commit is unavailable; no source version has been guessed".into();
        None
    };
    let mut selected = saved(checkpoint)["analysisStage"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if decision.route == Route::Source {
                "source"
            } else {
                "browser"
            }
            .into()
        });
    let mut attempted = saved(checkpoint)["investigationAttempted"]
        .as_bool()
        .unwrap_or(false);
    let mut usable = saved(checkpoint)["usableInvestigation"]
        .as_bool()
        .unwrap_or(false);
    let mut browser_attempted = saved(checkpoint)["browserAttempted"]
        .as_bool()
        .unwrap_or(false);
    let mut live_browser_attempted = saved(checkpoint)["liveBrowserAttempted"]
        .as_bool()
        .unwrap_or(false);
    let mut live_browser_observed = saved(checkpoint)["liveBrowserObserved"]
        .as_bool()
        .unwrap_or(false);
    let mut preview = None;
    if let Some(prior) = saved(checkpoint).get("priorAnalysis") {
        context["priorAnalysis"] = prior.clone();
    }

    loop {
        context
            .as_object_mut()
            .ok_or("Invalid analysis context")?
            .remove("browser");
        context
            .as_object_mut()
            .ok_or("Invalid analysis context")?
            .remove("diagnostics");
        context["headSha"] = job.detail.source_sha.clone().into();
        let mut observed_source = job.detail.source_sha.clone();
        let stage = if selected == "isolated" {
            let tree = tree
                .as_ref()
                .ok_or("Pinned source is unavailable for isolated reproduction")?;
            if preview.is_none() {
                analysis_progress(
                    platform,
                    job,
                    checkpoint,
                    "Preparing the requested isolated reproduction",
                    json!({"phase":"isolated_reproduction","analysisStage":"isolated"}),
                )
                .await?;
                let baseline = async {
                    tree.build(config).await?;
                    verification::NativePreview::start(config, tree, None).await
                }
                .await;
                match baseline {
                    Ok(value) => preview = Some(value),
                    Err(error) => {
                        context["isolatedBaselineUnavailable"] = error.to_string().into();
                        context["continuationConstraint"] = "An isolated build was attempted and is unavailable. Continue source analysis; do not request the same unavailable baseline again. Identified issues can still have an uncertain proposal; infrastructure alone cannot make a report unclear.".into();
                        selected = "source".into();
                    }
                }
            }
            if let Some(preview) = &preview {
                context["browser"] =
                    json!({"url":preview.origin,"readOnly":false,"fixtureLogin":true});
                AnalystStage::Browser
            } else {
                AnalystStage::Source
            }
        } else if selected == "browser" {
            match platform.metadata(job).await {
                Ok(metadata) => {
                    observed_source = metadata["sourceSha"]
                        .as_str()
                        .filter(|source| is_sha(source))
                        .ok_or("The observed runtime source is unavailable")?
                        .into();
                    context["browser"] = browser(&metadata)?;
                    context["headSha"] = observed_source.clone().into();
                    context["observedRuntime"] = observed_source.clone().into();
                    match platform.diagnostics(job).await {
                        Ok(value) => context["diagnostics"] = value,
                        Err(_) => context["diagnosticsUnavailable"] = true.into(),
                    }
                    context["imageInputs"] = report_images(config, platform, job).await?;
                    AnalystStage::Browser
                }
                Err(error) => {
                    context["liveBrowserUnavailable"] = error.to_string().into();
                    context["continuationConstraint"] = "The live browser grant is unavailable. Continue useful source analysis; do not request the same unavailable grant again. Access failure alone cannot make a report unclear.".into();
                    selected = "source".into();
                    AnalystStage::Source
                }
            }
        } else {
            require(selected == "source", "Invalid durable analysis stage")?;
            AnalystStage::Source
        };
        if stage == AnalystStage::Source {
            require(
                tree.is_some(),
                "Pinned source and useful browser access are unavailable; investigation needs infrastructure access",
            )?;
        }
        analysis_progress(platform, job, checkpoint,
            if stage == AnalystStage::Source { "Reading the relevant source and prior context" } else { "Inspecting the relevant behavior in a real browser" },
            json!({"phase":"analysis","analysisStage":selected,"browserObservation":if stage == AnalystStage::Source { "skipped" } else { "granted" }})).await?;
        let mut result = hermes::analyze(
            config,
            job,
            tree.as_ref(),
            stage,
            crate::intake::ANALYSIS,
            context.clone(),
        )
        .await?;
        let activity = result
            .as_object_mut()
            .ok_or("Invalid analysis result")?
            .remove("toolActivity")
            .and_then(|value| value.as_array().cloned())
            .ok_or("Trusted tool activity is missing")?;
        let observations = result
            .as_object_mut()
            .ok_or("Invalid analysis result")?
            .remove("toolObservations")
            .and_then(|value| value.as_array().cloned())
            .ok_or("Trusted usable tool observations are missing")?;
        attempted |= !activity.is_empty();
        usable |= !observations.is_empty();
        let actual_browser = activity.iter().any(|tool| tool == "dispatch_browser");
        let observed_browser = observations.iter().any(|tool| tool == "dispatch_browser");
        browser_attempted |= actual_browser;
        live_browser_attempted |= actual_browser && selected != "isolated";
        live_browser_observed |= observed_browser && selected != "isolated";
        analysis_progress(platform, job, checkpoint, "Recorded the tool observations", json!({"phase":"analysis","analysisStage":selected,"investigationAttempted":attempted,"usableInvestigation":usable,"browserAttempted":browser_attempted,"liveBrowserAttempted":live_browser_attempted,"liveBrowserObserved":live_browser_observed,"browserObservation":if observed_browser { "observed" } else if actual_browser { "attempted" } else if stage == AnalystStage::Source { "skipped" } else { "not_used" }})).await?;
        if actual_browser {
            verification::investigation(config, platform, job, &observed_source).await?;
        }
        let analysis: Analysis = serde_json::from_value(result)?;
        analysis.validate(&job.detail.ticket.kind, attempted, usable)?;
        analysis_progress(
            platform,
            job,
            checkpoint,
            "Recorded the investigation findings",
            json!({"phase":"analysis","analysisStage":selected,"priorAnalysis":analysis}),
        )
        .await?;

        match analysis.next_step {
            NextStep::Browser => {
                require(
                    selected == "source" && context.get("liveBrowserUnavailable").is_none(),
                    "The requested browser continuation is unavailable or was already inspected",
                )?;
                selected = "browser".into();
            }
            NextStep::Isolated => {
                require(
                    selected != "isolated" && context.get("isolatedBaselineUnavailable").is_none(),
                    "The requested isolated reproduction is unavailable or was already inspected",
                )?;
                selected = "isolated".into();
            }
            NextStep::Finish => {
                if let Some(mut proposal) = analysis.proposal.clone() {
                    require(
                        is_sha(&job.detail.source_sha),
                        "A proposal cannot guess the submitted runtime source",
                    )?;
                    proposal.source_sha = job.detail.source_sha.clone();
                    require(
                        !proposal.acceptance_criteria.is_empty()
                            && !proposal.production_verification.is_empty(),
                        "Proposal has no verifiable implementation or readonly production criteria",
                    )?;
                    if !live_browser_observed {
                        proposal.uncertainties.push("No live browser inspection was recorded; this proposal relies on source, supplied context or isolated observations, with no live browser pass claimed.".into());
                    }
                    require(
                        job.detail.ticket.pr_url.is_none(),
                        "Published approval is immutable; additional scope needs a follow-up",
                    )?;
                    platform
                        .update(job, "proposal", json!({"proposal":proposal}))
                        .await?;
                }
                let completion =
                    if job.kind == "answer" || analysis.outcome == Some(Outcome::Proposal) {
                        json!({"answer":analysis.reply})
                    } else {
                        analysis.completion(attempted)
                    };
                platform
                    .update(job, "complete", json!({"result":completion}))
                    .await?;
                drop(preview);
                if let Some(tree) = tree {
                    tree.cleanup_analysis(config).await?;
                }
                return Ok(());
            }
        }
        context["priorAnalysis"] = serde_json::to_value(&analysis)?;
        analysis_progress(
            platform,
            job,
            checkpoint,
            "Continuing with the requested evidence",
            json!({"phase":"analysis","analysisStage":selected}),
        )
        .await?;
    }
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
            let queue_run = github::queue_run(config, &tree, number).await?;
            if let github::QueueRun::Failed(run, logs) = &queue_run
                && *run > saved(checkpoint)["lastHandledRun"].as_u64().unwrap_or(0)
            {
                let run = *run;
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
            // Out of the queue with its run still going, it waits for that run's verdict.
            if status["mergeQueueEntry"].is_null()
                && !matches!(queue_run, github::QueueRun::Going)
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
