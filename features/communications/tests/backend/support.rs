//! What every Communications backend test installs, and how one starts. A test binary holds one
//! registry, so each installs the same: Slack, Communications, and stand-ins for what other
//! features expose that it reads. A test starts with a platform with one DSP, every feature on
//! for it, and one of its fixture workspaces connected, its members read: the workspace's admin
//! has approved all the app asks for.
use crate::api::types::Message;
use crate::backend::{conversations, storage::CommunicationsStore};
use crate::integrations::messages::incoming;
use dispatch_core::{
    Result, State,
    accounts::{Context, SessionLifetime},
    db::{Store, now},
    foundation::config::Site,
    manifest::{
        Feature, feature, mandatory, optional,
        people::{Directory, Everyone, Person, Photo, Source, SourceInfo},
    },
    server::http::route::Dsp,
    testing,
};
use dispatch_routes_contract::{Collected, CollectedDay, Driven, DriverRoutes, RouteDay};
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

/// Installs Slack, Communications and the stand-ins.
pub fn install() {
    testing::install(
        &[],
        &[
            &dispatch_slack::FEATURE,
            &crate::FEATURE,
            &NAMING,
            &DRIVING,
            &FLEET,
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
/// who people are (`directory`, always), their routes (`driver_routes`, while `driving` is on)
/// and the DSP's vans (`amazon_fleet`, while `fleet` is on). Each answers what a test says of
/// its DSP with `know`, and nothing else.
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

/// What the stand-ins know of a DSP.
#[derive(Default)]
struct Known {
    people: Vec<Person>,
    routes: Vec<RouteDay>,
    vans: Vec<Van>,
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
    known.insert(
        dsp.to_owned(),
        Known {
            people,
            routes,
            vans,
        },
    );
    state.changed(Some(dsp));
}
/// Switches the stand-in `feature`, `driving` or `fleet`, on or off for the DSP.
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
    /// The routes it knows driven by `ids` from `from` to `to`, and their days.
    fn routes(&self, _: &Store, dsp: &str, ids: &[String], from: &str, to: &str) -> Result<Driven> {
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
    fn vans(&self, _: &Store, dsp: &str, vins: &[String]) -> Result<Vec<Van>> {
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
