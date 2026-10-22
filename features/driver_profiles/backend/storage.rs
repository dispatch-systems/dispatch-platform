//! Driver Profiles' storage: the roster in a database of its own beside Cortex's, and the
//! photos in the DSP's cache. Only this crate writes SQL against it.
use super::photos;
use dispatch_core::{
    Result,
    collection::registry::AddedStorage,
    db::{Db, DspLease, FromRow, Kind, Row, Store, at, now},
    ensure,
    foundation::validate,
    manifest::people::Photo,
};
use dispatch_cortex::{
    self as cortex,
    associates::{Capture, Collection, MAX_KEPT_PHOTO_BYTES, PhotoFormat, Request},
};
use dispatch_driver_profiles_contract::Associate;
use rusqlite::params;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Its database beside Cortex's: `driver_profiles/driver_profiles.sqlite` in each DSP's data.
pub(crate) const DATABASE: Kind = Kind::new("driver_profiles", 1);
pub(crate) static STORAGE: AddedStorage = AddedStorage {
    id: "driver_profiles",
    kind: DATABASE,
    marker: "storage.driver_profiles",
    source: "driver_profiles-v1",
    verify,
};
/// Fails closed when the initialized storage lost a table.
fn verify(db: &Db) -> Result<()> {
    for table in ["associates", "photos", "roster_runs"] {
        db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
    }
    Ok(())
}
/// How long a photo kept stands for its address before a collection downloads it again, in
/// case the associate changed it. An offboarded associate's stands for good: they change it no
/// more.
const PHOTO_REFRESH_MS: i64 = 30 * 24 * 3_600_000;

/// What a roster job carries while queued; the DSP's code is bound just before it runs.
pub(crate) fn roster_request() -> Value {
    json!({"collection": "associates"})
}
/// The request a roster job runs with: the DSP's code, as its profile has it now.
pub(crate) fn bind_roster_request(store: &Store, dsp: &str) -> Result<Value> {
    let abbreviation = store.profile(dsp)?.abbreviation;
    ensure(
        !abbreviation.is_empty(),
        "driver_profiles_abbreviation_required",
        409,
    )?;
    let request = Request {
        collection: Collection::Associates,
        dsp_abbreviation: abbreviation,
    };
    request.validate()?;
    Ok(serde_json::to_value(request)?)
}

/// What Driver Profiles reads and writes for one DSP.
pub trait DriverProfilesStore {
    fn driver_profiles_db(&self, dsp: &str) -> Result<DspLease<'_>>;
    /// The addresses of the photos kept for `company`'s associates that still stand, whose
    /// files are there: a collection downloads only the rest. An offboarded associate's always
    /// stands; a JPEG or PNG heavier than any shrunk photo, kept before photos were shrunk,
    /// never does, so the next collection shrinks it.
    fn kept_photos(&self, dsp: &str, company: &str) -> Result<Vec<String>>;
    /// Stores a roster as the current one. The photos' files are written already.
    fn publish_roster(&self, dsp: &str, job: &str, capture: &Capture) -> Result<()>;
    /// The photo of the first of `transporters` the roster lists with one: its kind and its
    /// bytes.
    fn photo(&self, dsp: &str, transporters: &[String]) -> Result<Option<Photo>>;
    /// What the roster says of each of `transporters` it has listed, in their order. Refuses
    /// an empty ID, one over 128 characters, or more than 32 IDs.
    fn associates(&self, dsp: &str, transporters: &[String]) -> Result<Vec<Associate>>;
}
impl DriverProfilesStore for Store {
    fn driver_profiles_db(&self, dsp: &str) -> Result<DspLease<'_>> {
        self.added_storage(dsp, cortex::PROVIDER, &STORAGE)
    }
    fn kept_photos(&self, dsp: &str, company: &str) -> Result<Vec<String>> {
        let rows = self
            .driver_profiles_db(dsp)?
            .query_as::<(String, String, String)>(
                "SELECT p.address,p.sha256,p.format FROM photos p WHERE EXISTS \
             (SELECT 1 FROM associates a WHERE a.photo_address=p.address AND a.company_id=?1 \
             AND (p.stored_at>=?2 OR a.status='OFFBOARDED')) \
             AND (p.bytes<=?3 OR p.format NOT IN (?4,?5)) ORDER BY p.address",
                params![
                    company,
                    at(now() - PHOTO_REFRESH_MS),
                    MAX_KEPT_PHOTO_BYTES as i64,
                    PhotoFormat::Jpeg.extension(),
                    PhotoFormat::Png.extension()
                ],
            )?;
        let dir = photos::dir(self, dsp)?;
        Ok(rows
            .into_iter()
            .filter(|(_, sha256, format)| {
                photos::file_name(sha256, format).is_some_and(|name| dir.join(name).is_file())
            })
            .map(|(address, _, _)| address)
            .collect())
    }
    fn publish_roster(&self, dsp: &str, job: &str, capture: &Capture) -> Result<()> {
        let request = Request::parse(&bind_roster_request(self, dsp)?)?
            .ok_or_else(|| dispatch_core::Error::new("invalid_input", 400))?;
        capture.validate(&request)?;
        let dir = photos::dir(self, dsp)?;
        let collected_at = at(capture.finished_at);
        let db = self.driver_profiles_db(dsp)?;
        let gone = db.transaction(|| {
            let before = files(&db)?;
            for a in &capture.associates {
                db.exec(
                    "INSERT INTO associates(transporter_id,company_id,name,email,personal_phone,\
                     work_phone,status,roles,qualifications,license_expires,photo_address,listed,\
                     first_listed,last_listed,row) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,1,?12,?12,?13) \
                     ON CONFLICT(transporter_id) DO UPDATE SET company_id=excluded.company_id,\
                     name=excluded.name,email=excluded.email,personal_phone=excluded.personal_phone,\
                     work_phone=excluded.work_phone,status=excluded.status,roles=excluded.roles,\
                     qualifications=excluded.qualifications,license_expires=excluded.license_expires,\
                     photo_address=excluded.photo_address,listed=1,last_listed=excluded.last_listed,\
                     row=excluded.row",
                    params![
                        a.transporter_id,
                        capture.company_id,
                        a.name,
                        a.email,
                        a.personal_phone,
                        a.work_phone,
                        a.status,
                        serde_json::to_string(&a.roles)?,
                        serde_json::to_string(&a.qualifications)?,
                        a.license_expires,
                        a.photo,
                        collected_at,
                        a.row.to_string(),
                    ],
                )?;
            }
            let listed: Vec<&str> = capture
                .associates
                .iter()
                .map(|a| a.transporter_id.as_str())
                .collect();
            db.exec(
                "UPDATE associates SET listed=0 WHERE listed=1 AND transporter_id NOT IN \
                 (SELECT value FROM json_each(?))",
                [serde_json::to_string(&listed)?],
            )?;
            for photo in &capture.photos {
                let name = photos::file_name(&photo.sha256, photo.format.extension())
                    .ok_or_else(|| dispatch_core::Error::new("associates_capture_invalid", 502))?;
                let bytes = std::fs::metadata(dir.join(&name))
                    .map_err(|_| dispatch_core::Error::new("driver_profiles_photo_missing", 500))?
                    .len();
                db.exec(
                    "INSERT INTO photos(address,sha256,format,bytes,stored_at) VALUES (?,?,?,?,?) \
                     ON CONFLICT(address) DO UPDATE SET sha256=excluded.sha256,\
                     format=excluded.format,bytes=excluded.bytes,stored_at=excluded.stored_at",
                    params![
                        photo.address,
                        photo.sha256,
                        photo.format.extension(),
                        bytes as i64,
                        collected_at
                    ],
                )?;
            }
            // A photo goes once the roster no longer lists whoever it shows.
            db.exec(
                "DELETE FROM photos WHERE address NOT IN (SELECT photo_address FROM associates \
                 WHERE listed=1 AND photo_address IS NOT NULL)",
                [],
            )?;
            db.exec(
                "INSERT INTO roster_runs(job_id,company_id,collected_at,listed,photos,photos_waiting) \
                 VALUES (?,?,?,?,?,?)",
                params![
                    job,
                    capture.company_id,
                    collected_at,
                    capture.associates.len() as i64,
                    capture.photos.len() as i64,
                    capture.photos_waiting as i64
                ],
            )?;
            let after = files(&db)?;
            Ok(before.difference(&after).cloned().collect::<Vec<_>>())
        })?;
        photos::remove(&dir, &gone)
    }
    fn photo(&self, dsp: &str, transporters: &[String]) -> Result<Option<Photo>> {
        let rows = self
            .driver_profiles_db(dsp)?
            .query_as::<(String, String, String)>(
                "SELECT a.transporter_id,p.sha256,p.format FROM associates a JOIN photos p \
             ON p.address=a.photo_address WHERE a.listed=1 \
             AND a.transporter_id IN (SELECT value FROM json_each(?))",
                [serde_json::to_string(transporters)?],
            )?;
        let Some((_, sha256, format)) = transporters
            .iter()
            .find_map(|id| rows.iter().find(|(t, _, _)| t == id))
        else {
            return Ok(None);
        };
        let (Some(name), Some(kind)) = (photos::file_name(sha256, format), photos::kind(format))
        else {
            return Ok(None);
        };
        match std::fs::read(photos::dir(self, dsp)?.join(name)) {
            Ok(bytes) => Ok(Some((kind.content_type(), bytes))),
            // Not collected again since a restore: the next collection brings it.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn associates(&self, dsp: &str, transporters: &[String]) -> Result<Vec<Associate>> {
        validate::ids(transporters, 32)?;
        let mut kept = self
            .driver_profiles_db(dsp)?
            .query_as::<Kept>(
                "SELECT transporter_id,listed,status,roles,qualifications,license_expires,\
                 last_listed FROM associates \
                 WHERE transporter_id IN (SELECT value FROM json_each(?))",
                [serde_json::to_string(transporters)?],
            )?
            .into_iter()
            .map(|Kept(associate)| (associate.id.clone(), associate))
            .collect::<BTreeMap<_, _>>();
        Ok(transporters
            .iter()
            .filter_map(|id| kept.remove(id))
            .collect())
    }
}
/// An associate as the roster's row has them.
struct Kept(Associate);
impl FromRow for Kept {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let list = |column: &str| -> Result<Vec<String>> {
            Ok(serde_json::from_str(&row.get::<String>(column)?)?)
        };
        Ok(Self(Associate {
            id: row.get("transporter_id")?,
            listed: row.get("listed")?,
            status: row.get("status")?,
            roles: list("roles")?,
            qualifications: list("qualifications")?,
            license_expires: row.get("license_expires")?,
            last_listed: row.get("last_listed")?,
        }))
    }
}
/// Every photo file the kept photos name.
fn files(db: &Db) -> Result<BTreeSet<String>> {
    Ok(db
        .query_as::<(String, String)>("SELECT sha256,format FROM photos", [])?
        .into_iter()
        .filter_map(|(sha256, format)| photos::file_name(&sha256, &format))
        .collect())
}

#[cfg(test)]
#[path = "../tests/backend/storage.rs"]
mod tests;
