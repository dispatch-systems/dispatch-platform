use super::*;
use dispatch_core::testing as common;
use serde_json::json;

#[test]
fn sql_normalization_preserves_source_flags_and_unicode_codes() {
    common::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, id) = common::bootstrapped();
    let dsp = db.find_dsp(&id).unwrap();
    let data = database(&db, &dsp).unwrap();
    let sql = format!(
        "SELECT {} flag,{} code FROM (SELECT ? row) x",
        yes("flag"),
        normalized("code")
    );
    for (flag, expected) in [
        (json!(true), 1),
        (json!(false), 0),
        (json!(1), 1),
        (json!(1.0), 0),
        (json!(" YES "), 1),
        (json!("y"), 1),
        (json!("true"), 1),
        (json!("N"), 0),
        (Value::Null, 0),
        (json!({"yes":true}), 0),
    ] {
        let row = data
            .one(
                &sql,
                [json!({"flag":flag,"code":"  BÜSINESS / / CLOSED- "}).to_string()],
            )
            .unwrap()
            .unwrap();
        assert_eq!(row["flag"], expected, "{flag}");
        assert_eq!(row["code"], "büsiness_closed");
    }
}
