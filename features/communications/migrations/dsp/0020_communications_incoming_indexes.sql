-- Seek directly to the latest eligible incoming row, skipping outgoing or blocked suffixes.
CREATE INDEX IF NOT EXISTS phones_texts_incoming
    ON phones_texts(conversation_id,at,id) WHERE direction='in' AND blocked=0;
CREATE INDEX IF NOT EXISTS communications_messages_incoming
    ON communications_messages(conversation_id,ts) WHERE direction='in';
