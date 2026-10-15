//! What an associates collection brings: the roster of the DSP's company as Amazon lists
//! it, each row kept as Amazon sent it beside the fields read from it, and the photos the
//! DSP lacked. Every field is untrusted input, bounded here.
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::NaiveDate;
use dispatch_core::{Error, Result, ensure, foundation::crypto::sha, foundation::validate::token};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub const JOB_KIND: &str = "cortex.associates.collect";
/// Far more than one company employs: a longer roster is not one DSP's.
pub const MAX_ASSOCIATES: usize = 5_000;
/// The largest photo kept. Amazon's run to 3.5 MB.
pub const MAX_PHOTO_BYTES: usize = 12 * 1024 * 1024;
/// The largest row kept as Amazon sent it.
const MAX_ROW_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Collection {
    #[serde(rename = "associates")]
    Associates,
}

/// The roster of one DSP's company, as a job asks for it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub collection: Collection,
    /// The DSP's short code, which picks its company out of those its sign-in sees.
    pub dsp_abbreviation: String,
}
impl Request {
    /// Whether a job request or a collection names the associates collection.
    pub fn is(value: &Value) -> bool {
        matches!(
            value.get("collection").and_then(Value::as_str),
            Some("associates")
        )
    }
    pub fn parse(value: &Value) -> Result<Option<Self>> {
        if !Self::is(value) {
            return Ok(None);
        }
        let request: Self =
            serde_json::from_value(value.clone()).map_err(|_| Error::new("invalid_input", 400))?;
        request.validate()?;
        Ok(Some(request))
    }
    pub fn validate(&self) -> Result<()> {
        ensure(token(&self.dsp_abbreviation, 32), "invalid_input", 400)
    }
}

/// One associate as the roster lists them.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Associate {
    pub transporter_id: String,
    pub name: String,
    pub email: Option<String>,
    pub personal_phone: Option<String>,
    pub work_phone: Option<String>,
    /// `ACTIVE` or `INACTIVE`, as Amazon writes it.
    pub status: String,
    /// `Driver`, `Helper`: what they may do.
    pub roles: Vec<String>,
    /// The vans and kinds of work they are qualified for.
    pub qualifications: Vec<String>,
    pub license_expires: Option<String>,
    /// Where the application serves their photo, a path on its own host.
    pub photo: Option<String>,
    /// The row as Amazon sent it.
    pub row: Value,
}
impl Associate {
    /// One roster row, read. `None` for a row naming no transporter ID, which nothing else
    /// Amazon sends could name either.
    pub fn read(row: Value) -> Result<Option<Self>> {
        let invalid = || Error::new("associates_row_invalid", 502);
        ensure(row.is_object(), "associates_row_invalid", 502)?;
        ensure(
            row.to_string().len() <= MAX_ROW_BYTES,
            "associates_source_too_large",
            502,
        )?;
        let text = |key: &str, max: usize| -> Option<String> {
            let value = row.get(key)?.as_str()?.trim();
            (!value.is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control))
                .then(|| value.to_owned())
        };
        let Some(transporter_id) = text("transporter_id", 64).filter(|id| token(id, 64)) else {
            return Ok(None);
        };
        let name = text("full_name", 256)
            .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
            .ok_or_else(invalid)?;
        let status = text("operational_status", 32)
            .filter(|s| s.bytes().all(|b| b.is_ascii_uppercase() || b == b'_'))
            .ok_or_else(invalid)?;
        let list = |key: &str| -> Vec<String> {
            text(key, 2048)
                .map(|value| {
                    let mut seen = BTreeSet::new();
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|part| !part.is_empty() && part.len() <= 96)
                        .filter(|part| seen.insert(part.to_string()))
                        .take(32)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        Ok(Some(Self {
            name,
            email: text("email_address", 254)
                .map(|email| email.to_ascii_lowercase())
                .filter(|email| plausible_email(email)),
            personal_phone: text("personal_phone_number", 32),
            work_phone: text("work_phone_number", 32),
            status,
            roles: list("roles"),
            qualifications: list("qualifications"),
            license_expires: text("driver_license_expiration_date", 10)
                .filter(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").is_ok()),
            photo: text("photo_url", 2048).filter(|path| photo_address(path)),
            transporter_id,
            row,
        }))
    }
}
/// An address with one `@`, something before it and a dotted domain after it.
fn plausible_email(email: &str) -> bool {
    let mut parts = email.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.chars().any(char::is_whitespace)
}
/// Whether `path` is where the application serves a photo: a plain path on its own host.
pub(crate) fn photo_address(path: &str) -> bool {
    path.starts_with("/workforce/")
        && path.is_ascii()
        && !path.contains("..")
        && !path.contains("//")
        && !path.contains('#')
        && !path.bytes().any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
}

/// What a photo's bytes are. S3 serves Amazon's photos with no type of their own, so it is
/// read from their first bytes; anything else is not kept.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PhotoFormat {
    Jpeg,
    Png,
    Webp,
    Gif,
    Avif,
}
impl PhotoFormat {
    pub fn of(bytes: &[u8]) -> Option<Self> {
        match bytes {
            [0xFF, 0xD8, 0xFF, ..] => Some(Self::Jpeg),
            [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, ..] => Some(Self::Png),
            [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some(Self::Webp),
            [b'G', b'I', b'F', b'8', ..] => Some(Self::Gif),
            [_, _, _, _, b'f', b't', b'y', b'p', b'a', b'v', b'i', b'f', ..] => Some(Self::Avif),
            _ => None,
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Gif => "gif",
            Self::Avif => "avif",
        }
    }
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Gif => "image/gif",
            Self::Avif => "image/avif",
        }
    }
}

/// A photo the collection downloaded.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Photo {
    /// The address the roster gave it, which names it from one collection to the next.
    pub address: String,
    pub sha256: String,
    pub format: PhotoFormat,
    /// Its bytes, in base64.
    pub data: String,
}
impl Photo {
    /// `bytes` as the photo at `address`, when they are an image.
    pub fn new(address: &str, bytes: &[u8]) -> Option<Self> {
        Some(Self {
            address: address.to_owned(),
            sha256: sha(bytes),
            format: PhotoFormat::of(bytes)?,
            data: STANDARD.encode(bytes),
        })
    }
    /// Its bytes, as its digest and format say they are.
    pub fn bytes(&self) -> Result<Vec<u8>> {
        let bytes = STANDARD
            .decode(&self.data)
            .map_err(|_| Error::new("associates_capture_invalid", 502))?;
        ensure(
            bytes.len() <= MAX_PHOTO_BYTES
                && PhotoFormat::of(&bytes) == Some(self.format)
                && sha(&bytes) == self.sha256,
            "associates_capture_invalid",
            502,
        )?;
        Ok(bytes)
    }
}

/// The DSP's roster as collected, before publication.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture {
    pub version: u32,
    pub collection: Collection,
    pub company_id: String,
    pub company_code: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub associates: Vec<Associate>,
    /// Photos the DSP lacked, downloaded this time.
    pub photos: Vec<Photo>,
    /// Photos it lacks still, left for the next collection: one brings only so many.
    pub photos_waiting: usize,
}
impl Capture {
    pub fn validate(&self, request: &Request) -> Result<()> {
        request.validate()?;
        ensure(self.version == 1, "associates_capture_invalid", 502)?;
        ensure(
            token(&self.company_id, 128)
                && token(&self.company_code, 32)
                && self
                    .company_code
                    .eq_ignore_ascii_case(&request.dsp_abbreviation),
            "associates_scope_mismatch",
            502,
        )?;
        ensure(
            self.started_at > 0 && self.finished_at >= self.started_at,
            "associates_capture_invalid",
            502,
        )?;
        ensure(
            self.associates.len() <= MAX_ASSOCIATES,
            "associates_source_too_large",
            502,
        )?;
        let mut ids = BTreeSet::new();
        let mut addresses = BTreeSet::new();
        for associate in &self.associates {
            ensure(
                ids.insert(associate.transporter_id.as_str()),
                "associates_capture_invalid",
                502,
            )?;
            if let Some(photo) = &associate.photo {
                addresses.insert(photo.as_str());
            }
        }
        let mut photos = BTreeSet::new();
        for photo in &self.photos {
            ensure(
                addresses.contains(photo.address.as_str())
                    && photos.insert(photo.address.as_str())
                    && photo.sha256.len() == 64
                    && photo.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                "associates_capture_invalid",
                502,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/associates/capture.rs"]
mod tests;
