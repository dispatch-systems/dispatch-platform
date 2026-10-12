//! One immutable employee-period sync: every Paycom employee and every elapsed Cortex day.
use crate::{
    api::employee_sync::{EmployeeSyncSource, EmployeeSyncStatus},
    backend::TimecardStore,
};
use dispatch_core::{
    Result,
    collection::{
        api::jobs::{JobRow, JobStatus},
        registry::Provider,
    },
    db::{Store, s},
    ensure,
};
use dispatch_cortex::{
    self as cortex,
    discovery::{CollectionRequest, Scope},
    meals::DayRequest,
};
use dispatch_paycom::{
    self as paycom,
    timecards::{EmployeeTimecardPeriod, PERIOD_DAYS, PeriodSync},
};
use rusqlite::params;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn prefix(key: &str) -> String {
    format!("employees:{key}:")
}

fn batch_jobs(store: &Store, id: &str, key: &str) -> Result<Vec<JobRow>> {
    let prefix = prefix(key);
    Ok(store
        .jobs_keyed(id, &prefix, paycom::timecards::JOB_KIND)?
        .into_iter()
        .filter(|row| {
            let suffix = row.idempotency_key.strip_prefix(&prefix).unwrap_or("");
            suffix == "paycom"
                || suffix
                    .strip_prefix("cortex:")
                    .is_some_and(|date| dispatch_core::foundation::validate::date(date).is_ok())
        })
        .collect())
}

fn batch_period(rows: &[JobRow]) -> Result<Option<EmployeeTimecardPeriod>> {
    rows.iter()
        .find(|row| row.kind.as_str() == paycom::timecards::JOB_KIND)
        .map(|row| {
            let request: Value = serde_json::from_str(&row.request)?;
            let request = PeriodSync::parse(&request)?
                .ok_or_else(|| dispatch_core::Error::new("operation_failed", 500))?;
            Ok(request.period())
        })
        .transpose()
}

/// Prefer scope evidence from the requested period, then fill every missing identity
/// from the tenant's own history. A partially collected period must not omit a station
/// merely because that station has not yet been collected for one of its dates.
fn period_scopes(store: &Store, id: &str, period: &EmployeeTimecardPeriod) -> Result<Vec<Scope>> {
    let db = store.collector(id, cortex::PROVIDER)?;
    let mut rows = db.all(
        "SELECT station,service_area_id,provider,timezone FROM meal_publications \
         WHERE active=1 AND report_date BETWEEN ? AND ? ORDER BY collected_at DESC,id DESC",
        params![period.from, period.to],
    )?;
    rows.extend(db.all(
        "SELECT station,service_area_id,provider,timezone FROM meal_publications \
         WHERE active=1 AND NOT (report_date BETWEEN ? AND ?) ORDER BY collected_at DESC,id DESC",
        params![period.from, period.to],
    )?);
    let mut scopes = BTreeMap::new();
    for row in rows {
        let scope = Scope {
            date: period.from.clone(),
            station: s(&row, "station").into(),
            service_area_id: s(&row, "service_area_id").into(),
            provider: s(&row, "provider").into(),
            timezone: s(&row, "timezone").into(),
        };
        scopes
            .entry((
                scope.station.clone(),
                scope.service_area_id.clone(),
                scope.provider.clone(),
            ))
            .or_insert(scope);
    }
    Ok(scopes.into_values().collect())
}

fn scope_available(store: &Store, id: &str, period: &EmployeeTimecardPeriod) -> Result<bool> {
    if !period_scopes(store, id, period)?.is_empty() {
        return Ok(true);
    }
    let discovery = super::sync::meal_sync_discovery(store, id, &period.from)?;
    Ok((3..=8).contains(&discovery.station.len())
        && discovery
            .station
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit()))
}

fn summarize(
    store: &Store,
    id: &str,
    provider: Provider,
    rows: Vec<JobRow>,
) -> Result<EmployeeSyncSource> {
    let jobs = rows
        .into_iter()
        .map(|row| store.public_job(row))
        .collect::<Result<Vec<_>>>()?;
    let total = jobs.len();
    let completed = jobs
        .iter()
        .filter(|job| job.status == JobStatus::Succeeded)
        .count();
    let failed = jobs
        .iter()
        .filter(|job| job.status == JobStatus::Failed)
        .count();
    let cancelled = jobs
        .iter()
        .filter(|job| job.status == JobStatus::Cancelled)
        .count();
    let progress = if total == 0 {
        0
    } else {
        (jobs
            .iter()
            .map(|job| {
                if job.status == JobStatus::Succeeded {
                    100
                } else {
                    i64::from(job.progress.min(99))
                }
            })
            .sum::<i64>()
            / total as i64) as u8
    };
    Ok(EmployeeSyncSource {
        enabled: store.connection_for(id, provider)?.enabled,
        active: jobs.iter().any(|job| job.status.is_active()),
        total,
        completed,
        failed,
        cancelled,
        progress,
        jobs,
    })
}

fn status(
    store: &Store,
    id: &str,
    period: &EmployeeTimecardPeriod,
    key: Option<&str>,
    rows: Vec<JobRow>,
) -> Result<EmployeeSyncStatus> {
    if !rows.is_empty() {
        ensure(
            batch_period(&rows)?.as_ref() == Some(period),
            "idempotency_conflict",
            409,
        )?;
    }
    let mut through = period.to.clone().min(store.local_date(id)?);
    let mut cortex_rows = Vec::new();
    let mut paycom_rows = Vec::new();
    let mut dates = BTreeSet::new();
    for row in rows {
        if row.kind.as_str() == paycom::timecards::JOB_KIND {
            paycom_rows.push(row);
        } else {
            let value: Value = serde_json::from_str(&row.request)?;
            let request = DayRequest::parse(&value)?
                .ok_or_else(|| dispatch_core::Error::new("operation_failed", 500))?;
            ensure(
                request.date >= period.from
                    && request.date <= period.to
                    && dates.insert(request.date),
                "invalid_stored_record",
                500,
            )?;
            cortex_rows.push(row);
        }
    }
    if let Some(last) = dates.last() {
        through = last.clone();
    }
    let paycom = summarize(store, id, paycom::PROVIDER, paycom_rows)?;
    let cortex = summarize(store, id, cortex::PROVIDER, cortex_rows)?;
    let kinds: Vec<_> = Provider::all().flat_map(Provider::job_kinds).collect();
    let elapsed = (through
        .parse::<chrono::NaiveDate>()
        .map_err(|_| dispatch_core::Error::new("invalid_date", 400))?
        - period.start()?)
    .num_days()
        + 1;
    let complete = paycom.total == 1
        && cortex.total > 0
        && cortex.total as i64 == elapsed
        && dates.first() == Some(&period.from)
        && paycom.completed == paycom.total
        && cortex.completed == cortex.total;
    Ok(EmployeeSyncStatus {
        period: period.clone(),
        through,
        request_id: key.map(str::to_owned),
        scope_available: scope_available(store, id, period)?,
        active: store.any_active_job(id, &kinds)?,
        complete,
        paycom,
        cortex,
    })
}

pub(crate) fn employee_sync_status(
    store: &Store,
    id: &str,
    period: &EmployeeTimecardPeriod,
    request_id: Option<&str>,
) -> Result<EmployeeSyncStatus> {
    store.timecard_period(id, None, None, Some(period))?;
    let key = if let Some(key) = request_id {
        Some(key.to_owned())
    } else {
        // Do not use the recent-jobs listing: another busy owner can push a date's job
        // outside its 200-row window while the batch is still relevant.
        store
            .jobs_keyed(id, "employees:", paycom::timecards::JOB_KIND)?
            .into_iter()
            .filter(|row| {
                if row.kind.as_str() != paycom::timecards::JOB_KIND {
                    return false;
                }
                serde_json::from_str::<Value>(&row.request)
                    .ok()
                    .and_then(|value| PeriodSync::parse(&value).ok().flatten())
                    .is_some_and(|request| request.period() == *period)
            })
            .max_by(|a, b| (&a.created_at, &a.id).cmp(&(&b.created_at, &b.id)))
            .and_then(|row| {
                row.idempotency_key
                    .strip_prefix("employees:")?
                    .strip_suffix(":paycom")
                    .map(str::to_owned)
            })
    };
    let rows = key
        .as_deref()
        .map(|key| batch_jobs(store, id, key))
        .transpose()?
        .unwrap_or_default();
    status(store, id, period, key.as_deref(), rows)
}

pub(crate) fn enqueue_employee_sync(
    store: &Store,
    id: &str,
    actor: &str,
    key: &str,
    period: &EmployeeTimecardPeriod,
) -> Result<EmployeeSyncStatus> {
    period.start()?;
    // Return an accepted immutable batch before checking today's date or connections.
    let existing = batch_jobs(store, id, key)?;
    if !existing.is_empty() {
        return status(store, id, period, Some(key), existing);
    }
    store.timecard_period(id, None, None, Some(period))?;
    let paycom_request = PeriodSync {
        all_employees: true,
        from: period.from.clone(),
        to: period.to.clone(),
    };
    paycom_request.date(&store.find_dsp(id)?.timezone)?;
    ensure(
        store.connection_for(id, paycom::PROVIDER)?.enabled,
        "meal_sync_paycom_required",
        409,
    )?;
    ensure(
        store.connection_for(id, cortex::PROVIDER)?.enabled,
        "meal_sync_flex_required",
        409,
    )?;
    let through = period.to.clone().min(store.local_date(id)?);
    let templates = period_scopes(store, id, period)?;
    let start = period.start()?;
    let prefix = prefix(key);
    let mut requests = vec![(
        format!("{prefix}paycom"),
        paycom::PROVIDER,
        serde_json::to_value(paycom_request)?,
    )];
    for offset in 0..PERIOD_DAYS {
        let date = (start + chrono::Duration::days(offset)).to_string();
        if date > through {
            break;
        }
        let scope_requests = if templates.is_empty() {
            let discovery = super::sync::meal_sync_discovery(store, id, &date)?;
            ensure(
                !discovery.station.is_empty(),
                "meal_sync_scope_required",
                409,
            )?;
            discovery.scope("discovery", "discovery")?;
            vec![CollectionRequest::Discover(discovery)]
        } else {
            templates
                .iter()
                .map(|template| {
                    let mut scope = template.clone();
                    scope.date = date.clone();
                    CollectionRequest::Scoped(scope)
                })
                .collect()
        };
        let request = DayRequest {
            date: date.clone(),
            scope_requests,
        };
        request.validate()?;
        requests.push((
            format!("{prefix}cortex:{date}"),
            cortex::PROVIDER,
            serde_json::to_value(request)?,
        ));
    }
    store.enqueue_batch(id, Some(actor), &requests)?;
    status(store, id, period, Some(key), batch_jobs(store, id, key)?)
}
