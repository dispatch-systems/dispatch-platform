//! Communications' migration 24: each line's texting its own.
use dispatch_core::{
    Result,
    db::{Db, migrations::add_column},
};

/// Communications' migration 24: each conversation by text is of the line whose phones its texts
/// went through (`line`, by its feature's name; none for Dispatch Phone's own), one a number's for
/// each line it texts; each change to a conversation is noted with its line, so a page waiting on
/// one line's hears only its own; and the names saved, the numbers blocked and the browsers told
/// of new messages of another line are kept apart from Dispatch Phone's, which stay as they were
/// (`0024_communications_lines.sql`). Everything kept before is Dispatch Phone's. And whether
/// each phone of another line has its texts filed, as its line said as it paired (`texting`, and
/// `line_texting` on the code that pairs it): none kept before is. Neither is read since every
/// phone's texts are filed, but each is still written, for a rollback to the release that read
/// them (`phones_new_pairing`). Each step is taken only if it wasn't.
///
/// The numbers' conversations are told apart by their line too: the index that kept one per kind
/// and number gives way to one keeping one per kind, number and line. Over the conversations with
/// no line, which are all the release before the lines writes, it holds exactly what the old one
/// did, `(kind, number)` once each, so that release's writes and reads stand as they did.
pub(crate) fn add_lines(db: &Db) -> Result<()> {
    add_column(db, "communications_text_conversations", "line", "TEXT")?;
    add_column(db, "communications_changes", "line", "TEXT")?;
    add_column(
        db,
        "phones_pairings",
        "line_texting",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column(
        db,
        "phones_devices",
        "texting",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    db.0.execute_batch(include_str!(
        "../../../migrations/dsp/0024_communications_lines.sql"
    ))?;
    db.0.execute_batch(
        "DROP INDEX IF EXISTS communications_text_conversations_numbers; \
         CREATE UNIQUE INDEX IF NOT EXISTS communications_text_conversations_lines \
         ON communications_text_conversations (kind, number, ifnull(line, '')) \
         WHERE driver_id IS NULL; \
         CREATE INDEX IF NOT EXISTS communications_changes_lines \
         ON communications_changes (line, revision);",
    )?;
    Ok(())
}
