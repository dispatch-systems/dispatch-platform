//! Cortex: meal evidence, the scorecard, daily routes, and short DVIC inspections from Amazon Logistics.
//! Its storage was added to DSPs that already existed, which is the path every later
//! provider takes.
#[path = "codes.rs"]
pub mod codes;
#[path = "discovery/scope.rs"]
pub mod discovery;
#[path = "collections/dvic/capture.rs"]
pub mod dvic;
#[path = "collections/meals/live.rs"]
pub mod live;
#[path = "collections/meals/capture.rs"]
pub mod meals;
#[path = "collections/routes/capture.rs"]
pub mod routes;
#[path = "collections/scorecard/capture.rs"]
pub mod scorecard;

use super::{AddedStorage, Provider};
use crate::{
    Code, Result,
    browsers::{
        Collected, Driver, Pending,
        browseros::{self, NetworkPolicy},
        cortex::{self, dvic::REPORT_HOST},
        egress::HostPolicy,
        http::RequestHosts,
    },
    db::{self, Db, Kind},
    ensure,
    job_metrics::Counts,
    manifest::{Collection, Collector},
    validate as v,
};
use discovery::CollectionRequest;
use serde_json::{Value, json};
use std::path::Path;

pub const PROVIDER: Provider = Provider::new("cortex");
pub static COLLECTOR: Cortex = Cortex;

/// The application's own host.
pub const HOST: &str = "logistics.amazon.com";
/// What its browser may open: Amazon's sign-in and the application, the report host its
/// downloads come from, and the media hosts its pages load.
pub fn allowed_host(host: &str) -> bool {
    [
        HOST,
        REPORT_HOST,
        "amazon.com",
        "www.amazon.com",
        "unagi.amazon.com",
        "unagi-na.amazon.com",
    ]
    .contains(&host)
        || ["media-amazon.com", "ssl-images-amazon.com"]
            .iter()
            .any(|root| host == *root || host.ends_with(&format!(".{root}")))
}
// Only the application's own host: its data API lives there.
fn own_host(host: &str) -> bool {
    allowed_host(host) && host == HOST
}
fn amazon_cookie(domain: &str) -> bool {
    domain == HOST || domain == "amazon.com" || domain.ends_with(".amazon.com")
}
fn report_host(host: &str) -> bool {
    host == REPORT_HOST && allowed_host(host)
}
fn no_cookie(_: &str) -> bool {
    false
}
pub static BROWSER_HOSTS: HostPolicy = HostPolicy {
    allowed: allowed_host,
};
/// Its reads over plain HTTP, with Amazon's cookies. It answers over HTTP/2 and
/// compresses its JSON to a twelfth: every read of a job shares one connection.
pub static HOSTS: RequestHosts = RequestHosts {
    allowed: own_host,
    cookies: amazon_cookie,
    http2: true,
};
/// S3 pre-signed report downloads: the observed report host only, with no Amazon cookies.
pub static REPORT_HOSTS: RequestHosts = RequestHosts {
    allowed: report_host,
    cookies: no_cookie,
    http2: false,
};

/// What it reads, meal breaks first.
static COLLECTIONS: [Collection; 4] = [
    Collection {
        job_kind: meals::JOB_KIND,
        schedule: "meal_break",
        unconnected: "schedule_meals_required",
    },
    Collection {
        job_kind: scorecard::JOB_KIND,
        schedule: "scorecard",
        unconnected: "schedule_scorecard_required",
    },
    Collection {
        job_kind: routes::JOB_KIND,
        schedule: "routes",
        unconnected: "schedule_routes_required",
    },
    Collection {
        job_kind: dvic::JOB_KIND,
        schedule: "dvic",
        unconnected: "schedule_dvic_required",
    },
];

pub struct Cortex;
impl Collector for Cortex {
    fn id(&self) -> &'static str {
        PROVIDER.id()
    }
    fn label(&self) -> &'static str {
        "Cortex"
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["meal_breaks", "routes", "dvic", "scorecard"]
    }
    fn collections(&self) -> &'static [Collection] {
        &COLLECTIONS
    }
    fn job_kind_for(&self, request: &Value) -> &'static str {
        if dvic::Request::is(request) {
            dvic::JOB_KIND
        } else if scorecard::Request::is(request) {
            scorecard::JOB_KIND
        } else if routes::Request::is(request) {
            routes::JOB_KIND
        } else {
            meals::JOB_KIND
        }
    }
    fn added_storages(&self) -> &'static [&'static AddedStorage] {
        static ADDED: [&AddedStorage; 3] = [
            &crate::scorecard::STORAGE,
            &crate::routedata::STORAGE,
            &crate::dvic::STORAGE,
        ];
        &ADDED
    }
    fn database(&self) -> Kind {
        Kind::Cortex
    }
    fn seed(&self, dsp: &str) -> String {
        format!(
            "INSERT INTO storage_identity VALUES ('{dsp}','cortex','cortex-v1');\nINSERT \
                INTO connections(provider,updated_at) VALUES ('cortex','{}');",
            db::iso()
        )
    }
    fn marker(&self) -> Option<&'static str> {
        Some("storage.cortex")
    }
    fn verify(&self, db: &Db) -> Result<()> {
        ensure(
            db.all("SELECT version FROM meal_schema", [])? == vec![json!({"version":1})],
            "unsupported_cortex_schema",
            503,
        )?;
        // Missing initialized feature tables fail closed, rather than recreating lost data.
        // meal_delivery_events, meal_breaks and their trigger hold nothing and are not
        // required here. The baseline still creates them because v0.0.9 refuses to start
        // without them.
        for table in ["meal_publications", "meal_itineraries"] {
            db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
        }
        ensure(
            db.all("SELECT version FROM meal_record_schema", [])? == vec![json!({"version":1})],
            "unsupported_cortex_schema",
            503,
        )?;
        db.one("SELECT count(*) FROM meal_records WHERE 0", [])?;
        Ok(())
    }
    fn browser_entries(&self) -> &'static [&'static str] {
        &[
            "cortex-browseros",
            "cortex-attempt.json",
            ".cortex-browseros.browseros.lock",
        ]
    }
    fn network(&self) -> NetworkPolicy {
        NetworkPolicy::Hosts(&BROWSER_HOSTS)
    }
    fn validate_credentials(&self, value: &Value) -> Result<()> {
        v::fields(value, &["username", "password"])?;
        v::name(value, "username", 200)?;
        v::text(value, "password", 1, 256)?;
        Ok(())
    }
    fn driver<'a>(
        &self,
        browser: browseros::Session,
        profile: &'a Path,
        fixture: Option<&'a str>,
    ) -> Pending<'a, Box<dyn Driver>> {
        Box::pin(async move {
            Ok(Box::new(cortex::Driver::new(browser, profile, fixture).await?) as Box<dyn Driver>)
        })
    }
    fn fixture(&self, _: &str, request: &Value) -> Result<Collected> {
        if let Some(request) = dvic::Request::parse(request)? {
            let scope = match request.scope_request() {
                CollectionRequest::Discover(discovery) => {
                    discovery.scope("area-fixture", "company-fixture")?
                }
                CollectionRequest::Scoped(scope) => scope,
            };
            return Ok(Collected {
                data: serde_json::to_value(dvic::fixture(&request)?)?,
                scope: Some(serde_json::to_value(scope)?),
            });
        }
        if let Some(request) = scorecard::Request::parse(request)? {
            let scope = match request.scope_request() {
                CollectionRequest::Discover(discovery) => {
                    discovery.scope("area-fixture", "company-fixture")?
                }
                CollectionRequest::Scoped(scope) => scope,
            };
            return Ok(Collected {
                data: serde_json::to_value(scorecard::fixture(&request)?)?,
                scope: Some(serde_json::to_value(scope)?),
            });
        }
        if let Some(request) = routes::Request::parse(request)? {
            let capture = routes::fixture(&request)?;
            let scope = serde_json::to_value(&capture.scope)?;
            return Ok(Collected {
                data: serde_json::to_value(capture)?,
                scope: Some(scope),
            });
        }
        let scope = match serde_json::from_value(request.clone())? {
            CollectionRequest::Scoped(scope) => scope,
            CollectionRequest::Discover(discovery) => {
                discovery.scope("area-demo", "provider-demo")?
            }
        };
        Ok(Collected {
            data: serde_json::to_value(meals::fixture(&scope))?,
            scope: Some(serde_json::to_value(scope)?),
        })
    }
    fn progress(&self, request: &Value) -> &'static str {
        if dvic::Request::is(request) {
            "Collecting DVIC"
        } else if scorecard::Request::is(request) {
            "Collecting scorecard"
        } else if routes::Request::is(request) {
            "Collecting routes"
        } else {
            "Collecting meal breaks"
        }
    }
    fn counts(&self, data: &Value) -> Counts {
        let total_rows = |items: &Value| {
            items.as_array().map(|items| {
                items
                    .iter()
                    .map(|item| item["rows"].as_array().map_or(0, Vec::len))
                    .sum()
            })
        };
        Counts {
            itineraries: data["itineraries"].as_array().map(Vec::len),
            meals: data["itineraries"].as_array().map(|rows| {
                rows.iter()
                    .map(|r| r["meals"].as_array().map_or(0, Vec::len))
                    .sum()
            }),
            rows: total_rows(&data["reports"]).or_else(|| total_rows(&data["datasets"])),
            ..Counts::default()
        }
    }
    fn codes(&self) -> &'static [Code] {
        codes::ALL
    }
    fn retryable(&self) -> &'static [Code] {
        codes::RETRYABLE
    }
}
