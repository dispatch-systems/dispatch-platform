//! Who may ask to connect, and when. An app asks only while the platform owner has the
//! pairing window open, for ten minutes from when they opened it on the Connect tab, so a
//! sign-in link someone else sends them finds connecting closed. And only a kind of app the
//! owner lets connect: the four known apps and apps on the owner's own computer unless they
//! turn them off, websites and other apps only once they turn them on.
use super::{clients, issuer, network};
use crate::{
    Result,
    db::{Store, at, iso, now},
    mcp::api::types::{
        OAuthAllowedApp, OAuthAllowedApps, OAuthAppChoice, OAuthAppId, OAuthPairing,
        OAuthPairingOpened,
    },
};
use std::collections::HashMap;

/// How long the window stays open from when it was last opened.
const PAIRING: i64 = 10 * 60 * 1000;

/// Every kind of app, as the Connect tab lists them: its name, and whether it may connect
/// until the owner says otherwise.
const APPS: &[(OAuthAppId, &str, bool)] = &[
    (OAuthAppId::Chatgpt, "ChatGPT", true),
    (OAuthAppId::Codex, "Codex", true),
    (OAuthAppId::ClaudeCode, "Claude Code", true),
    (OAuthAppId::Hermes, "Hermes Agent", true),
    (OAuthAppId::Local, "Apps on this computer", true),
    (OAuthAppId::Web, "Websites and other apps", false),
];
fn named(app: OAuthAppId) -> (&'static str, bool) {
    APPS.iter()
        .find(|(id, ..)| *id == app)
        .map_or(("", false), |(_, name, default)| (name, *default))
}

/// The kind of app an authorization request comes from, by its client id: a known app's, a
/// website's, or one that registered itself, which is a website's when the redirect it asks
/// for is `https` and otherwise an app on the owner's own computer. None when the client id
/// is no app's at all.
pub fn app_of(client_id: &str, redirect_uri: Option<&str>) -> Option<OAuthAppId> {
    if let Some(app) = clients::known_app(client_id) {
        return Some(app);
    }
    if client_id.starts_with("dcr_") {
        let web = redirect_uri.is_some_and(|uri| uri.starts_with("https://"));
        return Some(if web {
            OAuthAppId::Web
        } else {
            OAuthAppId::Local
        });
    }
    network::web_client_id(client_id).map(|_| OAuthAppId::Web)
}

impl Store {
    /// When the window closes, while it is open.
    pub fn oauth_pairing(&self) -> Result<OAuthPairing> {
        let open_until = self
            .platform
            .one_as::<(String,)>(
                "SELECT open_until FROM oauth_pairing WHERE id=1 AND open_until>?",
                [iso()],
            )?
            .map(|(until,)| until);
        Ok(OAuthPairing { open_until })
    }

    /// Opens the window for ten minutes from now, or keeps it open until then. The log
    /// records it opening; keeping an open window open is the same opening.
    pub fn open_oauth_pairing(&self, owner: &str) -> Result<OAuthPairingOpened> {
        let open_until = at(now() + PAIRING);
        self.platform.transaction(|| {
            let closed = self.oauth_pairing()?.open_until.is_none();
            if closed {
                // Registrations are invitations to this pairing attempt, not durable public
                // inventory. A new window starts with its whole capacity available.
                self.platform
                    .exec("DELETE FROM oauth_clients WHERE last_used_at IS NULL", [])?;
            }
            self.platform.exec(
                "INSERT INTO oauth_pairing(id,open_until,opened_by,opened_at) VALUES (1,?1,?2,?3) \
                 ON CONFLICT(id) DO UPDATE SET open_until=?1,opened_by=?2,opened_at=?3",
                [open_until.as_str(), owner, &iso()],
            )?;
            if closed {
                self.audit_with(
                    Some(owner),
                    None,
                    "agent.pairing_opened",
                    "",
                    None,
                    &[("openUntil", None, Some(open_until.clone()))],
                )?;
            }
            Ok(())
        })?;
        Ok(OAuthPairingOpened { open_until })
    }

    /// Whether the owner lets this kind of app connect.
    pub fn oauth_app_allowed(&self, app: OAuthAppId) -> Result<bool> {
        let chosen = self
            .platform
            .one_as::<(i64,)>("SELECT allowed FROM oauth_apps WHERE id=?", [app.as_str()])?;
        Ok(chosen.map_or(named(app).1, |(allowed,)| allowed == 1))
    }

    /// Whether the owner lets the app with this client id connect, sent back to this redirect.
    pub fn oauth_client_allowed(&self, client_id: &str, redirect_uri: &str) -> Result<bool> {
        match app_of(client_id, Some(redirect_uri)) {
            Some(app) => self.oauth_app_allowed(app),
            None => Ok(false),
        }
    }

    /// Every kind of app, and whether it may connect.
    pub fn oauth_apps(&self) -> Result<OAuthAllowedApps> {
        let chosen: HashMap<String, bool> = self
            .platform
            .query_as::<(String, i64)>("SELECT id,allowed FROM oauth_apps", [])?
            .into_iter()
            .map(|(id, allowed)| (id, allowed == 1))
            .collect();
        Ok(OAuthAllowedApps {
            apps: APPS
                .iter()
                .map(|(id, name, default)| OAuthAllowedApp {
                    id: *id,
                    name: (*name).to_owned(),
                    allowed: chosen.get(id.as_str()).copied().unwrap_or(*default),
                })
                .collect(),
        })
    }

    /// The owner lets a kind of app connect, or stops it from connecting. Apps of that kind
    /// connected already stay connected; each is revoked on its own. Logged when it changes.
    pub fn allow_oauth_app(
        &self,
        owner: &str,
        choice: &OAuthAppChoice,
    ) -> Result<OAuthAllowedApps> {
        self.platform.transaction(|| {
            if self.oauth_app_allowed(choice.id)? == choice.allowed {
                return Ok(());
            }
            self.platform.exec(
                "INSERT INTO oauth_apps(id,allowed,changed_by,changed_at) VALUES (?1,?2,?3,?4) \
                 ON CONFLICT(id) DO UPDATE SET allowed=?2,changed_by=?3,changed_at=?4",
                rusqlite::params![choice.id, i64::from(choice.allowed), owner, iso()],
            )?;
            let action = if choice.allowed {
                "agent.app_allowed"
            } else {
                "agent.app_disallowed"
            };
            self.audit_with(
                Some(owner),
                None,
                action,
                choice.id.as_str(),
                Some(named(choice.id).0),
                &[],
            )
        })?;
        self.oauth_apps()
    }

    /// Whether an authorization request may go on, decided before anything is fetched or
    /// stored for it: the kind of app it comes from, or the page a refused one is sent to.
    /// A client id that is no app's, or a website's while websites may not connect, is an
    /// unknown app; a kind the owner turned off is not allowed, and the page names it; and
    /// with the window closed, nothing may ask.
    pub fn admit_authorize(
        &self,
        query: &super::Query,
    ) -> Result<std::result::Result<OAuthAppId, String>> {
        let origin = issuer(&self.config);
        let page = |error: &str| Ok(Err(format!("{origin}/#authorize?error={error}")));
        let Ok(Some(client_id)) = query.one("client_id") else {
            return page("unknown_app");
        };
        let redirect_uri = query.one("redirect_uri").ok().flatten();
        let Some(app) = app_of(client_id, redirect_uri) else {
            return page("unknown_app");
        };
        if !self.oauth_app_allowed(app)? {
            if app == OAuthAppId::Web && !client_id.starts_with("dcr_") {
                return page("unknown_app");
            }
            return page(&format!("app_not_allowed&app={}", app.as_str()));
        }
        if self.oauth_pairing()?.open_until.is_none() {
            return page("pairing_closed");
        }
        Ok(Ok(app))
    }
}

#[cfg(test)]
#[path = "../../tests/backend/oauth/guard.rs"]
mod tests;
