use crate::{Result, config::Config, require, worktrees::Worktree};
use dispatch_core::support::api::types::SupportJob;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tokio::process::Command;

pub fn resources() -> PathBuf {
    std::env::var_os("SUPPORT_WORKER_RESOURCES")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let installed = std::env::current_exe().ok().and_then(|exe| {
                exe.parent()?
                    .parent()
                    .map(|services| services.join("support-worker/hermes"))
            });
            installed
                .filter(|path| path.is_dir())
                .unwrap_or_else(|| PathBuf::from("services/support-worker/hermes"))
        })
}

pub async fn run(
    config: &Config,
    job: &SupportJob,
    worktree: &Worktree,
    role: &str,
    instructions: &str,
    context: Value,
) -> Result<Value> {
    run_private(
        config,
        job,
        Some(worktree),
        role,
        if role == "analyst" {
            Some("browser")
        } else {
            None
        },
        instructions,
        context,
    )
    .await
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalystStage {
    Intake,
    Source,
    Browser,
}

pub async fn analyze(
    config: &Config,
    job: &SupportJob,
    worktree: Option<&Worktree>,
    stage: AnalystStage,
    instructions: &str,
    mut context: Value,
) -> Result<Value> {
    let name = match stage {
        AnalystStage::Intake => "intake",
        AnalystStage::Source => "source",
        AnalystStage::Browser => "browser",
    };
    if stage == AnalystStage::Intake {
        require(
            worktree.is_none()
                && context.get("browser").is_none()
                && context.get("diagnostics").is_none()
                && context.get("imageInputs").is_none(),
            "Tool-free intake cannot receive source or browser authority",
        )?;
    } else if stage == AnalystStage::Source {
        require(
            worktree.is_some() && context.get("browser").is_none(),
            "Source analysis requires assigned source and forbids a browser grant",
        )?;
    }
    context["stage"] = name.into();
    run_private(
        config,
        job,
        worktree,
        "analyst",
        Some(name),
        instructions,
        context,
    )
    .await
}

async fn run_private(
    config: &Config,
    job: &SupportJob,
    worktree: Option<&Worktree>,
    role: &str,
    stage: Option<&str>,
    instructions: &str,
    context: Value,
) -> Result<Value> {
    require(
        matches!(role, "analyst" | "builder" | "reviewer"),
        "Invalid support role",
    )?;
    let home = profile_home(config, job, role)?;
    fs::create_dir_all(&home)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
    }
    let mut profile: Value = serde_json::from_slice(&fs::read(
        resources().join("profiles").join(format!("{role}.json")),
    )?)?;
    if let Some(stage) = stage {
        let selected = profile["stages"][stage]
            .as_object()
            .ok_or("Named support stage is missing")?
            .clone();
        for (name, value) in selected {
            require(
                matches!(name.as_str(), "toolset" | "tools" | "skills"),
                "Support stages cannot change the shared model configuration",
            )?;
            profile[name] = value;
        }
    }
    profile
        .as_object_mut()
        .ok_or("Invalid support profile")?
        .remove("stages");
    if stage == Some("intake") {
        require(profile["tools"] == json!([]), "Intake must have zero tools")?;
    }
    let safe_job = safe_job(job)?;
    let request = json!({"profile":profile,"role":role,"stage":stage,"instructions":instructions,"context":context,"job":safe_job,"worktree":worktree.map(|tree| &tree.path),"home":home,"resources":resources(),"node":config.node,"trustedNodeModules":config.workspace_root.join("dev/node_modules")});
    let input = home.join("request.json");
    atomic_json(&input, &request)?;
    let result = home.join("result.json");
    if result.exists() {
        fs::remove_file(&result)?;
    }
    if home.join("witness.json").exists() {
        fs::remove_file(home.join("witness.json"))?;
    }
    if home.join("browser-attempt.json").exists() {
        fs::remove_file(home.join("browser-attempt.json"))?;
    }
    if home.join("evidence").exists() {
        fs::remove_dir_all(home.join("evidence"))?;
    }
    let mut command = Command::new(&config.hermes_python);
    command
        .args(["-I", "-B"])
        .arg(resources().join("runner.py"))
        .arg(&config.hermes_source)
        .arg(&input)
        .arg(&result)
        // No worker bearer, GitHub key or provider collection credentials enter this process.
        .env_clear()
        .env("HOME", std::env::var("HOME")?)
        .env("HERMES_HOME", &home)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("HERMES_ENABLE_PROJECT_PLUGINS", "false")
        .env(
            "HERMES_DEFAULT_HOME",
            config
                .hermes_source
                .parent()
                .ok_or("Hermes source has no parent")?,
        )
        .current_dir(worktree.map(|tree| tree.path.as_path()).unwrap_or(&home))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let (child, _group) = crate::process::spawn(&mut command)?;
    let output = child.wait_with_output().await?;
    require(
        output.status.success(),
        "Hermes run unavailable; see private runner log and reauthenticate if necessary",
    )?;
    let value: Value = serde_json::from_slice(&fs::read(result)?)?;
    require(
        value["completed"].as_bool() == Some(true),
        "Hermes run did not complete",
    )?;
    let mut result = value["result"].clone();
    require(result.is_object(), "Hermes result is not an object")?;
    // These ledgers are overwritten by the trusted runner, never accepted from model JSON.
    result["toolActivity"] = value["toolActivity"].clone();
    result["toolObservations"] = value["toolObservations"].clone();
    Ok(result)
}

/// Public intake never receives owner-private discussion, evidence or proposals. This
/// boundary applies to every analyst stage, before the model sees the request payload.
pub fn safe_job(job: &SupportJob) -> Result<Value> {
    let mut value = serde_json::to_value(job)?;
    let object = value.as_object_mut().ok_or("Invalid job")?;
    object.remove("lease");
    object.remove("checkpoint");
    if job.kind == "investigate" {
        let detail = value["detail"]
            .as_object_mut()
            .ok_or("Invalid support detail")?;
        for field in ["proposals", "evidence", "intake"] {
            detail.remove(field);
        }
        if let Some(messages) = detail.get_mut("messages").and_then(Value::as_array_mut) {
            messages.retain(|message| message["private"] != true);
        }
        if let Some(attachments) = detail.get_mut("attachments").and_then(Value::as_array_mut) {
            attachments.retain(|attachment| attachment["private"] != true);
        }
    }
    Ok(value)
}

pub fn profile_home(config: &Config, job: &SupportJob, role: &str) -> Result<PathBuf> {
    Ok(config
        .hermes_source
        .parent()
        .ok_or("Hermes source has no parent")?
        .join("profiles")
        .join(format!(
            "dispatch-support-{}-{}-{role}",
            job.environment, job.detail.ticket.id
        )))
}

/// Uploaded evidence and durable proposals live in the platform. Retain only the latest
/// private runner diagnostics after failures, instead of accumulating screenshots per job.
pub fn cleanup_job(config: &Config, job: &SupportJob, success: bool) -> Result<()> {
    for role in ["analyst", "builder", "reviewer"] {
        let home = profile_home(config, job, role)?;
        if !home.is_dir() {
            continue;
        }
        if success {
            fs::remove_dir_all(home)?;
        } else {
            for entry in fs::read_dir(&home)? {
                let entry = entry?;
                if entry.file_name() == "runner.log" {
                    continue;
                }
                if entry.file_type()?.is_dir() {
                    fs::remove_dir_all(entry.path())?;
                } else {
                    fs::remove_file(entry.path())?;
                }
            }
        }
    }
    let inputs = config
        .runtime_root
        .join(&job.environment)
        .join(&job.detail.ticket.id)
        .join("inputs");
    if inputs.is_dir() {
        fs::remove_dir_all(inputs)?;
    }
    Ok(())
}

pub fn atomic_json(path: &Path, value: &Value) -> Result<()> {
    let temp = path.with_extension("new");
    fs::write(&temp, serde_json::to_vec_pretty(value)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(temp, path)?;
    Ok(())
}
