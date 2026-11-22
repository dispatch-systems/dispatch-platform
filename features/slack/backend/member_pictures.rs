//! Each member's profile picture, which Dispatch keeps a copy of so the person Driver Match knows
//! them as shows with it, in every feature that shows people. Slack describes it as it describes
//! the member, read whole hourly or one at a time as Slack tells of a change: the one they set,
//! at one size, downloaded only from Slack's own servers, and only again once Slack gives it at
//! another address. A member with Slack's own picture, drawn from their initials, has none.
use super::{
    bot::Bot,
    kit,
    picture::{MOST, address, kind},
    slack::text,
    storage::member_pictures::MemberPictureStore,
};
use dispatch_core::{Error, Result, db::Store, foundation::observability, manifest::people::Photo};
use serde_json::{Value, json};

/// The size kept, in pixels square: enough for a face on a sharp screen.
const SIZE: &str = "image_192";

/// The address of the picture the member set, as Slack describes the user: none for Slack's own.
pub fn source(user: &Value) -> Option<String> {
    let profile = &user["profile"];
    (profile["is_custom_image"] == true)
        .then(|| text(&profile[SIZE]))
        .flatten()
}

/// Keeps the pictures of the workspace's members, `sources` by member: each downloaded only when
/// Slack gives it at another address than the one kept came from, and forgotten once they set
/// none, each showing at once. One that fails to come is left as it was, for the next reading.
/// A picture changes nobody's person, so Driver Match isn't told.
pub async fn keep(bot: &Bot, sources: Vec<(String, Option<String>)>) -> Result<()> {
    let (dsp, team) = (bot.dsp.clone(), bot.team.clone());
    let kept = bot
        .state
        .read(move |db| db.slack_member_picture_sources(&dsp, &team))
        .await?;
    let mut changed = false;
    for (user, source) in sources {
        let (dsp, team) = (bot.dsp.clone(), bot.team.clone());
        match source {
            None if kept.contains_key(&user) => {
                bot.keep(move |db| db.drop_slack_member_picture(&dsp, &team, &user))
                    .await?;
                changed = true;
            }
            Some(source) if kept.get(&user) != Some(&source) => {
                match download(bot, &source).await {
                    Ok(picture) => {
                        bot.keep(move |db| {
                            db.keep_slack_member_picture(&dsp, &team, &user, &source, &picture)
                        })
                        .await?;
                        changed = true;
                    }
                    Err(error) => failed(&error),
                }
            }
            _ => {}
        }
    }
    // The DSP's people as features read them carry their photos' versions.
    if changed {
        bot.state.changed(Some(&bot.dsp));
    }
    Ok(())
}
/// The picture at `source`, if it's one kept: from Slack's own servers, and a PNG, JPEG or GIF
/// that isn't too large.
async fn download(bot: &Bot, source: &str) -> Result<Vec<u8>> {
    let refused = || Error::new("slack_refused", 502);
    let url = address(source).ok_or_else(refused)?;
    let picture = bot.picture(&url, MOST).await?;
    kind(&picture).ok_or_else(refused)?;
    Ok(picture)
}

/// The picture kept of the first of `ids`, Driver Match's IDs of members, that has one.
pub fn photo(db: &Store, dsp: &str, ids: &[String]) -> Result<Option<Photo>> {
    for id in ids {
        let Some((team, user)) = kit::member_of(id) else {
            continue;
        };
        if let Some(picture) = db.slack_member_picture(dsp, team, user)?
            && let Some(kind) = kind(&picture)
        {
            return Ok(Some((kind, picture)));
        }
    }
    Ok(None)
}

fn failed(error: &Error) {
    observability::event(
        "warn",
        "slack_member_picture_failed",
        json!({"error": error.code}),
    );
}
