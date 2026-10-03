//! What a routes collection reads from Cortex: a day's route list, the newer routes
//! page's list and every itinerary, each kept as Amazon sent it.
use crate::discovery::{CollectionRequest, Discovery, Scope};
use chrono::NaiveDate;
use dispatch_core::{Error, Result, db::now, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const JOB_KIND: &str = "cortex.routes.collect";
/// Itineraries a day may list, well above any station seen.
pub const MAX_ITINERARIES: usize = 1000;
/// Stops and tasks one itinerary may hold.
pub const MAX_STOPS: usize = 5000;
pub const MAX_TASKS: usize = 20_000;
/// As much as one response may be. The largest itinerary seen was 1.4 MB.
pub const MAX_BODY: usize = 32 * 1024 * 1024;
/// Uncompressed provider responses retained for one day. Ordinary days hold tens of
/// megabytes; this bounds their combined parse, compression and storage amplification.
pub const MAX_CAPTURE_BYTES: usize = 128 * 1024 * 1024;

/// Adds one response to a day's total without allowing overflow or a partial capture.
pub fn add_capture_bytes(total: usize, additional: usize) -> Result<usize> {
    let next = total
        .checked_add(additional)
        .filter(|next| *next <= MAX_CAPTURE_BYTES);
    next.ok_or_else(|| Error::new("routes_source_too_large", 502))
}

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
/// A day's routes as collected, before they are stored: the two lists and every
/// itinerary, each as Amazon sent it.
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
}
/// Only what validation reads of an itinerary: whose it is and how many stops and
/// tasks it has, without building the rest of its tree.
#[derive(Deserialize)]
struct Outline {
    #[serde(rename = "itineraryDetails")]
    details: OutlineDetails,
}
#[derive(Deserialize)]
struct OutlineDetails {
    #[serde(rename = "itineraryId")]
    itinerary: Value,
    #[serde(rename = "transporterId")]
    transporter: Value,
    stops: Option<Vec<OutlineStop>>,
}
#[derive(Deserialize)]
struct OutlineStop {
    tasks: Option<Vec<serde::de::IgnoredAny>>,
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
    /// The total uncompressed provider data this capture holds.
    pub fn validate_response_budget(&self) -> Result<()> {
        let mut total = add_capture_bytes(0, serde_json::to_vec(&self.summaries)?.len())?;
        total = add_capture_bytes(total, serde_json::to_vec(&self.route_summaries)?.len())?;
        for itinerary in &self.itineraries {
            total = add_capture_bytes(total, itinerary.detail.len())?;
        }
        Ok(())
    }
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
        self.validate_response_budget()?;
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
            let outline: Outline = serde_json::from_str(&captured.detail)
                .map_err(|_| Error::new("routes_capture_invalid", 502))?;
            let stops = outline.details.stops.unwrap_or_default();
            ensure(
                outline.details.itinerary == json!(id)
                    && outline.details.transporter == json!(transporter)
                    && stops.len() <= MAX_STOPS,
                "routes_scope_mismatch",
                502,
            )?;
            let tasks: usize = stops
                .iter()
                .map(|stop| stop.tasks.as_ref().map_or(0, Vec::len))
                .sum();
            ensure(tasks <= MAX_TASKS, "routes_source_too_large", 502)?;
        }
        Ok(())
    }
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
                "city":"Fixture","state":"CA","postalCode":"90000",
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
                    "breaks":[],"plannedBreaks":[],
                    "inactiveTasks":[task(base + 9, "DROP_OFF", "INACTIVE", "address-2", midnight + hour, "TBA000000000009")],
                    "unknownStops":[{"enterTime":midnight + 5 * hour,"exitTime":midnight + 5 * hour + 600_000,
                        "enterLatitude":34.02,"enterLongitude":-118.01,"exitLatitude":34.02,"exitLongitude":-118.01}],
                    "rescueActions":[{"type":"RESCUED","transporterId":"driver-9"}],
                    "stops":stops,"weatherData":null},
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
    };
    capture.validate(request)?;
    Ok(capture)
}
