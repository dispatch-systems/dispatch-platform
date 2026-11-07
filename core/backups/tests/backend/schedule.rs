//! A DSP's backup rules: saved only over the schedule they were read from and only as the DSP's
//! own, each due moved on to its next run, a failed one run again within the hour, and what each
//! takes kept as long as it says, a month as the calendar has it.
use super::*;
use crate::testing::{self, dsp_named};
use serde_json::json;

fn rule(id: Option<&str>, time: &str) -> Value {
    json!({
        "id": id, "on": "week", "days": [0, 1, 2, 3, 4, 5, 6], "dates": [], "lastDay": false,
        "kind": "times", "times": [time], "every": null, "from": null, "to": null,
        "keep": "for", "keepCount": 7, "keepUnit": "day", "skipped": [],
    })
}

#[test]
fn rules_save_over_the_schedule_they_were_read_from_and_run_when_due() {
    testing::install(&[], &[]);
    let (_root, db) = testing::seeded();
    let north = dsp_named(&db, "Northline Logistics");
    let summit = dsp_named(&db, "Summit Delivery");
    // Unsaved, a DSP has the rule every DSP starts with.
    let first = schedule(&db.platform, &north).unwrap();
    assert_eq!(
        (first.enabled, first.revision, first.rules.len()),
        (true, 0, 1)
    );
    assert_eq!(first.rules[0].skipped, [contents::PICTURES]);

    save(
        &db,
        &north,
        &json!({"revision": 0, "rules": [rule(None, "03:00")]}),
    )
    .unwrap();
    let saved = schedule(&db.platform, &north).unwrap();
    let id = saved.rules[0].id.clone().unwrap();
    // Read before that save, a schedule can't be saved over it.
    let stale = save(&db, &north, &json!({"revision": 0, "rules": []})).unwrap_err();
    assert_eq!(stale.code, "backup_schedule_changed");
    // Another DSP's rule can't be changed through this one's.
    save(
        &db,
        &summit,
        &json!({"revision": 0, "rules": [rule(None, "04:00")]}),
    )
    .unwrap();
    let theirs = schedule(&db.platform, &summit).unwrap().rules[0]
        .id
        .clone()
        .unwrap();
    let foreign = save(
        &db,
        &north,
        &json!({"revision": saved.revision, "rules": [rule(Some(&theirs), "05:00")]}),
    )
    .unwrap_err();
    assert_eq!(foreign.code, "invalid_input");
    // A rule keeps what it takes for a while, and every backup holds the DSP's own data.
    let mut kept_for_none = rule(None, "05:00");
    kept_for_none["keepCount"] = json!(0);
    let mut without_dispatch = rule(None, "05:00");
    without_dispatch["skipped"] = json!([contents::DISPATCH]);
    for wrong in [kept_for_none, without_dispatch] {
        let refused = save(
            &db,
            &north,
            &json!({"revision": saved.revision, "rules": [wrong]}),
        )
        .unwrap_err();
        assert_eq!(refused.code, "invalid_input");
    }

    // Due, a rule moves on to its next run; failed, it runs again within the hour.
    let tz = timezone(&db, &north).unwrap();
    let next = saved.rules[0].next_run.unwrap();
    assert!(due(&db.platform, &north, tz, next - 1).unwrap().is_empty());
    let ran = due(&db.platform, &north, tz, next).unwrap();
    assert_eq!(
        ran.iter().map(|k| k.id.clone()).collect::<Vec<_>>(),
        [Some(id.clone())]
    );
    let moved = schedule(&db.platform, &north).unwrap().rules[0]
        .next_run
        .unwrap();
    assert_eq!(moved, saved.rules[0].when.next(tz, next).unwrap());
    retry(&db.platform, &id, next).unwrap();
    let retried = schedule(&db.platform, &north).unwrap().rules[0]
        .next_run
        .unwrap();
    assert_eq!(retried, (next + 60 * 60 * 1000).min(moved));
    // Off, none is due.
    switch(&db, &north, false).unwrap();
    assert!(due(&db.platform, &north, tz, i64::MAX).unwrap().is_empty());
}

#[test]
fn what_a_rule_takes_is_kept_as_long_as_it_says() {
    let day = 24 * 60 * 60 * 1000;
    let at = |text: &str| {
        chrono::DateTime::parse_from_rfc3339(text)
            .unwrap()
            .timestamp_millis()
    };
    let taken = at("2027-01-31T02:00:00Z");
    assert_eq!(expires(taken, 30, KeepUnit::Day), taken + 30 * day);
    assert_eq!(expires(taken, 2, KeepUnit::Week), taken + 14 * day);
    assert_eq!(
        expires(taken, 1, KeepUnit::Month),
        at("2027-02-28T02:00:00Z")
    );
    assert_eq!(
        expires(taken, 1, KeepUnit::Year),
        at("2028-01-31T02:00:00Z")
    );
}
