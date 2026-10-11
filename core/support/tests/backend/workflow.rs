use super::api::types::{SupportEvidence, SupportProposal};
use crate::{
    accounts::Auth,
    db::{Store, now},
    foundation::config::Site,
    support::*,
    testing,
};
use serde_json::json;

fn owner(db: &Store) -> Auth {
    Auth {
        user: db.active_platform_owners().unwrap().remove(0),
        hash: String::new(),
        csrf: String::new(),
        raw: String::new(),
        preview: None,
        site: Site::Admin,
        scope: None,
    }
}
fn reporter(db: &Store, dsp: &str, email: &str) -> Auth {
    let user = db
        .create_user(
            Some(dsp),
            email,
            "Alex",
            "Reporter",
            "an-example-test-password",
        )
        .unwrap();
    Auth {
        user,
        scope: Some(dsp.into()),
        ..owner(db)
    }
}
fn report(db: &Store, auth: &Auth) -> api::types::SupportDetail {
    tickets::create(db, auth, &json!({
        "requestId":"a-valid-client-idempotency-key", "kind":"bug", "title":"Selection disappears",
        "description":"The selected record disappears after refresh.","attachmentIds":[]
    })).unwrap()
}
fn proposal() -> SupportProposal {
    SupportProposal {
        version: 0,
        source_sha: "a".repeat(40),
        reproduction: "Reproduced after refresh".into(),
        observations: vec!["Selection disappeared".into()],
        uncertainties: vec![],
        analysis: "Selection was stored only in the component".into(),
        plan: vec!["Persist the selected id".into()],
        alternatives: vec![],
        acceptance_criteria: vec!["Selection survives refresh".into()],
        verification: vec!["Select a record and refresh".into()],
        affected_areas: vec!["Record selection".into()],
        production_verification: vec!["Selection control is available".into()],
        risks: vec![],
        created_at: 0,
    }
}
fn approved(db: &Store, auth: &Auth) -> api::types::SupportJob {
    let ticket = report(db, auth);
    let job = jobs::claim(db).unwrap().unwrap();
    proposals::publish(db, &job.id, &job.lease, proposal()).unwrap();
    verification::complete(db, &job.id, &job.lease, &json!({})).unwrap();
    approvals::decide(db, &owner(db), &ticket.ticket.id, 1, "approve", "").unwrap();
    jobs::claim(db).unwrap().unwrap()
}
fn proof(db: &Store, job: &api::types::SupportJob, head: &str, phase: &str, passed: bool) {
    verification::record(
        db,
        &job.id,
        &job.lease,
        SupportEvidence {
            id: String::new(),
            phase: phase.into(),
            source_sha: head.into(),
            criterion: if phase == "release" {
                "Selection control is available"
            } else {
                "Selection survives refresh"
            }
            .into(),
            steps: vec![
                "Select the record".into(),
                "Refresh and observe the selected record".into(),
            ],
            outcome: "The record remains selected after refreshing".into(),
            passed,
            attachment_ids: vec![],
            created_at: 0,
        },
    )
    .unwrap();
}

#[test]
fn submission_is_atomic_private_and_uses_authenticated_reporter_metadata() {
    testing::install(&[], &[]);
    let (_root, db, dsp) = testing::bootstrapped();
    let a = reporter(&db, &dsp, "alex@example.test");
    let b = reporter(&db, &dsp, "other@example.test");
    let detail = report(&db, &a);
    assert_eq!(detail.ticket.reporter_name, "Alex Reporter");
    assert_eq!(
        detail.ticket.dsp_name,
        Some(db.find_dsp(&dsp).unwrap().name)
    );
    assert_eq!(detail.ticket.dsp_id.as_deref(), Some(dsp.as_str()));
    assert_eq!(report(&db, &a).ticket.id, detail.ticket.id);
    assert_eq!(
        db.platform
            .count("SELECT count(*) FROM support_jobs", [])
            .unwrap(),
        1
    );
    assert!(tickets::detail(&db, &b, &detail.ticket.id).is_err());
    assert!(diagnostic_dsp(&db, &detail.ticket.id, &dsp, &b.user.id).is_err());
    assert!(
        tickets::create(
            &db,
            &a,
            &json!({
                "requestId":"another-valid-client-key", "kind":"bug", "title":"A valid report",
                "description":"A complete description", "attachmentIds":[], "page":"/irrelevant"
            }),
        )
        .is_err()
    );
    conversations::reply(
        &db,
        &owner(&db),
        &detail.ticket.id,
        "Why this approach?",
        "agent",
    )
    .unwrap();
    assert!(
        tickets::detail(&db, &a, &detail.ticket.id)
            .unwrap()
            .messages
            .is_empty()
    );
    assert_eq!(
        tickets::detail(&db, &owner(&db), &detail.ticket.id)
            .unwrap()
            .messages
            .len(),
        1
    );
    assert!(conversations::reply(&db, &a, &detail.ticket.id, "Private", "agent").is_err());
}

#[test]
fn an_invalid_or_someone_elses_attachment_rolls_back_the_whole_submission() {
    testing::install(&[], &[]);
    let (_root, db, dsp) = testing::bootstrapped();
    let a = reporter(&db, &dsp, "first@example.test");
    let b = reporter(&db, &dsp, "second@example.test");
    let image = attachments::save(&db, &a, "shot.png", b"\x89PNG\r\n\x1a\nexample").unwrap();
    assert!(attachments::save(&db, &a, "script.svg", b"<svg onload='x'/>").is_err());
    assert!(attachments::read(&db, &b, &image.id).is_err());
    assert!(
        tickets::create(
            &db,
            &b,
            &json!({"requestId":"another-valid-client-key","kind":"bug",
        "title":"Example","description":"Details","attachmentIds":[image.id]})
        )
        .is_err()
    );
    assert_eq!(
        db.platform
            .count("SELECT count(*) FROM support_tickets", [])
            .unwrap(),
        0
    );
    assert_eq!(
        db.platform
            .count("SELECT count(*) FROM support_jobs", [])
            .unwrap(),
        0
    );
    assert!(attachments::read(&db, &a, &image.id).is_ok());
}

#[test]
fn intake_finishes_atomically_without_a_proposal_and_clarifications_return_to_routing() {
    testing::install(&[], &[]);
    let (_root, db, dsp) = testing::bootstrapped();
    let member = reporter(&db, &dsp, "intake@example.test");
    let platform_owner = owner(&db);
    for (outcome, kind, auth) in [
        ("acknowledged", "bug", &member),
        ("answered", "feature", &member),
        ("unclear_bug", "bug", &member),
        ("unclear_feature", "feature", &platform_owner),
    ] {
        let ticket = tickets::create(
            &db,
            auth,
            &json!({
                "requestId":format!("adaptive-intake-{outcome}-request"),"kind":kind,
                "title":"A short request","description":"Please help","attachmentIds":[]
            }),
        )
        .unwrap();
        assert_eq!(ticket.ticket.status, "triaging");
        let mut job = jobs::claim(&db).unwrap().unwrap();
        if outcome == "acknowledged" {
            conversations::reply(
                &db,
                auth,
                &ticket.ticket.id,
                "This is just a test",
                "reporter",
            )
            .unwrap();
            assert_eq!(
                jobs::heartbeat(&db, &job.id, &job.lease, &json!({}))
                    .unwrap_err()
                    .code,
                "support_lease_lost"
            );
            job = jobs::claim(&db).unwrap().unwrap();
            assert_eq!(job.checkpoint["phase"], "routing");
        }
        let unclear = outcome.starts_with("unclear_");
        let result = json!({
            "outcome":outcome,"reply":"A helpful response for the reporter",
            "reason":"Private intake decision context",
            "findings":if unclear {vec!["Read-only inspection could not identify the intended outcome"]} else {vec![]},
            "questions":if unclear {vec!["Which page and result did you intend?"]} else {vec![]},
            "investigationAttempted":unclear
        });
        assert_eq!(
            verification::complete(&db, &job.id, "stale-lease", &result)
                .unwrap_err()
                .code,
            "support_lease_lost"
        );
        if unclear {
            let mut invalid = result.clone();
            invalid["investigationAttempted"] = json!(false);
            assert_eq!(
                verification::complete(&db, &job.id, &job.lease, &invalid)
                    .unwrap_err()
                    .code,
                "support_intake_incomplete"
            );
            invalid = result.clone();
            invalid["outcome"] = json!(if kind == "bug" {
                "unclear_feature"
            } else {
                "unclear_bug"
            });
            assert_eq!(
                verification::complete(&db, &job.id, &job.lease, &invalid)
                    .unwrap_err()
                    .code,
                "support_intake_incomplete"
            );
            assert!(
                tickets::detail(&db, auth, &ticket.ticket.id)
                    .unwrap()
                    .messages
                    .is_empty()
            );
        }
        let mut invalid = result.clone();
        invalid["approve"] = json!(true);
        assert_eq!(
            verification::complete(&db, &job.id, &job.lease, &invalid)
                .unwrap_err()
                .code,
            "invalid_input"
        );
        verification::complete(&db, &job.id, &job.lease, &result).unwrap();
        let detail = tickets::detail(&db, auth, &ticket.ticket.id).unwrap();
        assert_eq!(detail.ticket.status, outcome);
        assert_eq!(detail.ticket.proposal_version, 0);
        assert_eq!(detail.ticket.approved_version, None);
        assert!(detail.proposals.is_empty() && detail.evidence.is_empty());
        let public: Vec<_> = detail
            .messages
            .iter()
            .filter(|message| !message.private && message.role == "agent")
            .collect();
        assert_eq!(public.len(), 1);
        assert_eq!(public[0].role, "agent");
        assert_eq!(public[0].body, "A helpful response for the reporter");
        if !auth.user.platform_owner {
            assert!(detail.messages.iter().all(|message| !message.private));
            assert!(detail.intake.is_none());
        }
        let owner_detail = tickets::detail(&db, &platform_owner, &ticket.ticket.id).unwrap();
        assert_eq!(
            owner_detail.intake.as_ref().unwrap().reason,
            "Private intake decision context"
        );
        assert!(notifications::unread(&db, auth).unwrap() > 0);
        assert!(jobs::claim(&db).unwrap().is_none());
        assert_eq!(
            approvals::decide(&db, &platform_owner, &ticket.ticket.id, 0, "approve", "")
                .unwrap_err()
                .code,
            "support_proposal_changed"
        );
        assert_eq!(
            verification::complete(&db, &job.id, &job.lease, &result)
                .unwrap_err()
                .code,
            "support_lease_lost"
        );
        conversations::reply(&db, auth, &ticket.ticket.id, "Thanks", "reporter").unwrap();
        let thanks = jobs::claim(&db).unwrap().unwrap();
        assert_eq!(thanks.kind, "investigate");
        assert_eq!(thanks.checkpoint["phase"], "routing");
        assert_eq!(thanks.checkpoint["previousStatus"], outcome);
        // Routing checkpoints can advance without losing the previous durable unclear review.
        jobs::heartbeat(&db, &thanks.id, &thanks.lease, &json!({"phase":"triaged"})).unwrap();
        verification::complete(
            &db,
            &thanks.id,
            &thanks.lease,
            &json!({
                "outcome":"acknowledged","reply":"You are welcome","reason":"A thanks reply",
                "findings":[],"questions":[],"investigationAttempted":false
            }),
        )
        .unwrap();
        let after = tickets::detail(&db, &platform_owner, &ticket.ticket.id).unwrap();
        assert_eq!(
            after.ticket.status,
            if unclear { outcome } else { "acknowledged" }
        );
        if unclear {
            let original = owner_detail.intake.unwrap();
            let retained = after.intake.unwrap();
            assert_eq!(retained.reason, original.reason);
            assert_eq!(retained.findings, original.findings);
            assert_eq!(retained.questions, original.questions);
        }
        assert!(jobs::claim(&db).unwrap().is_none());
        conversations::reply(&db, auth, &ticket.ticket.id, "One more thing", "reporter").unwrap();
        let superseded = jobs::claim(&db).unwrap().unwrap();
        conversations::reply(
            &db,
            auth,
            &ticket.ticket.id,
            "The selection disappears on refresh",
            "reporter",
        )
        .unwrap();
        assert_eq!(
            jobs::heartbeat(&db, &superseded.id, &superseded.lease, &json!({}))
                .unwrap_err()
                .code,
            "support_lease_lost"
        );
        let clarified = jobs::claim(&db).unwrap().unwrap();
        assert_eq!(clarified.checkpoint["phase"], "routing");
        assert_ne!(
            clarified.checkpoint["replyId"],
            superseded.checkpoint["replyId"]
        );
        assert_eq!(clarified.checkpoint["previousStatus"], after.ticket.status);
        assert_eq!(clarified.detail.messages.last().unwrap().role, "reporter");
        assert_eq!(
            clarified.detail.messages.last().unwrap().body,
            "The selection disappears on refresh"
        );
        jobs::progress(
            &db,
            &clarified.id,
            &clarified.lease,
            "investigating",
            "Inspecting the source",
            &json!({}),
        )
        .unwrap();
        proposals::publish(&db, &clarified.id, &clarified.lease, proposal()).unwrap();
        verification::complete(&db, &clarified.id, &clarified.lease, &json!({})).unwrap();
        let planned = tickets::detail(&db, &platform_owner, &ticket.ticket.id).unwrap();
        assert_eq!(planned.ticket.status, "awaiting_approval");
        assert_eq!(planned.ticket.proposal_version, 1);
        assert!(planned.intake.is_none());
    }
}

#[test]
fn approval_is_version_bound_and_a_revised_proposal_revokes_running_implementation() {
    testing::install(&[], &[]);
    let (_root, db, dsp) = testing::bootstrapped();
    let a = reporter(&db, &dsp, "reporter@example.test");
    let job = approved(&db, &a);
    assert_eq!(job.version, 1);
    assert_eq!(job.kind, "implement");
    assert_eq!(
        verification::complete(
            &db,
            &job.id,
            &job.lease,
            &json!({
                "outcome":"acknowledged","reply":"Hello","reason":"A greeting",
                "findings":[],"questions":[],"investigationAttempted":false
            })
        )
        .unwrap_err()
        .code,
        "support_merge_unconfirmed"
    );
    assert!(approvals::decide(&db, &a, &job.detail.ticket.id, 1, "approve", "").is_err());
    let mut incomplete = proposal();
    incomplete.production_verification = vec![" ".into()];
    assert_eq!(
        proposals::publish(&db, &job.id, &job.lease, incomplete)
            .unwrap_err()
            .code,
        "support_proposal_incomplete"
    );
    let revised = proposals::publish(&db, &job.id, &job.lease, proposal()).unwrap();
    assert_eq!(revised.version, 2);
    assert!(jobs::heartbeat(&db, &job.id, &job.lease, &json!({})).is_err());
    assert_eq!(
        approvals::decide(&db, &owner(&db), &job.detail.ticket.id, 1, "approve", "")
            .unwrap_err()
            .code,
        "support_proposal_changed"
    );
    let accepted =
        approvals::decide(&db, &owner(&db), &job.detail.ticket.id, 2, "approve", "").unwrap();
    assert_eq!(accepted.ticket.approved_version, Some(2));
    approvals::decide(&db, &owner(&db), &job.detail.ticket.id, 2, "approve", "").unwrap();
    assert_eq!(
        db.platform
            .count(
                "SELECT count(*) FROM support_jobs WHERE kind='implement' AND version=2",
                []
            )
            .unwrap(),
        1
    );
}

#[test]
fn restart_preserves_the_job_checkpoint_and_invalidates_the_expired_worker_lease() {
    testing::install(&[], &[]);
    let (_root, db, _) = testing::bootstrapped();
    report(&db, &owner(&db));
    let first = jobs::claim(&db).unwrap().unwrap();
    jobs::heartbeat(
        &db,
        &first.id,
        &first.lease,
        &json!({"phase":"browser","session":"isolated"}),
    )
    .unwrap();
    db.platform
        .exec(
            "UPDATE support_jobs SET lease_until=? WHERE id=?",
            rusqlite::params![now() - 1, first.id],
        )
        .unwrap();
    let second = jobs::claim(&db).unwrap().unwrap();
    assert_eq!(second.id, first.id);
    assert_eq!(second.checkpoint["phase"], "browser");
    assert_ne!(second.lease, first.lease);
    assert!(jobs::heartbeat(&db, &first.id, &first.lease, &json!({})).is_err());
    assert!(jobs::claim(&db).unwrap().is_none());
}

#[test]
fn merge_and_resolution_require_current_commit_browser_proof_and_are_separate_states() {
    testing::install(&[], &[]);
    let (_root, db, dsp) = testing::bootstrapped();
    let a = reporter(&db, &dsp, "reporter@example.test");
    let job = approved(&db, &a);
    let head = "b".repeat(40);
    let pr = "https://github.com/dispatch-systems/dispatch-platform/pull/999";
    assert!(verification::github(&db, &job.id, &job.lease, pr, &head).is_err());
    proof(&db, &job, &"c".repeat(40), "verification", true);
    assert!(verification::github(&db, &job.id, &job.lease, pr, &head).is_err());
    proof(&db, &job, &head, "verification", true);
    proof(&db, &job, &head, "verification", false);
    assert!(verification::github(&db, &job.id, &job.lease, pr, &head).is_err());
    proof(&db, &job, &head, "verification", true);
    verification::github(&db, &job.id, &job.lease, pr, &head).unwrap();
    assert_eq!(
        proposals::publish(&db, &job.id, &job.lease, proposal())
            .unwrap_err()
            .code,
        "support_proposal_already_published"
    );
    assert_eq!(
        tickets::detail(&db, &owner(&db), &job.detail.ticket.id)
            .unwrap()
            .ticket
            .approved_version,
        Some(1)
    );
    let merge = "d".repeat(40);
    verification::complete(
        &db,
        &job.id,
        &job.lease,
        &json!({"prUrl":pr,"headSha":head,"mergeSha":merge}),
    )
    .unwrap();
    let detail = tickets::detail(&db, &a, &job.detail.ticket.id).unwrap();
    assert_eq!(detail.ticket.status, "merged");
    assert!(detail.proposals.is_empty() && detail.evidence.is_empty());
    let released = jobs::claim(&db).unwrap().unwrap();
    let result = json!({"release":"0.0.20","headSha":merge,"mergeSha":merge});
    assert!(verification::complete(&db, &released.id, &released.lease, &result).is_err());
    proof(&db, &released, &merge, "release", true);
    verification::complete(&db, &released.id, &released.lease, &result).unwrap();
    assert_eq!(
        tickets::detail(&db, &a, &job.detail.ticket.id)
            .unwrap()
            .ticket
            .status,
        "resolved"
    );
    assert!(notifications::unread(&db, &a).unwrap() > 0);
    tickets::reopen(&db, &a, &job.detail.ticket.id).unwrap();
    let reopened = jobs::claim(&db).unwrap().unwrap();
    assert_eq!(reopened.kind, "investigate");
    assert_eq!(reopened.detail.ticket.pr_url, None);
    assert_eq!(
        proposals::publish(&db, &reopened.id, &reopened.lease, proposal())
            .unwrap()
            .version,
        2
    );
}

#[test]
fn waiting_can_resume_without_an_attempt_limit_and_a_denial_cancels_work() {
    testing::install(&[], &[]);
    let (_root, db, _) = testing::bootstrapped();
    let auth = owner(&db);
    let job = approved(&db, &auth);
    jobs::wait(
        &db,
        &job.id,
        &job.lease,
        "Provider needs verification",
        &json!({"phase":"provider"}),
        None,
    )
    .unwrap();
    assert!(jobs::claim(&db).unwrap().is_none());
    approvals::resume(&db, &auth, &job.detail.ticket.id).unwrap();
    let resumed = jobs::claim(&db).unwrap().unwrap();
    assert_eq!(resumed.checkpoint["phase"], "provider");
    proposals::publish(&db, &resumed.id, &resumed.lease, proposal()).unwrap();
    approvals::decide(
        &db,
        &auth,
        &job.detail.ticket.id,
        2,
        "deny",
        "Outside the intended scope",
    )
    .unwrap();
    assert!(jobs::heartbeat(&db, &resumed.id, &resumed.lease, &json!({})).is_err());
    assert!(jobs::claim(&db).unwrap().is_none());
}

#[test]
fn read_only_browser_authority_is_scoped_to_the_active_job_and_reporters_host() {
    testing::install(&[], &[]);
    let (_root, db, _) = testing::bootstrapped();
    report(&db, &owner(&db));
    let job = jobs::claim(&db).unwrap().unwrap();
    let cookie = browser::issue(&db, &job.id, &job.lease).unwrap();
    let raw = cookie["cookie"]["value"].as_str().unwrap();
    let identity = db.authenticate(raw, &Site::Admin).unwrap();
    assert!(browser::read_only(&identity));
    assert_eq!(identity.user.id, owner(&db).user.id);
    assert!(db.authenticate(raw, &Site::Dsp("another".into())).is_err());
    jobs::wait(&db, &job.id, &job.lease, "Stopped", &json!({}), None).unwrap();
    assert!(db.authenticate(raw, &Site::Admin).is_err());
}
