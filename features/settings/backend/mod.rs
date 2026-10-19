//! Settings' own section: the DSP's profile, on the DSP's page of Settings, kept where core
//! keeps it, as onboarding first sets it.
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Kept, Section, section, text, timezone},
};
use serde_json::{Value, json};

/// The DSP's name, its short code, which names its address and stays once given, its
/// station code and its timezone, which its schedules run in.
pub const PROFILE: Section = section(
    "settings.profile",
    "DSP profile",
    "settings.manage",
    &[
        text("name", "Name", 1, 100, ""),
        text("abbreviation", "Short code", 2, 16, "")
            .code()
            .locked(),
        text("station_code", "Station code", 3, 8, "").code(),
        timezone("timezone", "Timezone", "America/Los_Angeles"),
    ],
)
.on("dsp")
.keywords(&["abbreviation", "station", "timezone"])
.kept(&Profile)
.order(10);

/// The profile as core keeps it: the DSP's own record, and its profile's station code.
struct Profile;
impl Kept for Profile {
    fn load(&self, db: &Store, dsp: &str) -> Result<Value> {
        let own = db.find_dsp(dsp)?;
        Ok(json!({
            "name": own.name,
            // Its short code once it has one; until then, none, for its owner to give.
            "abbreviation": own.code.unwrap_or_default().to_ascii_uppercase(),
            "station_code": db.profile(dsp)?.station_code,
            "timezone": own.timezone,
        }))
    }
    /// Keeps it where onboarding does, without its record in the log, as Settings records the
    /// change: its name and timezone, and its station code and short code once given, each
    /// left unset until then. Settings checked each as its field holds it.
    fn keep(&self, db: &Store, dsp: &str, _: &str, values: &Value) -> Result<()> {
        let text = |id: &str| values[id].as_str().unwrap_or_default();
        let given = |id: &str| Some(text(id)).filter(|text| !text.is_empty());
        db.keep_dsp_profile(
            dsp,
            text("name"),
            text("timezone"),
            given("station_code"),
            given("abbreviation"),
        )
    }
}

#[cfg(test)]
#[path = "../tests/backend/profile.rs"]
mod tests;
