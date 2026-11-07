//! Local Android builds and fixture setup; T3 owns the visible emulator and its UI.
mod device;
mod environment;
mod gradle;
mod relay;

use crate::{Result, Runner, build::Environment, require, to_hex};
use device::Device;
use environment::Tools;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub enum Action<'a> {
    Doctor,
    Apps,
    Prepare {
        app: &'a str,
        device: &'a str,
        variant: Option<&'a str>,
    },
    Pair {
        app: &'a str,
        device: &'a str,
        dsp: &'a str,
    },
}

pub fn run(root: &Path, action: Action<'_>, runner: &dyn Runner) -> Result<()> {
    let env = std::env::vars().collect();
    if let Some(value) = execute(root, action, &env, runner)? {
        println!("{}", serde_json::to_string_pretty(&value)?);
    }
    Ok(())
}

fn execute(
    root: &Path,
    action: Action<'_>,
    env: &Environment,
    runner: &dyn Runner,
) -> Result<Option<Value>> {
    if matches!(action, Action::Doctor) {
        environment::doctor(root, env, runner)?;
        return Ok(None);
    }
    if let Action::Pair { app, .. } = action {
        require(
            app == "relay" || app == ":relay",
            "Fixture pairing is available for Relay only. Other apps can use android prepare.",
        )?;
    }
    let tools = Tools::find(root, env)?;
    match action {
        Action::Doctor => unreachable!(),
        Action::Apps => {
            for variant in gradle::apps(root, &tools, env, runner)? {
                println!("{}  {}", variant.module, variant.variant);
            }
        }
        Action::Prepare {
            app,
            device,
            variant,
        } => {
            let device = Device::take(root, device, &tools, env, runner)?;
            let source = gradle::source(root, runner)?;
            let apps = gradle::apps(root, &tools, env, runner)?;
            let selected = gradle::select(&apps, app, variant)?;
            let (built, apk) = gradle::build(root, selected, &tools, env, runner)?;
            require(
                source == gradle::source(root, runner)?,
                "Android source changed during the build. Run prepare again before installing.",
            )?;
            let sha256 = to_hex(&Sha256::digest(std::fs::read(&apk)?));
            device.install(&apk, &tools, runner)?;
            return Ok(Some(
                json!({"app": selected.module, "variant": built.variant, "package": built.application_id, "device": device.serial, "apk": apk, "sha256": sha256, "source": source}),
            ));
        }
        Action::Pair { device, dsp, .. } => {
            let device = Device::take(root, device, &tools, env, runner)?;
            return Ok(Some(relay::pair(root, &device, dsp, &tools, runner)?));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests;
