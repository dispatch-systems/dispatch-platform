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
pub struct Input {
    pub path: String,
    pub method: Method,
    pub headers: HeaderMap,
    pub body: Value,
    pub query: Value,
    pub ip: String,
    /// Which of the server's addresses the request came to.
    pub site: crate::foundation::config::Site,
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
        let prefix = if development {
            "dispatch_session="
        } else {
            "__Host-dispatch_session="
        };
        let mut values = self
            .headers
            .get_all("cookie")
            .iter()
            .filter_map(|header| header.to_str().ok())
            .flat_map(|header| header.split(';'))
            .map(str::trim)
            .filter_map(|part| part.strip_prefix(prefix));
        let first = values.next().unwrap_or("");
        if values.next().is_some() { "" } else { first }
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
}
impl Reply {
    pub fn json(value: Value) -> Self {
        Self::status(value, 200)
    }
    /// A typed response: the struct the route answers with, as the dashboard's contract has it.
    pub fn of<T: serde::Serialize>(value: &T) -> Result<Self> {
        Self::of_status(value, 200)
    }
    pub fn of_status<T: serde::Serialize>(value: &T, status: u16) -> Result<Self> {
        Ok(Self {
            value: Payload::Encoded(Bytes::from(serde_json::to_vec(value)?)),
            status,
            cookie: None,
        })
    }
    pub fn ok() -> Self {
        Self::json(json!({"ok":true}))
    }
    pub fn status(value: Value, status: u16) -> Self {
        Self {
            value: Payload::Value(value),
            status,
            cookie: None,
        }
    }
    /// JSON already serialized by a trusted response producer, never raw request input.
    pub fn encoded(value: Bytes) -> Self {
        Self {
            value: Payload::Encoded(value),
            status: 200,
            cookie: None,
        }
    }
    /// A document made by the server, never request input, that a client saves as `name`.
    pub fn file(content_type: &'static str, name: &'static str, body: String) -> Self {
        Self {
            value: Payload::File(content_type, name, body),
            status: 200,
            cookie: None,
        }
    }
    /// A file a client saves as `name`, streamed as `body` arrives: of `content_type`, and
    /// `length` bytes when that is known.
    pub fn download(
        content_type: &str,
        name: &str,
        length: Option<u64>,
        body: axum::body::Body,
    ) -> Self {
        Self {
            value: Payload::Download(content_type.to_owned(), name.to_owned(), length, body),
            status: 200,
            cookie: None,
        }
    }
    /// A picture of `content_type` for the page to show, which the browser keeps: its address
    /// names the version, so a new picture comes at a new one. Kept apart for each view of a
    /// DSP, since the view decides who may see it.
    pub fn picture(content_type: &str, body: Bytes) -> Self {
        Self {
            value: Payload::Picture(content_type.to_owned(), body),
            status: 200,
            cookie: None,
        }
    }
    /// A `302 Found` to `location`, which the server built from what it validated.
    pub fn redirect(location: String) -> Self {
        Self {
            value: Payload::Redirect(location),
            status: 302,
            cookie: None,
        }
    }
    /// `{"ok":true}` that also starts the browser's session.
    pub fn signed_in(raw: &str, development: bool, lifetime: SessionLifetime) -> Self {
        let secure = if development { "" } else { "; Secure" };
        let name = if development {
            "dispatch_session"
        } else {
            "__Host-dispatch_session"
        };
        Self::ok().cookie(format!(
            "{name}={raw}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}{secure}",
            lifetime.seconds()
        ))
    }
    /// `{"ok":true}` that also ends the browser's session.
    pub fn signed_out(development: bool) -> Self {
        let (name, secure) = if development {
            ("dispatch_session", "")
        } else {
            ("__Host-dispatch_session", "; Secure")
        };
        Self::ok().cookie(format!(
            "{name}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{secure}"
        ))
    }
    pub fn cookie(mut self, cookie: String) -> Self {
        self.cookie = Some(cookie);
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
        response
    }
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
