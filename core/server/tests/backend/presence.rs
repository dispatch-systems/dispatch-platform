use super::*;

#[test]
fn most_active_dashboard_wins_until_beats_stop() {
    let presence = Presence::default();
    let start = Instant::now();
    assert_eq!(presence.status_at("dsp", "user", start), "offline");
    presence.beat_at("dsp", "user", "one", Some(false), start);
    assert_eq!(presence.status_at("dsp", "user", start), "idle");
    presence.beat_at("dsp", "user", "two", Some(true), start);
    assert_eq!(presence.status_at("dsp", "user", start), "active");
    assert_eq!(presence.status_at("other", "user", start), "offline");
    presence.beat_at("dsp", "user", "two", None, start);
    assert_eq!(presence.status_at("dsp", "user", start), "idle");
    let later = start + FRESH;
    assert_eq!(presence.status_at("dsp", "user", later), "offline");
    presence.beat_at("dsp", "else", "one", Some(true), later);
    assert!(!presence.0.lock().unwrap()["dsp"].contains_key("user"));
}

#[test]
fn one_member_cannot_hold_unbounded_tabs() {
    let presence = Presence::default();
    let now = Instant::now();
    for tab in 0..TABS + 4 {
        presence.beat_at("dsp", "user", &tab.to_string(), Some(true), now);
    }
    assert_eq!(presence.0.lock().unwrap()["dsp"]["user"].len(), TABS);
}
