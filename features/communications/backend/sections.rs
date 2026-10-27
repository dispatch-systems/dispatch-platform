//! The page's sections, each listing one kind of conversation: Today's Routes, drivers' work
//! conversations on the live day, whose own list `texts::todays_routes` reads whole; Messaging,
//! the numbers' texts; and Slack, the workspaces' direct messages and the groups. Each is described
//! once, in `SECTIONS`: its stable ID, whether the DSP has it, how many of its conversations wait,
//! what the page lists of it as it opens, and how its conversations are found. The overview, its
//! updates and the search read them; a new section is one more there, with its place in what the
//! page reads.
use super::{
    conversations,
    groups::{self, storage::GroupStore},
    model::ConversationKind,
    people::People,
    storage::CommunicationsStore,
    texts::{self, live_day, numbers::Numbers, reading, todays_routes},
};
use crate::api::types::{Conversation, Conversations, Group};
use dispatch_core::{Error, Result, State, db::Store, ensure};

/// One of the page's sections.
pub struct Section {
    /// Its stable ID, as the page names it: never changed once shipped.
    pub id: &'static str,
    /// The kind of conversation it lists, as a search of it names it (`conversations?kind=`), but
    /// Slack's, searched by default.
    pub kind: ConversationKind,
    /// Whether the DSP has it now.
    pub offered: fn(&Store, &str) -> Result<bool>,
    /// How many of its conversations wait on an answer, as the counts go.
    pub waiting: fn(&Store, &str) -> Result<usize>,
    /// What the page lists of it as it opens.
    pub listed: fn(&Reading<'_>) -> Result<Listed>,
    /// Its conversations found as asked.
    pub found: fn(&Store, &State, &str, &Query<'_>) -> Result<Conversations>,
}

/// Today's Routes, while the DSP texts and has live route data: its drivers' conversations waiting
/// on the live day, found by a name or a work phone's digits, never a page at a time.
pub const TODAYS_ROUTES: Section = Section {
    id: "routes",
    kind: ConversationKind::Work,
    offered: todays_routes::offered,
    waiting: |db, dsp| Ok(live_day::waiting(db, dsp)?.len()),
    listed: |r| {
        let waiting = live_day::waiting(r.db, r.dsp)?;
        Ok(Listed {
            waiting: reading::conversations(r.db, r.people, r.numbers, r.dsp, waiting)?,
            ..Listed::default()
        })
    },
    found: |db, state, dsp, query| match (query.team, &query.asked) {
        (None, Asked::Typed(typed)) => {
            reading::search(db, state, dsp, ConversationKind::Work, typed)
        }
        _ => Err(Error::new("invalid_input", 400)),
    },
};
/// Messaging, while the DSP texts: the numbers' conversations, but blocked numbers'.
pub const MESSAGING: Section = Section {
    id: "messaging",
    kind: ConversationKind::Number,
    offered: texts::offered,
    waiting: |db, dsp| Ok(reading::waiting_numbers(db, dsp)?.len()),
    listed: |r| {
        let waiting = reading::waiting_numbers(r.db, r.dsp)?;
        let (answered, more) = reading::answered(r.db, r.people, r.numbers, r.dsp, None)?;
        Ok(Listed {
            waiting: reading::conversations(r.db, r.people, r.numbers, r.dsp, waiting)?,
            answered,
            groups: Vec::new(),
            more,
        })
    },
    found: |db, state, dsp, query| {
        ensure(query.team.is_none(), "invalid_input", 400)?;
        match query.asked {
            Asked::Typed(typed) => reading::search(db, state, dsp, ConversationKind::Number, typed),
            Asked::Before(before) => reading::older(db, state, dsp, before),
        }
    },
};
/// Slack's: the direct messages of every workspace or the one shown, and the groups. What a
/// workspace's members wrote stays listed once it's gone, so it's always the DSP's; the page shows
/// it while a workspace is there.
pub const SLACK: Section = Section {
    id: "slack",
    kind: ConversationKind::Slack,
    offered: |_, _| Ok(true),
    waiting: |db, dsp| Ok(db.waiting_communications(dsp)?.len()),
    listed: |r| {
        let waiting = r.db.waiting_communications(r.dsp)?;
        let waiting_groups = r.db.waiting_communications_groups(r.dsp)?;
        let (answered, answered_groups, more) =
            conversations::answered(r.db, r.people, r.dsp, r.team, None)?;
        Ok(Listed {
            waiting: conversations::conversations(r.db, r.people, r.dsp, waiting)?,
            answered,
            groups: [
                groups::listed(r.db, r.people, r.dsp, waiting_groups)?,
                answered_groups,
            ]
            .concat(),
            more,
        })
    },
    found: |db, state, dsp, query| match query.asked {
        Asked::Typed(typed) => conversations::search(db, state, dsp, query.team, typed),
        Asked::Before(before) => conversations::older(db, state, dsp, query.team, before),
    },
};
/// Every section, in the order the overview lists their conversations: Slack's, then by text, as
/// it always has; the page shows them in an order of its own.
pub const SECTIONS: &[&Section] = &[&SLACK, &MESSAGING, &TODAYS_ROUTES];

/// What a section's list is read with: the DSP, who everyone is, what it keeps of numbers, and
/// the workspace a Slack list is of, if one.
pub struct Reading<'a> {
    pub db: &'a Store,
    pub dsp: &'a str,
    pub people: &'a People,
    pub numbers: &'a Numbers,
    pub team: Option<&'a str>,
}

/// What the page lists of a section as it opens: its conversations waiting, the longest waiting
/// first; then the latest answered, a page of them, with the groups among them; and whether older
/// answered ones are left to read.
#[derive(Default)]
pub struct Listed {
    pub waiting: Vec<Conversation>,
    pub answered: Vec<Conversation>,
    pub groups: Vec<Group>,
    pub more: bool,
}

/// What a search of a section asks: in the workspace `team`, for Slack's, what was typed, or the
/// answered ones before the last listed.
pub struct Query<'a> {
    pub team: Option<&'a str>,
    pub asked: Asked<'a>,
}
pub enum Asked<'a> {
    /// A name, or a number's digits.
    Typed(&'a str),
    /// When the last listed was last written in, and its ID.
    Before((i64, &'a str)),
}

/// Each section the DSP has now, with what the page lists of it.
pub fn listed(r: &Reading<'_>) -> Result<Vec<(&'static Section, Listed)>> {
    let mut listed = Vec::new();
    for &section in SECTIONS {
        if (section.offered)(r.db, r.dsp)? {
            listed.push((section, (section.listed)(r)?));
        }
    }
    Ok(listed)
}

/// How many conversations wait in the sections the DSP has now.
pub fn waiting(db: &Store, dsp: &str) -> Result<usize> {
    let mut waiting = 0;
    for &section in SECTIONS {
        if (section.offered)(db, dsp)? {
            waiting += (section.waiting)(db, dsp)?;
        }
    }
    Ok(waiting)
}

/// The conversations of the section listing `kind` found as `query` asks.
pub fn found(
    db: &Store,
    state: &State,
    dsp: &str,
    kind: ConversationKind,
    query: &Query<'_>,
) -> Result<Conversations> {
    let section = SECTIONS
        .iter()
        .find(|section| section.kind == kind)
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    (section.found)(db, state, dsp, query)
}
