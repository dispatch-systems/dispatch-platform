//! The phones' tables as Communications adopts them: a DSP's database Phones migrated, with a
//! phone paired, a text Dispatch queued for it and the picture it carries, upgrades with nothing
//! lost. That the runner leaves Phones' own records of its migrations as they were is core's to
//! prove.
use super::*;
use crate::{
    api::types::PairingStatus,
    backend::phone::support::{CODE, kept, outgoing, paired, store},
};
use dispatch_core::{
    db::migrations::{Apply, Migration},
    foundation::crypto,
    testing,
};

/// Communications' migration `id`, as its manifest declares it.
fn migration(id: u32) -> &'static Migration {
    crate::FEATURE.own_migrations[0]
        .list
        .iter()
        .find(|migration| migration.id == id)
        .unwrap()
}

#[test]
fn a_database_phones_migrated_upgrades_with_its_phone_its_waiting_text_and_its_picture_working() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    // As Phones left it, its tables made as Communications' are: a phone paired, and a text
    // Dispatch queued for it with a picture, its bytes kept.
    let key = crypto::sha("phone-key");
    let phone = paired(&db, &dsp, &key, "2065550100");
    let bytes = b"door";
    let sha256 = crypto::sha(bytes);
    std::fs::write(kept(&db, &dsp).join(&sha256), bytes).unwrap();
    let file = ReportedFile {
        name: "door.png".into(),
        mime: "image/png".into(),
        size: 4,
    };
    let picture = db
        .phones_add_file(&dsp, &owner, &file, &sha256, 3_000)
        .unwrap();
    let waiting = db
        .phones_send(
            &dsp,
            &outgoing("4255550199", vec![picture.id.clone()], &owner),
            3_000,
        )
        .unwrap();

    // The server starts on a build where Communications owns them: its migrations 7 and 8 run
    // over it, as the runner runs what a database hasn't recorded.
    let upgraded = db.dsp(&dsp).unwrap();
    for id in [7, 8] {
        match migration(id).apply {
            Apply::Sql(sql) => upgraded.0.execute_batch(sql).unwrap(),
            Apply::Code(code) => code(&upgraded).unwrap(),
        }
    }
    drop(upgraded);

    // The phone's key still works, its text still waits for it with its picture, and the
    // picture's bytes are still there to fetch.
    let found = db.phones_device_by_key(&dsp, &key).unwrap();
    assert_eq!(found.map(|device| device.id), Some(phone.id.clone()));
    let outbox: Vec<_> = db
        .phones_outbox(&dsp, &phone.id)
        .unwrap()
        .into_iter()
        .map(|(text, _)| {
            (
                text.id,
                text.files.into_iter().map(|file| file.id).collect(),
            )
        })
        .collect();
    assert_eq!(outbox, [(waiting.id, vec![picture.id.clone()])]);
    let fetched = db
        .phones_outgoing_file(&dsp, &phone.id, &picture.id)
        .unwrap()
        .and_then(|file| file.sha256)
        .expect("its picture, kept");
    let read = std::fs::read(kept(&db, &dsp).join(fetched)).unwrap();
    assert_eq!(read, bytes);
    // A code made now pairs by its own ID, as migration 8's columns keep it.
    let made = db
        .phones_new_pairing(&dsp, &owner, &format!("{CODE}-2"), 4_000)
        .unwrap();
    assert_eq!(
        db.phones_pairing(&dsp, &made, 4_000).unwrap(),
        PairingStatus::Waiting
    );
}
