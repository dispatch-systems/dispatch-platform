use super::*;
use crate::{
    collectors::cortex,
    contracts::{DvicInspection, DvicInspections, DvicStatus},
    db::{DspLease, Store, at, now, s},
    manifest::Keeper,
};
use rusqlite::params;
use std::collections::HashMap;

#[derive(Clone)]
pub struct KnownReport {
    pub etag: Option<String>,
    pub sha256: String,
    pub modified_at: i64,
}

impl Store {
    pub fn dvic(&self, id: &str) -> Result<DspLease<'_>> {
        self.added_storage(id, cortex::PROVIDER, &STORAGE)
    }

    pub(crate) fn bind_dvic_request(&self, id: &str, weeks: Vec<String>) -> Result<Value> {
        let dsp = self.find_dsp(id)?;
        let profile = self.profile(id)?;
        ensure(
            !profile.station_code.is_empty(),
            "dvic_station_required",
            409,
        )?;
        let timezone: chrono_tz::Tz = dsp
            .timezone
            .parse()
            .map_err(|_| Error::new("invalid_timezone", 400))?;
        let request = Request {
            collection: Collection::Dvic,
            station: profile.station_code,
            weeks,
            date: chrono::Utc::now()
                .with_timezone(&timezone)
                .date_naive()
                .to_string(),
            timezone: dsp.timezone,
            dsp_name: dsp.name,
            dsp_abbreviation: profile.abbreviation,
        };
        request.validate()?;
        Ok(serde_json::to_value(request)?)
    }
    pub(crate) fn dvic_request(&self, id: &str, weeks: Vec<String>) -> Result<Value> {
        let request = self.bind_dvic_request(id, weeks)?;
        // Keep the durable job payload readable by the previous binary. The worker
        // binds live tenant context immediately before collection and publication.
        Ok(json!({"collection":"dvic","station":request["station"],"weeks":request["weeks"]}))
    }
    pub fn enqueue_dvic(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        week: Option<&str>,
        weeks: usize,
    ) -> Result<Value> {
        let latest = report_week(chrono::Utc::now().date_naive());
        let week = week.unwrap_or(&latest);
        weeks::parse_week(week)?;
        ensure(week <= latest.as_str(), "dvic_week_not_available", 400)?;
        let request = self.dvic_request(id, weeks_ending(week, weeks)?)?;
        let job = self
            .enqueue_batch(id, actor, &[(key.into(), cortex::PROVIDER, request)])?
            .remove(0);
        Ok(serde_json::to_value(job)?)
    }
    pub(crate) fn dvic_schedule_ready(&self, id: &str) -> Result<()> {
        ensure(
            !self.profile(id)?.station_code.is_empty(),
            "dvic_station_required",
            409,
        )
    }
    /// Recheck the current and previous publication weeks each run, and catch up
    /// two older weeks per run. One browser session handles the entire batch.
    pub fn dvic_jobs(&self, id: &str) -> Result<Vec<(String, Value)>> {
        let latest = report_week(chrono::Utc::now().date_naive());
        let station = self.profile(id)?.station_code;
        let db = self.dvic(id)?;
        let checked: HashMap<String, i64> = db.all(
            "SELECT week,max(checked_at) checked_at FROM dvic_weeks WHERE station=? AND scope_verified=1 GROUP BY week",
            [&station],
        )?.into_iter().filter_map(|row| {
            chrono::DateTime::parse_from_rfc3339(s(&row, "checked_at")).ok()
                .map(|at| (s(&row, "week").to_owned(), at.timestamp_millis()))
        }).collect();
        let mut weeks = weeks_ending(&latest, MAX_WEEKS)?;
        let mut older = weeks.split_off(2);
        let cutoff = now() - 7 * 86400 * 1000;
        older.retain(|week| checked.get(week).is_none_or(|time| *time <= cutoff));
        // Unseen weeks first, then least recently checked: daily runs must not
        // starve the older backlog by repeatedly refreshing the newest old weeks.
        older.sort_by_key(|week| checked.get(week).copied().unwrap_or(i64::MIN));
        weeks.extend(older.into_iter().take(2));
        Ok(vec![("dvic".into(), self.dvic_request(id, weeks)?)])
    }
    pub(crate) fn dvic_known(
        &self,
        id: &str,
        station: &str,
        company: &str,
    ) -> Result<HashMap<String, KnownReport>> {
        self.dvic(id)?
            .all(
                "SELECT source_key,etag,sha256,modified_at FROM dvic_reports WHERE station=? AND company_id=? AND scope_verified=1",
                params![station, company],
            )?
            .into_iter()
            .map(|row| {
                Ok((s(&row, "source_key").into(), KnownReport {
                    etag: row["etag"].as_str().map(str::to_owned),
                    sha256: s(&row, "sha256").into(),
                    modified_at: row["modified_at"].as_i64()
                        .ok_or_else(|| Error::new("invalid_stored_record", 500))?,
                }))
            })
            .collect()
    }

    /// Validate the complete batch before committing any report, revision, or row.
    /// A job can be published again after interruption without adding history twice.
    pub fn publish_dvic(
        &self,
        id: &str,
        job: &str,
        capture: &Capture,
        scope: &crate::collectors::cortex::discovery::Scope,
    ) -> Result<()> {
        let job_row = self.job_row(job, Some(id))?;
        ensure(
            job_row.kind.as_str() == JOB_KIND,
            "dvic_capture_invalid",
            502,
        )?;
        let request: Value = serde_json::from_str(&job_row.request)?;
        let request = super::keeper::Dvic.bind(self, id, &request)?;
        let request =
            Request::parse(&request)?.ok_or_else(|| Error::new("invalid_dvic_request", 400))?;
        capture.validate_scope(&request, scope)?;
        let db = self.dvic(id)?;
        let checked = at(capture.finished_at);
        db.transaction(|| {
            if db.one("SELECT job_id FROM dvic_runs WHERE job_id=?", [job])?.is_some() { return Ok(()); }
            let hidden = super::hidden::hidden(&db)?;
            let mut downloaded = 0;
            let mut row_count = 0;
            for report in &capture.reports {
                let report_id = hash(serde_json::to_string(&[&capture.company_id, &capture.station, &report.source_key])?.as_bytes());
                let existing = db.one(
                    "SELECT sha256,etag,modified_at,scope_verified FROM dvic_reports WHERE id=?",
                    [&report_id],
                )?;
                let trusted = existing
                    .as_ref()
                    .is_some_and(|row| row["scope_verified"].as_i64() == Some(1));
                let Some(rows) = &report.rows else {
                    ensure(trusted && existing.as_ref().is_some_and(|r| {
                        s(r,"sha256") == report.sha256
                            && r["etag"].as_str() == report.etag.as_deref()
                            && report.etag.is_some()
                            && r["modified_at"].as_i64() == Some(report.modified_at)
                    }), "dvic_cache_changed", 409)?;
                    db.exec(
                        "UPDATE dvic_reports SET checked_at=?,scope_verified=1 WHERE id=? AND scope_verified=1",
                        params![checked, report_id],
                    )?;
                    continue;
                };
                // A hidden driver's rows are never written: not in the report's copy,
                // its counts, or the inspections.
                let rows = super::hidden::kept(rows, &hidden);
                downloaded += 1;
                row_count += rows.len();
                // A stale capture cannot roll the same object's metadata backwards.
                ensure(
                    !trusted || existing.as_ref().and_then(|r|r["modified_at"].as_i64()).is_none_or(|t|report.modified_at>=t),
                    "dvic_source_changed", 502,
                )?;
                let revision = hash(format!("{report_id}:{}",report.sha256).as_bytes());
                let min_date = rows.iter().map(|r|r.start_date.as_str()).min();
                let max_date = rows.iter().map(|r|r.start_date.as_str()).max();
                let shorts = rows.iter().map(|row| row.is_short()).collect::<Result<Vec<_>>>()?.into_iter().filter(|short|*short).count();
                db.exec(
                    "INSERT INTO dvic_reports(id,company_id,dsp_code,station,source_key,name,week,report_date,\
                     modified_at,etag,sha256,revision_id,row_count,short_count,min_date,max_date,checked_at,scope_verified) \
                     VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,1) ON CONFLICT(id) DO UPDATE SET \
                     modified_at=excluded.modified_at,etag=excluded.etag,sha256=excluded.sha256,\
                     revision_id=excluded.revision_id,row_count=excluded.row_count,short_count=excluded.short_count,\
                     min_date=excluded.min_date,max_date=excluded.max_date,checked_at=excluded.checked_at,scope_verified=1",
                    params![report_id,capture.company_id,capture.dsp_code,capture.station,report.source_key,
                        report.name,report.week,report.report_date,report.modified_at,report.etag,report.sha256,
                        revision,rows.len() as i64,shorts as i64,min_date,max_date,checked],
                )?;
                db.exec("INSERT OR IGNORE INTO dvic_revisions(id,report_id,sha256,modified_at,collected_at,rows) VALUES \
                    (?,?,?,?,?,?)",
                    params![revision,report_id,report.sha256,report.modified_at,checked,serde_json::to_string(&rows)?],
                )?;
                for row in &rows {
                    db.exec(
                        "INSERT INTO dvic_inspections(company_id,inspection_key,dsp_code,station,start_date,\
                         transporter_id,transporter_name,vin,fleet_type,inspection_type,inspection_status,start_time,\
                         end_time,duration_seconds,minimum_seconds,short,report_date,source_modified_at,revision_id,scope_verified) \
                         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,1) ON CONFLICT(company_id,inspection_key) DO UPDATE SET \
                         transporter_name=excluded.transporter_name,fleet_type=excluded.fleet_type,\
                         inspection_status=excluded.inspection_status,end_time=excluded.end_time,\
                         duration_seconds=excluded.duration_seconds,minimum_seconds=excluded.minimum_seconds,short=excluded.short,\
                         report_date=excluded.report_date,source_modified_at=excluded.source_modified_at,\
                         revision_id=excluded.revision_id,scope_verified=1 WHERE dvic_inspections.scope_verified=0 OR \
                         excluded.report_date>dvic_inspections.report_date OR \
                         (excluded.report_date=dvic_inspections.report_date \
                         AND excluded.source_modified_at>=dvic_inspections.source_modified_at)",
                        params![capture.company_id,row.key(),row.dsp,row.station,row.start_date,row.transporter_id,
                            row.transporter_name,row.vin,row.fleet_type,row.inspection_type,row.inspection_status,
                            row.start_time,row.end_time,row.duration,row.minimum_seconds()?,row.is_short()?,
                            report.report_date,report.modified_at,revision],
                    )?;
                }
            }
            for week in &capture.weeks {
                let count = capture.reports.iter().filter(|r| &r.week == week).count();
                db.exec("INSERT INTO dvic_weeks(station,company_id,week,checked_at,report_count,scope_verified) VALUES (?,?,?,?,?,1) ON \
                    CONFLICT(station,company_id,week) DO UPDATE SET \
                    checked_at=excluded.checked_at,report_count=excluded.report_count,scope_verified=1",
                    params![capture.station,capture.company_id,week,checked,count as i64],
                )?;
            }
            db.exec("INSERT INTO \
                    dvic_runs(job_id,station,company_id,started_at,collected_at,weeks,reports,downloaded,unchanged,rows,scope_verified) \
                    VALUES (?,?,?,?,?,?,?,?,?,?,1)",
                params![job,capture.station,capture.company_id,at(capture.started_at),checked,
                    serde_json::to_string(&capture.weeks)?,capture.reports.len() as i64,downloaded as i64,
                    (capture.reports.len()-downloaded) as i64,row_count as i64],
            )?;
            Ok(())
        })
    }
    pub fn dvic_status(&self, id: &str) -> Result<DvicStatus> {
        let station = self.profile(id)?.station_code;
        let db = self.dvic(id)?;
        let decode = |sql: &str| db.all(sql, [&station]);
        Ok(DvicStatus {
            latest_week: report_week(chrono::Utc::now().date_naive()),
            short_inspections: db.count(
                "SELECT count(*) FROM dvic_inspections \
                 WHERE station=? AND short=1 AND scope_verified=1",
                [&station],
            )? as usize,
            weeks: decode(
                "SELECT week,checked_at AS checkedAt,report_count AS reportCount \
                 FROM dvic_weeks WHERE station=? AND scope_verified=1 \
                 ORDER BY week DESC LIMIT 104",
            )?
            .into_iter()
            .map(serde_json::from_value)
            .collect::<std::result::Result<_, _>>()?,
            reports: decode("SELECT name,week,report_date AS reportDate,row_count AS rowCount,short_count AS \
                    shortCount,min_date AS minDate,max_date AS maxDate,checked_at AS checkedAt FROM dvic_reports WHERE \
                    station=? AND scope_verified=1 ORDER BY report_date DESC LIMIT 366")?
                .into_iter().map(serde_json::from_value).collect::<std::result::Result<_,_>>()?,
            jobs: self.recent_jobs_of(id, JOB_KIND)?,
            station,
        })
    }
    pub fn dvic_inspections(
        &self,
        id: &str,
        from: &str,
        to: &str,
        driver: Option<&str>,
        after: &str,
        limit: usize,
    ) -> Result<DvicInspections> {
        validate::date(from)?;
        validate::date(to)?;
        ensure(
            from <= to && (1..=500).contains(&limit) && after.len() <= 256,
            "invalid_input",
            400,
        )?;
        if let Some(driver) = driver {
            ensure(validate::token(driver, 128), "invalid_input", 400)?;
        }
        let station = self.profile(id)?.station_code;
        let mut rows: Vec<DvicInspection>=self.dvic(id)?.all(
                    "SELECT company_id||':'||inspection_key AS id,start_date AS startDate,transporter_id AS \
                    driverId,transporter_name AS driverName,vin,fleet_type AS fleetType,inspection_type AS \
                    inspectionType,inspection_status AS status,start_time AS startTime,end_time AS \
                    endTime,duration_seconds AS durationSeconds,minimum_seconds AS \
                    minimumSeconds,minimum_seconds-duration_seconds AS shortBySeconds,report_date AS sourceReportDate \
                    FROM dvic_inspections WHERE station=?1 AND short=1 AND scope_verified=1 \
                    AND start_date>=?2 AND start_date<=?3 AND (?4 IS \
                    NULL OR transporter_id=?4) AND start_date||':'||company_id||':'||inspection_key>?5 ORDER BY \
                    start_date,company_id,inspection_key LIMIT ?6",
                    params![station,from,to,driver,after,(limit+1) as i64],
                )?.into_iter().map(serde_json::from_value).collect::<std::result::Result<_,_>>()?;
        let more = rows.len() > limit;
        rows.truncate(limit);
        let next_cursor = if more {
            rows.last().map(|r| format!("{}:{}", r.start_date, r.id))
        } else {
            None
        };
        Ok(DvicInspections {
            inspections: rows,
            next_cursor,
        })
    }
}
