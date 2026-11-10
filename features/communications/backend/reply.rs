//! Answering from Dispatch. A message someone at the DSP writes, with any photos they add, goes to
//! the member's direct message with the app in Slack (`chat:write`), under their own name and
//! photo (`chat:write.customize`, as `sender` says), its photos uploaded first, shared in no
//! conversation but shown in the message (`files:write`). Someone at the DSP may open a direct
//! message with any member of a workspace where Communications works, to message them first
//! (`im:write`). Slack is called outside the database, and the member's access checked again
//! before anything is kept, or, for a message, before it goes to Slack: what Slack has is kept.
//!
//! A message is noted as it's checked, before it goes to Slack, held by the try posting it, and
//! its post carries the note's ID as Slack's message metadata. Once Slack has it, it's kept,
//! waiting its turn at the database rather than turned away, and the note cleared. One Slack may
//! have taken that wasn't kept, as when the server stopped between, is looked for in the direct
//! message (`im:history`) by its note's ID, and kept if it's there: by the maintenance pass every
//! minute once no try holds it, which clears the note if it isn't, or at once when the same member
//! sends the same message there again, which then isn't posted a second time, and is otherwise
//! tried again under the same note. Each claims the note first, a first try noting it in the same
//! write that finds none, so a try begun while another of it is under way is refused. A note stays
//! until it's settled so, and no other message takes its photos meanwhile.
use super::{
    compose::{SLACK, checked, photo_kind},
    conversations, files,
    files::{File, FileStore},
    gifs::{self, Sending},
    live,
    people::People,
    sender::{self, Identity},
    storage::{
        DOMAIN,
        conversations::{Conversation as StoredConversation, ConversationStore},
        messages::Sent,
        replies::{Noted, ReplyStore},
    },
    text,
};
use crate::{
    api::types::{CommunicationsUpload, Conversation, Message, MessageGif},
    integrations::messages::{ID, incoming},
};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::{Store, now},
    ensure,
    foundation::crypto,
    manifest::{Maintenance, Upkeep},
    server::http::route::Dsp,
    server::http::upload::Upload,
};
use dispatch_slack::kit::{self, Bot};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};

/// Every minute, each message that may have gone to Slack without being kept, looked for there.
pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60),
    run: pass,
};
/// How long a message, and its photos, are held for a try while it goes to Slack, in
/// milliseconds: longer than its uploads and post take, Slack asking it to slow down included.
const SENDING_MS: i64 = 10 * 60 * 1000;
/// What each message's post carries, as Slack's message metadata, to be found again by.
const EVENT: &str = "dispatch_reply";
/// How a failure is reported.
const FAILED: &str = "communications_reply_failed";

/// Opens a direct message with the workspace `team`'s member `user`, for the member `c` to
/// message them first: their conversation, made if it's new.
pub async fn open(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    team: &str,
    user: &str,
) -> Result<Conversation> {
    let (dsp, at, of) = (c.dsp.id.clone(), team.to_owned(), user.to_owned());
    let (member, kept) = state
        .read(move |db| {
            ensure(
                kit::workspaces_on(db, &dsp, ID)?.contains(&at),
                "communications_off",
                409,
            )?;
            let member = kit::members(db, &dsp)?
                .into_iter()
                .find(|(team, member)| *team == at && member.user == of && member.active)
                .map(|(_, member)| member)
                .ok_or_else(|| Error::new("communications_contact_not_found", 404))?;
            Ok((member, db.communications_conversation_with(&dsp, &at, &of)?))
        })
        .await?;
    let channel = match kept {
        Some(kept) => kept.channel,
        None => opened(&Bot::new(state, &c.dsp.id, team), user).await?,
    };
    let (shared, c, team, user) = (state.clone(), c.clone(), team.to_owned(), user.to_owned());
    state
        .run_scoped(c.dsp.id.clone(), DOMAIN, move |db| {
            let c = access.revalidate(db, &c)?;
            let kept = db.open_communications_conversation(
                &c.dsp.id,
                &team,
                &user,
                &channel,
                &member.name,
            )?;
            conversations::one(db, &shared, &c.dsp.id, &kept.id)
        })
        .await
}
/// The direct message the app has with the member `user` of the bot's workspace, as Slack opens
/// it.
pub(crate) async fn opened(bot: &Bot, user: &str) -> Result<String> {
    let answer = bot
        .call("conversations.open", &json!({"users": user}))
        .await?;
    answer["channel"]["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new("slack_refused", 502))
}

/// Sends what the member `c` wrote in the conversation `id`, with the photos they added, or a GIF
/// in their place (`adding`): the message, as the page reads it.
pub async fn send(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    id: &str,
    written: &str,
    (photos, gif): (&[String], Option<&MessageGif>),
) -> Result<Message> {
    let written = checked(written, photos, gif.is_some(), SLACK)?;
    let (dsp, at, by, chosen, gif) = (
        c.dsp.id.clone(),
        id.to_owned(),
        c.auth.user.id.clone(),
        photos.to_vec(),
        gif.cloned(),
    );
    // Its photos, and its note, are claimed for it, so another message can't take them as it
    // goes.
    let until = now() + SENDING_MS;
    let (asked, name) = (written.to_owned(), sender::name(&c.auth.user));
    let (conversation, uploads, noted, earlier) = state
        .run_scoped(c.dsp.id.clone(), DOMAIN, move |db| {
            begin(db, &dsp, &at, (&by, &name), (&asked, gif), &chosen, until)
        })
        .await?;
    let sent = deliver(
        state,
        c,
        access,
        &conversation,
        written,
        &uploads,
        (noted.clone(), earlier),
    )
    .await;
    if sent.is_err() {
        // Slack didn't take it, or may have: its photos are free to send again, and its note to
        // try again or settle. Should letting them go fail, the claims end on their own.
        let (dsp, chosen) = (c.dsp.id.clone(), photos.to_vec());
        let _ = state
            .run_scoped(c.dsp.id.clone(), DOMAIN, move |db| {
                db.release_communications_reply(&dsp, &noted.id, noted.claimed_until)?;
                db.release_communications_uploads(&dsp, &chosen)
            })
            .await;
    }
    sent
}
/// Begins a try of what the member `by` wrote, with the GIF they chose, if any, under the name
/// `name`, in the conversation `id`, with the photos `chosen`, holding them and its note until `until`: the conversation, the
/// photos, the note, and whether it was noted before. The same message sent there before, should
/// Slack have it unkept, is that note, claimed, and refused while another try of it is under way;
/// else it's noted now, in the same write, so a try of it begun meanwhile is refused too.
fn begin(
    db: &Store,
    dsp: &str,
    id: &str,
    (by, name): (&str, &str),
    (text, gif): (&str, Option<MessageGif>),
    chosen: &[String],
    until: i64,
) -> Result<(StoredConversation, Vec<File>, Noted, bool)> {
    let conversation = conversations::found(db, dsp, id)?;
    ensure(
        kit::workspaces_on(db, dsp, ID)?.contains(&conversation.team),
        "communications_off",
        409,
    )?;
    let earlier = db
        .noted_communications_replies(dsp, Some(id), i64::MAX)?
        .into_iter()
        .find(|noted| {
            noted.by == by
                && noted.text == text
                && noted.gif == gif
                && noted.photos.iter().map(|(photo, _)| photo).eq(chosen)
        });
    let of = earlier.as_ref().map(|noted| noted.id.as_str());
    let uploads = db.claim_communications_uploads(dsp, by, chosen, until, of)?;
    let Some(earlier) = earlier else {
        let noted = Noted {
            id: crypto::id("rpl")?,
            conversation: id.to_owned(),
            text: text.to_owned(),
            by: by.to_owned(),
            name: name.to_owned(),
            photos: chosen
                .iter()
                .map(|photo| (photo.clone(), String::new()))
                .collect(),
            gif,
            posting_since: now(),
            claimed_until: until,
        };
        db.note_communications_reply(dsp, &noted)?;
        return Ok((conversation, uploads, noted, false));
    };
    let claimed = db.claim_communications_reply(dsp, &earlier.id, now(), until)?;
    ensure(claimed, "communications_reply_sending", 409)?;
    let noted = Noted {
        claimed_until: until,
        ..earlier
    };
    Ok((conversation, uploads, noted, true))
}
/// Sends what the member `c` wrote in `conversation`, with the photos `uploads` they added, under
/// the note `noted`, then keeps it: the message, as the page reads it. Noted `earlier`, and in
/// Slack unkept, that post is kept instead; not in Slack, it's tried again under the same note.
async fn deliver(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    conversation: &StoredConversation,
    written: &str,
    uploads: &[File],
    (noted, earlier): (Noted, bool),
) -> Result<Message> {
    let dsp = c.dsp.id.as_str();
    let bot = Bot::new(state, dsp, &conversation.team);
    let channel = &conversation.channel;
    if earlier
        && let Some(ts) = posted(&bot, channel, EVENT, &noted.id, noted.posting_since).await?
    {
        let message = keep(state, dsp, &noted, &ts).await?;
        live::stopped(state, dsp, &conversation.id, &c.auth.user.id);
        return Ok(message);
    }
    let mut shown = Vec::new();
    for upload in uploads {
        let sha256 = upload.sha256.as_deref().unwrap_or_default();
        let bytes = files::read(state, dsp, sha256).await?;
        let slack_id = bot.upload(&upload.name, upload.size as u64, bytes).await?;
        shown.push((upload.id.clone(), slack_id, upload.name.clone()));
    }
    let identity = sender::identity(state, dsp, &conversation.team, &c.auth.user).await;
    let noted = Noted {
        name: identity.name.clone(),
        photos: shown
            .iter()
            .map(|(photo, slack_id, _)| (photo.clone(), slack_id.clone()))
            .collect(),
        ..noted
    };
    let mut args = message(channel, written, &shown, &identity);
    if let Some(gif) = &noted.gif {
        with_gif(&mut args, written, gif);
    }
    args["metadata"] = tag(EVENT, &noted.id);
    // Checked again, and noted as it goes, before it goes to Slack: once Slack has it, it's kept.
    let (asked, note) = (c.clone(), noted.clone());
    let c = state
        .run_bookkeeping(move |db| {
            let c = access.revalidate(db, &asked)?;
            db.note_communications_reply(&c.dsp.id, &note)?;
            Ok(c)
        })
        .await?;
    let posted = post(&bot, &args, !shown.is_empty()).await?;
    let ts = posted["ts"].as_str().unwrap_or_default();
    let message = keep(state, &c.dsp.id, &noted, ts).await?;
    live::stopped(state, &c.dsp.id, &conversation.id, &c.auth.user.id);
    Ok(message)
}
/// Keeps the message `noted` as Slack took it, `ts`, waiting its turn at the database rather
/// than turned away: the message, as the page reads it.
async fn keep(state: &Arc<State>, dsp: &str, noted: &Noted, ts: &str) -> Result<Message> {
    let at = incoming::millis(ts).ok_or_else(|| Error::new("slack_refused", 502))?;
    let (shared, of, noted, ts) = (state.clone(), dsp.to_owned(), noted.clone(), ts.to_owned());
    let message = State::patiently(state.run_scoped(dsp, DOMAIN, move |db| {
        let sent = Sent {
            ts: &ts,
            text: &noted.text,
            at,
            by: &noted.by,
            name: &noted.name,
            photos: &noted.photos,
            gif: noted.gif.as_ref(),
        };
        let kept = db.keep_communications_reply(&of, &noted.conversation, &sent, &noted.id)?;
        let people = People::read(db, &shared, &of)?;
        Ok(conversations::messages(db, &people, &of, vec![kept])?.remove(0))
    }))
    .await?;
    live::changed(state, dsp);
    Ok(message)
}

/// Each DSP's messages that may have gone to Slack without being kept, looked for there.
fn pass(state: Arc<State>, _: bool) -> Upkeep {
    Box::pin(async move {
        for dsp in kit::dsps(&state, FAILED).await {
            if let Err(error) = settle(&state, &dsp).await {
                kit::failed(FAILED, Some(&dsp), &error);
            }
        }
    })
}
/// Keeps each message noted in the DSP that no try holds and Slack has, and clears the note of each
/// it hasn't, while Communications works in its workspace; one where it doesn't waits until it
/// does, and one whose conversation is gone is cleared. Each is claimed first, so no try of it
/// runs meanwhile.
async fn settle(state: &Arc<State>, dsp: &str) -> Result<()> {
    let at = dsp.to_owned();
    let unheld = state
        .read(move |db| {
            Ok(!db
                .noted_communications_replies(&at, None, now())?
                .is_empty())
        })
        .await?;
    if !unheld {
        return Ok(());
    }
    let at = dsp.to_owned();
    let noted = state
        .run_bookkeeping(move |db| {
            let noted = db.claim_communications_replies(&at, now(), now() + SENDING_MS)?;
            let on = kit::workspaces_on(db, &at, ID)?;
            let ids: Vec<String> = noted.iter().map(|each| each.conversation.clone()).collect();
            let conversations: HashMap<String, StoredConversation> = db
                .communications_conversations(&at, &ids)?
                .into_iter()
                .map(|conversation| (conversation.id.clone(), conversation))
                .collect();
            Ok(noted
                .into_iter()
                .map(|noted| {
                    let conversation = conversations.get(&noted.conversation).cloned();
                    let working = conversation
                        .as_ref()
                        .is_some_and(|conversation| on.contains(&conversation.team));
                    (noted, conversation, working)
                })
                .collect::<Vec<_>>())
        })
        .await?;
    for (noted, conversation, working) in noted {
        let found = match conversation {
            // Its conversation forgotten, as a release before this one forgets a workspace's
            // conversations with no message: nothing is left to keep it in.
            None => None,
            Some(_) if !working => continue,
            Some(conversation) => {
                let bot = Bot::new(state, dsp, &conversation.team).patient();
                let (channel, since) = (&conversation.channel, noted.posting_since);
                match posted(&bot, channel, EVENT, &noted.id, since).await {
                    Ok(found) => found,
                    Err(error) => {
                        kit::failed(FAILED, Some(dsp), &error);
                        continue;
                    }
                }
            }
        };
        match found {
            Some(ts) => keep(state, dsp, &noted, &ts).await.map(drop)?,
            None => {
                let (at, id, held) = (dsp.to_owned(), noted.id, noted.claimed_until);
                state
                    .run_bookkeeping(move |db| db.clear_communications_reply(&at, &id, held))
                    .await?
            }
        }
    }
    Ok(())
}

/// What a post carries, as Slack's message metadata, to be found again by: the kind of post
/// `event`, and the ID `id` of what it posts.
pub(crate) fn tag(event: &str, id: &str) -> Value {
    json!({"event_type": event, "event_payload": {"message": id}})
}
/// The `ts` of the post tagged as the kind `event` of `id` in the direct message `channel`,
/// posted since `since` (a minute before, for clocks that differ), if it's there: every page
/// Slack has of them is read until it's found.
pub(crate) async fn posted(
    bot: &Bot,
    channel: &str,
    event: &str,
    id: &str,
    since: i64,
) -> Result<Option<String>> {
    let mut cursor = Value::Null;
    loop {
        let page = bot
            .call(
                "conversations.history",
                &json!({"channel": channel, "oldest": incoming::ts(since - 60_000),
                    "include_all_metadata": true, "limit": 200, "cursor": cursor}),
            )
            .await?;
        let found = page["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|post| {
                post["metadata"]["event_type"] == event
                    && post["metadata"]["event_payload"]["message"] == id
            })
            .and_then(|post| post["ts"].as_str().map(str::to_owned));
        if found.is_some() {
            return Ok(found);
        }
        cursor = match page["response_metadata"]["next_cursor"].as_str() {
            Some(next) if !next.is_empty() => json!(next),
            _ => return Ok(None),
        };
    }
}
/// What `chat.postMessage` is asked in the direct message `channel`: what was written, as
/// written, with the photos `shown` (each by its ID, Slack's ID of it and its name), under
/// `identity`.
pub(crate) fn message(
    channel: &str,
    written: &str,
    shown: &[(String, String, String)],
    identity: &Identity,
) -> Value {
    let mut args = json!({
        "channel": channel,
        "text": if written.is_empty() {
            shown.iter().map(|(_, _, name)| text::escaped(name)).collect::<Vec<_>>().join(", ")
        } else {
            text::escaped(written)
        },
        "username": identity.name,
        "icon_url": identity.icon,
        // As written: Slack's markup, as `*` for bold, isn't read in it.
        "mrkdwn": false,
        "unfurl_links": false,
    });
    if !shown.is_empty() {
        args["blocks"] = blocks(written, shown);
    }
    args
}
/// `args`, a message's, with `gif` in it, after what was `written`: shown as an image from
/// KLIPY's address, which Slack fetches, or, sending links, that address after it.
fn with_gif(args: &mut Value, written: &str, gif: &MessageGif) {
    match gifs::SENDING {
        Sending::File => {
            let shown = gifs::shared(gif);
            let alt = if gif.title.is_empty() {
                "GIF"
            } else {
                &gif.title
            };
            let mut blocks = Vec::new();
            if !written.is_empty() {
                blocks.push(
                    json!({"type": "section", "text": {"type": "plain_text", "text": written}}),
                );
            }
            blocks.push(json!({"type": "image", "image_url": shown.url, "alt_text": alt}));
            args["text"] = json!(if written.is_empty() {
                text::escaped(alt)
            } else {
                text::escaped(written)
            });
            args["blocks"] = Value::Array(blocks);
        }
        Sending::Link => args["text"] = json!(text::escaped(&gifs::linked(written, gif))),
    }
}
/// A message's blocks, for one with photos: what was written, as written, then each photo, by
/// Slack's ID of it.
fn blocks(written: &str, shown: &[(String, String, String)]) -> Value {
    let mut blocks = Vec::new();
    if !written.is_empty() {
        blocks.push(json!({"type": "section", "text": {"type": "plain_text", "text": written}}));
    }
    for (_, slack_id, name) in shown {
        blocks.push(json!({"type": "image", "slack_file": {"id": slack_id}, "alt_text": name}));
    }
    Value::Array(blocks)
}
/// Posts a message as the bot. Slack may not show a photo just uploaded until it has looked at
/// it, so one with photos is tried again, twice, a second apart, when Slack refuses it.
pub(crate) async fn post(bot: &Bot, args: &Value, photos: bool) -> Result<Value> {
    let mut tries = 0;
    loop {
        match bot.call("chat.postMessage", args).await {
            Err(error) if photos && error.code == "slack_refused" && tries < 2 => {
                tries += 1;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            answer => return answer,
        }
    }
}

/// Keeps a photo the member `c` adds for a message not sent yet, named `name`, of the kind
/// `mime`: as the page reads it. One that isn't a photo Slack shows is refused, by how its bytes
/// begin. Their access is checked again in the write that keeps it, once its bytes are in: one
/// refused, or whose access ended meanwhile, keeps none of it.
pub async fn upload(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    name: &str,
    mime: &str,
    upload: Upload,
) -> Result<CommunicationsUpload> {
    let begins = photo_kind(mime).ok_or_else(|| Error::new("communications_photo_kind", 415))?;
    let arrived = files::BYTES
        .receive(state, &c.dsp.id, upload, SLACK.largest)
        .await?;
    ensure(
        arrived.head(begins.len()).await? == begins,
        "communications_photo_kind",
        415,
    )?;
    let (c, name, mime) = (c.clone(), name.to_owned(), mime.to_owned());
    state
        .run_scoped(c.dsp.id.clone(), DOMAIN, move |db| {
            let c = access.revalidate(db, &c)?;
            let (dsp, by, size) = (&c.dsp.id, &c.auth.user.id, arrived.size);
            let id = files::BYTES.keep(db, dsp, &arrived, || {
                db.add_communications_upload(dsp, by, &name, &mime, size, &arrived.sha256)
            })?;
            Ok(CommunicationsUpload {
                id,
                name,
                mime,
                size,
            })
        })
        .await
}

#[cfg(test)]
#[path = "../tests/backend/reply.rs"]
mod tests;
