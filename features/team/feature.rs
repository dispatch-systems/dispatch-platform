//! Team & Roles: a DSP's members and the roles that grant their permissions.
mod api;

/// Who may read the member and role lists, which the app names for a DSP path no route
/// matches too.
pub use api::routes::TEAM;

use dispatch_core::manifest::{Feature, feature, mandatory, perm};

pub const FEATURE: Feature = Feature {
    place: 100,
    switch: mandatory("team", "Team & Roles"),
    permissions: &[
        perm("members.invite", "Invite Members", 80)
            .group("Team")
            .recently_verified()
            .invites(),
        perm("members.manage", "Manage Members", 81)
            .group("Team")
            .recently_verified(),
        perm("roles.manage", "Manage Roles", 82).group("Team"),
    ],
    routes: api::routes::routes,
    ..feature("team")
};
