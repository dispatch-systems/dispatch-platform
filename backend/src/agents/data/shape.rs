//! How answers are laid out for a model to read cheaply: what was understood first, then
//! tables whose column names appear once, cut into pages that keep every answer within a
//! budget every agent can take whole.
use super::{
    Refusal,
    scope::{Period, param, today},
};
use crate::contracts::Dsp;
use serde_json::{Map, Value, json};

/// The most an answer may weigh, in bytes of JSON: about 8,000 tokens, under every agent's
/// limit for one tool result (Codex cuts at about 40 KB, Claude Code warns at 10,000 tokens,
/// Hermes moves results over 50,000 characters to a file).
pub const BUDGET: usize = 24_000;

/// A table: the column names once, then one array of values per row.
pub struct Table {
    pub columns: Vec<&'static str>,
    pub rows: Vec<Vec<Value>>,
}
impl Table {
    pub fn new(columns: &[&'static str]) -> Self {
        Self {
            columns: columns.to_vec(),
            rows: vec![],
        }
    }
    pub fn push(&mut self, row: Vec<Value>) {
        self.rows.push(row);
    }
    fn value(&self, rows: &[Vec<Value>]) -> Value {
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
    let cursor = param(query, "cursor");
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
/// for the next page, which the answer names with `next_cursor` and says so in words.
pub fn page(
    answer: &mut Value,
    key: &str,
    mut table: Table,
    offset: usize,
    total: usize,
    limit: usize,
) {
    table.rows.truncate(limit);
    let mut shown = table.rows.len();
    answer[key] = table.value(&table.rows[..shown]);
    // Halve the rows, then add back one at a time: the budget is met in a few
    // serializations whatever the size.
    if answer.to_string().len() > BUDGET {
        let (mut low, mut high) = (0, shown);
        while low < high {
            let middle = (low + high).div_ceil(2);
            answer[key] = table.value(&table.rows[..middle]);
            if answer.to_string().len() + 200 <= BUDGET {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        shown = low;
        answer[key] = table.value(&table.rows[..shown]);
    }
    let next = offset + shown;
    if next < total {
        answer["page"] =
            json!({"returned": shown, "total": total, "next_cursor": next.to_string()});
        answer["note"] = json!(format!(
            "Rows {}–{} of {total}. Ask again with cursor \"{next}\" for the next page.",
            offset + 1,
            next
        ));
    }
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
    );
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_stops_at_the_budget_and_names_the_next() {
        let mut table = Table::new(&["n", "text"]);
        for n in 0..2_000 {
            table.push(vec![json!(n), json!("x".repeat(40))]);
        }
        let mut answer = json!({"understood": {}});
        page(&mut answer, "rows", table, 0, 2_000, 500);
        assert!(answer.to_string().len() <= BUDGET);
        let shown = answer["rows"]["rows"].as_array().unwrap().len();
        assert!(shown > 300 && shown < 500, "{shown}");
        assert_eq!(answer["page"]["next_cursor"], shown.to_string());
        assert!(answer["note"].as_str().unwrap().contains("of 2000"));

        let mut small = Table::new(&["n"]);
        small.push(vec![json!(1)]);
        let mut answer = json!({});
        page(&mut answer, "rows", small, 0, 1, 50);
        assert!(answer.get("page").is_none());
    }
    #[test]
    fn days_read_as_ranges() {
        let days: Vec<String> = ["2026-09-01", "2026-09-02", "2026-09-03", "2026-09-05"]
            .iter()
            .map(|d| d.to_string())
            .collect();
        assert_eq!(
            ranges(&days),
            json!(["2026-09-01..2026-09-03", "2026-09-05"])
        );
    }
}
