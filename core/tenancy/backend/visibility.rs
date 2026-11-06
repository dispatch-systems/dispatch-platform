//! Per-DSP visibility and beta permissions, independent of whether features run.
use super::catalog::{Kind, catalog, find};
use crate::{
    Result,
    db::{Store, iso},
    ensure,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};

// Its wire export stays at the platform-owner API's established path.
/// Who among a DSP's people sees one of its features while it is on: all of them, as their
/// roles allow; only its beta testers, who hold all of it; or none, as the platform owner alone
/// still does. Whatever it runs keeps running either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Shown,
    Beta,
    Hidden,
}
/// Optional features hidden from members or restricted to beta testers, in catalog order.
#[derive(Debug)]
pub struct FeatureVisibility {
    pub hidden: Vec<String>,
    pub beta: Vec<String>,
}

/// A feature's visibility as `dsp_features` stores it: one in beta is stored hidden as well.
pub(crate) fn visibility(shown: bool, beta: bool) -> Visibility {
    match (shown, beta) {
        (true, _) => Visibility::Shown,
        (false, true) => Visibility::Beta,
        (false, false) => Visibility::Hidden,
    }
}
impl Store {
    /// What one of the DSP's people sees of its features: what it has, less what the platform
    /// owner hides from its members and, unless they are one of its beta testers, what is in
    /// beta there; a page's parts go with it. Either keeps running.
    pub fn shown_features(&self, dsp: &str, tester: bool) -> Result<Vec<String>> {
        let unshown = self.unshown_features(dsp)?;
        let seen = |id: &str| match unshown.iter().find(|(u, _)| u == id) {
            None => true,
            Some((_, visibility)) => tester && *visibility == Visibility::Beta,
        };
        Ok(self
            .features(dsp)?
            .into_iter()
            .filter(|id| {
                seen(id)
                    && match find(id).map(|f| f.kind) {
                        Some(Kind::Sub(page)) => seen(page),
                        _ => true,
                    }
            })
            .collect())
    }
    /// What a beta tester holds besides their role's: every permission of what they see that
    /// is in beta, a page's parts' with the page's, and what each of those includes.
    pub fn beta_permissions(&self, dsp: &str, seen: &[String]) -> Result<Vec<String>> {
        let beta: Vec<String> = self
            .unshown_features(dsp)?
            .into_iter()
            .filter(|(_, visibility)| *visibility == Visibility::Beta)
            .map(|(id, _)| id)
            .collect();
        let in_beta = |id: &str| beta.iter().any(|b| b == id);
        let held: Vec<String> = seen
            .iter()
            .filter_map(|id| find(id))
            .filter(|f| {
                in_beta(f.id)
                    || match f.kind {
                        Kind::Sub(page) => in_beta(page),
                        _ => false,
                    }
            })
            .flat_map(|f| f.permissions.iter().map(|p| (*p).to_owned()))
            .collect();
        Ok(super::roles::granting(&held))
    }
    /// The optional features and parts the DSP's members don't all see, each hidden from them
    /// or in beta there, in catalog order.
    fn unshown_features(&self, dsp: &str) -> Result<Vec<(String, Visibility)>> {
        let rows: Vec<(String, bool)> = self.platform.query_as(
            "SELECT feature,beta FROM dsp_features WHERE dsp_id=? AND shown=0",
            [dsp],
        )?;
        Ok(catalog()
            .iter()
            .filter(|f| !f.mandatory && f.kind != Kind::Connection)
            .filter_map(|f| {
                rows.iter()
                    .find(|(id, _)| id == f.id)
                    .map(|(_, beta)| (f.id.to_owned(), visibility(false, *beta)))
            })
            .collect())
    }
    /// What the DSP's members don't all see, by who sees it.
    fn dsp_visibility(&self, dsp: &str) -> Result<FeatureVisibility> {
        let unshown = self.unshown_features(dsp)?;
        let those = |wanted: Visibility| {
            unshown
                .iter()
                .filter(|(_, visibility)| *visibility == wanted)
                .map(|(id, _)| id.clone())
                .collect()
        };
        Ok(FeatureVisibility {
            hidden: those(Visibility::Hidden),
            beta: those(Visibility::Beta),
        })
    }
    /// Shows an optional feature or part to the DSP's members, puts it in beta there for its
    /// beta testers alone, or hides it from them all. Whatever it runs keeps running, and
    /// whether it is on stays as it is; the platform owner's own view still sees it. Audited;
    /// open views expire.
    pub fn set_visibility(
        &self,
        dsp: &str,
        id: &str,
        visibility: Visibility,
        actor: &str,
    ) -> Result<FeatureVisibility> {
        let feature = find(id).ok_or_else(|| crate::Error::new("feature_not_found", 404))?;
        ensure(!feature.mandatory, "feature_mandatory", 409)?;
        ensure(feature.kind != Kind::Connection, "invalid_input", 400)?;
        self.platform.transaction(|| {
            self.find_dsp(dsp)?;
            let was = self
                .unshown_features(dsp)?
                .into_iter()
                .find(|(u, _)| u == id)
                .map_or(Visibility::Shown, |(_, visibility)| visibility);
            if was != visibility {
                let on = self.switches(dsp)?.iter().any(|e| e == id);
                self.platform.exec(
                    "INSERT INTO dsp_features(dsp_id,feature,enabled,shown,beta,changed_by,changed_at) \
                     VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(dsp_id,feature) DO UPDATE SET \
                     shown=?4,beta=?5,changed_by=?6,changed_at=?7",
                    params![
                        dsp,
                        id,
                        on,
                        visibility == Visibility::Shown,
                        visibility == Visibility::Beta,
                        actor,
                        iso()
                    ],
                )?;
                self.audit_with(
                    Some(actor),
                    Some(dsp),
                    match visibility {
                        Visibility::Shown => "dsp.feature_shown",
                        Visibility::Beta => "dsp.feature_beta",
                        Visibility::Hidden => "dsp.feature_hidden",
                    },
                    id,
                    Some(&feature.name()),
                    &[],
                )?;
                // Open views sign the DSP revision, so members lose or regain it at once.
                self.platform
                    .exec("UPDATE dsps SET revision=revision+1 WHERE id=?", [dsp])?;
            }
            self.dsp_visibility(dsp)
        })
    }
}
