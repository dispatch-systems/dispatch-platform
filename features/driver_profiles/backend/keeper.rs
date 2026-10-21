//! Driver Profiles keeps Cortex's `associates` collection: the roster, and the photos the
//! DSP lacked, written to its cache before the roster is published.
use super::storage::{STORAGE, bind_roster_request, roster_request};
use super::{DOMAIN, photos};
use crate::DriverProfilesStore;
use dispatch_core::{
    Error, Result, State,
    collection::{
        browser::{Collected, Pending},
        registry::AddedStorage,
    },
    db::{Store, s},
    manifest::{Keeper, Schedules, schedules},
    server::cache::DataDomain,
};
use dispatch_cortex::associates::{Capture, JOB_KIND};
use serde_json::Value;
use std::sync::Arc;

pub(crate) struct Roster;
impl Keeper for Roster {
    fn keeps(&self) -> &'static str {
        JOB_KIND
    }
    fn permission(&self) -> &'static str {
        "driver_profiles.collect"
    }
    fn schedules(&self) -> Schedules {
        schedules("driver_profiles.collect")
    }
    fn domain(&self) -> DataDomain {
        DOMAIN
    }
    fn bind(&self, store: &Store, dsp: &str, _: &Value) -> Result<Value> {
        bind_roster_request(store, dsp)
    }
    /// The photos still standing for the company the question names.
    fn kept(&self, store: &Store, dsp: &str, question: &Value) -> Result<Value> {
        Ok(serde_json::to_value(
            store.kept_photos(dsp, s(question, "company"))?,
        )?)
    }
    /// Writes the photos' files, outside the database, before publishing makes them current.
    fn stage<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        job: &'a str,
        owner: &'a str,
        collected: Collected,
    ) -> Pending<'a, Collected> {
        Box::pin(async move {
            let mut capture: Capture = serde_json::from_value(collected.data)?;
            let (tenant, jid, worker) = (dsp.to_owned(), job.to_owned(), owner.to_owned());
            let dir = state
                .read(move |db| {
                    db.guard(&jid, &worker)?;
                    photos::dir(db, &tenant)
                })
                .await?;
            let mut taken = std::mem::take(&mut capture.photos);
            capture.photos = tokio::task::spawn_blocking(move || {
                photos::write(&dir, &mut taken)?;
                Ok::<_, Error>(taken)
            })
            .await
            .map_err(|_| Error::new("associates_capture_invalid", 500))??;
            Ok(Collected {
                data: serde_json::to_value(capture)?,
                scope: collected.scope,
            })
        })
    }
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()> {
        store.publish_roster(dsp, job, &serde_json::from_value(collected.data)?)
    }
    fn last_collected(&self, store: &Store, dsp: &str) -> Result<Option<String>> {
        Ok(store
            .driver_profiles_db(dsp)?
            .query_as::<(Option<String>,)>("SELECT max(collected_at) FROM roster_runs", [])?
            .into_iter()
            .next()
            .and_then(|(at,)| at))
    }
    fn schedule_ready(&self, store: &Store, dsp: &str) -> Result<()> {
        bind_roster_request(store, dsp).map(|_| ())
    }
    fn scheduled(&self, _: &Store, _dsp: &str, _: &str) -> Result<Vec<(String, Value)>> {
        Ok(vec![("associates".into(), roster_request())])
    }
    fn storages(&self) -> &'static [&'static AddedStorage] {
        static STORAGES: [&AddedStorage; 1] = [&STORAGE];
        &STORAGES
    }
}
