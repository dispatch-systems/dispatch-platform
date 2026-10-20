//! What people write to the app directly, in its Messages tab in Slack. An integration that
//! hears of it (`message.im`) takes it, while it's on in the workspace and its feature is on for
//! the DSP; one that no integration took is answered that DSP Messaging is unavailable, so
//! whoever wrote it knows no one at the DSP reads it.
use super::bot::Bot;
use dispatch_core::Result;
use serde_json::{Value, json};

/// What a message no integration took is answered with.
const UNAVAILABLE: &str = "DSP Messaging is unavailable";

/// A message written to the app directly that no integration heard of: one a person wrote anew
/// is answered that DSP Messaging is unavailable. Any other, as the app's own answer, another
/// bot's post, an edit or a deletion, is left be, so an answer is never answered.
pub async fn unheard(bot: &Bot, event: &Value) -> Result<()> {
    if !written(event) {
        return Ok(());
    }
    let args = json!({"channel": event["channel"], "text": UNAVAILABLE});
    bot.call("chat.postMessage", &args).await?;
    Ok(())
}

/// Whether `event` is a message a person wrote anew to the app: by someone, from no bot, as the
/// app's own posts are, and with no subtype, which an edit, a deletion, a file shared or a
/// bot's post has.
fn written(event: &Value) -> bool {
    event["user"].as_str().is_some_and(|user| !user.is_empty())
        && event["bot_id"].is_null()
        && event["subtype"].is_null()
}

#[cfg(test)]
#[path = "../tests/backend/messages.rs"]
mod tests;
