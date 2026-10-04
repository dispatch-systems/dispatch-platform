//! Every table of every database has one owner, which declares it: core, a collector or a
//! feature. Only that owner's code runs SQL on it.
use crate::REGISTRY;
use dispatch_core::manifest::{Feature, Registry, feature};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// Each database's tables, as its recorded schema in core/db/tests/backend/schema creates
/// them.
fn recorded() -> BTreeMap<String, BTreeSet<String>> {
    // The repository root holds Cargo.lock, wherever this crate's manifest sits in it.
    let schemas = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|dir| dir.join("Cargo.lock").is_file())
        .expect("repository root")
        .join("core/db/tests/backend/schema");
    let mut databases = BTreeMap::new();
    for entry in std::fs::read_dir(schemas).unwrap() {
        let path = entry.unwrap().path();
        let database = path.file_stem().unwrap().to_string_lossy().into_owned();
        let tables = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .filter_map(|line| line.strip_prefix("CREATE TABLE "))
            .map(|rest| {
                let name = rest.split([' ', '(']).next().unwrap();
                name.trim_matches('"').to_owned()
            })
            .collect();
        databases.insert(database, tables);
    }
    databases
}

#[test]
fn every_table_has_exactly_one_declared_owner() {
    crate::install();
    let databases = recorded();
    let declared = REGISTRY.tables();
    let kinds: BTreeSet<String> = REGISTRY.databases().map(|k| k.name().into()).collect();
    assert_eq!(
        kinds,
        databases.keys().cloned().collect(),
        "every database has a recorded schema, which `{}` writes",
        crate::snapshot::UPDATE
    );
    let mut wrong = vec![];
    for (database, tables) in &databases {
        for table in tables {
            let owners: Vec<&str> = declared
                .iter()
                .filter(|(_, at, name)| (*at == database || *at == "*") && name == table)
                .map(|(owner, _, _)| *owner)
                .collect();
            if owners.len() != 1 {
                wrong.push(format!("{database}/{table} is declared by {owners:?}"));
            }
        }
    }
    for (owner, database, table) in &declared {
        let held = match *database {
            "*" => databases.values().any(|tables| tables.contains(*table)),
            database => databases
                .get(database)
                .is_some_and(|tables| tables.contains(*table)),
        };
        if !held {
            wrong.push(format!(
                "{owner} declares {database}/{table}, which no schema has"
            ));
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
}

#[test]
#[should_panic(expected = "dsp/people is declared by driver_match and twice")]
fn a_registry_where_two_owners_declare_one_table_is_refused() {
    static TWICE: Feature = Feature {
        tables: &[("dsp", &["people"])],
        ..feature("twice")
    };
    crate::install();
    let features: Vec<&'static Feature> =
        REGISTRY.features.iter().copied().chain([&TWICE]).collect();
    Registry {
        collectors: REGISTRY.collectors,
        features: Box::leak(features.into_boxed_slice()),
    }
    .check();
}
