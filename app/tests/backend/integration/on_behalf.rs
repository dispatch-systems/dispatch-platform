//! Work done for someone without a request of theirs, as telling them of a new message, follows
//! what a session of theirs would hold now, at the address they used (`Store::may`).
use dispatch_core::{db::s, foundation::config::Site, testing as common};

#[test]
fn work_done_for_someone_follows_what_a_session_of_theirs_would_hold_now() {
    dispatch_backend::install();
    let (_root, db) = common::seeded();
    let north = db
        .platform
        .one("SELECT id FROM dsps WHERE code='nll'", [])
        .unwrap()
        .unwrap();
    let north = s(&north, "id");
    let people = db.dsp(north).unwrap();
    let member = people
        .one(
            "SELECT users.id,role_id FROM users JOIN memberships ON user_id=users.id \
             WHERE email='member@dispatch.test'",
            [],
        )
        .unwrap()
        .unwrap();
    let (member, role) = (s(&member, "id"), s(&member, "role_id"));
    let owner = common::platform_owner(&db);
    let (at_north, at_admin) = (Site::Dsp("nll".into()), Site::Admin);
    let may = |user: &str, site: &Site| db.may(user, site, north, "communications.use").unwrap();

    // Their role doesn't hold it; once it does, they may, signed in at the DSP's own address alone.
    assert!(!may(member, &at_north));
    people
        .exec(
            "UPDATE roles SET permissions='[\"communications.use\"]' WHERE id=?",
            [role],
        )
        .unwrap();
    assert!(may(member, &at_north));
    assert!(!may(member, &at_admin));
    // A platform owner may at the admin's address, where their account is, alone.
    assert!(may(&owner, &at_admin));
    assert!(!may(&owner, &at_north));

    // Their account disabled, they may not; nor anyone once the feature is off for the DSP.
    people
        .exec("UPDATE users SET status='disabled' WHERE id=?", [member])
        .unwrap();
    assert!(!may(member, &at_north));
    db.set_feature(north, "communications", false, &owner)
        .unwrap();
    assert!(!may(&owner, &at_admin));
}
