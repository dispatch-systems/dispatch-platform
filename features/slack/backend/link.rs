//! The DSP's install link: for whoever runs its Slack, who needn't be in Dispatch, to add the
//! app to their workspace for the DSP. It works once, for seven days, until it's cancelled,
//! and only while the DSP is active with Slack on. Dispatch keeps only its hash.
use super::{
    app::INSTALL_PATH,
    connection::{self, Started},
    slack::Slack,
    storage::{SlackStore, Starter},
};
use crate::api::types::{SlackAccount, SlackNewLink};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::{Store, identifier},
    ensure,
    foundation::crypto,
};
use std::sync::Arc;

/// Makes the DSP's install link, in place of any it had: shown this once.
pub fn create(db: &Store, c: &Context) -> Result<SlackNewLink> {
    Slack::of(&db.config)?;
    let link = format!("{}.{}", c.dsp.id, crypto::token()?);
    let kept = db.save_slack_link(&c.dsp.id, &crypto::sha(&link), c.actor())?;
    c.audit(db, "slack.link_created", "")?;
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("link", &link)
        .finish();
    Ok(SlackNewLink {
        url: format!("{}{INSTALL_PATH}?{query}", db.config.origin),
        expires_at: kept.expires_at,
    })
}
/// Ends the DSP's install link, so it no longer works.
pub fn cancel(db: &Store, c: &Context) -> Result<SlackAccount> {
    if db.end_slack_link(&c.dsp.id, None)? {
        c.audit(db, "slack.link_cancelled", "")?;
    }
    connection::account(db, &c.dsp.id)
}

/// Whether `link`, as someone opened it, is a DSP's install link that still works: the DSP it
/// is for and its hash. A read, so a link that doesn't work costs no write.
pub fn opened(db: &Store, link: &str) -> Result<(String, String)> {
    let (dsp, _) = link
        .split_once('.')
        .filter(|(dsp, _)| identifier(dsp, "dsp_"))
        .ok_or_else(|| Error::new("slack_link_expired", 404))?;
    let hash = crypto::sha(link);
    let works = db.slack_link_works(dsp, &hash).unwrap_or(false) && connection::open(db, dsp)?;
    ensure(works, "slack_link_expired", 404)?;
    Ok((dsp.to_owned(), hash))
}
/// Starts installing the app by the link `link`, for whoever opened it: where their browser
/// goes next.
pub fn start(db: &Store, link: &str) -> Result<String> {
    let (dsp, hash) = opened(db, link)?;
    let slack = Slack::of(&db.config)?;
    let state = connection::new_state(&dsp, Started::Link)?;
    db.start_slack_install(&dsp, &state, &Starter::Link(hash))?;
    connection::install_url(db, &slack, &state, None)
}

/// Finishes an install the link started, now that Slack sent the browser back with `code`:
/// keeps the workspace and uses the link up. The workspace's ID and the app's, for Slack to
/// open the app there.
pub async fn finish(state: &Arc<State>, sign_in: String, code: String) -> Result<(String, String)> {
    let dsp = match connection::started(&sign_in) {
        Some((dsp, Started::Link)) => dsp.to_owned(),
        _ => return Err(Error::new("slack_connect_expired", 409)),
    };
    let at = dsp.clone();
    let starter = state
        .run(move |db| db.finish_slack_install(&at, &sign_in))
        .await?;
    let Starter::Link(hash) = starter else {
        return Err(Error::new("slack_connect_expired", 409));
    };
    let installed = Slack::of(&state.config)?
        .exchange(&state.config.origin, &code)
        .await?;
    let opened = (installed.team.clone(), installed.app.clone());
    let made = (installed.team.clone(), installed.tokens.access.clone());
    let kept = state
        .run(move |db| {
            // The link may have been cancelled, or used, while Slack asked.
            let works = db.slack_link_works(&dsp, &hash)? && connection::open(db, &dsp)?;
            ensure(works, "slack_link_expired", 404)?;
            let (action, name) = connection::keep(db, &dsp, installed, None)?;
            db.end_slack_link(&dsp, Some(&hash))?;
            db.audit(None, Some(&dsp), action, &name)
        })
        .await;
    if kept.is_err() {
        connection::undo(state, made).await;
    }
    kept.map(|()| opened)
}
