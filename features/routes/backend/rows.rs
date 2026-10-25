//! An itinerary as the tables hold it: its summary and detail flattened into the itinerary's
//! row, and its stops, tasks, breaks and unknown stops into theirs.
use super::*;

/// A driver's itinerary flattened for its row.
pub(super) struct Flat {
    pub(super) itinerary: Vec<(&'static str, Value)>,
}

/// The itinerary row's columns from the list's summary and the detail.
pub(super) fn flatten(
    summary: &Value,
    detail: &Value,
    transporter: &str,
    driver_name: &str,
) -> Flat {
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
            ("rescue_actions", json_list(either("rescueActions"))),
            ("sequence_edits", json_list(either("sequenceEdits"))),
            ("pause_events", json_list(either("routePauseEvents"))),
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
pub(super) struct StopRow {
    pub(super) id: String,
    pub(super) sequence: Option<i64>,
    pub(super) address_id: Option<String>,
    pub(super) stop_type: Option<String>,
    pub(super) route_code: Option<String>,
    pub(super) planned_start_at: Option<i64>,
    pub(super) planned_end_at: Option<i64>,
    pub(super) expected_start_at: Option<i64>,
    pub(super) actual_start_at: Option<i64>,
    pub(super) flags: Value,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct TaskRow {
    pub(super) id: String,
    pub(super) stop_id: String,
    pub(super) tracking_id: Option<String>,
    pub(super) order_id: Option<String>,
    pub(super) task_type: Option<String>,
    pub(super) task_state: Option<String>,
    pub(super) state_context: Option<String>,
    pub(super) execution_status: Option<String>,
    pub(super) address_id: Option<String>,
    pub(super) window_start_at: Option<i64>,
    pub(super) window_end_at: Option<i64>,
    pub(super) executed_at: Option<i64>,
    pub(super) latitude: Option<f64>,
    pub(super) longitude: Option<f64>,
    pub(super) box_type: Option<String>,
    pub(super) weight: Option<f64>,
    pub(super) weight_unit: Option<String>,
    pub(super) length: Option<f64>,
    pub(super) width: Option<f64>,
    pub(super) height: Option<f64>,
    pub(super) volume_unit: Option<String>,
    pub(super) time_windowed: Option<i64>,
    pub(super) high_value: Option<i64>,
    pub(super) customer_return: Option<i64>,
    pub(super) address_type: Option<String>,
    pub(super) events: Value,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct DayRow {
    pub(super) route_code: Option<String>,
    pub(super) departed_at: Option<i64>,
    pub(super) session_end_at: Option<i64>,
    pub(super) breaks_secs: Option<i64>,
    pub(super) overtime_secs: Option<i64>,
    pub(super) first_stop_at: Option<i64>,
    pub(super) last_stop_at: Option<i64>,
    pub(super) stops_total: i64,
    pub(super) stops_completed: i64,
    pub(super) tasks_total: i64,
    pub(super) delivered: i64,
    pub(super) picked_up: i64,
    pub(super) not_delivered: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct BreakRow {
    pub(super) planned: bool,
    pub(super) break_id: Option<String>,
    pub(super) kind: Option<String>,
    pub(super) state: Option<String>,
    pub(super) sequence: Option<i64>,
    pub(super) punch_id: Option<String>,
    pub(super) started_at: Option<i64>,
    pub(super) ended_at: Option<i64>,
    pub(super) planned_start_at: Option<i64>,
    pub(super) planned_end_at: Option<i64>,
    pub(super) min_duration_ms: Option<i64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct UnknownStopRow {
    pub(super) entered_at: Option<i64>,
    pub(super) exited_at: Option<i64>,
    pub(super) enter_latitude: Option<f64>,
    pub(super) enter_longitude: Option<f64>,
    pub(super) exit_latitude: Option<f64>,
    pub(super) exit_longitude: Option<f64>,
}
/// A stop as shaping reads it, as a JSON tree would answer it: read straight from the response
/// (`wire`), without a tree of its tasks.
pub(super) trait StopFields {
    type Task: TaskFields;
    /// One of its fields as a tree answers it: null when it has none.
    fn field(&self, key: &str) -> &Value;
    fn tasks(&self) -> impl Iterator<Item = &Self::Task>;
    /// The names of its flags that are set, in order.
    fn flags(&self) -> Value;
}
/// A task as shaping reads it, as a JSON tree would answer it.
pub(super) trait TaskFields {
    /// One of its fields as a tree answers it: null when it has none.
    fn field(&self, key: &str) -> &Value;
    /// A field of an object it holds, as a tree answers it.
    fn inner(&self, object: &str, key: &str) -> &Value;
    /// Its recent events as [time, state, context] triples: every event Amazon sends
    /// carries just these three, and their names repeated in every row would be a third of
    /// the table.
    fn events(&self) -> Value;
}
/// A task's row, at `stop_id` or removed from the route. A task without an identifier
/// cannot be a row.
pub(super) fn task_row(task: &impl TaskFields, stop_id: &str) -> Option<TaskRow> {
    let task_id = text(task.field("taskId")).filter(|v| identifier(v, 256))?;
    let dimension = |key: &str| number(task.field(key));
    Some(TaskRow {
        id: task_id,
        stop_id: stop_id.to_owned(),
        tracking_id: text(task.inner("domainMap", "scannableId"))
            .or_else(|| text(task.field("scannableId"))),
        order_id: text(task.inner("domainMap", "orderId")),
        task_type: text(task.field("taskType")),
        task_state: text(task.field("taskState")),
        state_context: text(task.field("taskStateContext")),
        execution_status: text(task.field("executionStatus")),
        address_id: text(task.field("addressId")),
        window_start_at: stamp(task.field("windowStartTime")),
        window_end_at: stamp(task.field("windowEndTime")),
        executed_at: stamp(task.field("actualExecutionTime"))
            .or_else(|| stamp(task.field("taskExecutionTime"))),
        latitude: number(task.inner("executionGeocode", "latitude")),
        longitude: number(task.inner("executionGeocode", "longitude")),
        box_type: text(task.field("boxType")),
        weight: dimension("weight"),
        weight_unit: text(task.field("weightUnit")),
        length: dimension("length"),
        width: dimension("width"),
        height: dimension("height"),
        volume_unit: text(task.field("volumeUnit")),
        time_windowed: flag(task.field("timeWindowed")),
        high_value: flag(task.field("highValueTaskFlag")),
        customer_return: flag(task.field("customerReturn")),
        address_type: text(task.field("addressTypeInfo")),
        events: task.events(),
    })
}
/// The rows of `stops`, and the day summary they add up to, with the stops completed as
/// `progress` counts them when it does. A task or stop without an identifier, or one
/// repeated, is skipped: it cannot be a row.
pub(super) fn stop_rows<'a, S: StopFields + 'a>(
    stops: impl Iterator<Item = &'a S>,
    progress: &Value,
    transporter: &str,
) -> (Vec<StopRow>, Vec<TaskRow>, DayRow) {
    let mut rows = Vec::new();
    let mut tasks = Vec::new();
    let mut day = DayRow::default();
    let mut seen_stops = std::collections::HashSet::new();
    let mut seen_tasks = std::collections::HashSet::new();
    for (index, stop) in stops.enumerate() {
        // The API sends no stop id; the page adds one. The sequence number is the
        // stop's identity within its itinerary, and its position stands in without one.
        let stop_id = text(stop.field("stopId"))
            .filter(|v| identifier(v, 256))
            .or_else(|| integer(stop.field("sequenceNumber")).map(|n| format!("seq-{n}")))
            .unwrap_or_else(|| format!("index-{index}"));
        if !seen_stops.insert(stop_id.clone()) {
            continue;
        }
        let mut completed = false;
        for task in stop.tasks() {
            // A task another driver executed belongs to that driver's itinerary.
            if task
                .field("transporterId")
                .as_str()
                .is_some_and(|t| t != transporter)
            {
                continue;
            }
            let Some(row) = task_row(task, &stop_id) else {
                continue;
            };
            if !seen_tasks.insert(row.id.clone()) {
                continue;
            }
            day.tasks_total += 1;
            match (row.task_type.as_deref(), row.task_state.as_deref()) {
                (Some("DROP_OFF"), Some("DELIVERED")) => {
                    day.delivered += 1;
                    completed = true;
                    if let Some(at) = row.executed_at {
                        day.first_stop_at = Some(day.first_stop_at.map_or(at, |v| v.min(at)));
                        day.last_stop_at = Some(day.last_stop_at.map_or(at, |v| v.max(at)));
                    }
                }
                (Some("DROP_OFF"), _) => day.not_delivered += 1,
                (Some("PICK_UP"), Some("PICKED_UP")) => day.picked_up += 1,
                _ => (),
            }
            tasks.push(row);
        }
        day.stops_total += 1;
        if completed {
            day.stops_completed += 1;
        }
        rows.push(StopRow {
            id: stop_id,
            sequence: integer(stop.field("sequenceNumber")),
            address_id: text(stop.field("addressId")),
            stop_type: text(stop.field("itineraryStopType")),
            route_code: text(stop.field("routeCode")),
            planned_start_at: stamp(stop.field("plannedStartTime")),
            planned_end_at: stamp(stop.field("plannedEndTime")),
            expected_start_at: stamp(stop.field("expectedStartTime")),
            actual_start_at: stamp(stop.field("actualStartTime")),
            flags: stop.flags(),
        });
    }
    if let Some(completed) = integer(&progress["completed"]) {
        day.stops_completed = completed;
    }
    (rows, tasks, day)
}
/// The rows of tasks removed from the route, each once.
pub(super) fn inactive_rows<'a, T: TaskFields + 'a>(
    tasks: impl Iterator<Item = &'a T>,
) -> Vec<TaskRow> {
    let mut seen = std::collections::HashSet::new();
    tasks
        .filter_map(|task| task_row(task, ""))
        .filter(|row| seen.insert(row.id.clone()))
        .collect()
}
/// Break punches, from the detail or else the list's summary, then planned breaks.
pub(super) fn break_rows(summary: &Value, detail: &Value) -> Vec<BreakRow> {
    let taken = match detail["breaks"].as_array() {
        Some(list) if !list.is_empty() => Some(list),
        _ => summary["breaks"].as_array(),
    };
    let planned = match detail["plannedBreaks"].as_array() {
        Some(list) if !list.is_empty() => Some(list),
        _ => summary["plannedBreaks"].as_array(),
    };
    let mut rows: Vec<BreakRow> = taken
        .into_iter()
        .flatten()
        .map(|b| BreakRow {
            planned: false,
            break_id: text(&b["breakId"]),
            kind: text(&b["type"]),
            state: text(&b["state"]),
            sequence: integer(&b["sequenceNumber"]),
            punch_id: text(&b["punchId"]),
            started_at: stamp(&b["timeStampOn"]),
            ended_at: stamp(&b["timeStampOff"]),
            planned_start_at: None,
            planned_end_at: None,
            min_duration_ms: None,
        })
        .collect();
    rows.extend(planned.into_iter().flatten().map(|b| BreakRow {
        planned: true,
        break_id: text(&b["breakId"]),
        kind: text(&b["type"]).or_else(|| text(&b["breakType"])),
        state: None,
        sequence: integer(&b["plannedSequenceNumber"]),
        punch_id: None,
        started_at: None,
        ended_at: None,
        planned_start_at: stamp(&b["plannedStart"]),
        planned_end_at: stamp(&b["plannedEnd"]),
        min_duration_ms: integer(&b["minDurationInMillis"]),
    }));
    rows
}
pub(super) fn unknown_stop_rows(detail: &Value) -> Vec<UnknownStopRow> {
    detail["unknownStops"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|u| UnknownStopRow {
            entered_at: stamp(&u["enterTime"]),
            exited_at: stamp(&u["exitTime"]),
            enter_latitude: number(&u["enterLatitude"]),
            enter_longitude: number(&u["enterLongitude"]),
            exit_latitude: number(&u["exitLatitude"]),
            exit_longitude: number(&u["exitLongitude"]),
        })
        .collect()
}
