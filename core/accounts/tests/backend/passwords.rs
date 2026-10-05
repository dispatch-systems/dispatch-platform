use std::{sync::Arc, time::Duration};
#[tokio::test]
async fn password_work_is_bounded_without_holding_database_slots() {
    crate::testing::install(&[], &[]);
    let root = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = crate::foundation::config::Config::load().unwrap();
    config.root = root.path().into();
    let state = crate::State::new(config).unwrap();
    let mut workers = Vec::new();
    let mut release = Vec::new();
    for _ in 0..2 {
        let (began, started) = tokio::sync::oneshot::channel();
        let (send, wait) = std::sync::mpsc::channel();
        release.push(send);
        let state = Arc::clone(&state);
        workers.push(tokio::spawn(async move {
            state
                .password_work(move || {
                    let _ = began.send(());
                    let _ = wait.recv();
                    Ok(())
                })
                .await
        }));
        started.await.unwrap();
    }
    let busy = state.password_work(|| Ok(())).await;
    let read = tokio::time::timeout(
        Duration::from_secs(2),
        state.read(|db| db.platform.one("SELECT 1 ready", [])),
    )
    .await;
    for send in release {
        send.send(()).unwrap();
    }
    for worker in workers {
        worker.await.unwrap().unwrap();
    }
    assert_eq!(busy.unwrap_err().code, "login_busy");
    assert_eq!(read.unwrap().unwrap().unwrap()["ready"], 1);
    assert_eq!(state.password_slots.available_permits(), 2);
}
