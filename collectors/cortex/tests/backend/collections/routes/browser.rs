use super::*;
use crate::routes::{Collection, Mode};

fn config(fixture: bool, fixture_url: Option<&str>) -> Config {
    let mut config = Config::load().unwrap();
    config.fixture = fixture;
    config.fixture_url = fixture_url.map(str::to_owned);
    config
}
/// Today's routes at a station, its service area and provider known or not.
fn request(known: bool) -> Request {
    Request {
        collection: Collection::Routes,
        mode: Mode::Snapshot,
        date: "2026-09-25".into(),
        station: "TST1".into(),
        timezone: "America/Los_Angeles".into(),
        dsp_name: "Fixture Logistics".into(),
        dsp_abbreviation: "FXTR".into(),
        service_area_id: known.then(|| "area-1".into()),
        provider: known.then(|| "company-1".into()),
    }
}

#[tokio::test]
async fn fixture_mode_reads_a_whole_day_in_the_scope_it_resolves() {
    let config = config(true, None);
    for (known, area, provider) in [
        (true, "area-1", "company-1"),
        (false, "area-demo", "provider-demo"),
    ] {
        let request = request(known);
        let mut browser = Browser::new(None, &config).unwrap();
        let scope = browser.scope(&request).await.unwrap();
        assert_eq!(
            (scope.service_area_id.as_str(), scope.provider.as_str()),
            (area, provider)
        );
        let day = browser.read(&request, &scope).await.unwrap();
        day.capture.validate(&request).unwrap();
        assert_eq!(day.capture.scope, scope);
        assert_eq!(day.capture.itineraries.len(), 2);
        // Nothing read it but the fixture, and there is no session to read the next with.
        assert!(day.method.is_none() && day.fallback.is_none() && day.native.is_none());
    }
}

#[tokio::test]
async fn a_scope_kept_for_another_request_is_refused() {
    let config = config(true, None);
    // A scope kept for `known`'s request with one thing changed, as a browser's read refuses it.
    let refused = async |known: bool, change: fn(&mut Scope)| {
        let request = request(known);
        let mut kept = Browser::new(None, &config)
            .unwrap()
            .scope(&request)
            .await
            .unwrap();
        change(&mut kept);
        let read = Browser::new(None, &config).unwrap().read(&request, &kept);
        read.await.err().map(|error| error.code)
    };
    let changes: [(bool, fn(&mut Scope)); 7] = [
        (false, |scope| scope.date = "2026-09-24".into()),
        // Each part of the scope a request names.
        (true, |scope| scope.date = "2026-09-24".into()),
        (true, |scope| scope.provider = "company-2".into()),
        (true, |scope| scope.service_area_id = "area-2".into()),
        (true, |scope| scope.station = "TST2".into()),
        (true, |scope| scope.timezone = "America/Denver".into()),
        // Every driver is never a DSP's own provider.
        (false, |scope| scope.provider = "ALL_DRIVERS".into()),
    ];
    for (index, (known, change)) in changes.into_iter().enumerate() {
        assert_eq!(
            refused(known, change).await.as_deref(),
            Some("cortex_scope_mismatch"),
            "change {index}"
        );
    }
}

#[test]
fn without_a_browser_only_fixture_mode_reads() {
    // A real server, and one pointing its browsers at a stand-in, never read the fixture's day.
    for config in [
        config(false, None),
        config(true, Some("http://fixture.dispatch.invalid:9")),
    ] {
        let refused = Browser::new(None, &config).err().unwrap();
        assert_eq!(refused.code, "browser_unavailable");
    }
}
