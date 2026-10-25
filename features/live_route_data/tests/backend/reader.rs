//! When a live read opens the DSP's browser rather than reading natively: as the ladder answers
//! each read, its native reader failing or not.
use super::*;

/// A browser read the day and handed back a native reader.
fn browsed(ladder: &mut Ladder) {
    ladder.browsing();
    ladder.browsed();
}

#[test]
fn cortex_briefly_unavailable_fails_a_few_reads_in_a_row_before_one_opens_the_browser() {
    let mut ladder = Ladder::default();
    browsed(&mut ladder);
    ladder.read_natively();
    for _ in 0..2 {
        // Each read fails with no browser, until the third in a row opens it.
        for _ in 0..UNAVAILABLE_READS {
            assert!(ladder.unavailable());
        }
        assert!(!ladder.unavailable());
        browsed(&mut ladder);
        // A whole read starts the count again.
        assert!(ladder.unavailable());
        ladder.read_natively();
    }
    // Unavailable is no failure of the reader: the next read goes native.
    assert!(ladder.native_first());
}

#[test]
fn a_reader_failing_fresh_from_a_browser_leaves_the_next_reads_to_the_browser() {
    let mut ladder = Ladder::default();
    // One that read since it came fails: only that read opens the browser.
    browsed(&mut ladder);
    ladder.read_natively();
    ladder.failed();
    browsed(&mut ladder);
    assert!(ladder.native_first());
    // The one that browser handed back fails at once: that read opens the browser, as do the
    // next with no native read first, until native is tried again.
    ladder.failed();
    browsed(&mut ladder);
    let mut browser_reads = 0;
    while !ladder.native_first() {
        browsed(&mut ladder);
        browser_reads += 1;
    }
    assert_eq!(browser_reads, BROWSER_READS - 1);
    // And again, while a fresh one keeps failing.
    ladder.failed();
    browsed(&mut ladder);
    assert!(!ladder.native_first());
}
