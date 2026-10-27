//! Messaging a group. Someone at the DSP picks whom, and writes: the group is kept with its
//! message, and the message goes to each of its people in the background, in their own direct
//! message with the app (`chat:write`, opened first if need be, `im:write`), under the sender's
//! full name and photo (`chat:write.customize`), as an answer does, its photos uploaded once to
//! each workspace (`files:write`). One at a time, at most one a second in each workspace, as Slack
//! asks of an app posting messages; Slack asking Dispatch to slow down (429, Retry-After) is
//! waited out as long as it says, up to two minutes, and beyond that tried again two minutes
//! later, uncounted. A try that fails is tried again after a minute, five and fifteen, then given
//! up: not sent. One to a workspace where Communications no longer works, or a member no longer
//! there, isn't sent at all.
//!
//! Each message to each person is kept until it's sent or given up, so sending picks up where it
//! was after a restart, from the maintenance pass every minute. A try that may have posted before
//! the server stopped, and kept nothing, first looks for its post in their direct message
//! (`im:history`) by the metadata each carries, and keeps that one rather than send a second.
use super::{
    one,
    storage::{Delivered, Due, GroupMessage, GroupStore, Member, Writing},
};
use crate::{
    api::types::{Group, GroupEntry},
    backend::{
        changes::ChangeStore,
        compose,
        files::{self, FileStore},
        initials, live, reply, sender,
        storage::{CommunicationsStore, DOMAIN},
    },
    integrations::messages::{ID, incoming},
};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::{Store, now},
    ensure,
    manifest::{Maintenance, Upkeep},
    server::http::route::Dsp,
};
use dispatch_slack::kit::{self, Bot, Queue};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::time::Instant;

/// Every minute, what's due to go in each DSP: what a restart left, and what's tried again.
pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60),
    run: pass,
};
/// The most people a group may have: as many as are listed to message.
pub const PEOPLE_MOST: usize = 2000;
/// The longest a group's name may be, in characters.
pub const NAME_MOST: usize = 100;
/// How far apart two posts to one workspace go: Slack takes about one message a second.
const PACE: Duration = Duration::from_secs(1);
/// How long a try holds its message, in milliseconds: longer than uploading its photos and
/// waiting out Slack asking to slow down take.
const CLAIM_MS: i64 = 15 * 60 * 1000;
/// How long a failed try waits before the next, in milliseconds: given up after the last.
const RETRIES_MS: &[i64] = &[60 * 1000, 5 * 60 * 1000, 15 * 60 * 1000];
/// How long a message waits, uncounted, while Slack asks Dispatch to slow down for longer than a
/// patient bot waits itself, in milliseconds: as long again.
const SLOWED_MS: i64 = 2 * 60 * 1000;
/// What each post carries, as Slack's message metadata, to be found again by.
const EVENT: &str = "dispatch_group_message";
/// How a failure is reported.
const FAILED: &str = "communications_group_send_failed";

/// Makes a group of `people`, each a workspace and a member's ID there, named `name`, by the
/// member `c`, with what they wrote and the photos they added, due to go to each: the group, as
/// the list reads it. Each person gets it once: two accounts of one person are one.
#[allow(clippy::too_many_arguments)]
pub async fn create(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    name: &str,
    people: &[(String, String)],
    written: &str,
    photos: &[String],
) -> Result<Group> {
    let written = compose::checked(written, photos, compose::SLACK)?.to_owned();
    let name = name.trim().to_owned();
    ensure(
        !name.is_empty()
            && name.chars().count() <= NAME_MOST
            && !people.is_empty()
            && people.len() <= PEOPLE_MOST,
        "invalid_input",
        400,
    )?;
    let dsp = c.dsp.id.clone();
    let (shared, c, asked, photos) = (state.clone(), c.clone(), people.to_vec(), photos.to_vec());
    let group = state
        .run_scoped(dsp.clone(), DOMAIN, move |db| {
            let c = access.revalidate(db, &c)?;
            let dsp = c.dsp.id.as_str();
            let members = reachable(db, &shared, dsp, &asked)?;
            let user = &c.auth.user;
            let letters = initials::of(&user.first_name, &user.last_name);
            let writing = Writing {
                text: &written,
                by: &user.id,
                name: &sender::name(user),
                initials: letters.as_deref(),
                photos: &photos,
            };
            let kept = db.create_communications_group(dsp, &name, &members, &writing)?;
            one(db, &shared, dsp, &kept.id)
        })
        .await?;
    live::changed(state, &dsp);
    Ok(group)
}

/// `asked`, each a workspace and a member's ID there, as a group's people: each an active member
/// of a workspace where Communications works, under the name Slack gives them, each person once.
fn reachable(
    db: &Store,
    state: &State,
    dsp: &str,
    asked: &[(String, String)],
) -> Result<Vec<Member>> {
    let on = kit::workspaces_on(db, dsp, ID)?;
    let members: HashMap<(String, String), String> = kit::members(db, dsp)?
        .into_iter()
        .filter(|(team, member)| member.active && on.contains(team))
        .map(|(team, member)| ((team, member.user), member.name))
        .collect();
    let everyone = dispatch_core::manifest::people::everyone(db, state, dsp)?;
    let mut seen = HashSet::new();
    let mut listed = Vec::new();
    for (team, user) in asked {
        let name = members
            .get(&(team.clone(), user.clone()))
            .ok_or_else(|| Error::new("communications_contact_not_found", 404))?;
        let id = kit::member_id(team, user);
        let person = everyone
            .holding(kit::SOURCE, &id)
            .map(|person| person.code.clone());
        if seen.insert(person.unwrap_or(id)) {
            listed.push(Member {
                team: team.clone(),
                user: user.clone(),
                name: name.clone(),
            });
        }
    }
    Ok(listed)
}

/// Writes the group `id` what the member `c` wrote, with the photos they added, due to go to the
/// same people: the message, as the group's page reads it.
pub async fn again(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    id: &str,
    written: &str,
    photos: &[String],
) -> Result<GroupEntry> {
    let written = compose::checked(written, photos, compose::SLACK)?.to_owned();
    let dsp = c.dsp.id.clone();
    let (shared, c, group, photos) = (state.clone(), c.clone(), id.to_owned(), photos.to_vec());
    let entry = state
        .run_scoped(dsp.clone(), DOMAIN, move |db| {
            let c = access.revalidate(db, &c)?;
            let dsp = c.dsp.id.as_str();
            let user = &c.auth.user;
            let letters = initials::of(&user.first_name, &user.last_name);
            let writing = Writing {
                text: &written,
                by: &user.id,
                name: &sender::name(user),
                initials: letters.as_deref(),
                photos: &photos,
            };
            let message = db.add_communications_group_message(dsp, &group, &writing)?;
            let opened = super::opened(db, &shared, dsp, &group, None)?;
            opened
                .entries
                .into_iter()
                .find(|entry| entry.id == message)
                .ok_or_else(|| Error::new("communications_group_not_found", 404))
        })
        .await?;
    live::changed(state, &dsp);
    Ok(entry)
}

/// Sends what's due in the DSP soon, in the background.
pub fn send_soon(state: &Arc<State>, dsp: &str) {
    let (state, dsp) = (state.clone(), dsp.to_owned());
    tokio::spawn(async move { drain(&state, &dsp).await });
}
/// Each DSP sends what's due on its own, so one with much to send, or waiting on Slack, holds up
/// no other.
fn pass(state: Arc<State>, _: bool) -> Upkeep {
    Box::pin(async move {
        for dsp in kit::dsps(&state, FAILED).await {
            send_soon(&state, &dsp);
        }
    })
}

/// Whether the DSP's messages are wanted sent, and the sending of them, which runs once at a time.
#[derive(Default)]
struct Sending {
    wanted: AtomicBool,
    running: tokio::sync::Mutex<()>,
}
fn sending(dsp: &str) -> Arc<Sending> {
    static SENDING: LazyLock<Mutex<HashMap<String, Arc<Sending>>>> =
        LazyLock::new(Default::default);
    SENDING
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .entry(dsp.to_owned())
        .or_default()
        .clone()
}
/// Sends what's due in the DSP, unless that's under way already, in which case it goes on to
/// what was asked for since: asked for while it was finishing, it goes on too.
pub(crate) async fn drain(state: &Arc<State>, dsp: &str) {
    let sending = sending(dsp);
    sending.wanted.store(true, Ordering::SeqCst);
    loop {
        let Ok(running) = sending.running.try_lock() else {
            return;
        };
        if !sending.wanted.swap(false, Ordering::SeqCst) {
            drop(running);
            if sending.wanted.load(Ordering::SeqCst) {
                continue;
            }
            return;
        }
        due(state, dsp).await;
    }
}
/// Sends the DSP's messages due now, one at a time, telling the pages waiting of each that came to
/// something they show, until none is due, or one comes to nothing, which the next pass tries
/// again.
async fn due(state: &Arc<State>, dsp: &str) {
    let mut last = None;
    loop {
        let at = dsp.to_owned();
        let read = state
            .read(move |db| {
                let first = db.due_communications_deliveries(&at, now(), 1)?;
                Ok((first.into_iter().next(), db.communications_revision(&at)?))
            })
            .await;
        let (first, before) = match read {
            Ok(read) => read,
            Err(error) => return kit::failed(FAILED, &error),
        };
        let Some(first) = first else {
            return;
        };
        let key = (first.message, first.team, first.user);
        if last.as_ref() == Some(&key) {
            return;
        }
        kit::due::<Deliveries>(state, dsp).await;
        let at = dsp.to_owned();
        let after = state.read(move |db| db.communications_revision(&at)).await;
        if after.ok() != Some(before) {
            live::changed(state, dsp);
        }
        last = Some(key);
    }
}

/// Waits its turn to post to the DSP's workspace `team`: a second after the last.
async fn pace(dsp: &str, team: &str) {
    static NEXT: LazyLock<Mutex<HashMap<(String, String), Instant>>> =
        LazyLock::new(Default::default);
    let turn = {
        let mut next = NEXT.lock().unwrap_or_else(|poison| poison.into_inner());
        let now = Instant::now();
        let slot = next.entry((dsp.to_owned(), team.to_owned())).or_insert(now);
        let turn = (*slot).max(now);
        *slot = turn + PACE;
        turn
    };
    tokio::time::sleep_until(turn).await;
}

/// What trying to send a group message to one of its people came to.
pub enum Outcome {
    /// It's in their direct message, under their name in Slack now.
    Sent {
        name: String,
        channel: String,
        ts: String,
        at: i64,
        photos: Vec<(String, String)>,
    },
    /// Tried again at `at`, counted as a failure or not; `unposted` once it's known nothing of it
    /// is in Slack, so the next try needn't look for an earlier post.
    Later {
        reason: String,
        at: i64,
        counted: bool,
        unposted: bool,
    },
    Failed {
        reason: String,
    },
}

/// Each group message going to each of its people, while Communications is on for the DSP.
struct Deliveries;
impl Queue for Deliveries {
    type Item = Due;
    /// Since when an earlier try may have posted it, if one may have.
    type Claimed = Option<i64>;
    type Outcome = Outcome;
    // One at a time, so the pages waiting hear of each as it goes.
    const BATCH: usize = 1;
    const CLAIM_MS: i64 = CLAIM_MS;
    const FAILED: &'static str = FAILED;

    fn due(db: &Store, dsp: &str, at: i64, most: usize) -> Result<Vec<Due>> {
        if !db.feature_enabled(dsp, crate::FEATURE.name)? {
            return Ok(Vec::new());
        }
        db.due_communications_deliveries(dsp, at, most)
    }
    fn claim(
        db: &Store,
        dsp: &str,
        item: &Due,
        at: i64,
        until: i64,
    ) -> Result<Option<Option<i64>>> {
        db.claim_communications_delivery(dsp, item, at, until)
    }
    async fn try_one(state: &Arc<State>, dsp: &str, item: &Due, since: Option<i64>) -> Outcome {
        match deliver(state, dsp, item, since).await {
            Ok(outcome) => outcome,
            Err(error) => failure(item, &error),
        }
    }
    fn finish(db: &Store, dsp: &str, item: &Due, outcome: &Outcome, _: i64) -> Result<()> {
        match outcome {
            Outcome::Sent {
                name,
                channel,
                ts,
                at,
                photos,
            } => {
                let delivered = Delivered {
                    channel,
                    ts,
                    at: *at,
                    photos,
                };
                db.deliver_communications_message(dsp, item, name, &delivered)
            }
            Outcome::Later {
                reason,
                at,
                counted,
                unposted,
            } => db.retry_communications_delivery(dsp, item, *at, reason, *counted, *unposted),
            Outcome::Failed { reason } => db.fail_communications_delivery(dsp, item, reason),
        }
    }
}

/// Sends the group message to one of its people: in their direct message, opened first if need
/// be; or, when an earlier try `since` then may have, keeps that post if it's there.
async fn deliver(state: &Arc<State>, dsp: &str, item: &Due, since: Option<i64>) -> Result<Outcome> {
    let (at, due) = (dsp.to_owned(), item.clone());
    let found = state
        .read(move |db| {
            let message = db
                .communications_group_message(&at, &due.message)?
                .ok_or_else(|| Error::new("communications_group_not_found", 404))?;
            let on = kit::workspaces_on(db, &at, ID)?.contains(&due.team);
            let member = kit::members(db, &at)?
                .into_iter()
                .find(|(team, member)| *team == due.team && member.user == due.user)
                .map(|(_, member)| member)
                .filter(|member| member.active);
            let channel = db
                .communications_conversation_with(&at, &due.team, &due.user)?
                .map(|conversation| conversation.channel);
            // Their email address as it is now, for their Slack photo.
            let email = db
                .members(&at)?
                .into_iter()
                .find(|each| each.user_id == message.sent_by)
                .map(|each| each.email);
            let photos = db.communications_group_files(&at, std::slice::from_ref(&due.message))?;
            let uploaded = db.communications_group_uploads(&at, &due.message, &due.team)?;
            Ok((message, on, member, channel, email, photos, uploaded))
        })
        .await?;
    let (message, on, member, channel, email, photos, uploaded) = found;
    if !on {
        return Ok(Outcome::Failed {
            reason: "communications_off".into(),
        });
    }
    let Some(member) = member else {
        return Ok(Outcome::Failed {
            reason: "communications_contact_not_found".into(),
        });
    };
    pace(dsp, &item.team).await;
    // Nobody waits on it: Slack asking to slow down is waited out as long as it says.
    let bot = Bot::new(state, dsp, &item.team).patient();
    let channel = match channel {
        Some(channel) => channel,
        None => reply::opened(&bot, &item.user).await?,
    };
    let mut shown: Vec<(String, String, String)> = Vec::new();
    for photo in &photos {
        let slack_id = match uploaded.iter().find(|(file, _)| *file == photo.id) {
            Some((_, slack_id)) => slack_id.clone(),
            None => upload(state, &bot, &message, photo).await?,
        };
        shown.push((photo.id.clone(), slack_id, photo.name.clone()));
    }
    let sent = |ts: String| -> Result<Outcome> {
        Ok(Outcome::Sent {
            name: member.name.clone(),
            channel: channel.clone(),
            at: incoming::millis(&ts).ok_or_else(|| Error::new("slack_refused", 502))?,
            ts,
            photos: shown
                .iter()
                .map(|(photo, slack_id, _)| (photo.clone(), slack_id.clone()))
                .collect(),
        })
    };
    if let Some(since) = since
        && let Some(ts) = posted(&bot, &channel, &message.id, since).await?
    {
        return sent(ts);
    }
    let identity = sender::named(
        state,
        dsp,
        &item.team,
        &message.sender_name,
        email.as_deref(),
        message.sender_initials.as_deref(),
    )
    .await;
    let mut args = reply::message(&channel, &message.text, &shown, &identity);
    args["metadata"] = json!({"event_type": EVENT, "event_payload": {"message": message.id}});
    let answer = match reply::post(&bot, &args, !shown.is_empty()).await {
        // Slack took nothing, and no earlier try's post is there: the next needn't look for one.
        Err(error) if error.code == "slack_rate_limited" => {
            return Ok(Outcome::Later {
                reason: error.code,
                at: now() + SLOWED_MS,
                counted: false,
                unposted: true,
            });
        }
        answer => answer?,
    };
    sent(answer["ts"].as_str().unwrap_or_default().to_owned())
}

/// Uploads the group message's photo to the bot's workspace, shared nowhere, and keeps Slack's
/// ID of it there, for each of its people there to be shown it: that ID.
async fn upload(
    state: &Arc<State>,
    bot: &Bot,
    message: &GroupMessage,
    photo: &files::File,
) -> Result<String> {
    let sha256 = photo.sha256.as_deref().unwrap_or_default();
    let bytes = files::read(state, &bot.dsp, sha256).await?;
    let slack_id = bot.upload(&photo.name, photo.size as u64, bytes).await?;
    let (dsp, team, of, file, kept) = (
        bot.dsp.clone(),
        bot.team.clone(),
        message.id.clone(),
        photo.id.clone(),
        slack_id.clone(),
    );
    state
        .run_scoped(bot.dsp.clone(), DOMAIN, move |db| {
            db.add_communications_group_upload(&dsp, &of, &team, &file, &kept)
        })
        .await?;
    Ok(slack_id)
}

/// The `ts` of the post of the group message `message` in the direct message `channel`, posted
/// since `since` (a minute before, for clocks that differ), if it's there.
async fn posted(bot: &Bot, channel: &str, message: &str, since: i64) -> Result<Option<String>> {
    let read = bot
        .call(
            "conversations.history",
            &json!({"channel": channel, "oldest": incoming::ts(since - 60_000),
                "include_all_metadata": true, "limit": 200}),
        )
        .await?;
    let found = read["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|post| {
            post["metadata"]["event_type"] == EVENT
                && post["metadata"]["event_payload"]["message"] == message
        })
        .and_then(|post| post["ts"].as_str().map(str::to_owned));
    Ok(found)
}

/// What `error` makes of trying `item`: waited out while Slack asks Dispatch to slow down, given
/// up where it can't go, or tried again later, and given up after the last try.
fn failure(item: &Due, error: &Error) -> Outcome {
    let code = error.code.clone();
    if code == "slack_rate_limited" {
        return Outcome::Later {
            reason: code,
            at: now() + SLOWED_MS,
            counted: false,
            unposted: false,
        };
    }
    let gone = [
        "communications_off",
        "communications_contact_not_found",
        "slack_needs_update",
        "slack_connection_broken",
        "slack_not_connected",
    ];
    match RETRIES_MS.get(item.attempts.max(0) as usize) {
        Some(after) if !gone.contains(&code.as_str()) => Outcome::Later {
            reason: code,
            at: now() + after,
            counted: true,
            unposted: false,
        },
        _ => Outcome::Failed { reason: code },
    }
}
