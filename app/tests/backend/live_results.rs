//! Live results from both collectors, overlaid on Timecard's views before they publish.
use crate::{
    collectors::cortex::{self, discovery::Scope, live::Writer},
    workforce::TimecardStore,
};
use dispatch_core::{
    Result, State, collection::registry::Provider, db::s, foundation::config::Config,
    server::operations,
};
use dispatch_paycom::{self as paycom, fixtures, timecards::Checkpoint};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn driver_results_overlay_both_views_without_publishing_and_revert_on_failure() -> Result<()>
{
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
    let mut config = Config::load()?;
    config.root = root.path().into();
    config.fixture = true;
    config.development = true;
    config.environment = "preview".into();
    let state = State::new(config)?;
    let (dsp, paycom_job, old) = state
        .run(|db| {
            let bootstrap = operations::bootstrap(
                db,
                "live@example.test",
                "Live",
                "Owner",
                "live-password-test",
            )?;
            let dsp = s(&bootstrap["dsp"], "id").to_owned();
            for provider in Provider::all() {
                db.collector(&dsp, provider)?
                    .exec("UPDATE connections SET enabled=1", [])?;
            }
            let old = fixtures::fixture_date("UTC", Some("2026-01-19".parse().unwrap()))?;
            db.publish_timecards(&dsp, &old)?;
            let paycom = db.enqueue_timecards(&dsp, None, "live-paycom")?;
            db.claim_job("paycom-owner", |_, _| true)?;
            Ok((dsp, s(&paycom, "id").to_owned(), old))
        })
        .await?;
    let period = json!({"start":"2026-01-06","end":"2026-01-19"});
    let employee = old["employees"][0].clone();
    let records = (6..20)
        .map(|day| {
            json!({"employeeCode":employee["code"],"date":format!("2026-01-{day:02}"),
        "hours":9,"status":"Complete","punches":[{"in":"09:00","out":"18:00","hours":9}]})
        })
        .collect::<Vec<_>>();
    let checkpoint = Checkpoint::new(state.clone(), &paycom_job, "paycom-owner");
    let resume = checkpoint
        .prepare(&period, old["employees"].as_array().unwrap(), "UTC")
        .await?;
    let tenant = dsp.clone();
    state
        .read(move |db| {
            let live =
                db.live_results_range(&tenant, paycom::PROVIDER, "2026-01-05", "2026-01-20")?;
            assert_eq!(live.len(), 14, "only metadata-covered days are included");
            assert_eq!(
                live["2026-01-06"][0].0["roster"].as_array().unwrap().len(),
                12
            );
            assert!(
                live.values()
                    .all(|runs| runs.len() == 1 && runs[0].1.is_empty())
            );
            Ok(())
        })
        .await?;
    let mut updates = state.updates.subscribe(&dsp);
    let other_updates = state.updates.subscribe("unrelated-dsp");
    let before = state.updates.token(&updates);
    checkpoint
        .save(&resume.token, &employee, &period, &records)
        .await?;
    updates.changed().await.unwrap();
    assert_ne!(before, state.updates.token(&updates));
    assert!(!other_updates.has_changed().unwrap());
    let tenant = dsp.clone();
    let old_snapshot = old.clone();
    state
        .run(move |db| {
            let daily = db
                .daily_timecards(&tenant, "2026-01-19", "name", false)
                .map(|value| serde_json::to_value(value).unwrap())?;
            assert_eq!(daily["rows"].as_array().unwrap().len(), 12);
            let row = daily["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["employeeCode"] == "E001")
                .unwrap();
            assert_eq!(row["hours"], 9.0);
            assert_eq!(daily["collectedAt"], old_snapshot["collectedAt"]);
            let range =
                db.live_results_range(&tenant, paycom::PROVIDER, "2026-01-18", "2026-01-20")?;
            assert_eq!(range.len(), 2);
            for day in ["2026-01-18", "2026-01-19"] {
                assert_eq!(range[day], db.live_results(&tenant, paycom::PROVIDER, day)?);
                assert_eq!(range[day][0].1.len(), 1);
            }
            let comparison = db
                .meal_comparison(&tenant, "2026-01-19", "UTC")
                .map(|value| serde_json::to_value(value).unwrap())?;
            let row = comparison["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == "paycom:E001")
                .unwrap();
            assert_eq!(row["paycom"]["punches"][0]["in"], "09:00");
            assert!(
                db.daily_timecards(&tenant, "2026-02-01", "name", false)
                    .map(|value| serde_json::to_value(value).unwrap())?["rows"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            // A changed connection must immediately hide the old attempt.
            db.collector(&tenant, paycom::PROVIDER)?
                .exec("UPDATE connections SET revision=revision+1", [])?;
            assert!(
                db.live_results(&tenant, paycom::PROVIDER, "2026-01-19")?
                    .is_empty()
            );
            db.collector(&tenant, paycom::PROVIDER)?
                .exec("UPDATE connections SET revision=revision-1", [])?;
            Ok(())
        })
        .await?;
    let mut empty = records.clone();
    empty.last_mut().unwrap()["punches"] = json!([]);
    checkpoint
        .save(&resume.token, &employee, &period, &empty)
        .await?;
    let tenant = dsp.clone();
    let job = paycom_job.clone();
    state
        .run(move |db| {
            let comparison = db
                .meal_comparison(&tenant, "2026-01-19", "UTC")
                .map(|value| serde_json::to_value(value).unwrap())?;
            assert!(
                comparison["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|r| r["id"] != "paycom:E001")
            );
            db.finish(&job, "paycom-owner", Some("invalid_checkpoint"))?;
            let daily = db
                .daily_timecards(&tenant, "2026-01-19", "name", false)
                .map(|value| serde_json::to_value(value).unwrap())?;
            let row = daily["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["employeeCode"] == "E001")
                .unwrap();
            assert_eq!(row["hours"], 8.5);
            assert!(
                db.collector(&tenant, paycom::PROVIDER)?
                    .all("SELECT * FROM collection_live_items", [])?
                    .is_empty()
            );
            Ok(())
        })
        .await?;
    assert!(
        checkpoint
            .save(&resume.token, &employee, &period, &records)
            .await
            .is_err()
    );
    let scope = Scope {
        date: "2026-01-19".into(),
        station: "DEMO1".into(),
        service_area_id: "area-1".into(),
        provider: "provider-1".into(),
        timezone: "UTC".into(),
    };
    let tenant = dsp.clone();
    let requested = scope.clone();
    let flex_job = state
        .run(move |db| {
            let job = db.enqueue_meals(&tenant, None, "live-flex", &requested)?;
            db.claim_job("flex-owner", |_, _| true)?;
            Ok(s(&job, "id").to_owned())
        })
        .await?;
    let mut capture = crate::collectors::cortex::meals::fixture(&scope);
    capture.itineraries[0].driver = s(&employee, "name").into();
    let writer = Writer::new(state.clone(), &flex_job, "flex-owner");
    writer
        .start_cortex(
            &scope,
            json!([{"id":"fixture-driver","name":employee["name"]}]),
        )
        .await?;
    writer.cortex(&capture).await?;
    let tenant = dsp.clone();
    state
        .read(move |db| {
            let comparison = db
                .meal_comparison(&tenant, "2026-01-19", "UTC")
                .map(|value| serde_json::to_value(value).unwrap())?;
            let row = comparison["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == "paycom:E001")
                .unwrap();
            assert_eq!(row["cortex"].as_array().unwrap().len(), 1);
            assert!(
                comparison["cortexPublications"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            Ok(())
        })
        .await?;
    // Invalid data leaves the previous validated driver result intact.
    let mut invalid = capture.clone();
    invalid.scope.provider = "other-provider".into();
    assert_eq!(
        writer.cortex(&invalid).await.unwrap_err().code,
        "cortex_scope_mismatch"
    );
    invalid = capture.clone();
    invalid.itineraries[0].meals[0].end = Some(0);
    assert!(writer.cortex(&invalid).await.is_err());
    capture.itineraries[0].meals.clear();
    writer.cortex(&capture).await?;
    state
        .run(move |db| {
            let comparison = db
                .meal_comparison(&dsp, "2026-01-19", "UTC")
                .map(|value| serde_json::to_value(value).unwrap())?;
            assert!(
                comparison["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|r| r["cortex"].as_array().unwrap().is_empty())
            );
            db.finish(&flex_job, "flex-owner", Some("invalid_cortex_capture"))?;
            assert!(
                db.collector(&dsp, cortex::PROVIDER)?
                    .all("SELECT * FROM collection_live_items", [])?
                    .is_empty()
            );
            Ok(())
        })
        .await?;
    Ok(())
}
