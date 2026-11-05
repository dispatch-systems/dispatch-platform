//! Deleting a DSP gives its Google account back only when no other DSP connects that account,
//! as giving it back ends Google's grant to every DSP connected to it.
use super::*;
use crate::backend::support::{connect, connect_as, platform};
use dispatch_core::testing;

#[tokio::test]
async fn a_deleted_dsps_google_account_is_given_back_only_when_no_other_dsp_connects_it() {
    let (_root, state, first) = platform();
    let others = state
        .run(|db| {
            let owner = testing::platform_owner(db);
            let mut others = Vec::new();
            for name in ["Shared DSP", "Own DSP"] {
                others.push(db.new_dsp(name, "UTC", &owner, false)?.id);
            }
            Ok(others)
        })
        .await
        .unwrap();
    let (shared, own) = (&others[0], &others[1]);
    // Two DSPs connect one account; a third, another.
    connect(&state, &first).await;
    connect(&state, shared).await;
    connect_as(&state, own, "fixture:own@example.test").await;
    let token = |dsp: &str| {
        let at = dsp.to_owned();
        state.read(move |db| Ok(db.documents_refresh_token(&at)?.unwrap()))
    };
    let tokens = [
        token(&first).await.unwrap(),
        token(shared).await.unwrap(),
        token(own).await.unwrap(),
    ];

    revoke(state.clone(), first.clone()).await;
    revoke(state.clone(), own.clone()).await;
    let revoked = tokens.map(|token| google::revoked(&token));
    assert_eq!(revoked, [false, false, true]);
}
