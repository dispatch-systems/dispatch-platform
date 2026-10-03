//! Historical daily sources loaded once for a range, including guarded employee overlays.
use dispatch_core::{
    Result,
    db::{Store, s},
    ensure,
    foundation::validate,
};
use dispatch_paycom as paycom;
use rusqlite::params;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default)]
pub(crate) struct DailySource {
    pub publication: Option<Value>,
    pub roster: Arc<BTreeMap<String, Value>>,
    pub rows: BTreeMap<String, Value>,
    pub available: bool,
}

pub(crate) fn daily_sources(
    store: &Store,
    id: &str,
    from: &str,
    to: &str,
    codes: Option<&[String]>,
) -> Result<BTreeMap<String, DailySource>> {
    validate::date(from)?;
    validate::date(to)?;
    ensure(from <= to, "invalid_date", 400)?;
    let first = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d").unwrap();
    let last = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d").unwrap();
    let db = store.collector(id, paycom::PROVIDER)?;
    let publications = db.all(
        "SELECT id,period_from,period_to,collected_at FROM publications \
         WHERE period_from<=? AND period_to>=? ORDER BY collected_at DESC,id DESC LIMIT ?",
        params![to, from, if from == to { 1 } else { -1 }],
    )?;
    let mut days = BTreeMap::new();
    for day in first.iter_days().take_while(|day| *day <= last) {
        let day = day.to_string();
        let publication = publications
            .iter()
            .find(|p| s(p, "period_from") <= day.as_str() && s(p, "period_to") >= day.as_str())
            .map(|p| json!({"id":p["id"],"collected_at":p["collected_at"]}));
        days.insert(
            day,
            DailySource {
                available: publication.is_some(),
                publication,
                ..DailySource::default()
            },
        );
    }
    let ids: std::collections::BTreeSet<&str> = days
        .values()
        .filter_map(|source| source.publication.as_ref().map(|p| s(p, "id")))
        .collect();
    let ids = serde_json::to_string(&ids)?;
    let mut rosters: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    for mut row in db.all(
        "SELECT publication_id,code,name FROM employees \
         WHERE publication_id IN (SELECT value FROM json_each(?)) ORDER BY code",
        [&ids],
    )? {
        let publication = s(&row, "publication_id").to_owned();
        row.as_object_mut().unwrap().remove("publication_id");
        rosters
            .entry(publication)
            .or_default()
            .insert(s(&row, "code").to_owned(), row);
    }
    let rosters: BTreeMap<_, _> = rosters
        .into_iter()
        .map(|(id, roster)| (id, Arc::new(roster)))
        .collect();
    for source in days.values_mut() {
        if let Some(roster) = source
            .publication
            .as_ref()
            .and_then(|p| rosters.get(s(p, "id")))
        {
            source.roster = roster.clone();
        }
    }
    let selected = codes.map(serde_json::to_string).transpose()?;
    for mut row in super::daily::cards(
        &db,
        "SELECT t.publication_id,t.employee_code employeeCode,e.name,e.department,e.station,\
         t.date,t.hours,t.status,t.punches,u.url sourceUrl FROM timecards t \
         JOIN employees e ON e.publication_id=t.publication_id AND e.code=t.employee_code \
         LEFT JOIN timecard_sources u ON u.publication_id=t.publication_id AND u.employee_code=t.employee_code \
         WHERE t.publication_id IN (SELECT value FROM json_each(?1)) AND t.date BETWEEN ?2 AND ?3 \
         AND (?4 IS NULL OR t.employee_code IN (SELECT value FROM json_each(?4)))",
        params![ids, from, to, selected],
    )? {
        let Some(source) = days.get_mut(s(&row, "date")) else {
            continue;
        };
        if source
            .publication
            .as_ref()
            .is_none_or(|p| p["id"] != row["publication_id"])
        {
            continue;
        }
        row.as_object_mut().unwrap().remove("publication_id");
        source.rows.insert(s(&row, "employeeCode").to_owned(), row);
    }
    let wanted = |code: &str| codes.is_none_or(|codes| codes.iter().any(|c| c == code));
    for sync in db.all(
        "SELECT period_from,period_to,collected_at,data FROM employee_timecard_syncs \
         WHERE period_from<=? AND period_to>=? ORDER BY collected_at,employee_code",
        [to, from],
    )? {
        let data: Value = serde_json::from_str(s(&sync, "data"))?;
        let employee = &data["employees"][0];
        let code = s(employee, "code");
        let cards: BTreeMap<_, _> = super::sync::synced_cards(&data)
            .into_iter()
            .map(|card| (s(&card, "date").to_owned(), card))
            .collect();
        for (date, source) in &mut days {
            if date.as_str() < s(&sync, "period_from")
                || date.as_str() > s(&sync, "period_to")
                || s(&sync, "collected_at")
                    < source
                        .publication
                        .as_ref()
                        .map_or("", |p| s(p, "collected_at"))
            {
                continue;
            }
            Arc::make_mut(&mut source.roster)
                .insert(code.into(), json!({"code":code,"name":employee["name"]}));
            if let Some(card) = cards.get(date) {
                source.available = true;
                if wanted(code) {
                    let mut card = card.clone();
                    for key in ["name", "department", "station"] {
                        card[key] = employee[key].clone();
                    }
                    source.rows.insert(code.into(), card);
                }
            }
        }
    }
    for (date, live) in store.live_results_range(id, paycom::PROVIDER, from, to)? {
        let Some(source) = days.get_mut(&date) else {
            continue;
        };
        for (metadata, items) in live {
            for employee in metadata["roster"].as_array().into_iter().flatten() {
                Arc::make_mut(&mut source.roster).insert(
                    s(employee, "code").to_owned(),
                    json!({"code":employee["code"],"name":employee["name"]}),
                );
            }
            source.available |= !items.is_empty();
            for row in items {
                let code = s(&row, "employeeCode");
                if wanted(code) {
                    source.rows.insert(code.to_owned(), row);
                }
            }
        }
    }
    Ok(days)
}
