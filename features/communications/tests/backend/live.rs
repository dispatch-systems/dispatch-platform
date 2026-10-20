//! Who at the DSP is writing in which conversation, as their teammates' pages see it.
use super::*;
use dispatch_core::accounts::api::types::PublicUser;

fn user(id: &str, first: &str, last: &str) -> PublicUser {
    PublicUser {
        id: id.into(),
        email: format!("{id}@example.com"),
        first_name: first.into(),
        last_name: last.into(),
        platform_owner: false,
    }
}

#[test]
fn teammates_see_who_is_writing_until_they_send() {
    crate::backend::support::install();
    let (_root, store) = dispatch_core::testing::store();
    let state = State::new(store.config.clone()).unwrap();
    let keisha = user("user_keisha", "Keisha", "Brown");
    let (_, before) = (typing("dsp_live", "user_tom"), typing("dsp_live", "user_tom").0.version);
    writing(&state, "dsp_live", "cv_1", &keisha);
    // Saying so again while writing changes nothing.
    writing(&state, "dsp_live", "cv_1", &keisha);
    let (seen, next) = typing("dsp_live", "user_tom");
    assert_eq!(seen.version, before + 1);
    let shown: Vec<(&str, &str, &str)> = seen
        .writing
        .iter()
        .map(|w| (w.conversation_id.as_str(), w.first_name.as_str(), w.name.as_str()))
        .collect();
    assert_eq!(shown, [("cv_1", "Keisha", "Keisha Brown")]);
    assert!(next.is_some());
    // Not to themselves.
    assert!(typing("dsp_live", "user_keisha").0.writing.is_empty());
    // Sent, they're writing no more.
    stopped(&state, "dsp_live", "cv_1", "user_keisha");
    let (after, next) = typing("dsp_live", "user_tom");
    assert_eq!((after.writing.len(), after.version, next), (0, before + 2, None));
}
