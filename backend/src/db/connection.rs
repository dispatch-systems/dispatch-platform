use super::{
    Kind, migrations, private_file,
    row::{FromRow, Row},
};
use crate::{Result, ensure};
use rusqlite::{Connection, Params, types::ValueRef};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
fn row_json(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let mut out = serde_json::Map::new();
    for i in 0..row.as_ref().column_count() {
        let name = row.as_ref().column_name(i)?;
        let value = match row.get_ref(i)? {
            ValueRef::Null | ValueRef::Blob(_) => Value::Null,
            ValueRef::Integer(n) => json!(n),
            ValueRef::Real(n) => json!(n),
            ValueRef::Text(s) => json!(String::from_utf8_lossy(s)),
        };
        out.insert(name.to_owned(), value);
    }
    Ok(Value::Object(out))
}
pub struct Db(pub Connection);
impl Db {
    /// Opens an existing database. Its schema is left alone: requests take this path.
    pub(crate) fn open(file: &Path, kind: Kind) -> Result<Self> {
        Self::connect(file, kind, false)
    }
    /// Startup and provisioning: creates the database when it is missing, then
    /// applies the migrations it lacks. `seed` holds the rows a new database needs
    /// and commits together with its schema, so no reader sees one without them.
    pub(crate) fn create(file: &Path, kind: Kind, seed: &str) -> Result<Self> {
        let db = Self::connect(file, kind, true)?;
        if db.version()? == 0 {
            let tx = migrations::immediate(&db)?;
            // Another connection may have created it while this one waited.
            if db.version()? == 0 {
                migrations::apply(&db, kind.name(), kind.migrations())?;
                tx.execute_batch(seed)?;
                tx.pragma_update(None, "user_version", kind.version())?;
            }
            tx.commit()?;
        }
        migrations::migrate(&db, kind)?;
        Ok(db)
    }
    /// Creates new core DSP storage with its identity in the schema transaction,
    /// or adopts a legacy database only while its identity migration is first applied.
    pub(crate) fn create_dsp(file: &Path, id: &str) -> Result<Self> {
        ensure(super::identifier(id, "dsp_"), "invalid_dsp_id", 400)?;
        let db = Self::connect(file, Kind::Dsp, true)?;
        if db.version()? == 0 {
            let tx = migrations::immediate(&db)?;
            if db.version()? == 0 {
                migrations::apply(&db, Kind::Dsp.name(), Kind::Dsp.migrations())?;
                db.0.execute(
                    "INSERT INTO storage_identity(dsp_id,provider,source) \
                     VALUES (?,'dispatch','dispatch-v1')",
                    [id],
                )?;
                tx.pragma_update(None, "user_version", Kind::Dsp.version())?;
            }
            tx.commit()?;
        }
        migrations::migrate_dsp(&db, id)?;
        Ok(db)
    }
    fn version(&self) -> Result<i64> {
        Ok(self.0.query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }
    fn connect(file: &Path, kind: Kind, create: bool) -> Result<Self> {
        private_file(file, create)?;
        for suffix in ["-wal", "-shm", "-journal"] {
            private_file(&PathBuf::from(format!("{}{suffix}", file.display())), false)?;
        }
        let db = Connection::open_with_flags(
            file,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        db.create_collation("dispatch_unicode", crate::workforce::compare)?;
        let flags = rusqlite::functions::FunctionFlags::SQLITE_UTF8
            | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC;
        db.create_scalar_function("dispatch_name", 2, flags, |ctx| {
            Ok(crate::workforce::display_name(
                &ctx.get::<String>(0)?,
                &ctx.get::<String>(1)?,
            ))
        })?;
        db.create_scalar_function("dispatch_lower", 1, flags, |ctx| {
            Ok(ctx.get::<String>(0)?.to_lowercase())
        })?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.set_prepared_statement_cache_capacity(32);
        // Kibibytes. The route data database holds a day of tasks per publication and
        // answers day-range reads, so its indexes stay in memory.
        db.pragma_update(
            None,
            "cache_size",
            if kind == Kind::RouteData {
                -32768
            } else {
                -512
            },
        )?;
        let db = Self(db);
        let current = db.version()?;
        ensure(
            (current == 0 && create) || current == kind.version(),
            "incompatible_database",
            503,
        )?;
        db.0.execute_batch("PRAGMA foreign_keys=ON;")?;
        // Moving a new database to WAL takes a lock that ignores the busy timeout,
        // so connections racing to create one retry. Afterwards this changes nothing.
        let began = std::time::Instant::now();
        loop {
            match db.0.execute_batch("PRAGMA journal_mode=WAL;") {
                Err(error)
                    if error.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy)
                        && began.elapsed() < Duration::from_secs(5) =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                result => break result?,
            }
        }
        db.0.execute_batch("PRAGMA synchronous=FULL;")?;
        Ok(db)
    }
    pub fn exec(&self, sql: &str, p: impl Params) -> Result<usize> {
        Ok(self.0.prepare_cached(sql)?.execute(p)?)
    }
    pub fn all(&self, sql: &str, p: impl Params) -> Result<Vec<Value>> {
        let mut stmt = self.0.prepare_cached(sql)?;
        Ok(stmt
            .query_map(p, row_json)?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    /// Reads rows with a hard bound on SQLite virtual-machine work. The handler belongs to
    /// this connection and is removed before it returns, including after an interrupted query.
    pub fn all_bounded(&self, sql: &str, p: impl Params, max_ops: u64) -> Result<Vec<Value>> {
        const INTERVAL: u64 = 1_000;
        let mut callbacks_left = max_ops.div_ceil(INTERVAL).max(1);
        self.0.progress_handler(
            INTERVAL as i32,
            Some(move || {
                callbacks_left = callbacks_left.saturating_sub(1);
                callbacks_left == 0
            }),
        )?;
        let result = (|| {
            let mut stmt = self.0.prepare_cached(sql)?;
            stmt.query_map(p, row_json)?
                .collect::<std::result::Result<Vec<_>, _>>()
        })();
        self.0.progress_handler(0, None::<fn() -> bool>)?;
        match result {
            Err(error)
                if error.sqlite_error_code() == Some(rusqlite::ErrorCode::OperationInterrupted) =>
            {
                Err(crate::Error::new("query_limit_exceeded", 503))
            }
            Err(error) => Err(error.into()),
            Ok(rows) => Ok(rows),
        }
    }
    pub fn one(&self, sql: &str, p: impl Params) -> Result<Option<Value>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .0
            .prepare_cached(sql)?
            .query_row(p, row_json)
            .optional()?)
    }
    /// Every row, read into `T`. A column `T` needs that the query lacks is an error.
    pub fn query_as<T: FromRow>(&self, sql: &str, p: impl Params) -> Result<Vec<T>> {
        let mut stmt = self.0.prepare_cached(sql)?;
        let mut rows = stmt.query(p)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(T::from_row(&Row(row))?);
        }
        Ok(out)
    }
    pub fn one_as<T: FromRow>(&self, sql: &str, p: impl Params) -> Result<Option<T>> {
        let mut stmt = self.0.prepare_cached(sql)?;
        let mut rows = stmt.query(p)?;
        rows.next()?.map(|row| T::from_row(&Row(row))).transpose()
    }
    /// The single number a `SELECT count(*) ...` answers with.
    pub fn count(&self, sql: &str, p: impl Params) -> Result<i64> {
        Ok(self.0.prepare_cached(sql)?.query_row(p, |row| row.get(0))?)
    }
    pub fn transaction<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        let tx = self.0.unchecked_transaction()?;
        let result = f()?;
        tx.commit()?;
        Ok(result)
    }
    pub fn setting(&self, key: &str, default: Value) -> Result<Value> {
        match self.one("SELECT value FROM settings WHERE key=?", [key])? {
            Some(v) => Ok(serde_json::from_str(s(&v, "value"))?),
            None => Ok(default),
        }
    }
    pub fn set(&self, key: &str, value: &Value) -> Result<()> {
        self.exec(
            "INSERT INTO settings(key,value) VALUES (?,?) ON CONFLICT(key) DO UPDATE \
            SET value=excluded.value",
            [key, &value.to_string()],
        )?;
        Ok(())
    }
}
pub fn s<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
pub fn n(value: &Value, key: &str) -> i64 {
    value[key].as_i64().unwrap_or(0)
}
pub fn flag(value: &Value, key: &str) -> bool {
    value[key].as_bool().unwrap_or_else(|| n(value, key) != 0)
}
pub fn boolean(value: &mut Value, keys: &[&str]) {
    for key in keys {
        value[*key] = json!(flag(value, key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bounded_query_is_interrupted_and_leaves_the_connection_reusable() {
        let db = Db(Connection::open_in_memory().unwrap());
        let error = db
            .all_bounded(
                "WITH RECURSIVE n(v) AS (VALUES(1) UNION ALL SELECT v+1 FROM n WHERE v<10000) \
                 SELECT sum(a.v*b.v) total FROM n a CROSS JOIN n b",
                [],
                1_000,
            )
            .unwrap_err();
        assert_eq!(error.code, "query_limit_exceeded");
        assert_eq!(n(&db.one("SELECT 1 n", []).unwrap().unwrap(), "n"), 1);
    }
}
