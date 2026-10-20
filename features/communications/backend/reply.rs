//! Answering from Dispatch. A message someone at the DSP writes, with any photos they add, goes to
//! the member's direct message with the app in Slack (`chat:write`), under their own name and
//! photo (`chat:write.customize`, as `sender` says), its photos uploaded first, shared in no
//! conversation but shown in the message (`files:write`). Someone at the DSP may open a direct
//! message with any member of a workspace where Communications works, to message them first
//! (`im:write`). Slack is called outside the database, and the member's access checked again
//! before anything is kept.
use super::{
    conversations, files,
    files::FileStore,
    live, sender,
    storage::{CommunicationsStore, DOMAIN, Sent},
    text,
};
use crate::{
    api::types::{CommunicationsUpload, Conversation, Message},
    integrations::messages::{ID, incoming},
};
use dispatch_core::{
    Error, Result, State, accounts::Context, ensure, server::http::route::Dsp,
    server::http::upload::Upload,
};
use dispatch_slack::kit::{self, Bot};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

/// The most a message's text may be, in characters, as Slack shows a section of a message.
pub const TEXT_MOST: usize = 3000;
/// The most photos a message may carry, and the largest each may be, in bytes.
pub const PHOTOS_MOST: usize = 10;
pub const PHOTO_MOST: u64 = 20 * 1024 * 1024;
/// The kinds of photo Slack shows in a message, each with how its bytes begin.
const PHOTO_KINDS: &[(&str, &[u8])] = &[
    ("image/png", b"\x89PNG"),
    ("image/jpeg", b"\xff\xd8\xff"),
    ("image/gif", b"GIF8"),
];

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
    let channel = match kept.and_then(|kept| kept.channel) {
        Some(channel) => channel,
        None => opened(state, &c.dsp.id, team, user).await?,
    };
    let (shared, c, team, user) = (state.clone(), c.clone(), team.to_owned(), user.to_owned());
    state
        .run_scoped(c.dsp.id.clone(), DOMAIN, move |db| {
            let c = access.revalidate(db, &c)?;
            let kept =
                db.open_communications_conversation(&c.dsp.id, &team, &user, &channel, &member.name)?;
            conversations::one(db, &shared, &c.dsp.id, &kept.id)
        })
        .await
}
/// The direct message the app has with the workspace `team`'s member `user`, as Slack opens it.
async fn opened(state: &Arc<State>, dsp: &str, team: &str, user: &str) -> Result<String> {
    let answer = Bot::new(state, dsp, team)
        .call("conversations.open", &json!({"users": user}))
        .await?;
    answer["channel"]["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new("slack_refused", 502))
}

/// Sends what the member `c` wrote in the conversation `id`, with the photos `photos` they
/// added: the message, as the page reads it.
pub async fn send(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    id: &str,
    written: &str,
    photos: &[String],
) -> Result<Message> {
    let written = written.trim();
    ensure(
        (!written.is_empty() || !photos.is_empty())
            && written.chars().count() <= TEXT_MOST
            && photos.len() <= PHOTOS_MOST,
        "invalid_input",
        400,
    )?;
    let (dsp, at, by, chosen) = (
        c.dsp.id.clone(),
        id.to_owned(),
        c.auth.user.id.clone(),
        photos.to_vec(),
    );
    let (conversation, uploads) = state
        .read(move |db| {
            let conversation = db
                .communications_conversation(&dsp, &at)?
                .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
            ensure(
                kit::workspaces_on(db, &dsp, ID)?.contains(&conversation.team),
                "communications_off",
                409,
            )?;
            Ok((conversation, db.communications_uploads(&dsp, &by, &chosen)?))
        })
        .await?;
    let dsp = c.dsp.id.as_str();
    let channel = match conversation.channel {
        Some(channel) => channel,
        None => opened(state, dsp, &conversation.team, &conversation.user).await?,
    };
    let bot = Bot::new(state, dsp, &conversation.team);
    let mut shown = Vec::new();
    for upload in &uploads {
        let sha256 = upload.sha256.as_deref().unwrap_or_default();
        let bytes = files::read(state, dsp, sha256).await?;
        let slack_id = bot.upload(&upload.name, upload.size as u64, bytes).await?;
        shown.push((upload.id.clone(), slack_id, upload.name.clone()));
    }
    let identity = sender::identity(state, dsp, &conversation.team, &c.auth.user).await;
    let mut args = json!({
        "channel": channel,
        "text": if written.is_empty() {
            shown.iter().map(|(_, _, name)| text::escaped(name)).collect::<Vec<_>>().join(", ")
        } else {
            text::escaped(written)
        },
        "username": identity.name,
        "icon_url": identity.icon,
        "unfurl_links": false,
    });
    if !shown.is_empty() {
        args["blocks"] = blocks(written, &shown);
    }
    let posted = post(&bot, &args, !shown.is_empty()).await?;
    let ts = posted["ts"].as_str().unwrap_or_default().to_owned();
    let at = incoming::millis(&ts).ok_or_else(|| Error::new("slack_refused", 502))?;
    let photos: Vec<(String, String)> = shown
        .into_iter()
        .map(|(photo, slack_id, _)| (photo, slack_id))
        .collect();
    let (shared, by, conversation, name) = (state.clone(), c.clone(), id.to_owned(), identity.name);
    let written = written.to_owned();
    let message = state
        .run_scoped(c.dsp.id.clone(), DOMAIN, move |db| {
            let c = access.revalidate(db, &by)?;
            let sent = Sent {
                ts: &ts,
                text: &written,
                at,
                by: &c.auth.user.id,
                name: &name,
                photos: &photos,
            };
            let kept = db.keep_communications_reply(&c.dsp.id, &conversation, &sent)?;
            let people = conversations::People::read(db, &shared, &c.dsp.id)?;
            Ok(conversations::messages(db, &people, &c.dsp.id, vec![kept])?.remove(0))
        })
        .await?;
    live::stopped(state, &c.dsp.id, id, &c.auth.user.id);
    live::changed(state, &c.dsp.id);
    Ok(message)
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
async fn post(bot: &Bot, args: &Value, photos: bool) -> Result<Value> {
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
/// `mime`: as the page reads it. One that isn't a photo Slack shows is refused.
pub async fn upload(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    name: &str,
    mime: &str,
    upload: Upload,
) -> Result<CommunicationsUpload> {
    let begins = PHOTO_KINDS
        .iter()
        .find(|(kind, _)| *kind == mime)
        .map(|(_, begins)| *begins)
        .ok_or_else(|| Error::new("communications_photo_kind", 415))?;
    let size = upload.length as i64;
    let sha256 = files::keep(state, &c.dsp.id, upload).await?;
    let head = files::head(state, &c.dsp.id, &sha256, begins.len()).await?;
    if head != begins {
        files::discard(state, &c.dsp.id, &sha256).await?;
        return Err(Error::new("communications_photo_kind", 415));
    }
    let (c, name, mime) = (c.clone(), name.to_owned(), mime.to_owned());
    state
        .run_scoped(c.dsp.id.clone(), DOMAIN, move |db| {
            let c = access.revalidate(db, &c)?;
            let id =
                db.add_communications_upload(&c.dsp.id, &c.auth.user.id, &name, &mime, size, &sha256)?;
            Ok(CommunicationsUpload {
                id,
                name,
                mime,
                size,
            })
        })
        .await
}
