//! What the phone side's backend tests start from: a platform with one DSP and Communications on,
//! as every Communications test installs it, a phone paired to it, and the texts and pictures they
//! make.
use crate::backend::phone::{
    model::{Direction, Outgoing},
    storage::{Device, DeviceStore, Pairing, Reported, ReportedFile},
};
use dispatch_core::{
    db::{self, Store},
    testing,
};
use std::path::PathBuf;

/// The SHA-256 of the code `paired` pairs a phone by.
pub const CODE: &str = "code-sha";

/// A platform with one DSP, Communications on for it.
pub fn store() -> (impl Drop, Store, String) {
    crate::backend::support::install();
    testing::bootstrapped()
}
/// A phone paired by a code the DSP's owner made, with the key `key`.
pub fn paired(db: &Store, dsp: &str, key: &str, number: &str) -> Device {
    let owner = testing::a_user(db);
    db.phones_new_pairing(dsp, &owner, CODE, 1_000).unwrap();
    let pairing = Pairing {
        number,
        model: "Pixel 8a",
        app_version: "1.0.0",
    };
    db.phones_pair(dsp, CODE, key, &pairing, 2_000).unwrap()
}
/// A text the phone received from `number` at `at`, by its own ID `id`, with `files` photos.
pub fn reported(id: &str, number: &str, at: i64, files: usize) -> Reported {
    Reported {
        client_id: id.into(),
        direction: Direction::In,
        number: number.into(),
        body: format!("text {id}"),
        at,
        files: (0..files)
            .map(|index| ReportedFile {
                name: format!("photo{index}.jpg"),
                mime: "image/jpeg".into(),
                size: 100,
            })
            .collect(),
    }
}
/// A text `by` sends to `to`, from whichever phone it goes from, with the pictures `files`.
pub fn outgoing(to: &str, files: Vec<String>, by: &str) -> Outgoing {
    Outgoing {
        phone: None,
        to: to.into(),
        body: "On my way".into(),
        files,
        by: by.into(),
    }
}
/// Where the DSP's pictures' bytes are kept, made private as the phone side makes it.
pub fn kept(db: &Store, dsp: &str) -> PathBuf {
    let owner = db::private_dir(&db.area(dsp, "data").unwrap().join("phones")).unwrap();
    db::private_dir(&owner.join("files")).unwrap()
}
