//! The DSP's install links: for whoever runs its Slack, who needn't be in Dispatch, to add the
//! app to a workspace for the DSP, or to update or reconnect one of its workspaces there. Each
//! works once, for seven days, until it's cancelled, and only while the DSP is active with Slack
//! on. The DSP has one for adding a workspace and one for each of its workspaces: a new one takes
//! the place only of the one for the same, and a workspace's goes with it. Dispatch keeps only
//! their hashes.
use super::{
    app::INSTALL_PATH,
    connection::{self, Started},
    logo, members,
    slack::Slack,
    storage::{Install, Link, SlackStore, Starter},
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

/// Makes the DSP's install link, in place of the one before it for the same: for adding a
/// workspace, to have the name `label` in Dispatch, or for updating or reconnecting the
/// workspace `team`, which keeps its own and is given none. Shown this once.
pub fn create(
    db: &Store,
    c: &Context,
    team: Option<&str>,
    label: Option<String>,
) -> Result<SlackNewLink> {
    Slack::of(&db.config)?;
    let named = match team {
        Some(team) => {
            let workspace = db
                .slack_workspace(&c.dsp.id, team)?
                .ok_or_else(|| Error::new("slack_workspace_not_found", 404))?;
            workspace.label.unwrap_or(workspace.name)
        }
        None => String::new(),
    };
    let link = format!("{}.{}", c.dsp.id, crypto::token()?);
    let hash = crypto::sha(&link);
    let kept = db.save_slack_link(&c.dsp.id, team, &hash, c.actor(), label.as_deref())?;
    c.audit(db, "slack.link_created", &named)?;
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("link", &link)
        .finish();
    Ok(SlackNewLink {
        url: format!("{}{INSTALL_PATH}?{query}", db.config.origin),
        expires_at: kept.expires_at,
    })
}
/// Ends the DSP's install link for adding a workspace, so it no longer works.
pub fn cancel(db: &Store, c: &Context) -> Result<SlackAccount> {
    if db.end_slack_link(&c.dsp.id, None)? {
        c.audit(db, "slack.link_cancelled", "")?;
    }
    connection::account(db, &c.dsp.id)
}

/// Whether `link`, as someone opened it, is a DSP's install link that still works: the DSP it
/// is for and its hash. A read, so a link that doesn't work costs no write.
pub fn opened(db: &Store, link: &str) -> Result<(String, String)> {
    working(db, link).map(|(dsp, hash, _)| (dsp, hash))
}
/// The DSP `link` is for, its hash and what's kept of it, while it works.
fn working(db: &Store, link: &str) -> Result<(String, String, Link)> {
    let (dsp, _) = link
        .split_once('.')
        .filter(|(dsp, _)| identifier(dsp, "dsp_"))
        .ok_or_else(|| Error::new("slack_link_expired", 404))?;
    let hash = crypto::sha(link);
    let kept = db.working_slack_link(dsp, &hash).ok().flatten();
    match kept {
        Some(kept) if connection::open(db, dsp)? => Ok((dsp.to_owned(), hash, kept)),
        _ => Err(Error::new("slack_link_expired", 404)),
    }
}
/// Starts installing the app by the link `link`, for whoever opened it, in the workspace it's
/// for, if it's for one: where their browser goes next.
pub fn start(db: &Store, link: &str) -> Result<String> {
    let (dsp, hash, link) = working(db, link)?;
    let slack = Slack::of(&db.config)?;
    let state = connection::new_state(&dsp, Started::Link)?;
    let install = Install {
        starter: Starter::Link(hash),
        team: link.team.clone(),
        label: link.label,
        page: None,
    };
    db.start_slack_install(&dsp, &state, &install)?;
    connection::install_url(db, &slack, &state, link.team)
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
    let Install { starter, label, .. } = state
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
    let at = dsp.clone();
    let kept = state
        .run(move |db| {
            // The link may have been cancelled, or used, while Slack asked.
            let works =
                db.working_slack_link(&dsp, &hash)?.is_some() && connection::open(db, &dsp)?;
            ensure(works, "slack_link_expired", 404)?;
            let (action, name) = connection::keep(db, &dsp, installed, None, label)?;
            db.use_slack_link(&dsp, &hash)?;
            db.audit(None, Some(&dsp), action, &name)
        })
        .await;
    if kept.is_err() {
        connection::undo(state, made).await;
        return kept.map(|()| opened);
    }
    members::read_soon(state, &at, &opened.0);
    logo::read_soon(state, &at, &opened.0);
    kept.map(|()| opened)
}
