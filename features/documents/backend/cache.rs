//! Tokens and shown-file thumbnail provenance, retained independently of Drive mirrors.
//! One deletion tombstone coordinates both caches; each retention check holds its cache lock.
use super::{grant::Key, lock, tree::Row};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

/// The DSPs deleted since the server started, of which nothing is kept in memory: what was
/// under way as one was deleted, a token's renewal or a folder's listing, would keep it once
/// it's forgotten otherwise. Each keeper asks while it holds what it keeps in.
static DELETED: LazyLock<Mutex<HashSet<(std::path::PathBuf, String)>>> =
    LazyLock::new(Default::default);
fn deleted(key: &Key) -> bool {
    lock(&DELETED).contains(&(key.namespace.clone(), key.dsp.clone()))
}
/// Forgets what's kept in memory of the DSP's files once it's deleted, and keeps none after.
pub(super) fn forget_deleted(namespace: &std::path::Path, dsp: &str) {
    lock(&DELETED).insert((namespace.to_owned(), dsp.to_owned()));
    forget(namespace, dsp);
}

/// The access tokens Google gave, by DSP, until a minute before each lapses.
static ACCESS: LazyLock<Mutex<BTreeMap<Key, (String, Instant)>>> = LazyLock::new(Default::default);
pub(super) fn kept(key: &Key) -> Option<String> {
    let tokens = lock(&ACCESS);
    tokens
        .get(key)
        .filter(|(_, until)| Instant::now() < *until)
        .map(|(token, _)| token.clone())
}
/// Keeps the DSP's access token `token` until `until`, unless the DSP is deleted.
pub(super) fn keep(key: &Key, token: &str, until: Instant) {
    let mut tokens = lock(&ACCESS);
    if !deleted(key) {
        tokens.insert(key.clone(), (token.to_owned(), until));
    }
}
/// Forgets the DSP's access token, as a disconnect or a new account does.
pub(super) fn forget(namespace: &std::path::Path, dsp: &str) {
    lock(&PICTURES).retain(|(key, _), _| key.namespace != namespace || key.dsp != dsp);
    lock(&ACCESS).retain(|key, _| key.namespace != namespace || key.dsp != dsp);
}

/// Retires exactly the refused token; another renewal may have already replaced it.
pub(super) fn refused(key: &Key, token: &str) {
    let mut tokens = lock(&ACCESS);
    if tokens.get(key).is_some_and(|(current, _)| current == token) {
        tokens.remove(key);
    }
}
/// A refreshed metadata observation also renews the admitted link, under the same owner.
pub(super) fn observed(key: &Key, id: &str, link: &str, given: Instant) {
    if let Some(shown) = lock(&PICTURES).get_mut(&(key.clone(), id.to_owned())) {
        shown.link = link.to_owned();
        shown.given = given;
    }
}

/// Google's links to pictures of what each DSP's Documents showed, by DSP and file. A picture
/// is fetched only for a file kept here: so only one the DSP's own Documents showed, and never
/// by a link a browser sent.
static PICTURES: LazyLock<Mutex<HashMap<(Key, String), Shown>>> = LazyLock::new(Default::default);
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
pub(super) fn shown(key: &Key, id: &str) -> Option<Shown> {
    lock(&PICTURES)
        .get(&(key.clone(), id.to_owned()))
        .filter(|shown| shown.shown.elapsed() < PICTURE_LINK_LASTS)
        .cloned()
}

/// Where the page fetches Google's picture of `row`, keeping its link for the DSP to
/// fetch it by: none when Google made none, or the DSP is deleted. The address names the
/// picture's version, so the browser keeps each until the file changes.
pub(super) fn picture(owner: &Key, row: &Row) -> Option<String> {
    let link = row.thumbnail.as_deref()?.to_owned();
    let mut links = lock(&PICTURES);
    if deleted(owner) {
        return None;
    }
    let key = (owner.clone(), row.id.to_string());
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
