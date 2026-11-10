//! What Documents exposes to every other feature: somewhere to keep a DSP's files (the `Files`
//! contract, in its `contract/` crate), in its Google Drive, as Slack keeps the files posted in
//! a channel. A file saved through it is Documents' like any other, shown, opened and moved as
//! an upload is, with who it came from as who added it. A page is written as a Google Doc, and
//! over it as the page changes. Every call works only inside the DSP's own folder.
use super::{
    access,
    files::{self, make, put},
    google::FOLDER,
    grant::{self, Grant},
    storage::DocumentsStore,
    tree::{Mime, Row},
};
use dispatch_core::{
    Result, State, ensure, manifest::exposes::Shown, server::http::upload::UPLOAD_LIMIT,
};
use dispatch_documents_contract::{
    Answer, Author, Browsed, Files, Folder, Incoming, NewFolder, Page, Saved,
};
use std::sync::Arc;

/// How it refuses a folder that isn't one of the DSP's.
const FOLDER_NOT_FOUND: &str = "files_folder_not_found";
/// The largest page Google turns into a Doc, in bytes.
const PAGE_LIMIT: usize = 50 * 1024 * 1024;

/// Documents, as the feature keeping a DSP's files.
pub(crate) struct DocumentsFiles;
pub(crate) static FILES: DocumentsFiles = DocumentsFiles;

impl Shown for DocumentsFiles {
    fn name(&self) -> &'static str {
        "Documents"
    }
    fn permission(&self) -> &'static str {
        "documents.use"
    }
}

impl Files for DocumentsFiles {
    fn largest(&self) -> u64 {
        UPLOAD_LIMIT
    }
    fn place(&self) -> &'static str {
        "Google Drive"
    }
    fn refusal(&self, code: &str) -> Option<&'static str> {
        match code {
            "documents_storage_full" => Some("Google Drive is out of storage"),
            "google_unavailable" => Some("Google Drive isn't responding"),
            _ => None,
        }
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
            let owner = grant::capture(state, dsp, None).await?;
            let tree = access::tree_for(state, &owner).await?;
            let connection = owner.connection.clone();
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
            let answer = Browsed {
                path: std::iter::once(top)
                    .chain(tree.path(at).into_iter().map(folder))
                    .collect(),
                folders: tree
                    .children(at)
                    .into_iter()
                    .filter(|row| row.folder())
                    .map(folder)
                    .collect(),
            };
            access::current(state, &owner).await?;
            Ok(answer)
        })
    }
    fn folder<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
    ) -> Answer<'a, Option<Folder>> {
        Box::pin(async move {
            let owner = grant::capture(state, dsp, None).await?;
            let tree = access::tree_for(state, &owner).await?;
            let connection = owner.connection.clone();
            if id == tree.root() {
                access::current(state, &owner).await?;
                return Ok(Some(Folder {
                    id: id.to_owned(),
                    name: connection.folder_name,
                }));
            }
            access::current(state, &owner).await?;
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
        folder: NewFolder<'a>,
    ) -> Answer<'a, Folder> {
        Box::pin(async move {
            let owner = grant::capture(state, dsp, None).await?;
            let tree = access::tree_for(state, &owner).await?;
            let parent = folder.parent.unwrap_or(tree.root());
            ensure(tree.is_folder(parent), FOLDER_NOT_FOUND, 404)?;
            let made = make(state, &owner, parent, FOLDER, folder.name).await?;
            let logged = folder.logged_as.unwrap_or(&made.name);
            record(
                state,
                &owner,
                &made.id,
                folder.by,
                "documents.created",
                logged,
            )
            .await?;
            Ok(Folder {
                id: made.id,
                name: made.name,
            })
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
            let owner = grant::capture(state, dsp, None).await?;
            let tree = access::tree_for(state, &owner).await?;
            ensure(tree.is_folder(folder), FOLDER_NOT_FOUND, 404)?;
            let made = put(state, &owner, folder, &file.name, &file.mime, file.bytes).await?;
            let detail = format!("{} from {}", made.name, file.from);
            record(
                state,
                &owner,
                &made.id,
                &file.by,
                "documents.saved",
                &detail,
            )
            .await?;
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
            let owner = grant::capture(state, dsp, None).await?;
            let tree = access::tree_for(state, &owner).await?;
            ensure(tree.is_folder(folder), FOLDER_NOT_FOUND, 404)?;
            let made = files::write(state, &owner, folder, &page.name, &page.html).await?;
            let detail = format!("{} from {}", made.name, page.from);
            record(
                state,
                &owner,
                &made.id,
                &page.by,
                "documents.saved",
                &detail,
            )
            .await?;
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
            let owner = grant::capture(state, dsp, None).await?;
            let tree = access::tree_for(state, &owner).await?;
            if !tree.get(id).is_some_and(|row| row.mime == Mime::Doc) {
                return Ok(None);
            }
            let written = match files::rewrite(state, &owner, id, &page.name, &page.html).await {
                Err(error) if error.code == "documents_item_not_found" => return Ok(None),
                written => written?,
            };
            // Changed as a member changes a file in Drive: shown as theirs, not logged.
            let (changed, by) = (written.id.clone(), page.by);
            owner
                .run(state, move |db, owner| {
                    db.record_documents_change_by(&owner.key.dsp, &changed, &by)
                })
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
    owner: &Grant,
    id: &str,
    by: &Author,
    action: &'static str,
    detail: &str,
) -> Result<()> {
    let (id, by, detail) = (id.to_owned(), by.clone(), detail.to_owned());
    owner
        .run(state, move |db, owner| {
            db.record_documents_change_by(&owner.key.dsp, &id, &by)?;
            let actor = match &by {
                Author::Member(user) => Some(user.as_str()),
                Author::Named(_) => None,
            };
            db.audit(actor, Some(&owner.key.dsp), action, &detail)
        })
        .await
}

#[cfg(test)]
#[path = "../tests/backend/service.rs"]
mod tests;
