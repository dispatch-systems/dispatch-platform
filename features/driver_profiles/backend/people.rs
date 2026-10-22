//! The associates Amazon's roster names, as Driver Match reads them: everyone it has listed,
//! by the name Amazon has on file for them. The roster says who works for the DSP now; it
//! says nothing of the days anyone worked.
use crate::DriverProfilesStore;
use dispatch_core::manifest::people::{self, KindInfo, People};

/// Everyone the roster has listed, listed while it lists them as active and offboarded once it
/// lists them so, with the email address and personal phone number Amazon has: a work phone may
/// be the DSP's, passed from driver to driver. Whoever it still lists has their photo, by its
/// digest.
pub const PEOPLE: People = People::new(
    KindInfo {
        id: "driver_profiles",
        order: 15,
        label: "Roster",
        icon: "Contact",
    },
    dispatch_cortex::SOURCE,
    |store, dsp| {
        people::named(
            &*store.driver_profiles_db(dsp)?,
            "SELECT a.transporter_id id,a.name,a.listed=1 AND a.status='ACTIVE' listed,\
             a.status='OFFBOARDED' offboarded,a.email,a.personal_phone phone,a.roles,\
             p.sha256 photo FROM associates a \
             LEFT JOIN photos p ON p.address=a.photo_address AND a.listed=1",
        )
    },
)
.photos(|store, dsp, ids| store.photo(dsp, ids));
