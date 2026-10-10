//! Core's test support: the registry a test installs, the store it starts from, and what a
//! test may change or read in core's tables, which only core's code runs SQL on. Core's tests
//! reach it as `crate::testing`; every other crate's tests as `dispatch_core::testing`, through
//! the `testing` feature.
use crate::{
    Error, Result,
    collection::{browser::Collected, registry::Provider},
    db::{self, Migration, Migrations, Store, migrations::Apply, s},
    foundation::config::Config,
    manifest::{self, Collector, Feature, Keeper, Registry, feature, mandatory, optional},
    mcp::{
        Caller,
        api::types::{AgentAccess, AgentKeyRequest},
        tools::{Answer, Cx, Effect, Nothing, Tool, Toolbox},
    },
    server::operations,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    os::unix::fs::PermissionsExt,
    sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    },
};

/// Installs a registry of `collectors` and `features`, what a test needs beside core,
/// unless one holding them is installed already. A test binary holds one registry: core's
/// own tests get core alone, each integration test just what it names, and the app's module
/// tests the app's whole registry, which holds every part they name.
pub fn install(collectors: &[&'static dyn Collector], features: &[&'static Feature]) {
    static INSTALLING: Mutex<()> = Mutex::new(());
    let _one = INSTALLING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let registry = manifest::installed().unwrap_or_else(|| {
        let registry = whole(collectors, features);
        manifest::install(registry);
        registry
    });
    for collector in collectors {
        assert!(
            registry.collectors.iter().any(|c| c.id() == collector.id()),
            "the installed registry has no {} collector",
            collector.id()
        );
    }
    for feature in features {
        assert!(
            registry.features.iter().any(|f| f.name == feature.name),
            "the installed registry has no {} feature",
            feature.name
        );
    }
}

/// A registry of `collectors` and `features`, with a stand-in for what they leave out.
pub fn whole(
    collectors: &[&'static dyn Collector],
    features: &[&'static Feature],
) -> &'static Registry {
    let mut parts = features.to_vec();
    parts.push(stand_in(collectors, features));
    leak(Registry {
        collectors: leak(collectors.to_vec()),
        features: leak(parts),
    })
}

/// What the parts a test names leave a registry without. A registry is whole: each of its
/// collections kept once, and each database's migrations numbered from 1 without a gap, though other owners add
/// some of them. A stand-in takes the place of each missing owner: it keeps nothing, owns
/// no table, and its migrations change nothing.
fn stand_in(
    collectors: &[&'static dyn Collector],
    features: &[&'static Feature],
) -> &'static Feature {
    let named = Registry {
        collectors: leak(collectors.to_vec()),
        features: leak(features.to_vec()),
    };
    let keeps: Vec<&'static dyn Keeper> = collectors
        .iter()
        .flat_map(|collector| collector.collections())
        .filter(|collection| named.keeping(collection.job_kind).is_none())
        .map(|collection| leak::<Unkept>(Unkept(collection.job_kind)) as &'static dyn Keeper)
        .collect();
    let lists = db::CORE_MIGRATIONS
        .iter()
        .chain(
            collectors
                .iter()
                .flat_map(|collector| collector.migrations()),
        )
        .chain(features.iter().flat_map(|feature| feature.migrations));
    let mut missing = Vec::new();
    for kind in named.databases() {
        let ids: BTreeSet<u32> = lists
            .clone()
            .filter(|owned| owned.kind == kind)
            .flat_map(|owned| owned.list.iter().map(|migration| migration.id))
            .collect();
        let gaps: Vec<Migration> = (1..ids.last().copied().unwrap_or(1))
            .filter(|id| !ids.contains(id))
            .map(|id| Migration {
                id,
                name: "stand_in",
                apply: Apply::Sql(""),
            })
            .collect();
        if !gaps.is_empty() {
            missing.push(Migrations {
                kind,
                list: leak(gaps),
            });
        }
    }
    // Running collections, it is switched like any page that does; else every DSP has it.
    let runs = !keeps.is_empty();
    leak(Feature {
        switch: if runs {
            optional("stand_in", "Stand-in", &[])
        } else {
            mandatory("stand_in", "Stand-in")
        },
        keeps: leak(keeps),
        migrations: leak(missing),
        tools: &[&StandInRead, &StandInChange],
        ..feature("stand_in")
    })
}
/// What a stand-in tool answers: the DSP it was handed.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct StandInAnswer {
    pub dsp: String,
}
/// A tool of the stand-in's that reads, for tests of what a key may use.
pub struct StandInRead;
impl Tool for StandInRead {
    const NAME: &'static str = "stand_in_read";
    const TITLE: &'static str = "Stand-in read";
    const DESCRIPTION: &'static str = "Answer with the DSP.";
    type Input = Nothing;
    type Output = StandInAnswer;
    fn call(cx: &Cx, _: Nothing) -> Answer<StandInAnswer> {
        Ok(StandInAnswer {
            dsp: cx.dsp().id.clone(),
        })
    }
}
/// A tool of the stand-in's that changes something: it records that it did.
pub struct StandInChange;
impl Tool for StandInChange {
    const NAME: &'static str = "stand_in_change";
    const TITLE: &'static str = "Stand-in change";
    const DESCRIPTION: &'static str = "Record a change at the DSP.";
    const EFFECT: Effect = Effect::Changes;
    type Input = Nothing;
    type Output = StandInAnswer;
    fn call(cx: &Cx, _: Nothing) -> Answer<StandInAnswer> {
        cx.audit("stand_in.changed", "Changed", None, &[])?;
        Ok(StandInAnswer {
            dsp: cx.dsp().id.clone(),
        })
    }
}
/// Keeps a collection of a collector a test names whose own keeper it leaves out.
struct Unkept(&'static str);
impl Keeper for Unkept {
    fn keeps(&self) -> &'static str {
        self.0
    }
    fn permission(&self) -> &'static str {
        "stand_in.collect"
    }
    fn publish(&self, _: &Store, _: &str, _: &str, _: Collected) -> Result<()> {
        Err(Error::new("collection_not_kept", 500))
    }
}
fn leak<T: ?Sized>(value: impl Into<Box<T>>) -> &'static T {
    Box::leak(value.into())
}

/// An empty preview platform in a private temporary directory, under the installed registry.
///
/// `Config` has no test constructor, so this is the one place that reads the process
/// environment; everything a test relies on is overridden here.
pub fn store() -> (tempfile::TempDir, Store) {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = Config::load().unwrap();
    config.root = root.path().to_owned();
    config.development = true;
    config.fixture = true;
    config.environment = "preview".into();
    config.standalone = true;
    let store = Store::initialize(config).unwrap();
    (root, store)
}

/// A platform with its first owner, `owner@example.test`, and the id of their empty DSP.
pub fn bootstrapped() -> (tempfile::TempDir, Store, String) {
    let (root, db) = store();
    let bootstrap = operations::bootstrap(
        &db,
        "owner@example.test",
        "Test",
        "Owner",
        "test-password-long",
    )
    .unwrap();
    let id = s(&bootstrap["dsp"], "id").to_owned();
    (root, db, id)
}

/// A platform holding the development fixtures: Northline Logistics, its owner and a member.
pub fn seeded() -> (tempfile::TempDir, Store) {
    let (root, db) = store();
    operations::seed(&db).unwrap();
    (root, db)
}

/// A key the platform owner made, reaching `dsps` (every DSP when none) and allowed `tools`
/// with those added later that only read, as an agent signs in with it.
pub fn agent(db: &Store, dsps: &[&str], tools: &[&str]) -> Caller {
    static MADE: AtomicU32 = AtomicU32::new(0);
    let made = MADE.fetch_add(1, Ordering::Relaxed);
    let request = AgentKeyRequest::parse(&json!({
        "name": format!("Test agent {made}"),
        "allDsps": dsps.is_empty(),
        "dsps": dsps,
        "access": AgentAccess::Read,
        "allTools": true,
        "tools": tools,
        "expiresAt": null,
    }))
    .unwrap();
    let key = db.create_agent_key(&platform_owner(db), &request).unwrap();
    db.authenticate_agent(&key.token, "test").unwrap()
}

/// Calls a tool of the installed registry as `caller`, checked as the MCP server checks a
/// call: `call_tool(&db, &caller, "approve_timecard", json!({"dsp": dsp, "date": "…"}))`.
pub fn call_tool(db: &Store, caller: &Caller, name: &str, arguments: Value) -> Answer<Value> {
    let Value::Object(arguments) = arguments else {
        panic!("a tool's arguments are an object");
    };
    Toolbox::installed()
        .invoke(db, caller, name, arguments)
        .answer
}

/// The newest audit events the platform (`None`) or one DSP may see.
pub fn audits(db: &Store, dsp: Option<&str>) -> Result<Value> {
    let mut page = db
        .audit_page(&db::AuditQuery {
            dsp,
            limit: 200,
            ..db::AuditQuery::default()
        })
        .map(|value| serde_json::to_value(value).unwrap())?;
    Ok(page["events"].take())
}

/// The actions of a DSP's audit events that start with `prefix`, oldest first.
pub fn audit_actions(db: &Store, dsp: &str, prefix: &str) -> Vec<String> {
    db.platform
        .all(
            "SELECT action FROM audit WHERE dsp_id=? AND action LIKE ? ORDER BY id",
            [dsp, &format!("{prefix}%")],
        )
        .unwrap()
        .iter()
        .map(|row| s(row, "action").to_owned())
        .collect()
}

/// The id of a user, any one: the first the platform holds.
pub fn a_user(db: &Store) -> String {
    let user = db
        .platform
        .one("SELECT id FROM users LIMIT 1", [])
        .unwrap()
        .unwrap();
    s(&user, "id").to_owned()
}

/// The id of the platform's owner.
pub fn platform_owner(db: &Store) -> String {
    let user = db
        .platform
        .one("SELECT id FROM users WHERE platform_owner=1", [])
        .unwrap()
        .unwrap();
    s(&user, "id").to_owned()
}

/// Renames a DSP and moves it to `timezone`, as nothing but a test does: no audit, and
/// its schedules keep their deadlines.
pub fn set_dsp(db: &Store, dsp: &str, name: &str, timezone: &str) -> Result<()> {
    db.platform.exec(
        "UPDATE dsps SET name=?,timezone=? WHERE id=?",
        [name, timezone, dsp],
    )?;
    Ok(())
}

/// Switches a DSP's connection to `provider` on, without checking its credentials.
pub fn enable_connection(db: &Store, dsp: &str, provider: Provider) -> Result<()> {
    db.collector(dsp, provider)?
        .exec("UPDATE connections SET enabled=1", [])?;
    Ok(())
}

/// Switches a DSP's connection to `provider` on and ready, as a verified sign-in leaves it.
pub fn ready_connection(db: &Store, dsp: &str, provider: Provider) -> Result<()> {
    db.collector(dsp, provider)?
        .exec("UPDATE connections SET enabled=1,status='ready'", [])?;
    Ok(())
}

/// Sets the revision of a DSP's connection to `provider`, as a credential change does.
pub fn set_connection_revision(
    db: &Store,
    dsp: &str,
    provider: Provider,
    revision: i64,
) -> Result<()> {
    db.collector(dsp, provider)?
        .exec("UPDATE connections SET revision=?", [revision])?;
    Ok(())
}

/// Moves a DSP's connection to `provider` to its next revision, as a credential change does.
pub fn change_connection(db: &Store, dsp: &str, provider: Provider) -> Result<()> {
    db.collector(dsp, provider)?
        .exec("UPDATE connections SET revision=revision+1", [])?;
    Ok(())
}

/// Sets a job's status, as a worker would leave it.
pub fn set_job_status(db: &Store, job: &str, status: &str) -> Result<()> {
    db.jobs
        .exec("UPDATE jobs SET status=? WHERE id=?", [status, job])?;
    Ok(())
}

/// Sets the status of every job of `kind`.
pub fn set_status_of_kind(db: &Store, kind: &str, status: &str) -> Result<()> {
    db.jobs
        .exec("UPDATE jobs SET status=? WHERE kind=?", [status, kind])?;
    Ok(())
}

/// Sets the status of every job.
pub fn set_every_status(db: &Store, status: &str) -> Result<()> {
    db.jobs.exec("UPDATE jobs SET status=?", [status])?;
    Ok(())
}

/// Dates a job's creation to `created_at`.
pub fn set_job_created(db: &Store, job: &str, created_at: &str) -> Result<()> {
    db.jobs
        .exec("UPDATE jobs SET created_at=? WHERE id=?", [created_at, job])?;
    Ok(())
}

/// Binds a job to the connection revision `revision`, as one queued after that change is.
pub fn set_job_connection_revision(db: &Store, job: &str, revision: i64) -> Result<()> {
    db.jobs.exec(
        "UPDATE jobs SET connection_revision=? WHERE id=?",
        rusqlite::params![revision, job],
    )?;
    Ok(())
}

/// Records finished jobs of `kind` for a DSP, each an id and when it was created, in one
/// transaction, as many workers would have left them.
pub fn finished_jobs(db: &Store, dsp: &str, kind: &str, jobs: &[(String, String)]) -> Result<()> {
    let transaction = db.jobs.0.unchecked_transaction()?;
    for (id, created_at) in jobs {
        db.jobs.exec(
            "INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,release,\
             connection_revision,idempotency_key) VALUES (?,?,'preview',?,'succeeded',0,?,'test',1,?)",
            rusqlite::params![id, dsp, kind, created_at, id],
        )?;
    }
    transaction.commit()?;
    Ok(())
}
