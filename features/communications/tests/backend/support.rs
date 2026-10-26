//! What every Communications backend test installs, and how one starts. A test binary holds one
//! registry, so each installs the same: Slack, Communications, and stand-ins for what other
//! features expose that it reads, a dispatch phone among them. A test starts with a platform with
//! one DSP, every feature on for it, and one of its fixture workspaces connected, its members
//! read: the workspace's admin has approved all the app asks for.
use crate::api::types::Message;
use crate::backend::{
    conversations,
    storage::CommunicationsStore,
    texts::storage::{TextConversation, TextStore},
};
use crate::integrations::messages::incoming;
use axum::body::Body;
use chrono::{NaiveDate, TimeZone};
use chrono_tz::Tz;
use dispatch_core::{
    Error, Result, State,
    accounts::{Context, SessionLifetime},
    db::{Store, now},
    foundation::{clock, config::Site, crypto},
    manifest::{
        Feature, events, feature, mandatory, optional,
        people::{Directory, Everyone, Person, PersonRole, Photo, Source, SourceInfo},
    },
    server::http::{input::Reply, route::Dsp, upload::Upload},
    testing,
};
use dispatch_phones_contract::{
    Added, Answer, Changes, Condition, Direction, File, Limits, Origin, Outgoing, Phone, Status,
    Text, Texting, TextsChanged,
};
use dispatch_routes_contract::{
    Collected, CollectedDay, Driven, DriverRoutes, LiveRouteData, RouteDay, WorkPhone,
    WorkPhoneHold, WorkPhonesChanged,
};
use dispatch_slack::{kit::Bot, testing as slack};
use dispatch_vans_contract::{AmazonFleet, Van};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

/// Reading the DSP's conversations and answering them.
pub const USE: Dsp = Dsp("communications.use");
/// What the workspace's admin approved: what Slack and Communications ask for.
const APPROVED: &[&str] = &[
    "users:read",
    "users:read.email",
    "chat:write",
    "chat:write.customize",
    "im:history",
    "im:write",
    "files:read",
    "files:write",
    "im:read",
];

/// A platform with one DSP, every feature on for it, and its server's state.
pub fn started() -> (impl Drop, Arc<State>, String) {
    install();
    let (root, store, dsp) = testing::bootstrapped();
    let config = store.config.clone();
    drop(store);
    (root, State::new(config).unwrap(), dsp)
}
/// The DSP's own timezone, and its day today.
pub async fn today(state: &Arc<State>, dsp: &str) -> (Tz, NaiveDate) {
    let dsp = dsp.to_owned();
    let zone: Tz = state
        .read(move |db| Ok(db.find_dsp(&dsp)?.timezone))
        .await
        .unwrap()
        .parse()
        .unwrap();
    (zone, clock::today(zone))
}
/// `hour`:`minute` on `day` in `zone`, in milliseconds.
pub fn at(zone: Tz, day: NaiveDate, hour: u32, minute: u32) -> i64 {
    zone.from_local_datetime(&day.and_hms_opt(hour, minute, 0).unwrap())
        .earliest()
        .unwrap()
        .timestamp_millis()
}
/// The conversation the text `id` is filed in.
pub async fn filed(state: &Arc<State>, dsp: &str, id: &str) -> TextConversation {
    let (dsp, id) = (dsp.to_owned(), id.to_owned());
    state
        .read(move |db| {
            let text = db.communications_text(&dsp, &id)?.unwrap();
            Ok(db
                .communications_text_conversation(&dsp, &text.conversation)?
                .unwrap())
        })
        .await
        .unwrap()
}
/// The route `code` the driver `driver` drove on `day` in the van `vin`, with `stops` stops, half
/// of them done, its wave starting at `wave`.
pub fn route(day: &str, driver: &str, code: &str, vin: &str, stops: i64, wave: i64) -> RouteDay {
    RouteDay {
        day: day.into(),
        driver_id: driver.into(),
        route_code: Some(code.into()),
        status: None,
        progress: None,
        vin: Some(vin.into()),
        wave_start_at: Some(wave),
        planned_departure_at: None,
        departed_at: None,
        first_delivery_at: None,
        last_delivery_at: None,
        session_end_at: None,
        stops,
        stops_completed: stops / 2,
        delivered: 0,
        not_delivered: 0,
        removed: 0,
        breaks_secs: None,
        overtime_secs: None,
        undelivered: Vec::new(),
    }
}

/// A driver Driver Match knows by their ID `id` of the stand-in drivers' source.
pub fn driver(code: &str, name: &str, id: &str) -> Person {
    Person {
        code: code.into(),
        name: name.into(),
        names: vec![name.into()],
        role: PersonRole::Driver,
        title: None,
        ids: BTreeMap::from([(DRIVERS, vec![id.into()])]),
        emails: Vec::new(),
        phones: Vec::new(),
        confirmed: true,
        photo: None,
    }
}
/// The van `vin`, named `name`.
pub fn van(vin: &str, name: &str) -> Van {
    Van {
        vin: vin.into(),
        name: Some(name.into()),
        status: "ACTIVE".into(),
        listed: true,
        last_listed: "2026-10-20T04:30:00Z".into(),
    }
}

/// Installs Slack, Communications and the stand-ins.
pub fn install() {
    testing::install(
        &[],
        &[
            &dispatch_slack::FEATURE,
            &crate::FEATURE,
            &NAMING,
            &DRIVING,
            &LIVE,
            &FLEET,
            &PHONING,
        ],
    );
}
/// A platform with one DSP, and its server's state, with the DSP's workspace `team` connected
/// and its members read.
pub async fn connected(team: &str) -> (impl Drop, Arc<State>, Bot) {
    install();
    let (root, store, dsp) = testing::bootstrapped();
    let config = store.config.clone();
    drop(store);
    let state = State::new(config).unwrap();
    let bot = workspace(&state, &dsp, team).await;
    (root, state, bot)
}
/// Connects the DSP's workspace `team`, its admin having approved all the app asks for, and
/// reads its members.
pub async fn workspace(state: &Arc<State>, dsp: &str, team: &str) -> Bot {
    let bot = slack::connect(state, dsp, team, APPROVED).await;
    slack::read_members(&bot).await.unwrap();
    bot
}
/// The DSP's owner, Test Owner, signed in and acting with Communications, under the name
/// `first` `last` and the email address `email` in place of theirs.
pub async fn member(
    state: &Arc<State>,
    dsp: &str,
    first: &str,
    last: &str,
    email: &str,
) -> Context {
    let raw = state
        .login(
            "owner@example.test".into(),
            "test-password-long".into(),
            "127.0.0.1".into(),
            "test".into(),
            SessionLifetime::Standard,
            Site::Admin,
        )
        .await
        .unwrap();
    let (dsp, first, last, email) = (
        dsp.to_owned(),
        first.to_owned(),
        last.to_owned(),
        email.to_owned(),
    );
    state
        .read(move |db| {
            let mut auth = db.authenticate(&raw, &Site::Admin)?;
            (auth.user.first_name, auth.user.last_name, auth.user.email) = (first, last, email);
            db.context(&auth, &dsp, USE.0)
        })
        .await
        .unwrap()
}
/// The workspace `team`'s member `user` writing `text` to the app, as Slack tells of it.
pub async fn writes(state: &Arc<State>, dsp: &str, team: &str, user: &str, text: &str) {
    let event = json!({"type": "message", "channel_type": "im", "channel": format!("D{}", &user[1..]),
        "user": user, "text": text, "ts": incoming::ts(now())});
    incoming::heard(state, dsp, team, &event).await.unwrap();
}
/// The texts of the conversation's messages, in the order written.
pub async fn texts(state: &Arc<State>, dsp: &str, conversation: &str) -> Vec<String> {
    messages(state, dsp, conversation)
        .await
        .into_iter()
        .map(|message| message.text)
        .collect()
}
/// The conversation's messages, as the page reads them.
pub async fn messages(state: &Arc<State>, dsp: &str, conversation: &str) -> Vec<Message> {
    let (shared, dsp, conversation) = (state.clone(), dsp.to_owned(), conversation.to_owned());
    state
        .read(move |db| Ok(conversations::opened(db, &shared, &dsp, &conversation, None)?.messages))
        .await
        .unwrap()
}
/// The conversation with the workspace `team`'s member `user`.
pub async fn conversation_with(
    state: &Arc<State>,
    dsp: &str,
    team: &str,
    user: &str,
) -> Option<crate::backend::storage::Conversation> {
    let (dsp, team, user) = (dsp.to_owned(), team.to_owned(), user.to_owned());
    state
        .read(move |db| db.communications_conversation_with(&dsp, &team, &user))
        .await
        .unwrap()
}

/// Stand-ins for what other features expose that Communications reads, which it may not name:
/// who people are (`directory`, always), their routes (`driver_routes`, while `driving` is on),
/// today's as read live (`live_route_data`, while `live` is on), the DSP's vans (`amazon_fleet`,
/// while `fleet` is on) and its dispatch phone (`texting`, while `phoning` is on). Each answers
/// what a test says of its DSP, with `know`, `held` and `texted`, and nothing else.
pub struct Standing;
static STANDING: Standing = Standing;
static NAMING: Feature = Feature {
    switch: mandatory("naming", "Naming"),
    exposes: &[dispatch_core::expose!(
        dyn Directory,
        &STANDING,
        "directory",
        "Who people are, as a test says.",
    )],
    ..feature("naming")
};
static DRIVING: Feature = Feature {
    switch: optional("driving", "Driving", &[]),
    exposes: &[dispatch_core::expose!(
        dyn DriverRoutes,
        &STANDING,
        "driver_routes",
        "Drivers' routes, as a test says.",
    )],
    ..feature("driving")
};
static LIVE: Feature = Feature {
    switch: optional("live", "Live", &[]),
    exposes: &[dispatch_core::expose!(
        dyn LiveRouteData,
        &STANDING,
        "live_route_data",
        "Today's routes as read live, as a test says.",
    )],
    ..feature("live")
};
static PHONING: Feature = Feature {
    switch: optional("phoning", "Phoning", &[]),
    exposes: &[dispatch_core::expose!(
        dyn Texting,
        &STANDING,
        "texting",
        "A dispatch phone's texts, as a test says.",
    )],
    ..feature("phoning")
};
static FLEET: Feature = Feature {
    switch: optional("fleet", "Fleet", &[]),
    exposes: &[dispatch_core::expose!(
        dyn AmazonFleet,
        &STANDING,
        "amazon_fleet",
        "The DSP's vans, as a test says.",
    )],
    ..feature("fleet")
};
/// The source whose IDs name the stand-in's drivers.
pub const DRIVERS: Source = Source::new(&SourceInfo {
    id: "drivers",
    label: "Drivers",
    order: 90,
});
/// The most days one read of drivers' routes reaches, and VINs one read of the fleet names, as
/// Routes and Vans refuse more.
const ROUTES_DAYS_MOST: i64 = 366;
const FLEET_VINS_MOST: usize = 100;

/// What the stand-ins know of a DSP.
#[derive(Default)]
struct Known {
    people: Vec<Person>,
    routes: Vec<RouteDay>,
    vans: Vec<Van>,
    /// The day live route data holds, its routes, and its holds on the work phones.
    live_day: Option<String>,
    live_routes: Vec<RouteDay>,
    live: Vec<WorkPhoneHold>,
    /// Each saved day's holds.
    saved: Vec<(String, WorkPhoneHold)>,
    /// The phone's texts, the last change given, what Dispatch sent from it, and whether what
    /// it sends next fails.
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
    /// bytes.
    off_while_reading: bool,
}
/// Who holds a DSP's work phones, as `held` says it.
struct Holds {
    live_day: Option<String>,
    live: Vec<WorkPhoneHold>,
    saved: Vec<(String, WorkPhoneHold)>,
}
static KNOWN: Mutex<BTreeMap<String, Known>> = Mutex::new(BTreeMap::new());
fn known<T>(dsp: &str, read: impl FnOnce(&Known) -> T) -> T {
    let known = KNOWN.lock().unwrap_or_else(|poison| poison.into_inner());
    read(known.get(dsp).unwrap_or(&Known::default()))
}
/// Has the stand-ins answer, for the DSP `dsp`, who `people` are, their `routes`, and its `vans`,
/// letting go of the DSP's cached reads, as who everyone is, so the next reads them anew.
pub fn know(state: &State, dsp: &str, people: Vec<Person>, routes: Vec<RouteDay>, vans: Vec<Van>) {
    let mut known = KNOWN.lock().unwrap_or_else(|poison| poison.into_inner());
    let known = known.entry(dsp.to_owned()).or_default();
    (known.people, known.routes, known.vans) = (people, routes, vans);
    state.changed(Some(dsp));
}
/// Has the stand-ins answer who held the DSP's work phones: live route data's holds of its live
/// day `live_day`, if it holds one, and each saved day's; then says they changed, as Routes does.
pub async fn held(
    state: &Arc<State>,
    dsp: &str,
    live_day: Option<&str>,
    live: Vec<WorkPhoneHold>,
    saved: Vec<(String, WorkPhoneHold)>,
) {
    {
        let mut known = KNOWN.lock().unwrap_or_else(|poison| poison.into_inner());
        let known = known.entry(dsp.to_owned()).or_default();
        (known.live_day, known.live, known.saved) = (live_day.map(str::to_owned), live, saved);
    }
    events::publish(state, dsp, WorkPhonesChanged).await;
}
/// Has live route data's stand-in answer `routes` as the live day's, as a live read leaves them,
/// which Routes says nothing of.
pub fn routed(dsp: &str, routes: Vec<RouteDay>) {
    let mut known = KNOWN.lock().unwrap_or_else(|poison| poison.into_inner());
    known.entry(dsp.to_owned()).or_default().live_routes = routes;
}
/// A hold on the work phone `phone`, ten digits, by the driver `driver`, from `since` until
/// `until`.
pub fn hold(driver: &str, phone: &str, since: i64, until: Option<i64>) -> WorkPhoneHold {
    WorkPhoneHold {
        driver_id: driver.to_owned(),
        phone: phone.to_owned(),
        since,
        until,
    }
}
/// The DSP's phone receives `body` from `number`, written at `at`, and says so, as Phones does.
pub async fn texted(state: &Arc<State>, dsp: &str, number: &str, body: &str, at: i64) -> String {
    let id = phone(dsp, |phone| {
        phone.change += 1;
        let text = Text {
            id: given("sms"),
            phone: STAND_IN.to_owned(),
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
/// Has what the DSP's phone sends next fail, or go.
pub fn failing(dsp: &str, failing: bool) {
    phone(dsp, |phone| phone.failing = failing);
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
    phone(dsp, |phone| {
        phone.held_while_queuing = Some(Holds {
            live_day,
            live,
            saved,
        });
    });
}
/// Hands the DSP's work phones over as `held_while_queuing` said, if it did, as its phone goes to
/// queue a text.
async fn queuing(state: &Arc<State>, dsp: &str) {
    if let Some(holds) = phone(dsp, |phone| phone.held_while_queuing.take()) {
        let Holds {
            live_day,
            live,
            saved,
        } = holds;
        held(state, dsp, live_day.as_deref(), live, saved).await;
    }
}
/// Has Communications switched off for the DSP while its phone next reads a file's bytes, one
/// asked of it or added for a text, as the platform owner might while a member waits on it.
pub fn off_while_reading(dsp: &str) {
    phone(dsp, |phone| phone.off_while_reading = true);
}
/// The pictures the DSP's phone keeps for a text not sent yet, by their IDs.
pub fn added(dsp: &str) -> Vec<String> {
    phone(dsp, |phone| {
        phone.added.iter().map(|(id, _)| id.clone()).collect()
    })
}
/// What Dispatch sent from the DSP's phone, in order.
pub fn sent(dsp: &str) -> Vec<Outgoing> {
    phone(dsp, |phone| phone.sent.clone())
}
/// The stand-in phone's ID.
const STAND_IN: &str = "phone_stand_in";
/// An ID the stand-in gives a text or a picture: opaque, as texting's contract says, and unlike
/// any Phones gives, as another offerer's may be.
fn given(kind: &str) -> String {
    format!("{kind}-{}", crypto::hex(&crypto::random::<6>().unwrap()))
}
/// What one text the stand-in sends may carry.
const LIMITS: Limits = Limits {
    body: 1_000,
    files: 3,
    largest: 1024 * 1024,
};
/// The most texts the stand-in phone gives at once: a page smaller than Phones', so what reads it
/// reads on as it says.
const PAGE: usize = 2;
fn phone<T>(dsp: &str, change: impl FnOnce(&mut Known) -> T) -> T {
    let mut known = KNOWN.lock().unwrap_or_else(|poison| poison.into_inner());
    change(known.entry(dsp.to_owned()).or_default())
}
/// Switches `feature`, a stand-in as `driving` or `fleet`, or Communications, on or off for the DSP.
pub async fn switch(state: &Arc<State>, dsp: &str, feature: &'static str, on: bool) {
    let dsp = dsp.to_owned();
    state
        .run(move |db| {
            let owner = testing::a_user(db);
            db.set_feature(&dsp, feature, on, &owner).map(drop)
        })
        .await
        .unwrap();
}

impl Directory for Standing {
    fn everyone(&self, _: &Store, dsp: &str) -> Result<Everyone> {
        let people = known(dsp, |known| known.people.clone());
        Ok(Everyone::new(people, BTreeMap::new()))
    }
    fn photo(&self, _: &Store, _: &str, _: &str) -> Result<Option<Photo>> {
        Ok(None)
    }
}
impl DriverRoutes for Standing {
    fn source(&self) -> Source {
        DRIVERS
    }
    fn collected(&self, _: &Store, _: &str) -> Result<Option<Collected>> {
        Ok(None)
    }
    /// The routes it knows driven by `ids` from `from` to `to`, and their days; refused across
    /// more days than Routes reads.
    fn routes(&self, _: &Store, dsp: &str, ids: &[String], from: &str, to: &str) -> Result<Driven> {
        let day = |day: &str| NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap();
        if (day(to) - day(from)).num_days() >= ROUTES_DAYS_MOST {
            return Err(Error::new("invalid_input", 400));
        }
        let routes: Vec<RouteDay> = known(dsp, |known| {
            known
                .routes
                .iter()
                .filter(|route| ids.contains(&route.driver_id))
                .filter(|route| (from..=to).contains(&route.day.as_str()))
                .cloned()
                .collect()
        });
        Ok(driven(routes))
    }
    /// Every route it knows driven on `day`.
    fn day(&self, _: &Store, dsp: &str, day: &str) -> Result<Driven> {
        let routes = known(dsp, |known| {
            known
                .routes
                .iter()
                .filter(|route| route.day == day)
                .cloned()
                .collect()
        });
        Ok(driven(routes))
    }
    /// The holds it knows of a saved day.
    fn work_phone_holds(&self, _: &Store, dsp: &str, day: &str) -> Result<Vec<WorkPhoneHold>> {
        Ok(known(dsp, |known| {
            known
                .saved
                .iter()
                .filter(|(saved, _)| saved == day)
                .map(|(_, hold)| hold.clone())
                .collect()
        }))
    }
    /// Whether any hold it knows, live or saved, is on `phone`.
    fn was_work_phone(&self, _: &Store, dsp: &str, phone: &str) -> Result<bool> {
        Ok(known(dsp, |known| {
            known
                .live
                .iter()
                .chain(known.saved.iter().map(|(_, hold)| hold))
                .any(|hold| hold.phone == phone)
        }))
    }
}
impl LiveRouteData for Standing {
    fn source(&self) -> Source {
        DRIVERS
    }
    fn live_day(&self, _: &Store, dsp: &str) -> Result<Option<String>> {
        Ok(known(dsp, |known| known.live_day.clone()))
    }
    /// The live day's routes, while it holds one.
    fn day(&self, _: &Store, dsp: &str) -> Result<Driven> {
        Ok(driven(known(dsp, |known| {
            known
                .live_routes
                .iter()
                .filter(|_| known.live_day.is_some())
                .cloned()
                .collect()
        })))
    }
    /// The live day's routes driven by `ids`.
    fn routes(&self, db: &Store, dsp: &str, ids: &[String]) -> Result<Driven> {
        let mut day = LiveRouteData::day(self, db, dsp)?;
        day.routes.retain(|route| ids.contains(&route.driver_id));
        Ok(day)
    }
    /// The work phones the live day's holds that haven't ended hold.
    fn work_phones(&self, _: &Store, dsp: &str) -> Result<Vec<WorkPhone>> {
        Ok(known(dsp, |known| {
            known
                .live
                .iter()
                .filter(|hold| hold.until.is_none())
                .map(|hold| WorkPhone {
                    driver_id: hold.driver_id.clone(),
                    phone: hold.phone.clone(),
                    since: hold.since,
                })
                .collect()
        }))
    }
    fn work_phone_holds(&self, _: &Store, dsp: &str) -> Result<Vec<WorkPhoneHold>> {
        Ok(known(dsp, |known| known.live.clone()))
    }
}
/// One phone, Dispatch, (206) 555-0100. What Dispatch sends from it is delivered at once, or fails
/// once a test says it does; tried again, as its condition allows in the write that queues it, it
/// waits on the phone. It keeps each picture added for a text as its adder's, and any file asked
/// of it is a photo.
impl Texting for Standing {
    fn phones(&self, _: &Store, _: &str) -> Result<Vec<Phone>> {
        Ok(vec![Phone {
            id: STAND_IN.to_owned(),
            name: "Dispatch".to_owned(),
            number: "+12065550100".to_owned(),
            online: true,
            last_seen_at: Some(now()),
        }])
    }
    /// A page of `PAGE` at most, as it pages.
    fn changes(&self, _: &Store, dsp: &str, after: i64) -> Result<Changes> {
        Ok(phone(dsp, |phone| {
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
                    phone(&at, |phone| {
                        phone.change += 1;
                        let text = Text {
                            id: given("sms"),
                            phone: STAND_IN.to_owned(),
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
                    phone(&at, |phone| {
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
            phone(dsp, |phone| phone.added.push((file.id.clone(), added.by)));
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
            phone(dsp, |phone| {
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
/// The phone reads a file's bytes: Communications goes off for the DSP meanwhile, once a test says
/// it does.
async fn read_bytes(state: &Arc<State>, dsp: &str) {
    if phone(dsp, |phone| std::mem::take(&mut phone.off_while_reading)) {
        switch(state, dsp, "communications", false).await;
    }
}
/// `routes`, with each day they're on read while under way.
fn driven(routes: Vec<RouteDay>) -> Driven {
    let days: BTreeSet<String> = routes.iter().map(|route| route.day.clone()).collect();
    Driven {
        days: days
            .into_iter()
            .map(|day| CollectedDay {
                day,
                complete: false,
            })
            .collect(),
        routes,
    }
}
impl AmazonFleet for Standing {
    fn collected(&self, _: &Store, _: &str) -> Result<Option<String>> {
        Ok(None)
    }
    /// The vans it knows under `vins`; refused for more than Vans reads.
    fn vans(&self, _: &Store, dsp: &str, vins: &[String]) -> Result<Vec<Van>> {
        if vins.len() > FLEET_VINS_MOST {
            return Err(Error::new("invalid_input", 400));
        }
        Ok(known(dsp, |known| {
            known
                .vans
                .iter()
                .filter(|van| vins.contains(&van.vin))
                .cloned()
                .collect()
        }))
    }
}
