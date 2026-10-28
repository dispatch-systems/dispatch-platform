//! An itinerary's work phone as shaping takes it from a read: the day's list's record of its driver,
//! else the itinerary's own, by its last ten digits; and the one it keeps when a read shows none,
//! of two stores of its day merged.
use super::*;
use dispatch_cortex::routes::{Collection, Request, fixture};

/// The fixture's day: two drivers, the list and each itinerary showing each on a phone of their
/// own, 206-555-0190 and 206-555-0191.
fn day() -> Capture {
    let request = Request {
        collection: Collection::Routes,
        mode: Mode::Final,
        date: "2026-09-25".into(),
        station: "TST1".into(),
        timezone: "America/Los_Angeles".into(),
        dsp_name: "Northline Logistics".into(),
        dsp_abbreviation: "NLOG".into(),
        service_area_id: None,
        provider: None,
    };
    fixture(&request).unwrap()
}
/// Each itinerary's work phone in `read`'s rows, as shaped.
fn phones(read: &Capture) -> Vec<Option<String>> {
    let shaper = Shaper::new(read);
    let shaped = read.itineraries.iter().map(|i| shaper.shape(i).unwrap());
    shaped
        .map(|itinerary| {
            let (_, phone) = itinerary.columns.iter().find(|(c, _)| c == "work_phone")?;
            phone.as_str().map(str::to_owned)
        })
        .collect()
}
/// `read`'s `n`th itinerary's response with its driver's record showing `phone`, or none.
fn own(read: &mut Capture, n: usize, phone: Option<&str>) {
    let mut response: Value = serde_json::from_str(&read.itineraries[n].detail).unwrap();
    response["transporters"][0]["workPhoneNumber"] = json!(phone);
    read.itineraries[n].detail = response.to_string();
}

#[test]
fn an_itinerarys_work_phone_is_its_drivers_on_the_days_list_else_its_own() {
    let shown = |phone: &str| Some(phone.to_owned());
    assert_eq!(phones(&day()), [shown("2065550190"), shown("2065550191")]);

    // The list's, as written however, before the itinerary's own.
    let mut read = day();
    read.summaries["transporters"][0]["workPhoneNumber"] = json!("+1 (206) 555-0192");
    own(&mut read, 0, Some("2065550193"));
    // A record on the list without a phone, or with too few digits, gives way to the itinerary's.
    let listed = read.summaries["transporters"][1].as_object_mut().unwrap();
    listed.remove("workPhoneNumber");
    own(&mut read, 1, Some("12065550194"));
    assert_eq!(phones(&read), [shown("2065550192"), shown("2065550194")]);
    read.summaries["transporters"][1]["workPhoneNumber"] = json!("555-0195");
    assert_eq!(phones(&read)[1], shown("2065550194"));

    // A driver the list doesn't name has the itinerary's; one neither shows a phone for, none.
    read.summaries["transporters"] = json!([]);
    own(&mut read, 1, None);
    assert_eq!(phones(&read), [shown("2065550193"), None]);
}

#[test]
fn of_two_stores_merged_a_phone_the_newer_gives_a_driver_is_kept_by_no_other_driver() {
    let (first, second) = ("2065550190", "2065550191");
    let stored = |itineraries: &[(&str, &str, &str)]| {
        let stored = itineraries.iter().map(|(itinerary, driver, phone)| {
            let phone = Some((*phone).to_owned());
            ((*itinerary).to_owned(), ((*driver).to_owned(), phone))
        });
        Stored(stored.collect())
    };
    // The newer store gives the first phone to the second driver. The older gives it to an
    // itinerary of the first driver's and another of the second's, neither of which the newer
    // holds, and the second phone to a third driver.
    let newer = stored(&[("itinerary-2", "driver-2", first)]);
    let older = stored(&[
        ("itinerary-1", "driver-1", first),
        ("itinerary-3", "driver-2", first),
        ("itinerary-4", "driver-3", second),
    ]);
    // A read showing no phone keeps the first phone on the second driver's itineraries alone.
    let read = [
        ("itinerary-1", "driver-1", None),
        ("itinerary-2", "driver-2", None),
        ("itinerary-3", "driver-2", None),
        ("itinerary-4", "driver-3", None),
    ];
    let kept = newer.or(older).kept(&read);
    let want = [
        ("itinerary-2", first),
        ("itinerary-3", first),
        ("itinerary-4", second),
    ];
    let want = want.map(|(itinerary, phone)| (itinerary.to_owned(), phone.to_owned()));
    assert_eq!(kept, BTreeMap::from(want));
}
