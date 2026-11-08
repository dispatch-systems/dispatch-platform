//! What a DSP's backup can hold, as the owner chooses it: the DSP's own data, always; each
//! collector's database and each feature's added databases, by the registry, under their own
//! names; the DSP's saved sign-ins; and its pictures. Each file of the DSP's folder belongs to one
//! of them, and `cache`, what its owners collect again, to none.
use crate::{Result, collection::registry::Provider, manifest::registry, tenancy::catalog};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

/// The DSP's own data: its database, settings and the state its features keep. Always held.
pub const DISPATCH: &str = "dispatch";
/// Its saved sign-ins: the credentials of its connections, sealed beside the key they're
/// sealed with. The browsers' signed-in profiles aren't held: a browser in use can't be copied
/// whole, and they sign in again with these.
pub const LOGINS: &str = "logins";
/// The DSP's own database, which every backup holds.
pub const DSP_DATABASE: &str = "data/dispatch.sqlite";
/// The files its features keep that aren't databases, and those kept in the object store.
pub const PICTURES: &str = "pictures";

/// One thing a backup can hold, as the owner sees it.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub label: String,
}

/// Which item each folder of `data/` belongs to, by the registry: a collector's database is
/// `collector.<id>`, a feature's added database `feature.<name>`.
fn owners() -> BTreeMap<&'static str, (String, String)> {
    let mut owners = BTreeMap::new();
    for provider in Provider::all() {
        owners.insert(
            provider.id(),
            (
                format!("collector.{}", provider.id()),
                provider.collector().label().to_owned(),
            ),
        );
    }
    for feature in registry().features {
        let label = catalog::find(feature.name)
            .map(|found| found.label)
            .unwrap_or(feature.name);
        for keeper in feature.keeps {
            for storage in keeper.storages() {
                owners
                    .entry(storage.id)
                    .or_insert_with(|| (format!("feature.{}", feature.name), label.to_owned()));
            }
        }
    }
    owners
}

/// What a DSP's backup can hold, for the DSP whose folder is `folder`: its own data first,
/// then the databases it has, in the registry's order, then its sign-ins and pictures.
pub fn items(folder: &Path) -> Vec<Item> {
    let data = folder.join("data");
    let mut items = vec![Item {
        id: DISPATCH.into(),
        label: "Dispatch".into(),
    }];
    let mut seen = Vec::new();
    for (folder, (id, label)) in owners() {
        if data.join(folder).is_dir() && !seen.contains(&id) {
            seen.push(id.clone());
            items.push(Item { id, label });
        }
    }
    items.push(Item {
        id: LOGINS.into(),
        label: "Saved logins".into(),
    });
    items.push(Item {
        id: PICTURES.into(),
        label: "Pictures".into(),
    });
    items
}

/// A file of the DSP's folder a backup holds: where it is in the folder, the item it belongs
/// to, and whether it's a database, which is copied as SQLite copies one while it's in use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub item: String,
    pub database: bool,
}

/// Every file of the DSP's folder `folder` that one of `held` holds, by its path in the folder:
/// files alone, never a link or a file linked from elsewhere, and none of SQLite's own beside a
/// database, which its copy holds already.
pub fn files(folder: &Path, held: &[String]) -> Result<Vec<Found>> {
    let owners = owners();
    let mut found = Vec::new();
    for area in ["config", "data", "secrets", "state"] {
        walk(folder, Path::new(area), &mut |path| {
            let item = item_of(&owners, path);
            if held.contains(&item) {
                found.push(Found {
                    database: path.extension().is_some_and(|e| e == "sqlite"),
                    path: path.to_owned(),
                    item,
                });
            }
        })?;
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

fn item_of(owners: &BTreeMap<&'static str, (String, String)>, path: &Path) -> String {
    let parts: Vec<&str> = path.iter().filter_map(|part| part.to_str()).collect();
    match parts.as_slice() {
        ["secrets", ..] => LOGINS.into(),
        ["data", folder, _, ..] => owners
            .get(folder)
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| PICTURES.into()),
        _ => DISPATCH.into(),
    }
}

/// Each file under `relative`, but the browsers' profiles: a folder that isn't there holds none,
/// and one that can't be read fails the backup rather than leave it out.
fn walk(folder: &Path, relative: &Path, each: &mut impl FnMut(&Path)) -> Result<()> {
    if relative == Path::new("state/browsers") {
        return Ok(());
    }
    let entries = match fs::read_dir(folder.join(relative)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let path = relative.join(name);
        let stat = fs::symlink_metadata(entry.path())?;
        if stat.is_dir() {
            walk(folder, &path, each)?;
        } else if stat.is_file()
            && stat.nlink() == 1
            && !name.ends_with("-wal")
            && !name.ends_with("-shm")
            && !name.ends_with("-journal")
            && !name.ends_with(".lock")
        {
            each(&path);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/backend/contents.rs"]
mod tests;
