//! Phone work must remain reachable while ordinary page and video waits fill their budget.
use super::support::{Call, OWNER, Server};
use dispatch_core::db::now;
use dispatch_core::{State, testing::requested};
use serde_json::json;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, mpsc},
};

type Pending = Pin<Box<dyn Future<Output = dispatch_core::Result<()>> + Send>>;

/// Actual routine database calls, held by channels until this test releases them.
/// Dropping on a failed assertion still releases every started blocking callback.
struct Polling {
    release: Option<mpsc::Sender<()>>,
    calls: Vec<Pending>,
}
impl Polling {
    async fn hold(state: &Arc<State>) -> Self {
        let (release, wait) = mpsc::channel();
        let wait = Arc::new(Mutex::new(wait));
        let (entered, mut starts) = tokio::sync::mpsc::unbounded_channel();
        let mut held = Self {
            release: Some(release),
            calls: Vec::new(),
        };
        for _ in 0..56 {
            let (state, wait, entered) = (state.clone(), wait.clone(), entered.clone());
            let mut call: Pending = Box::pin(async move {
                requested(State::patiently(State::polling(state.read(move |_| {
                    let _ = entered.send(());
                    let _ = wait.lock().unwrap().recv();
                    Ok(())
                }))))
                .await
            });
            let pending = std::future::poll_fn(|cx| {
                std::task::Poll::Ready(call.as_mut().poll(cx).is_pending())
            })
            .await;
            held.calls.push(call);
            assert!(pending);
        }
        for _ in 0..3 {
            starts.recv().await.unwrap();
        }
        held
    }
    fn release(&mut self) {
        if let Some(release) = self.release.take() {
            for _ in 0..56 {
                let _ = release.send(());
            }
        }
    }
    async fn close(mut self) {
        self.release();
        for call in std::mem::take(&mut self.calls) {
            call.await.unwrap();
        }
    }
}
impl Drop for Polling {
    fn drop(&mut self) {
        self.release();
    }
}

#[tokio::test]
async fn ordinary_wait_saturation_does_not_block_an_authorized_phones_outbox() {
    let server = Server::start().await;
    let owner = server.member(OWNER).await;
    let host = server.host("nll");
    let origin = format!("http://{host}");
    let pairing = server
        .send(Call::post("/api/dsp/communications/phones/pairing", json!({})).who(&owner))
        .await;
    assert_eq!(pairing.status, 200, "{}", pairing.body);
    let paired = server.send(
        Call::post("/api/communications/phone/pair", json!({
            "code": pairing.body["code"], "number":"2065550177", "model":"Fixture phone", "version":"1.0.0"
        })).without_origin().header("host", &host).header("origin", &origin)
    ).await;
    assert_eq!(paired.status, 200, "{}", paired.body);
    let key = format!("Bearer {}", paired.body["key"].as_str().unwrap());
    let reported = server
        .send(
            Call::post(
                "/api/communications/phone/texts",
                json!({"texts":[{
                    "clientId":"admission-message", "direction":"in", "number":"4255550142",
                    "body":"Ready for work", "at":now()
                }]}),
            )
            .without_origin()
            .header("host", &host)
            .header("origin", &origin)
            .header("authorization", &key),
        )
        .await;
    assert_eq!(reported.status, 200, "{}", reported.body);
    let conversation = server
        .send(
            Call::post(
                "/api/dsp/communications/texts/conversations",
                json!({"number":"4255550142"}),
            )
            .who(&owner),
        )
        .await;
    assert_eq!(conversation.status, 200, "{}", conversation.body);
    let id = conversation.body["id"].as_str().unwrap();
    let queued = server
        .send(
            Call::post(
                &format!("/api/dsp/communications/conversations/{id}/messages"),
                json!({"text":"Collect this queued work"}),
            )
            .who(&owner),
        )
        .await;
    assert_eq!(queued.status, 201, "{}", queued.body);

    let ordinary = server.state.topics.slots.clone();
    let capacity = ordinary.available_permits();
    assert!(capacity > 0);
    let _held = ordinary
        .clone()
        .acquire_many_owned(capacity as u32)
        .await
        .unwrap();
    assert_eq!(ordinary.available_permits(), 0);
    let outbox = "/api/communications/phone/outbox";
    let refused = server
        .send(
            Call::get(outbox)
                .without_origin()
                .header("host", &host)
                .header("origin", &origin)
                .header("authorization", &format!("Bearer {}", "0".repeat(64))),
        )
        .await;
    assert_eq!(refused.status, 401, "{}", refused.body);
    let work = server
        .send(
            Call::get(outbox)
                .without_origin()
                .header("host", &host)
                .header("origin", &origin)
                .header("authorization", &key),
        )
        .await;
    assert_eq!(work.status, 200, "{}", work.body);
    assert!(
        work.body["texts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|text| text["body"] == "Collect this queued work")
    );
    assert_eq!(
        ordinary.available_permits(),
        0,
        "phone work did not borrow ordinary admission"
    );

    // Saturate actual routine work, independently of the idle-wait budget above.
    // A member's real People read retains its execution opportunity; another outbox
    // poll uses the bounded routine share even though it already has ready work.
    let polling = Polling::hold(&server.state).await;
    let people = server.send(Call::get("/api/dsp/people").who(&owner)).await;
    let busy = server
        .send(
            Call::get(outbox)
                .without_origin()
                .header("host", &host)
                .header("origin", &origin)
                .header("authorization", &key),
        )
        .await;
    polling.close().await;
    assert_eq!(people.status, 200, "{}", people.body);
    assert_eq!(busy.status, 503, "{}", busy.body);
    assert_eq!(busy.error(), "platform_busy");
    assert!(busy.body["texts"].is_null());
    let work = server
        .send(
            Call::get(outbox)
                .without_origin()
                .header("host", &host)
                .header("origin", &origin)
                .header("authorization", &key),
        )
        .await;
    assert_eq!(work.status, 200, "{}", work.body);
    assert!(
        work.body["texts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|text| text["body"] == "Collect this queued work")
    );
}
