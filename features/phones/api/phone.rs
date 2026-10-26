//! The address a DSP's phones talk to, on the DSP's own address: pairing by a code, then, with
//! the key that gave it, waiting on the texts Dispatch has for it, reporting the texts it
//! receives and sends, their pictures' bytes, and how Dispatch's texts went. No browser calls
//! these: a request carrying a cookie, or at another address than a DSP's, is refused.
use crate::backend::{
    files, live, numbers, pairing,
    storage::{
        DOMAIN, Device, Kept, MOST_TEXTS, Pairing, PhonesStore, Reported, ReportedFile,
        StatusReport,
    },
};
use dispatch_core::accounts::api::types::Dsp;
use dispatch_core::{
    Error, Result, State,
    db::{self, Store},
    ensure,
    foundation::{config::Site, crypto, validate as v},
    server::http::{
        input::{Input, Reply},
        route::{Access, Grant, Public},
        upload::Upload,
    },
    tenancy::api::types::DspStatus,
};
use dispatch_phones_contract::{Direction, Status, Text};
use serde::Serialize;
use serde_json::Value;
use std::{sync::Arc, time::Duration};

/// How long a phone waiting on its outbox waits before it's answered with nothing.
const WAIT: Duration = Duration::from_secs(20);

/// A paired phone, by the key it was given: `Authorization: Bearer <key>`, at its DSP's address,
/// while Phones is on there. Public to the server, as Slack's signed events are: the route checks
/// the key itself, and no session reaches it.
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
async fn checked_in(state: &Arc<State>, input: &Input, access: Phone) -> Result<Caller> {
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
fn site_dsp(db: &Store, input: &Input) -> Result<Dsp> {
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
pub(super) async fn pair(state: Arc<State>, input: Input, access: Public) -> Result<Reply> {
    v::fields(&input.body, &["code", "number", "model", "version"])?;
    let code = pairing::kept(v::text(&input.body, "code", 8, 16)?)
        .ok_or_else(|| Error::new("phones_pairing_invalid", 401))?;
    let number = v::text(&input.body, "number", 0, 32)?.to_owned();
    let model = v::name(&input.body, "model", 100)?;
    let version = v::name(&input.body, "version", 40)?;
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
                &shown(&device),
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
/// How a phone reads in the log: its name and number.
pub(super) fn shown(device: &Device) -> String {
    if device.number.is_empty() {
        device.name.clone()
    } else {
        format!("{} ({})", device.name, device.number)
    }
}

/// The texts Dispatch has for the phone, each to send from its number to `number`, with its
/// pictures to fetch first. A text tried again comes again with a later `attempt`.
#[derive(Serialize)]
struct Outbox {
    texts: Vec<Outgoing>,
}
#[derive(Serialize)]
struct Outgoing {
    id: String,
    number: String,
    body: String,
    files: Vec<OutgoingFile>,
    attempt: i64,
}
#[derive(Serialize)]
struct OutgoingFile {
    id: String,
    name: String,
    mime: String,
    size: i64,
}
impl Outgoing {
    fn of((text, attempt): (Text, i64)) -> Self {
        Self {
            attempt,
            id: text.id,
            number: text.number,
            body: text.body,
            files: text
                .files
                .into_iter()
                .map(|file| OutgoingFile {
                    id: file.id,
                    name: file.name,
                    mime: file.mime,
                    size: file.size,
                })
                .collect(),
        }
    }
}
/// Waits, up to twenty seconds, for texts Dispatch has for the phone: checking in, as it does.
pub(super) async fn outbox(state: Arc<State>, input: Input, access: Phone) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let caller = checked_in(&state, &input, access).await?;
    let _slot = state
        .topics
        .slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::new("platform_busy", 503))?;
    // Subscribed before reading, so a text queued between the read and the wait still wakes it.
    let mut listener = state.topics.subscribe(live::TOPIC, &caller.dsp);
    let until = tokio::time::Instant::now() + WAIT;
    loop {
        let asked = input.clone();
        let texts = state
            .read(move |db| {
                let caller = access.authorize(db, &asked)?;
                db.phones_outbox(&caller.dsp, &caller.device.id)
            })
            .await?;
        if !texts.is_empty() || tokio::time::Instant::now() >= until {
            return Reply::of(&Outbox {
                texts: texts.into_iter().map(Outgoing::of).collect(),
            });
        }
        let _ = tokio::time::timeout_at(until, listener.changed()).await;
    }
}

/// What reporting answers with: for each text, by the phone's ID of it, Dispatch's, and its
/// files' in the order reported, whose bytes the phone sends next.
#[derive(Serialize)]
struct Reports {
    texts: Vec<Report>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    client_id: String,
    id: String,
    files: Vec<String>,
}
/// Keeps the texts the phone reports: those it received, and those sent from its own messaging
/// app, each once.
pub(super) async fn texts(state: Arc<State>, input: Input, access: Phone) -> Result<Reply> {
    v::fields(&input.body, &["texts"])?;
    let reported = input.body["texts"]
        .as_array()
        .filter(|texts| !texts.is_empty() && texts.len() <= MOST_TEXTS)
        .ok_or_else(|| Error::new("invalid_input", 400))?
        .iter()
        .map(reported)
        .collect::<Result<Vec<_>>>()?;
    let caller = checked_in(&state, &input, access).await?;
    let (asked, dsp) = (input.clone(), caller.dsp.clone());
    let kept = state
        .run_scoped(caller.dsp, DOMAIN, move |db| {
            let caller = access.authorize(db, &asked)?;
            db.phones_report(&caller.dsp, &caller.device.id, reported)
        })
        .await?;
    live::changed(&state, &dsp).await;
    Reply::of(&Reports {
        texts: kept
            .into_iter()
            .map(
                |Kept {
                     client_id,
                     id,
                     files,
                 }| Report {
                    client_id,
                    id,
                    files,
                },
            )
            .collect(),
    })
}
/// One text as the phone reports it.
fn reported(text: &Value) -> Result<Reported> {
    v::fields(
        text,
        &["clientId", "direction", "number", "body", "at", "files"],
    )?;
    let files = match &text["files"] {
        Value::Null => Vec::new(),
        Value::Array(files) if files.len() <= 10 => files
            .iter()
            .map(|file| {
                v::fields(file, &["name", "mime", "size"])?;
                Ok(ReportedFile {
                    name: v::name(file, "name", 200)?,
                    mime: v::text(file, "mime", 1, 100)?.to_owned(),
                    size: v::integer(file, "size", 0, files::LARGEST as i64)?,
                })
            })
            .collect::<Result<_>>()?,
        _ => return Err(Error::new("invalid_input", 400)),
    };
    Ok(Reported {
        client_id: v::text(text, "clientId", 1, 128)?.to_owned(),
        direction: match v::choice(text, "direction", &["in", "out"])? {
            "in" => Direction::In,
            _ => Direction::Out,
        },
        number: numbers::normal(v::text(text, "number", 1, 32)?)
            .ok_or_else(|| Error::new("invalid_input", 400))?,
        body: v::text(text, "body", 0, 10_000)?.to_owned(),
        at: v::integer(text, "at", 0, i64::MAX)?,
        files,
    })
}

/// Keeps the bytes of a file of a text the phone reported.
pub(super) async fn file_in(
    state: Arc<State>,
    input: Input,
    access: Phone,
    bytes: Upload,
) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let asked = input.clone();
    let caller = state.read(move |db| access.authorize(db, &asked)).await?;
    let arrived = files::receive(&state, &caller.dsp, bytes).await?;
    let (asked, id) = (input.clone(), input.param("id").to_owned());
    state
        .run_scoped(caller.dsp.clone(), DOMAIN, move |db| {
            let caller = access.authorize(db, &asked)?;
            arrived.place(db, &caller.dsp)?;
            match db.phones_file_arrived(&caller.dsp, &caller.device.id, &id, &arrived.sha256) {
                Ok(before) => {
                    before.map_or(Ok(()), |before| files::forget(db, &caller.dsp, &before))
                }
                Err(error) => {
                    files::forget(db, &caller.dsp, &arrived.sha256)?;
                    Err(error)
                }
            }
        })
        .await?;
    live::changed(&state, &caller.dsp).await;
    Ok(Reply::ok())
}
/// The bytes of a picture of a text Dispatch has for the phone to send: asked again, once the
/// download is ready, whether the phone still may.
pub(super) async fn file_out(state: Arc<State>, input: Input, access: Phone) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let (asked, id) = (input.clone(), input.param("id").to_owned());
    let (dsp, file) = state
        .read(move |db| {
            let caller = access.authorize(db, &asked)?;
            let file = db.phones_outgoing_file(&caller.dsp, &caller.device.id, &id)?;
            Ok((caller.dsp, file))
        })
        .await?;
    let file = file.ok_or_else(|| Error::new("phones_file_not_found", 404))?;
    let sha256 = file
        .sha256
        .ok_or_else(|| Error::new("phones_file_not_found", 404))?;
    let reply = files::answer(&state, &dsp, &sha256, &file.file.name, &file.file.mime).await?;
    let asked = input.clone();
    state.read(move |db| access.authorize(db, &asked)).await?;
    Ok(reply)
}

/// Notes how the texts Dispatch had for the phone went.
pub(super) async fn statuses(state: Arc<State>, input: Input, access: Phone) -> Result<Reply> {
    v::fields(&input.body, &["statuses"])?;
    let statuses = input.body["statuses"]
        .as_array()
        .filter(|statuses| !statuses.is_empty() && statuses.len() <= MOST_TEXTS)
        .ok_or_else(|| Error::new("invalid_input", 400))?
        .iter()
        .map(|status| {
            v::fields(status, &["id", "attempt", "status", "at", "error"])?;
            let error = match &status["error"] {
                Value::Null => None,
                _ => Some(v::text(status, "error", 0, 500)?.to_owned()),
            };
            Ok(StatusReport {
                id: v::text(status, "id", 1, 64)?.to_owned(),
                attempt: v::integer(status, "attempt", 0, i64::MAX)?,
                status: match v::choice(status, "status", &["sent", "delivered", "failed"])? {
                    "sent" => Status::Sent,
                    "delivered" => Status::Delivered,
                    _ => Status::Failed,
                },
                at: v::integer(status, "at", 0, i64::MAX)?,
                error,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let caller = checked_in(&state, &input, access).await?;
    let (asked, dsp) = (input.clone(), caller.dsp.clone());
    let moved = state
        .run_scoped(caller.dsp, DOMAIN, move |db| {
            let caller = access.authorize(db, &asked)?;
            let mut moved = false;
            for report in &statuses {
                moved |= db.phones_status(&caller.dsp, &caller.device.id, report)?;
            }
            Ok(moved)
        })
        .await?;
    if moved {
        live::changed(&state, &dsp).await;
    }
    Ok(Reply::ok())
}
