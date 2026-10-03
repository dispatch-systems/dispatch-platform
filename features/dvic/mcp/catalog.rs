//! DVIC's part of the agent catalog: its endpoint, its metrics for team_table and the
//! words its answers use.
use super::{DVIC, views};
use crate::agents::data::catalog::{
    CURSOR, DATE, DETAIL, DRIVER, DSP, Endpoint, FROM, Kind, LIMIT, Metric, PERIOD, Param, TO, Term,
};

pub const ENDPOINTS: &[Endpoint] = &[Endpoint {
    id: "dvic",
    tool: "dvic_inspections",
    area: Some(DVIC),
    path: "/api/v1/dvic",
    summary: "Vehicle inspections",
    description: "Use for DVIC questions, as which drivers were short: each driver's \
            inspections, how many were shorter than the minimum, and the shortest. detail \
            full lists the inspections.",
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
            description: "Only drivers, or inspections, short of the minimum.",
        },
        DETAIL,
        LIMIT,
        CURSOR,
    ],
    order: 130,
    answer: |db, state, caller, _, query| views::dvic(db, state, caller, query),
}];

pub const METRICS: &[Metric] = &[
    Metric {
        name: "inspections",
        area: DVIC,
        unit: "inspections",
        total: "sum",
        description: "DVIC inspections done.",
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
