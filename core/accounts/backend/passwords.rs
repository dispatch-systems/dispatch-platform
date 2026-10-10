use super::*;
impl Store {
    fn replace_password(
        &self,
        dsp: Option<&str>,
        user: &PublicUser,
        encoded: &str,
        action: &str,
    ) -> Result<()> {
        let id = user.id.as_str();
        let people = self.directory(dsp)?;
        self.across(&people, || {
            people.exec(
                "UPDATE users SET password=?,version=version+1 WHERE id=?",
                [encoded, id],
            )?;
            people.exec("DELETE FROM sessions WHERE user_id=?", [id])?;
            people.exec("DELETE FROM resets WHERE user_id=?", [id])?;
            // A reset can follow a stolen password, so whatever it may have made stops too,
            // in the same step as the password itself: agents' credentials are platform
            // owners'.
            if action == "account.password_reset"
                && dsp.is_none()
                && let Some(agents) = crate::manifest::registry().agents
            {
                (agents.password_reset)(self, id)?;
            }
            self.audit_account(user, dsp, action, "")
        })
    }
    /// Mails a link to reset the password of the account `email` names in the directory of
    /// the address the request came to; the link goes back there.
    pub fn recovery(&self, email: &str, site: &Site) -> Result<()> {
        ensure(self.config.mail_available(), "email_unavailable", 503)?;
        let Some(scope) = self.site_directory(site)? else {
            return Ok(());
        };
        let dsp = scope.as_deref();
        let people = self.directory(dsp)?;
        let found = UserRow::find(&people, "email", &email.trim().to_lowercase())?;
        let found = match found.filter(UserRow::active) {
            Some(row) if super::sessions::admitted(&people, &row.user, dsp)? => Some(row),
            _ => None,
        };
        if let Some(UserRow { user, version, .. }) = found {
            let raw = crypto::token()?;
            self.across(&people, || {
                self.platform.exec(
                    "DELETE FROM outbox WHERE user_id=? AND kind='reset' AND status IN ('pending','failed')",
                    [&user.id],
                )?;
                people.exec(
                    "DELETE FROM resets WHERE user_id=? OR expires_at<?",
                    params![user.id, now()],
                )?;
                people.exec(
                    "INSERT INTO resets(hash,user_id,user_version,expires_at) VALUES (?,?,?,?)",
                    params![crypto::sha(&raw), user.id, version, now() + 1800000],
                )?;
                let mail = email::reset(
                    &self.config.origin,
                    self.config.env().is_preview(),
                    &user.email,
                    &format!("{}/#reset?token={raw}", self.config.site_origin(site)),
                );
                self.queue_mail(
                    dsp,
                    &user.email,
                    &mail,
                    MailContext::Reset { user: &user.id },
                )
            })?;
        }
        Ok(())
    }
    /// The account a reset link names, in the directory of the address it was opened at.
    fn reset_user(&self, raw: &str, site: &Site) -> Result<(Option<String>, UserRow)> {
        let scope = self
            .site_directory(site)?
            .ok_or_else(|| Error::new("reset_expired", 400))?;
        let row = self
            .directory(scope.as_deref())?
            .one_as(RESET_USER, params![crypto::sha(raw), now()])?
            .ok_or_else(|| Error::new("reset_expired", 400))?;
        Ok((scope, row))
    }
}
impl crate::State {
    pub async fn reauthenticate(
        self: &std::sync::Arc<Self>,
        auth: Auth,
        password: String,
        ip: String,
    ) -> Result<()> {
        let a = auth.clone();
        let row = self
            .run(move |db| {
                db.authenticate(&a.raw, &a.site)?;
                db.password_attempt(a.scope.as_deref(), &a.user.email, &ip)?;
                UserRow::find(&*db.people(&a)?, "id", &a.user.id)?
                    .ok_or_else(|| Error::new("sign_in_required", 401))
            })
            .await?;
        let expected = row.clone();
        self.password_work(move || {
            ensure(
                crypto::check_password(&password, &expected.password),
                "invalid_password",
                403,
            )
        })
        .await?;
        self.run(move |db| {
            db.authenticate(&auth.raw, &auth.site)?;
            let fresh = UserRow::find(&*db.people(&auth)?, "id", &auth.user.id)?;
            ensure(
                fresh
                    .as_ref()
                    .is_some_and(|fresh| same_password_user(fresh, &row)),
                "sign_in_required",
                401,
            )?;
            db.people(&auth)?.exec(
                "INSERT INTO session_security(session_hash,password_verified_at) VALUES (?,?) \
                 ON CONFLICT(session_hash) DO UPDATE SET password_verified_at=excluded.password_verified_at",
                params![auth.hash, now()],
            )?;
            Ok(())
        })
        .await
    }

    pub(super) async fn password_work<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let permit = self
            .password_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::new("login_busy", 429))?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            work()
        })
        .await
        .map_err(|_| Error::new("password_operation_failed", 500))?
    }
    pub async fn change_password(
        self: &std::sync::Arc<Self>,
        auth: Auth,
        current: String,
        password: String,
        ip: String,
    ) -> Result<()> {
        let a = auth.clone();
        let row = self
            .run(move |db| {
                let a = db.authenticate(&a.raw, &a.site)?;
                db.password_attempt(a.scope.as_deref(), &a.user.email, &ip)?;
                UserRow::find(&*db.people(&a)?, "id", &a.user.id)?
                    .ok_or_else(|| Error::new("sign_in_required", 401))
            })
            .await?;
        let expected = row.clone();
        let encoded = self
            .password_work(move || {
                ensure(
                    crypto::check_password(&current, &expected.password),
                    "invalid_password",
                    403,
                )?;
                crypto::hash_password(&password)
            })
            .await?;
        self.run(move |db| {
            let a = db.authenticate(&auth.raw, &auth.site)?;
            let fresh = UserRow::find(&*db.people(&a)?, "id", &row.user.id)?;
            ensure(
                fresh
                    .as_ref()
                    .is_some_and(|fresh| same_password_user(fresh, &row)),
                "sign_in_required",
                401,
            )?;
            db.replace_password(
                a.scope.as_deref(),
                &row.user,
                &encoded,
                "account.password_changed",
            )
        })
        .await
    }
    pub async fn reset_password(
        self: &std::sync::Arc<Self>,
        raw: String,
        password: String,
        site: Site,
    ) -> Result<()> {
        let (token, at) = (raw.clone(), site.clone());
        let (_, row) = self.read(move |db| db.reset_user(&token, &at)).await?;
        let encoded = self
            .password_work(move || crypto::hash_password(&password))
            .await?;
        self.run(move |db| {
            let (scope, fresh) = db.reset_user(&raw, &site)?;
            ensure(same_password_user(&fresh, &row), "reset_expired", 400)?;
            db.replace_password(
                scope.as_deref(),
                &row.user,
                &encoded,
                "account.password_reset",
            )
        })
        .await
    }
}

pub(super) fn same_password_user(a: &UserRow, b: &UserRow) -> bool {
    a.user.id == b.user.id
        && a.version == b.version
        && a.password == b.password
        && a.active()
        && b.active()
}

#[cfg(test)]
#[path = "../tests/backend/passwords.rs"]
mod tests;
