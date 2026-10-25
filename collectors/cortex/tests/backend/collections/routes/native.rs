use super::*;
use crate::routes::{Request, fixture};
use axum::{
    Router,
    body::Body,
    extract::{Request as Asked, State},
    http::{Response, StatusCode, header},
};
use dispatch_core::collection::browser::http::{Cookie, FIXTURE};
use serde_json::json;
use std::{
    collections::HashMap,
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

const AREA: &str = "area-1";
const COMPANY: &str = "company-1";

fn request() -> Request {
    Request {
        collection: Collection::Routes,
        mode: Mode::Final,
        date: "2026-09-25".into(),
        station: "TST1".into(),
        timezone: "America/Los_Angeles".into(),
        dsp_name: "Fixture Logistics".into(),
        dsp_abbreviation: "FXTR".into(),
        service_area_id: Some(AREA.into()),
        provider: Some(COMPANY.into()),
    }
}

/// What the stand-in does besides answering as Cortex does.
#[derive(Clone, Copy, Debug)]
enum Plays {
    /// The second itinerary fails once.
    FlakyItinerary,
    /// Every read signed with the first key the page handed out is refused.
    RefusesFirstKey,
    RefusesEveryKey,
    /// The first read redirects to sign-in, as Cortex did once for a whole poll.
    RedirectsOnce,
    /// Every read redirects to sign-in, and so does the page after its first load.
    SignsOut,
    /// The console's page, without a key.
    NoKey,
    /// A sign-in page served in the console's place.
    SignInPage,
    /// The list answered is not one.
    MalformedList,
    /// The second itinerary never answers.
    ItineraryDown,
    /// The second itinerary is answered with the first's.
    WrongItinerary,
}

/// Cortex for one session: its list page hands out a key, and every read must be signed
/// with one as Cortex checks it, carry the session's cookie and ask for gzip.
struct Stand {
    plays: Plays,
    day: Capture,
    /// Each key the page handed out, by the session it names, in order.
    keys: Mutex<Vec<(String, String)>>,
    pages: AtomicUsize,
    reads: AtomicUsize,
    /// How many times each itinerary was asked for.
    asked: Mutex<HashMap<String, usize>>,
}
impl Stand {
    fn signed(&self, path: &str, query: &str, request: &Asked) -> Option<usize> {
        let header = |name: &str| {
            request
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
        };
        let (session, stamp) = (header("x-cortex-session"), header("x-cortex-timestamp"));
        let keys = self.keys.lock().unwrap();
        let key = keys.iter().position(|(s, _)| s == session)?;
        let mut pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        pairs.sort();
        let sorted = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(pairs)
            .finish();
        let canonical = [
            "GET",
            path,
            &sorted,
            &format!("x-cortex-session:{session}"),
            &format!("x-cortex-timestamp:{stamp}"),
            "",
            "x-cortex-session;x-cortex-timestamp",
        ]
        .join("\n");
        let mut mac =
            <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(keys[key].1.as_bytes()).unwrap();
        mac.update(canonical.as_bytes());
        let fresh = stamp
            .parse::<i64>()
            .is_ok_and(|t| (now() - t).abs() < 15 * 60 * 1000);
        (fresh
            && header("user-ref") == "cortex-webapp-user"
            && STANDARD.encode(mac.finalize().into_bytes()) == header("x-cortex-hmac-signature"))
        .then_some(key)
    }
    fn page(&self) -> Response<Body> {
        let loads = self.pages.fetch_add(1, Ordering::SeqCst) + 1;
        let html = |title: &str, body: String| {
            Response::builder()
                .header(header::CONTENT_TYPE, "text/html;charset=UTF-8")
                .body(Body::from(format!(
                    "<html><head><title>{title}</title></head><body>{body}</body></html>"
                )))
                .unwrap()
        };
        match self.plays {
            Plays::SignsOut if loads > 1 => redirect(),
            Plays::NoKey => html("DSP Console", "<div id=\"root\"></div>".into()),
            Plays::SignInPage => html("Amazon Sign-In", "<form></form>".into()),
            _ => {
                let (session, secret) =
                    (format!("session-{loads}"), format!("fixture-key-{loads}"));
                self.keys
                    .lock()
                    .unwrap()
                    .push((session.clone(), secret.clone()));
                let state =
                    json!({"hmacSession":{"sessionKey":secret,"sessionId":session,"enabled":true}});
                html(
                    "DSP Console",
                    format!(
                        "<script type=\"a-state\" data-a-state=\"{{&quot;key&quot;:&quot;hmacSession&quot;}}\">{state}</script>"
                    ),
                )
            }
        }
    }
    fn read(&self, path: &str, query: &str, request: &Asked) -> Response<Body> {
        let first = self.reads.fetch_add(1, Ordering::SeqCst) == 0;
        if matches!(self.plays, Plays::SignsOut)
            || matches!(self.plays, Plays::RedirectsOnce) && first
        {
            return redirect();
        }
        let Some(key) = self.signed(path, query, request) else {
            return status(StatusCode::FORBIDDEN);
        };
        if matches!(self.plays, Plays::RefusesEveryKey)
            || matches!(self.plays, Plays::RefusesFirstKey) && key == 0
        {
            return status(StatusCode::FORBIDDEN);
        }
        let gzip = request
            .headers()
            .get(header::ACCEPT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("gzip"));
        if !gzip {
            return status(StatusCode::NOT_ACCEPTABLE);
        }
        let query: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        let asks = |wanted: &[(&str, &str)]| {
            query.len() == wanted.len()
                && wanted
                    .iter()
                    .all(|(n, v)| query.get(*n).is_some_and(|q| q == v))
        };
        let day = [
            ("historicalDay", "true"),
            ("localDate", "2026-09-25"),
            ("serviceAreaId", AREA),
        ];
        let body = match path.strip_prefix("/operations/execution/api/") {
            Some("summaries") if asks(&day) => match self.plays {
                Plays::MalformedList => "[]".into(),
                _ => self.day.summaries.to_string(),
            },
            Some("route-summaries")
                if asks(&[day.as_slice(), &[("statsFromSummaries", "true")]].concat()) =>
            {
                self.day.route_summaries.to_string()
            }
            Some(route) => {
                let Some(id) = route.strip_prefix("itineraries/") else {
                    return status(StatusCode::NOT_FOUND);
                };
                let wanted = [
                    ("documentType", "Itinerary"),
                    ("historicalDay", "true"),
                    ("itineraryId", id),
                    ("serviceAreaId", AREA),
                ];
                let Some(index) = self.day.itineraries.iter().position(|i| i.id == id) else {
                    return status(StatusCode::NOT_FOUND);
                };
                if !asks(&wanted) {
                    return status(StatusCode::BAD_REQUEST);
                }
                let times = {
                    let mut asked = self.asked.lock().unwrap();
                    let times = asked.entry(id.to_owned()).or_default();
                    *times += 1;
                    *times
                };
                match self.plays {
                    Plays::FlakyItinerary if index == 1 && times == 1 => {
                        return status(StatusCode::SERVICE_UNAVAILABLE);
                    }
                    Plays::ItineraryDown if index == 1 => {
                        return status(StatusCode::SERVICE_UNAVAILABLE);
                    }
                    Plays::WrongItinerary if index == 1 => self.day.itineraries[0].detail.clone(),
                    _ => self.day.itineraries[index].detail.clone(),
                }
            }
            None => return status(StatusCode::NOT_FOUND),
        };
        let mut gzipped = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzipped.write_all(body.as_bytes()).unwrap();
        Response::builder()
            .header(header::CONTENT_TYPE, "application/json;charset=UTF-8")
            .header(header::CONTENT_ENCODING, "gzip")
            .body(Body::from(gzipped.finish().unwrap()))
            .unwrap()
    }
    fn asked(&self, id: &str) -> usize {
        self.asked.lock().unwrap().get(id).copied().unwrap_or(0)
    }
}
fn status(code: StatusCode) -> Response<Body> {
    Response::builder()
        .status(code)
        .body(Body::empty())
        .unwrap()
}
fn redirect() -> Response<Body> {
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "/ap/signin")
        .body(Body::empty())
        .unwrap()
}
async fn answer(State(stand): State<Arc<Stand>>, request: Asked) -> Response<Body> {
    let signed_in = request
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split("; ").any(|c| c == "session-token=fixture"));
    if !signed_in {
        return redirect();
    }
    let (path, query) = (
        request.uri().path().to_owned(),
        request.uri().query().unwrap_or("").to_owned(),
    );
    if path == "/operations/execution/itineraries" {
        stand.page()
    } else {
        stand.read(&path, &query, &request)
    }
}

/// A stand-in playing `plays`, and the session a browser signed in to it with.
async fn stand_in(plays: Plays) -> (Arc<Stand>, SignedIn) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{FIXTURE}:{}", listener.local_addr().unwrap().port());
    let stand = Arc::new(Stand {
        plays,
        day: fixture(&request()).unwrap(),
        keys: Mutex::default(),
        pages: AtomicUsize::new(0),
        reads: AtomicUsize::new(0),
        asked: Mutex::default(),
    });
    let app = Router::new().fallback(answer).with_state(stand.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let session = SignedIn {
        origin,
        cookies: vec![Cookie {
            name: "session-token".into(),
            value: "fixture".into(),
            domain: FIXTURE.into(),
            path: "/".into(),
            secure: false,
        }],
        user_agent: "Fixture".into(),
    };
    (stand, session)
}

#[tokio::test]
async fn a_day_is_read_whole_with_the_key_its_list_page_carries() {
    let (stand, session) = stand_in(Plays::FlakyItinerary).await;
    let mut native = Native::new(&session).unwrap();
    let scope = stand.day.scope.clone();
    let capture = native.read(&scope, Mode::Final).await.unwrap();
    capture.validate(&request()).unwrap();
    assert_eq!(capture.summaries, stand.day.summaries);
    assert_eq!(capture.route_summaries, stand.day.route_summaries);
    let read = |capture: &Capture| {
        capture
            .itineraries
            .iter()
            .map(|i| (i.id.clone(), i.transporter_id.clone(), i.detail.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(read(&capture), read(&stand.day));
    // The itinerary that failed was asked for once more.
    assert_eq!(stand.asked("itinerary-2"), 2);
    // The key is kept for the next read.
    native.read(&scope, Mode::Final).await.unwrap();
    assert_eq!(stand.pages.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_day_not_read_whole_says_why() {
    let cases = [
        // A refused key is asked of the page again once; a redirect for a moment is asked
        // again.
        (Plays::RefusesFirstKey, "read", 2),
        (Plays::RedirectsOnce, "read", 1),
        (Plays::RefusesEveryKey, "routes_native_refused", 2),
        // A redirect that lasts has the page asked again, which sends the session to sign in.
        (Plays::SignsOut, "routes_native_signed_out", 2),
        (Plays::NoKey, "routes_native_no_key", 1),
        (Plays::SignInPage, "routes_native_signed_out", 1),
        // Never a day of no itineraries from a list that is not one.
        (Plays::MalformedList, "cortex_content_incomplete", 1),
        (Plays::ItineraryDown, "provider_unavailable", 1),
        (Plays::WrongItinerary, "routes_scope_mismatch", 1),
    ];
    let outcomes = futures_util::future::join_all(cases.map(|(plays, _, _)| async move {
        let (stand, session) = stand_in(plays).await;
        let outcome = match Native::new(&session)
            .unwrap()
            .read(&stand.day.scope, Mode::Final)
            .await
        {
            Ok(capture) => {
                capture.validate(&request()).unwrap();
                "read".to_owned()
            }
            Err(Failure::Unavailable(error)) => error.code,
            Err(failure) => failure.label().to_owned(),
        };
        (plays, outcome, stand.pages.load(Ordering::SeqCst))
    }))
    .await;
    for ((plays, outcome, pages), (_, expected, expected_pages)) in outcomes.into_iter().zip(cases)
    {
        assert_eq!(
            (outcome.as_str(), pages),
            (expected, expected_pages),
            "{plays:?}"
        );
    }
}
