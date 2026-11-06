//! File streams and bounded public pictures, with their distinct HTTP policies.
use super::{Bytestream, Slack, timed, unreachable};
use crate::backend::fixture;
use dispatch_core::{Error, Result};
use futures_util::TryStreamExt;
use std::{sync::LazyLock, time::Duration};

/// Downloads a file's bytes for as long as they take: only connecting is timed, as how long a
/// file takes depends on its size, and whatever the bytes feed gives up once they stall.
static DOWNLOADS: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .build()
        .expect("the TLS backend is built in")
});
/// Where Slack keeps the files its API names: a download from anywhere else is refused.
const FILES_HOST: &str = "https://files.slack.com/";
/// Downloads a picture from the address Slack names alone: one that sends it elsewhere is
/// refused, not followed.
static PICTURES: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("the TLS backend is built in")
});

impl Slack {
    /// Downloads the file of type `mime` Slack keeps at `url`, its `url_private_download`, as
    /// the bot whose token this is: its bytes as they arrive. Slack answers a token that may
    /// not read the file with its sign-in page, which is refused as a permission the workspace
    /// hasn't approved yet.
    pub async fn download(&self, token: &str, url: &str, mime: &str) -> Result<Bytestream> {
        if let Self::Fixture = self {
            return fixture::download(url);
        }
        if !url.starts_with(FILES_HOST) {
            return Err(Error::new("slack_refused", 502));
        }
        let response = timed("files.download", DOWNLOADS.get(url).bearer_auth(token))
            .await
            .map_err(unreachable)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::new("slack_file_gone", 404));
        }
        if !status.is_success() {
            return Err(Error::new("slack_unreachable", 502));
        }
        let signing_in = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|kind| kind.to_str().ok())
            .is_some_and(|kind| kind.starts_with("text/html"))
            && !mime.starts_with("text/html");
        if signing_in {
            return Err(Error::new("slack_needs_update", 409));
        }
        Ok(Box::pin(
            response.bytes_stream().map_err(std::io::Error::other),
        ))
    }
    /// Downloads the picture Slack keeps at `url`, one of its own addresses that needs no token,
    /// as a workspace's logo's is: its bytes, at most `most` of them, else it's refused.
    pub async fn picture(&self, url: &url::Url, most: usize) -> Result<Vec<u8>> {
        if let Self::Fixture = self {
            return fixture::picture(url.as_str());
        }
        let response = timed("team.icon", PICTURES.get(url.clone()))
            .await
            .map_err(unreachable)?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(Error::new("slack_unreachable", 502));
        }
        let too_large = || Error::new("slack_refused", 502);
        if response
            .content_length()
            .is_some_and(|length| length > most as u64)
        {
            return Err(too_large());
        }
        let mut picture = Vec::new();
        let mut bytes = response.bytes_stream();
        while let Some(chunk) = bytes.try_next().await.map_err(unreachable)? {
            if picture.len() + chunk.len() > most {
                return Err(too_large());
            }
            picture.extend_from_slice(&chunk);
        }
        Ok(picture)
    }
    /// Sends a file's `length` bytes to `url`, where `files.getUploadURLExternal` said it goes,
    /// as they arrive. The address is Slack's own for this one file, so it needs no token.
    pub async fn upload(&self, url: &str, length: u64, bytes: Bytestream) -> Result<()> {
        if let Self::Fixture = self {
            return fixture::upload(url, bytes).await;
        }
        if !url.starts_with(FILES_HOST) {
            return Err(Error::new("slack_refused", 502));
        }
        let request = DOWNLOADS
            .post(url)
            .header(reqwest::header::CONTENT_LENGTH, length)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(reqwest::Body::wrap_stream(bytes));
        let response = timed("files.upload", request).await.map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(Error::new("slack_unreachable", 502));
        }
        Ok(())
    }
}
