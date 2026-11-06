//! Historical Slack storage migrations; registration stays in `feature.rs`.
use dispatch_core::{
    Result,
    db::{Db, migrations::add_column},
};

/// Communications' migration 13: how far within its millisecond each conversation's latest Done
/// covered (`done_ts`, the `ts` of the latest message it covered), none for one before, which
/// covered all of it.
pub(crate) fn add_done_places(db: &Db) -> Result<()> {
    add_column(db, "communications_conversations", "done_ts", "TEXT")
}

/// Communications' migration 3: what ties conversations, messages and photos to groups. A
/// conversation's group is the one it's listed under (`group_id`); a message's, the group it
/// went to or answers; a photo's, the group message it was sent with (`group_message_id`).
pub(crate) fn add_groups(db: &Db) -> Result<()> {
    add_column(db, "communications_conversations", "group_id", "TEXT")?;
    add_column(db, "communications_messages", "group_id", "TEXT")?;
    add_column(db, "communications_files", "group_message_id", "TEXT")?;
    db.exec(
        "CREATE INDEX IF NOT EXISTS communications_conversations_groups \
         ON communications_conversations (group_id, state)",
        [],
    )?;
    db.exec(
        "CREATE INDEX IF NOT EXISTS communications_messages_groups \
         ON communications_messages (group_id, at)",
        [],
    )?;
    db.exec(
        "CREATE INDEX IF NOT EXISTS communications_files_group_messages \
         ON communications_files (group_message_id)",
        [],
    )?;
    Ok(())
}
