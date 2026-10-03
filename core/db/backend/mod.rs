#[path = "connection.rs"]
mod connection;
#[path = "files.rs"]
mod files;
#[path = "migrations.rs"]
pub mod migrations;
#[path = "row.rs"]
mod row;
#[path = "schema.rs"]
mod schema;
#[path = "store.rs"]
mod store;
pub use crate::audit::{AuditChange, AuditQuery};
pub use connection::{Db, boolean, flag, n, s};
pub use files::{key_file, private_dir, private_file, write_private};
pub use migrations::{Kind, Migration, Migrations, migrate};
pub use row::{FromRow, Row};
/// Core's own databases, what its parts add to them and the tables it keeps, as the
/// registry gathers them.
pub use schema::{
    DATABASES as CORE_DATABASES, MIGRATIONS as CORE_MIGRATIONS, TABLES as CORE_TABLES,
};
pub use store::{DspLease, Store};
pub fn identifier(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|s| {
        s.len() == 32
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
pub fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
pub fn iso() -> String {
    at(now())
}
pub fn at(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
