//! A writeup from written to signed, against a real store in a temporary directory.
use super::*;
use crate::{
    api::types::{CategoryInput, ConductFinalKind, ConductStepDown, ConductTier, ConductUnit},
    backend::{categories, copies},
};
use dispatch_core::{accounts::Auth, db::s, server::operations, testing};

const TODAY: &str = "2026-11-08";

/// A preview platform's first DSP, every feature on, and its owner writing up.
fn ready() -> (tempfile::TempDir, Store, Context) {
    testing::install(&[], &[&crate::FEATURE]);
    let (root, store) = testing::store();
    let bootstrap = operations::bootstrap(
        &store,
        "conduct@example.test",
        "Casey",
        "Supervisor",
        "test-password-long",
    )
    .unwrap();
    let auth = Auth {
        user: serde_json::from_value(bootstrap["owner"].clone()).unwrap(),
        hash: String::new(),
        csrf: String::new(),
        raw: String::new(),
        preview: None,
        site: dispatch_core::foundation::config::Site::Admin,
        scope: None,
    };
    let c = store
        .context(&auth, s(&bootstrap["dsp"], "id"), "conduct.manage")
        .unwrap();
    (root, store, c)
}

fn subject() -> Subject {
    Subject {
        code: "P-0001".into(),
        name: "Jordan Gray".into(),
    }
}

fn writing(category: &str, tier: ConductTier, revision: Option<i64>) -> WriteupInput {
    WriteupInput {
        person: "P-0001".into(),
        category: category.into(),
        tier,
        final_kind: (tier == ConductTier::Final).then_some(ConductFinalKind::Suspension),
        suspension: (tier == ConductTier::Final).then(|| "1 day".into()),
        written_on: "2026-11-02".into(),
        statement: "DA had excessive tardies on 10/30, 11/1 and 11/2.".into(),
        prior: "DA is aware of the start time.".into(),
        plan: "DA will arrive on time.".into(),
        revision,
    }
}

#[test]
fn a_writeup_changes_until_it_is_signed_and_never_after() {
    let (_root, store, c) = ready();
    let id = add(
        &store,
        &c,
        &subject(),
        &writing("tardies", ConductTier::Verbal, None),
        TODAY,
    )
    .unwrap();
    let written = read(&store, &c.dsp.id, &id).unwrap();
    assert_eq!(written.row.supervisor_name, "Casey Supervisor");
    assert_eq!(written.row.revision, 1);

    change(
        &store,
        &c,
        &id,
        &subject(),
        &writing("tardies", ConductTier::Final, Some(1)),
        TODAY,
    )
    .unwrap();
    let changed = read(&store, &c.dsp.id, &id).unwrap();
    assert_eq!(changed.row.tier, ConductTier::Final);
    assert_eq!(changed.row.suspension.as_deref(), Some("1 day"));
    // What another session read before the change is refused.
    let stale = change(
        &store,
        &c,
        &id,
        &subject(),
        &writing("tardies", ConductTier::Written, Some(1)),
        TODAY,
    );
    assert_eq!(stale.unwrap_err().code, "conduct_writeup_changed");

    // Signed no earlier than it was written, and no later than today.
    for day in ["2026-11-01", "2026-11-09"] {
        assert_eq!(
            sign(&store, &c, &id, 2, day, TODAY).unwrap_err().code,
            "conduct_signed_date"
        );
    }
    copies::keep(&store, &c, &id, &[0xff, 0xd8, 0xff, 0xe0]).unwrap();
    sign(&store, &c, &id, 2, "2026-11-03", TODAY).unwrap();
    let signed = read(&store, &c.dsp.id, &id).unwrap();
    assert_eq!(signed.row.signed_on.as_deref(), Some("2026-11-03"));
    assert_eq!(signed.signed_by_name.as_deref(), Some("Casey Supervisor"));

    // Signed, it stays as it is: its words, its photo and the writeup itself.
    let locked = change(
        &store,
        &c,
        &id,
        &subject(),
        &writing("tardies", ConductTier::Verbal, Some(3)),
        TODAY,
    );
    assert_eq!(locked.unwrap_err().code, "conduct_writeup_signed");
    assert_eq!(
        copies::keep(&store, &c, &id, &[0xff, 0xd8, 0xff, 0xe1])
            .unwrap_err()
            .code,
        "conduct_writeup_signed"
    );
    assert_eq!(
        delete(&store, &c, &id, 3).unwrap_err().code,
        "conduct_writeup_signed"
    );
    assert_eq!(
        copies::read(&store, &c.dsp.id, &id).unwrap().1,
        [0xff, 0xd8, 0xff, 0xe0]
    );
}

#[test]
fn a_retired_category_takes_no_new_writeup_and_keeps_its_own() {
    let (_root, store, c) = ready();
    let id = add(
        &store,
        &c,
        &subject(),
        &writing("pod", ConductTier::Verbal, None),
        TODAY,
    )
    .unwrap();
    categories::retire(&store, &c, "pod", false).unwrap();
    let new = add(
        &store,
        &c,
        &subject(),
        &writing("pod", ConductTier::Verbal, None),
        TODAY,
    );
    assert_eq!(new.unwrap_err().code, "conduct_category_not_found");
    change(
        &store,
        &c,
        &id,
        &subject(),
        &writing("pod", ConductTier::Written, Some(1)),
        TODAY,
    )
    .unwrap();
    // Back in use, it takes new writeups again, unless another in use has its name by then.
    categories::add(
        &store,
        &c,
        &CategoryInput {
            name: "POD".into(),
            start_tier: ConductTier::Verbal,
            step_downs: vec![ConductStepDown {
                tier: ConductTier::Final,
                after: 2,
                unit: ConductUnit::Weeks,
                to: None,
            }],
            statement: "Statement".into(),
            prior: "Prior".into(),
            plan: "Plan".into(),
            revision: None,
        },
    )
    .unwrap();
    assert_eq!(
        categories::retire(&store, &c, "pod", true)
            .unwrap_err()
            .code,
        "conduct_category_name_taken"
    );
    // It keeps the step downs it was given: here only Final's, straight to a clean record.
    let db = store.dsp(&c.dsp.id).unwrap();
    let added = categories::all(&db)
        .unwrap()
        .into_iter()
        .find(|category| category.name == "POD" && !category.retired)
        .unwrap();
    assert_eq!(added.step_downs.len(), 1);
    assert_eq!(added.step_downs[0].to, None);
}

#[test]
fn a_writeup_is_dated_no_later_than_today_and_its_deletion_is_logged() {
    let (_root, store, c) = ready();
    let mut ahead = writing("uniform", ConductTier::Verbal, None);
    ahead.written_on = "2026-11-09".into();
    assert_eq!(
        add(&store, &c, &subject(), &ahead, TODAY).unwrap_err().code,
        "conduct_date_ahead"
    );

    let id = add(
        &store,
        &c,
        &subject(),
        &writing("uniform", ConductTier::Verbal, None),
        TODAY,
    )
    .unwrap();
    delete(&store, &c, &id, 1).unwrap();
    assert_eq!(
        read(&store, &c.dsp.id, &id).unwrap_err().code,
        "conduct_writeup_not_found"
    );
    assert_eq!(
        testing::audit_actions(&store, &c.dsp.id, "conduct."),
        ["conduct.writeup_deleted"]
    );
}
