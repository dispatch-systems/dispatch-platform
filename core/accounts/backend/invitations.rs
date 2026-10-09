use super::*;
use crate::manifest::registry;
impl Store {
    /// Invites `email` to the DSP in `role`: the platform owner may, and a member holding the
    /// permission a feature declares to invite with, as Team's Invite Members.
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
        ensure(
            !email.eq_ignore_ascii_case(&a.user.email),
            "already_a_member",
            409,
        )?;
        ensure(
            self.platform.count(
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
                "SELECT count(*) FROM outbox o JOIN invitations i ON i.hash=o.invitation_hash \
             WHERE o.status='pending' AND i.dsp_id=?",
                [dsp],
            )? < self.config.security.mail_tenant_pending,
            "email_queue_full",
            429,
        )?;
        // A resend replaces the outstanding grant rather than accumulating links.
        self.platform.exec(
            "DELETE FROM invitations WHERE dsp_id=? AND email=? COLLATE NOCASE AND used_at IS NULL",
            [dsp, email],
        )?;
        let raw = crypto::token()?;
        self.platform.exec(
            "INSERT INTO invitations(hash,dsp_id,email,role,role_id,expires_at,created_by) \
             VALUES (?,?,?,?,?,?,?)",
            params![
                crypto::sha(&raw),
                dsp,
                email.to_lowercase(),
                role.legacy(),
                role.id,
                now() + INVITATION_TTL,
                a.user.id
            ],
        )?;
        self.audit_with(
            Some(&a.user.id),
            Some(dsp),
            "member.invited",
            &role.name,
            Some(&email.to_lowercase()),
            &[],
        )?;
        Ok(raw)
    }
    /// A DSP's last hundred invitations, open or accepted, the latest to expire first.
    pub fn dsp_invitations(&self, dsp: &str) -> Result<Vec<Value>> {
        self.platform.all(
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
        self.platform.exec(
            "DELETE FROM invitations WHERE dsp_id=? AND email=? COLLATE NOCASE AND used_at IS NULL",
            [c.dsp.id.as_str(), email],
        )?;
        self.audit(
            Some(c.actor()),
            Some(&c.dsp.id),
            "invitation.revoked",
            email,
        )
    }
    /// The address an invitation to `dsp` is accepted at: the DSP's own, once it has a short
    /// code, and until then the invite page, where its first owner gives it one.
    pub fn invitation_site(&self, dsp: &str) -> Result<Site> {
        Ok(match self.find_dsp(dsp)?.code {
            Some(code) => Site::Dsp(code),
            None => Site::Invite,
        })
    }
    /// An invitation's link works at the address its email named, and at the invite page,
    /// which a DSP's links named until it had a short code; at no other DSP's.
    fn ensure_invitation_site(&self, hash: &str, site: &Site) -> Result<()> {
        if site == &Site::Invite {
            return Ok(());
        }
        let dsp: Option<(String,)> = self
            .platform
            .one_as("SELECT dsp_id FROM invitations WHERE hash=?", [hash])?;
        let at = match dsp {
            Some((dsp,)) => Some(self.invitation_site(&dsp)?),
            None => None,
        };
        ensure(at.as_ref() == Some(site), "invitation_expired", 404)
    }
    pub fn invitation(&self, raw: &str, site: &Site) -> Result<Value> {
        ensure(raw.len() == 43, "invitation_expired", 404)?;
        let hash = crypto::sha(raw);
        self.ensure_invitation_site(&hash, site)?;
        let mut invitation = self
            .platform
            .one(INVITATION, params![hash, now(), self.config.environment])?
            .ok_or_else(|| Error::new("invitation_expired", 404))?;
        ensure(self.inviter_authorized(&hash)?, "invitation_expired", 404)?;
        let owner = flag(&invitation, "owner");
        invitation.as_object_mut().unwrap().remove("owner");
        let dsp = s(&invitation, "dspId").to_owned();
        let profile = self.profile(&dsp)?;
        invitation["onboarding"] = json!(owner && profile.setup_required);
        invitation["stationCode"] = json!(profile.station_code);
        // A short code the platform owner already gave the DSP stays as it is.
        invitation["code"] = json!(self.find_dsp(&dsp)?.code);
        Ok(invitation)
    }
    /// What an invitation's link shows: the open invitation, or that it was already accepted.
    pub fn invitation_link(&self, raw: &str, site: &Site) -> Result<Value> {
        let accepted = match raw.len() {
            43 => self.platform.one(
                ACCEPTED_INVITATION,
                params![crypto::sha(raw), now(), self.config.environment],
            )?,
            _ => None,
        };
        match accepted {
            Some(mut accepted) => {
                self.ensure_invitation_site(&crypto::sha(raw), site)?;
                accepted["accepted"] = json!(true);
                accepted["signIn"] = json!(self.sign_in_at(s(&accepted, "dspId"))?);
                Ok(accepted)
            }
            None => self.invitation(raw, site),
        }
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
    /// An outstanding invitation never outlives the authority that issued it.
    fn inviter_authorized(&self, hash: &str) -> Result<bool> {
        let row: Option<(String, String, String)> = self.platform.one_as(
            "SELECT created_by,dsp_id,role_id FROM invitations WHERE hash=?",
            [hash],
        )?;
        let Some((actor, dsp, role)) = row else {
            return Ok(false);
        };
        let Some(user) = UserRow::find(&self.platform, "id", &actor)? else {
            return Ok(false);
        };
        if !user.active() {
            return Ok(false);
        }
        if user.user.platform_owner {
            return Ok(true);
        }
        let Some(grant) = self.grant(&actor, &dsp)? else {
            return Ok(false);
        };
        let Some(role) = self.find_role(&dsp, &role)? else {
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
        let (dsp_id,): (String,) = self
            .platform
            .one_as(
                "SELECT dsp_id FROM invitations WHERE hash=?",
                [crypto::sha(raw)],
            )?
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
            to,
            &mail,
            MailContext::Invitation {
                hash: &crypto::sha(raw),
            },
        )
    }
}
impl crate::State {
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
        let (invite, existing, addressed) = self
            .run(move |db| {
                db.throttle(&format!("invite-token:{}", crypto::sha(&token)), 10, 900000)?;
                let invite = db.invitation(&token, &at)?;
                db.password_attempt(s(&invite, "email"), &ip)?;
                let existing = UserRow::find(&db.platform, "email", s(&invite, "email"))?;
                let addressed = db.find_dsp(s(&invite, "dspId"))?.code.is_some();
                Ok((invite, existing, addressed))
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
        let expected = existing.clone();
        let encoded = self
            .password_work(move || {
                if let Some(row) = expected {
                    ensure(
                        row.active() && crypto::check_password(&password, &row.password),
                        "sign_in_with_existing_password",
                        403,
                    )?;
                    Ok(None)
                } else {
                    Ok(Some(crypto::hash_password(&password)?))
                }
            })
            .await?;
        self.run(move |db| {
            db.platform.transaction(|| {
                let fresh_invite = db.invitation(&raw, &site)?;
                ensure(fresh_invite == invite, "invitation_expired", 404)?;
                let (email, dsp) = (s(&invite, "email"), s(&invite, "dspId"));
                let fresh = UserRow::find(&db.platform, "email", email)?;
                let id = match (existing.as_ref(), fresh.as_ref()) {
                    (Some(before), Some(after)) if same_password_user(before, after) => {
                        after.user.id.clone()
                    }
                    (None, None) => {
                        let id = crypto::id("usr")?;
                        db.platform.exec(
                            "INSERT INTO users(id,email,first_name,last_name,password,created_at) \
                             VALUES (?,?,?,?,?,?)",
                            params![id, email, first, last, encoded, iso()],
                        )?;
                        id
                    }
                    _ => return Err(Error::new("sign_in_with_existing_password", 403)),
                };
                let role = db.role(dsp, s(&invite, "roleId"))?;
                db.platform.exec(
                    "INSERT INTO memberships(id,user_id,dsp_id,role,role_id) VALUES (?,?,?,?,?) \
                     ON CONFLICT(user_id,dsp_id) DO NOTHING",
                    params![crypto::id("mem")?, id, dsp, role.legacy(), role.id],
                )?;
                db.platform.exec(
                    "UPDATE invitations SET used_at=? WHERE hash=?",
                    params![now(), crypto::sha(&raw)],
                )?;
                // A platform owner's name never reaches a DSP's log.
                let inviter = db.platform.one(INVITER, [crypto::sha(&raw)])?;
                let invited_by = inviter.and_then(|u| {
                    if !flag(&u, "platform_owner") {
                        Some(s(&u, "name").to_owned())
                    } else if db.support_visible(dsp) {
                        Some("Platform support".to_owned())
                    } else {
                        None
                    }
                });
                let name = format!("{first} {last}");
                let changes: Vec<_> = invited_by
                    .map(|name| ("invitedBy", None, Some(name)))
                    .into_iter()
                    .collect();
                db.audit_ref(
                    Some(&id),
                    Some(dsp),
                    "member.joined",
                    &role.name,
                    Some(&name),
                    &changes,
                    Some(("member", &id)),
                )?;
                if let Some(profile) = &dsp_profile {
                    db.complete_dsp_profile(dsp, &id, profile)?;
                }
                Ok(json!({
                    "email": invite["email"],
                    "dspId": invite["dspId"],
                    "signIn": db.sign_in_at(dsp)?,
                }))
            })
        })
        .await
    }
}
