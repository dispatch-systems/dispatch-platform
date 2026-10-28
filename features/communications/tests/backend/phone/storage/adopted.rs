//! The phones' tables as Communications adopts them: a DSP's database from before, as Phones left
//! it, with a phone paired, a text Dispatch queued for it and the picture it carries, upgrades with
//! nothing lost, and Phones' records of its own migrations left as they were.
use super::*;
use crate::{
    api::types::PairingStatus,
    backend::phone::support::{CODE, kept, outgoing, paired, store},
};
use dispatch_core::{
    db::{Kind, migrations::migrate},
    foundation::crypto,
    testing,
};

#[test]
fn a_database_phones_migrated_upgrades_with_its_phone_its_waiting_text_and_its_picture_working() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    // As Phones left it: its tables, its own two migrations recorded, Communications' 7 and 8 not.
    {
        let raw = db.dsp(&dsp).unwrap();
        raw.exec(
            "DELETE FROM owner_migrations WHERE owner='communications' AND id IN (7,8)",
            [],
        )
        .unwrap();
        raw.exec(
            "INSERT INTO owner_migrations(owner,id,name,applied_at) \
             VALUES ('phones',1,'phones',1),('phones',2,'pairing_ids',1)",
            [],
        )
        .unwrap();
    }
    // A phone paired, and a text Dispatch queued for it with a picture, its bytes kept.
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

    // The server starts on a build where Communications owns them: its migrations run over it.
    migrate(&db.dsp(&dsp).unwrap(), Kind::DSP).unwrap();

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
    // Communications' 7 and 8 are recorded, beside Phones' own, left as they were.
    let recorded: Vec<(String, i64)> = db
        .dsp(&dsp)
        .unwrap()
        .all(
            "SELECT owner,id FROM owner_migrations WHERE owner='phones' \
             OR (owner='communications' AND id>6) ORDER BY owner,id",
            [],
        )
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["owner"].as_str().unwrap().to_owned(),
                row["id"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        recorded,
        [
            ("communications".to_owned(), 7),
            ("communications".to_owned(), 8),
            ("phones".to_owned(), 1),
            ("phones".to_owned(), 2),
        ]
    );
}
