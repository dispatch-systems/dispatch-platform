//! Routes' part of the agent catalog: its endpoints, its metrics for team_table and the
//! words its answers use.
use super::ROUTES;
use crate::agents::data::{
    self,
    catalog::{
        CURSOR, DATE, DAY, DETAIL, DRIVER, DSP, Endpoint, FROM, Kind, LIMIT, Metric, PERIOD, Param,
        TO, Term,
    },
    facts,
};

const GROUPS_CURSOR: Param = Param {
    name: "groups_cursor",
    kind: Kind::Text,
    description: "The groups table's next_cursor; cursor separately pages the package list.",
};
const ROUTE_PATH: Param = Param {
    name: "route",
    kind: Kind::Text,
    description: "The route code, as CX101, or the itinerary ID route_day gives.",
};

pub const ENDPOINTS: &[Endpoint] = &[
    Endpoint {
        id: "packages",
        tool: "packages",
        area: Some(ROUTES),
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
                kind: Kind::Choice(facts::OUTCOMES),
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
        order: 50,
        answer: |db, state, caller, _, query| data::packages(db, state, caller, query),
    },
    Endpoint {
        id: "routes",
        tool: "route_day",
        area: Some(ROUTES),
        path: "/api/v1/routes",
        summary: "A day's routes",
        description: "Use for one day's routes: each route's driver, packages delivered and \
            undeliverable, stops, departure and end, and whether the day is final.",
        path_params: &[],
        params: &[DSP, DAY, LIMIT, CURSOR],
        order: 80,
        answer: |db, state, caller, _, query| data::routes(db, state, caller, query),
    },
    Endpoint {
        id: "route",
        tool: "route_stops",
        area: Some(ROUTES),
        path: "/api/v1/routes/{route}",
        summary: "One route's packages",
        description: "Use for what happened on one route: outcomes, reasons and the packages \
            that were not delivered. detail full lists every package, only when the user \
            asks for all of them. Addresses appear only for keys allowed them.",
        path_params: &[ROUTE_PATH],
        params: &[DSP, DAY, DETAIL, LIMIT, CURSOR],
        order: 90,
        answer: |db, state, caller, named, query| data::route(db, state, caller, named, query),
    },
    Endpoint {
        id: "package",
        tool: "find_package",
        area: Some(ROUTES),
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
        order: 100,
        answer: |db, state, caller, named, query| data::package(db, state, caller, named, query),
    },
];

pub const METRICS: &[Metric] = &[
    Metric {
        name: "routes",
        area: ROUTES,
        unit: "itineraries",
        total: "count",
        description: "Itineraries the driver was assigned.",
    },
    Metric {
        name: "stops_completed",
        area: ROUTES,
        unit: "stops",
        total: "sum",
        description: "Delivery stops completed, as Amazon's itinerary summary counts them; the station pickup is not a stop.",
    },
    Metric {
        name: "stops_total",
        area: ROUTES,
        unit: "stops",
        total: "sum",
        description: "Delivery stops on the itinerary.",
    },
    Metric {
        name: "packages_delivered",
        area: ROUTES,
        unit: "packages",
        total: "sum",
        description: "Packages delivered, as Amazon's itinerary summary counts them.",
    },
    Metric {
        name: "packages_total",
        area: ROUTES,
        unit: "packages",
        total: "sum",
        description: "Packages on the itinerary.",
    },
    Metric {
        name: "packages_remaining",
        area: ROUTES,
        unit: "packages",
        total: "sum",
        description: "Packages not yet delivered or returned when the day was collected.",
    },
    Metric {
        name: "packages_undeliverable",
        area: ROUTES,
        unit: "packages",
        total: "sum",
        description: "Packages Amazon marked undeliverable.",
    },
    Metric {
        name: "break_minutes",
        area: ROUTES,
        unit: "minutes",
        total: "sum",
        description: "Break time Amazon recorded on the itinerary.",
    },
    Metric {
        name: "overtime_minutes",
        area: ROUTES,
        unit: "minutes",
        total: "sum",
        description: "Overtime Amazon recorded on the itinerary.",
    },
];

pub const TERMS: &[Term] = &[
    Term {
        term: "departed",
        meaning: "When the driver left the station to start the route, in the DSP's time.",
        order: 50,
    },
    Term {
        term: "ended",
        meaning: "When the route's session ended, after the last stop.",
        order: 60,
    },
    Term {
        term: "snapshot",
        meaning: "A route day collected while it was still in progress; numbers can still change.",
        order: 70,
    },
];
