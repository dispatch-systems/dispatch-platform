use super::*;
#[test]
fn rows_come_as_objects_or_json_strings_for_the_named_dataset() {
    let value = json!({"tableData":{"other":{"rows":[{"week":1}]},
        "dsp_weekly_cdf":{"rows":["{\"week\":38}",{"week":39}]}}});
    assert_eq!(
        rows("dsp_weekly_cdf", &value).unwrap(),
        vec![json!({"week":38}), json!({"week":39})]
    );
    assert!(
        rows("dsp_weekly_cdf", &json!({"tableData":{}}))
            .unwrap()
            .is_empty()
    );
    assert!(rows("x", &json!({"tableData":{"x":{"rows":["not json"]}}})).is_err());
    assert!(rows("x", &json!({"tableData":{"x":{"rows":[1]}}})).is_err());
    assert!(rows("x", &json!({"rows":[]})).is_err());
}
#[test]
fn data_requests_name_the_api_and_this_dsp_on_the_pages_origin_only() {
    let origin = "https://logistics.amazon.com";
    let (base, dsp, station) = data_request(
        "https://logistics.amazon.com/performance/api/v1/getData?dataSetId=x&dsp=NLOG&station=TST1",
        origin,
    )
    .unwrap();
    assert_eq!(base, "https://logistics.amazon.com/performance/api/v1");
    assert_eq!((dsp.as_str(), station.as_str()), ("NLOG", "TST1"));
    assert!(
        data_request(
            "https://evil.example/performance/api/v1/getData?dsp=NLOG",
            origin
        )
        .is_err()
    );
    assert!(
        data_request(
            "https://logistics.amazon.com/other/api/v1/getData?dsp=NLOG",
            origin
        )
        .is_err()
    );
    assert!(
        data_request(
            "https://logistics.amazon.com/performance/api/v1/getData",
            origin
        )
        .is_err()
    );
}
#[test]
fn addresses_follow_the_pages_parameter_order() {
    let api = Api {
        base: "https://logistics.amazon.com/performance/api/v1".into(),
        dsp: "NLOG".into(),
    };
    let dataset = crate::weekly_scorecard::dataset("da_dsp_station_weekly_performance").unwrap();
    assert_eq!(
        api.address(dataset, "TST1", "2026-W38", "2026-W38"),
        concat!(
            "https://logistics.amazon.com/performance/api/v1/getData?dataSetId=",
            "da_dsp_station_weekly_performance&dsp=NLOG&from=2026-W38&program=AMZL",
            "&station=TST1&timeFrame=Weekly&to=2026-W38"
        )
    );
}

#[test]
fn daily_thresholds_omit_station_while_day_rows_keep_their_exact_date() {
    let api = Api {
        base: "https://logistics.amazon.com/performance/api/v1".into(),
        dsp: "NLOG".into(),
    };
    let threshold = crate::daily_performance::dataset("driver_thresholds").unwrap();
    let url = api.address(threshold, "TST1", "2026-10-03", "2026-10-03");
    assert!(!url.contains("station="));
    assert!(url.contains("program=AMZL"));
    assert!(url.contains("timeFrame=Daily"));
    let daily = crate::daily_performance::dataset("driver_quality").unwrap();
    let url = api.address(daily, "TST1", "2026-10-03", "2026-10-03");
    assert!(url.contains("from=2026-10-03"));
    assert!(url.contains("to=2026-10-03"));
    assert!(url.contains("station=TST1"));
}

// Every read is asked once, by whichever lane is free. Once one fails the whole read has: no
// lane takes another, those already asked drain, and the failure is what the lanes answer.
#[tokio::test]
async fn once_a_read_fails_no_lane_takes_another() {
    let lanes = in_lanes(10, 3, |index| async move { Ok(index) }).await;
    let mut read: Vec<usize> = lanes
        .into_iter()
        .flat_map(|lane| lane.unwrap())
        .map(|(index, value)| {
            assert_eq!(index, value);
            index
        })
        .collect();
    read.sort_unstable();
    assert_eq!(read, (0..10).collect::<Vec<_>>());

    let (asked, finished) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let lanes = in_lanes(100, 4, |index| {
        let (asked, finished) = (&asked, &finished);
        async move {
            asked.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            finished.fetch_add(1, Ordering::SeqCst);
            if index == 0 {
                return Err(Error::new("performance_api_unreadable", 502));
            }
            Ok(index)
        }
    })
    .await;
    let asked = asked.load(Ordering::SeqCst);
    assert!(asked <= 4, "{asked} reads asked after the first failed");
    assert_eq!(finished.load(Ordering::SeqCst), asked);
    let failed: Vec<&str> = lanes
        .iter()
        .filter_map(|lane| lane.as_ref().err())
        .map(|error| error.code.as_str())
        .collect();
    assert_eq!(failed, ["performance_api_unreadable"]);
}

// A reply longer than the reader takes fails as the source too large, as too many rows do,
// whatever the other reads beside it met; any other refusal is unreadable.
#[test]
fn a_reply_too_long_is_the_source_too_large() {
    let metrics = Recorder::new(&json!({}));
    let error = refused(Refusal::Unreadable("http_too_large"), &metrics);
    assert!(error.is(cortex::codes::PERFORMANCE_SOURCE_TOO_LARGE));
    let error = refused(Refusal::Unreadable("http_status"), &metrics);
    assert!(error.is(cortex::codes::PERFORMANCE_API_UNREADABLE));
}
/// Cortex's data API for a read's datasets, each answered with a row naming it. With
/// `held`, each answer takes a moment of the test's clock, so those asked together
/// overlap. `throttles` asks once for a cooldown of so many seconds.
struct Stand {
    held: bool,
    throttles: Option<(&'static str, u64)>,
    asked: std::sync::Mutex<std::collections::HashMap<String, usize>>,
    /// The stand-in tells when a throttled dataset is asked again before its delay passed.
    throttled_until: std::sync::Mutex<Option<tokio::time::Instant>>,
    early: AtomicBool,
    answering: AtomicUsize,
    most_at_once: AtomicUsize,
}
impl Stand {
    fn asked(&self, dataset: &str) -> usize {
        self.asked
            .lock()
            .unwrap()
            .get(dataset)
            .copied()
            .unwrap_or(0)
    }
}
async fn answer(
    axum::extract::State(stand): axum::extract::State<std::sync::Arc<Stand>>,
    request: axum::extract::Request,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let query = request.uri().query().unwrap_or("").as_bytes();
    let (_, dataset) = url::form_urlencoded::parse(query)
        .into_owned()
        .find(|(name, _)| name == "dataSetId")
        .unwrap();
    let asked = {
        let mut asked = stand.asked.lock().unwrap();
        let count = asked.entry(dataset.clone()).or_default();
        *count += 1;
        *count
    };
    let answering = stand.answering.fetch_add(1, Ordering::SeqCst) + 1;
    stand.most_at_once.fetch_max(answering, Ordering::SeqCst);
    if stand.held {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    stand.answering.fetch_sub(1, Ordering::SeqCst);
    if let Some((throttled, seconds)) = stand.throttles
        && throttled == dataset
    {
        let mut until = stand.throttled_until.lock().unwrap();
        if asked == 1 {
            *until = Some(tokio::time::Instant::now() + Duration::from_secs(seconds));
            return (
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                [("retry-after", seconds.to_string())],
            )
                .into_response();
        }
        if until.is_some_and(|until| tokio::time::Instant::now() < until) {
            stand.early.store(true, Ordering::SeqCst);
        }
    }
    axum::Json(json!({"tableData":{dataset.clone():{"rows":[{"dataset":dataset}]}}}))
        .into_response()
}
const DATASETS: [&str; 14] = [
    "d00", "d01", "d02", "d03", "d04", "d05", "d06", "d07", "d08", "d09", "d10", "d11", "d12",
    "d13",
];
/// An authority that never stops a read.
fn allowed() -> crate::connection::gate::Authority {
    Box::new(|| Box::pin(async { Ok(()) }))
}
/// Every dataset of a week read as `read_all` reads them, fourteen lanes at once, at the
/// gate of a stand-in playing `held` and `throttles`, for as long as `authority` allows: the
/// rows each read, in order, or the first error a lane met.
async fn read_week(
    held: bool,
    throttles: Option<(&'static str, u64)>,
    authority: crate::connection::gate::Authority,
) -> (
    std::sync::Arc<Stand>,
    std::sync::Arc<crate::connection::gate::Gate>,
    Result<Vec<Vec<Value>>>,
) {
    use dispatch_core::collection::browser::http::{Cookie, FIXTURE, SignedIn};
    let stand = std::sync::Arc::new(Stand {
        held,
        throttles,
        asked: Default::default(),
        throttled_until: Default::default(),
        early: AtomicBool::new(false),
        answering: AtomicUsize::new(0),
        most_at_once: AtomicUsize::new(0),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{FIXTURE}:{}", listener.local_addr().unwrap().port());
    let app = axum::Router::new()
        .route("/performance/api/v1/getData", axum::routing::get(answer))
        .with_state(stand.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let session = SignedIn {
        origin: origin.clone(),
        cookies: vec![Cookie {
            name: "session-token".into(),
            value: "fixture".into(),
            domain: FIXTURE.into(),
            path: "/".into(),
            secure: false,
        }],
        user_agent: "Fixture".into(),
    };
    let http = Http::with_session(&session, &cortex::HOSTS).unwrap();
    let metrics = Recorder::new(&json!({}));
    let turn = Turn::of(&origin, "company-1", authority);
    let referer = format!("{origin}/performance");
    let (http, turn, referer, metrics) = (&http, &turn, &referer, &metrics);
    let origin = &origin;
    let lanes = in_lanes(DATASETS.len(), DATASETS.len(), |index| async move {
        let url = format!(
            "{origin}/performance/api/v1/getData?dataSetId={}",
            DATASETS[index]
        );
        read_rows(http, turn, &url, referer, DATASETS[index], metrics).await
    })
    .await;
    let mut read: Vec<Option<Vec<Value>>> = vec![None; DATASETS.len()];
    let mut failed = None;
    for lane in lanes {
        match lane {
            Ok(lane) => lane
                .into_iter()
                .for_each(|(index, rows)| read[index] = Some(rows)),
            Err(error) => failed = failed.or(Some(error)),
        }
    }
    let read = match failed {
        Some(error) => Err(error),
        None => Ok(read.into_iter().map(Option::unwrap).collect()),
    };
    (
        stand,
        crate::connection::gate::of(origin, "company-1"),
        read,
    )
}

// A week's datasets are asked for no more at once than the account's gate admits, however
// many lanes read them. One Cortex throttles cools the account: it is asked again only once
// the delay it named has passed, the gate narrows, and the read ends with every dataset's
// rows, each other dataset asked once.
#[cfg_attr(target_os = "macos", ignore = "paused clock races I/O; CI runs it")]
#[tokio::test(start_paused = true)]
async fn a_throttled_dataset_is_read_again_once_the_account_cooled_and_the_week_comes_whole() {
    let (stand, gate, read) = read_week(true, Some(("d05", 7)), allowed()).await;
    let read = read.unwrap();
    assert_eq!(
        read,
        DATASETS
            .iter()
            .map(|dataset| vec![json!({ "dataset": dataset })])
            .collect::<Vec<_>>()
    );
    assert!(stand.most_at_once.load(Ordering::SeqCst) <= 4);
    assert_eq!(stand.asked("d05"), 2);
    assert!(!stand.early.load(Ordering::SeqCst));
    for dataset in DATASETS.iter().filter(|dataset| **dataset != "d05") {
        assert_eq!(stand.asked(dataset), 1, "{dataset}");
    }
    assert_eq!(gate.width(), 2);
    assert!(gate.cooldown().is_none());
}

// A cooldown longer than a read waits fails it at once as throttled, before the cooldown
// has passed, and leaves the cooldown with the gate for the job's next attempt.
#[tokio::test]
async fn a_cooldown_longer_than_a_read_waits_fails_it_promptly_as_throttled() {
    let (stand, gate, read) = read_week(false, Some(("d00", 600)), allowed()).await;
    assert_eq!(read.unwrap_err().code, "cortex_throttled");
    assert_eq!(stand.asked("d00"), 1);
    assert!(gate.cooling());
}

// A read whose job stopped asks Cortex for nothing more: each request asks its authority
// once it has its place, before it is sent.
#[tokio::test]
async fn a_read_no_longer_its_jobs_sends_nothing() {
    let (stand, gate, read) = read_week(
        false,
        None,
        Box::new(|| Box::pin(async { Err(Error::new("job_cancelled", 409)) })),
    )
    .await;
    assert_eq!(read.unwrap_err().code, "job_cancelled");
    for dataset in DATASETS {
        assert_eq!(stand.asked(dataset), 0, "{dataset}");
    }
    assert!(!gate.cooling());
}
