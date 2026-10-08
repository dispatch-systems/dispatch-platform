//! A DSP's Documents as a tree: the files Dispatch can reach in the account that sit inside
//! the DSP's main folder, however deep. Whatever else the account holds, such as another DSP's
//! folder made with the same account, is not in it, so nothing outside it can be named.
use super::drive::Item;
use std::collections::BTreeMap;

/// How deep a folder may sit: deeper than anyone nests, and a bound on a parent loop.
const DEPTH: usize = 64;

pub struct Tree {
    root: String,
    items: BTreeMap<String, Item>,
}
impl Tree {
    /// The files of `all` inside the folder `root`.
    pub fn new(root: &str, all: Vec<Item>) -> Self {
        let parent: BTreeMap<String, String> = all
            .iter()
            .filter_map(|item| Some((item.id.clone(), item.parents.first()?.clone())))
            .collect();
        let inside = |id: &str| {
            let mut at = id;
            for _ in 0..DEPTH {
                match parent.get(at) {
                    Some(up) if up == root => return true,
                    Some(up) => at = up,
                    None => return false,
                }
            }
            false
        };
        let items = all
            .into_iter()
            .filter(|item| item.id != root && inside(&item.id))
            .map(|item| (item.id.clone(), item))
            .collect();
        Self {
            root: root.to_owned(),
            items,
        }
    }
    pub fn root(&self) -> &str {
        &self.root
    }
    /// The file `id`, if it is inside.
    pub fn get(&self, id: &str) -> Option<&Item> {
        self.items.get(id)
    }
    /// Whether `id` is the main folder or a folder inside it.
    pub fn is_folder(&self, id: &str) -> bool {
        id == self.root || self.get(id).is_some_and(Item::folder)
    }
    /// What the folder `id` holds: its folders by name, then its files, the newest first.
    pub fn children(&self, id: &str) -> Vec<&Item> {
        let mut children: Vec<_> = self
            .items
            .values()
            .filter(|item| item.parents.first().is_some_and(|parent| parent == id))
            .collect();
        sort(&mut children);
        children
    }
    /// The folders from the main folder down to `id`, the main folder itself left out.
    pub fn path(&self, id: &str) -> Vec<&Item> {
        let mut path = Vec::new();
        let mut at = id;
        while let Some(item) = self.get(at) {
            path.push(item);
            match item.parents.first() {
                Some(up) if path.len() < DEPTH => at = up,
                _ => break,
            }
        }
        path.reverse();
        path
    }
    /// Everything inside the folder `id` whose name holds each of the words of `query`, in
    /// any case: folders by name, then files, the newest first.
    pub fn search(&self, id: &str, query: &str) -> Vec<&Item> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut found: Vec<_> = self
            .items
            .values()
            .filter(|item| {
                let name = item.name.to_lowercase();
                words.iter().all(|word| name.contains(word.as_str()))
                    && (id == self.root
                        || self
                            .path(&item.id)
                            .iter()
                            .any(|up| up.id == id && up.id != item.id))
            })
            .collect();
        sort(&mut found);
        found
    }
}

/// Folders first, by name; then files, the most recently changed first.
fn sort(items: &mut [&Item]) {
    items.sort_by(|a, b| {
        b.folder().cmp(&a.folder()).then_with(|| {
            if a.folder() {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            } else {
                b.modified_time.cmp(&a.modified_time)
            }
        })
    });
}

#[cfg(test)]
#[path = "../tests/backend/tree.rs"]
mod tests;
