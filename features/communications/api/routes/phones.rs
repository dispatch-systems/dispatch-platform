//! Managing the DSP's dispatch phones, as its card on Settings' DSP Connections does: listing them,
//! the code that pairs the next, renaming one and removing it. Each behind `phones.manage` (Manage
//! Phones) or core's `connections.manage` (Manage DSP Connections), and refused while
//! Communications is off for the DSP, which managing all its connections still lets a member in.
use super::id;
use crate::{
    api::types::{PhonePairing, PhonePairingStatus, PhoneRow, Phones},
    backend::{
        phone::{
            live, pairing,
            storage::{Device, DeviceStore, PAIRING_MS},
        },
        storage::DOMAIN,
        texts::filing,
    },
};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::{self, Store},
    ensure,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Grant, Member},
    },
};
use std::sync::Arc;

/// Managing the DSP's phones, as managing all its connections includes.
pub(super) const MANAGE: Dsp = Dsp("connections.manage|phones.manage");
/// The most characters a phone's name may be.
const NAME_MOST: usize = 60;

/// Refuses while Communications is off for the DSP, as managing all its connections still lets a
/// member in.
fn on(c: &Context) -> Result<()> {
    ensure(c.has(crate::FEATURE.switch.id), "not_found", 404)
}
fn row(device: &Device, now: i64) -> PhoneRow {
    PhoneRow {
        id: device.id.clone(),
        name: device.name.clone(),
        number: device.number.clone(),
        model: device.model.clone(),
        app_version: device.app_version.clone(),
        online: device.online(now),
        last_seen_at: device.last_seen_at,
    }
}

/// The DSP's phones.
pub(super) fn listed(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    on(c)?;
    v::fields(&input.query, &[])?;
    let now = db::now();
    Reply::of(&Phones {
        phones: db
            .phones_devices(c.dsp_id())?
            .iter()
            .map(|device| row(device, now))
            .collect(),
    })
}
/// A new code to pair the next phone, in place of any shown before: refused as
/// `phones_address_required` while the DSP has no address of its own for the phone to reach.
pub(super) fn new_pairing(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    on(c)?;
    v::fields(&input.body, &[])?;
    let site = c
        .dsp
        .code
        .as_deref()
        .map(|code| db.config.dsp_url(code))
        .ok_or_else(|| Error::new("phones_address_required", 409))?;
    let code = pairing::new_code()?;
    let kept = pairing::kept(&code).ok_or_else(|| Error::new("phones_pairing_unavailable", 503))?;
    let at = db::now();
    let id = db.phones_new_pairing(c.dsp_id(), c.actor(), &kept, at)?;
    Reply::of(&PhonePairing {
        id,
        picture: pairing::picture(&pairing::link(&site, &code))?,
        code,
        expires_at: at + PAIRING_MS,
    })
}
/// How the code `id` the card shows stands: waiting for a phone, paired one, replaced by a newer
/// code, or expired.
pub(super) fn pairing_status(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    on(c)?;
    v::fields(&input.query, &[])?;
    let id = id(input.param("id"), "pairing")?;
    Reply::of(&PhonePairingStatus {
        status: db.phones_pairing(c.dsp_id(), &id, db::now())?,
    })
}
/// Gives the phone a new name.
pub(super) fn rename(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    on(c)?;
    v::fields(&input.body, &["name"])?;
    let id = id(input.param("id"), "phone")?;
    let name = v::name(&input.body, "name", NAME_MOST)?;
    let before = db
        .phones_device(c.dsp_id(), &id)?
        .ok_or_else(|| Error::new("phones_not_found", 404))?;
    let device = db.phones_rename(c.dsp_id(), &id, &name)?;
    if device.name != before.name {
        c.audit(
            db,
            "phones.renamed",
            &format!("{} to {}", before.name, device.name),
        )?;
    }
    Reply::of(&row(&device, db::now()))
}
/// Removes the phone: its key works no more, its texts stay, and those still waiting on it fail,
/// which filing hears of.
pub(super) async fn remove(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    on(&c)?;
    v::fields(&input.body, &[])?;
    let id = id(input.param("id"), "phone")?;
    let dsp = c.dsp.id.clone();
    state
        .run_scoped(dsp.clone(), DOMAIN, move |db| {
            let c = access.revalidate(db, &c)?;
            on(&c)?;
            let device = db.phones_remove(&c.dsp.id, &id, db::now())?;
            c.audit(db, "phones.removed", &device.logged())
        })
        .await?;
    // A phone waiting on its outbox hears at once that it was removed.
    live::queued(&state, &dsp);
    filing::told(&state, &dsp).await;
    Ok(Reply::ok())
}
