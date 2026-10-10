//! Accounts, authenticated contexts, sessions and invitations.
#[path = "../api/mod.rs"]
pub mod api;
pub mod directories;
mod invitations;
mod passwords;
mod security;
mod sessions;

use crate::{
    Error, Result,
    accounts::api::types::{Dsp, PublicUser, UserStatus},
    db::{Db, FromRow, Row, Store, flag, iso, now, s},
    ensure,
    foundation::{config::Site, crypto, validate as v},
    server::mail::templates as email,
    tenancy::api::types::DspStatus,
};
use rusqlite::params;
use serde_json::{Value, json};
/// One fixed lifetime drives both the server deadline and the browser cookie.
#[derive(Clone, Copy)]
pub enum SessionLifetime {
    Standard,
    Remembered,
}
impl SessionLifetime {
    pub fn seconds(self) -> i64 {
        match self {
            Self::Standard => 8 * 60 * 60,
            Self::Remembered => 7 * 24 * 60 * 60,
        }
    }
}

const INVITATION_TTL: i64 = 7 * 86400000;
const SESSION_USER: &str = "SELECT u.* FROM users u JOIN sessions s ON s.user_id=u.id \
    WHERE s.hash=? AND s.expires_at>? AND s.user_version=u.version AND u.status='active'";
const RESET_USER: &str = "SELECT u.* FROM resets r JOIN users u ON u.id=r.user_id \
    WHERE r.hash=? AND r.used_at IS NULL AND r.expires_at>? AND r.user_version=u.version \
    AND u.status='active'";
// An invitation, as its DSP's directory keeps it; the DSP's own details are the platform's.
const INVITATION: &str = "SELECT i.email,i.dsp_id dspId,r.name role,r.id roleId,\
    r.system owner FROM invitations i JOIN roles r ON r.id=i.role_id AND r.dsp_id=i.dsp_id \
    WHERE i.hash=? AND i.used_at IS NULL AND i.expires_at>?";
/// A used invitation still names its DSP and role, so opening its link again can point to Sign In.
/// It does so only until the invitation would have expired, the same window an open one has.
const ACCEPTED_INVITATION: &str = "SELECT i.email,i.dsp_id dspId,COALESCE(r.name,i.role) role \
    FROM invitations i LEFT JOIN roles r ON r.id=i.role_id AND r.dsp_id=i.dsp_id \
    WHERE i.hash=? AND i.used_at IS NOT NULL AND i.expires_at>?";

/// What a queued message is for. Diagnostics joins it back to the invitation or account.
enum MailContext<'a> {
    Invitation {
        hash: &'a str,
    },
    Reset {
        user: &'a str,
    },
    /// A notice to a platform owner, of a kind its sender names.
    PlatformNotice {
        kind: &'a str,
        user: &'a str,
    },
    /// What a feature writes to a member, of the feature's own kind.
    Feature {
        kind: &'a str,
        user: &'a str,
    },
}

/// A row of `users`, with the password hash: it never leaves the backend.
#[derive(Clone)]
pub struct UserRow {
    pub user: PublicUser,
    pub password: String,
    pub status: UserStatus,
    pub version: i64,
}
impl FromRow for UserRow {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            user: PublicUser::from_row(row)?,
            password: row.get("password")?,
            status: row.get("status")?,
            version: row.get("version")?,
        })
    }
}
impl UserRow {
    fn active(&self) -> bool {
        self.status == UserStatus::Active
    }
    fn find(db: &Db, column: &str, value: &str) -> Result<Option<Self>> {
        match column {
            "id" => db.one_as("SELECT * FROM users WHERE id=?", [value]),
            _ => db.one_as("SELECT * FROM users WHERE email=?", [value]),
        }
    }
}
/// The database an account lives in: the platform's own for its owners, and each DSP's own
/// for that DSP's people, whose tables match the platform's.
pub enum Directory<'a> {
    Platform(&'a Db),
    Dsp(crate::db::DspLease<'a>),
}
impl std::ops::Deref for Directory<'_> {
    type Target = Db;
    fn deref(&self) -> &Db {
        match self {
            Self::Platform(db) => db,
            Self::Dsp(db) => db,
        }
    }
}
impl Store {
    /// The directory of `dsp`'s people, or with none, the platform's own.
    pub fn directory(&self, dsp: Option<&str>) -> Result<Directory<'_>> {
        Ok(match dsp {
            None => Directory::Platform(&self.platform),
            Some(dsp) => Directory::Dsp(self.dsp(dsp)?),
        })
    }
    /// The directory that holds the account of `a`.
    pub(crate) fn people(&self, a: &Auth) -> Result<Directory<'_>> {
        self.directory(a.scope.as_deref())
    }
    /// Runs `f` in a transaction of `people`, a DSP's directory, inside one of the
    /// platform's, for a change written to both, as a DSP's change and its audit event are:
    /// either commits with the other, unless the platform's own commit fails after the DSP's.
    pub(crate) fn across<T>(&self, people: &Db, f: impl FnOnce() -> Result<T>) -> Result<T> {
        self.platform.transaction(|| people.transaction(f))
    }
    /// The directory a request to `site` signs in against, and the DSP it is, if any: the
    /// platform's at the admin's address, a DSP's own at its address, and none at the invite
    /// page or at an address no DSP has.
    pub(crate) fn site_directory(&self, site: &Site) -> Result<Option<Option<String>>> {
        Ok(match site {
            Site::Admin => Some(None),
            Site::Dsp(code) => self.dsp_at(code)?.map(|dsp| Some(dsp.id)),
            Site::Invite => None,
        })
    }
}
#[derive(Clone)]
pub struct Auth {
    pub user: PublicUser,
    pub hash: String,
    pub csrf: String,
    pub raw: String,
    // The DSP role a platform owner chose to look through instead of their
    // own owner access. Members never carry one.
    pub preview: Option<String>,
    /// The address the session is used at, where it was made.
    pub site: Site,
    /// The DSP whose directory holds the account, the only DSP its session may open. None for
    /// a platform owner, whose account is the platform's.
    pub scope: Option<String>,
}
#[derive(Clone)]
pub struct Context {
    pub auth: Auth,
    pub dsp: Dsp,
    pub role: String,
    pub role_name: String,
    pub owner: bool,
    /// The role's permissions as stored; `can` reads them within `features`.
    pub permissions: Vec<String>,
    /// The features the DSP has (`features`).
    pub features: Vec<String>,
}
impl Context {
    // A permission of a feature the DSP lacks is held by nobody, owners included.
    pub fn can(&self, permission: &str) -> bool {
        crate::tenancy::catalog::grants(&self.features, permission)
            && (self.owner || self.permissions.iter().any(|p| p == permission))
    }
    /// The permissions of `stored` that exist in this DSP.
    pub fn visible<'a>(&'a self, stored: &'a [String]) -> impl Iterator<Item = &'a String> {
        crate::tenancy::catalog::visible(&self.features, stored)
    }
    /// Whether the DSP has `feature`. A page gates on its permissions instead; this is for
    /// a connection or a tab, which own none.
    pub fn has(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }
    // Alternatives are separated by `|`; any one of them grants the request.
    pub fn allows(&self, permission: &str) -> bool {
        permission
            .split('|')
            .any(|wanted| wanted == crate::tenancy::roles::ACCESS || self.can(wanted))
    }
}
impl Context {
    /// The member who is acting, for the audit log.
    pub fn actor(&self) -> &str {
        &self.auth.user.id
    }
    /// Records what the member did in this DSP.
    pub fn audit(&self, db: &Store, action: &str, detail: &str) -> Result<()> {
        db.audit(Some(self.actor()), Some(&self.dsp.id), action, detail)
    }
}
impl Store {
    /// `user` of the platform's directory while they are an active platform owner, as whom
    /// an outside agent acts: `None` once they are not.
    pub fn active_platform_owner(&self, user: &str) -> Result<Option<PublicUser>> {
        Ok(UserRow::find(&self.platform, "id", user)?
            .filter(|row| row.active() && row.user.platform_owner)
            .map(|row| row.user))
    }
    /// The name of `user` of the platform's directory, whatever their standing: `None` once
    /// their account is gone.
    pub fn platform_user_name(&self, user: &str) -> Result<Option<String>> {
        Ok(UserRow::find(&self.platform, "id", user)?.map(|row| row.user.name()))
    }
    /// Who did something in `dsp`, as it sees them: a platform owner is always Platform
    /// support. `None` once their account is gone.
    pub fn actor_name(&self, dsp: &str, user: &str) -> Result<Option<String>> {
        if self.platform_owner(user)? {
            return Ok(Some("Platform support".to_owned()));
        }
        Ok(self
            .dsp(dsp)?
            .query_as::<(String,)>(
                "SELECT first_name||' '||last_name FROM users WHERE id=?",
                [user],
            )?
            .into_iter()
            .next()
            .map(|(name,)| name))
    }
    /// A new account: a platform owner's in the platform's directory, or with `dsp`, one of
    /// that DSP's people in its own, which a membership then admits.
    pub fn create_user(
        &self,
        dsp: Option<&str>,
        email: &str,
        first: &str,
        last: &str,
        password: &str,
    ) -> Result<PublicUser> {
        let value = json!({"email":email,"firstName":first,"lastName":last});
        let email = v::email(&value, "email")?;
        let first = v::name(&value, "firstName", 100)?;
        let last = v::name(&value, "lastName", 100)?;
        let encoded = crypto::hash_password(password)?;
        let directory = self.directory(dsp)?;
        ensure(
            directory
                .one("SELECT id FROM users WHERE email=?", [&email])?
                .is_none(),
            "email_already_registered",
            409,
        )?;
        let id = crypto::id("usr")?;
        let owner = dsp.is_none();
        directory.exec(
            "INSERT INTO users(id,email,first_name,last_name,password,platform_owner,created_at) \
             VALUES (?,?,?,?,?,?,?)",
            params![id, email, first, last, encoded, owner, iso()],
        )?;
        Ok(PublicUser {
            id,
            email,
            first_name: first,
            last_name: last,
            platform_owner: owner,
        })
    }
    /// Queues `mail` to `to`, for what `context` names: in `dsp`'s directory when it is one
    /// of its people's.
    fn queue_mail(
        &self,
        dsp: Option<&str>,
        to: &str,
        mail: &email::Message,
        context: MailContext,
    ) -> Result<()> {
        ensure(self.config.mail_available(), "email_unavailable", 503)?;
        let (text, html) = (&mail.text, Some(&mail.html));
        let subject = if self.config.env().is_preview() {
            format!("[Dispatch Dev] {}", mail.subject)
        } else {
            mail.subject.clone()
        };
        let id = crypto::id("mail")?;
        let encrypted = crypto::encrypt(
            &self.key,
            &id,
            &json!({"to":to,"subject":subject,"text":text,"html":html,"environment":self.config.environment,"origin":self.config.origin}),
        )?;
        let (kind, invitation, user) = match context {
            MailContext::Invitation { hash } => ("invitation", Some(hash), None),
            MailContext::Reset { user } => ("reset", None, Some(user)),
            MailContext::PlatformNotice { kind, user } => (kind, None, Some(user)),
            MailContext::Feature { kind, user } => (kind, None, Some(user)),
        };
        // Invitation traffic has its own ceiling; recovery keeps reserved capacity.
        ensure(
            self.platform.count(
                "SELECT count(*) FROM outbox WHERE status='pending' AND kind=?",
                [kind],
            )? < self.config.security.mail_kind_pending,
            "email_queue_full",
            429,
        )?;
        self.platform.exec(
            "INSERT INTO outbox(id,encrypted_message,available_at,created_at,kind,\
             invitation_hash,user_id,dsp_id) VALUES (?,?,?3,?3,?,?,?,?)",
            params![id, encrypted, now(), kind, invitation, user, dsp],
        )?;
        self.mail_queued();
        Ok(())
    }

    /// Emails a member what a feature wrote them, of its own `kind`, named
    /// `<feature>.<what>` such as `documents.google_account`. It is queued as every email
    /// is: retried, listed in the platform owner's mail log and caught in a preview. It goes
    /// unsent once the member's account is gone.
    pub fn email_member(
        &self,
        dsp: &str,
        user: &str,
        kind: &str,
        mail: &email::Message,
    ) -> Result<()> {
        let named = kind.split_once('.').is_some_and(|(feature, what)| {
            [feature, what].iter().all(|part| {
                !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
            })
        });
        ensure(named, "invalid_mail_kind", 500)?;
        let to = self
            .dsp(dsp)?
            .one(
                "SELECT email FROM users WHERE id=? AND status='active'",
                [user],
            )?
            .map(|row| s(&row, "email").to_owned())
            .ok_or_else(|| Error::new("user_not_found", 404))?;
        self.queue_mail(Some(dsp), &to, mail, MailContext::Feature { kind, user })
    }

    /// Emails every active platform owner a notice of `kind`, a word without a dot,
    /// `message` written for each address, when email is on. What it reports has already
    /// happened, so a notice that cannot be queued is noted in the log and never fails the
    /// caller. One still waiting is sent only while its owner is an active platform owner.
    pub fn notify_platform_owners(&self, kind: &str, message: impl Fn(&str) -> email::Message) {
        if !self.config.mail_available() {
            return;
        }
        let skipped = |error: Error| {
            crate::foundation::observability::event(
                "warn",
                "mail.notice_skipped",
                json!({"kind":kind,"error":error.code}),
            );
        };
        let owners = match self.platform.query_as::<(String, String)>(
            "SELECT id,email FROM users WHERE platform_owner=1 AND status='active' ORDER BY email",
            [],
        ) {
            Ok(owners) => owners,
            Err(error) => return skipped(error),
        };
        for (user, to) in owners {
            let mail = message(&to);
            let context = MailContext::PlatformNotice { kind, user: &user };
            if let Err(error) = self.queue_mail(None, &to, &mail, context) {
                skipped(error);
            }
        }
    }
}
