//! What agents can ask of Timecard: Paycom's timecards, and the meal breaks Cortex
//! reports beside Paycom's lunch punches.
mod catalog;
mod facts;
mod synthetic;
pub mod views;

use dispatch_core::mcp::{
    Mcp,
    api::types::{AgentArea, AgentSource, DriverSource, ReadSource, ReadToggle},
};

/// Timecard's page, read through its daily timecards or its employee search.
pub const TIMECARDS_SOURCE: AgentSource = AgentSource::new(&ReadSource {
    id: "timecards",
    switch: "Timecard",
    label: "Timecard",
    order: 20,
    features: &["timecard.daily", "timecard.employees"],
    key: "timecards",
    fresh: facts::fresh_timecards,
});
pub const MEAL_BREAKS_SOURCE: AgentSource = AgentSource::new(&ReadSource {
    id: "meal_breaks",
    switch: "Timecard · Meal Breaks",
    label: "Meal Breaks",
    order: 30,
    features: &["timecard.meal_breaks"],
    key: "mealBreaks",
    fresh: facts::fresh_meals,
});
pub const TIMECARDS: AgentArea = AgentArea::new(&ReadToggle {
    id: "timecards",
    label: "Timecards",
    hint: None,
    missing: "timecards",
    order: 30,
    source: TIMECARDS_SOURCE,
    with: None,
    opt_in: false,
    names: &[DriverSource::Paycom],
});
/// Cortex's meal breaks name drivers by their transporter IDs, Paycom's lunches by their
/// employee codes.
pub const MEAL_BREAKS: AgentArea = AgentArea::new(&ReadToggle {
    id: "meal_breaks",
    label: "Meal breaks",
    hint: None,
    missing: "meal breaks",
    order: 40,
    source: MEAL_BREAKS_SOURCE,
    with: None,
    opt_in: false,
    names: &[DriverSource::Paycom, DriverSource::Amazon],
});

pub const MCP: Mcp = Mcp {
    instructions: "- Timecards default to yesterday for everyone or the last 30 days for one driver. \
        Meal breaks default to yesterday. Timecards and meal comparisons allow up to 92 days.",
    reads: &[TIMECARDS, MEAL_BREAKS],
    missing: "timecard data",
    sources: &[TIMECARDS_SOURCE, MEAL_BREAKS_SOURCE],
    endpoints: catalog::ENDPOINTS,
    metrics: catalog::METRICS,
    terms: catalog::TERMS,
    daily: &[&facts::TimecardDays, &facts::MealDays],
    synthetic: synthetic::SYNTHETIC,
    ..Mcp::NONE
};
