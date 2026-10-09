use crate::collection::registry::Provider;
use crate::{
    Error, Result,
    accounts::{
        Auth, Context,
        api::{
            requests::DspSetupRequest,
            types::{Dsp, DspProfile, DspSummary, DspSummaryLegacy, Member},
        },
    },
    collection::api::types::ConnectionStatus,
    db::{self, FromRow, Row, Store, iso, now},
    ensure,
    foundation::{
        config::{Config, short_code},
        crypto,
    },
    manifest::registry,
    tenancy::api::types::{DspStatus, OwnerStatus},
};
use rusqlite::params;
use serde_json::{Value, json};

/// Marks a DSP's database as holding its people: set when it is made, and once theirs move
/// into it.
pub(crate) const DIRECTORY: &str = "accounts.directory";
const INSERT_DSP: &str = "INSERT INTO dsps(id,name,environment,status,timezone,permanent,created_at) \
    VALUES (?,?,?,'provisioning',?,?,?)";
// Who owns a DSP, from its directory: an active owner, else whoever holds the newest open
// owner invitation.
const OWNER_EMAIL: &str = "SELECT MIN(u.email) FROM memberships o JOIN users u ON u.id=o.user_id \
     WHERE o.dsp_id=? AND o.role='owner' AND u.status='active'";
const INVITE_EMAIL: &str = "SELECT email FROM invitations WHERE dsp_id=? AND role='owner' \
     AND used_at IS NULL AND expires_at>? ORDER BY expires_at DESC LIMIT 1";
const MEMBERS: &str = "SELECT m.id,m.user_id,m.dsp_id,u.email,u.first_name||' '||u.last_name name,\
    COALESCE(r.name,m.role) role,r.id role_id,COALESCE(r.system,m.role='owner') owner \
    FROM memberships m JOIN users u ON u.id=m.user_id LEFT JOIN roles r ON r.id=m.role_id \
    WHERE m.dsp_id=? ORDER BY u.first_name,u.last_name";
const OWNER_COUNT: &str = "SELECT count(*) FROM memberships WHERE dsp_id=? AND role='owner'";
// Only an account with no membership left.
const REMOVABLE_ACCOUNT: &str = "SELECT id FROM users u WHERE id=? \
    AND NOT EXISTS (SELECT 1 FROM memberships WHERE user_id=u.id)";

/// A member as stored; who is online is added by the route.
pub struct MemberRow {
    pub id: String,
    pub user_id: String,
    pub dsp_id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub role_id: Option<String>,
    pub owner: bool,
}
impl FromRow for MemberRow {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            user_id: row.get("user_id")?,
            dsp_id: row.get("dsp_id")?,
            email: row.get("email")?,
            name: row.get("name")?,
            role: row.get("role")?,
            role_id: row.get("role_id")?,
            owner: row.get("owner")?,
        })
    }
}
impl MemberRow {
    pub fn public(self, status: crate::server::api::types::Presence) -> Member {
        Member {
            id: self.id,
            user_id: self.user_id,
            dsp_id: self.dsp_id,
            email: self.email,
            name: self.name,
            role: self.role,
            role_id: self.role_id,
            owner: self.owner,
            status,
        }
    }
}
impl Store {
    pub fn find_dsp(&self, id: &str) -> Result<Dsp> {
        self.platform
            .one_as("SELECT * FROM dsps WHERE id=?", [id])?
            .ok_or_else(|| Error::new("dsp_not_found", 404))
    }
    /// The DSP of this environment whose address has short code `code`, if any has.
    pub fn dsp_at(&self, code: &str) -> Result<Option<Dsp>> {
        self.platform.one_as(
            "SELECT * FROM dsps WHERE code=? AND environment=?",
            params![code.to_ascii_lowercase(), self.config.environment],
        )
    }
    /// Gives a DSP the short code `code`, which then names its address: letters and digits,
    /// free, and not kept for the platform's own addresses. Its abbreviation is the code.
    pub fn set_code(&self, id: &str, code: &str) -> Result<String> {
        let code = code.trim().to_ascii_lowercase();
        ensure(short_code(&code), "invalid_short_code", 400)?;
        ensure(
            !self.config.reserved_code(&code)
                && self.platform.count(
                    "SELECT count(*) FROM dsps WHERE code=? AND id<>?",
                    [code.as_str(), id],
                )? == 0,
            "short_code_taken",
            409,
        )?;
        self.platform.exec(
            "UPDATE dsps SET code=?,revision=revision+1 WHERE id=?",
            [code.as_str(), id],
        )?;
        self.set_profile(id, json!({"abbreviation": code.to_ascii_uppercase()}))?;
        Ok(code)
    }
    /// Whether `code` is free for a DSP to take as its short code.
    pub fn code_available(&self, code: &str) -> Result<bool> {
        let code = code.trim().to_ascii_lowercase();
        Ok(short_code(&code)
            && !self.config.reserved_code(&code)
            && self
                .platform
                .count("SELECT count(*) FROM dsps WHERE code=?", [&code])?
                == 0)
    }
    /// Gives each DSP set up before its short code named its address the code its
    /// abbreviation already is, when that is one and is free. Any other waits for the
    /// platform owner to give it one.
    pub fn backfill_codes(&self) -> Result<()> {
        let waiting: Vec<(String,)> = self.platform.query_as(
            "SELECT id FROM dsps WHERE code IS NULL AND environment=? \
             AND status IN ('active','suspended')",
            [self.config.environment.as_str()],
        )?;
        for (id,) in waiting {
            let abbreviation = self.profile(&id)?.abbreviation;
            if self.code_available(&abbreviation)? {
                let code = self.set_code(&id, &abbreviation)?;
                crate::foundation::observability::event(
                    "info",
                    "dsp.code_assigned",
                    json!({"dspId": id, "code": code}),
                );
            }
        }
        Ok(())
    }
    /// A DSP that may collect: active, and of the environment this backend serves.
    pub fn ensure_dsp_active(&self, id: &str) -> Result<Dsp> {
        let dsp = self.find_dsp(id)?;
        ensure(self.serves(&dsp), "dsp_unavailable", 409)?;
        Ok(dsp)
    }
    pub fn serves(&self, dsp: &Dsp) -> bool {
        dsp.status == DspStatus::Active && dsp.environment == self.config.env()
    }
    /// The DSPs whose data is kept up: every active or suspended one.
    pub fn kept_dsps(&self) -> Result<Vec<String>> {
        let dsps: Vec<(String,)> = self.platform.query_as(
            "SELECT id FROM dsps WHERE status IN ('active','suspended')",
            [],
        )?;
        Ok(dsps.into_iter().map(|(id,)| id).collect())
    }
    pub fn new_dsp(&self, name: &str, timezone: &str, actor: &str, permanent: bool) -> Result<Dsp> {
        ensure(
            timezone.parse::<chrono_tz::Tz>().is_ok(),
            "invalid_timezone",
            400,
        )?;
        let id = crypto::id("dsp")?;
        self.platform.exec(
            INSERT_DSP,
            params![
                id,
                name,
                self.config.environment,
                timezone,
                permanent,
                iso()
            ],
        )?;
        self.seed_features(&id)?;
        self.provision(&id)?;
        self.audit(Some(actor), Some(&id), "dsp.created", "")?;
        self.find_dsp(&id)
    }
    pub fn provision(&self, id: &str) -> Result<()> {
        let dsp = self.find_dsp(id)?;
        ensure(
            [DspStatus::Provisioning, DspStatus::Failed].contains(&dsp.status),
            "dsp_already_initialized",
            409,
        )?;
        let result = (|| {
            // Its directory begins with its default roles, and nobody else's to bring in.
            let people = self.initialize_dsp(id)?;
            super::roles::seed(&people, id)?;
            people.set(DIRECTORY, &json!("own"))?;
            drop(people);
            self.initialize_collectors(id)?;
            for provider in Provider::all() {
                provider.collector().provision(self, id, &dsp.timezone)?;
            }
            self.initialize_schedules(id)?;
            self.platform
                .exec("UPDATE dsps SET status='active' WHERE id=?", [id])?;
            Ok(())
        })();
        if result.is_err() {
            self.platform
                .exec("UPDATE dsps SET status='failed' WHERE id=?", [id])?;
        }
        result
    }
    /// The DSPs a session lists: every one for a platform owner, and for one of a DSP's
    /// people, the DSP whose directory holds their account.
    pub fn dsps(&self, a: &Auth) -> Result<Vec<DspSummary>> {
        let platform = a.user.platform_owner;
        let rows: Vec<Dsp> = self.platform.query_as(
            "SELECT * FROM dsps WHERE ?1 OR id=?2 ORDER BY permanent DESC,name",
            params![platform, a.scope],
        )?;
        let platform_email: Option<(Option<String>,)> = self.platform.one_as(
            "SELECT MIN(email) FROM users WHERE platform_owner=1 AND status='active'",
            [],
        )?;
        let platform_email = platform_email.and_then(|(email,)| email);
        let mut result = Vec::new();
        for dsp in rows {
            // A DSP still being provisioned, or that failed to be, has no directory yet.
            let opened = [DspStatus::Active, DspStatus::Suspended].contains(&dsp.status);
            let mut legacy = DspSummaryLegacy {
                member_role: None,
                owner_email: None,
                platform_email: dsp.permanent.then(|| platform_email.clone()).flatten(),
                invite_email: None,
            };
            let mut members = 0;
            if opened {
                let people = self.dsp(&dsp.id)?;
                let owner: Option<(Option<String>,)> = people.one_as(OWNER_EMAIL, [&dsp.id])?;
                legacy.owner_email = owner.and_then(|(email,)| email);
                let invite: Option<(String,)> =
                    people.one_as(INVITE_EMAIL, params![dsp.id, now()])?;
                legacy.invite_email = invite.map(|(email,)| email);
                members =
                    people.count("SELECT count(*) FROM memberships WHERE dsp_id=?", [&dsp.id])?;
                if !platform {
                    legacy.member_role = self.grant(&a.user.id, &dsp.id)?.map(|grant| grant.name);
                }
            }
            let owner = legacy
                .owner_email
                .as_deref()
                .or(legacy.platform_email.as_deref());
            let invite = legacy.invite_email.as_deref();
            let owner_status = if owner.is_some() {
                OwnerStatus::Active
            } else if invite.is_some() {
                OwnerStatus::Invited
            } else {
                OwnerStatus::Missing
            };
            // A member's list never names what is hidden from them.
            let features = if platform {
                self.features(&dsp.id)?
            } else {
                self.shown_features(&dsp.id)?
            };
            let mut summary = DspSummary {
                features,
                members,
                profile: profile_default(),
                owner_email: owner.or(invite).map(str::to_owned),
                owner_status,
                paycom: ConnectionStatus::NotConnected,
                connections: std::collections::BTreeMap::new(),
                last_collection: None,
                next_collection: None,
                role: if platform {
                    Some("platform_owner".to_owned())
                } else {
                    legacy.member_role.clone()
                },
                dsp,
                legacy,
            };
            if [DspStatus::Active, DspStatus::Suspended].contains(&summary.dsp.status) {
                let id = &summary.dsp.id;
                summary.profile = self.profile(id)?;
                for provider in Provider::all() {
                    let status: Option<(ConnectionStatus,)> =
                        self.collector(id, provider)?.one_as(
                            "SELECT status FROM connections WHERE provider=?",
                            [provider.id()],
                        )?;
                    if let Some((status,)) = status {
                        summary.connections.insert(provider.id().to_owned(), status);
                    }
                }
                // `paycom` is the original provider's connection, as clients from before
                // `connections` read it.
                if let Some(status) =
                    Provider::original().and_then(|original| summary.connections.get(original.id()))
                {
                    summary.paycom = *status;
                }
                // The latest collection any keeper knows of.
                for keeper in registry().keepers() {
                    let collected = keeper.last_collected(self, id)?;
                    summary.last_collection = summary.last_collection.take().max(collected);
                }
                let next: Option<(Option<String>,)> = self.dsp(id)?.one_as(
                    "SELECT MIN(next_run) FROM collection_schedules WHERE enabled=1 \
                    AND collection IN (SELECT value FROM json_each(?))",
                    [crate::collection::api::types::ScheduleCollection::known()?],
                )?;
                summary.next_collection = next.and_then(|(at,)| at);
            }
            result.push(summary);
        }
        Ok(result)
    }
    pub fn profile(&self, id: &str) -> Result<DspProfile> {
        Ok(serde_json::from_value(
            self.dsp(id)?.setting("dsp.profile", json!({}))?,
        )?)
    }
    pub fn set_profile(&self, id: &str, changes: Value) -> Result<DspProfile> {
        let mut profile = serde_json::to_value(self.profile(id)?)?;
        for (k, v) in changes
            .as_object()
            .ok_or_else(|| Error::new("invalid_input", 400))?
        {
            profile[k] = v.clone();
        }
        let profile: DspProfile = serde_json::from_value(profile)?;
        self.dsp(id)?
            .set("dsp.profile", &serde_json::to_value(&profile)?)?;
        Ok(profile)
    }
    pub fn set_status(&self, id: &str, status: DspStatus, actor: &str) -> Result<Dsp> {
        let dsp = self.find_dsp(id)?;
        ensure(!dsp.permanent, "permanent_dev_required", 409)?;
        ensure(
            status != DspStatus::Active || !self.profile(id)?.removed,
            "restore_removed_dsp_first",
            409,
        )?;
        ensure(
            [DspStatus::Active, DspStatus::Suspended].contains(&dsp.status),
            "dsp_unavailable",
            409,
        )?;
        self.platform.exec(
            "UPDATE dsps SET status=?,revision=revision+1 WHERE id=?",
            params![status, id],
        )?;
        self.audit(
            Some(actor),
            Some(id),
            if status == DspStatus::Active {
                "dsp.resumed"
            } else {
                "dsp.suspended"
            },
            "",
        )?;
        self.find_dsp(id)
    }
    fn update_dsp_details(&self, id: &str, actor: &str, name: &str, timezone: &str) -> Result<Dsp> {
        let before = self.find_dsp(id)?;
        self.platform.exec(
            "UPDATE dsps SET name=?,timezone=?,revision=revision+1 WHERE id=?",
            [name, timezone, id],
        )?;
        if before.timezone != timezone {
            self.retime_schedules(id, timezone)?;
        }
        let changes: Vec<_> = [
            ("name", before.name.as_str(), name),
            ("timezone", before.timezone.as_str(), timezone),
        ]
        .into_iter()
        .filter(|(_, before, value)| before != value)
        .map(|(field, before, value)| (field, Some(before.to_owned()), Some(value.to_owned())))
        .collect();
        self.audit_with(
            Some(actor),
            Some(id),
            "dsp.settings_updated",
            "",
            None,
            &changes,
        )?;
        self.find_dsp(id)
    }
    /// Whether `profile` can set `id` up: the abbreviation is the DSP's short code, which
    /// names its address, chosen once, and free.
    pub fn ensure_dsp_profile(&self, id: &str, profile: &DspSetupRequest) -> Result<()> {
        match self.find_dsp(id)?.code {
            Some(code) => ensure(
                code.eq_ignore_ascii_case(&profile.abbreviation),
                "short_code_locked",
                409,
            ),
            None => {
                let code = profile.abbreviation.to_ascii_lowercase();
                ensure(short_code(&code), "invalid_short_code", 400)?;
                ensure(self.code_available(&code)?, "short_code_taken", 409)
            }
        }
    }
    pub fn complete_dsp_profile(
        &self,
        id: &str,
        actor: &str,
        profile: &DspSetupRequest,
    ) -> Result<()> {
        self.ensure_dsp_profile(id, profile)?;
        if self.find_dsp(id)?.code.is_none() {
            self.set_code(id, &profile.abbreviation)?;
        }
        self.update_dsp_details(id, actor, &profile.name, &profile.timezone)?;
        self.set_profile(
            id,
            json!({
                "abbreviation": profile.abbreviation,
                "stationCode": profile.station_code,
                "setupRequired": false
            }),
        )?;
        self.audit_with(
            Some(actor),
            Some(id),
            "dsp.profile_completed",
            "",
            None,
            &[
                ("station", None, Some(profile.station_code.clone())),
                ("abbreviation", None, Some(profile.abbreviation.clone())),
            ],
        )
    }
    pub fn members(&self, id: &str) -> Result<Vec<MemberRow>> {
        self.dsp(id)?.query_as(MEMBERS, [id])
    }
    // The owner role is mirrored into the legacy column on every write, so it
    // stays the single count that protects a DSP from losing its last owner.
    pub fn set_role(&self, c: &Context, member: &str, role: Option<&str>) -> Result<()> {
        let dsp = c.dsp.id.as_str();
        let people = self.dsp(dsp)?;
        people.transaction(|| {
            let user: (String,) = people
                .one_as(
                    "SELECT user_id FROM memberships WHERE id=? AND dsp_id=?",
                    [member, dsp],
                )?
                .ok_or_else(|| Error::new("member_not_found", 404))?;
            let user = user.0.as_str();
            let current = self
                .grant(user, dsp)?
                .ok_or_else(|| Error::new("member_not_found", 404))?;
            self.ensure_assignable(c, &self.role(dsp, &current.id)?)?;
            let next = role.map(|id| self.role(dsp, id)).transpose()?;
            if let Some(next) = &next {
                self.ensure_assignable(c, next)?;
            }
            if current.owner && !next.as_ref().is_some_and(|next| next.system) {
                ensure(
                    people.count(OWNER_COUNT, [dsp])? > 1,
                    "last_owner_required",
                    409,
                )?;
            }
            let name: (String,) = people
                .one_as(
                    "SELECT first_name||' '||last_name FROM users WHERE id=?",
                    [user],
                )?
                .ok_or_else(|| Error::new("member_not_found", 404))?;
            if let Some(next) = &next {
                people.exec(
                    "UPDATE memberships SET role=?,role_id=? WHERE id=?",
                    [next.legacy(), &next.id, member],
                )?;
            } else {
                // Removal invalidates links addressed to them, including legacy duplicates,
                // so they cannot rejoin on their own. Invitations they sent stay open.
                people.exec(
                    "DELETE FROM invitations WHERE dsp_id=? AND used_at IS NULL \
                     AND email=(SELECT email FROM users WHERE id=?) COLLATE NOCASE",
                    [dsp, user],
                )?;
                people.exec("DELETE FROM memberships WHERE id=?", [member])?;
            }
            self.audit_ref(
                Some(c.actor()),
                Some(dsp),
                if next.is_some() {
                    "member.role_changed"
                } else {
                    "member.removed"
                },
                next.as_ref().map_or("", |next| &next.name),
                Some(&name.0),
                &[(
                    "role",
                    Some(self.role(dsp, &current.id)?.name),
                    next.as_ref().map(|next| next.name.clone()),
                )],
                Some(("member", user)),
            )?;
            if next.is_none() {
                self.delete_account(dsp, user)?;
            }
            self.platform
                .exec("UPDATE dsps SET revision=revision+1 WHERE id=?", [dsp])?;
            Ok(())
        })
    }
    // A removed member's account goes with their membership, freeing the email for a fresh
    // invitation. Their name stays on the activity they left.
    fn delete_account(&self, dsp: &str, user: &str) -> Result<()> {
        let people = self.dsp(dsp)?;
        if people.one(REMOVABLE_ACCOUNT, [user])?.is_none() {
            return Ok(());
        }
        people.exec("DELETE FROM sessions WHERE user_id=?", [user])?;
        people.exec("DELETE FROM resets WHERE user_id=?", [user])?;
        // Every invitation must name an existing sender, so the ones they sent go too.
        people.exec("DELETE FROM invitations WHERE created_by=?", [user])?;
        people.exec("DELETE FROM users WHERE id=?", [user])?;
        Ok(())
    }
}
pub fn profile_default() -> DspProfile {
    DspProfile::default()
}

/// Fails unless the platform lists the DSP, for an operator command run beside the
/// server: it reads the platform's database read-only and changes nothing.
pub fn ensure_listed(config: &Config, id: &str) -> Result<()> {
    let path = config.platform().join("accounts.sqlite");
    ensure(path.is_file(), "platform_not_initialized", 404)?;
    db::private_file(&path, false)?;
    let platform = db::Db(rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?);
    ensure(
        platform
            .one("SELECT id FROM dsps WHERE id=?", [id])?
            .is_some(),
        "dsp_not_found",
        404,
    )
}
