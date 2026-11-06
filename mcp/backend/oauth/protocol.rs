use dispatch_core::{
    Error,
    foundation::{config::Config, crypto},
};
use serde_json::{Value, json};

/// The one granted scope. `offline_access` is tolerated as a client hint because Dispatch
/// always issues a rotating refresh token; no unadvertised data scope is accepted.
pub const SCOPE: &str = "dispatch";

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
pub(super) fn grant(description: &str) -> Refusal {
    Refusal::new("invalid_grant", description)
}

/// A missing scope means the one advertised scope. Clients may also ask for the conventional
/// `offline_access` hint; every other name is refused rather than silently broadened or ignored.
pub(super) fn check_scope(query: &Query) -> Answer<()> {
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
    pub(super) fn all(&self, name: &str) -> Vec<&str> {
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
    pub(super) fn required(&self, name: &str) -> Answer<&str> {
        self.one(name)?
            .ok_or_else(|| Refusal::new("invalid_request", &format!("{name} is required")))
    }
}

/// `uri` with parameters added to its query, skipping those without a value. A redirect
/// keeps the query it was registered with.
pub(super) fn redirect(uri: &str, params: &[(&str, Option<&str>)]) -> String {
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
pub(super) fn verifies(verifier: &str, challenge: &str) -> bool {
    (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
        && crypto::equal(&crypto::s256(verifier), challenge)
}
