use super::*;
use std::{collections::BTreeMap, path::PathBuf};
use ts_rs::TS;

macro_rules! exported {
    ($cfg:expr, $($ty:ty),* $(,)?) => {
        BTreeMap::from([$((
            <$ty>::output_path().expect("named type"),
            <$ty>::export_to_string($cfg).expect("exportable type")
                .lines().map(|line| format!("{}\n", line.trim_end())).collect::<String>(),
        )),*])
    };
}
fn bindings() -> BTreeMap<PathBuf, String> {
    let cfg = ts_rs::Config::new();
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
        DriverLink,
        DriverStatus,
        DriverData,
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
        UniformFit,
        UniformEventKind,
        UniformVariant,
        Uniform,
        UniformInventory,
        UniformAdjustment,
        UniformUpdates,
        UniformEvent,
        UniformHistory,
        DspProfile,
        JobMetrics,
        JobPhase,
        JobOutcome,
        PageRead,
        PageReads,
        PageStage,
        DocumentState,
        PaycomPreferences,
        PaycomPage,
        PaycomSort,
        PaycomColumn,
        NameOrder,
        PreferenceRevision,
        DepartmentOption,
        PaycomOptions,
        PaycomSettings,
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
        EmployeeTimecardResponse,
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
        ScorecardDatasetCount,
        ScorecardPublication,
        ScorecardWeek,
        ScorecardWeeks,
        DvicReport,
        DvicWeek,
        DvicStatus,
        DvicInspection,
        DvicInspections,
        SessionResponse,
        SecurityStatus,
        PasskeySummary,
        AuthenticatorSetup,
        AccountSession,
    );
    bindings.insert("access-catalog.ts".into(), access_catalog());
    bindings
}

/// Runtime catalogs, exported from their actual values rather than Rust source formatting.
fn access_catalog() -> String {
    use crate::{features, roles};
    use serde_json::json;
    let catalog = features::catalog();
    let ids = |kind| {
        catalog
            .iter()
            .filter(|feature| match (feature.kind, kind) {
                (features::Kind::Tab(_), features::Kind::Tab(_)) => true,
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
                features::Kind::Page => entry["kind"] = json!("page"),
                features::Kind::Tab(page) => {
                    entry["kind"] = json!("tab");
                    entry["page"] = json!(page);
                }
                features::Kind::Connection => {
                    entry["kind"] = json!("connection");
                    entry["provides"] = json!(feature.provides);
                }
            }
            entry
        })
        .collect();
    let labels: BTreeMap<_, _> = roles::LABELS.iter().copied().collect();
    let implied: BTreeMap<_, _> = roles::IMPLIED.iter().copied().collect();
    let groups: Vec<_> = features::PAGES
        .iter()
        .map(|feature| (feature.label, feature.permissions))
        .chain(roles::GROUPS.iter().copied())
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
        ("pages", json!(ids(features::Kind::Page))),
        ("pageTabs", json!(ids(features::Kind::Tab("")))),
        ("connections", json!(ids(features::Kind::Connection))),
        (
            "features",
            json!(catalog.iter().map(|feature| feature.id).collect::<Vec<_>>()),
        ),
        ("featureCatalog", json!(entries)),
        ("schedulesFeature", json!(features::SCHEDULES)),
        ("permissions", json!(roles::PERMISSIONS)),
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
#[test]
fn typescript_contracts_match_the_rust_types() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|dir| dir.join("Cargo.lock").is_file())
        .expect("repository root")
        .join("shared/contracts/generated");
    let bindings = bindings();
    if std::env::var_os("DISPATCH_UPDATE_CONTRACTS").is_some() {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (file, text) in &bindings {
            std::fs::write(dir.join(file), text).unwrap();
        }
    }
    let mut stored = BTreeMap::new();
    for entry in std::fs::read_dir(&dir).expect("shared/contracts/generated") {
        let path = entry.unwrap().path();
        let name = PathBuf::from(path.file_name().unwrap());
        stored.insert(name, std::fs::read_to_string(&path).unwrap());
    }
    assert!(
        stored == bindings,
        "shared/contracts/generated is out of date: run `npm run contracts:generate`"
    );
}
#[test]
fn the_job_kinds_written_for_typescript_are_the_registered_ones() {
    let kinds: Vec<_> = Provider::ALL
        .iter()
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
