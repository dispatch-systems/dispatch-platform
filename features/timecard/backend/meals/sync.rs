//! Queue both sources together; a missing connection/scope cannot start half a sync.
use crate::collectors::cortex::{
    self,
    discovery::{Discovery, Scope},
};
use dispatch_core::{
    Result,
    collection::registry::Provider,
    db::{Store, s},
    ensure,
};
use dispatch_paycom::{self as paycom, timecards::collection_date};
use serde_json::{Value, json};

// Reuse the selected day's proven scopes. For an uncollected day, use the
// most recently collected day's scopes, never another DSP or ALL_DSPS.
const SCOPES: &str = "SELECT station,service_area_id,provider,timezone FROM meal_publications \
    WHERE active=1 AND report_date=COALESCE(\
    (SELECT report_date FROM meal_publications WHERE active=1 AND report_date=? LIMIT 1),\
    (SELECT report_date FROM meal_publications WHERE active=1 \
     ORDER BY collected_at DESC,id DESC LIMIT 1)) ORDER BY station,service_area_id,provider";

fn meal_sync_discovery(store: &Store, id: &str, date: &str) -> Result<Discovery> {
    let profile = store.profile(id)?;
    let dsp = store.find_dsp(id)?;
    Ok(Discovery {
        date: date.into(),
        station: profile.station_code,
        timezone: dsp.timezone,
        dsp_name: dsp.name,
        dsp_abbreviation: profile.abbreviation,
    })
}
pub(crate) fn meal_sync_scopes(store: &Store, id: &str, date: &str) -> Result<Vec<Scope>> {
    let db = store.collector(id, cortex::PROVIDER)?;
    let rows = db.all(SCOPES, [date])?;
    Ok(rows
        .iter()
        .map(|row| Scope {
            date: date.into(),
            station: s(row, "station").into(),
            service_area_id: s(row, "service_area_id").into(),
            provider: s(row, "provider").into(),
            timezone: s(row, "timezone").into(),
        })
        .collect())
}
fn sync_source(store: &Store, id: &str, date: &str, provider: Provider) -> Result<Value> {
    let kind = provider.job_kind();
    let active = store.active_job_of(id, kind)?;
    let mut latest = store.latest_job_for_date(id, kind, date)?;
    if let Some(row) = &latest
        && provider == cortex::PROVIDER
        && let Some((prefix, _)) = row.idempotency_key.rsplit_once(":flex:")
    {
        let prefix = format!("{prefix}:flex:");
        // A multi-station sync succeeds only when every station succeeds.
        let failed = store.stopped_job_keyed(id, kind, &prefix)?;
        if failed.is_some() {
            latest = failed;
        }
    }
    let collected = dispatch_core::manifest::registry()
        .keeper(kind)
        .collected_at(store, id, date)?;
    let job = active.or(latest);
    let request = job
        .as_ref()
        .map(|row| serde_json::from_str::<Value>(&row.request))
        .transpose()?;
    Ok(json!({
        "enabled":store.connection_for(id,provider)?.enabled,
        "active":job.as_ref().is_some_and(|row|row.status.is_active()),
        "jobDate":request.as_ref().and_then(|request|request.get("date")).and_then(Value::as_str),
        "job":job.map(|row|store.public_job(row)).transpose()?,
        "collectedAt":collected.map(|row|row["collected_at"].clone()),
    }))
}
pub(crate) fn meal_sync_status(store: &Store, id: &str, date: &str) -> Result<Value> {
    // Reading a calendar date is valid even when the viewer is a day ahead
    // of the DSP. Collection still validates each provider's business date.
    dispatch_core::foundation::validate::date(date)?;
    let discovery = meal_sync_discovery(store, id, date)?;
    let station_available = (3..=8).contains(&discovery.station.len())
        && discovery
            .station
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    Ok(
        json!({"date":date,"scopeAvailable":station_available || !meal_sync_scopes(store, id,date)?.is_empty(),
        "paycom":sync_source(store,id,date,paycom::PROVIDER)?,
        "flex":sync_source(store,id,date,cortex::PROVIDER)?}),
    )
}
pub(crate) fn enqueue_meals(
    store: &Store,
    id: &str,
    actor: Option<&str>,
    key: &str,
    scope: &Scope,
) -> Result<Value> {
    scope.validate()?;
    store.enqueue_for(
        id,
        actor,
        key,
        cortex::PROVIDER,
        &serde_json::to_value(scope)?,
    )
}
pub(crate) fn enqueue_meal_sync(
    store: &Store,
    id: &str,
    actor: &str,
    key: &str,
    date: &str,
) -> Result<Value> {
    collection_date(&json!({"date":date}), &store.find_dsp(id)?.timezone)?;
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
    // Replay the original batch even after discovery publishes its first
    // scope, or subsequent collections change the available stations.
    let prefix = format!("meal:{key}:");
    // Paycom's job first, then Cortex's stations in order.
    let existing = store.jobs_keyed(id, &prefix, paycom::timecards::JOB_KIND)?;
    let existing: Vec<_> = existing
        .into_iter()
        .filter(|row| {
            let suffix = row.idempotency_key.strip_prefix(&prefix).unwrap_or("");
            suffix == "paycom"
                || suffix.strip_prefix("flex:").is_some_and(|index| {
                    !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit())
                })
        })
        .collect();
    if !existing.is_empty() {
        let mut jobs = Vec::new();
        for row in existing {
            let request: Value = serde_json::from_str(&row.request)?;
            ensure(s(&request, "date") == date, "idempotency_conflict", 409)?;
            jobs.push(store.public_job(row)?);
        }
        return Ok(json!({"date":date,"jobs":jobs}));
    }
    let scopes = meal_sync_scopes(store, id, date)?;
    let mut requests = vec![(
        format!("meal:{key}:paycom"),
        paycom::PROVIDER,
        json!({"date":date}),
    )];
    if scopes.is_empty() {
        let discovery = meal_sync_discovery(store, id, date)?;
        ensure(
            !discovery.station.is_empty(),
            "meal_sync_scope_required",
            409,
        )?;
        discovery.scope("discovery", "discovery")?;
        requests.push((
            format!("meal:{key}:flex:0"),
            cortex::PROVIDER,
            serde_json::to_value(discovery)?,
        ));
    }
    for (index, scope) in scopes.iter().enumerate() {
        scope.validate()?;
        requests.push((
            format!("meal:{key}:flex:{index}"),
            cortex::PROVIDER,
            serde_json::to_value(scope)?,
        ));
    }
    let jobs = store.enqueue_batch(id, Some(actor), &requests)?;
    Ok(json!({"date":date,"jobs":jobs}))
}
