//! Typed reads. A struct names its columns once in `FromRow`; a column the query did
//! not select, or a value of the wrong type, is an error instead of an empty default.
use crate::{Error, Result};
use rusqlite::types::FromSql;
use serde_json::json;

pub struct Row<'a>(pub(super) &'a rusqlite::Row<'a>);
impl Row<'_> {
    pub fn get<T: FromSql>(&self, column: &str) -> Result<T> {
        self.0.get(column).map_err(|cause| {
            crate::foundation::observability::event(
                "error",
                "database.row_invalid",
                json!({"column":column,"cause":cause.to_string()}),
            );
            Error::new("invalid_stored_record", 500)
        })
    }
}
pub trait FromRow: Sized {
    fn from_row(row: &Row<'_>) -> Result<Self>;
}
/// A query that selects one value, such as an id or a name.
impl<T: FromSql> FromRow for (T,) {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok((row.0.get(0)?,))
    }
}

impl<A: FromSql, B: FromSql> FromRow for (A, B) {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok((row.0.get(0)?, row.0.get(1)?))
    }
}
impl<A: FromSql, B: FromSql, C: FromSql> FromRow for (A, B, C) {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok((row.0.get(0)?, row.0.get(1)?, row.0.get(2)?))
    }
}
impl<A: FromSql, B: FromSql, C: FromSql, D: FromSql> FromRow for (A, B, C, D) {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok((row.0.get(0)?, row.0.get(1)?, row.0.get(2)?, row.0.get(3)?))
    }
}
