//! What Documents' backend tests share: a platform with one DSP, its Documents connected to
//! fixture mode's Google, and disabling and enabling it.
use super::{
    google::Google,
    storage::{Connection, DocumentsStore},
};
use dispatch_core::{
    State,
    db::{Store, iso},
    testing,
};
use std::sync::Arc;

/// The account fixture mode's Google signs in as, holding the DSP's Documents.
pub const ACCOUNT: &str = "fixture:documents@example.test";

/// A platform with one DSP, Documents on and not yet connected, and its server's state.
pub fn platform() -> (impl Drop, Arc<State>, String) {
    testing::install(&[], &[&crate::FEATURE]);
    let (root, store, dsp) = testing::bootstrapped();
    let config = store.config.clone();
    drop(store);
    (root, State::new(config).unwrap(), dsp)
}
/// Connects the DSP's Documents to fixture mode's Google, with a main folder of its own.
pub async fn connect(state: &Arc<State>, dsp: &str) {
    connect_as(state, dsp, ACCOUNT).await;
}
/// Connects the DSP's Documents as `connect` does, signed in as `account`, `fixture:<email>`.
pub async fn connect_as(state: &Arc<State>, dsp: &str, account: &str) {
    let google = Google::of(&state.config).unwrap();
    let granted = google
        .exchange(&state.config.origin, account, "-")
        .await
        .unwrap();
    let folder_id = google
        .create_folder(&granted.access, "Test Documents")
        .await
        .unwrap();
    let connection = Connection {
        broken: false,
        full: true,
        account: granted.account,
        folder_id,
        folder_name: "Test Documents".into(),
        connected_by: "user_1".into(),
        connected_at: iso(),
    };
    let (at, refresh) = (dsp.to_owned(), granted.refresh);
    state
        .run(move |db| db.save_documents_connection(&at, &connection, &refresh))
        .await
        .unwrap();
}
/// Disables the DSP, as the platform owner does, when it's called.
pub fn disabling(state: &Arc<State>, dsp: &str) -> impl FnOnce() + Send + 'static {
    let (config, key, dsp) = (state.config.clone(), state.key.clone(), dsp.to_owned());
    move || {
        let db = Store::open(config, key).unwrap();
        testing::set_dsp_status(&db, &dsp, "suspended").unwrap();
    }
}
pub async fn enable(state: &Arc<State>, dsp: &str) {
    let at = dsp.to_owned();
    state
        .run(move |db| testing::set_dsp_status(db, &at, "active"))
        .await
        .unwrap();
}

/// An authenticated fixture owner's current Documents context.
pub async fn context(state: &Arc<State>, dsp: &str) -> dispatch_core::accounts::Context {
    use dispatch_core::{accounts::SessionLifetime, foundation::config::Site};
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
    let at = dsp.to_owned();
    state
        .read(move |db| {
            let auth = db.authenticate(&raw, &Site::Admin)?;
            db.context(&auth, &at, "documents.use")
        })
        .await
        .unwrap()
}
