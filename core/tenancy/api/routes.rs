//! Who is signed in, and which DSP they are looking at.
use crate::{
    Result,
    accounts::api::types::{DspView, RoleSummary, SessionResponse},
    db::Store,
    ensure,
    foundation::{config::ProviderMode, validate as v},
    server::http::{
        input::{Input, Reply},
        route::{Route, Session, User, read, write},
    },
    tenancy::roles,
};

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/session", Session, session),
        write("/api/session/dsp", Session, open_dsp),
    ]
}

fn session(db: &Store, user: &User, _: &Input) -> Result<Reply> {
    let security = db.security_status(user)?;
    Reply::of(&SessionResponse {
        user: user.user.clone(),
        csrf: user.csrf.clone(),
        dsps: if security.required && !security.verified {
            Vec::new()
        } else {
            summaries(db, user)?
        },
        development: db.config.development,
        environment: db.config.env(),
        release: db.config.release.clone(),
        provider_mode: if db.config.fixture {
            ProviderMode::Fixture
        } else {
            ProviderMode::Native
        },
        source: db.config.source.clone(),
        security,
    })
}

// Answers with the signed view token every DSP route then expects as a header.
fn open_dsp(db: &Store, user: &User, input: &Input) -> Result<Reply> {
    let b = &input.body;
    let mut a = (**user).clone();
    v::fields(b, &["dspId", "roleId"])?;
    let platform = a.user.platform_owner;
    // Only a platform owner may look through a role other than their own.
    if !b["roleId"].is_null() {
        ensure(platform, "permission_denied", 403)?;
        a.preview = Some(v::text(b, "roleId", 1, 100)?.to_owned());
    }
    let c = db.context(&a, v::text(b, "dspId", 1, 100)?, roles::ACCESS)?;
    let action = if platform {
        "dsp.owner_view_opened"
    } else {
        "dsp.view_opened"
    };
    let previewed = if a.preview.is_some() {
        &c.role_name
    } else {
        ""
    };
    db.audit_visit(a.user.id.as_str(), c.dsp.id.as_str(), action, previewed)?;
    let summary = |role: crate::accounts::api::types::Role| RoleSummary {
        id: role.id,
        name: role.name,
        owner: role.owner,
    };
    let roles = if platform {
        Some(db.roles(&c.dsp.id)?.into_iter().map(summary).collect())
    } else {
        None
    };
    let held = if c.owner {
        roles::all()
    } else {
        c.permissions.clone()
    };
    Reply::of(&DspView {
        token: db.view_token(&c),
        profile: db.profile(&c.dsp.id)?,
        permissions: c.visible(&held).cloned().collect(),
        features: c.features.clone(),
        role: RoleSummary {
            id: c.role,
            name: c.role_name,
            owner: c.owner,
        },
        dsp: c.dsp,
        roles,
    })
}

// Authorization still runs on every request; keys cannot share membership-specific listings.
pub(crate) fn summaries(
    db: &Store,
    user: &User,
) -> Result<Vec<crate::accounts::api::types::DspSummary>> {
    user.state.read_cache.read(
        crate::server::cache::Scope::listings(),
        format!("dsps:{}:{}", user.actor(), user.user.platform_owner),
        user.state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed),
        || db.dsps(user),
    )
}
