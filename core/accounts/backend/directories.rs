//! Each DSP's people, moved from the platform's directory into the DSP's own: once, as the
//! server starts, for a DSP whose database does not hold them yet. What the platform held stays
//! where it was, so the release before reads it as it always did.
use super::*;
use crate::tenancy::dsps::DIRECTORY;

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

impl Store {
    /// Moves `dsp`'s people into its own directory, unless it holds them already: its roles,
    /// its members' accounts, memberships, authenticator apps and recovery codes, and its
    /// invitations, which the platform then routes to it. Sign-ins and resets end with the
    /// move, as everyone signs in again at the DSP's own address. Answers what it copied: a
    /// row the DSP's database already had is not copied again.
    pub fn move_people(&self, dsp: &str) -> Result<Option<Moved>> {
        let people = self.dsp(dsp)?;
        if !people.setting(DIRECTORY, Value::Null)?.is_null() {
            return Ok(None);
        }
        let mut moved = Moved::new();
        people.transaction(|| {
            for (table, select) in PEOPLE {
                // Each value as SQLite holds it, so the copy is exact.
                let mut query = self.platform.0.prepare(select)?;
                let columns: Vec<String> = query
                    .column_names()
                    .into_iter()
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
            people.set(DIRECTORY, &json!("moved"))
        })?;
        crate::foundation::observability::event(
            "info",
            "accounts.moved",
            json!({"dspId": dsp, "copied": moved}),
        );
        Ok(Some(moved))
    }
}
