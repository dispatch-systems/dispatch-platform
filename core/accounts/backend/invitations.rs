use super::*;
use crate::manifest::registry;
impl Store {
    /// Invites `email` to the DSP in `role`: the platform owner may, and a member holding the
    /// permission a feature declares to invite with, as Team's Invite Members. The invitation
    /// is the DSP's own, kept in its directory; the platform keeps only which DSP its token is
    /// for.
    pub fn invite(&self, a: &Auth, dsp: &str, email: &str, role: &str) -> Result<String> {
        let c = match registry().inviting() {
            Some(inviting) => self.context(a, dsp, inviting)?,
            // With no feature to invite through, only the platform owner, as themselves, does.
            None => {
                ensure(
                    a.user.platform_owner && a.preview.is_none(),
                    "permission_denied",
                    403,
                )?;
                self.context(a, dsp, crate::tenancy::roles::ACCESS)?
            }
        };
        let role = self.role(dsp, role)?;
        self.ensure_assignable(&c, &role)?;
        ensure(self.config.mail_available(), "email_unavailable", 503)?;
        let people = self.dsp(dsp)?;
        ensure(
            people.count(
                "SELECT count(*) FROM memberships m JOIN users u ON u.id=m.user_id \
                 WHERE m.dsp_id=? AND u.email=? COLLATE NOCASE",
                [dsp, email],
            )? == 0,
            "already_a_member",
            409,
        )?;
        self.reserve_quota(
            &format!("mail:actor:{}", a.user.id),
            self.config.security.mail_actor_hourly,
            3600000,
        )?;
        self.reserve_quota(
            &format!("mail:dsp:{dsp}"),
            self.config.security.mail_tenant_hourly,
            3600000,
        )?;
        self.reserve_quota(
            &format!("mail:recipient:{}", email.to_lowercase()),
            self.config.security.mail_recipient_daily,
            86400000,
        )?;
        self.reserve_quota(
            &format!("mail:cooldown:{}", email.to_lowercase()),
            1,
            self.config.security.mail_cooldown_seconds * 1000,
        )?;
        ensure(
            self.platform.count(
                "SELECT count(*) FROM outbox WHERE status='pending' AND kind='invitation' \
                 AND dsp_id=?",
                [dsp],
            )? < self.config.security.mail_tenant_pending,
            "email_queue_full",
            429,
        )?;
        let raw = crypto::token()?;
        let hash = crypto::sha(&raw);
        self.across(&people, || {
            // A resend replaces the outstanding grant rather than accumulating links.
            people.exec(
                "DELETE FROM invitations WHERE dsp_id=? AND email=? COLLATE NOCASE AND used_at IS NULL",
                [dsp, email],
            )?;
            people.exec(
                "INSERT INTO invitations(hash,dsp_id,email,role,role_id,expires_at,created_by) \
                 VALUES (?,?,?,?,?,?,?)",
                params![
                    hash,
                    dsp,
                    email.to_lowercase(),
                    role.legacy(),
                    role.id,
                    now() + INVITATION_TTL,
                    a.user.id
                ],
            )?;
            self.platform.exec(
                "INSERT INTO invitation_routes(hash,dsp_id) VALUES (?,?)",
                [hash.as_str(), dsp],
            )?;
            self.audit_with(
                Some(&a.user.id),
                Some(dsp),
                "member.invited",
                &role.name,
                Some(&email.to_lowercase()),
                &[],
            )
        })?;
        Ok(raw)
    }
    /// A DSP's last hundred invitations, open or accepted, the latest to expire first.
    pub fn dsp_invitations(&self, dsp: &str) -> Result<Vec<Value>> {
        self.dsp(dsp)?.all(
            "SELECT i.email,COALESCE(r.name,i.role) role,i.expires_at expiresAt,\
            i.used_at IS NOT NULL accepted FROM invitations i LEFT JOIN roles r ON r.id=i.role_id \
            WHERE i.dsp_id=? ORDER BY i.expires_at DESC LIMIT 100",
            [dsp],
        )
    }
    /// Withdraws a DSP's open invitation to an address; an accepted one stays.
    /// Revokes what is outstanding of `email`'s invitations to the member's DSP, and logs it,
    /// as every invitation's events are.
    pub fn revoke_invitation(&self, c: &Context, email: &str) -> Result<()> {
        let people = self.dsp(&c.dsp.id)?;
        self.across(&people, || {
            people.exec(
                "DELETE FROM invitations WHERE dsp_id=? AND email=? COLLATE NOCASE AND used_at IS NULL",
                [c.dsp.id.as_str(), email],
            )?;
            self.audit(
                Some(c.actor()),
                Some(&c.dsp.id),
                "invitation.revoked",
                email,
            )
        })
    }
    /// The address an invitation to `dsp` is accepted at: the DSP's own, once it has a short
    /// code, and until then the invite page, where its first owner gives it one.
    pub fn invitation_site(&self, dsp: &str) -> Result<Site> {
        Ok(match self.find_dsp(dsp)?.code {
            Some(code) => Site::Dsp(code),
            None => Site::Invite,
        })
    }
    /// The DSP whose directory keeps the invitation `hash`: its link names only its token.
    fn invitation_dsp(&self, hash: &str) -> Result<Option<String>> {
        let row: Option<(String,)> = self
            .platform
            .one_as("SELECT dsp_id FROM invitation_routes WHERE hash=?", [hash])?;
        Ok(row.map(|(dsp,)| dsp))
    }
    /// The DSP an invitation is for, while that DSP is active here: its link works at the
    /// address its email named, and at the invite page, which a DSP's links named until it
    /// had a short code; at no other DSP's.
    fn invited_dsp(&self, hash: &str, site: &Site) -> Result<Dsp> {
        let expired = || Error::new("invitation_expired", 404);
        let id = self.invitation_dsp(hash)?.ok_or_else(expired)?;
        let dsp = self.find_dsp(&id).map_err(|_| expired())?;
        ensure(
            dsp.status == DspStatus::Active && dsp.environment == self.config.env(),
            "invitation_expired",
            404,
        )?;
        if site != &Site::Invite {
            ensure(
                &self.invitation_site(&id)? == site,
                "invitation_expired",
                404,
            )?;
        }
        Ok(dsp)
    }
    pub fn invitation(&self, raw: &str, site: &Site) -> Result<Value> {
        ensure(raw.len() == 43, "invitation_expired", 404)?;
        let hash = crypto::sha(raw);
        let dsp = self.invited_dsp(&hash, site)?;
        let mut invitation = self
            .dsp(&dsp.id)?
            .one(INVITATION, params![hash, now()])?
            .ok_or_else(|| Error::new("invitation_expired", 404))?;
        ensure(
            self.inviter_authorized(&dsp.id, &hash)?,
            "invitation_expired",
            404,
        )?;
        let owner = flag(&invitation, "owner");
        invitation.as_object_mut().unwrap().remove("owner");
        let profile = self.profile(&dsp.id)?;
        invitation["dspName"] = json!(dsp.name);
        invitation["timezone"] = json!(dsp.timezone);
        invitation["onboarding"] = json!(owner && profile.setup_required);
        invitation["stationCode"] = json!(profile.station_code);
        // A short code the platform owner already gave the DSP stays as it is.
        invitation["code"] = json!(dsp.code);
        Ok(invitation)
    }
    /// What an invitation's link shows: the open invitation, or that it was already accepted.
    pub fn invitation_link(&self, raw: &str, site: &Site) -> Result<Value> {
        if raw.len() == 43 {
            let hash = crypto::sha(raw);
            let dsp = self.invited_dsp(&hash, site)?;
            let accepted = self
                .dsp(&dsp.id)?
                .one(ACCEPTED_INVITATION, params![hash, now()])?;
            if let Some(mut accepted) = accepted {
                accepted["dspName"] = json!(dsp.name);
                accepted["accepted"] = json!(true);
                accepted["signIn"] = json!(self.sign_in_at(&dsp.id)?);
                return Ok(accepted);
            }
        }
        self.invitation(raw, site)
    }
    /// Where the members of `dsp` sign in: its own address, once it has a short code.
    fn sign_in_at(&self, dsp: &str) -> Result<Option<String>> {
        Ok(self
            .find_dsp(dsp)?
            .code
            .map(|code| format!("{}/#signin", self.config.dsp_url(&code))))
    }
    /// Whether an onboarding invitation's DSP could take `code` as its short code, and the
    /// address it would then have.
    pub fn short_code_check(&self, raw: &str, site: &Site, code: &str) -> Result<Value> {
        let invitation = self.invitation(raw, site)?;
        ensure(flag(&invitation, "onboarding"), "permission_denied", 403)?;
        let own = invitation["code"]
            .as_str()
            .is_some_and(|own| own.eq_ignore_ascii_case(code.trim()));
        Ok(json!({
            "available": own || self.code_available(code)?,
            "address": self.config.dsp_url(code),
        }))
    }
    /// An outstanding invitation never outlives the authority that issued it: an active
    /// platform owner's, or the DSP's own member's while they may still invite to its role.
    fn inviter_authorized(&self, dsp: &str, hash: &str) -> Result<bool> {
        let people = self.dsp(dsp)?;
        let row: Option<(String, String)> = people.one_as(
            "SELECT created_by,role_id FROM invitations WHERE hash=?",
            [hash],
        )?;
        let Some((actor, role)) = row else {
            return Ok(false);
        };
        if let Some(owner) = UserRow::find(&self.platform, "id", &actor)?
            && owner.user.platform_owner
        {
            return Ok(owner.active());
        }
        let Some(user) = UserRow::find(&people, "id", &actor)? else {
            return Ok(false);
        };
        if !user.active() {
            return Ok(false);
        }
        let Some(grant) = self.grant(&actor, dsp)? else {
            return Ok(false);
        };
        let Some(role) = self.find_role(dsp, &role)? else {
            return Ok(false);
        };
        Ok(grant.owner
            || (!role.system
                && registry()
                    .inviting()
                    .is_some_and(|inviting| grant.permissions.iter().any(|p| p == inviting))
                && role
                    .permissions
                    .iter()
                    .all(|permission| grant.permissions.contains(permission))))
    }
    pub fn invitation_mail(
        &self,
        a: &Auth,
        to: &str,
        dsp: &str,
        role: &str,
        raw: &str,
        onboarding: bool,
    ) -> Result<()> {
        let inviter = a.user.name();
        let hash = crypto::sha(raw);
        let dsp_id = self
            .invitation_dsp(&hash)?
            .ok_or_else(|| Error::new("invitation_expired", 404))?;
        let at = self.config.site_origin(&self.invitation_site(&dsp_id)?);
        let mail = email::invitation(&email::Invitation {
            origin: &self.config.origin,
            dev: self.config.env().is_preview(),
            to,
            inviter: inviter.trim(),
            dsp,
            role,
            url: &format!("{at}/#invite?token={raw}"),
            expires_at: now() + INVITATION_TTL,
            onboarding,
        });
        self.queue_mail(
            Some(&dsp_id),
            to,
            &mail,
            MailContext::Invitation { hash: &hash },
        )
    }
    /// Who sent an invitation, as its DSP's log names them: a platform owner only as Platform
    /// support, where the DSP shows it.
    fn inviter_name(&self, dsp: &str, hash: &str) -> Result<Option<String>> {
        let sender: Option<(String,)> = self
            .dsp(dsp)?
            .one_as("SELECT created_by FROM invitations WHERE hash=?", [hash])?;
        let Some((sender,)) = sender else {
            return Ok(None);
        };
        if self.platform_owner(&sender)? {
            return Ok(self
                .support_visible(dsp)
                .then(|| "Platform support".to_owned()));
        }
        self.actor_name(dsp, &sender)
    }
}
impl crate::State {
    /// Accepts an invitation: a new account in the DSP's own directory, with the membership
    /// it grants, and for a new DSP's first owner, the DSP's details, short code included.
    /// An address the DSP already has an account for is already a member's.
    pub async fn accept_invitation(
        self: &std::sync::Arc<Self>,
        raw: String,
        request: crate::accounts::api::requests::InvitationRequest,
        ip: String,
        site: Site,
    ) -> Result<Value> {
        let crate::accounts::api::requests::InvitationRequest {
            first_name: first,
            last_name: last,
            password,
            dsp_profile,
        } = request;
        let token = raw.clone();
        let at = site.clone();
        let (invite, addressed) = self
            .run(move |db| {
                db.throttle(&format!("invite-token:{}", crypto::sha(&token)), 10, 900000)?;
                let invite = db.invitation(&token, &at)?;
                let dsp = s(&invite, "dspId");
                db.password_attempt(Some(dsp), s(&invite, "email"), &ip)?;
                ensure(
                    UserRow::find(&*db.dsp(dsp)?, "email", s(&invite, "email"))?.is_none(),
                    "already_a_member",
                    409,
                )?;
                let addressed = db.find_dsp(dsp)?.code.is_some();
                Ok((invite, addressed))
            })
            .await?;
        ensure(
            dsp_profile.is_none() || flag(&invite, "onboarding"),
            "permission_denied",
            403,
        )?;
        // A DSP's first owner sets it up as they join, short code and so address included,
        // unless the platform owner gave it one: then they finish setting it up there.
        ensure(
            dsp_profile.is_some() || !flag(&invite, "onboarding") || addressed,
            "dsp_setup_required",
            400,
        )?;
        let encoded = self
            .password_work(move || crypto::hash_password(&password))
            .await?;
        self.run(move |db| {
            let fresh_invite = db.invitation(&raw, &site)?;
            ensure(fresh_invite == invite, "invitation_expired", 404)?;
            let (email, dsp) = (s(&invite, "email"), s(&invite, "dspId"));
            let hash = crypto::sha(&raw);
            let role = db.role(dsp, s(&invite, "roleId"))?;
            // The DSP's details are checked before anything is written, so a short code
            // another DSP took leaves the invitation open.
            if let Some(profile) = &dsp_profile {
                db.ensure_dsp_profile(dsp, profile)?;
            }
            let id = crypto::id("usr")?;
            // The account, its joining and the DSP's details commit together.
            let people = db.dsp(dsp)?;
            db.across(&people, || {
                ensure(
                    people
                        .one("SELECT id FROM users WHERE email=?", [email])?
                        .is_none(),
                    "already_a_member",
                    409,
                )?;
                people.exec(
                    "INSERT INTO users(id,email,first_name,last_name,password,created_at) \
                     VALUES (?,?,?,?,?,?)",
                    params![id, email, first, last, encoded, iso()],
                )?;
                people.exec(
                    "INSERT INTO memberships(id,user_id,dsp_id,role,role_id) VALUES (?,?,?,?,?)",
                    params![crypto::id("mem")?, id, dsp, role.legacy(), role.id],
                )?;
                people.exec(
                    "UPDATE invitations SET used_at=? WHERE hash=?",
                    params![now(), hash],
                )?;
                let changes: Vec<_> = db
                    .inviter_name(dsp, &hash)?
                    .map(|name| ("invitedBy", None, Some(name)))
                    .into_iter()
                    .collect();
                db.audit_ref(
                    Some(&id),
                    Some(dsp),
                    "member.joined",
                    &role.name,
                    Some(&format!("{first} {last}")),
                    &changes,
                    Some(("member", &id)),
                )?;
                if let Some(profile) = &dsp_profile {
                    db.complete_dsp_profile(dsp, &id, profile)?;
                }
                Ok(())
            })?;
            drop(people);
            Ok(json!({
                "email": invite["email"],
                "dspId": invite["dspId"],
                "signIn": db.sign_in_at(dsp)?,
            }))
        })
        .await
    }
}
