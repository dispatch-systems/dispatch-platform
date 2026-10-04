//! The TypeScript the owners' API types are written to, each in its owner's api/generated/,
//! and the access catalog's, in tenancy's, checked against the files committed there.
use dispatch_core::accounts::api::types::*;
use dispatch_core::collection::api::{jobs::*, metrics::*, types::*};
use dispatch_core::collection::registry::Provider;
use dispatch_core::foundation::config::{Environment, ProviderMode};
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
#[cfg(feature = "scorecard")]
use dispatch_scorecard::{
    ScorecardDatasetCount, ScorecardPublication, ScorecardWeek, ScorecardWeeks,
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
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use ts_rs::TS;

/// The access catalog's file: its types are tenancy's.
const ACCESS_CATALOG: &str = "core/tenancy/api/generated/access-catalog.ts";

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
    #[cfg(feature = "scorecard")]
    bindings.extend(exported!(
        &cfg,
        ScorecardDatasetCount,
        ScorecardPublication,
        ScorecardWeek,
        ScorecardWeeks,
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
    bindings.insert(ACCESS_CATALOG.into(), access_catalog());
    bindings
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
        ("schedulesFeature", json!(catalog::schedules())),
        ("permissions", json!(*roles::PERMISSIONS)),
        ("permissionLabels", json!(labels)),
        ("permissionGroups", json!(groups)),
        ("impliedPermissions", json!(implied)),
    ];
    let mut text = "// Generated from backend feature, collector and permission catalogs.\n\
                    // Run `npm run contracts:generate` after changing them.\n"
        .to_owned();
    for (name, value) in constants {
        text.push_str(&format!(
            "export const {name} = {} as const;\n",
            serde_json::to_string_pretty(&value).unwrap()
        ));
    }
    text
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
#[test]
fn the_job_kinds_written_for_typescript_are_the_registered_ones() {
    crate::install();
    let kinds: Vec<_> = Provider::all()
        .flat_map(|p| p.job_kinds())
        .map(|kind| format!("{kind:?}"))
        .collect();
    let cfg = ts_rs::Config::new();
    assert!(
        PublicJob::export_to_string(&cfg)
            .unwrap()
            .contains(&kinds.join(" | "))
    );
}
