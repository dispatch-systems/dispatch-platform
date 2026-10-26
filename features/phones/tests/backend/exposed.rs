//! What Phones exposes, as a feature using it finds it: only while it's on for the DSP, with
//! the DSP's phones and the texts they relay, and sending one from them.
use dispatch_core::{State, testing};
use dispatch_phones_contract::{Outgoing, Status, for_dsp};

#[tokio::test]
async fn texting_sends_from_the_dsps_phone_while_phones_is_on() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    let owner = testing::a_user(&db);
    crate::backend::demo::demo(&db, &dsp, "America/Los_Angeles").unwrap();
    let texting = for_dsp(&db, &dsp).unwrap().expect("Phones is on");
    let phones = texting.phones(&db, &dsp).unwrap();
    assert_eq!(phones.len(), 1);
    assert!(phones[0].online);
    let received = texting.changes(&db, &dsp, 0, 100).unwrap();
    assert_eq!(received.texts.len(), 3);
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let outgoing = Outgoing {
        phone: None,
        to: "(425) 555-0199".into(),
        body: "Come to the station first".into(),
        files: Vec::new(),
        by: owner.clone(),
    };
    let sent = texting.send(&state, &dsp, outgoing).await.unwrap();
    assert_eq!(sent.phone, phones[0].id);
    assert_eq!(sent.number, "+14255550199");
    assert_eq!(
        sent.status,
        Some(Status::Delivered),
        "the demo's stand-in delivers at once"
    );
    let (at, after) = (dsp.clone(), received.last);
    let changed = state
        .read(move |db| texting.changes(db, &at, after, 100))
        .await
        .unwrap();
    assert_eq!(changed.texts, [sent]);

    let at = dsp.clone();
    state
        .run(move |db| db.set_feature(&at, "phones", false, &owner))
        .await
        .unwrap();
    let at = dsp.clone();
    let offered = state.read(move |db| for_dsp(db, &at)).await.unwrap();
    assert!(offered.is_none());
}
