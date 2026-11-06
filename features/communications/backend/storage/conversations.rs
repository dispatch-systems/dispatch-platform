//! Slack conversations in each DSP's database: lookup, opening, inbox lists and Done.
use super::settlement::{mark_done, settle};
use crate::backend::model::{ConversationState, SLACK_PREFIX};
use dispatch_core::{
    Error, Result,
    db::{Db, FromRow, Row, Store},
    foundation::crypto,
};
use rusqlite::params;

/// A conversation with a member of one of the DSP's workspaces, as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversation {
    pub id: String,
    pub team: String,
    /// The member, by their ID in the workspace.
    pub user: String,
    /// The direct message the app has with them.
    pub channel: String,
    /// Their name, as their Slack profile last gave it.
    pub name: String,
    pub state: ConversationState,
    /// Since when it waits: the first of the member's messages since it was last answered.
    pub waiting_since: Option<i64>,
    /// When its latest message was written; none before its first.
    pub last_at: Option<i64>,
    /// Up to when catching up has read its messages.
    pub caught_up_at: Option<i64>,
    /// Where catching up goes on from, with more since `caught_up_at` than a pass reads: up to
    /// when it will have read once it's back down there, and the oldest message it read, by its
    /// `ts`.
    pub catching_up: Option<(i64, String)>,
    /// The group it's listed under: the one the DSP last wrote to them in, while all they wait on
    /// answers a message to a group.
    pub group: Option<String>,
}
impl FromRow for Conversation {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            team: row.get("team_id")?,
            user: row.get("user_id")?,
            channel: row.get("channel_id")?,
            name: row.get("name")?,
            state: row.get("state")?,
            waiting_since: row.get("waiting_since")?,
            last_at: row.get("last_at")?,
            caught_up_at: row.get("caught_up_at")?,
            catching_up: match (row.get("catching_up_to")?, row.get("catching_up_before")?) {
                (Some(to), Some(before)) => Some((to, before)),
                _ => None,
            },
            group: row.get("group_id")?,
        })
    }
}

/// Slack conversation lookup, lifetime and inbox state.
pub trait ConversationStore {
    fn communications_conversation(&self, dsp: &str, id: &str) -> Result<Option<Conversation>>;
    /// The conversation with the workspace's member `user`, if there is one.
    fn communications_conversation_with(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
    ) -> Result<Option<Conversation>>;
    /// The conversation with the workspace's member `user`, in their direct message `channel`
    /// with the app, under the name `name`: made with no message if it's new.
    fn open_communications_conversation(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
        channel: &str,
        name: &str,
    ) -> Result<Conversation>;
    /// Marks the conversation done without an answer, as the member `by`, while it waits, covering
    /// what was written up to `seen`, the latest message the member was shown, when it was written
    /// and its `ts`, else up to now: whether it was waiting.
    fn mark_communications_done(
        &self,
        dsp: &str,
        conversation: &str,
        by: &str,
        seen: Option<(i64, &str)>,
    ) -> Result<bool>;
    /// The conversations waiting, the longest waiting first.
    fn waiting_communications(&self, dsp: &str) -> Result<Vec<Conversation>>;
    /// Every conversation with a message, the latest first.
    fn communications_with_messages(&self, dsp: &str) -> Result<Vec<Conversation>>;
    /// The conversations `ids` name, in no order.
    fn communications_conversations(&self, dsp: &str, ids: &[String]) -> Result<Vec<Conversation>>;
    /// The workspace's conversations.
    fn communications_in(&self, dsp: &str, team: &str) -> Result<Vec<Conversation>>;
    /// Clears what's kept of the workspace beside its messages: each conversation with none, but
    /// one with an answer noted on its way to Slack.
    fn forget_empty_communications(&self, dsp: &str, team: &str) -> Result<()>;
}

impl ConversationStore for Store {
    fn communications_conversation(&self, dsp: &str, id: &str) -> Result<Option<Conversation>> {
        self.dsp(dsp)?.one_as(
            "SELECT * FROM communications_conversations WHERE id=?",
            [id],
        )
    }
    fn communications_conversation_with(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
    ) -> Result<Option<Conversation>> {
        self.dsp(dsp)?.one_as(
            "SELECT * FROM communications_conversations WHERE team_id=? AND user_id=?",
            [team, user],
        )
    }
    fn open_communications_conversation(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
        channel: &str,
        name: &str,
    ) -> Result<Conversation> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let id = opened(&db, team, user, channel, name)?;
            db.one_as(
                "SELECT * FROM communications_conversations WHERE id=?",
                [&id],
            )?
            .ok_or_else(|| Error::new("communications_conversation_not_found", 404))
        })
    }
    fn mark_communications_done(
        &self,
        dsp: &str,
        conversation: &str,
        by: &str,
        seen: Option<(i64, &str)>,
    ) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let waiting = db.count(
                "SELECT count(*) FROM communications_conversations WHERE id=? AND state='waiting'",
                [conversation],
            )? > 0;
            if waiting {
                mark_done(&db, conversation, by, seen)?;
                settle(&db, conversation)?;
            }
            Ok(waiting)
        })
    }
    fn waiting_communications(&self, dsp: &str) -> Result<Vec<Conversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_conversations WHERE state='waiting' \
             ORDER BY waiting_since,id",
            [],
        )
    }
    fn communications_with_messages(&self, dsp: &str) -> Result<Vec<Conversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_conversations WHERE last_at IS NOT NULL \
             ORDER BY last_at DESC,id DESC",
            [],
        )
    }
    fn communications_conversations(&self, dsp: &str, ids: &[String]) -> Result<Vec<Conversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_conversations \
             WHERE id IN (SELECT value FROM json_each(?))",
            [serde_json::to_string(ids)?],
        )
    }
    fn communications_in(&self, dsp: &str, team: &str) -> Result<Vec<Conversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_conversations WHERE team_id=? ORDER BY id",
            [team],
        )
    }
    fn forget_empty_communications(&self, dsp: &str, team: &str) -> Result<()> {
        self.dsp(dsp)?.exec(
            "DELETE FROM communications_conversations WHERE team_id=? AND last_at IS NULL \
             AND id NOT IN (SELECT conversation_id FROM communications_replies)",
            [team],
        )?;
        Ok(())
    }
}

/// The conversation with the workspace's member `user`, made if it's new, with their direct
/// message `channel` and the name `name`: its ID.
pub(crate) fn opened(db: &Db, team: &str, user: &str, channel: &str, name: &str) -> Result<String> {
    db.exec(
        "INSERT INTO communications_conversations (id,team_id,user_id,channel_id,name) \
         VALUES (?,?,?,?,?) ON CONFLICT(team_id,user_id) DO UPDATE SET \
         channel_id=excluded.channel_id,name=excluded.name",
        params![crypto::id(SLACK_PREFIX)?, team, user, channel, name],
    )?;
    let (id,): (String,) = db
        .one_as(
            "SELECT id FROM communications_conversations WHERE team_id=? AND user_id=?",
            [team, user],
        )?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    Ok(id)
}
