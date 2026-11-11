//! The built dashboard, loaded once and served from memory.
use crate::{
    Error, Result, State, ensure,
    foundation::{config::Environment, crypto},
};
use axum::{
    body::Body,
    http::{HeaderMap, Method, StatusCode},
    response::Response,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

pub fn browser_update_ready(config: &crate::foundation::config::Config) -> bool {
    let channel = if config.environment.is_production() {
        "production"
    } else {
        "dev"
    };
    let platform = config.platform();
    let status = std::fs::read(platform.join(format!("{channel}-update.json")))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    !platform.join(format!("{channel}-activation.json")).exists()
        && (config.development
            || status.is_some_and(|s| s["status"] == "ready" && s["digest"] == config.release))
}

struct Representation {
    bytes: axum::body::Bytes,
    etag: String,
    encoding: Option<&'static str>,
}
impl Representation {
    fn new(bytes: Vec<u8>, encoding: Option<&'static str>) -> Self {
        Self {
            etag: format!("\"{}\"", crypto::sha(&bytes)),
            bytes: bytes.into(),
            encoding,
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrivateAsset {
    owner: Option<String>,
    #[serde(default)]
    platform: bool,
    feature: Option<String>,
    #[serde(default)]
    styles: Vec<String>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Audience {
    pub code: Option<String>,
    pub platform: bool,
}
impl Audience {
    pub fn sees(&self, owner: Option<&str>) -> bool { owner.is_none() || self.platform || owner == self.code.as_deref() }
}

pub(crate) async fn audience(state: &std::sync::Arc<State>, request: &axum::extract::Request) -> Audience {
    if !crate::manifest::registry().features.iter().any(|feature| feature.owner.is_some())
        && !state.assets.get(request.uri().path()).is_some_and(|asset| asset.private.is_some()) {
        return Audience::default();
    }
    let mut probe = axum::http::Request::new(());
    *probe.method_mut() = request.method().clone();
    *probe.uri_mut() = request.uri().clone();
    *probe.headers_mut() = request.headers().clone();
    if let Some(site) = request.extensions().get::<crate::foundation::config::Site>() { probe.extensions_mut().insert(site.clone()); }
    let (parts, _) = probe.into_parts();
    let Ok(input) = super::middleware::head(state, &parts, "/assets/*") else { return Audience::default(); };
    state.read(move |db| {
        use super::route::Grant;
        let auth = super::route::Session.authorize(db, &input)?;
        let code = if let crate::foundation::config::Site::Dsp(code) = &input.site {
            if let Some(dsp) = db.dsp_at(code)? && db.may(&auth.user.id, &input.site, &dsp.id, crate::tenancy::roles::ACCESS)? { Some(code.clone()) } else { None }
        } else { None };
        Ok(Audience { code, platform: auth.user.platform_owner })
    }).await.unwrap_or_default()
}

pub fn custom_frontend(state: &State, name: &str) -> Option<(String, Vec<String>)> {
    state.assets.iter().find_map(|(url, asset)| asset.private.as_ref().filter(|private| private.feature.as_deref() == Some(name)).map(|private| (url.clone(), private.styles.clone())))
        .or_else(|| state.config.development.then(|| (format!("/custom/{name}.js"), Vec::new())))
}

pub struct Asset {
    private: Option<PrivateAsset>,
    identity: Representation,
    compressed: Vec<Representation>,
    mime: &'static str,
    cache: &'static str,
}
// Artifacts are immutable for the process lifetime. Deployment restarts the core.
// Load once so both GET and HEAD need no filesystem work on the request path.
pub fn assets(
    root: &std::path::Path,
    release: &str,
    environment: Environment,
) -> Result<HashMap<String, Asset>> {
    // The service worker serves the pages of the address it's at and below: it's at the root.
    let mut files = vec![
        ("/".to_owned(), root.join("index.html")),
        (
            "/service-worker.js".to_owned(),
            root.join("service-worker.js"),
        ),
    ];
    let directory = root.join("assets");
    if directory.is_dir() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                files.push((
                    format!("/assets/{}", entry.file_name().to_string_lossy()),
                    entry.path(),
                ));
            }
        }
    }
    let manifest = root.join("private-assets.json");
    let mut private: HashMap<String, PrivateAsset> = if manifest.is_file() { serde_json::from_slice(&std::fs::read(manifest)?)? } else { HashMap::new() };
    let mut result = HashMap::new();
    let mut size = 0;
    for (route, file) in files {
        if !file.is_file() {
            continue;
        }
        let extension = file.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ["br", "gz"].contains(&extension) {
            continue;
        }
        let mut bytes = std::fs::read(&file)?;
        if route == "/" {
            let mut html = String::from_utf8_lossy(&bytes).into_owned();
            // The page names Dev's yellow tab icon; Production's is blue, so the two are never
            // mistaken for each other.
            if environment.is_production() {
                html = html.replace("/assets/favicon-dev.", "/assets/favicon.");
            }
            bytes = html
                .replacen(
                    "</head>",
                    &format!(
                        "<meta name=\"dispatch-build\" content=\"{}\"></head>",
                        crypto::sha(release.as_bytes())
                    ),
                    1,
                )
                .into_bytes();
        }
        size += bytes.len();
        ensure(size <= 64 * 1024 * 1024, "dashboard_too_large", 503)?;
        let mut compressed = Vec::new();
        if route != "/" {
            for (suffix, encoding) in [("br", "br"), ("gz", "gzip")] {
                let sibling = file.with_extension(format!("{extension}.{suffix}"));
                if sibling.is_file() {
                    let encoded = std::fs::read(sibling)?;
                    size += encoded.len();
                    ensure(size <= 64 * 1024 * 1024, "dashboard_too_large", 503)?;
                    compressed.push(Representation::new(encoded, Some(encoding)));
                }
            }
        }
        let stem = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .as_bytes();
        let hashed = ["js", "css", "svg", "glb", "png"].contains(&extension)
            && stem.len() > 9
            && stem[stem.len() - 9] == b'-'
            && stem[stem.len() - 8..]
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-');
        let mime = match extension {
            "html" => "text/html; charset=utf-8",
            "js" => "text/javascript; charset=utf-8",
            "css" => "text/css; charset=utf-8",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "glb" => "model/gltf-binary",
            "woff2" => "font/woff2",
            "webmanifest" => "application/manifest+json",
            _ => "application/octet-stream",
        };
        result.insert(
            route.clone(),
            Asset {
                private: private.remove(&route),
                identity: Representation::new(bytes, None),
                compressed,
                mime,
                cache: if route == "/" {
                    "no-store"
                } else if hashed {
                    "public, max-age=31536000, immutable"
                } else {
                    "public, no-cache"
                },
            },
        );
    }
    ensure(private.is_empty(), "custom_asset_missing", 503)?;
    for (path, asset) in &result {
        ensure(!path.starts_with("/assets/custom-") || asset.private.is_some(), "custom_asset_unowned", 503)?;
    }
    Ok(result)
}
/// `GET` and `HEAD` of `/`, `/service-worker.js` and `/assets/*`, with an ETag so browsers revalidate cheaply.
pub(super) fn serve(state: &State, method: &Method, path: &str, headers: &HeaderMap, audience: &Audience) -> Result<Response> {
    ensure(
        !path.contains("..") && !path.contains('%') && !path.contains('\\'),
        "not_found",
        404,
    )?;
    let asset = state
        .assets
        .get(path)
        .ok_or_else(|| Error::new("not_found", 404))?;
    if let Some(private) = &asset.private {
        ensure((private.platform && audience.platform) || (!private.platform && private.owner.is_some() && audience.sees(private.owner.as_deref())), "not_found", 404)?;
    }
    let accepted = headers
        .get("accept-encoding")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let mut representation = &asset.identity;
    let mut quality = 0.0;
    for candidate in &asset.compressed {
        let next = encoding_quality(accepted, candidate.encoding.unwrap());
        if next > quality {
            representation = candidate;
            quality = next;
        }
    }
    let unchanged = path != "/"
        && headers
            .get("if-none-match")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|tags| {
                tags.split(',').any(|tag| {
                    tag.trim().trim_start_matches("W/") == representation.etag || tag.trim() == "*"
                })
            });
    let mut reply = Response::new(if method == Method::HEAD || unchanged {
        Body::empty()
    } else {
        Body::from(representation.bytes.clone())
    });
    if unchanged {
        *reply.status_mut() = StatusCode::NOT_MODIFIED;
    }
    let h = reply.headers_mut();
    h.insert("content-type", asset.mime.parse().unwrap());
    h.insert("etag", representation.etag.parse().unwrap());
    if !asset.compressed.is_empty() {
        h.insert("vary", "Accept-Encoding".parse().unwrap());
    }
    if let Some(encoding) = representation.encoding {
        h.insert("content-encoding", encoding.parse().unwrap());
    }
    h.insert("cache-control", if asset.private.is_some() { "private, no-store" } else { asset.cache }.parse().unwrap());
    if !unchanged {
        h.insert(
            "content-length",
            representation.bytes.len().to_string().parse().unwrap(),
        );
    }
    Ok(reply)
}

// An explicit q=0 overrides a wildcard, so clients can opt out of any representation.
fn encoding_quality(header: &str, encoding: &str) -> f32 {
    let mut wildcard = 0.0;
    for item in header.split(',') {
        let mut parts = item.trim().split(';');
        let name = parts.next().unwrap_or("").trim();
        let quality = parts
            .find_map(|part| part.trim().strip_prefix("q="))
            .map_or(1.0, |q| q.parse::<f32>().unwrap_or(0.0));
        let quality = if (0.0..=1.0).contains(&quality) {
            quality
        } else {
            0.0
        };
        if name.eq_ignore_ascii_case(encoding) {
            return quality;
        }
        if name == "*" {
            wildcard = quality;
        }
    }
    wildcard
}

#[cfg(test)]
#[path = "../../tests/backend/http/assets.rs"]
mod tests;
