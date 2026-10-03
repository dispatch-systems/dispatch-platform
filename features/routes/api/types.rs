use super::*;

/// One day's routes as published: the active reading of that day at the station.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RoutePublication {
    pub id: String,
    pub day: String,
    /// `final` for a completed day, `snapshot` for a day read as it stood.
    pub mode: String,
    pub station: String,
    pub collected_at: String,
    pub route_count: i64,
    pub itinerary_count: i64,
    pub stop_count: i64,
    pub task_count: i64,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteDays {
    pub station: String,
    /// The most recent completed day, which a collection targets by default.
    pub latest_day: String,
    pub days: Vec<RoutePublication>,
}
/// One driver's day on the road, from the itinerary and its computed summary. Times are
/// epoch milliseconds.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteItinerary {
    pub itinerary_id: String,
    pub transporter_id: String,
    pub driver_name: String,
    pub route_code: Option<String>,
    pub execution_status: Option<String>,
    pub progress_status: Option<String>,
    pub departed_at: Option<i64>,
    pub first_stop_at: Option<i64>,
    pub last_stop_at: Option<i64>,
    pub session_end_at: Option<i64>,
    pub stops_total: i64,
    pub stops_completed: i64,
    pub tasks_total: i64,
    pub delivered: i64,
    pub picked_up: i64,
    pub not_delivered: i64,
    pub breaks_secs: Option<i64>,
    pub overtime_secs: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteDayView {
    pub publication: RoutePublication,
    pub itineraries: Vec<RouteItinerary>,
}
/// A location a stop or task was at.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteAddress {
    pub address_id: String,
    pub address1: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub postal_code: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}
/// One package action: a pickup at the station, a drop-off, or a return.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteTask {
    pub task_id: String,
    pub tracking_id: Option<String>,
    pub order_id: Option<String>,
    pub task_type: Option<String>,
    pub task_state: Option<String>,
    pub state_context: Option<String>,
    pub execution_status: Option<String>,
    pub executed_at: Option<i64>,
    pub window_start_at: Option<i64>,
    pub window_end_at: Option<i64>,
    /// Where the driver's device was when the task was executed.
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub address_id: Option<String>,
    pub address_type: Option<String>,
    /// False for a task Amazon removed from the route.
    pub active: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteStop {
    pub stop_id: String,
    pub sequence: Option<i64>,
    pub stop_type: Option<String>,
    pub planned_start_at: Option<i64>,
    pub planned_end_at: Option<i64>,
    pub address: Option<RouteAddress>,
    pub tasks: Vec<RouteTask>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteBreak {
    pub planned: bool,
    pub break_id: Option<String>,
    pub kind: Option<String>,
    pub state: Option<String>,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub planned_start_at: Option<i64>,
    pub planned_end_at: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteUnknownStop {
    pub entered_at: Option<i64>,
    pub exited_at: Option<i64>,
    pub enter_latitude: Option<f64>,
    pub enter_longitude: Option<f64>,
    pub exit_latitude: Option<f64>,
    pub exit_longitude: Option<f64>,
}
/// One driver's itinerary in full: its stops with their tasks, its breaks, the places it
/// dwelt that were not stops, and the tasks removed from it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteItineraryDetail {
    pub publication: RoutePublication,
    pub itinerary: RouteItinerary,
    pub stops: Vec<RouteStop>,
    pub breaks: Vec<RouteBreak>,
    pub unknown_stops: Vec<RouteUnknownStop>,
    pub inactive_tasks: Vec<RouteTask>,
}
/// One thing that happened to a package: on which day, by whom, where.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RoutePackageEvent {
    pub day: String,
    pub transporter_id: String,
    pub driver_name: Option<String>,
    pub route_code: Option<String>,
    pub itinerary_id: String,
    pub task: RouteTask,
    pub address: Option<RouteAddress>,
}
/// A package's history across the days stored, newest first.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RoutePackage {
    pub tracking_id: String,
    pub events: Vec<RoutePackageEvent>,
}
/// How long a DSP keeps its route data, and what it holds now.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteRetention {
    /// Days kept, counted back from today where the DSP is. `None` keeps every day.
    #[cfg_attr(test, ts(type = "number | null"))]
    pub days: Option<i64>,
    pub changed_at: Option<String>,
    #[cfg_attr(test, ts(type = "number"))]
    pub stored_days: i64,
    pub oldest_day: Option<String>,
}
/// What a reprocess rebuilt: each publication with its counts as they now stand.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteReprocess {
    pub publications: Vec<RoutePublication>,
}
