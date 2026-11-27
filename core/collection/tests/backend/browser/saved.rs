//! A saved session keeps each cookie only until it expires, and one saved before cookies kept
//! their expiry no longer opens. A session the provider refused goes only at the revision it was
//! saved under, and a job that lost its authority as the provider refused it leaves the saved
//! session as it is and starts no browser.
use super::super::{ProviderAuthority, http::Cookie, tests::registered};
use super::*;
use crate::{State, collection::metrics::SavedSession, testing};
use serde_json::{Value, json};

const PROVIDER: Provider = Provider::new("probe_site");

fn cookie(name: &str, expires: Option<i64>) -> Cookie {
    Cookie {
        name: name.into(),
        value: "1".into(),
        domain: ".amazon.com".into(),
        path: "/".into(),
        secure: true,
        expires,
    }
}
fn session(cookies: Vec<Cookie>) -> SignedIn {
    SignedIn {
        origin: "https://logistics.amazon.com".into(),
        cookies,
        user_agent: "Browser".into(),
    }
}
/// Writes `saved` as the session's file at revision 1, just as it is.
fn write(db: &Store, dsp: &str, saved: &Value) {
    let sealed = crypto::encrypt(
        &db.vault_key(dsp).unwrap(),
        &binding(dsp, PROVIDER, 1),
        saved,
    )
    .unwrap();
    db::private_dir(&db.area(dsp, "state").unwrap().join("browsers")).unwrap();
    db::write_private(
        &db.saved_session_path(dsp, PROVIDER).unwrap(),
        sealed.as_bytes(),
    )
    .unwrap();
}
/// Its cookies by name, with when each expires.
fn expiring(session: &SignedIn) -> Vec<(&str, Option<i64>)> {
    session
        .cookies
        .iter()
        .map(|cookie| (cookie.name.as_str(), cookie.expires))
        .collect()
}

#[test]
fn a_saved_cookie_past_its_expiry_is_neither_saved_nor_restored() {
    testing::install(&[], &[]);
    let (_root, db, dsp) = testing::bootstrapped();
    let now = db::now();
    let (past, future) = (now - 60_000, now + 3_600_000);
    let cookies = vec![
        cookie("expired", Some(past)),
        cookie("lasting", Some(future)),
        cookie("session", None),
    ];
    let kept = [("lasting", Some(future)), ("session", None)];
    db.seal(&dsp, PROVIDER, 1, &session(cookies.clone()))
        .unwrap();
    let saved = db.opened(&dsp, PROVIDER, 1).unwrap().unwrap();
    assert_eq!(expiring(&saved.session), kept);
    // One that expired after it was saved is not restored either.
    let saved = Saved {
        revision: 1,
        session: session(cookies),
    };
    write(&db, &dsp, &serde_json::to_value(saved).unwrap());
    let restored = db.saved_session(&dsp, PROVIDER, 1).unwrap().unwrap();
    assert_eq!(expiring(&restored), kept);
}

#[test]
fn a_session_saved_before_cookies_kept_their_expiry_is_removed() {
    testing::install(&[], &[]);
    let (_root, db, dsp) = testing::bootstrapped();
    let cookie = json!({"name":"session-token","value":"1","domain":".amazon.com","path":"/",
        "secure":true});
    let session = json!({"origin":"https://logistics.amazon.com","cookies":[cookie],
        "userAgent":"Browser"});
    write(&db, &dsp, &json!({"revision":1,"session":session}));
    assert!(db.saved_session(&dsp, PROVIDER, 1).unwrap().is_none());
    assert!(!db.saved_session_path(&dsp, PROVIDER).unwrap().exists());
}

#[test]
fn a_refused_session_goes_only_at_the_revision_it_was_saved_under() {
    testing::install(&[], &[]);
    let (_root, db, dsp) = testing::bootstrapped();
    let saved = session(vec![cookie("session-token", None)]);
    db.seal(&dsp, PROVIDER, 2, &saved).unwrap();
    db.discard_session_at(&dsp, PROVIDER, 1).unwrap();
    assert!(db.saved_session(&dsp, PROVIDER, 2).unwrap().is_some());
    db.discard_session_at(&dsp, PROVIDER, 2).unwrap();
    assert!(!db.saved_session_path(&dsp, PROVIDER).unwrap().exists());
}

#[tokio::test]
async fn a_revoked_jobs_refused_session_leaves_the_saved_session_and_starts_no_browser() {
    testing::install(&[], &[]);
    let (_root, db, dsp) = testing::bootstrapped();
    let state = State::new(db.config.clone()).unwrap();
    // A sign-in saved this while the job's check of the session it started from waited.
    db.seal(
        &dsp,
        PROVIDER,
        1,
        &session(vec![cookie("session-token", None)]),
    )
    .unwrap();
    let job = registered(&state, &dsp, None, true);
    job.became(SavedSession::Browserless);
    // A job no worker holds, as one cancelled while its check waited.
    let authority = ProviderAuthority::Job {
        id: "job_saved".into(),
        owner: "worker".into(),
    };
    let refused = state.refused(&job, &authority, false).await.unwrap_err();
    assert_eq!(refused.code, "job_not_found");
    assert!(db.saved_session(&dsp, PROVIDER, 1).unwrap().is_some());
    assert!(job.browserless());
    assert_eq!(job.saved_session(), Some(SavedSession::Browserless));
}
