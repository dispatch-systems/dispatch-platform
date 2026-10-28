//! Reading texts: a text's picture goes only to a member whose access still stands once the phone
//! side has it ready; a driver's work conversation is found by each work phone they texted on.
use super::*;
use crate::backend::{
    phone::{storage::FileStore, support::kept},
    support::{self, at, phoned, phones, saved, switch, texted, today},
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

/// Lets a reader waiting in its open of the pipe at `path` through, if one is: whether one was. A
/// writer opens a pipe without waiting only once a reader is in its open, which counts itself
/// before it waits; it waits on a writer coming, so this writer, closed at once, lets it through.
fn release(path: &Path) -> bool {
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
    // Its bytes, where the phone side keeps them, become a pipe with no writer: the phone side's
    // open of it, as it gets the picture ready, waits until this test lets it through.
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

    // Asked for with their access standing, the picture is got ready until the phone side waits in
    // its open, which no poll gets past; this test polls until then, waking itself, as nothing
    // else would. Let through, it isn't polled again until access has ended.
    let asked = file(&state, &c, support::USE, &added.id);
    tokio::pin!(asked);
    std::future::poll_fn(|cx| {
        if release(&bytes) {
            return Poll::Ready(());
        }
        match asked.as_mut().poll(cx) {
            Poll::Ready(_) => panic!("answered before the phone side opened the picture"),
            Poll::Pending => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
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
}

#[tokio::test]
async fn a_drivers_work_conversation_is_found_by_the_digits_of_each_work_phone_they_texted_on() {
    let (_root, state, dsp) = support::started();
    let (zone, day) = today(&state, &dsp).await;
    let yesterday = day.pred_opt().unwrap();
    // Yesterday Avery had 0101, and today 0202, texting on each.
    let (midnight, then) = (at(zone, day, 0, 0), at(zone, yesterday, 9, 0));
    let earlier = saved(yesterday, &[("A1", "2065550101")]);
    let now = phones(&[("A1", "2065550202")]);
    phoned(&state, &dsp, Some(&day.to_string()), now, vec![earlier]).await;
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
