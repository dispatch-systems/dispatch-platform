//! What DVIC exposes to every other feature and agent tool: its short inspections, read as its
//! own pages read them (`dispatch_dvic_contract`).
use super::DvicStore;
use dispatch_core::{Result, db::Store, manifest::people::Source};
use dispatch_dvic_contract::{Asked, Collected, Driver, Inspection, Page, ShortInspections};

/// DVIC's short inspections, as `ShortInspections` reads them.
pub(crate) struct Reader;
pub(crate) static READER: Reader = Reader;

impl ShortInspections for Reader {
    fn source(&self) -> Source {
        dispatch_cortex::SOURCE
    }
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<Collected>> {
        Ok(db.dvic_coverage(dsp)?.map(|coverage| Collected {
            from: coverage.from,
            through: coverage.through,
        }))
    }
    fn drivers(&self, db: &Store, dsp: &str) -> Result<Vec<Driver>> {
        Ok(db
            .dvic_drivers(dsp)?
            .into_iter()
            .map(|driver| Driver {
                id: driver.id,
                name: driver.name,
                first_date: driver.first_date,
            })
            .collect())
    }
    fn inspections(&self, db: &Store, dsp: &str, asked: &Asked<'_>) -> Result<Page> {
        let page = db.dvic_inspections(
            dsp,
            asked.from,
            asked.to,
            asked.driver,
            asked.after,
            asked.limit,
        )?;
        Ok(Page {
            inspections: page
                .inspections
                .into_iter()
                .map(|row| Inspection {
                    date: row.start_date,
                    start_time: row.start_time,
                    driver_id: row.driver_id,
                    driver_name: row.driver_name,
                    vehicle_type: row.fleet_type,
                    vin: row.vin,
                    duration_seconds: row.duration_seconds,
                    minimum_seconds: row.minimum_seconds,
                    short_by_seconds: row.short_by_seconds,
                })
                .collect(),
            next: page.next_cursor,
        })
    }
}
