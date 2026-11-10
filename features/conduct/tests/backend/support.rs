//! What every Conduct backend test installs and starts from. A test binary holds one registry,
//! so each installs the same: Conduct, and a stand-in for where a DSP keeps its files, which
//! keeps its folders and what's saved in them in memory, and can be made unready, or to refuse
//! what's saved. And how a test starts: a platform with one DSP and its owner writing up.
use crate::FEATURE;
use dispatch_core::{
    Error, State,
    accounts::{Auth, Context},
    db::{Store, s},
    manifest::{Feature, exposes::Shown, feature, optional},
    server::{http::upload::UPLOAD_LIMIT, operations},
    testing,
};
use dispatch_documents_contract::{Answer, Author, Browsed, Files, Folder, Incoming, Page, Saved};
use futures_util::TryStreamExt;
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

/// A preview platform's first DSP, every feature on, and its owner writing up.
pub fn ready() -> (tempfile::TempDir, Store, Context) {
    testing::install(&[], &[&FEATURE, &KEEPER]);
    let (root, store) = testing::store();
    let bootstrap = operations::bootstrap(
        &store,
        "conduct@example.test",
        "Casey",
        "Supervisor",
        "test-password-long",
    )
    .unwrap();
    let auth = Auth {
        user: serde_json::from_value(bootstrap["owner"].clone()).unwrap(),
        hash: String::new(),
        csrf: String::new(),
        raw: String::new(),
        preview: None,
        site: dispatch_core::foundation::config::Site::Admin,
        scope: None,
    };
    let c = store
        .context(&auth, s(&bootstrap["dsp"], "id"), "conduct.manage")
        .unwrap();
    (root, store, c)
}
/// The same, with its server's state in the store's place.
pub fn platform() -> (tempfile::TempDir, Arc<State>, Context) {
    let (root, store, c) = ready();
    let config = store.config.clone();
    drop(store);
    (root, State::new(config).unwrap(), c)
}

/// A folder the stand-in keeps: its DSP, its ID, the folder it's in, and its name.
struct KeptFolder {
    dsp: String,
    id: String,
    parent: Option<String>,
    name: String,
}
/// A file the stand-in keeps: its DSP, the folder it's in, its name, how many bytes it holds,
/// and who it came from.
struct KeptFile {
    dsp: String,
    folder: String,
    name: String,
    length: usize,
    by: Author,
}
/// A file as a test reads it: the folder it's in, as the names from the top down to it; its
/// name; how many bytes it holds; and who it came from.
pub type Saving = (String, String, usize, Author);
/// What the stand-in keeps, in every test, each DSP's apart.
struct Kept {
    folders: Vec<KeptFolder>,
    files: Vec<KeptFile>,
    unready: BTreeSet<String>,
    refusing: BTreeSet<String>,
}
impl Kept {
    /// The names of the DSP's folders from the top down to `id`, as a path.
    fn path(&self, dsp: &str, id: &str) -> String {
        let folder = self
            .folders
            .iter()
            .find(|folder| folder.dsp == dsp && folder.id == id)
            .expect("the folder is kept");
        match &folder.parent {
            Some(parent) => format!("{}/{}", self.path(dsp, parent), folder.name),
            None => folder.name.clone(),
        }
    }
}
static KEPT: Mutex<Kept> = Mutex::new(Kept {
    folders: Vec::new(),
    files: Vec::new(),
    unready: BTreeSet::new(),
    refusing: BTreeSet::new(),
});
fn kept() -> std::sync::MutexGuard<'static, Kept> {
    KEPT.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// The files the stand-in keeps for the DSP `dsp`, in the order they were saved.
pub fn files(dsp: &str) -> Vec<Saving> {
    let kept = kept();
    kept.files
        .iter()
        .filter(|file| file.dsp == dsp)
        .map(|file| {
            (
                kept.path(dsp, &file.folder),
                file.name.clone(),
                file.length,
                file.by.clone(),
            )
        })
        .collect()
}
/// How many folders the stand-in keeps for the DSP `dsp`.
pub fn folders(dsp: &str) -> usize {
    kept().folders.iter().filter(|f| f.dsp == dsp).count()
}
/// Takes the DSP's folder at `path` away, with every folder in it, as a member trashing it.
pub fn trash(dsp: &str, path: &str) {
    let mut kept = kept();
    let gone: BTreeSet<String> = kept
        .folders
        .iter()
        .filter(|folder| folder.dsp == dsp)
        .filter(|folder| {
            let at = kept.path(dsp, &folder.id);
            at == path || at.starts_with(&format!("{path}/"))
        })
        .map(|folder| folder.id.clone())
        .collect();
    kept.folders
        .retain(|folder| !(folder.dsp == dsp && gone.contains(&folder.id)));
}
/// Has the stand-in keep the DSP's files, or not for now.
pub fn ready_to_keep(dsp: &str, ready: bool) {
    let mut kept = kept();
    if ready {
        kept.unready.remove(dsp);
    } else {
        kept.unready.insert(dsp.to_owned());
    }
}
/// Has the stand-in refuse what's saved for the DSP, or keep it again.
pub fn refuse(dsp: &str, refusing: bool) {
    let mut kept = kept();
    if refusing {
        kept.refusing.insert(dsp.to_owned());
    } else {
        kept.refusing.remove(dsp);
    }
}

/// The stand-in for where a DSP keeps its files.
struct Keeping;
static KEEPING: Keeping = Keeping;
static KEEPER: Feature = Feature {
    switch: optional("keeping", "Keeping", &[]),
    exposes: &[dispatch_core::expose!(
        dyn Files,
        &KEEPING,
        "files",
        "Where the stand-in keeps files.",
        shown,
    )],
    ..feature("keeping")
};
impl Shown for Keeping {
    fn name(&self) -> &'static str {
        "Keeping"
    }
    fn permission(&self) -> &'static str {
        "keeping.use"
    }
}
fn not_found() -> Error {
    Error::new("files_folder_not_found", 404)
}
impl Files for Keeping {
    fn largest(&self) -> u64 {
        UPLOAD_LIMIT
    }
    fn unready<'a>(&'a self, _: &'a Arc<State>, dsp: &'a str) -> Answer<'a, Option<String>> {
        let unready = kept().unready.contains(dsp);
        Box::pin(async move { Ok(unready.then(|| "Not ready".to_owned())) })
    }
    fn browse<'a>(
        &'a self,
        _: &'a Arc<State>,
        _: &'a str,
        _: Option<&'a str>,
    ) -> Answer<'a, Browsed> {
        Box::pin(async move {
            Ok(Browsed {
                path: Vec::new(),
                folders: Vec::new(),
            })
        })
    }
    fn folder<'a>(
        &'a self,
        _: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
    ) -> Answer<'a, Option<Folder>> {
        let found = kept()
            .folders
            .iter()
            .find(|folder| folder.dsp == dsp && folder.id == id)
            .map(|folder| Folder {
                id: folder.id.clone(),
                name: folder.name.clone(),
            });
        Box::pin(async move { Ok(found) })
    }
    fn create_folder<'a>(
        &'a self,
        _: &'a Arc<State>,
        dsp: &'a str,
        parent: Option<&'a str>,
        name: &'a str,
        _: &'a Author,
    ) -> Answer<'a, Folder> {
        Box::pin(async move {
            let mut kept = kept();
            let inside = |id: &str| kept.folders.iter().any(|f| f.dsp == dsp && f.id == id);
            if parent.is_some_and(|id| !inside(id)) {
                return Err(not_found());
            }
            let id = format!("folder-{}", kept.folders.len() + 1);
            kept.folders.push(KeptFolder {
                dsp: dsp.to_owned(),
                id: id.clone(),
                parent: parent.map(str::to_owned),
                name: name.to_owned(),
            });
            Ok(Folder {
                id,
                name: name.to_owned(),
            })
        })
    }
    fn save<'a>(
        &'a self,
        _: &'a Arc<State>,
        dsp: &'a str,
        folder: &'a str,
        file: Incoming,
    ) -> Answer<'a, Saved> {
        Box::pin(async move {
            let bytes: Vec<u8> = file
                .bytes
                .stream()
                .map_ok(|chunk| chunk.to_vec())
                .try_concat()
                .await?;
            let mut kept = kept();
            if kept.refusing.contains(dsp) {
                return Err(Error::new("keeping_refused", 502));
            }
            if !kept.folders.iter().any(|f| f.dsp == dsp && f.id == folder) {
                return Err(not_found());
            }
            let id = format!("saved-{}", kept.files.len() + 1);
            kept.files.push(KeptFile {
                dsp: dsp.to_owned(),
                folder: folder.to_owned(),
                name: file.name.clone(),
                length: bytes.len(),
                by: file.by,
            });
            Ok(Saved {
                link: format!("https://example.test/{id}"),
                id,
                name: file.name,
            })
        })
    }
    fn write<'a>(&'a self, _: &'a Arc<State>, _: &'a str, _: &'a str, _: Page) -> Answer<'a, Saved> {
        Box::pin(async { Err(Error::new("keeping_no_pages", 501)) })
    }
    fn rewrite<'a>(
        &'a self,
        _: &'a Arc<State>,
        _: &'a str,
        _: &'a str,
        _: Page,
    ) -> Answer<'a, Option<Saved>> {
        Box::pin(async { Err(Error::new("keeping_no_pages", 501)) })
    }
}
