use super::*;

#[test]
fn a_bounded_query_is_interrupted_and_leaves_the_connection_reusable() {
    let db = Db(Connection::open_in_memory().unwrap());
    let error = db
        .all_bounded(
            "WITH RECURSIVE n(v) AS (VALUES(1) UNION ALL SELECT v+1 FROM n WHERE v<10000) \
             SELECT sum(a.v*b.v) total FROM n a CROSS JOIN n b",
            [],
            1_000,
        )
        .unwrap_err();
    assert!(error.is(crate::Code::QueryLimitExceeded));
    assert_eq!(n(&db.one("SELECT 1 n", []).unwrap().unwrap(), "n"), 1);
}
