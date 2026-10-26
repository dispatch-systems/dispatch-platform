//! Amazon's fleet of the DSP's vans, as any feature or agent tool reads it without depending on
//! Vans: the name the DSP gives each van, by its VIN, and when the fleet was collected. A route
//! names its van by VIN alone; this turns it into the DSP's own name for it. Vans exposes it as
//! `amazon_fleet`.
use dispatch_core::{Result, db::Store};

/// The most VINs one read of the fleet names.
pub const MOST_VINS: usize = 100;

/// Reads what Amazon's fleet says of a DSP's vans, as Vans keeps it.
pub trait AmazonFleet: Send + Sync {
    /// When the fleet was last collected, RFC 3339 in UTC, or none before the first collection.
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<String>>;
    /// The vans a fleet has listed under `vins`, in their order, leaving out a VIN no fleet has
    /// listed. A VIN is matched in capitals, without the spaces around it. Vans refuses an empty
    /// VIN, one over 128 characters, or more than `MOST_VINS` with `invalid_input`.
    fn vans(&self, db: &Store, dsp: &str, vins: &[String]) -> Result<Vec<Van>>;
}

/// One van, as the newest fleet to list it did.
#[derive(Clone, Debug)]
pub struct Van {
    /// Its VIN, in capitals.
    pub vin: String,
    /// What the DSP calls it, as it named it in Amazon's fleet, such as `EDV 214`; none when
    /// it gave none. A name is the van's own while it runs: a returned van's may be another's
    /// since, so name a van by its VIN, never a VIN by a name.
    pub name: Option<String>,
    /// Its status, as Amazon writes it: `ACTIVE`, `MAINTENANCE` or `PENDING` while the DSP
    /// runs it, `INACTIVE` once it is returned, sold or retired.
    pub status: String,
    /// Whether the newest fleet lists it. One it stopped listing keeps what the last fleet to
    /// list it said.
    pub listed: bool,
    /// When a fleet last listed it, RFC 3339 in UTC.
    pub last_listed: String,
}

/// What reads the DSP's Amazon fleet: only while Vans is on there.
pub fn for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn AmazonFleet>> {
    dispatch_core::manifest::exposes::for_dsp::<dyn AmazonFleet>(db, dsp)
}
