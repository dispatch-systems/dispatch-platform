//! Actor-bound receipts, committed with a photo or signature, for uncertain network outcomes.
use dispatch_core::{
    Result,
    accounts::Context,
    db::{Db, FromRow, Row},
    ensure,
};
use rusqlite::params;

struct Receipt {
    actor: String,
    writeup: String,
    kind: String,
    revision: i64,
    payload: String,
    result: i64,
}
impl FromRow for Receipt {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            actor: row.get("actor_id")?,
            writeup: row.get("writeup_id")?,
            kind: row.get("kind")?,
            revision: row.get("expected_revision")?,
            payload: row.get("payload")?,
            result: row.get("result_revision")?,
        })
    }
}

pub(super) struct Operation<'a> {
    pub request: Option<&'a str>,
    pub kind: &'a str,
    pub writeup: &'a str,
    pub revision: i64,
    pub payload: &'a str,
}
impl Operation<'_> {
    pub fn previous(&self, db: &Db, c: &Context) -> Result<Option<i64>> {
        let Some(request) = self.request else {
            return Ok(None);
        };
        ensure(
            dispatch_core::foundation::validate::token(request, 100) && request.len() >= 16,
            "invalid_input",
            400,
        )?;
        let stored: Option<Receipt> = db.one_as(
            "SELECT actor_id,writeup_id,kind,expected_revision,payload,result_revision FROM conduct_requests WHERE id=?", [request],
        )?;
        let Some(Receipt {
            actor,
            writeup,
            kind,
            revision,
            payload,
            result,
        }) = stored
        else {
            return Ok(None);
        };
        ensure(
            actor == c.auth.user.id.as_str()
                && writeup == self.writeup
                && kind == self.kind
                && revision == self.revision
                && payload == self.payload,
            "conduct_request_conflict",
            409,
        )?;
        Ok(Some(result))
    }
    pub fn record(&self, db: &Db, c: &Context, result: i64) -> Result<()> {
        if let Some(request) = self.request {
            db.exec("INSERT INTO conduct_requests (id,actor_id,writeup_id,kind,expected_revision,payload,result_revision) VALUES (?,?,?,?,?,?,?)", params![request,c.auth.user.id,self.writeup,self.kind,self.revision,self.payload,result])?;
        }
        Ok(())
    }
}
