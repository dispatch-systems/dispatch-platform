-- Previous outgoing render identity must seek past arbitrarily long incoming-only histories.
-- Additive: existing incoming indexes and shipped migrations remain intact.
CREATE INDEX IF NOT EXISTS communications_messages_outgoing
    ON communications_messages (conversation_id, ts) WHERE direction='out';
CREATE INDEX IF NOT EXISTS phones_texts_outgoing
    ON phones_texts (conversation_id, at, id) WHERE direction='out';
