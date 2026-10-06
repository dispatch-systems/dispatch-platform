//! SQL filters and counts run before decoding a bounded page of daily rows.
use super::fields;
use crate::DailyPerformanceStore;
use dispatch_core::{
    State,
    db::{Store, s},
    mcp::{
        Caller,
        api::types::DriverSource,
        data::{
            Answer, Refusal,
            access::Access,
            catalog,
            scope::{People, param, period, today},
            shape::{Table, limit, offset, offset_named, page, page_named, understood},
        },
    },
};
use serde_json::{Value, json};
fn normalized(field: &str) -> String {
    format!("replace(replace(lower(COALESCE(json_extract(x.row,'$.{field}'),'')),' ','_'),'-','_')")
}
fn event_type() -> String {
    "replace(replace(lower(COALESCE(json_extract(x.row,'$.type'),\
        json_extract(x.row,'$.dashboard_metric_type'),'')),' ','_'),'-','_')"
        .into()
}
pub(super) fn daily_performance(
    db: &Store,
    state: &State,
    caller: &Caller,
    query: &Value,
) -> Answer {
    catalog::check("daily_performance", query)?;
    read(db, state, caller, query)
}
pub(super) fn read(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
    let period = period(query, today(dsp), "yesterday")?;
    let dataset = match param(query, "dataset") {
        "" => "driver_quality",
        value => value,
    };
    let descriptor = dispatch_cortex::daily_performance::dataset(dataset)
        .ok_or_else(|| Refusal::new(400, "unknown_dataset", "Choose a daily dataset."))?;
    let dataset = descriptor.table;
    let driver_dataset = dataset.starts_with("driver_") && dataset != "driver_thresholds"
        || matches!(
            dataset,
            "returns_to_station" | "safety_events" | "live_safety_events"
        );
    let feedback = matches!(dataset, "driver_feedback" | "dsp_feedback");
    if !driver_dataset && !param(query, "driver").is_empty() {
        return Err(Refusal::new(
            400,
            "invalid_filter",
            "This dataset has no individual driver rows.",
        )
        .into());
    }
    let feedback_type = if feedback && !param(query, "type").is_empty() {
        let wanted = param(query, "type")
            .trim()
            .to_lowercase()
            .replace([' ', '-'], "_");
        Some(
            fields::FEEDBACK_TYPES
                .iter()
                .find(|(name, field)| wanted == *name || wanted == *field)
                .map(|(_, field)| *field)
                .ok_or_else(|| {
                    Refusal::new(400, "unknown_type", "Choose a daily feedback category.").choices(
                        fields::FEEDBACK_TYPES
                            .iter()
                            .map(|(name, _)| (*name).into())
                            .collect(),
                    )
                })?,
        )
    } else {
        None
    };
    let fields = if param(query, "fields").is_empty() {
        fields::of(dataset).to_vec()
    } else {
        let wanted: Vec<&str> = param(query, "fields").split(',').map(str::trim).collect();
        let allowed = fields::of(dataset);
        if wanted.iter().any(|field| !allowed.contains(field)) {
            return Err(Refusal::new(
                400,
                "unknown_field",
                "Choose verified fields for this daily dataset.",
            )
            .choices(allowed.iter().map(|field| (*field).into()).collect())
            .into());
        }
        wanted
            .into_iter()
            .map(|wanted| {
                *allowed
                    .iter()
                    .find(|field| **field == wanted)
                    .expect("checked field")
            })
            .collect()
    };
    let station = db.profile(&dsp.id)?.station_code;
    let data = db.daily_performance_db(&dsp.id)?;
    let people = People::load(db, state, &access)?;
    let mut understood = understood(dsp, Some(&period));
    let mut scope = String::from(
        " FROM daily_rows x JOIN daily_publications p ON p.id=x.publication_id \
        WHERE p.active=1 AND p.station=? AND x.dataset=? AND x.date BETWEEN ? AND ?",
    );
    let mut args = vec![
        station.clone(),
        dataset.into(),
        period.first(),
        period.last(),
    ];
    if !param(query, "driver").is_empty() {
        let wanted = param(query, "driver");
        let (name, ids) = match people.find(wanted) {
            Ok(person) => (person.name.clone(), person.amazon.clone()),
            Err(refusal) => {
                // An exact source ID remains usable in a build without Driver Match.
                let mut direct = args.clone();
                direct.push(wanted.into());
                let row=data.one(&format!("SELECT json_extract(x.row,'$.da_name') name{scope} AND x.transporter_id=? LIMIT 1"),
                    rusqlite::params_from_iter(&direct))?;
                let Some(row) = row else {
                    return Err(refusal.into());
                };
                (
                    if s(&row, "name").is_empty() {
                        wanted.into()
                    } else {
                        s(&row, "name").into()
                    },
                    vec![wanted.into()],
                )
            }
        };
        understood.insert("driver".into(), json!(name));
        scope.push_str(" AND x.transporter_id IN (SELECT value FROM json_each(?))");
        args.push(serde_json::to_string(&ids).map_err(dispatch_core::Error::from)?);
    }
    for (name, field, allowed) in [
        ("reason", "rts_reason_code", dataset == "returns_to_station"),
        (
            "type",
            "type",
            matches!(dataset, "safety_events" | "live_safety_events") || feedback,
        ),
    ] {
        let wanted = param(query, name);
        if !wanted.is_empty() {
            if !allowed {
                return Err(Refusal::new(
                    400,
                    "invalid_filter",
                    format!("{name} does not apply to {dataset}."),
                )
                .into());
            }
            if let Some(field) = feedback_type
                && name == "type"
            {
                scope.push_str(&format!(" AND json_extract(x.row,'$.{field}')>0"));
                continue;
            }
            let field = if name == "type" {
                event_type()
            } else {
                normalized(field)
            };
            scope.push_str(&format!(" AND instr({field},?)>0"));
            args.push(wanted.trim().to_lowercase().replace([' ', '-'], "_"));
        }
    }
    if !param(query, "impacting").is_empty() {
        if descriptor.impact.is_none() {
            return Err(Refusal::new(
                400,
                "invalid_filter",
                "This dataset has no verified impact flag.",
            )
            .into());
        }
        scope.push_str(" AND x.impact=?");
        args.push(
            if catalog::flag(query, "impacting") {
                "1"
            } else {
                "0"
            }
            .into(),
        );
    }
    if !param(query, "contact").is_empty() && dataset != "returns_to_station" {
        return Err(Refusal::new(
            400,
            "invalid_filter",
            "Contact coaching belongs to daily returns.",
        )
        .into());
    }
    if param(query, "contact") == "missed" {
        if dataset != "returns_to_station" {
            return Err(Refusal::new(
                400,
                "invalid_filter",
                "Contact coaching belongs to daily returns.",
            )
            .into());
        }
        scope.push_str(
            " AND (instr(lower(COALESCE(json_extract(x.row,'$.daily_coaching'),'')),'contact')>0 \
            OR instr(lower(COALESCE(json_extract(x.row,'$.daily_coaching'),'')),'call')>0 \
            OR instr(lower(COALESCE(json_extract(x.row,'$.daily_coaching'),'')),'text')>0)",
        );
    }
    if param(query, "contact") == "compliant" {
        if dataset != "returns_to_station" {
            return Err(Refusal::new(
                400,
                "invalid_filter",
                "Contact coaching belongs to daily returns.",
            )
            .into());
        }
        scope.push_str(" AND instr(lower(COALESCE(json_extract(x.row,'$.daily_exemption_reason'),'')),'contact compliant')>0");
    }
    if !param(query, "counting").is_empty() {
        if dataset != "safety_events" {
            return Err(Refusal::new(
                400,
                "invalid_filter",
                "Reviewed outcomes belong to assessed daily safety events.",
            )
            .into());
        }
        scope.push_str(" AND trim(COALESCE(json_extract(x.row,'$.final_resolution'),''))<>''");
        scope.push_str(&format!(
            " AND {} {} 'dispute_approved'",
            normalized("final_resolution"),
            if catalog::flag(query, "counting") {
                "<>"
            } else {
                "="
            }
        ));
    }
    if !param(query, "feedback").is_empty() {
        if !matches!(dataset, "driver_feedback" | "dsp_feedback") {
            return Err(Refusal::new(
                400,
                "invalid_filter",
                "Feedback selection belongs to daily feedback counts.",
            )
            .into());
        }
        if param(query, "feedback") != "all" {
            let field = if param(query, "feedback") == "positive" {
                "positive_response_cnt"
            } else {
                "negative_response_cnt"
            };
            scope.push_str(&format!(" AND json_extract(x.row,'$.{field}')>0"));
        }
    }
    let recorded = data.count(
        &("SELECT count(*)".to_owned() + &scope),
        rusqlite::params_from_iter(&args),
    )? as usize;
    let coverage = data.all("SELECT p.date,d.coverage FROM daily_publications p JOIN daily_datasets d ON d.publication_id=p.id \
        WHERE p.active=1 AND p.station=? AND d.dataset=? AND p.date BETWEEN ? AND ? ORDER BY p.date",
        [&station,dataset,&period.first(),&period.last()])?;
    let observed = coverage
        .iter()
        .filter(|row| row["coverage"] == "observed")
        .count();
    let days = (period.to - period.from).num_days() + 1;
    if observed == 0 {
        return Err(Refusal::new(
            404,
            "data_unavailable",
            format!(
                "No confirmed {dataset} data in {}. Empty source responses do not establish zero.",
                period.label
            ),
        )
        .into());
    }
    let dates: Vec<String> = coverage
        .iter()
        .filter(|row| row["coverage"] == "observed")
        .map(|row| s(row, "date").into())
        .collect();
    let mut answer = json!({"understood":understood,"source":"daily_performance","dataset":dataset,"recorded_rows":recorded,
        "coverage":{"status":if observed as i64 == days {"complete"} else {"partial"},
            "observed_days":observed,"requested_days":days,
            "observed":dispatch_core::mcp::data::shape::ranges(&dates),
            "note":"Counts cover recorded source rows. Missing or empty datasets are unconfirmed."}});
    if feedback {
        let totals = data.one(
            &format!(
                "SELECT SUM(json_extract(x.row,'$.positive_response_cnt')) positive,\
            SUM(json_extract(x.row,'$.negative_response_cnt')) negative{scope}"
            ),
            rusqlite::params_from_iter(&args),
        )?;
        answer["feedback_counts"] = json!(totals);
        if let Some(field) = feedback_type {
            answer["feedback_counts"]["selected_type"] = json!({"field":field,
                "count":data.one(&format!("SELECT SUM(json_extract(x.row,'$.{field}')) total{scope}"),
                    rusqlite::params_from_iter(&args))?.map(|row| row["total"].clone())});
        }
    }
    people.mark(&mut answer);
    let groups: Vec<&'static str> = if param(query, "group_by").is_empty() {
        vec![]
    } else {
        param(query, "group_by")
            .split(',')
            .map(|group| match group.trim() {
                "driver" => "driver",
                "day" => "day",
                "reason" => "reason",
                "type" => "type",
                _ => "invalid",
            })
            .collect()
    };
    if !groups.is_empty() {
        if groups.len() > 3
            || groups.iter().any(|group| {
                !["driver", "day", "reason", "type"].contains(group)
                    || *group == "driver" && !driver_dataset
                    || *group == "reason" && dataset != "returns_to_station"
                    || *group == "type"
                        && !matches!(dataset, "safety_events" | "live_safety_events")
            })
        {
            return Err(Refusal::new(
                400,
                "invalid_group",
                "Group by driver, day, reason or type, up to three keys.",
            )
            .into());
        }
        let expressions: Vec<String> = groups
            .iter()
            .map(|group| match *group {
                "driver" => "COALESCE(x.transporter_id,'')".into(),
                "day" => "x.date".into(),
                "reason" => normalized("rts_reason_code"),
                _ => event_type(),
            })
            .collect();
        let select = expressions
            .iter()
            .enumerate()
            .map(|(index, expression)| format!("{expression} k{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let grouping = (0..groups.len())
            .map(|index| format!("k{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let feedback_sums = if feedback {
            ",SUM(json_extract(x.row,'$.positive_response_cnt')) positive,\
             SUM(json_extract(x.row,'$.negative_response_cnt')) negative"
        } else {
            ""
        };
        let sql =
            format!("SELECT {select},count(*) rows{feedback_sums}{scope} GROUP BY {grouping}");
        let total = data.count(
            &format!("SELECT count(*) FROM ({sql})"),
            rusqlite::params_from_iter(&args),
        )? as usize;
        let (start, take) = (offset_named(query, "groups_cursor")?, limit(query, 50));
        let mut paging = args.clone();
        paging.extend([take.to_string(), start.min(i64::MAX as usize).to_string()]);
        let rows = data.all(
            &format!("{sql} ORDER BY rows DESC,{grouping} LIMIT ? OFFSET ?"),
            rusqlite::params_from_iter(&paging),
        )?;
        let mut columns = groups.clone();
        columns.push("recorded_rows");
        if feedback {
            columns.extend(["positive_responses", "negative_responses"]);
        }
        let mut table = Table::new(&columns);
        for row in rows {
            let mut values: Vec<Value> = groups
                .iter()
                .enumerate()
                .map(|(index, group)| {
                    let key = format!("k{index}");
                    if *group == "driver" {
                        json!(
                            people
                                .holder(DriverSource::Amazon, s(&row, &key))
                                .map_or_else(
                                    || s(&row, &key).to_owned(),
                                    |person| person.name.clone()
                                )
                        )
                    } else {
                        row[&key].clone()
                    }
                })
                .collect();
            values.push(row["rows"].clone());
            if feedback {
                values.extend([row["positive"].clone(), row["negative"].clone()]);
            }
            table.push(values);
        }
        page_named(
            &mut answer,
            "groups",
            table,
            start,
            total,
            take,
            "groups_cursor",
        )?;
    }
    if param(query, "detail") == "full" || catalog::flag(query, "list") {
        let (start, take) = (offset(query)?, limit(query, 50));
        let projection = fields
            .iter()
            .map(|field| format!(",json_extract(x.row,'$.{field}') [{field}]"))
            .collect::<String>();
        let mut paging = args.clone();
        paging.extend([take.to_string(), start.min(i64::MAX as usize).to_string()]);
        let rows = data.all(
            &format!(
                "SELECT x.date,x.transporter_id,json_extract(x.row,'$.da_name') da_name{projection}{scope} \
            ORDER BY x.date DESC,x.publication_id,x.row_index LIMIT ? OFFSET ?"
            ),
            rusqlite::params_from_iter(&paging),
        )?;
        let mut columns = vec!["date", "driver"];
        columns.extend(&fields);
        let mut table = Table::new(&columns);
        for row in rows {
            let driver = people
                .holder(DriverSource::Amazon, s(&row, "transporter_id"))
                .map_or_else(
                    || {
                        if s(&row, "da_name").is_empty() {
                            s(&row, "transporter_id").to_owned()
                        } else {
                            s(&row, "da_name").to_owned()
                        }
                    },
                    |person| person.name.clone(),
                );
            let mut values = vec![row["date"].clone(), json!(driver)];
            values.extend(fields.iter().map(|field| row[*field].clone()));
            table.push(values);
        }
        page(&mut answer, "list", table, start, recorded, take)?;
    }
    Ok(answer)
}
