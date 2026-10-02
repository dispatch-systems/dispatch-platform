-- Sign in with Dispatch: apps that connect to the MCP endpoint through OAuth. Each approval
-- becomes an agent_keys row of kind 'app' (added in code before this runs), which works
-- exactly like a key but has no static token: its hash is 'app:<id>' and its hint empty.
-- Codes and tokens are kept only as SHA-256 hashes. Times are RFC 3339, as on agent_keys.

-- oauth_clients: apps that registered themselves (RFC 7591), only ever public clients that
-- redirect to the owner's own computer. redirect_uris is a JSON array; last_used_at is the
-- last time the client was given tokens.
CREATE TABLE IF NOT EXISTS oauth_clients (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    redirect_uris TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT
);
-- oauth_requests: an authorization waiting for the platform owner's answer, for ten minutes.
-- verified is whether Dispatch knows the app (its published document) or only what it says
-- about itself (a registered client). scope is what is granted.
CREATE TABLE IF NOT EXISTS oauth_requests (
    id TEXT PRIMARY KEY,
    client_id TEXT NOT NULL,
    client_name TEXT NOT NULL,
    verified INTEGER NOT NULL CHECK(verified IN (0,1)),
    redirect_uri TEXT NOT NULL,
    state TEXT,
    code_challenge TEXT NOT NULL,
    resource TEXT NOT NULL,
    scope TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
-- oauth_codes: an approval, redeemable once within five minutes by the client that asked,
-- at the redirect it asked for, with the PKCE verifier. choices is what the owner granted.
-- key_id is the connected app the code made, so a replayed code can end it.
CREATE TABLE IF NOT EXISTS oauth_codes (
    hash TEXT PRIMARY KEY,
    client_id TEXT NOT NULL,
    client_name TEXT NOT NULL,
    client_verified INTEGER NOT NULL CHECK(client_verified IN (0,1)),
    redirect_uri TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    resource TEXT NOT NULL,
    choices TEXT NOT NULL,
    approved_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    key_id TEXT
);
-- oauth_tokens: a connected app's access tokens (an hour) and refresh tokens (30 days).
-- A refresh token's used_at is when it was first exchanged, replaced_at the last time.
CREATE TABLE IF NOT EXISTS oauth_tokens (
    hash TEXT PRIMARY KEY,
    key_id TEXT NOT NULL REFERENCES agent_keys(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('access','refresh')),
    resource TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    replaced_at TEXT
);
CREATE INDEX IF NOT EXISTS oauth_tokens_key ON oauth_tokens(key_id);
CREATE INDEX IF NOT EXISTS oauth_tokens_expiry ON oauth_tokens(expires_at);
