//! The API's driver in fixture mode: no request leaves the server. The Token chooses how
//! connecting goes, as a password does for a sign-in, and a collection returns the fixture's
//! data.
use super::{CREDENTIALS, Checks, Credentials, PUNCH_HISTORY};
use crate::codes;
use dispatch_core::{
    Error,
    collection::{
        api::types::ConnectionCheck,
        browser::{Collected, Driver as Drives, Pending, Run, browseros},
    },
    db::s,
    manifest::Collector,
};
use serde_json::{Value, json};
use std::time::Duration;

/// A Token Paycom refuses, as it does one that is wrong or sent from an address it hasn't
/// allowed.
const REFUSED: &str = "refused-token";
/// A Token whose client left Punch History switched off.
const PUNCH_HISTORY_OFF: &str = "punch-history-off";

pub(super) struct Driver {
    token: String,
    checks: Checks,
}
impl Driver {
    pub fn new(credentials: Credentials) -> Self {
        Self {
            token: credentials.token,
            checks: Checks::waiting(),
        }
    }
}
impl Drives for Driver {
    fn request(&mut self, command: Value) -> Pending<'_, Value> {
        Box::pin(async move {
            if !["start", "check"].contains(&s(&command, "action")) {
                return Err(Error::new("verification_expired", 409));
            }
            self.checks = Checks::waiting();
            if self.token == REFUSED {
                self.checks.fail(CREDENTIALS, codes::API_REFUSED);
            } else {
                for check in self.checks.list() {
                    self.checks.pass(&check.id);
                }
                if self.token == PUNCH_HISTORY_OFF {
                    self.checks.fail(PUNCH_HISTORY, codes::API_FUNCTION_OFF);
                }
            }
            self.checks.outcome()?;
            Ok(json!({"type":"ready"}))
        })
    }
    fn collect<'a>(&'a mut self, run: &'a Run<'a>) -> Pending<'a, Collected> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            run.metrics.method("api");
            crate::COLLECTOR.fixture(run.timezone, run.request)
        })
    }
    fn browser(&self) -> Option<&browseros::Session> {
        None
    }
    fn checks(&self) -> Vec<ConnectionCheck> {
        self.checks.list()
    }
}
