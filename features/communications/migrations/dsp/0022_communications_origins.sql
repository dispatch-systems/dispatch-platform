-- Additive derived metadata. No source traversal or application SQL function is used here.
-- No FKs from derived state: forgetting an empty conversation keeps its existing behavior.
CREATE TABLE IF NOT EXISTS communications_origin_state (
    transport TEXT NOT NULL CHECK (transport IN ('slack','text')),
    conversation_id TEXT NOT NULL,
    source_epoch INTEGER NOT NULL DEFAULT 0 CHECK (typeof(source_epoch)='integer' AND source_epoch>=0),
    active_generation TEXT,
    indexed_epoch INTEGER,
    zone TEXT,
    algorithm TEXT,
    PRIMARY KEY (transport,conversation_id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS communications_origin_control (
    singleton INTEGER PRIMARY KEY CHECK (singleton=1),
    slack_after TEXT NOT NULL DEFAULT '',
    text_after TEXT NOT NULL DEFAULT ''
);
INSERT INTO communications_origin_control(singleton) VALUES (1)
ON CONFLICT(singleton) DO NOTHING;
CREATE TABLE IF NOT EXISTS communications_origin_builds (
    transport TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    generation TEXT NOT NULL,
    source_epoch INTEGER NOT NULL,
    zone TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    previous TEXT,
    PRIMARY KEY (transport,conversation_id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS communications_slack_origins (
    conversation_id TEXT NOT NULL,generation TEXT NOT NULL,ts TEXT NOT NULL,message_id TEXT NOT NULL,
    day_start INTEGER NOT NULL CHECK (day_start IN (0,1)),
    sender_present INTEGER NOT NULL CHECK (sender_present IN (0,1)),
    PRIMARY KEY (conversation_id,generation,ts)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS communications_slack_origins_days ON communications_slack_origins(conversation_id,generation,ts) WHERE day_start=1;
CREATE INDEX IF NOT EXISTS communications_slack_origins_senders ON communications_slack_origins(conversation_id,generation,ts) WHERE sender_present=1;
CREATE TABLE IF NOT EXISTS communications_text_origins (
    conversation_id TEXT NOT NULL,generation TEXT NOT NULL,at INTEGER NOT NULL,id TEXT NOT NULL,
    day_start INTEGER NOT NULL CHECK (day_start IN (0,1)),
    sender_present INTEGER NOT NULL CHECK (sender_present IN (0,1)),
    PRIMARY KEY (conversation_id,generation,at,id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS communications_text_origins_days ON communications_text_origins(conversation_id,generation,at,id) WHERE day_start=1;
CREATE INDEX IF NOT EXISTS communications_text_origins_senders ON communications_text_origins(conversation_id,generation,at,id) WHERE sender_present=1;

-- Each relevant source-row transition bumps once per affected conversation. Insert/delete
-- and UPDATE all survive older binaries. No ownership flag can suppress these triggers.
CREATE TRIGGER IF NOT EXISTS communications_origins_slack_insert AFTER INSERT ON communications_messages BEGIN
    SELECT RAISE(ABORT,'communications_origins_epoch_exhausted')
    FROM communications_origin_state WHERE transport='slack' AND source_epoch=9223372036854775807
        AND (conversation_id=NEW.conversation_id);
    INSERT INTO communications_origin_state(transport,conversation_id,source_epoch)
    SELECT 'slack',NEW.conversation_id,1 WHERE NEW.conversation_id IS NOT NULL AND (1)
    ON CONFLICT(transport,conversation_id) DO UPDATE SET source_epoch=source_epoch+1;
END;
CREATE TRIGGER IF NOT EXISTS communications_origins_slack_delete AFTER DELETE ON communications_messages BEGIN
    SELECT RAISE(ABORT,'communications_origins_epoch_exhausted')
    FROM communications_origin_state WHERE transport='slack' AND source_epoch=9223372036854775807
        AND (conversation_id=OLD.conversation_id);
    INSERT INTO communications_origin_state(transport,conversation_id,source_epoch)
    SELECT 'slack',OLD.conversation_id,1 WHERE OLD.conversation_id IS NOT NULL AND (1)
    ON CONFLICT(transport,conversation_id) DO UPDATE SET source_epoch=source_epoch+1;
END;
CREATE TRIGGER IF NOT EXISTS communications_origins_slack_update AFTER UPDATE ON communications_messages
WHEN OLD.id IS NOT NEW.id
 OR OLD.conversation_id IS NOT NEW.conversation_id
 OR OLD.at IS NOT NEW.at
 OR OLD.direction IS NOT NEW.direction
 OR OLD.ts IS NOT NEW.ts
 OR OLD.sent_by IS NOT NEW.sent_by
 OR (CASE WHEN OLD.direction='out' THEN (SELECT id FROM communications_groups WHERE id=OLD.group_id) END) IS NOT (CASE WHEN NEW.direction='out' THEN (SELECT id FROM communications_groups WHERE id=NEW.group_id) END)
BEGIN
    SELECT RAISE(ABORT,'communications_origins_epoch_exhausted')
    FROM communications_origin_state WHERE transport='slack' AND source_epoch=9223372036854775807
        AND (conversation_id=OLD.conversation_id OR conversation_id=NEW.conversation_id);
    INSERT INTO communications_origin_state(transport,conversation_id,source_epoch)
    SELECT 'slack',OLD.conversation_id,1 WHERE OLD.conversation_id IS NOT NULL AND (1)
    ON CONFLICT(transport,conversation_id) DO UPDATE SET source_epoch=source_epoch+1;
    INSERT INTO communications_origin_state(transport,conversation_id,source_epoch)
    SELECT 'slack',NEW.conversation_id,1 WHERE NEW.conversation_id IS NOT NULL AND (NEW.conversation_id IS NOT OLD.conversation_id)
    ON CONFLICT(transport,conversation_id) DO UPDATE SET source_epoch=source_epoch+1;
END;
CREATE TRIGGER IF NOT EXISTS communications_origins_text_insert AFTER INSERT ON phones_texts BEGIN
    SELECT RAISE(ABORT,'communications_origins_epoch_exhausted')
    FROM communications_origin_state WHERE transport='text' AND source_epoch=9223372036854775807
        AND (conversation_id=NEW.conversation_id);
    INSERT INTO communications_origin_state(transport,conversation_id,source_epoch)
    SELECT 'text',NEW.conversation_id,1 WHERE NEW.conversation_id IS NOT NULL AND (1)
    ON CONFLICT(transport,conversation_id) DO UPDATE SET source_epoch=source_epoch+1;
END;
CREATE TRIGGER IF NOT EXISTS communications_origins_text_delete AFTER DELETE ON phones_texts BEGIN
    SELECT RAISE(ABORT,'communications_origins_epoch_exhausted')
    FROM communications_origin_state WHERE transport='text' AND source_epoch=9223372036854775807
        AND (conversation_id=OLD.conversation_id);
    INSERT INTO communications_origin_state(transport,conversation_id,source_epoch)
    SELECT 'text',OLD.conversation_id,1 WHERE OLD.conversation_id IS NOT NULL AND (1)
    ON CONFLICT(transport,conversation_id) DO UPDATE SET source_epoch=source_epoch+1;
END;
CREATE TRIGGER IF NOT EXISTS communications_origins_text_update AFTER UPDATE ON phones_texts
WHEN OLD.id IS NOT NEW.id
 OR OLD.conversation_id IS NOT NEW.conversation_id
 OR OLD.at IS NOT NEW.at
 OR OLD.direction IS NOT NEW.direction
 OR OLD.number IS NOT NEW.number
 OR (CASE WHEN OLD.direction='out' THEN OLD.sent_by END) IS NOT (CASE WHEN NEW.direction='out' THEN NEW.sent_by END)
 OR (CASE WHEN OLD.direction='out' AND OLD.sent_by IS NULL THEN OLD.device_id END) IS NOT (CASE WHEN NEW.direction='out' AND NEW.sent_by IS NULL THEN NEW.device_id END)
BEGIN
    SELECT RAISE(ABORT,'communications_origins_epoch_exhausted')
    FROM communications_origin_state WHERE transport='text' AND source_epoch=9223372036854775807
        AND (conversation_id=OLD.conversation_id OR conversation_id=NEW.conversation_id);
    INSERT INTO communications_origin_state(transport,conversation_id,source_epoch)
    SELECT 'text',OLD.conversation_id,1 WHERE OLD.conversation_id IS NOT NULL AND (1)
    ON CONFLICT(transport,conversation_id) DO UPDATE SET source_epoch=source_epoch+1;
    INSERT INTO communications_origin_state(transport,conversation_id,source_epoch)
    SELECT 'text',NEW.conversation_id,1 WHERE NEW.conversation_id IS NOT NULL AND (NEW.conversation_id IS NOT OLD.conversation_id)
    ON CONFLICT(transport,conversation_id) DO UPDATE SET source_epoch=source_epoch+1;
END;
CREATE TRIGGER IF NOT EXISTS communications_origins_group_insert AFTER INSERT ON communications_groups
BEGIN
    SELECT RAISE(ABORT,'communications_origins_epoch_exhausted')
    WHERE EXISTS (SELECT 1 FROM communications_origin_state s WHERE s.transport='slack'
        AND s.source_epoch=9223372036854775807 AND s.conversation_id IN (
            SELECT m.conversation_id FROM communications_messages m
            WHERE (m.group_id=NEW.id) AND m.direction='out'));
    INSERT INTO communications_origin_state(transport,conversation_id,source_epoch)
    SELECT DISTINCT 'slack',conversation_id,1 FROM communications_messages WHERE group_id=NEW.id AND direction='out'
    ON CONFLICT(transport,conversation_id) DO UPDATE SET source_epoch=source_epoch+1;
END;
CREATE TRIGGER IF NOT EXISTS communications_origins_group_delete AFTER DELETE ON communications_groups
BEGIN
    SELECT RAISE(ABORT,'communications_origins_epoch_exhausted')
    WHERE EXISTS (SELECT 1 FROM communications_origin_state s WHERE s.transport='slack'
        AND s.source_epoch=9223372036854775807 AND s.conversation_id IN (
            SELECT m.conversation_id FROM communications_messages m
            WHERE (m.group_id=OLD.id) AND m.direction='out'));
    INSERT INTO communications_origin_state(transport,conversation_id,source_epoch)
    SELECT DISTINCT 'slack',conversation_id,1 FROM communications_messages WHERE group_id=OLD.id AND direction='out'
    ON CONFLICT(transport,conversation_id) DO UPDATE SET source_epoch=source_epoch+1;
END;
CREATE TRIGGER IF NOT EXISTS communications_origins_group_update AFTER UPDATE OF id ON communications_groups
WHEN OLD.id IS NOT NEW.id
BEGIN
    SELECT RAISE(ABORT,'communications_origins_epoch_exhausted')
    WHERE EXISTS (SELECT 1 FROM communications_origin_state s WHERE s.transport='slack'
        AND s.source_epoch=9223372036854775807 AND s.conversation_id IN (
            SELECT m.conversation_id FROM communications_messages m
            WHERE (m.group_id=OLD.id OR m.group_id=NEW.id) AND m.direction='out'));
    INSERT INTO communications_origin_state(transport,conversation_id,source_epoch)
    SELECT DISTINCT 'slack',conversation_id,1 FROM communications_messages WHERE group_id=OLD.id AND direction='out'
    UNION
    SELECT DISTINCT 'slack',conversation_id,1 FROM communications_messages WHERE group_id=NEW.id AND direction='out'
    ON CONFLICT(transport,conversation_id) DO UPDATE SET source_epoch=source_epoch+1;
END;
