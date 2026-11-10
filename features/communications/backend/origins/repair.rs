//! Write-side repair: a known source-row mutation keeps a ready generation's boundaries and its
//! epoch exact, inside the mutation's own transaction.
use super::{
    IndexState, algorithm, backfill, invariant,
    source::{Topology, Transport, by_id, flags, neighbor},
    state,
};
use chrono_tz::Tz;
use dispatch_core::{Result, db::Db, ensure};
use rusqlite::params;

/// A real SQLite write before any adjacency/state read. If a caller already established a
/// stale deferred snapshot, SQLite refuses its upgrade and the entire enclosing operation fails.
/// No stale neighbor can be reused. The caller's savepoint/transaction rolls this write back too.
pub(crate) fn lock(db: &Db) -> Result<()> {
    db.exec(
        "UPDATE communications_origin_control SET singleton=singleton WHERE singleton=1",
        [],
    )?;
    Ok(())
}
pub(crate) fn ensure_state(db: &Db, transport: Transport, conversation: &str) -> Result<()> {
    db.exec(
        "INSERT INTO communications_origin_state(transport,conversation_id) VALUES (?,?) \
        ON CONFLICT(transport,conversation_id) DO NOTHING",
        params![transport.name(), conversation],
    )?;
    Ok(())
}
fn remove_boundary(
    db: &Db,
    transport: Transport,
    conversation: &str,
    generation: &str,
    row: &Topology,
) -> Result<()> {
    let bound = transport.bound("m", "=", 3, 4, 5);
    db.exec(
        &format!(
            "DELETE FROM {} AS m WHERE conversation_id=?1 AND generation=?2 \
        AND {bound} AND ?3 IS NOT NULL AND ?5 IS NOT NULL",
            transport.boundaries()
        ),
        params![conversation, generation, row.ts, row.at, row.id],
    )?;
    Ok(())
}
pub(crate) fn boundary(
    db: &Db,
    transport: Transport,
    conversation: &str,
    generation: &str,
    zone: Tz,
    previous: Option<&Topology>,
    row: &Topology,
) -> Result<()> {
    let (start, day_start) = flags(previous, row, zone)?;
    if !start {
        return Ok(());
    }
    match transport {
        Transport::Slack => db.exec("INSERT INTO communications_slack_origins \
            (conversation_id,generation,ts,message_id,day_start,sender_present) VALUES (?,?,?,?,?,?) \
            ON CONFLICT(conversation_id,generation,ts) DO UPDATE SET message_id=excluded.message_id, \
            day_start=excluded.day_start,sender_present=excluded.sender_present",
            params![conversation,generation,row.ts,row.id,day_start,row.sender_present()])?,
        Transport::Text => db.exec("INSERT INTO communications_text_origins \
            (conversation_id,generation,at,id,day_start,sender_present) VALUES (?,?,?,?,?,?) \
            ON CONFLICT(conversation_id,generation,at,id) DO UPDATE SET \
            day_start=excluded.day_start,sender_present=excluded.sender_present",
            params![conversation,generation,row.at,row.id,day_start,row.sender_present()])?,
    };
    Ok(())
}

/// One known source-row mutation, inside its existing transaction/savepoint. `to` is the
/// destination of an insertion or move. No callbacks in triggers and no connection-local flag.
/// Epoch deltas must equal exactly this row's projected transition; an earlier unknown mutation
/// or an unexpected extra write cannot be acknowledged by the next ordinary send.
/// Audited single INSERT or NULL-owner filing only. The callback must perform exactly
/// that row's placement; refiles, retries and arbitrary mutations use `change`.
pub(crate) fn place<T>(
    db: &Db,
    transport: Transport,
    id: &str,
    to: Option<&str>,
    write: impl FnOnce() -> Result<T>,
) -> Result<T> {
    mutate(db, transport, id, to, true, write)
}

/// A general known mutation keeps unfinished candidates subject to normal epoch invalidation.
pub(crate) fn change<T>(
    db: &Db,
    transport: Transport,
    id: &str,
    to: Option<&str>,
    write: impl FnOnce() -> Result<T>,
) -> Result<T> {
    mutate(db, transport, id, to, false, write)
}

fn mutate<T>(
    db: &Db,
    transport: Transport,
    id: &str,
    to: Option<&str>,
    placement: bool,
    write: impl FnOnce() -> Result<T>,
) -> Result<T> {
    db.transaction(|| {
        lock(db)?;
        let before = by_id(db, transport, id)?;
        let mut conversations = std::collections::BTreeSet::new();
        if let Some(conversation) = before
            .as_ref()
            .and_then(|row| row.conversation_id.as_deref())
        {
            conversations.insert(conversation.to_owned());
        }
        if let Some(to) = to {
            conversations.insert(to.to_owned());
        }
        let mut entries = Vec::new();
        for conversation in conversations {
            ensure_state(db, transport, &conversation)?;
            let entry = state(db, transport, &conversation)?.ok_or_else(invariant)?;
            let successor = before
                .as_ref()
                .filter(|row| row.conversation_id.as_deref() == Some(conversation.as_str()))
                .map(|row| neighbor(db, transport, &conversation, row, true))
                .transpose()?
                .flatten();
            let candidate: Option<backfill::Build> = if placement {
                db.one_as(
                    "SELECT generation,source_epoch,zone,algorithm,previous FROM communications_origin_builds \
                    WHERE transport=? AND conversation_id=?",
                    params![transport.name(), conversation],
                )?
            } else {
                None
            };
            entries.push((conversation, entry, successor, candidate));
        }
        let result = write()?;
        let after = by_id(db, transport, id)?;
        if let Some(conversation) = after
            .as_ref()
            .and_then(|row| row.conversation_id.as_deref())
        {
            ensure(
                entries.iter().any(|(id, _, _, _)| id == conversation),
                "communications_origins_owner",
                500,
            )?;
        }
        for (conversation, entry, old_next, candidate) in entries {
            let changed = before != after;
            let from_here = before
                .as_ref()
                .is_some_and(|row| row.conversation_id.as_deref() == Some(conversation.as_str()));
            let to_here = after
                .as_ref()
                .is_some_and(|row| row.conversation_id.as_deref() == Some(conversation.as_str()));
            let expected = i64::from(changed && (from_here || to_here));
            let current = state(db, transport, &conversation)?.ok_or_else(invariant)?;
            if placement
                && before.as_ref().is_none_or(|row| row.conversation_id.is_none())
                && to_here
                && entry.epoch.checked_add(1) == Some(current.epoch)
                && let (Some(candidate), Some(row)) = (candidate, after.as_ref())
            {
                advance_candidate(db, transport, &conversation, &entry, &current, &candidate, row)?;
            }
            let Some(zone) = entry.zone.as_deref().filter(|zone| entry.ready(zone)) else {
                continue;
            };
            if entry.epoch.checked_add(expected) != Some(current.epoch)
                || current.generation != entry.generation
            {
                continue;
            }
            let zone: Tz = zone.parse().map_err(|_| invariant())?;
            let generation = entry.generation.as_deref().ok_or_else(invariant)?;
            let repaired = db.transaction(|| {
                if changed {
                    // Recompute against final adjacency. Each candidate is a compact key, never a
                    // materialized run. Sequential batch callers compose even for adjacent moves.
                    let mut keys = Vec::new();
                    if from_here {
                        keys.extend(before.iter().cloned());
                    }
                    keys.extend(old_next);
                    if to_here {
                        let row = after.as_ref().ok_or_else(invariant)?;
                        keys.push(row.clone());
                        keys.extend(neighbor(db, transport, &conversation, row, true)?);
                    }
                    let mut seen = std::collections::BTreeSet::new();
                    for key in keys {
                        if !seen.insert((key.ts.clone(), key.at, key.id.clone())) {
                            continue;
                        }
                        remove_boundary(db, transport, &conversation, generation, &key)?;
                        let row = by_id(db, transport, &key.id)?;
                        let Some(row) = row.filter(|row| {
                            row.conversation_id.as_deref() == Some(conversation.as_str())
                                && row.ts == key.ts
                                && row.at == key.at
                        }) else {
                            continue;
                        };
                        let previous = neighbor(db, transport, &conversation, &row, false)?;
                        boundary(
                            db,
                            transport,
                            &conversation,
                            generation,
                            zone,
                            previous.as_ref(),
                            &row,
                        )?;
                    }
                }
                db.exec(
                    "UPDATE communications_origin_state SET indexed_epoch=source_epoch WHERE \
                transport=? AND conversation_id=? AND source_epoch=? AND indexed_epoch=? \
                AND active_generation=?",
                    params![
                        transport.name(),
                        conversation,
                        current.epoch,
                        entry.indexed,
                        generation
                    ],
                )?;
                Ok(())
            });
            match repaired {
                Ok(()) => {}
                // The source operation has no calendar prerequisite. Roll back only derived
                // repair, retaining the source epoch so this generation stays explicitly dirty.
                Err(error) if error.code == "communications_origins_date" => {}
                Err(error) => return Err(error),
            }
        }
        Ok(result)
    })
}
fn advance_candidate(
    db: &Db,
    transport: Transport,
    conversation: &str,
    entry: &IndexState,
    current: &IndexState,
    candidate: &backfill::Build,
    row: &Topology,
) -> Result<()> {
    if candidate.epoch != entry.epoch
        || candidate.algorithm != algorithm()
        || candidate.zone.parse::<Tz>().is_err()
    {
        return Ok(());
    }
    let Some(previous) = candidate.previous.as_deref() else {
        return Ok(());
    };
    // A candidate is derived state. An unsupported cursor cannot become a source-write
    // prerequisite: keep its old epoch so the next chunk starts a fresh build.
    let Ok(previous_row) = serde_json::from_str::<Topology>(previous) else {
        return Ok(());
    };
    let bound = transport.bound("m", ">", 2, 3, 4);
    let suffix = db.count(
        &format!(
            "SELECT count(*) FROM {} m WHERE m.id=?1 AND {bound} \
            AND ?2 IS NOT NULL AND ?4 IS NOT NULL",
            transport.source()
        ),
        params![row.id, previous_row.ts, previous_row.at, previous_row.id],
    )? == 1;
    if !suffix {
        return Ok(());
    }
    let advanced = db.exec(
        "UPDATE communications_origin_builds SET source_epoch=?1 WHERE transport=?2 \
        AND conversation_id=?3 AND generation=?4 AND source_epoch=?5 AND zone=?6 \
        AND algorithm=?7 AND previous=?8",
        params![
            current.epoch,
            transport.name(),
            conversation,
            candidate.generation,
            candidate.epoch,
            candidate.zone,
            candidate.algorithm,
            previous
        ],
    )?;
    ensure(advanced == 1, "communications_origins_invariant", 500)?;
    Ok(())
}
