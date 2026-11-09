//! Documents item projections: file kinds, extensions, actor names and shown-file provenance.
use super::{
    cache, drive,
    google::FOLDER,
    grant::Key,
    storage::{DocumentsStore, Record},
    tree::{Row, Tree},
};
use crate::api::types::{DocumentsItem, ItemKind};
use dispatch_core::{Result, db::Store};
use dispatch_documents_contract::Author;
use std::collections::{BTreeMap, BTreeSet};

/// What Dispatch recorded of the files `ids`, with the names of whoever it names.
pub(super) fn known(
    db: &Store,
    dsp: &str,
    ids: &[String],
) -> Result<(BTreeMap<String, Record>, BTreeMap<String, String>)> {
    let records = db.documents_records(dsp, ids)?;
    let people: BTreeSet<&str> = records
        .values()
        .flat_map(|record| [&record.added_by, &record.changed_by])
        .filter_map(|by| match by {
            Author::Member(user) => Some(user.as_str()),
            Author::Named(_) => None,
        })
        .collect();
    let mut names = BTreeMap::new();
    for user in people {
        if let Some(name) = db.actor_name(dsp, user)? {
            names.insert(user.to_owned(), name);
        }
    }
    Ok((records, names))
}

/// Files as the page shows them.
pub(super) struct Described<'a> {
    pub(super) owner: &'a Key,
    pub(super) tree: &'a Tree,
    pub(super) records: &'a BTreeMap<String, Record>,
    pub(super) names: &'a BTreeMap<String, String>,
    /// The connected account, which Google names for every change Dispatch makes.
    pub(super) account: &'a str,
}
impl Described<'_> {
    pub(super) fn item(&self, row: &Row, located: bool) -> DocumentsItem {
        let record = self.records.get(&*row.id);
        // A member by their name now; someone else by the name they came with.
        let named = |by: &Author| match by {
            Author::Member(user) => self.names.get(user).cloned(),
            Author::Named(name) => Some(name.clone()),
        };
        // A shortcut shows the file it stands for: its name, and who changed it last.
        let shown = row.shown();
        let modifier = shown.by.as_deref();
        // Google names the account for what Dispatch did, and the person for what they did
        // in Google.
        let by_account = modifier
            .and_then(|person| person.email_address.as_deref())
            .is_none_or(|email| email.eq_ignore_ascii_case(self.account));
        let modified_by = if by_account {
            record.and_then(|record| named(&record.changed_by))
        } else {
            modifier.and_then(|person| person.display_name.clone())
        };
        DocumentsItem {
            id: row.id.to_string(),
            name: shown.name.to_string(),
            kind: kind(row.mime.as_str()),
            size: shown.size,
            modified_at: shown.modified.to_string(),
            modified_by,
            added_by: record.and_then(|record| named(&record.added_by)),
            items: row.folder().then(|| self.tree.count(&row.id) as u32),
            url: row.link.to_string(),
            location: row
                .parent
                .as_deref()
                .and_then(|parent| self.tree.get(parent))
                .filter(|_| located)
                .map(|parent| parent.name.to_string()),
            thumbnail: cache::picture(self.owner, shown),
            extension: extension(&shown.name, row.mime.as_str()).map(str::to_owned),
            shortcut: row.target.is_some(),
            unavailable: row.unavailable(),
        }
    }
}

/// The extension a file's name keeps, such as `.pdf`: the letters and digits after its last
/// dot, up to eight, with something before the dot. Folders and Google's own Docs, Sheets and
/// Slides have none.
pub(super) fn extension<'a>(name: &'a str, mime: &str) -> Option<&'a str> {
    if mime.starts_with("application/vnd.google-apps.") {
        return None;
    }
    let dot = name.rfind('.')?;
    let after = &name[dot + 1..];
    let named = dot > 0 && (1..=8).contains(&after.len());
    (named && after.bytes().all(|b| b.is_ascii_alphanumeric())).then(|| &name[dot..])
}

pub(crate) fn kind(mime: &str) -> ItemKind {
    match mime {
        FOLDER => ItemKind::Folder,
        drive::DOC => ItemKind::Doc,
        drive::SHEET => ItemKind::Sheet,
        drive::SLIDES => ItemKind::Slides,
        "application/pdf" => ItemKind::Pdf,
        image if image.starts_with("image/") => ItemKind::Image,
        _ => ItemKind::File,
    }
}
