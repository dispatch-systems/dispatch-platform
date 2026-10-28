//! The DSP's dispatch phone, as a test says: Dispatch, (206) 555-0100, paired to the DSP each test
//! starts with. It receives a text and says how one Dispatch sent went, as its app does through
//! the phones' address, each kept by the phone side and filed in the same write; and what Dispatch
//! queued for it is read from its outbox, as its app reads it.
use crate::backend::{
    phone::{
        model::{Direction, Status},
        storage::{DeviceStore, Pairing, Reported, StatusReport, TextStore},
    },
    texts::filing,
};
use dispatch_core::{
    Error, Result, State,
    db::{Store, now},
    foundation::crypto,
    testing,
};
use std::sync::Arc;

/// The DSP's phone's own number.
pub const NUMBER: &str = "+12065550100";

/// Pairs the DSP's phone, as its app does with a code the DSP's owner made.
pub(super) fn pair(db: &Store, dsp: &str) {
    let code = crypto::sha("phone-code");
    db.phones_new_pairing(dsp, &testing::a_user(db), &code, now())
        .unwrap();
    let pairing = Pairing {
        number: NUMBER,
        model: "Pixel 8a",
        app_version: "1.0.0",
    };
    let key = crypto::sha(crypto::random::<32>().unwrap());
    db.phones_pair(dsp, &code, &key, &pairing, now()).unwrap();
}
/// The DSP's phone, by its ID.
fn phone(db: &Store, dsp: &str) -> Result<String> {
    Ok(db
        .phones_devices(dsp)?
        .into_iter()
        .next()
        .ok_or_else(|| Error::new("phones_none", 409))?
        .id)
}

/// The DSP's phone receives `body` from `number`, written at `at`, and reports it, as its app
/// does: the text's ID.
pub async fn texted(state: &Arc<State>, dsp: &str, number: &str, body: &str, at: i64) -> String {
    let text = Reported {
        client_id: crypto::id("sms").unwrap(),
        direction: Direction::In,
        number: number.to_owned(),
        body: body.to_owned(),
        at,
        files: Vec::new(),
    };
    let at_dsp = dsp.to_owned();
    filing::changing(state, dsp, move |db| {
        let kept = db.phones_report(&at_dsp, &phone(db, &at_dsp)?, vec![text])?;
        let id = kept[0].id.clone();
        Ok((id.clone(), vec![id]))
    })
    .await
    .unwrap()
}
/// The DSP's phone says how the text `id` Dispatch sent went, `status`, as its app does: of the
/// attempt its outbox gave it, while it waits there; a delivery report counts from any.
pub async fn reported(state: &Arc<State>, dsp: &str, id: &str, status: Status) {
    let (at_dsp, id) = (dsp.to_owned(), id.to_owned());
    filing::changing(state, dsp, move |db| {
        let phone = phone(db, &at_dsp)?;
        let attempt = db
            .phones_outbox(&at_dsp, &phone)?
            .into_iter()
            .find(|(text, _)| text.id == id)
            .map_or(0, |(_, attempt)| attempt);
        let report = StatusReport {
            id: id.clone(),
            attempt,
            status,
            at: now(),
            error: (status == Status::Failed).then(|| "No service".to_owned()),
        };
        let moved = db.phones_status(&at_dsp, &phone, &report)?;
        Ok(((), moved.then_some(id).into_iter().collect()))
    })
    .await
    .unwrap();
}
/// What waits in the DSP's phone's outbox, as its app reads it: each text's number, body and who
/// sent it, oldest first.
pub async fn sent(state: &Arc<State>, dsp: &str) -> Vec<(String, String, String)> {
    let dsp = dsp.to_owned();
    let waiting = state
        .read(move |db| db.phones_outbox(&dsp, &phone(db, &dsp)?))
        .await
        .unwrap();
    waiting
        .into_iter()
        .map(|(text, _)| (text.number, text.body, text.sent_by.unwrap_or_default()))
        .collect()
}
