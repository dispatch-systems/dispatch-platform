//! What a scorecard collection reads from Cortex's performance API: the datasets behind
//! each scorecard page, one week at a time, every row as Amazon sent it.
use super::discovery::{CollectionRequest as ScopeRequest, Discovery, Scope};
use dispatch_core::{
    Error, Result,
    db::now,
    ensure,
    foundation::{
        validate::token,
        weeks::{parse_week, week_days},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const JOB_KIND: &str = "cortex.scorecard.collect";
/// Rows one dataset may hold, well above any week seen.
pub const MAX_ROWS: usize = 50_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeFrame {
    Weekly,
    Daily,
}
impl TimeFrame {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Weekly => "Weekly",
            Self::Daily => "Daily",
        }
    }
}
/// One dataset of the performance API and the table that holds its rows.
pub struct Dataset {
    pub id: &'static str,
    pub table: &'static str,
    pub time_frame: TimeFrame,
    /// A `program` the page sends with the request.
    pub program: Option<&'static str>,
    /// The row field that says whether the row counts against the scorecard.
    pub impact: Option<&'static str>,
}
/// Every dataset a week's collection reads. The first six are the scorecard pages'
/// tables; the rest are the DSP's own weekly numbers.
pub const DATASETS: &[Dataset] = &[
    Dataset {
        id: "da_dsp_station_weekly_performance",
        table: "driver_scorecards",
        time_frame: TimeFrame::Weekly,
        program: Some("AMZL"),
        impact: None,
    },
    Dataset {
        id: "da_dsp_weekly_rts_deep_dive",
        table: "returns_to_station",
        time_frame: TimeFrame::Weekly,
        program: None,
        impact: Some("impacting_dcr"),
    },
    Dataset {
        id: "da_dsp_station_daily_dsb_dnr_tba",
        table: "delivery_concessions",
        time_frame: TimeFrame::Daily,
        program: None,
        impact: Some("dsb_flag"),
    },
    Dataset {
        id: "da_dsp_weekly_cdf_deep_dive",
        table: "customer_feedback",
        time_frame: TimeFrame::Weekly,
        program: None,
        impact: Some("cdf_impact_flag"),
    },
    Dataset {
        id: "da_dsp_daily_psb_stop",
        table: "pickup_failures",
        time_frame: TimeFrame::Daily,
        program: None,
        impact: None,
    },
    Dataset {
        id: "da_dsp_station_daily_safety_oss_events_intraday",
        table: "safety_events",
        time_frame: TimeFrame::Daily,
        program: None,
        impact: Some("oss_impact_flag"),
    },
    Dataset {
        id: "da_dsp_station_weekly_safety_oss_v2",
        table: "driver_safety",
        time_frame: TimeFrame::Weekly,
        program: None,
        impact: None,
    },
    Dataset {
        id: "dsp_station_weekly_quality",
        table: "dsp_quality",
        time_frame: TimeFrame::Weekly,
        program: None,
        impact: None,
    },
    Dataset {
        id: "dsp_station_weekly_team",
        table: "dsp_team",
        time_frame: TimeFrame::Weekly,
        program: None,
        impact: None,
    },
    Dataset {
        id: "dsp_station_weekly_compliance",
        table: "dsp_compliance",
        time_frame: TimeFrame::Weekly,
        program: None,
        impact: None,
    },
    Dataset {
        id: "dsp_station_weekly_safety_oss_v2",
        table: "dsp_safety",
        time_frame: TimeFrame::Weekly,
        program: None,
        impact: None,
    },
    Dataset {
        id: "dsp_station_weekly_working_device",
        table: "dsp_working_device",
        time_frame: TimeFrame::Weekly,
        program: None,
        impact: None,
    },
    Dataset {
        id: "dsp_weekly_cdf",
        table: "dsp_feedback",
        time_frame: TimeFrame::Weekly,
        program: None,
        impact: None,
    },
    Dataset {
        id: "dsp_weekly_psb",
        table: "dsp_pickups",
        time_frame: TimeFrame::Weekly,
        program: None,
        impact: None,
    },
];
/// The DSP's own scorecard row: a week without one is not posted yet.
pub const POSTED_SIGNAL: &str = "dsp_station_weekly_quality";
pub fn dataset(id: &str) -> Option<&'static Dataset> {
    DATASETS.iter().find(|d| d.id == id)
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Collection {
    #[serde(rename = "scorecard")]
    Scorecard,
}
/// One week of one station's scorecard, as a job asks for it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub collection: Collection,
    pub week: String,
    pub station: String,
    pub timezone: String,
    pub dsp_name: String,
    pub dsp_abbreviation: String,
}
impl Request {
    /// Whether a job request or a collection names the scorecard collection.
    pub fn is(value: &Value) -> bool {
        value.get("collection") == Some(&json!("scorecard"))
    }
    pub fn parse(value: &Value) -> Result<Option<Self>> {
        if !Self::is(value) {
            return Ok(None);
        }
        let request: Self =
            serde_json::from_value(value.clone()).map_err(|_| Error::new("invalid_input", 400))?;
        request.validate()?;
        Ok(Some(request))
    }
    pub fn validate(&self) -> Result<()> {
        parse_week(&self.week)?;
        ensure(
            (3..=8).contains(&self.station.len())
                && self
                    .station
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                && token(&self.dsp_abbreviation, 32)
                && !self.dsp_name.trim().is_empty()
                && self.dsp_name.len() <= 256
                && !self.dsp_name.chars().any(char::is_control),
            "invalid_station",
            400,
        )?;
        self.scope_request()
            .validate_scope(&self.discovery().scope("discovery", "discovery")?)
    }
    fn discovery(&self) -> Discovery {
        Discovery {
            date: week_days(&self.week)
                .map(|(_, saturday)| saturday.to_string())
                .unwrap_or_default(),
            station: self.station.clone(),
            timezone: self.timezone.clone(),
            dsp_name: self.dsp_name.clone(),
            dsp_abbreviation: self.dsp_abbreviation.clone(),
        }
    }
    pub fn scope_request(&self) -> ScopeRequest {
        ScopeRequest::Discover(self.discovery())
    }
    /// The interval a dataset takes for this week.
    pub fn interval(&self, dataset: &Dataset) -> Result<(String, String)> {
        Ok(match dataset.time_frame {
            TimeFrame::Weekly => (self.week.clone(), self.week.clone()),
            TimeFrame::Daily => {
                let (first, last) = week_days(&self.week)?;
                (first.to_string(), last.to_string())
            }
        })
    }
}

/// One dataset's rows for the week, as objects, with the address they came from.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatasetCapture {
    pub id: String,
    pub from: String,
    pub to: String,
    pub source_url: String,
    pub rows: Vec<Value>,
}
/// A week's scorecard as collected, before publication.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture {
    pub version: u32,
    pub collection: Collection,
    pub week: String,
    pub station: String,
    pub company_id: String,
    pub dsp_code: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub posted: bool,
    pub datasets: Vec<DatasetCapture>,
}
impl Capture {
    pub fn validate(&self, request: &Request) -> Result<()> {
        request.validate()?;
        ensure(self.version == 1, "scorecard_capture_invalid", 502)?;
        ensure(
            self.week == request.week && self.station == request.station,
            "scorecard_scope_mismatch",
            502,
        )?;
        ensure(
            token(&self.company_id, 128)
                && token(&self.dsp_code, 32)
                && self
                    .dsp_code
                    .eq_ignore_ascii_case(&request.dsp_abbreviation),
            "scorecard_scope_mismatch",
            502,
        )?;
        ensure(
            self.started_at > 0 && self.finished_at >= self.started_at,
            "scorecard_capture_invalid",
            502,
        )?;
        ensure(
            self.datasets.len() == DATASETS.len(),
            "scorecard_capture_invalid",
            502,
        )?;
        for (dataset, captured) in DATASETS.iter().zip(&self.datasets) {
            ensure(captured.id == dataset.id, "scorecard_capture_invalid", 502)?;
            let (from, to) = request.interval(dataset)?;
            ensure(
                captured.from == from && captured.to == to,
                "scorecard_scope_mismatch",
                502,
            )?;
            ensure(
                captured.rows.len() <= MAX_ROWS,
                "scorecard_source_too_large",
                502,
            )?;
            ensure(
                captured.source_url.len() <= 2048,
                "scorecard_capture_invalid",
                502,
            )?;
            ensure(
                captured.rows.iter().all(Value::is_object),
                "scorecard_row_invalid",
                502,
            )?;
        }
        let posted = self
            .datasets
            .iter()
            .find(|d| d.id == POSTED_SIGNAL)
            .is_some_and(|d| !d.rows.is_empty());
        ensure(posted == self.posted, "scorecard_capture_invalid", 502)
    }
    pub fn validate_scope(&self, request: &Request, scope: &Scope) -> Result<()> {
        self.validate(request)?;
        request.scope_request().validate_scope(scope)?;
        ensure(
            self.company_id == scope.provider,
            "scorecard_scope_mismatch",
            502,
        )
    }
    pub fn row_count(&self) -> usize {
        self.datasets.iter().map(|d| d.rows.len()).sum()
    }
}

/// What fixture mode collects: a small, complete week.
pub fn fixture(request: &Request) -> Result<Capture> {
    request.validate()?;
    let started_at = now();
    let (first, last) = week_days(&request.week)?;
    let drivers = ["driver-1", "driver-2", "driver-3"];
    let mut datasets = Vec::new();
    for dataset in DATASETS {
        let (from, to) = request.interval(dataset)?;
        let rows: Vec<Value> = match dataset.id {
            "da_dsp_station_weekly_performance" => drivers
                .iter()
                .enumerate()
                .map(|(i, driver)| {
                    json!({"transporter_id":driver,"da_name":format!("Fixture Driver {}", i + 1),"week":"38","year":2026,
                        "data_date":request.week,"station_code":request.station,"dsp_code":"FXTR",
                        "da_overall_score":90 - i as i64,"da_overall_tier":"Fantastic","delivered":1000 + i as i64,
                        "cdf_metric_dpmo":100 * i as i64,"dsb_metric_dpmo":0,"rts_metric_adjusted":0})
                })
                .collect(),
            "da_dsp_station_weekly_safety_oss_v2" => drivers
                .iter()
                .map(|driver| {
                    json!({"transporter_id":driver,"data_date":request.week,
                        "speeding_rate":0,"seatbelt_rate":0})
                })
                .collect(),
            "da_dsp_weekly_rts_deep_dive" => vec![
                json!({"transporter_id":"driver-1","tracking_id":"TBA000000000001",
                    "data_date":request.week,"delivery_planned_date":first.to_string(),
                    "impacting_dcr":"Y","rts_reason_code":"BUSINESS CLOSED","weekly_exemption":0}),
                json!({"transporter_id":"driver-2","tracking_id":"TBA000000000002",
                    "data_date":request.week,"delivery_planned_date":last.to_string(),
                    "impacting_dcr":"N","rts_reason_code":"CUSTOMER UNAVAILABLE","weekly_exemption":1}),
            ],
            "da_dsp_station_daily_dsb_dnr_tba" => vec![json!({"transporter_id":"driver-3",
                "tracking_id":"TBA000000000003","data_date":first.to_string(),"dsb_flag":1,
                "delivery_type":"Residential"})],
            "da_dsp_weekly_cdf_deep_dive" => vec![
                json!({"transporter_id":"driver-1","tracking_id":"TBA000000000004",
                    "data_date":request.week,"cdf_impact_flag":"Y","negative_feedback_flag":1,
                    "driver_was_unprofessional":1}),
                json!({"transporter_id":"driver-2","tracking_id":"TBA000000000005",
                    "data_date":request.week,"cdf_impact_flag":"N","negative_feedback_flag":0,
                    "delivered_with_care":1}),
            ],
            "da_dsp_daily_psb_stop" => vec![],
            "da_dsp_station_daily_safety_oss_events_intraday" => vec![json!({
                "transporter_id":"driver-3","event_id":"90000001","data_date":last.to_string(),
                "oss_impact_flag":1,"dashboard_metric_type":"Speeding"})],
            "dsp_station_weekly_quality" => vec![json!({"dsp_code":"FXTR",
                "station_code":request.station,"data_date":request.week,
                "dsp_final_score":88.5,"dsp_final_tier":"Great"})],
            "dsp_weekly_cdf" => vec![json!({"dsp_code":"FXTR","data_date":request.week,
                "positive_response_cnt":40,"negative_response_cnt":1,"no_feedback_cnt":900})],
            "dsp_weekly_psb" => vec![json!({"dsp_code":"FXTR","data_date":request.week,
                "total_stops":12,"failed_stops":0})],
            _ => vec![json!({"dsp_code":"FXTR","station_code":request.station,
                "data_date":request.week})],
        };
        datasets.push(DatasetCapture {
            id: dataset.id.into(),
            source_url: format!(
                "fixture://performance/api/v1/getData?dataSetId={}",
                dataset.id
            ),
            from,
            to,
            rows,
        });
    }
    let capture = Capture {
        version: 1,
        collection: Collection::Scorecard,
        week: request.week.clone(),
        station: request.station.clone(),
        company_id: "company-fixture".into(),
        dsp_code: request.dsp_abbreviation.clone(),
        started_at,
        finished_at: now().max(started_at),
        posted: true,
        datasets,
    };
    capture.validate(request)?;
    Ok(capture)
}
