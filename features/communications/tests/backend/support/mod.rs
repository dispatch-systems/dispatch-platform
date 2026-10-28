//! What every Communications backend test installs, and how one starts. A test binary holds one
//! registry, so each installs the same: Slack, Communications, and stand-ins for what other
//! features expose that it reads, which it may not name, each by its contract: who people are
//! (`directory`, always), their routes and each saved day's holds (`routes`, `driver_routes` while
//! `driving` is on), today's as read live (`live`, `live_route_data` while `live` is on) and the
//! DSP's vans (`fleet`, `amazon_fleet` while `fleet` is on). Each answers what a test says of its
//! DSP, with `know`, `held` and `routed`, and nothing else. A test starts with a platform with one
//! DSP, every feature on for it, and its dispatch phone paired (`phone`), which `texted` and
//! `reported` speak for; or with one of its fixture workspaces connected too, its members read:
//! the workspace's admin has approved all the app asks for.
mod directory;
mod fleet;
mod live;
mod phone;
mod routes;

pub use live::routed;
pub use phone::{reported, sent, texted};

use crate::api::types::Message;
use crate::backend::{
    conversations,
    storage::CommunicationsStore,
    texts::{model::TextConversation, storage::TextStore},
};
use crate::integrations::messages::incoming;
use chrono::{NaiveDate, TimeZone};
use chrono_tz::Tz;
use dispatch_core::{
    State,
    accounts::{Context, SessionLifetime},
    db::now,
    foundation::{clock, config::Site},
    manifest::{
        events,
        people::{Person, PersonRole, Source, SourceInfo},
    },
    server::http::route::Dsp,
    testing,
};
use dispatch_routes_contract::{RouteDay, WorkPhoneHold, WorkPhonesChanged};
use dispatch_slack::{kit::Bot, testing as slack};
use dispatch_vans_contract::Van;
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

/// A platform with one DSP, every feature on for it, its phone paired, and its server's state.
pub fn started() -> (impl Drop, Arc<State>, String) {
    install();
    let (root, store, dsp) = testing::bootstrapped();
    phone::pair(&store, &dsp);
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
            &directory::NAMING,
            &routes::DRIVING,
            &live::LIVE,
            &fleet::FLEET,
        ],
    );
}
/// A platform with one DSP, its phone paired, and its server's state, with the DSP's workspace
/// `team` connected and its members read.
pub async fn connected(team: &str) -> (impl Drop, Arc<State>, Bot) {
    install();
    let (root, store, dsp) = testing::bootstrapped();
    phone::pair(&store, &dsp);
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

/// The source whose IDs name the stand-ins' drivers.
pub const DRIVERS: Source = Source::new(&SourceInfo {
    id: "drivers",
    label: "Drivers",
    order: 90,
});

/// What a stand-in knows of each DSP, by its ID: nothing until a test says.
struct Known<T>(Mutex<BTreeMap<String, T>>);
impl<T: Default> Known<T> {
    const fn new() -> Self {
        Self(Mutex::new(BTreeMap::new()))
    }
    /// What it knows of the DSP `dsp`, to read or change.
    fn of<R>(&self, dsp: &str, with: impl FnOnce(&mut T) -> R) -> R {
        let mut known = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        with(known.entry(dsp.to_owned()).or_default())
    }
}

/// Has the stand-ins answer, for the DSP `dsp`, who `people` are, their `routes`, and its `vans`,
/// letting go of the DSP's cached reads, as who everyone is, so the next reads them anew.
pub fn know(state: &State, dsp: &str, people: Vec<Person>, routes: Vec<RouteDay>, vans: Vec<Van>) {
    directory::know(dsp, people);
    routes::know(dsp, routes);
    fleet::know(dsp, vans);
    state.changed(Some(dsp));
}
/// Has the stand-ins answer who held the DSP's work phones: live route data's holds of its live
/// day `live_day`, if it holds one, and each saved day's; then says, as Routes does, of which days
/// they changed: the live day it held before, as emptying it does, the one it holds now, and each
/// saved one.
pub async fn held(
    state: &Arc<State>,
    dsp: &str,
    live_day: Option<&str>,
    live: Vec<WorkPhoneHold>,
    saved: Vec<(String, WorkPhoneHold)>,
) {
    let before = live::hold(dsp, live_day, live);
    let changed: BTreeSet<String> = before
        .into_iter()
        .chain(live_day.map(str::to_owned))
        .chain(saved.iter().map(|(day, _)| day.clone()))
        .collect();
    routes::save(dsp, saved);
    for day in changed {
        events::publish(state, dsp, WorkPhonesChanged { day }).await;
    }
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
