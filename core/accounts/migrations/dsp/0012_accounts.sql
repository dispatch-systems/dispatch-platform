-- A DSP's own people, kept in its own database: their accounts and roles, their sign-ins and
-- what proves who they are, and the invitations that bring them in. The platform's database
-- keeps its owners' alone. The tables match the platform's, so one query reads either.
CREATE TABLE IF NOT EXISTS roles (
    id TEXT PRIMARY KEY,
    dsp_id TEXT NOT NULL,
    name TEXT NOT NULL COLLATE NOCASE,
    permissions TEXT NOT NULL DEFAULT '[]',
    system INTEGER NOT NULL DEFAULT 0 CHECK(system IN (0,1)),
    created_at TEXT NOT NULL,
    UNIQUE(dsp_id,name)
);
CREATE UNIQUE INDEX IF NOT EXISTS roles_owner ON roles(dsp_id) WHERE system=1;
-- Nobody here is a platform owner: their account is the platform's.
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE COLLATE NOCASE,
    first_name TEXT NOT NULL,
    last_name TEXT NOT NULL,
    password TEXT NOT NULL,
    platform_owner INTEGER NOT NULL DEFAULT 0 CHECK(platform_owner=0),
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','disabled')),
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS memberships (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    dsp_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('owner','manager','member')),
    role_id TEXT REFERENCES roles(id),
    UNIQUE(user_id,dsp_id)
);
CREATE INDEX IF NOT EXISTS memberships_role ON memberships(role_id);
CREATE INDEX IF NOT EXISTS memberships_dsp_role ON memberships(dsp_id,role,user_id);
CREATE TABLE IF NOT EXISTS sessions (
    hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    user_version INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS session_expiry ON sessions(expires_at);
CREATE TABLE IF NOT EXISTS session_metadata (
    session_hash TEXT PRIMARY KEY REFERENCES sessions(hash) ON DELETE CASCADE,
    device TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS session_security (
    session_hash TEXT PRIMARY KEY REFERENCES sessions(hash) ON DELETE CASCADE,
    verified_at INTEGER NOT NULL DEFAULT 0,
    password_verified_at INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS security_challenges (
    session_hash TEXT PRIMARY KEY REFERENCES sessions(hash) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    state TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS resets (
    hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    user_version INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    used_at INTEGER
);
CREATE INDEX IF NOT EXISTS resets_expiry ON resets(expires_at);
CREATE TABLE IF NOT EXISTS recovery_codes (
    hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS recovery_codes_user ON recovery_codes(user_id);
CREATE TABLE IF NOT EXISTS authenticator_apps (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    secret TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_counter INTEGER NOT NULL DEFAULT -1
);
CREATE TABLE IF NOT EXISTS account_passkeys (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    credential TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS account_passkeys_user ON account_passkeys(user_id);
-- An invitation may come from the platform owner, whose account is not here.
CREATE TABLE IF NOT EXISTS invitations (
    hash TEXT PRIMARY KEY,
    dsp_id TEXT NOT NULL,
    email TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('owner','manager','member')),
    expires_at INTEGER NOT NULL,
    created_by TEXT NOT NULL,
    used_at INTEGER,
    role_id TEXT
);
CREATE INDEX IF NOT EXISTS invitations_dsp_owner ON invitations(dsp_id,role,expires_at DESC) WHERE used_at IS NULL;
CREATE INDEX IF NOT EXISTS invitations_expiry ON invitations(expires_at);
CREATE INDEX IF NOT EXISTS invitations_role ON invitations(role_id) WHERE used_at IS NULL;
