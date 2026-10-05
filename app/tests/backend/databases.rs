//! Every database as it stood before its owners declared it and its migrations: its
//! pinned version, its connections' page cache, and the migrations `schema_migrations`
//! records, in order, held in the snapshot app/tests/backend/snapshots/databases.json. A
//! SQL migration's text is held by its hash, since a shipped migration never changes,
//! wherever it is declared.
use crate::snapshot;
use dispatch_core::{
    db::{Db, Kind, Migration, Migrations, migrations::Apply},
    foundation::crypto,
    manifest::{Feature, Registry, feature},
};
use serde_json::json;
use std::{
    os::unix::fs::PermissionsExt,
    panic::{AssertUnwindSafe, catch_unwind},
};

fn databases() -> Vec<Kind> {
    dispatch_core::manifest::registry().databases().collect()
}

#[test]
fn every_database_keeps_its_version_cache_and_migrations() {
    crate::install();
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut found = vec![];
    for kind in databases() {
        let db = Db::create(
            &root.path().join(format!("{}.sqlite", kind.name())),
            kind,
            "",
        )
        .unwrap();
        let pragma = |name: &str| db.count(&format!("PRAGMA {name}"), []).unwrap();
        let migrations: Vec<_> = kind
            .migrations()
            .iter()
            .map(|migration| {
                let apply = match migration.apply {
                    Apply::Sql(sql) => crypto::sha(sql)[..16].to_owned(),
                    Apply::Code(_) => "code".to_owned(),
                };
                (migration.id, migration.name, apply)
            })
            .collect();
        let recorded: Vec<(u32, String)> = db
            .query_as("SELECT id,name FROM schema_migrations ORDER BY id", [])
            .unwrap();
        let listed: Vec<_> = migrations
            .iter()
            .map(|(id, name, _)| (*id, (*name).to_owned()))
            .collect();
        assert_eq!(recorded, listed, "{}", kind.name());
        // Each database: its name, `user_version`, `cache_size`, and each migration's id,
        // name and the first 16 hex digits of its SQL's SHA-256, or `code`.
        found.push(json!({
            "name": kind.name(),
            "user_version": pragma("user_version"),
            "cache_size": pragma("cache_size"),
            "migrations": migrations,
        }));
    }
    snapshot::check("databases.json", &json!(found));
}

/// The registry with one more feature, as `install` would check it.
fn with(extra: &'static Feature) -> Registry {
    let features: Vec<&'static Feature> = crate::REGISTRY
        .features
        .iter()
        .copied()
        .chain([extra])
        .collect();
    Registry {
        collectors: crate::REGISTRY.collectors,
        features: Box::leak(features.into_boxed_slice()),
    }
}

#[test]
fn a_registry_whose_migrations_skip_an_id_is_refused() {
    crate::install();
    // The DSP database's next migration, whichever that is now, and one after it.
    let next = crate::REGISTRY.migrations(Kind::DSP).len() as u32 + 1;
    let gap: &'static Feature = Box::leak(Box::new(Feature {
        migrations: Box::leak(Box::new([Migrations {
            kind: Kind::DSP,
            list: Box::leak(Box::new([Migration {
                id: next + 1,
                name: "after_a_gap",
                apply: Apply::Sql(""),
            }])),
        }])),
        ..feature("gap")
    }));
    let refused = catch_unwind(AssertUnwindSafe(|| with(gap).check())).expect_err("a gap");
    assert_eq!(
        refused.downcast_ref::<String>().map(String::as_str),
        Some(format!("dsp migration {next} is missing").as_str())
    );
}

#[test]
#[should_panic(expected = "dsp migration 7 is declared twice")]
fn a_registry_whose_migrations_repeat_an_id_is_refused() {
    static REPEAT: Feature = Feature {
        migrations: &[Migrations {
            kind: Kind::DSP,
            list: &[Migration {
                id: 7,
                name: "again",
                apply: Apply::Sql(""),
            }],
        }],
        ..feature("repeat")
    };
    crate::install();
    with(&REPEAT).check();
}
