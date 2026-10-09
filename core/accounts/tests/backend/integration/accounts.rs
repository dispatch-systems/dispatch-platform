use common::{audits, bootstrapped, seeded};
use dispatch_core::testing as common;
use dispatch_core::{
    db::{self, s},
    foundation::crypto,
};
use serde_json::json;

#[tokio::test]
async fn concurrent_password_resets_cannot_reuse_a_consumed_token() {
    common::install(&[], &[]);
    use dispatch_core::State;
    let (_root, db, _) = bootstrapped();
    let user = db
        .platform
        .one("SELECT * FROM users WHERE email='owner@example.test'", [])
        .unwrap()
        .unwrap();
    let token = crypto::token().unwrap();
    db.platform
        .exec(
            "INSERT INTO resets(hash,user_id,user_version,expires_at) VALUES (?,?,?,?)",
            rusqlite::params![
                crypto::sha(&token),
                s(&user, "id"),
                db::n(&user, "version"),
                db::now() + 60000
            ],
        )
        .unwrap();
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let admin = || dispatch_core::foundation::config::Site::Admin;
    let (a, b) = tokio::join!(
        state.reset_password(token.clone(), "first-replacement-password".into(), admin()),
        state.reset_password(token.clone(), "second-replacement-password".into(), admin())
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(a.err().or(b.err()).unwrap().code, "reset_expired");
    assert_eq!(
        state
            .reset_password(token, "third-replacement-password".into(), admin())
            .await
            .unwrap_err()
            .code,
        "reset_expired"
    );
}

#[tokio::test]
async fn removing_a_member_deletes_their_account_and_keeps_their_name_in_the_log() {
    common::install(&[], &[]);
    use dispatch_core::{State, accounts::Auth};
    let (_root, db) = seeded();
    let one = |sql: &str| db.platform.one(sql, []).unwrap();
    let tenant = one("SELECT id FROM dsps WHERE name='Northline Logistics'").unwrap();
    let dsp = s(&tenant, "id");
    // Northline's people are in its own directory.
    let theirs = |sql: &str| db.dsp(dsp).unwrap().one(sql, []).unwrap();
    let owner = one("SELECT id FROM users WHERE platform_owner=1").unwrap();
    let member = theirs("SELECT id FROM users WHERE email='member@dispatch.test'").unwrap();
    let membership = theirs("SELECT id,role_id FROM memberships").unwrap();
    let invite = |email: &str, by: &str| {
        let raw = crypto::token().unwrap();
        db.dsp(dsp).unwrap().exec("INSERT INTO invitations(hash,dsp_id,email,role,role_id,expires_at,created_by) VALUES (?,?,?,'member',?,?,?)",rusqlite::params![crypto::sha(&raw),dsp,email,s(&membership,"role_id"),db::now()+60000,by]).unwrap();
        db.platform
            .exec(
                "INSERT INTO invitation_routes(hash,dsp_id) VALUES (?,?)",
                [crypto::sha(&raw).as_str(), dsp],
            )
            .unwrap();
        raw
    };
    db.audit(Some(s(&member, "id")), Some(dsp), "schedule.updated", "")
        .unwrap();
    db.dsp(dsp)
        .unwrap()
        .exec(
            "INSERT INTO sessions(hash,user_id,user_version,expires_at,created_at) VALUES ('session',?,1,?,?)",
            rusqlite::params![s(&member, "id"), db::now() + 60000, db::now()],
        )
        .unwrap();
    db.dsp(dsp)
        .unwrap()
        .exec(
            "INSERT INTO resets(hash,user_id,user_version,expires_at) VALUES ('reset',?,1,?)",
            rusqlite::params![s(&member, "id"), db::now() + 60000],
        )
        .unwrap();
    invite("colleague@dispatch.test", s(&member, "id"));
    let auth = Auth {
        user: serde_json::from_value(
            json!({"id":owner["id"],"email":"","firstName":"","lastName":"","platformOwner":true}),
        )
        .unwrap(),
        hash: String::new(),
        csrf: String::new(),
        raw: String::new(),
        preview: None,
        site: dispatch_core::foundation::config::Site::Admin,
        scope: None,
    };
    let context = db.context(&auth, dsp, "members.manage").unwrap();
    db.set_role(&context, s(&membership, "id"), None).unwrap();
    for table in [
        "users WHERE email='member@dispatch.test'",
        "sessions",
        "resets",
    ] {
        assert!(
            theirs(&format!("SELECT 1 FROM {table}")).is_none(),
            "{table}"
        );
    }
    assert!(theirs("SELECT created_by FROM invitations").is_none());
    let log = audits(&db, Some(dsp)).unwrap();
    let event = log
        .as_array()
        .unwrap()
        .iter()
        .find(|row| s(row, "action") == "schedule.updated")
        .unwrap();
    assert_eq!(s(event, "actorName"), "Jordan Ellis");
    assert_eq!(event["actorId"], member["id"]);
    let platform = audits(&db, None).unwrap();
    let removal = platform
        .as_array()
        .unwrap()
        .iter()
        .find(|row| s(row, "action") == "member.removed")
        .unwrap();
    assert_eq!(s(removal, "target"), "Jordan Ellis");
    assert_eq!(removal["changes"][0]["field"], "role");
    assert!(removal["changes"][0]["to"].is_null());
    assert!(one("SELECT 1 FROM users WHERE platform_owner=1").is_some());

    let raw = invite("member@dispatch.test", s(&owner, "id"));
    let site = db.invitation_site(dsp).unwrap();
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    state
        .accept_invitation(
            raw,
            dispatch_core::accounts::api::requests::InvitationRequest {
                first_name: "Riley".into(),
                last_name: "Shaw".into(),
                password: "a-brand-new-password".into(),
                dsp_profile: None,
            },
            "127.0.0.1".into(),
            site,
        )
        .await
        .unwrap();
    let north = dsp.to_owned();
    let joined = state
        .read(move |db| {
            db.dsp(&north)?.one(
                "SELECT u.id,u.first_name FROM users u JOIN memberships m ON m.user_id=u.id WHERE u.email='member@dispatch.test'",
                [],
            )
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(s(&joined, "first_name"), "Riley");
    assert_ne!(joined["id"], member["id"]);
}

#[test]
fn a_feature_emails_an_active_member_under_a_kind_of_its_own() {
    common::install(&[], &[]);
    use dispatch_core::server::mail::templates::Message;
    let (_root, db) = seeded();
    let tenant = db
        .platform
        .one("SELECT id FROM dsps WHERE name='Northline Logistics'", [])
        .unwrap()
        .unwrap();
    let dsp = s(&tenant, "id");
    let member = db
        .dsp(dsp)
        .unwrap()
        .one(
            "SELECT id FROM users WHERE email='member@dispatch.test'",
            [],
        )
        .unwrap()
        .unwrap();
    let member = s(&member, "id");
    let mail = Message {
        subject: "Link a Google account".into(),
        text: "Open Documents.".into(),
        html: "<p>Open Documents.</p>".into(),
    };
    // A kind names the feature, then what it sends.
    for kind in [
        "documents",
        "Documents.link",
        "documents.",
        ".link",
        "documents.link-it",
    ] {
        let refused = db.email_member(dsp, member, kind, &mail).unwrap_err();
        assert_eq!(refused.code, "invalid_mail_kind", "{kind}");
    }
    db.email_member(dsp, member, "documents.google_account", &mail)
        .unwrap();
    let queued = db
        .platform
        .one(
            "SELECT user_id FROM outbox WHERE kind='documents.google_account'",
            [],
        )
        .unwrap()
        .unwrap();
    assert_eq!(s(&queued, "user_id"), member);
    db.dsp(dsp)
        .unwrap()
        .exec("UPDATE users SET status='disabled' WHERE id=?", [member])
        .unwrap();
    let refused = db
        .email_member(dsp, member, "documents.google_account", &mail)
        .unwrap_err();
    assert_eq!(refused.code, "user_not_found");
}

#[test]
fn without_a_feature_to_invite_through_only_the_platform_owner_invites() {
    common::install(&[], &[]);
    use dispatch_core::{accounts::Auth, manifest::registry};
    assert_eq!(registry().inviting(), None);
    let (_root, mut db) = seeded();
    db.config.mail_mode = "capture".into();
    let one = |db: &db::Store, sql: &str| db.platform.one(sql, []).unwrap().unwrap();
    let dsp = s(
        &one(&db, "SELECT id FROM dsps WHERE name='Northline Logistics'"),
        "id",
    )
    .to_owned();
    let theirs = |db: &db::Store, sql: &str| db.dsp(&dsp).unwrap().one(sql, []).unwrap().unwrap();
    let role = s(
        &theirs(&db, "SELECT role_id FROM memberships LIMIT 1"),
        "role_id",
    )
    .to_owned();
    let owner = one(&db, "SELECT id FROM users WHERE platform_owner=1");
    let member = theirs(
        &db,
        "SELECT id FROM users WHERE email='member@dispatch.test'",
    );
    let auth = |user: &serde_json::Value, platform_owner: bool, preview: Option<&str>| Auth {
        user: serde_json::from_value(json!({"id":user["id"],"email":"","firstName":"",
            "lastName":"","platformOwner":platform_owner}))
        .unwrap(),
        hash: String::new(),
        csrf: String::new(),
        raw: String::new(),
        preview: preview.map(str::to_owned),
        site: dispatch_core::foundation::config::Site::Admin,
        scope: (!platform_owner).then(|| dsp.clone()),
    };
    let invite = |a: &Auth| db.invite(a, &dsp, "new@dispatch.test", &role);
    // No member may, whatever their role, nor a platform owner looking through one.
    assert_eq!(
        invite(&auth(&member, false, None)).unwrap_err().code,
        "permission_denied"
    );
    assert_eq!(
        invite(&auth(&owner, true, Some(&role))).unwrap_err().code,
        "permission_denied"
    );
    // The platform owner, as themselves, still invites a DSP's people.
    assert!(invite(&auth(&owner, true, None)).is_ok());
}

#[tokio::test]
async fn a_dsps_people_move_into_its_own_directory_once_and_sign_in_there_as_before() {
    common::install(&[], &[]);
    use dispatch_core::{
        State, accounts::SessionLifetime, foundation::config::Site, server::operations,
        tenancy::roles,
    };
    let (_root, db, dsp) = bootstrapped();
    db.set_code(&dsp, "lgcy").unwrap();
    // As the release before kept this DSP: its people in the platform's directory, its own
    // holding none of them.
    let theirs = db.dsp(&dsp).unwrap();
    theirs
        .exec("DELETE FROM settings WHERE key='accounts.directory'", [])
        .unwrap();
    theirs.exec("DELETE FROM roles", []).unwrap();
    drop(theirs);
    roles::seed(&db.platform, &dsp).unwrap();
    let owner = db
        .platform
        .one("SELECT id FROM users WHERE platform_owner=1", [])
        .unwrap()
        .unwrap();
    let member = "usr_00000000000000000000000000000001";
    let password = crypto::hash_password("the-same-old-password").unwrap();
    let platform = |sql: &str, values: &[&dyn rusqlite::ToSql]| {
        db.platform.exec(sql, values).unwrap();
    };
    platform(
        "INSERT INTO users(id,email,first_name,last_name,password,created_at) \
         VALUES (?,'driver@example.test','Dana','Driver',?,'then')",
        &[&member, &password],
    );
    // Written by an older runtime: a legacy role and no role id, which startup resolves.
    platform(
        "INSERT INTO memberships(id,user_id,dsp_id,role) VALUES ('mem_legacy',?,?,'manager')",
        &[&member, &dsp],
    );
    platform(
        "INSERT INTO memberships(id,user_id,dsp_id,role) VALUES ('mem_owner',?,?,'owner')",
        &[&s(&owner, "id"), &dsp],
    );
    platform(
        "INSERT INTO authenticator_apps(user_id,secret,created_at) VALUES (?,'sealed',1)",
        &[&member],
    );
    platform(
        "INSERT INTO recovery_codes(hash,user_id) VALUES ('code',?)",
        &[&member],
    );
    platform(
        "INSERT INTO account_passkeys VALUES ('key',?,'{}','Old key',1)",
        &[&member],
    );
    platform(
        "INSERT INTO sessions VALUES ('session',?,1,?,1)",
        &[&member, &(db::now() + 60000)],
    );
    for (hash, used) in [("open", None), ("used", Some(1))] {
        platform(
            "INSERT INTO invitations(hash,dsp_id,email,role,expires_at,created_by,used_at) \
             VALUES (?,?,'new@example.test','member',?,?,?)",
            &[&hash, &dsp, &(db::now() + 60000), &s(&owner, "id"), &used],
        );
    }
    roles::backfill(&db.platform).unwrap();
    let moves = json!({"authenticator_apps":1,"invitations":2,"memberships":1,
        "recovery_codes":1,"roles":3,"users":1});
    // A read-only report says what starting the release will move.
    let report = operations::directories(&db.config).unwrap();
    let listed = &report["dsps"][0];
    assert_eq!(
        (listed["directory"].clone(), listed["moves"].clone()),
        (json!(null), moves.clone())
    );

    let moved = db.move_people(&dsp).unwrap().unwrap();
    assert_eq!(json!(moved), moves);
    assert!(db.move_people(&dsp).unwrap().is_none());
    let theirs = db.dsp(&dsp).unwrap();
    let count = |sql: &str| theirs.count(sql, []).unwrap();
    let account = theirs.one("SELECT * FROM users", []).unwrap().unwrap();
    assert_eq!(
        (s(&account, "id"), s(&account, "password")),
        (member, password.as_str())
    );
    let manager = db
        .platform
        .one(
            "SELECT id FROM roles WHERE dsp_id=? AND name='Manager'",
            [&dsp],
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        s(
            &theirs
                .one("SELECT role_id FROM memberships", [])
                .unwrap()
                .unwrap(),
            "role_id"
        ),
        s(&manager, "id")
    );
    // The platform owner opens the DSP without a membership; sign-ins end; passkeys belong
    // to the address they were made at.
    for (table, expected) in [
        ("users", 1),
        ("memberships", 1),
        ("roles", 3),
        ("authenticator_apps", 1),
        ("recovery_codes", 1),
        ("invitations", 2),
        ("sessions", 0),
        ("account_passkeys", 0),
    ] {
        assert_eq!(
            count(&format!("SELECT count(*) FROM {table}")),
            expected,
            "{table}"
        );
    }
    drop(theirs);
    assert_eq!(
        db.platform
            .count(
                "SELECT count(*) FROM invitation_routes WHERE dsp_id=?",
                [&dsp]
            )
            .unwrap(),
        2
    );
    // The release before still finds everything where it left it.
    assert_eq!(
        db.platform
            .count("SELECT count(*) FROM memberships WHERE dsp_id=?", [&dsp])
            .unwrap(),
        2
    );
    assert_eq!(
        operations::directories(&db.config).unwrap()["dsps"][0]["directory"],
        "moved"
    );

    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let sign_in = |site: Site| {
        state.login(
            "driver@example.test".into(),
            "the-same-old-password".into(),
            "127.0.0.1".into(),
            String::new(),
            SessionLifetime::Standard,
            site,
        )
    };
    assert!(sign_in(Site::Dsp("lgcy".into())).await.is_ok());
    assert_eq!(
        sign_in(Site::Admin).await.unwrap_err().code,
        "invalid_login"
    );
}

#[test]
fn people_move_again_after_the_release_before_ran_unless_their_directory_changed_too() {
    common::install(&[], &[]);
    use dispatch_core::tenancy::roles;
    let (_root, db, dsp) = bootstrapped();
    let theirs = db.dsp(&dsp).unwrap();
    theirs
        .exec("DELETE FROM settings WHERE key='accounts.directory'", [])
        .unwrap();
    theirs.exec("DELETE FROM roles", []).unwrap();
    drop(theirs);
    roles::seed(&db.platform, &dsp).unwrap();
    let platform = |sql: &str, values: &[&dyn rusqlite::ToSql]| {
        db.platform.exec(sql, values).unwrap();
    };
    let member = |id: &str, email: &str, password: &str| {
        platform(
            "INSERT INTO users(id,email,first_name,last_name,password,created_at) \
             VALUES (?,?,'Dana','Driver',?,'then')",
            &[&id, &email, &password],
        );
        platform(
            "INSERT INTO memberships(id,user_id,dsp_id,role) VALUES (?,?,?,'member')",
            &[&format!("mem_{email}"), &id, &dsp],
        );
        roles::backfill(&db.platform).unwrap();
    };
    let first = "usr_00000000000000000000000000000001";
    member(first, "first@example.test", "before");
    assert!(db.move_people(&dsp).unwrap().is_some());
    // The new release signs them in at the DSP's address, which changes none of their people.
    db.dsp(&dsp)
        .unwrap()
        .exec(
            "INSERT INTO sessions VALUES ('session',?,1,?,1)",
            [first, &(db::now() + 60000).to_string()],
        )
        .unwrap();
    assert!(db.move_people(&dsp).unwrap().is_none());

    // The release before runs again: a password changes and someone joins, in the platform's.
    platform(
        "UPDATE users SET password='after',version=version+1 WHERE id=?",
        &[&first],
    );
    member(
        "usr_00000000000000000000000000000002",
        "second@example.test",
        "theirs",
    );
    let moved = db.move_people(&dsp).unwrap().unwrap();
    assert_eq!((moved["users"], moved["memberships"]), (2, 2));
    let theirs = db.dsp(&dsp).unwrap();
    let passwords = |people: &dispatch_core::db::DspLease| {
        people
            .all("SELECT email,password FROM users ORDER BY email", [])
            .unwrap()
    };
    assert_eq!(
        json!(passwords(&theirs)),
        json!([
            {"email": "first@example.test", "password": "after"},
            {"email": "second@example.test", "password": "theirs"}
        ])
    );
    drop(theirs);
    assert!(db.move_people(&dsp).unwrap().is_none());
    // Startup renames a retired permission in the platform's roles too, which is not the
    // release before running.
    platform(
        "UPDATE roles SET permissions='[\"renamed\"]' WHERE dsp_id=?",
        &[&dsp],
    );
    assert!(db.move_people(&dsp).unwrap().is_none());

    // Changed in both, neither directory is the whole story: nothing moves.
    platform(
        "UPDATE users SET password='rolled back' WHERE id=?",
        &[&first],
    );
    db.dsp(&dsp)
        .unwrap()
        .exec("UPDATE users SET first_name='Renamed' WHERE id=?", [first])
        .unwrap();
    assert!(db.move_people(&dsp).unwrap().is_none());
    let theirs = db.dsp(&dsp).unwrap();
    assert_eq!(passwords(&theirs)[0]["password"], "after");
}
