//! What Documents exposes to every other feature: somewhere to keep a DSP's files (the `Files`
//! contract, in its `contract/` crate), in its Google Drive, as Slack keeps the files posted in
//! a channel. A file saved through it is Documents' like any other, shown, opened and moved as
//! an upload is, with who it came from as who added it. A page is written as a Google Doc, and
//! over it as the page changes. Every call works only inside the DSP's own folder.
use super::{
    files::{self, make, put},
    google::FOLDER,
    storage::DocumentsStore,
    tree::{Mime, Row},
};
use dispatch_core::{Result, State, ensure, server::http::upload::UPLOAD_LIMIT};
use dispatch_documents_contract::{Answer, Author, Browsed, Files, Folder, Incoming, Page, Saved};
use std::sync::Arc;

/// How it refuses a folder that isn't one of the DSP's.
const FOLDER_NOT_FOUND: &str = "files_folder_not_found";
/// The largest page Google turns into a Doc, in bytes.
const PAGE_LIMIT: usize = 50 * 1024 * 1024;

/// Documents, as the feature keeping a DSP's files.
pub(crate) struct DocumentsFiles;
pub(crate) static FILES: DocumentsFiles = DocumentsFiles;

impl Files for DocumentsFiles {
    fn name(&self) -> &'static str {
        "Documents"
    }
    fn permission(&self) -> &'static str {
        "documents.use"
    }
    fn largest(&self) -> u64 {
        UPLOAD_LIMIT
    }
    fn unready<'a>(&'a self, state: &'a Arc<State>, dsp: &'a str) -> Answer<'a, Option<String>> {
        Box::pin(async move {
            let at = dsp.to_owned();
            let connection = state.read(move |db| db.documents_connection(&at)).await?;
            Ok(match connection {
                None => Some("Google Drive isn't connected".to_owned()),
                Some(connection) if connection.broken => {
                    Some("Google Drive needs reconnecting".to_owned())
                }
                Some(_) => None,
            })
        })
    }
    fn browse<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: Option<&'a str>,
    ) -> Answer<'a, Browsed> {
        Box::pin(async move {
            let (connection, tree) = files::tree(state, dsp).await?;
            let at = id.unwrap_or(tree.root());
            ensure(tree.is_folder(at), FOLDER_NOT_FOUND, 404)?;
            let folder = |row: &Row| Folder {
                id: row.id.to_string(),
                name: row.name.to_string(),
            };
            let top = Folder {
                id: tree.root().to_owned(),
                name: connection.folder_name,
            };
            Ok(Browsed {
                path: std::iter::once(top)
                    .chain(tree.path(at).into_iter().map(folder))
                    .collect(),
                folders: tree
                    .children(at)
                    .into_iter()
                    .filter(|row| row.folder())
                    .map(folder)
                    .collect(),
            })
        })
    }
    fn folder<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
    ) -> Answer<'a, Option<Folder>> {
        Box::pin(async move {
            let (connection, tree) = files::tree(state, dsp).await?;
            if id == tree.root() {
                return Ok(Some(Folder {
                    id: id.to_owned(),
                    name: connection.folder_name,
                }));
            }
            Ok(tree
                .get(id)
                .filter(|_| tree.is_folder(id))
                .map(|row| Folder {
                    id: id.to_owned(),
                    name: row.name.to_string(),
                }))
        })
    }
    fn create_folder<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        name: &'a str,
        by: &'a Author,
    ) -> Answer<'a, Folder> {
        Box::pin(async move {
            let (_, tree) = files::tree(state, dsp).await?;
            let (_, made) = make(state, dsp, tree.root(), FOLDER, name).await?;
            let folder = Folder {
                id: made.id,
                name: made.name,
            };
            record(
                state,
                dsp,
                &folder.id,
                by,
                "documents.created",
                &folder.name,
            )
            .await?;
            Ok(folder)
        })
    }
    fn save<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        folder: &'a str,
        file: Incoming,
    ) -> Answer<'a, Saved> {
        Box::pin(async move {
            ensure(file.bytes.length <= self.largest(), "upload_too_large", 413)?;
            let (_, tree) = files::tree(state, dsp).await?;
            ensure(tree.is_folder(folder), FOLDER_NOT_FOUND, 404)?;
            let (_, made) = put(state, dsp, folder, &file.name, &file.mime, file.bytes).await?;
            let detail = format!("{} from {}", made.name, file.from);
            record(state, dsp, &made.id, &file.by, "documents.saved", &detail).await?;
            Ok(Saved {
                id: made.id,
                name: made.name,
                link: made.web_view_link,
            })
        })
    }
    fn write<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        folder: &'a str,
        page: Page,
    ) -> Answer<'a, Saved> {
        Box::pin(async move {
            ensure(page.html.len() <= PAGE_LIMIT, "upload_too_large", 413)?;
            let (_, tree) = files::tree(state, dsp).await?;
            ensure(tree.is_folder(folder), FOLDER_NOT_FOUND, 404)?;
            let made = files::write(state, dsp, folder, &page.name, &page.html).await?;
            let detail = format!("{} from {}", made.name, page.from);
            record(state, dsp, &made.id, &page.by, "documents.saved", &detail).await?;
            Ok(Saved {
                id: made.id,
                name: made.name,
                link: made.web_view_link,
            })
        })
    }
    fn rewrite<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
        page: Page,
    ) -> Answer<'a, Option<Saved>> {
        Box::pin(async move {
            ensure(page.html.len() <= PAGE_LIMIT, "upload_too_large", 413)?;
            let (_, tree) = files::tree(state, dsp).await?;
            if !tree.get(id).is_some_and(|row| row.mime == Mime::Doc) {
                return Ok(None);
            }
            let written = match files::rewrite(state, dsp, id, &page.name, &page.html).await {
                Err(error) if error.code == "documents_item_not_found" => return Ok(None),
                written => written?,
            };
            // Changed as a member changes a file in Drive: shown as theirs, not logged.
            let (at, by) = (dsp.to_owned(), page.by);
            let changed = written.id.clone();
            state
                .run(move |db| db.record_documents_change_by(&at, &changed, &by))
                .await?;
            Ok(Some(Saved {
                id: written.id,
                name: written.name,
                link: written.web_view_link,
            }))
        })
    }
}

/// Records who `by` made or saved the file `id`, and logs it as `action`: as the member, or
/// with no member as its actor for someone known only by name.
async fn record(
    state: &Arc<State>,
    dsp: &str,
    id: &str,
    by: &Author,
    action: &'static str,
    detail: &str,
) -> Result<()> {
    let (at, id, by, detail) = (dsp.to_owned(), id.to_owned(), by.clone(), detail.to_owned());
    state
        .run(move |db| {
            db.record_documents_change_by(&at, &id, &by)?;
            let actor = match &by {
                Author::Member(user) => Some(user.as_str()),
                Author::Named(_) => None,
            };
            db.audit(actor, Some(&at), action, &detail)
        })
        .await
}

#[cfg(test)]
#[path = "../tests/backend/service.rs"]
mod tests;
