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
    let mut behind: Vec<_> = (0..state.db_slots.available_permits())
        .map(|_| {
            let state = Arc::clone(&state);
            Box::pin(requested(
                async move { state.run_bookkeeping(|_| Ok(())).await },
            ))
        })
        .collect();
    for write in &mut behind {
        let pending =
            std::future::poll_fn(|cx| std::task::Poll::Ready(write.as_mut().poll(cx).is_pending()))
                .await;
        assert!(pending);
    }
    assert_eq!(state.db_slots.available_permits(), 0);
    let job = tokio::spawn({
        let state = Arc::clone(&state);
        async move { state.read(ready).await }
    });
    let request = requested(state.read(ready)).await;
    release.send(()).unwrap();
    long.await.unwrap().unwrap();
    for write in behind {
        write.await.unwrap();
    }
    assert_eq!(job.await.unwrap().unwrap().unwrap()["ready"], 1);
    let error = request.unwrap_err();
    assert_eq!(error.code, "platform_busy");
    assert_eq!(error.admission_origin(), Some("db_slots_timeout"));
}

#[tokio::test]
async fn request_queue_refusals_keep_their_origin_before_the_work_runs() {
    use std::sync::atomic::{AtomicBool, Ordering};
    for closed in [false, true] {
        let (_root, state) = started();
        let held = state
            .db_queue
            .clone()
            .acquire_many_owned(state.db_queue.available_permits() as u32)
            .await
            .unwrap();
        if closed {
            state.db_queue.close();
        }
        let ran = Arc::new(AtomicBool::new(false));
        let entered = ran.clone();
        let error = requested(state.read(move |_| {
            entered.store(true, Ordering::SeqCst);
            Ok(())
        }))
        .await
        .unwrap_err();
        assert_eq!((error.code.as_str(), error.status), ("platform_busy", 503));
        assert_eq!(
            error.admission_origin(),
            Some(if closed {
                "db_queue_closed"
            } else {
                "db_queue_full"
            })
        );
        assert!(!ran.load(Ordering::SeqCst));
        drop(held);
        if !closed {
            assert_eq!(
                requested(state.read(ready)).await.unwrap().unwrap()["ready"],
                1
            );
        }
    }
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

#[tokio::test]
async fn polling_saturation_leaves_foreground_admission_and_execution() {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    for patient in [false, true] {
        let (_root, state) = started();
        let (entered, mut starts) = tokio::sync::mpsc::unbounded_channel();
        let (release, wait) = mpsc::channel();
        let wait = Arc::new(Mutex::new(wait));
        let ran = Arc::new(AtomicUsize::new(0));
        let mut pending = Vec::new();
        let mut refused = Vec::new();
        // Drive real traced database futures into admission. The first 56 may wait as a
        // claimed job does; excess ordinary polls still exercise the bounded class queue.
        for ordinal in 0..64 {
            let (entered, wait, ran) = (entered.clone(), wait.clone(), ran.clone());
            let work = crate::State::polling(state.read(move |_| {
                let _ = entered.send(());
                let _ = wait.lock().unwrap().recv();
                ran.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }));
            let mut call = Box::pin(requested(async move {
                if patient && ordinal < 56 {
                    crate::State::patiently(work).await
                } else {
                    work.await
                }
            }));
            match std::future::poll_fn(|cx| std::task::Poll::Ready(call.as_mut().poll(cx))).await {
                std::task::Poll::Pending => pending.push(call),
                std::task::Poll::Ready(result) => refused.push(result),
            }
        }
        for _ in 0..3 {
            starts.recv().await.unwrap();
        }
        let admitted = pending.len();
        let queued_capacity = state.db_queue.available_permits();
        let worker_capacity = state.db_slots.available_permits();
        // A routine caller waiting for its worker has not entered the global queue.
        drop(pending.pop());
        let after_cancel = (
            state.db_queue.available_permits(),
            state.routine_queue.available_permits(),
        );
        let foreground = requested(state.read(ready)).await;
        // A burst of dashboard work queues behind its own active request while the three
        // routine workers stay occupied. It must not lose admission to routine waiters.
        let (began, started) = tokio::sync::oneshot::channel();
        let (resume, paused) = mpsc::channel();
        let mut holding = Box::pin(requested(state.read(move |_| {
            let _ = began.send(());
            let _ = paused.recv();
            Ok(())
        })));
        assert!(
            std::future::poll_fn(|cx| {
                std::task::Poll::Ready(holding.as_mut().poll(cx).is_pending())
            })
            .await
        );
        started.await.unwrap();
        let mut ordinary = Vec::new();
        let mut answers = Vec::new();
        for _ in 0..40 {
            let mut call = Box::pin(requested(state.read(ready)));
            match std::future::poll_fn(|cx| std::task::Poll::Ready(call.as_mut().poll(cx))).await {
                std::task::Poll::Pending => ordinary.push(call),
                std::task::Poll::Ready(result) => answers.push(result),
            }
        }
        // Release before asserting, so every started blocking callback finishes even when an
        // assertion below fails.
        resume.send(()).unwrap();
        for _ in 0..64 {
            release.send(()).unwrap();
        }
        holding.await.unwrap();
        for call in ordinary {
            answers.push(call.await);
        }
        let mut finished = Vec::new();
        for call in pending {
            finished.push(call.await);
        }
        assert_eq!(foreground.unwrap().unwrap()["ready"], 1);
        assert_eq!(answers.len(), 40);
        for answer in answers {
            assert_eq!(answer.unwrap().unwrap()["ready"], 1);
        }
        for result in finished {
            result.unwrap();
        }
        assert_eq!(admitted, 56);
        assert_eq!((queued_capacity, worker_capacity), (61, 1));
        assert_eq!(after_cancel, (queued_capacity, 1));
        assert_eq!(refused.len(), 8);
        for result in refused {
            let error = result.unwrap_err();
            assert_eq!((error.code.as_str(), error.status), ("platform_busy", 503));
            assert_eq!(error.admission_origin(), Some("db_routine_queue_full"));
        }
        assert_eq!(ran.load(Ordering::SeqCst), 55);
        assert_eq!(state.db_queue.available_permits(), 64);
        assert_eq!(state.db_slots.available_permits(), 4);
        assert_eq!(state.routine_queue.available_permits(), 56);
        assert_eq!(state.routine_slots.available_permits(), 3);
    }
}

#[tokio::test(start_paused = true)]
async fn polling_worker_waits_share_one_deadline_and_release_cancelled_admission() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let (_root, state) = started();
    let mut held = Vec::new();
    for _ in 0..3 {
        held.push(state.routine_slots.clone().acquire_owned().await.unwrap());
    }
    let global = state.db_slots.clone().acquire_many_owned(4).await.unwrap();
    let called = Arc::new(AtomicBool::new(false));
    let seen = called.clone();
    let mut waiting = Box::pin(requested(crate::State::polling(state.read(move |_| {
        seen.store(true, Ordering::SeqCst);
        Ok(())
    }))));
    assert!(
        std::future::poll_fn(|cx| {
            std::task::Poll::Ready(waiting.as_mut().poll(cx).is_pending())
        })
        .await
    );
    assert_eq!(state.db_queue.available_permits(), 64);
    tokio::time::advance(Duration::from_millis(1500)).await;
    drop(held.pop());
    assert!(
        std::future::poll_fn(|cx| {
            std::task::Poll::Ready(waiting.as_mut().poll(cx).is_pending())
        })
        .await
    );
    assert_eq!(state.db_queue.available_permits(), 63);
    tokio::time::advance(Duration::from_millis(500)).await;
    let answer = std::future::poll_fn(|cx| std::task::Poll::Ready(waiting.as_mut().poll(cx))).await;
    let std::task::Poll::Ready(answer) = answer else {
        panic!("the second worker wait restarted the original deadline");
    };
    let error = answer.unwrap_err();
    assert_eq!(error.admission_origin(), Some("db_slots_timeout"));
    assert!(!called.load(Ordering::SeqCst));
    assert_eq!(state.db_queue.available_permits(), 64);
    assert_eq!(state.routine_queue.available_permits(), 56);
    assert_eq!(state.routine_slots.available_permits(), 1);
    drop(global);
    drop(held);

    // Cancellation while waiting at either gate returns all the caller's admission.
    for patient in [false, true] {
        for at_global_queue in [false, true] {
            let class = if at_global_queue {
                None
            } else {
                Some(
                    state
                        .routine_slots
                        .clone()
                        .acquire_many_owned(3)
                        .await
                        .unwrap(),
                )
            };
            let queue = if at_global_queue {
                Some(state.db_queue.clone().acquire_many_owned(64).await.unwrap())
            } else {
                None
            };
            let work = crate::State::polling(state.read(ready));
            let mut waiting = Box::pin(requested(async move {
                if patient {
                    crate::State::patiently(work).await
                } else {
                    work.await
                }
            }));
            let answer =
                std::future::poll_fn(|cx| std::task::Poll::Ready(waiting.as_mut().poll(cx))).await;
            if at_global_queue && !patient {
                let std::task::Poll::Ready(answer) = answer else {
                    panic!("a full global queue must refuse an ordinary poll");
                };
                assert_eq!(
                    answer.unwrap_err().admission_origin(),
                    Some("db_queue_full")
                );
            } else {
                assert!(answer.is_pending());
            }
            drop(waiting);
            assert_eq!(state.routine_queue.available_permits(), 56);
            assert_eq!(
                state.routine_slots.available_permits(),
                if class.is_some() { 0 } else { 3 }
            );
            assert_eq!(
                state.db_queue.available_permits(),
                if queue.is_some() { 0 } else { 64 }
            );
            drop(queue);
            drop(class);
        }
    }
    assert_eq!(
        requested(state.read(ready)).await.unwrap().unwrap()["ready"],
        1
    );
}

#[tokio::test]
async fn a_cancelled_started_poll_keeps_its_permits_until_its_write_finishes() {
    let (_root, state) = started();
    requested(state.run_bookkeeping(|db| {
        db.platform.exec(
            "CREATE TABLE cancelled_poll_probe(id INTEGER PRIMARY KEY)",
            [],
        )?;
        Ok(())
    }))
    .await
    .unwrap();
    let (began, began_work) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let caller = tokio::spawn({
        let state = state.clone();
        async move {
            requested(crate::State::polling(state.run_bookkeeping(move |db| {
                let _ = began.send(());
                let _ = wait.recv();
                db.platform
                    .exec("INSERT INTO cancelled_poll_probe VALUES(1)", [])?;
                Ok(())
            })))
            .await
        }
    });
    began_work.await.unwrap();
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    let held = (
        state.db_queue.available_permits(),
        state.db_slots.available_permits(),
        state.routine_queue.available_permits(),
        state.routine_slots.available_permits(),
    );
    release.send(()).unwrap();
    // Acquiring the entire class's execution share joins its started callback rather than
    // mistaking the dropped request future for completion.
    let joined = state
        .routine_slots
        .clone()
        .acquire_many_owned(3)
        .await
        .unwrap();
    drop(joined);
    let count = requested(state.read(|db| {
        db.platform
            .count("SELECT count(*) FROM cancelled_poll_probe", [])
    }))
    .await
    .unwrap();
    assert_eq!(held, (63, 3, 55, 2));
    assert_eq!(count, 1);
    assert_eq!(state.db_queue.available_permits(), 64);
    assert_eq!(state.db_slots.available_permits(), 4);
    assert_eq!(state.routine_queue.available_permits(), 56);
    assert_eq!(state.routine_slots.available_permits(), 3);
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
