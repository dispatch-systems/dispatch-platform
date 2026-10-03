//! Cortex meal evidence as collected. Browser data is untrusted input.
use super::discovery::Scope;
use crate::{Error, Result, db::now, ensure};
use chrono::{NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const JOB_KIND: &str = "cortex.meal_breaks.collect";

fn token(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 256
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:#".contains(&b))
}
fn label(v: &str) -> bool {
    !v.trim().is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Meal {
    pub id: String,
    pub start: i64,
    pub end: Option<i64>,
    pub last_delivery: Option<i64>,
    pub first_delivery: Option<i64>,
    /// The place in the route, from 0, of the stop that held each delivery: how Cortex's
    /// itinerary page names a stop in its address. Captures from before carry none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_delivery_stop: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_delivery_stop: Option<u32>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Itinerary {
    pub id: String,
    pub transporter_id: String,
    pub driver: String,
    pub route: String,
    pub observed_at: i64,
    pub route_complete: bool,
    pub delivery_coverage: Coverage,
    pub meals: Vec<Meal>,
    /// The Cortex page this route was read from. Set by the collector, never by
    /// page content; captures from before links were retained carry none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Complete,
    Unavailable,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture {
    pub scope: Scope,
    pub started_at: i64,
    pub finished_at: i64,
    pub itineraries: Vec<Itinerary>,
}
impl Capture {
    pub fn validate(&self, expected: &Scope) -> Result<()> {
        expected.validate()?;
        ensure(&self.scope == expected, "cortex_scope_mismatch", 502)?;
        ensure(
            self.started_at > 0
                && self.started_at <= self.finished_at
                && self.finished_at <= now() + 60000
                && self.finished_at - self.started_at <= 1800000,
            "invalid_cortex_capture",
            502,
        )?;
        ensure(
            self.itineraries.len() <= 1000,
            "invalid_cortex_capture",
            502,
        )?;
        let date = NaiveDate::parse_from_str(&expected.date, "%Y-%m-%d").unwrap();
        let tz: chrono_tz::Tz = expected.timezone.parse().unwrap();
        let start = tz
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .ok_or_else(|| Error::new("invalid_date", 400))?
            .timestamp_millis();
        let end = start + 48 * 3600000;
        let valid_time =
            |time: i64| time >= start && time < end && time <= self.finished_at + 60000;
        let mut ids = HashSet::new();
        for route in &self.itineraries {
            ensure(
                token(&route.id)
                    && token(&route.transporter_id)
                    && label(&route.driver)
                    && label(&route.route)
                    && ids.insert(&route.id),
                "invalid_cortex_identity",
                502,
            )?;
            ensure(
                route.observed_at >= self.started_at
                    && route.observed_at <= self.finished_at
                    && route.meals.len() <= 16,
                "invalid_cortex_capture",
                502,
            )?;
            if let Some(url) = &route.source_url {
                ensure(
                    crate::validate::source_url(url).is_ok()
                        && url.ends_with(&expected.detail_path(&route.id)),
                    "invalid_cortex_capture",
                    502,
                )?;
            }
            let mut meal_ids = HashSet::new();
            let mut ordered = route.meals.iter().collect::<Vec<_>>();
            ordered.sort_by_key(|meal| meal.start);
            for (index, meal) in ordered.iter().enumerate() {
                ensure(
                    token(&meal.id) && meal_ids.insert(&meal.id) && valid_time(meal.start),
                    "invalid_cortex_meal",
                    502,
                )?;
                if let Some(end) = meal.end {
                    ensure(
                        valid_time(end) && end >= meal.start && end - meal.start <= 12 * 3600000,
                        "invalid_cortex_meal",
                        502,
                    )?;
                }
                ensure(
                    meal.last_delivery
                        .is_none_or(|time| valid_time(time) && time <= meal.start)
                        && meal.first_delivery.is_none_or(|time| {
                            valid_time(time) && meal.end.is_some_and(|end| time >= end)
                        })
                        && (route.delivery_coverage == Coverage::Complete
                            || (meal.last_delivery.is_none() && meal.first_delivery.is_none())),
                    "invalid_cortex_delivery_boundary",
                    502,
                )?;
                // A stop only names where a delivery was, and meal.js reads at most 2,000.
                let stop = |stop: Option<u32>, delivery: Option<i64>| {
                    stop.is_none_or(|stop| delivery.is_some() && stop < 2000)
                };
                ensure(
                    stop(meal.last_delivery_stop, meal.last_delivery)
                        && stop(meal.first_delivery_stop, meal.first_delivery),
                    "invalid_cortex_delivery_boundary",
                    502,
                )?;
                if index > 0 {
                    ensure(
                        ordered[index - 1].end.is_some_and(|end| end <= meal.start),
                        "overlapping_cortex_meals",
                        502,
                    )?;
                }
            }
        }
        Ok(())
    }
}
// Used only by the explicitly configured synthetic provider mode.
pub fn fixture(scope: &Scope) -> Capture {
    let tz: chrono_tz::Tz = scope.timezone.parse().unwrap();
    let date = NaiveDate::parse_from_str(&scope.date, "%Y-%m-%d").unwrap();
    let start = tz
        .from_local_datetime(&date.and_hms_opt(12, 0, 0).unwrap())
        .earliest()
        .unwrap()
        .timestamp_millis();
    let observed = now();
    Capture {
        scope: scope.clone(),
        started_at: observed,
        finished_at: observed,
        itineraries: vec![Itinerary {
            id: "fixture-itinerary".into(),
            transporter_id: "fixture-driver".into(),
            driver: "Fixture Driver".into(),
            route: "CX1".into(),
            observed_at: observed,
            route_complete: true,
            delivery_coverage: Coverage::Complete,
            meals: vec![Meal {
                id: "meal-1".into(),
                start,
                end: Some(start + 1800000),
                last_delivery: Some(start - 300000),
                first_delivery: Some(start + 2100000),
                last_delivery_stop: Some(11),
                first_delivery_stop: Some(12),
            }],
            source_url: None,
        }],
    }
}
