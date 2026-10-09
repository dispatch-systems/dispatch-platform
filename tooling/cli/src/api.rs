//! `dispatchdev api <name> <method> <path>`: one signed-in call to the change's preview, as the
//! owner at the admin's address or as any demo account at its DSP's, with the session's CSRF
//! token and, for a DSP's routes, its view. Sessions are kept in the change's scratch folder.
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
    // The jar holds a session: only this user may read it. curl keeps the mode it finds.
    if !jar.exists() {
        workspace::write_private(jar, "")?;
    }
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
pub fn dsp<'a>(dsps: &'a [Value], wanted: &str) -> Result<&'a Value> {
    if let Some(dsp) = dsps.iter().find(|d| d["id"] == wanted) {
        return Ok(dsp);
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
        [one] => Ok(one),
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

const OWNER: &str = "owner@dispatch.test";

/// Where the preview is reached: its address, the `curl` arguments that reach it there, and
/// the jar its session is kept in. The platform owner's is the admin's; a member's is their
/// DSP's own, a name under `localhost` that `--resolve` sends to the preview's host.
struct Site {
    origin: String,
    reach: Vec<String>,
    jar: std::path::PathBuf,
}
impl Site {
    fn curl(&self, runner: &dyn Runner, args: &[&str]) -> Result<Answer> {
        let reach: Vec<&str> = self.reach.iter().map(String::as_str).collect();
        curl(runner, &self.jar, &[&reach[..], args].concat())
    }
    /// The session of `who` here: kept from an earlier call, or signed in now, the owner by
    /// the preview's own link and anyone else with the demo password.
    fn session(&self, runner: &dyn Runner, log: &str, who: &str) -> Result<Value> {
        let url = format!("{}/api/session", self.origin);
        let mut answer = self.curl(runner, &[&url])?;
        if answer.status == 401 || answer.status == 403 {
            // A session from an earlier run of the preview, or none yet.
            let _ = std::fs::remove_file(&self.jar);
            if who == OWNER {
                let link =
                    crate::preview::link(log).ok_or("The preview printed no sign-in link yet.")?;
                self.curl(runner, &["-o", "/dev/null", &link])?;
            } else {
                let password =
                    crate::preview::password(log).ok_or("The preview printed no demo password.")?;
                let body = json!({"email": who, "password": password}).to_string();
                let signed = self.curl(
                    runner,
                    &[
                        "-X",
                        "POST",
                        "-H",
                        &format!("origin: {}", self.origin),
                        "-H",
                        "content-type: application/json",
                        "--data-binary",
                        &body,
                        &format!("{}/api/auth/login", self.origin),
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
            answer = self.curl(runner, &[&url])?;
        }
        require(
            answer.status == 200,
            &format!("The session answered {}: {}", answer.status, answer.body),
        )?;
        Ok(serde_json::from_str(&answer.body)?)
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
    let host = crate::preview::host()?;
    let log = crate::preview::journal(name, runner)?;
    require(
        !log.is_empty(),
        &format!("{name}'s preview isn't running. Start it with dispatchdev preview {name}."),
    )?;
    let who = call.who.unwrap_or(OWNER);
    require(
        who.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@.-_+".contains(&b)),
        "An account is an email address.",
    )?;
    let scratch = workspace::scratch_dir(name)?;
    let admin = Site {
        origin: format!("http://{host}:{port}"),
        reach: vec![],
        jar: scratch.join(format!("api-{OWNER}.cookies")),
    };
    // A member signs in at their DSP's own address, which the owner's list names.
    let (site, session, dsps) = if who == OWNER {
        let session = admin.session(runner, &log, OWNER)?;
        let dsps = session["dsps"].clone();
        (admin, session, dsps)
    } else {
        let wanted = call
            .dsp
            .ok_or("A member signs in at their DSP's address: name the DSP with --dsp.")?;
        let listed = admin.session(runner, &log, OWNER)?["dsps"].clone();
        let all = listed.as_array().map(Vec::as_slice).unwrap_or(&[]);
        let code = dsp(all, wanted)?["code"]
            .as_str()
            .ok_or("That DSP has no short code, so no address of its own yet.")?
            .to_owned();
        let address = format!("{code}.localhost:{port}");
        let site = Site {
            origin: format!("http://{address}"),
            reach: vec!["--resolve".into(), format!("{address}:{host}")],
            jar: scratch.join(format!("api-{who}.cookies")),
        };
        let session = site.session(runner, &log, who)?;
        let dsps = session["dsps"].clone();
        (site, session, dsps)
    };
    let csrf = session["csrf"]
        .as_str()
        .ok_or("The session has no CSRF token")?;
    let mut headers = vec![
        "-H".to_owned(),
        format!("origin: {}", site.origin),
        "-H".to_owned(),
        format!("x-csrf-token: {csrf}"),
    ];
    if let Some(wanted) = call.dsp {
        let dsps = dsps.as_array().map(Vec::as_slice).unwrap_or(&[]);
        let id = dsp(dsps, wanted)?["id"]
            .as_str()
            .ok_or("A DSP without an id")?;
        let body = json!({ "dspId": id }).to_string();
        let mut args: Vec<&str> = headers.iter().map(String::as_str).collect();
        let url = format!("{}/api/session/dsp", site.origin);
        args.extend([
            "-X",
            "POST",
            "-H",
            "content-type: application/json",
            "--data-binary",
            &body,
            &url,
        ]);
        let opened = site.curl(runner, &args)?;
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
    let url = format!("{}{}", site.origin, call.path);
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
    let reply = site.curl(runner, &args)?;
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
        assert_eq!(dsp(&dsps, "dsp_3").unwrap()["id"], "dsp_3");
        assert_eq!(dsp(&dsps, "northline").unwrap()["id"], "dsp_2");
        assert_eq!(dsp(&dsps, "Summit").unwrap()["id"], "dsp_3");
        assert!(dsp(&dsps, "nowhere").is_err());
        let both = dsp(&dsps, "d").unwrap_err().to_string();
        assert!(
            both.contains("Dev DSP") && both.contains("Summit Delivery"),
            "{both}"
        );
    }
}
