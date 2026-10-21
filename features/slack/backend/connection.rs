//! Adding Dispatch to a DSP's Slack workspaces, on Settings' DSP Connections: starting an
//! install, finishing it once Slack sends the browser back, and taking Dispatch out of a
//! workspace; and what Slack says of a workspace it stopped honouring or removed the app from.
//! Slack is called outside the database, and the member's permission is checked again after
//! every wait, before anything is written. The install link, the other way in, is `link`'s.
use super::{
    app::scopes,
    bot::Bot,
    fixture, logo, members,
    slack::{Installed, Slack},
    storage::{Install, SlackStore, Starter, Workspace},
};
use crate::api::types::{SlackAccount, SlackInstall, SlackLink, SlackWorkspace, WorkspaceStatus};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::{Store, identifier, iso},
    ensure,
    foundation::{config::Site, crypto},
    server::http::route::Dsp,
    tenancy::api::types::DspStatus,
};
use std::sync::Arc;

/// Where an install started, as the state Slack carries back says: on the admin's address, on
/// the DSP's own, or by the DSP's install link. The browser comes back to where it started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Started {
    Admin,
    Own,
    Link,
}
/// The DSP an install's state names, and where it started: `<dsp>.<where>.<secret>`.
pub fn started(state: &str) -> Option<(&str, Started)> {
    let mut parts = state.splitn(3, '.');
    let (dsp, at, _) = (parts.next()?, parts.next()?, parts.next()?);
    let at = match at {
        "admin" => Started::Admin,
        "own" => Started::Own,
        "link" => Started::Link,
        _ => return None,
    };
    identifier(dsp, "dsp_").then_some((dsp, at))
}
/// A new install's state, for the DSP and where it started.
pub(super) fn new_state(dsp: &str, at: Started) -> Result<String> {
    let at = match at {
        Started::Admin => "admin",
        Started::Own => "own",
        Started::Link => "link",
    };
    Ok(format!("{dsp}.{at}.{}", crypto::token()?))
}
/// Where an install goes: the workspace named, or Slack's choice. Fixture mode has no choice
/// to offer, so it takes its next workspace free.
pub(super) fn install_url(
    db: &Store,
    slack: &Slack,
    state: &str,
    team: Option<String>,
) -> Result<String> {
    let team = match (slack, team) {
        (Slack::Fixture, None) => Some(fixture::free_team(db)?),
        (_, team) => team,
    };
    Ok(slack.install_url(&db.config.origin, state, team.as_deref()))
}

/// The DSP's workspaces, each with its logo's version, and its install link for adding one, as
/// those who manage its connections see them.
pub fn account(db: &Store, dsp: &str) -> Result<SlackAccount> {
    let mut logos = db.slack_logos(dsp)?;
    let workspaces = db
        .slack_workspaces(dsp)?
        .into_iter()
        .map(|workspace| {
            let logo = logos.remove(&workspace.team);
            described(db, dsp, workspace, logo)
        })
        .collect::<Result<_>>()?;
    let link = match db.slack_link(dsp, None)? {
        Some(link) => Some(SlackLink {
            created_by: db.actor_name(dsp, &link.created_by)?,
            expires_at: link.expires_at,
            label: link.label,
        }),
        None => None,
    };
    Ok(SlackAccount {
        available: Slack::available(&db.config),
        workspaces,
        link,
    })
}
fn described(
    db: &Store,
    dsp: &str,
    workspace: Workspace,
    logo: Option<String>,
) -> Result<SlackWorkspace> {
    Ok(SlackWorkspace {
        logo,
        status: if workspace.broken {
            WorkspaceStatus::Broken
        } else {
            WorkspaceStatus::Connected
        },
        needs_update: !scopes()
            .iter()
            .all(|scope| workspace.scopes.iter().any(|had| had == scope)),
        integrations: db.slack_integrations(dsp, &workspace.team)?,
        connected_by: match &workspace.connected_by {
            Some(user) => db.actor_name(dsp, user)?,
            None => None,
        },
        by_link: workspace.connected_by.is_none(),
        connected_at: workspace.connected_at,
        team_id: workspace.team,
        name: workspace.name,
        label: workspace.label,
    })
}

/// Starts installing Dispatch in a workspace for the member, from the address `site` they're
/// at: a new one, to have the name `label` in Dispatch, or one of the DSP's, `team`, to
/// reconnect or update, which Slack then doesn't ask them to choose. Slack sends them back to
/// the DSP's page `page`, where they started, or else to Settings' Connections.
pub fn start(
    db: &Store,
    c: &Context,
    site: &Site,
    team: Option<String>,
    label: Option<String>,
    page: Option<String>,
) -> Result<SlackInstall> {
    let slack = Slack::of(&db.config)?;
    if let Some(team) = &team {
        ensure(
            db.slack_workspace(&c.dsp.id, team)?.is_some(),
            "slack_workspace_not_found",
            404,
        )?;
    }
    let at = match site {
        Site::Dsp(_) => Started::Own,
        _ => Started::Admin,
    };
    let state = new_state(&c.dsp.id, at)?;
    let install = Install {
        starter: Starter::Member(c.actor().to_owned()),
        team: team.clone(),
        label,
        page,
    };
    db.start_slack_install(&c.dsp.id, &state, &install)?;
    Ok(SlackInstall {
        url: install_url(db, &slack, &state, team)?,
    })
}

/// Finishes the install the member started with `sign_in`, now that Slack sent them back with
/// `code`: keeps the workspace, which becomes the DSP's. One started to add a workspace that
/// comes back with one the DSP has already is refused (`slack_workspace_connected`), after
/// keeping its new tokens: Slack installed the app there again, and may stop honouring the old.
pub async fn finish(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    sign_in: String,
    code: String,
) -> Result<SlackAccount> {
    let ours = started(&sign_in).is_some_and(|(dsp, at)| dsp == c.dsp.id && at != Started::Link);
    ensure(ours, "slack_connect_expired", 409)?;
    let (dsp, actor) = (c.dsp.id.clone(), c.actor().to_owned());
    let install = state
        .run(move |db| db.finish_slack_install(&dsp, &sign_in))
        .await?;
    ensure(
        install.starter == Starter::Member(actor.clone()),
        "slack_connect_expired",
        409,
    )?;
    let installed = Slack::of(&state.config)?
        .exchange(&state.config.origin, &code)
        .await?;
    let made = (installed.team.clone(), installed.tokens.access.clone());
    let team = installed.team.clone();
    let id = c.dsp.id.clone();
    let (named, label) = (install.team, install.label);
    let kept = state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            let (action, name) = keep(db, &c.dsp.id, installed, Some(&actor), label)?;
            c.audit(db, action, &name)?;
            Ok(action)
        })
        .await;
    if kept.is_err() {
        undo(state, made).await;
    }
    let action = kept?;
    // The account as the install left it, before Slack is asked anything more there.
    let at = id.clone();
    let answer = state.read(move |db| account(db, &at)).await;
    // Whatever its admin approved now, its members are read as it allows, and its logo.
    members::read_soon(state, &id, &team);
    logo::read_soon(state, &id, &team);
    let again = action == RECONNECTED && named.is_none();
    ensure(!again, "slack_workspace_connected", 409)?;
    answer
}

/// Takes back an install Slack made and Dispatch then refused, `(team, token)`, so the app
/// isn't left in a workspace with tokens nobody holds. A workspace a DSP holds keeps its
/// installation, whichever DSP's it is: Slack removing the app removes it for every token.
pub(super) async fn undo(state: &Arc<State>, (team, token): (String, String)) {
    let held = state.read(move |db| db.slack_team_dsp(&team)).await;
    if let (Ok(None), Ok(slack)) = (held, Slack::of(&state.config)) {
        slack.uninstall(&token).await;
    }
}

/// What the activity log records for an install that gave the DSP a workspace it had already.
const RECONNECTED: &str = "slack.reconnected";

/// Keeps a workspace an install gave the DSP, connected by `by`, or by the install link, who
/// then shows as having connected it, reconnected or not: the action the activity log records,
/// and the workspace's name. A workspace new to the DSP takes the name `label` in Dispatch; one
/// it had keeps its own. A workspace another DSP has is refused, and stays that DSP's.
pub(super) fn keep(
    db: &Store,
    dsp: &str,
    installed: Installed,
    by: Option<&str>,
    label: Option<String>,
) -> Result<(&'static str, String)> {
    let existing = db.slack_workspace(dsp, &installed.team)?;
    let workspace = Workspace {
        label: match &existing {
            Some(had) => had.label.clone(),
            None => label,
        },
        team: installed.team,
        name: installed.name,
        broken: false,
        scopes: installed.scopes,
        bot_user: installed.bot_user,
        app: installed.app,
        connected_by: by.map(str::to_owned),
        connected_at: iso(),
    };
    db.save_slack_workspace(dsp, &workspace, &installed.tokens)?;
    let action = match (existing, by) {
        (Some(_), _) => RECONNECTED,
        (None, Some(_)) => "slack.connected",
        (None, None) => "slack.connected_by_link",
    };
    Ok((action, workspace.name))
}

/// Names the DSP's workspace `team` in Dispatch, `label`, or with none, has it known by its
/// name in Slack.
pub fn rename(db: &Store, c: &Context, team: &str, label: Option<String>) -> Result<SlackAccount> {
    let workspace = db.label_slack_workspace(&c.dsp.id, team, label.as_deref())?;
    let workspace = workspace.ok_or_else(|| Error::new("slack_workspace_not_found", 404))?;
    c.audit(
        db,
        "slack.renamed",
        label.as_deref().unwrap_or(&workspace.name),
    )?;
    account(db, &c.dsp.id)
}

/// Takes Dispatch out of the DSP's workspace `team`: Dispatch forgets it, then Slack removes
/// the app from it.
pub async fn disconnect(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    team: String,
) -> Result<SlackAccount> {
    let (dsp, of) = (c.dsp.id.clone(), team.clone());
    let found = state.read(move |db| db.slack_workspace(&dsp, &of)).await?;
    ensure(found.is_some(), "slack_workspace_not_found", 404)?;
    let token = Bot::new(state, &c.dsp.id, &team).token().await.ok();
    let id = c.dsp.id.clone();
    state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            if let Some(found) = db.remove_slack_workspace(&c.dsp.id, &team)? {
                c.audit(db, "slack.disconnected", &found.name)?;
            }
            Ok(())
        })
        .await?;
    if let (Some(token), Ok(slack)) = (token, Slack::of(&state.config)) {
        slack.uninstall(&token).await;
    }
    state.read(move |db| account(db, &id)).await
}

/// Marks the workspace broken, as Slack stopped honouring its tokens: once, in the activity
/// log too, whoever noticed.
pub(super) fn broken(db: &Store, dsp: &str, team: &str) -> Result<()> {
    if db.break_slack_workspace(dsp, team)?
        && let Some(found) = db.slack_workspace(dsp, team)?
    {
        db.audit(None, Some(dsp), "slack.connection_broken", &found.name)?;
    }
    Ok(())
}
/// Forgets the workspace, as Slack says the app was removed from it there.
pub(super) fn removed(db: &Store, dsp: &str, team: &str) -> Result<()> {
    if let Some(found) = db.remove_slack_workspace(dsp, team)? {
        db.audit(None, Some(dsp), "slack.removed_in_slack", &found.name)?;
    }
    Ok(())
}

/// Whether the DSP is active, with Slack switched on for it: else the app stays in its
/// workspaces and does nothing.
pub(super) fn open(db: &Store, dsp: &str) -> Result<bool> {
    Ok(db.find_dsp(dsp)?.status == DspStatus::Active && db.feature_enabled(dsp, "slack")?)
}
