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
            tokio::spawn(requested(async move { state.run_bookkeeping(|_| Ok(())).await }))
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
