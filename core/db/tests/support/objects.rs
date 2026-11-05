//! A stand-in for R2, in the test's own process: it keeps what's put in a bucket of its own, and
//! answers getting, deleting and listing it, a page of two keys at a time, as S3 does, each page
//! after the first asked for by a token an address must escape; down, it answers each request
//! with 503. It takes a request only signed as S3 checks it, by the key it gave out: its
//! signature made again of what arrived, with the key's secret. It counts what it's asked for,
//! and holds a request for a key the test holds until it lets go.
use crate::foundation::{config::ObjectStore, crypto};
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Request, State},
    http::{Method, StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{Notify, Semaphore};

const BUCKET: &str = "pictures";
const KEY_ID: &str = "stand-in-key";
const SECRET: &str = "stand-in-secret";
/// How many keys a page of a listing holds.
const PAGE: usize = 2;
/// What begins each token of a listing's next page, before the last key's base64: so every
/// token has `+`, `/` and `=`, for an address to escape.
const TOKEN: &str = "+/=";

#[derive(Clone, Default)]
struct Bucket {
    objects: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    down: Arc<AtomicBool>,
    /// Each request taken, by its method and key.
    asked: Arc<Mutex<Vec<(Method, String)>>>,
    held: Arc<Mutex<HashMap<String, Arc<Held>>>>,
}
/// Requests for a key, held until the test lets them go.
pub struct Held {
    arrived: Notify,
    /// Closed once they may go.
    going: Semaphore,
}
impl Held {
    /// Waits for a request for the key to arrive.
    pub async fn arrived(&self) {
        self.arrived.notified().await;
    }
    /// Lets the request for the key go on, and those after it.
    pub fn go(&self) {
        self.going.close();
    }
}
/// The stand-in, answering at its address while it's kept.
pub struct Fake {
    address: String,
    bucket: Bucket,
    serving: tokio::task::JoinHandle<()>,
}
impl Drop for Fake {
    fn drop(&mut self) {
        self.serving.abort();
    }
}
impl Fake {
    pub async fn start() -> Self {
        let bucket = Bucket::default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new().fallback(answer).with_state(bucket.clone());
        let serving = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            address,
            bucket,
            serving,
        }
    }
    /// The object store a server's config names to use it, the bytes it keeps staying on disk
    /// too, as a server's do until `R2_ONLY` is set.
    pub fn store(&self) -> ObjectStore {
        ObjectStore {
            endpoint: self.address.clone(),
            bucket: BUCKET.into(),
            access_key_id: KEY_ID.into(),
            secret_access_key: SECRET.into(),
            only: false,
        }
    }
    /// How many requests `method` for `key` it took.
    pub fn asked(&self, method: Method, key: &str) -> usize {
        self.bucket
            .asked
            .lock()
            .unwrap()
            .iter()
            .filter(|asked| asked.0 == method && asked.1 == key)
            .count()
    }
    /// Holds each request for `key`, once its body arrived, until `Held::go`.
    pub fn hold(&self, key: &str) -> Arc<Held> {
        let held = Arc::new(Held {
            arrived: Notify::new(),
            going: Semaphore::new(0),
        });
        self.bucket
            .held
            .lock()
            .unwrap()
            .insert(key.into(), held.clone());
        held
    }
    /// Every key it keeps, in order.
    pub fn keys(&self) -> Vec<String> {
        self.bucket
            .objects
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }
    /// What it keeps at `key`.
    pub fn object(&self, key: &str) -> Option<Vec<u8>> {
        self.bucket.objects.lock().unwrap().get(key).cloned()
    }
    /// Keeps `bytes` at `key`, as another server's upload would have.
    pub fn put(&self, key: &str, bytes: &[u8]) {
        self.bucket
            .objects
            .lock()
            .unwrap()
            .insert(key.into(), bytes.to_vec());
    }
    /// Answers every request with 503 while `down`.
    pub fn down(&self, down: bool) {
        self.bucket.down.store(down, Ordering::SeqCst);
    }
}

async fn answer(State(bucket): State<Bucket>, request: Request) -> Response {
    let (asked, body) = request.into_parts();
    let Ok(body) = axum::body::to_bytes(body, usize::MAX).await else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if bucket.down.load(Ordering::SeqCst) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if !signed(&asked, &body) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let method = asked.method;
    let path = asked.uri.path();
    let query = asked.uri.query().unwrap_or_default();
    let Some(rest) = path.strip_prefix(&format!("/{BUCKET}")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if rest.is_empty() && method == Method::GET {
        return match listed(&bucket, query) {
            Some(page) => page.into_response(),
            None => StatusCode::BAD_REQUEST.into_response(),
        };
    }
    let Some(key) = rest.strip_prefix('/').map(String::from) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    bucket
        .asked
        .lock()
        .unwrap()
        .push((method.clone(), key.clone()));
    let held = bucket.held.lock().unwrap().get(&key).cloned();
    if let Some(held) = held {
        held.arrived.notify_one();
        let _ = held.going.acquire().await;
    }
    match method {
        Method::PUT => {
            bucket.objects.lock().unwrap().insert(key, body.to_vec());
            StatusCode::OK.into_response()
        }
        Method::GET => match bucket.objects.lock().unwrap().get(&key) {
            Some(bytes) => Body::from(Bytes::from(bytes.clone())).into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        },
        Method::DELETE => {
            bucket.objects.lock().unwrap().remove(&key);
            StatusCode::NO_CONTENT.into_response()
        }
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

/// A page of the keys beginning with the query's `prefix`, after its `continuation-token`: none
/// for a token it didn't give out.
fn listed(bucket: &Bucket, query: &str) -> Option<String> {
    let asked: BTreeMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    let prefix = asked.get("prefix").cloned().unwrap_or_default();
    let after = match asked.get("continuation-token") {
        Some(token) => {
            let last = STANDARD.decode(token.strip_prefix(TOKEN)?).ok()?;
            Some(String::from_utf8(last).ok()?)
        }
        None => None,
    };
    let keys: Vec<String> = bucket
        .objects
        .lock()
        .unwrap()
        .keys()
        .filter(|key| key.starts_with(&prefix))
        .filter(|key| after.as_ref().is_none_or(|after| *key > after))
        .cloned()
        .collect();
    let page = &keys[..keys.len().min(PAGE)];
    let contents: String = page
        .iter()
        .map(|key| format!("<Contents><Key>{key}</Key></Contents>"))
        .collect();
    let more = match (keys.len() > PAGE, page.last()) {
        (true, Some(last)) => format!(
            "<IsTruncated>true</IsTruncated>\
             <NextContinuationToken>{TOKEN}{}</NextContinuationToken>",
            STANDARD.encode(last)
        ),
        _ => "<IsTruncated>false</IsTruncated>".into(),
    };
    Some(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult>{contents}{more}</ListBucketResult>"
    ))
}

/// Whether the request `asked`, with `body`, is signed as S3 checks it by the key the stand-in
/// gave out: its host, time and body's hash signed, and its signature the one the key's secret
/// makes of its method, path and query, those headers, and that hash, as they arrived.
fn signed(asked: &Parts, body: &[u8]) -> bool {
    let header = |name: &str| {
        asked
            .headers
            .get(name)
            .and_then(|value| value.to_str().ok())
    };
    let Some(fields) =
        header("authorization").and_then(|value| value.strip_prefix("AWS4-HMAC-SHA256 "))
    else {
        return false;
    };
    let fields: BTreeMap<&str, &str> = fields
        .split(',')
        .filter_map(|field| field.trim().split_once('='))
        .collect();
    let (Some(credential), Some(names), Some(signature), Some(at), Some(payload)) = (
        fields.get("Credential"),
        fields.get("SignedHeaders"),
        fields.get("Signature"),
        header("x-amz-date"),
        header("x-amz-content-sha256"),
    ) else {
        return false;
    };
    let scope = format!("{}/auto/s3/aws4_request", at.get(..8).unwrap_or_default());
    let names: Vec<&str> = names.split(';').collect();
    if *credential != format!("{KEY_ID}/{scope}")
        || !["host", "x-amz-content-sha256", "x-amz-date"]
            .iter()
            .all(|name| names.contains(name))
        || (payload != "UNSIGNED-PAYLOAD" && payload != crypto::hex(&Sha256::digest(body)))
    {
        return false;
    }
    let mut headers = String::new();
    for name in &names {
        let Some(value) = header(name) else {
            return false;
        };
        headers += &format!("{name}:{}\n", value.trim());
    }
    let mut query: Vec<String> =
        url::form_urlencoded::parse(asked.uri.query().unwrap_or_default().as_bytes())
            .map(|(name, value)| format!("{}={}", escaped(&name), escaped(&value)))
            .collect();
    query.sort();
    let canonical = format!(
        "{}\n{}\n{}\n{headers}\n{}\n{payload}",
        asked.method,
        asked.uri.path(),
        query.join("&"),
        names.join(";")
    );
    let signing = format!(
        "AWS4-HMAC-SHA256\n{at}\n{scope}\n{}",
        crypto::hex(&Sha256::digest(canonical))
    );
    let key = scope
        .split('/')
        .fold(format!("AWS4{SECRET}").into_bytes(), |key, part| {
            mac(&key, part.as_bytes())
        });
    crypto::hex(&mac(&key, signing.as_bytes())) == *signature
}
fn mac(key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key).unwrap();
    mac.update(value);
    mac.finalize().into_bytes().to_vec()
}
/// `value` as S3 signs it in a query: every byte but letters, digits and `-._~` as `%XX`.
fn escaped(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
