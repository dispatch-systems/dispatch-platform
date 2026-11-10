//! An operation's captured local replacement-grant identity. This is not OAuth material or
//! authority: every admission and publication still checks the current DSP and connection.
use super::{
    mirror::{self, Mirror},
    storage::{Connection, DocumentsStore},
};
use dispatch_core::{Error, Result, State, db::Store, ensure};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone)]
pub(super) struct Admission {
    state: Arc<State>,
    owner: Grant,
}
impl Admission {
    pub fn new(state: &Arc<State>, owner: &Grant) -> Self {
        Self {
            state: Arc::clone(state),
            owner: owner.clone(),
        }
    }
    pub async fn check(&self) -> Result<()> {
        self.owner.read(&self.state, |_, _| Ok(())).await
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct Key {
    pub namespace: PathBuf,
    pub dsp: String,
    pub generation: String,
}
#[derive(Clone)]
pub(super) struct Grant {
    pub key: Key,
    pub connection: Connection,
    pub mirror: Arc<Mirror>,
    member: Option<(
        dispatch_core::accounts::Context,
        dispatch_core::server::http::route::Dsp,
    )>,
}
impl Grant {
    /// Called only inside a State read/write turn; no lazy storage writes in a shared read.
    pub fn capture(db: &Store, dsp: &str, expected: Option<&str>) -> Result<Self> {
        db.ensure_dsp_active(dsp)?;
        ensure(
            db.feature_enabled(dsp, "documents")?,
            "feature_disabled",
            403,
        )?;
        let connection = db
            .documents_connection(dsp)?
            .ok_or_else(|| Error::new("documents_not_connected", 409))?;
        ensure(!connection.broken, "google_connection_broken", 409)?;
        ensure(
            expected.is_none_or(|value| value == connection.generation),
            "documents_not_connected",
            409,
        )?;
        ensure(
            !connection.generation.is_empty(),
            "documents_not_connected",
            409,
        )?;
        let key = Key {
            namespace: db.config.environment_root(),
            dsp: dsp.into(),
            generation: connection.generation.clone(),
        };
        let mirror = mirror::of(&key);
        Ok(Self {
            key,
            connection,
            mirror,
            member: None,
        })
    }
    pub fn check(&self, db: &Store) -> Result<()> {
        if let Some((c, access)) = &self.member {
            access.revalidate(db, c)?;
        }
        ensure(
            db.config.environment_root() == self.key.namespace,
            "documents_not_connected",
            409,
        )?;
        db.ensure_dsp_active(&self.key.dsp)?;
        ensure(
            db.feature_enabled(&self.key.dsp, "documents")?,
            "feature_disabled",
            403,
        )?;
        let current = db
            .documents_connection(&self.key.dsp)?
            .ok_or_else(|| Error::new("documents_not_connected", 409))?;
        ensure(!current.broken, "google_connection_broken", 409)?;
        ensure(
            current == self.connection && mirror::current(&self.key, &self.mirror),
            "documents_not_connected",
            409,
        )
    }
    /// Already completed sharing bookkeeping is kept through suspension, as before, but
    /// never under a replacement/deleted/broken generation. No provider work is admitted here.
    pub async fn run_finished<T: Send + 'static>(
        &self,
        state: &Arc<State>,
        f: impl FnOnce(&Store, &Grant) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let owner = self.clone();
        state
            .run(move |db| {
                ensure(
                    db.config.environment_root() == owner.key.namespace
                        && db.documents_connection(&owner.key.dsp)?.as_ref()
                            == Some(&owner.connection)
                        && mirror::current(&owner.key, &owner.mirror),
                    "documents_not_connected",
                    409,
                )?;
                f(db, &owner)
            })
            .await
    }
    /// A short shared transition protects publication from reconnect/disconnect/deletion.
    /// Never await provider work, refresh turns, or renewal turns inside this closure.
    pub async fn read<T: Send + 'static>(
        &self,
        state: &Arc<State>,
        f: impl FnOnce(&Store, &Grant) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let owner = self.clone();
        state
            .read(move |db| {
                owner.check(db)?;
                f(db, &owner)
            })
            .await
    }
    /// Actual provenance, sharing and audit changes retain the exclusive State transition.
    pub async fn run<T: Send + 'static>(
        &self,
        state: &Arc<State>,
        f: impl FnOnce(&Store, &Grant) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let owner = self.clone();
        state
            .run(move |db| {
                owner.check(db)?;
                f(db, &owner)
            })
            .await
    }
}
pub(super) async fn capture_member(
    state: &Arc<State>,
    c: dispatch_core::accounts::Context,
    access: dispatch_core::server::http::route::Dsp,
    expected: Option<String>,
) -> Result<Grant> {
    state
        .read(move |db| {
            let c = access.revalidate(db, &c)?;
            let mut owner = Grant::capture(db, &c.dsp.id, expected.as_deref())?;
            owner.member = Some((c, access));
            Ok(owner)
        })
        .await
}
pub(super) async fn capture(
    state: &Arc<State>,
    dsp: &str,
    expected: Option<String>,
) -> Result<Grant> {
    let dsp = dsp.to_owned();
    state
        .read(move |db| Grant::capture(db, &dsp, expected.as_deref()))
        .await
}
