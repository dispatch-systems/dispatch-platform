//! Get profile, called as an agent calls it.
use crate::testing::{self, agent, call_tool, serving};
use dispatch_core::testing::bootstrapped;
use serde_json::json;

#[tokio::test]
async fn answers_one_stable_profile_for_every_connection_of_its_owner() {
    testing::install();
    let (_root, db, dsp) = bootstrapped();
    // Every connection may use it.
    let laptop = agent(&db, &[&dsp], &[]);
    let desk = agent(&db, &[], &[]);
    let state = serving(db);
    let profile = |caller| {
        let state = state.clone();
        async move {
            call_tool(&state, &caller, "get_profile", json!({}))
                .await
                .unwrap()
                .data
        }
    };
    let (first, second) = (profile(laptop.clone()).await, profile(desk).await);
    assert!(
        first["id"].as_str().unwrap().starts_with("profile_"),
        "{first}"
    );
    assert_eq!(first, second);
    // The profile's id never names the owner's own record.
    assert!(!first.to_string().contains(&laptop.user), "{first}");
}
