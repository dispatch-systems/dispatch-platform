//! Each DSP's people, moved from the platform's directory into the DSP's own: once, as the
//! server starts, for a DSP whose database does not hold them yet. What the platform held stays
//! where it was, so the release before reads it as it always did. Should that release run
//! again for a while, what it changed there is moved again at the next start, as long as
//! nothing changed in the DSP's own directory meanwhile.
use super::*;
use crate::tenancy::dsps::DIRECTORY;

/// What both directories held of a DSP's people as they were last moved, as digests.
const MOVED_FROM: &str = "accounts.moved_from";

/// What a DSP's people are, in the platform's directory, each table with the query that
/// selects that DSP's rows of it, in the order their references need. Platform owners stay
/// the platform's: they open every DSP without a membership. Passkeys stay too: each belongs
/// to the address it was made at, which a DSP's people no longer sign in at.
const PEOPLE: [(&str, &str); 6] = [
    ("roles", "SELECT * FROM roles WHERE dsp_id=?1"),
    (
        "users",
        "SELECT u.id,u.email,u.first_name,u.last_name,u.password,u.status,u.version,u.created_at \
         FROM users u WHERE u.platform_owner=0 AND EXISTS \
         (SELECT 1 FROM memberships m WHERE m.user_id=u.id AND m.dsp_id=?1)",
    ),
    (
        "memberships",
        "SELECT m.* FROM memberships m JOIN users u ON u.id=m.user_id \
         WHERE m.dsp_id=?1 AND u.platform_owner=0",
    ),
    (
        "authenticator_apps",
        "SELECT a.* FROM authenticator_apps a JOIN users u ON u.id=a.user_id \
         WHERE u.platform_owner=0 AND EXISTS \
         (SELECT 1 FROM memberships m WHERE m.user_id=u.id AND m.dsp_id=?1)",
    ),
    (
        "recovery_codes",
        "SELECT c.* FROM recovery_codes c JOIN users u ON u.id=c.user_id \
         WHERE u.platform_owner=0 AND EXISTS \
         (SELECT 1 FROM memberships m WHERE m.user_id=u.id AND m.dsp_id=?1)",
    ),
    ("invitations", "SELECT * FROM invitations WHERE dsp_id=?1"),
];

/// How many of each a move copies, by table, as `PEOPLE` names them.
pub type Moved = std::collections::BTreeMap<&'static str, usize>;

/// What moving `dsp`'s people into its own directory copies from `platform`.
pub fn people(platform: &Db, dsp: &str) -> Result<Moved> {
    PEOPLE
        .iter()
        .map(|(table, select)| {
            let count = platform.count(&format!("SELECT count(*) FROM ({select})"), [dsp])?;
            Ok((*table, usize::try_from(count).unwrap_or(0)))
        })
        .collect()
}

/// A digest of `dsp`'s people as `db` holds them, the rows `PEOPLE` selects: two equal
/// digests of one directory, the same people. Roles count without their permissions, which
/// startup renames in both directories as permissions retire.
fn digest(db: &Db, dsp: &str) -> Result<String> {
    let mut all = Vec::new();
    for (table, select) in PEOPLE {
        let select = match table {
            "roles" => "SELECT id,dsp_id,name,system,created_at FROM roles WHERE dsp_id=?1",
            _ => select,
        };
        all.push(json!([
            table,
            db.all(&format!("SELECT * FROM ({select}) ORDER BY 1"), [dsp])?
        ]));
    }
    Ok(crypto::sha(&serde_json::to_string(&all)?))
}

impl Store {
    /// Moves `dsp`'s people into its own directory, unless it holds them already: its roles,
    /// its members' accounts, memberships, authenticator apps and recovery codes, and its
    /// invitations, which the platform then routes to it. Sign-ins and resets end with the
    /// move, as everyone signs in again at the DSP's own address. Answers what it copied.
    ///
    /// Once moved, they are moved again only when the platform's rows changed since, as the
    /// release before writes them, and the DSP's own did not: then the platform's are the
    /// current ones. When both changed, neither is the whole story, and an error says so.
    pub fn move_people(&self, dsp: &str) -> Result<Option<Moved>> {
        let people = self.dsp(dsp)?;
        let again = match people.setting(DIRECTORY, Value::Null)?.as_str() {
            None => false,
            Some("moved") => {
                let from = people.setting(MOVED_FROM, Value::Null)?;
                if from["platform"] == digest(&self.platform, dsp)? {
                    return Ok(None);
                }
                // A passkey is made only here, so one means this directory is in use.
                let untouched = from["directory"] == digest(&people, dsp)?
                    && people.count("SELECT count(*) FROM account_passkeys", [])? == 0;
                if !untouched {
                    crate::foundation::observability::event(
                        "error",
                        "accounts.diverged",
                        json!({"dspId": dsp}),
                    );
                    return Ok(None);
                }
                true
            }
            Some(_) => return Ok(None),
        };
        let mut moved = Moved::new();
        self.across(&people, || {
            if again {
                // In the order their references need; the rest goes with sessions and users.
                for table in ["sessions", "resets", "invitations", "memberships", "users", "roles"] {
                    people.exec(&format!("DELETE FROM {table}"), [])?;
                }
            }
            for (table, select) in PEOPLE {
                // Each value as SQLite holds it, so the copy is exact.
                let mut query = self.platform.0.prepare(select)?;
                let columns: Vec<String> = query
                    .column_names()
                    .map(str::to_owned)
                    .collect();
                let rows: Vec<Vec<rusqlite::types::Value>> = query
                    .query_map([dsp], |row| {
                        (0..columns.len()).map(|i| row.get(i)).collect()
                    })?
                    .collect::<rusqlite::Result<_>>()?;
                let insert = format!(
                    "INSERT OR IGNORE INTO {table}({}) VALUES ({})",
                    columns.join(","),
                    vec!["?"; columns.len()].join(",")
                );
                let mut copied = 0;
                for row in rows {
                    copied += people.exec(&insert, rusqlite::params_from_iter(row))?;
                }
                moved.insert(table, copied);
            }
            self.platform.exec(
                "INSERT OR IGNORE INTO invitation_routes(hash,dsp_id) \
                 SELECT hash,dsp_id FROM invitations WHERE dsp_id=?",
                [dsp],
            )?;
            // Repaired as startup repairs them, so what is compared next time is what stays.
            crate::tenancy::roles::backfill_directory(&people, dsp)?;
            crate::manifest::retirement::roles(&people)?;
            people.set(DIRECTORY, &json!("moved"))?;
            people.set(
                MOVED_FROM,
                &json!({"platform": digest(&self.platform, dsp)?, "directory": digest(&people, dsp)?}),
            )
        })?;
        crate::foundation::observability::event(
            "info",
            "accounts.moved",
            json!({"dspId": dsp, "copied": moved, "again": again}),
        );
        Ok(Some(moved))
    }
}

/// The accounts the platform's directory still holds of a DSP's people that belong to no other
/// DSP: theirs alone. Platform owners stay.
const ONLY_THERE: &str = "SELECT u.id FROM users u WHERE u.platform_owner=0 \
    AND EXISTS (SELECT 1 FROM memberships m WHERE m.user_id=u.id AND m.dsp_id=?1) \
    AND NOT EXISTS (SELECT 1 FROM memberships m WHERE m.user_id=u.id AND m.dsp_id<>?1)";

impl Store {
    /// The DSP's people who are no other DSP's: of `own`, those its own directory holds, the
    /// ones the platform's directory holds of no other DSP's, as a person moved from there into
    /// each DSP's they belonged to keeps one ID in each; and the accounts the platform's holds
    /// of its people alone.
    pub fn people_alone(&self, dsp: &str, own: &[String]) -> Result<Vec<String>> {
        let people: Vec<(String,)> = self.platform.query_as(
            &format!(
                "SELECT value FROM json_each(?2) WHERE NOT EXISTS \
                 (SELECT 1 FROM memberships m WHERE m.user_id=value AND m.dsp_id<>?1) \
                 UNION {ONLY_THERE}"
            ),
            [dsp, serde_json::to_string(own)?.as_str()],
        )?;
        Ok(people.into_iter().map(|(id,)| id).collect())
    }
    /// Deletes what the platform's directory holds of a DSP being deleted: its roles,
    /// memberships and invitations, the routes to its own invitations, and the accounts of its
    /// people that belong to no other DSP, with their sign-ins and resets. An account another
    /// DSP's rows still name stays, as the sender of its invitations or in its log, though it
    /// belongs to no DSP now. What they did to their accounts goes before, with the DSP's log.
    pub fn forget_legacy_people(&self, dsp: &str) -> Result<()> {
        let people: Vec<(String,)> = self.platform.query_as(ONLY_THERE, [dsp])?;
        for table in ["memberships", "invitations", "invitation_routes", "roles"] {
            self.platform
                .exec(&format!("DELETE FROM {table} WHERE dsp_id=?"), [dsp])?;
        }
        for (user,) in people {
            for table in ["sessions", "resets"] {
                self.platform
                    .exec(&format!("DELETE FROM {table} WHERE user_id=?"), [&user])?;
            }
            self.platform.exec(
                "DELETE FROM users WHERE id=?1 \
                 AND NOT EXISTS (SELECT 1 FROM invitations WHERE created_by=?1) \
                 AND NOT EXISTS (SELECT 1 FROM audit WHERE actor_id=?1)",
                [&user],
            )?;
        }
        Ok(())
    }
}
