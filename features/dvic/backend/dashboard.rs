//! Compact, complete counts alongside bounded dashboard pages. Every query uses the
//! same station, verified scope, date range, Unicode search and vehicle selection.
use super::{DvicStore, Result, ensure, validate};
use crate::api::types::{
    DvicCell, DvicDaySummary, DvicDriverSummary, DvicInspectionDetail, DvicInspectionPage,
    DvicSummary,
};
use dispatch_core::db::{FromRow, Row, Store, s};
use rusqlite::params;

const MATCHED: &str = "WITH matched AS (
    SELECT company_id||':'||inspection_key id,transporter_id driverId,
        transporter_name driverName,start_date startDate,start_time startTime,
        fleet_type fleetType,duration_seconds durationSeconds,minimum_seconds minimumSeconds
    FROM dvic_inspections WHERE station=?1 AND short=1 AND scope_verified=1
        AND start_date>=?2 AND start_date<=?3
        AND (?4='' OR instr(dispatch_lower(transporter_name),?4)>0
            OR instr(dispatch_lower(transporter_id),?4)>0)
        AND (?5='' OR (fleet_type='SV')=(?5='dot'))
)";
// Keep the name chosen by the existing dashboard's date-descending inspection order:
// the last row for a driver, after filtering. A name change never splits their count.
const DRIVERS: &str = ", named AS (
    SELECT *,row_number() OVER (PARTITION BY driverId ORDER BY
        startDate,startTime COLLATE dispatch_unicode DESC,id COLLATE dispatch_unicode DESC) nameRank
    FROM matched
), driver_totals AS (
    SELECT driverId,coalesce(nullif(max(CASE WHEN nameRank=1 THEN driverName END),''),driverId) driverName,
        count(*) count,json_group_array(DISTINCT fleetType) fleets,
        json_group_array(DISTINCT minimumSeconds) minimums
    FROM named GROUP BY driverId
)";
const INSPECTION: &str = "company_id||':'||inspection_key AS id,start_date AS startDate,
    transporter_id AS driverId,transporter_name AS driverName,vin,fleet_type AS fleetType,
    inspection_type AS inspectionType,inspection_status AS status,start_time AS startTime,
    end_time AS endTime,duration_seconds AS durationSeconds,minimum_seconds AS minimumSeconds,
    minimum_seconds-duration_seconds AS shortBySeconds,report_date AS sourceReportDate";

fn range(from: &str, to: &str) -> Result<()> {
    validate::date(from)?;
    validate::date(to)?;
    let from = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d").unwrap();
    let to = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d").unwrap();
    ensure(
        (0..=6).contains(&(to - from).num_days()),
        "invalid_input",
        400,
    )
}
fn selection(query: &str, vehicles: &str, offset: usize) -> Result<String> {
    ensure(
        query.len() <= 800 && ["", "dot", "non-dot"].contains(&vehicles) && offset <= 1_000_000,
        "invalid_input",
        400,
    )?;
    // Match String.trim() in the dashboard, including BOM and excluding NEL.
    Ok(query
        .trim_matches(|c: char| (c.is_whitespace() && c != '\u{0085}') || c == '\u{feff}')
        .to_lowercase())
}
impl FromRow for DvicDriverSummary {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let mut fleets: Vec<String> = serde_json::from_str(&row.get::<String>("fleets")?)?;
        let mut minimums: Vec<usize> = serde_json::from_str(&row.get::<String>("minimums")?)?;
        fleets.sort();
        minimums.sort_unstable();
        Ok(Self {
            driver_id: row.get("driverId")?,
            driver_name: row.get("driverName")?,
            count: row.get::<i64>("count")? as usize,
            fleets,
            minimums,
            cells: Vec::new(),
        })
    }
}

pub(crate) fn summary(
    store: &Store,
    id: &str,
    from: &str,
    to: &str,
    query: &str,
    vehicles: &str,
    offset: usize,
) -> Result<DvicSummary> {
    range(from, to)?;
    let query = selection(query, vehicles, offset)?;
    let station = store.profile(id)?.station_code;
    let db = store.dvic_db(id)?;
    let mut days: Vec<DvicDaySummary> = db
        .query_as::<(String, i64, i64, i64)>(
            &format!(
                "{MATCHED} SELECT startDate,count(*),sum(fleetType='SV'),sum(fleetType<>'SV')
                FROM matched GROUP BY startDate ORDER BY startDate"
            ),
            params![station, from, to, query, vehicles],
        )?
        .into_iter()
        .map(|(date, count, dot, non_dot)| DvicDaySummary {
            date,
            count: count as usize,
            dot: dot as usize,
            non_dot: non_dot as usize,
            repeat_drivers: Vec::new(),
        })
        .collect();
    let total_inspections = days.iter().map(|day| day.count).sum();
    let total_drivers = db.count(
        &format!("{MATCHED} SELECT count(DISTINCT driverId) FROM matched"),
        params![station, from, to, query, vehicles],
    )? as usize;
    let mut drivers: Vec<DvicDriverSummary> = db.query_as(
        &format!("{MATCHED}{DRIVERS} SELECT * FROM driver_totals ORDER BY count DESC,
            driverName COLLATE dispatch_unicode,driverId COLLATE dispatch_unicode LIMIT 25 OFFSET ?6"),
        params![station, from, to, query, vehicles, offset as i64],
    )?;
    // Rank cells only for the drivers on this page. Large weeks never become a
    // vector of full inspection records in the API or browser.
    if !drivers.is_empty() {
        let selected =
            serde_json::to_string(&drivers.iter().map(|d| &d.driver_id).collect::<Vec<_>>())?;
        for row in db.all(
            &format!(
                "{MATCHED}, cells AS (
                SELECT *,row_number() OVER (PARTITION BY driverId,startDate ORDER BY
                    durationSeconds*1.0/minimumSeconds,startTime COLLATE dispatch_unicode,
                    id COLLATE dispatch_unicode) shortest
                FROM matched WHERE driverId IN (SELECT value FROM json_each(?6))
            ) SELECT * FROM cells WHERE shortest=1 ORDER BY startDate"
            ),
            params![station, from, to, query, vehicles, selected],
        )? {
            let driver = drivers
                .iter_mut()
                .find(|d| d.driver_id == s(&row, "driverId"))
                .unwrap();
            driver.cells.push(serde_json::from_value::<DvicCell>(row)?);
        }
    }
    for (date, name) in db.query_as::<(String, String)>(
        &format!("{MATCHED}{DRIVERS} SELECT m.startDate,d.driverName FROM matched m
            JOIN driver_totals d USING(driverId) WHERE d.count>=3 GROUP BY m.startDate,m.driverId
            ORDER BY d.count DESC,d.driverName COLLATE dispatch_unicode,d.driverId COLLATE dispatch_unicode"),
        params![station, from, to, query, vehicles],
    )? {
        days.iter_mut().find(|day| day.date == date).unwrap().repeat_drivers.push(name);
    }
    Ok(DvicSummary {
        days,
        drivers,
        total_drivers,
        total_inspections,
    })
}

pub(crate) fn day(
    store: &Store,
    id: &str,
    date: &str,
    query: &str,
    vehicles: &str,
    offset: usize,
) -> Result<DvicInspectionPage> {
    range(date, date)?;
    let query = selection(query, vehicles, offset)?;
    let station = store.profile(id)?.station_code;
    let db = store.dvic_db(id)?;
    let (total, minimum) = db
        .one_as::<(i64, i64)>(
            &format!("{MATCHED} SELECT count(*),coalesce(max(minimumSeconds),0) FROM matched"),
            params![station, date, date, query, vehicles],
        )?
        .unwrap();
    let inspections = db
        .all(
            &format!(
                "{MATCHED} SELECT {INSPECTION} FROM dvic_inspections
            WHERE station=?1 AND short=1 AND scope_verified=1 AND start_date=?2
                AND company_id||':'||inspection_key IN (SELECT id FROM matched)
            ORDER BY duration_seconds*1.0/minimum_seconds,start_time COLLATE dispatch_unicode,
                id COLLATE dispatch_unicode LIMIT 100 OFFSET ?6"
            ),
            params![station, date, date, query, vehicles, offset as i64],
        )?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<std::result::Result<_, _>>()?;
    Ok(DvicInspectionPage {
        inspections,
        total: total as usize,
        minimum: minimum as usize,
    })
}

pub(crate) fn detail(
    store: &Store,
    id: &str,
    from: &str,
    to: &str,
    inspection: &str,
) -> Result<DvicInspectionDetail> {
    range(from, to)?;
    let (company, key) = inspection
        .rsplit_once(':')
        .ok_or_else(|| super::Error::new("invalid_input", 400))?;
    let station = store.profile(id)?.station_code;
    let db = store.dvic_db(id)?;
    let inspection = db.one(
        &format!("SELECT {INSPECTION} FROM dvic_inspections WHERE company_id=?1 AND inspection_key=?2
            AND station=?3 AND short=1 AND scope_verified=1 AND start_date>=?4 AND start_date<=?5"),
        params![company, key, station, from, to],
    )?.ok_or_else(|| super::Error::new("inspection_not_found", 404))?;
    let driver = s(&inspection, "driverId");
    let related_total = db.count(
        "SELECT count(*) FROM dvic_inspections WHERE station=?1 AND short=1 AND scope_verified=1
            AND start_date>=?2 AND start_date<=?3 AND transporter_id=?4
            AND NOT (company_id=?5 AND inspection_key=?6)",
        params![station, from, to, driver, company, key],
    )? as usize;
    let related = db.all(
        &format!("SELECT {INSPECTION} FROM dvic_inspections WHERE station=?1 AND short=1 AND scope_verified=1
            AND start_date>=?2 AND start_date<=?3 AND transporter_id=?4
            AND NOT (company_id=?5 AND inspection_key=?6)
            ORDER BY start_time COLLATE dispatch_unicode DESC,id COLLATE dispatch_unicode LIMIT 50"),
        params![station, from, to, driver, company, key],
    )?.into_iter().map(serde_json::from_value).collect::<std::result::Result<_, _>>()?;
    Ok(DvicInspectionDetail {
        inspection: serde_json::from_value(inspection)?,
        related,
        related_total,
    })
}
