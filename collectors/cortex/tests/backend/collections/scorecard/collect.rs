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
        company_id: "company".into(),
    };
    let dataset =
        crate::collectors::cortex::scorecard::dataset("da_dsp_station_weekly_performance").unwrap();
    assert_eq!(
        api.address(dataset, "TST1", "2026-W38", "2026-W38"),
        concat!(
            "https://logistics.amazon.com/performance/api/v1/getData?dataSetId=",
            "da_dsp_station_weekly_performance&dsp=NLOG&from=2026-W38&program=AMZL",
            "&station=TST1&timeFrame=Weekly&to=2026-W38"
        )
    );
}
