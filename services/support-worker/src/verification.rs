use crate::{Result, config::Config, hermes::profile_home, jobs::Platform, require};
use dispatch_core::support::api::types::{SupportEvidence, SupportJob};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::Path};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Witness {
    source_sha: String,
    browser: String,
    #[serde(default)]
    origin: String,
    #[serde(default)]
    steps: Vec<String>,
    final_screenshot: Option<String>,
    assertions: Vec<Assertion>,
}

pub async fn investigation(
    config: &Config,
    platform: &Platform,
    job: &SupportJob,
    head: &str,
) -> Result<()> {
    let home = profile_home(config, job, "analyst")?;
    let path = home.join("witness.json");
    if !path.exists() {
        let attempt: Value = serde_json::from_slice(&fs::read(home.join("browser-attempt.json"))?)?;
        require(
            attempt["attempted"] == true && attempt["sourceSha"] == head,
            "No actual browser investigation was attempted",
        )?;
        let evidence = SupportEvidence {id:String::new(), phase:"investigation".into(), source_sha:head.into(), criterion:"Real-browser investigation attempted".into(), steps:vec![format!("Attempt actual browser {}", attempt["action"])], outcome:"The real-browser attempt was unavailable. Analysis continues read-only with explicit reproduction uncertainty.".into(), passed:false, attachment_ids:vec![], created_at:0};
        platform.evidence(job, &evidence).await?;
        return Ok(());
    }
    let witness: Witness = serde_json::from_slice(&fs::read(path)?)?;
    require(
        witness.source_sha == head && !witness.steps.is_empty(),
        "Browser investigation source or observation is missing",
    )?;
    if witness.assertions.is_empty() {
        let mut attachments = vec![];
        if let Some(screenshot) = witness.final_screenshot {
            let bytes =
                read_screenshot(&home, &screenshot, "Invalid browser observation artifact")?;
            attachments.push(platform.upload(job, &screenshot, bytes).await?);
        }
        let evidence = SupportEvidence {id:String::new(), phase:"investigation".into(), source_sha:head.into(), criterion:"Real-browser platform inspection".into(), steps:witness.steps, outcome:"The browser inspected the reported environment. The proposal distinguishes actual observations, reproduction uncertainty and inferred causes.".into(), passed:true, attachment_ids:attachments, created_at:0};
        platform.evidence(job, &evidence).await?;
        return Ok(());
    }
    for assertion in witness.assertions {
        let bytes = read_screenshot(
            &home,
            &assertion.screenshot,
            "Invalid investigation screenshot",
        )?;
        let attachment = platform.upload(job, &assertion.screenshot, bytes).await?;
        let evidence = SupportEvidence {
            id: String::new(),
            phase: "investigation".into(),
            source_sha: head.into(),
            criterion: assertion.criterion,
            steps: assertion.steps,
            outcome: format!("Observed Chromium result: {}", assertion.actual),
            passed: assertion.passed,
            attachment_ids: vec![attachment],
            created_at: 0,
        };
        platform.evidence(job, &evidence).await?;
    }
    Ok(())
}

/// A screenshot the browser saved in its evidence folder, by a name that cannot leave it;
/// `invalid` refuses any other name.
fn read_screenshot(home: &Path, screenshot: &str, invalid: &str) -> Result<Vec<u8>> {
    require(
        !screenshot.contains('/') && screenshot.ends_with(".png"),
        invalid,
    )?;
    Ok(fs::read(home.join("evidence").join(screenshot))?)
}

#[derive(Deserialize, Serialize)]
struct Assertion {
    criterion: String,
    passed: bool,
    steps: Vec<String>,
    screenshot: String,
    actual: Value,
}

pub async fn record(
    config: &Config,
    platform: &Platform,
    job: &SupportJob,
    criteria: &[String],
    head: &str,
    phase: &str,
) -> Result<()> {
    let home = profile_home(config, job, "reviewer")?;
    let witness: Witness = serde_json::from_slice(&fs::read(home.join("witness.json"))?)?;
    require(
        witness.source_sha == head && witness.browser == "chromium",
        "Browser witness does not match the verified commit",
    )?;
    for criterion in criteria {
        let assertion = witness
            .assertions
            .iter()
            .rev()
            .find(|entry| entry.criterion == *criterion)
            .ok_or("An approved criterion has no actual browser assertion")?;
        require(
            assertion.passed && assertion.steps.len() >= 2,
            "Browser verification did not pass every criterion",
        )?;
        let bytes = read_screenshot(&home, &assertion.screenshot, "Invalid browser artifact")?;
        require(
            bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "Browser screenshot missing",
        )?;
        let attachment = platform.upload(job, &assertion.screenshot, bytes).await?;
        let evidence = SupportEvidence {
            id: String::new(),
            phase: phase.into(),
            source_sha: head.into(),
            criterion: criterion.clone(),
            steps: assertion.steps.clone(),
            outcome: if phase == "release" && job.environment == "dev" {
                format!(
                    "Chromium assertion at {}: {}. Dev tickets have public production identity/ancestry proof; authenticated UI observations use an isolated exact-deployed-source counterpart.",
                    witness.origin, assertion.actual
                )
            } else {
                format!(
                    "Real Chromium assertion at {}: {}",
                    witness.origin, assertion.actual
                )
            },
            passed: true,
            attachment_ids: vec![attachment],
            created_at: 0,
        };
        platform.evidence(job, &evidence).await?;
    }
    Ok(())
}
