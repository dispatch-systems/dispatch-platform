//! Exact datasets used by Amazon's Daily Quality and Day Safety pages.
use crate::performance::{Dataset, TimeFrame};
const fn daily(id: &'static str, table: &'static str, impact: Option<&'static str>) -> Dataset {
    Dataset {
        id,
        table,
        time_frame: TimeFrame::Daily,
        program: None,
        station: true,
        impact,
    }
}
const fn threshold(id: &'static str, table: &'static str) -> Dataset {
    Dataset {
        id,
        table,
        time_frame: TimeFrame::Daily,
        program: Some("AMZL"),
        station: false,
        impact: None,
    }
}
pub const DATASETS: &[Dataset] = &[
    daily(
        "da_dsp_station_daily_supplemental_quality",
        "driver_quality",
        None,
    ),
    daily("dsp_daily_supplemental_quality", "dsp_quality", None),
    daily("da_dsp_daily_rts", "driver_returns", None),
    daily("dsp_daily_rts", "dsp_returns", None),
    daily(
        "da_dsp_daily_rts_deep_dive",
        "returns_to_station",
        Some("impacting_dcr"),
    ),
    daily(
        "da_dsp_station_daily_dsb_dnr",
        "driver_delivery_behaviors",
        None,
    ),
    daily("dsp_station_daily_dsb_dnr", "dsp_delivery_behaviors", None),
    daily("da_dsp_daily_cdf", "driver_feedback", None),
    daily("dsp_daily_cdf", "dsp_feedback", None),
    daily("da_dsp_daily_psb", "driver_pickups", None),
    daily("dsp_daily_psb", "dsp_pickups", None),
    daily("da_dsp_daily_lmcx_call_text", "driver_contacts", None),
    daily("dsp_daily_lmcx_call_text", "dsp_contacts", None),
    threshold("da_daily_thresholds", "driver_thresholds"),
    threshold("dsp_daily_thresholds", "dsp_thresholds"),
    daily(
        "da_dsp_station_daily_safety_oss_intraday",
        "driver_safety",
        None,
    ),
    daily("dsp_station_daily_safety_oss_intraday", "dsp_safety", None),
    daily(
        "da_dsp_station_daily_safety_oss_events_intraday",
        "safety_events",
        Some("oss_impact_flag"),
    ),
    daily("dsp_station_daily_working_device", "working_devices", None),
    daily(
        "da_dsp_station_daily_safety_oss",
        "driver_safety_batch",
        None,
    ),
    daily("dsp_station_daily_safety_oss", "dsp_safety_batch", None),
    daily(
        "da_dsp_station_daily_safety_oss_events_real_time",
        "live_safety_events",
        None,
    ),
];
pub fn dataset(name: &str) -> Option<&'static Dataset> {
    DATASETS
        .iter()
        .find(|dataset| dataset.table == name || dataset.id == name)
}
