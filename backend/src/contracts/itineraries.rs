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
