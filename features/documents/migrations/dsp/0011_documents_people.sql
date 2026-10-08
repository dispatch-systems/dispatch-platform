-- What Documents keeps for a DSP. Shipped migrations never change: add the next one instead.

-- Each member Documents shares the main folder with: the Google account they linked, if they
-- did, the address the folder is shared with and Drive's ID for that share, and how sharing
-- went: shared, waiting for them to have a Google account, or refused by Google for another
-- reason. Members who leave go, and their share with them.
CREATE TABLE IF NOT EXISTS documents_people (
    user_id TEXT PRIMARY KEY,
    linked_email TEXT,
    shared_email TEXT,
    share_id TEXT,
    state TEXT NOT NULL CHECK (state IN ('shared', 'needs_account', 'refused')),
    refusal TEXT,
    emailed_at TEXT,
    updated_at TEXT NOT NULL
);
