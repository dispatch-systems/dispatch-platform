//! Telling members of new messages with Dispatch closed, or on a phone (Web Push). A browser a
//! member turns Notifications on in is subscribed with its push service and kept as theirs
//! (`storage`); each message new in what waits, as the page's alerts see it (`inbox::waiting`'s
//! `latest`: one starting a conversation waiting or arriving in one waiting, never the DSP's own),
//! goes to each of those browsers of each member who may be told of it now, titled with who wrote
//! it, saying what as the list shows it, and opening the conversation at the address the browser
//! was subscribed at (`sending`). The browser's service worker shows it, but quietly where a page
//! of Dispatch in front says it alerts of it itself.
//!
//! A DSP with a browser kept is watched from the minute's upkeep, or once one is kept, for as long
//! as the server runs: each change to its conversations (`live::TOPIC`) has what waits read again,
//! and what's new in it since the last read pushed, apart from the change, a batch at a time. What
//! waited when the watching began tells no one. Who may be told is decided as a message goes, as a
//! session of theirs at that address would be let in (`Store::may`): a member whose account, role
//! or DSP no longer lets them use Communications, or who turned Notifications off, has their
//! browsers let go of.
//!
//! Each line's pushes are its own (`Pushed`): Dispatch Phone's own (`OWN`), of what waits on its
//! page, Slack's among it; or another feature's, of its own texts alone, told to the browsers kept
//! for that line, of members holding its permission with its Notifications on, titled with its
//! name and opening its page. A browser told of one line is kept, and let go of, apart from the
//! same browser told of another.
pub(crate) mod fixture;
pub(crate) mod sending;
pub(crate) mod storage;
pub(crate) mod webpush;

use super::{changes::ChangeStore, display::DISPLAY, inbox, lines, live};
use crate::api::{display::MessageDisplay, types::WaitingMessage};
use dispatch_core::{
    Result, State,
    db::Store,
    foundation::{config::Site, observability},
    manifest::{Maintenance, Section, Upkeep},
    settings,
};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};
use storage::{Address, PushStore, Subscription};

/// Every minute, each DSP with a browser kept for Dispatch Phone's own line is watched, as after a
/// restart.
pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60),
    run: |state, _| pushing(state, &OWN),
};
/// How many messages no longer waiting are remembered, the latest; those waiting all are.
const MOST: usize = 500;

/// What one line's pushes are: who is told of them, what they say and where they open.
pub struct Pushed {
    /// The line, by its feature's name: none for Dispatch Phone's own.
    pub line: Option<&'static str>,
    /// What a member must hold to be told.
    pub permission: &'static str,
    /// The page a push opens, after the DSP's address.
    pub page: &'static str,
    /// What a push's title says before who wrote it: none for Dispatch Phone's own.
    pub label: Option<&'static str>,
    /// Where each member's Message display is kept, whose Notifications says whether they're told.
    pub display: &'static Section,
}
/// Dispatch Phone's own line's: of what waits on its page, Slack's among it, titled with who wrote
/// it alone.
pub const OWN: Pushed = Pushed {
    line: None,
    permission: "communications.use",
    page: "communications",
    label: None,
    display: &DISPLAY,
};

fn failed(dsp: &str, error: &dispatch_core::Error) {
    observability::event(
        "error",
        "communications_push_failed",
        json!({"dspId": dsp, "error": error.code}),
    );
}

/// Watches each DSP with a browser kept for `pushed`'s line, as after a restart: while that line is
/// on there.
pub fn pushing(state: Arc<State>, pushed: &'static Pushed) -> Upkeep {
    Box::pin(async move {
        if webpush::vapid(&state.config).is_none() {
            return;
        }
        let pushing = state
            .read(move |db| {
                let mut pushing = Vec::new();
                for dsp in db.kept_dsps()? {
                    if lines::on(db, &dsp, pushed.line)?
                        && db.communications_pushing(&dsp, pushed.line)?
                    {
                        pushing.push(dsp);
                    }
                }
                Ok(pushing)
            })
            .await;
        match pushing {
            Ok(pushing) => pushing.iter().for_each(|dsp| watch(&state, dsp, pushed)),
            Err(error) => failed("", &error),
        }
    })
}

/// A DSP watched, and the line: Dispatch Phone's own for none.
type Watched = (String, Option<&'static str>);
/// Watches the DSP's conversations of `pushed`'s line for what's new to push, unless they're
/// watched already.
pub fn watch(state: &Arc<State>, dsp: &str, pushed: &'static Pushed) {
    static WATCHED: LazyLock<Mutex<HashSet<Watched>>> = LazyLock::new(Default::default);
    if webpush::vapid(&state.config).is_none()
        || !WATCHED
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert((dsp.to_owned(), pushed.line))
    {
        return;
    }
    let (state, dsp) = (state.clone(), dsp.to_owned());
    tokio::spawn(async move { watching(&state, &dsp, pushed).await });
}

/// Reads what waits in the DSP on `pushed`'s line as it first does and once each change of that
/// line since, pushing what's new before it reads again: changes while a batch goes are read
/// together after it.
async fn watching(state: &Arc<State>, dsp: &str, pushed: &'static Pushed) {
    // Subscribed before the first read, so a change while it reads still wakes it.
    let mut wakes = live::listen(state, dsp, pushed.line);
    let (mut seen, mut revision) = (None, None);
    loop {
        let told = match look(state, dsp, pushed, &mut seen, &mut revision).await {
            Ok(new) if !new.is_empty() => tell(state, dsp, pushed, new).await,
            Ok(_) => Ok(()),
            Err(error) => Err(error),
        };
        if let Err(error) = told {
            failed(dsp, &error);
        }
        if wakes.changed().await.is_err() {
            return;
        }
    }
}

/// What's new in what waits in the DSP on `pushed`'s line since `seen`, read again only if it
/// changed since `revision`.
async fn look(
    state: &Arc<State>,
    dsp: &str,
    pushed: &'static Pushed,
    seen: &mut Option<Vec<String>>,
    revision: &mut Option<i64>,
) -> Result<Vec<WaitingMessage>> {
    let (shared, at, last) = (state.clone(), dsp.to_owned(), *revision);
    let read = state
        .read(move |db| {
            let now = db.communications_revision(&at, pushed.line)?;
            if last == Some(now) {
                return Ok(None);
            }
            Ok(Some((
                now,
                inbox::waiting(db, &shared, &at, pushed.line)?.latest,
            )))
        })
        .await?;
    let Some((now, latest)) = read else {
        return Ok(Vec::new());
    };
    *revision = Some(now);
    let (new, next) = fresh(latest, seen.as_deref());
    *seen = Some(next);
    Ok(new)
}

/// What's new in `latest`, the message each conversation waits on, since `seen`, the IDs of those
/// read before; and what's seen after it. Nothing is new to a first read.
fn fresh(
    latest: Vec<WaitingMessage>,
    seen: Option<&[String]>,
) -> (Vec<WaitingMessage>, Vec<String>) {
    let ids: Vec<String> = latest.iter().map(|each| each.message.id.clone()).collect();
    let Some(seen) = seen else {
        return (Vec::new(), ids);
    };
    let (known, waiting): (HashSet<&String>, HashSet<&String>) =
        (seen.iter().collect(), ids.iter().collect());
    // Those no longer waiting are remembered too, the latest of them, so a message that waits
    // again, as the one written after it was deleted, isn't new.
    let gone: Vec<&String> = seen.iter().filter(|id| !waiting.contains(id)).collect();
    let next = gone[gone.len().saturating_sub(MOST)..]
        .iter()
        .map(|id| (*id).clone())
        .chain(ids.iter().cloned())
        .collect();
    let new = latest
        .into_iter()
        .filter(|each| !known.contains(&each.message.id))
        .collect();
    (new, next)
}

/// Pushes each of `new` to the browsers kept for `pushed`'s line of each member of the DSP who may
/// be told of it now, letting go of those of a member who may not.
async fn tell(
    state: &Arc<State>,
    dsp: &str,
    pushed: &'static Pushed,
    new: Vec<WaitingMessage>,
) -> Result<()> {
    let at = dsp.to_owned();
    let (told, untold, code) = state
        .read(move |db| {
            let code = db.find_dsp(&at)?.code;
            let mut decided = HashMap::new();
            let (mut told, mut untold) = (Vec::new(), Vec::new());
            for subscription in db.communications_pushes(&at, pushed.line)? {
                let whom = (subscription.user.clone(), subscription.address);
                let tells = match decided.get(&whom) {
                    Some(tells) => *tells,
                    None => {
                        let tells = tells(db, &at, pushed, code.as_deref(), &subscription)?;
                        decided.insert(whom, tells);
                        tells
                    }
                };
                if tells {
                    told.push(subscription);
                } else {
                    untold.push(subscription.endpoint);
                }
            }
            Ok((told, untold, code))
        })
        .await?;
    if !untold.is_empty() {
        let at = dsp.to_owned();
        state
            .run_bookkeeping(move |db| db.drop_communications_pushes(&at, pushed.line, &untold))
            .await?;
    }
    // Where each push opens: the DSP's page at the address its browser was subscribed at.
    let config = &state.config;
    let opens = |address: Address| match (address, &code) {
        (Address::Dsp, Some(code)) => format!("{}/#", config.dsp_url(code)),
        _ => format!("{}/#dsp/{dsp}/", config.origin),
    };
    let pushes: Vec<(Subscription, Vec<u8>)> = new
        .iter()
        .flat_map(|message| {
            let pushes: HashMap<Address, Vec<u8>> = [Address::Admin, Address::Dsp]
                .into_iter()
                .map(|address| {
                    let says = sending::message(dsp, pushed, message, &opens(address));
                    (address, says)
                })
                .collect();
            told.iter().map(move |subscription| {
                (subscription.clone(), pushes[&subscription.address].clone())
            })
        })
        .collect();
    sending::send(state, dsp, pushed, pushes).await
}

/// What becomes of a push to `subscription` as its turn to go comes, read again then.
pub enum Turn {
    /// It goes: the browser is kept as it was, of a member who is told.
    Goes,
    /// It doesn't, and the browser is let go of: its member is no longer told.
    Untold,
    /// It doesn't: the browser is no longer kept so, let go of or kept anew since.
    Gone,
}
/// What becomes of a push of `pushed`'s line to `subscription` in the DSP `dsp` now.
pub async fn turn(
    state: &Arc<State>,
    dsp: &str,
    pushed: &'static Pushed,
    subscription: &Subscription,
) -> Result<Turn> {
    let (at, subscription) = (dsp.to_owned(), subscription.clone());
    state
        .read(move |db| {
            if db
                .communications_push(&at, pushed.line, &subscription.endpoint)?
                .as_ref()
                != Some(&subscription)
            {
                return Ok(Turn::Gone);
            }
            let code = db.find_dsp(&at)?.code;
            Ok(
                match tells(db, &at, pushed, code.as_deref(), &subscription)? {
                    true => Turn::Goes,
                    false => Turn::Untold,
                },
            )
        })
        .await
}

/// Whether the member whose browser `subscription` names is told of the DSP's new messages of
/// `pushed`'s line: with its Notifications on, as one who may use that line there now, signed in
/// where they subscribed it, which holds only while the line's own feature is on there and in
/// their sight.
fn tells(
    db: &Store,
    dsp: &str,
    pushed: &Pushed,
    code: Option<&str>,
    subscription: &Subscription,
) -> Result<bool> {
    let site = match (subscription.address, code) {
        (Address::Admin, _) => Site::Admin,
        (Address::Dsp, Some(code)) => Site::Dsp(code.to_owned()),
        (Address::Dsp, None) => return Ok(false),
    };
    let user = &subscription.user;
    Ok(db.may(user, &site, dsp, pushed.permission)?
        && settings::read_member::<MessageDisplay>(db, dsp, user, pushed.display)?.notifications)
}

#[cfg(test)]
#[path = "../../tests/backend/push/mod.rs"]
mod tests;
