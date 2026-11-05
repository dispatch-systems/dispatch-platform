-- Each browser a member is told of new messages in, as its push service subscribed it: where a
-- push to it goes (`endpoint`, which names the browser), the keys its pushes are encrypted for
-- (`p256dh`, `auth`), and the address it was subscribed at, the admin's or the DSP's own
-- (`site`), whose directory holds the member's account. One browser is one member's, the latest
-- to subscribe it. Kept until it's
-- removed, refused by its push service, or its member no longer may be told. Shipped migrations
-- never change: add the next one instead.
CREATE TABLE IF NOT EXISTS communications_push (
    endpoint TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    p256dh TEXT NOT NULL,
    auth TEXT NOT NULL,
    site TEXT NOT NULL CHECK (site IN ('admin', 'dsp')),
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS communications_push_users ON communications_push (user_id);
