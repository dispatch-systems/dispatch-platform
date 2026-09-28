//! The only service the owner dashboard can start. No request supplies a command or unit.
use crate::{Error, Result, config::Playground, contracts::PlaygroundStatus, ensure};
use std::{process::Stdio, time::Duration};
use tokio::{
    process::Command,
    time::{sleep, timeout},
};

async fn healthy(config: &Playground) -> bool {
    let origin = config.local_port.map_or_else(
        || config.origin.clone(),
        |port| format!("http://127.0.0.1:{port}"),
    );
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
    else {
        return false;
    };
    let Ok(response) = client.get(format!("{origin}/health")).send().await else {
        return false;
    };
    response.status().is_success()
        && response
            .json::<serde_json::Value>()
            .await
            .is_ok_and(|value| value["ok"] == true)
}

pub async fn status(config: Option<&Playground>) -> PlaygroundStatus {
    let Some(config) = config else {
        return PlaygroundStatus {
            configured: false,
            running: false,
            can_start: false,
        };
    };
    PlaygroundStatus {
        configured: true,
        running: healthy(config).await,
        can_start: config.local_port.is_some(),
    }
}

pub async fn start(config: &Playground) -> Result<PlaygroundStatus> {
    ensure(
        config.local_port.is_some(),
        "playground_start_unavailable",
        409,
    )?;
    let work = async {
        if !healthy(config).await {
            let result = Command::new("/usr/bin/systemctl")
                .args(["--user", "start", "dispatch-design-playground.service"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .status()
                .await
                .map_err(|_| Error::new("playground_start_failed", 503))?;
            ensure(result.success(), "playground_start_failed", 503)?;
            while !healthy(config).await {
                sleep(Duration::from_millis(250)).await;
            }
        }
        Ok(PlaygroundStatus {
            configured: true,
            running: true,
            can_start: true,
        })
    };
    timeout(Duration::from_secs(12), work)
        .await
        .map_err(|_| Error::new("playground_start_failed", 503))?
}
