//! Today's Routes from a live read to the page, over HTTP against the real router served
//! in-process: Routes keeps a live read of the Cortex fixture's day, as Live Route Data does in
//! fixture mode, its drivers claiming their work phones as any read's do; a text a paired phone
//! reports from one of those phones is filed under the driver holding it; and once live route
//! data is emptied, as at the stop, Today's Routes lists nobody.
use dispatch_core::{
    State,
    db::{self, Store, s},
    foundation::{config::Config, crypto},
    server::operations,
    testing as common,
};
use dispatch_cortex as cortex;
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, sync::Arc};

/// The DSP the test reads, at its own address.
const NORTHLINE: &str = "Northline Logistics";
const CODE: &str = "nll";

struct Server {
    _root: tempfile::TempDir,
    origin: String,
    /// The DSP's own address.
    at: String,
    state: Arc<State>,
    client: reqwest::Client,
}
/// The platform owner, at the admin's address, looking at the DSP.
struct Owner {
    cookie: String,
    csrf: String,
    view: String,
}
impl Server {
    /// The router alone, on the development fixtures, Northline at a station with Cortex
    /// connected, so Routes can keep its live reads.
    async fn start() -> (Self, String) {
        dispatch_backend::install();
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut config = Config::load().unwrap();
        config.root = root.path().join("state");
        db::private_dir(&config.root).unwrap();
        config.development = true;
        config.fixture = true;
        config.environment = "preview".into();
        config.standalone = true;
        config.port = port;
        config.origin = format!("http://127.0.0.1:{port}");
        config.dsp_origin = format!("http://{{code}}.localhost:{port}");
        let store = Store::initialize(config.clone()).unwrap();
        operations::seed(&store).unwrap();
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
        common::ready_connection(&store, &dsp, cortex::PROVIDER).unwrap();
        drop(store);
        let state = State::new(config).unwrap();
        let app = dispatch_core::server::http::router(state.clone())
            .into_make_service_with_connect_info::<std::net::SocketAddr>();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let server = Self {
            _root: root,
            origin: format!("http://127.0.0.1:{port}"),
            at: format!("http://{CODE}.localhost:{port}"),
            state,
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
        };
        (server, dsp)
    }
    /// What a request answered: its status and body.
    async fn send(&self, request: reqwest::RequestBuilder) -> (u16, Value) {
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let bytes = response.bytes().await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
    /// The platform owner, looking at `dsp`: their session written directly, so the test skips
    /// password hashing.
    async fn owner(&self, dsp: &str) -> Owner {
        let raw = crypto::token().unwrap();
        let token = crypto::sha(&raw);
        self.state
            .run(move |db| {
                let user = db
                    .platform
                    .one(
                        "SELECT id,version FROM users WHERE email='owner@dispatch.test'",
                        [],
                    )?
                    .unwrap();
                db.platform.exec(
                    "INSERT INTO sessions VALUES (?,?,?,?,?)",
                    rusqlite::params![
                        token,
                        s(&user, "id"),
                        db::n(&user, "version"),
                        db::now() + 600000,
                        db::now()
                    ],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        let cookie = format!("dispatch_session={raw}");
        let session = self
            .send(
                self.client
                    .get(format!("{}/api/session", self.origin))
                    .header("cookie", &cookie),
            )
            .await;
        let csrf = s(&session.1, "csrf").to_owned();
        let mut owner = Owner {
            cookie,
            csrf,
            view: String::new(),
        };
        let (status, view) = self
            .post(&owner, "/api/session/dsp", json!({"dspId":dsp}))
            .await;
        assert_eq!(status, 200, "{view}");
        owner.view = s(&view, "token").to_owned();
        owner
    }
    async fn get(&self, owner: &Owner, path: &str) -> Value {
        let (status, body) = self
            .send(
                self.client
                    .get(format!("{}{path}", self.origin))
                    .header("cookie", &owner.cookie)
                    .header("x-dispatch-view", &owner.view),
            )
            .await;
        assert_eq!(status, 200, "{path}: {body}");
        body
    }
    async fn post(&self, owner: &Owner, path: &str, body: Value) -> (u16, Value) {
        self.send(
            self.client
                .post(format!("{}{path}", self.origin))
                .header("origin", &self.origin)
                .header("cookie", &owner.cookie)
                .header("x-csrf-token", &owner.csrf)
                .header("x-dispatch-view", &owner.view)
                .header("content-type", "application/json")
                .body(body.to_string()),
        )
        .await
    }
    /// A phone's call to the DSP's own address, with its key once it has one.
    async fn phone(&self, key: Option<&str>, path: &str, body: Value) -> Value {
        let mut request = self
            .client
            .post(format!("{}{path}", self.origin))
            .header("host", self.at.trim_start_matches("http://"))
            .header("origin", &self.at)
            .header("content-type", "application/json")
            .body(body.to_string());
        if let Some(key) = key {
            request = request.header("authorization", format!("Bearer {key}"));
        }
        let (status, body) = self.send(request).await;
        assert_eq!(status, 200, "{path}: {body}");
        body
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
    let owner = server.owner(&dsp).await;
    // A phone paired at the DSP's own address with a code the owner made.
    let (status, pairing) = server
        .post(&owner, "/api/dsp/phones/pairing", json!({}))
        .await;
    assert_eq!(status, 200, "{pairing}");
    let phone = json!({"code":pairing["code"],"number":"(206) 555-0177","model":"Pixel 8a",
        "version":"1.0.0"});
    let paired = server.phone(None, "/api/phones/pair", phone).await;
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
        .phone(Some(&key), "/api/phones/texts", json!({"texts":[text]}))
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
