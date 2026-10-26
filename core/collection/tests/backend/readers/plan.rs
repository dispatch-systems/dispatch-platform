//! When a DSP reads: its window's instants on each day, across midnight and clock changes; the
//! days it opens on; its grid from the window's start; and what it does next, kept across
//! restarts.
use super::*;
use chrono::Weekday::{self, *};

const CHICAGO: Tz = chrono_tz::America::Chicago;
const MINUTES: i64 = 60_000;
const EVERY_DAY: &[Weekday] = &[Mon, Tue, Wed, Thu, Fri, Sat, Sun];
const FIVE_MINUTES: std::time::Duration = std::time::Duration::from_secs(300);

fn date(text: &str) -> NaiveDate {
    text.parse().unwrap()
}
/// The instant of a local time in Chicago, read as the clock there shows it.
fn local(text: &str) -> i64 {
    let at = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").unwrap();
    CHICAGO
        .from_local_datetime(&at)
        .earliest()
        .unwrap()
        .timestamp_millis()
}
/// Reading on `days` every `every` from `start` until `stop` in Chicago.
fn reading(days: &[Weekday], start: &str, stop: &str, every: std::time::Duration) -> Plan {
    Plan {
        schedule: Schedule::new(days, start, stop, every).unwrap(),
        timezone: CHICAGO,
    }
}
/// Reading every day, every five minutes from `start` until `stop` in Chicago.
fn plan(start: &str, stop: &str) -> Plan {
    reading(EVERY_DAY, start, stop, FIVE_MINUTES)
}

#[test]
fn a_window_closes_the_same_day_the_next_or_a_whole_day_on() {
    let day = plan("08:00", "21:00").day(date("2026-10-20"));
    assert_eq!(
        (day.open, day.close),
        (local("2026-10-20 08:00:00"), local("2026-10-20 21:00:00"))
    );
    let overnight = plan("20:00", "02:00");
    let day = overnight.day(date("2026-10-20"));
    assert_eq!(day.close, local("2026-10-21 02:00:00"));
    // After midnight, the window open is still the one that opened the evening before.
    assert_eq!(overnight.open_at(local("2026-10-21 01:00:00")), Some(day));
    assert_eq!(overnight.open_at(local("2026-10-21 03:00:00")), None);
    assert_eq!(
        overnight.next(local("2026-10-21 03:00:00")).open,
        local("2026-10-21 20:00:00")
    );
    let all_day = plan("06:00", "06:00").day(date("2026-10-20"));
    assert_eq!(all_day.close, local("2026-10-21 06:00:00"));
}

#[test]
fn a_window_opens_as_the_clock_shows_its_times_across_a_change() {
    // Clocks skip from 2:00 to 3:00: a start at 2:30 opens as they resume.
    let skipped = plan("02:30", "21:00").day(date("2026-03-08"));
    assert_eq!(skipped.open, local("2026-03-08 03:00:00"));
    // Clocks repeat 1:00 to 2:00: a start at 1:30 opens at the first.
    let repeated = plan("01:30", "21:00").day(date("2026-11-01"));
    assert_eq!(repeated.open, local("2026-11-01 01:30:00"));
    // An all-day window spans the day as the clock does: 23 hours, or 25.
    let spring = plan("00:00", "00:00").day(date("2026-03-08"));
    assert_eq!(spring.close - spring.open, 23 * 60 * MINUTES);
    let autumn = plan("00:00", "00:00").day(date("2026-11-01"));
    assert_eq!(autumn.close - autumn.open, 25 * 60 * MINUTES);
}

#[test]
fn reads_fall_on_a_grid_of_its_interval_from_the_window_start() {
    let plan = plan("08:00", "21:00");
    let day = plan.day(date("2026-10-20"));
    let first = local("2026-10-20 08:00:00");
    assert_eq!(plan.slot_after(&day, i64::MIN), first);
    assert_eq!(plan.slot_after(&day, first - 1), first);
    assert_eq!(plan.slot_after(&day, first), first + 5 * MINUTES);
    assert_eq!(
        plan.slot_after(&day, local("2026-10-20 12:04:00")),
        local("2026-10-20 12:05:00")
    );
    let quarterly = reading(
        EVERY_DAY,
        "08:00",
        "21:00",
        std::time::Duration::from_secs(15 * 60),
    );
    assert_eq!(
        quarterly.slot_after(&day, local("2026-10-20 12:04:00")),
        local("2026-10-20 12:15:00")
    );
}

#[test]
fn a_window_opens_only_on_its_days_the_next_as_much_as_a_week_on() {
    // 2026-10-20 is a Tuesday.
    let plan = reading(&[Tue, Thu], "08:00", "21:00", FIVE_MINUTES);
    assert_eq!(
        plan.open_at(local("2026-10-20 12:00:00")),
        Some(plan.day(date("2026-10-20")))
    );
    assert_eq!(plan.open_at(local("2026-10-21 12:00:00")), None);
    // Closed on Tuesday evening, it waits through Wednesday for Thursday's window.
    assert_eq!(
        plan.step(local("2026-10-20 21:00:00"), i64::MIN, &[], None),
        Step::Wait(local("2026-10-22 08:00:00"))
    );
    // On Tuesdays alone, once Tuesday's has opened the next is a week on.
    let weekly = reading(&[Tue], "08:00", "21:00", FIVE_MINUTES);
    assert_eq!(
        weekly.next(local("2026-10-20 08:00:00")).open,
        local("2026-10-27 08:00:00")
    );
    // Overnight from a Tuesday, it's open past midnight into Wednesday all the same.
    let overnight = reading(&[Tue], "20:00", "02:00", FIVE_MINUTES);
    assert_eq!(
        overnight.open_at(local("2026-10-21 01:00:00")),
        Some(overnight.day(date("2026-10-20")))
    );
}

#[test]
fn a_dsp_reads_at_its_window_start_and_on_each_slot_until_the_window_closes() {
    let plan = plan("08:00", "21:00");
    let day = plan.day(date("2026-10-20"));
    assert_eq!(
        plan.step(local("2026-10-20 07:00:00"), i64::MIN, &[], None),
        Step::Wait(day.open)
    );
    assert_eq!(plan.step(day.open, i64::MIN, &[], None), Step::Read(day));
    // Read, it waits for its next slot, which a slow read that passed it skips.
    let read = local("2026-10-20 08:00:01");
    assert_eq!(
        plan.step(read, read, &[], None),
        Step::Wait(local("2026-10-20 08:05:00"))
    );
    let slow = local("2026-10-20 08:06:00");
    assert_eq!(
        plan.step(slow, slow, &[], None),
        Step::Wait(local("2026-10-20 08:10:00"))
    );
    // Its last slot is the last before the close; then it waits for the close.
    let last = local("2026-10-20 20:55:01");
    assert_eq!(plan.step(last, last, &[], None), Step::Wait(day.close));
}

#[test]
fn started_again_a_dsp_keeps_its_slot_and_reads_at_once_only_past_it() {
    let plan = plan("08:00", "21:00");
    let day = plan.day(date("2026-10-20"));
    let resumed = |now: &str, last: &str| {
        let now = local(now);
        plan.step(now, plan.resumed(now, Some(local(last))), &[], None)
    };
    // Back two minutes after its last read, it waits for its next slot.
    assert_eq!(
        resumed("2026-10-20 10:02:00", "2026-10-20 10:00:00"),
        Step::Wait(local("2026-10-20 10:05:00"))
    );
    // Back once that slot has passed, it reads at once, and only once.
    let back = local("2026-10-20 10:20:00");
    assert_eq!(
        resumed("2026-10-20 10:20:00", "2026-10-20 10:00:00"),
        Step::Read(day)
    );
    assert_eq!(
        plan.step(back, back, &[], None),
        Step::Wait(local("2026-10-20 10:25:00"))
    );
    // Its last read came late, off the grid: back soon after, though its slot has passed, it
    // waits for the next.
    assert_eq!(
        resumed("2026-10-20 10:06:40", "2026-10-20 10:06:20"),
        Step::Wait(local("2026-10-20 10:10:00"))
    );
    // Its last read yesterday counts for nothing today: it reads as the window opens.
    assert_eq!(
        resumed("2026-10-20 08:00:00", "2026-10-19 20:55:00"),
        Step::Read(day)
    );
    // Never read, it reads as soon as a slot has come.
    let first = local("2026-10-20 09:00:00");
    assert_eq!(
        plan.step(first, plan.resumed(first, None), &[], None),
        Step::Read(day)
    );
}

#[test]
fn a_started_day_stops_as_its_window_closes_before_a_later_day_reads() {
    let plan = plan("08:00", "21:00");
    let today = date("2026-10-20");
    let last = local("2026-10-20 20:55:00");
    assert_eq!(
        plan.step(local("2026-10-20 20:59:59"), last, &[today], None),
        Step::Wait(local("2026-10-20 21:00:00"))
    );
    assert_eq!(
        plan.step(local("2026-10-20 21:00:00"), last, &[today], None),
        Step::Stop(today)
    );
    // A day that never started has nothing to stop: it waits for tomorrow's window.
    assert_eq!(
        plan.step(local("2026-10-20 21:00:00"), last, &[], None),
        Step::Wait(local("2026-10-21 08:00:00"))
    );
    // Down as it closed, the server stops it as it's back, whenever that is.
    assert_eq!(
        plan.step(local("2026-10-21 09:00:00"), last, &[today], None),
        Step::Stop(today)
    );
    // An all-day window's next day reads only once the day before has stopped.
    let all_day = reading(EVERY_DAY, "06:00", "06:00", FIVE_MINUTES);
    let tomorrow = all_day.day(date("2026-10-21"));
    assert_eq!(
        all_day.step(tomorrow.open, last, &[today], None),
        Step::Stop(today)
    );
    assert_eq!(
        all_day.step(tomorrow.open, last, &[], Some(today)),
        Step::Read(tomorrow)
    );
}

#[test]
fn a_started_day_on_a_day_no_longer_chosen_reads_no_more_and_stops_as_its_window_closes() {
    // Tuesday's day started, then the DSP chose Mondays alone.
    let plan = reading(&[Mon], "08:00", "21:00", FIVE_MINUTES);
    let today = date("2026-10-20");
    let last = local("2026-10-20 12:00:00");
    assert_eq!(
        plan.step(local("2026-10-20 12:05:00"), last, &[today], None),
        Step::Wait(local("2026-10-20 21:00:00"))
    );
    assert_eq!(
        plan.step(local("2026-10-20 21:00:00"), last, &[today], None),
        Step::Stop(today)
    );
    assert_eq!(
        plan.step(local("2026-10-20 21:00:00"), last, &[], Some(today)),
        Step::Wait(local("2026-10-26 08:00:00"))
    );
}

#[test]
fn a_day_that_ended_reads_no_more_though_its_window_is_moved_to_end_later() {
    let today = date("2026-10-20");
    let last = local("2026-10-20 20:55:00");
    // Its stop moved to 22:00 once the day stopped at 21:00, today's window is open again.
    let later = plan("08:00", "22:00");
    let now = local("2026-10-20 21:10:00");
    assert_eq!(
        later.step(now, last, &[], None),
        Step::Read(later.day(today))
    );
    // Its stop begun but not done, as when its hook failed part of the way, it stops at once,
    // reading nothing first.
    assert_eq!(
        later.step(now, last, &[today], Some(today)),
        Step::Stop(today)
    );
    // Stopped, the day reads no more: tomorrow's window is the next to read.
    let tomorrow = later.day(date("2026-10-21"));
    assert_eq!(
        later.step(now, last, &[], Some(today)),
        Step::Wait(tomorrow.open)
    );
    assert_eq!(
        later.step(tomorrow.open, last, &[], Some(today)),
        Step::Read(tomorrow)
    );
}
