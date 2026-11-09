//! Exhaust the actual admission resource, without pretending these guards are network waiters.
use super::{WAITING, outbox};
use crate::api::phone::access::Phone;
use crate::backend::{
    phone::{
        self,
        model::Status,
        storage::{DeviceStore, FileStore, ReportedFile, TextStore, Wanting},
        support::{outgoing, paired, reported},
    },
    support::install,
    texts::filing,
};
use axum::{body::Body, extract::ConnectInfo, http::Request};
use dispatch_core::{
    State,
    db::now,
    foundation::crypto,
    server::http::{self, middleware},
    testing,
};
use reqwest::{Method, RequestBuilder};
use serde_json::{Value, json};
use std::{
    future::Future,
    net::SocketAddr,
    sync::Arc,
    task::{Context, Wake, Waker},
};
use tokio::sync::{Notify, oneshot};

const OUTBOX: &str = "/api/communications/phone/outbox";
const STATUSES: &str = "/api/communications/phone/statuses";

/// Own the listener on success and on assertion failure. Normal cleanup joins it.
struct Serving {
    stop: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}
impl Serving {
    async fn close(mut self) {
        self.stop.take().unwrap().send(()).unwrap();
        (&mut self.task).await.unwrap();
    }
}
impl Drop for Serving {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn answer(request: RequestBuilder, status: u16) -> Value {
    let response = request.send().await.unwrap();
    assert!(response.headers().contains_key("x-request-id"));
    let actual = response.status().as_u16();
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(actual, status, "{body}");
    body
}
async fn refused(request: RequestBuilder, status: u16, code: &str) {
    let body = answer(request, status).await;
    assert_eq!(body["error"], code, "{body}");
    assert!(body["texts"].is_null() && body["wanted"].is_null());
}

#[tokio::test]
async fn pending_work_survives_phone_admission_exhaustion() {
    install();
    let (root, db, a) = testing::bootstrapped();
    let owner = testing::a_user(&db);
    let b = db.new_dsp("Other DSP", "UTC", &owner, false).unwrap().id;
    db.enable_all_features(&b).unwrap();
    db.set_code(&a, "aaa").unwrap();
    db.set_code(&b, "bbb").unwrap();
    let key_a = "a".repeat(64);
    let key_b = "b".repeat(64);
    let phone_a = paired(&db, &a, &crypto::sha(&key_a), "2065550100");
    let phone_b = paired(&db, &b, &crypto::sha(&key_b), "2065550177");
    assert!(!phone_a.stand_in && !phone_b.stand_in);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let origin = format!("http://127.0.0.1:{port}");
    let host_a = format!("aaa.localhost:{port}");
    let host_b = format!("bbb.localhost:{port}");
    let mut config = db.config.clone();
    config.port = port;
    config.origin = origin.clone();
    config.invite_origin = format!("http://invite.localhost:{port}");
    config.dsp_origin = format!("http://{{code}}.localhost:{port}");
    config.dashboard = root.path().join("dashboard");
    let relay = config.dashboard.join("relay");
    std::fs::create_dir_all(&relay).unwrap();
    std::fs::write(relay.join("relay-dev.apk"), b"dev apk").unwrap();
    std::fs::write(
        relay.join("relay.json"),
        json!({"dev":{
            "version":"2.0.0", "versionCode":2000, "asset":"relay-dev.apk",
            "sha256":crypto::sha(b"dev apk"), "size":7
        }})
        .to_string(),
    )
    .unwrap();
    drop(db);
    let state = State::new(config).unwrap();
    let app = http::router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
    let (stop, stopped) = oneshot::channel();
    let serving = Serving {
        stop: Some(stop),
        task: tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
        }),
    };
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let call = |method: Method, path: &str, host: &str, key: &str| {
        client
            .request(method, format!("{origin}{path}"))
            .header("host", host)
            .header("origin", format!("http://{host}"))
            .bearer_auth(key)
    };

    // Queue via the real send/filing owner, explicitly from B; no stand-in auto-delivery.
    let at = now();
    let (of, device) = (b.clone(), phone_b.id.clone());
    let (id, attempt) = testing::requested(filing::changing(&state, &b, move |db| {
        let mut outgoing = outgoing("4255550199", vec![], &owner);
        outgoing.phone = Some(device.clone());
        let sent = phone::outbox::send(db, &of, &outgoing, at)?;
        assert_eq!(sent.phone, device);
        assert_eq!(sent.status, Some(Status::Waiting));
        let pending = db.phones_outbox(&of, &device)?;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0.id, sent.id);
        Ok(((sent.id.clone(), pending[0].1), vec![sent.id]))
    }))
    .await
    .unwrap();
    phone::live::queued(&state, &b);
    let expected = json!({
        "texts":[{"id":id, "number":"+14255550199", "body":"On my way",
            "files":[], "attempt":attempt, "expires":at + 600_000}],
        "app":{"version":"2.0.0", "versionCode":2000}
    });
    // The admitted ready path still returns its guard, and GET does not claim the attempt.
    assert_eq!(
        answer(call(Method::GET, OUTBOX, &host_b, &key_b), 200).await,
        expected
    );
    let capacity = WAITING.available_permits();
    assert_eq!(capacity, 128);
    let held = WAITING.try_acquire_many(capacity as u32).unwrap();
    let ordinary = state.topics.slots.clone();
    let ordinary_capacity = ordinary.available_permits();
    let ordinary_held = ordinary.try_acquire_many(ordinary_capacity as u32).unwrap();
    assert_eq!(WAITING.available_permits(), 0);
    assert_eq!(ordinary.available_permits(), 0);
    assert_eq!(state.db_queue.available_permits(), 64);
    assert_eq!(state.db_slots.available_permits(), 4);
    // A real foreground DB turn succeeds with both wait resources exhausted.
    testing::requested(state.read(|_| Ok(()))).await.unwrap();
    // On the pin this answers 503, while the same known B attempt remains waiting.
    assert_eq!(
        answer(call(Method::GET, OUTBOX, &host_b, &key_b), 200).await,
        expected
    );

    // Even a busy empty phone checks in and records its changed app/health first.
    let health = json!({"setup":[], "read":true, "send":true, "sim":"ready",
        "waiting":{"texts":0, "files":0, "statuses":0}});
    refused(
        call(
            Method::GET,
            &format!("{OUTBOX}?version=1.2.0&versionCode=2"),
            &host_a,
            &key_a,
        )
        .header("x-dispatch-health", health.to_string()),
        503,
        "platform_busy",
    )
    .await;
    let (of, device) = (a.clone(), phone_a.id.clone());
    testing::requested(state.read(move |db| {
        let device = db.phones_device(&of, &device)?.unwrap();
        assert!(device.last_seen_at.unwrap() > 2_000);
        assert_eq!(device.app_version, "1.2.0");
        assert_eq!(device.app_version_code, Some(2));
        assert!(device.health.is_some());
        Ok(())
    }))
    .await
    .unwrap();
    refused(
        call(
            Method::GET,
            &format!("{OUTBOX}?version=1.2.0"),
            &host_b,
            &key_b,
        ),
        400,
        "invalid_input",
    )
    .await;
    refused(
        call(Method::GET, OUTBOX, &host_b, &key_b).header("x-dispatch-health", "{"),
        400,
        "invalid_input",
    )
    .await;
    for request in [
        call(Method::GET, OUTBOX, &host_b, &"0".repeat(64)),
        call(Method::GET, OUTBOX, &host_a, &key_b),
        call(Method::GET, OUTBOX, &host_b, &key_b).header("cookie", "dispatch_session=fixture"),
    ] {
        refused(request, 401, "sign_in_required").await;
    }

    // An unknown attempt does not consume work. Repeated GETs return the exact queued attempt.
    let report = |attempt| {
        json!({"statuses":[{
            "id":id, "attempt":attempt, "status":"sent", "at":at
        }]})
    };
    answer(
        call(Method::POST, STATUSES, &host_b, &key_b).json(&report(attempt + 1)),
        200,
    )
    .await;
    assert_eq!(
        answer(call(Method::GET, OUTBOX, &host_b, &key_b), 200).await,
        expected
    );
    answer(
        call(Method::POST, STATUSES, &host_b, &key_b).json(&report(attempt)),
        200,
    )
    .await;
    let (of, text) = (b.clone(), id.clone());
    testing::requested(state.read(move |db| {
        assert_eq!(
            db.phones_text(&of, &text)?.unwrap().status,
            Some(Status::Sent)
        );
        Ok(())
    }))
    .await
    .unwrap();
    refused(
        call(Method::GET, OUTBOX, &host_b, &key_b),
        503,
        "platform_busy",
    )
    .await;

    // Wanted-only work uses the same response and handed bookkeeping, once per ask.
    let (of, device) = (b.clone(), phone_b.id.clone());
    let video = testing::requested(filing::changing(&state, &b, move |db| {
        let mut text = reported("admission-video", "4255550199", now(), 1);
        text.files[0] = ReportedFile {
            name: "door.mp4".into(),
            mime: "video/mp4".into(),
            size: 7,
            on_phone: true,
        };
        let kept = db.phones_report(&of, &device, vec![text])?;
        let video = kept[0].files[0].clone();
        db.phones_seen(&of, &device, now())?;
        assert_eq!(
            db.phones_want_file(&of, &video, now())?,
            Some(Wanting::Asked)
        );
        Ok((video, vec![kept[0].id.clone()]))
    }))
    .await
    .unwrap();
    phone::live::queued(&state, &b);
    assert_eq!(
        answer(call(Method::GET, OUTBOX, &host_b, &key_b), 200).await,
        json!({
            "texts":[], "wanted":[video], "app":{"version":"2.0.0", "versionCode":2000}
        })
    );
    let (of, device) = (b.clone(), phone_b.id.clone());
    testing::requested(state.read(move |db| {
        assert!(db.phones_wanted(&of, &device, 0)?.is_empty());
        Ok(())
    }))
    .await
    .unwrap();
    refused(
        call(Method::GET, OUTBOX, &host_b, &key_b),
        503,
        "platform_busy",
    )
    .await;

    let (of, device) = (a.clone(), phone_a.id.clone());
    testing::requested(filing::changing(&state, &a, move |db| {
        let (_, failed) = db.phones_remove(&of, &device, now())?;
        Ok(((), failed))
    }))
    .await
    .unwrap();
    refused(
        call(Method::GET, OUTBOX, &host_a, &key_a),
        401,
        "phones_removed",
    )
    .await;
    assert_eq!(WAITING.available_permits(), 0);
    assert_eq!(ordinary.available_permits(), 0);
    drop(held);
    drop(ordinary_held);
    assert_eq!(WAITING.available_permits(), capacity);
    assert_eq!(ordinary.available_permits(), ordinary_capacity);
    drop(client);
    serving.close().await;
    tokio::time::pause();

    // Use the public parser, then a real request trace, when polling the actual handler.
    let request = Request::builder()
        .uri(OUTBOX)
        .header("host", &host_b)
        .header("authorization", format!("Bearer {key_b}"))
        .extension(state.config.site(&host_b).unwrap())
        .extension(ConnectInfo(
            "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
        ))
        .body(Body::empty())
        .unwrap();
    let (parts, _) = request.into_parts();
    let input = middleware::head(&state, &parts, OUTBOX).unwrap();
    let mut waiting = Box::pin(testing::requested(outbox(state.clone(), input, Phone)));
    let wake = Arc::new(Awoken(Notify::new()));
    let waker = Waker::from(wake.clone());
    let mut cx = Context::from_waker(&waker);
    loop {
        assert!(
            waiting.as_mut().poll(&mut cx).is_pending(),
            "empty admitted outbox must wait"
        );
        if WAITING.available_permits() == capacity - 1 {
            break;
        }
        // Only an actual dependency wake permits another handler poll; no readiness sampler.
        wake.0.notified().await;
    }
    drop(waiting);
    assert_eq!(WAITING.available_permits(), capacity);
    // Running blocking work retains its own permits; wait for that work to release them.
    let queued = state.db_queue.acquire_many(64).await.unwrap();
    let slots = state.db_slots.acquire_many(4).await.unwrap();
    drop(slots);
    drop(queued);
    testing::requested(state.read(|_| Ok(()))).await.unwrap();
    assert_eq!(state.db_queue.available_permits(), 64);
    assert_eq!(state.db_slots.available_permits(), 4);
    drop(state);
    // Inventory the owned fixture root before its explicit cleanup.
    let contents = std::fs::read_dir(root.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    eprintln!("removing fixture {}: {contents:?}", root.path().display());
    root.close().unwrap();
}

/// Woken by the handler's real DB/watch dependencies, including a wake during its poll.
struct Awoken(Notify);
impl Wake for Awoken {
    fn wake(self: Arc<Self>) {
        self.0.notify_one();
    }
}
