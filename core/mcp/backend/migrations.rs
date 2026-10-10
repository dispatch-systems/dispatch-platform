//! What agents' keys, connected apps and their activity add to the platform's database,
//! numbered with core's own migrations there. They shipped in core's list, so each keeps the
//! number, name and steps it was recorded with.
use crate::{
    Result,
    db::{
        Db, Kind, Migration, Migrations,
        migrations::{
            Apply::{Code, Sql},
            add_column,
        },
    },
};

pub const MIGRATIONS: &[Migrations] = &[Migrations {
    kind: Kind::PLATFORM,
    list: &[
        Migration {
            id: 11,
            name: "agent_keys",
            apply: Sql(include_str!("../migrations/platform/0011_agent_keys.sql")),
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
                "../migrations/platform/0015_agent_activity.sql"
            )),
        },
        Migration {
            id: 16,
            name: "agent_reads",
            apply: Code(agent_reads),
        },
        Migration {
            id: 21,
            name: "agent_tools",
            apply: Code(agent_tools),
        },
    ],
}];

/// The tables of the platform's database it keeps.
pub const TABLES: crate::manifest::Tables = &[(
    "platform",
    &[
        "agent_keys",
        "agent_key_dsps",
        "agent_key_dsp_reads",
        "agent_key_tools",
        "agent_activity",
        "oauth_apps",
        "oauth_clients",
        "oauth_codes",
        "oauth_pairing",
        "oauth_requests",
        "oauth_tokens",
    ],
)];

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
    db.0.execute_batch(include_str!("../migrations/platform/0013_oauth.sql"))?;
    Ok(())
}

// browser: the SHA-256 of the nonce the browser that asked to connect was given in a cookie,
// so that only that browser sees or answers the request.
fn oauth_guard(db: &Db) -> Result<()> {
    add_column(db, "oauth_requests", "browser", "TEXT")?;
    db.0.execute_batch(include_str!("../migrations/platform/0014_oauth_guard.sql"))?;
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
    db.0.execute_batch(include_str!("../migrations/platform/0016_agent_reads.sql"))?;
    Ok(())
}

// all_tools: whether a key or app uses tools that only read as they are added. Keys and apps
// from before do, as they used every tool that read.
fn agent_tools(db: &Db) -> Result<()> {
    add_column(
        db,
        "agent_keys",
        "all_tools",
        "INTEGER NOT NULL DEFAULT 1 CHECK(all_tools IN (0,1))",
    )?;
    db.0.execute_batch(include_str!("../migrations/platform/0021_agent_tools.sql"))?;
    Ok(())
}
