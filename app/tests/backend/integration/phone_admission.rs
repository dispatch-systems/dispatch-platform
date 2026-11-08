//! Phone work must remain reachable while ordinary page and video waits fill their budget.
use super::support::{Call, OWNER, Server};
use dispatch_core::db::now;
use serde_json::json;

#[tokio::test]
async fn ordinary_wait_saturation_does_not_block_an_authorized_phones_outbox() {
    let server = Server::start().await;
    let owner = server.member(OWNER).await;
    let host = server.host("nll");
    let pairing = server
        .send(Call::post("/api/dsp/communications/phones/pairing", json!({})).who(&owner))
        .await;
    assert_eq!(pairing.status, 200, "{}", pairing.body);
    let paired = server.send(
        Call::post("/api/communications/phone/pair", json!({
            "code": pairing.body["code"], "number":"2065550177", "model":"Fixture phone", "version":"1.0.0"
        })).without_origin().header("host", &host)
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
                .header("authorization", &format!("Bearer {}", "0".repeat(64))),
        )
        .await;
    assert_eq!(refused.status, 401, "{}", refused.body);
    let work = server
        .send(
            Call::get(outbox)
                .without_origin()
                .header("host", &host)
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
}
