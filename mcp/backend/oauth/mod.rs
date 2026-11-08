//! Sign in with Dispatch: an OAuth 2.1 authorization server for the MCP endpoint. An app the
//! platform owner approves becomes a connected app: an agent key with no static token, that
//! signs in with an access token good for an hour and renews it with a refresh token good for
//! 30 days, each renewal bringing a new one. It reaches what the owner chose, like a key, and
//! stops when revoked like one. Only the platform owner approves, on the dashboard, and
//! only while they have the pairing window open, for the kinds of app they let connect.
pub mod clients;
pub mod guard;
pub mod limits;
mod mail;
pub mod network;
mod notices;
mod protocol;
mod requests;
mod tokens;

pub use clients::Documents;
pub use protocol::{
    Answer, Query, Refusal, SCOPE, authorization_server, issuer, protected_resource, resource,
    resource_metadata,
};

use super::Caller;
use crate::api::types::{OAuthApproval, OAuthRedirect, OAuthRequest};
use clients::Client;
use dispatch_core::{Result, db::Store};
use serde_json::Value;

/// Where an authorization request sends the browser and, when it made a request, what the
/// browser keeps to show it is the one the app sent: the request's id and its nonce.
pub struct Authorized {
    pub location: String,
    pub browser: Option<(String, String)>,
}

pub trait OAuthStore {
    /// Counts an authorization request against its address, before anything else is done for
    /// it, a known app's document fetched included. Answers where a refused one goes.
    fn throttle_authorize(&self, ip: &str) -> Result<Option<String>>;

    /// Where an authorization request sends the browser: to the approval page with the
    /// request waiting there, or back to the app with why not. Until the app and its redirect
    /// are known good, a refusal goes to Dispatch's own page and never to the app. A known
    /// app's or a website's client comes as `document`, read from its published document
    /// beforehand. Nothing is stored unless the pairing window is open and the owner lets
    /// this kind of app connect. A request is bound to the browser that brought it, by a
    /// nonce only that browser is given: a link to it sent to anyone else is no use.
    fn authorize_oauth(
        &self,
        query: &Query,
        document: Option<std::result::Result<Client, &'static str>>,
    ) -> Result<Authorized>;

    /// An app asking to connect, as the approval page shows it, with the connection that
    /// approving it as `name` would replace. `name` is the app's own name unless given.
    fn oauth_request(&self, id: &str, name: Option<&str>, browser: &str) -> Result<OAuthRequest>;

    /// The owner approves: a code the app redeems once, within five minutes, for the
    /// connected app made with these choices. Checked as a new key is, and refused if the
    /// owner has since stopped this kind of app from connecting.
    fn approve_oauth(
        &self,
        owner: &str,
        id: &str,
        approval: &OAuthApproval,
        browser: &str,
    ) -> Result<OAuthRedirect>;

    /// The owner `owner` refuses: the app is told so.
    fn deny_oauth(&self, owner: &str, id: &str, browser: &str) -> Result<OAuthRedirect>;

    /// The token endpoint: a code or a refresh token exchanged for new tokens. Every client is
    /// public, so the `client_id` it sends must be the one the grant was made to.
    fn oauth_token(&self, form: &Query) -> Answer<Value>;

    /// Revocation (RFC 7009): any token of a connected app ends the whole app, as an app
    /// signing out expects. A token Dispatch does not know changes nothing and is no error.
    fn revoke_oauth(&self, form: &Query) -> Answer<()>;

    /// The connected app an access token signs in as, or why it may not: the token must be
    /// current and for the MCP endpoint, and then the app passes every check a key does.
    fn authenticate_app(&self, token: &str, client: &str) -> Result<Caller>;

    /// Removes what can no longer be used: requests past their ten minutes, codes a day past
    /// their expiry, expired tokens, and apps that registered themselves but were never given
    /// a token during their pairing attempt, or not for 90 days.
    fn prune_oauth(&self) -> Result<()>;
}
impl OAuthStore for Store {
    fn throttle_authorize(&self, ip: &str) -> Result<Option<String>> {
        requests::throttle(self, ip)
    }

    fn authorize_oauth(
        &self,
        query: &Query,
        document: Option<std::result::Result<Client, &'static str>>,
    ) -> Result<Authorized> {
        requests::authorize(self, query, document)
    }

    fn oauth_request(&self, id: &str, name: Option<&str>, browser: &str) -> Result<OAuthRequest> {
        requests::request(self, id, name, browser)
    }

    fn approve_oauth(
        &self,
        owner: &str,
        id: &str,
        approval: &OAuthApproval,
        browser: &str,
    ) -> Result<OAuthRedirect> {
        requests::approve(self, owner, id, approval, browser)
    }

    fn deny_oauth(&self, owner: &str, id: &str, browser: &str) -> Result<OAuthRedirect> {
        requests::deny(self, owner, id, browser)
    }

    fn oauth_token(&self, form: &Query) -> Answer<Value> {
        tokens::exchange(self, form)
    }

    fn revoke_oauth(&self, form: &Query) -> Answer<()> {
        tokens::revoke(self, form)
    }

    fn authenticate_app(&self, token: &str, client: &str) -> Result<Caller> {
        tokens::authenticate(self, token, client)
    }

    fn prune_oauth(&self) -> Result<()> {
        tokens::prune(self)
    }
}

#[cfg(test)]
#[path = "../../tests/backend/oauth/mod.rs"]
mod tests;
