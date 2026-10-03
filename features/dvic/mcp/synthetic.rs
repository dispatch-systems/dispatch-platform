//! What DVIC holds of the synthetic DSP: one pre-trip inspection each route day; under 90
//! seconds is short.
use crate::dvic::DvicStore;
use dispatch_core::{
    Result,
    db::Store,
    mcp::synthetic::{Made, STATION, Step, Synthetic, World, hhmm, plan},
};
use serde_json::json;

pub const SYNTHETIC: Synthetic = Synthetic {
    people: None,
    steps: &[Step {
        order: 40,
        run: inspections,
    }],
};

fn inspections(db: &Store, world: &mut World) -> Result<Made> {
    let dates = &world.dates;
    let dvic = db.dvic_db(&world.dsp)?;
    let (first, last) = (dates[0].to_string(), dates[dates.len() - 1].to_string());
    dvic.exec(
        "INSERT OR REPLACE INTO dvic_reports(id,company_id,dsp_code,station,source_key,name,week,\
         report_date,modified_at,sha256,revision_id,row_count,short_count,min_date,max_date,checked_at,scope_verified) \
         VALUES ('synthetic','synthetic','NLOG',?1,'synthetic','DVIC','synthetic',?3,0,'synthetic',\
         'synthetic',0,0,?2,?3,?3,1)",
        rusqlite::params![STATION, first, last],
    )?;
    dvic.exec(
        "INSERT OR REPLACE INTO dvic_revisions(id,report_id,sha256,modified_at,collected_at,rows) \
         VALUES ('synthetic','synthetic','synthetic',0,?,'[]')",
        [&last],
    )?;
    let mut inspections = 0;
    for (d, date) in dates.iter().enumerate() {
        for (i, _, name) in &world.drivers {
            let Some(plan) = plan(*i, d as i64) else {
                continue;
            };
            dvic.exec(
                "INSERT OR REPLACE INTO dvic_inspections(company_id,inspection_key,dsp_code,station,\
                 start_date,transporter_id,transporter_name,vin,fleet_type,inspection_type,\
                 inspection_status,start_time,end_time,duration_seconds,minimum_seconds,short,\
                 report_date,source_modified_at,revision_id,scope_verified) VALUES ('synthetic',?1,'NLOG',?2,?3,?4,?5,\
                 ?6,'CDV','PRE_TRIP','COMPLETE',?7,?8,?9,90,?10,?3,0,'synthetic',1)",
                rusqlite::params![
                    format!("synthetic-{date}-{i}"),
                    STATION,
                    date.to_string(),
                    world.transporter(*i),
                    name,
                    format!("SYNTHVIN{i:09}"),
                    hhmm(plan.departed - 15),
                    hhmm(plan.departed - 15 + (plan.inspection_seconds + 59) / 60),
                    plan.inspection_seconds as f64,
                    i64::from(plan.inspection_seconds < 90),
                ],
            )?;
            inspections += 1;
        }
    }
    Ok(Some(("inspections", json!(inspections))))
}
