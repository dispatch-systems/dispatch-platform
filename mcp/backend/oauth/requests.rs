//! Authorization requests, browser binding, and the owner's one-use approval or denial.
use super::{
    Answer, Authorized, Query, Refusal, SCOPE,
    clients::{Client, redirects},
    issuer,
    protocol::{check_scope, redirect},
    resource,
    tokens::replaced_apps,
};
use crate::api::types::{OAuthApp, OAuthApproval, OAuthRedirect, OAuthReplaced, OAuthRequest};
use crate::{ClientStore, GuardStore, keys};
use dispatch_core::{
    Error, Result,
    db::{AuditChange, FromRow, Row, Store, at, iso, now},
    ensure,
    foundation::crypto,
};
use serde_json::json;

const REQUEST_LIFETIME: i64 = 10 * 60 * 1000;
const CODE_LIFETIME: i64 = 5 * 60 * 1000;
const STATE_LONGEST: usize = 2048;

pub(super) fn throttle(db: &Store, ip: &str) -> Result<Option<String>> {
    match db.throttle_ip("oauth-authorize", ip, 60, 10 * 60 * 1000) {
        Err(error) if error.code == "rate_limited" => Ok(Some(format!(
            "{}/#authorize?error=rate_limited",
            issuer(&db.config)
        ))),
        result => result.map(|()| None),
    }
}

pub(super) fn authorize(
    db: &Store,
    query: &Query,
    document: Option<std::result::Result<Client, &'static str>>,
) -> Result<Authorized> {
    let nonce = crypto::token()?;
    Ok(match request_oauth(db, query, document, &nonce)? {
        Ok(id) => Authorized {
            location: format!("{}/#authorize?request={id}", issuer(&db.config)),
            browser: Some((id, nonce)),
        },
        Err(location) => Authorized {
            location,
            browser: None,
        },
    })
}

pub(super) fn request(
    db: &Store,
    id: &str,
    name: Option<&str>,
    browser: &str,
) -> Result<OAuthRequest> {
    let request = waiting_request(db, id, browser)?;
    let (redirect_host, redirect_scheme) = redirects::destination(&request.redirect_uri);
    let replaced = replaced_apps(
        db,
        name.unwrap_or(&request.client_name),
        &request.client_id,
        &request.client_name,
        request.verified == 1,
    )?;
    let replaces = match replaced.first() {
        Some(id) => {
            let earlier = keys::agent_key(db, id)?;
            Some(OAuthReplaced {
                name: earlier.name,
                connected_at: earlier.created_at,
            })
        }
        None => None,
    };
    Ok(OAuthRequest {
        id: request.id,
        app: OAuthApp {
            name: request.client_name,
            client_id: request.client_id,
            known: request.verified == 1,
            redirect_host,
            redirect_scheme,
        },
        expires_at: request.expires_at,
        replaces,
    })
}

pub(super) fn approve(
    db: &Store,
    owner: &str,
    id: &str,
    approval: &OAuthApproval,
    browser: &str,
) -> Result<OAuthRedirect> {
    let request = waiting_request(db, id, browser)?;
    ensure(
        db.oauth_client_allowed(&request.client_id, &request.redirect_uri)?,
        "app_not_allowed",
        403,
    )?;
    // Connecting an app again under its name replaces its earlier connection.
    let replaced = replaced_apps(
        db,
        &approval.name,
        &request.client_id,
        &request.client_name,
        request.verified == 1,
    )?;
    keys::check_agent_key(db, None, &approval.key(), &replaced)?;
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
    db.platform.transaction(|| {
        answer_request(db, id)?;
        db.platform.exec(
            "INSERT INTO oauth_codes(hash,client_id,client_name,client_verified,redirect_uri,\
             code_challenge,resource,choices,approved_by,created_at,expires_at) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            rusqlite::params![
                crypto::sha(&code),
                request.client_id,
                request.client_name,
                request.verified,
                request.redirect_uri,
                request.code_challenge,
                request.resource,
                choices.to_string(),
                owner,
                iso(),
                at(now() + CODE_LIFETIME)
            ],
        )?;
        db.audit_with(
            Some(owner),
            None,
            "agent.app_approved",
            "",
            Some(&approval.name),
            &asked_by(&request),
        )
    })?;
    Ok(OAuthRedirect {
        redirect: redirect(
            &request.redirect_uri,
            &[
                ("code", Some(&code)),
                ("state", request.state.as_deref()),
                ("iss", Some(issuer(&db.config))),
            ],
        ),
    })
}

pub(super) fn deny(db: &Store, owner: &str, id: &str, browser: &str) -> Result<OAuthRedirect> {
    let request = waiting_request(db, id, browser)?;
    db.platform.transaction(|| {
        answer_request(db, id)?;
        db.audit_with(
            Some(owner),
            None,
            "agent.app_denied",
            "",
            Some(&request.client_name),
            &asked_by(&request),
        )
    })?;
    Ok(OAuthRedirect {
        redirect: redirect(
            &request.redirect_uri,
            &[
                ("error", Some("access_denied")),
                ("state", request.state.as_deref()),
                ("iss", Some(issuer(&db.config))),
            ],
        ),
    })
}

/// Which app asked, as a decision's audit event keeps it: its client ID, which its name can't
/// stand in for, as an app that isn't verified names itself; and whether it's verified.
fn asked_by(request: &WaitingRequest) -> [AuditChange; 2] {
    [
        ("client", None, Some(request.client_id.clone())),
        ("verified", None, Some((request.verified == 1).to_string())),
    ]
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
        Ok(Some(uri)) if redirects::allowed(&client.redirect_uris, uri) => uri,
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

struct WaitingRequest {
    id: String,
    client_id: String,
    client_name: String,
    verified: i64,
    redirect_uri: String,
    state: Option<String>,
    code_challenge: String,
    resource: String,
    expires_at: String,
    browser: Option<String>,
}
impl FromRow for WaitingRequest {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            client_id: row.get("client_id")?,
            client_name: row.get("client_name")?,
            verified: row.get("verified")?,
            redirect_uri: row.get("redirect_uri")?,
            state: row.get("state")?,
            code_challenge: row.get("code_challenge")?,
            resource: row.get("resource")?,
            expires_at: row.get("expires_at")?,
            browser: row.get("browser")?,
        })
    }
}

/// A request still waiting for the owner's answer, in the browser the app sent to
/// Dispatch: `browser` is the nonce that browser was given. Any other browser, such as
/// one that was sent a link to the request, is refused.
fn waiting_request(db: &Store, id: &str, browser: &str) -> Result<WaitingRequest> {
    let request = db
        .platform
        .one_as::<WaitingRequest>(
            "SELECT * FROM oauth_requests WHERE id=? AND expires_at>?",
            [id.to_owned(), iso()],
        )?
        .ok_or_else(|| Error::new("authorization_not_found", 404))?;
    let held = request
        .browser
        .as_deref()
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
