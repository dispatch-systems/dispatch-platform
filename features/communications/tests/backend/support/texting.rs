//! The stand-in for Phones' `texting`, while `phoning` is on: one dispatch phone, Dispatch,
//! (206) 555-0100, and its texts, as a test says. What Dispatch sends from it is delivered at
//! once, or fails once a test says it does; tried again, as its condition allows in the write that
//! queues it, it waits on the phone. It keeps each picture added for a text as its adder's, and any
//! file asked of it is a photo.
use super::{Known, held, switch};
use axum::body::Body;
use dispatch_core::{
    Error, Result, State,
    db::{Store, now},
    foundation::crypto,
    manifest::{Feature, events, feature, optional},
    server::http::{input::Reply, upload::Upload},
};
use dispatch_phones_contract::{
    Added, Answer, Changes, Condition, Direction, File, Limits, Origin, Outgoing, Phone, Status,
    Text, Texting, TextsChanged,
};
use dispatch_routes_contract::WorkPhoneHold;
use std::sync::Arc;

pub(super) static PHONING: Feature = Feature {
    switch: optional("phoning", "Phoning", &[]),
    exposes: &[dispatch_core::expose!(
        dyn Texting,
        &STAND_IN,
        "texting",
        "A dispatch phone's texts, as a test says.",
    )],
    ..feature("phoning")
};
struct Phoning;
static STAND_IN: Phoning = Phoning;
/// The stand-in phone's ID.
const PHONE: &str = "phone_stand_in";
/// What one text the stand-in sends may carry.
const LIMITS: Limits = Limits {
    body: 1_000,
    files: 3,
    largest: 1024 * 1024,
};
/// The most texts the stand-in phone gives at once: a page smaller than Phones', so what reads it
/// reads on as it says.
const PAGE: usize = 2;

/// What each DSP's phone holds.
static RELAYED: Known<Relayed> = Known::new();
/// The phone's texts, the last change given, what Dispatch sent from it, and whether what it sends
/// next fails.
#[derive(Default)]
struct Relayed {
    texts: Vec<Text>,
    change: i64,
    sent: Vec<Outgoing>,
    failing: bool,
    /// The pictures added for a text not sent yet, each by its ID with who added it.
    added: Vec<(String, String)>,
    /// Who holds the work phones once the phone next goes to queue a text, to send or to send
    /// again.
    held_while_queuing: Option<Holds>,
    /// Whether Communications is switched off for the DSP while the phone next reads a file's
    /// bytes, as it next goes to queue a text, or once it has queued the next.
    off_while_reading: bool,
    off_while_queuing: bool,
    off_once_queued: bool,
}
/// Who holds a DSP's work phones, as `held` says it.
struct Holds {
    live_day: Option<String>,
    live: Vec<WorkPhoneHold>,
    saved: Vec<(String, WorkPhoneHold)>,
}

/// The DSP's phone receives `body` from `number`, written at `at`, and says so, as Phones does.
pub async fn texted(state: &Arc<State>, dsp: &str, number: &str, body: &str, at: i64) -> String {
    let id = RELAYED.of(dsp, |phone| {
        phone.change += 1;
        let text = Text {
            id: given("sms"),
            phone: PHONE.to_owned(),
            direction: Direction::In,
            origin: Origin::Phone,
            number: number.to_owned(),
            body: body.to_owned(),
            at,
            sent_by: None,
            status: None,
            status_at: None,
            error: None,
            files: Vec::new(),
            change: phone.change,
        };
        phone.texts.push(text.clone());
        text.id
    });
    events::publish(state, dsp, TextsChanged).await;
    id
}
/// The DSP's phone says how the text `id` Dispatch sent went, `status`, as Phones does.
pub async fn reported(state: &Arc<State>, dsp: &str, id: &str, status: Status) {
    RELAYED.of(dsp, |phone| {
        phone.change += 1;
        let change = phone.change;
        if let Some(text) = phone.texts.iter_mut().find(|text| text.id == id) {
            (text.status, text.status_at, text.error) = (Some(status), Some(now()), None);
            text.change = change;
        }
    });
    events::publish(state, dsp, TextsChanged).await;
}
/// Has what the DSP's phone sends next fail, or go.
pub fn failing(dsp: &str, failing: bool) {
    RELAYED.of(dsp, |phone| phone.failing = failing);
}
/// Has the DSP's work phones change hands, as `held` says, as its phone next goes to queue a text,
/// sent or sent again, before it does: as a live read or the stop may land while an answer or
/// Retry is under way.
pub fn held_while_queuing(
    dsp: &str,
    live_day: Option<&str>,
    live: Vec<WorkPhoneHold>,
    saved: Vec<(String, WorkPhoneHold)>,
) {
    let live_day = live_day.map(str::to_owned);
    RELAYED.of(dsp, |phone| {
        phone.held_while_queuing = Some(Holds {
            live_day,
            live,
            saved,
        });
    });
}
/// Has Communications switched off for the DSP as its phone next goes to queue a text, sent or
/// sent again, as the platform owner might while a member's answer is under way.
pub fn off_while_queuing(dsp: &str) {
    RELAYED.of(dsp, |phone| phone.off_while_queuing = true);
}
/// Has Communications switched off for the DSP once its phone has queued the next text it sends,
/// as the platform owner might as the answer comes back.
pub fn off_once_queued(dsp: &str) {
    RELAYED.of(dsp, |phone| phone.off_once_queued = true);
}
/// Hands the DSP's work phones over as `held_while_queuing` said, and switches Communications off
/// as `off_while_queuing` did, if they did, as its phone goes to queue a text.
async fn queuing(state: &Arc<State>, dsp: &str) {
    if let Some(holds) = RELAYED.of(dsp, |phone| phone.held_while_queuing.take()) {
        let Holds {
            live_day,
            live,
            saved,
        } = holds;
        held(state, dsp, live_day.as_deref(), live, saved).await;
    }
    if RELAYED.of(dsp, |phone| std::mem::take(&mut phone.off_while_queuing)) {
        switch(state, dsp, "communications", false).await;
    }
}
/// Has Communications switched off for the DSP while its phone next reads a file's bytes, one
/// asked of it or added for a text, as the platform owner might while a member waits on it.
pub fn off_while_reading(dsp: &str) {
    RELAYED.of(dsp, |phone| phone.off_while_reading = true);
}
/// The phone reads a file's bytes: Communications goes off for the DSP meanwhile, once a test says
/// it does.
async fn read_bytes(state: &Arc<State>, dsp: &str) {
    if RELAYED.of(dsp, |phone| std::mem::take(&mut phone.off_while_reading)) {
        switch(state, dsp, "communications", false).await;
    }
}
/// The pictures the DSP's phone keeps for a text not sent yet, by their IDs.
pub fn added(dsp: &str) -> Vec<String> {
    RELAYED.of(dsp, |phone| {
        phone.added.iter().map(|(id, _)| id.clone()).collect()
    })
}
/// What Dispatch sent from the DSP's phone, in order.
pub fn sent(dsp: &str) -> Vec<Outgoing> {
    RELAYED.of(dsp, |phone| phone.sent.clone())
}
/// An ID the stand-in gives a text or a picture: opaque, as texting's contract says, and unlike
/// any Phones gives, as another offerer's may be.
fn given(kind: &str) -> String {
    format!("{kind}-{}", crypto::hex(&crypto::random::<6>().unwrap()))
}

impl Texting for Phoning {
    fn phones(&self, _: &Store, _: &str) -> Result<Vec<Phone>> {
        Ok(vec![Phone {
            id: PHONE.to_owned(),
            name: "Dispatch".to_owned(),
            number: "+12065550100".to_owned(),
            online: true,
            last_seen_at: Some(now()),
        }])
    }
    /// A page of `PAGE` at most, as it pages.
    fn changes(&self, _: &Store, dsp: &str, after: i64) -> Result<Changes> {
        Ok(RELAYED.of(dsp, |phone| {
            let mut texts: Vec<Text> = phone
                .texts
                .iter()
                .filter(|text| text.change > after)
                .cloned()
                .collect();
            texts.sort_by_key(|text| text.change);
            let more = texts.len() > PAGE;
            texts.truncate(PAGE);
            let next = match texts.last() {
                Some(text) if more => text.change,
                _ => phone.change,
            };
            Changes { texts, next, more }
        }))
    }
    fn send<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        outgoing: Outgoing,
        condition: Condition,
    ) -> Answer<'a, Text> {
        Box::pin(async move {
            queuing(state, dsp).await;
            let at = dsp.to_owned();
            let text = state
                .run(move |db| {
                    condition(db)?;
                    RELAYED.of(&at, |phone| {
                        phone.change += 1;
                        let text = Text {
                            id: given("sms"),
                            phone: PHONE.to_owned(),
                            direction: Direction::Out,
                            origin: Origin::Dispatch,
                            number: outgoing.to.clone(),
                            body: outgoing.body.clone(),
                            at: now(),
                            sent_by: Some(outgoing.by.clone()),
                            status: Some(if phone.failing {
                                Status::Failed
                            } else {
                                Status::Delivered
                            }),
                            status_at: Some(now()),
                            error: phone.failing.then(|| "No service".to_owned()),
                            files: Vec::new(),
                            change: phone.change,
                        };
                        phone.texts.push(text.clone());
                        phone.sent.push(outgoing);
                        Ok::<_, Error>(text)
                    })
                })
                .await?;
            events::publish(state, dsp, TextsChanged).await;
            if RELAYED.of(dsp, |phone| std::mem::take(&mut phone.off_once_queued)) {
                switch(state, dsp, "communications", false).await;
            }
            Ok(text)
        })
    }
    fn retry<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
        condition: Condition,
    ) -> Answer<'a, Text> {
        Box::pin(async move {
            queuing(state, dsp).await;
            let (at, wanted) = (dsp.to_owned(), id.to_owned());
            let text = state
                .run(move |db| {
                    condition(db)?;
                    RELAYED.of(&at, |phone| {
                        phone.change += 1;
                        let change = phone.change;
                        let text = phone
                            .texts
                            .iter_mut()
                            .find(|text| text.id == wanted && text.status == Some(Status::Failed))
                            .ok_or_else(|| Error::new("phones_text_not_found", 404))?;
                        (text.status, text.error, text.change) =
                            (Some(Status::Waiting), None, change);
                        Ok(text.clone())
                    })
                })
                .await?;
            events::publish(state, dsp, TextsChanged).await;
            Ok(text)
        })
    }
    fn add_file<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        added: Added,
        bytes: Upload,
    ) -> Answer<'a, File> {
        Box::pin(async move {
            read_bytes(state, dsp).await;
            let file = File {
                id: given("mms"),
                name: added.name,
                mime: added.mime,
                size: bytes.length as i64,
                kept: true,
            };
            RELAYED.of(dsp, |phone| phone.added.push((file.id.clone(), added.by)));
            Ok(file)
        })
    }
    fn remove_file<'a>(
        &'a self,
        _: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
        by: &'a str,
    ) -> Answer<'a, ()> {
        Box::pin(async move {
            RELAYED.of(dsp, |phone| {
                phone
                    .added
                    .retain(|(added, adder)| !(added == id && adder == by));
            });
            Ok(())
        })
    }
    fn limits(&self) -> Limits {
        LIMITS
    }
    /// Any file asked of it, a photo.
    fn file<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> Answer<'a, Option<Reply>> {
        Box::pin(async move {
            read_bytes(state, dsp).await;
            let photo = Reply::download("image/jpeg", "photo.jpg", Some(3), Body::from("jpg"));
            Ok(Some(photo))
        })
    }
}
