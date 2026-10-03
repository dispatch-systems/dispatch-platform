//! The read cache as it stood before its owners declared their domains and what each
//! cached read depends on: which cached reads a write to each domain evicts, and the
//! domain each collection writes.
use crate::{
    collectors::Provider,
    contracts::JobKind,
    manifest::registry,
    read_cache::{self, DataDomain, ReadCache, Scope},
};

/// For a write to each domain for DSP `a`: whether it evicts the DSP listings, `a`'s
/// meal reads and `a`'s people. Another DSP's reads are never evicted.
const EVICTS: &[(&str, [bool; 3])] = &[
    ("paycom", [true, true, true]),
    ("meals", [false, true, true]),
    ("live", [false, true, false]),
    ("routes", [false, false, true]),
    ("scorecard", [false, false, true]),
    ("dvic", [false, false, true]),
    ("drivers", [false, true, true]),
    ("tenant", [true, true, true]),
    ("schedules", [true, false, false]),
];
/// The domain a finished collection of each job kind writes.
const COLLECTIONS: &[(&str, &str)] = &[
    ("paycom.collect", "paycom"),
    ("cortex.meal_breaks.collect", "meals"),
    ("cortex.scorecard.collect", "scorecard"),
    ("cortex.routes.collect", "routes"),
    ("cortex.dvic.collect", "dvic"),
];

fn domain(name: &str) -> DataDomain {
    registry()
        .domains()
        .find(|domain| domain.id() == name)
        .unwrap_or_else(|| panic!("no domain {name}"))
}
fn scope(read: &str, dsp: &str) -> Scope {
    match read {
        "listings" => Scope::listings(),
        "meals" => Scope::tenant(crate::meals::CACHED, dsp),
        "people" => Scope::tenant(read_cache::PEOPLE, dsp),
        _ => panic!("no cached read {read}"),
    }
}

#[test]
fn a_write_evicts_the_cached_reads_it_did() {
    let reads = [
        ("listings", "a"),
        ("meals", "a"),
        ("people", "a"),
        ("meals", "b"),
        ("people", "b"),
    ];
    for (name, [listings, meals, people]) in EVICTS {
        let cache = ReadCache::default();
        for (read, dsp) in reads {
            cache
                .read(scope(read, dsp), "key".into(), 1, || Ok(1))
                .unwrap();
        }
        cache.invalidate_tenant("a", domain(name));
        let evicted: Vec<bool> = reads
            .iter()
            .map(|(read, dsp)| {
                cache
                    .read(scope(read, dsp), "key".into(), 1, || Ok(2))
                    .unwrap()
                    == 2
            })
            .collect();
        assert_eq!(
            evicted,
            [*listings, *meals, *people, false, false],
            "{name}"
        );
    }
}

#[test]
fn each_collection_writes_the_domain_it_did() {
    let mut registered: Vec<_> = Provider::all().flat_map(|p| p.job_kinds()).collect();
    let mut listed: Vec<_> = COLLECTIONS.iter().map(|(kind, _)| *kind).collect();
    registered.sort();
    listed.sort();
    assert_eq!(registered, listed);
    for (kind, name) in COLLECTIONS {
        let kind = JobKind::parse(kind).unwrap();
        assert_eq!(
            DataDomain::collection(kind),
            domain(name),
            "{}",
            kind.as_str()
        );
    }
}
