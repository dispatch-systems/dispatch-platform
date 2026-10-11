CREATE TABLE support_tickets (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL,
    reporter_id TEXT NOT NULL,
    reporter_scope TEXT NOT NULL,
    reporter_name TEXT NOT NULL,
    dsp_id TEXT,
    dsp_name TEXT,
    kind TEXT NOT NULL CHECK(kind IN ('bug', 'feature')),
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    release TEXT NOT NULL,
    source_sha TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'investigating',
    progress TEXT NOT NULL DEFAULT 'Queued for browser investigation',
    proposal_version INTEGER NOT NULL DEFAULT 0,
    approved_version INTEGER,
    pr_url TEXT,
    head_sha TEXT,
    merge_sha TEXT,
    released_version TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(reporter_scope, reporter_id, request_id)
);
CREATE INDEX support_tickets_reporter ON support_tickets(reporter_scope,reporter_id,updated_at DESC);
CREATE INDEX support_tickets_status ON support_tickets(status,updated_at DESC);
CREATE TABLE support_messages (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    ticket_id TEXT NOT NULL REFERENCES support_tickets(id),
    author TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('reporter','owner','agent','system')),
    body TEXT NOT NULL,
    private INTEGER NOT NULL CHECK(private IN (0,1)),
    created_at INTEGER NOT NULL
);
CREATE INDEX support_messages_ticket ON support_messages(ticket_id,sequence);
CREATE TABLE support_attachments (
    id TEXT PRIMARY KEY,
    ticket_id TEXT REFERENCES support_tickets(id),
    uploader_id TEXT NOT NULL,
    uploader_scope TEXT NOT NULL,
    name TEXT NOT NULL,
    media_type TEXT NOT NULL,
    bytes BLOB NOT NULL,
    private INTEGER NOT NULL DEFAULT 0 CHECK(private IN (0,1)),
    created_at INTEGER NOT NULL
);
CREATE INDEX support_attachments_ticket ON support_attachments(ticket_id);
CREATE TABLE support_proposals (
    ticket_id TEXT NOT NULL REFERENCES support_tickets(id),
    version INTEGER NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(ticket_id,version)
);
CREATE TABLE support_approvals (
    ticket_id TEXT NOT NULL REFERENCES support_tickets(id),
    version INTEGER NOT NULL,
    owner_id TEXT NOT NULL,
    decision TEXT NOT NULL CHECK(decision IN ('approve','deny')),
    note TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(ticket_id,version),
    FOREIGN KEY(ticket_id,version) REFERENCES support_proposals(ticket_id,version)
);
CREATE TABLE support_jobs (
    id TEXT PRIMARY KEY,
    ticket_id TEXT NOT NULL REFERENCES support_tickets(id),
    kind TEXT NOT NULL CHECK(kind IN ('investigate','answer','implement','release')),
    version INTEGER NOT NULL,
    question_id TEXT REFERENCES support_messages(id),
    status TEXT NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','running','waiting','done','cancelled')),
    lease_hash TEXT,
    lease_until INTEGER,
    checkpoint TEXT NOT NULL DEFAULT '{}',
    attempts INTEGER NOT NULL DEFAULT 0,
    available_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX support_jobs_claim ON support_jobs(status,available_at,lease_until);
CREATE UNIQUE INDEX support_jobs_active ON support_jobs(ticket_id,kind,version)
    WHERE status IN ('queued','running','waiting') AND kind<>'answer';
CREATE UNIQUE INDEX support_jobs_question ON support_jobs(question_id) WHERE question_id IS NOT NULL;
CREATE TABLE support_evidence (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    job_id TEXT NOT NULL REFERENCES support_jobs(id),
    ticket_id TEXT NOT NULL REFERENCES support_tickets(id),
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX support_evidence_ticket ON support_evidence(ticket_id,sequence);
CREATE TABLE support_notifications (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    ticket_id TEXT NOT NULL REFERENCES support_tickets(id),
    user_id TEXT NOT NULL,
    user_scope TEXT NOT NULL,
    body TEXT NOT NULL,
    read_at INTEGER,
    created_at INTEGER NOT NULL
);
CREATE INDEX support_notifications_user ON support_notifications(user_scope,user_id,created_at DESC);
CREATE TABLE support_github_events (
    delivery_id TEXT PRIMARY KEY,
    ticket_id TEXT NOT NULL REFERENCES support_tickets(id),
    event TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
