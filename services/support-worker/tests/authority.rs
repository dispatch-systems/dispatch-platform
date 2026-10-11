//! Contracts guarding queue admission and private service configuration.
use dispatch_support_worker::{
    config::{Config, Origin, ProductionReader, read_private},
    github::{admitted, redact_identities},
    hermes::{production_server, safe_job},
    intake::{Analysis, Decision},
    verification::cortex_collections,
};
use serde_json::json;
use std::path::PathBuf;

fn config() -> Config {
    Config {
        origins: vec![Origin {
            environment: "production".into(),
            url: "https://dispatch.example".into(),
            token_file: PathBuf::from("/private/support.token"),
        }],
        workspace_root: "/workspace".into(),
        runtime_root: "/runtime".into(),
        hermes_source: "/hermes/hermes-agent".into(),
        hermes_python: "/hermes/python".into(),
        repository: "dispatch-systems/dispatch-platform".into(),
        collection_bridge: "/runtime/collection".into(),
        node: "/usr/bin/node".into(),
        production_reader: None,
    }
}

#[test]
fn public_summary_cannot_carry_reporter_or_dsp_names_even_with_case_changes() {
    let value = redact_identities(
        "Fix Example Person's view in SAMPLE ORGANIZATION; İpek saw it.",
        &["Example Person", "Sample Organization", "İpek", ""],
    );
    assert_eq!(
        value,
        "Fix the affected account's view in the affected account; the affected account saw it."
    );
}

#[test]
fn verification_selects_native_datasets_for_feature_paths_and_shared_changes() {
    assert_eq!(
        cortex_collections("collectors/cortex/collections/meals/live.rs", &[]),
        vec!["meal_break"]
    );
    assert_eq!(
        cortex_collections(
            "collectors/cortex/collections/dvic/xlsx.rs",
            &["weekly-scorecard".into(), "Routes".into()]
        ),
        vec!["routes", "weekly_scorecard", "dvic"]
    );
    assert_eq!(
        cortex_collections("collectors/cortex/connection/session.rs", &[]).len(),
        5
    );
}

#[test]
fn service_authority_rejects_shared_files_and_symlinks() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root =
        std::env::temp_dir().join(format!("dispatch-support-authority-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("token");
    std::fs::write(&file, b"private test authority").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(read_private(&file).unwrap(), b"private test authority");
    symlink(&file, root.join("alias")).unwrap();
    assert!(read_private(&root.join("alias")).is_err());
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert!(read_private(&file).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn service_origin_cannot_redirect_worker_authority_or_expose_it_in_a_url() {
    let mut value = config();
    assert!(value.validate().is_ok());
    for origin in [
        "https://user:secret@dispatch.example",
        "http://dispatch.example",
        "https://dispatch.example/other",
        "https://dispatch.example?token=private",
        "https://dispatch.example#private",
    ] {
        value.origins[0].url = origin.into();
        assert!(value.validate().is_err(), "{origin}");
    }
}

#[test]
fn only_the_required_admission_success_permits_queuing() {
    for (name, conclusion, expected) in [
        ("platform", "SUCCESS", true),
        ("platform", "FAILURE", false),
        ("platform", "", false),
        ("some-unrelated-check", "SUCCESS", false),
    ] {
        let status = json!({"commits":{"nodes":[{"commit":{"statusCheckRollup":{"contexts":{"nodes":[{"name":name,"conclusion":conclusion}]}}}}]}});
        assert_eq!(admitted(&status), expected);
    }
    assert!(!admitted(&json!({})));
}

#[test]
fn reporter_model_payload_excludes_owner_private_context_but_owner_answers_retain_it() {
    let mut job: dispatch_core::support::api::types::SupportJob = serde_json::from_value(json!({
        "id":"job","kind":"investigate","version":0,"lease":"LEASE_CANARY","leaseUntil":0,
        "checkpoint":{"reason":"CHECKPOINT_CANARY"},"questionId":null,"environment":"dev","origin":"https://dispatch.example",
        "detail":{
            "ticket":{"id":"support_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","kind":"bug","title":"Public report","status":"triaging","progress":"","reporterName":"Demo reporter","dspId":null,"dspName":null,"proposalVersion":0,"approvedVersion":null,"prUrl":null,"headSha":null,"mergeSha":null,"releasedVersion":null,"createdAt":0,"updatedAt":0},
            "description":"PUBLIC_REPORT","release":"v1","sourceSha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "messages":[
                {"id":"public","author":"reporter","role":"reporter","body":"PUBLIC_REPLY","private":false,"createdAt":0},
                {"id":"owner","author":"owner","role":"owner","body":"OWNER_CANARY","private":true,"createdAt":0}
            ],
            "attachments":[{"id":"private","name":"ATTACHMENT_CANARY","mediaType":"image/png","size":0,"private":true}],
            "proposals":[{"version":1,"sourceSha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","reproduction":"","observations":[],"uncertainties":[],"analysis":"PROPOSAL_CANARY","plan":[],"alternatives":[],"acceptanceCriteria":[],"verification":[],"productionVerification":[],"affectedAreas":[],"risks":[],"createdAt":0}],
            "evidence":[{"id":"proof","phase":"investigation","sourceSha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","criterion":"","steps":[],"outcome":"EVIDENCE_CANARY","passed":false,"attachmentIds":[],"createdAt":0}],
            "intake":{"outcome":"answered","reply":"Public answer","reason":"INTAKE_CANARY","findings":[],"questions":[],"investigationAttempted":false}
        }
    })).unwrap();
    let public = safe_job(&job).unwrap().to_string();
    assert!(public.contains("PUBLIC_REPORT") && public.contains("PUBLIC_REPLY"));
    for canary in [
        "LEASE_CANARY",
        "CHECKPOINT_CANARY",
        "OWNER_CANARY",
        "ATTACHMENT_CANARY",
        "PROPOSAL_CANARY",
        "EVIDENCE_CANARY",
        "INTAKE_CANARY",
    ] {
        assert!(!public.contains(canary), "private field leaked: {canary}");
    }
    job.kind = "answer".into();
    let owner = safe_job(&job).unwrap().to_string();
    assert!(owner.contains("OWNER_CANARY") && owner.contains("PROPOSAL_CANARY"));
    assert!(!owner.contains("LEASE_CANARY") && !owner.contains("CHECKPOINT_CANARY"));
}

#[test]
fn production_server_reads_go_only_to_production_analysis_with_the_tickets_dsp() {
    let mut job: dispatch_core::support::api::types::SupportJob = serde_json::from_value(json!({
        "id":"job","kind":"investigate","version":0,"lease":"lease","leaseUntil":0,"checkpoint":{},
        "questionId":null,"environment":"production","origin":"https://dispatch.example",
        "detail":{
            "ticket":{"id":"support_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","kind":"bug","title":"Report","status":"triaging","progress":"","reporterName":"Reporter","dspId":"dsp_reported","dspName":"Reported DSP","proposalVersion":0,"approvedVersion":null,"prUrl":null,"headSha":null,"mergeSha":null,"releasedVersion":null,"createdAt":0,"updatedAt":0},
            "description":"","release":"v1","sourceSha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "messages":[],"attachments":[],"proposals":[],"evidence":[],"intake":null
        }
    })).unwrap();
    let mut config = config();
    assert!(production_server(&config, &job, "analyst", Some("source")).is_none());
    config.production_reader = Some(ProductionReader {
        host: "production.test".into(),
        user: "dispatch-reader".into(),
        key_file: "/runtime/production-reader".into(),
        known_hosts: "/runtime/production-reader.known_hosts".into(),
    });
    config.validate().unwrap();
    for stage in ["source", "browser"] {
        let grant = production_server(&config, &job, "analyst", Some(stage)).unwrap();
        assert_eq!(grant["dsp"], "dsp_reported");
        assert_eq!(grant["keyFile"], "/runtime/production-reader");
    }
    for (role, stage) in [
        ("analyst", Some("intake")),
        ("analyst", None),
        ("builder", None),
        ("reviewer", None),
    ] {
        assert!(
            production_server(&config, &job, role, stage).is_none(),
            "{role} {stage:?}"
        );
    }
    job.environment = "dev".into();
    assert!(production_server(&config, &job, "analyst", Some("source")).is_none());
    config.production_reader.as_mut().unwrap().host = "host;command".into();
    assert!(config.validate().is_err());
}

#[test]
fn unclear_intake_requires_real_investigation_and_matching_clarification_context() {
    let decision: Decision = serde_json::from_value(
        json!({"route":"acknowledge","reason":"Only a greeting","reply":"Hello!"}),
    )
    .unwrap();
    decision.validate().unwrap();
    assert_eq!(
        decision.completion().unwrap()["investigationAttempted"],
        false
    );
    assert!(
        serde_json::from_value::<Decision>(
            json!({"route":"unclear_bug","reason":"Too vague","reply":"What happened?"})
        )
        .is_err()
    );
    let mut analysis: Analysis = serde_json::from_value(json!({"nextStep":"finish","outcome":"unclear_bug","reason":"The affected action is still unidentified","reply":"Which action failed?","findings":["Read the relevant route module but no failing action was identified"],"questions":["Which route action failed?"],"proposal":null})).unwrap();
    assert!(analysis.validate("bug", false, false).is_err());
    assert!(analysis.validate("bug", true, false).is_err());
    assert!(analysis.validate("feature", true, true).is_err());
    analysis.validate("bug", true, true).unwrap();
    analysis.reply = "Please share more details.".into();
    let completion = analysis.completion(true);
    let public_reply = completion["reply"].as_str().unwrap();
    assert!(public_reply.contains("Which route action failed?"));
    assert!(!public_reply.contains(&analysis.reason));
    assert!(!public_reply.contains(&analysis.findings[0]));
    analysis.reply = public_reply.into();
    assert_eq!(
        analysis.completion(true)["reply"]
            .as_str()
            .unwrap()
            .matches("Which route action failed?")
            .count(),
        1
    );
    analysis.questions.clear();
    assert!(analysis.validate("bug", true, true).is_err());
}
