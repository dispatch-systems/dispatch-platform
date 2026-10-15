//! The associates Amazon's roster names, as Driver Match reads them: everyone it has listed,
//! by the name Amazon has on file for them. The roster says who works for the DSP now; it
//! says nothing of the days anyone worked.
use crate::DriverProfilesStore;
use dispatch_core::{
    Result,
    db::{Store, s},
    manifest::people::{self, Appearances, DriverData, DriverSource, Named, People, PeopleData},
};

/// What it names people in, as Driver Match lists it.
const DATA: PeopleData = PeopleData {
    id: "driver_profiles",
    order: 15,
    label: "Roster",
    unit: "listing",
    icon: "Contact",
};
pub struct Associates;
impl People for Associates {
    fn data(&self) -> DriverData {
        DriverData::new(&DATA)
    }
    fn source(&self) -> DriverSource {
        DriverSource::new(&dispatch_cortex::PEOPLE)
    }
    /// Amazon's first: the roster holds the name Amazon has on file.
    fn order(&self) -> u16 {
        15
    }
    /// Listed while the roster lists them as active, with the email address and personal
    /// phone number Amazon has: a work phone may be the DSP's, passed from driver to driver.
    fn named(&self, store: &Store, dsp: &str) -> Result<Vec<Named>> {
        let rows = store.driver_profiles_db(dsp)?.all(
            "SELECT transporter_id id,name,listed=1 AND status='ACTIVE' current,email,\
             personal_phone phone,roles FROM associates",
            [],
        )?;
        Ok(rows
            .iter()
            .map(|row| Named {
                name: s(row, "name").to_owned(),
                listed: row["current"] == 1,
                emails: row["email"]
                    .as_str()
                    .map(str::to_owned)
                    .into_iter()
                    .collect(),
                phones: row["phone"]
                    .as_str()
                    .map(str::to_owned)
                    .into_iter()
                    .collect(),
                roles: serde_json::from_str(s(row, "roles")).unwrap_or_default(),
                ..Named::new(s(row, "id"))
            })
            .collect())
    }
    fn name(&self, store: &Store, dsp: &str, id: &str) -> Result<Option<String>> {
        people::first_name(
            &*store.driver_profiles_db(dsp)?,
            "SELECT name FROM associates WHERE transporter_id=?",
            id,
        )
    }
    /// None: being listed is not something anyone did.
    fn appearances(&self, _: &Store, _dsp: &str) -> Result<Vec<Appearances>> {
        Ok(vec![])
    }
}
