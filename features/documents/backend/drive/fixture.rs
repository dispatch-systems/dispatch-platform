//! Account-scoped in-memory Drive state shared by every connected DSP.
use super::{Capabilities, DOC, Item, Person, Refused, SHEET, Share, link, missing};
use crate::backend::google::FOLDER;
use dispatch_core::{Error, Result, db::iso, foundation::crypto};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{LazyLock, Mutex},
    time::Instant,
};

/// One account's Drive, in fixture mode.
pub(in crate::backend) struct Drive {
    pub(super) account: String,
    pub(super) files: BTreeMap<String, Item>,
    /// Who each folder is shared with, beside the account.
    pub(super) shares: BTreeMap<String, Vec<Share>>,
    /// Uploads started, by where their bytes go: the name, type and folder each was given.
    pub(super) uploads: BTreeMap<String, (String, String, String)>,
    /// What was uploaded, by file.
    pub(super) contents: BTreeMap<String, Vec<u8>>,
    /// The files someone made directly in Drive, which Dispatch can't reach until someone
    /// picks them.
    pub(super) hidden: BTreeSet<String>,
    /// How many changes the Drive has had, and which of them each file changed in last, as
    /// Google's change feed counts them.
    pub(super) changes: u64,
    pub(super) changed: BTreeMap<String, u64>,
    #[cfg(test)]
    pub(in crate::backend) requests: usize,
    #[cfg(test)]
    pub(in crate::backend) holds:
        BTreeMap<usize, std::pin::Pin<Box<dyn Future<Output = ()> + Send>>>,
}
impl Drive {
    pub(in crate::backend) fn create(
        &mut self,
        id: String,
        name: &str,
        mime: &str,
        parent: Option<&str>,
    ) -> Result<Item> {
        let account = self.account.clone();
        let item = Item {
            web_view_link: link(mime, &id),
            id: id.clone(),
            name: name.to_owned(),
            mime_type: mime.to_owned(),
            parents: parent.map(str::to_owned).into_iter().collect(),
            modified_time: iso(),
            last_modifying_user: Some(Person {
                display_name: Some(account.clone()),
                email_address: Some(account),
            }),
            size: None,
            thumbnail_link: None,
            thumbnail_version: None,
            owned_by_me: true,
            shortcut_details: None,
            capabilities: Capabilities { can_share: true },
            seen: Instant::now(),
        };
        self.files.insert(id.clone(), item.clone());
        self.touch(&id);
        Ok(item)
    }
    pub(in crate::backend) fn shares(&self, id: &str) -> Vec<Share> {
        let mut shares = vec![Share {
            id: "owner".into(),
            role: "owner".into(),
            kind: "user".into(),
            email_address: Some(self.account.clone()),
        }];
        shares.extend(self.shares.get(id).cloned().unwrap_or_default());
        shares
    }
    pub(super) fn share(
        &mut self,
        id: &str,
        email: &str,
    ) -> Result<std::result::Result<String, Refused>> {
        // Fixture mode's Google knows no account at example.net.
        if email.ends_with("@example.net") {
            return Ok(Err(Refused::NoGoogleAccount));
        }
        if self
            .files
            .get(id)
            .is_some_and(|file| !file.capabilities.can_share)
        {
            return Ok(Err(Refused::Other("insufficientFilePermissions".into())));
        }
        let share = Share {
            id: format!("fixture-share-{}", crypto::hex(&crypto::random::<6>()?)),
            role: "writer".into(),
            kind: "user".into(),
            email_address: Some(email.to_owned()),
        };
        let shares = self.shares.entry(id.to_owned()).or_default();
        shares.push(share.clone());
        Ok(Ok(share.id))
    }
    pub(super) fn finish_upload(
        &mut self,
        session: &str,
        length: u64,
        bytes: Vec<u8>,
    ) -> Result<Item> {
        let (name, mime, parent) = self.uploads.remove(session).ok_or_else(missing)?;
        let account = self.account.clone();
        let id = session.replace("fixture-upload-", "fixture-");
        // Google pictures an image as itself; fixture mode, only images.
        let pictured = mime.starts_with("image/");
        let item = Item {
            web_view_link: link(&mime, &id),
            thumbnail_link: pictured.then(|| format!("fixture:{id}")),
            thumbnail_version: pictured.then(|| "1".to_owned()),
            id: id.clone(),
            name,
            mime_type: mime,
            parents: vec![parent],
            modified_time: iso(),
            last_modifying_user: Some(Person {
                display_name: Some(account.clone()),
                email_address: Some(account),
            }),
            size: Some(length.to_string()),
            owned_by_me: true,
            shortcut_details: None,
            capabilities: Capabilities { can_share: true },
            seen: Instant::now(),
        };
        self.files.insert(id.clone(), item.clone());
        self.contents.insert(id.clone(), bytes);
        self.touch(&id);
        Ok(item)
    }
    /// The files Dispatch can reach.
    pub(in crate::backend) fn values(&self) -> impl Iterator<Item = &Item> {
        self.files
            .values()
            .filter(|item| !self.hidden.contains(&item.id))
    }
    /// Notes a change to the file `id`, for the change feed.
    pub(super) fn touch(&mut self, id: &str) {
        self.changes += 1;
        self.changed.insert(id.to_owned(), self.changes);
    }
}
/// A file as fixture mode's Google describes it now.
pub(super) fn described(item: &Item) -> Item {
    Item {
        seen: Instant::now(),
        ..item.clone()
    }
}
static DRIVES: LazyLock<Mutex<BTreeMap<String, Drive>>> = LazyLock::new(Default::default);
/// Runs `work` on the Drive of the account fixture mode's access token names.
pub(in crate::backend) fn fixture<T>(
    access: &str,
    work: impl FnOnce(&mut Drive) -> Result<T>,
) -> Result<T> {
    let account = access
        .strip_prefix("fixture:")
        .ok_or_else(|| Error::new("google_connection_broken", 409))?;
    let mut drives = DRIVES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let drive = drives.entry(account.to_owned()).or_insert_with(|| Drive {
        account: account.to_owned(),
        files: BTreeMap::new(),
        shares: BTreeMap::new(),
        uploads: BTreeMap::new(),
        contents: BTreeMap::new(),
        hidden: BTreeSet::new(),
        changes: 0,
        changed: BTreeMap::new(),
        #[cfg(test)]
        requests: 0,
        #[cfg(test)]
        holds: BTreeMap::new(),
    });
    work(drive)
}
impl crate::backend::google::Connected {
    /// The in-process transport uses the same post-wait admission boundary as HTTP.
    pub(in crate::backend) async fn fixture<T>(
        &self,
        access: &str,
        work: impl FnOnce(&mut Drive) -> Result<T>,
    ) -> Result<T> {
        #[cfg(test)]
        {
            let hold = fixture(access, |drive| {
                Ok(drive.holds.remove(&(drive.requests + 1)))
            })?;
            if let Some(hold) = hold {
                hold.await;
            }
        }
        self.admission.check().await?;
        fixture(access, |drive| {
            #[cfg(test)]
            {
                drive.requests += 1;
            }
            work(drive)
        })
    }
}

/// Puts files in fixture mode's Drive elsewhere than the main folder, as its account holds
/// them: a picture someone made directly in its Drive, and two Sheets someone else owns and
/// shared with the account, one to view only. Each comes into Documents once it's picked.
pub fn fixture_elsewhere(access: &str) -> Result<()> {
    fixture(access, |drive| {
        for (name, mime, size) in [
            ("Route map 2025.png", "image/png", Some("231004")),
            (SHARED_SHEET, SHEET, None),
            (VIEW_ONLY_SHEET, SHEET, None),
        ] {
            let item = made_in_drive(name, mime, size, None)?;
            drive.hidden.insert(item.id.clone());
            drive.files.insert(item.id.clone(), item);
        }
        Ok(())
    })
}
/// Puts files in the main folder `folder` as the team makes them directly in Drive in time: a
/// folder, a Doc in it and a PDF. Documents shows them as it reads the folder next. Fixture
/// mode does so once, as the DSP reconnects to the folder it kept.
pub fn fixture_made_in_folder(access: &str, folder: &str) -> Result<()> {
    let safety = made_in_drive("Safety", FOLDER, None, Some(folder))?;
    let huddle = made_in_drive("Weekly safety huddle", DOC, None, Some(&safety.id))?;
    let receipts = made_in_drive(
        "Fuel receipts.pdf",
        "application/pdf",
        Some("48213"),
        Some(folder),
    )?;
    fixture(access, |drive| {
        let made = drive.files.values().any(|file| {
            file.name == receipts.name && file.parents.iter().any(|parent| parent == folder)
        });
        if !made {
            for item in [safety, huddle, receipts] {
                drive.touch(&item.id);
                drive.files.insert(item.id.clone(), item);
            }
        }
        Ok(())
    })
}
/// A file someone made directly in fixture mode's Drive, in `parent`, or elsewhere.
fn made_in_drive(name: &str, mime: &str, size: Option<&str>, parent: Option<&str>) -> Result<Item> {
    let id = format!("fixture-{}", crypto::hex(&crypto::random::<8>()?));
    Ok(Item {
        web_view_link: link(mime, &id),
        id,
        name: name.to_owned(),
        mime_type: mime.to_owned(),
        parents: parent.map(str::to_owned).into_iter().collect(),
        modified_time: iso(),
        last_modifying_user: Some(Person {
            display_name: Some("Keisha Brown".to_owned()),
            email_address: Some("keisha.brown@example.com".to_owned()),
        }),
        size: size.map(str::to_owned),
        thumbnail_link: None,
        thumbnail_version: None,
        // Google shows a file shared with the account in no folder of the account's.
        owned_by_me: name != SHARED_SHEET && name != VIEW_ONLY_SHEET,
        shortcut_details: None,
        capabilities: Capabilities {
            can_share: name != VIEW_ONLY_SHEET,
        },
        seen: Instant::now(),
    })
}
/// The Sheet someone else owns in fixture mode, shared with the account.
pub const SHARED_SHEET: &str = "Route tracker";
/// One shared with the account to view only, which it can't share on.
pub const VIEW_ONLY_SHEET: &str = "Fuel prices";
/// The files fixture mode's Drive holds that Dispatch can't reach yet.
pub fn fixture_hidden(access: &str) -> Result<Vec<Item>> {
    fixture(access, |drive| {
        Ok(drive
            .hidden
            .iter()
            .filter_map(|id| drive.files.get(id).cloned())
            .collect())
    })
}
/// Whether fixture mode's Drive has the file, out of the trash.
pub fn fixture_has(access: &str, id: &str) -> Result<bool> {
    fixture(access, |drive| Ok(drive.files.contains_key(id)))
}

/// What a test has happen once fixture mode's Google next shares the file or folder `id`, by
/// ID, or takes a share of it back: as someone disabling the DSP between two shares.
#[cfg(test)]
static ON_SHARING: LazyLock<Mutex<BTreeMap<String, OnSharing>>> = LazyLock::new(Default::default);
#[cfg(test)]
type OnSharing = Box<dyn FnOnce() + Send>;
#[cfg(test)]
pub fn fixture_on_sharing(id: &str, then: impl FnOnce() + Send + 'static) {
    ON_SHARING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(id.to_owned(), Box::new(then));
}
#[cfg(test)]
pub(super) fn fixture_sharing_changed(id: &str) {
    let then = ON_SHARING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(id);
    if let Some(then) = then {
        then();
    }
}
