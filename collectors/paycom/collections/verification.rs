//! Verification reads only a selected day's timecards through the ordinary collector.
use dispatch_core::{Error, Result, db::Store};
use serde_json::{Value, json};

pub(crate) fn request(db: &Store, dsp: &str, input: &Value) -> Result<Value> {
    let scope = db.find_dsp(dsp)?;
    let timezone: chrono_tz::Tz = scope
        .timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    let date = crate::timecards::collection_date(input, &scope.timezone)?
        .unwrap_or_else(|| chrono::Utc::now().with_timezone(&timezone).date_naive());
    Ok(json!({"date":date.to_string()}))
}

#[cfg(test)]
#[path = "../tests/backend/verification.rs"]
mod tests;
