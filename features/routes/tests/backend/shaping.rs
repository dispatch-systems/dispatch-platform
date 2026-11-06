use super::*;
use crate::backend::{
    rows::{StopFields, TaskFields},
    tests::request,
};
use dispatch_cortex::routes::{Mode, fixture};

/// A stop as a JSON tree holds it, read as shaping read every itinerary before `wire`: what
/// the typed reading answers as.
impl StopFields for Value {
    type Task = Value;
    fn field(&self, key: &str) -> &Value {
        &self[key]
    }
    fn tasks(&self) -> impl Iterator<Item = &Value> {
        self["tasks"].as_array().into_iter().flatten()
    }
    fn flags(&self) -> Value {
        let mut names: Vec<&str> = self
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(k, v)| (k.ends_with("Flag") || k.starts_with("is")) && **v == json!(true))
            .map(|(k, _)| k.as_str())
            .collect();
        names.sort_unstable();
        json!(names)
    }
}
/// A task as a JSON tree holds it.
impl TaskFields for Value {
    fn field(&self, key: &str) -> &Value {
        &self[key]
    }
    fn inner(&self, object: &str, key: &str) -> &Value {
        &self[object][key]
    }
    fn events(&self) -> Value {
        Value::Array(
            self["recentTaskEvents"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| json!([e["executionTime"], e["taskState"], e["taskStateContext"]]))
                .collect(),
        )
    }
}
#[test]
fn an_itinerary_read_straight_from_its_response_shapes_as_from_its_tree() {
    let capture = fixture(&request(Mode::Snapshot)).unwrap();
    let shaper = Shaper::new(&capture);
    // Fields missing, repeated, of another kind than expected, or not objects at all, as text:
    // a tree built here would keep one of each repeated field before either reading saw it.
    let odd = r#"{"itineraryDetails":{"itineraryId":"it-9","transporterId":"driver-1","routeCode":"CX9",
        "stopProgress":{"completed":"1"},"plannedBreaks":[{"breakId":"b1","plannedStart":1758800000}],
        "stops":[null, 5, {"stopId":"s1","sequenceNumber":"3","dispatchStopFlag":true,"isLocker":true,
            "isOpen":"yes","aFlag":true,"aFlag":false,"bFlag":"true","routeCode":["CX9"],
            "tasks":[{"taskId":"t1","transporterId":"driver-1","domainMap":"none","weight":"1.5","length":2,
                "taskType":"DROP_OFF","taskState":"DELIVERED","actualExecutionTime":1758800000.5,
                "recentTaskEvents":[{"executionTime":{"at":1},"taskState":"X"},7]},
                {"taskId":"t1","taskType":"PICK_UP"},{"taskId":["t5"]},{"taskId":"t2","transporterId":"driver-2"},
                {"taskId":"t3","taskId":"t4","executionGeocode":{"latitude":"47.1","longitude":[1]},
                "highValueTaskFlag":true,"timeWindowed":"true","recentTaskEvents":{"at":1}}]},
            {"sequenceNumber":4,"tasks":"none"}],
        "inactiveTasks":[{"taskId":"t9"},{"taskId":"t9","taskState":"X"},"t8",{"taskId":"t4"}]},
        "addresses":[{"addressId":"a1","geocode":{"latitude":1}}],"transporters":"none"}"#;
    let odd = ItineraryCapture {
        id: "it-9".into(),
        transporter_id: "driver-1".into(),
        detail: odd.into(),
    };
    for captured in capture.itineraries.iter().chain([&odd]) {
        let typed = shaper.shape(captured).unwrap();
        let tree: Value = serde_json::from_str(&captured.detail).unwrap();
        let detail = &tree["itineraryDetails"];
        let (stops, tasks, day) = stop_rows(
            detail["stops"].as_array().into_iter().flatten(),
            &detail["stopProgress"],
            &captured.transporter_id,
        );
        let read = Read {
            detail,
            transporters: &tree["transporters"],
            addresses: &tree["addresses"],
            stops,
            tasks,
            day,
            inactive: inactive_rows(detail["inactiveTasks"].as_array().into_iter().flatten()),
        };
        let from_tree = shaper
            .shape_read(
                &captured.id,
                &captured.transporter_id,
                read,
                captured.detail.len(),
                typed.raw.clone(),
            )
            .unwrap();
        assert!(!from_tree.tasks.is_empty() && !from_tree.stops.is_empty());
        assert_eq!(
            serde_json::to_string(&typed).unwrap(),
            serde_json::to_string(&from_tree).unwrap()
        );
    }
    // Of a field repeated, the last is read.
    let repeated = shaper.shape(&odd).unwrap();
    let stop = repeated.stops.iter().find(|stop| stop.id == "s1").unwrap();
    assert_eq!(stop.flags, json!(["dispatchStopFlag", "isLocker"]));
    assert!(repeated.tasks.iter().any(|task| task.id == "t4"));
    // A detail that isn't an object is no itinerary.
    let refused = shaper.shape(&ItineraryCapture {
        detail: json!({"itineraryDetails":[]}).to_string(),
        ..odd
    });
    assert_eq!(refused.unwrap_err().code, "routes_capture_invalid");
}
