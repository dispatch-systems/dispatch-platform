use crate::testing::requested;
use std::{sync::Arc, time::Duration};

fn started() -> (tempfile::TempDir, Arc<crate::State>) {
    crate::testing::install(&[], &[]);
    let root = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = crate::foundation::config::Config::load().unwrap();
    config.root = root.path().into();
    (root, crate::State::new(config).unwrap())
}
fn ready(db: &crate::db::Store) -> crate::Result<Option<serde_json::Value>> {
    db.platform.one("SELECT 1 ready", [])
}

#[tokio::test]
async fn a_running_job_waits_its_turn_at_the_database_where_a_request_is_turned_away() {
    let (_root, state) = started();
    // One write holds the transition lock, as another DSP's publication may, and requests'
    // writes waiting behind it take every other slot.
    let (began, started) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let long = tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            state
                .run_bookkeeping(move |_| {
                    let _ = began.send(());
                    let _ = wait.recv();
                    Ok(())
                })
                .await
        }
    });
    started.await.unwrap();
    let behind: Vec<_> = (0..state.db_slots.available_permits())
        .map(|_| {
            let state = Arc::clone(&state);
            tokio::spawn(requested(
                async move { state.run_bookkeeping(|_| Ok(())).await },
            ))
        })
        .collect();
    while state.db_slots.available_permits() > 0 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let job = tokio::spawn({
        let state = Arc::clone(&state);
        async move { state.read(ready).await }
    });
    let request = requested(state.read(ready)).await;
    assert_eq!(request.unwrap_err().code, "platform_busy");
    release.send(()).unwrap();
    long.await.unwrap().unwrap();
    for write in behind {
        write.await.unwrap().unwrap();
    }
    assert_eq!(job.await.unwrap().unwrap().unwrap()["ready"], 1);
}

#[tokio::test]
async fn a_request_takes_a_slot_while_work_outside_requests_waits_for_every_slot() {
    let (_root, state) = started();
    // Work outside a request, as every DSP's upkeep starting at once, asks for every slot, and
    // holds every turn it may take.
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let wait = Arc::new(std::sync::Mutex::new(wait));
    let holding: Vec<_> = (0..state.db_slots.available_permits())
        .map(|_| {
            let (state, wait) = (Arc::clone(&state), Arc::clone(&wait));
            tokio::spawn(async move {
                state
                    .read(move |_| {
                        let _ = wait.lock().unwrap().recv();
                        Ok(())
                    })
                    .await
            })
        })
        .collect();
    while state.background_slots.available_permits() > 0 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // A request is answered while that work still holds its turns and the rest of it waits.
    let request = requested(state.read(ready)).await;
    assert_eq!(request.unwrap().unwrap()["ready"], 1);
    assert!(holding.iter().all(|work| !work.is_finished()));
    for _ in &holding {
        release.send(()).unwrap();
    }
    for work in holding {
        work.await.unwrap().unwrap();
    }
}

fn tenant_started() -> (tempfile::TempDir, Arc<crate::State>, String) {
    crate::testing::install(&[], &[]);
    let (root, store, dsp) = crate::testing::bootstrapped();
    let config = store.config.clone();
    drop(store);
    (root, crate::State::new(config).unwrap(), dsp)
}

#[tokio::test]
async fn unpublished_writes_allow_foreground_wal_reads_and_wait_by_tenant_before_the_pool() {
    let (_root, state, dsp) = tenant_started();
    let at = dsp.clone();
    state
        .run_bookkeeping(move |db| {
            let data = db.dsp(&at)?;
            data.exec(
                "CREATE TABLE unpublished_probe(id INTEGER PRIMARY KEY,visible INTEGER NOT NULL)",
                [],
            )?;
            data.exec("INSERT INTO unpublished_probe VALUES(1,1)", [])?;
            Ok(())
        })
        .await
        .unwrap();
    let revision = state
        .data_revision
        .load(std::sync::atomic::Ordering::Relaxed);
    let (began, writing) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let first = tokio::spawn({
        let (state, at, dsp) = (state.clone(), dsp.clone(), dsp.clone());
        async move {
            state
                .run_unpublished(dsp, move |db| {
                    let data = db.dsp(&at)?;
                    data.transaction(|| {
                        data.exec("INSERT INTO unpublished_probe VALUES(2,0)", [])?;
                        let _ = began.send(());
                        let _ = wait.recv();
                        Ok(())
                    })
                })
                .await
        }
    });
    writing.await.unwrap();
    // Check before awaiting a reader so an accidental exclusive-lock regression can release
    // the controlled writer and fail cleanly rather than deadlocking the test.
    let shared = state.transition.try_read().is_ok();
    if !shared {
        release.send(()).unwrap();
        first.await.unwrap().unwrap();
        panic!("unpublished work held the exclusive transition");
    }
    let free = state.db_slots.available_permits();
    let (release_second, wait_second) = std::sync::mpsc::channel();
    let mut second = Box::pin(state.run_unpublished(dsp.clone(), move |_| {
        let _ = wait_second.recv();
        Ok(())
    }));
    let pending =
        std::future::poll_fn(|cx| std::task::Poll::Ready(second.as_mut().poll(cx).is_pending()))
            .await;
    let free_while_waiting = state.db_slots.available_permits();
    drop(second);
    let _ = release_second.send(());
    let at = dsp.clone();
    let visible = requested(state.read(move |db| {
        let data = db.dsp(&at)?;
        Ok((
            data.count("SELECT count(*) FROM unpublished_probe", [])?,
            data.count("SELECT count(*) FROM unpublished_probe WHERE visible=1", [])?,
        ))
    }))
    .await;
    release.send(()).unwrap();
    first.await.unwrap().unwrap();
    assert!(pending);
    assert_eq!(free_while_waiting, free);
    assert_eq!(
        visible.unwrap(),
        (1, 1),
        "the reader sees the committed snapshot while staging writes"
    );
    assert_eq!(
        state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed),
        revision
    );
    let counts = state
        .read(move |db| {
            let data = db.dsp(&dsp)?;
            Ok((
                data.count("SELECT count(*) FROM unpublished_probe", [])?,
                data.count("SELECT count(*) FROM unpublished_probe WHERE visible=1", [])?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(counts, (2, 1));
}

#[tokio::test]
async fn unpublished_work_rechecks_tenant_lifetime_after_its_keyed_wait() {
    let (_root, state, dsp) = tenant_started();
    let held = state
        .unpublished_turns
        .enter_for(dsp.clone(), super::PATIENCE)
        .await
        .unwrap();
    let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let changed = called.clone();
    let mut waiting = Box::pin(state.run_unpublished(dsp.clone(), move |_| {
        changed.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }));
    assert!(
        std::future::poll_fn(|cx| {
            std::task::Poll::Ready(waiting.as_mut().poll(cx).is_pending())
        })
        .await
    );
    state
        .run(move |db| crate::testing::set_dsp_status(db, &dsp, "suspended"))
        .await
        .unwrap();
    drop(held);
    assert_eq!(waiting.await.unwrap_err().code, "dsp_unavailable");
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
}
