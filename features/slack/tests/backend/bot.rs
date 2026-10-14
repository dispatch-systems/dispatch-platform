//! How the bot renews a workspace's token: Slack honours each refresh token once, so a token is
//! renewed as it nears its end and kept at once, and a token with time left, or one another call
//! has renewed since Slack refused it, is used as it is.
use super::*;
use dispatch_core::testing;

const TEAM: &str = "T0FIXTURE1";

/// A platform with one DSP, its server's state, and the bot in the DSP's workspace `TEAM`.
fn bot() -> (impl Drop, Arc<State>, Bot) {
    testing::install(&[], &[&crate::FEATURE]);
    let (root, store, dsp) = testing::bootstrapped();
    let config = store.config.clone();
    drop(store);
    let state = State::new(config).unwrap();
    let bot = Bot::new(&state, &dsp, TEAM);
    (root, state, bot)
}
/// Keeps fixture tokens for the workspace that expire `in_ms` from now.
async fn kept_until(bot: &Bot, in_ms: i64) -> Tokens {
    let tokens = Tokens {
        access: format!("fixture-access:{TEAM}:first"),
        refresh: Some(format!("fixture-refresh:{TEAM}:first")),
        expires_at: Some(now() + in_ms),
    };
    let (dsp, kept) = (bot.dsp.clone(), tokens.clone());
    bot.state
        .run(move |db| db.save_slack_tokens(&dsp, TEAM, &kept))
        .await
        .unwrap();
    tokens
}
async fn stored(bot: &Bot) -> Tokens {
    let dsp = bot.dsp.clone();
    bot.state
        .read(move |db| db.slack_tokens(&dsp, TEAM))
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn a_token_is_renewed_as_it_nears_its_end_and_kept_and_never_twice() {
    let (_root, _state, bot) = bot();
    // Two hours left: used as it is.
    let lasting = kept_until(&bot, 2 * 60 * 60 * 1000).await;
    assert_eq!(bot.token().await.unwrap(), lasting.access);

    // Within the hour before the next check: renewed, and the new pair kept.
    let first = kept_until(&bot, 30 * 60 * 1000).await;
    let renewed = bot.token().await.unwrap();
    assert_ne!(renewed, first.access);
    let kept = stored(&bot).await;
    assert_eq!(kept.access, renewed);
    assert_ne!(kept.refresh, first.refresh);
    // Renewed, it lasts: the next call uses it as it is.
    assert_eq!(bot.token().await.unwrap(), renewed);
    // A call Slack refused with the token from before is given the renewed one.
    assert_eq!(bot.instead_of(&first.access).await.unwrap(), renewed);
    assert_eq!(stored(&bot).await, kept);
    // One Slack refused with the token kept now is renewed, though it has time left.
    assert_ne!(bot.instead_of(&renewed).await.unwrap(), renewed);
}
