use crate::{
    Error, Result,
    accounts::{Context, api::types::Role},
    db::{Db, FromRow, Row, Store, iso, now, s},
    ensure,
    foundation::crypto,
    manifest::{DefaultRole, Permission, registry},
};
use rusqlite::params;
use serde_json::json;
use std::sync::LazyLock;

const ROLES: &str = "SELECT r.*,\
    (SELECT count(*) FROM memberships m WHERE m.role_id=r.id) members,\
    (SELECT count(*) FROM invitations i WHERE i.role_id=r.id AND i.used_at IS NULL \
     AND i.expires_at>?1) invitations \
    FROM roles r WHERE r.dsp_id=?2 ORDER BY r.system DESC,r.created_at,r.name";

// Every permission a DSP owner can grant, as core and the features declare them, in their
// order. Owners implicitly hold all of them, so a permission added to a manifest reaches
// owners without touching stored roles.
static DECLARED: LazyLock<Vec<&'static Permission>> = LazyLock::new(|| {
    let mut all: Vec<_> = registry().permissions().collect();
    all.sort_by_key(|permission| (permission.order, permission.id));
    all
});
pub static PERMISSIONS: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| DECLARED.iter().map(|permission| permission.id).collect());
// Build-time metadata for the generated dashboard catalog; not shipped at runtime.
#[cfg(feature = "ts")]
pub static LABELS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    DECLARED
        .iter()
        .map(|permission| (permission.id, permission.label))
        .collect()
});
// Any membership satisfies this; it guards pages every member may open.
pub const ACCESS: &str = "access";
// What each permission grants as well, directly: what it implies and what it sits under, in
// the order the registry declares them.
pub static IMPLIED: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    registry()
        .permissions()
        .flat_map(|permission| {
            permission
                .implies
                .iter()
                .chain(&permission.under)
                .map(|implied| (permission.id, *implied))
        })
        .collect()
});
/// `permissions` with everything they grant as well, however many steps away.
pub fn granting(permissions: &[String]) -> Vec<String> {
    granting_by(&IMPLIED, permissions)
}
/// `granting`, by the pairs of `implied`: what each grants directly.
fn granting_by(implied: &[(&str, &str)], permissions: &[String]) -> Vec<String> {
    let mut all = permissions.to_vec();
    let mut grew = true;
    while grew {
        grew = false;
        for (permission, implied) in implied {
            if all.iter().any(|p| p == permission) && !all.iter().any(|p| p == implied) {
                all.push((*implied).to_owned());
                grew = true;
            }
        }
    }
    all
}
// Permissions outside a feature-owned page have these role-sheet sections, each where its
// first permission falls in the order. Build-time metadata, as `LABELS` is.
#[cfg(feature = "ts")]
pub static GROUPS: LazyLock<Vec<(&'static str, Vec<&'static str>)>> = LazyLock::new(|| {
    let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
    for permission in DECLARED.iter() {
        let Some(group) = permission.group else {
            continue;
        };
        match groups.iter_mut().find(|(name, _)| *name == group) {
            Some((_, permissions)) => permissions.push(permission.id),
            None => groups.push((group, vec![permission.id])),
        }
    }
    groups
});
// Each default role's key, name and the permissions it holds in a demo DSP, in their order.
// Everywhere else it starts with none.
pub static DEMO: LazyLock<Vec<(&'static str, &'static str, Vec<&'static str>)>> =
    LazyLock::new(|| {
        DefaultRole::ALL
            .iter()
            .map(|role| {
                let permissions = DECLARED
                    .iter()
                    .filter(|permission| permission.demo.contains(role))
                    .map(|permission| permission.id)
                    .collect();
                (role.key(), role.name(), permissions)
            })
            .collect()
    });
/// Gives a demo DSP's default roles what each holds there, so its demo shows a manager's and a
/// member's view.
pub fn demo(db: &Db, dsp: &str) -> Result<()> {
    for (_, name, permissions) in DEMO.iter() {
        db.exec(
            "UPDATE roles SET permissions=? WHERE dsp_id=? AND system=0 AND name=?",
            params![json!(permissions).to_string(), dsp, name],
        )?;
    }
    Ok(())
}

pub fn all() -> Vec<String> {
    PERMISSIONS.iter().map(|p| (*p).to_owned()).collect()
}
// Granted permissions have no previous value; revoked ones have no new value.
fn permission_changes(before: &[String], after: &[String]) -> Vec<crate::db::AuditChange> {
    let missing = |from: &[String], p: &String| !from.contains(p);
    let added = after.iter().filter(|p| missing(before, p));
    let removed = before.iter().filter(|p| missing(after, p));
    added
        .map(|p| ("permission", None, Some(p.clone())))
        .chain(removed.map(|p| ("permission", Some(p.clone()), None)))
        .collect()
}
// What a stored list grants: known permissions only, in their canonical order.
fn stored(system: bool, permissions: &str) -> Vec<String> {
    if system {
        return all();
    }
    let saved: Vec<String> = serde_json::from_str(permissions).unwrap_or_default();
    PERMISSIONS
        .iter()
        .filter(|p| saved.iter().any(|v| v == *p))
        .map(|p| (*p).to_owned())
        .collect()
}
/// A row of `roles`. `system` marks the owner role, which holds every permission.
#[derive(Clone)]
pub struct RoleRow {
    pub id: String,
    pub name: String,
    pub system: bool,
    pub permissions: Vec<String>,
}
impl FromRow for RoleRow {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let system = row.get("system")?;
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            system,
            permissions: stored(system, &row.get::<String>("permissions")?),
        })
    }
}
impl RoleRow {
    pub fn legacy(&self) -> &'static str {
        legacy(self.system, &self.permissions)
    }
    fn public(self, counts: Option<(i64, i64)>) -> Role {
        Role {
            id: self.id,
            name: self.name,
            owner: self.system,
            permissions: self.permissions,
            members: counts.map(|(members, _)| members),
            invitations: counts.map(|(_, invitations)| invitations),
        }
    }
}
struct CountedRole(Role);
impl FromRow for CountedRole {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let counts = (row.get("members")?, row.get("invitations")?);
        Ok(Self(RoleRow::from_row(row)?.public(Some(counts))))
    }
}
// The legacy role column stays populated so an older Rust runtime keeps
// working after rollback, never with more access than the role grants. That runtime's
// managers were the members who could run collections, so the mapping stays frozen on it.
fn legacy(system: bool, permissions: &[String]) -> &'static str {
    if system {
        "owner"
    } else if permissions.iter().any(|p| p == "collections.run") {
        "manager"
    } else {
        "member"
    }
}

// Seeds every DSP's default roles, and gives a role_id to each membership and
// invitation that has only the legacy role.
pub fn backfill(db: &Db) -> Result<()> {
    db.transaction(|| {
        for dsp in db.all("SELECT id FROM dsps", [])? {
            seed(db, s(&dsp, "id"))?;
        }
        legacy_roles(db)
    })
}
/// `backfill` in `dsp`'s own directory, which holds its people and roles alone.
pub fn backfill_directory(db: &Db, dsp: &str) -> Result<()> {
    db.transaction(|| {
        seed(db, dsp)?;
        legacy_roles(db)
    })
}
fn legacy_roles(db: &Db) -> Result<()> {
    for table in ["memberships", "invitations"] {
        for row in db.all(
            &format!("SELECT DISTINCT dsp_id,role FROM {table} WHERE role_id IS NULL"),
            [],
        )? {
            let id = default_role(db, s(&row, "dsp_id"), s(&row, "role"))?;
            db.exec(
                &format!(
                    "UPDATE {table} SET role_id=? WHERE role_id IS NULL AND dsp_id=? AND role=?"
                ),
                [&id, s(&row, "dsp_id"), s(&row, "role")],
            )?;
        }
    }
    Ok(())
}
pub fn seed(db: &Db, dsp: &str) -> Result<()> {
    if db
        .one("SELECT id FROM roles WHERE dsp_id=? LIMIT 1", [dsp])?
        .is_some()
    {
        return Ok(());
    }
    for role in ["owner", "manager", "member"] {
        default_role(db, dsp, role)?;
    }
    Ok(())
}
// Finds the role a legacy value maps to, restoring a deleted default when an
// older runtime wrote a membership that still needs one. A default role starts with no
// permission, as every role does; the DSP's owner turns them on.
pub fn default_role(db: &Db, dsp: &str, role: &str) -> Result<String> {
    let (name, system) = match role {
        "owner" => ("Owner", true),
        other => DefaultRole::ALL
            .iter()
            .find(|d| d.key() == other)
            .map(|d| (d.name(), false))
            .ok_or_else(|| Error::new("invalid_role", 400))?,
    };
    let found = if system {
        db.one("SELECT id FROM roles WHERE dsp_id=? AND system=1", [dsp])?
    } else {
        db.one(
            "SELECT id FROM roles WHERE dsp_id=? AND system=0 AND name=?",
            [dsp, name],
        )?
    };
    if let Some(row) = found {
        return Ok(s(&row, "id").to_owned());
    }
    let id = crypto::id("role")?;
    db.exec(
        "INSERT INTO roles(id,dsp_id,name,permissions,system,created_at) VALUES (?,?,?,?,?,?)",
        params![id, dsp, name, "[]", system, iso()],
    )?;
    Ok(id)
}

pub struct Grant {
    pub id: String,
    pub name: String,
    pub owner: bool,
    pub permissions: Vec<String>,
}
impl From<RoleRow> for Grant {
    fn from(row: RoleRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            owner: row.system,
            permissions: row.permissions,
        }
    }
}
impl Store {
    // A member's effective role. Rows written by an older runtime have no
    // role_id yet, so they resolve through the legacy value without writing.
    pub fn grant(&self, user: &str, dsp: &str) -> Result<Option<Grant>> {
        let people = self.dsp(dsp)?;
        let member: Option<(String, Option<String>)> = people.one_as(
            "SELECT role,role_id FROM memberships WHERE user_id=? AND dsp_id=?",
            [user, dsp],
        )?;
        let Some((legacy, role_id)) = member else {
            return Ok(None);
        };
        let row: Option<RoleRow> = if let Some(id) = &role_id {
            self.find_role(dsp, id)?
        } else if legacy == "owner" {
            people.one_as("SELECT * FROM roles WHERE dsp_id=? AND system=1", [dsp])?
        } else {
            let name = DefaultRole::ALL
                .iter()
                .find(|d| d.key() == legacy)
                .map_or("", |d| d.name());
            people.one_as(
                "SELECT * FROM roles WHERE dsp_id=? AND system=0 AND name=?",
                [dsp, name],
            )?
        };
        Ok(row.map(Grant::from))
    }
    pub fn owner_role(&self, dsp: &str) -> Result<String> {
        default_role(&*self.dsp(dsp)?, dsp, "owner")
    }
    pub fn find_role(&self, dsp: &str, id: &str) -> Result<Option<RoleRow>> {
        self.dsp(dsp)?
            .one_as("SELECT * FROM roles WHERE id=? AND dsp_id=?", [id, dsp])
    }
    pub fn role(&self, dsp: &str, id: &str) -> Result<RoleRow> {
        self.find_role(dsp, id)?
            .ok_or_else(|| Error::new("role_not_found", 404))
    }
    pub fn roles(&self, dsp: &str) -> Result<Vec<Role>> {
        let roles: Vec<CountedRole> = self.dsp(dsp)?.query_as(ROLES, params![now(), dsp])?;
        Ok(roles.into_iter().map(|role| role.0).collect())
    }
    // Nobody hands out access they do not hold: the owner role is reserved for
    // owners, and any other role must fit inside the actor's own permissions.
    pub fn ensure_assignable(&self, c: &Context, role: &RoleRow) -> Result<()> {
        ensure(
            if role.system {
                c.owner
            } else {
                role.permissions
                    .iter()
                    .all(|permission| c.permissions.contains(permission))
            },
            "role_exceeds_permissions",
            403,
        )
    }
    fn role_input(c: &Context, name: &str, requested: &[String]) -> Result<(String, Vec<String>)> {
        let name = name.trim();
        ensure(
            (1..=40).contains(&name.chars().count()) && !name.eq_ignore_ascii_case("owner"),
            "invalid_role_name",
            400,
        )?;
        ensure(
            requested.iter().all(|p| PERMISSIONS.contains(&p.as_str())),
            "invalid_input",
            400,
        )?;
        let wanted = granting(requested);
        ensure(
            wanted.iter().all(|p| c.can(p)),
            "role_exceeds_permissions",
            403,
        )?;
        let ordered = PERMISSIONS
            .iter()
            .filter(|p| wanted.iter().any(|v| v == *p))
            .map(|p| (*p).to_owned())
            .collect();
        Ok((name.to_owned(), ordered))
    }
    fn ensure_name_free(&self, dsp: &str, name: &str, except: &str) -> Result<()> {
        ensure(
            self.dsp(dsp)?
                .one(
                    "SELECT id FROM roles WHERE dsp_id=? AND name=? AND id<>?",
                    [dsp, name, except],
                )?
                .is_none(),
            "role_name_taken",
            409,
        )
    }
    pub fn create_role(&self, c: &Context, name: &str, permissions: &[String]) -> Result<Role> {
        let dsp = c.dsp.id.as_str();
        let (name, permissions) = Self::role_input(c, name, permissions)?;
        let people = self.dsp(dsp)?;
        self.across(&people, || {
            self.ensure_name_free(dsp, &name, "")?;
            let count = people.count("SELECT count(*) FROM roles WHERE dsp_id=?", [dsp])?;
            ensure(count < 50, "role_limit", 409)?;
            let id = crypto::id("role")?;
            people.exec(
                "INSERT INTO roles(id,dsp_id,name,permissions,created_at) VALUES (?,?,?,?,?)",
                params![id, dsp, name, json!(permissions).to_string(), iso()],
            )?;
            self.audit_ref(
                Some(c.actor()),
                Some(dsp),
                "role.created",
                &name,
                Some(&name),
                &permission_changes(&[], &permissions),
                Some(("role", &id)),
            )?;
            Ok(self.role(dsp, &id)?.public(None))
        })
    }
    pub fn update_role(
        &self,
        c: &Context,
        id: &str,
        name: &str,
        permissions: &[String],
    ) -> Result<Role> {
        let dsp = c.dsp.id.as_str();
        let (name, permissions) = Self::role_input(c, name, permissions)?;
        let people = self.dsp(dsp)?;
        self.across(&people, || {
            let role = self.role(dsp, id)?;
            ensure(!role.system, "owner_role_locked", 409)?;
            self.ensure_assignable(c, &role)?;
            self.ensure_name_free(dsp, &name, id)?;
            // A permission of a feature the DSP lacks was never shown, so it stays as
            // it was; switching the feature back on finds the role unchanged.
            let hidden: Vec<&String> = role
                .permissions
                .iter()
                .filter(|p| !crate::tenancy::catalog::grants(&c.features, p))
                .collect();
            let permissions: Vec<String> = PERMISSIONS
                .iter()
                .filter(|p| permissions.iter().any(|v| v == *p) || hidden.iter().any(|v| v == p))
                .map(|p| (*p).to_owned())
                .collect();
            people.exec(
                "UPDATE roles SET name=?,permissions=? WHERE id=?",
                params![name, json!(permissions).to_string(), id],
            )?;
            let mirror = legacy(false, &permissions);
            people.exec(
                "UPDATE memberships SET role=? WHERE role_id=?",
                [mirror, id],
            )?;
            people.exec(
                "UPDATE invitations SET role=? WHERE role_id=? AND used_at IS NULL",
                [mirror, id],
            )?;
            // Open views sign the DSP revision, so members pick up the change.
            self.platform
                .exec("UPDATE dsps SET revision=revision+1 WHERE id=?", [dsp])?;
            let mut changes = permission_changes(&role.permissions, &permissions);
            if role.name != name {
                changes.insert(0, ("name", Some(role.name.clone()), Some(name.clone())));
            }
            self.audit_ref(
                Some(c.actor()),
                Some(dsp),
                "role.updated",
                &name,
                Some(&role.name),
                &changes,
                Some(("role", id)),
            )?;
            Ok(self.role(dsp, id)?.public(None))
        })
    }
    pub fn delete_role(&self, c: &Context, id: &str) -> Result<()> {
        let dsp = c.dsp.id.as_str();
        let people = self.dsp(dsp)?;
        self.across(&people, || {
            let role = self.role(dsp, id)?;
            ensure(!role.system, "owner_role_locked", 409)?;
            self.ensure_assignable(c, &role)?;
            let members = people.count("SELECT count(*) FROM memberships WHERE role_id=?", [id])?;
            ensure(members == 0, "role_in_use", 409)?;
            // Deleting a role is the one change that cancels its pending invitations.
            people.exec(
                "DELETE FROM invitations WHERE role_id=? AND used_at IS NULL",
                [id],
            )?;
            people.exec("DELETE FROM roles WHERE id=?", [id])?;
            self.audit_ref(
                Some(c.actor()),
                Some(dsp),
                "role.deleted",
                &role.name,
                Some(&role.name),
                &[],
                Some(("role", id)),
            )
        })
    }
}

#[cfg(test)]
#[path = "../tests/backend/roles.rs"]
mod tests;
