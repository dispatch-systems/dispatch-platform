//! Timecard: Paycom's punches and timecards, and the meal breaks Cortex reports.
use crate::manifest::{
    DefaultRole::{Manager, Member},
    Feature, Switch, feature, perm, tab,
};

pub const FEATURE: Feature = Feature {
    switch: Some(Switch {
        id: "timecard",
        label: "Timecard",
        requires: &["timecards", "meal_breaks"],
    }),
    tabs: &[
        tab("timecard.daily", "Timecard"),
        tab("timecard.meal_breaks", "Meal Breaks"),
        tab("timecard.employees", "Employee Search"),
    ],
    schedules: true,
    permissions: &[
        perm("timecard.view", "View Timecard", 20).defaults(&[Manager, Member]),
        perm("timecard.manage", "Manage Timecard", 21).implies(&["timecard.view"]),
        perm("collections.run", "Run Collections", 22).defaults(&[Manager]),
    ],
    // Collection progress refreshes both the timecard pages and the collections page.
    live: &["timecard.view", "collections.run"],
    keeps: &[
        &crate::workforce::keeper::Timecards,
        &crate::meals::keeper::MealBreaks,
    ],
    ..feature("timecard")
};
