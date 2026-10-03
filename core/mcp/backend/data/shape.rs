//! How answers are laid out for a model to read cheaply: what was understood first, then
//! tables whose column names appear once, cut into pages that keep every answer within a
//! budget every agent can take whole.
use super::{
    Refusal,
    scope::{Period, param, today},
};
use crate::accounts::api::types::Dsp;
use serde_json::{Map, Value, json};

/// The most an answer may weigh, in bytes of JSON: about 8,000 tokens, under every agent's
/// limit for one tool result (Codex cuts at about 40 KB, Claude Code warns at 10,000 tokens,
/// Hermes moves results over 50,000 characters to a file).
pub const BUDGET: usize = 24_000;
/// Room a page leaves within the budget for `bypassed`, which the gate adds to an answer
/// read by bypassing its feature once the answer is made: every switch's name fits.
const BYPASSED_ROOM: usize = 100;

/// A table: the column names once, then one array of values per row.
pub struct Table {
    pub columns: Vec<&'static str>,
    pub rows: Vec<Vec<Value>>,
    budget: usize,
}
impl Table {
    pub fn new(columns: &[&'static str]) -> Self {
        Self {
            columns: columns.to_vec(),
            rows: vec![],
            budget: BUDGET,
        }
    }
    pub fn with_budget(mut self, budget: usize) -> Self {
        self.budget = budget;
        self
    }
    pub fn push(&mut self, row: Vec<Value>) {
        self.rows.push(row);
    }
    pub fn value(&self, rows: &[Vec<Value>]) -> Value {
        json!({"columns": self.columns, "rows": rows})
    }
}

/// What an answer understood: the DSP, its date today and the days asked about.
pub fn understood(dsp: &Dsp, period: Option<&Period>) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert("dsp".into(), json!(dsp.name));
    out.insert("today".into(), json!(today(dsp).to_string()));
    if let Some(period) = period {
        out.insert("period".into(), json!(period.label));
        out.insert("from".into(), json!(period.first()));
        out.insert("to".into(), json!(period.last()));
        out.insert(
            "days".into(),
            json!((period.to - period.from).num_days() + 1),
        );
    }
    out
}

/// Where a page starts: the `cursor` an earlier answer gave, or the beginning.
pub fn offset(query: &Value) -> Result<usize, Refusal> {
    offset_named(query, "cursor")
}
pub fn offset_named(query: &Value, name: &str) -> Result<usize, Refusal> {
    let cursor = param(query, name);
    if cursor.is_empty() {
        return Ok(0);
    }
    cursor.parse().map_err(|_| {
        Refusal::new(
            400,
            "invalid_cursor",
            "That cursor is not one this tool gave; ask again without it to start over.",
        )
    })
}
/// How many rows a page holds at most: the request's `limit`, or the tool's own.
pub fn limit(query: &Value, default: usize) -> usize {
    param(query, "limit").parse().unwrap_or(default)
}

/// Puts one page of `table` into `answer` under `key`. `table` holds the rows from `offset`
/// on, and `total` counts every row there is. Rows past `limit`, or past the budget, wait
/// for the next page, which the table itself names with `next_cursor` and says so in words,
/// so two tables in one answer never share a cursor.
pub fn page(
    answer: &mut Value,
    key: &str,
    table: Table,
    offset: usize,
    total: usize,
    limit: usize,
) -> Result<(), Refusal> {
    page_named(answer, key, table, offset, total, limit, "cursor")
}

/// A page with its own request parameter when an answer contains independent tables.
pub fn page_named(
    answer: &mut Value,
    key: &str,
    mut table: Table,
    offset: usize,
    total: usize,
    limit: usize,
    cursor: &str,
) -> Result<(), Refusal> {
    table.rows.truncate(limit);
    let mut shown = table.rows.len();
    let value = |shown: usize| {
        let mut value = table.value(&table.rows[..shown]);
        let next = offset + shown;
        if next < total {
            value["page"] =
                json!({"returned": shown, "total": total, "next_cursor": next.to_string()});
            value["note"] = json!(format!(
                "Rows {}–{} of {total}. Ask again with {cursor} \"{next}\" for the next page.",
                offset + 1,
                next
            ));
        }
        value
    };
    answer[key] = value(shown);
    // Count the final page, including its metadata and every other table in the answer.
    let room = BUDGET - BYPASSED_ROOM;
    if answer.to_string().len() > room || answer[key].to_string().len() > table.budget {
        let (mut low, mut high) = (0, shown);
        while low < high {
            let middle = (low + high).div_ceil(2);
            answer[key] = value(middle);
            if answer.to_string().len() <= room && answer[key].to_string().len() <= table.budget {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        shown = low;
    }
    answer[key] = value(shown);
    let next = offset + shown;
    if shown == 0 && next < total {
        // A cursor that cannot move on would send an agent round the same page.
        return Err(Refusal::new(
            400,
            "answer_too_large",
            "One row is larger than an answer may hold; ask for the summary instead.",
        ));
    }
    check_budget(answer)?;
    Ok(())
}

/// Also covers summaries with no pageable rows, for both REST and MCP.
pub fn check_budget(answer: &Value) -> Result<(), Refusal> {
    if answer.to_string().len() > BUDGET {
        return Err(Refusal::new(
            400,
            "answer_too_large",
            "The answer is too large; ask for fewer days, a driver, or the summary.",
        ));
    }
    Ok(())
}
/// [`page`] for rows already in memory: skips to the request's cursor first.
pub fn paged(
    answer: &mut Value,
    key: &str,
    mut table: Table,
    query: &Value,
    default_limit: usize,
) -> Result<(), Refusal> {
    let start = offset(query)?;
    let total = table.rows.len();
    table.rows.drain(..start.min(total));
    page(
        answer,
        key,
        table,
        start,
        total,
        limit(query, default_limit),
    )
}

/// Consecutive dates written as ranges, as "2026-09-01..2026-09-04", at most ten of them.
pub fn ranges(days: &[String]) -> Value {
    let parse = |d: &String| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
    let mut spans: Vec<(String, String)> = vec![];
    for day in days {
        match spans.last_mut() {
            Some((_, end))
                if parse(end)
                    .zip(parse(day))
                    .is_some_and(|(a, b)| (b - a).num_days() == 1) =>
            {
                *end = day.clone();
            }
            _ => spans.push((day.clone(), day.clone())),
        }
    }
    let mut out: Vec<Value> = spans
        .iter()
        .take(10)
        .map(|(a, b)| {
            json!(if a == b {
                a.clone()
            } else {
                format!("{a}..{b}")
            })
        })
        .collect();
    if spans.len() > 10 {
        out.push(json!(format!("and {} more spans", spans.len() - 10)));
    }
    Value::Array(out)
}

/// Hours to two decimals.
pub fn hours(value: f64) -> Value {
    json!((value * 100.0).round() / 100.0)
}
