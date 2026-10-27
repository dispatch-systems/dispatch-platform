//! Pick driver: whom someone at the DSP said held a work phone nobody held, on a day.
use crate::backend::texts::model::Filed;
use dispatch_core::{
    Error, Result,
    db::{Store, now},
};
use rusqlite::params;

/// What texting reads and writes of the drivers picked for work phones.
pub trait PickStore {
    /// Each work phone a driver was picked for on `day`, by its ten digits: the driver's source
    /// and ID.
    fn communications_picked(&self, dsp: &str, day: &str) -> Result<Vec<(String, Filed)>>;
    /// Picks the driver `driver` of the source `source` as holding the work phone `phone`, by its
    /// ten digits, on `day`, as the member `by`.
    fn pick_communications_driver(
        &self,
        dsp: &str,
        phone: &str,
        day: &str,
        driver: &Filed,
        by: &str,
    ) -> Result<()>;
}

impl PickStore for Store {
    fn communications_picked(&self, dsp: &str, day: &str) -> Result<Vec<(String, Filed)>> {
        let picked: Vec<(String, String, String)> = self.dsp(dsp)?.query_as(
            "SELECT phone,source,driver_id FROM communications_picked WHERE day=?",
            [day],
        )?;
        Ok(picked
            .into_iter()
            .map(|(phone, source, id)| (phone, Filed::Driver { source, id }))
            .collect())
    }
    fn pick_communications_driver(
        &self,
        dsp: &str,
        phone: &str,
        day: &str,
        driver: &Filed,
        by: &str,
    ) -> Result<()> {
        let Filed::Driver { source, id } = driver else {
            return Err(Error::new("invalid_input", 400));
        };
        self.dsp(dsp)?.exec(
            "INSERT INTO communications_picked (phone,day,source,driver_id,picked_by,picked_at) \
             VALUES (?,?,?,?,?,?) ON CONFLICT(phone,day) DO UPDATE SET source=excluded.source,\
             driver_id=excluded.driver_id,picked_by=excluded.picked_by,\
             picked_at=excluded.picked_at",
            params![phone, day, source, id, by, now()],
        )?;
        Ok(())
    }
}
