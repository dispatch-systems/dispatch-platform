//! A code reads the same however it's typed, and only a code of its letters is one. A code tried is
//! told right or wrong by a read, writing nothing; an address, and a DSP, that guessed too many
//! wrong wait, a right code too.
use super::{ADDRESS_GUESSES, DSP_GUESSES, kept, new_code, opens};
use crate::backend::{
    phone::storage::DeviceStore,
    support::{cached, started},
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
        tried(&state, &dsp, "198.51.100.1", &wrong).await,
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
