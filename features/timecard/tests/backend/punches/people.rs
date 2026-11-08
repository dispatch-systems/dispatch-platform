use super::*;
use crate::backend::{TimecardStore, punches::people_summary};
use dispatch_core::testing;
use serde_json::{Value, json};

fn install() {
    testing::install(
        &[&dispatch_paycom::COLLECTOR, &dispatch_cortex::COLLECTOR],
        &[&crate::FEATURE],
    );
}
fn captured() -> Value {
    json!({
        "from":"2026-09-01","to":"2026-09-03","collectedAt":"2026-09-02T12:00:00Z",
        "employees":[
            {"code":"E1","name":" First name ","department":"Delivery","position":"Driver","station":"","active":true},
            {"code":"E2","name":"No hours","department":"","position":"","station":"","active":true}
        ],
        "timecards":[
            {"employeeCode":"E1","date":"2026-09-01","hours":4.0,"status":"","punches":[]},
            {"employeeCode":"E1","date":"2026-09-02","hours":0.0,"status":"","punches":[]},
            {"employeeCode":"E2","date":"2026-09-03","hours":0.0,"status":"","punches":[]}
        ]
    })
}

/// Compact reads must not reach publication history, regardless of how much it has grown.
struct WithoutHistory<'a>(&'a rusqlite::Connection);
impl<'a> WithoutHistory<'a> {
    fn new(connection: &'a rusqlite::Connection) -> Self {
        unsafe extern "C" fn authorize(
            _: *mut std::ffi::c_void,
            action: std::ffi::c_int,
            table: *const std::ffi::c_char,
            _: *const std::ffi::c_char,
            _: *const std::ffi::c_char,
            _: *const std::ffi::c_char,
        ) -> std::ffi::c_int {
            if action == rusqlite::ffi::SQLITE_READ && !table.is_null() {
                // SAFETY: SQLite supplies a NUL-terminated table name for SQLITE_READ.
                let table = unsafe { std::ffi::CStr::from_ptr(table) }.to_bytes();
                if [b"employees".as_slice(), b"timecards", b"publications"].contains(&table) {
                    return rusqlite::ffi::SQLITE_DENY;
                }
            }
            rusqlite::ffi::SQLITE_OK
        }
        // SAFETY: the callback has no context or captured state and the connection stays on
        // this thread. Drop removes it before releasing this borrow.
        let status = unsafe {
            rusqlite::ffi::sqlite3_set_authorizer(
                connection.handle(),
                Some(authorize),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(status, rusqlite::ffi::SQLITE_OK);
        Self(connection)
    }
}
impl Drop for WithoutHistory<'_> {
    fn drop(&mut self) {
        // SAFETY: the borrowed connection is still live and used only on this thread.
        unsafe {
            rusqlite::ffi::sqlite3_set_authorizer(self.0.handle(), None, std::ptr::null_mut());
        }
    }
}

#[test]
fn summaries_keep_historical_names_workdays_and_unchanged_collection_advancement_without_reading_history()
 {
    install();
    let (_root, store, dsp) = testing::bootstrapped();
    let old = captured();
    let mut current = old.clone();
    current["from"] = json!("2026-09-04");
    current["to"] = json!("2026-09-06");
    current["collectedAt"] = json!("2026-09-05T12:00:00Z");
    current["employees"] = json!([{
        "code":"E1","name":" Current name ","department":"Office","position":"Coordinator","station":"","active":true
    }]);
    current["timecards"] = json!([
        {"employeeCode":"E1","date":"2026-09-04","hours":8.0,"status":"","punches":[]},
        {"employeeCode":"E1","date":"2026-09-05","hours":0.0,"status":"","punches":[]},
        {"employeeCode":"E1","date":"2026-09-06","hours":0.0,"status":"","punches":[]}
    ]);
    let mut revision = old.clone();
    revision["collectedAt"] = json!("2026-09-06T12:00:00Z");
    revision["employees"][0]["name"] = json!("Corrected old period");
    for value in [&old, &current, &revision] {
        store.publish_timecards(&dsp, value).unwrap();
    }
    // Directory names follow actual collection order, while listed still follows the current roster.
    let people = employees(&store, &dsp).unwrap();
    assert_eq!(people[0].name, "Corrected old period");
    assert_eq!(people[0].department.as_deref(), Some("Delivery"));
    assert!(people[0].listed);
    assert!(!people[1].listed);

    // No payload changed; advancing the current collection changes its naming precedence and
    // makes its final zero-hour dates collected, without creating another publication.
    current["collectedAt"] = json!("2026-09-07T12:00:00Z");
    store.publish_timecards(&dsp, &current).unwrap();
    let db = store.collector(&dsp, paycom::PROVIDER).unwrap();
    assert_eq!(
        db.count("SELECT count(*) FROM publications", []).unwrap(),
        3
    );
    let _history = WithoutHistory::new(&db.0);
    let people = employees(&store, &dsp).unwrap();
    let first = &people[0];
    assert_eq!(first.name, " Current name ");
    assert_eq!(first.department.as_deref(), Some("Office"));
    assert_eq!(first.position.as_deref(), Some("Coordinator"));
    assert_eq!(first.first_seen.as_deref(), Some("2026-09-01"));
    assert_eq!(first.last_seen.as_deref(), Some("2026-09-04"));
    assert!(first.listed);
    assert_eq!(people[1].first_seen.as_deref(), Some("2026-09-02"));
    assert_eq!(people[1].last_seen.as_deref(), Some("2026-09-06"));
    assert!(!people[1].listed);
    let mut days = workdays(&store, &dsp).unwrap();
    days.worked.sort();
    days.collected.sort();
    assert_eq!(
        days.worked,
        [
            ("E1".into(), "2026-09-01".into()),
            ("E1".into(), "2026-09-04".into())
        ]
    );
    assert_eq!(
        days.collected,
        [
            "2026-09-01",
            "2026-09-02",
            "2026-09-03",
            "2026-09-04",
            "2026-09-05",
            "2026-09-06"
        ]
    );
    let records = (PEOPLE.records.unwrap().1)(&store, &dsp).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(
        (&*records[0].id, records[0].count, &*records[0].last),
        ("E1", 2, "2026-09-04")
    );
}

#[test]
fn legacy_updates_and_deletions_use_history_until_summaries_are_rebuilt() {
    install();
    let (_root, store, dsp) = testing::bootstrapped();
    store.publish_timecards(&dsp, &captured()).unwrap();
    let db = store.collector(&dsp, paycom::PROVIDER).unwrap();
    db.exec(
        "UPDATE employees SET name='Rollback name' WHERE code='E1'",
        [],
    )
    .unwrap();
    db.exec("UPDATE timecards SET hours=3 WHERE employee_code='E2'", [])
        .unwrap();
    assert!(!people_summary::valid(&db).unwrap());
    let people = employees(&store, &dsp).unwrap();
    assert_eq!(people[0].name, "Rollback name");
    assert_eq!(people[1].first_seen.as_deref(), Some("2026-09-03"));
    assert_eq!(people[1].last_seen.as_deref(), Some("2026-09-03"));
    db.transaction(|| people_summary::prepare(&db)).unwrap();
    assert!(people_summary::valid(&db).unwrap());
    let people = employees(&store, &dsp).unwrap();
    assert_eq!(people[0].name, "Rollback name");
    assert_eq!(people[1].first_seen.as_deref(), Some("2026-09-03"));
    db.exec("DELETE FROM publications", []).unwrap();
    assert!(!people_summary::valid(&db).unwrap());
    assert!(employees(&store, &dsp).unwrap().is_empty());
    assert!(workdays(&store, &dsp).unwrap().worked.is_empty());
    db.transaction(|| people_summary::prepare(&db)).unwrap();
    assert!(people_summary::valid(&db).unwrap());
    assert!(employees(&store, &dsp).unwrap().is_empty());
}
