//! Pending placement and overdue fallback, live-day transitions, queued-send expiry and
//! fetched-video release. The filing coordinator runs these during minute upkeep.
//! Text mutations below use `changing` so filing and post-commit notifications stay coupled.
use super::super::{
    live_day,
    storage::{DayStore, TextStore},
    zone,
};
use super::{
    UNFILED_MS, changing,
    placement::{sender_name, unread},
};
use crate::backend::{
    live,
    phone::storage::{FileStore as _, MOST_TEXTS, TextStore as _},
    storage::DOMAIN,
};
use dispatch_core::{Result, State, foundation::clock};
use std::{collections::HashMap, sync::Arc};

/// Files the DSP's texts waiting to be, oldest first, a page in each write, then wakes the pages
/// waiting on what changed: one that can't be placed yet waits for the next pass, after the rest.
/// Written only once one waits.
pub async fn unfiled(state: &Arc<State>, dsp: &str) -> Result<()> {
    let asked = dsp.to_owned();
    if state
        .read(move |db| db.unfiled_communications_texts(&asked, None, 1))
        .await?
        .is_empty()
    {
        return Ok(());
    }
    let mut after: Option<(i64, String)> = None;
    loop {
        let (at, from) = (dsp.to_owned(), after.take());
        let (last, more) = changing(state, dsp, move |db| {
            let from = from.as_ref().map(|(at, id)| (*at, id.as_str()));
            let page = db.unfiled_communications_texts(&at, from, MOST_TEXTS)?;
            let last = page.last().cloned();
            let ids = page.into_iter().map(|(_, id)| id).collect::<Vec<_>>();
            Ok(((last, ids.len() == MOST_TEXTS), ids))
        })
        .await?;
        if !more {
            return Ok(());
        }
        after = last;
    }
}

/// Places the DSP's texts that couldn't be placed for `UNFILED_MS` by `at` under their number,
/// as a day Routes can't read does (`unread`), the oldest page of them, noting each one's day to
/// be filed again, which moves it where it belongs once Routes can say: written only once one has
/// waited so long.
pub async fn overdue(state: &Arc<State>, dsp: &str, at: i64) -> Result<()> {
    let (asked, since) = (dsp.to_owned(), at - UNFILED_MS);
    if state
        .read(move |db| db.overdue_communications_texts(&asked, since, 1))
        .await?
        .is_empty()
    {
        return Ok(());
    }
    let at_dsp = dsp.to_owned();
    changing(state, dsp, move |db| {
        let texts = db.overdue_communications_texts(&at_dsp, since, MOST_TEXTS)?;
        // The day each is of, to be filed again, while the DSP's timezone can be read.
        let zone = zone(db, &at_dsp).ok();
        let mut senders = HashMap::new();
        for text in &texts {
            let sender = sender_name(db, &mut senders, text.sent_by.as_deref())?;
            let filed = unread(db, &at_dsp, &text.number);
            db.place_communications_text(&at_dsp, &text.id, &filed, sender.as_deref())?;
            if let Some(zone) = zone {
                let day = clock::day(zone, text.at).to_string();
                db.note_communications_refiling(&at_dsp, &day, at)?;
            }
        }
        Ok(((), texts.into_iter().map(|text| text.id).collect()))
    })
    .await
}

/// Keeps what changed of the DSP's live day at `at` (`live_day::watched`), as it began or ended;
/// once the two hours after it ended pass, wakes the pages waiting on the work conversations that
/// don't wait any more. Written only once something changed.
pub async fn watch_live_day(state: &Arc<State>, dsp: &str, at: i64) -> Result<()> {
    let asked = dsp.to_owned();
    if state
        .read(move |db| live_day::watched(db, &asked, at))
        .await?
        .is_none()
    {
        return Ok(());
    }
    let at_dsp = dsp.to_owned();
    let over = state
        .run_serving(dsp, DOMAIN, move |db| {
            let Some(watched) = live_day::watched(db, &at_dsp, at)? else {
                return Ok(false);
            };
            live_day::keep(db, &at_dsp, &watched)?;
            Ok(matches!(watched, live_day::Watched::Over { .. }))
        })
        .await?;
    if over {
        live::changed(state, dsp);
    }
    Ok(())
}

/// Fails the answers waiting past their expiry at `at` on a phone offline then, as
/// `phones_offline`, each filed as it fails, so its conversation waits again: written only once
/// one has expired.
pub async fn expire(state: &Arc<State>, dsp: &str, at: i64) -> Result<()> {
    let asked = dsp.to_owned();
    if state
        .read(move |db| db.phones_expired(&asked, at))
        .await?
        .is_empty()
    {
        return Ok(());
    }
    let at_dsp = dsp.to_owned();
    changing(state, dsp, move |db| {
        Ok(((), db.phones_expire(&at_dsp, at)?))
    })
    .await
}

/// Lets go of the bytes of the videos kept on their phone that came before `before`, each text
/// filed as its video goes back to its phone, so the pages showing it hear: written only once one
/// has.
pub async fn let_go_fetched(state: &Arc<State>, dsp: &str, before: i64) -> Result<()> {
    let asked = dsp.to_owned();
    if !state
        .read(move |db| db.phones_fetched_before(&asked, before))
        .await?
    {
        return Ok(());
    }
    let at = dsp.to_owned();
    changing(state, dsp, move |db| {
        Ok(((), db.phones_let_go_fetched(&at, before)?))
    })
    .await
}
