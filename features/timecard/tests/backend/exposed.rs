//! What Timecard exposes, as a feature using it finds it: only while it's on for the DSP.
use crate::TimecardStore;
use dispatch_core::{db::Store, testing};
use dispatch_paycom::fixtures;
use dispatch_timecard_contract::{
    self as timecards, Collected, Day,
    PunchKind::{self, InDay, InLunch, OutDay, OutLunch},
    Timecards,
};
use serde_json::json;

/// A new DSP with Timecard on, and what it offers there.
fn offered() -> (tempfile::TempDir, Store, String, &'static dyn Timecards) {
    testing::install(
        &[&dispatch_cortex::COLLECTOR, &dispatch_paycom::COLLECTOR],
        &[&crate::FEATURE],
    );
    let (root, db, dsp) = testing::bootstrapped();
    let offered = timecards::for_dsp(&db, &dsp)
        .unwrap()
        .expect("Timecard is on");
    (root, db, dsp, offered)
}

fn ids(codes: &[&str]) -> Vec<String> {
    codes.iter().map(|code| (*code).to_owned()).collect()
}

/// A day's punches, each by what it says and its clock.
fn punches(day: &Day) -> Vec<(PunchKind, Option<&str>)> {
    day.punches
        .iter()
        .map(|punch| (punch.kind, punch.time.as_deref()))
        .collect()
}

#[test]
fn timecards_read_a_drivers_days_by_their_ids_telling_unknown_and_partial_days_apart() {
    let (_root, db, dsp, offered) = offered();
    assert_eq!(offered.collected(&db, &dsp).unwrap(), None);
    // The period of Sep 6-19, collected on the 12th: its days after then are still to come,
    // and E002 hadn't punched in on the 12th by then.
    let mut workforce = fixtures::fixture_date("UTC", Some("2026-09-12".parse().unwrap())).unwrap();
    workforce["collectedAt"] = json!("2026-09-12T18:00:00Z");
    for card in workforce["timecards"].as_array_mut().unwrap() {
        match (card["employeeCode"].as_str(), card["date"].as_str()) {
            (Some("E002"), Some("2026-09-07" | "2026-09-12")) => {
                card["hours"] = json!(0);
                card["status"] = json!("No punches");
                card["punches"] = json!([]);
            }
            (Some("E002"), Some("2026-09-08")) => {
                card["punches"] = json!([
                    {"in":"07:58","inKind":"IN DAY","out":"12:00","outKind":"OUT LUNCH","hours":4.03},
                    {"in":"12:20","inKind":"IN LUNCH","out":null,"outKind":"OUT DAY","hours":null},
                ]);
                card["status"] = json!("Missing punch");
            }
            _ => {}
        }
    }
    db.publish_timecards(&dsp, &workforce).unwrap();
    // Then E003's own sync of the period, on the 15th, punched on every day of it.
    let mut sync = workforce.clone();
    sync["employees"]
        .as_array_mut()
        .unwrap()
        .retain(|employee| employee["code"] == "E003");
    let cards = sync["timecards"].as_array_mut().unwrap();
    cards.retain(|card| card["employeeCode"] == "E003");
    let worked = cards.last().unwrap().clone();
    cards.extend((13..=19).map(|day| {
        let mut card = worked.clone();
        card["date"] = json!(format!("2026-09-{day}"));
        card
    }));
    db.collector(&dsp, dispatch_paycom::PROVIDER)
        .unwrap()
        .exec(
            "INSERT INTO employee_timecard_syncs VALUES ('E003','2026-09-06','2026-09-19',?,?)",
            rusqlite::params!["2026-09-15T12:00:00Z", sync.to_string()],
        )
        .unwrap();
    // One employee's sync speaks for them alone.
    assert_eq!(
        offered.collected(&db, &dsp).unwrap(),
        Some(Collected {
            from: "2026-09-06".into(),
            through: "2026-09-12".into(),
        })
    );

    let read = offered
        .timecards(&db, &dsp, &ids(&["E002"]), "2026-09-04", "2026-09-14")
        .unwrap();
    // Before the period and after its collection are unknown; the 12th, collected under way,
    // partial; the 7th, with nothing, a day off.
    assert_eq!(
        read.unknown,
        ["2026-09-04", "2026-09-05", "2026-09-13", "2026-09-14"]
    );
    assert_eq!(read.partial, ["2026-09-12"]);
    let dates: Vec<_> = read.days.iter().map(|day| day.date.as_str()).collect();
    assert_eq!(
        dates,
        [
            "2026-09-06",
            "2026-09-08",
            "2026-09-09",
            "2026-09-10",
            "2026-09-11"
        ]
    );
    let worked = &read.days[0];
    assert_eq!(
        (
            worked.status.as_str(),
            worked.hours,
            worked.worked_minutes,
            worked.review
        ),
        ("Complete", 8.0, Some(480), false)
    );
    assert_eq!(
        punches(worked),
        [
            (InDay, Some("08:00")),
            (OutLunch, Some("12:00")),
            (InLunch, Some("12:30")),
            (OutDay, Some("16:30"))
        ]
    );
    let meal = &worked.meals[0];
    assert_eq!(
        (meal.out.as_deref(), meal.back.as_deref(), meal.minutes),
        (Some("12:00"), Some("12:30"), Some(30))
    );
    // No punch out for the day: its punches end on the way back from the meal, which is known,
    // and the time worked isn't.
    let missing = &read.days[1];
    assert_eq!(
        (
            missing.status.as_str(),
            missing.worked_minutes,
            missing.meals[0].minutes
        ),
        ("Missing punch", None, Some(20))
    );
    assert_eq!(
        punches(missing),
        [
            (InDay, Some("07:58")),
            (OutLunch, Some("12:00")),
            (InLunch, Some("12:20"))
        ]
    );

    // E003's sync decides their days: the 15th, collected under way, still reads its punches.
    let synced = offered
        .timecards(&db, &dsp, &ids(&["E003"]), "2026-09-04", "2026-09-16")
        .unwrap();
    assert_eq!(synced.unknown, ["2026-09-04", "2026-09-05", "2026-09-16"]);
    assert_eq!(synced.partial, ["2026-09-15"]);
    assert_eq!(
        synced.days.last().map(|day| day.date.as_str()),
        Some("2026-09-15")
    );

    // Both of a driver's IDs, by day and then ID; an ID nobody holds, or none, reads nothing.
    let both = offered
        .timecards(
            &db,
            &dsp,
            &ids(&["E003", "E002"]),
            "2026-09-06",
            "2026-09-06",
        )
        .unwrap();
    let held: Vec<_> = both.days.iter().map(|day| day.id.as_str()).collect();
    assert_eq!(held, ["E002", "E003"]);
    for asked in [ids(&["E999"]), vec![]] {
        let nobody = offered
            .timecards(&db, &dsp, &asked, "2026-09-04", "2026-09-14")
            .unwrap();
        assert!(nobody.days.is_empty());
        assert_eq!(
            (&nobody.unknown, &nobody.partial),
            (&read.unknown, &read.partial)
        );
    }
}

#[test]
fn timecards_refuse_bad_input_and_are_offered_only_while_timecard_is_on() {
    let (_root, db, dsp, offered) = offered();
    let driver = ids(&["E002"]);
    let many: Vec<_> = (1..=17).map(|n| format!("E{n:03}")).collect();
    for (asked, from, to, code) in [
        (&driver, "2026-9-01", "2026-09-02", "invalid_date"),
        (&driver, "2026-09-01", "2026-10-02", "invalid_input"),
        (&many, "2026-09-01", "2026-09-02", "invalid_input"),
    ] {
        let refused = offered.timecards(&db, &dsp, asked, from, to).unwrap_err();
        assert_eq!(refused.code, code, "{asked:?} {from} {to}");
    }
    // A month of 31 days, and 16 IDs, are read.
    assert!(
        offered
            .timecards(&db, &dsp, &many[..16], "2026-09-01", "2026-10-01")
            .is_ok()
    );
    let owner = testing::a_user(&db);
    db.set_feature(&dsp, "timecard", false, &owner).unwrap();
    assert!(timecards::for_dsp(&db, &dsp).unwrap().is_none());
}
