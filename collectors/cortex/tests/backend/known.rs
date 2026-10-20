use super::*;

fn asked(station: &str, date: &str, timezone: &str) -> Discovery {
    Discovery {
        date: date.into(),
        station: station.into(),
        timezone: timezone.into(),
        dsp_name: "Northline Logistics".into(),
        dsp_abbreviation: "NLL".into(),
    }
}

// What discovery found answers only its own station, in the DSP's timezone, within the
// week; the API's address only its own company and area; a forgotten station, or a file
// that can't be read, answers nothing.
#[test]
fn kept_scopes_answer_their_station_company_and_week_alone() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), PermissionsExt::from_mode(0o700)).unwrap();
    let known = KnownScopes::beside(&root.path().join("cortex-browseros")).unwrap();
    let la = "America/Los_Angeles";
    let first = asked("TST1", "2026-10-17", la);
    let now = 1_790_000_000_000;
    assert!(known.scope(&first, now).is_none());
    let scope = first.scope("area-1", "company-1").unwrap();
    known.keep(&scope, now).unwrap();
    assert_eq!(known.scope(&first, now), Some(scope.clone()));
    // Another day of the station is the same station's scope, on that day.
    let next = asked("TST1", "2026-10-18", la);
    assert_eq!(
        known.scope(&next, now + 1),
        Some(next.scope("area-1", "company-1").unwrap())
    );
    assert!(known.scope(&asked("TST2", "2026-10-17", la), now).is_none());
    assert!(
        known
            .scope(&asked("TST1", "2026-10-17", "America/Chicago"), now)
            .is_none()
    );
    assert!(known.scope(&first, now + FRESH_MS).is_none());
    assert!(known.scope(&first, now - 1).is_none());

    let api = Api {
        base: "https://logistics.amazon.com/performance/api/v2".into(),
        dsp: "NLL".into(),
    };
    known.keep_api(&scope, api.clone()).unwrap();
    assert_eq!(known.api(&scope), Some(api.clone()));
    // Found again for the same company, the address stays; for another, it goes.
    known.keep(&scope, now + 5).unwrap();
    assert_eq!(known.api(&scope), Some(api.clone()));
    let other = first.scope("area-1", "company-2").unwrap();
    assert!(known.api(&other).is_none());
    known.keep(&other, now + 6).unwrap();
    assert!(known.api(&other).is_none());
    assert!(known.api(&scope).is_none());
    // An address kept for a scope other than the station's is not kept.
    known.keep_api(&scope, api.clone()).unwrap();
    assert!(known.api(&other).is_none());

    known.forget("TST1").unwrap();
    assert!(known.scope(&first, now + 7).is_none());
    std::fs::write(root.path().join(FILE), b"not json").unwrap();
    std::fs::set_permissions(root.path().join(FILE), PermissionsExt::from_mode(0o600)).unwrap();
    assert!(known.scope(&first, now).is_none());
    known.keep(&scope, now).unwrap();
    assert_eq!(known.scope(&first, now), Some(scope));
}
