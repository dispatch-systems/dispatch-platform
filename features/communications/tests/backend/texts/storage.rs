//! Texting's storage: a Done marked before migration 6, on its conversation alone, covers the texts
//! written before it once the migration runs, so they stay handled wherever they're filed again,
//! and what came after it still waits.
use super::*;
use crate::ConversationState;
use crate::backend::{
    support::{DRIVERS, at, filed, phoned, phones, started, texted, today},
    texts::reading,
};
use rusqlite::params;

#[tokio::test]
async fn a_done_from_before_migration_6_still_covers_its_texts_once_theyre_filed_again() {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    let today = day.to_string();
    // Avery has 0202. From 0101, no work phone yet, texts at 6:00 and 6:30, then one at 7:30.
    let on_0202 = phones(&[("A1", "2065550202")]);
    phoned(&state, &dsp, Some(&today), on_0202, Vec::new()).await;
    let mut texts = Vec::new();
    for (body, hour, minute) in [
        ("Which van?", 6, 0),
        ("Any van?", 6, 30),
        ("Van 214 is flat", 7, 30),
    ] {
        let written = at(zone, day, hour, minute);
        texts.push(texted(&state, &dsp, "+12065550101", body, written).await);
    }
    let number = filed(&state, &dsp, &texts[0]).await.id;
    // Its number's conversation was marked done at 7:00 as Done was kept before migration 6, on
    // the conversation alone; the migration then runs over it as an upgrade does.
    let (at_dsp, done_at) = (dsp.clone(), at(zone, day, 7, 0));
    state
        .run(move |db| {
            let db = db.dsp(&at_dsp)?;
            db.exec(
                "UPDATE communications_text_conversations SET done_at=?,done_by='me' WHERE id=?",
                params![done_at, number],
            )?;
            add_text_done(&db)
        })
        .await
        .unwrap();

    // A read shows Avery with 0101, so its texts are theirs. Those the Done covered stay
    // handled; the 7:30 one waits.
    let on_0101 = phones(&[("A1", "2065550101")]);
    phoned(&state, &dsp, Some(&today), on_0101, Vec::new()).await;
    let avery = filed(&state, &dsp, &texts[0]).await;
    let theirs = Some((DRIVERS.as_str().to_owned(), "A1".to_owned()));
    assert_eq!(avery.driver, theirs);
    let (shared, at_dsp) = (state.clone(), dsp.clone());
    let shown = state
        .read(move |db| reading::one(db, &shared, &at_dsp, &avery.id))
        .await
        .unwrap();
    assert_eq!(
        (shown.state, shown.waiting_since),
        (ConversationState::Waiting, Some(at(zone, day, 7, 30)))
    );
}
