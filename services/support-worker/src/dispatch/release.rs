use crate::{
    Result,
    checkpoint::Checkpoint,
    config::Config,
    hermes::{self, browser, rules},
    jobs::Platform,
    preview::NativePreview,
    process::command,
    require, verification,
    worktrees::{Worktree, is_sha},
};
use dispatch_core::support::api::types::SupportJob;
use serde_json::json;

pub(super) async fn run(
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
    let grant = if job.environment == "production" {
        Some(platform.metadata(job).await?)
    } else {
        None
    };
    let metadata = if let Some(grant) = &grant {
        serde_json::to_value(grant)?
    } else {
        Platform::new(production)?.deployment().await?
    };
    let source = match &grant {
        Some(grant) => grant.source_sha.as_deref(),
        None => metadata["sourceSha"].as_str(),
    }
    .ok_or("Production runtime source is not available")?;
    let release = match &grant {
        Some(grant) => json!(grant.release),
        None => metadata["release"].clone(),
    };
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
    let root = &config.checkout;
    command(root, "git", &["fetch", "origin", source]).await?;
    if command(root, "git", &["merge-base", "--is-ancestor", merge, source])
        .await
        .is_err()
    {
        let reason = "The merged change is awaiting a production release";
        checkpoint.wait(platform, job, reason, 60_000).await?;
        return Ok(());
    }
    let proposal = super::approved_proposal(job)?;
    let tree = Worktree::prepare(config, job, Some(source)).await?;
    let mut target = if let Some(grant) = &grant {
        browser(grant)?
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
        let preview = NativePreview::start(config, &tree, None).await?;
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
            json!({"result":{"release":release,"mergeSha":merge,"headSha":source}}),
        )
        .await?;
    drop(counterpart);
    tree.cleanup(config).await?;
    Ok(())
}
