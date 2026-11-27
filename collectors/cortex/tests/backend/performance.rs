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
