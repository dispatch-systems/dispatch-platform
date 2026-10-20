//! The keeping slot: how a feature keeps a collection a collector collects (`keeps`), and the
//! names a schedule runs several of its collections by (`schedule_aliases`). The engine finds
//! a keeper by its collection's job kind.
use crate::{
    Result, State,
    collection::{
        browser::{Collected, Pending},
        registry::AddedStorage,
    },
    db::{Db, Store},
    manifest::{Collection, Collector, Feature, Registry},
    server::cache::DataDomain,
};
use serde_json::Value;
use std::sync::Arc;

/// A name a schedule collects by other than a collection's own: one that runs several
/// collections at once, as Timecard's `both`, or one that reads a collection another way, as
/// Routes' `routes_today`, which its keeper tells apart by the name `scheduled` is handed.
pub struct ScheduleAlias {
    /// The `collection` a schedule names it by. Permanent: schedules store it.
    pub schedule: &'static str,
    /// The job kinds of the collections it runs, each one its feature keeps.
    pub runs: &'static [&'static str],
}

/// Minimum backlog capacity a collection keeper may declare.
pub const MIN_QUEUE_CAPACITY: i64 = 5;
/// Maximum backlog capacity a collection keeper may declare.
pub const MAX_QUEUE_CAPACITY: i64 = 31;

/// How a feature keeps one collection: what a queued request needs from the DSP before
/// it runs, how what it brings is stored, and what a schedule of it queues. The engine
/// finds it by the collection's job kind.
pub trait Keeper: Sync {
    /// Bounded queued backlog for this collection, independent of browser worker capacity.
    /// The engine clamps declarations to MIN_QUEUE_CAPACITY..=MAX_QUEUE_CAPACITY.
    fn queue_capacity(&self) -> i64 {
        MIN_QUEUE_CAPACITY
    }
    /// The job kind of the collection it keeps.
    fn keeps(&self) -> &'static str;
    /// The permission its collection runs under: what a member needs to queue or cancel
    /// it, and what a job a member queued still needs when it runs.
    fn permission(&self) -> &'static str;
    /// What publishing its collection changes, for the read cache. Without one of its own,
    /// it evicts every read the DSP's settings do.
    fn domain(&self) -> DataDomain {
        DataDomain::TENANT
    }
    /// Adds current tenant context needed only while a queued request executes. The
    /// persisted request stays compatible with the previous binary for rollback.
    fn bind(&self, _: &Store, _dsp: &str, request: &Value) -> Result<Value> {
        Ok(request.clone())
    }
    /// Stores what it can of a finished collection before the job publishes it, in
    /// steps short enough that the platform lock is never held for long; `publish` then
    /// only makes it current. Each step checks the job is still this worker's.
    fn stage<'a>(
        &'a self,
        _state: &'a Arc<State>,
        _dsp: &'a str,
        _job: &'a str,
        _owner: &'a str,
        collected: Collected,
    ) -> Pending<'a, Collected> {
        Box::pin(async move { Ok(collected) })
    }
    /// Stores a finished collection. Runs while the job is still this worker's.
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()>;
    /// Answers a running collection that asks what is already kept for the DSP, so that
    /// it reads from the site only what it lacks. The question and the answer are the
    /// collection's own JSON.
    fn kept(&self, _: &Store, _dsp: &str, _question: &Value) -> Result<Value> {
        Ok(Value::Null)
    }
    /// When `date` was last collected, as a row with `collected_at`.
    fn collected_at(&self, _: &Store, _dsp: &str, _date: &str) -> Result<Option<Value>> {
        Ok(None)
    }
    /// When what it holds for the DSP was last collected, for the DSP list.
    fn last_collected(&self, _: &Store, _dsp: &str) -> Result<Option<String>> {
        Ok(None)
    }
    /// What else a schedule needs before it can run the collection.
    fn schedule_ready(&self, _: &Store, _dsp: &str) -> Result<()> {
        Ok(())
    }
    /// The jobs one run of a schedule queues: an idempotency key suffix and a request each.
    /// `schedule` is the name the schedule collects by: the collection's own, or an alias of
    /// its feature's that runs it.
    fn scheduled(&self, _: &Store, _dsp: &str, _schedule: &str) -> Result<Vec<(String, Value)>> {
        Ok(vec![])
    }
    /// The databases it keeps the collection in, beside its collector's own.
    fn storages(&self) -> &'static [&'static AddedStorage] {
        &[]
    }
    /// Fails closed when what it keeps in its collector's database was lost, wherever that
    /// database is verified.
    fn verify(&self, _: &Db) -> Result<()> {
        Ok(())
    }
    /// What its last publication holds, read from its collector's database alone, as the
    /// collection brings it: what an operator's probe measures a new collection against.
    fn published(&self, _: &Db) -> Result<Vec<Value>> {
        Ok(vec![])
    }
}

impl Registry {
    /// Every feature's keepers, in the registry's order.
    pub fn keepers(&self) -> impl Iterator<Item = &'static dyn Keeper> {
        self.features
            .iter()
            .flat_map(|feature| feature.keeps.iter().copied())
    }
    /// The feature that keeps the collection a job of `kind` collects.
    pub fn keeping(&self, kind: &str) -> Option<&'static Feature> {
        self.features
            .iter()
            .copied()
            .find(|feature| feature.keeps.iter().any(|keeper| keeper.keeps() == kind))
    }
    /// The keeper of the collection a job of `kind` collects.
    pub fn keeper(&self, kind: &str) -> &'static dyn Keeper {
        self.keepers()
            .find(|keeper| keeper.keeps() == kind)
            .unwrap_or_else(|| panic!("no feature keeps {kind}"))
    }
    /// Every feature's schedule aliases, in the registry's order.
    pub fn schedule_aliases(&self) -> impl Iterator<Item = &'static ScheduleAlias> {
        self.features
            .iter()
            .flat_map(|feature| feature.schedule_aliases)
    }
    /// Every collection a schedule may name: the collections an alias runs, the aliases,
    /// then the other collections, each in the registry's order.
    pub fn schedule_collections(&self) -> Vec<&'static str> {
        let (together, alone): (Vec<&'static Collection>, Vec<_>) = self
            .collectors
            .iter()
            .flat_map(|collector| collector.collections())
            .partition(|collection| {
                self.schedule_aliases()
                    .any(|alias| alias.runs.contains(&collection.job_kind))
            });
        let aliases = self.schedule_aliases().map(|alias| alias.schedule);
        together
            .iter()
            .map(|collection| collection.schedule)
            .chain(aliases)
            .chain(alone.iter().map(|collection| collection.schedule))
            .collect()
    }
    /// What a schedule of `collection` runs, each collection with its collector, in the
    /// registry's order: the collections the alias of that name runs, or the one it names.
    pub fn scheduled<'a>(
        &'a self,
        collection: &'a str,
    ) -> impl Iterator<Item = (&'static dyn Collector, &'static Collection)> + 'a {
        let alias = self
            .schedule_aliases()
            .find(|alias| alias.schedule == collection);
        self.collectors.iter().copied().flat_map(move |collector| {
            collector
                .collections()
                .iter()
                .filter(move |scheduled| match alias {
                    Some(alias) => alias.runs.contains(&scheduled.job_kind),
                    None => scheduled.schedule == collection,
                })
                .map(move |scheduled| (collector, scheduled))
        })
    }
}

/// Panics unless every collection has exactly one keeper, every keeper keeps a registered
/// collection, and every schedule alias has a name of its own and runs only collections its
/// feature keeps.
pub(crate) fn check(registry: &Registry) {
    let kinds: Vec<&str> = registry
        .collectors
        .iter()
        .flat_map(|collector| collector.collections())
        .map(|collection| collection.job_kind)
        .collect();
    for kind in &kinds {
        let keepers = registry.keepers().filter(|k| k.keeps() == *kind).count();
        assert!(keepers == 1, "{kind} has {keepers} keepers, not one");
    }
    for keeper in registry.keepers() {
        assert!(
            kinds.contains(&keeper.keeps()),
            "{} is kept, but no collector collects it",
            keeper.keeps()
        );
    }
    let mut schedule_names: Vec<&str> = registry
        .collectors
        .iter()
        .flat_map(|collector| collector.collections())
        .map(|collection| collection.schedule)
        .collect();
    for feature in registry.features {
        for alias in feature.schedule_aliases {
            assert!(
                !schedule_names.contains(&alias.schedule),
                "the {} schedule collection is declared twice",
                alias.schedule
            );
            schedule_names.push(alias.schedule);
            for kind in alias.runs {
                assert!(
                    feature.keeps.iter().any(|keeper| keeper.keeps() == *kind),
                    "{}'s {} schedule runs {kind}, which it does not keep",
                    feature.name,
                    alias.schedule
                );
            }
        }
    }
}
