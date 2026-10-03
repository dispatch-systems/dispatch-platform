//! Team & Roles: a DSP's members and the roles that grant their permissions.
use dispatch_core::manifest::{Feature, feature, perm};

#[path = "api/routes.rs"]
pub mod api;

pub const FEATURE: Feature = Feature {
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
    routes: api::routes,
    ..feature("team")
};
