//! Telling members of new messages with Dispatch closed, or on a phone (Web Push). A browser a
//! member turns Notifications on in is subscribed with its push service and kept as theirs
//! (`storage`); each message new in what waits, as the page's alerts see it (`inbox::waiting`'s
//! `latest`: one starting a conversation waiting or arriving in one waiting, never the DSP's own),
//! goes to each of those browsers of each member who may be told of it now, titled with who wrote
//! it, saying what as the list shows it, and opening the conversation (`sending`). The browser's
//! service worker shows it, unless a window of Dispatch is focused there, which alerts itself.
//!
//! A DSP with a browser kept is watched from the minute's upkeep, or once one is kept, for as long
//! as the server runs: each change to its conversations (`live::TOPIC`) has what waits read again,
//! and what's new in it since the last read pushed, in the background. What waited when the
//! watching began tells no one. A member no longer holding Use Communications, or who turned
//! Notifications off, has their browsers let go of as a message would go to them.
pub(crate) mod fixture;
pub(crate) mod sending;
pub(crate) mod storage;
pub(crate) mod webpush;

use super::{changes::ChangeStore, display::DISPLAY, inbox, live};
use crate::api::{display::MessageDisplay, types::WaitingMessage};
use dispatch_core::{
    Result, State,
    db::Store,
    foundation::observability,
    manifest::{Maintenance, Upkeep},
    settings,
};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};
use storage::{PushStore, Subscription};

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
/// in the background.
async fn watching(state: &Arc<State>, dsp: &str) {
    // Subscribed before the first read, so a change while it reads still wakes it.
    let mut wakes = state.topics.subscribe(live::TOPIC, dsp);
    let (mut seen, mut revision) = (None, None);
    loop {
        match look(state, dsp, &mut seen, &mut revision).await {
            Ok(new) if !new.is_empty() => {
                let (state, dsp) = (state.clone(), dsp.to_owned());
                tokio::spawn(async move {
                    if let Err(error) = tell(&state, &dsp, new).await {
                        failed(&dsp, &error);
                    }
                });
            }
            Ok(_) => {}
            Err(error) => failed(dsp, &error),
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
    let (told, untold, address) = state
        .read(move |db| {
            let mut decided = HashMap::new();
            let (mut told, mut untold) = (Vec::new(), Vec::new());
            for subscription in db.communications_pushes(&at)? {
                let told_them = match decided.get(&subscription.user) {
                    Some(told) => *told,
                    None => {
                        let told = tells(db, &at, &subscription.user)?;
                        decided.insert(subscription.user.clone(), told);
                        told
                    }
                };
                if told_them {
                    told.push(subscription);
                } else {
                    untold.push(subscription.endpoint);
                }
            }
            let dsp = db.find_dsp(&at)?;
            let address = match dsp.code {
                Some(code) => format!("{}/#", db.config.dsp_url(&code)),
                None => format!("{}/#dsp/{at}/", db.config.origin),
            };
            Ok((told, untold, address))
        })
        .await?;
    if !untold.is_empty() {
        let at = dsp.to_owned();
        state
            .run_bookkeeping(move |db| db.drop_communications_pushes(&at, &untold))
            .await?;
    }
    let pushes: Vec<(Subscription, Vec<u8>)> = new
        .iter()
        .flat_map(|message| {
            let push = sending::message(message, &address);
            told.iter()
                .map(move |subscription| (subscription.clone(), push.clone()))
        })
        .collect();
    sending::send(state, dsp, pushes).await
}

/// Whether `user` is told of the DSP's new messages: with Notifications on, an active platform
/// owner, who holds every permission, or a member whose role holds Use Communications.
fn tells(db: &Store, dsp: &str, user: &str) -> Result<bool> {
    let holds = db.active_platform_owner(user)?.is_some()
        || db.grant(user, dsp)?.is_some_and(|grant| {
            grant.owner || grant.permissions.iter().any(|permission| permission == USE)
        });
    Ok(holds && settings::read_member::<MessageDisplay>(db, dsp, user, &DISPLAY)?.notifications)
}

#[cfg(test)]
#[path = "../../tests/backend/push/mod.rs"]
mod tests;
