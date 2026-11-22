//! Pictures Dispatch keeps a copy of from Slack, as a workspace's logo and a member's profile
//! picture: downloaded only from Slack's own servers, and kept only as a PNG, JPEG or GIF of
//! modest size.

/// How large a picture is kept at most.
pub(super) const MOST: usize = 256 * 1024;
/// What Slack's own servers are called, which pictures come from alone.
const HOSTS: &str = "slack-edge.com";
/// The kinds a picture may be, by how its bytes start.
const KINDS: &[(&[u8], &str)] = &[
    (b"\x89PNG\r\n\x1a\n", "image/png"),
    (b"\xff\xd8\xff", "image/jpeg"),
    (b"GIF87a", "image/gif"),
    (b"GIF89a", "image/gif"),
];

/// The address `url` names, if a picture may come from there: Slack's own servers, over HTTPS.
pub(super) fn address(url: &str) -> Option<url::Url> {
    let url = url::Url::parse(url).ok()?;
    let host = url.host_str()?;
    let slacks = host == HOSTS
        || host
            .strip_suffix(HOSTS)
            .is_some_and(|sub| sub.ends_with('.'));
    let plain = url.port().is_none() && url.username().is_empty() && url.password().is_none();
    (url.scheme() == "https" && slacks && plain).then_some(url)
}
/// The kind of picture `bytes` are, if one that's kept: a PNG, JPEG or GIF, and not too large.
pub(super) fn kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() > MOST {
        return None;
    }
    KINDS
        .iter()
        .find(|(start, _)| bytes.starts_with(start))
        .map(|(_, kind)| *kind)
}
