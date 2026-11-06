//! Human-readable summaries of the published answer.
use super::{Section, answer::Found, parse};
use dispatch_core::manifest::people::PersonTitle;

impl Section {
    fn name(self) -> &'static str {
        match self {
            Section::Scorecards => "scorecards",
            Section::Daily => "daily",
            Section::Routes => "routes",
            Section::Timecards => "timecards",
            Section::Roster => "roster",
            Section::Chat => "chat",
            Section::Inspections => "inspections",
        }
    }
    /// What its section of `found` tells, or none where the section is null.
    fn told(self, found: &Found) -> Option<Told> {
        Some(match self {
            Section::Scorecards => {
                let s = found.scorecards.as_ref()?;
                let mut words = count(s.weeks.len(), "weekly scorecard", "weekly scorecards");
                let packages = s.packages.len() + s.more_packages;
                if packages > 0 {
                    words += ", ";
                    words += &count(packages, "scorecard package", "scorecard packages");
                }
                Told::new(words, &s.ids, &s.unknown_weeks, &[])
            }
            Section::Daily => {
                let d = found.daily.as_ref()?;
                let mut dates: Vec<&str> = d.days.iter().map(|day| day.date.as_str()).collect();
                dates.dedup();
                let days = dates.len() + d.more_days;
                let words = count(days, "day of daily metrics", "days of daily metrics");
                Told::new(words, &d.ids, &d.unknown_days, &[])
            }
            Section::Routes => {
                let r = found.routes.as_ref()?;
                let words = count(r.routes.len() + r.more_routes, "route", "routes");
                Told::new(words, &r.ids, &r.unknown_days, &r.partial_days)
            }
            Section::Timecards => {
                let t = found.timecards.as_ref()?;
                let words = count(t.days.len() + t.more_days, "timecard", "timecards");
                Told::new(words, &t.ids, &t.unknown_days, &t.partial_days)
            }
            Section::Roster => {
                let r = found.roster.as_ref()?;
                // A rehire's new ID says more than the one Amazon offboarded.
                let listed = |status: Option<&str>| {
                    r.associates.iter().find(|associate| {
                        associate.listed && status.is_none_or(|status| associate.status == status)
                    })
                };
                let words = match listed(Some("ACTIVE")).or_else(|| listed(None)) {
                    Some(associate) if associate.status == "OFFBOARDED" => {
                        "offboarded from Amazon's roster".into()
                    }
                    Some(associate) => format!("on Amazon's roster as {}", associate.status),
                    None if r.collected.is_none() => "Amazon's roster not collected yet".into(),
                    None if r.associates.is_empty() => "not on Amazon's roster".into(),
                    None => "no longer on Amazon's roster".into(),
                };
                Told::new(words, &r.ids, &[], &[])
            }
            Section::Chat => {
                let c = found.chat.as_ref()?;
                let words = count(c.accounts.len(), "chat account", "chat accounts");
                Told::new(words, &c.ids, &[], &[])
            }
            Section::Inspections => {
                let i = found.inspections.as_ref()?;
                let short = i.inspections.len() + i.more_inspections;
                let words = count(short, "short DVIC inspection", "short DVIC inspections");
                Told::new(words, &i.ids, &i.unknown_days, &[])
            }
        })
    }
}
/// What a section tells the summary: what it found, in words, and whether it knows none of their
/// IDs, has days unknown, or has days read while under way.
struct Told {
    words: String,
    no_ids: bool,
    unknown: bool,
    partial: bool,
}
impl Told {
    fn new(words: String, ids: &[String], unknown: &[String], partial: &[String]) -> Self {
        Self {
            words,
            no_ids: ids.is_empty(),
            unknown: !unknown.is_empty(),
            partial: !partial.is_empty(),
        }
    }
}

/// A day as people read it: Oct 4.
fn day(date: &str) -> String {
    parse(date).map_or_else(|| date.to_owned(), |date| date.format("%b %-d").to_string())
}
/// So many of something, in words: no routes, 1 route, 5 routes.
fn count(count: usize, one: &str, many: &str) -> String {
    match count {
        0 => format!("no {many}"),
        1 => format!("1 {one}"),
        count => format!("{count} {many}"),
    }
}
/// What the answer says, in a sentence or a few, for the agent to read out: who, what each
/// section found and doesn't know, which sections asked for are off, and whether lists were cut
/// short.
pub(super) fn summary(found: &Found, wanted: &[Section]) -> String {
    let (mut told, mut no_ids, mut unknown, mut partial, mut off) =
        (vec![], vec![], vec![], vec![], vec![]);
    for section in Section::ALL {
        let name = section.name();
        let Some(section_told) = section.told(found) else {
            if wanted.contains(&section) {
                off.push(name);
            }
            continue;
        };
        told.push(section_told.words);
        if section_told.no_ids {
            no_ids.push(name);
        }
        if section_told.unknown {
            unknown.push(name);
        }
        if section_told.partial {
            partial.push(name);
        }
    }
    let person = &found.person;
    let title = person.title.as_deref().and_then(PersonTitle::parse);
    let at = match (person.role.as_str(), title) {
        ("former", _) => format!("who has left {}", found.dsp),
        ("offboarded", _) => format!("offboarded from {}", found.dsp),
        (_, Some(title)) => format!("{} at {}", title.label(), found.dsp),
        ("office", None) => format!("in management at {}", found.dsp),
        (role, None) => format!("a {role} at {}", found.dsp),
    };
    let (name, code, from) = (&person.name, &person.code, day(&found.from));
    let mut text = format!("{name} ({code}), {at}, {from}");
    if found.to != found.from {
        text += &format!(" to {}", day(&found.to));
    }
    if !told.is_empty() {
        text += &format!(": {}", told.join(", "));
    }
    text.push('.');
    if !no_ids.is_empty() {
        let names = no_ids.join(", ");
        text += &format!(" They hold no ID that {names} read, so nothing of theirs is there.");
    }
    if !unknown.is_empty() {
        let names = unknown.join(", ");
        text += &format!(" Some days aren't collected in {names}, so they're unknown.");
    }
    if !partial.is_empty() {
        let names = partial.join(", ");
        text += &format!(
            " Some days in {names} were read while under way, so they show only what was known \
             then."
        );
    }
    if !off.is_empty() {
        text += &format!(" Off at {}: {}.", found.dsp, off.join(", "));
    }
    if found.trimmed {
        text += " Some lists were cut short: ask for fewer days or sections to see the rest.";
    }
    text
}
