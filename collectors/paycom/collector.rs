//! Paycom: employees and timecards. Every DSP was created with this storage.
#[path = "collections/timecards/checkpoint.rs"]
pub mod checkpoint;
#[path = "codes.rs"]
pub mod codes;
#[path = "fixtures/timecards.rs"]
pub mod fixtures;
#[path = "collections/timecards/types.rs"]
pub mod timecards;
#[path = "collections/timecards/validation.rs"]
pub mod validation;

use super::Provider;
use crate::{
    Code, Error, Result,
    browsers::{
        Collected, Driver, Pending,
        browseros::{self, NetworkPolicy},
        egress::HostPolicy,
        http::RequestHosts,
        paycom,
    },
    db::{Db, Kind, Store, iso, s},
    ensure,
    job_metrics::Counts,
    manifest::{Collection, Collector},
    validate as v,
};
use serde_json::Value;
use std::{collections::HashSet, path::Path};
use timecards::EmployeeSync;

pub const PROVIDER: Provider = Provider::new("paycom");
pub static COLLECTOR: Paycom = Paycom;

/// What its browser may open: Paycom's own hosts, and the fonts and reCAPTCHA its
/// sign-in pages load.
pub fn allowed_host(host: &str) -> bool {
    host.bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        && (host == "paycomonline.net"
            || host.ends_with(".paycomonline.net")
            || [
                "www.paycom.com",
                "fonts.googleapis.com",
                "fonts.gstatic.com",
                "www.google.com",
                "www.gstatic.com",
                "www.recaptcha.net",
            ]
            .contains(&host))
}
// Its own hosts, where its pages are read over plain HTTP.
fn own_host(host: &str) -> bool {
    allowed_host(host) && (host == "paycomonline.net" || host.ends_with(".paycomonline.net"))
}
pub static BROWSER_HOSTS: HostPolicy = HostPolicy {
    allowed: allowed_host,
};
/// Its reads over plain HTTP: its own hosts, with the browser's cookies for them, over
/// uncompressed HTTP/1.1 as measured.
pub static HOSTS: RequestHosts = RequestHosts {
    allowed: own_host,
    cookies: own_host,
    http2: false,
};

/// What it reads: the timecards of a pay period, or of one employee's.
static COLLECTIONS: [Collection; 1] = [Collection {
    job_kind: timecards::JOB_KIND,
    schedule: "paycom",
    unconnected: "schedule_paycom_required",
}];

pub struct Paycom;
impl Collector for Paycom {
    fn id(&self) -> &'static str {
        PROVIDER.id()
    }
    fn label(&self) -> &'static str {
        "Paycom"
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["timecards"]
    }
    fn collections(&self) -> &'static [Collection] {
        &COLLECTIONS
    }
    fn database(&self) -> Kind {
        Kind::Paycom
    }
    fn seed(&self, dsp: &str) -> String {
        format!("INSERT INTO storage_identity VALUES ('{dsp}','paycom','paycom-v1');")
    }
    fn marker(&self) -> Option<&'static str> {
        None
    }
    fn opened(&self, store: &Store, dsp: &str) -> Result<()> {
        store.prune_checkpoints(dsp)
    }
    fn provision(&self, store: &Store, dsp: &str, timezone: &str) -> Result<()> {
        let db = store.collector(dsp, PROVIDER)?;
        db.exec(
            "INSERT OR IGNORE INTO connections(provider,updated_at) VALUES ('paycom',?)",
            [iso()],
        )?;
        // v0.0.9 reads this row on every Paycom status request. Drop it, and the
        // table, once a release without that reader has shipped.
        db.exec(
            "INSERT OR IGNORE INTO schedules(provider,timezone) VALUES ('paycom',?)",
            [timezone],
        )?;
        Ok(())
    }
    fn browser_entries(&self) -> &'static [&'static str] {
        &[
            "paycom", // Retired profile retained on existing hosts.
            "paycom-browseros",
            "paycom-attempt.json",
            "paycom-diagnostics.json",
            ".paycom-browseros.browseros.lock",
        ]
    }
    fn network(&self) -> NetworkPolicy {
        NetworkPolicy::Hosts(&BROWSER_HOSTS)
    }
    fn validate_credentials(&self, value: &Value) -> Result<()> {
        v::fields(
            value,
            &["clientCode", "username", "password", "securityAnswers"],
        )?;
        v::name(value, "clientCode", 80)?;
        v::name(value, "username", 200)?;
        v::text(value, "password", 1, 256)?;
        let answers = value["securityAnswers"]
            .as_array()
            .ok_or_else(|| Error::new("invalid_input", 400))?;
        ensure(
            answers.len() == 5
                && answers.iter().all(|a| {
                    a.as_str().is_some_and(|s| {
                        !s.is_empty() && s.chars().count() <= 64 && !s.contains(['\r', '\n', '\0'])
                    })
                })
                && answers
                    .iter()
                    .map(Value::to_string)
                    .collect::<HashSet<_>>()
                    .len()
                    == 5,
            "invalid_input",
            400,
        )
    }
    fn account_label<'a>(&self, credentials: &'a Value) -> &'a str {
        s(credentials, "clientCode")
    }
    fn driver<'a>(
        &self,
        browser: browseros::Session,
        profile: &'a Path,
        fixture: Option<&'a str>,
    ) -> Pending<'a, Box<dyn Driver>> {
        Box::pin(async move {
            Ok(Box::new(paycom::Driver::new(browser, profile, fixture).await?) as Box<dyn Driver>)
        })
    }
    fn fixture(&self, timezone: &str, request: &Value) -> Result<Collected> {
        let data = if let Some(scope) = EmployeeSync::parse(request)? {
            scope.fixture(timezone)?
        } else {
            fixtures::fixture_date(timezone, validation::collection_date(request, timezone)?)?
        };
        Ok(Collected { data, scope: None })
    }
    fn progress(&self, _: &Value) -> &'static str {
        "Collecting workforce"
    }
    fn counts(&self, data: &Value) -> Counts {
        Counts {
            employees: data["employees"].as_array().map(Vec::len),
            timecards: data["timecards"].as_array().map(Vec::len),
            ..Counts::default()
        }
    }
    fn roster(&self) -> bool {
        true
    }
    fn codes(&self) -> &'static [Code] {
        codes::ALL
    }
    fn discard(&self, store: &Store, dsp: &str, job: Option<&str>) -> Result<()> {
        store.clear_checkpoint(dsp, job)
    }
    fn prune(&self, store: &Store, dsp: &str) -> Result<()> {
        store.prune_checkpoints(dsp)
    }
    // v0.0.9 refuses Paycom settings saves while its old schedule row is on
    // and Paycom is disconnected. Drop this with the table.
    fn disabled(&self, db: &Db) -> Result<()> {
        db.exec(
            "UPDATE schedules SET enabled=0,next_run=NULL WHERE provider=?",
            [self.id()],
        )?;
        Ok(())
    }
}
