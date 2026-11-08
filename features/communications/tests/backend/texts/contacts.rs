//! Texting a number first: its conversation is where a text to it written now is filed, a driver's
//! while it's their work phone, so what's sent there stays there; and what's sent once the number
//! changed hands is answered with the conversation it went to.
use super::*;
use crate::backend::{
    dispatcher,
    model::{ConversationKind, Reference},
    support::{self, phoned, phones, today},
};
use std::sync::Arc;

/// The number `number`'s conversation, opened to text it first.
async fn opened(state: &Arc<State>, dsp: &str, number: &str) -> Conversation {
    let (shared, dsp, number) = (state.clone(), dsp.to_owned(), number.to_owned());
    state
        .run(move |db| open(db, &shared, &dsp, &number))
        .await
        .unwrap()
}

#[tokio::test]
async fn a_number_opened_to_text_first_is_where_its_texts_go_a_drivers_while_its_their_work_phone()
{
    let (_root, state, dsp) = support::started();
    let (_, day) = today(&state, &dsp).await;
    let today = day.to_string();
    let avery = phones(&[("A1", "2065550101")]);
    phoned(&state, &dsp, Some(&today), avery, Vec::new()).await;
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let send = |conversation: &Conversation, text: &'static str| {
        let reference = Reference::Text(conversation.id.clone());
        let (state, c) = (state.clone(), c.clone());
        async move {
            dispatcher::send(&state, &c, support::USE, &reference, text, (&[], None))
                .await
                .unwrap()
        }
    };

    // Avery has 0101: opened, it's their work conversation, and what's sent there stays there.
    let theirs = opened(&state, &dsp, "(206) 555-0101").await;
    assert_eq!(
        (theirs.kind, theirs.driver_id.as_deref()),
        (ConversationKind::Work, Some("A1"))
    );
    assert_eq!(send(&theirs, "Van 214").await.conversation.id, theirs.id);

    // 0303 is nobody's: opened, it's the number's. Blake takes it once it's open, before the text
    // goes, so it's answered with Blake's conversation, where it went.
    let number = opened(&state, &dsp, "2065550303").await;
    assert_eq!(number.kind, ConversationKind::Number);
    let both = phones(&[("A1", "2065550101"), ("B2", "2065550303")]);
    phoned(&state, &dsp, Some(&today), both, Vec::new()).await;
    let went = send(&number, "Which van?").await.conversation;
    assert_eq!(
        (went.kind, went.driver_id.as_deref()),
        (ConversationKind::Work, Some("B2"))
    );
}

/// A driver with a number of their own and one someone added: each number is listed under them in
/// Contacts, and a text from the added one is theirs, the conversation saying it was added so it
/// can be taken off them again; their own number's says nothing of the kind.
#[tokio::test]
async fn every_number_a_driver_has_is_theirs_and_an_added_one_says_so() {
    let (_root, state, dsp) = support::started();
    let mut avery = support::driver("AVERY1", "Avery Lopez", "A1");
    avery.phones = vec!["(206) 555-0101".into(), "+12065550202".into()];
    avery.added_phones = vec!["+12065550202".into()];
    support::know(&state, &dsp, vec![avery], Vec::new(), Vec::new());

    let (shared, d) = (state.clone(), dsp.clone());
    let listed = state
        .read(move |db| contacts(db, &shared, &d, None))
        .await
        .unwrap();
    let theirs: Vec<(&str, Option<&str>)> = listed
        .contacts
        .iter()
        .map(|contact| (contact.number.as_str(), contact.person.code.as_deref()))
        .collect();
    assert_eq!(
        theirs,
        [
            ("+12065550101", Some("AVERY1")),
            ("+12065550202", Some("AVERY1"))
        ]
    );

    let added = opened(&state, &dsp, "2065550202").await;
    assert_eq!(
        (added.with.code.as_deref(), added.added),
        (Some("AVERY1"), true)
    );
    let own = opened(&state, &dsp, "2065550101").await;
    assert_eq!(
        (own.with.code.as_deref(), own.added),
        (Some("AVERY1"), false)
    );
}
