//! Posted scorecard semantics stay with their weekly owner.
use super::{FEEDBACK, RETURNS, SAFETY, weekly_scorecard};
use crate::WeeklyScorecardStore;
use chrono::Datelike;
use dispatch_core::{
    State,
    db::Store,
    mcp::{
        Caller,
        data::{
            Answer,
            access::Access,
            catalog,
            performance::Adapter,
            scope::{param, period, today},
        },
    },
};
use serde_json::{Value, json};
pub const ADAPTERS: &[Adapter] = &[
    Adapter {
        endpoint: "returns",
        view: "posted_scorecard",
        area: RETURNS,
        answer: |db, state, caller, _, query| read(db, state, caller, query, "returns"),
    },
    Adapter {
        endpoint: "safety",
        view: "posted_scorecard",
        area: SAFETY,
        answer: |db, state, caller, _, query| read(db, state, caller, query, "safety"),
    },
    Adapter {
        endpoint: "feedback",
        view: "posted_scorecard",
        area: FEEDBACK,
        answer: |db, state, caller, _, query| read(db, state, caller, query, "feedback"),
    },
];
fn read(db: &Store, state: &State, caller: &Caller, query: &Value, id: &str) -> Answer {
    let mut query = query.clone();
    if query.get("groups_cursor").is_none() {
        query["groups_cursor"] = json!("0");
    }
    let query = &query;
    let mut answer = match id {
        "returns" => weekly_scorecard::posted_returns(db, state, caller, query)?,
        "safety" => weekly_scorecard::safety(db, state, caller, query)?,
        _ => weekly_scorecard::posted_feedback(db, state, caller, query)?,
    };
    answer["counts"] = match id {
        "returns" => {
            json!({"records":answer["returns"],"unit":"return_attempts",
                "hurting_dcr":answer["hurting_dcr"],"contact_missed":answer["contact_missed"]})
        }
        "safety" => {
            json!({"records":answer["events"],"unit":"assessed_safety_events","counting":answer["counting"]})
        }
        _ => json!({"records":answer["feedback"],"unit":"package_feedback_records"}),
    };
    let mut coverage = answer["coverage"]["weeks"].clone();
    coverage["requested"] = coverage["of"].clone();
    coverage
        .as_object_mut()
        .expect("weekly coverage")
        .remove("of");
    coverage["unit"] = json!("weeks");
    answer["coverage"] = coverage;
    if id == "feedback"
        && ["driver", "type", "impacting"]
            .iter()
            .all(|k| param(query, k).is_empty())
    {
        let access = Access::of(db, caller, query)?;
        let dates = period(query, today(access.dsp), "last week")?;
        // A DSP weekly counter cannot answer a single driver's question or a partial week.
        if dates.from.weekday().num_days_from_sunday() == 0
            && dates.to.weekday().num_days_from_sunday() == 6
        {
            let first = (dates.from + chrono::Duration::days(6)).iso_week();
            let last = dates.to.iso_week();
            let station = db.profile(&access.dsp.id)?.station_code;
            let totals=db.weekly_scorecard_db(&access.dsp.id)?.one(
                "SELECT count(*) weeks,SUM(json_extract(x.row,'$.negative_response_cnt')) negative,\
                    SUM(json_extract(x.row,'$.positive_response_cnt')) positive FROM dsp_feedback x \
                    JOIN weekly_scorecard_publications p ON p.id=x.publication_id \
                    WHERE p.active=1 AND p.scope_verified=1 AND p.station=? AND p.week BETWEEN ? AND ?",
                [station,format!("{}-W{:02}",first.year(),first.week()),format!("{}-W{:02}",last.year(),last.week())])?.unwrap_or_default();
            if totals["negative"].is_number() || totals["positive"].is_number() {
                answer["counts"]["reported_feedback"] = json!({"positive":totals["positive"],"negative":totals["negative"],
                    "unit":"dsp_week_feedback_responses","collected_weeks":totals["weeks"],
                    "requested_weeks":(dates.to-dates.from).num_days()/7+1});
                answer["note"] = json!(
                    "Weekly DSP response counters and package feedback records are separate measures and may differ."
                );
            }
        }
    }
    for key in [
        "returns",
        "hurting_dcr",
        "contact_missed",
        "events",
        "counting",
        "feedback",
    ] {
        answer.as_object_mut().expect("answer").remove(key);
    }
    if !catalog::flag(query, "list") {
        answer.as_object_mut().expect("answer").remove("list");
    }
    if let Some(note) = answer["groups"].get_mut("note") {
        *note = json!("Follow groups.next_cursor using groups_cursor with the same filters.");
    }
    Ok(answer)
}
