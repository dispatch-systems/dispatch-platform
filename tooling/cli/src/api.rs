//! `dispatchdev api <name> <method> <path>`: one signed-in call to the change's preview, as the
//! owner or as any demo account, with the session's CSRF token and, for a DSP's routes, its
//! view. Sessions are kept in the change's scratch folder.
use crate::{
    Result, Runner, require,
    workspace::{self, Workspace},
};
use serde_json::{Value, json};
use std::path::Path;

pub struct Call<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub dsp: Option<&'a str>,
    pub who: Option<&'a str>,
    pub data: Option<&'a str>,
}
/// What one request answered.
struct Answer {
    status: u16,
    body: String,
}
fn curl(runner: &dyn Runner, jar: &Path, args: &[&str]) -> Result<Answer> {
    let jar = jar.to_str().ok_or("Non-UTF8 path")?;
    let base = ["curl", "-sS", "-b", jar, "-c", jar, "-w", "\n%{http_code}"];
    let bytes = runner.command(&[&base[..], args].concat(), None, 120)?;
    let text = String::from_utf8_lossy(&bytes);
    let (body, status) = text.rsplit_once('\n').ok_or("curl gave no status")?;
    Ok(Answer {
        status: status.trim().parse()?,
        body: body.to_owned(),
    })
}
/// The DSP `wanted` names, by its id or a unique part of its name.
pub fn dsp<'a>(dsps: &'a [Value], wanted: &str) -> Result<&'a str> {
    if let Some(id) = dsps
        .iter()
        .find_map(|d| (d["id"] == wanted).then(|| d["id"].as_str()))
        .flatten()
    {
        return Ok(id);
    }
    let wanted = wanted.to_lowercase();
    let matches: Vec<&Value> = dsps
        .iter()
        .filter(|d| {
            d["name"]
                .as_str()
                .is_some_and(|name| name.to_lowercase().contains(&wanted))
        })
        .collect();
    match matches.as_slice() {
        [one] => one["id"]
            .as_str()
            .ok_or_else(|| "A DSP without an id".into()),
        [] => Err(format!("No DSP is named like {wanted}.").into()),
        many => Err(format!(
            "{} DSPs are named like {wanted}: {}.",
            many.len(),
            many.iter()
                .filter_map(|d| d["name"].as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .into()),
    }
}

pub fn run(ws: &Workspace, name: &str, call: Call<'_>, runner: &dyn Runner) -> Result<bool> {
    workspace::valid_name(name)?;
    require(
        ws.worktree(name).is_dir(),
        &format!("worktrees/{name} doesn't exist."),
    )?;
    let port = crate::preview::port(name)?.ok_or_else(|| {
        format!("{name} has no preview. Start it with dispatchdev preview {name}.")
    })?;
    let origin = format!("http://{}:{port}", crate::preview::host()?);
    let log = crate::preview::journal(name, runner)?;
    require(
        !log.is_empty(),
        &format!("{name}'s preview isn't running. Start it with dispatchdev preview {name}."),
    )?;
    let who = call.who.unwrap_or("owner@dispatch.test");
    require(
        who.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@.-_+".contains(&b)),
        "An account is an email address.",
    )?;
    let jar = workspace::scratch_dir(name)?.join(format!("api-{who}.cookies"));
    let session = |runner: &dyn Runner| curl(runner, &jar, &[&format!("{origin}/api/session")]);
    let mut answer = session(runner)?;
    if answer.status == 401 || answer.status == 403 {
        // A session from an earlier run of the preview, or none yet.
        let _ = std::fs::remove_file(&jar);
        if call.who.is_none() {
            let link =
                crate::preview::link(&log).ok_or("The preview printed no sign-in link yet.")?;
            curl(runner, &jar, &["-o", "/dev/null", &link])?;
        } else {
            let password =
                crate::preview::password(&log).ok_or("The preview printed no demo password.")?;
            let body = json!({"email": who, "password": password}).to_string();
            let signed = curl(
                runner,
                &jar,
                &[
                    "-X",
                    "POST",
                    "-H",
                    &format!("origin: {origin}"),
                    "-H",
                    "content-type: application/json",
                    "--data-binary",
                    &body,
                    &format!("{origin}/api/auth/login"),
                ],
            )?;
            require(
                signed.status < 300,
                &format!(
                    "Signing in as {who} answered {}: {}",
                    signed.status, signed.body
                ),
            )?;
        }
        answer = session(runner)?;
    }
    require(
        answer.status == 200,
        &format!("The session answered {}: {}", answer.status, answer.body),
    )?;
    let session: Value = serde_json::from_str(&answer.body)?;
    let csrf = session["csrf"]
        .as_str()
        .ok_or("The session has no CSRF token")?;
    let mut headers = vec![
        "-H".to_owned(),
        format!("origin: {origin}"),
        "-H".to_owned(),
        format!("x-csrf-token: {csrf}"),
    ];
    if let Some(wanted) = call.dsp {
        let dsps = session["dsps"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        let id = dsp(dsps, wanted)?;
        let body = json!({ "dspId": id }).to_string();
        let mut args: Vec<&str> = headers.iter().map(String::as_str).collect();
        let url = format!("{origin}/api/session/dsp");
        args.extend([
            "-X",
            "POST",
            "-H",
            "content-type: application/json",
            "--data-binary",
            &body,
            &url,
        ]);
        let opened = curl(runner, &jar, &args)?;
        require(
            opened.status == 200,
            &format!(
                "Opening the DSP answered {}: {}",
                opened.status, opened.body
            ),
        )?;
        let token = serde_json::from_str::<Value>(&opened.body)?["token"]
            .as_str()
            .ok_or("Opening the DSP gave no view token")?
            .to_owned();
        headers.extend(["-H".to_owned(), format!("x-dispatch-view: {token}")]);
    }
    let method = call.method.to_uppercase();
    require(
        matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE"),
        "The method is GET, POST, PUT, PATCH or DELETE.",
    )?;
    require(
        call.path.starts_with('/'),
        "The path starts with /, such as /api/session.",
    )?;
    let url = format!("{origin}{}", call.path);
    let mut args: Vec<&str> = headers.iter().map(String::as_str).collect();
    args.extend(["-X", &method]);
    if let Some(data) = call.data {
        serde_json::from_str::<Value>(data).map_err(|_| "--data is JSON.")?;
        args.extend([
            "-H",
            "content-type: application/json",
            "--data-binary",
            data,
        ]);
    }
    args.push(&url);
    let reply = curl(runner, &jar, &args)?;
    println!("HTTP {}", reply.status);
    match serde_json::from_str::<Value>(&reply.body) {
        Ok(value) => println!("{}", serde_json::to_string_pretty(&value)?),
        Err(_) if !reply.body.is_empty() => println!("{}", reply.body),
        Err(_) => {}
    }
    Ok(reply.status < 400)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_dsp_is_named_by_its_id_or_a_unique_part_of_its_name() {
        let dsps = vec![
            json!({"id":"dsp_1","name":"Dev DSP"}),
            json!({"id":"dsp_2","name":"Northline Logistics"}),
            json!({"id":"dsp_3","name":"Summit Delivery"}),
        ];
        assert_eq!(dsp(&dsps, "dsp_3").unwrap(), "dsp_3");
        assert_eq!(dsp(&dsps, "northline").unwrap(), "dsp_2");
        assert_eq!(dsp(&dsps, "Summit").unwrap(), "dsp_3");
        assert!(dsp(&dsps, "nowhere").is_err());
        let both = dsp(&dsps, "d").unwrap_err().to_string();
        assert!(
            both.contains("Dev DSP") && both.contains("Summit Delivery"),
            "{both}"
        );
    }
}
