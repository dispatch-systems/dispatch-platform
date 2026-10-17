//! The permissions slot: what a DSP's roles grant, as the role sheet offers it. A feature
//! declares its own (`permissions`), and its parts theirs; core's own are declared by the core
//! part that checks each.
use crate::manifest::Registry;

/// Core's own permissions, each declared by the core part that checks it.
const CORE_PERMISSIONS: &[&Permission] = &[&crate::collection::registry::CONNECTIONS];

/// A permission a DSP's roles grant, as the role sheet offers it.
pub struct Permission {
    /// Permanent: roles store it and routes ask for it.
    pub id: &'static str,
    pub label: &'static str,
    /// Its place in the one order every list of permissions follows: the role sheet, API
    /// answers and audit entries. Two at one place go by id, so a number need not be free.
    pub order: u16,
    /// What holding it grants as well.
    pub implies: &'static [&'static str],
    /// The permission it is a finer part of, which it grants as well and is listed under on
    /// the role sheet.
    pub under: Option<&'static str>,
    /// The demo DSPs' roles that hold it, so previews and tests show what a manager or a
    /// member sees. A real DSP's roles start with no permission; its owner turns them on.
    pub demo: &'static [DefaultRole],
    /// The role sheet's section for a permission no page owns, such as `Team`. A page's own
    /// permissions are listed under the page.
    pub group: Option<&'static str>,
    /// Whether a write it allows asks its holder to have verified who they are recently.
    pub recently_verified: bool,
    /// Whether it lets its holder invite members. One permission at most does, and a member's
    /// invitations are good only while they hold it.
    pub invites: bool,
    /// The role name the memberships' legacy column records for a role holding it, from before
    /// roles held permissions, so a runtime from then still tells its managers apart. One
    /// permission at most declares it, and a new one never does.
    pub legacy_role: Option<&'static str>,
}
/// A permission in its own place in the order, implying nothing, held by no demo role and
/// owned by its feature's page: `perm("x.collect", "Collect X", 41).implies(…)`.
pub const fn perm(id: &'static str, label: &'static str, order: u16) -> Permission {
    Permission {
        id,
        label,
        order,
        implies: &[],
        under: None,
        demo: &[],
        group: None,
        recently_verified: false,
        invites: false,
        legacy_role: None,
    }
}
impl Permission {
    pub const fn implies(self, implies: &'static [&'static str]) -> Self {
        Self { implies, ..self }
    }
    /// A finer part of `parent`: holding it grants `parent`, and the role sheet lists it under.
    pub const fn under(self, parent: &'static str) -> Self {
        Self {
            under: Some(parent),
            ..self
        }
    }
    pub const fn demo(self, demo: &'static [DefaultRole]) -> Self {
        Self { demo, ..self }
    }
    pub const fn group(self, group: &'static str) -> Self {
        Self {
            group: Some(group),
            ..self
        }
    }
    pub const fn recently_verified(self) -> Self {
        Self {
            recently_verified: true,
            ..self
        }
    }
    pub const fn invites(self) -> Self {
        Self {
            invites: true,
            ..self
        }
    }
    pub const fn legacy_role(self, role: &'static str) -> Self {
        Self {
            legacy_role: Some(role),
            ..self
        }
    }
}
/// The roles besides the owner's that every DSP starts with, holding no permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultRole {
    Manager,
    Member,
}
impl DefaultRole {
    pub const ALL: [Self; 2] = [Self::Manager, Self::Member];
    /// The legacy role value it stands for.
    pub fn key(self) -> &'static str {
        match self {
            Self::Manager => "manager",
            Self::Member => "member",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Manager => "Manager",
            Self::Member => "Member",
        }
    }
}

impl Registry {
    /// Every permission as declared: core's own, then each feature's, in the registry's
    /// order. Lists of permissions follow their `order` instead.
    pub fn permissions(&self) -> impl Iterator<Item = &'static Permission> {
        let features = self.features.iter().flat_map(|feature| {
            let subs = feature.subfeatures.iter().flat_map(|sub| sub.permissions);
            feature.permissions.iter().chain(subs)
        });
        CORE_PERMISSIONS.iter().copied().chain(features)
    }
    /// The permission that lets a member invite others, if a feature declares one; without
    /// it, only the platform owner invites.
    pub fn inviting(&self) -> Option<&'static str> {
        self.permissions()
            .find(|permission| permission.invites)
            .map(|permission| permission.id)
    }
    /// The role name the memberships' legacy column records for a role holding the
    /// permissions `held`, if a feature declares one for any of them.
    pub fn legacy_role(&self, held: &[String]) -> Option<&'static str> {
        self.permissions()
            .find(|permission| {
                permission.legacy_role.is_some() && held.iter().any(|id| id == permission.id)
            })
            .and_then(|permission| permission.legacy_role)
    }
}

/// Panics unless every permission has an id of its own and implies, or sits under, only
/// permissions that exist and never itself, one at most lets members invite, and one at most
/// names a legacy role.
pub(crate) fn check(registry: &Registry) {
    let permissions: Vec<_> = registry.permissions().collect();
    let inviting = permissions.iter().filter(|p| p.invites).count();
    assert!(
        inviting <= 1,
        "{inviting} permissions let members invite; one at most may"
    );
    let legacy = permissions
        .iter()
        .filter(|p| p.legacy_role.is_some())
        .count();
    assert!(
        legacy <= 1,
        "{legacy} permissions name a legacy role; one at most may"
    );
    for (index, permission) in permissions.iter().enumerate() {
        assert!(
            permissions[..index]
                .iter()
                .all(|other| other.id != permission.id),
            "{} repeats another permission's id",
            permission.id
        );
        for implied in permission.implies.iter().chain(&permission.under) {
            assert!(
                permissions.iter().any(|other| other.id == *implied),
                "{} implies {implied}, which is not a permission",
                permission.id
            );
        }
    }
    // What a permission grants as well never comes back to it, however many steps away.
    let grants: std::collections::BTreeMap<&str, Vec<&str>> = permissions
        .iter()
        .map(|p| (p.id, p.implies.iter().chain(&p.under).copied().collect()))
        .collect();
    for permission in &permissions {
        let mut reached: Vec<&str> = grants[permission.id].clone();
        let mut at = 0;
        while let Some(id) = reached.get(at).copied() {
            assert!(
                id != permission.id,
                "{} grants itself through what it implies",
                permission.id
            );
            for next in grants.get(id).into_iter().flatten() {
                if !reached.contains(next) {
                    reached.push(next);
                }
            }
            at += 1;
        }
    }
}
