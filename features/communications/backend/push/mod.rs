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
pub(crate) mod fixture;
pub(crate) mod sending;
pub(crate) mod storage;
pub(crate) mod webpush;

use super::{changes::ChangeStore, display::DISPLAY, inbox, live};
use crate::api::{display::MessageDisplay, types::WaitingMessage};
use dispatch_core::{
    Result, State,
    db::Store,
    foundation::{config::Site, observability},
    manifest::{Maintenance, Upkeep},
    settings,
};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};
use storage::{Address, PushStore, Subscription};

/// Every minute, each DSP with a browser kept is watched, as after a restart.
pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60),
    run: pass,
};
/// What a member must hold to be told.
const USE: &str = "communications.use";
/// How many messages no longer waiting are remembered, the latest; those waiting all are.
const MOST: usize = 500;

fn failed(dsp: &str, error: &dispatch_core::Error) {
    observability::event(
        "error",
        "communications_push_failed",
        json!({"dspId": dsp, "error": error.code}),
    );
}

fn pass(state: Arc<State>, _: bool) -> Upkeep {
    Box::pin(async move {
        if webpush::vapid(&state.config).is_none() {
            return;
        }
        let pushing = state
            .read(|db| {
                let mut pushing = Vec::new();
                for dsp in db.kept_dsps()? {
                    if db.feature_enabled(&dsp, crate::FEATURE.switch.id)?
                        && db.communications_pushing(&dsp)?
                    {
                        pushing.push(dsp);
                    }
                }
                Ok(pushing)
            })
            .await;
        match pushing {
            Ok(pushing) => pushing.iter().for_each(|dsp| watch(&state, dsp)),
            Err(error) => failed("", &error),
        }
    })
}

/// Watches the DSP's conversations for what's new to push, unless they're watched already.
pub fn watch(state: &Arc<State>, dsp: &str) {
    static WATCHED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);
    if webpush::vapid(&state.config).is_none()
        || !WATCHED
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(dsp.to_owned())
    {
        return;
    }
    let (state, dsp) = (state.clone(), dsp.to_owned());
    tokio::spawn(async move { watching(&state, &dsp).await });
}

/// Reads what waits in the DSP as it first does and once each change since, pushing what's new
/// before it reads again: changes while a batch goes are read together after it.
async fn watching(state: &Arc<State>, dsp: &str) {
    // Subscribed before the first read, so a change while it reads still wakes it.
    let mut wakes = state.topics.subscribe(live::TOPIC, dsp);
    let (mut seen, mut revision) = (None, None);
    loop {
        let told = match look(state, dsp, &mut seen, &mut revision).await {
            Ok(new) if !new.is_empty() => tell(state, dsp, new).await,
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

/// What's new in what waits in the DSP since `seen`, read again only if it changed since
/// `revision`.
async fn look(
    state: &Arc<State>,
    dsp: &str,
    seen: &mut Option<Vec<String>>,
    revision: &mut Option<i64>,
) -> Result<Vec<WaitingMessage>> {
    let (shared, at, last) = (state.clone(), dsp.to_owned(), *revision);
    let read = state
        .read(move |db| {
            let now = db.communications_revision(&at)?;
            if last == Some(now) {
                return Ok(None);
            }
            Ok(Some((now, inbox::waiting(db, &shared, &at)?.latest)))
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

/// Pushes each of `new` to the browsers of each member of the DSP who may be told of it now,
/// letting go of those of a member who may not.
async fn tell(state: &Arc<State>, dsp: &str, new: Vec<WaitingMessage>) -> Result<()> {
    let at = dsp.to_owned();
    let (told, untold, code) = state
        .read(move |db| {
            let code = db.find_dsp(&at)?.code;
            let mut decided = HashMap::new();
            let (mut told, mut untold) = (Vec::new(), Vec::new());
            for subscription in db.communications_pushes(&at)? {
                let whom = (subscription.user.clone(), subscription.address);
                let tells = match decided.get(&whom) {
                    Some(tells) => *tells,
                    None => {
                        let tells = tells(db, &at, code.as_deref(), &subscription)?;
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
            .run_bookkeeping(move |db| db.drop_communications_pushes(&at, &untold))
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
                .map(|address| (address, sending::message(dsp, message, &opens(address))))
                .collect();
            told.iter().map(move |subscription| {
                (subscription.clone(), pushes[&subscription.address].clone())
            })
        })
        .collect();
    sending::send(state, dsp, pushes).await
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
/// What becomes of a push to `subscription` in the DSP `dsp` now.
pub async fn turn(state: &Arc<State>, dsp: &str, subscription: &Subscription) -> Result<Turn> {
    let (at, subscription) = (dsp.to_owned(), subscription.clone());
    state
        .read(move |db| {
            if db
                .communications_push(&at, &subscription.endpoint)?
                .as_ref()
                != Some(&subscription)
            {
                return Ok(Turn::Gone);
            }
            let code = db.find_dsp(&at)?.code;
            Ok(match tells(db, &at, code.as_deref(), &subscription)? {
                true => Turn::Goes,
                false => Turn::Untold,
            })
        })
        .await
}

/// Whether the member whose browser `subscription` names is told of the DSP's new messages: with
/// Notifications on, as one who may use Communications there now, signed in where they subscribed
/// it.
fn tells(db: &Store, dsp: &str, code: Option<&str>, subscription: &Subscription) -> Result<bool> {
    let site = match (subscription.address, code) {
        (Address::Admin, _) => Site::Admin,
        (Address::Dsp, Some(code)) => Site::Dsp(code.to_owned()),
        (Address::Dsp, None) => return Ok(false),
    };
    let user = &subscription.user;
    Ok(db.may(user, &site, dsp, USE)?
        && settings::read_member::<MessageDisplay>(db, dsp, user, &DISPLAY)?.notifications)
}

#[cfg(test)]
#[path = "../../tests/backend/push/mod.rs"]
mod tests;
