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
    let dev = db.new_dsp("Dev DSP", "America/Chicago", &owner.id, true)?;
    let north = db.new_dsp("Northline Logistics", "America/Chicago", &owner.id, false)?;
    let summit = db.new_dsp("Summit Delivery", "America/Denver", &owner.id, false)?;
    for dsp in [&dev, &north, &summit] {
        db.enable_all_features(&dsp.id)?;
    }
    // Each DSP but Dev's has an address of its own, as a DSP has once it is set up.
    db.set_code(&north.id, "nll")?;
    db.set_code(&summit.id, "sum")?;
    // Northline's member, in its own directory.
    let member = db.create_user(
        Some(&north.id),
        "member@dispatch.test",
        "Jordan",
        "Ellis",
        "Dispatch-demo-2026!",
    )?;
    let people = db.dsp(&north.id)?;
    people.exec(
        "INSERT INTO memberships(id,user_id,dsp_id,role,role_id) VALUES (?,?,?,'member',?)",
        params![
            crypto::id("mem")?,
            member.id,
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
/// Fills a new DSP with every feature's demo data, given its timezone, and gives its default
/// roles what they hold in a demo.
pub fn demo(db: &Store, dsp: &str, timezone: &str) -> Result<()> {
    let code = db.feature_code(dsp)?;
    for demo in registry()
        .features
        .iter()
        .filter(|f| f.belongs_to(code.as_deref()))
        .filter_map(|f| f.demo)
    {
        demo(db, dsp, timezone)?;
    }
    crate::tenancy::roles::demo(&*db.dsp(dsp)?, dsp)
}
