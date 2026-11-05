//! The browsers members are told of new messages in (`communications_push`), each by where a push
//! to it goes, its endpoint.
use dispatch_core::{
    Result,
    db::{Store, now},
};

/// A browser a member is told in, as its push service subscribed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subscription {
    pub endpoint: String,
    pub user: String,
    /// The browser's public key, its 65 bytes uncompressed, and its secret, 16 bytes, each in
    /// base64url: what its pushes are encrypted for.
    pub p256dh: String,
    pub auth: String,
}

/// What Communications keeps of the browsers its members are told in.
pub trait PushStore {
    /// Keeps the browser `subscription` names as its member's, in place of whoever's it was.
    fn keep_communications_push(&self, dsp: &str, subscription: &Subscription) -> Result<()>;
    /// Lets go of the browser at `endpoint`, if it's `user`'s.
    fn remove_communications_push(&self, dsp: &str, user: &str, endpoint: &str) -> Result<()>;
    /// Every browser the DSP's members are told in, each member's together.
    fn communications_pushes(&self, dsp: &str) -> Result<Vec<Subscription>>;
    /// Lets go of the browsers at `endpoints`, whoever's they are.
    fn drop_communications_pushes(&self, dsp: &str, endpoints: &[String]) -> Result<()>;
    /// Whether any member of the DSP is told in a browser.
    fn communications_pushing(&self, dsp: &str) -> Result<bool>;
}

impl PushStore for Store {
    fn keep_communications_push(&self, dsp: &str, subscription: &Subscription) -> Result<()> {
        let Subscription {
            endpoint,
            user,
            p256dh,
            auth,
        } = subscription;
        self.dsp(dsp)?.exec(
            "INSERT INTO communications_push (endpoint,user_id,p256dh,auth,created_at) \
             VALUES (?,?,?,?,?) ON CONFLICT (endpoint) DO UPDATE SET user_id=excluded.user_id,\
             p256dh=excluded.p256dh,auth=excluded.auth,created_at=excluded.created_at",
            rusqlite::params![endpoint, user, p256dh, auth, now()],
        )?;
        Ok(())
    }
    fn remove_communications_push(&self, dsp: &str, user: &str, endpoint: &str) -> Result<()> {
        self.dsp(dsp)?.exec(
            "DELETE FROM communications_push WHERE endpoint=? AND user_id=?",
            [endpoint, user],
        )?;
        Ok(())
    }
    fn communications_pushes(&self, dsp: &str) -> Result<Vec<Subscription>> {
        let rows: Vec<(String, String, String, String)> = self.dsp(dsp)?.query_as(
            "SELECT endpoint,user_id,p256dh,auth FROM communications_push ORDER BY user_id,endpoint",
            [],
        )?;
        Ok(rows
            .into_iter()
            .map(|(endpoint, user, p256dh, auth)| Subscription {
                endpoint,
                user,
                p256dh,
                auth,
            })
            .collect())
    }
    fn drop_communications_pushes(&self, dsp: &str, endpoints: &[String]) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            for endpoint in endpoints {
                db.exec(
                    "DELETE FROM communications_push WHERE endpoint=?",
                    [endpoint],
                )?;
            }
            Ok(())
        })
    }
    fn communications_pushing(&self, dsp: &str) -> Result<bool> {
        Ok(self
            .dsp(dsp)?
            .count("SELECT count(*) FROM communications_push", [])?
            > 0)
    }
}
