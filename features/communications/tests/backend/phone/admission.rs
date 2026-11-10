//! Exhaust the actual admission resource, without pretending these guards are network waiters.
use super::{QUEUED, WAITING, outbox};
use crate::api::phone::access::Phone;
use crate::backend::{
    phone::{
        self,
        model::Status,
        storage::{Device, DeviceStore, FileStore, Pairing, ReportedFile, TextStore, Wanting},
        support::{outgoing, paired, reported},
    },
    support::install,
    texts::filing,
};
use axum::{body::Body, extract::ConnectInfo, http::Request, response::IntoResponse};
use dispatch_core::{
    State,
    db::{Store, now},
    foundation::crypto,
    server::http::{self, middleware},
    testing,
};
use futures_util::FutureExt;
use reqwest::{Method, RequestBuilder};
use serde_json::{Value, json};
use std::{
    future::Future,
    net::SocketAddr,
    sync::Arc,
    task::{Context, Wake, Waker},
};
use tokio::sync::{Notify, Semaphore, SemaphorePermit, oneshot};

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

/// Two DSPs, `a` and `b`, each with two phones paired: `phone_a` and `phone_c` are `a`'s,
/// `phone_b` and `phone_d` are `b`'s. Each phone calls its DSP's host with the key of its letter.
struct Fixture {
    root: tempfile::TempDir,
    state: Arc<State>,
    owner: String,
    a: String,
    b: String,
    host_a: String,
    host_b: String,
    key_a: String,
    key_b: String,
    key_c: String,
    key_d: String,
    phone_a: Device,
    phone_b: Device,
    phone_c: Device,
    phone_d: Device,
}
/// The fixture's server on a real listener, and a client of it.
struct Served {
    serving: Serving,
    client: reqwest::Client,
    origin: String,
}
impl Served {
    fn call(&self, method: Method, path: &str, host: &str, key: &str) -> RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.origin))
            .header("host", host)
            .header("origin", format!("http://{host}"))
            .bearer_auth(key)
    }
    async fn close(self) {
        drop(self.client);
        self.serving.close().await;
    }
}
/// A phone paired to `dsp` by the code `code`, with the key `key`. The shared pairing helper
/// uses one fixed code per DSP; a DSP's second phone needs its own.
fn paired_by(db: &Store, dsp: &str, owner: &str, code: &str, key: &str, number: &str) -> Device {
    let code = crypto::sha(code);
    db.phones_new_pairing(dsp, owner, &code, 1_000).unwrap();
    db.phones_pair(
        dsp,
        &code,
        &crypto::sha(key),
        &Pairing {
            number,
            model: "Pixel 8a",
            app_version: "1.0.0",
        },
        2_000,
    )
    .unwrap()
}
impl Fixture {
    async fn start() -> (Self, Served) {
        install();
        let (root, db, a) = testing::bootstrapped();
        let owner = testing::a_user(&db);
        let b = db.new_dsp("Other DSP", "UTC", &owner, false).unwrap().id;
        db.enable_all_features(&b).unwrap();
        db.set_code(&a, "aaa").unwrap();
        db.set_code(&b, "bbb").unwrap();
        let key_a = "a".repeat(64);
        let key_b = "b".repeat(64);
        let key_c = "c".repeat(64);
        let key_d = "d".repeat(64);
        let phone_a = paired(&db, &a, &crypto::sha(&key_a), "2065550100");
        let phone_b = paired(&db, &b, &crypto::sha(&key_b), "2065550177");
        let third = "idle-admission-third-phone";
        let phone_c = paired_by(&db, &a, &owner, third, &key_c, "2065550188");
        assert!(!phone_a.stand_in && !phone_b.stand_in && !phone_c.stand_in);
        let fourth = "idle-admission-fourth-phone";
        let phone_d = paired_by(&db, &b, &owner, fourth, &key_d, "2065550189");
        assert!(!phone_d.stand_in);

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
        let fixture = Self {
            root,
            state,
            owner,
            a,
            b,
            host_a,
            host_b,
            key_a,
            key_b,
            key_c,
            key_d,
            phone_a,
            phone_b,
            phone_c,
            phone_d,
        };
        let served = Served {
            serving,
            client,
            origin,
        };
        (fixture, served)
    }
}

#[tokio::test]
async fn pending_work_survives_phone_admission_exhaustion() {
    let (fixture, served) = Fixture::start().await;
    let queued = an_admitted_poll_reads_queued_work(&fixture, &served).await;
    let exhausted = Exhausted::take(&fixture);
    a_known_attempt_stays_waiting(&fixture, &served, &queued).await;
    a_busy_empty_phone_checks_in_first(&fixture, &served).await;
    malformed_and_unauthorized_polls_are_refused(&fixture, &served).await;
    an_unknown_attempt_consumes_no_work(&fixture, &served, &queued).await;
    wanted_only_work_is_handed_once(&fixture, &served).await;
    a_removed_phone_is_refused(&fixture, &served).await;
    let capacity = exhausted.release();
    served.close().await;
    tokio::time::pause();

    let input = an_unchanged_poll_idles_on_its_first_answer(&fixture, capacity).await;
    let held = a_queued_caller_expires_at_its_deadline(&fixture, &input, capacity).await;
    cancelling_returns_a_place_and_an_unobserved_grant(&fixture, &input, held).await;
    a_wake_keeps_the_queue_in_order(&fixture, &input, capacity).await;
    ready_work_skips_the_queue_and_wakes_only_its_phone(&fixture, &input, capacity).await;
    a_grant_reauthorizes_a_phone_removed_while_queued(&fixture, capacity).await;
    removal_wakes_a_registered_phone(&fixture, capacity).await;
    blocking_work_returns_its_own_permits(&fixture).await;
    forgetting_a_dsp_closes_its_phone_listeners(&fixture);
    let Fixture { root, state, .. } = fixture;
    drop(state);
    root.close().unwrap();
}

/// The text queued for B's phone: its ID, attempt and when it was queued, and the outbox's
/// answer that hands it out.
struct Queued {
    id: String,
    attempt: i64,
    at: i64,
    expected: Value,
}
async fn an_admitted_poll_reads_queued_work(fixture: &Fixture, served: &Served) -> Queued {
    let Fixture {
        state,
        owner,
        b,
        host_b,
        key_b,
        phone_b,
        ..
    } = fixture;
    // Queue via the real send/filing owner, from B, the phone that last texted the number; no
    // stand-in auto-delivery.
    let at = now();
    let (of, device, author) = (b.clone(), phone_b.id.clone(), owner.clone());
    let (id, attempt) = testing::requested(filing::changing(state, b, move |db| {
        let heard = reported("heard", "+14255550199", at, 0);
        let heard = db.phones_report(&of, &device, vec![heard])?.remove(0).id;
        let sent = phone::outbox::send(db, &of, &outgoing("4255550199", vec![], &author), at)?;
        assert_eq!(sent.phone, device);
        assert_eq!(sent.status, Some(Status::Waiting));
        let pending = db.phones_outbox(&of, &device)?;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0.id, sent.id);
        Ok(((sent.id.clone(), pending[0].1), vec![heard, sent.id]))
    }))
    .await
    .unwrap();
    phone::live::queued(state, b);
    let expected = json!({
        "texts":[{"id":id, "number":"+14255550199", "body":"On my way",
            "files":[], "attempt":attempt, "expires":at + 600_000}],
        "app":{"version":"2.0.0", "versionCode":2000}
    });
    // The admitted ready path still returns its guard, and GET does not claim the attempt.
    assert_eq!(
        answer(served.call(Method::GET, OUTBOX, host_b, key_b), 200).await,
        expected
    );
    Queued {
        id,
        attempt,
        at,
        expected,
    }
}

/// Every wait resource taken: the phones' idle permits and their queue's, and the pages' own.
struct Exhausted<'a> {
    capacity: usize,
    held: SemaphorePermit<'static>,
    queued_held: SemaphorePermit<'static>,
    ordinary: &'a Semaphore,
    ordinary_capacity: usize,
    ordinary_held: SemaphorePermit<'a>,
}
impl<'a> Exhausted<'a> {
    fn take(fixture: &'a Fixture) -> Self {
        let state = &fixture.state;
        let capacity = WAITING.available_permits();
        assert_eq!(capacity, 128);
        let held = WAITING.try_acquire_many(capacity as u32).unwrap();
        assert_eq!(QUEUED.available_permits(), 128);
        let queued_held = QUEUED.try_acquire_many(128).unwrap();
        let ordinary: &Semaphore = &state.topics.slots;
        let ordinary_capacity = ordinary.available_permits();
        let ordinary_held = ordinary.try_acquire_many(ordinary_capacity as u32).unwrap();
        assert_eq!(WAITING.available_permits(), 0);
        assert_eq!(ordinary.available_permits(), 0);
        assert_eq!(state.db_queue.available_permits(), 64);
        assert_eq!(state.db_slots.available_permits(), 4);
        Self {
            capacity,
            held,
            queued_held,
            ordinary,
            ordinary_capacity,
            ordinary_held,
        }
    }
    /// Gives each back whole, nothing having taken or leaked one meanwhile: the phones' idle
    /// permits, as many as there are.
    fn release(self) -> usize {
        assert_eq!(WAITING.available_permits(), 0);
        assert_eq!(self.ordinary.available_permits(), 0);
        drop(self.held);
        drop(self.queued_held);
        assert_eq!(QUEUED.available_permits(), 128);
        drop(self.ordinary_held);
        assert_eq!(WAITING.available_permits(), self.capacity);
        assert_eq!(self.ordinary.available_permits(), self.ordinary_capacity);
        self.capacity
    }
}
async fn a_known_attempt_stays_waiting(fixture: &Fixture, served: &Served, queued: &Queued) {
    let Fixture {
        state,
        host_b,
        key_b,
        ..
    } = fixture;
    // A real foreground DB turn succeeds with both wait resources exhausted.
    testing::requested(state.read(|_| Ok(()))).await.unwrap();
    // Ready work needs no idle permit: the same known B attempt is answered, still waiting.
    assert_eq!(
        answer(served.call(Method::GET, OUTBOX, host_b, key_b), 200).await,
        queued.expected
    );
}
async fn a_busy_empty_phone_checks_in_first(fixture: &Fixture, served: &Served) {
    let Fixture {
        state,
        a,
        host_a,
        key_a,
        phone_a,
        ..
    } = fixture;
    // Even a busy empty phone checks in and records its changed app/health first.
    let health = json!({"setup":[], "read":true, "send":true, "sim":"ready",
        "waiting":{"texts":0, "files":0, "statuses":0}});
    refused(
        served
            .call(
                Method::GET,
                &format!("{OUTBOX}?version=1.2.0&versionCode=2"),
                host_a,
                key_a,
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
}
async fn malformed_and_unauthorized_polls_are_refused(fixture: &Fixture, served: &Served) {
    let Fixture {
        host_a,
        host_b,
        key_b,
        ..
    } = fixture;
    refused(
        served.call(
            Method::GET,
            &format!("{OUTBOX}?version=1.2.0"),
            host_b,
            key_b,
        ),
        400,
        "invalid_input",
    )
    .await;
    refused(
        served
            .call(Method::GET, OUTBOX, host_b, key_b)
            .header("x-dispatch-health", "{"),
        400,
        "invalid_input",
    )
    .await;
    for request in [
        served.call(Method::GET, OUTBOX, host_b, &"0".repeat(64)),
        served.call(Method::GET, OUTBOX, host_a, key_b),
        served
            .call(Method::GET, OUTBOX, host_b, key_b)
            .header("cookie", "dispatch_session=fixture"),
    ] {
        refused(request, 401, "sign_in_required").await;
    }
}
async fn an_unknown_attempt_consumes_no_work(fixture: &Fixture, served: &Served, queued: &Queued) {
    let Fixture {
        state,
        b,
        host_b,
        key_b,
        ..
    } = fixture;
    // An unknown attempt does not consume work. Repeated GETs return the exact queued attempt.
    let report = |attempt| {
        json!({"statuses":[{
            "id":queued.id, "attempt":attempt, "status":"sent", "at":queued.at
        }]})
    };
    answer(
        served
            .call(Method::POST, STATUSES, host_b, key_b)
            .json(&report(queued.attempt + 1)),
        200,
    )
    .await;
    assert_eq!(
        answer(served.call(Method::GET, OUTBOX, host_b, key_b), 200).await,
        queued.expected
    );
    answer(
        served
            .call(Method::POST, STATUSES, host_b, key_b)
            .json(&report(queued.attempt)),
        200,
    )
    .await;
    let (of, text) = (b.clone(), queued.id.clone());
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
        served.call(Method::GET, OUTBOX, host_b, key_b),
        503,
        "platform_busy",
    )
    .await;
}
async fn wanted_only_work_is_handed_once(fixture: &Fixture, served: &Served) {
    let Fixture {
        state,
        b,
        host_b,
        key_b,
        phone_b,
        ..
    } = fixture;
    // Wanted-only work uses the same response and handed bookkeeping, once per ask.
    let (of, device) = (b.clone(), phone_b.id.clone());
    let video = testing::requested(filing::changing(state, b, move |db| {
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
    phone::live::queued_for(state, b, Some(&phone_b.id));
    assert_eq!(
        answer(served.call(Method::GET, OUTBOX, host_b, key_b), 200).await,
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
        served.call(Method::GET, OUTBOX, host_b, key_b),
        503,
        "platform_busy",
    )
    .await;
}
async fn a_removed_phone_is_refused(fixture: &Fixture, served: &Served) {
    let Fixture {
        state,
        a,
        host_a,
        key_a,
        phone_a,
        ..
    } = fixture;
    let (of, device) = (a.clone(), phone_a.id.clone());
    testing::requested(filing::changing(state, a, move |db| {
        let (_, failed) = db.phones_remove(&of, &device, now())?;
        Ok(((), failed))
    }))
    .await
    .unwrap();
    refused(
        served.call(Method::GET, OUTBOX, host_a, key_a),
        401,
        "phones_removed",
    )
    .await;
}

/// B's phone's wait, as the handler reads it, for the scenarios that poll the handler itself.
async fn an_unchanged_poll_idles_on_its_first_answer(
    fixture: &Fixture,
    capacity: usize,
) -> http::input::Input {
    let Fixture {
        state,
        b,
        host_b,
        key_b,
        phone_b,
        ..
    } = fixture;
    // Use the public parser, then a real request trace, when polling the actual handler.
    // Make presence current: this poll needs neither bookkeeping nor metadata writes.
    let (of, device) = (b.clone(), phone_b.id.clone());
    testing::requested(state.run_bookkeeping(move |db| db.phones_seen(&of, &device, now())))
        .await
        .unwrap();
    let input = outbox_input(state, host_b, key_b);
    let mut waiting = Box::pin(testing::requested(outbox(
        state.clone(),
        input.clone(),
        Phone,
    )));
    let wake = Arc::new(Awoken(Notify::new()));
    let waker = Waker::from(wake.clone());
    let mut cx = Context::from_waker(&waker);
    {
        // Let the handler submit its first real DB turn, but hold its read until this poll ends.
        let transition = state.transition.write().unwrap();
        assert!(waiting.as_mut().poll(&mut cx).is_pending());
        assert_eq!(state.db_queue.available_permits(), 63);
        drop(transition);
    }
    // Taking every permit waits for that turn to finish, without resuming the handler. An
    // unchanged poll must be able to enter its idle wait using that answer alone, even when
    // another request occupies the queue. A second database turn, to check in or to probe
    // again, would fail db_queue_full here.
    let queued = state.db_queue.acquire_many(64).await.unwrap();
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
    drop(queued);
    input
}
/// Leaves every idle permit held, for the cancellations that follow.
async fn a_queued_caller_expires_at_its_deadline(
    fixture: &Fixture,
    input: &http::input::Input,
    capacity: usize,
) -> SemaphorePermit<'static> {
    let state = &fixture.state;
    // A bounded queued caller expires at the original deadline without an empty success.
    let held = WAITING.try_acquire_many(capacity as u32).unwrap();
    let mut expired = Waiting::new(state.clone(), input.clone());
    expired.drive_to(127).await;
    tokio::time::advance(super::WAIT).await;
    let Err(error) = expired.call.await else {
        panic!("timeout without an idle permit returned an empty success");
    };
    assert_eq!(
        error.admission_origin(),
        Some("phone_outbox_idle_unavailable")
    );
    assert_eq!(QUEUED.available_permits(), 128);
    assert_eq!(WAITING.available_permits(), 0);
    held
}
async fn cancelling_returns_a_place_and_an_unobserved_grant(
    fixture: &Fixture,
    input: &http::input::Input,
    mut held: SemaphorePermit<'static>,
) {
    let state = &fixture.state;
    // Cancelling the FIFO head removes it; cancellation after assignment but before the
    // handler observes its grant also returns the grant to the semaphore.
    let mut cancelled = Waiting::new(state.clone(), input.clone());
    cancelled.drive_to(127).await;
    let mut successor = Waiting::new(state.clone(), input.clone());
    successor.drive_to(126).await;
    drop(cancelled);
    assert_eq!(QUEUED.available_permits(), 127);
    drop(held.split(1).unwrap());
    assert_eq!(WAITING.available_permits(), 0);
    drop(successor);
    assert_eq!(WAITING.available_permits(), 1);
    assert_eq!(QUEUED.available_permits(), 128);
    drop(held);
}
async fn a_wake_keeps_the_queue_in_order(
    fixture: &Fixture,
    input: &http::input::Input,
    capacity: usize,
) {
    let Fixture { state, b, .. } = fixture;
    let mut held = WAITING.try_acquire_many(capacity as u32).unwrap();
    let mut first = Waiting::new(state.clone(), input.clone());
    first.drive_to(127).await;
    let mut second = Waiting::new(state.clone(), input.clone());
    second.drive_to(126).await;
    // An unrelated same-DSP wake starts the head's fresh probe. Its pending acquisition
    // must retain its place even while the database answer has not been observed.
    phone::live::queued(state, b);
    assert!(first.poll().is_pending());
    let mut third = Waiting::new(state.clone(), input.clone());
    third.drive_to(125).await;
    let mut tail = Waiting::new(state.clone(), input.clone());
    tail.drive_to(124).await;
    drop(tail);
    assert_eq!(QUEUED.available_permits(), 125);
    drop(held.split(1).unwrap());
    assert!(second.poll().is_pending());
    assert_eq!(
        QUEUED.available_permits(),
        125,
        "a later caller received the FIFO head's grant after a wake"
    );
    first.drive_to(126).await;
    drop(first);
    assert!(third.poll().is_pending());
    assert_eq!(QUEUED.available_permits(), 126);
    second.drive_to(127).await;
    drop(second);
    third.drive_to(128).await;
    drop(third);
    assert_eq!(WAITING.available_permits(), 1);
    drop(held);
    assert_eq!(WAITING.available_permits(), capacity);
}
async fn ready_work_skips_the_queue_and_wakes_only_its_phone(
    fixture: &Fixture,
    input: &http::input::Input,
    capacity: usize,
) {
    let Fixture {
        state,
        owner,
        b,
        host_b,
        key_d,
        phone_b,
        ..
    } = fixture;
    // Ready work arriving after registration does not wait for an idle permit. Another real
    // phone in the same DSP must receive no dependency wake or perform another probe for it.
    let held = WAITING.try_acquire_many(capacity as u32).unwrap();
    let mut quiet = Waiting::new(state.clone(), outbox_input(state, host_b, key_d));
    quiet.drive_to(127).await;
    // Notify coalesces to one permit; consume the completed initial DB dependency's wake.
    let _ = quiet.wake.0.notified().now_or_never();
    let mut ready = Waiting::new(state.clone(), input.clone());
    ready.drive_to(126).await;
    // B's phone texted the number last, so the text goes from it.
    let later = now();
    let (of, device, author) = (b.clone(), phone_b.id.clone(), owner.clone());
    let (late_id, late_attempt) = testing::requested(filing::changing(state, b, move |db| {
        let text = phone::outbox::send(db, &of, &outgoing("4255550199", vec![], &author), later)?;
        let pending = db.phones_outbox(&of, &device)?;
        let attempt = pending.iter().find(|(row, _)| row.id == text.id).unwrap().1;
        Ok(((text.id.clone(), attempt), vec![text.id]))
    }))
    .await
    .unwrap();
    phone::live::queued_for(state, b, Some(&phone_b.id));
    assert!(quiet.wake.0.notified().now_or_never().is_none());
    let response = ready.call.await.unwrap().into_response();
    let body = axum::body::to_bytes(response.into_body(), 65_536)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({
            "texts":[{"id":late_id, "number":"+14255550199", "body":"On my way",
                "files":[], "attempt":late_attempt, "expires":later + 600_000}],
            "app":{"version":"2.0.0", "versionCode":2000}
        })
    );
    assert_eq!(WAITING.available_permits(), 0);
    assert_eq!(QUEUED.available_permits(), 127);
    assert!(quiet.wake.0.notified().now_or_never().is_none());
    // Unknown-device fallback still wakes this exact same idle handler.
    phone::live::queued_for(state, b, None);
    quiet.wake.0.notified().await;
    drop(quiet);
    assert_eq!(QUEUED.available_permits(), 128);
    drop(held);
}
async fn a_grant_reauthorizes_a_phone_removed_while_queued(fixture: &Fixture, capacity: usize) {
    let Fixture {
        state,
        a,
        host_a,
        key_c,
        phone_c,
        ..
    } = fixture;
    // A grant cannot turn authorization captured before the queue into an empty heartbeat.
    // Removal intentionally sends no phone-topic hint: the grant itself must reauthorize.
    let mut held = WAITING.try_acquire_many(capacity as u32).unwrap();
    let input_c = outbox_input(state, host_a, key_c);
    let mut removed = Waiting::new(state.clone(), input_c);
    removed.drive_to(127).await;
    let (of, device) = (a.clone(), phone_c.id.clone());
    testing::requested(filing::changing(state, a, move |db| {
        let (_, failed) = db.phones_remove(&of, &device, now())?;
        Ok(((), failed))
    }))
    .await
    .unwrap();
    drop(held.split(1).unwrap());
    let Err(error) = removed.call.await else {
        panic!("a removed phone was authorized by its former queue position");
    };
    assert_eq!(error.code, "phones_removed");
    assert_eq!(WAITING.available_permits(), 1);
    assert_eq!(QUEUED.available_permits(), 128);
    drop(held);
    assert_eq!(WAITING.available_permits(), capacity);
}
async fn removal_wakes_a_registered_phone(fixture: &Fixture, capacity: usize) {
    let Fixture {
        state,
        b,
        host_b,
        key_d,
        phone_d,
        ..
    } = fixture;
    // Broad removal wakes a registered phone before its heartbeat or idle admission.
    let held = WAITING.try_acquire_many(capacity as u32).unwrap();
    let mut removed = Waiting::new(state.clone(), outbox_input(state, host_b, key_d));
    removed.drive_to(127).await;
    let (of, device) = (b.clone(), phone_d.id.clone());
    testing::requested(filing::changing(state, b, move |db| {
        let (_, failed) = db.phones_remove(&of, &device, now())?;
        Ok(((), failed))
    }))
    .await
    .unwrap();
    phone::live::queued(state, b);
    let Err(error) = removed.call.await else {
        panic!("removed phone did not reauthorize after its lifecycle wake");
    };
    assert_eq!(error.code, "phones_removed");
    assert_eq!(QUEUED.available_permits(), 128);
    assert_eq!(WAITING.available_permits(), 0);
    drop(held);
}
async fn blocking_work_returns_its_own_permits(fixture: &Fixture) {
    let state = &fixture.state;
    // Running blocking work retains its own permits; wait for that work to release them.
    let queued = state.db_queue.acquire_many(64).await.unwrap();
    let slots = state.db_slots.acquire_many(4).await.unwrap();
    drop(slots);
    drop(queued);
    testing::requested(state.read(|_| Ok(()))).await.unwrap();
    assert_eq!(state.db_queue.available_permits(), 64);
    assert_eq!(state.db_slots.available_permits(), 4);
}
fn forgetting_a_dsp_closes_its_phone_listeners(fixture: &Fixture) {
    let Fixture {
        state, b, phone_b, ..
    } = fixture;
    // State's DSP deletion hook closes phone listeners. The facade rechecks once and then
    // stays pending, so callers that ignore changed()'s value cannot spin on closure.
    let mut closing = phone::live::listen(state, b, &phone_b.id);
    state.forget_dsp(b);
    assert!(closing.changed().now_or_never().is_some());
    assert!(closing.changed().now_or_never().is_none());
    drop(closing);
}

/// Woken by the handler's real DB/watch dependencies, including a wake during its poll.
struct Awoken(Notify);
impl Wake for Awoken {
    fn wake(self: Arc<Self>) {
        self.0.notify_one();
    }
}

/// A real handler driven only by its DB/watch dependencies, never a periodic sampler.
struct Waiting {
    call: std::pin::Pin<Box<dyn Future<Output = dispatch_core::Result<http::input::Reply>> + Send>>,
    wake: Arc<Awoken>,
}
impl Waiting {
    fn new(state: Arc<State>, input: http::input::Input) -> Self {
        Self {
            call: Box::pin(testing::requested(outbox(state, input, Phone))),
            wake: Arc::new(Awoken(Notify::new())),
        }
    }
    fn poll(&mut self) -> std::task::Poll<dispatch_core::Result<http::input::Reply>> {
        let waker = Waker::from(self.wake.clone());
        self.call.as_mut().poll(&mut Context::from_waker(&waker))
    }
    async fn drive_to(&mut self, queue_available: usize) {
        loop {
            match self.poll() {
                std::task::Poll::Pending => {}
                std::task::Poll::Ready(Err(error)) => panic!("outbox left admission: {error:?}"),
                std::task::Poll::Ready(Ok(_)) => panic!("empty outbox completed before admission"),
            }
            if QUEUED.available_permits() == queue_available {
                return;
            }
            self.wake.0.notified().await;
        }
    }
}
fn outbox_input(state: &State, host: &str, key: &str) -> http::input::Input {
    let request = Request::builder()
        .uri(OUTBOX)
        .header("host", host)
        .header("authorization", format!("Bearer {key}"))
        .extension(state.config.site(host).unwrap())
        .extension(ConnectInfo(
            "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
        ))
        .body(Body::empty())
        .unwrap();
    let (parts, _) = request.into_parts();
    middleware::head(state, &parts, OUTBOX).unwrap()
}
