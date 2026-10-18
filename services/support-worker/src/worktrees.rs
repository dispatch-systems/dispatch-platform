use crate::{Result, config::Config, require};
use dispatch_core::support::api::types::SupportJob;
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Clone)]
pub struct Worktree {
    pub name: String,
    pub path: PathBuf,
}

pub async fn command(directory: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(directory)
        .kill_on_drop(true)
        .output()
        .await?;
    if !output.status.success() {
        // Build and GitHub errors go into the private agent context, never a public event log.
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

impl Worktree {
    pub async fn prepare(config: &Config, job: &SupportJob, source: Option<&str>) -> Result<Self> {
        let identifier = job
            .detail
            .ticket
            .id
            .strip_prefix("support_")
            .ok_or("Invalid support ticket id")?;
        require(
            identifier.len() <= 32 && identifier.chars().all(|c| c.is_ascii_hexdigit()),
            "Invalid support ticket id",
        )?;
        require(
            matches!(job.environment.as_str(), "dev" | "production"),
            "Invalid support environment",
        )?;
        let suffix = if source.is_some() { "-analysis" } else { "" };
        let name = format!("support-{}-{identifier}{suffix}", job.environment);
        let path = config.workspace_root.join("worktrees").join(&name);
        let root = config.workspace_root.join("dev");
        if !path.exists() {
            require(
                dispatchdev_cli::workspace::worktrees_free(&config.workspace_root)?
                    >= dispatchdev_cli::start::LOW_DISK,
                "Host disk capacity must be restored before creating an isolated support build",
            )?;
            command(&root, "dispatchdev", &["start", &name]).await?;
            if let Some(sha) = source {
                require(is_sha(sha), "Source commit is not an immutable Git SHA")?;
                command(&path, "git", &["fetch", "origin", sha]).await?;
                command(&path, "git", &["switch", "-C", &name, sha]).await?;
            }
        }
        // Offline source checks use only the scanner archive pinned by trusted installed Dev.
        let source_cache = root.join(".ci-tools/privacy");
        let target = path.join(".ci-tools/privacy");
        if source_cache.is_dir() {
            std::fs::create_dir_all(&target)?;
            for entry in std::fs::read_dir(source_cache)? {
                let entry = entry?;
                let name = entry.file_name();
                if entry.file_type()?.is_file() && name.to_string_lossy().ends_with(".tar.gz") {
                    std::fs::copy(entry.path(), target.join(name))?;
                }
            }
        }
        if let Some(sha) = source {
            require(is_sha(sha), "Source commit is not an immutable Git SHA")?;
            // Analysis never owns implementation edits. A reopened report may pin a later release.
            command(&path, "git", &["fetch", "origin", sha]).await?;
            command(&path, "git", &["reset", "--hard", sha]).await?;
        }
        Ok(Self { name, path })
    }

    pub async fn head(&self) -> Result<String> {
        command(&self.path, "git", &["rev-parse", "HEAD"]).await
    }

    pub async fn sandbox(&self, config: &Config, script: &str) -> Result<String> {
        let mut command = Command::new("/usr/bin/python3");
        command
            .args(["-I"])
            .arg(crate::hermes::resources().join("host-sandbox.py"))
            .args(["--mode", "build", "--worktree"])
            .arg(&self.path)
            .arg("--node")
            .arg(&config.node)
            .arg("--command")
            .arg(script)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let (child, _group) = crate::process::spawn(&mut command)?;
        let output = child.wait_with_output().await?;
        require(
            output.status.success(),
            &format!(
                "Isolated check/build failed:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub async fn build(&self, config: &Config) -> Result<()> {
        self.sandbox(config, "npm run build -- --debug").await?;
        Ok(())
    }

    pub async fn check(&self, config: &Config) -> Result<String> {
        let root = self.path.clone();
        let plan = tokio::task::spawn_blocking(move || {
            dispatchdev_cli::check::plan(&root, true, &dispatchdev_cli::Native)
        })
        .await??;
        let scripts: Vec<_> = ["npm run check:rules".to_owned()]
            .into_iter()
            .chain(plan.commands)
            .collect();
        self.sandbox(config, &scripts.join(" && ")).await
    }

    pub async fn commit(&self, job: &SupportJob) -> Result<String> {
        let changed = command(&self.path, "git", &["status", "--porcelain"]).await?;
        if !changed.is_empty() {
            command(&self.path, "git", &["add", "--all"]).await?;
            let kind = if job.detail.ticket.kind == "bug" {
                "fix"
            } else {
                "feat"
            };
            command(
                &self.path,
                "git",
                &[
                    "commit",
                    "-m",
                    &format!(
                        "{kind}: implement approved support ticket {}",
                        job.detail.ticket.id
                    ),
                ],
            )
            .await?;
        }
        self.head().await
    }

    pub async fn reconcile(&self) -> Result<String> {
        command(&self.path, "git", &["fetch", "origin", "main"]).await?;
        command(&self.path, "git", &["merge", "--no-edit", "origin/main"]).await
    }

    pub async fn cleanup(&self, config: &Config) -> Result<()> {
        command(
            &config.workspace_root.join("dev"),
            "dispatchdev",
            &["finish", &self.name],
        )
        .await?;
        Ok(())
    }

    /// Keep an unmerged implementation intact while making room for another ticket's
    /// investigation. Only ignored, reproducible build products can be discarded.
    pub async fn relieve_disk_pressure(&self, config: &Config) -> Result<()> {
        if dispatchdev_cli::workspace::worktrees_free(&config.workspace_root)?
            >= dispatchdev_cli::start::LOW_DISK
        {
            return Ok(());
        }
        for name in ["target", ".build"] {
            require(
                command(&self.path, "git", &["ls-files", "--", name])
                    .await?
                    .is_empty(),
                "Build product cleanup refuses tracked source files",
            )?;
            let path = self.path.join(name);
            if let Ok(metadata) = std::fs::symlink_metadata(&path)
                && metadata.file_type().is_dir()
            {
                std::fs::remove_dir_all(path)?;
            }
        }
        Ok(())
    }

    pub async fn cleanup_analysis(&self, config: &Config) -> Result<()> {
        command(
            &config.workspace_root.join("dev"),
            "dispatchdev",
            &["finish", &self.name, "--abandon"],
        )
        .await?;
        Ok(())
    }

    pub async fn cleanup_analysis_job(config: &Config, job: &SupportJob) -> Result<()> {
        let id = job
            .detail
            .ticket
            .id
            .strip_prefix("support_")
            .ok_or("Invalid support id")?;
        require(
            id.len() == 32
                && id.chars().all(|c| c.is_ascii_hexdigit())
                && matches!(job.environment.as_str(), "dev" | "production"),
            "Invalid support source identity",
        )?;
        let name = format!("support-{}-{id}-analysis", job.environment);
        let tree = Self {
            path: config.workspace_root.join("worktrees").join(&name),
            name,
        };
        if tree.path.exists() {
            tree.cleanup_analysis(config).await?;
        }
        Ok(())
    }
}

pub fn is_sha(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|c| c.is_ascii_hexdigit())
}
