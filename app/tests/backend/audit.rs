//! The activity log's areas as they stood before features declared the prefixes of their
//! actions: the area each action is listed and counted under, in the platform's log and
//! in a DSP's.
use crate::{config::Config, db::AuditQuery, db::Store};
use std::{collections::BTreeMap, os::unix::fs::PermissionsExt};

/// An action, then its area in the platform's log and in its DSP's.
const AREAS: &[(&str, &str, &str)] = &[
    ("member.invited", "team", "team"),
    ("invitation.revoked", "team", "team"),
    ("role.created", "roles", "roles"),
    ("collection.completed", "collections", "collections"),
    ("collection.failed", "collections", "collections"),
    ("cortex.collection.requested", "collections", "collections"),
    ("meal_breaks.sync_requested", "collections", "collections"),
    ("dvic.collection_requested", "collections", "collections"),
    ("dvic.driver_hidden", "collections", "collections"),
    // SQL's LIKE ignores ASCII case and reads `_` as any one character.
    ("DVIC.Upper", "collections", "collections"),
    ("mealxbreaks.wildcard", "collections", "collections"),
    ("cortexxcollection.dotted", "settings", "settings"),
    ("cortex.other", "settings", "settings"),
    ("meal_breaks", "settings", "settings"),
    ("schedule.created", "schedules", "schedules"),
    ("connection.connected", "connections", "connections"),
    ("dsp.view_opened", "access", "team"),
    ("dsp.owner_view_opened", "access", "team"),
    ("account.password_changed", "access", "access"),
    ("agent.key_created", "access", "access"),
    ("dsp.created", "dsps", "dsps"),
    ("dsp.removed", "dsps", "dsps"),
    ("dsp.restored", "dsps", "dsps"),
    ("dsp.suspended", "dsps", "dsps"),
    ("dsp.resumed", "dsps", "dsps"),
    ("dsp.feature_enabled", "dsps", "dsps"),
    ("dsp.feature_disabled", "dsps", "dsps"),
    ("dsp.support_visibility_changed", "settings", "settings"),
    ("routes.collection_requested", "settings", "settings"),
    ("scorecard.collection_requested", "settings", "settings"),
    ("driver_match.merged", "settings", "settings"),
    ("driver_match.failed", "settings", "settings"),
    ("uniform.adjusted", "settings", "settings"),
    ("profile.updated", "settings", "settings"),
    ("development.fixtures_loaded", "settings", "settings"),
    ("diagnostics.fixtures_loaded", "settings", "settings"),
];

#[test]
fn each_action_keeps_its_area() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = Config::load().unwrap();
    config.root = root.path().into();
    let store = Store::initialize(config).unwrap();
    let dsp = format!("dsp_{}", "a".repeat(32));
    store
        .platform
        .exec(
            "INSERT INTO dsps(id,name,environment,status,timezone,created_at) \
             VALUES (?,'Audit','preview','active','UTC','2026-01-01')",
            [&dsp],
        )
        .unwrap();
    for (action, ..) in AREAS {
        store
            .platform
            .exec(
                "INSERT INTO audit(at,dsp_id,action,detail) VALUES ('2026-01-01T00:00:00.000Z',?,?,'')",
                [&dsp, *action],
            )
            .unwrap();
    }
    for (index, within) in [None, Some(dsp.as_str())].into_iter().enumerate() {
        let query = AuditQuery {
            dsp: within,
            limit: 1000,
            ..AuditQuery::default()
        };
        let page = serde_json::to_value(store.audit_page(&query).unwrap()).unwrap();
        let found: BTreeMap<String, String> = page["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| {
                (
                    event["action"].as_str().unwrap().into(),
                    event["area"].as_str().unwrap().into(),
                )
            })
            .collect();
        let area =
            |&(_, platform, own): &(&str, &'static str, &'static str)| [platform, own][index];
        let expected: BTreeMap<String, String> = AREAS
            .iter()
            .map(|entry| (entry.0.into(), area(entry).into()))
            .collect();
        assert_eq!(found, expected, "{within:?}");
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for entry in AREAS {
            *counts.entry(area(entry).into()).or_default() += 1;
        }
        let failures = AREAS
            .iter()
            .filter(|(action, ..)| action.ends_with(".failed"))
            .count();
        counts.insert("failures".into(), failures);
        let counted: BTreeMap<String, usize> =
            serde_json::from_value(page["counts"].clone()).unwrap();
        assert_eq!(counted, counts, "{within:?}");
    }
}
