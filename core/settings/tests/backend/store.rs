//! A section's values: defaults until saved, a save checked as each field's kind says and
//! merged onto the rest, refused at a stale revision, held back by a warning until confirmed,
//! and recorded in the DSP's log as each field shows it; a member's own aren't.
use super::*;
use crate::{
    Result,
    db::Store,
    manifest::settings::{
        Field, Kept, Options, Section, choice, many, number, pick, schedules, section, text, time,
        timezone, toggle,
    },
    testing,
};
use serde::Deserialize;
use serde_json::{Value, json};

fn departments(_: &Store, _: &str) -> Result<Vec<(String, String)>> {
    Ok(vec![
        ("dsp".into(), "Delivery".into()),
        ("ops".into(), "Operations".into()),
    ])
}
const FIELDS: &[Field] = &[
    toggle("enabled", "Collects", true),
    choice(
        "mode",
        "Mode",
        &[("final", "Final"), ("snapshot", "Snapshot")],
        "final",
    ),
    pick("department", "Department", departments, None).none("Every department"),
    many(
        "columns",
        "Columns",
        Options::Fixed(&[("in", "In"), ("out", "Out"), ("total", "Total")]),
        &["in", "out"],
    ),
    number("days", "Keep", 30, 3650, None)
        .unit("days")
        .presets(&[(90, "90 days")])
        .none("Every day"),
    text("code", "Short code", 2, 16, "").code().locked(),
    time("at", "Collects at", "18:00"),
    timezone("zone", "Timezone", "America/Chicago"),
    schedules(
        "schedules",
        "Schedules",
        "/api/dsp/stand-in/schedules",
        "stand_in",
        ("Daily", "18:00"),
    ),
];
const POLICY: Section = section("stand_in.policy", "Policy", "stand_in.manage", FIELDS);

#[derive(Debug, Deserialize, PartialEq)]
struct Policy {
    enabled: bool,
    mode: String,
    department: Option<String>,
    columns: Vec<String>,
    days: Option<i64>,
    code: String,
    at: String,
    zone: String,
}

fn northline() -> (tempfile::TempDir, Store, String, String) {
    testing::install(&[], &[]);
    let (root, db, dsp) = testing::bootstrapped();
    let owner = testing::platform_owner(&db);
    (root, db, dsp, owner)
}
fn saved(db: &Store, dsp: &str, actor: &str, values: Value, revision: i64) -> Result<Saved> {
    save(db, dsp, actor, &POLICY, &values, revision, false)
}

#[test]
fn defaults_fill_what_is_unsaved_and_a_save_changes_only_what_it_sends() {
    let (_root, db, dsp, owner) = northline();
    let defaults = Policy {
        enabled: true,
        mode: "final".into(),
        department: None,
        columns: vec!["in".into(), "out".into()],
        days: None,
        code: String::new(),
        at: "18:00".into(),
        zone: "America/Chicago".into(),
    };
    assert_eq!(read::<Policy>(&db, &dsp, &POLICY).unwrap(), defaults);

    // Columns are kept in their options' order, whatever order they're sent in.
    let first = json!({"days": 90, "columns": ["total", "in"], "code": " nl1 "});
    assert!(matches!(
        saved(&db, &dsp, &owner, first, 0).unwrap(),
        Saved::Saved { revision: 1, .. }
    ));
    saved(&db, &dsp, &owner, json!({"department": "ops"}), 1).unwrap();
    assert_eq!(
        read::<Policy>(&db, &dsp, &POLICY).unwrap(),
        Policy {
            department: Some("ops".into()),
            columns: vec!["in".into(), "total".into()],
            days: Some(90),
            code: "NL1".into(),
            ..defaults
        }
    );
    // Saving what's already there keeps the revision.
    let same = saved(&db, &dsp, &owner, json!({"days": 90}), 2).unwrap();
    assert!(matches!(same, Saved::Saved { revision: 2, .. }));

    // A value an earlier release kept that the field no longer holds reads as its default.
    let record = json!({"revision": 2, "values": {"days": 7, "mode": "draft", "zone": "UTC"}});
    db.dsp(&dsp)
        .unwrap()
        .set("section:stand_in.policy", &record)
        .unwrap();
    let now = read::<Policy>(&db, &dsp, &POLICY).unwrap();
    assert_eq!(
        (now.days, now.mode, now.zone),
        (None, "final".into(), "UTC".into())
    );
}

#[test]
fn a_value_its_field_does_not_hold_is_refused_and_keeps_nothing() {
    let (_root, db, dsp, owner) = northline();
    saved(&db, &dsp, &owner, json!({"code": "NL"}), 0).unwrap();
    for values in [
        json!({"enabled": "yes"}),
        json!({"mode": "draft"}),
        json!({"department": "hr"}),
        json!({"columns": ["in", "in"]}),
        json!({"columns": ["late"]}),
        json!({"days": 29}),
        json!({"days": 3651}),
        json!({"days": 90.5}),
        json!({"code": "NL-2"}),
        json!({"code": "SUM"}),
        json!({"at": "24:00"}),
        json!({"at": "6:00"}),
        json!({"zone": "Mars/Olympus"}),
        json!({"schedules": []}),
        json!({"unknown": true}),
        json!([]),
    ] {
        let refused = saved(&db, &dsp, &owner, values.clone(), 1).unwrap_err();
        assert_eq!(
            (refused.code.as_str(), refused.status),
            ("invalid_input", 400),
            "{values}"
        );
    }
    let stale = saved(&db, &dsp, &owner, json!({"days": 90}), 0).unwrap_err();
    assert_eq!(
        (stale.code.as_str(), stale.status),
        ("settings_changed_reload_before_saving", 409)
    );
    // None of them changed anything, and none is none unless the field may be.
    let pick = saved(
        &db,
        &dsp,
        &owner,
        json!({"department": null, "days": null}),
        1,
    );
    assert!(matches!(pick.unwrap(), Saved::Saved { revision: 1, .. }));
    let now = read::<Policy>(&db, &dsp, &POLICY).unwrap();
    assert_eq!((now.code.as_str(), now.days), ("NL", None));
    let refused = saved(&db, &dsp, &owner, json!({"mode": null}), 1).unwrap_err();
    assert_eq!(refused.code, "invalid_input");
}

static SAVES: std::sync::Mutex<Vec<Value>> = std::sync::Mutex::new(Vec::new());
fn shrinking(_: &Store, _: &str, before: &Value, after: &Value) -> Result<Option<String>> {
    let fewer = after["days"].as_i64() < before["days"].as_i64() && after["days"].is_i64();
    Ok(fewer.then(|| "Older days will be deleted.".to_owned()))
}
fn record(_: &Store, _: &str, values: &Value) -> Result<()> {
    SAVES.lock().unwrap().push(values.clone());
    Ok(())
}

#[test]
fn a_save_its_warning_has_words_for_keeps_nothing_until_confirmed() {
    const WARNED: Section = section("stand_in.warned", "Warned", "stand_in.manage", FIELDS)
        .warn(shrinking)
        .saved(record);
    let (_root, db, dsp, owner) = northline();
    let days = |days: i64| json!({ "days": days });
    let save = |values, revision, confirmed| {
        super::save(&db, &dsp, &owner, &WARNED, &values, revision, confirmed).unwrap()
    };
    assert!(matches!(
        save(days(365), 0, false),
        Saved::Saved { revision: 1, .. }
    ));
    assert_eq!(
        save(days(90), 1, false),
        Saved::Confirm("Older days will be deleted.".into())
    );
    assert_eq!(read::<Policy>(&db, &dsp, &WARNED).unwrap().days, Some(365));
    assert!(matches!(
        save(days(90), 1, true),
        Saved::Saved { revision: 2, .. }
    ));
    // What it does after each save it did with what was saved, and only once saved.
    let saves = SAVES.lock().unwrap();
    assert_eq!(
        saves
            .iter()
            .map(|values| &values["days"])
            .collect::<Vec<_>>(),
        [&json!(365), &json!(90)]
    );
}

#[test]
fn the_log_shows_each_change_as_its_field_does_and_not_a_members_own() {
    const MINE: Section = section("stand_in.mine", "Mine", "access", FIELDS).member();
    let (_root, db, dsp, owner) = northline();
    let values = json!({"enabled": false, "mode": "snapshot", "department": "ops",
        "columns": ["in", "out", "total"], "days": 90, "at": "06:30"});
    saved(&db, &dsp, &owner, values, 0).unwrap();
    saved(&db, &dsp, &owner, json!({"days": 120, "columns": []}), 1).unwrap();
    save(&db, &dsp, &owner, &MINE, &json!({"days": 45}), 0, false).unwrap();

    // The platform's log, which a platform owner's changes are in.
    let events = testing::audits(&db, None).unwrap();
    let changed: Vec<&Value> = events
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["action"] == "settings.changed")
        .collect();
    assert_eq!(changed.len(), 2, "a member's own save isn't logged");
    let change = |event: &Value| {
        let reference = json!({"kind": "setting", "id": "stand_in.policy"});
        assert_eq!(
            (&event["target"], &event["area"], &event["ref"]),
            (&json!("Policy"), &json!("settings"), &reference)
        );
        event["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c["field"].clone(), c["from"].clone(), c["to"].clone()))
            .collect::<Vec<_>>()
    };
    // The newest first.
    assert_eq!(
        change(changed[0]),
        [
            (json!("Columns"), json!("In, Out, Total"), Value::Null),
            (json!("Keep"), json!("90 days"), json!("120 days")),
        ]
    );
    assert_eq!(
        change(changed[1]),
        [
            (json!("Collects"), json!("On"), json!("Off")),
            (json!("Mode"), json!("Final"), json!("Snapshot")),
            (
                json!("Department"),
                json!("Every department"),
                json!("Operations")
            ),
            (json!("Columns"), json!("In, Out"), json!("In, Out, Total")),
            (json!("Keep"), json!("Every day"), json!("90 days")),
            (json!("Collects at"), json!("6:00 PM"), json!("6:30 AM")),
        ]
    );
    // The member's own values are theirs.
    let mine = |member: &str| {
        read_member::<Policy>(&db, &dsp, member, &MINE)
            .unwrap()
            .days
    };
    assert_eq!((mine(&owner), mine("someone_else")), (Some(45), None));
}

/// Keeps a section's values in a setting of its own, as a feature's tables would.
struct Elsewhere;
impl Kept for Elsewhere {
    fn load(&self, db: &Store, dsp: &str) -> Result<Value> {
        db.dsp(dsp)?
            .setting("stand_in.elsewhere", json!({"days": 365}))
    }
    fn keep(&self, db: &Store, dsp: &str, _: &str, values: &Value) -> Result<()> {
        db.dsp(dsp)?.set("stand_in.elsewhere", values)
    }
}

#[test]
fn a_section_kept_elsewhere_reads_and_saves_there_and_core_keeps_its_revision() {
    const KEPT: Section =
        section("stand_in.kept", "Kept", "stand_in.manage", FIELDS).kept(&Elsewhere);
    let (_root, db, dsp, owner) = northline();
    assert_eq!(read::<Policy>(&db, &dsp, &KEPT).unwrap().days, Some(365));
    save(
        &db,
        &dsp,
        &owner,
        &KEPT,
        &json!({"mode": "snapshot"}),
        0,
        false,
    )
    .unwrap();
    let kept = db
        .dsp(&dsp)
        .unwrap()
        .setting("stand_in.elsewhere", Value::Null)
        .unwrap();
    assert_eq!(
        (&kept["days"], &kept["mode"]),
        (&json!(365), &json!("snapshot"))
    );
    let stale = save(&db, &dsp, &owner, &KEPT, &json!({"days": 90}), 0, false).unwrap_err();
    assert_eq!(stale.status, 409);
}

#[test]
fn values_a_feature_kept_itself_are_adopted_once_and_never_over_a_save() {
    let (_root, db, dsp, owner) = northline();
    let adopt = |values: Value| super::adopt(&db, &dsp, &POLICY, &values);
    assert_eq!(
        adopt(json!({"days": 29})).unwrap_err().code,
        "invalid_input"
    );
    assert!(
        !adopt(json!({"days": null, "mode": "final"})).unwrap(),
        "its defaults"
    );
    assert!(adopt(json!({"days": 120})).unwrap());
    assert!(!adopt(json!({"days": 365})).unwrap(), "adopted already");
    assert_eq!(read::<Policy>(&db, &dsp, &POLICY).unwrap().days, Some(120));
    // It's at revision 1, which a save then reads at, and the log has nothing of it.
    saved(&db, &dsp, &owner, json!({"days": 90}), 1).unwrap();
    assert!(!adopt(json!({"days": 365})).unwrap());
    assert_eq!(read::<Policy>(&db, &dsp, &POLICY).unwrap().days, Some(90));
    let logged = testing::audit_actions(&db, &dsp, "settings.");
    assert_eq!(logged, ["settings.changed"]);
}
