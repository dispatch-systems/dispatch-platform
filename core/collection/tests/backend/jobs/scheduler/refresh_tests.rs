use super::*;
#[test]
fn a_failed_deadline_read_waits_before_trying_again() {
    let mut refresh = Refresh::new();
    assert!(refresh.due(7, 0), "the first tick reads");
    refresh.failed(0);
    assert!(!refresh.due(7, 1_000), "not again on the next tick");
    assert!(
        !refresh.due(8, 1_000),
        "nor when a schedule changes meanwhile"
    );
    assert!(refresh.due(7, 5_000));
    refresh.read(7, 5_000);
    assert!(!refresh.due(7, 6_000));
    assert!(refresh.due(8, 6_000), "a schedule change reads at once");
    assert!(refresh.due(7, 65_000), "and a minute after the last read");
}
