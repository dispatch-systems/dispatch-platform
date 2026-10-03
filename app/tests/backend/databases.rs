//! Every database as it stood before its owners declared it and its migrations: its
//! pinned version, its connections' page cache, and the migrations `schema_migrations`
//! records, in order. A SQL migration's text is held by its hash, since a shipped
//! migration never changes, wherever it is declared.
use dispatch_core::{
    db::{Db, Kind, Migration, Migrations, migrations::Apply},
    foundation::crypto,
    manifest::{Feature, Registry, feature},
};
use std::os::unix::fs::PermissionsExt;

type Database = (
    &'static str,
    i64,
    i64,
    &'static [(u32, &'static str, &'static str)],
);

/// Each database: its name, `user_version`, `cache_size`, and each migration's id, name
/// and the first 16 hex digits of its SQL's SHA-256, or `code`.
const DATABASES: &[Database] = &[
    (
        "platform",
        3,
        -512,
        &[
            (1, "baseline", "7c4ca236b93777bc"),
            (2, "role_columns_and_indexes", "code"),
            (3, "audit_actor_name", "code"),
            (4, "audit_data_and_shown", "code"),
            (5, "outbox_context", "code"),
            (6, "account_security", "18f68ad78ae2529c"),
            (7, "dsp_features", "db94d28ce3bb0de7"),
            (8, "security_hardening", "code"),
            (9, "features_kept_on", "538da674f555fc7a"),
            (10, "driver_codes", "4ad9dc4324530a4e"),
            (11, "agent_keys", "49750173a66d1f9d"),
            (12, "scorecard_feature", "0e5742ffeb7ac0f3"),
            (13, "oauth", "code"),
            (14, "oauth_guard", "code"),
            (15, "agent_activity", "58be6602d76e2bec"),
            (16, "agent_reads", "code"),
        ],
    ),
    (
        "jobs",
        1,
        -512,
        &[
            (1, "baseline", "3e2853f942d01e7d"),
            (2, "scorecard_kind", "ebd9416e6f0716d4"),
            (3, "routes_kind", "de8b5fe1fb72270b"),
            (4, "dvic_kind", "311993998eedd13c"),
            (5, "open_kinds", "13830dde50ae8243"),
        ],
    ),
    (
        "dsp",
        1,
        -512,
        &[
            (1, "baseline", "2053225a86ff281c"),
            (2, "uniform_inventory", "5f838f287f095dc0"),
            (3, "scorecard_collection", "68cc293773d6f259"),
            (4, "routes_collection", "f7b5427163604372"),
            (5, "storage_identity", "1389abbd3214cd37"),
            (6, "dvic_collection", "8bb4e0b288be4890"),
            (7, "driver_match", "dd7cd53bbdbd5805"),
            (8, "open_collections", "a28a44cb4df03634"),
        ],
    ),
    (
        "paycom",
        1,
        -512,
        &[
            (1, "baseline", "a9424b780613b6ee"),
            (2, "employee_timecard_syncs", "21d5410af8708ea4"),
            (3, "employee_history_index", "84322bbfe8d97d08"),
        ],
    ),
    (
        "cortex",
        1,
        -512,
        &[
            (1, "baseline", "76af5a5dacd69f70"),
            (2, "meal_stops", "fd745ffd2ec11469"),
        ],
    ),
    (
        "scorecard",
        1,
        -512,
        &[
            (1, "baseline", "3e392d1896de8f02"),
            (2, "sources", "8be86f869c2b0ebf"),
            (3, "verified_scope", "code"),
        ],
    ),
    (
        "routedata",
        1,
        -32768,
        &[
            (1, "baseline", "2c060a3b390b65b7"),
            (2, "details", "code"),
            (3, "task_keys", "0e69273e130b765a"),
        ],
    ),
    (
        "dvic",
        1,
        -512,
        &[
            (1, "baseline", "c58606f7b1e05e4a"),
            (2, "hidden_drivers", "ddd0114315e99a2a"),
            (3, "verified_scope", "code"),
        ],
    ),
];

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
        found.push((
            kind.name(),
            pragma("user_version"),
            pragma("cache_size"),
            migrations,
        ));
    }
    let expected: Vec<_> = DATABASES
        .iter()
        .map(|(name, version, cache, migrations)| {
            let migrations: Vec<_> = migrations
                .iter()
                .map(|(id, name, apply)| (*id, *name, (*apply).to_owned()))
                .collect();
            (*name, *version, *cache, migrations)
        })
        .collect();
    assert_eq!(found, expected);
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
#[should_panic(expected = "dsp migration 9 is missing")]
fn a_registry_whose_migrations_skip_an_id_is_refused() {
    static GAP: Feature = Feature {
        migrations: &[Migrations {
            kind: Kind::DSP,
            list: &[Migration {
                id: 10,
                name: "after_a_gap",
                apply: Apply::Sql(""),
            }],
        }],
        ..feature("gap")
    };
    crate::install();
    with(&GAP).check();
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
