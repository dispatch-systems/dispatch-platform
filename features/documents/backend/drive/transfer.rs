//! Uploads, downloads and bounded thumbnails from Google's own hosts.
use super::{
    ALL_DRIVES, DOC, FIELDS, Item, SHEET, SLIDES,
    fixture::{described, fixture},
    missing,
};
use crate::backend::google::{FILES, Google, HTTP, Timed, read, send, unreachable};
use axum::body::{Body, Bytes};
use dispatch_core::{
    Error, Result, db::iso, ensure, foundation::crypto, server::http::upload::Upload,
};
use futures_util::TryStreamExt;
use serde_json::json;
use std::{sync::LazyLock, time::Duration};

/// Where a file's bytes go up.
const UPLOADS: &str = "https://www.googleapis.com/upload/drive/v3/files";
/// The longest sides a card's picture is fetched at, in pixels: sharp on an ordinary screen,
/// and on a high-density one.
pub const PICTURE_SIZES: [u32; 2] = [300, 600];
/// The largest picture Dispatch hands on; Google's are a small part of it.
const PICTURE_LIMIT: usize = 2 * 1024 * 1024;
/// How many pictures the server fetches from Google at once, across every DSP.
static PICTURES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(8);
/// The kinds of picture Dispatch hands on.
const PICTURE_KINDS: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];
/// Fetches pictures, following a redirect only to Google's own hosts.
static PICTURE_HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 5 && google(attempt.url()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .expect("the TLS backend is built in")
});
/// The Office files Google's own Docs, Sheets and Slides download as, with their endings.
const DOCX: (&str, &str) = (
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "docx",
);
const XLSX: (&str, &str) = (
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "xlsx",
);
const PPTX: (&str, &str) = (
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "pptx",
);

/// Moves a file's bytes, which takes as long as they do: it gives up only on a Google that
/// stops answering for a minute.
static TRANSFER: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(60))
        .build()
        .expect("the TLS backend is built in")
});

/// A file to save: its content type, the name it's saved as, its length when Google says,
/// and its bytes as they come.
pub struct Download {
    pub kind: String,
    pub name: String,
    pub length: Option<u64>,
    pub body: Body,
}

impl Google {
    /// Makes a Doc named `name` in the folder `parent`, written from the page `html`: Google
    /// turns the page, with the pictures in it, into the Doc. As with what Dispatch makes, its
    /// editors can't share it on.
    pub async fn write(&self, access: &str, name: &str, parent: &str, html: &str) -> Result<Item> {
        if let Self::Fixture = self {
            let made = self.create(access, name, DOC, Some(parent)).await?;
            return fixture(access, |drive| {
                drive
                    .contents
                    .insert(made.id.clone(), html.as_bytes().to_vec());
                Ok(made)
            });
        }
        let metadata = json!({
            "name": name,
            "mimeType": DOC,
            "parents": [parent],
            "writersCanShare": false,
        });
        let (kind, body) = page(&metadata, html)?;
        send(
            TRANSFER
                .post(UPLOADS)
                .bearer_auth(access)
                .query(&[("uploadType", "multipart"), ("fields", FIELDS), ALL_DRIVES])
                .header(reqwest::header::CONTENT_TYPE, kind)
                .body(body),
        )
        .await
    }
    /// Writes the page `html` over the Doc `id`, named `name`, where it is.
    pub async fn rewrite(&self, access: &str, id: &str, name: &str, html: &str) -> Result<Item> {
        if let Self::Fixture = self {
            return fixture(access, |drive| {
                let item = drive
                    .files
                    .get_mut(id)
                    .filter(|item| item.mime_type == DOC)
                    .ok_or_else(missing)?;
                item.name = name.to_owned();
                item.modified_time = iso();
                let item = described(item);
                drive
                    .contents
                    .insert(id.to_owned(), html.as_bytes().to_vec());
                drive.touch(id);
                Ok(item)
            });
        }
        let (kind, body) = page(&json!({"name": name}), html)?;
        send(
            TRANSFER
                .patch(format!("{UPLOADS}/{id}"))
                .bearer_auth(access)
                .query(&[("uploadType", "multipart"), ("fields", FIELDS), ALL_DRIVES])
                .header(reqwest::header::CONTENT_TYPE, kind)
                .body(body),
        )
        .await
    }
    /// Starts uploading `length` bytes of `mime` named `name` into the folder `parent`, and
    /// answers where the bytes go. As with what Dispatch makes, its editors can't share it on.
    pub async fn start_upload(
        &self,
        access: &str,
        name: &str,
        mime: &str,
        parent: &str,
        length: u64,
    ) -> Result<String> {
        if let Self::Fixture = self {
            let session = format!("fixture-upload-{}", crypto::hex(&crypto::random::<8>()?));
            return fixture(access, |drive| {
                let started = (name.to_owned(), mime.to_owned(), parent.to_owned());
                drive.uploads.insert(session.clone(), started);
                Ok(session)
            });
        }
        let response = upload_start(access, name, mime, parent, length)
            .timed()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(read::<serde_json::Value>(response)
                .await
                .err()
                .unwrap_or_else(|| Error::new("google_unreachable", 502)));
        }
        response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|location| location.to_str().ok())
            .map(str::to_owned)
            .ok_or_else(|| Error::new("google_unreachable", 502))
    }
    /// Sends the upload's bytes to where `session` said, and answers the file Drive made, as
    /// the upload's start asked for it.
    pub async fn finish_upload(&self, access: &str, session: &str, upload: Upload) -> Result<Item> {
        let length = upload.length;
        if let Self::Fixture = self {
            let bytes: Vec<u8> = upload
                .stream()
                .map_ok(|chunk| chunk.to_vec())
                .try_concat()
                .await
                .map_err(|_| Error::new("upload_incomplete", 400))?;
            return fixture(access, |drive| drive.finish_upload(session, length, bytes));
        }
        let response = TRANSFER
            .put(session)
            .bearer_auth(access)
            .header(reqwest::header::CONTENT_LENGTH, length)
            .body(reqwest::Body::wrap_stream(upload.stream()))
            .timed()
            .await
            .map_err(|error| {
                // The member's bytes stopped coming, rather than Google answering.
                if error.is_body() || error.is_request() {
                    Error::new("upload_incomplete", 400)
                } else {
                    unreachable(error)
                }
            })?;
        read(response).await
    }
    /// Google's picture of what a file holds, by the `link` a listing gave, sized for a card.
    /// Google refusing it only leaves the card its drawing, so it never counts against the
    /// connection.
    pub async fn picture(&self, access: &str, link: &str, size: u32) -> Result<Picture> {
        if let Self::Fixture = self {
            let id = link.strip_prefix("fixture:").ok_or_else(missing)?;
            return fixture(access, |drive| {
                let item = drive.files.get(id).ok_or_else(missing)?;
                let bytes = drive.contents.get(id).ok_or_else(missing)?;
                ensure(
                    handed(&item.mime_type, bytes.len()),
                    "documents_item_not_found",
                    404,
                )?;
                Ok(Picture {
                    kind: item.mime_type.clone(),
                    bytes: Bytes::from(bytes.clone()),
                })
            });
        }
        let link = sized(link, size).ok_or_else(missing)?;
        let _turn = PICTURES.acquire().await.map_err(|_| missing())?;
        let mut response = PICTURE_HTTP
            .get(link)
            .bearer_auth(access)
            .timed()
            .await
            .map_err(unreachable)?;
        let kind = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|kind| kind.to_str().ok())
            .map(|kind| {
                kind.split(';')
                    .next()
                    .unwrap_or(kind)
                    .trim()
                    .to_ascii_lowercase()
            })
            .unwrap_or_default();
        let length = response
            .content_length()
            .map_or(0, |length| usize::try_from(length).unwrap_or(usize::MAX));
        if !response.status().is_success() || !handed(&kind, length) {
            return Err(missing());
        }
        // Read as it comes, giving up past the limit, whatever length Google stated.
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(unreachable)? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > PICTURE_LIMIT {
                return Err(missing());
            }
        }
        Ok(Picture {
            kind,
            bytes: bytes.into(),
        })
    }
    /// The file `id`, named `name`, of `mime`, to save: Google's own Docs, Sheets and Slides
    /// as the Word, Excel and PowerPoint files they export as, and an uploaded file as it was.
    pub async fn download(
        &self,
        access: &str,
        id: &str,
        name: &str,
        mime: &str,
    ) -> Result<Download> {
        let exported = match mime {
            DOC => Some(DOCX),
            SHEET => Some(XLSX),
            SLIDES => Some(PPTX),
            google if google.starts_with("application/vnd.google-apps.") => {
                return Err(Error::new("documents_not_downloadable", 409));
            }
            _ => None,
        };
        let (kind, saved) = match exported {
            Some((kind, ending)) => (kind.to_owned(), format!("{name}.{ending}")),
            None => (mime.to_owned(), name.to_owned()),
        };
        if let Self::Fixture = self {
            let bytes = fixture(access, |drive| {
                drive.files.get(id).ok_or_else(missing)?;
                Ok(drive
                    .contents
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| format!("Fixture mode's export of {name}").into_bytes()))
            })?;
            return Ok(Download {
                kind,
                name: saved,
                length: Some(bytes.len() as u64),
                body: Body::from(bytes),
            });
        }
        let request = match exported {
            Some((kind, _)) => TRANSFER
                .get(format!("{FILES}/{id}/export"))
                .query(&[("mimeType", kind), ALL_DRIVES]),
            None => TRANSFER
                .get(format!("{FILES}/{id}"))
                .query(&[("alt", "media"), ALL_DRIVES]),
        };
        let response = request
            .bearer_auth(access)
            .timed()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(read::<serde_json::Value>(response)
                .await
                .err()
                .unwrap_or_else(|| Error::new("google_unreachable", 502)));
        }
        Ok(Download {
            kind,
            name: saved,
            length: response.content_length(),
            body: Body::from_stream(response.bytes_stream()),
        })
    }
}

/// The request that starts an upload. Google describes the file it makes with the fields
/// this request asks for, and ignores those the request carrying the bytes asks for.
fn upload_start(
    access: &str,
    name: &str,
    mime: &str,
    parent: &str,
    length: u64,
) -> reqwest::RequestBuilder {
    HTTP.post(UPLOADS)
        .bearer_auth(access)
        .query(&[("uploadType", "resumable"), ("fields", FIELDS)])
        .header("X-Upload-Content-Type", mime)
        .header("X-Upload-Content-Length", length)
        .json(&json!({
            "name": name,
            "mimeType": mime,
            "parents": [parent],
            "writersCanShare": false,
        }))
}

/// A request carrying a file's description `metadata` and the page `html` it's written from,
/// as Google takes both at once: its content type, and its body, the two parted by a boundary
/// the page can't hold.
fn page(metadata: &serde_json::Value, html: &str) -> Result<(String, Vec<u8>)> {
    let boundary = format!("page-{}", crypto::hex(&crypto::random::<16>()?));
    let mut body = format!(
        "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{metadata}\r\n\
         --{boundary}\r\nContent-Type: text/html; charset=UTF-8\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(html.as_bytes());
    body.extend_from_slice(format!("\r\n--{boundary}--").as_bytes());
    Ok((format!("multipart/related; boundary={boundary}"), body))
}

/// A picture of what a file holds, as Google sent it.
#[derive(Clone)]
pub struct Picture {
    pub kind: String,
    pub bytes: Bytes,
}
/// Whether a picture of `kind` and `length` bytes is one Dispatch hands on.
fn handed(kind: &str, length: usize) -> bool {
    PICTURE_KINDS.contains(&kind) && length <= PICTURE_LIMIT
}
/// Whether `url` is Google's own, over HTTPS: the only place a picture is fetched from, since
/// the account's token goes with it.
fn google(url: &reqwest::Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    url.scheme() == "https"
        && (host.ends_with(".googleusercontent.com")
            || ["docs.google.com", "drive.google.com"].contains(&host))
}
/// Google's link to a picture, asking for it `size` pixels on its longest side: only a link of
/// Google's own.
fn sized(link: &str, size: u32) -> Option<String> {
    if !google(&reqwest::Url::parse(link).ok()?) {
        return None;
    }
    // Google names the size at the link's end, `=s220`.
    Some(match link.rsplit_once("=s") {
        Some((start, given)) if !given.is_empty() && given.bytes().all(|b| b.is_ascii_digit()) => {
            format!("{start}=s{size}")
        }
        _ => link.to_owned(),
    })
}

#[cfg(test)]
#[path = "../../tests/backend/drive/transfer.rs"]
mod tests;
