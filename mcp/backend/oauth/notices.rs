//! What the platform owners hear about connected apps: an email when an app connects, and
//! one when Dispatch itself disconnects an app because its code or a refresh token was
//! presented again. Each is best effort and never stops what it reports.
use super::{
    super::{keys, tools::Toolbox},
    clients,
    mail::{self as email, ConnectedApp},
};
use dispatch_core::{
    Result,
    db::{Store, now, s},
    foundation::observability,
};
use serde_json::json;

/// The kind of email the notices are, as the mail log names it.
pub const KIND: &str = "connected_app";

/// What happened to a connected app.
#[derive(Clone, Copy)]
pub(super) enum Told<'a> {
    /// It connected, sending its access to this redirect.
    Connected { redirect_uri: &'a str },
    /// Dispatch ended it, for this reason.
    Disconnected { reason: &'a str },
}

/// Emails every platform owner what happened to the connected app `key`.
pub(super) fn tell_owners(db: &Store, key: &str, told: Told) {
    if let Err(error) = try_tell_owners(db, key, told) {
        observability::event(
            "warn",
            "oauth.notice_failed",
            json!({"keyId":key,"error":error.code}),
        );
    }
}

fn try_tell_owners(db: &Store, key: &str, told: Told) -> Result<()> {
    let Some(app) = db.platform.one(
        "SELECT k.name,k.client_name,k.client_verified,k.all_dsps,k.all_tools,k.user_id,\
         (SELECT redirect_uri FROM oauth_codes c WHERE c.key_id=k.id) redirect_uri \
         FROM agent_keys k WHERE k.id=? AND k.kind='app'",
        [key],
    )?
    else {
        return Ok(());
    };
    let Some(approved_by) = db.platform_user_name(s(&app, "user_id"))? else {
        return Ok(());
    };
    let dsps: Vec<String> = if app["all_dsps"] == 1 {
        Vec::new()
    } else {
        let given = keys::agent_key_dsps(db, key)?;
        let given: Vec<&str> = given.iter().map(String::as_str).collect();
        let mut names: Vec<String> = db.dsp_names(&given)?.into_values().collect();
        names.sort();
        names
    };
    let redirect_uri = match told {
        Told::Connected { redirect_uri } => Some(redirect_uri),
        Told::Disconnected { .. } => app["redirect_uri"].as_str(),
    };
    let destination = redirect_uri.map(|uri| clients::destination(uri).0);
    let tools =
        Toolbox::installed().summary(&keys::agent_key_grants(db, key, app["all_tools"] == 1)?);
    let at = now();
    db.notify_platform_owners(KIND, |to| {
        let described = ConnectedApp {
            origin: &db.config.origin,
            dev: db.config.env().is_preview(),
            to,
            connection: s(&app, "name"),
            app: s(&app, "client_name"),
            known: app["client_verified"] == 1,
            destination: destination.as_deref(),
            dsps: &dsps,
            tools: &tools,
            approved_by: &approved_by,
            at,
        };
        match told {
            Told::Connected { .. } => email::app_connected(&described),
            Told::Disconnected { reason } => email::app_disconnected(&described, reason),
        }
    });
    Ok(())
}
