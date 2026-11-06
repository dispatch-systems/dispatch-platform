//! HTTP replies for attachment previews and byte-version caching. Callers keep record lookup,
//! upload ownership, original-byte length/error policy and post-wait access checks.
use super::{
    bytes::{self, Folder},
    previews,
};
use dispatch_core::{Result, State, server::http::input::Reply};
use std::sync::Arc;

/// The preview of the DSP's picture kept by `sha256` in `folder`, as `mime` named `name`, for a
/// page to show: none for a file that isn't a picture, or a picture with none.
pub async fn preview(
    state: &Arc<State>,
    folder: Folder,
    dsp: &str,
    sha256: &str,
    name: &str,
    mime: &str,
) -> Result<Option<Reply>> {
    if !mime.starts_with("image/") {
        return Ok(None);
    }
    Ok(previews::preview(state, folder, dsp, sha256)
        .await?
        .map(|small| {
            Reply::download(
                previews::MIME,
                name,
                small.length,
                axum::body::Body::from_stream(small.chunks),
            )
        }))
}

/// `reply`, the bytes `sha256`, kept by the browser when the address asked for their version,
/// `asked`: another version comes at another address.
pub fn as_asked(reply: Reply, asked: Option<&str>, sha256: &str) -> Reply {
    if asked.is_some_and(|asked| asked == bytes::version(sha256)) {
        reply.kept()
    } else {
        reply
    }
}
