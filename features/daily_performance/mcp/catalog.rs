//! One daily tool can read every dataset with bounded projections and detail pages.
use super::{DAILY_PERFORMANCE, views};
use dispatch_core::mcp::data::{
    catalog::{
        CURSOR, DATE, DETAIL, DRIVER, DSP, Endpoint, FROM, GROUPS_CURSOR, Kind, LIMIT, PERIOD,
        Param, TO,
    },
    schema,
};
pub const DATASETS: &[&str] = &[
    "driver_quality",
    "dsp_quality",
    "driver_returns",
    "dsp_returns",
    "returns_to_station",
    "driver_delivery_behaviors",
    "dsp_delivery_behaviors",
    "driver_feedback",
    "dsp_feedback",
    "driver_pickups",
    "dsp_pickups",
    "driver_contacts",
    "dsp_contacts",
    "driver_thresholds",
    "dsp_thresholds",
    "driver_safety",
    "dsp_safety",
    "safety_events",
    "working_devices",
    "driver_safety_batch",
    "dsp_safety_batch",
    "live_safety_events",
];
pub(super) const DATASET: Param = Param {
    name: "dataset",
    kind: Kind::Choice(DATASETS),
    description: "Daily dataset; defaults to driver_quality. Live safety stays separate.",
};
pub(super) const FIELDS: Param = Param {
    name: "fields",
    kind: Kind::Text,
    description: "Comma-separated source fields for detail, or omit for the dataset's safe fields.",
};
pub(super) const GROUP_BY: Param = Param {
    name: "group_by",
    kind: Kind::Text,
    description: "Count recorded rows by driver, day, reason or type; comma-separated.",
};
pub(super) const REASON: Param = Param {
    name: "reason",
    kind: Kind::Text,
    description: "Return reason, such as business_closed; only returns_to_station.",
};
pub(super) const TYPE: Param = Param {
    name: "type",
    kind: Kind::Text,
    description: "Safety event type or daily feedback category, such as mishandled.",
};
pub(super) const IMPACTING: Param = Param {
    name: "impacting",
    kind: Kind::Boolean,
    description: "Source impact flag, where known; weekly impact is not inferred.",
};
pub(super) const CONTACT: Param = Param {
    name: "contact",
    kind: Kind::Choice(&["missed", "compliant", "all"]),
    description: "Daily return coaching mentions missed call, text or contact.",
};
pub(super) const LIST: Param = Param {
    name: "list",
    kind: Kind::Boolean,
    description: "Include a page of source rows; equivalent to detail full.",
};
pub(super) const COUNTING: Param = Param {
    name: "counting",
    kind: Kind::Boolean,
    description: "Assessed event dispute filter; true excludes approved disputes.",
};
pub(super) const FEEDBACK: Param = Param {
    name: "feedback",
    kind: Kind::Choice(&["negative", "positive", "all"]),
    description: "Daily feedback counts; choose negative, positive or all.",
};
pub(super) const PARAMS: &[Param] = &[
    DSP,
    PERIOD,
    DATE,
    FROM,
    TO,
    DRIVER,
    DATASET,
    FIELDS,
    GROUP_BY,
    REASON,
    TYPE,
    IMPACTING,
    CONTACT,
    LIST,
    COUNTING,
    FEEDBACK,
    DETAIL,
    LIMIT,
    CURSOR,
    GROUPS_CURSOR,
];
pub(super) const FEEDBACK_PARAMS: &[Param] = &[
    DSP,
    PERIOD,
    DATE,
    FROM,
    TO,
    DRIVER,
    FIELDS,
    GROUP_BY,
    TYPE,
    LIST,
    FEEDBACK,
    DETAIL,
    LIMIT,
    CURSOR,
    GROUPS_CURSOR,
];
pub(super) const SAFETY_PARAMS: &[Param] = &[
    DSP,
    PERIOD,
    DATE,
    FROM,
    TO,
    DRIVER,
    FIELDS,
    GROUP_BY,
    TYPE,
    IMPACTING,
    LIST,
    COUNTING,
    DETAIL,
    LIMIT,
    CURSOR,
    GROUPS_CURSOR,
];
pub(super) const RETURNS_PARAMS: &[Param] = &[
    DSP,
    PERIOD,
    DATE,
    FROM,
    TO,
    DRIVER,
    FIELDS,
    GROUP_BY,
    REASON,
    IMPACTING,
    CONTACT,
    LIST,
    DETAIL,
    LIMIT,
    CURSOR,
    GROUPS_CURSOR,
];
pub(super) const ENDPOINTS: &[Endpoint] = &[Endpoint {
    id: "daily_performance",
    tool: "daily_performance",
    area: Some(DAILY_PERFORMANCE),
    path: "/api/v1/daily-performance",
    summary: "Daily Quality and Day Safety",
    description: "Daily source rows, by driver and date or range. Defaults to yesterday. summary counts recorded rows; \
        full returns selected metrics or events. Coverage distinguishes absent data. Feedback is daily counts, not package reviews.",
    path_params: &[],
    params: PARAMS,
    order: 180,
    output,
    answer: |db, state, caller, _, query| views::daily_performance(db, state, caller, query),
}];

pub(super) fn output() -> serde_json::Value {
    schema::answer(
        &[
            (
                "source",
                serde_json::json!({"type":"string","const":"daily_performance"}),
            ),
            ("dataset", serde_json::json!({"type":"string"})),
            ("recorded_rows", schema::count()),
            (
                "coverage",
                schema::object(
                    &[
                        (
                            "status",
                            serde_json::json!({"type":"string","enum":["complete","partial"]}),
                        ),
                        ("observed_days", schema::count()),
                        ("requested_days", schema::count()),
                        ("observed", schema::array(schema::text())),
                        ("note", schema::text()),
                    ],
                    &["status", "observed_days", "requested_days", "observed"],
                ),
            ),
            ("groups", schema::table(&[])),
            ("list", schema::table(&[])),
            (
                "feedback_counts",
                schema::nullable(schema::object(&[], &[])),
            ),
        ],
        &["source", "dataset", "recorded_rows", "coverage"],
    )
}
