use crate::{
    Result,
    db::{self, Store},
    ensure,
    foundation::crypto,
    manifest::registry,
};
use rusqlite::params;
use serde_json::{Value, json};

pub fn bootstrap(
    db: &Store,
    email: &str,
    first: &str,
    last: &str,
    password: &str,
) -> Result<Value> {
    ensure(
        db.platform
            .one("SELECT id FROM users LIMIT 1", [])?
            .is_none(),
        "bootstrap_requires_empty_platform",
        409,
    )?;
    let owner = db.create_user(None, email, first, last, password)?;
    let dsp = if db.config.environment.is_preview() {
        let dsp = db.new_dsp("Dev DSP", "UTC", &owner.id, true)?;
        db.enable_all_features(&dsp.id)?;
        Some(dsp)
    } else {
        None
    };
    Ok(json!({"owner":owner,"dsp":dsp}))
}
/// Whether the platform has an active owner, as bootstrapping makes one. The server starts
/// only once it has.
pub fn has_active_owner(db: &Store) -> Result<bool> {
    Ok(db
        .platform
        .one(
            "SELECT id FROM users WHERE platform_owner=1 AND status='active' LIMIT 1",
            [],
        )?
        .is_some())
}
pub fn seed(db: &Store) -> Result<()> {
    ensure(
        db.config.development && db.config.fixture,
        "fixtures_require_development",
        403,
    )?;
    if db.config.platform().join("development-seeded").exists() {
        return Ok(());
    }
    ensure(
        db.platform
            .one("SELECT id FROM users LIMIT 1", [])?
            .is_none(),
        "fixture_state_not_empty",
        409,
    )?;
    let owner = db.create_user(
        None,
        "owner@dispatch.test",
        "Platform",
        "Owner",
        "Dispatch-demo-2026!",
    )?;
    let new_dsp = |name: &str, timezone: &str, permanent: bool| {
        db.new_dsp_as(
            &demo_id(db, "dsp", name),
            name,
            timezone,
            &owner.id,
            permanent,
        )
    };
    let dev = new_dsp("Dev DSP", "America/Chicago", true)?;
    let north = new_dsp("Northline Logistics", "America/Chicago", false)?;
    let summit = new_dsp("Summit Delivery", "America/Denver", false)?;
    for dsp in [&dev, &north, &summit] {
        db.enable_all_features(&dsp.id)?;
    }
    // Each DSP but Dev's has an address of its own, as a DSP has once it is set up.
    db.set_code(&north.id, "nll")?;
    db.set_code(&summit.id, "sum")?;
    // Each owns the custom features under a custom ID of its own, apart from its code.
    db.set_custom_id(&north.id, "n1")?;
    db.set_custom_id(&summit.id, "s2")?;
    // Northline's member, in its own directory.
    db.create_user(
        Some(&north.id),
        "member@dispatch.test",
        "Jordan",
        "Ellis",
        "Dispatch-demo-2026!",
    )?;
    let people = db.dsp(&north.id)?;
    // Under the same ID each time, as the DSPs are, so the person Driver Match makes of their
    // login keeps its code.
    let member = demo_id(db, "usr", "member@dispatch.test");
    people.exec(
        "UPDATE users SET id=? WHERE email='member@dispatch.test'",
        [&member],
    )?;
    people.exec(
        "INSERT INTO memberships(id,user_id,dsp_id,role,role_id) VALUES (?,?,?,'member',?)",
        params![
            crypto::id("mem")?,
            member,
            north.id,
            crate::tenancy::roles::default_role(&people, &north.id, "member")?
        ],
    )?;
    drop(people);
    for dsp in [&dev, &north] {
        let id = dsp.id.as_str();
        for collector in registry().collectors {
            collector.demo(db, id)?;
        }
        demo(db, id, &dsp.timezone)?;
        db.audit(Some(&owner.id), Some(id), "development.fixtures_loaded", "")?;
    }
    db::write_private(
        &db.config.platform().join("development-seeded"),
        b"synthetic fixtures initialized\n",
    )
}
/// The ID of the development fixtures' `name`, of the kind `prefix` names, as `crypto::id` writes
/// one: the same each time they start afresh at the platform's address, so a link into a DSP
/// still opens it, and apart from those of fixtures at another address, as of tests running
/// side by side.
fn demo_id(db: &Store, prefix: &str, name: &str) -> String {
    let drawn = crypto::sha(format!("{} {name}", db.config.origin));
    format!("{prefix}_{}", &drawn[..32])
}
/// Fills a new DSP with every feature's demo data, given its timezone, and gives its default
/// roles what they hold in a demo.
pub fn demo(db: &Store, dsp: &str, timezone: &str) -> Result<()> {
    let owner = db.feature_owner(dsp)?;
    for demo in registry()
        .features
        .iter()
        .filter(|f| f.belongs_to(owner.as_deref()))
        .filter_map(|f| f.demo)
    {
        demo(db, dsp, timezone)?;
    }
    crate::tenancy::roles::demo(&*db.dsp(dsp)?, dsp)
}
