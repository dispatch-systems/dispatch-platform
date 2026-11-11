//! Feature diagnostics for the platform owner's DSP page.
use crate::{
    Result,
    db::{FromRow, Row, Store},
    job_statuses,
    platform_owner::api::types::{DspFeatureReport, FeatureState, Visibility},
    tenancy::{
        api::types::DspStatus,
        catalog::{Kind, catalog},
        visibility::visibility,
    },
};

impl FromRow for FeatureState {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            feature: row.get("feature")?,
            enabled: row.get("enabled")?,
            visibility: visibility(row.get("shown")?, row.get("beta")?),
            changed_at: row.get("changed_at")?,
            changed_by: row.get("changed_by")?,
        })
    }
}
impl Store {
    /// Every feature of the catalog as the DSP has it, with its schedules switched on and its
    /// jobs under way, for the platform's DSP page.
    pub fn feature_report(&self, dsp: &str) -> Result<DspFeatureReport> {
        let row = self.find_dsp(dsp)?;
        let stored: Vec<FeatureState> = self.platform.query_as(
            "SELECT f.feature,f.enabled,f.shown,f.beta,f.changed_at,u.first_name||' '||u.last_name changed_by \
             FROM dsp_features f LEFT JOIN users u ON u.id=f.changed_by WHERE f.dsp_id=?",
            [dsp],
        )?;
        let features = catalog()
            .iter()
            .filter(|feature| feature.owner.is_none() || feature.owner == row.code.as_deref())
            .map(|f| {
                stored
                    .iter()
                    .find(|s| s.feature == f.id)
                    .cloned()
                    .map(|state| FeatureState {
                        enabled: state.enabled || f.mandatory,
                        visibility: if f.mandatory || f.kind == Kind::Connection {
                            Visibility::Shown
                        } else {
                            state.visibility
                        },
                        ..state
                    })
                    .unwrap_or(FeatureState {
                        feature: f.id.to_owned(),
                        enabled: f.default,
                        visibility: Visibility::Shown,
                        changed_at: None,
                        changed_by: None,
                    })
            })
            .collect();
        let provisioned = [DspStatus::Active, DspStatus::Suspended].contains(&row.status);
        let custom = crate::tenancy::custom::catalog(row.code.as_deref());
        Ok(DspFeatureReport {
            features,
            custom: (!custom.is_empty()).then_some(custom),
            schedules: if provisioned {
                self.dsp(dsp)?.count(
                    "SELECT count(*) FROM collection_schedules WHERE enabled=1",
                    [],
                )?
            } else {
                0
            },
            active_jobs: self.jobs.count(
                concat!(
                    "SELECT count(*) FROM jobs WHERE dsp_id=? AND status IN ",
                    job_statuses!(active)
                ),
                [dsp],
            )?,
        })
    }
}
