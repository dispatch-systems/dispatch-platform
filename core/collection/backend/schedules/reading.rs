//! Collectors page and activity-log projections of a DSP's schedules.
use super::{
    Scheduled,
    storage::{ScheduleRow, revision},
    timezone,
};
use crate::{
    Error, Result,
    accounts::Context,
    collection::{
        api::{
            jobs::JobStatus,
            types::{
                Collectors, LastRun, RuleKind, ScheduleChoice, ScheduleCollection, ScheduleOption,
                ScheduleRule, ScheduledCollection,
            },
        },
        rules::{self, Timing},
    },
    db::Store,
    job_statuses,
};

/// What the newest run of a collection queued or under way says it's doing.
const RUNNING: &str = concat!(
    "SELECT message FROM jobs WHERE dsp_id=? AND kind=? AND status IN ",
    job_statuses!(active),
    " ORDER BY created_at DESC LIMIT 1"
);
const LAST_RUN: &str = "SELECT status,completed_at,error FROM jobs WHERE dsp_id=? AND kind=? \
    AND status IN ('succeeded','failed') ORDER BY created_at DESC LIMIT 1";
/// The metrics of a collection's latest successful run, as its last attempt kept them.
const LAST_COUNTS: &str = "SELECT m.metrics FROM jobs j JOIN job_metrics m ON m.job_id=j.id \
    WHERE j.dsp_id=? AND j.kind=? AND j.status='succeeded' \
    ORDER BY j.created_at DESC, m.attempt DESC LIMIT 1";

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
        let running: Option<(String,)> = self.jobs.one_as(RUNNING, [id, kind])?;
        let counts: Option<(String,)> = self.jobs.one_as(LAST_COUNTS, [id, kind])?;
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
            counts: counts.and_then(|(metrics,)| serde_json::from_str(&metrics).ok()),
            running: running.is_some(),
            message: running.map(|(message,)| message).unwrap_or_default(),
            can_collect,
        })
    }
}
