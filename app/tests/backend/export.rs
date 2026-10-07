//! The TypeScript the owners' API types are written to, each in its owner's api/generated/,
//! and the catalogs': the access catalog and the capabilities' labels in tenancy's, the
//! Agents page's read toggles in mcp's and the collections' labels in collection's, checked
//! against the files committed there.
use dispatch_core::accounts::api::types::*;
use dispatch_core::collection::api::{jobs::*, metrics::*, types::*};
use dispatch_core::collection::{
    browser::{self, Collected, Pending, browseros},
    registry::Provider,
};
use dispatch_core::db::{Kind, Migrations};
use dispatch_core::foundation::config::{Environment, ProviderMode};
use dispatch_core::manifest::{Capability, Collection, Collector};
use dispatch_core::mcp::api::types::*;
use dispatch_core::platform_owner::api::types::*;
use dispatch_core::server::api::types::*;
use dispatch_core::tenancy::api::{audit::*, types::*};
#[cfg(feature = "driver_match")]
use dispatch_driver_match::{
    Driver, DriverActivity, DriverCounts, DriverDay, DriverDetails, DriverEvent, DriverEventKind,
    DriverEvidence, DriverEvidenceKind, DriverId, DriverLink, DriverMatch, DriverPair,
    DriverStrength,
};
#[cfg(feature = "dvic")]
use dispatch_dvic::{DvicInspection, DvicInspections, DvicReport, DvicStatus, DvicWeek};
use dispatch_paycom::timecards::EmployeeTimecardPeriod;
#[cfg(feature = "routes")]
use dispatch_routes::{
    RouteAddress, RouteBreak, RouteDayView, RouteDays, RouteItinerary, RouteItineraryDetail,
    RoutePackage, RoutePackageEvent, RoutePublication, RouteReprocess, RouteRetention, RouteStop,
    RouteTask, RouteUnknownStop,
};
#[cfg(feature = "timecard")]
use dispatch_timecard::{
    AssessedClock, CortexMeal, CortexPublication, DailyTimecard, DailyTimecards, DeliveryGap,
    DeliveryGaps, DepartmentOption, Employee, EmployeeTimecard, EmployeeTimecardResponse,
    EmployeesResponse, InPunchKind, LateRule, Lunch, MatchType, MealAssessment, MealComparison,
    MealDriver, MealEmployee, MealPair, MealPaycom, MealSource, MealStatus, NameOrder,
    OutPunchKind, PaycomColumn, PaycomDay, PaycomOptions, PaycomPage, PaycomPreferences,
    PaycomSettings, PaycomSort, PreferenceRevision, Punch, PunchEvent, Timecard,
};
#[cfg(feature = "uniforms")]
use dispatch_uniforms::{
    Uniform, UniformAdjustment, UniformEvent, UniformEventKind, UniformFit, UniformHistory,
    UniformInventory, UniformUpdates, UniformVariant,
};
#[cfg(feature = "weekly_scorecard")]
use dispatch_weekly_scorecard::{
    WeeklyScorecardDatasetCount, WeeklyScorecardPolicy, WeeklyScorecardPublication,
    WeeklyScorecardWeek, WeeklyScorecardWeeks,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use ts_rs::TS;

/// The access catalog's file: its types are tenancy's.
const ACCESS_CATALOG: &str = "core/tenancy/api/generated/access-catalog.ts";
// The labels only the platform owner's pages show, each in a file of its own, so that no
// other page loads them: how the DSPs page names what a page needs, the Agents page's read
// toggles, and how Diagnostics and the audit log name each collection. The job schema reads
// the collections' kinds too, so every page loads that one.
const CAPABILITIES: &str = "core/tenancy/api/generated/capabilities.ts";
const READ_TOGGLES: &str = "core/mcp/api/generated/read-toggles.ts";
const COLLECTIONS: &str = "core/collection/api/generated/collections.ts";
/// The kinds of record an audit event names, core's and the features', which the audit
/// log's reply check accepts.
const AUDIT_SUBJECTS: &str = "core/tenancy/api/generated/audit-subjects.ts";

/// Each binding by its file's path from the repository root, which its type's `export_to`
/// names, so ts-rs writes the imports between owners' folders from there.
macro_rules! exported {
    ($cfg:expr, $($ty:ty),* $(,)?) => {
        BTreeMap::from([$((
            <$ty>::output_path().expect("named type"),
            <$ty>::export_to_string($cfg).expect("exportable type")
                .lines().map(|line| format!("{}\n", line.trim_end())).collect::<String>(),
        )),*])
    };
}
fn bindings(root: &Path) -> BTreeMap<PathBuf, String> {
    let cfg = ts_rs::Config::new().with_out_dir(root);
    let mut bindings = exported!(
        &cfg,
        AgentAccess,
        AgentArea,
        AgentSource,
        AgentReads,
        AgentDspReads,
        AgentKeyKind,
        AgentAppStatus,
        AgentClient,
        AgentKey,
        AgentDsp,
        AgentKeyDsp,
        AgentKeys,
        AgentKeyCreated,
        AgentKeyRequest,
        AgentKeysRevoked,
        AgentWhoami,
        AgentWhoamiKey,
        AgentWhoamiDsp,
        AgentActivityKey,
        AgentActivity,
        AgentActivityPage,
        OAuthApproval,
        OAuthRequest,
        OAuthApp,
        OAuthReplaced,
        OAuthRedirect,
        OAuthPairing,
        OAuthPairingOpened,
        OAuthAppId,
        OAuthAllowedApp,
        OAuthAllowedApps,
        OAuthAppChoice,
        Cadence,
        DriverSource,
        DriverStatus,
        DriverData,
        DspProfile,
        JobMetrics,
        JobPhase,
        JobOutcome,
        PageRead,
        PageReads,
        PageStage,
        DocumentState,
        AuditArea,
        AuditSubject,
        AuditReference,
        AuditChange,
        AuditEvent,
        AuditName,
        AuditPage,
        AuditCount,
        BrowserAdmission,
        BrowserHealth,
        TransportHealth,
        MailHealth,
        PlatformHealth,
        CollectionChange,
        CollectionUpdates,
        CollectionSchedule,
        CollectionSchedules,
        Connection,
        ConnectionStatus,
        Dsp,
        DspStatus,
        DspFeatureReport,
        DspFeatures,
        DspSummary,
        DspView,
        FeatureChange,
        FeatureState,
        EmployeeTimecardPeriod,
        Environment,
        JobStatus,
        MailMessage,
        Member,
        OwnerStatus,
        Presence,
        ProviderMode,
        PublicJob,
        PublicUser,
        Role,
        RoleSummary,
        RuntimeSource,
        ScheduleCollection,
        SchedulePreview,
        SessionResponse,
        SecurityStatus,
        PasskeySummary,
        AuthenticatorSetup,
        AccountSession,
    );
    // Each feature's, when the build has it.
    #[cfg(feature = "timecard")]
    bindings.extend(exported!(
        &cfg,
        PaycomPreferences,
        PaycomPage,
        PaycomSort,
        PaycomColumn,
        NameOrder,
        PreferenceRevision,
        DepartmentOption,
        PaycomOptions,
        PaycomSettings,
        Employee,
        Punch,
        InPunchKind,
        OutPunchKind,
        Timecard,
        EmployeeTimecard,
        DailyTimecard,
        DailyTimecards,
        EmployeesResponse,
        AssessedClock,
        Lunch,
        PunchEvent,
        PaycomDay,
        DeliveryGap,
        DeliveryGaps,
        MealPair,
        MealAssessment,
        MealStatus,
        LateRule,
        CortexMeal,
        MealPaycom,
        MealSource,
        MealEmployee,
        MatchType,
        MealDriver,
        CortexPublication,
        MealComparison,
        EmployeeTimecardResponse,
    ));
    #[cfg(feature = "uniforms")]
    bindings.extend(exported!(
        &cfg,
        UniformFit,
        UniformEventKind,
        UniformVariant,
        Uniform,
        UniformInventory,
        UniformAdjustment,
        UniformUpdates,
        UniformEvent,
        UniformHistory,
    ));
    #[cfg(feature = "routes")]
    bindings.extend(exported!(
        &cfg,
        RouteAddress,
        RouteBreak,
        RouteDayView,
        RouteDays,
        RouteItinerary,
        RouteItineraryDetail,
        RoutePackage,
        RoutePackageEvent,
        RoutePublication,
        RouteReprocess,
        RouteRetention,
        RouteStop,
        RouteTask,
        RouteUnknownStop,
    ));
    #[cfg(feature = "dvic")]
    bindings.extend(exported!(
        &cfg,
        DvicReport,
        DvicWeek,
        DvicStatus,
        DvicInspection,
        DvicInspections,
    ));
    #[cfg(feature = "weekly_scorecard")]
    bindings.extend(exported!(
        &cfg,
        WeeklyScorecardDatasetCount,
        WeeklyScorecardPublication,
        WeeklyScorecardWeek,
        WeeklyScorecardWeeks,
        WeeklyScorecardPolicy,
    ));
    #[cfg(feature = "driver_match")]
    bindings.extend(exported!(
        &cfg,
        DriverLink,
        DriverStrength,
        DriverEvidenceKind,
        DriverEventKind,
        DriverId,
        Driver,
        DriverCounts,
        DriverEvidence,
        DriverPair,
        DriverMatch,
        DriverActivity,
        DriverDay,
        DriverEvent,
        DriverDetails,
    ));
    #[cfg(feature = "daily_performance")]
    bindings.extend(exported!(
        &cfg,
        dispatch_daily_performance::DailyPerformanceSummary,
        dispatch_daily_performance::DailyPerformanceDay,
        dispatch_daily_performance::DailyPerformanceDataset,
        dispatch_daily_performance::DailyPerformancePolicy,
    ));
    bindings.insert(ACCESS_CATALOG.into(), access_catalog());
    bindings.insert(CAPABILITIES.into(), capabilities());
    bindings.insert(READ_TOGGLES.into(), read_toggles());
    bindings.insert(COLLECTIONS.into(), collections());
    bindings.insert(AUDIT_SUBJECTS.into(), audit_subjects());
    bindings
}

/// A generated file of constants, each `as const`, under a header naming what they are
/// generated from.
fn generated(from: &str, constants: &[(&str, serde_json::Value)]) -> String {
    let mut text = format!(
        "// Generated from {from}.\n// Run `npm run contracts:generate` after changing them.\n"
    );
    for (name, value) in constants {
        text.push_str(&format!(
            "export const {name} = {} as const;\n",
            serde_json::to_string_pretty(value).unwrap()
        ));
    }
    text
}

/// How the DSPs page names each capability a page needs.
fn capabilities() -> String {
    let labels = capability_labels(dispatch_core::manifest::registry().collectors);
    generated(
        "the collectors' capabilities",
        &[("capabilityLabels", labels.into())],
    )
}
/// Each capability `collectors` supply, named as the first of them listed that supplies it
/// names it.
fn capability_labels(collectors: &[&dyn Collector]) -> serde_json::Map<String, serde_json::Value> {
    let mut labels = serde_json::Map::new();
    for collector in collectors {
        for capability in collector.capabilities() {
            labels
                .entry(capability.id)
                .or_insert(capability.label.into());
        }
    }
    labels
}

/// The Agents page's switches: each feature's kinds of data under its switch's name, the
/// features in the order of their kinds, with how a key's row names what it doesn't read.
fn read_toggles() -> String {
    use dispatch_core::{manifest::registry, mcp::api::types::AgentArea};
    use serde_json::json;
    let mut groups: Vec<_> = registry()
        .features
        .iter()
        .filter(|feature| !feature.mcp.reads.is_empty())
        .collect();
    groups.sort_by_key(|feature| feature.mcp.reads.iter().map(|area| area.order()).min());
    let listed = groups.iter().flat_map(|feature| feature.mcp.reads).copied();
    assert!(
        listed.eq(AgentArea::all()),
        "the Agents page lists each kind of data once, under its feature, in the one order"
    );
    let groups: Vec<_> = groups
        .iter()
        .map(|feature| {
            let switch = feature
                .switch
                .expect("a feature whose data agents read has a switch");
            json!({
                "label": switch.label,
                "missing": feature.mcp.missing,
                "sources": feature.mcp.sources.iter()
                    .map(|source| json!({"id": source.as_str(), "label": source.label()}))
                    .collect::<Vec<_>>(),
                "toggles": feature.mcp.reads.iter()
                    .map(|area| json!({
                        "id": area.as_str(),
                        "label": area.label(),
                        "hint": area.hint(),
                        "missing": area.missing(),
                        "source": area.source().as_str(),
                        "with": area.with().map(AgentArea::as_str),
                        "optIn": area.opt_in(),
                    }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    generated(
        "the features' read toggles",
        &[("readToggleGroups", groups.into())],
    )
}

/// Every collection, with the collector that runs it, as Diagnostics and the audit log name
/// it: its job kind, its schedules' collection and label, and what its workload counts.
fn collections() -> String {
    use serde_json::json;
    let collections: Vec<_> = Provider::all()
        .flat_map(|provider| {
            let collector = provider.collector();
            collector.collections().iter().map(move |collection| {
                json!({
                    "kind": collection.job_kind,
                    "provider": collector.id(),
                    "schedule": collection.schedule,
                    "label": collection.label,
                    "unit": collection.unit,
                    "counted": collection.counted.field(),
                })
            })
        })
        .collect();
    generated(
        "the collectors' collections",
        &[("collections", collections.into())],
    )
}

/// Every kind of record an audit event may name, in the registry's order.
fn audit_subjects() -> String {
    let kinds: Vec<_> = AuditSubject::all().map(AuditSubject::as_str).collect();
    generated(
        "the audit log's subjects",
        &[("auditSubjects", kinds.into())],
    )
}

/// Runtime catalogs, exported from their actual values rather than Rust source formatting.
fn access_catalog() -> String {
    use dispatch_core::tenancy::{catalog, roles};
    use serde_json::json;
    let catalog = catalog::catalog();
    let ids = |kind| {
        catalog
            .iter()
            .filter(|feature| match (feature.kind, kind) {
                (catalog::Kind::Tab(_), catalog::Kind::Tab(_)) => true,
                (actual, expected) => actual == expected,
            })
            .map(|feature| feature.id)
            .collect::<Vec<_>>()
    };
    let entries: Vec<_> = catalog
        .iter()
        .map(|feature| {
            let mut entry = json!({
                "id": feature.id,
                "label": feature.label,
                "permissions": feature.permissions,
                "requires": feature.requires,
            });
            match feature.kind {
                catalog::Kind::Page => entry["kind"] = json!("page"),
                catalog::Kind::Tab(page) => {
                    entry["kind"] = json!("tab");
                    entry["page"] = json!(page);
                }
                catalog::Kind::Connection => {
                    entry["kind"] = json!("connection");
                    entry["provides"] = json!(feature.provides);
                }
            }
            entry
        })
        .collect();
    let labels: BTreeMap<_, _> = roles::LABELS.iter().copied().collect();
    let implied: BTreeMap<_, _> = roles::IMPLIED.iter().copied().collect();
    let groups: Vec<_> = catalog::pages()
        .map(|feature| (feature.label, feature.permissions))
        .chain(
            roles::GROUPS
                .iter()
                .map(|(group, permissions)| (*group, permissions.as_slice())),
        )
        .collect();
    let mut grouped: Vec<_> = groups
        .iter()
        .flat_map(|(_, permissions)| *permissions)
        .copied()
        .collect();
    let mut permissions = roles::PERMISSIONS.to_vec();
    grouped.sort();
    permissions.sort();
    assert_eq!(
        grouped, permissions,
        "every permission belongs to one role-sheet section"
    );
    assert!(
        implied
            .iter()
            .all(|(from, to)| labels.contains_key(from) && labels.contains_key(to))
    );
    let constants = [
        ("pages", json!(ids(catalog::Kind::Page))),
        ("pageTabs", json!(ids(catalog::Kind::Tab("")))),
        ("connections", json!(ids(catalog::Kind::Connection))),
        (
            "features",
            json!(catalog.iter().map(|feature| feature.id).collect::<Vec<_>>()),
        ),
        ("featureCatalog", json!(entries)),
        ("permissions", json!(*roles::PERMISSIONS)),
        ("permissionLabels", json!(labels)),
        ("permissionGroups", json!(groups)),
        ("impliedPermissions", json!(implied)),
    ];
    generated(
        "backend feature, collector and permission catalogs",
        &constants,
    )
}
/// Every owner's api/generated/ folder there is: core's parts', the collectors' and the
/// features'.
fn generated_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for top in ["core", "collectors", "features"] {
        for entry in std::fs::read_dir(root.join(top)).expect("an owners' folder") {
            let dir = entry.unwrap().path().join("api/generated");
            if dir.is_dir() {
                dirs.push(dir);
            }
        }
    }
    dirs
}
#[test]
fn typescript_contracts_match_the_rust_types() {
    crate::install();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|dir| dir.join("Cargo.lock").is_file())
        .expect("repository root");
    let bindings = bindings(root);
    for file in bindings.keys() {
        let parts: Vec<_> = file.iter().filter_map(|part| part.to_str()).collect();
        assert!(
            matches!(
                parts[..],
                ["core" | "collectors" | "features", _, "api", "generated", _]
            ),
            "{} is in no owner's api/generated/: name it in its type's export_to",
            file.display()
        );
    }
    if std::env::var_os("DISPATCH_UPDATE_CONTRACTS").is_some() {
        for dir in generated_dirs(root) {
            std::fs::remove_dir_all(dir).unwrap();
        }
        for (file, text) in &bindings {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
    }
    // A file no type writes is stale too.
    let mut stored = BTreeMap::new();
    for dir in generated_dirs(root) {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let file = path.strip_prefix(root).unwrap().to_owned();
            stored.insert(file, std::fs::read_to_string(&path).unwrap());
        }
    }
    assert!(
        stored == bindings,
        "an owner's api/generated/ is out of date: run `npm run contracts:generate`"
    );
}
/// A collector that only supplies capabilities, under its id.
struct StandIn(&'static str, &'static [Capability]);
impl Collector for StandIn {
    fn id(&self) -> &'static str {
        self.0
    }
    fn label(&self) -> &'static str {
        self.0
    }
    fn capabilities(&self) -> &'static [Capability] {
        self.1
    }
    fn collections(&self) -> &'static [Collection] {
        &[]
    }
    fn database(&self) -> Kind {
        unreachable!("a stand-in only supplies capabilities")
    }
    fn migrations(&self) -> &'static [Migrations] {
        &[]
    }
    fn seed(&self, _: &str) -> String {
        unreachable!("a stand-in only supplies capabilities")
    }
    fn marker(&self) -> Option<&'static str> {
        None
    }
    fn browser_entries(&self) -> &'static [&'static str] {
        &[]
    }
    fn network(&self) -> browseros::NetworkPolicy {
        unreachable!("a stand-in only supplies capabilities")
    }
    fn validate_credentials(&self, _: &serde_json::Value) -> dispatch_core::Result<()> {
        unreachable!("a stand-in only supplies capabilities")
    }
    fn driver<'a>(
        &self,
        _: browseros::Session,
        _: &'a Path,
        _: Option<&'a str>,
    ) -> Pending<'a, Box<dyn browser::Driver>> {
        unreachable!("a stand-in only supplies capabilities")
    }
    fn fixture(&self, _: &str, _: &serde_json::Value) -> dispatch_core::Result<Collected> {
        unreachable!("a stand-in only supplies capabilities")
    }
    fn progress(&self, _: &serde_json::Value) -> &'static str {
        unreachable!("a stand-in only supplies capabilities")
    }
}
#[test]
fn a_capability_is_named_by_the_first_collector_listed_that_supplies_it() {
    let alpha = StandIn(
        "alpha",
        &[Capability {
            id: "photos",
            label: "a photo source",
        }],
    );
    let beta = StandIn(
        "beta",
        &[
            Capability {
                id: "photos",
                label: "another photo source",
            },
            Capability {
                id: "notes",
                label: "a notes source",
            },
        ],
    );
    let labels = capability_labels(&[&alpha, &beta]);
    assert_eq!(labels["photos"], "a photo source");
    assert_eq!(labels["notes"], "a notes source");
    assert_eq!(labels.get("maps"), None);
    // Listed the other way round, the other names it.
    assert_eq!(
        capability_labels(&[&beta, &alpha])["photos"],
        "another photo source"
    );
}

/// The feature map: every feature this build has, by name, with what it declares.
const FEATURE_MAP: &str = "app/generated/features.json";
/// The frontend's list of every owner's manifest.
const FRONTEND_LIST: &str = "app/frontend/features.ts";

/// Whether this build has the owner at `dir`, such as `features/timecard`: core's parts are
/// in every build, and so are the collectors it registers.
fn in_build(dir: &str) -> bool {
    match dir.split_once('/') {
        Some(("core", _)) => true,
        Some(("collectors", id)) => crate::REGISTRY.collectors.iter().any(|c| c.id() == id),
        Some(("features", name)) => crate::REGISTRY.features.iter().any(|f| f.name == name),
        _ => panic!("{dir} is no owner's directory"),
    }
}

/// Each feature this build has, by name, with what it declares: its ids, the features and
/// collectors it uses, the collections it keeps and their collectors, its tables, its
/// migrations, and what it adds to each slot. Sorted, so it changes only with them.
fn feature_map(root: &Path) -> String {
    use serde_json::json;
    use std::collections::BTreeSet;
    let registry = &crate::REGISTRY;
    let mut map = BTreeMap::new();
    for feature in registry.features {
        let kinds: Vec<_> = feature.keeps.iter().map(|keeper| keeper.keeps()).collect();
        let collectors = registry
            .collectors
            .iter()
            .filter(|collector| {
                let collected = collector.collections().iter();
                collected
                    .map(|c| c.job_kind)
                    .any(|kind| kinds.contains(&kind))
            })
            .map(|collector| collector.id())
            .collect::<BTreeSet<_>>();
        let mut tables: BTreeMap<_, BTreeSet<&str>> = BTreeMap::new();
        for (database, names) in feature.tables {
            tables
                .entry(*database)
                .or_default()
                .extend(names.iter().copied());
        }
        let mut migrations: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for owned in feature.migrations {
            let list = migrations.entry(owned.kind.name()).or_default();
            list.extend(
                owned
                    .list
                    .iter()
                    .map(|m| json!({"id": m.id, "name": m.name})),
            );
            list.sort_by_key(|migration| migration["id"].as_u64());
        }
        let mcp = &feature.mcp;
        let audit = &feature.audit;
        let slots = json!({
            "after_collection": feature.after_collection.is_some(),
            "audit": {
                "areas": audit.areas.iter().map(|(prefix, area)| json!([prefix, area.as_str()]))
                    .collect::<Vec<_>>(),
                "names": audit.names.is_some(),
                "subjects": audit.subjects,
            },
            "cached": feature.cached.iter().map(|cached| cached.read).collect::<Vec<_>>(),
            "commands": feature.commands.as_ref().map(|commands| commands.prefix),
            "demo": feature.demo.is_some(),
            "domains": feature.domains.iter().map(|domain| domain.id()).collect::<Vec<_>>(),
            "live": feature.live,
            "maintenance": feature.maintenance.iter().map(|task| task.every.as_secs())
                .collect::<Vec<_>>(),
            "mcp": {
                "daily": mcp.daily.iter().map(|daily| daily.coverage()).collect::<Vec<_>>(),
                "examples": mcp.examples.len(),
                "endpoints": mcp.endpoints.iter().map(|endpoint| endpoint.tool)
                    .collect::<Vec<_>>(),
                "identity": mcp.identity.is_some(),
                "metrics": mcp.metrics.iter().map(|metric| metric.name).collect::<Vec<_>>(),
                "places": mcp.places.is_some(),
                "reads": mcp.reads.iter().map(|area| area.as_str()).collect::<Vec<_>>(),
                "sources": mcp.sources.iter().map(|source| source.as_str()).collect::<Vec<_>>(),
                "synthetic": mcp.synthetic.steps.len(),
                "terms": mcp.terms.iter().map(|term| term.term).collect::<Vec<_>>(),
            },
            "people": feature.people.iter().map(|people| people.data().as_str())
                .collect::<Vec<_>>(),
            "routes": (feature.routes)().iter()
                .map(|route| format!("{} {}", route.method, route.path))
                .collect::<Vec<_>>(),
            "schedules": feature.schedules,
        });
        let dir = format!("features/{}", feature.name);
        let entry = json!({
            "collectors": collectors,
            "depends_on": feature.depends_on.iter().collect::<BTreeSet<_>>(),
            "frontend": root.join(&dir).join("frontend/feature.ts").is_file(),
            "keeps": kinds.iter().collect::<BTreeSet<_>>(),
            "migrations": migrations,
            "permissions": feature.permissions.iter().map(|p| p.id).collect::<Vec<_>>(),
            "slots": slots,
            "switch": feature.switch.map(|switch| switch.id),
            "tables": tables,
            "tabs": feature.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
        });
        map.insert(feature.name, entry);
    }
    serde_json::to_string_pretty(&map).unwrap() + "\n"
}

/// The frontend's list: an import of each owner's `frontend/feature.ts` this build has, in
/// the order of the app's `FRONTEND`, which must name every owner that has one.
fn frontend_list(root: &Path) -> String {
    let manifest = |dir: &str| root.join(dir).join("frontend/feature.ts").is_file();
    for top in ["core", "collectors", "features"] {
        for entry in std::fs::read_dir(root.join(top)).expect("an owners' folder") {
            let dir = format!("{top}/{}", entry.unwrap().file_name().to_string_lossy());
            assert!(
                !manifest(&dir) || !in_build(&dir) || crate::FRONTEND.contains(&dir.as_str()),
                "{dir} has a frontend/feature.ts, but no place in the app's FRONTEND"
            );
        }
    }
    let owners: Vec<_> = crate::FRONTEND
        .iter()
        .filter(|dir| {
            // Only a build that leaves features out may name one it does not have.
            assert!(
                in_build(dir) || cfg!(not(feature = "default")),
                "the app's FRONTEND names {dir}, which the app does not register"
            );
            in_build(dir)
        })
        .collect();
    let mut imports = String::new();
    let mut list = String::new();
    for dir in owners {
        assert!(
            manifest(dir),
            "{dir} is in the app's FRONTEND, but has no frontend/feature.ts"
        );
        let name = dir.rsplit('/').next().unwrap();
        let local: String = name
            .split('_')
            .enumerate()
            .map(|(index, word)| match index {
                0 => word.to_owned(),
                _ => word[..1].to_uppercase() + &word[1..],
            })
            .collect();
        let from = format!("'../../{dir}/frontend/feature.js'");
        let line = format!("import {{ feature as {local} }} from {from};\n");
        // As Prettier writes an import longer than the line.
        imports += &if line.len() > 101 {
            format!("import {{\n  feature as {local},\n}} from {from};\n")
        } else {
            line
        };
        list += &format!("  {local},\n");
    }
    format!(
        "// Generated by `npm run contracts:generate` from the app's registry and its FRONTEND.\n\
         import type {{ FrontendFeature }} from '../../core/shell/frontend/runtime/slots.js';\n\
         {imports}\n\
         // Every owner's frontend manifest: the features, the collectors, then core's parts with screens.\n\
         // The order is the sidebar's, and the order in which Settings tabs warm their reads.\n\
         export const features: readonly FrontendFeature[] = [\n\
         {list}];\n"
    )
}

#[test]
fn the_feature_map_and_the_frontend_list_are_the_registrys() {
    crate::install();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|dir| dir.join("Cargo.lock").is_file())
        .expect("repository root");
    for (file, text) in [
        (FEATURE_MAP, feature_map(root)),
        (FRONTEND_LIST, frontend_list(root)),
    ] {
        let path = root.join(file);
        if std::env::var_os("DISPATCH_UPDATE_CONTRACTS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &text).unwrap();
        }
        assert!(
            std::fs::read_to_string(&path).ok() == Some(text),
            "{file} is out of date: run `npm run contracts:generate`"
        );
    }
}
