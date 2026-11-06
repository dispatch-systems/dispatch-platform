//! Redirect matching, native-app schemes, registration policy, and destination wording.
const LONGEST_REDIRECT: usize = 512;
/// Native apps known by a scheme without a dot, which RFC 8252 §7.1 would otherwise ask for.
const NATIVE: &[&str] = &[
    "cursor",
    "vscode",
    "vscode-insiders",
    "vscodium",
    "windsurf",
];
/// Schemes that are no app's own: the web's, and those a browser would run or read itself.
const NOT_PRIVATE: &[&str] = &[
    "http",
    "https",
    "javascript",
    "data",
    "file",
    "vbscript",
    "about",
    "blob",
    "ftp",
    "ws",
    "wss",
    "mailto",
];
const LOOPBACK: &[&str] = &["localhost", "127.0.0.1", "[::1]"];

/// Whether `requested` is one of the app's redirects: the same text, or for a loopback `http`
/// redirect the same but for the port, which a native app picks when it starts listening.
pub fn allowed(registered: &[String], requested: &str) -> bool {
    registered.iter().any(|uri| {
        uri == requested || loopback(uri).is_some_and(|parts| Some(parts) == loopback(requested))
    })
}

/// A loopback `http` redirect's host and what follows its port, or None for any other.
pub(super) fn loopback(uri: &str) -> Option<(&'static str, &str)> {
    let rest = uri.strip_prefix("http://")?;
    let (authority, path) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    let host = LOOPBACK.iter().find(|host| {
        authority.strip_prefix(**host).is_some_and(|port| {
            port.is_empty()
                || port.strip_prefix(':').is_some_and(|port| {
                    port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok()
                })
        })
    })?;
    Some((host, path))
}

/// The private-use scheme (RFC 8252 §7.1) a redirect opens a native app by, or None for any
/// other. Such a scheme is reverse-domain, as `com.example.app`, or a known native app's, as
/// `cursor`. A `web+` scheme is a website's, however a browser hands it on.
pub fn private_scheme(uri: &str) -> Option<&str> {
    let (scheme, rest) = uri.split_once(':')?;
    let valid = scheme
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_lowercase())
        && scheme
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+.-".contains(&b));
    let native = scheme.contains('.') || NATIVE.contains(&scheme);
    (valid
        && native
        && !rest.is_empty()
        && !scheme.starts_with("web+")
        && !NOT_PRIVATE.contains(&scheme))
    .then_some(scheme)
}

/// Whether an app that registered itself may be sent back to `uri`: a loopback `http`
/// address, `http://localhost[:port]/…` and the like, or the app's own private-use scheme.
/// Either opens only an app on the owner's own computer.
pub(super) fn registrable(uri: &str) -> bool {
    let plain = uri.len() <= LONGEST_REDIRECT
        && uri.is_ascii()
        && !uri.contains('#')
        && !uri.bytes().any(|b| b.is_ascii_control() || b == b' ')
        && url::Url::parse(uri).is_ok();
    plain
        && match loopback(uri) {
            Some((_, path)) => path.starts_with('/'),
            None => private_scheme(uri).is_some(),
        }
}

/// Whether an app that registered itself may be sent back to `uri` as a website: an `https`
/// address written exactly as the URL standard writes it, naming its host by a dotted name,
/// with no credentials or fragment. Only while the owner lets websites and other apps connect.
pub(super) fn registrable_website(uri: &str) -> bool {
    uri.len() <= LONGEST_REDIRECT
        && url::Url::parse(uri).is_ok_and(|url| {
            url.scheme() == "https"
                && url.as_str() == uri
                && matches!(url.host(), Some(url::Host::Domain(host)) if host.contains('.'))
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
        })
}

/// Where a redirect sends the owner, as the approval page says it: "this computer" for a
/// loopback address or an app's own scheme, with that scheme, and otherwise the host.
pub fn destination(uri: &str) -> (String, Option<String>) {
    if loopback(uri).is_some() {
        return ("this computer".into(), None);
    }
    if let Some(scheme) = private_scheme(uri) {
        return ("this computer".into(), Some(scheme.to_owned()));
    }
    let host = url::Url::parse(uri)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default();
    (host, None)
}

#[cfg(test)]
#[path = "../../../tests/backend/oauth/clients/redirects.rs"]
mod tests;
