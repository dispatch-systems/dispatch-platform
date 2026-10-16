//! What every Slack backend test installs. A test binary holds one registry, so each installs
//! the same: Slack, and a stand-in feature with an integration through it, which hears of
//! Slack's events as an integration would and keeps what it heard.
use crate::FEATURE;
use dispatch_core::{
    manifest::{
        Feature, Upkeep, feature,
        integrations::{Heard, Integration, integration},
        optional,
    },
    testing,
};
use std::sync::Mutex;

/// The stand-in's integration: it needs to read files, and hears of files shared.
pub const LISTENING: Integration = Integration {
    needs: &["files:read"],
    hears: &["file_shared"],
    heard: Some(hear),
    ..integration("slack", "listening.files", "Listening")
};
/// A feature that works through Slack, switched off for every DSP until a test switches it on.
static LISTENER: Feature = Feature {
    switch: optional("listening", "Listening", &[]),
    depends_on: &["slack"],
    integrations: &[LISTENING],
    ..feature("listening")
};
/// What the stand-in heard of, in every test: each event's DSP, workspace and kind.
static HEARD: Mutex<Vec<(String, String, String)>> = Mutex::new(Vec::new());
fn hear(_: std::sync::Arc<dispatch_core::State>, heard: Heard) -> Upkeep {
    let mut kept = HEARD.lock().unwrap_or_else(|poison| poison.into_inner());
    kept.push((heard.dsp, heard.account, heard.kind));
    Box::pin(async {})
}
/// What the stand-in heard of from the DSP `dsp`: each event's workspace and kind.
pub fn heard(dsp: &str) -> Vec<(String, String)> {
    let kept = HEARD.lock().unwrap_or_else(|poison| poison.into_inner());
    kept.iter()
        .filter(|(at, _, _)| at == dsp)
        .map(|(_, team, kind)| (team.clone(), kind.clone()))
        .collect()
}

/// Installs Slack and the stand-in.
pub fn install() {
    testing::install(&[], &[&FEATURE, &LISTENER]);
}
