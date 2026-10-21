//! Each workspace's logo, as fixture mode's Slack describes it: the hourly check keeps a copy of
//! a workspace's own, from Slack's own servers alone, and forgets it once the workspace shows
//! Slack's, or goes.
use super::*;
use crate::backend::{app, maintenance::MAINTENANCE, support, testing::disconnect};
use dispatch_core::testing;

const OPERATIONS: &str = "T0FIXTURE1";
const DRIVERS: &str = "T0FIXTURE2";
/// Where fixture mode's Slack keeps Operations' logo.
const OWN: &str = "https://avatars.slack-edge.com/fixture/T0FIXTURE1_132.png";

/// The logos kept of the DSP's workspaces: each one's workspace, and the kind it's shown as.
async fn kept(state: &Arc<State>, dsp: &str) -> Vec<(String, &'static str)> {
    let of = dsp.to_owned();
    state
        .read(move |db| {
            let teams = db.slack_logos(&of)?.into_keys();
            teams
                .map(|team| {
                    let (kind, _) = picture(db, &of, &team)?.expect("kept");
                    Ok((team, kind))
                })
                .collect()
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn the_hourly_check_keeps_a_workspaces_own_logo_and_forgets_slacks() {
    let (_root, state, dsp) = support::platform();
    let of = dsp.clone();
    state
        .run(move |db| {
            let owner = testing::a_user(db);
            db.set_feature(&of, "slack", true, &owner).map(drop)
        })
        .await
        .unwrap();
    let operations = support::connect(&state, &dsp, OPERATIONS, &app::scopes()).await;
    let drivers = support::connect(&state, &dsp, DRIVERS, &app::scopes()).await;
    // Drivers had a logo of its own, and shows Slack's now.
    keep(&drivers, &json!({"image_132": OWN, "image_default": false}))
        .await
        .unwrap();
    (MAINTENANCE.run)(state.clone(), true).await;
    let operations_only = vec![(OPERATIONS.to_owned(), "image/png")];
    assert_eq!(kept(&state, &dsp).await, operations_only);
    // A logo from anywhere but Slack's own servers isn't downloaded, and the one kept stays.
    for elsewhere in [
        "http://avatars.slack-edge.com/fixture/T0FIXTURE1_132.png",
        "https://avatars.slack-edge.com.example.com/T0FIXTURE1_132.png",
        "https://notslack-edge.com/T0FIXTURE1_132.png",
        "https://avatars.slack-edge.com:8443/fixture/T0FIXTURE1_132.png",
        "https://someone@avatars.slack-edge.com/fixture/T0FIXTURE1_132.png",
    ] {
        let refused = keep(&operations, &json!({"image_132": elsewhere})).await;
        assert_eq!(refused.unwrap_err().code, "slack_refused", "{elsewhere}");
    }
    assert_eq!(kept(&state, &dsp).await, operations_only);
    // Disconnected, its logo goes with it.
    disconnect(&state, &dsp, OPERATIONS).await;
    assert!(kept(&state, &dsp).await.is_empty());
}

#[test]
fn a_logo_is_kept_only_as_a_small_png_jpeg_or_gif() {
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    assert_eq!(kind(png), Some("image/png"));
    assert_eq!(kind(b"\xff\xd8\xff\xe0\0\x10JFIF"), Some("image/jpeg"));
    assert_eq!(kind(b"GIF89a\x01\0\x01\0"), Some("image/gif"));
    for other in [
        &b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"[..],
        b"<!DOCTYPE html><title>Sign in</title>",
        b"RIFF\0\0\0\0WEBPVP8 ",
        b"",
    ] {
        assert_eq!(kind(other), None);
    }
    let mut large = png.to_vec();
    large.resize(MOST + 1, 0);
    assert_eq!(kind(&large), None);
}
