-- An answer on its way to Slack, noted before it's posted so a post that went is kept once and
-- never posted twice. Kept, as the rest, until the DSP is deleted. Shipped migrations never
-- change: add the next one instead.

-- An answer someone at the DSP wrote in a conversation, by their account (`sent_by`), under the
-- name it goes out under (`sender_name`), with its photos as kept and as Slack has each
-- (`photos`, JSON pairs), from when it may have been posted (`posting_since`). Its post carries
-- its ID as Slack's message metadata. It's cleared once its post is kept, or found not to be in
-- Slack.
CREATE TABLE IF NOT EXISTS communications_replies (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES communications_conversations (id),
    text TEXT NOT NULL,
    sent_by TEXT NOT NULL,
    sender_name TEXT NOT NULL,
    photos TEXT NOT NULL,
    posting_since INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS communications_replies_conversations
    ON communications_replies (conversation_id);
CREATE INDEX IF NOT EXISTS communications_replies_since
    ON communications_replies (posting_since);
