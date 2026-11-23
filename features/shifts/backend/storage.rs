//! Schedules' storage: every shift assignment Amazon's Scheduling has listed, and every change
//! a collection found to one, in a database of its own beside Cortex's. Only this crate writes
//! SQL against it.
use dispatch_core::{
    Result,
    collection::registry::AddedStorage,
    db::{Db, DspLease, FromRow, Kind, Row, Store, at},
    ensure,
};
use dispatch_cortex::{
    self as cortex,
    shifts::{Assignment, Capture, Collection, Request},
};
use rusqlite::params;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Its database beside Cortex's: `shifts/shifts.sqlite` in each DSP's data.
pub(crate) const DATABASE: Kind = Kind::new("shifts", 1);
pub(crate) static STORAGE: AddedStorage = AddedStorage {
    id: "shifts",
    kind: DATABASE,
    marker: "storage.shifts",
    source: "shifts-v1",
    verify,
};
/// Fails closed when the initialized storage lost a table.
fn verify(db: &Db) -> Result<()> {
    for table in ["shift_runs", "shift_assignments", "shift_changes"] {
        db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
    }
    Ok(())
}
/// What a week's job carries while queued; the station and its day are bound just before it
/// runs.
pub(crate) fn week_request() -> Value {
    json!({"collection": "shifts"})
}
/// The request a week's job runs with: the DSP's station, names and time zone as its profile
/// has them now, and the day it is there, whose week it reads.
pub(crate) fn bind_week_request(store: &Store, dsp: &str) -> Result<Request> {
    let profile = store.profile(dsp)?;
    let found = store.find_dsp(dsp)?;
    ensure(
        !profile.station_code.is_empty(),
        "shifts_station_required",
        409,
    )?;
    ensure(
        !profile.abbreviation.is_empty(),
        "shifts_abbreviation_required",
        409,
    )?;
    let zone = dispatch_core::foundation::clock::zone(&found.timezone)?;
    let request = Request {
        collection: Collection::Shifts,
        date: dispatch_core::foundation::clock::today(&zone).to_string(),
        station: profile.station_code,
        timezone: found.timezone,
        dsp_name: found.name,
        dsp_abbreviation: profile.abbreviation,
    };
    request.validate()?;
    Ok(request)
}

/// What a change compares of an assignment, and what the log keeps of it before and after.
/// The driver's name and the DSP's ID for them follow the roster, and change nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Fields {
    date: String,
    transporter_id: String,
    kind: String,
    shift_id: String,
    shift_name: String,
    shift_type: String,
    position: Option<String>,
    start_min: i64,
    start_max: i64,
    duration_min: i64,
    duration_max: i64,
    qualification: Option<String>,
    reason: Option<String>,
    status: String,
    state: Option<String>,
    comment: Option<String>,
}
impl Fields {
    fn of(assignment: &Assignment) -> Self {
        Self {
            date: assignment.date.clone(),
            transporter_id: assignment.transporter_id.clone(),
            kind: assignment.kind.clone(),
            shift_id: assignment.shift_id.clone(),
            shift_name: assignment.shift_name.clone(),
            shift_type: assignment.shift_type.clone(),
            position: assignment.position.clone(),
            start_min: assignment.start_min,
            start_max: assignment.start_max,
            duration_min: assignment.duration_min,
            duration_max: assignment.duration_max,
            qualification: assignment.qualification.clone(),
            reason: assignment.reason.clone(),
            status: assignment.status.clone(),
            state: assignment.state.clone(),
            comment: assignment.comment.clone(),
        }
    }
}
/// An assignment as kept: its id, whether its week's newest collection listed it, and what a
/// change compares.
struct Kept {
    mapping_id: String,
    listed: bool,
    fields: Fields,
}
impl FromRow for Kept {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            mapping_id: row.get("mapping_id")?,
            listed: row.get("listed")?,
            fields: Fields {
                date: row.get("date")?,
                transporter_id: row.get("transporter_id")?,
                kind: row.get("kind")?,
                shift_id: row.get("shift_id")?,
                shift_name: row.get("shift_name")?,
                shift_type: row.get("shift_type")?,
                position: row.get("position")?,
                start_min: row.get("start_min")?,
                start_max: row.get("start_max")?,
                duration_min: row.get("duration_min")?,
                duration_max: row.get("duration_max")?,
                qualification: row.get("qualification")?,
                reason: row.get("reason")?,
                status: row.get("status")?,
                state: row.get("state")?,
                comment: row.get("comment")?,
            },
        })
    }
}

/// What Schedules reads and writes for one DSP.
pub trait ShiftsStore {
    fn shifts_db(&self, dsp: &str) -> Result<DspLease<'_>>;
    /// Adds a collected week to what it keeps: each assignment as the week now lists it, and
    /// a change for each one added, changed or no longer listed since the week's last read. A
    /// read older than the week's newest kept one changes nothing.
    fn publish_shifts(&self, dsp: &str, job: &str, capture: &Capture) -> Result<()>;
}
impl ShiftsStore for Store {
    fn shifts_db(&self, dsp: &str) -> Result<DspLease<'_>> {
        self.added_storage(dsp, cortex::PROVIDER, &STORAGE)
    }
    fn publish_shifts(&self, dsp: &str, job: &str, capture: &Capture) -> Result<()> {
        // The week it read, at the station the DSP's profile names now: a job bound late on a
        // Saturday may publish on Sunday.
        let mut request = bind_week_request(self, dsp)?;
        request.date = capture.week_start.clone();
        capture.validate(&request)?;
        let collected_at = at(capture.finished_at);
        let db = self.shifts_db(dsp)?;
        db.transaction(|| {
            let newer = db.count(
                "SELECT count(*) FROM shift_runs WHERE service_area_id=? AND week_start=? \
                 AND collected_at>?",
                params![capture.service_area_id, capture.week_start, collected_at],
            )?;
            if newer > 0 {
                return Ok(());
            }
            let ids: Vec<&str> = capture
                .assignments
                .iter()
                .map(|a| a.mapping_id.as_str())
                .collect();
            let kept: BTreeMap<String, Kept> = db
                .query_as::<Kept>(
                    "SELECT * FROM shift_assignments WHERE mapping_id IN \
                     (SELECT value FROM json_each(?1)) OR (listed=1 AND service_area_id=?2 \
                     AND date BETWEEN ?3 AND ?4)",
                    params![
                        serde_json::to_string(&ids)?,
                        capture.service_area_id,
                        capture.week_start,
                        capture.week_end
                    ],
                )?
                .into_iter()
                .map(|kept| (kept.mapping_id.clone(), kept))
                .collect();
            let log = |mapping_id: &str,
                       fields: &Fields,
                       change: &str,
                       before: Option<&Fields>,
                       after: Option<&Fields>|
             -> Result<()> {
                db.exec(
                    "INSERT INTO shift_changes(job_id,mapping_id,date,transporter_id,change,\
                     before,after,seen_at) VALUES (?,?,?,?,?,?,?,?)",
                    params![
                        job,
                        mapping_id,
                        fields.date,
                        fields.transporter_id,
                        change,
                        before.map(serde_json::to_string).transpose()?,
                        after.map(serde_json::to_string).transpose()?,
                        collected_at,
                    ],
                )
                .map(|_| ())
            };
            let (mut added, mut changed) = (0, 0);
            for assignment in &capture.assignments {
                let fields = Fields::of(assignment);
                match kept.get(&assignment.mapping_id) {
                    Some(kept) if kept.listed && kept.fields == fields => (),
                    Some(kept) if kept.listed => {
                        changed += 1;
                        log(
                            &assignment.mapping_id,
                            &fields,
                            "changed",
                            Some(&kept.fields),
                            Some(&fields),
                        )?;
                    }
                    _ => {
                        added += 1;
                        log(
                            &assignment.mapping_id,
                            &fields,
                            "added",
                            None,
                            Some(&fields),
                        )?;
                    }
                }
                db.exec(
                    "INSERT INTO shift_assignments(mapping_id,service_area_id,date,transporter_id,\
                     driver_name,employee_id,kind,shift_id,shift_name,shift_type,position,\
                     start_min,start_max,duration_min,duration_max,qualification,reason,status,\
                     state,comment,created_at,listed,first_seen,last_seen,row) VALUES \
                     (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,\
                     1,?22,?22,?23) ON CONFLICT(mapping_id) DO UPDATE SET \
                     service_area_id=excluded.service_area_id,date=excluded.date,\
                     transporter_id=excluded.transporter_id,driver_name=excluded.driver_name,\
                     employee_id=excluded.employee_id,kind=excluded.kind,\
                     shift_id=excluded.shift_id,shift_name=excluded.shift_name,\
                     shift_type=excluded.shift_type,position=excluded.position,\
                     start_min=excluded.start_min,start_max=excluded.start_max,\
                     duration_min=excluded.duration_min,duration_max=excluded.duration_max,\
                     qualification=excluded.qualification,reason=excluded.reason,\
                     status=excluded.status,state=excluded.state,comment=excluded.comment,\
                     created_at=excluded.created_at,listed=1,last_seen=excluded.last_seen,\
                     row=excluded.row",
                    params![
                        assignment.mapping_id,
                        capture.service_area_id,
                        assignment.date,
                        assignment.transporter_id,
                        assignment.driver_name,
                        assignment.employee_id,
                        assignment.kind,
                        assignment.shift_id,
                        assignment.shift_name,
                        assignment.shift_type,
                        assignment.position,
                        assignment.start_min,
                        assignment.start_max,
                        assignment.duration_min,
                        assignment.duration_max,
                        assignment.qualification,
                        assignment.reason,
                        assignment.status,
                        assignment.state,
                        assignment.comment,
                        assignment.created_at,
                        collected_at,
                        assignment.row.to_string(),
                    ],
                )?;
            }
            let listed: BTreeSet<&str> = ids.into_iter().collect();
            let mut removed = 0;
            for kept in kept.values().filter(|kept| kept.listed) {
                if listed.contains(kept.mapping_id.as_str()) {
                    continue;
                }
                removed += 1;
                log(
                    &kept.mapping_id,
                    &kept.fields,
                    "removed",
                    Some(&kept.fields),
                    None,
                )?;
                db.exec(
                    "UPDATE shift_assignments SET listed=0 WHERE mapping_id=?",
                    [&kept.mapping_id],
                )?;
            }
            db.exec(
                "INSERT INTO shift_runs(job_id,station,service_area_id,week_start,week_end,\
                 collected_at,drivers,assignments,added,changed,removed) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,?)",
                params![
                    job,
                    capture.station,
                    capture.service_area_id,
                    capture.week_start,
                    capture.week_end,
                    collected_at,
                    capture.drivers as i64,
                    capture.assignments.len() as i64,
                    added,
                    changed,
                    removed,
                ],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "../tests/backend/storage.rs"]
mod tests;
