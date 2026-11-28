//! Requests for a demo, as the public site's form sends them: each kept once for the platform
//! owners, who are emailed it and say where they stand with it.
use crate::{
    Error, Result,
    db::{FromRow, Row, Store, iso},
    ensure,
    foundation::{config::Config, crypto, validate as v},
    platform_owner::api::types::{DemoRequest, DemoStatus},
    server::mail::templates,
};
use axum::http::HeaderMap;
use rusqlite::params;
use serde_json::Value;

/// The kind of email a new request's notice is, as the mail log names it.
const NOTICE: &str = "demo_request";
/// What signs a request where no site sends any: on the dev server and in tests.
pub const FIXTURE_SECRET: &str = "fixture-demo-requests-secret-0123456789";
/// The fewest characters a secret may have. A shorter one is taken as none.
const SECRET_LEAST: usize = 32;
/// The most requests kept in an hour, whoever sends them: the site limits each visitor, and this
/// bounds what a flood there, or a secret that got out, can store and email.
const HOURLY: i64 = 120;
const HOUR_MS: i64 = 60 * 60 * 1000;

/// The secret the site signs each request with, `DEMO_REQUESTS_SECRET` under the environment's
/// prefix: none takes no requests, so the route answers as if it weren't there.
pub fn secret(config: &Config) -> Option<String> {
    config
        .setting("DEMO_REQUESTS_SECRET")
        .filter(|secret| secret.len() >= SECRET_LEAST)
        .or_else(|| config.fixture.then(|| FIXTURE_SECRET.to_owned()))
}

/// Whether `body` came from the site: signed with `secret` as `v1:<timestamp>:<body>`, at a
/// time within five minutes of `now`, in seconds, so one sent again later is refused.
pub fn signed(secret: &str, headers: &HeaderMap, body: &str, now: i64) -> bool {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
    };
    let timestamp = header("x-dispatch-timestamp");
    let Some(sent) = header("x-dispatch-signature").strip_prefix("v1=") else {
        return false;
    };
    if timestamp
        .parse::<i64>()
        .map_or(true, |at| (now - at).abs() > 5 * 60)
    {
        return false;
    }
    let made = crypto::hmac(secret.as_bytes(), &format!("v1:{timestamp}:{body}"));
    crypto::equal(&crypto::hex(&made), sent)
}

/// What a visitor asked with, as the site sent it.
pub struct Asked {
    submission: String,
    name: String,
    email: String,
    dsp: String,
    message: String,
}
impl Asked {
    /// A request's fields, each within what the form allows: a name and a DSP's name on one
    /// line, an address an email reaches, and a message that may be empty.
    pub fn of(sent: &Value) -> Result<Self> {
        v::fields(sent, &["id", "name", "email", "dsp", "message"])?;
        let line = |key: &str| {
            let text = v::name(sent, key, 100)?;
            ensure(!text.chars().any(char::is_control), "invalid_input", 400)?;
            Ok::<_, Error>(text)
        };
        let submission = v::text(sent, "id", 16, 64)?;
        ensure(v::token(submission, 64), "invalid_input", 400)?;
        Ok(Self {
            submission: submission.to_owned(),
            name: line("name")?,
            email: v::email(sent, "email")?,
            dsp: line("dsp")?,
            message: v::text(sent, "message", 0, 2000)?.trim().to_owned(),
        })
    }
}

impl FromRow for DemoRequest {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            email: row.get("email")?,
            dsp: row.get("dsp_name")?,
            message: row.get("message")?,
            status: row.get("status")?,
            received_at: row.get("received_at")?,
        })
    }
}

impl Store {
    /// Keeps what a visitor asked and emails the platform owners, the first time the site
    /// sends it: one it sends again, as after an answer it never got, changes nothing.
    pub fn keep_demo_request(&self, asked: &Asked) -> Result<()> {
        let known = self.platform.one(
            "SELECT 1 FROM demo_requests WHERE submission=?",
            [&asked.submission],
        )?;
        if known.is_some() {
            return Ok(());
        }
        self.throttle("demo-requests:all", HOURLY, HOUR_MS)?;
        self.platform.exec(
            "INSERT INTO demo_requests(id,submission,name,email,dsp_name,message,received_at) \
             VALUES (?,?,?,?,?,?,?)",
            params![
                crypto::id("dmr")?,
                asked.submission,
                asked.name,
                asked.email,
                asked.dsp,
                asked.message,
                iso()
            ],
        )?;
        self.notify_demo_request(asked);
        Ok(())
    }
    fn notify_demo_request(&self, asked: &Asked) {
        let config = &self.config;
        let subject = format!("{} asked for a demo", asked.dsp);
        let from = format!("{} at {}, {}", asked.name, asked.dsp, asked.email);
        let url = format!("{}/#new-clients", config.origin);
        self.notify_platform_owners(NOTICE, |_| {
            let mut paragraphs = vec![from.as_str()];
            if !asked.message.is_empty() {
                paragraphs.push(&asked.message);
            }
            templates::notice(&templates::Notice {
                origin: &config.origin,
                dev: config.environment.is_preview(),
                subject: &subject,
                preheader: &from,
                heading: "New demo request",
                paragraphs: &paragraphs,
                action: Some(("Open New Clients", &url)),
                note: "",
                footer: "You're getting this because you're a platform owner of Dispatch.",
            })
        });
    }
    /// Every request, the newest first.
    pub fn demo_requests(&self) -> Result<Vec<DemoRequest>> {
        self.platform.query_as(
            "SELECT id,name,email,dsp_name,message,status,received_at FROM demo_requests \
             ORDER BY received_at DESC,id DESC",
            [],
        )
    }
    /// How many requests nobody has answered yet.
    pub fn new_demo_requests(&self) -> Result<i64> {
        let new: Option<(i64,)> = self.platform.one_as(
            "SELECT count(*) FROM demo_requests WHERE status=?",
            [DemoStatus::New],
        )?;
        Ok(new.map_or(0, |(count,)| count))
    }
    /// Says where the owner `actor` stands with a request, and notes it in the audit log by the
    /// DSP that asked.
    pub fn set_demo_request_status(&self, actor: &str, id: &str, status: DemoStatus) -> Result<()> {
        self.platform.transaction(|| {
            let dsp = self.demo_request_dsp(id)?;
            self.platform.exec(
                "UPDATE demo_requests SET status=? WHERE id=?",
                params![status, id],
            )?;
            self.audit_with(
                Some(actor),
                None,
                "demo_request.status_changed",
                status.as_str(),
                Some(&dsp),
                &[],
            )
        })
    }
    /// Deletes a request for the owner `actor`, with everything it held, and notes it in the
    /// audit log by the DSP that asked.
    pub fn delete_demo_request(&self, actor: &str, id: &str) -> Result<()> {
        self.platform.transaction(|| {
            let dsp = self.demo_request_dsp(id)?;
            self.platform
                .exec("DELETE FROM demo_requests WHERE id=?", [id])?;
            self.audit_with(
                Some(actor),
                None,
                "demo_request.deleted",
                "",
                Some(&dsp),
                &[],
            )
        })
    }
    /// The name of the DSP a request came from, as an audit entry names the request.
    fn demo_request_dsp(&self, id: &str) -> Result<String> {
        let dsp: Option<(String,)> = self
            .platform
            .one_as("SELECT dsp_name FROM demo_requests WHERE id=?", [id])?;
        dsp.map(|(dsp,)| dsp)
            .ok_or_else(|| Error::new("demo_request_not_found", 404))
    }
}
