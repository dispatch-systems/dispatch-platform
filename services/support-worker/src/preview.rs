use crate::{Result, config::Config, require, worktrees::Worktree};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Stdio};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub(crate) struct NativePreview {
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
    /// Builds the worktree, then serves the build in a sandbox of its own.
    pub async fn start(config: &Config, tree: &Worktree, state: Option<&PathBuf>) -> Result<Self> {
        tree.build(config).await?;
        let scratch = tree.scratch();
        tree.clear_scratch(&["browser-state-", "browser-bridge-"])?;
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

    /// The browser target an agent drives this preview through, signed in as a fixture user.
    pub fn browser(&self) -> Value {
        json!({"url":self.origin,"readOnly":false,"fixtureLogin":true})
    }
}
