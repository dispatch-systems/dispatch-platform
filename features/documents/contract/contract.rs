//! Files, the contract Documents exposes: somewhere to keep a DSP's files, in folders. A feature
//! bringing files in from elsewhere saves them through it, never knowing which feature keeps
//! them or where; it uses this crate, which depends on core alone, and asks `for_dsp`.
use dispatch_core::{
    Result, State, db::Store, manifest::exposes::Shown, server::http::upload::Upload,
};
use std::{future::Future, pin::Pin, sync::Arc};

/// What its asynchronous answers are: work done outside the database, as a call to Google is.
pub type Answer<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// What a feature that keeps files offers, which a page shows as `files`: its name, and the
/// permission a member needs to see its folders, and so to choose one. Every call names the
/// DSP, and works only inside the files it keeps for that DSP: a folder that isn't one of them
/// is refused as `files_folder_not_found`.
pub trait Files: Shown + Send + Sync {
    /// The most one file may be, in bytes.
    fn largest(&self) -> u64;
    /// Why it can't keep the DSP's files now, in words a member reads, as what it keeps them in
    /// isn't connected; none once it can.
    fn unready<'a>(&'a self, state: &'a Arc<State>, dsp: &'a str) -> Answer<'a, Option<String>>;
    /// The folder `id`, or the top of the DSP's files: where it is, and the folders it holds.
    fn browse<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: Option<&'a str>,
    ) -> Answer<'a, Browsed>;
    /// The folder `id`, while it's still there.
    fn folder<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
    ) -> Answer<'a, Option<Folder>>;
    /// Makes a folder named `name` at the top of the DSP's files, as `by`.
    fn create_folder<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        name: &'a str,
        by: &'a Author,
    ) -> Answer<'a, Folder>;
    /// Keeps `file` in the folder `folder`.
    fn save<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        folder: &'a str,
        file: Incoming,
    ) -> Answer<'a, Saved>;
    /// Keeps `page` in the folder `folder` as a document members open and edit as one of their
    /// own, written from its HTML with its pictures in it. One too large to write is refused as
    /// `upload_too_large`.
    fn write<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        folder: &'a str,
        page: Page,
    ) -> Answer<'a, Saved>;
    /// Writes `page` over the document `id` that `write` kept, wherever it's been moved among
    /// the DSP's files, named as the page is: none once it's gone from them.
    fn rewrite<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
        page: Page,
    ) -> Answer<'a, Option<Saved>>;
}

/// The feature keeping the DSP's files, while it's on for the DSP.
pub fn for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn Files>> {
    dispatch_core::manifest::exposes::for_dsp::<dyn Files>(db, dsp)
}

/// A folder of a DSP's files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    pub id: String,
    pub name: String,
}

/// A folder, as a member choosing one sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Browsed {
    /// The folders from the top of the DSP's files down to it, itself last.
    pub path: Vec<Folder>,
    /// The folders it holds, by name.
    pub folders: Vec<Folder>,
}

/// A file coming in, as it arrives.
pub struct Incoming {
    pub name: String,
    pub mime: String,
    pub bytes: Upload,
    /// Who it's from.
    pub by: Author,
    /// Where it came from, by the name members know that by.
    pub from: &'static str,
}

/// A page kept as a document: its name, and its HTML, a whole page whose pictures are in it as
/// `data:` addresses.
pub struct Page {
    pub name: String,
    pub html: String,
    /// Who wrote it: for a page written over a document, who changed it last.
    pub by: Author,
    /// Where it came from, by the name members know that by.
    pub from: &'static str,
}

/// Who a file or folder came from: one of the DSP's members, or someone the DSP knows only by
/// name, as someone elsewhere no member matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Author {
    Member(String),
    Named(String),
}

/// A file kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub id: String,
    pub name: String,
    /// Where it opens.
    pub link: String,
}
