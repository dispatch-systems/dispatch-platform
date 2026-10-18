//! What Weekly Scorecard exposes, as a feature using it finds it: only while it's on for the
//! DSP, and only the weeks its own reads show.
use crate::WeeklyScorecardStore;
use dispatch_core::{
    db::{Store, s},
    testing,
};
use dispatch_cortex::{
    discovery::CollectionRequest,
    weekly_scorecard::{self, Request},
};
use dispatch_weekly_scorecard_contract::{self as scorecards, Collected, PackageKind};
use serde_json::json;

/// Weekly Scorecard and the Cortex collector whose scorecard it keeps, and a DSP with a station
/// and a ready Cortex connection.
fn ready() -> (tempfile::TempDir, Store, String) {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (root, db, dsp) = testing::bootstrapped();
    testing::set_dsp(&db, &dsp, "Fixture Delivery", "America/Los_Angeles").unwrap();
    db.set_profile(
        &dsp,
        json!({"stationCode":"TST1","abbreviation":"FXTR","setupRequired":false}),
    )
    .unwrap();
    testing::ready_connection(&db, &dsp, dispatch_cortex::PROVIDER).unwrap();
    (root, db, dsp)
}
/// Publishes the fixture's `week` as the outcome of a job queued as `key`.
fn publish(db: &Store, dsp: &str, key: &str, week: &str) {
    let request = Request::parse(
        &json!({"collection":"weekly_scorecard","week":week,"station":"TST1",
            "timezone":"America/Los_Angeles","dspName":"Fixture Delivery","dspAbbreviation":"FXTR"}),
    )
    .unwrap()
    .unwrap();
    let CollectionRequest::Discover(discovery) = request.scope_request() else {
        unreachable!("a week's scorecard is discovered");
    };
    let scope = discovery.scope("area-fixture", "company-fixture").unwrap();
    let job = db
        .enqueue_weekly_scorecard(dsp, None, key, Some(week))
        .unwrap();
    let capture = weekly_scorecard::fixture(&request).unwrap();
    db.publish_weekly_scorecard(dsp, s(&job, "id"), &capture, &scope)
        .unwrap();
}
fn ids(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn driver_scorecards_reads_the_drivers_asked_for_in_the_weeks_shown_only_while_on() {
    let (_root, db, dsp) = ready();
    let offered = scorecards::for_dsp(&db, &dsp)
        .unwrap()
        .expect("Weekly Scorecard is on");
    assert!(offered.source() == dispatch_cortex::SOURCE);
    assert_eq!(offered.collected(&db, &dsp).unwrap(), None);
    publish(&db, &dsp, "w37", "2026-W37");
    publish(&db, &dsp, "w38", "2026-W38");
    // Collected again, a week is read from its newest publication alone.
    publish(&db, &dsp, "w38-again", "2026-W38");
    // A legacy publication of the week before, its scope unverified, stays quarantined.
    let storage = db.weekly_scorecard_db(&dsp).unwrap();
    storage
        .exec(
            "INSERT INTO weekly_scorecard_publications(id,job_id,week,station,company_id,dsp_code,\
             started_at,collected_at,active,row_count,adapter_version) VALUES \
             ('legacy','legacy-job','2026-W36','TST1','company-foreign','FOREIGN',\
             '2026-01-01T00:00:00Z','2026-01-01T00:01:00Z',1,1,1)",
            [],
        )
        .unwrap();
    storage
        .exec(
            "INSERT INTO driver_weekly_scorecards(publication_id,row_index,week,transporter_id,row) \
             VALUES ('legacy',0,'2026-W36','driver-2','{\"da_name\":\"Foreign\"}')",
            [],
        )
        .unwrap();
    assert_eq!(
        offered.collected(&db, &dsp).unwrap(),
        Some(Collected {
            from: "2026-09-06".into(),
            through: "2026-09-19".into(),
            weeks: ids(&["2026-W37", "2026-W38"]),
        })
    );

    // Three weeks overlap the days asked: the first, never collected, is unknown and read as none.
    let found = offered
        .scorecards(
            &db,
            &dsp,
            &ids(&["driver-2", "someone-else"]),
            "2026-08-30",
            "2026-09-13",
        )
        .unwrap();
    assert_eq!(
        found
            .iter()
            .map(|card| (card.week.as_str(), card.driver_id.as_str()))
            .collect::<Vec<_>>(),
        [("2026-W37", "driver-2"), ("2026-W38", "driver-2")]
    );
    let card = &found[1];
    assert_eq!(
        (
            card.from.as_str(),
            card.through.as_str(),
            card.name.as_str()
        ),
        ("2026-09-13", "2026-09-19", "Fixture Driver 2")
    );
    assert_eq!(
        (
            card.overall_score,
            card.overall_tier.as_deref(),
            card.delivered
        ),
        (Some(89.0), Some("Fantastic"), Some(1001))
    );
    let cdf = card.metrics.iter().find(|m| m.name == "cdf_dpmo").unwrap();
    assert_eq!(cdf.value, Some(100.0));
    // A collected week without them is empty.
    assert!(
        offered
            .scorecards(
                &db,
                &dsp,
                &ids(&["someone-else"]),
                "2026-09-13",
                "2026-09-19"
            )
            .unwrap()
            .is_empty()
    );

    // A package counts in its scorecard week, though its own day is outside the days asked.
    let packages = offered
        .packages(
            &db,
            &dsp,
            &ids(&["driver-1", "driver-3"]),
            "2026-09-19",
            "2026-09-19",
        )
        .unwrap();
    assert!(packages.iter().all(|package| package.week == "2026-W38"));
    assert_eq!(
        packages
            .iter()
            .map(|p| (
                p.kind,
                p.tracking_id.as_str(),
                p.driver_id.as_str(),
                p.date.as_deref(),
                p.impacting,
                p.reasons.clone()
            ))
            .collect::<Vec<_>>(),
        [
            (
                PackageKind::Feedback,
                "TBA000000000004",
                "driver-1",
                None,
                Some(true),
                ids(&["driver_was_unprofessional"])
            ),
            (
                PackageKind::Concession,
                "TBA000000000003",
                "driver-3",
                Some("2026-09-13"),
                Some(true),
                ids(&[])
            ),
            (
                PackageKind::Return,
                "TBA000000000001",
                "driver-1",
                Some("2026-09-13"),
                Some(true),
                ids(&["BUSINESS CLOSED"])
            ),
        ]
    );

    let owner = testing::a_user(&db);
    db.set_feature(&dsp, "weekly_scorecard", false, &owner)
        .unwrap();
    assert!(scorecards::for_dsp(&db, &dsp).unwrap().is_none());
}

#[test]
fn driver_scorecards_refuses_bad_days_and_ids() {
    let (_root, db, dsp) = ready();
    let offered = scorecards::for_dsp(&db, &dsp)
        .unwrap()
        .expect("Weekly Scorecard is on");
    let many: Vec<String> = (0..33).map(|n| format!("driver-{n}")).collect();
    for (asked, from, to, code) in [
        (
            ids(&["driver-1"]),
            "2026-9-13",
            "2026-09-19",
            "invalid_date",
        ),
        (
            ids(&["driver-1"]),
            "2026-09-19",
            "2026-09-13",
            "invalid_input",
        ),
        (
            ids(&["driver-1"]),
            "2025-09-18",
            "2026-09-19",
            "invalid_input",
        ),
        (
            ids(&["driver 1"]),
            "2026-09-13",
            "2026-09-19",
            "invalid_input",
        ),
        (many, "2026-09-13", "2026-09-19", "invalid_input"),
    ] {
        let refused = offered.scorecards(&db, &dsp, &asked, from, to).unwrap_err();
        assert_eq!(refused.code, code, "{from} to {to}");
        let refused = offered.packages(&db, &dsp, &asked, from, to).unwrap_err();
        assert_eq!(refused.code, code, "{from} to {to}");
    }
    // 366 days are the most one read covers.
    let asked = ids(&["driver-1"]);
    assert!(
        offered
            .scorecards(&db, &dsp, &asked, "2025-09-19", "2026-09-19")
            .is_ok()
    );
}
