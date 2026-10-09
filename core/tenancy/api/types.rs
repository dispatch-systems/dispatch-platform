use crate::text_enum;
use serde::Serialize;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/tenancy/api/generated/"))]
        pub enum DspStatus {
        Provisioning => "provisioning",
        Active => "active",
        Suspended => "suspended",
        Failed => "failed",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/tenancy/api/generated/"))]
        pub enum OwnerStatus {
        Active => "active",
        Invited => "invited",
        Missing => "missing",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/tenancy/api/generated/"))]
        pub enum SiteKind {
        Admin => "admin",
        Invite => "invite",
        Dsp => "dsp",
    }
}
/// Which of the server's addresses a page was loaded from, which the dashboard reads before
/// anyone signs in: the platform owner's admin, the invite page, or a DSP's own.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/tenancy/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SiteInfo {
    pub kind: SiteKind,
    /// At a DSP's address, the DSP whose short code names it: none when no DSP has it.
    pub dsp: Option<SiteDsp>,
    /// Every DSP's address, with `{code}` where its short code goes.
    pub dsp_address: String,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/tenancy/api/generated/"))]
pub struct SiteDsp {
    pub id: String,
    pub code: String,
}
