//! Backing signed copies up where the DSP keeps its files (the `files` contract), while the DSP
//! chose to: each signed writeup's photo, those signed before it chose to as well, goes in a
//! folder for its category, inside one for its person, inside one for them all. Every minute
//! the photos not yet saved are tried, so marking a writeup signed never waits on it; one whose
//! try fails waits longer each time, and is tried for as long as it isn't saved. A folder is
//! known by its ID, so one renamed or moved among the DSP's files still takes its photos, and
//! one no longer there is made again. The copy saved is never changed or taken back: Conduct
//! keeps its own.
use super::{DOMAIN, copies, settings};
use crate::api::types::{ConductBackup, ConductTier};
use dispatch_core::{
    Result, State,
    db::{FromRow, Row, Store, now},
    ensure,
    foundation::observability,
    manifest::{Maintenance, Upkeep, people},
    server::http::upload::Upload,
};
use dispatch_documents_contract::{self as files, Author, Files, Incoming, NewFolder};
use futures_util::stream;
use rusqlite::params;
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60),
    run: pass,
};

/// How a failure is reported.
const FAILED: &str = "conduct.backup_failed";
/// How many photos are tried in one pass for a DSP, at most.
const BATCH: usize = 20;
/// How long a photo waits after each failed try, in milliseconds: the last, after every try
/// from then on.
const RETRIES_MS: &[i64] = &[
    60 * 1000,
    5 * 60 * 1000,
    15 * 60 * 1000,
    60 * 60 * 1000,
    6 * 60 * 60 * 1000,
];
/// Where a photo came from, as where it's saved says.
const FROM: &str = "Conduct";
/// The folder that holds everyone's.
const EVERYONE: &str = "Writeups";
/// What the activity log calls a person's folder: it keeps no one's disciplinary record, and
/// the folder's name, with where it is, says they have one.
const THEIRS: &str = "a driver's folder";
/// How a try ends once the DSP no longer backs its copies up: nothing more is sent, and the
/// photo waits as it was.
const STOPPED: &str = "conduct_backup_stopped";

fn pass(state: Arc<State>, _: bool) -> Upkeep {
    Box::pin(async move {
        let dsps = match state.read(|db| db.served_ids()).await {
            Ok(dsps) => dsps,
            Err(error) => return failed(None, &error.code),
        };
        for dsp in dsps {
            if let Err(error) = back_up(&state, &dsp, now()).await {
                failed(Some(&dsp), &error.code);
            }
        }
    })
}
/// Reports a failure by its code, with the DSP it was working for, if it was working for one.
fn failed(dsp: Option<&str>, code: &str) {
    observability::event("error", FAILED, json!({"dspId": dsp, "error": code}));
}

/// A signed writeup's photo to back up: whose it is, as the directory knows them now, and what
/// names its folders and its file.
struct Photo {
    writeup: String,
    person: String,
    person_name: String,
    category: String,
    category_name: String,
    tier: ConductTier,
    written_on: String,
    signed_by: String,
    attempts: i64,
}
impl FromRow for Photo {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            writeup: row.get("id")?,
            person: row.get("person")?,
            person_name: row.get("person_name")?,
            category: row.get("category_id")?,
            category_name: row.get("category_name")?,
            tier: row.get("tier")?,
            written_on: row.get("written_on")?,
            signed_by: row.get("signed_by_name")?,
            attempts: row.get("attempts")?,
        })
    }
}

/// The signed writeups with a photo that isn't saved yet, each with its category.
const UNSAVED: &str = "FROM conduct_writeups w JOIN conduct_copies k ON k.writeup_id=w.id \
     JOIN conduct_categories c ON c.id=w.category_id \
     LEFT JOIN conduct_backups b ON b.writeup_id=w.id \
     WHERE w.signed_on IS NOT NULL AND b.saved_id IS NULL";

/// Whether Conduct backs the DSP's signed copies up now: it serves the DSP, with Conduct on
/// there and the backup chosen.
fn chosen(db: &Store, dsp: &str) -> Result<bool> {
    Ok(db.serves_dsp(dsp)? && db.feature_enabled(dsp, "conduct")? && settings::backs_up(db, dsp)?)
}

/// Refuses, as `STOPPED`, once Conduct no longer backs the DSP's signed copies up: checked in
/// the read before each thing sent where the DSP keeps its files.
fn still_chosen(db: &Store, dsp: &str) -> Result<()> {
    ensure(chosen(db, dsp)?, STOPPED, 409)
}

/// The DSP's photos due a try by `at`, the longest waiting first, `most` at most, each person
/// as the directory knows them now.
fn due(store: &Store, state: &State, dsp: &str, at: i64, most: usize) -> Result<Vec<Photo>> {
    let mut due: Vec<Photo> = store.dsp(dsp)?.query_as(
        &format!(
            "SELECT w.id,w.person,w.person_name,w.category_id,c.name category_name,w.tier,\
             w.written_on,w.signed_by_name,COALESCE(b.attempts,0) attempts {UNSAVED} \
             AND COALESCE(b.due_at,0)<=? ORDER BY COALESCE(b.due_at,0),w.signed_at,w.id LIMIT ?"
        ),
        params![at, most as i64],
    )?;
    let everyone = people::everyone(store, state, dsp)?;
    for photo in &mut due {
        if let Some(person) = everyone.person(&photo.person) {
            photo.person.clone_from(&person.code);
            photo.person_name.clone_from(&person.name);
        }
    }
    Ok(due)
}

/// Saves the DSP's photos due by `at`, in milliseconds, while it backs them up and what keeps
/// its files can: once it no longer does, as the backup or Conduct switched off, nothing more is
/// sent, in a pass under way too. After a failure the rest wait for the next pass, as whatever
/// failed one fails the others.
pub(crate) async fn back_up(state: &Arc<State>, dsp: &str, at: i64) -> Result<()> {
    let (of, with) = (dsp.to_owned(), state.clone());
    let found = state
        .read(move |db| {
            if !chosen(db, &of)? {
                return Ok(None);
            }
            let Some(files) = files::for_dsp(db, &of)? else {
                return Ok(None);
            };
            Ok(Some((files, due(db, &with, &of, at, BATCH)?)))
        })
        .await?;
    let Some((files, due)) = found else {
        return Ok(());
    };
    if due.is_empty() || files.unready(state, dsp).await?.is_some() {
        return Ok(());
    }
    for photo in due {
        let outcome = match save(state, files, dsp, &photo).await {
            Err(error) if error.code == STOPPED => return Ok(()),
            outcome => outcome.map_err(|error| error.code),
        };
        let stopped = outcome.is_err();
        if let Err(code) = &outcome {
            failed(Some(dsp), code);
        }
        let of = dsp.to_owned();
        state
            .run_serving(dsp, DOMAIN, move |db| finish(db, &of, &photo, &outcome, at))
            .await?;
        if stopped {
            break;
        }
    }
    Ok(())
}

/// Saves `photo` in its category's folder, inside its person's: its ID where it's saved.
async fn save(
    state: &Arc<State>,
    files: &'static dyn Files,
    dsp: &str,
    photo: &Photo,
) -> Result<String> {
    let by = Author::Named(photo.signed_by.clone());
    let place = Place {
        state,
        files,
        dsp,
        by: &by,
    };
    let everyone = place.folder(("", ""), None, EVERYONE, None).await?;
    let theirs = place
        .folder(
            (&photo.person, ""),
            Some(&everyone),
            &photo.person_name,
            Some(THEIRS),
        )
        .await?;
    let into = place
        .folder(
            (&photo.person, &photo.category),
            Some(&theirs),
            &photo.category_name,
            None,
        )
        .await?;
    let (at, id) = (dsp.to_owned(), photo.writeup.clone());
    let (mime, bytes) = state
        .read(move |db| {
            still_chosen(db, &at)?;
            copies::read(db, &at, &id)
        })
        .await?;
    let name = format!(
        "{} {}.{}",
        photo.written_on,
        tier(photo.tier),
        extension(&mime)
    );
    let length = bytes.len() as u64;
    let sent = stream::once(std::future::ready(Ok(bytes.into())));
    let incoming = Incoming {
        name,
        mime,
        bytes: Upload::streamed(length, files.largest(), sent).await?,
        by,
        from: FROM,
    };
    Ok(files.save(state, dsp, &into, incoming).await?.id)
}

/// Where a DSP's photos go, and who a folder made there is from.
struct Place<'a> {
    state: &'a Arc<State>,
    files: &'static dyn Files,
    dsp: &'a str,
    by: &'a Author,
}
impl Place<'_> {
    /// The folder kept for the person and category `of`, while it's still among the DSP's
    /// files; else one made in `parent`, named `name`, which the activity log calls
    /// `logged_as` instead when given, and kept in its place.
    async fn folder(
        &self,
        of: (&str, &str),
        parent: Option<&str>,
        name: &str,
        logged_as: Option<&'static str>,
    ) -> Result<String> {
        let (person, category) = of;
        let (at, whose, which) = (self.dsp.to_owned(), person.to_owned(), category.to_owned());
        let kept = self
            .state
            .read(move |db| {
                still_chosen(db, &at)?;
                db.dsp(&at)?.one_as::<(String,)>(
                    "SELECT folder_id FROM conduct_backup_folders WHERE person=? AND category_id=?",
                    [&whose, &which],
                )
            })
            .await?;
        if let Some((id,)) = kept
            && self
                .files
                .folder(self.state, self.dsp, &id)
                .await?
                .is_some()
        {
            return Ok(id);
        }
        let folder = NewFolder {
            parent,
            name,
            by: self.by,
            logged_as,
        };
        let made = self
            .files
            .create_folder(self.state, self.dsp, folder)
            .await?;
        let (at, whose, which, id) = (
            self.dsp.to_owned(),
            person.to_owned(),
            category.to_owned(),
            made.id.clone(),
        );
        self.state
            .run_serving(self.dsp, DOMAIN, move |db| {
                db.dsp(&at)?
                    .exec(
                        "INSERT INTO conduct_backup_folders (person,category_id,folder_id) \
                         VALUES (?,?,?) ON CONFLICT(person,category_id) \
                         DO UPDATE SET folder_id=excluded.folder_id",
                        params![whose, which, id],
                    )
                    .map(drop)
            })
            .await?;
        Ok(made.id)
    }
}

/// Keeps what trying `photo` at `at` came to: where it was saved, or the code of why it wasn't,
/// with when it's due its next try.
fn finish(
    store: &Store,
    dsp: &str,
    photo: &Photo,
    outcome: &std::result::Result<String, String>,
    at: i64,
) -> Result<()> {
    let db = store.dsp(dsp)?;
    match outcome {
        Ok(saved) => db.exec(
            "INSERT INTO conduct_backups (writeup_id,saved_id,saved_at) VALUES (?,?,?) \
             ON CONFLICT(writeup_id) DO UPDATE SET saved_id=excluded.saved_id,\
             saved_at=excluded.saved_at,reason=NULL",
            params![photo.writeup, saved, at],
        ),
        Err(code) => {
            let wait = RETRIES_MS[(photo.attempts.max(0) as usize).min(RETRIES_MS.len() - 1)];
            db.exec(
                "INSERT INTO conduct_backups (writeup_id,attempts,due_at,reason) VALUES (?,1,?,?) \
                 ON CONFLICT(writeup_id) DO UPDATE SET attempts=attempts+1,\
                 due_at=excluded.due_at,reason=excluded.reason",
                params![photo.writeup, at + wait, code],
            )
        }
    }
    .map(drop)
}

/// Where the DSP's signed copies are backed up and how that's going, for Conduct's page of
/// Settings: nowhere while nothing keeps the DSP's files, and nothing failing or waiting while
/// the DSP doesn't back them up. It's failing while what keeps the DSP's files says it can't, or
/// the photo tried latest wasn't saved, and says why in the words of what keeps them, where it
/// has some.
pub(crate) async fn status(state: &Arc<State>, dsp: &str) -> Result<ConductBackup> {
    let at = dsp.to_owned();
    let found = state
        .read(move |db| {
            let Some(files) = files::for_dsp(db, &at)? else {
                return Ok(None);
            };
            if !chosen(db, &at)? {
                return Ok(Some((files, None)));
            }
            let db = db.dsp(&at)?;
            let waiting = db
                .one_as::<(i64,)>(&format!("SELECT COUNT(*) {UNSAVED}"), [])?
                .map_or(0, |(waiting,)| waiting);
            // Why the photo tried latest wasn't saved, by its error's code.
            let refused = db.one_as::<(String,)>(
                &format!(
                    "SELECT b.reason {UNSAVED} AND b.attempts>0 ORDER BY b.due_at DESC LIMIT 1"
                ),
                [],
            )?;
            Ok(Some((files, Some((waiting, refused.map(|(code,)| code))))))
        })
        .await?;
    let Some((files, counted)) = found else {
        return Ok(ConductBackup {
            place: None,
            failing: false,
            why: None,
            waiting: 0,
        });
    };
    let place = Some(files.place().to_owned());
    let Some((waiting, refused)) = counted else {
        return Ok(ConductBackup {
            place,
            failing: false,
            why: None,
            waiting: 0,
        });
    };
    let unready = files.unready(state, dsp).await?;
    Ok(ConductBackup {
        place,
        failing: unready.is_some() || refused.is_some(),
        why: unready.or_else(|| {
            let words = files.refusal(&refused?)?;
            Some(words.to_owned())
        }),
        waiting,
    })
}

/// A tier as a file's name says it.
fn tier(tier: ConductTier) -> &'static str {
    match tier {
        ConductTier::Verbal => "Verbal",
        ConductTier::Written => "Written",
        ConductTier::Final => "Final",
    }
}
/// The extension of a photo of `mime`, one of those a copy is kept as.
fn extension(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "jpg",
    }
}

#[cfg(test)]
#[path = "../tests/backend/backups.rs"]
mod tests;
