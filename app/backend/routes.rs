//! What a path no route matches asks for. Core lists its own routes and every feature's, and
//! asks the app only about the DSP area's old answers.
#[cfg(feature = "team")]
use dispatch_team::TEAM;

/// What each part of the DSP area asked for before routes carried their own access, for a
/// path no route matches. New routes never come here; this only keeps old answers the same.
/// A feature's answers go with it when a build leaves it out.
pub fn area_permission(path: &str, post: bool) -> &'static str {
    let mut parts = path.split('/').skip(3);
    let (endpoint, detail) = (parts.next().unwrap_or(""), parts.next());
    match (endpoint, detail, post) {
        ("connections", ..) => "connections.manage",
        #[cfg(feature = "team")]
        ("members", Some("invite"), true) | ("invitations", ..) => "members.invite",
        #[cfg(feature = "team")]
        ("members", _, true) => "members.manage",
        #[cfg(feature = "team")]
        ("roles", _, true) => "roles.manage",
        #[cfg(feature = "team")]
        ("members" | "roles", _, false) => TEAM,
        #[cfg(feature = "settings")]
        ("profile", _, true) => "settings.manage",
        #[cfg(feature = "timecard")]
        ("paycom", _, true) | ("schedules", ..) => "timecard.manage",
        #[cfg(feature = "timecard")]
        ("jobs", Some("meal-breaks"), false) => "timecard.view",
        #[cfg(feature = "timecard")]
        ("jobs", ..) | ("cortex", _, true) => "collections.run",
        #[cfg(feature = "dvic")]
        ("dvic", Some("schedules"), _) => "dvic.manage",
        #[cfg(feature = "dvic")]
        ("dvic", _, true) => "dvic.collect",
        #[cfg(feature = "dvic")]
        ("dvic", ..) => "dvic.view",
        #[cfg(feature = "routes")]
        ("routes", Some("schedules"), _) => "routes.manage",
        #[cfg(feature = "routes")]
        ("routes", _, true) => "routes.collect",
        #[cfg(feature = "routes")]
        ("routes", ..) => "routes.view",
        #[cfg(feature = "scorecard")]
        ("scorecard", Some("schedules"), _) => "scorecard.manage",
        #[cfg(feature = "scorecard")]
        ("scorecard", _, true) => "scorecard.collect",
        #[cfg(feature = "scorecard")]
        ("scorecard", ..) => "scorecard.view",
        #[cfg(feature = "driver_match")]
        ("driver-match", ..) => "driver_match.manage",
        _ => "timecard.view",
    }
}
