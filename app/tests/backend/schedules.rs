//! Collection schedules as the Collectors page sets them, of the collections Timecard keeps
//! from Paycom and Cortex: their rules saved together, switched on and off, what they queue
//! and when, and what stops them.
#[cfg(feature = "timecard")]
use dispatch_core::testing::schedule;
use dispatch_core::{
    collection::{metrics::Metrics, schedules::Scheduled},
    db::{Store, iso, now, s},
    foundation::{config::Config, crypto},
    testing::schedule_due_at,
};
use dispatch_cortex as cortex;
use dispatch_paycom as paycom;
#[cfg(feature = "timecard")]
use dispatch_timecard::TimecardStore;
use rusqlite::params;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;

fn setup() -> (tempfile::TempDir, Store, String) {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = Config::load().unwrap();
    config.root = root.path().into();
    let db = Store::initialize(config).unwrap();
    let id = crypto::id("dsp").unwrap();
    db.platform
        .exec(
            "INSERT INTO dsps(id,name,environment,status,timezone,created_at) \
        VALUES (?,'Schedule test','preview','provisioning','America/Chicago',?)",
            [&id, &iso()],
        )
        .unwrap();
    db.provision(&id).unwrap();
    db.enable_all_features(&id).unwrap();
    db.collector(&id, paycom::PROVIDER)
        .unwrap()
        .exec("UPDATE connections SET enabled=1", [])
        .unwrap();
    (root, db, id)
}
/// A rule of `collection`'s at `times` every day.
fn daily(collection: &str, times: &[&str]) -> Value {
    json!({"collects":collection,"kind":"times","days":[0,1,2,3,4,5,6],"times":times})
}
/// `collection`'s schedule as saved: its rules' ids and pending runs, and its revision.
fn saved(db: &Store, id: &str, collection: &str) -> (Vec<(String, Option<String>)>, String) {
    let shown = db
        .scheduled_collection(id, Scheduled::named(collection).unwrap(), false)
        .unwrap();
    let rules = shown
        .rules
        .into_iter()
        .map(|rule| (rule.id.unwrap(), rule.next_run))
        .collect();
    (rules, shown.revision)
}
/// When the collection runs next, as the Collectors page shows it.
fn next_run(db: &Store, id: &str, collection: &str) -> Option<String> {
    db.scheduled_collection(id, Scheduled::named(collection).unwrap(), false)
        .unwrap()
        .next_run
}
fn save(db: &Store, id: &str, collection: &str, rules: Value, revision: &str) -> String {
    let scheduled = Scheduled::named(collection).unwrap();
    let value = json!({"rules":rules,"revision":revision});
    match db.save_rules(id, scheduled, &value) {
        Ok(_) => "saved".into(),
        Err(error) => error.code,
    }
}
fn switch(db: &Store, id: &str, collection: &str, enabled: bool) -> String {
    let scheduled = Scheduled::named(collection).unwrap();
    let revision = saved(db, id, collection).1;
    let value = json!({"enabled":enabled,"revision":revision});
    match db.switch_rules(id, scheduled, &value) {
        Ok(_) => "switched".into(),
        Err(error) => error.code,
    }
}
#[cfg(feature = "timecard")]
fn meals(db: &Store, id: &str) {
    db.collector(id, cortex::PROVIDER)
        .unwrap()
        .exec("UPDATE connections SET enabled=1", [])
        .unwrap();
    let scope = cortex::discovery::Scope {
        date: "2026-01-10".into(),
        station: "DEMO1".into(),
        service_area_id: "area-demo".into(),
        provider: "provider-demo".into(),
        timezone: "America/Chicago".into(),
    };
    db.publish_meals(id, "seed-meals", &cortex::meals::fixture(&scope), &scope)
        .unwrap();
}
#[cfg(feature = "timecard")]
fn jobs(db: &Store) -> Vec<String> {
    let mut kinds: Vec<String> = db
        .jobs
        .all("SELECT kind FROM jobs", [])
        .unwrap()
        .iter()
        .map(|job| s(job, "kind").to_owned())
        .collect();
    kinds.sort();
    kinds
}

/// A collection's rules are saved together, as at the revision they were read at: a rule
/// kept by its id keeps its row and, unchanged, its pending run; one left out is removed; and
/// a collection stays on or off as it was, one with no rules starting on.
#[test]
fn a_collections_rules_are_saved_together_as_they_were_read() {
    crate::install();
    let (_root, db, id) = setup();
    assert_eq!(
        save(&db, &id, "paycom", json!([daily("paycom", &["06:00"])]), ""),
        "saved"
    );
    let (rules, revision) = saved(&db, &id, "paycom");
    let (first, pending) = rules[0].clone();
    assert!(pending.is_some());
    // A run moved by the scheduler stays put while another rule is added.
    schedule_due_at(&db, &id, &first, "2030-01-01T12:00:00.000Z").unwrap();
    let mut kept = daily("paycom", &["06:00"]);
    kept["id"] = json!(first);
    let rules = json!([kept, daily("paycom", &["14:00", "09:30"])]);
    assert_eq!(save(&db, &id, "paycom", rules.clone(), &revision), "saved");
    let (now_saved, revision) = saved(&db, &id, "paycom");
    assert_eq!(now_saved.len(), 2);
    assert_eq!(
        now_saved[0],
        (first.clone(), Some("2030-01-01T12:00:00.000Z".into()))
    );
    // What another save passed, or a rule it removed, is stale.
    assert_eq!(
        save(&db, &id, "paycom", rules, "paycom.1"),
        "schedule_changed"
    );
    let mut gone = daily("paycom", &["06:00"]);
    gone["id"] = json!("schedule_gone");
    assert_eq!(
        save(&db, &id, "paycom", json!([gone]), &revision),
        "schedule_changed"
    );
    // A rule reads only by what its collection offers.
    assert_eq!(
        save(
            &db,
            &id,
            "paycom",
            json!([daily("dvic", &["06:00"])]),
            &revision
        ),
        "invalid_input"
    );
    assert_eq!(saved(&db, &id, "paycom").1, revision);

    // Off, a collection keeps no pending run, and what's saved stays off; on, each runs again.
    assert_eq!(switch(&db, &id, "paycom", false), "switched");
    let (rules, revision) = saved(&db, &id, "paycom");
    assert!(rules.iter().all(|(_, next)| next.is_none()));
    let second = rules[1].0.clone();
    let mut changed = daily("paycom", &["07:00"]);
    changed["id"] = json!(second);
    assert_eq!(
        save(&db, &id, "paycom", json!([changed]), &revision),
        "saved"
    );
    let (rules, _) = saved(&db, &id, "paycom");
    assert_eq!(rules, [(second, None)]);
    assert_eq!(next_run(&db, &id, "paycom"), None);
    assert_eq!(switch(&db, &id, "paycom", true), "switched");
    assert!(next_run(&db, &id, "paycom").is_some());
    // A collection whose collector isn't connected starts off, and doesn't come on.
    assert_eq!(
        save(&db, &id, "vans", json!([daily("vans", &["06:00"])]), ""),
        "saved"
    );
    assert_eq!(next_run(&db, &id, "vans"), None);
    assert_eq!(switch(&db, &id, "vans", true), "schedule_vans_required");
}

/// The rules due together are queued in one batch, a collection once however many of its
/// rules are due, and each moves on to its next run; a run queued before a restart isn't
/// queued again.
#[cfg(feature = "timecard")]
#[test]
fn rules_due_together_queue_one_batch_once() {
    crate::install();
    let (_root, db, id) = setup();
    meals(&db, &id);
    let timecards = schedule(
        &db,
        &id,
        "paycom",
        json!([daily("paycom", &["06:00"]), daily("paycom", &["18:00"])]),
    )
    .unwrap();
    let breaks = schedule(
        &db,
        &id,
        "meal_break",
        json!([daily("meal_break", &["06:00"])]),
    )
    .unwrap();
    let due = "2026-01-01T00:00:00.000Z";
    let rules: Vec<String> = timecards
        .rules
        .iter()
        .chain(&breaks.rules)
        .map(|rule| rule.id.clone().unwrap())
        .collect();
    for rule in &rules {
        schedule_due_at(&db, &id, rule, due).unwrap();
    }
    assert!(db.schedule_due(&id).unwrap().unwrap() > now());
    assert_eq!(jobs(&db), ["cortex.meal_breaks.collect", "paycom.collect"]);
    let today = db.local_date(&id).unwrap();
    let meal = db
        .jobs
        .one(
            "SELECT request FROM jobs WHERE kind='cortex.meal_breaks.collect'",
            [],
        )
        .unwrap()
        .unwrap();
    let request: Value = serde_json::from_str(s(&meal, "request")).unwrap();
    assert_eq!(
        (request["station"].as_str(), request["date"].as_str()),
        (Some("DEMO1"), Some(today.as_str()))
    );
    for (_, next) in saved(&db, &id, "paycom").0 {
        assert!(next.is_some_and(|next| next.as_str() > due));
    }
    // Stopped after queueing, before moving on the rules that queued it: started again, it
    // queues nothing.
    let first = |shown: &dispatch_core::collection::api::types::ScheduledCollection| {
        shown
            .rules
            .iter()
            .filter_map(|rule| rule.id.clone())
            .min()
            .unwrap()
    };
    for rule in [first(&timecards), first(&breaks)] {
        schedule_due_at(&db, &id, &rule, due).unwrap();
    }
    db.schedule_due(&id).unwrap();
    assert_eq!(jobs(&db).len(), 2);
    let waiting = db
        .dsp(&id)
        .unwrap()
        .count(
            "SELECT count(*) FROM collection_schedules WHERE last_error IS NOT NULL",
            [],
        )
        .unwrap();
    assert_eq!(waiting, 0);
}

/// A rule that can't run yet waits, saying why, without holding back the others due with it;
/// while any collection runs, every rule due waits for it; and a collector disconnected
/// pauses only the rules that need it.
#[cfg(feature = "timecard")]
#[test]
fn a_rule_that_cannot_run_waits_without_holding_back_the_rest() {
    crate::install();
    let (_root, db, id) = setup();
    meals(&db, &id);
    let timecards = schedule(&db, &id, "paycom", json!([daily("paycom", &["06:00"])])).unwrap();
    let breaks = schedule(
        &db,
        &id,
        "meal_break",
        json!([daily("meal_break", &["06:00"])]),
    )
    .unwrap();
    let paycom = timecards.rules[0].id.clone().unwrap();
    let meal = breaks.rules[0].id.clone().unwrap();
    let error = |rule: &str| {
        db.dsp(&id)
            .unwrap()
            .one_as::<(Option<String>,)>(
                "SELECT last_error FROM collection_schedules WHERE id=?",
                [rule],
            )
            .unwrap()
            .unwrap()
            .0
    };
    db.collector(&id, cortex::PROVIDER)
        .unwrap()
        .exec("UPDATE connections SET enabled=0", [])
        .unwrap();
    for rule in [&paycom, &meal] {
        schedule_due_at(&db, &id, rule, "2026-01-01T00:00:00.000Z").unwrap();
    }
    db.schedule_due(&id).unwrap();
    assert_eq!(jobs(&db), ["paycom.collect"]);
    assert_eq!(error(&paycom), None);
    assert_eq!(error(&meal).as_deref(), Some("schedule_meals_required"));

    // Paycom's run is queued: its next, due, waits for it.
    schedule_due_at(&db, &id, &paycom, "2026-01-02T00:00:00.000Z").unwrap();
    db.schedule_due(&id).unwrap();
    assert_eq!(jobs(&db).len(), 1);
    assert_eq!(error(&paycom).as_deref(), Some("sync_in_progress"));

    db.pause_provider_schedules(&id, cortex::PROVIDER).unwrap();
    assert!(
        !saved(&db, &id, "meal_break")
            .0
            .iter()
            .any(|(_, next)| next.is_some())
    );
    assert!(next_run(&db, &id, "paycom").is_some());
}

#[test]
fn a_timezone_change_retimes_every_rule() {
    crate::install();
    let (_root, db, id) = setup();
    assert_eq!(
        save(&db, &id, "paycom", json!([daily("paycom", &["06:00"])]), ""),
        "saved"
    );
    db.retime_schedules(&id, "Asia/Tokyo").unwrap();
    let next = next_run(&db, &id, "paycom").unwrap();
    let local = chrono::DateTime::parse_from_rfc3339(&next)
        .unwrap()
        .with_timezone(&chrono_tz::Asia::Tokyo);
    assert_eq!(local.format("%H:%M").to_string(), "06:00");
}

/// Timecard's `both` ran Paycom's timecards and Cortex's meal breaks at once. As a DSP opens,
/// each of its schedules becomes one rule of each, timed as it was; a rule the previous release
/// changed reads as that release ran it, and one it wrote as it runs it: one time every day, or
/// a repeat all day, or, repeating less often than a rule may, the times it falls at.
#[cfg(feature = "timecard")]
#[test]
fn schedules_the_previous_release_left_read_as_it_ran_them() {
    crate::install();
    let (_root, db, id) = setup();
    let old = |key: &str, collection: &str, time: &str, interval: Option<i64>, created: &str| {
        db.dsp(&id)
            .unwrap()
            .exec(
                "INSERT INTO collection_schedules(id,name,collection,cadence,interval_minutes,\
                local_time,anchor,enabled,next_run,created_at) VALUES (?,'Sync',?,?,?,?,0,1,\
                '2030-01-01T22:00:00.000Z',?)",
                params![
                    key,
                    collection,
                    if interval.is_some() {
                        "interval"
                    } else {
                        "daily"
                    },
                    interval,
                    time,
                    created
                ],
            )
            .unwrap();
    };
    let set = |key: &str, columns: &str| {
        let sql = format!("UPDATE collection_schedules SET {columns} WHERE id=?");
        db.dsp(&id).unwrap().exec(&sql, [key]).unwrap();
    };
    old(
        "sch_both",
        "both",
        "16:00",
        None,
        "2026-01-01T00:00:01.000Z",
    );
    old(
        "sch_paycom",
        "paycom",
        "11:00",
        None,
        "2026-01-01T00:00:02.000Z",
    );
    // A weekday rule of two times whose first time the previous release moved to 08:00.
    old(
        "sch_moved",
        "paycom",
        "08:00",
        None,
        "2026-01-01T00:00:03.000Z",
    );
    set("sch_moved", "days=31,times='[\"06:00\",\"14:00\"]'");
    // A Monday and Wednesday repeat, 6:00 AM to 6:00 PM, that it moved to start at 8:00 PM.
    old(
        "sch_repeat",
        "paycom",
        "20:00",
        Some(60),
        "2026-01-01T00:00:04.000Z",
    );
    set(
        "sch_repeat",
        "days=5,times='{\"from\":\"06:00\",\"every\":60}',until_time='18:00'",
    );
    // Every 15 hours around the clock, as the previous release allowed.
    old(
        "sch_long",
        "paycom",
        "06:00",
        Some(900),
        "2026-01-01T00:00:05.000Z",
    );
    db.open_collectors(&id).unwrap();
    let rules = |collection: &str| -> Vec<Value> {
        let shown = db
            .scheduled_collection(&id, Scheduled::named(collection).unwrap(), false)
            .unwrap();
        shown
            .rules
            .into_iter()
            .map(|rule| {
                let when = json!([rule.kind, rule.times, rule.every, rule.from, rule.to]);
                json!([rule.collects, rule.days, when, rule.next_run])
            })
            .collect()
    };
    let every_day = json!([0, 1, 2, 3, 4, 5, 6]);
    let pending = "2030-01-01T22:00:00.000Z";
    let at = |collection: &str, times: &[&str]| {
        json!([
            collection,
            every_day,
            ["times", times, null, null, null],
            pending
        ])
    };
    let all_day = json!([
        "paycom",
        every_day,
        ["repeat", [], 60, "20:00", "20:00"],
        pending
    ]);
    assert_eq!(
        rules("paycom"),
        [
            at("paycom", &["16:00"]),
            at("paycom", &["11:00"]),
            at("paycom", &["08:00"]),
            all_day,
            at("paycom", &["06:00", "21:00"]),
        ]
    );
    assert_eq!(rules("meal_break"), [at("meal_break", &["16:00"])]);
    let both = db
        .dsp(&id)
        .unwrap()
        .count(
            "SELECT count(*) FROM collection_schedules WHERE collection='both'",
            [],
        )
        .unwrap();
    assert_eq!(both, 0);
}

/// After a rollback, a newer release's schedule and queued job name a collection no
/// registered collector offers. This release lists, counts, retimes and runs neither, runs
/// the DSP's own as before, and leaves them as they were for the release that knows them.
/// The DSP's own runs Paycom's timecards, which Timecard keeps.
#[cfg(feature = "timecard")]
#[test]
fn a_newer_releases_schedule_and_job_wait_untouched_for_it() {
    crate::install();
    let (_root, db, id) = setup();
    let own = schedule(&db, &id, "paycom", json!([daily("paycom", &["06:00"])]))
        .unwrap()
        .rules[0]
        .id
        .clone()
        .unwrap();
    db.dsp(&id)
        .unwrap()
        .exec(
            "INSERT INTO collection_schedules(id,name,collection,cadence,interval_minutes,\
            local_time,anchor,enabled,next_run,created_at) VALUES ('sch_newer','Newer',\
            'newer','interval',120,'00:00',0,1,'2026-01-01T00:00:00.000Z',?)",
            [iso()],
        )
        .unwrap();
    db.jobs
        .exec(
            "INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,\
            release,connection_revision,idempotency_key) \
            VALUES ('job_newer',?,?,'newer.collect','queued',0,?,'next',1,'newer')",
            params![id, db.config.environment, iso()],
        )
        .unwrap();
    let newer = || {
        (
            db.dsp(&id)
                .unwrap()
                .all(
                    "SELECT * FROM collection_schedules WHERE id='sch_newer'",
                    [],
                )
                .unwrap(),
            db.jobs
                .all("SELECT * FROM jobs WHERE id='job_newer'", [])
                .unwrap(),
        )
    };
    let left = newer();

    assert_eq!(db.schedule_label(&id, "sch_newer").unwrap(), None);
    assert_eq!(db.schedule_label(&id, &own).unwrap(), Some("Timecards"));
    assert!(db.recent_jobs(None).unwrap().is_empty());
    assert!(db.recent_jobs(Some(&id)).unwrap().is_empty());
    assert_eq!(
        db.job_row("job_newer", None).unwrap_err().code,
        "job_not_found"
    );

    // The DSP's own schedule runs, neither held back by the job nor timed by the schedule.
    schedule_due_at(&db, &id, &own, "2026-01-01T00:00:00.000Z").unwrap();
    db.schedule_due(&id).unwrap();
    assert_eq!(jobs(&db), ["newer.collect", "paycom.collect"]);
    let deadlines = db.schedule_deadlines().unwrap();
    let (_, deadline) = deadlines.iter().find(|(dsp, _)| *dsp == id).unwrap();
    assert!(*deadline > now());
    db.retime_schedules(&id, "Asia/Tokyo").unwrap();

    // A worker runs the DSP's job, never the newer one, and a manual sync isn't refused
    // as one already in progress.
    let claimed = db.claim_job("worker", |_, _| true).unwrap().unwrap();
    assert_eq!(claimed.kind.as_str(), "paycom.collect");
    db.jobs
        .exec(
            "UPDATE jobs SET status='succeeded',lease_owner=NULL WHERE id=?",
            [&claimed.id],
        )
        .unwrap();
    assert!(db.claim_job("worker", |_, _| true).unwrap().is_none());
    db.enqueue_for(
        &id,
        Some("usr_sync"),
        "manual",
        paycom::PROVIDER,
        &json!({}),
    )
    .unwrap();
    assert_eq!(db.recent_jobs(Some(&id)).unwrap().len(), 2);

    assert_eq!(newer(), left);
}

/// The Collectors page counts what a collection's latest successful run brought, as its last
/// attempt counted it: an earlier run, or a later one that failed, counts for nothing.
#[test]
fn a_collection_counts_what_its_latest_successful_run_brought() {
    let (_root, db, id) = setup();
    let counts = || {
        db.scheduled_collection(&id, Scheduled::named("paycom").unwrap(), false)
            .unwrap()
            .counts
            .map(|counts| (counts.employees, counts.timecards))
    };
    assert_eq!(counts(), None);
    let run = |job: &str, status: &str, created_at: &str, attempts: &[(i64, usize)]| {
        db.jobs
            .exec(
                "INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,\
                 release,connection_revision,idempotency_key) \
                 VALUES (?,?,'preview','paycom.collect',?,0,?,'test',1,?)",
                params![job, id, status, created_at, job],
            )
            .unwrap();
        for &(attempt, employees) in attempts {
            let mut metrics = Metrics::new(&json!({ "attempt": attempt }));
            metrics.employees = Some(employees);
            metrics.timecards = Some(employees * 8);
            db.jobs
                .exec(
                    "INSERT INTO job_metrics(job_id,attempt,owner,metrics) VALUES (?,?,'test',?)",
                    params![job, attempt, serde_json::to_string(&metrics).unwrap()],
                )
                .unwrap();
        }
    };
    run("earlier", "succeeded", "2026-09-01T05:00:00Z", &[(1, 50)]);
    run(
        "latest",
        "succeeded",
        "2026-09-02T05:00:00Z",
        &[(1, 10), (2, 64)],
    );
    run("failed", "failed", "2026-09-03T05:00:00Z", &[(1, 3)]);
    assert_eq!(counts(), Some((Some(64), Some(512))));
}
