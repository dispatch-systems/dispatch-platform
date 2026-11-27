//! The session a provider's sign-in left, kept beside the DSP's browser profile so the next
//! job can read with it and start no browser (`Collector::resumed`): the cookies on the
//! collector's cookie domains and the browser's user agent, as `SignedIn` holds them. It is
//! encrypted with the DSP's vault key, as its credentials are, and bound to the connection's
//! revision: saving credentials again or disconnecting moves the revision on and removes it
//! with the profile (`clear_collector_browser_state`), and one written under another revision
//! no longer opens. It is never logged.
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
    /// The session saved for the DSP's connection to `provider` at `revision`: none when there
    /// is none, and none, its file removed, when it was saved under another revision or can't
    /// be read.
    pub(crate) fn saved_session(
        &self,
        dsp: &str,
        provider: Provider,
        revision: i64,
    ) -> Result<Option<SignedIn>> {
        let path = self.saved_session_path(dsp, provider)?;
        db::private_file(&path, false)?;
        if !path.try_exists()? {
            return Ok(None);
        }
        let opened = std::fs::read_to_string(&path)
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
            .filter(|saved| saved.revision == revision);
        match opened {
            Some(saved) => Ok(Some(saved.session)),
            None => {
                self.discard_session(dsp, provider)?;
                Ok(None)
            }
        }
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
        let sealed = crypto::encrypt(
            &self.vault_key(dsp)?,
            &binding(dsp, provider, revision),
            &serde_json::to_value(Saved {
                revision,
                session: session.clone(),
            })?,
        )?;
        db::private_dir(&self.area(dsp, "state")?.join("browsers"))?;
        db::write_private(&self.saved_session_path(dsp, provider)?, sealed.as_bytes())
    }
    /// Removes the session saved for the DSP's connection to `provider`, as when the provider
    /// refused it.
    pub(crate) fn discard_session(&self, dsp: &str, provider: Provider) -> Result<()> {
        let path = self.saved_session_path(dsp, provider)?;
        db::private_file(&path, false)?;
        match std::fs::remove_file(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }
}
