//! The station and day a Cortex collection covers: resolved, or what discovery needs
//! to resolve it.
use crate::{Error, Result, db::s, ensure};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scope {
    pub date: String,
    pub station: String,
    pub service_area_id: String,
    pub provider: String,
    pub timezone: String,
}
// The server pins the DSP's profile when queuing first-use discovery. Keep this
// request unchanged after resolving it, so retries remain idempotent.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum CollectionRequest {
    Scoped(Scope),
    Discover(Discovery),
}
impl CollectionRequest {
    pub fn validate_scope(&self, scope: &Scope) -> Result<()> {
        scope.validate()?;
        let matches = match self {
            Self::Scoped(expected) => scope == expected,
            Self::Discover(discovery) => {
                scope == &discovery.scope(&scope.service_area_id, &scope.provider)?
                    && !["ALL_DSPS", "ALL_DRIVERS"].contains(&scope.provider.as_str())
            }
        };
        ensure(matches, "cortex_scope_mismatch", 502)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Discovery {
    pub date: String,
    pub station: String,
    pub timezone: String,
    pub dsp_name: String,
    pub dsp_abbreviation: String,
}
impl Discovery {
    pub fn scope(&self, service_area_id: &str, provider: &str) -> Result<Scope> {
        let scope = Scope {
            date: self.date.clone(),
            station: self.station.clone(),
            timezone: self.timezone.clone(),
            service_area_id: service_area_id.into(),
            provider: provider.into(),
        };
        scope.validate()?;
        Ok(scope)
    }
}
impl Scope {
    pub fn validate(&self) -> Result<()> {
        let tz: chrono_tz::Tz = self
            .timezone
            .parse()
            .map_err(|_| Error::new("invalid_timezone", 400))?;
        let date = NaiveDate::parse_from_str(&self.date, "%Y-%m-%d")
            .map_err(|_| Error::new("invalid_date", 400))?;
        ensure(
            date.to_string() == self.date
                && self.date.as_str() >= "2000-01-01"
                && date <= Utc::now().with_timezone(&tz).date_naive(),
            "invalid_date",
            400,
        )?;
        ensure(
            (3..=8).contains(&self.station.len())
                && self
                    .station
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                && [&self.service_area_id, &self.provider].iter().all(|v| {
                    !v.is_empty()
                        && v.len() <= 128
                        && v.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                })
                && self.provider != "ALL_DSPS",
            "invalid_cortex_scope",
            400,
        )
    }
    pub fn request(value: &Value, timezone: &str) -> Result<Self> {
        crate::validate::fields(
            value,
            &[
                "requestId",
                "date",
                "station",
                "serviceAreaId",
                "provider",
                "timezone",
            ],
        )?;
        let scope = Self {
            date: s(value, "date").into(),
            station: s(value, "station").into(),
            service_area_id: s(value, "serviceAreaId").into(),
            provider: s(value, "provider").into(),
            timezone: if value.get("timezone").is_some() {
                crate::validate::timezone(value, "timezone")?
            } else {
                timezone.into()
            },
        };
        scope.validate()?;
        Ok(scope)
    }
    pub fn list_path(&self) -> String {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query
            .append_pair("navMenuVariant", "external")
            .append_pair("provider", &self.provider)
            .append_pair("selectedDay", &self.date)
            .append_pair("serviceAreaId", &self.service_area_id);
        format!("/operations/execution/itineraries?{}", query.finish())
    }
    pub fn detail_path(&self, id: &str) -> String {
        self.list_path().replacen(
            "/itineraries?",
            &format!(
                "/itineraries/{}/documentType/Itinerary?",
                url::form_urlencoded::byte_serialize(id.as_bytes()).collect::<String>()
            ),
            1,
        )
    }
}
