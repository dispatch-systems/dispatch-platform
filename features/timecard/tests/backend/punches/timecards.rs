use dispatch_core::db::Db;
use std::os::unix::fs::PermissionsExt;

fn private() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    root
}

#[test]
fn employee_history_uses_the_code_index() {
    crate::testing::install(
        &[&dispatch_paycom::COLLECTOR, &dispatch_cortex::COLLECTOR],
        &[
            &dispatch_driver_match::FEATURE,
            &crate::feature_manifests::timecard::FEATURE,
        ],
    );
    let root = private();
    let file = root.path().join("paycom.sqlite");
    let db = Db::create(&file, dispatch_paycom::DATABASE, "").unwrap();
    let plan = db
        .all(
            "EXPLAIN QUERY PLAN SELECT p.id FROM publications p \
        JOIN employees e ON e.publication_id=p.id WHERE e.code='E001' \
        ORDER BY p.period_to DESC LIMIT 1",
            [],
        )
        .unwrap();
    assert!(
        plan.iter().any(|row| row["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("employees_by_code")),
        "{plan:?}"
    );
    assert!(
        !plan.iter().any(|row| row["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("SCAN e")),
        "{plan:?}"
    );
}
