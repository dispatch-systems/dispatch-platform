//! Who is signed in, and which DSP they are looking at.
use crate::{
    Result,
    accounts::api::types::{DspView, RoleSummary, SessionResponse, SidebarArrangement},
    db::Store,
    ensure,
    foundation::{
        config::{ProviderMode, Site},
        validate as v,
    },
    server::http::{
        input::{Input, Reply},
        route::{Anyone, Dsp, Member, Public, Route, Session, User, read, write},
    },
    tenancy::{
        api::types::{SiteDsp, SiteInfo, SiteKind},
        roles,
    },
};

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/site", Public, site),
        read("/api/session", Session, session),
        write("/api/session/dsp", Session, open_dsp),
        // A member's own arrangement, which no cached read holds.
        write("/api/dsp/sidebar", Dsp(roles::ACCESS), arrange_sidebar).bookkeeping(),
    ]
}

// Which address the dashboard was loaded from, before anyone signs in there.
fn site(db: &Store, _: &Anyone, input: &Input) -> Result<Reply> {
    let (kind, dsp) = match &input.site {
        Site::Admin => (SiteKind::Admin, None),
        Site::Invite => (SiteKind::Invite, None),
        Site::Dsp(code) => (
            SiteKind::Dsp,
            db.dsp_at(code)?.map(|dsp| SiteDsp {
                id: dsp.id,
                code: code.clone(),
            }),
        ),
    };
    Reply::of(&SiteInfo {
        kind,
        dsp,
        dsp_address: db.config.dsp_origin.clone(),
    })
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
        platform_frontend: (user.user.platform_owner
            && (!security.required || security.verified)
            && crate::manifest::registry()
                .features
                .iter()
                .any(|feature| feature.owner.is_some()))
        .then(|| crate::server::http::custom_frontend(user.state, "platform"))
        .flatten()
        .map(|(url, _)| url),
        development: db.config.development,
        environment: db.config.environment,
        release: db.config.release.clone(),
        provider_mode: if db.config.fixture {
            ProviderMode::Fixture
        } else {
            ProviderMode::Native
        },
        source: db.config.source.clone(),
        security,
        read_only: crate::accounts::browser::read_only(user),
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
    if !crate::accounts::browser::read_only(&a) {
        db.audit_visit(a.user.id.as_str(), c.dsp.id.as_str(), action, previewed)?;
    }
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
        custom: crate::tenancy::custom::view(db, user.state, &c)?,
        token: db.view_token(&c),
        profile: db.profile(&c.dsp.id)?,
        permissions: c.visible(&held).cloned().collect(),
        features: c.features.clone(),
        role: RoleSummary {
            id: c.role,
            name: c.role_name,
            owner: c.owner,
        },
        sidebar: db.sidebar(&c.dsp.id, a.user.id.as_str())?,
        dsp: c.dsp,
        roles,
    })
}

/// The most pages an arrangement names, far past what a DSP has.
const ARRANGED: usize = 200;

// Keeps how the member arranged the sidebar of the DSP their view is of: theirs alone, so it
// expires no view and is not audited. The pages are the dashboard's to know, so any id is kept.
fn arrange_sidebar(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["order", "hidden"])?;
    let to: SidebarArrangement = serde_json::from_value(input.body.clone())
        .map_err(|_| crate::Error::new("invalid_input", 400))?;
    v::ids(&to.order, ARRANGED)?;
    v::ids(&to.hidden, ARRANGED)?;
    db.arrange_sidebar(c.dsp_id(), c.actor(), &to)?;
    Reply::of(&to)
}

// Authorization still runs on every request; keys cannot share membership-specific listings.
pub(crate) fn summaries(
    db: &Store,
    user: &User,
) -> Result<Vec<crate::accounts::api::types::DspSummary>> {
    user.state.read_cache.read(
        crate::server::cache::Scope::listings(),
        format!(
            "dsps:{}:{}:{}",
            user.actor(),
            user.user.platform_owner,
            user.scope.as_deref().unwrap_or("")
        ),
        user.state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed),
        || db.dsps(user),
    )
}
