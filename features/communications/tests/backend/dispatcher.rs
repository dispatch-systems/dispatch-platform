//! A conversation of either kind, through the dispatcher: an answer that went stays kept though its
//! writer's access ended as it went, and is read back to them only while they may read it.
use super::*;
use crate::backend::{
    support::{self, filed, texted},
    texts::storage::TextStore,
};
use dispatch_core::db::now;

#[tokio::test]
async fn an_answer_that_went_stays_kept_though_its_writers_access_ended_as_it_went() {
    let (_root, state, dsp) = support::started();
    let asked = texted(&state, &dsp, "+12535550177", "Can I swap Saturday?", now()).await;
    let conversation = filed(&state, &dsp, &asked).await.id;
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    // Communications goes off for the DSP once the phone has queued it, and with it their
    // permission: it went and is kept, but nothing of the conversation is read back to them.
    support::off_once_queued(&dsp);
    let reference = Reference::Text(conversation.clone());
    let refused = send(&state, &c, support::USE, &reference, "Swap it", &[]).await;
    assert_eq!(refused.unwrap_err().code, "permission_denied");
    let at = dsp.clone();
    let kept = state
        .read(move |db| db.communications_texts(&at, &conversation, None, 10))
        .await
        .unwrap();
    let bodies: Vec<&str> = kept.iter().map(|text| text.body.as_str()).collect();
    assert_eq!(bodies, ["Can I swap Saturday?", "Swap it"]);
}
