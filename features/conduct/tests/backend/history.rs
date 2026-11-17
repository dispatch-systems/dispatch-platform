//! History uses stored ordering and current identities, independently of list pagination.
use super::*;
use crate::{
    api::types::{ConductTier, WriteupInput},
    backend::{support::ready, writeups::Subject},
};
use dispatch_core::manifest::people::{Person, PersonRole};

fn directory() -> Everyone {
    Everyone::new(
        vec![Person {
            code: "current".into(),
            name: "Jordan Gray".into(),
            names: vec!["Jordan Gray".into()],
            role: PersonRole::Driver,
            title: None,
            ids: BTreeMap::new(),
            emails: vec![],
            phones: vec![],
            added_phones: vec![],
            confirmed: true,
            photo: None,
        }],
        BTreeMap::from([("merged".into(), "current".into())]),
    )
}
fn add(store: &Store, c: &Context, person: &str, date: &str, tier: ConductTier) -> String {
    writeups::add(
        store,
        c,
        &Subject {
            code: person.into(),
            name: "Old printed name".into(),
        },
        &WriteupInput {
            person: person.into(),
            category: "tardies".into(),
            tier,
            final_kind: None,
            suspension: None,
            written_on: date.into(),
            statement: "Arrived late.".into(),
            prior: "Start time discussed.".into(),
            plan: "Arrive on time.".into(),
            revision: None,
        },
        "2026-11-08",
    )
    .unwrap()
}
fn query(after: &str) -> Query<'_> {
    Query {
        person: "current",
        search: "jordan",
        category: "tardies",
        tier: "",
        status: "waiting",
        after,
        limit: 1,
    }
}

#[test]
fn merged_people_are_filtered_before_pagination_and_a_deleted_cursor_still_advances() {
    let (_root, store, c) = ready();
    let first = add(&store, &c, "merged", "2026-11-01", ConductTier::Verbal);
    let second = add(&store, &c, "current", "2026-11-01", ConductTier::Written);
    add(&store, &c, "other", "2026-11-02", ConductTier::Final);
    // Equal dates and timestamps must still have a deterministic cursor boundary.
    store
        .dsp(&c.dsp.id)
        .unwrap()
        .exec(
            "UPDATE conduct_writeups SET created_at='2026-11-03T00:00:00.000Z'",
            [],
        )
        .unwrap();
    let mut ids = [first, second];
    ids.sort();
    let everyone = directory();
    let page = read_from(&store, &c.dsp.id, &everyone, query("")).unwrap();
    assert_eq!(page.writeups.len(), 1);
    assert_eq!(page.writeups[0].id, ids[0]);
    assert_eq!(page.writeups[0].person, "current");
    writeups::delete(&store, &c, &ids[0], 1).unwrap();
    let next = read_from(
        &store,
        &c.dsp.id,
        &everyone,
        query(page.next_cursor.as_deref().unwrap()),
    )
    .unwrap();
    assert_eq!(
        next.writeups
            .iter()
            .map(|w| w.id.as_str())
            .collect::<Vec<_>>(),
        [ids[1].as_str()]
    );
    assert!(next.next_cursor.is_none());
}

#[test]
fn context_uses_the_selected_date_and_an_edits_original_position_on_that_date() {
    let (_root, store, c) = ready();
    let earlier = add(&store, &c, "merged", "2026-01-01", ConductTier::Written);
    let later = add(&store, &c, "current", "2026-11-01", ConductTier::Final);
    let everyone = directory();
    let historical = context_from(
        &store,
        &c.dsp.id,
        &everyone,
        "current",
        "tardies",
        "2026-10-01",
        "",
    )
    .unwrap();
    assert_eq!(historical.standing, Some(ConductTier::Verbal));
    assert_eq!(
        historical
            .earlier
            .iter()
            .map(|w| w.id.as_str())
            .collect::<Vec<_>>(),
        [earlier.as_str()]
    );
    // Move the older notice onto the newer notice's date. Its original creation order stays.
    store
        .dsp(&c.dsp.id)
        .unwrap()
        .exec(
            "UPDATE conduct_writeups SET created_at=? WHERE id=?",
            ["2026-11-01T00:00:00.000Z", earlier.as_str()],
        )
        .unwrap();
    store
        .dsp(&c.dsp.id)
        .unwrap()
        .exec(
            "UPDATE conduct_writeups SET created_at=? WHERE id=?",
            ["2026-11-02T00:00:00.000Z", later.as_str()],
        )
        .unwrap();
    let moving = context_from(
        &store,
        &c.dsp.id,
        &everyone,
        "current",
        "tardies",
        "2026-11-01",
        &earlier,
    )
    .unwrap();
    assert!(moving.earlier.is_empty());
    assert_eq!(moving.standing, None);
}
