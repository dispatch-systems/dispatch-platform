//! Cortex: meal evidence, the scorecard, daily routes, and short DVIC inspections from Amazon Logistics.
//! Its storage was added to DSPs that already existed, which is the path every later
//! provider takes.
mod collections;
mod connection;
pub mod discovery;
#[cfg(feature = "operator-probes")]
mod probes;

pub use collections::{dvic, meals, routes, scorecard};
/// Measures a day's routes read by a method, with the rows the feature keeping them shapes
/// them into, which only the app's registry holds: the app's tests run it.
#[cfg(feature = "operator-probes")]
pub use probes::measure_route_method;

use collections::dvic::collect::REPORT_HOST;
use discovery::CollectionRequest;
use dispatch_core::collection::registry::Provider;
use dispatch_core::{
    Code, Result,
    collection::{
        browser::{
            Collected, Driver, Pending,
            browseros::{self, NetworkPolicy},
            egress::HostPolicy,
            http::RequestHosts,
        },
        metrics::{Counted, Counts},
        registry,
    },
    db::{Kind, Migration, Migrations, migrations::Apply::Sql},
    foundation::validate as v,
    manifest::{Collection, Collector},
};
use serde_json::Value;
use std::path::Path;

pub const PROVIDER: Provider = Provider::new("cortex");
pub static COLLECTOR: Cortex = Cortex;

/// The application's own host.
pub(crate) const HOST: &str = "logistics.amazon.com";
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
pub(crate) static BROWSER_HOSTS: HostPolicy = HostPolicy {
    allowed: allowed_host,
};
/// Its reads over plain HTTP, with Amazon's cookies. It answers over HTTP/2 and
/// compresses its JSON to a twelfth: every read of a job shares one connection.
pub(crate) static HOSTS: RequestHosts = RequestHosts {
    allowed: own_host,
    cookies: amazon_cookie,
    http2: true,
};
/// S3 pre-signed report downloads: the observed report host only, with no Amazon cookies.
pub(crate) static REPORT_HOSTS: RequestHosts = RequestHosts {
    allowed: report_host,
    cookies: no_cookie,
    http2: false,
};

/// Its database, `cortex/cortex.sqlite` in each DSP's data.
pub const DATABASE: Kind = Kind::new("cortex", 1);
const MIGRATIONS: &[Migrations] = &[Migrations {
    kind: DATABASE,
    list: &[Migration {
        id: 1,
        name: "baseline",
        apply: Sql(include_str!("migrations/cortex/0001_baseline.sql")),
    }],
}];

/// What it reads, meal breaks first.
static COLLECTIONS: [Collection; 4] = [
    Collection {
        job_kind: meals::JOB_KIND,
        schedule: "meal_break",
        label: "Meal breaks",
        unit: "itinerary",
        counted: Counted::Itineraries,
        unconnected: "schedule_meals_required",
    },
    Collection {
        job_kind: scorecard::JOB_KIND,
        schedule: "scorecard",
        label: "Scorecard",
        unit: "row",
        counted: Counted::Rows,
        unconnected: "schedule_scorecard_required",
    },
    Collection {
        job_kind: routes::JOB_KIND,
        schedule: "routes",
        label: "Routes",
        unit: "itinerary",
        counted: Counted::Itineraries,
        unconnected: "schedule_routes_required",
    },
    Collection {
        job_kind: dvic::JOB_KIND,
        schedule: "dvic",
        label: "DVIC",
        unit: "row",
        counted: Counted::Rows,
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
    fn database(&self) -> Kind {
        DATABASE
    }
    fn migrations(&self) -> &'static [Migrations] {
        MIGRATIONS
    }
    // Its connection row commits with its schema: an added collector's storage counts as
    // created only once it holds one.
    fn seed(&self, dsp: &str) -> String {
        format!(
            "{}\n{}",
            registry::identity_seed(dsp, PROVIDER, "cortex-v1"),
            registry::connection_seed(PROVIDER)
        )
    }
    fn marker(&self) -> Option<&'static str> {
        Some("storage.cortex")
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
            Ok(
                Box::new(connection::Driver::new(browser, profile, fixture).await?)
                    as Box<dyn Driver>,
            )
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

/// The error codes Cortex's collections raise and branch on. Their text is the wire format.
pub(crate) mod codes {
    use dispatch_core::Code;

    pub const CORTEX_SOURCE_CHANGED: Code = Code::new("cortex_source_changed");
    pub const ROUTES_MOVE_UNANSWERED: Code = Code::new("routes_move_unanswered");
    pub const CORTEX_CONTENT_INCOMPLETE: Code = Code::new("cortex_content_incomplete");
    pub const CORTEX_SCOPE_MISMATCH: Code = Code::new("cortex_scope_mismatch");
    pub const CORTEX_TIMEZONE_MISMATCH: Code = Code::new("cortex_timezone_mismatch");
    pub const CORTEX_SOURCE_TOO_LARGE: Code = Code::new("cortex_source_too_large");
    pub const CORTEX_STATION_UNAVAILABLE: Code = Code::new("cortex_station_unavailable");
    pub const CORTEX_PROVIDER_AMBIGUOUS: Code = Code::new("cortex_provider_ambiguous");
    pub const CORTEX_INVALID_MEAL_EVIDENCE: Code = Code::new("cortex_invalid_meal_evidence");
    pub const CORTEX_INVALID_IDENTITY: Code = Code::new("cortex_invalid_identity");
    pub const INVALID_CORTEX_SCOPE: Code = Code::new("invalid_cortex_scope");
    pub const DVIC_SOURCE_CHANGED: Code = Code::new("dvic_source_changed");
    pub const SCORECARD_API_UNREADABLE: Code = Code::new("scorecard_api_unreadable");

    /// Every code above.
    pub const ALL: &[Code] = &[
        CORTEX_SOURCE_CHANGED,
        ROUTES_MOVE_UNANSWERED,
        CORTEX_CONTENT_INCOMPLETE,
        CORTEX_SCOPE_MISMATCH,
        CORTEX_TIMEZONE_MISMATCH,
        CORTEX_SOURCE_TOO_LARGE,
        CORTEX_STATION_UNAVAILABLE,
        CORTEX_PROVIDER_AMBIGUOUS,
        CORTEX_INVALID_MEAL_EVIDENCE,
        CORTEX_INVALID_IDENTITY,
        INVALID_CORTEX_SCOPE,
        DVIC_SOURCE_CHANGED,
        SCORECARD_API_UNREADABLE,
    ];
    /// A failed attempt with one of these is queued again while attempts remain.
    pub const RETRYABLE: &[Code] = &[
        CORTEX_SOURCE_CHANGED,
        DVIC_SOURCE_CHANGED,
        CORTEX_CONTENT_INCOMPLETE,
        SCORECARD_API_UNREADABLE,
    ];
}
