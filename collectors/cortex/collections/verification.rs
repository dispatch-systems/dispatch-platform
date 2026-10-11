//! Verification binds every read to the configured account's selected provider scope.
use dispatch_core::{Error, Result, db::Store, ensure, foundation::validate};
use serde_json::{Value, json};

pub(crate) fn request(db: &Store, dsp: &str, input: &Value) -> Result<Value> {
    validate::fields(input, &["date", "week", "collection"])?;
    ensure(
        ["date", "week", "collection"]
            .iter()
            .all(|field| input.get(*field).is_none_or(Value::is_string)),
        "invalid_input",
        400,
    )?;
    let dsp_scope = db.find_dsp(dsp)?;
    let timezone: chrono_tz::Tz = dsp_scope
        .timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    let today = chrono::Utc::now().with_timezone(&timezone).date_naive();
    let date = input["date"].as_str().map_or(Ok(today), |date| {
        validate::date(date)?;
        date.parse::<chrono::NaiveDate>()
            .map_err(|_| Error::new("invalid_date", 400))
    })?;
    ensure(date <= today, "invalid_date", 400)?;
    let profile = db.profile(dsp)?;
    ensure(
        !profile.station_code.is_empty() && !profile.abbreviation.is_empty(),
        "invalid_verification_reference",
        400,
    )?;
    let collection = input["collection"].as_str().unwrap_or("meal_break");
    ensure(
        input.get("week").is_none() || matches!(collection, "weekly_scorecard" | "dvic"),
        "invalid_input",
        400,
    )?;
    let mut scope = json!({
        "date":date.to_string(),
        "station":profile.station_code,
        "timezone":dsp_scope.timezone,
        "dspName":dsp_scope.name,
        "dspAbbreviation":profile.abbreviation
    });
    match collection {
        "meal_break" => {}
        "routes" => {
            scope["collection"] = json!(collection);
            scope["mode"] = json!(if date == today { "snapshot" } else { "final" });
        }
        "daily_performance" => scope["collection"] = json!(collection),
        "weekly_scorecard" | "dvic" => {
            let week =
                dispatch_core::foundation::weeks::week_or_latest(input["week"].as_str(), today)?;
            scope["collection"] = json!(collection);
            if collection == "dvic" {
                scope["weeks"] = json!([week]);
            } else {
                scope.as_object_mut().expect("scope").remove("date");
                scope["week"] = json!(week);
            }
        }
        _ => return Err(Error::new("invalid_input", 400)),
    }
    Ok(scope)
}

#[cfg(test)]
#[path = "../tests/backend/verification.rs"]
mod tests;
