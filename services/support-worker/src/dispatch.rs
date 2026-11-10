use crate::{
    Result, checkpoint::Checkpoint, config::Config, github, hermes, jobs::Platform,
    worktrees::Worktree,
};
use dispatch_core::support::api::types::{SupportJob, SupportProposal};
use fs2::FileExt;
use std::{fs, time::Duration};

mod implementation;
mod investigation;
mod release;

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
    let checkpoint = Checkpoint::new(job.checkpoint.clone());
    let heartbeat = async {
        loop {
            tokio::time::sleep(Duration::from_secs(25)).await;
            checkpoint.heartbeat(platform, &job).await?;
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
            if let Err(cancellation) = github::cancel(config, &job, &checkpoint).await {
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
        let _ = checkpoint.wait(platform, &job, public_reason, 60_000).await;
        return Err(error);
    }
    Ok(())
}

/// The proposal version the job carries, the one the owner approved.
fn approved_proposal(job: &SupportJob) -> Result<&SupportProposal> {
    Ok(job
        .detail
        .proposals
        .iter()
        .find(|proposal| proposal.version == job.version)
        .ok_or("Approved proposal not found")?)
}

async fn execute(
    config: &Config,
    platform: &Platform,
    job: &SupportJob,
    checkpoint: &Checkpoint,
) -> Result<()> {
    match job.kind.as_str() {
        "investigate" | "answer" => investigation::run(config, platform, job, checkpoint).await,
        "implement" => implementation::run(config, platform, job, checkpoint).await,
        "release" => release::run(config, platform, job, checkpoint).await,
        _ => Err("Unknown support job kind".into()),
    }
}
