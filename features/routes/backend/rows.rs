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
/// A task's row, at `stop_id` or removed from the route. A task without an identifier
/// cannot be a row.
pub(super) fn task_row(task: &Value, stop_id: &str) -> Option<TaskRow> {
    let task_id = text(&task["taskId"]).filter(|v| identifier(v, 256))?;
    let dimension = |key: &str| number(&task[key]);
    Some(TaskRow {
        id: task_id,
        stop_id: stop_id.to_owned(),
        tracking_id: text(&task["domainMap"]["scannableId"]).or_else(|| text(&task["scannableId"])),
        order_id: text(&task["domainMap"]["orderId"]),
        task_type: text(&task["taskType"]),
        task_state: text(&task["taskState"]),
        state_context: text(&task["taskStateContext"]),
        execution_status: text(&task["executionStatus"]),
        address_id: text(&task["addressId"]),
        window_start_at: stamp(&task["windowStartTime"]),
        window_end_at: stamp(&task["windowEndTime"]),
        executed_at: stamp(&task["actualExecutionTime"])
            .or_else(|| stamp(&task["taskExecutionTime"])),
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
        events: compact_events(&task["recentTaskEvents"]),
    })
}
/// A task's recent events as [time, state, context] triples: every event Amazon sends
/// carries just these three, and their names repeated in every row would be a third of
/// the table.
pub(super) fn compact_events(events: &Value) -> Value {
    Value::Array(
        events
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| json!([e["executionTime"], e["taskState"], e["taskStateContext"]]))
            .collect(),
    )
}
/// The stops and tasks of a detail, and the day summary they add up to. A task or
/// stop without an identifier, or one repeated, is skipped: it cannot be a row.
pub(super) fn rows(detail: &Value, transporter: &str) -> (Vec<StopRow>, Vec<TaskRow>, DayRow) {
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
            // A task another driver executed belongs to that driver's itinerary.
            if task["transporterId"]
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
/// Tasks removed from the route, each once.
pub(super) fn inactive_tasks(detail: &Value) -> Vec<TaskRow> {
    let mut seen = std::collections::HashSet::new();
    detail["inactiveTasks"]
        .as_array()
        .into_iter()
        .flatten()
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
