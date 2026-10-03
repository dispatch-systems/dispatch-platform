use super::*;
use crate::{config::Config, operations, testing};
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn resume_is_bound_to_job_roster_period_revision_and_fixed_expiry() -> Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
    let mut config = Config::load()?;
    config.root = root.path().into();
    config.fixture = true;
    config.development = true;
    config.environment = "preview".into();
    let state = State::new(config)?;
    let (dsp, job) = state
        .run(|db| {
            let bootstrap = operations::bootstrap(
                db,
                "checkpoint@example.test",
                "Test",
                "Owner",
                "checkpoint-password",
            )?;
            let dsp = s(&bootstrap["dsp"], "id").to_owned();
            testing::enable_connection(db, &dsp, paycom::PROVIDER)?;
            testing::set_connection_revision(db, &dsp, paycom::PROVIDER, 1)?;
            let job =
                db.enqueue_for(&dsp, None, "checkpoint-test", paycom::PROVIDER, &json!({}))?;
            db.claim_job("owner", |_, _| true)?;
            Ok((dsp, s(&job, "id").to_owned()))
        })
        .await?;
    let checkpoint = Checkpoint::new(state.clone(), &job, "owner");
    let employee = json!({"code":"AA01","name":"Fixture","department":"Driver","position":"Driver","station":"S","active":true});
    let period = json!({"start":"2026-09-06","end":"2026-09-19"});
    let records = (6..20)
        .map(|day| {
            json!({"employeeCode":"AA01","date":format!("2026-09-{day:02}"),"hours":0,
        "status":"Complete","punches":[]})
        })
        .collect::<Vec<_>>();
    let prepare = || checkpoint.prepare(&period, std::slice::from_ref(&employee), "UTC");
    let initial = prepare().await?;
    checkpoint
        .save(&initial.token, &employee, &period, &records)
        .await?;
    let resumed = prepare().await?;
    assert_eq!(resumed.token, initial.token);
    assert_eq!(resumed.pages["AA01"], records);
    assert!(
        checkpoint
            .save(&initial.token, &employee, &period, &records[..13])
            .await
            .is_err()
    );
    let mut wrong = records.clone();
    wrong[0]["employeeCode"] = json!("BB02");
    assert!(
        checkpoint
            .save(&initial.token, &employee, &period, &wrong)
            .await
            .is_err()
    );

    let mut changed_employee = employee.clone();
    changed_employee["name"] = json!("Changed");
    let changed = checkpoint
        .prepare(&period, &[changed_employee], "UTC")
        .await?;
    assert!(changed.pages.is_empty());
    assert_ne!(changed.token, initial.token);
    checkpoint
        .save(&initial.token, &employee, &period, &records)
        .await?;
    assert!(
        prepare().await?.pages.is_empty(),
        "old token cannot repopulate a replaced checkpoint"
    );
    let current = prepare().await?;
    checkpoint
        .save(&current.token, &employee, &period, &records)
        .await?;
    assert!(
        checkpoint
            .prepare(&period, std::slice::from_ref(&employee), "America/New_York")
            .await?
            .pages
            .is_empty()
    );
    assert!(
        checkpoint
            .prepare(
                &json!({"start":"2026-09-20","end":"2026-10-03"}),
                std::slice::from_ref(&employee),
                "UTC"
            )
            .await?
            .pages
            .is_empty()
    );
    let current = prepare().await?;
    checkpoint
        .save(&current.token, &employee, &period, &records)
        .await?;
    let tenant = dsp.clone();
    let id = job.clone();
    state
        .run(move |db| {
            let storage = db.collector(&tenant, paycom::PROVIDER)?;
            storage.exec(
                "UPDATE collection_checkpoints SET created_at=? WHERE job_id=?",
                params![db::now() - TTL_MS - 1, id],
            )?;
            Ok(())
        })
        .await?;
    let fresh = prepare().await?;
    assert!(fresh.pages.is_empty());
    assert_ne!(fresh.token, current.token);
    checkpoint
        .save(&fresh.token, &employee, &period, &records)
        .await?;
    let tenant = dsp.clone();
    let id = job.clone();
    state
        .run(move |db| {
            testing::set_connection_revision(db, &tenant, paycom::PROVIDER, 2)?;
            testing::set_job_connection_revision(db, &id, 2)?;
            Ok(())
        })
        .await?;
    let revised = prepare().await?;
    assert!(revised.pages.is_empty());
    assert_ne!(fresh.token, revised.token);
    checkpoint
        .save(&revised.token, &employee, &period, &records)
        .await?;
    let tenant = dsp.clone();
    let id = job.clone();
    let other = state
        .run(move |db| {
            db.finish(&id, "owner", Some("provider_unavailable"))?;
            let other =
                db.enqueue_for(&tenant, None, "separate-job", paycom::PROVIDER, &json!({}))?;
            db.claim_job("new-owner", |_, _| true)?;
            Ok(s(&other, "id").to_owned())
        })
        .await?;
    let separate = Checkpoint::new(state.clone(), &other, "new-owner");
    assert!(
        separate
            .prepare(&period, std::slice::from_ref(&employee), "UTC")
            .await?
            .pages
            .is_empty()
    );
    let tenant = dsp.clone();
    state
        .run(move |db| {
            db.finish(&other, "new-owner", None)?;
            assert_eq!(
                db.collector(&tenant, paycom::PROVIDER)?
                    .all("SELECT * FROM collection_checkpoint_pages", [])?
                    .len(),
                1,
                "a separate job cannot consume or clear another job's pages"
            );
            Ok(())
        })
        .await?;
    let tenant = dsp.clone();
    let id = job.clone();
    state
        .run(move |db| {
            db.cancel(&id, &tenant)?;
            Ok(())
        })
        .await?;
    assert!(
        checkpoint
            .save(&revised.token, &employee, &period, &records)
            .await
            .is_err()
    );
    state
        .run(move |db| {
            assert!(
                db.collector(&dsp, paycom::PROVIDER)?
                    .all("SELECT * FROM collection_checkpoint_pages", [])?
                    .is_empty()
            );
            Ok(())
        })
        .await?;
    Ok(())
}
