use crate::{Result, config::Config, require, worktrees::Worktree};
use dispatch_core::support::api::types::{SupportEvidence, SupportJob, SupportProposal};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Stdio};
use tokio::process::Command;

pub(crate) struct Collected {
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

pub(crate) async fn collect(
    config: &Config,
    tree: &Worktree,
    job: &SupportJob,
    proposal: &SupportProposal,
) -> Result<Collected> {
    let diff = crate::process::command(
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
    let date = crate::process::command(
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
            command
                .arg("-I")
                .arg(&config.collection_bridge)
                .env("SUPPORT_WORKTREES", &config.worktrees);
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
