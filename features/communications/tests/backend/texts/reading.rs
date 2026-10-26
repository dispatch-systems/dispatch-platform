//! Reading texts: a text's picture goes only to a member whose access still stands once the phone
//! has read it.
use super::*;
use crate::backend::support;
use dispatch_core::testing;

#[tokio::test]
async fn a_texts_picture_is_refused_once_the_members_access_ended_while_the_phone_read_it() {
    support::install();
    let (_root, store, dsp) = testing::bootstrapped();
    let config = store.config.clone();
    drop(store);
    let state = State::new(config).unwrap();
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let photo = "file_photo";
    assert!(file(&state, &c, support::USE, photo).await.is_ok());

    // Communications goes off for the DSP as the phone reads it, and with it their permission.
    support::off_while_reading(&dsp);
    let refused = file(&state, &c, support::USE, photo).await.err();
    assert_eq!(
        refused.map(|error| error.code).as_deref(),
        Some("permission_denied")
    );
}
