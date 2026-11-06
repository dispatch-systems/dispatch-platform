use crate::{
    Result,
    checkpoint::{AnalysisProgress, Checkpoint},
    config::Config,
    hermes::{self, browser, rules},
    jobs::Platform,
    preview::NativePreview,
    require, verification,
    worktrees::{Worktree, is_sha},
};
use dispatch_core::support::api::types::SupportJob;
use serde_json::{Value, json};
use std::fs;

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

pub(super) async fn run(
    config: &Config,
    platform: &Platform,
    job: &SupportJob,
    checkpoint: &Checkpoint,
) -> Result<()> {
    use crate::hermes::AnalystStage;
    use crate::intake::{Analysis, Decision, NextStep, Outcome, Route};

    // This is intentionally before source preparation, diagnostics, screenshots and grants.
    let decision: Decision = if let Some(value) = checkpoint.routing_decision() {
        serde_json::from_value(value)?
    } else {
        checkpoint
            .analysis(
                platform,
                job,
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
        checkpoint
            .analysis(
                platform,
                job,
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

    checkpoint
        .analysis(
            platform,
            job,
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
    let AnalysisProgress {
        stage,
        mut attempted,
        mut usable,
        mut browser_attempted,
        mut live_browser_attempted,
        mut live_browser_observed,
    } = checkpoint.analysis_progress();
    let mut selected = stage.unwrap_or_else(|| {
        if decision.route == Route::Source {
            "source"
        } else {
            "browser"
        }
        .into()
    });
    let mut preview = None;
    if let Some(prior) = checkpoint.prior_analysis() {
        context["priorAnalysis"] = prior;
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
                checkpoint
                    .analysis(
                        platform,
                        job,
                        "Preparing the requested isolated reproduction",
                        json!({"phase":"isolated_reproduction","analysisStage":"isolated"}),
                    )
                    .await?;
                let baseline = async {
                    tree.build(config).await?;
                    NativePreview::start(config, tree, None).await
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
                    observed_source = metadata
                        .source_sha
                        .as_deref()
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
        checkpoint.analysis(platform, job,
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
        checkpoint.analysis(platform, job, "Recorded the tool observations", json!({"phase":"analysis","analysisStage":selected,"investigationAttempted":attempted,"usableInvestigation":usable,"browserAttempted":browser_attempted,"liveBrowserAttempted":live_browser_attempted,"liveBrowserObserved":live_browser_observed,"browserObservation":if observed_browser { "observed" } else if actual_browser { "attempted" } else if stage == AnalystStage::Source { "skipped" } else { "not_used" }})).await?;
        if actual_browser {
            verification::investigation(config, platform, job, &observed_source).await?;
        }
        let analysis: Analysis = serde_json::from_value(result)?;
        analysis.validate(&job.detail.ticket.kind, attempted, usable)?;
        checkpoint
            .analysis(
                platform,
                job,
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
                    tree.cleanup(config).await?;
                }
                return Ok(());
            }
        }
        context["priorAnalysis"] = serde_json::to_value(&analysis)?;
        checkpoint
            .analysis(
                platform,
                job,
                "Continuing with the requested evidence",
                json!({"phase":"analysis","analysisStage":selected}),
            )
            .await?;
    }
}
