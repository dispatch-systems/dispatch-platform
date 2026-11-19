//! Which lines of the DSP's phones are on: each by its own switch alone, Dispatch Phone's own line
//! by Dispatch Phone's, another feature's by that feature's, never by the other's. Every line is
//! Dispatch Phone's own or a feature's that depends on Dispatch Phone, whose code it runs on.
use dispatch_core::{Result, db::Store, manifest::registry};

/// The switch the line `line` answers to: Dispatch Phone's for its own (none), else its feature's,
/// by whose switch it's named.
pub(crate) fn switch(line: Option<&str>) -> &str {
    line.unwrap_or(crate::FEATURE.switch.id)
}
/// Whether the line `line` is on for the DSP.
pub(crate) fn on(db: &Store, dsp: &str, line: Option<&str>) -> Result<bool> {
    db.feature_enabled(dsp, switch(line))
}
/// Of `dsps`, those with any line on, in the same order: where the phones' address answers and
/// the upkeep all lines share runs.
pub(crate) fn serving(db: &Store, dsps: Vec<String>) -> Result<Vec<String>> {
    let mut serving = Vec::new();
    for dsp in dsps {
        if On::of(db, &dsp)?.any() {
            serving.push(dsp);
        }
    }
    Ok(serving)
}

/// The lines on for a DSP, as read once: Dispatch Phone's own as none.
#[derive(Clone, Debug)]
pub(crate) struct On(Vec<Option<&'static str>>);
impl On {
    /// The DSP's lines on now.
    pub fn of(db: &Store, dsp: &str) -> Result<Self> {
        let features = db.features(dsp)?;
        Ok(Self(
            every()
                .filter(|line| features.iter().any(|on| on == switch(*line)))
                .collect(),
        ))
    }
    /// Whether any is.
    pub fn any(&self) -> bool {
        !self.0.is_empty()
    }
    /// Whether `line` is.
    pub fn has(&self, line: Option<&str>) -> bool {
        self.0.contains(&line)
    }
    /// The lines as a query binds them: JSON, Dispatch Phone's own as `null`, for `includes`.
    pub fn json(&self) -> String {
        serde_json::json!(self.0).to_string()
    }
    /// SQL that holds where `line`, a phone's line column, is one of those `json` bound as the
    /// parameter `?{at}`.
    pub fn includes(line: &str, at: usize) -> String {
        format!("EXISTS (SELECT 1 FROM json_each(?{at}) j WHERE j.value IS {line})")
    }
}

/// Every line: Dispatch Phone's own, then each of a feature that depends on it.
fn every() -> impl Iterator<Item = Option<&'static str>> {
    let others = registry()
        .features
        .iter()
        .filter(|feature| feature.depends_on.contains(&crate::FEATURE.name))
        .map(|feature| Some(feature.switch.id));
    std::iter::once(None).chain(others)
}
