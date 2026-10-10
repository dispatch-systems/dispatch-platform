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

pub use clients::Documents;

use super::{Caller, token, token::Kind, toolbox::Toolbox};
use crate::api::types::{
    AgentAccess, AgentKeyRequest, OAuthApp, OAuthApproval, OAuthRedirect, OAuthReplaced,
    OAuthRequest, ToolLevel,
};
use crate::{ClientStore, GuardStore, keys};
use clients::Client;
use dispatch_core::{
    Error, Result,
    db::{Store, at, iso, now, s},
    ensure,
    foundation::{config::Config, crypto},
};
use notices::Told;
use serde_json::{Value, json};

/// The one granted scope. `offline_access` is tolerated as a client hint because Dispatch
/// always issues a rotating refresh token; no unadvertised data scope is accepted.
pub const SCOPE: &str = "dispatch";
const REQUEST_LIFETIME: i64 = 10 * 60 * 1000;
const CODE_LIFETIME: i64 = 5 * 60 * 1000;
/// Seconds, as the token endpoint says it.
const ACCESS_SECONDS: i64 = 60 * 60;
const REFRESH_LIFETIME: i64 = 30 * 24 * 60 * 60 * 1000;
/// Used codes are kept this long past their expiry, so a replay still ends what they made.
const CODE_KEPT: i64 = 24 * 60 * 60 * 1000;
const STATE_LONGEST: usize = 2048;

/// Where an authorization request sends the browser and, when it made a request, what the
/// browser keeps to show it is the one the app sent: the request's id and its nonce.
pub struct Authorized {
    pub location: String,
    pub browser: Option<(String, String)>,
}

/// The issuer: the origin exactly as configured, which never ends in a slash.
pub fn issuer(config: &Config) -> &str {
    &config.origin
}
/// The MCP endpoint: the one resource every token is for.
pub fn resource(config: &Config) -> String {
    format!("{}/api/v1/mcp", config.origin)
}
/// Where the MCP endpoint's protected resource metadata is, as every 401 names it.
pub fn resource_metadata(config: &Config) -> String {
    format!(
        "{}/.well-known/oauth-protected-resource/api/v1/mcp",
        config.origin
    )
}

/// The protected resource metadata (RFC 9728).
pub fn protected_resource(config: &Config) -> Value {
    json!({
        "resource": resource(config),
        "authorization_servers": [issuer(config)],
        "scopes_supported": [SCOPE],
        "bearer_methods_supported": ["header"],
        "resource_name": "Dispatch",
    })
}

/// The authorization server metadata (RFC 8414). Only public clients exist: a client offered
/// any other way to authenticate may pick it.
pub fn authorization_server(config: &Config) -> Value {
    let issuer = issuer(config);
    json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/oauth/authorize"),
        "token_endpoint": format!("{issuer}/oauth/token"),
        "registration_endpoint": format!("{issuer}/oauth/register"),
        "revocation_endpoint": format!("{issuer}/oauth/revoke"),
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": [SCOPE],
        "client_id_metadata_document_supported": true,
        "authorization_response_iss_parameter_supported": true,
    })
}

/// An OAuth error answer (RFC 6749 §5.2): the code a client acts on, and why in words.
#[derive(Debug)]
pub struct Refusal {
    pub error: &'static str,
    pub description: String,
    pub status: u16,
}
impl Refusal {
    pub fn new(error: &'static str, description: &str) -> Self {
        Self {
            error,
            description: description.to_owned(),
            status: if error == "invalid_client" { 401 } else { 400 },
        }
    }
}
/// Dispatch's own failure, said the OAuth way.
impl From<Error> for Refusal {
    fn from(error: Error) -> Self {
        let (code, status) = match error.status {
            429 => ("rate_limited", 429),
            503 => ("temporarily_unavailable", 503),
            500.. => ("server_error", 500),
            _ => ("invalid_request", 400),
        };
        Self {
            error: code,
            description: error.code,
            status,
        }
    }
}
/// What a protocol request is answered with, or the OAuth error it is refused with.
pub type Answer<T> = std::result::Result<T, Refusal>;
fn grant(description: &str) -> Refusal {
    Refusal::new("invalid_grant", description)
}

/// A missing scope means the one advertised scope. Clients may also ask for the conventional
/// `offline_access` hint; every other name is refused rather than silently broadened or ignored.
fn check_scope(query: &Query) -> Answer<()> {
    let Some(scope) = query.one("scope")? else {
        return Ok(());
    };
    let scopes: Vec<_> = scope.split_ascii_whitespace().collect();
    if scopes.contains(&SCOPE)
        && scopes
            .iter()
            .all(|scope| matches!(*scope, SCOPE | "offline_access"))
    {
        Ok(())
    } else {
        Err(Refusal::new(
            "invalid_scope",
            "The only supported scope is dispatch",
        ))
    }
}

/// A query or form as sent, every parameter in order. An empty value counts as absent.
#[derive(Clone)]
pub struct Query(Vec<(String, String)>);
impl Query {
    pub fn parse(text: &[u8]) -> Self {
        Self(url::form_urlencoded::parse(text).into_owned().collect())
    }
    fn all(&self, name: &str) -> Vec<&str> {
        self.0
            .iter()
            .filter(|(key, value)| key == name && !value.is_empty())
            .map(|(_, value)| value.as_str())
            .collect()
    }
    /// The parameter's one value. One sent more than once is refused (RFC 6749 §3.1).
    pub fn one(&self, name: &str) -> Answer<Option<&str>> {
        match self.all(name)[..] {
            [] => Ok(None),
            [value] => Ok(Some(value)),
            _ => Err(Refusal::new(
                "invalid_request",
                &format!("{name} was sent more than once"),
            )),
        }
    }
    fn required(&self, name: &str) -> Answer<&str> {
        self.one(name)?
            .ok_or_else(|| Refusal::new("invalid_request", &format!("{name} is required")))
    }
}

/// `uri` with parameters added to its query, skipping those without a value. A redirect
/// keeps the query it was registered with.
fn redirect(uri: &str, params: &[(&str, Option<&str>)]) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in params {
        if let Some(value) = value {
            query.append_pair(name, value);
        }
    }
    let separator = if uri.contains('?') { '&' } else { '?' };
    format!("{uri}{separator}{}", query.finish())
}

/// Whether `challenge` is BASE64URL(SHA-256(`verifier`)), for a verifier of 43 to 128
/// unreserved characters (RFC 7636 §4.1, §4.6).
fn verifies(verifier: &str, challenge: &str) -> bool {
    (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
        && crypto::equal(&crypto::s256(verifier), challenge)
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

    /// The owner refuses: the app is told so.
    fn deny_oauth(&self, id: &str, browser: &str) -> Result<OAuthRedirect>;

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
        match self.throttle_ip("oauth-authorize", ip, 60, 10 * 60 * 1000) {
            Err(error) if error.code == "rate_limited" => Ok(Some(format!(
                "{}/#authorize?error=rate_limited",
                issuer(&self.config)
            ))),
            result => result.map(|()| None),
        }
    }

    fn authorize_oauth(
        &self,
        query: &Query,
        document: Option<std::result::Result<Client, &'static str>>,
    ) -> Result<Authorized> {
        let nonce = crypto::token()?;
        Ok(match request_oauth(self, query, document, &nonce)? {
            Ok(id) => Authorized {
                location: format!("{}/#authorize?request={id}", issuer(&self.config)),
                browser: Some((id, nonce)),
            },
            Err(location) => Authorized {
                location,
                browser: None,
            },
        })
    }

    fn oauth_request(&self, id: &str, name: Option<&str>, browser: &str) -> Result<OAuthRequest> {
        let request = waiting_request(self, id, browser)?;
        let (redirect_host, redirect_scheme) = clients::destination(s(&request, "redirect_uri"));
        let replaced = replaced_apps(
            self,
            name.unwrap_or(s(&request, "client_name")),
            s(&request, "client_id"),
            s(&request, "client_name"),
            request["verified"] == 1,
        )?;
        let replaces = match replaced.first() {
            Some(id) => {
                let earlier = keys::agent_key(self, id)?;
                Some(OAuthReplaced {
                    name: earlier.name,
                    connected_at: earlier.created_at,
                })
            }
            None => None,
        };
        Ok(OAuthRequest {
            id: s(&request, "id").to_owned(),
            app: OAuthApp {
                name: s(&request, "client_name").to_owned(),
                client_id: s(&request, "client_id").to_owned(),
                known: request["verified"] == 1,
                redirect_host,
                redirect_scheme,
            },
            expires_at: s(&request, "expires_at").to_owned(),
            replaces,
        })
    }

    fn approve_oauth(
        &self,
        owner: &str,
        id: &str,
        approval: &OAuthApproval,
        browser: &str,
    ) -> Result<OAuthRedirect> {
        let request = waiting_request(self, id, browser)?;
        ensure(
            self.oauth_client_allowed(s(&request, "client_id"), s(&request, "redirect_uri"))?,
            "app_not_allowed",
            403,
        )?;
        // Connecting an app again under its name replaces its earlier connection.
        let replaced = replaced_apps(
            self,
            &approval.name,
            s(&request, "client_id"),
            s(&request, "client_name"),
            request["verified"] == 1,
        )?;
        keys::check_agent_key(self, None, &approval.key(), &replaced)?;
        let code = crypto::token()?;
        // What an older release redeeming the code would let the app read: nothing. `tools`
        // is its own word for something else, and `agent_tools` the tools it may use at all.
        let allowed: Vec<&String> = approval.tools.keys().collect();
        let choices = json!({
            "name": approval.name,
            "all_dsps": approval.all_dsps,
            "dsps": approval.dsps,
            "all_tools": approval.all_tools,
            "tool_levels": approval.tools,
            "agent_tools": allowed,
            "reads": {"areas": [], "bypass": false},
            "tools": "full",
            "locations": false,
        });
        self.platform.transaction(|| {
            answer_request(self, id)?;
            self.platform.exec(
                "INSERT INTO oauth_codes(hash,client_id,client_name,client_verified,redirect_uri,\
                 code_challenge,resource,choices,approved_by,created_at,expires_at) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,?)",
                rusqlite::params![
                    crypto::sha(&code),
                    s(&request, "client_id"),
                    s(&request, "client_name"),
                    request["verified"].as_i64(),
                    s(&request, "redirect_uri"),
                    s(&request, "code_challenge"),
                    s(&request, "resource"),
                    choices.to_string(),
                    owner,
                    iso(),
                    at(now() + CODE_LIFETIME)
                ],
            )?;
            Ok(())
        })?;
        Ok(OAuthRedirect {
            redirect: redirect(
                s(&request, "redirect_uri"),
                &[
                    ("code", Some(&code)),
                    ("state", request["state"].as_str()),
                    ("iss", Some(issuer(&self.config))),
                ],
            ),
        })
    }

    fn deny_oauth(&self, id: &str, browser: &str) -> Result<OAuthRedirect> {
        let request = waiting_request(self, id, browser)?;
        answer_request(self, id)?;
        Ok(OAuthRedirect {
            redirect: redirect(
                s(&request, "redirect_uri"),
                &[
                    ("error", Some("access_denied")),
                    ("state", request["state"].as_str()),
                    ("iss", Some(issuer(&self.config))),
                ],
            ),
        })
    }

    fn oauth_token(&self, form: &Query) -> Answer<Value> {
        let grant_type = form.required("grant_type")?;
        let client_id = form.required("client_id")?;
        if form.one("resource")? != Some(resource(&self.config).as_str()) {
            return Err(Refusal::new(
                "invalid_target",
                "resource must be Dispatch's MCP endpoint",
            ));
        }
        check_scope(form)?;
        let known = clients::known(client_id)
            || (client_id.starts_with("dcr_") && self.registered_client(client_id)?.is_some())
            || (client_id.starts_with("https://") && website_approved(self, client_id)?);
        if !known {
            return Err(Refusal::new(
                "invalid_client",
                "The client is not registered",
            ));
        }
        match grant_type {
            "authorization_code" => redeem_code(self, form, client_id),
            "refresh_token" => refresh(self, form, client_id),
            _ => Err(Refusal::new(
                "unsupported_grant_type",
                "Only authorization_code and refresh_token are supported",
            )),
        }
    }

    fn revoke_oauth(&self, form: &Query) -> Answer<()> {
        let presented = form.required("token")?;
        let client_id = form.one("client_id")?;
        let row = self.platform.one(
            "SELECT t.key_id,k.client_id FROM oauth_tokens t JOIN agent_keys k ON k.id=t.key_id \
             WHERE t.hash=?",
            [crypto::sha(presented)],
        )?;
        if let Some(row) = row
            && client_id.is_none_or(|client| client == s(&row, "client_id"))
        {
            end_app(self, s(&row, "key_id"), "signed_out")?;
        }
        Ok(())
    }

    fn authenticate_app(&self, token: &str, client: &str) -> Result<Caller> {
        ensure(
            token::well_formed(token, Kind::Access, self.config.env()),
            "access_token_invalid",
            401,
        )?;
        let row = self
            .platform
            .one(
                "SELECT t.expires_at token_expires_at,t.resource,k.id,k.name,k.user_id,k.all_dsps,\
                 k.access,k.all_tools,k.expires_at,k.revoked_at FROM oauth_tokens t \
                 JOIN agent_keys k ON k.id=t.key_id WHERE t.hash=? AND t.kind='access'",
                [crypto::sha(token)],
            )?
            .ok_or_else(|| Error::new("access_token_invalid", 401))?;
        ensure(
            s(&row, "resource") == resource(&self.config),
            "access_token_invalid",
            401,
        )?;
        ensure(
            s(&row, "token_expires_at") > iso().as_str(),
            "access_token_expired",
            401,
        )?;
        keys::agent_caller(self, &row, client)
    }

    fn prune_oauth(&self) -> Result<()> {
        let day = 24 * 60 * 60 * 1000;
        self.platform.transaction(|| {
            self.platform
                .exec("DELETE FROM oauth_requests WHERE expires_at<?", [iso()])?;
            self.platform.exec(
                "DELETE FROM oauth_codes WHERE expires_at<?",
                [at(now() - CODE_KEPT)],
            )?;
            self.platform
                .exec("DELETE FROM oauth_tokens WHERE expires_at<?", [iso()])?;
            self.platform.exec(
                "DELETE FROM oauth_clients WHERE (last_used_at IS NULL AND created_at<?1) \
                 OR last_used_at<?2",
                [at(now() - clients::UNUSED_LIFETIME), at(now() - 90 * day)],
            )?;
            Ok(())
        })
    }
}

/// The request made, waiting for the owner in the browser given `nonce`; or where a
/// refused one goes.
fn request_oauth(
    db: &Store,
    query: &Query,
    document: Option<std::result::Result<Client, &'static str>>,
    nonce: &str,
) -> Result<std::result::Result<String, String>> {
    let origin = issuer(&db.config);
    let page = |error: &str| Ok(Err(format!("{origin}/#authorize?error={error}")));
    // Asked again: the window may have closed while the document was fetched.
    if let Err(refused) = db.admit_authorize(query)? {
        return Ok(Err(refused));
    }
    let Ok(Some(client_id)) = query.one("client_id") else {
        return page("unknown_app");
    };
    let client = match document {
        Some(Ok(client)) => client,
        Some(Err(code)) => return page(code),
        None if client_id.starts_with("dcr_") => match db.registered_client(client_id)? {
            Some(client) => client,
            None => return page("unknown_app"),
        },
        None => return page("unknown_app"),
    };
    let redirect_uri = match query.one("redirect_uri") {
        Ok(Some(uri)) if clients::allowed(&client.redirect_uris, uri) => uri,
        _ => return page("invalid_redirect"),
    };
    // From here on the app hears why, at the redirect it registered.
    let state = query.one("state").ok().flatten();
    let back = |error: &str, description: &str| {
        Ok(Err(redirect(
            redirect_uri,
            &[
                ("error", Some(error)),
                ("error_description", Some(description)),
                ("state", state),
                ("iss", Some(origin)),
            ],
        )))
    };
    let checked = (|| -> Answer<(&str, String)> {
        for name in [
            "response_type",
            "code_challenge",
            "code_challenge_method",
            "state",
            "scope",
        ] {
            query.one(name)?;
        }
        if query.one("response_type")? != Some("code") {
            return Err(Refusal::new(
                "unsupported_response_type",
                "Only the code response type is supported",
            ));
        }
        let challenge = query
            .one("code_challenge")?
            .ok_or_else(|| Refusal::new("invalid_request", "code_challenge is required"))?;
        if query.one("code_challenge_method")? != Some("S256") {
            return Err(Refusal::new(
                "invalid_request",
                "code_challenge_method must be S256",
            ));
        }
        if challenge.len() != 43
            || !challenge
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        {
            return Err(Refusal::new(
                "invalid_request",
                "code_challenge must be an S256 challenge",
            ));
        }
        let resource = resource(&db.config);
        let resources = query.all("resource");
        if resources.is_empty() || resources.iter().any(|value| *value != resource) {
            return Err(Refusal::new(
                "invalid_target",
                "resource must be Dispatch's MCP endpoint",
            ));
        }
        check_scope(query)?;
        if state.is_some_and(|state| state.len() > STATE_LONGEST) {
            return Err(Refusal::new("invalid_request", "state is too long"));
        }
        Ok((challenge, resource))
    })();
    let (challenge, resource) = match checked {
        Ok(checked) => checked,
        Err(refusal) => return back(refusal.error, &refusal.description),
    };
    let id = crypto::id("authreq")?;
    db.platform.exec(
        "INSERT INTO oauth_requests(id,client_id,client_name,verified,redirect_uri,state,\
         code_challenge,resource,scope,created_at,expires_at,browser) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
        rusqlite::params![
            id,
            client.id,
            client.name,
            i64::from(client.known),
            redirect_uri,
            state,
            challenge,
            resource,
            SCOPE,
            iso(),
            at(now() + REQUEST_LIFETIME),
            crypto::sha(nonce)
        ],
    )?;
    Ok(Ok(id))
}

/// A request still waiting for the owner's answer, in the browser the app sent to
/// Dispatch: `browser` is the nonce that browser was given. Any other browser, such as
/// one that was sent a link to the request, is refused.
fn waiting_request(db: &Store, id: &str, browser: &str) -> Result<Value> {
    let request = db
        .platform
        .one(
            "SELECT * FROM oauth_requests WHERE id=? AND expires_at>?",
            [id.to_owned(), iso()],
        )?
        .ok_or_else(|| Error::new("authorization_not_found", 404))?;
    let held = request["browser"]
        .as_str()
        .is_some_and(|hash| crypto::equal(hash, &crypto::sha(browser)));
    ensure(held, "wrong_browser", 403)?;
    Ok(request)
}

/// Answers a request once: a second answer finds nothing.
fn answer_request(db: &Store, id: &str) -> Result<()> {
    let answered = db.platform.exec(
        "DELETE FROM oauth_requests WHERE id=? AND expires_at>?",
        [id.to_owned(), iso()],
    )?;
    ensure(answered == 1, "authorization_not_found", 404)
}

/// A code for the connected app it was approved as, and its first tokens.
fn redeem_code(db: &Store, form: &Query, client_id: &str) -> Answer<Value> {
    let code = form.required("code")?;
    let verifier = form.required("code_verifier")?;
    let redirect_uri = form.required("redirect_uri")?;
    let row = db
        .platform
        .one(
            "SELECT * FROM oauth_codes WHERE hash=?",
            [crypto::sha(code)],
        )?
        .ok_or_else(|| grant("The code is not one Dispatch issued"))?;
    // Only the client that asked, at the redirect it asked for, with the verifier it
    // began with. A mismatch ends nothing: it proves no one held the code rightly.
    if s(&row, "client_id") != client_id
        || s(&row, "redirect_uri") != redirect_uri
        || !verifies(verifier, s(&row, "code_challenge"))
    {
        return Err(grant(
            "The code was issued for another client, redirect or verifier",
        ));
    }
    if !row["used_at"].is_null() {
        // Redeemed once already, and presented again in full: whoever holds it, the
        // connected app it made can no longer be trusted.
        if let Some(key) = row["key_id"].as_str() {
            end_app(db, key, "code_reused")?;
        }
        return Err(grant("The code was already used"));
    }
    if s(&row, "expires_at") <= iso().as_str() {
        return Err(grant("The code expired"));
    }
    if !db.oauth_client_allowed(client_id, redirect_uri)? {
        return Err(grant("The owner no longer lets this kind of app connect"));
    }
    let choices: Value = serde_json::from_str(s(&row, "choices")).map_err(Error::from)?;
    // Approved before tools had levels, each tool it may use reads; approved before tools
    // were chosen, every tool reads, and those added later.
    let (all_tools, tools) = match (choices.get("tool_levels"), choices.get("agent_tools")) {
        (Some(levels), _) => (
            choices["all_tools"] == true,
            serde_json::from_value(levels.clone()).map_err(Error::from)?,
        ),
        (None, Some(tools)) => (
            choices["all_tools"] == true,
            serde_json::from_value::<Vec<String>>(tools.clone())
                .map_err(Error::from)?
                .into_iter()
                .map(|tool| (tool, ToolLevel::Read))
                .collect(),
        ),
        (None, None) => (true, Toolbox::installed().defaults()),
    };
    let key = AgentKeyRequest {
        name: s(&choices, "name").to_owned(),
        all_dsps: choices["all_dsps"] == true,
        dsps: serde_json::from_value(choices["dsps"].clone()).map_err(Error::from)?,
        access: AgentAccess::Read,
        all_tools,
        tools,
        expires_at: None,
    };
    let owner = s(&row, "approved_by");
    if db.active_platform_owner(owner)?.is_none() {
        return Err(grant("The approval no longer stands"));
    }
    // What the owner chose is checked again, as a key's would be when it is made.
    let replaced = replaced_apps(
        db,
        &key.name,
        client_id,
        s(&row, "client_name"),
        row["client_verified"] == 1,
    )?;
    keys::check_agent_key(db, None, &key, &replaced).map_err(|error| match error.status {
        500.. => error.into(),
        _ => grant(&format!(
            "The approval can no longer be used: {}",
            error.code
        )),
    })?;
    let id = crypto::id("agentkey")?;
    let tokens = db.platform.transaction(|| {
        let redeemed = db.platform.exec(
            "UPDATE oauth_codes SET used_at=?1,key_id=?2 WHERE hash=?3 AND used_at IS NULL",
            [iso(), id.clone(), crypto::sha(code)],
        )?;
        ensure(redeemed == 1, "code_already_used", 409)?;
        // The app connected again: its earlier connection of the same name ends with it.
        for earlier in &replaced {
            end_app_within(db, Some(owner), earlier, "replaced")?;
        }
        // What an older release reads it may read, run again in a rollback: nothing, as a
        // key's.
        db.platform.exec(
            "INSERT INTO agent_keys(id,name,hash,hint,user_id,all_dsps,access,tools,locations,\
             areas,bypass,created_at,kind,client_id,client_name,client_verified) \
             VALUES (?,?,?,'',?,?,?,'full',0,'',0,?,'app',?,?,?)",
            rusqlite::params![
                id,
                key.name,
                format!("app:{id}"),
                owner,
                i64::from(key.all_dsps),
                key.access,
                iso(),
                client_id,
                s(&row, "client_name"),
                row["client_verified"].as_i64()
            ],
        )?;
        keys::set_agent_key_dsps(db, &id, &key)?;
        keys::set_agent_key_tools(db, &id, &key)?;
        db.audit_with(
            Some(owner),
            None,
            "agent.app_connected",
            &id,
            Some(&key.name),
            &[],
        )?;
        issue_tokens(db, &id, client_id)
    })?;
    notices::tell_owners(db, &id, Told::Connected { redirect_uri });
    Ok(tokens)
}

/// Whether a website's client id was ever approved, and so may use the token endpoint.
fn website_approved(db: &Store, client_id: &str) -> Result<bool> {
    Ok(db.platform.count(
        "SELECT EXISTS(SELECT 1 FROM oauth_codes WHERE client_id=?1) \
         OR EXISTS(SELECT 1 FROM agent_keys WHERE kind='app' AND client_id=?1)",
        [client_id],
    )? == 1)
}

/// A refresh token for a new pair. The one presented is spent exactly once; presenting it
/// again proves that the app's token family may be compromised and ends the connection.
fn refresh(db: &Store, form: &Query, client_id: &str) -> Answer<Value> {
    let presented = form.required("refresh_token")?;
    if !token::well_formed(presented, Kind::Refresh, db.config.env()) {
        return Err(grant("The refresh token is not one Dispatch issued"));
    }
    let row = db
        .platform
        .one(
            "SELECT t.key_id,t.resource,t.expires_at,t.used_at,k.client_id,k.revoked_at,\
             k.user_id FROM oauth_tokens t JOIN agent_keys k ON k.id=t.key_id \
             WHERE t.hash=? AND t.kind='refresh'",
            [crypto::sha(presented)],
        )?
        .ok_or_else(|| grant("The refresh token is not one Dispatch issued"))?;
    if s(&row, "client_id") != client_id || s(&row, "resource") != resource(&db.config) {
        return Err(grant("The refresh token was issued to another client"));
    }
    if s(&row, "expires_at") <= iso().as_str() {
        return Err(grant("The refresh token expired"));
    }
    let live =
        row["revoked_at"].is_null() && db.active_platform_owner(s(&row, "user_id"))?.is_some();
    if !live {
        return Err(grant("The connection was ended"));
    }
    let key = s(&row, "key_id").to_owned();
    if !row["used_at"].is_null() {
        end_app(db, &key, "refresh_reused")?;
        return Err(grant("The refresh token was already used"));
    }
    let hash = crypto::sha(presented);
    match db.platform.transaction(|| {
        // Only the transaction that wins this unused-row compare-and-swap writes either
        // exchange marker and issues the replacement pair.
        let exchanged = db.platform.exec(
            "UPDATE oauth_tokens SET used_at=?1,replaced_at=?1 \
             WHERE hash=?2 AND kind='refresh' AND used_at IS NULL",
            [iso(), hash],
        )?;
        ensure(exchanged == 1, "refresh_reused", 409)?;
        issue_tokens(db, &key, client_id)
    }) {
        Ok(tokens) => Ok(tokens),
        Err(error) if error.code == "refresh_reused" => {
            end_app(db, &key, "refresh_reused")?;
            Err(grant("The refresh token was already used"))
        }
        Err(error) => Err(error.into()),
    }
}

/// The live connections a new connection named `name` replaces: the same app's, under the
/// same name. The same app is the same client; or, for apps that registered themselves and
/// so get a new client each time, one that gave the same name. A known app and a
/// self-registered one are never the same, whatever they call themselves.
fn replaced_apps(
    db: &Store,
    name: &str,
    client_id: &str,
    client_name: &str,
    verified: bool,
) -> Result<Vec<String>> {
    Ok(db
        .platform
        .query_as::<(String,)>(
            "SELECT id FROM agent_keys WHERE kind='app' AND revoked_at IS NULL \
             AND lower(name)=lower(?1) AND (client_id=?2 OR (?3=0 AND client_verified=0 \
             AND lower(trim(client_name))=lower(trim(?4))))",
            rusqlite::params![name, client_id, i64::from(verified), client_name],
        )?
        .into_iter()
        .map(|(id,)| id)
        .collect())
}

/// A new access and refresh token for a connected app, as the token endpoint answers.
fn issue_tokens(db: &Store, key: &str, client_id: &str) -> Result<Value> {
    let environment = db.config.env();
    let access = token::new(Kind::Access, environment)?;
    let refresh = token::new(Kind::Refresh, environment)?;
    for (value, kind, lifetime) in [
        (&access, "access", ACCESS_SECONDS * 1000),
        (&refresh, "refresh", REFRESH_LIFETIME),
    ] {
        db.platform.exec(
            "INSERT INTO oauth_tokens(hash,key_id,kind,resource,created_at,expires_at) \
             VALUES (?,?,?,?,?,?)",
            [
                crypto::sha(value),
                key.to_owned(),
                kind.to_owned(),
                resource(&db.config),
                iso(),
                at(now() + lifetime),
            ],
        )?;
    }
    db.used_oauth_client(client_id)?;
    Ok(json!({
        "access_token": access,
        "token_type": "Bearer",
        "expires_in": ACCESS_SECONDS,
        "refresh_token": refresh,
        "scope": SCOPE,
    }))
}

/// Ends a connected app from the protocol's side: a code or refresh token replayed, or the
/// app signing out. Its tokens stop at once. A replay is Dispatch's own doing, so the
/// platform owners are told why.
fn end_app(db: &Store, key: &str, reason: &str) -> Result<()> {
    let ended = db
        .platform
        .transaction(|| end_app_within(db, None, key, reason))?;
    if ended && matches!(reason, "code_reused" | "refresh_reused") {
        notices::tell_owners(db, key, Told::Disconnected { reason });
    }
    Ok(())
}

/// The same inside a transaction the caller holds, such as the one connecting the app's
/// replacement. `actor` is the owner when it is their doing. Whether it was live until now.
fn end_app_within(db: &Store, actor: Option<&str>, key: &str, reason: &str) -> Result<bool> {
    let ended = db.platform.exec(
        "UPDATE agent_keys SET revoked_at=? WHERE id=? AND kind='app' AND revoked_at IS NULL",
        [iso(), key.to_owned()],
    )?;
    db.platform
        .exec("DELETE FROM oauth_tokens WHERE key_id=?", [key])?;
    if ended > 0 {
        let (name,): (String,) = db
            .platform
            .one_as("SELECT name FROM agent_keys WHERE id=?", [key])?
            .ok_or_else(|| Error::new("agent_key_not_found", 404))?;
        db.audit_with(
            actor,
            None,
            "agent.app_revoked",
            key,
            Some(&name),
            &[("reason", None, Some(reason.to_owned()))],
        )?;
    }
    Ok(ended > 0)
}

#[cfg(test)]
#[path = "../../tests/backend/oauth/mod.rs"]
mod tests;
