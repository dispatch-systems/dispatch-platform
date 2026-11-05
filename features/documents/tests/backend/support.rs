//! What Documents' backend tests share: a platform with one DSP, and its Documents connected
//! to fixture mode's Google.
use super::{
    google::Google,
    storage::{Connection, DocumentsStore},
};
use dispatch_core::{State, db::iso, testing};
use std::sync::Arc;

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
    let google = Google::of(&state.config).unwrap();
    let granted = google
        .exchange(&state.config.origin, "fixture:documents@example.test", "-")
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
