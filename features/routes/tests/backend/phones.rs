//! The work phone rules on made-up drivers and phones, each settled read by read into a DSP's
//! live route data: who claims a phone and when, how it passes on and ends, and the holds kept.
use super::*;
use crate::{backend::RoutesStore, testing};

const DAY: &str = "2026-09-25";
const P1: &str = "5550000001";
const P2: &str = "5550000002";
const P3: &str = "5550000003";
const P4: &str = "5550000004";
const P5: &str = "5550000005";

/// A DSP whose live route data holds no phones yet.
fn live() -> (tempfile::TempDir, Store, String) {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    testing::bootstrapped()
}
/// A driver the list lists, showing `phone`.
fn seen(driver: &str, phone: &str, ended: bool, last_event: i64) -> Listed {
    Listed {
        driver: driver.into(),
        phone: Some(phone.into()),
        ended,
        last_event,
    }
}
/// Settles a read at `at` listing `listed`, whose list names each with the phone it shows,
/// answering how many holds it claimed, moved or ended.
fn read(db: &Store, dsp: &str, at: i64, listed: Vec<Listed>) -> u64 {
    let named = listed
        .iter()
        .filter_map(|l| Some((l.driver.clone(), l.phone.clone()?)))
        .collect();
    let shown = Shown {
        listed,
        named,
        itineraries: BTreeMap::new(),
    };
    let live = db.routes_live_db(dsp).unwrap();
    live.transaction(|| settle(&live, DAY, at, &shown))
        .unwrap()
        .events
}
/// Who holds `phone` now.
fn holders(db: &Store, dsp: &str, phone: &str) -> Vec<String> {
    let live = db.routes_live_db(dsp).unwrap();
    let rows = live
        .all(
            "SELECT transporter_id FROM work_phones WHERE day=? AND phone=? AND until IS NULL \
             ORDER BY transporter_id",
            [DAY, phone],
        )
        .unwrap();
    rows.iter()
        .map(|row| s(row, "transporter_id").to_owned())
        .collect()
}
/// Every hold `driver` had today, as (phone, since, until).
fn holds(db: &Store, dsp: &str, driver: &str) -> Vec<(String, i64, Option<i64>)> {
    let live = db.routes_live_db(dsp).unwrap();
    let rows = live
        .all(
            "SELECT phone,since,until FROM work_phones WHERE day=? AND transporter_id=? ORDER BY since",
            [DAY, driver],
        )
        .unwrap();
    rows.iter()
        .map(|row| {
            let since = row["since"].as_i64().unwrap();
            (s(row, "phone").to_owned(), since, row["until"].as_i64())
        })
        .collect()
}

#[test]
fn a_shift_that_ends_and_resumes_on_the_same_phone_claims_it_again() {
    let (_root, db, dsp) = live();
    assert_eq!(read(&db, &dsp, 1, vec![seen("a", P1, false, 1)]), 1);
    assert_eq!(read(&db, &dsp, 2, vec![seen("a", P1, true, 2)]), 1);
    assert!(holders(&db, &dsp, P1).is_empty());
    assert_eq!(read(&db, &dsp, 3, vec![seen("a", P1, false, 3)]), 1);
    // A session ends its hold though its record shows no phone as it does.
    let ended = Listed {
        phone: None,
        ..seen("a", P1, true, 4)
    };
    assert_eq!(read(&db, &dsp, 4, vec![ended]), 1);
    assert_eq!(
        holds(&db, &dsp, "a"),
        [(P1.into(), 1, Some(2)), (P1.into(), 3, Some(4))]
    );
}

#[test]
fn a_handed_over_holders_record_never_takes_the_phone_back() {
    let (_root, db, dsp) = live();
    read(&db, &dsp, 1, vec![seen("b", P2, false, 1)]);
    // The claim, and the handover it is.
    let both = |b_event, b_ended, c_event| {
        vec![
            seen("b", P2, b_ended, b_event),
            seen("c", P2, false, c_event),
        ]
    };
    assert_eq!(read(&db, &dsp, 2, both(1, false, 2)), 2);
    assert_eq!(read(&db, &dsp, 3, both(3, false, 2)), 0);
    // A hold handed over doesn't end again with its shift, nor come back with the next one.
    assert_eq!(read(&db, &dsp, 4, both(4, true, 4)), 0);
    assert_eq!(read(&db, &dsp, 5, both(5, false, 4)), 0);
    assert_eq!(holders(&db, &dsp, P2), ["c"]);
    assert_eq!(holds(&db, &dsp, "b"), [(P2.into(), 1, Some(2))]);
}

#[test]
fn a_driver_gone_from_the_list_ends_their_hold_and_a_list_naming_nobody_ends_none() {
    let (_root, db, dsp) = live();
    read(
        &db,
        &dsp,
        1,
        vec![seen("d", P3, false, 1), seen("e", P4, false, 1)],
    );
    assert_eq!(read(&db, &dsp, 2, vec![seen("e", P4, false, 2)]), 1);
    assert!(holders(&db, &dsp, P3).is_empty());
    assert_eq!(read(&db, &dsp, 3, vec![]), 0);
    assert_eq!(holders(&db, &dsp, P4), ["e"]);
    // Back on the list, they claim it again.
    let back = vec![seen("d", P3, false, 4), seen("e", P4, false, 4)];
    assert_eq!(read(&db, &dsp, 4, back), 1);
    assert_eq!(holders(&db, &dsp, P3), ["d"]);
}

#[test]
fn a_driver_claims_a_phone_only_once_signed_in() {
    let (_root, db, dsp) = live();
    // Without a driver event, a record shows the last phone its driver had.
    let first = vec![seen("i", P1, false, 0), seen("j", P1, false, 1)];
    read(&db, &dsp, 1, first);
    assert_eq!(holders(&db, &dsp, P1), ["j"]);
    read(
        &db,
        &dsp,
        2,
        vec![seen("i", P2, false, 2), seen("j", P1, false, 2)],
    );
    assert_eq!(
        (holders(&db, &dsp, P1), holders(&db, &dsp, P2)),
        (vec!["j".to_owned()], vec!["i".to_owned()])
    );
    // A claim made before signing in is withdrawn, never having been a hold.
    read(&db, &dsp, 3, vec![seen("k", P3, false, 3)]);
    assert_eq!(read(&db, &dsp, 4, vec![seen("k", P3, false, 0)]), 1);
    assert!(holds(&db, &dsp, "k").is_empty());
    // A record changed from the first phone it showed is signed in, event or not.
    read(&db, &dsp, 5, vec![seen("m", P4, false, 0)]);
    read(&db, &dsp, 6, vec![seen("m", P5, false, 0)]);
    assert_eq!(holders(&db, &dsp, P5), ["m"]);
    assert!(holders(&db, &dsp, P4).is_empty());
}

#[test]
fn a_switch_to_another_phone_holds_once_two_reads_in_a_row_show_it() {
    let (_root, db, dsp) = live();
    read(&db, &dsp, 1, vec![seen("n", P1, false, 1)]);
    // A record flipping between two phones moves none.
    for (at, phone) in [(2, P5), (3, P1), (4, P5)] {
        assert_eq!(read(&db, &dsp, at, vec![seen("n", phone, false, at)]), 0);
    }
    assert_eq!(holders(&db, &dsp, P1), ["n"]);
    assert_eq!(read(&db, &dsp, 5, vec![seen("n", P5, false, 5)]), 1);
    assert_eq!(
        holds(&db, &dsp, "n"),
        [(P1.into(), 1, Some(5)), (P5.into(), 5, None)]
    );
}

#[test]
fn claims_are_settled_oldest_activity_first() {
    let (_root, db, dsp) = live();
    // Two records showing one phone: the driver active more recently holds it, whatever the
    // list's order.
    read(
        &db,
        &dsp,
        1,
        vec![seen("q", P1, false, 9), seen("r", P1, false, 5)],
    );
    assert_eq!(holders(&db, &dsp, P1), ["q"]);
    assert_eq!(holds(&db, &dsp, "r"), [(P1.into(), 1, Some(1))]);
}

#[test]
fn a_read_shows_each_driver_once_with_their_phones_by_their_digits() {
    let summaries = json!({"itinerarySummaries":[
            {"itineraryId":"it-1","transporterId":"f","driverSessionEnded":true,"lastDriverEventTime":100},
            {"itineraryId":"it-2","transporterId":"f","driverSessionEnded":false,"lastDriverEventTime":50},
            {"itineraryId":"it-3","transporterId":"g","driverSessionEnded":false,"lastDriverEventTime":20},
            {"itineraryId":"it-4","transporterId":"g","driverSessionEnded":false,"lastDriverEventTime":10},
            {"itineraryId":"it-5","transporterId":"h"}],
        "transporters":[{"transporterId":"f","workPhoneNumber":"+1 555 000 0005"},
            {"transporterId":"g","workPhoneNumber":"5550000006"},
            {"transporterId":"assigned","workPhoneNumber":"(555) 000-0007"},
            {"transporterId":"short","workPhoneNumber":"555-0008"},
            {"transporterId":"not an id","workPhoneNumber":"5550000009"}]});
    let listed: Vec<(String, String)> = [("it-1", "f"), ("it-3", "g"), ("it-5", "h")]
        .map(|(id, driver)| (id.into(), driver.into()))
        .into();
    let shown = Shown::read(&summaries, &listed, [("h".into(), P1.into())]);
    let entries: Vec<_> = shown
        .listed
        .iter()
        .map(|l| (l.driver.as_str(), l.phone.as_deref(), l.ended, l.last_event))
        .collect();
    assert_eq!(
        entries,
        [
            ("f", Some(P5), false, 50),
            ("g", Some("5550000006"), false, 20),
            ("h", None, false, 0)
        ]
    );
    let named: Vec<_> = shown
        .named
        .iter()
        .map(|(d, p)| (d.as_str(), p.as_str()))
        .collect();
    assert_eq!(
        named,
        [("assigned", "5550000007"), ("f", P5), ("g", "5550000006")]
    );
    assert_eq!(shown.itineraries.get("h").map(String::as_str), Some(P1));
}
