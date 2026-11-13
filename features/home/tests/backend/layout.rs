use super::LAYOUT;
use serde_json::json;

#[test]
fn a_layout_is_kept_only_when_each_widget_fits_the_grid_once() {
    let placed =
        |x: u8, w: u8, h: u8| json!({"id": "vans.overview", "x": x, "y": 0, "w": w, "h": h});
    let kept = |widgets: serde_json::Value| LAYOUT.keeps(&json!({ "widgets": widgets })).is_some();
    assert!(kept(json!([])));
    assert!(kept(
        json!([placed(9, 3, 3), {"id": "dvic.today", "x": 0, "y": 3, "w": 12, "h": 6}])
    ));
    for refused in [
        json!([placed(10, 3, 3)]),
        json!([placed(0, 0, 3)]),
        json!([placed(0, 3, 0)]),
        json!([placed(0, 3, 25)]),
        json!([placed(0, 3, 3), placed(6, 3, 3)]),
        json!([{"id": "", "x": 0, "y": 0, "w": 3, "h": 3}]),
        json!([{"id": "vans.overview", "x": 0, "y": 1001, "w": 3, "h": 3}]),
    ] {
        assert!(!kept(refused.clone()), "{refused} was kept");
    }
}
