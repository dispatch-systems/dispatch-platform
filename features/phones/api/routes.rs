//! Phones' endpoints, each registered with its path and access: the Phones card's behind
//! `phones.manage` (Manage Phones) or `connections.manage` (Manage DSP Connections), and its
//! phones' behind the key each was given, at the DSP's own address (`phone.rs`).
use super::{
    phone::{self, Phone},
    types::{PhonePairing, PhoneRow, Phones},
};
use crate::backend::{
    files, live, pairing,
    storage::{Device, PAIRING_MS, PhonesStore},
};
use dispatch_core::{
    Error, Result,
    db::{self, Store},
    ensure,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Member, Public, Route, async_get, async_post, read, upload, write},
    },
};

/// Managing the DSP's phones, as managing all its connections includes.
const MANAGE: Dsp = Dsp("connections.manage|phones.manage");

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/phones", MANAGE, listed),
        write("/api/dsp/phones/pairing", MANAGE, new_pairing),
        write("/api/dsp/phones/{id}/rename", MANAGE, rename),
        write("/api/dsp/phones/{id}/remove", MANAGE, remove),
        async_post("/api/phones/pair", Public, phone::pair),
        async_get("/api/phones/outbox", Phone, phone::outbox),
        async_post("/api/phones/texts", Phone, phone::texts),
        upload(
            "/api/phones/files/{id}",
            Phone,
            files::LARGEST,
            phone::file_in,
        ),
        async_get("/api/phones/files/{id}", Phone, phone::file_out),
        async_post("/api/phones/statuses", Phone, phone::statuses),
    ]
}

/// Refuses the card's calls while Phones is off for the DSP, as managing all its connections
/// still lets a member in.
fn on(c: &Member) -> Result<()> {
    ensure(c.has("phones"), "not_found", 404)
}
/// One of Phones' IDs, `phone_` and 32 hexadecimal digits.
fn phone_id(value: &str) -> Result<&str> {
    ensure(db::identifier(value, "phone_"), "invalid_input", 400)?;
    Ok(value)
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
fn listed(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
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
fn new_pairing(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
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
    db.phones_new_pairing(c.dsp_id(), c.actor(), &kept, at)?;
    Reply::of(&PhonePairing {
        picture: pairing::picture(&pairing::link(&site, &code))?,
        code,
        expires_at: at + PAIRING_MS,
    })
}
/// Gives the phone a new name.
fn rename(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    on(c)?;
    v::fields(&input.body, &["name"])?;
    let id = phone_id(input.param("id"))?;
    let name = v::name(&input.body, "name", 60)?;
    let before = db
        .phones_device(c.dsp_id(), id)?
        .ok_or_else(|| Error::new("phones_not_found", 404))?;
    let device = db.phones_rename(c.dsp_id(), id, &name)?;
    if device.name != before.name {
        c.audit(
            db,
            "phones.renamed",
            &format!("{} to {}", before.name, device.name),
        )?;
    }
    Reply::of(&row(&device, db::now()))
}
/// Removes the phone: its key works no more, and its texts stay.
fn remove(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    on(c)?;
    v::fields(&input.body, &[])?;
    let id = phone_id(input.param("id"))?;
    let device = db.phones_remove(c.dsp_id(), id, db::now())?;
    c.audit(db, "phones.removed", &phone::shown(&device))?;
    // A phone waiting on its outbox hears at once that it was removed.
    live::queued(c.state, c.dsp_id());
    Ok(Reply::ok())
}
