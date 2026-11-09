//! A DSP's Documents as a tree: the files Dispatch can reach in the account that sit inside
//! the DSP's main folder, however deep. Whatever else the account holds, such as another DSP's
//! folder made with the same account, is not in it, so nothing outside it can be named.
//!
//! The server keeps one for each DSP using Documents, so it's packed: each file once, the
//! folder it's in sharing that folder's ID, and who changed it shared among the files they
//! changed. A change rebuilds the places of the files from the files themselves.
use super::{
    drive::{self, Changed, Item, Person},
    google::FOLDER,
};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    mem,
    sync::Arc,
    time::Instant,
};

/// How deep a folder may sit: deeper than anyone nests, and a bound on a parent loop.
const DEPTH: usize = 64;

/// A file's type: a folder, Google's own and PDFs by name, any other as Google wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mime {
    Folder,
    Doc,
    Sheet,
    Slides,
    Pdf,
    Other(Box<str>),
}
impl Mime {
    fn of(mime: &str) -> Self {
        match mime {
            FOLDER => Self::Folder,
            drive::DOC => Self::Doc,
            drive::SHEET => Self::Sheet,
            drive::SLIDES => Self::Slides,
            "application/pdf" => Self::Pdf,
            other => Self::Other(other.into()),
        }
    }
    pub fn as_str(&self) -> &str {
        match self {
            Self::Folder => FOLDER,
            Self::Doc => drive::DOC,
            Self::Sheet => drive::SHEET,
            Self::Slides => drive::SLIDES,
            Self::Pdf => "application/pdf",
            Self::Other(mime) => mime,
        }
    }
}

/// A file or folder as the tree keeps it: what the page shows of it, and Google's links.
#[derive(Clone, Debug)]
pub struct Row {
    pub id: Arc<str>,
    pub name: Box<str>,
    pub mime: Mime,
    /// The folder it's in: Google lists one for each file Dispatch made or was given.
    pub parent: Option<Arc<str>>,
    pub modified: Box<str>,
    pub by: Option<Arc<Person>>,
    pub size: Option<u64>,
    pub link: Box<str>,
    pub thumbnail: Option<Box<str>>,
    /// Which picture `thumbnail` is: a number that grows as the file changes.
    pub version: Option<u64>,
    /// When Google described it, which its picture's link lasts hours from.
    pub seen: Instant,
    /// For a shortcut, the file it stands for, whose bytes a download takes.
    pub target: Option<Arc<str>>,
    /// That file as Google last described it, while the account reaches it.
    pub stands_for: Option<Arc<Row>>,
}
impl Row {
    /// A record's own metadata. `seen` is observation time; `stands_for` is resolved by
    /// grown(), so incoming records must be compared with the canonical rows/linked entry.
    fn same_metadata(&self, other: &Self) -> bool {
        self.id == other.id
            && self.name == other.name
            && self.mime == other.mime
            && self.parent == other.parent
            && self.modified == other.modified
            && self.by == other.by
            && self.size == other.size
            && self.link == other.link
            && self.thumbnail == other.thumbnail
            && self.version == other.version
            && self.target == other.target
    }
    fn same_semantics(&self, other: &Self) -> bool {
        self.same_metadata(other)
            && match (&self.stands_for, &other.stands_for) {
                (Some(a), Some(b)) => a.same_semantics(b),
                (None, None) => true,
                _ => false,
            }
    }
    pub fn folder(&self) -> bool {
        self.mime == Mime::Folder
    }
    /// What the page shows of it: for a shortcut, the file it stands for, whose name and
    /// changes are its owner's.
    pub fn shown(&self) -> &Row {
        self.stands_for.as_deref().unwrap_or(self)
    }
    /// A shortcut whose file the account no longer reaches: its owner trashed it, or stopped
    /// sharing it.
    pub fn unavailable(&self) -> bool {
        self.target.is_some() && self.stands_for.is_none()
    }
}
impl From<Item> for Row {
    fn from(item: Item) -> Self {
        // A shortcut shows as the file it stands for, and opens it. One to a folder opens it in
        // Drive, since Dispatch can't reach what that folder holds.
        let (mime, link, target) = match &item.shortcut_details {
            Some(shortcut) => (
                match shortcut.target_mime_type.as_str() {
                    FOLDER => Mime::of(drive::SHORTCUT),
                    target => Mime::of(target),
                },
                drive::link(&shortcut.target_mime_type, &shortcut.target_id),
                Some(Arc::from(shortcut.target_id.as_str())),
            ),
            None => (Mime::of(&item.mime_type), item.web_view_link, None),
        };
        Self {
            parent: item.parents.into_iter().next().map(Arc::from),
            id: item.id.into(),
            name: item.name.into(),
            mime,
            modified: item.modified_time.into(),
            by: item.last_modifying_user.map(Arc::new),
            size: item.size.and_then(|size| size.parse().ok()),
            link: link.into(),
            thumbnail: item.thumbnail_link.map(Into::into),
            version: item.thumbnail_version.and_then(|v| v.parse().ok()),
            seen: item.seen,
            target,
            stands_for: None,
        }
    }
}

#[derive(Clone)]
pub struct Tree {
    root: Arc<str>,
    rows: Vec<Row>,
    /// Each row's place in `rows`, by its ID.
    places: HashMap<Arc<str>, u32>,
    /// What each folder holds, in the order it's shown, as ranges of `held`: each row's at its
    /// place, the main folder's after them.
    starts: Vec<u32>,
    held: Vec<u32>,
    /// The files outside the main folder that shortcuts inside stand for, by ID.
    linked: HashMap<Arc<str>, Row>,
    /// Owner-local mutation sequence, preserved through regrow; never a cache revision.
    mutation: u64,
}
/// Only IDs touched in one mutation, with their initial membership and canonical record.
/// This temporary baseline is dropped after regrow; it never retains an old Tree.
type Before = HashMap<Arc<str>, Option<(bool, Row)>>;

impl Tree {
    /// The files of `all` inside the folder `root`.
    pub fn new(root: &str, all: Vec<Item>) -> Self {
        let mut people: HashMap<(Option<String>, Option<String>), Arc<Person>> = HashMap::new();
        let rows = all
            .into_iter()
            .map(|mut item| {
                let by = item.last_modifying_user.take().map(|person| {
                    let key = (person.display_name.clone(), person.email_address.clone());
                    Arc::clone(people.entry(key).or_insert_with(|| Arc::new(person)))
                });
                Row {
                    by,
                    ..Row::from(item)
                }
            })
            .collect();
        Self::grown(root.into(), rows)
    }
    /// The tree of the rows inside `root`, each pointing at its folder's own ID.
    fn grown(root: Arc<str>, mut rows: Vec<Row>) -> Self {
        let places: HashMap<Arc<str>, u32> = rows
            .iter()
            .enumerate()
            .map(|(place, row)| (Arc::clone(&row.id), place as u32))
            .collect();
        let inside = |row: &Row| {
            let mut at = row;
            for _ in 0..DEPTH {
                match at.parent.as_deref() {
                    Some(up) if up == &*root => return true,
                    Some(up) => match places.get(up) {
                        Some(place) => at = &rows[*place as usize],
                        None => return false,
                    },
                    None => return false,
                }
            }
            false
        };
        let kept: Vec<bool> = rows
            .iter()
            .map(|row| row.id != root && inside(row))
            .collect();
        drop(places);
        // The files the shortcuts inside stand for stay beside the tree, wherever they are.
        let targets: HashSet<Arc<str>> = rows
            .iter()
            .zip(&kept)
            .filter(|(_, kept)| **kept)
            .filter_map(|(row, _)| row.target.clone())
            .collect();
        let mut linked = HashMap::new();
        let mut place = 0;
        rows.retain_mut(|row| {
            place += 1;
            if !kept[place - 1] && targets.contains(&row.id) {
                linked.insert(Arc::clone(&row.id), row.clone());
            }
            kept[place - 1]
        });
        for row in &mut rows {
            row.stands_for = row
                .target
                .as_ref()
                .and_then(|target| linked.get(target))
                .map(|file: &Row| Arc::new(file.clone()));
        }
        rows.shrink_to_fit();
        let places: HashMap<Arc<str>, u32> = rows
            .iter()
            .enumerate()
            .map(|(place, row)| (Arc::clone(&row.id), place as u32))
            .collect();
        // Each row's folder: the main folder, after the rows, or a row.
        let folders: Vec<usize> = rows
            .iter()
            .map(|row| match row.parent.as_deref() {
                Some(up) if up != &*root => places[up] as usize,
                _ => rows.len(),
            })
            .collect();
        let parents: Vec<Arc<str>> = folders
            .iter()
            .map(|folder| match rows.get(*folder) {
                Some(row) => Arc::clone(&row.id),
                None => Arc::clone(&root),
            })
            .collect();
        for (row, parent) in rows.iter_mut().zip(parents) {
            row.parent = Some(parent);
        }
        let mut starts = vec![0u32; rows.len() + 2];
        for folder in &folders {
            starts[folder + 1] += 1;
        }
        for at in 1..starts.len() {
            starts[at] += starts[at - 1];
        }
        let mut next = starts.clone();
        let mut held = vec![0u32; rows.len()];
        for (place, folder) in folders.iter().enumerate() {
            held[next[*folder] as usize] = place as u32;
            next[*folder] += 1;
        }
        for folder in 0..=rows.len() {
            let range = starts[folder] as usize..starts[folder + 1] as usize;
            held[range].sort_by(|a, b| order(&rows[*a as usize], &rows[*b as usize]));
        }
        Self {
            root,
            rows,
            places,
            starts,
            held,
            linked,
            mutation: 0,
        }
    }
    pub fn root(&self) -> &str {
        &self.root
    }
    /// The file `id`, if it is inside.
    pub fn get(&self, id: &str) -> Option<&Row> {
        self.places.get(id).map(|place| &self.rows[*place as usize])
    }
    /// Whether `id` is the main folder or a folder inside it.
    /// The files the shortcuts in Documents stand for.
    pub fn targets(&self) -> BTreeSet<String> {
        self.rows
            .iter()
            .filter_map(|row| row.target.as_deref().map(str::to_owned))
            .collect()
    }
    /// Whether a shortcut in Documents stands for the file `id`.
    pub fn stands_for(&self, id: &str) -> bool {
        self.rows
            .iter()
            .any(|row| row.target.as_deref() == Some(id))
    }
    pub fn is_folder(&self, id: &str) -> bool {
        id == &*self.root || self.get(id).is_some_and(Row::folder)
    }
    fn held(&self, id: &str) -> &[u32] {
        let folder = match self.places.get(id) {
            Some(place) => *place as usize,
            None if id == &*self.root => self.rows.len(),
            None => return &[],
        };
        &self.held[self.starts[folder] as usize..self.starts[folder + 1] as usize]
    }
    /// Borrowed direct children in the existing order; no complete result allocation.
    pub(super) fn ordered_children(&self, id: &str) -> impl Iterator<Item = &Row> {
        self.held(id)
            .iter()
            .map(|place| &self.rows[*place as usize])
    }
    /// Complete equality is needed only when a full listing replaces the tree.
    pub(super) fn same_semantics(&self, other: &Self) -> bool {
        // Duplicate raw IDs cannot be matched one-to-one through places. Conservatively
        // invalidate rather than let one canonical row stand for multiple different rows.
        self.root == other.root
            && self.rows.len() == other.rows.len()
            && self.places.len() == self.rows.len()
            && other.places.len() == other.rows.len()
            && self.rows.iter().all(|row| {
                other
                    .get(&row.id)
                    .is_some_and(|other| row.same_semantics(other))
            })
    }
    pub(super) fn mutation(&self) -> u64 {
        self.mutation
    }
    /// Canonical records, never the potentially stale resolved target on a shortcut.
    fn record(&self, id: &str) -> Option<(bool, &Row)> {
        self.get(id)
            .map(|row| (true, row))
            .or_else(|| self.linked.get(id).map(|row| (false, row)))
    }
    fn remember(&self, before: &mut Before, id: &str) {
        before
            .entry(Arc::from(id))
            .or_insert_with(|| self.record(id).map(|(inside, row)| (inside, row.clone())));
    }
    /// What the folder `id` holds: its folders by name, then its files, the newest first.
    pub fn children(&self, id: &str) -> Vec<&Row> {
        self.held(id)
            .iter()
            .map(|place| &self.rows[*place as usize])
            .collect()
    }
    /// How many files and folders the folder `id` holds.
    pub fn count(&self, id: &str) -> usize {
        self.held(id).len()
    }
    /// The folders from the main folder down to `id`, the main folder itself left out.
    pub fn path(&self, id: &str) -> Vec<&Row> {
        let mut path = Vec::new();
        let mut at = id;
        while let Some(row) = self.get(at) {
            path.push(row);
            match row.parent.as_deref() {
                Some(up) if path.len() < DEPTH => at = up,
                _ => break,
            }
        }
        path.reverse();
        path
    }
    /// Everything inside the folder `id` whose name holds each of the words of `query`, in
    /// any case: folders by name, then files, the newest first.
    pub fn search(&self, id: &str, query: &str) -> Vec<&Row> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut found: Vec<&Row> = self
            .rows
            .iter()
            .filter(|row| {
                let name = row.shown().name.to_lowercase();
                words.iter().all(|word| name.contains(word.as_str()))
                    && (id == &*self.root
                        || self
                            .path(&row.id)
                            .iter()
                            .any(|up| &*up.id == id && up.id != row.id))
            })
            .collect();
        found.sort_by(|a, b| order(a, b));
        found
    }

    /// Puts in confirmed changes, rebuilding the tree once: a file Google moved out of the
    /// main folder goes. The last observation
    /// of an ID wins, including IDs first seen in this batch.
    pub fn put_many(&mut self, items: impl IntoIterator<Item = Item>) {
        let count = self.rows.len();
        let mut before = Before::new();
        for item in items {
            self.remember(&mut before, &item.id);
            let row = Row::from(item);
            match self.places.get(&row.id) {
                Some(place) => self.rows[*place as usize] = row,
                None => {
                    self.linked.remove(&row.id);
                    self.places
                        .insert(Arc::clone(&row.id), self.rows.len() as u32);
                    self.rows.push(row);
                }
            }
        }
        if !before.is_empty() {
            self.regrow(before, count);
        }
    }
    /// Takes out the file `id`, and anything inside it.
    pub fn remove(&mut self, id: &str) {
        if let Some(place) = self.places.get(id).copied() {
            let count = self.rows.len();
            let mut before = Before::new();
            self.remember(&mut before, id);
            self.rows.remove(place as usize);
            self.regrow(before, count);
        }
    }
    /// Brings the tree up to date with what changed in Drive. A file Dispatch changed itself
    /// keeps its change when the feed describes it from before. Answers whether a folder
    /// came in from elsewhere in the account, whose files the tree doesn't know: then the
    /// Drive is listed whole again.
    pub fn follow(&mut self, changes: Vec<Changed>) -> bool {
        if changes.is_empty() {
            return false;
        }
        let count = self.rows.len();
        let mut before = Before::new();
        let mut arrived = Vec::new();
        let mut gone = HashSet::new();
        for change in changes {
            match change {
                Changed::Kept(item) => {
                    self.remember(&mut before, &item.id);
                    match self.places.get(item.id.as_str()) {
                        Some(place) => {
                            let row = &mut self.rows[*place as usize];
                            if *row.modified <= *item.modified_time {
                                *row = Row::from(*item);
                            }
                        }
                        None => {
                            if item.mime_type == FOLDER {
                                arrived.push(item.id.clone());
                            }
                            self.linked.remove(item.id.as_str());
                            self.rows.push(Row::from(*item));
                        }
                    }
                }
                Changed::Gone(id) => {
                    self.remember(&mut before, &id);
                    gone.insert(id);
                }
            }
        }
        self.rows.retain(|row| !gone.contains(&*row.id));
        self.linked.retain(|id, _| !gone.contains(&**id));
        self.regrow(before, count);
        // A folder that stays outside, such as another DSP's in the same account, brings
        // nothing in.
        arrived
            .iter()
            .any(|id| self.places.contains_key(id.as_str()))
    }
    fn regrow(&mut self, before: Before, count: usize) {
        let mutation = self.mutation;
        let mut rows = mem::take(&mut self.rows);
        rows.extend(mem::take(&mut self.linked).into_values());
        *self = Self::grown(Arc::clone(&self.root), rows);
        // Parent edges and target IDs determine paths, counts, membership and resolution.
        // Any derived change has a touched record whose metadata or membership changed.
        // Comparing final canonical records also handles duplicate IDs and restored batches.
        let changed = count != self.rows.len()
            || before
                .into_iter()
                .any(|(id, before)| match (before, self.record(&id)) {
                    (Some((inside, row)), Some((current, other))) => {
                        inside != current || !row.same_metadata(other)
                    }
                    (None, None) => false,
                    _ => true,
                });
        self.mutation = mutation
            .checked_add(u64::from(changed))
            .expect("Documents mutation sequence exhausted");
    }
}

/// Folders first, by name; then files, the most recently changed first.
fn order(a: &Row, b: &Row) -> std::cmp::Ordering {
    let (shown_a, shown_b) = (a.shown(), b.shown());
    b.folder()
        .cmp(&a.folder())
        .then_with(|| {
            if a.folder() {
                shown_a
                    .name
                    .to_lowercase()
                    .cmp(&shown_b.name.to_lowercase())
            } else {
                shown_b.modified.cmp(&shown_a.modified)
            }
        })
        .then_with(|| a.id.cmp(&b.id))
}

#[cfg(test)]
#[path = "../tests/backend/tree.rs"]
pub(crate) mod tests;
