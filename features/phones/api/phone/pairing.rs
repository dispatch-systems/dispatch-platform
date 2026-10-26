//! Pairing a phone by the code someone at the DSP showed: what it says of itself, traded for its
//! key.
use super::access::site_dsp;
use crate::backend::{
    pairing,
    storage::{DOMAIN, DeviceStore, Pairing},
};
use dispatch_core::{
    Error, Result, State, db,
    foundation::{crypto, validate as v},
    server::http::{
        input::{Input, Reply},
        route::{Grant, Public},
    },
};
use dispatch_phones_contract::NUMBER_MOST;
use serde::Serialize;
use std::sync::Arc;

/// The most characters a code may be typed with, spaces or dashes among its letters.
const TYPED_MOST: usize = 16;
/// The most characters a phone's model may be, and the app's version.
const MODEL_MOST: usize = 100;
const VERSION_MOST: usize = 40;

/// What pairing answers the phone with: its key, which it keeps and sends with every call, and
/// what it's called.
#[derive(Serialize)]
struct Paired {
    key: String,
    phone: PairedPhone,
    dsp: PairedDsp,
}
#[derive(Serialize)]
struct PairedPhone {
    id: String,
    name: String,
}
#[derive(Serialize)]
struct PairedDsp {
    name: String,
}
/// Pairs a phone by the code someone at the DSP showed: its own number, model and the app's
/// version, then its key. What it sends is checked before the database is asked for.
pub(in crate::api) async fn pair(state: Arc<State>, input: Input, access: Public) -> Result<Reply> {
    v::fields(&input.body, &["code", "number", "model", "version"])?;
    let code = pairing::kept(v::text(&input.body, "code", pairing::LENGTH, TYPED_MOST)?)
        .ok_or_else(|| Error::new("phones_pairing_invalid", 401))?;
    let number = v::text(&input.body, "number", 0, NUMBER_MOST)?.to_owned();
    let model = v::name(&input.body, "model", MODEL_MOST)?;
    let version = v::name(&input.body, "version", VERSION_MOST)?;
    let asked = input.clone();
    let dsp = state
        .read(move |db| {
            access.authorize(db, &asked)?;
            site_dsp(db, &asked)
        })
        .await?;
    let key = pairing::new_key()?;
    let (asked, kept) = (input.clone(), crypto::sha(&key));
    let device = state
        .run_scoped(dsp.id.clone(), DOMAIN, move |db| {
            let dsp = site_dsp(db, &asked)?;
            let pairing = Pairing {
                number: &number,
                model: &model,
                app_version: &version,
            };
            let device = db.phones_pair(&dsp.id, &code, &kept, &pairing, db::now())?;
            let by = db.phones_paired_by(&dsp.id, &device.id)?;
            db.audit(
                by.as_deref(),
                Some(&dsp.id),
                "phones.paired",
                &device.logged(),
            )?;
            Ok(device)
        })
        .await?;
    Reply::of(&Paired {
        key,
        phone: PairedPhone {
            id: device.id,
            name: device.name,
        },
        dsp: PairedDsp { name: dsp.name },
    })
}
