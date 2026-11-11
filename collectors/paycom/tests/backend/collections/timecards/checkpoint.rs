use super::*;
use crate as paycom;
use crate::testing;
use dispatch_core::{
    foundation::config::{Config, Environment},
    server::operations,
};
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn resume_is_bound_to_job_roster_period_revision_and_fixed_expiry() -> Result<()> {
    let (_root, state, dsp, job) = checkpoint_started().await?;
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

async fn checkpoint_started() -> Result<(tempfile::TempDir, Arc<State>, String, String)> {
    crate::testing::install(&[&crate::COLLECTOR], &[]);
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
    let mut config = Config::load()?;
    config.root = root.path().into();
    config.fixture = true;
    config.development = true;
    config.environment = Environment::Preview;
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
    Ok((root, state, dsp, job))
}

#[tokio::test]
async fn a_checkpoint_waiting_for_sqlite_does_not_block_foreground_reads() -> Result<()> {
    use std::sync::{Mutex, mpsc};
    static BUSY: Mutex<Option<(tokio::sync::oneshot::Sender<()>, mpsc::Receiver<()>)>> =
        Mutex::new(None);
    fn blocked(_: i32) -> bool {
        if let Some((began, release)) = BUSY.lock().unwrap().take() {
            let _ = began.send(());
            let _ = release.recv();
        }
        true
    }
    let (_root, state, dsp, job) = checkpoint_started().await?;
    let checkpoint = Checkpoint::new(state.clone(), &job, "owner");
    let employee = json!({"code":"AA01","name":"Fixture","department":"Driver","position":"Driver","station":"S","active":true});
    let period = json!({"start":"2026-09-06","end":"2026-09-19"});
    let records = (6..20).map(|day| json!({"employeeCode":"AA01","date":format!("2026-09-{day:02}"),"hours":0,"status":"Complete","punches":[]})).collect::<Vec<_>>();
    let resume = checkpoint
        .prepare(&period, std::slice::from_ref(&employee), "UTC")
        .await?;
    let at = dsp.clone();
    state
        .read(move |db| {
            db.collector(&at, PROVIDER)?.0.busy_handler(Some(blocked))?;
            Ok(())
        })
        .await?;
    let outside = Store::open(state.config.clone(), state.key.clone())?;
    let held = outside.collector(&dsp, PROVIDER)?;
    held.exec("BEGIN IMMEDIATE", [])?;
    let (began, waiting) = tokio::sync::oneshot::channel();
    let (release, blocked_until) = mpsc::channel();
    *BUSY.lock().unwrap() = Some((began, blocked_until));
    let save = tokio::spawn(async move {
        checkpoint
            .save(&resume.token, &employee, &period, &records)
            .await
    });
    let entered = tokio::time::timeout(std::time::Duration::from_secs(5), waiting)
        .await
        .is_ok_and(|result| result.is_ok());
    // The actual save is now inside SQLite's busy handler, before its transaction commits.
    // Release it before failing so the original exclusive-lock implementation exits cleanly.
    let shared = entered && state.transition.try_read().is_ok();
    let read = if shared {
        let at = dsp.clone();
        Some(
            dispatch_core::testing::requested(state.read(move |db| {
                db.collector(&at, PROVIDER)?
                    .count("SELECT count(*) FROM collection_live_items", [])
            }))
            .await,
        )
    } else {
        None
    };
    held.exec("ROLLBACK", [])?;
    release.send(()).unwrap();
    save.await.unwrap()?;
    assert!(entered, "the controlled SQLite write was never reached");
    assert!(
        shared,
        "a Paycom page save held the global exclusive transition while waiting for SQLite"
    );
    assert_eq!(
        read.unwrap()?,
        0,
        "WAL readers see the previous committed page"
    );
    let count = state
        .read(move |db| {
            db.collector(&dsp, PROVIDER)?
                .count("SELECT count(*) FROM collection_live_items", [])
        })
        .await?;
    assert_eq!(count, 14);
    Ok(())
}
