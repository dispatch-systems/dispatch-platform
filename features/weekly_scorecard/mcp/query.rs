//! Weekly Scorecard scopes shared by SQL counts, grouped summaries and bounded detail pages.
use super::weekly_scorecard::{Row, scalar_text, snake, tier_rank};
use crate::backend::WeeklyScorecardStore;
use dispatch_core::{
    Result,
    accounts::api::types::Dsp,
    db::{DspLease, Store, s},
    mcp::data::scope::Period,
};
use rusqlite::functions::FunctionFlags;
use serde_json::Value;

/// Preserve the source's string, Boolean and numeric conventions in SQL. Normalizing
/// individual scalar fields avoids decoding or returning every source document.
pub fn database<'a>(db: &'a Store, dsp: &Dsp) -> Result<DspLease<'a>> {
    let data = db.weekly_scorecard_db(&dsp.id)?;
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    data.0
        .create_scalar_function("agent_scorecard_text", 1, flags, |ctx| {
            let raw = ctx.get::<Option<String>>(0)?;
            let value: Value = raw
                .and_then(|raw| serde_json::from_str(&raw).ok())
                .unwrap_or_default();
            Ok(scalar_text(&value))
        })?;
    data.0
        .create_scalar_function("agent_scorecard_yes", 1, flags, |ctx| {
            Ok(matches!(
                ctx.get::<String>(0)?.to_lowercase().as_str(),
                "1" | "y" | "yes" | "true"
            ))
        })?;
    data.0
        .create_scalar_function("agent_scorecard_snake", 1, flags, |ctx| {
            Ok(snake(&ctx.get::<String>(0)?))
        })?;
    data.0
        .create_scalar_function("agent_scorecard_rank", 1, flags, |ctx| {
            Ok(tier_rank(&ctx.get::<String>(0)?))
        })?;
    Ok(data)
}
pub fn field(name: &str) -> String {
    format!("agent_scorecard_text(x.row -> '$.{name}')")
}
pub fn yes(name: &str) -> String {
    format!("agent_scorecard_yes({})", field(name))
}
pub fn normalized(name: &str) -> String {
    format!("agent_scorecard_snake({})", field(name))
}
pub fn missed() -> String {
    let coaching = format!("dispatch_lower({})", field("weekly_coaching"));
    format!(
        "(instr({coaching},'contact')>0 OR instr({coaching},'call')>0 OR instr({coaching},'text')>0)"
    )
}

pub struct Dataset<'a> {
    data: DspLease<'a>,
    scope: String,
    params: Vec<String>,
    date: &'static str,
}
impl<'a> Dataset<'a> {
    pub fn new(
        db: &'a Store,
        dsp: &Dsp,
        table: &'static str,
        date: &'static str,
        period: &Period,
        drivers: Option<&[String]>,
    ) -> Result<Self> {
        let station = db.profile(&dsp.id)?.station_code;
        let data = database(db, dsp)?;
        let scope = format!(
            " FROM {table} x JOIN scorecard_publications p ON p.id=x.publication_id AND p.active=1 AND p.scope_verified=1 \
             WHERE p.station=? AND substr(json_extract(x.row,'$.{date}'),1,10) BETWEEN ? AND ?"
        );
        let mut selected = Self {
            data,
            scope,
            params: vec![station, period.first(), period.last()],
            date,
        };
        if let Some(ids) = drivers {
            if ids.is_empty() {
                selected.and("0", None);
            } else {
                selected.and(
                    &format!("x.transporter_id IN ({})", vec!["?"; ids.len()].join(",")),
                    None,
                );
                selected.params.extend_from_slice(ids);
            }
        }
        Ok(selected)
    }
    pub fn and(&mut self, predicate: &str, value: Option<&str>) {
        self.scope.push_str(&format!(" AND ({predicate})"));
        if let Some(value) = value {
            self.params.push(value.to_owned());
        }
    }
    pub fn totals(&self, sums: &[(&str, String)]) -> Result<Value> {
        let mut select = String::from("SELECT COUNT(*) total");
        for (name, expression) in sums {
            select.push_str(&format!(",COALESCE(SUM({expression}),0) {name}"));
        }
        Ok(self
            .data
            .one(
                &(select + &self.scope),
                rusqlite::params_from_iter(&self.params),
            )?
            .unwrap_or_default())
    }
    /// Keys are owner-declared SQL expressions, never request text. Feedback can expand
    /// its flags here so a record counts under each kind while its overall count stays one.
    pub fn groups(
        &self,
        keys: &[String],
        sums: &[(&str, String)],
        expansion: &str,
    ) -> Result<Vec<Value>> {
        let mut fields: Vec<String> = keys
            .iter()
            .enumerate()
            .map(|(i, key)| format!("{key} k{i}"))
            .collect();
        fields.push("COUNT(*) total".into());
        fields.extend(
            sums.iter()
                .map(|(name, expr)| format!("COALESCE(SUM({expr}),0) {name}")),
        );
        let scope = if expansion.is_empty() {
            self.scope.clone()
        } else {
            self.scope
                .replacen(" WHERE ", &format!(" {expansion} WHERE "), 1)
        };
        self.data.all(
            &format!(
                "SELECT {}{} GROUP BY {}",
                fields.join(","),
                scope,
                (0..keys.len())
                    .map(|i| format!("k{i}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            rusqlite::params_from_iter(&self.params),
        )
    }
    pub fn list(&self, offset: usize, limit: usize) -> Result<Vec<Row>> {
        let sql = format!(
            "SELECT x.week,COALESCE(x.transporter_id,'') transporter_id,COALESCE(x.tracking_id,'') tracking_id,x.row{} \
             ORDER BY json_extract(x.row,'$.{}') DESC,x.row_index,x.publication_id LIMIT ? OFFSET ?",
            self.scope, self.date
        );
        let mut params = self.params.clone();
        params.extend([limit.to_string(), offset.min(i64::MAX as usize).to_string()]);
        Ok(self
            .data
            .all(&sql, rusqlite::params_from_iter(&params))?
            .iter()
            .map(|r| Row {
                transporter_id: s(r, "transporter_id").into(),
                tracking_id: s(r, "tracking_id").into(),
                data: serde_json::from_str(s(r, "row")).unwrap_or_default(),
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "../tests/backend/mcp/query.rs"]
mod tests;
