//! Everything an agent can ask, written once: each endpoint with its parameters, and each
//! metric with what it means. Requests are checked against it, the OpenAPI document is
//! built from it, and later the MCP tools are too, so none of them can drift apart.
use super::Refusal;
use serde_json::{Map, Value, json};

/// What a parameter holds.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Text,
    Integer(i64, i64),
    Boolean,
    Choice(&'static [&'static str]),
}
#[derive(Clone, Copy, Debug)]
pub struct Param {
    pub name: &'static str,
    pub kind: Kind,
    pub description: &'static str,
}
/// A source an agent reads, switched per DSP on the platform's DSPs page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Timecards,
    MealBreaks,
    Routes,
    Dvic,
    Scorecard,
}
impl Source {
    pub const ALL: [Source; 5] = [
        Source::Timecards,
        Source::MealBreaks,
        Source::Routes,
        Source::Dvic,
        Source::Scorecard,
    ];
    /// The switch's name on the platform's DSPs page.
    pub fn switch(self) -> &'static str {
        match self {
            Source::Timecards => "Timecard",
            Source::MealBreaks => "Timecard · Meal Breaks",
            Source::Routes => "Routes",
            Source::Dvic => "DVIC",
            Source::Scorecard => "Scorecard",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Endpoint {
    pub id: &'static str,
    /// The MCP tool's name: snake_case, as every model and harness accepts.
    pub tool: &'static str,
    /// In the Essential toolset, the few tools small models choose between best.
    pub essential: bool,
    /// The one source it reads, refused for a DSP that has it switched off before the
    /// endpoint is asked. `None` for those that read none, or several and say which are off.
    pub source: Option<Source>,
    pub path: &'static str,
    pub summary: &'static str,
    pub description: &'static str,
    /// The parts of the path an agent fills in, such as `{driver}`.
    pub path_params: &'static [Param],
    pub params: &'static [Param],
}

const DSP: Param = Param {
    name: "dsp",
    kind: Kind::Text,
    description: "The DSP, by name. Leave it out when the key reaches one DSP.",
};
const PERIOD: Param = Param {
    name: "period",
    kind: Kind::Text,
    description: "The days as the user said them, read in the DSP's own time: yesterday, \
        last night, last week, this month, last 14 days, 2026-09-28, 2026-09-01..2026-09-30 \
        or 2026-W39. Weeks run Sunday to Saturday. Leave out for the last 30 days.",
};
const DATE: Param = Param {
    name: "date",
    kind: Kind::Text,
    description: "One day: today, yesterday, last night or 2026-09-28.",
};
const DAY: Param = Param {
    name: "date",
    kind: Kind::Text,
    description: "The day: yesterday, last night, today or 2026-09-28. Leave out for yesterday.",
};
const FROM: Param = Param {
    name: "from",
    kind: Kind::Text,
    description: "The first day, as 2026-09-01, with `to`; instead of `period`.",
};
const TO: Param = Param {
    name: "to",
    kind: Kind::Text,
    description: "The last day, as 2026-09-30, with `from`.",
};
const DRIVER: Param = Param {
    name: "driver",
    kind: Kind::Text,
    description: "A driver as the user named them: a name or part of one, a Driver Match \
        code, a Paycom employee code or an Amazon transporter ID.",
};
const LIMIT: Param = Param {
    name: "limit",
    kind: Kind::Integer(1, 500),
    description: "The most rows to return, 1 to 500.",
};
const CURSOR: Param = Param {
    name: "cursor",
    kind: Kind::Text,
    description: "The next_cursor an earlier answer gave, for its next page.",
};
const GROUPS_CURSOR: Param = Param {
    name: "groups_cursor",
    kind: Kind::Text,
    description: "The groups table's next_cursor; cursor separately pages the package list.",
};
const DETAIL: Param = Param {
    name: "detail",
    kind: Kind::Choice(&["summary", "full"]),
    description: "summary (the default) or full, only when the user wants every row.",
};

const DRIVER_PATH: Param = Param {
    name: "driver",
    kind: Kind::Text,
    description: DRIVER.description,
};
const ROUTE_PATH: Param = Param {
    name: "route",
    kind: Kind::Text,
    description: "The route code, as CX101, or the itinerary ID route_day gives.",
};

pub const ENDPOINTS: &[Endpoint] = &[
    Endpoint {
        id: "whoami",
        tool: "whoami",
        essential: true,
        source: None,
        path: "/api/v1/whoami",
        summary: "Who this key belongs to",
        description: "Use when you need the DSPs this key reaches, each DSP's date today or \
            what its key may do. Questions about days need no call to this first: every \
            answer says which days it read.",
        path_params: &[],
        params: &[],
    },
    Endpoint {
        id: "status",
        tool: "data_status",
        essential: false,
        source: None,
        path: "/api/v1/status",
        summary: "How fresh each source is",
        description: "Use when asked how current the data is: which sources the DSP has on \
            (Paycom timecards, Cortex meal breaks, routes, DVIC) and when each last collected.",
        path_params: &[],
        params: &[DSP],
    },
    Endpoint {
        id: "metrics",
        tool: "list_metrics",
        essential: false,
        source: None,
        path: "/api/v1/metrics",
        summary: "Every metric and term",
        description: "Use when unsure what a team_table metric or an Amazon term means: each \
            metric's source, unit and meaning, and a glossary.",
        path_params: &[],
        params: &[],
    },
    Endpoint {
        id: "drivers",
        tool: "find_drivers",
        essential: true,
        source: None,
        path: "/api/v1/drivers",
        summary: "Find drivers",
        description: "Use to look a driver up or list the DSP's people: name, Driver Match \
            code and the Paycom and Amazon IDs each holds. Other tools already accept a \
            driver's name, so look up only when asked to.",
        path_params: &[],
        params: &[
            DSP,
            Param {
                name: "q",
                kind: Kind::Text,
                description: "Part of a name, a code or an ID.",
            },
            LIMIT,
            CURSOR,
        ],
    },
    Endpoint {
        id: "packages",
        tool: "packages",
        essential: true,
        source: Some(Source::Routes),
        path: "/api/v1/packages",
        summary: "Count packages by what happened",
        description: "Use for questions about packages: how many a driver delivered, who \
            returned packages and why, how many were business closed. Answers a count, \
            optionally grouped, from Amazon's record of every drop-off. Add list only when \
            the user wants the packages themselves.",
        path_params: &[],
        params: &[
            DSP,
            PERIOD,
            DATE,
            FROM,
            TO,
            DRIVER,
            Param {
                name: "outcome",
                kind: Kind::Choice(super::facts::OUTCOMES),
                description: "delivered, returned (brought back to the station), attempted, \
                    not_picked_up (missing at the station), cancelled or open (still out \
                    when collected).",
            },
            Param {
                name: "reason",
                kind: Kind::Text,
                description: "Amazon's reason, as business_closed, object_missing, damaged, \
                    inaccessible_delivery_location, address_not_found or locker_issue; for \
                    delivered packages, where they were left, as doorstep.",
            },
            Param {
                name: "route",
                kind: Kind::Text,
                description: "A route code, as CX101.",
            },
            Param {
                name: "group_by",
                kind: Kind::Text,
                description: "Count per driver, day, outcome, reason, route or address; two \
                    may be joined, as driver,reason.",
            },
            Param {
                name: "list",
                kind: Kind::Boolean,
                description: "Also list the packages, a page at a time.",
            },
            LIMIT,
            CURSOR,
            GROUPS_CURSOR,
        ],
    },
    Endpoint {
        id: "driver",
        tool: "driver_report",
        essential: true,
        source: None,
        path: "/api/v1/drivers/{driver}",
        summary: "One driver's days",
        description: "Use for how one driver did over some days, and their averages: one \
            line per day with their route, stops, packages delivered and undeliverable, hours, \
            clock in and out, meal break and inspections, plus totals.",
        path_params: &[DRIVER_PATH],
        params: &[DSP, PERIOD, DATE, FROM, TO, DETAIL, LIMIT, CURSOR],
    },
    Endpoint {
        id: "team",
        tool: "team_table",
        essential: true,
        source: None,
        path: "/api/v1/team",
        summary: "Compare drivers",
        description: "Use to rank or compare drivers on chosen metrics: one row per driver, \
            or per driver per day, with the team's total for each metric.",
        path_params: &[],
        params: &[
            DSP,
            PERIOD,
            DATE,
            FROM,
            TO,
            Param {
                name: "metrics",
                kind: Kind::Text,
                description: "Comma-separated metric names; stops, packages and hours when \
                    left out.",
            },
            Param {
                name: "per",
                kind: Kind::Choice(&["driver", "day"]),
                description: "driver (totals, the default) or day.",
            },
            Param {
                name: "sort",
                kind: Kind::Text,
                description: "One of the metrics asked for, or name.",
            },
            Param {
                name: "order",
                kind: Kind::Choice(&["highest", "lowest"]),
                description: "highest first (the default) or lowest first, as for who did the \
                    fewest.",
            },
            LIMIT,
            CURSOR,
        ],
    },
    Endpoint {
        id: "routes",
        tool: "route_day",
        essential: true,
        source: Some(Source::Routes),
        path: "/api/v1/routes",
        summary: "A day's routes",
        description: "Use for one day's routes: each route's driver, packages delivered and \
            undeliverable, stops, departure and end, and whether the day is final.",
        path_params: &[],
        params: &[DSP, DAY, LIMIT, CURSOR],
    },
    Endpoint {
        id: "route",
        tool: "route_stops",
        essential: false,
        source: Some(Source::Routes),
        path: "/api/v1/routes/{route}",
        summary: "One route's packages",
        description: "Use for what happened on one route: outcomes, reasons and the packages \
            that were not delivered. detail full lists every package, only when the user \
            asks for all of them. Addresses appear only for keys allowed them.",
        path_params: &[ROUTE_PATH],
        params: &[DSP, DAY, DETAIL, LIMIT, CURSOR],
    },
    Endpoint {
        id: "package",
        tool: "find_package",
        essential: false,
        source: Some(Source::Routes),
        path: "/api/v1/packages/{tracking}",
        summary: "Find a package",
        description: "Use for one tracking ID: who carried it, on which route and day, and \
            what happened to it.",
        path_params: &[Param {
            name: "tracking",
            kind: Kind::Text,
            description: "The tracking ID, as TBA123456789000.",
        }],
        params: &[DSP],
    },
    Endpoint {
        id: "timecards",
        tool: "timecards",
        essential: false,
        source: Some(Source::Timecards),
        path: "/api/v1/timecards",
        summary: "Timecards",
        description: "Use for Paycom hours and punches: everyone's for one day, or one \
            driver's over a period with driver. Hours, clock in and out, lunch minutes.",
        path_params: &[],
        params: &[DSP, DATE, DRIVER, PERIOD, FROM, TO, LIMIT, CURSOR],
    },
    Endpoint {
        id: "meal_breaks",
        tool: "meal_breaks",
        essential: false,
        source: Some(Source::MealBreaks),
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
    },
    Endpoint {
        id: "dvic",
        tool: "dvic_inspections",
        essential: true,
        source: Some(Source::Dvic),
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
    },
    Endpoint {
        id: "feedback",
        tool: "customer_feedback",
        essential: false,
        source: Some(Source::Scorecard),
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
                kind: Kind::Choice(super::scorecard::FEEDBACK_NAMES),
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
    },
    Endpoint {
        id: "safety",
        tool: "safety_events",
        essential: false,
        source: Some(Source::Scorecard),
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
    },
    Endpoint {
        id: "returns",
        tool: "returns",
        essential: false,
        source: Some(Source::Scorecard),
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
    },
    Endpoint {
        id: "scorecard",
        tool: "scorecard",
        essential: false,
        source: Some(Source::Scorecard),
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
    },
];

pub fn endpoint(id: &str) -> &'static Endpoint {
    ENDPOINTS
        .iter()
        .find(|e| e.id == id)
        .expect("a listed endpoint")
}

/// Refuses a parameter the endpoint does not take, or a value of the wrong kind.
pub fn check(id: &str, query: &Value) -> Result<(), Refusal> {
    let endpoint = endpoint(id);
    let names = || endpoint.params.iter().map(|p| p.name.to_owned()).collect();
    for (name, value) in query.as_object().into_iter().flatten() {
        let Some(param) = endpoint.params.iter().find(|p| p.name == name) else {
            return Err(Refusal::new(
                400,
                "unknown_parameter",
                format!("{} takes no parameter `{name}`.", endpoint.path),
            )
            .choices(names()));
        };
        let text = value.as_str().unwrap_or("").trim();
        let fits = match param.kind {
            Kind::Text => text.len() <= 200,
            Kind::Integer(low, high) => {
                text.parse::<i64>().is_ok_and(|n| (low..=high).contains(&n))
            }
            Kind::Boolean => ["true", "false", "1", "0", "yes", "no"].contains(&text),
            Kind::Choice(choices) => choices.contains(&text),
        };
        if !fits {
            let expected = match param.kind {
                Kind::Text => "at most 200 characters".to_owned(),
                Kind::Integer(low, high) => format!("a whole number from {low} to {high}"),
                Kind::Boolean => "true or false".to_owned(),
                Kind::Choice(choices) => format!("one of {}", choices.join(", ")),
            };
            return Err(Refusal::new(
                400,
                "invalid_parameter",
                format!("`{name}` is {expected}."),
            ));
        }
    }
    Ok(())
}
/// A boolean parameter: true when given as true, 1 or yes.
pub fn flag(query: &Value, name: &str) -> bool {
    ["true", "1", "yes"].contains(&super::scope::param(query, name))
}

/// What a metric counts, where it comes from, and how a period adds it up.
#[derive(Clone, Copy, Debug)]
pub struct Metric {
    pub name: &'static str,
    pub source: &'static str,
    pub unit: &'static str,
    /// `sum` and `count` add up over a period; `day` metrics exist only per day.
    pub total: &'static str,
    pub description: &'static str,
}
pub const METRICS: &[Metric] = &[
    Metric {
        name: "routes",
        source: "routes",
        unit: "itineraries",
        total: "count",
        description: "Itineraries the driver was assigned.",
    },
    Metric {
        name: "stops_completed",
        source: "routes",
        unit: "stops",
        total: "sum",
        description: "Delivery stops completed, as Amazon's itinerary summary counts them; the station pickup is not a stop.",
    },
    Metric {
        name: "stops_total",
        source: "routes",
        unit: "stops",
        total: "sum",
        description: "Delivery stops on the itinerary.",
    },
    Metric {
        name: "packages_delivered",
        source: "routes",
        unit: "packages",
        total: "sum",
        description: "Packages delivered, as Amazon's itinerary summary counts them.",
    },
    Metric {
        name: "packages_total",
        source: "routes",
        unit: "packages",
        total: "sum",
        description: "Packages on the itinerary.",
    },
    Metric {
        name: "packages_remaining",
        source: "routes",
        unit: "packages",
        total: "sum",
        description: "Packages not yet delivered or returned when the day was collected.",
    },
    Metric {
        name: "packages_undeliverable",
        source: "routes",
        unit: "packages",
        total: "sum",
        description: "Packages Amazon marked undeliverable.",
    },
    Metric {
        name: "break_minutes",
        source: "routes",
        unit: "minutes",
        total: "sum",
        description: "Break time Amazon recorded on the itinerary.",
    },
    Metric {
        name: "overtime_minutes",
        source: "routes",
        unit: "minutes",
        total: "sum",
        description: "Overtime Amazon recorded on the itinerary.",
    },
    Metric {
        name: "hours_worked",
        source: "timecards",
        unit: "hours",
        total: "sum",
        description: "Hours on the Paycom timecard.",
    },
    Metric {
        name: "days_worked",
        source: "timecards",
        unit: "days",
        total: "count",
        description: "Days with hours on the Paycom timecard.",
    },
    Metric {
        name: "lunch_minutes",
        source: "timecards",
        unit: "minutes",
        total: "sum",
        description: "Minutes between Paycom's lunch out and lunch in punches.",
    },
    Metric {
        name: "clock_in",
        source: "timecards",
        unit: "time",
        total: "day",
        description: "First clock in, in the DSP's time.",
    },
    Metric {
        name: "clock_out",
        source: "timecards",
        unit: "time",
        total: "day",
        description: "Last clock out, in the DSP's time.",
    },
    Metric {
        name: "meal_issues",
        source: "meal_breaks",
        unit: "days",
        total: "count",
        description: "Days the meal-break comparison found something to look at, among the \
            days the driver had a Cortex route.",
    },
    Metric {
        name: "meal_status",
        source: "meal_breaks",
        unit: "verdict",
        total: "day",
        description: "The meal-break comparison's verdict; see the glossary.",
    },
    Metric {
        name: "inspections",
        source: "dvic",
        unit: "inspections",
        total: "sum",
        description: "DVIC inspections done.",
    },
    Metric {
        name: "short_inspections",
        source: "dvic",
        unit: "inspections",
        total: "sum",
        description: "DVIC inspections shorter than their minimum.",
    },
];
pub fn metric(name: &str) -> Option<&'static Metric> {
    METRICS.iter().find(|m| m.name == name)
}

pub const GLOSSARY: &[(&str, &str)] = &[
    (
        "Driver Match code",
        "A six-character code for one person, the same across every source. Use it to name a driver exactly.",
    ),
    (
        "transporter ID",
        "Amazon's ID for a driver, in routes, meal breaks, DVIC and the scorecard.",
    ),
    ("employee code", "Paycom's ID for an employee."),
    (
        "Amazon week",
        "Sunday to Saturday, named by the ISO week of its Saturday, as 2026-W39.",
    ),
    (
        "departed",
        "When the driver left the station to start the route, in the DSP's time.",
    ),
    (
        "ended",
        "When the route's session ended, after the last stop.",
    ),
    (
        "snapshot",
        "A route day collected while it was still in progress; numbers can still change.",
    ),
    (
        "DVIC",
        "Daily Vehicle Inspection Checklist, done before driving. Inspections under their \
         minimum (90 or 300 seconds by vehicle) count as short.",
    ),
    (
        "meal status: same",
        "Cortex's meal break and Paycom's lunch punches agree.",
    ),
    (
        "meal status: different",
        "They disagree by more than the allowed difference.",
    ),
    (
        "meal status: missing_lunch",
        "Cortex has a meal break; Paycom has no lunch punches.",
    ),
    (
        "meal status: no_flex_meal",
        "Paycom has lunch punches; Cortex has no meal break. With cortexRoute false, Cortex \
         had no route for the person, so no meal break was expected there.",
    ),
    (
        "meal status: flex_only",
        "Only Cortex has the driver that day.",
    ),
    (
        "meal status: review_punches",
        "Paycom's punches don't read as a whole day.",
    ),
    (
        "meal status: review_pairing",
        "Meal breaks and lunches don't pair up one to one.",
    ),
    (
        "meal status: missing_data",
        "A source has nothing for the driver yet.",
    ),
];

/// The metrics and glossary, as `/api/v1/metrics` answers.
pub fn metrics() -> Value {
    json!({
        "metrics": METRICS.iter().map(|m| json!({
            "name": m.name, "source": m.source, "unit": m.unit,
            "total": m.total, "description": m.description
        })).collect::<Vec<_>>(),
        "glossary": GLOSSARY.iter().map(|(term, meaning)| json!({"term": term, "meaning": meaning})).collect::<Vec<_>>(),
    })
}

fn schema(kind: Kind) -> Value {
    match kind {
        Kind::Text => json!({"type":"string"}),
        Kind::Integer(low, high) => json!({"type":"integer","minimum":low,"maximum":high}),
        Kind::Boolean => json!({"type":"boolean"}),
        Kind::Choice(choices) => json!({"type":"string","enum":choices}),
    }
}

/// The endpoint an MCP tool asks.
pub fn tool(name: &str) -> Option<&'static Endpoint> {
    ENDPOINTS.iter().find(|e| e.tool == name)
}

/// An MCP tool's input: one flat object of strings, numbers, yes-or-no and fixed choices,
/// which every model's function calling accepts. The parts of the path are required.
pub fn input_schema(endpoint: &Endpoint) -> Map<String, Value> {
    let mut properties = Map::new();
    for param in endpoint.path_params.iter().chain(endpoint.params) {
        let mut property = schema(param.kind);
        let description = if param.name == "metrics" {
            let names: Vec<&str> = METRICS.iter().map(|m| m.name).collect();
            format!(
                "{} One or more of: {}.",
                param.description,
                names.join(", ")
            )
        } else {
            param.description.to_owned()
        };
        property["description"] = json!(description);
        properties.insert(param.name.to_owned(), property);
    }
    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), Value::Object(properties));
    if !endpoint.path_params.is_empty() {
        let required: Vec<&str> = endpoint.path_params.iter().map(|p| p.name).collect();
        schema.insert("required".into(), json!(required));
    }
    schema
}

/// The OpenAPI 3.1 document for the agent API, for tools that read one.
pub fn openapi(origin: &str) -> Value {
    let mut paths = Map::new();
    for endpoint in ENDPOINTS {
        let parameters: Vec<Value> = endpoint
            .path_params
            .iter()
            .map(|p| json!({"name":p.name,"in":"path","required":true,"description":p.description,"schema":schema(p.kind)}))
            .chain(endpoint.params.iter().map(|p| {
                json!({"name":p.name,"in":"query","required":false,"description":p.description,"schema":schema(p.kind)})
            }))
            .collect();
        paths.insert(
            endpoint.path.to_owned(),
            json!({"get": {
                "operationId": endpoint.id,
                "summary": endpoint.summary,
                "description": endpoint.description,
                "parameters": parameters,
                "responses": {
                    "200": {"description": "The answer.", "content": {"application/json": {"schema": {"type":"object"}}}},
                    "400": {"description": "Something unclear; `message` says what and `choices` what it could mean."},
                    "401": {"description": "No key, or a key that is revoked, expired or not for this Dispatch."},
                    "404": {"description": "Nothing by that name."},
                    "429": {"description": "Too many calls this minute; wait for `Retry-After` seconds."}
                }
            }}),
        );
    }
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Dispatch agent API",
            "version": "1",
            "description": "Read a DSP's drivers, routes, timecards, meal breaks and DVIC. \
                Every answer names the DSP, the days and the driver it understood, and which \
                days each source has. Times are the DSP's own."
        },
        "servers": [{"url": origin}],
        "security": [{"key": []}],
        "components": {"securitySchemes": {"key": {"type":"http","scheme":"bearer","description":"An agent key made on the Agents page."}}},
        "paths": paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_endpoint_and_metric_is_listed_once() {
        let mut ids: Vec<&str> = ENDPOINTS.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ENDPOINTS.len());
        let mut names: Vec<&str> = METRICS.iter().map(|m| m.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), METRICS.len());
        let mut tools: Vec<&str> = ENDPOINTS.iter().map(|e| e.tool).collect();
        tools.sort_unstable();
        tools.dedup();
        assert_eq!(tools.len(), ENDPOINTS.len());
        for endpoint in ENDPOINTS {
            assert!(endpoint.path.starts_with("/api/v1/"), "{}", endpoint.path);
            // Names every model accepts: lower snake_case, well under 64 characters.
            assert!(
                endpoint.tool.len() <= 32
                    && endpoint
                        .tool
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c == '_'),
                "{}",
                endpoint.tool
            );
            for param in endpoint.path_params {
                assert!(endpoint.path.contains(&format!("{{{}}}", param.name)));
            }
        }
    }
    #[test]
    fn requests_take_only_their_own_parameters() {
        assert!(
            check(
                "team",
                &json!({"period":"last week","per":"day","limit":"10"})
            )
            .is_ok()
        );
        let unknown = check("team", &json!({"week":"39"})).unwrap_err();
        assert_eq!(unknown.code, "unknown_parameter");
        assert!(unknown.choices.contains(&"period".to_owned()));
        assert_eq!(
            check("team", &json!({"per":"month"})).unwrap_err().code,
            "invalid_parameter"
        );
        assert_eq!(
            check("team", &json!({"limit":"0"})).unwrap_err().code,
            "invalid_parameter"
        );
        assert_eq!(
            check("dvic", &json!({"short":"maybe"})).unwrap_err().code,
            "invalid_parameter"
        );
    }
    #[test]
    fn the_openapi_document_lists_every_endpoint() {
        let document = openapi("https://dispatch.example.com");
        assert_eq!(
            document["paths"].as_object().unwrap().len(),
            ENDPOINTS.len()
        );
        let team = &document["paths"]["/api/v1/team"]["get"];
        assert_eq!(team["operationId"], "team");
        assert!(
            team["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["name"] == "metrics")
        );
    }
}
