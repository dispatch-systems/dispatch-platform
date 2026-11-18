//! Communications' migration 24: each line's texting its own.
use dispatch_core::{
    Result,
    db::{Db, migrations::add_column},
};

/// Communications' migration 24: each conversation by text is of the line whose phones its texts
/// went through (`line`, by its feature's name; none for Dispatch Phone's own), one a number's for
/// each line it texts; each change to a conversation is noted with its line, so a page waiting on
/// one line's hears only its own; and the names saved, the numbers blocked and the browsers told
/// of new messages are each kept by their line (`0024_communications_lines.sql`). Everything kept
/// before is Dispatch Phone's. Each step is taken only if it wasn't.
pub(crate) fn add_lines(db: &Db) -> Result<()> {
    add_column(db, "communications_text_conversations", "line", "TEXT")?;
    add_column(db, "communications_changes", "line", "TEXT")?;
    let lined = db.count(
        "SELECT count(*) FROM pragma_table_info('communications_contacts') WHERE name='line'",
        [],
    )? > 0;
    if !lined {
        db.0.execute_batch(include_str!(
            "../../../migrations/dsp/0024_communications_lines.sql"
        ))?;
    }
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
