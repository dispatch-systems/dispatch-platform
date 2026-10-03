//! What a path no route matches asks for. Core lists its own routes and every feature's, and
//! asks the app only about the DSP area's old answers.
use crate::feature_manifests::team::api::TEAM;

/// What each part of the DSP area asked for before routes carried their own access, for a
/// path no route matches. New routes never come here; this only keeps old answers the same.
pub fn area_permission(path: &str, post: bool) -> &'static str {
    let mut parts = path.split('/').skip(3);
    let (endpoint, detail) = (parts.next().unwrap_or(""), parts.next());
    match (endpoint, detail, post) {
        ("connections", ..) => "connections.manage",
        ("members", Some("invite"), true) | ("invitations", ..) => "members.invite",
        ("members", _, true) => "members.manage",
        ("roles", _, true) => "roles.manage",
        ("members" | "roles", _, false) => TEAM,
        ("profile", _, true) => "settings.manage",
        ("paycom", _, true) | ("schedules", ..) => "timecard.manage",
        ("jobs", Some("meal-breaks"), false) => "timecard.view",
        ("jobs", ..) | ("cortex", _, true) => "collections.run",
        ("dvic", Some("schedules"), _) => "dvic.manage",
        ("dvic", _, true) => "dvic.collect",
        ("dvic", ..) => "dvic.view",
        ("routes", Some("schedules"), _) => "routes.manage",
        ("routes", _, true) => "routes.collect",
        ("routes", ..) => "routes.view",
        ("scorecard", Some("schedules"), _) => "scorecard.manage",
        ("scorecard", _, true) => "scorecard.collect",
        ("scorecard", ..) => "scorecard.view",
        ("driver-match", ..) => "driver_match.manage",
        _ => "timecard.view",
    }
}
