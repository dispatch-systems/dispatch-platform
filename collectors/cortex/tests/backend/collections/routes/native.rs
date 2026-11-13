use super::*;
use crate::collections::routes::collect::read_natively;
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

use tokio::time::Instant;
fn authorized() -> dispatch_core::collection::browser::Pending<'static, ()> {
    Box::pin(async { Ok(()) })
}

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
    /// The second itinerary is its driver's, but not one the collection keeps.
    UncheckedItinerary,
    /// One itinerary asks for a cooldown once, then answers normally.
    ThrottledOnce,
    /// That itinerary remains throttled after retrying.
    Throttled,
    /// That itinerary is throttled the first two times it is asked for, then answers.
    ThrottledTwice,
    /// That itinerary asks once for a cooldown longer than a read waits, then answers.
    ThrottledLong,
    /// The list shows the first route finished, its driver's session ended.
    Settled,
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
    /// The stand-in rejects a retry before the delay it told the reader has passed.
    throttled_until: Mutex<Option<Instant>>,
    /// Itineraries being answered now, and the most there were at once.
    answering: AtomicUsize,
    most_at_once: AtomicUsize,
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
        let mut level = flate2::Compression::fast();
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
                    Plays::ThrottledOnce if index == 1 && times == 1 => {
                        *self.throttled_until.lock().unwrap() =
                            Some(Instant::now() + Duration::from_secs(20));
                        return Response::builder()
                            .status(StatusCode::TOO_MANY_REQUESTS)
                            .header(header::RETRY_AFTER, "20")
                            .body(Body::empty())
                            .unwrap();
                    }
                    Plays::ThrottledOnce
                        if index == 1
                            && self
                                .throttled_until
                                .lock()
                                .unwrap()
                                .is_some_and(|until| until > Instant::now()) =>
                    {
                        return status(StatusCode::TOO_MANY_REQUESTS);
                    }
                    Plays::ThrottledLong if index == 1 && times == 1 => {
                        return Response::builder()
                            .status(StatusCode::TOO_MANY_REQUESTS)
                            .header(header::RETRY_AFTER, "180")
                            .body(Body::empty())
                            .unwrap();
                    }
                    Plays::ThrottledTwice if index == 1 && times > 2 => {
                        self.day.itineraries[index].detail.text().unwrap()
                    }
                    Plays::Throttled | Plays::ThrottledTwice if index == 1 => {
                        return Response::builder()
                            .status(StatusCode::TOO_MANY_REQUESTS)
                            .header(header::RETRY_AFTER, "0")
                            .body(Body::empty())
                            .unwrap();
                    }
                    Plays::FlakyItinerary if index == 1 && times == 1 => {
                        return status(StatusCode::SERVICE_UNAVAILABLE);
                    }
                    Plays::ItineraryDown if index == 1 => {
                        return status(StatusCode::SERVICE_UNAVAILABLE);
                    }
                    Plays::WrongItinerary if index == 1 => {
                        self.day.itineraries[0].detail.text().unwrap()
                    }
                    Plays::UncheckedItinerary if index == 1 => {
                        let mut detail: Value =
                            serde_json::from_str(&self.day.itineraries[1].detail.text().unwrap())
                                .unwrap();
                        detail["itineraryDetails"]["stops"] = json!("none");
                        detail.to_string()
                    }
                    // The finished route's itinerary comes compressed another way the second
                    // time it is asked for, and changed from the third.
                    Plays::Settled if index == 0 && times == 2 => {
                        level = flate2::Compression::best();
                        self.day.itineraries[0].detail.text().unwrap()
                    }
                    Plays::Settled if index == 0 && times > 2 => {
                        let mut detail: Value =
                            serde_json::from_str(&self.day.itineraries[0].detail.text().unwrap())
                                .unwrap();
                        detail["itineraryDetails"]["progressStatus"] = json!("BEHIND");
                        detail.to_string()
                    }
                    _ => self.day.itineraries[index].detail.text().unwrap(),
                }
            }
            None => return status(StatusCode::NOT_FOUND),
        };
        let mut gzipped = flate2::write::GzEncoder::new(Vec::new(), level);
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
        return stand.page();
    }
    // An itinerary takes a moment of the test's clock, so those asked for together overlap.
    let held = matches!(stand.plays, Plays::ThrottledTwice) && path.contains("/itineraries/");
    if held {
        let answering = stand.answering.fetch_add(1, Ordering::SeqCst) + 1;
        stand.most_at_once.fetch_max(answering, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let answer = stand.read(&path, &query, &request);
    if held {
        stand.answering.fetch_sub(1, Ordering::SeqCst);
    }
    answer
}

/// A stand-in playing `plays`, and the session a browser signed in to it with.
async fn stand_in(plays: Plays) -> (Arc<Stand>, SignedIn) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{FIXTURE}:{}", listener.local_addr().unwrap().port());
    let mut day = fixture(&request()).unwrap();
    if matches!(plays, Plays::Settled) {
        day.summaries["itinerarySummaries"][0]["driverSessionEnded"] = json!(true);
    }
    let stand = Arc::new(Stand {
        plays,
        day,
        keys: Mutex::default(),
        pages: AtomicUsize::new(0),
        reads: AtomicUsize::new(0),
        asked: Mutex::default(),
        throttled_until: Mutex::default(),
        answering: AtomicUsize::new(0),
        most_at_once: AtomicUsize::new(0),
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
    let capture = native.read(&scope, Mode::Final, &authorized).await.unwrap();
    capture.validate(&request()).unwrap();
    assert_eq!(capture.summaries, stand.day.summaries);
    assert_eq!(capture.route_summaries, stand.day.route_summaries);
    let read = |capture: &Capture| {
        capture
            .itineraries
            .iter()
            .map(|i| {
                (
                    i.id.clone(),
                    i.transporter_id.clone(),
                    i.detail.text().unwrap(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(read(&capture), read(&stand.day));
    // The itinerary that failed was asked for once more.
    assert_eq!(stand.asked("itinerary-2"), 2);
    // The key is kept for the next read.
    native.read(&scope, Mode::Final, &authorized).await.unwrap();
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
            .read(&stand.day.scope, Mode::Final, &authorized)
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

async fn unprompted(_: i64, _: String) -> Result<()> {
    Ok(())
}

#[tokio::test]
async fn a_day_read_natively_is_kept_once_checked_and_its_reader_reads_the_next() {
    let read = |plays| async move {
        let (stand, session) = stand_in(plays).await;
        let scope = stand.day.scope.clone();
        let read = read_natively(&session, &request(), &scope, &unprompted, &authorized)
            .await
            .unwrap();
        (stand, read)
    };
    // A whole day is kept, and the reader that read it reads the next with the key it found.
    let (stand, day) = read(Plays::FlakyItinerary).await;
    let (mut reader, day) = day.unwrap();
    day.validate(&request()).unwrap();
    reader
        .read(&stand.day.scope, Mode::Final, &authorized)
        .await
        .unwrap();
    assert_eq!(stand.pages.load(Ordering::SeqCst), 1);
    // Otherwise it says why: as the reader names it, or a day read whole that fails the checks.
    for (plays, why) in [
        (Plays::NoKey, "routes_native_no_key"),
        (Plays::UncheckedItinerary, "routes_native_invalid"),
    ] {
        let (_, day) = read(plays).await;
        assert_eq!(day.err(), Some(why), "{plays:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_throttled_itinerary_is_retried_without_reading_successful_ones_again() {
    let (stand, session) = stand_in(Plays::ThrottledOnce).await;
    let mut reader = Native::new(&session).unwrap();
    let capture = reader
        .read(&stand.day.scope, Mode::Final, &authorized)
        .await
        .unwrap();
    capture.validate(&request()).unwrap();
    assert_eq!(stand.asked("itinerary-1"), 1);
    assert_eq!(stand.asked("itinerary-2"), 2);
    assert_eq!(stand.pages.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn persistent_throttling_does_not_request_a_browser_fallback() {
    let (stand, session) = stand_in(Plays::Throttled).await;
    let result = read_natively(
        &session,
        &request(),
        &stand.day.scope,
        &unprompted,
        &authorized,
    )
    .await;
    assert_eq!(
        result.err().map(|error| error.code).as_deref(),
        Some("cortex_throttled")
    );
    assert_eq!(stand.asked("itinerary-1"), 1);
    // It is asked for round after round, and the read ends once they are spent.
    assert_eq!(stand.asked("itinerary-2"), 12);
    assert_eq!(stand.pages.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_day_throttled_round_after_round_is_read_whole_fewer_at_a_time_until_reads_run_clean() {
    let (stand, session) = stand_in(Plays::ThrottledTwice).await;
    let mut reader = Native::new(&session).unwrap();
    let scope = stand.day.scope.clone();
    let capture = reader.read(&scope, Mode::Final, &authorized).await.unwrap();
    capture.validate(&request()).unwrap();
    // What was read is kept through each round; only the throttled itinerary is asked again.
    assert_eq!(stand.asked("itinerary-1"), 1);
    assert_eq!(stand.asked("itinerary-2"), 3);
    // Each throttled round narrowed the next, and the read's record says so.
    assert_eq!(reader.counts(), [("throttled", 2), ("downloads", 1)]);
    // Another reader of the account, as a job's beside a live read's, is admitted as narrowly.
    stand.most_at_once.store(0, Ordering::SeqCst);
    let mut other = Native::new(&session).unwrap();
    other.read(&scope, Mode::Final, &authorized).await.unwrap();
    assert_eq!(other.counts(), [("downloads", 1)]);
    assert_eq!(stand.most_at_once.load(Ordering::SeqCst), 1);
    // Reads that go on meeting no throttle widen it again, a step at a time.
    reader.read(&scope, Mode::Final, &authorized).await.unwrap();
    assert_eq!(reader.counts(), [("downloads", 1)]);
    reader.read(&scope, Mode::Final, &authorized).await.unwrap();
    assert_eq!(reader.counts(), [("downloads", 2)]);
}

#[tokio::test(start_paused = true)]
async fn a_read_ended_by_a_long_cooldown_narrows_the_read_after_it() {
    let (stand, session) = stand_in(Plays::ThrottledLong).await;
    let mut reader = Native::new(&session).unwrap();
    let ended = reader
        .read(&stand.day.scope, Mode::Final, &authorized)
        .await;
    assert!(matches!(ended, Err(Failure::Throttled(_))));
    tokio::time::advance(Duration::from_secs(180)).await;
    reader
        .read(&stand.day.scope, Mode::Final, &authorized)
        .await
        .unwrap();
    assert_eq!(reader.counts(), [("downloads", 2)]);
}

#[tokio::test(start_paused = true)]
async fn authority_is_rechecked_after_cooldown_before_another_provider_request() {
    let (stand, session) = stand_in(Plays::ThrottledOnce).await;
    let mut native = Native::new(&session).unwrap();
    // The read's authority goes as Cortex throttles it.
    let check = {
        let stand = stand.clone();
        move || -> dispatch_core::collection::browser::Pending<'static, ()> {
            let current = stand.throttled_until.lock().unwrap().is_none();
            Box::pin(async move { dispatch_core::ensure(current, "connection_changed", 409) })
        }
    };
    let result = native.read(&stand.day.scope, Mode::Final, &check).await;
    assert!(
        matches!(result, Err(Failure::Stopped(error)) if error.is(dispatch_core::Code::ConnectionChanged))
    );
    // Each itinerary was asked for once, and the throttled one not again after the cooldown.
    assert_eq!(stand.asked("itinerary-1"), 1);
    assert_eq!(stand.asked("itinerary-2"), 1);
}

#[tokio::test]
async fn a_request_that_waited_at_the_gate_asks_its_authority_again_before_it_is_sent() {
    let (stand, session) = stand_in(Plays::FlakyItinerary).await;
    let scope = stand.day.scope.clone();
    // Another reader of the account holds every place its gate has.
    let gate = crate::connection::gate::of(&session.origin, &scope.provider);
    let mut held = Vec::new();
    for _ in 0..gate.width() {
        held.push(gate.admit().await);
    }
    // The read's authority goes while its first request waits for a place.
    let checks = Arc::new(AtomicUsize::new(0));
    let check = {
        let checks = checks.clone();
        move || -> dispatch_core::collection::browser::Pending<'static, ()> {
            let current = checks.fetch_add(1, Ordering::SeqCst) == 0;
            Box::pin(async move { dispatch_core::ensure(current, "connection_changed", 409) })
        }
    };
    let mut native = Native::new(&session).unwrap();
    let read = native.read(&scope, Mode::Final, &check);
    tokio::pin!(read);
    tokio::select! {
        biased;
        _ = &mut read => panic!("the read was admitted while every place was held"),
        _ = std::future::ready(()) => {}
    }
    drop(held);
    assert!(
        matches!(read.await, Err(Failure::Stopped(error)) if error.is(dispatch_core::Code::ConnectionChanged))
    );
    // Nothing was asked of Cortex once it had gone, not even the list's page for its key.
    assert_eq!(checks.load(Ordering::SeqCst), 2);
    assert_eq!(stand.pages.load(Ordering::SeqCst), 0);
    assert_eq!(stand.asked("itinerary-1"), 0);
}

#[tokio::test]
async fn a_snapshot_asks_again_only_for_routes_something_may_be_happening_on() {
    let (stand, session) = stand_in(Plays::Settled).await;
    let scope = stand.day.scope.clone();
    let mut reader = Native::new(&session).unwrap();
    let asked = || (stand.asked("itinerary-1"), stand.asked("itinerary-2"));
    // Its first read of the day asks for every itinerary.
    let first = reader
        .read(&scope, Mode::Snapshot, &authorized)
        .await
        .unwrap();
    assert_eq!(asked(), (1, 1));
    assert!(reader.counts().is_empty());
    // The next keeps the finished route's, which the list says the same of, and is whole.
    let next = reader
        .read(&scope, Mode::Snapshot, &authorized)
        .await
        .unwrap();
    assert_eq!(asked(), (1, 2));
    assert_eq!(reader.counts(), [("reused", 1)]);
    assert_eq!(next.itineraries.len(), 2);
    let text = |capture: &Capture| capture.itineraries[0].detail.text().unwrap();
    assert_eq!(text(&next), text(&first));
    // One read in six asks for them all again, and found nothing the list had not shown.
    for _ in 0..4 {
        reader
            .read(&scope, Mode::Snapshot, &authorized)
            .await
            .unwrap();
    }
    assert_eq!(asked(), (1, 6));
    reader
        .read(&scope, Mode::Snapshot, &authorized)
        .await
        .unwrap();
    assert_eq!(asked(), (2, 7));
    // Cortex compressed the finished route's another way that time, which is no change.
    assert!(reader.counts().is_empty());
    // The next such read finds it changed though the list said the same, and counts it.
    for _ in 0..5 {
        reader
            .read(&scope, Mode::Snapshot, &authorized)
            .await
            .unwrap();
    }
    assert_eq!(asked(), (2, 12));
    reader
        .read(&scope, Mode::Snapshot, &authorized)
        .await
        .unwrap();
    assert_eq!(asked(), (3, 13));
    assert_eq!(reader.counts(), [("unseen", 1)]);
    // A completed day's read reuses nothing.
    reader.read(&scope, Mode::Final, &authorized).await.unwrap();
    assert_eq!(asked(), (4, 14));
}

#[tokio::test]
async fn a_read_of_some_itineraries_asks_its_authority_again_before_each_request() {
    // The read's authority goes once Cortex has answered with the list's page, the list, or
    // the first itinerary: nothing more is asked of it after any. The account's gate admits
    // one request at a time here, so the second itinerary would follow the first.
    let cases: [(fn(&Stand) -> bool, usize, usize, usize); 3] = [
        (|stand| stand.pages.load(Ordering::SeqCst) > 0, 1, 0, 0),
        (|stand| stand.reads.load(Ordering::SeqCst) > 0, 1, 1, 0),
        (|stand| stand.asked("itinerary-1") > 0, 1, 2, 1),
    ];
    for (index, (gone, pages, reads, first)) in cases.into_iter().enumerate() {
        let (stand, session) = stand_in(Plays::Settled).await;
        let gate = crate::connection::gate::of(&session.origin, &stand.day.scope.provider);
        for _ in 0..2 {
            gate.throttled(Duration::ZERO);
            gate.narrow();
        }
        let check = {
            let stand = stand.clone();
            move || -> dispatch_core::collection::browser::Pending<'static, ()> {
                let current = !gone(&stand);
                Box::pin(async move { dispatch_core::ensure(current, "connection_changed", 409) })
            }
        };
        let every = |summaries: &Value| Ok(crate::routes::listed(summaries, &stand.day.scope));
        let read = Native::new(&session)
            .unwrap()
            .read_some(&stand.day.scope, &check, &every)
            .await;
        assert!(matches!(read, Err(Failure::Stopped(_))), "{index}");
        assert_eq!(stand.pages.load(Ordering::SeqCst), pages, "{index}");
        assert_eq!(stand.reads.load(Ordering::SeqCst), reads, "{index}");
        assert_eq!(stand.asked("itinerary-1"), first, "{index}");
        assert_eq!(stand.asked("itinerary-2"), 0, "{index}");
    }
}
