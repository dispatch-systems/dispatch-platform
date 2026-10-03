//! A small, process-wide first gate for the public OAuth endpoints. It is deliberately
//! independent of the database: an unauthenticated flood must not acquire a database slot or
//! the exclusive transition lock merely to learn that it has made too many requests.
use crate::{Error, Result, db::now};
use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
};

const WINDOW: i64 = 60_000;
const MOST_SOURCES: usize = 2_000;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Endpoint {
    Authorize,
    Register,
    Token,
    Revoke,
}
impl Endpoint {
    pub fn name(self) -> &'static str {
        match self {
            Self::Authorize => "authorize",
            Self::Register => "register",
            Self::Token => "token",
            Self::Revoke => "revoke",
        }
    }
    fn limits(self) -> (u32, u32) {
        match self {
            // Durable, longer-window limits still apply to authorization and registration.
            Self::Authorize => (30, 300),
            Self::Register => (30, 200),
            Self::Token => (120, 600),
            Self::Revoke => (60, 300),
        }
    }
}

#[derive(Default)]
pub struct Limits(Mutex<Counts>);
#[derive(Default)]
struct Counts {
    sources: HashMap<(Endpoint, String), TrafficWindow>,
    global: HashMap<Endpoint, TrafficWindow>,
    refusals: HashMap<(String, String), CountWindow>,
}
#[derive(Default)]
struct TrafficWindow(VecDeque<i64>);
impl TrafficWindow {
    fn prune(&mut self, at: i64) {
        while self.0.front().is_some_and(|seen| *seen <= at - WINDOW) {
            self.0.pop_front();
        }
    }
    fn full(&self, limit: u32) -> bool {
        self.0.len() >= limit as usize
    }
    fn record(&mut self, at: i64) {
        self.0.push_back(at);
    }
    fn active(&self, at: i64) -> bool {
        self.0.back().is_some_and(|seen| *seen > at - WINDOW)
    }
}
#[derive(Clone, Copy)]
struct CountWindow {
    reset: i64,
    count: u32,
}
impl CountWindow {
    fn current(at: i64) -> Self {
        Self {
            reset: at - at.rem_euclid(WINDOW) + WINDOW,
            count: 0,
        }
    }
    fn refresh(&mut self, at: i64) {
        if self.reset <= at {
            *self = Self::current(at);
        }
    }
}

impl Limits {
    /// Counts a request by endpoint and source before any database work. A global emergency
    /// ceiling also keeps a distributed burst from consuming the whole database queue.
    pub fn admit(&self, endpoint: Endpoint, source: &str) -> Result<()> {
        self.admit_at(endpoint, source, now())
    }

    fn admit_at(&self, endpoint: Endpoint, source: &str, at: i64) -> Result<()> {
        let (per_source, global) = endpoint.limits();
        let mut counts = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        {
            let total = counts.global.entry(endpoint).or_default();
            total.prune(at);
            if total.full(global) {
                return Err(Error::new("rate_limited", 429));
            }
        }

        if counts.sources.len() >= MOST_SOURCES {
            counts.sources.retain(|_, window| window.active(at));
        }
        // The global ceiling bounds how many fresh source entries one live window can make.
        if counts.sources.len() >= MOST_SOURCES
            && !counts.sources.contains_key(&(endpoint, source.to_owned()))
        {
            return Err(Error::new("rate_limited", 429));
        }
        let source = counts
            .sources
            .entry((endpoint, source.to_owned()))
            .or_default();
        source.prune(at);
        if source.full(per_source) {
            return Err(Error::new("rate_limited", 429));
        }
        source.record(at);
        counts.global.get_mut(&endpoint).unwrap().record(at);
        Ok(())
    }

    /// The first few identical protocol refusals each minute are logged, then exponentially
    /// fewer. Request metrics still count every response without turning credential spraying
    /// into unbounded warning logs.
    pub fn sample_refusal(&self, endpoint: &str, error: &str) -> Option<u32> {
        self.sample_refusal_at(endpoint, error, now())
    }

    fn sample_refusal_at(&self, endpoint: &str, error: &str, at: i64) -> Option<u32> {
        let mut counts = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        let entry = counts
            .refusals
            .entry((endpoint.to_owned(), error.to_owned()))
            .or_insert_with(|| CountWindow::current(at));
        entry.refresh(at);
        entry.count = entry.count.saturating_add(1);
        (entry.count <= 5 || entry.count.is_power_of_two()).then_some(entry.count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_and_global_bursts_are_bounded_without_a_database() {
        let limits = Limits::default();
        let at = 1_790_000_000_000;
        for _ in 0..120 {
            limits.admit_at(Endpoint::Token, "one", at).unwrap();
        }
        assert_eq!(
            limits
                .admit_at(Endpoint::Token, "one", at + WINDOW - 1)
                .unwrap_err()
                .code,
            "rate_limited"
        );
        // Other sources may use the remaining global allowance, but no more than it.
        for source in 0..480 {
            limits
                .admit_at(Endpoint::Token, &format!("source-{source}"), at)
                .unwrap();
        }
        assert_eq!(
            limits
                .admit_at(Endpoint::Token, "last", at)
                .unwrap_err()
                .code,
            "rate_limited"
        );
        limits
            .admit_at(Endpoint::Token, "one", at + WINDOW)
            .unwrap();
    }

    #[test]
    fn repeated_refusals_are_sampled_and_reset() {
        let limits = Limits::default();
        let at = 1_790_000_000_000;
        let sampled: Vec<u32> = (0..10)
            .filter_map(|_| limits.sample_refusal_at("token", "invalid_grant", at))
            .collect();
        assert_eq!(sampled, [1, 2, 3, 4, 5, 8]);
        assert_eq!(
            limits.sample_refusal_at("token", "invalid_grant", at + WINDOW),
            Some(1)
        );
    }
}
