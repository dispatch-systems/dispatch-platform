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
#[derive(Clone, Copy, Debug)]
pub struct Endpoint {
    pub id: &'static str,
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
    description: "The DSP, by id or name. Optional when the key reaches only one DSP.",
};
const PERIOD: Param = Param {
    name: "period",
    kind: Kind::Text,
    description: "The days to cover: today, yesterday, this week, last week, this month, \
        last month, last N days, a date (2026-09-28), two dates (2026-09-01..2026-09-30) or an \
        Amazon week (2026-W39). Weeks run Sunday to Saturday; days are the DSP's own.",
};
const DATE: Param = Param {
    name: "date",
    kind: Kind::Text,
    description: "One day: today, yesterday or a date such as 2026-09-28, in the DSP's time.",
};
const FROM: Param = Param {
    name: "from",
    kind: Kind::Text,
    description: "The first day, as 2026-09-01. Use with `to` instead of `period`.",
};
const TO: Param = Param {
    name: "to",
    kind: Kind::Text,
    description: "The last day, as 2026-09-30. Use with `from`.",
};
const DRIVER: Param = Param {
    name: "driver",
    kind: Kind::Text,
    description: "A driver: their name, Driver Match code, Paycom employee code or Amazon \
        transporter ID. A name several drivers share is refused with the choices.",
};
const LIMIT: Param = Param {
    name: "limit",
    kind: Kind::Integer(1, 500),
    description: "The most rows to return.",
};

const DRIVER_PATH: Param = Param {
    name: "driver",
    kind: Kind::Text,
    description: DRIVER.description,
};

pub const ENDPOINTS: &[Endpoint] = &[
    Endpoint {
        id: "whoami",
        path: "/api/v1/whoami",
        summary: "Who this key belongs to",
        description: "The key's access, the time, and each DSP it reaches with that DSP's \
            own date and the features it has on. Call this first.",
        path_params: &[],
        params: &[],
    },
    Endpoint {
        id: "status",
        path: "/api/v1/status",
        summary: "How fresh each source is",
        description: "Which sources the DSP has switched on, and when each last collected: \
            Paycom timecards, Cortex meal breaks, routes and DVIC.",
        path_params: &[],
        params: &[DSP],
    },
    Endpoint {
        id: "metrics",
        path: "/api/v1/metrics",
        summary: "Every metric and term",
        description: "Each metric `team` can show, with its source, unit and meaning, and a \
            glossary of the Amazon terms answers use.",
        path_params: &[],
        params: &[],
    },
    Endpoint {
        id: "drivers",
        path: "/api/v1/drivers",
        summary: "Find drivers",
        description: "Everyone in the DSP with a Driver Match code: their name, how they are \
            matched, and every Paycom and Amazon ID they hold.",
        path_params: &[],
        params: &[
            DSP,
            Param {
                name: "q",
                kind: Kind::Text,
                description: "Part of a name, a code or an ID.",
            },
            LIMIT,
        ],
    },
    Endpoint {
        id: "driver",
        path: "/api/v1/drivers/{driver}",
        summary: "One driver's days",
        description: "Everything collected about one driver in a period: each day's routes \
            with packages and stops, timecard, meal breaks and inspections, with totals and \
            which days each source has. Defaults to the last 7 days.",
        path_params: &[DRIVER_PATH],
        params: &[DSP, PERIOD, DATE, FROM, TO],
    },
    Endpoint {
        id: "team",
        path: "/api/v1/team",
        summary: "Everyone's numbers",
        description: "A table of metrics for every driver: one row per driver with totals for \
            the period, or one row per driver per day. Defaults to yesterday's stops, \
            packages and hours.",
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
                description: "Comma-separated metric names; see /api/v1/metrics.",
            },
            Param {
                name: "per",
                kind: Kind::Choice(&["driver", "day"]),
                description: "One row per driver (totals), or per driver per day.",
            },
            Param {
                name: "sort",
                kind: Kind::Text,
                description: "One of the metrics asked for, to sort by highest first, or `name`.",
            },
            LIMIT,
        ],
    },
    Endpoint {
        id: "routes",
        path: "/api/v1/routes",
        summary: "A day's routes",
        description: "Every itinerary on one day: route, driver, packages, stops, times and \
            breaks. Says whether the day is final or a snapshot of a day in progress.",
        path_params: &[],
        params: &[DSP, DATE],
    },
    Endpoint {
        id: "route",
        path: "/api/v1/routes/{itinerary}",
        summary: "One itinerary's stops",
        description: "Each stop of one itinerary in order, with every package's outcome and \
            scan time. Addresses and GPS appear only for keys allowed them.",
        path_params: &[Param {
            name: "itinerary",
            kind: Kind::Text,
            description: "The itinerary ID, from /api/v1/routes.",
        }],
        params: &[DSP, DATE],
    },
    Endpoint {
        id: "package",
        path: "/api/v1/packages/{tracking}",
        summary: "Find a package",
        description: "Who carried a package, on which route and day, and what happened to \
            it. Addresses and GPS appear only for keys allowed them.",
        path_params: &[Param {
            name: "tracking",
            kind: Kind::Text,
            description: "The tracking ID, as TBA123456789000.",
        }],
        params: &[DSP],
    },
    Endpoint {
        id: "timecards",
        path: "/api/v1/timecards",
        summary: "Timecards",
        description: "Paycom timecards: everyone's for one day, or one driver's for a period \
            with `driver`. Hours, clock in and out, and lunch minutes.",
        path_params: &[],
        params: &[DSP, DATE, DRIVER, PERIOD, FROM, TO],
    },
    Endpoint {
        id: "meal_breaks",
        path: "/api/v1/meal-breaks",
        summary: "Meal breaks",
        description: "One day's meal breaks: what Cortex recorded beside Paycom's lunch \
            punches, with the comparison's verdict for each driver.",
        path_params: &[],
        params: &[
            DSP,
            DATE,
            Param {
                name: "issues",
                kind: Kind::Boolean,
                description: "Only drivers whose meal break needs a look.",
            },
        ],
    },
    Endpoint {
        id: "dvic",
        path: "/api/v1/dvic",
        summary: "Vehicle inspections",
        description: "DVIC inspections in a period, with how long each took against the \
            minimum. Defaults to the last 7 days.",
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
                description: "Only inspections shorter than their minimum.",
            },
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
        description: "Days the meal-break comparison found something to look at.",
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
        "Paycom has lunch punches; Cortex has no meal break.",
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
        for endpoint in ENDPOINTS {
            assert!(endpoint.path.starts_with("/api/v1/"), "{}", endpoint.path);
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
