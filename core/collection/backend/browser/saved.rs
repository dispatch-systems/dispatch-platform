//! The session a provider's sign-in left, kept beside the DSP's browser profile so the next
//! job can read with it and start no browser (`Collector::resumed`): the cookies on the
//! collector's cookie domains, each until it expires, and the browser's user agent, as
//! `SignedIn` holds them. It is encrypted with the DSP's vault key, as its credentials are, and
//! bound to the connection's revision: saving credentials again or disconnecting moves the
//! revision on and removes it with the profile (`clear_collector_browser_state`), and one
//! written under another revision no longer opens. It is never logged.
use super::{Provider, http::SignedIn};
use crate::{Result, db, db::Store, foundation::crypto};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Saved {
    revision: i64,
    session: SignedIn,
}
fn binding(dsp: &str, provider: Provider, revision: i64) -> String {
    format!("{dsp}:{}:session:{revision}", provider.id())
}
impl Store {
    /// Its file, beside the profile.
    pub(crate) fn saved_session_path(&self, dsp: &str, provider: Provider) -> Result<PathBuf> {
        Ok(self
            .area(dsp, "state")?
            .join("browsers")
            .join(format!("{}-session.enc", provider.id())))
    }
    fn vault_key(&self, dsp: &str) -> Result<Vec<u8>> {
        db::key_file(&self.area(dsp, "secrets")?.join("vault.key"))
    }
    /// The session saved for the DSP's connection to `provider` at `revision`, its expired
    /// cookies dropped: none when there is none, and none, its file removed, when it was saved
    /// under another revision or can't be read.
    pub(crate) fn saved_session(
        &self,
        dsp: &str,
        provider: Provider,
        revision: i64,
    ) -> Result<Option<SignedIn>> {
        match self.opened(dsp, provider, revision)? {
            Some(saved) => Ok(Some(saved.session.unexpired())),
            None => {
                self.discard_session(dsp, provider)?;
                Ok(None)
            }
        }
    }
    /// What the session's file holds when it opens at `revision`: none when there is none, or it
    /// was saved under another revision or can't be read.
    fn opened(&self, dsp: &str, provider: Provider, revision: i64) -> Result<Option<Saved>> {
        let path = self.saved_session_path(dsp, provider)?;
        db::private_file(&path, false)?;
        Ok(std::fs::read_to_string(&path)
            .ok()
            .and_then(|sealed| {
                crypto::decrypt(
                    &self.vault_key(dsp).ok()?,
                    &binding(dsp, provider, revision),
                    &sealed,
                )
                .ok()
            })
            .and_then(|value| serde_json::from_value::<Saved>(value).ok())
            .filter(|saved| saved.revision == revision))
    }
    /// Saves `session` for the DSP's connection to `provider` at `revision`, while that is
    /// still the connection's enabled revision.
    pub(crate) fn save_session(
        &self,
        dsp: &str,
        provider: Provider,
        revision: i64,
        session: &SignedIn,
    ) -> Result<()> {
        if self.connection_revision(dsp, provider)? != Some(revision) {
            return Ok(());
        }
        self.seal(dsp, provider, revision, session)
    }
    /// Writes `session` as saved at `revision`, but for its expired cookies.
    fn seal(&self, dsp: &str, provider: Provider, revision: i64, session: &SignedIn) -> Result<()> {
        let sealed = crypto::encrypt(
            &self.vault_key(dsp)?,
            &binding(dsp, provider, revision),
            &serde_json::to_value(Saved {
                revision,
                session: session.clone().unexpired(),
            })?,
        )?;
        db::private_dir(&self.area(dsp, "state")?.join("browsers"))?;
        db::write_private(&self.saved_session_path(dsp, provider)?, sealed.as_bytes())
    }
    /// Removes the session saved for the DSP's connection to `provider` at `revision`, as when
    /// the provider refused it: one saved since under another revision stays.
    pub(crate) fn discard_session_at(
        &self,
        dsp: &str,
        provider: Provider,
        revision: i64,
    ) -> Result<()> {
        match self.opened(dsp, provider, revision)? {
            Some(_) => self.discard_session(dsp, provider),
            None => Ok(()),
        }
    }
    /// Removes the session saved for the DSP's connection to `provider`, whatever its revision.
    pub(crate) fn discard_session(&self, dsp: &str, provider: Provider) -> Result<()> {
        let path = self.saved_session_path(dsp, provider)?;
        db::private_file(&path, false)?;
        match std::fs::remove_file(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/backend/browser/saved.rs"]
mod tests;
