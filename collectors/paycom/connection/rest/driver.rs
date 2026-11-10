//! The driver of a connection to Paycom's API: no browser, nothing to sign in to and nothing
//! for a member to verify. Connecting finds the data center that takes the SID and Token and
//! asks each function a collection reads through once, so the card can say which is missing.
use super::{
    Answered, CATEGORIES, CREDENTIALS, Checks, Client, Credentials, DATA_CENTERS, EMPLOYEE,
    EMPLOYEE_DIRECTORY, PAY_PERIOD, PUNCH_HISTORY, fixture,
};
use crate::{
    codes,
    timecards::{self, api},
};
use chrono::NaiveDate;
use dispatch_core::{
    Error, Result,
    collection::{
        api::types::ConnectionCheck,
        browser::{Collected, Driver as Drives, Pending, Run, browseros},
    },
    db::{self, Store, s},
    ensure,
    manifest::registry,
};
use serde_json::{Value, json};

/// Its site, where a timecard's link opens.
const SITE: &str = "https://www.paycomonline.net";

/// The driver for a DSP's API `credentials`. A server with fixture providers and no stand-in
/// to reach answers from the fixture instead.
pub(crate) fn driver(store: &Store, dsp: &str, credentials: &Value) -> Result<Box<dyn Drives>> {
    // Not offered to DSPs until its hours are proven against the totals Paycom itself shows.
    ensure(
        !store.config.environment.is_production(),
        codes::API_UNAVAILABLE.as_str(),
        409,
    )?;
    let credentials = Credentials::parse(credentials)?;
    if store.config.fixture && store.config.fixture_url.is_none() {
        return Ok(Box::new(fixture::Driver::new(credentials)));
    }
    // A period Paycom's own pages named counts for more than one a member typed.
    let kept =
        registry()
            .keeper(timecards::JOB_KIND)
            .kept(store, dsp, &json!({"latestPeriod":true}))?;
    let anchor = match s(&kept, "from") {
        "" => credentials.period_start,
        from => Some(
            from.parse::<NaiveDate>()
                .map_err(|_| Error::new("invalid_period", 502))?,
        ),
    };
    let (site, data_centers) = match &store.config.fixture_url {
        Some(stand_in) => {
            let stand_in = stand_in.trim_end_matches('/').to_owned();
            (stand_in.clone(), vec![stand_in])
        }
        None => (
            SITE.to_owned(),
            DATA_CENTERS
                .iter()
                .map(|&center| center.to_owned())
                .collect(),
        ),
    };
    Ok(Box::new(Driver::new(
        credentials,
        site,
        data_centers,
        anchor,
    )))
}

pub(crate) struct Driver {
    credentials: Credentials,
    site: String,
    data_centers: Vec<String>,
    /// The first day of one of the DSP's pay periods, which the others are counted from.
    anchor: Option<NaiveDate>,
    checks: Checks,
    connected: Option<Connected>,
}
/// What connecting found: the data center that answers, and which of its labor allocation
/// categories holds each employee's delivery station.
struct Connected {
    client: Client,
    station: usize,
}

/// The code of the first employee a directory page lists, when it is one a path may carry.
fn listed(rows: &[Value]) -> Option<&str> {
    rows.first()
        .map(|row| s(row, "eecode"))
        .filter(|code| api::path_code(code))
}

impl Driver {
    /// A driver that reaches `data_centers` with `credentials`, its pay periods counted from
    /// `anchor`, not yet connected.
    fn new(
        credentials: Credentials,
        site: String,
        data_centers: Vec<String>,
        anchor: Option<NaiveDate>,
    ) -> Self {
        Self {
            credentials,
            site,
            data_centers,
            anchor,
            checks: Checks::waiting(),
            connected: None,
        }
    }
    /// A driver for Paycom itself, as an operator's probe connects: with an SID and Token
    /// given to it, and the period it measures to count from.
    #[cfg(feature = "operator-probes")]
    pub(crate) fn live(credentials: Credentials, anchor: NaiveDate) -> Self {
        Self::new(
            credentials,
            SITE.to_owned(),
            DATA_CENTERS
                .iter()
                .map(|&center| center.to_owned())
                .collect(),
            Some(anchor),
        )
    }
    /// How each check went, the last time it connected.
    pub(crate) fn asked(&self) -> Vec<ConnectionCheck> {
        self.checks.list()
    }
    /// Where a collection reads from, once it has connected.
    pub(crate) fn source(&self) -> Option<api::Source<'_>> {
        let (connected, anchor) = (self.connected.as_ref()?, self.anchor?);
        Some(api::Source {
            client: &connected.client,
            station: connected.station,
            anchor,
            site: &self.site,
        })
    }
    /// Connects, asking each check in turn; ready only when every one passed.
    pub(crate) async fn connect(&mut self) -> Result<()> {
        self.connected = None;
        self.checks = Checks::waiting();
        // The client's data center is the first that doesn't refuse its SID and Token.
        let mut found = None;
        for center in &self.data_centers {
            let client = Client::new(center, &self.credentials)?;
            match client.get("api/v1/employeedirectory?pagesize=1").await? {
                Answered::Refused => continue,
                directory => {
                    found = Some((client, directory));
                    break;
                }
            }
        }
        let Some((client, directory)) = found else {
            self.checks.fail(CREDENTIALS, codes::API_REFUSED);
            return self.checks.outcome();
        };
        self.checks.pass(CREDENTIALS);
        self.checks.answered(EMPLOYEE_DIRECTORY, &directory);
        let sample = match &directory {
            Answered::Rows(page) => listed(&page.rows).map(str::to_owned),
            _ => None,
        };
        // The functions that read one employee are asked about one the directory listed.
        if let Some(code) = sample {
            let employee = client.get(&format!("api/v1/employee/{code}")).await?;
            self.checks.answered(EMPLOYEE, &employee);
            let day = db::now() / 1000;
            let punches = client
                .get(&api::punch_history(&code, day - 86400, day))
                .await?;
            self.checks.answered(PUNCH_HISTORY, &punches);
        }
        let categories = client.get("api/v1/cl/category").await?;
        self.checks.answered(CATEGORIES, &categories);
        let station = match &categories {
            Answered::Rows(page) => {
                let station = api::station_category(&page.rows);
                if station.is_none() {
                    self.checks.fail(CATEGORIES, codes::STATION_MISSING);
                }
                station
            }
            _ => None,
        };
        match self.anchor {
            Some(_) => self.checks.pass(PAY_PERIOD),
            None => self.checks.fail(PAY_PERIOD, codes::PERIOD_REQUIRED),
        }
        self.checks.outcome()?;
        self.connected = Some(Connected {
            client,
            station: station.expect("a station category, as its check passed"),
        });
        Ok(())
    }
}

impl Drives for Driver {
    fn request(&mut self, command: Value) -> Pending<'_, Value> {
        Box::pin(async move {
            match s(&command, "action") {
                "start" | "check" => {
                    self.connect().await?;
                    Ok(json!({"type":"ready"}))
                }
                // Nothing here waits for a member's verification.
                _ => Err(Error::new("verification_expired", 409)),
            }
        })
    }
    fn collect<'a>(&'a mut self, run: &'a Run<'a>) -> Pending<'a, Collected> {
        Box::pin(async move {
            let source = self
                .source()
                .ok_or_else(|| Error::new("verification_required", 409))?;
            let data = api::collect(&source, run).await?;
            Ok(Collected { data, scope: None })
        })
    }
    fn browser(&self) -> Option<&browseros::Session> {
        None
    }
    fn checks(&self) -> Vec<ConnectionCheck> {
        self.asked()
    }
}
