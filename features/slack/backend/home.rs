//! The app's Home tab in Slack: the DSP the workspace belongs to, and the way into its
//! Dispatch. It's drawn each time someone opens the tab, so it shows the DSP as it is now.
use super::bot::Bot;
use dispatch_core::{Result, ensure};
use serde_json::{Value, json};

/// The tab's button, which opens the DSP's Dispatch in the browser.
pub const OPEN_DISPATCH: &str = "open_dispatch";

/// Someone opened the app in Slack: its Home tab, to them, or its messages, which it has none
/// of yet.
pub async fn opened(bot: &Bot, event: &Value) -> Result<()> {
    if event["tab"] != "home" {
        return Ok(());
    }
    let user = event["user"].as_str().unwrap_or("");
    ensure(slack_user(user), "invalid_input", 400)?;
    let dsp = bot.dsp.clone();
    let (name, address) = bot
        .state
        .read(move |db| {
            let found = db.find_dsp(&dsp)?;
            Ok((found.name, found.code.map(|code| db.config.dsp_url(&code))))
        })
        .await?;
    let args = json!({"user_id": user, "view": view(&name, address.as_deref())});
    bot.call("views.publish", &args).await?;
    Ok(())
}

/// The tab: the DSP's name, and a button to its address once it has one.
fn view(name: &str, address: Option<&str>) -> Value {
    // Slack takes a header of up to 150 characters.
    let name: String = name.chars().take(150).collect();
    let mut blocks = vec![json!({
        "type": "header",
        "text": {"type": "plain_text", "text": name},
    })];
    if let Some(address) = address {
        blocks.push(json!({
            "type": "actions",
            "elements": [{
                "type": "button",
                "action_id": OPEN_DISPATCH,
                "style": "primary",
                "text": {"type": "plain_text", "text": "Open Dispatch"},
                "url": address,
            }],
        }));
    }
    json!({"type": "home", "blocks": blocks})
}

/// Whether `user` is a Slack user ID as Slack writes them.
fn slack_user(user: &str) -> bool {
    (2..=32).contains(&user.len())
        && user.starts_with(['U', 'W'])
        && user
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
