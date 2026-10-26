//! Who a phone's call comes from: a paired phone, by the key it was given, at its DSP's own
//! address, while the DSP is active and Phones is on there.
use crate::backend::storage::{DOMAIN, Device, DeviceStore};
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
/// while Phones is on there. Public to the server, as Slack's signed events are: the route checks
/// the key itself, and no session reaches it.
#[derive(Clone, Copy)]
pub(in crate::api) struct Phone;
/// Who a phone's call comes from: the DSP, and the phone.
pub(in crate::api) struct Caller {
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
                && bearer.len() == 64
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
/// Authorizes a phone's call and notes that it checked in.
pub(super) async fn checked_in(state: &Arc<State>, input: &Input, access: Phone) -> Result<Caller> {
    let asked = input.clone();
    let caller = state.read(move |db| access.authorize(db, &asked)).await?;
    let (asked, at) = (input.clone(), caller.dsp.clone());
    state
        .run_scoped(at, DOMAIN, move |db| {
            let caller = access.authorize(db, &asked)?;
            db.phones_seen(&caller.dsp, &caller.device.id, db::now())?;
            Ok(caller)
        })
        .await
}
/// The DSP whose address the request came to, active and with Phones on, for a request no
/// browser made.
pub(super) fn site_dsp(db: &Store, input: &Input) -> Result<Dsp> {
    let Site::Dsp(code) = &input.site else {
        return Err(Error::new("sign_in_required", 401));
    };
    ensure(
        !input.headers.contains_key("cookie"),
        "sign_in_required",
        401,
    )?;
    let dsp = db
        .dsp_at(code)?
        .filter(|dsp| dsp.status == DspStatus::Active)
        .ok_or_else(|| Error::new("sign_in_required", 401))?;
    ensure(db.feature_enabled(&dsp.id, "phones")?, "phones_off", 403)?;
    Ok(dsp)
}
