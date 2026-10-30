//! `dispatchdev api <method> <path>`: one signed-in call to the dev server `npm run dev` runs in
//! the checkout, as the owner at the admin's address or as any demo account at its DSP's, with
//! the session's CSRF token and, for a DSP's routes, its view. Sessions are kept in the
//! checkout's scratch folder.
use crate::{Result, Runner, require, workspace};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

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

pub(crate) const OWNER: &str = "owner@dispatch.test";

/// Where the dev server is reached: its address, the `curl` arguments that reach it there, and
/// the jar its session is kept in. The platform owner's is the admin's; a member's is their
/// DSP's own, a name under `localhost` that `--resolve` sends to the server's host.
pub(crate) struct Site {
    pub(crate) origin: String,
    reach: Vec<String>,
    pub(crate) jar: PathBuf,
}
impl Site {
    fn curl(&self, runner: &dyn Runner, args: &[&str]) -> Result<Answer> {
        let reach: Vec<&str> = self.reach.iter().map(String::as_str).collect();
        curl(runner, &self.jar, &[&reach[..], args].concat())
    }
    /// The session of `who` here: kept from an earlier call, or signed in now, the owner by
    /// the server's own link and anyone else with the demo password.
    fn session(&self, runner: &dyn Runner, server: &Server, who: &str) -> Result<Value> {
        let url = format!("{}/api/session", self.origin);
        let mut answer = self.curl(runner, &[&url])?;
        if answer.status == 401 || answer.status == 403 {
            // A session from an earlier run of the server, or none yet.
            let _ = std::fs::remove_file(&self.jar);
            if who == OWNER {
                self.curl(runner, &["-o", "/dev/null", &server.link])?;
            } else {
                let body = json!({"email": who, "password": server.password}).to_string();
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

/// Where `npm run dev` records the server it runs in its checkout, for as long as it runs.
pub const RECORD: &str = ".runtime/dev-server.json";

/// The dev server running in a checkout, as `npm run dev` records it in `RECORD`: its process,
/// its address, the link that signs the owner in, and the demo accounts' password.
#[derive(Deserialize)]
struct Record {
    pid: i32,
    origin: String,
    link: String,
    password: String,
}
/// The dev server running in a checkout, as a call reaches it: its host and port, how its
/// accounts sign in, and the checkout's scratch folder, where sessions are kept.
pub(crate) struct Server {
    pub(crate) host: String,
    port: u16,
    link: String,
    password: String,
    pub(crate) scratch: PathBuf,
}
impl Server {
    /// The server `npm run dev` runs in the checkout at `root`, or what to do without one.
    pub(crate) fn find(root: &Path) -> Result<Self> {
        let start = "No dev server runs in this checkout: start npm run dev in it first.";
        let text = std::fs::read_to_string(root.join(RECORD)).map_err(|_| start)?;
        let record: Record = serde_json::from_str(&text)?;
        // A server that was killed leaves its record behind.
        require(unsafe { libc::kill(record.pid, 0) } == 0, start)?;
        let (host, port) = address(&record.origin)
            .ok_or_else(|| format!("{RECORD} names no address: {}.", record.origin))?;
        Ok(Self {
            host,
            port,
            link: record.link,
            password: record.password,
            scratch: workspace::checkout_scratch(root)?,
        })
    }
    /// The admin's address, where the platform owner signs in.
    fn admin(&self) -> Site {
        Site {
            origin: format!("http://{}:{}", self.host, self.port),
            reach: vec![],
            jar: self.scratch.join(format!("api-{OWNER}.cookies")),
        }
    }
}
/// The host and port of an origin, `http://<host>:<port>`.
fn address(origin: &str) -> Option<(String, u16)> {
    let (host, port) = origin.strip_prefix("http://")?.rsplit_once(':')?;
    Some((host.to_owned(), port.parse().ok()?))
}

/// Where `who` is signed in on the dev server, and their session there, kept from an earlier
/// call or signed in now: the owner at the admin's address, and anyone else at the address of
/// the DSP `wanted` names, which the owner's list gives.
pub(crate) fn sign_in(
    server: &Server,
    who: &str,
    wanted: Option<&str>,
    runner: &dyn Runner,
) -> Result<(Site, Value)> {
    require(
        who.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@.-_+".contains(&b)),
        "An account is an email address.",
    )?;
    let admin = server.admin();
    if who == OWNER {
        let session = admin.session(runner, server, OWNER)?;
        return Ok((admin, session));
    }
    let wanted =
        wanted.ok_or("A member signs in at their DSP's address: name the DSP with --dsp.")?;
    let listed = admin.session(runner, server, OWNER)?["dsps"].clone();
    let all = listed.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let code = dsp(all, wanted)?["code"]
        .as_str()
        .ok_or("That DSP has no short code, so no address of its own yet.")?
        .to_owned();
    let address = format!("{code}.localhost:{}", server.port);
    let site = Site {
        origin: format!("http://{address}"),
        reach: vec!["--resolve".into(), format!("{address}:{}", server.host)],
        jar: server.scratch.join(format!("api-{who}.cookies")),
    };
    let session = site.session(runner, server, who)?;
    Ok((site, session))
}

pub fn run(root: &Path, call: Call<'_>, runner: &dyn Runner) -> Result<bool> {
    let server = Server::find(root)?;
    let (site, session) = sign_in(&server, call.who.unwrap_or(OWNER), call.dsp, runner)?;
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
        let dsps = session["dsps"].as_array().map(Vec::as_slice).unwrap_or(&[]);
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
    // The body alone on stdout, so it pipes into `jq`.
    eprintln!("HTTP {}", reply.status);
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
    fn the_record_names_where_the_server_listens() {
        assert_eq!(
            address("http://127.0.0.1:41733"),
            Some(("127.0.0.1".to_owned(), 41733))
        );
        assert_eq!(
            address("http://localhost:5173"),
            Some(("localhost".to_owned(), 5173))
        );
        assert_eq!(address("https://127.0.0.1:41733"), None);
        assert_eq!(address("http://127.0.0.1"), None);
    }
    #[test]
    fn without_a_live_server_a_call_says_to_start_one() {
        let temp = tempfile::tempdir().unwrap();
        let refused = || Server::find(temp.path()).err().unwrap().to_string();
        let start = "No dev server runs in this checkout: start npm run dev in it first.";
        assert_eq!(refused(), start);
        // A record left by a server that was killed is as good as none.
        let record = temp.path().join(RECORD);
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        let dead = json!({"pid": i32::MAX, "origin": "http://127.0.0.1:41733",
            "link": "http://127.0.0.1:41733/__preview/x", "password": "p"});
        std::fs::write(&record, dead.to_string()).unwrap();
        assert_eq!(refused(), start);
    }
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
