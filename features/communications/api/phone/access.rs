//! Who a phone's call comes from: a paired phone, by the key it was given, at its DSP's own
//! address, while the DSP is active and Communications is on there; or, for the app itself, which
//! a phone downloads before it has a key, anyone at that address.
use crate::backend::phone::{
    self,
    pairing::KEY_BYTES,
    storage::{Device, DeviceStore},
};
use dispatch_core::accounts::api::types::Dsp;
use dispatch_core::{
    Error, Result, State,
    db::{self, Store},
    ensure,
    foundation::{config::Site, crypto},
    server::http::{
        input::Input,
        route::{Access, Grant},
    },
    tenancy::api::types::DspStatus,
};
use std::sync::Arc;

/// A paired phone, by the key it was given: `Authorization: Bearer <key>`, at its DSP's address,
/// while Communications is on there. Public to the server, as Slack's signed events are: the route
/// checks the key itself, and no session reaches it.
#[derive(Clone, Copy)]
pub(super) struct Phone;
/// Who a phone's call comes from: the DSP, and the phone.
pub(super) struct Caller {
    pub dsp: String,
    pub device: Device,
}
impl Grant for Phone {
    type Who = Caller;
    fn access(self) -> Access {
        Access::Public
    }
    fn authorize(self, db: &Store, input: &Input) -> Result<Caller> {
        let dsp = site_dsp(db, input)?.id;
        let bearer = input
            .header("authorization")
            .strip_prefix("Bearer ")
            .unwrap_or("");
        ensure(
            input.headers.get_all("authorization").iter().count() == 1
                && bearer.len() == 2 * KEY_BYTES
                && bearer.bytes().all(|b| b.is_ascii_hexdigit()),
            "sign_in_required",
            401,
        )?;
        let key = crypto::sha(bearer);
        let device = match db.phones_device_by_key(&dsp, &key)? {
            Some(device) => device,
            None if db.phones_key_removed(&dsp, &key)? => {
                return Err(Error::new("phones_removed", 401));
            }
            None => return Err(Error::new("sign_in_required", 401)),
        };
        Ok(Caller { dsp, device })
    }
    fn tenant(self, who: &Caller) -> Option<&str> {
        Some(&who.dsp)
    }
}
/// Anyone at a DSP's own address, a browser too, while the DSP is active and Communications is on
/// there: for the app it hands out, which a phone downloads to install before it's paired. That's
/// the app anyone may download from its GitHub release, and nothing of the DSP's.
#[derive(Clone, Copy)]
pub(super) struct Address;
impl Grant for Address {
    type Who = ();
    fn access(self) -> Access {
        Access::Public
    }
    fn authorize(self, db: &Store, input: &Input) -> Result<()> {
        addressed(db, input).map(drop)
    }
}
/// Authorizes a phone's call and notes that it checked in, once `SEEN_MS` passed since it last was
/// (`phone::seen`), as read first and again in the write: bookkeeping, which no cached read depends
/// on, as presence is read afresh.
pub(super) async fn checked_in(state: &Arc<State>, input: &Input, access: Phone) -> Result<Caller> {
    let asked = input.clone();
    let caller = state.read(move |db| access.authorize(db, &asked)).await?;
    if !caller.device.noting(db::now()) {
        return Ok(caller);
    }
    let asked = input.clone();
    state
        .run_bookkeeping(move |db| {
            let caller = access.authorize(db, &asked)?;
            phone::seen(db, &caller.dsp, &caller.device, db::now())?;
            Ok(caller)
        })
        .await
}
/// The DSP whose address the request came to, active and with Communications on, for a request no
/// browser made: refused as `phones_off` while Communications is off there.
pub(super) fn site_dsp(db: &Store, input: &Input) -> Result<Dsp> {
    ensure(
        !input.headers.contains_key("cookie"),
        "sign_in_required",
        401,
    )?;
    addressed(db, input)
}
/// The DSP whose address the request came to, active and with Communications on: refused as
/// `phones_off` while Communications is off there.
fn addressed(db: &Store, input: &Input) -> Result<Dsp> {
    let Site::Dsp(code) = &input.site else {
        return Err(Error::new("sign_in_required", 401));
    };
    let dsp = db
        .dsp_at(code)?
        .filter(|dsp| dsp.status == DspStatus::Active)
        .ok_or_else(|| Error::new("sign_in_required", 401))?;
    ensure(
        db.feature_enabled(&dsp.id, crate::FEATURE.switch.id)?,
        "phones_off",
        403,
    )?;
    Ok(dsp)
}
