//! What a handler receives and what it answers with.
use crate::{Error, Result, accounts::SessionLifetime, ensure, foundation::validate as v};
use axum::{
    Json,
    body::Bytes,
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

#[derive(Clone)]
pub(super) struct FeatureOwner(pub &'static str);

#[derive(Clone)]
pub struct Input {
    pub path: String,
    pub method: Method,
    pub headers: HeaderMap,
    pub body: Value,
    pub query: Value,
    pub ip: String,
    /// Which of the server's addresses the request came to.
    pub site: crate::foundation::config::Site,
    /// Assigned by the registered route, never by a request header.
    pub(super) owner: Option<&'static str>,
    pub trace: crate::foundation::observability::RequestTrace,
    // The registered pattern, whose `{name}` segments name the path parameters.
    pub(super) pattern: &'static str,
}
impl Input {
    pub fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }
    /// The path segment registered as `{name}`, exactly as the client sent it:
    /// never percent-decoded, and empty when the client sent an empty segment.
    pub fn param(&self, name: &str) -> &str {
        self.pattern
            .split('/')
            .zip(self.path.split('/'))
            .find(|(pattern, _)| {
                pattern
                    .strip_prefix('{')
                    .and_then(|p| p.strip_suffix('}'))
                    .is_some_and(|p| p == name)
            })
            .map_or("", |(_, segment)| segment)
    }
    pub fn session_token(&self, development: bool) -> &str {
        self.only_cookie("dispatch_session", development)
    }
    /// A support investigation's separate, read-only identity. Duplicate cookies are refused.
    pub fn support_read_token(&self, development: bool) -> &str {
        self.only_cookie("dispatch_support_read", development)
    }
    /// The value of the one cookie called `name`: empty when there is none, or more than one.
    fn only_cookie(&self, name: &'static str, development: bool) -> &str {
        let mut values = cookies(&self.headers, name, development);
        let first = values.next().unwrap_or("");
        if values.next().is_some() { "" } else { first }
    }
    /// Observation cookie errors never fall back to a human sign-in in the same browser.
    pub fn observation_token(
        &self,
        config: &crate::foundation::config::Config,
    ) -> Result<Option<&str>> {
        crate::accounts::browser::token(&self.headers, config)
    }
}

enum Payload {
    Value(Value),
    Encoded(Bytes),
    /// A document saved as a file, such as an agent's skill, with its content type and name.
    File(&'static str, &'static str, String),
    /// Where the browser goes instead: a URL the server built, never one a request named.
    Redirect(String),
    /// A file streamed as it comes, with its content type, the name it's saved as and, when
    /// known, its length.
    Download(String, String, Option<u64>, axum::body::Body),
    /// A picture the page shows, with its content type.
    Picture(String, Bytes),
}

pub struct Reply {
    value: Payload,
    status: u16,
    cookie: Option<String>,
    /// Whether the browser keeps it: see [`Reply::kept`].
    kept: bool,
}
impl Reply {
    fn new(value: Payload, status: u16) -> Self {
        Self {
            value,
            status,
            cookie: None,
            kept: false,
        }
    }
    pub fn json(value: Value) -> Self {
        Self::status(value, 200)
    }
    /// A typed response: the struct the route answers with, as the dashboard's contract has it.
    pub fn of<T: serde::Serialize>(value: &T) -> Result<Self> {
        Self::of_status(value, 200)
    }
    pub fn of_status<T: serde::Serialize>(value: &T, status: u16) -> Result<Self> {
        let encoded = Bytes::from(serde_json::to_vec(value)?);
        Ok(Self::new(Payload::Encoded(encoded), status))
    }
    pub fn ok() -> Self {
        Self::json(json!({"ok":true}))
    }
    pub fn status(value: Value, status: u16) -> Self {
        Self::new(Payload::Value(value), status)
    }
    /// JSON already serialized by a trusted response producer, never raw request input.
    pub fn encoded(value: Bytes) -> Self {
        Self::new(Payload::Encoded(value), 200)
    }
    /// A document made by the server, never request input, that a client saves as `name`.
    pub fn file(content_type: &'static str, name: &'static str, body: String) -> Self {
        Self::new(Payload::File(content_type, name, body), 200)
    }
    /// A file a client saves as `name`, streamed as `body` arrives: of `content_type`, and
    /// `length` bytes when that is known.
    pub fn download(
        content_type: &str,
        name: &str,
        length: Option<u64>,
        body: axum::body::Body,
    ) -> Self {
        let payload = Payload::Download(content_type.to_owned(), name.to_owned(), length, body);
        Self::new(payload, 200)
    }
    /// A picture of `content_type` for the page to show, which the browser keeps: its address
    /// names the version, so a new picture comes at a new one. Kept apart for each view of a
    /// DSP, since the view decides who may see it.
    pub fn picture(content_type: &str, body: Bytes) -> Self {
        Self::new(Payload::Picture(content_type.to_owned(), body), 200)
    }
    /// A `302 Found` to `location`, which the server built from what it validated.
    pub fn redirect(location: String) -> Self {
        Self::new(Payload::Redirect(location), 302)
    }
    /// `{"ok":true}` that also starts the browser's session.
    pub fn signed_in(raw: &str, development: bool, lifetime: SessionLifetime) -> Self {
        Self::ok().cookie(session_cookie(raw, lifetime.seconds(), development))
    }
    /// `{"ok":true}` that also ends the browser's session.
    pub fn signed_out(development: bool) -> Self {
        Self::ok().cookie(session_cookie("", 0, development))
    }
    pub fn cookie(mut self, cookie: String) -> Self {
        self.cookie = Some(cookie);
        self
    }
    /// Kept by the browser, as its address names the version of what it answers, so another
    /// version comes at another address. Kept apart for each view of a DSP, since the view
    /// decides who may see it, and a view is its session's alone.
    pub fn kept(mut self) -> Self {
        self.kept = true;
        self
    }
}
impl IntoResponse for Reply {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap();
        let mut response = match self.value {
            Payload::Value(value) => (status, Json(value)).into_response(),
            Payload::Encoded(value) => (
                status,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                value,
            )
                .into_response(),
            Payload::File(kind, name, body) => (
                status,
                [
                    (axum::http::header::CONTENT_TYPE, kind.to_owned()),
                    (
                        axum::http::header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{name}\""),
                    ),
                ],
                body,
            )
                .into_response(),
            Payload::Redirect(location) => {
                (status, [(axum::http::header::LOCATION, location)]).into_response()
            }
            Payload::Download(kind, name, length, body) => {
                let mut response = (
                    status,
                    [
                        (axum::http::header::CONTENT_TYPE, kind),
                        (axum::http::header::CONTENT_DISPOSITION, attachment(&name)),
                    ],
                    body,
                )
                    .into_response();
                if let Some(length) = length {
                    response
                        .headers_mut()
                        .insert(axum::http::header::CONTENT_LENGTH, length.into());
                }
                response
            }
            Payload::Picture(kind, body) => (
                status,
                [
                    (axum::http::header::CONTENT_TYPE, kind),
                    (
                        axum::http::header::CACHE_CONTROL,
                        "private, max-age=604800, immutable".to_owned(),
                    ),
                    (axum::http::header::VARY, "X-Dispatch-View".to_owned()),
                ],
                body,
            )
                .into_response(),
        };
        if let Some(cookie) = self.cookie
            && let Ok(value) = cookie.parse()
        {
            response.headers_mut().insert("set-cookie", value);
        }
        if self.kept {
            let headers = response.headers_mut();
            headers.insert(
                axum::http::header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static("private, max-age=31536000, immutable"),
            );
            headers.insert(
                axum::http::header::VARY,
                axum::http::HeaderValue::from_static("X-Dispatch-View"),
            );
        }
        response
    }
}

/// A cookie's name in the browser: outside development, `__Host-` keeps it to this address.
fn cookie_name(name: &str, development: bool) -> String {
    if development {
        name.to_owned()
    } else {
        format!("__Host-{name}")
    }
}
/// The value of each cookie called `name` that the request sent.
pub(super) fn cookies<'a>(
    headers: &'a HeaderMap,
    name: &'static str,
    development: bool,
) -> impl Iterator<Item = &'a str> {
    let prefix = format!("{}=", cookie_name(name, development));
    headers
        .get_all("cookie")
        .iter()
        .filter_map(|header| header.to_str().ok())
        .flat_map(|header| header.split(';'))
        .map(str::trim)
        .filter_map(move |part| part.strip_prefix(prefix.as_str()))
}
/// The browser's session cookie, holding `raw` for `seconds`; outside development it is
/// sent over HTTPS alone.
fn session_cookie(raw: &str, seconds: i64, development: bool) -> String {
    let name = cookie_name("dispatch_session", development);
    let secure = if development { "" } else { "; Secure" };
    format!("{name}={raw}; Path=/; HttpOnly; SameSite=Strict; Max-Age={seconds}{secure}")
}

/// `Content-Disposition` for saving a file as `name`: a plain ASCII name for any client, and
/// the name as it is for those that read `filename*`.
fn attachment(name: &str) -> String {
    let plain: String = name
        .chars()
        .map(|c| match c {
            ' '..='~' if !matches!(c, '"' | '\\' | '%') => c,
            _ => '_',
        })
        .collect();
    let exact: String = name
        .bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'-' | b'_' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("attachment; filename=\"{plain}\"; filename*=UTF-8''{exact}")
}

/// Reads a field only when it is present: `optional(b, "visible", v::boolean)?`.
pub fn optional<'a, T>(
    value: &'a Value,
    key: &str,
    read: impl FnOnce(&'a Value, &str) -> Result<T>,
) -> Result<Option<T>> {
    value.get(key).map(|_| read(value, key)).transpose()
}
pub fn optional_text<'a>(value: &'a Value, key: &str, max: usize) -> Result<&'a str> {
    Ok(optional(value, key, |q, key| v::text(q, key, 0, max))?.unwrap_or(""))
}
/// `direction=desc` in a query; ascending when absent.
pub fn descending(q: &Value) -> Result<bool> {
    let direction = optional(q, "direction", |q, key| v::choice(q, key, &["asc", "desc"]))?;
    Ok(direction == Some("desc"))
}
pub fn query_number(q: &Value, key: &str, default: usize, min: usize, max: usize) -> Result<usize> {
    let n = if let Some(v) = q.get(key) {
        v.as_str()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| Error::new("invalid_input", 400))?
    } else {
        default
    };
    ensure((min..=max).contains(&n), "invalid_input", 400)?;
    Ok(n)
}

#[cfg(test)]
#[path = "../../tests/backend/http/input.rs"]
mod tests;
