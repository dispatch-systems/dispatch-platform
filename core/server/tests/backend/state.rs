use std::{sync::Arc, time::Duration};

#[tokio::test]
async fn a_running_job_waits_its_turn_at_the_database_where_a_request_is_turned_away() {
    crate::testing::install(&[], &[]);
    let root = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = crate::foundation::config::Config::load().unwrap();
    config.root = root.path().into();
    let state = crate::State::new(config).unwrap();
    // One write holds the transition lock, as another DSP's publication may, and writes
    // waiting behind it take every other slot.
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
            tokio::spawn(async move { state.run_bookkeeping(|_| Ok(())).await })
        })
        .collect();
    while state.db_slots.available_permits() > 0 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let ready = |db: &crate::db::Store| db.platform.one("SELECT 1 ready", []);
    let job = tokio::spawn({
        let state = Arc::clone(&state);
        crate::State::patiently(async move { state.read(ready).await })
    });
    let request = state.read(ready).await;
    assert_eq!(request.unwrap_err().code, "platform_busy");
    release.send(()).unwrap();
    long.await.unwrap().unwrap();
    for write in behind {
        write.await.unwrap().unwrap();
    }
    assert_eq!(job.await.unwrap().unwrap().unwrap()["ready"], 1);
}
