//! A delete cut short once the DSP's rows are gone holds nothing up: the server starts again
//! without the DSP, and its first upkeep finishes what the delete left. And a delete leaves
//! what another DSP holds of the people they shared, or who once belonged there.
use super::*;
use crate::testing;

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
