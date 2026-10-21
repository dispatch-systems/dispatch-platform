//! A DSP's collection schedules: each collection's rules (`rules`), which its members set on
//! Settings' Collectors page, and the scheduler's turn at them. A rule is one row; a collection
//! is on or off as a whole, and its rules are saved together.
use crate::{
    Error, Result,
    accounts::Context,
    collection::{
        api::{
            jobs::{JobKind, JobStatus, PublicJob},
            types::{
                Collectors, LastRun, RuleKind, ScheduleChoice, ScheduleCollection, ScheduleOption,
                ScheduleRule, ScheduleRuns, ScheduledCollection,
            },
        },
        registry::Provider,
        rules::{self, Rule, Timing},
    },
    db::{AuditChange, FromRow, Row, Store, at, iso, now},
    ensure,
    foundation::{crypto, validate as v},
    job_statuses,
    manifest::{Collection, Collector, Keeper, registry},
};
use chrono::{NaiveTime, TimeZone};
use chrono_tz::Tz;
use rusqlite::params;
use serde_json::Value;

const SAVE: &str = "INSERT INTO collection_schedules(id,name,collection,cadence,interval_minutes,\
    local_time,anchor,enabled,next_run,created_at,days,times,until_time) \
    VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,\
    collection=excluded.collection,cadence=excluded.cadence,\
    interval_minutes=excluded.interval_minutes,local_time=excluded.local_time,\
    anchor=excluded.anchor,enabled=excluded.enabled,next_run=excluded.next_run,last_error=NULL,\
    days=excluded.days,times=excluded.times,until_time=excluded.until_time,\
    revision=collection_schedules.revision+1";
const SWITCH: &str = "UPDATE collection_schedules SET enabled=?,next_run=?,last_error=NULL,\
    revision=revision+1 WHERE id=?";
const PAUSE: &str = "UPDATE collection_schedules SET enabled=0,next_run=NULL,last_error=NULL,\
    revision=revision+1 WHERE enabled=1 AND collection=?";
const RETIME: &str = "UPDATE collection_schedules SET anchor=?,next_run=?,last_error=NULL,\
    revision=revision+1 WHERE id=?";
// Each query that reads, retimes or runs a schedule takes `ScheduleCollection::known()`,
// and each that asks for a DSP's jobs `JobKind::known()`, so that none reaches what this
// release doesn't run.
const NEXT_DEADLINE: &str = "SELECT collection,next_run FROM collection_schedules \
    WHERE enabled=1 AND collection IN (SELECT value FROM json_each(?)) ORDER BY next_run";
const ACTIVE_JOB: &str = concat!(
    "SELECT id FROM jobs WHERE dsp_id=? AND kind IN (SELECT value FROM json_each(?)) \
     AND status IN ",
    job_statuses!(active),
    " LIMIT 1"
);
const LAST_RUN: &str = "SELECT status,completed_at,error FROM jobs WHERE dsp_id=? AND kind=? \
    AND status IN ('succeeded','failed') ORDER BY created_at DESC LIMIT 1";
/// How many runs a preview lists.
const UPCOMING: usize = 5;
/// The most rules a DSP's collections have between them.
const MOST_RULES: i64 = 50;

/// A row of `collection_schedules`: one rule of a collection's schedule.
#[derive(Clone)]
pub struct ScheduleRow {
    pub id: String,
    pub name: String,
    /// What it collects by: its collection's own schedule name, or an alias that reads it.
    pub collection: ScheduleCollection,
    pub rule: Rule,
    pub enabled: bool,
    pub next_run: Option<String>,
    pub revision: i64,
    pub last_error: Option<String>,
    /// The first run of its single time or repeat, which only the previous release reads.
    anchor: i64,
}
impl FromRow for ScheduleRow {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let first = rules::time(&row.get::<String>("local_time")?)?;
        let days: i64 = row.get("days")?;
        // The previous release reads a rule as its first time, or a repeat all day from its
        // start, and saves only those: a rule whose first time it changed is what it ran.
        let (days, timing) = match row.get::<String>("cadence")?.as_str() {
            "daily" => {
                let times: Option<String> = row.get("times")?;
                let times = match times {
                    Some(times) => serde_json::from_str::<Vec<String>>(&times)?
                        .iter()
                        .map(|time| rules::time(time))
                        .collect::<Result<Vec<_>>>()?,
                    None => vec![],
                };
                match times.first() {
                    Some(at) if *at == first => (days, Timing::At(times)),
                    _ => (rules::EVERY_DAY, Timing::At(vec![first])),
                }
            }
            _ => {
                let to: Option<String> = row.get("until_time")?;
                let to = to.as_deref().map(rules::time).transpose()?;
                let timing = Timing::Repeat {
                    every: row.get("interval_minutes")?,
                    from: first,
                    to: to.unwrap_or(first),
                };
                (days, timing)
            }
        };
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            collection: row.get("collection")?,
            rule: Rule { days, timing },
            enabled: row.get("enabled")?,
            next_run: row.get("next_run")?,
            revision: row.get("revision")?,
            last_error: row.get("last_error")?,
            anchor: row.get("anchor")?,
        })
    }
}

fn timezone(tz: &str) -> Result<Tz> {
    tz.parse().map_err(|_| Error::new("invalid_timezone", 400))
}
/// When a rule first runs, as the previous release times a schedule: its first time today,
/// or the next day that has it.
pub fn anchor(time: NaiveTime, tz: Tz, after: i64) -> Result<i64> {
    let date = chrono::DateTime::from_timestamp_millis(after)
        .ok_or_else(|| Error::new("invalid_schedule", 400))?
        .with_timezone(&tz)
        .date_naive();
    (0..4)
        .find_map(|days| {
            tz.from_local_datetime(&(date + chrono::Duration::days(days)).and_time(time))
                .earliest()
        })
        .map(|first| first.timestamp_millis())
        .ok_or_else(|| Error::new("schedule_unresolvable", 400))
}
fn next_run(rule: &Rule, tz: Tz, after: i64) -> Result<String> {
    rule.next(tz, after)
        .map(at)
        .ok_or_else(|| Error::new("schedule_unresolvable", 400))
}
/// The single time and interval the previous release runs a rule by: its first time, or the
/// start of its repeat.
fn legacy(rule: &Rule) -> (&'static str, Option<i64>, NaiveTime) {
    match &rule.timing {
        Timing::At(times) => ("daily", None, times[0]),
        Timing::Repeat { every, from, .. } => ("interval", Some(*every), *from),
    }
}

/// A collection a schedule may run on the Collectors page: one a collector reads, its
/// collector, and its feature's keeper of it.
#[derive(Clone, Copy)]
pub struct Scheduled {
    pub collector: &'static dyn Collector,
    pub collection: &'static Collection,
    pub keeper: &'static dyn Keeper,
}
impl Scheduled {
    /// Every collection, in the registry's order.
    pub fn all() -> impl Iterator<Item = Self> {
        registry().collectors.iter().flat_map(|collector| {
            collector.collections().iter().map(move |collection| Self {
                collector: *collector,
                collection,
                keeper: registry().keeper(collection.job_kind),
            })
        })
    }
    /// The collection whose own schedule name is `name`.
    pub fn named(name: &str) -> Result<Self> {
        Self::all()
            .find(|scheduled| scheduled.collection.schedule == name)
            .ok_or_else(|| Error::new("not_found", 404))
    }
    /// What each of its rules may collect by, with its label: its own name, then its feature's
    /// aliases that read it another way.
    fn options(self) -> Vec<(ScheduleCollection, &'static str)> {
        let declared: &[(&str, &str)] = match self.keeper.schedules().choice {
            Some((_, options)) => options,
            None => &[],
        };
        let own = [(self.collection.schedule, self.collection.label)];
        let options = if declared.is_empty() {
            &own[..]
        } else {
            declared
        };
        options
            .iter()
            .filter_map(|(name, label)| ScheduleCollection::parse(name).map(|c| (c, *label)))
            .collect()
    }
    fn names(self) -> Result<String> {
        let names: Vec<&str> = self.options().iter().map(|(c, _)| c.as_str()).collect();
        Ok(serde_json::to_string(&names)?)
    }
    fn offers(self, collection: &str) -> Result<ScheduleCollection> {
        self.options()
            .into_iter()
            .map(|(offered, _)| offered)
            .find(|offered| offered.as_str() == collection)
            .ok_or_else(|| Error::new("invalid_input", 400))
    }
    fn provider(self) -> Provider {
        Provider::new(self.collector.id())
    }
}

/// The rules of a request: what each collects by, and when.
fn requested(
    scheduled: Scheduled,
    value: &Value,
) -> Result<Vec<(Option<String>, ScheduleCollection, Rule)>> {
    let rules = value["rules"]
        .as_array()
        .filter(|rules| rules.len() as i64 <= MOST_RULES)
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    rules
        .iter()
        .map(|rule| {
            v::fields(
                rule,
                &[
                    "id", "collects", "kind", "days", "times", "every", "from", "to",
                ],
            )?;
            let id = match &rule["id"] {
                Value::Null => None,
                _ => Some(v::text(rule, "id", 1, 128)?.to_owned()),
            };
            let collects = scheduled.offers(v::text(rule, "collects", 1, 64)?)?;
            Ok((id, collects, Rule::parse(rule)?))
        })
        .collect()
}
/// A collection's rules, each as words, for the activity log.
fn described(rows: &[(ScheduleCollection, Rule)], scheduled: Scheduled) -> Option<String> {
    let labels = scheduled.options();
    let chooses = labels.len() > 1;
    let words: Vec<String> = rows
        .iter()
        .map(|(collection, rule)| {
            let label = labels
                .iter()
                .find(|(c, _)| c == collection)
                .map(|(_, l)| *l);
            match label.filter(|_| chooses) {
                Some(label) => format!("{} ({label})", rule.describe()),
                None => rule.describe(),
            }
        })
        .collect();
    (!words.is_empty()).then(|| words.join("; "))
}
/// What a collection's schedule was at: each of its rules and the revision it was saved at.
fn revision(rows: &[ScheduleRow]) -> String {
    let mut kept: Vec<String> = rows
        .iter()
        .map(|row| format!("{}.{}", row.id, row.revision))
        .collect();
    kept.sort();
    kept.join(",")
}
fn rule_of(row: &ScheduleRow) -> ScheduleRule {
    let days = (0..7)
        .filter(|day| row.rule.days & (1 << day) != 0)
        .collect();
    let (kind, times, every, from, to) = match &row.rule.timing {
        Timing::At(times) => (
            RuleKind::Times,
            times.iter().map(|at| rules::text(*at)).collect(),
            None,
            None,
            None,
        ),
        Timing::Repeat { every, from, to } => (
            RuleKind::Repeat,
            vec![],
            Some(*every),
            Some(rules::text(*from)),
            Some(rules::text(*to)),
        ),
    };
    ScheduleRule {
        id: Some(row.id.clone()),
        collects: row.collection,
        kind,
        days,
        times,
        every,
        from,
        to,
        next_run: row.next_run.clone().filter(|_| row.enabled),
        last_error: row.last_error.clone(),
    }
}

impl Store {
    pub(crate) fn initialize_schedules(&self, id: &str) -> Result<()> {
        let db = self.dsp(id)?;
        // v0.0.9 imports the DSP's old single schedule unless this is set.
        if db.setting("collectionSchedules.initialized", serde_json::json!(false))?
            != serde_json::json!(true)
        {
            db.set("collectionSchedules.initialized", &serde_json::json!(true))?;
        }
        Ok(())
    }
    fn rows_of(&self, id: &str, scheduled: Scheduled) -> Result<Vec<ScheduleRow>> {
        self.dsp(id)?.query_as(
            "SELECT * FROM collection_schedules \
            WHERE collection IN (SELECT value FROM json_each(?)) ORDER BY created_at,id",
            [scheduled.names()?],
        )
    }
    /// The soonest run of `collection`'s schedule, while it's on.
    pub fn next_scheduled(&self, id: &str, collection: &str) -> Result<Option<String>> {
        let rows = self.rows_of(id, Scheduled::named(collection)?)?;
        Ok(rows
            .into_iter()
            .filter(|row| row.enabled)
            .filter_map(|row| row.next_run)
            .min())
    }
    /// How the activity log names the run of the rule `schedule`, while there is one: by the
    /// collection it runs, as "Routes".
    pub fn schedule_label(&self, id: &str, schedule: &str) -> Result<Option<&'static str>> {
        let row: Option<ScheduleRow> = self.dsp(id)?.one_as(
            "SELECT * FROM collection_schedules WHERE id=? \
            AND collection IN (SELECT value FROM json_each(?))",
            [schedule, &ScheduleCollection::known()?],
        )?;
        Ok(row.and_then(|row| {
            Scheduled::all()
                .find(|scheduled| scheduled.offers(row.collection.as_str()).is_ok())
                .map(|scheduled| scheduled.collection.label)
        }))
    }
    /// Today, where the DSP is.
    pub fn local_date(&self, id: &str) -> Result<String> {
        let tz = timezone(&self.find_dsp(id)?.timezone)?;
        Ok(crate::foundation::clock::today(tz).to_string())
    }
    // An alias, such as Routes' `routes_today`, selects the collections its feature declares
    // it runs; any other value selects the collection it names. Each comes with its provider.
    fn scheduled_collections(
        collection: ScheduleCollection,
    ) -> impl Iterator<Item = (Provider, &'static Collection)> {
        registry()
            .scheduled(collection.as_str())
            .map(|(collector, scheduled)| (Provider::new(collector.id()), scheduled))
    }
    fn check_schedule_sources(&self, id: &str, collection: ScheduleCollection) -> Result<()> {
        for (provider, scheduled) in Self::scheduled_collections(collection) {
            ensure(
                self.connection_for(id, provider)?.enabled,
                scheduled.unconnected,
                409,
            )?;
            registry()
                .keeper(scheduled.job_kind)
                .schedule_ready(self, id)?;
        }
        Ok(())
    }

    /// The collections whose schedules `c` sets, as the Collectors page shows them.
    pub fn collectors(&self, c: &Context) -> Result<Collectors> {
        let collections = Scheduled::all()
            .filter(|scheduled| c.can(scheduled.keeper.schedules().permission))
            .map(|scheduled| {
                let collects = c.can(scheduled.keeper.permission());
                self.scheduled_collection(&c.dsp.id, scheduled, collects)
            })
            .collect::<Result<_>>()?;
        Ok(Collectors {
            timezone: c.dsp.timezone.clone(),
            collections,
        })
    }
    /// One collection of the DSP `id`'s as the Collectors page shows it, to a member who
    /// `can_collect` it or not.
    pub fn scheduled_collection(
        &self,
        id: &str,
        scheduled: Scheduled,
        can_collect: bool,
    ) -> Result<ScheduledCollection> {
        let rows = self.rows_of(id, scheduled)?;
        let kind = scheduled.collection.job_kind;
        let last: Option<(JobStatus, Option<String>, Option<String>)> =
            self.jobs.one_as(LAST_RUN, [id, kind])?;
        let kinds = serde_json::to_string(&[kind])?;
        let running = self.jobs.one(ACTIVE_JOB, [id, kinds.as_str()])?.is_some();
        let choice = scheduled
            .keeper
            .schedules()
            .choice
            .map(|(label, _)| ScheduleChoice {
                label: label.to_owned(),
                options: scheduled
                    .options()
                    .into_iter()
                    .map(|(collection, label)| ScheduleOption {
                        collection,
                        label: label.to_owned(),
                    })
                    .collect(),
            });
        Ok(ScheduledCollection {
            collection: ScheduleCollection::parse(scheduled.collection.schedule)
                .ok_or_else(|| Error::new("not_found", 404))?,
            label: scheduled.collection.label.to_owned(),
            feature: crate::tenancy::catalog::automation(scheduled.collection.job_kind)
                .unwrap_or_default()
                .to_owned(),
            collector: scheduled.collector.id().to_owned(),
            collector_label: scheduled.collector.label().to_owned(),
            connected: self
                .connection_for(id, scheduled.provider())
                .is_ok_and(|connection| connection.enabled),
            choice,
            enabled: rows.iter().any(|row| row.enabled),
            revision: revision(&rows),
            next_run: rows
                .iter()
                .filter(|row| row.enabled)
                .filter_map(|row| row.next_run.clone())
                .min(),
            rules: rows.iter().map(rule_of).collect(),
            last_run: last.and_then(|(status, at, error)| {
                Some(LastRun {
                    status,
                    at: at?,
                    error,
                })
            }),
            running,
            can_collect,
        })
    }
    /// Replaces a collection's rules with those of `value`, as at its `revision`: a rule
    /// with an id keeps its row, one without is new, and one left out is removed. It stays
    /// on or off as it was; one with no rules yet starts on, unless what it reads isn't ready,
    /// as its collector not connected. Answers what the activity log keeps of the change: its
    /// rules as they were and are, as words.
    pub fn save_rules(
        &self,
        id: &str,
        scheduled: Scheduled,
        value: &Value,
    ) -> Result<Option<AuditChange>> {
        v::fields(value, &["rules", "revision"])?;
        let requested = requested(scheduled, value)?;
        let tz = timezone(&self.find_dsp(id)?.timezone)?;
        let db = self.dsp(id)?;
        db.transaction(|| {
            let before = self.rows_of(id, scheduled)?;
            ensure(
                revision(&before) == v::text(value, "revision", 0, 8192)?,
                "schedule_changed",
                409,
            )?;
            let others =
                db.count("SELECT count(*) FROM collection_schedules", [])? - before.len() as i64;
            ensure(
                others + requested.len() as i64 <= MOST_RULES,
                "schedule_limit",
                409,
            )?;
            let mut collects: Vec<ScheduleCollection> = Vec::new();
            for (_, collection, _) in &requested {
                if !collects.contains(collection) {
                    collects.push(*collection);
                }
            }
            let ready = || -> Result<()> {
                for collection in &collects {
                    self.check_schedule_sources(id, *collection)?;
                }
                Ok(())
            };
            // A collection's first rules start it on once what it reads is ready, and off till
            // then; one on stays on, and needs it ready.
            let enabled = if before.is_empty() {
                ready().is_ok()
            } else {
                before.iter().any(|row| row.enabled)
            };
            if enabled {
                ready()?;
            }
            let mut kept = Vec::new();
            for (key, collects, rule) in &requested {
                let row = match key {
                    Some(key) => Some(
                        before
                            .iter()
                            .find(|row| &row.id == key && !kept.contains(&row.id))
                            .ok_or_else(|| Error::new("schedule_changed", 409))?,
                    ),
                    None => None,
                };
                let same = row.filter(|row| row.collection == *collects && row.rule == *rule);
                let (cadence, interval, first) = legacy(rule);
                let anchor = match same {
                    Some(row) => row.anchor,
                    None => anchor(first, tz, now())?,
                };
                let next = if !enabled {
                    None
                } else {
                    match same
                        .filter(|row| row.enabled)
                        .and_then(|row| row.next_run.clone())
                    {
                        Some(pending) => Some(pending),
                        None => Some(next_run(rule, tz, now())?),
                    }
                };
                let (times, until) = match &rule.timing {
                    Timing::At(times) => {
                        let times: Vec<String> = times.iter().map(|at| rules::text(*at)).collect();
                        (Some(serde_json::to_string(&times)?), None)
                    }
                    Timing::Repeat { to, .. } => (None, Some(rules::text(*to))),
                };
                let key = match row {
                    Some(row) => row.id.clone(),
                    None => crypto::id("schedule")?,
                };
                db.exec(
                    SAVE,
                    params![
                        key,
                        rule.describe(),
                        collects,
                        cadence,
                        interval,
                        rules::text(first),
                        anchor,
                        enabled,
                        next,
                        iso(),
                        rule.days,
                        times,
                        until
                    ],
                )?;
                kept.push(key);
            }
            for row in &before {
                if !kept.contains(&row.id) {
                    db.exec("DELETE FROM collection_schedules WHERE id=?", [&row.id])?;
                }
            }
            let words = |rows: Vec<(ScheduleCollection, Rule)>| described(&rows, scheduled);
            let was = words(
                before
                    .iter()
                    .map(|r| (r.collection, r.rule.clone()))
                    .collect(),
            );
            let is = words(requested.iter().map(|(_, c, r)| (*c, r.clone())).collect());
            Ok((was != is).then_some(("rules", was, is)))
        })
    }
    /// Turns a collection's schedule on or off, as at its `revision`: every rule at once.
    /// Answers whether that changed it.
    pub fn switch_rules(&self, id: &str, scheduled: Scheduled, value: &Value) -> Result<bool> {
        v::fields(value, &["enabled", "revision"])?;
        let enabled = v::boolean(value, "enabled")?;
        let tz = timezone(&self.find_dsp(id)?.timezone)?;
        let db = self.dsp(id)?;
        db.transaction(|| {
            let rows = self.rows_of(id, scheduled)?;
            ensure(
                revision(&rows) == v::text(value, "revision", 0, 8192)?,
                "schedule_changed",
                409,
            )?;
            ensure(!rows.is_empty(), "schedule_not_found", 404)?;
            let was = rows.iter().any(|row| row.enabled);
            if was == enabled && rows.iter().all(|row| row.enabled == enabled) {
                return Ok(false);
            }
            for row in &rows {
                if enabled {
                    self.check_schedule_sources(id, row.collection)?;
                }
                let next = if enabled {
                    Some(next_run(&row.rule, tz, now())?)
                } else {
                    None
                };
                db.exec(SWITCH, params![enabled, next, row.id])?;
            }
            Ok(was != enabled)
        })
    }
    /// The next runs of the rules of `value`, unsaved, as though the collection were on.
    pub fn preview_rules(
        &self,
        id: &str,
        scheduled: Scheduled,
        value: &Value,
    ) -> Result<ScheduleRuns> {
        v::fields(value, &["rules"])?;
        let requested = requested(scheduled, value)?;
        let tz = timezone(&self.find_dsp(id)?.timezone)?;
        let rules: Vec<Rule> = requested.into_iter().map(|(_, _, rule)| rule).collect();
        Ok(ScheduleRuns {
            runs: rules::upcoming(&rules, tz, now(), UPCOMING)
                .into_iter()
                .map(at)
                .collect(),
        })
    }
    /// Queues now what one run of a schedule collecting by `value`'s `collects` queues, for
    /// `actor`, once for its `requestId`.
    pub fn collect_now(
        &self,
        id: &str,
        actor: &str,
        scheduled: Scheduled,
        value: &Value,
    ) -> Result<Vec<PublicJob>> {
        v::fields(value, &["collects", "requestId"])?;
        let collects = scheduled.offers(v::text(value, "collects", 1, 64)?)?;
        let request = v::text(value, "requestId", 1, 128)?;
        self.check_schedule_sources(id, collects)?;
        let key = format!("collect:{request}:");
        self.enqueue_batch(
            id,
            Some(actor),
            &self.scheduled_requests(id, collects, &key)?,
        )
    }
    /// What one run of a schedule collecting by `collection` queues, each request keyed
    /// under `key`.
    fn scheduled_requests(
        &self,
        id: &str,
        collection: ScheduleCollection,
        key: &str,
    ) -> Result<Vec<(String, Provider, Value)>> {
        let mut requests = Vec::new();
        for (provider, scheduled) in Self::scheduled_collections(collection) {
            let keeper = registry().keeper(scheduled.job_kind);
            for (suffix, request) in keeper.scheduled(self, id, collection.as_str())? {
                requests.push((format!("{key}{suffix}"), provider, request));
            }
        }
        Ok(requests)
    }

    /// Pauses the schedules that need `provider`: those of its collections, and of each
    /// alias that runs one of them.
    pub fn pause_provider_schedules(&self, id: &str, provider: Provider) -> Result<()> {
        for collection in ScheduleCollection::all() {
            let needs = Self::scheduled_collections(collection).any(|(other, _)| other == provider);
            if needs {
                self.dsp(id)?.exec(PAUSE, [collection.as_str()])?;
            }
        }
        Ok(())
    }
    pub fn retime_schedules(&self, id: &str, tz: &str) -> Result<()> {
        self.retime_schedules_for(id, tz, None)
    }
    pub(crate) fn retime_feature_schedules(&self, id: &str, tz: &str, feature: &str) -> Result<()> {
        self.retime_schedules_for(id, tz, Some(feature))
    }
    /// Runs the DSP's schedules, or a feature's, in `tz` from now on, and wakes the scheduler
    /// once the work is done, whichever route or job it's part of.
    fn retime_schedules_for(&self, id: &str, tz: &str, feature: Option<&str>) -> Result<()> {
        let tz = timezone(tz)?;
        let db = self.dsp(id)?;
        db.transaction(|| {
            let rows: Vec<ScheduleRow> = db.query_as(
                "SELECT * FROM collection_schedules \
                WHERE collection IN (SELECT value FROM json_each(?))",
                [ScheduleCollection::known()?],
            )?;
            for row in rows {
                if feature.is_some_and(|f| {
                    crate::tenancy::catalog::automation(row.collection.as_str()) != Some(f)
                }) {
                    continue;
                }
                let anchor = anchor(legacy(&row.rule).2, tz, now())?;
                let deadline = if row.enabled {
                    Some(next_run(&row.rule, tz, now())?)
                } else {
                    None
                };
                db.exec(RETIME, params![anchor, deadline, row.id])?;
            }
            Ok(())
        })?;
        self.schedules_moved();
        Ok(())
    }
    pub fn schedule_deadlines(&self) -> Result<Vec<(String, i64)>> {
        let mut deadlines = Vec::new();
        let dsps: Vec<(String,)> = self.platform.query_as(
            "SELECT id FROM dsps WHERE status='active' AND environment=?",
            [&self.config.environment],
        )?;
        for (id,) in dsps {
            let enabled = self.features(&id)?;
            if !crate::tenancy::catalog::automates(&enabled) {
                continue;
            }
            // The soonest schedule whose page is on; the others wait for their page.
            let rows: Vec<(String, Option<String>)> = self
                .dsp(&id)?
                .query_as(NEXT_DEADLINE, [ScheduleCollection::known()?])?;
            let next = rows
                .into_iter()
                .find(|(collection, _)| {
                    crate::tenancy::catalog::automation(collection)
                        .is_some_and(|page| enabled.iter().any(|f| f == page))
                })
                .map(|(_, next_run)| next_run);
            if let Some(next_run) = next {
                let deadline = next_run
                    .and_then(|v| chrono::DateTime::parse_from_rfc3339(&v).ok())
                    .map_or(0, |d| d.timestamp_millis());
                deadlines.push((id, deadline));
            }
        }
        Ok(deadlines)
    }
    /// What the rule `row`'s pending run queues, unless that run was queued already, as when
    /// the server stopped after queueing it and before moving the rule on.
    fn due_requests(&self, id: &str, row: &ScheduleRow) -> Result<Vec<(String, Provider, Value)>> {
        let pending = row.next_run.as_deref().unwrap_or("");
        let key = format!("schedule:{}:{}:", row.id, pending);
        if self
            .jobs
            .one(
                "SELECT id FROM jobs WHERE dsp_id=? AND substr(idempotency_key,1,?)=? LIMIT 1",
                params![id, key.len() as i64, key],
            )?
            .is_some()
        {
            return Ok(vec![]);
        }
        self.check_schedule_sources(id, row.collection)?;
        self.scheduled_requests(id, row.collection, &key)
    }
    /// Queues the runs of the DSP's rules that are due, all in one batch, so the collections
    /// due together run together, and moves each rule on to its next run. A rule that can't
    /// run yet keeps its run and says why, and is tried again in a minute. Answers when the
    /// next is due.
    pub fn schedule_due(&self, id: &str) -> Result<Option<i64>> {
        let dsp = self.find_dsp(id)?;
        let enabled = self.features(id)?;
        if !self.serves(&dsp) || !crate::tenancy::catalog::automates(&enabled) {
            return Ok(None);
        }
        let tz = timezone(&dsp.timezone)?;
        let db = self.dsp(id)?;
        let rows: Vec<ScheduleRow> = db.query_as(
            "SELECT * FROM collection_schedules WHERE enabled=1 \
            AND collection IN (SELECT value FROM json_each(?)) ORDER BY next_run,id",
            [ScheduleCollection::known()?],
        )?;
        // A schedule whose page is off waits, as one no feature keeps does.
        let rows: Vec<ScheduleRow> = rows
            .into_iter()
            .filter(|row| {
                let page = crate::tenancy::catalog::automation(row.collection.as_str());
                page.is_some_and(|page| enabled.iter().any(|f| f == page))
            })
            .collect();
        let now_iso = iso();
        let due = rows
            .iter()
            .filter(|row| row.next_run.as_ref().is_some_and(|value| value <= &now_iso));
        // The first of a collection's rules due together queues its run, for them all. The
        // others move on before it's queued: stopped once it is, only the first is still due,
        // and that run is queued already.
        let mut queueing: Vec<(&str, ScheduleCollection)> = Vec::new();
        let mut moved: Vec<&str> = Vec::new();
        let mut waiting: Vec<(&str, String)> = Vec::new();
        let mut requests = Vec::new();
        for row in due {
            if queueing
                .iter()
                .any(|(_, collection)| *collection == row.collection)
            {
                db.exec(
                    "UPDATE collection_schedules SET next_run=?,last_error=NULL WHERE id=?",
                    [&next_run(&row.rule, tz, now())?, &row.id],
                )?;
                moved.push(&row.id);
                continue;
            }
            match self.due_requests(id, row) {
                Ok(due) if due.is_empty() => moved.push(&row.id),
                Ok(due) => {
                    requests.extend(due);
                    queueing.push((&row.id, row.collection));
                }
                Err(error) => waiting.push((&row.id, error.code)),
            }
        }
        if !requests.is_empty() {
            // Other collections finish before another recurring batch enters the queue.
            let started = ensure(
                self.jobs
                    .one(ACTIVE_JOB, [id, JobKind::known()?.as_str()])?
                    .is_none(),
                "sync_in_progress",
                409,
            )
            .and_then(|()| self.enqueue_batch(id, None, &requests));
            let queued = queueing.into_iter().map(|(row, _)| row);
            match started {
                Ok(_) => moved.extend(queued),
                // None of the batch is queued: each rule that queued it waits.
                Err(error) => waiting.extend(queued.map(|row| (row, error.code.clone()))),
            }
        }
        let mut earliest = None;
        let mut soonest = |ms: i64| earliest = Some(earliest.map_or(ms, |v: i64| v.min(ms)));
        for row in &rows {
            if let Some((_, code)) = waiting.iter().find(|(key, _)| *key == row.id) {
                db.exec(
                    "UPDATE collection_schedules SET last_error=? WHERE id=?",
                    [code, &row.id],
                )?;
                soonest(now() + 60000);
                continue;
            }
            let deadline = match &row.next_run {
                Some(pending) if !moved.contains(&row.id.as_str()) => pending.clone(),
                _ => next_run(&row.rule, tz, now())?,
            };
            db.exec(
                "UPDATE collection_schedules SET next_run=?,last_error=NULL WHERE id=?",
                [&deadline, &row.id],
            )?;
            let ms = chrono::DateTime::parse_from_rfc3339(&deadline)
                .map_err(|_| Error::new("invalid_schedule", 500))?
                .timestamp_millis();
            soonest(ms);
        }
        Ok(earliest)
    }
}
