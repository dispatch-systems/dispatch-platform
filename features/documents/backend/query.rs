//! Compact query indices retained by core's bounded read cache. No descriptions, provenance,
//! picture admission or Tree handles are cached. Continuations only look up admitted indices.
use super::{
    access,
    description::{Described, kind, known},
    files::not_found,
    google::folder_url,
    grant, mirror,
    tree::Tree,
};
use crate::api::types::{
    DocumentsCounts, DocumentsDescriptionRead, DocumentsFilter, DocumentsItem, DocumentsManifest,
    DocumentsManifestEntry, DocumentsQueryFolder, DocumentsQueryPage, DocumentsQueryRead,
    DocumentsQueryScope, DocumentsResetReason, FolderStep, ItemKind,
};
use dispatch_core::{
    Result, State,
    accounts::Context,
    db::Store,
    ensure,
    foundation::crypto,
    server::{
        cache::{ReadCache, Scope},
        http::route::Dsp,
    },
};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, atomic::Ordering},
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
pub(super) fn items(
    db: &Store,
    owner: &super::grant::Key,
    account: &str,
    tree: &Tree,
    ids: &[String],
    located: bool,
) -> Result<Vec<DocumentsItem>> {
    let (records, names) = known(db, &owner.dsp, ids)?;
    let described = Described {
        owner,
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

/// Shared preparation for the ephemeral query protocol, its pages and its descriptions.
/// `files::folder` and `files::folder_read` read through access::tree_for and the complete
/// reader. Captured grant ownership is independent of semantic snapshots.
pub(crate) struct QueryRequest {
    pub scope: DocumentsQueryScope,
    pub page: u32,
    pub size: u32,
    pub snapshot: Option<String>,
    pub manifest_since: Option<String>,
    pub since: Option<String>,
}
async fn query_read(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    request: QueryRequest,
    descriptions: Option<Vec<String>>,
) -> Result<QueryAnswer> {
    let QueryRequest {
        scope,
        snapshot,
        page,
        size,
        manifest_since,
        since,
    } = request;
    let scope = normalized(scope);
    ensure((1..=100).contains(&size), "invalid_input", 400)?;
    if let Some(ids) = &descriptions {
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        ensure(
            !ids.is_empty() && ids.len() <= 100 && unique.len() == ids.len(),
            "invalid_input",
            400,
        )?;
    }
    let captured = grant::capture_member(state, c.clone(), access, None).await?;
    access::tree_for(state, &captured).await?;
    let owner = Arc::clone(&captured.mirror);
    let connection = captured.connection.clone();
    // Serialize openings before database admission; a cancelled caller's worker keeps its
    // turn until the calculation ends. Continuations have no builder and need no keyed turn.
    let turn = if snapshot.is_none() {
        Some(
            state
                .read_turns
                .enter(digest(&(
                    "documents-query-turn-v1",
                    &c.dsp.id,
                    owner.incarnation(),
                    &connection.folder_id,
                    &scope.id,
                    &scope.q,
                ))?)
                .await?,
        )
    } else {
        None
    };
    let shared = Arc::clone(state);
    state
        .read(move |db| {
            let _turn = turn;
            captured.check(db)?;
            let reset = |reason| match descriptions.as_ref() {
                Some(_) => QueryAnswer::Descriptions(DocumentsDescriptionRead::Reset { reason }),
                None => QueryAnswer::Page(DocumentsQueryRead::Reset { reason }),
            };
            let Some((tree, semantic)) = owner.snapshot(&connection.folder_id) else {
                return Ok(reset(DocumentsResetReason::Changed));
            };
            if !mirror::current(&captured.key, &owner) || !owner.matches(tree.root(), semantic) {
                return Ok(reset(DocumentsResetReason::Changed));
            }
            let at = scope.id.as_deref().unwrap_or(tree.root());
            if !tree.is_folder(at) {
                if snapshot.is_some() {
                    return Ok(reset(DocumentsResetReason::Changed));
                }
                return Err(not_found());
            }
            // Persisted grant identity, mirror incarnation, and semantic sequence have separate
            // lifetimes. Process identity still prevents ephemeral continuations across restart.
            let key = digest(&(
                "documents-query-index-v1",
                process()?,
                shared.config.environment_root(),
                &c.dsp.id,
                owner.incarnation(),
                semantic,
                &connection.folder_id,
                (
                    &connection.generation,
                    connection.broken,
                    connection.full,
                    &connection.account.email,
                    connection.account.workspace,
                    &connection.folder_name,
                    &connection.connected_by,
                    &connection.connected_at,
                ),
                &scope.id,
                &scope.q,
            ))?;
            let expected = digest(&("documents-query-snapshot-v1", &key, scope.filter))?;
            if snapshot
                .as_ref()
                .is_some_and(|snapshot| *snapshot != expected)
            {
                return Ok(reset(DocumentsResetReason::Changed));
            }
            // Global revision, never the mirror sequence: core prunes unequal revisions globally.
            let global = shared.data_revision.load(Ordering::Relaxed);
            let Some(index) = index(
                &shared.read_cache,
                Scope::tenant(CACHED, &c.dsp.id),
                key,
                global,
                &tree,
                (at, scope.q.as_deref()),
                snapshot.is_none(),
            )?
            else {
                return Ok(reset(DocumentsResetReason::Missing));
            };
            let answer = if let Some(ids) = &descriptions {
                ensure(
                    ids.iter().all(|id| index.contains(scope.filter, id)),
                    "documents_item_not_found",
                    404,
                )?;
                QueryAnswer::Descriptions(DocumentsDescriptionRead::Full {
                    snapshot: expected,
                    items: items(
                        db,
                        &captured.key,
                        &connection.account.email,
                        &tree,
                        ids,
                        scope.q.is_some(),
                    )?,
                })
            } else {
                let (page, ids) = index.page(scope.filter, page, size);
                let reading = DocumentsQueryPage {
                    grant_generation: connection.generation.clone(),
                    snapshot: expected,
                    manifest_revision: index.manifest_revision.clone(),
                    page,
                    page_size: size,
                    total: index.total(scope.filter),
                    unfiltered_total: index.counts().all,
                    counts: index.counts(),
                    folder: header(&tree, at, &scope, &connection.folder_name),
                    items: items(
                        db,
                        &captured.key,
                        &connection.account.email,
                        &tree,
                        &ids,
                        scope.q.is_some(),
                    )?,
                };
                let content_revision = digest(&("documents-query-page-v1", &reading))?;
                let same_manifest =
                    manifest_since.as_deref() == Some(index.manifest_revision.as_str());
                QueryAnswer::Page(
                    if same_manifest && since.as_deref() == Some(content_revision.as_str()) {
                        DocumentsQueryRead::Unchanged {
                            snapshot: reading.snapshot,
                            manifest_revision: reading.manifest_revision,
                            content_revision,
                        }
                    } else {
                        DocumentsQueryRead::Full {
                            reading: Box::new(reading),
                            manifest: (!same_manifest).then(|| index.manifest.clone()),
                            content_revision,
                        }
                    },
                )
            };
            // Synchronous publications may run while this worker projects. No awaits follow this
            // check, and no mixed old manifest/current tree answer is allowed out.
            if !mirror::current(&captured.key, &owner) || !owner.matches(tree.root(), semantic) {
                return Ok(reset(DocumentsResetReason::Changed));
            }
            Ok(answer)
        })
        .await
}

enum QueryAnswer {
    Page(DocumentsQueryRead),
    Descriptions(DocumentsDescriptionRead),
}
pub async fn query(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    request: QueryRequest,
) -> Result<DocumentsQueryRead> {
    match query_read(state, c, access, request, None).await? {
        QueryAnswer::Page(answer) => Ok(answer),
        QueryAnswer::Descriptions(_) => unreachable!("page request"),
    }
}
pub async fn describe(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    scope: DocumentsQueryScope,
    snapshot: String,
    ids: Vec<String>,
) -> Result<DocumentsDescriptionRead> {
    let request = QueryRequest {
        scope,
        snapshot: Some(snapshot),
        page: 0,
        size: 100,
        manifest_since: None,
        since: None,
    };
    match query_read(state, c, access, request, Some(ids)).await? {
        QueryAnswer::Descriptions(answer) => Ok(answer),
        QueryAnswer::Page(_) => unreachable!("description request"),
    }
}

#[cfg(test)]
#[path = "../tests/backend/query.rs"]
mod tests;
