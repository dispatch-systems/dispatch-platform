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
}
impl Row {
    pub fn folder(&self) -> bool {
        self.mime == Mime::Folder
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
}
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
        let mut place = 0;
        rows.retain(|_| {
            place += 1;
            kept[place - 1]
        });
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
                let name = row.name.to_lowercase();
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

    /// Puts in a file Dispatch made or changed, as Google answered: one Google moved out of
    /// the main folder goes.
    pub fn put(&mut self, item: Item) {
        let row = Row::from(item);
        match self.places.get(&row.id) {
            Some(place) => self.rows[*place as usize] = row,
            None => self.rows.push(row),
        }
        self.regrow();
    }
    /// Takes out the file `id`, and anything inside it.
    pub fn remove(&mut self, id: &str) {
        if let Some(place) = self.places.get(id) {
            self.rows.remove(*place as usize);
            self.regrow();
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
        let mut arrived = Vec::new();
        let mut gone = HashSet::new();
        for change in changes {
            match change {
                Changed::Kept(item) => match self.places.get(item.id.as_str()) {
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
                        self.rows.push(Row::from(*item));
                    }
                },
                Changed::Gone(id) => {
                    gone.insert(id);
                }
            }
        }
        self.rows.retain(|row| !gone.contains(&*row.id));
        self.regrow();
        // A folder that stays outside, such as another DSP's in the same account, brings
        // nothing in.
        arrived
            .iter()
            .any(|id| self.places.contains_key(id.as_str()))
    }
    fn regrow(&mut self) {
        let rows = mem::take(&mut self.rows);
        *self = Self::grown(Arc::clone(&self.root), rows);
    }
}

/// Folders first, by name; then files, the most recently changed first.
fn order(a: &Row, b: &Row) -> std::cmp::Ordering {
    b.folder()
        .cmp(&a.folder())
        .then_with(|| {
            if a.folder() {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            } else {
                b.modified.cmp(&a.modified)
            }
        })
        .then_with(|| a.id.cmp(&b.id))
}

#[cfg(test)]
#[path = "../tests/backend/tree.rs"]
pub(crate) mod tests;
