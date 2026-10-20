//! What discovery found for a station, kept beside the DSP's browser profile so the next
//! performance and DVIC collections needn't find it again: the station's service area,
//! the DSP's company there, and where the performance API answered for it. It goes with
//! the profile when the credentials change (the collector's `browser_entries`), is found
//! again after a week, and a station whose reads stop matching it is forgotten.
use super::{Discovery, Scope};
use dispatch_core::{Error, Result, db, ensure, foundation::validate::token};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// The file's name, beside the profile.
pub const FILE: &str = "cortex-known.json";
/// How long what discovery found is trusted before it is found again.
const FRESH_MS: i64 = 7 * 24 * 3_600_000;
/// As much as the file may hold: a DSP has a handful of stations.
const LIMIT: u64 = 64 * 1024;

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Known {
    stations: BTreeMap<String, Station>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Station {
    service_area_id: String,
    provider: String,
    timezone: String,
    found_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api: Option<Api>,
}
/// Where the performance API answered for the station's company, and how it names the DSP.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Api {
    pub base: String,
    pub dsp: String,
}

pub struct KnownScopes {
    path: PathBuf,
}
impl KnownScopes {
    /// The DSP's file, beside its browser profile.
    pub fn beside(profile: &Path) -> Result<Self> {
        Ok(Self {
            path: profile
                .parent()
                .ok_or_else(|| Error::new("unsafe_storage_path", 500))?
                .join(FILE),
        })
    }
    /// What the file holds; a file that can't be read holds nothing.
    fn read(&self) -> Known {
        let read = || -> Result<Known> {
            if !self.path.try_exists()? {
                return Ok(Known::default());
            }
            db::private_file(&self.path, false)?;
            ensure(
                std::fs::metadata(&self.path)?.len() <= LIMIT,
                "known_scopes_invalid",
                409,
            )?;
            serde_json::from_slice(&std::fs::read(&self.path)?)
                .map_err(|_| Error::new("known_scopes_invalid", 409))
        };
        read().unwrap_or_default()
    }
    fn write(&self, known: &Known) -> Result<()> {
        let bytes = serde_json::to_vec(known)?;
        ensure(bytes.len() as u64 <= LIMIT, "known_scopes_invalid", 409)?;
        db::write_private(&self.path, &bytes)
    }
    /// The scope discovery found for the station `discovery` asks about, on its date, when
    /// found within the week in the DSP's own timezone.
    pub fn scope(&self, discovery: &Discovery, now: i64) -> Option<Scope> {
        let known = self.read();
        let station = known.stations.get(&discovery.station)?;
        (now - station.found_at < FRESH_MS
            && now >= station.found_at
            && station.timezone == discovery.timezone)
            .then(|| {
                discovery
                    .scope(&station.service_area_id, &station.provider)
                    .ok()
            })
            .flatten()
    }
    /// Where the performance API answered for `scope`'s company at its station.
    pub fn api(&self, scope: &Scope) -> Option<Api> {
        let known = self.read();
        let station = known.stations.get(&scope.station)?;
        (station.provider == scope.provider && station.service_area_id == scope.service_area_id)
            .then(|| station.api.clone())
            .flatten()
            .filter(|api| token(&api.dsp, 32) && api.base.len() <= 512)
    }
    /// Keeps what discovery found for `scope`'s station, with where the API answered when
    /// it is the same company's.
    pub fn keep(&self, scope: &Scope, now: i64) -> Result<()> {
        let mut known = self.read();
        let api = known
            .stations
            .get(&scope.station)
            .filter(|kept| kept.provider == scope.provider)
            .and_then(|kept| kept.api.clone());
        known.stations.insert(
            scope.station.clone(),
            Station {
                service_area_id: scope.service_area_id.clone(),
                provider: scope.provider.clone(),
                timezone: scope.timezone.clone(),
                found_at: now,
                api,
            },
        );
        self.write(&known)
    }
    /// Keeps where the performance API answered for `scope`'s company, when the station's
    /// scope is the one kept.
    pub fn keep_api(&self, scope: &Scope, api: Api) -> Result<()> {
        let mut known = self.read();
        let Some(station) = known.stations.get_mut(&scope.station) else {
            return Ok(());
        };
        if station.provider != scope.provider
            || station.service_area_id != scope.service_area_id
            || station.api.as_ref() == Some(&api)
        {
            return Ok(());
        }
        station.api = Some(api);
        self.write(&known)
    }
    /// Forgets the station: its next collection finds it again.
    pub fn forget(&self, station: &str) -> Result<()> {
        let mut known = self.read();
        if known.stations.remove(station).is_some() {
            self.write(&known)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/backend/known.rs"]
mod tests;
