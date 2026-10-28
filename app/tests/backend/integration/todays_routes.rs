//! Today's Routes from a live read to the page, over HTTP against the real router served
//! in-process: Routes keeps a live read of the Cortex fixture's day, as Live Route Data does in
//! fixture mode, its drivers claiming their work phones as any read's do; a text a paired phone
//! reports from one of those phones is filed under the driver holding it; and once live route
//! data is emptied, as at the stop, Today's Routes lists nobody.
use crate::support::{self, Call, OWNER, Who};
use dispatch_core::{
    db::{self, s},
    testing as common,
};
use dispatch_cortex as cortex;
use serde_json::{Value, json};

/// The DSP the test reads, at its own address.
const NORTHLINE: &str = "Northline Logistics";
const CODE: &str = "nll";

/// The served router, with the calls this test makes on it.
struct Server(support::Server);
impl std::ops::Deref for Server {
    type Target = support::Server;
    fn deref(&self) -> &support::Server {
        &self.0
    }
}
impl Server {
    /// The router with Northline at a station with Cortex connected, so Routes can keep its
    /// live reads, and Northline's id.
    async fn start() -> (Self, String) {
        let (server, dsp) = support::Server::start_with(|store| {
            let dsp = s(
                &store
                    .platform
                    .one("SELECT id FROM dsps WHERE name=?", [NORTHLINE])
                    .unwrap()
                    .unwrap(),
                "id",
            )
            .to_owned();
            store
                .set_profile(&dsp, json!({"stationCode":"TST1","abbreviation":"NLOG"}))
                .unwrap();
            common::ready_connection(store, &dsp, cortex::PROVIDER).unwrap();
            dsp
        })
        .await;
        (Self(server), dsp)
    }
    async fn get(&self, owner: &Who, path: &str) -> Value {
        let answer = self.send(Call::get(path).who(owner)).await;
        assert_eq!(answer.status, 200, "{path}: {}", answer.body);
        answer.body
    }
    /// A phone's call to the DSP's own address, with its key once it has one.
    async fn phone(&self, key: Option<&str>, path: &str, body: Value) -> Value {
        let at = self.host(CODE);
        let mut request = self
            .client
            .post(self.url(path))
            .header("host", &at)
            .header("origin", format!("http://{at}"))
            .header("content-type", "application/json")
            .body(body.to_string());
        if let Some(key) = key {
            request = request.header("authorization", format!("Bearer {key}"));
        }
        let answer = self.answer(request).await;
        assert_eq!(answer.status, 200, "{path}: {}", answer.body);
        answer.body
    }
}

/// Each driver Today's Routes lists, by their ID, with the work phone it shows them holding and
/// their work conversation's latest text.
fn listed(routes: &Value) -> Vec<(String, Option<String>, Option<String>)> {
    let drivers = routes["drivers"].as_array().unwrap();
    let mut listed: Vec<_> = drivers
        .iter()
        .map(|driver| {
            (
                s(driver, "driverId").to_owned(),
                driver["today"]["phone"].as_str().map(str::to_owned),
                driver["conversation"]["last"]["text"]
                    .as_str()
                    .map(str::to_owned),
            )
        })
        .collect();
    listed.sort();
    listed
}

#[tokio::test]
async fn a_live_reads_holds_file_a_drivers_texts_under_them_until_live_route_data_empties() {
    let (server, dsp) = Server::start().await;
    // The platform owner, at the admin's address, looking at Northline.
    let owner = server.member(OWNER).await;
    // A phone paired at the DSP's own address with a code the owner made.
    let pairing = server
        .send(Call::post("/api/dsp/communications/phones/pairing", json!({})).who(&owner))
        .await;
    assert_eq!(pairing.status, 200, "{}", pairing.body);
    let phone = json!({"code":pairing.body["code"],"number":"(206) 555-0177","model":"Pixel 8a",
        "version":"1.0.0"});
    let paired = server
        .phone(None, "/api/communications/phone/pair", phone)
        .await;
    let key = s(&paired, "key").to_owned();

    // Routes keeps a live read of today, the Cortex fixture's day as Live Route Data reads it in
    // fixture mode: each driver claims the work phone their record shows.
    let (state, at) = (server.state.clone(), dsp.clone());
    let (read, revision) = server
        .state
        .read(move |db| {
            let request = dispatch_routes::live_request(db, &at)?;
            let revision = db
                .connection_revision(&at, cortex::PROVIDER)?
                .expect("Cortex is connected");
            Ok((cortex::routes::fixture(&request)?, revision))
        })
        .await
        .unwrap();
    let kept = dispatch_routes::keep_live(&state, &dsp, read, revision)
        .await
        .unwrap();
    assert_eq!(kept.holding, 2);
    let routes = server.get(&owner, "/api/dsp/communications/routes").await;
    let phones: Vec<_> = listed(&routes)
        .into_iter()
        .map(|(id, phone, _)| (id, phone))
        .collect();
    assert_eq!(
        phones,
        [
            ("driver-1".into(), Some("+12065550190".into())),
            ("driver-2".into(), Some("+12065550191".into())),
        ]
    );

    // The second driver texts from theirs: it's filed under them.
    let text = json!({"clientId":"sms-1","direction":"in","number":"+12065550191",
        "body":"Running ten minutes late","at":db::now()});
    server
        .phone(
            Some(&key),
            "/api/communications/phone/texts",
            json!({"texts":[text]}),
        )
        .await;
    let routes = server.get(&owner, "/api/dsp/communications/routes").await;
    assert_eq!(
        listed(&routes)[1],
        (
            "driver-2".into(),
            Some("+12065550191".into()),
            Some("Running ten minutes late".into())
        )
    );

    // Emptied, as at the stop: nobody's listed.
    dispatch_routes::clear_live(&state, &dsp).await.unwrap();
    let routes = server.get(&owner, "/api/dsp/communications/routes").await;
    assert_eq!((&routes["day"], listed(&routes).len()), (&Value::Null, 0));
}
