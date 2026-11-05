//! A DSP disabled while Google renews its access token, whose Drive is reached no more.
use super::*;
use crate::backend::{
    drive::DOC,
    google,
    support::{ACCOUNT, connect, disabling, platform},
};
use dispatch_core::Code;

#[tokio::test]
async fn a_dsp_disabled_while_its_token_is_renewed_makes_nothing_in_its_drive() {
    let (_root, state, dsp) = platform();
    connect(&state, &dsp).await;
    let at = dsp.clone();
    let (connection, refresh) = state
        .read(move |db| {
            Ok((
                db.documents_connection(&at)?,
                db.documents_refresh_token(&at)?,
            ))
        })
        .await
        .unwrap();
    let (folder, refresh) = (connection.unwrap().folder_id, refresh.unwrap());
    google::fixture_on_renewal(&refresh, disabling(&state, &dsp));
    let refused = make(&state, &dsp, &folder, DOC, "Made while disabled").await;
    assert!(refused.unwrap_err().is(Code::DspUnavailable));
    let google = Google::of(&state.config).unwrap();
    let made = google.inside(ACCOUNT, &folder).await.unwrap();
    assert!(!made.iter().any(|item| item.name == "Made while disabled"));
}
