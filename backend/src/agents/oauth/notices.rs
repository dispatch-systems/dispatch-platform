//! What the platform owners hear about connected apps: an email when an app connects, and
//! one when Dispatch itself disconnects an app because its code or a refresh token was
//! presented again. Each is best effort and never stops what it reports.
use super::clients;
use crate::{
    Result,
    db::{Store, now, s},
    mail::templates::{self as email, ConnectedApp},
    observability,
};
use serde_json::json;

/// What happened to a connected app.
#[derive(Clone, Copy)]
pub(super) enum Told<'a> {
    /// It connected, sending its access to this redirect.
    Connected { redirect_uri: &'a str },
    /// Dispatch ended it, for this reason.
    Disconnected { reason: &'a str },
}

impl Store {
    /// Emails every platform owner what happened to the connected app `key`.
    pub(super) fn tell_owners(&self, key: &str, told: Told) {
        if let Err(error) = self.try_tell_owners(key, told) {
            observability::event(
                "warn",
                "oauth.notice_failed",
                json!({"keyId":key,"error":error.code}),
            );
        }
    }

    fn try_tell_owners(&self, key: &str, told: Told) -> Result<()> {
        let Some(app) = self.platform.one(
            "SELECT k.name,k.client_name,k.client_verified,k.all_dsps,k.tools,k.locations,\
             u.first_name||' '||u.last_name approved_by,\
             (SELECT redirect_uri FROM oauth_codes c WHERE c.key_id=k.id) redirect_uri \
             FROM agent_keys k JOIN users u ON u.id=k.user_id WHERE k.id=? AND k.kind='app'",
            [key],
        )?
        else {
            return Ok(());
        };
        let dsps: Vec<String> = if app["all_dsps"] == 1 {
            Vec::new()
        } else {
            self.platform
                .query_as::<(String,)>(
                    "SELECT d.name FROM agent_key_dsps k JOIN dsps d ON d.id=k.dsp_id \
                     WHERE k.key_id=? ORDER BY d.name",
                    [key],
                )?
                .into_iter()
                .map(|(name,)| name)
                .collect()
        };
        let redirect_uri = match told {
            Told::Connected { redirect_uri } => Some(redirect_uri),
            Told::Disconnected { .. } => app["redirect_uri"].as_str(),
        };
        let destination = redirect_uri.map(|uri| clients::destination(uri).0);
        let at = now();
        self.notify_platform_owners(|to| {
            let described = ConnectedApp {
                origin: &self.config.origin,
                dev: self.config.env().is_preview(),
                to,
                connection: s(&app, "name"),
                app: s(&app, "client_name"),
                known: app["client_verified"] == 1,
                destination: destination.as_deref(),
                dsps: &dsps,
                essential: s(&app, "tools") == "essential",
                locations: app["locations"] == 1,
                approved_by: s(&app, "approved_by"),
                at,
            };
            match told {
                Told::Connected { .. } => email::app_connected(&described),
                Told::Disconnected { reason } => email::app_disconnected(&described, reason),
            }
        });
        Ok(())
    }
}
