//! Timecard's part of the agent catalog: its endpoints, its metrics for team_table and
//! the words its answers use.
use super::{MEAL_BREAKS, TIMECARDS, views};
use dispatch_core::mcp::data::catalog::{
    CURSOR, DATE, DAY, DRIVER, DSP, Endpoint, FROM, Kind, LIMIT, Metric, PERIOD, Param, TO, Term,
};
use dispatch_core::mcp::data::schema;

pub const ENDPOINTS: &[Endpoint] = &[
    Endpoint {
        id: "timecards",
        tool: "timecards",
        area: Some(TIMECARDS),
        path: "/api/v1/timecards",
        summary: "Timecards",
        description: "Use for Paycom hours and punches: everyone's for one day, or one \
            driver's over a period with driver. Hours, clock in and out, lunch minutes.",
        path_params: &[],
        params: &[
            DSP,
            DATE,
            DRIVER,
            Param {
                description: "The days in the DSP's time, as last week, last 14 days or 2026-W39. \
                    Defaults to yesterday for everyone, or the last 30 days for one driver. \
                    At most 92 days.",
                ..PERIOD
            },
            FROM,
            TO,
            LIMIT,
            CURSOR,
        ],
        order: 110,
        output: || {
            schema::answer(
                &[
                    ("hours", schema::nullable(schema::number())),
                    ("coverage", schema::coverage()),
                    (
                        "timecards",
                        schema::table(&[
                            "date",
                            "driver",
                            "hours",
                            "in",
                            "out",
                            "lunch_minutes",
                            "needs_review",
                        ]),
                    ),
                ],
                &["hours", "coverage", "timecards"],
            )
        },
        answer: |db, state, caller, _, query| views::timecards(db, state, caller, query),
    },
    Endpoint {
        id: "meal_breaks",
        tool: "meal_breaks",
        area: Some(MEAL_BREAKS),
        path: "/api/v1/meal-breaks",
        summary: "Meal breaks",
        description: "Use for one day's meal breaks: Cortex's meal break beside Paycom's \
            lunch punches and the comparison's verdict, for each driver Cortex had a route \
            for.",
        path_params: &[],
        params: &[
            DSP,
            DAY,
            Param {
                name: "issues",
                kind: Kind::Boolean,
                description: "Only drivers whose meal break needs a look.",
            },
            LIMIT,
            CURSOR,
        ],
        order: 120,
        output: || {
            schema::answer(
                &[
                    ("collected", schema::boolean()),
                    ("coverage", schema::coverage()),
                    (
                        "drivers",
                        schema::table(&[
                            "driver",
                            "status",
                            "meal",
                            "minutes",
                            "late_clock_in",
                            "long_gap",
                        ]),
                    ),
                ],
                &["collected", "coverage", "drivers"],
            )
        },
        answer: |db, state, caller, _, query| views::meal_breaks(db, state, caller, query),
    },
];

pub const METRICS: &[Metric] = &[
    Metric {
        name: "hours_worked",
        area: TIMECARDS,
        unit: "hours",
        total: "sum",
        description: "Hours on the Paycom timecard.",
    },
    Metric {
        name: "days_worked",
        area: TIMECARDS,
        unit: "days",
        total: "count",
        description: "Days with hours on the Paycom timecard.",
    },
    Metric {
        name: "lunch_minutes",
        area: TIMECARDS,
        unit: "minutes",
        total: "sum",
        description: "Minutes between Paycom's lunch out and lunch in punches.",
    },
    Metric {
        name: "clock_in",
        area: TIMECARDS,
        unit: "time",
        total: "day",
        description: "First clock in, in the DSP's time.",
    },
    Metric {
        name: "clock_out",
        area: TIMECARDS,
        unit: "time",
        total: "day",
        description: "Last clock out, in the DSP's time.",
    },
    Metric {
        name: "meal_issues",
        area: MEAL_BREAKS,
        unit: "days",
        total: "count",
        description: "Days the meal-break comparison found something to look at, among the \
            days the driver had a Cortex route.",
    },
    Metric {
        name: "meal_status",
        area: MEAL_BREAKS,
        unit: "verdict",
        total: "day",
        description: "The meal-break comparison's verdict; see the glossary.",
    },
];

pub const TERMS: &[Term] = &[
    Term {
        term: "meal status: same",
        meaning: "Cortex's meal break and Paycom's lunch punches agree.",
        order: 90,
    },
    Term {
        term: "meal status: different",
        meaning: "They disagree by more than the allowed difference.",
        order: 100,
    },
    Term {
        term: "meal status: missing_lunch",
        meaning: "Cortex has a meal break; Paycom has no lunch punches.",
        order: 110,
    },
    Term {
        term: "meal status: no_flex_meal",
        meaning: "Paycom has lunch punches; Cortex has no meal break. With cortexRoute false, Cortex \
         had no route for the person, so no meal break was expected there.",
        order: 120,
    },
    Term {
        term: "meal status: flex_only",
        meaning: "Only Cortex has the driver that day.",
        order: 130,
    },
    Term {
        term: "meal status: review_punches",
        meaning: "Paycom's punches don't read as a whole day.",
        order: 140,
    },
    Term {
        term: "meal status: review_pairing",
        meaning: "Meal breaks and lunches don't pair up one to one.",
        order: 150,
    },
    Term {
        term: "meal status: missing_data",
        meaning: "A source has nothing for the driver yet.",
        order: 160,
    },
];
