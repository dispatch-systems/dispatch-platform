use super::{
    api::types::*, attachments, authorize, conversations, found, jobs, proposals, verification,
};
use crate::{
    Result,
    accounts::Auth,
    db::{FromRow, Row, Store, now, s},
    ensure,
    foundation::{crypto, validate as v},
};
use rusqlite::params;
use serde_json::Value;

impl FromRow for SupportTicket {
    fn from_row(r: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            kind: r.get("kind")?,
            title: r.get("title")?,
            status: r.get("status")?,
            progress: r.get("progress")?,
            reporter_name: r.get("reporter_name")?,
            dsp_id: r.get("dsp_id")?,
            dsp_name: r.get("dsp_name")?,
            proposal_version: r.get("proposal_version")?,
            approved_version: r.get("approved_version")?,
            pr_url: r.get("pr_url")?,
            head_sha: r.get("head_sha")?,
            merge_sha: r.get("merge_sha")?,
            released_version: r.get("released_version")?,
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
        })
    }
}

pub(crate) fn record(db: &Store, id: &str) -> Result<Value> {
    ensure(
        crate::db::identifier(id, "support_"),
        "support_ticket_not_found",
        404,
    )?;
    found(
        db.platform
            .one("SELECT * FROM support_tickets WHERE id=?", [id])?,
    )
}

pub fn list(
    db: &Store,
    auth: &Auth,
    owner: bool,
    status: &str,
    before: i64,
    before_id: &str,
    search: &str,
) -> Result<Vec<SupportTicket>> {
    ensure(
        !owner || auth.user.platform_owner,
        "platform_owner_required",
        403,
    )?;
    db.platform.query_as(
        "SELECT * FROM support_tickets WHERE (? OR (reporter_scope=? AND reporter_id=?)) \
         AND (?='' OR status=?) AND (updated_at<? OR (updated_at=? AND id<?)) \
         AND (?='' OR instr(lower(title||' '||reporter_name||' '||COALESCE(dsp_name,'')),lower(?))>0) \
         ORDER BY updated_at DESC,id DESC LIMIT 100",
        params![owner, auth.scope.as_deref().unwrap_or(""), auth.user.id, status, status,
            before, before, before_id, search, search],
    )
}

pub fn detail(db: &Store, auth: &Auth, id: &str) -> Result<SupportDetail> {
    authorize(db, auth, id)?;
    view(db, id, auth.user.platform_owner)
}

/// The trusted worker's ticket payload, never available through a reporter's endpoint.
pub(crate) fn view(db: &Store, id: &str, private: bool) -> Result<SupportDetail> {
    let row = record(db, id)?;
    Ok(SupportDetail {
        intake: if private {
            super::intake::latest(db, id, row["proposal_version"].as_u64().unwrap_or(0) as u32)?
        } else {
            None
        },
        ticket: found(
            db.platform
                .one_as("SELECT * FROM support_tickets WHERE id=?", [id])?,
        )?,
        description: s(&row, "description").into(),
        source_sha: s(&row, "source_sha").into(),
        release: s(&row, "release").into(),
        messages: conversations::list(db, id, private)?,
        attachments: attachments::list(db, id, private)?,
        proposals: if private {
            proposals::list(db, id)?
        } else {
            vec![]
        },
        evidence: if private {
            verification::list(db, id)?
        } else {
            vec![]
        },
    })
}

pub fn create(db: &Store, auth: &Auth, body: &Value) -> Result<SupportDetail> {
    v::fields(
        body,
        &["requestId", "kind", "title", "description", "attachmentIds"],
    )?;
    let request = v::text(body, "requestId", 16, 100)?;
    let kind = v::choice(body, "kind", &["bug", "feature"])?;
    let title = v::name(body, "title", 180)?;
    let description = v::name(body, "description", 40_000)?;
    let ids: Vec<String> = serde_json::from_value(
        body.get("attachmentIds")
            .cloned()
            .unwrap_or(serde_json::json!([])),
    )?;
    let scope = auth.scope.as_deref().unwrap_or("");
    let dsp_name = auth
        .scope
        .as_deref()
        .map(|id| db.find_dsp(id).map(|dsp| dsp.name))
        .transpose()?;
    db.platform.transaction(|| {
        if let Some((id,)) = db.platform.one_as::<(String,)>(
            "SELECT id FROM support_tickets WHERE reporter_scope=? AND reporter_id=? AND request_id=?",
            params![scope, auth.user.id, request],
        )? {
            return detail(db, auth, &id);
        }
        let id = crypto::id("support")?;
        let time = now();
        db.platform.exec(
            "INSERT INTO support_tickets(id,request_id,reporter_id,reporter_scope,reporter_name,dsp_id,dsp_name,\
             kind,title,description,release,source_sha,created_at,updated_at,status,progress) \
             VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,'triaging','Routing your request')",
            params![id, request, auth.user.id, scope, auth.user.name(), auth.scope, dsp_name, kind,
                title, description, db.config.release, db.config.source.commit.as_deref().unwrap_or(""), time, time],
        )?;
        attachments::assign(db, auth, &id, &ids)?;
        jobs::enqueue(db, &id, "investigate", 0, None)?;
        super::notifications::owners(db, &id, "New AI Support ticket")?;
        detail(db, auth, &id)
    })
}

pub(crate) fn progress(db: &Store, id: &str, status: &str, message: &str) -> Result<()> {
    db.platform.exec(
        "UPDATE support_tickets SET status=?,progress=?,updated_at=? WHERE id=?",
        params![status, message, now(), id],
    )?;
    Ok(())
}

pub fn reopen(db: &Store, auth: &Auth, id: &str) -> Result<SupportDetail> {
    authorize(db, auth, id)?;
    db.platform.transaction(|| {
        let row = record(db, id)?;
        ensure(["resolved", "merged", "denied"].contains(&s(&row, "status")), "support_ticket_active", 409)?;
        db.platform.exec(
            "UPDATE support_tickets SET approved_version=NULL,pr_url=NULL,head_sha=NULL,merge_sha=NULL,\
             released_version=NULL,release=?,source_sha=? WHERE id=?",
            params![db.config.release, db.config.source.commit.as_deref().unwrap_or(""), id],
        )?;
        jobs::cancel(db, id)?;
        progress(db, id, "investigating", "Reopened · queued for a fresh investigation")?;
        let version = row["proposal_version"].as_u64().unwrap_or(0) as u32;
        jobs::enqueue(db, id, "investigate", version, None)?;
        super::notifications::owners(db, id, "AI Support ticket reopened")?;
        detail(db, auth, id)
    })
}
