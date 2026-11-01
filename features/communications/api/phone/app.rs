//! The app itself, as the DSP's address hands it out: its latest version's details, which the app
//! checks an update against, and its APK, which a phone downloads to install it or update.
use super::access::Address;
use crate::backend::{
    bytes::Reading,
    phone::app::{self, App},
};
use dispatch_core::{
    Error, Result, State,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::Grant,
    },
};
use serde::Serialize;
use std::sync::Arc;

/// What an APK is, to the browser or app that downloads it.
const APK: &str = "application/vnd.android.package-archive";

/// The latest version's details: its version, the `versionCode` Android compares, and its APK's
/// size and SHA-256, which the app checks a download against.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Details {
    version: String,
    version_code: u32,
    size: u64,
    sha256: String,
}

/// The latest version's details.
pub(super) async fn details(state: Arc<State>, input: Input, access: Address) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let app = handed_out(&state, &input, access).await?;
    Reply::of(&Details {
        version: app.version,
        version_code: app.version_code,
        size: app.size,
        sha256: app.sha256,
    })
}
/// The latest version's APK, as a download of its name: asked again, once it's open, whether the
/// DSP still hands it out.
pub(super) async fn download(state: Arc<State>, input: Input, access: Address) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let app = handed_out(&state, &input, access).await?;
    let file = tokio::fs::File::open(&app.path)
        .await
        .map_err(|_| Error::new("phones_app_not_found", 404))?;
    let apk = Reading::of(file).await;
    let asked = input.clone();
    state.read(move |db| access.authorize(db, &asked)).await?;
    let body = axum::body::Body::from_stream(apk.chunks);
    Ok(Reply::download(APK, &app.file, apk.length, body))
}
/// The app the DSP's address hands out: refused as `phones_app_not_found` while the server has
/// none.
async fn handed_out(state: &Arc<State>, input: &Input, access: Address) -> Result<App> {
    let asked = input.clone();
    state
        .read(move |db| {
            access.authorize(db, &asked)?;
            app::latest(&db.config).ok_or_else(|| Error::new("phones_app_not_found", 404))
        })
        .await
}
