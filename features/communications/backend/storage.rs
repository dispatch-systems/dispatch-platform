//! Communications's storage: its `communications_items` table, in each DSP's own database.
//! Only this crate writes SQL against it.
use dispatch_core::{
    Result,
    db::{FromRow, Row, Store, iso},
    foundation::crypto,
};
use rusqlite::params;
use serde::Serialize;

/// One row of `communications_items`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunicationsItem {
    pub id: String,
    pub name: String,
    pub created_at: String,
}
impl FromRow for CommunicationsItem {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            created_at: row.get("created_at")?,
        })
    }
}

/// What Communications reads and writes for one DSP.
pub trait CommunicationsStore {
    fn communications_items(&self, dsp: &str) -> Result<Vec<CommunicationsItem>>;
    fn add_communications_item(&self, dsp: &str, name: &str) -> Result<CommunicationsItem>;
}
impl CommunicationsStore for Store {
    fn communications_items(&self, dsp: &str) -> Result<Vec<CommunicationsItem>> {
        let db = self.dsp(dsp)?;
        db.query_as(
            "SELECT * FROM communications_items ORDER BY created_at,id",
            [],
        )
    }
    fn add_communications_item(&self, dsp: &str, name: &str) -> Result<CommunicationsItem> {
        let item = CommunicationsItem {
            id: crypto::token()?,
            name: name.to_owned(),
            created_at: iso(),
        };
        let db = self.dsp(dsp)?;
        db.exec(
            "INSERT INTO communications_items (id,name,created_at) VALUES (?,?,?)",
            params![item.id, item.name, item.created_at],
        )?;
        Ok(item)
    }
}

#[cfg(test)]
#[path = "../tests/backend/storage.rs"]
mod tests;
