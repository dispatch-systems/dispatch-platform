//! An itinerary's work phone as shaping takes it from a read: the day's list's record of its driver,
//! else the itinerary's own, by its last ten digits.
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
