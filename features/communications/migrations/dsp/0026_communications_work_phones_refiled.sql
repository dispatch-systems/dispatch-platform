-- Each day a text was filed under a work phone nobody had, noted to be filed again, as a number
-- someone in Driver Match has is filed as theirs since, never as a work phone nobody has: the upkeep
-- moves each such text to that number's own conversation. A text's day is the DSP's, which this
-- can't read, so the days either side of its UTC day are noted too; a day filed again that moves
-- nothing changes nothing.
INSERT OR IGNORE INTO communications_refiling (day, noted_at)
SELECT DISTINCT date(t.at / 1000 + shift.days * 86400, 'unixepoch'),
       CAST(strftime('%s', 'now') AS INTEGER) * 1000
FROM phones_texts t
JOIN communications_text_conversations c ON c.id = t.conversation_id
CROSS JOIN (SELECT -1 AS days UNION ALL SELECT 0 UNION ALL SELECT 1) shift
WHERE c.kind = 'work' AND c.driver_id IS NULL;
