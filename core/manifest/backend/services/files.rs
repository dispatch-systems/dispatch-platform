//! Files: somewhere to keep a DSP's files, in folders. A feature bringing files in from
//! elsewhere saves them through it, never knowing which feature keeps them or where.
use super::Answer;
use crate::{Result, State, db::Store, manifest::registry, server::http::upload::Upload};
use std::sync::Arc;

/// What a feature that keeps files offers. Every call names the DSP, and works only inside the
/// files it keeps for that DSP: a folder that isn't one of them is refused as
/// `files_folder_not_found`.
pub trait Files: Send + Sync {
    /// What it's called where members see it, as its page is named.
    fn name(&self) -> &'static str;
    /// The permission a member needs to see its folders, and so to choose one.
    fn permission(&self) -> &'static str;
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
}

/// The feature keeping the DSP's files, while it's on for the DSP.
pub fn for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn Files>> {
    super::for_dsp(db, dsp, registry().files())
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
