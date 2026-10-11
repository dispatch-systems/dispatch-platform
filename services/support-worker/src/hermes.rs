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
    let profile: Value = serde_json::from_slice(&fs::read(
        resources().join("profiles").join(format!("{role}.json")),
    )?)?;
    let mut safe_job = serde_json::to_value(job)?;
    safe_job
        .as_object_mut()
        .ok_or("Invalid job")?
        .remove("lease");
    let request = json!({"profile":profile,"role":role,"instructions":instructions,"context":context,"job":safe_job,"worktree":worktree.path,"home":home,"resources":resources(),"node":config.node,"trustedNodeModules":config.workspace_root.join("dev/node_modules")});
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
        .current_dir(&worktree.path)
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
    Ok(value["result"].clone())
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
