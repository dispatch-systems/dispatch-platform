//! Deleting a DSP gives its Google account back only when no other DSP connects that account,
//! as giving it back ends Google's grant to every DSP connected to it: one signing in to it
//! meanwhile too.
use super::*;
use crate::backend::support::{connect, connect_as, platform};
use dispatch_core::{
    accounts::SessionLifetime, foundation::config::Site, server::http::route::Dsp, testing,
};
use std::time::Duration;

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

#[tokio::test(start_paused = true)]
async fn a_sign_in_under_way_keeps_the_google_account_a_deleted_dsp_connected() {
    let (_root, state, signing) = platform();
    let raw = state
        .login(
            "owner@example.test".into(),
            "test-password-long".into(),
            "127.0.0.1".into(),
            "test".into(),
            SessionLifetime::Standard,
            Site::Admin,
        )
        .await
        .unwrap();
    let at = signing.clone();
    let (c, started) = state
        .run(move |db| {
            let auth = db.authenticate(&raw, &Site::Admin)?;
            let c = db.context(&auth, &at, "connections.manage")?;
            let started = start(db, &c, &Site::Admin)?;
            Ok((c, started))
        })
        .await
        .unwrap();
    let back = url::Url::parse(&started.url).unwrap();
    let asked = |key: &str| {
        let (_, value) = back.query_pairs().find(|(name, _)| name == key).unwrap();
        value.into_owned()
    };
    let (sign_in, code) = (asked("state"), asked("code"));
    // A disabled DSP connects the account the owner signs in to the first DSP with.
    let deleted = state
        .run(|db| {
            let owner = testing::platform_owner(db);
            let deleted = db.new_dsp("Deleted DSP", "UTC", &owner, false)?.id;
            testing::set_dsp_status(db, &deleted, "suspended")?;
            Ok(deleted)
        })
        .await
        .unwrap();
    connect_as(&state, &deleted, &code).await;
    let at = deleted.clone();
    let given = state
        .read(move |db| Ok(db.documents_refresh_token(&at)?.unwrap()))
        .await
        .unwrap();

    // The sign-in holds once Google has given its tokens, before it keeps them.
    let (exchanged, held) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    google::fixture_on_exchange(&code, async move {
        exchanged.send(()).unwrap();
        released.await.unwrap();
    });
    let finishing = state.clone();
    let signing_in = tokio::spawn(async move {
        finish(&finishing, c, Dsp("connections.manage"), sign_in, code).await
    });
    held.await.unwrap();
    // The disabled DSP is deleted meanwhile, and its account waits for the sign-in to be given
    // back: with the clock paused, the wait ends only once nothing else can run.
    let mut revoking = tokio::spawn(revoke(state.clone(), deleted));
    let waited = tokio::time::timeout(Duration::from_secs(60), &mut revoking).await;
    assert!(waited.is_err(), "given back while a sign-in kept it");
    release.send(()).unwrap();
    signing_in.await.unwrap().unwrap();
    revoking.await.unwrap();

    assert!(!google::revoked(&given));
    let connected = state
        .read(move |db| db.documents_connection(&signing))
        .await
        .unwrap();
    assert_eq!(connected.unwrap().account.email, "documents@example.com");
}
