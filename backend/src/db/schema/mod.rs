//! The migration list of every database kind. See `migrations` for the rules.
//! Each 0001 creates what a new database needs and changes nothing on a database
//! that older code created, which is how those were adopted.
use super::{
    Db,
    migrations::{Apply::Code, Apply::Sql, Migration, add_column},
};
use crate::Result;

pub const PLATFORM: &[Migration] = &[
    Migration {
        id: 1,
        name: "baseline",
        apply: Sql(include_str!("platform/0001_baseline.sql")),
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
        apply: Sql(include_str!("platform/0006_account_security.sql")),
    },
    Migration {
        id: 7,
        name: "dsp_features",
        apply: Sql(include_str!("platform/0007_dsp_features.sql")),
    },
    Migration {
        id: 8,
        name: "security_hardening",
        apply: Code(security_hardening),
    },
    Migration {
        id: 9,
        name: "features_kept_on",
        apply: Sql(include_str!("platform/0009_features_kept_on.sql")),
    },
    Migration {
        id: 10,
        name: "driver_codes",
        apply: Sql(include_str!("platform/0010_driver_codes.sql")),
    },
    Migration {
        id: 11,
        name: "agent_keys",
        apply: Sql(include_str!("platform/0011_agent_keys.sql")),
    },
    Migration {
        id: 12,
        name: "scorecard_feature",
        apply: Sql(include_str!("platform/0012_scorecard_feature.sql")),
    },
    Migration {
        id: 13,
        name: "oauth",
        apply: Code(oauth),
    },
    Migration {
        id: 14,
        name: "oauth_guard",
        apply: Sql(include_str!("platform/0014_oauth_guard.sql")),
    },
    Migration {
        id: 15,
        name: "agent_activity",
        apply: Sql(include_str!("platform/0015_agent_activity.sql")),
    },
];
pub const JOBS: &[Migration] = &[
    Migration {
        id: 1,
        name: "baseline",
        apply: Sql(include_str!("jobs/0001_baseline.sql")),
    },
    Migration {
        id: 2,
        name: "scorecard_kind",
        apply: Sql(include_str!("jobs/0002_scorecard_kind.sql")),
    },
    Migration {
        id: 3,
        name: "routes_kind",
        apply: Sql(include_str!("jobs/0003_routes_kind.sql")),
    },
    Migration {
        id: 4,
        name: "dvic_kind",
        apply: Sql(include_str!("jobs/0004_dvic_kind.sql")),
    },
];
pub const DSP: &[Migration] = &[
    Migration {
        id: 1,
        name: "baseline",
        apply: Sql(include_str!("dsp/0001_baseline.sql")),
    },
    Migration {
        id: 2,
        name: "uniform_inventory",
        apply: Sql(include_str!("dsp/0002_uniform_inventory.sql")),
    },
    Migration {
        id: 3,
        name: "scorecard_collection",
        apply: Sql(include_str!("dsp/0003_scorecard_collection.sql")),
    },
    Migration {
        id: 4,
        name: "routes_collection",
        apply: Sql(include_str!("dsp/0004_routes_collection.sql")),
    },
    Migration {
        id: 5,
        name: "storage_identity",
        apply: Sql(include_str!("dsp/0005_storage_identity.sql")),
    },
    Migration {
        id: 6,
        name: "dvic_collection",
        apply: Sql(include_str!("dsp/0006_dvic_collection.sql")),
    },
    Migration {
        id: 7,
        name: "driver_match",
        apply: Sql(include_str!("dsp/0007_driver_match.sql")),
    },
];
pub const PAYCOM: &[Migration] = &[
    Migration {
        id: 1,
        name: "baseline",
        apply: Sql(include_str!("paycom/0001_baseline.sql")),
    },
    Migration {
        id: 2,
        name: "employee_timecard_syncs",
        apply: Sql(include_str!("paycom/0002_employee_timecard_syncs.sql")),
    },
    Migration {
        id: 3,
        name: "employee_history_index",
        apply: Sql(include_str!("paycom/0003_employee_history_index.sql")),
    },
];
pub const CORTEX: &[Migration] = &[
    Migration {
        id: 1,
        name: "baseline",
        apply: Sql(include_str!("cortex/0001_baseline.sql")),
    },
    Migration {
        id: 2,
        name: "meal_stops",
        apply: Sql(include_str!("cortex/0002_meal_stops.sql")),
    },
];
pub const SCORECARD: &[Migration] = &[
    Migration {
        id: 1,
        name: "baseline",
        apply: Sql(include_str!("scorecard/0001_baseline.sql")),
    },
    Migration {
        id: 2,
        name: "sources",
        apply: Sql(include_str!("scorecard/0002_sources.sql")),
    },
    Migration {
        id: 3,
        name: "verified_scope",
        apply: Code(scorecard_verified_scope),
    },
];
pub const ROUTEDATA: &[Migration] = &[
    Migration {
        id: 1,
        name: "baseline",
        apply: Sql(include_str!("routedata/0001_baseline.sql")),
    },
    Migration {
        id: 2,
        name: "details",
        apply: Code(routedata_details),
    },
    Migration {
        id: 3,
        name: "task_keys",
        apply: Sql(include_str!("routedata/0003_task_keys.sql")),
    },
];

/// Removed tasks join `tasks`, marked inactive; the itinerary keeps its route-level lists;
/// breaks and unknown stops get tables, and two views pre-join the common questions.
fn routedata_details(db: &Db) -> Result<()> {
    add_column(db, "tasks", "active", "INTEGER NOT NULL DEFAULT 1")?;
    for column in ["rescue_actions", "sequence_edits", "pause_events"] {
        add_column(db, "itineraries", column, "TEXT")?;
    }
    db.0.execute_batch(include_str!("routedata/0002_details.sql"))?;
    Ok(())
}

fn scorecard_verified_scope(db: &Db) -> Result<()> {
    for table in ["scorecard_publications", "scorecard_weeks"] {
        add_column(
            db,
            table,
            "scope_verified",
            "INTEGER NOT NULL DEFAULT 0 CHECK(scope_verified IN (0,1))",
        )?;
    }
    Ok(())
}

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
    db.0.execute_batch(include_str!("platform/0013_oauth.sql"))?;
    Ok(())
}

fn security_hardening(db: &Db) -> Result<()> {
    db.0.execute_batch(include_str!("platform/0008_security_hardening.sql"))?;
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

pub const DVIC: &[Migration] = &[
    Migration {
        id: 1,
        name: "baseline",
        apply: Sql(include_str!("dvic/0001_baseline.sql")),
    },
    Migration {
        id: 2,
        name: "hidden_drivers",
        apply: Sql(include_str!("dvic/0002_hidden_drivers.sql")),
    },
    Migration {
        id: 3,
        name: "verified_scope",
        apply: Code(dvic_verified_scope),
    },
];

fn dvic_verified_scope(db: &Db) -> Result<()> {
    for table in [
        "dvic_reports",
        "dvic_inspections",
        "dvic_weeks",
        "dvic_runs",
    ] {
        add_column(
            db,
            table,
            "scope_verified",
            "INTEGER NOT NULL DEFAULT 0 CHECK(scope_verified IN (0,1))",
        )?;
    }
    Ok(())
}
