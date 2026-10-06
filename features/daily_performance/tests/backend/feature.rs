//! The manifest: what the DSPs page and the role sheet show for Daily Performance.
use super::FEATURE;

#[test]
fn declares_its_switch_and_permissions() {
    assert_eq!(FEATURE.name, "daily_performance");
    let switch = FEATURE.switch.expect("a switch on the DSPs page");
    assert_eq!(
        (switch.id, switch.label),
        ("daily_performance", "Daily Performance")
    );
    let view = FEATURE
        .permissions
        .iter()
        .find(|p| p.id == "daily_performance.view");
    // Only a DSP's owners hold it until a role grants it.
    assert!(view.is_some_and(|p| p.defaults.is_empty()));
}
