//! Read-only comparison across provider snapshots. Drivers join employees through
//! Driver Match; unique names join the drivers it has not reached yet.
use crate::{
    collectors::cortex,
    contracts::{LateRule, MealComparison, MealSource},
    driver_match::{self, DriverMatchStore},
    workforce::{self, TimecardStore},
};
use dispatch_core::{
    Result,
    db::{Store, s},
    foundation::names::{self, Name, name_key},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

// Exact matches take priority over the more conservative name-variant pass.
fn match_drivers(drivers: &mut BTreeMap<String, Value>, roster: &[Value], settings: &Value) {
    let saved = settings["links"].as_array().cloned().unwrap_or_default();
    let separate = settings["separate"].as_array().cloned().unwrap_or_default();
    let mut reserved: HashSet<String> = saved
        .iter()
        .map(|l| s(l, "paycomCode").to_owned())
        .collect();
    let mut employees_by_name: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    let mut driver_counts: BTreeMap<String, usize> = BTreeMap::new();
    for employee in roster {
        employees_by_name
            .entry(name_key(s(employee, "name")))
            .or_default()
            .push(employee);
    }
    for driver in drivers.values() {
        *driver_counts
            .entry(name_key(s(driver, "name")))
            .or_default() += 1;
    }
    for (id, driver) in drivers.iter_mut() {
        let key = name_key(s(driver, "name"));
        let candidates = employees_by_name.get(&key);
        let (code, kind) = if let Some(link) = saved.iter().find(|l| s(l, "cortexId") == id) {
            (link["paycomCode"].clone(), "saved")
        } else if separate.iter().any(|v| v.as_str() == Some(id)) {
            (Value::Null, "separate")
        } else if !key.is_empty()
            && driver_counts[&key] == 1
            && let Some(matches) = candidates
            && matches.len() == 1
            && !reserved.contains(s(matches[0], "code"))
        {
            (matches[0]["code"].clone(), "name")
        } else {
            (Value::Null, "unmatched")
        };
        if let Some(code) = code.as_str() {
            reserved.insert(code.to_owned());
        }
        driver["paycomCode"] = code;
        driver["matchType"] = json!(kind);
    }

    let mut employees_by_given: BTreeMap<String, Vec<(&Value, Name)>> = BTreeMap::new();
    for employee in roster {
        let name = Name::new(s(employee, "name"));
        employees_by_given
            .entry(name.given.clone())
            .or_default()
            .push((employee, name));
    }
    let mut candidates = BTreeMap::new();
    let mut claims: BTreeMap<String, usize> = BTreeMap::new();
    // Count every source identity, even one without punches/meals or with a saved
    // override. Removing such a person must not make an ambiguous name look unique.
    for (id, driver) in drivers.iter() {
        let name = Name::new(s(driver, "name"));
        let matches: Vec<_> = employees_by_given
            .get(&name.given)
            .into_iter()
            .flatten()
            .filter(|(_, employee_name)| name.matches(employee_name))
            .map(|(employee, _)| s(employee, "code").to_owned())
            .collect();
        for code in &matches {
            *claims.entry(code.clone()).or_default() += 1;
        }
        candidates.insert(id.clone(), matches);
    }
    for (id, driver) in drivers {
        let matches = &candidates[id];
        if s(driver, "matchType") == "unmatched"
            && matches.len() == 1
            && claims[&matches[0]] == 1
            && !reserved.contains(&matches[0])
        {
            driver["paycomCode"] = json!(matches[0]);
            driver["matchType"] = json!("name");
        }
    }
}

pub(crate) fn meal_comparison(
    store: &Store,
    id: &str,
    date: &str,
    timezone: &str,
) -> Result<MealComparison> {
    Ok(store
        .meal_comparisons(id, date, date, timezone, None)?
        .remove(date)
        .unwrap())
}
/// Batch source reads over the period. Matching keeps the full roster and
/// every source identity; selection limits cards and final assessments.
pub(crate) fn meal_comparisons(
    store: &Store,
    id: &str,
    from: &str,
    to: &str,
    timezone: &str,
    selected: Option<&[String]>,
) -> Result<BTreeMap<String, MealComparison>> {
    let codes = selected.map(|sources| {
        sources
            .iter()
            .filter_map(|source| source.strip_prefix("paycom:").map(str::to_owned))
            .collect::<Vec<_>>()
    });
    let days = crate::workforce::daily_sources(store, id, from, to, codes.as_deref())?;
    let cortex = store.collector(id, cortex::PROVIDER)?;
    let mut publications: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut ids = vec![];
    for mut publication in cortex.all(
        "SELECT id,report_date date,station,service_area_id serviceAreaId,provider,timezone,\
         collected_at collectedAt FROM meal_publications WHERE report_date BETWEEN ? AND ? \
         AND active=1 ORDER BY report_date,collected_at DESC,id DESC",
        [from, to],
    )? {
        let date = s(&publication, "date").to_owned();
        ids.push(s(&publication, "id").to_owned());
        publication.as_object_mut().unwrap().remove("date");
        publications.entry(date).or_default().push(publication);
    }
    let ids = serde_json::to_string(&ids)?;
    let mut meals: HashMap<(String, String), Vec<Value>> = HashMap::new();
    for mut meal in cortex.all(
        "SELECT m.publication_id,m.itinerary_id,m.meal_id mealId,m.last_delivery_at lastDelivery,\
         m.started_at start,m.ended_at end,m.first_delivery_at firstDelivery,\
         m.before_status beforeStatus,m.after_status afterStatus,t.last_delivery_stop lastDeliveryStop,\
         t.first_delivery_stop firstDeliveryStop FROM meal_records m \
         LEFT JOIN meal_stops t USING(publication_id,itinerary_id,meal_id) \
         WHERE m.publication_id IN (SELECT value FROM json_each(?)) \
         ORDER BY m.publication_id,m.itinerary_id,m.started_at,m.meal_id",
        [&ids],
    )? {
        let key = (s(&meal, "publication_id").to_owned(), s(&meal, "itinerary_id").to_owned());
        for field in ["publication_id", "itinerary_id"] {
            meal.as_object_mut().unwrap().remove(field);
        }
        meals.entry(key).or_default().push(meal);
    }
    let mut itineraries: HashMap<String, Vec<Value>> = HashMap::new();
    for mut itinerary in cortex.all(
        "SELECT i.publication_id,i.itinerary_id,i.transporter_id,i.driver_name,u.url sourceUrl \
         FROM meal_itineraries i LEFT JOIN meal_sources u \
         ON u.publication_id=i.publication_id AND u.itinerary_id=i.itinerary_id \
         WHERE i.publication_id IN (SELECT value FROM json_each(?)) \
         ORDER BY i.publication_id,i.itinerary_id",
        [&ids],
    )? {
        let publication = s(&itinerary, "publication_id").to_owned();
        itinerary.as_object_mut().unwrap().remove("publication_id");
        itineraries.entry(publication).or_default().push(itinerary);
    }
    let preferences = store.timecard_preference_values(id)?;
    let late = LateRule {
        time: s(&preferences["values"], "late_da_time").into(),
        departments: serde_json::from_value(preferences["values"]["late_da_departments"].clone())?,
    };
    let mut context = ComparisonContext {
        timezone, selected, meals, itineraries, late,
        links: store.dsp(id)?.setting(driver_match::LINKS, json!({"revision":0,"links":[]}))?,
        known: store.driver_links(id)?,
        latest_zone: cortex.one(
            "SELECT timezone FROM meal_publications WHERE active=1                  ORDER BY collected_at DESC,id DESC LIMIT 1", [],
        )?,
    };
    let mut live = store.live_results_range(id, cortex::PROVIDER, from, to)?;
    days.into_iter()
        .map(|(date, source)| {
            let publications = publications.remove(&date).unwrap_or_default();
            let live = live.remove(&date).unwrap_or_default();
            let comparison = context.day(&date, source, &publications, &live)?;
            Ok((date, comparison))
        })
        .collect()
}
struct ComparisonContext<'a> {
    timezone: &'a str,
    selected: Option<&'a [String]>,
    links: Value,
    known: BTreeMap<String, (Vec<String>, bool)>,
    late: LateRule,
    latest_zone: Option<Value>,
    meals: HashMap<(String, String), Vec<Value>>,
    itineraries: HashMap<String, Vec<Value>>,
}
impl ComparisonContext<'_> {
    fn day(
        &mut self,
        date: &str,
        source: workforce::DailySource,
        publications: &[Value],
        live: &dispatch_core::collection::live::LiveResults,
    ) -> Result<MealComparison> {
        let publication = source.publication;
        let roster: Vec<Value> = source.roster.values().cloned().collect();
        let cards = source.rows.into_values();
        let mut rows = BTreeMap::new();
        for row in cards {
            if !row["punches"].as_array().is_some_and(|p| {
                p.iter()
                    .any(|p| ["in", "out"].iter().any(|k| !s(p, k).trim().is_empty()))
            }) {
                continue;
            }
            let key = format!("paycom:{}", s(&row, "employeeCode"));
            rows.insert(
                key.clone(),
                json!({"id":key,"name":row["name"],"paycom":row,"cortex":[]}),
            );
        }
        // Broader and narrower provider scopes may observe the same itinerary.
        // The newest observation wins, including a newer snapshot with no meal.
        let mut itineraries = HashSet::new();
        let mut drivers = BTreeMap::new();
        let mut observations = vec![];
        for (metadata, captures) in live {
            for driver in metadata["drivers"].as_array().into_iter().flatten() {
                drivers.insert(s(driver, "id").to_owned(), driver.clone());
            }
            for capture in captures {
                let capture: crate::collectors::cortex::meals::Capture =
                    serde_json::from_value(capture.clone())?;
                let p = json!({"station":capture.scope.station,"serviceAreaId":capture.scope.service_area_id,
                    "timezone":capture.scope.timezone,"collectedAt":dispatch_core::db::at(capture.finished_at)});
                for route in capture.itineraries {
                    if !itineraries.insert((s(&p, "serviceAreaId").to_owned(), route.id.clone())) {
                        continue;
                    }
                    let itinerary = json!({"itinerary_id":route.id,"transporter_id":route.transporter_id,
                        "driver_name":route.driver,"sourceUrl":route.source_url});
                    let meals = route
                        .meals
                        .iter()
                        .map(|meal| crate::meals::comparison_meal(&route, meal))
                        .collect();
                    drivers.insert(
                        route.transporter_id.clone(),
                        json!({"id":route.transporter_id,"name":route.driver}),
                    );
                    observations.push((p.clone(), itinerary, meals));
                }
            }
        }
        for p in publications {
            for itinerary in self.itineraries.remove(s(p, "id")).unwrap_or_default() {
                if !itineraries.insert((
                    s(p, "serviceAreaId").to_owned(),
                    s(&itinerary, "itinerary_id").to_owned(),
                )) {
                    continue;
                }
                let meals = self
                    .meals
                    .remove(&(
                        s(p, "id").to_owned(),
                        s(&itinerary, "itinerary_id").to_owned(),
                    ))
                    .unwrap_or_default();
                let transporter = s(&itinerary, "transporter_id");
                // Include meal-free drivers in uniqueness checks so a name shared by
                // two drivers cannot match just because one did not take a meal.
                drivers
                    .entry(transporter.to_owned())
                    .or_insert(json!({"id":transporter,"name":itinerary["driver_name"]}));
                observations.push((p.clone(), itinerary, meals));
            }
        }
        // Names join the drivers Driver Match has not given a person yet, such as those of
        // a collection still running; Driver Match decides for every driver it knows.
        match_drivers(&mut drivers, &roster, &self.links);
        let known = &self.known;
        let listed: HashSet<&str> = roster.iter().map(|e| s(e, "code")).collect();
        let separate: HashSet<&str> = self.links["separate"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let mut claimed = HashSet::new();
        for (transporter, driver) in drivers.iter_mut() {
            let Some((codes, confirmed)) = known.get(transporter) else {
                continue;
            };
            let code = codes.iter().find(|c| listed.contains(c.as_str()));
            driver["paycomCode"] = json!(code);
            driver["matchType"] = json!(match code {
                Some(_) if *confirmed => "saved",
                Some(_) => "name",
                None if separate.contains(transporter.as_str()) => "separate",
                None => "unmatched",
            });
            claimed.extend(code.cloned());
        }
        for (transporter, driver) in drivers.iter_mut() {
            if !known.contains_key(transporter)
                && driver["paycomCode"]
                    .as_str()
                    .is_some_and(|code| claimed.contains(code))
            {
                driver["paycomCode"] = Value::Null;
                driver["matchType"] = json!("unmatched");
            }
        }
        let roster_by_code: HashMap<_, _> = roster
            .iter()
            .map(|employee| (s(employee, "code"), employee))
            .collect();
        let mut meal_drivers = HashSet::new();
        for (p, itinerary, meals) in observations {
            if meals.is_empty() {
                continue;
            }
            let transporter = s(&itinerary, "transporter_id");
            meal_drivers.insert(transporter.to_owned());
            let code = drivers[transporter]["paycomCode"].as_str();
            let key = code
                .map(|c| format!("paycom:{c}"))
                .unwrap_or_else(|| format!("cortex:{transporter}"));
            let name = code
                .and_then(|c| roster_by_code.get(c))
                .map(|e| e["name"].clone())
                .unwrap_or(itinerary["driver_name"].clone());
            let row = rows
                .entry(key.clone())
                .or_insert(json!({"id":key,"name":name,"paycom":null,"cortex":[]}));
            for mut meal in meals {
                meal["cortexId"] = json!(transporter);
                meal["driverName"] = itinerary["driver_name"].clone();
                meal["itineraryId"] = itinerary["itinerary_id"].clone();
                meal["sourceUrl"] = itinerary["sourceUrl"].clone();
                // Each delivery opens the route at the stop that held it, when both are known.
                for (stop, link) in [
                    ("lastDeliveryStop", "lastDeliveryUrl"),
                    ("firstDeliveryStop", "firstDeliveryUrl"),
                ] {
                    let place = meal
                        .as_object_mut()
                        .unwrap()
                        .remove(stop)
                        .and_then(|v| v.as_u64())
                        .and_then(|v| u32::try_from(v).ok());
                    let url = itinerary["sourceUrl"]
                        .as_str()
                        .zip(place)
                        .and_then(|(route, place)| crate::meals::stop_url(route, place));
                    meal[link] = json!(url);
                }
                meal["station"] = p["station"].clone();
                meal["timezone"] = p["timezone"].clone();
                meal["collectedAt"] = p["collectedAt"].clone();
                row["cortex"].as_array_mut().unwrap().push(meal);
            }
        }
        drivers.retain(|id, _| meal_drivers.contains(id));
        let mut rows: Vec<Value> = rows.into_values().collect();
        for row in &mut rows {
            row["cortex"].as_array_mut().unwrap().sort_by(|a, b| {
                s(a, "start")
                    .cmp(s(b, "start"))
                    .then_with(|| s(a, "mealId").cmp(s(b, "mealId")))
            });
        }
        rows.sort_by(|a, b| {
            names::compare(s(a, "name"), s(b, "name")).then_with(|| s(a, "id").cmp(s(b, "id")))
        });
        let live_zone = live.first().map(|(meta, _)| &meta["scope"]);
        let zone = live_zone
            .or(publications.first().or(self.latest_zone.as_ref()))
            .map(|p| s(p, "timezone"))
            .unwrap_or(self.timezone);
        let rows = rows
            .into_iter()
            .filter(|row| {
                self.selected
                    .is_none_or(|selected| selected.iter().any(|id| id == s(row, "id")))
            })
            .map(|row| {
                Ok(serde_json::from_value::<MealSource>(row)?.assessed(date, Some(&self.late)))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(MealComparison {
            date: date.into(),
            timezone: zone.into(),
            rows,
            paycom_collected_at: publication.map(|p| s(&p, "collected_at").into()),
            cortex_publications: serde_json::from_value(json!(publications))?,
            drivers: serde_json::from_value(json!(drivers.into_values().collect::<Vec<_>>()))?,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/backend/meals/comparison.rs"]
mod tests;
