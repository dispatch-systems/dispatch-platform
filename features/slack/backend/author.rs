//! Who a member of a workspace is, as what Dispatch keeps from Slack names them: who posted a
//! file, made a canvas or is mentioned in one. The DSP's member whose email address their Slack
//! profile has, or else the name Slack shows them by. A member of the workspace is read from
//! what Slack keeps of its members, or asked of Slack when they're not there yet.
use super::{
    bot::Bot,
    members,
    storage::{Member, SlackStore},
};
use dispatch_core::Result;
use dispatch_documents_contract::Author;
use serde_json::json;

/// Who the workspace's member `user` is to the DSP, and the name they're shown by: Slack itself
/// for what Slack names nobody for.
pub(crate) async fn of(bot: &Bot, user: Option<&str>) -> Result<(Author, String)> {
    let Some(user) = user else {
        return Ok((Author::Named("Slack".into()), "Slack".into()));
    };
    let member = match kept(bot, user).await? {
        Some(member) => Some(member),
        None => asked(bot, user).await?,
    };
    let Some(member) = member else {
        return Ok((Author::Named(user.to_owned()), user.to_owned()));
    };
    let shown = member.name.clone();
    let Some(email) = member.email else {
        return Ok((Author::Named(shown.clone()), shown));
    };
    let dsp = bot.dsp.clone();
    let matched = bot
        .state
        .read(move |db| {
            Ok(db
                .members(&dsp)?
                .into_iter()
                .find(|member| member.email.eq_ignore_ascii_case(&email)))
        })
        .await?;
    Ok(match matched {
        Some(member) => (Author::Member(member.user_id), member.name),
        None => (Author::Named(shown.clone()), shown),
    })
}

/// The workspace's member `user`, as Slack's members were last read.
async fn kept(bot: &Bot, user: &str) -> Result<Option<Member>> {
    let (dsp, team, user) = (bot.dsp.clone(), bot.team.clone(), user.to_owned());
    bot.state
        .read(move |db| {
            Ok(db
                .slack_members(&dsp)?
                .into_iter()
                .find(|(of, member)| *of == team && member.user == user)
                .map(|(_, member)| member))
        })
        .await
}

/// The workspace's member `user`, as Slack describes them now: none for a bot.
async fn asked(bot: &Bot, user: &str) -> Result<Option<Member>> {
    let info = bot.call("users.info", &json!({"user": user})).await?;
    Ok(members::member(&info["user"]))
}
