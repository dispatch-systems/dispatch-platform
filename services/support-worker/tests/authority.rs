//! Contracts guarding queue admission and private service configuration.
use dispatch_support_worker::{
    config::{Config, Origin, read_private},
    github::{admitted, redact_identities},
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
