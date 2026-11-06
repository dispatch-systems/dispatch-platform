//! Redacted collection health for an authorized ticket's DSP and time window.
use crate::{
    Result,
    collection::registry::Provider,
    db::{self, Store},
};
use serde_json::{Value, json};

/// Ticket investigations need provider/job health, never payroll records or credential
/// labels. The caller supplies its already-authorized DSP and ticket-derived time window.
pub fn diagnostics(db: &Store, dsp: &str, since: i64) -> Result<Value> {
    db.ensure_dsp_active(dsp)?;
    let since = since
        .max(db::now() - 7 * 24 * 60 * 60 * 1000)
        .min(db::now());
    let connections = Provider::all()
        .map(|provider| {
            let connection = db.connection_for(dsp, provider)?;
            Ok(json!({
                "provider":provider.id(),
                "enabled":connection.enabled,
                "status":connection.status,
                "error":connection.error.as_deref().map(error_code)
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    let jobs = db
        .recent_jobs(Some(dsp))?
        .into_iter()
        .filter(|job| {
            chrono::DateTime::parse_from_rfc3339(&job.created_at)
                .is_ok_and(|date| date.timestamp_millis() >= since)
        })
        .take(20)
        .map(|job| {
            json!({
                "kind":job.kind,
                "status":job.status,
                "error":job.error.as_deref().map(error_code),
                "createdAt":job.created_at,
                "completedAt":job.completed_at
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({"since":since,"connections":connections,"jobs":jobs}))
}

fn error_code(code: &str) -> &str {
    if code.len() <= 64
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
    {
        code
    } else {
        "provider_failed"
    }
}
