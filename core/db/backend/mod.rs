mod connection;
mod files;
pub mod migrations;
pub mod objects;
mod row;
mod schema;
mod store;
pub use crate::tenancy::audit::{AuditChange, AuditQuery};
pub use connection::{Db, boolean, flag, n, s};
pub use files::{key_file, lock_file, private_dir, private_file, read_only, retire, write_private};
pub use migrations::{
    Kind, Migration, Migrations, OwnMigrations, OwnedMigration, migrate, migrate_for, owned_ledger,
};
pub use row::{FromRow, Row};
/// Core's own databases, what its parts add to them and the tables it keeps, as the
/// registry gathers them.
pub use schema::{
    DATABASES as CORE_DATABASES, MIGRATIONS as CORE_MIGRATIONS, TABLES as CORE_TABLES,
};
pub use store::{CACHE, DspLease, Store};
pub fn identifier(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|s| crate::foundation::crypto::is_hex(s, 32))
}
pub fn now() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}
pub fn iso() -> String {
    at(now())
}
/// The instant an RFC 3339 time is, as `at` writes one, in milliseconds since the epoch.
pub fn millis(at: &str) -> Option<i64> {
    let at = at.parse::<jiff::Timestamp>().ok()?;
    Some(at.as_millisecond())
}
pub fn at(ms: i64) -> String {
    jiff::Timestamp::from_millisecond(ms)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .strftime("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}
