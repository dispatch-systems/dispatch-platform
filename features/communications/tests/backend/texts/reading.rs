//! Reading texts: a text's picture goes only to a member whose access still stands once the phone
//! side has it ready; a driver's work conversation is found by each work phone they texted on.
use super::*;
use crate::backend::{
    phone::{storage::FileStore, support::kept},
    support::{self, at, held, hold, switch, texted, today},
    texts::reply,
};
use axum::body::Bytes;
use dispatch_core::server::http::upload::Upload;
use futures_util::stream;
use std::{
    ffi::CString,
    fs::OpenOptions,
    os::unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::Path,
    task::Poll,
};

/// Whether something has the pipe at `path` open to read from: only then may a writer open it
/// without waiting.
fn read_open(path: &Path) -> bool {
    match OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
    {
        Ok(_) => true,
        Err(error) if error.raw_os_error() == Some(libc::ENXIO) => false,
        Err(error) => panic!("{error}"),
    }
}

#[tokio::test]
async fn a_texts_picture_is_refused_once_the_members_access_ended_while_the_phone_side_got_it_ready()
 {
    let (_root, state, dsp) = support::started();
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let chunks = stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(b"png"))]);
    let photo = Upload::streamed(3, u64::MAX, chunks).await.unwrap();
    let added = reply::upload(&state, &c, support::USE, "door.png", "image/png", photo)
        .await
        .unwrap();
    // Its bytes, where the phone side keeps them, become a pipe, a writer waiting on it: the phone
    // side opening it, as it gets the picture ready, is then seen from here.
    let (at_dsp, id) = (dsp.clone(), added.id.clone());
    let bytes = state
        .read(move |db| {
            let sha256 = db.phones_file(&at_dsp, &id)?.unwrap().sha256.unwrap();
            Ok(kept(db, &at_dsp).join(sha256))
        })
        .await
        .unwrap();
    std::fs::remove_file(&bytes).unwrap();
    let path = CString::new(bytes.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let waiting = bytes.clone();
    let writer = std::thread::spawn(move || OpenOptions::new().write(true).open(waiting));

    // Asked for with their access standing, the picture is got ready until the phone side has it
    // open, and held there.
    let asked = file(&state, &c, support::USE, &added.id);
    tokio::pin!(asked);
    std::future::poll_fn(|cx| {
        if read_open(&bytes) {
            return Poll::Ready(());
        }
        match asked.as_mut().poll(cx) {
            Poll::Ready(_) => panic!("answered before the phone side opened the picture"),
            Poll::Pending => Poll::Pending,
        }
    })
    .await;
    // Communications goes off for the DSP meanwhile, and with it their permission: once the
    // picture is ready, it's refused them.
    switch(&state, &dsp, "communications", false).await;
    let refused = asked.await.err();
    assert_eq!(
        refused.map(|error| error.code).as_deref(),
        Some("permission_denied")
    );
    writer.join().unwrap().unwrap();
}

#[tokio::test]
async fn a_drivers_work_conversation_is_found_by_the_digits_of_each_work_phone_they_texted_on() {
    let (_root, state, dsp) = support::started();
    let (zone, day) = today(&state, &dsp).await;
    let yesterday = day.pred_opt().unwrap();
    // Yesterday Avery held 0101, and today 0202, texting on each.
    let (midnight, then) = (at(zone, day, 0, 0), at(zone, yesterday, 9, 0));
    let earlier = (yesterday.to_string(), hold("A1", "2065550101", then, None));
    let now_held = vec![hold("A1", "2065550202", midnight, None)];
    held(
        &state,
        &dsp,
        Some(&day.to_string()),
        now_held,
        vec![earlier],
    )
    .await;
    texted(&state, &dsp, "+12065550101", "Leaving the station", then).await;
    texted(&state, &dsp, "+12065550202", "Which van today?", midnight).await;

    let found = |typed: &'static str| {
        let (shared, dsp) = (state.clone(), dsp.clone());
        state.read(move |db| {
            let found = search(db, &shared, &dsp, ConversationKind::Work, typed)?;
            Ok(found
                .conversations
                .into_iter()
                .filter_map(|each| each.driver_id)
                .collect::<Vec<_>>())
        })
    };
    assert_eq!(found("555-0202").await.unwrap(), ["A1"]);
    assert_eq!(found("0101").await.unwrap(), ["A1"]);
    assert!(found("0303").await.unwrap().is_empty());
}
