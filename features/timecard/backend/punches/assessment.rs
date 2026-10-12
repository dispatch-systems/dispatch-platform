//! Interpret collected punch kinds and day offsets. Display labels stay in the dashboard.
use crate::api::{
    assessment::{AssessedClock, Lunch, PaycomDay, PunchEvent},
    types::{EmployeeTimecard, Punch, Timecard},
};
use chrono::TimeZone;

// Collected display strings historically used JavaScript's whitespace rules.
fn clock_space(c: char) -> bool {
    c.is_whitespace() && c != '\u{0085}' || c == '\u{feff}'
}

pub(crate) fn parse_clock(value: &str) -> Option<i32> {
    let value = value.trim_matches(clock_space);
    let upper = value.to_ascii_uppercase();
    let suffix = upper
        .strip_suffix("AM")
        .map(|s| (s, false))
        .or_else(|| upper.strip_suffix("PM").map(|s| (s, true)));
    let clock = suffix.map_or(upper.as_str(), |(s, _)| s.trim_end_matches(clock_space));
    let (hours, minutes) = clock.split_once(':')?;
    if !(1..=2).contains(&hours.len())
        || minutes.len() != 2
        || !hours
            .bytes()
            .chain(minutes.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let mut hours: i32 = hours.parse().ok()?;
    let minutes: i32 = minutes.parse().ok()?;
    if minutes > 59
        || if suffix.is_some() {
            !(1..=12).contains(&hours)
        } else {
            hours > 23
        }
    {
        return None;
    }
    if let Some((_, pm)) = suffix {
        hours = hours % 12 + if pm { 12 } else { 0 };
    }
    Some(hours * 60 + minutes)
}

pub fn paycom_day(status: &str, punches: &[Punch]) -> PaycomDay {
    let typed = punches
        .iter()
        .any(|p| p.in_kind.is_some() || p.out_kind.is_some());
    let legacy = !typed
        && status == "Complete"
        && (1..=2).contains(&punches.len())
        && punches.iter().all(|p| {
            p.clock_in.as_deref().and_then(parse_clock).is_some()
                && p.clock_out.as_deref().and_then(parse_clock).is_some()
        });
    let mut result = PaycomDay {
        in_day: None,
        out_day: None,
        lunches: vec![],
        events: vec![],
        review: !punches.is_empty() && !typed && !legacy,
        legacy,
    };
    let mut previous = -1;
    let mut day = 0;
    for (i, punch) in punches.iter().enumerate() {
        for (incoming, raw, explicit) in [
            (
                true,
                punch.clock_in.as_deref(),
                punch.in_kind.flatten().map(|k| k.as_str()),
            ),
            (
                false,
                punch.clock_out.as_deref(),
                punch.out_kind.flatten().map(|k| k.as_str()),
            ),
        ] {
            let Some(raw) = raw.filter(|s| !s.trim_matches(clock_space).is_empty()) else {
                continue;
            };
            let clock = parse_clock(raw);
            if let Some(clock) = clock {
                if clock < previous {
                    if previous - clock > 12 * 60 {
                        day += 1;
                    } else {
                        result.review = true;
                    }
                }
                previous = clock;
            }
            let time = clock.map(|minute| AssessedClock {
                minute: minute + day * 1440,
                day,
            });
            let kind = if typed {
                explicit
            } else if legacy {
                Some(if incoming {
                    if i == 0 { "IN DAY" } else { "IN LUNCH" }
                } else if i == punches.len() - 1 {
                    "OUT DAY"
                } else {
                    "OUT LUNCH"
                })
            } else {
                None
            }
            .unwrap_or("Unlabeled punch");
            if time.is_none() || kind == "Unlabeled punch" || day > 1 {
                result.review = true;
            }
            match kind {
                "IN DAY" if result.in_day.is_none() => result.in_day = time.clone(),
                "OUT DAY" => result.out_day = time.clone(),
                "OUT LUNCH" => result.lunches.push(Lunch {
                    out: time.clone(),
                    clock_in: None,
                }),
                "IN LUNCH" => match result.lunches.last_mut() {
                    Some(lunch) if lunch.clock_in.is_none() => lunch.clock_in = time.clone(),
                    _ => result.lunches.push(Lunch {
                        out: None,
                        clock_in: time.clone(),
                    }),
                },
                _ => {}
            }
            result.events.push(PunchEvent {
                kind: kind.into(),
                time,
                raw: raw.into(),
            });
        }
    }
    result
}

enum WorkState {
    Outside,
    Working(i64),
    Meal,
}

// Sum only complete on-clock segments. Reported hours can include paid leave and
// already exclude meals, so they cannot safely supply or adjust worked time.
fn worked_minutes(card: &Timecard, day: &PaycomDay, timezone: &str) -> Option<i32> {
    if day.review || !matches!(card.status.as_str(), "Complete" | "No punches") {
        return None;
    }
    if card.punches.iter().any(|punch| {
        [
            (punch.in_kind.flatten().is_some(), punch.clock_in.as_deref()),
            (
                punch.out_kind.flatten().is_some(),
                punch.clock_out.as_deref(),
            ),
        ]
        .into_iter()
        .any(|(typed, clock)| {
            typed && clock.is_none_or(|clock| clock.trim_matches(clock_space).is_empty())
        })
    }) {
        return None;
    }
    if day.events.is_empty() {
        return (card.punches.is_empty() && card.hours == 0.0).then_some(0);
    }
    if card.status != "Complete" {
        return None;
    }
    let timezone: chrono_tz::Tz = timezone.parse().ok()?;
    let midnight = card
        .date
        .parse::<chrono::NaiveDate>()
        .ok()?
        .and_hms_opt(0, 0, 0)?;
    let mut state = WorkState::Outside;
    let mut total: i32 = 0;
    let mut previous = i64::MIN;
    let mut closed = false;
    for event in &day.events {
        let local = midnight.checked_add_signed(chrono::Duration::minutes(i64::from(
            event.time.as_ref()?.minute,
        )))?;
        // Repeated or skipped local clocks cannot establish an unambiguous duration.
        let minute = timezone
            .from_local_datetime(&local)
            .single()?
            .timestamp()
            .div_euclid(60);
        if minute < previous {
            return None;
        }
        previous = minute;
        state = match (state, event.kind.as_str()) {
            (WorkState::Outside, "IN DAY") => WorkState::Working(minute),
            (WorkState::Working(start), "OUT LUNCH") => {
                total = total.checked_add(i32::try_from(minute - start).ok()?)?;
                WorkState::Meal
            }
            (WorkState::Meal, "IN LUNCH") => WorkState::Working(minute),
            (WorkState::Working(start), "OUT DAY") => {
                total = total.checked_add(i32::try_from(minute - start).ok()?)?;
                closed = true;
                WorkState::Outside
            }
            _ => return None,
        };
    }
    (closed && matches!(state, WorkState::Outside)).then_some(total)
}

impl Timecard {
    pub fn assessed(self, timezone: &str) -> EmployeeTimecard {
        let assessment = paycom_day(&self.status, &self.punches);
        let worked = worked_minutes(&self, &assessment, timezone);
        EmployeeTimecard {
            card: self,
            assessment,
            regular_minutes: worked.map(|minutes| minutes.min(480)),
            overtime_minutes: worked.map(|minutes| (minutes - 480).max(0)),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/backend/punches/assessment.rs"]
mod tests;
