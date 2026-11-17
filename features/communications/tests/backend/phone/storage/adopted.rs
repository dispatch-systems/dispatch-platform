//! The phones' tables as Communications adopts them: a DSP's database with Phones' tables as Dev
//! had them before the move (`schema/phones.sql`), a phone paired, a text Dispatch queued for it
//! and the picture it carries, upgrades with nothing lost. That the runner leaves Phones' own
//! records of its migrations as they were is core's to prove.
use super::*;
use crate::{
    api::types::PairingStatus,
    backend::{
        phone::support::{CODE, kept, store},
        support::migrate,
    },
};
use dispatch_core::{foundation::crypto, testing};

#[test]
fn a_database_phones_migrated_upgrades_with_its_phone_its_waiting_text_and_its_picture_working() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    // Phones' tables as Dev had them, in place of those Communications made.
    let before = db.dsp(&dsp).unwrap();
    before
        .0
        .execute_batch(
            "DROP TABLE phones_files; DROP TABLE phones_texts; DROP TABLE phones_pairings; \
             DROP TABLE phones_devices; DROP TABLE phones_change;",
        )
        .unwrap();
    before
        .0
        .execute_batch(include_str!("schema/phones.sql"))
        .unwrap();
    // As Phones left them: a phone paired, as Phones paired it, and a text Dispatch queued for it
    // with a picture, its bytes kept.
    let key = crypto::sha("phone-key");
    let bytes = b"door";
    let sha256 = crypto::sha(bytes);
    before
        .0
        .execute_batch(&format!(
            "INSERT INTO phones_devices \
             (id,name,number,model,app_version,key_sha256,paired_at,paired_by,last_seen_at) \
             VALUES ('phone_adopted','Dispatch','+12065550100','Pixel 8a','1.0.0','{key}',2000,\
             '{owner}',2000); \
             UPDATE phones_change SET last=1; \
             INSERT INTO phones_texts \
             (id,device_id,direction,origin,number,body,at,sent_by,status,status_at,attempt,change) \
             VALUES ('text_adopted','phone_adopted','out','dispatch','+14255550199','On my way',\
             3000,'{owner}','waiting',3000,1,1); \
             INSERT INTO phones_files (id,text_id,added_by,name,mime,size,sha256,added_at) \
             VALUES ('file_adopted','text_adopted','{owner}','door.png','image/png',4,'{sha256}',\
             3000);"
        ))
        .unwrap();
    drop(before);
    std::fs::write(kept(&db, &dsp).join(&sha256), bytes).unwrap();
    let phone = db.phones_device(&dsp, "phone_adopted").unwrap().unwrap();

    // The server starts on a build where Communications owns them: its migrations 7 and 8 run
    // over it, with the later ones adding columns its code writes (19, 23).
    migrate(&db, &dsp, &[7, 8, 19, 23]);

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
    assert_eq!(
        outbox,
        [("text_adopted".to_owned(), vec!["file_adopted".to_owned()])]
    );
    let fetched = db
        .phones_outgoing_file(&dsp, &phone.id, "file_adopted")
        .unwrap()
        .and_then(|file| file.file.sha256)
        .expect("its picture, kept");
    let read = std::fs::read(kept(&db, &dsp).join(fetched)).unwrap();
    assert_eq!(read, bytes);
    // A code made now pairs by its own ID, as migration 8's columns keep it.
    let made = db
        .phones_new_pairing(&dsp, None, &owner, &format!("{CODE}-2"), 4_000)
        .unwrap();
    assert_eq!(
        db.phones_pairing(&dsp, None, &made, 4_000).unwrap(),
        PairingStatus::Waiting
    );
}
