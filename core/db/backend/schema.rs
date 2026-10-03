//! Core's own databases, and the migrations core's parts add to them. Collectors and
//! features declare theirs in their manifests. See `migrations` for the rules.
//! Each 0001 creates what a new database needs and changes nothing on a database
//! that older code created, which is how those were adopted.
use super::{
    Db,
    migrations::{Apply::Code, Apply::Sql, Kind, Migration, Migrations, add_column},
};
use crate::Result;

/// The platform's accounts, the environment's jobs and each DSP's own database.
pub const DATABASES: &[Kind] = &[Kind::PLATFORM, Kind::JOBS, Kind::DSP];
pub const MIGRATIONS: &[Migrations] = &[
    Migrations {
        kind: Kind::PLATFORM,
        list: PLATFORM,
    },
    Migrations {
        kind: Kind::JOBS,
        list: JOBS,
    },
    Migrations {
        kind: Kind::DSP,
        list: DSP,
    },
];

const PLATFORM: &[Migration] = &[
    Migration {
        id: 1,
        name: "baseline",
        apply: Sql(include_str!("../migrations/platform/0001_baseline.sql")),
    },
    // The baseline has these columns. A database older than each of them gained it
    // from that release's startup, or gains it here.
    Migration {
        id: 2,
        name: "role_columns_and_indexes",
        apply: Code(role_columns),
    },
    Migration {
        id: 3,
        name: "audit_actor_name",
        apply: Code(audit_actor_name),
    },
    Migration {
        id: 4,
        name: "audit_data_and_shown",
        apply: Code(audit_data_and_shown),
    },
    Migration {
        id: 5,
        name: "outbox_context",
        apply: Code(outbox_context),
    },
    Migration {
        id: 6,
        name: "account_security",
        apply: Sql(include_str!(
            "../../accounts/migrations/platform/0006_account_security.sql"
        )),
    },
    Migration {
        id: 7,
        name: "dsp_features",
        apply: Sql(include_str!(
            "../../tenancy/migrations/platform/0007_dsp_features.sql"
        )),
    },
    Migration {
        id: 8,
        name: "security_hardening",
        apply: Code(security_hardening),
    },
    Migration {
        id: 9,
        name: "features_kept_on",
        apply: Sql(include_str!(
            "../../tenancy/migrations/platform/0009_features_kept_on.sql"
        )),
    },
    Migration {
        id: 11,
        name: "agent_keys",
        apply: Sql(include_str!(
            "../../mcp/migrations/platform/0011_agent_keys.sql"
        )),
    },
    Migration {
        id: 12,
        name: "scorecard_feature",
        apply: Sql(include_str!(
            "../../tenancy/migrations/platform/0012_scorecard_feature.sql"
        )),
    },
    Migration {
        id: 13,
        name: "oauth",
        apply: Code(oauth),
    },
    Migration {
        id: 14,
        name: "oauth_guard",
        apply: Code(oauth_guard),
    },
    Migration {
        id: 15,
        name: "agent_activity",
        apply: Sql(include_str!(
            "../../mcp/migrations/platform/0015_agent_activity.sql"
        )),
    },
    Migration {
        id: 16,
        name: "agent_reads",
        apply: Code(agent_reads),
    },
];
const JOBS: &[Migration] = &[
    Migration {
        id: 1,
        name: "baseline",
        apply: Sql(include_str!("../migrations/jobs/0001_baseline.sql")),
    },
    Migration {
        id: 2,
        name: "scorecard_kind",
        apply: Sql(include_str!(
            "../../collection/migrations/jobs/0002_scorecard_kind.sql"
        )),
    },
    Migration {
        id: 3,
        name: "routes_kind",
        apply: Sql(include_str!(
            "../../collection/migrations/jobs/0003_routes_kind.sql"
        )),
    },
    Migration {
        id: 4,
        name: "dvic_kind",
        apply: Sql(include_str!(
            "../../collection/migrations/jobs/0004_dvic_kind.sql"
        )),
    },
];
const DSP: &[Migration] = &[
    Migration {
        id: 1,
        name: "baseline",
        apply: Sql(include_str!("../migrations/dsp/0001_baseline.sql")),
    },
    Migration {
        id: 3,
        name: "scorecard_collection",
        apply: Sql(include_str!(
            "../../collection/migrations/dsp/0003_scorecard_collection.sql"
        )),
    },
    Migration {
        id: 4,
        name: "routes_collection",
        apply: Sql(include_str!(
            "../../collection/migrations/dsp/0004_routes_collection.sql"
        )),
    },
    Migration {
        id: 5,
        name: "storage_identity",
        apply: Sql(include_str!(
            "../../collection/migrations/dsp/0005_storage_identity.sql"
        )),
    },
    Migration {
        id: 6,
        name: "dvic_collection",
        apply: Sql(include_str!(
            "../../collection/migrations/dsp/0006_dvic_collection.sql"
        )),
    },
];
fn role_columns(db: &Db) -> Result<()> {
    add_column(db, "memberships", "role_id", "TEXT REFERENCES roles(id)")?;
    add_column(db, "invitations", "role_id", "TEXT")?;
    // Here rather than in the baseline, which would fail on a database without the columns.
    db.0.execute_batch(
        "CREATE INDEX IF NOT EXISTS memberships_role ON memberships(role_id); \
        CREATE INDEX IF NOT EXISTS invitations_role ON invitations(role_id) WHERE used_at IS NULL;",
    )?;
    Ok(())
}
// Names the actor once their account is deleted.
fn audit_actor_name(db: &Db) -> Result<()> {
    add_column(db, "audit", "actor_name", "TEXT")
}
// What a queued message was for, so Diagnostics can follow an invitation from the email
// to the moment it is accepted. Mail queued before this has none.
fn outbox_context(db: &Db) -> Result<()> {
    add_column(db, "outbox", "kind", "TEXT")?;
    add_column(db, "outbox", "invitation_hash", "TEXT")?;
    add_column(db, "outbox", "user_id", "TEXT")
}
// data: who or what an event touched, and the values it changed, as JSON.
// shown: set when a platform owner acted in a DSP that shows Platform support.
fn audit_data_and_shown(db: &Db) -> Result<()> {
    add_column(db, "audit", "data", "TEXT")?;
    add_column(db, "audit", "shown", "INTEGER")
}

// kind: a key made on the Agents page, or an app the owner connected through OAuth, which
// carries the app's client id, the name it goes by and whether Dispatch knows it.
fn oauth(db: &Db) -> Result<()> {
    add_column(
        db,
        "agent_keys",
        "kind",
        "TEXT NOT NULL DEFAULT 'key' CHECK(kind IN ('key','app'))",
    )?;
    add_column(db, "agent_keys", "client_id", "TEXT")?;
    add_column(db, "agent_keys", "client_name", "TEXT")?;
    add_column(
        db,
        "agent_keys",
        "client_verified",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    db.0.execute_batch(include_str!("../../mcp/migrations/platform/0013_oauth.sql"))?;
    Ok(())
}
// browser: the SHA-256 of the nonce the browser that asked to connect was given in a cookie,
// so that only that browser sees or answers the request.
fn oauth_guard(db: &Db) -> Result<()> {
    add_column(db, "oauth_requests", "browser", "TEXT")?;
    db.0.execute_batch(include_str!(
        "../../mcp/migrations/platform/0014_oauth_guard.sql"
    ))?;
    Ok(())
}

// areas and bypass: what a key or app reads, and whether it reads it where a DSP has the
// feature switched off. A key an older release makes reads every kind but addresses.
// bypassed: whether an agent's call read a switched-off feature so.
fn agent_reads(db: &Db) -> Result<()> {
    add_column(
        db,
        "agent_keys",
        "areas",
        "TEXT NOT NULL DEFAULT 'routes,timecards,meal_breaks,dvic,feedback,safety,returns,scorecard'",
    )?;
    add_column(
        db,
        "agent_keys",
        "bypass",
        "INTEGER NOT NULL DEFAULT 0 CHECK(bypass IN (0,1))",
    )?;
    add_column(
        db,
        "agent_activity",
        "bypassed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    db.0.execute_batch(include_str!(
        "../../mcp/migrations/platform/0016_agent_reads.sql"
    ))?;
    Ok(())
}

fn security_hardening(db: &Db) -> Result<()> {
    db.0.execute_batch(include_str!(
        "../../accounts/migrations/platform/0008_security_hardening.sql"
    ))?;
    add_column(
        db,
        "throttle",
        "namespace",
        "TEXT NOT NULL DEFAULT 'legacy'",
    )?;
    db.0.execute_batch(
        "CREATE INDEX IF NOT EXISTS throttle_namespace_expiry ON throttle(namespace, reset_at)",
    )?;
    Ok(())
}
