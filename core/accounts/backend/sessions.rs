use super::*;
impl Store {
    /// Counts a password tried for `email` in `dsp`'s directory, or the platform's: an
    /// account's attempts are its directory's, so the same address elsewhere is untouched.
    pub fn password_attempt(&self, dsp: Option<&str>, email: &str, ip: &str) -> Result<()> {
        self.throttle_ip(
            "login-ip",
            ip,
            self.config.security.password_ip_attempts,
            self.config.security.password_window_seconds * 1000,
        )?;
        self.throttle(
            &format!(
                "login:email:{}{}",
                dsp.map_or(String::new(), |dsp| format!("{dsp}:")),
                email.to_lowercase()
            ),
            self.config.security.password_account_attempts,
            self.config.security.password_window_seconds * 1000,
        )
    }
    pub fn throttle(&self, key: &str, max: i64, window: i64) -> Result<()> {
        self.platform
            .transaction(|| self.reserve_quota(key, max, window))
    }
    pub fn throttle_ip(&self, namespace: &str, ip: &str, max: i64, window: i64) -> Result<()> {
        self.throttle(&format!("{namespace}:{}", quota_ip(ip)), max, window)
    }
    // Called under the invitation transaction, so failed issuance releases the reservation.
    pub(super) fn reserve_quota(&self, key: &str, max: i64, window: i64) -> Result<()> {
        let namespace = key.split(':').next().unwrap_or("unknown");
        ensure(
            !namespace.is_empty()
                && namespace.len() <= 32
                && namespace
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-'),
            "invalid_throttle_key",
            500,
        )?;
        let key = crypto::sha(key);
        self.platform
            .exec("DELETE FROM throttle WHERE reset_at<?", [now()])?;
        let row: Option<(i64,)> = self
            .platform
            .one_as("SELECT count FROM throttle WHERE key=?", [&key])?;
        ensure(row.as_ref().map_or(0, |r| r.0) < max, "rate_limited", 429)?;
        let known = row.is_some();
        if !known {
            // High-cardinality input must not fill a global table and lock out every unrelated
            // throttle. Retain frequently hit keys; evict a low-use key within the noisy
            // namespace first, then globally if legacy rows already exceed the new bound.
            if self.platform.count(
                "SELECT count(*) FROM throttle WHERE namespace=?",
                [namespace],
            )? >= 4096
            {
                self.platform.exec(
                    "DELETE FROM throttle WHERE key=(SELECT key FROM throttle WHERE namespace=? \
                     ORDER BY count ASC,reset_at ASC,key ASC LIMIT 1)",
                    [namespace],
                )?;
            }
            if self.platform.count("SELECT count(*) FROM throttle", [])? >= 32768 {
                self.platform.exec(
                    "DELETE FROM throttle WHERE key=(SELECT key FROM throttle \
                     ORDER BY count ASC,reset_at ASC,key ASC LIMIT 1)",
                    [],
                )?;
            }
        }
        self.platform.exec(
            "INSERT INTO throttle(key,count,reset_at,namespace) VALUES (?,1,?,?) \
             ON CONFLICT(key) DO UPDATE SET count=count+1,namespace=excluded.namespace",
            params![key, now() + window, namespace],
        )?;
        Ok(())
    }
    /// The session `raw` names at the address the request came to, kept in that address's
    /// directory: the platform's at the admin's, for its owners, and a DSP's own at its
    /// address, for the people it still has. Anywhere else it reads as signed out, as a
    /// removed member's does.
    pub fn authenticate(&self, raw: &str, site: &Site) -> Result<Auth> {
        ensure(
            raw.len() == 43
                && raw
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "sign_in_required",
            401,
        )?;
        let scope = self
            .site_directory(site)?
            .ok_or_else(|| Error::new("sign_in_required", 401))?;
        let hash = crypto::sha(raw);
        let directory = self.directory(scope.as_deref())?;
        let user: PublicUser = directory
            .one_as(SESSION_USER, params![hash, now()])?
            .ok_or_else(|| Error::new("sign_in_required", 401))?;
        ensure(
            admitted(&directory, &user, scope.as_deref())?,
            "sign_in_required",
            401,
        )?;
        Ok(Auth {
            user,
            hash,
            csrf: crypto::sign(&self.key, &format!("csrf:{raw}")),
            raw: raw.into(),
            preview: None,
            site: site.clone(),
            scope,
        })
    }
    pub fn context(&self, a: &Auth, id: &str, permission: &str) -> Result<Context> {
        // At a DSP's address, no other DSP exists.
        if let Some(scope) = &a.scope {
            ensure(scope == id, "permission_denied", 403)?;
        }
        // An ordinary member learns whether they belong to a DSP, never whether a
        // caller-supplied DSP id exists. Resolve their grant first so an absent DSP
        // and an existing DSP outside their membership have the same answer.
        let (dsp, grant) = if !a.user.platform_owner {
            let grant = self
                .grant(&a.user.id, id)?
                .ok_or_else(|| Error::new("permission_denied", 403))?;
            (self.find_dsp(id)?, grant)
        } else {
            // Platform owners may distinguish DSPs, and historically resolve the
            // DSP before a preview role. Preserve that 404/409 ordering.
            let dsp = self.find_dsp(id)?;
            let grant = if let Some(role) = &a.preview {
                // A previewed role that was deleted reads as a stale view, so the
                // dashboard reopens the DSP rather than showing a denial.
                self.find_role(id, role)?
                    .ok_or_else(|| Error::new("dsp_view_expired", 409))?
                    .into()
            } else {
                crate::tenancy::roles::Grant {
                    id: "platform_owner".to_owned(),
                    name: "Platform owner".to_owned(),
                    owner: true,
                    permissions: crate::tenancy::roles::all(),
                }
            };
            (dsp, grant)
        };
        // The platform owner's own view sees what is hidden from the DSP; everyone else,
        // its owners and a role a platform owner looks through included, only what is shown.
        let features = if a.user.platform_owner && a.preview.is_none() {
            self.features(&dsp.id)?
        } else {
            self.shown_features(&dsp.id)?
        };
        let c = Context {
            auth: a.clone(),
            dsp,
            role: grant.id,
            role_name: grant.name,
            owner: grant.owner,
            permissions: grant.permissions,
            features,
        };
        ensure(c.allows(permission), "permission_denied", 403)?;
        ensure(c.dsp.status == DspStatus::Active, "dsp_unavailable", 409)?;
        ensure(
            c.dsp.environment == self.config.env(),
            "environment_mismatch",
            403,
        )?;
        Ok(c)
    }
    // A previewed role rides in the token so every request rebuilds the same
    // access; the signature covers it, so it cannot be swapped for another.
    pub fn view_token(&self, c: &Context) -> String {
        format!(
            "{}.{}{}",
            c.dsp.id,
            c.auth
                .preview
                .as_ref()
                .map_or_else(String::new, |role| format!("{role}.")),
            crypto::sign(
                &self.key,
                &format!(
                    "view:{}:{}:{}:{}",
                    c.auth.hash, c.dsp.id, c.dsp.revision, c.role
                )
            )
        )
    }
    pub fn from_view(&self, a: &Auth, token: &str, permission: &str) -> Result<Context> {
        ensure(
            !token.is_empty() && token.len() < 200,
            "dsp_view_required",
            403,
        )?;
        // A stale view is reported before a missing permission so a member whose
        // role just changed reopens the DSP instead of seeing a denial.
        let parts: Vec<_> = token.split('.').collect();
        let a = Auth {
            preview: (parts.len() == 3).then(|| parts[1].to_owned()),
            ..a.clone()
        };
        let c = self.context(&a, parts[0], crate::tenancy::roles::ACCESS)?;
        ensure(
            crypto::equal(&self.view_token(&c), token),
            "dsp_view_expired",
            409,
        )?;
        ensure(c.allows(permission), "permission_denied", 403)?;
        Ok(c)
    }
    pub fn revalidate(&self, c: &Context, permission: &str) -> Result<Context> {
        let a = Auth {
            preview: c.auth.preview.clone(),
            ..self.authenticate(&c.auth.raw, &c.auth.site)?
        };
        let fresh = self.context(&a, &c.dsp.id, permission)?;
        ensure(
            fresh.dsp.revision == c.dsp.revision
                && fresh.role == c.role
                && fresh.permissions == c.permissions
                && fresh.features == c.features,
            "dsp_view_expired",
            409,
        )?;
        Ok(fresh)
    }
}

fn quota_ip(value: &str) -> String {
    match value.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V6(ip)) => {
            if let Some(ip) = ip.to_ipv4_mapped() {
                return ip.to_string();
            }
            let [a, b, c, d, ..] = ip.segments();
            format!("{a:x}:{b:x}:{c:x}:{d:x}::/64")
        }
        Ok(ip) => ip.to_string(),
        // The request boundary normally supplies a parsed address. Keeping an invalid value
        // in one fixed bucket is safer than letting malformed variants evade the quota.
        Err(_) => "invalid".into(),
    }
}
impl crate::State {
    pub async fn login(
        self: &std::sync::Arc<Self>,
        email: String,
        password: String,
        ip: String,
        user_agent: String,
        lifetime: SessionLifetime,
        site: Site,
    ) -> Result<String> {
        let permit = self
            .password_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::new("login_busy", 429))?;
        let at = site.clone();
        // An account another directory keeps, or one this one no longer admits, is as unknown
        // here as a missing one, and takes as long to refuse.
        let (scope, row) = self
            .run(move |db| {
                let Some(scope) = db.site_directory(&at)? else {
                    db.password_attempt(None, &email, &ip)?;
                    return Ok((None, None));
                };
                db.password_attempt(scope.as_deref(), &email, &ip)?;
                let directory = db.directory(scope.as_deref())?;
                let row = UserRow::find(&directory, "email", &email)?;
                let row = match row {
                    Some(row) if admitted(&directory, &row.user, scope.as_deref())? => Some(row),
                    _ => None,
                };
                Ok((Some(scope), row))
            })
            .await?;
        let value = row.clone();
        let valid = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
            let encoded = if let Some(ref row) = value {
                &row.password
            } else {
                DUMMY.get_or_init(|| {
                    crypto::hash_password("a-long-dummy-password-for-timing")
                        .expect("password hashing")
                })
            };
            crypto::check_password(&password, encoded)
        })
        .await
        .map_err(|_| Error::new("login_failed", 500))?;
        let (Some(scope), Some(row)) = (scope, row.filter(|r| valid && r.active())) else {
            return Err(Error::new("invalid_login", 401));
        };
        self.run(move |db| {
            let directory = db.directory(scope.as_deref())?;
            let current = UserRow::find(&directory, "id", &row.user.id)?
                .ok_or_else(|| Error::new("invalid_login", 401))?;
            ensure(
                current.active()
                    && current.version == row.version
                    && admitted(&directory, &current.user, scope.as_deref())?,
                "invalid_login",
                401,
            )?;
            let raw = crypto::token()?;
            let created_at = now();
            let device = device_label(&user_agent);
            let hash = crypto::sha(&raw);
            directory.transaction(|| {
                directory.exec("DELETE FROM sessions WHERE expires_at<?", [now()])?;
                directory.exec(
                    "INSERT INTO sessions VALUES (?,?,?,?,?)",
                    params![
                        hash,
                        row.user.id,
                        row.version,
                        created_at + lifetime.seconds() * 1000,
                        created_at
                    ],
                )?;
                directory.exec(
                    "INSERT INTO session_metadata(session_hash,device) VALUES (?,?)",
                    params![hash, device],
                )
            })?;
            drop(directory);
            db.audit_account(&row.user, scope.as_deref(), "account.signed_in", "")?;
            Ok(raw)
        })
        .await
    }
}

/// Whether `directory` admits `user`: the platform's its owners alone, whose every DSP is
/// theirs to open, and a DSP's own the people it still has.
pub(super) fn admitted(directory: &Db, user: &PublicUser, dsp: Option<&str>) -> Result<bool> {
    Ok(match dsp {
        None => user.platform_owner,
        Some(dsp) => directory
            .one(
                "SELECT 1 FROM memberships WHERE user_id=? AND dsp_id=?",
                [user.id.as_str(), dsp],
            )?
            .is_some(),
    })
}

fn device_label(user_agent: &str) -> String {
    let browser = if user_agent.contains("Firefox/") {
        "Firefox"
    } else if user_agent.contains("Edg/") {
        "Edge"
    } else if user_agent.contains("Chrome/") || user_agent.contains("Chromium/") {
        "Chrome"
    } else if user_agent.contains("Safari/") {
        "Safari"
    } else {
        "Browser"
    };
    let system = if user_agent.contains("Android") {
        "Android"
    } else if user_agent.contains("iPhone") || user_agent.contains("iPad") {
        "iOS"
    } else if user_agent.contains("Windows") {
        "Windows"
    } else if user_agent.contains("Macintosh") || user_agent.contains("Mac OS") {
        "macOS"
    } else if user_agent.contains("Linux") {
        "Linux"
    } else {
        "unknown device"
    };
    format!("{browser} on {system}")
}

#[cfg(test)]
#[path = "../tests/backend/sessions.rs"]
mod device_tests;
