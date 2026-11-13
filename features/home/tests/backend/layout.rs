use super::LAYOUT;
use serde_json::{Value, json};

#[test]
fn a_layout_is_kept_only_when_each_widget_fits_the_grid_once() {
    let placed = |x: u16, y: u16, w: u16, h: u16| json!({"id": "vans.overview", "x": x, "y": y, "w": w, "h": h});
    let kept = |widgets: Value| LAYOUT.keeps(&json!({ "widgets": widgets })).is_some();
    assert!(kept(json!([])));
    assert!(kept(json!([
        placed(13, 0, 3, 3),
        {"id": "dvic.today", "x": 0, "y": 3, "w": 16, "h": 6}
    ])));
    for refused in [
        // Past the grid's right edge, or its bottom, which the page fills.
        json!([placed(14, 0, 3, 3)]),
        json!([placed(250, 0, 250, 3)]),
        json!([placed(0, 4, 3, 6)]),
        json!([placed(0, 0, 3, 10)]),
        json!([placed(0, 65535, 3, 3)]),
        json!([placed(0, 0, 0, 3)]),
        json!([placed(0, 0, 3, 0)]),
        // One widget twice, or one with no id.
        json!([placed(0, 0, 3, 3), placed(6, 0, 3, 3)]),
        json!([{"id": "", "x": 0, "y": 0, "w": 3, "h": 3}]),
    ] {
        assert!(!kept(refused.clone()), "{refused} was kept");
    }
}
