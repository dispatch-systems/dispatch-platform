-- Each member's own preferences in the DSP, as how they arranged its sidebar: a row for each
-- they saved, with the version of its value and the revision the save made. Not tied to
-- `users`, as a platform owner keeps theirs here too; each goes with the member's account.
CREATE TABLE IF NOT EXISTS member_preferences (
    user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    version INTEGER NOT NULL,
    revision INTEGER NOT NULL,
    value TEXT NOT NULL,
    saved_at TEXT NOT NULL,
    PRIMARY KEY (user_id, id)
) WITHOUT ROWID;

-- How each member arranged the sidebar, as a release before kept it among the settings, which
-- stay for that release to read after a rollback.
INSERT OR IGNORE INTO member_preferences(user_id, id, version, revision, value, saved_at)
SELECT substr(key, 9), 'shell.sidebar', 1, 1, value, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM settings
WHERE key > 'sidebar:' AND key < 'sidebar;' AND json_valid(value);
