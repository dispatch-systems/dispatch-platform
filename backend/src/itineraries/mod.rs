//! Daily routes from Cortex's execution pages: the station's route list, every driver's
//! itinerary with its stops and packages, and the newer routes page's list, one day at a
//! time, published into the DSP's itineraries database. Normalized rows hold what reads
//! filter on; every response is kept whole, compressed, for reprocessing. Browser and
//! HTTP data is untrusted input. The collection is `routes` to the platform; the module
//! and its database take Amazon's word, since `routes` names the HTTP routes here.
use crate::{
    Error, Result,
    collectors::{AddedStorage, Provider},
    contracts::{RouteDayView, RouteDays, RouteItinerary, RoutePublication},
    db::{Db, DspLease, Kind, Store, at, now, s},
    ensure,
    meals::{CollectionRequest, Discovery, Scope},
};
use chrono::{Duration, NaiveDate};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::io::Write;

pub const JOB_KIND: &str = "cortex.routes.collect";
pub const COLLECTION: &str = "routes";
pub const ADAPTER_VERSION: i64 = 1;
/// The itineraries database beside `cortex.sqlite`.
pub static STORAGE: AddedStorage = AddedStorage {
    id: "itineraries",
    kind: Kind::Itineraries,
    marker: "storage.itineraries",
    source: "itineraries-v1",
    verify,
};
/// How far back a schedule collects days it has no final publication of.
pub const BACKFILL_DAYS: usize = 7;
/// Jobs one scheduled run queues at most; the queue admits five active per DSP.
pub const MAX_JOBS_PER_RUN: usize = 4;
/// Days one request may queue at once.
pub const MAX_DAYS_PER_REQUEST: i64 = 5;
/// Itineraries a day may list, well above any station seen.
pub const MAX_ITINERARIES: usize = 1000;
/// Stops and tasks one itinerary may hold.
pub const MAX_STOPS: usize = 5000;
pub const MAX_TASKS: usize = 20_000;
/// As much as one response may be. The largest itinerary seen was 1.4 MB.
pub const MAX_BODY: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Collection {
    #[serde(rename = "routes")]
    Routes,
}
/// How a day is read: a completed day once, or today as it stands.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Final,
    Snapshot,
}
impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Final => "final",
            Self::Snapshot => "snapshot",
        }
    }
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "final" => Ok(Self::Final),
            "snapshot" => Ok(Self::Snapshot),
            _ => Err(Error::new("invalid_input", 400)),
        }
    }
}
/// One day of one station's routes, as a job asks for it. The station, names and
/// timezone let the driver find the service area and provider on Cortex, as the meal
/// collection does; a resolved scope is carried once known.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub collection: Collection,
    #[serde(default)]
    pub mode: Mode,
    pub date: String,
    pub station: String,
    pub timezone: String,
    pub dsp_name: String,
    pub dsp_abbreviation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_area_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}
impl Request {
    /// Whether a job request or a collection names the routes collection.
    pub fn is(value: &Value) -> bool {
        value.get("collection") == Some(&json!("routes"))
    }
    pub fn parse(value: &Value) -> Result<Option<Self>> {
        if !Self::is(value) {
            return Ok(None);
        }
        let request: Self =
            serde_json::from_value(value.clone()).map_err(|_| Error::new("invalid_input", 400))?;
        request.validate()?;
        Ok(Some(request))
    }
    pub fn validate(&self) -> Result<()> {
        self.discovery().scope("discovery", "discovery")?;
        if let (Some(area), Some(provider)) = (&self.service_area_id, &self.provider) {
            self.discovery().scope(area, provider)?;
        } else {
            ensure(
                self.service_area_id.is_none() && self.provider.is_none(),
                "invalid_input",
                400,
            )?;
        }
        Ok(())
    }
    fn discovery(&self) -> Discovery {
        Discovery {
            date: self.date.clone(),
            station: self.station.clone(),
            timezone: self.timezone.clone(),
            dsp_name: self.dsp_name.clone(),
            dsp_abbreviation: self.dsp_abbreviation.clone(),
        }
    }
    /// What the driver resolves the scope from: the scope itself when known.
    pub fn scope_request(&self) -> CollectionRequest {
        match (&self.service_area_id, &self.provider) {
            (Some(area), Some(provider)) => CollectionRequest::Scoped(Scope {
                date: self.date.clone(),
                station: self.station.clone(),
                service_area_id: area.clone(),
                provider: provider.clone(),
                timezone: self.timezone.clone(),
            }),
            _ => CollectionRequest::Discover(self.discovery()),
        }
    }
}

/// One driver's itinerary as the detail API answered, kept as the JSON text it came
/// as: a day holds tens of megabytes, which parsed trees would multiply.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItineraryCapture {
    pub id: String,
    pub transporter_id: String,
    pub detail: String,
}
impl ItineraryCapture {
    /// The detail parsed, once, when a reader needs it.
    pub fn parsed(&self) -> Result<Value> {
        ensure(
            self.detail.len() <= MAX_BODY,
            "routes_source_too_large",
            502,
        )?;
        let value: Value = serde_json::from_str(&self.detail)
            .map_err(|_| Error::new("routes_capture_invalid", 502))?;
        ensure(
            value["itineraryDetails"].is_object(),
            "routes_capture_invalid",
            502,
        )?;
        Ok(value)
    }
}
/// One itinerary's rows, shaped and compressed ahead of publication.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedItinerary {
    id: String,
    transporter_id: String,
    columns: Vec<(String, Value)>,
    stops: Vec<StopRow>,
    tasks: Vec<TaskRow>,
    day: DayRow,
    addresses: Vec<Value>,
    person: Value,
    raw_bytes: usize,
    /// The detail gzip-compressed, base64.
    raw: String,
}
/// A capture's rows and blobs, computed outside the platform lock so publication only
/// inserts. The lists are compressed here too.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    itineraries: Vec<PreparedItinerary>,
    summaries_bytes: usize,
    summaries: String,
    route_summaries_bytes: usize,
    route_summaries: String,
}
/// A day's routes as collected, before publication: the two lists and every itinerary,
/// each as Amazon sent it, and once prepared, the rows they become.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture {
    pub version: u32,
    pub collection: Collection,
    pub mode: Mode,
    pub scope: Scope,
    pub started_at: i64,
    pub finished_at: i64,
    pub summaries: Value,
    pub route_summaries: Value,
    pub itineraries: Vec<ItineraryCapture>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared: Option<Prepared>,
}
/// An identifier a row can carry: printable text of a bounded length. Amazon's task and
/// stop ids mix letters, digits and punctuation.
pub fn identifier(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && !value.chars().any(|c| c.is_control())
        && !value.trim().is_empty()
}
/// An identifier as the API spells them.
pub fn token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:#".contains(&b))
}
/// The list's summaries this scope covers: every driver's, or the provider's own.
pub fn listed(summaries: &Value, scope: &Scope) -> Vec<(String, String)> {
    summaries["itinerarySummaries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|summary| {
            scope.provider == "ALL_DRIVERS" || summary["companyId"] == json!(scope.provider)
        })
        .filter_map(|summary| {
            Some((
                summary["itineraryId"].as_str()?.to_owned(),
                summary["transporterId"].as_str()?.to_owned(),
            ))
        })
        .collect()
}
impl Capture {
    pub fn validate(&self, request: &Request) -> Result<()> {
        request.validate()?;
        ensure(self.version == 1, "routes_capture_invalid", 502)?;
        self.scope.validate()?;
        ensure(
            self.scope.date == request.date
                && self.scope.station == request.station
                && self.mode == request.mode,
            "routes_scope_mismatch",
            502,
        )?;
        ensure(
            self.started_at > 0 && self.finished_at >= self.started_at,
            "routes_capture_invalid",
            502,
        )?;
        let listed = listed(&self.summaries, &self.scope);
        ensure(
            listed.len() <= MAX_ITINERARIES && self.itineraries.len() == listed.len(),
            "routes_capture_invalid",
            502,
        )?;
        ensure(
            self.route_summaries["rmsRouteSummaries"].is_array(),
            "routes_capture_invalid",
            502,
        )?;
        for (captured, (id, transporter)) in self.itineraries.iter().zip(&listed) {
            ensure(
                token(&captured.id, 128)
                    && token(&captured.transporter_id, 64)
                    && captured.id == *id
                    && captured.transporter_id == *transporter,
                "routes_scope_mismatch",
                502,
            )?;
            if let Some(prepared) = &self.prepared {
                // Prepared from these itineraries: the details themselves were dropped.
                ensure(
                    prepared.itineraries.len() == listed.len()
                        && prepared.itineraries.iter().zip(&listed).all(
                            |(p, (id, transporter))| {
                                p.id == *id
                                    && p.transporter_id == *transporter
                                    && p.stops.len() <= MAX_STOPS
                                    && p.tasks.len() <= MAX_TASKS
                            },
                        ),
                    "routes_capture_invalid",
                    502,
                )?;
                continue;
            }
            let parsed = captured.parsed()?;
            let detail = &parsed["itineraryDetails"];
            ensure(
                detail["itineraryId"] == json!(id)
                    && detail["transporterId"] == json!(transporter)
                    && detail["stops"]
                        .as_array()
                        .is_some_and(|s| s.len() <= MAX_STOPS),
                "routes_scope_mismatch",
                502,
            )?;
            let tasks: usize = detail["stops"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|stop| stop["tasks"].as_array().map_or(0, Vec::len))
                .sum();
            ensure(tasks <= MAX_TASKS, "routes_source_too_large", 502)?;
        }
        Ok(())
    }
}

/// An epoch time as the API sends it: milliseconds, or seconds for older fields.
pub fn stamp(value: &Value) -> Option<i64> {
    let number = value.as_f64()?;
    if !number.is_finite() || number <= 0.0 {
        return None;
    }
    Some(if number < 100_000_000_000.0 {
        (number * 1000.0).round() as i64
    } else {
        number.round() as i64
    })
}
fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.chars().take(512).collect()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}
fn integer(value: &Value) -> Option<i64> {
    number(value).map(|n| n.round() as i64)
}
fn flag(value: &Value) -> Option<i64> {
    match value {
        Value::Bool(flag) => Some(i64::from(*flag)),
        _ => None,
    }
}
/// The names of an object's flags that are set, for a compact record of a stop's kinds.
fn set_flags(value: &Value) -> Value {
    let mut names: Vec<&str> = value
        .as_object()
        .into_iter()
        .flat_map(|o| o.iter())
        .filter(|(_, v)| **v == json!(true))
        .map(|(k, _)| k.as_str())
        .collect();
    names.sort_unstable();
    json!(names)
}
fn gzip(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}
/// A stored response, inflated.
pub fn gunzip(bytes: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut out)?;
    ensure(out.len() <= MAX_BODY, "routes_source_too_large", 502)?;
    Ok(out)
}

/// What fixture mode collects: a small, complete day with two drivers.
pub fn fixture(request: &Request) -> Result<Capture> {
    request.validate()?;
    let started_at = now();
    let scope = Scope {
        date: request.date.clone(),
        station: request.station.clone(),
        service_area_id: request
            .service_area_id
            .clone()
            .unwrap_or_else(|| "area-demo".into()),
        provider: request
            .provider
            .clone()
            .unwrap_or_else(|| "provider-demo".into()),
        timezone: request.timezone.clone(),
    };
    let midnight = NaiveDate::parse_from_str(&request.date, "%Y-%m-%d")
        .map_err(|_| Error::new("invalid_date", 400))?
        .and_hms_opt(8, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis();
    let hour = 3_600_000;
    let drivers = [
        ("driver-1", "Fixture", "Driver", "CX101"),
        ("driver-2", "Second", "Driver", "CX102"),
    ];
    let summaries: Vec<Value> = drivers
        .iter()
        .enumerate()
        .map(|(i, (transporter, _, _, route))| {
            json!({"itineraryId":format!("itinerary-{}", i + 1),"transporterId":transporter,
                "companyId":scope.provider,"routeCode":route,"routeCodes":[route],"vinNumber":"1FTFIXTURE00000",
                "executionStatus":"COMPLETE","progressStatus":"ON_TIME","serviceTypeName":"Standard Parcel",
                "plannedDepartureTime":midnight,"itineraryStartTime":midnight + 10 * 60_000,
                "waveStartTime":midnight - 20 * 60_000,"sessionEndTime":midnight + 8 * hour,
                "projectedCompletionTime":midnight + 8 * hour,"lastStopExecutionTime":midnight + 7 * hour,
                "blockDurationInMinutes":600,"totalLocations":2,"completedLocations":2,"totalPackages":3,
                "packageSummary":{"DELIVERED":3 - i as i64,"REMAINING":0,"UNDELIVERABLE":i as i64},
                "stopsImpacted":0,"packagesImpacted":i as i64,"totalBreaksDurationSecs":1800,"overtimeDurationSecs":0,
                "stopCompletionRate":1.0,"stopProgress":{"completed":2,"total":2},
                "breaks":[{"breakId":"break-1","punchId":"punch-1","sequenceNumber":1,"state":"OFF","type":"MEAL",
                    "timeStampOn":midnight + 4 * hour,"timeStampOff":midnight + 4 * hour + 1_800_000}],
                "plannedBreaks":[{"breakId":"planned-1","type":"MEAL","plannedStart":midnight + 4 * hour,
                    "plannedEnd":midnight + 4 * hour + 1_800_000}]})
        })
        .collect();
    let transporters: Vec<Value> = drivers
        .iter()
        .map(|(transporter, first, last, _)| {
            json!({"transporterId":transporter,"firstName":first,"lastName":last,"initials":"FD",
                "workPhoneNumber":"15550000000","personType":["DELIVERY_ASSOCIATE"],"lastLocation":null})
        })
        .collect();
    let companies = json!([{"companyId":scope.provider,"companyName":"Fixture Logistics","companyShortCode":"FXTR","id":scope.provider}]);
    let summaries_body = json!({"itinerarySummaries":summaries,"transporters":transporters,"companies":companies,
        "transporterPackageSummaries":[],"workHourAnomalies":[],"backendTime":started_at});
    let route_summaries = json!({"rmsRouteSummaries":drivers.iter().enumerate().map(|(i, (transporter, _, _, route))| {
            json!({"routeId":format!("{}", 1000 + i),"rmsRouteId":format!("rms-{}", i + 1),"routeCode":route,
                "serviceAreaId":scope.service_area_id,"companyId":scope.provider,"serviceTypeName":"Standard Parcel",
                "routeStatus":"COMPLETE","progressStatus":"ON_TIME","plannedDepartureTime":midnight,
                "routeCreationTime":midnight - 6 * hour,"routeDuration":28800,"totalStops":2,"totalTasks":3,
                "unassignedStops":0,"unassignedPackages":0,"lateDeparting":false,"preDispatch":false,"sameDayRoute":false,
                "transporters":[{"transporterId":transporter,"itineraryId":format!("itinerary-{}", i + 1)}]})
        }).collect::<Vec<_>>(),"transporters":transporters,"companies":companies,"transporterPackageSummaries":[],
        "workHourAnomalies":[],"backendTime":started_at});
    let addresses: Vec<Value> = (1..=2)
        .map(|n| {
            json!({"addressId":format!("address-{n}"),"address1":format!("{n}00 Example St"),"address2":null,"address3":null,
                "city":"Fixture","state":"CA","postalCode":"90000","customerName":null,"customerPhone":null,
                "geocode":{"latitude":34.0 + n as f64 / 100.0,"longitude":-118.0,"scope":0}})
        })
        .collect();
    let task = |index: usize, kind: &str, state: &str, address: &str, when: i64, tracking: &str| {
        json!({"taskId":format!("task-{index}"),"transporterId":null,"referenceId":format!("ref-{index}"),
            "taskType":kind,"taskState":state,"taskStateContext":if state == "DELIVERED" {"DELIVERED_TO_DOORSTEP"} else {"NONE"},
            "executionStatus":"COMPLETE","addressId":address,"promiseType":"STANDARD",
            "windowStartTime":midnight,"windowEndTime":midnight + 12 * hour,"actualExecutionTime":when,
            "executionGeocode":{"latitude":34.01,"longitude":-118.0,"scope":0},
            "recentTaskEvents":[{"taskState":state,"taskStateContext":"NONE","executionTime":when}],
            "length":"10.0","width":"5.0","height":"4.0","weight":"2.5","volumeUnit":"IN","weightUnit":"LB",
            "boxType":"SMALL_ENVELOPE","timeWindowed":false,"highValueTaskFlag":false,"customerReturn":false,
            "addressTypeInfo":"RESIDENTIAL",
            "domainMap":{"scannableId":tracking,"orderId":format!("order-{index}"),"trId":format!("ref-{index}"),"SOURCE":"AMAZON"}})
    };
    let itineraries = drivers
        .iter()
        .enumerate()
        .map(|(i, (transporter, _, _, route))| {
            let id = format!("itinerary-{}", i + 1);
            let base = 10 * i;
            let stops = json!([
                {"stopId":format!("stop-{}-1", i + 1),"sequenceNumber":1,"addressId":"station","itineraryStopType":"PICK_UP",
                 "routeCode":route,"plannedStartTime":midnight,"plannedEndTime":midnight + 600_000,
                 "expectedStartTime":midnight,"actualStartTime":null,
                 "dispatchStopFlag":true,
                 "tasks":[task(base + 1, "PICK_UP", "PICKED_UP", "station", midnight, "TBA000000000001"),
                          task(base + 2, "PICK_UP", "PICKED_UP", "station", midnight, "TBA000000000002")]},
                {"stopId":format!("stop-{}-2", i + 1),"sequenceNumber":2,"addressId":"address-1","itineraryStopType":"DROP_OFF",
                 "routeCode":route,"plannedStartTime":midnight + 2 * hour,"plannedEndTime":midnight + 2 * hour + 600_000,
                 "expectedStartTime":midnight + 2 * hour,"actualStartTime":null,
                 "highValueStopFlag":false,
                 "tasks":[task(base + 3, "DROP_OFF", "DELIVERED", "address-1", midnight + 2 * hour, "TBA000000000001"),
                          task(base + 4, "DROP_OFF", if i == 0 {"DELIVERED"} else {"UNDELIVERABLE"},
                               "address-2", midnight + 3 * hour, "TBA000000000002")]}
            ]);
            ItineraryCapture {
                id: id.clone(),
                transporter_id: (*transporter).into(),
                detail: json!({"itineraryDetails":{"itineraryId":id,"transporterId":transporter,"transporterName":"Fixture Driver",
                    "serviceAreaId":scope.service_area_id,"companyId":scope.provider,"localDate":[2026,9,25],"routeCode":route,
                    "executionStatus":"COMPLETE","progressStatus":"ON_TIME","itineraryStartTime":midnight + 10 * 60_000,
                    "plannedDepartureTime":midnight,"sessionEndTime":midnight + 8 * hour,"routeCreationTime":midnight - 6 * hour,
                    "routeDuration":28800,"itineraryDuration":28200,"stopProgress":{"completed":2,"total":2},
                    "transporterTimeAttributes":{"actualDepartureTime":midnight + 15 * 60_000,"plannedDepartureTime":midnight},
                    "breaks":[],"plannedBreaks":[],"inactiveTasks":[],"unknownStops":[],"stops":stops,"weatherData":null},
                    "addresses":addresses,"transporters":[transporters[i].clone()],"companies":companies,
                    "itineraryPartners":null,"routePauseStatus":null})
                .to_string(),
            }
        })
        .collect();
    let capture = Capture {
        version: 1,
        collection: Collection::Routes,
        mode: request.mode,
        scope,
        started_at,
        finished_at: now().max(started_at),
        summaries: summaries_body,
        route_summaries,
        itineraries,
        prepared: None,
    };
    capture.validate(request)?;
    Ok(capture)
}

/// Shapes every itinerary into its rows and compresses every body, outside any lock,
/// and drops the bodies from the capture: what remains is what publication inserts.
pub fn prepare(mut capture: Capture) -> Result<Capture> {
    let summaries: Map<String, Value> = capture.summaries["itinerarySummaries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| Some((v["itineraryId"].as_str()?.to_owned(), v.clone())))
        .collect();
    let people = capture.summaries["transporters"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut itineraries = Vec::with_capacity(capture.itineraries.len());
    for captured in &mut capture.itineraries {
        let parsed = captured.parsed()?;
        let detail = &parsed["itineraryDetails"];
        let summary = summaries.get(&captured.id).cloned().unwrap_or(Value::Null);
        let person = people
            .iter()
            .chain(parsed["transporters"].as_array().into_iter().flatten())
            .find(|t| t["transporterId"] == json!(captured.transporter_id))
            .cloned()
            .unwrap_or(Value::Null);
        let driver_name = match (text(&person["firstName"]), text(&person["lastName"])) {
            (Some(first), Some(last)) => format!("{first} {last}"),
            _ => {
                text(&detail["transporterName"]).unwrap_or_else(|| captured.transporter_id.clone())
            }
        };
        let flat = flatten(&summary, detail, &captured.transporter_id, &driver_name);
        let (stops, tasks, day) = rows(detail, &captured.transporter_id);
        let raw = std::mem::take(&mut captured.detail);
        itineraries.push(PreparedItinerary {
            id: captured.id.clone(),
            transporter_id: captured.transporter_id.clone(),
            columns: flat
                .itinerary
                .into_iter()
                .map(|(c, v)| (c.to_owned(), v))
                .collect(),
            stops,
            tasks,
            day: DayRow {
                departed_at: stamp(&detail["transporterTimeAttributes"]["actualDepartureTime"])
                    .or_else(|| stamp(&summary["itineraryStartTime"]))
                    .or_else(|| stamp(&detail["itineraryStartTime"])),
                session_end_at: stamp(&summary["sessionEndTime"])
                    .or_else(|| stamp(&detail["sessionEndTime"])),
                route_code: text(&summary["routeCode"]).or_else(|| text(&detail["routeCode"])),
                breaks_secs: integer(&summary["totalBreaksDurationSecs"]),
                overtime_secs: integer(&summary["overtimeDurationSecs"])
                    .or_else(|| integer(&detail["overtimeDurationSecs"])),
                ..day
            },
            addresses: parsed["addresses"].as_array().cloned().unwrap_or_default(),
            person,
            raw_bytes: raw.len(),
            raw: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                gzip(raw.as_bytes())?,
            ),
        });
    }
    let summaries_text = capture.summaries.to_string();
    let route_summaries_text = capture.route_summaries.to_string();
    capture.prepared = Some(Prepared {
        itineraries,
        summaries_bytes: summaries_text.len(),
        summaries: base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            gzip(summaries_text.as_bytes())?,
        ),
        route_summaries_bytes: route_summaries_text.len(),
        route_summaries: base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            gzip(route_summaries_text.as_bytes())?,
        ),
    });
    Ok(capture)
}
fn decode(blob: &str) -> Result<Vec<u8>> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, blob)
        .map_err(|_| Error::new("routes_capture_invalid", 502))
}

fn verify(db: &Db) -> Result<()> {
    ensure(
        db.all("SELECT version FROM itineraries_schema", [])? == vec![json!({"version":1})],
        "unsupported_itineraries_schema",
        503,
    )?;
    // Missing initialized tables fail closed, rather than recreating lost data.
    for table in [
        "route_publications",
        "routes",
        "itineraries",
        "stops",
        "tasks",
        "addresses",
        "drivers",
        "driver_days",
        "route_raw",
    ] {
        db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
    }
    Ok(())
}

/// The day `mode` reads by default: yesterday for a final read, today for a snapshot.
pub fn default_day(mode: Mode, today: NaiveDate) -> String {
    match mode {
        Mode::Final => (today - Duration::days(1)).to_string(),
        Mode::Snapshot => today.to_string(),
    }
}
/// Whether `day` can be read in `mode` on `today`: a final read needs a completed day,
/// a snapshot reads today only.
pub fn day_allowed(mode: Mode, day: &str, today: NaiveDate) -> Result<()> {
    let date =
        NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| Error::new("invalid_date", 400))?;
    ensure(date.to_string() == day, "invalid_date", 400)?;
    ensure(
        match mode {
            Mode::Final => date < today,
            Mode::Snapshot => date == today,
        },
        "routes_day_not_available",
        400,
    )
}

/// A driver's itinerary flattened for its row.
struct Flat {
    itinerary: Vec<(&'static str, Value)>,
}

impl Store {
    pub fn itineraries(&self, id: &str) -> Result<DspLease<'_>> {
        self.added_storage(id, Provider::Cortex, &STORAGE)
    }
    /// Today, where the DSP is.
    fn routes_today(&self, id: &str) -> Result<NaiveDate> {
        let tz: chrono_tz::Tz = self
            .find_dsp(id)?
            .timezone
            .parse()
            .map_err(|_| Error::new("invalid_timezone", 400))?;
        Ok(chrono::Utc::now().with_timezone(&tz).date_naive())
    }
    /// The request for one day: the DSP's station and names, which the driver resolves
    /// to a service area and provider on Cortex.
    pub(crate) fn routes_request(&self, id: &str, day: &str, mode: Mode) -> Result<Value> {
        let profile = self.profile(id)?;
        let dsp = self.find_dsp(id)?;
        ensure(
            !profile.station_code.is_empty(),
            "routes_station_required",
            409,
        )?;
        // A scope proven by an earlier day at this station spares discovery, which
        // starts from an address Cortex may send elsewhere.
        let known = self.itineraries(id)?.one(
            "SELECT service_area_id,provider FROM route_publications WHERE station=? AND active=1 \
             ORDER BY collected_at DESC LIMIT 1",
            [&profile.station_code],
        )?;
        let request = Request {
            collection: Collection::Routes,
            mode,
            date: day.into(),
            station: profile.station_code,
            timezone: dsp.timezone,
            dsp_name: dsp.name,
            dsp_abbreviation: profile.abbreviation,
            service_area_id: known.as_ref().map(|k| s(k, "service_area_id").to_owned()),
            provider: known.as_ref().map(|k| s(k, "provider").to_owned()),
        };
        request.validate()?;
        Ok(serde_json::to_value(request)?)
    }
    /// Queues `days` days ending at `day`, or the default day of `mode`, answering with
    /// the jobs. A day already queued under `key` answers the same job.
    pub fn enqueue_routes(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        day: Option<&str>,
        mode: Mode,
        days: i64,
    ) -> Result<Value> {
        ensure(
            (1..=MAX_DAYS_PER_REQUEST).contains(&days),
            "invalid_input",
            400,
        )?;
        let today = self.routes_today(id)?;
        let last = day
            .map(str::to_owned)
            .unwrap_or_else(|| default_day(mode, today));
        day_allowed(mode, &last, today)?;
        ensure(days == 1 || mode == Mode::Final, "invalid_input", 400)?;
        let last_date = NaiveDate::parse_from_str(&last, "%Y-%m-%d")
            .map_err(|_| Error::new("invalid_date", 400))?;
        let requests = (0..days)
            .map(|back| {
                let day = (last_date - Duration::days(back)).to_string();
                let suffix = if days == 1 {
                    String::new()
                } else {
                    format!(":{day}")
                };
                Ok((
                    format!("{key}{suffix}"),
                    Provider::Cortex,
                    self.routes_request(id, &day, mode)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let jobs = self.enqueue_batch(id, actor, &requests)?;
        Ok(serde_json::to_value(jobs)?)
    }
    pub fn routes_schedule_ready(&self, id: &str) -> Result<()> {
        ensure(
            !self.profile(id)?.station_code.is_empty(),
            "routes_station_required",
            409,
        )
    }
    /// The days a scheduled run collects: yesterday, and recent days without a final
    /// publication, newest first.
    pub fn routes_jobs(&self, id: &str) -> Result<Vec<(String, Value)>> {
        let today = self.routes_today(id)?;
        let station = self.profile(id)?.station_code;
        let db = self.itineraries(id)?;
        let mut jobs = Vec::new();
        for back in 1..=(BACKFILL_DAYS as i64) {
            let day = (today - Duration::days(back)).to_string();
            let published = db.one(
                "SELECT id FROM route_publications WHERE day=? AND station=? AND mode='final' AND active=1",
                [&day, &station],
            )?;
            if published.is_none() {
                jobs.push((
                    format!("routes:{day}"),
                    self.routes_request(id, &day, Mode::Final)?,
                ));
                if jobs.len() >= MAX_JOBS_PER_RUN {
                    break;
                }
            }
        }
        Ok(jobs)
    }
    /// Stores a day's capture as its active publication, replacing the day's previous
    /// one. One transaction: a reader never sees part of a day. A capture prepared
    /// beforehand only inserts here; one that was not is prepared first.
    pub fn publish_routes(&self, id: &str, job: &str, capture: &Capture) -> Result<()> {
        let request: Value = serde_json::from_str(&self.job_row(job, Some(id))?.request)?;
        let request = Request::parse(&request)?.ok_or_else(|| Error::new("invalid_input", 400))?;
        capture.validate(&request)?;
        let prepared = match &capture.prepared {
            Some(prepared) => prepared.clone(),
            None => prepare(capture.clone())?
                .prepared
                .ok_or_else(|| Error::new("routes_capture_invalid", 502))?,
        };
        let db = self.itineraries(id)?;
        let day = capture.scope.date.as_str();
        let publication = crate::crypto::id("routes")?;
        let routes = capture.route_summaries["rmsRouteSummaries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let stop_count: usize = prepared.itineraries.iter().map(|i| i.stops.len()).sum();
        let task_count: usize = prepared.itineraries.iter().map(|i| i.tasks.len()).sum();
        db.transaction(|| {
            let connection = &db.0;
            connection.prepare_cached(
                "INSERT INTO route_publications(id,job_id,day,station,service_area_id,provider,timezone,mode,\
                 started_at,collected_at,active,route_count,itinerary_count,stop_count,task_count,adapter_version) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,0,?,?,?,?,?)",
            )?.execute(params![
                publication,
                job,
                day,
                capture.scope.station,
                capture.scope.service_area_id,
                capture.scope.provider,
                capture.scope.timezone,
                capture.mode.as_str(),
                at(capture.started_at),
                at(capture.finished_at),
                routes.len() as i64,
                prepared.itineraries.len() as i64,
                stop_count as i64,
                task_count as i64,
                ADAPTER_VERSION
            ])?;
            let mut insert_route = connection.prepare_cached(
                "INSERT OR REPLACE INTO routes(publication_id,day,route_id,rms_route_id,route_code,company_id,service_type,\
                 route_status,progress_status,planned_departure_at,created_at,duration_secs,total_stops,total_tasks,\
                 unassigned_stops,unassigned_packages,late_departing,pre_dispatch,same_day,transporter_ids,summary) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            )?;
            for route in &routes {
                let transporters: Vec<Value> = route["transporters"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|t| t["transporterId"].as_str().map(|v| json!(v)))
                    .collect();
                insert_route.execute(params![
                    publication,
                    day,
                    text(&route["routeId"]).unwrap_or_else(|| text(&route["rmsRouteId"]).unwrap_or_default()),
                    text(&route["rmsRouteId"]),
                    text(&route["routeCode"]),
                    text(&route["companyId"]),
                    text(&route["serviceTypeName"]),
                    text(&route["routeStatus"]),
                    text(&route["progressStatus"]),
                    stamp(&route["plannedDepartureTime"]),
                    stamp(&route["routeCreationTime"]),
                    integer(&route["routeDuration"]),
                    integer(&route["totalStops"]),
                    integer(&route["totalTasks"]),
                    integer(&route["unassignedStops"]),
                    integer(&route["unassignedPackages"]),
                    flag(&route["lateDeparting"]),
                    flag(&route["preDispatch"]),
                    flag(&route["sameDayRoute"]),
                    Value::Array(transporters).to_string(),
                    route.to_string()
                ])?;
            }
            let mut insert_stop = connection.prepare_cached(
                "INSERT OR REPLACE INTO stops(publication_id,day,itinerary_id,stop_id,sequence,address_id,stop_type,route_code,\
                 planned_start_at,planned_end_at,expected_start_at,actual_start_at,flags) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
            )?;
            let mut insert_task = connection.prepare_cached(
                "INSERT OR REPLACE INTO tasks(publication_id,day,itinerary_id,stop_id,task_id,transporter_id,tracking_id,order_id,\
                 task_type,task_state,state_context,execution_status,address_id,window_start_at,window_end_at,executed_at,\
                 latitude,longitude,box_type,weight,weight_unit,length,width,height,volume_unit,time_windowed,high_value,\
                 customer_return,address_type,events) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            )?;
            let mut insert_day = connection.prepare_cached(
                "INSERT INTO driver_days(publication_id,day,transporter_id,itinerary_id,route_code,departed_at,first_stop_at,\
                 last_stop_at,session_end_at,stops_total,stops_completed,tasks_total,delivered,picked_up,not_delivered,\
                 breaks_secs,overtime_secs) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            )?;
            let mut upsert_address = connection.prepare_cached(
                "INSERT INTO addresses(address_id,address1,address2,address3,city,state,postal_code,customer_name,\
                 customer_phone,latitude,longitude,first_seen_day,last_seen_day) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?) \
                 ON CONFLICT(address_id) DO UPDATE SET address1=excluded.address1,address2=excluded.address2,\
                 address3=excluded.address3,city=excluded.city,state=excluded.state,postal_code=excluded.postal_code,\
                 customer_name=excluded.customer_name,customer_phone=excluded.customer_phone,latitude=excluded.latitude,\
                 longitude=excluded.longitude,first_seen_day=min(first_seen_day,excluded.first_seen_day),\
                 last_seen_day=max(last_seen_day,excluded.last_seen_day)",
            )?;
            let mut upsert_driver = connection.prepare_cached(
                "INSERT INTO drivers(transporter_id,first_name,last_name,initials,work_phone,person_type,company_id,\
                 first_seen_day,last_seen_day) VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT(transporter_id) DO UPDATE SET \
                 first_name=excluded.first_name,last_name=excluded.last_name,initials=excluded.initials,\
                 work_phone=excluded.work_phone,person_type=excluded.person_type,company_id=excluded.company_id,\
                 first_seen_day=min(first_seen_day,excluded.first_seen_day),\
                 last_seen_day=max(last_seen_day,excluded.last_seen_day)",
            )?;
            let mut insert_raw = connection.prepare_cached(
                "INSERT INTO route_raw(publication_id,name,encoding,raw_bytes,body) VALUES (?,?,'gzip',?,?)",
            )?;
            for itinerary in &prepared.itineraries {
                let columns: Vec<&str> = itinerary.columns.iter().map(|(c, _)| c.as_str()).collect();
                let placeholders = vec!["?"; columns.len() + 3].join(",");
                let mut bound: Vec<Value> = vec![json!(publication), json!(day), json!(itinerary.id)];
                bound.extend(itinerary.columns.iter().map(|(_, v)| v.clone()));
                connection
                    .prepare_cached(&format!(
                        "INSERT INTO itineraries(publication_id,day,itinerary_id,{}) VALUES ({placeholders})",
                        columns.join(",")
                    ))?
                    .execute(rusqlite::params_from_iter(bound.iter().map(sql_value)))?;
                for stop in &itinerary.stops {
                    insert_stop.execute(params![
                        publication, day, itinerary.id, stop.id, stop.sequence, stop.address_id, stop.stop_type,
                        stop.route_code, stop.planned_start_at, stop.planned_end_at, stop.expected_start_at,
                        stop.actual_start_at, stop.flags.to_string()
                    ])?;
                }
                for task in &itinerary.tasks {
                    insert_task.execute(params![
                        publication, day, itinerary.id, task.stop_id, task.id, itinerary.transporter_id, task.tracking_id,
                        task.order_id, task.task_type, task.task_state, task.state_context, task.execution_status,
                        task.address_id, task.window_start_at, task.window_end_at, task.executed_at, task.latitude,
                        task.longitude, task.box_type, task.weight, task.weight_unit, task.length, task.width, task.height,
                        task.volume_unit, task.time_windowed, task.high_value, task.customer_return, task.address_type,
                        task.events.to_string()
                    ])?;
                }
                let d = &itinerary.day;
                insert_day.execute(params![
                    publication, day, itinerary.transporter_id, itinerary.id, d.route_code, d.departed_at,
                    d.first_stop_at, d.last_stop_at, d.session_end_at, d.stops_total, d.stops_completed, d.tasks_total,
                    d.delivered, d.picked_up, d.not_delivered, d.breaks_secs, d.overtime_secs
                ])?;
                for address in &itinerary.addresses {
                    let Some(address_id) = text(&address["addressId"]) else { continue };
                    upsert_address.execute(params![
                        address_id, text(&address["address1"]), text(&address["address2"]), text(&address["address3"]),
                        text(&address["city"]), text(&address["state"]), text(&address["postalCode"]),
                        text(&address["customerName"]), text(&address["customerPhone"]),
                        number(&address["geocode"]["latitude"]), number(&address["geocode"]["longitude"]), day, day
                    ])?;
                }
                let person = &itinerary.person;
                if !person.is_null() {
                    let company = itinerary
                        .columns
                        .iter()
                        .find(|(c, _)| c == "company_id")
                        .and_then(|(_, v)| v.as_str().map(str::to_owned));
                    upsert_driver.execute(params![
                        itinerary.transporter_id, text(&person["firstName"]), text(&person["lastName"]),
                        text(&person["initials"]), text(&person["workPhoneNumber"]),
                        person["personType"].as_array().map(|v| Value::Array(v.clone()).to_string()).unwrap_or_else(|| "[]".into()),
                        company, day, day
                    ])?;
                }
                insert_raw.execute(params![
                    publication,
                    format!("itinerary:{}", itinerary.id),
                    itinerary.raw_bytes as i64,
                    decode(&itinerary.raw)?
                ])?;
            }
            insert_raw.execute(params![publication, "summaries", prepared.summaries_bytes as i64, decode(&prepared.summaries)?])?;
            insert_raw.execute(params![
                publication,
                "route_summaries",
                prepared.route_summaries_bytes as i64,
                decode(&prepared.route_summaries)?
            ])?;
            // The day's previous reading goes, and everything under it with it.
            db.exec(
                "DELETE FROM route_publications WHERE day=? AND station=? AND service_area_id=? AND provider=? AND id<>?",
                params![day, capture.scope.station, capture.scope.service_area_id, capture.scope.provider, publication],
            )?;
            db.exec(
                "UPDATE route_publications SET active=1 WHERE id=?",
                [&publication],
            )?;
            Ok(())
        })
    }
    /// Every day with a publication at the DSP's station, newest first.
    pub fn route_days(&self, id: &str) -> Result<RouteDays> {
        let today = self.routes_today(id)?;
        let station = self.profile(id)?.station_code;
        let db = self.itineraries(id)?;
        let days = db
            .all(
                "SELECT id,day,mode,station,collected_at,route_count,itinerary_count,stop_count,task_count \
                 FROM route_publications WHERE station=? AND active=1 ORDER BY day DESC",
                [&station],
            )?
            .iter()
            .map(publication)
            .collect();
        Ok(RouteDays {
            station,
            latest_day: default_day(Mode::Final, today),
            days,
        })
    }
    /// A day's publication with its driver summaries, or none.
    pub fn route_day(&self, id: &str, day: &str) -> Result<Option<RouteDayView>> {
        crate::validate::date(day)?;
        let station = self.profile(id)?.station_code;
        let db = self.itineraries(id)?;
        let Some(row) = db.one(
            "SELECT id,day,mode,station,collected_at,route_count,itinerary_count,stop_count,task_count \
             FROM route_publications WHERE station=? AND day=? AND active=1",
            [&station, day],
        )?
        else {
            return Ok(None);
        };
        let itineraries = db
            .all(
                "SELECT i.itinerary_id,i.transporter_id,i.driver_name,i.route_code,i.execution_status,i.progress_status,\
                 d.departed_at,d.first_stop_at,d.last_stop_at,d.session_end_at,d.stops_total,d.stops_completed,d.tasks_total,\
                 d.delivered,d.picked_up,d.not_delivered,d.breaks_secs,d.overtime_secs \
                 FROM itineraries i JOIN driver_days d ON d.publication_id=i.publication_id AND d.itinerary_id=i.itinerary_id \
                 WHERE i.publication_id=? ORDER BY i.route_code,i.driver_name",
                [s(&row, "id")],
            )?
            .iter()
            .map(|r| RouteItinerary {
                itinerary_id: s(r, "itinerary_id").into(),
                transporter_id: s(r, "transporter_id").into(),
                driver_name: s(r, "driver_name").into(),
                route_code: r["route_code"].as_str().map(str::to_owned),
                execution_status: r["execution_status"].as_str().map(str::to_owned),
                progress_status: r["progress_status"].as_str().map(str::to_owned),
                departed_at: r["departed_at"].as_i64(),
                first_stop_at: r["first_stop_at"].as_i64(),
                last_stop_at: r["last_stop_at"].as_i64(),
                session_end_at: r["session_end_at"].as_i64(),
                stops_total: r["stops_total"].as_i64().unwrap_or(0),
                stops_completed: r["stops_completed"].as_i64().unwrap_or(0),
                tasks_total: r["tasks_total"].as_i64().unwrap_or(0),
                delivered: r["delivered"].as_i64().unwrap_or(0),
                picked_up: r["picked_up"].as_i64().unwrap_or(0),
                not_delivered: r["not_delivered"].as_i64().unwrap_or(0),
                breaks_secs: r["breaks_secs"].as_i64(),
                overtime_secs: r["overtime_secs"].as_i64(),
            })
            .collect();
        Ok(Some(RouteDayView {
            publication: publication(&row),
            itineraries,
        }))
    }
}
fn publication(row: &Value) -> RoutePublication {
    RoutePublication {
        id: s(row, "id").into(),
        day: s(row, "day").into(),
        mode: s(row, "mode").into(),
        station: s(row, "station").into(),
        collected_at: s(row, "collected_at").into(),
        route_count: row["route_count"].as_i64().unwrap_or(0),
        itinerary_count: row["itinerary_count"].as_i64().unwrap_or(0),
        stop_count: row["stop_count"].as_i64().unwrap_or(0),
        task_count: row["task_count"].as_i64().unwrap_or(0),
    }
}
fn sql_value(value: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as Sql;
    match value {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(i64::from(*b)),
        Value::Number(n) => n
            .as_i64()
            .map(Sql::Integer)
            .unwrap_or_else(|| Sql::Real(n.as_f64().unwrap_or(0.0))),
        Value::String(text) => Sql::Text(text.clone()),
        other => Sql::Text(other.to_string()),
    }
}
fn opt<T: Into<Value>>(value: Option<T>) -> Value {
    value.map(Into::into).unwrap_or(Value::Null)
}
/// The itinerary row's columns from the list's summary and the detail.
fn flatten(summary: &Value, detail: &Value, transporter: &str, driver_name: &str) -> Flat {
    let either = |key: &str| {
        if summary[key].is_null() {
            &detail[key]
        } else {
            &summary[key]
        }
    };
    let packages = &summary["packageSummary"];
    Flat {
        itinerary: vec![
            ("transporter_id", json!(transporter)),
            ("driver_name", json!(driver_name)),
            ("route_code", opt(text(either("routeCode")))),
            (
                "route_id",
                opt(text(&summary["routes"][0]["routeId"]).or_else(|| text(&detail["rmsRouteId"]))),
            ),
            ("company_id", opt(text(either("companyId")))),
            ("vin", opt(text(&summary["vinNumber"]))),
            ("service_type", opt(text(either("serviceTypeName")))),
            ("execution_status", opt(text(either("executionStatus")))),
            ("progress_status", opt(text(either("progressStatus")))),
            (
                "planned_departure_at",
                opt(stamp(either("plannedDepartureTime"))),
            ),
            (
                "departed_at",
                opt(stamp(
                    &detail["transporterTimeAttributes"]["actualDepartureTime"],
                )),
            ),
            ("started_at", opt(stamp(either("itineraryStartTime")))),
            ("wave_start_at", opt(stamp(either("waveStartTime")))),
            ("session_end_at", opt(stamp(either("sessionEndTime")))),
            (
                "projected_completion_at",
                opt(stamp(either("projectedCompletionTime"))),
            ),
            (
                "last_stop_at",
                opt(stamp(&summary["lastStopExecutionTime"])),
            ),
            (
                "block_minutes",
                opt(integer(either("blockDurationInMinutes"))),
            ),
            ("total_locations", opt(integer(&summary["totalLocations"]))),
            (
                "completed_locations",
                opt(integer(&summary["completedLocations"])),
            ),
            ("total_packages", opt(integer(&summary["totalPackages"]))),
            ("delivered", opt(integer(&packages["DELIVERED"]))),
            ("remaining", opt(integer(&packages["REMAINING"]))),
            ("undeliverable", opt(integer(&packages["UNDELIVERABLE"]))),
            ("stops_impacted", opt(integer(either("stopsImpacted")))),
            (
                "packages_impacted",
                opt(integer(either("packagesImpacted"))),
            ),
            (
                "breaks_secs",
                opt(integer(&summary["totalBreaksDurationSecs"])),
            ),
            (
                "overtime_secs",
                opt(integer(either("overtimeDurationSecs"))),
            ),
            (
                "stop_completion_rate",
                opt(number(either("stopCompletionRate"))),
            ),
            (
                "breaks",
                json!(
                    either("breaks")
                        .as_array()
                        .map(|v| Value::Array(v.clone()).to_string())
                        .unwrap_or_else(|| "[]".into())
                ),
            ),
            (
                "planned_breaks",
                json!(
                    either("plannedBreaks")
                        .as_array()
                        .map(|v| Value::Array(v.clone()).to_string())
                        .unwrap_or_else(|| "[]".into())
                ),
            ),
            (
                "summary",
                json!(if summary.is_null() {
                    "{}".to_owned()
                } else {
                    summary.to_string()
                }),
            ),
        ],
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct StopRow {
    id: String,
    sequence: Option<i64>,
    address_id: Option<String>,
    stop_type: Option<String>,
    route_code: Option<String>,
    planned_start_at: Option<i64>,
    planned_end_at: Option<i64>,
    expected_start_at: Option<i64>,
    actual_start_at: Option<i64>,
    flags: Value,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct TaskRow {
    id: String,
    stop_id: String,
    tracking_id: Option<String>,
    order_id: Option<String>,
    task_type: Option<String>,
    task_state: Option<String>,
    state_context: Option<String>,
    execution_status: Option<String>,
    address_id: Option<String>,
    window_start_at: Option<i64>,
    window_end_at: Option<i64>,
    executed_at: Option<i64>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    box_type: Option<String>,
    weight: Option<f64>,
    weight_unit: Option<String>,
    length: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    volume_unit: Option<String>,
    time_windowed: Option<i64>,
    high_value: Option<i64>,
    customer_return: Option<i64>,
    address_type: Option<String>,
    events: Value,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct DayRow {
    route_code: Option<String>,
    departed_at: Option<i64>,
    session_end_at: Option<i64>,
    breaks_secs: Option<i64>,
    overtime_secs: Option<i64>,
    first_stop_at: Option<i64>,
    last_stop_at: Option<i64>,
    stops_total: i64,
    stops_completed: i64,
    tasks_total: i64,
    delivered: i64,
    picked_up: i64,
    not_delivered: i64,
}
/// The stops and tasks of a detail, and the day summary they add up to. A task or
/// stop without an identifier, or one repeated, is skipped: it cannot be a row.
fn rows(detail: &Value, transporter: &str) -> (Vec<StopRow>, Vec<TaskRow>, DayRow) {
    let mut stops = Vec::new();
    let mut tasks = Vec::new();
    let mut day = DayRow::default();
    let mut seen_stops = std::collections::HashSet::new();
    let mut seen_tasks = std::collections::HashSet::new();
    for (index, stop) in detail["stops"].as_array().into_iter().flatten().enumerate() {
        // The API sends no stop id; the page adds one. The sequence number is the
        // stop's identity within its itinerary, and its position stands in without one.
        let stop_id = text(&stop["stopId"])
            .filter(|v| identifier(v, 256))
            .or_else(|| integer(&stop["sequenceNumber"]).map(|n| format!("seq-{n}")))
            .unwrap_or_else(|| format!("index-{index}"));
        if !seen_stops.insert(stop_id.clone()) {
            continue;
        }
        let mut completed = false;
        for task in stop["tasks"].as_array().into_iter().flatten() {
            let Some(task_id) = text(&task["taskId"]).filter(|v| identifier(v, 256)) else {
                continue;
            };
            if !seen_tasks.insert(task_id.clone()) {
                continue;
            }
            // A task another driver executed belongs to that driver's itinerary.
            if task["transporterId"]
                .as_str()
                .is_some_and(|t| t != transporter)
            {
                continue;
            }
            let executed_at =
                stamp(&task["actualExecutionTime"]).or_else(|| stamp(&task["taskExecutionTime"]));
            let task_type = text(&task["taskType"]);
            let task_state = text(&task["taskState"]);
            day.tasks_total += 1;
            match (task_type.as_deref(), task_state.as_deref()) {
                (Some("DROP_OFF"), Some("DELIVERED")) => {
                    day.delivered += 1;
                    completed = true;
                    if let Some(at) = executed_at {
                        day.first_stop_at = Some(day.first_stop_at.map_or(at, |v| v.min(at)));
                        day.last_stop_at = Some(day.last_stop_at.map_or(at, |v| v.max(at)));
                    }
                }
                (Some("DROP_OFF"), _) => day.not_delivered += 1,
                (Some("PICK_UP"), Some("PICKED_UP")) => day.picked_up += 1,
                _ => (),
            }
            let dimension = |key: &str| number(&task[key]);
            tasks.push(TaskRow {
                id: task_id,
                stop_id: stop_id.clone(),
                tracking_id: text(&task["domainMap"]["scannableId"])
                    .or_else(|| text(&task["scannableId"])),
                order_id: text(&task["domainMap"]["orderId"]),
                task_type,
                task_state,
                state_context: text(&task["taskStateContext"]),
                execution_status: text(&task["executionStatus"]),
                address_id: text(&task["addressId"]),
                window_start_at: stamp(&task["windowStartTime"]),
                window_end_at: stamp(&task["windowEndTime"]),
                executed_at,
                latitude: number(&task["executionGeocode"]["latitude"]),
                longitude: number(&task["executionGeocode"]["longitude"]),
                box_type: text(&task["boxType"]),
                weight: dimension("weight"),
                weight_unit: text(&task["weightUnit"]),
                length: dimension("length"),
                width: dimension("width"),
                height: dimension("height"),
                volume_unit: text(&task["volumeUnit"]),
                time_windowed: flag(&task["timeWindowed"]),
                high_value: flag(&task["highValueTaskFlag"]),
                customer_return: flag(&task["customerReturn"]),
                address_type: text(&task["addressTypeInfo"]),
                events: task["recentTaskEvents"]
                    .as_array()
                    .map(|v| Value::Array(v.clone()))
                    .unwrap_or_else(|| json!([])),
            });
        }
        day.stops_total += 1;
        if completed {
            day.stops_completed += 1;
        }
        let mut flags = stop.clone();
        if let Some(object) = flags.as_object_mut() {
            object.retain(|k, v| k.ends_with("Flag") || (k.starts_with("is") && v.is_boolean()));
        }
        stops.push(StopRow {
            id: stop_id,
            sequence: integer(&stop["sequenceNumber"]),
            address_id: text(&stop["addressId"]),
            stop_type: text(&stop["itineraryStopType"]),
            route_code: text(&stop["routeCode"]),
            planned_start_at: stamp(&stop["plannedStartTime"]),
            planned_end_at: stamp(&stop["plannedEndTime"]),
            expected_start_at: stamp(&stop["expectedStartTime"]),
            actual_start_at: stamp(&stop["actualStartTime"]),
            flags: set_flags(&flags),
        });
    }
    if let Some(completed) = integer(&detail["stopProgress"]["completed"]) {
        day.stops_completed = completed;
    }
    (stops, tasks, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(mode: Mode) -> Request {
        Request {
            collection: Collection::Routes,
            mode,
            date: "2026-09-25".into(),
            station: "TST1".into(),
            timezone: "America/Los_Angeles".into(),
            dsp_name: "Northline Logistics".into(),
            dsp_abbreviation: "NLOG".into(),
            service_area_id: None,
            provider: None,
        }
    }
    #[test]
    fn requests_are_recognized_and_carry_a_scope_once_resolved() {
        assert!(
            Request::parse(&json!({"date":"2026-09-25"}))
                .unwrap()
                .is_none()
        );
        let value = serde_json::to_value(request(Mode::Final)).unwrap();
        assert_eq!(value["collection"], "routes");
        assert_eq!(value["mode"], "final");
        assert!(value.get("serviceAreaId").is_none());
        let parsed = Request::parse(&value).unwrap().unwrap();
        assert!(matches!(
            parsed.scope_request(),
            CollectionRequest::Discover(_)
        ));
        let mut scoped = request(Mode::Snapshot);
        scoped.service_area_id = Some("area-1".into());
        scoped.provider = Some("provider-1".into());
        assert!(matches!(
            scoped.scope_request(),
            CollectionRequest::Scoped(_)
        ));
        let mut half = request(Mode::Final);
        half.provider = Some("provider-1".into());
        assert!(half.validate().is_err());
        assert!(
            Request::parse(&json!({"collection":"routes","date":"2026-09-25","station":"tst1"}))
                .is_err()
        );
    }
    #[test]
    fn days_follow_the_mode() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 26).unwrap();
        assert_eq!(default_day(Mode::Final, today), "2026-09-25");
        assert_eq!(default_day(Mode::Snapshot, today), "2026-09-26");
        assert!(day_allowed(Mode::Final, "2026-09-25", today).is_ok());
        assert!(day_allowed(Mode::Final, "2026-09-26", today).is_err());
        assert!(day_allowed(Mode::Snapshot, "2026-09-26", today).is_ok());
        assert!(day_allowed(Mode::Snapshot, "2026-09-25", today).is_err());
        assert!(day_allowed(Mode::Final, "2026-9-25", today).is_err());
    }
    #[test]
    fn the_fixture_is_a_valid_day_whose_rows_add_up() {
        let capture = fixture(&request(Mode::Final)).unwrap();
        assert_eq!(capture.itineraries.len(), 2);
        let (stops, tasks, day) = rows(
            &capture.itineraries[1].parsed().unwrap()["itineraryDetails"],
            "driver-2",
        );
        assert_eq!((stops.len(), tasks.len()), (2, 4));
        assert_eq!((day.delivered, day.picked_up, day.not_delivered), (1, 2, 1));
        assert_eq!((day.stops_total, day.stops_completed), (2, 2));
        assert_eq!(tasks[0].tracking_id.as_deref(), Some("TBA000000000001"));
        assert_eq!(stops[0].flags, json!(["dispatchStopFlag"]));
        assert!(day.first_stop_at.unwrap() <= day.last_stop_at.unwrap());
        let inflated = gunzip(&gzip(b"{\"a\":1}").unwrap()).unwrap();
        assert_eq!(inflated, b"{\"a\":1}");
    }
    #[test]
    fn stamps_take_seconds_and_milliseconds() {
        assert_eq!(stamp(&json!(1_758_800_000)), Some(1_758_800_000_000));
        assert_eq!(stamp(&json!(1_758_800_000_123i64)), Some(1_758_800_000_123));
        assert_eq!(stamp(&json!(null)), None);
        assert_eq!(stamp(&json!(-5)), None);
    }
}
