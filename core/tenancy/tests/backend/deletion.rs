//! A delete cut short once the DSP's rows are gone holds nothing up: the server starts again
//! without the DSP, and its first upkeep finishes what the delete left. One stopped before
//! leaves the DSP disabled, to delete again. A delete leaves what another DSP holds of the
//! people they shared, or who once belonged there; and takes the mail development captured for
//! the DSP, with none captured after, as no capture outlives its message.
use super::*;
use crate::{db::now, foundation::crypto, testing};
use rusqlite::params;

fn dsp_named(db: &Store, name: &str) -> String {
    let (id,): (String,) = db
        .platform
        .one_as("SELECT id FROM dsps WHERE name=?", [name])
        .unwrap()
        .unwrap();
    id
}

#[tokio::test]
async fn a_delete_cut_short_is_finished_once_the_server_starts_again() {
    testing::install(&[], &[]);
    let (_root, db) = testing::seeded();
    let id = dsp_named(&db, "Northline Logistics");
    let owner = testing::platform_owner(&db);
    testing::set_dsp_status(&db, &id, "suspended").unwrap();
    db.jobs
        .exec(
            "INSERT INTO live_days(reader,dsp_id,day,started_at) VALUES ('r',?,'2026-11-05',0)",
            [&id],
        )
        .unwrap();
    // The server stops once the DSP's rows are gone, before anything else of it is.
    db.delete_dsp(&id, "Northline Logistics", &owner).unwrap();
    let folder = db.dsp_folder(&id).unwrap();
    assert!(folder.is_dir());
    let config = db.config.clone();
    drop(db);

    let state = crate::State::new(config).unwrap();
    finish(&state).await;
    // Nothing of its folder is left among the DSPs'.
    let dsps = std::fs::read_dir(folder.parent().unwrap()).unwrap();
    let names: Vec<String> = dsps
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().all(|name| !name.contains(&id)), "{names:?}");
    let left = state
        .read(move |db| {
            Ok((
                db.platform
                    .count("SELECT count(*) FROM dsp_deletions", [])?,
                db.jobs
                    .count("SELECT count(*) FROM live_days WHERE dsp_id=?", [&id])?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(left, (0, 0));
}

#[tokio::test]
async fn a_delete_stopped_before_its_rows_go_leaves_the_dsp_disabled_to_delete_again() {
    testing::install(&[], &[]);
    let (_root, db) = testing::seeded();
    let id = dsp_named(&db, "Northline Logistics");
    let owner = testing::platform_owner(&db);
    testing::set_dsp_status(&db, &id, "suspended").unwrap();
    let state = crate::State::new(db.config.clone()).unwrap();
    // The owner's verification lapses once the delete has begun.
    let turns = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let by = owner.clone();
    let lapsing = move |_: &Store| {
        let turn = turns.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        ensure(turn == 0, "reauthentication_required", 403)?;
        Ok(("Northline Logistics".to_owned(), by.clone()))
    };
    let stopped = delete(&state, &id, lapsing).await;
    assert_eq!(stopped.unwrap_err().code, "reauthentication_required");
    assert_eq!(db.find_dsp(&id).unwrap().status, DspStatus::Suspended);
    let allowed = move |_: &Store| Ok(("Northline Logistics".to_owned(), owner.clone()));
    delete(&state, &id, allowed).await.unwrap();
    assert_eq!(db.find_dsp(&id).unwrap_err().code, "dsp_not_found");
}

#[test]
fn a_deleted_dsp_takes_nothing_another_dsp_holds_of_people_it_shared_or_had() {
    testing::install(&[], &[]);
    let (_root, db) = testing::seeded();
    let (north, summit) = (
        dsp_named(&db, "Northline Logistics"),
        dsp_named(&db, "Summit Delivery"),
    );
    let owner = testing::platform_owner(&db);
    let platform = |sql: &str, values: &[&str]| {
        db.platform
            .exec(sql, rusqlite::params_from_iter(values))
            .unwrap();
    };
    let user = "INSERT INTO users(id,email,first_name,last_name,password,created_at) \
        VALUES (?1,?1||'@dispatch.test','A','B','x','')";
    // In the platform's directory: one person of both DSPs, moved into each's own with one
    // ID; one now Northline's alone who invited someone to Summit, and one who acted there,
    // before leaving it; and one who was ever Northline's alone.
    for person in ["usr_shared", "usr_sender", "usr_former", "usr_legacy"] {
        platform(user, &[person]);
        platform(
            "INSERT INTO memberships(id,user_id,dsp_id,role) VALUES (?1||?2,?1,?2,'member')",
            &[person, &north],
        );
    }
    platform(
        "INSERT INTO memberships(id,user_id,dsp_id,role) VALUES ('mem_s','usr_shared',?,'member')",
        &[&summit],
    );
    for (hash, dsp) in [("north", &north), ("summit", &summit)] {
        platform(
            "INSERT INTO invitations(hash,dsp_id,email,role,expires_at,created_by) \
             VALUES (?,?,'new@dispatch.test','member',0,'usr_sender')",
            &[hash, dsp],
        );
    }
    for dsp in [&north, &summit] {
        db.dsp(dsp).unwrap().exec(user, ["usr_shared"]).unwrap();
    }
    // One of Northline's own directory alone.
    db.dsp(&north).unwrap().exec(user, ["usr_alone"]).unwrap();
    let event = "INSERT INTO audit(at,actor_id,member_id,dsp_id,action,detail) \
        VALUES ('',?,?,?,'case',?)";
    let (north_id, summit_id) = (Some(north.as_str()), Some(summit.as_str()));
    let events = [
        (None, Some("usr_shared"), north_id, "shared at Northline"),
        (None, Some("usr_shared"), summit_id, "shared at Summit"),
        (Some("usr_former"), None, summit_id, "former at Summit"),
        (None, Some("usr_shared"), None, "shared, of their account"),
        (None, Some("usr_alone"), None, "alone, of their account"),
        (Some("usr_legacy"), None, None, "legacy, of their account"),
        (None, Some("usr_alone"), north_id, "alone at Northline"),
    ];
    for (actor, member, dsp, detail) in events {
        db.platform
            .exec(event, rusqlite::params![actor, member, dsp, detail])
            .unwrap();
    }

    testing::set_dsp_status(&db, &north, "suspended").unwrap();
    db.delete_dsp(&north, "Northline Logistics", &owner)
        .unwrap();
    let left = |sql: &str| -> Vec<String> {
        let rows: Vec<(String,)> = db.platform.query_as(sql, []).unwrap();
        rows.into_iter().map(|(value,)| value).collect()
    };
    assert_eq!(
        left("SELECT detail FROM audit WHERE action='case' ORDER BY id"),
        [
            "shared at Summit",
            "former at Summit",
            "shared, of their account"
        ]
    );
    assert_eq!(
        left("SELECT hash FROM invitations WHERE hash IN ('north','summit')"),
        ["summit"]
    );
    // Accounts another DSP's rows name stay; the one no DSP's does goes.
    assert_eq!(
        left(
            "SELECT id FROM users WHERE id IN ('usr_shared','usr_sender','usr_former',\
             'usr_legacy') ORDER BY id"
        ),
        ["usr_former", "usr_sender", "usr_shared"]
    );
    let theirs = db.dsp(&summit).unwrap();
    assert_eq!(
        theirs
            .count("SELECT count(*) FROM users WHERE id='usr_shared'", [])
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn a_deleted_dsps_captured_mail_goes_and_none_is_after_nor_outlives_its_message() {
    testing::install(&[], &[]);
    let (_root, db) = testing::seeded();
    let (north, summit) = (
        dsp_named(&db, "Northline Logistics"),
        dsp_named(&db, "Summit Delivery"),
    );
    let owner = testing::platform_owner(&db);
    // Mail picked to send: Northline's, Summit's and the platform's own captured, and another
    // of Northline's still on its way as Northline is deleted; and one of Northline's sent over
    // a month ago, captured by an earlier release, named by its ID alone as now.
    let mail = [
        ("mail_north", Some(&north), now()),
        ("mail_late", Some(&north), now()),
        ("mail_summit", Some(&summit), now()),
        ("mail_platform", None, now()),
        ("mail_north_sent", Some(&north), now() - 31 * 86_400_000),
    ];
    let mut picked = Vec::new();
    for (id, dsp, created) in mail {
        let message = crypto::encrypt(&db.key, id, &json!({"to": "member@dispatch.test"}));
        db.platform
            .exec(
                "INSERT INTO outbox(id,encrypted_message,available_at,created_at,dsp_id) \
                 VALUES (?1,?2,?3,?3,?4)",
                params![id, message.unwrap(), created, dsp],
            )
            .unwrap();
        let row = db.platform.one(
            "SELECT id,encrypted_message,attempts FROM outbox WHERE id=?",
            [id],
        );
        picked.push(row.unwrap().unwrap());
    }
    let late = picked.remove(1);
    let captures = db.config.platform().join("development-mail");
    for row in &picked {
        let id = crate::db::s(row, "id");
        if id == "mail_north_sent" {
            std::fs::write(captures.join(format!("{id}.json")), "{}").unwrap();
        } else {
            mail::capture(&db, row).unwrap().unwrap().unwrap();
        }
    }
    // The month-old message was sent, and the outbox lets it go before Northline is deleted.
    db.platform
        .exec(
            "UPDATE outbox SET status='sent' WHERE id='mail_north_sent'",
            [],
        )
        .unwrap();
    mail::discard_stale(&db).unwrap();

    testing::set_dsp_status(&db, &north, "suspended").unwrap();
    db.delete_dsp(&north, "Northline Logistics", &owner)
        .unwrap();
    let state = crate::State::new(db.config.clone()).unwrap();
    finish(&state).await;
    assert!(mail::capture(&db, &late).unwrap().is_none());
    let captured = std::fs::read_dir(captures).unwrap();
    let mut captured: Vec<String> = captured
        .map(|file| file.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    captured.sort();
    assert_eq!(captured, ["mail_platform.json", "mail_summit.json"]);
}
