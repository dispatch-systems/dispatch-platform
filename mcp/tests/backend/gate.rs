use super::*;

#[test]
fn an_address_waits_a_minute_after_twenty_refusals_and_quiet_ones_make_room() {
    let gate = Gate::default();
    let at = 1_000_000;
    for i in 0..REFUSALS as i64 {
        gate.admit_at("first", at + i).unwrap();
        gate.refused_at("first", at + i);
    }
    let waiting = gate.admit_at("first", at + 100).unwrap_err();
    assert_eq!(
        (waiting.code.as_str(), waiting.status),
        ("rate_limited", 429)
    );
    // Another address is its own count, and the first is let through once its oldest
    // refusal is a minute old.
    gate.admit_at("second", at + 100).unwrap();
    gate.admit_at("first", at + WINDOW + 1).unwrap();
    // Past the bound, only an address quiet for a minute makes room for a new one.
    let gate = Gate::default();
    for n in 0..MOST_SOURCES {
        gate.refused_at(&format!("source-{n}"), at);
    }
    gate.refused_at("late", at + 1);
    assert!(!gate.sources().contains_key("late"));
    gate.refused_at("late", at + WINDOW + 1);
    assert!(gate.sources().contains_key("late"));
    assert!(gate.sources().len() <= MOST_SOURCES);
}
