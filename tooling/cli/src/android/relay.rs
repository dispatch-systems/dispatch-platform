use super::{device::Device, environment::Tools};
use crate::{Result, Runner, api, require};
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn pair(
    root: &Path,
    device: &Device<'_>,
    dsp: &str,
    tools: &Tools,
    runner: &dyn Runner,
) -> Result<Value> {
    let server = api::Server::find(root)?;
    let port = server.local_port()?;
    let (_, session) = api::sign_in(&server, api::OWNER, None, runner)?;
    let dsps = session["dsps"]
        .as_array()
        .ok_or("The fixture session has no DSPs.")?;
    let selected = api::dsp(dsps, dsp)?;
    let code = selected["code"]
        .as_str()
        .filter(|code| {
            !code.is_empty()
                && code
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
        .ok_or("That fixture DSP has no address. Choose Summit Delivery or Northline Logistics.")?;
    let reverse = format!("tcp:{port}");
    tools.adb(
        &["-s", device.serial, "reverse", &reverse, &reverse],
        runner,
    )?;
    let pairing = api::request(
        &server,
        api::Call {
            method: "POST",
            path: "/api/dsp/communications/phones/pairing",
            dsp: Some(dsp),
            who: None,
            data: Some("{}"),
        },
        runner,
    )?;
    require(
        pairing.status == 200,
        &format!(
            "Fixture pairing answered {}: {}",
            pairing.status, pairing.body
        ),
    )?;
    let pairing: Value = serde_json::from_str(&pairing.body)?;
    let token = pairing["code"]
        .as_str()
        .filter(|code| {
            !code.is_empty() && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        .ok_or("The fixture returned no valid pairing code.")?;
    Ok(
        json!({"app": "relay", "device": device.serial, "dsp": selected["name"], "link": format!("dispatch-phone:pair?site=http://{code}.localhost:{port}&code={token}"), "expiresAt": pairing["expiresAt"]}),
    )
}
