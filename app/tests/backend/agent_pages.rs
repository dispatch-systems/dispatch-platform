//! A page of an agent's answer, with room for every feature a gate may name as bypassed.
use crate::agents::data::shape::*;
use serde_json::json;

#[test]
fn a_page_stops_at_the_budget_and_names_the_next() {
    let mut table = Table::new(&["n", "text"]);
    for n in 0..2_000 {
        table.push(vec![json!(n), json!("x".repeat(40))]);
    }
    let mut answer = json!({"understood": {}});
    page(&mut answer, "rows", table, 0, 2_000, 500).unwrap();
    assert!(answer.to_string().len() <= BUDGET);
    let shown = answer["rows"]["rows"].as_array().unwrap().len();
    assert!(shown > 300 && shown < 500, "{shown}");
    // With room left for every feature a gate may name as bypassed.
    let switches: Vec<&str> = crate::contracts::AgentSource::all()
        .map(|source| source.switch())
        .collect();
    answer["bypassed"] = json!(switches);
    assert!(answer.to_string().len() <= BUDGET);
    // The cursor belongs to the table it pages.
    assert_eq!(answer["rows"]["page"]["next_cursor"], shown.to_string());
    assert!(answer["rows"]["note"].as_str().unwrap().contains("of 2000"));
    assert!(answer.get("page").is_none());

    let mut small = Table::new(&["n"]);
    small.push(vec![json!(1)]);
    let mut answer = json!({});
    page(&mut answer, "rows", small, 0, 1, 50).unwrap();
    assert!(answer["rows"].get("page").is_none());

    // A row past the budget alone is refused, never a cursor that stays put.
    let mut huge = Table::new(&["text"]);
    huge.push(vec![json!("x".repeat(BUDGET))]);
    huge.push(vec![json!("y")]);
    let mut answer = json!({});
    assert_eq!(
        page(&mut answer, "rows", huge, 0, 2, 50).unwrap_err().code,
        "answer_too_large"
    );
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

#[test]
fn pagination_metadata_is_counted_even_when_rows_alone_fit() {
    let mut table = Table::new(&["text"]);
    table.push(vec![json!("x".repeat(23_956))]);
    let mut answer = json!({});
    assert!(json!({"rows":table.value(&table.rows)}).to_string().len() <= BUDGET);
    assert_eq!(
        page(&mut answer, "rows", table, 0, 2, 1).unwrap_err().code,
        "answer_too_large"
    );
}

#[test]
fn multiple_tables_and_unpageable_summaries_share_one_budget() {
    let mut answer = json!({"summary":"x".repeat(BUDGET)});
    assert!(check_budget(&answer).is_err());
    assert!(page(&mut answer, "rows", Table::new(&["n"]), 0, 0, 10).is_err());
    let mut answer = json!({});
    for key in ["first", "second"] {
        let mut table = Table::new(&["text"]);
        for _ in 0..100 {
            table.push(vec![json!("x".repeat(200))]);
        }
        page(&mut answer, key, table, 0, 100, 100).unwrap();
    }
    assert!(answer.to_string().len() <= BUDGET);
    assert!(answer["second"]["page"]["next_cursor"].is_string());
}
