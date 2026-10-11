use crate::{
    Result, config::Config, hermes::profile_home, jobs::Platform, require, worktrees::Worktree,
};
use dispatch_core::support::api::types::{SupportEvidence, SupportJob, SupportProposal};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Stdio};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

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
        platform
            .update(job, "evidence", json!({"evidence":evidence}))
            .await?;
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
            require(
                !screenshot.contains('/') && screenshot.ends_with(".png"),
                "Invalid browser observation artifact",
            )?;
            attachments.push(
                platform
                    .upload(
                        job,
                        &screenshot,
                        fs::read(home.join("evidence").join(&screenshot))?,
                    )
                    .await?,
            );
        }
        let evidence = SupportEvidence {id:String::new(), phase:"investigation".into(), source_sha:head.into(), criterion:"Real-browser platform inspection".into(), steps:witness.steps, outcome:"The browser inspected the reported environment. The proposal distinguishes actual observations, reproduction uncertainty and inferred causes.".into(), passed:true, attachment_ids:attachments, created_at:0};
        platform
            .update(job, "evidence", json!({"evidence":evidence}))
            .await?;
        return Ok(());
    }
    for assertion in witness.assertions {
        require(
            !assertion.screenshot.contains('/') && assertion.screenshot.ends_with(".png"),
            "Invalid investigation screenshot",
        )?;
        let attachment = platform
            .upload(
                job,
                &assertion.screenshot,
                fs::read(home.join("evidence").join(&assertion.screenshot))?,
            )
            .await?;
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
        platform
            .update(job, "evidence", json!({"evidence":evidence}))
            .await?;
    }
    Ok(())
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
        require(
            !assertion.screenshot.contains('/') && assertion.screenshot.ends_with(".png"),
            "Invalid browser artifact",
        )?;
        let bytes = fs::read(home.join("evidence").join(&assertion.screenshot))?;
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
        platform
            .update(job, "evidence", json!({"evidence": evidence}))
            .await?;
    }
    Ok(())
}

pub struct Collected {
    pub summaries: Vec<String>,
    pub state: Option<PathBuf>,
    pub proofs: Vec<SupportEvidence>,
}

/// Select every affected native dataset; shared collector changes exercise all datasets.
pub fn cortex_collections(diff: &str, areas: &[String]) -> Vec<&'static str> {
    let context = format!("{diff}\n{}", areas.join("\n"))
        .to_lowercase()
        .replace(['-', ' '], "_");
    let selected: Vec<_> = [
        (
            "meal_break",
            &["meal_break", "meals", "meal.js", "meal_hook", "meal_rules"][..],
        ),
        ("routes", &["routes"][..]),
        (
            "daily_performance",
            &["daily_performance", "connection/performance"][..],
        ),
        ("weekly_scorecard", &["weekly_scorecard"][..]),
        ("dvic", &["dvic"][..]),
    ]
    .into_iter()
    .filter(|(_, aliases)| aliases.iter().any(|alias| context.contains(alias)))
    .map(|(collection, _)| collection)
    .collect();
    if selected.is_empty() {
        vec![
            "meal_break",
            "routes",
            "daily_performance",
            "weekly_scorecard",
            "dvic",
        ]
    } else {
        selected
    }
}

pub struct NativePreview {
    pub origin: String,
    _child: tokio::process::Child,
    _group: crate::process::Group,
    state: PathBuf,
    bridge: PathBuf,
}

impl Drop for NativePreview {
    fn drop(&mut self) {
        // Kill the isolated execution group before unlinking only its assigned temporary state.
        unsafe {
            libc::kill(-(self._group.0 as i32), libc::SIGKILL);
        }
        let _ = fs::remove_dir_all(&self.state);
        let _ = fs::remove_dir_all(&self.bridge);
    }
}

impl NativePreview {
    pub async fn start(_config: &Config, tree: &Worktree, state: Option<&PathBuf>) -> Result<Self> {
        let scratch = PathBuf::from(format!("/tmp/dispatch-{}", tree.name));
        if scratch.is_dir() {
            for entry in fs::read_dir(&scratch)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if entry.file_type()?.is_dir()
                    && (name.starts_with("browser-state-") || name.starts_with("browser-bridge-"))
                {
                    fs::remove_dir_all(entry.path())?;
                }
            }
        }
        let mut command = Command::new("/usr/bin/python3");
        command
            .arg("-I")
            .arg(crate::hermes::resources().join("host-sandbox.py"))
            .args(["--mode", "preview", "--worktree"])
            .arg(&tree.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(state) = state {
            command.arg("--state-root").arg(state);
        }
        let (mut child, group) = crate::process::spawn(&mut command)?;
        let mut line = String::new();
        let stdout = child.stdout.take().ok_or("Native preview output missing")?;
        tokio::time::timeout(
            std::time::Duration::from_secs(40),
            BufReader::new(stdout).read_line(&mut line),
        )
        .await??;
        let result: Value = serde_json::from_str(&line)?;
        let state = PathBuf::from(
            result["stateRoot"]
                .as_str()
                .ok_or("Preview state missing")?,
        );
        let bridge = PathBuf::from(
            result["bridgeRoot"]
                .as_str()
                .ok_or("Preview bridge missing")?,
        );
        require(
            state.starts_with(&scratch)
                && bridge.starts_with(&scratch)
                && state != scratch
                && bridge != scratch,
            "Preview state leaves assigned scratch",
        )?;
        Ok(Self {
            origin: result["origin"]
                .as_str()
                .ok_or("Native preview origin missing")?
                .to_owned(),
            _child: child,
            _group: group,
            state,
            bridge,
        })
    }
}

pub async fn collect(
    config: &Config,
    tree: &Worktree,
    job: &SupportJob,
    proposal: &SupportProposal,
) -> Result<Collected> {
    let diff = crate::worktrees::command(
        &tree.path,
        "git",
        &["diff", "--name-only", "origin/main...HEAD"],
    )
    .await?;
    let relevant = diff.contains("collectors/")
        || diff.contains("core/collection/")
        || proposal.affected_areas.iter().any(|area| {
            area.contains("collection") || area.contains("paycom") || area.contains("cortex")
        });
    if !relevant {
        return Ok(Collected {
            summaries: vec![],
            state: None,
            proofs: vec![],
        });
    }
    // An interrupted previous attempt can leave rows, never credential copies. The new
    // attempt starts clean; all successful cases below share their returned private state.
    let scratch = PathBuf::from(format!("/tmp/dispatch-{}", tree.name));
    if scratch.is_dir() {
        for entry in fs::read_dir(&scratch)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("support-native-")
            {
                fs::remove_dir_all(entry.path())?;
            }
        }
    }
    tree.build(config).await?;
    let providers: Vec<_> = ["paycom", "cortex"]
        .into_iter()
        .filter(|provider| {
            diff.contains(&format!("collectors/{provider}/"))
                || proposal
                    .affected_areas
                    .iter()
                    .any(|area| area.to_lowercase().contains(provider))
                || diff.contains("core/collection/")
        })
        .collect();
    let providers = if providers.is_empty() {
        vec!["paycom", "cortex"]
    } else {
        providers
    };
    let mut evidence = vec![];
    let mut state: Option<PathBuf> = None;
    let mut proofs = vec![];
    let date = crate::worktrees::command(
        &tree.path,
        "/usr/bin/date",
        &["-u", "-d", "yesterday", "+%F"],
    )
    .await?;
    let week = dispatch_core::foundation::weeks::last_completed_week(date.parse()?);
    for provider in providers {
        let cases: Vec<(&str, Value)> = if provider == "paycom" {
            vec![("timecards", json!({"date":date}))]
        } else {
            cortex_collections(&diff, &proposal.affected_areas)
                .into_iter()
                .map(|collection| {
                    (
                        collection,
                        if matches!(collection, "weekly_scorecard" | "dvic") {
                            json!({"collection":collection,"week":week})
                        } else {
                            json!({"collection":collection,"date":date})
                        },
                    )
                })
                .collect()
        };
        for (collection, request) in cases {
            // The trusted bridge derives the configured Dev verification account and isolated DSP.
            // No provider credential or vault file is supplied to a Hermes process.
            let mut command = Command::new("/usr/bin/python3");
            command.arg("-I").arg(&config.collection_bridge);
            let request = serde_json::to_string(&request)?;
            command.args([
                "--worktree",
                tree.path.to_str().ok_or("Invalid worktree path")?,
                "--ticket",
                &job.detail.ticket.id,
                "--provider",
                provider,
                "--request",
                &request,
            ]);
            if let Some(path) = &state {
                command.arg("--state-root").arg(path);
            }
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
            let (child, _group) = crate::process::spawn(&mut command)?;
            let output = child.wait_with_output().await?;
            require(
                output.status.success(),
                "Live collection verification unavailable; authentication or provider action may be required",
            )?;
            let result: Value = serde_json::from_slice(&output.stdout)?;
            require(
                result["passed"] == true,
                "Changed collector did not complete live verification",
            )?;
            require(
                result["sourceSha"] == tree.head().await?,
                "Collection proof is from a different source commit",
            )?;
            let root = PathBuf::from(
                result["stateRoot"]
                    .as_str()
                    .ok_or("Private collection state missing")?,
            )
            .canonicalize()?;
            require(
                root.starts_with(format!("/tmp/dispatch-{}/", tree.name)),
                "Collection state leaves the assigned scratch directory",
            )?;
            state = Some(root);
            evidence.push(format!("Changed {provider}/{collection} collector completed isolated live verification using the configured Dev verification account"));
            let proof = SupportEvidence {
                id: String::new(),
                phase: "collection".into(),
                source_sha: tree.head().await?,
                criterion: format!(
                    "Live {provider}/{collection} collection completes on the changed build"
                ),
                steps: vec![
                    "Build the exact committed branch artifact".into(),
                    format!(
                        "Run the changed {provider} collector in an isolated DSP with the configured Dev verification account"
                    ),
                    "Confirm the completed job and preserved collection rows".into(),
                ],
                outcome: format!("Trusted collector result: {}", result["counts"]),
                passed: true,
                attachment_ids: vec![],
                created_at: 0,
            };
            proofs.push(proof);
        }
    }
    Ok(Collected {
        summaries: evidence,
        state,
        proofs,
    })
}
