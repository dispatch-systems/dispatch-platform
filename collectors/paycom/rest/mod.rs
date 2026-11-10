//! Paycom's own REST API, for a DSP that connects with an SID and a Token rather than
//! signing in: its roster and punches read over plain HTTP, with no browser. Paycom answers
//! only the addresses a client has allowed, at the data center that holds the client, and
//! only the functions the client switched on.
mod driver;
mod fixture;

pub(crate) use driver::driver;

use crate::codes;
use chrono::{Datelike, NaiveDate, Weekday};
use dispatch_core::{
    Code, Error, Result,
    collection::{
        api::types::{CheckState, ConnectionCheck},
        browser::http::{Http, Refusal, RequestHosts},
    },
    ensure,
    foundation::validate as v,
};
use serde_json::Value;
use std::time::Duration;
use tokio::{
    sync::Mutex,
    time::{Instant, sleep, sleep_until},
};

/// Paycom's data centers, in the order its guide lists them. A client's data is at one.
const DATA_CENTERS: [&str; 3] = [
    "https://api.paycomonline.net",
    "https://api.phx.us-west.paycomonline.net",
    "https://api.paycomdfw.net",
];
/// Where the API starts at each.
const ROOT: &str = "/v4/rest/index.php/";
fn data_center(host: &str) -> bool {
    DATA_CENTERS
        .iter()
        .any(|center| center.strip_prefix("https://") == Some(host))
}
/// Its API's hosts, which carry no cookies: each request carries the SID and Token.
static HOSTS: RequestHosts = RequestHosts {
    allowed: data_center,
    cookies: |_| false,
    http2: false,
};
/// As much as one page of 500 employees takes, with room.
const LIMIT: usize = 4 * 1024 * 1024;
/// The most a page holds, as Paycom allows.
const PAGE: usize = 500;
/// Paycom allows ten calls a second; this keeps to eight.
const SPACING: Duration = Duration::from_millis(125);

/// What a DSP connects to the API with. `period_start` is the first day of one of its pay
/// periods, for a DSP whose timecards were never collected, which has no period to go on from.
pub(crate) struct Credentials {
    pub sid: String,
    pub token: String,
    pub period_start: Option<NaiveDate>,
}
impl Credentials {
    /// Whether these are the API's, rather than a sign-in's.
    pub fn offered(value: &Value) -> bool {
        value.get("sid").is_some()
    }
    pub fn parse(value: &Value) -> Result<Self> {
        v::fields(value, &["sid", "token", "periodStart"])?;
        let secret = |key: &str, max: usize| {
            let text = v::text(value, key, 1, max)?;
            ensure(
                text.bytes().all(|b| b.is_ascii_graphic()),
                "invalid_input",
                400,
            )?;
            Ok::<_, Error>(text.to_owned())
        };
        let period_start = match value.get("periodStart") {
            None | Some(Value::Null) => None,
            Some(date) => {
                let date = date
                    .as_str()
                    .ok_or_else(|| Error::new("invalid_input", 400))?;
                v::date(date)?;
                let date: NaiveDate = date.parse().map_err(|_| Error::new("invalid_input", 400))?;
                // Its periods run Sunday to Saturday, as the timecards it shows do.
                ensure(date.weekday() == Weekday::Sun, "invalid_period", 400)?;
                Some(date)
            }
        };
        Ok(Self {
            sid: secret("sid", 200)?,
            token: secret("token", 256)?,
            period_start,
        })
    }
}

/// How Paycom answered a request.
pub(crate) enum Answered {
    Rows(Page),
    /// The SID and Token, or the address the request came from, aren't the client's here.
    Refused,
    /// The client hasn't switched the function on.
    Off,
    /// Nothing is at that address.
    Missing,
}
pub(crate) struct Page {
    pub rows: Vec<Value>,
    /// Whether Paycom said more follows.
    partial: bool,
    /// How many there are in all, when it said.
    total: Option<usize>,
}
impl Answered {
    /// Its rows, or why it has none.
    pub fn rows(self) -> Result<Vec<Value>> {
        match self {
            Self::Rows(page) => Ok(page.rows),
            Self::Refused => Err(raise(codes::API_REFUSED, 409)),
            Self::Off => Err(raise(codes::API_FUNCTION_OFF, 409)),
            Self::Missing => Err(raise(codes::API_UNREADABLE, 502)),
        }
    }
}

pub(crate) fn raise(code: Code, status: u16) -> Error {
    Error::new(code.as_str(), status)
}
pub(crate) fn unreadable() -> Error {
    raise(codes::API_UNREADABLE, 502)
}

/// The rows of an answer: a list, or one under `data`, or a single record.
fn rows(text: &str) -> Result<Vec<Value>> {
    match serde_json::from_str(text).map_err(|_| unreadable())? {
        Value::Array(rows) => Ok(rows),
        Value::Object(mut answer) => {
            ensure(
                answer.get("result") != Some(&Value::Bool(false)),
                codes::API_UNREADABLE.as_str(),
                502,
            )?;
            match answer.remove("data") {
                Some(Value::Array(rows)) => Ok(rows),
                Some(row @ Value::Object(_)) => Ok(vec![row]),
                Some(Value::Null) => Ok(Vec::new()),
                Some(_) => Err(unreadable()),
                None => Ok(vec![Value::Object(answer)]),
            }
        }
        _ => Err(unreadable()),
    }
}

/// One data center, asked as one client.
pub(crate) struct Client {
    http: Http,
    base: String,
    authorization: (String, String),
    /// When the next request may start.
    pace: Mutex<Instant>,
}
impl Client {
    /// The client of `origin`, one of Paycom's data centers or the local stand-in for one.
    fn new(origin: &str, credentials: &Credentials) -> Result<Self> {
        Ok(Self {
            http: Http::detached(origin, &HOSTS)?,
            base: format!("{}{ROOT}", origin.trim_end_matches('/')),
            authorization: Http::basic(&credentials.sid, &credentials.token),
            pace: Mutex::new(Instant::now()),
        })
    }
    /// What Paycom answers a GET of `path`, under the API's root. A request it throttled or
    /// failed is sent once more.
    pub async fn get(&self, path: &str) -> Result<Answered> {
        let url = format!("{}{path}", self.base);
        let headers = std::slice::from_ref(&self.authorization);
        let mut again = true;
        let answer = loop {
            {
                let mut next = self.pace.lock().await;
                sleep_until(*next).await;
                *next = Instant::now() + SPACING;
            }
            match self.http.answer(&url, headers, LIMIT).await {
                Ok(answer) => break answer,
                Err(Refusal::Unavailable) if again => {
                    again = false;
                    sleep(Duration::from_secs(2)).await;
                }
                Err(Refusal::Unavailable) => {
                    return Err(Error::new("provider_unavailable", 502));
                }
                Err(Refusal::Unreadable(_)) => return Err(unreadable()),
            }
        };
        match answer.status {
            200 | 206 => Ok(Answered::Rows(Page {
                rows: rows(&answer.body)?,
                partial: answer.status == 206,
                total: answer
                    .header("X-Total-Count")
                    .and_then(|total| total.trim().parse().ok()),
            })),
            401 => Ok(Answered::Refused),
            // Its guide lists "Forbidden (endpoint is not enabled)" against 413.
            403 | 413 => Ok(Answered::Off),
            404 => Ok(Answered::Missing),
            _ => Err(unreadable()),
        }
    }
    /// Every row of a list Paycom pages, at a `path` that carries a query, read page by page.
    pub async fn all(&self, path: &str) -> Result<Vec<Value>> {
        let mut rows = Vec::new();
        // Twenty thousand rows: four times the most a roster may hold.
        for page in 1..=40 {
            let answer = self
                .get(&format!("{path}&pagesize={PAGE}&page={page}"))
                .await?;
            let page = match answer {
                Answered::Rows(page) => page,
                // A page past the last one.
                Answered::Missing if page > 1 => return Ok(rows),
                other => return other.rows(),
            };
            let count = page.rows.len();
            rows.extend(page.rows);
            // The last page: one Paycom calls whole, an empty one, one that reaches the total
            // it gave, or a short one when it gave none.
            let last = match page.total {
                Some(total) => rows.len() >= total,
                None => count < PAGE,
            };
            if !page.partial || count == 0 || last {
                return Ok(rows);
            }
        }
        Err(unreadable())
    }
}

/// What a connection to the API asks of Paycom as it connects, in the order the card lists
/// them: that it takes the SID and Token, each function a collection reads through, and a
/// pay period to count from.
pub(crate) const CREDENTIALS: &str = "credentials";
pub(crate) const EMPLOYEE_DIRECTORY: &str = "employee_directory";
pub(crate) const EMPLOYEE: &str = "employee";
pub(crate) const PUNCH_HISTORY: &str = "punch_history";
pub(crate) const CATEGORIES: &str = "labor_allocation_categories";
pub(crate) const PAY_PERIOD: &str = "pay_period";
const CHECKS: [&str; 6] = [
    CREDENTIALS,
    EMPLOYEE_DIRECTORY,
    EMPLOYEE,
    PUNCH_HISTORY,
    CATEGORIES,
    PAY_PERIOD,
];

/// How each of them went, the last time a connection asked.
pub(crate) struct Checks(Vec<ConnectionCheck>);
impl Checks {
    /// Each still waiting to be asked.
    pub fn waiting() -> Self {
        Self(
            CHECKS
                .iter()
                .map(|id| ConnectionCheck::waiting(id))
                .collect(),
        )
    }
    pub fn list(&self) -> Vec<ConnectionCheck> {
        self.0.clone()
    }
    fn set(&mut self, id: &str, state: CheckState, error: Option<Code>) {
        let check = self
            .0
            .iter_mut()
            .find(|check| check.id == id)
            .expect("a check the connection lists");
        check.state = state;
        check.error = error.map(|code| code.as_str().to_owned());
    }
    pub fn pass(&mut self, id: &str) {
        self.set(id, CheckState::Passed, None);
    }
    pub fn fail(&mut self, id: &str, code: Code) {
        self.set(id, CheckState::Failed, Some(code));
    }
    /// Passes or fails `id` by how Paycom answered the function it reads through.
    pub fn answered(&mut self, id: &str, answer: &Answered) {
        match answer {
            Answered::Rows(_) => self.pass(id),
            Answered::Refused => self.fail(id, codes::API_REFUSED),
            Answered::Off => self.fail(id, codes::API_FUNCTION_OFF),
            Answered::Missing => self.fail(id, codes::API_UNREADABLE),
        }
    }
    /// Ready when every check passed; else the first failure's code, or that the roster is
    /// empty, which leaves nobody to ask the employee functions about.
    pub fn outcome(&self) -> Result<()> {
        if let Some(code) = self.0.iter().find_map(|check| check.error.as_deref()) {
            return Err(Error::new(code, 409));
        }
        ensure(
            self.0.iter().all(|check| check.state == CheckState::Passed),
            "roster_not_complete",
            409,
        )
    }
}

#[cfg(test)]
#[path = "../tests/backend/rest.rs"]
mod tests;
