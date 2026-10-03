//! What agents can ask of Timecard: Paycom's timecards, and the meal breaks Cortex
//! reports beside Paycom's lunch punches.
mod catalog;
mod facts;
pub mod views;

use crate::{
    agents::Mcp,
    contracts::{AgentArea, AgentSource, DriverSource, ReadSource, ReadToggle},
};

/// Timecard's page, read through its daily timecards or its employee search.
pub const TIMECARDS_SOURCE: AgentSource = AgentSource::new(&ReadSource {
    id: "timecards",
    switch: "Timecard",
    order: 20,
    features: &["timecard.daily", "timecard.employees"],
    key: "timecards",
    fresh: facts::fresh_timecards,
});
pub const MEAL_BREAKS_SOURCE: AgentSource = AgentSource::new(&ReadSource {
    id: "meal_breaks",
    switch: "Timecard · Meal Breaks",
    order: 30,
    features: &["timecard.meal_breaks"],
    key: "mealBreaks",
    fresh: facts::fresh_meals,
});
pub const TIMECARDS: AgentArea = AgentArea::new(&ReadToggle {
    id: "timecards",
    label: "Timecards",
    order: 30,
    source: TIMECARDS_SOURCE,
    with: None,
    names: &[DriverSource::Paycom],
});
/// Cortex's meal breaks name drivers by their transporter IDs, Paycom's lunches by their
/// employee codes.
pub const MEAL_BREAKS: AgentArea = AgentArea::new(&ReadToggle {
    id: "meal_breaks",
    label: "Meal breaks",
    order: 40,
    source: MEAL_BREAKS_SOURCE,
    with: None,
    names: &[DriverSource::Paycom, DriverSource::Amazon],
});

pub const MCP: Mcp = Mcp {
    reads: &[TIMECARDS, MEAL_BREAKS],
    sources: &[TIMECARDS_SOURCE, MEAL_BREAKS_SOURCE],
    endpoints: catalog::ENDPOINTS,
    metrics: catalog::METRICS,
    terms: catalog::TERMS,
    daily: &[&facts::TimecardDays, &facts::MealDays],
};
