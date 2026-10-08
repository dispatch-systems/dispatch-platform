//! A file a route takes as it arrives: the one kind of request whose body isn't JSON. Its
//! bytes stream on to wherever the route keeps them, never held whole in memory, and the
//! route says how large one may be.
use crate::{Error, Result, ensure};
use axum::{
    body::{Body, Bytes},
    http::{header, request::Parts},
};
use futures_util::{Stream, StreamExt, stream};
use std::time::Duration;
use tokio::sync::{Semaphore, SemaphorePermit};

/// The most any route takes in one upload: 100 MB.
pub const UPLOAD_LIMIT: u64 = 100 * 1024 * 1024;
/// How long an upload may stall before it is given up.
const STALL: Duration = Duration::from_secs(60);
/// Uploads at once, across the server: each holds a connection open to where it goes.
static UPLOADS: Semaphore = Semaphore::const_new(4);

/// A file's bytes as they arrive, as many as the client said it sends.
pub struct Upload {
    pub length: u64,
    body: Body,
    turn: SemaphorePermit<'static>,
}
impl Upload {
    /// The upload a request carries: `application/octet-stream` of a stated length, at most
    /// `limit` bytes.
    pub(super) fn begin(parts: &Parts, body: Body, limit: u64) -> Result<Self> {
        let header = |name| parts.headers.get(name).and_then(|v| v.to_str().ok());
        let kind = header(header::CONTENT_TYPE).and_then(|v| v.split(';').next());
        ensure(
            kind == Some("application/octet-stream"),
            "upload_required",
            415,
        )?;
        let length = header(header::CONTENT_LENGTH)
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or_else(|| Error::new("length_required", 411))?;
        ensure(length <= limit, "upload_too_large", 413)?;
        let turn = UPLOADS
            .try_acquire()
            .map_err(|_| Error::new("uploads_busy", 503))?;
        Ok(Self { length, body, turn })
    }
    /// The bytes as they arrive. The stream fails if they stall for a minute or don't come
    /// to the length the client stated, so whatever it feeds never keeps a partial file.
    pub fn stream(self) -> impl Stream<Item = std::io::Result<Bytes>> + Send + 'static {
        let Self { length, body, turn } = self;
        let failed = |code: &str| Some(std::io::Error::other(code.to_owned()));
        stream::unfold(
            (body.into_data_stream(), 0, Some(turn)),
            move |(mut data, seen, turn)| async move {
                // The turn goes with the stream's end.
                turn.as_ref()?;
                let (answer, seen) = match tokio::time::timeout(STALL, data.next()).await {
                    Err(_) => (failed("upload_stalled").map(Err), seen),
                    Ok(None) if seen == length => (None, seen),
                    Ok(None) | Ok(Some(Err(_))) => (failed("upload_incomplete").map(Err), seen),
                    Ok(Some(Ok(chunk))) => {
                        let seen = seen + chunk.len() as u64;
                        if seen > length {
                            (failed("upload_too_large").map(Err), seen)
                        } else {
                            (Some(Ok(chunk)), seen)
                        }
                    }
                };
                let next = match &answer {
                    Some(Ok(_)) => turn,
                    _ => None,
                };
                answer.map(|answer| (answer, (data, seen, next)))
            },
        )
    }
}
