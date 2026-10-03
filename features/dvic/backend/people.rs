//! The drivers DVIC's inspections name, as Driver Match reads them.
use crate::{
    Result,
    contracts::{DriverData, DriverSource},
    db::{Store, n, s},
    dvic::DvicStore,
    manifest::people::{self, Appearances, Named, People, Workdays},
};

pub struct Drivers;
impl People for Drivers {
    fn data(&self) -> DriverData {
        DriverData::Dvic
    }
    fn source(&self) -> DriverSource {
        DriverSource::Amazon
    }
    /// Amazon's last: DVIC sometimes writes only a last initial.
    fn order(&self) -> u16 {
        50
    }
    fn named(&self, store: &Store, dsp: &str) -> Result<Vec<Named>> {
        let rows = store.dvic_db(dsp)?.all(
            "SELECT transporter_id id,transporter_name name,min(start_date) first,\
             max(start_date) last FROM dvic_inspections WHERE scope_verified=1 GROUP BY 1,2 ORDER BY last DESC",
            [],
        )?;
        Ok(rows
            .iter()
            .map(|row| {
                let mut named = Named::new(s(row, "id"));
                named.name = s(row, "name").to_owned();
                named.seen(s(row, "first"), s(row, "last"));
                named
            })
            .collect())
    }
    fn name(&self, store: &Store, dsp: &str, id: &str) -> Result<Option<String>> {
        people::first_name(
            &*store.dvic_db(dsp)?,
            "SELECT transporter_name FROM dvic_inspections WHERE transporter_id=? AND scope_verified=1 \
             ORDER BY start_date DESC",
            id,
        )
    }
    fn appearances(&self, store: &Store, dsp: &str) -> Result<Vec<Appearances>> {
        let rows = store.dvic_db(dsp)?.all(
            "SELECT transporter_id id,count(*) count,max(start_date) last \
             FROM dvic_inspections WHERE scope_verified=1 GROUP BY transporter_id",
            [],
        )?;
        Ok(rows
            .iter()
            .map(|row| Appearances {
                id: s(row, "id").to_owned(),
                count: n(row, "count") as usize,
                last: s(row, "last").to_owned(),
            })
            .collect())
    }
    /// An inspection means the driver was out that day, though DVIC alone does not say
    /// which days were collected.
    fn workdays(&self, store: &Store, dsp: &str) -> Result<Workdays> {
        Ok(Workdays {
            worked: store.dvic_db(dsp)?.query_as::<(String, String)>(
                "SELECT DISTINCT transporter_id,start_date FROM dvic_inspections WHERE scope_verified=1",
                [],
            )?,
            collected: vec![],
        })
    }
}
