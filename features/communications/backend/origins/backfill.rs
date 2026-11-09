//! Resumable metadata-only builds. Each chunk and its cursor commit together. Audited suffix
//! placement preserves progress; other topology changes may still starve a candidate. It stays
//! explicitly unready and never falls back to an HTTP scan.
use super::*;
use crate::backend::texts;
use dispatch_core::{
    State,
    manifest::{Maintenance, Upkeep},
};
use std::{sync::Arc, time::Duration};

const CHUNK: usize = 256;
const CONVERSATIONS: i64 = 8;
pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60),
    run: pass,
};
pub(super) struct Build {
    pub(super) generation: String,
    pub(super) epoch: i64,
    pub(super) zone: String,
    pub(super) algorithm: String,
    pub(super) previous: Option<String>,
}
impl FromRow for Build {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            generation: row.get("generation")?,
            epoch: row.get("source_epoch")?,
            zone: row.get("zone")?,
            algorithm: row.get("algorithm")?,
            previous: row.get("previous")?,
        })
    }
}
/// Delete at most one chunk from an unreferenced generation. Separate generation ranges
/// skip protected generations by index seek, without walking their boundary rows.
fn prune(
    db: &Db,
    transport: Transport,
    conversation: &str,
    active: Option<&str>,
    candidate: Option<&str>,
) -> Result<bool> {
    let mut protected: Vec<&str> = active.into_iter().chain(candidate).collect();
    protected.sort_unstable();
    protected.dedup();
    let mut obsolete = None;
    for edge in 0..=protected.len() {
        let mut sql = format!(
            "SELECT generation FROM {} WHERE conversation_id=?1",
            transport.boundaries()
        );
        let mut values = vec![conversation];
        if let Some(lower) = edge.checked_sub(1).map(|i| protected[i]) {
            values.push(lower);
            sql.push_str(&format!(" AND generation>?{}", values.len()));
        }
        if let Some(upper) = protected.get(edge) {
            values.push(upper);
            sql.push_str(&format!(" AND generation<?{}", values.len()));
        }
        sql.push_str(" ORDER BY generation LIMIT 1");
        let found: Option<(String,)> = db.one_as(&sql, rusqlite::params_from_iter(values))?;
        if let Some((generation,)) = found {
            obsolete = Some(generation);
            break;
        }
    }
    let Some(generation) = obsolete else {
        return Ok(false);
    };
    let keys = match transport {
        Transport::Slack => "conversation_id,generation,ts",
        Transport::Text => "conversation_id,generation,at,id",
    };
    db.exec(
        &format!(
            "DELETE FROM {} WHERE ({keys}) IN (SELECT {keys} FROM {} m \
        WHERE conversation_id=?1 AND generation=?2 ORDER BY {} LIMIT ?3)",
            transport.boundaries(),
            transport.boundaries(),
            transport.order(false)
        ),
        params![conversation, generation, CHUNK as i64],
    )?;
    Ok(true)
}

/// A production upkeep unit and its resumable public-to-owner entry point. Returns true only
/// after atomic publication (or if already current). Never called by the window route.
pub(crate) fn chunk(db: &Db, transport: Transport, conversation: &str, zone: Tz) -> Result<bool> {
    db.transaction(|| {
        lock(db)?;
        ensure_state(db,transport,conversation)?;
        let exists:Option<(String,)>=db.one_as(&format!("SELECT id FROM {} WHERE id=?",transport.conversations()),[conversation])?;
        if exists.is_none() {
            // Conversation removal can leave derived rows behind. Discovery visits these
            // owners too, draining bounded chunks before forgetting their state.
            db.exec("UPDATE communications_origin_state SET active_generation=NULL,indexed_epoch=NULL \
                WHERE transport=? AND conversation_id=?",params![transport.name(),conversation])?;
            db.exec("DELETE FROM communications_origin_builds WHERE transport=? AND conversation_id=?",params![transport.name(),conversation])?;
            if !prune(db,transport,conversation,None,None)? {
                db.exec("DELETE FROM communications_origin_state WHERE transport=? AND conversation_id=?",params![transport.name(),conversation])?;
            }
            return Ok(true);
        }
        let index=state(db,transport,conversation)?.ok_or_else(invariant)?;
        if index.ready(zone.name()) {
            prune(db,transport,conversation,index.generation.as_deref(),None)?;
            db.exec("DELETE FROM communications_origin_builds WHERE transport=? AND conversation_id=?",params![transport.name(),conversation])?;
            return Ok(true);
        }
        let build:Option<Build>=db.one_as("SELECT generation,source_epoch,zone,algorithm,previous \
            FROM communications_origin_builds WHERE transport=? AND conversation_id=?",
            params![transport.name(),conversation])?;
        let mut build=match build.filter(|build| build.epoch==index.epoch && build.zone==zone.name()
            && build.algorithm==algorithm()) {
            Some(build)=>build,
            None=>Build { generation:crypto::id("origin")?,epoch:index.epoch,zone:zone.name().to_owned(),
                algorithm:algorithm(),previous:None },
        };
        prune(db,transport,conversation,index.generation.as_deref(),Some(&build.generation))?;
        let previous:Option<Topology>=build.previous.as_deref().map(serde_json::from_str).transpose()?;
        let prefix=format!("SELECT {} FROM {} m {} WHERE m.conversation_id=?1",transport.columns(),transport.source(),transport.join());
        let rows:Vec<Topology>=if let Some(previous)=&previous {
            db.query_as(&format!("{prefix} AND {} AND ?2 IS NOT NULL ORDER BY {} LIMIT ?5",
                transport.bound("m",">",2,3,4),transport.order(false)),
                params![conversation,previous.ts,previous.at,previous.id,CHUNK as i64])?
        } else {
            db.query_as(&format!("{prefix} ORDER BY {} LIMIT ?2",transport.order(false)),params![conversation,CHUNK as i64])?
        };
        let mut previous=previous;
        for row in &rows {
            boundary(db,transport,conversation,&build.generation,zone,previous.as_ref(),row)?;
            previous=Some(row.clone());
        }
        build.previous=previous.as_ref().map(serde_json::to_string).transpose()?;
        db.exec("INSERT INTO communications_origin_builds \
            (transport,conversation_id,generation,source_epoch,zone,algorithm,previous) VALUES (?,?,?,?,?,?,?) \
            ON CONFLICT(transport,conversation_id) DO UPDATE SET generation=excluded.generation, \
            source_epoch=excluded.source_epoch,zone=excluded.zone,algorithm=excluded.algorithm,previous=excluded.previous",
            params![transport.name(),conversation,build.generation,build.epoch,build.zone,build.algorithm,build.previous])?;
        if rows.len()==CHUNK { return Ok(false); }
        // EOF was observed under this write lock, not inferred from cursor age or row count.
        let published=db.exec("UPDATE communications_origin_state SET active_generation=?,indexed_epoch=?, \
            zone=?,algorithm=? WHERE transport=? AND conversation_id=? AND source_epoch=?",
            params![build.generation,build.epoch,build.zone,build.algorithm,transport.name(),conversation,build.epoch])?;
        ensure(published==1,"communications_origins_owner",500)?;
        Ok(true)
    })
}
/// Keyset discovery is bounded in conversation count. Source metadata work is at most one
/// CHUNK for each discovered conversation per pass, plus at most CHUNK obsolete boundaries.
/// Cursors survive restart; source history is never reclaimed.
fn candidates(db: &Db, transport: Transport) -> Result<Vec<String>> {
    db.transaction(|| {
        lock(db)?;
        let column = if transport == Transport::Slack {
            "slack_after"
        } else {
            "text_after"
        };
        let table = transport.conversations();
        let (after,): (String,) = db
            .one_as(
                &format!("SELECT {column} FROM communications_origin_control WHERE singleton=1"),
                [],
            )?
            .ok_or_else(invariant)?;
        let rows: Vec<(String,)> = db.query_as(
            &format!("SELECT id FROM {table} WHERE id>? ORDER BY id LIMIT ?"),
            params![after, CONVERSATIONS],
        )?;
        let states: Vec<(String,)> = db.query_as(
            "SELECT conversation_id FROM communications_origin_state \
            WHERE transport=? AND conversation_id>? ORDER BY conversation_id LIMIT ?",
            params![transport.name(), after, CONVERSATIONS],
        )?;
        // Merge two bounded indexed pages instead of materializing a UNION of all owners.
        let rows: Vec<(String,)> = rows
            .into_iter()
            .chain(states)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .take(CONVERSATIONS as usize)
            .collect();
        let next = rows.last().map(|row| row.0.as_str()).unwrap_or("");
        db.exec(
            &format!("UPDATE communications_origin_control SET {column}=? WHERE singleton=1"),
            [next],
        )?;
        Ok(rows.into_iter().map(|row| row.0).collect())
    })
}
fn pass(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if !due {
            return;
        }
        let dsps = state
            .read(|db| {
                let mut on = Vec::new();
                for dsp in db.served_ids()? {
                    if db.feature_enabled(&dsp, crate::FEATURE.switch.id)? {
                        on.push(dsp);
                    }
                }
                Ok(on)
            })
            .await;
        let dsps = match dsps {
            Ok(dsps) => dsps,
            Err(error) => {
                failed(None, &error);
                return;
            }
        };
        for dsp in dsps {
            for transport in [Transport::Slack, Transport::Text] {
                let asked = dsp.clone();
                let work = state
                    .run_bookkeeping(move |db| {
                        db.ensure_dsp_active(&asked)?;
                        if !db.feature_enabled(&asked, crate::FEATURE.switch.id)? {
                            return Ok(Vec::new());
                        }
                        let connection = db.dsp(&asked)?;
                        candidates(&connection, transport)
                    })
                    .await;
                let work = match work {
                    Ok(work) => work,
                    Err(error) => {
                        failed(Some(&dsp), &error);
                        continue;
                    }
                };
                for conversation in work {
                    let asked = dsp.clone();
                    let result = state
                        .run_bookkeeping(move |db| {
                            db.ensure_dsp_active(&asked)?;
                            if !db.feature_enabled(&asked, crate::FEATURE.switch.id)? {
                                return Ok(false);
                            }
                            let zone = texts::zone(db, &asked)?;
                            let connection = db.dsp(&asked)?;
                            chunk(&connection, transport, &conversation, zone)
                        })
                        .await;
                    if let Err(error) = result {
                        failed(Some(&dsp), &error);
                    }
                }
            }
        }
    })
}
fn failed(dsp: Option<&str>, error: &Error) {
    observability::event(
        "error",
        "communications_origins_backfill_failed",
        json!({"dspId":dsp,"error":error.code}),
    );
}
