//! Letting an old day's delivered and picked-up task rows go, once every one of them is proven
//! to read the same from the response kept whole beside them.
//!
//! Three quarters of historical route data is its task rows and their indexes, though each
//! day's responses hold all of it compressed. A day older than `KEPT_DAYS` is checked an
//! itinerary at a time: its task rows as they are stored against the rows its stored response
//! reads as (`reads::stored_tasks`, which every read of such a day uses). Only when every
//! itinerary's rows match is the day marked (`tasks_pruned`) and its packages named in
//! `package_days`, in one write; its rows are then deleted a few at a time. A day with one
//! itinerary that differs, or whose response can't be read, keeps every row. Its undelivered
//! and removed tasks stay as rows, which what other features read of a day's drivers is made
//! of. A reprocess of the day rebuilds every row from the same responses.
use super::{COLLECTION, RoutesStore, scope::routes_today, storage::stored_tasks};
use dispatch_core::{
    Result,
    db::{Db, Store, s},
    foundation::observability,
};
use rusqlite::params;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{LazyLock, Mutex},
};

/// The days kept with every task row: a day older than these, where the DSP is, lets its
/// delivered and picked-up rows go.
const KEPT_DAYS: i64 = 30;
/// Rows deleted in one step, so the DSP's writes never wait long for it.
const BATCH: i64 = 2000;
/// The task rows a pruned day lets go: what a driver delivered or picked up. Its undelivered
/// drop-offs and the tasks removed from a route stay.
const LET_GO: &str = "t.active=1 AND (t.task_type='PICK_UP' OR t.task_state='DELIVERED')";
/// What a task row holds, as its stored response must read too.
const FACTS: [&str; 29] = [
    "task_id",
    "stop_id",
    "day",
    "transporter_id",
    "tracking_id",
    "order_id",
    "task_type",
    "task_state",
    "state_context",
    "execution_status",
    "address_id",
    "window_start_at",
    "window_end_at",
    "executed_at",
    "latitude",
    "longitude",
    "box_type",
    "weight",
    "weight_unit",
    "length",
    "width",
    "height",
    "volume_unit",
    "time_windowed",
    "high_value",
    "customer_return",
    "address_type",
    "events",
    "active",
];

/// Where a DSP's old days stand between steps. Kept in the server's memory: started again,
/// it finds a day left half deleted first, and checks the next from its first itinerary.
#[derive(Clone, Default)]
enum Progress {
    /// A day left marked with rows still to delete, as a restart leaves one, is looked for.
    #[default]
    Unknown,
    /// No day is under way.
    Idle,
    /// A day's itineraries still to check, each against its stored response.
    Checking {
        publication: String,
        pending: Vec<String>,
    },
    /// A day whose every itinerary matched, to mark.
    Marking(String),
    /// A marked day's rows still to delete.
    Deleting(String),
}
#[derive(Default)]
struct Days {
    progress: Progress,
    /// Days that keep their rows for good while the server runs: an itinerary differed.
    kept: HashSet<String>,
}
static DAYS: LazyLock<Mutex<HashMap<String, Days>>> = LazyLock::new(Mutex::default);

/// What is due next for a DSP's old days.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Next {
    /// Nothing more now: no old day keeps rows it could let go.
    Done,
    /// Another look (`look`).
    Look,
    /// A write (`write`): a day to mark, or rows of a marked day to delete.
    Write,
}

/// One look at the DSP's old days, changing nothing stored: finds the day to take next, or
/// checks one of its itineraries against its stored response. Nothing while Routes is off for
/// the DSP.
pub(super) fn look(store: &Store, id: &str) -> Result<Next> {
    if !store.feature_enabled(id, COLLECTION)? {
        return Ok(Next::Done);
    }
    let db = store.routes_db(id)?;
    let mut all = DAYS.lock().expect("old route days");
    let days = all.entry(id.to_owned()).or_default();
    // A look that fails leaves the day where it was, for the next pass to take up again.
    let was = days.progress.clone();
    let looked = looked(store, id, &db, days);
    if looked.is_err() {
        days.progress = was;
    }
    looked
}
fn looked(store: &Store, id: &str, db: &Db, days: &mut Days) -> Result<Next> {
    match std::mem::replace(&mut days.progress, Progress::Idle) {
        Progress::Unknown => {
            let left = db.one_as::<(String,)>(
                &format!(
                    "SELECT p.id FROM route_publications p WHERE p.tasks_pruned=1 AND EXISTS \
                     (SELECT 1 FROM tasks t WHERE t.publication_id=p.id AND {LET_GO}) LIMIT 1"
                ),
                [],
            )?;
            if let Some((publication,)) = left {
                days.progress = Progress::Deleting(publication);
            }
            Ok(Next::Look)
        }
        Progress::Idle => {
            let before = (routes_today(store, id)? - chrono::Duration::days(KEPT_DAYS)).to_string();
            let kept = serde_json::to_string(&days.kept.iter().collect::<Vec<_>>())?;
            let Some((publication,)) = db.one_as::<(String,)>(
                "SELECT id FROM route_publications WHERE active=1 AND tasks_pruned=0 AND day<?1 \
                 AND id NOT IN (SELECT value FROM json_each(?2)) ORDER BY day LIMIT 1",
                params![before, kept],
            )?
            else {
                return Ok(Next::Done);
            };
            let pending = db
                .all(
                    "SELECT itinerary_id FROM itineraries WHERE publication_id=? \
                     ORDER BY itinerary_id DESC",
                    [&publication],
                )?
                .iter()
                .map(|row| s(row, "itinerary_id").to_owned())
                .collect();
            days.progress = Progress::Checking {
                publication,
                pending,
            };
            Ok(Next::Look)
        }
        Progress::Checking {
            publication,
            mut pending,
        } => {
            // The day as it is now: one retired or replaced since is left alone.
            if !unpruned(db, &publication)? {
                return Ok(Next::Look);
            }
            let Some(itinerary) = pending.pop() else {
                days.progress = Progress::Marking(publication);
                return Ok(Next::Write);
            };
            match differs(db, &publication, &itinerary) {
                Ok(None) => {
                    days.progress = Progress::Checking {
                        publication,
                        pending,
                    };
                }
                differs => {
                    // Which fact differed, by its column's name: never what it held.
                    let (why, fact) = match differs {
                        Ok(fact) => ("rows_differ".to_owned(), fact),
                        Err(error) => (error.code, None),
                    };
                    observability::event(
                        "warn",
                        "routes.old_day_kept",
                        json!({"dspId":id,"publication":publication,"reason":why,"fact":fact}),
                    );
                    days.kept.insert(publication);
                }
            }
            Ok(Next::Look)
        }
        writing => {
            days.progress = writing;
            Ok(Next::Write)
        }
    }
}

/// The write a look found due: marks a day whose every itinerary matched, or deletes a batch
/// of a marked day's rows. A day retired or replaced since it was checked is left alone.
pub(super) fn write(store: &Store, id: &str) -> Result<()> {
    if !store.feature_enabled(id, COLLECTION)? {
        return Ok(());
    }
    let db = store.routes_db(id)?;
    let mut all = DAYS.lock().expect("old route days");
    let days = all.entry(id.to_owned()).or_default();
    // A write that fails leaves the day where it was: a marked day's rows are deleted by a
    // later pass, never forgotten.
    let was = days.progress.clone();
    let wrote = wrote(&db, days);
    if wrote.is_err() {
        days.progress = was;
    }
    wrote
}
fn wrote(db: &Db, days: &mut Days) -> Result<()> {
    match std::mem::replace(&mut days.progress, Progress::Idle) {
        Progress::Marking(publication) => {
            if unpruned(db, &publication)? {
                mark(db, &publication)?;
                days.progress = Progress::Deleting(publication);
            }
        }
        Progress::Deleting(publication) => {
            let removed = db.exec(
                &format!(
                    "DELETE FROM tasks WHERE rowid IN (SELECT t.rowid FROM tasks t \
                     JOIN route_publications p ON p.id=t.publication_id AND p.tasks_pruned=1 \
                     WHERE t.publication_id=?1 AND {LET_GO} LIMIT ?2)"
                ),
                params![publication, BATCH],
            )?;
            if removed > 0 {
                days.progress = Progress::Deleting(publication);
            }
        }
        other => days.progress = other,
    }
    Ok(())
}

/// Whether `publication` is still a day's read with every task row.
fn unpruned(db: &Db, publication: &str) -> Result<bool> {
    Ok(db
        .one(
            "SELECT 1 FROM route_publications WHERE id=? AND active=1 AND tasks_pruned=0",
            [publication],
        )?
        .is_some())
}

/// How an itinerary's task rows differ from what its stored response reads as, if they do:
/// `tasks` when they are not the same tasks, else the first fact of a task that differs, by
/// its column's name. None when every task is there with every fact a row holds.
fn differs(db: &Db, publication: &str, itinerary: &str) -> Result<Option<&'static str>> {
    let facts = |task: &Value, events: Value| -> (String, Vec<Value>) {
        let facts = FACTS
            .iter()
            .map(|fact| match *fact {
                "events" => events.clone(),
                fact => task[fact].clone(),
            })
            .collect();
        (s(task, "task_id").to_owned(), facts)
    };
    let stored: BTreeMap<String, Vec<Value>> = db
        .all(
            "SELECT * FROM tasks WHERE publication_id=? AND itinerary_id=?",
            [publication, itinerary],
        )?
        .iter()
        .map(|task| {
            // A row keeps its events as JSON text.
            let events = serde_json::from_str(s(task, "events")).unwrap_or(Value::Null);
            facts(task, events)
        })
        .collect();
    let read: BTreeMap<String, Vec<Value>> = stored_tasks(db, publication, itinerary)?
        .iter()
        .map(|task| facts(task, task["events"].clone()))
        .collect();
    if !stored.keys().eq(read.keys()) {
        return Ok(Some("tasks"));
    }
    Ok(stored.values().zip(read.values()).find_map(|(row, read)| {
        let at = row.iter().zip(read).position(|(a, b)| a != b)?;
        Some(FACTS[at])
    }))
}

/// Marks a day whose every itinerary matched, and names where each package it lets go is, in
/// one write: from then its tasks are read from its stored responses.
fn mark(db: &Db, publication: &str) -> Result<()> {
    db.transaction(|| {
        db.exec(
            &format!(
                "INSERT OR IGNORE INTO package_days (tracking_id,publication_id,itinerary_id) \
                 SELECT DISTINCT t.tracking_id,t.publication_id,t.itinerary_id FROM tasks t \
                 WHERE t.publication_id=? AND t.tracking_id IS NOT NULL AND {LET_GO}"
            ),
            [publication],
        )?;
        db.exec(
            "UPDATE route_publications SET tasks_pruned=1 WHERE id=? AND active=1",
            [publication],
        )?;
        Ok(())
    })
}
