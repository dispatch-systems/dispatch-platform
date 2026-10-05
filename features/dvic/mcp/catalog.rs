//! DVIC's part of the agent catalog: its endpoint, its metrics for team_table, the words
//! its answers use and the question they answer.
use super::{DVIC, views};
use dispatch_core::mcp::data::schema;
use dispatch_core::mcp::{
    data::catalog::{
        CURSOR, DATE, DETAIL, DRIVER, DSP, Endpoint, FROM, Kind, LIMIT, Metric, PERIOD, Param, TO,
        Term,
    },
    skill::Example,
};

pub const ENDPOINTS: &[Endpoint] = &[Endpoint {
    id: "dvic",
    tool: "dvic_inspections",
    area: Some(DVIC),
    path: "/api/v1/dvic",
    summary: "Short vehicle inspections",
    description: "Recorded short DVIC exceptions, by driver and period. detail full pages \
        individual durations. No exception does not prove an inspection was completed.",
    path_params: &[],
    params: &[
        DSP,
        PERIOD,
        DATE,
        FROM,
        TO,
        DRIVER,
        Param {
            name: "short",
            kind: Kind::Boolean,
            description: "Compatibility option; DVIC always returns short inspections.",
        },
        DETAIL,
        LIMIT,
        CURSOR,
    ],
    order: 130,
    output: || {
        schema::answer(
            &[
                ("inspections", schema::nullable(schema::count())),
                ("short", schema::nullable(schema::count())),
                ("coverage", schema::coverage()),
                (
                    "drivers",
                    schema::table(&["driver", "inspections", "short", "shortest_seconds"]),
                ),
                (
                    "list",
                    schema::table(&[
                        "date", "driver", "type", "started", "seconds", "minimum", "short",
                    ]),
                ),
            ],
            &["inspections", "short", "coverage"],
        )
    },
    answer: |db, state, caller, _, query| views::dvic(db, state, caller, query),
}];

pub const METRICS: &[Metric] = &[
    Metric {
        name: "inspections",
        area: DVIC,
        unit: "inspections",
        total: "sum",
        description: "Recorded short DVIC exceptions; alias of short_inspections.",
    },
    Metric {
        name: "short_inspections",
        area: DVIC,
        unit: "inspections",
        total: "sum",
        description: "DVIC inspections shorter than their minimum.",
    },
];

pub const TERMS: &[Term] = &[Term {
    term: "DVIC",
    meaning: "Daily Vehicle Inspection Checklist, done before driving. Inspections under their \
         minimum (90 or 300 seconds by vehicle) count as short.",
    order: 80,
}];

pub const EXAMPLES: &[Example] = &[Example {
    question: "Which drivers were short on their DVIC?",
    call: r#"dvic_inspections(short: true)"#,
    rest: "/api/v1/dvic?short=true",
    order: 40,
}];
