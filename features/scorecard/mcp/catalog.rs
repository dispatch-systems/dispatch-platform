//! Scorecard's part of the agent catalog: its endpoints and the questions they answer.
use super::{FEEDBACK, RETURNS, SAFETY, SCORECARD, scorecard};
use dispatch_core::mcp::data::schema;
use dispatch_core::mcp::{
    data::catalog::{CURSOR, DATE, DRIVER, DSP, Endpoint, FROM, Kind, LIMIT, PERIOD, Param, TO},
    skill::Example,
};

pub const ENDPOINTS: &[Endpoint] = &[
    Endpoint {
        id: "feedback",
        tool: "customer_feedback",
        area: Some(FEEDBACK),
        path: "/api/v1/feedback",
        summary: "Customer feedback (CDF)",
        description: "Use for customer delivery feedback (CDF) from Amazon's weekly scorecard: \
            how much negative feedback, of which kinds, for which drivers, and repeated \
            feedback at the same address (group_by address, min_count 2; needs a key allowed \
            addresses). CDF means negative feedback; ask for positive only when the user asks \
            for praise.",
        path_params: &[],
        params: &[
            DSP,
            PERIOD,
            DATE,
            FROM,
            TO,
            DRIVER,
            Param {
                name: "feedback",
                kind: Kind::Choice(&["negative", "positive", "all"]),
                description: "negative (the default), positive or all.",
            },
            Param {
                name: "type",
                kind: Kind::Choice(scorecard::FEEDBACK_NAMES),
                description: "One kind of feedback, as wrong_address or never_received.",
            },
            Param {
                name: "impacting",
                kind: Kind::Boolean,
                description: "Only feedback that counts against the scorecard.",
            },
            Param {
                name: "group_by",
                kind: Kind::Text,
                description: "Count per driver, address, type, week or day; two may be joined.",
            },
            Param {
                name: "min_count",
                kind: Kind::Integer(1, 100),
                description: "Only groups with at least this many, as 2 for repeated feedback.",
            },
            Param {
                name: "list",
                kind: Kind::Boolean,
                description: "Also list the feedback, a page at a time.",
            },
            LIMIT,
            CURSOR,
        ],
        order: 140,
        output: || {
            schema::answer(
                &[
                    ("feedback", schema::count()),
                    (
                        "coverage",
                        schema::object(&[("weeks", schema::coverage())], &["weeks"]),
                    ),
                    ("unplaced", schema::count()),
                    (
                        "groups",
                        schema::table(&["driver", "address", "type", "week", "day", "feedback"]),
                    ),
                    (
                        "list",
                        schema::table(&[
                            "date",
                            "tracking",
                            "driver",
                            "types",
                            "impacting",
                            "address",
                        ]),
                    ),
                ],
                &["feedback", "coverage"],
            )
        },
        answer: |db, state, caller, _, query| scorecard::feedback(db, state, caller, query),
    },
    Endpoint {
        id: "safety",
        tool: "safety_events",
        area: Some(SAFETY),
        path: "/api/v1/safety",
        summary: "Netradyne safety events",
        description: "Use for Netradyne safety infractions from Amazon's scorecard: speeding, \
            distraction, sign violations, following distance, seatbelt. Gives counts by type; \
            for one driver also each event with its severity and dispute outcome.",
        path_params: &[],
        params: &[
            DSP,
            PERIOD,
            DATE,
            FROM,
            TO,
            DRIVER,
            Param {
                name: "type",
                kind: Kind::Text,
                description: "One kind, as speeding, distraction or seatbelt.",
            },
            Param {
                name: "group_by",
                kind: Kind::Text,
                description: "Count per driver, type, day or week; two may be joined.",
            },
            Param {
                name: "list",
                kind: Kind::Boolean,
                description: "List the events, a page at a time.",
            },
            LIMIT,
            CURSOR,
        ],
        order: 150,
        output: || {
            schema::answer(
                &[
                    ("events", schema::count()),
                    ("counting", schema::count()),
                    (
                        "coverage",
                        schema::object(&[("weeks", schema::coverage())], &["weeks"]),
                    ),
                    ("by_type", schema::table(&["type", "events", "counting"])),
                    (
                        "groups",
                        schema::table(&["driver", "type", "day", "week", "events", "counting"]),
                    ),
                    (
                        "list",
                        schema::table(&[
                            "date",
                            "time",
                            "driver",
                            "type",
                            "subtype",
                            "severity",
                            "resolution",
                        ]),
                    ),
                ],
                &["events", "counting", "coverage", "by_type"],
            )
        },
        answer: |db, state, caller, _, query| scorecard::safety(db, state, caller, query),
    },
    Endpoint {
        id: "returns",
        tool: "returns",
        area: Some(RETURNS),
        path: "/api/v1/returns",
        summary: "Contact compliance and returns to station (RTS)",
        description: "Use for contact compliance: which drivers didn't do it, that is returned \
            packages without the required call or text (contact missed, group_by driver). \
            Also Amazon's returns to station from the weekly scorecard, their reasons, and \
            which returns hurt the completion rate (DCR). Amazon posts a week's scorecard after \
            it ends: for packages returned last night or this week, use packages.",
        path_params: &[],
        params: &[
            DSP,
            PERIOD,
            DATE,
            FROM,
            TO,
            DRIVER,
            Param {
                name: "contact",
                kind: Kind::Choice(&["missed", "compliant"]),
                description: "missed: the driver did not call or text as required; compliant: \
                    the return was excused because they did.",
            },
            Param {
                name: "reason",
                kind: Kind::Text,
                description: "Amazon's RTS reason, as business_closed or object_missing.",
            },
            Param {
                name: "impacting",
                kind: Kind::Boolean,
                description: "Only returns that hurt the completion rate (DCR).",
            },
            Param {
                name: "group_by",
                kind: Kind::Text,
                description: "Count per driver, reason, coaching, week or day; two may be joined.",
            },
            Param {
                name: "list",
                kind: Kind::Boolean,
                description: "Also list the returns, a page at a time.",
            },
            LIMIT,
            CURSOR,
        ],
        order: 160,
        output: || {
            schema::answer(
                &[
                    ("returns", schema::count()),
                    ("hurting_dcr", schema::count()),
                    ("contact_missed", schema::count()),
                    (
                        "coverage",
                        schema::object(&[("weeks", schema::coverage())], &["weeks"]),
                    ),
                    (
                        "groups",
                        schema::table(&["driver", "reason", "coaching", "week", "day", "returns"]),
                    ),
                    (
                        "list",
                        schema::table(&[
                            "date",
                            "tracking",
                            "driver",
                            "reason",
                            "coaching",
                            "hurts_dcr",
                            "exemption",
                        ]),
                    ),
                ],
                &["returns", "hurting_dcr", "contact_missed", "coverage"],
            )
        },
        answer: |db, state, caller, _, query| scorecard::returns(db, state, caller, query),
    },
    Endpoint {
        id: "scorecard",
        tool: "scorecard",
        area: Some(SCORECARD),
        path: "/api/v1/scorecard",
        summary: "A week's scorecard",
        description: "Use for Amazon's weekly scorecard: the DSP's tier and focus areas, and each \
            driver's overall tier, score and tiers for CDF, DSB, POD, RTS and safety, lowest \
            scores first. Which drivers missed contact compliance is in returns; feedback, \
            safety events and returns themselves have their own tools.",
        path_params: &[],
        params: &[
            DSP,
            Param {
                name: "week",
                kind: Kind::Text,
                description: "An Amazon week as 2026-W39, last week, or latest (the default).",
            },
            DRIVER,
            Param {
                name: "below",
                kind: Kind::Choice(&["platinum", "gold", "silver", "bronze"]),
                description: "Only drivers whose overall tier is below this one.",
            },
            LIMIT,
            CURSOR,
        ],
        order: 170,
        output: || {
            schema::answer(
                &[
                    ("posted", schema::boolean()),
                    ("weeks_collected", schema::array(schema::text())),
                    (
                        "dsp",
                        schema::object(
                            &[
                                ("tier", schema::text()),
                                ("score", schema::nullable(schema::number())),
                                ("focus_areas", schema::array(schema::text())),
                                ("dcr", schema::text()),
                                ("contact_compliance", schema::text()),
                                ("dsb", schema::text()),
                                ("pod", schema::text()),
                                ("rts", schema::text()),
                                ("contact_compliance_by_driver", schema::text()),
                            ],
                            &[],
                        ),
                    ),
                    ("drivers_by_tier", schema::map(schema::count())),
                    (
                        "drivers",
                        schema::table(&[
                            "driver",
                            "score",
                            "overall",
                            "cdf",
                            "dsb",
                            "pod",
                            "rts",
                            "speeding",
                            "seatbelt",
                            "distractions",
                            "following_distance",
                            "signals",
                            "delivered",
                        ]),
                    ),
                ],
                &["posted"],
            )
        },
        answer: |db, state, caller, _, query| scorecard::weekly(db, state, caller, query),
    },
];

pub const EXAMPLES: &[Example] = &[
    Example {
        question: "Which addresses have given us repeated negative feedback?",
        call: r#"customer_feedback(group_by: "address", min_count: 2)"#,
        rest: "/api/v1/feedback?group_by=address&min_count=2",
        order: 50,
    },
    Example {
        question: "Which drivers didn't do contact compliance last week?",
        call: r#"returns(contact: "missed", period: "last week", group_by: "driver")"#,
        rest: "/api/v1/returns?contact=missed&period=last%20week&group_by=driver",
        order: 60,
    },
    Example {
        question: "Did Daniel get any Netradyne infractions last week?",
        call: r#"safety_events(driver: "Daniel", period: "last week")"#,
        rest: "/api/v1/safety?driver=Daniel&period=last%20week",
        order: 70,
    },
    Example {
        question: "Who scored lowest on last week's scorecard?",
        call: r#"scorecard(week: "last week")"#,
        rest: "/api/v1/scorecard?week=last%20week",
        order: 80,
    },
];
