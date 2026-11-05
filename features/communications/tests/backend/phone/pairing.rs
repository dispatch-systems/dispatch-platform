//! A code reads the same however it's typed, and only a code of its letters is one. A code tried is
//! told right or wrong by a read, writing nothing; an address, and a DSP, that guessed too many
//! wrong wait, a right code too, however many of its guesses answer together, and however many other
//! addresses guessed lately.
use super::{ADDRESS_GUESSES, DSP_GUESSES, Guessed, MOST_ADDRESSES, Tried, kept, new_code, opens};
use crate::backend::{
    phone::storage::DeviceStore,
    support::{cached, started, unwritten},
};
use dispatch_core::{State, db::now, testing};
use std::sync::Arc;

#[test]
fn a_code_is_kept_however_it_is_typed() {
    let code = new_code().unwrap();
    let typed = format!("{}-{}", &code[..4], &code[4..]).to_lowercase();
    assert_eq!(kept(&typed), kept(&code));
    assert!(kept(&code).is_some());
    assert_eq!(kept("O0I1O0I1"), None, "letters a code never has");
    assert_eq!(kept("ABCD"), None, "too short");
}

/// A code that pairs the next phone at the DSP, made by its owner: as it's kept.
async fn made(state: &Arc<State>, dsp: &str) -> String {
    let (at, code) = (dsp.to_owned(), kept(&new_code().unwrap()).unwrap());
    let kept = code.clone();
    state
        .run(move |db| {
            let owner = testing::a_user(db);
            db.phones_new_pairing(&at, &owner, &kept, now())
        })
        .await
        .unwrap();
    code
}
/// What trying `code` at the DSP from the address `ip` comes to: the DSP, or why not.
async fn tried(state: &Arc<State>, dsp: &str, ip: &str, code: &str) -> Result<String, String> {
    let at = dsp.to_owned();
    opens(state, ip, move |db| db.find_dsp(&at), code.to_owned())
        .await
        .map(|dsp| dsp.id)
        .map_err(|error| error.code.to_string())
}

#[tokio::test]
async fn a_wrong_code_is_told_by_a_read_and_writes_nothing() {
    let (_root, state, dsp) = started();
    let code = made(&state, &dsp).await;
    let kept_read = cached(&state, &dsp);
    let wrong = kept("ABCD-EFGH").unwrap();
    assert_eq!(
        unwritten(&state, tried(&state, &dsp, "198.51.100.1", &wrong)).await,
        Err("phones_pairing_invalid".into())
    );
    assert!(kept_read(), "the DSP's cached reads were evicted");
    assert_eq!(tried(&state, &dsp, "198.51.100.1", &code).await, Ok(dsp));
}

#[tokio::test]
async fn an_address_and_a_dsp_that_guessed_too_many_wrong_wait_a_right_code_too() {
    let (_root, state, dsp) = started();
    let code = made(&state, &dsp).await;
    let wrong = kept("ABCD-EFGH").unwrap();
    for _ in 0..ADDRESS_GUESSES {
        let guessed = tried(&state, &dsp, "198.51.100.2", &wrong).await;
        assert_eq!(guessed, Err("phones_pairing_invalid".into()));
    }
    // That address waits, the right code too; another may still pair.
    let waits = Err("rate_limited".into());
    assert_eq!(tried(&state, &dsp, "198.51.100.2", &code).await, waits);
    assert_eq!(
        tried(&state, &dsp, "198.51.100.3", &code).await,
        Ok(dsp.clone())
    );
    // Guessed from address after address, as many as each may, the DSP waits, wherever a code
    // comes from.
    for guess in ADDRESS_GUESSES..DSP_GUESSES {
        let ip = format!("198.51.100.{}", 10 + guess / ADDRESS_GUESSES);
        let guessed = tried(&state, &dsp, &ip, &wrong).await;
        assert_eq!(guessed, Err("phones_pairing_invalid".into()));
    }
    assert_eq!(tried(&state, &dsp, "198.51.100.200", &code).await, waits);
}

#[tokio::test]
async fn guesses_from_an_address_answering_together_count_against_it_all_the_same() {
    let (_root, state, dsp) = started();
    made(&state, &dsp).await;
    let wrong = kept("ABCD-EFGH").unwrap();
    // A write holds the lock, as another DSP's may: each guess passes the address's first check and
    // waits on the database behind it, more than the address may make.
    let (began, holding) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let write = tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            state
                .run_bookkeeping(move |_| {
                    let _ = began.send(());
                    let _ = wait.recv();
                    Ok(())
                })
                .await
        }
    });
    holding.await.unwrap();
    let free = state.db_queue.available_permits();
    let guesses: Vec<_> = (0..=ADDRESS_GUESSES)
        .map(|_| {
            let (state, dsp, wrong) = (Arc::clone(&state), dsp.clone(), wrong.clone());
            tokio::spawn(State::patiently(async move {
                tried(&state, &dsp, "198.51.100.30", &wrong).await
            }))
        })
        .collect();
    while state.db_queue.available_permits() > free - guesses.len() {
        tokio::task::yield_now().await;
    }
    release.send(()).unwrap();
    write.await.unwrap().unwrap();
    let mut answers = Vec::new();
    for guess in guesses {
        answers.push(guess.await.unwrap().unwrap_err());
    }
    let count = |code: &str| answers.iter().filter(|each| *each == code).count();
    assert_eq!(
        count("phones_pairing_invalid"),
        ADDRESS_GUESSES,
        "{answers:?}"
    );
    assert_eq!(count("rate_limited"), 1, "the rest waits: {answers:?}");
}

#[test]
fn a_new_address_is_counted_though_every_address_remembered_guessed_lately() {
    let mut guessed = Guessed::default();
    let at = 1_800_000_000_000;
    for each in 0..MOST_ADDRESSES {
        guessed.wrong(&format!("10.0.{}.{}", each / 256, each % 256), "dsp_a", at);
    }
    for _ in 0..ADDRESS_GUESSES {
        guessed.wrong("198.51.100.9", "dsp_b", at + 1);
    }
    assert!(
        guessed
            .waits(Tried::Address("198.51.100.9"), at + 2)
            .is_err()
    );
    assert_eq!(guessed.addresses.len(), MOST_ADDRESSES);
}
