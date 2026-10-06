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
    read_impl(db, state, caller, query, false)
}
pub(super) fn operational(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    read_impl(db, state, caller, query, true)
}
fn read_impl(
    db: &Store,
    state: &State,
    caller: &Caller,
    query: &Value,
    operational: bool,
) -> Answer {
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
    if operational && dataset == "returns_to_station" {
        // Rank before annotation filters: corrected later snapshots replace earlier ones.
        // Later active captures can improve historical detail; history stays untouched.
        scope = String::from(
            " FROM (SELECT r.*,p.date snapshot_date,p.collected_at snapshot_collected_at,row_number() OVER (PARTITION BY \
            CASE WHEN COALESCE(r.tracking_id,'')<>'' AND COALESCE(r.transporter_id,'')<>'' \
                AND length(json_extract(r.row,'$.delivery_planned_date'))>=10 \
            THEN json_array(r.tracking_id,substr(json_extract(r.row,'$.delivery_planned_date'),1,10),r.transporter_id) \
            ELSE json_array(r.publication_id,r.row_index) END \
            ORDER BY p.date DESC,p.collected_at DESC,p.id DESC,r.row_index DESC) chosen \
            FROM daily_rows r JOIN daily_publications p ON p.id=r.publication_id \
            WHERE p.active=1 AND p.station=? AND r.dataset=? AND r.date>=? \
                AND substr(json_extract(r.row,'$.delivery_planned_date'),1,10) BETWEEN ? AND ?) x WHERE x.chosen=1",
        );
        args = vec![
            station.clone(),
            dataset.into(),
            period.first(),
            period.first(),
            period.last(),
        ];
    } else if operational && dataset == "safety_events" {
        scope = String::from(
            " FROM (SELECT r.*,row_number() OVER (PARTITION BY \
            CASE WHEN COALESCE(r.event_id,'')<>'' THEN r.event_id ELSE json_array(r.publication_id,r.dataset,r.row_index) END \
            ORDER BY CASE r.dataset WHEN 'safety_events' THEN 0 ELSE 1 END,p.date DESC,p.collected_at DESC,p.id DESC,r.row_index DESC) chosen \
            FROM daily_rows r JOIN daily_publications p ON p.id=r.publication_id \
            WHERE p.active=1 AND p.station=? AND r.dataset IN ('safety_events','live_safety_events') \
                AND r.date BETWEEN ? AND ?) x WHERE x.chosen=1",
        );
        args = vec![station.clone(), period.first(), period.last()];
    }
    let date_expression = if operational && dataset == "returns_to_station" {
        "substr(json_extract(x.row,'$.delivery_planned_date'),1,10)"
    } else {
        "x.date"
    };
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
        if operational && dataset == "returns_to_station" {
            scope = scope.replacen(
                ") x WHERE x.chosen=1",
                " AND r.transporter_id IN (SELECT value FROM json_each(?))) x WHERE x.chosen=1",
                1,
            );
        } else {
            scope.push_str(" AND x.transporter_id IN (SELECT value FROM json_each(?))");
        }
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
            scope.push_str(&format!(
                " AND {}",
                if operational && name == "reason" {
                    format!("{field}=?")
                } else {
                    format!("instr({field},?)>0")
                }
            ));
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
        if operational {
            scope.push_str(" AND x.dataset='safety_events'");
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
    let coverage = data.all(&format!("SELECT p.date,CASE WHEN max(d.coverage='observed') THEN 'observed' ELSE 'unconfirmed' END coverage \
        FROM daily_publications p JOIN daily_datasets d ON d.publication_id=p.id \
        WHERE p.active=1 AND p.station=? AND {} AND p.date BETWEEN ? AND ? GROUP BY p.date ORDER BY p.date",
        if operational && dataset == "safety_events" { "(?='safety_events' AND d.dataset IN ('safety_events','live_safety_events'))" } else { "d.dataset=?" }),
        [&station,dataset,&period.first(),&period.last()])?;
    let observed = coverage
        .iter()
        .filter(|row| row["coverage"] == "observed")
        .count();
    let days = (period.to - period.from).num_days() + 1;
    if observed == 0 && (!operational || recorded == 0) {
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
                "SELECT COALESCE(SUM(json_extract(x.row,'$.positive_response_cnt')),0) positive,\
            COALESCE(SUM(json_extract(x.row,'$.negative_response_cnt')),0) negative{scope}"
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
    if operational {
        answer["coverage"]["collected"] = json!(observed);
        answer["coverage"]["requested"] = json!(days);
        answer["coverage"]["unit"] = json!("days");
        answer["counts"] = json!({"records":recorded,"unit":match dataset {
            "returns_to_station" => "return_attempts", "safety_events" => "safety_events", _ => "driver_day_feedback_counts"
        }});
        answer
            .as_object_mut()
            .expect("answer")
            .remove("recorded_rows");
        if dataset == "returns_to_station" {
            let totals = data.one(&format!("SELECT COALESCE(sum(x.impact=1),0) hurting_dcr,\
                COALESCE(sum(instr(lower(COALESCE(json_extract(x.row,'$.daily_coaching'),'')),'contact')>0 \
                    OR instr(lower(COALESCE(json_extract(x.row,'$.daily_coaching'),'')),'call')>0 \
                    OR instr(lower(COALESCE(json_extract(x.row,'$.daily_coaching'),'')),'text')>0),0) contact_missed,\
                COALESCE(sum(COALESCE(x.tracking_id,'')='' OR COALESCE(x.transporter_id,'')=''),0) unidentified,\
                max(x.snapshot_date) latest_snapshot_date,max(x.snapshot_collected_at) collected_at{scope}"),rusqlite::params_from_iter(&args))?.unwrap_or_default();
            answer["counts"]["hurting_dcr"] = totals["hurting_dcr"].clone();
            answer["counts"]["contact_missed"] = totals["contact_missed"].clone();
            answer["counts"]["unidentified"] = totals["unidentified"].clone();
            answer["coverage"]["latest_snapshot_date"] = totals["latest_snapshot_date"].clone();
            answer["coverage"]["collected_at"] = totals["collected_at"].clone();
            answer["coverage"]["note"] = json!(
                "Latest active snapshots by tracking ID, delivery date and transporter ID. Delivery dates select attempts; missing identities remain separate. Daily annotations do not establish posted scorecard impact."
            );
            // Official aggregates are separate evidence, never invented detail rows.
            if ["reason", "impacting", "contact"]
                .iter()
                .all(|k| param(query, k).is_empty() || *k == "contact" && param(query, k) == "all")
            {
                let mut aggregate_scope = String::from(
                    " FROM daily_rows x JOIN daily_publications p ON p.id=x.publication_id \
                    WHERE p.active=1 AND p.station=? AND x.dataset='driver_returns' AND x.date BETWEEN ? AND ?",
                );
                let mut aggregate_args = vec![station.clone(), period.first(), period.last()];
                if !param(query, "driver").is_empty() {
                    aggregate_scope
                        .push_str(" AND x.transporter_id IN (SELECT value FROM json_each(?))");
                    aggregate_args.push(args.last().expect("driver filter").clone());
                }
                let reported = data.one(&format!("SELECT count(DISTINCT CASE WHEN json_extract(x.row,'$.rts_all') IS NOT NULL THEN x.date END) days,\
                    SUM(json_extract(x.row,'$.rts_all')) returns{aggregate_scope}"),rusqlite::params_from_iter(&aggregate_args))?.unwrap_or_default();
                if reported["returns"].is_number() {
                    answer["counts"]["reported_returns"] = reported["returns"].clone();
                    answer["coverage"]["reported_returns"] = json!({"observed_days":reported["days"],"requested_days":days,
                        "status":if reported["days"].as_i64()==Some(days) {"complete"} else {"partial"}});
                    answer["reconciliation"] = json!({"status":if reported["returns"].as_u64()==Some(recorded as u64) {"matched"} else {"mismatch"},
                        "note":"reported_returns sums collected driver_returns aggregates; records counts reconciled detail. Missing aggregate days remain unknown."});
                }
            }
        } else if dataset == "safety_events" {
            let totals = data
                .one(
                    &format!(
                        "SELECT COALESCE(sum(x.dataset='safety_events'),0) assessed,\
                COALESCE(sum(x.dataset='live_safety_events'),0) live_only{scope}"
                    ),
                    rusqlite::params_from_iter(&args),
                )?
                .unwrap_or_default();
            answer["counts"]["assessed"] = totals["assessed"].clone();
            answer["counts"]["live_only"] = totals["live_only"].clone();
            answer["coverage"]["note"] = json!(
                "Unique event IDs; assessed records replace live copies. Live-only events are pending assessment. Posted counting belongs to the weekly view. Empty datasets remain unconfirmed."
            );
        } else {
            let selected = match param(query, "feedback") {
                "positive" => answer["feedback_counts"]["positive"].clone(),
                "all" => {
                    json!({"positive":answer["feedback_counts"]["positive"],"negative":answer["feedback_counts"]["negative"]})
                }
                _ => answer["feedback_counts"]["negative"].clone(),
            };
            answer["counts"]["responses"] = selected;
            answer["counts"]["response_unit"] = json!("feedback_responses");
            answer["counts"]["selected_category"] =
                answer["feedback_counts"]["selected_type"].clone();
            answer
                .as_object_mut()
                .expect("answer")
                .remove("feedback_counts");
            answer["coverage"]["note"] = json!(
                "Daily response counts, not individual package reviews. Rows cover only matching driver-days."
            );
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
                "day" => date_expression.into(),
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
        columns.push(if operational {
            "records"
        } else {
            "recorded_rows"
        });
        if feedback {
            columns.extend(["positive_responses", "negative_responses"]);
        }
        let mut table = Table::new(&columns);
        if operational && catalog::flag(query, "list") {
            table = table.with_budget(dispatch_core::mcp::data::BUDGET / 2);
        }
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
                "SELECT {date_expression} date,x.dataset,x.transporter_id,json_extract(x.row,'$.da_name') da_name{projection}{scope} \
            ORDER BY {date_expression} DESC,x.publication_id,x.row_index LIMIT ? OFFSET ?"
            ),
            rusqlite::params_from_iter(&paging),
        )?;
        let mut columns = vec!["date", "driver"];
        if operational && dataset == "safety_events" {
            columns.push("assessment");
        }
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
            if operational && dataset == "safety_events" {
                values.push(json!(if row["dataset"] == "safety_events" {
                    "assessed"
                } else {
                    "pending"
                }));
            }
            values.extend(fields.iter().map(|field| row[*field].clone()));
            table.push(values);
        }
        page(&mut answer, "list", table, start, recorded, take)?;
    }
    Ok(answer)
}
