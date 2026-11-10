//! The platform's object store: a bucket of Cloudflare R2, spoken to as S3 is (path-style
//! addresses, region `auto`, each request signed with Signature Version 4), where what a DSP keeps
//! off this server's disk is kept. Every key is the server's own, `dsps/<dsp>/<owner>/<sha256>`:
//! a DSP's ID it checked, the owner's name, and the SHA-256 of the bytes, never what a request
//! named, so one DSP's objects are never another's. Bytes go up streamed from their file and
//! come down streamed, a request given up when R2 stops answering for half a minute: a download
//! when a read of its bytes waits that long, however long its reader waits between reads. An
//! upload is given up when it sends nothing for half a minute, R2 doesn't answer half a minute
//! after it sent all, or after ten minutes whatever it does. Its key's secret signs requests and goes
//! nowhere else: a failure is its code alone.
use crate::{
    Error, Result,
    db::{identifier, now},
    ensure,
    foundation::{config::ObjectStore, crypto},
};
use axum::body::Bytes;
use futures_util::{Stream, StreamExt, stream};
use reqwest::{Method, StatusCode, header};
use sha2::{Digest, Sha256};
use std::{path::Path, pin::Pin, time::Duration};
use tokio::io::AsyncReadExt;

/// The region R2 is signed for: it has one.
const REGION: &str = "auto";
/// What a request's signature says of a body it doesn't hash, as one streamed over TLS.
const UNSIGNED: &str = "UNSIGNED-PAYLOAD";
/// The signed headers, by name, in order.
const SIGNED: &str = "host;x-amz-content-sha256;x-amz-date";
/// How many bytes of a file are read at a time as it's sent.
const CHUNK: usize = 64 * 1024;
/// How long an upload may take, as long as the largest file takes on a slow link.
pub const UPLOAD: Duration = Duration::from_secs(10 * 60);
/// How long R2 may go without answering, and an upload without sending any of its bytes.
const STALLED: Duration = Duration::from_secs(30);

/// The bytes of an object, as they come down.
pub struct Fetched {
    /// How many there are, when R2 says.
    pub length: Option<u64>,
    pub chunks: Pin<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send>>,
}

/// The bucket, and its clients: one for the bytes of objects, going up or coming down, timed as
/// they're waited on, and one for the rest, which R2 answers at once.
pub struct Objects {
    store: ObjectStore,
    client: reqwest::Client,
    bytes: reqwest::Client,
}
impl Objects {
    pub fn new(store: &ObjectStore) -> Result<Self> {
        let client = |reading: Option<Duration>| {
            let builder = reqwest::Client::builder()
                .user_agent("Dispatch-Objects/1.0")
                .connect_timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none());
            match reading {
                Some(stalled) => builder.read_timeout(stalled),
                None => builder,
            }
            .build()
            .map_err(|_| Error::new("object_store_unavailable", 503))
        };
        Ok(Self {
            store: store.clone(),
            client: client(Some(STALLED))?,
            bytes: client(None)?,
        })
    }
    /// Whether the bytes it keeps may leave the server's disk (`ObjectStore::only`).
    pub fn only(&self) -> bool {
        self.store.only
    }
    /// What tells this bucket from any other, by where it answers and its name, never its key:
    /// the same each time the server starts with it, another for another bucket or account.
    pub fn identity(&self) -> String {
        crypto::sha(format!("{}\n{}", self.store.endpoint, self.store.bucket))[..16].to_owned()
    }
    /// Keeps the bytes of `file` as the DSP's `owner`'s by their `sha256`, in place of any kept
    /// by it before.
    pub async fn put(&self, dsp: &str, owner: &str, sha256: &str, file: &Path) -> Result<()> {
        let key = key(dsp, owner, sha256)?;
        let file = tokio::fs::File::open(file).await?;
        let length = file.metadata().await?.len();
        let chunks = stream::unfold(file, |mut file| async move {
            let mut chunk = vec![0; CHUNK];
            match file.read(&mut chunk).await {
                Ok(0) => None,
                Ok(read) => {
                    chunk.truncate(read);
                    Some((Ok::<_, std::io::Error>(Bytes::from(chunk)), file))
                }
                Err(error) => Some((Err(error), file)),
            }
        });
        self.upload(&key, length, chunks).await
    }
    /// Keeps the `length` bytes `chunks` at `key`, as they're sent: given up once it sends none
    /// of them for `STALLED`, R2 doesn't answer within `STALLED` of the last, or after `UPLOAD`.
    async fn upload(
        &self,
        key: &str,
        length: u64,
        chunks: impl Stream<Item = std::io::Result<Bytes>> + Send + 'static,
    ) -> Result<()> {
        let (sent, mut sending) = tokio::sync::watch::channel(());
        let chunks = chunks.inspect(move |_| {
            sent.send_replace(());
        });
        let response = self
            .request(&self.bytes, Method::PUT, key, &[], UNSIGNED)?
            .header(header::CONTENT_LENGTH, length)
            .timeout(UPLOAD)
            .body(reqwest::Body::wrap_stream(chunks))
            .send();
        tokio::pin!(response);
        loop {
            tokio::select! {
                biased;
                response = &mut response => {
                    return answered(response, &[StatusCode::OK]).map(drop);
                }
                moved = tokio::time::timeout(STALLED, sending.changed()) => match moved {
                    Ok(Ok(())) => {}
                    // All sent, as their stream is let go of: R2 answers.
                    Ok(Err(_)) => {
                        let response = tokio::time::timeout(STALLED, &mut response)
                            .await
                            .map_err(|_| Error::new("object_store_unavailable", 503))?;
                        return answered(response, &[StatusCode::OK]).map(drop);
                    }
                    Err(_) => return Err(Error::new("object_store_unavailable", 503)),
                },
            }
        }
    }
    /// The bytes the DSP's `owner` keeps by `sha256`, as they come down: none when it keeps none
    /// by it. Given up when R2 doesn't answer within `STALLED`, and once it has, when a read of
    /// its bytes waits that long, timed from when it's read: the time between reads is the
    /// reader's own.
    pub async fn get(&self, dsp: &str, owner: &str, sha256: &str) -> Result<Option<Fetched>> {
        let key = key(dsp, owner, sha256)?;
        let request = self
            .request(&self.bytes, Method::GET, &key, &[], &crypto::sha(""))?
            .send();
        let response = tokio::time::timeout(STALLED, request)
            .await
            .map_err(|_| Error::new("object_store_unavailable", 503))?;
        let response = answered(response, &[StatusCode::OK, StatusCode::NOT_FOUND])?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(Fetched {
            length: response.content_length(),
            chunks: Box::pin(stream::try_unfold(response, |mut response| async move {
                let chunk = tokio::time::timeout(STALLED, response.chunk())
                    .await
                    .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?
                    .map_err(std::io::Error::other)?;
                Ok(chunk.map(|chunk| (chunk, response)))
            })),
        }))
    }
    /// Lets go of the bytes the DSP's `owner` keeps by `sha256`, once they're gone already too.
    pub async fn delete(&self, dsp: &str, owner: &str, sha256: &str) -> Result<()> {
        self.delete_key(&key(dsp, owner, sha256)?).await
    }
    /// The SHA-256 of every bytes the DSP's `owner` keeps.
    pub async fn listed(&self, dsp: &str, owner: &str) -> Result<Vec<String>> {
        let prefix = owned(dsp, owner)?;
        Ok(self
            .keys(&prefix)
            .await?
            .into_iter()
            .filter_map(|key| key.strip_prefix(&prefix).map(String::from))
            .collect())
    }
    /// Every bytes the DSP keeps, of every owner: each owner's name with their SHA-256.
    pub async fn kept(&self, dsp: &str) -> Result<Vec<(String, String)>> {
        let prefix = of_dsp(dsp)?;
        Ok(self
            .keys(&prefix)
            .await?
            .into_iter()
            .filter_map(|key| {
                let (owner, sha256) = key.strip_prefix(&prefix)?.split_once('/')?;
                Some((owner.to_owned(), sha256.to_owned()))
            })
            .collect())
    }
    /// Lets go of everything the DSP keeps, of every owner, and nothing of another DSP's.
    pub async fn delete_dsp(&self, dsp: &str) -> Result<()> {
        for key in self.keys(&of_dsp(dsp)?).await? {
            self.delete_key(&key).await?;
        }
        Ok(())
    }

    async fn delete_key(&self, key: &str) -> Result<()> {
        let response = self
            .request(&self.client, Method::DELETE, key, &[], &crypto::sha(""))?
            .send()
            .await;
        answered(
            response,
            &[
                StatusCode::NO_CONTENT,
                StatusCode::OK,
                StatusCode::NOT_FOUND,
            ],
        )
        .map(drop)
    }
    /// Every key that begins with `prefix`, a page of them at a time (ListObjectsV2).
    async fn keys(&self, prefix: &str) -> Result<Vec<String>> {
        let mut keys = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let mut query = vec![("list-type", "2"), ("prefix", prefix)];
            if let Some(token) = &after {
                query.push(("continuation-token", token));
            }
            let response = self
                .request(&self.client, Method::GET, "", &query, &crypto::sha(""))?
                .send()
                .await;
            let listing = answered(response, &[StatusCode::OK])?
                .text()
                .await
                .map_err(|_| Error::new("object_store_unavailable", 503))?;
            keys.extend(tagged(&listing, "Key"));
            if tagged(&listing, "IsTruncated").next().as_deref() != Some("true") {
                return Ok(keys);
            }
            after = tagged(&listing, "NextContinuationToken").next();
            ensure(after.is_some(), "object_store_unavailable", 503)?;
        }
    }
    /// A request by `client` for `key` in the bucket, or for the bucket itself with none, asking
    /// `query`, signed for a body whose SHA-256 is `payload`.
    fn request(
        &self,
        client: &reqwest::Client,
        method: Method,
        key: &str,
        query: &[(&str, &str)],
        payload: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let endpoint = url::Url::parse(&self.store.endpoint)
            .map_err(|_| Error::new("invalid_object_store", 500))?;
        let host = match endpoint.port() {
            Some(port) => format!("{}:{port}", endpoint.host_str().unwrap_or_default()),
            None => endpoint.host_str().unwrap_or_default().to_owned(),
        };
        let path = match key {
            "" => format!("/{}", encoded(&self.store.bucket, false)),
            key => format!(
                "/{}/{}",
                encoded(&self.store.bucket, false),
                encoded(key, true)
            ),
        };
        let query = canonical_query(query);
        let at = chrono::DateTime::from_timestamp_millis(now())
            .unwrap_or_default()
            .format("%Y%m%dT%H%M%SZ")
            .to_string();
        let authorization = Signing {
            method: method.as_str(),
            host: &host,
            path: &path,
            query: &query,
            payload,
            at: &at,
        }
        .authorization(
            &self.store.access_key_id,
            &self.store.secret_access_key,
            REGION,
        );
        let address = match query.as_str() {
            "" => format!("{}{path}", endpoint.origin().ascii_serialization()),
            query => format!("{}{path}?{query}", endpoint.origin().ascii_serialization()),
        };
        Ok(client
            .request(method, address)
            .header("x-amz-content-sha256", payload)
            .header("x-amz-date", &at)
            .header(header::AUTHORIZATION, authorization))
    }
}

/// What R2 answered, when it answered one of `expected`: anything else is
/// `object_store_unavailable`, to be tried again.
fn answered(
    response: reqwest::Result<reqwest::Response>,
    expected: &[StatusCode],
) -> Result<reqwest::Response> {
    response
        .ok()
        .filter(|response| expected.contains(&response.status()))
        .ok_or_else(|| Error::new("object_store_unavailable", 503))
}

/// The key of the bytes the DSP's `owner` keeps by `sha256`: a DSP's ID, an owner's name and
/// a SHA-256 in hexadecimal, or refused.
fn key(dsp: &str, owner: &str, sha256: &str) -> Result<String> {
    ensure(crypto::is_hex(sha256, 64), "invalid_object_key", 400)?;
    Ok(format!("{}{sha256}", owned(dsp, owner)?))
}
/// Where the DSP's `owner`'s keys begin: an owner's name is lowercase letters and underscores.
fn owned(dsp: &str, owner: &str) -> Result<String> {
    ensure(
        !owner.is_empty() && owner.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
        "invalid_object_key",
        400,
    )?;
    Ok(format!("{}{owner}/", of_dsp(dsp)?))
}
/// Where the DSP's keys begin.
fn of_dsp(dsp: &str) -> Result<String> {
    ensure(identifier(dsp, "dsp_"), "invalid_dsp_id", 400)?;
    Ok(format!("dsps/{dsp}/"))
}

/// A request as Signature Version 4 signs it: its signed headers are its host, its body's hash
/// and its time.
struct Signing<'a> {
    method: &'a str,
    host: &'a str,
    /// Its path, encoded.
    path: &'a str,
    /// Its query, encoded and in order.
    query: &'a str,
    /// The SHA-256 of its body in hexadecimal, or `UNSIGNED-PAYLOAD`.
    payload: &'a str,
    /// When it's made, as `20130524T000000Z`.
    at: &'a str,
}
impl Signing<'_> {
    /// Its `Authorization`, by the key `key_id` whose secret is `secret`, for `region`'s S3.
    fn authorization(&self, key_id: &str, secret: &str, region: &str) -> String {
        let canonical = format!(
            "{}\n{}\n{}\nhost:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n\n{SIGNED}\n{}",
            self.method, self.path, self.query, self.host, self.payload, self.at, self.payload
        );
        let day = &self.at[..8.min(self.at.len())];
        let scope = format!("{day}/{region}/s3/aws4_request");
        let signed = format!(
            "AWS4-HMAC-SHA256\n{}\n{scope}\n{}",
            self.at,
            crypto::hex(&Sha256::digest(canonical))
        );
        let key = [day, region, "s3", "aws4_request"]
            .iter()
            .fold(format!("AWS4{secret}").into_bytes(), |key, part| {
                crypto::hmac(&key, part)
            });
        format!(
            "AWS4-HMAC-SHA256 Credential={key_id}/{scope},SignedHeaders={SIGNED},Signature={}",
            crypto::hex(&crypto::hmac(&key, &signed))
        )
    }
}
/// `value` as a signed address carries it: every byte but letters, digits and `-._~` as
/// `%XX`, and `/` too unless it's a `path`.
fn encoded(value: &str, path: bool) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(b).to_string()
            }
            b'/' if path => "/".into(),
            b => format!("%{b:02X}"),
        })
        .collect()
}
/// `query`, encoded, in the order its names sort in.
fn canonical_query(query: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(String, String)> = query
        .iter()
        .map(|(name, value)| (encoded(name, false), encoded(value, false)))
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}
/// The text of each element `tag` in the XML `xml`, its entities read.
fn tagged<'a>(xml: &'a str, tag: &str) -> impl Iterator<Item = String> + 'a {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut rest = xml;
    std::iter::from_fn(move || {
        let start = rest.find(&open)? + open.len();
        let end = start + rest[start..].find(&close)?;
        let text = &rest[start..end];
        rest = &rest[end + close.len()..];
        Some(
            text.replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&apos;", "'")
                .replace("&amp;", "&"),
        )
    })
}

#[cfg(test)]
#[path = "../tests/backend/objects.rs"]
mod tests;
