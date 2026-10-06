//! Daily evaluation data goes through the same validated publisher as the collector.
use crate::DailyPerformanceStore;
use dispatch_core::{
    Result,
    db::{Store, s},
    manifest::registry,
    mcp::synthetic::{Made, Step, Synthetic, World, packages_at, plan, roll},
};
use dispatch_cortex::{
    self as cortex,
    daily_performance::{self, Request},
    discovery::CollectionRequest,
};
use serde_json::json;
pub const SYNTHETIC: Synthetic = Synthetic {
    people: None,
    steps: &[Step {
        order: 55,
        run: days,
    }],
};
fn days(db: &Store, world: &mut World) -> Result<Made> {
    world.connect(db, cortex::PROVIDER)?;
    let id = world.dsp.as_str();
    for (offset, date) in world.dates.iter().enumerate() {
        let date = date.to_string();
        let jobs = db.enqueue_daily_performance(
            id,
            None,
            &format!("synthetic-daily:{date}"),
            &date,
            &date,
        )?;
        let job = s(&jobs[0], "id");
        let payload = serde_json::from_str(&db.job_row(job, Some(id))?.request)?;
        let request: Request = serde_json::from_value(
            registry()
                .keeper(daily_performance::JOB_KIND)
                .bind(db, id, &payload)?,
        )?;
        let mut capture = daily_performance::fixture(&request)?;
        let (mut quality, mut feedback, mut returns, mut safety) = (vec![], vec![], vec![], vec![]);
        for (index, _, name) in &world.drivers {
            let Some(day) = plan(*index, offset as i64) else {
                continue;
            };
            let seed = roll(*index * 1000 + offset as u64);
            let common = json!({"data_date":date, "company_id":capture.company_id,
                "station_code":capture.station, "dsp_code":capture.dsp_code,
                "transporter_id":world.transporter(*index), "da_name":name});
            let mut row = common.clone();
            row["delivered"] = json!(
                (1..=day.stops - day.missed)
                    .map(|stop| packages_at(*index, offset as i64, stop))
                    .sum::<i64>()
            );
            row["pod_success_rate"] = json!(99.0);
            quality.push(row);
            let mut row = common.clone();
            row["positive_response_cnt"] = json!(seed % 5);
            row["negative_response_cnt"] = json!(seed % 3);
            row["driver_mishandled_package_cnt"] = json!(seed % 3);
            feedback.push(row);
            if day.missed > 0 {
                let mut row = common.clone();
                row["tracking_id"] = json!(format!("TBA-DAILY-{date}-{index}"));
                row["rts_reason_code"] = json!("BUSINESS CLOSED");
                row["impacting_dcr"] = json!("Y");
                row["daily_coaching"] = json!("No contact attempted: call or text customer");
                returns.push(row);
            }
            if seed.is_multiple_of(3) {
                let mut row = common;
                row["event_id"] = json!(format!("daily-{date}-{index}"));
                row["dashboard_metric_type"] = json!("Speeding");
                row["oss_impact_flag"] = json!(1);
                row["final_resolution"] = json!("None");
                safety.push(row);
            }
        }
        for dataset in &mut capture.datasets {
            dataset.rows = match daily_performance::dataset(&dataset.id)
                .expect("fixture dataset")
                .table
            {
                "driver_quality" => quality.clone(),
                "driver_feedback" => feedback.clone(),
                "returns_to_station" => returns.clone(),
                "safety_events" | "live_safety_events" => safety.clone(),
                _ => vec![],
            };
        }
        let CollectionRequest::Discover(discovery) = request.scope_request() else {
            unreachable!()
        };
        db.publish_daily_performance(
            id,
            job,
            &capture,
            &discovery.scope("area-synthetic", "company-fixture")?,
        )?;
        world.collected(db, job)?;
    }
    Ok(Some(("daily_performance_days", json!(world.dates.len()))))
}
