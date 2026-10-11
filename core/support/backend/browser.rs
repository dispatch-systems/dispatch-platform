//! A lease-bound browser identity with the reporter's current access and no write authority.
use super::{jobs, tickets};
use crate::{
    Result,
    accounts::Auth,
    db::{Store, now, s},
    ensure,
    foundation::{config::Site, crypto},
};
use serde_json::{Value, json};

pub const PREFIX: &str = "support-read:";
pub fn read_only(auth: &Auth) -> bool {
    auth.raw.starts_with(PREFIX)
}
pub fn issue(db: &Store, job: &str, lease: &str) -> Result<Value> {
    let run = jobs::leased(db, job, lease)?;
    let ticket = tickets::record(db, &run.ticket)?;
    let scope = s(&ticket, "reporter_scope");
    let origin = if scope.is_empty() {
        db.config.origin.clone()
    } else {
        let dsp = db.find_dsp(scope)?;
        let code = dsp
            .code
            .ok_or_else(|| crate::Error::new("support_browser_unavailable", 409))?;
        db.config.dsp_url(&code)
    };
    let unsigned = format!("{PREFIX}{job}:{lease}");
    let token = format!("{unsigned}:{}", crypto::sign(&db.key, &unsigned));
    let secure = !db.config.development;
    Ok(json!({
        "origin":origin,"ticketId":run.ticket,"expiresAt":now()+90000,
        "sourceSha":db.config.source.commit,"version":db.config.source.version,"release":db.config.release,
        "cookie":{
            "name":if secure {"__Host-dispatch_support_read"} else {"dispatch_support_read"},
            "value":token,"url":origin,"path":"/","httpOnly":true,"secure":secure,"sameSite":"Strict"
        }
    }))
}
pub(crate) fn identity(db: &Store, raw: &str, site: &Site) -> Result<(String, Option<String>)> {
    ensure(raw.len() < 300, "sign_in_required", 401)?;
    let parts: Vec<_> = raw.split(':').collect();
    ensure(
        parts.len() == 4 && parts[0] == "support-read",
        "sign_in_required",
        401,
    )?;
    let unsigned = parts[..3].join(":");
    ensure(
        crypto::equal(&crypto::sign(&db.key, &unsigned), parts[3]),
        "sign_in_required",
        401,
    )?;
    let run = jobs::leased(db, parts[1], parts[2])
        .map_err(|_| crate::Error::new("sign_in_required", 401))?;
    let ticket = tickets::record(db, &run.ticket)?;
    let scope = s(&ticket, "reporter_scope");
    let expected = if scope.is_empty() {
        Site::Admin
    } else {
        let dsp = db.find_dsp(scope)?;
        Site::Dsp(
            dsp.code
                .ok_or_else(|| crate::Error::new("sign_in_required", 401))?,
        )
    };
    ensure(site == &expected, "sign_in_required", 401)?;
    Ok((
        s(&ticket, "reporter_id").to_owned(),
        (!scope.is_empty()).then(|| scope.to_owned()),
    ))
}
