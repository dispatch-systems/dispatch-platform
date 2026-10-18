//! What every Slack backend test installs. A test binary holds one registry, so each installs
//! the same: Slack; a stand-in feature with an integration through it, which hears of Slack's
//! events as an integration would and keeps what it heard, and hears when the DSP's people
//! change; and a stand-in for where a DSP keeps its files, which keeps what's saved and written
//! there in memory.
use crate::FEATURE;
use dispatch_core::{
    State,
    manifest::{
        Feature, Upkeep,
        events::PeopleChanged,
        feature,
        integrations::{Heard, Integration, integration},
        optional,
    },
    server::http::upload::UPLOAD_LIMIT,
    testing,
};
use dispatch_documents_contract::{Answer, Author, Browsed, Files, Folder, Incoming, Page, Saved};
use futures_util::TryStreamExt;
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

/// The stand-in's integration: it needs to read files, hears of files shared, answers fixture
/// mode's Slack for `listening.ping`, a method only it calls, and clears what it keeps of a
/// workspace Slack forgets.
pub const LISTENING: Integration = Integration {
    needs: &["files:read"],
    hears: &["file_shared"],
    heard: Some(hear),
    fixture: Some(answer),
    forget: Some(forget),
    ..integration("slack", "listening.files", "Listening")
};
/// Each DSP and workspace whose files the stand-in cleared, in every test.
static FORGOTTEN: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
fn forget(_: &dispatch_core::db::Store, dsp: &str, team: &str) -> dispatch_core::Result<()> {
    let mut kept = FORGOTTEN
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    kept.push((dsp.to_owned(), team.to_owned()));
    Ok(())
}
/// The workspaces of the DSP `dsp` whose files the stand-in cleared.
pub fn forgotten(dsp: &str) -> Vec<String> {
    let kept = FORGOTTEN
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    kept.iter()
        .filter(|(at, _)| at == dsp)
        .map(|(_, team)| team.clone())
        .collect()
}
fn answer(
    team: &str,
    method: &str,
    _: &serde_json::Value,
) -> Option<dispatch_core::Result<serde_json::Value>> {
    (method == "listening.ping").then(|| Ok(serde_json::json!({"ok": true, "team": team})))
}
/// A feature that works through Slack, on for a test's DSP as every feature is, until a test
/// switches it off.
static LISTENER: Feature = Feature {
    switch: optional("listening", "Listening", &[]),
    depends_on: &["slack"],
    integrations: &[LISTENING],
    on: &[dispatch_core::on!(PeopleChanged, people_changed)],
    ..feature("listening")
};
/// Each DSP whose people the stand-in heard changed, in every test, once each time.
static CHANGED: Mutex<Vec<String>> = Mutex::new(Vec::new());
fn people_changed(_: Arc<State>, dsp: String, _: PeopleChanged) -> Upkeep {
    let mut kept = CHANGED.lock().unwrap_or_else(|poison| poison.into_inner());
    kept.push(dsp);
    Box::pin(async {})
}
/// How many times the stand-in heard the DSP `dsp`'s people changed.
pub fn changed(dsp: &str) -> usize {
    let kept = CHANGED.lock().unwrap_or_else(|poison| poison.into_inner());
    kept.iter().filter(|at| *at == dsp).count()
}
/// What the stand-in heard of, in every test: each event's DSP, workspace and kind.
static HEARD: Mutex<Vec<(String, String, String)>> = Mutex::new(Vec::new());
fn hear(_: std::sync::Arc<dispatch_core::State>, heard: Heard) -> Upkeep {
    let mut kept = HEARD.lock().unwrap_or_else(|poison| poison.into_inner());
    kept.push((heard.dsp, heard.account, heard.kind));
    Box::pin(async {})
}
/// What the stand-in heard of from the DSP `dsp`: each event's workspace and kind.
pub fn heard(dsp: &str) -> Vec<(String, String)> {
    let kept = HEARD.lock().unwrap_or_else(|poison| poison.into_inner());
    kept.iter()
        .filter(|(at, _, _)| at == dsp)
        .map(|(_, team, kind)| (team.clone(), kind.clone()))
        .collect()
}

/// A file kept by the stand-in: its DSP, folder, name, length and who it came from.
pub type KeptFile = (String, String, String, usize, Author);
/// Where the stand-in keeps files, in every test.
static KEPT: Mutex<Vec<KeptFile>> = Mutex::new(Vec::new());
/// The DSPs whose files the stand-in can't keep for now.
static UNREADY: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
/// The stand-in for where a DSP keeps its files: every folder whose ID begins `folder-` is one.
pub struct Keeping;
static KEEPING: Keeping = Keeping;
static KEEPER: Feature = Feature {
    switch: optional("keeping", "Keeping", &[]),
    exposes: &[dispatch_core::expose!(
        dyn Files,
        &KEEPING,
        "files",
        "Where the stand-in keeps files."
    )],
    ..feature("keeping")
};
impl Files for Keeping {
    fn name(&self) -> &'static str {
        "Keeping"
    }
    fn permission(&self) -> &'static str {
        "keeping.use"
    }
    fn largest(&self) -> u64 {
        UPLOAD_LIMIT
    }
    fn unready<'a>(&'a self, _: &'a Arc<State>, dsp: &'a str) -> Answer<'a, Option<String>> {
        let unready = UNREADY.lock().unwrap().contains(dsp);
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
        _: &'a str,
        id: &'a str,
    ) -> Answer<'a, Option<Folder>> {
        Box::pin(async move {
            Ok(id.starts_with("folder-").then(|| Folder {
                id: id.to_owned(),
                name: id.trim_start_matches("folder-").to_owned(),
            }))
        })
    }
    fn create_folder<'a>(
        &'a self,
        _: &'a Arc<State>,
        _: &'a str,
        name: &'a str,
        _: &'a Author,
    ) -> Answer<'a, Folder> {
        Box::pin(async move {
            Ok(Folder {
                id: format!("folder-{name}"),
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
            let kept = (
                dsp.to_owned(),
                folder.to_owned(),
                file.name.clone(),
                bytes.len(),
                file.by,
            );
            KEPT.lock().unwrap().push(kept);
            Ok(Saved {
                id: format!("saved-{}", file.name),
                name: file.name.clone(),
                link: format!("https://example.test/{}", file.name),
            })
        })
    }
    fn write<'a>(
        &'a self,
        _: &'a Arc<State>,
        dsp: &'a str,
        folder: &'a str,
        page: Page,
    ) -> Answer<'a, Saved> {
        Box::pin(async move {
            let id = format!("doc-{}", WRITTEN.fetch_add(1, Ordering::Relaxed) + 1);
            let mut pages = PAGES.lock().unwrap();
            pages.push(KeptPage {
                dsp: dsp.to_owned(),
                folder: folder.to_owned(),
                id: id.clone(),
                name: page.name.clone(),
                html: page.html,
                by: page.by,
                writes: 1,
            });
            Ok(Saved {
                link: format!("https://example.test/{id}"),
                id,
                name: page.name,
            })
        })
    }
    fn rewrite<'a>(
        &'a self,
        _: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
        page: Page,
    ) -> Answer<'a, Option<Saved>> {
        Box::pin(async move {
            let mut pages = PAGES.lock().unwrap();
            let Some(kept) = pages
                .iter_mut()
                .find(|kept| kept.dsp == dsp && kept.id == id)
            else {
                return Ok(None);
            };
            kept.name = page.name;
            kept.html = page.html;
            kept.by = page.by;
            kept.writes += 1;
            Ok(Some(Saved {
                id: id.to_owned(),
                name: kept.name.clone(),
                link: format!("https://example.test/{id}"),
            }))
        })
    }
}
/// A page the stand-in keeps as a document: where, as what, who wrote it last, and how often
/// it was written.
#[derive(Clone, Debug)]
pub struct KeptPage {
    pub dsp: String,
    pub folder: String,
    pub id: String,
    pub name: String,
    pub html: String,
    pub by: Author,
    pub writes: usize,
}
/// The pages the stand-in keeps, in every test, and how many it ever wrote.
static PAGES: Mutex<Vec<KeptPage>> = Mutex::new(Vec::new());
static WRITTEN: AtomicUsize = AtomicUsize::new(0);
/// The pages the stand-in keeps for the DSP `dsp`.
pub fn pages(dsp: &str) -> Vec<KeptPage> {
    PAGES
        .lock()
        .unwrap()
        .iter()
        .filter(|kept| kept.dsp == dsp)
        .cloned()
        .collect()
}
/// Takes the DSP's document `id` away, as a member deleting it would.
pub fn delete_page(dsp: &str, id: &str) {
    PAGES
        .lock()
        .unwrap()
        .retain(|kept| !(kept.dsp == dsp && kept.id == id));
}
/// What the stand-in kept for the DSP `dsp`.
pub fn kept(dsp: &str) -> Vec<KeptFile> {
    KEPT.lock()
        .unwrap()
        .iter()
        .filter(|kept| kept.0 == dsp)
        .cloned()
        .collect()
}
/// Has the stand-in keep the DSP's files, or not for now.
pub fn ready(dsp: &str, ready: bool) {
    let mut unready = UNREADY.lock().unwrap();
    if ready {
        unready.remove(dsp);
    } else {
        unready.insert(dsp.to_owned());
    }
}

/// Installs Slack and the stand-ins.
pub fn install() {
    testing::install(&[], &[&FEATURE, &LISTENER, &KEEPER]);
}
