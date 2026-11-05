use crate::{Result, config::Config, require};
use dispatch_core::support::api::types::SupportJob;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio::process::Command;

/// Below this, a new worktree's build could fill the disk: a debug build takes about 3 GB, and
/// 7 GB once tested.
const LOW_DISK: u64 = 20_000_000_000;

/// Bytes free where support worktrees build: the worktrees folder, or the disk it links to.
fn free(config: &Config) -> Result<u64> {
    fs2::available_space(&config.worktrees).map_err(|_| "Couldn't read the free disk space".into())
}

/// What installing a commit's toolchain and crates leaves free, at least, on the disk holding this
/// user's `.rustup` and Cargo registry, which the live service shares: a toolchain takes about
/// 600 MB.
const HOME_RESERVE: u64 = 10_000_000_000;
/// What a checkout of a commit, without its builds, needs beside the worktrees.
const CHECKOUT: u64 = 1_000_000_000;
/// A provisioning checkout's name begins with this; one a worker stopped mid-way left goes once it
/// is `ABANDONED` old, longer than any provisioning takes.
const PROVISIONING: &str = ".provision-";
const ABANDONED: Duration = Duration::from_secs(60 * 60);
static CHECKOUTS: AtomicU64 = AtomicU64::new(0);

/// A provisioning checkout, removed however the provisioning ends, a cancelled job's included.
struct Scratch {
    clone: PathBuf,
    at: PathBuf,
}
impl Drop for Scratch {
    fn drop(&mut self) {
        discard(&self.clone, &self.at);
    }
}

/// The provisioning checkout `at` and its registration in `clone`: forced twice, as one whose
/// creation was stopped is still locked, and its folder removed whether Git knew it or not.
fn discard(clone: &Path, at: &Path) {
    let _ = std::process::Command::new("git")
        .args(["worktree", "remove", "-f", "-f"])
        .arg(at)
        .current_dir(clone)
        .output();
    let _ = std::fs::remove_dir_all(at);
}

/// The provisioning checkouts a worker stopped mid-way left beside the worktrees.
fn sweep(config: &Config) -> Result<()> {
    for entry in std::fs::read_dir(&config.worktrees)? {
        let entry = entry?;
        let old = entry.metadata()?.modified()?.elapsed().unwrap_or_default() > ABANDONED;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(PROVISIONING)
            && old
        {
            discard(&config.checkout, &entry.path());
        }
    }
    Ok(())
}

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

/// The Rust toolchain `commit` pins, and the crates its lockfile names, installed on the host: a
/// worktree's builds run in the sandbox offline, from this user's `.rustup` and Cargo registry
/// read-only, so they can neither install a toolchain a new pin names nor download a crate a new
/// dependency brings. `commit` is one GitHub holds: main's, or a release's.
///
/// It installs from a checkout of `commit` of its own beside the worktrees, never from the
/// worktree an agent works in, whose files could point rustup or Cargo at a program of their
/// choosing. rustup runs nothing the checkout holds. Cargo's fetch asks the compiler about its
/// targets, through the configured wrapper otherwise, so it runs with none.
async fn provision(config: &Config, commit: &str) -> Result<()> {
    require(is_sha(commit), "Provisioning needs an immutable Git SHA")?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME required")?);
    let room = fs2::available_space(&home).map_err(|_| "Couldn't read the free disk space")?;
    require(
        room >= HOME_RESERVE && free(config)? >= CHECKOUT,
        "Host disk capacity must be restored before installing a build's Rust toolchain and crates",
    )?;
    let tool = |name: &str| {
        home.join(".cargo/bin")
            .join(name)
            .to_str()
            .map(str::to_owned)
            .ok_or("Non-UTF8 HOME")
    };
    let (rustup, cargo) = (tool("rustup")?, tool("cargo")?);
    let count = CHECKOUTS.fetch_add(1, Ordering::Relaxed);
    let at = config.worktrees.join(format!(
        "{PROVISIONING}{commit}-{}-{count}",
        std::process::id()
    ));
    let scratch = Scratch {
        clone: config.checkout.clone(),
        at: at.clone(),
    };
    let path = at.to_str().ok_or("Non-UTF8 worktree path")?;
    command(
        &scratch.clone,
        "git",
        &["worktree", "add", "-q", "--detach", path, commit],
    )
    .await?;
    command(&at, &rustup, &["toolchain", "install", "--no-self-update"]).await?;
    let fetched = Command::new(&cargo)
        .args(["fetch", "--locked"])
        .env("RUSTC_WRAPPER", "")
        .env("RUSTC_WORKSPACE_WRAPPER", "")
        .current_dir(&at)
        .kill_on_drop(true)
        .output()
        .await?;
    require(
        fetched.status.success(),
        &format!(
            "cargo fetch failed: {}",
            String::from_utf8_lossy(&fetched.stderr)
        ),
    )
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
        let path = config.worktrees.join(&name);
        sweep(config)?;
        if !path.exists() {
            require(
                free(config)? >= LOW_DISK,
                "Host disk capacity must be restored before creating an isolated support build",
            )?;
            // A worktree of its own on a new branch from current main, with its packages.
            let at = path.to_str().ok_or("Non-UTF8 worktree path")?;
            command(&config.checkout, "git", &["fetch", "-q", "origin", "main"]).await?;
            let main = command(&config.checkout, "git", &["rev-parse", "origin/main"]).await?;
            command(
                &config.checkout,
                "git",
                &["worktree", "add", "-q", "-b", &name, at, &main],
            )
            .await?;
            command(&path, "npm", &["ci", "--silent"]).await?;
            if let Some(sha) = source {
                require(is_sha(sha), "Source commit is not an immutable Git SHA")?;
                command(&path, "git", &["fetch", "origin", sha]).await?;
                command(&path, "git", &["switch", "-C", &name, sha]).await?;
            }
        }
        // Offline source checks use the clone's cached secret scanner, which the scan holds to
        // its pinned checksum.
        let source_cache = config.checkout.join(".ci-tools/privacy");
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
        // What the commit it builds on pins for Rust, each time, so a provisioning that failed is
        // tried again: the release an analysis reads, or the commit of main an implementation
        // branched from or last merged, which the agent's own commits can't change.
        let base = match source {
            Some(sha) => sha.to_owned(),
            None => command(&path, "git", &["merge-base", "HEAD", "origin/main"]).await?,
        };
        provision(config, &base).await?;
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
            dispatchdev_cli::check::plan(&root, &dispatchdev_cli::Native)
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

    /// Main merged in, what it pins for Rust provisioned first, as the builder resolves any
    /// conflict offline. A failure to fetch or provision is the error; a conflict the merge left
    /// for the builder, the answer's.
    pub async fn reconcile(&self, config: &Config) -> Result<std::result::Result<String, String>> {
        command(&self.path, "git", &["fetch", "origin", "main"]).await?;
        let main = command(&self.path, "git", &["rev-parse", "origin/main"]).await?;
        provision(config, &main).await?;
        Ok(command(&self.path, "git", &["merge", "--no-edit", &main])
            .await
            .map_err(|conflict| conflict.to_string()))
    }

    /// Once its PR merged, or for analysis that never will: the worktree, its branch here and on
    /// GitHub, and its scratch folder go. A worktree holding uncommitted changes stays.
    pub async fn cleanup(&self, config: &Config) -> Result<()> {
        let clone = &config.checkout;
        if self.path.exists() {
            let at = self.path.to_str().ok_or("Non-UTF8 worktree path")?;
            command(clone, "git", &["worktree", "remove", at]).await?;
        }
        command(clone, "git", &["worktree", "prune"]).await?;
        let branch = format!("refs/heads/{}", self.name);
        if command(clone, "git", &["rev-parse", "--verify", "--quiet", &branch])
            .await
            .is_ok()
        {
            command(clone, "git", &["branch", "-D", &self.name]).await?;
        }
        // GitHub deletes a merged PR's branch itself; this catches the rest. One GitHub deleted
        // meanwhile refuses the push, and is gone all the same.
        let remote = ["ls-remote", "--heads", "origin", &self.name];
        if !command(clone, "git", &remote).await?.is_empty()
            && let Err(error) = command(
                clone,
                "git",
                &["push", "-q", "origin", "--delete", &self.name],
            )
            .await
            && !command(clone, "git", &remote).await?.is_empty()
        {
            return Err(error);
        }
        let scratch = PathBuf::from(format!("/tmp/dispatch-{}", self.name));
        if scratch.exists() {
            std::fs::remove_dir_all(scratch)?;
        }
        Ok(())
    }

    /// Keep an unmerged implementation intact while making room for another ticket's
    /// investigation. Only ignored, reproducible build products can be discarded.
    pub async fn relieve_disk_pressure(&self, config: &Config) -> Result<()> {
        if free(config)? >= LOW_DISK {
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
            path: config.worktrees.join(&name),
            name,
        };
        if tree.path.exists() {
            tree.cleanup(config).await?;
        }
        Ok(())
    }
}

pub fn is_sha(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|c| c.is_ascii_hexdigit())
}
