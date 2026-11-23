//! The weeks it keeps, against a real store in a temporary directory.
use super::*;
use crate::backend::support::{station, week};
use dispatch_core::testing;

#[test]
fn each_run_adds_to_the_week_logging_every_assignment_added_changed_or_removed() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    station(&store, &dsp);
    let first = week(&store, &dsp, 1_790_830_800_000);
    store.publish_shifts(&dsp, "job-1", &first).unwrap();
    // The next read: one shift moved an hour later, one gone, and one new.
    let mut next = week(&store, &dsp, 1_790_917_200_000);
    let moved = next.assignments[0].mapping_id.clone();
    next.assignments[0].start_min += 60;
    next.assignments[0].start_max += 60;
    let gone = next.assignments.remove(1).mapping_id;
    let mut new = next.assignments[1].clone();
    new.mapping_id = "mapping-new".into();
    new.kind = "BACKUP".into();
    next.assignments.push(new);
    // A driver's name follows the roster, and changes no shift.
    next.assignments[2].driver_name = Some("Renamed Example".into());
    store.publish_shifts(&dsp, "job-2", &next).unwrap();
    // A read older than the week's newest arrives last, and changes nothing; nor does a job
    // recovered after its publication committed, collecting again.
    let late = week(&store, &dsp, 1_790_900_000_000);
    store.publish_shifts(&dsp, "job-late", &late).unwrap();
    let again = week(&store, &dsp, 1_791_000_000_000);
    store.publish_shifts(&dsp, "job-2", &again).unwrap();
    // A read listing nothing of a week that listed shifts is refused, not taken as every
    // shift removed.
    let mut emptied = week(&store, &dsp, 1_791_100_000_000);
    emptied.assignments.clear();
    assert_eq!(
        store
            .publish_shifts(&dsp, "job-3", &emptied)
            .unwrap_err()
            .code,
        "shifts_week_emptied"
    );

    let db = store.shifts_db(&dsp).unwrap();
    let changes: Vec<(String, String, String)> = db
        .query_as(
            "SELECT job_id,mapping_id,change FROM shift_changes WHERE job_id<>'job-1' ORDER BY id",
            [],
        )
        .unwrap();
    assert_eq!(
        changes,
        [
            ("job-2".to_owned(), moved.clone(), "changed".to_owned()),
            (
                "job-2".to_owned(),
                "mapping-new".to_owned(),
                "added".to_owned()
            ),
            ("job-2".to_owned(), gone.clone(), "removed".to_owned()),
        ]
    );
    let count = |sql: &str| db.count(sql, []).unwrap();
    assert_eq!(
        (
            count("SELECT count(*) FROM shift_changes WHERE job_id='job-1' AND change='added'"),
            count("SELECT count(*) FROM shift_runs"),
            count("SELECT count(*) FROM shift_assignments WHERE listed=1"),
        ),
        (
            first.assignments.len() as i64,
            2,
            first.assignments.len() as i64
        )
    );
    // What it was and what it became, and the one removed keeps its row.
    let (before, after): (String, String) = db
        .one_as(
            "SELECT before,after FROM shift_changes WHERE change='changed'",
            [],
        )
        .unwrap()
        .unwrap();
    let start = |fields: &str| serde_json::from_str::<Value>(fields).unwrap()["startMin"].clone();
    assert_eq!(
        (start(&before), start(&after)),
        (
            json!(first.assignments[0].start_min),
            json!(first.assignments[0].start_min + 60)
        )
    );
    assert_eq!(
        count(&format!(
            "SELECT count(*) FROM shift_assignments WHERE mapping_id='{gone}' AND listed=0"
        )),
        1
    );
}
