//! What Timecard holds of the synthetic DSP: Paycom's roster and timecards, and the meal
//! breaks Cortex records beside the routes.
use crate::{
    Error, Result,
    agents::synthetic::{Made, Step, Synthetic, World, hhmm, plan},
    collectors::{
        cortex::{discovery::Scope, meals},
        paycom,
    },
    db::{Store, s},
    workforce::TimecardStore,
};
use chrono::Datelike;
use serde_json::{Value, json};

pub const SYNTHETIC: Synthetic = Synthetic {
    people: Some(people),
    steps: &[
        Step {
            order: 10,
            run: timecards,
        },
        Step {
            order: 30,
            run: meal_breaks,
        },
    ],
};

/// Paycom's demo roster: Avery Morgan dispatches; everyone else drives.
fn people(timezone: &str) -> Result<Vec<Value>> {
    let roster = paycom::fixtures::fixture(timezone)?;
    Ok(roster["employees"].as_array().cloned().unwrap_or_default())
}

/// Paycom's timecards: the dispatcher's weekdays, and each driver's days, their lunch
/// punched or not.
fn timecards(db: &Store, world: &mut World) -> Result<Made> {
    let dispatcher = world.employees.first().map(|e| s(e, "code").to_owned());
    let mut timecards = vec![];
    for (d, date) in world.dates.iter().enumerate() {
        if let Some(code) = &dispatcher
            && date.weekday().number_from_monday() <= 5
        {
            timecards.push(
                json!({"employeeCode": code, "date": date.to_string(), "hours": 8.0,
                "status": "Complete", "punches": [{"in":"08:00","out":"12:00","hours":4},
                {"in":"12:30","out":"16:30","hours":4}]}),
            );
        }
        for (i, code, _) in &world.drivers {
            let Some(day) = plan(*i, d as i64) else {
                continue;
            };
            let lunch = if day.lunch_punched {
                day.meal.unwrap_or(day.departed + day.length / 2)
            } else {
                -1
            };
            let worked = day.clock_out() - day.clock_in() - if lunch >= 0 { 30 } else { 0 };
            let punches = if lunch >= 0 {
                json!([
                    {"in": hhmm(day.clock_in()), "out": hhmm(lunch),
                     "hours": (lunch - day.clock_in()) as f64 / 60.0},
                    {"in": hhmm(lunch + 30), "out": hhmm(day.clock_out()),
                     "hours": (day.clock_out() - lunch - 30) as f64 / 60.0}
                ])
            } else {
                json!([{"in": hhmm(day.clock_in()), "out": hhmm(day.clock_out()),
                        "hours": worked as f64 / 60.0}])
            };
            timecards.push(json!({"employeeCode": code, "date": date.to_string(),
                "hours": (worked as f64 / 60.0 * 100.0).round() / 100.0,
                "status": "Complete", "punches": punches}));
        }
    }
    let dates = &world.dates;
    db.publish_timecards(
        &world.dsp,
        &json!({"employees": world.employees, "timecards": timecards, "collectedAt": crate::db::iso(),
            "from": dates[0].to_string(), "to": dates[dates.len() - 1].to_string()}),
    )?;
    Ok(Some(("timecards", json!(timecards.len()))))
}

/// Cortex's meal breaks for each day's itineraries, under the scope the day's routes were
/// collected in.
fn meal_breaks(db: &Store, world: &mut World) -> Result<Made> {
    for (d, date) in world.dates.iter().enumerate() {
        let day = date.to_string();
        let scope: Scope = serde_json::from_value(
            world
                .scopes
                .get(&day)
                .cloned()
                .ok_or_else(|| Error::new("synthetic_routes_required", 500))?,
        )?;
        let observed = crate::db::now();
        let mut found = vec![];
        for (i, _, name) in &world.drivers {
            let Some(plan) = plan(*i, d as i64) else {
                continue;
            };
            found.push(meals::Itinerary {
                id: format!("synthetic-{day}-{i}"),
                transporter_id: world.transporter(*i),
                driver: name.clone(),
                route: format!("CX{}", 100 + i),
                observed_at: observed,
                route_complete: true,
                delivery_coverage: meals::Coverage::Complete,
                meals: plan
                    .meal
                    .map(|start| meals::Meal {
                        id: format!("meal-{day}-{i}"),
                        start: world.at(*date, start),
                        end: Some(world.at(*date, start + 30)),
                        last_delivery: Some(world.at(*date, start - 4)),
                        first_delivery: Some(world.at(*date, start + 36)),
                        last_delivery_stop: None,
                        first_delivery_stop: None,
                    })
                    .into_iter()
                    .collect(),
                source_url: None,
            });
        }
        db.publish_meals(
            &world.dsp,
            &format!("synthetic-meals:{day}"),
            &meals::Capture {
                scope: scope.clone(),
                started_at: observed,
                finished_at: observed,
                itineraries: found,
            },
            &scope,
        )?;
    }
    Ok(None)
}
