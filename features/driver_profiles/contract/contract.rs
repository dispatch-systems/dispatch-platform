//! Amazon's roster of the DSP's associates, as any feature or agent tool reads it without
//! depending on Driver Profiles: whether Amazon still lists each, their status, roles,
//! qualifications and license expiry, and when the roster was collected. Their names, email
//! addresses, personal phone numbers and photos are the directory's. Driver Profiles exposes it
//! as `amazon_roster`.
use dispatch_core::{Result, db::Store, manifest::people::Source};

/// Reads what Amazon's roster says of a DSP's associates, as Driver Profiles keeps it.
pub trait AmazonRoster: Send + Sync {
    /// The source whose IDs name its associates, to find them among the DSP's people.
    fn source(&self) -> Source;
    /// When the roster was last collected, RFC 3339 in UTC, or none before the first collection.
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<String>>;
    /// The associates a roster has listed under `ids`, in their order, leaving out an ID no
    /// roster has listed, each matched exactly. Driver Profiles refuses an empty ID, one over 128
    /// characters, or more than 32 IDs with `invalid_input`.
    fn associates(&self, db: &Store, dsp: &str, ids: &[String]) -> Result<Vec<Associate>>;
}

/// One associate, as the newest roster to list them did.
#[derive(Clone, Debug)]
pub struct Associate {
    /// Their ID in `source`.
    pub id: String,
    /// Whether the newest roster lists them, offboarded ones included. One it stopped listing
    /// keeps what the last roster to list them said, so their status may be out of date.
    pub listed: bool,
    /// Their status, as Amazon writes it: `ACTIVE`, `INACTIVE` or `OFFBOARDED`.
    pub status: String,
    /// What they may do, as Amazon writes it: `Driver`, `Helper`.
    pub roles: Vec<String>,
    /// The vans and kinds of work they are qualified for, as Amazon writes them: `CDV`, `EDV`.
    pub qualifications: Vec<String>,
    /// The day their driver's license expires, YYYY-MM-DD, when Amazon has it.
    pub license_expires: Option<String>,
    /// When a roster last listed them, RFC 3339 in UTC.
    pub last_listed: String,
}

/// What reads the DSP's Amazon roster: only while Driver Profiles is on there.
pub fn for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn AmazonRoster>> {
    dispatch_core::manifest::exposes::for_dsp::<dyn AmazonRoster>(db, dsp)
}
