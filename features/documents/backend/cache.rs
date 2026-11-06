//! Tokens and shown-file thumbnail provenance, retained independently of Drive mirrors.
//! One deletion tombstone coordinates both caches; each retention check holds its cache lock.
use super::tree::Row;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

/// The DSPs deleted since the server started, of which nothing is kept in memory: what was
/// under way as one was deleted, a token's renewal or a folder's listing, would keep it once
/// it's forgotten otherwise. Each keeper asks while it holds what it keeps in.
static DELETED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);
fn deleted(dsp: &str) -> bool {
    DELETED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains(dsp)
}
/// Forgets what's kept in memory of the DSP's files once it's deleted, and keeps none after.
pub(super) fn forget_deleted(dsp: &str) {
    DELETED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(dsp.to_owned());
    forget(dsp);
    PICTURES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .retain(|(shown_by, _), _| shown_by != dsp);
}

/// The access tokens Google gave, by DSP, until a minute before each lapses.
static ACCESS: LazyLock<Mutex<BTreeMap<String, (String, Instant)>>> =
    LazyLock::new(Default::default);
pub(super) fn kept(dsp: &str) -> Option<String> {
    let tokens = ACCESS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    tokens
        .get(dsp)
        .filter(|(_, until)| Instant::now() < *until)
        .map(|(token, _)| token.clone())
}
/// Keeps the DSP's access token `token` until `until`, unless the DSP is deleted.
pub(super) fn keep(dsp: &str, token: &str, until: Instant) {
    let mut tokens = ACCESS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !deleted(dsp) {
        tokens.insert(dsp.to_owned(), (token.to_owned(), until));
    }
}
/// Forgets the DSP's access token, as a disconnect or a new account does.
pub(super) fn forget(dsp: &str) {
    ACCESS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(dsp);
}

/// Google's links to pictures of what each DSP's Documents showed, by DSP and file. A picture
/// is fetched only for a file kept here: so only one the DSP's own Documents showed, and never
/// by a link a browser sent.
static PICTURES: LazyLock<Mutex<HashMap<(String, String), Shown>>> =
    LazyLock::new(Default::default);
/// Google's link to a file's picture, when Google gave the link, and when Documents showed
/// the file.
#[derive(Clone)]
pub(super) struct Shown {
    pub(super) link: String,
    pub(super) given: Instant,
    shown: Instant,
}
/// How long a picture is fetched after Documents showed its file, and how long a link Google
/// gave is used, well inside the hours Google's last: an older one is asked for again.
pub(super) const PICTURE_LINK_LASTS: Duration = Duration::from_secs(60 * 60);
/// The most links kept across every DSP; past them, a card keeps its drawing.
const PICTURE_LINKS: usize = 20_000;
/// The picture of the file `id`, when the DSP's Documents showed it within the hour.
pub(super) fn shown(dsp: &str, id: &str) -> Option<Shown> {
    PICTURES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&(dsp.to_owned(), id.to_owned()))
        .filter(|shown| shown.shown.elapsed() < PICTURE_LINK_LASTS)
        .cloned()
}

/// Where the page fetches Google's picture of `row`, keeping its link for the DSP to
/// fetch it by: none when Google made none, or the DSP is deleted. The address names the
/// picture's version, so the browser keeps each until the file changes.
pub(super) fn picture(dsp: &str, row: &Row) -> Option<String> {
    let link = row.thumbnail.as_deref()?.to_owned();
    let mut links = PICTURES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if deleted(dsp) {
        return None;
    }
    let key = (dsp.to_owned(), row.id.to_string());
    if links.len() >= PICTURE_LINKS && !links.contains_key(&key) {
        links.retain(|_, shown| shown.shown.elapsed() < PICTURE_LINK_LASTS);
        if links.len() >= PICTURE_LINKS {
            return None;
        }
    }
    // Google's version tells its pictures apart; without one, when the file changed does.
    let version: String = match row.version {
        Some(version) => version.to_string(),
        None => row
            .modified
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect(),
    };
    links.insert(
        key,
        Shown {
            link,
            given: row.seen,
            shown: Instant::now(),
        },
    );
    Some(format!(
        "/api/dsp/documents/items/{}/thumbnail?v={version}",
        row.id
    ))
}

#[cfg(test)]
#[path = "../tests/backend/files.rs"]
mod tests;
