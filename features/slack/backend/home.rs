//! The app's Home tab in Slack: the DSP the workspace belongs to. It's drawn each time someone
//! opens the tab, so it shows the DSP as it is now.
use super::bot::Bot;
use dispatch_core::{Result, ensure};
use serde_json::{Value, json};

/// Someone opened the app in Slack: its Home tab, to them, or its messages, which it has none
/// of yet.
pub async fn opened(bot: &Bot, event: &Value) -> Result<()> {
    if event["tab"] != "home" {
        return Ok(());
    }
    let user = event["user"].as_str().unwrap_or("");
    ensure(slack_user(user), "invalid_input", 400)?;
    let dsp = bot.dsp.clone();
    let name = bot
        .state
        .read(move |db| Ok(db.find_dsp(&dsp)?.name))
        .await?;
    let args = json!({"user_id": user, "view": view(&name)});
    bot.call("views.publish", &args).await?;
    Ok(())
}

/// The tab: the DSP's name.
fn view(name: &str) -> Value {
    // Slack takes a header of up to 150 characters.
    let name: String = name.chars().take(150).collect();
    json!({
        "type": "home",
        "blocks": [{
            "type": "header",
            "text": {"type": "plain_text", "text": name},
        }],
    })
}

/// Whether `user` is a Slack user ID as Slack writes them.
fn slack_user(user: &str) -> bool {
    (2..=32).contains(&user.len())
        && user.starts_with(['U', 'W'])
        && user
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
