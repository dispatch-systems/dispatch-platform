//! A member's own preferences: kept a revision at a time, so a save made from an older one never
//! replaces a newer one; read through each upgrade since the version that kept them; and never
//! replaced while this release can't read them.
use dispatch_core::{
    accounts::api::types::{KeptPreference, PreferenceSaved},
    manifest::{Preference, PreferenceValue, preference},
    testing as common,
};
use serde_json::{Value, json};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Layout {
    widgets: Vec<String>,
    #[serde(default)]
    pinned: Vec<String>,
}
impl PreferenceValue for Layout {}
/// Version 1 kept the list alone; version 2 keeps it under `widgets`.
const LAYOUT: Preference =
    preference::<Layout>("stand_in.layout").upgrades(&[|list| Some(json!({ "widgets": list }))]);

const fn saved(revision: i64) -> PreferenceSaved {
    PreferenceSaved::Saved { revision }
}
fn stale(revision: i64, value: Option<Value>) -> PreferenceSaved {
    PreferenceSaved::Stale {
        current: KeptPreference { revision, value },
    }
}

#[test]
fn a_save_made_from_an_older_revision_is_told_what_is_kept_instead_of_replacing_it() {
    common::install(&[], &[]);
    let (_root, db, dsp) = common::bootstrapped();
    let save = |user: &str, revision, value: Option<&Value>| {
        db.save_preference(&dsp, user, &LAYOUT, revision, value)
            .unwrap()
    };
    let kept = |user: &str| db.preferences(&dsp, user, [&LAYOUT]).unwrap();
    // Each is kept as its type writes it, every field written out.
    let (laptop, phone) = (json!({"widgets": ["vans"]}), json!({"widgets": ["dvic"]}));
    let whole = |value: &Value| json!({"widgets": value["widgets"], "pinned": []});
    assert_eq!(save("ana", 0, Some(&laptop)), saved(1));
    // Their phone, open since before, is told what the laptop kept, and makes its change on it.
    assert_eq!(save("ana", 0, Some(&phone)), stale(1, Some(whole(&laptop))));
    assert_eq!(save("ana", 1, Some(&phone)), saved(2));
    // Clearing it from an older revision is refused the same way.
    assert_eq!(save("ana", 1, None), stale(2, Some(whole(&phone))));
    // Another member's are their own.
    assert_eq!(save("ben", 0, Some(&laptop)), saved(1));
    assert_eq!(
        kept("ana")[LAYOUT.id],
        KeptPreference {
            revision: 2,
            value: Some(whole(&phone))
        }
    );
    // Cleared, it's back to its default, which is never kept, at a revision of its own: a save
    // made from one before is still told what is kept.
    assert_eq!(save("ana", 2, None), saved(3));
    assert_eq!(
        kept("ana")[LAYOUT.id],
        KeptPreference {
            revision: 3,
            value: None
        }
    );
    assert_eq!(save("ana", 2, Some(&laptop)), stale(3, None));
    assert_eq!(kept("ben")[LAYOUT.id].revision, 1);
    // Only a value of its shape is kept.
    for refused in [
        json!({"widgets": "vans"}),
        json!({"widgets": [], "hidden": []}),
    ] {
        let error = db
            .save_preference(&dsp, "ana", &LAYOUT, 3, Some(&refused))
            .unwrap_err();
        assert_eq!((error.code.as_str(), error.status), ("invalid_input", 400));
    }
    assert_eq!(kept("ana")[LAYOUT.id].value, None);
}

#[test]
fn an_earlier_version_reads_upgraded_and_one_this_release_cant_read_is_only_cleared() {
    common::install(&[], &[]);
    let (_root, db, dsp) = common::bootstrapped();
    let keep = |user: &str, version: u32, revision: i64, value: Value| {
        db.dsp(&dsp)
            .unwrap()
            .exec(
                "INSERT INTO member_preferences(user_id,id,version,revision,value,saved_at) \
                 VALUES (?,?,?,?,?,'')",
                rusqlite::params![user, LAYOUT.id, version, revision, value.to_string()],
            )
            .unwrap();
    };
    let kept = |user: &str| db.preferences(&dsp, user, [&LAYOUT]).unwrap()[LAYOUT.id].clone();
    keep("ana", 1, 3, json!(["vans"]));
    assert_eq!(
        kept("ana").value,
        Some(json!({"widgets": ["vans"], "pinned": []}))
    );
    // A newer release kept this one, so this release reads none and replaces nothing.
    let newer = json!({"widgets": ["vans"], "pinned": ["vans"]});
    keep("ben", 3, 4, newer.clone());
    assert_eq!(
        kept("ben"),
        KeptPreference {
            revision: 4,
            value: None
        }
    );
    let error = db
        .save_preference(&dsp, "ben", &LAYOUT, 4, Some(&json!({"widgets": []})))
        .unwrap_err();
    assert_eq!(
        (error.code.as_str(), error.status),
        ("preference_unreadable", 409)
    );
    let stored = || {
        db.dsp(&dsp)
            .unwrap()
            .one_as::<(Option<String>,)>(
                "SELECT value FROM member_preferences WHERE user_id='ben'",
                [],
            )
            .unwrap()
            .unwrap()
            .0
    };
    assert_eq!(stored(), Some(newer.to_string()));
    // Clearing it is the member's own choice.
    assert_eq!(
        db.save_preference(&dsp, "ben", &LAYOUT, 4, None).unwrap(),
        saved(5)
    );
    assert_eq!(stored(), None);
}
