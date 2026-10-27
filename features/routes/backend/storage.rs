//! What Routes keeps for a DSP, in the database beside Cortex's: each day's routes, itineraries
//! and packages, the collections that bring them and their expiry past the DSP's window
//! (`RoutesStore`), and how its rows read back as the API's types.
use super::*;

/// What Routes reads and writes for a DSP: each day's routes, itineraries and packages, in
/// the database beside Cortex's, the collections that bring them and their expiry.
pub trait RoutesStore {
    /// Historical route data: the days kept.
    fn routes_db(&self, id: &str) -> Result<DspLease<'_>>;
    /// Live route data: today as the last whole live read left it.
    fn routes_live_db(&self, id: &str) -> Result<DspLease<'_>>;
    fn enqueue_routes(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        day: Option<&str>,
        mode: Mode,
        days: i64,
    ) -> Result<Value>;
    fn routes_schedule_ready(&self, id: &str) -> Result<()>;
    fn routes_jobs(&self, id: &str, mode: Mode) -> Result<Vec<(String, Value)>>;
    fn stage_routes_start(
        &self,
        id: &str,
        job: &str,
        lists: &Capture,
        itineraries: usize,
    ) -> Result<Staged>;
    fn stage_routes(&self, id: &str, job: &str, capture: Capture) -> Result<Staged>;
    fn stage_routes_itinerary(
        &self,
        id: &str,
        staged: &Staged,
        itinerary: &PreparedItinerary,
    ) -> Result<()>;
    fn publish_routes(&self, id: &str, job: &str, staged: &Staged) -> Result<bool>;
    fn expire_routes(&self, id: &str) -> Result<usize>;
    fn sweep_routes(&self, id: &str) -> Result<bool>;
    fn reprocess_routes(&self, id: &str, day: Option<&str>) -> Result<RouteReprocess>;
    fn route_days(&self, id: &str) -> Result<RouteDays>;
    fn route_day(&self, id: &str, day: &str) -> Result<Option<RouteDayView>>;
    fn route_itinerary(
        &self,
        id: &str,
        day: &str,
        itinerary_id: &str,
    ) -> Result<Option<RouteItineraryDetail>>;
    fn route_package(&self, id: &str, tracking: &str) -> Result<RoutePackage>;
}
impl RoutesStore for Store {
    fn routes_db(&self, id: &str) -> Result<DspLease<'_>> {
        self.added_storage(id, cortex::PROVIDER, &STORAGE)
    }
    fn routes_live_db(&self, id: &str) -> Result<DspLease<'_>> {
        self.added_storage(id, cortex::PROVIDER, &LIVE_STORAGE)
    }
    /// Queues `days` days ending at `day`, or the default day of `mode`, answering with
    /// the jobs. A day already queued under `key` answers the same job.
    fn enqueue_routes(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        day: Option<&str>,
        mode: Mode,
        days: i64,
    ) -> Result<Value> {
        ensure(
            (1..=MAX_DAYS_PER_REQUEST).contains(&days),
            "invalid_input",
            400,
        )?;
        let today = routes_today(self, id)?;
        let last = day
            .map(str::to_owned)
            .unwrap_or_else(|| default_day(mode, today));
        day_allowed(mode, &last, today)?;
        ensure(days == 1 || mode == Mode::Final, "invalid_input", 400)?;
        // A day the retention window has passed would be deleted again within the hour.
        if let Some(start) = retention_start(self, id)? {
            let first = NaiveDate::parse_from_str(&last, "%Y-%m-%d")
                .map_err(|_| Error::new("invalid_date", 400))?
                - Duration::days(days - 1);
            ensure(first >= start, "routes_day_outside_retention", 400)?;
        }
        let last_date = NaiveDate::parse_from_str(&last, "%Y-%m-%d")
            .map_err(|_| Error::new("invalid_date", 400))?;
        let requests = (0..days)
            .map(|back| {
                let day = (last_date - Duration::days(back)).to_string();
                let suffix = if days == 1 {
                    String::new()
                } else {
                    format!(":{day}")
                };
                Ok((
                    format!("{key}{suffix}"),
                    cortex::PROVIDER,
                    routes_request(self, id, &day, mode)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let jobs = self.enqueue_batch(id, actor, &requests)?;
        Ok(serde_json::to_value(jobs)?)
    }
    fn routes_schedule_ready(&self, id: &str) -> Result<()> {
        ensure(
            !self.profile(id)?.station_code.is_empty(),
            "routes_station_required",
            409,
        )
    }
    /// The days a scheduled run collects in `mode`: a snapshot of today; or as final,
    /// yesterday and recent days without a final publication, newest first. A snapshot of
    /// a day leaves its final read to the first run after it ends.
    fn routes_jobs(&self, id: &str, mode: Mode) -> Result<Vec<(String, Value)>> {
        let today = routes_today(self, id)?;
        if mode == Mode::Snapshot {
            let day = today.to_string();
            return Ok(vec![(
                format!("routes:{day}"),
                routes_request(self, id, &day, mode)?,
            )]);
        }
        let station = self.profile(id)?.station_code;
        let db = self.routes_db(id)?;
        let mut jobs = Vec::new();
        for back in 1..=(BACKFILL_DAYS as i64) {
            let day = (today - Duration::days(back)).to_string();
            let published = db.one(
                "SELECT id FROM route_publications WHERE day=? AND station=? AND mode='final' AND active=1",
                [&day, &station],
            )?;
            if published.is_none() {
                jobs.push((
                    format!("routes:{day}"),
                    routes_request(self, id, &day, Mode::Final)?,
                ));
                if jobs.len() >= MAX_JOBS_PER_RUN {
                    break;
                }
            }
        }
        Ok(jobs)
    }
    /// Starts a day's inactive publication for `job`: the publication row, both lists'
    /// responses and the planned routes. A previous attempt's publication is left to the
    /// sweep.
    /// `lists` is the validated capture with its itineraries taken out; `itineraries`
    /// is how many it had.
    fn stage_routes_start(
        &self,
        id: &str,
        job: &str,
        lists: &Capture,
        itineraries: usize,
    ) -> Result<Staged> {
        let capture = lists;
        let db = self.routes_db(id)?;
        let publication = dispatch_core::foundation::crypto::id("routes")?;
        let routes = capture.route_summaries["rmsRouteSummaries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let day = capture.scope.date.as_str();
        let summaries = serde_json::to_vec(&capture.summaries)?;
        let route_summaries = serde_json::to_vec(&capture.route_summaries)?;
        db.transaction(|| {
            // A previous attempt's day is handed to `sweep_routes`, which deletes it in
            // small steps; its job id is freed for this attempt.
            db.exec(
                "UPDATE route_publications SET job_id=job_id||':abandoned:'||id \
                 WHERE job_id=? AND active=0",
                [job],
            )?;
            db.0.prepare_cached(
                "INSERT INTO route_publications(id,job_id,day,station,service_area_id,provider,timezone,mode,\
                 started_at,collected_at,active,route_count,itinerary_count,stop_count,task_count,adapter_version) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,0,?,?,0,0,?)",
            )?
            .execute(params![
                publication,
                job,
                day,
                capture.scope.station,
                capture.scope.service_area_id,
                capture.scope.provider,
                capture.scope.timezone,
                capture.mode.as_str(),
                at(capture.started_at),
                at(capture.finished_at),
                routes.len() as i64,
                itineraries as i64,
                ADAPTER_VERSION
            ])?;
            write_routes(&db, Writes::Staged, &publication, day, &routes)?;
            for (name, list) in [
                ("summaries", &summaries),
                ("route_summaries", &route_summaries),
            ] {
                write_raw(&db, Writes::Staged, &publication, name, list.len(), &gzip(list)?)?;
            }
            Ok(())
        })?;
        Ok(Staged {
            collection: Collection::Routes,
            publication,
            day: day.into(),
            station: capture.scope.station.clone(),
            service_area_id: capture.scope.service_area_id.clone(),
            provider: capture.scope.provider.clone(),
            itineraries,
        })
    }
    /// Stages a whole capture at once, without the platform's lock: what tests and
    /// tools use where `stage` would need a running job.
    fn stage_routes(&self, id: &str, job: &str, mut capture: Capture) -> Result<Staged> {
        let request: Value = serde_json::from_str(&self.job_row(job, Some(id))?.request)?;
        let request = Request::parse(&request)?.ok_or_else(|| Error::new("invalid_input", 400))?;
        capture.validate(&request)?;
        capture.validate_response_budget()?;
        let shaper = Shaper::new(&capture);
        let itineraries = std::mem::take(&mut capture.itineraries);
        let staged = self.stage_routes_start(id, job, &capture, itineraries.len())?;
        for captured in &itineraries {
            self.stage_routes_itinerary(id, &staged, &shaper.shape(captured)?)?;
        }
        Ok(staged)
    }
    /// Adds one shaped itinerary to a staged publication, in one short transaction.
    fn stage_routes_itinerary(
        &self,
        id: &str,
        staged: &Staged,
        itinerary: &PreparedItinerary,
    ) -> Result<()> {
        let db = self.routes_db(id)?;
        db.transaction(|| {
            ensure(
                db.one(
                    "SELECT id FROM route_publications WHERE id=? AND active=0",
                    [&staged.publication],
                )?
                .is_some(),
                "routes_capture_invalid",
                502,
            )?;
            write_itinerary(&db, Writes::Staged, &staged.publication, &staged.day, itinerary)?;
            write_raw(
                &db,
                Writes::Staged,
                &staged.publication,
                &format!("itinerary:{}", itinerary.id),
                itinerary.raw_bytes,
                &itinerary.raw,
            )?;
            db.exec(
                "UPDATE route_publications SET stop_count=stop_count+?,task_count=task_count+? WHERE id=?",
                params![
                    itinerary.stops.len() as i64,
                    itinerary.tasks.len() as i64,
                    staged.publication
                ],
            )?;
            Ok(())
        })
    }
    /// Makes a staged day the day's read if it was collected later than the one the day has
    /// (`publish_day`), keeping the day's work phones: the standard collection reads none. A day
    /// it changed is noted, to tell of (`changed`). Answers whether it is the day's read now.
    fn publish_routes(&self, id: &str, job: &str, staged: &Staged) -> Result<bool> {
        let current = publish_day(&*self.routes_db(id)?, job, staged, None)?;
        if current {
            changed::note(id, &staged.day);
        }
        Ok(current)
    }
    /// Retires the days a DSP's retention window has passed: they become inactive, and
    /// `sweep_routes` deletes them in small steps. Addresses and drivers last seen before
    /// the window go with them, as do the days' work phones. Nothing is retired while the
    /// routes feature is off, so switching it off never deletes. Answers how many days were
    /// retired.
    fn expire_routes(&self, id: &str) -> Result<usize> {
        if !self.feature_enabled(id, COLLECTION)? {
            return Ok(0);
        }
        let Some(start) = retention_start(self, id)? else {
            return Ok(0);
        };
        let start = start.to_string();
        let db = self.routes_db(id)?;
        let retired = db.transaction(|| {
            let retired = db.exec(
                "UPDATE route_publications SET active=0 WHERE active=1 AND day<?",
                [&start],
            )?;
            db.exec("DELETE FROM addresses WHERE last_seen_day<?", [&start])?;
            db.exec("DELETE FROM drivers WHERE last_seen_day<?", [&start])?;
            db.exec("DELETE FROM work_phones WHERE day<?", [&start])?;
            Ok(retired)
        })?;
        if retired > 0 {
            self.audit(None, Some(id), "routes.data_expired", &start)?;
        }
        Ok(retired)
    }
    /// One small step of removing what no reader sees: publications a newer one
    /// replaced, a retention window retired, or a job left unfinished. Answers whether
    /// more remains. A publication whose job still runs is being staged and is kept.
    fn sweep_routes(&self, id: &str) -> Result<bool> {
        sweep_routes_step(self, id, &mut None)
    }
    /// Rebuilds every active publication's rows, or one day's, from the responses it
    /// stored: what a release that reads more of them needs, without collecting again. Each day
    /// rebuilt is noted, to tell of (`changed`).
    fn reprocess_routes(&self, id: &str, day: Option<&str>) -> Result<RouteReprocess> {
        if let Some(day) = day {
            dispatch_core::foundation::validate::date(day)?;
        }
        let db = self.routes_db(id)?;
        let publications = db.all(
            "SELECT id,day,mode,station,service_area_id,provider,timezone,started_at,collected_at \
             FROM route_publications WHERE active=1 AND (?1 IS NULL OR day=?1) ORDER BY day",
            [day],
        )?;
        let mut rebuilt = Vec::new();
        for row in &publications {
            let publication = s(row, "id").to_owned();
            let capture = stored_capture(&db, row)?;
            let prepared = prepare(&capture)?;
            let routes = capture.route_summaries["rmsRouteSummaries"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            db.transaction(|| {
                for table in [
                    "routes",
                    "itineraries",
                    "stops",
                    "tasks",
                    "driver_days",
                    "breaks",
                    "unknown_stops",
                ] {
                    db.exec(
                        &format!("DELETE FROM {table} WHERE publication_id=?"),
                        [&publication],
                    )?;
                }
                let day = s(row, "day");
                write_routes(&db, Writes::Staged, &publication, day, &routes)?;
                for itinerary in &prepared {
                    write_itinerary(&db, Writes::Staged, &publication, day, itinerary)?;
                }
                share(&db, &publication, day)?;
                db.exec(
                    "UPDATE route_publications SET route_count=?,itinerary_count=?,stop_count=?,task_count=?,\
                     adapter_version=? WHERE id=?",
                    params![
                        routes.len() as i64,
                        prepared.len() as i64,
                        prepared.iter().map(|i| i.stops.len()).sum::<usize>() as i64,
                        prepared.iter().map(|i| i.tasks.len()).sum::<usize>() as i64,
                        ADAPTER_VERSION,
                        publication
                    ],
                )?;
                Ok(())
            })?;
            changed::note(id, s(row, "day"));
            let fresh = db
                .one(
                    "SELECT id,day,mode,station,collected_at,route_count,itinerary_count,stop_count,task_count \
                     FROM route_publications WHERE id=?",
                    [&publication],
                )?
                .ok_or_else(|| Error::new("not_found", 404))?;
            rebuilt.push(self::publication(&fresh));
        }
        Ok(RouteReprocess {
            publications: rebuilt,
        })
    }
    /// Every day with a publication at the DSP's station, newest first.
    fn route_days(&self, id: &str) -> Result<RouteDays> {
        let today = routes_today(self, id)?;
        let station = self.profile(id)?.station_code;
        let db = self.routes_db(id)?;
        let days = db
            .all(
                "SELECT id,day,mode,station,collected_at,route_count,itinerary_count,stop_count,task_count \
                 FROM route_publications WHERE station=? AND active=1 ORDER BY day DESC",
                [&station],
            )?
            .iter()
            .map(publication)
            .collect();
        Ok(RouteDays {
            station,
            latest_day: default_day(Mode::Final, today),
            days,
        })
    }
    /// A day's publication with its driver summaries, or none.
    fn route_day(&self, id: &str, day: &str) -> Result<Option<RouteDayView>> {
        dispatch_core::foundation::validate::date(day)?;
        let station = self.profile(id)?.station_code;
        let db = self.routes_db(id)?;
        let Some(row) = db.one(
            "SELECT id,day,mode,station,collected_at,route_count,itinerary_count,stop_count,task_count \
             FROM route_publications WHERE station=? AND day=? AND active=1",
            [&station, day],
        )?
        else {
            return Ok(None);
        };
        let itineraries = db
            .all(
                &format!("{ITINERARY_SELECT} WHERE i.publication_id=? ORDER BY i.route_code,i.driver_name"),
                [s(&row, "id")],
            )?
            .iter()
            .map(itinerary_from)
            .collect();
        Ok(Some(RouteDayView {
            publication: publication(&row),
            itineraries,
        }))
    }
    /// One driver's itinerary for a day: its stops with their tasks and locations, its
    /// breaks, its unknown stops and the tasks removed from it.
    fn route_itinerary(
        &self,
        id: &str,
        day: &str,
        itinerary_id: &str,
    ) -> Result<Option<RouteItineraryDetail>> {
        dispatch_core::foundation::validate::date(day)?;
        let station = self.profile(id)?.station_code;
        let db = self.routes_db(id)?;
        let Some(row) = db.one(
            "SELECT id,day,mode,station,collected_at,route_count,itinerary_count,stop_count,task_count \
             FROM route_publications WHERE station=? AND day=? AND active=1",
            [&station, day],
        )?
        else {
            return Ok(None);
        };
        let publication_id = s(&row, "id").to_owned();
        let Some(itinerary) = db
            .one(
                &format!("{ITINERARY_SELECT} WHERE i.publication_id=? AND i.itinerary_id=?"),
                [&publication_id, itinerary_id],
            )?
            .as_ref()
            .map(itinerary_from)
        else {
            return Ok(None);
        };
        let tasks = db.all(
            "SELECT t.*,a.address1,a.city,a.state,a.postal_code,a.latitude address_latitude,a.longitude address_longitude \
             FROM tasks t LEFT JOIN addresses a ON a.address_id=t.address_id \
             WHERE t.publication_id=? AND t.itinerary_id=? ORDER BY t.executed_at,t.task_id",
            [&publication_id, itinerary_id],
        )?;
        let stops = db
            .all(
                "SELECT s.*,a.address1,a.city,a.state,a.postal_code,a.latitude,a.longitude \
                 FROM stops s LEFT JOIN addresses a ON a.address_id=s.address_id \
                 WHERE s.publication_id=? AND s.itinerary_id=? ORDER BY s.sequence,s.stop_id",
                [&publication_id, itinerary_id],
            )?
            .iter()
            .map(|stop| RouteStop {
                stop_id: s(stop, "stop_id").into(),
                sequence: stop["sequence"].as_i64(),
                stop_type: stop["stop_type"].as_str().map(str::to_owned),
                planned_start_at: stop["planned_start_at"].as_i64(),
                planned_end_at: stop["planned_end_at"].as_i64(),
                address: address_from(stop, "latitude", "longitude"),
                tasks: tasks
                    .iter()
                    .filter(|t| t["stop_id"] == stop["stop_id"] && t["active"] != 0)
                    .map(task_from)
                    .collect(),
            })
            .collect();
        let breaks = db
            .all(
                "SELECT * FROM breaks WHERE publication_id=? AND itinerary_id=? ORDER BY planned,ordinal",
                [&publication_id, itinerary_id],
            )?
            .iter()
            .map(|b| RouteBreak {
                planned: b["planned"] == 1,
                break_id: b["break_id"].as_str().map(str::to_owned),
                kind: b["kind"].as_str().map(str::to_owned),
                state: b["state"].as_str().map(str::to_owned),
                started_at: b["started_at"].as_i64(),
                ended_at: b["ended_at"].as_i64(),
                planned_start_at: b["planned_start_at"].as_i64(),
                planned_end_at: b["planned_end_at"].as_i64(),
            })
            .collect();
        let unknown_stops = db
            .all(
                "SELECT * FROM unknown_stops WHERE publication_id=? AND itinerary_id=? ORDER BY ordinal",
                [&publication_id, itinerary_id],
            )?
            .iter()
            .map(|u| RouteUnknownStop {
                entered_at: u["entered_at"].as_i64(),
                exited_at: u["exited_at"].as_i64(),
                enter_latitude: u["enter_latitude"].as_f64(),
                enter_longitude: u["enter_longitude"].as_f64(),
                exit_latitude: u["exit_latitude"].as_f64(),
                exit_longitude: u["exit_longitude"].as_f64(),
            })
            .collect();
        Ok(Some(RouteItineraryDetail {
            publication: publication(&row),
            itinerary,
            stops,
            breaks,
            unknown_stops,
            inactive_tasks: tasks
                .iter()
                .filter(|t| t["active"] == 0)
                .map(task_from)
                .collect(),
        }))
    }
    /// Everything that happened to a package across the days stored, newest first.
    fn route_package(&self, id: &str, tracking: &str) -> Result<RoutePackage> {
        let db = self.routes_db(id)?;
        let events = db
            .all(
                "SELECT t.*,i.driver_name,i.route_code,a.address1,a.city,a.state,a.postal_code,\
                 a.latitude address_latitude,a.longitude address_longitude \
                 FROM tasks t JOIN route_publications p ON p.id=t.publication_id AND p.active=1 \
                 LEFT JOIN itineraries i ON i.publication_id=t.publication_id AND i.itinerary_id=t.itinerary_id \
                 LEFT JOIN addresses a ON a.address_id=t.address_id \
                 WHERE t.tracking_id=? ORDER BY t.day DESC,t.executed_at,t.task_id LIMIT 500",
                [tracking],
            )?
            .iter()
            .map(|t| RoutePackageEvent {
                day: s(t, "day").into(),
                transporter_id: s(t, "transporter_id").into(),
                driver_name: t["driver_name"].as_str().map(str::to_owned),
                route_code: t["route_code"].as_str().map(str::to_owned),
                itinerary_id: s(t, "itinerary_id").into(),
                task: task_from(t),
                address: address_from(t, "address_latitude", "address_longitude"),
            })
            .collect();
        Ok(RoutePackage {
            tracking_id: tracking.into(),
            events,
        })
    }
}

/// The itinerary and day-summary columns a `RouteItinerary` is read from.
pub(super) const ITINERARY_SELECT: &str = "SELECT i.itinerary_id,i.transporter_id,i.driver_name,\
    i.route_code,i.execution_status,i.progress_status,\
    d.departed_at,d.first_stop_at,d.last_stop_at,d.session_end_at,d.stops_total,d.stops_completed,d.tasks_total,\
    d.delivered,d.picked_up,d.not_delivered,d.breaks_secs,d.overtime_secs \
    FROM itineraries i JOIN driver_days d ON d.publication_id=i.publication_id AND d.itinerary_id=i.itinerary_id";
pub(super) fn itinerary_from(r: &Value) -> RouteItinerary {
    RouteItinerary {
        itinerary_id: s(r, "itinerary_id").into(),
        transporter_id: s(r, "transporter_id").into(),
        driver_name: s(r, "driver_name").into(),
        route_code: r["route_code"].as_str().map(str::to_owned),
        execution_status: r["execution_status"].as_str().map(str::to_owned),
        progress_status: r["progress_status"].as_str().map(str::to_owned),
        departed_at: r["departed_at"].as_i64(),
        first_stop_at: r["first_stop_at"].as_i64(),
        last_stop_at: r["last_stop_at"].as_i64(),
        session_end_at: r["session_end_at"].as_i64(),
        stops_total: r["stops_total"].as_i64().unwrap_or(0),
        stops_completed: r["stops_completed"].as_i64().unwrap_or(0),
        tasks_total: r["tasks_total"].as_i64().unwrap_or(0),
        delivered: r["delivered"].as_i64().unwrap_or(0),
        picked_up: r["picked_up"].as_i64().unwrap_or(0),
        not_delivered: r["not_delivered"].as_i64().unwrap_or(0),
        breaks_secs: r["breaks_secs"].as_i64(),
        overtime_secs: r["overtime_secs"].as_i64(),
    }
}
pub(super) fn task_from(t: &Value) -> RouteTask {
    RouteTask {
        task_id: s(t, "task_id").into(),
        tracking_id: t["tracking_id"].as_str().map(str::to_owned),
        order_id: t["order_id"].as_str().map(str::to_owned),
        task_type: t["task_type"].as_str().map(str::to_owned),
        task_state: t["task_state"].as_str().map(str::to_owned),
        state_context: t["state_context"].as_str().map(str::to_owned),
        execution_status: t["execution_status"].as_str().map(str::to_owned),
        executed_at: t["executed_at"].as_i64(),
        window_start_at: t["window_start_at"].as_i64(),
        window_end_at: t["window_end_at"].as_i64(),
        latitude: t["latitude"].as_f64(),
        longitude: t["longitude"].as_f64(),
        address_id: t["address_id"].as_str().map(str::to_owned),
        address_type: t["address_type"].as_str().map(str::to_owned),
        active: t["active"] != 0,
    }
}
/// The address joined onto a row, when the row has one.
pub(super) fn address_from(row: &Value, latitude: &str, longitude: &str) -> Option<RouteAddress> {
    let address_id = row["address_id"].as_str()?;
    Some(RouteAddress {
        address_id: address_id.into(),
        address1: row["address1"].as_str().map(str::to_owned),
        city: row["city"].as_str().map(str::to_owned),
        state: row["state"].as_str().map(str::to_owned),
        postal_code: row["postal_code"].as_str().map(str::to_owned),
        latitude: row[latitude].as_f64(),
        longitude: row[longitude].as_f64(),
    })
}

pub(super) fn publication(row: &Value) -> RoutePublication {
    RoutePublication {
        id: s(row, "id").into(),
        day: s(row, "day").into(),
        mode: s(row, "mode").into(),
        station: s(row, "station").into(),
        collected_at: s(row, "collected_at").into(),
        route_count: row["route_count"].as_i64().unwrap_or(0),
        itinerary_count: row["itinerary_count"].as_i64().unwrap_or(0),
        stop_count: row["stop_count"].as_i64().unwrap_or(0),
        task_count: row["task_count"].as_i64().unwrap_or(0),
    }
}
pub(super) fn sql_value(value: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as Sql;
    match value {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(i64::from(*b)),
        Value::Number(n) => n
            .as_i64()
            .map(Sql::Integer)
            .unwrap_or_else(|| Sql::Real(n.as_f64().unwrap_or(0.0))),
        Value::String(text) => Sql::Text(text.clone()),
        other => Sql::Text(other.to_string()),
    }
}
pub(super) fn opt<T: Into<Value>>(value: Option<T>) -> Value {
    value.map(Into::into).unwrap_or(Value::Null)
}
/// A list as JSON text, or nothing when absent or empty.
pub(super) fn json_list(value: &Value) -> Value {
    match value.as_array() {
        Some(items) if !items.is_empty() => json!(Value::Array(items.clone()).to_string()),
        _ => Value::Null,
    }
}
