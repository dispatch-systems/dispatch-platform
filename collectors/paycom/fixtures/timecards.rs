use dispatch_core::{Result, db::iso};
use jiff::ToSpan;
use serde_json::{Value, json};
pub fn fixture(timezone: &str) -> Result<Value> {
    fixture_date(timezone, None)
}
pub fn fixture_date(timezone: &str, selected: Option<jiff::civil::Date>) -> Result<Value> {
    let tz = dispatch_core::foundation::clock::zone(timezone)?;
    let today = selected.unwrap_or_else(|| dispatch_core::foundation::clock::today(&tz));
    // Demo periods follow the known Sep 6–19 cycle. Real collections use Paycom's bounds.
    let anchor = jiff::civil::date(2026, 9, 6);
    let start = anchor + (i64::from((today - anchor).get_days()).div_euclid(14) * 14).days();
    let end = start + 13.days();
    let dates: Vec<_> = (0..7)
        .map(|i| today - (6 - i).days())
        .filter(|date| *date >= start)
        .map(|date| date.to_string())
        .collect();
    let names = [
        "Avery Morgan",
        "Jordan Ellis",
        "Morgan Reed",
        "Taylor Brooks",
        "Cameron Hayes",
        "Casey Rivera",
        "Riley Bennett",
        "Alex Parker",
        "Jamie Collins",
        "Drew Sullivan",
        "Sam Mitchell",
        "Quinn Foster",
    ];
    let employees: Vec<_> = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            json!({"code":format!("E{:03}",i+1),"name":name,
        "department":if i==0{"Operations"}else{"Delivery"},
        "position":if i==0{"Dispatcher"}else{"Delivery associate"},"station":"DEMO1","active":true})
        })
        .collect();
    let timecards:Vec<_>=names.iter().enumerate().flat_map(|(i,
        _)|dates.iter().map(move|date|json!({"employeeCode":format!("E{:03}",i+1),"date":date,
        "hours":if i%3==0{8.5}else{8.0},"status":"Complete","punches":[{"in":"08:00","out":"12:00","hours":4},
        {"in":"12:30","out":if i%3==0{"17:00"}else{"16:30"},"hours":if i%3==0{4.5}else{4.0}}]}))).collect();
    Ok(
        json!({"employees":employees,"timecards":timecards,"collectedAt":iso(),"from":start.to_string(),"to":end.to_string()}),
    )
}
