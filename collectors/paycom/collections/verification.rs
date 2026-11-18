//! Verification reads only a selected day's timecards through the ordinary collector.
use dispatch_core::{Result, db::Store};
use serde_json::{Value, json};

pub(crate) fn request(db: &Store, dsp: &str, input: &Value) -> Result<Value> {
    let scope = db.find_dsp(dsp)?;
    let timezone = dispatch_core::foundation::clock::zone(&scope.timezone)?;
    let date = crate::timecards::collection_date(input, &scope.timezone)?
        .unwrap_or_else(|| dispatch_core::foundation::clock::today(&timezone));
    Ok(json!({"date":date.to_string()}))
}

#[cfg(test)]
#[path = "../tests/backend/verification.rs"]
mod tests;
