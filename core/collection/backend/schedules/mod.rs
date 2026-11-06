//! Collection schedules: registry discovery shared by storage, editing, page reads and dispatch.
mod dispatch;
mod editing;
mod reading;
mod storage;

pub use storage::anchor;

use crate::{
    Error, Result,
    collection::{api::types::ScheduleCollection, registry::Provider, rules::Rule},
    db::{Store, at},
    ensure,
    manifest::{Collection, Collector, Keeper, registry},
};
use chrono_tz::Tz;

fn timezone(tz: &str) -> Result<Tz> {
    tz.parse().map_err(|_| Error::new("invalid_timezone", 400))
}
fn next_run(rule: &Rule, tz: Tz, after: i64) -> Result<String> {
    rule.next(tz, after)
        .map(at)
        .ok_or_else(|| Error::new("schedule_unresolvable", 400))
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

impl Store {
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
}
