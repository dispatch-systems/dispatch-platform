//! Compact query indices retained by core's bounded read cache. No descriptions, provenance,
//! picture admission or Tree handles are cached. Continuations only look up admitted indices.
use super::{
    description::{Described, kind, known},
    google::folder_url,
    tree::Tree,
};
use crate::api::types::{
    DocumentsCounts, DocumentsFilter, DocumentsItem, DocumentsManifest, DocumentsManifestEntry,
    DocumentsQueryFolder, DocumentsQueryScope, FolderStep, ItemKind,
};
use dispatch_core::{
    Result,
    db::Store,
    foundation::crypto,
    server::cache::{ReadCache, Scope},
};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

pub(super) const CACHED: &str = "documents-query";
static PROCESS: OnceLock<String> = OnceLock::new();

pub(super) fn process() -> Result<&'static str> {
    if let Some(process) = PROCESS.get() {
        return Ok(process);
    }
    let random = crypto::hex(&crypto::random::<32>()?);
    Ok(PROCESS.get_or_init(|| random))
}
pub(super) fn digest(value: &impl Serialize) -> Result<String> {
    Ok(crypto::sha(serde_json::to_vec(value)?))
}
pub(super) fn normalized(mut scope: DocumentsQueryScope) -> DocumentsQueryScope {
    scope.q = scope
        .q
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(str::to_owned);
    scope
}
fn bucket(kind: ItemKind) -> usize {
    match kind {
        ItemKind::Folder => 1,
        ItemKind::Doc => 2,
        ItemKind::Sheet => 3,
        ItemKind::Slides => 4,
        ItemKind::Pdf | ItemKind::Image | ItemKind::File => 5,
    }
}
fn selected(filter: DocumentsFilter) -> usize {
    match filter {
        DocumentsFilter::All => 0,
        DocumentsFilter::Folders => 1,
        DocumentsFilter::Docs => 2,
        DocumentsFilter::Sheets => 3,
        DocumentsFilter::Slides => 4,
        DocumentsFilter::Uploads => 5,
    }
}
#[derive(Serialize)]
pub(super) struct Index {
    complete: bool,
    pub(super) manifest: DocumentsManifest,
    pub(super) manifest_revision: String,
    /// Original manifest ordinals, separately indexed by filter. Never scan N for a page.
    orders: [Vec<u32>; 6],
    members: HashMap<String, u32>,
}
impl Index {
    fn build(tree: &Tree, at: &str, q: Option<&str>, key: &str) -> Result<Self> {
        let mut manifest = DocumentsManifest {
            entries: Vec::new(),
        };
        let mut orders: [Vec<u32>; 6] = Default::default();
        let mut members = HashMap::new();
        let mut add = |row: &super::tree::Row| {
            // Descriptions resolve IDs through Tree::get. A raw duplicate or a search
            // result that is not that canonical row needs the complete legacy reader.
            if !tree
                .get(&row.id)
                .is_some_and(|canonical| std::ptr::eq(canonical, row))
                || members.contains_key(row.id.as_ref())
            {
                return false;
            }
            let ordinal = manifest.entries.len() as u32;
            let kind = kind(row.mime.as_str());
            orders[0].push(ordinal);
            orders[bucket(kind)].push(ordinal);
            members.insert(row.id.to_string(), ordinal);
            manifest.entries.push(DocumentsManifestEntry {
                id: row.id.to_string(),
                kind,
                name: row.shown().name.to_string(),
                modified_at: row.shown().modified.to_string(),
            });
            true
        };
        let complete = match q {
            Some(words) => tree.search(at, words).into_iter().all(&mut add),
            None => tree.ordered_children(at).all(&mut add),
        };
        if !complete {
            // Cache only the small refusal, not an unusable partial universe.
            return Ok(Self {
                complete: false,
                manifest: DocumentsManifest {
                    entries: Vec::new(),
                },
                manifest_revision: String::new(),
                orders: Default::default(),
                members: HashMap::new(),
            });
        }
        let manifest_revision = digest(&("documents-manifest-v1", key, &manifest))?;
        Ok(Self {
            complete: true,
            manifest,
            manifest_revision,
            orders,
            members,
        })
    }
    pub(super) fn counts(&self) -> DocumentsCounts {
        DocumentsCounts {
            all: self.orders[0].len() as u32,
            folders: self.orders[1].len() as u32,
            docs: self.orders[2].len() as u32,
            sheets: self.orders[3].len() as u32,
            slides: self.orders[4].len() as u32,
            uploads: self.orders[5].len() as u32,
        }
    }
    pub(super) fn page(&self, filter: DocumentsFilter, page: u32, size: u32) -> (u32, Vec<String>) {
        let order = &self.orders[selected(filter)];
        let last = order.len().saturating_sub(1) / size as usize;
        let page = (page as usize).min(last);
        let start = page * size as usize;
        let end = (start + size as usize).min(order.len());
        let ids = order[start..end]
            .iter()
            .map(|ordinal| self.manifest.entries[*ordinal as usize].id.clone())
            .collect();
        (page as u32, ids)
    }
    pub(super) fn contains(&self, filter: DocumentsFilter, id: &str) -> bool {
        self.members.get(id).is_some_and(|ordinal| {
            selected(filter) == 0
                || selected(filter) == bucket(self.manifest.entries[*ordinal as usize].kind)
        })
    }
    pub(super) fn total(&self, filter: DocumentsFilter) -> u32 {
        self.orders[selected(filter)].len() as u32
    }
}

/// Opening may build; continuing never does. An unadmitted opening also resets, allowing a
/// future client to fall back once to the complete legacy reader under memory pressure.
pub(super) fn index(
    cache: &ReadCache,
    scope: Scope,
    key: String,
    global: u64,
    tree: &Tree,
    query: (&str, Option<&str>),
    opening: bool,
) -> Result<Option<Arc<Index>>> {
    let (at, q) = query;
    if opening {
        cache.shared(scope.clone(), key.clone(), global, || {
            Index::build(tree, at, q, &key)
        })?;
    }
    Ok(cache
        .lookup_shared::<Index>(&scope, &key, global)
        .filter(|index| index.complete))
}
/// Only requested IDs reach the authoritative provenance query and thumbnail admission.
/// The caller has revalidated current authorization and connection after every await.
pub(super) fn describe(
    db: &Store,
    dsp: &str,
    account: &str,
    tree: &Tree,
    ids: &[String],
    located: bool,
) -> Result<Vec<DocumentsItem>> {
    let (records, names) = known(db, dsp, ids)?;
    let described = Described {
        dsp,
        tree,
        records: &records,
        names: &names,
        account,
    };
    ids.iter()
        .map(|id| {
            tree.get(id)
                .map(|row| described.item(row, located))
                .ok_or_else(|| dispatch_core::Error::new("documents_item_not_found", 404))
        })
        .collect()
}
pub(super) fn header(
    tree: &Tree,
    at: &str,
    scope: &DocumentsQueryScope,
    name: &str,
) -> DocumentsQueryFolder {
    let path: Vec<FolderStep> = tree
        .path(at)
        .into_iter()
        .map(|row| FolderStep {
            id: row.id.to_string(),
            name: row.name.to_string(),
        })
        .collect();
    DocumentsQueryFolder {
        id: scope.id.clone(),
        name: path
            .last()
            .map_or_else(|| name.to_owned(), |row| row.name.clone()),
        url: folder_url(at),
        path,
        items: tree.count(at) as u32,
    }
}

#[cfg(test)]
#[path = "../tests/backend/query.rs"]
mod tests;
