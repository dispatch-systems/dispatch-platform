//! What every Conduct backend test installs and starts from. A test binary holds one registry,
//! so each installs the same: Conduct, and a stand-in for where a DSP keeps its files, which
//! keeps its folders and what's saved in them in memory, and can be made unready, or to refuse
//! what's saved or taken back. And how a test starts: a platform with one DSP and its owner writing up.
use crate::FEATURE;
use dispatch_core::{
    Error, Result, State,
    accounts::{Auth, Context},
    db::{Store, s},
    manifest::{Feature, exposes::Shown, feature, optional},
    server::{http::upload::UPLOAD_LIMIT, operations},
    testing,
};
use dispatch_documents_contract::{
    Answer, Author, Browsed, Files, Folder, Incoming, NewFolder, Page, Saved,
};
use futures_util::TryStreamExt;
use std::{
    collections::{BTreeMap, BTreeSet},
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

/// A folder the stand-in keeps: its DSP, its ID, the folder it's in, its name, and what the
/// activity log was to call it in its name's place.
struct KeptFolder {
    dsp: String,
    id: String,
    parent: Option<String>,
    name: String,
    logged_as: Option<&'static str>,
}
/// A file the stand-in keeps: its DSP, its ID, the folder it's in, its name, how many bytes it
/// holds, and who it came from.
struct KeptFile {
    dsp: String,
    id: String,
    folder: String,
    name: String,
    length: usize,
    by: Author,
}
/// A file as a test reads it: the folder it's in, as the names from the top down to it; its
/// name; how many bytes it holds; and who it came from.
pub type Saving = (String, String, usize, Author);
/// What a test has the stand-in do to the platform as it keeps a DSP's next file.
type OnSave = Box<dyn FnOnce(&Store, &str) -> Result<()> + Send>;
/// What the stand-in keeps, in every test, each DSP's apart.
struct Kept {
    /// How many folders it ever made: each one's ID is its own.
    made: usize,
    /// How many files it ever saved: each one's ID is its own.
    saved: usize,
    folders: Vec<KeptFolder>,
    files: Vec<KeptFile>,
    unready: BTreeSet<String>,
    refusing: BTreeMap<String, &'static str>,
    on_save: BTreeMap<String, OnSave>,
    /// Whose files it took back, by DSP: who asked, in the order they did.
    removed: Vec<(String, Author)>,
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
    made: 0,
    saved: 0,
    folders: Vec::new(),
    files: Vec::new(),
    unready: BTreeSet::new(),
    refusing: BTreeMap::new(),
    on_save: BTreeMap::new(),
    removed: Vec::new(),
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
/// Who asked the stand-in to take a file of the DSP `dsp` back, in the order they did.
pub fn removed(dsp: &str) -> Vec<Author> {
    kept()
        .removed
        .iter()
        .filter(|(of, _)| of == dsp)
        .map(|(_, by)| by.clone())
        .collect()
}
/// The folders the stand-in keeps for the DSP `dsp`, each as the activity log was to call it:
/// by its name, unless it was given something to call it in its place.
pub fn folders(dsp: &str) -> Vec<String> {
    kept()
        .folders
        .iter()
        .filter(|folder| folder.dsp == dsp)
        .map(|folder| folder.logged_as.unwrap_or(&folder.name).to_owned())
        .collect()
}
/// Takes the DSP's folder at `path` away, with every folder and file in it, as a member
/// trashing it.
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
    kept.files
        .retain(|file| !(file.dsp == dsp && gone.contains(&file.folder)));
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
/// Has the stand-in refuse what's saved for the DSP, as `FULL` or `BUSY`, or with none keep it
/// again.
pub fn refuse(dsp: &str, with: Option<&'static str>) {
    let mut kept = kept();
    match with {
        Some(code) => kept.refusing.insert(dsp.to_owned(), code),
        None => kept.refusing.remove(dsp),
    };
}

/// Has the stand-in switch Conduct off for the DSP once it keeps the next file of theirs, as
/// the platform owner switching it off while its copies are saved.
pub fn switch_off_on_save(dsp: &str) {
    on_save(dsp, |db, dsp| {
        let owner = testing::a_user(db);
        db.set_feature(dsp, "conduct", false, &owner).map(drop)
    });
}
/// Has the stand-in do `then` to the platform once it keeps the next file of the DSP's, as
/// someone changing things while its copies are saved.
pub fn on_save(dsp: &str, then: impl FnOnce(&Store, &str) -> Result<()> + Send + 'static) {
    kept().on_save.insert(dsp.to_owned(), Box::new(then));
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
/// How the stand-in refuses what's saved, each of which it has words for.
pub const FULL: &str = "keeping_full";
pub const BUSY: &str = "keeping_busy";
fn not_found() -> Error {
    Error::new("files_folder_not_found", 404)
}
impl Files for Keeping {
    fn largest(&self) -> u64 {
        UPLOAD_LIMIT
    }
    fn place(&self) -> &'static str {
        "Kept Drive"
    }
    fn refusal(&self, code: &str) -> Option<&'static str> {
        match code {
            FULL => Some("Kept Drive is full"),
            BUSY => Some("Kept Drive is busy"),
            _ => None,
        }
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
        folder: NewFolder<'a>,
    ) -> Answer<'a, Folder> {
        Box::pin(async move {
            let mut kept = kept();
            let inside = |id: &str| kept.folders.iter().any(|f| f.dsp == dsp && f.id == id);
            if folder.parent.is_some_and(|id| !inside(id)) {
                return Err(not_found());
            }
            kept.made += 1;
            let id = format!("folder-{}", kept.made);
            kept.folders.push(KeptFolder {
                dsp: dsp.to_owned(),
                id: id.clone(),
                parent: folder.parent.map(str::to_owned),
                name: folder.name.to_owned(),
                logged_as: folder.logged_as,
            });
            Ok(Folder {
                id,
                name: folder.name.to_owned(),
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
            let bytes: Vec<u8> = file
                .bytes
                .stream()
                .map_ok(|chunk| chunk.to_vec())
                .try_concat()
                .await?;
            let (id, then) = {
                let mut kept = kept();
                if let Some(code) = kept.refusing.get(dsp) {
                    return Err(Error::new(*code, 502));
                }
                if !kept.folders.iter().any(|f| f.dsp == dsp && f.id == folder) {
                    return Err(not_found());
                }
                kept.saved += 1;
                let id = format!("saved-{}", kept.saved);
                kept.files.push(KeptFile {
                    dsp: dsp.to_owned(),
                    id: id.clone(),
                    folder: folder.to_owned(),
                    name: file.name.clone(),
                    length: bytes.len(),
                    by: file.by,
                });
                (id, kept.on_save.remove(dsp))
            };
            if let Some(then) = then {
                let at = dsp.to_owned();
                state.run(move |db| then(db, &at)).await?;
            }
            Ok(Saved {
                link: format!("https://example.test/{id}"),
                id,
                name: file.name,
            })
        })
    }
    fn write<'a>(
        &'a self,
        _: &'a Arc<State>,
        _: &'a str,
        _: &'a str,
        _: Page,
    ) -> Answer<'a, Saved> {
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
    fn remove<'a>(
        &'a self,
        _: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
        by: &'a Author,
    ) -> Answer<'a, bool> {
        Box::pin(async move {
            let mut kept = kept();
            if let Some(code) = kept.refusing.get(dsp) {
                return Err(Error::new(*code, 502));
            }
            let before = kept.files.len();
            kept.files
                .retain(|file| !(file.dsp == dsp && file.id == id));
            let removed = kept.files.len() < before;
            if removed {
                kept.removed.push((dsp.to_owned(), by.clone()));
            }
            Ok(removed)
        })
    }
}
